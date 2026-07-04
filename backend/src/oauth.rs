//! OAuth 2.1 authorization server for one-click MCP connection.
//!
//! Public clients only (PKCE/S256, no client secret). User authentication is
//! delegated to the existing Google-backed session cookie; an HTML consent screen
//! gates each grant. Access tokens are stateless JWTs (validated by `/mcp` without a
//! DB hit); authorization codes and refresh tokens are DB-backed and hashed.
//!
//! Endpoints:
//!   GET  /.well-known/oauth-protected-resource   (RFC 9728)
//!   GET  /.well-known/oauth-authorization-server (RFC 8414)
//!   POST /register                               (RFC 7591 dynamic client registration)
//!   GET  /authorize                              (consent) / POST /authorize (decision)
//!   POST /token                                  (code exchange + refresh rotation)

use std::sync::Arc;

use axum::{
    extract::{Form, Query, RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    Json,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{auth, db, AppState};

/// Scopes this resource server understands. Mirrors the PAT scope set.
const SCOPES: [&str; 3] = ["read", "write", "sync"];

/// Authorization codes are single-use and short-lived.
const CODE_TTL_SECS: i64 = 60;
/// A consent ticket must be acted on within this window.
const CONSENT_TTL_SECS: i64 = 600;
/// Access tokens are short-lived; revocation is via refresh-token removal.
const ACCESS_TTL_SECS: i64 = 3600;
/// Refresh tokens live 30 days (rotated on each use).
const REFRESH_TTL_SECS: i64 = 2_592_000;

const AC_PREFIX: &str = "wa_ac_";
const RT_PREFIX: &str = "wa_rt_";

// ── Small helpers ─────────────────────────────────────────────────────────────

/// Mint a random opaque token with a static prefix; returns (raw, 8-char lookup).
fn mint_token(prefix: &str) -> (String, String) {
    let mut bytes = [0u8; 33];
    rand::thread_rng().fill_bytes(&mut bytes);
    let raw = format!("{prefix}{}", URL_SAFE_NO_PAD.encode(bytes));
    let lookup = lookup_of(prefix, &raw);
    (raw, lookup)
}

/// The prefix-lookup key for a raw token: the 8 chars after the static prefix.
pub fn lookup_of(prefix: &str, raw: &str) -> String {
    raw.chars().skip(prefix.len()).take(8).collect()
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Percent-encode a query-parameter value (RFC 3986 unreserved set passes through).
fn pct_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Append query params to a redirect URI, handling existing `?`.
fn redirect_with(redirect_uri: &str, params: &[(&str, &str)]) -> String {
    let mut url = redirect_uri.to_string();
    let mut sep = if url.contains('?') { '&' } else { '?' };
    for (k, v) in params {
        url.push(sep);
        url.push_str(k);
        url.push('=');
        url.push_str(&pct_encode(v));
        sep = '&';
    }
    url
}

fn error_redirect(redirect_uri: &str, error: &str, state: Option<&str>) -> Response {
    let mut pairs: Vec<(&str, &str)> = vec![("error", error)];
    if let Some(s) = state {
        pairs.push(("state", s));
    }
    Redirect::to(&redirect_with(redirect_uri, &pairs)).into_response()
}

/// A standalone HTML error page for failures we must not redirect (untrusted
/// client / redirect_uri), or expired sessions.
fn error_page(msg: &str) -> Response {
    let html = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>WealthAgent</title></head>\
         <body style=\"font-family:system-ui,sans-serif;background:#020617;color:#e2e8f0;\
         display:flex;min-height:100vh;align-items:center;justify-content:center;margin:0\">\
         <div style=\"max-width:420px;padding:32px;text-align:center\">\
         <h1 style=\"font-size:18px;color:#f8717\">Could not authorize</h1>\
         <p style=\"color:#94a3b8;font-size:14px;line-height:1.5\">{}</p></div></body></html>",
        html_escape(msg)
    );
    (StatusCode::BAD_REQUEST, Html(html)).into_response()
}

/// Keep only recognised scopes (accepting space- or comma-delimited input);
/// default to the full set. Returns a comma-delimited string (internal form).
fn normalize_scope(requested: Option<&str>) -> String {
    let picked: Vec<&str> = match requested {
        Some(s) if !s.trim().is_empty() => s
            .split([' ', ','])
            .map(str::trim)
            .filter(|t| SCOPES.contains(t))
            .collect(),
        _ => SCOPES.to_vec(),
    };
    if picked.is_empty() {
        SCOPES.join(",")
    } else {
        picked.join(",")
    }
}

/// Build an RFC 6749 §5.2 / RFC 7591 §3.2.2 style JSON error response.
fn oauth_error(status: StatusCode, error: &str, description: &str) -> Response {
    (status, Json(json!({ "error": error, "error_description": description }))).into_response()
}

/// A redirect URI is acceptable if it is https, or a loopback http URI (native
/// apps / local dev), and carries no fragment (RFC 6749 §3.1.2).
fn is_valid_redirect_uri(uri: &str) -> bool {
    if uri.contains('#') {
        return false;
    }
    uri.starts_with("https://")
        || uri.starts_with("http://localhost")
        || uri.starts_with("http://127.0.0.1")
        || uri.starts_with("http://[::1]")
}

/// The canonical resource identifier (audience) for issued access tokens.
pub fn resource_url(state: &AppState) -> String {
    format!("{}/mcp", state.public_url)
}

/// RFC 9728 — tells the MCP client which authorization server protects `/mcp`.
pub async fn protected_resource_metadata(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(json!({
        "resource": resource_url(&state),
        "authorization_servers": [state.public_url],
        "scopes_supported": SCOPES,
        "bearer_methods_supported": ["header"],
    }))
}

/// RFC 8414 — advertises the authorize/token/registration endpoints and the
/// PKCE/grant capabilities the client must use.
pub async fn authorization_server_metadata(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let base = &state.public_url;
    Json(json!({
        "issuer": base,
        "authorization_endpoint": format!("{base}/authorize"),
        "token_endpoint": format!("{base}/token"),
        "registration_endpoint": format!("{base}/register"),
        "scopes_supported": SCOPES,
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
    }))
}

// ── Dynamic client registration (RFC 7591) ────────────────────────────────────

#[derive(Deserialize)]
pub struct RegisterRequest {
    #[serde(default)]
    redirect_uris: Vec<String>,
    client_name: Option<String>,
}

/// MCP clients (Claude Desktop, ChatGPT) self-register here to obtain a client_id.
/// We only issue public clients: PKCE-protected, no secret.
pub async fn register(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<RegisterRequest>,
) -> Response {
    // Unauthenticated by design (RFC 7591) — throttle so it can't be used to
    // fill the oauth_clients table.
    if !state.limiters.register.allow(&crate::ratelimit::client_key(&headers)).await {
        return oauth_error(StatusCode::TOO_MANY_REQUESTS, "slow_down", "too many registrations; retry later");
    }
    if req.redirect_uris.is_empty() {
        return oauth_error(StatusCode::BAD_REQUEST, "invalid_redirect_uri", "redirect_uris is required");
    }
    if let Some(bad) = req.redirect_uris.iter().find(|u| !is_valid_redirect_uri(u)) {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_redirect_uri",
            &format!("unsupported redirect_uri: {bad}"),
        );
    }

    let client_id = format!("client_{}", uuid::Uuid::new_v4());
    let client_name = req
        .client_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("MCP Client")
        .to_string();

    if let Err(e) = db::oauth_create_client(&state.pool, &client_id, &client_name, &req.redirect_uris).await {
        tracing::error!("DCR failed: {e}");
        return oauth_error(StatusCode::INTERNAL_SERVER_ERROR, "server_error", "could not register client");
    }

    tracing::info!(%client_id, %client_name, "Registered OAuth client");
    (
        StatusCode::CREATED,
        Json(json!({
            "client_id": client_id,
            "client_name": client_name,
            "redirect_uris": req.redirect_uris,
            "token_endpoint_auth_method": "none",
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "client_id_issued_at": chrono::Utc::now().timestamp(),
        })),
    )
        .into_response()
}

// ── Authorization endpoint + consent ──────────────────────────────────────────

/// Signed, short-lived ticket that carries the validated authorization request
/// from the consent GET to the decision POST. Binding to `sub` makes it a CSRF
/// token: the POST must arrive with the same session whose user approved it.
#[derive(Serialize, Deserialize)]
struct ConsentClaims {
    sub: String,
    client_id: String,
    redirect_uri: String,
    scope: String,
    resource: Option<String>,
    code_challenge: String,
    #[serde(default)]
    oauth_state: Option<String>,
    exp: usize,
    kind: String,
}

fn create_consent_ticket(state: &AppState, claims: &ConsentClaims) -> Option<String> {
    encode(
        &Header::default(),
        claims,
        &EncodingKey::from_secret(state.jwt_secret.as_bytes()),
    )
    .ok()
}

fn verify_consent_ticket(state: &AppState, token: &str) -> Option<ConsentClaims> {
    let mut v = Validation::new(Algorithm::HS256);
    v.validate_aud = false;
    v.required_spec_claims = ["exp"].into_iter().map(String::from).collect();
    decode::<ConsentClaims>(token, &DecodingKey::from_secret(state.jwt_secret.as_bytes()), &v)
        .ok()
        .map(|d| d.claims)
        .filter(|c| c.kind == "consent")
}

#[derive(Deserialize)]
pub struct AuthorizeQuery {
    response_type: Option<String>,
    client_id: Option<String>,
    redirect_uri: Option<String>,
    code_challenge: Option<String>,
    code_challenge_method: Option<String>,
    scope: Option<String>,
    state: Option<String>,
    resource: Option<String>,
}

/// GET /authorize — validate the request, ensure the user is logged in (bridging
/// to the SPA login if not), then render the consent screen.
pub async fn authorize(
    State(state): State<Arc<AppState>>,
    Query(q): Query<AuthorizeQuery>,
    RawQuery(raw): RawQuery,
    headers: HeaderMap,
) -> Response {
    // Params we must validate before trusting redirect_uri enough to redirect to it.
    let Some(client_id) = q.client_id.as_deref().filter(|c| !c.is_empty()) else {
        return error_page("Missing client_id.");
    };
    let Some(redirect_uri) = q.redirect_uri.as_deref().filter(|r| !r.is_empty()) else {
        return error_page("Missing redirect_uri.");
    };

    let client = match db::oauth_get_client(&state.pool, client_id).await {
        Ok(Some(c)) => c,
        Ok(None) => return error_page("Unknown client. Remove and re-add the connector so it can register again."),
        Err(e) => {
            tracing::error!("authorize: client lookup failed: {e}");
            return error_page("A server error occurred. Please try again.");
        }
    };
    if !client.redirect_uris.iter().any(|u| u == redirect_uri) {
        return error_page("This redirect URI is not registered for the client.");
    }

    // From here a valid redirect_uri lets us report errors back to the client.
    let st = q.state.as_deref();
    if q.response_type.as_deref() != Some("code") {
        return error_redirect(redirect_uri, "unsupported_response_type", st);
    }
    if q.code_challenge_method.as_deref() != Some("S256") {
        return error_redirect(redirect_uri, "invalid_request", st);
    }
    let Some(code_challenge) = q.code_challenge.clone().filter(|c| !c.is_empty()) else {
        return error_redirect(redirect_uri, "invalid_request", st);
    };
    let scope = normalize_scope(q.scope.as_deref());

    // Require a logged-in session. If absent, bounce through the SPA login,
    // carrying the original query so it returns here afterwards (same-site nav,
    // so the SameSite=Strict session cookie is then sent).
    let Some(sub) = auth::session_sub(&headers, &state.jwt_secret) else {
        let raw = raw.unwrap_or_default();
        return Redirect::to(&format!("/?{raw}&_consent=1")).into_response();
    };

    let ticket = ConsentClaims {
        sub,
        client_id: client_id.to_string(),
        redirect_uri: redirect_uri.to_string(),
        scope: scope.clone(),
        resource: q.resource.clone(),
        code_challenge,
        oauth_state: q.state.clone(),
        exp: (chrono::Utc::now().timestamp() + CONSENT_TTL_SECS) as usize,
        kind: "consent".to_string(),
    };
    let Some(ticket_jwt) = create_consent_ticket(&state, &ticket) else {
        return error_page("A server error occurred. Please try again.");
    };

    Html(consent_html(&client.client_name, &scope, &ticket_jwt)).into_response()
}

const SCOPE_BLURB: &[(&str, &str)] = &[
    ("read", "Read your accounts, balances, transactions and capital gains"),
    ("write", "Tag, note and rename — edit your transactions and accounts"),
    ("sync", "Refresh data from your linked banks"),
];

fn consent_html(client_name: &str, scope: &str, ticket: &str) -> String {
    let granted: Vec<&str> = scope.split(',').collect();
    let scope_items: String = SCOPE_BLURB
        .iter()
        .filter(|(k, _)| granted.contains(k))
        .map(|(_, desc)| format!("<li>{}</li>", html_escape(desc)))
        .collect();
    let client = html_escape(client_name);
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>Authorize {client} · WealthAgent</title>\
         <style>\
           body{{font-family:system-ui,-apple-system,sans-serif;background:#020617;color:#e2e8f0;margin:0;\
             display:flex;min-height:100vh;align-items:center;justify-content:center}}\
           .card{{width:100%;max-width:420px;margin:16px;background:#0f172a;border:1px solid #1e293b;\
             border-radius:18px;padding:32px}}\
           .brand{{display:flex;align-items:center;gap:8px;font-weight:800;font-size:18px;color:#60a5fa;margin-bottom:20px}}\
           h1{{font-size:18px;font-weight:700;margin:0 0 6px}}\
           p.sub{{color:#94a3b8;font-size:14px;margin:0 0 20px;line-height:1.5}}\
           .name{{color:#f1f5f9;font-weight:700}}\
           ul{{list-style:none;padding:0;margin:0 0 24px}}\
           li{{position:relative;padding:8px 0 8px 26px;font-size:13.5px;color:#cbd5e1;border-top:1px solid #1e293b}}\
           li:before{{content:'✓';position:absolute;left:0;color:#34d399;font-weight:700}}\
           .actions{{display:flex;gap:10px}}\
           button{{flex:1;padding:11px;border-radius:10px;font-size:14px;font-weight:700;cursor:pointer;border:1px solid transparent}}\
           .allow{{background:#2563eb;color:#fff}} .allow:hover{{background:#3b82f6}}\
           .deny{{background:transparent;color:#cbd5e1;border-color:#334155}} .deny:hover{{background:#1e293b}}\
         </style></head><body>\
         <div class=\"card\">\
           <div class=\"brand\">🛡 WealthAgent</div>\
           <h1>Authorize access</h1>\
           <p class=\"sub\"><span class=\"name\">{client}</span> is requesting access to your WealthAgent financial data. It will be able to:</p>\
           <ul>{scope_items}</ul>\
           <form method=\"post\" action=\"/authorize\">\
             <input type=\"hidden\" name=\"consent_ticket\" value=\"{ticket}\">\
             <div class=\"actions\">\
               <button class=\"deny\" type=\"submit\" name=\"decision\" value=\"deny\">Cancel</button>\
               <button class=\"allow\" type=\"submit\" name=\"decision\" value=\"allow\">Authorize</button>\
             </div>\
           </form>\
         </div></body></html>"
    )
}

#[derive(Deserialize)]
pub struct DecisionForm {
    consent_ticket: String,
    decision: String,
}

/// POST /authorize — the consent decision. Re-verifies the session and the
/// signed ticket (CSRF), then issues a single-use authorization code.
pub async fn authorize_decision(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Form(form): Form<DecisionForm>,
) -> Response {
    let Some(sub) = auth::session_sub(&headers, &state.jwt_secret) else {
        return error_page("Your session expired. Please sign in again and retry from your AI client.");
    };
    let Some(ticket) = verify_consent_ticket(&state, &form.consent_ticket) else {
        return error_page("This authorization request expired. Start again from your AI client.");
    };
    if ticket.sub != sub {
        return error_page("Session mismatch. Please retry from your AI client.");
    }

    if form.decision != "allow" {
        return error_redirect(&ticket.redirect_uri, "access_denied", ticket.oauth_state.as_deref());
    }

    let (raw_code, lookup) = mint_token(AC_PREFIX);
    let Ok(code_hash) = auth::hash_token(&raw_code) else {
        return error_page("A server error occurred. Please try again.");
    };
    let id = format!("ac_{}", uuid::Uuid::new_v4());
    if let Err(e) = db::oauth_insert_code(
        &state.pool,
        &id,
        &lookup,
        &code_hash,
        &ticket.client_id,
        &ticket.sub,
        &ticket.redirect_uri,
        &ticket.scope,
        ticket.resource.as_deref(),
        &ticket.code_challenge,
        CODE_TTL_SECS,
    )
    .await
    {
        tracing::error!("authorize_decision: insert code failed: {e}");
        return error_page("A server error occurred. Please try again.");
    }

    tracing::info!(client_id = %ticket.client_id, user_id = %ticket.sub, "Issued authorization code");
    let mut pairs: Vec<(&str, &str)> = vec![("code", &raw_code)];
    if let Some(s) = ticket.oauth_state.as_deref() {
        pairs.push(("state", s));
    }
    Redirect::to(&redirect_with(&ticket.redirect_uri, &pairs)).into_response()
}

// ── Token endpoint ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct TokenForm {
    grant_type: Option<String>,
    // authorization_code grant
    code: Option<String>,
    redirect_uri: Option<String>,
    code_verifier: Option<String>,
    client_id: Option<String>,
    // refresh_token grant
    refresh_token: Option<String>,
}

fn token_error(error: &str, description: &str) -> Response {
    // OAuth token errors are 400 with a JSON body and must not be cached.
    let mut resp = (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": error, "error_description": description })),
    )
        .into_response();
    resp.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    resp
}

/// True if the PKCE code_verifier matches the stored S256 challenge.
fn pkce_matches(verifier: &str, challenge: &str) -> bool {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest) == challenge
}

fn token_response(access_token: &str, scope: &str, refresh_token: &str) -> Response {
    // Per OAuth, the `scope` response value is space-delimited.
    let scope_spaced = scope.replace(',', " ");
    let mut resp = Json(json!({
        "access_token": access_token,
        "token_type": "Bearer",
        "expires_in": ACCESS_TTL_SECS,
        "refresh_token": refresh_token,
        "scope": scope_spaced,
    }))
    .into_response();
    resp.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    resp
}

/// POST /token — RFC 6749 token endpoint (authorization_code + refresh_token).
pub async fn token(State(state): State<Arc<AppState>>, headers: HeaderMap, Form(f): Form<TokenForm>) -> Response {
    if !state.limiters.token.allow(&crate::ratelimit::client_key(&headers)).await {
        return oauth_error(StatusCode::TOO_MANY_REQUESTS, "slow_down", "too many token requests; retry later");
    }
    match f.grant_type.as_deref() {
        Some("authorization_code") => token_authorization_code(&state, &f).await,
        Some("refresh_token") => token_refresh(&state, &f).await,
        _ => token_error("unsupported_grant_type", "grant_type must be authorization_code or refresh_token"),
    }
}

async fn token_authorization_code(state: &Arc<AppState>, f: &TokenForm) -> Response {
    let (Some(code), Some(redirect_uri), Some(verifier), Some(client_id)) = (
        f.code.as_deref(),
        f.redirect_uri.as_deref(),
        f.code_verifier.as_deref(),
        f.client_id.as_deref(),
    ) else {
        return token_error("invalid_request", "code, redirect_uri, code_verifier and client_id are required");
    };

    if code.len() < AC_PREFIX.len() + 8 {
        return token_error("invalid_grant", "invalid authorization code");
    }
    let lookup = lookup_of(AC_PREFIX, code);
    let rows = match db::oauth_find_codes(&state.pool, &lookup).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("token: code lookup failed: {e}");
            return token_error("server_error", "internal error");
        }
    };
    let Some(row) = rows.into_iter().find(|r| auth::verify_token_hash(code, &r.code_hash)) else {
        return token_error("invalid_grant", "invalid or expired authorization code");
    };

    if row.client_id != client_id || row.redirect_uri != redirect_uri {
        return token_error("invalid_grant", "client_id or redirect_uri mismatch");
    }
    if !pkce_matches(verifier, &row.code_challenge) {
        return token_error("invalid_grant", "PKCE verification failed");
    }

    // Single-use: consume atomically. A losing race (already consumed) is rejected.
    match db::oauth_consume_code(&state.pool, &row.id).await {
        Ok(true) => {}
        Ok(false) => return token_error("invalid_grant", "authorization code already used"),
        Err(e) => {
            tracing::error!("token: consume code failed: {e}");
            return token_error("server_error", "internal error");
        }
    }

    let (resp, _) = issue_tokens(state, &row.user_id, &row.client_id, &row.scope, row.resource.as_deref()).await;
    resp
}

async fn token_refresh(state: &Arc<AppState>, f: &TokenForm) -> Response {
    let Some(refresh) = f.refresh_token.as_deref() else {
        return token_error("invalid_request", "refresh_token is required");
    };
    if refresh.len() < RT_PREFIX.len() + 8 {
        return token_error("invalid_grant", "invalid refresh token");
    }
    let lookup = lookup_of(RT_PREFIX, refresh);
    let rows = match db::oauth_find_refresh(&state.pool, &lookup).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("token: refresh lookup failed: {e}");
            return token_error("server_error", "internal error");
        }
    };
    let Some(row) = rows.into_iter().find(|r| auth::verify_token_hash(refresh, &r.token_hash)) else {
        return token_error("invalid_grant", "invalid refresh token");
    };

    if let Some(cid) = f.client_id.as_deref() {
        if cid != row.client_id {
            return token_error("invalid_grant", "client_id mismatch");
        }
    }

    // Reuse detection: a revoked token being presented means it was already
    // rotated (or the grant was revoked). Kill the whole grant defensively.
    if row.revoked_at.is_some() {
        tracing::warn!(client_id = %row.client_id, user_id = %row.user_id, "Refresh token reuse — revoking grant");
        let _ = db::oauth_revoke_grant(&state.pool, &row.user_id, &row.client_id).await;
        return token_error("invalid_grant", "refresh token has been revoked");
    }
    if row.expires_at < chrono::Utc::now() {
        return token_error("invalid_grant", "refresh token expired");
    }

    // Rotate: mint a replacement, then revoke the old one pointing at it.
    let (resp, new_id) = issue_tokens(state, &row.user_id, &row.client_id, &row.scope, row.resource.as_deref()).await;
    if let Some(new_id) = new_id {
        let _ = db::oauth_rotate_refresh(&state.pool, &row.id, &new_id).await;
    }
    resp
}

/// Mint an access JWT + a fresh refresh token for (user, client, scope, resource).
/// Returns the response and, on success, the new refresh token's id (so refresh
/// rotation can record what the old token rotated into).
async fn issue_tokens(
    state: &Arc<AppState>,
    user_id: &str,
    client_id: &str,
    scope: &str,
    resource: Option<&str>,
) -> (Response, Option<String>) {
    let aud = resource_url(state);
    let access = match auth::create_access_jwt(
        &state.jwt_secret,
        &state.public_url,
        user_id,
        client_id,
        scope,
        &aud,
        ACCESS_TTL_SECS,
    ) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("token: access jwt failed: {e}");
            return (token_error("server_error", "internal error"), None);
        }
    };

    let (raw_refresh, lookup) = mint_token(RT_PREFIX);
    let Ok(refresh_hash) = auth::hash_token(&raw_refresh) else {
        return (token_error("server_error", "internal error"), None);
    };
    let refresh_id = format!("rt_{}", uuid::Uuid::new_v4());
    if let Err(e) = db::oauth_insert_refresh(
        &state.pool,
        &refresh_id,
        &lookup,
        &refresh_hash,
        user_id,
        client_id,
        scope,
        resource,
        REFRESH_TTL_SECS,
    )
    .await
    {
        tracing::error!("token: insert refresh failed: {e}");
        return (token_error("server_error", "internal error"), None);
    }

    tracing::info!(%client_id, %user_id, "Issued access + refresh tokens");
    (token_response(&access, scope, &raw_refresh), Some(refresh_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_s256_rfc7636_vector() {
        // RFC 7636 Appendix B reference verifier/challenge pair.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        assert!(pkce_matches(verifier, challenge));
        assert!(!pkce_matches("wrong-verifier", challenge));
    }

    #[test]
    fn pct_encode_escapes_reserved_only() {
        assert_eq!(pct_encode("abcXYZ09-._~"), "abcXYZ09-._~");
        assert_eq!(pct_encode("a b&c=d"), "a%20b%26c%3Dd");
    }

    #[test]
    fn redirect_with_picks_separator_and_encodes() {
        assert_eq!(
            redirect_with("https://claude.ai/cb", &[("code", "abc"), ("state", "a b")]),
            "https://claude.ai/cb?code=abc&state=a%20b"
        );
        assert_eq!(
            redirect_with("https://x.test/cb?foo=1", &[("code", "z")]),
            "https://x.test/cb?foo=1&code=z"
        );
    }

    #[test]
    fn normalize_scope_filters_and_defaults() {
        assert_eq!(normalize_scope(Some("read write")), "read,write");
        assert_eq!(normalize_scope(Some("read,bogus,sync")), "read,sync");
        assert_eq!(normalize_scope(None), "read,write,sync");
        assert_eq!(normalize_scope(Some("nonsense")), "read,write,sync");
    }

    #[test]
    fn lookup_is_eight_chars_after_prefix() {
        let (raw, lookup) = mint_token(AC_PREFIX);
        assert!(raw.starts_with(AC_PREFIX));
        assert_eq!(lookup.len(), 8);
        assert_eq!(lookup, lookup_of(AC_PREFIX, &raw));
    }
}
