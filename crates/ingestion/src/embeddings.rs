//! Supervised local 1024-d dense embedding runtime (`S-02` / `S-03` / `S-04`).
//!
//! Runs strictly sequential to OCR and LLM generation under `ModelLease` using
//! the shared contained `llama-server` harness. Every vector is L2-normalized
//! and bound to its model id, revision, dimension (1024), and preprocessing
//! version before reaching PostgreSQL `pgvector` HNSW storage.

use std::{path::PathBuf, sync::Arc, time::Duration};

use campuslingo_storage::ingestion::{EMBEDDING_DIM, EmbeddingVector, validate_embedding_vector};
use serde::Deserialize;
use serde_json::json;
use thiserror::Error;

use crate::llama::{
    LlamaError, LlamaLaunchSpec, MAX_MODEL_RSS_BYTES, ManagedLlamaLease, ManagedLlamaRuntime,
    ModelRelease, build_local_http_client, check_model_memory, validate_loopback_endpoint,
};

pub const EMBEDDING_PREPROCESS_VERSION: &str = "critical-v2-indent";
pub const DEFAULT_EMBEDDING_MODEL_ID: &str = "Qwen/Qwen3-Embedding-0.6B-GGUF";
pub const DEFAULT_EMBEDDING_REVISION: &str = "370f27d7550e0def9b39c1f16d3fbaa13aa67728";
pub const EMBEDDING_CONTEXT_SIZE: u32 = 2048;
pub const EMBEDDING_RESTART_AFTER_REQUESTS: usize = 64;
const MAX_EMBEDDING_BATCH_ITEMS: usize = 8;
const MAX_EMBEDDING_BATCH_CHARS: usize = 1600;

#[derive(Debug, Error)]
pub enum EmbeddingError {
    #[error("embedding inference requires a literal loopback HTTP(S) endpoint")]
    NonLocalEndpoint,
    #[error("the embedding model is not configured")]
    NotConfigured,
    #[error("the embedding model could not be started")]
    ModelStart,
    #[error("the embedding model did not become ready")]
    ModelTimeout,
    #[error("the embedding model exceeded its memory budget")]
    Memory,
    #[error("the embedding request failed")]
    Transport,
    #[error("the embedding endpoint returned HTTP {0}")]
    Http(u16),
    #[error("the embedding response was malformed")]
    InvalidResponse,
    #[error("the embedding input or response was empty")]
    Empty,
    #[error("the embedding request timed out")]
    Timeout,
    #[error("the embedding model tree or its memory release could not be verified")]
    Release,
    #[error("the embedding vector violated S-02 invariants")]
    StorageContract,
}

impl From<LlamaError> for EmbeddingError {
    fn from(error: LlamaError) -> Self {
        match error {
            LlamaError::NonLocalEndpoint => Self::NonLocalEndpoint,
            LlamaError::NotConfigured => Self::NotConfigured,
            LlamaError::ModelStart => Self::ModelStart,
            LlamaError::ModelTimeout => Self::ModelTimeout,
            LlamaError::Memory => Self::Memory,
            LlamaError::Transport => Self::Transport,
            LlamaError::Http(code) => Self::Http(code),
            LlamaError::InvalidResponse => Self::InvalidResponse,
            LlamaError::Empty => Self::Empty,
            LlamaError::Timeout => Self::Timeout,
            LlamaError::Release => Self::Release,
        }
    }
}

#[derive(Clone)]
pub struct EmbeddingClientConfig {
    pub endpoint: String,
    pub model: String,
    pub model_id: String,
    pub revision: String,
    pub preprocess_version: String,
    pub timeout: Duration,
    pub max_model_rss_bytes: u64,
    pub model_pid: Option<u32>,
    authorization: Option<reqwest::header::HeaderValue>,
    runtime: Option<Arc<ManagedEmbeddingRuntime>>,
}

