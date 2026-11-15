use std::collections::BTreeSet;

use campuslingo_domain::memory::validate_fsrs6_parameters;
use campuslingo_domain::shields::{
    CourseShieldInput, CourseShieldOutput, DORMANCY_THRESHOLD_DAYS, MIN_DORMANCY_CYCLE_DAYS,
    NEW_PLAYER_BUBBLE_HOURS, NodeShieldEvaluation, POST_RAID_COOLDOWN_HOURS, PrerequisiteEdgeInput,
    RESUME_GRACE_HOURS, STALE_RECOMPUTE_SECS, ShieldCardInput, ShieldNodeInput,
    VERSION_GRACE_ROLLING_WINDOW_DAYS, VersionGraceInput, VersionGraceWindow,
    compute_version_grace, evaluate_course_shields,
};
use campuslingo_domain::windows::Accommodation;
pub use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{StorageError, begin_as, is_permission_denied, is_unique_violation};

fn is_exclusion_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|db| db.code())
        .as_deref()
        == Some("23P01")
}

fn is_check_or_fk_violation(error: &sqlx::Error) -> bool {
    matches!(
        error
            .as_database_error()
            .and_then(|db| db.code())
            .as_deref(),
        Some("23514" | "23503")
    )
}

fn map_shield_db_err(error: sqlx::Error) -> StorageError {
    if is_exclusion_violation(&error) || is_unique_violation(&error) {
        StorageError::Conflict
    } else if is_permission_denied(&error) {
        StorageError::Forbidden
    } else if is_check_or_fk_violation(&error) {
        StorageError::Invalid("violates shield schema or DAG invariant")
    } else {
        StorageError::Database(error)
    }
}

