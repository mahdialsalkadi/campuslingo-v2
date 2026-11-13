import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { evaluateJson, initSync } from '../generated/campuslingo_domain.js';

const root = new URL('../../../', import.meta.url);
const wasmPath = fileURLToPath(new URL('packages/engine/generated/campuslingo_domain_bg.wasm', root));
const nativeBin = fileURLToPath(new URL('target/release/domain-eval', root));

initSync({ module: readFileSync(wasmPath) });

function runBoth(request) {
  const payload = JSON.stringify(request);
  const nativeOut = JSON.parse(execFileSync(nativeBin, { input: payload, encoding: 'utf8' }));
  const wasmOut = JSON.parse(evaluateJson(payload));
  assert.deepEqual(wasmOut, nativeOut);
  return wasmOut;
}

test('M7 quiet hours 22:00-08:00 deferral parity across native and WASM', () => {
  const resQuiet = runBoth({
    kind: 'quiet_hours',
    local_hour: 22,
    local_minute: 30,
    local_second: 0,
  });
  assert.equal(resQuiet.kind, 'quiet_hours');
  assert.equal(resQuiet.result.in_quiet_hours, true);
  assert.equal(resQuiet.result.deferred_to_local_hour, 8);
  assert.equal(resQuiet.result.defer_seconds, 9 * 3600 + 30 * 60);

  const resActive = runBoth({
    kind: 'quiet_hours',
    local_hour: 15,
    local_minute: 0,
    local_second: 0,
  });
  assert.equal(resActive.result.in_quiet_hours, false);
  assert.equal(resActive.result.defer_seconds, 0);
});

test('M7 aggregated notification plan parity across native and WASM', () => {
  const res = runBoth({
    kind: 'notification_plan',
    prefs: {
      push_enabled: true,
      raid_alerts_enabled: true,
      shield_digest_enabled: true,
    },
    raids: [
      { raid_id: 'r-1', occurred_local_hour: 23, occurred_local_minute: 0, occurred_local_second: 0 },
      { raid_id: 'r-2', occurred_local_hour: 9, occurred_local_minute: 15, occurred_local_second: 0 },
    ],
    shield_digest: {
      expiring_node_ids: ['n-1', 'n-2'],
      occurred_local_hour: 7,
      occurred_local_minute: 30,
      occurred_local_second: 0,
    },
  });
  assert.equal(res.kind, 'notification_plan');
  assert.equal(res.result.max_push_per_day, 2);
  assert.equal(res.result.total_queued, 2);
  assert.equal(res.result.items[0].summary_text, 'Raided 2 times today');
  assert.deepEqual(res.result.items[0].raid_ids, ['r-1', 'r-2']);
  assert.equal(res.result.items[0].quiet_hours_deferred, true);
  assert.equal(res.result.items[1].summary_text, '2 shields nearing expiry');
  assert.equal(res.result.items[1].quiet_hours_deferred, true);
});
