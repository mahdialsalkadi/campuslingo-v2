import initWasm, { evaluateJson } from '/wasm/campuslingo_domain.js';
import {
  computeParameterTargetBand,
  evaluateStateViaWasm,
  findActiveClause,
  findActiveDirectorTrack,
  probeOpusSupport,
  quantizeParameterStep,
  verifySandboxViaWasm,
} from './controller.js';

const params = new URLSearchParams(window.location.search);
if (params.get('compact-test') === '1') {
  document.body.classList.add('compact-test');
}

const DEFAULT_BUNDLE = {
  schema_version: '1.0.0',
  node_id: 'm3-ui-node-1',
  language: 'en',
  world_theme: 'CYBER_GRID',
  title: 'Buck Converter Duty-Cycle Stabilization',
  tactical_summary: [
    'Adjust power until output reaches [12, 14].',
    'Match discrete source region limits without overflow.',
  ],
  source_regions: [
    {
      source_sha256: 'a'.repeat(64),
      page: 1,
      bbox: [36, 48, 220, 140],
      page_width: 612,
      page_height: 792,
      text_quote: 'A linear gain stage scales output = 2 * power within [12, 14]V.',
    },
    {
      source_sha256: 'b'.repeat(64),
      page: 2,
      bbox: [260, 110, 460, 210],
      page_width: 612,
      page_height: 792,
      text_quote: 'الملاحظة التكميلية: ضبط الجهد ضمن النطاق الآمن للوحدة.',
    },
  ],
  challenges: [],
  sandbox: {
    schema_version: '1.0.0',
    archetype: 'TRADEOFF',
    modifier: 'BUDGET_CAP',
    parameters: [
      {
        name: 'power',
        label: 'Gain Power',
        minimum: 0,
        maximum: 10,
        step: 1,
        initial: 1,
      },
    ],
    targets: [
      {
        name: 'output',
        expression: {
          kind: 'operation',
          op: 'multiply',
          args: [
            { kind: 'variable', name: 'power' },
            { kind: 'constant', value: 2 },
          ],
        },
        lower: 12,
        upper: 14,
      },
    ],
    time_limit_ms: 11000,
  },
  director_tracks: [
    { start_ms: 0, end_ms: 1200, focus_role: 'concept', shot_type: 'overview', explode_factor: 0.0 },
    { start_ms: 1260, end_ms: 2800, focus_role: 'output', shot_type: 'close_up', explode_factor: 0.3 },
  ],
  audio: {
    asset_id: 'm3-briefing-1',
    language: 'en',
    codec: 'opus',
    text: 'Step one: increase parameter power from 1 to 6. Output reaches 12 volts inside [12, 14].',
    duration_ms: 2800,
    alignment_method: 'CLAUSE_CONCAT',
    words: [
      { text: 'Step one: increase parameter power from 1 to 6.', start_ms: 0, end_ms: 1200 },
      { text: 'Output reaches 12 volts inside [12, 14]. / النطاق الآمن 12 إلى 14 فولت.', start_ms: 1260, end_ms: 2800 },
    ],
  },
  generator_version: 'qwen3.5-4b-gbnf-m3-v1',
};

async function loadBundle() {
  const bundleUrl = params.get('bundle');
  if (!bundleUrl) return DEFAULT_BUNDLE;
  const resp = await fetch(bundleUrl);
  if (!resp.ok) throw new Error(`bundle fetch HTTP ${resp.status}`);
  return resp.json();
}

