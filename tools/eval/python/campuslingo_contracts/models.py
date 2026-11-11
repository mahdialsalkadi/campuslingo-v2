"""Offline reference contracts; canonical schemas originate in crates/domain."""

from __future__ import annotations

from decimal import Decimal
from typing import Annotated, Literal
from uuid import UUID

from pydantic import BaseModel, ConfigDict, Field, RootModel, model_validator

VERSION = "1.0.0"
Archetype = Literal["PIPELINE", "TRADEOFF", "SPATIAL", "CONTRAST", "TRAP", "AXIOM"]
WorldTheme = Literal["BIO_SYSTEM", "CYBER_GRID", "MACRO_POLIS", "ARCANE_ARCHIVE"]
MODIFIERS: dict[str, tuple[str, ...]] = {
    "PIPELINE": ("REVERSE_ENG", "BOTTLENECK_RELIEF", "FAULT_INJECTION"),
    "TRADEOFF": ("OVERLOAD_STRESS", "BUDGET_CAP", "PARETO_BALANCE"),
    "SPATIAL": ("BLIND_DIAGNOSIS", "SURGICAL_REPAIR", "CROSS_SECTION_SCAN"),
    "CONTRAST": ("STRESS_BENCHMARK", "BLIND_CHAMBER_ID", "CROSS_CALIBRATION"),
    "TRAP": ("FORENSIC_AUDIT", "COUNTEREXAMPLE_PROBE", "SAFEGUARD_PATCH"),
    "AXIOM": ("SPEED_BLITZ", "COROLLARY_CHAIN", "BOUNDARY_TEST"),
}
Identifier = Annotated[str, Field(pattern=r"^[a-z][a-z0-9_]{0,47}$")]
FiniteNumber = Annotated[float, Field(ge=-1_000_000, le=1_000_000)]


class Contract(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, strict=True, allow_inf_nan=False)


class Constant(Contract):
    kind: Literal["constant"]
    value: FiniteNumber


class Variable(Contract):
    kind: Literal["variable"]
    name: Identifier


class Operation(Contract):
    kind: Literal["operation"]
    op: Literal["add", "subtract", "multiply", "divide", "min", "max"]
    args: Annotated[list[Expression], Field(min_length=2, max_length=4)]

    @model_validator(mode="after")
    def binary_arity(self) -> Operation:
        if self.op in ("subtract", "divide") and len(self.args) != 2:
            raise ValueError("subtract/divide require exactly two arguments")
        return self


Expression = Annotated[Constant | Variable | Operation, Field(discriminator="kind")]
Operation.model_rebuild()


class ParameterSpec(Contract):
    name: Identifier
    label: Annotated[str, Field(min_length=1, max_length=120)]
    minimum: FiniteNumber
    maximum: FiniteNumber
    step: Annotated[float, Field(gt=0, le=1_000_000)]
    initial: FiniteNumber

    @model_validator(mode="after")
    def finite_grid(self) -> ParameterSpec:
        lo, hi, step, initial = map(
            lambda value: Decimal(str(value)),
            (self.minimum, self.maximum, self.step, self.initial),
        )
        if hi <= lo:
            raise ValueError("parameter maximum must exceed minimum")
        ticks = (hi - lo) / step
        if ticks != ticks.to_integral_value() or ticks > 100:
            raise ValueError("grid must have integral steps and at most 101 points")
        offset = (initial - lo) / step
        if not lo <= initial <= hi or offset != offset.to_integral_value():
            raise ValueError("initial value must lie on the parameter grid")
        return self

    def values(self) -> list[float]:
        lo, hi, step = map(lambda value: Decimal(str(value)), (self.minimum, self.maximum, self.step))
        return [float(lo + step * index) for index in range(int((hi - lo) / step) + 1)]


class NumericTarget(Contract):
    name: Identifier
    expression: Expression
    lower: FiniteNumber
    upper: FiniteNumber

    @model_validator(mode="after")
    def ordered_bounds(self) -> NumericTarget:
        if self.upper < self.lower:
            raise ValueError("target upper bound must be >= lower bound")
        return self


class SandboxSpec(Contract):
    schema_version: Literal["1.0.0"]
    archetype: Archetype
    modifier: Annotated[str, Field(min_length=1, max_length=48)]
    parameters: Annotated[list[ParameterSpec], Field(min_length=1, max_length=4)]
    targets: Annotated[list[NumericTarget], Field(min_length=1, max_length=8)]
    time_limit_ms: Annotated[int, Field(ge=1_000, le=30_000)]

    @model_validator(mode="after")
    def bounded_program(self) -> SandboxSpec:
        if self.modifier not in MODIFIERS[self.archetype]:
            raise ValueError("modifier is incompatible with archetype")
        names = {parameter.name for parameter in self.parameters}
        if len(names) != len(self.parameters):
            raise ValueError("parameter names must be unique")
        if len({target.name for target in self.targets}) != len(self.targets):
            raise ValueError("target names must be unique")
        states = 1
        for parameter in self.parameters:
            states *= len(parameter.values())
        if states > 4096:
            raise ValueError("sandbox exceeds the 4096-state exhaustive validation budget")
        for target in self.targets:
            count = 0
            pending = [(target.expression, 1)]
            while pending:
                expression, depth = pending.pop()
                count += 1
                if depth > 8 or count > 64:
                    raise ValueError("expression exceeds depth/node budget")
                if isinstance(expression, Variable) and expression.name not in names:
                    raise ValueError(f"undefined variable: {expression.name}")
                if isinstance(expression, Operation):
                    pending.extend((argument, depth + 1) for argument in expression.args)
        return self


