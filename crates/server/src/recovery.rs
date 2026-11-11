//! Out-of-process recovery delivery.
//!
//! The web process only encrypts recovery material into the outbox. This module
//! runs in a separate binary with its own least-privilege database role and a
//! trusted delivery endpoint. It never logs the recipient or token, and it
//! validates the endpoint so a misconfiguration cannot send recovery secrets in
//! cleartext to a remote host.

use std::time::Duration;

use reqwest::{Client, Url};
use serde::Serialize;
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

use campuslingo_storage::crypto::RecoveryKey;
use campuslingo_storage::{StorageError, auth};

const CLAIM_LEASE_SECONDS: i32 = 120;
const MAXIMUM_ATTEMPTS: i32 = 8;

#[derive(Debug, Error)]
pub enum RecoveryError {
    #[error("recovery database operation failed")]
    Storage(#[from] StorageError),
    #[error("recovery envelope could not be decrypted")]
    Crypto,
    #[error("recovery delivery endpoint must use HTTPS unless it is loopback HTTP")]
    UnsafeEndpoint,
    #[error("recovery delivery request failed")]
    Transport,
    #[error("recovery delivery endpoint returned HTTP {0}")]
    Rejected(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryOutcome {
    Idle,
    Delivered { outbox_id: Uuid },
    Released { outbox_id: Uuid, attempts: i32 },
}

#[derive(Serialize)]
struct DeliveryPayload<'a> {
    user_id: Uuid,
    email: &'a str,
    token: &'a str,
    expires_in_seconds: u32,
}

/// Only HTTPS is accepted for non-loopback endpoints. Redirects are disabled by
/// the caller's client so the validated URL cannot be silently changed.
pub fn validate_delivery_endpoint(raw: &str) -> Result<Url, RecoveryError> {
    let url = Url::parse(raw).map_err(|_| RecoveryError::UnsafeEndpoint)?;
    if url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(RecoveryError::UnsafeEndpoint);
    }
    let loopback = matches!(
        url.host_str(),
        Some("127.0.0.1" | "::1" | "[::1]" | "localhost")
    );
    match url.scheme() {
        "https" => Ok(url),
        "http" if loopback => Ok(url),
        _ => Err(RecoveryError::UnsafeEndpoint),
    }
}

fn retry_delay_seconds(attempts: i32) -> i32 {
    let exponent = (attempts.clamp(1, 7) - 1) as u32;
    (5_i32.saturating_mul(1_i32 << exponent)).min(300)
}

/// Claim and deliver at most one outbox row. Success marks it delivered;
/// failures release it with bounded exponential backoff. Delivery is
/// at-least-once: a crash after the remote accepts the message can cause a
/// duplicate, but the token remains single-use at confirmation.
pub async fn deliver_once(
    pool: &PgPool,
    key: &RecoveryKey,
    endpoint: &Url,
    client: &Client,
) -> Result<DeliveryOutcome, RecoveryError> {
    let Some(claim) = auth::claim_recovery(pool, MAXIMUM_ATTEMPTS, CLAIM_LEASE_SECONDS).await?
    else {
        return Ok(DeliveryOutcome::Idle);
    };
    let (user_id, email, token) = key
        .decrypt(&claim.ciphertext)
        .map_err(|_| RecoveryError::Crypto)?;

    let request = client.post(endpoint.clone()).json(&DeliveryPayload {
        user_id,
        email: &email,
        token: &token,
        expires_in_seconds: 15 * 60,
    });
    let response = match request.send().await {
        Ok(response) => response,
        Err(_) => {
            let delay = retry_delay_seconds(claim.delivery_attempts);
            let _ = auth::release_recovery(pool, claim.id, delay).await;
            return Err(RecoveryError::Transport);
        }
    };
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let delay = retry_delay_seconds(claim.delivery_attempts);
        auth::release_recovery(pool, claim.id, delay).await?;
        return Err(RecoveryError::Rejected(status));
    }
    auth::mark_recovery_delivered(pool, claim.id).await?;
    Ok(DeliveryOutcome::Delivered {
        outbox_id: claim.id,
    })
}

/// Configuration used by the separate binary. Kept here so endpoint and key
/// handling stay testable without launching the process.
pub struct DeliveryConfig {
    pub database_url: String,
    pub key: RecoveryKey,
    pub endpoint: Url,
}

impl DeliveryConfig {
    pub fn from_env() -> Result<Self, RecoveryError> {
        let database_url = std::env::var("CAMPUSLINGO_RECOVERY_DATABASE_URL").map_err(|_| {
            RecoveryError::Storage(StorageError::Invalid("database URL is required"))
        })?;
        let key = std::env::var("CAMPUSLINGO_RECOVERY_KEY")
            .map_err(|_| RecoveryError::Crypto)
            .and_then(|value| {
                RecoveryKey::from_hex(value.trim()).map_err(|_| RecoveryError::Crypto)
            })?;
        let endpoint = validate_delivery_endpoint(
            &std::env::var("CAMPUSLINGO_RECOVERY_DELIVERY_URL")
                .map_err(|_| RecoveryError::UnsafeEndpoint)?,
        )?;
        Ok(Self {
            database_url,
            key,
            endpoint,
        })
    }
}

pub fn http_client() -> Result<Client, RecoveryError> {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| RecoveryError::Transport)
}
