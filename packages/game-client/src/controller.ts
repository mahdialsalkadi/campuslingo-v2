import type {
  AudioManifest,
  LearningUnitBundle,
  SandboxSpec,
} from '../../contracts/src/index.js';

export type ParameterSpec = SandboxSpec['parameters'][number];
export type SourceRegion = LearningUnitBundle['source_regions'][number];
export type DirectorTrack = LearningUnitBundle['director_tracks'][number];
export type AudioWord = AudioManifest['words'][number];

export interface TargetEvaluation {
  name: string;
  value: number;
  lower: number;
  upper: number;
  in_bounds: boolean;
}

export interface SandboxStateResult {
  solved: boolean;
  targets: TargetEvaluation[];
}

export interface SandboxValidationResult {
  valid: boolean;
  reason: string;
  states_checked: number;
  witness: Record<string, number> | null;
  proof_domain: string;
}

export interface DiscreteStepPoint {
  value: number;
  pct: number;
  inTarget: boolean;
}

export interface TouchTargetRect {
  id: string;
  width: number;
  height: number;
  left?: number;
  right?: number;
}

export interface TouchGeometryReport {
  ok: boolean;
  minRequiredPx: number;
  viewportWidth: number;
  docScrollWidth: number;
  failures: string[];
}

export const MIN_MOBILE_TOUCH_TARGET_PX = 44;

export type EvaluateJsonFn = (rawJson: string) => string;

/** Quantize a numeric slider value onto a ParameterSpec's discrete step grid. */
export function quantizeParameterStep(
  param: ParameterSpec,
  candidate: number,
): number {
  if (!Number.isFinite(candidate)) {
    return param.initial;
  }
  const steps = Math.max(
    1,
    Math.round((param.maximum - param.minimum) / param.step),
  );
  const rawIdx = Math.round((candidate - param.minimum) / param.step);
  const clampedIdx = Math.max(0, Math.min(steps, rawIdx));
  const value = param.minimum + clampedIdx * param.step;
  return Number(Math.min(param.maximum, value).toFixed(6));
}

/** Verify the entire SandboxSpec via canonical WASM `evaluateJson`. */
export function verifySandboxViaWasm(
  evaluateJson: EvaluateJsonFn,
  spec: SandboxSpec,
): SandboxValidationResult {
  const parsed = JSON.parse(
    evaluateJson(JSON.stringify({ kind: 'sandbox', spec })),
  ) as {
    kind: string;
    result?: SandboxValidationResult;
    error?: string;
  };
  if (parsed.kind !== 'sandbox' || !parsed.result) {
    throw new Error(parsed.error ?? 'WASM sandbox verification failed');
  }
  return parsed.result;
}

/** Evaluate concrete parameter assignments via canonical WASM `evaluateJson`. */
export function evaluateStateViaWasm(
  evaluateJson: EvaluateJsonFn,
  spec: SandboxSpec,
  values: Record<string, number>,
): SandboxStateResult {
  const parsed = JSON.parse(
    evaluateJson(JSON.stringify({ kind: 'sandbox_state', spec, values })),
  ) as {
    kind: string;
    result?: SandboxStateResult;
    error?: string;
  };
  if (parsed.kind !== 'sandbox_state' || !parsed.result) {
    throw new Error(parsed.error ?? 'WASM state evaluation failed');
  }
  return parsed.result;
}

/** Compute highlighted `[lower, upper]` zone geometry along one parameter track. */
export function computeParameterTargetBand(
  evaluateJson: EvaluateJsonFn,
  spec: SandboxSpec,
  param: ParameterSpec,
  baseValues: Record<string, number>,
): DiscreteStepPoint[] {
  const totalSpan = Math.max(1e-9, param.maximum - param.minimum);
  const stepCount = Math.min(
    256,
    Math.max(1, Math.round(totalSpan / param.step)),
  );
  const points: DiscreteStepPoint[] = [];
  for (let i = 0; i <= stepCount; i++) {
    const val = quantizeParameterStep(param, param.minimum + i * param.step);
    const evalRes = evaluateStateViaWasm(evaluateJson, spec, {
      ...baseValues,
      [param.name]: val,
    });
    const pct = Math.max(
      0,
      Math.min(100, ((val - param.minimum) / totalSpan) * 100),
    );
    points.push({ value: val, pct, inTarget: evalRes.solved });
  }
  return points;
}

