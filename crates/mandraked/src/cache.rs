//! Short-lived read cache for driver lists (ADR-0002, ADR-0011).
//!
//! Reads are cached for a few seconds; every write the daemon performs
//! clears the affected cache so the next read sees the change at once.

use std::{
    future::Future,
    sync::Mutex,
    time::{Duration, Instant},
};

/// A single cached value with a time to live.
pub struct TtlCache<T> {
    ttl: Duration,
    slot: Mutex<Option<(Instant, T)>>,
}

impl<T: Clone> TtlCache<T> {
    /// A cache that keeps values for `ttl`.
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            slot: Mutex::new(None),
        }
    }

    /// The cached value if fresh, else `fill`'s result, which is cached.
    pub async fn get_or<F, Fut, E>(&self, fill: F) -> Result<T, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        if let Some(v) = self.fresh() {
            return Ok(v);
        }
        let value = fill().await?;
        if let Ok(mut slot) = self.slot.lock() {
            *slot = Some((Instant::now(), value.clone()));
        }
        Ok(value)
    }

    fn fresh(&self) -> Option<T> {
        let slot = self.slot.lock().ok()?;
        let (at, v) = slot.as_ref()?;
        (at.elapsed() < self.ttl).then(|| v.clone())
    }

    /// Forget the cached value.
    pub fn clear(&self) {
        if let Ok(mut slot) = self.slot.lock() {
            *slot = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[tokio::test]
    async fn caches_until_cleared_or_expired() {
        let cache = TtlCache::new(Duration::from_secs(60));
        let fills = AtomicUsize::new(0);
        let fill = || async {
            fills.fetch_add(1, Ordering::SeqCst);
            Ok::<_, ()>(42)
        };
        assert_eq!(cache.get_or(fill).await, Ok(42));
        assert_eq!(cache.get_or(fill).await, Ok(42));
        assert_eq!(fills.load(Ordering::SeqCst), 1);
        cache.clear();
        assert_eq!(cache.get_or(fill).await, Ok(42));
        assert_eq!(fills.load(Ordering::SeqCst), 2);

        let short = TtlCache::new(Duration::from_millis(1));
        assert_eq!(short.get_or(fill).await, Ok(42));
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert_eq!(short.get_or(fill).await, Ok(42));
        assert_eq!(fills.load(Ordering::SeqCst), 4);
    }
}
