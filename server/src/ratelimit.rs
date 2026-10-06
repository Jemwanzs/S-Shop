//! Per-client rate limits for unauthenticated endpoints (sign-in, access requests, ordering portal).
//! In memory, sliding window — fine for one replica (see docs/production-readiness-2026-10.md before scaling out).
//! Requests without a forwarded client address (local development, CI) are not limited: behind Railway's proxy every
//! request carries one.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::error::{AppError, AppResult};

#[derive(Default)]
pub struct Limiter {
    hits: Mutex<HashMap<(String, &'static str), VecDeque<Instant>>>,
}

impl Limiter {
    /// Allows at most `max` requests per `window` for this client and bucket.
    pub fn check(&self, client: &str, bucket: &'static str, max: usize, window: Duration) -> AppResult<()> {
        if client.is_empty() {
            return Ok(());
        }
        let now = Instant::now();
        let mut hits = self.hits.lock().unwrap_or_else(|p| p.into_inner());
        if hits.len() > 50_000 {
            hits.retain(|_, q| q.back().is_some_and(|t| now.duration_since(*t) < Duration::from_secs(3600)));
        }
        let q = hits.entry((client.to_string(), bucket)).or_default();
        while q.front().is_some_and(|t| now.duration_since(*t) >= window) {
            q.pop_front();
        }
        if q.len() >= max {
            let wait = window.saturating_sub(now.duration_since(*q.front().unwrap_or(&now))).as_secs().max(1);
            return Err(AppError::RateLimited(wait));
        }
        q.push_back(now);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sliding_window() {
        let l = Limiter::default();
        for _ in 0..3 {
            assert!(l.check("1.2.3.4", "login", 3, Duration::from_secs(60)).is_ok());
        }
        assert!(l.check("1.2.3.4", "login", 3, Duration::from_secs(60)).is_err());
        assert!(l.check("5.6.7.8", "login", 3, Duration::from_secs(60)).is_ok(), "other clients unaffected");
        assert!(l.check("1.2.3.4", "access", 3, Duration::from_secs(60)).is_ok(), "buckets are separate");
        assert!(l.check("", "login", 0, Duration::from_secs(60)).is_ok(), "no client address: not limited");
    }
}