impl EmbeddingClientConfig {
    pub fn external(
        endpoint: impl Into<String>,
        model: impl Into<String>,
        model_id: impl Into<String>,
        revision: impl Into<String>,
    ) -> Self {
        Self {
            endpoint: endpoint.into(),
            model: model.into(),
            model_id: model_id.into(),
            revision: revision.into(),
            preprocess_version: EMBEDDING_PREPROCESS_VERSION.into(),
            timeout: Duration::from_secs(60),
            max_model_rss_bytes: MAX_MODEL_RSS_BYTES,
            model_pid: None,
            authorization: None,
            runtime: None,
        }
    }

    fn managed(
        runtime: Arc<ManagedEmbeddingRuntime>,
        model: impl Into<String>,
        model_id: impl Into<String>,
        revision: impl Into<String>,
        preprocess_version: impl Into<String>,
    ) -> Self {
        let mut config = Self::external("http://127.0.0.1/", model, model_id, revision);
        config.preprocess_version = preprocess_version.into();
        config.runtime = Some(runtime);
        config
    }
}

impl std::fmt::Debug for EmbeddingClientConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EmbeddingClientConfig")
            .field("endpoint", &self.endpoint)
            .field("model", &self.model)
            .field("model_id", &self.model_id)
            .field("revision", &self.revision)
            .field("preprocess_version", &self.preprocess_version)
            .field("timeout", &self.timeout)
            .field("max_model_rss_bytes", &self.max_model_rss_bytes)
            .field("model_pid", &self.model_pid)
            .field(
                "authorization",
                &self.authorization.as_ref().map(|_| "[REDACTED]"),
            )
            .field("managed_runtime", &self.runtime.is_some())
            .finish()
    }
}

struct EmbeddingConnection {
    endpoint: String,
    authorization: Option<reqwest::header::HeaderValue>,
    model_pid: Option<u32>,
    _lease: Option<ManagedLlamaLease>,
}

#[derive(Deserialize)]
struct EmbeddingsApiResponse {
    data: Vec<EmbeddingItem>,
}

#[derive(Deserialize)]
struct EmbeddingItem {
    index: usize,
    embedding: Vec<f32>,
}

#[derive(Clone)]
pub struct EmbeddingClient {
    client: reqwest::Client,
    config: EmbeddingClientConfig,
}

impl std::fmt::Debug for EmbeddingClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EmbeddingClient")
            .field("config", &self.config)
            .finish()
    }
}

impl EmbeddingClient {
    pub fn new(config: EmbeddingClientConfig) -> Result<Self, EmbeddingError> {
        let _ = validate_loopback_endpoint(&config.endpoint)?;
        if config.model_id.trim().is_empty()
            || config.revision.trim().is_empty()
            || config.preprocess_version.trim().is_empty()
        {
            return Err(EmbeddingError::StorageContract);
        }
        let client = build_local_http_client(config.timeout)?;
        Ok(Self { client, config })
    }

    pub fn config(&self) -> &EmbeddingClientConfig {
        &self.config
    }

    async fn connection(&self) -> Result<EmbeddingConnection, EmbeddingError> {
        if let Some(runtime) = &self.config.runtime {
            let lease = runtime.inner.acquire_request().await?;
            return Ok(EmbeddingConnection {
                endpoint: lease.endpoint.clone(),
                authorization: Some(lease.authorization.clone()),
                model_pid: Some(lease.model_pid),
                _lease: Some(lease),
            });
        }
        Ok(EmbeddingConnection {
            endpoint: self.config.endpoint.clone(),
            authorization: self.config.authorization.clone(),
            model_pid: self.config.model_pid,
            _lease: None,
        })
    }

