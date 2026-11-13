use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::{
    Json,
    extract::{
        Path, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode, Uri, header},
    response::Response,
};
use campuslingo_domain::combat::RATING_MAP_VERSION;
use campuslingo_domain::raids_leagues::{
    MAX_INBOUND_RAIDS_PER_DAY, MAX_OUTBOUND_RAIDS_PER_DAY, MAX_UNORDERED_PAIR_RAIDS_DAY,
};
use campuslingo_storage::m6::{
    NaiveDate, UpsertLeagueScoreInput, Utc, claim_boss_chest, create_chapter_boss, create_clan,
    create_or_join_boss_lobby, get_boss_lobby, get_chapter_boss, get_clan_war_state,
    get_league_bracket, get_user_economy_and_cosmetics, get_user_notifications, join_clan,
    list_chapter_bosses, record_clan_war_stability_or_raid, rollover_league_bracket,
    scan_raid_targets, set_course_raid_opt_in, settle_raid_session, start_clan_war_week,
    start_raid_session, strike_chapter_boss, upsert_league_score,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::{Mutex, mpsc};
use uuid::Uuid;

use crate::AppState;
use crate::auth::require_actor;
use crate::duels::{WS_MAX_PAYLOAD_BYTES, WS_OUTBOUND_QUEUE_CAP};
use crate::error::ApiError;

#[derive(Default)]
pub struct BossLobbyHub {
    subscribers: Mutex<HashMap<Uuid, Vec<mpsc::Sender<Value>>>>,
    tx_seq: AtomicU64,
}

impl BossLobbyHub {
    pub fn new() -> Self {
        Self::default()
    }

    async fn subscribe(&self, lobby_id: Uuid) -> mpsc::Receiver<Value> {
        let (tx, rx) = mpsc::channel(WS_OUTBOUND_QUEUE_CAP);
        let mut guard = self.subscribers.lock().await;
        guard.entry(lobby_id).or_default().push(tx);
        rx
    }

    pub async fn broadcast(&self, lobby_id: Uuid, event: Value) {
        let mut guard = self.subscribers.lock().await;
        let Some(list) = guard.get_mut(&lobby_id) else {
            return;
        };
        list.retain(|tx| match tx.try_send(event.clone()) {
            Ok(()) => true,
            Err(mpsc::error::TrySendError::Full(_)) => true,
            Err(mpsc::error::TrySendError::Closed(_)) => false,
        });
    }

    fn next_seq(&self) -> u64 {
        self.tx_seq.fetch_add(1, Ordering::Relaxed) + 1
    }
}

fn validate_boss_ws_origin(state: &AppState, headers: &HeaderMap) -> bool {
    let origin = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok());
    let Some(origin_value) = origin else {
        return true;
    };
    if let Some(allowed) = state.settings.allowed_origin.as_deref() {
        return origin_value == allowed;
    }
    let Some(host) = headers.get(header::HOST).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let Ok(uri) = origin_value.parse::<Uri>() else {
        return false;
    };
    match (uri.scheme_str(), uri.authority().map(|a| a.as_str())) {
        (Some("http" | "https"), Some(authority)) => authority == host,
        _ => false,
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RaidOptInRequest {
    pub opt_in: bool,
}

pub async fn set_raid_opt_in_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(course_id): Path<Uuid>,
    Json(body): Json<RaidOptInRequest>,
) -> Result<Json<Value>, ApiError> {
    let actor = require_actor(&state, &headers).await?;
    set_course_raid_opt_in(&state.app_pool, actor, course_id, body.opt_in).await?;
    Ok(Json(json!({
        "course_id": course_id,
        "raid_opt_in": body.opt_in,
    })))
}

pub async fn scan_raids_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(course_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let actor = require_actor(&state, &headers).await?;
    let candidates = scan_raid_targets(&state.app_pool, actor, course_id, Utc::now()).await?;
    Ok(Json(json!({
        "course_id": course_id,
        "limits": {
            "max_outbound_per_day": MAX_OUTBOUND_RAIDS_PER_DAY,
            "max_inbound_per_day": MAX_INBOUND_RAIDS_PER_DAY,
            "max_unordered_pair_per_day": MAX_UNORDERED_PAIR_RAIDS_DAY,
        },
        "candidates": candidates,
    })))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartRaidRequest {
    pub defender_user_id: Uuid,
    pub defender_node_id: Uuid,
    #[serde(default)]
    pub is_revenge: bool,
    pub revenge_source_raid_id: Option<Uuid>,
    pub settlement_key: Option<String>,
}

pub async fn start_raid_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(course_id): Path<Uuid>,
    Json(body): Json<StartRaidRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let actor = require_actor(&state, &headers).await?;
    let raid_id = Uuid::new_v4();
    let settlement_key = body
        .settlement_key
        .unwrap_or_else(|| format!("raid:{raid_id}"));
    let rec = start_raid_session(
        &state.app_pool,
        actor,
        raid_id,
        course_id,
        body.defender_user_id,
        body.defender_node_id,
        body.is_revenge,
        body.revenge_source_raid_id,
        &settlement_key,
        Utc::now(),
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "raid": rec,
            "rating_map_version": RATING_MAP_VERSION,
        })),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettleRaidRequest {
    pub sandbox_solved: bool,
    pub drills_passed: i16,
    pub base_score_points: i32,
}

