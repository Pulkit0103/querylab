//! Outgoing email behind a small provider abstraction: Resend in
//! production, the log in development, or disabled.

use std::time::Duration;

use serde_json::json;

use crate::{config::EmailConfig, errors::AppError};

pub struct Email<'a> {
    pub to: &'a str,
    pub subject: &'a str,
    pub text: String,
}

#[derive(Clone)]
pub struct Mailer {
    config: EmailConfig,
    http: reqwest::Client,
}

impl Mailer {
    pub fn new(config: EmailConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("HTTP client builds");
        Self { config, http }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.is_enabled()
    }

    pub async fn send(&self, email: Email<'_>) -> Result<(), AppError> {
        match &self.config {
            EmailConfig::Disabled => Err(AppError::BadRequest(
                "email delivery is not configured on this server".into(),
            )),
            EmailConfig::Log => {
                tracing::info!(to = email.to, subject = email.subject, body = %email.text, "email (log provider)");
                Ok(())
            }
            EmailConfig::Resend { api_key, from } => {
                let res = self
                    .http
                    .post("https://api.resend.com/emails")
                    .bearer_auth(api_key)
                    .json(&json!({
                        "from": from,
                        "to": [email.to],
                        "subject": email.subject,
                        "text": email.text,
                    }))
                    .send()
                    .await
                    .map_err(|e| AppError::internal(format!("email request failed: {e}")))?;
                if !res.status().is_success() {
                    let status = res.status();
                    let body = res.text().await.unwrap_or_default();
                    return Err(AppError::internal(format!(
                        "email provider returned {status}: {}",
                        body.chars().take(300).collect::<String>()
                    )));
                }
                Ok(())
            }
        }
    }
}
