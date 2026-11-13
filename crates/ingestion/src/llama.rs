//! Shared contained `llama-server` lifecycle for local OCR, embedding, and
//! two-pass generation stages under single-model residency (`ModelLease`).

use std::{
    fs::OpenOptions,
    io::Write,
    net::TcpListener,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use serde::Serialize;
use thiserror::Error;
use tokio::{
    process::Command,
    sync::{Mutex, OwnedRwLockReadGuard, OwnedSemaphorePermit, RwLock, Semaphore},
    time::{Instant, sleep},
};

use crate::{
    ModelLease, RESERVED_HOST_BYTES, host_memory,
    process::{HelperError, HelperLimits, ResidentProcess},
    process_rss,
};

pub const MAX_MODEL_RSS_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const MODEL_START_TIMEOUT: Duration = Duration::from_secs(120);

/// Default inference thread count sized to host compute parallelism (clamped
/// safely to `llama-server`'s 1..=16 thread envelope).
pub fn default_compute_threads() -> u32 {
    std::thread::available_parallelism()
        .map_or(8, |n| n.get().clamp(4, 16))
        .try_into()
        .unwrap_or(8)
}

#[derive(Debug, Error)]
pub enum LlamaError {
    #[error("model inference requires a literal loopback HTTP(S) endpoint")]
    NonLocalEndpoint,
    #[error("the local model is not configured")]
    NotConfigured,
    #[error("the local model could not be started")]
    ModelStart,
    #[error("the local model did not become ready in time")]
    ModelTimeout,
    #[error("the local model exceeded its memory budget")]
    Memory,
    #[error("the local model request failed")]
    Transport,
    #[error("the local model endpoint returned HTTP {0}")]
    Http(u16),
    #[error("the local model response was malformed")]
    InvalidResponse,
    #[error("the local model response was empty")]
    Empty,
    #[error("the local model request timed out")]
    Timeout,
    #[error("the local model tree or its memory release could not be verified")]
    Release,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelRelease {
    pub model_pid: u32,
    pub process_tree_reaped: bool,
    pub cgroup_removed: bool,
    pub cgroup_memory_peak_bytes: u64,
    pub cgroup_oom_kill_count: u64,
    pub process_peak_rss_bytes: Option<u64>,
    pub minimum_host_available_bytes: Option<u64>,
    pub termination: Option<String>,
    pub memory_budget_exceeded: bool,
}

#[derive(Debug, Clone)]
pub struct LlamaLaunchSpec {
    pub role: &'static str,
    pub binary: PathBuf,
    pub model: PathBuf,
    pub mmproj: Option<PathBuf>,
    pub work_dir: PathBuf,
    pub lease_path: PathBuf,
    pub threads: u32,
    pub context_size: u32,
    pub parallel_slots: u32,
    pub gpu_device: Option<String>,
    pub budget_bytes: u64,
    pub restart_after_requests: Option<usize>,
    pub extra_args: Vec<String>,
}

/// Parse and enforce literal loopback HTTP(S) endpoints without credentials,
/// paths, query strings, or fragments so no private derivative leaves localhost.
pub fn validate_loopback_endpoint(endpoint: &str) -> Result<reqwest::Url, LlamaError> {
    let parsed = reqwest::Url::parse(endpoint).map_err(|_| LlamaError::NonLocalEndpoint)?;
    let host = parsed
        .host_str()
        .ok_or(LlamaError::NonLocalEndpoint)?
        .trim_matches(['[', ']']);
    let local = host
        .parse::<std::net::IpAddr>()
        .is_ok_and(|ip| ip.is_loopback());
    if !local
        || !["http", "https"].contains(&parsed.scheme())
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || parsed.path() != "/"
    {
        return Err(LlamaError::NonLocalEndpoint);
    }
    Ok(parsed)
}

pub fn build_local_http_client(timeout: Duration) -> Result<reqwest::Client, LlamaError> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(timeout)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| LlamaError::ModelStart)
}

/// Verify the managed model RSS and available host RAM stay within safety limits.
pub fn check_model_memory(
    model_pid: Option<u32>,
    max_model_rss_bytes: u64,
) -> Result<(), LlamaError> {
    if let Some(pid) = model_pid {
        let rss = process_rss(pid).map_err(|_| LlamaError::Memory)?;
        if rss > max_model_rss_bytes {
            return Err(LlamaError::Memory);
        }
    }
    let available = host_memory()
        .map_err(|_| LlamaError::Memory)?
        .available_bytes;
    if available < RESERVED_HOST_BYTES {
        return Err(LlamaError::Memory);
    }
    Ok(())
}

