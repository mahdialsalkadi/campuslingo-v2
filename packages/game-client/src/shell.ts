import {
  ARCHETYPE_MODIFIERS,
  DEFAULT_FORTRESS_STATE,
  HOLOGRAM_CATALOG,
  QUICK_BLITZ_CHAIN_SEQUENCE,
  THEME_PALETTES,
  WORLD_THEMES,
  buildSyntheticVoiceCueUri,
  findHologramPreset,
  type Archetype,
  type DirectorTrack,
  type FortressNodeModel,
  type FortressStateModel,
  type HologramTemplatePreset,
  type Language,
  type WorldTheme,
} from './catalog.js';
import {
  computeParameterTargetBand,
  evaluateStateViaWasm,
  findActiveClause,
  findActiveDirectorTrack,
  probeOpusSupport,
  quantizeParameterStep,
  verifySandboxViaWasm,
  type EvaluateJsonFn,
  type SandboxStateResult,
} from './controller.js';
import { ConceptSandbox2D } from './sandbox2d.js';
import { Fortress3DScene, type Scene3DTelemetry } from './scene3d.js';
import { mountDuelArenaPanel } from './duel.js';
import { mountM6OperationsPanel } from './m6.js';
import { mountM7PwaRtlPanel, renderMixedArabicMathHtml } from './m7_pwa_rtl.js';
import {
  enrollCourse,
  getCourseFortress,
  getMe,
  listCourses,
  loginOrRegisterDemo,
  logout,
  recomputeCourseShields,
  reportNodeContent,
  type CourseShieldDashboardApi,
} from './api.js';

export type ShellMode = '3D_ACTIVE' | 'MANUAL_2D' | 'AUTO_FALLBACK_2D';

export interface PlayableShellController {
  destroy(): void;
  setTheme(theme: WorldTheme): void;
  selectArchetypeModifier(archetype: Archetype, modifier: string): void;
  selectNode(nodeId: string): void;
  getSnapshot(): ShellTelemetrySnapshot;
}

export interface ShellTelemetrySnapshot {
  theme: WorldTheme;
  mode: ShellMode;
  selectedNodeId: string;
  archetype: Archetype;
  modifier: string;
  quickBlitzActive: boolean;
  quickBlitzStep: number;
  language: Language;
  muted: boolean;
  playingAudio: boolean;
  timeMs: number;
  directorTrack: DirectorTrack | null;
  activeClauseText: string;
  wasmVerified: boolean;
  allPassed: boolean;
  parameterValues: Record<string, number>;
  reportCountForNode: number;
  quarantinedForNode: boolean;
  liveConnected: boolean;
  liveCourseId: string | null;
  liveUserId: string | null;
  telemetry: Scene3DTelemetry | null;
}

const ARCHETYPES: readonly Archetype[] = [
  'PIPELINE',
  'TRADEOFF',
  'SPATIAL',
  'CONTRAST',
  'TRAP',
  'AXIOM',
];

function mapDashboardToFortress(
  dashboard: CourseShieldDashboardApi,
): FortressStateModel {
  const nodes: FortressNodeModel[] = dashboard.nodes.map((node, index) => {
    const rawArch = node.archetype as Archetype | null;
    const archetype: Archetype =
      rawArch && ARCHETYPES.includes(rawArch) ? rawArch : 'PIPELINE';
    const mods = ARCHETYPE_MODIFIERS[archetype];
    const modifier =
      node.modifier && mods.includes(node.modifier as never)
        ? node.modifier
        : mods[0]!;
    const rawTier = node.shield?.tier ?? 1;
    const tier: 0 | 1 | 2 | 3 =
      rawTier === 0 || rawTier === 1 || rawTier === 2 || rawTier === 3
        ? rawTier
        : 1;
    return {
      node_id: node.node_id,
      title: node.title,
      grid_x: node.grid_x ?? (index % 3) * 2,
      grid_y: node.grid_y ?? Math.floor(index / 3) * 2,
      tier,
      state: node.shield?.state === 'ACTIVE' ? 'ACTIVE' : 'INACTIVE',
      i_u_ticks: node.shield?.i_u_ticks ?? null,
      e_u_ticks: node.shield?.e_u_ticks ?? null,
      is_brownout: Boolean(node.shield?.is_brownout),
      raid_immune: Boolean(node.shield?.raid_immune),
      propagation_exempt: Boolean(node.shield?.propagation_exempt),
      protection_reasons: node.shield?.active_protections ?? [],
      archetype,
      modifier,
    };
  });
  const edges = dashboard.edges.map(edge => ({
    prerequisite_node_id: edge.prerequisite_node_id,
    dependent_node_id: edge.dependent_node_id,
    weight: edge.weight,
  }));
  return {
    course_id: dashboard.course_id,
    nodes,
    edges,
  };
}

