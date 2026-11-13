//! Linux helper containment (A-05). No shell, unbounded output, or silent
//! fallback to an address-space limit in place of cgroup-v2 memory accounting.

use std::{
    fs::{self, File, OpenOptions},
    io,
    os::{fd::AsRawFd, unix::fs::MetadataExt},
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::{Child, Command},
    sync::oneshot,
    task::JoinHandle,
    time::{sleep, timeout},
};
use uuid::Uuid;

pub const CGROUP_ROOT_ENV: &str = "CAMPUSLINGO_HELPER_CGROUP_ROOT";
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);

/// Internal helper budgets, not model admission estimates. RLIMIT_AS is only a
/// generous backstop; memory.max accounts resident memory across the whole tree.
#[derive(Debug, Clone, Copy)]
pub struct HelperLimits {
    pub wall_time: Duration,
    pub max_output_bytes: usize,
    pub memory_bytes: u64,
    pub cpu_seconds: u64,
    pub file_bytes: u64,
    pub address_space_bytes: u64,
}

impl HelperLimits {
    pub fn pdf(wall_time: Duration, max_output_bytes: usize) -> Self {
        Self {
            wall_time,
            max_output_bytes,
            memory_bytes: 512 * 1024 * 1024,
            cpu_seconds: 60,
            file_bytes: 64 * 1024 * 1024,
            address_space_bytes: 8 * 1024 * 1024 * 1024,
        }
    }

    fn validate(self) -> Result<Self, HelperError> {
        if self.wall_time.is_zero()
            || self.max_output_bytes == 0
            || self.max_output_bytes.checked_add(1).is_none()
            || self.memory_bytes == 0
            || self.cpu_seconds == 0
            || self.file_bytes == 0
            || self.address_space_bytes < self.memory_bytes
        {
            return Err(HelperError::InvalidLimits);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, Error)]
pub enum HelperError {
    #[error("helper resource limits are invalid")]
    InvalidLimits,
    #[error(
        "helper containment requires a delegated cgroup-v2 root (CAMPUSLINGO_HELPER_CGROUP_ROOT)"
    )]
    ContainmentUnavailable,
    #[error("helper could not be started or its pipes could not be read")]
    Io,
    #[error("helper exceeded its wall-clock timeout")]
    Timeout,
    #[error("helper output exceeded the per-stream byte limit")]
    OutputOverflow,
    #[error("resident worker exceeded its memory or host-headroom budget")]
    Memory,
    #[error("helper tree could not be reaped or its memory release verified")]
    Cleanup,
}

#[derive(Debug)]
pub struct HelperOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// cgroup accounting includes the whole tree and charged page cache; it is
    /// deliberately not labeled process RSS or added to unified GPU memory.
    pub usage: Option<HelperUsage>,
}

#[derive(Debug, serde::Serialize)]
pub struct HelperUsage {
    pub memory_peak_bytes: u64,
    pub oom_kill_count: u64,
    /// Peak `VmHWM` sampled by the resident watchdog. `None` means the helper
    /// was short-lived and this was not sampled.
    pub process_peak_rss_bytes: Option<u64>,
    /// Minimum host `MemAvailable` sampled by the resident watchdog. `None`
    /// means the helper was short-lived and this was not sampled.
    pub minimum_host_available_bytes: Option<u64>,
}

/// A delegated *empty* parent cgroup must be provisioned by the service manager.
/// Never enable controllers in somebody else's cgroup or move the API process.
struct HelperCgroup {
    path: PathBuf,
    procs: File,
}

