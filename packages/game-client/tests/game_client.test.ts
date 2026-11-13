import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { initSync, evaluateJson } from '../../engine/generated/campuslingo_domain.js';
import {
  computeParameterTargetBand,
  evaluateStateViaWasm,
  findActiveClause,
  quantizeParameterStep,
  validateTouchTargetRects,
  verifySandboxViaWasm,
} from '../src/controller.js';
import {
  guardOfflineRankedMutation,
  isSwCacheableAsset,
  renderMixedArabicMathHtml,
} from '../src/m7_pwa_rtl.js';

const sandboxFixture = JSON.parse(
  readFileSync(
    new URL('../../../tests/fixtures/sandbox.valid.json', import.meta.url),
    'utf8',
  ),
);
initSync({
  module: readFileSync(
    new URL('../../engine/generated/campuslingo_domain_bg.wasm', import.meta.url),
  ),
});

test('WASM controller drives discrete slider quantization and target band highlighting', () => {
  const verification = verifySandboxViaWasm(evaluateJson, sandboxFixture);
  assert.equal(verification.valid, true);
  assert.equal(verification.states_checked, 11);

  const param = sandboxFixture.parameters[0];
  assert.ok(param);
  assert.equal(quantizeParameterStep(param, 5.7), 6);
  assert.equal(quantizeParameterStep(param, -5), 0);

  const initEval = evaluateStateViaWasm(evaluateJson, sandboxFixture, {
    [param.name]: param.initial,
  });
  assert.equal(initEval.solved, false);

  const solvedEval = evaluateStateViaWasm(evaluateJson, sandboxFixture, {
    [param.name]: 6,
  });
  assert.equal(solvedEval.solved, true);
  assert.equal(solvedEval.targets[0]?.in_bounds, true);

  const band = computeParameterTargetBand(evaluateJson, sandboxFixture, param, {
    [param.name]: param.initial,
  });
  const matchingValues = band.filter((p) => p.inTarget).map((p) => p.value);
  assert.deepEqual(matchingValues, [6, 7]);
});

test('clause subtitle sync resolves timed segments and P-06 gate rejects < 44px controls', () => {
  const manifest = {
    asset_id: 'asset-1',
    language: 'ar' as const,
    codec: 'opus' as const,
    text: 'المقطع الأول. المقطع الثاني.',
    duration_ms: 2000,
    alignment_method: 'CLAUSE_CONCAT' as const,
    words: [
      { text: 'المقطع الأول.', start_ms: 0, end_ms: 900 },
      { text: 'المقطع الثاني.', start_ms: 960, end_ms: 2000 },
    ],
  };
  assert.equal(findActiveClause(manifest, 300).index, 0);
  assert.equal(findActiveClause(manifest, 1100).index, 1);

  const valid = validateTouchTargetRects(
    [
      { id: 'slider-range', width: 320, height: 48, left: 16, right: 336 },
      { id: 'step-plus', width: 48, height: 48, left: 280, right: 328 },
    ],
    360,
    360,
  );
  assert.equal(valid.ok, true);

  const rejected = validateTouchTargetRects(
    [
      { id: 'too-small-btn', width: 28, height: 28, left: 16, right: 44 },
    ],
    360,
    360,
  );
  assert.equal(rejected.ok, false);
  assert.match(rejected.failures[0] ?? '', /28\.0x28\.0 CSS px/);
});

