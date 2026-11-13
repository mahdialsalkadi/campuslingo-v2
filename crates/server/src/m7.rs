use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, Uri},
};
use campuslingo_storage::m7::{
    DateTime, Utc, apply_versioned_retention_sweep, dispatch_due_notifications,
    enqueue_shield_digest_outbox, list_notification_outbox, pseudonymize_user_account,
    register_notification_device, upsert_notification_preferences,
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::auth::require_actor;
use crate::error::ApiError;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterDeviceRequest {
    pub platform: String,
    pub endpoint_or_token: String,
    pub p256dh_key: Option<String>,
    pub auth_secret: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpsertPreferencesRequest {
    pub push_enabled: bool,
    pub raid_alerts_enabled: bool,
    pub shield_digest_enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnqueueShieldDigestRequest {
    pub node_ids: Vec<Uuid>,
    pub now_epoch_secs: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchNotificationsRequest {
    pub now_epoch_secs: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutboxQuery {
    pub course_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PseudonymizeAccountRequest {
    pub reason: String,
    pub now_epoch_secs: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetentionSweepRequest {
    pub now_epoch_secs: Option<i64>,
}

fn parse_now(now_epoch_secs: Option<i64>) -> Result<DateTime<Utc>, ApiError> {
    match now_epoch_secs {
        Some(ts) => DateTime::from_timestamp(ts, 0)
            .ok_or(ApiError::Invalid("invalid now_epoch_secs timestamp")),
        None => Ok(Utc::now()),
    }
}

pub async fn register_device_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RegisterDeviceRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let actor = require_actor(&state, &headers).await?;
    let device_id = register_notification_device(
        &state.app_pool,
        actor,
        body.platform.trim(),
        body.endpoint_or_token.trim(),
        body.p256dh_key.as_deref(),
        body.auth_secret.as_deref(),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(json!({ "device_id": device_id }))))
}

pub async fn upsert_preferences_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<UpsertPreferencesRequest>,
) -> Result<Json<Value>, ApiError> {
    let actor = require_actor(&state, &headers).await?;
    let prefs = upsert_notification_preferences(
        &state.app_pool,
        actor,
        body.push_enabled,
        body.raid_alerts_enabled,
        body.shield_digest_enabled,
    )
    .await?;
    Ok(Json(json!({ "preferences": prefs })))
}

pub async fn list_outbox_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Value>, ApiError> {
    let actor = require_actor(&state, &headers).await?;
    let mut course_id: Option<Uuid> = None;
    if let Some(raw_q) = uri.query() {
        for pair in raw_q.split('&') {
            if pair.is_empty() {
                continue;
            }
            let mut kv = pair.splitn(2, '=');
            let key = kv.next().unwrap_or("");
            let val = kv.next().unwrap_or("");
            if matches!(key, "user_id" | "actor_id" | "role") {
                return Err(ApiError::Invalid("identity query parameters are forbidden"));
            }
            if key == "course_id" && !val.is_empty() {
                course_id =
                    Some(Uuid::parse_str(val).map_err(|_| ApiError::Invalid("invalid course_id"))?);
            }
        }
    }
    let items = list_notification_outbox(&state.app_pool, actor, course_id).await?;
    Ok(Json(json!({ "outbox": items })))
}

pub async fn enqueue_shield_digest_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(course_id): Path<Uuid>,
    Json(body): Json<EnqueueShieldDigestRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let actor = require_actor(&state, &headers).await?;
    let now = parse_now(body.now_epoch_secs)?;
    let outbox_id = enqueue_shield_digest_outbox(
        &state.app_pool,
        actor,
        actor,
        course_id,
        &body.node_ids,
        now,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(json!({ "outbox_id": outbox_id }))))
}

pub async fn dispatch_notifications_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<DispatchNotificationsRequest>,
) -> Result<Json<Value>, ApiError> {
    let actor = require_actor(&state, &headers).await?;
    let now = parse_now(body.now_epoch_secs)?;
    let delivered = dispatch_due_notifications(&state.app_pool, actor, actor, now).await?;
    Ok(Json(json!({ "delivered": delivered })))
}

pub async fn pseudonymize_me_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<PseudonymizeAccountRequest>,
) -> Result<Json<Value>, ApiError> {
    let actor = require_actor(&state, &headers).await?;
    let reason = body.reason.trim();
    if reason.is_empty() || reason.chars().count() > 240 {
        return Err(ApiError::Invalid("reason must be 1..=240 chars"));
    }
    let now = parse_now(body.now_epoch_secs)?;
    let outcome = pseudonymize_user_account(&state.app_pool, actor, actor, reason, now).await?;
    Ok(Json(json!({ "erasure": outcome })))
}

pub async fn retention_sweep_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(course_id): Path<Uuid>,
    Json(body): Json<RetentionSweepRequest>,
) -> Result<Json<Value>, ApiError> {
    let actor = require_actor(&state, &headers).await?;
    let now = parse_now(body.now_epoch_secs)?;
    let sweep = apply_versioned_retention_sweep(&state.app_pool, actor, course_id, now).await?;
    Ok(Json(json!({ "retention_sweep": sweep })))
}