pub type Result<T> = std::result::Result<T, StorageError>;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct FsrsParametersRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub tenant_id: Uuid,
    pub version: i32,
    pub weights: Vec<f32>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct UserProtectionStateRecord {
    pub user_id: Uuid,
    pub tenant_id: Uuid,
    pub last_qualifying_study_at: DateTime<Utc>,
    pub is_dormant: bool,
    pub return_study_at: Option<DateTime<Utc>>,
    pub resume_grace_until: Option<DateTime<Utc>>,
    pub last_resume_grace_granted_at: Option<DateTime<Utc>>,
    pub new_player_bubble_until: DateTime<Utc>,
    pub eligible_transition_count: i32,
    pub consecutive_dormant_weeks: i16,
    pub accommodation_multiplier: f32,
    pub accommodation_version: i32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct DormancyPeriodRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub tenant_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct CachedNodeShieldRecord {
    pub user_id: Uuid,
    pub node_id: Uuid,
    pub tenant_id: Uuid,
    pub course_id: Uuid,
    pub content_version_id: Option<Uuid>,
    pub state: String,
    pub tier: Option<i16>,
    pub e_u_ticks: Option<i32>,
    pub i_u_ticks: Option<i32>,
    pub s_u_ticks: Option<i32>,
    pub is_brownout: Option<bool>,
    pub unlocked_at: Option<DateTime<Utc>>,
    pub raid_immune: bool,
    pub propagation_exempt: bool,
    pub active_protections: Vec<String>,
    pub computed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FortressEdgeView {
    pub prerequisite_node_id: Uuid,
    pub dependent_node_id: Uuid,
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct FortressNodeView {
    pub node_id: Uuid,
    pub title: String,
    pub world_theme: Option<String>,
    pub archetype: Option<String>,
    pub modifier: Option<String>,
    pub grid_x: Option<i32>,
    pub grid_y: Option<i32>,
    pub shield: Option<CachedNodeShieldRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CourseShieldDashboard {
    pub course_id: Uuid,
    pub tenant_id: Uuid,
    pub active_content_version_id: Option<Uuid>,
    pub updating: bool,
    pub computed_at: Option<DateTime<Utc>>,
    pub protection_state: UserProtectionStateRecord,
    pub nodes: Vec<FortressNodeView>,
    pub edges: Vec<FortressEdgeView>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewNodeReport {
    pub content_version_id: Option<Uuid>,
    pub reason: String,
    #[serde(default)]
    pub details: Option<String>,
    #[serde(default)]
    pub reporter_cluster: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeReportOutcome {
    pub report_id: Uuid,
    pub content_version_id: Uuid,
    pub is_eligible: bool,
    pub refund_applied: bool,
    pub local_replacement_card_id: Option<Uuid>,
    pub version_quarantined: bool,
}

async fn resolve_course_tenant_and_check_read(
    transaction: &mut Transaction<'_, Postgres>,
    course_id: Uuid,
) -> Result<Uuid> {
    let row = sqlx::query_scalar::<_, Uuid>(
        "SELECT tenant_id FROM courses WHERE id = $1 AND can_read_course(id)",
    )
    .bind(course_id)
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or(StorageError::NotFound)?;

    let can_access: bool =
        sqlx::query_scalar("SELECT can_study_course($1) OR can_manage_course($1)")
            .bind(course_id)
            .fetch_one(&mut **transaction)
            .await?;
    if !can_access {
        return Err(StorageError::Forbidden);
    }
    Ok(row)
}

async fn resolve_active_version(
    transaction: &mut Transaction<'_, Postgres>,
    course_id: Uuid,
) -> Result<Option<(Uuid, Option<DateTime<Utc>>)>> {
    let row = sqlx::query_as::<_, (Uuid, Option<DateTime<Utc>>)>(
        "SELECT id, activated_at
         FROM content_versions
         WHERE course_id = $1 AND NOT is_quarantined
         ORDER BY version_number DESC
         LIMIT 1",
    )
    .bind(course_id)
    .fetch_optional(&mut **transaction)
    .await?;
    Ok(row)
}

async fn ensure_user_protection_state_tx(
    transaction: &mut Transaction<'_, Postgres>,
    actor: Uuid,
    tenant_id: Uuid,
) -> Result<UserProtectionStateRecord> {
    let created_at =
        sqlx::query_scalar::<_, Option<DateTime<Utc>>>("SELECT user_account_created_at($1)")
            .bind(actor)
            .fetch_one(&mut **transaction)
            .await?
            .ok_or(StorageError::NotFound)?;

    let bubble_until = created_at + Duration::hours(NEW_PLAYER_BUBBLE_HOURS);
    sqlx::query(
        "INSERT INTO user_protection_state (
             user_id, tenant_id, last_qualifying_study_at, new_player_bubble_until
         ) VALUES ($1, $2, $3, $4)
         ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(actor)
    .bind(tenant_id)
    .bind(created_at)
    .bind(bubble_until)
    .execute(&mut **transaction)
    .await
    .map_err(map_shield_db_err)?;

    let row = sqlx::query_as::<_, UserProtectionStateRecord>(
        "SELECT user_id, tenant_id, last_qualifying_study_at, is_dormant,
                return_study_at, resume_grace_until, last_resume_grace_granted_at,
                new_player_bubble_until, eligible_transition_count,
                consecutive_dormant_weeks, accommodation_multiplier, accommodation_version,
                updated_at
         FROM user_protection_state
         WHERE user_id = $1",
    )
    .bind(actor)
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or(StorageError::Forbidden)?;

    Ok(row)
}

/// Persists a validated 21-weight FSRS-6 parameter vector (`S-19`) in PostgreSQL `REAL[]`.
pub async fn save_user_fsrs_parameters(
    pool: &PgPool,
    actor: Uuid,
    tenant_id: Uuid,
    weights: &[f32],
) -> Result<FsrsParametersRecord> {
    let valid = validate_fsrs6_parameters(weights)
        .map_err(|_| StorageError::Invalid("invalid FSRS-6 parameters"))?;
    let mut transaction = begin_as(pool, actor).await?;

    let in_scope: bool = sqlx::query_scalar("SELECT user_has_tenant_scope($1)")
        .bind(tenant_id)
        .fetch_one(&mut *transaction)
        .await?;
    if !in_scope {
        return Err(StorageError::Forbidden);
    }

    sqlx::query(
        "UPDATE user_fsrs_parameters SET is_active = FALSE WHERE user_id = $1 AND is_active",
    )
    .bind(actor)
    .execute(&mut *transaction)
    .await
    .map_err(map_shield_db_err)?;

    let next_version: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version), 0) + 1 FROM user_fsrs_parameters WHERE user_id = $1",
    )
    .bind(actor)
    .fetch_one(&mut *transaction)
    .await?;

    let weight_vec: Vec<f32> = valid.to_vec();
    let row = sqlx::query_as::<_, FsrsParametersRecord>(
        "INSERT INTO user_fsrs_parameters (user_id, tenant_id, version, weights, is_active)
         VALUES ($1, $2, $3, $4, TRUE)
         RETURNING id, user_id, tenant_id, version, weights, is_active, created_at",
    )
    .bind(actor)
    .bind(tenant_id)
    .bind(next_version)
    .bind(&weight_vec)
    .fetch_one(&mut *transaction)
    .await
    .map_err(map_shield_db_err)?;

    transaction.commit().await?;
    Ok(row)
}

/// Loads the user's active FSRS-6 21-weight parameter vector (`S-19`) with finite validation.
pub async fn load_active_fsrs_parameters(
    pool: &PgPool,
    actor: Uuid,
) -> Result<Option<FsrsParametersRecord>> {
    let mut transaction = begin_as(pool, actor).await?;
    let row = sqlx::query_as::<_, FsrsParametersRecord>(
        "SELECT id, user_id, tenant_id, version, weights, is_active, created_at
         FROM user_fsrs_parameters
         WHERE user_id = $1 AND is_active",
    )
    .bind(actor)
    .fetch_optional(&mut *transaction)
    .await?;

    if let Some(ref rec) = row {
        validate_fsrs6_parameters(&rec.weights)
            .map_err(|_| StorageError::Invalid("corrupted FSRS-6 weights in storage"))?;
    }
    transaction.commit().await?;
    Ok(row)
}

/// Updates user accommodation multiplier using exact approved values (`DEC-010`).
pub async fn set_user_accommodation(
    pool: &PgPool,
    actor: Uuid,
    tenant_id: Uuid,
    multiplier: f32,
) -> Result<UserProtectionStateRecord> {
    let accommodation = Accommodation::try_from(multiplier)
        .map_err(|_| StorageError::Invalid("invalid accommodation multiplier"))?;
    let verified_multiplier: f32 = match accommodation {
        Accommodation::M100 => 1.0,
        Accommodation::M125 => 1.25,
        Accommodation::M150 => 1.5,
    };
    let mut transaction = begin_as(pool, actor).await?;
    ensure_user_protection_state_tx(&mut transaction, actor, tenant_id).await?;

    let row = sqlx::query_as::<_, UserProtectionStateRecord>(
        "UPDATE user_protection_state
         SET accommodation_multiplier = $2,
             accommodation_version = accommodation_version + 1,
             updated_at = now()
         WHERE user_id = $1
         RETURNING user_id, tenant_id, last_qualifying_study_at, is_dormant,
                   return_study_at, resume_grace_until, last_resume_grace_granted_at,
                   new_player_bubble_until, eligible_transition_count,
                   consecutive_dormant_weeks, accommodation_multiplier, accommodation_version,
                   updated_at",
    )
    .bind(actor)
    .bind(verified_multiplier)
    .fetch_one(&mut *transaction)
    .await
    .map_err(map_shield_db_err)?;

    transaction.commit().await?;
    Ok(row)
}

/// Initializes or retrieves the caller's single protection state row without resetting across tenants (`DEC-005`).
pub async fn ensure_user_protection_state(
    pool: &PgPool,
    actor: Uuid,
    tenant_id: Uuid,
) -> Result<UserProtectionStateRecord> {
    let mut transaction = begin_as(pool, actor).await?;
    let in_scope: bool = sqlx::query_scalar("SELECT user_has_tenant_scope($1)")
        .bind(tenant_id)
        .fetch_one(&mut *transaction)
        .await?;
    if !in_scope {
        return Err(StorageError::Forbidden);
    }
    let state = ensure_user_protection_state_tx(&mut transaction, actor, tenant_id).await?;
    transaction.commit().await?;
    Ok(state)
}

/// Opens a study-anchored dormancy period when `at - last_qualifying_study_at >= 7 days` (§9.6).
pub async fn enter_user_dormancy(
    pool: &PgPool,
    actor: Uuid,
    tenant_id: Uuid,
    at: DateTime<Utc>,
) -> Result<DormancyPeriodRecord> {
    let mut transaction = begin_as(pool, actor).await?;
    let state = ensure_user_protection_state_tx(&mut transaction, actor, tenant_id).await?;

    if (at - state.last_qualifying_study_at) < Duration::days(DORMANCY_THRESHOLD_DAYS) {
        return Err(StorageError::Invalid(
            "dormancy requires >= 7 days since last qualifying study",
        ));
    }

    let started_at = state.last_qualifying_study_at + Duration::days(DORMANCY_THRESHOLD_DAYS);
    sqlx::query(
        "UPDATE user_protection_state
         SET is_dormant = TRUE, updated_at = $2
         WHERE user_id = $1",
    )
    .bind(actor)
    .bind(at)
    .execute(&mut *transaction)
    .await
    .map_err(map_shield_db_err)?;

    let period = sqlx::query_as::<_, DormancyPeriodRecord>(
        "INSERT INTO user_dormancy_periods (user_id, tenant_id, started_at, ended_at)
         VALUES ($1, $2, $3, NULL)
         RETURNING id, user_id, tenant_id, started_at, ended_at",
    )
    .bind(actor)
    .bind(state.tenant_id)
    .bind(started_at)
    .fetch_one(&mut *transaction)
    .await
    .map_err(map_shield_db_err)?;

    transaction.commit().await?;
    Ok(period)
}

/// Inserts an explicit dormancy interval for testing / administrative backfill; fails with Conflict on overlap.
pub async fn insert_dormancy_period(
    pool: &PgPool,
    actor: Uuid,
    tenant_id: Uuid,
    started_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
) -> Result<DormancyPeriodRecord> {
    let mut transaction = begin_as(pool, actor).await?;
    let state = ensure_user_protection_state_tx(&mut transaction, actor, tenant_id).await?;

    let period = sqlx::query_as::<_, DormancyPeriodRecord>(
        "INSERT INTO user_dormancy_periods (user_id, tenant_id, started_at, ended_at)
         VALUES ($1, $2, $3, $4)
         RETURNING id, user_id, tenant_id, started_at, ended_at",
    )
    .bind(actor)
    .bind(state.tenant_id)
    .bind(started_at)
    .bind(ended_at)
    .fetch_one(&mut *transaction)
    .await
    .map_err(map_shield_db_err)?;

    transaction.commit().await?;
    Ok(period)
}

/// Records a qualifying study event, closing dormancy & granting 72h RESUME_GRACE iff >= 14d cycle elapsed (§9.6).
pub async fn record_qualifying_study(
    pool: &PgPool,
    actor: Uuid,
    tenant_id: Uuid,
    at: DateTime<Utc>,
    eligible_transitions_delta: i32,
) -> Result<UserProtectionStateRecord> {
    if eligible_transitions_delta < 0 {
        return Err(StorageError::Invalid("negative transition delta"));
    }
    let mut transaction = begin_as(pool, actor).await?;
    let state = ensure_user_protection_state_tx(&mut transaction, actor, tenant_id).await?;

    let elapsed_since_study = at - state.last_qualifying_study_at;
    let returning_from_dormancy =
        state.is_dormant || elapsed_since_study >= Duration::days(DORMANCY_THRESHOLD_DAYS);

    let mut return_study_at = state.return_study_at;
    let mut resume_grace_until = state.resume_grace_until;
    let mut last_resume_granted = state.last_resume_grace_granted_at;

    if returning_from_dormancy {
        let open_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM user_dormancy_periods WHERE user_id = $1 AND ended_at IS NULL LIMIT 1",
        )
        .bind(actor)
        .fetch_optional(&mut *transaction)
        .await?;

        if let Some(pid) = open_id {
            sqlx::query("UPDATE user_dormancy_periods SET ended_at = $2 WHERE id = $1")
                .bind(pid)
                .bind(at)
                .execute(&mut *transaction)
                .await
                .map_err(map_shield_db_err)?;
        } else if elapsed_since_study >= Duration::days(DORMANCY_THRESHOLD_DAYS) {
            let started_at =
                state.last_qualifying_study_at + Duration::days(DORMANCY_THRESHOLD_DAYS);
            sqlx::query(
                "INSERT INTO user_dormancy_periods (user_id, tenant_id, started_at, ended_at)
                 VALUES ($1, $2, $3, $4)",
            )
            .bind(actor)
            .bind(state.tenant_id)
            .bind(started_at)
            .bind(at)
            .execute(&mut *transaction)
            .await
            .map_err(map_shield_db_err)?;
        }

        return_study_at = Some(at);
        let eligible_for_resume_grace = match state.last_resume_grace_granted_at {
            None => true,
            Some(prev) => (at - prev) >= Duration::days(MIN_DORMANCY_CYCLE_DAYS),
        };
        if eligible_for_resume_grace {
            resume_grace_until = Some(at + Duration::hours(RESUME_GRACE_HOURS));
            last_resume_granted = Some(at);
        }
    }

    let next_study_at = if at > state.last_qualifying_study_at {
        at
    } else {
        state.last_qualifying_study_at
    };

    let updated = sqlx::query_as::<_, UserProtectionStateRecord>(
        "UPDATE user_protection_state
         SET last_qualifying_study_at = $2,
             is_dormant = FALSE,
             return_study_at = $3,
             resume_grace_until = $4,
             last_resume_grace_granted_at = $5,
             eligible_transition_count = eligible_transition_count + $6,
             updated_at = $7
         WHERE user_id = $1
         RETURNING user_id, tenant_id, last_qualifying_study_at, is_dormant,
                   return_study_at, resume_grace_until, last_resume_grace_granted_at,
                   new_player_bubble_until, eligible_transition_count,
                   consecutive_dormant_weeks, accommodation_multiplier, accommodation_version,
                   updated_at",
    )
    .bind(actor)
    .bind(next_study_at)
    .bind(return_study_at)
    .bind(resume_grace_until)
    .bind(last_resume_granted)
    .bind(eligible_transitions_delta)
    .bind(at)
    .fetch_one(&mut *transaction)
    .await
    .map_err(map_shield_db_err)?;

    transaction.commit().await?;
    Ok(updated)
}

/// Adds a same-course prerequisite DAG edge (`S-05`: trigger rejects cycles, depth > 16, closure > 256).
pub async fn add_node_prerequisite(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    prerequisite_node_id: Uuid,
    dependent_node_id: Uuid,
    weight: f32,
) -> Result<()> {
    if !weight.is_finite() || !(0.0..=1.0).contains(&weight) {
        return Err(StorageError::Invalid(
            "prerequisite weight must be in [0, 1]",
        ));
    }
    if prerequisite_node_id == dependent_node_id {
        return Err(StorageError::Invalid("self-loop forbidden"));
    }
    let mut transaction = begin_as(pool, actor).await?;
    let can_manage: bool = sqlx::query_scalar("SELECT can_manage_course($1)")
        .bind(course_id)
        .fetch_one(&mut *transaction)
        .await?;
    if !can_manage {
        return Err(StorageError::Forbidden);
    }

    let tenant_id = sqlx::query_scalar::<_, Uuid>("SELECT tenant_id FROM courses WHERE id = $1")
        .bind(course_id)
        .fetch_one(&mut *transaction)
        .await?;

    sqlx::query(
        "INSERT INTO node_prerequisites (
             prerequisite_node_id, dependent_node_id, tenant_id, course_id, weight
         ) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(prerequisite_node_id)
    .bind(dependent_node_id)
    .bind(tenant_id)
    .bind(course_id)
    .bind(weight)
    .execute(&mut *transaction)
    .await
    .map_err(map_shield_db_err)?;

    transaction.commit().await?;
    Ok(())
}

/// Attaches a challenge card to a content version (`S-07`) or updates its archived state without touching review logs.
pub async fn attach_card_to_version(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    content_version_id: Uuid,
    card_id: Uuid,
) -> Result<()> {
    let mut transaction = begin_as(pool, actor).await?;
    let can_manage: bool = sqlx::query_scalar("SELECT can_manage_course($1)")
        .bind(course_id)
        .fetch_one(&mut *transaction)
        .await?;
    if !can_manage {
        return Err(StorageError::Forbidden);
    }
    let tenant_id: Uuid = sqlx::query_scalar("SELECT tenant_id FROM courses WHERE id = $1")
        .bind(course_id)
        .fetch_one(&mut *transaction)
        .await?;

    sqlx::query(
        "INSERT INTO content_version_cards (content_version_id, card_id, tenant_id, course_id)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (content_version_id, card_id)
         DO UPDATE SET archived_at = NULL",
    )
    .bind(content_version_id)
    .bind(card_id)
    .bind(tenant_id)
    .bind(course_id)
    .execute(&mut *transaction)
    .await
    .map_err(map_shield_db_err)?;

    transaction.commit().await?;
    Ok(())
}

/// Non-destructively archives a card from `content_version_cards` (`S-07`) while keeping `fsrs_cards` & `review_logs`.
pub async fn archive_card_in_version(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    content_version_id: Uuid,
    card_id: Uuid,
    archived_at: DateTime<Utc>,
) -> Result<()> {
    let mut transaction = begin_as(pool, actor).await?;
    let can_manage: bool = sqlx::query_scalar("SELECT can_manage_course($1)")
        .bind(course_id)
        .fetch_one(&mut *transaction)
        .await?;
    if !can_manage {
        return Err(StorageError::Forbidden);
    }

    let res = sqlx::query(
        "UPDATE content_version_cards
         SET archived_at = $4
         WHERE content_version_id = $1 AND card_id = $2 AND course_id = $3",
    )
    .bind(content_version_id)
    .bind(card_id)
    .bind(course_id)
    .bind(archived_at)
    .execute(&mut *transaction)
    .await
    .map_err(map_shield_db_err)?;

    if res.rows_affected() == 0 {
        return Err(StorageError::NotFound);
    }
    transaction.commit().await?;
    Ok(())
}

/// Sets activation timestamp on a content version (§9.2 version transition; resets seen/engaged implicitly per version).
pub async fn activate_content_version(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    content_version_id: Uuid,
    activated_at: DateTime<Utc>,
) -> Result<()> {
    let mut transaction = begin_as(pool, actor).await?;
    let can_manage: bool = sqlx::query_scalar("SELECT can_manage_course($1)")
        .bind(course_id)
        .fetch_one(&mut *transaction)
        .await?;
    if !can_manage {
        return Err(StorageError::Forbidden);
    }

    let res = sqlx::query(
        "UPDATE content_versions
         SET activated_at = $3
         WHERE id = $1 AND course_id = $2",
    )
    .bind(content_version_id)
    .bind(course_id)
    .bind(activated_at)
    .execute(&mut *transaction)
    .await
    .map_err(map_shield_db_err)?;
    if res.rows_affected() == 0 {
        return Err(StorageError::NotFound);
    }
    transaction.commit().await?;
    Ok(())
}

/// Timing inputs evaluated when granting non-negative `VERSION_GRACE` (S-25).
#[derive(Debug, Clone, Copy)]
pub struct VersionGraceTiming {
    pub opened_at: Option<DateTime<Utc>>,
    pub engaged_at: Option<DateTime<Utc>>,
    pub now: DateTime<Utc>,
}

/// Evaluates & grants non-negative `VERSION_GRACE` (§9.2, S-25) bounded by 14d/30d ledger & once-ever per card.
pub async fn evaluate_and_grant_version_grace(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    node_id: Uuid,
    content_version_id: Uuid,
    timing: &VersionGraceTiming,
) -> Result<Option<VersionGraceWindow>> {
    let VersionGraceTiming {
        opened_at,
        engaged_at,
        now,
    } = *timing;
    let mut transaction = begin_as(pool, actor).await?;
    let tenant_id = resolve_course_tenant_and_check_read(&mut transaction, course_id).await?;
    ensure_user_protection_state_tx(&mut transaction, actor, tenant_id).await?;

    let version_activated_at: DateTime<Utc> = sqlx::query_scalar(
        "SELECT COALESCE(activated_at, created_at)
         FROM content_versions
         WHERE id = $1 AND course_id = $2 AND tenant_id = $3 AND NOT is_quarantined",
    )
    .bind(content_version_id)
    .bind(course_id)
    .bind(tenant_id)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or(StorageError::NotFound)?;

    if let Some(open_ts) = opened_at {
        sqlx::query(
            "INSERT INTO user_course_version_seen (user_id, course_id, content_version_id, tenant_id, first_opened_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (user_id, course_id, content_version_id) DO NOTHING",
        )
        .bind(actor)
        .bind(course_id)
        .bind(content_version_id)
        .bind(tenant_id)
        .bind(open_ts)
        .execute(&mut *transaction)
        .await
        .map_err(map_shield_db_err)?;
    }

    if let Some(eng_ts) = engaged_at {
        sqlx::query(
            "INSERT INTO user_node_version_engagement (user_id, node_id, content_version_id, tenant_id, course_id, engaged_at)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (user_id, node_id, content_version_id) DO NOTHING",
        )
        .bind(actor)
        .bind(node_id)
        .bind(content_version_id)
        .bind(tenant_id)
        .bind(course_id)
        .bind(eng_ts)
        .execute(&mut *transaction)
        .await
        .map_err(map_shield_db_err)?;
    }

    let first_open: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT first_opened_at FROM user_course_version_seen
         WHERE user_id = $1 AND course_id = $2 AND content_version_id = $3",
    )
    .bind(actor)
    .bind(course_id)
    .bind(content_version_id)
    .fetch_optional(&mut *transaction)
    .await?;

    let node_eng: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT engaged_at FROM user_node_version_engagement
         WHERE user_id = $1 AND node_id = $2 AND content_version_id = $3",
    )
    .bind(actor)
    .bind(node_id)
    .bind(content_version_id)
    .fetch_optional(&mut *transaction)
    .await?;

    // §9.2 historical dormancy check: was_dormant_at(user, version_activated_at)
    let dormant_row: Option<Option<DateTime<Utc>>> = sqlx::query_scalar(
        "SELECT ended_at FROM user_dormancy_periods
         WHERE user_id = $1
           AND started_at <= $2
           AND (ended_at IS NULL OR ended_at > $2)
         LIMIT 1",
    )
    .bind(actor)
    .bind(version_activated_at)
    .fetch_optional(&mut *transaction)
    .await?;

    let effective_notice_anchor: Option<DateTime<Utc>> = match dormant_row {
        Some(ended_at_opt) => ended_at_opt, // NULL if still dormant (+infinity)
        None => Some(version_activated_at),
    };

    let ungraced_cards: Vec<Uuid> = sqlx::query_scalar(
        "SELECT cvc.card_id
         FROM content_version_cards cvc
         JOIN challenges c ON c.id = cvc.card_id
         WHERE cvc.content_version_id = $1
           AND cvc.course_id = $2
           AND cvc.archived_at IS NULL
           AND c.node_id = $3
           AND NOT EXISTS (
               SELECT 1 FROM user_card_version_grace ucvg
               WHERE ucvg.user_id = $4 AND ucvg.card_id = cvc.card_id
           )
         ORDER BY cvc.card_id",
    )
    .bind(content_version_id)
    .bind(course_id)
    .bind(node_id)
    .bind(actor)
    .fetch_all(&mut *transaction)
    .await?;

    if ungraced_cards.is_empty() {
        transaction.commit().await?;
        return Ok(None);
    }

    let window_start = now - Duration::days(VERSION_GRACE_ROLLING_WINDOW_DAYS);
    let used_grace_sec_f64: f64 = sqlx::query_scalar(
        "SELECT COALESCE(
             SUM(EXTRACT(EPOCH FROM (LEAST(ended_at, $4) - GREATEST(started_at, $3)))),
             0
         )::DOUBLE PRECISION
         FROM user_node_grace_ledger
         WHERE user_id = $1 AND node_id = $2 AND ended_at > $3 AND started_at < $4",
    )
    .bind(actor)
    .bind(node_id)
    .bind(window_start)
    .bind(now)
    .fetch_one(&mut *transaction)
    .await?;

    let grace_opt = compute_version_grace(&VersionGraceInput {
        effective_notice_anchor_epoch_s: effective_notice_anchor.map(|ts| ts.timestamp()),
        first_course_open_after_activation_epoch_s: first_open.map(|ts| ts.timestamp()),
        node_engaged_at_epoch_s: node_eng.map(|ts| ts.timestamp()),
        new_ungraced_cards_count: u32::try_from(ungraced_cards.len()).unwrap_or(u32::MAX),
        used_grace_sec_in_last_30d: used_grace_sec_f64.round() as i64,
    });

    let Some(window) = grace_opt else {
        transaction.commit().await?;
        return Ok(None);
    };

    let started_ts = DateTime::<Utc>::from_timestamp(window.start_epoch_s, 0)
        .ok_or(StorageError::Invalid("invalid grace start timestamp"))?;
    let ended_ts = DateTime::<Utc>::from_timestamp(window.end_epoch_s, 0)
        .ok_or(StorageError::Invalid("invalid grace end timestamp"))?;

    let ledger_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO user_node_grace_ledger (
             user_id, node_id, tenant_id, course_id, content_version_id, started_at, ended_at
         ) VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id",
    )
    .bind(actor)
    .bind(node_id)
    .bind(tenant_id)
    .bind(course_id)
    .bind(content_version_id)
    .bind(started_ts)
    .bind(ended_ts)
    .fetch_one(&mut *transaction)
    .await
    .map_err(map_shield_db_err)?;

    for card_id in ungraced_cards {
        sqlx::query(
            "INSERT INTO user_card_version_grace (
                 user_id, card_id, tenant_id, course_id, content_version_id, granted_at, grace_until
             ) VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (user_id, card_id) DO NOTHING",
        )
        .bind(actor)
        .bind(card_id)
        .bind(tenant_id)
        .bind(course_id)
        .bind(content_version_id)
        .bind(started_ts)
        .bind(ended_ts)
        .execute(&mut *transaction)
        .await
        .map_err(map_shield_db_err)?;
    }

    sqlx::query(
        "INSERT INTO user_node_protection (
             user_id, node_id, tenant_id, course_id, kind, protected_until, source_event_id, computed_at
         ) VALUES ($1, $2, $3, $4, 'VERSION_GRACE', $5, $6, $7)
         ON CONFLICT (user_id, node_id, kind) DO UPDATE
         SET protected_until = GREATEST(user_node_protection.protected_until, EXCLUDED.protected_until),
             source_event_id = EXCLUDED.source_event_id,
             computed_at = EXCLUDED.computed_at",
    )
    .bind(actor)
    .bind(node_id)
    .bind(tenant_id)
    .bind(course_id)
    .bind(ended_ts)
    .bind(ledger_id)
    .bind(now)
    .execute(&mut *transaction)
    .await
    .map_err(map_shield_db_err)?;

    transaction.commit().await?;
    Ok(Some(window))
}

