//! Session handlers: Google sign-in, logout, refresh, whoami.

use std::sync::Arc;

use axum::{
    extract::State,
    http::{header::SET_COOKIE, HeaderMap, HeaderValue},
    response::IntoResponse,
    Json,
};

use crate::models::SuccessResponse;
use crate::{auth, auth::AuthUser, db, error::AppError, models, AppState};

fn session_cookie(token: &str) -> String {
    let secure = if std::env::var("HTTPS_ENABLED").as_deref() == Ok("true") { "Secure; " } else { "" };
    // Cookie Max-Age matches the 30-day hard cap so the browser retains it across
    // sessions. The JWT exp (24 h sliding window) is the actual security boundary.
    format!("wa_session={}; HttpOnly; {}SameSite=Strict; Path=/; Max-Age=2592000", token, secure)
}

pub async fn auth_google(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<models::GoogleAuthRequest>,
) -> Result<impl IntoResponse, AppError> {
    if !state.limiters.google.allow(&crate::ratelimit::client_key(&headers)).await {
        return Err(AppError::TooManyRequests("Too many login attempts. Try again shortly.".to_string()));
    }
    let url = format!("https://oauth2.googleapis.com/tokeninfo?id_token={}", req.credential);
    let resp = state.http_client.get(&url).send().await?;
    if !resp.status().is_success() {
        return Err(AppError::Unauthorized);
    }
    let info: serde_json::Value = resp.json().await?;

    let aud = info["aud"].as_str().ok_or(AppError::Unauthorized)?;
    if aud != state.google_client_id { return Err(AppError::Unauthorized); }

    let iss = info["iss"].as_str().unwrap_or("");
    if iss != "accounts.google.com" && iss != "https://accounts.google.com" {
        return Err(AppError::Unauthorized);
    }
    if info["email_verified"].as_str() != Some("true") && info["email_verified"].as_bool() != Some(true) {
        return Err(AppError::Unauthorized);
    }

    let google_id = info["sub"].as_str()
        .ok_or_else(|| AppError::InternalServerError("Missing sub in Google response".to_string()))?
        .to_string();
    let email = info["email"].as_str().unwrap_or("").to_lowercase();
    let name = info["name"].as_str().unwrap_or(&email).to_string();

    // Invite-only gate: reject sign-ins whose email isn't on the allowlist.
    if !db::is_invited(&state.pool, &email).await? {
        tracing::warn!(%email, "Rejected login — not on invite allowlist");
        return Err(AppError::Forbidden(
            "WealthAgent is invite-only right now. Email support@texasnetworth.com with your Google email to request access.".to_string()
        ));
    }

    let new_id = uuid::Uuid::new_v4().to_string();
    db::upsert_user(&state.pool, &new_id, &google_id, &email, &name).await?;
    let actual_id = db::get_user_id_by_google(&state.pool, &google_id).await?;

    let token = auth::create_jwt(&actual_id, &state.jwt_secret)?;
    tracing::info!(user_id = %actual_id, "Login");

    let mut headers = HeaderMap::new();
    headers.insert(SET_COOKIE, HeaderValue::from_str(&session_cookie(&token))
        .map_err(|_| AppError::InternalServerError("Cookie header error".to_string()))?);

    Ok((headers, Json(serde_json::json!({
        "user": { "id": actual_id, "email": email, "name": name }
    }))))
}

pub async fn logout() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(SET_COOKIE,
        HeaderValue::from_static("wa_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0"));
    (headers, Json(serde_json::json!({ "status": "success" })))
}

pub async fn refresh_session(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let claims = auth.claims.ok_or(AppError::Unauthorized)?;
    let token = auth::renew_jwt(&claims, &state.jwt_secret)?;
    let mut headers = HeaderMap::new();
    headers.insert(SET_COOKIE, HeaderValue::from_str(&session_cookie(&token))
        .map_err(|_| AppError::InternalServerError("Cookie header error".to_string()))?);
    Ok((headers, Json(serde_json::json!({ "status": "ok" }))))
}

pub async fn whoami(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<SuccessResponse<models::WhoamiResponse>>, AppError> {
    let user = db::get_user(&state.pool, &auth.user_id).await?;
    Ok(Json(SuccessResponse {
        status: "success".to_string(),
        data: models::WhoamiResponse {
            user_id: user.id,
            email: user.email,
            name: user.name,
            scopes: auth.scopes,
        },
    }))
}
