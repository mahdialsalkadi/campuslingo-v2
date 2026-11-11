//! M1 acceptance tests. They run only through scripts/run_m1_db_tests.mjs,
//! which creates a disposable PostgreSQL cluster with the four runtime roles.
//!
//! These tests intentionally use the HTTP surface for every authorization
//! decision and use direct SQL only to inspect durable state or prove that a
//! role lacks a privilege.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::ServiceExt;
use uuid::Uuid;

use campuslingo_server::auth::{AuthLimiter, hash_password};
use campuslingo_server::config::Settings;
use campuslingo_server::recovery::{
    DeliveryOutcome, deliver_once, http_client, validate_delivery_endpoint,
};
use campuslingo_server::{AppState, build_router};
use campuslingo_storage::crypto::RecoveryKey;
use campuslingo_storage::{begin_as, jobs};

const PASSWORD: &str = "correct horse battery staple";
const NEW_PASSWORD: &str = "a different long password";

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set by run_m1_db_tests.mjs"))
}

fn email(prefix: &str) -> String {
    format!("{prefix}-{}@example.test", Uuid::new_v4())
}

fn uuid_field(value: &Value, field: &str) -> Uuid {
    let text = value
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("missing {field} in {value}"));
    Uuid::parse_str(text).unwrap()
}

fn session_cookie(headers: &HeaderMap) -> String {
    headers
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_else(|| panic!("missing Set-Cookie header"))
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}

async fn pool(name: &str, maximum: u32) -> PgPool {
    PgPoolOptions::new()
        .max_connections(maximum)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&env(name))
        .await
        .unwrap_or_else(|error| panic!("{name} connection failed: {error}"))
}

struct TestApp {
    router: Router,
    state: AppState,
    admin: PgPool,
    app_pool: PgPool,
    auth_pool: PgPool,
    grader_pool: PgPool,
    worker: PgPool,
    recovery_pool: PgPool,
    recovery_key: RecoveryKey,
    asset_dir: PathBuf,
}

impl TestApp {
    async fn new() -> Self {
        let admin = pool("CAMPUSLINGO_TEST_ADMIN_URL", 4).await;
        let app_pool = pool("CAMPUSLINGO_TEST_APP_URL", 8).await;
        let auth_pool = pool("CAMPUSLINGO_TEST_AUTH_URL", 4).await;
        let grader_pool = pool("CAMPUSLINGO_TEST_GRADER_URL", 4).await;
        let worker = pool("CAMPUSLINGO_TEST_WORKER_URL", 4).await;
        let recovery_pool = pool("CAMPUSLINGO_TEST_RECOVERY_URL", 2).await;
        let recovery_key = RecoveryKey::from_hex(&env("CAMPUSLINGO_RECOVERY_KEY")).unwrap();
        let asset_dir = PathBuf::from(env("CAMPUSLINGO_TEST_ASSET_DIR"));
        tokio::fs::create_dir_all(&asset_dir).await.unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&asset_dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        }

