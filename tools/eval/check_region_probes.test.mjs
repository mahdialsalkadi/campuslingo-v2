// Synthetic header/metadata fixtures only; never reads the private corpus.
import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const checker = fileURLToPath(new URL('./check_region_probes.mjs', import.meta.url));
const version = 'm2-pipeline-v4-vector-consolidation';
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
const save = (file, value) => writeFileSync(file, JSON.stringify(value), { mode: 0o600 });

function fixture(callback) {
  const directory = mkdtempSync('/tmp/opencode/region-check-test-');
  try {
    for (const name of ['pages', 'crops']) mkdirSync(path.join(directory, name), { mode: 0o700 });
    const png = Buffer.alloc(24);
    Buffer.from('89504e470d0a1a0a', 'hex').copy(png);
    png.writeUInt32BE(150, 16);
    png.writeUInt32BE(150, 20);
    for (const name of ['overlay.png', 'mask.png']) {
      writeFileSync(path.join(directory, 'pages', name), png, { mode: 0o600 });
    }
    const webp = Buffer.from('524946461600000057454250565038200a0000000000009d012a47004700', 'hex');
    const file = path.join(directory, 'crops', 'fixture.webp');
    writeFileSync(file, webp, { mode: 0o600 });
    const crop = { id: 'fixture', kind: 'image', dpi: 250, width: 71, height: 71,
      bbox: { x0: 10, y0: 10, x1: 30, y1: 30 }, pixel_bbox: [34, 34, 71, 71],
      relative_path: 'crops/fixture.webp', bytes: webp.length, sha256: hash(webp) };
    const artifact = { pipeline_version: version, page_number: 1, width_pt: 72, height_pt: 72,
      render_dpi: 150, overlay_path: 'pages/overlay.png', vector_mask_path: 'pages/mask.png',
      vector_evidence: { primitive_count: 0, components: [], excluded_backgrounds: 0,
        excluded_text_glyphs: 0, excluded_templates: 0, template_status: 'complete',
        mask_width: 150, mask_height: 150, incomplete: [] },
      actions: { actions: [{ kind: 'ACTION_CROP_RASTER_FIGURES', bbox: crop.bbox }], incomplete: [] },
      crops: [crop], failures: [] };
    const manifest = { pipeline_version: version, source_pages: 1, failures: [] };
    const report = { status: 'partial', source_pages: 1, run_directory: directory,
      failure_counts: {}, bad_crops: 0, text_crops: 0, private_modes: true };
    const index = { annotated_corpus_acceptance: false, leased_model_inference_performed: false,
      reports: [report] };
    const run = () => {
      save(path.join(directory, 'pages', 'page-0001.json'), artifact);
      save(path.join(directory, 'manifest.json'), manifest);
      const input = path.join(directory, 'index.json');
      save(input, index);
      return spawnSync(process.execPath, [checker, input], { encoding: 'utf8' });
    };
    callback({ directory, artifact, manifest, report, index, file, crop, run });
  } finally {
    rmSync(directory, { recursive: true });
  }
}

test('full-deck verifier accepts synthetic [x,y,width,height] geometry and binary WebP', () => fixture(({ run }) => {
  const result = run();
  assert.equal(result.status, 0, result.stderr);
  assert.equal(JSON.parse(result.stdout).crops, 1);
}));

test('wrong hash and independently wrong encoded dimensions fail', () => fixture(({ run, crop, file }) => {
  crop.sha256 = '0'.repeat(64);
  assert.notEqual(run().status, 0);
  const bytes = readFileSync(file);
  bytes.writeUInt16LE(72, 26);
  writeFileSync(file, bytes);
  crop.sha256 = hash(bytes);
  assert.notEqual(run().status, 0);
}));

test('missing v4 vector evidence or inference declaration fails closed', () => fixture(({ run, artifact, index }) => {
  const evidence = artifact.vector_evidence;
  delete artifact.vector_evidence;
  assert.notEqual(run().status, 0);
  artifact.vector_evidence = evidence;
  delete index.leased_model_inference_performed;
  assert.notEqual(run().status, 0);
}));

test('explicit unsupported coverage remains reported, never accepted as full evidence', () => fixture(({
  run, artifact, manifest, report,
}) => {
  const code = 'unsupported_vector_group';
  artifact.vector_evidence.incomplete = [code];
  artifact.actions.incomplete = [code];
  artifact.failures = [{ code }];
  manifest.failures = [{ code }];
  report.failure_counts = { [code]: 1 };
  const result = run();
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(JSON.parse(result.stdout).explicit_partial_codes, { [code]: 1 });
  manifest.failures.push({ code: 'crop_failed' });
  report.failure_counts.crop_failed = 1;
  assert.notEqual(run().status, 0);
}));

test('managed OCR probe requires leased inference and clean model releases', () => fixture(({
  run, report, index,
}) => {
  index.ocr_model_configured = true;
  index.leased_model_inference_performed = true;
  report.ocr_model_configured = true;
  report.model_release_failed = false;
  report.model_release_oom_kills = 0;
  assert.equal(run().status, 0);
  report.model_release_failed = true;
  assert.notEqual(run().status, 0);
}));