/** Resolve the active clause subtitle at `currentMs` (`P-07` `CLAUSE_CONCAT`). */
export function findActiveClause(
  manifest: AudioManifest | null | undefined,
  currentMs: number,
): { index: number; word: AudioWord | null } {
  if (!manifest || manifest.words.length === 0) {
    return { index: -1, word: null };
  }
  const clamped = Math.max(0, currentMs);
  for (let i = 0; i < manifest.words.length; i++) {
    const word = manifest.words[i];
    if (word && clamped >= word.start_ms && clamped <= word.end_ms) {
      return { index: i, word };
    }
  }
  const nextIdx = manifest.words.findIndex((w) => w.start_ms >= clamped);
  if (nextIdx >= 0) {
    return { index: nextIdx, word: manifest.words[nextIdx] ?? null };
  }
  const lastIdx = manifest.words.length - 1;
  return { index: lastIdx, word: manifest.words[lastIdx] ?? null };
}

/** Resolve the synchronized visual DirectorTrack cue at `currentMs`. */
export function findActiveDirectorTrack(
  tracks: DirectorTrack[],
  currentMs: number,
): DirectorTrack | null {
  if (tracks.length === 0) {
    return null;
  }
  const clamped = Math.max(0, currentMs);
  return (
    tracks.find((t) => clamped >= t.start_ms && clamped <= t.end_ms) ??
    tracks[0] ??
    null
  );
}

/** Detect native browser `.opus` codec support using a media-capability probe. */
export function probeOpusSupport(
  canPlayType: ((mime: string) => string) | null | undefined,
): { supported: boolean; fallbackReason: string | null } {
  if (!canPlayType) {
    return { supported: false, fallbackReason: 'audio-element-unavailable' };
  }
  const ogg = canPlayType('audio/ogg; codecs="opus"');
  const opus = canPlayType('audio/opus');
  const webm = canPlayType('audio/webm; codecs="opus"');
  if (ogg !== '' || opus !== '' || webm !== '') {
    return { supported: true, fallbackReason: null };
  }
  return { supported: false, fallbackReason: 'opus-codec-unsupported-using-clock' };
}

/** Enforce `P-06` mobile touch geometry (`>= 44 CSS px` width & height, zero overflow). */
export function validateTouchTargetRects(
  rects: TouchTargetRect[],
  viewportWidth: number,
  docScrollWidth: number,
  minRequiredPx: number = MIN_MOBILE_TOUCH_TARGET_PX,
): TouchGeometryReport {
  const failures: string[] = [];
  if (docScrollWidth > viewportWidth) {
    failures.push(
      `horizontal overflow detected: scrollWidth=${docScrollWidth} > viewportWidth=${viewportWidth}`,
    );
  }
  if (rects.length === 0) {
    failures.push('no interactive touch targets measured');
  }
  for (const rect of rects) {
    if (rect.width < minRequiredPx || rect.height < minRequiredPx) {
      failures.push(
        `target ${rect.id} measured ${rect.width.toFixed(1)}x${rect.height.toFixed(1)} CSS px (< ${minRequiredPx}px)`,
      );
    }
    if (
      typeof rect.left === 'number' &&
      typeof rect.right === 'number' &&
      (rect.left < 0 || rect.right > viewportWidth + 0.5)
    ) {
      failures.push(
        `target ${rect.id} overflows viewport horizontally [${rect.left.toFixed(1)}, ${rect.right.toFixed(1)}]`,
      );
    }
  }
  return {
    ok: failures.length === 0,
    minRequiredPx,
    viewportWidth,
    docScrollWidth,
    failures,
  };
}
