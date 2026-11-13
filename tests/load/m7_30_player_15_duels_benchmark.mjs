#!/usr/bin/env node
/**
 * M7 Blocking Gate O-06: 30 Concurrent Players / 15 Simultaneous Live 1v1 Duels
 * inside delegated cgroup-v2 memory containment.
 *
 * Run 1: Standalone 30-player / 15-simultaneous-duel load + concurrent HTTP traffic.
 * Run 2: Concurrent 30-player / 15-duel load while sequential Piper Arabic TTS/ingestion
 *        runs in background under exclusive ModelLease.
 *
 * Usage:
 *   bash scripts/run_ingestion_contained.sh node tests/load/m7_30_player_15_duels_benchmark.mjs
 */

import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { createHash, randomBytes, randomUUID } from 'node:crypto';
import { mkdir, readFile, rm, stat, writeFile } from 'node:fs/promises';
import net from 'node:net';
import path from 'node:path';
import { createInterface } from 'node:readline';
import { performance } from 'node:perf_hooks';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const pgBin =
  process.env.CAMPUSLINGO_PG_BIN ??
  '/home/mahdi/campus-lingo/services/core-ai/.venv/lib/python3.11/site-packages/pgserver/pginstall/bin';
const work = path.join(root, 'artifacts', 'private', 'pgtest', 'm7-o06-load');
const data = path.join(work, 'data');
const socketDir = path.join(work, 'socket');
const log = path.join(work, 'postgres.log');
const assetDir = path.join(work, 'assets');
const outPath = path.join(root, 'artifacts', 'benchmarks', 'm7-o06-30p-15duels-load.json');
const PLAYERS = 30;
const DUEL_PAIRS = 15;

function runSync(command, args, options = {}) {
  const res = spawnSync(command, args, {
    cwd: root,
    stdio: 'inherit',
    env: { ...process.env, ...options.env },
  });
  if (res.error) throw res.error;
  if (res.status !== 0) {
    throw new Error(`${command} ${args.join(' ')} exited ${res.status}`);
  }
}

function psql(args, port) {
  return spawnSync(path.join(pgBin, 'psql'), args, {
    cwd: root,
    stdio: 'inherit',
    env: { ...process.env, PGHOST: socketDir, PGPORT: String(port) },
  });
}

async function freePort() {
  return await new Promise((resolve, reject) => {
    const srv = net.createServer();
    srv.on('error', reject);
    srv.listen(0, '127.0.0.1', () => {
      const addr = srv.address();
      const port = typeof addr === 'object' && addr ? addr.port : 0;
      srv.close(() => resolve(port));
    });
  });
}

async function rssOf(pid) {
  try {
    const raw = await readFile(`/proc/${pid}/status`, 'utf8');
    return Number(raw.match(/^VmRSS:\s+(\d+)/m)?.[1] ?? 0) * 1024;
  } catch {
    return 0;
  }
}

async function readCgroupMetrics() {
  try {
    const cgroupRel = (await readFile('/proc/self/cgroup', 'utf8')).trim().replace(/^0::/, '');
    const base = path.join('/sys/fs/cgroup', cgroupRel);
    const parent = path.dirname(base);
    const tryRead = async p => {
      try {
        return Number((await readFile(p, 'utf8')).trim());
      } catch {
        return null;
      }
    };
    return {
      cgroup_path: cgroupRel,
      memory_peak_bytes: (await tryRead(path.join(parent, 'memory.peak'))) ?? (await tryRead(path.join(base, 'memory.peak'))),
      memory_swap_current_bytes: (await tryRead(path.join(parent, 'memory.swap.current'))) ?? (await tryRead(path.join(base, 'memory.swap.current'))),
    };
  } catch {
    return { cgroup_path: null, memory_peak_bytes: null, memory_swap_current_bytes: null };
  }
}

function percentile(sorted, p) {
  if (sorted.length === 0) return 0;
  const idx = Math.min(sorted.length - 1, Math.max(0, Math.ceil((p / 100) * sorted.length) - 1));
  return Number(sorted[idx].toFixed(3));
}

