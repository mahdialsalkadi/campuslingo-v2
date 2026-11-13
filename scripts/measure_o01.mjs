#!/usr/bin/env node
// O-01 whole-stack resident measurement for the current M2 stack. Run it through
// scripts/run_ingestion_contained.sh so PostgreSQL, the Rust API, the ingestion
// worker, the managed OCR model and every helper are charged to one cgroup.
//
// Scope: one synthetic PDF ingestion with one OCR page. Redis is not part of the
// current M2 stack and is reported as absent, not simulated. No private corpus
// bytes are read and no network request is made during inference.

import { spawn, spawnSync } from 'node:child_process';
import { createHash, randomBytes, randomUUID } from 'node:crypto';
import {
  chmod,
  copyFile,
  mkdir,
  open,
  readFile,
  rm,
  stat,
  writeFile,
} from 'node:fs/promises';
import { readFileSync, readdirSync } from 'node:fs';
import net from 'node:net';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const pgBin =
  process.env.CAMPUSLINGO_PG_BIN ??
  '/home/mahdi/campus-lingo/services/core-ai/.venv/lib/python3.11/site-packages/pgserver/pginstall/bin';
const serverBinary = path.join(root, 'target/debug/campuslingo-server');
const ingestionBinary = path.join(root, 'target/debug/campuslingo-ingestion');
const fixture = path.join(root, 'tests/fixtures/m2/annotated-english.pdf');
const model = path.join(
  root,
  'artifacts/private/models/paddleocr-vl-1.6/PaddleOCR-VL-1.6.Q8_0.gguf',
);
const mmproj = path.join(
  root,
  'artifacts/private/models/paddleocr-vl-1.6/PaddleOCR-VL-1.6-mmproj.gguf',
);
const threads = Number(process.env.CAMPUSLINGO_O01_THREADS ?? '8');
const safetyReserveBytes = 1536 * 1024 * 1024;
const modelHostReserveBytes = 3 * 1024 * 1024 * 1024;

const base = path.join(root, 'artifacts/private', `o01-${Date.now()}`);
const data = path.join(base, 'pgdata');
const socket = path.join(base, 'socket');
const postgresLog = path.join(base, 'postgres.log');
const serverLog = path.join(base, 'server.log');
const workerLog = path.join(base, 'worker.log');
const assetDir = path.join(base, 'assets');
const workDir = path.join(base, 'work');

function membershipPath() {
  const line = readFileSync('/proc/self/cgroup', 'utf8')
    .trim()
    .split('\n')
    .find((entry) => entry.startsWith('0::'));
  if (!line) throw new Error('missing unified cgroup membership');
  return line.slice(3);
}

// Top-level code discovers the delegated service cgroup before starting any
// child, so the peak includes PostgreSQL, the API, the worker and the model.
const membership = membershipPath();
if (!membership.endsWith('/supervisor')) {
  throw new Error('run through scripts/run_ingestion_contained.sh (missing delegated supervisor subgroup)');
}
const serviceRoot = `/sys/fs/cgroup${path.dirname(membership)}`;
const memoryPeakPath = path.join(serviceRoot, 'memory.peak');
const memoryCurrentPath = path.join(serviceRoot, 'memory.current');

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: root,
    stdio: options.stdio ?? 'inherit',
    env: { ...process.env, ...options.env },
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`${path.basename(command)} exited with ${result.status}`);
  }
  return result;
}

function psql(args, options = {}) {
  return spawnSync(path.join(pgBin, 'psql'), args, {
    cwd: root,
    encoding: 'utf8',
    env: { ...process.env, PGHOST: socket, PGPORT: String(port) },
    ...options,
  });
}

function psqlValue(sql) {
  const result = psql(['-v', 'ON_ERROR_STOP=1', '-U', 'campuslingo_owner',
    '-d', 'campuslingo_test', '-t', '-A', '-c', sql], { stdio: ['ignore', 'pipe', 'pipe'] });
  if (result.status !== 0) throw new Error(`psql query failed: ${result.stderr?.slice(0, 200)}`);
  return result.stdout.trim();
}

