import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { evaluateJson, initSync } from '../generated/campuslingo_domain.js';

const root = new URL('../../../', import.meta.url);
const wasmPath = fileURLToPath(new URL('packages/engine/generated/campuslingo_domain_bg.wasm', root));
const nativeBinPath = fileURLToPath(new URL('target/release/domain-eval', root));
const reportPath = fileURLToPath(new URL('artifacts/benchmarks/g4-sequential-chains.json', root));

const wasmBytes = readFileSync(wasmPath);
initSync({ module: wasmBytes });

const CUSTOM_S19_WEIGHTS = [
  0.254, 1.38, 2.52, 8.95, 6.21, 0.89, 2.85, 0.0025, 1.91, 0.158, 0.82,
  1.51, 0.058, 0.28, 1.59, 0.58, 1.94, 0.51, 0.088, 0.071, 0.21,
];

function evalNative(request) {
  const stdout = execFileSync(nativeBinPath, {
    input: JSON.stringify(request),
    encoding: 'utf8',
    maxBuffer: 1024 * 1024,
  });
  return JSON.parse(stdout);
}

function evalWasm(request) {
  return JSON.parse(evaluateJson(JSON.stringify(request)));
}

function withinPrecisionPolicy(reference, candidate) {
  const absError = Math.abs(reference - candidate);
  const limit = 1e-6 + 1e-6 * Math.abs(reference);
  return { absError, limit, ok: Number.isFinite(candidate) && absError <= limit };
}

function readFsrsCrateVersion() {
  const cargoLock = readFileSync(new URL('Cargo.lock', root), 'utf8');
  const match = cargoLock.match(/name = "fsrs"\nversion = "([^"]+)"/);
  return match ? match[1] : 'unknown';
}