/// Applies 6h `POST_RAID_COOLDOWN` to a node (`S-24`, §9.5).
pub async fn grant_post_raid_cooldown(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    node_id: Uuid,
    at: DateTime<Utc>,
    source_event_id: Option<Uuid>,
) -> Result<()> {
    let mut transaction = begin_as(pool, actor).await?;
    let tenant_id = resolve_course_tenant_and_check_read(&mut transaction, course_id).await?;
    let until = at + Duration::hours(POST_RAID_COOLDOWN_HOURS);

    sqlx::query(
        "INSERT INTO user_node_protection (
             user_id, node_id, tenant_id, course_id, kind, protected_until, source_event_id, computed_at
         ) VALUES ($1, $2, $3, $4, 'POST_RAID_COOLDOWN', $5, $6, $7)
         ON CONFLICT (user_id, node_id, kind) DO UPDATE
         SET protected_until = GREATEST(user_node_protection.protected_until, EXCLUDED.protected_until),
             source_event_id = EXCLUDED.source_event_id,
             computed_at = EXCLUDED.computed_at",
    )
    .bind(actor)
    .bind(node_id)
    .bind(tenant_id)
    .bind(course_id)
    .bind(until)
    .bind(source_event_id)
    .bind(at)
    .execute(&mut *transaction)
    .await
    .map_err(map_shield_db_err)?;

    transaction.commit().await?;
    Ok(())
}

