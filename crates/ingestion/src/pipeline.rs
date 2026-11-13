//! Resumable region-aware extraction orchestration.
//!
//! A run is a private directory with one JSON artifact per completed page and a
//! final manifest. Pages that failed are not cached, so a retry of the same run
//! ID reprocesses only the missing or failed pages. Deduplication is applied
//! across all pages after extraction, including across digital and OCR routes.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{Semaphore, mpsc},
    task::JoinSet,
};
use uuid::Uuid;

use crate::{
    consolidation::{consolidate, scan_regions},
    crops::{
        CropError, CropOutput, CropTools, Region, RegionKind, crop_pdf_region, tesseract_regions,
    },
    fingerprint::{
        ChunkFingerprint, CriticalToken, DedupDecision, chunk_text, compare, fingerprint,
    },
    ocr::{OcrClient, OcrError, OcrResult},
    pdf::{
        PageFeatures, PageGeometry, PageLayout, PageRoute, PdfError, PdfInfo, PdfTools,
        classify_page,
    },
    raster::Raster,
    regions::{
        ANALYSIS_DPI, ActionKind, PageActions, RegionAction, TemplateImages, UNIFORM_VARIANCE,
        plan_page_with_vectors,
    },
};

pub const PIPELINE_VERSION: &str = "m2-pipeline-v4-vector-consolidation";
pub const DEFAULT_RENDER_DPI: u32 = ANALYSIS_DPI;
pub const DEFAULT_MAX_CHUNK_CHARS: usize = 1_200;