pub async fn settle_raid_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((course_id, raid_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<SettleRaidRequest>,
) -> Result<Json<Value>, ApiError> {
    let actor = require_actor(&state, &headers).await?;
    if !(0..=3).contains(&body.drills_passed) {
        return Err(ApiError::Invalid("drills_passed must be 0..=3"));
    }
    let settled = settle_raid_session(
        &state.app_pool,
        actor,
        raid_id,
        course_id,
        body.sandbox_solved,
        body.drills_passed,
        body.base_score_points.max(0),
        Utc::now(),
    )
    .await?;
    Ok(Json(json!({
        "raid_id": raid_id,
        "settlement": settled,
    })))
}

pub async fn list_notifications_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(course_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let actor = require_actor(&state, &headers).await?;
    let notifications = get_user_notifications(&state.app_pool, actor, course_id).await?;
    Ok(Json(json!({ "notifications": notifications })))
}

pub async fn get_league_bracket_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((course_id, week_key)): Path<(Uuid, String)>,
    uri: Uri,
) -> Result<Json<Value>, ApiError> {
    let actor = require_actor(&state, &headers).await?;
    let mut scope: Option<String> = None;
    let mut tier: Option<i16> = None;
    if let Some(raw_q) = uri.query() {
        for pair in raw_q.split('&') {
            let mut kv = pair.splitn(2, '=');
            let key = kv.next().unwrap_or("");
            let val = kv.next().unwrap_or("");
            if key == "scope" && !val.is_empty() {
                scope = Some(val.to_string());
            } else if key == "tier" && !val.is_empty() {
                tier = Some(
                    val.parse::<i16>()
                        .map_err(|_| ApiError::Invalid("invalid tier"))?,
                );
            }
        }
    }
    let scope = scope.unwrap_or_else(|| format!("COURSE:{course_id}"));
    let tier = tier.unwrap_or(1);
    let (bracket, standings) =
        get_league_bracket(&state.app_pool, actor, course_id, &scope, &week_key, tier).await?;
    Ok(Json(json!({
        "bracket": bracket,
        "standings": standings,
    })))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpsertLeagueScoreRequest {
    pub league_scope: Option<String>,
    pub week_key: String,
    pub week_start_date: NaiveDate,
    pub tier: i16,
    #[serde(default)]
    pub late_joiner_shielded: bool,
    pub timezone: Option<String>,
    #[serde(default)]
    pub trophies_delta: i32,
    #[serde(default)]
    pub recovery_delta: i32,
    #[serde(default)]
    pub is_dormant: bool,
    #[serde(default)]
    pub returned_from_dormancy_this_week: bool,
    #[serde(default)]
    pub qualifying_sessions_delta: i32,
    #[serde(default)]
    pub consecutive_dormant_weeks: i16,
}

pub async fn upsert_league_score_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(course_id): Path<Uuid>,
    Json(body): Json<UpsertLeagueScoreRequest>,
) -> Result<Json<Value>, ApiError> {
    let actor = require_actor(&state, &headers).await?;
    let league_scope = body
        .league_scope
        .unwrap_or_else(|| format!("COURSE:{course_id}"));
    let standing = upsert_league_score(
        &state.app_pool,
        actor,
        actor,
        UpsertLeagueScoreInput {
            league_scope,
            course_id,
            week_key: body.week_key,
            week_start_date: body.week_start_date,
            tier: body.tier,
            late_joiner_shielded: body.late_joiner_shielded,
            timezone: body.timezone.unwrap_or_else(|| "Asia/Kuala_Lumpur".into()),
            trophies_delta: body.trophies_delta,
            recovery_delta: body.recovery_delta,
            is_dormant: body.is_dormant,
            returned_from_dormancy_this_week: body.returned_from_dormancy_this_week,
            qualifying_sessions_delta: body.qualifying_sessions_delta,
            consecutive_dormant_weeks: body.consecutive_dormant_weeks,
            now: Utc::now(),
        },
    )
    .await?;
    Ok(Json(json!({ "standing": standing })))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeagueRolloverRequest {
    pub league_scope: Option<String>,
    pub week_key: String,
    pub tier: i16,
}

pub async fn rollover_league_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(course_id): Path<Uuid>,
    Json(body): Json<LeagueRolloverRequest>,
) -> Result<Json<Value>, ApiError> {
    let actor = require_actor(&state, &headers).await?;
    let league_scope = body
        .league_scope
        .unwrap_or_else(|| format!("COURSE:{course_id}"));
    let summary = rollover_league_bracket(
        &state.app_pool,
        actor,
        course_id,
        &league_scope,
        &body.week_key,
        body.tier,
    )
    .await?;
    Ok(Json(json!(summary)))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateBossRequest {
    pub node_id: Uuid,
    pub title: String,
    pub mode: String,
    pub max_hp: i32,
    pub gold_reward: Option<i32>,
    pub cosmetic_reward_key: Option<String>,
}

pub async fn create_boss_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(course_id): Path<Uuid>,
    Json(body): Json<CreateBossRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let actor = require_actor(&state, &headers).await?;
    let boss = create_chapter_boss(
        &state.app_pool,
        actor,
        course_id,
        body.node_id,
        &body.title,
        &body.mode,
        body.max_hp,
        body.gold_reward.unwrap_or(150),
        body.cosmetic_reward_key.as_deref(),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(json!({ "boss": boss }))))
}