#[derive(FromRow)]
struct NodeMetaRow {
    id: Uuid,
}

#[derive(FromRow)]
struct PrevShieldRow {
    node_id: Uuid,
    is_brownout: Option<bool>,
    unlocked_at: Option<DateTime<Utc>>,
}

#[derive(FromRow)]
struct ActiveCardRow {
    card_id: Uuid,
    node_id: Uuid,
    stability: Option<f32>,
    reps: Option<i32>,
    last_review: Option<DateTime<Utc>>,
    grace_until: Option<DateTime<Utc>>,
}

#[derive(FromRow)]
struct NodeTimedProtRow {
    node_id: Uuid,
    kind: String,
    protected_until: DateTime<Utc>,
}

#[derive(FromRow)]
struct EdgeRow {
    prerequisite_node_id: Uuid,
    dependent_node_id: Uuid,
    weight: f32,
}

async fn compute_and_persist_shields_tx(
    transaction: &mut Transaction<'_, Postgres>,
    target_user: Uuid,
    course_id: Uuid,
    tenant_id: Uuid,
    now: DateTime<Utc>,
) -> Result<(CourseShieldOutput, Option<Uuid>)> {
    let prot_state = ensure_user_protection_state_tx(transaction, target_user, tenant_id).await?;
    let active_version = resolve_active_version(transaction, course_id).await?;
    let active_version_id = active_version.map(|(v, _)| v);

    let decay_override = sqlx::query_scalar::<_, Option<f32>>(
        "SELECT weights[21] FROM user_fsrs_parameters WHERE user_id = $1 AND is_active",
    )
    .bind(target_user)
    .fetch_optional(&mut **transaction)
    .await?
    .flatten();

    let node_rows = sqlx::query_as::<_, NodeMetaRow>(
        "SELECT id FROM learning_nodes WHERE course_id = $1 ORDER BY title, id",
    )
    .bind(course_id)
    .fetch_all(&mut **transaction)
    .await?;

    let edge_rows = sqlx::query_as::<_, EdgeRow>(
        "SELECT prerequisite_node_id, dependent_node_id, weight
         FROM node_prerequisites
         WHERE course_id = $1
         ORDER BY prerequisite_node_id, dependent_node_id",
    )
    .bind(course_id)
    .fetch_all(&mut **transaction)
    .await?;

    let prev_rows = sqlx::query_as::<_, PrevShieldRow>(
        "SELECT node_id, is_brownout, unlocked_at
         FROM user_node_shield_state
         WHERE user_id = $1 AND course_id = $2",
    )
    .bind(target_user)
    .bind(course_id)
    .fetch_all(&mut **transaction)
    .await?;

    let timed_prot_rows = sqlx::query_as::<_, NodeTimedProtRow>(
        "SELECT node_id, kind, protected_until
         FROM user_node_protection
         WHERE user_id = $1 AND course_id = $2 AND protected_until > $3",
    )
    .bind(target_user)
    .bind(course_id)
    .bind(now)
    .fetch_all(&mut **transaction)
    .await?;

    let version_has_card_map = match active_version_id {
        Some(ver_id) => sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(
                 SELECT 1 FROM content_version_cards WHERE course_id = $1 AND content_version_id = $2
             )",
        )
        .bind(course_id)
        .bind(ver_id)
        .fetch_one(&mut **transaction)
        .await?,
        None => false,
    };

    let active_cards: Vec<ActiveCardRow> = if let Some(ver_id) = active_version_id
        && version_has_card_map
    {
        sqlx::query_as::<_, ActiveCardRow>(
            "SELECT c.id AS card_id, c.node_id, fc.stability, fc.reps, fc.last_review,
                    ucvg.grace_until
             FROM content_version_cards cvc
             JOIN challenges c ON c.id = cvc.card_id
             LEFT JOIN actor_course_fsrs_card_states($1, $3) fc ON fc.challenge_id = c.id
             LEFT JOIN user_card_version_grace ucvg ON ucvg.card_id = c.id AND ucvg.user_id = $3
             WHERE cvc.course_id = $1
               AND cvc.content_version_id = $2
               AND cvc.archived_at IS NULL
             ORDER BY c.node_id, c.id",
        )
        .bind(course_id)
        .bind(ver_id)
        .bind(target_user)
        .fetch_all(&mut **transaction)
        .await?
    } else if active_version_id.is_none() {
        sqlx::query_as::<_, ActiveCardRow>(
            "SELECT c.id AS card_id, c.node_id, fc.stability, fc.reps, fc.last_review,
                    NULL::TIMESTAMPTZ AS grace_until
             FROM challenges c
             LEFT JOIN actor_course_fsrs_card_states($1, $2) fc ON fc.challenge_id = c.id
             WHERE c.course_id = $1
             ORDER BY c.node_id, c.id",
        )
        .bind(course_id)
        .bind(target_user)
        .fetch_all(&mut **transaction)
        .await?
    } else {
        sqlx::query_as::<_, ActiveCardRow>(
            "SELECT c.id AS card_id, c.node_id, fc.stability, fc.reps, fc.last_review,
                    ucvg.grace_until
             FROM challenges c
             LEFT JOIN actor_course_fsrs_card_states($1, $2) fc ON fc.challenge_id = c.id
             LEFT JOIN user_card_version_grace ucvg ON ucvg.card_id = c.id AND ucvg.user_id = $2
             WHERE c.course_id = $1
             ORDER BY c.node_id, c.id",
        )
        .bind(course_id)
        .bind(target_user)
        .fetch_all(&mut **transaction)
        .await?
    };

    let shield_nodes: Vec<ShieldNodeInput> = node_rows
        .iter()
        .map(|node| {
            let prev = prev_rows.iter().find(|p| p.node_id == node.id);
            let cooldown_until = timed_prot_rows
                .iter()
                .find(|p| p.node_id == node.id && p.kind == "POST_RAID_COOLDOWN")
                .map(|p| p.protected_until.timestamp());
            let version_grace_active = timed_prot_rows
                .iter()
                .any(|p| p.node_id == node.id && p.kind == "VERSION_GRACE");

            let cards_for_node = active_cards
                .iter()
                .filter(|c| c.node_id == node.id)
                .map(|c| ShieldCardInput {
                    card_id: c.card_id.to_string(),
                    stability: c.stability.unwrap_or(0.0),
                    reps: u32::try_from(c.reps.unwrap_or(0).max(0)).unwrap_or(0),
                    last_review_epoch_s: c.last_review.map(|ts| ts.timestamp()),
                    decay: decay_override,
                    version_grace_active: c.grace_until.is_some_and(|g| now < g),
                })
                .collect();

            ShieldNodeInput {
                node_id: node.id.to_string(),
                active_cards: cards_for_node,
                previous_brownout: prev.and_then(|p| p.is_brownout),
                unlocked_at_epoch_s: prev.and_then(|p| p.unlocked_at.map(|ts| ts.timestamp())),
                post_raid_cooldown_until_epoch_s: cooldown_until,
                version_grace_active,
            }
        })
        .collect();

    let edges: Vec<PrerequisiteEdgeInput> = edge_rows
        .iter()
        .map(|e| PrerequisiteEdgeInput {
            parent_node_id: e.prerequisite_node_id.to_string(),
            child_node_id: e.dependent_node_id.to_string(),
            weight: e.weight,
        })
        .collect();

    let output = evaluate_course_shields(&CourseShieldInput {
        now_epoch_s: now.timestamp(),
        last_qualifying_study_at_epoch_s: prot_state.last_qualifying_study_at.timestamp(),
        new_player_bubble_until_epoch_s: prot_state.new_player_bubble_until.timestamp(),
        eligible_transition_count: prot_state.eligible_transition_count,
        resume_grace_until_epoch_s: prot_state.resume_grace_until.map(|ts| ts.timestamp()),
        nodes: shield_nodes,
        edges,
    })
    .map_err(|_| StorageError::Invalid("failed to evaluate course shield DAG"))?;

    for eval in &output.nodes {
        persist_node_shield_eval_tx(
            transaction,
            target_user,
            tenant_id,
            course_id,
            active_version_id,
            eval,
            now,
        )
        .await?;
    }

    sqlx::query(
        "UPDATE shield_recompute_queue
         SET completed_at = $3
         WHERE user_id = $1 AND course_id = $2",
    )
    .bind(target_user)
    .bind(course_id)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(map_shield_db_err)?;

    Ok((output, active_version_id))
}

