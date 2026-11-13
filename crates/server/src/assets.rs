//! Private asset upload and authorized delivery.
//!
//! Bytes are streamed to a private UUID file while being hashed, so an upload
//! never has to fit in memory alongside other requests. The database stores
//! only metadata; a download re-authorizes the course on every request.

use axum::{
    Json,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, Request, StatusCode, header},
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::{AppState, auth::require_actor, error::ApiError};

type ApiResult<T> = Result<T, ApiError>;

const MAX_ASSET_BYTES: u64 = 80 * 1024 * 1024;
const ALLOWED_MEDIA_TYPES: &[&str] = &[
    "application/pdf",
    "image/png",
    "image/jpeg",
    "image/webp",
    "text/plain",
    "audio/ogg",
    "audio/opus",
];

fn media_type_from(headers: &HeaderMap) -> ApiResult<String> {
    let raw = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .ok_or(ApiError::Invalid("a Content-Type header is required"))?;
    let media_type = raw
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if !ALLOWED_MEDIA_TYPES.contains(&media_type.as_str()) {
        return Err(ApiError::Invalid("the asset media type is not allowed"));
    }
    Ok(media_type)
}

fn private_file_options() -> tokio::fs::OpenOptions {
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    options
}

pub async fn upload_asset(
    State(state): State<AppState>,
    Path(course_id): Path<Uuid>,
    request: Request<Body>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let actor = require_actor(&state, request.headers()).await?;
    let media_type = media_type_from(request.headers())?;
    // Authorize before accepting any body bytes. The transactional insert below
    // repeats this check so a concurrent membership change still fails closed.
    if !campuslingo_storage::courses::can_manage(&state.app_pool, actor, course_id).await? {
        return Err(ApiError::NotFound);
    }
    let body = request.into_body();

    let asset_id = Uuid::new_v4();
    let final_path = state.settings.asset_dir.join(asset_id.to_string());
    let temp_path = state
        .settings
        .asset_dir
        .join(format!("{asset_id}.uploading"));

    let mut file = private_file_options()
        .open(&temp_path)
        .await
        .map_err(|error| {
            eprintln!("asset temp file open failed: {error}");
            ApiError::Internal
        })?;
    let mut digest = Sha256::new();
    let mut total: u64 = 0;
    let mut stream = body.into_data_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(_) => {
                drop(file);
                let _ = tokio::fs::remove_file(&temp_path).await;
                return Err(ApiError::Invalid("the asset upload was interrupted"));
            }
        };
        let Some(next_total) = total.checked_add(chunk.len() as u64) else {
            drop(file);
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(ApiError::Invalid("the asset is too large"));
        };
        total = next_total;
        if total > MAX_ASSET_BYTES {
            drop(file);
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(ApiError::Invalid("the asset exceeds the 80 MiB limit"));
        }
        digest.update(&chunk);
        if file.write_all(&chunk).await.is_err() {
            drop(file);
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(ApiError::Internal);
        }
    }

    if total == 0 {
        drop(file);
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(ApiError::Invalid("the asset is empty"));
    }
    if file.flush().await.is_err() {
        drop(file);
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(ApiError::Internal);
    }
    drop(file);

    if let Err(error) = tokio::fs::rename(&temp_path, &final_path).await {
        eprintln!("asset publish failed: {error}");
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(ApiError::Internal);
    }

    let sha256 = hex::encode(digest.finalize());
    match campuslingo_storage::assets::create_asset(
        &state.app_pool,
        actor,
        course_id,
        asset_id,
        &sha256,
        &media_type,
        i64::try_from(total).unwrap_or(i64::MAX),
    )
    .await
    {
        Ok(_) => Ok((
            StatusCode::CREATED,
            Json(json!({
                "asset_id": asset_id,
                "bytes": total,
                "media_type": media_type,
                "sha256": sha256,
            })),
        )),
        Err(error) => {
            let _ = tokio::fs::remove_file(&final_path).await;
            Err(error.into())
        }
    }
}

pub async fn download_asset(
    State(state): State<AppState>,
    Path(asset_id): Path<Uuid>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let actor = require_actor(&state, &headers).await?;
    let record =
        campuslingo_storage::assets::asset_for_read(&state.app_pool, actor, asset_id).await?;

    let path = state.settings.asset_dir.join(record.id.to_string());
    let file = match tokio::fs::File::open(&path).await {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("asset metadata exists without file: {}", record.id);
            return Err(ApiError::Internal);
        }
        Err(error) => {
            eprintln!("asset open failed: {error}");
            return Err(ApiError::Internal);
        }
    };
    let metadata = file.metadata().await.map_err(|error| {
        eprintln!("asset metadata read failed: {error}");
        ApiError::Internal
    })?;
    if metadata.len() != u64::try_from(record.bytes).unwrap_or(u64::MAX) {
        eprintln!("asset size mismatch: {}", record.id);
        return Err(ApiError::Internal);
    }

    let disposition = if matches!(
        record.media_type.as_str(),
        "application/pdf" | "image/png" | "image/jpeg" | "image/webp"
    ) {
        "inline"
    } else {
        "attachment"
    };
    let mut response = (StatusCode::OK, Body::from_stream(ReaderStream::new(file))).into_response();
    let response_headers = response.headers_mut();
    response_headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&record.media_type).map_err(|_| ApiError::Internal)?,
    );
    response_headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&record.bytes.to_string()).map_err(|_| ApiError::Internal)?,
    );
    response_headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static(disposition),
    );
    response_headers.insert(
        header::ETAG,
        HeaderValue::from_str(&format!("\"{}\"", record.sha256)).map_err(|_| ApiError::Internal)?,
    );
    response_headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response_headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    Ok(response)
}