function summarize(values) {
  const sorted = [...values].sort((a, b) => a - b);
  return {
    count: sorted.length,
    p50: percentile(sorted, 50),
    p95: percentile(sorted, 95),
    p99: percentile(sorted, 99),
    max: sorted.length ? Number(sorted[sorted.length - 1].toFixed(3)) : 0,
  };
}

function quote(v) {
  return `'${String(v).replaceAll("'", "''")}'`;
}

function digestHex(token) {
  return createHash('sha256').update(token).digest('hex');
}

/**
 * Minimal loopback RFC6455 text WebSocket client with Cookie + Origin headers.
 */
async function connectWs(host, port, requestPath, headers) {
  const key = randomBytes(16).toString('base64');
  const sock = net.connect(port, host);
  await new Promise((resolve, reject) => {
    sock.once('connect', resolve);
    sock.once('error', reject);
  });

  const headerLines = [
    `GET ${requestPath} HTTP/1.1`,
    `Host: ${host}:${port}`,
    'Upgrade: websocket',
    'Connection: Upgrade',
    `Sec-WebSocket-Key: ${key}`,
    'Sec-WebSocket-Version: 13',
  ];
  for (const [k, v] of Object.entries(headers)) {
    headerLines.push(`${k}: ${v}`);
  }
  sock.write(headerLines.join('\r\n') + '\r\n\r\n');

  let buf = Buffer.alloc(0);
  const messages = [];
  const waiters = [];

  const wake = () => {
    while (waiters.length > 0) {
      const idx = waiters.findIndex(w => messages.some(w.pred));
      if (idx < 0) break;
      const w = waiters.splice(idx, 1)[0];
      const msgIdx = messages.findIndex(w.pred);
      const [found] = messages.splice(msgIdx, 1);
      w.resolve(found);
    }
  };

  let handshakeDone = false;
  let closed = false;

  await new Promise((resolve, reject) => {
    const onData = chunk => {
      buf = Buffer.concat([buf, chunk]);
      if (!handshakeDone) {
        const end = buf.indexOf('\r\n\r\n');
        if (end < 0) return;
        const head = buf.slice(0, end).toString('utf8');
        buf = buf.slice(end + 4);
        handshakeDone = true;
        if (!head.startsWith('HTTP/1.1 101')) {
          reject(new Error(`WS handshake failed: ${head.split('\r\n')[0]}`));
          return;
        }
        resolve();
      }
      // Decode any buffered WebSocket frames
      while (buf.length >= 2) {
        const b0 = buf[0];
        const b1 = buf[1];
        const opcode = b0 & 0x0f;
        const masked = (b1 & 0x80) !== 0;
        let len = b1 & 0x7f;
        let offset = 2;
        if (len === 126) {
          if (buf.length < 4) break;
          len = buf.readUInt16BE(2);
          offset = 4;
        } else if (len === 127) {
          if (buf.length < 10) break;
          len = Number(buf.readBigUInt64BE(2));
          offset = 10;
        }
        const maskLen = masked ? 4 : 0;
        if (buf.length < offset + maskLen + len) break;
        const payload = Buffer.from(buf.slice(offset + maskLen, offset + maskLen + len));
        if (masked) {
          const mask = buf.slice(offset, offset + 4);
          for (let i = 0; i < payload.length; i++) payload[i] ^= mask[i % 4];
        }
        buf = buf.slice(offset + maskLen + len);
        if (opcode === 0x8) {
          closed = true;
          sock.end();
        } else if (opcode === 0x9) {
          // Pong frame
          sendRawFrame(0x0a, payload);
        } else if (opcode === 0x1) {
          try {
            messages.push(JSON.parse(payload.toString('utf8')));
            wake();
          } catch {
            // ignore
          }
        }
      }
    };
    sock.on('data', onData);
    sock.once('error', err => {
      if (!handshakeDone) reject(err);
    });
  });

  function sendRawFrame(opcode, payloadBuf) {
    if (closed) return;
    const mask = randomBytes(4);
    const len = payloadBuf.length;
    let header;
    if (len < 126) {
      header = Buffer.alloc(6);
      header[0] = 0x80 | opcode;
      header[1] = 0x80 | len;
      mask.copy(header, 2);
    } else {
      header = Buffer.alloc(8);
      header[0] = 0x80 | opcode;
      header[1] = 0x80 | 126;
      header.writeUInt16BE(len, 2);
      mask.copy(header, 4);
    }
    const maskedPayload = Buffer.from(payloadBuf);
    for (let i = 0; i < maskedPayload.length; i++) maskedPayload[i] ^= mask[i % 4];
    sock.write(Buffer.concat([header, maskedPayload]));
  }

  return {
    sendJson(obj) {
      sendRawFrame(0x1, Buffer.from(JSON.stringify(obj), 'utf8'));
    },
    waitFor(pred, timeoutMs = 6000) {
      const idx = messages.findIndex(pred);
      if (idx >= 0) {
        return Promise.resolve(messages.splice(idx, 1)[0]);
      }
      return new Promise((resolve, reject) => {
        const timer = setTimeout(() => {
          const wIdx = waiters.indexOf(entry);
          if (wIdx >= 0) waiters.splice(wIdx, 1);
          const recent = messages.slice(-5).map(m => `${m.type}:${m.code ?? m.payload?.status ?? ''}`);
          reject(new Error(`WS waitFor timeout (queued=[${recent.join(',')}])`));
        }, timeoutMs);
        const entry = {
          pred,
          resolve: msg => {
            clearTimeout(timer);
            resolve(msg);
          },
        };
        waiters.push(entry);
      });
    },
    close() {
      if (!closed) {
        try {
          sendRawFrame(0x8, Buffer.alloc(0));
        } catch {
          // ignore
        }
        closed = true;
        sock.destroy();
      }
    },
  };
}

