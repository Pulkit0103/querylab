//! Operational metrics in the Prometheus text format.
//!
//! Request counts (by method and status class) and a latency histogram are
//! kept in atomics by a middleware; gauges (running executions, data jobs,
//! pool usage) are read when `/metrics` is scraped. Paths are not labels, so
//! cardinality stays fixed.

use std::{
    fmt::Write,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

use axum::{
    extract::{Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::state::AppState;

const METHODS: [&str; 5] = ["GET", "POST", "PUT", "DELETE", "OTHER"];
const CLASSES: [&str; 5] = ["1xx", "2xx", "3xx", "4xx", "5xx"];
/// Upper bounds in seconds.
const BUCKETS: [f64; 11] = [
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

pub struct Metrics {
    started: Instant,
    requests: [[AtomicU64; 5]; 5],
    buckets: [AtomicU64; BUCKETS.len()],
    latency_count: AtomicU64,
    /// Microseconds.
    latency_sum: AtomicU64,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            requests: Default::default(),
            buckets: Default::default(),
            latency_count: AtomicU64::new(0),
            latency_sum: AtomicU64::new(0),
        }
    }
}

impl Metrics {
    pub fn observe(&self, method: &str, status: u16, seconds: f64) {
        let m = METHODS.iter().position(|x| *x == method).unwrap_or(4);
        let c = ((status / 100).clamp(1, 5) - 1) as usize;
        self.requests[m][c].fetch_add(1, Ordering::Relaxed);
        for (i, bound) in BUCKETS.iter().enumerate() {
            if seconds <= *bound {
                self.buckets[i].fetch_add(1, Ordering::Relaxed);
            }
        }
        self.latency_count.fetch_add(1, Ordering::Relaxed);
        self.latency_sum
            .fetch_add((seconds * 1e6) as u64, Ordering::Relaxed);
    }

    /// The exposition text, with the given gauges appended.
    pub fn render(&self, gauges: &[(&str, &str, f64)]) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "# HELP querylab_http_requests_total HTTP requests by method and status class."
        );
        let _ = writeln!(out, "# TYPE querylab_http_requests_total counter");
        for (m, method) in METHODS.iter().enumerate() {
            for (c, class) in CLASSES.iter().enumerate() {
                let n = self.requests[m][c].load(Ordering::Relaxed);
                if n > 0 {
                    let _ = writeln!(
                        out,
                        "querylab_http_requests_total{{method=\"{method}\",status=\"{class}\"}} {n}"
                    );
                }
            }
        }
        let _ = writeln!(
            out,
            "# HELP querylab_http_request_duration_seconds Request latency."
        );
        let _ = writeln!(
            out,
            "# TYPE querylab_http_request_duration_seconds histogram"
        );
        for (i, bound) in BUCKETS.iter().enumerate() {
            let _ = writeln!(
                out,
                "querylab_http_request_duration_seconds_bucket{{le=\"{bound}\"}} {}",
                self.buckets[i].load(Ordering::Relaxed)
            );
        }
        let count = self.latency_count.load(Ordering::Relaxed);
        let _ = writeln!(
            out,
            "querylab_http_request_duration_seconds_bucket{{le=\"+Inf\"}} {count}"
        );
        let _ = writeln!(
            out,
            "querylab_http_request_duration_seconds_sum {}",
            self.latency_sum.load(Ordering::Relaxed) as f64 / 1e6
        );
        let _ = writeln!(out, "querylab_http_request_duration_seconds_count {count}");
        let mut all = vec![(
            "querylab_uptime_seconds",
            "Seconds since the API started.",
            self.started.elapsed().as_secs_f64(),
        )];
        all.extend_from_slice(gauges);
        for (name, help, value) in all {
            let _ = writeln!(out, "# HELP {name} {help}");
            let _ = writeln!(out, "# TYPE {name} gauge");
            let _ = writeln!(out, "{name} {value}");
        }
        let _ = writeln!(out, "# HELP querylab_build_info Build information.");
        let _ = writeln!(out, "# TYPE querylab_build_info gauge");
        let _ = writeln!(
            out,
            "querylab_build_info{{version=\"{}\"}} 1",
            env!("CARGO_PKG_VERSION")
        );
        out
    }
}

/// Middleware: counts every request and its latency.
pub async fn track(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let method = req.method().as_str().to_string();
    let started = Instant::now();
    let res = next.run(req).await;
    state.metrics.observe(
        &method,
        res.status().as_u16(),
        started.elapsed().as_secs_f64(),
    );
    res
}

fn authorized(headers: &HeaderMap, token: &str) -> bool {
    let Some(given) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    else {
        return false;
    };
    // Constant-time comparison.
    given.len() == token.len()
        && given
            .bytes()
            .zip(token.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
}

/// `GET /metrics`: 404 unless `METRICS_TOKEN` is set, 401 without it.
pub async fn handler(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(token) = state.config.metrics_token.as_deref() else {
        return crate::errors::AppError::NotFound("route").into_response();
    };
    if !authorized(&headers, token) {
        return crate::errors::AppError::Unauthorized.into_response();
    }
    let pool_size = f64::from(state.db.size());
    let pool_idle = state.db.num_idle() as f64;
    let gauges = [
        (
            "querylab_executions_running",
            "SQL executions in progress.",
            state.executions.running() as f64,
        ),
        (
            "querylab_data_jobs_active",
            "Imports and mock data runs not finished.",
            state.jobs.active() as f64,
        ),
        (
            "querylab_import_reserved_bytes",
            "Upload memory reserved by imports.",
            state.jobs.reserved_bytes() as f64,
        ),
        (
            "querylab_db_pool_connections",
            "Metadata pool connections.",
            pool_size,
        ),
        (
            "querylab_db_pool_idle",
            "Idle metadata pool connections.",
            pool_idle,
        ),
    ];
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        state.metrics.render(&gauges),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_counters_and_histogram() {
        let m = Metrics::default();
        m.observe("GET", 200, 0.003);
        m.observe("GET", 200, 0.2);
        m.observe("PATCH", 503, 3.0);
        let text = m.render(&[("querylab_test_gauge", "A gauge.", 2.0)]);
        assert!(
            text.contains("querylab_http_requests_total{method=\"GET\",status=\"2xx\"} 2"),
            "{text}"
        );
        assert!(text.contains("querylab_http_requests_total{method=\"OTHER\",status=\"5xx\"} 1"));
        assert!(text.contains("querylab_http_request_duration_seconds_bucket{le=\"0.005\"} 1"));
        assert!(text.contains("querylab_http_request_duration_seconds_bucket{le=\"0.25\"} 2"));
        assert!(text.contains("querylab_http_request_duration_seconds_bucket{le=\"+Inf\"} 3"));
        assert!(text.contains("querylab_test_gauge 2"));
        assert!(text.contains("querylab_build_info{version="));
    }

    #[test]
    fn token_check() {
        let mut h = HeaderMap::new();
        assert!(!authorized(&h, "secret-token-123456"));
        h.insert(
            header::AUTHORIZATION,
            "Bearer secret-token-123456".parse().unwrap(),
        );
        assert!(authorized(&h, "secret-token-123456"));
        assert!(!authorized(&h, "secret-token-123457"));
    }
}