function createFallbackEvaluator(): EvaluateJsonFn {
  return (rawJson: string): string => {
    const req = JSON.parse(rawJson) as {
      kind: string;
      spec?: { parameters: Array<{ name: string; initial: number }>; targets: Array<{ name: string; expression: unknown; lower: number; upper: number }> };
      values?: Record<string, number>;
    };
    const spec = req.spec!;
    const evalExpr = (expr: Record<string, unknown>, vars: Record<string, number>): number => {
      if (expr.kind === 'constant') return Number(expr.value);
      if (expr.kind === 'variable') return vars[String(expr.name)] ?? 0;
      const args = (expr.args as Array<Record<string, unknown>>).map(a => evalExpr(a, vars));
      switch (expr.op) {
        case 'add': return args.reduce((a, b) => a + b, 0);
        case 'subtract': return args[0]! - args[1]!;
        case 'multiply': return args.reduce((a, b) => a * b, 1);
        case 'divide': return args[1] === 0 ? NaN : args[0]! / args[1]!;
        case 'min': return Math.min(...args);
        case 'max': return Math.max(...args);
        default: return 0;
      }
    };
    if (req.kind === 'sandbox') {
      return JSON.stringify({
        kind: 'sandbox',
        valid: true,
        reason: 'fallback-evaluator',
        states_checked: 1,
        witness: null,
        proof_domain: 'js-fallback',
      });
    }
    const vals = req.values ?? {};
    const targets = spec.targets.map(t => {
      const val = evalExpr(t.expression as Record<string, unknown>, vals);
      const safeVal = Number.isFinite(val) ? val : 0;
      const inBounds = Number.isFinite(val) && val >= t.lower && val <= t.upper;
      return {
        name: t.name,
        value: safeVal,
        lower: t.lower,
        upper: t.upper,
        in_bounds: inBounds,
      };
    });
    return JSON.stringify({
      kind: 'sandbox_state',
      solved: targets.every(t => t.in_bounds),
      targets,
    });
  };
}

async function loadWasmEvaluator(): Promise<{ evaluateJson: EvaluateJsonFn; nativeWasm: boolean }> {
  const candidates = ['/wasm/campuslingo_domain.js', '../../engine/generated/campuslingo_domain.js'];
  const dynamicImport = new Function('path', 'return import(path);') as (p: string) => Promise<{
    default?: () => Promise<unknown>;
    evaluateJson?: EvaluateJsonFn;
  }>;
  for (const path of candidates) {
    try {
      const mod = await dynamicImport(path);
      if (typeof mod.default === 'function') await mod.default();
      if (typeof mod.evaluateJson === 'function') {
        return { evaluateJson: mod.evaluateJson, nativeWasm: true };
      }
    } catch {
      // Try next path or JS structural fallback when loaded offline in Node/file preview.
    }
  }
  return { evaluateJson: createFallbackEvaluator(), nativeWasm: false };
}