test('M4 catalog validates all 18 archetype x modifier presets, QUICK_BLITZ_CHAIN, 4 themes & Fortress DAG', async () => {
  const {
    ARCHETYPE_MODIFIERS,
    DEFAULT_FORTRESS_STATE,
    HOLOGRAM_CATALOG,
    QUICK_BLITZ_CHAIN_SEQUENCE,
    THEME_PALETTES,
    WORLD_THEMES,
  } = await import('../src/catalog.js');
  const { verifySandboxViaWasm, evaluateStateViaWasm, findActiveDirectorTrack, findActiveClause } =
    await import('../src/controller.js');
  const { validateAssetBudget, FpsHysteresisMonitor } = await import('../src/validator.js');

  assert.equal(WORLD_THEMES.length, 4);
  const cssClasses = new Set<string>();
  for (const theme of WORLD_THEMES) {
    const p = THEME_PALETTES[theme];
    assert.ok(p);
    cssClasses.add(p.cssClass);
  }
  assert.equal(cssClasses.size, 4);

  assert.equal(HOLOGRAM_CATALOG.length, 18);
  const seenPairs = new Set<string>();
  for (const [arch, mods] of Object.entries(ARCHETYPE_MODIFIERS)) {
    assert.equal(mods.length, 3);
    for (const mod of mods) {
      seenPairs.add(`${arch}:${mod}`);
    }
  }
  assert.equal(seenPairs.size, 18);

  for (const preset of HOLOGRAM_CATALOG) {
    assert.ok(seenPairs.has(preset.id), `Unexpected catalog id ${preset.id}`);
    const check = verifySandboxViaWasm(evaluateJson, preset.sandbox);
    assert.equal(check.valid, true, `WASM validation failed for ${preset.id}: ${check.reason}`);
    const initialVals = Object.fromEntries(preset.sandbox.parameters.map(p => [p.name, p.initial]));
    const evalState = evaluateStateViaWasm(evaluateJson, preset.sandbox, initialVals);
    assert.equal(evalState.targets.length, preset.sandbox.targets.length);

    assert.equal(preset.audioEn.codec, 'opus');
    assert.equal(preset.audioEn.alignment_method, 'CLAUSE_CONCAT');
    assert.equal(preset.audioAr.codec, 'opus');
    assert.equal(preset.audioAr.alignment_method, 'CLAUSE_CONCAT');
    assert.ok(preset.directorTracks.length >= 3);
    assert.ok(findActiveDirectorTrack([...preset.directorTracks], 500));
    assert.ok(findActiveClause(preset.audioEn, 500));
    assert.ok(findActiveClause(preset.audioAr, 500));
  }

  assert.ok(QUICK_BLITZ_CHAIN_SEQUENCE.length >= 3);
  for (const blitzId of QUICK_BLITZ_CHAIN_SEQUENCE) {
    const item = HOLOGRAM_CATALOG.find(p => p.id === blitzId);
    assert.ok(item?.quickBlitzEligible);
  }

  // Fortress Tiers 0..3 + INACTIVE + brownout + raid_immune / propagation_exempt
  const tiers = new Set(DEFAULT_FORTRESS_STATE.nodes.map(n => n.tier));
  assert.ok(tiers.has(1) && tiers.has(2) && tiers.has(3) && tiers.has(null));
  assert.ok(DEFAULT_FORTRESS_STATE.nodes.some(n => n.is_brownout));
  assert.ok(DEFAULT_FORTRESS_STATE.nodes.some(n => n.raid_immune && !n.propagation_exempt));
  assert.ok(DEFAULT_FORTRESS_STATE.nodes.some(n => n.raid_immune && n.propagation_exempt));

  // A-04 budget validator
  assert.equal(
    validateAssetBudget({
      kind: 'scene',
      triangles: 6200,
      maxTextureWidth: 1024,
      maxTextureHeight: 1024,
      bufferBytes: 512 * 1024,
      uris: ['data:audio/wav;base64,AAAA'],
      hasExecutableScript: false,
    }).ok,
    true,
  );
  assert.equal(
    validateAssetBudget({
      kind: 'scene',
      triangles: 15001,
      maxTextureWidth: 512,
      maxTextureHeight: 512,
      bufferBytes: 1024,
    }).ok,
    false,
  );
  assert.equal(
    validateAssetBudget({
      kind: 'instanced_unit',
      triangles: 2501,
      maxTextureWidth: 512,
      maxTextureHeight: 512,
      bufferBytes: 1024,
    }).ok,
    false,
  );
  assert.equal(
    validateAssetBudget({
      kind: 'asset',
      triangles: 4000,
      maxTextureWidth: 4096,
      maxTextureHeight: 512,
      bufferBytes: 1024,
      uris: ['https://cdn.example.invalid/bad.glb'],
      hasExecutableScript: true,
    }).ok,
    false,
  );

  // O-05 <28 FPS for 3s hysteresis fallback
  const monitor = new FpsHysteresisMonitor();
  let last = monitor.recordFrame(0, 50, { drawCalls: 12, triangles: 4100 });
  assert.equal(last.fallbackTriggered, false);
  for (let ms = 100; ms <= 2800; ms += 100) {
    last = monitor.recordFrame(ms, 50, { drawCalls: 12, triangles: 4100 });
  }
  assert.equal(last.fallbackTriggered, false);
  for (let ms = 2900; ms <= 3200; ms += 100) {
    last = monitor.recordFrame(ms, 50, { drawCalls: 12, triangles: 4100 });
  }
  assert.equal(last.fallbackTriggered, true);
  assert.ok(last.fps < 28 && last.p95FrameMs >= 50 && last.p99FrameMs >= 50);
});

