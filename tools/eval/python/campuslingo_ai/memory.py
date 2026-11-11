"""Offline py-fsrs oracle. The production implementation is shared Rust/WASM.

The shield curve is explicitly continuous; it is not silently substituted for
the upstream scheduler's whole-day retrievability calculation.
"""

from __future__ import annotations

import math
from datetime import UTC, datetime

from fsrs import Card, Rating, Scheduler, State

PARAMETERS = tuple(Scheduler().parameters)


def scheduler() -> Scheduler:
    return Scheduler(
        parameters=PARAMETERS,
        desired_retention=0.9,
        learning_steps=(),
        relearning_steps=(),
        enable_fuzzing=False,
    )


def shield_retrievability(stability: float, elapsed_days: float, decay: float = PARAMETERS[20]) -> float:
    if not all(math.isfinite(value) for value in (stability, elapsed_days, decay)):
        raise ValueError("memory inputs must be finite")
    if stability < 0 or elapsed_days < 0 or not 0.1 <= decay <= 0.8:
        raise ValueError("invalid stability, elapsed time, or decay")
    if stability == 0:
        return 0.0  # explicitly unreviewed; eligibility is handled separately
    factor = math.expm1(math.log(0.9) / -decay)
    return (1 + factor * elapsed_days / stability) ** -decay


def review_snapshot(snapshot: dict, rating: int, now: datetime) -> dict:
    if now.tzinfo != UTC or rating not in (1, 2, 3, 4):
        raise ValueError("review requires UTC time and rating 1–4")
    last = datetime.fromisoformat(snapshot["last_review"]) if snapshot["last_review"] else None
    if last is not None and (last.tzinfo != UTC or now < last):
        raise ValueError("review timestamps must be UTC and nondecreasing")
    state = snapshot["state"]
    card = Card(
        card_id=1,
        state=State.Learning if state == "new" else State(state),
        step=0 if state in ("new", 1, 3) else None,
        stability=snapshot["stability"],
        difficulty=snapshot["difficulty"],
        due=datetime.fromisoformat(snapshot["due"]),
        last_review=last,
    )
    model = scheduler()
    before = model.get_card_retrievability(card, now)
    reviewed, _ = model.review_card(card, Rating(rating), now)
    return {
        "stability": reviewed.stability,
        "difficulty": reviewed.difficulty,
        "state": int(reviewed.state),
        "due": reviewed.due.isoformat(),
        "last_review": reviewed.last_review.isoformat(),
        "retrievability_before": before,
    }
