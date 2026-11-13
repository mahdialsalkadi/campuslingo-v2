// Diagnostic geometry only. No acceptance threshold, OCR inference or G-1
// decision is implemented here. Reviewed references must be supplied locally.
import assert from 'node:assert/strict';

const kinds = new Set(['vector', 'image', 'math']);
export const reviewChecks = [
  'formula_numbers_units_negation_preserved', 'code_indentation_preserved',
  'diagram_labels_complete', 'prose_barrier_respected',
];

export function validateRegions(regions, pageSize) {
  assert.ok(Array.isArray(regions) && regions.length <= 512,
    'region list exceeds the diagnostic evaluator budget');
  assert.ok(Array.isArray(pageSize) && pageSize.length === 2 &&
    pageSize.every((value) => Number.isFinite(value) && value > 0) &&
    Number.isFinite(pageSize[0] * pageSize[1]) && pageSize[0] * pageSize[1] > 0, 'invalid page dimensions');
  const seen = new Set();
  for (const region of regions) {
    assert.ok(typeof region.id === 'string' && region.id.length > 0 && !seen.has(region.id),
      'region IDs must be nonempty and unique');
    seen.add(region.id);
    assert.ok(kinds.has(region.kind), 'unknown annotation region kind');
    const box = region.bbox;
    assert.ok(box && ['x0', 'y0', 'x1', 'y1'].every((key) => Number.isFinite(box[key])),
      'non-finite annotation geometry');
    assert.ok(box.x0 >= 0 && box.y0 >= 0 && box.x1 > box.x0 && box.y1 > box.y0 &&
      box.x1 <= pageSize[0] && box.y1 <= pageSize[1], 'annotation bbox outside page');
    assert.ok((box.x1 - box.x0) * (box.y1 - box.y0) > 0, 'degenerate annotation area');
  }
}

export function iou(a, b) {
  const intersection = Math.max(0, Math.min(a.x1, b.x1) - Math.max(a.x0, b.x0)) *
    Math.max(0, Math.min(a.y1, b.y1) - Math.max(a.y0, b.y0));
  const area = (box) => (box.x1 - box.x0) * (box.y1 - box.y0);
  return intersection / (area(a) + area(b) - intersection);
}

// Rectangular Hungarian assignment, minimizing 1-IoU. Each prediction/reference
// can contribute once. Unlike greedy matching, overlapping duplicates cannot
// inflate coverage by claiming the same reference repeatedly.
function assignment(weights) {
  const rows = weights.length;
  if (!rows) return [];
  const columns = weights[0].length;
  const u = new Float64Array(rows + 1), v = new Float64Array(columns + 1);
  const p = new Int32Array(columns + 1), way = new Int32Array(columns + 1);
  for (let row = 1; row <= rows; row++) {
    p[0] = row;
    let column = 0;
    const minimum = new Float64Array(columns + 1).fill(Infinity);
    const used = new Uint8Array(columns + 1);
    do {
      used[column] = 1;
      const currentRow = p[column];
      let delta = Infinity, nextColumn = 0;
      for (let candidate = 1; candidate <= columns; candidate++) {
        if (used[candidate]) continue;
        const cost = 1 - weights[currentRow - 1][candidate - 1] - u[currentRow] - v[candidate];
        if (cost < minimum[candidate]) {
          minimum[candidate] = cost;
          way[candidate] = column;
        }
        if (minimum[candidate] < delta) {
          delta = minimum[candidate];
          nextColumn = candidate;
        }
      }
      for (let candidate = 0; candidate <= columns; candidate++) {
        if (used[candidate]) { u[p[candidate]] += delta; v[candidate] -= delta; }
        else minimum[candidate] -= delta;
      }
      column = nextColumn;
    } while (p[column] !== 0);
    do {
      const previous = way[column];
      p[column] = p[previous];
      column = previous;
    } while (column !== 0);
  }
  return Array.from(p).flatMap((row, column) => column && row ? [[row - 1, column - 1]] : []);
}

export function scoreSample(sample) {
  validateRegions(sample.predicted_regions, sample.page_size_pt);
  validateRegions(sample.reference_regions, sample.page_size_pt);
  assert.ok(['NOT_REVIEWED', 'REVIEWED'].includes(sample.visual_review_status), 'invalid review state');
  assert.equal(typeof sample.reference_regions_complete, 'boolean', 'reference coverage must be explicit');
  for (const key of reviewChecks) {
    assert.ok(sample[key] === null || typeof sample[key] === 'boolean', 'invalid content review check');
  }
  if (sample.visual_review_status !== 'REVIEWED' || !sample.reference_regions_complete) {
    return { status: 'NOT_SCORED', reason: 'references_not_reviewed_or_incomplete', metrics: null };
  }
  const predictions = sample.predicted_regions, references = sample.reference_regions;
  const transpose = predictions.length < references.length;
  const rows = transpose ? predictions : references;
  const columns = transpose ? references : predictions;
  const weights = rows.map((a) => columns.map((b) => a.kind === b.kind ? iou(a.bbox, b.bbox) : 0));
  const pairs = columns.length ? assignment(weights).filter(([r, c]) => weights[r][c] > 0)
    .map(([r, c]) => ({ reference_id: (transpose ? columns[c] : rows[r]).id,
      prediction_id: (transpose ? rows[r] : columns[c]).id, iou: weights[r][c] })) : [];
  const sum = pairs.reduce((value, pair) => value + pair.iou, 0);
  return {
    status: 'DIAGNOSTIC_ONLY',
    metrics: { references: references.length, predictions: predictions.length,
      positive_overlap_pairs: pairs.length,
      unmatched_reference_ids: references.filter((r) => !pairs.some((p) => p.reference_id === r.id)).map((r) => r.id),
      unmatched_prediction_ids: predictions.filter((r) => !pairs.some((p) => p.prediction_id === r.id)).map((r) => r.id),
      mean_iou_per_reference: references.length ? sum / references.length : null,
      mean_iou_per_prediction: predictions.length ? sum / predictions.length : null,
      iou_sum: sum, pairs },
  };
}