async function boot() {
  await initWasm();
  const bundle = await loadBundle();
  document.getElementById('bundle-title').textContent = bundle.title;

  const sandbox = bundle.sandbox;
  const verification = verifySandboxViaWasm(evaluateJson, sandbox);
  const wasmStatus = document.getElementById('wasm-status');
  wasmStatus.textContent = `WASM verified: ${verification.states_checked} states (witness: ${JSON.stringify(verification.witness)})`;

  const values = {};
  for (const p of sandbox.parameters) {
    values[p.name] = p.initial;
  }

  const controlsHost = document.getElementById('sandbox-controls');
  const readoutEl = document.getElementById('target-readout');
  const badgeEl = document.getElementById('target-badge');

  function refreshEval() {
    const state = evaluateStateViaWasm(evaluateJson, sandbox, values);
    badgeEl.dataset.solved = String(state.solved);
    badgeEl.textContent = state.solved ? 'IN TARGET ZONE' : 'OUTSIDE TARGET';
    readoutEl.textContent = state.targets
      .map((t) => `${t.name} = ${t.value} (target [${t.lower}, ${t.upper}])`)
      .join(' | ');
  }

  for (const param of sandbox.parameters) {
    const row = document.createElement('div');
    row.className = 'param-row';
    const label = document.createElement('label');
    label.textContent = `${param.label}: ${values[param.name]}`;
    const controls = document.createElement('div');
    controls.className = 'param-controls';

    const decBtn = document.createElement('button');
    decBtn.type = 'button';
    decBtn.className = 'touch-btn';
    decBtn.textContent = '-';
    decBtn.setAttribute('aria-label', `Decrease ${param.label}`);

    const sliderWrap = document.createElement('div');
    sliderWrap.className = 'slider-wrap';

    const bandBar = document.createElement('div');
    bandBar.className = 'target-band-bar';
    const band = computeParameterTargetBand(evaluateJson, sandbox, param, values);
    const inTargetPts = band.filter((p) => p.inTarget);
    if (inTargetPts.length > 0) {
      const zone = document.createElement('div');
      zone.className = 'target-zone-highlight';
      const minPct = inTargetPts[0].pct;
      const maxPct = inTargetPts[inTargetPts.length - 1].pct;
      zone.style.left = `${Math.max(0, minPct - 2)}%`;
      zone.style.width = `${Math.max(6, maxPct - minPct + 4)}%`;
      zone.setAttribute('data-testid', 'target-zone');
      bandBar.appendChild(zone);
    }

    const slider = document.createElement('input');
    slider.type = 'range';
    slider.className = 'discrete-slider';
    slider.min = String(param.minimum);
    slider.max = String(param.maximum);
    slider.step = String(param.step);
    slider.value = String(values[param.name]);
    slider.setAttribute('aria-label', param.label);
    slider.setAttribute('data-testid', `slider-${param.name}`);

    const incBtn = document.createElement('button');
    incBtn.type = 'button';
    incBtn.className = 'touch-btn';
    incBtn.textContent = '+';
    incBtn.setAttribute('aria-label', `Increase ${param.label}`);
    incBtn.setAttribute('data-testid', `inc-${param.name}`);
    decBtn.setAttribute('data-testid', `dec-${param.name}`);

    function setParam(nextVal) {
      const snapped = quantizeParameterStep(param, nextVal);
      values[param.name] = snapped;
      slider.value = String(snapped);
      label.textContent = `${param.label}: ${snapped}`;
      refreshEval();
    }

    slider.addEventListener('input', () => setParam(Number(slider.value)));
    decBtn.addEventListener('click', () => setParam(values[param.name] - param.step));
    incBtn.addEventListener('click', () => setParam(values[param.name] + param.step));

    sliderWrap.appendChild(bandBar);
    sliderWrap.appendChild(slider);
    controls.appendChild(decBtn);
    controls.appendChild(sliderWrap);
    controls.appendChild(incBtn);
    row.appendChild(label);
    row.appendChild(controls);
    controlsHost.appendChild(row);
  }
  refreshEval();

  // Interactive SourceRegion hotspots
  const diagramStage = document.getElementById('diagram-stage');
  const quoteEl = document.getElementById('source-quote');
  const hotspotBtns = [];
  bundle.source_regions.forEach((region, idx) => {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'hotspot-btn';
    btn.textContent = `Region ${idx + 1}`;
    btn.setAttribute('data-testid', `hotspot-${idx}`);
    btn.setAttribute('aria-pressed', String(idx === 0));
    const leftPct = Math.min(68, Math.max(4, (region.bbox[0] / region.page_width) * 100));
    const topPct = Math.min(64, Math.max(6, (region.bbox[1] / region.page_height) * 100));
    btn.style.left = `${leftPct.toFixed(1)}%`;
    btn.style.top = `${topPct.toFixed(1)}%`;
    btn.addEventListener('click', () => {
      hotspotBtns.forEach((b, j) => b.setAttribute('aria-pressed', String(j === idx)));
      quoteEl.textContent = `p.${region.page}: ${region.text_quote}`;
    });
    hotspotBtns.push(btn);
    diagramStage.appendChild(btn);
  });
  if (bundle.source_regions[0]) {
    quoteEl.textContent = `p.${bundle.source_regions[0].page}: ${bundle.source_regions[0].text_quote}`;
  }

  // Synchronized AudioManifest + DirectorTrack + fallback clock
  const audioProbeEl = document.createElement('audio');
  const opusCap = probeOpusSupport((mime) => audioProbeEl.canPlayType(mime));
  const codecStatusEl = document.getElementById('codec-status');
  const audioUrl = params.get('audio');
  codecStatusEl.textContent = opusCap.supported
    ? (audioUrl ? '.opus ready (native)' : '.opus supported (clock fallback)')
    : `.opus fallback (${opusCap.fallbackReason})`;

  if (audioUrl && opusCap.supported) {
    audioProbeEl.src = audioUrl;
  }

  let currentMs = 0;
  let playing = false;
  let timer = null;
  const subtitleEl = document.getElementById('subtitle-banner');
  const directorBadgeEl = document.getElementById('director-badge');
  const chipsEl = document.getElementById('clause-chips');

  function syncCaptions(ms) {
    currentMs = ms;
    const active = findActiveClause(bundle.audio, ms);
    subtitleEl.textContent = active.word ? active.word.text : bundle.audio.text;
    const cue = findActiveDirectorTrack(bundle.director_tracks, ms);
    if (cue) {
      directorBadgeEl.textContent = `Cue: ${cue.shot_type} (${cue.focus_role})`;
    }
  }

  (bundle.audio?.words ?? []).forEach((w, idx) => {
    const chip = document.createElement('button');
    chip.type = 'button';
    chip.className = 'touch-btn';
    chip.textContent = `Clause ${idx + 1}`;
    chip.setAttribute('data-testid', `clause-btn-${idx}`);
    chip.addEventListener('click', () => {
      syncCaptions(w.start_ms + 10);
      if (audioUrl && opusCap.supported) {
        audioProbeEl.currentTime = w.start_ms / 1000;
      }
    });
    chipsEl.appendChild(chip);
  });

  document.getElementById('audio-next-clause').addEventListener('click', () => {
    const active = findActiveClause(bundle.audio, currentMs);
    const words = bundle.audio?.words ?? [];
    const nextIdx = words.length > 0 ? (active.index + 1) % words.length : 0;
    if (words[nextIdx]) syncCaptions(words[nextIdx].start_ms + 10);
  });

  const toggleBtn = document.getElementById('audio-toggle');
  toggleBtn.addEventListener('click', () => {
    playing = !playing;
    toggleBtn.textContent = playing ? 'Pause Briefing' : 'Play Briefing';
    if (timer) clearInterval(timer);
    if (playing) {
      if (audioUrl && opusCap.supported) {
        audioProbeEl.play().catch(() => {});
      }
      timer = setInterval(() => {
        const next = currentMs + 100;
        syncCaptions(next > (bundle.audio?.duration_ms ?? 2800) ? 0 : next);
      }, 100);
    } else if (audioUrl && opusCap.supported) {
      audioProbeEl.pause();
    }
  });

  syncCaptions(0);
  window.__campuslingoReady = true;
}

boot().catch((err) => {
  const el = document.getElementById('wasm-status');
  if (el) el.textContent = `ERROR: ${err.message}`;
  window.__campuslingoError = String(err);
});
