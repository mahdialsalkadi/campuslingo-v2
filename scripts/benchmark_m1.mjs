#!/usr/bin/env node
// M1 authenticated HTTP load probe. This starts a disposable PostgreSQL 16
// cluster (no Docker), seeds 30 learner sessions and one public course, runs the
// real release server, and drives review, job, and private-asset routes.
//
// The report deliberately separates the Rust server's RSS from PostgreSQL's.
// It is NOT a 15-duel/combat benchmark; M1 has no combat loop or matchmaker.

import { spawn, spawnSync } from 'node:child_process';
import { createHash, randomBytes, randomUUID } from 'node:crypto';
import { chmod, mkdir, readFile, rm, stat, writeFile } from 'node:fs/promises';
import net from 'node:net';
import path from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import { performance } from 'node:perf_hooks';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const pgBin =
  process.env.CAMPUSLINGO_PG_BIN ??
  '/home/mahdi/campus-lingo/services/core-ai/.venv/lib/python3.11/site-packages/pgserver/pginstall/bin';
const work = path.join(root, 'artifacts', 'private', 'pgtest', 'm1-load');
const data = path.join(work, 'data');
const socketDir = path.join(work, 'socket');
const log = path.join(work, 'postgres.log');
const assetDir = path.join(work, 'assets');
const keep = process.env.CAMPUSLINGO_KEEP_PG === '1';
const seconds = Number(process.env.CAMPUSLINGO_LOAD_SECONDS ?? '15');
const players = Number(process.env.CAMPUSLINGO_PLAYERS ?? '30');
const thinkMs = Number(process.env.CAMPUSLINGO_THINK_MS ?? '350');

if (!Number.isFinite(seconds) || seconds < 2 || seconds > 300) {
  throw Error('CAMPUSLINGO_LOAD_SECONDS must be between 2 and 300');
}
if (!Number.isInteger(players) || players < 1 || players > 60) {
  throw Error('CAMPUSLINGO_PLAYERS must be an integer between 1 and 60');
}
if (!Number.isFinite(thinkMs) || thinkMs < 50 || thinkMs > 5000) {
  throw Error('CAMPUSLINGO_THINK_MS must be between 50 and 5000');
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: root,
    stdio: 'inherit',
    env: { ...process.env, ...options.env },
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(' ')} exited with ${result.status}`);
  }
}

function psql(args) {
  return spawnSync(path.join(pgBin, 'psql'), args, {
    cwd: root,
    stdio: 'inherit',
    env: { ...process.env, PGHOST: socketDir, PGPORT: String(port) },
  });
}

function requireSuccess(result, label) {
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${label} failed with ${result.status}`);
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

const wait = milliseconds => new Promise(resolve => setTimeout(resolve, milliseconds));

async function rssOf(pid) {
  try {
    const status = await readFile(`/proc/${pid}/status`, 'utf8');
    return Number(status.match(/^VmRSS:\s+(\d+)/m)?.[1] ?? 0) * 1024;
  } catch {
    return 0;
  }
}

async function hwmOf(pid) {
  try {
    const status = await readFile(`/proc/${pid}/status`, 'utf8');
    return Number(status.match(/^VmHWM:\s+(\d+)/m)?.[1] ?? 0) * 1024;
  } catch {
    return 0;
  }
}

async function childrenOf(pid) {
  try {
    const text = await readFile(`/proc/${pid}/task/${pid}/children`, 'utf8');
    return text.trim().split(/\s+/).filter(Boolean).map(Number);
  } catch {
    return [];
  }
}

async function treeRss(pid) {
  let total = await rssOf(pid);
  for (const child of await childrenOf(pid)) total += await treeRss(child);
  return total;
}

function url(role, password) {
  return `postgres://${role}:${password}@127.0.0.1:${port}/campuslingo_load`;
}

function quote(value) {
  return `'${String(value).replaceAll("'", "''")}'`;
}

function digestHex(token) {
  return createHash('sha256').update(token).digest('hex');
}

const port = await freePort();
const passwords = {
  owner: randomBytes(24).toString('hex'),
  auth: randomBytes(24).toString('hex'),
  app: randomBytes(24).toString('hex'),
  grader: randomBytes(24).toString('hex'),
  worker: randomBytes(24).toString('hex'),
  recovery: randomBytes(24).toString('hex'),
};

let started = false;
let server;
let sampler;
let sampling = false;
let serverPid;
let postgresPid;
let serverStderr = '';
const serverLogLines = { count: 0 };
const serverSamples = [];
const postgresSamples = [];