async fn persist_node_shield_eval_tx(
    transaction: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    tenant_id: Uuid,
    course_id: Uuid,
    active_version_id: Option<Uuid>,
    eval: &NodeShieldEvaluation,
    now: DateTime<Utc>,
) -> Result<()> {
    let node_id = Uuid::parse_str(&eval.node_id).map_err(|_| StorageError::Invalid("node_id"))?;
    let state_str = match eval.state {
        campuslingo_domain::shields::NodeState::Inactive => "INACTIVE",
        campuslingo_domain::shields::NodeState::Active => "ACTIVE",
        campuslingo_domain::shields::NodeState::Invalid => "INVALID",
    };
    let tier_i16 = eval.tier.map(i16::from);
    let e_ticks = eval.e_u_ticks.map(|v| v as i32);
    let i_ticks = eval.i_u_ticks.map(|v| v as i32);
    let s_ticks = eval.s_u_ticks.map(|v| v as i32);
    let unlocked_ts = eval
        .unlocked_at_epoch_s
        .and_then(|s| DateTime::<Utc>::from_timestamp(s, 0));
    let prot_names: Vec<String> = eval
        .protection
        .active_protections
        .iter()
        .map(|p| {
            serde_json::to_value(p)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default()
        })
        .collect();

    sqlx::query(
        "INSERT INTO user_node_shield_state (
             user_id, node_id, tenant_id, course_id, content_version_id,
             state, tier, e_u_ticks, i_u_ticks, s_u_ticks, is_brownout,
             unlocked_at, raid_immune, propagation_exempt, active_protections, computed_at
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
         ON CONFLICT (user_id, node_id) DO UPDATE
         SET content_version_id = EXCLUDED.content_version_id,
             state = EXCLUDED.state,
             tier = EXCLUDED.tier,
             e_u_ticks = EXCLUDED.e_u_ticks,
             i_u_ticks = EXCLUDED.i_u_ticks,
             s_u_ticks = EXCLUDED.s_u_ticks,
             is_brownout = EXCLUDED.is_brownout,
             unlocked_at = COALESCE(user_node_shield_state.unlocked_at, EXCLUDED.unlocked_at),
             raid_immune = EXCLUDED.raid_immune,
             propagation_exempt = EXCLUDED.propagation_exempt,
             active_protections = EXCLUDED.active_protections,
             computed_at = EXCLUDED.computed_at",
    )
    .bind(user_id)
    .bind(node_id)
    .bind(tenant_id)
    .bind(course_id)
    .bind(active_version_id)
    .bind(state_str)
    .bind(tier_i16)
    .bind(e_ticks)
    .bind(i_ticks)
    .bind(s_ticks)
    .bind(eval.is_brownout)
    .bind(unlocked_ts)
    .bind(eval.protection.raid_immune)
    .bind(eval.protection.propagation_exempt)
    .bind(&prot_names)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(map_shield_db_err)?;

    Ok(())
}

