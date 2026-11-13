// Check private artifact integrity/geometry without displaying lecture content.
// Supports both the older per-PDF page-summary index and the full-deck index.
import assert from 'node:assert/strict';
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { webpDimensions } from './webp_dimensions.mjs';

const indexPath = process.argv[2];
assert(indexPath, 'usage: node tools/eval/check_region_probes.mjs PRIVATE_INDEX_JSON');
const index = JSON.parse(readFileSync(indexPath, 'utf8'));
const indexDirectory = path.dirname(path.resolve(indexPath));
const repoRoot = fileURLToPath(new URL('../../', import.meta.url));
const allowedFailures = new Set(['no_chunks']);
// Explicit coverage gaps are reported, not silently treated as full vector
// evidence. They do not make the artifact structurally invalid, but they do
// keep G-1 open.
const explicitPartialCode = (code) =>
  code === 'vector_template_deck_incomplete' ||
  code === 'filtered_vector_use' ||
  code.startsWith('unsupported_vector_') ||
  code.startsWith('vector_use_');
const allowedCropKinds = new Set(['vector', 'image', 'math', 'ocr_line', 'page']);

const mode = (file) => statSync(file).mode & 0o777;
const pngSize = (file) => {
  const bytes = readFileSync(file);
  assert.equal(bytes.subarray(1, 4).toString(), 'PNG', `not a PNG: ${file}`);
  return [bytes.readUInt32BE(16), bytes.readUInt32BE(20)];
};
const area = (box) => (box.x1 - box.x0) * (box.y1 - box.y0);
const pixels = (value, dpi) => Math.fround(value) * dpi / 72;

function resolveRun(report) {
  if (report.run_directory) {
    for (const candidate of [
      path.resolve(repoRoot, report.run_directory),
      path.resolve(indexDirectory, '../../..', report.run_directory),
    ]) {
      if (existsSync(candidate)) return candidate;
    }
  }
  if (report.run_id) return path.join(indexDirectory, report.run_id);
  assert.fail('report has neither run_directory nor run_id');
}

function expectedPixelBbox(box, dpi) {
  const x = Math.floor(pixels(box.x0, dpi));
  const y = Math.floor(pixels(box.y0, dpi));
  const width = Math.ceil(pixels(box.x1, dpi)) - x;
  const height = Math.ceil(pixels(box.y1, dpi)) - y;
  return [x, y, width, height];
}

let pdfs = 0;
let pages = 0;
let crops = 0;
let overlays = 0;
let masks = 0;
let vectorComponents = 0;
const versions = new Set();
const explicitPartialCodes = {};

