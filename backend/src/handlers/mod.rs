//! Axum HTTP handlers, grouped by domain. All SQL is delegated to
//! [`crate::db`]; pure business logic lives in its own modules (e.g. the FIFO
//! engine in [`crate::capital_gains`]). The `inner_*` functions double as the
//! service layer shared with the MCP surface in [`crate::mcp`].

pub mod accounts;
pub mod auth;
pub mod gains;
pub mod plaid;
pub mod system;
pub mod tokens;
pub mod transactions;

pub use accounts::*;
pub use auth::*;
pub use gains::*;
pub use plaid::*;
pub use system::*;
pub use tokens::*;
pub use transactions::*;
