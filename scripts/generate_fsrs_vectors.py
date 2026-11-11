"""Synthetic fixtures generated from pinned upstream py-fsrs, never lecture data."""

import json
from datetime import UTC, datetime, timedelta
from importlib.metadata import version
from pathlib import Path

from campuslingo_ai.memory import PARAMETERS, review_snapshot, shield_retrievability

ROOT = Path(__file__).resolve().parents[1]
BASE = datetime(2026, 1, 1, 12, tzinfo=UTC)


def generate() -> dict:
    curves = []
    for decay in (PARAMETERS[20], 0.3, 0.7):
        for stability in (0.0, 0.05, 1.0, 5.0, 21.0, 365.0):
            for elapsed in (0.0, 0.001, 0.5, 1.0, 21.0, 365.0):
                curves.append({
                    "stability": stability,
                    "elapsed_days": elapsed,
                    "decay": decay,
                    "expected": shield_retrievability(stability, elapsed, decay),
                })
    reviews = []
    for rating in range(1, 5):
        snapshot = {"state": "new", "stability": None, "difficulty": None, "last_review": None, "due": BASE.isoformat()}
        reviews.append({"name": f"new-{rating}", "before": snapshot, "rating": rating, "now": BASE.isoformat(), "expected": review_snapshot(snapshot, rating, BASE)})
    for state in (1, 2, 3):
        for stability in (0.05, 1.0, 5.0, 21.0, 365.0):
            for elapsed in (0.0, 0.5, 1.0, 21.0, 365.0):
                for rating in range(1, 5):
                    snapshot = {"state": state, "stability": stability, "difficulty": 5.0, "last_review": BASE.isoformat(), "due": (BASE + timedelta(days=stability)).isoformat()}
                    now = BASE + timedelta(days=elapsed)
                    reviews.append({"name": f"state{state}-S{stability}-t{elapsed}-r{rating}", "before": snapshot, "rating": rating, "now": now.isoformat(), "expected": review_snapshot(snapshot, rating, now)})
    return {
        "schema_version": "1.0.0",
        "producer": f"py-fsrs/{version('fsrs')}",
        "algorithm": "FSRS-6",
        "absolute_tolerance": 1e-6,
        "parameters": list(PARAMETERS),
        "options": {"retention": 0.9, "learning_steps": [], "relearning_steps": [], "fuzz": False},
        "semantics": {"scheduler": "upstream whole elapsed days", "shield_curve": "continuous fractional days", "new_retrievability": 0},
        "curves": curves,
        "reviews": reviews,
    }


if __name__ == "__main__":
    path = ROOT / "tests/fsrs_golden_vectors.json"
    content = generate()
    path.write_text(json.dumps(content, indent=2, sort_keys=True) + "\n")
    print(f"Wrote {len(content['curves'])} curve and {len(content['reviews'])} review vectors")
