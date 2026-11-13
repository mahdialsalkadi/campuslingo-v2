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

test('G-6b rating_map_version = 2 native and WASM parity across all precedence tiers', () => {
  const cases = [
    { termination: 'VOID_DELIVERY', is_correct: true, raw_elapsed_ms: 1000, l_net_ms: 900, prior_void_transport_count: 3, prior_too_fast_count: 0 },
    { termination: 'VOID_TRANSPORT_LOSS', is_correct: false, raw_elapsed_ms: 600, l_net_ms: 600, prior_void_transport_count: 1, prior_too_fast_count: 0 },
    { termination: 'VOID_TRANSPORT_LOSS', is_correct: false, raw_elapsed_ms: 600, l_net_ms: 600, prior_void_transport_count: 2, prior_too_fast_count: 0 },
    { termination: 'SUBMITTED', is_correct: true, raw_elapsed_ms: 140, l_net_ms: 120, prior_void_transport_count: 0, prior_too_fast_count: 0 },
    { termination: 'SUBMITTED', is_correct: true, raw_elapsed_ms: 140, l_net_ms: 120, prior_void_transport_count: 0, prior_too_fast_count: 2 },
    { termination: 'TIMEOUT', is_correct: true, raw_elapsed_ms: 9000, l_net_ms: 8800, prior_void_transport_count: 0, prior_too_fast_count: 0 },
    { termination: 'ABANDONED_AFTER_REVEAL', is_correct: true, raw_elapsed_ms: 1200, l_net_ms: 1200, prior_void_transport_count: 0, prior_too_fast_count: 0 },
    { termination: 'SUBMITTED', is_correct: false, raw_elapsed_ms: 2000, l_net_ms: 1900, prior_void_transport_count: 0, prior_too_fast_count: 0 },
    { termination: 'SUBMITTED', is_correct: true, hint_used: true, raw_elapsed_ms: 1500, l_net_ms: 1400, prior_void_transport_count: 0, prior_too_fast_count: 0 },
    { termination: 'SUBMITTED', is_correct: true, server_sandbox_submits: 2, raw_elapsed_ms: 1500, l_net_ms: 1400, prior_void_transport_count: 0, prior_too_fast_count: 0 },
    { termination: 'SUBMITTED', is_correct: true, partial_credit: true, raw_elapsed_ms: 1500, l_net_ms: 1400, prior_void_transport_count: 0, prior_too_fast_count: 0 },
    { termination: 'SUBMITTED', is_correct: true, raw_elapsed_ms: 8500, l_net_ms: 8100, prior_void_transport_count: 0, prior_too_fast_count: 0 },
    { termination: 'SUBMITTED', is_correct: true, raw_elapsed_ms: 3500, l_net_ms: 3400, prior_void_transport_count: 0, prior_too_fast_count: 0 },
    { termination: 'SUBMITTED', is_correct: true, raw_elapsed_ms: 5200, l_net_ms: 5000, prior_void_transport_count: 0, prior_too_fast_count: 0 },
  ];

  for (const c of cases) {
    const out = runBoth({
      kind: 'rating_map_v2',
      input: {
        termination: c.termination,
        is_correct: c.is_correct,
        partial_credit: c.partial_credit ?? false,
        hint_used: c.hint_used ?? false,
        server_sandbox_submits: c.server_sandbox_submits ?? 1,
        raw_elapsed_ms: c.raw_elapsed_ms,
        l_net_ms: c.l_net_ms,
        w_personal_eff_ms: 8000,
        w_round_cut_ms: 9600,
        prior_void_transport_count: c.prior_void_transport_count,
        prior_too_fast_count: c.prior_too_fast_count,
        is_live_duel: true,
      },
    });
    assert.equal(out.kind, 'rating_map_v2');
    assert.equal(out.result.rating_map_version, 2);
  }
});

test('G-6b score_v2 exact i128 native and WASM parity with 3.0x clamp and ties-even rounding', () => {
  const vectors = [
    { base_points: 10, rating: 1, recall_bps: 10000, mastery_bps: 10000, mode_bps: 10000, recovery_x1e4: 10000 },
    { base_points: 10, rating: 2, recall_bps: 10000, mastery_bps: 10000, mode_bps: 10000, recovery_x1e4: 10000 },
    { base_points: 10, rating: 3, recall_bps: 12000, mastery_bps: 11000, mode_bps: 12500, recovery_x1e4: 10000 },
    { base_points: 10, rating: 4, recall_bps: 15000, mastery_bps: 15000, mode_bps: 15000, recovery_x1e4: 15000 },
  ];
  for (const v of vectors) {
    const out = runBoth({ kind: 'score_v2', input: v });
    assert.equal(out.kind, 'score_v2');
    assert.equal(out.result.score_version, 2);
  }
});

test('G-6b duel_strike native and WASM bit-identical state transition', () => {
  const arena = {
    protocol_version: 2,
    match_clock_ms: 12000,
    sudden_death_elapsed_ms: 0,
    status: 'IN_PROGRESS',
    winner: null,
    player_a: {
      side: 'PLAYER_A',
      energy_milli: 7000,
      tower_hp: [1800, 1800],
      tower_shield: [300, 300],
      core_hp: 3000,
      accommodation_num: 1,
      accommodation_den: 1,
      combat_level: 11,
      void_transport_count: 0,
      too_fast_count: 0,
      overcharge_count: 0,
      counter_strike_until_ms: null,
      quick_blitz_chain_available: false,
      last_sequence: 0,
      hand: [{ card_id: 'a-1', archetype: 'AXIOM_PACK', modifier: 'STANDARD', card_cost: 3, base_window_ms: 9000 }],
      selected_card_id: null,
      selection_started_at_ms: null,
    },
    player_b: {
      side: 'PLAYER_B',
      energy_milli: 7000,
      tower_hp: [1800, 1800],
      tower_shield: [300, 300],
      core_hp: 3000,
      accommodation_num: 5,
      accommodation_den: 4,
      combat_level: 11,
      void_transport_count: 0,
      too_fast_count: 0,
      overcharge_count: 0,
      counter_strike_until_ms: null,
      quick_blitz_chain_available: false,
      last_sequence: 0,
      hand: [{ card_id: 'b-1', archetype: 'AXIOM_PACK', modifier: 'STANDARD', card_cost: 2, base_window_ms: 8000 }],
      selected_card_id: null,
      selection_started_at_ms: null,
    },
  };

  const out = runBoth({
    kind: 'duel_strike',
    arena,
    strike: {
      attacker: 'PLAYER_A',
      sequence: 1,
      lane: 0,
      card_id: 'a-1',
      use_quick_blitz_chain: false,
      termination: 'SUBMITTED',
      is_correct: true,
      partial_credit: false,
      hint_used: false,
      server_sandbox_submits: 1,
      raw_elapsed_ms: 3000,
      rtt_compensation_ms: 100,
      opponent_card_base_ms: 8000,
    },
  });
  assert.equal(out.kind, 'duel_strike');
  assert.equal(out.result.overcharged, true);
  assert.equal(out.result.damage_to_shield, 0);
  assert.equal(out.result.damage_to_tower, 720);
  assert.equal(out.arena.player_b.tower_hp[0], 1080);
});
