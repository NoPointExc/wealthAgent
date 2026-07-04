//! Plaid integration: the HTTP API client, its response models, and the
//! per-item sync pipeline that writes Plaid data into the DB.

pub mod client;
pub mod models;
pub mod sync;