async fn load_dashboard_view_tx(
    transaction: &mut Transaction<'_, Postgres>,
    actor: Uuid,
    course_id: Uuid,
    tenant_id: Uuid,
    active_version_id: Option<Uuid>,
    updating: bool,
) -> Result<CourseShieldDashboard> {
    let prot_state = ensure_user_protection_state_tx(transaction, actor, tenant_id).await?;

    #[derive(FromRow)]
    struct JoinedNode {
        node_id: Uuid,
        title: String,
        world_theme: Option<String>,
        archetype: Option<String>,
        modifier: Option<String>,
        grid_x: Option<i32>,
        grid_y: Option<i32>,
    }

    let joined_nodes = sqlx::query_as::<_, JoinedNode>(
        "SELECT ln.id AS node_id, ln.title,
                lub.world_theme, lub.archetype, lub.modifier, lub.grid_x, lub.grid_y
         FROM learning_nodes ln
         LEFT JOIN learning_unit_bundles lub
           ON lub.node_id = ln.id AND lub.content_version_id = $2
         WHERE ln.course_id = $1
         ORDER BY COALESCE(lub.grid_y, 0), COALESCE(lub.grid_x, 0), ln.title, ln.id",
    )
    .bind(course_id)
    .bind(active_version_id)
    .fetch_all(&mut **transaction)
    .await?;

    let shields = sqlx::query_as::<_, CachedNodeShieldRecord>(
        "SELECT user_id, node_id, tenant_id, course_id, content_version_id,
                state, tier, e_u_ticks, i_u_ticks, s_u_ticks, is_brownout,
                unlocked_at, raid_immune, propagation_exempt, active_protections, computed_at
         FROM user_node_shield_state
         WHERE user_id = $1 AND course_id = $2",
    )
    .bind(actor)
    .bind(course_id)
    .fetch_all(&mut **transaction)
    .await?;

    let computed_at = shields.iter().map(|s| s.computed_at).min();
    let edge_rows = sqlx::query_as::<_, EdgeRow>(
        "SELECT prerequisite_node_id, dependent_node_id, weight
         FROM node_prerequisites
         WHERE course_id = $1
         ORDER BY prerequisite_node_id, dependent_node_id",
    )
    .bind(course_id)
    .fetch_all(&mut **transaction)
    .await?;

    let nodes = joined_nodes
        .into_iter()
        .map(|n| FortressNodeView {
            shield: shields.iter().find(|s| s.node_id == n.node_id).cloned(),
            node_id: n.node_id,
            title: n.title,
            world_theme: n.world_theme,
            archetype: n.archetype,
            modifier: n.modifier,
            grid_x: n.grid_x,
            grid_y: n.grid_y,
        })
        .collect();

    let edges = edge_rows
        .into_iter()
        .map(|e| FortressEdgeView {
            prerequisite_node_id: e.prerequisite_node_id,
            dependent_node_id: e.dependent_node_id,
            weight: e.weight,
        })
        .collect();

    Ok(CourseShieldDashboard {
        course_id,
        tenant_id,
        active_content_version_id: active_version_id,
        updating,
        computed_at,
        protection_state: prot_state,
        nodes,
        edges,
    })
}

