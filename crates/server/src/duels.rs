use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
use std::time::Duration;

use axum::{
    Json,
    extract::{
        Path, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode, Uri, header},
    response::Response,
};
use campuslingo_domain::combat::{
    AttemptTermination, CombatStrikeInput, DuelArenaState, DuelCardSlot, DuelMatchStatus, DuelSide,
    FighterArenaState, GhostReplayFrame, OpponentKind, ScoreV2Input, adapt_ghost_action,
    compute_score_v2, decide_bot_action,
};
use campuslingo_storage::duels::{
    Duration as ChronoDuration, MatchDeckCard, Utc, append_match_replay_event,
    find_permissioned_ghost_replay, get_match_summary, get_user_accommodation_ratio,
    grade_challenge_answer, load_live_duel_hand, mark_match_outage_freeze, record_duel_card_review,
    recover_or_void_interrupted_matches, save_match_snapshot, settle_match_atomic,
    start_match_record,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::time::{Instant, interval, timeout};
use uuid::Uuid;

use crate::AppState;
use crate::auth::require_actor;
use crate::error::ApiError;

pub const MATCHMAKING_WAIT_BUDGET_MS: u64 = 4_000;
pub const WS_HEARTBEAT_INTERVAL_SECS: u64 = 5;
pub const WS_CLIENT_TIMEOUT_SECS: u64 = 10;
pub const WS_RECONNECT_GRACE_SECS: i64 = 15;
pub const SNAPSHOT_INTERVAL_MS: u32 = 10_000;
pub const WS_OUTBOUND_QUEUE_CAP: usize = 32;
pub const WS_MAX_PAYLOAD_BYTES: usize = 4_096;
pub const WS_RATE_LIMIT_MAX_CMDS: usize = 16;
pub const WS_RATE_LIMIT_WINDOW_MS: u128 = 2_000;

struct WaitingPlayer {
    user_id: Uuid,
    cards: Vec<MatchDeckCard>,
    accommodation: (u32, u32),
    entry_cost_gems: i32,
    tx: oneshot::Sender<MatchJoinedNotification>,
}

struct MatchJoinedNotification {
    match_id: Uuid,
    side: DuelSide,
    opponent_kind: OpponentKind,
    projection: Value,
}

pub struct DuelRoom {
    pub match_id: Uuid,
    pub course_id: Uuid,
    pub player_a_id: Uuid,
    pub player_b_id: Option<Uuid>,
    pub opponent_kind: OpponentKind,
    pub arena: Mutex<DuelArenaState>,
    pub deck_a: Vec<MatchDeckCard>,
    pub deck_b: Vec<MatchDeckCard>,
    pub card_to_challenge: HashMap<String, Uuid>,
    pub ghost_frames: Vec<GhostReplayFrame>,
    pub next_ghost_idx: Mutex<usize>,
    pub bot_step: AtomicU64,
    pub snapshot_seq: AtomicI32,
    pub last_snapshot_clock_ms: Mutex<u32>,
    pub round_reveal_at: Mutex<HashMap<DuelSide, Instant>>,
    pub last_client_activity: Mutex<HashMap<DuelSide, Instant>>,
    pub disconnected_at: Mutex<HashMap<DuelSide, Instant>>,
    pub rate_windows: Mutex<HashMap<Uuid, VecDeque<Instant>>>,
    pub subscribers: Mutex<HashMap<DuelSide, Vec<mpsc::Sender<Value>>>>,
    pub tx_seq: AtomicU64,
}

impl DuelRoom {
    fn side_for_actor(&self, actor_id: Uuid) -> Option<DuelSide> {
        if actor_id == self.player_a_id {
            Some(DuelSide::PlayerA)
        } else if self.player_b_id == Some(actor_id) {
            Some(DuelSide::PlayerB)
        } else {
            None
        }
    }

    async fn check_rate_limit(&self, actor_id: Uuid) -> bool {
        let now = Instant::now();
        let mut windows = self.rate_windows.lock().await;
        let q = windows.entry(actor_id).or_default();
        while let Some(front) = q.front() {
            if now.duration_since(*front).as_millis() > WS_RATE_LIMIT_WINDOW_MS {
                q.pop_front();
            } else {
                break;
            }
        }
        if q.len() >= WS_RATE_LIMIT_MAX_CMDS {
            return false;
        }
        q.push_back(now);
        true
    }

    async fn broadcast(&self, event_type: &str, payload_for_a: Value, payload_for_b: Value) {
        let seq = self.tx_seq.fetch_add(1, Ordering::Relaxed) + 1;
        let mut subs = self.subscribers.lock().await;
        for (side, payload) in [
            (DuelSide::PlayerA, payload_for_a),
            (DuelSide::PlayerB, payload_for_b),
        ] {
            if let Some(list) = subs.get_mut(&side) {
                let msg = json!({
                    "type": event_type,
                    "seq": seq,
                    "match_id": self.match_id,
                    "payload": payload,
                });
                list.retain(|tx| tx.try_send(msg.clone()).is_ok());
            }
        }
    }
}

pub struct DuelHub {
    rooms: Mutex<HashMap<Uuid, Arc<DuelRoom>>>,
    queue: Mutex<HashMap<Uuid, Vec<WaitingPlayer>>>,
}

impl Default for DuelHub {
    fn default() -> Self {
        Self::new()
    }
}

impl DuelHub {
    pub fn new() -> Self {
        Self {
            rooms: Mutex::new(HashMap::new()),
            queue: Mutex::new(HashMap::new()),
        }
    }

    pub async fn get_room(&self, match_id: Uuid) -> Option<Arc<DuelRoom>> {
        self.rooms.lock().await.get(&match_id).cloned()
    }

    pub async fn insert_room(&self, room: Arc<DuelRoom>) {
        self.rooms.lock().await.insert(room.match_id, room);
    }

    pub async fn remove_room(&self, match_id: Uuid) {
        self.rooms.lock().await.remove(&match_id);
    }
}

fn to_duel_slots(
    prefix: &str,
    cards: &[MatchDeckCard],
) -> (Vec<DuelCardSlot>, HashMap<String, Uuid>) {
    let mut slots = Vec::with_capacity(cards.len());
    let mut map = HashMap::new();
    for (idx, c) in cards.iter().enumerate() {
        let card_id = format!("{prefix}-card-{idx}");
        map.insert(card_id.clone(), c.challenge_id);
        slots.push(DuelCardSlot {
            card_id,
            archetype: c.archetype.clone(),
            modifier: c.modifier.clone(),
            card_cost: c.card_cost,
            base_window_ms: c.base_window_ms,
        });
    }
    (slots, map)
}

/// `F-04` competition-projection privacy allowlist: never leaks opponent accommodations,
/// hidden card order/prompts outside the viewer's own hand, raw stability, or answers.
pub fn project_arena_for_actor(
    arena: &DuelArenaState,
    viewer_side: DuelSide,
    opponent_kind: OpponentKind,
    viewer_deck: &[MatchDeckCard],
) -> Value {
    let self_f = arena.fighter(viewer_side);
    let opp_f = arena.fighter(viewer_side.opponent());
    let hand_view: Vec<Value> = self_f
        .hand
        .iter()
        .enumerate()
        .map(|(i, slot)| {
            let meta = viewer_deck.get(i);
            json!({
                "card_id": slot.card_id,
                "archetype": slot.archetype,
                "modifier": slot.modifier,
                "card_cost": slot.card_cost,
                "base_window_ms": slot.base_window_ms,
                "prompt": meta.map(|m| m.prompt.as_str()).unwrap_or(""),
                "options": meta.map(|m| m.options.clone()).unwrap_or_default(),
            })
        })
        .collect();

    json!({
        "protocol_version": arena.protocol_version,
        "viewer_side": viewer_side,
        "opponent_kind": opponent_kind,
        "match_clock_ms": arena.match_clock_ms,
        "sudden_death_elapsed_ms": arena.sudden_death_elapsed_ms,
        "remaining_ms": arena.remaining_ms(),
        "double_energy": arena.is_double_energy(),
        "status": arena.status,
        "winner": arena.winner,
        "self": {
            "energy_milli": self_f.energy_milli,
            "tower_hp": self_f.tower_hp,
            "tower_shield": self_f.tower_shield,
            "core_hp": self_f.core_hp,
            "combat_level": self_f.combat_level,
            "last_sequence": self_f.last_sequence,
            "void_transport_count": self_f.void_transport_count,
            "too_fast_count": self_f.too_fast_count,
            "overcharge_count": self_f.overcharge_count,
            "quick_blitz_chain_available": self_f.quick_blitz_chain_available,
            "counter_strike_active": self_f.counter_strike_until_ms.is_some(),
            "selected_card_id": self_f.selected_card_id,
            "hand": hand_view,
        },
        "opponent": {
            "energy_milli": opp_f.energy_milli,
            "tower_hp": opp_f.tower_hp,
            "tower_shield": opp_f.tower_shield,
            "core_hp": opp_f.core_hp,
            "combat_level": opp_f.combat_level,
            "overcharge_count": opp_f.overcharge_count,
            "counter_strike_active": opp_f.counter_strike_until_ms.is_some(),
        }
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchmakeRequest {
    #[serde(default)]
    pub wait_budget_ms: Option<u64>,
    #[serde(default)]
    pub prefer_mode: Option<String>,
    #[serde(default)]
    pub entry_cost_gems: Option<i32>,
}

pub async fn matchmake_duel(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(course_id): Path<Uuid>,
    body: Option<Json<MatchmakeRequest>>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let actor = require_actor(&state, &headers).await?;
    let req = body.map(|b| b.0).unwrap_or(MatchmakeRequest {
        wait_budget_ms: None,
        prefer_mode: None,
        entry_cost_gems: None,
    });
    let wait_budget_ms = req
        .wait_budget_ms
        .unwrap_or(MATCHMAKING_WAIT_BUDGET_MS)
        .min(MATCHMAKING_WAIT_BUDGET_MS);
    let entry_cost_gems = req.entry_cost_gems.unwrap_or(0).clamp(0, 1_000);
    let prefer = req.prefer_mode.as_deref().unwrap_or("AUTO");

    let cards = load_live_duel_hand(&state.app_pool, actor, course_id, 6).await?;
    if cards.is_empty() {
        return Err(ApiError::Invalid("course has no duel-eligible cards"));
    }
    let accom_a = get_user_accommodation_ratio(&state.app_pool, actor).await?;

    if prefer == "AUTO" {
        // Attempt immediate human pairing
        let matched = {
            let mut q = state.duels.queue.lock().await;
            let list = q.entry(course_id).or_default();
            list.iter()
                .position(|w| w.user_id != actor)
                .map(|pos| list.remove(pos))
        };

        if let Some(waiter) = matched {
            let match_id = Uuid::new_v4();
            let (slots_a, mut map) = to_duel_slots("a", &waiter.cards);
            let (slots_b, map_b) = to_duel_slots("b", &cards);
            map.extend(map_b);
            let fa = FighterArenaState::new(
                DuelSide::PlayerA,
                waiter.accommodation.0,
                waiter.accommodation.1,
                slots_a,
            );
            let fb = FighterArenaState::new(DuelSide::PlayerB, accom_a.0, accom_a.1, slots_b);
            let arena = DuelArenaState::new(fa, fb);
            let settlement_key = format!("duel:{match_id}");

            start_match_record(
                &state.app_pool,
                waiter.user_id,
                match_id,
                course_id,
                waiter.user_id,
                Some(actor),
                "HUMAN",
                None,
                &settlement_key,
                waiter.entry_cost_gems.max(entry_cost_gems),
                &arena,
            )
            .await?;

            let proj_a = project_arena_for_actor(
                &arena,
                DuelSide::PlayerA,
                OpponentKind::Human,
                &waiter.cards,
            );
            let proj_b =
                project_arena_for_actor(&arena, DuelSide::PlayerB, OpponentKind::Human, &cards);

            let room = Arc::new(DuelRoom {
                match_id,
                course_id,
                player_a_id: waiter.user_id,
                player_b_id: Some(actor),
                opponent_kind: OpponentKind::Human,
                arena: Mutex::new(arena),
                deck_a: waiter.cards.clone(),
                deck_b: cards.clone(),
                card_to_challenge: map,
                ghost_frames: Vec::new(),
                next_ghost_idx: Mutex::new(0),
                bot_step: AtomicU64::new(0),
                snapshot_seq: AtomicI32::new(0),
                last_snapshot_clock_ms: Mutex::new(0),
                round_reveal_at: Mutex::new(HashMap::new()),
                last_client_activity: Mutex::new(HashMap::new()),
                disconnected_at: Mutex::new(HashMap::new()),
                rate_windows: Mutex::new(HashMap::new()),
                subscribers: Mutex::new(HashMap::new()),
                tx_seq: AtomicU64::new(0),
            });
            state.duels.insert_room(room).await;

            let _ = waiter.tx.send(MatchJoinedNotification {
                match_id,
                side: DuelSide::PlayerA,
                opponent_kind: OpponentKind::Human,
                projection: proj_a,
            });

            return Ok((
                StatusCode::CREATED,
                Json(json!({
                    "match_id": match_id,
                    "side": "PLAYER_B",
                    "opponent_kind": "HUMAN",
                    "arena": proj_b,
                })),
            ));
        }

        // Wait up to wait_budget_ms (<= 4000ms) for a human opponent before Ghost/Bot fallback
        if wait_budget_ms > 0 {
            let (tx, rx) = oneshot::channel();
            {
                let mut q = state.duels.queue.lock().await;
                q.entry(course_id).or_default().push(WaitingPlayer {
                    user_id: actor,
                    cards: cards.clone(),
                    accommodation: accom_a,
                    entry_cost_gems,
                    tx,
                });
            }

            if let Ok(Ok(notif)) = timeout(Duration::from_millis(wait_budget_ms), rx).await {
                return Ok((
                    StatusCode::CREATED,
                    Json(json!({
                        "match_id": notif.match_id,
                        "side": notif.side,
                        "opponent_kind": notif.opponent_kind,
                        "arena": notif.projection,
                    })),
                ));
            }

            // Timeout elapsed -> remove our waiting entry
            let mut q = state.duels.queue.lock().await;
            if let Some(list) = q.get_mut(&course_id) {
                list.retain(|w| w.user_id != actor);
            }
        }
    }

    // Fallback order: permissioned GHOST -> BOT
    let ghost = if prefer == "BOT" {
        None
    } else {
        find_permissioned_ghost_replay(&state.app_pool, actor, course_id).await?
    };

    let (opponent_kind, opponent_kind_str, ghost_source_id, ghost_frames) = match ghost {
        Some((source_id, frames)) => (OpponentKind::Ghost, "GHOST", Some(source_id), frames),
        None => (OpponentKind::Bot, "BOT", None, Vec::new()),
    };

    let match_id = Uuid::new_v4();
    let (slots_a, mut map) = to_duel_slots("a", &cards);
    let (slots_b, map_b) = to_duel_slots("b", &cards);
    map.extend(map_b);
    let fa = FighterArenaState::new(DuelSide::PlayerA, accom_a.0, accom_a.1, slots_a);
    let fb = FighterArenaState::new(DuelSide::PlayerB, 1, 1, slots_b);
    let arena = DuelArenaState::new(fa, fb);
    let settlement_key = format!("duel:{match_id}");

    start_match_record(
        &state.app_pool,
        actor,
        match_id,
        course_id,
        actor,
        None,
        opponent_kind_str,
        ghost_source_id,
        &settlement_key,
        entry_cost_gems,
        &arena,
    )
    .await?;

    let proj = project_arena_for_actor(&arena, DuelSide::PlayerA, opponent_kind, &cards);
    let room = Arc::new(DuelRoom {
        match_id,
        course_id,
        player_a_id: actor,
        player_b_id: None,
        opponent_kind,
        arena: Mutex::new(arena),
        deck_a: cards.clone(),
        deck_b: cards,
        card_to_challenge: map,
        ghost_frames,
        next_ghost_idx: Mutex::new(0),
        bot_step: AtomicU64::new(0),
        snapshot_seq: AtomicI32::new(0),
        last_snapshot_clock_ms: Mutex::new(0),
        round_reveal_at: Mutex::new(HashMap::new()),
        last_client_activity: Mutex::new(HashMap::new()),
        disconnected_at: Mutex::new(HashMap::new()),
        rate_windows: Mutex::new(HashMap::new()),
        subscribers: Mutex::new(HashMap::new()),
        tx_seq: AtomicU64::new(0),
    });
    state.duels.insert_room(room).await;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "match_id": match_id,
            "side": "PLAYER_A",
            "opponent_kind": opponent_kind,
            "ghost_source_match_id": ghost_source_id,
            "arena": proj,
        })),
    ))
}

