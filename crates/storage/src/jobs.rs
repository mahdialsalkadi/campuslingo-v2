use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::{Result, StorageError, begin_as, is_permission_denied};

#[derive(Debug, Serialize, FromRow)]
pub struct JobView {
    pub id: Uuid,
    pub course_id: Uuid,
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub progress_done: i32,
    pub progress_total: i32,
    pub stage: String,
    pub error_code: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub output: Option<Value>,
}

#[derive(Debug, FromRow)]
pub struct ClaimedJob {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub course_id: Uuid,
    pub payload: Value,
    pub attempts: i32,
    pub max_attempts: i32,
    pub lease_token: Uuid,
}

pub async fn enqueue(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    idempotency_key: Uuid,
    raw_payload: &[u8],
    payload: &Value,
) -> Result<JobView> {
    if raw_payload.len() > 32_768 {
        return Err(StorageError::Invalid("job payload exceeds 32 KiB"));
    }
    let payload_hash = Sha256::digest(raw_payload).to_vec();
    let mut transaction = begin_as(pool, actor).await?;
    let inserted = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO jobs
            (tenant_id, course_id, created_by, idempotency_key, payload, payload_hash)
         SELECT c.tenant_id, c.id, $2, $3, $4, $5
         FROM courses c WHERE c.id = $1 AND can_manage_course(c.id)
         ON CONFLICT (course_id, created_by, idempotency_key) DO NOTHING
         RETURNING id",
    )
    .bind(course_id)
    .bind(actor)
    .bind(idempotency_key)
    .bind(payload)
    .bind(&payload_hash)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| {
        if is_permission_denied(&error) {
            StorageError::Forbidden
        } else {
            StorageError::Database(error)
        }
    })?;

    let job_id = match inserted {
        Some(id) => id,
        None => {
            let existing = sqlx::query_as::<_, (Uuid, Vec<u8>)>(
                "SELECT id, payload_hash FROM jobs
                 WHERE course_id = $1 AND created_by = $2 AND idempotency_key = $3",
            )
            .bind(course_id)
            .bind(actor)
            .bind(idempotency_key)
            .fetch_optional(&mut *transaction)
            .await?;
            match existing {
                Some((id, hash)) if hash == payload_hash => id,
                Some(_) => return Err(StorageError::Conflict),
                None => return Err(StorageError::Forbidden),
            }
        }
    };

    let job = get_job_in(&mut transaction, actor, course_id, job_id).await?;
    transaction.commit().await?;
    Ok(job)
}

async fn get_job_in(
    transaction: &mut sqlx::PgConnection,
    _actor: Uuid,
    course_id: Uuid,
    job_id: Uuid,
) -> Result<JobView> {
    sqlx::query_as::<_, JobView>(
        "SELECT j.id, j.course_id, j.status, j.attempts, j.max_attempts,
                j.progress_done, j.progress_total, j.stage, j.error_code,
                j.created_at, j.updated_at, r.output
         FROM jobs j LEFT JOIN job_results r ON r.job_id = j.id
         WHERE j.id = $1 AND j.course_id = $2 AND can_manage_course(j.course_id)",
    )
    .bind(job_id)
    .bind(course_id)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or(StorageError::NotFound)
}

pub async fn get_job(pool: &PgPool, actor: Uuid, course_id: Uuid, job_id: Uuid) -> Result<JobView> {
    let mut transaction = begin_as(pool, actor).await?;
    let job = get_job_in(&mut transaction, actor, course_id, job_id).await?;
    transaction.commit().await?;
    Ok(job)
}

/// Worker-role operation. There is intentionally no actor parameter: the worker
/// role has BYPASSRLS, and its credentials never reach the HTTP server.
pub async fn reaper_and_claim(pool: &PgPool) -> Result<Option<ClaimedJob>> {
    reaper_and_claim_kind(pool, None).await
}

pub async fn reaper_and_claim_kind(
    pool: &PgPool,
    kind: Option<&str>,
) -> Result<Option<ClaimedJob>> {
    sqlx::query(
        "UPDATE jobs
         SET status = 'failed', lease_token = NULL, lease_until = NULL,
             error_code = 'lease_expired', stage = 'failed', updated_at = clock_timestamp()
         WHERE status = 'running' AND lease_until < clock_timestamp()
           AND attempts >= max_attempts",
    )
    .execute(pool)
    .await?;

    let lease_token = Uuid::new_v4();
    let job = sqlx::query_as::<_, ClaimedJob>(
        "WITH candidate AS (
             SELECT id FROM jobs
             WHERE (status = 'queued'
                    OR (status = 'running' AND lease_until < clock_timestamp()))
                AND attempts < max_attempts
                AND ($2::text IS NULL OR payload->>'kind'=$2)
             ORDER BY created_at
             FOR UPDATE SKIP LOCKED
             LIMIT 1
         )
         UPDATE jobs j
         SET status = 'running', attempts = j.attempts + 1,
             lease_token = $1,
             lease_until = clock_timestamp() + interval '60 seconds',
             stage = 'claimed', error_code = NULL, updated_at = clock_timestamp()
         FROM candidate c
         WHERE j.id = c.id
         RETURNING j.id, j.tenant_id, j.course_id, j.payload,
                   j.attempts, j.max_attempts, j.lease_token",
    )
    .bind(lease_token)
    .bind(kind)
    .fetch_optional(pool)
    .await?;
    Ok(job)
}

