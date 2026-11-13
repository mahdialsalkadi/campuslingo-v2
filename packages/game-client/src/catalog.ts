import type {
  AudioManifest,
  AudioWord,
} from '../../contracts/src/generated/AudioManifest.js';
import type { DirectorTrack } from '../../contracts/src/generated/DirectorTrack.js';
import type {
  Archetype,
  Language,
  WorldTheme,
} from '../../contracts/src/generated/LearningUnitBundle.js';
import type {
  Expression,
  ParameterSpec,
  SandboxSpec,
} from '../../contracts/src/generated/SandboxSpec.js';

export type {
  Archetype,
  AudioManifest,
  AudioWord,
  DirectorTrack,
  Expression,
  Language,
  ParameterSpec,
  SandboxSpec,
  WorldTheme,
};

export interface ThemePalette {
  id: WorldTheme;
  label: string;
  subtitle: string;
  backgroundHex: number;
  fogHex: number;
  gridPrimaryHex: number;
  gridSecondaryHex: number;
  plinthHex: number;
  healthyHex: number;
  brownoutHex: number;
  shieldDomeHex: number;
  inactiveHex: number;
  accentHex: number;
  accentCss: string;
  cssClass: string;
}

export const WORLD_THEMES: readonly WorldTheme[] = [
  'BIO_SYSTEM',
  'CYBER_GRID',
  'MACRO_POLIS',
  'ARCANE_ARCHIVE',
];

export const THEME_PALETTES: Record<WorldTheme, ThemePalette> = {
  BIO_SYSTEM: {
    id: 'BIO_SYSTEM',
    label: 'Bio-System Archipelago',
    subtitle: 'Cellular membranes, ATP conduits & homeostatic barriers',
    backgroundHex: 0x061918,
    fogHex: 0x061918,
    gridPrimaryHex: 0x1e6f63,
    gridSecondaryHex: 0x0d3632,
    plinthHex: 0x102a29,
    healthyHex: 0x2dd4bf,
    brownoutHex: 0xf97316,
    shieldDomeHex: 0x38bdf8,
    inactiveHex: 0x4b6360,
    accentHex: 0x5eead4,
    accentCss: '#2dd4bf',
    cssClass: 'theme-bio-system',
  },
  CYBER_GRID: {
    id: 'CYBER_GRID',
    label: 'Cyber-Grid Silicon Bus',
    subtitle: 'Isometric logic traces, packet firewalls & thermal guards',
    backgroundHex: 0x070d1b,
    fogHex: 0x070d1b,
    gridPrimaryHex: 0x1d4ed8,
    gridSecondaryHex: 0x0f1f3d,
    plinthHex: 0x0f172a,
    healthyHex: 0x22d3ee,
    brownoutHex: 0xf43f5e,
    shieldDomeHex: 0x60a5fa,
    inactiveHex: 0x475569,
    accentHex: 0x38bdf8,
    accentCss: '#38bdf8',
    cssClass: 'theme-cyber-grid',
  },
  MACRO_POLIS: {
    id: 'MACRO_POLIS',
    label: 'Macro-Polis Equilibrium Citadels',
    subtitle: 'Civic fiscal bastions, supply arteries & reserve vaults',
    backgroundHex: 0x17120c,
    fogHex: 0x17120c,
    gridPrimaryHex: 0x8a6432,
    gridSecondaryHex: 0x3b2c1a,
    plinthHex: 0x241d15,
    healthyHex: 0x34d399,
    brownoutHex: 0xf59e0b,
    shieldDomeHex: 0xfbbf24,
    inactiveHex: 0x6b6052,
    accentHex: 0xfbbf24,
    accentCss: '#fbbf24',
    cssClass: 'theme-macro-polis',
  },
  ARCANE_ARCHIVE: {
    id: 'ARCANE_ARCHIVE',
    label: 'Arcane-Archive Observatory',
    subtitle: 'Astrolabe spires, proof prisms & theorem wards',
    backgroundHex: 0x110c21,
    fogHex: 0x110c21,
    gridPrimaryHex: 0x5b42a8,
    gridSecondaryHex: 0x261a4a,
    plinthHex: 0x1b1334,
    healthyHex: 0xa78bfa,
    brownoutHex: 0xf472b6,
    shieldDomeHex: 0x818cf8,
    inactiveHex: 0x645c82,
    accentHex: 0xc4b5fd,
    accentCss: '#a78bfa',
    cssClass: 'theme-arcane-archive',
  },
};