async function freePort() {
  return await new Promise((resolve, reject) => {
    const server = net.createServer();
    server.on('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      const chosen = typeof address === 'object' && address ? address.port : 0;
      server.close(() => resolve(chosen));
    });
  });
}

async function sha256(file) {
  return createHash('sha256').update(await readFile(file)).digest('hex');
}

async function readNumber(file) {
  try {
    return Number((await readFile(file, 'utf8')).trim());
  } catch {
    return null;
  }
}

async function availableBytes() {
  const text = await readFile('/proc/meminfo', 'utf8');
  const match = /^MemAvailable:\s+(\d+) kB$/m.exec(text);
  return match ? Number(match[1]) * 1024 : null;
}

async function drmGpuMemory() {
  const vramTotal = await readNumber('/sys/class/drm/card0/device/mem_info_vram_total');
  const vramUsed = await readNumber('/sys/class/drm/card0/device/mem_info_vram_used');
  const gttTotal = await readNumber('/sys/class/drm/card0/device/mem_info_gtt_total');
  const gttUsed = await readNumber('/sys/class/drm/card0/device/mem_info_gtt_used');
  return { vramTotal, vramUsed, gttTotal, gttUsed };
}

async function waitFor(url, deadline, predicate = (response) => response.ok) {
  let last = null;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url, { signal: AbortSignal.timeout(1000) });
      if (predicate(response)) return response;
      last = `HTTP ${response.status}`;
    } catch (error) {
      last = error.message;
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`timed out waiting for ${url}: ${last}`);
}

const startedAt = new Date();
const timings = {};
let port = null;
let postgresStarted = false;
let server = null;
let workerResult = null;
let jobId = null;
let assetId = null;
const samples = [];
let baselinePeak = null;
let minimumAvailable = await availableBytes();
const baselineGpu = await drmGpuMemory();
const sampleStarted = Date.now();

await mkdir(base, { recursive: true, mode: 0o700 });
await mkdir(socket, { recursive: true, mode: 0o700 });
await mkdir(assetDir, { recursive: true, mode: 0o700 });
await mkdir(workDir, { recursive: true, mode: 0o700 });
baselinePeak = await readNumber(memoryPeakPath);

const passwords = {
  owner: randomBytes(24).toString('hex'),
  auth: randomBytes(24).toString('hex'),
  app: randomBytes(24).toString('hex'),
  grader: randomBytes(24).toString('hex'),
  worker: randomBytes(24).toString('hex'),
  recovery: randomBytes(24).toString('hex'),
};
const url = (role, password) =>
  `postgres://${role}:${password}@127.0.0.1:${port}/campuslingo_test`;