for (const report of index.reports) {
  pdfs += 1;
  assert.match(String(report.status).toLowerCase(), /^(partial|succeeded)$/);
  assert.equal(report.annotated_corpus_acceptance ?? index.annotated_corpus_acceptance, false);
  const ocrConfigured = Boolean(report.ocr_model_configured ?? index.ocr_model_configured);
  assert.equal(
    report.leased_model_inference_performed ?? report.inference_performed ?? index.leased_model_inference_performed,
    ocrConfigured,
  );
  if (ocrConfigured) {
    assert.equal(report.model_release_failed, false, 'managed OCR run must not fail release containment');
    assert.equal(report.model_release_oom_kills ?? 0, 0, 'managed OCR run must not have cgroup OOM kills');
  }
  if ('private_modes' in report) assert.equal(report.private_modes, true);
  if ('bad_crops' in report) assert.equal(report.bad_crops, 0, 'crop integrity failed');
  if ('text_crops' in report) assert.equal(report.text_crops, 0, 'digital text must not be cropped');

  const runDirectory = resolveRun(report);
  assert.equal(mode(runDirectory), 0o700, 'run directory must be private');
  const manifestPath = path.join(runDirectory, 'manifest.json');
  const manifest = JSON.parse(readFileSync(existsSync(manifestPath) ? manifestPath :
    path.join(runDirectory, 'region-probe.json'), 'utf8'));
  const legacy = manifest.pipeline_version !== 'm2-pipeline-v4-vector-consolidation';
  const knownPartialCode = (code) => explicitPartialCode(code) || (legacy && [
    'deck_template_sampling_incomplete', 'vector_mask_and_clusters_pending',
    'raster_60_percent_fallback_requires_vector_analysis',
  ].includes(code));
  versions.add(manifest.pipeline_version);
  assert.equal(manifest.source_pages, report.source_pages);

  const failureCounts = {};
  const failures = manifest.failures ?? (manifest.pages ?? []).flatMap((summary) => {
    assert.notEqual(summary.status, 'failed');
    const artifact = JSON.parse(readFileSync(path.join(runDirectory, 'pages',
      `page-${String(summary.page).padStart(4, '0')}.json`), 'utf8'));
    return artifact.failures;
  });
  for (const failure of failures) {
    failureCounts[failure.code] = (failureCounts[failure.code] ?? 0) + 1;
    const allowedNoOcr = !ocrConfigured && failure.code === 'ocr_model_not_configured';
    assert.ok(
      allowedNoOcr || allowedFailures.has(failure.code) || knownPartialCode(failure.code),
      `unexpected failure code: ${failure.code}`,
    );
    if (knownPartialCode(failure.code)) {
      explicitPartialCodes[failure.code] = (explicitPartialCodes[failure.code] ?? 0) + 1;
    }
  }
  assert.deepEqual(failureCounts, report.failure_counts, 'failure counts must match artifacts');
  if (report.geometry_warnings !== undefined) assert.equal(report.geometry_warnings, 0);
  for (const code of [
    'crop_failed',
    'invalid_analysis_raster',
    'ocr_failed',
    'region_ocr_failed',
    'region_plan_failed',
    'region_consolidation_failed',
    'render_failed',
    'scan_layout_failed',
    'scan_region_isolation_failed',
    'vector_analysis_failed',
    'vector_svg_load_failed',
  ]) {
    assert.equal(failureCounts[code] ?? 0, 0, `${code} must not occur`);
  }

  const pageDirectory = path.join(runDirectory, 'pages');
  assert.equal(mode(pageDirectory), 0o700);
  assert.equal(mode(path.join(runDirectory, 'crops')), 0o700);
  const pageFiles = report.pages
    ? report.pages.map((summary) => `page-${String(summary.page).padStart(4, '0')}.json`)
    : readdirSync(pageDirectory).filter((name) => /^page-\d+\.json$/.test(name)).sort();
  const expectedPages = report.pages ? report.pages.length : report.source_pages ?? pageFiles.length;
  assert.equal(pageFiles.length, expectedPages);
  assert.equal(new Set(pageFiles).size, pageFiles.length);

  const recomputed = {
    crops: 0,
    overlays: 0,
    masks: 0,
    vector_pages: 0,
    vector_primitives: 0,
    vector_components: 0,
    vector_eligible_components: 0,
    vector_excluded_backgrounds: 0,
    vector_excluded_text_glyphs: 0,
    vector_excluded_templates: 0,
    crop_kinds: {},
    action_counts: {},
    template_status: {},
  };

  for (const name of pageFiles) {
    const artifact = JSON.parse(readFileSync(path.join(pageDirectory, name), 'utf8'));
    assert.equal(artifact.pipeline_version, manifest.pipeline_version);
    assert.equal(artifact.render_dpi, 150);
    for (const code of artifact.actions.incomplete) {
      assert.ok(knownPartialCode(code), `unexpected incomplete page analysis: ${code}`);
      assert.ok(artifact.failures.some((failure) => failure.code === code),
        'incomplete analysis must be surfaced as a failure');
    }
    assert.equal(name, `page-${String(artifact.page_number).padStart(4, '0')}.json`);
    if (!legacy) {
      assert.ok(artifact.vector_evidence, 'v4 requires vector evidence on every processed page');
      assert.ok(artifact.vector_mask_path, 'v4 requires a persisted vector mask');
    }
    pages += 1;

    const pageWidth = Math.ceil(artifact.width_pt * 150 / 72);
    const pageHeight = Math.ceil(artifact.height_pt * 150 / 72);

    const overlayPath = path.join(runDirectory, artifact.overlay_path);
    assert.deepEqual(pngSize(overlayPath), [pageWidth, pageHeight]);
    assert.equal(mode(overlayPath), 0o600, 'overlay must be private');
    overlays += 1;
    recomputed.overlays += 1;

    if (artifact.vector_mask_path) {
      const maskPath = path.join(runDirectory, artifact.vector_mask_path);
      const evidence = artifact.vector_evidence;
      assert.ok(evidence, 'mask without vector evidence');
      assert.deepEqual(pngSize(maskPath), [evidence.mask_width, evidence.mask_height]);
      assert.equal(mode(maskPath), 0o600, 'vector mask must be private');
      masks += 1;
      recomputed.masks += 1;
    }

    for (const action of artifact.actions.actions) {
      assert.ok(action.bbox.x0 >= 0 && action.bbox.y0 >= 0);
      assert.ok(action.bbox.x1 <= artifact.width_pt && action.bbox.y1 <= artifact.height_pt);
      recomputed.action_counts[action.kind] = (recomputed.action_counts[action.kind] ?? 0) + 1;
      const fraction = area(action.bbox) / (artifact.width_pt * artifact.height_pt);
      if (action.kind === 'ACTION_CROP_RASTER_FIGURES') {
        assert.ok(fraction >= 0.04 && fraction < 0.85, 'raster action outside frozen bounds');
        assert.notEqual(action.reason, 'single_raster_covers_85_percent');
      }
      if (action.kind === 'ACTION_CROP_VECTOR_CLUSTERS') {
        assert.ok(fraction >= 0.04 && fraction < 0.90, 'vector action outside frozen bounds');
      }
    }

    const evidence = artifact.vector_evidence;
    if (evidence) {
      recomputed.vector_pages += 1;
      recomputed.vector_primitives += evidence.primitive_count;
      recomputed.vector_components += evidence.components.length;
      recomputed.vector_excluded_backgrounds += evidence.excluded_backgrounds;
      recomputed.vector_excluded_text_glyphs += evidence.excluded_text_glyphs ?? 0;
      recomputed.vector_excluded_templates += evidence.excluded_templates;
      recomputed.template_status[evidence.template_status] =
        (recomputed.template_status[evidence.template_status] ?? 0) + 1;
      for (const code of evidence.incomplete) {
        assert.ok(knownPartialCode(code), `unexpected incomplete evidence: ${code}`);
      }
      for (const component of evidence.components) {
        vectorComponents += 1;
        if (!component.eligible) continue;
        recomputed.vector_eligible_components += 1;
        assert.ok(component.segments >= 25, 'eligible vector component has too few segments');
        const fraction = area(component.bbox) / (artifact.width_pt * artifact.height_pt);
        assert.ok(fraction >= 0.04 && fraction < 0.90, 'eligible vector component outside area bounds');
        assert.ok(component.non_text_ink_ratio >= 0.35, 'eligible vector component is text-covered');
        assert.ok(component.path_ink_ratio >= 0.05, 'eligible vector component has too little path ink');
      }
    }

    const scan = artifact.actions.actions.some((action) => action.kind === 'ACTION_OCR_FULL_PAGE_SCAN');
    for (const crop of artifact.crops) {
      crops += 1;
      recomputed.crops += 1;
      recomputed.crop_kinds[crop.kind] = (recomputed.crop_kinds[crop.kind] ?? 0) + 1;
      assert.ok(allowedCropKinds.has(crop.kind) || (legacy && crop.kind === 'text'),
        `unexpected crop kind: ${crop.kind}`);
      assert.equal(crop.dpi, 250);
      assert.ok(crop.width > 0 && crop.height > 0);
      assert.ok(crop.width <= 8192 && crop.height <= 8192);
      assert.ok(crop.width * crop.height <= 24000000);
      const box = crop.bbox;
      assert.ok(box.x0 >= 0 && box.y0 >= 0);
      assert.ok(box.x1 <= artifact.width_pt && box.y1 <= artifact.height_pt);
      const expected = expectedPixelBbox(box, 250);
      assert.deepEqual(crop.pixel_bbox, expected, 'pixel_bbox is [x,y,width,height] at 250 DPI');
      assert.deepEqual([crop.width, crop.height], expected.slice(2));
      const file = path.join(runDirectory, crop.relative_path);
      const bytes = readFileSync(file);
      assert.equal(bytes.subarray(0, 4).toString(), 'RIFF');
      assert.equal(bytes.subarray(8, 12).toString(), 'WEBP');
      assert.deepEqual(webpDimensions(bytes), [crop.width, crop.height], 'encoded WebP dimensions');
      assert.equal(bytes.length, crop.bytes);
      assert.ok(bytes.length <= 8 * 1024 * 1024);
      assert.equal(createHash('sha256').update(bytes).digest('hex'), crop.sha256);
      assert.equal(mode(file), 0o600, 'crop must be private');
      assert.ok(existsSync(path.join(runDirectory, 'crops', `${crop.id}.webp`)));
      if (scan) {
        assert.ok(area(box) / (artifact.width_pt * artifact.height_pt) < 0.85,
          'full-page scan must not become a giant raw crop');
      }
    }
  }

  for (const key of Object.keys(recomputed)) {
    if (key === 'crop_kinds' || key === 'action_counts' || key === 'template_status') continue;
    if (report[key] !== undefined) {
      assert.equal(report[key], recomputed[key], `aggregate mismatch for ${key}`);
    }
  }
  if (report.crop_kinds) assert.deepEqual(report.crop_kinds, recomputed.crop_kinds);
  if (report.action_counts) assert.deepEqual(report.action_counts, recomputed.action_counts);
  if (typeof report.vector_template_status === 'object') {
    assert.deepEqual(report.vector_template_status, recomputed.template_status);
  } else if (typeof report.vector_template_status === 'string') {
    assert.deepEqual(Object.keys(recomputed.template_status), [report.vector_template_status]);
  }
  if (report.pages_failed !== undefined) assert.equal(report.pages_failed, 0);
  if (report.pages_succeeded !== undefined) assert.equal(report.pages_succeeded, report.source_pages);
}

console.log(JSON.stringify({
  status: 'PASS',
  pdfs,
  pages,
  crops,
  overlays,
  masks,
  vector_components: vectorComponents,
  explicit_partial_codes: explicitPartialCodes,
  pipeline_versions: [...versions],
  scope: 'Private file integrity, numeric geometry, DPI, modes and frozen region predicates only; not annotated G-1 acceptance.',
}, null, 2));
