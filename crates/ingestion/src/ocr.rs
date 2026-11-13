//! Out-of-process PaddleOCR-VL OCR through llama-server.
//!
//! Model residency is owned by this worker: one lease, one child process, and a
//! hard RSS/host-headroom watchdog. The OCR text is returned with model and
//! prompt provenance so a later content version can be reproduced or rejected.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;
use tokio::time::Instant;

pub use crate::llama::{
    MAX_MODEL_RSS_BYTES, MODEL_START_TIMEOUT, ModelRelease, detect_vulkan_device,
};
use crate::{
    llama::{
        LlamaError, LlamaLaunchSpec, ManagedLlamaLease, ManagedLlamaRuntime, ManagedLlamaServer,
        build_local_http_client, check_model_memory, validate_loopback_endpoint,
    },
    process_peak_rss,
};

pub const OCR_PROMPT_VERSION: &str = "paddleocr-vl-ocr-v1";
pub const OCR_PROMPT: &str = "OCR:";
pub const MAX_OCR_IMAGE_BYTES: usize = 16 * 1024 * 1024;
/// llama-server retains allocator/vision memory across requests. A bounded
/// restart interval keeps a long deck under the same RSS budget without
/// changing page extraction. Counted in OCR requests, not pages.
pub const OCR_RESTART_AFTER_REQUESTS: usize = 16;
const OCR_VULKAN_PARALLEL_SLOTS: usize = 2;

#[derive(Debug, Error)]
pub enum OcrError {
    #[error("OCR inference requires a literal loopback HTTP(S) endpoint")]
    NonLocalEndpoint,
    #[error("the OCR model is not configured")]
    NotConfigured,
    #[error("the OCR model could not be started")]
    ModelStart,
    #[error("the OCR model did not become ready")]
    ModelTimeout,
    #[error("the OCR model exceeded its memory budget")]
    Memory,
    #[error("the OCR image is too large")]
    ImageTooLarge,
    #[error("the OCR request failed")]
    Transport,
    #[error("the OCR endpoint returned HTTP {0}")]
    Http(u16),
    #[error("the OCR response was malformed")]
    InvalidResponse,
    #[error("the OCR response was empty")]
    Empty,
    #[error("the OCR request timed out")]
    Timeout,
    #[error("the OCR model tree or its memory release could not be verified")]
    Release,
}

impl From<LlamaError> for OcrError {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrResult {
    pub text: String,
    pub model: String,
    pub prompt_version: String,
    pub duration_ms: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    #[serde(default)]
    pub model_peak_rss_bytes: Option<u64>,
}

#[derive(Clone)]
pub struct OcrClientConfig {
    pub endpoint: String,
    pub model: String,
    pub prompt: String,
    pub prompt_version: String,
    pub timeout: Duration,
    pub max_image_bytes: usize,
    pub max_model_rss_bytes: u64,
    pub model_pid: Option<u32>,
    authorization: Option<reqwest::header::HeaderValue>,
    runtime: Option<Arc<ManagedOcrRuntime>>,
}

impl OcrClientConfig {
    pub fn external(endpoint: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            model: model.into(),
            prompt: OCR_PROMPT.into(),
            prompt_version: OCR_PROMPT_VERSION.into(),
            timeout: Duration::from_secs(120),
            max_image_bytes: MAX_OCR_IMAGE_BYTES,
            max_model_rss_bytes: MAX_MODEL_RSS_BYTES,
            model_pid: None,
            authorization: None,
            runtime: None,
        }
    }

    fn managed(runtime: Arc<ManagedOcrRuntime>, model: impl Into<String>) -> Self {
        // The runtime supplies the live endpoint and bearer token per request;
        // this literal loopback placeholder only satisfies construction-time
        // endpoint validation.
        let mut config = Self::external("http://127.0.0.1/", model);
        config.runtime = Some(runtime);
        config
    }
}

impl std::fmt::Debug for OcrClientConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OcrClientConfig")
            .field("endpoint", &self.endpoint)
            .field("model", &self.model)
            .field("prompt_version", &self.prompt_version)
            .field("timeout", &self.timeout)
            .field("max_image_bytes", &self.max_image_bytes)
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

#[derive(Clone)]
pub struct OcrClient {
    client: reqwest::Client,
    config: OcrClientConfig,
}

impl std::fmt::Debug for OcrClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OcrClient")
            .field("config", &self.config)
            .finish()
    }
}

struct OcrConnection {
    endpoint: String,
    authorization: Option<reqwest::header::HeaderValue>,
    model_pid: Option<u32>,
    _lease: Option<ManagedLlamaLease>,
}