        let dummy = hash_password(
            "campuslingo-test-dummy".into(),
            Arc::new(tokio::sync::Semaphore::new(1)),
        )
        .await
        .unwrap();
        let settings = Settings {
            bind: "127.0.0.1:0".parse().unwrap(),
            cookie_secure: false,
            allowed_origin: None,
            recovery_key: Some(recovery_key.clone()),
            asset_dir: asset_dir.clone(),
        };
        let state = AppState {
            auth_pool: auth_pool.clone(),
            app_pool: app_pool.clone(),
            grader_pool: grader_pool.clone(),
            settings: Arc::new(settings),
            limiter: Arc::new(AuthLimiter::with_limit(100_000)),
            hashing: Arc::new(tokio::sync::Semaphore::new(1)),
            dummy_hash: dummy.into(),
        };
        let router = build_router(state.clone());
        Self {
            router,
            state,
            admin,
            app_pool,
            auth_pool,
            grader_pool,
            worker,
            recovery_pool,
            recovery_key,
            asset_dir,
        }
    }

    async fn send(
        &self,
        method: Method,
        uri: &str,
        cookie: Option<&str>,
        content_type: Option<&str>,
        body: Vec<u8>,
        extra: &[(&str, &str)],
    ) -> (StatusCode, HeaderMap, Vec<u8>) {
        send_with(&self.router, method, uri, cookie, content_type, body, extra).await
    }

    async fn json(
        &self,
        method: Method,
        uri: &str,
        cookie: Option<&str>,
        body: Value,
    ) -> (StatusCode, HeaderMap, Value) {
        let (status, headers, bytes) = self
            .send(
                method,
                uri,
                cookie,
                Some("application/json"),
                body.to_string().into_bytes(),
                &[],
            )
            .await;
        (status, headers, decode_json(&bytes))
    }

    async fn register(&self, address: &str) -> (Uuid, String) {
        let (status, headers, value) = self
            .json(
                Method::POST,
                "/v1/auth/register",
                None,
                json!({"email": address, "password": PASSWORD}),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "register: {value}");
        (uuid_field(&value, "user_id"), session_cookie(&headers))
    }

    async fn login(&self, address: &str, password: &str) -> (StatusCode, HeaderMap, Value) {
        self.json(
            Method::POST,
            "/v1/auth/login",
            None,
            json!({"email": address, "password": password}),
        )
        .await
    }

    async fn tenant(&self, cookie: &str, name: &str) -> Uuid {
        let (status, _, value) = self
            .json(
                Method::POST,
                "/v1/tenants",
                Some(cookie),
                json!({"name": name}),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "tenant: {value}");
        uuid_field(&value, "tenant_id")
    }

    async fn add_tenant_member(&self, cookie: &str, tenant: Uuid, address: &str, role: &str) {
        let (status, _, value) = self
            .json(
                Method::POST,
                &format!("/v1/tenants/{tenant}/members"),
                Some(cookie),
                json!({"email": address, "role": role}),
            )
            .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "tenant member: {value}");
    }

    async fn course(&self, cookie: &str, tenant: Uuid, name: &str) -> Uuid {
        let (status, _, value) = self
            .json(
                Method::POST,
                &format!("/v1/tenants/{tenant}/courses"),
                Some(cookie),
                json!({"name": name}),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "course: {value}");
        uuid_field(&value, "course_id")
    }

    async fn add_course_member(
        &self,
        cookie: &str,
        course: Uuid,
        address: &str,
        role: &str,
    ) -> StatusCode {
        let (status, _, value) = self
            .json(
                Method::POST,
                &format!("/v1/courses/{course}/members"),
                Some(cookie),
                json!({"email": address, "role": role}),
            )
            .await;
        if status != StatusCode::NO_CONTENT {
            eprintln!("course member failed: {status:?} {value}");
        }
        status
    }

    async fn node(&self, cookie: &str, course: Uuid, title: &str) -> Uuid {
        let (status, _, value) = self
            .json(
                Method::POST,
                &format!("/v1/courses/{course}/nodes"),
                Some(cookie),
                json!({"title": title}),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "node: {value}");
        uuid_field(&value, "node_id")
    }

    async fn challenge(&self, cookie: &str, course: Uuid, node: Uuid, correct: u8) -> Uuid {
        let (status, _, value) = self
            .json(
                Method::POST,
                &format!("/v1/courses/{course}/nodes/{node}/challenges"),
                Some(cookie),
                json!({
                    "prompt": "Which option is correct?",
                    "options": ["alpha", "beta", "gamma", "delta"],
                    "correct_index": correct,
                    "explanation": "The grader owns this explanation."
                }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "challenge: {value}");
        uuid_field(&value, "challenge_id")
    }

    async fn attempt(&self, cookie: &str, course: Uuid, challenge: Uuid) -> Uuid {
        let (status, _, value) = self
            .json(
                Method::POST,
                &format!("/v1/courses/{course}/challenges/{challenge}/attempts"),
                Some(cookie),
                json!({}),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "attempt: {value}");
        uuid_field(&value, "attempt_id")
    }

    async fn answer(&self, cookie: &str, attempt: Uuid, index: u8) -> (StatusCode, Value) {
        let (status, _, value) = self
            .json(
                Method::POST,
                &format!("/v1/attempts/{attempt}/answer"),
                Some(cookie),
                json!({"selected_index": index}),
            )
            .await;
        (status, value)
    }

    async fn publish(&self, cookie: &str, course: Uuid, rights: bool) -> StatusCode {
        let (status, _, _) = self
            .json(
                Method::POST,
                &format!("/v1/courses/{course}/publish"),
                Some(cookie),
                json!({"rights_attested": rights}),
            )
            .await;
        status
    }

    async fn enroll(&self, cookie: &str, course: Uuid) -> StatusCode {
        let (status, _, _) = self
            .json(
                Method::POST,
                &format!("/v1/courses/{course}/enroll"),
                Some(cookie),
                json!({}),
            )
            .await;
        status
    }

    async fn enqueue(
        &self,
        cookie: &str,
        course: Uuid,
        key: Uuid,
        payload: &Value,
    ) -> (StatusCode, Value) {
        let key_text = key.to_string();
        let (status, _, bytes) = self
            .send(
                Method::POST,
                &format!("/v1/courses/{course}/jobs"),
                Some(cookie),
                Some("application/json"),
                payload.to_string().into_bytes(),
                &[("idempotency-key", key_text.as_str())],
            )
            .await;
        (status, decode_json(&bytes))
    }

    async fn get_job(&self, cookie: &str, course: Uuid, job: Uuid) -> (StatusCode, Value) {
        let (status, _, value) = self
            .json(
                Method::GET,
                &format!("/v1/courses/{course}/jobs/{job}"),
                Some(cookie),
                Value::Null,
            )
            .await;
        (status, value)
    }

    async fn asset_count(&self) -> usize {
        let mut entries = tokio::fs::read_dir(&self.asset_dir).await.unwrap();
        let mut count = 0;
        while entries.next_entry().await.unwrap().is_some() {
            count += 1;
        }
        count
    }
}

async fn send_with(
    router: &Router,
    method: Method,
    uri: &str,
    cookie: Option<&str>,
    content_type: Option<&str>,
    body: Vec<u8>,
    extra: &[(&str, &str)],
) -> (StatusCode, HeaderMap, Vec<u8>) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    if let Some(content_type) = content_type {
        builder = builder.header(header::CONTENT_TYPE, content_type);
    }
    for (name, value) in extra {
        builder = builder.header(*name, *value);
    }
    let mut request = builder.body(Body::from(body)).unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 41234))));
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap()
        .to_vec();
    (status, headers, bytes)
}

fn decode_json(bytes: &[u8]) -> Value {
    if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(bytes)
            .unwrap_or_else(|_| json!({"non_json_body": String::from_utf8_lossy(bytes)}))
    }
}

fn permission_denied(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(database) if database.code().as_deref() == Some("42501"))
}

struct Fixture {
    owner: String,
    student_id: Uuid,
    student: String,
    student_email: String,
    tenant: Uuid,
    course: Uuid,
    node: Uuid,
    challenge: Uuid,
}

