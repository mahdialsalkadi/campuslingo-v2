"""Offline evidence writing; sensitive reports are ignored by Git."""

import json
import os
from datetime import UTC, datetime
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
ARTIFACTS = ROOT / "artifacts"


def utc_now() -> str:
    return datetime.now(UTC).isoformat()


def write_report(relative: str, data: dict, *, private: bool = True) -> Path:
    path = (ARTIFACTS / relative).resolve()
    if not path.is_relative_to(ARTIFACTS.resolve()):
        raise ValueError("reports must stay inside project artifacts")
    path.parent.mkdir(parents=True, exist_ok=True)
    if private:
        path.parent.chmod(0o700)
    temporary = path.with_suffix(path.suffix + ".tmp")
    with temporary.open("w", encoding="utf-8") as handle:
        json.dump(data, handle, indent=2, sort_keys=True, allow_nan=False)
        handle.write("\n")
    temporary.chmod(0o600 if private else 0o644)
    os.replace(temporary, path)
    return path