test('G-4: 60-step sequential FSRS-6 review chains match across native and WASM (self-evolving + server-canonical)', () => {
  const chains = [
    {
      id: 'steady_progression_default',
      desired_retention: 0.9,
      parameters: null,
      decay: null,
      steps: Array.from({ length: 60 }, (_, step) => ({
        elapsed_days: step === 0 ? 0 : [1, 2, 4, 7, 14, 28, 45][step % 7],
        rating: [3, 3, 4, 3, 3, 4][step % 6],
      })),
    },
    {
      id: 'lapses_and_recovery_default',
      desired_retention: 0.9,
      parameters: null,
      decay: null,
      steps: Array.from({ length: 60 }, (_, step) => ({
        elapsed_days: step === 0 ? 0 : [1, 1, 2, 5, 1, 3, 8][step % 7],
        rating: step === 0 ? 3 : [3, 1, 2, 3, 1, 3, 4][step % 7],
      })),
    },
    {
      id: 'high_retention_mixed_intervals',
      desired_retention: 0.95,
      parameters: null,
      decay: null,
      steps: Array.from({ length: 60 }, (_, step) => ({
        elapsed_days: step === 0 ? 0 : [1, 1, 2, 3, 5, 9, 15, 30][step % 8],
        rating: step === 0 ? 3 : [2, 3, 3, 4, 2, 3, 1, 4][step % 8],
      })),
    },
    {
      id: 'custom_s19_21_weights',
      desired_retention: 0.9,
      parameters: CUSTOM_S19_WEIGHTS,
      decay: CUSTOM_S19_WEIGHTS[20],
      steps: Array.from({ length: 60 }, (_, step) => ({
        elapsed_days: step === 0 ? 0 : [1, 2, 3, 6, 1, 2, 7, 14][step % 8],
        rating: step === 0 ? 3 : [3, 2, 3, 4, 1, 3, 3, 4][step % 8],
      })),
    },
  ];

  const metrics = ['stability', 'difficulty', 'interval_days', 'retrievability_before'];
  const summaryChains = [];
  let globalMaxAbsError = 0;
  let globalMaxShieldAbsError = 0;
  let globalScheduledDaysDeltas = 0;
  let globalReviewsEvaluated = 0;

  for (const chain of chains) {
    let nativeMemory = null;
    let wasmSelfMemory = null;
    let chainMaxAbsError = 0;
    let chainMaxShieldAbsError = 0;
    let selfEvolvingMaxAbsError = 0;
    let canonicalAdvisoryMaxAbsError = 0;
    let scheduledDaysDeltas = 0;

    for (let stepIdx = 0; stepIdx < chain.steps.length; stepIdx += 1) {
      const { elapsed_days, rating } = chain.steps[stepIdx];
      const baseInput = {
        elapsed_days,
        rating,
        desired_retention: chain.desired_retention,
        ...(chain.parameters ? { parameters: chain.parameters } : {}),
      };

      const nativeRes = evalNative({
        kind: 'review',
        input: { ...baseInput, memory: nativeMemory },
      });
      const wasmSelfRes = evalWasm({
        kind: 'review',
        input: { ...baseInput, memory: wasmSelfMemory },
      });
      const wasmAdvisoryRes = evalWasm({
        kind: 'review',
        input: { ...baseInput, memory: nativeMemory },
      });

      assert.equal(nativeRes.kind, 'review');
      assert.equal(wasmSelfRes.kind, 'review');
      assert.equal(wasmAdvisoryRes.kind, 'review');
      assert.equal(nativeRes.result.algorithm, 'FSRS-6');
      assert.equal(wasmSelfRes.result.algorithm, 'FSRS-6');
      assert.equal(wasmAdvisoryRes.result.algorithm, 'FSRS-6');

      for (const field of metrics) {
        const ref = nativeRes.result[field];
        const selfCheck = withinPrecisionPolicy(ref, wasmSelfRes.result[field]);
        const advCheck = withinPrecisionPolicy(ref, wasmAdvisoryRes.result[field]);
        assert.ok(
          selfCheck.ok,
          `${chain.id} step ${stepIdx} self-evolving ${field}: abs=${selfCheck.absError} limit=${selfCheck.limit}`,
        );
        assert.ok(
          advCheck.ok,
          `${chain.id} step ${stepIdx} server-canonical advisory ${field}: abs=${advCheck.absError} limit=${advCheck.limit}`,
        );
        selfEvolvingMaxAbsError = Math.max(selfEvolvingMaxAbsError, selfCheck.absError);
        canonicalAdvisoryMaxAbsError = Math.max(canonicalAdvisoryMaxAbsError, advCheck.absError);
        chainMaxAbsError = Math.max(chainMaxAbsError, selfCheck.absError, advCheck.absError);
      }

      if (nativeRes.result.scheduled_days !== wasmSelfRes.result.scheduled_days) {
        scheduledDaysDeltas += 1;
      }
      assert.equal(
        wasmAdvisoryRes.result.scheduled_days,
        nativeRes.result.scheduled_days,
        `${chain.id} step ${stepIdx}: server-canonical input advisory scheduled_days drifted`,
      );

      // Continuous shield curve check with fractional elapsed time
      const fractionalElapsed = 0.25 + (stepIdx % 11) * 0.375;
      const nativeShield = evalNative({
        kind: 'shield',
        input: {
          stability: nativeRes.result.stability,
          elapsed_days: fractionalElapsed,
          ...(chain.decay !== null ? { decay: chain.decay } : {}),
        },
      });
      const wasmShield = evalWasm({
        kind: 'shield',
        input: {
          stability: wasmSelfRes.result.stability,
          elapsed_days: fractionalElapsed,
          ...(chain.decay !== null ? { decay: chain.decay } : {}),
        },
      });
      assert.equal(nativeShield.kind, 'shield');
      assert.equal(wasmShield.kind, 'shield');
      const shieldCheck = withinPrecisionPolicy(nativeShield.retrievability, wasmShield.retrievability);
      assert.ok(
        shieldCheck.ok,
        `${chain.id} step ${stepIdx} continuous shield: abs=${shieldCheck.absError} limit=${shieldCheck.limit}`,
      );
      chainMaxShieldAbsError = Math.max(chainMaxShieldAbsError, shieldCheck.absError);

      nativeMemory = {
        stability: nativeRes.result.stability,
        difficulty: nativeRes.result.difficulty,
      };
      wasmSelfMemory = {
        stability: wasmSelfRes.result.stability,
        difficulty: wasmSelfRes.result.difficulty,
      };
      globalReviewsEvaluated += 1;
    }

    globalMaxAbsError = Math.max(globalMaxAbsError, chainMaxAbsError);
    globalMaxShieldAbsError = Math.max(globalMaxShieldAbsError, chainMaxShieldAbsError);
    globalScheduledDaysDeltas += scheduledDaysDeltas;
    assert.ok(chain.steps.length >= 50, 'G-4 requires 50+ step chains');
    summaryChains.push({
      id: chain.id,
      steps: chain.steps.length,
      desired_retention: chain.desired_retention,
      custom_s19_weights: Boolean(chain.parameters),
      decay: chain.decay ?? 0.1542,
      self_evolving_max_abs_error: selfEvolvingMaxAbsError,
      server_canonical_advisory_max_abs_error: canonicalAdvisoryMaxAbsError,
      continuous_shield_max_abs_error: chainMaxShieldAbsError,
      self_evolving_scheduled_days_deltas: scheduledDaysDeltas,
      final_stability: nativeMemory.stability,
      final_difficulty: nativeMemory.difficulty,
    });
  }

  assert.ok(globalReviewsEvaluated >= 200, 'Expected at least 4 chains x 50+ steps');

  const report = {
    schema_version: '1.0.0',
    gate: 'G-4',
    status: 'passed',
    generated_at: new Date().toISOString(),
    provenance: {
      node_version: process.version,
      platform: process.platform,
      arch: process.arch,
      fsrs_algorithm: 'FSRS-6',
      fsrs_crate_version: readFsrsCrateVersion(),
      wasm_sha256: createHash('sha256').update(wasmBytes).digest('hex'),
      native_binary: 'target/release/domain-eval',
    },
    precision_policy: {
      tolerance: 'abs_error <= 1e-6 + 1e-6 * abs(reference)',
      schedule_days_authority: 'server_canonical',
      wasm_role: 'advisory_preview',
      custom_s19_21_weights_verified: true,
    },
    totals: {
      chains: summaryChains.length,
      steps_per_chain: 60,
      reviews_per_mode: globalReviewsEvaluated,
      max_abs_error: globalMaxAbsError,
      max_continuous_shield_abs_error: globalMaxShieldAbsError,
      self_evolving_scheduled_days_deltas: globalScheduledDaysDeltas,
    },
    chains: summaryChains,
  };

  mkdirSync(dirname(reportPath), { recursive: true });
  writeFileSync(reportPath, `${JSON.stringify(report, null, 2)}\n`);
});
