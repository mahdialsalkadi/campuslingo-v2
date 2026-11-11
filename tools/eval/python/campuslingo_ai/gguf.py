"""Offline bounded GGUF metadata reader; never loads tensor weights into memory."""

import struct
from pathlib import Path

SCALARS = {0: "B", 1: "b", 2: "H", 3: "h", 4: "I", 5: "i", 6: "f", 7: "?", 10: "Q", 11: "q", 12: "d"}


def metadata(path: Path) -> dict:
    size = path.stat().st_size
    with path.open("rb") as handle:
        def number(fmt: str):
            length = struct.calcsize("<" + fmt)
            data = handle.read(length)
            if len(data) != length or handle.tell() > min(size, 64 * 1024**2):
                raise ValueError("truncated or excessive GGUF metadata")
            return struct.unpack("<" + fmt, data)[0]

        def string(capture: bool = True):
            length = number("Q")
            if length > 2 * 1024**2 or handle.tell() + length > min(size, 64 * 1024**2):
                raise ValueError("excessive GGUF string")
            if not capture:
                handle.seek(length, 1)
                return None
            return handle.read(length).decode("utf-8", errors="replace")

        def value(kind: int, capture: bool = True, depth: int = 0):
            if depth > 3:
                raise ValueError("nested GGUF metadata exceeds limit")
            if kind in SCALARS:
                item = number(SCALARS[kind])
                return item if capture else None
            if kind == 8:
                return string(capture)
            if kind == 9:
                subtype, length = number("I"), number("Q")
                if length > 1_000_000:
                    raise ValueError("excessive GGUF array")
                items = []
                for _ in range(length):
                    item = value(subtype, capture and length <= 32, depth + 1)
                    if capture and length <= 32:
                        items.append(item)
                return items if capture and length <= 32 else {"array_length": length}
            raise ValueError(f"unknown GGUF value type {kind}")

        if handle.read(4) != b"GGUF":
            raise ValueError("not a GGUF file")
        version, tensors, fields = number("I"), number("Q"), number("Q")
        if version not in (2, 3) or fields > 10_000:
            raise ValueError("unsupported GGUF version or metadata count")
        result = {"gguf_version": version, "tensor_count": tensors, "file_bytes": size}
        for _ in range(fields):
            key = string()
            capture = key.startswith("general.") or key.endswith(("context_length", "embedding_length", "block_count"))
            item = value(number("I"), capture)
            if capture:
                result[key] = item
        result["quantization_label"] = {7: "Q8_0", 15: "Q4_K_M", 18: "Q6_K"}.get(result.get("general.file_type"), "inspect llama.h enum")
        return result
