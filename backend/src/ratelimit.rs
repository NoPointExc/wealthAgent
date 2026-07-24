//! Minimal in-memory rate limiting for the auth-sensitive endpoints.
//!
//! Two flavors:
//!   - [`RateLimiter`]: fixed-window counter per key (client IP) — caps cheap
//!     abuse of unauthenticated endpoints (/register, /token, /api/auth/google).
//!   - [`FailureLimiter`]: counts only *failed* attempts per key (user id) —
//!     throttles online passphrase guessing on /api/privacy/unlock without
//!     ever locking out a user who unlocks successfully many times.
//!
//! State is process-local (lost on restart) — acceptable: the goal is slowing
//! brute force, not accounting. Maps prune lazily and are size-capped so an
//! attacker rotating keys can't grow memory unboundedly.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

const MAX_TRACKED_KEYS: usize = 10_000;

fn prune(map: &mut HashMap<String, (u32, Instant)>, window: Duration, now: Instant) {
    if map.len() >= MAX_TRACKED_KEYS {
        map.retain(|_, (_, start)| now.duration_since(*start) < window);
    }
}

/// Fixed-window counter: at most `max` hits per `window` per key.
pub struct RateLimiter {
    max: u32,
    window: Duration,
    hits: Mutex<HashMap<String, (u32, Instant)>>,
}

impl RateLimiter {
    pub fn new(max: u32, window: Duration) -> Self {
        Self { max, window, hits: Mutex::new(HashMap::new()) }
    }

    /// Record a hit; returns false when the key is over its budget.
    pub async fn allow(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut map = self.hits.lock().await;
        prune(&mut map, self.window, now);
        let entry = map.entry(key.to_string()).or_insert((0, now));
        if now.duration_since(entry.1) >= self.window {
            *entry = (0, now);
        }
        entry.0 += 1;
        entry.0 <= self.max
    }
}

/// Counts failures per key; `blocked` once `max_failures` accumulate in the
/// window. A success clears the key.
pub struct FailureLimiter {
    max_failures: u32,
    window: Duration,
    failures: Mutex<HashMap<String, (u32, Instant)>>,
}

impl FailureLimiter {
    pub fn new(max_failures: u32, window: Duration) -> Self {
        Self { max_failures, window, failures: Mutex::new(HashMap::new()) }
    }

    pub async fn blocked(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut map = self.failures.lock().await;
        match map.get(key) {
            Some((count, start)) if now.duration_since(*start) < self.window => *count >= self.max_failures,
            Some(_) => {
                map.remove(key);
                false
            }
            None => false,
        }
    }

    pub async fn record_failure(&self, key: &str) {
        let now = Instant::now();
        let mut map = self.failures.lock().await;
        prune(&mut map, self.window, now);
        let entry = map.entry(key.to_string()).or_insert((0, now));
        if now.duration_since(entry.1) >= self.window {
            *entry = (0, now);
        }
        entry.0 += 1;
    }

    pub async fn clear(&self, key: &str) {
        self.failures.lock().await.remove(key);
    }
}

/// The app's limiter set, sized for a small self-hosted deployment.
pub struct Limiters {
    /// Failed unlock attempts per user: 5 wrong passphrases → 15 min cooldown.
    pub unlock: FailureLimiter,
    /// Google logins per IP.
    pub google: RateLimiter,
    /// OAuth /token requests per IP.
    pub token: RateLimiter,
    /// OAuth dynamic client registrations per IP.
    pub register: RateLimiter,
    /// Demo-session creations per IP (demo instance only).
    pub demo: RateLimiter,
}

impl Limiters {
    pub fn new() -> Self {
        Self {
            unlock: FailureLimiter::new(5, Duration::from_secs(15 * 60)),
            google: RateLimiter::new(10, Duration::from_secs(60)),
            token: RateLimiter::new(30, Duration::from_secs(60)),
            register: RateLimiter::new(5, Duration::from_secs(60 * 60)),
            demo: RateLimiter::new(3, Duration::from_secs(60 * 60)),
        }
    }
}

impl Default for Limiters {
    fn default() -> Self {
        Self::new()
    }
}

/// Best-effort client key for per-IP limiting. Caddy (the only ingress in the
/// shipped deployment) sets X-Forwarded-For; fall back to a shared bucket so
/// the limiter still bounds total throughput when the header is absent.
pub fn client_key(headers: &axum::http::HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rate_limiter_caps_within_window() {
        let rl = RateLimiter::new(3, Duration::from_secs(60));
        assert!(rl.allow("ip1").await);
        assert!(rl.allow("ip1").await);
        assert!(rl.allow("ip1").await);
        assert!(!rl.allow("ip1").await); // 4th hit rejected
        assert!(rl.allow("ip2").await); // other keys unaffected
    }

    #[tokio::test]
    async fn failure_limiter_blocks_after_max_and_clears_on_success() {
        let fl = FailureLimiter::new(2, Duration::from_secs(60));
        assert!(!fl.blocked("u1").await);
        fl.record_failure("u1").await;
        assert!(!fl.blocked("u1").await);
        fl.record_failure("u1").await;
        assert!(fl.blocked("u1").await);
        fl.clear("u1").await;
        assert!(!fl.blocked("u1").await);
    }

    #[tokio::test]
    async fn demo_limiter_allows_three_per_ip() {
        let l = Limiters::new();
        assert!(l.demo.allow("ip1").await);
        assert!(l.demo.allow("ip1").await);
        assert!(l.demo.allow("ip1").await);
        assert!(!l.demo.allow("ip1").await); // 4th within the hour rejected
        assert!(l.demo.allow("ip2").await); // other IPs unaffected
    }

    #[test]
    fn client_key_takes_first_forwarded_hop() {
        let mut h = axum::http::HeaderMap::new();
        h.insert("x-forwarded-for", "203.0.113.9, 10.0.0.1".parse().unwrap());
        assert_eq!(client_key(&h), "203.0.113.9");
        assert_eq!(client_key(&axum::http::HeaderMap::new()), "unknown");
    }
}
