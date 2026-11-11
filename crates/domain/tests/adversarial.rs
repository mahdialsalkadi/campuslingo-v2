use campuslingo_domain::{
    contracts::{ParameterSpec, SandboxSpec},
    evaluate_json,
    memory::{ShieldInput, shield},
    sandbox,
};
use serde_json::{Value, json};

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../../tests/fixtures/sandbox.valid.json")).unwrap()
}

fn rejected(value: Value) -> bool {
    match serde_json::from_value::<SandboxSpec>(value) {
        Err(_) => true,
        Ok(spec) => !matches!(sandbox::verify(&spec), Ok(result) if result.valid),
    }
}

#[test]
fn incompatible_modifiers_duplicate_names_and_timing_are_rejected() {
    let mut value = fixture();
    value["modifier"] = json!("SPEED_BLITZ");
    assert!(rejected(value));
    let mut value = fixture();
    let duplicate = value["parameters"][0].clone();
    value["parameters"].as_array_mut().unwrap().push(duplicate);
    assert!(rejected(value));
    for limit in [0, 999, 30001, 1000000] {
        let mut value = fixture();
        value["time_limit_ms"] = json!(limit);
        assert!(rejected(value));
    }
}

#[test]
fn invalid_grids_and_off_grid_initial_values_are_rejected() {
    for (field, replacement) in [
        ("step", json!(0)),
        ("step", json!(-1)),
        ("initial", json!(0.15)),
        ("initial", json!(11)),
        ("maximum", json!(0)),
    ] {
        let mut value = fixture();
        value["parameters"][0][field] = replacement;
        assert!(rejected(value));
    }
    let mut value = fixture();
    value["parameters"][0]["step"] = json!(0.3);
    assert!(rejected(value));
}

#[test]
fn decimal_endpoint_is_preserved() {
    let parameter = ParameterSpec {
        name: "x".into(),
        label: "X".into(),
        minimum: 0.0,
        maximum: 0.3,
        step: 0.1,
        initial: 0.2,
    };
    let values = parameter.values().unwrap();
    assert_eq!(values.len(), 4);
    assert_eq!(values[3], 0.3);
}

#[test]
fn excessive_grid_product_and_deep_trees_are_rejected() {
    let mut value = fixture();
    let parameters = (0..4)
        .map(|index| {
            let mut parameter = value["parameters"][0].clone();
            parameter["name"] = json!(format!("v{index}"));
            parameter
        })
        .collect();
    value["parameters"] = Value::Array(parameters);
    assert!(rejected(value));
    let mut expression = json!({"kind":"constant", "value":1});
    for _ in 0..10 {
        expression = json!({"kind":"operation", "op":"add", "args":[expression,{"kind":"constant", "value":1}]});
    }
    let mut value = fixture();
    value["targets"][0]["expression"] = expression;
    assert!(rejected(value));
}

#[test]
fn unsafe_intermediate_and_invalid_arity_are_rejected() {
    let mut value = fixture();
    value["targets"][0]["expression"] = json!({"kind":"operation", "op":"multiply", "args":[{"kind":"constant","value":1000000},{"kind":"constant","value":1000000},{"kind":"constant","value":1000000}]});
    assert!(rejected(value));
    for args in [
        vec![],
        vec![json!({"kind":"constant","value":1})],
        vec![json!({"kind":"constant","value":1}); 3],
    ] {
        let mut value = fixture();
        value["targets"][0]["expression"] = json!({"kind":"operation", "op":"divide", "args":args});
        assert!(rejected(value));
    }
}

#[test]
fn all_targets_must_have_one_joint_witness() {
    let mut value = fixture();
    let mut second = value["targets"][0].clone();
    second["name"] = json!("other");
    second["lower"] = json!(2);
    second["upper"] = json!(4);
    value["targets"].as_array_mut().unwrap().push(second);
    assert!(rejected(value));
}

#[test]
fn continuous_shields_are_not_rounded_to_scheduler_days() {
    let at_zero = shield(&ShieldInput {
        stability: 5.0,
        elapsed_days: 0.0,
    })
    .unwrap();
    let halfway = shield(&ShieldInput {
        stability: 5.0,
        elapsed_days: 0.5,
    })
    .unwrap();
    let at_stability = shield(&ShieldInput {
        stability: 5.0,
        elapsed_days: 5.0,
    })
    .unwrap();
    assert_eq!(at_zero, 1.0);
    assert!(halfway < at_zero);
    assert!((at_stability - 0.9).abs() <= 1e-6);
    assert_eq!(
        shield(&ShieldInput {
            stability: 0.0,
            elapsed_days: 0.0
        })
        .unwrap(),
        0.0
    );
}

#[test]
fn fractional_scheduler_days_and_non_json_numbers_fail_closed() {
    let request = json!({"kind":"review","input":{"memory":null,"elapsed_days":0.5,"rating":3,"desired_retention":0.9}});
    assert_eq!(
        serde_json::from_str::<Value>(&evaluate_json(&request.to_string())).unwrap()["kind"],
        "error"
    );
    for raw in [
        "NaN",
        "Infinity",
        "{\"kind\":\"shield\",\"input\":{\"stability\":1e400,\"elapsed_days\":0}}",
    ] {
        assert_eq!(
            serde_json::from_str::<Value>(&evaluate_json(raw)).unwrap()["kind"],
            "error"
        );
    }
}
