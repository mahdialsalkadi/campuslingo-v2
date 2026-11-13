//! Milestone M7 PostgreSQL 16 Storage: Notification Devices, Preferences, Outbox (`S-18`, `F-05`),
//! Account Pseudonymization & Versioned Retention Sweep (`S-07`, `O-07`).

pub use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::{Result, StorageError, begin_as, is_permission_denied};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct NotificationPreferencesRecord {
    pub push_enabled: bool,
    pub raid_alerts_enabled: bool,
    pub shield_digest_enabled: bool,
    pub quiet_hours_start_minute: i16,
    pub quiet_hours_end_minute: i16,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct NotificationOutboxRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub course_id: Uuid,
    pub notification_kind: String,
    pub local_quota_date: chrono::NaiveDate,
    pub event_count: i32,
    pub summary_text: String,
    pub raid_ids: Vec<Uuid>,
    pub shield_node_ids: Vec<Uuid>,
    pub quiet_hours_deferred: bool,
    pub deliver_after: DateTime<Utc>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DeliveredNotificationRecord {
    pub id: Uuid,
    pub notification_kind: String,
    pub summary_text: String,
    pub raid_ids: Vec<Uuid>,
    pub shield_node_ids: Vec<Uuid>,
    pub delivered_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AccountPseudonymizationResult {
    pub user_id: Uuid,
    pub pseudonymized_email: String,
    pub retained_review_count: i32,
    pub retained_reward_tx_count: i32,
    pub retained_match_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RetentionSweepResult {
    pub archived_versions_count: i32,
    pub archived_cards_count: i32,
    pub purged_sessions_count: i32,
    pub purged_replays_count: i32,
}

fn map_m7_db_err(err: sqlx::Error) -> StorageError {
    if is_permission_denied(&err) {
        return StorageError::Forbidden;
    }
    if let sqlx::Error::Database(ref db_err) = err {
        let code = db_err.code().unwrap_or_default();
        if code == "23514" || code == "22023" {
            return StorageError::Invalid("notification quota or constraint rejected");
        }
        if code == "23505" {
            return StorageError::Conflict;
        }
    }
    StorageError::Database(err)
}

pub async fn register_notification_device(
    pool: &PgPool,
    actor: Uuid,
    platform: &str,
    endpoint_or_token: &str,
    p256dh_key: Option<&str>,
    auth_secret: Option<&str>,
) -> Result<Uuid> {
    let mut tx = begin_as(pool, actor).await?;
    let device_id: Uuid =
        sqlx::query_scalar("SELECT register_notification_device_secure($1, $2, $3, $4)")
            .bind(platform)
            .bind(endpoint_or_token)
            .bind(p256dh_key)
            .bind(auth_secret)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_m7_db_err)?;
    tx.commit().await?;
    Ok(device_id)
}

pub async fn upsert_notification_preferences(
    pool: &PgPool,
    actor: Uuid,
    push_enabled: bool,
    raid_alerts_enabled: bool,
    shield_digest_enabled: bool,
) -> Result<NotificationPreferencesRecord> {
    let mut tx = begin_as(pool, actor).await?;
    let row: NotificationPreferencesRecord = sqlx::query_as(
        "SELECT push_enabled, raid_alerts_enabled, shield_digest_enabled,
                quiet_hours_start_minute, quiet_hours_end_minute
         FROM upsert_notification_preferences_secure($1, $2, $3)",
    )
    .bind(push_enabled)
    .bind(raid_alerts_enabled)
    .bind(shield_digest_enabled)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_m7_db_err)?;
    tx.commit().await?;
    Ok(row)
}

pub async fn enqueue_raid_alert_outbox(
    pool: &PgPool,
    actor: Uuid,
    defender_user_id: Uuid,
    course_id: Uuid,
    raid_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Uuid> {
    let mut tx = begin_as(pool, actor).await?;
    let id: Uuid = sqlx::query_scalar("SELECT enqueue_raid_alert_outbox_secure($1, $2, $3, $4)")
        .bind(defender_user_id)
        .bind(course_id)
        .bind(raid_id)
        .bind(now)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_m7_db_err)?;
    tx.commit().await?;
    Ok(id)
}

pub async fn enqueue_shield_digest_outbox(
    pool: &PgPool,
    actor: Uuid,
    target_user_id: Uuid,
    course_id: Uuid,
    node_ids: &[Uuid],
    now: DateTime<Utc>,
) -> Result<Uuid> {
    let mut tx = begin_as(pool, actor).await?;
    let id: Uuid = sqlx::query_scalar("SELECT enqueue_shield_digest_outbox_secure($1, $2, $3, $4)")
        .bind(target_user_id)
        .bind(course_id)
        .bind(node_ids)
        .bind(now)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_m7_db_err)?;
    tx.commit().await?;
    Ok(id)
}

pub async fn dispatch_due_notifications(
    pool: &PgPool,
    actor: Uuid,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Vec<DeliveredNotificationRecord>> {
    let mut tx = begin_as(pool, actor).await?;
    let rows: Vec<DeliveredNotificationRecord> = sqlx::query_as(
        "SELECT id, notification_kind, summary_text, raid_ids, shield_node_ids, delivered_at
         FROM dispatch_due_notifications_secure($1, $2)
         ORDER BY delivered_at ASC, notification_kind ASC",
    )
    .bind(user_id)
    .bind(now)
    .fetch_all(&mut *tx)
    .await
    .map_err(map_m7_db_err)?;
    tx.commit().await?;
    Ok(rows)
}

pub async fn list_notification_outbox(
    pool: &PgPool,
    actor: Uuid,
    course_id: Option<Uuid>,
) -> Result<Vec<NotificationOutboxRecord>> {
    let mut tx = begin_as(pool, actor).await?;
    let rows: Vec<NotificationOutboxRecord> = sqlx::query_as(
        "SELECT id, user_id, course_id, notification_kind, local_quota_date,
                event_count, summary_text, raid_ids, shield_node_ids,
                quiet_hours_deferred, deliver_after, delivered_at, status
         FROM notification_outbox
         WHERE user_id = $1 AND ($2::uuid IS NULL OR course_id = $2)
         ORDER BY local_quota_date DESC, created_at DESC",
    )
    .bind(actor)
    .bind(course_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(map_m7_db_err)?;
    tx.commit().await?;
    Ok(rows)
}

pub async fn pseudonymize_user_account(
    pool: &PgPool,
    actor: Uuid,
    target_user_id: Uuid,
    reason: &str,
    now: DateTime<Utc>,
) -> Result<AccountPseudonymizationResult> {
    let mut tx = begin_as(pool, actor).await?;
    let res: AccountPseudonymizationResult = sqlx::query_as(
        "SELECT user_id, pseudonymized_email, retained_review_count,
                retained_reward_tx_count, retained_match_count
         FROM pseudonymize_user_account_secure($1, $2, $3)",
    )
    .bind(target_user_id)
    .bind(reason)
    .bind(now)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_m7_db_err)?;
    tx.commit().await?;
    Ok(res)
}

pub async fn apply_versioned_retention_sweep(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    now: DateTime<Utc>,
) -> Result<RetentionSweepResult> {
    let mut tx = begin_as(pool, actor).await?;
    let res: RetentionSweepResult = sqlx::query_as(
        "SELECT archived_versions_count, archived_cards_count,
                purged_sessions_count, purged_replays_count
         FROM apply_versioned_retention_sweep_secure($1, $2)",
    )
    .bind(course_id)
    .bind(now)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_m7_db_err)?;
    tx.commit().await?;
    Ok(res)
}