impl HelperCgroup {
    fn create(root: &Path, memory_bytes: u64) -> Result<Self, HelperError> {
        let setup = || -> io::Result<Self> {
            let root = root.canonicalize()?;
            // An ordinary directory with lookalike control files is not a limit.
            if !root.starts_with("/sys/fs/cgroup") {
                return Err(io::Error::other("not a cgroup-v2 mount"));
            }
            let metadata = fs::metadata(&root)?;
            if metadata.dev() != fs::metadata("/sys/fs/cgroup")?.dev()
                || !fs::read_to_string(root.join("cgroup.controllers"))?
                    .split_whitespace()
                    .any(|controller| controller == "memory")
                || !fs::read_to_string(root.join("cgroup.subtree_control"))?
                    .split_whitespace()
                    .any(|controller| controller == "memory")
            {
                return Err(io::Error::other("memory controller not delegated"));
            }
            let path = root.join(format!("helper-{}", Uuid::new_v4()));
            fs::create_dir(&path)?;
            let result = (|| {
                fs::write(path.join("memory.max"), memory_bytes.to_string())?;
                fs::write(path.join("memory.swap.max"), "0")?;
                fs::write(path.join("memory.oom.group"), "1")?;
                // Both controls are mandatory; no weaker process-only fallback.
                OpenOptions::new()
                    .write(true)
                    .open(path.join("cgroup.kill"))?;
                let procs = OpenOptions::new()
                    .write(true)
                    .open(path.join("cgroup.procs"))?;
                Ok(Self {
                    path: path.clone(),
                    procs,
                })
            })();
            if result.is_err() {
                let _ = fs::remove_dir(&path);
            }
            result
        };
        setup().map_err(|_| HelperError::ContainmentUnavailable)
    }

    fn kill(&self) {
        let _ = fs::write(self.path.join("cgroup.kill"), "1");
    }

    async fn release(&self) -> Result<HelperUsage, HelperError> {
        let released = async {
            loop {
                let events = fs::read_to_string(self.path.join("cgroup.events"))
                    .map_err(|_| HelperError::Cleanup)?;
                // A cgroup can retain reclaimable page-cache charges after exit.
                // Require no tasks, remove it, then verify the kernel removed the
                // accounting directory rather than inferring release from RSS.
                if events.lines().any(|line| line == "populated 0") {
                    let memory_peak_bytes = fs::read_to_string(self.path.join("memory.peak"))
                        .map_err(|_| HelperError::Cleanup)?
                        .trim()
                        .parse::<u64>()
                        .map_err(|_| HelperError::Cleanup)?;
                    let events = fs::read_to_string(self.path.join("memory.events"))
                        .map_err(|_| HelperError::Cleanup)?;
                    let oom_kill_count = events
                        .lines()
                        .find_map(|line| line.strip_prefix("oom_kill ")?.parse::<u64>().ok())
                        .ok_or(HelperError::Cleanup)?;
                    fs::remove_dir(&self.path).map_err(|_| HelperError::Cleanup)?;
                    return if self.path.exists() {
                        Err(HelperError::Cleanup)
                    } else {
                        Ok(HelperUsage {
                            memory_peak_bytes,
                            oom_kill_count,
                            process_peak_rss_bytes: None,
                            minimum_host_available_bytes: None,
                        })
                    };
                }
                sleep(Duration::from_millis(10)).await;
            }
        };
        timeout(CLEANUP_TIMEOUT, released)
            .await
            .map_err(|_| HelperError::Cleanup)?
    }
}

impl Drop for HelperCgroup {
    fn drop(&mut self) {
        self.kill();
        let _ = fs::remove_dir(&self.path);
    }
}

struct OwnedHelper {
    child: Option<Child>,
    pgid: i32,
    cgroup: Option<HelperCgroup>,
    // A model lease must survive cancellation until tree exit/accounting
    // removal is verified, not merely until the caller drops its handle.
    release_guard: Option<Box<dyn Send>>,
}

impl OwnedHelper {
    fn kill_tree(&self) {
        if let Some(cgroup) = &self.cgroup {
            // Includes descendants that tried to escape their process group.
            cgroup.kill();
        }
        if self.pgid > 0 {
            // SAFETY: pgid is the positive PID of our own process-group leader.
            unsafe { libc::kill(-self.pgid, libc::SIGKILL) };
        }
    }

