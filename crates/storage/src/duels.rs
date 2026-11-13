use campuslingo_domain::combat::{
    CombatStrikeInput, DuelArenaState, GhostReplayFrame, RatingOutcome,
};
use campuslingo_domain::windows::w_base_for_live_duel_ms;
pub use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::{StorageError, begin_as, is_permission_denied, is_unique_violation};

pub type Result<T> = std::result::Result<T, StorageError>;

fn map_duel_db_err(error: sqlx::Error) -> StorageError {
    if is_unique_violation(&error) {
        StorageError::Conflict
    } else if is_permission_denied(&error) {
        StorageError::Forbidden
    } else {
        StorageError::Database(error)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MatchRecord {
    pub id: Uuid,
    pub course_id: Uuid,
    pub player_a_id: Uuid,
    pub player_b_id: Option<Uuid>,
    pub opponent_kind: String,
    pub ghost_source_match_id: Option<Uuid>,
    pub status: String,
    pub winner_user_id: Option<Uuid>,
    pub is_draw: bool,
    pub void_reason: Option<String>,
    pub settlement_key: String,
    pub protocol_version: i16,
    pub content_version: i32,
    pub rating_map_version: i16,
    pub score_version: i16,
    pub accommodation_version: i16,
    pub entry_cost_gems: i32,
    pub entry_refunded: bool,
    pub clock_frozen_at: Option<DateTime<Utc>>,
    pub total_outage_frozen_ms: i32,
    pub match_clock_ms: i32,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MatchSnapshotRecord {
    pub match_id: Uuid,
    pub course_id: Uuid,
    pub snapshot_seq: i32,
    pub match_clock_ms: i32,
    pub state_json: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MatchReplayEventRecord {
    pub id: i64,
    pub match_id: Uuid,
    pub course_id: Uuid,
    pub actor_side: String,
    pub event_seq: i32,
    pub event_kind: String,
    pub elapsed_ms: i32,
    pub payload_json: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct ChallengeDeckRow {
    id: Uuid,
    prompt: String,
    options: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchDeckCard {
    pub challenge_id: Uuid,
    pub prompt: String,
    pub options: Vec<String>,
    pub archetype: String,
    pub modifier: String,
    pub card_cost: u8,
    pub base_window_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuelReviewRecordResult {
    pub fsrs_step_eligible: bool,
    pub reward_eligible: bool,
    pub eligibility_blocker: Option<String>,
    pub awarded_points: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CrashRecoveryOutcome {
    ResumedFromSnapshot {
        match_id: Uuid,
        snapshot_seq: i32,
        replayed_events: usize,
        outage_frozen_ms: i32,
        arena: Box<DuelArenaState>,
    },
    VoidedAfterGrace {
        match_id: Uuid,
        settlement_key: String,
        refunded_gems: i32,
    },
}

pub async fn get_user_accommodation_ratio(pool: &PgPool, actor_id: Uuid) -> Result<(u32, u32)> {
    let mut tx = begin_as(pool, actor_id).await?;
    let mult: Option<f64> = sqlx::query_scalar(
        "SELECT accommodation_multiplier::double precision FROM user_protection_state WHERE user_id = $1",
    )
    .bind(actor_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_duel_db_err)?;
    tx.commit().await.map_err(StorageError::Database)?;
    let ratio = match mult {
        Some(m) if (m - 1.5).abs() < 0.05 => (3, 2),
        Some(m) if (m - 1.25).abs() < 0.05 => (5, 4),
        _ => (1, 1),
    };
    Ok(ratio)
}

pub async fn grade_challenge_answer(
    grader_pool: &PgPool,
    challenge_id: Uuid,
    selected_index: i16,
) -> Result<bool> {
    let correct: Option<bool> = sqlx::query_scalar(
        "SELECT (correct_index = $2) FROM challenge_answers WHERE challenge_id = $1",
    )
    .bind(challenge_id)
    .bind(selected_index)
    .fetch_optional(grader_pool)
    .await
    .map_err(map_duel_db_err)?;
    Ok(correct.unwrap_or(false))
}

pub async fn load_live_duel_hand(
    app_pool: &PgPool,
    actor_id: Uuid,
    course_id: Uuid,
    limit: i64,
) -> Result<Vec<MatchDeckCard>> {
    let mut tx = begin_as(app_pool, actor_id).await?;
    let rows: Vec<ChallengeDeckRow> = sqlx::query_as(
        "SELECT c.id, c.prompt, c.options
         FROM challenges c
         WHERE c.course_id = $1
           AND can_study_course(c.course_id)
         ORDER BY c.id ASC
         LIMIT $2",
    )
    .bind(course_id)
    .bind(limit.clamp(1, 16))
    .fetch_all(&mut *tx)
    .await
    .map_err(map_duel_db_err)?;
    tx.commit().await.map_err(StorageError::Database)?;

    let archetypes = [
        ("PIPELINE", "FAULT_INJECTION", 3_u8),
        ("AXIOM", "COROLLARY_CHAIN", 2_u8),
        ("SPATIAL", "CROSS_SECTION_SCAN", 3_u8),
        ("AXIOM", "SPEED_BLITZ", 1_u8),
        ("TRAP", "SAFEGUARD_PATCH", 4_u8),
        ("CONTRAST", "BLIND_CHAMBER_ID", 3_u8),
    ];
    let mut cards = Vec::with_capacity(rows.len());
    for (idx, row) in rows.into_iter().enumerate() {
        let (arch, modi, cost) = archetypes[idx % archetypes.len()];
        let base_ms = w_base_for_live_duel_ms(arch, modi).unwrap_or(9_000);
        let options = row
            .options
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(ToOwned::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        cards.push(MatchDeckCard {
            challenge_id: row.id,
            prompt: row.prompt,
            options,
            archetype: arch.to_string(),
            modifier: modi.to_string(),
            card_cost: cost,
            base_window_ms: base_ms,
        });
    }
    Ok(cards)
}

#[allow(clippy::too_many_arguments)]
pub async fn start_match_record(
    pool: &PgPool,
    actor_id: Uuid,
    match_id: Uuid,
    course_id: Uuid,
    player_a_id: Uuid,
    player_b_id: Option<Uuid>,
    opponent_kind: &str,
    ghost_source_match_id: Option<Uuid>,
    settlement_key: &str,
    entry_cost_gems: i32,
    initial_arena: &DuelArenaState,
) -> Result<()> {
    let mut tx = begin_as(pool, actor_id).await?;
    let snapshot = serde_json::to_value(initial_arena)
        .map_err(|_| StorageError::Invalid("arena serialization"))?;
    sqlx::query("SELECT create_match_record($1, $2, $3, $4, $5, $6, $7, $8, $9)")
        .bind(match_id)
        .bind(course_id)
        .bind(player_a_id)
        .bind(player_b_id)
        .bind(opponent_kind)
        .bind(ghost_source_match_id)
        .bind(settlement_key)
        .bind(entry_cost_gems)
        .bind(snapshot)
        .execute(&mut *tx)
        .await
        .map_err(map_duel_db_err)?;
    tx.commit().await.map_err(StorageError::Database)?;
    Ok(())
}

pub async fn find_permissioned_ghost_replay(
    app_pool: &PgPool,
    actor_id: Uuid,
    course_id: Uuid,
) -> Result<Option<(Uuid, Vec<GhostReplayFrame>)>> {
    let mut tx = begin_as(app_pool, actor_id).await?;
    let source_match_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id
         FROM matches
         WHERE course_id = $1
           AND status = 'COMPLETED'
           AND replay_opt_in_a = TRUE
           AND player_a_id <> $2
         ORDER BY started_at DESC
         LIMIT 1",
    )
    .bind(course_id)
    .bind(actor_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_duel_db_err)?;

    let Some(ghost_id) = source_match_id else {
        tx.commit().await.map_err(StorageError::Database)?;
        return Ok(None);
    };

    let events: Vec<MatchReplayEventRecord> = sqlx::query_as(
        "SELECT id, match_id, course_id, actor_side, event_seq, event_kind, elapsed_ms, payload_json, created_at
         FROM match_replay_events
         WHERE match_id = $1
           AND actor_side = 'PLAYER_A'
           AND event_kind = 'STRIKE'
         ORDER BY elapsed_ms ASC, event_seq ASC
         LIMIT 128",
    )
    .bind(ghost_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(map_duel_db_err)?;
    tx.commit().await.map_err(StorageError::Database)?;

    let mut frames = Vec::with_capacity(events.len());
    for ev in events {
        if let Ok(strike) = serde_json::from_value::<CombatStrikeInput>(ev.payload_json.clone()) {
            frames.push(GhostReplayFrame {
                recorded_elapsed_ms: ev.elapsed_ms.max(0) as u32,
                preferred_lane: strike.lane,
                preferred_card_cost: 2,
                historical_is_correct: strike.is_correct,
                historical_response_ms: strike.raw_elapsed_ms,
                historical_hint_used: strike.hint_used,
                historical_damage_ignored: 0,
            });
        }
    }
    if frames.is_empty() {
        return Ok(None);
    }
    Ok(Some((ghost_id, frames)))
}

#[allow(clippy::too_many_arguments)]
pub async fn append_match_replay_event(
    pool: &PgPool,
    actor_id: Option<Uuid>,
    match_id: Uuid,
    course_id: Uuid,
    actor_side: &str,
    event_seq: i32,
    event_kind: &str,
    elapsed_ms: i32,
    payload_json: &Value,
) -> Result<()> {
    let mut tx = if let Some(uid) = actor_id {
        begin_as(pool, uid).await?
    } else {
        pool.begin().await.map_err(StorageError::Database)?
    };
    sqlx::query("SELECT append_match_replay_event_secure($1, $2, $3, $4, $5, $6, $7)")
        .bind(match_id)
        .bind(course_id)
        .bind(actor_side)
        .bind(event_seq)
        .bind(event_kind)
        .bind(elapsed_ms)
        .bind(payload_json)
        .execute(&mut *tx)
        .await
        .map_err(map_duel_db_err)?;
    tx.commit().await.map_err(StorageError::Database)?;
    Ok(())
}

pub async fn save_match_snapshot(
    pool: &PgPool,
    actor_id: Option<Uuid>,
    match_id: Uuid,
    course_id: Uuid,
    snapshot_seq: i32,
    match_clock_ms: i32,
    arena: &DuelArenaState,
) -> Result<()> {
    let mut tx = if let Some(uid) = actor_id {
        begin_as(pool, uid).await?
    } else {
        pool.begin().await.map_err(StorageError::Database)?
    };
    let state_json =
        serde_json::to_value(arena).map_err(|_| StorageError::Invalid("arena json"))?;
    sqlx::query("SELECT save_match_snapshot_secure($1, $2, $3, $4, $5)")
        .bind(match_id)
        .bind(course_id)
        .bind(snapshot_seq)
        .bind(match_clock_ms)
        .bind(state_json)
        .execute(&mut *tx)
        .await
        .map_err(map_duel_db_err)?;
    tx.commit().await.map_err(StorageError::Database)?;
    Ok(())
}

#[derive(Debug, FromRow)]
struct ReviewProcedureRow {
    fsrs_step_eligible: bool,
    reward_eligible: bool,
    eligibility_blocker: Option<String>,
    awarded_points: i32,
}

#[allow(clippy::too_many_arguments)]
pub async fn record_duel_card_review(
    grader_or_worker_pool: &PgPool,
    attempt_id: Uuid,
    user_id: Uuid,
    challenge_id: Uuid,
    course_id: Uuid,
    match_id: Uuid,
    interaction_source: &str,
    outcome: &RatingOutcome,
    score_points: i32,
    reviewed_at: DateTime<Utc>,
    next_stability: f32,
    next_difficulty: f32,
    scheduled_days: i32,
) -> Result<DuelReviewRecordResult> {
    let rating: i16 = i16::from(outcome.fsrs_rating.unwrap_or(1));
    let void_reason = if outcome.is_void {
        Some(format!("{:?}", outcome.reason))
    } else {
        None
    };
    let row: ReviewProcedureRow = sqlx::query_as(
        "SELECT fsrs_step_eligible, reward_eligible, eligibility_blocker, awarded_points
         FROM record_duel_review_secure($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
    )
    .bind(attempt_id)
    .bind(user_id)
    .bind(challenge_id)
    .bind(course_id)
    .bind(match_id)
    .bind(interaction_source)
    .bind(rating)
    .bind(score_points)
    .bind(outcome.is_void)
    .bind(void_reason)
    .bind(reviewed_at)
    .bind(next_stability)
    .bind(next_difficulty)
    .bind(scheduled_days)
    .fetch_one(grader_or_worker_pool)
    .await
    .map_err(map_duel_db_err)?;

    Ok(DuelReviewRecordResult {
        fsrs_step_eligible: row.fsrs_step_eligible,
        reward_eligible: row.reward_eligible,
        eligibility_blocker: row.eligibility_blocker,
        awarded_points: row.awarded_points,
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn settle_match_atomic(
    pool: &PgPool,
    actor_id: Option<Uuid>,
    match_id: Uuid,
    course_id: Uuid,
    status: &str,
    winner_user_id: Option<Uuid>,
    is_draw: bool,
    void_reason: Option<&str>,
    match_clock_ms: i32,
) -> Result<bool> {
    let mut tx = if let Some(uid) = actor_id {
        begin_as(pool, uid).await?
    } else {
        pool.begin().await.map_err(StorageError::Database)?
    };
    let settled: bool =
        sqlx::query_scalar("SELECT settle_match_atomic($1, $2, $3, $4, $5, $6, $7)")
            .bind(match_id)
            .bind(course_id)
            .bind(status)
            .bind(winner_user_id)
            .bind(is_draw)
            .bind(void_reason)
            .bind(match_clock_ms)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_duel_db_err)?;
    tx.commit().await.map_err(StorageError::Database)?;
    Ok(settled)
}

pub async fn get_match_summary(
    app_pool: &PgPool,
    actor_id: Uuid,
    course_id: Uuid,
    match_id: Uuid,
) -> Result<Option<(MatchRecord, Option<MatchSnapshotRecord>)>> {
    let mut tx = begin_as(app_pool, actor_id).await?;
    let m: Option<MatchRecord> = sqlx::query_as(
        "SELECT id, course_id, player_a_id, player_b_id, opponent_kind, ghost_source_match_id,
                status, winner_user_id, is_draw, void_reason, settlement_key, protocol_version,
                content_version, rating_map_version, score_version, accommodation_version,
                entry_cost_gems, entry_refunded, clock_frozen_at, total_outage_frozen_ms,
                match_clock_ms, started_at, ended_at
         FROM matches
         WHERE id = $1 AND course_id = $2",
    )
    .bind(match_id)
    .bind(course_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_duel_db_err)?;

    let Some(match_row) = m else {
        tx.commit().await.map_err(StorageError::Database)?;
        return Ok(None);
    };

    let snap: Option<MatchSnapshotRecord> = sqlx::query_as(
        "SELECT match_id, course_id, snapshot_seq, match_clock_ms, state_json, created_at
         FROM match_snapshots
         WHERE match_id = $1
         ORDER BY snapshot_seq DESC
         LIMIT 1",
    )
    .bind(match_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_duel_db_err)?;
    tx.commit().await.map_err(StorageError::Database)?;
    Ok(Some((match_row, snap)))
}

pub async fn mark_match_outage_freeze(
    recovery_pool: &PgPool,
    match_id: Uuid,
    frozen_at: DateTime<Utc>,
) -> Result<bool> {
    let res = sqlx::query(
        "UPDATE matches
         SET clock_frozen_at = COALESCE(clock_frozen_at, $2)
         WHERE id = $1 AND status IN ('MATCH_STARTED', 'IN_PROGRESS', 'SUDDEN_DEATH')",
    )
    .bind(match_id)
    .bind(frozen_at)
    .execute(recovery_pool)
    .await
    .map_err(map_duel_db_err)?;
    Ok(res.rows_affected() > 0)
}

/// `R-07` Event-Sourced Crash Recovery:
/// - Inside 15s reconnect grace: freeze match clock over outage window and deterministically replay
///   committed events since latest 10s snapshot so match state and `review_logs` never diverge.
/// - Past 15s grace: settle idempotently as `VOID_SERVER_INTERRUPTION` with entry cost refunded.
pub async fn recover_or_void_interrupted_matches(
    recovery_pool: &PgPool,
    now: DateTime<Utc>,
    reconnect_grace_secs: i64,
) -> Result<Vec<CrashRecoveryOutcome>> {
    let open_matches: Vec<MatchRecord> = sqlx::query_as(
        "SELECT id, course_id, player_a_id, player_b_id, opponent_kind, ghost_source_match_id,
                status, winner_user_id, is_draw, void_reason, settlement_key, protocol_version,
                content_version, rating_map_version, score_version, accommodation_version,
                entry_cost_gems, entry_refunded, clock_frozen_at, total_outage_frozen_ms,
                match_clock_ms, started_at, ended_at
         FROM matches
         WHERE status IN ('MATCH_STARTED', 'IN_PROGRESS', 'SUDDEN_DEATH')
         ORDER BY started_at ASC",
    )
    .fetch_all(recovery_pool)
    .await
    .map_err(map_duel_db_err)?;

    let mut outcomes = Vec::with_capacity(open_matches.len());
    for m in open_matches {
        let freeze_anchor = m.clock_frozen_at.unwrap_or(m.started_at);
        let outage_ms = (now - freeze_anchor).num_milliseconds().max(0);
        if outage_ms > reconnect_grace_secs.saturating_mul(1_000) {
            settle_match_atomic(
                recovery_pool,
                None,
                m.id,
                m.course_id,
                "VOID_SERVER_INTERRUPTION",
                None,
                false,
                Some("VOID_SERVER_INTERRUPTION"),
                m.match_clock_ms,
            )
            .await?;
            outcomes.push(CrashRecoveryOutcome::VoidedAfterGrace {
                match_id: m.id,
                settlement_key: m.settlement_key,
                refunded_gems: m.entry_cost_gems,
            });
            continue;
        }

        let snap: MatchSnapshotRecord = sqlx::query_as(
            "SELECT match_id, course_id, snapshot_seq, match_clock_ms, state_json, created_at
             FROM match_snapshots
             WHERE match_id = $1
             ORDER BY snapshot_seq DESC
             LIMIT 1",
        )
        .bind(m.id)
        .fetch_one(recovery_pool)
        .await
        .map_err(map_duel_db_err)?;

        let mut arena: DuelArenaState = serde_json::from_value(snap.state_json)
            .map_err(|_| StorageError::Invalid("corrupted snapshot state_json"))?;

        let events: Vec<MatchReplayEventRecord> = sqlx::query_as(
            "SELECT id, match_id, course_id, actor_side, event_seq, event_kind, elapsed_ms, payload_json, created_at
             FROM match_replay_events
             WHERE match_id = $1
               AND (elapsed_ms > $2 OR event_kind = 'STRIKE')
             ORDER BY elapsed_ms ASC, event_seq ASC",
        )
        .bind(m.id)
        .bind(snap.match_clock_ms)
        .fetch_all(recovery_pool)
        .await
        .map_err(map_duel_db_err)?;

        let mut replayed = 0_usize;
        for ev in events {
            let target_t = ev.elapsed_ms.max(0) as u32;
            let current_t = arena.total_elapsed_ms();
            if target_t > current_t {
                let _ = arena.advance_clock(target_t - current_t);
            }
            if ev.event_kind == "STRIKE"
                && let Ok(strike) = serde_json::from_value::<CombatStrikeInput>(ev.payload_json)
                && strike.sequence > arena.fighter(strike.attacker).last_sequence
                && arena.resolve_strike(&strike).is_ok()
            {
                replayed += 1;
            }
        }

        let frozen_delta_ms = i32::try_from(outage_ms).unwrap_or(i32::MAX);
        let new_seq = snap.snapshot_seq + 1;
        if replayed > 0 {
            save_match_snapshot(
                recovery_pool,
                None,
                m.id,
                m.course_id,
                new_seq,
                arena.match_clock_ms as i32,
                &arena,
            )
            .await?;
        }
        sqlx::query(
            "UPDATE matches
             SET clock_frozen_at = NULL,
                 total_outage_frozen_ms = total_outage_frozen_ms + $2
             WHERE id = $1",
        )
        .bind(m.id)
        .bind(frozen_delta_ms)
        .execute(recovery_pool)
        .await
        .map_err(map_duel_db_err)?;

        outcomes.push(CrashRecoveryOutcome::ResumedFromSnapshot {
            match_id: m.id,
            snapshot_seq: if replayed > 0 {
                new_seq
            } else {
                snap.snapshot_seq
            },
            replayed_events: replayed,
            outage_frozen_ms: frozen_delta_ms,
            arena: Box::new(arena),
        });
    }

    Ok(outcomes)
}

pub async fn purge_expired_replays(worker_pool: &PgPool) -> Result<u64> {
    let res = sqlx::query("DELETE FROM match_replay_events WHERE expires_at <= now()")
        .execute(worker_pool)
        .await
        .map_err(map_duel_db_err)?;
    Ok(res.rows_affected())
}
