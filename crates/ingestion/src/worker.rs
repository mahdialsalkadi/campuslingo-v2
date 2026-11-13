//! DB-backed M2 worker. Files are durable before their metadata transaction;
//! replay/restart is fenced by the current job lease and immutable provenance.
use crate::{
    embeddings::{EmbeddingClient, ManagedEmbeddingConfig, ManagedEmbeddingRuntime},
    fingerprint,
    llama::ModelRelease,
    ocr::{ManagedOcrConfig, ManagedOcrRuntime, OcrClient},
    pdf::PdfTools,
    pipeline::{ExtractionPipeline, PIPELINE_VERSION, PageArtifact, PipelineConfig, PipelineEvent},
};
use campuslingo_storage::{
    ingestion::{self, ChunkCheckpoint, CropCheckpoint, EmbeddingVector, PageCheckpoint, RunInput},
    jobs,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::mpsc,
};
use uuid::Uuid;

type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    schema_version: u32,
    kind: String,
    asset_id: Uuid,
}

#[derive(Clone)]
pub struct ManagedOcrWorkerSpec {
    pub config: ManagedOcrConfig,
    pub model_name: String,
}

#[derive(Clone)]
pub struct WorkerConfig {
    pub work_dir: PathBuf,
    pub asset_dir: PathBuf,
    /// Optional authenticated loopback OCR client. When absent alongside
    /// `managed_ocr`, OCR-required content remains explicitly incomplete; no
    /// model is loaded implicitly.
    pub ocr: Option<OcrClient>,
    /// Optional supervised OCR config started for Phase 1 and stopped/released
    /// before Phase 2 embedding begins.
    pub managed_ocr: Option<ManagedOcrWorkerSpec>,
    /// Optional authenticated loopback embedding client.
    pub embeddings: Option<EmbeddingClient>,
    /// Optional supervised embedding config started only after OCR release.
    pub managed_embeddings: Option<ManagedEmbeddingConfig>,
    /// Non-secret model identities bound into run provenance.
    pub ocr_provenance: Option<Value>,
    pub embedding_provenance: Option<Value>,
}

impl WorkerConfig {
    pub fn new(work_dir: impl Into<PathBuf>, asset_dir: impl Into<PathBuf>) -> Self {
        Self {
            work_dir: work_dir.into(),
            asset_dir: asset_dir.into(),
            ocr: None,
            managed_ocr: None,
            embeddings: None,
            managed_embeddings: None,
            ocr_provenance: None,
            embedding_provenance: None,
        }
    }
}

async fn hash_file(path: &Path) -> Result<(String, u64), Error> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut buffer = vec![0; 65536];
    let mut sha = Sha256::new();
    let mut bytes = 0;
    loop {
        let n = file.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        sha.update(&buffer[..n]);
    }
    Ok((hex::encode(sha.finalize()), bytes))
}

/// Read executable bytes, not just a display version that could hide a replaced
/// binary. No paths, secrets or private source content are emitted to logs.
async fn tool_provenance(
    ocr_provenance: Option<&Value>,
    embedding_provenance: Option<&Value>,
) -> Result<Value, Error> {
    let mut tools = serde_json::Map::new();
    for tool in [
        "pdfinfo",
        "pdftohtml",
        "pdftotext",
        "pdfimages",
        "pdftoppm",
        "pdftocairo",
        "rsvg-convert",
        "magick",
        "tesseract",
    ] {
        let path = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|p| p.join(tool))
            .find(|p| p.is_file())
            .ok_or("required ingestion helper missing")?
            .canonicalize()?;
        tools.insert(tool.into(), json!({"sha256":hash_file(&path).await?.0}));
    }
    let mut provenance = json!({
        "schema_version": 1,
        "pipeline_version": PIPELINE_VERSION,
        "tools": tools,
        "ocr_model": ocr_provenance.cloned(),
        "normalization_version": "critical-v2-indent"
    });
    if let Some(embedding) = embedding_provenance {
        provenance["embedding_model"] = embedding.clone();
    }
    Ok(provenance)
}