/// Computes, persists, and returns fresh shields & fortress state for a course.
pub async fn recompute_course_shields(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    now: DateTime<Utc>,
) -> Result<CourseShieldDashboard> {
    let mut transaction = begin_as(pool, actor).await?;
    let tenant_id = resolve_course_tenant_and_check_read(&mut transaction, course_id).await?;
    let (_, active_version_id) =
        compute_and_persist_shields_tx(&mut transaction, actor, course_id, tenant_id, now).await?;
    let dashboard = load_dashboard_view_tx(
        &mut transaction,
        actor,
        course_id,
        tenant_id,
        active_version_id,
        false,
    )
    .await?;
    transaction.commit().await?;
    Ok(dashboard)
}

/// Serves cached shields immediately and enqueues async recompute if stale (>= 15 min, §9.6).
pub async fn get_course_shields_dashboard(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    now: DateTime<Utc>,
) -> Result<CourseShieldDashboard> {
    let mut transaction = begin_as(pool, actor).await?;
    let tenant_id = resolve_course_tenant_and_check_read(&mut transaction, course_id).await?;
    let active_version = resolve_active_version(&mut transaction, course_id).await?;
    let active_version_id = active_version.map(|(v, _)| v);

    let cached_min_computed: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT MIN(computed_at) FROM user_node_shield_state WHERE user_id = $1 AND course_id = $2",
    )
    .bind(actor)
    .bind(course_id)
    .fetch_one(&mut *transaction)
    .await?;

    let node_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM learning_nodes WHERE course_id = $1")
            .bind(course_id)
            .fetch_one(&mut *transaction)
            .await?;

    if cached_min_computed.is_none() && node_count > 0 {
        let (_, ver) =
            compute_and_persist_shields_tx(&mut transaction, actor, course_id, tenant_id, now)
                .await?;
        let dashboard =
            load_dashboard_view_tx(&mut transaction, actor, course_id, tenant_id, ver, false)
                .await?;
        transaction.commit().await?;
        return Ok(dashboard);
    }

    let is_stale =
        cached_min_computed.is_some_and(|ts| (now - ts) >= Duration::seconds(STALE_RECOMPUTE_SECS));

    if is_stale {
        sqlx::query(
            "INSERT INTO shield_recompute_queue (user_id, course_id, tenant_id, requested_at, completed_at)
             VALUES ($1, $2, $3, $4, NULL)
             ON CONFLICT (user_id, course_id) DO UPDATE
             SET requested_at = EXCLUDED.requested_at, completed_at = NULL",
        )
        .bind(actor)
        .bind(course_id)
        .bind(tenant_id)
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(map_shield_db_err)?;
    }

    let pending_queue: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM shield_recompute_queue
             WHERE user_id = $1 AND course_id = $2 AND completed_at IS NULL
         )",
    )
    .bind(actor)
    .bind(course_id)
    .fetch_one(&mut *transaction)
    .await?;

    let dashboard = load_dashboard_view_tx(
        &mut transaction,
        actor,
        course_id,
        tenant_id,
        active_version_id,
        is_stale || pending_queue,
    )
    .await?;
    transaction.commit().await?;
    Ok(dashboard)
}

