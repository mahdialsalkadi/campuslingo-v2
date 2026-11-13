pub mod assets;
pub mod auth;
pub mod config;
pub mod duels;
pub mod error;
pub mod m6;
pub mod m7;
pub mod recovery;
pub mod routes;

use std::sync::Arc;
use std::time::Instant;

use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, State},
    http::{HeaderValue, Request, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::json;
use sqlx::{PgPool, postgres::PgPoolOptions};
use tokio::{
    sync::Semaphore,
    time::{Duration, timeout},
};

use auth::{ActorRateLimiter, AuthLimiter};
use config::Settings;

#[derive(Clone)]
pub struct AppState {
    pub auth_pool: PgPool,
    pub app_pool: PgPool,
    pub grader_pool: PgPool,
    pub recovery_pool: Option<PgPool>,
    pub settings: Arc<Settings>,
    pub limiter: Arc<AuthLimiter>,
    pub actor_limiter: Arc<ActorRateLimiter>,
    pub hashing: Arc<Semaphore>,
    pub dummy_hash: Arc<str>,
    pub duels: Arc<duels::DuelHub>,
    pub bosses: Arc<m6::BossLobbyHub>,
}

impl AppState {
    pub async fn connect(settings: Settings) -> Result<Self, Box<dyn std::error::Error>> {
        let build = |url: &str, maximum: u32| {
            PgPoolOptions::new()
                .max_connections(maximum)
                .min_connections(0)
                .acquire_timeout(Duration::from_secs(5))
                .connect_lazy(url)
        };
        let auth_pool = build(&Settings::auth_database_url()?, 8)?;
        let app_pool = build(&Settings::app_database_url()?, 16)?;
        let grader_pool = build(&Settings::grader_database_url()?, 8)?;
        let recovery_pool = std::env::var("CAMPUSLINGO_RECOVERY_DATABASE_URL")
            .ok()
            .filter(|v| !v.is_empty())
            .map(|url| build(&url, 2))
            .transpose()?;
        let hashing = Arc::new(Semaphore::new(1));
        let dummy_hash = auth::dummy_hash(hashing.clone()).await?.into();
        tokio::fs::create_dir_all(&settings.asset_dir).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&settings.asset_dir, std::fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self {
            auth_pool,
            app_pool,
            grader_pool,
            recovery_pool,
            settings: Arc::new(settings),
            limiter: Arc::new(AuthLimiter::new()),
            actor_limiter: Arc::new(ActorRateLimiter::from_env()),
            hashing,
            dummy_hash,
            duels: Arc::new(duels::DuelHub::new()),
            bosses: Arc::new(m6::BossLobbyHub::new()),
        })
    }
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(health))
        .route("/readyz", get(readiness))
        .nest("/v1", routes::router())
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(middleware::from_fn_with_state(state.clone(), same_origin))
        .layer(middleware::from_fn(security_headers))
        .layer(middleware::from_fn(log_request))
        .with_state(state)
}

async fn security_headers(request: Request<Body>, next: Next) -> Response {
    let is_api = request.uri().path().starts_with("/v1");
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("geolocation=(), microphone=(), camera=()"),
    );
    if is_api && !headers.contains_key(header::CACHE_CONTROL) {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    response
}

/// Structured request logs intentionally exclude cookies, bodies, emails, and
/// query strings so authentication and content material cannot leak to logs.
async fn log_request(request: Request<Body>, next: Next) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let started = Instant::now();
    let response = next.run(request).await;
    println!(
        "{}",
        json!({
            "event": "request",
            "method": method.as_str(),
            "path": path,
            "status": response.status().as_u16(),
            "duration_ms": started.elapsed().as_millis(),
        })
    );
    response
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({
        "status": "alive",
        "stage": "M1",
        "milestone": "M7",
        "algorithm": "FSRS-6",
        "schema_version": campuslingo_domain::SCHEMA_VERSION
    }))
}

async fn readiness(State(state): State<AppState>) -> Response {
    let probes = [
        ("auth", sqlx::query("SELECT 1").execute(&state.auth_pool)),
        ("app", sqlx::query("SELECT 1").execute(&state.app_pool)),
        (
            "grader",
            sqlx::query("SELECT 1").execute(&state.grader_pool),
        ),
    ];
    let mut failed = Vec::new();
    for (name, probe) in probes {
        if !matches!(timeout(Duration::from_secs(1), probe).await, Ok(Ok(_))) {
            failed.push(name);
        }
    }
    if failed.is_empty() {
        Json(json!({"status": "ready", "database": "reachable"})).into_response()
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status": "not_ready", "unavailable": failed})),
        )
            .into_response()
    }
}

/// Cookie-authenticated writes must be same-origin. If no allow-list is set,
/// compare Origin with the request Host; never trust proxy headers implicitly.
async fn same_origin(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    if matches!(
        request.method().as_str(),
        "POST" | "PUT" | "PATCH" | "DELETE"
    ) && let Some(origin) = request.headers().get(header::ORIGIN)
    {
        let allowed = match (origin.to_str(), request.headers().get(header::HOST)) {
            (Ok(origin), Some(host)) => match &state.settings.allowed_origin {
                Some(configured) => origin == configured,
                None => origin
                    .parse::<axum::http::Uri>()
                    .ok()
                    .and_then(|uri| uri.authority().map(|authority| authority.as_str() == host))
                    .unwrap_or(false),
            },
            _ => false,
        };
        if !allowed {
            return (
                StatusCode::FORBIDDEN,
                Json(
                    json!({"error": "origin_rejected", "message": "request origin is not allowed"}),
                ),
            )
                .into_response();
        }
    }
    next.run(request).await
}
