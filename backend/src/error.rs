use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::fmt;

#[derive(Debug)]
pub enum AppError {
    InternalServerError(String),
    BadRequest(String),
    NotFound(String),
    PlaidError(String),
    Unauthorized,
    Forbidden(String),
    TooManyRequests(String),
    /// 402 — authenticated but no active subscription (BILLING=on paywall).
    PaymentRequired(String),
    /// 428 — Google sign-in valid but the user must accept the current Terms /
    /// Privacy Policy before an account is created or a session is issued.
    ConsentRequired(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::InternalServerError(msg) => write!(f, "Internal Server Error: {}", msg),
            AppError::BadRequest(msg) => write!(f, "Bad Request: {}", msg),
            AppError::NotFound(msg) => write!(f, "Not Found: {}", msg),
            AppError::PlaidError(msg) => write!(f, "Plaid Error: {}", msg),
            AppError::Unauthorized => write!(f, "Unauthorized"),
            AppError::Forbidden(msg) => write!(f, "Forbidden: {}", msg),
            AppError::TooManyRequests(msg) => write!(f, "Too Many Requests: {}", msg),
            AppError::PaymentRequired(msg) => write!(f, "Payment Required: {}", msg),
            AppError::ConsentRequired(msg) => write!(f, "Consent Required: {}", msg),
        }
    }
}

impl std::error::Error for AppError {}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::InternalServerError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::PlaidError(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized".to_string()),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            AppError::TooManyRequests(msg) => (StatusCode::TOO_MANY_REQUESTS, msg),
            AppError::PaymentRequired(msg) => (StatusCode::PAYMENT_REQUIRED, msg),
            AppError::ConsentRequired(msg) => (StatusCode::PRECONDITION_REQUIRED, msg),
        };

        let body = Json(json!({
            "status": "error",
            "message": message,
        }));

        (status, body).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        // Log detail server-side; never forward raw DB errors to the client
        tracing::error!("db error: {}", err);
        AppError::InternalServerError("A database error occurred".to_string())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(err: reqwest::Error) -> Self {
        AppError::InternalServerError(err.to_string())
    }
}