/// Inspect `llama-server --list-devices` and select `Vulkan0` when present so
/// supported APU VRAM handles model weights without exhausting the worker's host
/// cgroup/RSS ceiling.
pub async fn detect_vulkan_device(binary: &Path) -> Option<String> {
    let output = Command::new(binary)
        .arg("--list-devices")
        .output()
        .await
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .any(|line| line.trim_start().starts_with("Vulkan0:"))
        .then(|| "Vulkan0".to_owned())
}

pub struct ManagedLlamaServer {
    process: ResidentProcess,
    endpoint: String,
    pub model_pid: u32,
    log_path: PathBuf,
    authorization: reqwest::header::HeaderValue,
}

impl ManagedLlamaServer {
    pub async fn start(spec: &LlamaLaunchSpec) -> Result<Self, LlamaError> {
        let model_bytes = std::fs::metadata(&spec.model)
            .map_err(|_| LlamaError::ModelStart)?
            .len();
        let mmproj_bytes = match &spec.mmproj {
            Some(path) => std::fs::metadata(path)
                .map_err(|_| LlamaError::ModelStart)?
                .len(),
            None => 0,
        };
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&spec.work_dir)
            .map_err(|_| LlamaError::ModelStart)?;
        let reserve = RESERVED_HOST_BYTES
            .saturating_add(model_bytes)
            .saturating_add(mmproj_bytes)
            .saturating_add(256 * 1024 * 1024);
        if host_memory()
            .map_err(|_| LlamaError::Memory)?
            .available_bytes
            < reserve
        {
            return Err(LlamaError::Memory);
        }
        let lease =
            ModelLease::acquire(&spec.lease_path, reserve).map_err(|_| LlamaError::ModelStart)?;
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|_| LlamaError::ModelStart)?;
        let port = listener
            .local_addr()
            .map_err(|_| LlamaError::ModelStart)?
            .port();
        drop(listener);
        let endpoint = format!("http://127.0.0.1:{port}");
        let log_path =
            spec.work_dir
                .join(format!("llama-{}-{}.log", spec.role, uuid::Uuid::new_v4()));
        let parallel_slots = spec.parallel_slots.clamp(1, 8);
        let threads = spec.threads.clamp(1, 16);
        let threads_str = threads.to_string();
        let threads_batch_str = (threads.saturating_mul(2)).clamp(threads, 16).to_string();
        let context = spec
            .context_size
            .clamp(512, 32768)
            .saturating_mul(parallel_slots)
            .to_string();
        let api_key = uuid::Uuid::new_v4().to_string();
        let mut authorization =
            reqwest::header::HeaderValue::from_str(&format!("Bearer {api_key}"))
                .map_err(|_| LlamaError::ModelStart)?;
        authorization.set_sensitive(true);

        let mut command = Command::new(&spec.binary);
        command.arg("--model").arg(&spec.model);
        if let Some(mmproj) = &spec.mmproj {
            command.arg("--mmproj").arg(mmproj);
        }
        command
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--api-key")
            .arg(&api_key)
            .arg("--port")
            .arg(port.to_string())
            .arg("--ctx-size")
            .arg(&context)
            .arg("--parallel")
            .arg(parallel_slots.to_string())
            .arg("--threads")
            .arg(&threads_str)
            .arg("--threads-batch")
            .arg(&threads_batch_str);

        if let Some(device) = &spec.gpu_device {
            let shader_cache = spec.work_dir.join("mesa-cache");
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&shader_cache)
                .map_err(|_| LlamaError::ModelStart)?;
            command
                .arg("--gpu-layers")
                .arg("all")
                .arg("--device")
                .arg(device)
                .arg("--fit")
                .arg("off")
                .arg("--flash-attn")
                .arg("on")
                .env("MESA_SHADER_CACHE_DIR", shader_cache);
        } else {
            command
                .arg("--gpu-layers")
                .arg("0")
                .arg("--device")
                .arg("none")
                .arg("--no-op-offload");
            if spec.mmproj.is_some() {
                command.arg("--no-mmproj-offload");
            }
        }
        command
            .arg("--no-warmup")
            .arg("--no-webui")
            .arg("--cache-ram")
            .arg("0")
            .arg("--no-cache-prompt")
            .arg("--no-cache-idle-slots")
            .arg("--ctx-checkpoints")
            .arg("0")
            .args(&spec.extra_args)
            .env("OMP_NUM_THREADS", &threads_batch_str);

        let limits = HelperLimits {
            memory_bytes: spec.budget_bytes.max(256 * 1024 * 1024),
            cpu_seconds: 24 * 60 * 60,
            ..HelperLimits::pdf(Duration::from_secs(24 * 60 * 60), 8 * 1024 * 1024)
        };
        let process = ResidentProcess::start(&mut command, limits, RESERVED_HOST_BYTES, lease)
            .map_err(|_| LlamaError::ModelStart)?;
        let model_pid = process.pid();
        let client = build_local_http_client(Duration::from_secs(5))?;
        let started = Instant::now();
        let server = Self {
            process,
            endpoint,
            model_pid,
            log_path,
            authorization,
        };
        loop {
            if server.process.has_exited() {
                let release = server.stop().await?;
                return Err(if release.memory_budget_exceeded {
                    LlamaError::Memory
                } else {
                    LlamaError::ModelStart
                });
            }
            if started.elapsed() > MODEL_START_TIMEOUT {
                server.stop().await?;
                return Err(LlamaError::ModelTimeout);
            }
            if let Ok(response) = client
                .get(format!("{}/health", server.endpoint))
                .header(reqwest::header::AUTHORIZATION, server.authorization.clone())
                .send()
                .await
                && response.status().is_success()
            {
                break;
            }
            sleep(Duration::from_millis(250)).await;
        }
        Ok(server)
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn log_path(&self) -> &Path {
        &self.log_path
    }

    pub fn authorization(&self) -> &reqwest::header::HeaderValue {
        &self.authorization
    }

    pub async fn stop(self) -> Result<ModelRelease, LlamaError> {
        let stopped = self.process.stop().await.map_err(|_| LlamaError::Release)?;
        let release = ModelRelease {
            model_pid: self.model_pid,
            process_tree_reaped: true,
            cgroup_removed: true,
            cgroup_memory_peak_bytes: stopped.usage.memory_peak_bytes,
            cgroup_oom_kill_count: stopped.usage.oom_kill_count,
            process_peak_rss_bytes: stopped.usage.process_peak_rss_bytes,
            minimum_host_available_bytes: stopped.usage.minimum_host_available_bytes,
            termination: stopped.failure.map(|error| error.to_string()),
            memory_budget_exceeded: stopped.usage.oom_kill_count > 0
                || matches!(stopped.failure, Some(HelperError::Memory)),
        };
        let mut log = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&self.log_path)
            .map_err(|_| LlamaError::Release)?;
        if let Some(output) = stopped.output {
            log.write_all(&output.stdout)
                .map_err(|_| LlamaError::Release)?;
            log.write_all(&output.stderr)
                .map_err(|_| LlamaError::Release)?;
        }
        let evidence = self.log_path.with_extension("release.json");
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(evidence)
            .map_err(|_| LlamaError::Release)?;
        file.write_all(&serde_json::to_vec_pretty(&release).map_err(|_| LlamaError::Release)?)
            .map_err(|_| LlamaError::Release)?;
        file.sync_all().map_err(|_| LlamaError::Release)?;
        Ok(release)
    }
}