/// Install verified immutable bytes under a UUID. Never overwrite a preexisting
/// asset; an interrupted or conflicting file fails closed instead of replacing
/// unfamiliar/user bytes. A retry reuses an already-complete matching blob.
async fn install_crop(
    config: &WorkerConfig,
    run_dir: &Path,
    crop: &crate::crops::CropOutput,
    lease: Uuid,
) -> Result<(), Error> {
    if crop.relative_path != format!("crops/{}.webp", crop.id) {
        return Err("unsafe crop path".into());
    }
    let source = run_dir.join(&crop.relative_path);
    let (hash, bytes) = hash_file(&source).await?;
    if hash != crop.sha256 || bytes != crop.bytes || bytes > 8 * 1024 * 1024 {
        return Err("crop integrity mismatch".into());
    }
    let destination = config.asset_dir.join(crop.id.to_string());
    if destination.exists() {
        if hash_file(&destination).await? == (hash, bytes) {
            return Ok(());
        }
        return Err("existing asset does not match checkpoint".into());
    }
    let temporary = config
        .asset_dir
        .join(format!("{}.{}.ingesting", crop.id, lease));
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut output = options.open(&temporary).await?;
    let mut input = tokio::fs::File::open(&source).await?;
    tokio::io::copy(&mut input, &mut output).await?;
    output.flush().await?;
    output.sync_all().await?;
    drop(output);
    let result = tokio::fs::hard_link(&temporary, &destination).await;
    let _ = tokio::fs::remove_file(&temporary).await;
    if let Err(error) = result
        && (error.kind() != std::io::ErrorKind::AlreadyExists
            || hash_file(&destination).await? != (hash, bytes))
    {
        return Err(error.into());
    }
    // Metadata cannot commit before directory entries survive a host restart.
    std::fs::File::open(&config.asset_dir)?.sync_all()?;
    Ok(())
}

pub fn page_checkpoint(artifact: &PageArtifact) -> Result<PageCheckpoint, Error> {
    page_checkpoint_with_embeddings(artifact, &HashMap::new())
}

pub fn page_checkpoint_with_embeddings(
    artifact: &PageArtifact,
    embeddings: &HashMap<Uuid, EmbeddingVector>,
) -> Result<PageCheckpoint, Error> {
    let mut value = serde_json::to_value(artifact)?;
    value["schema_version"] = json!(1);
    let crops = artifact
        .crops
        .iter()
        .map(|crop| CropCheckpoint {
            id: crop.id,
            region_index: crop.region_index as i32,
            dpi: crop.dpi as i32,
            width: crop.width as i32,
            height: crop.height as i32,
            bbox: json!({"schema_version":1,"rect":crop.bbox}),
            bytes: crop.bytes as i64,
            sha256: crop.sha256.clone(),
        })
        .collect();
    let chunks = artifact
        .chunks
        .iter()
        .map(|chunk| {
            let fp = fingerprint::fingerprint(&chunk.text);
            ChunkCheckpoint {
                id: chunk.id,
                order: chunk.order as i32,
                source_text: chunk.text.clone(),
                normalized_candidate: fp.normalized_text,
                text_sha256: fp.text_sha256,
                structural_fingerprint: fp.critical_sha256,
                critical_tokens: json!({"schema_version":1,"tokens":fp.critical_tokens}),
                duplicate_of: chunk.duplicate_of,
                crop_ids: chunk.crop_ids.clone(),
                language_code: None,
                is_quarantined: false,
                embedding: embeddings.get(&chunk.id).cloned(),
            }
        })
        .collect();
    Ok(PageCheckpoint {
        page_number: artifact.page_number as i32,
        ready: artifact.failures.is_empty(),
        artifact: value,
        crops,
        chunks,
    })
}

async fn read_page_artifact(run_dir: &Path, page: u32) -> Result<PageArtifact, Error> {
    let path = run_dir.join("pages").join(format!("page-{page:04}.json"));
    let artifact: PageArtifact = serde_json::from_slice(&tokio::fs::read(path).await?)?;
    if artifact.page_number != page || artifact.pipeline_version != PIPELINE_VERSION {
        return Err("page provenance mismatch".into());
    }
    Ok(artifact)
}

async fn persist_page_artifact(
    pool: &PgPool,
    job: &jobs::ClaimedJob,
    run: Uuid,
    run_dir: &Path,
    config: &WorkerConfig,
    artifact: &PageArtifact,
    embeddings: &HashMap<Uuid, EmbeddingVector>,
) -> Result<(), Error> {
    for crop in &artifact.crops {
        install_crop(config, run_dir, crop, job.lease_token).await?;
    }
    ingestion::checkpoint_page(
        pool,
        job.id,
        job.lease_token,
        run,
        &page_checkpoint_with_embeddings(artifact, embeddings)?,
    )
    .await?;
    Ok(())
}

async fn persist_page(
    pool: &PgPool,
    job: &jobs::ClaimedJob,
    run: Uuid,
    run_dir: &Path,
    config: &WorkerConfig,
    page: u32,
) -> Result<(), Error> {
    let artifact = read_page_artifact(run_dir, page).await?;
    persist_page_artifact(pool, job, run, run_dir, config, &artifact, &HashMap::new()).await
}

fn verify_clean_release(release: &ModelRelease, stage: &str) -> Result<(), Error> {
    if !release.process_tree_reaped
        || !release.cgroup_removed
        || release.memory_budget_exceeded
        || release.cgroup_oom_kill_count > 0
        || release.termination.is_some()
    {
        return Err(format!("{stage} resource release failed").into());
    }
    Ok(())
}