impl OcrClient {
    pub fn new(config: OcrClientConfig) -> Result<Self, OcrError> {
        let _ = validate_loopback_endpoint(&config.endpoint)?;
        let client = build_local_http_client(config.timeout)?;
        Ok(Self { client, config })
    }

    pub fn config(&self) -> &OcrClientConfig {
        &self.config
    }

    async fn connection(&self) -> Result<OcrConnection, OcrError> {
        if let Some(runtime) = &self.config.runtime {
            return runtime.connection().await;
        }
        Ok(OcrConnection {
            endpoint: self.config.endpoint.clone(),
            authorization: self.config.authorization.clone(),
            model_pid: self.config.model_pid,
            _lease: None,
        })
    }

    /// Enforce the model budget before and after each request. A page image
    /// request is serialized by the pipeline, so RSS cannot be multiplied by
    /// concurrent OCR prompts.
    fn check_memory(&self, model_pid: Option<u32>) -> Result<(), OcrError> {
        check_model_memory(model_pid, self.config.max_model_rss_bytes).map_err(OcrError::from)
    }

    pub async fn recognize_png(&self, path: &Path) -> Result<OcrResult, OcrError> {
        let connection = self.connection().await?;
        self.check_memory(connection.model_pid)?;
        let metadata = tokio::fs::metadata(path)
            .await
            .map_err(|_| OcrError::Transport)?;
        if metadata.len() == 0 || metadata.len() > self.config.max_image_bytes as u64 {
            return Err(OcrError::ImageTooLarge);
        }
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|_| OcrError::Transport)?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        let image_url = format!("data:image/png;base64,{encoded}");
        let body = json!({
            "model": self.config.model,
            "temperature": 0,
            "max_tokens": 1024,
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "text", "text": self.config.prompt},
                    {"type": "image_url", "image_url": {"url": image_url}}
                ]
            }]
        });
        let started = Instant::now();
        let mut request = self
            .client
            .post(format!(
                "{}/v1/chat/completions",
                connection.endpoint.trim_end_matches('/')
            ))
            .json(&body);
        if let Some(authorization) = &connection.authorization {
            request = request.header(reqwest::header::AUTHORIZATION, authorization.clone());
        }
        let response = request.send().await.map_err(|error| {
            if error.is_timeout() {
                OcrError::Timeout
            } else {
                OcrError::Transport
            }
        })?;
        let status = response.status();
        if !status.is_success() {
            return Err(OcrError::Http(status.as_u16()));
        }
        let value: Value = response
            .json()
            .await
            .map_err(|_| OcrError::InvalidResponse)?;
        let content = value["choices"][0]["message"]["content"]
            .as_str()
            .ok_or(OcrError::InvalidResponse)?;
        let text = normalize_ocr_text(content);
        if text.is_empty() {
            return Err(OcrError::Empty);
        }
        self.check_memory(connection.model_pid)?;
        Ok(OcrResult {
            text,
            model: self.config.model.clone(),
            prompt_version: self.config.prompt_version.clone(),
            duration_ms: started.elapsed().as_millis() as u64,
            prompt_tokens: value["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
            completion_tokens: value["usage"]["completion_tokens"].as_u64().unwrap_or(0),
            model_peak_rss_bytes: connection
                .model_pid
                .and_then(|pid| process_peak_rss(pid).ok()),
        })
    }
}

/// Remove VLM wrappers that are not source text, without weakening numbers,
/// units, formulas, or negation.
pub fn normalize_ocr_text(raw: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in raw.lines() {
        let mut line = line.trim().to_owned();
        for fence in ["```markdown", "```text", "```"] {
            if line.starts_with(fence) {
                line = line[fence.len()..].trim().to_owned();
            }
            if line.ends_with(fence) {
                line.truncate(line.len() - fence.len());
                line = line.trim().to_owned();
            }
        }
        if line.is_empty() {
            continue;
        }
        // PaddleOCR-VL sometimes wraps formulas in LaTeX delimiters.
        line = line
            .replace("\\(", "")
            .replace("\\)", "")
            .replace("\\[", "")
            .replace("\\]", "");
        while line.starts_with('$') && line.ends_with('$') && line.len() > 1 {
            line = line[1..line.len() - 1].trim().to_owned();
        }
        if !line.is_empty() {
            lines.push(line);
        }
    }
    lines.join("\n")
}

#[derive(Debug, Clone)]
pub struct ManagedOcrConfig {
    pub binary: PathBuf,
    pub model: PathBuf,
    pub mmproj: PathBuf,
    pub work_dir: PathBuf,
    pub lease_path: PathBuf,
    pub threads: u32,
    pub context_size: u32,
    pub gpu_device: Option<String>,
}

