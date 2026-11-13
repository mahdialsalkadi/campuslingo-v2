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

test('ShieldDag native and WASM agree identically across tiers, brownout, and propagation', () => {
  const now = 1_760_000_000;
  const request = {
    kind: 'shield_dag',
    input: {
      now_epoch_s: now,
      last_qualifying_study_at_epoch_s: now - 3600,
      new_player_bubble_until_epoch_s: now - 1,
      eligible_transition_count: 10,
      resume_grace_until_epoch_s: null,
      nodes: [
        {
          node_id: 'root',
          active_cards: [
            {
              card_id: 'c1',
              stability: 2.0,
              reps: 3,
              last_review_epoch_s: now - 25 * 86_400,
              version_grace_active: false,
            },
          ],
          previous_brownout: false,
          unlocked_at_epoch_s: now - 10 * 86_400,
          post_raid_cooldown_until_epoch_s: now + 3600,
          version_grace_active: false,
        },
        {
          node_id: 'leaf',
          active_cards: [
            {
              card_id: 'c2',
              stability: 25.0,
              reps: 5,
              last_review_epoch_s: now - 3600,
              version_grace_active: false,
            },
          ],
          previous_brownout: false,
          unlocked_at_epoch_s: now - 10 * 86_400,
          post_raid_cooldown_until_epoch_s: null,
          version_grace_active: false,
        },
      ],
      edges: [
        {
          parent_node_id: 'root',
          child_node_id: 'leaf',
          weight: 0.8,
        },
      ],
    },
  };

  const nativeOut = JSON.parse(
    execFileSync(nativeBin, { input: JSON.stringify(request), encoding: 'utf8' }),
  );
  const wasmOut = JSON.parse(evaluateJson(JSON.stringify(request)));
  assert.equal(nativeOut.kind, 'shield_dag');
  assert.deepEqual(wasmOut, nativeOut);
  assert.equal(wasmOut.result.nodes[0].is_brownout, true);
  assert.equal(wasmOut.result.nodes[0].protection.raid_immune, true);
  assert.equal(wasmOut.result.nodes[0].protection.propagation_exempt, false);
  assert.ok(wasmOut.result.nodes[1].e_u_ticks < wasmOut.result.nodes[1].i_u_ticks);
});
