use campuslingo_domain::raids_leagues::{
    ClanStabilityInput, LeagueBracketRolloverInput, LeagueBracketRolloverOutput, LeagueOutcomeKind,
    LeagueParticipantInput, LeagueStandingResult, compute_clan_stability_points,
    evaluate_league_rollover,
};
pub use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::{StorageError, begin_as, is_permission_denied, is_unique_violation};

pub type Result<T> = std::result::Result<T, StorageError>;

fn map_m6_db_err(error: sqlx::Error) -> StorageError {
    if is_unique_violation(&error) {
        return StorageError::Conflict;
    }
    if is_permission_denied(&error) {
        return StorageError::Forbidden;
    }
    if let Some(db) = error.as_database_error() {
        match db.code().as_deref() {
            Some("23P01") => return StorageError::Conflict,
            Some("23514") => {
                let msg = db.message();
                if msg.contains("OUTBOUND_DAILY_CAP_REACHED") {
                    return StorageError::Invalid("daily outbound raid cap (10) reached");
                }
                if msg.contains("INBOUND_DAILY_CAP_REACHED") {
                    return StorageError::Invalid("daily inbound raid cap (3) reached");
                }
                if msg.contains("UNORDERED_PAIR_DAILY_CAP_REACHED") {
                    return StorageError::Invalid("daily unordered pair raid cap (1) reached");
                }
                if msg.contains("REVENGE_WINDOW_INVALID") {
                    return StorageError::Invalid("revenge window is invalid or already consumed");
                }
                if msg.contains("DEFENDER_NODE_RAID_IMMUNE") {
                    return StorageError::Invalid("defender sector is currently raid immune");
                }
                if msg.contains("REWARD_ELIGIBLE_CAP_REACHED") {
                    return StorageError::Invalid("daily reward-eligible exposure cap reached");
                }
                if msg.contains("SAME_DAY_MIN_GAP") {
                    return StorageError::Invalid("30-minute same-day exposure gap required");
                }
                return StorageError::Invalid("violates M6 storage constraint");
            }
            Some("23503") => return StorageError::Invalid("referenced M6 entity does not exist"),
            _ => {}
        }
    }
    StorageError::Database(error)
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RaidCandidateRecord {
    pub defender_user_id: Uuid,
    pub node_id: Uuid,
    pub node_title: String,
    pub tier: i16,
    pub e_u_ticks: i32,
    pub raid_immune: bool,
    pub immunity_reason: Option<String>,
    pub has_revenge_window: bool,
    pub revenge_source_raid_id: Option<Uuid>,
    pub revenge_window_expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct StartedRaidRecord {
    pub raid_id: Uuid,
    pub is_revenge: bool,
    pub quota_date: NaiveDate,
    pub defender_snapshot_json: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SettledRaidRecord {
    pub status: String,
    pub concurrent_cooldown_blocked: bool,
    pub attacker_trophy_gain: i32,
    pub defender_trophy_loss: i32,
    pub practice_xp_awarded: i32,
    pub gold_awarded: i32,
    pub loot_chest_tier: Option<String>,
    pub consumes_defender_inbound_cap: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct NotificationRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub course_id: Uuid,
    pub notification_kind: String,
    pub local_quota_date: NaiveDate,
    pub raid_count: i32,
    pub summary_text: String,
    pub payload_json: Value,
    pub deliver_after: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct LeagueBracketRecord {
    pub id: Uuid,
    pub league_scope: String,
    pub course_id: Uuid,
    pub week_key: String,
    pub week_start_date: NaiveDate,
    pub tier: i16,
    pub late_joiner_shielded: bool,
    pub rolled_over: bool,
    pub rolled_over_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct LeagueStandingRecord {
    pub id: Uuid,
    pub bracket_id: Uuid,
    pub league_scope: String,
    pub course_id: Uuid,
    pub week_key: String,
    pub user_id: Uuid,
    pub timezone: String,
    pub tier_before: i16,
    pub tier_after: Option<i16>,
    pub trophies_earned: i32,
    pub eligible_mastery_recovery_count: i32,
    pub last_trophy_earned_at: DateTime<Utc>,
    pub is_dormant: bool,
    pub returned_from_dormancy_this_week: bool,
    pub qualifying_sessions_since_return: i32,
    pub consecutive_dormant_weeks: i16,
    pub league_status: String,
    pub rollover_outcome: Option<String>,
    pub final_rank: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeagueRolloverSummary {
    pub bracket: LeagueBracketRecord,
    pub outcome: LeagueBracketRolloverOutput,
    pub standings: Vec<LeagueStandingRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ChapterBossRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub course_id: Uuid,
    pub node_id: Uuid,
    pub title: String,
    pub mode: String,
    pub max_hp: i32,
    pub current_hp: i32,
    pub reward_tier: String,
    pub gold_reward: i32,
    pub cosmetic_reward_key: Option<String>,
    pub status: String,
    pub expires_at: DateTime<Utc>,
    pub defeated_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BossLobbyRecord {
    pub id: Uuid,
    pub boss_id: Uuid,
    pub course_id: Uuid,
    pub host_user_id: Uuid,
    pub member_user_ids: Vec<Uuid>,
    pub max_players: i16,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BossStrikeRecord {
    pub raw_damage: i32,
    pub applied_damage: i32,
    pub overkill_damage: i32,
    pub boss_hp_after: i32,
    pub boss_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BossChestClaimResult {
    pub claimed_now: bool,
    pub reward_tier: String,
    pub gold_awarded: i32,
    pub cosmetic_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ClanRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub course_id: Uuid,
    pub name: String,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ClanWarRecord {
    pub id: Uuid,
    pub course_id: Uuid,
    pub clan_id: Uuid,
    pub week_key: String,
    pub week_start_date: NaiveDate,
    pub war_roster_member_ids: Vec<Uuid>,
    pub active_members_snapshot: i32,
    pub total_raw_points: i64,
    pub total_normalized_points: i64,
    pub status: String,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ClanWarContributionRecord {
    pub id: Uuid,
    pub war_id: Uuid,
    pub clan_id: Uuid,
    pub course_id: Uuid,
    pub user_id: Uuid,
    pub contribution_kind: String,
    pub raw_points: i32,
    pub normalized_points: i32,
    pub relative_gain_bps: i32,
    pub settlement_key: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EconomyBalanceRecord {
    pub user_id: Uuid,
    pub course_id: Uuid,
    pub gold: i32,
    pub trophies: i32,
    pub gems: i32,
    pub practice_xp: i32,
    pub daily_practice_xp: i32,
    pub daily_practice_xp_date: NaiveDate,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CosmeticInventoryRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub course_id: Uuid,
    pub cosmetic_key: String,
    pub rarity: String,
    pub source_kind: String,
    pub settlement_key: String,
    pub acquired_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RewardTransactionRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub course_id: Option<Uuid>,
    pub source_kind: String,
    pub source_id: Option<Uuid>,
    pub settlement_key: String,
    pub gold_delta: i32,
    pub trophies_delta: i32,
    pub gems_delta: i32,
    pub practice_xp_delta: i32,
    pub chest_tier: Option<String>,
    pub quota_date: NaiveDate,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserEconomyOverview {
    pub balance: Option<EconomyBalanceRecord>,
    pub cosmetics: Vec<CosmeticInventoryRecord>,
    pub recent_transactions: Vec<RewardTransactionRecord>,
}

pub async fn set_course_raid_opt_in(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    opt_in: bool,
) -> Result<()> {
    let mut tx = begin_as(pool, actor).await?;
    sqlx::query("SELECT set_course_raid_opt_in_secure($1, $2)")
        .bind(course_id)
        .bind(opt_in)
        .execute(&mut *tx)
        .await
        .map_err(map_m6_db_err)?;
    tx.commit().await?;
    Ok(())
}

pub async fn scan_raid_targets(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Vec<RaidCandidateRecord>> {
    let mut tx = begin_as(pool, actor).await?;
    let rows = sqlx::query_as::<_, RaidCandidateRecord>(
        "SELECT defender_user_id, defender_node_id AS node_id, node_title, node_tier AS tier, \
                e_u_ticks, raid_immune, immunity_reason, has_revenge_window, \
                revenge_source_raid_id, revenge_expires_at AS revenge_window_expires_at \
         FROM list_raid_candidates_secure($1, $2) \
         ORDER BY raid_immune ASC, has_revenge_window DESC, node_title ASC, defender_user_id ASC",
    )
    .bind(course_id)
    .bind(now)
    .fetch_all(&mut *tx)
    .await
    .map_err(map_m6_db_err)?;
    tx.commit().await?;
    Ok(rows)
}

#[allow(clippy::too_many_arguments)]
pub async fn start_raid_session(
    pool: &PgPool,
    actor: Uuid,
    raid_id: Uuid,
    course_id: Uuid,
    defender_user_id: Uuid,
    defender_node_id: Uuid,
    is_revenge: bool,
    revenge_source_raid_id: Option<Uuid>,
    settlement_key: &str,
    now: DateTime<Utc>,
) -> Result<StartedRaidRecord> {
    let mut tx = begin_as(pool, actor).await?;
    let rec = sqlx::query_as::<_, StartedRaidRecord>(
        "SELECT raid_id, is_revenge, quota_date, defender_snapshot_json \
         FROM start_raid_session_secure($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(raid_id)
    .bind(course_id)
    .bind(defender_user_id)
    .bind(defender_node_id)
    .bind(is_revenge)
    .bind(revenge_source_raid_id)
    .bind(settlement_key)
    .bind(now)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_m6_db_err)?;
    tx.commit().await?;
    Ok(rec)
}

#[allow(clippy::too_many_arguments)]
pub async fn settle_raid_session(
    pool: &PgPool,
    actor: Uuid,
    raid_id: Uuid,
    course_id: Uuid,
    sandbox_solved: bool,
    drills_passed: i16,
    base_score_points: i32,
    now: DateTime<Utc>,
) -> Result<SettledRaidRecord> {
    let mut tx = begin_as(pool, actor).await?;
    let rec = sqlx::query_as::<_, SettledRaidRecord>(
        "SELECT status, concurrent_cooldown_blocked, attacker_trophy_gain, defender_trophy_loss, \
                practice_xp_awarded, gold_awarded, loot_chest_tier, consumes_defender_inbound_cap \
         FROM settle_raid_session_secure($1, $2, $3, $4, $5, $6)",
    )
    .bind(raid_id)
    .bind(course_id)
    .bind(sandbox_solved)
    .bind(drills_passed)
    .bind(base_score_points)
    .bind(now)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_m6_db_err)?;
    tx.commit().await?;
    Ok(rec)
}

pub async fn get_user_notifications(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
) -> Result<Vec<NotificationRecord>> {
    let mut tx = begin_as(pool, actor).await?;
    let rows = sqlx::query_as::<_, NotificationRecord>(
        "SELECT id, user_id, course_id, notification_kind, local_quota_date, raid_count, \
                summary_text, payload_json, deliver_after, created_at, updated_at \
         FROM notification_delivery_ledger \
         WHERE user_id = $1 AND course_id = $2 \
         ORDER BY local_quota_date DESC, updated_at DESC",
    )
    .bind(actor)
    .bind(course_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(map_m6_db_err)?;
    tx.commit().await?;
    Ok(rows)
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpsertLeagueScoreInput {
    pub league_scope: String,
    pub course_id: Uuid,
    pub week_key: String,
    pub week_start_date: NaiveDate,
    pub tier: i16,
    pub late_joiner_shielded: bool,
    pub timezone: String,
    pub trophies_delta: i32,
    pub recovery_delta: i32,
    pub is_dormant: bool,
    pub returned_from_dormancy_this_week: bool,
    pub qualifying_sessions_delta: i32,
    pub consecutive_dormant_weeks: i16,
    pub now: DateTime<Utc>,
}

pub async fn upsert_league_score(
    pool: &PgPool,
    actor: Uuid,
    target_user_id: Uuid,
    input: UpsertLeagueScoreInput,
) -> Result<LeagueStandingRecord> {
    if !(1..=10).contains(&input.tier) {
        return Err(StorageError::Invalid("league tier must be 1..=10"));
    }
    let mut tx = begin_as(pool, actor).await?;

    let existing_bracket: Option<LeagueBracketRecord> = sqlx::query_as::<_, LeagueBracketRecord>(
        "SELECT id, league_scope, course_id, week_key, week_start_date, tier, late_joiner_shielded, rolled_over, rolled_over_at \
         FROM league_brackets \
         WHERE league_scope = $1 AND course_id = $2 AND week_key = $3 AND tier = $4 \
         ORDER BY created_at ASC LIMIT 1",
    )
    .bind(&input.league_scope)
    .bind(input.course_id)
    .bind(&input.week_key)
    .bind(input.tier)
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_m6_db_err)?;

    let bracket_id = match existing_bracket {
        Some(b) => {
            if input.late_joiner_shielded && !b.late_joiner_shielded {
                sqlx::query("UPDATE league_brackets SET late_joiner_shielded = TRUE WHERE id = $1")
                    .bind(b.id)
                    .execute(&mut *tx)
                    .await
                    .map_err(map_m6_db_err)?;
            }
            b.id
        }
        None => {
            let new_id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO league_brackets \
                    (id, league_scope, course_id, week_key, week_start_date, tier, late_joiner_shielded) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(new_id)
            .bind(&input.league_scope)
            .bind(input.course_id)
            .bind(&input.week_key)
            .bind(input.week_start_date)
            .bind(input.tier)
            .bind(input.late_joiner_shielded)
            .execute(&mut *tx)
            .await
            .map_err(map_m6_db_err)?;
            new_id
        }
    };

    let standing = sqlx::query_as::<_, LeagueStandingRecord>(
        "INSERT INTO league_standings ( \
            bracket_id, league_scope, course_id, week_key, user_id, timezone, \
            tier_before, trophies_earned, eligible_mastery_recovery_count, \
            last_trophy_earned_at, is_dormant, returned_from_dormancy_this_week, \
            qualifying_sessions_since_return, consecutive_dormant_weeks \
         ) VALUES ( \
            $1, $2, $3, $4, $5, $6, \
            $7, $8, $9, \
            $10, $11, $12, \
            $13, $14 \
         ) \
         ON CONFLICT (league_scope, week_key, user_id) DO UPDATE \
         SET trophies_earned = league_standings.trophies_earned + EXCLUDED.trophies_earned, \
             eligible_mastery_recovery_count = league_standings.eligible_mastery_recovery_count + EXCLUDED.eligible_mastery_recovery_count, \
             last_trophy_earned_at = CASE \
                 WHEN EXCLUDED.trophies_earned > 0 THEN EXCLUDED.last_trophy_earned_at \
                 ELSE league_standings.last_trophy_earned_at \
             END, \
             is_dormant = EXCLUDED.is_dormant, \
             returned_from_dormancy_this_week = EXCLUDED.returned_from_dormancy_this_week, \
             qualifying_sessions_since_return = league_standings.qualifying_sessions_since_return + EXCLUDED.qualifying_sessions_since_return, \
             consecutive_dormant_weeks = EXCLUDED.consecutive_dormant_weeks \
         RETURNING id, bracket_id, league_scope, course_id, week_key, user_id, timezone, \
                   tier_before, tier_after, trophies_earned, eligible_mastery_recovery_count, \
                   last_trophy_earned_at, is_dormant, returned_from_dormancy_this_week, \
                   qualifying_sessions_since_return, consecutive_dormant_weeks, league_status, \
                   rollover_outcome, final_rank",
    )
    .bind(bracket_id)
    .bind(&input.league_scope)
    .bind(input.course_id)
    .bind(&input.week_key)
    .bind(target_user_id)
    .bind(&input.timezone)
    .bind(input.tier)
    .bind(input.trophies_delta.max(0))
    .bind(input.recovery_delta.max(0))
    .bind(input.now)
    .bind(input.is_dormant)
    .bind(input.returned_from_dormancy_this_week)
    .bind(input.qualifying_sessions_delta.max(0))
    .bind(input.consecutive_dormant_weeks.max(0))
    .fetch_one(&mut *tx)
    .await
    .map_err(map_m6_db_err)?;

    tx.commit().await?;
    Ok(standing)
}

pub async fn get_league_bracket(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    league_scope: &str,
    week_key: &str,
    tier: i16,
) -> Result<(LeagueBracketRecord, Vec<LeagueStandingRecord>)> {
    let mut tx = begin_as(pool, actor).await?;
    let bracket = sqlx::query_as::<_, LeagueBracketRecord>(
        "SELECT id, league_scope, course_id, week_key, week_start_date, tier, late_joiner_shielded, rolled_over, rolled_over_at \
         FROM league_brackets \
         WHERE course_id = $1 AND league_scope = $2 AND week_key = $3 AND tier = $4 \
         ORDER BY created_at ASC LIMIT 1",
    )
    .bind(course_id)
    .bind(league_scope)
    .bind(week_key)
    .bind(tier)
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_m6_db_err)?
    .ok_or(StorageError::NotFound)?;

    let standings = sqlx::query_as::<_, LeagueStandingRecord>(
        "SELECT id, bracket_id, league_scope, course_id, week_key, user_id, timezone, \
                tier_before, tier_after, trophies_earned, eligible_mastery_recovery_count, \
                last_trophy_earned_at, is_dormant, returned_from_dormancy_this_week, \
                qualifying_sessions_since_return, consecutive_dormant_weeks, league_status, \
                rollover_outcome, final_rank \
         FROM league_standings \
         WHERE bracket_id = $1 \
         ORDER BY COALESCE(final_rank, 9999) ASC, trophies_earned DESC, eligible_mastery_recovery_count DESC, last_trophy_earned_at ASC, user_id ASC",
    )
    .bind(bracket.id)
    .fetch_all(&mut *tx)
    .await
    .map_err(map_m6_db_err)?;

    tx.commit().await?;
    Ok((bracket, standings))
}

fn outcome_kind_str(kind: &LeagueOutcomeKind) -> &'static str {
    match kind {
        LeagueOutcomeKind::Promoted => "PROMOTED",
        LeagueOutcomeKind::Held => "HELD",
        LeagueOutcomeKind::Demoted => "DEMOTED",
        LeagueOutcomeKind::DormantPreserved => "DORMANT_PRESERVED",
        LeagueOutcomeKind::DormantDemoted => "DORMANT_DEMOTED",
    }
}

fn parse_outcome_kind(s: &str) -> LeagueOutcomeKind {
    match s {
        "PROMOTED" => LeagueOutcomeKind::Promoted,
        "DEMOTED" => LeagueOutcomeKind::Demoted,
        "DORMANT_PRESERVED" => LeagueOutcomeKind::DormantPreserved,
        "DORMANT_DEMOTED" => LeagueOutcomeKind::DormantDemoted,
        _ => LeagueOutcomeKind::Held,
    }
}

pub async fn rollover_league_bracket(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    league_scope: &str,
    week_key: &str,
    tier: i16,
) -> Result<LeagueRolloverSummary> {
    let mut tx = begin_as(pool, actor).await?;

    let bracket = sqlx::query_as::<_, LeagueBracketRecord>(
        "SELECT id, league_scope, course_id, week_key, week_start_date, tier, late_joiner_shielded, rolled_over, rolled_over_at \
         FROM league_brackets \
         WHERE course_id = $1 AND league_scope = $2 AND week_key = $3 AND tier = $4 \
         ORDER BY created_at ASC LIMIT 1 \
         FOR UPDATE",
    )
    .bind(course_id)
    .bind(league_scope)
    .bind(week_key)
    .bind(tier)
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_m6_db_err)?
    .ok_or(StorageError::NotFound)?;

    let rows = sqlx::query_as::<_, LeagueStandingRecord>(
        "SELECT id, bracket_id, league_scope, course_id, week_key, user_id, timezone, \
                tier_before, tier_after, trophies_earned, eligible_mastery_recovery_count, \
                last_trophy_earned_at, is_dormant, returned_from_dormancy_this_week, \
                qualifying_sessions_since_return, consecutive_dormant_weeks, league_status, \
                rollover_outcome, final_rank \
         FROM league_standings \
         WHERE bracket_id = $1 \
         ORDER BY user_id ASC",
    )
    .bind(bracket.id)
    .fetch_all(&mut *tx)
    .await
    .map_err(map_m6_db_err)?;

    let output = if bracket.rolled_over {
        let mut sorted = rows.clone();
        sorted.sort_by_key(|r| (r.final_rank.unwrap_or(i32::MAX), r.user_id));
        let ranked_count = sorted.iter().filter(|r| r.final_rank.is_some()).count();
        let max_promo_slots = if ranked_count == 0 {
            0
        } else {
            (5 * ranked_count).div_ceil(30).max(1)
        };
        let demotion_slots = if bracket.late_joiner_shielded || ranked_count < 10 {
            0
        } else {
            (5 * ranked_count).div_ceil(30).max(1)
        };
        LeagueBracketRolloverOutput {
            tier_before: bracket.tier as u8,
            ranked_count,
            max_promo_slots,
            demotion_slots,
            standings: sorted
                .into_iter()
                .map(|r| LeagueStandingResult {
                    user_id: r.user_id.to_string(),
                    rank: r.final_rank.and_then(|v| usize::try_from(v).ok()),
                    outcome: parse_outcome_kind(r.rollover_outcome.as_deref().unwrap_or("HELD")),
                    league_status: r.league_status,
                    tier_before: r.tier_before as u8,
                    tier_after: r.tier_after.unwrap_or(r.tier_before) as u8,
                    next_consecutive_dormant_weeks: u16::try_from(
                        r.consecutive_dormant_weeks.max(0),
                    )
                    .unwrap_or(0),
                    trophies_earned: r.trophies_earned,
                })
                .collect(),
        }
    } else {
        let participants: Vec<LeagueParticipantInput> = rows
            .iter()
            .map(|r| LeagueParticipantInput {
                user_id: r.user_id.to_string(),
                trophies_earned: r.trophies_earned,
                eligible_mastery_recovery_count: r.eligible_mastery_recovery_count,
                last_trophy_earned_epoch_ms: r.last_trophy_earned_at.timestamp_millis(),
                is_dormant: r.is_dormant,
                returned_from_dormancy_this_week: r.returned_from_dormancy_this_week,
                qualifying_sessions_since_return: u32::try_from(
                    r.qualifying_sessions_since_return.max(0),
                )
                .unwrap_or(0),
                consecutive_dormant_weeks: u16::try_from(r.consecutive_dormant_weeks.max(0))
                    .unwrap_or(0),
            })
            .collect();

        let out = evaluate_league_rollover(&LeagueBracketRolloverInput {
            tier: u8::try_from(bracket.tier).map_err(|_| StorageError::Invalid("invalid tier"))?,
            late_joiner_shielded: bracket.late_joiner_shielded,
            participants,
        })
        .map_err(|_| StorageError::Invalid("invalid league rollover input"))?;

        for standing in &out.standings {
            let uid = Uuid::parse_str(&standing.user_id)
                .map_err(|_| StorageError::Invalid("invalid participant uuid"))?;
            let final_rank = standing.rank.map(|r| r as i32);
            let tier_after = standing.tier_after as i16;
            let next_dormant_weeks = standing.next_consecutive_dormant_weeks as i16;
            let outcome_str = outcome_kind_str(&standing.outcome);
            sqlx::query(
                "UPDATE league_standings \
                 SET final_rank = $1, \
                     rollover_outcome = $2, \
                     league_status = $3, \
                     tier_after = $4, \
                     consecutive_dormant_weeks = $5 \
                 WHERE bracket_id = $6 AND user_id = $7",
            )
            .bind(final_rank)
            .bind(outcome_str)
            .bind(&standing.league_status)
            .bind(tier_after)
            .bind(next_dormant_weeks)
            .bind(bracket.id)
            .bind(uid)
            .execute(&mut *tx)
            .await
            .map_err(map_m6_db_err)?;
        }

        sqlx::query(
            "UPDATE league_brackets SET rolled_over = TRUE, rolled_over_at = now() WHERE id = $1",
        )
        .bind(bracket.id)
        .execute(&mut *tx)
        .await
        .map_err(map_m6_db_err)?;

        out
    };

    let updated_bracket = sqlx::query_as::<_, LeagueBracketRecord>(
        "SELECT id, league_scope, course_id, week_key, week_start_date, tier, late_joiner_shielded, rolled_over, rolled_over_at \
         FROM league_brackets WHERE id = $1",
    )
    .bind(bracket.id)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_m6_db_err)?;

    let updated_standings = sqlx::query_as::<_, LeagueStandingRecord>(
        "SELECT id, bracket_id, league_scope, course_id, week_key, user_id, timezone, \
                tier_before, tier_after, trophies_earned, eligible_mastery_recovery_count, \
                last_trophy_earned_at, is_dormant, returned_from_dormancy_this_week, \
                qualifying_sessions_since_return, consecutive_dormant_weeks, league_status, \
                rollover_outcome, final_rank \
         FROM league_standings \
         WHERE bracket_id = $1 \
         ORDER BY COALESCE(final_rank, 9999) ASC, user_id ASC",
    )
    .bind(bracket.id)
    .fetch_all(&mut *tx)
    .await
    .map_err(map_m6_db_err)?;

    tx.commit().await?;
    Ok(LeagueRolloverSummary {
        bracket: updated_bracket,
        outcome: output,
        standings: updated_standings,
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn create_chapter_boss(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    node_id: Uuid,
    title: &str,
    mode: &str,
    max_hp: i32,
    gold_reward: i32,
    cosmetic_reward_key: Option<&str>,
) -> Result<ChapterBossRecord> {
    let mut tx = begin_as(pool, actor).await?;
    let tenant_id: Uuid = sqlx::query_scalar("SELECT tenant_id FROM courses WHERE id = $1")
        .bind(course_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_m6_db_err)?
        .ok_or(StorageError::NotFound)?;

    let rec = sqlx::query_as::<_, ChapterBossRecord>(
        "INSERT INTO chapter_bosses \
            (tenant_id, course_id, node_id, title, mode, max_hp, current_hp, gold_reward, cosmetic_reward_key) \
         VALUES ($1, $2, $3, $4, $5, $6, $6, $7, $8) \
         RETURNING id, tenant_id, course_id, node_id, title, mode, max_hp, current_hp, \
                   reward_tier, gold_reward, cosmetic_reward_key, status, expires_at, defeated_at, created_at",
    )
    .bind(tenant_id)
    .bind(course_id)
    .bind(node_id)
    .bind(title)
    .bind(mode)
    .bind(max_hp)
    .bind(gold_reward)
    .bind(cosmetic_reward_key)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_m6_db_err)?;

    tx.commit().await?;
    Ok(rec)
}

pub async fn list_chapter_bosses(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
) -> Result<Vec<ChapterBossRecord>> {
    let mut tx = begin_as(pool, actor).await?;
    let rows = sqlx::query_as::<_, ChapterBossRecord>(
        "SELECT id, tenant_id, course_id, node_id, title, mode, max_hp, current_hp, \
                reward_tier, gold_reward, cosmetic_reward_key, status, expires_at, defeated_at, created_at \
         FROM chapter_bosses \
         WHERE course_id = $1 \
         ORDER BY created_at DESC",
    )
    .bind(course_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(map_m6_db_err)?;
    tx.commit().await?;
    Ok(rows)
}

pub async fn get_chapter_boss(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    boss_id: Uuid,
) -> Result<ChapterBossRecord> {
    let mut tx = begin_as(pool, actor).await?;
    let row = sqlx::query_as::<_, ChapterBossRecord>(
        "SELECT id, tenant_id, course_id, node_id, title, mode, max_hp, current_hp, \
                reward_tier, gold_reward, cosmetic_reward_key, status, expires_at, defeated_at, created_at \
         FROM chapter_bosses \
         WHERE id = $1 AND course_id = $2",
    )
    .bind(boss_id)
    .bind(course_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_m6_db_err)?
    .ok_or(StorageError::NotFound)?;
    tx.commit().await?;
    Ok(row)
}

pub async fn create_or_join_boss_lobby(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    boss_id: Uuid,
    lobby_id: Option<Uuid>,
) -> Result<BossLobbyRecord> {
    let mut tx = begin_as(pool, actor).await?;
    let rec = match lobby_id {
        Some(existing_id) => {
            let row = sqlx::query_as::<_, BossLobbyRecord>(
                "SELECT id, boss_id, course_id, host_user_id, member_user_ids, max_players, status, created_at \
                 FROM boss_lobbies WHERE id = $1 AND boss_id = $2 AND course_id = $3 FOR UPDATE",
            )
            .bind(existing_id)
            .bind(boss_id)
            .bind(course_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(map_m6_db_err)?
            .ok_or(StorageError::NotFound)?;

            if !row.member_user_ids.contains(&actor) {
                if row.member_user_ids.len() >= row.max_players as usize {
                    return Err(StorageError::Invalid("boss lobby is full"));
                }
                sqlx::query_as::<_, BossLobbyRecord>(
                    "UPDATE boss_lobbies \
                     SET member_user_ids = array_append(member_user_ids, $1) \
                     WHERE id = $2 \
                     RETURNING id, boss_id, course_id, host_user_id, member_user_ids, max_players, status, created_at",
                )
                .bind(actor)
                .bind(existing_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(map_m6_db_err)?
            } else {
                row
            }
        }
        None => sqlx::query_as::<_, BossLobbyRecord>(
            "INSERT INTO boss_lobbies (boss_id, course_id, host_user_id, member_user_ids, max_players, status) \
             VALUES ($1, $2, $3, ARRAY[$3]::uuid[], 4, 'OPEN') \
             RETURNING id, boss_id, course_id, host_user_id, member_user_ids, max_players, status, created_at",
        )
        .bind(boss_id)
        .bind(course_id)
        .bind(actor)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_m6_db_err)?,
    };

    tx.commit().await?;
    Ok(rec)
}

pub async fn get_boss_lobby(pool: &PgPool, actor: Uuid, lobby_id: Uuid) -> Result<BossLobbyRecord> {
    let mut tx = begin_as(pool, actor).await?;
    let row = sqlx::query_as::<_, BossLobbyRecord>(
        "SELECT id, boss_id, course_id, host_user_id, member_user_ids, max_players, status, created_at \
         FROM boss_lobbies WHERE id = $1",
    )
    .bind(lobby_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_m6_db_err)?
    .ok_or(StorageError::NotFound)?;
    tx.commit().await?;
    Ok(row)
}

#[allow(clippy::too_many_arguments)]
pub async fn strike_chapter_boss(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    boss_id: Uuid,
    lobby_id: Option<Uuid>,
    sequence: i32,
    rating: i16,
    overcharged: bool,
    base_damage: i32,
    settlement_key: &str,
) -> Result<BossStrikeRecord> {
    let mut tx = begin_as(pool, actor).await?;
    let rec = sqlx::query_as::<_, BossStrikeRecord>(
        "SELECT raw_damage, applied_damage, overkill_damage, boss_hp_after, boss_status \
         FROM apply_boss_strike_secure($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(boss_id)
    .bind(course_id)
    .bind(lobby_id)
    .bind(sequence)
    .bind(rating)
    .bind(overcharged)
    .bind(base_damage)
    .bind(settlement_key)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_m6_db_err)?;
    tx.commit().await?;
    Ok(rec)
}

pub async fn claim_boss_chest(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    boss_id: Uuid,
    settlement_key: &str,
) -> Result<BossChestClaimResult> {
    let mut tx = begin_as(pool, actor).await?;
    let rec = sqlx::query_as::<_, BossChestClaimResult>(
        "SELECT claimed_now, reward_tier, gold_awarded, cosmetic_key \
         FROM claim_boss_chest_secure($1, $2, $3)",
    )
    .bind(boss_id)
    .bind(course_id)
    .bind(settlement_key)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_m6_db_err)?;
    tx.commit().await?;
    Ok(rec)
}

pub async fn create_clan(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    name: &str,
) -> Result<ClanRecord> {
    let mut tx = begin_as(pool, actor).await?;
    let tenant_id: Uuid = sqlx::query_scalar("SELECT tenant_id FROM courses WHERE id = $1")
        .bind(course_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_m6_db_err)?
        .ok_or(StorageError::NotFound)?;

    let clan = sqlx::query_as::<_, ClanRecord>(
        "INSERT INTO clans (tenant_id, course_id, name, created_by) \
         VALUES ($1, $2, $3, $4) \
         RETURNING id, tenant_id, course_id, name, created_by, created_at",
    )
    .bind(tenant_id)
    .bind(course_id)
    .bind(name)
    .bind(actor)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_m6_db_err)?;

    sqlx::query(
        "INSERT INTO clan_memberships (clan_id, course_id, user_id, role) \
         VALUES ($1, $2, $3, 'LEADER')",
    )
    .bind(clan.id)
    .bind(course_id)
    .bind(actor)
    .execute(&mut *tx)
    .await
    .map_err(map_m6_db_err)?;

    tx.commit().await?;
    Ok(clan)
}

pub async fn join_clan(pool: &PgPool, actor: Uuid, course_id: Uuid, clan_id: Uuid) -> Result<()> {
    let mut tx = begin_as(pool, actor).await?;
    sqlx::query(
        "INSERT INTO clan_memberships (clan_id, course_id, user_id, role) \
         VALUES ($1, $2, $3, 'MEMBER')",
    )
    .bind(clan_id)
    .bind(course_id)
    .bind(actor)
    .execute(&mut *tx)
    .await
    .map_err(map_m6_db_err)?;
    tx.commit().await?;
    Ok(())
}

pub async fn start_clan_war_week(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    clan_id: Uuid,
    week_key: &str,
    week_start_date: NaiveDate,
) -> Result<ClanWarRecord> {
    let mut tx = begin_as(pool, actor).await?;
    let member_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT user_id FROM clan_memberships WHERE clan_id = $1 AND course_id = $2 ORDER BY joined_at ASC, user_id ASC",
    )
    .bind(clan_id)
    .bind(course_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(map_m6_db_err)?;

    if member_ids.is_empty() {
        return Err(StorageError::Invalid("clan has no members"));
    }

    let snapshot_count = member_ids.len() as i32;
    let war = sqlx::query_as::<_, ClanWarRecord>(
        "INSERT INTO clan_wars (course_id, clan_id, week_key, week_start_date, war_roster_member_ids, active_members_snapshot) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         ON CONFLICT (clan_id, week_key) DO UPDATE \
         SET status = clan_wars.status \
         RETURNING id, course_id, clan_id, week_key, week_start_date, war_roster_member_ids, \
                   active_members_snapshot, total_raw_points, total_normalized_points, status, started_at",
    )
    .bind(course_id)
    .bind(clan_id)
    .bind(week_key)
    .bind(week_start_date)
    .bind(&member_ids)
    .bind(snapshot_count)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_m6_db_err)?;

    tx.commit().await?;
    Ok(war)
}

#[allow(clippy::too_many_arguments)]
pub async fn record_clan_war_stability_or_raid(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    war_id: Uuid,
    contribution_kind: &str,
    s_before: Option<f64>,
    s_after: Option<f64>,
    reward_eligible: bool,
    settlement_key: &str,
) -> Result<ClanWarRecord> {
    let mut tx = begin_as(pool, actor).await?;
    let war = sqlx::query_as::<_, ClanWarRecord>(
        "SELECT id, course_id, clan_id, week_key, week_start_date, war_roster_member_ids, \
                active_members_snapshot, total_raw_points, total_normalized_points, status, started_at \
         FROM clan_wars WHERE id = $1 AND course_id = $2",
    )
    .bind(war_id)
    .bind(course_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_m6_db_err)?
    .ok_or(StorageError::NotFound)?;

    let in_roster = war.war_roster_member_ids.contains(&actor);
    if !in_roster {
        return Err(StorageError::Forbidden);
    }

    let (db_kind, raw_points, normalized_points, relative_gain_bps) = match contribution_kind {
        "STABILITY" | "STABILITY_GAIN" => {
            let sb = s_before.ok_or(StorageError::Invalid("s_before is required"))?;
            let sa = s_after.ok_or(StorageError::Invalid("s_after is required"))?;
            let out = compute_clan_stability_points(&ClanStabilityInput {
                s_before: sb,
                s_after: sa,
                active_members_snapshot: u32::try_from(war.active_members_snapshot.max(1))
                    .unwrap_or(1),
                reward_eligible,
                member_in_frozen_roster: true,
            })
            .map_err(|_| StorageError::Invalid("invalid stability inputs"))?;
            (
                "STABILITY",
                out.raw_points as i32,
                out.normalized_points as i32,
                out.relative_gain_bps as i32,
            )
        }
        "RAID_WIN" => {
            if reward_eligible {
                ("RAID_WIN", 25, 25, 0)
            } else {
                ("RAID_WIN", 0, 0, 0)
            }
        }
        "REVENGE_WIN" => {
            if reward_eligible {
                ("REVENGE_WIN", 38, 38, 0)
            } else {
                ("REVENGE_WIN", 0, 0, 0)
            }
        }
        _ => return Err(StorageError::Invalid("unsupported contribution_kind")),
    };

    sqlx::query("SELECT record_clan_war_contribution_secure($1, $2, $3, $4, $5, $6, $7)")
        .bind(war_id)
        .bind(course_id)
        .bind(db_kind)
        .bind(raw_points)
        .bind(normalized_points)
        .bind(relative_gain_bps)
        .bind(settlement_key)
        .execute(&mut *tx)
        .await
        .map_err(map_m6_db_err)?;

    let updated = sqlx::query_as::<_, ClanWarRecord>(
        "SELECT id, course_id, clan_id, week_key, week_start_date, war_roster_member_ids, \
                active_members_snapshot, total_raw_points, total_normalized_points, status, started_at \
         FROM clan_wars WHERE id = $1",
    )
    .bind(war_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_m6_db_err)?;

    tx.commit().await?;
    Ok(updated)
}

pub async fn get_clan_war_state(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    war_id: Uuid,
) -> Result<(ClanWarRecord, Vec<ClanWarContributionRecord>)> {
    let mut tx = begin_as(pool, actor).await?;
    let war = sqlx::query_as::<_, ClanWarRecord>(
        "SELECT id, course_id, clan_id, week_key, week_start_date, war_roster_member_ids, \
                active_members_snapshot, total_raw_points, total_normalized_points, status, started_at \
         FROM clan_wars WHERE id = $1 AND course_id = $2",
    )
    .bind(war_id)
    .bind(course_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_m6_db_err)?
    .ok_or(StorageError::NotFound)?;

    let contributions = sqlx::query_as::<_, ClanWarContributionRecord>(
        "SELECT id, war_id, clan_id, course_id, user_id, contribution_kind, raw_points, \
                normalized_points, relative_gain_bps, settlement_key, created_at \
         FROM clan_war_contributions WHERE war_id = $1 ORDER BY created_at ASC",
    )
    .bind(war_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(map_m6_db_err)?;

    tx.commit().await?;
    Ok((war, contributions))
}

pub async fn get_user_economy_and_cosmetics(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
) -> Result<UserEconomyOverview> {
    let mut tx = begin_as(pool, actor).await?;
    let balance = sqlx::query_as::<_, EconomyBalanceRecord>(
        "SELECT user_id, course_id, gold, trophies, gems, practice_xp, daily_practice_xp, daily_practice_xp_date, updated_at \
         FROM user_economy_balances WHERE user_id = $1 AND course_id = $2",
    )
    .bind(actor)
    .bind(course_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_m6_db_err)?;

    let cosmetics = sqlx::query_as::<_, CosmeticInventoryRecord>(
        "SELECT id, user_id, course_id, cosmetic_key, rarity, source_kind, settlement_key, acquired_at \
         FROM user_cosmetic_inventory WHERE user_id = $1 AND course_id = $2 ORDER BY acquired_at DESC",
    )
    .bind(actor)
    .bind(course_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(map_m6_db_err)?;

    let recent_transactions = sqlx::query_as::<_, RewardTransactionRecord>(
        "SELECT id, user_id, course_id, source_kind, source_id, settlement_key, gold_delta, \
                trophies_delta, gems_delta, practice_xp_delta, chest_tier, quota_date, created_at \
         FROM reward_transactions WHERE user_id = $1 AND course_id = $2 ORDER BY created_at DESC LIMIT 50",
    )
    .bind(actor)
    .bind(course_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(map_m6_db_err)?;

    tx.commit().await?;
    Ok(UserEconomyOverview {
        balance,
        cosmetics,
        recent_transactions,
    })
}
