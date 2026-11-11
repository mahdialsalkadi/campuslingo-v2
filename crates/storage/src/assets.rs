//! Authorized private asset metadata.
//!
//! File bytes never live in PostgreSQL. A UUID filename under the configured
//! private directory is written only after the application role proves course
//! management, and read only after the caller proves course read access.

use serde::Serialize;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::{Result, StorageError, begin_as, is_permission_denied};

pub const MAX_ASSET_BYTES: i64 = 80 * 1024 * 1024;

#[derive(Debug, Serialize, FromRow)]
pub struct AssetRecord {
    pub id: Uuid,
    pub media_type: String,
    pub bytes: i64,
    pub sha256: String,
}

#[derive(Debug, Serialize, FromRow)]
pub struct CreatedAsset {
    pub id: Uuid,
    pub bytes: i64,
}

pub async fn create_asset(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    id: Uuid,
    sha256: &str,
    media_type: &str,
    bytes: i64,
) -> Result<CreatedAsset> {
    if bytes <= 0 || bytes > MAX_ASSET_BYTES {
        return Err(StorageError::Invalid("asset size is out of range"));
    }
    if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(StorageError::Invalid("asset digest is invalid"));
    }
    let mut transaction = begin_as(pool, actor).await?;
    let inserted = sqlx::query(
        "INSERT INTO assets (id, tenant_id, course_id, created_by, sha256, media_type, bytes)
         SELECT $3, c.tenant_id, c.id, $2, $4, $5, $6
         FROM courses c WHERE c.id = $1 AND can_manage_course(c.id)",
    )
    .bind(course_id)
    .bind(actor)
    .bind(id)
    .bind(sha256)
    .bind(media_type)
    .bind(bytes)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        if is_permission_denied(&error) {
            StorageError::Forbidden
        } else {
            StorageError::Database(error)
        }
    })?
    .rows_affected();
    transaction.commit().await?;
    if inserted != 1 {
        return Err(StorageError::NotFound);
    }
    Ok(CreatedAsset { id, bytes })
}

/// Returns metadata only when the authenticated actor may read the course.
/// The explicit predicate keeps a denial indistinguishable from a missing id.
pub async fn asset_for_read(pool: &PgPool, actor: Uuid, asset_id: Uuid) -> Result<AssetRecord> {
    let mut transaction = begin_as(pool, actor).await?;
    let record = sqlx::query_as::<_, AssetRecord>(
        "SELECT a.id, a.media_type, a.bytes, a.sha256 FROM assets a
         WHERE a.id = $1 AND can_read_course(a.course_id)",
    )
    .bind(asset_id)
    .fetch_optional(&mut *transaction)
    .await?;
    transaction.commit().await?;
    record.ok_or(StorageError::NotFound)
}