    async fn finish(&mut self) -> Result<Option<HelperUsage>, HelperError> {
        self.kill_tree();
        self.pgid = 0;
        // Keep resources in the guard across awaits: cancellation during
        // cleanup itself must still reach the Drop kill/reap path.
        if let Some(child) = self.child.as_mut() {
            timeout(CLEANUP_TIMEOUT, child.wait())
                .await
                .map_err(|_| HelperError::Cleanup)?
                .map_err(|_| HelperError::Cleanup)?;
        }
        self.child.take();
        let usage = match self.cgroup.as_ref() {
            Some(cgroup) => Some(cgroup.release().await?),
            None => None,
        };
        self.cgroup.take();
        self.release_guard.take();
        Ok(usage)
    }
}

impl Drop for OwnedHelper {
    fn drop(&mut self) {
        if self.child.is_none() && self.cgroup.is_none() {
            return;
        }
        // Cancellation cannot leave an executing parser behind. Tokio's child
        // reaper is the fallback during runtime teardown; normally the cleanup
        // task below also verifies empty cgroup accounting and removes it.
        self.kill_tree();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let mut cleanup = Self {
                child: self.child.take(),
                pgid: self.pgid,
                cgroup: self.cgroup.take(),
                release_guard: self.release_guard.take(),
            };
            self.pgid = 0;
            runtime.spawn(async move {
                if cleanup.finish().await.is_err()
                    && let Some(guard) = cleanup.release_guard.take()
                {
                    // Fail closed: retain the lock for this process's lifetime
                    // rather than authorize a second model after failed cleanup.
                    std::mem::forget(guard);
                }
                // On an OS cleanup failure, fall back to synchronous Drop and
                // Tokio's child reaper, not an unbounded chain of cleanup tasks.
                cleanup.child.take();
                cleanup.cgroup.take();
            });
        } else if let Some(guard) = self.release_guard.take() {
            // Runtime teardown cannot prove asynchronous release. Keep the
            // lease locked until OS process exit rather than allow overlap.
            std::mem::forget(guard);
        }
    }
}

async fn drain(pipe: impl AsyncRead + Unpin, max: usize) -> Result<Vec<u8>, HelperError> {
    let mut bytes = Vec::new();
    pipe.take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| HelperError::Io)?;
    if bytes.len() > max {
        Err(HelperError::OutputOverflow)
    } else {
        Ok(bytes)
    }
}

/// Run a local helper with mandatory cgroup-v2 resident-memory enforcement.
/// stdout and stderr each have their own MAX_BYTES+1 overflow detector. Neither
/// captured output nor paths/arguments are included in error messages.
pub async fn run_helper(
    command: &mut Command,
    limits: HelperLimits,
) -> Result<HelperOutput, HelperError> {
    let limits = limits.validate()?;
    let root = std::env::var_os(CGROUP_ROOT_ENV)
        .map(PathBuf::from)
        .ok_or(HelperError::ContainmentUnavailable)?;
    run_in_root(command, limits, &root).await
}

async fn run_in_root(
    command: &mut Command,
    limits: HelperLimits,
    root: &Path,
) -> Result<HelperOutput, HelperError> {
    let limits = limits.validate()?;
    let cgroup = HelperCgroup::create(root, limits.memory_bytes)?;
    run_contained(command, limits, Some(cgroup)).await
}

async fn run_contained(
    command: &mut Command,
    limits: HelperLimits,
    cgroup: Option<HelperCgroup>,
) -> Result<HelperOutput, HelperError> {
    let mut owned = spawn_owned(command, limits, cgroup, None)?;
    let child = owned.child.as_mut().ok_or(HelperError::Io)?;
    let stdout = child.stdout.take().ok_or(HelperError::Io)?;
    let stderr = child.stderr.take().ok_or(HelperError::Io)?;
    let workload = async {
        let pipes = async {
            tokio::try_join!(
                drain(stdout, limits.max_output_bytes),
                drain(stderr, limits.max_output_bytes)
            )
        };
        let wait = async { child.wait().await.map_err(|_| HelperError::Io) };
        let ((stdout, stderr), status) = tokio::try_join!(pipes, wait)?;
        Ok(HelperOutput {
            status,
            stdout,
            stderr,
            usage: None,
        })
    };
    let result = timeout(limits.wall_time, workload)
        .await
        .unwrap_or(Err(HelperError::Timeout));
    let usage = owned.finish().await?;
    result.map(|mut output| {
        output.usage = usage;
        output
    })
}

