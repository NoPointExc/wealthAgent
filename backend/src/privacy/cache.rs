//! In-memory cache of unlocked private keys, keyed by user id.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// How long an unlocked private key stays in server memory after unlock.
const UNLOCK_TTL: Duration = Duration::from_secs(12 * 60 * 60);

pub struct KeyCache(tokio::sync::RwLock<HashMap<String, ([u8; 32], Instant)>>);

impl KeyCache {
    pub fn new() -> Self {
        Self(tokio::sync::RwLock::new(HashMap::new()))
    }

    pub async fn insert(&self, user_id: &str, secret: [u8; 32]) {
        self.0.write().await.insert(user_id.to_string(), (secret, Instant::now() + UNLOCK_TTL));
    }

    pub async fn get(&self, user_id: &str) -> Option<[u8; 32]> {
        let now = Instant::now();
        {
            let map = self.0.read().await;
            match map.get(user_id) {
                Some((secret, exp)) if *exp > now => return Some(*secret),
                Some(_) => {} // expired — fall through to prune
                None => return None,
            }
        }
        self.0.write().await.remove(user_id);
        None
    }

    pub async fn remove(&self, user_id: &str) {
        self.0.write().await.remove(user_id);
    }
}

impl Default for KeyCache {
    fn default() -> Self {
        Self::new()
    }
}
