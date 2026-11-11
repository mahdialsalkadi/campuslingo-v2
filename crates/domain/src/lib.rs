//! Canonical Rust contracts and validation, shared by the server and WASM client.

pub const SCHEMA_VERSION: &str = "1.0.0";

pub mod combat;
pub mod contracts;
pub mod memory;
pub mod sandbox;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EvaluationRequest {
    Sandbox { spec: contracts::SandboxSpec },
    Review { input: memory::ReviewInput },
    Shield { input: memory::ShieldInput },
    Deploy { input: combat::DeploymentInput },
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EvaluationResponse {
    Sandbox { result: sandbox::ValidationResult },
    Review { result: memory::ReviewOutput },
    Shield { retrievability: f32 },
    Deploy { result: combat::DeploymentOutput },
    Error { error: String },
}

pub fn evaluate(request: EvaluationRequest) -> EvaluationResponse {
    match request {
        EvaluationRequest::Sandbox { spec } => match sandbox::verify(&spec) {
            Ok(result) => EvaluationResponse::Sandbox { result },
            Err(error) => EvaluationResponse::Error { error },
        },
        EvaluationRequest::Review { input } => match memory::review(&input) {
            Ok(result) => EvaluationResponse::Review { result },
            Err(error) => EvaluationResponse::Error { error },
        },
        EvaluationRequest::Shield { input } => match memory::shield(&input) {
            Ok(retrievability) => EvaluationResponse::Shield { retrievability },
            Err(error) => EvaluationResponse::Error { error },
        },
        EvaluationRequest::Deploy { input } => match combat::validate_deployment(&input) {
            Ok(result) => EvaluationResponse::Deploy { result },
            Err(error) => EvaluationResponse::Error { error },
        },
    }
}

pub fn evaluate_json(input: &str) -> String {
    let result = if input.len() > 32768 {
        EvaluationResponse::Error {
            error: "evaluation payload exceeds 32 KiB".into(),
        }
    } else {
        match serde_json::from_str(input) {
            Ok(request) => evaluate(request),
            Err(_) => EvaluationResponse::Error {
                error: "invalid evaluation contract".into(),
            },
        }
    };
    serde_json::to_string(&result).expect("validated finite domain result")
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(js_name = evaluateJson)]
pub fn wasm_evaluate_json(input: &str) -> String {
    evaluate_json(input)
}
