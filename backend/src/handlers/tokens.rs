//! Personal API token and OAuth-grant management. Everything here is
//! web-session-only: a leaked token must not be able to manage tokens.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use base64::engine::{general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;

use crate::models::SuccessResponse;
use crate::{auth, auth::AuthUser, db, error::AppError, models, privacy, AppState};

pub async fn create_api_token(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<models::CreateTokenRequest>,
) -> Result<Json<SuccessResponse<models::CreateTokenResponse>>, AppError> {
    // Web session only — otherwise any leaked scoped PAT could mint itself a
    // fresh full-scope replacement.
    auth.require_web_session()?;
    // Generate raw token: wa_pat_ + 44 base64url chars (33 random bytes)
    let mut bytes = [0u8; 33];
    rand::thread_rng().fill_bytes(&mut bytes);
    let b64 = URL_SAFE_NO_PAD.encode(bytes);
    let raw_token = format!("wa_pat_{}", b64);
    let prefix = raw_token[7..15].to_string(); // 8 chars after wa_pat_

    let token_hash = auth::hash_token(&raw_token)?;
    let scopes = req.scopes.unwrap_or_else(|| "read,write,sync".to_string());
    let pat_id = format!("pat_{}", uuid::Uuid::new_v4());
    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::BadRequest("Token name required".to_string()));
    }

    // Privacy encryption: give the token its own wrapped copy of the user's
    // private key so agents can decrypt. Only possible while unlocked — a token
    // created against a locked account will see sealed fields as "[locked]".
    let crypto = privacy::user_crypto(&state, &auth).await?;
    if !crypto.is_plaintext() && crypto.secret().is_none() {
        return Err(AppError::Forbidden(
            "Unlock privacy encryption before creating an API token, or the token won't be able to read your data.".to_string(),
        ));
    }
    let wrapped_private_key = crypto.secret()
        .map(|sec| privacy::wrap_secret(&privacy::derive_token_kek(&raw_token), sec))
        .transpose()?;

    db::create_api_token(&state.pool, &pat_id, &auth.user_id, &name, &prefix, &token_hash, &scopes, wrapped_private_key).await?;

    tracing::info!(user_id = %auth.user_id, token_id = %pat_id, "API token created");
    Ok(Json(SuccessResponse {
        status: "success".to_string(),
        data: models::CreateTokenResponse { id: pat_id, token: raw_token, name, scopes },
    }))
}

pub async fn list_api_tokens(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<SuccessResponse<Vec<models::ApiTokenListItem>>>, AppError> {
    auth.require_web_session()?;
    let tokens = db::list_api_tokens(&state.pool, &auth.user_id).await?;
    Ok(Json(SuccessResponse { status: "success".to_string(), data: tokens }))
}

pub async fn revoke_api_token(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    auth.require_web_session()?;
    db::revoke_api_token(&state.pool, &id, &auth.user_id).await?;
    tracing::info!(user_id = %auth.user_id, token_id = %id, "API token revoked");
    Ok(Json(serde_json::json!({ "status": "success" })))
}

pub async fn list_oauth_grants(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<SuccessResponse<Vec<models::OAuthGrantListItem>>>, AppError> {
    auth.require_web_session()?;
    let grants = db::oauth_list_grants(&state.pool, &auth.user_id).await?;
    Ok(Json(SuccessResponse { status: "success".to_string(), data: grants }))
}

pub async fn revoke_oauth_grant(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(client_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    auth.require_web_session()?;
    db::oauth_revoke_grant(&state.pool, &auth.user_id, &client_id).await?;
    tracing::info!(user_id = %auth.user_id, %client_id, "OAuth grant revoked");
    Ok(Json(serde_json::json!({ "status": "success" })))
}
