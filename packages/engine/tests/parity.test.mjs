import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { initSync, evaluateJson } from '../generated/campuslingo_domain.js';

const root = new URL('../../../', import.meta.url);
const data = JSON.parse(readFileSync(new URL('tests/domain_vectors.json', root), 'utf8'));
initSync({ module: readFileSync(new URL('../generated/campuslingo_domain_bg.wasm', import.meta.url)) });

const memoryFloatFields = new Set(['stability', 'difficulty', 'interval_days', 'retrievability_before', 'retrievability']);
function compare(native, wasm, tolerance, relative, memory, path = '', errors = [], strictDiagnostics = []) {
  if (typeof native === 'number' && typeof wasm === 'number') {
    const error = Math.abs(native - wasm);
    const memoryFloat = memory && memoryFloatFields.has(path.split('.').at(-1));
    if (memoryFloat && error > tolerance) strictDiagnostics.push({ path, native, wasm, absolute_error: error });
    const allowed = memoryFloat ? tolerance + relative * Math.abs(native) : 0;
    if (error > allowed) errors.push({ path, native, wasm, absolute_error: error, allowed_error: allowed });
  } else if (native && wasm && typeof native === 'object' && typeof wasm === 'object') {
    assert.deepEqual(Object.keys(native).sort(), Object.keys(wasm).sort(), path);
    for (const key of Object.keys(native)) compare(native[key], wasm[key], tolerance, relative, memory, `${path}.${key}`, errors, strictDiagnostics);
  } else assert.deepEqual(native, wasm, path);
  return errors;
}

test('native Rust and browser-compatible WASM share FSRS-6, sandbox, and combat behavior', () => {
  const input = data.requests.map(request => JSON.stringify(request)).join('\n') + '\n';
  const lines = execFileSync(fileURLToPath(new URL('target/release/domain-eval', root)), [], { input, encoding: 'utf8', maxBuffer: 8 * 1024 * 1024 }).trim().split('\n');
  assert.equal(lines.length, data.requests.length);
  const differences = [];
  const strictDiagnostics = [];
  for (let index = 0; index < lines.length; index++) {
    const request = data.requests[index];
    compare(JSON.parse(lines[index]), JSON.parse(evaluateJson(JSON.stringify(request))), data.absolute_tolerance, data.relative_tolerance, ['review', 'shield'].includes(request.kind), `vector[${index}]`, differences, strictDiagnostics);
  }
  const report = { observed_at: new Date().toISOString(), algorithm: 'FSRS-6', rust_crate: 'fsrs/7.0.0 explicitly using FSRS6_DEFAULT_PARAMETERS', precision: 'upstream f32 memory; f64 sandbox arithmetic', precision_policy: data.precision_policy, exact_discrete_outcomes: true, vectors: lines.length, absolute_tolerance: data.absolute_tolerance, relative_tolerance: data.relative_tolerance, difference_count: differences.length, differences, strict_absolute_diagnostic: { status: strictDiagnostics.length ? 'exceeds_absolute_only_tolerance' : 'within_absolute_only_tolerance', difference_count: strictDiagnostics.length, differences: strictDiagnostics }, status: differences.length === 0 ? 'passed' : 'failed' };
  mkdirSync(new URL('artifacts/benchmarks/', root), { recursive: true });
  writeFileSync(new URL('artifacts/benchmarks/native-wasm-parity.json', root), JSON.stringify(report, null, 2) + '\n');
  assert.deepEqual(differences, [], 'Drift exceeds the approved memory policy or changes exact outcomes; inspect artifacts/benchmarks/native-wasm-parity.json');
});

test('WASM rejects oversized and invalid untrusted requests', () => {
  assert.equal(JSON.parse(evaluateJson(' '.repeat(32769))).kind, 'error');
  assert.equal(JSON.parse(evaluateJson('{"kind":"exec"}')).kind, 'error');
});
