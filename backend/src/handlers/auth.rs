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

/// Public, unauthenticated: lets the frontend learn the deployment's mode
/// before login, so a single frontend image serves both prod and demo.
pub async fn get_config(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "demo_mode": state.demo_mode,
        "privacy_enabled": state.privacy_enabled,
        "billing_enabled": state.billing.is_some(),
    }))
}

pub async fn auth_google(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<models::GoogleAuthRequest>,
) -> Result<impl IntoResponse, AppError> {
    if state.demo_mode {
        return Err(AppError::Forbidden("Google sign-in is disabled in the demo.".to_string()));
    }
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
    // Paywalled deployments are open-signup instead — anyone may create an
    // account, and the subscription (enforced on every request) is the gate.
    if state.billing.is_none() && !db::is_invited(&state.pool, &email).await? {
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

/// Max concurrent ephemeral demo users; keeps disk/DB bounded between reaper runs.
const DEMO_USER_CAP: i64 = 500;

/// Demo-only: mint an ephemeral user pre-populated with a clone of the demo
/// template's sandbox data, and return a normal session cookie. 404 on
/// non-demo deployments so the route is invisible on prod.
pub async fn auth_demo(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    if !state.demo_mode {
        return Err(AppError::NotFound("Not found".to_string()));
    }
    if !state.limiters.demo.allow(&crate::ratelimit::client_key(&headers)).await {
        return Err(AppError::TooManyRequests(
            "Too many demo sessions from your network. Try again shortly.".to_string(),
        ));
    }
    if db::count_demo_users(&state.pool).await? >= DEMO_USER_CAP {
        return Err(AppError::TooManyRequests(
            "The demo is at capacity right now. Please try again in a little while.".to_string(),
        ));
    }

    let user_id = uuid::Uuid::new_v4().to_string();
    let short = &user_id[..8];
    let google_id = format!("demo:{user_id}");
    let email = format!("demo-{short}@demo.local");
    let name = "Demo User".to_string();
    db::create_demo_user(&state.pool, &user_id, &google_id, &email, &name).await?;

    db::clone_template_into(&state.pool, &user_id).await?;

    let token = auth::create_jwt(&user_id, &state.jwt_secret)?;
    tracing::info!(user_id = %user_id, "Demo session created");

    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(SET_COOKIE, HeaderValue::from_str(&session_cookie(&token))
        .map_err(|_| AppError::InternalServerError("Cookie header error".to_string()))?);

    Ok((resp_headers, Json(serde_json::json!({
        "user": { "id": user_id, "email": email, "name": name }
    }))))
}

/// DEV_LOGIN-only: mint a session for a fixed, non-owner test user and redirect
/// to the app, bypassing Google. For local browser testing (e.g. of the billing
/// paywall) where a real OAuth client isn't configured. 404 unless DEV_LOGIN=on;
/// the user is a plain paywalled account (not in OWNER_EMAILS), so the paywall
/// still applies. A GET so you can just visit the URL in a browser.
pub async fn dev_login(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    if !state.dev_login {
        return Err(AppError::NotFound("Not found".to_string()));
    }
    let (google_id, email, name) = ("dev:local", "dev@localhost", "Dev Tester");
    let new_id = uuid::Uuid::new_v4().to_string();
    db::upsert_user(&state.pool, &new_id, google_id, email, name).await?;
    let actual_id = db::get_user_id_by_google(&state.pool, google_id).await?;

    let token = auth::create_jwt(&actual_id, &state.jwt_secret)?;
    tracing::info!(user_id = %actual_id, "DEV_LOGIN session created");

    // Seed the localStorage user hint the SPA expects, then land on the app.
    let redirect = format!(
        "/?dev_user={}",
        urlencoding_min(&serde_json::json!({ "id": actual_id, "email": email, "name": name }).to_string())
    );
    let mut headers = HeaderMap::new();
    headers.insert(SET_COOKIE, HeaderValue::from_str(&session_cookie(&token))
        .map_err(|_| AppError::InternalServerError("Cookie header error".to_string()))?);
    headers.insert(axum::http::header::LOCATION, HeaderValue::from_str(&redirect)
        .map_err(|_| AppError::InternalServerError("Location header error".to_string()))?);
    Ok((axum::http::StatusCode::SEE_OTHER, headers))
}

/// Minimal percent-encoding for a query-value (space and the handful of chars
/// that break a URL). Enough for the dev-login redirect; not a general encoder.
fn urlencoding_min(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", c as u32 & 0xFF),
        })
        .collect()
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