try {
  port = await freePort();
  await stat(path.join(pgBin, 'initdb'));
  await stat(fixture);
  await stat(model);
  await stat(mmproj);
  await stat(serverBinary);
  await stat(ingestionBinary);

  const initStarted = Date.now();
  run(path.join(pgBin, 'initdb'), [
    '-D', data, '-U', 'postgres', '--auth-local=trust', '--auth-host=scram-sha-256',
    '--encoding=UTF8', '--locale=C', '--no-sync',
  ]);
  run(path.join(pgBin, 'pg_ctl'), [
    '-D', data, '-l', postgresLog, '-o',
    `-p ${port} -k ${socket} -c listen_addresses=127.0.0.1 -c unix_socket_permissions=0700 -c max_connections=100`,
    '-w', 'start',
  ]);
  postgresStarted = true;
  timings.postgres_start_ms = Date.now() - initStarted;

  const roles = `
SET password_encryption = 'scram-sha-256';
CREATE ROLE campuslingo_owner LOGIN PASSWORD '${passwords.owner}' BYPASSRLS;
CREATE ROLE campuslingo_auth LOGIN PASSWORD '${passwords.auth}';
CREATE ROLE campuslingo_app LOGIN PASSWORD '${passwords.app}';
CREATE ROLE campuslingo_grader LOGIN PASSWORD '${passwords.grader}';
CREATE ROLE campuslingo_worker LOGIN PASSWORD '${passwords.worker}' BYPASSRLS;
CREATE ROLE campuslingo_recovery LOGIN PASSWORD '${passwords.recovery}';
`;
  let result = psql(['-v', 'ON_ERROR_STOP=1', '-U', 'postgres', '-d', 'postgres', '-c', roles],
    { stdio: ['ignore', 'ignore', 'pipe'] });
  if (result.status !== 0) throw new Error(`role bootstrap failed: ${result.stderr?.slice(0, 200)}`);
  result = psql(['-v', 'ON_ERROR_STOP=1', '-U', 'postgres', '-d', 'postgres', '-c',
    'CREATE DATABASE campuslingo_test OWNER campuslingo_owner'], { stdio: ['ignore', 'ignore', 'pipe'] });
  if (result.status !== 0) throw new Error(`database bootstrap failed: ${result.stderr?.slice(0, 200)}`);
  result = psql(['-v', 'ON_ERROR_STOP=1', '-U', 'postgres', '-d', 'campuslingo_test', '-c',
    'CREATE EXTENSION vector'], { stdio: ['ignore', 'ignore', 'pipe'] });
  if (result.status !== 0) throw new Error(`pgvector bootstrap failed: ${result.stderr?.slice(0, 200)}`);

  const adminUrl = url('campuslingo_owner', passwords.owner);
  run(serverBinary, ['migrate'], {
    env: { CAMPUSLINGO_ADMIN_DATABASE_URL: adminUrl },
    stdio: ['ignore', 'ignore', 'pipe'],
  });
  for (const grants of ['m1_grants.sql', 'm2_grants.sql']) {
    result = psql(['-v', 'ON_ERROR_STOP=1', '-U', 'campuslingo_owner', '-d',
      'campuslingo_test', '-f', path.join(root, 'scripts', grants)],
      { stdio: ['ignore', 'ignore', 'pipe'] });
    if (result.status !== 0) throw new Error(`${grants} failed: ${result.stderr?.slice(0, 200)}`);
  }

  const ownerId = randomUUID();
  const tenantId = randomUUID();
  const courseId = randomUUID();
  const jobIdValue = randomUUID();
  const idempotency = randomUUID();
  assetId = randomUUID();
  jobId = jobIdValue;
  await copyFile(fixture, path.join(assetDir, assetId));
  await chmod(path.join(assetDir, assetId), 0o600);
  const sourceInfo = await stat(fixture);
  const sourceSha = await sha256(fixture);
  const payload = JSON.stringify({ schema_version: 1, kind: 'pdf_ingestion', asset_id: assetId });
  const payloadHash = createHash('sha256').update(payload).digest('hex');
  const seed = `
BEGIN;
INSERT INTO users (id, email, password_hash) VALUES
  ('${ownerId}', 'o01@local.test', 'o01-unusable-password-hash');
INSERT INTO tenants (id, owner_id, name) VALUES
  ('${tenantId}', '${ownerId}', 'O-01 measurement');
INSERT INTO tenant_members (tenant_id, user_id, role) VALUES
  ('${tenantId}', '${ownerId}', 'owner');
INSERT INTO courses (id, tenant_id, created_by, name, visibility) VALUES
  ('${courseId}', '${tenantId}', '${ownerId}', 'O-01 measurement', 'private');
INSERT INTO course_members (course_id, tenant_id, user_id, role) VALUES
  ('${courseId}', '${tenantId}', '${ownerId}', 'instructor');
INSERT INTO assets (id, tenant_id, course_id, created_by, sha256, media_type, bytes) VALUES
  ('${assetId}', '${tenantId}', '${courseId}', '${ownerId}', '${sourceSha}', 'application/pdf', ${sourceInfo.size});
INSERT INTO jobs (id, tenant_id, course_id, created_by, idempotency_key, payload, payload_hash) VALUES
  ('${jobIdValue}', '${tenantId}', '${courseId}', '${ownerId}', '${idempotency}',
   '${payload.replaceAll("'", "''")}'::jsonb, decode('${payloadHash}', 'hex'));
COMMIT;
`;
  result = psql(['-v', 'ON_ERROR_STOP=1', '-U', 'campuslingo_owner', '-d',
    'campuslingo_test', '-c', seed], { stdio: ['ignore', 'ignore', 'pipe'] });
  if (result.status !== 0) throw new Error(`seed failed: ${result.stderr?.slice(0, 400)}`);

  const httpPort = await freePort();
  const serverFd = await open(serverLog, 'w', 0o600);
  const serverStarted = Date.now();
  server = spawn(serverBinary, [], {
    cwd: root,
    env: {
      ...process.env,
      CAMPUSLINGO_BIND: `127.0.0.1:${httpPort}`,
      CAMPUSLINGO_AUTH_DATABASE_URL: url('campuslingo_auth', passwords.auth),
      CAMPUSLINGO_APP_DATABASE_URL: url('campuslingo_app', passwords.app),
      CAMPUSLINGO_GRADER_DATABASE_URL: url('campuslingo_grader', passwords.grader),
      CAMPUSLINGO_RECOVERY_KEY: randomBytes(32).toString('hex'),
      CAMPUSLINGO_ASSET_DIR: assetDir,
    },
    stdio: ['ignore', serverFd.fd, serverFd.fd],
  });
  await waitFor(`http://127.0.0.1:${httpPort}/readyz`, Date.now() + 15_000);
  timings.server_ready_ms = Date.now() - serverStarted;

  const availableBeforeWorker = await availableBytes();
  minimumAvailable = Math.min(minimumAvailable ?? availableBeforeWorker, availableBeforeWorker);
  const workerFd = await open(workerLog, 'w', 0o600);
  const workerStarted = Date.now();
  const worker = spawn(ingestionBinary, [
    'worker-once', '--work-dir', workDir, '--asset-dir', assetDir,
    '--managed-ocr', '--model', model, '--mmproj', mmproj, '--threads', String(threads),
  ], {
    cwd: root,
    env: {
      ...process.env,
      CAMPUSLINGO_WORKER_DATABASE_URL: url('campuslingo_worker', passwords.worker),
      CAMPUSLINGO_HELPER_CGROUP_ROOT: serviceRoot,
    },
    stdio: ['ignore', workerFd.fd, workerFd.fd],
  });
  const pidPeaks = new Map([[worker.pid, 0], [server.pid, 0]]);
  const sample = async () => {
    const gpu = await drmGpuMemory();
    samples.push({
      at_ms: Date.now() - sampleStarted,
      cgroup_current_bytes: await readNumber(memoryCurrentPath),
      mem_available_bytes: await availableBytes(),
      vram_used_bytes: gpu.vramUsed,
      gtt_used_bytes: gpu.gttUsed,
    });
    for (const [pid] of pidPeaks) {
      try {
        const status = await readFile(`/proc/${pid}/status`, 'utf8');
        const rss = /^VmRSS:\s+(\d+) kB$/m.exec(status);
        if (rss) pidPeaks.set(pid, Math.max(pidPeaks.get(pid), Number(rss[1]) * 1024));
      } catch {
        // Process exit between samples is expected.
      }
    }
  };
  const sampler = setInterval(() => { void sample(); }, 500);
  await sample();
  const workerExit = await new Promise((resolve) => {
    worker.on('exit', (code, signal) => resolve({ code, signal }));
  });
  clearInterval(sampler);
  await sample();
  timings.worker_ms = Date.now() - workerStarted;
  await workerFd.close();
  const workerText = await readFile(workerLog, 'utf8');
  try {
    workerResult = JSON.parse(workerText);
  } catch {
    workerResult = { parse_error: true, tail: workerText.slice(-2000) };
  }
  if (workerExit.code !== 0) {
    throw new Error(`worker exited with ${workerExit.signal ?? workerExit.code}`);
  }
  const released = workerResult?.model_release;
  if (!released || released.process_tree_reaped !== true || released.cgroup_removed !== true ||
      released.memory_budget_exceeded !== false || (released.cgroup_oom_kill_count ?? 0) !== 0) {
    throw new Error('model release evidence is incomplete or failed');
  }
  const releaseFiles = readdirSync(workDir)
    .filter((name) => /^llama-ocr-.*\.release\.json$/.test(name))
    .sort();
  const modelReleases = releaseFiles.map((name) =>
    JSON.parse(readFileSync(path.join(workDir, name), 'utf8')));
  if (modelReleases.length === 0 || modelReleases.some((release) =>
    release.process_tree_reaped !== true || release.cgroup_removed !== true ||
    release.memory_budget_exceeded !== false || (release.cgroup_oom_kill_count ?? 0) !== 0 ||
    release.termination)) {
    throw new Error('managed OCR release files are missing or failed');
  }
  const minimumModelHost = modelReleases.reduce((minimum, release) =>
    release.minimum_host_available_bytes == null ? minimum :
      Math.min(minimum, release.minimum_host_available_bytes), Infinity);
  if (Number.isFinite(minimumModelHost) && minimumModelHost < modelHostReserveBytes) {
    throw new Error('a managed OCR release dipped below the host safety reserve');
  }

  const jobStatus = psqlValue(`
SELECT status || '|' || attempts || '|' || progress_done || '|' || progress_total
FROM jobs WHERE id = '${jobIdValue}'`);
  const pages = psqlValue(`
SELECT count(*) || '|' || count(*) FILTER (WHERE artifact->>'route' = 'ocr') || '|' ||
       count(*) FILTER (WHERE artifact ? 'ocr' OR
         coalesce(jsonb_array_length(artifact->'region_ocr'), 0) > 0)
FROM ingestion_pages`);
  const ocrEvidencePages = Number(pages.split('|')[2] ?? 0);
  if (!(ocrEvidencePages > 0)) {
    throw new Error('the managed OCR model produced no persisted OCR evidence');
  }
  const chunksAndCrops = psqlValue(`
SELECT (SELECT count(*) FROM ingestion_chunks) || '|' || (SELECT count(*) FROM ingestion_crops)`);
  const provenance = psqlValue(`
SELECT (provenance->'ocr_model'->>'model') || '|' ||
       (provenance->'ocr_model'->>'model_sha256') || '|' ||
       (provenance->'ocr_model'->>'mmproj_sha256')
FROM ingestion_runs LIMIT 1`);

  minimumAvailable = Math.min(minimumAvailable ?? availableBeforeWorker,
    ...samples.map((entry) => entry.mem_available_bytes).filter(Number.isFinite));
  const finalPeak = await readNumber(memoryPeakPath);
  const peakSamples = samples.map((entry) => entry.cgroup_current_bytes).filter(Number.isFinite);
  const vramSamples = samples.map((entry) => entry.vram_used_bytes).filter(Number.isFinite);
  const gttSamples = samples.map((entry) => entry.gtt_used_bytes).filter(Number.isFinite);
  const peakVramUsed = vramSamples.length ? Math.max(...vramSamples) : baselineGpu.vramUsed;
  const peakGttUsed = gttSamples.length ? Math.max(...gttSamples) : baselineGpu.gttUsed;
  const report = {
    schema_version: 1,
    observed_at: startedAt.toISOString(),
    register: 'O-01',
    decision: 'DEC-017',
    status: 'MEASURED_SCOPED',
    scope: 'One synthetic 4-page PDF ingestion with one OCR page while the Rust API and PostgreSQL are resident. Redis is absent from the current M2 stack, not simulated.',
    profile: 'debug binaries',
    host: {
      cpus: 16,
      memory_kib: (await readFile('/proc/meminfo', 'utf8')).match(/^MemTotal:\s+(\d+) kB$/m)?.[1] ?? null,
      cgroup_peak_baseline_bytes: baselinePeak,
      cgroup_peak_final_bytes: finalPeak,
      cgroup_peak_absolute_bytes: finalPeak,
      cgroup_sampled_peak_bytes: peakSamples.length ? Math.max(...peakSamples) : null,
      cgroup_peak_path: memoryPeakPath,
      minimum_mem_available_bytes: minimumAvailable,
      safety_reserve_bytes: safetyReserveBytes,
      safety_reserve_respected: minimumAvailable === null ? null : minimumAvailable >= safetyReserveBytes,
      model_host_reserve_bytes: modelHostReserveBytes,
      model_minimum_host_available_bytes:
        Number.isFinite(minimumModelHost) ? minimumModelHost : null,
      drm_vram_total_bytes: baselineGpu.vramTotal,
      drm_vram_baseline_used_bytes: baselineGpu.vramUsed,
      drm_vram_peak_used_bytes: peakVramUsed,
      drm_gtt_total_bytes: baselineGpu.gttTotal,
      drm_gtt_baseline_used_bytes: baselineGpu.gttUsed,
      drm_gtt_peak_used_bytes: peakGttUsed,
    },
    model: {
      identity: {
        model,
        mmproj,
        model_sha256: await sha256(model),
        mmproj_sha256: await sha256(mmproj),
        threads,
        budget_bytes: 2 * 1024 * 1024 * 1024,
      },
      release: released,
      releases: modelReleases,
      minimum_host_available_bytes:
        Number.isFinite(minimumModelHost) ? minimumModelHost : null,
      restarts: workerResult?.model_restarts ?? 0,
      observed_rss_ceiling_bytes: workerResult?.result?.model_peak_rss_bytes ??
        workerResult?.model_peak_rss_bytes ?? null,
      provenance_observed: provenance,
    },
    worker: {
      exit: workerExit,
      duration_ms: timings.worker_ms,
      result: workerResult,
      process_peak_rss_bytes: pidPeaks.get(worker.pid) ?? null,
    },
    server: {
      ready_ms: timings.server_ready_ms,
      process_peak_rss_bytes: pidPeaks.get(server.pid) ?? null,
    },
    postgres: { start_ms: timings.postgres_start_ms },
    job: {
      id: jobId,
      asset_id: assetId,
      status_progress: jobStatus,
      pages: pages,
      ocr_evidence_pages: ocrEvidencePages,
      chunks_crops: chunksAndCrops,
    },
    samples: samples.length,
    exclusions: [
      'Redis is not installed or started; its footprint is not included.',
      'DRM VRAM carveout (4 GiB) is separate from Linux MemTotal (11 GiB), while GTT allocations share unified physical memory and overlap MemAvailable; do not add VRAM and GTT blindly.',
      'This is not a 30-player load test (O-06) or a full 3-PDF corpus run.',
    ],
    admission_evidence: {
      implementation: 'crates/ingestion/src/worker.rs pauses before reaper/claim below 1.5 GiB',
      unit_test: 'worker::tests::low_memory_pauses_before_any_database_claim',
      floor_bytes: safetyReserveBytes,
    },
    privacy: 'Synthetic fixture and public hash-pinned model only; no private corpus content and no inference-time network request.',
    g1_status: 'OPEN',
    o01_status: 'MEASURED_SCOPED',
  };
  const reportPath = path.join(base, 'o01-measurement.json');
  await writeFile(reportPath, JSON.stringify(report, null, 2) + '\n', { mode: 0o600 });
  process.stdout.write(JSON.stringify({
    status: report.status,
    report: path.relative(root, reportPath),
    cgroup_peak_bytes: report.host.cgroup_peak_absolute_bytes,
    minimum_mem_available_bytes: report.host.minimum_mem_available_bytes,
    model_peak_rss_bytes: report.model.observed_rss_ceiling_bytes,
    job_status_progress: report.job.status_progress,
    pages: report.job.pages,
    ocr_evidence_pages: report.job.ocr_evidence_pages,
    chunks_crops: report.job.chunks_crops,
  }, null, 2) + '\n');
} finally {
  if (server) {
    server.kill('SIGTERM');
    await new Promise((resolve) => {
      const timer = setTimeout(() => { server.kill('SIGKILL'); resolve(); }, 5000);
      server.on('exit', () => { clearTimeout(timer); resolve(); });
    });
  }
  if (postgresStarted) {
    spawnSync(path.join(pgBin, 'pg_ctl'), ['-D', data, '-m', 'immediate', 'stop'],
      { cwd: root, stdio: 'ignore' });
  }
  if (process.env.CAMPUSLINGO_KEEP_O01 === '1') {
    process.stderr.write(`kept ${base}\n`);
  } else {
    // Keep logs, assets and the report, but not the disposable PostgreSQL data.
    await rm(data, { recursive: true, force: true }).catch(() => {});
    await rm(socket, { recursive: true, force: true }).catch(() => {});
  }
}
