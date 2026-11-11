//! Request validation helpers and a validating JSON extractor.

use axum::{
    extract::{rejection::JsonRejection, FromRequest, Request},
    Json,
};
use serde::de::DeserializeOwned;

use crate::errors::{AppError, FieldError};

/// Implemented by request DTOs that need semantic validation after parsing.
pub trait Validate {
    fn validate(&self) -> Result<(), AppError>;
}

/// Accumulates field errors so clients see every problem at once.
#[derive(Default)]
pub struct Validator {
    errors: Vec<FieldError>,
}

impl Validator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn check(&mut self, ok: bool, field: &str, message: &str) -> &mut Self {
        if !ok {
            self.errors.push(FieldError {
                field: field.to_string(),
                message: message.to_string(),
            });
        }
        self
    }

    /// Character-length check (Unicode scalar values, not bytes).
    pub fn length(&mut self, field: &str, value: &str, min: usize, max: usize) -> &mut Self {
        let len = value.chars().count();
        if len < min || len > max {
            let message = if min == max {
                format!("must be exactly {min} characters")
            } else if min > 0 && len < min {
                if min == 1 {
                    "is required".to_string()
                } else {
                    format!("must be at least {min} characters")
                }
            } else {
                format!("must be at most {max} characters")
            };
            self.errors.push(FieldError {
                field: field.to_string(),
                message,
            });
        }
        self
    }

    /// Rejects control characters (including NUL) that are never valid in
    /// identifiers, names or connection parameters.
    pub fn no_control_chars(&mut self, field: &str, value: &str) -> &mut Self {
        self.check(
            !value.chars().any(char::is_control),
            field,
            "must not contain control characters",
        )
    }

    pub fn finish(&mut self) -> Result<(), AppError> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(AppError::Validation(std::mem::take(&mut self.errors)))
        }
    }
}

/// JSON body extractor that maps parse failures to the API error envelope and
/// runs [`Validate`] before the handler sees the value.
pub struct ValidatedJson<T>(pub T);

impl<S, T> FromRequest<S> for ValidatedJson<T>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(|rej: JsonRejection| AppError::BadRequest(rej.body_text()))?;
        value.validate()?;
        Ok(ValidatedJson(value))
    }
}

/// Like [`ValidatedJson`] but without semantic validation.
pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(|rej: JsonRejection| AppError::BadRequest(rej.body_text()))?;
        Ok(ApiJson(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_all_errors() {
        let err = Validator::new()
            .length("name", "", 1, 10)
            .length("host", &"x".repeat(300), 1, 253)
            .no_control_chars("db", "a\0b")
            .finish()
            .unwrap_err();
        match err {
            AppError::Validation(fields) => {
                let names: Vec<_> = fields.iter().map(|f| f.field.as_str()).collect();
                assert_eq!(names, ["name", "host", "db"]);
                assert_eq!(fields[0].message, "is required");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn length_counts_characters_not_bytes() {
        assert!(Validator::new().length("n", "ééé", 1, 3).finish().is_ok());
    }
}