async fn stop_managed_ocr(runtime: Arc<ManagedOcrRuntime>) -> Result<(ModelRelease, usize), Error> {
    let restarts = runtime.restarts();
    let release = runtime.stop().await?;
    verify_clean_release(&release, "OCR")?;
    Ok((release, restarts))
}

async fn embed_artifacts(
    pool: &PgPool,
    job: &jobs::ClaimedJob,
    page_count: u32,
    artifacts: &[PageArtifact],
    client: &EmbeddingClient,
) -> Result<HashMap<Uuid, EmbeddingVector>, Error> {
    let mut items: Vec<(Uuid, String)> = Vec::new();
    for artifact in artifacts {
        for chunk in &artifact.chunks {
            if chunk.duplicate_of.is_some() {
                continue;
            }
            let fp = fingerprint::fingerprint(&chunk.text);
            if !fp.normalized_text.trim().is_empty() {
                items.push((chunk.id, fp.normalized_text));
            }
        }
    }
    let mut vectors = HashMap::with_capacity(items.len());
    for batch in items.chunks(16) {
        let refs: Vec<&str> = batch.iter().map(|(_, text)| text.as_str()).collect();
        let embedded = client.embed_texts(&refs).await?;
        for ((id, _), vector) in batch.iter().zip(embedded) {
            vectors.insert(*id, vector);
        }
        jobs::heartbeat(
            pool,
            job.id,
            job.lease_token,
            "embedding",
            page_count as i32,
            page_count as i32,
        )
        .await?;
    }
    Ok(vectors)
}

async fn process_claim(
    pool: &PgPool,
    config: &WorkerConfig,
    job: &jobs::ClaimedJob,
) -> Result<Value, Error> {
    if config.ocr.is_some() && config.managed_ocr.is_some() {
        return Err("ambiguous OCR configuration".into());
    }
    if config.embeddings.is_some() && config.managed_embeddings.is_some() {
        return Err("ambiguous embedding configuration".into());
    }
    let payload: Payload = serde_json::from_value(job.payload.clone())?;
    if payload.schema_version != 1 || payload.kind != "pdf_ingestion" {
        return Err("unsupported ingestion payload".into());
    }
    let source = config.asset_dir.join(payload.asset_id.to_string());
    let (input_sha256, _) = hash_file(&source).await?;
    let info = PdfTools::default().validate(&source).await?;
    let run = ingestion::start_run(
        pool,
        job.id,
        job.lease_token,
        &RunInput {
            source_asset_id: payload.asset_id,
            input_sha256,
            pipeline_version: PIPELINE_VERSION.into(),
            provenance: tool_provenance(
                config.ocr_provenance.as_ref(),
                config.embedding_provenance.as_ref(),
            )
            .await?,
            source_pages: info.page_count as i32,
        },
    )
    .await?;
    let run_dir = config.work_dir.join(run.to_string());
    let defer_checkpoint = config.embeddings.is_some() || config.managed_embeddings.is_some();

    let managed_ocr = match &config.managed_ocr {
        Some(spec) => {
            let runtime = ManagedOcrRuntime::start(spec.config.clone()).await?;
            let client = runtime.client(spec.model_name.clone())?;
            Some((runtime, client))
        }
        None => None,
    };

    let mut pipeline_config = PipelineConfig::new(config.work_dir.clone());
    pipeline_config.ocr = managed_ocr
        .as_ref()
        .map(|(_, client)| client.clone())
        .or_else(|| config.ocr.clone());
    let pipeline = ExtractionPipeline::new(pipeline_config);
    let mut loop_error: Option<Error> = None;
    let manifest_result = {
        let (sender, mut receiver) = mpsc::channel(32);
        let extraction = pipeline.run(&source, run, Some(sender));
        tokio::pin!(extraction);
        let mut heartbeat = tokio::time::interval(Duration::from_secs(15));
        let mut done = 0_u32;
        loop {
            tokio::select! {
                result = &mut extraction => break Some(result),
                event = receiver.recv() => if let Some(PipelineEvent::PageFinished { page, route: Some(_), .. }) = event {
                    if !defer_checkpoint && let Err(error) = persist_page(pool, job, run, &run_dir, config, page).await {
                        loop_error = Some(error);
                        break None;
                    }
                    done = done.max(page);
                },
                _ = heartbeat.tick() => {
                    if let Err(error) = jobs::heartbeat(pool, job.id, job.lease_token, "extracting", done as i32, info.page_count as i32).await {
                        loop_error = Some(error.into());
                        break None;
                    }
                }
            }
        }
    };
    drop(pipeline);

    let (ocr_release, ocr_restarts) = if let Some((runtime, client)) = managed_ocr {
        drop(client);
        let (release, restarts) = stop_managed_ocr(runtime).await?;
        (Some(release), Some(restarts))
    } else {
        (None, None)
    };
    if let Some(error) = loop_error {
        return Err(error);
    }
    let manifest = manifest_result.ok_or("PDF extraction did not finish")??;
    for page in &manifest.pages {
        if page.route.is_none() {
            return Err("PDF page extraction failed".into());
        }
    }

    let mut embedding_release: Option<ModelRelease> = None;
    let mut chunks_embedded = 0_usize;
    if defer_checkpoint {
        let mut artifacts = Vec::with_capacity(manifest.pages.len());
        for page in &manifest.pages {
            artifacts.push(read_page_artifact(&run_dir, page.page_number).await?);
        }
        let chunk_embeddings = if let Some(embed_cfg) = &config.managed_embeddings {
            let runtime = ManagedEmbeddingRuntime::start(embed_cfg.clone()).await?;
            let model_name = embed_cfg
                .model
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| embed_cfg.model_id.clone());
            let client = runtime.client(model_name)?;
            let embedded_result =
                embed_artifacts(pool, job, info.page_count, &artifacts, &client).await;
            drop(client);
            let release = runtime.stop().await?;
            verify_clean_release(&release, "embedding")?;
            embedding_release = Some(release);
            embedded_result?
        } else if let Some(client) = &config.embeddings {
            embed_artifacts(pool, job, info.page_count, &artifacts, client).await?
        } else {
            HashMap::new()
        };
        chunks_embedded = chunk_embeddings.len();
        for artifact in &artifacts {
            persist_page_artifact(
                pool,
                job,
                run,
                &run_dir,
                config,
                artifact,
                &chunk_embeddings,
            )
            .await?;
        }
    } else {
        // Reconcile events even when the bounded notification channel was full.
        for page in &manifest.pages {
            persist_page(pool, job, run, &run_dir, config, page.page_number).await?;
        }
    }

    let status = ingestion::finish_run(pool, job.id, job.lease_token, run).await?;
    let mut summary = json!({
        "schema_version": 1,
        "job_id": job.id,
        "run_id": run,
        "status": status,
        "pages": info.page_count,
    });
    if defer_checkpoint {
        summary["chunks_embedded"] = json!(chunks_embedded);
    }
    if let Some(release) = ocr_release {
        summary["ocr_release"] = json!(release);
    }
    if let Some(restarts) = ocr_restarts {
        summary["ocr_restarts"] = json!(restarts);
    }
    if let Some(release) = embedding_release {
        summary["embedding_release"] = json!(release);
    }
    Ok(summary)
}