fn default_pipeline_concurrency() -> usize {
    if let Some(raw) = std::env::var_os("CAMPUSLINGO_PIPELINE_CONCURRENCY")
        && let Ok(parsed) = raw.to_string_lossy().parse::<usize>()
    {
        return parsed.clamp(1, 16);
    }
    std::thread::available_parallelism()
        .map(|threads| threads.get().clamp(2, 12))
        .unwrap_or(4)
}

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("probe requires 1..=12 distinct in-range page numbers and no OCR client")]
    InvalidProbe,
    #[error("pipeline file operation failed")]
    Io(#[from] std::io::Error),
    #[error("PDF validation or layout failed: {0}")]
    Pdf(#[from] PdfError),
    #[error("pipeline state could not be serialized")]
    Serialize(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub pdf: PdfTools,
    pub crops: CropTools,
    pub work_dir: PathBuf,
    pub render_dpi: u32,
    pub max_chunk_chars: usize,
    pub ocr: Option<OcrClient>,
    pub max_concurrency: usize,
}

impl PipelineConfig {
    pub fn new(work_dir: impl Into<PathBuf>) -> Self {
        Self {
            pdf: PdfTools::default(),
            crops: CropTools::default(),
            work_dir: work_dir.into(),
            render_dpi: DEFAULT_RENDER_DPI,
            max_chunk_chars: DEFAULT_MAX_CHUNK_CHARS,
            ocr: None,
            max_concurrency: default_pipeline_concurrency(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Succeeded,
    Partial,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureArtifact {
    pub page_number: u32,
    pub stage: String,
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkArtifact {
    pub id: Uuid,
    pub page_number: u32,
    pub order: u32,
    pub route: PageRoute,
    pub text: String,
    pub text_sha256: String,
    pub critical_sha256: String,
    pub critical_tokens: Vec<CriticalToken>,
    pub word_count: u32,
    pub crop_ids: Vec<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duplicate_of: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dedup: Option<DedupDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageArtifact {
    pub page_number: u32,
    pub input_sha256: String,
    pub pipeline_version: String,
    pub route: PageRoute,
    pub width_pt: f32,
    pub height_pt: f32,
    pub render_dpi: u32,
    pub features: PageFeatures,
    pub text: String,
    pub source_text: String,
    pub geometry: Option<PageGeometry>,
    pub actions: PageActions,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vector_evidence: Option<crate::vectors::VectorEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vector_mask_path: Option<String>,
    pub overlay_path: String,
    pub region_ocr: Vec<RegionOcrArtifact>,
    pub selected_regions: Vec<Region>,
    pub ocr: Option<OcrResult>,
    pub crops: Vec<CropOutput>,
    pub chunks: Vec<ChunkArtifact>,
    pub failures: Vec<FailureArtifact>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionOcrArtifact {
    pub bbox: crate::pdf::BBox,
    pub result: OcrResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageSummary {
    pub page_number: u32,
    pub status: String,
    pub route: Option<PageRoute>,
    pub crop_count: u32,
    pub chunk_count: u32,
    pub duplicate_count: u32,
    pub failure_count: u32,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RunTotals {
    pub pages: u32,
    pub pages_succeeded: u32,
    pub pages_failed: u32,
    pub crops: u32,
    pub chunks: u32,
    pub unique_chunks: u32,
    pub duplicate_chunks: u32,
    pub failures: u32,
    pub digital_pages: u32,
    pub ocr_pages: u32,
    pub hybrid_pages: u32,
    pub blank_pages: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunManifest {
    pub run_id: Uuid,
    pub pipeline_version: String,
    pub input_sha256: String,
    pub input_bytes: u64,
    pub source_pages: u32,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub duration_ms: u64,
    pub status: RunStatus,
    pub render_dpi: u32,
    pub pages: Vec<PageSummary>,
    pub totals: RunTotals,
    pub chunks: Vec<ChunkArtifact>,
    pub failures: Vec<FailureArtifact>,
    pub model: Option<String>,
    pub model_peak_rss_bytes: Option<u64>,
}

#[derive(Debug, Clone)]
pub enum PipelineEvent {
    PageStarted {
        page: u32,
        total: u32,
    },
    PageFinished {
        page: u32,
        total: u32,
        route: Option<PageRoute>,
        failures: u32,
    },
    Finished {
        status: RunStatus,
    },
}

fn send_event(events: &Option<mpsc::Sender<PipelineEvent>>, event: PipelineEvent) {
    if let Some(sender) = events {
        let _ = sender.try_send(event);
    }
}

fn failure(
    page_number: u32,
    stage: &str,
    code: &str,
    message: impl Into<String>,
    retryable: bool,
) -> FailureArtifact {
    FailureArtifact {
        page_number,
        stage: stage.into(),
        code: code.into(),
        message: message.into(),
        retryable,
    }
}

async fn set_private(path: &Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).await;
    }
    #[cfg(not(unix))]
    let _ = (path, mode);
}

async fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), PipelineError> {
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(value)?;
    {
        let mut file = tokio::fs::File::create(&temporary).await?;
        file.write_all(&bytes).await?;
        file.sync_all().await?;
    }
    set_private(&temporary, 0o600).await;
    tokio::fs::rename(&temporary, path).await?;
    Ok(())
}

async fn sha256_file(path: &Path) -> Result<(String, u64), PipelineError> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        total += read as u64;
    }
    Ok((hex::encode(hasher.finalize()), total))
}

fn chunk_id(run_id: Uuid, page: u32, order: u32, text_sha256: &str) -> Uuid {
    let mut hasher = Sha256::new();
    hasher.update(run_id.as_bytes());
    hasher.update(page.to_le_bytes());
    hasher.update(order.to_le_bytes());
    hasher.update(text_sha256.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    Uuid::from_bytes(bytes)
}

struct DeckContext {
    source: PathBuf,
    run_id: Uuid,
    run_dir: PathBuf,
    input_sha256: String,
    layouts: Vec<PageLayout>,
    templates: TemplateImages,
    vector_templates: crate::vectors::VectorTemplates,
    vector_paths: Vec<Option<PathBuf>>,
}

async fn extract_layouts(
    pdf: &PdfTools,
    source: &Path,
    page_count: u32,
    concurrency: usize,
) -> Result<Vec<PageLayout>, PipelineError> {
    let semaphore = Arc::new(Semaphore::new(concurrency.clamp(1, 16)));
    let mut set = JoinSet::new();
    for page in 1..=page_count {
        let permit = semaphore.clone();
        let pdf = pdf.clone();
        let source = source.to_path_buf();
        set.spawn(async move {
            let _guard = permit.acquire().await.map_err(|_| PdfError::Layout)?;
            pdf.layout(&source, page).await.map(|layout| (page, layout))
        });
    }
    let mut collected = Vec::with_capacity(page_count as usize);
    while let Some(joined) = set.join_next().await {
        let (page, layout) = joined.map_err(|_| PdfError::Layout)??;
        collected.push((page, layout));
    }
    collected.sort_by_key(|(page, _)| *page);
    let mut text_bytes = 0usize;
    let mut layouts = Vec::with_capacity(collected.len());
    for (_, layout) in collected {
        text_bytes = text_bytes
            .checked_add(layout.text.len())
            .ok_or(PdfError::Layout)?;
        if text_bytes > 32 * 1024 * 1024 {
            return Err(PdfError::Layout.into());
        }
        layouts.push(layout);
    }
    Ok(layouts)
}

/// Full-deck vector pre-pass. The SVG artifact is retained privately and parsed
/// again per page; template recurrence needs every page, not just the sample.
async fn prepare_vector_deck(
    source: &Path,
    layouts: &[PageLayout],
    run_dir: &Path,
    concurrency: usize,
) -> (crate::vectors::TemplateCounter, Vec<Option<PathBuf>>) {
    let semaphore = Arc::new(Semaphore::new(concurrency.clamp(1, 16)));
    let mut set = JoinSet::new();
    for (index, layout) in layouts.iter().enumerate() {
        let permit = semaphore.clone();
        let source = source.to_path_buf();
        let layout = layout.clone();
        let path = run_dir
            .join("pages")
            .join(format!("page-{:04}-vector.svg", layout.page_number));
        set.spawn(async move {
            let Ok(_guard) = permit.acquire().await else {
                return (index, layout.page_number, None, None);
            };
            match crate::vectors::extract_svg(&source, layout.page_number, &path).await {
                Ok(()) => match crate::vectors::load_svg(&path, &layout).await {
                    Ok(page) => {
                        let keys = page
                            .primitives
                            .iter()
                            .map(|primitive| primitive.key.clone())
                            .collect::<HashSet<_>>();
                        (index, page.page_number, Some(path), Some(keys))
                    }
                    Err(_) => (index, layout.page_number, None, None),
                },
                Err(_) => (index, layout.page_number, None, None),
            }
        });
    }
    let mut results = Vec::with_capacity(layouts.len());
    while let Some(joined) = set.join_next().await {
        if let Ok(item) = joined {
            results.push(item);
        }
    }
    results.sort_by_key(|(index, ..)| *index);
    let mut counter = crate::vectors::TemplateCounter::default();
    let mut paths = vec![None; layouts.len()];
    let mut observed = vec![false; layouts.len()];
    for (index, page_number, path, keys) in results {
        observed[index] = true;
        paths[index] = path;
        if let Some(keys) = keys {
            counter.observe_keys(page_number, keys);
        } else {
            counter.fail();
        }
    }
    for seen in observed {
        if !seen {
            counter.fail();
        }
    }
    (counter, paths)
}

fn associated_crops(text: &str, crops: &[CropOutput]) -> Vec<Uuid> {
    let chunk = crate::fingerprint::normalize_text(text);
    if chunk.is_empty() {
        return Vec::new();
    }
    crops
        .iter()
        .filter(|crop| {
            crop.text.as_deref().is_some_and(|crop_text| {
                let crop = crate::fingerprint::normalize_text(crop_text);
                if crop.is_empty() {
                    return false;
                }
                let crop_words = crop.split_whitespace().count();
                chunk.contains(&crop)
                    || crop.contains(&chunk)
                    // A low-confidence trailing fragment is omitted from the
                    // crop label, so a multi-word prefix still identifies the
                    // same line.
                    || (crop_words >= 2 && chunk.starts_with(&crop))
            })
        })
        .map(|crop| crop.id)
        .collect()
}

fn merge_hybrid_text(layout_text: &str, ocr_text: &str) -> String {
    let mut lines: Vec<String> = layout_text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_owned)
        .collect();
    let mut seen: Vec<String> = lines
        .iter()
        .map(|line| crate::fingerprint::normalize_text(line))
        .collect();
    for line in ocr_text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let normalized = crate::fingerprint::normalize_text(line);
        if normalized.is_empty() || seen.contains(&normalized) {
            continue;
        }
        seen.push(normalized);
        lines.push(line.to_owned());
    }
    lines.join("\n")
}

pub struct ExtractionPipeline {
    config: PipelineConfig,
}

impl ExtractionPipeline {
    pub fn new(config: PipelineConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &PipelineConfig {
        &self.config
    }

    /// Bounded probe without leased GGUF/ONNX inference. Tesseract may run for
    /// geometry. Page JSON/crops/overlays remain private; summaries contain no text.
    pub async fn probe_selected(
        &self,
        source: &Path,
        pages: &[u32],
        run_id: Uuid,
    ) -> Result<serde_json::Value, PipelineError> {
        if pages.is_empty() || pages.len() > 12 || self.config.ocr.is_some() {
            return Err(PipelineError::InvalidProbe);
        }
        let info = self.config.pdf.validate(source).await?;
        let unique = pages
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        if unique.len() != pages.len() || unique.iter().any(|&p| p == 0 || p > info.page_count) {
            return Err(PipelineError::InvalidProbe);
        }
        let (input_sha256, _) = sha256_file(source).await?;
        let run_dir = self.config.work_dir.join(run_id.to_string());
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(run_dir.join("pages"))?;
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(run_dir.join("crops"))?;
        }
        #[cfg(not(unix))]
        {
            tokio::fs::create_dir_all(run_dir.join("pages")).await?;
            tokio::fs::create_dir_all(run_dir.join("crops")).await?;
        }
        set_private(&run_dir, 0o700).await;
        // Template exclusion is a deck property. The probe therefore evaluates
        // every page's layout and vector geometry, then crops only the sample.
        let concurrency = self.config.max_concurrency.clamp(1, 16);
        let layouts =
            extract_layouts(&self.config.pdf, source, info.page_count, concurrency).await?;
        let templates = TemplateImages::detect(&layouts);
        let (counter, vector_paths) =
            prepare_vector_deck(source, &layouts, &run_dir, concurrency).await;
        let vector_templates = counter.finish();
        let template_status = serde_json::to_value(vector_templates.status)?;
        let context = Arc::new(DeckContext {
            source: source.to_path_buf(),
            run_id,
            run_dir: run_dir.clone(),
            input_sha256: input_sha256.clone(),
            layouts,
            templates,
            vector_templates,
            vector_paths,
        });
        let runner = Arc::new(Self::new(self.config.clone()));
        let mut action_counts = std::collections::BTreeMap::<String, u64>::new();
        let mut failure_counts = std::collections::BTreeMap::<String, u64>::new();
        let mut summaries = Vec::new();
        let mut crop_count = 0u64;
        let mut digital_chars = 0u64;
        let mut math_fonts = 0u64;
        let mut geometry_warnings = 0u64;
        let mut vector_primitives = 0u64;
        let mut vector_components = 0u64;
        let mut vector_eligible = 0u64;
        let mut vector_backgrounds = 0u64;
        let mut vector_text_glyphs = 0u64;
        let mut vector_templates_excluded = 0u64;
        for &page in &unique {
            let layout = &context.layouts[(page - 1) as usize];
            digital_chars += layout.text.chars().count() as u64;
            math_fonts += layout
                .lines
                .iter()
                .filter(|l| {
                    crate::regions::math_or_garbled(&l.text, l.font_family.as_deref())
                        == Some("math_font")
                })
                .count() as u64;
            geometry_warnings += layout.geometry_warnings.len() as u64;
        }
        let semaphore = Arc::new(Semaphore::new(concurrency));
        let mut set = JoinSet::new();
        for &page in &unique {
            let permit = semaphore.clone();
            let runner = Arc::clone(&runner);
            let context = Arc::clone(&context);
            set.spawn(async move {
                let _guard = permit.acquire().await.map_err(|_| PdfError::Layout)?;
                let layout = &context.layouts[(page - 1) as usize];
                let outcome = runner
                    .process_page(
                        &context.source,
                        context.run_id,
                        &context.run_dir,
                        layout,
                        &context.templates,
                        &context.vector_templates,
                        context.vector_paths[(page - 1) as usize].as_deref(),
                        page,
                        &context.input_sha256,
                        layout.images.len() as u32,
                    )
                    .await;
                if let Ok(artifact) = &outcome {
                    write_json_atomic(
                        &context
                            .run_dir
                            .join("pages")
                            .join(format!("page-{page:04}.json")),
                        artifact,
                    )
                    .await?;
                }
                Ok::<_, PipelineError>((page, outcome))
            });
        }
        let mut collected = Vec::with_capacity(unique.len());
        while let Some(joined) = set.join_next().await {
            collected.push(joined.map_err(|_| PdfError::Layout)??);
        }
        collected.sort_by_key(|(page, _)| *page);
        for (page, outcome) in collected {
            match outcome {
                Ok(artifact) => {
                    if let Some(evidence) = &artifact.vector_evidence {
                        vector_primitives += u64::from(evidence.primitive_count);
                        vector_components += evidence.components.len() as u64;
                        vector_eligible += evidence
                            .components
                            .iter()
                            .filter(|component| component.eligible)
                            .count() as u64;
                        vector_backgrounds += u64::from(evidence.excluded_backgrounds);
                        vector_text_glyphs += u64::from(evidence.excluded_text_glyphs);
                        vector_templates_excluded += u64::from(evidence.excluded_templates);
                    }
                    for action in &artifact.actions.actions {
                        let name = serde_json::to_value(action.kind)?
                            .as_str()
                            .expect("action enum serializes to string")
                            .to_owned();
                        *action_counts.entry(name).or_default() += 1;
                    }
                    for f in &artifact.failures {
                        *failure_counts.entry(f.code.clone()).or_default() += 1;
                    }
                    crop_count += artifact.crops.len() as u64;
                    summaries.push(serde_json::json!({"page":page,"route":artifact.route,"crops":artifact.crops.len(),"failures":artifact.failures.len()}));
                }
                Err(f) => {
                    *failure_counts.entry(f.code).or_default() += 1;
                    summaries.push(serde_json::json!({"page":page,"status":"failed"}));
                }
            }
        }
        let status = if summaries.iter().all(|p| p["status"] == "failed") {
            "FAILED"
        } else {
            "PARTIAL"
        };
        let summary = serde_json::json!({"schema_version":1,"pipeline_version":PIPELINE_VERSION,"run_id":run_id,
            "status":status,"source_pages":info.page_count,"sampled_pages":unique,"pages":summaries,
            "crops":crop_count,"source_characters":digital_chars,"math_font_blocks":math_fonts,
            "geometry_warnings":geometry_warnings,"action_counts":action_counts,"failure_counts":failure_counts,
            "vector_primitives":vector_primitives,"vector_components":vector_components,
            "vector_eligible_components":vector_eligible,"vector_excluded_backgrounds":vector_backgrounds,
            "vector_excluded_text_glyphs":vector_text_glyphs,
            "vector_excluded_templates":vector_templates_excluded,"vector_template_status":template_status,
            "leased_model_inference_performed":false,
            "tesseract_geometry_attempted":action_counts.contains_key("ACTION_OCR_FULL_PAGE_SCAN"),
            "annotated_corpus_acceptance":false});
        let mut evidence = summary.clone();
        evidence["input_sha256"] = serde_json::json!(input_sha256);
        write_json_atomic(&run_dir.join("region-probe.json"), &evidence).await?;
        Ok(summary)
    }

    pub async fn run(
        &self,
        source: &Path,
        run_id: Uuid,
        events: Option<mpsc::Sender<PipelineEvent>>,
    ) -> Result<RunManifest, PipelineError> {
        let started_at = Utc::now();
        let started = Instant::now();
        let info: PdfInfo = self.config.pdf.validate(source).await?;
        let (input_sha256, input_bytes) = sha256_file(source).await?;
        let concurrency = self.config.max_concurrency.clamp(1, 16);
        // A failed/overflowed helper must not silently become "zero images".
        let (image_counts, layouts) = tokio::try_join!(
            async {
                self.config
                    .pdf
                    .image_counts(source)
                    .await
                    .map_err(PipelineError::from)
            },
            extract_layouts(&self.config.pdf, source, info.page_count, concurrency),
        )?;
        let templates = TemplateImages::detect(&layouts);
        let run_dir = self.config.work_dir.join(run_id.to_string());
        tokio::fs::create_dir_all(run_dir.join("pages")).await?;
        tokio::fs::create_dir_all(run_dir.join("crops")).await?;
        set_private(&run_dir, 0o700).await;
        set_private(&run_dir.join("pages"), 0o700).await;
        set_private(&run_dir.join("crops"), 0o700).await;
        let (vector_counter, vector_paths) =
            prepare_vector_deck(source, &layouts, &run_dir, concurrency).await;
        let vector_templates = vector_counter.finish();
        let context = Arc::new(DeckContext {
            source: source.to_path_buf(),
            run_id,
            run_dir: run_dir.clone(),
            input_sha256: input_sha256.clone(),
            layouts,
            templates,
            vector_templates,
            vector_paths,
        });
        let runner = Arc::new(Self::new(self.config.clone()));
        let image_counts = Arc::new(image_counts);
        let semaphore = Arc::new(Semaphore::new(concurrency));
        let mut set = JoinSet::new();
        for page in 1..=info.page_count {
            let permit = semaphore.clone();
            let runner = Arc::clone(&runner);
            let context = Arc::clone(&context);
            let image_counts = Arc::clone(&image_counts);
            let events = events.clone();
            let total_pages = info.page_count;
            set.spawn(async move {
                let _guard = permit.acquire().await.map_err(|_| PdfError::Layout)?;
                send_event(
                    &events,
                    PipelineEvent::PageStarted {
                        page,
                        total: total_pages,
                    },
                );
                let page_path = context
                    .run_dir
                    .join("pages")
                    .join(format!("page-{page:04}.json"));
                if let Some(artifact) = runner
                    .load_resumable(&page_path, &context.run_dir, page, &context.input_sha256)
                    .await
                {
                    send_event(
                        &events,
                        PipelineEvent::PageFinished {
                            page,
                            total: total_pages,
                            route: Some(artifact.route),
                            failures: 0,
                        },
                    );
                    return Ok::<_, PipelineError>((page, Ok(artifact)));
                }
                let layout = &context.layouts[(page - 1) as usize];
                let outcome = runner
                    .process_page(
                        &context.source,
                        context.run_id,
                        &context.run_dir,
                        layout,
                        &context.templates,
                        &context.vector_templates,
                        context.vector_paths[(page - 1) as usize].as_deref(),
                        page,
                        &context.input_sha256,
                        image_counts.get(&page).copied().unwrap_or(0),
                    )
                    .await;
                if let Ok(artifact) = &outcome {
                    write_json_atomic(&page_path, artifact).await?;
                    send_event(
                        &events,
                        PipelineEvent::PageFinished {
                            page,
                            total: total_pages,
                            route: Some(artifact.route),
                            failures: artifact.failures.len() as u32,
                        },
                    );
                } else {
                    send_event(
                        &events,
                        PipelineEvent::PageFinished {
                            page,
                            total: total_pages,
                            route: None,
                            failures: 1,
                        },
                    );
                }
                Ok((page, outcome))
            });
        }

        let mut collected = Vec::with_capacity(info.page_count as usize);
        while let Some(joined) = set.join_next().await {
            collected.push(joined.map_err(|_| PdfError::Layout)??);
        }
        collected.sort_by_key(|(page, _)| *page);
        let mut artifacts: Vec<PageArtifact> = Vec::new();
        let mut failures: Vec<FailureArtifact> = Vec::new();
        for (_, outcome) in collected {
            match outcome {
                Ok(artifact) => {
                    failures.extend(artifact.failures.iter().cloned());
                    artifacts.push(artifact);
                }
                Err(page_failure) => {
                    failures.push(page_failure);
                }
            }
        }

        let (chunks, mut totals) = deduplicate(&mut artifacts);
        let mut pages = Vec::with_capacity(info.page_count as usize);
        for page in 1..=info.page_count {
            if let Some(artifact) = artifacts
                .iter()
                .find(|artifact| artifact.page_number == page)
            {
                let duplicate_count = artifact
                    .chunks
                    .iter()
                    .filter(|chunk| chunk.duplicate_of.is_some())
                    .count() as u32;
                pages.push(PageSummary {
                    page_number: page,
                    status: if artifact.failures.is_empty() {
                        "succeeded"
                    } else {
                        "partial"
                    }
                    .into(),
                    route: Some(artifact.route),
                    crop_count: artifact.crops.len() as u32,
                    chunk_count: artifact.chunks.len() as u32,
                    duplicate_count,
                    failure_count: artifact.failures.len() as u32,
                    duration_ms: artifact.duration_ms,
                });
                match artifact.route {
                    PageRoute::Digital => totals.digital_pages += 1,
                    PageRoute::Ocr => totals.ocr_pages += 1,
                    PageRoute::Hybrid => totals.hybrid_pages += 1,
                    PageRoute::Blank => totals.blank_pages += 1,
                }
            } else {
                let page_failure_count = failures
                    .iter()
                    .filter(|failure| failure.page_number == page)
                    .count() as u32;
                pages.push(PageSummary {
                    page_number: page,
                    status: "failed".into(),
                    route: None,
                    crop_count: 0,
                    chunk_count: 0,
                    duplicate_count: 0,
                    failure_count: page_failure_count,
                    duration_ms: 0,
                });
            }
        }
        totals.pages = info.page_count;
        totals.pages_succeeded = artifacts.len() as u32;
        totals.pages_failed = info.page_count - artifacts.len() as u32;
        totals.failures = failures.len() as u32;
        let status = if totals.pages_failed == 0 && failures.is_empty() {
            RunStatus::Succeeded
        } else if totals.pages_succeeded == 0 {
            RunStatus::Failed
        } else {
            RunStatus::Partial
        };
        let model = artifacts.iter().find_map(|artifact| {
            artifact
                .ocr
                .as_ref()
                .or_else(|| artifact.region_ocr.first().map(|r| &r.result))
                .map(|ocr| ocr.model.clone())
        });
        let model_peak_rss_bytes = artifacts
            .iter()
            .filter_map(|artifact| {
                artifact
                    .ocr
                    .as_ref()
                    .into_iter()
                    .chain(artifact.region_ocr.iter().map(|r| &r.result))
                    .filter_map(|ocr| ocr.model_peak_rss_bytes)
                    .max()
            })
            .max();
        let manifest = RunManifest {
            run_id,
            pipeline_version: PIPELINE_VERSION.into(),
            input_sha256,
            input_bytes,
            source_pages: info.page_count,
            started_at,
            finished_at: Utc::now(),
            duration_ms: started.elapsed().as_millis() as u64,
            status,
            render_dpi: self.config.render_dpi,
            pages,
            totals,
            chunks,
            failures,
            model,
            model_peak_rss_bytes,
        };
        write_json_atomic(&run_dir.join("manifest.json"), &manifest).await?;
        send_event(&events, PipelineEvent::Finished { status });
        Ok(manifest)
    }

    async fn load_resumable(
        &self,
        page_path: &Path,
        run_dir: &Path,
        page: u32,
        input_sha256: &str,
    ) -> Option<PageArtifact> {
        let bytes = tokio::fs::read(page_path).await.ok()?;
        let artifact: PageArtifact = serde_json::from_slice(&bytes).ok()?;
        if artifact.page_number != page
            || artifact.input_sha256 != input_sha256
            || artifact.pipeline_version != PIPELINE_VERSION
            || !artifact.failures.is_empty()
        {
            return None;
        }
        if artifact
            .crops
            .iter()
            .any(|crop| !run_dir.join(&crop.relative_path).is_file())
        {
            return None;
        }
        Some(artifact)
    }

    #[allow(clippy::too_many_arguments)]
    async fn process_page(
        &self,
        source: &Path,
        run_id: Uuid,
        run_dir: &Path,
        layout: &PageLayout,
        templates: &TemplateImages,
        vector_templates: &crate::vectors::VectorTemplates,
        vector_path: Option<&Path>,
        page: u32,
        input_sha256: &str,
        embedded_images: u32,
    ) -> Result<PageArtifact, FailureArtifact> {
        let page_started = Instant::now();
        let mut features = classify_page(layout, embedded_images);
        let mut failures = Vec::new();
        let mut ocr: Option<OcrResult> = None;
        let mut crops: Vec<CropOutput> = Vec::new();
        let dpi = ANALYSIS_DPI;
        if self.config.render_dpi != dpi {
            return Err(failure(
                page,
                "render",
                "analysis_dpi_contract",
                "region analysis requires 150 DPI",
                false,
            ));
        }
        let render_prefix = run_dir.join("pages").join(format!("page-{page:04}"));
        let png = self
            .config
            .pdf
            .render_png(source, page, dpi, &render_prefix)
            .await
            .map_err(|e| failure(page, "render", "render_failed", e.to_string(), true))?;
        set_private(&png, 0o600).await;
        let raster = Raster::load(&png).map_err(|e| {
            failure(
                page,
                "analysis",
                "invalid_analysis_raster",
                e.to_string(),
                false,
            )
        })?;
        let size = (layout.width_pt, layout.height_pt);
        // The vector mask is derived from sanitized Poppler geometry only. Text
        // glyphs never enter it; labels stay in the digital-text stream.
        let mut vector_analysis = None;
        let mut vector_mask_path = None;
        if let Some(path) = vector_path {
            match crate::vectors::load_svg(path, layout).await {
                Ok(vector_page) => {
                    let mask_file = run_dir
                        .join("pages")
                        .join(format!("page-{page:04}-vector-mask.png"));
                    match crate::vectors::analyze(
                        &vector_page,
                        layout,
                        vector_templates,
                        &raster,
                        &mask_file,
                    )
                    .await
                    {
                        Ok(analysis) => {
                            set_private(&mask_file, 0o600).await;
                            vector_mask_path =
                                Some(format!("pages/page-{page:04}-vector-mask.png"));
                            vector_analysis = Some(analysis);
                        }
                        Err(e) => failures.push(failure(
                            page,
                            "vector",
                            "vector_analysis_failed",
                            e.to_string(),
                            false,
                        )),
                    }
                }
                Err(e) => failures.push(failure(
                    page,
                    "vector",
                    "vector_svg_load_failed",
                    e.to_string(),
                    false,
                )),
            }
        }
        let vector_evidence = vector_analysis.as_ref().map(|analysis| &analysis.evidence);
        let mut actions = plan_page_with_vectors(layout, &raster, templates, vector_evidence)
            .map_err(|e| failure(page, "analysis", "region_plan_failed", e.to_string(), false))?;
        // Consolidation replaces the raw single-primitive actions with barrier-
        // aware composites that still satisfy the frozen predicate for their
        // type. A failed consolidation keeps the raw actions instead of losing
        // evidence.
        if let Some(analysis) = &vector_analysis {
            let raster_boxes = actions
                .actions
                .iter()
                .filter(|action| action.kind == ActionKind::CropRasterFigures)
                .map(|action| action.bbox)
                .collect::<Vec<_>>();
            match consolidate(
                layout,
                &analysis.evidence,
                &analysis.mask,
                &raster,
                &raster_boxes,
            ) {
                Ok(composites) => {
                    actions.actions.retain(|action| {
                        !matches!(
                            action.kind,
                            ActionKind::CropVectorClusters | ActionKind::CropRasterFigures
                        )
                    });
                    for composite in composites {
                        let kind = if composite.has_vector {
                            ActionKind::CropVectorClusters
                        } else {
                            ActionKind::CropRasterFigures
                        };
                        actions.actions.push(RegionAction {
                            kind,
                            bbox: composite.bbox,
                            source_text: composite.source_text,
                            ocr_required: kind == ActionKind::CropRasterFigures
                                && layout.text.is_empty(),
                            reason: if composite.has_vector {
                                "consolidated_vector_composite"
                            } else {
                                "consolidated_raster_composite"
                            }
                            .into(),
                        });
                    }
                }
                Err(e) => failures.push(failure(
                    page,
                    "analysis",
                    "region_consolidation_failed",
                    e.to_string(),
                    false,
                )),
            }
        }
        merge_math_blocks(&mut actions.actions);

        let page_box = crate::pdf::BBox {
            x0: 0.0,
            y0: 0.0,
            x1: size.0,
            y1: size.1,
        };
        let non_uniform = raster.variance(page_box, size, dpi).map_err(|e| {
            failure(
                page,
                "analysis",
                "invalid_analysis_raster",
                e.to_string(),
                false,
            )
        })? > UNIFORM_VARIANCE;
        let route = actions.route(non_uniform);
        features.route = route;
        for code in &actions.incomplete {
            failures.push(failure(
                page,
                "analysis",
                code,
                "region analysis is incomplete; not G-1 acceptance",
                false,
            ));
        }
        let mut boxes = actions
            .actions
            .iter()
            .map(|action| {
                (
                    action.bbox,
                    match action.kind {
                        ActionKind::ExtractDigitalText => [0, 180, 0],
                        ActionKind::OcrMathOrGarbledRegions => [255, 0, 0],
                        ActionKind::CropRasterFigures => [0, 80, 255],
                        ActionKind::CropVectorClusters => [128, 0, 255],
                        ActionKind::OcrFullPageScan => [220, 180, 0],
                    },
                )
            })
            .collect::<Vec<_>>();
        let mut text = layout.text.clone();
        let mut region_ocr = Vec::new();
        let mut selected: Vec<(Region, bool)> = Vec::new();
        for action in &actions.actions {
            if action.kind == ActionKind::OcrFullPageScan {
                // Full-page OCR precedes sub-region isolation; it is never
                // emitted as a giant figure crop when geometry is unavailable.
                // Run GPU OCR and CPU Tesseract box geometry concurrently.
                let ocr_fut = async {
                    match &self.config.ocr {
                        Some(client) => Some(client.recognize_png(&png).await),
                        None => None,
                    }
                };
                let (ocr_outcome, tess_outcome) = tokio::join!(
                    ocr_fut,
                    tesseract_regions(&self.config.crops.tesseract, &png, dpi, size.0, size.1),
                );
                match ocr_outcome {
                    Some(Ok(result)) => {
                        text = merge_hybrid_text(&text, &result.text);
                        ocr = Some(result);
                    }
                    Some(Err(e)) => {
                        failures.push(failure(page, "ocr", "ocr_failed", ocr_message(&e), true))
                    }
                    None => failures.push(failure(
                        page,
                        "ocr",
                        "ocr_model_not_configured",
                        "full-page scan needs local OCR",
                        false,
                    )),
                }
                match tess_outcome {
                    Ok(regions) => {
                        // OCR runs on the page raster; only contrast-isolated,
                        // non-text sub-regions become crops. Tesseract boxes are
                        // geometry evidence, not crop regions by themselves.
                        let text_boxes =
                            regions.iter().map(|region| region.bbox).collect::<Vec<_>>();
                        match scan_regions(&raster, layout, &text_boxes) {
                            Ok(sub_regions) => {
                                for bbox in sub_regions {
                                    boxes.push((bbox, [220, 180, 0]));
                                    selected.push((
                                        Region {
                                            index: 0,
                                            kind: RegionKind::Image,
                                            bbox,
                                            text: None,
                                        },
                                        false,
                                    ));
                                }
                            }
                            Err(e) => failures.push(failure(
                                page,
                                "analysis",
                                "scan_region_isolation_failed",
                                e.to_string(),
                                false,
                            )),
                        }
                    }
                    Err(e) => failures.push(failure(
                        page,
                        "geometry",
                        "scan_layout_failed",
                        crop_message(&e),
                        true,
                    )),
                }
                continue;
            }
            if action.kind == ActionKind::ExtractDigitalText {
                // Text is already extracted from pdftohtml. Cropping every text
                // line inflated 123 crops and added no evidence.
                continue;
            }
            selected.push((
                Region {
                    index: 0,
                    kind: match action.kind {
                        ActionKind::CropVectorClusters => RegionKind::Vector,
                        ActionKind::CropRasterFigures => RegionKind::Image,
                        ActionKind::OcrMathOrGarbledRegions => RegionKind::Math,
                        _ => RegionKind::Text,
                    },
                    bbox: action.bbox,
                    text: action.source_text.clone(),
                },
                action.ocr_required,
            ));
        }
        let overlay_path = format!("pages/page-{page:04}-overlay.png");
        raster
            .overlay(&run_dir.join(&overlay_path), &boxes, size, dpi)
            .map_err(|e| failure(page, "overlay", "overlay_failed", e.to_string(), false))?;
        if selected.len() > crate::crops::MAX_CROPS_PER_PAGE {
            return Err(failure(
                page,
                "crop",
                "region_count_overflow",
                "region count exceeds the bounded crop budget; no silent truncation",
                false,
            ));
        }
        let mut selected_regions = Vec::with_capacity(selected.len());
        let mut region_set = JoinSet::new();
        for (index, (mut region, needs_ocr)) in selected.into_iter().enumerate() {
            region.index = index as u32;
            selected_regions.push(region.clone());
            let crop_tools = self.config.crops.clone();
            let source = source.to_path_buf();
            let layout = layout.clone();
            let run_dir = run_dir.to_path_buf();
            let input_sha256 = input_sha256.to_owned();
            let ocr_client = self.config.ocr.clone();
            region_set.spawn(async move {
                let cropped = crop_pdf_region(
                    &crop_tools,
                    &source,
                    &layout,
                    &run_dir,
                    run_id,
                    &input_sha256,
                    &region,
                )
                .await;
                let ocr_result = match (&cropped, needs_ocr) {
                    (Ok((_, region_png)), true) => match &ocr_client {
                        Some(client) => Some(Ok(client.recognize_png(region_png).await)),
                        None => Some(Err(())),
                    },
                    _ => None,
                };
                (index, region.bbox, cropped, ocr_result)
            });
        }
        let mut region_outcomes = Vec::new();
        while let Some(joined) = region_set.join_next().await {
            if let Ok(item) = joined {
                region_outcomes.push(item);
            }
        }
        region_outcomes.sort_by_key(|(index, ..)| *index);
        for (index, bbox, cropped, ocr_result) in region_outcomes {
            match cropped {
                Ok((crop, _)) => {
                    crops.push(crop);
                    match ocr_result {
                        Some(Ok(Ok(result))) => {
                            text = merge_hybrid_text(&text, &result.text);
                            region_ocr.push(RegionOcrArtifact { bbox, result });
                        }
                        Some(Ok(Err(e))) => failures.push(failure(
                            page,
                            "ocr",
                            "region_ocr_failed",
                            ocr_message(&e),
                            true,
                        )),
                        Some(Err(())) => failures.push(failure(
                            page,
                            "ocr",
                            "ocr_model_not_configured",
                            "math/image region needs local OCR; source crop preserved",
                            false,
                        )),
                        None => {}
                    }
                }
                Err(e) => failures.push(failure(
                    page,
                    "crop",
                    "crop_failed",
                    format!("region {index}: {}", crop_message(&e)),
                    !matches!(
                        e,
                        CropError::Unsafe | CropError::TooSmall | CropError::TooLarge
                    ),
                )),
            }
        }

        let mut chunks = Vec::new();
        for (order, chunk) in chunk_text(&text, self.config.max_chunk_chars)
            .into_iter()
            .enumerate()
        {
            let fingerprint = fingerprint(&chunk);
            if fingerprint.word_count == 0 {
                continue;
            }
            chunks.push(ChunkArtifact {
                id: chunk_id(run_id, page, order as u32, &fingerprint.text_sha256),
                page_number: page,
                order: order as u32,
                route,
                text: chunk.clone(),
                text_sha256: fingerprint.text_sha256,
                critical_sha256: fingerprint.critical_sha256,
                critical_tokens: fingerprint.critical_tokens,
                word_count: fingerprint.word_count,
                crop_ids: associated_crops(&chunk, &crops),
                duplicate_of: None,
                dedup: None,
            });
        }
        if chunks.is_empty() && route != PageRoute::Blank {
            failures.push(failure(
                page,
                "chunk",
                "no_chunks",
                "the page produced no usable text chunks",
                true,
            ));
        }
        Ok(PageArtifact {
            page_number: page,
            input_sha256: input_sha256.into(),
            pipeline_version: PIPELINE_VERSION.into(),
            route,
            width_pt: layout.width_pt,
            height_pt: layout.height_pt,
            render_dpi: dpi,
            features,
            text,
            source_text: layout.text.clone(),
            geometry: layout.geometry.clone(),
            actions,
            vector_evidence: vector_analysis.map(|analysis| analysis.evidence),
            vector_mask_path,
            overlay_path,
            region_ocr,
            selected_regions,
            ocr,
            crops,
            chunks,
            failures,
            duration_ms: page_started.elapsed().as_millis() as u64,
        })
    }
}

/// Adjacent math/garbled lines belong to one displayed equation. Merging them
/// keeps OCR blocks readable without inventing vector evidence or crossing a
/// clean digital paragraph.
fn merge_math_blocks(actions: &mut Vec<RegionAction>) {
    let prose = actions
        .iter()
        .filter(|action| {
            action.kind == ActionKind::ExtractDigitalText
                && action
                    .source_text
                    .as_deref()
                    .is_some_and(|text| text.chars().filter(|c| !c.is_whitespace()).count() > 35)
        })
        .map(|action| action.bbox)
        .collect::<Vec<_>>();
    let mut math = Vec::new();
    let mut rest = Vec::new();
    for action in actions.drain(..) {
        if action.kind == ActionKind::OcrMathOrGarbledRegions {
            math.push(action);
        } else {
            rest.push(action);
        }
    }
    math.sort_by(|a, b| {
        a.bbox
            .y0
            .total_cmp(&b.bbox.y0)
            .then(a.bbox.x0.total_cmp(&b.bbox.x0))
    });
    let mut merged: Vec<RegionAction> = Vec::new();
    for action in math {
        if let Some(last) = merged.last_mut() {
            let vertical_gap = (action.bbox.y0 - last.bbox.y1)
                .max(last.bbox.y0 - action.bbox.y1)
                .max(0.0);
            let overlap_width =
                (action.bbox.x1.min(last.bbox.x1) - action.bbox.x0.max(last.bbox.x0)).max(0.0);
            let narrower = action.bbox.width().min(last.bbox.width()).max(1.0);
            let candidate = crate::pdf::BBox {
                x0: last.bbox.x0.min(action.bbox.x0),
                y0: last.bbox.y0.min(action.bbox.y0),
                x1: last.bbox.x1.max(action.bbox.x1),
                y1: last.bbox.y1.max(action.bbox.y1),
            };
            let crosses_prose = prose.iter().any(|block| {
                candidate
                    .intersection(*block)
                    .is_some_and(|r| f64::from(r.area()) > 0.10 * f64::from(block.area()))
            });
            if vertical_gap <= 4.0 && overlap_width >= 0.5 * narrower && !crosses_prose {
                last.bbox = candidate;
                last.source_text = match (last.source_text.take(), action.source_text) {
                    (Some(a), Some(b)) => Some(format!("{a}\n{b}")),
                    (Some(a), None) => Some(a),
                    (None, b) => b,
                };
                last.reason = "merged_math_block".into();
                continue;
            }
        }
        merged.push(action);
    }
    rest.extend(merged);
    *actions = rest;
}

fn ocr_message(error: &OcrError) -> String {
    error.to_string()
}

fn crop_message(error: &CropError) -> String {
    error.to_string()
}

struct CanonicalChunk {
    id: Uuid,
    fingerprint: ChunkFingerprint,
}

fn deduplicate(artifacts: &mut [PageArtifact]) -> (Vec<ChunkArtifact>, RunTotals) {
    let mut canonical: Vec<CanonicalChunk> = Vec::new();
    let mut by_text: HashMap<String, usize> = HashMap::new();
    let mut by_critical: HashMap<String, Vec<usize>> = HashMap::new();
    let mut totals = RunTotals::default();
    let mut output = Vec::new();
    for artifact in artifacts.iter_mut() {
        totals.crops += artifact.crops.len() as u32;
        for chunk in artifact.chunks.iter_mut() {
            totals.chunks += 1;
            let candidate = fingerprint(&chunk.text);
            let mut match_index = None;
            let mut decision = DedupDecision::Distinct;
            if let Some(&index) = by_text.get(&candidate.text_sha256) {
                let existing = &canonical[index];
                if let DedupDecision::Duplicate { .. } = compare(&existing.fingerprint, &candidate)
                {
                    match_index = Some(index);
                    decision = compare(&existing.fingerprint, &candidate);
                }
            }
            if match_index.is_none()
                && let Some(indices) = by_critical.get(&candidate.critical_sha256)
            {
                for &index in indices {
                    if let DedupDecision::Duplicate { .. } =
                        compare(&canonical[index].fingerprint, &candidate)
                    {
                        match_index = Some(index);
                        decision = compare(&canonical[index].fingerprint, &candidate);
                        break;
                    }
                }
            }
            if let Some(index) = match_index {
                chunk.duplicate_of = Some(canonical[index].id);
                chunk.dedup = Some(decision);
                totals.duplicate_chunks += 1;
            } else {
                let index = canonical.len();
                by_text.insert(candidate.text_sha256.clone(), index);
                by_critical
                    .entry(candidate.critical_sha256.clone())
                    .or_default()
                    .push(index);
                canonical.push(CanonicalChunk {
                    id: chunk.id,
                    fingerprint: candidate,
                });
                totals.unique_chunks += 1;
            }
            output.push(chunk.clone());
        }
    }
    (output, totals)
}

/// Read a manifest from a run directory. Used by the CLI and acceptance tools.
pub async fn read_manifest(work_dir: &Path, run_id: Uuid) -> Result<RunManifest, PipelineError> {
    let bytes = tokio::fs::read(work_dir.join(run_id.to_string()).join("manifest.json")).await?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// Stream the OCR model peak and host headroom seen by a run. This is a
/// convenience for evidence generation; it never starts a model.
pub fn run_directory(work_dir: &Path, run_id: Uuid) -> PathBuf {
    work_dir.join(run_id.to_string())
}

/// Keep a stable default lease path so the OCR worker and M2 CLI agree.
pub fn default_lease_path(work_dir: &Path) -> PathBuf {
    work_dir.join("models.lock")
}

/// A short human-readable reason for a failed page, without leaking paths.
pub fn failure_summary(manifest: &RunManifest) -> String {
    match manifest.status {
        RunStatus::Succeeded => "extraction completed".into(),
        RunStatus::Partial => format!(
            "{} of {} pages failed; inspect failures",
            manifest.totals.pages_failed, manifest.totals.pages
        ),
        RunStatus::Failed => "extraction failed for every page".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::BBox;

    fn crop(text: &str) -> CropOutput {
        CropOutput {
            id: Uuid::from_u128(text.len() as u128 + 1),
            region_index: 0,
            kind: crate::crops::RegionKind::Text,
            bbox: BBox {
                x0: 0.0,
                y0: 0.0,
                x1: 10.0,
                y1: 10.0,
            },
            pixel_bbox: [0, 0, 10, 10],
            width: 10,
            height: 10,
            dpi: 200,
            sha256: String::new(),
            bytes: 0,
            relative_path: "crops/x.webp".into(),
            text: Some(text.into()),
        }
    }

    #[test]
    fn hybrid_text_adds_only_new_normalized_lines() {
        let merged = merge_hybrid_text(
            "Photosynthesis converts light energy.\nYield is 42%.",
            "Yield is 42%.\nFormula: E = mc^2\n",
        );
        assert_eq!(merged.lines().count(), 3);
        assert!(merged.contains("Formula: E = mc^2"));
        assert!(
            merge_hybrid_text("if x:\n    return x", "new OCR line").contains("\n    return x")
        );
    }

    #[test]
    fn crop_association_uses_normalized_containment() {
        let crops = vec![crop("Yield is 42%."), crop("Formula: E = mc^2")];
        let ids = associated_crops("The yield is 42%.\nFormula: E=mc^2", &crops);
        assert_eq!(ids.len(), 2);
        assert!(associated_crops("Unrelated sentence.", &crops).is_empty());
    }

    #[test]
    fn adjacent_math_lines_merge_without_crossing_digital_prose() {
        let math = |y0: f32, y1: f32| RegionAction {
            kind: ActionKind::OcrMathOrGarbledRegions,
            bbox: BBox {
                x0: 10.0,
                y0,
                x1: 40.0,
                y1,
            },
            source_text: Some("x".into()),
            ocr_required: true,
            reason: "math_font".into(),
        };
        let mut actions = vec![math(10.0, 20.0), math(22.0, 32.0)];
        merge_math_blocks(&mut actions);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].bbox.y0, 10.0);
        assert_eq!(actions[0].bbox.y1, 32.0);

        let mut blocked = vec![
            math(10.0, 20.0),
            RegionAction {
                kind: ActionKind::ExtractDigitalText,
                bbox: BBox {
                    x0: 10.0,
                    y0: 20.5,
                    x1: 40.0,
                    y1: 21.5,
                },
                source_text: Some("clean digital prose that stays a barrier here".into()),
                ocr_required: false,
                reason: "clean_digital_text".into(),
            },
            math(22.0, 32.0),
        ];
        merge_math_blocks(&mut blocked);
        assert_eq!(
            blocked
                .iter()
                .filter(|action| action.kind == ActionKind::OcrMathOrGarbledRegions)
                .count(),
            2
        );
    }

    #[tokio::test]
    #[ignore = "requires Poppler/Tesseract/ImageMagick and delegated helper cgroup"]
    async fn region_pipeline_preserves_source_and_records_vector_evidence() {
        let directory = tempfile::tempdir().unwrap();
        let pipeline = ExtractionPipeline::new(PipelineConfig::new(directory.path()));
        let source = crate::project_root().join("tests/fixtures/m2/region-cases.pdf");
        let run = Uuid::new_v4();
        let manifest = pipeline.run(&source, run, None).await.unwrap();
        assert_eq!(manifest.status, RunStatus::Partial);
        // Vector analysis now runs: the old "pending" placeholder must be gone.
        assert!(
            !manifest
                .failures
                .iter()
                .any(|f| f.code == "vector_mask_and_clusters_pending")
        );
        let mut excluded_backgrounds = 0;
        for page in 1..=6 {
            let bytes = tokio::fs::read(
                directory
                    .path()
                    .join(run.to_string())
                    .join(format!("pages/page-{page:04}.json")),
            )
            .await
            .unwrap();
            let artifact: PageArtifact = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(artifact.render_dpi, 150);
            let evidence = artifact
                .vector_evidence
                .as_ref()
                .expect("full-deck vector evidence is persisted");
            assert_eq!(
                evidence.template_status,
                crate::vectors::TemplateStatus::Complete
            );
            assert!(evidence.incomplete.is_empty());
            assert!(artifact.actions.incomplete.is_empty());
            let mask = artifact
                .vector_mask_path
                .as_ref()
                .expect("vector mask path is persisted");
            assert!(directory.path().join(run.to_string()).join(mask).exists());
            excluded_backgrounds += evidence.excluded_backgrounds;
            assert!(
                directory
                    .path()
                    .join(run.to_string())
                    .join(&artifact.overlay_path)
                    .exists()
            );
            assert!(artifact.crops.iter().all(|c| c.dpi == 250));
            // Text is extracted, never re-cropped as a presentation image.
            assert!(
                artifact
                    .crops
                    .iter()
                    .all(|c| c.kind != crate::crops::RegionKind::Text)
            );
            if page < 5 {
                assert!(artifact.source_text.contains("    return x"));
                assert!(
                    artifact
                        .actions
                        .actions
                        .iter()
                        .any(|a| a.kind == ActionKind::OcrMathOrGarbledRegions)
                );
                assert!(
                    artifact
                        .actions
                        .actions
                        .iter()
                        .any(|a| a.kind == ActionKind::CropRasterFigures)
                );
            } else if page == 5 {
                assert!(
                    artifact
                        .actions
                        .actions
                        .iter()
                        .any(|a| a.kind == ActionKind::OcrFullPageScan)
                );
                assert!(
                    artifact
                        .crops
                        .iter()
                        .all(|c| c.bbox.area() / (artifact.width_pt * artifact.height_pt) < 0.85)
                );
            } else {
                assert_eq!(artifact.route, PageRoute::Digital);
            }
        }
        assert!(excluded_backgrounds >= 1);
    }
}
