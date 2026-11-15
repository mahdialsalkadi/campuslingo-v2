#!/usr/bin/env node
/**
 * G-2 WASM audit and P-06 mobile touch/UI acceptance verifier (360 CSS px,
 * discrete slider target zone via WASM evaluateJson, SourceRegion hotspots,
 * CLAUSE_CONCAT dir="auto" + prefers-reduced-motion, and negative <44px test).
 */
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { createServer } from 'node:http';
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const chromiumBin =
  process.env.CAMPUSLINGO_CHROMIUM_BIN ??
  '/home/mahdi/.cache/ms-playwright/chromium_headless_shell-1254/chrome-headless-shell-linux64/chrome-headless-shell';

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.js': 'application/javascript; charset=utf-8',
  '.mjs': 'application/javascript; charset=utf-8',
  '.wasm': 'application/wasm',
  '.json': 'application/json; charset=utf-8',
};

function whichExists(cmd) {
  try {
    execFileSync('which', [cmd], { stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
}

function measureProcessTreeRss(rootPid) {
  const pids = new Set([rootPid]);
  let expanded = true;
  while (expanded) {
    expanded = false;
    for (const name of readdirSync('/proc')) {
      if (!/^\d+$/.test(name)) continue;
      const pid = Number(name);
      if (pids.has(pid)) continue;
      try {
        const status = readFileSync(`/proc/${pid}/status`, 'utf8');
        const ppidMatch = status.match(/^PPid:\s+(\d+)/m);
        if (ppidMatch && pids.has(Number(ppidMatch[1]))) {
          pids.add(pid);
          expanded = true;
        }
      } catch {
        // exited cleanly
      }
    }
  }
  let sumRssBytes = 0;
  let sumHwmBytes = 0;
  for (const pid of pids) {
    try {
      const status = readFileSync(`/proc/${pid}/status`, 'utf8');
      const rssKb = Number(status.match(/^VmRSS:\s+(\d+)\s+kB/m)?.[1] ?? '0');
      const hwmKb = Number(status.match(/^VmHWM:\s+(\d+)\s+kB/m)?.[1] ?? '0');
      sumRssBytes += rssKb * 1024;
      sumHwmBytes += hwmKb * 1024;
    } catch {
      // exited cleanly
    }
  }
  return { processCount: pids.size, sumRssBytes, sumHwmBytes };
}

class CdpClient {
  constructor(wsUrl) {
    this.ws = new WebSocket(wsUrl);
    this.nextId = 1;
    this.pending = new Map();
    this.events = new Map();
    this.ready = new Promise((resolve, reject) => {
      this.ws.addEventListener('open', resolve, { once: true });
      this.ws.addEventListener('error', (e) => reject(new Error(`CDP WS error: ${e.message ?? 'unknown'}`)), { once: true });
    });
    this.ws.addEventListener('message', (msg) => {
      const data = JSON.parse(String(msg.data));
      if (data.id) {
        const slot = this.pending.get(data.id);
        if (slot) {
          this.pending.delete(data.id);
          if (data.error) slot.reject(new Error(`${data.error.code}: ${data.error.message}`));
          else slot.resolve(data.result);
        }
      } else if (data.method) {
        const listeners = this.events.get(data.method);
        if (listeners) {
          for (const cb of listeners) cb(data.params);
        }
      }
    });
  }

  async send(method, params = {}, sessionId = undefined) {
    await this.ready;
    const id = this.nextId++;
    const payload = sessionId ? { id, method, params, sessionId } : { id, method, params };
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.ws.send(JSON.stringify(payload));
    });
  }

  close() {
    this.ws.close();
  }
}

async function main() {
  const wasmPath = path.join(root, 'packages/engine/generated/campuslingo_domain_bg.wasm');
  const gluePath = path.join(root, 'packages/engine/generated/campuslingo_domain.js');
  const wasmBytes = statSync(wasmPath).size;
  const glueBytes = statSync(gluePath).size;
  const wasmOptPresent = whichExists('wasm-opt');
  const cargoTree = execFileSync(
    'cargo',
    ['tree', '-e', 'features', '-p', 'campuslingo-domain', '--target', 'wasm32-unknown-unknown'],
    { cwd: root, encoding: 'utf8' },
  ).trim();

  const publicDir = path.join(root, 'packages/game-client/public');
  const wasmDir = path.join(root, 'packages/engine/generated');
  const server = createServer((req, res) => {
    const cleanPath = (req.url ?? '/').split('?')[0];
    let fsPath;
    if (cleanPath.startsWith('/wasm/')) {
      fsPath = path.resolve(wasmDir, cleanPath.slice('/wasm/'.length));
      if (!fsPath.startsWith(wasmDir + path.sep)) {
        res.writeHead(403).end();
        return;
      }
    } else {
      const rel = cleanPath === '/' ? 'index.html' : cleanPath.replace(/^\/+/, '');
      fsPath = path.resolve(publicDir, rel);
      if (!fsPath.startsWith(publicDir + path.sep) && fsPath !== publicDir) {
        res.writeHead(403).end();
        return;
      }
    }
    try {
      const body = readFileSync(fsPath);
      const ext = path.extname(fsPath);
      res.writeHead(200, { 'Content-Type': MIME[ext] ?? 'application/octet-stream' });
      res.end(body);
    } catch {
      res.writeHead(404).end('not found');
    }
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const { port } = server.address();

  const userDataDir = mkdtempSync(path.join(tmpdir(), 'campuslingo-cdp-'));
  const chrome = spawn(
    chromiumBin,
    [
      '--headless=new',
      '--no-sandbox',
      '--disable-gpu',
      '--disable-dev-shm-usage',
      '--remote-debugging-port=0',
      `--user-data-dir=${userDataDir}`,
      'about:blank',
    ],
    { stdio: ['ignore', 'pipe', 'pipe'] },
  );

  let wsUrl = '';
  try {
    wsUrl = await new Promise((resolve, reject) => {
      const deadline = setTimeout(() => reject(new Error('Chromium DevTools timeout')), 15000);
      let errBuf = '';
      chrome.stderr.on('data', (chunk) => {
        errBuf += chunk.toString('utf8');
        const m = errBuf.match(/DevTools listening on (ws:\/\/[^\s]+)/);
        if (m) {
          clearTimeout(deadline);
          resolve(m[1]);
        }
      });
      chrome.once('exit', (code) => {
        clearTimeout(deadline);
        reject(new Error(`Chromium exited early (${code}): ${errBuf}`));
      });
    });

    const cdp = new CdpClient(wsUrl);
    const { targetId } = await cdp.send('Target.createTarget', { url: 'about:blank' });
    const { sessionId } = await cdp.send('Target.attachToTarget', { targetId, flatten: true });
    await cdp.send('Page.enable', {}, sessionId);
    await cdp.send('Runtime.enable', {}, sessionId);
    await cdp.send('Performance.enable', {}, sessionId);
    await cdp.send(
      'Emulation.setDeviceMetricsOverride',
      { width: 360, height: 780, deviceScaleFactor: 2, mobile: true },
      sessionId,
    );
    await cdp.send(
      'Emulation.setEmulatedMedia',
      { features: [{ name: 'prefers-reduced-motion', value: 'reduce' }] },
      sessionId,
    );

    const evalPage = async (expression) => {
      const out = await cdp.send(
        'Runtime.evaluate',
        { expression, awaitPromise: true, returnByValue: true },
        sessionId,
      );
      if (out.exceptionDetails) {
        throw new Error(`Browser exception: ${JSON.stringify(out.exceptionDetails)}`);
      }
      return out.result?.value;
    };

    const waitForReady = async () => {
      for (let i = 0; i < 60; i++) {
        const state = await evalPage('({ ready: Boolean(window.__campuslingoReady), err: window.__campuslingoError ?? null })');
        if (state?.err) throw new Error(`UI boot error: ${state.err}`);
        if (state?.ready) return;
        await new Promise((r) => setTimeout(r, 100));
      }
      throw new Error('Timed out waiting for __campuslingoReady');
    };

    // 1. P-06 + interactive WASM UI check at 360 CSS px mobile width
    await cdp.send('Page.navigate', { url: `http://127.0.0.1:${port}/index.html` }, sessionId);
    await waitForReady();

    const uiReport = await evalPage(`(() => {
      const incBtn = document.querySelector('[data-testid="inc-power"]');
      const badgeBefore = document.querySelector('[data-testid="target-badge"]').textContent;
      const wasmStatus = document.querySelector('[data-testid="wasm-status"]').textContent;
      const targetZoneRect = document.querySelector('[data-testid="target-zone"]').getBoundingClientRect();
      for (let i = 0; i < 5; i++) incBtn.click();
      const badgeAfter = document.querySelector('[data-testid="target-badge"]').textContent;
      const readoutAfter = document.querySelector('[data-testid="target-readout"]').textContent;

      document.querySelector('[data-testid="hotspot-1"]').click();
      const quoteAfterHotspot = document.querySelector('[data-testid="source-quote"]').textContent;
      const quoteDir = document.querySelector('[data-testid="source-quote"]').getAttribute('dir');

      document.querySelector('[data-testid="clause-btn-1"]').click();
      const subtitleAfter = document.querySelector('[data-testid="subtitle-banner"]').textContent;
      const subtitleDir = document.querySelector('[data-testid="subtitle-banner"]').getAttribute('dir');
      const directorAfter = document.querySelector('[data-testid="director-badge"]').textContent;
      const codecStatus = document.querySelector('[data-testid="codec-status"]').textContent;
      const reducedMotionTransition = getComputedStyle(document.querySelector('.card')).transitionDuration;

      const controls = [...document.querySelectorAll('button, input[type="range"]')];
      const rects = controls.map((el, i) => {
        const r = el.getBoundingClientRect();
        return {
          id: el.getAttribute('data-testid') || el.tagName + '-' + i,
          width: r.width,
          height: r.height,
          left: r.left,
          right: r.right,
        };
      });
      return {
        wasmStatus,
        badgeBefore,
        badgeAfter,
        readoutAfter,
        targetZoneWidth: targetZoneRect.width,
        quoteAfterHotspot,
        quoteDir,
        subtitleAfter,
        subtitleDir,
        directorAfter,
        codecStatus,
        reducedMotionTransition,
        docScrollWidth: document.documentElement.scrollWidth,
        viewportWidth: window.innerWidth,
        rects,
      };
    })()`);

    assert.match(uiReport.wasmStatus, /WASM verified: 11 states/);
    assert.equal(uiReport.badgeBefore, 'OUTSIDE TARGET');
    assert.equal(uiReport.badgeAfter, 'IN TARGET ZONE');
    assert.match(uiReport.readoutAfter, /output = 12/);
    assert.ok(uiReport.targetZoneWidth > 0, 'target-zone highlight bar must have non-zero width');
    assert.match(uiReport.quoteAfterHotspot, /^p\.2:/);
    assert.equal(uiReport.quoteDir, 'auto');
    assert.equal(uiReport.subtitleDir, 'auto');
    assert.match(uiReport.directorAfter, /close_up \(output\)/);
    assert.equal(uiReport.reducedMotionTransition, '0s');
    assert.ok(
      uiReport.docScrollWidth <= uiReport.viewportWidth,
      `mobile overflow: ${uiReport.docScrollWidth} > ${uiReport.viewportWidth}`,
    );
    for (const rect of uiReport.rects) {
      assert.ok(
        rect.width >= 44 && rect.height >= 44,
        `P-06 violation on ${rect.id}: ${rect.width}x${rect.height} < 44 CSS px`,
      );
      assert.ok(
        rect.left >= 0 && rect.right <= uiReport.viewportWidth + 0.5,
        `Control ${rect.id} extends outside 360px viewport: [${rect.left}, ${rect.right}]`,
      );
    }

    // 2. Automated negative test: ?compact-test=1 renders 24x24px controls -> MUST fail gate
    await cdp.send('Page.navigate', { url: `http://127.0.0.1:${port}/index.html?compact-test=1` }, sessionId);
    await waitForReady();
    const compactRects = await evalPage(`(() => [...document.querySelectorAll('button, input[type="range"]')].map((el, i) => {
      const r = el.getBoundingClientRect();
      return { id: el.getAttribute('data-testid') || 'ctrl-' + i, width: r.width, height: r.height };
    }))()`);
    const compactViolations = compactRects.filter((r) => r.width < 44 || r.height < 44);
    assert.ok(
      compactViolations.length > 0,
      'Negative test fixture must detect < 44 CSS px interactive targets',
    );

    // 3. G-2 Chromium RSS + JS heap measurement running ALL 190 WASM parity vectors
    const vectors = JSON.parse(
      readFileSync(path.join(root, 'tests/domain_vectors.json'), 'utf8'),
    );
    const nodeWasm = await import(pathToFileURL(gluePath).href);
    await nodeWasm.default({ module_or_path: readFileSync(wasmPath) });
    const expectedResponses = vectors.requests.map((req) =>
      nodeWasm.evaluateJson(JSON.stringify(req)),
    );

    await cdp.send('Page.navigate', { url: `http://127.0.0.1:${port}/index.html` }, sessionId);
    await waitForReady();

    const parityResult = await evalPage(`(async () => {
      const mod = await import('/wasm/campuslingo_domain.js');
      await mod.default();
      const requests = ${JSON.stringify(vectors.requests)};
      const expected = ${JSON.stringify(expectedResponses)};
      const t0 = performance.now();
      let parityMatches = 0;
      let valid = 0;
      let expectedErrors = 0;
      for (let i = 0; i < requests.length; i++) {
        const resRaw = mod.evaluateJson(JSON.stringify(requests[i]));
        if (resRaw === expected[i]) parityMatches++;
        const res = JSON.parse(resRaw);
        if (res.kind === 'error') expectedErrors++;
        else valid++;
      }
      return {
        total: requests.length,
        valid,
        expectedErrors,
        parityMatches,
        elapsedMs: Number((performance.now() - t0).toFixed(2)),
      };
    })()`);
    assert.equal(parityResult.total, vectors.requests.length);
    assert.equal(parityResult.parityMatches, vectors.requests.length);
    assert.ok(parityResult.valid > 0 && parityResult.expectedErrors > 0);

    const heapUsage = await cdp.send('Runtime.getHeapUsage', {}, sessionId);
    const { metrics } = await cdp.send('Performance.getMetrics', {}, sessionId);
    const metricMap = Object.fromEntries(metrics.map((m) => [m.name, m.value]));
    const procMem = measureProcessTreeRss(chrome.pid);

    const report = {
      observed_at: new Date().toISOString(),
      g2_wasm_audit: {
        wasm_path: 'packages/engine/generated/campuslingo_domain_bg.wasm',
        wasm_bytes: wasmBytes,
        js_glue_bytes: glueBytes,
        wasm_opt_present: wasmOptPresent,
        cargo_tree_wasm32: cargoTree,
        chromium_parity_vectors: parityResult,
        chromium_js_heap_used_bytes: heapUsage.usedSize,
        chromium_js_heap_total_bytes: heapUsage.totalSize,
        chromium_perf_metrics: {
          JSHeapUsedSize: metricMap.JSHeapUsedSize,
          JSHeapTotalSize: metricMap.JSHeapTotalSize,
        },
        chromium_process_tree_rss_bytes: procMem.sumRssBytes,
        chromium_process_tree_hwm_bytes: procMem.sumHwmBytes,
        chromium_process_count: procMem.processCount,
      },
      p06_mobile_ui: {
        viewport_css_px: uiReport.viewportWidth,
        doc_scroll_width_css_px: uiReport.docScrollWidth,
        interactive_controls_checked: uiReport.rects.length,
        min_observed_width_px: Math.min(...uiReport.rects.map((r) => r.width)),
        min_observed_height_px: Math.min(...uiReport.rects.map((r) => r.height)),
        negative_compact_test_violations: compactViolations.length,
        reduced_motion_transition_duration: uiReport.reducedMotionTransition,
        codec_status: uiReport.codecStatus,
      },
    };

    const outDir = path.join(root, 'artifacts/private');
    mkdirSync(outDir, { recursive: true, mode: 0o700 });
    writeFileSync(
      path.join(outDir, 'm3-g2-p06-audit.json'),
      JSON.stringify(report, null, 2) + '\n',
    );
    console.log(JSON.stringify(report, null, 2));
    cdp.close();
  } finally {
    try { chrome.kill('SIGKILL'); } catch {}
    server.close();
    rmSync(userDataDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
