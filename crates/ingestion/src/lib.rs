//! Production model lifecycle primitives. Python is not part of this worker.

pub mod calibration;
pub mod consolidation;
pub mod crops;
pub mod embeddings;
pub mod fingerprint;
pub mod generator;
pub mod llama;
pub mod ocr;
pub mod pdf;
pub mod pipeline;
pub mod process;
pub mod raster;
pub mod regions;
pub mod tts;
pub mod vectors;
pub mod worker;

use fs2::FileExt;
use serde::Serialize;
use std::{
    fs::{self, File, OpenOptions},
    io,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

pub const RESERVED_HOST_BYTES: u64 = 3 * 1024 * 1024 * 1024;
/// Frozen O-01 admission floor. Model stages may require additional headroom;
/// this floor is not a replacement for their separately measured budgets.
pub const INGESTION_ADMISSION_RESERVE_BYTES: u64 = 1536 * 1024 * 1024;

pub fn admits_ingestion(available_bytes: u64) -> bool {
    available_bytes >= INGESTION_ADMISSION_RESERVE_BYTES
}

#[derive(Debug, Serialize)]
pub struct HostMemory {
    pub available_bytes: u64,
    pub swap_free_bytes: u64,
}

pub fn host_memory() -> io::Result<HostMemory> {
    let text = fs::read_to_string("/proc/meminfo")?;
    let read = |field: &str| -> io::Result<u64> {
        text.lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                (name == field)
                    .then(|| {
                        value
                            .split_whitespace()
                            .next()?
                            .parse::<u64>()
                            .ok()?
                            .checked_mul(1024)
                    })
                    .flatten()
            })
            .ok_or_else(|| io::Error::other(format!("missing memory field {field}")))
    };
    Ok(HostMemory {
        available_bytes: read("MemAvailable")?,
        swap_free_bytes: read("SwapFree")?,
    })
}

pub fn process_rss(pid: u32) -> io::Result<u64> {
    process_status_kb(pid, "VmRSS:")
}

/// Peak resident memory since the process started. Used for the M2 model
/// memory-budget evidence, where the transient peak is what matters.
pub fn process_peak_rss(pid: u32) -> io::Result<u64> {
    process_status_kb(pid, "VmHWM:")
}

fn process_status_kb(pid: u32, field: &str) -> io::Result<u64> {
    let text = fs::read_to_string(format!("/proc/{pid}/status"))?;
    let kb: u64 = text
        .lines()
        .find(|line| line.starts_with(field))
        .and_then(|line| line.split_whitespace().nth(1))
        .ok_or_else(|| io::Error::other("missing RSS"))?
        .parse()
        .map_err(io::Error::other)?;
    Ok(kb * 1024)
}

pub fn external_model_pids() -> io::Result<Vec<u32>> {
    let mut result = Vec::new();
    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let Ok(command) = fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        let arguments: Vec<&[u8]> = command
            .split(|byte| *byte == 0)
            .filter(|arg| !arg.is_empty())
            .collect();
        let Some(executable) = arguments.first() else {
            continue;
        };
        if executable.ends_with(b"llama-server")
            && arguments
                .iter()
                .any(|arg| matches!(*arg, b"-m" | b"--model" | b"-hf" | b"--hf-repo"))
        {
            result.push(pid);
        } else if executable.ends_with(b"ollama") || executable.ends_with(b"vllm") {
            // Residency unknown: refuse to silently coexist or stop another service.
            result.push(pid);
        }
    }
    Ok(result)
}

pub struct ModelLease {
    file: File,
    global_file: Option<File>,
}

impl ModelLease {
    pub fn acquire(path: &Path, reserve_bytes: u64) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)?;
        if !file.metadata()?.is_file() {
            return Err(io::Error::other("model lease is not a regular file"));
        }
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        let global_path = project_root().join("artifacts/private/models.lock");
        if let Some(parent) = global_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let global = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&global_path)?;
        if !global.metadata()?.is_file() {
            return Err(io::Error::other("global model lease is not a regular file"));
        }
        global.set_permissions(fs::Permissions::from_mode(0o600))?;
        FileExt::try_lock_exclusive(&global)?;
        let same_file = {
            use std::os::unix::fs::MetadataExt;
            let a = file.metadata()?;
            let b = global.metadata()?;
            a.dev() == b.dev() && a.ino() == b.ino()
        };
        // When both paths identify the same inode, keep the description that
        // owns the flock, not the separately opened/unlocked description.
        let (file, global_file) = if same_file {
            (global, None)
        } else {
            FileExt::try_lock_exclusive(&file)?;
            (file, Some(global))
        };
        if host_memory()?.available_bytes < reserve_bytes {
            return Err(io::Error::other("insufficient host memory headroom"));
        }
        if !external_model_pids()?.is_empty() {
            return Err(io::Error::other(
                "existing model process detected; no user service was stopped",
            ));
        }
        Ok(Self { file, global_file })
    }
}

impl Drop for ModelLease {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
        if let Some(global) = &self.global_file {
            let _ = FileExt::unlock(global);
        }
    }
}

pub fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

#[cfg(feature = "onnx")]
pub fn probe_onnx_library(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // No automatic runtime/model download. A TTS job will hold ModelLease while
    // owning its Session; this worker is separate from the API.
    tts::onnx_runtime::ensure_onnx_environment(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn admission_uses_the_frozen_one_point_five_gib_boundary() {
        assert!(!admits_ingestion(INGESTION_ADMISSION_RESERVE_BYTES - 1));
        assert!(admits_ingestion(INGESTION_ADMISSION_RESERVE_BYTES));
        assert!(admits_ingestion(INGESTION_ADMISSION_RESERVE_BYTES + 1));
    }
    #[test]
    fn host_snapshot_is_readable() {
        let memory = host_memory().unwrap();
        assert!(memory.available_bytes > 0);
    }
    #[test]
    fn independent_open_files_cannot_hold_two_model_leases() {
        let path = project_root().join(format!(
            "artifacts/private/test-model-{}.lock",
            std::process::id()
        ));
        let first = ModelLease::acquire(&path, 0).unwrap();
        assert!(ModelLease::acquire(&path, 0).is_err());
        let other = path.with_extension("other.lock");
        assert!(
            ModelLease::acquire(&other, 0).is_err(),
            "different output directories cannot bypass the global model lease"
        );
        drop(first);
        assert!(ModelLease::acquire(&path, 0).is_ok());
        fs::remove_file(path).unwrap();
        if other.exists() {
            fs::remove_file(other).unwrap();
        }
    }
}
