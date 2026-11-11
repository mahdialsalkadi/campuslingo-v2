//! Persistence and security primitives for the CampusLingo server.
//!
//! This crate deliberately exposes separate connection pools for authentication,
//! application data, grading secrets, and background jobs. The deployment
//! harness grants each PostgreSQL role only the tables it needs.

pub mod assets;
pub mod auth;
pub mod courses;
pub mod crypto;
pub mod jobs;
pub mod review;

use sqlx::{PgConnection, PgPool, Postgres, Transaction};
use thiserror::Error;
use uuid::Uuid;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
    #[error("database migration failed")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("record not found")]
    NotFound,
    #[error("request conflicts with existing state")]
    Conflict,
    #[error("operation is not authorized")]
    Forbidden,
    #[error("authentication failed")]
    Unauthorized,
    #[error("invalid input: {0}")]
    Invalid(&'static str),
    #[error("recovery material could not be processed")]
    Crypto,
}

pub type Result<T> = std::result::Result<T, StorageError>;

pub fn is_unique_violation(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(database) if database.code().as_deref() == Some("23505"))
}

pub fn is_permission_denied(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(database) if database.code().as_deref() == Some("42501"))
}

/// Set the transaction-local actor used by row-level security. Never use a
/// caller-supplied identifier here; callers must validate a session first.
pub async fn set_actor(transaction: &mut PgConnection, actor: Uuid) -> Result<()> {
    sqlx::query("SELECT set_config('campuslingo.actor', $1, true)")
        .bind(actor.to_string())
        .execute(transaction)
        .await?;
    Ok(())
}

pub async fn begin_as<'a>(pool: &'a PgPool, actor: Uuid) -> Result<Transaction<'a, Postgres>> {
    let mut transaction = pool.begin().await?;
    set_actor(&mut transaction, actor).await?;
    Ok(transaction)
}

pub async fn migrate(pool: &PgPool) -> Result<()> {
    MIGRATOR.run(pool).await?;
    Ok(())
}