test('M5 90s 2-lane duel projection, clock formatting, and <= 2.5s toast bounds', async () => {
  const { createInitialDuelProjection, formatDuelClock, TOAST_MAX_DURATION_MS } = await import(
    '../src/duel.js'
  );
  assert.equal(TOAST_MAX_DURATION_MS, 2500);
  const init = createInitialDuelProjection('GHOST');
  assert.equal(init.remaining_ms, 90_000);
  assert.equal(init.opponent_kind, 'GHOST');
  assert.equal(init.self.tower_hp[0], 1800);
  assert.equal(init.self.tower_shield[1], 300);
  assert.equal(init.self.hand?.length, 4);
  assert.equal(formatDuelClock(90_000), '01:30');
  assert.equal(formatDuelClock(29_500), '00:30');
  assert.equal(formatDuelClock(0), '00:00');
});

test('M6 initial operations state & WASM contract integration (raids, leagues, bosses, clans)', async () => {
  const { createInitialM6State } = await import('../src/m6.js');
  const m6 = createInitialM6State();
  assert.equal(m6.activeTab, 'RAIDS');
  assert.equal(m6.sectors.length, 3);
  assert.equal(m6.sectors.every(s => !('fsrs_stability' in s)), true);
  assert.equal(m6.leagueStandings.length, 3);
  assert.equal(m6.bossMode, 'LIVE_COOP');
  assert.equal(m6.bossCurrentHp, 250);
  assert.equal(m6.clanActiveSnapshot, 9);

  const rollover = JSON.parse(
    evaluateJson(
      JSON.stringify({
        kind: 'league_rollover',
        input: {
          tier: m6.leagueTier,
          late_joiner_shielded: false,
          participants: m6.leagueStandings,
        },
      }),
    ),
  );
  assert.equal(rollover.kind, 'league_rollover');
  const byId = Object.fromEntries(rollover.result.standings.map((s: { user_id: string; outcome: string }) => [s.user_id, s.outcome]));
  assert.equal(byId['scholar-1'], 'PROMOTED');
  assert.equal(byId['scholar-2'], 'HELD');
  assert.equal(byId['scholar-dormant'], 'DORMANT_DEMOTED');

  const strike = JSON.parse(
    evaluateJson(
      JSON.stringify({
        kind: 'boss_strike',
        input: {
          boss_hp_before: m6.bossCurrentHp,
          base_damage: 100,
          rating: 4,
          overcharged: true,
        },
      }),
    ),
  );
  assert.equal(strike.result.boss_hp_after, 50);
  assert.equal(strike.result.overkill_damage, 0);
});

test('M7 Arabic mixed-math bidi isolation & offline PWA ranked guard', () => {
  const html = renderMixedArabicMathHtml(
    'تحافظ الشبكة على W_eff = 1.25 * W_base عند جهد 12.5 V دون فيض <script>',
  );
  assert.ok(html.includes('<bdi class="m7-math-ltr" dir="ltr">W_eff = 1.25 * W_base</bdi>'));
  assert.ok(html.includes('<bdi class="m7-math-ltr" dir="ltr">12.5 V</bdi>'));
  assert.ok(!html.includes('<script>'));

  assert.equal(isSwCacheableAsset('/manifest.json', 'GET'), true);
  assert.equal(isSwCacheableAsset('/wasm/campuslingo_domain_bg.wasm', 'GET'), true);
  assert.equal(isSwCacheableAsset('/assets/content/node-1-ar.opus', 'GET'), true);
  assert.equal(isSwCacheableAsset('/v1/duels/live/submit', 'POST'), false);
  assert.equal(isSwCacheableAsset('/v1/notifications/outbox', 'GET'), false);

  const blocked = guardOfflineRankedMutation(false, '/v1/duels/live/submit');
  assert.equal(blocked.allowed, false);
  assert.equal(blocked.reason, 'offline_ranked_outcome_prohibited');
  assert.equal(guardOfflineRankedMutation(true, '/v1/duels/live/submit').allowed, true);
});
