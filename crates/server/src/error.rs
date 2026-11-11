use std::fmt;

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

use campuslingo_storage::StorageError;

#[derive(Debug)]
pub enum ApiError {
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict,
    TooManyRequests,
    Invalid(&'static str),
    ServiceUnavailable(&'static str),
    Internal,
}

impl From<StorageError> for ApiError {
    fn from(error: StorageError) -> Self {
        match error {
            StorageError::Unauthorized => Self::Unauthorized,
            StorageError::Forbidden => Self::Forbidden,
            StorageError::NotFound => Self::NotFound,
            StorageError::Conflict => Self::Conflict,
            StorageError::Invalid(_) => Self::Invalid("invalid request"),
            StorageError::Crypto => Self::Internal,
            StorageError::Database(error) => {
                eprintln!("database error: {error}");
                Self::Internal
            }
            StorageError::Migration(error) => {
                eprintln!("migration error: {error}");
                Self::Internal
            }
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Unauthorized => "authentication is required",
            Self::Forbidden => "the requested operation is not permitted",
            Self::NotFound => "the requested resource was not found",
            Self::Conflict => "the request conflicts with existing state",
            Self::TooManyRequests => "too many authentication attempts",
            Self::Invalid(detail) => detail,
            Self::ServiceUnavailable(detail) => detail,
            Self::Internal => "the request could not be completed",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ApiError {}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "authentication is required",
            ),
            Self::Forbidden => (
                StatusCode::FORBIDDEN,
                "forbidden",
                "the requested operation is not permitted",
            ),
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                "not_found",
                "the requested resource was not found",
            ),
            Self::Conflict => (
                StatusCode::CONFLICT,
                "conflict",
                "the request conflicts with existing state",
            ),
            Self::TooManyRequests => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "too many authentication attempts; retry later",
            ),
            Self::Invalid(detail) => (StatusCode::UNPROCESSABLE_ENTITY, "invalid", detail),
            Self::ServiceUnavailable(detail) => {
                (StatusCode::SERVICE_UNAVAILABLE, "unavailable", detail)
            }
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "the request could not be completed",
            ),
        };
        let mut response =
            (status, Json(json!({"error": code, "message": message}))).into_response();
        response
            .headers_mut()
            .insert("cache-control", "no-store".parse().expect("static header"));
        response
    }
}
