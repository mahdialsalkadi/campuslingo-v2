"""Read-only registry and local artifact audit. Does not download model weights."""

from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import httpx
from campuslingo_ai.corpus import sha256_file
from campuslingo_ai.gguf import metadata
from campuslingo_ai.reporting import utc_now, write_report

REPOSITORIES = {
    "ocr_upstream": "PaddlePaddle/PaddleOCR-VL-1.6",
    "ocr_gguf": "mradermacher/PaddleOCR-VL-1.6-GGUF",
    "router": "chaoliangUNSW/Jev-Style-2B-Decision-v4-GGUF",
    "generator_upstream": "Qwen/Qwen3.5-4B",
    "generator_gguf": "unsloth/Qwen3.5-4B-GGUF",
    "embedding": "Qwen/Qwen3-Embedding-0.6B-GGUF",
    "embedding_alternative": "ggml-org/bge-m3-Q8_0-GGUF",
    "glb_upstream": "ccore/Qwen3.5-4B-GLB-3D-Coder",
    "glb_gguf": "mradermacher/Qwen3.5-4B-GLB-3D-Coder-GGUF",
    "tts_english": "shawnahmed/Kokoro-82M-v1.0-ONNX-timestamped",
    "tts_arabic": "rhasspy/piper-voices",
    "alignment": "Systran/faster-whisper-tiny",
}
LOCAL_FILES = [
    Path("/home/mahdi/campus-lingo/models/qwen3.5-4b-instruct-q4_k_m.gguf"),
    Path("/home/mahdi/campus-lingo/models/qwen3.5-0.8b-instruct-q4_k_m.gguf"),
    Path("/home/mahdi/models/Jev-Style-Qwen3.5-2B-Decision-Q4_K_M.gguf"),
]


def audit_repository(item: tuple[str, str]) -> dict:
    role, repo = item
    url = f"https://huggingface.co/api/models/{repo}"
    try:
        response = httpx.get(url, timeout=25, follow_redirects=True)
        if response.status_code != 200:
            return {"role": role, "repository": repo, "url": url, "status": "blocked_repository_unavailable", "http_status": response.status_code}
        data = response.json()
        card = data.get("cardData") or {}
        files = [file["rfilename"] for file in data.get("siblings", [])]
        return {
            "role": role, "repository": data.get("id", repo), "url": url,
            "status": "metadata_available_not_runtime_verified", "revision": data.get("sha"),
            "license_claim": card.get("license"), "base_model": card.get("base_model"),
            "gated": data.get("gated"),
            "relevant_files": [name for name in files if any(key in name.lower() for key in ("q4_k_m", "q8_0", "mmproj", "license", "readme", "timestamp", "kareem", "config.json"))][:100],
            "license_status": "model_card_claim_only; upstream, redistribution, and per-voice/data terms require review",
        }
    except (httpx.HTTPError, ValueError) as exc:
        return {"role": role, "repository": repo, "url": url, "status": "blocked_metadata_request", "error": str(exc)}


if __name__ == "__main__":
    with ThreadPoolExecutor(max_workers=4) as pool:
        repositories = list(pool.map(audit_repository, REPOSITORIES.items()))
    local = []
    for path in LOCAL_FILES:
        if not path.is_file():
            local.append({"path": str(path), "status": "missing"})
            continue
        try:
            local.append({"path": str(path), "status": "inspected_read_only", "sha256": sha256_file(path), "metadata": metadata(path), "provenance": "local artifact; digest recorded but not yet matched to a particular upstream LFS object"})
        except (ValueError, OSError) as exc:
            local.append({"path": str(path), "status": "invalid", "error": str(exc)})
    report = {"observed_at": utc_now(), "repositories": repositories, "local_artifacts": local, "downloaded_weights": False}
    path = write_report("private/model-audit.json", report)
    for repo in repositories:
        print(f"{repo['role']}: {repo['status']} ({repo.get('license_claim', 'unverified license')})")
    for model in local:
        print(f"local {Path(model['path']).name}: {model['status']}")
    print(f"Private audit: {path}")