export interface FortressNodeModel {
  node_id: string;
  title: string;
  grid_x: number;
  grid_y: number;
  tier: 0 | 1 | 2 | 3 | null;
  state: 'ACTIVE' | 'INACTIVE';
  i_u_ticks: number | null;
  e_u_ticks: number | null;
  is_brownout: boolean;
  raid_immune: boolean;
  propagation_exempt: boolean;
  protection_reasons: readonly string[];
  archetype: Archetype;
  modifier: string;
}

export interface FortressEdgeModel {
  prerequisite_node_id: string;
  dependent_node_id: string;
  weight: number;
}

export interface FortressStateModel {
  course_id: string;
  nodes: readonly FortressNodeModel[];
  edges: readonly FortressEdgeModel[];
}

export interface HologramTemplatePreset {
  id: string;
  archetype: Archetype;
  modifier: string;
  title: string;
  tacticalSummary: readonly [string, string];
  quickBlitzEligible: boolean;
  sandbox: SandboxSpec;
  directorTracks: readonly DirectorTrack[];
  audioEn: AudioManifest;
  audioAr: AudioManifest;
}

function c(value: number): Expression {
  return { kind: 'constant', value };
}

function v(name: string): Expression {
  return { kind: 'variable', name };
}

function op(
  kindOp: 'add' | 'subtract' | 'multiply' | 'divide' | 'min' | 'max',
  ...args:
    | [Expression, Expression]
    | [Expression, Expression, Expression]
    | [Expression, Expression, Expression, Expression]
): Expression {
  return { kind: 'operation', op: kindOp, args };
}

function clauseAudio(
  assetId: string,
  language: Language,
  clauses: readonly { text: string; durationMs: number }[],
): AudioManifest {
  let cursor = 0;
  const words: AudioWord[] = [];
  for (const clause of clauses) {
    words.push({
      text: clause.text,
      start_ms: cursor,
      end_ms: cursor + clause.durationMs,
    });
    cursor += clause.durationMs;
  }
  return {
    asset_id: assetId,
    language,
    codec: 'opus',
    text: clauses.map(part => part.text).join(' '),
    duration_ms: cursor,
    alignment_method: 'CLAUSE_CONCAT',
    words,
  };
}

function makeTracks(durationMs: number): readonly DirectorTrack[] {
  const splitA = Math.floor(durationMs * 0.34);
  const splitB = Math.floor(durationMs * 0.68);
  return [
    { start_ms: 0, end_ms: splitA, focus_role: 'concept', shot_type: 'overview', explode_factor: 0.1 },
    { start_ms: splitA, end_ms: splitB, focus_role: 'input', shot_type: 'close_up', explode_factor: 0.45 },
    { start_ms: splitB, end_ms: durationMs, focus_role: 'comparison', shot_type: 'compare', explode_factor: 0.25 },
  ];
}

function makePreset(input: {
  archetype: Archetype;
  modifier: string;
  title: string;
  tacticalSummary: readonly [string, string];
  quickBlitzEligible: boolean;
  timeLimitMs: number;
  parameters: SandboxSpec['parameters'];
  targets: SandboxSpec['targets'];
  enClauses: readonly [string, string, string];
  arClauses: readonly [string, string, string];
}): HologramTemplatePreset {
  const id = `${input.archetype}:${input.modifier}`;
  const enSpecs = input.enClauses.map(text => ({ text, durationMs: 1200 }));
  const arSpecs = input.arClauses.map(text => ({ text, durationMs: 1200 }));
  const audioEn = clauseAudio(`opus-${input.archetype.toLowerCase()}-${input.modifier.toLowerCase()}-en`, 'en', enSpecs);
  const audioAr = clauseAudio(`opus-${input.archetype.toLowerCase()}-${input.modifier.toLowerCase()}-ar`, 'ar', arSpecs);
  return {
    id,
    archetype: input.archetype,
    modifier: input.modifier,
    title: input.title,
    tacticalSummary: input.tacticalSummary,
    quickBlitzEligible: input.quickBlitzEligible,
    sandbox: {
      schema_version: '1.0.0',
      archetype: input.archetype,
      modifier: input.modifier,
      parameters: input.parameters,
      targets: input.targets,
      time_limit_ms: input.timeLimitMs,
    },
    directorTracks: makeTracks(audioEn.duration_ms),
    audioEn,
    audioAr,
  };
}

