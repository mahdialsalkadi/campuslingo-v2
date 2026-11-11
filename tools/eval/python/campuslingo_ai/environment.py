"""Offline read-only host inspection; never modify existing services/GPU configuration."""

import importlib.metadata
import importlib.util
import platform
import shutil
import subprocess
from pathlib import Path

import psutil

from campuslingo_ai.reporting import ROOT, utc_now


def gpu_snapshot() -> list[dict]:
    devices = []
    seen = set()
    for path in sorted(Path("/sys/class/drm").glob("card[0-9]*/device")):
        identity = str(path.resolve())
        if identity in seen:
            continue
        seen.add(identity)
        memory = {}
        for field in ("mem_info_vram_total", "mem_info_vram_used", "mem_info_gtt_total", "mem_info_gtt_used"):
            try:
                memory[field] = int((path / field).read_text().strip())
            except (OSError, ValueError):
                pass
        if memory:
            devices.append({"physical_device": identity, "memory_bytes": memory})
    return devices


def loaded_model_processes() -> list[dict]:
    found = []
    for process in psutil.process_iter(["pid", "name", "cmdline"]):
        try:
            info = process.info
            name = (info["name"] or "").lower()
            args = info["cmdline"] or []
            if "llama-server" in name and any(arg in ("-m", "--model", "-hf", "--hf-repo") for arg in args):
                found.append({"pid": info["pid"], "kind": "llama-server"})
            elif any(term in name for term in ("vllm", "ollama")):
                found.append({"pid": info["pid"], "kind": name, "residency": "unknown; explicit inspection required"})
        except (psutil.NoSuchProcess, psutil.AccessDenied):
            continue
    return found


def command_output(command: list[str], timeout: float = 15) -> dict:
    executable = shutil.which(command[0])
    if executable is None:
        return {"status": "missing", "command": command}
    try:
        result = subprocess.run(command, capture_output=True, text=True, timeout=timeout, check=False)
        return {"status": "observed" if result.returncode == 0 else "failed", "command": command, "returncode": result.returncode, "stdout": result.stdout[:16_000], "stderr": result.stderr[:4000]}
    except (OSError, subprocess.TimeoutExpired) as exc:
        return {"status": "failed", "command": command, "error": str(exc)}


def environment_report() -> dict:
    packages = {}
    for name in ("fsrs", "pydantic", "PyMuPDF", "psutil", "httpx", "torch", "onnxruntime"):
        try:
            distribution = importlib.metadata.distribution(name)
            packages[name] = {"version": distribution.version, "license": distribution.metadata.get("License-Expression") or distribution.metadata.get("License")}
        except importlib.metadata.PackageNotFoundError:
            packages[name] = {"status": "not_installed"}
    return {
        "observed_at": utc_now(),
        "platform": platform.platform(),
        "python": platform.python_version(),
        "cpu_count": psutil.cpu_count(),
        "memory": psutil.virtual_memory()._asdict(),
        "swap": psutil.swap_memory()._asdict(),
        "disk": shutil.disk_usage(ROOT)._asdict(),
        "gpu": gpu_snapshot(),
        "memory_caveat": "DRM VRAM reservation, GTT aperture, and Vulkan unified capacity overlap; do not add them as independent physical memory.",
        "model_processes": loaded_model_processes(),
        "commands": [command_output(command) for command in (["rustc", "--version"], ["cargo", "--version"], ["rustup", "target", "list", "--installed"], ["node", "--version"], ["npm", "--version"], ["uv", "--version"], ["llama-server", "--version"], ["llama-server", "--list-devices"], ["vulkaninfo", "--summary"], ["ss", "-ltn"])],
        "packages": packages,
        "training": {
            "status": "unverified" if importlib.util.find_spec("torch") else "blocked_missing_training_runtime",
            "reason": "A Vulkan GGUF inference backend does not establish LoRA/GRPO kernel, autograd, optimizer, or memory compatibility. No large training dependencies were downloaded.",
        },
    }
