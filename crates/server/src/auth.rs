use std::{
    collections::HashMap,
    net::IpAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use axum::http::{HeaderMap, header};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, Semaphore};
use uuid::Uuid;

use crate::{AppState, error::ApiError};

pub const COOKIE_NAME: &str = "campuslingo_session";

pub fn normalize_email(raw: &str) -> Result<String, ApiError> {
    let email = raw.trim().to_ascii_lowercase();
    if !(3..=254).contains(&email.len()) {
        return Err(ApiError::Invalid("email address is invalid"));
    }
    let Some((local, domain)) = email.split_once('@') else {
        return Err(ApiError::Invalid("email address is invalid"));
    };
    if local.is_empty()
        || domain.is_empty()
        || domain.contains('@')
        || !domain.contains('.')
        || email
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
        || email.contains("..")
    {
        return Err(ApiError::Invalid("email address is invalid"));
    }
    Ok(email)
}

pub fn validate_password(password: &str) -> Result<(), ApiError> {
    if !(12..=128).contains(&password.chars().count()) || password.trim().is_empty() {
        return Err(ApiError::Invalid(
            "password must contain 12 to 128 characters",
        ));
    }
    Ok(())
}

pub async fn hash_password(password: String, limiter: Arc<Semaphore>) -> Result<String, ApiError> {
    let permit = limiter
        .acquire_owned()
        .await
        .map_err(|_| ApiError::Internal)?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut salt = [0u8; 16];
        getrandom::fill(&mut salt).map_err(|_| ApiError::Internal)?;
        let salt = SaltString::encode_b64(&salt).map_err(|_| ApiError::Internal)?;
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|_| ApiError::Internal)
    })
    .await
    .map_err(|_| ApiError::Internal)?
}

pub async fn verify_password(
    password: String,
    encoded: String,
    limiter: Arc<Semaphore>,
) -> Result<bool, ApiError> {
    let permit = limiter
        .acquire_owned()
        .await
        .map_err(|_| ApiError::Internal)?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let parsed = PasswordHash::new(&encoded).map_err(|_| ApiError::Internal)?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok())
    })
    .await
    .map_err(|_| ApiError::Internal)?
}

pub fn random_token() -> (String, Vec<u8>) {
    let mut token = [0u8; 32];
    getrandom::fill(&mut token).expect("operating system randomness must be available");
    let encoded = hex::encode(token);
    (encoded.clone(), hash_token(&encoded))
}

pub fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

pub fn cookie_session(token: &str, secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!("{COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age=604800{secure}")
}

pub fn cookie_clear(secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!("{COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0{secure}")
}

pub fn session_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find_map(|(name, value)| (name == COOKIE_NAME).then(|| value.to_owned()))
}

pub async fn optional_actor(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<Option<Uuid>, ApiError> {
    let Some(token) = session_token(headers) else {
        return Ok(None);
    };
    if token.len() != 64 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Ok(None);
    }
    let hash = hash_token(&token);
    campuslingo_storage::auth::actor_for_token(&state.auth_pool, &hash)
        .await
        .map_err(ApiError::from)
}

pub async fn require_actor(state: &AppState, headers: &HeaderMap) -> Result<Uuid, ApiError> {
    optional_actor(state, headers)
        .await?
        .ok_or(ApiError::Unauthorized)
}

pub struct AuthLimiter {
    buckets: Mutex<HashMap<IpAddr, Bucket>>,
    limit: u32,
}

struct Bucket {
    window: Instant,
    count: u32,
}

impl AuthLimiter {
    pub fn new() -> Self {
        Self::with_limit(20)
    }

    pub fn with_limit(limit: u32) -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
            limit: limit.max(1),
        }
    }

    pub async fn allow(&self, address: IpAddr) -> bool {
        let mut buckets = self.buckets.lock().await;
        if buckets.len() > 4096 {
            buckets.retain(|_, bucket| bucket.window.elapsed() < Duration::from_secs(120));
        }
        let bucket = buckets.entry(address).or_insert(Bucket {
            window: Instant::now(),
            count: 0,
        });
        if bucket.window.elapsed() >= Duration::from_secs(60) {
            bucket.window = Instant::now();
            bucket.count = 0;
        }
        bucket.count += 1;
        bucket.count <= self.limit
    }
}

impl Default for AuthLimiter {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn dummy_hash(limiter: Arc<Semaphore>) -> Result<String, ApiError> {
    hash_password("campuslingo-dummy-password".into(), limiter).await
}