const GAIN_PARAM: ParameterSpec = {
  name: 'gain',
  label: 'Flow Gain',
  minimum: 1,
  maximum: 10,
  step: 0.5,
  initial: 4,
};

const DAMPING_PARAM: ParameterSpec = {
  name: 'damping',
  label: 'Damping',
  minimum: 1,
  maximum: 6,
  step: 0.5,
  initial: 2,
};

const RESERVE_PARAM: ParameterSpec = {
  name: 'reserve',
  label: 'Reserve Floor',
  minimum: 2,
  maximum: 12,
  step: 1,
  initial: 6,
};

export const ARCHETYPE_MODIFIERS: Record<Archetype, readonly [string, string, string]> = {
  PIPELINE: ['REVERSE_ENG', 'BOTTLENECK_RELIEF', 'FAULT_INJECTION'],
  TRADEOFF: ['OVERLOAD_STRESS', 'BUDGET_CAP', 'PARETO_BALANCE'],
  SPATIAL: ['BLIND_DIAGNOSIS', 'SURGICAL_REPAIR', 'CROSS_SECTION_SCAN'],
  CONTRAST: ['STRESS_BENCHMARK', 'BLIND_CHAMBER_ID', 'CROSS_CALIBRATION'],
  TRAP: ['FORENSIC_AUDIT', 'COUNTEREXAMPLE_PROBE', 'SAFEGUARD_PATCH'],
  AXIOM: ['SPEED_BLITZ', 'COROLLARY_CHAIN', 'BOUNDARY_TEST'],
};

/**
 * Complete frozen matrix of all 6 archetypes x 3 canonical modifiers (= 18 templates).
 * Every SandboxSpec passes both AJV schema checks and the Rust/WASM evaluator cleanly.
 */
