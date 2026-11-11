/** Gather M0 evidence without treating a known failing gate as success. */
import { spawnSync } from 'node:child_process';
import { mkdirSync, openSync, closeSync, readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const directory = `${root}/artifacts/private/checks`;
mkdirSync(directory, { recursive: true, mode: 0o700 });
const checks = [];
const reuseProbes = process.argv.includes('--reuse-probes');
function run(name, command, args) {
  const path = `${directory}/${name}.log`;
  const descriptor = openSync(path, 'w', 0o600);
  const result = spawnSync(command, args, { cwd: root, stdio: ['ignore', descriptor, descriptor], timeout: 600000 });
  closeSync(descriptor);
  const check = { name, command: [command, ...args], exit_code: result.status, status: result.status === 0 ? 'passed' : 'failed', log: path };
  checks.push(check);
  console.log(`${name}: ${check.status}`);
  if (result.status !== 0) console.log(readFileSync(path, 'utf8').slice(-3000));
  return result.status === 0;
}

const built = run('build-native-wasm-contracts', 'npm', ['run', 'm0:build']);
if (built) {
  run('rust-unit-tests', 'cargo', ['test', '--workspace', '--locked']);
  run('rust-clippy', 'cargo', ['clippy', '--workspace', '--all-targets', '--all-features', '--locked', '--', '-D', 'warnings']);
  run('contract-drift', 'npm', ['run', 'contracts:check']);
  run('typescript-typecheck', 'npm', ['run', 'typecheck']);
  run('typescript-contract-tests', 'npm', ['run', 'test:ts']);
  run('approved-native-wasm-parity', 'npm', ['run', 'test:wasm']);
  run('offline-reference-tests', 'uv', ['run', 'pytest']);
  run('offline-python-lint', 'uv', ['run', 'ruff', 'check', 'scripts', 'tests', 'tools/eval/python']);
  if (!reuseProbes) {
    run('rust-resource-model-onnx-probes', 'node', ['scripts/probe_rust.mjs']);
    run('environment', 'npm', ['run', 'm0:environment']);
    run('model-audit', 'npm', ['run', 'm0:audit']);
  }
}
function evidence(relative) {
  try { return JSON.parse(readFileSync(`${root}/${relative}`, 'utf8')); } catch { return null; }
}
const generation = evidence('artifacts/private/benchmark-generator.json');
const generationTrials = generation?.trials?.filter(trial => trial.phase === 'generator') ?? [];
const firstPass = trial => trial.validation?.sandbox?.valid === true && trial.validation?.source_constraints_match === true;
const repairedPass = trial => trial.repair?.validation?.sandbox?.valid === true && trial.repair?.validation?.source_constraints_match === true;
const generationPassed = generationTrials.length > 0 && generationTrials.every(trial => firstPass(trial) || repairedPass(trial));
const report = {
  observed_at: new Date().toISOString(), milestone: 'M0',
  architecture: 'Rust/WASM; Python offline evaluation only', status: 'not_complete', checks,
  reused_existing_resource_probes: reuseProbes,
  synthetic_generation: {
    status: generationPassed ? 'passed_synthetic_only' : 'not_passed',
    trials: generationTrials.length,
    first_pass_valid: generationTrials.filter(firstPass).length,
    repaired_valid: generationTrials.filter(trial => !firstPass(trial) && repairedPass(trial)).length,
    warning: 'Tiny seeded capability probe, not educational correctness or a held-out quality rate.',
  },
  outstanding_gates: [
    ...(!generationPassed ? ['The generator smoke probe did not pass executable/source-grounded validation.'] : []),
    'OCR, embedding, actual TTS/alignment and GLB-model runtime validation are not established by repository metadata.',
    'Full license/provenance review and a representative routing evaluation are not replaced by metadata and three synthetic labels.',
  ],
  documented_later_dependencies: [
    'Local M8 training runtime/fit is unproven; the environment report records this rather than silently adopting cloud training.',
    '30 validation sockets are not the M5/M7 gate of 15 complete 90-second duels with database/cache contention.',
    'Physical Android and iOS Safari/PWA acceptance belongs to later delivery gates.',
  ],
};
mkdirSync(`${root}/artifacts/benchmarks`, { recursive: true });
writeFileSync(`${root}/artifacts/benchmarks/m0-status.json`, JSON.stringify(report, null, 2) + '\n');
console.log('Evidence saved to artifacts/benchmarks/m0-status.json; M0 remains gated.');
if (checks.some(check => check.status === 'failed')) process.exitCode = 1;
