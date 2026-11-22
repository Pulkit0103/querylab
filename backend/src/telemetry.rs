//! Structured logging, plus an optional error-tracking hook: ERROR-level
//! events are posted as JSON to `ERROR_WEBHOOK_URL` (Slack-compatible
//! `text` field included), at most [`WEBHOOK_PER_MINUTE`] per minute.

use std::{
    collections::BTreeMap,
    fmt,
    time::{Duration, Instant},
};

use serde_json::{json, Value};
use tokio::sync::mpsc;
use tracing::{
    field::{Field, Visit},
    span, Event, Level, Subscriber,
};
use tracing_subscriber::{
    fmt as tfmt, layer::Context, layer::SubscriberExt, registry::LookupSpan,
    util::SubscriberInitExt, EnvFilter, Layer,
};

use crate::config::LogFormat;

pub const WEBHOOK_PER_MINUTE: usize = 30;
const QUEUE: usize = 100;

#[derive(Default)]
struct Fields(BTreeMap<String, String>);

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_string(), value.to_string());
    }
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.0
            .insert(field.name().to_string(), format!("{value:?}"));
    }
}

/// The request id of the enclosing `http` span.
struct RequestId(String);

struct WebhookLayer {
    tx: mpsc::Sender<Value>,
    environment: &'static str,
}

impl<S> Layer<S> for WebhookLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        let mut fields = Fields::default();
        attrs.record(&mut fields);
        if let (Some(rid), Some(span)) = (fields.0.remove("request_id"), ctx.span(id)) {
            span.extensions_mut().insert(RequestId(rid));
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let meta = event.metadata();
        // Never report the reporter's own HTTP stack (no feedback loops).
        if *meta.level() != Level::ERROR
            || ["reqwest", "hyper", "h2"]
                .iter()
                .any(|t| meta.target().starts_with(t))
        {
            return;
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        let message = fields.0.remove("message").unwrap_or_default();
        let request_id = ctx.event_scope(event).and_then(|scope| {
            scope
                .from_root()
                .find_map(|s| s.extensions().get::<RequestId>().map(|r| r.0.clone()))
        });
        let payload = json!({
            "text": format!("[querylab-api {}] {message}", self.environment),
            "service": "querylab-api",
            "environment": self.environment,
            "level": "error",
            "message": message,
            "target": meta.target(),
            "fields": fields.0,
            "request_id": request_id,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        // Never block logging: drop when the queue is full.
        let _ = self.tx.try_send(payload);
    }
}

async fn deliver(url: String, mut rx: mpsc::Receiver<Value>) {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(_) => return,
    };
    let mut window = Instant::now();
    let mut sent = 0usize;
    while let Some(payload) = rx.recv().await {
        if window.elapsed() >= Duration::from_secs(60) {
            window = Instant::now();
            sent = 0;
        }
        if sent >= WEBHOOK_PER_MINUTE {
            continue;
        }
        sent += 1;
        if let Err(err) = client.post(&url).json(&payload).send().await {
            tracing::warn!(error = %err, "error webhook delivery failed");
        }
    }
}

/// Initialises structured logging. `RUST_LOG` controls verbosity. Must run
/// inside the Tokio runtime when a webhook is configured.
pub fn init(format: &LogFormat, error_webhook: Option<&str>, production: bool) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("querylab_api=info,tower_http=info,sqlx=warn"));

    let webhook = error_webhook.map(|url| {
        let (tx, rx) = mpsc::channel(QUEUE);
        tokio::spawn(deliver(url.to_string(), rx));
        WebhookLayer {
            tx,
            environment: if production {
                "production"
            } else {
                "development"
            },
        }
    });

    let registry = tracing_subscriber::registry().with(filter).with(webhook);
    match format {
        LogFormat::Json => registry
            .with(
                tfmt::layer()
                    .json()
                    .flatten_event(true)
                    .with_current_span(true)
                    // The request span already carries all context.
                    .with_span_list(false),
            )
            .init(),
        LogFormat::Pretty => registry.with(tfmt::layer().compact()).init(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn error_events_are_queued_with_their_request_id() {
        let (tx, mut rx) = mpsc::channel(10);
        let layer = WebhookLayer {
            tx,
            environment: "development",
        };
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("http", request_id = "abc-123");
            let _g = span.enter();
            tracing::info!("not reported");
            tracing::error!(status = 500, "request failed");
        });
        let payload = rx.try_recv().expect("one event");
        assert_eq!(payload["message"], "request failed");
        assert_eq!(payload["request_id"], "abc-123");
        assert_eq!(payload["fields"]["status"], "500");
        assert!(rx.try_recv().is_err());
    }
}