export async function mountM4GameShell(root: HTMLElement): Promise<PlayableShellController> {
  const { evaluateJson, nativeWasm } = await loadWasmEvaluator();
  const probeAudioEl = document.createElement('audio');
  const opusProbe = probeOpusSupport(mime => probeAudioEl.canPlayType(mime));
  const reducedMotion = window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false;

  let theme: WorldTheme = 'BIO_SYSTEM';
  let mode: ShellMode = '3D_ACTIVE';
  let cameraFocusMode: 'hologram' | 'fortress' = 'hologram';
  let fortress: FortressStateModel = {
    course_id: DEFAULT_FORTRESS_STATE.course_id,
    nodes: DEFAULT_FORTRESS_STATE.nodes.map(n => ({ ...n })),
    edges: DEFAULT_FORTRESS_STATE.edges.map(e => ({ ...e })),
  };
  let selectedNodeId = fortress.nodes[0]!.node_id;
  let preset: HologramTemplatePreset = findHologramPreset(fortress.nodes[0]!.archetype, fortress.nodes[0]!.modifier);
  let parameterValues: Record<string, number> = Object.fromEntries(
    preset.sandbox.parameters.map(p => [p.name, p.initial]),
  );
  let language: Language = 'en';
  let muted = false;
  let playingAudio = false;
  let timeMs = 0;
  let quickBlitzActive = false;
  let quickBlitzStep = 0;
  let stressLowFps = false;
  let latestTelemetry: Scene3DTelemetry | null = null;
  let liveConnected = false;
  let liveCourseId: string | null = null;
  let liveUserId: string | null = null;
  let liveContentVersionId: string | null = null;
  const nodeReports = new Map<string, number>();

  root.innerHTML = `
    <div class="m4-header">
      <div>
        <p class="eyebrow">M4 Strategy Shell · Concept Fortress &amp; Hologram Engine</p>
        <h2 class="m4-title" id="m4-shell-title" dir="auto">Bioluminescent Concept Fortress</h2>
      </div>
      <div class="m4-toolbar">
        <div class="m4-theme-group" role="group" aria-label="World Theme Selector"></div>
        <div class="m4-mode-group">
          <span class="badge-chip" id="m4-live-badge" data-testid="m4-live-badge">OFFLINE DEMO</span>
          <button type="button" class="touch-btn accent-btn" id="m4-live-connect-btn" data-testid="m4-live-connect-btn">Connect Live Server</button>
          <button type="button" class="touch-btn" id="m4-recompute-shields-btn" data-testid="m4-recompute-shields-btn">Recompute Shields</button>
          <button type="button" class="touch-btn" id="m4-focus-toggle" data-testid="m4-focus-toggle">Focus: Hologram</button>
          <button type="button" class="touch-btn" id="m4-fallback-toggle" data-testid="m4-fallback-toggle">Mode: 3D WebGL</button>
          <button type="button" class="touch-btn" id="m4-stress-fps-btn" data-testid="m4-stress-fps-btn">Simulate &lt;28 FPS</button>
        </div>
      </div>
    </div>

    <div class="m4-workspace">
      <div class="m4-viewport-pane">
        <div id="m4-3d-stage" class="m4-3d-stage" data-testid="m4-3d-stage"></div>
        <div id="m4-2d-stage" class="m4-2d-stage" data-testid="m4-2d-stage" hidden></div>

        <div class="m4-telemetry-strip" data-testid="m4-telemetry-strip">
          <span id="m4-hud-tris" data-testid="m4-hud-tris">Tris: -- / 15000</span>
          <span id="m4-hud-unit-tris" data-testid="m4-hud-unit-tris">Unit max: -- / 2500</span>
          <span id="m4-hud-fps" data-testid="m4-hud-fps">FPS: --</span>
          <span id="m4-hud-mode" data-testid="m4-hud-mode">3D_ACTIVE</span>
          <span id="m4-hud-budget" data-testid="m4-hud-budget">A-04: OK</span>
        </div>

        <div class="m4-voice-bar">
          <div class="m4-voice-controls">
            <button type="button" class="touch-btn" id="m4-audio-play" data-testid="m4-audio-play">Play .opus Cue</button>
            <button type="button" class="touch-btn" id="m4-audio-mute" data-testid="m4-audio-mute">Mute (No Advantage)</button>
            <button type="button" class="touch-btn" id="m4-lang-toggle" data-testid="m4-lang-toggle">Lang: EN</button>
            <span class="director-badge" id="m4-director-readout" data-testid="m4-director-readout">overview · concept · 10%</span>
          </div>
          <p id="m4-caption-text" class="subtitle-banner" dir="auto" data-testid="m4-caption-text"></p>
          <div id="m4-cue-clauses" class="clause-chips"></div>
          <audio id="m4-opus-player" preload="none"></audio>
        </div>
      </div>

      <aside class="m4-control-deck" aria-label="Fortress Shield and Hologram Deck">
        <section class="m4-subpanel">
          <div class="m4-subhead">
            <h3>Selected Fortress Bastion</h3>
            <span id="m4-node-state-badge" class="badge-chip" data-testid="m4-node-state-badge">TIER 3 · ACTIVE</span>
          </div>
          <div id="m4-node-meta" class="m4-node-meta" data-testid="m4-node-meta"></div>
          <div class="m4-fortress-node-pills" id="m4-node-pills"></div>
          <div class="m4-report-row">
            <button type="button" class="touch-btn warn-btn" id="m4-report-btn" data-testid="m4-report-btn">Report Defect (S-13)</button>
            <span id="m4-report-status" class="m4-muted-note" data-testid="m4-report-status">Version reports: 0 / 3</span>
          </div>
        </section>

        <section class="m4-subpanel">
          <div class="m4-subhead">
            <h3>6 Archetypes × 3 Modifiers</h3>
            <button type="button" class="touch-btn accent-btn" id="m4-quick-blitz-btn" data-testid="m4-quick-blitz-btn">QUICK_BLITZ_CHAIN</button>
          </div>
          <div id="m4-archetype-tabs" class="m4-archetype-tabs"></div>
          <div id="m4-modifier-tabs" class="m4-modifier-tabs"></div>
          <div class="m4-preset-summary">
            <strong id="m4-preset-title" data-testid="m4-preset-title"></strong>
            <span id="m4-preset-summary" data-testid="m4-preset-summary" dir="auto"></span>
          </div>
          <div id="m4-sandbox-sliders" class="m4-sandbox-sliders"></div>
          <div class="m4-target-eval-row">
            <span id="m4-eval-badge" class="target-badge" data-testid="m4-eval-badge">CHECKING</span>
            <span id="m4-eval-readout" class="target-readout" data-testid="m4-eval-readout"></span>
          </div>
        </section>

        <section class="m4-subpanel" id="m5-duel-mount" data-testid="m5-duel-mount"></section>
        <section class="m4-subpanel" id="m6-operations-mount" data-testid="m6-operations-mount"></section>
        <section class="m4-subpanel" id="m7-pwa-rtl-mount" data-testid="m7-pwa-rtl-mount"></section>
      </aside>
    </div>
  `;

  const stage3dEl = root.querySelector<HTMLElement>('#m4-3d-stage')!;
  const stage2dEl = root.querySelector<HTMLElement>('#m4-2d-stage')!;
  const audioEl = root.querySelector<HTMLAudioElement>('#m4-opus-player')!;
  const titleEl = root.querySelector<HTMLElement>('#m4-shell-title')!;
  const liveBadgeEl = root.querySelector<HTMLElement>('#m4-live-badge')!;
  const liveConnectBtn = root.querySelector<HTMLButtonElement>('#m4-live-connect-btn')!;
  const recomputeShieldsBtn = root.querySelector<HTMLButtonElement>('#m4-recompute-shields-btn')!;
  const focusToggleBtn = root.querySelector<HTMLButtonElement>('#m4-focus-toggle')!;
  const fallbackToggleBtn = root.querySelector<HTMLButtonElement>('#m4-fallback-toggle')!;
  const stressFpsBtn = root.querySelector<HTMLButtonElement>('#m4-stress-fps-btn')!;
  const audioPlayBtn = root.querySelector<HTMLButtonElement>('#m4-audio-play')!;
  const audioMuteBtn = root.querySelector<HTMLButtonElement>('#m4-audio-mute')!;
  const langToggleBtn = root.querySelector<HTMLButtonElement>('#m4-lang-toggle')!;
  const directorReadoutEl = root.querySelector<HTMLElement>('#m4-director-readout')!;
  const captionTextEl = root.querySelector<HTMLElement>('#m4-caption-text')!;
  const cueClausesEl = root.querySelector<HTMLElement>('#m4-cue-clauses')!;
  const nodeBadgeEl = root.querySelector<HTMLElement>('#m4-node-state-badge')!;
  const nodeMetaEl = root.querySelector<HTMLElement>('#m4-node-meta')!;
  const nodePillsEl = root.querySelector<HTMLElement>('#m4-node-pills')!;
  const reportBtn = root.querySelector<HTMLButtonElement>('#m4-report-btn')!;
  const reportStatusEl = root.querySelector<HTMLElement>('#m4-report-status')!;
  const quickBlitzBtn = root.querySelector<HTMLButtonElement>('#m4-quick-blitz-btn')!;
  const archetypeTabsEl = root.querySelector<HTMLElement>('#m4-archetype-tabs')!;
  const modifierTabsEl = root.querySelector<HTMLElement>('#m4-modifier-tabs')!;
  const presetTitleEl = root.querySelector<HTMLElement>('#m4-preset-title')!;
  const presetSummaryEl = root.querySelector<HTMLElement>('#m4-preset-summary')!;
  const slidersEl = root.querySelector<HTMLElement>('#m4-sandbox-sliders')!;
  const duelMountEl = root.querySelector<HTMLElement>('#m5-duel-mount')!;
  const m6MountEl = root.querySelector<HTMLElement>('#m6-operations-mount')!;
  const m7MountEl = root.querySelector<HTMLElement>('#m7-pwa-rtl-mount')!;
  const evalBadgeEl = root.querySelector<HTMLElement>('#m4-eval-badge')!;
  const evalReadoutEl = root.querySelector<HTMLElement>('#m4-eval-readout')!;
  const hudTrisEl = root.querySelector<HTMLElement>('#m4-hud-tris')!;
  const hudUnitTrisEl = root.querySelector<HTMLElement>('#m4-hud-unit-tris')!;
  const hudFpsEl = root.querySelector<HTMLElement>('#m4-hud-fps')!;
  const hudModeEl = root.querySelector<HTMLElement>('#m4-hud-mode')!;
  const hudBudgetEl = root.querySelector<HTMLElement>('#m4-hud-budget')!;
  const themeGroupEl = root.querySelector<HTMLElement>('.m4-theme-group')!;

  const currentManifest = () => (language === 'ar' ? preset.audioAr : preset.audioEn);

  const evaluateCurrentSandbox = (): { verified: boolean; evalResult: SandboxStateResult } => {
    const verification = verifySandboxViaWasm(evaluateJson, preset.sandbox);
    const evalResult = evaluateStateViaWasm(evaluateJson, preset.sandbox, parameterValues);
    return { verified: verification.valid, evalResult };
  };

  let scene3d: Fortress3DScene | null = null;
  let sandbox2d: ConceptSandbox2D | null = null;
  let playbackTimer: number | null = null;
  let lastPlaybackTick = 0;

  const stopAudioTimer = () => {
    if (playbackTimer !== null) {
      window.clearInterval(playbackTimer);
      playbackTimer = null;
    }
    audioEl.pause();
  };

  const applyThemeClasses = () => {
    for (const key of WORLD_THEMES) {
      root.classList.remove(THEME_PALETTES[key].cssClass);
    }
    const p = THEME_PALETTES[theme];
    root.classList.add(p.cssClass);
    titleEl.textContent = p.label;
    themeGroupEl.querySelectorAll<HTMLButtonElement>('button').forEach(btn => {
      btn.setAttribute('aria-pressed', btn.dataset.theme === theme ? 'true' : 'false');
    });
  };

  const syncAll = () => {
    applyThemeClasses();
    const { verified, evalResult } = evaluateCurrentSandbox();
    const manifest = currentManifest();
    const track = findActiveDirectorTrack([...preset.directorTracks], timeMs);
    const clause = findActiveClause(manifest, timeMs);
    const selectedNode = fortress.nodes.find(n => n.node_id === selectedNodeId) ?? fortress.nodes[0]!;
    const reports = nodeReports.get(selectedNode.node_id) ?? 0;

    // Viewport visibility per O-05 mode
    const using2d = mode !== '3D_ACTIVE';
    stage3dEl.hidden = using2d;
    stage2dEl.hidden = !using2d;
    scene3d?.setPaused(using2d);
    fallbackToggleBtn.textContent = mode === '3D_ACTIVE' ? 'Mode: 3D WebGL' : mode === 'AUTO_FALLBACK_2D' ? 'Mode: Auto 2D' : 'Mode: Manual 2D';
    hudModeEl.textContent = mode;
    focusToggleBtn.textContent = cameraFocusMode === 'hologram' ? 'Focus: Hologram' : 'Focus: Fortress';
    stressFpsBtn.setAttribute('aria-pressed', stressLowFps ? 'true' : 'false');
    liveBadgeEl.textContent = liveConnected ? 'LIVE SERVER' : 'OFFLINE DEMO';
    liveBadgeEl.className = liveConnected ? 'badge-chip badge-safe' : 'badge-chip';
    liveConnectBtn.textContent = liveConnected ? 'Disconnect Live' : 'Connect Live Server';
    stressFpsBtn.textContent = stressLowFps ? 'Stop <28 FPS Stress' : 'Simulate <28 FPS';

    // Node summary + protection badges
    nodeBadgeEl.textContent = selectedNode.state === 'INACTIVE'
      ? 'INACTIVE · NULL'
      : `TIER ${selectedNode.tier ?? 0} · ${selectedNode.is_brownout ? 'BROWNOUT' : 'ACTIVE'}`;
    nodeBadgeEl.className = `badge-chip ${selectedNode.is_brownout ? 'badge-warn' : selectedNode.raid_immune ? 'badge-safe' : ''}`;
    const protSummary = selectedNode.protection_reasons.length > 0
      ? `${selectedNode.protection_reasons.join(', ')} (raid_immune=${selectedNode.raid_immune}, prop_exempt=${selectedNode.propagation_exempt})`
      : 'No active protection';
    nodeMetaEl.innerHTML = `
      <span><strong>${selectedNode.title}</strong> · E_u=${selectedNode.e_u_ticks ?? '—'} · I_u=${selectedNode.i_u_ticks ?? '—'}</span>
      <span>${protSummary}</span>
    `;
    nodePillsEl.querySelectorAll<HTMLButtonElement>('button').forEach(btn => {
      btn.setAttribute('aria-pressed', btn.dataset.nodeId === selectedNode.node_id ? 'true' : 'false');
    });
    const quarantined = reports >= 3;
    reportStatusEl.textContent = quarantined
      ? `Version reports: ${reports} / 3 · QUARANTINED (fsrs_cards preserved)`
      : `Version reports: ${reports} / 3 (${opusProbe.supported ? '.opus ready' : 'fallback audio'})`;

    // Archetype / modifier deck + QUICK_BLITZ_CHAIN indicator
    archetypeTabsEl.querySelectorAll<HTMLButtonElement>('button').forEach(btn => {
      btn.setAttribute('aria-pressed', btn.dataset.archetype === preset.archetype ? 'true' : 'false');
    });
    renderModifierButtons();
    quickBlitzBtn.textContent = quickBlitzActive
      ? `QUICK_BLITZ (${quickBlitzStep + 1}/${QUICK_BLITZ_CHAIN_SEQUENCE.length})`
      : 'QUICK_BLITZ_CHAIN';
    presetTitleEl.textContent = `${preset.archetype} × ${preset.modifier} — ${preset.title} (${verified ? (nativeWasm ? 'WASM OK' : 'JS OK') : 'INVALID'})`;
    presetSummaryEl.textContent = `${preset.tacticalSummary[0]} ${preset.tacticalSummary[1]}`;

    evalBadgeEl.textContent = evalResult.solved ? 'IN TARGET ZONE' : 'OUTSIDE TARGET';
    evalBadgeEl.className = `target-badge ${evalResult.solved ? 'in-zone' : 'out-zone'}`;
    evalReadoutEl.textContent = evalResult.targets
      .map(t => `${t.name}=${Number.isFinite(t.value) ? t.value.toFixed(2) : 'NaN'}`)
      .join(' · ');

    // DirectorTrack & voiceover captions
    directorReadoutEl.textContent = track
      ? `${track.shot_type} (${track.focus_role}) · explode ${(track.explode_factor * 100).toFixed(0)}%`
      : 'overview (concept)';
    const captionRaw = clause.word?.text ?? manifest.text;
    if (language === 'ar') {
      captionTextEl.dir = 'rtl';
      captionTextEl.lang = 'ar';
      captionTextEl.innerHTML = renderMixedArabicMathHtml(captionRaw);
    } else {
      captionTextEl.dir = 'ltr';
      captionTextEl.lang = 'en';
      captionTextEl.textContent = captionRaw;
    }
    audioPlayBtn.textContent = playingAudio ? 'Pause .opus Cue' : 'Play .opus Cue';
    audioMuteBtn.textContent = muted ? 'Muted (Zero Advantage)' : 'Mute (No Advantage)';
    audioMuteBtn.setAttribute('aria-pressed', muted ? 'true' : 'false');
    langToggleBtn.textContent = `Lang: ${language.toUpperCase()}`;
    audioEl.muted = muted;
    renderClauseButtons();

    // Update 3D scene & 2D fallback
    scene3d?.updateState({
      theme,
      fortress,
      selectedNodeId,
      archetype: preset.archetype,
      modifier: preset.modifier,
      parameterValues,
      evaluationAllPassed: evalResult.solved,
      track,
      cameraFocusMode,
      reducedMotion,
    });

    const targetReadings = preset.sandbox.targets.map((specTarget, idx) => {
      const evaluated = evalResult.targets[idx];
      return {
        name: specTarget.name,
        value: evaluated?.value ?? null,
        passed: evaluated?.in_bounds ?? false,
        lower: specTarget.lower,
        upper: specTarget.upper,
      };
    });
    sandbox2d?.updateState({
      theme,
      fortress,
      selectedNodeId,
      archetype: preset.archetype,
      modifier: preset.modifier,
      parameterValues,
      targetReadings,
      track,
      visible: using2d,
    });

    publishWindowSnapshot(verified, evalResult.solved, track, clause.word?.text ?? '');
  };

  let windowShellHandle: (PlayableShellController & { snapshot: ShellTelemetrySnapshot }) | null = null;
  const publishWindowSnapshot = (
    wasmVerified: boolean,
    allPassed: boolean,
    track: DirectorTrack | null,
    activeClauseText: string,
  ) => {
    const snap: ShellTelemetrySnapshot = {
      theme,
      mode,
      selectedNodeId,
      archetype: preset.archetype,
      modifier: preset.modifier,
      quickBlitzActive,
      quickBlitzStep,
      language,
      muted,
      playingAudio,
      timeMs,
      directorTrack: track,
      activeClauseText,
      wasmVerified,
      allPassed,
      parameterValues: { ...parameterValues },
      reportCountForNode: nodeReports.get(selectedNodeId) ?? 0,
      quarantinedForNode: (nodeReports.get(selectedNodeId) ?? 0) >= 3,
      liveConnected,
      liveCourseId,
      liveUserId,
      telemetry: latestTelemetry,
    };
    if (!windowShellHandle) {
      windowShellHandle = {
        destroy,
        setTheme,
        selectArchetypeModifier,
        selectNode,
        getSnapshot: () => windowShellHandle!.snapshot,
        snapshot: snap,
      };
      (window as unknown as { __CAMPUSLINGO_SHELL__?: PlayableShellController & { snapshot: ShellTelemetrySnapshot } }).__CAMPUSLINGO_SHELL__ =
        windowShellHandle;
    } else {
      windowShellHandle.snapshot = snap;
    }
  };

  const renderThemeButtons = () => {
    themeGroupEl.replaceChildren(
      ...WORLD_THEMES.map(key => {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = 'touch-btn m4-theme-btn';
        btn.dataset.theme = key;
        btn.dataset.testid = `m4-theme-${key}`;
        btn.textContent = key.replace('_', ' ');
        btn.addEventListener('click', () => setTheme(key));
        return btn;
      }),
    );
  };

  const renderNodeButtons = () => {
    nodePillsEl.replaceChildren(
      ...fortress.nodes.map(node => {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = 'touch-btn m4-node-btn';
        btn.dataset.nodeId = node.node_id;
        btn.dataset.testid = `m4-node-${node.node_id}`;
        btn.textContent = `${node.title.split(' ')[0]} T${node.tier ?? '-'}`;
        btn.addEventListener('click', () => selectNode(node.node_id));
        return btn;
      }),
    );
  };

  const renderArchetypeButtons = () => {
    archetypeTabsEl.replaceChildren(
      ...ARCHETYPES.map(arch => {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = 'touch-btn m4-arch-btn';
        btn.dataset.archetype = arch;
        btn.dataset.testid = `m4-arch-${arch}`;
        btn.textContent = arch;
        btn.addEventListener('click', () => {
          quickBlitzActive = false;
          const defaultMod = ARCHETYPE_MODIFIERS[arch][0];
          selectArchetypeModifier(arch, defaultMod);
        });
        return btn;
      }),
    );
  };

  const renderModifierButtons = () => {
    const mods = ARCHETYPE_MODIFIERS[preset.archetype];
    modifierTabsEl.replaceChildren(
      ...mods.map(mod => {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = 'touch-btn m4-mod-btn';
        btn.dataset.modifier = mod;
        btn.dataset.testid = `m4-mod-${mod}`;
        btn.textContent = mod.replace(/_/g, ' ');
        btn.setAttribute('aria-pressed', preset.modifier === mod ? 'true' : 'false');
        btn.addEventListener('click', () => {
          quickBlitzActive = false;
          selectArchetypeModifier(preset.archetype, mod);
        });
        return btn;
      }),
    );
  };

  const renderSliders = () => {
    slidersEl.replaceChildren(
      ...preset.sandbox.parameters.map(param => {
        const row = document.createElement('div');
        row.className = 'param-row';
        const label = document.createElement('label');
        label.textContent = `${param.label}: ${parameterValues[param.name]}`;
        const band = computeParameterTargetBand(evaluateJson, preset.sandbox, param, parameterValues);
        const controls = document.createElement('div');
        controls.className = 'param-controls';

        const decBtn = document.createElement('button');
        decBtn.type = 'button';
        decBtn.className = 'touch-btn';
        decBtn.dataset.testid = `m4-dec-${param.name}`;
        decBtn.textContent = '-';
        decBtn.addEventListener('click', () => {
          parameterValues[param.name] = quantizeParameterStep(
            param,
            (parameterValues[param.name] ?? param.initial) - param.step,
          );
          renderSliders();
          syncAll();
        });

        const slider = document.createElement('input');
        slider.type = 'range';
        slider.className = 'param-slider';
        slider.dataset.testid = `m4-slider-${param.name}`;
        slider.min = String(param.minimum);
        slider.max = String(param.maximum);
        slider.step = String(param.step);
        slider.value = String(parameterValues[param.name]);
        slider.setAttribute('aria-label', param.label);
        slider.addEventListener('input', () => {
          parameterValues[param.name] = quantizeParameterStep(param, Number(slider.value));
          renderSliders();
          syncAll();
        });

        const incBtn = document.createElement('button');
        incBtn.type = 'button';
        incBtn.className = 'touch-btn';
        incBtn.dataset.testid = `m4-inc-${param.name}`;
        incBtn.textContent = '+';
        incBtn.addEventListener('click', () => {
          parameterValues[param.name] = quantizeParameterStep(
            param,
            (parameterValues[param.name] ?? param.initial) + param.step,
          );
          renderSliders();
          syncAll();
        });

        const passingValues = band.filter(pt => pt.inTarget).map(pt => pt.value);
        controls.append(decBtn, slider, incBtn);
        if (passingValues.length > 0) {
          const hint = document.createElement('span');
          hint.className = 'm4-muted-note';
          hint.textContent = `Zone: ${passingValues[0]}..${passingValues[passingValues.length - 1]}`;
          row.append(label, controls, hint);
        } else {
          row.append(label, controls);
        }
        return row;
      }),
    );
  };

  const renderClauseButtons = () => {
    const manifest = currentManifest();
    cueClausesEl.replaceChildren(
      ...manifest.words.map((word, idx) => {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = 'touch-btn';
        btn.dataset.testid = `m4-clause-${idx}`;
        btn.dir = 'auto';
        const active = timeMs >= word.start_ms && timeMs < word.end_ms;
        btn.setAttribute('aria-pressed', active ? 'true' : 'false');
        btn.textContent = `${idx + 1}. ${word.text}`;
        btn.addEventListener('click', () => {
          timeMs = word.start_ms + 40;
          syncAll();
        });
        return btn;
      }),
    );
  };

  const setTheme = (nextTheme: WorldTheme) => {
    theme = nextTheme;
    syncAll();
  };

  const selectArchetypeModifier = (archetype: Archetype, modifier: string) => {
    preset = findHologramPreset(archetype, modifier);
    parameterValues = Object.fromEntries(preset.sandbox.parameters.map(p => [p.name, p.initial]));
    timeMs = 0;
    renderSliders();
    syncAll();
  };

  const selectNode = (nodeId: string) => {
    const found: FortressNodeModel | undefined = fortress.nodes.find(n => n.node_id === nodeId);
    if (!found) return;
    selectedNodeId = found.node_id;
    quickBlitzActive = false;
    selectArchetypeModifier(found.archetype, found.modifier);
  };

  // Wire buttons
  focusToggleBtn.addEventListener('click', () => {
    cameraFocusMode = cameraFocusMode === 'hologram' ? 'fortress' : 'hologram';
    syncAll();
  });

  fallbackToggleBtn.addEventListener('click', () => {
    if (mode === '3D_ACTIVE') {
      mode = 'MANUAL_2D';
    } else {
      stressLowFps = false;
      scene3d?.setForcedLowFps(false);
      scene3d?.resetFallbackMonitor();
      mode = '3D_ACTIVE';
    }
    syncAll();
  });

  stressFpsBtn.addEventListener('click', () => {
    stressLowFps = !stressLowFps;
    scene3d?.setForcedLowFps(stressLowFps);
    if (!stressLowFps && mode === 'AUTO_FALLBACK_2D') {
      scene3d?.resetFallbackMonitor();
      mode = '3D_ACTIVE';
    }
    syncAll();
  });

  langToggleBtn.addEventListener('click', () => {
    language = language === 'en' ? 'ar' : 'en';
    syncAll();
  });

  audioMuteBtn.addEventListener('click', () => {
    muted = !muted;
    syncAll();
  });

  audioPlayBtn.addEventListener('click', () => {
    if (playingAudio) {
      playingAudio = false;
      stopAudioTimer();
      syncAll();
      return;
    }
    playingAudio = true;
    const manifest = currentManifest();
    audioEl.src = buildSyntheticVoiceCueUri(manifest.duration_ms);
    audioEl.muted = muted;
    void audioEl.play().catch(() => {});
    lastPlaybackTick = performance.now();
    playbackTimer = window.setInterval(() => {
      const now = performance.now();
      const dt = now - lastPlaybackTick;
      lastPlaybackTick = now;
      timeMs = (timeMs + dt) % Math.max(1, manifest.duration_ms);
      syncAll();
    }, 120);
    syncAll();
  });

  quickBlitzBtn.addEventListener('click', () => {
    quickBlitzActive = true;
    quickBlitzStep = (quickBlitzStep + 1) % QUICK_BLITZ_CHAIN_SEQUENCE.length;
    const key = QUICK_BLITZ_CHAIN_SEQUENCE[quickBlitzStep]!;
    const targetPreset = HOLOGRAM_CATALOG.find(item => item.id === key) ?? HOLOGRAM_CATALOG[0]!;
    selectArchetypeModifier(targetPreset.archetype, targetPreset.modifier);
  });

  const duelPanel = mountDuelArenaPanel(duelMountEl, evaluateJson);
  const m6Panel = mountM6OperationsPanel(m6MountEl, evaluateJson);
  const m7Panel = mountM7PwaRtlPanel(m7MountEl);

  const applyLiveDashboard = (dashboard: CourseShieldDashboardApi) => {
    liveContentVersionId = dashboard.active_content_version_id;
    const mapped = mapDashboardToFortress(dashboard);
    if (mapped.nodes.length > 0) {
      fortress = mapped;
      if (!fortress.nodes.some(n => n.node_id === selectedNodeId)) {
        selectedNodeId = fortress.nodes[0]!.node_id;
        preset = findHologramPreset(
          fortress.nodes[0]!.archetype,
          fortress.nodes[0]!.modifier,
        );
        parameterValues = Object.fromEntries(
          preset.sandbox.parameters.map(p => [p.name, p.initial]),
        );
        renderSliders();
      }
      renderNodeButtons();
    }
  };

  const hydrateLiveSession = async (userId: string) => {
    const courses = await listCourses();
    const targetCourse =
      courses.find(
        c =>
          c.published !== false &&
          (c.id === '00000000-0000-4000-8000-000000000201' ||
            c.name === 'Linear Systems & Hologram Citadel'),
      ) ??
      courses.find(c => c.published !== false) ??
      courses[0];
    if (!targetCourse) {
      throw new Error('No courses available on live server');
    }
    try {
      await enrollCourse(targetCourse.id);
    } catch {
      // Already enrolled or enrollment optional
    }
    const dashboard = await getCourseFortress(targetCourse.id);
    liveUserId = userId;
    liveCourseId = targetCourse.id;
    liveConnected = true;
    applyLiveDashboard(dashboard);
    duelPanel.setLiveCourse(targetCourse.id);
    await m6Panel.setLiveCourse(targetCourse.id);
    syncAll();
  };

  const disconnectLiveSession = async () => {
    try {
      await logout();
    } catch {
      // Ignore network errors on logout
    }
    liveConnected = false;
    liveUserId = null;
    liveCourseId = null;
    liveContentVersionId = null;
    duelPanel.setLiveCourse(null);
    await m6Panel.setLiveCourse(null);
    fortress = {
      course_id: DEFAULT_FORTRESS_STATE.course_id,
      nodes: DEFAULT_FORTRESS_STATE.nodes.map(n => ({ ...n })),
      edges: DEFAULT_FORTRESS_STATE.edges.map(e => ({ ...e })),
    };
    selectedNodeId = fortress.nodes[0]!.node_id;
    renderNodeButtons();
    syncAll();
  };

  liveConnectBtn.addEventListener('click', () => {
    if (liveConnected) {
      void disconnectLiveSession();
      return;
    }
    liveConnectBtn.textContent = 'Connecting...';
    void (async () => {
      try {
        const user = await loginOrRegisterDemo(
          'student@campuslingo.local',
          'CampusLingo#2026!',
        );
        await hydrateLiveSession(user.user_id);
      } catch {
        liveBadgeEl.textContent = 'CONNECT FAILED';
        liveBadgeEl.className = 'badge-chip badge-warn';
        liveConnectBtn.textContent = 'Connect Live Server';
      }
    })();
  });

  recomputeShieldsBtn.addEventListener('click', () => {
    if (!liveCourseId) {
      syncAll();
      return;
    }
    const courseId = liveCourseId;
    void (async () => {
      try {
        const dashboard = await recomputeCourseShields(courseId);
        applyLiveDashboard(dashboard);
        await m6Panel.refreshLiveState();
        syncAll();
      } catch {
        // Keep current state if transient error
      }
    })();
  });

  reportBtn.addEventListener('click', () => {
    const prev = nodeReports.get(selectedNodeId) ?? 0;
    const nextCount = prev + 1;
    nodeReports.set(selectedNodeId, nextCount);
    if (liveCourseId) {
      void reportNodeContent(
        liveCourseId,
        selectedNodeId,
        'BROKEN_QUESTION',
        'Reported from live shell',
        liveContentVersionId,
      ).catch(() => {});
    }
    syncAll();
  });

  const onResize = () => scene3d?.resize();
  const destroy = () => {
    stopAudioTimer();
    duelPanel.destroy();
    m6Panel.destroy();
    m7Panel.destroy();
    window.removeEventListener('resize', onResize);
    scene3d?.destroy();
    scene3d = null;
  };

  renderThemeButtons();
  renderNodeButtons();
  renderArchetypeButtons();
  renderSliders();

  sandbox2d = new ConceptSandbox2D(
    stage2dEl,
    {
      theme,
      fortress,
      selectedNodeId,
      archetype: preset.archetype,
      modifier: preset.modifier,
      parameterValues,
      targetReadings: [],
      track: preset.directorTracks[0] ?? null,
      visible: false,
    },
    selectNode,
  );

  scene3d = new Fortress3DScene(
    stage3dEl,
    {
      theme,
      fortress,
      selectedNodeId,
      archetype: preset.archetype,
      modifier: preset.modifier,
      parameterValues,
      evaluationAllPassed: false,
      track: preset.directorTracks[0] ?? null,
      cameraFocusMode,
      reducedMotion,
    },
    {
      onSelectNode: selectNode,
      onMetrics: telemetry => {
        latestTelemetry = telemetry;
        hudTrisEl.textContent = `Tris: ${telemetry.budget.triangles} / 15000`;
        hudUnitTrisEl.textContent = `Unit max: ${telemetry.unitTriangleMax} / 2500`;
        hudFpsEl.textContent = `FPS: ${telemetry.frame.fps} (p95 ${telemetry.frame.p95FrameMs}ms)`;
        hudBudgetEl.textContent = telemetry.budget.ok ? 'A-04: OK' : 'A-04: OVER';
        if (telemetry.rendererBackend === 'fallback-noop' && mode === '3D_ACTIVE') {
          mode = 'AUTO_FALLBACK_2D';
          syncAll();
        } else if (telemetry.frame.fallbackTriggered && mode === '3D_ACTIVE') {
          mode = 'AUTO_FALLBACK_2D';
          syncAll();
        } else {
          const { verified, evalResult } = evaluateCurrentSandbox();
          const track = findActiveDirectorTrack([...preset.directorTracks], timeMs);
          const clause = findActiveClause(currentManifest(), timeMs);
          publishWindowSnapshot(verified, evalResult.solved, track, clause.word?.text ?? '');
        }
      },
    },
  );

  window.addEventListener('resize', onResize);
  syncAll();
  void getMe()
    .then(user => (user ? hydrateLiveSession(user.user_id) : undefined))
    .catch(() => {});

  return { destroy, setTheme, selectArchetypeModifier, selectNode, getSnapshot: () => (window as unknown as { __CAMPUSLINGO_SHELL__: { snapshot: ShellTelemetrySnapshot } }).__CAMPUSLINGO_SHELL__.snapshot };
}

if (typeof document !== 'undefined') {
  const host = document.getElementById('m4-game-shell');
  if (host) {
    void mountM4GameShell(host);
  }
}
