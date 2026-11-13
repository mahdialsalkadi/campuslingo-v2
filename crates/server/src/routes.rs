use std::net::SocketAddr;

use axum::{
    Json, Router,
    body::Bytes,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    AppState, assets,
    auth::{
        self, cookie_clear, cookie_session, hash_password, normalize_email, random_token,
        require_actor, validate_password,
    },
    error::ApiError,
};

type ApiResult<T> = Result<T, ApiError>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RegisterRequest {
    email: String,
    password: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryRequest {
    email: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryConfirm {
    token: String,
    new_password: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NameRequest {
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MemberRequest {
    email: String,
    role: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PublishRequest {
    rights_attested: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeRequest {
    title: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChallengeRequest {
    prompt: String,
    options: Vec<String>,
    correct_index: u8,
    explanation: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnswerRequest {
    selected_index: u8,
}

fn required_text(value: &str, maximum: usize, field: &'static str) -> ApiResult<String> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > maximum
        || value.chars().any(|character| character == '\0')
    {
        return Err(ApiError::Invalid(field));
    }
    Ok(value.to_owned())
}

fn session_response(status: StatusCode, user_id: Uuid, token: &str, secure: bool) -> Response {
    let mut response = (status, Json(json!({"user_id": user_id}))).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie_session(token, secure))
            .expect("hex cookie is a valid header"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn register(
    State(state): State<AppState>,
    ConnectInfo(address): ConnectInfo<SocketAddr>,
    Json(body): Json<RegisterRequest>,
) -> ApiResult<Response> {
    if !state.limiter.allow(address.ip()).await {
        return Err(ApiError::TooManyRequests);
    }
    let email = normalize_email(&body.email)?;
    validate_password(&body.password)?;
    let password_hash = hash_password(body.password, state.hashing.clone()).await?;
    let user_id = campuslingo_storage::auth::create_user(&state.auth_pool, &email, &password_hash)
        .await
        .map_err(ApiError::from)?;
    let (token, token_hash) = random_token();
    campuslingo_storage::auth::create_session(&state.auth_pool, user_id, &token_hash)
        .await
        .map_err(ApiError::from)?;
    Ok(session_response(
        StatusCode::CREATED,
        user_id,
        &token,
        state.settings.cookie_secure,
    ))
}

async fn login(
    State(state): State<AppState>,
    ConnectInfo(address): ConnectInfo<SocketAddr>,
    Json(body): Json<LoginRequest>,
) -> ApiResult<Response> {
    if !state.limiter.allow(address.ip()).await {
        return Err(ApiError::TooManyRequests);
    }
    let email = normalize_email(&body.email).map_err(|_| ApiError::Unauthorized)?;
    let password = body.password;
    let credentials = campuslingo_storage::auth::credentials_by_email(&state.auth_pool, &email)
        .await
        .map_err(ApiError::from)?;
    let (user_id, encoded) = match credentials {
        Some(credentials) => (credentials.id, credentials.password_hash),
        None => {
            let _ = auth::verify_password(
                password,
                state.dummy_hash.to_string(),
                state.hashing.clone(),
            )
            .await?;
            return Err(ApiError::Unauthorized);
        }
    };
    if !auth::verify_password(password, encoded, state.hashing.clone()).await? {
        return Err(ApiError::Unauthorized);
    }
    let (token, token_hash) = random_token();
    campuslingo_storage::auth::create_session(&state.auth_pool, user_id, &token_hash)
        .await
        .map_err(ApiError::from)?;
    Ok(session_response(
        StatusCode::OK,
        user_id,
        &token,
        state.settings.cookie_secure,
    ))
}

async fn me(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    let actor = require_actor(&state, &headers).await?;
    Ok(Json(json!({"user_id": actor})))
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Response> {
    if let Some(token) = auth::session_token(&headers)
        && token.len() == 64
    {
        let _ =
            campuslingo_storage::auth::revoke_session(&state.auth_pool, &auth::hash_token(&token))
                .await;
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie_clear(state.settings.cookie_secure))
            .expect("static cookie is valid"),
    );
    Ok(response)
}

async fn request_recovery(
    State(state): State<AppState>,
    ConnectInfo(address): ConnectInfo<SocketAddr>,
    Json(body): Json<RecoveryRequest>,
) -> ApiResult<Response> {
    if !state.limiter.allow(address.ip()).await {
        return Err(ApiError::TooManyRequests);
    }
    let Some(key) = state.settings.recovery_key.as_ref() else {
        return Err(ApiError::ServiceUnavailable(
            "account recovery delivery is not configured",
        ));
    };
    let Ok(email) = normalize_email(&body.email) else {
        return Ok((StatusCode::ACCEPTED, Json(json!({"status": "accepted"}))).into_response());
    };
    if let Some(user_id) = campuslingo_storage::auth::user_id_by_email(&state.auth_pool, &email)
        .await
        .map_err(ApiError::from)?
    {
        let (token, token_hash) = random_token();
        let ciphertext = key.encrypt(user_id, &email, &token)?;
        campuslingo_storage::auth::record_recovery(
            &state.auth_pool,
            user_id,
            &token_hash,
            &ciphertext,
        )
        .await
        .map_err(ApiError::from)?;
    }
    Ok((StatusCode::ACCEPTED, Json(json!({"status": "accepted"}))).into_response())
}

async fn confirm_recovery(
    State(state): State<AppState>,
    ConnectInfo(address): ConnectInfo<SocketAddr>,
    Json(body): Json<RecoveryConfirm>,
) -> ApiResult<StatusCode> {
    if !state.limiter.allow(address.ip()).await {
        return Err(ApiError::TooManyRequests);
    }
    if body.token.len() != 64 || !body.token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ApiError::Unauthorized);
    }
    validate_password(&body.new_password)?;
    let password_hash = hash_password(body.new_password, state.hashing.clone()).await?;
    campuslingo_storage::auth::consume_recovery(
        &state.auth_pool,
        &auth::hash_token(&body.token),
        &password_hash,
    )
    .await
    .map_err(|error| match error {
        campuslingo_storage::StorageError::Unauthorized => ApiError::Unauthorized,
        other => ApiError::from(other),
    })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn create_tenant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<NameRequest>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let actor = require_actor(&state, &headers).await?;
    let name = required_text(&body.name, 160, "tenant name is invalid")?;
    let tenant_id =
        campuslingo_storage::courses::create_tenant(&state.app_pool, actor, &name).await?;
    Ok((StatusCode::CREATED, Json(json!({"tenant_id": tenant_id}))))
}

async fn list_tenants(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<campuslingo_storage::courses::TenantSummary>>> {
    let actor = require_actor(&state, &headers).await?;
    Ok(Json(
        campuslingo_storage::courses::list_tenants(&state.app_pool, actor).await?,
    ))
}

async fn add_tenant_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<Uuid>,
    Json(body): Json<MemberRequest>,
) -> ApiResult<StatusCode> {
    let actor = require_actor(&state, &headers).await?;
    let email = normalize_email(&body.email)?;
    let user_id = campuslingo_storage::auth::user_id_by_email(&state.auth_pool, &email)
        .await?
        .ok_or(ApiError::NotFound)?;
    campuslingo_storage::courses::add_tenant_member(
        &state.app_pool,
        actor,
        tenant_id,
        user_id,
        &body.role,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn create_course(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<Uuid>,
    Json(body): Json<NameRequest>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let actor = require_actor(&state, &headers).await?;
    let name = required_text(&body.name, 160, "course name is invalid")?;
    let course_id =
        campuslingo_storage::courses::create_course(&state.app_pool, actor, tenant_id, &name)
            .await?;
    Ok((StatusCode::CREATED, Json(json!({"course_id": course_id}))))
}

async fn list_courses(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<campuslingo_storage::courses::CourseSummary>>> {
    let actor = require_actor(&state, &headers).await?;
    Ok(Json(
        campuslingo_storage::courses::list_courses(&state.app_pool, actor).await?,
    ))
}

async fn publish_course(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(course_id): Path<Uuid>,
    Json(body): Json<PublishRequest>,
) -> ApiResult<StatusCode> {
    let actor = require_actor(&state, &headers).await?;
    if !body.rights_attested {
        return Err(ApiError::Invalid(
            "publication requires an explicit rights attestation",
        ));
    }
    campuslingo_storage::courses::publish_course(&state.app_pool, actor, course_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn enroll(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(course_id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let actor = require_actor(&state, &headers).await?;
    campuslingo_storage::courses::enroll(&state.app_pool, actor, course_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn add_course_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(course_id): Path<Uuid>,
    Json(body): Json<MemberRequest>,
) -> ApiResult<StatusCode> {
    let actor = require_actor(&state, &headers).await?;
    if !matches!(body.role.as_str(), "instructor" | "student") {
        return Err(ApiError::Invalid(
            "course role must be instructor or student",
        ));
    }
    let email = normalize_email(&body.email)?;
    let user_id = campuslingo_storage::auth::user_id_by_email(&state.auth_pool, &email)
        .await?
        .ok_or(ApiError::NotFound)?;
    campuslingo_storage::courses::add_course_member(
        &state.app_pool,
        actor,
        course_id,
        user_id,
        &body.role,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn create_node(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(course_id): Path<Uuid>,
    Json(body): Json<NodeRequest>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let actor = require_actor(&state, &headers).await?;
    let title = required_text(&body.title, 160, "node title is invalid")?;
    let node_id =
        campuslingo_storage::courses::create_node(&state.app_pool, actor, course_id, &title)
            .await?;
    Ok((StatusCode::CREATED, Json(json!({"node_id": node_id}))))
}

async fn create_challenge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((course_id, node_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<ChallengeRequest>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let actor = require_actor(&state, &headers).await?;
    let prompt = required_text(&body.prompt, 1200, "challenge prompt is invalid")?;
    if !(2..=6).contains(&body.options.len()) {
        return Err(ApiError::Invalid("challenge requires 2 to 6 options"));
    }
    if body.correct_index as usize >= body.options.len() {
        return Err(ApiError::Invalid("correct index is out of range"));
    }
    let mut seen = std::collections::HashSet::new();
    for option in &body.options {
        let option = required_text(option, 300, "challenge option is invalid")?;
        if !seen.insert(option.clone()) {
            return Err(ApiError::Invalid("challenge options must be unique"));
        }
    }
    let explanation = body.explanation.as_deref().unwrap_or("").trim().to_owned();
    if explanation.chars().count() > 2000 {
        return Err(ApiError::Invalid("explanation is too long"));
    }
    let options = serde_json::to_value(&body.options)
        .map_err(|_| ApiError::Invalid("challenge options are invalid"))?;
    let challenge_id = campuslingo_storage::courses::create_challenge(
        &state.app_pool,
        actor,
        course_id,
        node_id,
        &campuslingo_storage::courses::NewChallenge {
            prompt: &prompt,
            options: &options,
            correct_index: body.correct_index,
            explanation: &explanation,
        },
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({"challenge_id": challenge_id})),
    ))
}

async fn list_content(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(course_id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let actor = require_actor(&state, &headers).await?;
    let (nodes, challenges) =
        campuslingo_storage::courses::list_content(&state.app_pool, actor, course_id).await?;
    Ok(Json(json!({"nodes": nodes, "challenges": challenges})))
}

async fn practice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((course_id, challenge_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<campuslingo_storage::courses::PracticeQuestion>> {
    let actor = require_actor(&state, &headers).await?;
    Ok(Json(
        campuslingo_storage::courses::practice_question(
            &state.app_pool,
            actor,
            course_id,
            challenge_id,
        )
        .await?,
    ))
}

async fn create_attempt(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((course_id, challenge_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let actor = require_actor(&state, &headers).await?;
    let attempt = campuslingo_storage::courses::create_attempt(
        &state.app_pool,
        actor,
        course_id,
        challenge_id,
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({"attempt_id": attempt.id, "expires_at": attempt.expires_at})),
    ))
}

async fn answer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(attempt_id): Path<Uuid>,
    Json(body): Json<AnswerRequest>,
) -> ApiResult<Json<campuslingo_storage::review::AnswerResult>> {
    let actor = require_actor(&state, &headers).await?;
    Ok(Json(
        campuslingo_storage::review::answer_attempt(
            &state.grader_pool,
            actor,
            attempt_id,
            body.selected_index,
        )
        .await?,
    ))
}

async fn enqueue_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(course_id): Path<Uuid>,
    body: Bytes,
) -> ApiResult<(StatusCode, Json<campuslingo_storage::jobs::JobView>)> {
    let actor = require_actor(&state, &headers).await?;
    if body.len() > 32_768 {
        return Err(ApiError::Invalid("job payload exceeds 32 KiB"));
    }
    let key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .ok_or(ApiError::Invalid("Idempotency-Key header is required"))?;
    let key =
        Uuid::parse_str(key).map_err(|_| ApiError::Invalid("Idempotency-Key must be a UUID"))?;
    let payload: Value = serde_json::from_slice(&body)
        .map_err(|_| ApiError::Invalid("job payload must be valid JSON"))?;
    let job =
        campuslingo_storage::jobs::enqueue(&state.app_pool, actor, course_id, key, &body, &payload)
            .await?;
    Ok((StatusCode::CREATED, Json(job)))
}

async fn get_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((course_id, job_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<campuslingo_storage::jobs::JobView>> {
    let actor = require_actor(&state, &headers).await?;
    Ok(Json(
        campuslingo_storage::jobs::get_job(&state.app_pool, actor, course_id, job_id).await?,
    ))
}

async fn ingestion_run(
    State(state): State<AppState>,
    Path((course, run)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> ApiResult<Json<campuslingo_storage::ingestion::RunView>> {
    let actor = require_actor(&state, &headers).await?;
    Ok(Json(
        campuslingo_storage::ingestion::run_for_manager(&state.app_pool, actor, course, run)
            .await?,
    ))
}

async fn ingestion_crop(
    State(state): State<AppState>,
    Path((course, crop)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let actor = require_actor(&state, &headers).await?;
    let asset =
        campuslingo_storage::ingestion::crop_asset_for_read(&state.app_pool, actor, course, crop)
            .await?;
    // The ordinary asset route reauthorizes immediately before streaming.
    assets::download_asset(State(state), Path(asset), headers).await
}

async fn get_course_shields(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(course_id): Path<Uuid>,
) -> ApiResult<Json<campuslingo_storage::shields::CourseShieldDashboard>> {
    let actor = require_actor(&state, &headers).await?;
    Ok(Json(
        campuslingo_storage::shields::get_course_shields_dashboard_now(
            &state.app_pool,
            actor,
            course_id,
        )
        .await?,
    ))
}

async fn recompute_course_shields(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(course_id): Path<Uuid>,
) -> ApiResult<Json<campuslingo_storage::shields::CourseShieldDashboard>> {
    let actor = require_actor(&state, &headers).await?;
    Ok(Json(
        campuslingo_storage::shields::recompute_course_shields_now(
            &state.app_pool,
            actor,
            course_id,
        )
        .await?,
    ))
}

async fn report_node_content(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((course_id, node_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<campuslingo_storage::shields::NewNodeReport>,
) -> ApiResult<(
    StatusCode,
    Json<campuslingo_storage::shields::NodeReportOutcome>,
)> {
    let actor = require_actor(&state, &headers).await?;
    let outcome = campuslingo_storage::shields::submit_node_report(
        &state.app_pool,
        actor,
        course_id,
        node_id,
        &body,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(outcome)))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/recover", post(request_recovery))
        .route("/auth/recover/confirm", post(confirm_recovery))
        .route("/me", get(me))
        .route("/tenants", post(create_tenant).get(list_tenants))
        .route("/tenants/{tenant_id}/members", post(add_tenant_member))
        .route("/tenants/{tenant_id}/courses", post(create_course))
        .route("/courses", get(list_courses))
        .route("/courses/{course_id}/publish", post(publish_course))
        .route("/courses/{course_id}/enroll", post(enroll))
        .route("/courses/{course_id}/members", post(add_course_member))
        .route("/courses/{course_id}/nodes", post(create_node))
        .route(
            "/courses/{course_id}/nodes/{node_id}/challenges",
            post(create_challenge),
        )
        .route("/courses/{course_id}/content", get(list_content))
        .route(
            "/courses/{course_id}/challenges/{challenge_id}/practice",
            get(practice),
        )
        .route(
            "/courses/{course_id}/challenges/{challenge_id}/attempts",
            post(create_attempt),
        )
        .route("/attempts/{attempt_id}/answer", post(answer))
        .route("/courses/{course_id}/jobs", post(enqueue_job))
        .route("/courses/{course_id}/jobs/{job_id}", get(get_job))
        .route("/courses/{course_id}/assets", post(assets::upload_asset))
        .route("/assets/{asset_id}", get(assets::download_asset))
        .route(
            "/courses/{course_id}/ingestion/runs/{run_id}",
            get(ingestion_run),
        )
        .route(
            "/courses/{course_id}/ingestion/crops/{crop_id}",
            get(ingestion_crop),
        )
        .route("/courses/{course_id}/shields", get(get_course_shields))
        .route(
            "/courses/{course_id}/shields/recompute",
            post(recompute_course_shields),
        )
        .route("/courses/{course_id}/fortress", get(get_course_shields))
        .route(
            "/courses/{course_id}/nodes/{node_id}/report",
            post(report_node_content),
        )
        .route("/duels/ws", get(crate::duels::duel_websocket))
        .route(
            "/courses/{course_id}/duels/matchmake",
            post(crate::duels::matchmake_duel),
        )
        .route(
            "/courses/{course_id}/duels/recover",
            post(crate::duels::recover_duels_endpoint),
        )
        .route(
            "/courses/{course_id}/duels/{match_id}",
            get(crate::duels::get_duel_status),
        )
        .route(
            "/courses/{course_id}/raids/opt-in",
            post(crate::m6::set_raid_opt_in_route),
        )
        .route(
            "/courses/{course_id}/raids/scanner",
            get(crate::m6::scan_raids_route),
        )
        .route(
            "/courses/{course_id}/raids/start",
            post(crate::m6::start_raid_route),
        )
        .route(
            "/courses/{course_id}/raids/{raid_id}/settle",
            post(crate::m6::settle_raid_route),
        )
        .route(
            "/courses/{course_id}/notifications",
            get(crate::m6::list_notifications_route),
        )
        .route(
            "/courses/{course_id}/leagues/{week_key}",
            get(crate::m6::get_league_bracket_route),
        )
        .route(
            "/courses/{course_id}/leagues/score",
            post(crate::m6::upsert_league_score_route),
        )
        .route(
            "/courses/{course_id}/leagues/rollover",
            post(crate::m6::rollover_league_route),
        )
        .route(
            "/courses/{course_id}/bosses",
            get(crate::m6::list_bosses_route).post(crate::m6::create_boss_route),
        )
        .route(
            "/courses/{course_id}/bosses/{boss_id}/lobbies",
            post(crate::m6::create_or_join_boss_lobby_route),
        )
        .route(
            "/courses/{course_id}/bosses/{boss_id}/strike",
            post(crate::m6::strike_boss_route),
        )
        .route(
            "/courses/{course_id}/bosses/{boss_id}/claim",
            post(crate::m6::claim_boss_chest_route),
        )
        .route("/bosses/ws", get(crate::m6::boss_websocket))
        .route(
            "/courses/{course_id}/clans",
            post(crate::m6::create_clan_route),
        )
        .route(
            "/courses/{course_id}/clans/{clan_id}/join",
            post(crate::m6::join_clan_route),
        )
        .route(
            "/courses/{course_id}/clans/{clan_id}/wars",
            post(crate::m6::start_clan_war_route),
        )
        .route(
            "/courses/{course_id}/clans/{clan_id}/wars/{war_id}/contribute",
            post(crate::m6::contribute_clan_war_route),
        )
        .route(
            "/courses/{course_id}/clans/{clan_id}/wars/{war_id}",
            get(crate::m6::get_clan_war_route),
        )
        .route(
            "/courses/{course_id}/economy",
            get(crate::m6::get_economy_route),
        )
        .route(
            "/notifications/devices",
            axum::routing::post(crate::m7::register_device_route),
        )
        .route(
            "/notifications/preferences",
            axum::routing::put(crate::m7::upsert_preferences_route),
        )
        .route("/notifications/outbox", get(crate::m7::list_outbox_route))
        .route(
            "/courses/{course_id}/notifications/shield-digest",
            axum::routing::post(crate::m7::enqueue_shield_digest_route),
        )
        .route(
            "/notifications/dispatch",
            axum::routing::post(crate::m7::dispatch_notifications_route),
        )
        .route(
            "/me/pseudonymize",
            axum::routing::post(crate::m7::pseudonymize_me_route),
        )
        .route(
            "/courses/{course_id}/retention/sweep",
            axum::routing::post(crate::m7::retention_sweep_route),
        )
}