    pub async fn embed_texts(
        &self,
        texts: &[&str],
    ) -> Result<Vec<EmbeddingVector>, EmbeddingError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        for text in texts {
            if text.trim().is_empty() {
                return Err(EmbeddingError::Empty);
            }
        }
        let mut results = Vec::with_capacity(texts.len());
        for batch in partition_batches(texts) {
            let mut batch_vectors = self.embed_batch(batch).await?;
            results.append(&mut batch_vectors);
        }
        Ok(results)
    }

    async fn embed_batch(&self, batch: &[&str]) -> Result<Vec<EmbeddingVector>, EmbeddingError> {
        let connection = self.connection().await?;
        check_model_memory(connection.model_pid, self.config.max_model_rss_bytes)?;
        let body = json!({
            "model": self.config.model,
            "input": batch,
        });
        let mut request = self
            .client
            .post(format!(
                "{}/v1/embeddings",
                connection.endpoint.trim_end_matches('/')
            ))
            .json(&body);
        if let Some(authorization) = &connection.authorization {
            request = request.header(reqwest::header::AUTHORIZATION, authorization.clone());
        }
        let response = request.send().await.map_err(|error| {
            if error.is_timeout() {
                EmbeddingError::Timeout
            } else {
                EmbeddingError::Transport
            }
        })?;
        let status = response.status();
        if !status.is_success() {
            return Err(EmbeddingError::Http(status.as_u16()));
        }
        let parsed: EmbeddingsApiResponse = response
            .json()
            .await
            .map_err(|_| EmbeddingError::InvalidResponse)?;
        if parsed.data.len() != batch.len() {
            return Err(EmbeddingError::InvalidResponse);
        }
        let mut ordered: Vec<Option<Vec<f32>>> = vec![None; batch.len()];
        for item in parsed.data {
            let slot = ordered
                .get_mut(item.index)
                .ok_or(EmbeddingError::InvalidResponse)?;
            if slot.is_some() {
                return Err(EmbeddingError::InvalidResponse);
            }
            *slot = Some(item.embedding);
        }
        check_model_memory(connection.model_pid, self.config.max_model_rss_bytes)?;
        ordered
            .into_iter()
            .map(|entry| {
                let raw = entry.ok_or(EmbeddingError::InvalidResponse)?;
                let normalized = normalize_l2_1024(&raw)?;
                let vector = EmbeddingVector {
                    values: normalized,
                    model_id: self.config.model_id.clone(),
                    revision: self.config.revision.clone(),
                    preprocess_version: self.config.preprocess_version.clone(),
                };
                validate_embedding_vector(&vector).map_err(|_| EmbeddingError::StorageContract)?;
                Ok(vector)
            })
            .collect()
    }
}

pub fn normalize_l2_1024(raw: &[f32]) -> Result<Vec<f32>, EmbeddingError> {
    if raw.len() != EMBEDDING_DIM {
        return Err(EmbeddingError::StorageContract);
    }
    let mut sum_sq = 0.0_f64;
    for &value in raw {
        if !value.is_finite() {
            return Err(EmbeddingError::InvalidResponse);
        }
        let v = f64::from(value);
        sum_sq += v * v;
    }
    if !sum_sq.is_finite() || sum_sq <= f64::EPSILON * f64::EPSILON {
        return Err(EmbeddingError::InvalidResponse);
    }
    let norm = sum_sq.sqrt();
    let out: Vec<f32> = raw.iter().map(|&v| (f64::from(v) / norm) as f32).collect();
    if out.iter().any(|v| !v.is_finite()) {
        return Err(EmbeddingError::InvalidResponse);
    }
    Ok(out)
}

