#!/usr/bin/env node
/**
 * M4 Headless Chromium WebGL2 3D Strategy Shell & Responsive Accessibility Audit.
 * Verifies:
 *  - WebGL2 Three.js 3D Fortress & Concept Hologram mount + A-04 <= 15,000 scene / <= 2,500 unit tris
 *  - All 4 WorldThemes + 6 Archetypes x 3 Modifiers (18 pairs) + QUICK_BLITZ_CHAIN
 *  - DirectorTrack camera framing + synced .opus Arabic/English dir="auto" captions + Mute (Zero Advantage)
 *  - O-05 forced < 28 FPS for 3s auto-fallback to ConceptSandbox2D + manual 2D/3D toggle
 *  - S-13 3-report quarantine UI counter
 *  - P-06 >= 44x44 CSS px touch targets + zero horizontal overflow across 360px, 768px & 1280px
 */

import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const chromiumBin =
  '/home/mahdi/.cache/ms-playwright/chromium_headless_shell-1254/chrome-headless-shell-linux64/chrome-headless-shell';

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.wasm': 'application/wasm',
  '.json': 'application/json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.webmanifest': 'application/manifest+json',
};

class CdpClient {
  constructor(wsUrl) {
    this.ws = new WebSocket(wsUrl);
    this.nextId = 1;
    this.pending = new Map();
    this.browserExceptions = [];
    this.ready = new Promise((resolve, reject) => {
      this.ws.addEventListener('open', resolve, { once: true });
      this.ws.addEventListener('error', e => reject(new Error(`CDP WS error: ${e.message ?? 'unknown'}`)), { once: true });
    });
    this.ws.addEventListener('message', msg => {
      const data = JSON.parse(String(msg.data));
      if (data.id && this.pending.has(data.id)) {
        const { resolve, reject } = this.pending.get(data.id);
        this.pending.delete(data.id);
        if (data.error) reject(new Error(JSON.stringify(data.error)));
        else resolve(data.result);
      } else if (data.method === 'Runtime.exceptionThrown') {
        const desc = data.params?.exceptionDetails?.exception?.description ?? data.params?.exceptionDetails?.text ?? 'Unknown exception';
        this.browserExceptions.push(desc);
      }
    });
  }

  async send(method, params = {}, sessionId = undefined) {
    await this.ready;
    const id = this.nextId++;
    const payload = { id, method, params };
    if (sessionId) payload.sessionId = sessionId;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.ws.send(JSON.stringify(payload));
    });
  }

  close() {
    this.ws.close();
  }
}

