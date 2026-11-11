//! Resumable region-aware extraction orchestration.
//!
//! A run is a private directory with one JSON artifact per completed page and a
//! final manifest. Pages that failed are not cached, so a retry of the same run
//! ID reprocesses only the missing or failed pages. Deduplication is applied
//! across all pages after extraction, including across digital and OCR routes.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Instant,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::mpsc,
};
use uuid::Uuid;

use crate::{
    crops::{CropError, CropOutput, CropTools, crop_region, select_regions},
    fingerprint::{
        ChunkFingerprint, CriticalToken, DedupDecision, chunk_text, compare, fingerprint,
    },
    ocr::{OcrClient, OcrError, OcrResult},
    pdf::{MAX_RENDER_PIXELS, PageFeatures, PageRoute, PdfError, PdfInfo, PdfTools, classify_page},
};

pub const PIPELINE_VERSION: &str = "m2-pipeline-v1";
pub const DEFAULT_RENDER_DPI: u32 = 200;
pub const DEFAULT_MAX_CHUNK_CHARS: usize = 1_200;

#[derive(Debug, Error)]
pub enum PipelineError {
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
    pub ocr: Option<OcrResult>,
    pub crops: Vec<CropOutput>,
    pub chunks: Vec<ChunkArtifact>,
    pub failures: Vec<FailureArtifact>,
    pub duration_ms: u64,
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

/// Fit the requested DPI inside the global pixel cap without upscaling.
fn safe_render_dpi(width_pt: f32, height_pt: f32, requested: u32) -> u32 {
    let requested = requested.clamp(72, 300);
    let pixels_at_requested =
        (width_pt * requested as f32 / 72.0) * (height_pt * requested as f32 / 72.0);
    if pixels_at_requested <= MAX_RENDER_PIXELS as f32 {
        return requested;
    }
    let scale = (MAX_RENDER_PIXELS as f32 / pixels_at_requested).sqrt();
    ((requested as f32 * scale).floor() as u32).clamp(72, 300)
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
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    let mut seen: Vec<String> = lines
        .iter()
        .map(|line| crate::fingerprint::normalize_text(line))
        .collect();
    for line in ocr_text.lines().map(str::trim) {
        if line.is_empty() {
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
        let image_counts = self
            .config
            .pdf
            .image_counts(source)
            .await
            .unwrap_or_default();
        let run_dir = self.config.work_dir.join(run_id.to_string());
        tokio::fs::create_dir_all(run_dir.join("pages")).await?;
        tokio::fs::create_dir_all(run_dir.join("crops")).await?;
        set_private(&run_dir, 0o700).await;
        set_private(&run_dir.join("pages"), 0o700).await;
        set_private(&run_dir.join("crops"), 0o700).await;

        let mut artifacts: Vec<PageArtifact> = Vec::new();
        let mut failures: Vec<FailureArtifact> = Vec::new();
        for page in 1..=info.page_count {
            send_event(
                &events,
                PipelineEvent::PageStarted {
                    page,
                    total: info.page_count,
                },
            );
            let page_path = run_dir.join("pages").join(format!("page-{page:04}.json"));
            if let Some(artifact) = self
                .load_resumable(&page_path, &run_dir, page, &input_sha256)
                .await
            {
                send_event(
                    &events,
                    PipelineEvent::PageFinished {
                        page,
                        total: info.page_count,
                        route: Some(artifact.route),
                        failures: 0,
                    },
                );
                artifacts.push(artifact);
                continue;
            }
            match self
                .process_page(
                    source,
                    run_id,
                    &run_dir,
                    &info,
                    page,
                    &input_sha256,
                    image_counts.get(&page).copied().unwrap_or(0),
                )
                .await
            {
                Ok(artifact) => {
                    write_json_atomic(&page_path, &artifact).await?;
                    send_event(
                        &events,
                        PipelineEvent::PageFinished {
                            page,
                            total: info.page_count,
                            route: Some(artifact.route),
                            failures: artifact.failures.len() as u32,
                        },
                    );
                    failures.extend(artifact.failures.iter().cloned());
                    artifacts.push(artifact);
                }
                Err(page_failure) => {
                    send_event(
                        &events,
                        PipelineEvent::PageFinished {
                            page,
                            total: info.page_count,
                            route: None,
                            failures: 1,
                        },
                    );
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
                    status: "succeeded".into(),
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
        let model = artifacts
            .iter()
            .find_map(|artifact| artifact.ocr.as_ref().map(|ocr| ocr.model.clone()));
        let model_peak_rss_bytes = artifacts
            .iter()
            .filter_map(|artifact| {
                artifact
                    .ocr
                    .as_ref()
                    .and_then(|ocr| ocr.model_peak_rss_bytes)
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
        info: &PdfInfo,
        page: u32,
        input_sha256: &str,
        embedded_images: u32,
    ) -> Result<PageArtifact, FailureArtifact> {
        let page_started = Instant::now();
        let layout = self
            .config
            .pdf
            .layout(source, page)
            .await
            .map_err(|error| {
                failure(page, "layout", "pdf_layout_failed", error.to_string(), true)
            })?;
        let features = classify_page(&layout, embedded_images);
        let route = features.route;
        let mut failures = Vec::new();
        let mut ocr: Option<OcrResult> = None;
        let mut crops: Vec<CropOutput> = Vec::new();
        let dpi = safe_render_dpi(layout.width_pt, layout.height_pt, self.config.render_dpi);
        let mut page_png: Option<PathBuf> = None;

        if route == PageRoute::Blank {
            return Ok(PageArtifact {
                page_number: page,
                input_sha256: input_sha256.into(),
                pipeline_version: PIPELINE_VERSION.into(),
                route,
                width_pt: layout.width_pt,
                height_pt: layout.height_pt,
                render_dpi: dpi,
                features,
                text: String::new(),
                ocr: None,
                crops,
                chunks: Vec::new(),
                failures,
                duration_ms: page_started.elapsed().as_millis() as u64,
            });
        }

        let render_prefix = run_dir.join("pages").join(format!("page-{page:04}"));

        let text = match route {
            PageRoute::Digital => layout.text.clone(),
            PageRoute::Ocr => {
                let png = self
                    .config
                    .pdf
                    .render_png(source, page, dpi, &render_prefix)
                    .await
                    .map_err(|error| {
                        failure(page, "render", "render_failed", error.to_string(), true)
                    })?;
                page_png = Some(png.clone());
                let client = self.config.ocr.as_ref().ok_or_else(|| {
                    failure(
                        page,
                        "ocr",
                        "ocr_model_not_configured",
                        "OCR is required for this page but no OCR client is configured",
                        false,
                    )
                })?;
                let result = client.recognize_png(&png).await.map_err(|error| {
                    failure(
                        page,
                        "ocr",
                        "ocr_failed",
                        ocr_message(&error),
                        !matches!(
                            error,
                            OcrError::NotConfigured | OcrError::ImageTooLarge | OcrError::Empty
                        ),
                    )
                })?;
                let text = result.text.clone();
                ocr = Some(result);
                text
            }
            PageRoute::Hybrid => {
                let png = self
                    .config
                    .pdf
                    .render_png(source, page, dpi, &render_prefix)
                    .await
                    .map_err(|error| {
                        failure(page, "render", "render_failed", error.to_string(), true)
                    })?;
                page_png = Some(png.clone());
                let mut text = layout.text.clone();
                match &self.config.ocr {
                    Some(client) => match client.recognize_png(&png).await {
                        Ok(result) => {
                            text = merge_hybrid_text(&text, &result.text);
                            ocr = Some(result);
                        }
                        Err(error) => failures.push(failure(
                            page,
                            "ocr",
                            "ocr_failed",
                            ocr_message(&error),
                            !matches!(
                                error,
                                OcrError::NotConfigured | OcrError::ImageTooLarge | OcrError::Empty
                            ),
                        )),
                    },
                    None => failures.push(failure(
                        page,
                        "ocr",
                        "ocr_model_not_configured",
                        "hybrid OCR was requested but no OCR client is configured",
                        false,
                    )),
                }
                text
            }
            PageRoute::Blank => String::new(),
        };

        if page_png.is_none() {
            match self
                .config
                .pdf
                .render_png(source, page, dpi, &render_prefix)
                .await
            {
                Ok(png) => page_png = Some(png),
                Err(error) => failures.push(failure(
                    page,
                    "render",
                    "render_failed",
                    error.to_string(),
                    true,
                )),
            }
        }
        if let Some(png) = &page_png {
            match select_regions(&self.config.crops, &layout, route, png, dpi).await {
                Ok(regions) => {
                    for region in regions {
                        match crop_region(
                            &self.config.crops,
                            png,
                            layout.width_pt,
                            layout.height_pt,
                            dpi,
                            &run_dir.join("crops"),
                            run_id,
                            page,
                            &region,
                        )
                        .await
                        {
                            Ok(crop) => crops.push(crop),
                            Err(error) => failures.push(failure(
                                page,
                                "crop",
                                "crop_failed",
                                crop_message(&error),
                                !matches!(error, CropError::Unsafe),
                            )),
                        }
                    }
                }
                Err(error) => failures.push(failure(
                    page,
                    "crop",
                    "crop_regions_failed",
                    crop_message(&error),
                    !matches!(error, CropError::Unsafe),
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
        let _ = info;
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
            ocr,
            crops,
            chunks,
            failures,
            duration_ms: page_started.elapsed().as_millis() as u64,
        })
    }
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
    }

    #[test]
    fn crop_association_uses_normalized_containment() {
        let crops = vec![crop("Yield is 42%."), crop("Formula: E = mc^2")];
        let ids = associated_crops("The yield is 42%.\nFormula: E=mc^2", &crops);
        assert_eq!(ids.len(), 2);
        assert!(associated_crops("Unrelated sentence.", &crops).is_empty());
    }

    #[test]
    fn safe_dpi_caps_large_pages() {
        assert_eq!(safe_render_dpi(612.0, 792.0, 200), 200);
        assert!(safe_render_dpi(2000.0, 3000.0, 300) <= 300);
        let dpi = safe_render_dpi(2000.0, 3000.0, 300);
        let pixels = (2000.0 * dpi as f32 / 72.0) * (3000.0 * dpi as f32 / 72.0);
        assert!(pixels <= MAX_RENDER_PIXELS as f32);
    }
}
