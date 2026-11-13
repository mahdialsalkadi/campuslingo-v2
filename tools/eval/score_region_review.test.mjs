// Synthetic metadata only; fixture files never contain lecture derivatives.
import assert from 'node:assert/strict';
import test from 'node:test';
import { createHash } from 'node:crypto';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { reviewChecks } from './region_review_metrics.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));
const scorer = fileURLToPath(new URL('./score_region_review.mjs', import.meta.url));
const hash = (value) => createHash('sha256').update(value).digest('hex');
const save = (file, value) => writeFileSync(file, JSON.stringify(value), { mode: 0o600 });

function fixture(callback) {
  const directory = mkdtempSync(path.join(root, 'artifacts/private/review-score-test-'));
  try {
    mkdirSync(path.join(directory, 'pages'), { mode: 0o700 });
    const version = 'm2-pipeline-v4-vector-consolidation';
    const crop = { id: 'predicted', kind: 'image', bbox: { x0: 10, y0: 10, x1: 30, y1: 30 } };
    const artifact = { run_id: 'synthetic', page_number: 1, pipeline_version: version,
      width_pt: 100, height_pt: 100, crops: [crop], vector_evidence: { incomplete: [] } };
    const artifactPath = path.join(directory, 'pages/page-0001.json');
    save(artifactPath, artifact);
    const indexPath = path.join(directory, 'index.json');
    save(indexPath, { reports: [{ category: 'synthetic', run_directory: directory, pipeline_version: version }] });
    const packet = { schema_version: 2, index: indexPath, pipeline_versions: [version],
      annotated_corpus_acceptance: false, independent_review: false, reviewer: null, reviewed_at: null,
      samples: [{ category: 'synthetic', run_id: 'synthetic', page: 1, page_size_pt: [100, 100],
        artifact_sha256: hash(readFileSync(artifactPath)), predicted_regions: [crop], vector_incomplete: [],
        reference_regions: [], reference_regions_complete: false, visual_review_status: 'NOT_REVIEWED',
        ...Object.fromEntries(reviewChecks.map((key) => [key, null])) }] };
    const input = path.join(directory, 'annotations.json');
    const run = () => {
      save(input, packet);
      return spawnSync(process.execPath, [scorer, input], { encoding: 'utf8' });
    };
    const reviewed = () => {
      packet.reviewer = 'synthetic-test';
      packet.reviewed_at = '2026-11-12T00:00:00Z';
      packet.samples[0].visual_review_status = 'REVIEWED';
      packet.samples[0].reference_regions_complete = true;
      packet.samples[0].reference_regions = [{ ...crop, id: 'reference' }];
    };
    callback({ packet, artifact, artifactPath, run, reviewed });
  } finally {
    rmSync(directory, { recursive: true });
  }
}

test('unreviewed private packet records BLOCKED and no score', () => fixture(({ run }) => {
  const result = run();
  assert.equal(result.status, 2, result.stderr);
  const output = JSON.parse(result.stdout);
  assert.equal(output.status, 'BLOCKED_NO_REVIEWED_REFERENCES');
  assert.equal(output.mean_iou_per_reference, null);
  assert.equal(output.samples_scored, 0);
}));

test('reviewed scores stay diagnostic; self-declared independence is never verified', () => fixture(({
  packet, run, reviewed,
}) => {
  reviewed();
  packet.independent_review = true;
  const result = run();
  assert.equal(result.status, 0, result.stderr);
  const summary = JSON.parse(result.stdout);
  const output = JSON.parse(readFileSync(path.join(root, summary.report), 'utf8'));
  assert.equal(output.mean_iou_per_reference, 1);
  assert.equal(output.independent_review_declared, true);
  assert.equal(output.independent_review_verified, false);
  assert.equal(output.annotated_corpus_acceptance, false);
  assert.equal(output.g1_status, 'OPEN');
}));

test('reviewed samples require reviewer provenance', () => fixture(({ packet, run, reviewed }) => {
  reviewed();
  packet.reviewer = null;
  assert.equal(run().status, 1);
}));

test('stale artifact bytes, edited predictions and hidden gaps are rejected', () => fixture(({
  packet, artifact, artifactPath, run, reviewed,
}) => {
  reviewed();
  const original = packet.samples[0].predicted_regions;
  packet.samples[0].predicted_regions = [];
  assert.equal(run().status, 1);
  packet.samples[0].predicted_regions = original;
  packet.samples[0].vector_incomplete = ['different'];
  assert.equal(run().status, 1);
  packet.samples[0].vector_incomplete = [];
  save(artifactPath, { ...artifact, changed: true });
  assert.equal(run().status, 1);
}));

test('duplicate samples and non-private input paths are rejected', () => fixture(({ packet, run }) => {
  packet.samples.push(packet.samples[0]);
  assert.equal(run().status, 1);
  packet.samples.pop();
  packet.index = '/etc/passwd';
  assert.equal(run().status, 1);
}));