export const HOLOGRAM_CATALOG: readonly HologramTemplatePreset[] = [
  makePreset({
    archetype: 'PIPELINE',
    modifier: 'REVERSE_ENG',
    title: 'Pipeline Reverse Trace',
    tacticalSummary: ['Recover hidden stage gain from observed output.', 'Match downstream flux without saturating pipe capacity.'],
    quickBlitzEligible: true,
    timeLimitMs: 10000,
    parameters: [GAIN_PARAM, DAMPING_PARAM],
    targets: [{ name: 'net_flux', expression: op('subtract', op('multiply', v('gain'), c(3)), v('damping')), lower: 14, upper: 20 }],
    enClauses: ['Inspect downstream flux across three coupled stages.', 'Tune stage gain while holding damping inside bounds.', 'Confirm reconstructed throughput meets target window.'],
    arClauses: ['افحص التدفق عبر ثلاث مراحل مترابطة.', 'اضبط معامل الكسب مع بقاء التخميد متوازناً.', 'تحقق من أن الناتج يطابق النطاق المطلوب.'],
  }),
  makePreset({
    archetype: 'PIPELINE',
    modifier: 'BOTTLENECK_RELIEF',
    title: 'Stage Bottleneck Relief',
    tacticalSummary: ['Raise constrained bottleneck capacity.', 'Respect upstream conservation limit.'],
    quickBlitzEligible: true,
    timeLimitMs: 10000,
    parameters: [GAIN_PARAM, RESERVE_PARAM],
    targets: [{ name: 'stage_min', expression: op('min', op('multiply', v('gain'), c(2)), op('add', v('reserve'), c(6))), lower: 12, upper: 18 }],
    enClauses: ['Locate the narrowest stage in the conduit.', 'Boost flow while widening reserve buffer capacity.', 'Verify stage minimum clears brownout threshold.'],
    arClauses: ['حدد أضيق مرحلة في خط التدفق.', 'ارفع سعة الاحتياطي لتفادي الاختناق.', 'تأكد من تجاوز الحد الأدنى للتشغيل السليم.'],
  }),
  makePreset({
    archetype: 'PIPELINE',
    modifier: 'FAULT_INJECTION',
    title: 'Downstream Fault Isolation',
    tacticalSummary: ['Compensate for mid-stage bypass loss.', 'Keep tail sensor within tolerance.'],
    quickBlitzEligible: true,
    timeLimitMs: 9000,
    parameters: [GAIN_PARAM, DAMPING_PARAM],
    targets: [{ name: 'residual_flow', expression: op('divide', op('multiply', v('gain'), c(4)), op('add', v('damping'), c(1))), lower: 7, upper: 11 }],
    enClauses: ['Simulate a partial bypass fault on stage two.', 'Balance gain against denominator damping term.', 'Pin recovered output safely inside tolerance band.'],
    arClauses: ['حاكي عطلاً جزئياً في المرحلة الثانية.', 'وازن الكسب مع معامل التخميد في المقام.', 'ثبّت الخرج المستعاد داخل حدود الأمان.'],
  }),
  makePreset({
    archetype: 'TRADEOFF',
    modifier: 'OVERLOAD_STRESS',
    title: 'Overload Thermal Margin',
    tacticalSummary: ['Deliver peak output before thermal stress breaches ceiling.', 'Watch marginal cost curve accelerate.'],
    quickBlitzEligible: true,
    timeLimitMs: 10000,
    parameters: [GAIN_PARAM, DAMPING_PARAM],
    targets: [
      { name: 'output_power', expression: op('multiply', v('gain'), c(2.5)), lower: 14, upper: 20 },
      { name: 'thermal_load', expression: op('subtract', op('multiply', v('gain'), c(2)), v('damping')), lower: 6, upper: 12 },
    ],
    enClauses: ['Drive core utilization under rising load.', 'Trade cooling damping against thermal dissipation.', 'Hold both power and heat inside the safe Pareto box.'],
    arClauses: ['ارفع حمل التشغيل تحت ضغط حراري متزايد.', 'وازن التخميد لتقليل الحرارة دون هبوط الطاقة.', 'حافظ على الطاقة والحرارة ضمن المجال الآمن.'],
  }),
  makePreset({
    archetype: 'TRADEOFF',
    modifier: 'BUDGET_CAP',
    title: 'Constrained Budget Allocator',
    tacticalSummary: ['Maximize yield under a hard cost cap.', 'Unused slack hurts final score cleanly.'],
    quickBlitzEligible: false,
    timeLimitMs: 11000,
    parameters: [GAIN_PARAM, RESERVE_PARAM],
    targets: [
      { name: 'yield_score', expression: op('add', op('multiply', v('gain'), c(2)), v('reserve')), lower: 16, upper: 22 },
      { name: 'spend_cap', expression: op('add', v('gain'), v('reserve')), lower: 9, upper: 13 },
    ],
    enClauses: ['Inspect allocation sliders under fixed total budget.', 'Combine marginal gain with protective reserve allocation.', 'Clear minimum yield while honoring spend ceiling.'],
    arClauses: ['افحص توزيع الموارد ضمن سقف ميزانية صارم.', 'ادمج الكسب الحدي مع مخزون الأمان.', 'حقق العائد المطلوب دون تجاوز الحد الأعلى للإنفاق.'],
  }),
  makePreset({
    archetype: 'TRADEOFF',
    modifier: 'PARETO_BALANCE',
    title: 'Dual-Objective Equilibrium',
    tacticalSummary: ['Co-optimize latency reduction and reserve buffer.', 'Avoid corner solutions that starve one metric.'],
    quickBlitzEligible: false,
    timeLimitMs: 12000,
    parameters: [GAIN_PARAM, DAMPING_PARAM, RESERVE_PARAM],
    targets: [{ name: 'harmony_index', expression: op('min', op('add', v('gain'), v('reserve')), op('multiply', v('damping'), c(4))), lower: 10, upper: 15 }],
    enClauses: ['Compare competing axes on the balance fulcrum.', 'Raise weaker leg without saturating damping multiplier.', 'Settle the Pareto fulcrum cleanly inside interval.'],
    arClauses: ['قارن المحورين المتنافسين على ميزان التوازن.', 'ادعم الجانب الأضعف دون إفراط في التخميد.', 'استقر عند نقطة باريـتو المثلى داخل النطاق.'],
  }),
  makePreset({
    archetype: 'SPATIAL',
    modifier: 'BLIND_DIAGNOSIS',
    title: 'Latent Cavity Localization',
    tacticalSummary: ['Triangulate anomaly magnitude along exploded layer stack.', 'Explode layers with DirectorTrack compare view.'],
    quickBlitzEligible: true,
    timeLimitMs: 10000,
    parameters: [GAIN_PARAM, DAMPING_PARAM],
    targets: [{ name: 'focal_depth', expression: op('add', v('gain'), op('multiply', v('damping'), c(1.5))), lower: 8, upper: 12 }],
    enClauses: ['Separate nested anatomical rings in exploded mode.', 'Adjust spatial probe gain and depth attenuation.', 'Focus diagnostic beam precisely on target depth.'],
    arClauses: ['افصل الطبقات الهندسية المتداخلة لتحديد الموقع.', 'اضبط حساسية المجس وعمق التخميد المكاني.', 'وجّه حزمة التشخيص بدقة نحو العمق المستهدف.'],
  }),
  makePreset({
    archetype: 'SPATIAL',
    modifier: 'SURGICAL_REPAIR',
    title: 'Targeted Lattice Patch',
    tacticalSummary: ['Bridge severed node span with minimal overshoot.', 'Adjacent shells remain invariant under repair.'],
    quickBlitzEligible: true,
    timeLimitMs: 10000,
    parameters: [GAIN_PARAM, RESERVE_PARAM],
    targets: [{ name: 'bridge_span', expression: op('subtract', op('add', v('gain'), v('reserve')), c(2)), lower: 9, upper: 13 }],
    enClauses: ['Zoom directly onto fractured spatial junction.', 'Dial span width and anchor margin.', 'Seal lesion without distorting neighboring rings.'],
    arClauses: ['قرّب الرؤية مباشرة نحو الوصلة المتضررة.', 'حدد عرض الجسر الهندسي وهامش التثبيت.', 'أغلق الفجوة دون تشويه الحلقات المجاورة.'],
  }),
  makePreset({
    archetype: 'SPATIAL',
    modifier: 'CROSS_SECTION_SCAN',
    title: 'Axial Slice Tomography',
    tacticalSummary: ['Align cut-plane density index to expose hidden gradient.', 'Watch axial ring glow clarify interior gradient.'],
    quickBlitzEligible: true,
    timeLimitMs: 9000,
    parameters: [DAMPING_PARAM, RESERVE_PARAM],
    targets: [{ name: 'slice_density', expression: op('divide', op('multiply', v('reserve'), c(3)), v('damping')), lower: 12, upper: 16 }],
    enClauses: ['Sweep cross-sectional slice plane across core radius.', 'Tune reserve flux ratio against axial damping.', 'Capture sharp boundary contrast across layers.'],
    arClauses: ['مرر مستوى المقطع العرضي عبر نواة المجسم.', 'اضبط نسبة التدفق مقارنة بالتخميد المحوري.', 'التقط تباين الحدود بوضوح عبر الطبقات.'],
  }),
  makePreset({
    archetype: 'CONTRAST',
    modifier: 'STRESS_BENCHMARK',
    title: 'Chamber A/B Stress Delta',
    tacticalSummary: ['Amplify separation between chamber A and baseline B.', 'Observe side-by-side column symmetry breaking.'],
    quickBlitzEligible: true,
    timeLimitMs: 10000,
    parameters: [GAIN_PARAM, DAMPING_PARAM],
    targets: [{ name: 'delta_ratio', expression: op('subtract', op('multiply', v('gain'), c(2)), op('multiply', v('damping'), c(1))), lower: 8, upper: 12 }],
    enClauses: ['Compare twin chambers under identical excitation.', 'Widen divergence using primary gain vs damping.', 'Lock clear empirical separation across twin rigs.'],
    arClauses: ['قارن الحجرتين التوأم تحت التحفيز نفسه.', 'وسّع الفارق التشغيلي عبر ضبط الكسب والتخميد.', 'ثبّت التباين الواضح بين منظومتي الاختبار.'],
  }),
  makePreset({
    archetype: 'CONTRAST',
    modifier: 'BLIND_CHAMBER_ID',
    title: 'Unlabeled Chamber Discriminator',
    tacticalSummary: ['Classify unknown chamber via steady-state signature.', 'Avoid confounding oscillation near crossover.'],
    quickBlitzEligible: true,
    timeLimitMs: 9000,
    parameters: [GAIN_PARAM, RESERVE_PARAM],
    targets: [{ name: 'signature', expression: op('subtract', v('reserve'), op('multiply', v('gain'), c(0.5))), lower: 6, upper: 8.5 }],
    enClauses: ['Evaluate subtle baseline drift in chamber twin.', 'Probe separation coefficient via gain attenuation.', 'Isolate signature well clear of ambiguous zone.'],
    arClauses: ['راقب الانحراف الدقيق في الحجرة المجهولة.', 'اختبر معامل الفصل عبر تخفيف الكسب.', 'ميّز البصمة بعيداً عن منطقة الالتباس.'],
  }),
  makePreset({
    archetype: 'CONTRAST',
    modifier: 'CROSS_CALIBRATION',
    title: 'Dual Sensor Calibration',
    tacticalSummary: ['Bring both chambers onto common calibrated scale.', 'Zero out relative offset bias.'],
    quickBlitzEligible: false,
    timeLimitMs: 11000,
    parameters: [GAIN_PARAM, DAMPING_PARAM, RESERVE_PARAM],
    targets: [{ name: 'zeroed_gap', expression: op('subtract', op('add', v('gain'), v('damping')), v('reserve')), lower: 0.5, upper: 2.5 }],
    enClauses: ['Align twin scale references side by side.', 'Balance gain plus damping against reserve baseline.', 'Minimize calibration drift below residual limit.'],
    arClauses: ['وحّد مرجعية المقياسين جنباً إلى جنب.', 'عادل مجموع الكسب والتخميد أمام خط الاحتياطي.', 'قلص انحراف المعايرة دون الحد المسموح.'],
  }),
  makePreset({
    archetype: 'TRAP',
    modifier: 'FORENSIC_AUDIT',
    title: 'Adversarial Edge-Case Trap',
    tacticalSummary: ['Spot hidden failure regime lurking below steady average.', 'Raise damping margin before spike triggers trap.'],
    quickBlitzEligible: false,
    timeLimitMs: 12000,
    parameters: [GAIN_PARAM, DAMPING_PARAM],
    targets: [{ name: 'trap_margin', expression: op('subtract', op('multiply', v('damping'), c(3.5)), v('gain')), lower: 5, upper: 9 }],
    enClauses: ['Audit innocent-looking mean telemetry carefully.', 'Boost stabilizing damping to defuse latent singularity.', 'Eliminate failure overhang across worst-case corner.'],
    arClauses: ['دقق في القيم المتوسطة التي تخفي خللاً كامناً.', 'ارفع التخميد المستقر لإبطال الفخ الحسابي.', 'امنع الانهيار عند أسوأ حالة حدّية.'],
  }),
  makePreset({
    archetype: 'TRAP',
    modifier: 'COUNTEREXAMPLE_PROBE',
    title: 'Counterexample Boundary Probe',
    tacticalSummary: ['Force hidden assumption breach without zero-dividing.', 'Trap spike turns red to reveal broken invariant.'],
    quickBlitzEligible: false,
    timeLimitMs: 11000,
    parameters: [GAIN_PARAM, RESERVE_PARAM],
    targets: [{ name: 'witness', expression: op('subtract', op('multiply', v('gain'), c(1.5)), v('reserve')), lower: 1.5, upper: 4.5 }],
    enClauses: ['Probe upper edge of the naive monotonicity claim.', 'Push scaled gain marginally above reserve cushion.', 'Capture clean bounded counterexample witness.'],
    arClauses: ['اختبر الحد الأعلى للادعاء التبسيطي.', 'ادفع الكسب الموزون فوق وسادة الاحتياطي بقليل.', 'وثّق مثالاً مضاداً دقيقاً داخل النطاق.'],
  }),
  makePreset({
    archetype: 'TRAP',
    modifier: 'SAFEGUARD_PATCH',
    title: 'Invariant Safeguard Lock',
    tacticalSummary: ['Clamp overflow condition with positive guard rail.', 'Verify boundedness holds across slider range.'],
    quickBlitzEligible: true,
    timeLimitMs: 10000,
    parameters: [DAMPING_PARAM, RESERVE_PARAM],
    targets: [{ name: 'guard_level', expression: op('max', op('multiply', v('damping'), c(2.5)), v('reserve')), lower: 9, upper: 13 }],
    enClauses: ['Examine unguarded growth branch inside chamber.', 'Anchor reserve guardrail and positive damping multiplier.', 'Confirm guard level dominates spike cleanly.'],
    arClauses: ['افحص مسار النمو غير المقيد داخل المنظومة.', 'ثبّت حاجز الاحتياطي ومعامل التخميد الموجب.', 'تأكد من سيطرة الحاجز الوقائي على الذروة.'],
  }),
  makePreset({
    archetype: 'AXIOM',
    modifier: 'SPEED_BLITZ',
    title: 'Axiom Reflex Sprint',
    tacticalSummary: ['Lock fundamental invariant in under 6 seconds.', 'Eligible for AXIOM_PACK QUICK_BLITZ_CHAIN bursts.'],
    quickBlitzEligible: true,
    timeLimitMs: 6000,
    parameters: [GAIN_PARAM],
    targets: [{ name: 'invariant', expression: op('multiply', v('gain'), c(2)), lower: 10, upper: 14 }],
    enClauses: ['Read core identity crystal cleanly.', 'Snap single flow dial straight into green range.', 'Lock invariant fast to trigger chain momentum.'],
    arClauses: ['اقرأ بلورة البديهية الأساسية بسرعة.', 'وجّه المؤشر الوحيد مباشرة نحو المنطقة الخضراء.', 'ثبّت البديهية سريعاً لتفعيل سلسلة الإنجاز.'],
  }),
  makePreset({
    archetype: 'AXIOM',
    modifier: 'COROLLARY_CHAIN',
    title: 'Deductive Corollary Chain',
    tacticalSummary: ['Compose two elementary axioms in strict order.', 'Observe crystalline facet alignment.'],
    quickBlitzEligible: true,
    timeLimitMs: 8000,
    parameters: [GAIN_PARAM, DAMPING_PARAM],
    targets: [{ name: 'corollary', expression: op('add', op('multiply', v('gain'), c(1.5)), v('damping')), lower: 9, upper: 12.5 }],
    enClauses: ['Chain base lemma into immediate corollary.', 'Combine weighted premise with damping offset.', 'Align both crystal planes onto unified axis.'],
    arClauses: ['اربط التمهيد الأول بالنتيجة الفورية.', 'ادمج الفرضية الموزونة مع إزاحة التخميد.', 'وحّد مستويي البلورة على محور واحد.'],
  }),
  makePreset({
    archetype: 'AXIOM',
    modifier: 'BOUNDARY_TEST',
    title: 'Tight Lower/Upper Axiom Bound',
    tacticalSummary: ['Hold invariant inside razor-thin dual boundary.', 'Avoid boundary slack overshoot.'],
    quickBlitzEligible: true,
    timeLimitMs: 8000,
    parameters: [DAMPING_PARAM, RESERVE_PARAM],
    targets: [{ name: 'epsilon_fit', expression: op('subtract', v('reserve'), op('multiply', v('damping'), c(0.5))), lower: 6.5, upper: 8 }],
    enClauses: ['Inspect upper and lower boundary planes.', 'Trim reserve excess by half-weighted damping.', 'Thread crystal resonance cleanly through gap.'],
    arClauses: ['افحص مستويي الحد الأعلى والأدنى للبديهية.', 'اضبط فائض الاحتياطي بنصف معامل التخميد.', 'أدخل الرنين البلوري بدقة داخل الفجوة.'],
  }),
];

