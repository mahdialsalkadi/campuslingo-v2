//! Canonical Rust contracts and validation, shared by the server and WASM client.

pub const SCHEMA_VERSION: &str = "1.0.0";

pub mod combat;
pub mod contracts;
pub mod memory;
pub mod notifications;
pub mod raids_leagues;
pub mod sandbox;
pub mod shields;
pub mod windows;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EvaluationRequest {
    Sandbox {
        spec: contracts::SandboxSpec,
    },
    SandboxState {
        spec: contracts::SandboxSpec,
        values: std::collections::BTreeMap<String, f64>,
    },
    Review {
        input: memory::ReviewInput,
    },
    Shield {
        input: memory::ShieldInput,
    },
    ShieldDag {
        input: shields::CourseShieldInput,
    },
    Deploy {
        input: combat::DeploymentInput,
    },
    RatingMapV2 {
        input: combat::RatingMapV2Input,
    },
    ScoreV2 {
        input: combat::ScoreV2Input,
    },
    DuelStrike {
        arena: combat::DuelArenaState,
        strike: combat::CombatStrikeInput,
    },
    RaidTargetability {
        input: raids_leagues::RaidTargetabilityInput,
    },
    RaidSettlement {
        input: raids_leagues::RaidSettlementInput,
    },
    ClanStability {
        input: raids_leagues::ClanStabilityInput,
    },
    LeagueRollover {
        input: raids_leagues::LeagueBracketRolloverInput,
    },
    BossStrike {
        input: raids_leagues::BossStrikeInput,
    },
    QuietHours {
        local_hour: u8,
        local_minute: u8,
        local_second: u8,
    },
    NotificationPlan {
        prefs: notifications::NotificationPreferencesInput,
        raids: Vec<notifications::InboundRaidNotificationEvent>,
        shield_digest: Option<notifications::PreExpiryShieldDigestInput>,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EvaluationResponse {
    Sandbox {
        result: sandbox::ValidationResult,
    },
    SandboxState {
        result: sandbox::StateEvaluation,
    },
    Review {
        result: memory::ReviewOutput,
    },
    Shield {
        retrievability: f32,
    },
    ShieldDag {
        result: shields::CourseShieldOutput,
    },
    Deploy {
        result: combat::DeploymentOutput,
    },
    RatingMapV2 {
        result: combat::RatingOutcome,
    },
    ScoreV2 {
        result: combat::ScoreV2Output,
    },
    DuelStrike {
        arena: combat::DuelArenaState,
        result: combat::CombatStrikeResult,
    },
    RaidTargetability {
        result: raids_leagues::RaidTargetabilityOutcome,
    },
    RaidSettlement {
        result: raids_leagues::RaidSettlementOutcome,
    },
    ClanStability {
        result: raids_leagues::ClanStabilityOutput,
    },
    LeagueRollover {
        result: raids_leagues::LeagueBracketRolloverOutput,
    },
    BossStrike {
        result: raids_leagues::BossStrikeOutcome,
    },
    QuietHours {
        result: notifications::QuietHoursWindowEvaluation,
    },
    NotificationPlan {
        result: notifications::AggregatedDailyNotificationPlan,
    },
    Error {
        error: String,
    },
}

pub fn evaluate(request: EvaluationRequest) -> EvaluationResponse {
    match request {
        EvaluationRequest::Sandbox { spec } => match sandbox::verify(&spec) {
            Ok(result) => EvaluationResponse::Sandbox { result },
            Err(error) => EvaluationResponse::Error { error },
        },
        EvaluationRequest::SandboxState { spec, values } => {
            match sandbox::evaluate_state(&spec, &values) {
                Ok(result) => EvaluationResponse::SandboxState { result },
                Err(error) => EvaluationResponse::Error { error },
            }
        }
        EvaluationRequest::Review { input } => match memory::review(&input) {
            Ok(result) => EvaluationResponse::Review { result },
            Err(error) => EvaluationResponse::Error { error },
        },
        EvaluationRequest::Shield { input } => match memory::shield(&input) {
            Ok(retrievability) => EvaluationResponse::Shield { retrievability },
            Err(error) => EvaluationResponse::Error { error },
        },
        EvaluationRequest::ShieldDag { input } => match shields::evaluate_course_shields(&input) {
            Ok(result) => EvaluationResponse::ShieldDag { result },
            Err(error) => EvaluationResponse::Error { error },
        },
        EvaluationRequest::Deploy { input } => match combat::validate_deployment(&input) {
            Ok(result) => EvaluationResponse::Deploy { result },
            Err(error) => EvaluationResponse::Error { error },
        },
        EvaluationRequest::RatingMapV2 { input } => match combat::evaluate_rating_map_v2(&input) {
            Ok(result) => EvaluationResponse::RatingMapV2 { result },
            Err(error) => EvaluationResponse::Error { error },
        },
        EvaluationRequest::ScoreV2 { input } => match combat::compute_score_v2(&input) {
            Ok(result) => EvaluationResponse::ScoreV2 { result },
            Err(error) => EvaluationResponse::Error { error },
        },
        EvaluationRequest::DuelStrike { mut arena, strike } => {
            match arena.resolve_strike(&strike) {
                Ok(result) => EvaluationResponse::DuelStrike { arena, result },
                Err(error) => EvaluationResponse::Error { error },
            }
        }
        EvaluationRequest::RaidTargetability { input } => EvaluationResponse::RaidTargetability {
            result: raids_leagues::evaluate_raid_targetability(&input),
        },
        EvaluationRequest::RaidSettlement { input } => {
            match raids_leagues::evaluate_raid_settlement(&input) {
                Ok(result) => EvaluationResponse::RaidSettlement { result },
                Err(error) => EvaluationResponse::Error { error },
            }
        }
        EvaluationRequest::ClanStability { input } => {
            match raids_leagues::compute_clan_stability_points(&input) {
                Ok(result) => EvaluationResponse::ClanStability { result },
                Err(error) => EvaluationResponse::Error { error },
            }
        }
        EvaluationRequest::LeagueRollover { input } => {
            match raids_leagues::evaluate_league_rollover(&input) {
                Ok(result) => EvaluationResponse::LeagueRollover { result },
                Err(error) => EvaluationResponse::Error { error },
            }
        }
        EvaluationRequest::BossStrike { input } => {
            match raids_leagues::evaluate_boss_strike(&input) {
                Ok(result) => EvaluationResponse::BossStrike { result },
                Err(error) => EvaluationResponse::Error { error },
            }
        }
        EvaluationRequest::QuietHours {
            local_hour,
            local_minute,
            local_second,
        } => match notifications::evaluate_quiet_hours(local_hour, local_minute, local_second) {
            Ok(result) => EvaluationResponse::QuietHours { result },
            Err(error) => EvaluationResponse::Error { error },
        },
        EvaluationRequest::NotificationPlan {
            prefs,
            raids,
            shield_digest,
        } => {
            match notifications::plan_daily_notifications(&prefs, &raids, shield_digest.as_ref()) {
                Ok(result) => EvaluationResponse::NotificationPlan { result },
                Err(error) => EvaluationResponse::Error { error },
            }
        }
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
