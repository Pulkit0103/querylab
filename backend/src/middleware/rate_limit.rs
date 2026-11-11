//! In-memory fixed-window rate limiter.
//!
//! Suitable for a single API instance. For horizontally scaled deployments,
//! back it with Redis (the interface is intentionally small to allow that).

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use crate::errors::AppError;

/// Entries are pruned when the map grows beyond this size.
const PRUNE_THRESHOLD: usize = 10_000;

struct Window {
    started: Instant,
    count: u32,
}

pub struct RateLimiter {
    limit: u32,
    period: Duration,
    windows: Mutex<HashMap<String, Window>>,
}

impl RateLimiter {
    pub fn new(limit: u32, period: Duration) -> Self {
        Self {
            limit,
            period,
            windows: Mutex::new(HashMap::new()),
        }
    }

    /// Records a hit for `key`; errors with `RateLimited` once the limit for
    /// the current window is exceeded.
    pub fn check(&self, key: &str) -> Result<(), AppError> {
        self.check_at(key, Instant::now())
    }

    fn check_at(&self, key: &str, now: Instant) -> Result<(), AppError> {
        if self.limit == 0 {
            return Ok(());
        }
        let mut windows = self.windows.lock().unwrap_or_else(|p| p.into_inner());
        if windows.len() > PRUNE_THRESHOLD {
            let period = self.period;
            windows.retain(|_, w| now.duration_since(w.started) < period);
        }
        let window = windows.entry(key.to_string()).or_insert(Window {
            started: now,
            count: 0,
        });
        if now.duration_since(window.started) >= self.period {
            window.started = now;
            window.count = 0;
        }
        window.count += 1;
        if window.count > self.limit {
            let elapsed = now.duration_since(window.started);
            let retry_after_secs = self.period.saturating_sub(elapsed).as_secs().max(1);
            return Err(AppError::RateLimited { retry_after_secs });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_within_window_and_resets() {
        let rl = RateLimiter::new(2, Duration::from_secs(60));
        let t0 = Instant::now();
        assert!(rl.check_at("ip", t0).is_ok());
        assert!(rl.check_at("ip", t0).is_ok());
        match rl.check_at("ip", t0 + Duration::from_secs(10)) {
            Err(AppError::RateLimited { retry_after_secs }) => assert_eq!(retry_after_secs, 50),
            other => panic!("{other:?}"),
        }
        // Other keys are independent.
        assert!(rl.check_at("other", t0).is_ok());
        // New window.
        assert!(rl.check_at("ip", t0 + Duration::from_secs(61)).is_ok());
    }

    #[test]
    fn zero_limit_disables() {
        let rl = RateLimiter::new(0, Duration::from_secs(1));
        for _ in 0..100 {
            assert!(rl.check("k").is_ok());
        }
    }
}
