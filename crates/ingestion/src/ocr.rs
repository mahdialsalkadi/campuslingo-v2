//! Out-of-process PaddleOCR-VL OCR through llama-server.
//!
//! Model residency is owned by this worker: one lease, one child process, and a
//! hard RSS/host-headroom watchdog. The OCR text is returned with model and
//! prompt provenance so a later content version can be reproduced or rejected.

use std::{
    fs::File,
    net::TcpListener,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;
use tokio::{
    process::{Child, Command},
    time::{Instant, sleep},
};

use crate::{ModelLease, RESERVED_HOST_BYTES, host_memory, process_peak_rss, process_rss};

pub const OCR_PROMPT_VERSION: &str = "paddleocr-vl-ocr-v1";
pub const OCR_PROMPT: &str = "OCR:";
pub const MAX_OCR_IMAGE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_MODEL_RSS_BYTES: u64 = 1024 * 1024 * 1024;
pub const MODEL_START_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Error)]
pub enum OcrError {
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

#[derive(Debug, Clone)]
pub struct OcrClientConfig {
    pub endpoint: String,
    pub model: String,
    pub prompt: String,
    pub prompt_version: String,
    pub timeout: Duration,
    pub max_image_bytes: usize,
    pub max_model_rss_bytes: u64,
    pub model_pid: Option<u32>,
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
        }
    }
}

#[derive(Clone, Debug)]
pub struct OcrClient {
    client: reqwest::Client,
    config: OcrClientConfig,
}

impl OcrClient {
    pub fn new(config: OcrClientConfig) -> Result<Self, OcrError> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(config.timeout)
            .no_proxy()
            .build()
            .map_err(|_| OcrError::Transport)?;
        Ok(Self { client, config })
    }

    pub fn config(&self) -> &OcrClientConfig {
        &self.config
    }

    /// Enforce the model budget before and after each request. A page image
    /// request is serialized by the pipeline, so RSS cannot be multiplied by
    /// concurrent OCR prompts.
    pub fn check_memory(&self) -> Result<(), OcrError> {
        if let Some(pid) = self.config.model_pid {
            let rss = process_rss(pid).map_err(|_| OcrError::Memory)?;
            if rss > self.config.max_model_rss_bytes {
                return Err(OcrError::Memory);
            }
        }
        let available = host_memory().map_err(|_| OcrError::Memory)?.available_bytes;
        if available < RESERVED_HOST_BYTES {
            return Err(OcrError::Memory);
        }
        Ok(())
    }

    pub async fn recognize_png(&self, path: &Path) -> Result<OcrResult, OcrError> {
        self.check_memory()?;
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
        let response = self
            .client
            .post(format!(
                "{}/v1/chat/completions",
                self.config.endpoint.trim_end_matches('/')
            ))
            .json(&body)
            .send()
            .await
            .map_err(|error| {
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
        self.check_memory()?;
        Ok(OcrResult {
            text,
            model: self.config.model.clone(),
            prompt_version: self.config.prompt_version.clone(),
            duration_ms: started.elapsed().as_millis() as u64,
            prompt_tokens: value["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
            completion_tokens: value["usage"]["completion_tokens"].as_u64().unwrap_or(0),
            model_peak_rss_bytes: self
                .config
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
}

pub struct ManagedOcrServer {
    child: Child,
    endpoint: String,
    pub model_pid: u32,
    _lease: ModelLease,
    log_path: PathBuf,
}

impl ManagedOcrServer {
    pub async fn start(config: &ManagedOcrConfig) -> Result<Self, OcrError> {
        let model_bytes = std::fs::metadata(&config.model)
            .map_err(|_| OcrError::ModelStart)?
            .len();
        let mmproj_bytes = std::fs::metadata(&config.mmproj)
            .map_err(|_| OcrError::ModelStart)?
            .len();
        tokio::fs::create_dir_all(&config.work_dir)
            .await
            .map_err(|_| OcrError::ModelStart)?;
        let reserve = RESERVED_HOST_BYTES
            .saturating_add(model_bytes)
            .saturating_add(mmproj_bytes)
            .saturating_add(256 * 1024 * 1024);
        if host_memory().map_err(|_| OcrError::Memory)?.available_bytes < reserve {
            return Err(OcrError::Memory);
        }
        let lease =
            ModelLease::acquire(&config.lease_path, reserve).map_err(|_| OcrError::ModelStart)?;
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|_| OcrError::ModelStart)?;
        let port = listener
            .local_addr()
            .map_err(|_| OcrError::ModelStart)?
            .port();
        drop(listener);
        let endpoint = format!("http://127.0.0.1:{port}");
        let log_path = config.work_dir.join("llama-ocr.log");
        let log = File::create(&log_path).map_err(|_| OcrError::ModelStart)?;
        let threads = config.threads.clamp(1, 16).to_string();
        let context = config.context_size.clamp(1024, 8192).to_string();
        let mut command = Command::new(&config.binary);
        command
            .arg("--model")
            .arg(&config.model)
            .arg("--mmproj")
            .arg(&config.mmproj)
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .arg("--ctx-size")
            .arg(&context)
            .arg("--parallel")
            .arg("1")
            .arg("--threads")
            .arg(&threads)
            .arg("--threads-batch")
            .arg(&threads)
            .arg("--gpu-layers")
            .arg("0")
            .arg("--no-warmup")
            .arg("--no-webui")
            .env("OMP_NUM_THREADS", &threads)
            .stdin(Stdio::null())
            .stdout(log)
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|_| OcrError::ModelStart)?;
        let model_pid = child.id().ok_or(OcrError::ModelStart)?;
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(5))
            .no_proxy()
            .build()
            .map_err(|_| OcrError::ModelStart)?;
        let started = Instant::now();
        loop {
            if child
                .try_wait()
                .map_err(|_| OcrError::ModelStart)?
                .is_some()
            {
                return Err(OcrError::ModelStart);
            }
            if started.elapsed() > MODEL_START_TIMEOUT {
                return Err(OcrError::ModelTimeout);
            }
            if let Ok(response) = client.get(format!("{endpoint}/health")).send().await
                && response.status().is_success()
            {
                break;
            }
            sleep(Duration::from_millis(250)).await;
        }
        Ok(Self {
            child,
            endpoint,
            model_pid,
            _lease: lease,
            log_path,
        })
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn log_path(&self) -> &Path {
        &self.log_path
    }

    pub fn client_config(&self, model: String) -> OcrClientConfig {
        let mut config = OcrClientConfig::external(self.endpoint.clone(), model);
        config.model_pid = Some(self.model_pid);
        config
    }

    /// Stop the model and wait briefly for it to release memory.
    pub async fn stop(mut self) {
        let _ = self.child.start_kill();
        let _ = tokio::time::timeout(Duration::from_secs(10), self.child.wait()).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_wrappers_but_keeps_critical_text() {
        let raw = "```text\nPhotosynthesis converts light energy.\n\\(E = mc^2\\)\nThe yield is 42%, not 24%.\n```\n";
        let text = normalize_ocr_text(raw);
        assert!(text.contains("E = mc^2"));
        assert!(text.contains("42%, not 24%"));
        assert!(!text.contains("```"));
        assert!(!text.contains("\\("));
    }
}
