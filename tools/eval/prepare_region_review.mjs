// Prepare a host-only review packet. Does not view images, infer annotations,
// upload derivatives or claim independent review. Open review.html locally.
import assert from 'node:assert/strict';
import { mkdirSync, readFileSync, readdirSync, realpathSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));
const privateRoot = realpathSync(path.join(root, 'artifacts/private'));
const contained = (file) => {
  const resolved = realpathSync(file);
  assert.ok(resolved.startsWith(`${privateRoot}${path.sep}`), 'review inputs must stay private');
  return resolved;
};
assert.ok(process.argv[2], 'usage: node tools/eval/prepare_region_review.mjs PRIVATE_INDEX_JSON');
const input = contained(process.argv[2]);
const index = JSON.parse(readFileSync(input, 'utf8'));
const output = path.join(path.dirname(input), `review-${Date.now()}`);
mkdirSync(output, { mode: 0o700 });
const escape = (value) => String(value).replace(/[&<>"']/g, (char) =>
  ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[char]);
const url = (file) => path.relative(output, contained(file)).split(path.sep).map(encodeURIComponent).join('/');
const image = (file, label) => {
  const relative = url(file);
  return `<figure><img loading="lazy" src="${escape(relative)}" alt="${escape(label)}"><figcaption>${escape(label)}</figcaption></figure>`;
};
const sections = [];
const samples = [];
const visuals = [];
for (const report of index.reports) {
  const run = contained(path.resolve(root, report.run_directory));
  const pages = readdirSync(path.join(run, 'pages')).filter((name) => /^page-\d+\.json$/.test(name))
    .sort().map((name) => JSON.parse(readFileSync(path.join(run, 'pages', name), 'utf8')));
  const selected = new Map();
  const pick = (page, reason) => {
    if (!page) return;
    const entry = selected.get(page.page_number) ?? { page, reasons: [] };
    entry.reasons.push(reason);
    selected.set(page.page_number, entry);
  };
  for (const number of [1, 2, 10]) pick(pages.find((page) => page.page_number === number), 'fixed sample');
  pick([...pages].sort((a, b) => b.crops.length - a.crops.length)[0], 'highest crop count');
  const fraction = (page, crop) => (crop.bbox.x1 - crop.bbox.x0) * (crop.bbox.y1 - crop.bbox.y0) /
    (page.width_pt * page.height_pt);
  const vectors = pages.flatMap((page) => page.crops.filter((crop) => crop.kind === 'vector')
    .map((crop) => ({ page, fraction: fraction(page, crop) }))).sort((a, b) => b.fraction - a.fraction);
  pick(vectors[0]?.page, 'largest vector composite');
  for (const page of pages) {
    if (page.vector_evidence?.incomplete.length) pick(page, 'explicit vector coverage gap');
  }
  pick(pages.find((page) => page.actions.actions.some((action) => action.kind === 'ACTION_OCR_FULL_PAGE_SCAN') &&
    page.crops.some((crop) => crop.kind === 'image')), 'scan action with image crop(s)');
  for (const { page, reasons } of [...selected.values()].sort((a, b) => a.page.page_number - b.page.page_number)) {
    const pageId = `page-${String(page.page_number).padStart(4, '0')}`;
    samples.push({
      category: report.category, run_id: page.run_id, page: page.page_number,
      artifact_sha256: createHash('sha256').update(readFileSync(path.join(run, 'pages', `${pageId}.json`))).digest('hex'),
      selection_reasons: reasons, page_size_pt: [page.width_pt, page.height_pt],
      crop_count: page.crops.length,
      predicted_regions: page.crops.map((crop) => ({ id: crop.id, kind: crop.kind, bbox: crop.bbox })),
      vector_incomplete: page.vector_evidence?.incomplete ?? [],
      visual_review_status: 'NOT_REVIEWED', reference_regions_complete: false, reference_regions: [],
      formula_numbers_units_negation_preserved: null, code_indentation_preserved: null,
      diagram_labels_complete: null, prose_barrier_respected: null, reviewer_notes: '',
    });
    visuals.push({
      source: url(path.join(run, 'pages', `${pageId}.png`)),
      overlay: url(path.join(run, page.overlay_path)),
      crops: page.crops.map((crop) => ({ id: crop.id, kind: crop.kind,
        url: url(path.join(run, crop.relative_path)) })),
    });
    sections.push(`<section><h2>${escape(report.category)} / page ${page.page_number}</h2>
      <p>${escape(reasons.join('; '))}. Crops: ${page.crops.length}. Not visually reviewed.</p>
      <div class="pages">${image(path.join(run, 'pages', `${pageId}.png`), 'Source analysis raster')}
      ${image(path.join(run, page.overlay_path), 'Action overlay')}</div>
      <div class="crops">${page.crops.map((crop) => image(path.join(run, crop.relative_path),
        `${crop.kind} / ${crop.id}`)).join('')}</div></section>`);
  }
}
const packet = {
  schema_version: 2, index: path.relative(root, input), pipeline_versions: [...new Set(index.reports.map((r) => r.pipeline_version))],
  privacy: 'Host-local only. Do not upload this packet or its linked derivatives.',
  selection: 'Agent-assisted metadata sampling, not visual annotation or independent review.',
  independent_review: false, reviewer: null, reviewed_at: null,
  status: 'NOT_REVIEWED', annotated_corpus_acceptance: false, iou: null, samples,
};
writeFileSync(path.join(output, 'annotations.json'), JSON.stringify(packet, null, 2), { mode: 0o600 });
writeFileSync(path.join(output, 'review.html'), `<!doctype html><html lang="en"><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src file:; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'">
<title>Private M2 review — unannotated</title><style>body{font:16px sans-serif;margin:2rem;background:#eee}
section{border-top:2px solid #777;padding:1rem 0}figure{margin:.5rem;background:white;padding:.5rem}
img{max-width:100%;max-height:650px}figcaption{font-size:12px}.pages,.crops{display:flex;flex-wrap:wrap}
.pages figure{width:46%}.crops figure{max-width:45%}</style><h1>Host-local M2 review packet</h1>
<p>NOT REVIEWED. No inference, upload, IoU score or independent annotation has been performed.
Open locally only; do not use a remote browser or annotation service. Record reviewer provenance and reference boxes
in annotations.json using top-left CropBox points. Reference regions use unique id, kind (image, vector, math),
and bbox {x0,y0,x1,y1}. Set reference_regions_complete=true and visual_review_status=REVIEWED only after
annotating all target crop types on that sample. Supply reviewer and reviewed_at provenance. Empty reference
lists mean unannotated unless explicitly reviewed and declared complete. Scoring is diagnostic, never G-1 acceptance.</p>
${sections.join('\n')}</html>`, { mode: 0o600 });
const annotatorTemplate = readFileSync(path.join(root, 'tools/eval/review_annotator.html'), 'utf8');
assert.ok(annotatorTemplate.includes('__PACKET__') && annotatorTemplate.includes('__VISUALS__'),
  'review_annotator.html is missing its packet placeholders');
const inlineJson = (value) => JSON.stringify(value).replace(/</g, '\\u003c');
writeFileSync(path.join(output, 'review_annotator.html'),
  annotatorTemplate.replace('__PACKET__', () => inlineJson(packet))
    .replace('__VISUALS__', () => inlineJson(visuals)),
  { mode: 0o600 });
console.log(JSON.stringify({ status: 'NOT_REVIEWED', samples: samples.length,
  directory: path.relative(root, output), annotator: 'review_annotator.html',
  independent_review: false, iou: null }, null, 2));