class SourceRegion(Contract):
    source_sha256: Annotated[str, Field(pattern=r"^[0-9a-f]{64}$")]
    page: Annotated[int, Field(ge=1, le=10_000)]
    bbox: Annotated[list[float], Field(min_length=4, max_length=4)]
    page_width: Annotated[float, Field(gt=0, le=100_000)]
    page_height: Annotated[float, Field(gt=0, le=100_000)]
    text_quote: Annotated[str, Field(min_length=1, max_length=4000)]

    @model_validator(mode="after")
    def contained_box(self) -> SourceRegion:
        x0, y0, x1, y1 = self.bbox
        if not (0 <= x0 < x1 <= self.page_width and 0 <= y0 < y1 <= self.page_height):
            raise ValueError("source bbox must be non-empty and inside the page")
        return self


class ChoiceChallenge(Contract):
    id: UUID
    type: Literal["MCQ_QUIZ"]
    prompt: Annotated[str, Field(min_length=1, max_length=1200)]
    options: Annotated[list[Annotated[str, Field(min_length=1, max_length=400)]], Field(min_length=2, max_length=6)]
    correct_index: Annotated[int, Field(ge=0, le=5)]
    explanation: Annotated[str, Field(min_length=1, max_length=1500)]

    @model_validator(mode="after")
    def answer_in_options(self) -> ChoiceChallenge:
        if self.correct_index >= len(self.options) or len(set(self.options)) != len(self.options):
            raise ValueError("answer must exist and options must be distinct")
        return self


class DirectorTrack(Contract):
    start_ms: Annotated[int, Field(ge=0, le=30_000)]
    end_ms: Annotated[int, Field(ge=1, le=30_000)]
    focus_role: Literal["concept", "input", "output", "fault", "comparison"]
    shot_type: Literal["overview", "close_up", "compare"]
    explode_factor: Annotated[float, Field(ge=0, le=1)]

    @model_validator(mode="after")
    def positive_span(self) -> DirectorTrack:
        if self.end_ms <= self.start_ms:
            raise ValueError("director track duration must be positive")
        return self


class AudioWord(Contract):
    text: Annotated[str, Field(min_length=1, max_length=120)]
    start_ms: Annotated[int, Field(ge=0, le=30_000)]
    end_ms: Annotated[int, Field(ge=1, le=30_000)]


class AudioManifest(Contract):
    asset_id: UUID
    language: Literal["en", "ar"]
    codec: Literal["opus"]
    text: Annotated[str, Field(min_length=1, max_length=2000)]
    duration_ms: Annotated[int, Field(ge=1, le=30_000)]
    alignment_method: Literal["model_durations", "forced_alignment", "asr_estimate"]
    words: Annotated[list[AudioWord], Field(min_length=1, max_length=200)]

    @model_validator(mode="after")
    def timestamps_within_audio(self) -> AudioManifest:
        previous_end = 0
        for word in self.words:
            if not previous_end <= word.start_ms < word.end_ms <= self.duration_ms:
                raise ValueError("word timestamps must be ordered and contained in the audio")
            previous_end = word.end_ms
        return self


class LearningUnitBundle(Contract):
    schema_version: Literal["1.0.0"]
    node_id: UUID
    language: Literal["en", "ar"]
    world_theme: WorldTheme
    title: Annotated[str, Field(min_length=1, max_length=160)]
    tactical_summary: Annotated[list[Annotated[str, Field(min_length=1, max_length=400)]], Field(min_length=2, max_length=2)]
    source_regions: Annotated[list[SourceRegion], Field(min_length=1, max_length=20)]
    challenges: Annotated[list[ChoiceChallenge], Field(min_length=1, max_length=20)]
    sandbox: SandboxSpec | None
    director_tracks: Annotated[list[DirectorTrack], Field(max_length=30)]
    audio: AudioManifest | None
    generator_version: Annotated[str, Field(min_length=1, max_length=64)]


class DeployCommand(Contract):
    type: Literal["deploy"]
    protocol_version: Literal["1.0.0"]
    match_id: UUID
    sequence: Annotated[int, Field(ge=0, le=2**31 - 1)]
    card_id: UUID
    lane: Literal[0, 1]


class AnswerCommand(Contract):
    type: Literal["answer"]
    protocol_version: Literal["1.0.0"]
    match_id: UUID
    sequence: Annotated[int, Field(ge=0, le=2**31 - 1)]
    challenge_id: UUID
    selected_index: Annotated[int, Field(ge=0, le=5)]


class ClientCommand(RootModel[Annotated[DeployCommand | AnswerCommand, Field(discriminator="type")]]):
    """Actor identity comes from the authenticated transport, never this payload."""


class PublicQuestion(Contract):
    id: UUID
    prompt: Annotated[str, Field(min_length=1, max_length=1200)]
    options: Annotated[list[str], Field(min_length=2, max_length=6)]


EXPORTED_MODELS = {
    cls.__name__: cls
    for cls in (SandboxSpec, LearningUnitBundle, DirectorTrack, AudioManifest, ClientCommand, PublicQuestion)
}
