//! HTTP endpoints for privacy encryption (status/setup/unlock/lock) and the
//! background sweep that seals pre-existing plaintext rows.

use axum::{extract::State, Json};
use chacha20poly1305::aead::OsRng;
use rand::RngCore;
use std::sync::Arc;
use x25519_dalek::{PublicKey, StaticSecret};

use crate::{auth::AuthUser, db, error::AppError, models::SuccessResponse, AppState};

use super::context::user_crypto;
use super::crypto::{derive_passphrase_kek, seal, unwrap_secret, wrap_secret};

const MIN_PASSPHRASE_LEN: usize = 12;

#[derive(serde::Deserialize)]
pub struct PassphraseRequest {
    pub passphrase: String,
}

pub async fn privacy_status(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<SuccessResponse<serde_json::Value>>, AppError> {
    let crypto = user_crypto(&state, &auth).await?;
    Ok(Json(SuccessResponse {
        status: "success".to_string(),
        data: serde_json::json!({
            "enabled": state.privacy_enabled,
            "configured": !crypto.is_plaintext(),
            "unlocked": crypto.secret().is_some(),
        }),
    }))
}

/// Seal every not-yet-sealed row for a user (needs only the public key).
/// Idempotent — the queries return only rows without ciphertext — so it also
/// recovers from an earlier sweep that was interrupted partway.
pub async fn seal_unsealed_rows(
    state: &Arc<AppState>,
    user_id: &str,
    pk_bytes: &[u8; 32],
) -> Result<(usize, usize, usize), AppError> {
    let mut sealed_accounts = 0usize;
    for (id, name, custom_name) in db::list_account_rows_for_privacy(&state.pool, user_id).await? {
        let name_enc = seal(pk_bytes, name.as_bytes())?;
        let custom_enc = custom_name.map(|c| seal(pk_bytes, c.as_bytes())).transpose()?;
        db::seal_account_row(&state.pool, &id, name_enc, custom_enc).await?;
        sealed_accounts += 1;
    }
    let mut sealed_txns = 0usize;
    for (id, raw, merchant, note) in db::list_txn_rows_for_privacy(&state.pool, user_id).await? {
        let raw_enc = seal(pk_bytes, raw.as_bytes())?;
        let merchant_enc = merchant.map(|m| seal(pk_bytes, m.as_bytes())).transpose()?;
        let note_enc = note.map(|n| seal(pk_bytes, n.as_bytes())).transpose()?;
        db::seal_txn_row(&state.pool, &id, raw_enc, merchant_enc, note_enc).await?;
        sealed_txns += 1;
    }
    // Investment txns and holdings seal per-field: rows sealed by an earlier
    // version have name_enc set but plaintext symbol/security_name, so the
    // list queries project only the fields that still need sealing.
    for (id, name, symbol, security_name) in db::list_inv_txn_rows_for_privacy(&state.pool, user_id).await? {
        let name_enc = name.map(|n| seal(pk_bytes, n.as_bytes())).transpose()?;
        let symbol_enc = symbol.map(|s| seal(pk_bytes, s.as_bytes())).transpose()?;
        let security_name_enc = security_name.map(|s| seal(pk_bytes, s.as_bytes())).transpose()?;
        db::seal_inv_txn_row(&state.pool, &id, name_enc, symbol_enc, security_name_enc).await?;
        sealed_txns += 1;
    }
    let mut sealed_holdings = 0usize;
    for (id, symbol, name) in db::list_holding_rows_for_privacy(&state.pool, user_id).await? {
        let symbol_enc = symbol.map(|s| seal(pk_bytes, s.as_bytes())).transpose()?;
        let name_enc = name.map(|n| seal(pk_bytes, n.as_bytes())).transpose()?;
        db::seal_holding_row(&state.pool, id, symbol_enc, name_enc).await?;
        sealed_holdings += 1;
    }
    Ok((sealed_accounts, sealed_txns, sealed_holdings))
}

/// Run the sweep on a detached task: axum cancels a handler future when the
/// client disconnects, which must not leave rows half-sealed.
fn spawn_seal_sweep(state: Arc<AppState>, user_id: String, pk_bytes: [u8; 32]) {
    tokio::spawn(async move {
        match seal_unsealed_rows(&state, &user_id, &pk_bytes).await {
            Ok((a, t, h)) if a + t + h > 0 => {
                tracing::info!(user_id = %user_id, sealed_accounts = a, sealed_txns = t, sealed_holdings = h, "Privacy seal sweep complete");
            }
            Ok(_) => {}
            Err(e) => tracing::error!(user_id = %user_id, "Privacy seal sweep failed: {:?}", e),
        }
    });
}

/// Opt a user in: generate the keypair, wrap the private key under the
/// passphrase, and seal all existing rows. Web session only — a leaked API
/// token must not be able to re-key an account.
pub async fn privacy_setup(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<PassphraseRequest>,
) -> Result<Json<SuccessResponse<serde_json::Value>>, AppError> {
    if !state.privacy_enabled {
        return Err(AppError::Forbidden(
            "Privacy encryption is disabled on this deployment (set PRIVACY_ENCRYPTION=on).".to_string(),
        ));
    }
    if auth.claims.is_none() {
        return Err(AppError::Forbidden("Privacy setup requires a web session, not an API token.".to_string()));
    }
    if req.passphrase.len() < MIN_PASSPHRASE_LEN {
        return Err(AppError::BadRequest(format!("Passphrase must be at least {MIN_PASSPHRASE_LEN} characters.")));
    }
    if db::get_privacy_keys(&state.pool, &auth.user_id).await?.is_some() {
        return Err(AppError::Forbidden("Privacy encryption is already configured for this account.".to_string()));
    }

    let sk = StaticSecret::random_from_rng(OsRng);
    let pk = PublicKey::from(&sk);
    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    let kek = derive_passphrase_kek(&req.passphrase, &salt)?;
    let wrapped = wrap_secret(&kek, &sk.to_bytes())?;

    db::insert_privacy_keys(&state.pool, &auth.user_id, pk.as_bytes(), &wrapped, &salt).await?;

    // Report how much there is to seal, then seal on a detached task — if the
    // browser aborts this request mid-sweep the sealing still completes.
    let pk_bytes = pk.to_bytes();
    let sealed_accounts = db::list_account_rows_for_privacy(&state.pool, &auth.user_id).await?.len();
    let sealed_txns = db::list_txn_rows_for_privacy(&state.pool, &auth.user_id).await?.len()
        + db::list_inv_txn_rows_for_privacy(&state.pool, &auth.user_id).await?.len();
    let sealed_holdings = db::list_holding_rows_for_privacy(&state.pool, &auth.user_id).await?.len();

    state.unlocked_keys.insert(&auth.user_id, sk.to_bytes()).await;
    tracing::info!(user_id = %auth.user_id, sealed_accounts, sealed_txns, sealed_holdings, "Privacy encryption enabled — sealing existing rows");
    spawn_seal_sweep(state.clone(), auth.user_id.clone(), pk_bytes);

    Ok(Json(SuccessResponse {
        status: "success".to_string(),
        data: serde_json::json!({
            "sealed_accounts": sealed_accounts,
            "sealed_transactions": sealed_txns,
            "sealed_holdings": sealed_holdings,
            "note": "Existing API tokens cannot decrypt your data — revoke and recreate them while unlocked. \
                     There is no passphrase recovery; a lost passphrase means wiping and re-syncing from your bank.",
        }),
    }))
}

pub async fn privacy_unlock(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<PassphraseRequest>,
) -> Result<Json<SuccessResponse<serde_json::Value>>, AppError> {
    if auth.claims.is_none() {
        return Err(AppError::Forbidden("Unlock requires a web session.".to_string()));
    }
    // Throttle online passphrase guessing: after repeated wrong passphrases the
    // user must wait out the cooldown window.
    if state.limiters.unlock.blocked(&auth.user_id).await {
        tracing::warn!(user_id = %auth.user_id, "Unlock blocked — too many failed passphrase attempts");
        return Err(AppError::TooManyRequests(
            "Too many failed unlock attempts. Try again in a few minutes.".to_string(),
        ));
    }
    let keys = db::get_privacy_keys(&state.pool, &auth.user_id).await?
        .ok_or_else(|| AppError::NotFound("Privacy encryption is not configured.".to_string()))?;

    let kek = derive_passphrase_kek(&req.passphrase, &keys.kdf_salt)?;
    let secret = match unwrap_secret(&kek, &keys.wrapped_private_key) {
        Ok(s) => s,
        Err(e) => {
            state.limiters.unlock.record_failure(&auth.user_id).await;
            return Err(e); // Unauthorized on wrong passphrase
        }
    };
    state.limiters.unlock.clear(&auth.user_id).await;
    state.unlocked_keys.insert(&auth.user_id, secret).await;

    // Safety net: seal any rows an interrupted setup sweep (or a bug) left in
    // plaintext. No-op when everything is already sealed.
    if let Ok(pk_bytes) = <[u8; 32]>::try_from(keys.public_key.as_slice()) {
        spawn_seal_sweep(state.clone(), auth.user_id.clone(), pk_bytes);
    }

    Ok(Json(SuccessResponse { status: "success".to_string(), data: serde_json::json!({ "unlocked": true }) }))
}

pub async fn privacy_lock(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<SuccessResponse<serde_json::Value>>, AppError> {
    state.unlocked_keys.remove(&auth.user_id).await;
    Ok(Json(SuccessResponse { status: "success".to_string(), data: serde_json::json!({ "unlocked": false }) }))
}