pub async fn heartbeat(
    pool: &PgPool,
    job_id: Uuid,
    lease_token: Uuid,
    stage: &str,
    done: i32,
    total: i32,
) -> Result<()> {
    if stage.is_empty() || stage.len() > 64 || total < 1 || done < 0 || done > total {
        return Err(StorageError::Invalid("invalid progress"));
    }
    let mut transaction = pool.begin().await?;
    let updated = sqlx::query(
        "UPDATE jobs
         SET stage = $3, progress_done = $4, progress_total = $5,
             lease_until = clock_timestamp() + interval '60 seconds',
             updated_at = clock_timestamp()
          WHERE id = $1 AND lease_token = $2 AND status = 'running' AND lease_until > clock_timestamp()",
    )
    .bind(job_id)
    .bind(lease_token)
    .bind(stage)
    .bind(done)
    .bind(total)
    .execute(&mut *transaction)
    .await?
    .rows_affected();
    if updated != 1 {
        return Err(StorageError::Conflict);
    }
    sqlx::query(
        "INSERT INTO job_events (job_id, stage)
         SELECT $1, $2
         WHERE NOT EXISTS (
             SELECT 1 FROM job_events WHERE job_id = $1 AND stage = $2
         )",
    )
    .bind(job_id)
    .bind(stage)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn complete(
    pool: &PgPool,
    job_id: Uuid,
    lease_token: Uuid,
    output: &Value,
) -> Result<()> {
    let encoded_len = serde_json::to_vec(output)
        .map_err(|_| StorageError::Invalid("invalid job output"))?
        .len();
    if encoded_len > 262_144 {
        return Err(StorageError::Invalid("job output exceeds 256 KiB"));
    }
    let mut transaction = pool.begin().await?;
    let updated = sqlx::query(
        "UPDATE jobs
         SET status = 'succeeded', lease_token = NULL, lease_until = NULL,
             stage = 'succeeded', progress_done = progress_total,
             updated_at = clock_timestamp()
          WHERE id = $1 AND lease_token = $2 AND status = 'running' AND lease_until > clock_timestamp()",
    )
    .bind(job_id)
    .bind(lease_token)
    .execute(&mut *transaction)
    .await?
    .rows_affected();
    if updated != 1 {
        return Err(StorageError::Conflict);
    }
    sqlx::query(
        "INSERT INTO job_results (job_id, output) VALUES ($1, $2)
         ON CONFLICT (job_id) DO UPDATE SET output = EXCLUDED.output",
    )
    .bind(job_id)
    .bind(output)
    .execute(&mut *transaction)
    .await?;
    sqlx::query("INSERT INTO job_events (job_id, stage) VALUES ($1, 'succeeded')")
        .bind(job_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn fail(
    pool: &PgPool,
    job_id: Uuid,
    lease_token: Uuid,
    error_code: &str,
    retryable: bool,
) -> Result<()> {
    if error_code.is_empty() || error_code.len() > 64 {
        return Err(StorageError::Invalid("invalid error code"));
    }
    let mut transaction = pool.begin().await?;
    let row = sqlx::query_as::<_, (i32, i32)>(
        "SELECT attempts, max_attempts FROM jobs
          WHERE id = $1 AND lease_token = $2 AND status = 'running' AND lease_until > clock_timestamp()
         FOR UPDATE",
    )
    .bind(job_id)
    .bind(lease_token)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or(StorageError::Conflict)?;
    let will_retry = retryable && row.0 < row.1;
    sqlx::query(
        "UPDATE jobs
         SET status = $3, lease_token = NULL, lease_until = NULL,
             error_code = $4, stage = $5, updated_at = clock_timestamp()
         WHERE id = $1 AND lease_token = $2",
    )
    .bind(job_id)
    .bind(lease_token)
    .bind(if will_retry { "queued" } else { "failed" })
    .bind(error_code)
    .bind(if will_retry { "retry_queued" } else { "failed" })
    .execute(&mut *transaction)
    .await?;
    sqlx::query("INSERT INTO job_events (job_id, stage) VALUES ($1, $2)")
        .bind(job_id)
        .bind(if will_retry { "retry_queued" } else { "failed" })
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(())
}
