import { readFile, writeFile } from 'node:fs/promises';

const requests = [];
for (const stability of [null, 0.05, 1, 5, 21, 365]) {
  for (const elapsed of [0, 1, 5, 21, 365, 36500]) {
    for (const rating of [1, 2, 3, 4]) {
      requests.push({ kind: 'review', input: { memory: stability === null ? null : { stability, difficulty: 5 }, elapsed_days: elapsed, rating, desired_retention: 0.9 } });
    }
  }
}
for (const stability of [0, 0.05, 1, 5, 21, 365]) {
  for (const elapsed of [0, 0.001, 0.5, 1, 21, 365]) {
    requests.push({ kind: 'shield', input: { stability, elapsed_days: elapsed } });
  }
}
const sandbox = JSON.parse(await readFile(new URL('../tests/fixtures/sandbox.valid.json', import.meta.url), 'utf8'));
requests.push({ kind: 'sandbox', spec: sandbox });
const impossible = structuredClone(sandbox); impossible.targets[0].lower = 13; impossible.targets[0].upper = 13;
requests.push({ kind: 'sandbox', spec: impossible });
for (const changes of [{}, { energy_milli: 2000 }, { lane: 2 }, { sequence: 0 }, { remaining_ms: 0 }]) {
  requests.push({ kind: 'deploy', input: { energy_milli: 5000, card_cost: 3, lane: 0, sequence: 1, last_sequence: 0, remaining_ms: 90000, ...changes } });
}
requests.push({ kind: 'review', input: { memory: null, elapsed_days: -1, rating: 3, desired_retention: 0.9 } });
requests.push({ kind: 'review', input: { memory: null, elapsed_days: 0, rating: 5, desired_retention: 0.9 } });
requests.push({ kind: 'exec', code: 'untrusted' });
await writeFile(new URL('../tests/domain_vectors.json', import.meta.url), JSON.stringify({ schema_version: '1.0.0', precision_policy: 'user-approved-f32-aware-v1', absolute_tolerance: 1e-6, relative_tolerance: 1e-6, exact_discrete_outcomes: true, algorithm: 'FSRS-6', requests }, null, 2) + '\n');
console.log(`Generated ${requests.length} synthetic shared-domain inputs`);
