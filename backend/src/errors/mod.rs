//! Centralised error type. Every handler returns `Result<_, AppError>` and
//! errors are rendered with a single, consistent JSON envelope:
//!
//! ```json
//! { "error": { "code": "not_found", "message": "connection not found", "details": null } }
//! ```

use axum::{
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

/// Structured error raised by a user's target database while running SQL.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct SqlErrorInfo {
    pub message: String,
    pub sqlstate: Option<String>,
    /// 1-based character offset into the submitted SQL, when reported.
    pub position: Option<u32>,
    pub detail: Option<String>,
    pub hint: Option<String>,
    pub execution_time_ms: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("request validation failed")]
    Validation(Vec<FieldError>),
    #[error("{0}")]
    BadRequest(String),
    #[error("authentication required")]
    Unauthorized,
    #[error("{0}")]
    Forbidden(String),
    #[error("{0} not found")]
    NotFound(&'static str),
    #[error("{0}")]
    Conflict(String),
    #[error("too many requests, retry later")]
    RateLimited { retry_after_secs: u64 },
    #[error("{}", .0.message)]
    Sql(SqlErrorInfo),
    #[error("{0}")]
    TargetUnavailable(String),
    #[error("query exceeded the {timeout_secs}s execution timeout")]
    QueryTimeout { timeout_secs: u64, execution_time_ms: u64 },
    #[error("query was cancelled")]
    QueryCancelled { execution_time_ms: u64 },
    #[error("internal server error")]
    Internal(String),
}

impl AppError {
    pub fn validation(field: impl Into<String>, message: impl Into<String>) -> Self {
        AppError::Validation(vec![FieldError {
            field: field.into(),
            message: message.into(),
        }])
    }

    pub fn internal(context: impl std::fmt::Display) -> Self {
        AppError::Internal(context.to_string())
    }

    pub fn status(&self) -> StatusCode {
        match self {
            AppError::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            // A failing user query is a client-side problem, not a server fault.
            AppError::Sql(_) => StatusCode::BAD_REQUEST,
            AppError::TargetUnavailable(_) => StatusCode::BAD_GATEWAY,
            AppError::QueryTimeout { .. } => StatusCode::REQUEST_TIMEOUT,
            AppError::QueryCancelled { .. } => StatusCode::CONFLICT,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            AppError::Validation(_) => "validation_error",
            AppError::BadRequest(_) => "bad_request",
            AppError::Unauthorized => "unauthorized",
            AppError::Forbidden(_) => "forbidden",
            AppError::NotFound(_) => "not_found",
            AppError::Conflict(_) => "conflict",
            AppError::RateLimited { .. } => "rate_limited",
            AppError::Sql(_) => "sql_error",
            AppError::TargetUnavailable(_) => "connection_error",
            AppError::QueryTimeout { .. } => "query_timeout",
            AppError::QueryCancelled { .. } => "query_cancelled",
            AppError::Internal(_) => "internal_error",
        }
    }

    fn details(&self) -> serde_json::Value {
        match self {
            AppError::Validation(fields) => json!({ "fields": fields }),
            AppError::Sql(info) => json!(info),
            AppError::RateLimited { retry_after_secs } => {
                json!({ "retry_after_secs": retry_after_secs })
            }
            AppError::QueryTimeout {
                timeout_secs,
                execution_time_ms,
            } => json!({ "timeout_secs": timeout_secs, "execution_time_ms": execution_time_ms }),
            AppError::QueryCancelled { execution_time_ms } => {
                json!({ "execution_time_ms": execution_time_ms })
            }
            _ => serde_json::Value::Null,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        if let AppError::Internal(ref context) = self {
            // Details stay in the logs; clients get a generic message.
            tracing::error!(error = %context, "internal error");
        }
        let body = Json(json!({
            "error": {
                "code": self.code(),
                "message": self.to_string(),
                "details": self.details(),
            }
        }));
        let mut response = (status, body).into_response();
        if let AppError::RateLimited { retry_after_secs } = self {
            if let Ok(value) = HeaderValue::from_str(&retry_after_secs.to_string()) {
                response.headers_mut().insert(header::RETRY_AFTER, value);
            }
        }
        response
    }
}

/// Errors from the *metadata* database. Constraint violations become client
/// errors; everything else is an internal error.
impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match &err {
            sqlx::Error::RowNotFound => AppError::NotFound("resource"),
            sqlx::Error::Database(db) => match db.code().as_deref() {
                Some("23505") => AppError::Conflict("a resource with the same name already exists".into()),
                Some("23503") => AppError::BadRequest("a referenced resource does not exist".into()),
                Some("23514") | Some("22001") => {
                    AppError::BadRequest("a field value is out of the allowed range".into())
                }
                _ => AppError::Internal(format!("metadata database error: {}", db.message())),
            },
            sqlx::Error::PoolTimedOut => {
                AppError::Internal("metadata database pool timed out".into())
            }
            // Avoid echoing configuration errors, which may include connection details.
            sqlx::Error::Configuration(_) => {
                AppError::Internal("metadata database misconfigured".into())
            }
            other => AppError::Internal(format!("metadata database error: {other}")),
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_codes_and_codes_are_stable() {
        assert_eq!(AppError::Unauthorized.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(AppError::NotFound("x").code(), "not_found");
        assert_eq!(
            AppError::validation("name", "required").status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(AppError::Sql(SqlErrorInfo::default()).code(), "sql_error");
    }

    #[test]
    fn internal_errors_do_not_leak_details() {
        let err = AppError::internal("secret context");
        assert_eq!(err.to_string(), "internal server error");
    }

    #[test]
    fn rate_limited_sets_retry_after() {
        let res = AppError::RateLimited { retry_after_secs: 7 }.into_response();
        assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(res.headers().get(header::RETRY_AFTER).unwrap(), "7");
    }
}
