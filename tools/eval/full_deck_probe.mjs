// Full-deck private probe. Runs the contained ingestion CLI on every page of
// each selected PDF, then verifies crop identity, 250-DPI metadata, private
// modes and vector evidence. Managed local OCR is opt-in; no network call or
// upload is used.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { webpDimensions } from './webp_dimensions.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));
const recheck = process.argv[2] === '--recheck';
if (recheck && !process.argv[3]) throw new Error('--recheck requires an existing index.json');
const previous = recheck ? JSON.parse(readFileSync(process.argv[3], 'utf8')) : null;
const corpus = recheck ? null : JSON.parse(readFileSync(path.join(root, 'artifacts/private/corpus.json'), 'utf8'));
const releaseBinary = path.join(root, 'target/release/campuslingo-ingestion');
const debugBinary = path.join(root, 'target/debug/campuslingo-ingestion');
const binary = existsSync(releaseBinary) ? releaseBinary : debugBinary;
const modelHostReserveBytes = 3 * 1024 * 1024 * 1024;
const managedOcr = !recheck && process.argv.includes('--managed-ocr');
const modelArgument = process.argv.includes('--model') ? process.argv[process.argv.indexOf('--model') + 1] : null;
const mmprojArgument = process.argv.includes('--mmproj') ? process.argv[process.argv.indexOf('--mmproj') + 1] : null;
const threadArgument = process.argv.includes('--threads') ? process.argv[process.argv.indexOf('--threads') + 1] : null;
const ocrDeviceArgument = process.argv.includes('--ocr-device') ? process.argv[process.argv.indexOf('--ocr-device') + 1] : null;
if (managedOcr && (!modelArgument || !mmprojArgument)) {
  throw new Error('--managed-ocr requires --model and --mmproj');
}
const base = recheck ? path.dirname(path.resolve(process.argv[3])) :
  path.join(root, 'artifacts/private', `m2-full-deck-${Date.now()}`);
mkdirSync(base, { recursive: true, mode: 0o700 });

const mode = (file) => statSync(file).mode & 0o777;
const sha256 = (file) => createHash('sha256').update(readFileSync(file)).digest('hex');

