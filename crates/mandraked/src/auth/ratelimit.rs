//! Per-source login rate limiting (ADR-0007). In memory; resets on restart.

use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
    time::{Duration, Instant},
};

/// Sliding-window counter keyed by source address.
pub struct LoginLimiter {
    window: Duration,
    max: usize,
    hits: Mutex<HashMap<String, VecDeque<Instant>>>,
}

impl LoginLimiter {
    /// Allow `max` attempts per `window` per key.
    pub fn new(max: usize, window: Duration) -> Self {
        Self {
            window,
            max,
            hits: Mutex::new(HashMap::new()),
        }
    }

    /// The default: ten attempts a minute.
    pub fn default_login() -> Self {
        Self::new(10, Duration::from_secs(60))
    }

    /// Record an attempt for `key`. `Err(seconds)` says how long to wait.
    pub fn check(&self, key: &str) -> Result<(), u64> {
        self.check_at(key, Instant::now())
    }

    fn check_at(&self, key: &str, now: Instant) -> Result<(), u64> {
        let Ok(mut hits) = self.hits.lock() else {
            return Ok(());
        };
        if hits.len() > 10_000 {
            hits.retain(|_, q| {
                q.back()
                    .is_some_and(|t| now.duration_since(*t) < self.window)
            });
        }
        let queue = hits.entry(key.to_owned()).or_default();
        while queue
            .front()
            .is_some_and(|t| now.duration_since(*t) >= self.window)
        {
            queue.pop_front();
        }
        if queue.len() >= self.max {
            let retry = queue
                .front()
                .map_or(1, |t| {
                    self.window.saturating_sub(now.duration_since(*t)).as_secs()
                })
                .max(1);
            return Err(retry);
        }
        queue.push_back(now);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_within_the_window_and_recovers() {
        let limiter = LoginLimiter::new(3, Duration::from_secs(60));
        let t0 = Instant::now();
        assert!(limiter.check_at("a", t0).is_ok());
        assert!(limiter.check_at("a", t0).is_ok());
        assert!(limiter.check_at("a", t0).is_ok());
        assert_eq!(limiter.check_at("a", t0 + Duration::from_secs(10)), Err(50));
        assert!(limiter.check_at("b", t0).is_ok());
        assert!(limiter.check_at("a", t0 + Duration::from_secs(61)).is_ok());
    }
}