struct RuntimeState {
    server: Option<ManagedLlamaServer>,
    last_release: Option<ModelRelease>,
}

pub struct ManagedLlamaLease {
    pub endpoint: String,
    pub authorization: reqwest::header::HeaderValue,
    pub model_pid: u32,
    _permit: OwnedSemaphorePermit,
    _inflight: OwnedRwLockReadGuard<()>,
}

/// Managed single-model server pool supporting optional periodic restarts under
/// strict slot/inflight serialization.
pub struct ManagedLlamaRuntime {
    spec: LlamaLaunchSpec,
    state: Mutex<RuntimeState>,
    slots: Arc<Semaphore>,
    inflight: Arc<RwLock<()>>,
    requests: AtomicUsize,
    restarts: AtomicUsize,
    failed: AtomicBool,
}

impl ManagedLlamaRuntime {
    pub async fn start(spec: LlamaLaunchSpec) -> Result<Arc<Self>, LlamaError> {
        let slot_count = spec.parallel_slots.clamp(1, 8) as usize;
        let server = ManagedLlamaServer::start(&spec).await?;
        Ok(Arc::new(Self {
            spec,
            state: Mutex::new(RuntimeState {
                server: Some(server),
                last_release: None,
            }),
            slots: Arc::new(Semaphore::new(slot_count)),
            inflight: Arc::new(RwLock::new(())),
            requests: AtomicUsize::new(0),
            restarts: AtomicUsize::new(0),
            failed: AtomicBool::new(false),
        }))
    }

