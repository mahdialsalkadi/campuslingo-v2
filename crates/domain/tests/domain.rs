use campuslingo_domain::{
    combat::{DeploymentInput, validate_deployment},
    contracts::SandboxSpec,
    evaluate_json,
    memory::{ReviewInput, fsrs6, review},
    sandbox,
};
use fsrs::ModelVersion;
use serde_json::{Value, json};

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../../tests/fixtures/sandbox.valid.json")).unwrap()
}

#[test]
fn explicitly_uses_fsrs6_not_the_crates_fsrs7_default() {
    assert_eq!(fsrs6().version(), ModelVersion::Fsrs6);
    for rating in 1..=4 {
        let result = review(&ReviewInput {
            memory: None,
            elapsed_days: 0,
            rating,
            desired_retention: 0.9,
        })
        .unwrap();
        assert_eq!(result.algorithm, "FSRS-6");
        assert!(result.stability.is_finite() && result.interval_days.is_finite());
    }
}

#[test]
fn validates_every_declared_state_and_finds_a_joint_witness() {
    let spec: SandboxSpec = serde_json::from_value(fixture()).unwrap();
    let result = sandbox::verify(&spec).unwrap();
    assert!(result.valid);
    assert_eq!(result.states_checked, 11);
    assert_eq!(result.witness.unwrap()["power"], 6.0);
}

#[test]
fn continuous_but_not_quantized_solution_is_rejected() {
    let mut value = fixture();
    value["targets"][0]["lower"] = json!(13);
    value["targets"][0]["upper"] = json!(13);
    assert!(
        !sandbox::verify(&serde_json::from_value(value).unwrap())
            .unwrap()
            .valid
    );
}

#[test]
fn unsafe_state_after_a_witness_invalidates_the_whole_spec() {
    let mut value = fixture();
    value["targets"][0]["lower"] = json!(1);
    value["targets"][0]["upper"] = json!(2);
    value["targets"][0]["expression"] = json!({"kind":"operation", "op":"divide", "args":[{"kind":"constant", "value":10}, {"kind":"operation", "op":"subtract", "args":[{"kind":"constant", "value":10}, {"kind":"variable", "name":"power"}]}]});
    let result = sandbox::verify(&serde_json::from_value(value).unwrap()).unwrap();
    assert!(!result.valid);
    assert!(result.witness.is_none());
    assert_eq!(result.states_checked, 11);
}

#[test]
fn unknown_code_operations_and_fields_are_rejected() {
    for field in ["threejs_code", "execution_mode", "eval"] {
        let mut value = fixture();
        value[field] = json!("arbitrary code");
        assert!(serde_json::from_value::<SandboxSpec>(value).is_err());
    }
    let mut value = fixture();
    value["targets"][0]["expression"]["op"] = json!("eval");
    assert!(serde_json::from_value::<SandboxSpec>(value).is_err());
}

#[test]
fn excessive_domains_and_unknown_variables_fail_closed() {
    let mut value = fixture();
    value["parameters"][0]["maximum"] = json!(1000000);
    assert!(sandbox::verify(&serde_json::from_value(value).unwrap()).is_err());
    let mut value = fixture();
    value["targets"][0]["expression"]["args"][0]["name"] = json!("secret");
    assert!(sandbox::verify(&serde_json::from_value(value).unwrap()).is_err());
}

#[test]
fn invalid_grades_nan_and_negative_time_are_rejected() {
    for rating in [0, 5, 255] {
        assert!(
            review(&ReviewInput {
                memory: None,
                elapsed_days: 1,
                rating,
                desired_retention: 0.9
            })
            .is_err()
        );
    }
    for elapsed_days in [f32::NAN, f32::INFINITY, -1.0] {
        assert!(
            campuslingo_domain::memory::shield(&campuslingo_domain::memory::ShieldInput {
                stability: 5.0,
                elapsed_days,
            })
            .is_err()
        );
    }
}

#[test]
fn shared_combat_validation_bounds_energy_clock_lane_and_sequence() {
    let mut input = DeploymentInput {
        energy_milli: 5000,
        card_cost: 3,
        lane: 0,
        sequence: 1,
        last_sequence: Some(0),
        remaining_ms: 90000,
    };
    assert_eq!(
        validate_deployment(&input).unwrap().energy_after_milli,
        2000
    );
    input.sequence = 0;
    assert!(validate_deployment(&input).is_err());
    input.sequence = 1;
    input.energy_milli = 2000;
    assert!(validate_deployment(&input).is_err());
    input.energy_milli = 5000;
    input.lane = 2;
    assert!(validate_deployment(&input).is_err());
    input.lane = 0;
    input.remaining_ms = 0;
    assert!(validate_deployment(&input).is_err());
}

#[test]
fn native_and_wasm_entrypoint_has_bounded_untrusted_json() {
    assert!(evaluate_json(&" ".repeat(32769)).contains("exceeds"));
    for value in [
        json!({"kind":"exec", "code":"anything"}),
        json!({"kind":"review", "input":{"memory":null,"elapsed_days":0,"rating":3,"desired_retention":0.9,"reward":100}}),
    ] {
        assert_eq!(
            serde_json::from_str::<Value>(&evaluate_json(&value.to_string())).unwrap()["kind"],
            "error"
        );
    }
}