export const QUICK_BLITZ_CHAIN_SEQUENCE: readonly string[] = HOLOGRAM_CATALOG
  .filter(entry => entry.quickBlitzEligible)
  .slice(0, 4)
  .map(entry => entry.id);

export const DEFAULT_FORTRESS_STATE: FortressStateModel = {
  course_id: 'course-demo-m4',
  nodes: [
    {
      node_id: 'node-axiom-root',
      title: 'I. Conservation Axiom',
      grid_x: -2,
      grid_y: -1,
      tier: 3,
      state: 'ACTIVE',
      i_u_ticks: 9640,
      e_u_ticks: 9640,
      is_brownout: false,
      raid_immune: true,
      propagation_exempt: false,
      protection_reasons: ['POST_RAID_COOLDOWN'],
      archetype: 'AXIOM',
      modifier: 'SPEED_BLITZ',
    },
    {
      node_id: 'node-pipeline-core',
      title: 'II. Flow Pipeline',
      grid_x: 0,
      grid_y: -1,
      tier: 2,
      state: 'ACTIVE',
      i_u_ticks: 8420,
      e_u_ticks: 8420,
      is_brownout: false,
      raid_immune: false,
      propagation_exempt: false,
      protection_reasons: [],
      archetype: 'PIPELINE',
      modifier: 'REVERSE_ENG',
    },
    {
      node_id: 'node-tradeoff-brownout',
      title: 'III. Pareto Brownout',
      grid_x: 2,
      grid_y: -1,
      tier: 1,
      state: 'ACTIVE',
      i_u_ticks: 6820,
      e_u_ticks: 6410,
      is_brownout: true,
      raid_immune: false,
      propagation_exempt: false,
      protection_reasons: [],
      archetype: 'TRADEOFF',
      modifier: 'BUDGET_CAP',
    },
    {
      node_id: 'node-spatial-scan',
      title: 'IV. Spatial Cavity',
      grid_x: -1,
      grid_y: 1.4,
      tier: 2,
      state: 'ACTIVE',
      i_u_ticks: 8910,
      e_u_ticks: 8910,
      is_brownout: false,
      raid_immune: true,
      propagation_exempt: true,
      protection_reasons: ['NEW_NODE_GRACE'],
      archetype: 'SPATIAL',
      modifier: 'BLIND_DIAGNOSIS',
    },
    {
      node_id: 'node-contrast-twin',
      title: 'V. Twin Contrast',
      grid_x: 1,
      grid_y: 1.4,
      tier: 1,
      state: 'ACTIVE',
      i_u_ticks: 7950,
      e_u_ticks: 7420,
      is_brownout: false,
      raid_immune: true,
      propagation_exempt: true,
      protection_reasons: ['VERSION_GRACE'],
      archetype: 'CONTRAST',
      modifier: 'STRESS_BENCHMARK',
    },
    {
      node_id: 'node-trap-empty',
      title: 'VI. Trap Bastion (New)',
      grid_x: 2.5,
      grid_y: 1.4,
      tier: null,
      state: 'INACTIVE',
      i_u_ticks: null,
      e_u_ticks: null,
      is_brownout: false,
      raid_immune: true,
      propagation_exempt: true,
      protection_reasons: ['NEW_PLAYER_BUBBLE'],
      archetype: 'TRAP',
      modifier: 'FORENSIC_AUDIT',
    },
  ],
  edges: [
    { prerequisite_node_id: 'node-axiom-root', dependent_node_id: 'node-pipeline-core', weight: 0.65 },
    { prerequisite_node_id: 'node-pipeline-core', dependent_node_id: 'node-tradeoff-brownout', weight: 0.75 },
    { prerequisite_node_id: 'node-axiom-root', dependent_node_id: 'node-spatial-scan', weight: 0.5 },
    { prerequisite_node_id: 'node-tradeoff-brownout', dependent_node_id: 'node-contrast-twin', weight: 0.8 },
  ],
};

