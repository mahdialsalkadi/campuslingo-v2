"""Offline reference evaluator and exhaustive verifier for finite slider grids.

This proves reachability only over the declared quantized interaction domain.
No Python/JavaScript expression strings are interpreted.
"""

from __future__ import annotations

import itertools
import math
from dataclasses import asdict, dataclass

from campuslingo_contracts.models import Constant, Expression, Operation, SandboxSpec, Variable


class UnsafeExpression(ValueError):
    pass


def evaluate(expression: Expression, values: dict[str, float]) -> float:
    if isinstance(expression, Constant):
        result = expression.value
    elif isinstance(expression, Variable):
        result = values[expression.name]
    elif isinstance(expression, Operation):
        args = [evaluate(argument, values) for argument in expression.args]
        match expression.op:
            case "add":
                result = sum(args)
            case "subtract":
                result = args[0] - args[1]
            case "multiply":
                result = math.prod(args)
            case "divide":
                if abs(args[1]) <= 1e-12:
                    raise UnsafeExpression("division by zero or near-zero denominator")
                result = args[0] / args[1]
            case "min":
                result = min(args)
            case "max":
                result = max(args)
            case _:
                raise UnsafeExpression("unsupported operation")
    else:
        raise UnsafeExpression("unsupported expression")
    if not math.isfinite(result) or abs(result) > 1e12:
        raise UnsafeExpression("non-finite or excessive intermediate output")
    return result


@dataclass(frozen=True)
class ValidationResult:
    valid: bool
    reason: str
    states_checked: int
    witness: dict[str, float] | None
    proof_domain: str = "exhaustive_declared_discrete_grid"

    def to_dict(self) -> dict:
        return asdict(self)


def verify_sandbox(spec: SandboxSpec) -> ValidationResult:
    names = [parameter.name for parameter in spec.parameters]
    witness = None
    checked = 0
    for coordinates in itertools.product(*(parameter.values() for parameter in spec.parameters)):
        values = dict(zip(names, coordinates, strict=True))
        checked += 1
        try:
            outputs = [evaluate(target.expression, values) for target in spec.targets]
        except (UnsafeExpression, KeyError, OverflowError) as exc:
            return ValidationResult(False, str(exc), checked, None)
        if all(target.lower <= output <= target.upper for target, output in zip(spec.targets, outputs, strict=True)):
            witness = witness or values
        # Keep checking after a witness: unsafe values elsewhere invalidate the spec.
    if witness is None:
        return ValidationResult(False, "no reachable target in the declared grid", checked, None)
    return ValidationResult(True, "all states safe; target reachable", checked, witness)
