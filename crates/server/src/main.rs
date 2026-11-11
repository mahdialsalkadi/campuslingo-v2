use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::json;
use sqlx::{PgPool, postgres::PgPoolOptions};
use tokio::{
    net::TcpListener,
    sync::Semaphore,
    time::{Instant, timeout},
};

use campuslingo_server::{AppState, build_router, config::Settings};

/// The M0 validation harness remains available for reproducibility, but only
/// when explicitly requested and only on loopback. It is not the M1 server.
#[derive(Clone)]
struct M0State {
    database: Option<PgPool>,
    sockets: Arc<Semaphore>,
    evaluations: Arc<Semaphore>,
}

async fn m0_health() -> Json<serde_json::Value> {
    Json(
        json!({"status":"alive", "stage":"M0", "algorithm":"FSRS-6", "schema_version": campuslingo_domain::SCHEMA_VERSION}),
    )
}

async fn m0_readiness(State(state): State<M0State>) -> Response {
    match &state.database {
        Some(pool) => match timeout(Duration::from_secs(1), sqlx::query("SELECT 1").execute(pool)).await {
            Ok(Ok(_)) => Json(json!({"status":"ready", "database":"reachable"})).into_response(),
            _ => (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"status":"not_ready", "database":"unreachable"}))).into_response(),
        },
        None => (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"status":"not_ready", "database":"not_configured", "note":"M0 is not a production backend"}))).into_response(),
    }
}

async fn websocket(ws: WebSocketUpgrade, State(state): State<M0State>) -> Response {
    let Ok(permit) = state.sockets.clone().try_acquire_owned() else {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            "M0 connection budget exhausted",
        )
            .into_response();
    };
    ws.max_message_size(32768)
        .max_frame_size(32768)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            validation_socket(socket, state.evaluations).await;
        })
}

async fn validation_socket(mut socket: WebSocket, evaluations: Arc<Semaphore>) {
    let mut window = Instant::now();
    let mut requests = 0;
    while let Ok(Some(Ok(message))) = timeout(Duration::from_secs(15), socket.recv()).await {
        if window.elapsed() >= Duration::from_secs(1) {
            window = Instant::now();
            requests = 0;
        }
        requests += 1;
        if requests > 10 {
            break;
        }
        match message {
            Message::Text(text) => {
                let Ok(permit) = evaluations.clone().acquire_owned().await else {
                    break;
                };
                let output = tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    campuslingo_domain::evaluate_json(&text)
                })
                .await;
                let Ok(output) = output else {
                    break;
                };
                if socket.send(Message::Text(output.into())).await.is_err() {
                    break;
                }
            }
            Message::Ping(data) => {
                if socket.send(Message::Pong(data)).await.is_err() {
                    break;
                }
            }
            Message::Close(_) => break,
            Message::Binary(_) => break,
            Message::Pong(_) => {}
        }
    }
    let _ = socket.send(Message::Close(None)).await;
}

async fn run_m0() -> Result<(), Box<dyn std::error::Error>> {
    let address: SocketAddr = std::env::var("CAMPUSLINGO_BIND")
        .unwrap_or_else(|_| "127.0.0.1:18080".into())
        .parse()?;
    // Authentication is an M1 gate. Do not expose the M0 harness to a network.
    if !address.ip().is_loopback() {
        return Err("M0 only permits loopback binding until authentication is implemented".into());
    }
    let database = match std::env::var("CAMPUSLINGO_DATABASE_URL") {
        Ok(url) if !url.is_empty() => Some(
            PgPoolOptions::new()
                .max_connections(4)
                .min_connections(0)
                .acquire_timeout(Duration::from_secs(1))
                .connect_lazy(&url)?,
        ),
        _ => None,
    };
    let state = M0State {
        database,
        sockets: Arc::new(Semaphore::new(30)),
        evaluations: Arc::new(Semaphore::new(2)),
    };
    let mut app = Router::new()
        .route("/healthz", get(m0_health))
        .route("/readyz", get(m0_readiness));
    if std::env::var("CAMPUSLINGO_M0_WS").as_deref() == Ok("1") {
        app = app.route("/m0/ws", get(websocket));
    }
    let listener = TcpListener::bind(address).await?;
    println!(
        "{}",
        json!({"event":"listening", "address":listener.local_addr()?.to_string(), "stage":"M0-only"})
    );
    axum::serve(listener, app.with_state(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

async fn migrate_database() -> Result<(), Box<dyn std::error::Error>> {
    let url = Settings::admin_database_url()?;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .min_connections(0)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&url)
        .await?;
    campuslingo_storage::migrate(&pool).await?;
    println!("{}", json!({"event":"migrations_applied"}));
    Ok(())
}

async fn run_m1() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Settings::from_env()?;
    let bind = settings.bind;
    let state = AppState::connect(settings).await?;
    let app = build_router(state);
    let listener = TcpListener::bind(bind).await?;
    println!(
        "{}",
        json!({"event":"listening", "address":listener.local_addr()?.to_string(), "stage":"M1"})
    );
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let migrate = matches!(
        std::env::args().nth(1).as_deref(),
        Some("migrate") | Some("--migrate")
    );
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(4)
        .enable_all()
        .build()?;
    if migrate {
        return runtime.block_on(migrate_database());
    }
    let m0_only = std::env::var("CAMPUSLINGO_M0_WS").as_deref() == Ok("1")
        && std::env::var("CAMPUSLINGO_AUTH_DATABASE_URL").is_err();
    if m0_only {
        runtime.block_on(run_m0())
    } else {
        runtime.block_on(run_m1())
    }
}
