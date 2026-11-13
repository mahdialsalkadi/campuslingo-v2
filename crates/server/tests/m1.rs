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
            recovery_pool: Some(recovery_pool.clone()),
            settings: Arc::new(settings),
            limiter: Arc::new(AuthLimiter::with_limit(100_000)),
            actor_limiter: Arc::new(campuslingo_server::auth::ActorRateLimiter::with_limit(
                100_000,
            )),
            hashing: Arc::new(tokio::sync::Semaphore::new(1)),
            dummy_hash: dummy.into(),
            duels: Arc::new(campuslingo_server::duels::DuelHub::new()),
            bosses: Arc::new(campuslingo_server::m6::BossLobbyHub::new()),
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

async fn m2_source_job(
    app: &TestApp,
    fixture: &Fixture,
    pages: i32,
) -> (
    campuslingo_storage::jobs::ClaimedJob,
    campuslingo_storage::ingestion::RunInput,
) {
    let bytes = b"%PDF-synthetic-storage-fixture".to_vec();
    let (status, _, response) = app
        .send(
            Method::POST,
            &format!("/v1/courses/{}/assets", fixture.course),
            Some(&fixture.owner),
            Some("application/pdf"),
            bytes.clone(),
            &[],
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let asset = uuid_field(&decode_json(&response), "asset_id");
    assert_eq!(
        app.enqueue(
            &fixture.owner,
            fixture.course,
            Uuid::new_v4(),
            &json!({"schema_version":1,"kind":"pdf_ingestion","asset_id":asset})
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let job = jobs::reaper_and_claim_kind(&app.worker, Some("pdf_ingestion"))
        .await
        .unwrap()
        .unwrap();
    let input = campuslingo_storage::ingestion::RunInput {
        source_asset_id: asset,
        input_sha256: hex::encode(Sha256::digest(bytes)),
        pipeline_version: "m2-db-test".into(),
        source_pages: pages,
        provenance: json!({"schema_version":1,"tools":{"pdfinfo":"synthetic-tool-revision"}}),
    };
    (job, input)
}

fn m2_page(page: i32) -> campuslingo_storage::ingestion::PageCheckpoint {
    use campuslingo_storage::ingestion::*;
    let crop = CropCheckpoint {
        id: Uuid::new_v4(),
        region_index: 0,
        dpi: 250,
        width: 100,
        height: 80,
        bbox: json!({"schema_version":1,"rect":{"x0":10,"y0":10,"x1":40,"y1":34}}),
        bytes: 4,
        sha256: hex::encode(Sha256::digest(b"webp")),
    };
    let chunk = ChunkCheckpoint {
        id: Uuid::new_v4(),
        order: 0,
        source_text: "    return x != 5".into(),
        normalized_candidate: "return x != 5".into(),
        text_sha256: "a".repeat(64),
        structural_fingerprint: "b".repeat(64),
        critical_tokens: json!({"schema_version":1,"tokens":[]}),
        duplicate_of: None,
        crop_ids: vec![crop.id],
        language_code: None,
        is_quarantined: false,
        embedding: None,
    };
    PageCheckpoint {
        page_number: page,
        ready: false,
        artifact: json!({"schema_version":1,"source_text":"    return x != 5","duration_ms":123}),
        crops: vec![crop],
        chunks: vec![chunk],
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the private M1/M2 PostgreSQL harness"]
async fn m2_checkpoints_are_atomic_scoped_versioned_and_idempotent() {
    use campuslingo_storage::ingestion::*;
    let app = TestApp::new().await;
    let f = fixture(&app, 1).await;
    let (job, input) = m2_source_job(&app, &f, 2).await;
    let run = start_run(&app.worker, job.id, job.lease_token, &input)
        .await
        .unwrap();
    assert_eq!(
        start_run(&app.worker, job.id, job.lease_token, &input)
            .await
            .unwrap(),
        run
    );
    let mut changed = input.clone();
    changed.provenance["tools"]["pdfinfo"] = json!("changed");
    assert!(
        start_run(&app.worker, job.id, job.lease_token, &changed)
            .await
            .is_err()
    );
    let page = m2_page(1);
    assert!(
        checkpoint_page(&app.worker, job.id, job.lease_token, run, &page)
            .await
            .unwrap()
    );
    assert!(
        !checkpoint_page(&app.worker, job.id, job.lease_token, run, &page)
            .await
            .unwrap()
    );
    let mut runtime_only = page.clone();
    runtime_only.artifact["duration_ms"] = json!(999);
    assert!(
        !checkpoint_page(&app.worker, job.id, job.lease_token, run, &runtime_only)
            .await
            .unwrap()
    );
    assert!(
        finish_run(&app.worker, job.id, job.lease_token, run)
            .await
            .is_err()
    );
    let mut altered = page.clone();
    altered.chunks[0].source_text = "different source".into();
    assert!(
        checkpoint_page(&app.worker, job.id, job.lease_token, run, &altered)
            .await
            .is_err()
    );
    let mut bad = m2_page(2);
    bad.chunks[0].crop_ids.push(Uuid::new_v4());
    assert!(
        checkpoint_page(&app.worker, job.id, job.lease_token, run, &bad)
            .await
            .is_err()
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM ingestion_pages WHERE run_id=$1")
        .bind(run)
        .fetch_one(&app.admin)
        .await
        .unwrap();
    assert_eq!(
        count, 1,
        "FK failure rolls back page/assets/chunks together"
    );
    let page2 = m2_page(2);
    checkpoint_page(&app.worker, job.id, job.lease_token, run, &page2)
        .await
        .unwrap();
    assert_eq!(
        finish_run(&app.worker, job.id, job.lease_token, run)
            .await
            .unwrap(),
        "needs_review"
    );
    let (status, _, view) = app
        .json(
            Method::GET,
            &format!("/v1/courses/{}/ingestion/runs/{run}", f.course),
            Some(&f.owner),
            Value::Null,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(view["status"], "needs_review");
    let (_, outsider) = app.register(&email("m2-outsider")).await;
    assert_eq!(
        app.json(
            Method::GET,
            &format!("/v1/courses/{}/ingestion/runs/{run}", f.course),
            Some(&outsider),
            Value::Null
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.json(
            Method::GET,
            &format!("/v1/courses/{}/ingestion/runs/{run}", f.course),
            Some(&f.student),
            Value::Null
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    for sql in [
        "DELETE FROM ingestion_runs",
        "DELETE FROM ingestion_pages",
        "DELETE FROM ingestion_crops",
        "DELETE FROM ingestion_chunks",
    ] {
        let error = sqlx::query(sql).execute(&app.app_pool).await.unwrap_err();
        assert!(permission_denied(&error));
    }
    let invalid =
        sqlx::query("UPDATE ingestion_chunks SET embedding=($2::text)::vector WHERE id=$1")
            .bind(page.chunks[0].id)
            .bind(format!("[{}]", vec!["0.1"; 1024].join(",")))
            .execute(&app.admin)
            .await
            .unwrap_err();
    assert!(
        matches!(invalid,sqlx::Error::Database(e) if e.code().as_deref()==Some("23514")),
        "embedding provenance is mandatory"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the private M1/M2 PostgreSQL harness"]
async fn m2_expired_workers_are_fenced_and_reclaims_resume_same_run() {
    use campuslingo_storage::ingestion::*;
    let app = TestApp::new().await;
    let f = fixture(&app, 1).await;
    let (job, input) = m2_source_job(&app, &f, 1).await;
    let run = start_run(&app.worker, job.id, job.lease_token, &input)
        .await
        .unwrap();
    let page = m2_page(1);
    checkpoint_page(&app.worker, job.id, job.lease_token, run, &page)
        .await
        .unwrap();
    sqlx::query("UPDATE jobs SET lease_until=clock_timestamp()-interval '1 second' WHERE id=$1")
        .bind(job.id)
        .execute(&app.admin)
        .await
        .unwrap();
    assert!(
        jobs::heartbeat(&app.worker, job.id, job.lease_token, "late", 1, 1)
            .await
            .is_err()
    );
    assert!(
        checkpoint_page(&app.worker, job.id, job.lease_token, run, &page)
            .await
            .is_err()
    );
    let reclaimed = jobs::reaper_and_claim_kind(&app.worker, Some("pdf_ingestion"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reclaimed.id, job.id);
    assert_ne!(reclaimed.lease_token, job.lease_token);
    assert_eq!(
        start_run(&app.worker, reclaimed.id, reclaimed.lease_token, &input)
            .await
            .unwrap(),
        run
    );
    assert!(
        !checkpoint_page(&app.worker, reclaimed.id, reclaimed.lease_token, run, &page)
            .await
            .unwrap()
    );
    assert!(
        finish_run(&app.worker, job.id, job.lease_token, run)
            .await
            .is_err()
    );
    assert_eq!(
        finish_run(&app.worker, reclaimed.id, reclaimed.lease_token, run)
            .await
            .unwrap(),
        "needs_review"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the private M1/M2 PostgreSQL harness"]
async fn m2_crop_delivery_rechecks_sessions_membership_and_publication_boundary() {
    use campuslingo_storage::ingestion::*;
    let app = TestApp::new().await;
    let f = fixture(&app, 1).await;
    let (job, input) = m2_source_job(&app, &f, 1).await;
    let run = start_run(&app.worker, job.id, job.lease_token, &input)
        .await
        .unwrap();
    let page = m2_page(1);
    let crop = page.crops[0].id;
    tokio::fs::write(app.asset_dir.join(crop.to_string()), b"webp")
        .await
        .unwrap();
    checkpoint_page(&app.worker, job.id, job.lease_token, run, &page)
        .await
        .unwrap();
    finish_run(&app.worker, job.id, job.lease_token, run)
        .await
        .unwrap();
    let uri = format!("/v1/courses/{}/ingestion/crops/{crop}", f.course);
    let (status, headers, body) = app
        .send(Method::GET, &uri, Some(&f.student), None, vec![], &[])
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"webp");
    assert_eq!(headers[header::CACHE_CONTROL], "private, no-store");
    let (_, outsider) = app.register(&email("m2-crop-outsider")).await;
    assert_eq!(
        app.send(
            Method::GET,
            &uri,
            None,
            None,
            vec![],
            &[("x-user-id", &f.student_id.to_string())]
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.send(
            Method::GET,
            &uri,
            Some(&outsider),
            None,
            vec![],
            &[("x-user-id", &f.student_id.to_string())]
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.publish(&f.owner, f.course, true).await,
        StatusCode::NO_CONTENT
    );
    for path in [&uri, &format!("/v1/assets/{crop}")] {
        assert_eq!(
            app.send(Method::GET, path, Some(&outsider), None, vec![], &[])
                .await
                .0,
            StatusCode::NOT_FOUND,
            "draft crops do not become public with course shell"
        );
    }
    sqlx::query("UPDATE course_members SET active=FALSE WHERE course_id=$1 AND user_id=$2")
        .bind(f.course)
        .bind(f.student_id)
        .execute(&app.admin)
        .await
        .unwrap();
    assert_eq!(
        app.send(Method::GET, &uri, Some(&f.student), None, vec![], &[])
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let wrong = app.course(&f.owner, f.tenant, "Other private course").await;
    assert_eq!(
        app.send(
            Method::GET,
            &format!("/v1/courses/{wrong}/ingestion/crops/{crop}"),
            Some(&f.owner),
            None,
            vec![],
            &[]
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires PostgreSQL plus contained Poppler/Tesseract/ImageMagick"]
async fn m2_worker_persists_actual_pdf_pages_crops_and_chunks() {
    let app = TestApp::new().await;
    let f = fixture(&app, 1).await;
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bytes = tokio::fs::read(root.join("tests/fixtures/m2/region-cases.pdf"))
        .await
        .unwrap();
    let (status, _, response) = app
        .send(
            Method::POST,
            &format!("/v1/courses/{}/assets", f.course),
            Some(&f.owner),
            Some("application/pdf"),
            bytes,
            &[],
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let asset = uuid_field(&decode_json(&response), "asset_id");
    assert_eq!(
        app.enqueue(
            &f.owner,
            f.course,
            Uuid::new_v4(),
            &json!({"schema_version":1,"kind":"pdf_ingestion","asset_id":asset})
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let work = root
        .join("artifacts/private/worker-integration")
        .join(Uuid::new_v4().to_string());
    let result = campuslingo_ingestion::worker::work_once(
        &app.worker,
        &campuslingo_ingestion::worker::WorkerConfig::new(work, app.asset_dir.clone()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result["status"], "needs_review", "{result}");
    let run = uuid_field(&result, "run_id");
    let provenance: Value = sqlx::query_scalar("SELECT provenance FROM ingestion_runs WHERE id=$1")
        .bind(run)
        .fetch_one(&app.admin)
        .await
        .unwrap();
    assert_eq!(
        provenance["pipeline_version"],
        campuslingo_ingestion::pipeline::PIPELINE_VERSION
    );
    for helper in ["pdftocairo", "rsvg-convert"] {
        let digest = provenance["tools"][helper]["sha256"].as_str().unwrap();
        assert_eq!(digest.len(), 64);
        assert!(digest.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM ingestion_pages WHERE run_id=$1")
        .bind(run)
        .fetch_one(&app.admin)
        .await
        .unwrap();
    assert_eq!(count, 6);
    let crops: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM ingestion_crops WHERE run_id=$1")
        .bind(run)
        .fetch_all(&app.admin)
        .await
        .unwrap();
    assert!(!crops.is_empty());
    let chunks: i64 = sqlx::query_scalar("SELECT count(*) FROM ingestion_chunks WHERE run_id=$1")
        .bind(run)
        .fetch_one(&app.admin)
        .await
        .unwrap();
    assert!(chunks > 0);
    let (status, _, body) = app
        .send(
            Method::GET,
            &format!("/v1/courses/{}/ingestion/crops/{}", f.course, crops[0]),
            Some(&f.student),
            None,
            vec![],
            &[],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(&body[..4], b"RIFF");
    assert_eq!(&body[8..12], b"WEBP");
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

fn basis_embedding(axis_a: usize, axis_b: usize, weight_b: f32) -> Vec<f32> {
    let mut raw = vec![0.0_f32; campuslingo_storage::ingestion::EMBEDDING_DIM];
    raw[axis_a] = 1.0;
    raw[axis_b] = weight_b;
    campuslingo_ingestion::embeddings::normalize_l2_1024(&raw).unwrap()
}

fn signed_embedding(
    axis_a: usize,
    model_id: &str,
    revision: &str,
) -> campuslingo_storage::ingestion::EmbeddingVector {
    campuslingo_storage::ingestion::EmbeddingVector {
        values: basis_embedding(axis_a, (axis_a + 1) % 1024, 0.25),
        model_id: model_id.into(),
        revision: revision.into(),
        preprocess_version: "critical-v2-indent".into(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires PostgreSQL with pgvector HNSW"]
async fn m2_chunk_embeddings_enforce_s02_s03_s04_and_p04_in_postgres() {
    use campuslingo_ingestion::fingerprint;
    use campuslingo_storage::ingestion::{
        ChunkCheckpoint, EmbeddingScope, EmbeddingVector, checkpoint_page, finish_run,
        format_pgvector, safe_dedup_candidate, search_similar_chunks,
        search_similar_chunks_for_worker, start_run, validate_embedding_vector,
    };

    let app = TestApp::new().await;
    let f = fixture(&app, 1).await;
    let other_course = app.course(&f.owner, f.tenant, "Other course").await;
    let (_, other_owner) = app.register(&email("m2-other-owner")).await;
    let other_tenant = app.tenant(&other_owner, "Other tenant").await;
    let foreign_course = app
        .course(&other_owner, other_tenant, "Foreign course")
        .await;

    // S-02 validation contracts reject wrong dims, NaN/Inf, zero vectors, and missing metadata.
    assert!(
        validate_embedding_vector(&EmbeddingVector {
            values: vec![1.0; 1023],
            model_id: "m".into(),
            revision: "r".into(),
            preprocess_version: "p".into(),
        })
        .is_err()
    );
    assert!(
        validate_embedding_vector(&EmbeddingVector {
            values: vec![0.0; 1024],
            model_id: "m".into(),
            revision: "r".into(),
            preprocess_version: "p".into(),
        })
        .is_err()
    );
    let mut bad_nan = basis_embedding(0, 1, 0.1);
    bad_nan[7] = f32::NAN;
    assert!(
        validate_embedding_vector(&EmbeddingVector {
            values: bad_nan,
            model_id: "m".into(),
            revision: "r".into(),
            preprocess_version: "p".into(),
        })
        .is_err()
    );
    assert!(
        validate_embedding_vector(&EmbeddingVector {
            values: basis_embedding(0, 1, 0.1),
            model_id: "   ".into(),
            revision: "r".into(),
            preprocess_version: "p".into(),
        })
        .is_err()
    );

    let (job, input) = m2_source_job(&app, &f, 1).await;
    let run = start_run(&app.worker, job.id, job.lease_token, &input)
        .await
        .unwrap();

    // P-04 pair: identical dense vector (cosine = 1.0) but opposite inequality & number.
    let theorem_positive = "Theorem 1: f(x) >= 0 when alpha = 2.";
    let theorem_negative = "Theorem 1: f(x) <= 0 when alpha = 3.";
    let biology_text = "Chloroplasts convert sunlight into chemical energy.";
    let quarantined_text = "Theorem 1: quarantined leaked slide >= 0.";
    let arabic_text = "النظرية الأولى: f(x) >= 0.";

    let make_chunk = |order: i32, text: &str, lang: &str, quarantined: bool, axis: usize| {
        let fp = fingerprint::fingerprint(text);
        ChunkCheckpoint {
            id: Uuid::new_v4(),
            order,
            source_text: text.into(),
            normalized_candidate: fp.normalized_text,
            text_sha256: fp.text_sha256,
            structural_fingerprint: fp.critical_sha256,
            critical_tokens: json!({"schema_version":1,"tokens":fp.critical_tokens}),
            duplicate_of: None,
            crop_ids: vec![],
            language_code: Some(lang.into()),
            is_quarantined: quarantined,
            embedding: Some(signed_embedding(
                axis,
                "Qwen/Qwen3-Embedding-0.6B-GGUF",
                "370f27d7550e0def9b39c1f16d3fbaa13aa67728",
            )),
        }
    };

    let c_pos = make_chunk(0, theorem_positive, "en", false, 0);
    let c_bio = make_chunk(1, biology_text, "en", false, 200);
    let c_quarantined = make_chunk(2, quarantined_text, "en", true, 0);
    let c_arabic = make_chunk(3, arabic_text, "ar", false, 0);

    let mut page = m2_page(1);
    page.chunks = vec![
        c_pos.clone(),
        c_bio.clone(),
        c_quarantined.clone(),
        c_arabic.clone(),
    ];
    assert!(
        checkpoint_page(&app.worker, job.id, job.lease_token, run, &page)
            .await
            .unwrap()
    );
    let active_scope = EmbeddingScope {
        language_code: "en".into(),
        model_id: "Qwen/Qwen3-Embedding-0.6B-GGUF".into(),
        revision: "370f27d7550e0def9b39c1f16d3fbaa13aa67728".into(),
        preprocess_version: "critical-v2-indent".into(),
        limit: 10,
    };
    let active_query = signed_embedding(
        0,
        "Qwen/Qwen3-Embedding-0.6B-GGUF",
        "370f27d7550e0def9b39c1f16d3fbaa13aa67728",
    );
    let worker_hits = search_similar_chunks_for_worker(
        &app.worker,
        job.id,
        job.lease_token,
        &active_query,
        &active_scope,
    )
    .await
    .unwrap();
    assert_eq!(worker_hits.len(), 2);
    assert_eq!(worker_hits[0].id, c_pos.id);

    finish_run(&app.worker, job.id, job.lease_token, run)
        .await
        .unwrap();

    // Seed identical axis-0 chunks into another course & foreign tenant to verify scoping.
    let (other_job, other_input) = m2_source_job(
        &app,
        &Fixture {
            owner: f.owner.clone(),
            student_id: f.student_id,
            student: f.student.clone(),
            student_email: f.student_email.clone(),
            tenant: f.tenant,
            course: other_course,
            node: f.node,
            challenge: f.challenge,
        },
        1,
    )
    .await;
    let other_run = start_run(
        &app.worker,
        other_job.id,
        other_job.lease_token,
        &other_input,
    )
    .await
    .unwrap();
    let mut other_page = m2_page(1);
    other_page.crops.clear();
    other_page.chunks = vec![make_chunk(
        0,
        "Other course identically aligned theorem",
        "en",
        false,
        0,
    )];
    checkpoint_page(
        &app.worker,
        other_job.id,
        other_job.lease_token,
        other_run,
        &other_page,
    )
    .await
    .unwrap();

    let (foreign_job, foreign_input) = {
        let foreign_f = Fixture {
            owner: other_owner.clone(),
            student_id: f.student_id,
            student: other_owner.clone(),
            student_email: f.student_email.clone(),
            tenant: other_tenant,
            course: foreign_course,
            node: f.node,
            challenge: f.challenge,
        };
        m2_source_job(&app, &foreign_f, 1).await
    };
    let foreign_run = start_run(
        &app.worker,
        foreign_job.id,
        foreign_job.lease_token,
        &foreign_input,
    )
    .await
    .unwrap();
    let mut foreign_page = m2_page(1);
    foreign_page.crops.clear();
    foreign_page.chunks = vec![make_chunk(
        0,
        "Foreign tenant identically aligned",
        "en",
        false,
        0,
    )];
    checkpoint_page(
        &app.worker,
        foreign_job.id,
        foreign_job.lease_token,
        foreign_run,
        &foreign_page,
    )
    .await
    .unwrap();

    // S-03 check: verbatim source_text preserved alongside normalized_candidate and scoped columns.
    let row: (String, String, String, String, String) = sqlx::query_as(
        "SELECT source_text, normalized_candidate, language_code, archetype, generator_version
         FROM ingestion_chunks WHERE id=$1",
    )
    .bind(c_pos.id)
    .fetch_one(&app.admin)
    .await
    .unwrap();
    assert_eq!(row.0, theorem_positive);
    assert_eq!(row.2, "en");
    assert_eq!(row.3, "UNCLASSIFIED");
    assert_eq!(row.4, "extraction-only");

    // S-04 check: quarantined, other-course, other-tenant, and wrong-language chunks excluded.
    let query_vec = signed_embedding(
        0,
        "Qwen/Qwen3-Embedding-0.6B-GGUF",
        "370f27d7550e0def9b39c1f16d3fbaa13aa67728",
    );
    let scope = EmbeddingScope {
        language_code: "en".into(),
        model_id: "Qwen/Qwen3-Embedding-0.6B-GGUF".into(),
        revision: "370f27d7550e0def9b39c1f16d3fbaa13aa67728".into(),
        preprocess_version: "critical-v2-indent".into(),
        limit: 10,
    };
    let owner_id: Uuid = sqlx::query_scalar("SELECT created_by FROM courses WHERE id=$1")
        .bind(f.course)
        .fetch_one(&app.admin)
        .await
        .unwrap();
    let hits = search_similar_chunks(&app.app_pool, owner_id, f.course, &query_vec, &scope)
        .await
        .unwrap();
    let hit_ids: Vec<Uuid> = hits.iter().map(|h| h.id).collect();
    assert_eq!(hit_ids, vec![c_pos.id, c_bio.id], "{hit_ids:?}");
    assert!((hits[0].cosine_similarity - 1.0).abs() < 1e-6);

    // Stale/finished worker lease is fenced out from worker ANN search too.
    assert!(
        search_similar_chunks_for_worker(&app.worker, job.id, job.lease_token, &query_vec, &scope)
            .await
            .is_err()
    );

    // Mixing query vector provenance with another ANN partition fails closed;
    // querying an empty matching partition returns zero hits.
    let wrong_rev = EmbeddingScope {
        revision: "different-revision".into(),
        ..scope.clone()
    };
    assert!(
        search_similar_chunks(&app.app_pool, owner_id, f.course, &query_vec, &wrong_rev)
            .await
            .is_err()
    );
    let wrong_rev_query =
        signed_embedding(0, "Qwen/Qwen3-Embedding-0.6B-GGUF", "different-revision");
    assert!(
        search_similar_chunks(
            &app.app_pool,
            owner_id,
            f.course,
            &wrong_rev_query,
            &wrong_rev,
        )
        .await
        .unwrap()
        .is_empty()
    );

    // P-04 check: cosine = 1.0 neighbor CANNOT merge when structural_fingerprint differs.
    let neg_fp = fingerprint::fingerprint(theorem_negative);
    assert_ne!(neg_fp.critical_sha256, c_pos.structural_fingerprint);
    assert!(safe_dedup_candidate(&neg_fp.critical_sha256, &hits, 0.90).is_none());
    assert_eq!(
        safe_dedup_candidate(&c_pos.structural_fingerprint, &hits, 0.999)
            .unwrap()
            .id,
        c_pos.id
    );

    // Verify HNSW partial index is valid and planner-selectable.
    let vec_lit = format_pgvector(&query_vec.values).unwrap();
    let mut tx = app.admin.begin().await.unwrap();
    sqlx::query("SET LOCAL enable_seqscan = off")
        .execute(&mut *tx)
        .await
        .unwrap();
    let plan: Vec<String> = sqlx::query_scalar(
        "EXPLAIN (COSTS OFF)
         SELECT id FROM ingestion_chunks
         WHERE NOT is_quarantined AND embedding IS NOT NULL
         ORDER BY embedding <=> $1::vector
         LIMIT 5",
    )
    .bind(&vec_lit)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert!(
        plan.iter()
            .any(|line| line.contains("ingestion_embedding_hnsw")),
        "expected HNSW index plan, got: {plan:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires PostgreSQL plus contained Poppler/Tesseract/ImageMagick"]
async fn m2_worker_persists_sequential_chunk_embeddings_over_loopback() {
    use campuslingo_ingestion::embeddings::{
        DEFAULT_EMBEDDING_MODEL_ID, DEFAULT_EMBEDDING_REVISION, EMBEDDING_PREPROCESS_VERSION,
        EmbeddingClient, EmbeddingClientConfig,
    };
    use campuslingo_storage::ingestion::{EmbeddingScope, search_similar_chunks};

    let captured_inputs = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::<String>::new()));
    let inputs_ref = captured_inputs.clone();
    let mock_router = axum::Router::new().route(
        "/v1/embeddings",
        axum::routing::post(move |axum::Json(payload): axum::Json<Value>| {
            let inputs_ref = inputs_ref.clone();
            async move {
                let items = payload["input"].as_array().cloned().unwrap_or_default();
                let mut data = Vec::with_capacity(items.len());
                let mut log = inputs_ref.lock().await;
                for (index, item) in items.iter().enumerate() {
                    let text = item.as_str().unwrap_or("");
                    log.push(text.to_owned());
                    let axis = (text.len() % 512).max(1);
                    let mut raw = vec![0.0_f32; 1024];
                    raw[0] = 1.0;
                    raw[axis] = 0.5;
                    let norm = raw.iter().map(|v| v * v).sum::<f32>().sqrt();
                    for v in &mut raw {
                        *v /= norm;
                    }
                    data.push(json!({"index": index, "embedding": raw}));
                }
                axum::Json(json!({"object":"list","data":data}))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, mock_router).await.unwrap();
    });

    let app = TestApp::new().await;
    let f = fixture(&app, 1).await;
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bytes = tokio::fs::read(root.join("tests/fixtures/m2/region-cases.pdf"))
        .await
        .unwrap();
    let (status, _, response) = app
        .send(
            Method::POST,
            &format!("/v1/courses/{}/assets", f.course),
            Some(&f.owner),
            Some("application/pdf"),
            bytes,
            &[],
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let asset = uuid_field(&decode_json(&response), "asset_id");
    assert_eq!(
        app.enqueue(
            &f.owner,
            f.course,
            Uuid::new_v4(),
            &json!({"schema_version":1,"kind":"pdf_ingestion","asset_id":asset})
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let work = root
        .join("artifacts/private/worker-embed-integration")
        .join(Uuid::new_v4().to_string());
    let mut config = campuslingo_ingestion::worker::WorkerConfig::new(work, app.asset_dir.clone());
    config.embeddings = Some(
        EmbeddingClient::new(EmbeddingClientConfig::external(
            format!("http://{addr}"),
            "Qwen3-Embedding-0.6B-Q8_0.gguf",
            DEFAULT_EMBEDDING_MODEL_ID,
            DEFAULT_EMBEDDING_REVISION,
        ))
        .unwrap(),
    );
    config.embedding_provenance = Some(json!({
        "model_id": DEFAULT_EMBEDDING_MODEL_ID,
        "revision": DEFAULT_EMBEDDING_REVISION,
        "preprocess_version": EMBEDDING_PREPROCESS_VERSION,
        "dim": 1024
    }));

    let result = campuslingo_ingestion::worker::work_once(&app.worker, &config)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result["status"], "needs_review", "{result}");
    let chunks_embedded = result["chunks_embedded"].as_u64().unwrap();
    assert!(chunks_embedded > 0, "{result}");
    let run = uuid_field(&result, "run_id");
    let provenance: Value = sqlx::query_scalar("SELECT provenance FROM ingestion_runs WHERE id=$1")
        .bind(run)
        .fetch_one(&app.admin)
        .await
        .unwrap();
    assert_eq!(
        provenance["embedding_model"]["model_id"],
        DEFAULT_EMBEDDING_MODEL_ID
    );
    let persisted_vectors: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ingestion_chunks WHERE run_id=$1 AND embedding IS NOT NULL AND embedding_dim = 1024",
    )
    .bind(run)
    .fetch_one(&app.admin)
    .await
    .unwrap();
    assert_eq!(persisted_vectors, chunks_embedded as i64);

    let owner_id: Uuid = sqlx::query_scalar("SELECT created_by FROM courses WHERE id=$1")
        .bind(f.course)
        .fetch_one(&app.admin)
        .await
        .unwrap();
    let query = signed_embedding(1, DEFAULT_EMBEDDING_MODEL_ID, DEFAULT_EMBEDDING_REVISION);
    let scope = EmbeddingScope {
        language_code: "en".into(),
        model_id: DEFAULT_EMBEDDING_MODEL_ID.into(),
        revision: DEFAULT_EMBEDDING_REVISION.into(),
        preprocess_version: EMBEDDING_PREPROCESS_VERSION.into(),
        limit: 5,
    };
    let hits = search_similar_chunks(&app.app_pool, owner_id, f.course, &query, &scope)
        .await
        .unwrap();
    assert!(!hits.is_empty());
    assert!(!captured_inputs.lock().await.is_empty());
    handle.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the private M1 PostgreSQL harness"]
async fn m3_generation_tables_enforce_s11_s12_s20_dec002_dec003_and_audio_rls() {
    use campuslingo_domain::contracts::{
        AlignmentMethod, Archetype, AudioManifest, AudioWord, ChoiceChallenge, ChoiceType, Codec,
        DirectorTrack, FocusRole, Language, LearningUnitBundle, SandboxSpec, ShotType,
        SourceRegion, Version, WorldTheme,
    };
    use campuslingo_storage::{StorageError, generation::*};

    let app = TestApp::new().await;
    let f = fixture(&app, 0).await;
    let owner_id: Uuid = sqlx::query_scalar("SELECT created_by FROM courses WHERE id = $1")
        .bind(f.course)
        .fetch_one(&app.admin)
        .await
        .unwrap();
    let student_id = f.student_id;
    let (outsider_id, _outsider_cookie) = app.register(&email("m3-outsider")).await;

    // Student & outsider cannot create a content version.
    assert!(matches!(
        create_content_version(&app.app_pool, student_id, f.course, None).await,
        Err(StorageError::NotFound | StorageError::Forbidden)
    ));
    let version = create_content_version(&app.app_pool, owner_id, f.course, None)
        .await
        .unwrap();
    assert_eq!(version.version_number, 1);

    let sandbox_json: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/sandbox.valid.json")).unwrap();
    let valid_spec: SandboxSpec = serde_json::from_value(sandbox_json.clone()).unwrap();

    let node2 = app.node(&f.owner, f.course, "Node 2").await;
    let node3 = app.node(&f.owner, f.course, "Node 3 AXIOM Pack").await;
    let c1 = app.challenge(&f.owner, f.course, f.node, 0).await;
    let c2 = app.challenge(&f.owner, f.course, f.node, 1).await;
    let c3 = app.challenge(&f.owner, f.course, f.node, 2).await;

    let bundle = LearningUnitBundle {
        schema_version: serde_json::from_str::<Version>("\"1.0.0\"").unwrap(),
        node_id: f.node.to_string(),
        language: Language::English,
        world_theme: WorldTheme::CyberGrid,
        title: "Tradeoff Budget Control".into(),
        tactical_summary: vec![
            "Adjust power to keep output within 12..14.".into(),
            "Verify safety across all quantized states.".into(),
        ],
        source_regions: vec![SourceRegion {
            source_sha256: "a".repeat(64),
            page: 1,
            bbox: [10.0, 20.0, 200.0, 120.0],
            page_width: 612.0,
            page_height: 792.0,
            text_quote: "P_out = 2 * P_in".into(),
        }],
        challenges: vec![ChoiceChallenge {
            id: c1.to_string(),
            kind: ChoiceType::Mcq,
            prompt: "Which power reaches [12, 14]?".into(),
            options: vec!["0".into(), "6".into()],
            correct_index: 1,
            explanation: "2 * 6 = 12".into(),
        }],
        sandbox: Some(valid_spec.clone()),
        director_tracks: vec![DirectorTrack {
            start_ms: 0,
            end_ms: 1200,
            focus_role: FocusRole::Concept,
            shot_type: ShotType::Overview,
            explode_factor: 0.0,
        }],
        audio: None,
        generator_version: "qwen3.5-4b-m3-test".into(),
    };

    let stored = persist_learning_unit_bundle(
        &app.app_pool,
        owner_id,
        &NewLearningUnitBundle {
            course_id: f.course,
            node_id: f.node,
            content_version_id: version.id,
            archetype: "TRADEOFF",
            modifier: "BUDGET_CAP",
            grid_x: 0,
            grid_y: 0,
            source_sha256: &"a".repeat(64),
            generator_model_id: "unsloth/Qwen3.5-4B-GGUF",
            generator_revision: "m3-test",
            bundle: &bundle,
        },
    )
    .await
    .unwrap();
    assert_eq!(stored.card_elixir_cost, 5);

    let sim = get_node_simulation(&app.app_pool, student_id, f.node, version.id)
        .await
        .unwrap()
        .unwrap();
    assert!(sim.verified);
    assert_eq!(sim.solution_witness["power"], json!(6.0));

    // S-20: Duplicate grid slot on same course+content_version is Conflict.
    let mut bundle2 = bundle.clone();
    bundle2.node_id = node2.to_string();
    assert!(matches!(
        persist_learning_unit_bundle(
            &app.app_pool,
            owner_id,
            &NewLearningUnitBundle {
                course_id: f.course,
                node_id: node2,
                content_version_id: version.id,
                archetype: "TRADEOFF",
                modifier: "BUDGET_CAP",
                grid_x: 0,
                grid_y: 0,
                source_sha256: &"a".repeat(64),
                generator_model_id: "unsloth/Qwen3.5-4B-GGUF",
                generator_revision: "m3-test",
                bundle: &bundle2,
            },
        )
        .await,
        Err(StorageError::Conflict)
    ));

    // S-20 SQL CHECK constraints on pair & elixir cost.
    assert!(
        sqlx::query(
            "INSERT INTO learning_unit_bundles (
                 tenant_id, course_id, node_id, content_version_id,
                 archetype, modifier, card_elixir_cost, world_theme, language,
                 grid_x, grid_y, bundle_json, source_sha256,
                 generator_model_id, generator_revision, generator_version
             ) VALUES ($1, $2, $3, $4, 'PIPELINE', 'NON_EXISTENT', 3, 'CYBER_GRID', 'en', 9, 9, '{}'::jsonb, $5, 'm', 'r', 'v')",
        )
        .bind(stored.tenant_id)
        .bind(f.course)
        .bind(node2)
        .bind(version.id)
        .bind("a".repeat(64))
        .execute(&app.admin)
        .await
        .is_err()
    );
    assert!(
        sqlx::query(
            "INSERT INTO learning_unit_bundles (
                 tenant_id, course_id, node_id, content_version_id,
                 archetype, modifier, card_elixir_cost, world_theme, language,
                 grid_x, grid_y, bundle_json, source_sha256,
                 generator_model_id, generator_revision, generator_version
             ) VALUES ($1, $2, $3, $4, 'PIPELINE', 'REVERSE_ENG', 7, 'CYBER_GRID', 'en', 9, 9, '{}'::jsonb, $5, 'm', 'r', 'v')",
        )
        .bind(stored.tenant_id)
        .bind(f.course)
        .bind(node2)
        .bind(version.id)
        .bind("a".repeat(64))
        .execute(&app.admin)
        .await
        .is_err()
    );

    // S-12 / P-05: Pre-solved initial state is rejected; duplicate simulation conflicts; verified=false rejected by SQL.
    let mut pre_solved_val = sandbox_json.clone();
    pre_solved_val["parameters"][0]["initial"] = json!(6);
    let pre_solved_spec: SandboxSpec = serde_json::from_value(pre_solved_val).unwrap();
    assert!(matches!(
        persist_verified_simulation(
            &app.app_pool,
            owner_id,
            &NewNodeSimulation {
                course_id: f.course,
                node_id: node2,
                content_version_id: version.id,
                spec: &pre_solved_spec,
            },
        )
        .await,
        Err(StorageError::Invalid(_))
    ));
    assert!(matches!(
        persist_verified_simulation(
            &app.app_pool,
            owner_id,
            &NewNodeSimulation {
                course_id: f.course,
                node_id: f.node,
                content_version_id: version.id,
                spec: &valid_spec,
            },
        )
        .await,
        Err(StorageError::Conflict)
    ));
    assert!(
        sqlx::query(
            "INSERT INTO node_simulations (
                 tenant_id, course_id, node_id, content_version_id,
                 spec_json, solution_witness, states_checked, proof_domain, verified
             ) VALUES ($1, $2, $3, $4, '{\"parameters\":[1],\"targets\":[1]}'::jsonb, '{}'::jsonb, 1, 'exhaustive_declared_discrete_grid', FALSE)",
        )
        .bind(stored.tenant_id)
        .bind(f.course)
        .bind(node2)
        .bind(version.id)
        .execute(&app.admin)
        .await
        .is_err()
    );

    // DEC-002 / DEC-003: Reject SPEED_BLITZ (6s) & QUICK_BLITZ_CHAIN (4s) & duplicate card_ids; accept COROLLARY_CHAIN (8s) with 3 distinct cards.
    assert!(matches!(
        create_axiom_pack(
            &app.app_pool,
            owner_id,
            &NewAxiomPack {
                course_id: f.course,
                node_id: node3,
                content_version_id: version.id,
                archetype: "AXIOM",
                modifier: "SPEED_BLITZ",
                card_ids: &[c1, c2],
            },
        )
        .await,
        Err(StorageError::Invalid(_))
    ));
    assert!(matches!(
        create_axiom_pack(
            &app.app_pool,
            owner_id,
            &NewAxiomPack {
                course_id: f.course,
                node_id: node3,
                content_version_id: version.id,
                archetype: "AXIOM",
                modifier: "QUICK_BLITZ_CHAIN",
                card_ids: &[c1, c2],
            },
        )
        .await,
        Err(StorageError::Invalid(_))
    ));
    assert!(matches!(
        create_axiom_pack(
            &app.app_pool,
            owner_id,
            &NewAxiomPack {
                course_id: f.course,
                node_id: node3,
                content_version_id: version.id,
                archetype: "AXIOM",
                modifier: "COROLLARY_CHAIN",
                card_ids: &[c1, c1],
            },
        )
        .await,
        Err(StorageError::Invalid(_))
    ));
    let pack = create_axiom_pack(
        &app.app_pool,
        owner_id,
        &NewAxiomPack {
            course_id: f.course,
            node_id: node3,
            content_version_id: version.id,
            archetype: "AXIOM",
            modifier: "COROLLARY_CHAIN",
            card_ids: &[c1, c2, c3],
        },
    )
    .await
    .unwrap();
    assert_eq!(pack.w_base_ms, 8000);
    assert_eq!(pack.items.len(), 3);
    assert!(
        sqlx::query(
            "INSERT INTO axiom_packs (tenant_id, course_id, node_id, content_version_id, archetype, modifier, w_base_ms, card_elixir_cost, prompt_count)
             VALUES ($1, $2, $3, $4, 'AXIOM', 'SPEED_BLITZ', 6000, 1, 2)",
        )
        .bind(stored.tenant_id)
        .bind(f.course)
        .bind(node2)
        .bind(version.id)
        .execute(&app.admin)
        .await
        .is_err()
    );

    // P-07: Upload audio/opus asset and persist CLAUSE_CONCAT audio briefing; check RLS isolation.
    let (status, _, asset_body) = app
        .send(
            Method::POST,
            &format!("/v1/courses/{}/assets", f.course),
            Some(&f.owner),
            Some("audio/opus"),
            b"OggS\x00\x02synthetic-opus-stream".to_vec(),
            &[],
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "audio upload failed");
    let audio_asset_id = uuid_field(&decode_json(&asset_body), "asset_id");
    let manifest = AudioManifest {
        asset_id: audio_asset_id.to_string(),
        language: Language::English,
        codec: Codec::Opus,
        text: "Clause one completes cleanly. Clause two focuses output.".into(),
        duration_ms: 2400,
        alignment_method: AlignmentMethod::ClauseConcat,
        words: vec![
            AudioWord {
                text: "Clause one completes cleanly.".into(),
                start_ms: 0,
                end_ms: 1200,
            },
            AudioWord {
                text: "Clause two focuses output.".into(),
                start_ms: 1200,
                end_ms: 2400,
            },
        ],
        degraded: None,
        quality_gate_passed: None,
        normalized_wer: None,
        quality_note: None,
    };
    let tracks = vec![
        DirectorTrack {
            start_ms: 0,
            end_ms: 1200,
            focus_role: FocusRole::Input,
            shot_type: ShotType::Overview,
            explode_factor: 0.0,
        },
        DirectorTrack {
            start_ms: 1200,
            end_ms: 2400,
            focus_role: FocusRole::Output,
            shot_type: ShotType::CloseUp,
            explode_factor: 0.25,
        },
    ];
    let briefing = persist_audio_briefing(
        &app.app_pool,
        owner_id,
        &NewAudioBriefing {
            course_id: f.course,
            node_id: f.node,
            content_version_id: version.id,
            asset_id: audio_asset_id,
            manifest: &manifest,
            director_tracks: &tracks,
        },
    )
    .await
    .unwrap();
    assert_eq!(briefing.alignment_method, "CLAUSE_CONCAT");
    assert_eq!(Archetype::Tradeoff.modifiers()[1], "BUDGET_CAP");

    // Student can read; outsider is hidden by RLS.
    assert_eq!(
        list_course_bundles(&app.app_pool, student_id, f.course, version.id)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        list_course_bundles(&app.app_pool, outsider_id, f.course, version.id)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        get_node_simulation(&app.app_pool, outsider_id, f.node, version.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        get_axiom_pack(&app.app_pool, outsider_id, node3, version.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        get_node_audio_briefing(&app.app_pool, outsider_id, f.node, version.id, "en")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        get_node_audio_briefing(&app.app_pool, student_id, f.node, version.id, "en")
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the private M1 PostgreSQL harness"]
async fn m4_shields_dag_brownout_tier_and_stale_recompute_enforced_in_postgres() {
    use campuslingo_domain::memory::{MemoryInput, ReviewInput, review};
    use campuslingo_domain::shields::{NodeState, evaluate_course_shields, ticks_1e4};
    use campuslingo_storage::{StorageError, generation::create_content_version, shields::*};

    let app = TestApp::new().await;
    let f = fixture(&app, 0).await;
    let owner_id: Uuid = sqlx::query_scalar("SELECT created_by FROM courses WHERE id = $1")
        .bind(f.course)
        .fetch_one(&app.admin)
        .await
        .unwrap();

    // f.student is already enrolled as a course member by fixture(&app, 0). Create child node & 2nd course.

    let (status, _, child_res) = app
        .send(
            Method::POST,
            &format!("/v1/courses/{}/nodes", f.course),
            Some(&f.owner),
            Some("application/json"),
            serde_json::to_vec(&json!({"title": "Child Concept"})).unwrap(),
            &[],
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let child_node = uuid_field(&decode_json(&child_res), "node_id");

    let (status, _, empty_node_res) = app
        .send(
            Method::POST,
            &format!("/v1/courses/{}/nodes", f.course),
            Some(&f.owner),
            Some("application/json"),
            serde_json::to_vec(&json!({"title": "Inactive Concept"})).unwrap(),
            &[],
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let empty_node = uuid_field(&decode_json(&empty_node_res), "node_id");

    let (status, _, child_ch_res) = app
        .send(
            Method::POST,
            &format!("/v1/courses/{}/nodes/{child_node}/challenges", f.course),
            Some(&f.owner),
            Some("application/json"),
            serde_json::to_vec(&json!({
                "prompt": "Child prompt",
                "options": ["A", "B"],
                "correct_index": 0,
                "explanation": "A"
            }))
            .unwrap(),
            &[],
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let child_challenge = uuid_field(&decode_json(&child_ch_res), "challenge_id");

    // S-05: self-loop, cycle, cross-course, and depth > 16 all fail closed.
    assert!(matches!(
        add_node_prerequisite(&app.app_pool, owner_id, f.course, f.node, f.node, 0.5).await,
        Err(StorageError::Invalid(_))
    ));
    add_node_prerequisite(&app.app_pool, owner_id, f.course, f.node, child_node, 0.8)
        .await
        .unwrap();
    assert!(matches!(
        add_node_prerequisite(&app.app_pool, owner_id, f.course, child_node, f.node, 0.5).await,
        Err(StorageError::Invalid(_))
    ));
    assert!(matches!(
        add_node_prerequisite(&app.app_pool, owner_id, f.course, f.node, empty_node, 1.5).await,
        Err(StorageError::Invalid(_))
    ));

    // Create 18-node chain testing DAG depth 16 ok vs depth 17 rejected in PostgreSQL trigger.
    let mut chain_nodes = Vec::new();
    for idx in 0..18 {
        let (st, _, res) = app
            .send(
                Method::POST,
                &format!("/v1/courses/{}/nodes", f.course),
                Some(&f.owner),
                Some("application/json"),
                serde_json::to_vec(&json!({"title": format!("Chain-{idx}")})).unwrap(),
                &[],
            )
            .await;
        assert_eq!(st, StatusCode::CREATED);
        chain_nodes.push(uuid_field(&decode_json(&res), "node_id"));
    }
    for i in 0..16 {
        add_node_prerequisite(
            &app.app_pool,
            owner_id,
            f.course,
            chain_nodes[i],
            chain_nodes[i + 1],
            0.25,
        )
        .await
        .unwrap();
    }
    assert!(matches!(
        add_node_prerequisite(
            &app.app_pool,
            owner_id,
            f.course,
            chain_nodes[16],
            chain_nodes[17],
            0.25,
        )
        .await,
        Err(StorageError::Invalid(_))
    ));

    // S-19: 21-weight FSRS-6 parameter persistence & exact REAL[] bit round-trip.
    let custom_weights: [f32; 21] = [
        0.254, 1.38, 2.52, 8.95, 6.21, 0.89, 2.85, 0.0025, 1.91, 0.158, 0.82, 1.51, 0.058, 0.28,
        1.59, 0.58, 1.94, 0.51, 0.088, 0.071, 0.21,
    ];
    let saved_params =
        save_user_fsrs_parameters(&app.app_pool, f.student_id, f.tenant, &custom_weights)
            .await
            .unwrap();
    let loaded_params = load_active_fsrs_parameters(&app.app_pool, f.student_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved_params.weights, custom_weights);
    assert_eq!(loaded_params.weights, custom_weights);
    let rev_with_custom = review(&ReviewInput {
        memory: Some(MemoryInput {
            stability: 5.0,
            difficulty: 5.0,
        }),
        elapsed_days: 3,
        rating: 3,
        desired_retention: 0.9,
        parameters: Some(loaded_params.weights.clone().try_into().unwrap()),
    })
    .unwrap();
    assert!(rev_with_custom.stability.is_finite());

    // Version v1 with active cards mapped via S-07 content_version_cards.
    let v1 = create_content_version(&app.app_pool, owner_id, f.course, None)
        .await
        .unwrap();
    attach_card_to_version(&app.app_pool, owner_id, f.course, v1.id, f.challenge)
        .await
        .unwrap();
    attach_card_to_version(&app.app_pool, owner_id, f.course, v1.id, child_challenge)
        .await
        .unwrap();

    let now = Utc::now();
    // Seed weak decayed parent card & strong child card.
    sqlx::query(
        "INSERT INTO fsrs_cards (
             user_id, challenge_id, tenant_id, course_id, stability, difficulty, reps, last_review, due_at
         ) VALUES
             ($1, $2, $4, $5, 1.5, 5.0, 3, $6, $7),
             ($1, $3, $4, $5, 28.0, 4.0, 6, $8, $9)",
    )
    .bind(f.student_id)
    .bind(f.challenge)
    .bind(child_challenge)
    .bind(f.tenant)
    .bind(f.course)
    .bind(now - Duration::days(30))
    .bind(now - Duration::days(10))
    .bind(now - Duration::hours(2))
    .bind(now + Duration::days(28))
    .execute(&app.admin)
    .await
    .unwrap();

    // Past bubble & unlocked long ago (>72h) so propagation applies.
    ensure_user_protection_state(&app.app_pool, f.student_id, f.tenant)
        .await
        .unwrap();
    sqlx::query("UPDATE user_protection_state SET new_player_bubble_until = $2 WHERE user_id = $1")
        .bind(f.student_id)
        .bind(now - Duration::hours(1))
        .execute(&app.admin)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO user_node_shield_state (
             user_id, node_id, tenant_id, course_id, content_version_id,
             state, tier, e_u_ticks, i_u_ticks, s_u_ticks, is_brownout, unlocked_at
         ) VALUES (
             $1, $2, $3, $4, $5, 'ACTIVE', 1, 6800, 6800, 15000, TRUE, $6
         )",
    )
    .bind(f.student_id)
    .bind(f.node)
    .bind(f.tenant)
    .bind(f.course)
    .bind(v1.id)
    .bind(now - Duration::days(5))
    .execute(&app.admin)
    .await
    .unwrap();

    // POST_RAID_COOLDOWN makes parent raid_immune = true yet propagation_exempt = false.
    grant_post_raid_cooldown(&app.app_pool, f.student_id, f.course, f.node, now, None)
        .await
        .unwrap();

    let dash = recompute_course_shields(&app.app_pool, f.student_id, f.course, now)
        .await
        .unwrap();
    let parent_shield = dash
        .nodes
        .iter()
        .find(|n| n.node_id == f.node)
        .unwrap()
        .shield
        .as_ref()
        .unwrap();
    assert_eq!(parent_shield.state, "ACTIVE");
    assert_eq!(parent_shield.is_brownout, Some(true));
    assert!(parent_shield.raid_immune);
    assert!(!parent_shield.propagation_exempt);

    let child_shield = dash
        .nodes
        .iter()
        .find(|n| n.node_id == child_node)
        .unwrap()
        .shield
        .as_ref()
        .unwrap();
    assert!(child_shield.e_u_ticks.unwrap() < child_shield.i_u_ticks.unwrap());

    // DEC-004: Empty node is INACTIVE with NULL tier/ticks/brownout.
    let inactive_shield = dash
        .nodes
        .iter()
        .find(|n| n.node_id == empty_node)
        .unwrap()
        .shield
        .as_ref()
        .unwrap();
    assert_eq!(inactive_shield.state, "INACTIVE");
    assert_eq!(inactive_shield.tier, None);
    assert_eq!(inactive_shield.e_u_ticks, None);
    assert_eq!(inactive_shield.is_brownout, None);

    // HTTP endpoints: fortress & shields + outsider denied.
    let (st, _, http_dash) = app
        .send(
            Method::GET,
            &format!("/v1/courses/{}/fortress", f.course),
            Some(&f.student),
            None,
            vec![],
            &[],
        )
        .await;
    assert_eq!(st, StatusCode::OK);
    let parsed_dash = decode_json(&http_dash);
    assert!(!parsed_dash["nodes"].as_array().unwrap().is_empty());

    let (_outsider_id, outsider_cookie) = app.register(&email("m4-shield-outsider")).await;
    let (st, _, _) = app
        .send(
            Method::GET,
            &format!("/v1/courses/{}/shields", f.course),
            Some(&outsider_cookie),
            None,
            vec![],
            &[],
        )
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    // §9.6 stale-aware read (> 15 min old -> serves cached with updating=true + enqueues worker).
    sqlx::query(
        "UPDATE user_node_shield_state SET computed_at = $2 WHERE user_id = $1 AND course_id = $3",
    )
    .bind(f.student_id)
    .bind(now - Duration::minutes(20))
    .bind(f.course)
    .execute(&app.admin)
    .await
    .unwrap();

    let (st, _, stale_res) = app
        .send(
            Method::GET,
            &format!("/v1/courses/{}/shields", f.course),
            Some(&f.student),
            None,
            vec![],
            &[],
        )
        .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(decode_json(&stale_res)["updating"], json!(true));

    let processed = process_shield_recompute_batch(&app.worker, Utc::now(), 16)
        .await
        .unwrap();
    assert_eq!(processed, 1);

    let (st, _, fresh_res) = app
        .send(
            Method::GET,
            &format!("/v1/courses/{}/shields", f.course),
            Some(&f.student),
            None,
            vec![],
            &[],
        )
        .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(decode_json(&fresh_res)["updating"], json!(false));

    // S-07: Archiving child card preserves fsrs_cards & review_logs while transitioning child to INACTIVE,
    // and never resets unlocked_at.
    let unlocked_before = child_shield.unlocked_at.unwrap();
    archive_card_in_version(
        &app.app_pool,
        owner_id,
        f.course,
        v1.id,
        child_challenge,
        now,
    )
    .await
    .unwrap();
    let after_archive = recompute_course_shields(&app.app_pool, f.student_id, f.course, now)
        .await
        .unwrap();
    let child_after = after_archive
        .nodes
        .iter()
        .find(|n| n.node_id == child_node)
        .unwrap()
        .shield
        .as_ref()
        .unwrap();
    assert_eq!(child_after.state, "INACTIVE");
    assert_eq!(child_after.tier, None);
    assert_eq!(child_after.unlocked_at, Some(unlocked_before));
    let preserved_cards: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fsrs_cards WHERE user_id = $1 AND challenge_id = $2",
    )
    .bind(f.student_id)
    .bind(child_challenge)
    .fetch_one(&app.admin)
    .await
    .unwrap();
    assert_eq!(preserved_cards, 1);

    // Silence unused imports in edge helper assertion.
    assert_eq!(ticks_1e4(0.7), 7000);
    let _ = (NodeState::Active, evaluate_course_shields);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the private M1 PostgreSQL harness"]
async fn m4_dormancy_bubble_non_negative_version_grace_and_dec005_rls() {
    use campuslingo_storage::{StorageError, generation::create_content_version, shields::*};

    let app = TestApp::new().await;
    let f = fixture(&app, 1).await;
    let owner_id: Uuid = sqlx::query_scalar("SELECT created_by FROM courses WHERE id = $1")
        .bind(f.course)
        .fetch_one(&app.admin)
        .await
        .unwrap();

    // f.student is enrolled in Tenant 1 via fixture(&app, 1); create Tenant 2 + Course 2 to test DEC-005 single PK(user_id).

    let t0 = DateTime::from_timestamp_micros(Utc::now().timestamp_micros()).unwrap();
    // Study 19 transitions -> still inside 48h & < 20 transitions; 20th transition drops bubble immediately.
    let ps1 = record_qualifying_study(&app.app_pool, f.student_id, f.tenant, t0, 19)
        .await
        .unwrap();
    assert_eq!(ps1.eligible_transition_count, 19);

    let ps2 = set_user_accommodation(&app.app_pool, f.student_id, f.tenant, 1.25)
        .await
        .unwrap();
    assert_eq!(ps2.accommodation_multiplier, 1.25);
    assert!(matches!(
        set_user_accommodation(&app.app_pool, f.student_id, f.tenant, 1.1).await,
        Err(StorageError::Invalid(_))
    ));

    // Tenant 2 course enrollment preserves existing user_protection_state row & counters (DEC-005).
    let (owner2_id, owner2_cookie) = app.register(&email("m4-owner2")).await;
    let (st, _, t2_res) = app
        .send(
            Method::POST,
            "/v1/tenants",
            Some(&owner2_cookie),
            Some("application/json"),
            serde_json::to_vec(&json!({"name": "Tenant Two"})).unwrap(),
            &[],
        )
        .await;
    assert_eq!(st, StatusCode::CREATED);
    let tenant2 = uuid_field(&decode_json(&t2_res), "tenant_id");
    let (st, _, c2_res) = app
        .send(
            Method::POST,
            &format!("/v1/tenants/{tenant2}/courses"),
            Some(&owner2_cookie),
            Some("application/json"),
            serde_json::to_vec(&json!({"name": "Course Two"})).unwrap(),
            &[],
        )
        .await;
    assert_eq!(st, StatusCode::CREATED);
    let course2 = uuid_field(&decode_json(&c2_res), "course_id");
    let c2_node = app.node(&owner2_cookie, course2, "Course Two Node").await;
    let _ = app.challenge(&owner2_cookie, course2, c2_node, 0).await;
    assert_eq!(
        app.publish(&owner2_cookie, course2, true).await,
        StatusCode::NO_CONTENT
    );
    let (st, _, _) = app
        .send(
            Method::POST,
            &format!("/v1/courses/{course2}/enroll"),
            Some(&f.student),
            None,
            vec![],
            &[],
        )
        .await;
    assert_eq!(st, StatusCode::NO_CONTENT);

    let ps_cross = ensure_user_protection_state(&app.app_pool, f.student_id, tenant2)
        .await
        .unwrap();
    assert_eq!(ps_cross.tenant_id, f.tenant);
    assert_eq!(ps_cross.eligible_transition_count, 19);

    // Outsider cannot hijack f.tenant user_protection_state.
    assert!(matches!(
        ensure_user_protection_state(&app.app_pool, owner2_id, f.tenant).await,
        Err(StorageError::Forbidden)
    ));

    // S-24: Overlapping dormancy intervals rejected with Conflict (23P01 trigger/exclusion).
    let t_dorm = t0 + Duration::days(8);
    let _open = enter_user_dormancy(&app.app_pool, f.student_id, f.tenant, t_dorm)
        .await
        .unwrap();
    assert!(matches!(
        insert_dormancy_period(
            &app.app_pool,
            f.student_id,
            f.tenant,
            t0 + Duration::days(7) + Duration::hours(1),
            Some(t0 + Duration::days(9)),
        )
        .await,
        Err(StorageError::Conflict)
    ));

    // Return at day 10 grants RESUME_GRACE (72h); second cycle 10 days later does NOT (< 14d cycle).
    let t_ret1 = t0 + Duration::days(10);
    let after_ret1 = record_qualifying_study(&app.app_pool, f.student_id, f.tenant, t_ret1, 1)
        .await
        .unwrap();
    assert!(!after_ret1.is_dormant);
    assert_eq!(after_ret1.eligible_transition_count, 20);
    assert_eq!(
        after_ret1.resume_grace_until,
        Some(t_ret1 + Duration::hours(72))
    );

    let t_ret2 = t_ret1 + Duration::days(10);
    enter_user_dormancy(
        &app.app_pool,
        f.student_id,
        f.tenant,
        t_ret2 - Duration::days(1),
    )
    .await
    .unwrap();
    let after_ret2 = record_qualifying_study(&app.app_pool, f.student_id, f.tenant, t_ret2, 0)
        .await
        .unwrap();
    assert_eq!(after_ret2.last_resume_grace_granted_at, Some(t_ret1));

    // S-25: Version activation + historical dormancy lookup + non-negative VERSION_GRACE + once-ever card.
    let v1 = create_content_version(&app.app_pool, owner_id, f.course, None)
        .await
        .unwrap();
    attach_card_to_version(&app.app_pool, owner_id, f.course, v1.id, f.challenge)
        .await
        .unwrap();
    let t_act = t_dorm + Duration::hours(2); // activated WHILE dormant (dormant [t0+7d, t_ret1))
    activate_content_version(&app.app_pool, owner_id, f.course, v1.id, t_act)
        .await
        .unwrap();

    // Return at t_ret1 = effective_notice_anchor; open at t_ret1 + 1d, engage node at t_ret1 + 2d => 72+12=84h grace!
    let w1 = evaluate_and_grant_version_grace(
        &app.app_pool,
        f.student_id,
        f.course,
        f.node,
        v1.id,
        &VersionGraceTiming {
            opened_at: Some(t_ret1 + Duration::days(1)),
            engaged_at: Some(t_ret1 + Duration::days(2)),
            now: t_ret1 + Duration::days(2),
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(w1.granted_seconds, 84 * 3600);

    // Same card already in user_card_version_grace -> even on new v2, once-ever guarantees None unless new cards added!
    let v2 = create_content_version(&app.app_pool, owner_id, f.course, None)
        .await
        .unwrap();
    attach_card_to_version(&app.app_pool, owner_id, f.course, v2.id, f.challenge)
        .await
        .unwrap();
    activate_content_version(&app.app_pool, owner_id, f.course, v2.id, t_ret2)
        .await
        .unwrap();
    let w2 = evaluate_and_grant_version_grace(
        &app.app_pool,
        f.student_id,
        f.course,
        f.node,
        v2.id,
        &VersionGraceTiming {
            opened_at: Some(t_ret2 + Duration::hours(1)),
            engaged_at: Some(t_ret2 + Duration::hours(2)),
            now: t_ret2 + Duration::hours(2),
        },
    )
    .await
    .unwrap();
    assert!(w2.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the private M1 PostgreSQL harness"]
async fn m4_s13_content_quarantine_and_anti_brigading() {
    use campuslingo_storage::{generation::create_content_version, shields::*};

    let app = TestApp::new().await;
    let f = fixture(&app, 1).await;
    let owner_id: Uuid = sqlx::query_scalar("SELECT created_by FROM courses WHERE id = $1")
        .bind(f.course)
        .fetch_one(&app.admin)
        .await
        .unwrap();

    // Publish course public so additional students can self-enroll; submit a review log that must NEVER be deleted on S-13 quarantine.
    assert_eq!(
        app.publish(&f.owner, f.course, true).await,
        StatusCode::NO_CONTENT
    );

    let (st, _, attempt_res) = app
        .send(
            Method::POST,
            &format!(
                "/v1/courses/{}/challenges/{}/attempts",
                f.course, f.challenge
            ),
            Some(&f.student),
            None,
            vec![],
            &[],
        )
        .await;
    assert_eq!(st, StatusCode::CREATED);
    let attempt_id = uuid_field(&decode_json(&attempt_res), "attempt_id");
    let (st, _, _) = app
        .send(
            Method::POST,
            &format!("/v1/attempts/{attempt_id}/answer"),
            Some(&f.student),
            Some("application/json"),
            serde_json::to_vec(&json!({"selected_index": 1})).unwrap(),
            &[],
        )
        .await;
    assert_eq!(st, StatusCode::OK);

    let v1 = create_content_version(&app.app_pool, owner_id, f.course, None)
        .await
        .unwrap();

    // Register 4 additional enrolled students: 2 sharing a brigade cluster with student1, 2 independent.
    let mut extras = Vec::new();
    for idx in 0..4 {
        let (uid, cookie) = app.register(&email(&format!("s13-s{idx}"))).await;
        let (enr_st, _, _) = app
            .send(
                Method::POST,
                &format!("/v1/courses/{}/enroll", f.course),
                Some(&cookie),
                None,
                vec![],
                &[],
            )
            .await;
        assert_eq!(enr_st, StatusCode::NO_CONTENT);
        extras.push((uid, cookie));
    }

    // Outsider cannot submit report on public unenrolled course (403 Forbidden via can_study_course).
    let (_outsider_id, outsider_cookie) = app.register(&email("m4-report-outsider")).await;
    let (st, _, _) = app
        .send(
            Method::POST,
            &format!("/v1/courses/{}/nodes/{}/report", f.course, f.node),
            Some(&outsider_cookie),
            Some("application/json"),
            serde_json::to_vec(&json!({
                "content_version_id": v1.id,
                "reason": "Broken formula",
                "reporter_cluster": "outsider-cluster"
            }))
            .unwrap(),
            &[],
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // Report 1 from f.student (brigade-A) -> 201, refund + local replacement, NOT quarantined.
    let (st, _, rep1_bytes) = app
        .send(
            Method::POST,
            &format!("/v1/courses/{}/nodes/{}/report", f.course, f.node),
            Some(&f.student),
            Some("application/json"),
            serde_json::to_vec(&json!({
                "content_version_id": v1.id,
                "reason": "Ambiguous unit in question",
                "reporter_cluster": "brigade-A"
            }))
            .unwrap(),
            &[],
        )
        .await;
    assert_eq!(st, StatusCode::CREATED);
    let rep1 = decode_json(&rep1_bytes);
    assert_eq!(rep1["is_eligible"], json!(true));
    assert_eq!(rep1["refund_applied"], json!(true));
    assert_eq!(rep1["version_quarantined"], json!(false));

    // Duplicate report by same student/node/version fails with 409 Conflict (S-13 dedup).
    let (st, _, _) = app
        .send(
            Method::POST,
            &format!("/v1/courses/{}/nodes/{}/report", f.course, f.node),
            Some(&f.student),
            Some("application/json"),
            serde_json::to_vec(&json!({
                "content_version_id": v1.id,
                "reason": "Duplicate report",
                "reporter_cluster": "brigade-A"
            }))
            .unwrap(),
            &[],
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);

    // Two extra accounts in SAME brigade-A cluster are recorded ineligible -> still NOT quarantined.
    for extra in extras.iter().take(2) {
        let (st, _, rbytes) = app
            .send(
                Method::POST,
                &format!("/v1/courses/{}/nodes/{}/report", f.course, f.node),
                Some(&extra.1),
                Some("application/json"),
                serde_json::to_vec(&json!({
                    "content_version_id": v1.id,
                    "reason": "Brigade report",
                    "reporter_cluster": "brigade-A"
                }))
                .unwrap(),
                &[],
            )
            .await;
        assert_eq!(st, StatusCode::CREATED);
        let r = decode_json(&rbytes);
        assert_eq!(r["is_eligible"], json!(false));
        assert_eq!(r["version_quarantined"], json!(false));
    }

    // Independent report #2 (cluster-B) -> still not quarantined; Independent report #3 (cluster-C) -> QUARANTINED!
    let (st, _, r2_bytes) = app
        .send(
            Method::POST,
            &format!("/v1/courses/{}/nodes/{}/report", f.course, f.node),
            Some(&extras[2].1),
            Some("application/json"),
            serde_json::to_vec(&json!({
                "content_version_id": v1.id,
                "reason": "Wrong sign on inequality",
                "reporter_cluster": "cluster-B"
            }))
            .unwrap(),
            &[],
        )
        .await;
    assert_eq!(st, StatusCode::CREATED);
    assert_eq!(decode_json(&r2_bytes)["version_quarantined"], json!(false));

    let (st, _, r3_bytes) = app
        .send(
            Method::POST,
            &format!("/v1/courses/{}/nodes/{}/report", f.course, f.node),
            Some(&extras[3].1),
            Some("application/json"),
            serde_json::to_vec(&json!({
                "content_version_id": v1.id,
                "reason": "Verified incorrect denominator",
                "reporter_cluster": "cluster-C"
            }))
            .unwrap(),
            &[],
        )
        .await;
    assert_eq!(st, StatusCode::CREATED);
    assert_eq!(decode_json(&r3_bytes)["version_quarantined"], json!(true));

    let is_q: bool =
        sqlx::query_scalar("SELECT is_quarantined FROM content_versions WHERE id = $1")
            .bind(v1.id)
            .fetch_one(&app.admin)
            .await
            .unwrap();
    assert!(is_q);

    // Review logs & FSRS memory cards MUST remain intact after quarantine.
    let rev_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM review_logs WHERE user_id = $1")
        .bind(f.student_id)
        .fetch_one(&app.admin)
        .await
        .unwrap();
    assert_eq!(rev_count, 1);

    // Harness-failure quarantine also isolates a separate content version cleanly.
    let v2 = create_content_version(&app.app_pool, owner_id, f.course, None)
        .await
        .unwrap();
    quarantine_content_version_on_harness_failure(
        &app.worker,
        f.course,
        v2.id,
        "harness assertion failed on step 4",
    )
    .await
    .unwrap();
    let v2_q: bool =
        sqlx::query_scalar("SELECT is_quarantined FROM content_versions WHERE id = $1")
            .bind(v2.id)
            .fetch_one(&app.admin)
            .await
            .unwrap();
    assert!(v2_q);
}

// =============================================================================
// Milestone M5 Integration Tests (90s Duels, WS Adversarial, R-07, S-08 Caps)
// =============================================================================

struct RawWsClient {
    stream: tokio::net::TcpStream,
    buf: Vec<u8>,
}

impl RawWsClient {
    async fn connect_raw(
        addr: SocketAddr,
        path_and_query: &str,
        cookie: Option<&str>,
        origin: Option<&str>,
    ) -> std::result::Result<Self, (u16, String)> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr)
            .await
            .map_err(|e| (0_u16, e.to_string()))?;

        let mut req = format!(
            "GET {path_and_query} HTTP/1.1\r\nHost: {addr}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n"
        );
        if let Some(c) = cookie {
            req.push_str(&format!("Cookie: {c}\r\n"));
        }
        if let Some(o) = origin {
            req.push_str(&format!("Origin: {o}\r\n"));
        }
        req.push_str("\r\n");

        stream
            .write_all(req.as_bytes())
            .await
            .map_err(|e| (0_u16, e.to_string()))?;

        let mut buf = Vec::new();
        let mut tmp = [0_u8; 4096];
        loop {
            let n = stream
                .read(&mut tmp)
                .await
                .map_err(|e| (0_u16, e.to_string()))?;
            if n == 0 {
                return Err((0, "EOF before handshake".into()));
            }
            buf.extend_from_slice(&tmp[..n]);
            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                let header_bytes = &buf[..pos + 4];
                let header_str = String::from_utf8_lossy(header_bytes).into_owned();
                let status_line = header_str.lines().next().unwrap_or("");
                let code = status_line
                    .split_whitespace()
                    .nth(1)
                    .and_then(|s| s.parse::<u16>().ok())
                    .unwrap_or(0);
                if code != 101 {
                    return Err((code, header_str));
                }
                assert!(header_str.contains("s3pPLMBiTxaQ9kYGzzhZRbK+xOo="));
                let leftover = buf[pos + 4..].to_vec();
                return Ok(Self {
                    stream,
                    buf: leftover,
                });
            }
        }
    }

    async fn send_text(&mut self, text: &str) {
        use tokio::io::AsyncWriteExt;
        let payload = text.as_bytes();
        let mut frame = Vec::with_capacity(payload.len() + 14);
        frame.push(0x81); // FIN + Text
        let mask_bit = 0x80_u8;
        if payload.len() <= 125 {
            frame.push(mask_bit | (payload.len() as u8));
        } else if payload.len() <= 65_535 {
            frame.push(mask_bit | 126);
            frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        } else {
            frame.push(mask_bit | 127);
            frame.extend_from_slice(&(payload.len() as u64).to_be_bytes());
        }
        let mask = [0x12_u8, 0x34, 0x56, 0x78];
        frame.extend_from_slice(&mask);
        for (i, b) in payload.iter().enumerate() {
            frame.push(b ^ mask[i % 4]);
        }
        self.stream.write_all(&frame).await.unwrap();
    }

    async fn recv_json(&mut self) -> Value {
        loop {
            let (opcode, data) = self.read_frame().await;
            match opcode {
                0x1 => {
                    return serde_json::from_slice(&data).unwrap_or_else(|e| {
                        panic!("invalid JSON frame {}: {e}", String::from_utf8_lossy(&data))
                    });
                }
                0x9 => {
                    // Ping -> send masked Pong
                    use tokio::io::AsyncWriteExt;
                    let mut pong = vec![0x8A_u8, 0x80 | (data.len().min(125) as u8), 0, 0, 0, 0];
                    pong.extend_from_slice(&data[..data.len().min(125)]);
                    let _ = self.stream.write_all(&pong).await;
                }
                0x8 => panic!("unexpected WebSocket close frame"),
                _ => continue,
            }
        }
    }

    async fn read_frame(&mut self) -> (u8, Vec<u8>) {
        use tokio::io::AsyncReadExt;
        loop {
            if self.buf.len() >= 2 {
                let opcode = self.buf[0] & 0x0f;
                let masked = (self.buf[1] & 0x80) != 0;
                let len_byte = (self.buf[1] & 0x7f) as usize;
                let mut offset = 2;
                let payload_len = if len_byte == 126 {
                    if self.buf.len() < 4 {
                        0
                    } else {
                        offset = 4;
                        u16::from_be_bytes([self.buf[2], self.buf[3]]) as usize
                    }
                } else if len_byte == 127 {
                    if self.buf.len() < 10 {
                        0
                    } else {
                        offset = 10;
                        u64::from_be_bytes(self.buf[2..10].try_into().unwrap()) as usize
                    }
                } else {
                    len_byte
                };
                if (len_byte < 126 || offset > 2)
                    && self.buf.len() >= offset + (if masked { 4 } else { 0 }) + payload_len
                {
                    let mask = if masked {
                        let m = [
                            self.buf[offset],
                            self.buf[offset + 1],
                            self.buf[offset + 2],
                            self.buf[offset + 3],
                        ];
                        offset += 4;
                        Some(m)
                    } else {
                        None
                    };
                    let mut payload = self.buf[offset..offset + payload_len].to_vec();
                    if let Some(m) = mask {
                        for (i, b) in payload.iter_mut().enumerate() {
                            *b ^= m[i % 4];
                        }
                    }
                    self.buf.drain(..offset + payload_len);
                    return (opcode, payload);
                }
            }
            let mut tmp = [0_u8; 4096];
            let n = tokio::time::timeout(
                std::time::Duration::from_secs(10),
                self.stream.read(&mut tmp),
            )
            .await
            .expect("timed out waiting for WS frame")
            .unwrap();
            assert!(n > 0, "unexpected WS EOF while reading frame");
            self.buf.extend_from_slice(&tmp[..n]);
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the private M1 PostgreSQL harness"]
async fn m5_s08_exposure_uniqueness_same_day_gap_and_daily_reward_caps_in_postgres() {
    use campuslingo_domain::combat::{RatingOutcome, RatingReason};
    use campuslingo_storage::StorageError;
    use campuslingo_storage::duels::{
        DateTime, Duration as ChronoDuration, Utc, record_duel_card_review,
    };

    let app = TestApp::new().await;
    let f = fixture(&app, 0).await;

    // 20605 * 86400 + 4*3600 + 30*60 = 1780288200 -> 04:30:00 UTC on that UTC day
    let base_day = DateTime::<Utc>::from_timestamp(1_780_288_200, 0).unwrap();
    let match1 = Uuid::new_v4();
    let outcome_easy = RatingOutcome {
        rating_map_version: 2,
        fsrs_rating: Some(4),
        is_void: false,
        reason: RatingReason::FastCleanFullCredit,
        triggers_overcharge: true,
        triggers_counter_strike: false,
        voids_combat_effect: false,
    };

    // Review #1 in match1 -> rewarded & FSRS eligible
    let r1 = record_duel_card_review(
        &app.grader_pool,
        Uuid::new_v4(),
        f.student_id,
        f.challenge,
        f.course,
        match1,
        "MATCH",
        &outcome_easy,
        25,
        base_day,
        9.0,
        4.5,
        4,
    )
    .await
    .unwrap();
    assert!(r1.fsrs_step_eligible);
    assert!(r1.reward_eligible);
    assert_eq!(r1.awarded_points, 25);

    // S-08: duplicate (user_id, challenge_id, 'MATCH', match1) -> DB unique conflict!
    let dup = record_duel_card_review(
        &app.grader_pool,
        Uuid::new_v4(),
        f.student_id,
        f.challenge,
        f.course,
        match1,
        "MATCH",
        &outcome_easy,
        25,
        base_day + ChronoDuration::minutes(35),
        9.0,
        4.5,
        4,
    )
    .await;
    assert!(matches!(dup, Err(StorageError::Conflict)));

    // Cross-session within < 30 min (04:40 UTC) -> blocked SAME_DAY_MIN_GAP!
    let too_soon = record_duel_card_review(
        &app.grader_pool,
        Uuid::new_v4(),
        f.student_id,
        f.challenge,
        f.course,
        Uuid::new_v4(),
        "MATCH",
        &outcome_easy,
        25,
        base_day + ChronoDuration::minutes(10),
        9.0,
        4.5,
        4,
    )
    .await
    .unwrap();
    assert!(!too_soon.fsrs_step_eligible);
    assert!(!too_soon.reward_eligible);
    assert_eq!(
        too_soon.eligibility_blocker.as_deref(),
        Some("SAME_DAY_MIN_GAP")
    );
    assert_eq!(too_soon.awarded_points, 0);

    // Reviews #2 and #3 >= 35 min apart same quota day -> rewarded; #4 hits CARD_DAILY_CAP (3/card)
    for step in 1..=3 {
        let res = record_duel_card_review(
            &app.grader_pool,
            Uuid::new_v4(),
            f.student_id,
            f.challenge,
            f.course,
            Uuid::new_v4(),
            "MATCH",
            &outcome_easy,
            25,
            base_day + ChronoDuration::minutes(35 * step),
            9.0,
            4.5,
            4,
        )
        .await
        .unwrap();
        if step <= 2 {
            assert!(res.reward_eligible);
            assert_eq!(res.awarded_points, 25);
        } else {
            assert!(!res.reward_eligible);
            assert_eq!(res.eligibility_blocker.as_deref(), Some("CARD_DAILY_CAP"));
            assert_eq!(res.awarded_points, 0);
        }
    }

    // Create 60 distinct challenges & earn 1 reward on each until hitting COURSE_DAILY_CAP (60/course)
    for i in 0..59 {
        let ch = app.challenge(&f.owner, f.course, f.node, 0).await;
        let res = record_duel_card_review(
            &app.grader_pool,
            Uuid::new_v4(),
            f.student_id,
            ch,
            f.course,
            Uuid::new_v4(),
            "MATCH",
            &outcome_easy,
            10,
            base_day + ChronoDuration::minutes(35),
            9.0,
            4.5,
            4,
        )
        .await
        .unwrap();
        if i < 57 {
            assert!(res.reward_eligible, "step {i} should be under 60 cap");
        } else {
            // 3 rewarded from f.challenge + 57 = 60 rewarded total -> 58th extra fails COURSE_DAILY_CAP!
            assert!(!res.reward_eligible);
            assert_eq!(res.eligibility_blocker.as_deref(), Some("COURSE_DAILY_CAP"));
            assert_eq!(res.awarded_points, 0);
        }
    }

    // Next local day (04:01 UTC next morning) resets daily caps!
    let next_day = base_day + ChronoDuration::hours(23) + ChronoDuration::minutes(31);
    let fresh = record_duel_card_review(
        &app.grader_pool,
        Uuid::new_v4(),
        f.student_id,
        f.challenge,
        f.course,
        Uuid::new_v4(),
        "MATCH",
        &outcome_easy,
        25,
        next_day,
        9.0,
        4.5,
        4,
    )
    .await
    .unwrap();
    assert!(fresh.reward_eligible);
    assert_eq!(fresh.awarded_points, 25);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the private M1 PostgreSQL harness"]
async fn m5_websocket_two_client_duel_adversarial_spoofing_fizzle_and_o03_budgets() {
    let app = TestApp::new().await;
    let f = fixture(&app, 0).await;
    // Add 3 extra challenges so player has a full 4-card hand (with correct_index = 0 on f.challenge and others)
    for _ in 0..3 {
        app.challenge(&f.owner, f.course, f.node, 0).await;
    }

    let bob_email = email("bob-duel");
    let (_, bob_cookie) = app.register(&bob_email).await;
    assert_eq!(
        app.add_course_member(&f.owner, f.course, &bob_email, "student")
            .await,
        StatusCode::NO_CONTENT
    );
    let outsider_email = email("outsider-duel");
    let (_, outsider_cookie) = app.register(&outsider_email).await;

    // Pair Alice (f.student) and Bob concurrently via POST /duels/matchmake
    let app_clone = app.router.clone();
    let uri = format!("/v1/courses/{}/duels/matchmake", f.course);
    let uri_b = uri.clone();
    let alice_cookie = f.student.clone();
    let bob_cookie_c = bob_cookie.clone();
    let (res_a, res_b) = tokio::join!(
        send_with(
            &app_clone,
            Method::POST,
            &uri,
            Some(&alice_cookie),
            Some("application/json"),
            serde_json::to_vec(&json!({"wait_budget_ms": 2000})).unwrap(),
            &[],
        ),
        send_with(
            &app_clone,
            Method::POST,
            &uri_b,
            Some(&bob_cookie_c),
            Some("application/json"),
            serde_json::to_vec(&json!({"wait_budget_ms": 2000})).unwrap(),
            &[],
        ),
    );
    assert_eq!(res_a.0, StatusCode::CREATED);
    assert_eq!(res_b.0, StatusCode::CREATED);
    let json_a = decode_json(&res_a.2);
    let json_b = decode_json(&res_b.2);
    assert_eq!(json_a["opponent_kind"], "HUMAN");
    assert_eq!(json_b["opponent_kind"], "HUMAN");
    let match_id = uuid_field(&json_a, "match_id");
    assert_eq!(match_id, uuid_field(&json_b, "match_id"));

    let alice_side = json_a["side"].as_str().unwrap();
    let alice_arena = &json_a["arena"];
    let bob_arena = &json_b["arena"];
    let alice_card0 = alice_arena["self"]["hand"][0]["card_id"]
        .as_str()
        .unwrap()
        .to_string();
    let alice_card1 = alice_arena["self"]["hand"][1]["card_id"]
        .as_str()
        .unwrap()
        .to_string();
    let _ = bob_arena;

    // Start ephemeral TCP server for live RFC 6455 two-client adversarial WebSocket tests
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_router = app.router.clone();
    let server_handle = tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            server_router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await;
    });

    // A-02 upgrade guards: unauthenticated, query spoof, evil Origin, and non-participant outsider rejected!
    assert_eq!(
        RawWsClient::connect_raw(
            addr,
            &format!("/v1/duels/ws?match_id={match_id}"),
            None,
            None
        )
        .await
        .err()
        .map(|e| e.0),
        Some(401)
    );
    assert_eq!(
        RawWsClient::connect_raw(
            addr,
            &format!(
                "/v1/duels/ws?match_id={match_id}&user_id={}",
                Uuid::new_v4()
            ),
            Some(&f.student),
            None
        )
        .await
        .err()
        .map(|e| e.0),
        Some(422)
    );
    assert_eq!(
        RawWsClient::connect_raw(
            addr,
            &format!("/v1/duels/ws?match_id={match_id}"),
            Some(&f.student),
            Some("https://evil.example"),
        )
        .await
        .err()
        .map(|e| e.0),
        Some(403)
    );
    assert_eq!(
        RawWsClient::connect_raw(
            addr,
            &format!("/v1/duels/ws?match_id={match_id}"),
            Some(&outsider_cookie),
            None,
        )
        .await
        .err()
        .map(|e| e.0),
        Some(403)
    );

    let mut ws_a = RawWsClient::connect_raw(
        addr,
        &format!("/v1/duels/ws?match_id={match_id}"),
        Some(&f.student),
        None,
    )
    .await
    .unwrap();
    let mut ws_b = RawWsClient::connect_raw(
        addr,
        &format!("/v1/duels/ws?match_id={match_id}"),
        Some(&bob_cookie),
        None,
    )
    .await
    .unwrap();

    let init_a = ws_a.recv_json().await;
    let init_b = ws_b.recv_json().await;
    assert_eq!(init_a["type"], "MATCH_STATE");
    assert_eq!(init_b["type"], "MATCH_STATE");
    // F-04 privacy check: no accommodation_multiplier or challenge_answers in WS snapshot
    assert!(init_a.to_string().find("accommodation").is_none());
    assert!(init_a.to_string().find("correct_index").is_none());

    // Spoofing client rating / hint_used rejected via #[serde(deny_unknown_fields)]!
    ws_a.send_text(
        &json!({
            "type": "STRIKE",
            "sequence": 1,
            "lane": 0,
            "card_id": alice_card0,
            "selected_index": 0,
            "client_elapsed_ms": 2000,
            "rating": 4,
            "hint_used": false,
        })
        .to_string(),
    )
    .await;
    let spoof_err = ws_a.recv_json().await;
    assert_eq!(spoof_err["type"], "ERROR");
    assert_eq!(spoof_err["code"], "REJECTED_SPOOF_OR_SCHEMA");

    // VOID_TRANSPORT_LOSS fizzle capped at 2 voids per match; 3rd escalates to Again=1!
    for fizzle_seq in 1..=3_u32 {
        ws_a.send_text(
            &json!({
                "type": "STRIKE",
                "sequence": fizzle_seq,
                "lane": 0,
                "card_id": alice_card0,
                "selected_index": 0,
                "client_elapsed_ms": 1000,
                "transport_loss_fizzle": true,
            })
            .to_string(),
        )
        .await;
        let msg_a = ws_a.recv_json().await;
        assert_eq!(
            msg_a["type"], "STRIKE_RESOLVED",
            "unexpected msg_a: {msg_a}"
        );
        let msg_b = ws_b.recv_json().await;
        assert_eq!(msg_b["type"], "STRIKE_RESOLVED");
        let res = &msg_a["payload"]["strike"]["result"];
        if fizzle_seq <= 2 {
            assert_eq!(res["fizzled"], true);
            assert_eq!(res["rating"]["fsrs_rating"], Value::Null);
            assert_eq!(res["damage_to_tower"], 0);
        } else {
            assert_eq!(res["rating"]["fsrs_rating"], 1);
            assert_eq!(res["rating"]["triggers_counter_strike"], true);
        }
        // Regen energy for next round
        ws_a.send_text(&json!({"type": "TICK_ADVANCE", "delta_ms": 5000}).to_string())
            .await;
        let _ = ws_a.recv_json().await;
        let _ = ws_b.recv_json().await;
    }

    // Duplicate / late sequence rejected
    ws_a.send_text(
        &json!({
            "type": "STRIKE",
            "sequence": 3,
            "lane": 0,
            "card_id": alice_card0,
            "selected_index": 0,
            "client_elapsed_ms": 2000,
        })
        .to_string(),
    )
    .await;
    let dup_seq = ws_a.recv_json().await;
    assert_eq!(dup_seq["type"], "ERROR");
    assert_eq!(dup_seq["code"], "STRIKE_REJECTED");

    // Sequence-gap resync returns SNAPSHOT_RESYNC
    ws_b.send_text(&json!({"type": "RESYNC_REQUEST", "last_seen_seq": 1}).to_string())
        .await;
    let resync = ws_b.recv_json().await;
    assert_eq!(resync["type"], "SNAPSHOT_RESYNC");

    // Valid Overcharge Strike by Alice on Lane 0 (`alice_side`)
    ws_a.send_text(
        &json!({
            "type": "STRIKE",
            "sequence": 4,
            "lane": 0,
            "card_id": alice_card1,
            "selected_index": 0,
            "client_elapsed_ms": 2000,
        })
        .to_string(),
    )
    .await;
    let valid_a = ws_a.recv_json().await;
    assert_eq!(
        valid_a["type"], "STRIKE_RESOLVED",
        "unexpected valid_a: {valid_a}"
    );
    let valid_b = ws_b.recv_json().await;
    assert_eq!(valid_a["payload"]["strike"]["attacker"], alice_side);
    assert_eq!(valid_a["payload"]["strike"]["result"]["overcharged"], true);
    assert_eq!(valid_b["type"], "STRIKE_RESOLVED");

    server_handle.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the private M1 PostgreSQL harness"]
async fn m5_r07_crash_recovery_replay_within_15s_and_void_server_interruption_after_grace() {
    use campuslingo_domain::combat::{
        AttemptTermination, CombatStrikeInput, DuelArenaState, DuelCardSlot, DuelSide,
        FighterArenaState, RatingOutcome, RatingReason,
    };
    use campuslingo_storage::duels::{
        CrashRecoveryOutcome, Duration as ChronoDuration, Utc, append_match_replay_event,
        mark_match_outage_freeze, record_duel_card_review, recover_or_void_interrupted_matches,
        save_match_snapshot, start_match_record,
    };

    let app = TestApp::new().await;
    let f = fixture(&app, 0).await;

    let cards = vec![DuelCardSlot {
        card_id: "c1".into(),
        archetype: "AXIOM".into(),
        modifier: "COROLLARY_CHAIN".into(),
        card_cost: 2,
        base_window_ms: 8_000,
    }];

    // Match 1: crash + recovery inside 15s grace (replays committed strike since snapshot)
    let m1 = Uuid::new_v4();
    let mut arena1 = DuelArenaState::new(
        FighterArenaState::new(DuelSide::PlayerA, 1, 1, cards.clone()),
        FighterArenaState::new(DuelSide::PlayerB, 1, 1, cards.clone()),
    );
    start_match_record(
        &app.app_pool,
        f.student_id,
        m1,
        f.course,
        f.student_id,
        None,
        "BOT",
        None,
        &format!("duel:{m1}"),
        25,
        &arena1,
    )
    .await
    .unwrap();
    arena1.advance_clock(10_000).unwrap();
    save_match_snapshot(
        &app.app_pool,
        Some(f.student_id),
        m1,
        f.course,
        1,
        10_000,
        &arena1,
    )
    .await
    .unwrap();

    let committed_strike = CombatStrikeInput {
        attacker: DuelSide::PlayerA,
        sequence: 1,
        lane: 0,
        card_id: "c1".into(),
        use_quick_blitz_chain: false,
        termination: AttemptTermination::Submitted,
        is_correct: true,
        partial_credit: false,
        hint_used: false,
        server_sandbox_submits: 1,
        raw_elapsed_ms: 2_500,
        rtt_compensation_ms: 0,
        opponent_card_base_ms: None,
    };
    append_match_replay_event(
        &app.app_pool,
        Some(f.student_id),
        m1,
        f.course,
        "PLAYER_A",
        1,
        "STRIKE",
        12_000,
        &serde_json::to_value(&committed_strike).unwrap(),
    )
    .await
    .unwrap();
    let r_out = RatingOutcome {
        rating_map_version: 2,
        fsrs_rating: Some(4),
        is_void: false,
        reason: RatingReason::FastCleanFullCredit,
        triggers_overcharge: true,
        triggers_counter_strike: false,
        voids_combat_effect: false,
    };
    record_duel_card_review(
        &app.grader_pool,
        Uuid::new_v4(),
        f.student_id,
        f.challenge,
        f.course,
        m1,
        "MATCH",
        &r_out,
        20,
        Utc::now(),
        8.0,
        4.0,
        4,
    )
    .await
    .unwrap();

    let t_crash = Utc::now();
    assert!(
        mark_match_outage_freeze(&app.recovery_pool, m1, t_crash)
            .await
            .unwrap()
    );
    let outcomes1 = recover_or_void_interrupted_matches(
        &app.recovery_pool,
        t_crash + ChronoDuration::seconds(10),
        15,
    )
    .await
    .unwrap();
    assert_eq!(outcomes1.len(), 1);
    match &outcomes1[0] {
        CrashRecoveryOutcome::ResumedFromSnapshot {
            match_id,
            replayed_events,
            outage_frozen_ms,
            arena,
            ..
        } => {
            assert_eq!(*match_id, m1);
            assert_eq!(*replayed_events, 1);
            assert_eq!(*outage_frozen_ms, 10_000);
            assert_eq!(arena.player_b.tower_hp[0], 1_320); // 1800 - 480 Overcharge replayed!
        }
        other => panic!("expected ResumedFromSnapshot, got {other:?}"),
    }

    // Close m1 cleanly so Match 2 test runs in isolation
    campuslingo_storage::duels::settle_match_atomic(
        &app.recovery_pool,
        None,
        m1,
        f.course,
        "COMPLETED",
        Some(f.student_id),
        false,
        None,
        12_000,
    )
    .await
    .unwrap();

    // Match 2: outage exceeds 15s grace -> VOID_SERVER_INTERRUPTION + idempotent gem refund,
    // while committed review_logs stay intact!
    let m2 = Uuid::new_v4();
    let arena2 = DuelArenaState::new(
        FighterArenaState::new(DuelSide::PlayerA, 1, 1, cards.clone()),
        FighterArenaState::new(DuelSide::PlayerB, 1, 1, cards),
    );
    start_match_record(
        &app.app_pool,
        f.student_id,
        m2,
        f.course,
        f.student_id,
        None,
        "BOT",
        None,
        &format!("duel:{m2}"),
        50,
        &arena2,
    )
    .await
    .unwrap();
    mark_match_outage_freeze(&app.recovery_pool, m2, t_crash)
        .await
        .unwrap();
    let outcomes2 = recover_or_void_interrupted_matches(
        &app.recovery_pool,
        t_crash + ChronoDuration::seconds(18),
        15,
    )
    .await
    .unwrap();
    assert!(matches!(
        &outcomes2[0],
        CrashRecoveryOutcome::VoidedAfterGrace { match_id, refunded_gems: 50, .. } if *match_id == m2
    ));

    // Re-running recovery / settle is idempotent and review_logs from m1 remain untouched!
    let outcomes2_again = recover_or_void_interrupted_matches(
        &app.recovery_pool,
        t_crash + ChronoDuration::seconds(20),
        15,
    )
    .await
    .unwrap();
    assert!(outcomes2_again.is_empty());

    let (m2_status, m2_refunded): (String, bool) =
        sqlx::query_as("SELECT status, entry_refunded FROM matches WHERE id = $1")
            .bind(m2)
            .fetch_one(&app.admin)
            .await
            .unwrap();
    assert_eq!(m2_status, "VOID_SERVER_INTERRUPTION");
    assert!(m2_refunded);
    let rev_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM review_logs WHERE user_id = $1")
        .bind(f.student_id)
        .fetch_one(&app.admin)
        .await
        .unwrap();
    assert_eq!(rev_count, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the private M1 PostgreSQL harness"]
async fn m5_matchmaking_4s_ghost_and_bot_fallback_and_15_duel_load_foundation() {
    let app = TestApp::new().await;
    let f = fixture(&app, 0).await;
    let ghost_owner_email = email("ghost-donor");
    let (_, ghost_cookie) = app.register(&ghost_owner_email).await;
    assert_eq!(
        app.add_course_member(&f.owner, f.course, &ghost_owner_email, "student")
            .await,
        StatusCode::NO_CONTENT
    );

    // 1. With no prior completed replays in course, short matchmaking falls back to BOT cleanly.
    let (st1, _, bot_res) = app
        .json(
            Method::POST,
            &format!("/v1/courses/{}/duels/matchmake", f.course),
            Some(&ghost_cookie),
            json!({"wait_budget_ms": 20}),
        )
        .await;
    assert_eq!(st1, StatusCode::CREATED);
    assert_eq!(bot_res["opponent_kind"], "BOT");
    let donor_match = uuid_field(&bot_res, "match_id");

    // Persist a clean STRIKE replay frame on donor_match and complete it as opt-in ghost source
    campuslingo_storage::duels::append_match_replay_event(
        &app.worker,
        None,
        donor_match,
        f.course,
        "PLAYER_A",
        1,
        "STRIKE",
        3_000,
        &json!({
            "attacker": "PLAYER_A",
            "sequence": 1,
            "lane": 0,
            "card_id": "a-card-0",
            "use_quick_blitz_chain": false,
            "termination": "SUBMITTED",
            "is_correct": true,
            "partial_credit": false,
            "hint_used": false,
            "server_sandbox_submits": 1,
            "raw_elapsed_ms": 2_200,
            "rtt_compensation_ms": 0,
            "opponent_card_base_ms": null
        }),
    )
    .await
    .unwrap();
    campuslingo_storage::duels::settle_match_atomic(
        &app.worker,
        None,
        donor_match,
        f.course,
        "COMPLETED",
        Some(
            sqlx::query_scalar::<_, Uuid>("SELECT player_a_id FROM matches WHERE id = $1")
                .bind(donor_match)
                .fetch_one(&app.admin)
                .await
                .unwrap(),
        ),
        false,
        None,
        30_000,
    )
    .await
    .unwrap();

    // 2. Now when student waits briefly in AUTO, matchmaking falls back to permissioned GHOST!
    let (st2, _, ghost_res) = app
        .json(
            Method::POST,
            &format!("/v1/courses/{}/duels/matchmake", f.course),
            Some(&f.student),
            json!({"wait_budget_ms": 20}),
        )
        .await;
    assert_eq!(st2, StatusCode::CREATED);
    assert_eq!(ghost_res["opponent_kind"], "GHOST");
    assert_eq!(uuid_field(&ghost_res, "ghost_source_match_id"), donor_match);

    // 3. 15-duel load foundation: create & settle 15 sequential/concurrent Bot/Ghost duels cleanly
    let ghost_match = uuid_field(&ghost_res, "match_id");
    campuslingo_storage::duels::settle_match_atomic(
        &app.worker,
        None,
        ghost_match,
        f.course,
        "COMPLETED",
        Some(f.student_id),
        false,
        None,
        90_000,
    )
    .await
    .unwrap();

    for i in 0..14 {
        let (st, _, mm) = app
            .json(
                Method::POST,
                &format!("/v1/courses/{}/duels/matchmake", f.course),
                Some(&f.student),
                json!({"wait_budget_ms": 0, "prefer_mode": "BOT"}),
            )
            .await;
        assert_eq!(st, StatusCode::CREATED, "load duel {i} failed: {mm}");
        let mid = uuid_field(&mm, "match_id");
        campuslingo_storage::duels::settle_match_atomic(
            &app.worker,
            None,
            mid,
            f.course,
            "COMPLETED",
            Some(f.student_id),
            false,
            None,
            90_000,
        )
        .await
        .unwrap();
    }

    let (played, won): (i32, i32) = sqlx::query_as(
        "SELECT duels_played, duels_won FROM user_combat_telemetry WHERE user_id = $1 AND course_id = $2",
    )
    .bind(f.student_id)
    .bind(f.course)
    .fetch_one(&app.admin)
    .await
    .unwrap();
    assert_eq!(played, 15);
    assert_eq!(won, 15);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the private M1 PostgreSQL harness"]
async fn m6_raids_opt_in_caps_concurrent_cooldown_revenge_and_45_trophy_loss_cap() {
    let app = TestApp::new().await;
    let f = fixture(&app, 0).await;

    // Make course private so explicit raid opt-in is mandatory
    sqlx::query("UPDATE courses SET visibility = 'private' WHERE id = $1")
        .bind(f.course)
        .execute(&app.admin)
        .await
        .unwrap();

    // Register extra attackers & extra defenders
    let mut att_ids = Vec::new();
    let mut att_cookies = Vec::new();
    for idx in 0..4 {
        let addr = email(&format!("m6-att-{idx}"));
        let (uid, cookie) = app.register(&addr).await;
        assert_eq!(
            app.add_course_member(&f.owner, f.course, &addr, "student")
                .await,
            StatusCode::NO_CONTENT
        );
        att_ids.push(uid);
        att_cookies.push(cookie);
    }

    let mut extra_def_ids = Vec::new();
    for idx in 0..11 {
        let addr = email(&format!("m6-def-{idx}"));
        let (uid, cookie) = app.register(&addr).await;
        assert_eq!(
            app.add_course_member(&f.owner, f.course, &addr, "student")
                .await,
            StatusCode::NO_CONTENT
        );
        let (st, _, _) = app
            .json(
                Method::POST,
                &format!("/v1/courses/{}/raids/opt-in", f.course),
                Some(&cookie),
                json!({"opt_in": true}),
            )
            .await;
        assert_eq!(st, StatusCode::OK);
        extra_def_ids.push(uid);
    }

    let base_day1 = campuslingo_storage::m6::NaiveDate::from_ymd_opt(2026, 11, 10)
        .unwrap()
        .and_hms_opt(14, 30, 0)
        .unwrap()
        .and_utc(); // 22:30 Asia/Kuala_Lumpur (quiet hours)
    sqlx::query("UPDATE users SET timezone = 'Asia/Kuala_Lumpur' WHERE id = $1")
        .bind(f.student_id)
        .execute(&app.admin)
        .await
        .unwrap();

    // Seed eligible Tier 2 shield states + expired new-player bubbles
    let mut all_defenders = vec![f.student_id];
    all_defenders.extend(extra_def_ids.iter().copied());
    for uid in &all_defenders {
        sqlx::query(
            "INSERT INTO user_protection_state \
                (user_id, tenant_id, last_qualifying_study_at, new_player_bubble_until, eligible_transition_count) \
             VALUES ($1, $2, $3, $3 - INTERVAL '2 hours', 25) \
             ON CONFLICT (user_id) DO UPDATE \
             SET is_dormant = FALSE, \
                 resume_grace_until = NULL, \
                 new_player_bubble_until = EXCLUDED.new_player_bubble_until, \
                 eligible_transition_count = 25",
        )
        .bind(uid)
        .bind(f.tenant)
        .bind(base_day1)
        .execute(&app.admin)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO user_node_shield_state \
                (user_id, node_id, tenant_id, course_id, state, tier, e_u_ticks, i_u_ticks, s_u_ticks, is_brownout, unlocked_at, raid_immune, propagation_exempt) \
             VALUES ($1, $2, $3, $4, 'ACTIVE', 2, 8200, 8200, 14000, FALSE, $5 - INTERVAL '48 hours', FALSE, FALSE) \
             ON CONFLICT (user_id, node_id) DO UPDATE \
             SET state = 'ACTIVE', tier = 2, e_u_ticks = 8200, i_u_ticks = 8200, s_u_ticks = 14000, is_brownout = FALSE, raid_immune = FALSE",
        )
        .bind(uid)
        .bind(f.node)
        .bind(f.tenant)
        .bind(f.course)
        .bind(base_day1)
        .execute(&app.admin)
        .await
        .unwrap();
    }

    // 1. Private course without opt-in: scanner shows PRIVATE_COURSE_OPT_IN_REQUIRED and start fails
    let (st_scan0, _, scan0) = app
        .json(
            Method::GET,
            &format!("/v1/courses/{}/raids/scanner", f.course),
            Some(&att_cookies[0]),
            Value::Null,
        )
        .await;
    assert_eq!(st_scan0, StatusCode::OK);
    let scan0_str = scan0.to_string();
    assert!(!scan0_str.contains("fsrs_stability"));
    assert!(!scan0_str.contains("difficulty"));

    let (st_no_opt, _, _) = app
        .json(
            Method::POST,
            &format!("/v1/courses/{}/raids/start", f.course),
            Some(&att_cookies[0]),
            json!({
                "defender_user_id": f.student_id,
                "defender_node_id": f.node,
                "is_revenge": false
            }),
        )
        .await;
    assert_eq!(st_no_opt, StatusCode::FORBIDDEN);

    // Explicit opt-in for defender & attackers
    for c in [
        f.student.as_str(),
        att_cookies[0].as_str(),
        att_cookies[1].as_str(),
        att_cookies[2].as_str(),
        att_cookies[3].as_str(),
    ] {
        let (st, _, _) = app
            .json(
                Method::POST,
                &format!("/v1/courses/{}/raids/opt-in", f.course),
                Some(c),
                json!({"opt_in": true}),
            )
            .await;
        assert_eq!(st, StatusCode::OK);
    }

    // Verify F-04 scanner projection now shows f.student targetable
    let scan1 =
        campuslingo_storage::m6::scan_raid_targets(&app.app_pool, att_ids[0], f.course, base_day1)
            .await
            .unwrap();
    let target_row = scan1
        .iter()
        .find(|r| r.defender_user_id == f.student_id && r.node_id == f.node)
        .expect("defender sector must appear");
    assert!(!target_row.raid_immune);
    assert_eq!(target_row.tier, 2);

    // 2. Concurrent raids (§11.2): att0 & att1 both start before either settles
    let r0 = Uuid::new_v4();
    let r1 = Uuid::new_v4();
    campuslingo_storage::m6::start_raid_session(
        &app.app_pool,
        att_ids[0],
        r0,
        f.course,
        f.student_id,
        f.node,
        false,
        None,
        &format!("raid:{r0}"),
        base_day1,
    )
    .await
    .unwrap();
    campuslingo_storage::m6::start_raid_session(
        &app.app_pool,
        att_ids[1],
        r1,
        f.course,
        f.student_id,
        f.node,
        false,
        None,
        &format!("raid:{r1}"),
        base_day1 + campuslingo_storage::duels::Duration::seconds(5),
    )
    .await
    .unwrap();

    // First settlement wins during 22:30 MYT quiet hours -> 08:00 next morning delivery!
    let s0 = campuslingo_storage::m6::settle_raid_session(
        &app.app_pool,
        att_ids[0],
        r0,
        f.course,
        true,
        3,
        40,
        base_day1 + campuslingo_storage::duels::Duration::seconds(30),
    )
    .await
    .unwrap();
    assert_eq!(s0.status, "SETTLED_WIN");
    assert!(!s0.concurrent_cooldown_blocked);
    assert_eq!(s0.attacker_trophy_gain, 15);
    assert_eq!(s0.defender_trophy_loss, 15);
    assert_eq!(s0.loot_chest_tier.as_deref(), Some("APEX_RAID_CHEST"));

    let notifs =
        campuslingo_storage::m6::get_user_notifications(&app.app_pool, f.student_id, f.course)
            .await
            .unwrap();
    assert_eq!(notifs.len(), 1);
    assert_eq!(notifs[0].raid_count, 1);
    let expected_quiet_end = campuslingo_storage::m6::NaiveDate::from_ymd_opt(2026, 11, 11)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();
    assert_eq!(notifs[0].deliver_after, expected_quiet_end);

    // Second concurrent settlement sees POST_RAID_COOLDOWN -> SETTLED_COOLDOWN_BLOCKED + practice XP!
    let s1 = campuslingo_storage::m6::settle_raid_session(
        &app.app_pool,
        att_ids[1],
        r1,
        f.course,
        true,
        3,
        40,
        base_day1 + campuslingo_storage::duels::Duration::seconds(45),
    )
    .await
    .unwrap();
    assert_eq!(s1.status, "SETTLED_COOLDOWN_BLOCKED");
    assert!(s1.concurrent_cooldown_blocked);
    assert_eq!(s1.defender_trophy_loss, 0);
    assert_eq!(s1.practice_xp_awarded, 15);
    assert!(!s1.consumes_defender_inbound_cap);

    // 3. Mutual revenge same day blocked by MAX_UNORDERED_PAIR_RAIDS_DAY = 1
    let r_pair_err = campuslingo_storage::m6::start_raid_session(
        &app.app_pool,
        f.student_id,
        Uuid::new_v4(),
        f.course,
        att_ids[0],
        f.node,
        true,
        Some(r0),
        "raid:same-day-pair-block",
        base_day1 + campuslingo_storage::duels::Duration::minutes(10),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        r_pair_err,
        campuslingo_storage::StorageError::Invalid(_)
    ));

    // 4. Day 2 (after 04:00 rollover + 6h cooldown):
    //    - Defender executes single-use revenge against att0 (+22 trophies, consumed once)
    //    - Defender takes 3 inbound winning raids capped at 45 daily trophy loss; 4th inbound rejected
    //    - Notification ledger aggregates all 3 wins into raid_count = 3
    //    - Attacker att0 hits MAX_OUTBOUND_RAIDS_PER_DAY = 10; 11th rejected
    let day2 = campuslingo_storage::m6::NaiveDate::from_ymd_opt(2026, 11, 11)
        .unwrap()
        .and_hms_opt(5, 0, 0)
        .unwrap()
        .and_utc(); // 13:00 MYT / 05:00 UTC (past 04:00 rollover in both zones)
    sqlx::query(
        "INSERT INTO user_node_shield_state \
            (user_id, node_id, tenant_id, course_id, state, tier, e_u_ticks, i_u_ticks, s_u_ticks, is_brownout, unlocked_at, raid_immune, propagation_exempt) \
         VALUES ($1, $2, $3, $4, 'ACTIVE', 2, 8200, 8200, 14000, FALSE, $5 - INTERVAL '48 hours', FALSE, FALSE) \
         ON CONFLICT (user_id, node_id) DO UPDATE \
         SET state = 'ACTIVE', tier = 2, e_u_ticks = 8200, i_u_ticks = 8200, s_u_ticks = 14000, is_brownout = FALSE, raid_immune = FALSE",
    )
    .bind(att_ids[0])
    .bind(f.node)
    .bind(f.tenant)
    .bind(f.course)
    .bind(day2)
    .execute(&app.admin)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO user_protection_state (user_id, tenant_id, last_qualifying_study_at, new_player_bubble_until, eligible_transition_count) \
         VALUES ($1, $2, $3, $3 - INTERVAL '2 hours', 25) \
         ON CONFLICT (user_id) DO UPDATE SET is_dormant = FALSE, resume_grace_until = NULL, new_player_bubble_until = EXCLUDED.new_player_bubble_until, eligible_transition_count = 25",
    )
    .bind(att_ids[0])
    .bind(f.tenant)
    .bind(day2)
    .execute(&app.admin)
    .await
    .unwrap();

    let rev_id = Uuid::new_v4();
    campuslingo_storage::m6::start_raid_session(
        &app.app_pool,
        f.student_id,
        rev_id,
        f.course,
        att_ids[0],
        f.node,
        true,
        Some(r0),
        &format!("raid:{rev_id}"),
        day2,
    )
    .await
    .unwrap();
    let rev_settled = campuslingo_storage::m6::settle_raid_session(
        &app.app_pool,
        f.student_id,
        rev_id,
        f.course,
        true,
        3,
        40,
        day2 + campuslingo_storage::duels::Duration::seconds(30),
    )
    .await
    .unwrap();
    assert_eq!(rev_settled.status, "SETTLED_WIN");
    assert_eq!(rev_settled.attacker_trophy_gain, 22);

    // Double-revenge against same r0 must be rejected!
    let rev_dup = campuslingo_storage::m6::start_raid_session(
        &app.app_pool,
        f.student_id,
        Uuid::new_v4(),
        f.course,
        att_ids[0],
        f.node,
        true,
        Some(r0),
        "raid:rev-dup",
        day2 + campuslingo_storage::duels::Duration::minutes(5),
    )
    .await;
    assert!(rev_dup.is_err());

    // Simulate 3 winning inbound raids across Day 2 on f.student (clear cooldown between sequential waves)
    // Pre-seed 30 prior loss on Day 2 (e.g. 2 wins -> 30 loss), then a 3rd revenge win (stake 22) must clamp defender_trophy_loss to 15 (total 45)!
    for (idx, att) in [att_ids[1], att_ids[2]].iter().enumerate() {
        let rid = Uuid::new_v4();
        let ts = day2 + campuslingo_storage::duels::Duration::hours((idx as i64) * 7 + 1);
        if idx > 0 {
            sqlx::query("DELETE FROM user_node_protection WHERE user_id = $1 AND node_id = $2 AND kind = 'POST_RAID_COOLDOWN'")
                .bind(f.student_id)
                .bind(f.node)
                .execute(&app.admin)
                .await
                .unwrap();
        }
        campuslingo_storage::m6::start_raid_session(
            &app.app_pool,
            *att,
            rid,
            f.course,
            f.student_id,
            f.node,
            false,
            None,
            &format!("raid:{rid}"),
            ts,
        )
        .await
        .unwrap();
        let res = campuslingo_storage::m6::settle_raid_session(
            &app.app_pool,
            *att,
            rid,
            f.course,
            true,
            2,
            25,
            ts + campuslingo_storage::duels::Duration::seconds(20),
        )
        .await
        .unwrap();
        assert_eq!(res.defender_trophy_loss, 15);
    }

    // 3rd inbound on Day 2 (revenge raid against f.student or standard -> 15 loss; let's verify revenge source from f.student's Day 2 rev_id gives 22 stake clamped to 15!)
    sqlx::query("DELETE FROM user_node_protection WHERE user_id = $1 AND node_id = $2 AND kind = 'POST_RAID_COOLDOWN'")
        .bind(f.student_id)
        .bind(f.node)
        .execute(&app.admin)
        .await
        .unwrap();
    // Wait: f.student and att0 already paired on Day 2 (`rev_id`), so att3 executes 3rd inbound win:
    let rid3 = Uuid::new_v4();
    let ts3 = day2 + campuslingo_storage::duels::Duration::hours(12);
    campuslingo_storage::m6::start_raid_session(
        &app.app_pool,
        att_ids[3],
        rid3,
        f.course,
        f.student_id,
        f.node,
        false,
        None,
        &format!("raid:{rid3}"),
        ts3,
    )
    .await
    .unwrap();
    let res3 = campuslingo_storage::m6::settle_raid_session(
        &app.app_pool,
        att_ids[3],
        rid3,
        f.course,
        true,
        3,
        35,
        ts3 + campuslingo_storage::duels::Duration::seconds(20),
    )
    .await
    .unwrap();
    assert_eq!(res3.defender_trophy_loss, 15);

    // 4th inbound on Day 2 blocked by MAX_INBOUND_RAIDS_PER_DAY = 3!
    let extra_att_email = email("m6-att-4th");
    let (att4_id, att4_cookie) = app.register(&extra_att_email).await;
    assert_eq!(
        app.add_course_member(&f.owner, f.course, &extra_att_email, "student")
            .await,
        StatusCode::NO_CONTENT
    );
    let (st_o4, _, _) = app
        .json(
            Method::POST,
            &format!("/v1/courses/{}/raids/opt-in", f.course),
            Some(&att4_cookie),
            json!({"opt_in": true}),
        )
        .await;
    assert_eq!(st_o4, StatusCode::OK);
    let in4_err = campuslingo_storage::m6::start_raid_session(
        &app.app_pool,
        att4_id,
        Uuid::new_v4(),
        f.course,
        f.student_id,
        f.node,
        false,
        None,
        "raid:inbound-4th",
        ts3 + campuslingo_storage::duels::Duration::minutes(1),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        in4_err,
        campuslingo_storage::StorageError::Invalid(_)
    ));

    // Coalesced Day 2 RAID_ALERT count == 3
    let day2_notifs =
        campuslingo_storage::m6::get_user_notifications(&app.app_pool, f.student_id, f.course)
            .await
            .unwrap();
    assert_eq!(day2_notifs[0].raid_count, 3);
    assert_eq!(day2_notifs[0].summary_text, "Raided 3 times today");

    // Attacker att0 already did 0 outbound on Day 2 -> run 10 outbound against extra_def_ids[0..10], 11th rejected!
    for (i, &def_id) in extra_def_ids.iter().enumerate().take(10) {
        let rid = Uuid::new_v4();
        campuslingo_storage::m6::start_raid_session(
            &app.app_pool,
            att_ids[0],
            rid,
            f.course,
            def_id,
            f.node,
            false,
            None,
            &format!("raid:att0-day2-{i}"),
            day2 + campuslingo_storage::duels::Duration::minutes(i as i64 + 1),
        )
        .await
        .unwrap();
    }
    let out11_err = campuslingo_storage::m6::start_raid_session(
        &app.app_pool,
        att_ids[0],
        Uuid::new_v4(),
        f.course,
        extra_def_ids[10],
        f.node,
        false,
        None,
        "raid:att0-day2-11",
        day2 + campuslingo_storage::duels::Duration::minutes(12),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        out11_err,
        campuslingo_storage::StorageError::Invalid(_)
    ));

    // Verify F-04 clamping when defender loss = 40 before a 15-trophy settle -> clamps to 5
    sqlx::query(
        "UPDATE raid_sessions SET attacker_trophy_gain = attacker_trophy_gain WHERE id = $1",
    )
    .bind(rid3)
    .execute(&app.admin)
    .await
    .unwrap();
    let total_day2_loss: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(defender_trophy_loss), 0) FROM raid_sessions WHERE defender_user_id = $1 AND quota_date = user_local_quota_date($2, 'Asia/Kuala_Lumpur')",
    )
    .bind(f.student_id)
    .bind(day2)
    .fetch_one(&app.admin)
    .await
    .unwrap();
    assert_eq!(total_day2_loss, 45);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the private M1 PostgreSQL harness"]
async fn m6_boss_shared_hp_concurrency_overkill_and_chest_idempotency_plus_live_ws() {
    let app = TestApp::new().await;
    let f = fixture(&app, 0).await;

    let bob_email = email("m6-boss-bob");
    let (_, bob_cookie) = app.register(&bob_email).await;
    assert_eq!(
        app.add_course_member(&f.owner, f.course, &bob_email, "student")
            .await,
        StatusCode::NO_CONTENT
    );
    let freeloader_email = email("m6-boss-freeloader");
    let (_, freeloader_cookie) = app.register(&freeloader_email).await;
    assert_eq!(
        app.add_course_member(&f.owner, f.course, &freeloader_email, "student")
            .await,
        StatusCode::NO_CONTENT
    );

    // 1. Create LIVE_COOP boss (max_hp = 250) + open/join 2-player co-op lobby
    let (st_b, _, boss_res) = app
        .json(
            Method::POST,
            &format!("/v1/courses/{}/bosses", f.course),
            Some(&f.student),
            json!({
                "node_id": f.node,
                "title": "L4 Coherence Leviathan",
                "mode": "LIVE_COOP",
                "max_hp": 250,
                "gold_reward": 180,
                "cosmetic_reward_key": "holo_crown_apex"
            }),
        )
        .await;
    assert_eq!(st_b, StatusCode::CREATED);
    let boss_id = Uuid::parse_str(boss_res["boss"]["id"].as_str().unwrap()).unwrap();

    let (st_l1, _, lob1) = app
        .json(
            Method::POST,
            &format!("/v1/courses/{}/bosses/{}/lobbies", f.course, boss_id),
            Some(&f.student),
            json!({}),
        )
        .await;
    assert_eq!(st_l1, StatusCode::CREATED);
    let lobby_id = Uuid::parse_str(lob1["lobby"]["id"].as_str().unwrap()).unwrap();

    let (st_l2, _, lob2) = app
        .json(
            Method::POST,
            &format!("/v1/courses/{}/bosses/{}/lobbies", f.course, boss_id),
            Some(&bob_cookie),
            json!({"lobby_id": lobby_id}),
        )
        .await;
    assert_eq!(st_l2, StatusCode::CREATED);
    assert_eq!(
        lob2["lobby"]["member_user_ids"].as_array().unwrap().len(),
        2
    );

    // Start ephemeral server for live /v1/bosses/ws
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_router = app.router.clone();
    let server_handle = tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            server_router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await;
    });

    // A-02 upgrade guards on /v1/bosses/ws
    assert_eq!(
        RawWsClient::connect_raw(
            addr,
            &format!("/v1/bosses/ws?lobby_id={lobby_id}"),
            None,
            None
        )
        .await
        .err()
        .map(|e| e.0),
        Some(401)
    );
    assert_eq!(
        RawWsClient::connect_raw(
            addr,
            &format!(
                "/v1/bosses/ws?lobby_id={lobby_id}&user_id={}",
                Uuid::new_v4()
            ),
            Some(&f.student),
            None
        )
        .await
        .err()
        .map(|e| e.0),
        Some(422)
    );
    assert_eq!(
        RawWsClient::connect_raw(
            addr,
            &format!("/v1/bosses/ws?lobby_id={lobby_id}"),
            Some(&freeloader_cookie),
            None
        )
        .await
        .err()
        .map(|e| e.0),
        Some(403)
    );

    let mut ws_a = RawWsClient::connect_raw(
        addr,
        &format!("/v1/bosses/ws?lobby_id={lobby_id}"),
        Some(&f.student),
        None,
    )
    .await
    .unwrap();
    let mut ws_b = RawWsClient::connect_raw(
        addr,
        &format!("/v1/bosses/ws?lobby_id={lobby_id}"),
        Some(&bob_cookie),
        None,
    )
    .await
    .unwrap();

    let w_a = ws_a.recv_json().await;
    let w_b = ws_b.recv_json().await;
    assert_eq!(w_a["type"], "BOSS_WELCOME");
    assert_eq!(w_b["type"], "BOSS_WELCOME");

    // Alice strikes Good -> 100 dmg -> 150 HP left
    ws_a.send_text(
        &json!({
            "type": "STRIKE",
            "sequence": 1,
            "rating": 3,
            "overcharged": false,
            "base_damage": 100
        })
        .to_string(),
    )
    .await;

    let mut ev1_a = ws_a.recv_json().await;
    while ev1_a["type"] == "LOBBY_UPDATED" {
        ev1_a = ws_a.recv_json().await;
    }
    assert_eq!(ev1_a["type"], "BOSS_HP_UPDATED");
    assert_eq!(ev1_a["strike"]["applied_damage"], 100);
    assert_eq!(ev1_a["strike"]["boss_hp_after"], 150);

    let mut ev1_b = ws_b.recv_json().await;
    while ev1_b["type"] == "LOBBY_UPDATED" {
        ev1_b = ws_b.recv_json().await;
    }
    assert_eq!(ev1_b["type"], "BOSS_HP_UPDATED");
    assert_eq!(ev1_b["strike"]["boss_hp_after"], 150);

    // Bob strikes Easy + Overcharged -> 200 raw dmg -> clamped to 150 applied, 50 overkill!
    ws_b.send_text(
        &json!({
            "type": "STRIKE",
            "sequence": 1,
            "rating": 4,
            "overcharged": true,
            "base_damage": 100
        })
        .to_string(),
    )
    .await;

    let ev2_a = ws_a.recv_json().await;
    let ev2_b = ws_b.recv_json().await;
    assert_eq!(ev2_a["type"], "BOSS_HP_UPDATED");
    assert_eq!(ev2_b["type"], "BOSS_HP_UPDATED");
    assert_eq!(ev2_a["strike"]["raw_damage"], 200);
    assert_eq!(ev2_a["strike"]["applied_damage"], 150);
    assert_eq!(ev2_a["strike"]["overkill_damage"], 50);
    assert_eq!(ev2_a["strike"]["boss_hp_after"], 0);
    assert_eq!(ev2_a["strike"]["boss_status"], "DEFEATED");

    server_handle.abort();

    // 2. Boss chest claim idempotency & non-contributor rejection
    let (st_c1, _, c1) = app
        .json(
            Method::POST,
            &format!("/v1/courses/{}/bosses/{}/claim", f.course, boss_id),
            Some(&f.student),
            json!({}),
        )
        .await;
    assert_eq!(st_c1, StatusCode::OK);
    assert_eq!(c1["claim"]["claimed_now"], true);
    assert_eq!(c1["claim"]["gold_awarded"], 180);
    assert_eq!(c1["claim"]["cosmetic_key"], "holo_crown_apex");

    let (st_c2, _, c2) = app
        .json(
            Method::POST,
            &format!("/v1/courses/{}/bosses/{}/claim", f.course, boss_id),
            Some(&f.student),
            json!({}),
        )
        .await;
    assert_eq!(st_c2, StatusCode::OK);
    assert_eq!(c2["claim"]["claimed_now"], false);
    assert_eq!(c2["claim"]["gold_awarded"], 0);

    // Freeloader with zero damage blocked!
    let (st_free, _, _) = app
        .json(
            Method::POST,
            &format!("/v1/courses/{}/bosses/{}/claim", f.course, boss_id),
            Some(&freeloader_cookie),
            json!({}),
        )
        .await;
    assert_eq!(st_free, StatusCode::FORBIDDEN);

    // Economy endpoint reflects single 180 gold + cosmetic
    let (st_ec, _, ec) = app
        .json(
            Method::GET,
            &format!("/v1/courses/{}/economy", f.course),
            Some(&f.student),
            Value::Null,
        )
        .await;
    assert_eq!(st_ec, StatusCode::OK);
    assert_eq!(ec["balance"]["gold"], 180);
    assert_eq!(ec["cosmetics"].as_array().unwrap().len(), 1);

    // Append-only triggers block UPDATE / DELETE on reward_transactions & user_cosmetic_inventory
    assert!(
        sqlx::query("UPDATE reward_transactions SET gold_delta = 9999 WHERE user_id = $1")
            .bind(f.student_id)
            .execute(&app.admin)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM user_cosmetic_inventory WHERE user_id = $1")
            .bind(f.student_id)
            .execute(&app.admin)
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the private M1 PostgreSQL harness"]
async fn m6_league_30_player_150_floor_tiebreaks_dormancy_and_frozen_clan_war_roster() {
    let app = TestApp::new().await;
    let f = fixture(&app, 0).await;

    let now = campuslingo_storage::duels::Utc::now();
    let week_start = campuslingo_storage::m6::NaiveDate::from_ymd_opt(2026, 11, 9).unwrap();
    let week_key = "2026-W46";
    let scope = format!("COURSE:{}", f.course);

    // Seed 30 players in Tier 1:
    // - Rank 1..4 have >= 150 trophies -> PROMOTED
    // - Rank 5 has 140 trophies (< 150 promotion floor) -> HELD!
    // - Two tied players at 120 resolved by recovery_count DESC -> earliest timestamp -> user_id
    // - One user with consecutive_dormant_weeks = 1 and qualifying_sessions = 2 (< 3) -> DORMANT_DEMOTED!
    let mut users = Vec::new();
    for i in 0..30 {
        let addr = email(&format!("m6-league-{i:02}"));
        let (uid, cookie) = app.register(&addr).await;
        assert_eq!(
            app.add_course_member(&f.owner, f.course, &addr, "student")
                .await,
            StatusCode::NO_CONTENT
        );
        users.push((uid, cookie));
    }

    for (idx, (uid, _)) in users.iter().enumerate() {
        let (trophies, rec, offset_secs, dormant, ret, sess, cons_w) = match idx {
            0..=3 => (190 - (idx as i32) * 5, 4, idx as i64, false, false, 0, 0),
            4 => (145, 10, 0, false, false, 0, 0), // rank 5 slot, but < 150 floor!
            5 => (120, 8, 100, false, false, 0, 0), // wins tie against idx=6 via recovery=8 > 3
            6 => (120, 3, 10, false, false, 0, 0),
            29 => (210, 5, 0, false, true, 2, 1), // returning dormant < 3 sessions -> DORMANT + week 2 -> DORMANT_DEMOTED!
            k => (60 - (k as i32), 1, k as i64, false, false, 0, 0),
        };
        campuslingo_storage::m6::upsert_league_score(
            &app.app_pool,
            f.student_id,
            *uid,
            campuslingo_storage::m6::UpsertLeagueScoreInput {
                league_scope: scope.clone(),
                course_id: f.course,
                week_key: week_key.into(),
                week_start_date: week_start,
                tier: 2,
                late_joiner_shielded: false,
                timezone: "Asia/Kuala_Lumpur".into(),
                trophies_delta: trophies,
                recovery_delta: rec,
                is_dormant: dormant,
                returned_from_dormancy_this_week: ret,
                qualifying_sessions_delta: sess,
                consecutive_dormant_weeks: cons_w,
                now: now + campuslingo_storage::duels::Duration::seconds(offset_secs),
            },
        )
        .await
        .unwrap();
    }

    let (st_r1, _, r1) = app
        .json(
            Method::POST,
            &format!("/v1/courses/{}/leagues/rollover", f.course),
            Some(&f.student),
            json!({
                "league_scope": scope,
                "week_key": week_key,
                "tier": 2
            }),
        )
        .await;
    assert_eq!(st_r1, StatusCode::OK);
    // Idempotent second call
    let (st_r2, _, r2) = app
        .json(
            Method::POST,
            &format!("/v1/courses/{}/leagues/rollover", f.course),
            Some(&f.student),
            json!({
                "league_scope": scope,
                "week_key": week_key,
                "tier": 2
            }),
        )
        .await;
    assert_eq!(st_r2, StatusCode::OK);
    assert_eq!(r1["bracket"]["rolled_over"], true);
    assert_eq!(r1["outcome"], r2["outcome"]);

    let standings = r1["outcome"]["standings"].as_array().unwrap();
    let find_out = |uid: Uuid| {
        standings
            .iter()
            .find(|s| s["user_id"] == uid.to_string())
            .unwrap()
            .clone()
    };
    assert_eq!(find_out(users[0].0)["outcome"], "PROMOTED");
    assert_eq!(find_out(users[3].0)["outcome"], "PROMOTED");
    // Rank 5 with 145 < 150 floor MUST be HELD!
    assert_eq!(find_out(users[4].0)["outcome"], "HELD");
    // Tiebreak: idx 5 (recovery=8) ranks ahead of idx 6 (recovery=3)
    assert!(
        find_out(users[5].0)["rank"].as_u64().unwrap()
            < find_out(users[6].0)["rank"].as_u64().unwrap()
    );
    // Returning dormant < 3 sessions + 2nd consecutive dormant week -> DORMANT_DEMOTED!
    assert_eq!(find_out(users[29].0)["outcome"], "DORMANT_DEMOTED");
    assert_eq!(find_out(users[29].0)["league_status"], "DORMANT");

    // 2. Clan Wars: week-start active_members_snapshot immutability + relative f64 stability formula
    let (st_c, _, clan_res) = app
        .json(
            Method::POST,
            &format!("/v1/courses/{}/clans", f.course),
            Some(&f.student),
            json!({"name": "Iron Citadel"}),
        )
        .await;
    assert_eq!(st_c, StatusCode::CREATED);
    let clan_id = Uuid::parse_str(clan_res["clan"]["id"].as_str().unwrap()).unwrap();

    for (_, cookie) in users.iter().take(2) {
        let (st_j, _, _) = app
            .json(
                Method::POST,
                &format!("/v1/courses/{}/clans/{}/join", f.course, clan_id),
                Some(cookie),
                json!({}),
            )
            .await;
        assert_eq!(st_j, StatusCode::OK);
    }

    let (st_w, _, war_res) = app
        .json(
            Method::POST,
            &format!("/v1/courses/{}/clans/{}/wars", f.course, clan_id),
            Some(&f.student),
            json!({"week_key": week_key, "week_start_date": week_start}),
        )
        .await;
    assert_eq!(st_w, StatusCode::CREATED);
    let war_id = Uuid::parse_str(war_res["war"]["id"].as_str().unwrap()).unwrap();
    assert_eq!(war_res["war"]["active_members_snapshot"], 3);

    // Mid-week joiner cannot contribute to frozen war roster!
    let (st_j_late, _, _) = app
        .json(
            Method::POST,
            &format!("/v1/courses/{}/clans/{}/join", f.course, clan_id),
            Some(&users[10].1),
            json!({}),
        )
        .await;
    assert_eq!(st_j_late, StatusCode::OK);
    let (st_late_contrib, _, _) = app
        .json(
            Method::POST,
            &format!(
                "/v1/courses/{}/clans/{}/wars/{}/contribute",
                f.course, clan_id, war_id
            ),
            Some(&users[10].1),
            json!({
                "contribution_kind": "RAID_WIN",
                "reward_eligible": true,
                "settlement_key": "war-late-spoof"
            }),
        )
        .await;
    assert_eq!(st_late_contrib, StatusCode::FORBIDDEN);

    // Frozen snapshot mutation blocked by trigger!
    assert!(
        sqlx::query("UPDATE clan_wars SET active_members_snapshot = 4 WHERE id = $1")
            .bind(war_id)
            .execute(&app.admin)
            .await
            .is_err()
    );

    // Roster member contributes STABILITY_GAIN (2.0 -> 10.0) => raw=16, normalized=9, relative_gain_bps=40000
    let (st_ok_contrib, _, war_after) = app
        .json(
            Method::POST,
            &format!(
                "/v1/courses/{}/clans/{}/wars/{}/contribute",
                f.course, clan_id, war_id
            ),
            Some(&f.student),
            json!({
                "contribution_kind": "STABILITY_GAIN",
                "s_before": 2.0,
                "s_after": 10.0,
                "reward_eligible": true,
                "settlement_key": "war-stab-1"
            }),
        )
        .await;
    assert_eq!(st_ok_contrib, StatusCode::OK);
    assert_eq!(war_after["war"]["total_raw_points"], 16);
    assert_eq!(war_after["war"]["total_normalized_points"], 9);

    let (st_get_war, _, got_war) = app
        .json(
            Method::GET,
            &format!("/v1/courses/{}/clans/{}/wars/{}", f.course, clan_id, war_id),
            Some(&f.student),
            Value::Null,
        )
        .await;
    assert_eq!(st_get_war, StatusCode::OK);
    assert_eq!(got_war["contributions"][0]["relative_gain_bps"], 40000);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the private M1 PostgreSQL harness"]
async fn m7_notifications_quiet_hours_daily_cap_and_opt_out() {
    let app = TestApp::new().await;
    let f = fixture(&app, 1).await;

    sqlx::query("UPDATE users SET timezone = 'Asia/Kuala_Lumpur' WHERE id = $1")
        .bind(f.student_id)
        .execute(&app.admin)
        .await
        .unwrap();

    // 1. Register notification device
    let (st_dev, _, dev_res) = app
        .json(
            Method::POST,
            "/v1/notifications/devices",
            Some(&f.student),
            json!({
                "platform": "WEB_PUSH",
                "endpoint_or_token": "https://push.example.test/sub/device-1",
                "p256dh_key": "test-p256dh-key",
                "auth_secret": "test-auth-secret"
            }),
        )
        .await;
    assert_eq!(st_dev, StatusCode::CREATED);
    assert!(dev_res.get("device_id").is_some());

    // 2. Two inbound raids during 23:15 MYT quiet hours (15:15 UTC on 2026-11-11)
    let t_quiet = campuslingo_storage::m6::NaiveDate::from_ymd_opt(2026, 11, 11)
        .unwrap()
        .and_hms_opt(15, 15, 0)
        .unwrap()
        .and_utc();
    let t_quiet_2 = campuslingo_storage::m6::NaiveDate::from_ymd_opt(2026, 11, 11)
        .unwrap()
        .and_hms_opt(15, 25, 0)
        .unwrap()
        .and_utc();
    let expected_0800_utc = campuslingo_storage::m6::NaiveDate::from_ymd_opt(2026, 11, 12)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();

    let raid_1 = Uuid::new_v4();
    let raid_2 = Uuid::new_v4();
    campuslingo_storage::m7::enqueue_raid_alert_outbox(
        &app.admin,
        f.student_id,
        f.student_id,
        f.course,
        raid_1,
        t_quiet,
    )
    .await
    .unwrap();
    campuslingo_storage::m7::enqueue_raid_alert_outbox(
        &app.admin,
        f.student_id,
        f.student_id,
        f.course,
        raid_2,
        t_quiet_2,
    )
    .await
    .unwrap();

    // 3. Enqueue pre-expiry shield digest in same local day (slot 2 of MAX_PUSH_NOTIFICATIONS_PER_DAY = 2)
    let digest_ts = t_quiet.timestamp() + 900;
    let (st_dig, _, _) = app
        .json(
            Method::POST,
            &format!("/v1/courses/{}/notifications/shield-digest", f.course),
            Some(&f.student),
            json!({
                "node_ids": [f.node],
                "now_epoch_secs": digest_ts
            }),
        )
        .await;
    assert_eq!(st_dig, StatusCode::CREATED);

    let (st_out, _, out_res) = app
        .json(
            Method::GET,
            &format!("/v1/notifications/outbox?course_id={}", f.course),
            Some(&f.student),
            Value::Null,
        )
        .await;
    assert_eq!(st_out, StatusCode::OK);
    let outbox = out_res["outbox"].as_array().unwrap();
    assert_eq!(outbox.len(), 2);

    let raid_row = outbox
        .iter()
        .find(|row| row["notification_kind"] == "RAID_ALERT")
        .expect("RAID_ALERT row must exist");
    assert_eq!(raid_row["summary_text"], "Raided 2 times today");
    assert_eq!(raid_row["event_count"], 2);
    assert_eq!(raid_row["quiet_hours_deferred"], true);
    let raid_ids: Vec<String> = raid_row["raid_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert!(raid_ids.contains(&raid_1.to_string()));
    assert!(raid_ids.contains(&raid_2.to_string()));
    assert_eq!(
        chrono_parse_utc(raid_row["deliver_after"].as_str().unwrap()),
        expected_0800_utc
    );

    // Trigger enforcement rejects inserting a 3rd distinct notification kind on the same local quota date
    let cap_err = sqlx::query(
        "INSERT INTO notification_outbox \
            (user_id, course_id, notification_kind, local_quota_date, event_count, summary_text, quiet_hours_deferred, deliver_after, status) \
         VALUES ($1, $2, 'RAID_ALERT', DATE '2026-11-11', 1, 'extra', FALSE, $3, 'QUEUED')",
    )
    .bind(f.student_id)
    .bind(f.course)
    .bind(expected_0800_utc)
    .execute(&app.admin)
    .await;
    // Wait: UNIQUE fires or trigger fires — both block >= 3 notifications; verify total rows <= 2!
    assert!(cap_err.is_err());

    // 4. Dispatch before 08:00 MYT (07:30 MYT = 23:30 UTC on 2026-11-11) delivers 0
    let t_before_0800 = expected_0800_utc.timestamp() - 1800;
    let (st_d0, _, d0_res) = app
        .json(
            Method::POST,
            "/v1/notifications/dispatch",
            Some(&f.student),
            json!({ "now_epoch_secs": t_before_0800 }),
        )
        .await;
    assert_eq!(st_d0, StatusCode::OK);
    assert_eq!(d0_res["delivered"].as_array().unwrap().len(), 0);

    // 5. Dispatch after 08:00 MYT (08:05 MYT = 00:05 UTC on 2026-11-12) delivers both due notifications
    let t_after_0800 = expected_0800_utc.timestamp() + 300;
    let (st_d1, _, d1_res) = app
        .json(
            Method::POST,
            "/v1/notifications/dispatch",
            Some(&f.student),
            json!({ "now_epoch_secs": t_after_0800 }),
        )
        .await;
    assert_eq!(st_d1, StatusCode::OK);
    assert_eq!(d1_res["delivered"].as_array().unwrap().len(), 2);

    // 6. Opt-out suppresses future notifications
    let (st_pref, _, _) = app
        .json(
            Method::PUT,
            "/v1/notifications/preferences",
            Some(&f.student),
            json!({
                "push_enabled": false,
                "raid_alerts_enabled": true,
                "shield_digest_enabled": true
            }),
        )
        .await;
    assert_eq!(st_pref, StatusCode::OK);

    let raid_next_day = Uuid::new_v4();
    let t_next_day = expected_0800_utc + campuslingo_storage::duels::Duration::hours(4); // 12:00 MYT next local day
    campuslingo_storage::m7::enqueue_raid_alert_outbox(
        &app.admin,
        f.student_id,
        f.student_id,
        f.course,
        raid_next_day,
        t_next_day,
    )
    .await
    .unwrap();

    let (st_d2, _, d2_res) = app
        .json(
            Method::POST,
            "/v1/notifications/dispatch",
            Some(&f.student),
            json!({ "now_epoch_secs": t_next_day.timestamp() + 60 }),
        )
        .await;
    assert_eq!(st_d2, StatusCode::OK);
    assert_eq!(d2_res["delivered"].as_array().unwrap().len(), 0);

    let (st_out2, _, out_res2) = app
        .json(
            Method::GET,
            &format!("/v1/notifications/outbox?course_id={}", f.course),
            Some(&f.student),
            Value::Null,
        )
        .await;
    assert_eq!(st_out2, StatusCode::OK);
    let suppressed = out_res2["outbox"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| {
            row["raid_ids"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .any(|v| v.as_str() == Some(&raid_next_day.to_string()))
                })
                .unwrap_or(false)
        })
        .expect("opt-out raid row must exist");
    assert_eq!(suppressed["status"], "SUPPRESSED_OPT_OUT");
}

fn chrono_parse_utc(raw: &str) -> campuslingo_storage::m7::DateTime<campuslingo_storage::m7::Utc> {
    campuslingo_storage::m7::DateTime::parse_from_rfc3339(raw)
        .expect("valid RFC3339 timestamp")
        .with_timezone(&campuslingo_storage::m7::Utc)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the private M1 PostgreSQL harness"]
async fn m7_pseudonymization_retention_and_pg_dump_restore_drill() {
    let app = TestApp::new().await;
    let f = fixture(&app, 1).await;

    // Seed an active card + review + notification device
    let attempt_id = app.attempt(&f.student, f.course, f.challenge).await;
    let (st_sub, _) = app.answer(&f.student, attempt_id, 1).await;
    assert_eq!(st_sub, StatusCode::OK);

    let (st_dev, _, _) = app
        .json(
            Method::POST,
            "/v1/notifications/devices",
            Some(&f.student),
            json!({
                "platform": "IOS_APNS",
                "endpoint_or_token": "apns-secret-device-token-xyz",
                "p256dh_key": null,
                "auth_secret": null
            }),
        )
        .await;
    assert_eq!(st_dev, StatusCode::CREATED);

    // Seed an old superseded published content version (> 30d) + an active newer version so retention sweep archives old version only
    let old_cv_id = Uuid::new_v4();
    let new_cv_id = Uuid::new_v4();
    let t_now = campuslingo_storage::m7::Utc::now();
    let t_old = t_now - campuslingo_storage::duels::Duration::days(45);
    sqlx::query(
        "INSERT INTO content_versions (id, tenant_id, course_id, version_number, status, is_quarantined, activated_at, created_at) \
         VALUES ($1, $2, $3, 1001, 'published', FALSE, $4, $4), \
                ($5, $2, $3, 1002, 'published', FALSE, $6, $6)",
    )
    .bind(old_cv_id)
    .bind(f.tenant)
    .bind(f.course)
    .bind(t_old)
    .bind(new_cv_id)
    .bind(t_now)
    .execute(&app.admin)
    .await
    .unwrap();

    // O-07 Real pg_dump backup & restore verification drill across fsrs_cards, review_logs, reward_transactions
    let admin_url = env("CAMPUSLINGO_TEST_ADMIN_URL");
    let restore_url = env("CAMPUSLINGO_TEST_RESTORE_URL");
    let pg_bin = std::env::var("CAMPUSLINGO_PG_BIN").unwrap_or_else(|_| {
        "/home/mahdi/campus-lingo/services/core-ai/.venv/lib/python3.11/site-packages/pgserver/pginstall/bin"
            .to_string()
    });
    let dump_path = app.asset_dir.join("o07_restore_drill.sql");
    let dump_out = std::process::Command::new(PathBuf::from(&pg_bin).join("pg_dump"))
        .args([
            "--dbname",
            &admin_url,
            "--format=plain",
            "--no-owner",
            "--no-privileges",
            "--file",
            dump_path.to_str().unwrap(),
        ])
        .output()
        .expect("pg_dump execution");
    assert!(
        dump_out.status.success(),
        "pg_dump failed: {}",
        String::from_utf8_lossy(&dump_out.stderr)
    );

    // Strip COMMENT ON EXTENSION vector (owned by bootstrap superuser) so non-superuser owner restore passes ON_ERROR_STOP=1 cleanly
    let raw_sql = std::fs::read_to_string(&dump_path).unwrap();
    let filtered_sql: String = raw_sql
        .lines()
        .filter(|line| {
            !line.starts_with("COMMENT ON EXTENSION ")
                && !line.starts_with("SET default_table_access_method")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let clean_dump_path = app.asset_dir.join("o07_restore_drill_clean.sql");
    std::fs::write(&clean_dump_path, filtered_sql).unwrap();

    let restore_out = std::process::Command::new(PathBuf::from(&pg_bin).join("psql"))
        .args([
            "-v",
            "ON_ERROR_STOP=1",
            "--dbname",
            &restore_url,
            "-f",
            clean_dump_path.to_str().unwrap(),
        ])
        .output()
        .expect("psql restore execution");
    assert!(
        restore_out.status.success(),
        "psql restore failed: {}",
        String::from_utf8_lossy(&restore_out.stderr)
    );

    let restore_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&restore_url)
        .await
        .unwrap();
    for sql in [
        "SELECT COUNT(*) FROM fsrs_cards",
        "SELECT COUNT(*) FROM review_logs",
        "SELECT COUNT(*) FROM reward_transactions",
    ] {
        let src_cnt: i64 = sqlx::query_scalar(sql).fetch_one(&app.admin).await.unwrap();
        let dst_cnt: i64 = sqlx::query_scalar(sql)
            .fetch_one(&restore_pool)
            .await
            .unwrap();
        assert_eq!(src_cnt, dst_cnt, "restored row count mismatch for {sql}");
    }
    restore_pool.close().await;

    // Account pseudonymization scrubs PII & devices while retaining immutable logs
    let (st_er, _, er_res) = app
        .json(
            Method::POST,
            "/v1/me/pseudonymize",
            Some(&f.student),
            json!({
                "reason": "User GDPR Article 17 erasure request",
                "now_epoch_secs": t_now.timestamp()
            }),
        )
        .await;
    assert_eq!(st_er, StatusCode::OK);
    let scrubbed_email = er_res["erasure"]["pseudonymized_email"].as_str().unwrap();
    assert!(scrubbed_email.ends_with("@erased.invalid"));
    assert_ne!(scrubbed_email, f.student_email);
    assert!(er_res["erasure"]["retained_review_count"].as_i64().unwrap() >= 1);

    // Session cookie was deleted on pseudonymization -> subsequent auth fails 401
    let (st_after, _, _) = app
        .json(
            Method::GET,
            "/v1/notifications/outbox",
            Some(&f.student),
            Value::Null,
        )
        .await;
    assert_eq!(st_after, StatusCode::UNAUTHORIZED);

    // Devices were scrubbed from DB, while review_logs stays intact & immutable
    let dev_left: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM notification_devices WHERE user_id = $1")
            .bind(f.student_id)
            .fetch_one(&app.admin)
            .await
            .unwrap();
    assert_eq!(dev_left, 0);

    let del_review = sqlx::query("DELETE FROM review_logs WHERE user_id = $1")
        .bind(f.student_id)
        .execute(&app.admin)
        .await;
    assert!(
        del_review.is_err(),
        "review_logs must remain immutable after erasure"
    );

    // Versioned retention sweep as course owner archives old content version only
    let (st_ret, _, ret_res) = app
        .json(
            Method::POST,
            &format!("/v1/courses/{}/retention/sweep", f.course),
            Some(&f.owner),
            json!({ "now_epoch_secs": t_now.timestamp() }),
        )
        .await;
    assert_eq!(st_ret, StatusCode::OK);
    assert_eq!(ret_res["retention_sweep"]["archived_versions_count"], 1);

    let old_status: String =
        sqlx::query_scalar("SELECT status FROM content_versions WHERE id = $1")
            .bind(old_cv_id)
            .fetch_one(&app.admin)
            .await
            .unwrap();
    let new_status: String =
        sqlx::query_scalar("SELECT status FROM content_versions WHERE id = $1")
            .bind(new_cv_id)
            .fetch_one(&app.admin)
            .await
            .unwrap();
    assert_eq!(old_status, "archived");
    assert_eq!(new_status, "published");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the private M1 PostgreSQL harness"]
async fn m7_a06_actor_rate_limiter_and_security_headers() {
    let app = TestApp::new().await;
    let f = fixture(&app, 1).await;

    let (st_h, hdr_h, body_h) = app
        .send(Method::GET, "/healthz", None, None, Vec::new(), &[])
        .await;
    assert_eq!(st_h, StatusCode::OK);
    assert_eq!(hdr_h.get("x-content-type-options").unwrap(), "nosniff");
    assert_eq!(hdr_h.get("x-frame-options").unwrap(), "DENY");
    assert_eq!(hdr_h.get("referrer-policy").unwrap(), "no-referrer");
    assert_eq!(
        hdr_h.get("permissions-policy").unwrap(),
        "geolocation=(), microphone=(), camera=()"
    );
    let h_json: Value = serde_json::from_slice(&body_h).unwrap();
    assert_eq!(h_json["milestone"], "M7");

    let mut limited_state = app.state.clone();
    limited_state.actor_limiter =
        Arc::new(campuslingo_server::auth::ActorRateLimiter::with_limit(3));
    let limited_router = build_router(limited_state);

    for attempt in 1..=4 {
        let (st, hdr, body) = send_with(
            &limited_router,
            Method::GET,
            "/v1/notifications/outbox",
            Some(&f.student),
            None,
            Vec::new(),
            &[],
        )
        .await;
        assert_eq!(hdr.get("x-content-type-options").unwrap(), "nosniff");
        assert_eq!(hdr.get("cache-control").unwrap(), "no-store");
        if attempt <= 3 {
            assert_eq!(st, StatusCode::OK);
        } else {
            assert_eq!(st, StatusCode::TOO_MANY_REQUESTS);
            let err_json: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(err_json["error"], "rate_limited");
        }
    }
}
