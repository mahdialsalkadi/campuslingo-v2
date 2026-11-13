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

test('M6 raid targetability & settlement parity across native and WASM', () => {
  const targ = runBoth({
    kind: 'raid_targetability',
    input: {
      attacker_user_id: 'a',
      defender_user_id: 'b',
      course_is_private: true,
      attacker_raid_opt_in: true,
      defender_raid_opt_in: true,
      node_state: 'ACTIVE',
      node_tier: 2,
      active_protections: [],
      attacker_outbound_raids_today: 2,
      defender_inbound_raids_today: 1,
      unordered_pair_raids_today: 0,
      is_revenge: true,
      revenge_source_available: true,
      revenge_age_seconds: 1200,
    },
  });
  assert.equal(targ.result.targetable, true);
  assert.equal(targ.result.mode_bps, 15000);

  const settle = runBoth({
    kind: 'raid_settlement',
    input: {
      is_revenge: true,
      sandbox_solved: true,
      drills_passed: 3,
      elapsed_from_stage2_ms: 48000,
      defender_immune_at_settle: false,
      defender_daily_trophy_loss_so_far: 30,
      attacker_practice_xp_today: 0,
      reward_eligible: true,
      base_score_v2_points: 25,
    },
  });
  assert.equal(settle.result.status, 'SETTLED_WIN');
  assert.equal(settle.result.defender_trophy_loss, 15);
  assert.equal(settle.result.clan_points_awarded, 38);
});

test('M6 clan stability, league rollover & boss strike native and WASM parity', () => {
  const clan = runBoth({
    kind: 'clan_stability',
    input: {
      s_before: 2.0,
      s_after: 14.0,
      active_members_snapshot: 9,
      reward_eligible: true,
      member_in_frozen_roster: true,
    },
  });
  assert.equal(clan.result.raw_points, 19);
  assert.equal(clan.result.normalized_points, 6);

  const league = runBoth({
    kind: 'league_rollover',
    input: {
      tier: 2,
      late_joiner_shielded: false,
      participants: [
        {
          user_id: 'u1',
          trophies_earned: 165,
          eligible_mastery_recovery_count: 5,
          last_trophy_earned_epoch_ms: 1000,
          is_dormant: false,
          returned_from_dormancy_this_week: false,
          qualifying_sessions_since_return: 0,
          consecutive_dormant_weeks: 0,
        },
        {
          user_id: 'u2',
          trophies_earned: 149,
          eligible_mastery_recovery_count: 10,
          last_trophy_earned_epoch_ms: 500,
          is_dormant: false,
          returned_from_dormancy_this_week: false,
          qualifying_sessions_since_return: 0,
          consecutive_dormant_weeks: 0,
        },
      ],
    },
  });
  assert.equal(league.result.standings[0].outcome, 'PROMOTED');
  assert.equal(league.result.standings[1].outcome, 'HELD');

  const boss = runBoth({
    kind: 'boss_strike',
    input: {
      boss_hp_before: 150,
      base_damage: 100,
      rating: 4,
      overcharged: true,
    },
  });
  assert.equal(boss.result.applied_damage, 150);
  assert.equal(boss.result.overkill_damage, 50);
  assert.equal(boss.result.defeated, true);
});
