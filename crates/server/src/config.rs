use std::{net::SocketAddr, path::PathBuf};

use campuslingo_storage::crypto::RecoveryKey;

#[derive(Clone)]
pub struct Settings {
    pub bind: SocketAddr,
    pub cookie_secure: bool,
    pub allowed_origin: Option<String>,
    pub recovery_key: Option<RecoveryKey>,
    pub asset_dir: PathBuf,
}

fn env_bool(name: &str) -> bool {
    std::env::var(name)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes"))
        .unwrap_or(false)
}

impl Settings {
    pub fn from_env() -> Result<Self, String> {
        let bind = std::env::var("CAMPUSLINGO_BIND")
            .unwrap_or_else(|_| "127.0.0.1:18080".into())
            .parse()
            .map_err(|_| "CAMPUSLINGO_BIND must be a socket address")?;
        let recovery_key = match std::env::var("CAMPUSLINGO_RECOVERY_KEY") {
            Ok(value) if !value.trim().is_empty() => Some(
                RecoveryKey::from_hex(value.trim())
                    .map_err(|_| "CAMPUSLINGO_RECOVERY_KEY must be 64 hex characters")?,
            ),
            _ => None,
        };
        Ok(Self {
            bind,
            cookie_secure: env_bool("CAMPUSLINGO_COOKIE_SECURE"),
            allowed_origin: std::env::var("CAMPUSLINGO_ALLOWED_ORIGIN")
                .ok()
                .filter(|value| !value.is_empty()),
            recovery_key,
            asset_dir: std::env::var("CAMPUSLINGO_ASSET_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("artifacts/private/assets")),
        })
    }

    pub fn auth_database_url() -> Result<String, String> {
        std::env::var("CAMPUSLINGO_AUTH_DATABASE_URL")
            .map_err(|_| "CAMPUSLINGO_AUTH_DATABASE_URL is required".into())
    }

    pub fn app_database_url() -> Result<String, String> {
        std::env::var("CAMPUSLINGO_APP_DATABASE_URL")
            .map_err(|_| "CAMPUSLINGO_APP_DATABASE_URL is required".into())
    }

    pub fn grader_database_url() -> Result<String, String> {
        std::env::var("CAMPUSLINGO_GRADER_DATABASE_URL")
            .map_err(|_| "CAMPUSLINGO_GRADER_DATABASE_URL is required".into())
    }

    pub fn admin_database_url() -> Result<String, String> {
        std::env::var("CAMPUSLINGO_ADMIN_DATABASE_URL")
            .map_err(|_| "CAMPUSLINGO_ADMIN_DATABASE_URL is required".into())
    }
}
