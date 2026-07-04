//! WealthAgent backend library crate.
//!
//! Split into a library + two thin binaries (`main.rs` = HTTP server,
//! `bin/admin.rs` = admin CLI) so both share the same modules. SQL lives in
//! [`db`], HTTP handlers in [`handlers`], the Plaid client/models/sync in
//! [`plaid`], server wiring in [`server`].

pub mod auth;
pub mod capital_gains;
pub mod cli;
pub mod db;
pub mod encryption;
pub mod error;
pub mod handlers;
pub mod mcp;
pub mod models;
pub mod oauth;
pub mod plaid;
pub mod privacy;
pub mod ratelimit;
pub mod server;

use plaid::client::PlaidClient;
use sqlx::PgPool;

pub struct AppState {
    pub(crate) pool: PgPool,
    pub(crate) plaid: PlaidClient,
    pub(crate) jwt_secret: String,
    pub(crate) google_client_id: String,
    pub(crate) http_client: reqwest::Client,
    pub(crate) encryption_key: [u8; 32],
    /// Public origin of the deployment (e.g. https://app.texasnetworth.com).
    /// Used to build OAuth metadata, issuer, and token audience.
    pub(crate) public_url: String,
    /// PRIVACY_ENCRYPTION=on — allows users to opt into operator-blind
    /// per-user encryption (see `privacy` module).
    pub(crate) privacy_enabled: bool,
    /// Unlocked per-user private keys, held in memory with a TTL.
    pub(crate) unlocked_keys: privacy::KeyCache,
    /// user_id → number of detached initial syncs in flight. Lets the frontend
    /// poll /api/plaid/sync_status after linking instead of holding the
    /// exchange request open past every proxy/Link timeout.
    pub(crate) active_syncs: tokio::sync::Mutex<std::collections::HashMap<String, usize>>,
    /// In-memory throttles for the auth-sensitive endpoints (see `ratelimit`).
    pub(crate) limiters: ratelimit::Limiters,
}

// Preserve the crate-root surface that mcp.rs imports via `use crate::{…}`.
pub use db::inner_get_transactions;
pub use handlers::{inner_bulk_update, inner_get_capital_gains};
pub use plaid::sync::sync_item;
