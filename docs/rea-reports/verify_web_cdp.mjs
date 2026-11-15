import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';

const chromiumBin =
  '/home/mahdi/.cache/ms-playwright/chromium_headless_shell-1254/chrome-headless-shell-linux64/chrome-headless-shell';

class CdpClient {
  constructor(wsUrl) {
    this.ws = new WebSocket(wsUrl);
    this.nextId = 1;
    this.pending = new Map();
    this.events = [];
    this.ready = new Promise((resolve, reject) => {
      this.ws.addEventListener('open', resolve, { once: true });
      this.ws.addEventListener('error', e => reject(new Error(`WS err: ${e.message}`)), { once: true });
    });
    this.ws.addEventListener('message', msg => {
      const data = JSON.parse(String(msg.data));
      if (data.id && this.pending.has(data.id)) {
        const { resolve, reject } = this.pending.get(data.id);
        this.pending.delete(data.id);
        if (data.error) reject(new Error(JSON.stringify(data.error)));
        else resolve(data.result);
      } else if (data.method) {
        this.events.push({ method: data.method, params: data.params });
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

async function runCase(url, shotPath) {
  const userDataDir = mkdtempSync(path.join(tmpdir(), 'cl-web-cdp-'));
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
  const wsUrl = await new Promise((resolve, reject) => {
    const t = setTimeout(() => reject(new Error('CDP timeout')), 15000);
    let buf = '';
    chrome.stderr.on('data', d => {
      buf += d.toString('utf8');
      const m = buf.match(/DevTools listening on (ws:\/\/[^\s]+)/);
      if (m) { clearTimeout(t); resolve(m[1]); }
    });
    chrome.once('exit', c => { clearTimeout(t); reject(new Error(`exit ${c}: ${buf}`)); });
  });
  const cdp = new CdpClient(wsUrl);
  try {
    const { targetId } = await cdp.send('Target.createTarget', { url: 'about:blank' });
    const { sessionId } = await cdp.send('Target.attachToTarget', { targetId, flatten: true });
    await cdp.send('Page.enable', {}, sessionId);
    await cdp.send('Runtime.enable', {}, sessionId);
    await cdp.send('Network.enable', {}, sessionId);
    await cdp.send('Emulation.setDeviceMetricsOverride', { width: 1360, height: 840, deviceScaleFactor: 1, mobile: false }, sessionId);
    await cdp.send('Page.navigate', { url }, sessionId);

    // Wait up to 40s for canvas resize and full live sync (/v1/courses/.../economy)
    const deadline = Date.now() + 40000;
    while (Date.now() < deadline) {
      await new Promise(r => setTimeout(r, 1000));
      const res = await cdp.send('Runtime.evaluate', {
        expression: `JSON.stringify({ statusGone: !document.getElementById('status'), notice: document.getElementById('status-notice')?.textContent || '', w: document.getElementById('canvas')?.width || 0, h: document.getElementById('canvas')?.height || 0 })`,
        returnByValue: true,
      }, sessionId);
      const st = JSON.parse(res.result.value);
      if (st.notice) { console.log('NOTICE:', st.notice); break; }
      const econDone = cdp.events.some(
        e => e.method === 'Network.responseReceived' && e.params.response.url.includes('/content') && e.params.response.status === 200
      );
      if (st.statusGone && st.w >= 1000 && econDone) {
        await new Promise(r => setTimeout(r, 3500));
        break;
      }
    }

    const consoleLogs = cdp.events
      .filter(e => e.method === 'Runtime.consoleAPICalled')
      .map(e => `[${e.params.type}] ` + e.params.args.map(a => a.value ?? a.description ?? '').join(' '));
    const exceptions = cdp.events
      .filter(e => e.method === 'Runtime.exceptionThrown')
      .map(e => e.params.exceptionDetails?.text || 'exception');
    const netFails = cdp.events
      .filter(e => e.method === 'Network.responseReceived' && e.params.response.status >= 400)
      .map(e => `${e.params.response.status} ${e.params.response.url}`);
    const netOk = cdp.events
      .filter(e => e.method === 'Network.responseReceived' && e.params.response.url.includes('/v1/'))
      .map(e => `${e.params.response.status} ${e.params.response.url}`);

    const shot = await cdp.send('Page.captureScreenshot', { format: 'png' }, sessionId);
    mkdirSync(path.dirname(shotPath), { recursive: true });
    writeFileSync(shotPath, Buffer.from(shot.data, 'base64'));

    console.log(`\n=== URL: ${url} ===`);
    console.log('V1_NETWORK:', netOk);
    console.log('NET_FAILS:', netFails);
    console.log('EXCEPTIONS:', exceptions);
    console.log('CONSOLE:\n' + consoleLogs.join('\n'));
    console.log('SCREENSHOT:', shotPath);
  } finally {
    cdp.close();
    chrome.kill('SIGKILL');
    rmSync(userDataDir, { recursive: true, force: true });
  }
}

await runCase('http://127.0.0.1:18080/?study=1', '/home/mahdi/campuslingo-v2/apps/godot/Docs/progress/m5/m5_browser_study.png');