/// Worker drain for `shield_recompute_queue` (uses `BYPASSRLS` worker pool).
pub async fn process_shield_recompute_batch(
    worker_pool: &PgPool,
    now: DateTime<Utc>,
    limit: i64,
) -> Result<usize> {
    let mut transaction = worker_pool.begin().await?;
    let pending: Vec<(Uuid, Uuid, Uuid)> = sqlx::query_as(
        "SELECT user_id, course_id, tenant_id
         FROM shield_recompute_queue
         WHERE completed_at IS NULL
         ORDER BY requested_at ASC
         LIMIT $1
         FOR UPDATE SKIP LOCKED",
    )
    .bind(limit)
    .fetch_all(&mut *transaction)
    .await?;

    let count = pending.len();
    for (user_id, course_id, tenant_id) in pending {
        compute_and_persist_shields_tx(&mut transaction, user_id, course_id, tenant_id, now)
            .await?;
    }
    transaction.commit().await?;
    Ok(count)
}

/// Version-scoped reporting (`S-13`): deduplicates per `(user_id, node_id, content_version_id)`,
/// enforces active enrollment & reporter cluster independence, grants local replacement/refund,
/// and quarantines `content_version_id` at >= 3 eligible independent reports without deleting `review_logs`.
pub async fn submit_node_report(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    node_id: Uuid,
    input: &NewNodeReport,
) -> Result<NodeReportOutcome> {
    let reason = input.reason.trim();
    if reason.is_empty() || reason.len() > 240 {
        return Err(StorageError::Invalid("report reason must be 1..=240 chars"));
    }
    if let Some(ref d) = input.details
        && d.len() > 2000
    {
        return Err(StorageError::Invalid("report details exceed 2000 chars"));
    }

    let mut transaction = begin_as(pool, actor).await?;
    let can_study: bool = sqlx::query_scalar("SELECT can_study_course($1)")
        .bind(course_id)
        .fetch_one(&mut *transaction)
        .await?;
    if !can_study {
        let can_read: bool = sqlx::query_scalar("SELECT can_read_course($1)")
            .bind(course_id)
            .fetch_one(&mut *transaction)
            .await?;
        return Err(if can_read {
            StorageError::Forbidden
        } else {
            StorageError::NotFound
        });
    }

    let tenant_id: Uuid = sqlx::query_scalar("SELECT tenant_id FROM courses WHERE id = $1")
        .bind(course_id)
        .fetch_one(&mut *transaction)
        .await?;

    let node_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM learning_nodes WHERE id = $1 AND course_id = $2)",
    )
    .bind(node_id)
    .bind(course_id)
    .fetch_one(&mut *transaction)
    .await?;
    if !node_exists {
        return Err(StorageError::NotFound);
    }

    let version_id = match input.content_version_id {
        Some(v) => v,
        None => resolve_active_version(&mut transaction, course_id)
            .await?
            .map(|(v, _)| v)
            .ok_or(StorageError::NotFound)?,
    };

    let version_valid: Option<bool> = sqlx::query_scalar(
        "SELECT is_quarantined FROM content_versions
         WHERE id = $1 AND course_id = $2 AND tenant_id = $3",
    )
    .bind(version_id)
    .bind(course_id)
    .bind(tenant_id)
    .fetch_optional(&mut *transaction)
    .await?;
    if version_valid.is_none() {
        return Err(StorageError::NotFound);
    }

    let cluster = input
        .reporter_cluster
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("user:{actor}"));
    if cluster.len() > 128 {
        return Err(StorageError::Invalid("reporter_cluster exceeds 128 chars"));
    }

    // Anti-brigading independence check via privacy-preserving SECURITY DEFINER helper:
    // only first eligible report from a given cluster counts toward quarantine across all students.
    let is_eligible: bool = sqlx::query_scalar("SELECT is_report_cluster_eligible($1, $2, $3)")
        .bind(course_id)
        .bind(version_id)
        .bind(&cluster)
        .fetch_one(&mut *transaction)
        .await?;

    let replacement_card_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT c.id FROM challenges c
         WHERE c.course_id = $1 AND c.node_id = $2
         ORDER BY c.id
         LIMIT 1",
    )
    .bind(course_id)
    .bind(node_id)
    .fetch_optional(&mut *transaction)
    .await?;

    let inserted = sqlx::query_as::<_, (Uuid, bool)>(
        "INSERT INTO node_reports (
             user_id, node_id, content_version_id, tenant_id, course_id,
             reason, details, reporter_cluster, is_eligible, local_replacement_card_id, refund_applied
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, TRUE)
         RETURNING id, refund_applied",
    )
    .bind(actor)
    .bind(node_id)
    .bind(version_id)
    .bind(tenant_id)
    .bind(course_id)
    .bind(reason)
    .bind(&input.details)
    .bind(&cluster)
    .bind(is_eligible)
    .bind(replacement_card_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(map_shield_db_err)?;

    let quarantined: bool =
        sqlx::query_scalar("SELECT quarantine_content_version_if_threshold($1, $2, 3)")
            .bind(course_id)
            .bind(version_id)
            .fetch_one(&mut *transaction)
            .await?;

    transaction.commit().await?;
    Ok(NodeReportOutcome {
        report_id: inserted.0,
        content_version_id: version_id,
        is_eligible,
        refund_applied: inserted.1,
        local_replacement_card_id: replacement_card_id,
        version_quarantined: quarantined,
    })
}

/// Worker-triggered S-13 quarantine when an evaluation harness / simulator fails on a published version.
pub async fn quarantine_content_version_on_harness_failure(
    worker_pool: &PgPool,
    course_id: Uuid,
    content_version_id: Uuid,
    reason: &str,
) -> Result<()> {
    let reason = reason.trim();
    if reason.is_empty() || reason.len() > 240 {
        return Err(StorageError::Invalid(
            "quarantine reason must be 1..=240 chars",
        ));
    }
    let mut transaction = worker_pool.begin().await?;
    let res = sqlx::query(
        "UPDATE content_versions
         SET is_quarantined = TRUE,
             quarantined_at = COALESCE(quarantined_at, now()),
             quarantine_reason = $3
         WHERE id = $1 AND course_id = $2",
    )
    .bind(content_version_id)
    .bind(course_id)
    .bind(reason)
    .execute(&mut *transaction)
    .await?;
    if res.rows_affected() == 0 {
        return Err(StorageError::NotFound);
    }

    sqlx::query(
        "UPDATE ingestion_chunks ic
         SET is_quarantined = TRUE
         FROM learning_unit_bundles lub
         WHERE lub.content_version_id = $1
           AND lub.course_id = $2
           AND ic.course_id = lub.course_id
           AND ic.text_sha256 = lub.source_sha256",
    )
    .bind(content_version_id)
    .bind(course_id)
    .execute(&mut *transaction)
    .await?;

    transaction.commit().await?;
    Ok(())
}

/// Convenience wrapper using current UTC server clock for HTTP routes.
pub async fn recompute_course_shields_now(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
) -> Result<CourseShieldDashboard> {
    recompute_course_shields(pool, actor, course_id, Utc::now()).await
}

/// Convenience wrapper using current UTC server clock for HTTP dashboard reads.
pub async fn get_course_shields_dashboard_now(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
) -> Result<CourseShieldDashboard> {
    get_course_shields_dashboard(pool, actor, course_id, Utc::now()).await
}

/// Lists distinct version IDs for active vs quarantined checks without mutating history.
pub fn distinct_nodes_in_edges(edges: &[FortressEdgeView]) -> BTreeSet<Uuid> {
    edges
        .iter()
        .flat_map(|e| [e.prerequisite_node_id, e.dependent_node_id])
        .collect()
}
