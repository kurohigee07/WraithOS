//! Per-module quota: token bucket rate limiter + concurrency semaphore.
//! Manual implementation to avoid new dependency (governor) at this stage.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use shared_types::ModuleQuota;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use std::sync::Arc;

// ── Token bucket ────────────────────────────────────────────────────
struct BucketInner {
    tokens: f64,
    last_refill: Instant,
}

pub struct TokenBucket {
    inner: Mutex<BucketInner>,
    capacity: f64,
    refill_per_sec: f64,
}

impl TokenBucket {
    pub fn new(rate_per_sec: u32) -> Self {
        let cap = rate_per_sec.max(1) as f64;
        Self {
            inner: Mutex::new(BucketInner {
                tokens: cap,
                last_refill: Instant::now(),
            }),
            capacity: cap,
            refill_per_sec: cap,
        }
    }

    /// Returns Ok if a token was consumed, Err(retry_after_ms) if rate limited.
    pub fn try_acquire(&self) -> Result<(), u64> {
        if self.refill_per_sec <= 0.0 {
            return Ok(());
        }
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let now = Instant::now();
        let elapsed = now.duration_since(g.last_refill).as_secs_f64();
        if elapsed > 0.0 {
            let refill = elapsed * self.refill_per_sec;
            g.tokens = (g.tokens + refill).min(self.capacity);
            g.last_refill = now;
        }
        if g.tokens >= 1.0 {
            g.tokens -= 1.0;
            Ok(())
        } else {
            let needed = 1.0 - g.tokens;
            let wait_ms = ((needed / self.refill_per_sec) * 1000.0).ceil() as u64;
            Err(wait_ms.max(1))
        }
    }
}

// ── Module quota state ──────────────────────────────────────────────
pub struct ModuleQuotaState {
    rate: Option<TokenBucket>,
    semaphore: Option<Arc<Semaphore>>,
    acquire_timeout: Duration,
}

impl ModuleQuotaState {
    pub fn from_quota(quota: &ModuleQuota, acquire_timeout: Duration) -> Self {
        let rate = if quota.rate_limit_per_sec > 0 {
            Some(TokenBucket::new(quota.rate_limit_per_sec))
        } else {
            None
        };
        let semaphore = if quota.max_concurrent > 0 {
            Some(Arc::new(Semaphore::new(quota.max_concurrent as usize)))
        } else {
            None
        };
        Self { rate, semaphore, acquire_timeout }
    }

    pub fn try_acquire_rate(&self) -> Result<(), u64> {
        match &self.rate {
            Some(b) => b.try_acquire(),
            None => Ok(()),
        }
    }

    /// Acquire per-module concurrency permit. Returns Ok(None) if no per-module
    /// cap configured (only global pool applies). Err(()) on timeout / closed.
    pub async fn acquire_concurrency(
        &self,
    ) -> Result<Option<OwnedSemaphorePermit>, ()> {
        match &self.semaphore {
            Some(sem) => {
                let permit = tokio::time::timeout(
                    self.acquire_timeout,
                    sem.clone().acquire_owned(),
                )
                .await
                .map_err(|_| ())?
                .map_err(|_| ())?;
                Ok(Some(permit))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_bucket_refills() {
        let b = TokenBucket::new(10);
        for _ in 0..10 {
            assert!(b.try_acquire().is_ok());
        }
        assert!(b.try_acquire().is_err());
        std::thread::sleep(Duration::from_millis(150));
        assert!(b.try_acquire().is_ok());
    }

    #[tokio::test]
    async fn concurrency_limited() {
        let q = ModuleQuota {
            max_memory_mb: 0,
            max_concurrent: 1,
            rate_limit_per_sec: 0,
        };
        let state = ModuleQuotaState::from_quota(&q, Duration::from_millis(50));
        let p1 = state.acquire_concurrency().await.expect("first");
        assert!(state.acquire_concurrency().await.is_err());
        drop(p1);
        let _p2 = state.acquire_concurrency().await.expect("after release");
    }

    #[tokio::test]
    async fn no_quota_unlimited() {
        let q = ModuleQuota::default();
        let state = ModuleQuotaState::from_quota(&q, Duration::from_millis(50));
        assert!(state.try_acquire_rate().is_ok());
        for _ in 0..100 {
            assert!(state.try_acquire_rate().is_ok());
        }
        let p = state.acquire_concurrency().await.expect("no cap");
        assert!(p.is_none());
    }
}
