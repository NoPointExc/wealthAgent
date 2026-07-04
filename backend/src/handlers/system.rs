//! System handlers: the destructive full-account reset.

use std::sync::Arc;

use axum::{extract::State, Json};

use crate::{auth::AuthUser, db, encryption, error::AppError, AppState};

pub async fn reset_database(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    // Destructive: wipes all data and revokes Plaid items. Never allow a leaked
    // API token / OAuth grant to do this.
    auth.require_web_session()?;
    let uid = &auth.user_id;
    let pool = &state.pool;
    tracing::warn!(user_id = %uid, "reset_database called");

    // Revoke Plaid items at Plaid's side before deleting rows
    let items = db::get_user_plaid_items(pool, uid).await?;
    for item in &items {
        if let Some(blob) = &item.access_token_enc {
            if let Ok(token) = encryption::decrypt_token(&state.encryption_key, blob) {
                if let Err(e) = state.plaid.remove_item(&token).await {
                    tracing::warn!(item_id = %item.id, "Plaid /item/remove failed: {}", e);
                }
            }
        }
        db::revoke_plaid_item(pool, &item.id).await?;
    }

    // Delete data in dependency order
    db::delete_user_snapshots(pool, uid).await?;
    db::delete_user_holdings(pool, uid).await?;
    db::delete_user_transactions(pool, uid).await?;
    db::delete_user_accounts(pool, uid).await?;
    db::delete_user_plaid_items(pool, uid).await?;
    db::delete_user_saved_searches(pool, uid).await?;
    // Drop the privacy keypair too — reset is the documented recovery path for
    // a lost passphrase, and keeping the old key would lock all re-synced data.
    db::delete_user_privacy_keys(pool, uid).await?;
    state.unlocked_keys.remove(uid).await;

    Ok(Json(serde_json::json!({ "status": "success", "message": "Your data has been reset" })))
}