// 1. Ensure release binaries (server + migration) + precompiled onnx test exist BEFORE timing
runSync(process.env.CARGO ?? 'cargo', ['build', '--locked', '--release', '-p', 'campuslingo-server']);
runSync(process.env.CARGO ?? 'cargo', [
  'test',
  '--locked',
  '-p',
  'campuslingo-ingestion',
  '--features',
  'onnx',
  '--lib',
  '--no-run',
]);

await rm(work, { recursive: true, force: true });
await mkdir(socketDir, { recursive: true, mode: 0o700 });
await mkdir(assetDir, { recursive: true, mode: 0o700 });
await mkdir(path.dirname(outPath), { recursive: true });

const port = await freePort();
const passwords = {
  owner: randomBytes(24).toString('hex'),
  auth: randomBytes(24).toString('hex'),
  app: randomBytes(24).toString('hex'),
  grader: randomBytes(24).toString('hex'),
  worker: randomBytes(24).toString('hex'),
  recovery: randomBytes(24).toString('hex'),
};
const url = (role, pass) => `postgres://${role}:${pass}@127.0.0.1:${port}/campuslingo_load`;

let pgStarted = false;
let serverProc;

try {
  await stat(path.join(pgBin, 'initdb'));
  runSync(path.join(pgBin, 'initdb'), [
    '-D', data, '-U', 'postgres', '--auth-local=trust', '--auth-host=scram-sha-256',
    '--encoding=UTF8', '--locale=C', '--no-sync',
  ]);
  runSync(path.join(pgBin, 'pg_ctl'), [
    '-D', data, '-l', log,
    '-o', `-p ${port} -k ${socketDir} -c listen_addresses=127.0.0.1 -c unix_socket_permissions=0700 -c max_connections=120`,
    '-w', 'start',
  ]);
  pgStarted = true;

  const rolesSql = `
SET password_encryption = 'scram-sha-256';
CREATE ROLE campuslingo_owner LOGIN PASSWORD '${passwords.owner}' BYPASSRLS;
CREATE ROLE campuslingo_auth LOGIN PASSWORD '${passwords.auth}';
CREATE ROLE campuslingo_app LOGIN PASSWORD '${passwords.app}';
CREATE ROLE campuslingo_grader LOGIN PASSWORD '${passwords.grader}';
CREATE ROLE campuslingo_worker LOGIN PASSWORD '${passwords.worker}' BYPASSRLS;
CREATE ROLE campuslingo_recovery LOGIN PASSWORD '${passwords.recovery}';
`;
  if (psql(['-v', 'ON_ERROR_STOP=1', '-U', 'postgres', '-d', 'postgres', '-c', rolesSql], port).status !== 0) {
    throw new Error('role bootstrap failed');
  }
  if (
    psql(['-v', 'ON_ERROR_STOP=1', '-U', 'postgres', '-d', 'postgres', '-c', 'CREATE DATABASE campuslingo_load OWNER campuslingo_owner'], port).status !== 0 ||
    psql(['-v', 'ON_ERROR_STOP=1', '-U', 'postgres', '-d', 'campuslingo_load', '-c', 'CREATE EXTENSION vector'], port).status !== 0
  ) {
    throw new Error('database bootstrap failed');
  }

  runSync(path.join(root, 'target/release/campuslingo-server'), ['migrate'], {
    env: { CAMPUSLINGO_ADMIN_DATABASE_URL: url('campuslingo_owner', passwords.owner) },
  });
  for (let i = 1; i <= 7; i++) {
    const grantRes = psql(
      ['-v', 'ON_ERROR_STOP=1', '-U', 'campuslingo_owner', '-d', 'campuslingo_load', '-f', path.join(root, 'scripts', `m${i}_grants.sql`)],
      port,
    );
    if (grantRes.status !== 0) throw new Error(`m${i}_grants failed`);
  }

  const ownerId = randomUUID();
  const tenantId = randomUUID();
  const courseId = randomUUID();
  const nodeId = randomUUID();
  const learners = Array.from({ length: PLAYERS }, (_, idx) => ({
    id: randomUUID(),
    email: `duel-player-${String(idx + 1).padStart(2, '0')}@load.test`,
    token: randomBytes(32).toString('hex'),
  }));
  const challenges = Array.from({ length: 6 }, (_, i) => ({
    id: randomUUID(),
    prompt: `M7 Live Duel Load Challenge #${i + 1}`,
  }));

  const allUsers = [{ id: ownerId, email: 'owner@load.test', token: randomBytes(32).toString('hex') }, ...learners];
  const userRows = allUsers.map(u => `(${quote(u.id)},${quote(u.email)},${quote('load-unusable')})`).join(',');
  const sessionRows = allUsers
    .map(u => `(decode(${quote(digestHex(u.token))},'hex'),${quote(u.id)},clock_timestamp()+interval '1 day')`)
    .join(',');
  const memberRows = [
    `(${quote(courseId)},${quote(tenantId)},${quote(ownerId)},'instructor',TRUE)`,
    ...learners.map(l => `(${quote(courseId)},${quote(tenantId)},${quote(l.id)},'student',TRUE)`),
  ].join(',');
  const challengeRows = challenges
    .map(c => `(${quote(c.id)},${quote(tenantId)},${quote(courseId)},${quote(nodeId)},${quote(c.prompt)},'["alpha","beta","gamma","delta"]'::jsonb,1)`)
    .join(',');
  const answerRows = challenges.map(c => `(${quote(c.id)},1,'correct option')`).join(',');

  const seedSql = `
BEGIN;
INSERT INTO users (id, email, password_hash) VALUES ${userRows};
INSERT INTO sessions (token_hash, user_id, expires_at) VALUES ${sessionRows};
INSERT INTO tenants (id, owner_id, name) VALUES (${quote(tenantId)},${quote(ownerId)},'M7 Load Tenant');
INSERT INTO tenant_members (tenant_id, user_id, role) VALUES (${quote(tenantId)},${quote(ownerId)},'owner');
INSERT INTO courses (id, tenant_id, created_by, name, visibility, published, rights_attested_at, published_by)
VALUES (${quote(courseId)},${quote(tenantId)},${quote(ownerId)},'M7 Load Course','public',TRUE,clock_timestamp(),${quote(ownerId)});
INSERT INTO course_members (course_id, tenant_id, user_id, role, active) VALUES ${memberRows};
INSERT INTO learning_nodes (id, tenant_id, course_id, title)
VALUES (${quote(nodeId)},${quote(tenantId)},${quote(courseId)},'M7 Load Unit');
INSERT INTO challenges (id, tenant_id, course_id, node_id, prompt, options, version)
VALUES ${challengeRows};
INSERT INTO challenge_answers (challenge_id, correct_index, explanation) VALUES ${answerRows};
COMMIT;
`;
  if (psql(['-v', 'ON_ERROR_STOP=1', '-U', 'campuslingo_owner', '-d', 'campuslingo_load', '-c', seedSql], port).status !== 0) {
    throw new Error('seed failed');
  }

  // Launch authoritative release server with generous benchmark actor rate cap so 30 players run full waves cleanly
  serverProc = spawn(path.join(root, 'target/release/campuslingo-server'), [], {
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
      CAMPUSLINGO_ACTOR_RATE_LIMIT: '100000',
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let serverStderr = '';
  serverProc.stderr.on('data', b => {
    serverStderr = (serverStderr + b.toString()).slice(-8000);
  });
  const rl = createInterface({ input: serverProc.stdout });
  const address = await new Promise((resolve, reject) => {
    const t = setTimeout(() => reject(new Error(`server start timeout: ${serverStderr}`)), 15000);
    rl.on('line', line => {
      try {
        const ev = JSON.parse(line);
        if (ev.event === 'listening') {
          clearTimeout(t);
          resolve(ev.address);
        }
      } catch {
        // ignore request logs
      }
    });
    serverProc.once('exit', code => {
      clearTimeout(t);
      reject(new Error(`server exited ${code}: ${serverStderr}`));
    });
  });

  const [srvHost, srvPortStr] = address.split(':');
  const srvPort = Number(srvPortStr);
  const baseUrl = `http://${address}`;
  const idleServerRss = await rssOf(serverProc.pid);

  async function runDuelWave(waveLabel, backgroundTask = null) {
    const httpLatencies = [];
    const wsHeartbeatLatencies = [];
    const wsStrikeLatencies = [];
    const deadlineDriftMs = [];
    const errors = [];
    let peakRss = await rssOf(serverProc.pid);
    let sampling = true;
    const sampler = (async () => {
      while (sampling) {
        const rss = await rssOf(serverProc.pid);
        if (rss > peakRss) peakRss = rss;
        await new Promise(r => setTimeout(r, 80));
      }
    })();

    const waveStart = performance.now();
    const bgPromise = backgroundTask ? backgroundTask() : Promise.resolve(null);

    // 1. Matchmake all 30 players concurrently into 15 HUMAN 1v1 live duels
    const matchResults = await Promise.all(
      learners.map(async learner => {
        const t0 = performance.now();
        const resp = await fetch(`${baseUrl}/v1/courses/${courseId}/duels/matchmake`, {
          method: 'POST',
          headers: {
            'content-type': 'application/json',
            cookie: `campuslingo_session=${learner.token}`,
            origin: baseUrl,
          },
          body: JSON.stringify({ wait_budget_ms: 4000, entry_cost_gems: 0 }),
        });
        httpLatencies.push(performance.now() - t0);
        if (resp.status !== 201) {
          errors.push(`${waveLabel}: matchmake HTTP ${resp.status}`);
          return null;
        }
        const json = await resp.json();
        return { learner, ...json };
      }),
    );

    const byMatch = new Map();
    for (const item of matchResults) {
      if (!item) continue;
      assert.equal(item.opponent_kind, 'HUMAN', `Expected HUMAN pairing, got ${item.opponent_kind}`);
      const pair = byMatch.get(item.match_id) ?? [];
      pair.push(item);
      byMatch.set(item.match_id, pair);
    }
    assert.equal(byMatch.size, DUEL_PAIRS, `${waveLabel}: expected ${DUEL_PAIRS} live duels, got ${byMatch.size}`);

    // 2. Concurrent HTTP read background + 30 WebSocket clients fighting 15 simultaneous duels
    await Promise.all(
      [...byMatch.entries()].map(async ([matchId, [p1, p2]], pairIdx) => {
        const ws1 = await connectWs(srvHost, srvPort, `/v1/duels/ws?match_id=${matchId}`, {
          Cookie: `campuslingo_session=${p1.learner.token}`,
          Origin: baseUrl,
        });
        const ws2 = await connectWs(srvHost, srvPort, `/v1/duels/ws?match_id=${matchId}`, {
          Cookie: `campuslingo_session=${p2.learner.token}`,
          Origin: baseUrl,
        });

        try {
          await ws1.waitFor(m => m.type === 'MATCH_STATE');
          await ws2.waitFor(m => m.type === 'MATCH_STATE');

          // Heartbeat latency probe
          for (const ws of [ws1, ws2]) {
            const hbT0 = performance.now();
            ws.sendJson({ type: 'HEARTBEAT', client_ts_ms: Date.now() });
            await ws.waitFor(m => m.type === 'HEARTBEAT_ACK');
            wsHeartbeatLatencies.push(performance.now() - hbT0);
          }

          // Drive 3 synchronized combat rounds + concurrent HTTP outbox read + final settlement
          const hand1 = p1.arena.self.hand;
          const hand2 = p2.arena.self.hand;
          const side1 = p1.side;
          const side2 = p2.side;
          for (let round = 1; round <= 3; round++) {
            const seq = round;
            const c1 = hand1[round % hand1.length].card_id;
            const c2 = hand2[round % hand2.length].card_id;
            ws1.sendJson({ type: 'START_ROUND', sequence: seq });
            ws2.sendJson({ type: 'START_ROUND', sequence: seq });

            const strikeT0 = performance.now();
            ws1.sendJson({
              type: 'STRIKE',
              sequence: seq,
              lane: round % 2,
              card_id: c1,
              selected_index: 1,
              client_elapsed_ms: 650,
            });
            await ws1.waitFor(m => m.type === 'STRIKE_RESOLVED' && m.payload?.strike?.attacker === side1);
            wsStrikeLatencies.push(performance.now() - strikeT0);

            const strike2T0 = performance.now();
            ws2.sendJson({
              type: 'STRIKE',
              sequence: seq,
              lane: (round + 1) % 2,
              card_id: c2,
              selected_index: round === 3 ? 0 : 1,
              client_elapsed_ms: 700,
            });
            await ws2.waitFor(m => m.type === 'STRIKE_RESOLVED' && m.payload?.strike?.attacker === side2);
            wsStrikeLatencies.push(performance.now() - strike2T0);

            // Advance simulated duel clock + measure server tick response time as deadline drift bound
            const tickT0 = performance.now();
            ws1.sendJson({ type: 'TICK_ADVANCE', delta_ms: 25_000 });
            await ws1.waitFor(m => m.type === 'MATCH_STATE');
            deadlineDriftMs.push(performance.now() - tickT0);

            // Concurrent HTTP status + notification poll
            if (round === 1) {
              const h0 = performance.now();
              const res = await fetch(`${baseUrl}/v1/notifications/outbox`, {
                headers: { cookie: `campuslingo_session=${p1.learner.token}` },
              });
              httpLatencies.push(performance.now() - h0);
              if (res.status !== 200) errors.push(`${waveLabel}: outbox HTTP ${res.status}`);
            }
          }

          // Advance remaining simulated clock past 90s to force authoritative settlement
          ws1.sendJson({ type: 'TICK_ADVANCE', delta_ms: 20_000 });
          const finalMsg = await ws1.waitFor(
            m => m.type === 'MATCH_STATE' && m.payload?.status === 'COMPLETED',
            8000,
          );
          assert.equal(finalMsg.payload.status, 'COMPLETED', `duel ${pairIdx} did not settle COMPLETED`);

          const statusT0 = performance.now();
          const sumRes = await fetch(`${baseUrl}/v1/courses/${courseId}/duels/${matchId}`, {
            headers: { cookie: `campuslingo_session=${p1.learner.token}` },
          });
          httpLatencies.push(performance.now() - statusT0);
          if (sumRes.status !== 200) errors.push(`${waveLabel}: duel status HTTP ${sumRes.status}`);
        } finally {
          ws1.close();
          ws2.close();
        }
      }),
    );

    const backgroundResult = await bgPromise;

    sampling = false;
    await sampler;
    const waveMs = Number((performance.now() - waveStart).toFixed(2));
    assert.equal(errors.length, 0, `${waveLabel} errors: ${errors.join('; ')}`);
    assert.ok(peakRss < 50 * 1024 * 1024, `${waveLabel} server peak RSS ${peakRss} >= 50 MiB`);

    return {
      label: waveLabel,
      players: PLAYERS,
      simultaneous_duels: byMatch.size,
      settled_duels: DUEL_PAIRS,
      duration_ms: waveMs,
      error_count: errors.length,
      http_latency_ms: summarize(httpLatencies),
      ws_heartbeat_latency_ms: summarize(wsHeartbeatLatencies),
      ws_strike_roundtrip_ms: summarize(wsStrikeLatencies),
      round_deadline_response_ms: summarize(deadlineDriftMs),
      rust_server_idle_rss_bytes: idleServerRss,
      rust_server_peak_rss_bytes: peakRss,
      rust_server_rss_under_50mib: peakRss < 50 * 1024 * 1024,
      background_ingestion: backgroundResult,
    };
  }

  // Run 1: Standalone 30 players / 15 simultaneous live duels
  const run1 = await runDuelWave('run1_standalone');

  // Run 2: Concurrent 30 players / 15 simultaneous live duels + sequential background Piper Arabic TTS under ModelLease
  const run2 = await runDuelWave('run2_concurrent_with_modellease_tts', () => new Promise(resolve => {
    const t0 = performance.now();
    const child = spawn(
      process.env.CARGO ?? 'cargo',
      [
        'test',
        '--offline',
        '--locked',
        '-p',
        'campuslingo-ingestion',
        '--features',
        'onnx',
        '--lib',
        'contained_piper_ar_clause_concat_and_exclusive_model_lease',
        '--',
        '--ignored',
        '--nocapture',
      ],
      { cwd: root, stdio: ['ignore', 'pipe', 'pipe'], env: process.env },
    );
    let stderrTail = '';
    child.stderr.on('data', d => {
      stderrTail = (stderrTail + d.toString()).slice(-2000);
    });
    child.once('close', code => {
      resolve({
        exit_code: code,
        duration_ms: Number((performance.now() - t0).toFixed(2)),
        passed: code === 0,
        stderr_tail: code === 0 ? null : stderrTail,
      });
    });
  }));

  assert.equal(run2.background_ingestion?.passed, true, 'Background ModelLease TTS worker failed');

  const readinessResp = await fetch(`${baseUrl}/readyz`);
  assert.equal(readinessResp.status, 200);
  const cgroup = await readCgroupMetrics();
  if (cgroup.memory_swap_current_bytes !== null) {
    assert.equal(cgroup.memory_swap_current_bytes, 0, 'Swap must stay 0B');
  }

  const report = {
    observed_at: new Date().toISOString(),
    milestone: 'M7',
    gate: 'O-06',
    run1_standalone: run1,
    run2_concurrent_with_ingestion: run2,
    readiness_http_after_waves: readinessResp.status,
    cgroup,
    status: 'PASSED_O06_30P_15DUELS_STANDALONE_AND_CONCURRENT',
  };

  await writeFile(outPath, JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify(report, null, 2));
} finally {
  if (serverProc && serverProc.exitCode === null) {
    serverProc.kill('SIGTERM');
    await new Promise(r => setTimeout(r, 800));
    if (serverProc.exitCode === null) serverProc.kill('SIGKILL');
  }
  if (pgStarted) {
    spawnSync(path.join(pgBin, 'pg_ctl'), ['-D', data, '-m', 'immediate', 'stop'], { cwd: root, stdio: 'inherit' });
  }
  await rm(work, { recursive: true, force: true });
}