fn validate_ws_origin(state: &AppState, headers: &HeaderMap) -> bool {
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

pub async fn duel_websocket(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, ApiError> {
    if !validate_ws_origin(&state, &headers) {
        return Err(ApiError::Forbidden);
    }
    // A-02: never trust client-supplied identity query parameters
    let mut match_id: Option<Uuid> = None;
    if let Some(raw_q) = uri.query() {
        for pair in raw_q.split('&') {
            let mut kv = pair.splitn(2, '=');
            let key = kv.next().unwrap_or("");
            let val = kv.next().unwrap_or("");
            if matches!(key, "user_id" | "actor_id" | "email") {
                return Err(ApiError::Invalid("identity query parameters are forbidden"));
            }
            if key == "match_id" {
                match_id =
                    Some(Uuid::parse_str(val).map_err(|_| ApiError::Invalid("invalid match_id"))?);
            }
        }
    }
    let match_id = match_id.ok_or(ApiError::Invalid("match_id query parameter is required"))?;

    let actor = require_actor(&state, &headers).await?;
    let Some(room) = state.duels.get_room(match_id).await else {
        return Err(ApiError::NotFound);
    };
    let Some(side) = room.side_for_actor(actor) else {
        return Err(ApiError::Forbidden);
    };

    Ok(ws
        .max_message_size(WS_MAX_PAYLOAD_BYTES)
        .max_frame_size(WS_MAX_PAYLOAD_BYTES)
        .on_upgrade(move |socket| handle_duel_socket(state, room, actor, side, socket)))
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
enum DuelClientCommand {
    Heartbeat {
        client_ts_ms: u64,
    },
    ResyncRequest {
        last_seen_seq: u64,
    },
    StartRound {
        sequence: u32,
    },
    SelectCard {
        sequence: u32,
        card_id: String,
    },
    Strike {
        sequence: u32,
        lane: u8,
        card_id: String,
        selected_index: i16,
        #[serde(default)]
        use_quick_blitz_chain: bool,
        client_elapsed_ms: u32,
        #[serde(default)]
        transport_loss_fizzle: bool,
        #[serde(default)]
        rtt_ms: u32,
    },
    TickAdvance {
        delta_ms: u32,
    },
    AbandonMatch {
        sequence: u32,
        card_id: String,
    },
}

async fn handle_duel_socket(
    state: AppState,
    room: Arc<DuelRoom>,
    actor_id: Uuid,
    side: DuelSide,
    mut socket: WebSocket,
) {
    let (tx, mut rx) = mpsc::channel::<Value>(WS_OUTBOUND_QUEUE_CAP);
    {
        let mut subs = room.subscribers.lock().await;
        subs.entry(side).or_default().push(tx.clone());
        let mut disconnected = room.disconnected_at.lock().await;
        disconnected.remove(&side);
        let mut activity = room.last_client_activity.lock().await;
        activity.insert(side, Instant::now());
    }

    // Send initial authoritative F-04 snapshot upon upgrade / reconnect
    {
        let arena = room.arena.lock().await;
        let deck = match side {
            DuelSide::PlayerA => &room.deck_a,
            DuelSide::PlayerB => &room.deck_b,
        };
        let init_msg = json!({
            "type": "MATCH_STATE",
            "seq": room.tx_seq.load(Ordering::Relaxed),
            "match_id": room.match_id,
            "payload": project_arena_for_actor(&arena, side, room.opponent_kind, deck),
        });
        let _ = tx.try_send(init_msg);
    }

    let mut hb = interval(Duration::from_secs(WS_HEARTBEAT_INTERVAL_SECS));
    hb.tick().await;

    loop {
        tokio::select! {
            outbound = rx.recv() => {
                match outbound {
                    Some(val) => {
                        if socket.send(Message::Text(val.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            _ = hb.tick() => {
                let timed_out = {
                    let activity = room.last_client_activity.lock().await;
                    activity.get(&side).is_some_and(|t| t.elapsed().as_secs() >= WS_CLIENT_TIMEOUT_SECS)
                };
                if timed_out {
                    let _ = socket.send(Message::Close(None)).await;
                    break;
                }
                if socket.send(Message::Ping(b"hb".to_vec().into())).await.is_err() {
                    break;
                }
            }
            inbound = socket.recv() => {
                match inbound {
                    Some(Ok(Message::Text(text))) => {
                        {
                            let mut activity = room.last_client_activity.lock().await;
                            activity.insert(side, Instant::now());
                        }
                        if text.len() > WS_MAX_PAYLOAD_BYTES {
                            let _ = tx.try_send(json!({
                                "type": "ERROR",
                                "code": "PAYLOAD_TOO_LARGE",
                                "message": "frame exceeds 4096 bytes"
                            }));
                            continue;
                        }
                        if !room.check_rate_limit(actor_id).await {
                            let _ = tx.try_send(json!({
                                "type": "ERROR",
                                "code": "RATE_LIMITED",
                                "message": "command budget exceeded"
                            }));
                            continue;
                        }
                        let parsed: Result<DuelClientCommand, _> = serde_json::from_str(&text);
                        match parsed {
                            Ok(cmd) => {
                                process_duel_command(&state, &room, actor_id, side, &tx, cmd).await;
                            }
                            Err(_) => {
                                let _ = tx.try_send(json!({
                                    "type": "ERROR",
                                    "code": "REJECTED_SPOOF_OR_SCHEMA",
                                    "message": "unknown, spoofed, or malformed duel command"
                                }));
                            }
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {
                        let mut activity = room.last_client_activity.lock().await;
                        activity.insert(side, Instant::now());
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
        }
    }

    let mut disconnected = room.disconnected_at.lock().await;
    disconnected.insert(side, Instant::now());
}

async fn process_duel_command(
    state: &AppState,
    room: &Arc<DuelRoom>,
    actor_id: Uuid,
    side: DuelSide,
    tx: &mpsc::Sender<Value>,
    cmd: DuelClientCommand,
) {
    // Re-verify A-02 actor ownership on every command
    if room.side_for_actor(actor_id) != Some(side) {
        let _ = tx.try_send(json!({"type": "ERROR", "code": "FORBIDDEN"}));
        return;
    }

    match cmd {
        DuelClientCommand::Heartbeat { client_ts_ms } => {
            let _ = tx.try_send(json!({
                "type": "HEARTBEAT_ACK",
                "client_ts_ms": client_ts_ms,
                "server_seq": room.tx_seq.load(Ordering::Relaxed),
            }));
        }
        DuelClientCommand::ResyncRequest { last_seen_seq } => {
            let arena = room.arena.lock().await;
            let deck = if side == DuelSide::PlayerA {
                &room.deck_a
            } else {
                &room.deck_b
            };
            let _ = tx.try_send(json!({
                "type": "SNAPSHOT_RESYNC",
                "resync_from_seq": last_seen_seq,
                "seq": room.tx_seq.load(Ordering::Relaxed),
                "match_id": room.match_id,
                "payload": project_arena_for_actor(&arena, side, room.opponent_kind, deck),
            }));
        }
        DuelClientCommand::StartRound { sequence } => {
            if sequence == 0 {
                let _ = tx.try_send(json!({"type": "ERROR", "code": "INVALID_SEQUENCE"}));
                return;
            }
            let mut arena = room.arena.lock().await;
            let elapsed = arena.total_elapsed_ms();
            arena.fighter_mut(side).selection_started_at_ms = Some(elapsed);
            drop(arena);
            room.round_reveal_at
                .lock()
                .await
                .insert(side, Instant::now());
        }
        DuelClientCommand::SelectCard { sequence, card_id } => {
            if sequence == 0 {
                let _ = tx.try_send(json!({"type": "ERROR", "code": "INVALID_SEQUENCE"}));
                return;
            }
            let mut arena = room.arena.lock().await;
            let fighter = arena.fighter_mut(side);
            if fighter.hand.iter().any(|c| c.card_id == card_id) {
                fighter.selected_card_id = Some(card_id);
            } else {
                let _ = tx.try_send(json!({"type": "ERROR", "code": "UNKNOWN_CARD"}));
            }
        }
        DuelClientCommand::TickAdvance { delta_ms } => {
            if delta_ms == 0 || delta_ms > 90_000 {
                let _ = tx.try_send(json!({"type": "ERROR", "code": "INVALID_TICK_DELTA"}));
                return;
            }
            let (proj_a, proj_b, maybe_winner, clock_ms, snapshot_clone, should_snap) = {
                let mut arena = room.arena.lock().await;
                let _ = arena.advance_clock(delta_ms);

                // Trigger Ghost or Bot response during clock advancement
                if arena.status != DuelMatchStatus::Completed
                    && matches!(room.opponent_kind, OpponentKind::Ghost | OpponentKind::Bot)
                {
                    maybe_step_ai_opponent(state, room, &mut arena).await;
                }

                let mut last_snap = room.last_snapshot_clock_ms.lock().await;
                let snap_due = arena.match_clock_ms >= *last_snap + SNAPSHOT_INTERVAL_MS
                    || arena.status == DuelMatchStatus::Completed;
                if snap_due {
                    *last_snap = arena.match_clock_ms;
                }
                let winner = if arena.status == DuelMatchStatus::Completed {
                    Some((arena.winner, arena.match_clock_ms))
                } else {
                    None
                };
                (
                    project_arena_for_actor(
                        &arena,
                        DuelSide::PlayerA,
                        room.opponent_kind,
                        &room.deck_a,
                    ),
                    project_arena_for_actor(
                        &arena,
                        DuelSide::PlayerB,
                        room.opponent_kind,
                        &room.deck_b,
                    ),
                    winner,
                    arena.match_clock_ms,
                    arena.clone(),
                    snap_due,
                )
            };

            if should_snap {
                let snap_seq = room.snapshot_seq.fetch_add(1, Ordering::Relaxed) + 1;
                let _ = save_match_snapshot(
                    &state.app_pool,
                    Some(room.player_a_id),
                    room.match_id,
                    room.course_id,
                    snap_seq,
                    clock_ms as i32,
                    &snapshot_clone,
                )
                .await;
            }

            if let Some((winner_side, end_clock)) = maybe_winner {
                let winner_user_id = match winner_side {
                    Some(DuelSide::PlayerA) => Some(room.player_a_id),
                    Some(DuelSide::PlayerB) => room.player_b_id,
                    None => None,
                };
                let is_draw = winner_side.is_none();
                let _ = settle_match_atomic(
                    &state.app_pool,
                    Some(room.player_a_id),
                    room.match_id,
                    room.course_id,
                    "COMPLETED",
                    winner_user_id,
                    is_draw,
                    None,
                    end_clock as i32,
                )
                .await;
            }

            room.broadcast("MATCH_STATE", proj_a, proj_b).await;
        }
        DuelClientCommand::Strike {
            sequence,
            lane,
            card_id,
            selected_index,
            use_quick_blitz_chain,
            client_elapsed_ms,
            transport_loss_fizzle,
            rtt_ms,
        } => {
            let Some(&challenge_id) = room.card_to_challenge.get(&card_id) else {
                let _ = tx.try_send(json!({"type": "ERROR", "code": "UNKNOWN_CARD"}));
                return;
            };
            let is_correct = if transport_loss_fizzle {
                false
            } else {
                grade_challenge_answer(&state.grader_pool, challenge_id, selected_index)
                    .await
                    .unwrap_or(false)
            };

            let termination = if transport_loss_fizzle {
                AttemptTermination::VoidTransportLoss
            } else {
                AttemptTermination::Submitted
            };

            let reveal_elapsed_ms = {
                let map = room.round_reveal_at.lock().await;
                map.get(&side)
                    .map(|inst| u32::try_from(inst.elapsed().as_millis()).unwrap_or(u32::MAX))
            };
            let raw_elapsed_ms = match reveal_elapsed_ms {
                Some(_server_ms) if client_elapsed_ms < 150 => client_elapsed_ms,
                Some(server_ms) => server_ms.max(client_elapsed_ms).min(120_000),
                None => client_elapsed_ms.min(120_000),
            };

            let strike_input = CombatStrikeInput {
                attacker: side,
                sequence,
                lane,
                card_id: card_id.clone(),
                use_quick_blitz_chain,
                termination,
                is_correct,
                partial_credit: false,
                hint_used: false,
                server_sandbox_submits: 1,
                raw_elapsed_ms,
                rtt_compensation_ms: rtt_ms,
                opponent_card_base_ms: None,
            };

            let res_and_state = {
                let mut arena = room.arena.lock().await;
                match arena.resolve_strike(&strike_input) {
                    Ok(result) => Ok((
                        result,
                        arena.total_elapsed_ms(),
                        arena.match_clock_ms,
                        arena.status,
                        arena.winner,
                        arena.clone(),
                    )),
                    Err(e) => Err(e),
                }
            };

            let (result, total_elapsed_ms, clock_ms, status, winner_side, snap_arena) =
                match res_and_state {
                    Ok(v) => v,
                    Err(msg) => {
                        let _ = tx.try_send(json!({
                            "type": "ERROR",
                            "code": "STRIKE_REJECTED",
                            "message": msg,
                        }));
                        return;
                    }
                };

            let actor_side_str = match side {
                DuelSide::PlayerA => "PLAYER_A",
                DuelSide::PlayerB => "PLAYER_B",
            };
            let payload_val = serde_json::to_value(&strike_input).unwrap_or(json!({}));
            let event_kind = if result.fizzled { "VOID" } else { "STRIKE" };
            let _ = append_match_replay_event(
                &state.app_pool,
                Some(actor_id),
                room.match_id,
                room.course_id,
                actor_side_str,
                sequence as i32,
                event_kind,
                total_elapsed_ms as i32,
                &payload_val,
            )
            .await;

            // Compute score_v2 and record review_log under grader pool if non-void or void-logged
            if let Some(fsrs_r) = result.rating.fsrs_rating {
                let score_out = compute_score_v2(&ScoreV2Input {
                    base_points: 10,
                    rating: fsrs_r,
                    recall_bps: 10_000,
                    mastery_bps: 10_000,
                    mode_bps: 12_500,
                    recovery_x1e4: 10_000,
                })
                .map(|s| s.points as i32)
                .unwrap_or(0);
                let _ = record_duel_card_review(
                    &state.grader_pool,
                    Uuid::new_v4(),
                    actor_id,
                    challenge_id,
                    room.course_id,
                    room.match_id,
                    "MATCH",
                    &result.rating,
                    score_out,
                    Utc::now(),
                    match fsrs_r {
                        1 => 1.2,
                        2 => 3.0,
                        3 => 5.5,
                        _ => 9.0,
                    },
                    5.0,
                    i32::from(fsrs_r),
                )
                .await;
            }

            if status == DuelMatchStatus::Completed {
                let snap_seq = room.snapshot_seq.fetch_add(1, Ordering::Relaxed) + 1;
                let _ = save_match_snapshot(
                    &state.app_pool,
                    Some(room.player_a_id),
                    room.match_id,
                    room.course_id,
                    snap_seq,
                    clock_ms as i32,
                    &snap_arena,
                )
                .await;
                let winner_user_id = match winner_side {
                    Some(DuelSide::PlayerA) => Some(room.player_a_id),
                    Some(DuelSide::PlayerB) => room.player_b_id,
                    None => None,
                };
                let _ = settle_match_atomic(
                    &state.app_pool,
                    Some(room.player_a_id),
                    room.match_id,
                    room.course_id,
                    "COMPLETED",
                    winner_user_id,
                    winner_side.is_none(),
                    None,
                    clock_ms as i32,
                )
                .await;
            }

            let strike_summary = json!({
                "attacker": side,
                "result": result,
            });
            let proj_a = json!({
                "strike": strike_summary,
                "arena": project_arena_for_actor(&snap_arena, DuelSide::PlayerA, room.opponent_kind, &room.deck_a),
            });
            let proj_b = json!({
                "strike": strike_summary,
                "arena": project_arena_for_actor(&snap_arena, DuelSide::PlayerB, room.opponent_kind, &room.deck_b),
            });
            room.broadcast("STRIKE_RESOLVED", proj_a, proj_b).await;
        }
        DuelClientCommand::AbandonMatch { sequence, card_id } => {
            if let Some(&challenge_id) = room.card_to_challenge.get(&card_id) {
                let mut arena = room.arena.lock().await;
                let input = CombatStrikeInput {
                    attacker: side,
                    sequence,
                    lane: 0,
                    card_id,
                    use_quick_blitz_chain: false,
                    termination: AttemptTermination::AbandonedAfterReveal,
                    is_correct: false,
                    partial_credit: false,
                    hint_used: false,
                    server_sandbox_submits: 1,
                    raw_elapsed_ms: 1_000,
                    rtt_compensation_ms: 0,
                    opponent_card_base_ms: None,
                };
                if let Ok(res) = arena.resolve_strike(&input) {
                    let _ = record_duel_card_review(
                        &state.grader_pool,
                        Uuid::new_v4(),
                        actor_id,
                        challenge_id,
                        room.course_id,
                        room.match_id,
                        "MATCH",
                        &res.rating,
                        0,
                        Utc::now(),
                        1.0,
                        6.0,
                        1,
                    )
                    .await;
                }
            }
        }
    }
}

async fn maybe_step_ai_opponent(
    state: &AppState,
    room: &Arc<DuelRoom>,
    arena: &mut DuelArenaState,
) {
    let decision = match room.opponent_kind {
        OpponentKind::Ghost => {
            let mut idx = room.next_ghost_idx.lock().await;
            let mut chosen = None;
            while *idx < room.ghost_frames.len() {
                let frame = &room.ghost_frames[*idx];
                if frame.recorded_elapsed_ms <= arena.total_elapsed_ms() {
                    chosen = adapt_ghost_action(arena, DuelSide::PlayerB, frame);
                    *idx += 1;
                    if chosen.is_some() {
                        break;
                    }
                } else {
                    break;
                }
            }
            chosen
        }
        OpponentKind::Bot => {
            let step = room.bot_step.fetch_add(1, Ordering::Relaxed) + 1;
            decide_bot_action(arena, DuelSide::PlayerB, step)
        }
        OpponentKind::Human => None,
    };

    if let Some(pick) = decision {
        let next_seq = arena.player_b.last_sequence + 1;
        let strike = CombatStrikeInput {
            attacker: DuelSide::PlayerB,
            sequence: next_seq,
            lane: pick.chosen_lane,
            card_id: pick.chosen_card_id,
            use_quick_blitz_chain: false,
            termination: AttemptTermination::Submitted,
            is_correct: pick.is_correct,
            partial_credit: false,
            hint_used: pick.hint_used,
            server_sandbox_submits: 1,
            raw_elapsed_ms: pick.simulated_raw_elapsed_ms,
            rtt_compensation_ms: 0,
            opponent_card_base_ms: None,
        };
        if arena.resolve_strike(&strike).is_ok() {
            let payload_val = serde_json::to_value(&strike).unwrap_or(json!({}));
            let _ = append_match_replay_event(
                &state.app_pool,
                Some(room.player_a_id),
                room.match_id,
                room.course_id,
                "PLAYER_B",
                next_seq as i32,
                "STRIKE",
                arena.total_elapsed_ms() as i32,
                &payload_val,
            )
            .await;
        }
    }
}

pub async fn get_duel_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((course_id, match_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, ApiError> {
    let actor = require_actor(&state, &headers).await?;
    let Some((record, snap)) =
        get_match_summary(&state.app_pool, actor, course_id, match_id).await?
    else {
        return Err(ApiError::NotFound);
    };
    Ok(Json(json!({
        "match": record,
        "latest_snapshot": snap,
    })))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoverMatchesRequest {
    #[serde(default)]
    pub freeze_match_id: Option<Uuid>,
    #[serde(default)]
    pub simulate_outage_secs: Option<i64>,
}

pub async fn recover_duels_endpoint(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(course_id): Path<Uuid>,
    body: Option<Json<RecoverMatchesRequest>>,
) -> Result<Json<Value>, ApiError> {
    let actor = require_actor(&state, &headers).await?;
    let can_manage =
        campuslingo_storage::courses::can_manage(&state.app_pool, actor, course_id).await?;
    if !can_manage {
        return Err(ApiError::Forbidden);
    }
    let recovery_pool = state
        .recovery_pool
        .as_ref()
        .ok_or(ApiError::ServiceUnavailable("recovery pool not configured"))?;
    let req = body.map(|b| b.0).unwrap_or(RecoverMatchesRequest {
        freeze_match_id: None,
        simulate_outage_secs: None,
    });
    let now = Utc::now();
    if let Some(mid) = req.freeze_match_id {
        let _ = mark_match_outage_freeze(recovery_pool, mid, now).await?;
    }
    let check_now =
        now + ChronoDuration::seconds(req.simulate_outage_secs.unwrap_or(0).clamp(0, 60));
    let outcomes =
        recover_or_void_interrupted_matches(recovery_pool, check_now, WS_RECONNECT_GRACE_SECS)
            .await?;
    Ok(Json(json!({ "recovered": outcomes })))
}
