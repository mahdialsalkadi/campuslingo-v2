"""Offline reference tests only; the real lease/watchdog is in crates/ingestion."""

from __future__ import annotations

import fcntl
import os
import signal
import subprocess
import threading
from pathlib import Path

import psutil

from campuslingo_ai.environment import loaded_model_processes


class ModelAdmissionError(RuntimeError):
    pass


class ModelLease:
    def __init__(self, path: Path, minimum_available_bytes: int = 3 * 1024**3):
        self.path = path
        self.minimum_available_bytes = minimum_available_bytes
        self.handle = None

    def __enter__(self) -> ModelLease:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.handle = self.path.open("a+")
        os.chmod(self.path, 0o600)
        try:
            fcntl.flock(self.handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
            if psutil.virtual_memory().available < self.minimum_available_bytes:
                raise ModelAdmissionError("insufficient host memory headroom")
            if loaded_model_processes():
                raise ModelAdmissionError("an existing model server is present; it was not stopped")
        except (BlockingIOError, ModelAdmissionError) as exc:
            self.handle.close()
            self.handle = None
            raise ModelAdmissionError(str(exc)) from exc
        return self

    def __exit__(self, *_args) -> None:
        if self.handle:
            fcntl.flock(self.handle.fileno(), fcntl.LOCK_UN)
            self.handle.close()
            self.handle = None


def stop_owned_process(process: subprocess.Popen) -> None:
    """Only for children launched with start_new_session=True by this module."""
    if process.poll() is not None:
        return
    try:
        os.killpg(process.pid, signal.SIGTERM)
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        process.wait(timeout=5)
    except ProcessLookupError:
        process.wait(timeout=5)


class ProcessWatchdog:
    def __init__(self, process: subprocess.Popen, *, minimum_available_bytes: int = 3 * 1024**3, maximum_rss_bytes: int = 6 * 1024**3):
        self.process = process
        self.minimum_available_bytes = minimum_available_bytes
        self.maximum_rss_bytes = maximum_rss_bytes
        self.peak_rss_bytes = 0
        self.minimum_observed_available_bytes = psutil.virtual_memory().available
        self.reason: str | None = None
        self.done = threading.Event()
        self.thread = threading.Thread(target=self._run, daemon=True)

    def _run(self) -> None:
        breaches = 0
        while not self.done.wait(0.2):
            try:
                rss = psutil.Process(self.process.pid).memory_info().rss
                available = psutil.virtual_memory().available
                self.peak_rss_bytes = max(self.peak_rss_bytes, rss)
                self.minimum_observed_available_bytes = min(self.minimum_observed_available_bytes, available)
                breaches = breaches + 1 if available < self.minimum_available_bytes or rss > self.maximum_rss_bytes else 0
                if breaches >= 2:
                    self.reason = "host headroom or owned-process RSS budget exceeded"
                    stop_owned_process(self.process)
                    return
            except psutil.NoSuchProcess:
                return

    def __enter__(self) -> ProcessWatchdog:
        self.thread.start()
        return self

    def __exit__(self, *_args) -> None:
        self.done.set()
        self.thread.join(timeout=6)
