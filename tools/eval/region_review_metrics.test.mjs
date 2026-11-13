import assert from 'node:assert/strict';
import test from 'node:test';
import { iou, reviewChecks, scoreSample, validateRegions } from './region_review_metrics.mjs';

const region = (id, x0 = 10, x1 = 30, kind = 'image') =>
  ({ id, kind, bbox: { x0, y0: 10, x1, y1: 30 } });
const sample = (predicted = [], reference = []) => ({
  page_size_pt: [100, 100], predicted_regions: predicted, reference_regions: reference,
  visual_review_status: 'REVIEWED', reference_regions_complete: true,
  ...Object.fromEntries(reviewChecks.map((key) => [key, null])),
});

test('unreviewed or incomplete references never score as empty truth', () => {
  const value = sample([region('p')]);
  value.visual_review_status = 'NOT_REVIEWED';
  assert.equal(scoreSample(value).metrics, null);
  value.visual_review_status = 'REVIEWED';
  value.reference_regions_complete = false;
  assert.equal(scoreSample(value).metrics, null);
  value.reference_regions_complete = true;
  const metrics = scoreSample(value).metrics;
  assert.equal(metrics.references, 0);
  assert.equal(metrics.mean_iou_per_reference, null);
  assert.equal(metrics.mean_iou_per_prediction, 0);
});

test('exact, partial and disjoint IoU', () => {
  assert.equal(iou(region('a').bbox, region('b').bbox), 1);
  assert.equal(iou(region('a').bbox, region('b', 20, 40).bbox), 1 / 3);
  assert.equal(iou(region('a').bbox, region('b', 40, 60).bbox), 0);
});

test('duplicate predictions cannot claim a reference twice', () => {
  const metrics = scoreSample(sample([region('p1'), region('p2')], [region('r')])).metrics;
  assert.equal(metrics.positive_overlap_pairs, 1);
  assert.equal(metrics.unmatched_prediction_ids.length, 1);
  assert.equal(metrics.mean_iou_per_reference, 1);
  assert.equal(metrics.mean_iou_per_prediction, 0.5);
});

test('missing predictions and wrong kinds remain unmatched', () => {
  const missing = scoreSample(sample([], [region('r')])).metrics;
  assert.equal(missing.mean_iou_per_reference, 0);
  assert.equal(missing.mean_iou_per_prediction, null);
  const wrong = scoreSample(sample([region('p', 10, 30, 'math')], [region('r')])).metrics;
  assert.equal(wrong.positive_overlap_pairs, 0);
  assert.deepEqual(wrong.unmatched_reference_ids, ['r']);
});

test('invalid, duplicate, out-of-page and non-finite annotation boxes fail closed', () => {
  assert.throws(() => validateRegions([region('r'), region('r')], [100, 100]));
  for (const box of [region('r', -1), region('r', 30, 10), region('r', 10, 101),
    region('r', NaN), region('r', 10, Infinity), region('r', 10, 30, 'unknown')]) {
    assert.throws(() => validateRegions([box], [100, 100]));
  }
  assert.throws(() => validateRegions([], [Number.MAX_VALUE, Number.MAX_VALUE]));
  assert.throws(() => scoreSample({ ...sample(), diagram_labels_complete: 'yes' }));
  assert.throws(() => scoreSample({ ...sample(), reference_regions_complete: undefined }));
});

test('maximum-IoU assignment agrees with exhaustive small-set oracle', () => {
  function oracle(predictions, references, row = 0, used = new Set()) {
    if (row === references.length) return 0;
    let best = oracle(predictions, references, row + 1, used);
    for (let p = 0; p < predictions.length; p++) {
      if (used.has(p) || predictions[p].kind !== references[row].kind) continue;
      used.add(p);
      best = Math.max(best, iou(predictions[p].bbox, references[row].bbox) +
        oracle(predictions, references, row + 1, used));
      used.delete(p);
    }
    return best;
  }
  for (let seed = 0; seed < 60; seed++) {
    const predicted = Array.from({ length: seed % 4 + 1 }, (_, i) =>
      region(`p${i}`, (seed * 13 + i * 11) % 40, 60 + (seed + i) % 30, i % 3 ? 'image' : 'math'));
    const reference = Array.from({ length: (seed * 3) % 4 + 1 }, (_, i) =>
      region(`r${i}`, (seed * 7 + i * 17) % 40, 55 + (seed + i) % 30, i % 2 ? 'image' : 'math'));
    const actual = scoreSample(sample(predicted, reference)).metrics.iou_sum;
    assert.ok(Math.abs(actual - oracle(predicted, reference)) < 1e-12, `seed=${seed}`);
  }
});
