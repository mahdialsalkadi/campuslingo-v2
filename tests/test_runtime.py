import subprocess
import sys
from types import SimpleNamespace

import pytest
from campuslingo_ai import runtime
from campuslingo_ai.runtime import ModelAdmissionError, ModelLease, stop_owned_process


@pytest.fixture(autouse=True)
def quiet_host(monkeypatch):
    monkeypatch.setattr(runtime, "loaded_model_processes", lambda: [])
    monkeypatch.setattr(runtime.psutil, "virtual_memory", lambda: SimpleNamespace(available=10 * 1024**3))


def test_only_one_lease_even_with_separate_instances(tmp_path):
    path = tmp_path / "model.lock"
    with ModelLease(path):
        with pytest.raises(ModelAdmissionError):
            with ModelLease(path):
                pytest.fail("second model admitted")
    with ModelLease(path):
        pass


def test_existing_external_model_is_not_stopped(tmp_path, monkeypatch):
    monkeypatch.setattr(runtime, "loaded_model_processes", lambda: [{"pid": 123}])
    with pytest.raises(ModelAdmissionError, match="existing model"):
        with ModelLease(tmp_path / "model.lock"):
            pass


def test_memory_pressure_rejects_admission_and_releases_lock(tmp_path, monkeypatch):
    monkeypatch.setattr(runtime.psutil, "virtual_memory", lambda: SimpleNamespace(available=1))
    with pytest.raises(ModelAdmissionError, match="headroom"):
        with ModelLease(tmp_path / "model.lock"):
            pass
    monkeypatch.setattr(runtime.psutil, "virtual_memory", lambda: SimpleNamespace(available=10 * 1024**3))
    with ModelLease(tmp_path / "model.lock"):
        pass


def test_only_owned_process_group_is_reaped():
    process = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"], start_new_session=True)
    try:
        stop_owned_process(process)
        assert process.poll() is not None
        stop_owned_process(process)  # idempotent cleanup
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