const reports = [];
const wanted = new Set(recheck ? [] : process.argv.slice(2).filter((arg) => corpus.selected[arg]));
const records = recheck ? previous.reports.map((report) => [report.category, report]) : Object.entries(corpus.selected);
for (const [category, record] of records) {
  if (wanted.size > 0 && !wanted.has(category)) continue;
  const output = path.join(base, category);
  mkdirSync(output, { recursive: true, mode: 0o700 });
  const started = Date.now();
  const run = recheck ? { status: 0, stdout: readFileSync(path.join(root, record.run_directory, 'manifest.json'), 'utf8') } : spawnSync(
    binary,
    ['extract', '--pdf', record.path, '--output', output,
      ...(managedOcr ? ['--managed-ocr', '--model', modelArgument, '--mmproj', mmprojArgument,
        ...(threadArgument ? ['--threads', threadArgument] : []),
        ...(ocrDeviceArgument ? ['--ocr-device', ocrDeviceArgument] : [])] : [])],
    {
      cwd: root,
      env: process.env,
      encoding: 'utf8',
      maxBuffer: 256 * 1024 * 1024,
      timeout: 0,
    },
  );
  if (run.error || run.status !== 0) {
    reports.push({
      category,
      status: 'FAILED',
      pages: record.pages,
      exit_status: run.status,
      error: String(run.error ?? ''),
      stderr: (run.stderr ?? '').slice(-4000),
      duration_ms: Date.now() - started,
    });
    continue;
  }
  const stdoutManifest = JSON.parse(run.stdout);
  const runDir = path.join(output, stdoutManifest.run_id);
  const manifest = {
    ...JSON.parse(readFileSync(path.join(runDir, 'manifest.json'), 'utf8')),
    ...stdoutManifest,
  };
  const pageDir = path.join(runDir, 'pages');
  const cropDir = path.join(runDir, 'crops');
  const failures = {};
  for (const failure of manifest.failures ?? []) {
    failures[failure.code] = (failures[failure.code] ?? 0) + 1;
  }
  const modelReleases = readdirSync(output)
    .filter((name) => /^llama-ocr-.*\.release\.json$/.test(name))
    .sort()
    .map((name) => {
      const file = path.join(output, name);
      return { name, ...JSON.parse(readFileSync(file, 'utf8')) };
    });
  const releasePeak = modelReleases.reduce((peak, release) => Math.max(peak, release.cgroup_memory_peak_bytes ?? 0), 0);
  const releaseProcessPeakRss = modelReleases.reduce((peak, release) => Math.max(peak, release.process_peak_rss_bytes ?? 0), 0);
  const releaseOom = modelReleases.reduce((count, release) => count + (release.cgroup_oom_kill_count ?? 0), 0);
  const releaseMinimumHost = modelReleases.reduce((minimum, release) =>
    release.minimum_host_available_bytes == null ? minimum :
      Math.min(minimum, release.minimum_host_available_bytes), Infinity);
  const releaseFailed = modelReleases.some((release) =>
    release.termination || release.memory_budget_exceeded ||
    (release.minimum_host_available_bytes != null &&
      release.minimum_host_available_bytes < modelHostReserveBytes));
  let crops = 0;
  let cropBytes = 0;
  let badCrops = 0;
  let textCrops = 0;
  let overlays = 0;
  let masks = 0;
  let vectorPages = 0;
  let vectorPrimitives = 0;
  let vectorComponents = 0;
  let vectorEligible = 0;
  let excludedBackgrounds = 0;
  let excludedGlyphs = 0;
  let excludedTemplates = 0;
  const templateStatus = {};
  const cropKinds = {};
  const actionCounts = {};
  const pageJson = readdirSync(pageDir).filter((name) => /^page-\d+\.json$/.test(name)).sort();
  for (const name of pageJson) {
    const artifact = JSON.parse(readFileSync(path.join(pageDir, name), 'utf8'));
    for (const action of artifact.actions?.actions ?? []) {
      actionCounts[action.kind] = (actionCounts[action.kind] ?? 0) + 1;
    }
    if (artifact.overlay_path) {
      const overlay = path.join(runDir, artifact.overlay_path);
      if (statSync(overlay).isFile() && mode(overlay) === 0o600) overlays += 1;
    }
    if (artifact.vector_evidence) {
      vectorPages += 1;
      const evidence = artifact.vector_evidence;
      vectorPrimitives += evidence.primitive_count;
      vectorComponents += evidence.components.length;
      vectorEligible += evidence.components.filter((component) => component.eligible).length;
      excludedBackgrounds += evidence.excluded_backgrounds;
      excludedGlyphs += evidence.excluded_text_glyphs ?? 0;
      excludedTemplates += evidence.excluded_templates;
      templateStatus[evidence.template_status] = (templateStatus[evidence.template_status] ?? 0) + 1;
    }
    if (artifact.vector_mask_path) {
      const mask = path.join(runDir, artifact.vector_mask_path);
      if (statSync(mask).isFile() && mode(mask) === 0o600) masks += 1;
    }
    for (const crop of artifact.crops ?? []) {
      crops += 1;
      cropKinds[crop.kind] = (cropKinds[crop.kind] ?? 0) + 1;
      if (crop.kind === 'text') textCrops += 1;
      const file = path.join(runDir, crop.relative_path);
      const bytes = readFileSync(file);
      const [width, height] = webpDimensions(bytes) ?? [0, 0];
      const [x, y, pixelWidth, pixelHeight] = crop.pixel_bbox;
      const pageWidth = Math.ceil((artifact.width_pt * 250) / 72);
      const pageHeight = Math.ceil((artifact.height_pt * 250) / 72);
      const valid =
        crop.dpi === 250 &&
        mode(file) === 0o600 &&
        bytes.length === crop.bytes &&
        sha256(file) === crop.sha256 &&
        width === crop.width &&
        height === crop.height &&
        pixelWidth === width &&
        pixelHeight === height &&
        x >= 0 &&
        y >= 0 &&
        x + pixelWidth <= pageWidth &&
        y + pixelHeight <= pageHeight &&
        pixelWidth > 0 &&
        pixelHeight > 0 &&
        statSync(path.join(cropDir, `${crop.id}.webp`)).isFile();
      if (!valid) badCrops += 1;
      cropBytes += bytes.length;
    }
  }
  const runMode = mode(runDir);
  const pagesMode = mode(pageDir);
  const cropsMode = mode(cropDir);
  reports.push({
    category,
    pipeline_version: manifest.pipeline_version,
    status: manifest.status,
    source_pages: manifest.source_pages,
    pages_succeeded: manifest.totals?.pages_succeeded ?? 0,
    pages_failed: manifest.totals?.pages_failed ?? 0,
    crops,
    crop_bytes: cropBytes,
    crop_kinds: cropKinds,
    text_crops: textCrops,
    bad_crops: badCrops,
    overlays,
    masks,
    vector_pages: vectorPages,
    vector_primitives: vectorPrimitives,
    vector_components: vectorComponents,
    vector_eligible_components: vectorEligible,
    vector_excluded_backgrounds: excludedBackgrounds,
    vector_excluded_text_glyphs: excludedGlyphs,
    vector_excluded_templates: excludedTemplates,
    vector_template_status: templateStatus,
    action_counts: actionCounts,
    failure_counts: failures,
    run_directory: path.relative(root, runDir),
    private_modes: runMode === 0o700 && pagesMode === 0o700 && cropsMode === 0o700,
    ocr_model_configured: recheck ? (record.ocr_model_configured ?? false) : managedOcr,
    ocr_pages: manifest.totals?.ocr_pages ?? 0,
    model_peak_rss_bytes: manifest.model_peak_rss_bytes ?? null,
    model_restarts: manifest.model_restarts ?? record.model_restarts ?? null,
    model_releases: modelReleases.map((release) => ({
      file: release.name,
      process_tree_reaped: release.process_tree_reaped,
      cgroup_removed: release.cgroup_removed,
      cgroup_memory_peak_bytes: release.cgroup_memory_peak_bytes,
      cgroup_oom_kill_count: release.cgroup_oom_kill_count,
      process_peak_rss_bytes: release.process_peak_rss_bytes ?? null,
      minimum_host_available_bytes: release.minimum_host_available_bytes ?? null,
      termination: release.termination,
      memory_budget_exceeded: release.memory_budget_exceeded,
    })),
    model_release_peak_bytes: releasePeak || null,
    model_release_process_peak_rss_bytes: releaseProcessPeakRss || null,
    model_release_minimum_host_available_bytes:
      Number.isFinite(releaseMinimumHost) ? releaseMinimumHost : null,
    model_release_oom_kills: releaseOom,
    model_release_failed: releaseFailed,
    annotation_present: false,
    acceptance: 'PARTIAL',
    duration_ms: recheck ? record.duration_ms : Date.now() - started,
  });
}
const modelIdentity = managedOcr ? {
  model_path: path.relative(root, path.resolve(modelArgument)),
  model_sha256: sha256(path.resolve(modelArgument)),
  mmproj_path: path.relative(root, path.resolve(mmprojArgument)),
  mmproj_sha256: sha256(path.resolve(mmprojArgument)),
  threads: threadArgument ? Number(threadArgument) : 4,
  ocr_device: ocrDeviceArgument ?? 'auto',
  budget_bytes: 2 * 1024 * 1024 * 1024,
  decision: 'DEC-017',
} : (previous?.model_identity ?? null);
const summary = {
  schema_version: 1,
  observed_at: new Date().toISOString(),
  privacy: 'local evaluation only; source PDFs, text and crops were not uploaded',
  leased_model_inference_performed: recheck
    ? previous.leased_model_inference_performed
    : managedOcr,
  tesseract_geometry_attempted: true,
  ocr_model_configured: recheck ? previous.ocr_model_configured : managedOcr,
  model_identity: modelIdentity,
  annotated_corpus_acceptance: false,
  g1_status: 'OPEN',
  base: path.relative(root, base),
  ...(recheck ? { rechecked_from: path.basename(process.argv[3]), original_observed_at: previous.observed_at,
    verifier_version: 'webp-binary-signature-v2' } : {}),
  reports,
};
writeFileSync(path.join(base, recheck ? 'index-rechecked.json' : 'index.json'), JSON.stringify(summary, null, 2), { mode: 0o600 });
process.stdout.write(JSON.stringify(summary, null, 2));
if (reports.some((report) => report.status === 'FAILED' || report.bad_crops > 0 ||
    report.pages_failed > 0 || !report.private_modes || report.model_release_failed)) process.exitCode = 1;