try {
  await stat(path.join(pgBin, 'initdb'));
  await rm(work, { recursive: true, force: true });
  await mkdir(socketDir, { recursive: true, mode: 0o700 });
  await mkdir(assetDir, { recursive: true, mode: 0o700 });
  run(process.env.CARGO ?? 'cargo', ['build', '--quiet', '--release', '-p', 'campuslingo-server']);

  run(path.join(pgBin, 'initdb'), [
    '-D', data,
    '-U', 'postgres',
    '--auth-local=trust',
    '--auth-host=scram-sha-256',
    '--encoding=UTF8',
    '--locale=C',
    '--no-sync',
  ]);
  run(path.join(pgBin, 'pg_ctl'), [
    '-D', data,
    '-l', log,
    '-o',
    `-p ${port} -k ${socketDir} -c listen_addresses=127.0.0.1 -c unix_socket_permissions=0700 -c max_connections=150`,
    '-w', 'start',
  ]);
  started = true;
  postgresPid = Number((await readFile(path.join(data, 'postmaster.pid'), 'utf8')).split('\n')[0]);

  const roles = `
SET password_encryption = 'scram-sha-256';
CREATE ROLE campuslingo_owner LOGIN PASSWORD '${passwords.owner}' BYPASSRLS;
CREATE ROLE campuslingo_auth LOGIN PASSWORD '${passwords.auth}';
CREATE ROLE campuslingo_app LOGIN PASSWORD '${passwords.app}';
CREATE ROLE campuslingo_grader LOGIN PASSWORD '${passwords.grader}';
CREATE ROLE campuslingo_worker LOGIN PASSWORD '${passwords.worker}' BYPASSRLS;
CREATE ROLE campuslingo_recovery LOGIN PASSWORD '${passwords.recovery}';
`;
  requireSuccess(
    psql(['-v', 'ON_ERROR_STOP=1', '-U', 'postgres', '-d', 'postgres', '-c', roles]),
    'role bootstrap',
  );
  requireSuccess(
    psql([
      '-v', 'ON_ERROR_STOP=1', '-U', 'postgres', '-d', 'postgres',
      '-c', 'CREATE DATABASE campuslingo_load OWNER campuslingo_owner',
    ]),
    'database bootstrap',
  );

  run(process.env.CARGO ?? 'cargo', ['run', '--quiet', '-p', 'campuslingo-server', '--', 'migrate'], {
    env: { CAMPUSLINGO_ADMIN_DATABASE_URL: url('campuslingo_owner', passwords.owner) },
  });
  requireSuccess(
    psql([
      '-v', 'ON_ERROR_STOP=1', '-U', 'campuslingo_owner', '-d', 'campuslingo_load',
      '-f', path.join(root, 'scripts', 'm1_grants.sql'),
    ]),
    'grant application',
  );

  const ownerId = randomUUID();
  const courseId = randomUUID();
  const tenantId = randomUUID();
  const nodeId = randomUUID();
  const assetId = randomUUID();
  const ownerToken = randomBytes(32).toString('hex');
  const learners = Array.from({ length: players }, (_, index) => ({
    id: randomUUID(),
    email: `player-${String(index + 1).padStart(2, '0')}@load.example.test`,
    token: randomBytes(32).toString('hex'),
  }));
  const challenges = Array.from({ length: 6 }, () => ({
    id: randomUUID(),
    prompt: 'Load probe: which option is correct?',
  }));

  const assetBody = Buffer.alloc(128 * 1024, 0x61);
  const assetDigest = createHash('sha256').update(assetBody).digest('hex');
  const assetPath = path.join(assetDir, assetId);
  await writeFile(assetPath, assetBody, { mode: 0o600 });
  await chmod(assetPath, 0o600);

  const allUsers = [
    { id: ownerId, email: 'owner@load.example.test', token: ownerToken },
    ...learners,
  ];
  const userRows = allUsers
    .map(user => `(${quote(user.id)},${quote(user.email)},${quote('load-probe-unusable')})`)
    .join(',');
  const sessionRows = allUsers
    .map(user => `(decode(${quote(digestHex(user.token))},'hex'),${quote(user.id)},clock_timestamp()+interval '1 day')`)
    .join(',');
  const memberRows = [
    `(${quote(courseId)},${quote(tenantId)},${quote(ownerId)},'instructor',TRUE)`,
    ...learners.map(
      learner => `(${quote(courseId)},${quote(tenantId)},${quote(learner.id)},'student',TRUE)`,
    ),
  ].join(',');
  const challengeRows = challenges
    .map(
      challenge =>
        `(${quote(challenge.id)},${quote(tenantId)},${quote(courseId)},${quote(nodeId)},${quote(challenge.prompt)},'["alpha","beta","gamma","delta"]'::jsonb,1)`,
    )
    .join(',');
  const answerRows = challenges
    .map(challenge => `(${quote(challenge.id)},1,'Load probe explanation')`)
    .join(',');

  const seed = `
BEGIN;
INSERT INTO users (id, email, password_hash) VALUES ${userRows};
INSERT INTO sessions (token_hash, user_id, expires_at) VALUES ${sessionRows};
INSERT INTO tenants (id, owner_id, name) VALUES (${quote(tenantId)},${quote(ownerId)},'Load tenant');
INSERT INTO tenant_members (tenant_id, user_id, role) VALUES (${quote(tenantId)},${quote(ownerId)},'owner');
INSERT INTO courses (id, tenant_id, created_by, name, visibility, published, rights_attested_at, published_by)
VALUES (${quote(courseId)},${quote(tenantId)},${quote(ownerId)},'Load course','public',TRUE,clock_timestamp(),${quote(ownerId)});
INSERT INTO course_members (course_id, tenant_id, user_id, role, active) VALUES ${memberRows};
INSERT INTO learning_nodes (id, tenant_id, course_id, title)
VALUES (${quote(nodeId)},${quote(tenantId)},${quote(courseId)},'Load unit');
INSERT INTO challenges (id, tenant_id, course_id, node_id, prompt, options, version)
VALUES ${challengeRows};
INSERT INTO challenge_answers (challenge_id, correct_index, explanation) VALUES ${answerRows};
INSERT INTO assets (id, tenant_id, course_id, created_by, sha256, media_type, bytes)
VALUES (${quote(assetId)},${quote(tenantId)},${quote(courseId)},${quote(ownerId)},${quote(assetDigest)},'text/plain',${assetBody.length});
COMMIT;
`;
  requireSuccess(
    psql(['-v', 'ON_ERROR_STOP=1', '-U', 'campuslingo_owner', '-d', 'campuslingo_load', '-c', seed]),
    'load seed',
  );

  if (keep) {
    await writeFile(
      path.join(work, 'connection.json'),
      JSON.stringify(
        {
          host: '127.0.0.1',
          port,
          admin: url('campuslingo_owner', passwords.owner),
          auth: url('campuslingo_auth', passwords.auth),
          app: url('campuslingo_app', passwords.app),
          grader: url('campuslingo_grader', passwords.grader),
          worker: url('campuslingo_worker', passwords.worker),
          recovery: url('campuslingo_recovery', passwords.recovery),
          course_id: courseId,
          asset_id: assetId,
        },
        null,
        2,
      ) + '\n',
      { mode: 0o600 },
    );
  }

  server = spawn(path.join(root, 'target/release/campuslingo-server'), [], {
    cwd: root,
    env: {
      ...process.env,
      CAMPUSLINGO_BIND: '127.0.0.1:0',
      CAMPUSLINGO_AUTH_DATABASE_URL: url('campuslingo_auth', passwords.auth),
      CAMPUSLINGO_APP_DATABASE_URL: url('campuslingo_app', passwords.app),
      CAMPUSLINGO_GRADER_DATABASE_URL: url('campuslingo_grader', passwords.grader),
      CAMPUSLINGO_RECOVERY_KEY: randomBytes(32).toString('hex'),
      CAMPUSLINGO_ASSET_DIR: assetDir,
      CAMPUSLINGO_COOKIE_SECURE: '0',
      CAMPUSLINGO_M0_WS: '0',
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  serverPid = server.pid;
  server.stderr.on('data', bytes => {
    serverStderr = (serverStderr + bytes.toString()).slice(-8000);
  });
  const lineReader = createInterface({ input: server.stdout });
  const address = await new Promise((resolve, reject) => {
    const timer = setTimeout(
      () => reject(Error(`server startup timeout: ${serverStderr}`)),
      15000,
    );
    lineReader.on('line', line => {
      serverLogLines.count += 1;
      try {
        const event = JSON.parse(line);
        if (event.event === 'listening') {
          clearTimeout(timer);
          resolve(event.address);
        }
      } catch {
        // Only structured startup is trusted; request logs are counted, not stored.
      }
    });
    server.once('error', error => {
      clearTimeout(timer);
      reject(error);
    });
    server.once('exit', code => {
      clearTimeout(timer);
      reject(Error(`server exited ${code}: ${serverStderr}`));
    });
  });

  for (let attempt = 0; attempt < 50; attempt += 1) {
    try {
      const ready = await fetch(`http://${address}/readyz`);
      if (ready.ok) break;
    } catch {
      // Server may still be binding pools.
    }
    if (attempt === 49) throw Error(`readiness timeout: ${serverStderr}`);
    await wait(100);
  }

  const idleRss = await rssOf(serverPid);
  const idlePostgresRss = await treeRss(postgresPid);
  sampling = true;
  sampler = (async () => {
    while (sampling) {
      if (serverPid) serverSamples.push(await rssOf(serverPid));
      postgresSamples.push(await treeRss(postgresPid));
      await wait(100);
    }
  })();

  const origin = `http://${address}`;
  const base = `http://${address}`;
  const latencies = [];
  const byOperation = new Map();
  const statusCounts = new Map();
  const errors = [];
  let downloadedBytes = 0;
  let completedRequests = 0;

  async function call(kind, pathName, options, expected) {
    const started = performance.now();
    let response;
    try {
      response = await fetch(`${base}${pathName}`, {
        ...options,
        headers: {
          origin,
          ...(options?.headers ?? {}),
        },
        signal: AbortSignal.timeout(10000),
      });
      const body = await response.arrayBuffer();
      downloadedBytes += body.byteLength;
      completedRequests += 1;
      latencies.push(performance.now() - started);
      byOperation.set(kind, (byOperation.get(kind) ?? 0) + 1);
      statusCounts.set(response.status, (statusCounts.get(response.status) ?? 0) + 1);
      if (!expected.includes(response.status)) {
        errors.push(`${kind}: expected ${expected.join('/')} but received ${response.status}`);
      }
      return { status: response.status, body };
    } catch (error) {
      errors.push(`${kind}: ${String(error)}`);
      return { status: 0, body: new ArrayBuffer(0) };
    }
  }

  const cookie = token => `campuslingo_session=${token}`;
  const end = performance.now() + seconds * 1000;

  const ownerLoop = async () => {
    let sequence = 0;
    while (performance.now() < end) {
      sequence += 1;
      const key = randomUUID();
      const enqueued = await call(
        'enqueue_job',
        `/v1/courses/${courseId}/jobs`,
        {
          method: 'POST',
          headers: {
            'content-type': 'application/json',
            'idempotency-key': key,
            cookie: cookie(ownerToken),
          },
          body: JSON.stringify({ kind: 'load-probe', sequence }),
        },
        [201],
      );
      if (enqueued.status === 201) {
        try {
          const parsed = JSON.parse(Buffer.from(enqueued.body).toString('utf8'));
          await call(
            'get_job',
            `/v1/courses/${courseId}/jobs/${parsed.id}`,
            { headers: { cookie: cookie(ownerToken) } },
            [200],
          );
        } catch {
          errors.push('enqueue_job: response was not valid JSON');
        }
      }
      await wait(thinkMs);
    }
  };

  const learnerLoop = async (learner, index) => {
    let cycle = 0;
    while (performance.now() < end) {
      cycle += 1;
      const challenge = challenges[(index + cycle) % challenges.length];
      const attempt = await call(
        'create_attempt',
        `/v1/courses/${courseId}/challenges/${challenge.id}/attempts`,
        {
          method: 'POST',
          headers: { 'content-type': 'application/json', cookie: cookie(learner.token) },
          body: '{}',
        },
        [201],
      );
      if (attempt.status === 201) {
        try {
          const parsed = JSON.parse(Buffer.from(attempt.body).toString('utf8'));
          await call(
            'answer_attempt',
            `/v1/attempts/${parsed.attempt_id}/answer`,
            {
              method: 'POST',
              headers: { 'content-type': 'application/json', cookie: cookie(learner.token) },
              body: JSON.stringify({ selected_index: cycle % 4 }),
            },
            [200],
          );
        } catch {
          errors.push('create_attempt: response was not valid JSON');
        }
      }
      if (cycle % 3 === 0) {
        await call(
          'read_content',
          `/v1/courses/${courseId}/content`,
          { headers: { cookie: cookie(learner.token) } },
          [200],
        );
      }
      if (cycle % 4 === 0) {
        await call(
          'practice_question',
          `/v1/courses/${courseId}/challenges/${challenge.id}/practice`,
          { headers: { cookie: cookie(learner.token) } },
          [200],
        );
      }
      if (cycle % 5 === 0) {
        await call(
          'download_asset',
          `/v1/assets/${assetId}`,
          { headers: { cookie: cookie(learner.token) } },
          [200],
        );
      }
      await wait(thinkMs);
    }
  };

  await Promise.all([
    ownerLoop(),
    ...learners.map((learner, index) => learnerLoop(learner, index)),
  ]);
  const peakRss = Math.max(idleRss, ...serverSamples, await hwmOf(serverPid));
  const peakPostgresRss = Math.max(idlePostgresRss, ...postgresSamples);
  const health = await (await fetch(`${base}/healthz`)).json();
  const readiness = await fetch(`${base}/readyz`);
  latencies.sort((left, right) => left - right);
  const percentile = p =>
    latencies.length === 0 ? null : latencies[Math.min(latencies.length - 1, Math.floor(latencies.length * p))];

  const report = {
    observed_at: new Date().toISOString(),
    stage: 'M1 authenticated HTTP and persistence; no combat loop',
    workload:
      `${players} authenticated learners creating attempts, answering reviews, reading content/practice, and downloading a private 128 KiB asset; ` +
      'one instructor enqueues/reads durable jobs',
    duration_seconds: seconds,
    think_time_ms: thinkMs,
    seed: {
      users: allUsers.length,
      sessions: allUsers.length,
      course_members: players + 1,
      challenges: challenges.length,
      asset_bytes: assetBody.length,
    },
    requests: completedRequests,
    requests_per_second: Number((completedRequests / seconds).toFixed(2)),
    by_operation: Object.fromEntries([...byOperation.entries()].sort()),
    http_statuses: Object.fromEntries([...statusCounts.entries()].sort((a, b) => a[0] - b[0])),
    error_count: errors.length,
    errors: errors.slice(0, 20),
    downloaded_bytes: downloadedBytes,
    latency_ms: {
      p50: percentile(0.5),
      p95: percentile(0.95),
      p99: percentile(0.99),
      max: latencies.at(-1) ?? null,
    },
    rust_server: {
      idle_rss_bytes: idleRss,
      peak_rss_bytes: peakRss,
      target_bytes: 50 * 1024 * 1024,
      target_passed: peakRss < 50 * 1024 * 1024,
      note: 'Measured separately from PostgreSQL and any AI/model process.',
    },
    postgresql: {
      idle_tree_rss_bytes: idlePostgresRss,
      peak_tree_rss_bytes: peakPostgresRss,
      note: 'Sum of postmaster and backend RSS; shared_buffers are mapped by each process and are therefore counted more than once. This is a conservative separate figure, not unique physical memory.',
    },
    health,
    readiness_http: readiness.status,
    server_request_log_lines: serverLogLines.count,
    status:
      errors.length === 0 &&
      completedRequests > players &&
      peakRss < 50 * 1024 * 1024 &&
      readiness.status === 200
        ? 'passed_m1_http_probe'
        : 'failed',
    not_a_claim: [
      'This is not a 15-duel or 90-second combat benchmark; M1 has no combat, matchmaking, Redis, or AI worker contention.',
      'Sessions were seeded directly in PostgreSQL to avoid the authentication rate-limit path; auth itself is covered by the M1 acceptance suite.',
      'Latency includes local PostgreSQL, request logging, and localhost HTTP; it is not a WAN or device measurement.',
    ],
  };
  await mkdir(path.join(root, 'artifacts', 'benchmarks'), { recursive: true });
  await writeFile(
    path.join(root, 'artifacts', 'benchmarks', 'm1-server-load.json'),
    JSON.stringify(report, null, 2) + '\n',
  );
  console.log(JSON.stringify(report, null, 2));
  if (report.status === 'failed') process.exitCode = 1;
} finally {
  sampling = false;
  if (sampler) await sampler;
  if (server) {
    server.kill('SIGINT');
    await Promise.race([
      new Promise(resolve => server.once('exit', resolve)),
      wait(5000).then(() => server.kill('SIGKILL')),
    ]);
  }
  if (started) {
    spawnSync(path.join(pgBin, 'pg_ctl'), ['-D', data, '-m', 'immediate', 'stop'], {
      cwd: root,
      stdio: 'inherit',
    });
  }
  if (!keep) await rm(work, { recursive: true, force: true });
  else console.log(`kept private cluster at ${work}`);
}
