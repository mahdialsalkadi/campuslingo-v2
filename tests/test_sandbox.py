import copy
import json
from pathlib import Path

import pytest
from campuslingo_ai.sandbox import verify_sandbox
from campuslingo_contracts.models import ParameterSpec, SandboxSpec
from pydantic import ValidationError

FIXTURE = Path(__file__).parent / "fixtures/sandbox.valid.json"


@pytest.fixture
def payload():
    return json.loads(FIXTURE.read_text())


def test_valid_spec_is_exhaustively_checked(payload):
    result = verify_sandbox(SandboxSpec.model_validate(payload))
    assert result.valid
    assert result.states_checked == 11
    assert result.witness == {"power": 6.0}
    assert result.proof_domain == "exhaustive_declared_discrete_grid"


def test_continuous_solution_is_not_a_discrete_solution(payload):
    payload["targets"][0].update(lower=13, upper=13)
    result = verify_sandbox(SandboxSpec.model_validate(payload))
    assert not result.valid
    assert result.states_checked == 11


def test_unsafe_state_after_a_valid_witness_is_still_rejected(payload):
    payload["targets"][0].update(lower=1, upper=2)
    payload["targets"][0]["expression"] = {
        "kind": "operation", "op": "divide", "args": [
            {"kind": "constant", "value": 10},
            {"kind": "operation", "op": "subtract", "args": [
                {"kind": "constant", "value": 10}, {"kind": "variable", "name": "power"},
            ]},
        ],
    }
    result = verify_sandbox(SandboxSpec.model_validate(payload))
    assert not result.valid
    assert result.states_checked == 11
    assert result.witness is None
    assert "division" in result.reason


def test_all_targets_require_a_joint_witness(payload):
    target = copy.deepcopy(payload["targets"][0])
    target.update(name="other", lower=2, upper=4)
    payload["targets"].append(target)
    assert not verify_sandbox(SandboxSpec.model_validate(payload)).valid


@pytest.mark.parametrize("value", [float("nan"), float("inf"), -float("inf")])
def test_non_finite_input_is_rejected(payload, value):
    payload["parameters"][0]["maximum"] = value
    with pytest.raises(ValidationError):
        SandboxSpec.model_validate(payload)


@pytest.mark.parametrize("field", ["threejs_code", "execution_mode", "url", "eval"])
def test_unknown_execution_fields_rejected(payload, field):
    payload[field] = "arbitrary code"
    with pytest.raises(ValidationError):
        SandboxSpec.model_validate(payload)


@pytest.mark.parametrize("op", ["eval", "exec", "pow", "fetch", "__import__"])
def test_unknown_operations_rejected(payload, op):
    payload["targets"][0]["expression"]["op"] = op
    with pytest.raises(ValidationError):
        SandboxSpec.model_validate(payload)


def test_undefined_variables_rejected(payload):
    payload["targets"][0]["expression"]["args"][0]["name"] = "secret"
    with pytest.raises(ValidationError, match="undefined variable"):
        SandboxSpec.model_validate(payload)


def test_incompatible_modifier_rejected(payload):
    payload["modifier"] = "SPEED_BLITZ"
    with pytest.raises(ValidationError):
        SandboxSpec.model_validate(payload)


def test_excessive_grid_is_rejected(payload):
    payload["parameters"] *= 4
    for index, parameter in enumerate(payload["parameters"]):
        payload["parameters"][index] = {**parameter, "name": f"v{index}"}
    payload["targets"][0]["expression"] = {"kind": "constant", "value": 12}
    with pytest.raises(ValidationError, match="4096"):
        SandboxSpec.model_validate(payload)


def test_deep_expression_is_rejected(payload):
    expression = {"kind": "constant", "value": 1}
    for _ in range(10):
        expression = {"kind": "operation", "op": "add", "args": [expression, {"kind": "constant", "value": 1}]}
    payload["targets"][0]["expression"] = expression
    with pytest.raises(ValidationError, match="depth/node"):
        SandboxSpec.model_validate(payload)


def test_large_intermediate_values_are_rejected(payload):
    payload["targets"][0]["expression"] = {"kind": "operation", "op": "multiply", "args": [{"kind": "constant", "value": 1_000_000}] * 4}
    assert not verify_sandbox(SandboxSpec.model_validate(payload)).valid


def test_decimal_grid_does_not_lose_its_endpoint():
    parameter = ParameterSpec(name="x", label="X", minimum=0, maximum=0.3, step=0.1, initial=0.2)
    assert parameter.values() == [0.0, 0.1, 0.2, 0.3]


@pytest.mark.parametrize("changes", [{"step": 0}, {"step": -1}, {"maximum": 1.1, "step": 0.3}, {"initial": 0.15}, {"initial": 11}, {"maximum": 0}])
def test_invalid_parameter_domains(payload, changes):
    payload["parameters"][0].update(changes)
    with pytest.raises(ValidationError):
        SandboxSpec.model_validate(payload)
