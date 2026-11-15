pub mod assets;
pub mod auth;
pub mod config;
pub mod duels;
pub mod error;
pub mod m6;
pub mod m7;
pub mod recovery;
pub mod routes;

use std::path::Path as StdPath;
use std::sync::Arc;
use std::time::Instant;

use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Path, State},
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

fn resolve_godot_web_dir(state: &AppState) -> Option<std::path::PathBuf> {
    if std::env::var("CAMPUSLINGO_LEGACY_WEB")
        .is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
    {
        return None;
    }
    if let Ok(custom) = std::env::var("CAMPUSLINGO_GODOT_WEB_DIR") {
        let p = std::path::PathBuf::from(custom);
        if p.join("index.html").is_file() {
            return Some(p);
        }
    }
    let uses_default_web = state
        .settings
        .web_dir
        .ends_with(StdPath::new("packages/game-client/public"));
    if !uses_default_web {
        return None;
    }
    for candidate in [
        std::path::PathBuf::from("apps/godot/builds/web"),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/godot/builds/web"),
    ] {
        if candidate.join("index.html").is_file() {
            return Some(candidate);
        }
    }
    None
}

async fn serve_file_with_cache(path: &StdPath, cache_control: &'static str) -> Response {
    match tokio::fs::read(path).await {
        Ok(bytes) => {
            let ct = content_type_for_path(path);
            let mut resp = (
                StatusCode::OK,
                [(header::CONTENT_TYPE, ct), (header::CACHE_CONTROL, cache_control)],
                bytes,
            )
                .into_response();
            let _ = &mut resp;
            resp
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn serve_godot_or_legacy_index(State(state): State<AppState>) -> Response {
    if let Some(dir) = resolve_godot_web_dir(&state) {
        return serve_file_with_cache(&dir.join("index.html"), "no-store").await;
    }
    serve_index(State(state)).await
}

async fn serve_sw_js(State(state): State<AppState>) -> Response {
    if resolve_godot_web_dir(&state).is_some() {
        let cleanup_sw = "self.addEventListener('install',()=>self.skipWaiting());self.addEventListener('activate',e=>{e.waitUntil(caches.keys().then(ks=>Promise.all(ks.map(k=>caches.delete(k)))).then(()=>self.registration.unregister()).then(()=>self.clients.matchAll()).then(cs=>cs.forEach(c=>c.navigate(c.url))));});";
        return (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/javascript; charset=utf-8"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            cleanup_sw,
        )
            .into_response();
    }
    serve_file_from_dir(&state.settings.web_dir, "sw.js").await
}

async fn serve_root_asset(
    State(state): State<AppState>,
    axum::extract::Path(file): axum::extract::Path<String>,
) -> Response {
    if file.is_empty()
        || file.contains('/')
        || file.contains('\\')
        || file.contains("..")
        || file.starts_with('.')
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    if let Some(dir) = resolve_godot_web_dir(&state) {
        let candidate = dir.join(&file);
        if candidate.is_file() {
            let cc = if file.ends_with(".html") || file.ends_with(".js") {
                "no-store"
            } else {
                "public, max-age=60"
            };
            return serve_file_with_cache(&candidate, cc).await;
        }
    }
    serve_file_from_dir(&state.settings.web_dir, &file).await
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(serve_godot_or_legacy_index))
        .route("/index.html", get(serve_godot_or_legacy_index))
        .route("/legacy", get(serve_index))
        .route("/legacy/index.html", get(serve_index))
        .route("/styles.css", get(serve_web_static("styles.css")))
        .route("/m4-shell.css", get(serve_web_static("m4-shell.css")))
        .route("/app.mjs", get(serve_web_static("app.mjs")))
        .route("/controller.js", get(serve_web_static("controller.js")))
        .route("/shell.js", get(serve_web_static("shell.js")))
        .route("/manifest.json", get(serve_web_static("manifest.json")))
        .route("/sw.js", get(serve_sw_js))
        .route("/{file}", get(serve_root_asset))
        .route("/icons/{file}", get(serve_icon_file))
        .route("/wasm/{file}", get(serve_wasm_file))
        .route("/healthz", get(health))
        .route("/readyz", get(readiness))
        .nest("/v1", routes::router())
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(middleware::from_fn_with_state(state.clone(), same_origin))
        .layer(middleware::from_fn(security_headers))
        .layer(middleware::from_fn(log_request))
        .with_state(state)
}

fn is_safe_filename(name: &str) -> bool {
    !name.is_empty()
        && !name.contains("..")
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains('\0')
}

fn content_type_for_path(path: &StdPath) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") | Some("mjs") => "application/javascript; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("json") | Some("webmanifest") => "application/json; charset=utf-8",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("svg") => "image/svg+xml",
        Some("ttf") => "font/ttf",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

async fn serve_file_from_dir(dir: &StdPath, rel: &str) -> Response {
    let target = dir.join(rel);
    let Ok(bytes) = tokio::fs::read(&target).await else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };
    let mime = content_type_for_path(&target);
    let mut response = (StatusCode::OK, bytes).into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    response
}

async fn serve_index(State(state): State<AppState>) -> Response {
    serve_file_from_dir(&state.settings.web_dir, "index.html").await
}

fn serve_web_static(
    filename: &'static str,
) -> impl Fn(State<AppState>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>>
+ Clone {
    move |State(state): State<AppState>| {
        Box::pin(async move { serve_file_from_dir(&state.settings.web_dir, filename).await })
    }
}

async fn serve_icon_file(State(state): State<AppState>, Path(file): Path<String>) -> Response {
    if !is_safe_filename(&file) {
        return (StatusCode::FORBIDDEN, "forbidden").into_response();
    }
    serve_file_from_dir(&state.settings.web_dir.join("icons"), &file).await
}

async fn serve_wasm_file(State(state): State<AppState>, Path(file): Path<String>) -> Response {
    if !is_safe_filename(&file) {
        return (StatusCode::FORBIDDEN, "forbidden").into_response();
    }
    serve_file_from_dir(&state.settings.wasm_dir, &file).await
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
