/**
 * A-04 3D triangle/texture/buffer budget validator + O-05 < 28 FPS for 3.0s auto-fallback monitor.
 */

export const A04_LIMITS = {
  maxSceneTriangles: 15_000,
  maxAssetTriangles: 8_000,
  maxInstancedUnitTriangles: 2_500,
  maxTextureDimension: 2048,
  maxBufferBytes: 16 * 1024 * 1024,
} as const;

export const O05_FPS_THRESHOLD = 28;
export const O05_SUSTAINED_MS = 3000;
export const O05_RECOVER_FPS = 32;

export interface AssetBudgetCandidate {
  kind: 'asset' | 'instanced_unit' | 'scene';
  triangles: number;
  maxTextureWidth: number;
  maxTextureHeight: number;
  bufferBytes: number;
  uris?: readonly string[];
  hasExecutableScript?: boolean;
}

export interface BudgetValidationResult {
  ok: boolean;
  reasons: string[];
  triangles: number;
  triangleLimit: number;
}

export function isLocalOrInlineUri(uri: string): boolean {
  const trimmed = uri.trim().toLowerCase();
  if (trimmed === '') return false;
  if (trimmed.startsWith('data:') || trimmed.startsWith('blob:')) return true;
  if (trimmed.startsWith('/') || trimmed.startsWith('./') || trimmed.startsWith('../')) return true;
  if (trimmed.startsWith('http://localhost') || trimmed.startsWith('http://127.0.0.1')) return true;
  return !/^[a-z][a-z0-9+.-]*:/.test(trimmed);
}

export function validateAssetBudget(candidate: AssetBudgetCandidate): BudgetValidationResult {
  const reasons: string[] = [];
  const triangleLimit =
    candidate.kind === 'instanced_unit'
      ? A04_LIMITS.maxInstancedUnitTriangles
      : candidate.kind === 'asset'
        ? A04_LIMITS.maxAssetTriangles
        : A04_LIMITS.maxSceneTriangles;

  if (!Number.isFinite(candidate.triangles) || candidate.triangles < 0 || candidate.triangles > triangleLimit) {
    reasons.push(`triangle count ${candidate.triangles} exceeds A-04 limit ${triangleLimit}`);
  }
  if (
    candidate.maxTextureWidth > A04_LIMITS.maxTextureDimension
    || candidate.maxTextureHeight > A04_LIMITS.maxTextureDimension
  ) {
    reasons.push(
      `texture ${candidate.maxTextureWidth}x${candidate.maxTextureHeight} exceeds ${A04_LIMITS.maxTextureDimension}px`,
    );
  }
  if (candidate.bufferBytes < 0 || candidate.bufferBytes > A04_LIMITS.maxBufferBytes) {
    reasons.push(`buffer bytes ${candidate.bufferBytes} exceeds 16 MiB cap`);
  }
  if (candidate.hasExecutableScript) {
    reasons.push('embedded executable script forbidden by A-04');
  }
  for (const uri of candidate.uris ?? []) {
    if (!isLocalOrInlineUri(uri)) {
      reasons.push(`external URI blocked by A-04: ${uri}`);
    }
  }
  return {
    ok: reasons.length === 0,
    reasons,
    triangles: candidate.triangles,
    triangleLimit,
  };
}

export interface FrameMetricsSnapshot {
  fps: number;
  p50FrameMs: number;
  p95FrameMs: number;
  p99FrameMs: number;
  fallbackTriggered: boolean;
  lowFpsDurationMs: number;
  drawCalls: number;
  triangles: number;
}

function percentile(sorted: readonly number[], pct: number): number {
  if (sorted.length === 0) return 0;
  const idx = Math.min(sorted.length - 1, Math.max(0, Math.ceil((pct / 100) * sorted.length) - 1));
  return Number(sorted[idx]!.toFixed(2));
}

export class FpsHysteresisMonitor {
  private readonly frameTimesMs: number[] = [];
  private belowThresholdStartMs: number | null = null;
  private recoveryStartMs: number | null = null;
  private fallbackActive = false;
  private forcedLowFps = false;
  private lastDrawCalls = 0;
  private lastTriangles = 0;
  private smoothedFps = 60;

  setForcedLowFps(enabled: boolean): void {
    this.forcedLowFps = enabled;
  }

  reset(): void {
    this.frameTimesMs.length = 0;
    this.belowThresholdStartMs = null;
    this.recoveryStartMs = null;
    this.fallbackActive = false;
    this.forcedLowFps = false;
    this.smoothedFps = 60;
  }

  recordFrame(
    nowMs: number,
    deltaMs: number,
    renderStats?: { drawCalls: number; triangles: number },
  ): FrameMetricsSnapshot {
    const effectiveDelta = this.forcedLowFps ? Math.max(deltaMs, 48) : Math.max(1, Math.min(deltaMs, 250));
    if (renderStats) {
      this.lastDrawCalls = renderStats.drawCalls;
      this.lastTriangles = renderStats.triangles;
    }

    this.frameTimesMs.push(effectiveDelta);
    if (this.frameTimesMs.length > 180) this.frameTimesMs.shift();

    const instFps = 1000 / effectiveDelta;
    this.smoothedFps = this.frameTimesMs.length === 1
      ? instFps
      : this.smoothedFps * 0.72 + instFps * 0.28;

    let lowFpsDurationMs = 0;
    if (this.smoothedFps < O05_FPS_THRESHOLD) {
      this.recoveryStartMs = null;
      if (this.belowThresholdStartMs === null) {
        this.belowThresholdStartMs = nowMs;
      }
      lowFpsDurationMs = Math.max(0, nowMs - this.belowThresholdStartMs);
      if (lowFpsDurationMs >= O05_SUSTAINED_MS) {
        this.fallbackActive = true;
      }
    } else if (this.smoothedFps >= O05_RECOVER_FPS) {
      this.belowThresholdStartMs = null;
      if (this.fallbackActive) {
        if (this.recoveryStartMs === null) {
          this.recoveryStartMs = nowMs;
        }
      }
    } else if (this.belowThresholdStartMs !== null) {
      lowFpsDurationMs = Math.max(0, nowMs - this.belowThresholdStartMs);
    }

    const sorted = [...this.frameTimesMs].sort((a, b) => a - b);
    return {
      fps: Number(this.smoothedFps.toFixed(1)),
      p50FrameMs: percentile(sorted, 50),
      p95FrameMs: percentile(sorted, 95),
      p99FrameMs: percentile(sorted, 99),
      fallbackTriggered: this.fallbackActive,
      lowFpsDurationMs: Math.round(lowFpsDurationMs),
      drawCalls: this.lastDrawCalls,
      triangles: this.lastTriangles,
    };
  }
}