export function findHologramPreset(archetype: Archetype, modifier: string): HologramTemplatePreset {
  return (
    HOLOGRAM_CATALOG.find(item => item.archetype === archetype && item.modifier === modifier)
    ?? HOLOGRAM_CATALOG[0]!
  );
}

/**
 * Generates an inline playable binary WAV/Opus-style tone blob for client voiceover timing tests
 * without touching external URIs or third-party scripts (A-04 asset purity).
 */
export function buildSyntheticVoiceCueUri(durationMs: number, frequencyHz = 392): string {
  const sampleRate = 8000;
  const clampedMs = Math.max(400, Math.min(8000, durationMs));
  const samples = Math.floor((sampleRate * clampedMs) / 1000);
  const dataBytes = samples * 2;
  const buffer = new ArrayBuffer(44 + dataBytes);
  const view = new DataView(buffer);
  const writeStr = (offset: number, s: string) => {
    for (let i = 0; i < s.length; i += 1) view.setUint8(offset + i, s.charCodeAt(i));
  };
  writeStr(0, 'RIFF');
  view.setUint32(4, 36 + dataBytes, true);
  writeStr(8, 'WAVE');
  writeStr(12, 'fmt ');
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  writeStr(36, 'data');
  view.setUint32(40, dataBytes, true);
  for (let i = 0; i < samples; i += 1) {
    const t = i / sampleRate;
    const envelope = Math.min(1, i / 400) * Math.min(1, (samples - i) / 400);
    const sample = Math.sin(2 * Math.PI * frequencyHz * t) * 0.14 * envelope;
    view.setInt16(44 + i * 2, Math.round(sample * 32767), true);
  }
  const bytes = new Uint8Array(buffer);
  let bin = '';
  for (let i = 0; i < bytes.length; i += 1) bin += String.fromCharCode(bytes[i]!);
  return `data:audio/wav;base64,${btoa(bin)}`;
}
