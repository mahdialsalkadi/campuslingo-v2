// Synthetic private fixture only. Verifies that the review packet generator
// emits a self-contained local annotator with artifact-bound visuals. It never
// reads the real corpus or creates annotations.
import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync, existsSync, statSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));
const prepare = fileURLToPath(new URL('./prepare_region_review.mjs', import.meta.url));
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
const save = (file, value) => writeFileSync(file, JSON.stringify(value), { mode: 0o600 });

test('generated packet embeds a local annotator with existing private links', () => {
  const directory = mkdtempSync(path.join(root, 'artifacts/private/review-test-'));
  try {
    for (const name of ['pages', 'crops']) mkdirSync(path.join(directory, name), { mode: 0o700 });
    const png = Buffer.from('89504e470d0a1a0a0000000d49484452', 'hex');
    const webp = Buffer.from('524946461600000057454250565038200a0000000000009d012a47004700', 'hex');
    writeFileSync(path.join(directory, 'pages', 'page-0001.png'), png, { mode: 0o600 });
    writeFileSync(path.join(directory, 'pages', 'overlay.png'), png, { mode: 0o600 });
    writeFileSync(path.join(directory, 'crops', 'crop.webp'), webp, { mode: 0o600 });
    const artifact = {
      page_number: 1,
      run_id: '00000000-0000-4000-8000-000000000001',
      width_pt: 100,
      height_pt: 100,
      pipeline_version: 'm2-review-test',
      overlay_path: 'pages/overlay.png',
      vector_evidence: { incomplete: [] },
      actions: { actions: [], incomplete: [] },
      crops: [{
        id: 'crop', kind: 'image', bbox: { x0: 5, y0: 5, x1: 30, y1: 40 },
        relative_path: 'crops/crop.webp',
      }],
    };
    save(path.join(directory, 'pages', 'page-0001.json'), artifact);
    save(path.join(directory, 'index.json'), {
      annotated_corpus_acceptance: false,
      reports: [{
        category: 'synthetic', run_directory: path.relative(root, directory),
        pipeline_version: 'm2-review-test', status: 'partial',
      }],
    });
    const result = spawnSync(process.execPath, [prepare, path.join(directory, 'index.json')],
      { cwd: root, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    const summary = JSON.parse(result.stdout);
    assert.equal(summary.samples, 1);
    const packetDirectory = path.join(root, summary.directory);
    const annotatorPath = path.join(packetDirectory, 'review_annotator.html');
    assert.ok(existsSync(annotatorPath));
    assert.equal(statSync(annotatorPath).mode & 0o777, 0o600);
    const html = readFileSync(annotatorPath, 'utf8');
    assert.ok(!html.includes('__PACKET__') && !html.includes('__VISUALS__'));
    const packet = JSON.parse(html.match(/<script id="packet-data" type="application\/json">([\s\S]*?)<\/script>/)[1]);
    const visuals = JSON.parse(html.match(/<script id="visual-data" type="application\/json">([\s\S]*?)<\/script>/)[1]);
    assert.equal(packet.schema_version, 2);
    assert.equal(packet.status, 'NOT_REVIEWED');
    assert.equal(packet.annotated_corpus_acceptance, false);
    assert.equal(packet.samples.length, 1);
    assert.equal(visuals.length, 1);
    assert.equal(packet.samples[0].artifact_sha256,
      hash(readFileSync(path.join(directory, 'pages', 'page-0001.json'))));
    for (const link of [visuals[0].source, visuals[0].overlay, ...visuals[0].crops.map((crop) => crop.url)]) {
      assert.ok(existsSync(path.join(packetDirectory, decodeURIComponent(link))), link);
    }
    assert.ok(!/https?:|fetch\(|XMLHttpRequest|WebSocket/.test(
      html.replace(/['"]https?:\/\/www\.w3\.org\/2000\/svg['"]/g, '')), 'annotator must not reference network endpoints or network APIs');
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