async function runAudit() {
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
      res.writeHead(200, { 'Content-Type': MIME[path.extname(fsPath)] ?? 'application/octet-stream' });
      res.end(body);
    } catch {
      res.writeHead(404).end('not found');
    }
  });

  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const { port } = server.address();
  const userDataDir = mkdtempSync(path.join(tmpdir(), 'campuslingo-m4-cdp-'));

  const chrome = spawn(
    chromiumBin,
    [
      '--headless=new',
      '--no-sandbox',
      '--enable-webgl',
      '--use-gl=angle',
      '--use-angle=swiftshader',
      '--enable-unsafe-swiftshader',
      '--ignore-gpu-blocklist',
      '--disable-dev-shm-usage',
      '--remote-debugging-port=0',
      `--user-data-dir=${userDataDir}`,
      'about:blank',
    ],
    { stdio: ['ignore', 'pipe', 'pipe'] },
  );

  try {
    const wsUrl = await new Promise((resolve, reject) => {
      const deadline = setTimeout(() => reject(new Error('Chromium DevTools timeout')), 15000);
      let errBuf = '';
      chrome.stderr.on('data', chunk => {
        errBuf += chunk.toString('utf8');
        const m = errBuf.match(/DevTools listening on (ws:\/\/[^\s]+)/);
        if (m) {
          clearTimeout(deadline);
          resolve(m[1]);
        }
      });
      chrome.once('exit', code => {
        clearTimeout(deadline);
        reject(new Error(`Chromium exited (${code}): ${errBuf}`));
      });
    });

    const cdp = new CdpClient(wsUrl);
    const { targetId } = await cdp.send('Target.createTarget', { url: 'about:blank' });
    const { sessionId } = await cdp.send('Target.attachToTarget', { targetId, flatten: true });
    await cdp.send('Page.enable', {}, sessionId);
    await cdp.send('Runtime.enable', {}, sessionId);
    await cdp.send('Emulation.setDeviceMetricsOverride', {
      width: 1280,
      height: 820,
      deviceScaleFactor: 1,
      mobile: false,
    }, sessionId);

    const evalPage = async expression => {
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

    await cdp.send('Page.navigate', { url: `http://127.0.0.1:${port}/index.html` }, sessionId);
    try {
      await evalPage(`new Promise((res, rej) => {
        const start = Date.now();
        const poll = () => {
          const s = window.__CAMPUSLINGO_SHELL__?.snapshot;
          if (s && s.telemetry && s.wasmVerified) return res(true);
          if (Date.now() - start > 10000) return rej(new Error('M4 shell init timeout'));
          setTimeout(poll, 60);
        };
        poll();
      })`);
    } catch (err) {
      if (cdp.browserExceptions.length > 0) {
        throw new Error(`${err.message}; browserExceptions=${JSON.stringify(cdp.browserExceptions)}`);
      }
      throw err;
    }
    assert.equal(cdp.browserExceptions.length, 0, `Unexpected browser exceptions: ${JSON.stringify(cdp.browserExceptions)}`);

    const initialReport = await evalPage(`(() => {
      const s = window.__CAMPUSLINGO_SHELL__.snapshot;
      const paneRect = document.querySelector('.m4-viewport-pane').getBoundingClientRect();
      const deckRect = document.querySelector('.m4-control-deck').getBoundingClientRect();
      return {
        rendererBackend: s.telemetry.rendererBackend,
        sceneTriangles: s.telemetry.budget.triangles,
        budgetOk: s.telemetry.budget.ok,
        unitTriangleMax: s.telemetry.unitTriangleMax,
        paneWidth: paneRect.width,
        deckWidth: deckRect.width,
        fps: s.telemetry.frame.fps,
      };
    })()`);

    assert.equal(initialReport.rendererBackend, 'webgl', 'SwiftShader WebGL2 should mount cleanly');
    assert.equal(initialReport.budgetOk, true);
    assert.ok(initialReport.sceneTriangles > 0 && initialReport.sceneTriangles <= 15000);
    assert.ok(initialReport.unitTriangleMax > 0 && initialReport.unitTriangleMax <= 2500);
    assert.ok(
      initialReport.paneWidth > initialReport.deckWidth,
      `Desktop 65/35 layout expected viewport (${initialReport.paneWidth}) > deck (${initialReport.deckWidth})`,
    );

    // Cycle all 4 WorldThemes and all 6x3=18 pairs + QUICK_BLITZ_CHAIN + DirectorTrack + AR/EN captions + Mute
    const interactionSummary = await evalPage(`(async () => {
      const shell = window.__CAMPUSLINGO_SHELL__;
      const themesVisited = [];
      for (const theme of ['BIO_SYSTEM', 'CYBER_GRID', 'MACRO_POLIS', 'ARCANE_ARCHIVE']) {
        document.querySelector('[data-testid="m4-theme-' + theme + '"]').click();
        await new Promise(r => setTimeout(r, 40));
        themesVisited.push(document.getElementById('m4-game-shell').className);
      }

      const templatesChecked = [];
      const archButtons = [...document.querySelectorAll('.m4-arch-btn')];
      for (const archBtn of archButtons) {
        archBtn.click();
        await new Promise(r => setTimeout(r, 30));
        const modButtons = [...document.querySelectorAll('.m4-mod-btn')];
        for (const modBtn of modButtons) {
          modBtn.click();
          await new Promise(r => setTimeout(r, 30));
          const s = shell.snapshot;
          templatesChecked.push({
            pair: s.archetype + ':' + s.modifier,
            wasmVerified: s.wasmVerified,
            tris: s.telemetry?.budget?.triangles ?? -1,
            budgetOk: s.telemetry?.budget?.ok ?? false,
          });
        }
      }

      document.querySelector('[data-testid="m4-quick-blitz-btn"]').click();
      await new Promise(r => setTimeout(r, 40));
      const blitzActive = shell.snapshot.quickBlitzActive;

      document.querySelector('[data-testid="m4-lang-toggle"]').click();
      document.querySelector('[data-testid="m4-clause-1"]').click();
      document.querySelector('[data-testid="m4-audio-mute"]').click();
      await new Promise(r => setTimeout(r, 40));
      const arSnap = {
        lang: shell.snapshot.language,
        muted: shell.snapshot.muted,
        captionDir: document.querySelector('[data-testid="m4-caption-text"]').getAttribute('dir'),
        captionText: document.querySelector('[data-testid="m4-caption-text"]').textContent,
        directorText: document.querySelector('[data-testid="m4-director-readout"]').textContent,
      };

      // S-13 3-report quarantine counter
      document.querySelector('[data-testid="m4-report-btn"]').click();
      document.querySelector('[data-testid="m4-report-btn"]').click();
      document.querySelector('[data-testid="m4-report-btn"]').click();
      const reportStatus = document.querySelector('[data-testid="m4-report-status"]').textContent;

      return { themesVisited, templatesChecked, blitzActive, arSnap, reportStatus };
    })()`);

    assert.equal(interactionSummary.themesVisited.length, 4);
    assert.ok(interactionSummary.themesVisited.every(c => c.includes('theme-')));
    assert.equal(interactionSummary.templatesChecked.length, 18);
    assert.equal(
      new Set(interactionSummary.templatesChecked.map(item => item.pair)).size,
      18,
      `Expected 18 distinct Archetype x Modifier pairs, got ${JSON.stringify(interactionSummary.templatesChecked.map(i => i.pair))}`,
    );
    for (const item of interactionSummary.templatesChecked) {
      assert.equal(item.wasmVerified, true, `${item.pair} WASM failed`);
      assert.equal(item.budgetOk, true, `${item.pair} exceeded A-04`);
      assert.ok(item.tris > 0 && item.tris <= 15000, `${item.pair} tris=${item.tris}`);
    }
    assert.equal(interactionSummary.blitzActive, true);
    assert.equal(interactionSummary.arSnap.lang, 'ar');
    assert.equal(interactionSummary.arSnap.muted, true);
    assert.equal(interactionSummary.arSnap.captionDir, 'rtl');
    assert.match(interactionSummary.arSnap.directorText, /close_up \(input\)/);
    assert.match(interactionSummary.reportStatus, /QUARANTINED/);

    // O-05 < 28 FPS sustained 3.0s auto-fallback test
    await evalPage(`document.querySelector('[data-testid="m4-stress-fps-btn"]').click();`);
    await evalPage(`new Promise((res, rej) => {
      const start = Date.now();
      const poll = () => {
        const s = window.__CAMPUSLINGO_SHELL__.snapshot;
        if (s.mode === 'AUTO_FALLBACK_2D') return res(true);
        if (Date.now() - start > 7000) return rej(new Error('O-05 3s fallback timeout: mode=' + s?.mode));
        setTimeout(poll, 120);
      };
      poll();
    })`);
    const fallbackReport = await evalPage(`({
      mode: window.__CAMPUSLINGO_SHELL__.snapshot.mode,
      hidden3d: document.querySelector('[data-testid="m4-3d-stage"]').hidden,
      hidden2d: document.querySelector('[data-testid="m4-2d-stage"]').hidden,
      svgVisible: Boolean(document.querySelector('.fallback-dag-svg')),
    })`);
    assert.equal(fallbackReport.mode, 'AUTO_FALLBACK_2D');
    assert.equal(fallbackReport.hidden3d, true);
    assert.equal(fallbackReport.hidden2d, false);
    assert.equal(fallbackReport.svgVisible, true);

    // Recover manually back to 3D mode
    await evalPage(`document.querySelector('[data-testid="m4-fallback-toggle"]').click();`);
    const recoveredMode = await evalPage(`window.__CAMPUSLINGO_SHELL__.snapshot.mode`);
    assert.equal(recoveredMode, '3D_ACTIVE');

    // P-06 responsive touch targets >= 44 CSS px & zero horizontal overflow at 360px, 768px, 1280px
    const viewports = [
      { name: 'mobile-360', width: 360, height: 780, mobile: true },
      { name: 'tablet-768', width: 768, height: 1024, mobile: true },
      { name: 'desktop-1280', width: 1280, height: 820, mobile: false },
    ];
    const layoutResults = {};
    for (const vp of viewports) {
      await cdp.send('Emulation.setDeviceMetricsOverride', {
        width: vp.width,
        height: vp.height,
        deviceScaleFactor: 1,
        mobile: vp.mobile,
      }, sessionId);
      await evalPage(`new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))`);
      const layout = await evalPage(`(() => {
        const controls = [...document.querySelectorAll('#m4-game-shell button, #m4-game-shell input[type="range"]')]
          .filter(el => el.offsetParent !== null);
        const violations = controls
          .map((el, i) => {
            const r = el.getBoundingClientRect();
            return { id: el.getAttribute('data-testid') || 'ctrl-' + i, w: r.width, h: r.height };
          })
          .filter(c => c.w < 44 || c.h < 44);
        return {
          controlCount: controls.length,
          violations,
          scrollWidth: document.documentElement.scrollWidth,
          innerWidth: window.innerWidth,
        };
      })()`);
      assert.equal(layout.violations.length, 0, `${vp.name} P-06 violations: ${JSON.stringify(layout.violations)}`);
      assert.ok(
        layout.scrollWidth <= layout.innerWidth,
        `${vp.name} horizontal overflow: scrollWidth=${layout.scrollWidth} > innerWidth=${layout.innerWidth}`,
      );
      layoutResults[vp.name] = layout;
    }

    // M7 RTL/Bidi mixed-math isolation & PWA manifest/service-worker verification
    const m7PwaRtl = await evalPage(`(async () => {
      const preview = document.querySelector('[data-testid="m7-rtl-preview"]');
      const bdi = preview ? preview.querySelector('.m7-math-ltr') : null;
      const previewStyle = preview ? getComputedStyle(preview) : null;
      const bdiStyle = bdi ? getComputedStyle(bdi) : null;
      const manifestResp = await fetch('./manifest.json');
      const manifestJson = manifestResp.ok ? await manifestResp.json() : null;
      const swResp = await fetch('./sw.js');
      const swText = swResp.ok ? await swResp.text() : '';
      return {
        previewDirAttr: preview?.getAttribute('dir') ?? null,
        previewComputedDir: previewStyle?.direction ?? null,
        bdiText: bdi?.textContent ?? null,
        bdiComputedDir: bdiStyle?.direction ?? null,
        bdiUnicodeBidi: bdiStyle?.unicodeBidi ?? null,
        manifestOk: manifestResp.ok && manifestJson?.display === 'standalone',
        swOk: swResp.ok && swText.includes("/v1/"),
        viewportMeta: document.querySelector('meta[name="viewport"]')?.getAttribute('content') ?? '',
      };
    })()`);
    assert.equal(m7PwaRtl.previewDirAttr, 'rtl');
    assert.equal(m7PwaRtl.previewComputedDir, 'rtl');
    assert.equal(m7PwaRtl.bdiComputedDir, 'ltr');
    assert.equal(m7PwaRtl.bdiUnicodeBidi, 'isolate');
    assert.equal(m7PwaRtl.manifestOk, true);
    assert.equal(m7PwaRtl.swOk, true);
    assert.ok(m7PwaRtl.viewportMeta.includes('viewport-fit=cover'));

    const outDir = path.join(root, 'artifacts/private');
    mkdirSync(outDir, { recursive: true });
    const artifactPath = path.join(outDir, 'm4-ui-audit.json');
    const summary = {
      status: 'ok',
      initialReport,
      themesVisitedCount: interactionSummary.themesVisited.length,
      templatesVerifiedCount: interactionSummary.templatesChecked.length,
      blitzActive: interactionSummary.blitzActive,
      arabicClauseChecked: interactionSummary.arSnap,
      fallbackReport,
      recoveredMode,
      layoutResults,
      m7PwaRtl,
    };
    assert.equal(cdp.browserExceptions.length, 0, `Uncaught browser exceptions during audit: ${JSON.stringify(cdp.browserExceptions)}`);
    writeFileSync(artifactPath, JSON.stringify(summary, null, 2));
    console.log(JSON.stringify(summary, null, 2));

    cdp.close();
  } finally {
    try {
      chrome.kill('SIGKILL');
    } catch {
      // already exited
    }
    await new Promise(resolve => {
      const t = setTimeout(resolve, 1500);
      chrome.once('exit', () => {
        clearTimeout(t);
        resolve();
      });
    });
    server.close();
    try {
      rmSync(userDataDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 50 });
    } catch {
      // best-effort temp cleanup
    }
  }
}

runAudit().catch(err => {
  console.error(err);
  process.exit(1);
});