async fn fixture(app: &TestApp, correct: u8) -> Fixture {
    let owner_email = email("owner");
    let student_email = email("student");
    let (_, owner) = app.register(&owner_email).await;
    let (student_id, student) = app.register(&student_email).await;
    let tenant = app.tenant(&owner, "Tenant").await;
    let course = app.course(&owner, tenant, "Private course").await;
    assert_eq!(
        app.add_course_member(&owner, course, &student_email, "student")
            .await,
        StatusCode::NO_CONTENT
    );
    let node = app.node(&owner, course, "Unit one").await;
    let challenge = app.challenge(&owner, course, node, correct).await;
    Fixture {
        owner,
        student_id,
        student,
        student_email,
        tenant,
        course,
        node,
        challenge,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the private M1 PostgreSQL harness"]
async fn sessions_recovery_and_origin_controls() {
    let app = TestApp::new().await;
    let address = email("recovery");
    let (user_id, cookie) = app.register(&address).await;

    assert_eq!(
        app.json(Method::GET, "/v1/me", Some(&cookie), Value::Null)
            .await
            .0,
        StatusCode::OK
    );
    let me = app
        .json(Method::GET, "/v1/me", Some(&cookie), Value::Null)
        .await;
    assert_eq!(uuid_field(&me.2, "user_id"), user_id);

    let token = cookie.split_once('=').unwrap().1;
    let stored: Vec<u8> = sqlx::query_scalar(
        "SELECT token_hash FROM sessions WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(user_id)
    .fetch_one(&app.admin)
    .await
    .unwrap();
    assert_eq!(stored, Sha256::digest(token.as_bytes()).to_vec());
    assert_ne!(stored, token.as_bytes());

    let (status, headers, value) = app.login(&address, "wrong password entirely").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{value}");
    assert!(headers.get(header::SET_COOKIE).is_none());

    let (status, headers, _) = app.login(&address, PASSWORD).await;
    assert_eq!(status, StatusCode::OK);
    let cookie_text = headers
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(cookie_text.contains("HttpOnly"));
    assert!(cookie_text.contains("SameSite=Strict"));
    assert!(cookie_text.contains("Path=/"));
    assert!(cookie_text.contains("Max-Age=604800"));

    let mut tampered = cookie.clone();
    let replacement = if tampered.ends_with('0') { '1' } else { '0' };
    tampered.pop();
    tampered.push(replacement);
    assert_eq!(
        app.json(Method::GET, "/v1/me", Some(&tampered), Value::Null)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );

    // Unknown addresses receive the same accepted response and add no outbox row.
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM recovery_outbox")
        .fetch_one(&app.admin)
        .await
        .unwrap();
    let (status, _, _) = app
        .json(
            Method::POST,
            "/v1/auth/recover",
            None,
            json!({"email": email("nobody")}),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM recovery_outbox")
        .fetch_one(&app.admin)
        .await
        .unwrap();
    assert_eq!(before, after);

    let (status, _, _) = app
        .json(
            Method::POST,
            "/v1/auth/recover",
            None,
            json!({"email": address}),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let outbox: Vec<Vec<u8>> =
        sqlx::query_scalar("SELECT ciphertext FROM recovery_outbox ORDER BY created_at DESC")
            .fetch_all(&app.admin)
            .await
            .unwrap();
    let token = outbox
        .iter()
        .find_map(|ciphertext| {
            app.recovery_key
                .decrypt(ciphertext)
                .ok()
                .filter(|(id, _, _)| *id == user_id)
                .map(|(_, _, token)| token)
        })
        .expect("recovery token for user");

    let (status, _, _) = app
        .json(
            Method::POST,
            "/v1/auth/recover/confirm",
            None,
            json!({"token": token, "new_password": NEW_PASSWORD}),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Confirmation revokes every existing session and the token is single-use.
    assert_eq!(
        app.json(Method::GET, "/v1/me", Some(&cookie), Value::Null)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let (status, _, _) = app
        .json(
            Method::POST,
            "/v1/auth/recover/confirm",
            None,
            json!({"token": token, "new_password": NEW_PASSWORD}),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    assert_eq!(
        app.login(&address, PASSWORD).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (status, _, _) = app.login(&address, NEW_PASSWORD).await;
    assert_eq!(status, StatusCode::OK);

    // Expired sessions fail closed.
    sqlx::query(
        "UPDATE sessions SET expires_at = clock_timestamp() - interval '1 second' WHERE user_id = $1",
    )
    .bind(user_id)
    .execute(&app.admin)
    .await
    .unwrap();
    assert_eq!(
        app.json(Method::GET, "/v1/me", Some(&cookie), Value::Null)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );

    // Extra registration fields cannot impersonate a user id.
    let (status, _, _) = app
        .json(
            Method::POST,
            "/v1/auth/register",
            None,
            json!({"email": email("extra"), "password": PASSWORD, "user_id": user_id}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // Cookie-authenticated writes reject a cross-origin browser request.
    let (status, _, value) = app
        .send(
            Method::POST,
            "/v1/auth/login",
            None,
            Some("application/json"),
            json!({"email": address, "password": NEW_PASSWORD})
                .to_string()
                .into_bytes(),
            &[
                ("origin", "https://evil.example"),
                ("host", "campus.example"),
            ],
        )
        .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "{}",
        String::from_utf8_lossy(&value)
    );
    let (status, _, _) = app
        .send(
            Method::POST,
            "/v1/auth/login",
            None,
            Some("application/json"),
            json!({"email": address, "password": NEW_PASSWORD})
                .to_string()
                .into_bytes(),
            &[
                ("origin", "https://campus.example"),
                ("host", "campus.example"),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the private M1 PostgreSQL harness"]
async fn course_isolation_rls_and_publication_controls() {
    let app = TestApp::new().await;
    let outsider_email = email("outsider");
    let (outsider_id, outsider) = app.register(&outsider_email).await;
    let fixture = fixture(&app, 1).await;

    let content_uri = format!("/v1/courses/{}/content", fixture.course);
    assert_eq!(
        app.json(Method::GET, &content_uri, Some(&outsider), Value::Null)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.json(Method::GET, &content_uri, None, Value::Null)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );

    let (status, _, content) = app
        .json(
            Method::GET,
            &content_uri,
            Some(&fixture.student),
            Value::Null,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{content}");
    let challenge = &content["challenges"][0];
    assert_eq!(challenge["id"], fixture.challenge.to_string());
    let object = challenge.as_object().unwrap();
    assert_eq!(object.len(), 4);
    for forbidden in ["correct_index", "answer", "explanation", "selected_index"] {
        assert!(!object.contains_key(forbidden));
    }
    assert_eq!(content["nodes"][0]["id"], fixture.node.to_string());

    let (status, _, value) = app
        .json(
            Method::POST,
            &format!("/v1/courses/{}/nodes", fixture.course),
            Some(&outsider),
            json!({"title": "Outsider node"}),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{value}");

    let status = app
        .add_course_member(&outsider, fixture.course, &fixture.student_email, "student")
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        app.publish(&outsider, fixture.course, true).await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.enroll(&outsider, fixture.course).await,
        StatusCode::NOT_FOUND
    );

    let attempt = app
        .attempt(&fixture.student, fixture.course, fixture.challenge)
        .await;
    assert_eq!(
        app.answer(&outsider, attempt, 1).await.0,
        StatusCode::NOT_FOUND
    );

    // Direct application-role reads are filtered by RLS for a non-member.
    let mut transaction = begin_as(&app.app_pool, outsider_id).await.unwrap();
    let visible_courses: i64 = sqlx::query_scalar("SELECT count(*) FROM courses WHERE id = $1")
        .bind(fixture.course)
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    let visible_challenges: i64 =
        sqlx::query_scalar("SELECT count(*) FROM challenges WHERE id = $1")
            .bind(fixture.challenge)
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
    let visible_tenants: i64 = sqlx::query_scalar("SELECT count(*) FROM tenants")
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    assert_eq!(visible_courses, 0);
    assert_eq!(visible_challenges, 0);
    assert_eq!(visible_tenants, 0);

    // Course membership grants course read/study, not tenant metadata access.
    let mut transaction = begin_as(&app.app_pool, fixture.student_id).await.unwrap();
    let visible_courses: i64 = sqlx::query_scalar("SELECT count(*) FROM courses WHERE id = $1")
        .bind(fixture.course)
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    let visible_tenants: i64 = sqlx::query_scalar("SELECT count(*) FROM tenants")
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    assert_eq!(visible_courses, 1);
    assert_eq!(visible_tenants, 0);

    // Tenant staff membership exposes tenancy metadata but not course content
    // until the user is explicitly added to that course.
    let staff_email = email("staff");
    let (_, staff) = app.register(&staff_email).await;
    app.add_tenant_member(&fixture.owner, fixture.tenant, &staff_email, "instructor")
        .await;
    let (status, _, tenants) = app
        .json(Method::GET, "/v1/tenants", Some(&staff), Value::Null)
        .await;
    assert_eq!(status, StatusCode::OK, "{tenants}");
    assert_eq!(tenants[0]["id"], fixture.tenant.to_string());
    assert_eq!(
        app.json(Method::GET, &content_uri, Some(&staff), Value::Null)
            .await
            .0,
        StatusCode::NOT_FOUND
    );

    // The application role cannot read users, answers, or mutate jobs.
    for sql in [
        "SELECT count(*) FROM users",
        "SELECT correct_index FROM challenge_answers LIMIT 1",
        "UPDATE jobs SET stage = 'forged'",
    ] {
        let error = sqlx::query(sql).execute(&app.app_pool).await.unwrap_err();
        assert!(permission_denied(&error), "{sql}: {error}");
    }
    // Authentication material is not reachable from course roles.
    let error = sqlx::query("SELECT count(*) FROM courses")
        .execute(&app.auth_pool)
        .await
        .unwrap_err();
    assert!(permission_denied(&error));
    let error = sqlx::query("SELECT count(*) FROM users")
        .execute(&app.grader_pool)
        .await
        .unwrap_err();
    assert!(permission_denied(&error));

    // Only the grader role can read the secret index.
    let answer: i16 =
        sqlx::query_scalar("SELECT correct_index FROM challenge_answers WHERE challenge_id = $1")
            .bind(fixture.challenge)
            .fetch_one(&app.grader_pool)
            .await
            .unwrap();
    assert_eq!(answer, 1);

    // Publication requires an explicit rights attestation.
    assert_eq!(
        app.publish(&fixture.owner, fixture.course, false).await,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        app.publish(&fixture.owner, fixture.course, true).await,
        StatusCode::NO_CONTENT
    );

    // A public course is readable before enrollment, but study requires joining.
    assert_eq!(
        app.json(Method::GET, &content_uri, Some(&outsider), Value::Null)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        app.json(
            Method::GET,
            &format!(
                "/v1/courses/{}/challenges/{}/practice",
                fixture.course, fixture.challenge
            ),
            Some(&outsider),
            Value::Null,
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.enroll(&outsider, fixture.course).await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.json(
            Method::GET,
            &format!(
                "/v1/courses/{}/challenges/{}/practice",
                fixture.course, fixture.challenge
            ),
            Some(&outsider),
            Value::Null,
        )
        .await
        .0,
        StatusCode::OK
    );

    // A course member still cannot manage the course.
    let error = {
        let mut transaction = begin_as(&app.app_pool, fixture.student_id).await.unwrap();
        let result = sqlx::query("UPDATE courses SET name = 'stolen' WHERE id = $1")
            .bind(fixture.course)
            .execute(&mut *transaction)
            .await;
        transaction.commit().await.unwrap();
        result
    };
    assert_eq!(error.unwrap().rows_affected(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the private M1 PostgreSQL harness"]
async fn review_and_reward_are_exactly_once() {
    let app = TestApp::new().await;
    let fixture = fixture(&app, 1).await;
    let attempt = app
        .attempt(&fixture.student, fixture.course, fixture.challenge)
        .await;

    let (status, first) = app.answer(&fixture.student, attempt, 1).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["correct"], true);
    assert_eq!(first["rating"], 3);
    assert_eq!(first["duplicate"], false);

    let (status, again) = app.answer(&fixture.student, attempt, 1).await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["duplicate"], true);
    assert_eq!(again["rating"], first["rating"]);
    assert_eq!(again["scheduled_days"], first["scheduled_days"]);

    let (status, _) = app.answer(&fixture.student, attempt, 0).await;
    assert_eq!(status, StatusCode::CONFLICT);

    let reviews: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM review_logs WHERE user_id = $1 AND challenge_id = $2",
    )
    .bind(fixture.student_id)
    .bind(fixture.challenge)
    .fetch_one(&app.admin)
    .await
    .unwrap();
    let reward_rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM reward_ledger WHERE user_id = $1")
            .bind(fixture.student_id)
            .fetch_one(&app.admin)
            .await
            .unwrap();
    let card_reps: i32 =
        sqlx::query_scalar("SELECT reps FROM fsrs_cards WHERE user_id = $1 AND challenge_id = $2")
            .bind(fixture.student_id)
            .bind(fixture.challenge)
            .fetch_one(&app.admin)
            .await
            .unwrap();
    assert_eq!(reviews, 1);
    assert_eq!(reward_rows, 1);
    assert_eq!(card_reps, 1);

    // A wrong answer creates one more review and one more reward.
    let second = app
        .attempt(&fixture.student, fixture.course, fixture.challenge)
        .await;
    let (status, wrong) = app.answer(&fixture.student, second, 0).await;
    assert_eq!(status, StatusCode::OK, "{wrong}");
    assert_eq!(wrong["correct"], false);
    assert_eq!(wrong["rating"], 1);
    let reviews: i64 = sqlx::query_scalar("SELECT count(*) FROM review_logs WHERE user_id = $1")
        .bind(fixture.student_id)
        .fetch_one(&app.admin)
        .await
        .unwrap();
    let reward_total: i64 =
        sqlx::query_scalar("SELECT coalesce(sum(amount), 0) FROM reward_ledger WHERE user_id = $1")
            .bind(fixture.student_id)
            .fetch_one(&app.admin)
            .await
            .unwrap();
    assert_eq!(reviews, 2);
    assert_eq!(reward_total, 2);

    // Expired attempts fail closed and create no extra rows.
    let expired = app
        .attempt(&fixture.student, fixture.course, fixture.challenge)
        .await;
    sqlx::query(
        "UPDATE attempts SET expires_at = clock_timestamp() - interval '1 second' WHERE id = $1",
    )
    .bind(expired)
    .execute(&app.admin)
    .await
    .unwrap();
    assert_eq!(
        app.answer(&fixture.student, expired, 1).await.0,
        StatusCode::CONFLICT
    );
    let reviews: i64 = sqlx::query_scalar("SELECT count(*) FROM review_logs WHERE user_id = $1")
        .bind(fixture.student_id)
        .fetch_one(&app.admin)
        .await
        .unwrap();
    assert_eq!(reviews, 2);

    // A fresh router over the same durable state is the restart case: a same-
    // answer retry must still return the existing result, not append rewards.
    let restarted = build_router(app.state.clone());
    let (status, _, bytes) = send_with(
        &restarted,
        Method::POST,
        &format!("/v1/attempts/{attempt}/answer"),
        Some(&fixture.student),
        Some("application/json"),
        json!({"selected_index": 1}).to_string().into_bytes(),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let resumed = decode_json(&bytes);
    assert_eq!(resumed["duplicate"], true);
    let reviews: i64 = sqlx::query_scalar("SELECT count(*) FROM review_logs WHERE user_id = $1")
        .bind(fixture.student_id)
        .fetch_one(&app.admin)
        .await
        .unwrap();
    let reward_rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM reward_ledger WHERE user_id = $1")
            .bind(fixture.student_id)
            .fetch_one(&app.admin)
            .await
            .unwrap();
    assert_eq!(reviews, 2);
    assert_eq!(reward_rows, 2);

    // Audit rows are append-only even for the migration owner.
    let error = sqlx::query("UPDATE reward_ledger SET amount = 2 WHERE user_id = $1")
        .bind(fixture.student_id)
        .execute(&app.admin)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("append-only"), "{error}");
    let error = sqlx::query("DELETE FROM review_logs WHERE user_id = $1")
        .bind(fixture.student_id)
        .execute(&app.admin)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("append-only"), "{error}");

    // The application role cannot read economy or scheduler tables at all.
    for sql in [
        "SELECT count(*) FROM reward_ledger",
        "SELECT count(*) FROM review_logs",
        "SELECT count(*) FROM fsrs_cards",
    ] {
        let error = sqlx::query(sql).execute(&app.app_pool).await.unwrap_err();
        assert!(permission_denied(&error), "{sql}: {error}");
    }

    // The grader role holds those grants, so RLS must still hide another
    // learner's rows even when an actor is supplied.
    let other_id = app.register(&email("other")).await.0;
    let mut transaction = begin_as(&app.grader_pool, other_id).await.unwrap();
    let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM reward_ledger")
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    let visible_cards: i64 = sqlx::query_scalar("SELECT count(*) FROM fsrs_cards")
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    assert_eq!(visible, 0);
    assert_eq!(visible_cards, 0);

    let mut transaction = begin_as(&app.grader_pool, fixture.student_id)
        .await
        .unwrap();
    let own_rewards: i64 = sqlx::query_scalar("SELECT count(*) FROM reward_ledger")
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    let own_cards: i64 = sqlx::query_scalar("SELECT count(*) FROM fsrs_cards")
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    assert_eq!(own_rewards, 2);
    assert_eq!(own_cards, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the private M1 PostgreSQL harness"]
async fn jobs_are_idempotent_and_leases_are_enforced() {
    let app = TestApp::new().await;
    let fixture = fixture(&app, 1).await;
    let outsider = app.register(&email("job-outsider")).await.1;
    let key = Uuid::new_v4();
    let payload = json!({"kind": "probe", "value": 1});

    let (status, job) = app
        .enqueue(&fixture.owner, fixture.course, key, &payload)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{job}");
    let job_id = uuid_field(&job, "id");
    let (status, again) = app
        .enqueue(&fixture.owner, fixture.course, key, &payload)
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(uuid_field(&again, "id"), job_id);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE course_id = $1")
        .bind(fixture.course)
        .fetch_one(&app.admin)
        .await
        .unwrap();
    assert_eq!(count, 1);

    let (status, _) = app
        .enqueue(
            &fixture.owner,
            fixture.course,
            key,
            &json!({"kind": "probe", "value": 2}),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = app
        .enqueue(&outsider, fixture.course, Uuid::new_v4(), &payload)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let claim = jobs::reaper_and_claim(&app.worker).await.unwrap().unwrap();
    assert_eq!(claim.id, job_id);
    assert!(jobs::reaper_and_claim(&app.worker).await.unwrap().is_none());

    let stale = Uuid::new_v4();
    assert!(matches!(
        jobs::heartbeat(&app.worker, job_id, stale, "probe", 0, 2).await,
        Err(campuslingo_storage::StorageError::Conflict)
    ));
    jobs::heartbeat(&app.worker, job_id, claim.lease_token, "extract", 1, 2)
        .await
        .unwrap();
    let (status, progress) = app.get_job(&fixture.owner, fixture.course, job_id).await;
    assert_eq!(status, StatusCode::OK, "{progress}");
    assert_eq!(progress["stage"], "extract");
    assert_eq!(progress["progress_done"], 1);
    assert_eq!(progress["progress_total"], 2);

    let output = json!({"ok": true});
    jobs::complete(&app.worker, job_id, claim.lease_token, &output)
        .await
        .unwrap();
    assert!(matches!(
        jobs::complete(&app.worker, job_id, claim.lease_token, &output).await,
        Err(campuslingo_storage::StorageError::Conflict)
    ));
    let results: i64 = sqlx::query_scalar("SELECT count(*) FROM job_results WHERE job_id = $1")
        .bind(job_id)
        .fetch_one(&app.admin)
        .await
        .unwrap();
    let succeeded: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM job_events WHERE job_id = $1 AND stage = 'succeeded'",
    )
    .bind(job_id)
    .fetch_one(&app.admin)
    .await
    .unwrap();
    assert_eq!(results, 1);
    assert_eq!(succeeded, 1);
    let (status, view) = app.get_job(&fixture.owner, fixture.course, job_id).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(view["status"], "succeeded");
    assert_eq!(view["output"]["ok"], true);
    // Completed jobs remain idempotent and are not re-enqueued.
    let (status, completed_again) = app
        .enqueue(&fixture.owner, fixture.course, key, &payload)
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(uuid_field(&completed_again, "id"), job_id);

    // Bounded, deterministic retries.
    let retry_key = Uuid::new_v4();
    let (_, retry_job) = app
        .enqueue(&fixture.owner, fixture.course, retry_key, &payload)
        .await;
    let retry_id = uuid_field(&retry_job, "id");
    for expected_attempt in 1..=3 {
        let claim = jobs::reaper_and_claim(&app.worker).await.unwrap().unwrap();
        assert_eq!(claim.id, retry_id);
        assert_eq!(claim.attempts, expected_attempt);
        jobs::fail(&app.worker, retry_id, claim.lease_token, "transient", true)
            .await
            .unwrap();
    }
    assert!(jobs::reaper_and_claim(&app.worker).await.unwrap().is_none());
    let retry_status: String = sqlx::query_scalar("SELECT status FROM jobs WHERE id = $1")
        .bind(retry_id)
        .fetch_one(&app.admin)
        .await
        .unwrap();
    assert_eq!(retry_status, "failed");
    let retries: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM job_events WHERE job_id = $1 AND stage = 'retry_queued'",
    )
    .bind(retry_id)
    .fetch_one(&app.admin)
    .await
    .unwrap();
    assert_eq!(retries, 2);

    // An expired lease is reclaimable, and the old token cannot heartbeat.
    let lease_key = Uuid::new_v4();
    let (_, lease_job) = app
        .enqueue(&fixture.owner, fixture.course, lease_key, &payload)
        .await;
    let lease_id = uuid_field(&lease_job, "id");
    let first_claim = jobs::reaper_and_claim(&app.worker).await.unwrap().unwrap();
    sqlx::query(
        "UPDATE jobs SET lease_until = clock_timestamp() - interval '1 second' WHERE id = $1",
    )
    .bind(lease_id)
    .execute(&app.admin)
    .await
    .unwrap();
    let reclaimed = jobs::reaper_and_claim(&app.worker).await.unwrap().unwrap();
    assert_eq!(reclaimed.id, lease_id);
    assert_eq!(reclaimed.attempts, 2);
    assert_ne!(reclaimed.lease_token, first_claim.lease_token);
    assert!(matches!(
        jobs::heartbeat(
            &app.worker,
            lease_id,
            first_claim.lease_token,
            "stale",
            0,
            1
        )
        .await,
        Err(campuslingo_storage::StorageError::Conflict)
    ));
    jobs::heartbeat(
        &app.worker,
        lease_id,
        reclaimed.lease_token,
        "reclaimed",
        0,
        1,
    )
    .await
    .unwrap();
    jobs::complete(&app.worker, lease_id, reclaimed.lease_token, &output)
        .await
        .unwrap();

    // Two workers cannot claim the same row concurrently.
    let key_a = Uuid::new_v4();
    let key_b = Uuid::new_v4();
    let (_, job_a) = app
        .enqueue(&fixture.owner, fixture.course, key_a, &payload)
        .await;
    let (_, job_b) = app
        .enqueue(&fixture.owner, fixture.course, key_b, &payload)
        .await;
    let (left, right) = tokio::join!(
        jobs::reaper_and_claim(&app.worker),
        jobs::reaper_and_claim(&app.worker)
    );
    let left = left.unwrap().unwrap();
    let right = right.unwrap().unwrap();
    assert_ne!(left.id, right.id);
    let mut ids = [left.id, right.id];
    ids.sort();
    let mut expected = [uuid_field(&job_a, "id"), uuid_field(&job_b, "id")];
    expected.sort();
    assert_eq!(ids, expected);
    jobs::complete(&app.worker, left.id, left.lease_token, &output)
        .await
        .unwrap();
    jobs::complete(&app.worker, right.id, right.lease_token, &output)
        .await
        .unwrap();

    // Job details are hidden from non-managers.
    assert_eq!(
        app.get_job(&outsider, fixture.course, job_id).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the private M1 PostgreSQL harness"]
async fn assets_are_private_streamed_and_authorized() {
    let app = TestApp::new().await;
    let fixture = fixture(&app, 1).await;
    let outsider_email = email("asset-outsider");
    let (outsider_id, outsider) = app.register(&outsider_email).await;
    let instructor_email = email("asset-instructor");
    let (_, instructor) = app.register(&instructor_email).await;
    assert_eq!(
        app.add_course_member(
            &fixture.owner,
            fixture.course,
            &instructor_email,
            "instructor"
        )
        .await,
        StatusCode::NO_CONTENT
    );

    // Larger than the global 64 KiB body limit proves the upload path streams.
    let body = vec![b'a'; 100 * 1024];
    let digest = hex::encode(Sha256::digest(&body));
    let before = app.asset_count().await;
    let (status, _, bytes) = app
        .send(
            Method::POST,
            &format!("/v1/courses/{}/assets", fixture.course),
            Some(&fixture.owner),
            Some("text/plain; charset=utf-8"),
            body.clone(),
            &[],
        )
        .await;
    let value = decode_json(&bytes);
    assert_eq!(status, StatusCode::CREATED, "{value}");
    let asset = uuid_field(&value, "asset_id");
    assert_eq!(value["bytes"], json!(body.len()));
    assert_eq!(value["media_type"], "text/plain");
    assert_eq!(value["sha256"], digest);
    assert_eq!(app.asset_count().await, before + 1);

    let path = app.asset_dir.join(asset.to_string());
    let stored = tokio::fs::read(&path).await.unwrap();
    assert_eq!(stored, body);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = tokio::fs::metadata(&path)
            .await
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "asset must not be group/world accessible");
    }

    // A student cannot upload, and a rejected upload leaves no orphan file.
    let before = app.asset_count().await;
    let (status, _, _) = app
        .send(
            Method::POST,
            &format!("/v1/courses/{}/assets", fixture.course),
            Some(&fixture.student),
            Some("text/plain"),
            b"student upload".to_vec(),
            &[],
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(app.asset_count().await, before);

    // A course instructor can upload even though only the tenant owner can publish.
    let (status, _, _) = app
        .send(
            Method::POST,
            &format!("/v1/courses/{}/assets", fixture.course),
            Some(&instructor),
            Some("application/pdf"),
            b"%PDF-1.4 test".to_vec(),
            &[],
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    for media_type in ["text/html", "application/javascript", "image/svg+xml"] {
        let (status, _, _) = app
            .send(
                Method::POST,
                &format!("/v1/courses/{}/assets", fixture.course),
                Some(&fixture.owner),
                Some(media_type),
                b"rejected".to_vec(),
                &[],
            )
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{media_type}");
    }
    let before = app.asset_count().await;
    let (status, _, _) = app
        .send(
            Method::POST,
            &format!("/v1/courses/{}/assets", fixture.course),
            Some(&fixture.owner),
            Some("text/plain"),
            Vec::new(),
            &[],
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(app.asset_count().await, before);

    // Authorized course readers receive the exact bytes and safe headers.
    let uri = format!("/v1/assets/{asset}");
    let (status, headers, downloaded) = app
        .send(
            Method::GET,
            &uri,
            Some(&fixture.student),
            None,
            Vec::new(),
            &[],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(downloaded, body);
    assert_eq!(
        headers.get(header::CONTENT_TYPE).unwrap().to_str().unwrap(),
        "text/plain"
    );
    assert_eq!(
        headers
            .get(header::CONTENT_LENGTH)
            .unwrap()
            .to_str()
            .unwrap(),
        body.len().to_string()
    );
    assert_eq!(
        headers
            .get(header::CONTENT_DISPOSITION)
            .unwrap()
            .to_str()
            .unwrap(),
        "attachment"
    );
    assert_eq!(
        headers
            .get(header::CACHE_CONTROL)
            .unwrap()
            .to_str()
            .unwrap(),
        "private, no-store"
    );
    assert_eq!(
        headers
            .get(header::X_CONTENT_TYPE_OPTIONS)
            .unwrap()
            .to_str()
            .unwrap(),
        "nosniff"
    );
    assert_eq!(
        headers.get(header::ETAG).unwrap().to_str().unwrap(),
        format!("\"{digest}\"").as_str()
    );

    assert_eq!(
        app.send(Method::GET, &uri, Some(&outsider), None, Vec::new(), &[])
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.send(Method::GET, &uri, None, None, Vec::new(), &[])
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );

    // The asset row is invisible in another learner's RLS context.
    let mut transaction = begin_as(&app.app_pool, outsider_id).await.unwrap();
    let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM assets WHERE id = $1")
        .bind(asset)
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    assert_eq!(visible, 0);

    // Publishing a course makes authorized reads available to authenticated
    // learners, while the bytes remain outside the web root.
    assert_eq!(
        app.publish(&fixture.owner, fixture.course, true).await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.send(Method::GET, &uri, Some(&outsider), None, Vec::new(), &[])
            .await
            .0,
        StatusCode::OK
    );

    // Missing files fail closed with an internal error rather than path leakage.
    tokio::fs::remove_file(&path).await.unwrap();
    let (status, _, value) = app
        .send(
            Method::GET,
            &uri,
            Some(&fixture.student),
            None,
            Vec::new(),
            &[],
        )
        .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let text = String::from_utf8_lossy(&value);
    assert!(!text.contains(&app.asset_dir.to_string_lossy().to_string()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the private M1 PostgreSQL harness"]
async fn recovery_delivery_is_leased_then_single_use() {
    let app = TestApp::new().await;

    // Earlier tests intentionally leave an undelivered outbox row. Isolate this
    // test so the claim below can only select the row created here.
    sqlx::query(
        "UPDATE recovery_outbox SET delivered_at = clock_timestamp(), lease_until = NULL
         WHERE delivered_at IS NULL",
    )
    .execute(&app.admin)
    .await
    .unwrap();

    let address = email("delivery");
    let (user_id, _) = app.register(&address).await;
    let (status, _, _) = app
        .json(
            Method::POST,
            "/v1/auth/recover",
            None,
            json!({"email": address}),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let captured = Arc::new(tokio::sync::Mutex::new(Vec::<Value>::new()));
    let sink_state = captured.clone();
    let sink = Router::new().route(
        "/deliver",
        axum::routing::post(move |axum::Json(body): axum::Json<Value>| {
            let captured = sink_state.clone();
            async move {
                captured.lock().await.push(body);
                StatusCode::NO_CONTENT
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = validate_delivery_endpoint(&format!(
        "http://{}/deliver",
        listener.local_addr().unwrap()
    ))
    .unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, sink).await.unwrap();
    });

    let client = http_client().unwrap();
    let delivered = deliver_once(&app.recovery_pool, &app.recovery_key, &endpoint, &client)
        .await
        .unwrap();
    assert!(matches!(delivered, DeliveryOutcome::Delivered { .. }));

    let messages = captured.lock().await;
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["email"], address);
    assert_eq!(messages[0]["user_id"], user_id.to_string());
    assert_eq!(messages[0]["expires_in_seconds"], 15 * 60);
    let token = messages[0]["token"].as_str().unwrap().to_owned();
    assert_eq!(token.len(), 64);
    assert!(token.bytes().all(|byte| byte.is_ascii_hexdigit()));
    drop(messages);

    // A delivered row is not claimed again and the lease is cleared.
    assert_eq!(
        deliver_once(&app.recovery_pool, &app.recovery_key, &endpoint, &client)
            .await
            .unwrap(),
        DeliveryOutcome::Idle
    );
    let (delivered_at_set, attempts): (bool, i32) = sqlx::query_as(
        "SELECT delivered_at IS NOT NULL, delivery_attempts
         FROM recovery_outbox WHERE delivery_attempts > 0
         ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_one(&app.admin)
    .await
    .unwrap();
    assert!(delivered_at_set);
    assert_eq!(attempts, 1);

    // The delivered token still follows the single-use confirmation path.
    let (status, _, _) = app
        .json(
            Method::POST,
            "/v1/auth/recover/confirm",
            None,
            json!({"token": token, "new_password": NEW_PASSWORD}),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = app
        .json(
            Method::POST,
            "/v1/auth/recover/confirm",
            None,
            json!({"token": token, "new_password": NEW_PASSWORD}),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(app.login(&address, NEW_PASSWORD).await.0, StatusCode::OK);

    // The delivery role cannot read identities or session material.
    for sql in [
        "SELECT count(*) FROM users",
        "SELECT count(*) FROM sessions",
        "SELECT count(*) FROM recovery_tokens",
    ] {
        let error = sqlx::query(sql)
            .execute(&app.recovery_pool)
            .await
            .unwrap_err();
        assert!(permission_denied(&error), "{sql}: {error}");
    }
    // Nor can the web authentication role mark outbox rows delivered.
    let error = sqlx::query("UPDATE recovery_outbox SET delivered_at = clock_timestamp()")
        .execute(&app.auth_pool)
        .await
        .unwrap_err();
    assert!(permission_denied(&error));
    handle.abort();
}
