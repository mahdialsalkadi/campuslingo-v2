"""Offline corpus selection only. Source PDFs are never copied/uploaded."""

from __future__ import annotations

import hashlib
import re
import statistics
from collections import Counter
from pathlib import Path

import pymupdf

ENGLISH = set("the and of to is in for with that this as are be from data python set vector logic if else input output algorithm".split())
MALAY = set("yang dan untuk dengan dalam adalah ini itu pada saya bahasa malaysia komunikasi rutin harian integriti rasuah".split())
MATH = re.compile(r"[¬∧∨⇒⇔∀∃∈∉∪∩⊂⊆Σ∑∫√λθ]|\b(?:vector|matrix|logic|theorem|equation|set theory)\b", re.I)
ADMIN = re.compile(r"(?:^ci[ _-]|^utm-ci|course information|installpython|installpycharm|jadual|kalendar|pemakluman|assignment|submission|section02_group)", re.I)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(4 * 1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def inspect_pdf(path: Path) -> dict:
    if path.stat().st_size > 100 * 1024**2:
        return {"path": str(path), "status": "skipped_size_limit"}
    with pymupdf.open(path) as document:
        count = document.page_count
        if document.needs_pass or not 8 <= count <= 300:
            return {"path": str(path), "status": "skipped_encryption_or_page_limit", "pages": count}
        # Bound work even for long lecture decks; sample across the entire deck.
        indices = sorted({round(i * (count - 1) / min(count - 1, 23)) for i in range(min(count, 24))})
        texts = [document[index].get_text() for index in indices]
        repeated = Counter(line.strip() for text in texts for line in set(text.splitlines()) if 0 < len(line.strip()) < 100)
        boilerplate = {line for line, occurrences in repeated.items() if occurrences >= len(indices) * 0.6}
        texts = ["\n".join(line for line in text.splitlines() if line.strip() not in boilerplate) for text in texts]
        words = re.findall(r"[a-z]+", " ".join(texts).lower())
        english = sum(word in ENGLISH for word in words)
        malay = sum(word in MALAY for word in words)
        filename_words = set(re.findall(r"[a-z]+", path.stem.lower()))
        image_coverage = []
        drawings = []
        for index in indices:
            page = document[index]
            area = page.rect.get_area()
            coverage = sum((pymupdf.Rect(info["bbox"]) & page.rect).get_area() for info in page.get_image_info())
            # Overlaps are capped; this is a selection heuristic, not a geometry union proof.
            image_coverage.append(min(1.0, coverage / area) if area else 0.0)
            drawings.append(min(len(page.get_drawings()), 10_000))
        body_lengths = [len(text.strip()) for text in texts[1:-1] or texts]
        low_fraction = sum(length < 120 for length in body_lengths) / len(body_lengths)
        english_ratio = english / max(1, english + malay)
        english_candidate = english >= 10 and english_ratio >= 0.8
        inferred_english = not english_candidate and bool(filename_words & ENGLISH) and not (filename_words & MALAY)
        math_score = len(MATH.findall(" ".join(texts))) / len(indices) + len(MATH.findall(path.stem)) * 2
        return {
            "path": str(path), "status": "inspected", "pages": count,
            "sampled_pages": [index + 1 for index in indices],
            "median_body_chars": statistics.median(body_lengths), "low_text_fraction": low_fraction,
            "mean_image_coverage_heuristic": statistics.mean(image_coverage),
            "mean_vector_paths": statistics.mean(drawings), "math_score": math_score,
            "english_signal_ratio": english_ratio, "english_hits": english,
            "english_candidate": english_candidate or inferred_english,
            "language_evidence": "text_heuristic" if english_candidate else "filename_only_OCR_verification_pending" if inferred_english else "insufficient",
        }


def select_representatives(records: list[dict]) -> dict[str, dict]:
    candidates = [record for record in records if record.get("status") == "inspected" and record.get("english_candidate")]
    selected = {}
    used = set()
    filters_and_scores = [
        ("digital_text", lambda r: r["low_text_fraction"] < 0.35 and r["median_body_chars"] >= 180, lambda r: r["median_body_chars"]),
        ("diagram_math", lambda r: r["math_score"] > 0 and r["low_text_fraction"] < 0.85, lambda r: r["math_score"] + r["mean_image_coverage_heuristic"]),
        ("low_text_image", lambda r: r["low_text_fraction"] >= 0.65 and r["mean_image_coverage_heuristic"] >= 0.35, lambda r: r["low_text_fraction"] + r["mean_image_coverage_heuristic"]),
    ]
    for category, predicate, score in filters_and_scores:
        choices = [record for record in candidates if record["path"] not in used and predicate(record)]
        if not choices:
            raise ValueError(f"No suitable {category} PDF; do not silently substitute an unrepresentative file")
        chosen = max(choices, key=lambda record: (score(record), record["path"]))
        used.add(chosen["path"])
        selected[category] = {**chosen, "sha256": sha256_file(Path(chosen["path"])), "selection_reason": category}
    return selected


def scan_corpus(root: Path) -> dict:
    if not root.is_dir():
        raise ValueError("corpus root must be an existing directory")
    records = []
    files = sorted(root.rglob("*.pdf"))
    for path in files:
        if ADMIN.search(path.name) or "course information" in str(path.parent).lower() or any(part.lower() == "materials" for part in path.parts):
            continue
        try:
            records.append(inspect_pdf(path))
        except Exception as exc:
            records.append({"path": str(path), "status": "failed", "error": str(exc)})
    return {
        "source_root": str(root), "pdfs_discovered": len(files), "records": records,
        "selected": select_representatives(records),
        "privacy": "local evaluation only; source PDFs were not copied or uploaded",
        "limitations": "bounded page sampling and heuristic language/layout scoring; image-only language needs OCR confirmation",
    }
