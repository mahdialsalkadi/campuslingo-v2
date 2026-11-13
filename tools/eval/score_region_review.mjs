// Local-only, artifact-bound annotation diagnostics. This never closes G-1.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, realpathSync, statSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { reviewChecks, scoreSample } from './region_review_metrics.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));
const privateRoot = realpathSync(path.join(root, 'artifacts/private'));
const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');
const contained = (file) => {
  const resolved = realpathSync(file);
  assert.ok(resolved.startsWith(`${privateRoot}${path.sep}`), 'review inputs must stay private');
  return resolved;
};
function load(file) {
  const resolved = contained(file);
  const info = statSync(resolved);
  assert.ok(info.isFile() && info.size <= 8 * 1024 * 1024, 'review input exceeds diagnostic file budget');
  assert.equal(info.mode & 0o777, 0o600, 'review input must retain private file mode');
  const bytes = readFileSync(resolved);
  return { value: JSON.parse(bytes.toString('utf8')), sha256: sha256(bytes) };
}

assert.ok(process.argv[2], 'usage: node tools/eval/score_region_review.mjs PRIVATE_ANNOTATIONS_JSON');
const input = contained(process.argv[2]);
const loaded = load(input), packet = loaded.value;
assert.equal(packet.schema_version, 2, 'prepare an artifact-bound schema-v2 review packet');
assert.equal(packet.annotated_corpus_acceptance, false, 'diagnostic packets cannot assert acceptance');
assert.equal(typeof packet.independent_review, 'boolean', 'review provenance must be explicit');
assert.ok(Array.isArray(packet.samples) && packet.samples.length > 0 && packet.samples.length <= 200,
  'invalid sample set');
if (packet.samples.some((sample) => sample.visual_review_status === 'REVIEWED')) {
  assert.ok(typeof packet.reviewer === 'string' && packet.reviewer.trim().length > 0, 'reviewer is required');
  assert.ok(typeof packet.reviewed_at === 'string' && Number.isFinite(Date.parse(packet.reviewed_at)),
    'review timestamp is required');
}
const indexInput = load(path.resolve(root, packet.index));
const index = indexInput.value;
const categories = new Map(index.reports.map((report) => [report.category, report]));
assert.equal(categories.size, index.reports.length, 'duplicate index categories');
assert.deepEqual([...new Set(index.reports.map((report) => report.pipeline_version))].sort(),
  [...packet.pipeline_versions].sort(), 'review pipeline versions changed');
const seen = new Set(), results = [];
for (const sample of packet.samples) {
  assert.ok(Number.isInteger(sample.page) && sample.page >= 1, 'invalid sample page');
  const identity = JSON.stringify([sample.category, sample.run_id, sample.page]);
  assert.ok(!seen.has(identity), 'duplicate sample');
  seen.add(identity);
  const report = categories.get(sample.category);
  assert.ok(report, 'sample category not present in index');
  const run = contained(path.resolve(root, report.run_directory));
  const artifactInput = load(path.join(run, 'pages', `page-${String(sample.page).padStart(4, '0')}.json`));
  const artifact = artifactInput.value;
  assert.equal(artifactInput.sha256, sample.artifact_sha256, 'review artifact changed');
  assert.equal(artifact.run_id, sample.run_id, 'review run changed');
  assert.equal(artifact.pipeline_version, report.pipeline_version, 'review pipeline changed');
  assert.equal(artifact.page_number, sample.page, 'review page changed');
  assert.deepEqual(sample.page_size_pt, [artifact.width_pt, artifact.height_pt], 'review page dimensions changed');
  assert.deepEqual(sample.predicted_regions, artifact.crops.map((crop) =>
    ({ id: crop.id, kind: crop.kind, bbox: crop.bbox })), 'predictions must match stored artifacts');
  assert.deepEqual(sample.vector_incomplete, artifact.vector_evidence?.incomplete ?? [],
    'review must not hide incomplete vector evidence');
  results.push({ category: sample.category, page: sample.page, ...scoreSample(sample) });
}
const scored = results.filter((result) => result.metrics !== null);
const totalReferences = scored.reduce((sum, result) => sum + result.metrics.references, 0);
const totalPredictions = scored.reduce((sum, result) => sum + result.metrics.predictions, 0);
const totalIou = scored.reduce((sum, result) => sum + result.metrics.iou_sum, 0);
const checks = Object.fromEntries(reviewChecks.map((key) => [key, { true: 0, false: 0, unreviewed: 0 }]));
for (const sample of packet.samples) {
  for (const key of reviewChecks) {
    const reviewed = sample.visual_review_status === 'REVIEWED' && sample.reference_regions_complete;
    checks[key][reviewed && sample[key] !== null ? String(sample[key]) : 'unreviewed'] += 1;
  }
}
const output = path.join(path.dirname(input), `scores-${Date.now()}.json`);
const report = {
  schema_version: 1, observed_at: new Date().toISOString(),
  status: scored.length ? 'DIAGNOSTIC_PARTIAL' : 'BLOCKED_NO_REVIEWED_REFERENCES',
  packet_sha256: loaded.sha256, index_sha256: indexInput.sha256,
  independent_review_declared: packet.independent_review, independent_review_verified: false,
  reviewer: packet.reviewer, reviewed_at: packet.reviewed_at,
  samples_total: results.length, samples_scored: scored.length,
  samples_unscored: results.length - scored.length, references: totalReferences, predictions: totalPredictions,
  mean_iou_per_reference: totalReferences ? totalIou / totalReferences : null,
  mean_iou_per_prediction: totalPredictions ? totalIou / totalPredictions : null,
  content_checks: checks, results,
  matching: 'Maximum summed IoU, one-to-one within exact region kind. Positive overlap is diagnostic, not a passing threshold.',
  g1_status: 'OPEN', annotated_corpus_acceptance: false,
  scope: 'Selected pages only; no OCR fidelity, full-corpus recall, independent-review verification or gate acceptance.',
};
writeFileSync(output, JSON.stringify(report, null, 2), { mode: 0o600, flag: 'wx' });
console.log(JSON.stringify({ status: report.status, samples_total: report.samples_total,
  samples_scored: report.samples_scored, samples_unscored: report.samples_unscored,
  mean_iou_per_reference: report.mean_iou_per_reference, g1_status: 'OPEN',
  report: path.relative(root, output) }, null, 2));
if (!scored.length) process.exitCode = 2;