fn partition_batches<'a>(texts: &'a [&'a str]) -> Vec<&'a [&'a str]> {
    let mut batches = Vec::new();
    let mut start = 0_usize;
    let mut chars = 0_usize;
    for (index, text) in texts.iter().enumerate() {
        let len = text.chars().count();
        if index > start
            && (index - start >= MAX_EMBEDDING_BATCH_ITEMS
                || chars.saturating_add(len) > MAX_EMBEDDING_BATCH_CHARS)
        {
            batches.push(&texts[start..index]);
            start = index;
            chars = 0;
        }
        chars = chars.saturating_add(len);
    }
    if start < texts.len() {
        batches.push(&texts[start..]);
    }
    batches
}

#[derive(Debug, Clone)]
pub struct ManagedEmbeddingConfig {
    pub binary: PathBuf,
    pub model: PathBuf,
    pub work_dir: PathBuf,
    pub lease_path: PathBuf,
    pub threads: u32,
    pub context_size: u32,
    pub gpu_device: Option<String>,
    pub model_id: String,
    pub revision: String,
    pub preprocess_version: String,
}

impl From<&ManagedEmbeddingConfig> for LlamaLaunchSpec {
    fn from(config: &ManagedEmbeddingConfig) -> Self {
        let context_size = config.context_size.clamp(512, 4096);
        Self {
            role: "embed",
            binary: config.binary.clone(),
            model: config.model.clone(),
            mmproj: None,
            work_dir: config.work_dir.clone(),
            lease_path: config.lease_path.clone(),
            threads: config.threads,
            context_size,
            parallel_slots: 1,
            gpu_device: config.gpu_device.clone(),
            budget_bytes: MAX_MODEL_RSS_BYTES,
            restart_after_requests: Some(EMBEDDING_RESTART_AFTER_REQUESTS),
            extra_args: vec![
                "--embedding".into(),
                "--pooling".into(),
                "last".into(),
                "--ubatch-size".into(),
                context_size.to_string(),
            ],
        }
    }
}

pub struct ManagedEmbeddingRuntime {
    inner: Arc<ManagedLlamaRuntime>,
    model_id: String,
    revision: String,
    preprocess_version: String,
}

impl ManagedEmbeddingRuntime {
    pub async fn start(config: ManagedEmbeddingConfig) -> Result<Arc<Self>, EmbeddingError> {
        if config.model_id.trim().is_empty()
            || config.revision.trim().is_empty()
            || config.preprocess_version.trim().is_empty()
        {
            return Err(EmbeddingError::StorageContract);
        }
        let inner = ManagedLlamaRuntime::start(LlamaLaunchSpec::from(&config)).await?;
        Ok(Arc::new(Self {
            inner,
            model_id: config.model_id,
            revision: config.revision,
            preprocess_version: config.preprocess_version,
        }))
    }

    pub fn client(
        self: &Arc<Self>,
        model: impl Into<String>,
    ) -> Result<EmbeddingClient, EmbeddingError> {
        EmbeddingClient::new(EmbeddingClientConfig::managed(
            self.clone(),
            model,
            &self.model_id,
            &self.revision,
            &self.preprocess_version,
        ))
    }

    pub fn restarts(&self) -> usize {
        self.inner.restarts()
    }

    pub async fn stop(&self) -> Result<ModelRelease, EmbeddingError> {
        self.inner.stop().await.map_err(EmbeddingError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cosine(a: &[f32], b: &[f32]) -> f64 {
        a.iter()
            .zip(b.iter())
            .map(|(&x, &y)| f64::from(x) * f64::from(y))
            .sum()
    }

    #[test]
    fn normalizes_and_rejects_bad_vectors() {
        let mut raw = vec![0.0_f32; EMBEDDING_DIM];
        raw[0] = 3.0;
        raw[1] = 4.0;
        let normalized = normalize_l2_1024(&raw).unwrap();
        let norm: f64 = normalized
            .iter()
            .map(|&v| f64::from(v) * f64::from(v))
            .sum::<f64>()
            .sqrt();
        assert!((norm - 1.0).abs() < 1e-6);

        assert!(matches!(
            normalize_l2_1024(&vec![0.0; 1023]),
            Err(EmbeddingError::StorageContract)
        ));
        assert!(matches!(
            normalize_l2_1024(&vec![0.0; EMBEDDING_DIM]),
            Err(EmbeddingError::InvalidResponse)
        ));
        let mut bad = vec![1.0_f32; EMBEDDING_DIM];
        bad[5] = f32::NAN;
        assert!(matches!(
            normalize_l2_1024(&bad),
            Err(EmbeddingError::InvalidResponse)
        ));
    }

    #[test]
    fn redacts_secret_and_enforces_loopback() {
        let mut cfg = EmbeddingClientConfig::external(
            "http://127.0.0.1:8080",
            "local",
            DEFAULT_EMBEDDING_MODEL_ID,
            DEFAULT_EMBEDDING_REVISION,
        );
        let mut auth = reqwest::header::HeaderValue::from_static("Bearer secret-token");
        auth.set_sensitive(true);
        cfg.authorization = Some(auth);
        assert!(!format!("{cfg:?}").contains("secret-token"));
        assert!(matches!(
            EmbeddingClient::new(EmbeddingClientConfig::external(
                "https://example.com",
                "local",
                DEFAULT_EMBEDDING_MODEL_ID,
                DEFAULT_EMBEDDING_REVISION,
            )),
            Err(EmbeddingError::NonLocalEndpoint)
        ));
    }

    #[test]
    fn partitions_batches_within_limits() {
        let items: Vec<String> = (0..11)
            .map(|i| format!("item {i}: {}", "x".repeat(250)))
            .collect();
        let refs: Vec<&str> = items.iter().map(String::as_str).collect();
        let batches = partition_batches(&refs);
        assert!(batches.len() >= 2);
        let flat: Vec<&str> = batches.into_iter().flatten().copied().collect();
        assert_eq!(flat, refs);
    }

    #[tokio::test]
    #[ignore]
    async fn contained_qwen3_embedding_and_exclusive_model_lease() {
        let root = crate::project_root();
        let model = root
            .join("artifacts/private/models/qwen3-embedding-0.6b/Qwen3-Embedding-0.6B-Q8_0.gguf");
        if !model.is_file() {
            panic!(
                "missing Qwen3-Embedding-0.6B-Q8_0.gguf at {}",
                model.display()
            );
        }
        let work_dir = root.join(format!(
            "artifacts/private/embed-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&work_dir).unwrap();
        let lease_path = work_dir.join("model.lock");
        let binary = PathBuf::from("llama-server");
        let gpu_device = crate::llama::detect_vulkan_device(&binary).await;

        let runtime = ManagedEmbeddingRuntime::start(ManagedEmbeddingConfig {
            binary: binary.clone(),
            model: model.clone(),
            work_dir: work_dir.clone(),
            lease_path: lease_path.clone(),
            threads: 8,
            context_size: EMBEDDING_CONTEXT_SIZE,
            gpu_device,
            model_id: DEFAULT_EMBEDDING_MODEL_ID.into(),
            revision: DEFAULT_EMBEDDING_REVISION.into(),
            preprocess_version: EMBEDDING_PREPROCESS_VERSION.into(),
        })
        .await
        .expect("start managed embedding runtime");

        // Strict single-model residency: acquiring another ModelLease (or second server)
        // must fail fast while the embedding model holds the global/local lease.
        assert!(crate::ModelLease::acquire(&lease_path, 0).is_err());

        let client = runtime.client("Qwen3-Embedding-0.6B-Q8_0.gguf").unwrap();
        let vectors = client
            .embed_texts(&[
                "Newton's second law states that force equals mass times acceleration (F = ma).",
                "In classical mechanics, net force F is the product of mass m and acceleration a.",
                "Chloroplasts convert solar energy into chemical energy during photosynthesis.",
            ])
            .await
            .expect("embed three texts");
        assert_eq!(vectors.len(), 3);
        for vec in &vectors {
            validate_embedding_vector(vec).unwrap();
            let norm = cosine(&vec.values, &vec.values).sqrt();
            assert!((norm - 1.0).abs() < 1e-4, "unexpected norm {norm}");
        }
        let physics_sim = cosine(&vectors[0].values, &vectors[1].values);
        let bio_sim = cosine(&vectors[0].values, &vectors[2].values);
        assert!(
            physics_sim > bio_sim + 0.05,
            "expected physics pair similarity ({physics_sim:.4}) > biology ({bio_sim:.4})"
        );

        let release = runtime.stop().await.expect("stop embedding runtime");
        assert!(release.process_tree_reaped);
        assert!(release.cgroup_removed);
        assert!(!release.memory_budget_exceeded);
        assert_eq!(release.cgroup_oom_kill_count, 0);
        assert!(release.termination.is_none());

        // Lease must be free again cleanly.
        let lease = crate::ModelLease::acquire(&lease_path, 0).expect("lease released cleanly");
        drop(lease);
        let _ = std::fs::remove_dir_all(&work_dir);
    }
}
