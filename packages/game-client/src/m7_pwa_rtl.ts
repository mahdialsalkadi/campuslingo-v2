export interface OfflineRankedGuardResult {
  allowed: boolean;
  reason: string | null;
}

const RANKED_OR_MUTATION_PREFIXES = [
  '/v1/duels',
  '/v1/raids',
  '/v1/bosses',
  '/v1/leagues',
  '/v1/clans',
  '/v1/attempts',
  '/v1/notifications',
  '/v1/me/',
];

const SW_STATIC_PATHS = new Set([
  '/',
  '/index.html',
  '/styles.css',
  '/m4-shell.css',
  '/app.mjs',
  '/controller.js',
  '/shell.js',
  '/manifest.json',
  '/sw.js',
]);

function escapeHtml(text: string): string {
  return text
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}

/**
 * Wrap embedded mathematical formulas, equations, units, and ASCII technical tokens
 * inside `<bdi class="m7-math-ltr" dir="ltr">...</bdi>` so they render deterministically
 * left-to-right inside Arabic `dir="rtl"` containers.
 */
export function renderMixedArabicMathHtml(text: string): string {
  const mathTokenRe =
    /(?:[A-Za-z_][A-Za-z0-9_]*\s*(?:=|<=|>=|<|>|\+|-|\*|\/)\s*[A-Za-z0-9_.*+/()-]+(?:\s*[A-Za-z0-9_.*+/()-]+)*|[0-9]+(?:\.[0-9]+)?(?:\s*(?:ms|s|Hz|kHz|V|A|W|mV|bps|%|MB|GiB))?|[A-Za-z_][A-Za-z0-9_]{1,}(?:\.[A-Za-z0-9_]+)*)/g;

  let out = '';
  let lastIndex = 0;
  for (const match of text.matchAll(mathTokenRe)) {
    const idx = match.index ?? 0;
    const token = match[0];
    if (idx > lastIndex) {
      out += escapeHtml(text.slice(lastIndex, idx));
    }
    out += `<bdi class="m7-math-ltr" dir="ltr">${escapeHtml(token)}</bdi>`;
    lastIndex = idx + token.length;
  }
  if (lastIndex < text.length) {
    out += escapeHtml(text.slice(lastIndex));
  }
  return out;
}

/**
 * Restrict service-worker offline caching strictly to static app shell & content
 * assets; never cache `/v1/*` API responses or ranked submissions.
 */
export function isSwCacheableAsset(rawUrl: string, method = 'GET'): boolean {
  if (method.toUpperCase() !== 'GET') {
    return false;
  }
  let pathname = rawUrl;
  try {
    pathname = new URL(rawUrl, 'https://campuslingo.local').pathname;
  } catch {
    return false;
  }
  if (pathname.startsWith('/v1/')) {
    return false;
  }
  if (SW_STATIC_PATHS.has(pathname)) {
    return true;
  }
  return (
    pathname.startsWith('/wasm/') ||
    pathname.startsWith('/icons/') ||
    pathname.startsWith('/assets/content/') ||
    pathname.endsWith('.opus') ||
    pathname.endsWith('.wasm') ||
    pathname.endsWith('.svg')
  );
}

/**
 * Enforce that offline clients can never enqueue or report ranked duel/raid/boss
 * outcomes as trusted offline wins.
 */
export function guardOfflineRankedMutation(
  isOnline: boolean,
  endpoint: string,
): OfflineRankedGuardResult {
  if (isOnline) {
    return { allowed: true, reason: null };
  }
  const normalized = endpoint.startsWith('http')
    ? new URL(endpoint).pathname
    : endpoint.split('?')[0] ?? endpoint;
  if (RANKED_OR_MUTATION_PREFIXES.some((prefix) => normalized.startsWith(prefix))) {
    return {
      allowed: false,
      reason: 'offline_ranked_outcome_prohibited',
    };
  }
  return { allowed: true, reason: null };
}

export interface M7PwaRtlPanelController {
  destroy(): void;
  setSimulatedOffline(offline: boolean): void;
  getSampleHtml(): string;
}

const DEFAULT_RTL_SAMPLE =
  'تعمل وحدة التحكم وفق المعادلة W_eff = 1.25 * W_base بفاصل 60 ms وحد أقصى 12.5 V دون فيض.';

export function mountM7PwaRtlPanel(host: HTMLElement): M7PwaRtlPanelController {
  let rtlActive = true;
  let simulatedOffline = false;

  host.classList.add('m7-pwa-panel');
  host.innerHTML = `
    <div class="m4-subhead">
      <h3>M7 Arabic RTL/Bidi &amp; Mobile PWA</h3>
      <span class="badge-chip badge-safe" id="m7-pwa-badge" data-testid="m7-pwa-badge">PWA · Safe Area · Cap 2/day</span>
    </div>
    <p
      id="m7-rtl-preview"
      class="m7-rtl-preview"
      data-testid="m7-rtl-preview"
      dir="rtl"
      lang="ar"
    ></p>
    <div class="m7-pwa-actions">
      <button type="button" class="touch-btn" id="m7-rtl-dir-btn" data-testid="m7-rtl-dir-btn">Direction: RTL</button>
      <button type="button" class="touch-btn" id="m7-offline-guard-btn" data-testid="m7-offline-guard-btn">Ranked Guard: Online</button>
    </div>
    <p id="m7-pwa-status" class="m4-muted-note" data-testid="m7-pwa-status"></p>
  `;

  const previewEl = host.querySelector<HTMLElement>('#m7-rtl-preview')!;
  const dirBtn = host.querySelector<HTMLButtonElement>('#m7-rtl-dir-btn')!;
  const guardBtn = host.querySelector<HTMLButtonElement>('#m7-offline-guard-btn')!;
  const statusEl = host.querySelector<HTMLElement>('#m7-pwa-status')!;

  const sync = () => {
    previewEl.dir = rtlActive ? 'rtl' : 'ltr';
    previewEl.lang = rtlActive ? 'ar' : 'en';
    previewEl.innerHTML = renderMixedArabicMathHtml(DEFAULT_RTL_SAMPLE);
    dirBtn.textContent = rtlActive ? 'Direction: RTL' : 'Direction: LTR';
    dirBtn.setAttribute('aria-pressed', rtlActive ? 'true' : 'false');

    const duelCheck = guardOfflineRankedMutation(!simulatedOffline, '/v1/duels/live/submit');
    guardBtn.textContent = simulatedOffline
      ? 'Ranked Guard: Offline Blocked'
      : 'Ranked Guard: Online';
    guardBtn.setAttribute('aria-pressed', simulatedOffline ? 'true' : 'false');
    statusEl.textContent = duelCheck.allowed
      ? 'Quiet hours 22:00–08:00 local · SW caches static shell only'
      : `Blocked (${duelCheck.reason}): offline ranked outcomes disabled`;
  };

  const onDirClick = () => {
    rtlActive = !rtlActive;
    sync();
  };
  const onGuardClick = () => {
    simulatedOffline = !simulatedOffline;
    sync();
  };

  dirBtn.addEventListener('click', onDirClick);
  guardBtn.addEventListener('click', onGuardClick);
  sync();

  return {
    setSimulatedOffline(offline: boolean) {
      simulatedOffline = offline;
      sync();
    },
    getSampleHtml() {
      return previewEl.innerHTML;
    },
    destroy() {
      dirBtn.removeEventListener('click', onDirClick);
      guardBtn.removeEventListener('click', onGuardClick);
    },
  };
}
