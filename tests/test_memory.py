import json
from datetime import UTC, datetime
from pathlib import Path

import pytest
from campuslingo_ai.memory import PARAMETERS, review_snapshot, shield_retrievability


def test_curves_are_normalized_at_stability_and_monotonic():
    for stability in (0.05, 1, 5, 21, 365):
        assert shield_retrievability(stability, 0) == 1
        assert shield_retrievability(stability, stability) == pytest.approx(0.9)
        values = [shield_retrievability(stability, day) for day in (0, 1, 5, 21, 365)]
        assert values == sorted(values, reverse=True)
    assert shield_retrievability(0, 0) == 0
    assert len(PARAMETERS) == 21


def test_python_vectors_stay_in_sync_with_pinned_library():
    data = json.loads((Path(__file__).parent / "fsrs_golden_vectors.json").read_text())
    assert data["parameters"] == list(PARAMETERS)
    for vector in data["reviews"]:
        result = review_snapshot(vector["before"], vector["rating"], datetime.fromisoformat(vector["now"]))
        assert result == vector["expected"]


def test_invalid_memory_inputs():
    for args in ((-1, 0), (1, -1), (float("nan"), 1), (1, float("inf"))):
        with pytest.raises(ValueError):
            shield_retrievability(*args)


def test_review_rejects_naive_or_backwards_time():
    snapshot = {"state": 2, "stability": 5.0, "difficulty": 5.0, "due": "2026-01-02T00:00:00+00:00", "last_review": "2026-01-01T12:00:00+00:00"}
    for now in (datetime(2026, 1, 1), datetime(2026, 1, 1, tzinfo=UTC)):
        with pytest.raises(ValueError):
            review_snapshot(snapshot, 3, now)
