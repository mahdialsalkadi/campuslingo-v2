#!/usr/bin/env node
// End-to-End Full-Stack Browser Verification against real campuslingo-server + PostgreSQL 16.
// Uses an isolated database on the private live-dev PostgreSQL cluster, drives the real
// browser UI in headless Chromium via CDP, and verifies persistence across page reloads
// plus P-06 touch target sizes & zero horizontal overflow at 360px, 768px, and 1280px.

import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import {
  ensureLiveDatabase,
  freePort,
  startServerProcess,
} from './run_live_stack.mjs';

const chromiumBin =
  '/home/mahdi/.cache/ms-playwright/chromium_headless_shell-1254/chrome-headless-shell-linux64/chrome-headless-shell';

class CdpClient {
  constructor(wsUrl) {
    this.ws = new WebSocket(wsUrl);
    this.nextId = 1;
    this.pending = new Map();
    this.browserExceptions = [];
    this.ready = new Promise((resolve, reject) => {
      this.ws.addEventListener('open', resolve, { once: true });
      this.ws.addEventListener(
        'error',
        e => reject(new Error(`CDP WS error: ${e.message ?? 'unknown'}`)),
        { once: true },
      );
    });
    this.ws.addEventListener('message', msg => {
      const data = JSON.parse(String(msg.data));
      if (data.id && this.pending.has(data.id)) {
        const { resolve, reject } = this.pending.get(data.id);
        this.pending.delete(data.id);
        if (data.error) reject(new Error(JSON.stringify(data.error)));
        else resolve(data.result);
      } else if (data.method === 'Runtime.exceptionThrown') {
        const desc =
          data.params?.exceptionDetails?.exception?.description ??
          data.params?.exceptionDetails?.text ??
          'Unknown browser exception';
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

async function waitForCondition(evalPage, expr, description, timeoutMs = 15_000) {
  const deadline = Date.now() + timeoutMs;
  let lastVal = null;
  while (Date.now() < deadline) {
    lastVal = await evalPage(expr);
    if (lastVal) return lastVal;
    await new Promise(r => setTimeout(r, 100));
  }
  throw new Error(`Timed out waiting for: ${description} (last=${JSON.stringify(lastVal)})`);
}

async function main() {
  const e2eDbName = `campuslingo_e2e_${Date.now()}`;
  const dbInfo = await ensureLiveDatabase({ databaseName: e2eDbName });
  const srvPort = await freePort();
  const bind = `127.0.0.1:${srvPort}`;
  const { child: serverChild, baseUrl } = await startServerProcess(dbInfo, {
    bind,
    quiet: true,
  });

  const userDataDir = mkdtempSync(path.join(tmpdir(), 'campuslingo-e2e-cdp-'));
  let chrome = null;
  let cdp = null;

  try {
    // 1. HTTP static/WASM/health/traversal checks
    const healthRes = await fetch(`${baseUrl}/healthz`);
    assert.equal(healthRes.status, 200, '/healthz should return 200');
    const readyRes = await fetch(`${baseUrl}/readyz`);
    assert.equal(readyRes.status, 200, '/readyz should return 200');
    const indexRes = await fetch(`${baseUrl}/`);
    assert.equal(indexRes.status, 200, '/ should return 200');
    assert.match(indexRes.headers.get('content-type') ?? '', /text\/html/);
    const wasmRes = await fetch(`${baseUrl}/wasm/campuslingo_domain_bg.wasm`);
    assert.equal(wasmRes.status, 200, '/wasm/campuslingo_domain_bg.wasm should return 200');
    assert.match(wasmRes.headers.get('content-type') ?? '', /application\/wasm/);
    const traversalRes = await fetch(`${baseUrl}/wasm/..%2F..%2FCargo.toml`);
    assert.notEqual(traversalRes.status, 200, 'path traversal must never return 200');

    // 2. Launch headless Chromium via CDP
    chrome = spawn(
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

    const wsUrl = await new Promise((resolve, reject) => {
      const deadline = setTimeout(
        () => reject(new Error('Chromium DevTools startup timeout')),
        15_000,
      );
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

    cdp = new CdpClient(wsUrl);
    const { targetId } = await cdp.send('Target.createTarget', { url: 'about:blank' });
    const { sessionId } = await cdp.send('Target.attachToTarget', {
      targetId,
      flatten: true,
    });
    await cdp.send('Page.enable', {}, sessionId);
    await cdp.send('Runtime.enable', {}, sessionId);
    await cdp.send(
      'Emulation.setDeviceMetricsOverride',
      {
        width: 1280,
        height: 820,
        deviceScaleFactor: 1,
        mobile: false,
      },
      sessionId,
    );

    const evalPage = async expression => {
      const out = await cdp.send(
        'Runtime.evaluate',
        { expression, awaitPromise: true, returnByValue: true },
        sessionId,
      );
      if (out.exceptionDetails) {
        throw new Error(
          `Browser exception: ${JSON.stringify(out.exceptionDetails)}`,
        );
      }
      return out.result?.value;
    };

    // 3. Load app & verify WASM verification
    await cdp.send('Page.navigate', { url: `${baseUrl}/` }, sessionId);
    await waitForCondition(
      evalPage,
      `window.__CAMPUSLINGO_SHELL__?.snapshot?.wasmVerified === true`,
      'WASM verified snapshot on load',
    );
    const initialLive = await evalPage(
      `window.__CAMPUSLINGO_SHELL__.snapshot.liveConnected`,
    );
    assert.equal(initialLive, false, 'should start disconnected before login');

    // 4. Connect live session via UI button -> /v1/auth/* + /v1/courses + /v1/fortress
    await evalPage(
      `document.querySelector('[data-testid="m4-live-connect-btn"]').click()`,
    );
    const connectedSnap = await waitForCondition(
      evalPage,
      `(() => {
        const s = window.__CAMPUSLINGO_SHELL__?.snapshot;
        if (s?.liveConnected && s.liveCourseId && s.liveUserId) return s;
        const badge = document.querySelector('[data-testid="m4-live-badge"]')?.textContent;
        if (badge === 'CONNECT FAILED') {
          throw new Error('UI reported CONNECT FAILED');
        }
        return null;
      })()`,
      'live session connection',
    );
    assert.equal(
      await evalPage(
        `document.querySelector('[data-testid="m4-live-badge"]').textContent.trim()`,
      ),
      'LIVE SERVER',
    );

    // 5. Recompute live course shields
    await evalPage(
      `document.querySelector('[data-testid="m4-recompute-shields-btn"]').click()`,
    );
    await new Promise(r => setTimeout(r, 300));
    assert.equal(
      await evalPage(`window.__CAMPUSLINGO_SHELL__.snapshot.liveConnected`),
      true,
    );

    // 6. Live WebSocket 2-lane duel matchmaking, strike, and tick advance
    await evalPage(
      `document.querySelector('[data-testid="m5-live-matchmake"]').click()`,
    );
    await waitForCondition(
      evalPage,
      `document.querySelector('[data-testid="m5-live-matchmake"]')?.textContent?.trim() === 'Rematch WS'`,
      'live WS matchmake completion',
    );
    await new Promise(r => setTimeout(r, 250));
    const lane0Before = await evalPage(
      `document.querySelector('[data-testid="m5-lane-0"]')?.textContent ?? ''`,
    );
    await evalPage(
      `document.querySelector('[data-testid="m5-strike-overcharge"]').click()`,
    );
    const duelLaneAfterStrike = await waitForCondition(
      evalPage,
      `(() => {
        const laneTxt = document.querySelector('[data-testid="m5-lane-0"]')?.textContent ?? '';
        const statusTxt = document.querySelector('[data-testid="m5-status-readout"]')?.textContent ?? '';
        if (laneTxt !== ${JSON.stringify(lane0Before)} || statusTxt.includes('Overcharges: 1')) {
          return { laneTxt, statusTxt };
        }
        return null;
      })()`,
      'WS duel strike resolution',
    );
    assert.ok(duelLaneAfterStrike);
    await evalPage(
      `document.querySelector('[data-testid="m5-advance-30s"]').click()`,
    );
    const clockAfterAdvance = await waitForCondition(
      evalPage,
      `(() => {
        const c = document.querySelector('[data-testid="m5-clock-badge"]')?.textContent?.trim() ?? '';
        return c && c !== '01:30' ? c : null;
      })()`,
      'WS duel +30s tick advance',
    );

    // 7. M6 Live Raid execution & economy update
    const initialGoldText = await evalPage(
      `document.querySelector('[data-testid="m6-gold-badge"]').textContent`,
    );
    await evalPage(
      `document.querySelector('[data-testid="m6-raid-exec-0"]').click()`,
    );
    const raidStatus = await waitForCondition(
      evalPage,
      `(() => {
        const t = document.querySelector('[data-testid="m6-raid-status"]')?.textContent ?? '';
        return t.includes('COMPLETED') || t.includes('Gold') || t.includes('trophies') ? t : null;
      })()`,
      'live raid settlement status',
    );
    const goldAfterRaidText = await evalPage(
      `document.querySelector('[data-testid="m6-gold-badge"]').textContent`,
    );
    assert.notEqual(
      goldAfterRaidText,
      initialGoldText,
      'gold badge should increase after live raid settlement',
    );

    // 8. M6 Live Boss Strike & PostgreSQL persistence across reload
    await evalPage(
      `document.querySelector('[data-testid="m6-tab-bosses"]').click()`,
    );
    const bossBeforeText = await waitForCondition(
      evalPage,
      `(() => {
        const t = document.querySelector('[data-testid="m6-boss-hp"]')?.textContent ?? '';
        return t.includes('300 / 300') ? t : null;
      })()`,
      'seeded boss HP 300 / 300',
    );
    assert.match(bossBeforeText, /300 \/ 300/);
    await evalPage(
      `document.querySelector('[data-testid="m6-boss-strike-btn"]').click()`,
    );
    const bossAfterText = await waitForCondition(
      evalPage,
      `(() => {
        const t = document.querySelector('[data-testid="m6-boss-hp"]')?.textContent ?? '';
        return t && !t.includes('300 / 300') ? t : null;
      })()`,
      'boss HP drop after live strike',
    );

    // Reload and confirm cookie auto-reconnect + persisted DB state!
    await cdp.send('Page.reload', { ignoreCache: false }, sessionId);
    await waitForCondition(
      evalPage,
      `(() => {
        const s = window.__CAMPUSLINGO_SHELL__?.snapshot;
        return s?.wasmVerified && s?.liveConnected ? s : null;
      })()`,
      'auto-reconnect after page reload via HttpOnly cookie',
    );
    const goldAfterReloadText = await evalPage(
      `document.querySelector('[data-testid="m6-gold-badge"]').textContent`,
    );
    assert.equal(
      goldAfterReloadText,
      goldAfterRaidText,
      'persisted gold balance must survive page reload',
    );
    await evalPage(
      `document.querySelector('[data-testid="m6-tab-bosses"]').click()`,
    );
    const bossAfterReloadText = await waitForCondition(
      evalPage,
      `(() => {
        const t = document.querySelector('[data-testid="m6-boss-hp"]')?.textContent ?? '';
        return t && !t.includes('300 / 300') ? t : null;
      })()`,
      'persisted boss HP across reload',
    );

    // 9. P-06 >= 44x44px touch targets & zero horizontal overflow at 360px, 768px, 1280px
    for (const width of [360, 768, 1280]) {
      await cdp.send(
        'Emulation.setDeviceMetricsOverride',
        {
          width,
          height: 820,
          deviceScaleFactor: 1,
          mobile: width < 768,
        },
        sessionId,
      );
      await new Promise(r => setTimeout(r, 100));
      const layoutCheck = await evalPage(`(() => {
        const shell = document.getElementById('m4-game-shell');
        const badTargets = [];
        for (const el of shell.querySelectorAll('button, input[type="range"]')) {
          const rect = el.getBoundingClientRect();
          if (rect.width === 0 && rect.height === 0) continue;
          if (rect.width < 44 || rect.height < 44) {
            badTargets.push({ testid: el.dataset.testid || el.id || el.textContent, w: rect.width, h: rect.height });
          }
        }
        return {
          scrollWidth: document.documentElement.scrollWidth,
          innerWidth: window.innerWidth,
          badTargets,
        };
      })()`);
      assert.equal(
        layoutCheck.badTargets.length,
        0,
        `P-06 violation at ${width}px: ${JSON.stringify(layoutCheck.badTargets)}`,
      );
      assert.ok(
        layoutCheck.scrollWidth <= layoutCheck.innerWidth,
        `Horizontal overflow at ${width}px: scrollWidth=${layoutCheck.scrollWidth} > innerWidth=${layoutCheck.innerWidth}`,
      );
    }

    assert.equal(
      cdp.browserExceptions.length,
      0,
      `Uncaught browser exceptions: ${JSON.stringify(cdp.browserExceptions)}`,
    );

    console.log(
      JSON.stringify({
        ok: true,
        database: e2eDbName,
        connectedCourseId: connectedSnap.liveCourseId,
        duelLaneAfterStrike,
        clockAfterAdvance,
        raidStatus,
        goldAfterReloadText,
        bossAfterText,
        bossAfterReloadText,
      }),
    );
  } finally {
    if (cdp) {
      try {
        cdp.close();
      } catch {
        // Ignore close error
      }
    }
    if (chrome) {
      const exited = new Promise(resolve => {
        if (chrome.exitCode !== null) resolve();
        else {
          chrome.once('exit', resolve);
          setTimeout(resolve, 2000);
        }
      });
      try {
        chrome.kill('SIGTERM');
      } catch {
        // Ignore kill error
      }
      await exited;
    }
    try {
      rmSync(userDataDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
    } catch {
      // Best-effort temp profile cleanup
    }
    if (serverChild && serverChild.exitCode === null) {
      serverChild.kill('SIGTERM');
      await new Promise(r => {
        serverChild.once('exit', r);
        setTimeout(r, 3000);
      });
    }
    spawnSync(
      path.join(dbInfo.pgBin, 'psql'),
      [
        '-v',
        'ON_ERROR_STOP=1',
        '-U',
        'postgres',
        '-d',
        'postgres',
        '-c',
        `DROP DATABASE IF EXISTS ${e2eDbName} WITH (FORCE)`,
      ],
      {
        stdio: 'pipe',
        env: {
          ...process.env,
          PGHOST: dbInfo.pgHost,
          PGPORT: String(dbInfo.pgPort),
        },
      },
    );
  }
}

await main();