impl From<&ManagedOcrConfig> for LlamaLaunchSpec {
    fn from(config: &ManagedOcrConfig) -> Self {
        Self {
            role: "ocr",
            binary: config.binary.clone(),
            model: config.model.clone(),
            mmproj: Some(config.mmproj.clone()),
            work_dir: config.work_dir.clone(),
            lease_path: config.lease_path.clone(),
            threads: config.threads,
            context_size: config.context_size.clamp(1024, 8192),
            parallel_slots: if config.gpu_device.is_some() {
                OCR_VULKAN_PARALLEL_SLOTS as u32
            } else {
                1
            },
            gpu_device: config.gpu_device.clone(),
            budget_bytes: MAX_MODEL_RSS_BYTES,
            restart_after_requests: Some(OCR_RESTART_AFTER_REQUESTS),
            extra_args: Vec::new(),
        }
    }
}

pub struct ManagedOcrServer {
    inner: ManagedLlamaServer,
    pub model_pid: u32,
}

impl ManagedOcrServer {
    pub async fn start(config: &ManagedOcrConfig) -> Result<Self, OcrError> {
        let inner = ManagedLlamaServer::start(&LlamaLaunchSpec::from(config)).await?;
        let model_pid = inner.model_pid;
        Ok(Self { inner, model_pid })
    }

    pub fn endpoint(&self) -> &str {
        self.inner.endpoint()
    }

    pub fn log_path(&self) -> &Path {
        self.inner.log_path()
    }

    pub fn client_config(&self, model: String) -> OcrClientConfig {
        let mut config = OcrClientConfig::external(self.inner.endpoint(), model);
        config.model_pid = Some(self.model_pid);
        config.authorization = Some(self.inner.authorization().clone());
        config
    }

    /// Kill/reap the complete tree and remove its cgroup before releasing the
    /// model lease. Never infer resource release from a falling RSS sample.
    pub async fn stop(self) -> Result<ModelRelease, OcrError> {
        self.inner.stop().await.map_err(OcrError::from)
    }
}

pub struct ManagedOcrRuntime {
    inner: Arc<ManagedLlamaRuntime>,
}

impl ManagedOcrRuntime {
    pub async fn start(config: ManagedOcrConfig) -> Result<Arc<Self>, OcrError> {
        let inner = ManagedLlamaRuntime::start(LlamaLaunchSpec::from(&config)).await?;
        Ok(Arc::new(Self { inner }))
    }

    pub fn client(self: &Arc<Self>, model: String) -> Result<OcrClient, OcrError> {
        OcrClient::new(OcrClientConfig::managed(self.clone(), model))
    }

    pub fn restarts(&self) -> usize {
        self.inner.restarts()
    }

    async fn connection(&self) -> Result<OcrConnection, OcrError> {
        let lease = self.inner.acquire_request().await?;
        Ok(OcrConnection {
            endpoint: lease.endpoint.clone(),
            authorization: Some(lease.authorization.clone()),
            model_pid: Some(lease.model_pid),
            _lease: Some(lease),
        })
    }

    /// Stop the currently owned model and verify its tree/cgroup release.
    pub async fn stop(&self) -> Result<ModelRelease, OcrError> {
        self.inner.stop().await.map_err(OcrError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_authorization_is_redacted_from_debug_output() {
        let mut config = OcrClientConfig::external("http://127.0.0.1:8080", "local");
        let mut authorization =
            reqwest::header::HeaderValue::from_static("Bearer synthetic-secret");
        authorization.set_sensitive(true);
        config.authorization = Some(authorization);
        assert!(!format!("{config:?}").contains("synthetic-secret"));
    }

    #[test]
    fn removes_wrappers_but_keeps_critical_text() {
        let raw = "```text\nPhotosynthesis converts light energy.\n\\(E = mc^2\\)\nThe yield is 42%, not 24%.\n```\n";
        let text = normalize_ocr_text(raw);
        assert!(text.contains("E = mc^2"));
        assert!(text.contains("42%, not 24%"));
        assert!(!text.contains("```"));
        assert!(!text.contains("\\("));
    }

    #[test]
    fn private_derivatives_cannot_be_sent_to_nonlocal_ocr_endpoints() {
        for endpoint in [
            "https://example.com",
            "http://localhost:8080",
            "http://192.168.1.2:8080",
            "http://127.0.0.1:8080/path",
            "http://user:secret@127.0.0.1:8080",
        ] {
            assert!(matches!(
                OcrClient::new(OcrClientConfig::external(endpoint, "local")),
                Err(OcrError::NonLocalEndpoint)
            ));
        }
        for endpoint in ["http://127.0.0.1:8080", "http://[::1]:8080"] {
            assert!(OcrClient::new(OcrClientConfig::external(endpoint, "local")).is_ok());
        }
    }
}