fn spawn_owned(
    command: &mut Command,
    limits: HelperLimits,
    cgroup: Option<HelperCgroup>,
    release_guard: Option<Box<dyn Send>>,
) -> Result<OwnedHelper, HelperError> {
    let limits = limits.validate()?;
    // Keep the descriptor alive in the command's pre_exec closure too. A caller
    // reusing its Command must never write through a stale, recycled raw fd.
    let procs = cgroup
        .as_ref()
        .map(|group| group.procs.try_clone())
        .transpose()
        .map_err(|_| HelperError::Io)?;
    command
        .env("LC_ALL", "C.UTF-8")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .process_group(0);
    // SAFETY: only async-signal-safe libc calls and stack values are used after
    // fork. The already-open cgroup descriptor remains alive through spawn.
    unsafe {
        command.pre_exec(move || {
            for (resource, value) in [
                (libc::RLIMIT_CPU, limits.cpu_seconds),
                (libc::RLIMIT_FSIZE, limits.file_bytes),
                (libc::RLIMIT_AS, limits.address_space_bytes),
                (libc::RLIMIT_CORE, 0),
            ] {
                let bound = libc::rlimit {
                    rlim_cur: value as libc::rlim_t,
                    rlim_max: value as libc::rlim_t,
                };
                if libc::setrlimit(resource, &bound) != 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            if let Some(procs) = &procs
                && libc::write(procs.as_raw_fd(), b"0".as_ptr().cast(), 1) != 1
            {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = command.spawn().map_err(|_| HelperError::Io)?;
    let pgid = child.id().ok_or(HelperError::Io)? as i32;
    Ok(OwnedHelper {
        child: Some(child),
        pgid,
        cgroup,
        release_guard,
    })
}

pub struct ResidentOutput {
    pub output: Option<HelperOutput>,
    pub usage: HelperUsage,
    pub failure: Option<HelperError>,
}

/// Controller for a contained long-lived local worker. Its supervisor task owns
/// the child, cgroup and release guard even if this controller is cancelled.
pub struct ResidentProcess {
    pid: u32,
    shutdown: Option<oneshot::Sender<()>>,
    supervisor: Option<JoinHandle<Result<ResidentOutput, HelperError>>>,
}

impl ResidentProcess {
    pub fn start(
        command: &mut Command,
        limits: HelperLimits,
        minimum_host_bytes: u64,
        release_guard: impl Send + 'static,
    ) -> Result<Self, HelperError> {
        let limits = limits.validate()?;
        let root = std::env::var_os(CGROUP_ROOT_ENV)
            .map(PathBuf::from)
            .ok_or(HelperError::ContainmentUnavailable)?;
        let cgroup = HelperCgroup::create(&root, limits.memory_bytes)?;
        let kill_path = cgroup.path.join("cgroup.kill");
        let mut owned = spawn_owned(command, limits, Some(cgroup), Some(Box::new(release_guard)))?;
        let pid = owned
            .child
            .as_ref()
            .and_then(Child::id)
            .ok_or(HelperError::Io)?;
        let (shutdown, mut stopped) = oneshot::channel();
        let minimum_host_available = std::sync::Arc::new(AtomicU64::new(u64::MAX));
        let watchdog_minimum = minimum_host_available.clone();
        let process_peak_rss = std::sync::Arc::new(AtomicU64::new(0));
        let watchdog_peak = process_peak_rss.clone();
        let supervisor = tokio::spawn(async move {
            let (result, failure) = {
                let child = owned.child.as_mut().ok_or(HelperError::Io)?;
                let stdout = child.stdout.take().ok_or(HelperError::Io)?;
                let stderr = child.stderr.take().ok_or(HelperError::Io)?;
                let workload = async {
                    let pipes = async {
                        tokio::try_join!(
                            drain(stdout, limits.max_output_bytes),
                            drain(stderr, limits.max_output_bytes)
                        )
                    };
                    let wait = async { child.wait().await.map_err(|_| HelperError::Io) };
                    let ((stdout, stderr), status) = tokio::try_join!(pipes, wait)?;
                    Ok(HelperOutput {
                        status,
                        stdout,
                        stderr,
                        usage: None,
                    })
                };
                tokio::pin!(workload);
                let watchdog = async {
                    loop {
                        match crate::host_memory() {
                            Ok(memory) => {
                                watchdog_minimum
                                    .fetch_min(memory.available_bytes, Ordering::Relaxed);
                                if memory.available_bytes < minimum_host_bytes {
                                    return HelperError::Memory;
                                }
                            }
                            Err(_) => return HelperError::Memory,
                        }
                        if let Ok(peak) = crate::process_peak_rss(pid) {
                            watchdog_peak.fetch_max(peak, Ordering::Relaxed);
                        }
                        if let Ok(rss) = crate::process_rss(pid) {
                            watchdog_peak.fetch_max(rss, Ordering::Relaxed);
                            if rss > limits.memory_bytes {
                                return HelperError::Memory;
                            }
                        }
                        sleep(Duration::from_millis(100)).await;
                    }
                };
                let completion = tokio::select! {
                    result = &mut workload => Ok(result),
                    _ = &mut stopped => Err(None),
                    _ = sleep(limits.wall_time) => Err(Some(HelperError::Timeout)),
                    error = watchdog => Err(Some(error)),
                };
                match completion {
                    Ok(result) => {
                        let failure =
                            Some(result.as_ref().err().copied().unwrap_or(HelperError::Io));
                        (result, failure)
                    }
                    Err(reason) => {
                        let _ = fs::write(&kill_path, "1");
                        let result = timeout(CLEANUP_TIMEOUT, &mut workload)
                            .await
                            .unwrap_or(Err(HelperError::Cleanup));
                        (result, reason)
                    }
                }
            };
            let minimum = minimum_host_available.load(Ordering::Relaxed);
            let peak_rss = process_peak_rss.load(Ordering::Relaxed);
            finish_resident(result, failure, owned, minimum, peak_rss).await
        });
        Ok(Self {
            pid,
            shutdown: Some(shutdown),
            supervisor: Some(supervisor),
        })
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn has_exited(&self) -> bool {
        self.supervisor.as_ref().is_none_or(JoinHandle::is_finished)
    }

    pub async fn stop(mut self) -> Result<ResidentOutput, HelperError> {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        self.supervisor
            .take()
            .ok_or(HelperError::Cleanup)?
            .await
            .map_err(|_| HelperError::Cleanup)?
    }
}

async fn finish_resident(
    result: Result<HelperOutput, HelperError>,
    failure: Option<HelperError>,
    mut owned: OwnedHelper,
    minimum_host_available: u64,
    process_peak_rss: u64,
) -> Result<ResidentOutput, HelperError> {
    let mut usage = owned.finish().await?.ok_or(HelperError::Cleanup)?;
    if process_peak_rss > 0 {
        usage.process_peak_rss_bytes = Some(process_peak_rss);
    }
    if minimum_host_available != u64::MAX {
        usage.minimum_host_available_bytes = Some(minimum_host_available);
    }
    let failure = if usage.oom_kill_count > 0 {
        Some(HelperError::Memory)
    } else {
        failure.or_else(|| result.as_ref().err().copied())
    };
    Ok(ResidentOutput {
        output: result.ok(),
        usage,
        failure,
    })
}

impl Drop for ResidentProcess {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        // Detached supervisor still owns the release guard until cleanup ends.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ReleaseFlag(std::sync::Arc<std::sync::atomic::AtomicBool>);
    impl Drop for ReleaseFlag {
        fn drop(&mut self) {
            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[tokio::test]
    #[ignore = "requires delegated memory controller"]
    async fn resident_stop_reaps_tree_before_releasing_guard() {
        let released = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let service = ResidentProcess::start(
            Command::new("/bin/sh")
                .arg("-c")
                .arg("echo ready; sleep 30 & wait"),
            limits(),
            0,
            ReleaseFlag(released.clone()),
        )
        .unwrap();
        let pid = service.pid();
        sleep(Duration::from_millis(50)).await;
        assert!(!released.load(std::sync::atomic::Ordering::SeqCst));
        let result = service.stop().await.unwrap();
        assert!(result.failure.is_none());
        assert_eq!(result.output.unwrap().stdout, b"ready\n");
        assert!(result.usage.memory_peak_bytes > 0);
        assert!(!PathBuf::from(format!("/proc/{pid}")).exists());
        assert!(released.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    #[ignore = "requires delegated memory controller"]
    async fn resident_overflow_and_watchdog_fail_closed() {
        for (script, reserve, expected) in [
            (
                "head -c 129 /dev/zero; sleep 30",
                0,
                HelperError::OutputOverflow,
            ),
            ("sleep 30", u64::MAX, HelperError::Memory),
        ] {
            let mut limits = limits();
            limits.max_output_bytes = 128;
            let service = ResidentProcess::start(
                Command::new("/bin/sh").arg("-c").arg(script),
                limits,
                reserve,
                (),
            )
            .unwrap();
            timeout(Duration::from_secs(3), async {
                while !service.has_exited() {
                    sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            let result = service.stop().await.unwrap();
            assert_eq!(
                std::mem::discriminant(&result.failure.unwrap()),
                std::mem::discriminant(&expected)
            );
        }
    }

    #[tokio::test]
    #[ignore = "requires delegated memory controller"]
    async fn resident_drop_keeps_guard_until_escaped_descendants_are_stopped() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("escaped.pid");
        let released = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let service = ResidentProcess::start(
            Command::new("/bin/sh")
                .arg("-c")
                .arg("setsid /bin/sh -c 'echo $$ > \"$DESCENDANT\"; exec sleep 30' & wait")
                .env("DESCENDANT", &path),
            limits(),
            0,
            ReleaseFlag(released.clone()),
        )
        .unwrap();
        let pid = read_pid(&path).await;
        drop(service);
        assert_stopped(pid).await;
        timeout(Duration::from_secs(3), async {
            while !released.load(std::sync::atomic::Ordering::SeqCst) {
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }

    fn limits() -> HelperLimits {
        HelperLimits::pdf(Duration::from_secs(3), 128 * 1024)
    }

    async fn shell(script: &str, limits: HelperLimits) -> Result<HelperOutput, HelperError> {
        // Test-only non-cgroup path exercises pipes, groups and rlimits without
        // requiring service-manager delegation. Production has no bypass.
        run_contained(Command::new("/bin/sh").arg("-c").arg(script), limits, None).await
    }

    #[tokio::test]
    async fn drains_both_pipes_concurrently() {
        let output = shell(
            "head -c 100000 /dev/zero & head -c 100000 /dev/zero >&2 & wait",
            limits(),
        )
        .await
        .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout.len(), 100000);
        assert_eq!(output.stderr.len(), 100000);
    }

    #[tokio::test]
    async fn exact_limit_is_allowed_and_either_stream_overflow_fails_closed() {
        let mut limits = limits();
        limits.max_output_bytes = 4096;
        assert_eq!(
            shell("head -c 4096 /dev/zero", limits)
                .await
                .unwrap()
                .stdout
                .len(),
            4096
        );
        for script in [
            "head -c 4097 /dev/zero; sleep 30",
            "head -c 4097 /dev/zero >&2; sleep 30",
        ] {
            let started = std::time::Instant::now();
            assert!(matches!(
                shell(script, limits).await,
                Err(HelperError::OutputOverflow)
            ));
            assert!(started.elapsed() < Duration::from_secs(2));
        }
    }

    #[tokio::test]
    async fn timeout_kills_the_group_and_reaps_the_leader() {
        let mut limits = limits();
        limits.wall_time = Duration::from_millis(100);
        assert!(matches!(
            shell("sleep 30 & wait", limits).await,
            Err(HelperError::Timeout)
        ));
    }

    async fn read_pid(path: &Path) -> u32 {
        timeout(Duration::from_secs(3), async {
            loop {
                if let Ok(text) = fs::read_to_string(path)
                    && let Ok(pid) = text.trim().parse()
                {
                    break pid;
                }
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("helper wrote PID")
    }

    async fn assert_stopped(pid: u32) {
        timeout(Duration::from_secs(3), async {
            loop {
                match fs::read_to_string(format!("/proc/{pid}/stat")) {
                    Err(error) if error.kind() == io::ErrorKind::NotFound => break,
                    Ok(stat)
                        if stat
                            .rsplit_once(") ")
                            .is_some_and(|(_, rest)| rest.starts_with('Z')) =>
                    {
                        break;
                    }
                    _ => sleep(Duration::from_millis(10)).await,
                }
            }
        })
        .await
        .expect("descendant no longer executing");
    }

    #[tokio::test]
    async fn cancellation_kills_descendants_and_reaps_direct_child() {
        let directory = tempfile::tempdir().unwrap();
        let pid_path = directory.path().join("descendant.pid");
        let leader_path = directory.path().join("leader.pid");
        let task_pid_path = pid_path.clone();
        let task_leader_path = leader_path.clone();
        let task = tokio::spawn(async move {
            run_contained(
                Command::new("/bin/sh")
                    .arg("-c")
                    .arg("echo $$ > \"$LEADER\"; sleep 30 & echo $! > \"$DESCENDANT\"; wait")
                    .env("DESCENDANT", task_pid_path)
                    .env("LEADER", task_leader_path),
                limits(),
                None,
            )
            .await
        });
        let descendant = read_pid(&pid_path).await;
        let leader = read_pid(&leader_path).await;
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert_stopped(descendant).await;
        timeout(Duration::from_secs(3), async {
            while PathBuf::from(format!("/proc/{leader}")).exists() {
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("leader reaped, not merely left as zombie");
    }

    #[tokio::test]
    async fn successful_leader_exit_does_not_leave_detached_pipe_descendants() {
        let directory = tempfile::tempdir().unwrap();
        let pid_path = directory.path().join("descendant.pid");
        let output = run_contained(
            Command::new("/bin/sh")
                .arg("-c")
                .arg("sleep 30 >/dev/null 2>&1 & echo $! > \"$DESCENDANT\"")
                .env("DESCENDANT", &pid_path),
            limits(),
            None,
        )
        .await
        .unwrap();
        assert!(output.status.success());
        assert_stopped(read_pid(&pid_path).await).await;
    }

    #[tokio::test]
    async fn file_limit_is_enforced_before_exec() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("large");
        let mut limits = limits();
        limits.file_bytes = 4096;
        let output = run_contained(
            Command::new("/bin/dd")
                .arg("if=/dev/zero")
                .arg(format!("of={}", path.display()))
                .arg("bs=8192")
                .arg("count=1"),
            limits,
            None,
        )
        .await
        .unwrap();
        assert!(!output.status.success());
        assert!(fs::metadata(path).unwrap().len() <= 4096);
    }

    #[tokio::test]
    async fn fake_cgroup_directory_is_rejected_before_execution() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("executed");
        let result = run_in_root(
            Command::new("/usr/bin/touch").arg(&marker),
            limits(),
            directory.path(),
        )
        .await;
        assert!(matches!(result, Err(HelperError::ContainmentUnavailable)));
        assert!(!marker.exists());
    }

    #[test]
    fn rejects_invalid_resource_budgets() {
        let mut limits = limits();
        limits.max_output_bytes = usize::MAX;
        assert!(matches!(limits.validate(), Err(HelperError::InvalidLimits)));
    }

    #[tokio::test]
    #[ignore = "requires CAMPUSLINGO_HELPER_CGROUP_ROOT with delegated memory controller"]
    async fn delegated_cgroup_is_attached_before_exec_and_removed_after_exit() {
        let root = PathBuf::from(std::env::var_os(CGROUP_ROOT_ENV).expect("delegated cgroup root"));
        let output = run_helper(Command::new("/bin/cat").arg("/proc/self/cgroup"), limits())
            .await
            .unwrap();
        assert!(output.status.success());
        let membership = String::from_utf8(output.stdout).unwrap();
        assert!(membership.contains("/helper-"));
        assert!(output.usage.unwrap().memory_peak_bytes > 0);
        assert!(fs::read_dir(root).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("helper-")
        }));
    }

    #[tokio::test]
    #[ignore = "requires CAMPUSLINGO_HELPER_CGROUP_ROOT with delegated memory controller"]
    async fn delegated_cgroup_enforces_resident_memory_not_just_virtual_address_space() {
        let mut limits = limits();
        limits.memory_bytes = 32 * 1024 * 1024;
        let output = run_helper(
            Command::new("/bin/bash")
                .arg("-c")
                .arg("printf -v payload '%*s' 134217728 x"),
            limits,
        )
        .await
        .unwrap();
        assert!(!output.status.success());
        assert!(
            output.usage.unwrap().oom_kill_count > 0,
            "kernel, not a mock, enforced memory.max"
        );
    }

    #[tokio::test]
    #[ignore = "requires CAMPUSLINGO_HELPER_CGROUP_ROOT with delegated memory controller"]
    async fn delegated_cgroup_kills_descendants_that_escape_the_process_group() {
        let directory = tempfile::tempdir().unwrap();
        let pid_path = directory.path().join("escaped.pid");
        let mut limits = limits();
        limits.wall_time = Duration::from_millis(500);
        let result = run_helper(
            Command::new("/bin/sh")
                .arg("-c")
                .arg("setsid /bin/sh -c 'echo $$ > \"$DESCENDANT\"; exec sleep 30' & wait")
                .env("DESCENDANT", &pid_path),
            limits,
        )
        .await;
        assert!(matches!(result, Err(HelperError::Timeout)));
        assert_stopped(read_pid(&pid_path).await).await;
    }

    #[tokio::test]
    #[ignore = "requires CAMPUSLINGO_HELPER_CGROUP_ROOT with delegated memory controller"]
    async fn delegated_cgroup_cancellation_cleans_up_accounting_and_escaped_descendants() {
        let root = PathBuf::from(std::env::var_os(CGROUP_ROOT_ENV).expect("delegated cgroup root"));
        let directory = tempfile::tempdir().unwrap();
        let pid_path = directory.path().join("escaped.pid");
        let task_pid_path = pid_path.clone();
        let task = tokio::spawn(async move {
            run_helper(
                Command::new("/bin/sh")
                    .arg("-c")
                    .arg("setsid /bin/sh -c 'echo $$ > \"$DESCENDANT\"; exec sleep 30' & wait")
                    .env("DESCENDANT", task_pid_path),
                limits(),
            )
            .await
        });
        let descendant = read_pid(&pid_path).await;
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert_stopped(descendant).await;
        timeout(Duration::from_secs(3), async {
            loop {
                if fs::read_dir(&root).unwrap().all(|entry| {
                    !entry
                        .unwrap()
                        .file_name()
                        .to_string_lossy()
                        .starts_with("helper-")
                }) {
                    break;
                }
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("cancelled helper's cgroup removed");
    }
}
