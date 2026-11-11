/** M0 foundation benchmark, NOT a claim that M5/M7 combat is implemented. */
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { performance } from 'node:perf_hooks';

const root = new URL('../', import.meta.url);
const seconds = Number(process.env.CAMPUSLINGO_LOAD_SECONDS ?? '10');
if (!Number.isFinite(seconds) || seconds < 2 || seconds > 300) throw Error('Load duration must be 2–300 seconds');
const processHandle = spawn(fileURLToPath(new URL('target/release/campuslingo-server', root)), [], {
  cwd: fileURLToPath(root),
  env: { ...process.env, CAMPUSLINGO_BIND: '127.0.0.1:0', CAMPUSLINGO_M0_WS: '1', CAMPUSLINGO_DATABASE_URL: '' },
  stdio: ['ignore', 'pipe', 'pipe'],
});
// An empty DB URL must not accidentally select an inherited reference database.
// No connection is needed for the M0 validation workload.
let stderr = '';
processHandle.stderr.on('data', bytes => { stderr = (stderr + bytes.toString()).slice(-8000); });
const lines = createInterface({ input: processHandle.stdout });
const wait = milliseconds => new Promise(resolve => setTimeout(resolve, milliseconds));
const sockets = [];
let sampling = true;
let peakRss = 0;
let minimumHostAvailable = Infinity;
let sampler;

async function memory() {
  const status = await readFile(`/proc/${processHandle.pid}/status`, 'utf8');
  const rss = Number(status.match(/^VmRSS:\s+(\d+)/m)?.[1]) * 1024;
  const hwm = Number(status.match(/^VmHWM:\s+(\d+)/m)?.[1]) * 1024;
  peakRss = Math.max(peakRss, rss, hwm);
  const meminfo = await readFile('/proc/meminfo', 'utf8');
  minimumHostAvailable = Math.min(minimumHostAvailable, Number(meminfo.match(/^MemAvailable:\s+(\d+)/m)?.[1]) * 1024);
  return rss;
}

function connect(url) {
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(url);
    const timer = setTimeout(() => { socket.close(); reject(Error('WebSocket connect timeout')); }, 3000);
    socket.addEventListener('open', () => { clearTimeout(timer); resolve(socket); }, { once: true });
    socket.addEventListener('error', () => { clearTimeout(timer); reject(Error('WebSocket connection failed')); }, { once: true });
  });
}

function request(socket, payload) {
  return new Promise((resolve, reject) => {
    const start = performance.now();
    const onMessage = event => {
      cleanup();
      const result = JSON.parse(event.data);
      if (result.kind !== 'review' || result.result.algorithm !== 'FSRS-6') reject(Error(`Unexpected validation response: ${event.data}`));
      else resolve(performance.now() - start);
    };
    const onClose = () => { cleanup(); reject(Error('Socket closed during evaluation')); };
    const timer = setTimeout(() => { cleanup(); reject(Error('Validation exceeded 2 seconds')); }, 2000);
    function cleanup() { clearTimeout(timer); socket.removeEventListener('message', onMessage); socket.removeEventListener('close', onClose); }
    socket.addEventListener('message', onMessage, { once: true });
    socket.addEventListener('close', onClose, { once: true });
    socket.send(JSON.stringify(payload));
  });
}

try {
  const address = await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(Error(`Server startup timeout: ${stderr}`)), 10000);
    lines.on('line', line => {
      try { const data = JSON.parse(line); if (data.event === 'listening') { clearTimeout(timer); resolve(data.address); } } catch { /* only accept structured startup */ }
    });
    processHandle.once('error', error => { clearTimeout(timer); reject(error); });
    processHandle.once('exit', code => { clearTimeout(timer); reject(Error(`Server exited ${code}: ${stderr}`)); });
  });
  const idleRss = await memory();
  sampler = (async () => { while (sampling) { await memory().catch(() => {}); await wait(100); } })();
  const connected = await Promise.all(Array.from({ length: 30 }, () => connect(`ws://${address}/m0/ws`)));
  sockets.push(...connected);
  const latencies = [];
  const errors = [];
  const end = performance.now() + seconds * 1000;
  await Promise.all(sockets.map(async socket => {
    while (performance.now() < end) {
      try {
        latencies.push(await request(socket, { kind: 'review', input: { memory: { stability: 5, difficulty: 5 }, elapsed_days: 1, rating: 3, desired_retention: 0.9 } }));
      } catch (error) { errors.push(String(error)); break; }
      await wait(250);
    }
  }));
  await memory();
  const health = await (await fetch(`http://${address}/healthz`)).json();
  const readiness = await fetch(`http://${address}/readyz`);
  latencies.sort((a,b) => a-b);
  const percentile = p => latencies[Math.min(latencies.length - 1, Math.floor(latencies.length * p))] ?? null;
  const report = {
    observed_at: new Date().toISOString(), stage: 'M0 foundation only',
    workload: '30 persistent WebSockets evaluating the shared Rust FSRS-6 domain, up to 4 requests/sec/client',
    not_yet_implemented: 'auth, PostgreSQL transactions, real 90-second combat, matchmaking, and 15 complete duels remain M1/M5/M7 gates',
    duration_seconds: seconds, clients: sockets.length, successful_requests: latencies.length, errors,
    idle_rss_bytes: idleRss, peak_rss_bytes: peakRss, process_rss_target_bytes: 50 * 1024 * 1024,
    memory_target_passed: peakRss < 50 * 1024 * 1024,
    minimum_host_available_bytes: minimumHostAvailable,
    latency_ms: { p50: percentile(0.5), p95: percentile(0.95), p99: percentile(0.99), max: latencies.at(-1) ?? null },
    health, readiness_http: readiness.status,
    status: errors.length === 0 && sockets.length === 30 && latencies.length >= 30 && peakRss < 50 * 1024 * 1024 ? 'passed_foundation_probe' : 'failed',
  };
  await mkdir(new URL('artifacts/benchmarks/', root), { recursive: true });
  await writeFile(new URL('artifacts/benchmarks/server-load.json', root), JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify(report, null, 2));
  if (report.status === 'failed') process.exitCode = 1;
} finally {
  sampling = false;
  for (const socket of sockets) socket.close();
  processHandle.kill('SIGINT');
  const force = setTimeout(() => processHandle.kill('SIGKILL'), 3000); force.unref();
  if (sampler) await sampler;
  lines.close();
}