pub async fn list_bosses_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(course_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let actor = require_actor(&state, &headers).await?;
    let bosses = list_chapter_bosses(&state.app_pool, actor, course_id).await?;
    Ok(Json(json!({ "bosses": bosses })))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateOrJoinBossLobbyRequest {
    pub lobby_id: Option<Uuid>,
}

pub async fn create_or_join_boss_lobby_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((course_id, boss_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<CreateOrJoinBossLobbyRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let actor = require_actor(&state, &headers).await?;
    let lobby =
        create_or_join_boss_lobby(&state.app_pool, actor, course_id, boss_id, body.lobby_id)
            .await?;
    state
        .bosses
        .broadcast(
            lobby.id,
            json!({
                "type": "LOBBY_UPDATED",
                "seq": state.bosses.next_seq(),
                "lobby": lobby,
            }),
        )
        .await;
    Ok((StatusCode::CREATED, Json(json!({ "lobby": lobby }))))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BossStrikeRequest {
    pub lobby_id: Option<Uuid>,
    pub sequence: i32,
    pub rating: i16,
    #[serde(default)]
    pub overcharged: bool,
    pub base_damage: i32,
    pub settlement_key: Option<String>,
}

pub async fn strike_boss_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((course_id, boss_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<BossStrikeRequest>,
) -> Result<Json<Value>, ApiError> {
    let actor = require_actor(&state, &headers).await?;
    let settlement_key = body
        .settlement_key
        .unwrap_or_else(|| format!("boss-strike:{boss_id}:{actor}:{}", body.sequence));
    let strike = strike_chapter_boss(
        &state.app_pool,
        actor,
        course_id,
        boss_id,
        body.lobby_id,
        body.sequence,
        body.rating,
        body.overcharged,
        body.base_damage,
        &settlement_key,
    )
    .await?;

    if let Some(lobby_id) = body.lobby_id {
        state
            .bosses
            .broadcast(
                lobby_id,
                json!({
                    "type": "BOSS_HP_UPDATED",
                    "seq": state.bosses.next_seq(),
                    "boss_id": boss_id,
                    "actor_user_id": actor,
                    "strike": strike,
                }),
            )
            .await;
    }

    Ok(Json(json!({ "strike": strike })))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimBossChestRequest {
    pub settlement_key: Option<String>,
}

pub async fn claim_boss_chest_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((course_id, boss_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<ClaimBossChestRequest>,
) -> Result<Json<Value>, ApiError> {
    let actor = require_actor(&state, &headers).await?;
    let settlement_key = body
        .settlement_key
        .unwrap_or_else(|| format!("boss-chest:{boss_id}:{actor}"));
    let claim =
        claim_boss_chest(&state.app_pool, actor, course_id, boss_id, &settlement_key).await?;
    Ok(Json(json!({ "claim": claim })))
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
enum BossClientCommand {
    Heartbeat {
        client_ts_ms: u64,
    },
    Strike {
        sequence: i32,
        rating: i16,
        #[serde(default)]
        overcharged: bool,
        base_damage: i32,
    },
}

pub async fn boss_websocket(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, ApiError> {
    if !validate_boss_ws_origin(&state, &headers) {
        return Err(ApiError::Forbidden);
    }
    let mut lobby_id: Option<Uuid> = None;
    if let Some(raw_q) = uri.query() {
        for pair in raw_q.split('&') {
            let mut kv = pair.splitn(2, '=');
            let key = kv.next().unwrap_or("");
            let val = kv.next().unwrap_or("");
            if matches!(key, "user_id" | "actor_id" | "email") {
                return Err(ApiError::Invalid("identity query parameters are forbidden"));
            }
            if key == "lobby_id" {
                lobby_id =
                    Some(Uuid::parse_str(val).map_err(|_| ApiError::Invalid("invalid lobby_id"))?);
            }
        }
    }
    let lobby_id = lobby_id.ok_or(ApiError::Invalid("lobby_id query parameter is required"))?;
    let actor = require_actor(&state, &headers).await?;
    let lobby = get_boss_lobby(&state.app_pool, actor, lobby_id).await?;
    if !lobby.member_user_ids.contains(&actor) {
        return Err(ApiError::Forbidden);
    }

    Ok(ws
        .max_message_size(WS_MAX_PAYLOAD_BYTES)
        .max_frame_size(WS_MAX_PAYLOAD_BYTES)
        .on_upgrade(move |socket| {
            handle_boss_socket(
                state,
                lobby.course_id,
                lobby.boss_id,
                lobby_id,
                actor,
                socket,
            )
        }))
}

async fn handle_boss_socket(
    state: AppState,
    course_id: Uuid,
    boss_id: Uuid,
    lobby_id: Uuid,
    actor: Uuid,
    mut socket: WebSocket,
) {
    let mut rx = state.bosses.subscribe(lobby_id).await;
    if let Ok(boss) = get_chapter_boss(&state.app_pool, actor, course_id, boss_id).await {
        let _ = socket
            .send(Message::Text(
                json!({
                    "type": "BOSS_WELCOME",
                    "seq": state.bosses.next_seq(),
                    "lobby_id": lobby_id,
                    "boss": boss,
                })
                .to_string()
                .into(),
            ))
            .await;
    }

    loop {
        tokio::select! {
            maybe_out = rx.recv() => {
                match maybe_out {
                    Some(ev) => {
                        if socket.send(Message::Text(ev.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            maybe_in = socket.recv() => {
                match maybe_in {
                    Some(Ok(Message::Text(text))) => {
                        if text.len() > WS_MAX_PAYLOAD_BYTES {
                            let _ = socket.send(Message::Text(json!({ "type": "ERROR", "code": "PAYLOAD_TOO_LARGE" }).to_string().into())).await;
                            break;
                        }
                        match serde_json::from_str::<BossClientCommand>(&text) {
                            Ok(BossClientCommand::Heartbeat { client_ts_ms }) => {
                                let _ = socket.send(Message::Text(json!({
                                    "type": "HEARTBEAT_ACK",
                                    "client_ts_ms": client_ts_ms,
                                    "seq": state.bosses.next_seq(),
                                }).to_string().into())).await;
                            }
                            Ok(BossClientCommand::Strike { sequence, rating, overcharged, base_damage }) => {
                                let key = format!("boss-ws:{boss_id}:{actor}:{sequence}");
                                match strike_chapter_boss(&state.app_pool, actor, course_id, boss_id, Some(lobby_id), sequence, rating, overcharged, base_damage, &key).await {
                                    Ok(strike) => {
                                        let event = json!({
                                            "type": "BOSS_HP_UPDATED",
                                            "seq": state.bosses.next_seq(),
                                            "boss_id": boss_id,
                                            "actor_user_id": actor,
                                            "strike": strike,
                                        });
                                        state.bosses.broadcast(lobby_id, event).await;
                                    }
                                    Err(_) => {
                                        let _ = socket.send(Message::Text(json!({ "type": "ERROR", "code": "STRIKE_REJECTED" }).to_string().into())).await;
                                    }
                                }
                            }
                            Err(_) => {
                                let _ = socket.send(Message::Text(json!({ "type": "ERROR", "code": "INVALID_COMMAND" }).to_string().into())).await;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateClanRequest {
    pub name: String,
}

pub async fn create_clan_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(course_id): Path<Uuid>,
    Json(body): Json<CreateClanRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let actor = require_actor(&state, &headers).await?;
    let clan = create_clan(&state.app_pool, actor, course_id, body.name.trim()).await?;
    Ok((StatusCode::CREATED, Json(json!({ "clan": clan }))))
}

pub async fn join_clan_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((course_id, clan_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, ApiError> {
    let actor = require_actor(&state, &headers).await?;
    join_clan(&state.app_pool, actor, course_id, clan_id).await?;
    Ok(Json(json!({ "clan_id": clan_id, "joined": true })))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartClanWarRequest {
    pub week_key: String,
    pub week_start_date: NaiveDate,
}

pub async fn start_clan_war_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((course_id, clan_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<StartClanWarRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let actor = require_actor(&state, &headers).await?;
    let war = start_clan_war_week(
        &state.app_pool,
        actor,
        course_id,
        clan_id,
        &body.week_key,
        body.week_start_date,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(json!({ "war": war }))))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContributeClanWarRequest {
    pub contribution_kind: String,
    pub s_before: Option<f64>,
    pub s_after: Option<f64>,
    #[serde(default = "default_true")]
    pub reward_eligible: bool,
    pub settlement_key: String,
}

fn default_true() -> bool {
    true
}

pub async fn contribute_clan_war_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((course_id, _clan_id, war_id)): Path<(Uuid, Uuid, Uuid)>,
    Json(body): Json<ContributeClanWarRequest>,
) -> Result<Json<Value>, ApiError> {
    let actor = require_actor(&state, &headers).await?;
    let war = record_clan_war_stability_or_raid(
        &state.app_pool,
        actor,
        course_id,
        war_id,
        &body.contribution_kind,
        body.s_before,
        body.s_after,
        body.reward_eligible,
        &body.settlement_key,
    )
    .await?;
    Ok(Json(json!({ "war": war })))
}

pub async fn get_clan_war_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((course_id, _clan_id, war_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<Value>, ApiError> {
    let actor = require_actor(&state, &headers).await?;
    let (war, contributions) =
        get_clan_war_state(&state.app_pool, actor, course_id, war_id).await?;
    Ok(Json(json!({
        "war": war,
        "contributions": contributions,
    })))
}

pub async fn get_economy_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(course_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let actor = require_actor(&state, &headers).await?;
    let overview = get_user_economy_and_cosmetics(&state.app_pool, actor, course_id).await?;
    Ok(Json(json!(overview)))
}
