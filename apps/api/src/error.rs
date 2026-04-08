use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use tracing::error;

/// Distinguishes public API failures from internal implementation details. The
/// distinction between what we log and what we return is a security boundary.
/// Internal details stay internal.
#[derive(Debug)]
pub enum ApiError {
    Unauthorized(&'static str),
    Forbidden(&'static str),
    BadRequest(String),
    NotFound(&'static str),
    Conflict(String),
    Internal(anyhow::Error),
}

#[derive(Serialize)]
struct ErrorEnvelope<'a> {
    error: ErrorBody<'a>,
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    code: &'a str,
    message: String,
}

impl ApiError {
    pub fn internal<E>(error_value: E) -> Self
    where
        E: Into<anyhow::Error>,
    {
        Self::Internal(error_value.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            ApiError::Unauthorized(message) => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                message.to_string(),
            ),
            ApiError::Forbidden(message) => {
                (StatusCode::FORBIDDEN, "forbidden", message.to_string())
            }
            ApiError::BadRequest(message) => (StatusCode::BAD_REQUEST, "bad_request", message),
            ApiError::NotFound(message) => {
                (StatusCode::NOT_FOUND, "not_found", message.to_string())
            }
            ApiError::Conflict(message) => (StatusCode::CONFLICT, "conflict", message),
            ApiError::Internal(error_value) => {
                error!(error = ?error_value, "internal api error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "internal server error".to_string(),
                )
            }
        };

        (
            status,
            Json(ErrorEnvelope {
                error: ErrorBody { code, message },
            }),
        )
            .into_response()
    }
}
