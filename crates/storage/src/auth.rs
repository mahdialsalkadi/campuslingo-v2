use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::{Result, StorageError, is_unique_violation};

#[derive(Debug, FromRow)]
pub struct Credentials {
    pub id: Uuid,
    pub password_hash: String,
}

pub async fn create_user(pool: &PgPool, email: &str, password_hash: &str) -> Result<Uuid> {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (email, password_hash) VALUES ($1, $2) RETURNING id",
    )
    .bind(email)
    .bind(password_hash)
    .fetch_one(pool)
    .await
    .map_err(|error| {
        if is_unique_violation(&error) {
            StorageError::Conflict
        } else {
            StorageError::Database(error)
        }
    })
}

pub async fn credentials_by_email(pool: &PgPool, email: &str) -> Result<Option<Credentials>> {
    sqlx::query_as::<_, Credentials>("SELECT id, password_hash FROM users WHERE email = $1")
        .bind(email)
        .fetch_optional(pool)
        .await
        .map_err(StorageError::Database)
}

pub async fn user_id_by_email(pool: &PgPool, email: &str) -> Result<Option<Uuid>> {
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE email = $1")
        .bind(email)
        .fetch_optional(pool)
        .await
        .map_err(StorageError::Database)
}

pub async fn create_session(pool: &PgPool, user_id: Uuid, token_hash: &[u8]) -> Result<()> {
    if token_hash.len() != 32 {
        return Err(StorageError::Invalid("session token hash must be 32 bytes"));
    }
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, expires_at)
         VALUES ($1, $2, clock_timestamp() + interval '7 days')",
    )
    .bind(token_hash)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn actor_for_token(pool: &PgPool, token_hash: &[u8]) -> Result<Option<Uuid>> {
    if token_hash.len() != 32 {
        return Ok(None);
    }
    sqlx::query_scalar::<_, Uuid>(
        "SELECT s.user_id FROM sessions s
         JOIN users u ON u.id = s.user_id
         WHERE s.token_hash = $1 AND s.expires_at > clock_timestamp()",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
    .map_err(StorageError::Database)
}

pub async fn revoke_session(pool: &PgPool, token_hash: &[u8]) -> Result<()> {
    sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
        .bind(token_hash)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn record_recovery(
    pool: &PgPool,
    user_id: Uuid,
    token_hash: &[u8],
    ciphertext: &[u8],
) -> Result<()> {
    if token_hash.len() != 32 {
        return Err(StorageError::Invalid(
            "recovery token hash must be 32 bytes",
        ));
    }
    let mut transaction = pool.begin().await?;
    sqlx::query(
        "UPDATE recovery_tokens SET consumed_at = clock_timestamp()
         WHERE user_id = $1 AND consumed_at IS NULL",
    )
    .bind(user_id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO recovery_tokens (token_hash, user_id)
         VALUES ($1, $2)",
    )
    .bind(token_hash)
    .bind(user_id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query("INSERT INTO recovery_outbox (ciphertext) VALUES ($1)")
        .bind(ciphertext)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(())
}

/// Atomically consume a recovery token, replace the password, and revoke every
/// existing session. Returns the affected user id.
pub async fn consume_recovery(
    pool: &PgPool,
    token_hash: &[u8],
    new_password_hash: &str,
) -> Result<Uuid> {
    if token_hash.len() != 32 {
        return Err(StorageError::Unauthorized);
    }
    let mut transaction = pool.begin().await?;
    let user_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM recovery_tokens
         WHERE token_hash = $1 AND consumed_at IS NULL AND expires_at > clock_timestamp()
         FOR UPDATE",
    )
    .bind(token_hash)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or(StorageError::Unauthorized)?;
    sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
        .bind(new_password_hash)
        .bind(user_id)
        .execute(&mut *transaction)
        .await?;
    sqlx::query(
        "UPDATE recovery_tokens SET consumed_at = clock_timestamp()
         WHERE token_hash = $1",
    )
    .bind(token_hash)
    .execute(&mut *transaction)
    .await?;
    sqlx::query("DELETE FROM sessions WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(user_id)
}

#[derive(Debug, sqlx::FromRow)]
pub struct RecoveryDelivery {
    pub id: Uuid,
    pub ciphertext: Vec<u8>,
    pub delivery_attempts: i32,
}

/// Claim one undelivered message. The lease bounds duplicate delivery after a
/// crash; `SKIP LOCKED` lets multiple delivery processes run without contention.
pub async fn claim_recovery(
    pool: &PgPool,
    maximum_attempts: i32,
    lease_seconds: i32,
) -> Result<Option<RecoveryDelivery>> {
    if maximum_attempts < 1 || lease_seconds < 1 {
        return Err(StorageError::Invalid(
            "recovery claim bounds must be positive",
        ));
    }
    sqlx::query_as::<_, RecoveryDelivery>(
        "WITH candidate AS (
             SELECT id FROM recovery_outbox
             WHERE delivered_at IS NULL
               AND delivery_attempts < $1
               AND (lease_until IS NULL OR lease_until < clock_timestamp())
             ORDER BY created_at
             FOR UPDATE SKIP LOCKED
             LIMIT 1
         )
         UPDATE recovery_outbox o
         SET delivery_attempts = o.delivery_attempts + 1,
             lease_until = clock_timestamp() + make_interval(secs => $2)
         FROM candidate c
         WHERE o.id = c.id
         RETURNING o.id, o.ciphertext, o.delivery_attempts",
    )
    .bind(maximum_attempts)
    .bind(lease_seconds)
    .fetch_optional(pool)
    .await
    .map_err(StorageError::Database)
}

pub async fn mark_recovery_delivered(pool: &PgPool, id: Uuid) -> Result<()> {
    let affected = sqlx::query(
        "UPDATE recovery_outbox
         SET delivered_at = clock_timestamp(), lease_until = NULL
         WHERE id = $1 AND delivered_at IS NULL",
    )
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();
    if affected == 0 {
        return Err(StorageError::Conflict);
    }
    Ok(())
}

/// Return a failed delivery to the queue with bounded exponential backoff.
pub async fn release_recovery(pool: &PgPool, id: Uuid, retry_after_seconds: i32) -> Result<()> {
    let delay = retry_after_seconds.clamp(1, 3600);
    sqlx::query(
        "UPDATE recovery_outbox
         SET lease_until = clock_timestamp() + make_interval(secs => $2)
         WHERE id = $1 AND delivered_at IS NULL",
    )
    .bind(id)
    .bind(delay)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn sweep_expired(pool: &PgPool) -> Result<()> {
    sqlx::query("DELETE FROM sessions WHERE expires_at <= clock_timestamp()")
        .execute(pool)
        .await?;
    sqlx::query(
        "DELETE FROM recovery_tokens WHERE expires_at <= clock_timestamp() - interval '1 day'",
    )
    .execute(pool)
    .await?;
    Ok(())
}
