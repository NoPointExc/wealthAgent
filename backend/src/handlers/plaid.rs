//! Plaid link/exchange/sync handlers, plus the detached-sync helper shared
//! with the MCP `wealth_sync` tool.

use std::sync::Arc;

use axum::{extract::State, Json};

use crate::plaid::sync;
use crate::{auth::AuthUser, db, encryption, error::AppError, AppState};

pub async fn create_link_token(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    // Linking runs through Plaid Link in the browser; tokens have no use for it.
    auth.require_web_session()?;
    let token = state.plaid.create_link_token(&auth.user_id).await?;
    Ok(Json(serde_json::json!({ "link_token": token })))
}

#[derive(serde::Deserialize)]
pub struct ExchangeRequest { public_token: String }

pub async fn exchange_public_token(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<ExchangeRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    auth.require_web_session()?;
    let access_token = state.plaid.exchange_public_token(&req.public_token).await
        .map_err(|e| { tracing::error!(user_id = %auth.user_id, "Plaid token exchange failed: {}", e); e })?;
    let encrypted = encryption::encrypt_token(&state.encryption_key, &access_token)?;

    let item_id = uuid::Uuid::new_v4().to_string();
    db::insert_plaid_item(&state.pool, &item_id, &encrypted, &auth.user_id).await?;

    // The initial import can outlive the Plaid Link UI and any reverse-proxy
    // timeout, and axum cancels this handler future if the client gives up —
    // so run it detached and let the frontend poll /api/plaid/sync_status.
    spawn_detached_sync(&state, &auth.user_id, vec![(item_id.clone(), access_token)]).await;

    tracing::info!(user_id = %auth.user_id, item_id = %item_id, "Plaid item linked — initial sync started");
    Ok(Json(serde_json::json!({ "status": "success", "item_id": item_id, "syncing": true })))
}

/// Run one user's Plaid syncs on a detached task, tracked in `active_syncs`
/// so /api/plaid/sync_status reflects it. Detached because sync can outlive
/// the global request timeout and any reverse-proxy timeout, and axum drops
/// a handler future when the client disconnects.
pub(crate) async fn spawn_detached_sync(
    state: &Arc<AppState>,
    user_id: &str,
    items: Vec<(String, String)>, // (item_id, decrypted access token)
) {
    state.active_syncs.lock().await
        .entry(user_id.to_string())
        .and_modify(|c| *c += 1)
        .or_insert(1);
    let sync_state = state.clone();
    let sync_user = user_id.to_string();
    tokio::spawn(async move {
        for (item_id, access_token) in &items {
            match sync::sync_item(&sync_state, access_token, item_id).await {
                Ok(()) => tracing::info!(user_id = %sync_user, item_id = %item_id, "Sync complete"),
                Err(e) => tracing::error!(user_id = %sync_user, item_id = %item_id, "Sync failed: {:?}", e),
            }
        }
        let mut syncs = sync_state.active_syncs.lock().await;
        if let Some(c) = syncs.get_mut(&sync_user) {
            *c = c.saturating_sub(1);
            if *c == 0 { syncs.remove(&sync_user); }
        }
    });
}

/// Polled by the frontend after linking: true while a detached initial sync
/// for this user is still importing.
pub async fn plaid_sync_status(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let syncing = state.active_syncs.lock().await.get(&auth.user_id).copied().unwrap_or(0) > 0;
    Ok(Json(serde_json::json!({ "status": "success", "data": { "syncing": syncing } })))
}

pub async fn sync_plaid_data(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    auth.require_scope("sync")?;
    let items = db::get_user_plaid_items(&state.pool, &auth.user_id).await?;

    // Decrypt up front so config problems still fail the request, then sync
    // detached — a large re-sync exceeds the 30s request timeout and would be
    // dropped mid-import if run inline. Progress is on /api/plaid/sync_status.
    let mut to_sync = Vec::with_capacity(items.len());
    for item in &items {
        let blob = item.access_token_enc.as_deref()
            .ok_or_else(|| AppError::InternalServerError("Item has no encrypted token".to_string()))?;
        let access_token = encryption::decrypt_token(&state.encryption_key, blob)?;
        to_sync.push((item.id.clone(), access_token));
    }
    let count = to_sync.len();
    if count > 0 {
        spawn_detached_sync(&state, &auth.user_id, to_sync).await;
    }
    Ok(Json(serde_json::json!({ "status": "success", "synced_count": count, "syncing": count > 0 })))
}