/// One durable claim; no model is implicitly loaded. OCR-required content goes
/// to needs_review until the approved supervised OCR stage is configured.
pub async fn work_once(pool: &PgPool, config: &WorkerConfig) -> Result<Option<Value>, Error> {
    work_once_with_headroom(pool, config, crate::host_memory()?.available_bytes).await
}

async fn work_once_with_headroom(
    pool: &PgPool,
    config: &WorkerConfig,
    available: u64,
) -> Result<Option<Value>, Error> {
    if !crate::admits_ingestion(available) {
        // This branch must precede even the reaper/claim transaction. Existing
        // jobs stay queued; low host memory is not a job failure or a lease.
        return Ok(Some(json!({"schema_version":1,"status":"paused",
            "reason":"host_memory_below_safety_reserve", "available_host_bytes":available,
            "safety_reserve_bytes":crate::INGESTION_ADMISSION_RESERVE_BYTES})));
    }
    let Some(job) = jobs::reaper_and_claim_kind(pool, Some("pdf_ingestion")).await? else {
        return Ok(None);
    };
    match process_claim(pool, config, &job).await {
        Ok(result) => Ok(Some(result)),
        Err(_) => {
            // Do not log SQL details, filenames, source text or credentials.
            jobs::fail(
                pool,
                job.id,
                job.lease_token,
                "ingestion_checkpoint_failed",
                true,
            )
            .await?;
            Ok(Some(
                json!({"schema_version":1,"job_id":job.id,"status":"retry_or_failed"}),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn low_memory_pauses_before_any_database_claim() {
        // No database exists at this address. A claim/reaper call would fail,
        // proving the low-memory path does not acquire or mutate a job lease.
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://test:test@127.0.0.1:1/unreachable")
            .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let config = WorkerConfig::new(directory.path(), directory.path());
        let result =
            work_once_with_headroom(&pool, &config, crate::INGESTION_ADMISSION_RESERVE_BYTES - 1)
                .await
                .unwrap()
                .unwrap();
        assert_eq!(result["status"], "paused");
        assert_eq!(result["safety_reserve_bytes"], 1610612736_u64);
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }
}
