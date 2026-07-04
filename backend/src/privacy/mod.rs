//! Operator-blind privacy encryption (optional — enable with PRIVACY_ENCRYPTION=on).
//!
//! Each user who opts in gets an X25519 keypair. The public key stays in the DB
//! in the clear so the Plaid sync path can *seal* (encrypt) new data at any time,
//! including while the user is offline. The private key is stored only wrapped:
//!
//!   - under a KEK derived from the user's passphrase (Argon2id) — unwrapped at
//!     login/unlock and cached in server memory with a TTL, never persisted;
//!   - under a KEK derived from each personal API token's raw value — the server
//!     stores only the token's hash, so the DB alone cannot unwrap anything, but
//!     an agent presenting the token can.
//!
//! Sealed columns: transactions.raw_string/merchant_name/note,
//! accounts.name/custom_name, investment_transactions.name/symbol/
//! security_name, and holdings.symbol/name. Amounts, dates, and tags remain
//! plaintext so SQL aggregation keeps working; sentinel symbols (CASH/DEBT/
//! N/A) and synthetic cash/debt holdings rows also stay plaintext — they
//! reveal nothing. See README "Privacy encryption" for the threat model.
//!
//! Module layout: [`crypto`] holds the sealed-box and key-wrapping primitives,
//! [`cache`] the in-memory unlocked-key cache, [`context`] the per-request
//! [`UserCrypto`] plus field seal/reveal helpers and the [`Sealed`] trait, and
//! [`handlers`] the HTTP endpoints (setup/unlock/lock/status) and seal sweep.

mod cache;
mod context;
mod crypto;
mod handlers;

pub use cache::KeyCache;
pub use context::*;
pub use crypto::*;
pub use handlers::*;

/// Hard cap for the app-side search path (encrypted users can't use SQL ILIKE).
const APP_SEARCH_MAX_ROWS: i64 = 20_000;

pub(crate) fn app_search_max_rows() -> i64 {
    APP_SEARCH_MAX_ROWS
}