    pub fn restarts(&self) -> usize {
        self.restarts.load(Ordering::SeqCst)
    }

    pub fn spec(&self) -> &LlamaLaunchSpec {
        &self.spec
    }

    pub async fn last_release(&self) -> Option<ModelRelease> {
        self.state.lock().await.last_release.clone()
    }

    async fn release_server(
        &self,
        state: &mut RuntimeState,
    ) -> Result<Option<ModelRelease>, LlamaError> {
        let Some(server) = state.server.take() else {
            return Ok(None);
        };
        let release = server.stop().await?;
        let failed = release.termination.is_some() || release.memory_budget_exceeded;
        state.last_release = Some(release.clone());
        if failed {
            self.failed.store(true, Ordering::SeqCst);
            return Err(LlamaError::Memory);
        }
        Ok(Some(release))
    }

    pub async fn acquire_request(&self) -> Result<ManagedLlamaLease, LlamaError> {
        let permit = self
            .slots
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| LlamaError::Release)?;
        loop {
            if self.failed.load(Ordering::SeqCst) {
                return Err(LlamaError::Memory);
            }
            let inflight = self.inflight.clone().read_owned().await;
            let restart_needed = {
                let mut state = self.state.lock().await;
                if state
                    .server
                    .as_ref()
                    .is_some_and(|server| server.process.has_exited())
                {
                    self.release_server(&mut state).await?;
                    return Err(LlamaError::ModelStart);
                }
                if state.server.is_none() {
                    return Err(if self.failed.load(Ordering::SeqCst) {
                        LlamaError::Memory
                    } else {
                        LlamaError::ModelStart
                    });
                }
                if self
                    .spec
                    .restart_after_requests
                    .is_some_and(|limit| self.requests.load(Ordering::SeqCst) >= limit)
                {
                    true
                } else {
                    self.requests.fetch_add(1, Ordering::SeqCst);
                    let server = state.server.as_ref().ok_or(LlamaError::ModelStart)?;
                    return Ok(ManagedLlamaLease {
                        endpoint: server.endpoint.clone(),
                        authorization: server.authorization.clone(),
                        model_pid: server.model_pid,
                        _permit: permit,
                        _inflight: inflight,
                    });
                }
            };
            if restart_needed {
                drop(inflight);
                let _exclusive = self.inflight.write().await;
                if self.failed.load(Ordering::SeqCst) {
                    return Err(LlamaError::Memory);
                }
                let mut state = self.state.lock().await;
                if state
                    .server
                    .as_ref()
                    .is_some_and(|server| server.process.has_exited())
                {
                    self.release_server(&mut state).await?;
                    return Err(LlamaError::ModelStart);
                }
                if state.server.is_some()
                    && self
                        .spec
                        .restart_after_requests
                        .is_some_and(|limit| self.requests.load(Ordering::SeqCst) >= limit)
                {
                    self.release_server(&mut state).await?;
                    let server = match ManagedLlamaServer::start(&self.spec).await {
                        Ok(server) => server,
                        Err(error) => {
                            if matches!(error, LlamaError::Memory) {
                                self.failed.store(true, Ordering::SeqCst);
                            }
                            return Err(error);
                        }
                    };
                    state.server = Some(server);
                    self.requests.store(0, Ordering::SeqCst);
                    self.restarts.fetch_add(1, Ordering::SeqCst);
                }
            }
        }
    }

    pub async fn stop(&self) -> Result<ModelRelease, LlamaError> {
        let _exclusive = self.inflight.write().await;
        let mut state = self.state.lock().await;
        if let Some(server) = state.server.take() {
            let release = server.stop().await?;
            state.last_release = Some(release.clone());
            if self.failed.load(Ordering::SeqCst) {
                return Err(LlamaError::Memory);
            }
            return Ok(release);
        }
        if self.failed.load(Ordering::SeqCst) {
            return Err(LlamaError::Memory);
        }
        state.last_release.clone().ok_or(LlamaError::Release)
    }
}
