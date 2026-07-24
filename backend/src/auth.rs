use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{header, request::Parts, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{error::AppError, AppState};

const ISSUER: &str = "wealthagent";
const AUDIENCE: &str = "wealthagent-api";

const SESSION_WINDOW_SECS: i64 = 86_400;
const SESSION_MAX_LIFETIME_SECS: i64 = 2_592_000;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,
    pub iss: String,
    pub aud: String,
    pub exp: usize,
    pub login_at: usize,
}

pub fn create_jwt(user_id: &str, secret: &str) -> Result<String, AppError> {
    let now = chrono::Utc::now().timestamp();
    let exp = now + SESSION_WINDOW_SECS;
    let claims = Claims {
        sub: user_id.to_string(),
        iss: ISSUER.to_string(),
        aud: AUDIENCE.to_string(),
        exp: exp as usize,
        login_at: now as usize,
    };
    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes()))
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

pub fn renew_jwt(claims: &Claims, secret: &str) -> Result<String, AppError> {
    let now = chrono::Utc::now().timestamp();
    let deadline = claims.login_at as i64 + SESSION_MAX_LIFETIME_SECS;
    if now >= deadline {
        return Err(AppError::Unauthorized);
    }
    let exp = (now + SESSION_WINDOW_SECS).min(deadline);
    let renewed = Claims {
        sub: claims.sub.clone(),
        iss: ISSUER.to_string(),
        aud: AUDIENCE.to_string(),
        exp: exp as usize,
        login_at: claims.login_at,
    };
    encode(&Header::default(), &renewed, &EncodingKey::from_secret(secret.as_bytes()))
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

pub fn verify_jwt(token: &str, secret: &str) -> Result<Claims, AppError> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&[ISSUER]);
    validation.set_audience(&[AUDIENCE]);
    decode::<Claims>(token, &DecodingKey::from_secret(secret.as_bytes()), &validation)
        .map(|d| d.claims)
        .map_err(|_| AppError::Unauthorized)
}

// ── OAuth access tokens (stateless JWTs, distinct from the session JWT) ────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AccessClaims {
    pub sub: String,        // user id
    pub iss: String,        // deployment public_url
    pub aud: String,        // resource (e.g. https://app.texasnetworth.com/mcp)
    pub scope: String,      // comma-delimited (internal form)
    pub client_id: String,
    pub exp: usize,
    pub token_type: String, // always "oauth_access" — guards against session-JWT confusion
}

pub fn create_access_jwt(
    secret: &str,
    issuer: &str,
    sub: &str,
    client_id: &str,
    scope: &str,
    aud: &str,
    ttl_secs: i64,
) -> Result<String, AppError> {
    let exp = (chrono::Utc::now().timestamp() + ttl_secs) as usize;
    let claims = AccessClaims {
        sub: sub.to_string(),
        iss: issuer.to_string(),
        aud: aud.to_string(),
        scope: scope.to_string(),
        client_id: client_id.to_string(),
        exp,
        token_type: "oauth_access".to_string(),
    };
    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes()))
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// Verify an OAuth access token: signature, expiry, audience binding, and the
/// `oauth_access` type guard (so a session JWT can never be used as one).
pub fn verify_access_jwt(token: &str, secret: &str, expected_aud: &str) -> Result<AccessClaims, AppError> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&[expected_aud]);
    let claims = decode::<AccessClaims>(token, &DecodingKey::from_secret(secret.as_bytes()), &validation)
        .map(|d| d.claims)
        .map_err(|_| AppError::Unauthorized)?;
    if claims.token_type != "oauth_access" {
        return Err(AppError::Unauthorized);
    }
    Ok(claims)
}

pub fn hash_token(raw: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(raw.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

pub fn verify_token_hash(raw: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|h| Argon2::default().verify_password(raw.as_bytes(), &h).is_ok())
        .unwrap_or(false)
}

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String,
    pub claims: Option<Claims>,   // Some for cookie auth, None for PAT auth
    pub token_id: Option<String>, // Some for PAT auth, None for cookie
    pub scopes: String,           // "read,write,sync" for cookie; from token for PAT
    /// Privacy private key unwrapped from the PAT row using the presented raw
    /// token (see `privacy` module). None for cookie/OAuth auth — those paths
    /// resolve the key from the unlock cache instead.
    pub privacy_secret: Option<crate::privacy::PrivacySecret>,
}

impl AuthUser {
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.split(',').map(str::trim).any(|s| s == scope)
    }

    /// Gate an endpoint on a token scope. Cookie sessions always carry the full
    /// scope set, so this only ever rejects PAT / OAuth callers.
    pub fn require_scope(&self, scope: &str) -> Result<(), AppError> {
        if self.has_scope(scope) {
            Ok(())
        } else {
            Err(AppError::Forbidden(format!("This token does not have the '{scope}' scope.")))
        }
    }

    /// Gate management-plane endpoints (token/grant lifecycle, destructive ops)
    /// on a browser session — an API token must never mint or revoke credentials.
    pub fn require_web_session(&self) -> Result<(), AppError> {
        if self.claims.is_some() {
            Ok(())
        } else {
            Err(AppError::Forbidden("This action requires a web session, not an API token.".to_string()))
        }
    }
}

#[derive(sqlx::FromRow)]
struct PatRow {
    id: String,
    user_id: String,
    token_hash: String,
    scopes: String,
    wrapped_private_key: Option<Vec<u8>>,
}

#[async_trait]
impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &Arc<AppState>) -> Result<Self, Self::Rejection> {
        let user = resolve_user(parts, state).await?;
        // Paywall chokepoint (no-op unless BILLING=on): every auth flavor —
        // cookie, PAT, OAuth/MCP — passes through here, so gating once covers
        // all protected routes. 402 for unpaid users outside the billing/auth surface.
        crate::billing::enforce_entitlement(state, &user.user_id, parts.uri.path()).await?;
        Ok(user)
    }
}

async fn resolve_user(parts: &Parts, state: &Arc<AppState>) -> Result<AuthUser, AppError> {
    // 1. Authorization: Bearer …
    if let Some(bearer) = extract_bearer(parts) {
        // 1a. Personal access token (wa_pat_…)
        if bearer.starts_with("wa_pat_") {
            return authenticate_pat(&bearer, state).await;
        }
        // 1b. OAuth access token (JWT, audience-bound to the MCP resource)
        let aud = format!("{}/mcp", state.public_url);
        if let Ok(claims) = verify_access_jwt(&bearer, &state.jwt_secret, &aud) {
            // Record grant activity (best-effort, off the request path).
            let pool = state.pool.clone();
            let (uid, cid) = (claims.sub.clone(), claims.client_id.clone());
            tokio::spawn(async move {
                let _ = crate::db::oauth_touch_grant(&pool, &uid, &cid).await;
            });
            return Ok(AuthUser {
                user_id: claims.sub,
                claims: None,
                token_id: None,
                scopes: claims.scope, // already comma-delimited internal form
                privacy_secret: None,
            });
        }
        // Unrecognised bearer — fall through to cookie auth below.
    }
    // 2. Fall back to cookie
    authenticate_cookie(parts, state)
}

fn extract_bearer(parts: &Parts) -> Option<String> {
    parts
        .headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.trim().to_string())
}

async fn authenticate_pat(raw_token: &str, state: &Arc<AppState>) -> Result<AuthUser, AppError> {
    if raw_token.len() < 15 {
        return Err(AppError::Unauthorized);
    }
    // The raw token is wa_pat_<base64url>. prefix = first 8 chars after "wa_pat_"
    let prefix = &raw_token[7..15];

    let rows = sqlx::query_as::<_, PatRow>(
        "SELECT id, user_id, token_hash, scopes, wrapped_private_key
         FROM personal_api_tokens
         WHERE token_prefix = $1 AND revoked_at IS NULL"
    )
    .bind(prefix)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| AppError::Unauthorized)?;

    for row in rows {
        if verify_token_hash(raw_token, &row.token_hash) {
            let token_id = row.id.clone();
            let pool = state.pool.clone();
            tokio::spawn(async move {
                let _ = sqlx::query(
                    "UPDATE personal_api_tokens SET last_used_at = NOW() WHERE id = $1"
                )
                .bind(&token_id)
                .execute(&pool)
                .await;
            });
            // Tokens created while the user's privacy key was unlocked carry a
            // wrapped copy of it, openable only with the raw token we just saw.
            let privacy_secret = row.wrapped_private_key.as_deref().and_then(|blob| {
                let kek = crate::privacy::derive_token_kek(raw_token);
                crate::privacy::unwrap_secret(&kek, blob).ok().map(crate::privacy::PrivacySecret)
            });
            return Ok(AuthUser {
                user_id: row.user_id,
                claims: None,
                token_id: Some(row.id),
                scopes: row.scopes,
                privacy_secret,
            });
        }
    }
    Err(AppError::Unauthorized)
}

/// MCP-flavoured auth: same credential resolution as [`AuthUser`], but a failed
/// extraction returns 401 with a `WWW-Authenticate: Bearer resource_metadata="…"`
/// header so MCP clients (Claude Desktop, ChatGPT) kick off the OAuth discovery flow.
pub struct McpAuthUser(pub AuthUser);

#[async_trait]
impl FromRequestParts<Arc<AppState>> for McpAuthUser {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &Arc<AppState>) -> Result<Self, Self::Rejection> {
        match AuthUser::from_request_parts(parts, state).await {
            Ok(user) => Ok(McpAuthUser(user)),
            // An unpaid-but-authenticated caller must get the 402, not a 401
            // challenge — re-running the OAuth flow would never fix payment.
            Err(e @ AppError::PaymentRequired(_)) => Err(e.into_response()),
            Err(_) => {
                let metadata_url = format!("{}/.well-known/oauth-protected-resource", state.public_url);
                let challenge = format!("Bearer resource_metadata=\"{metadata_url}\"");
                let body = Json(serde_json::json!({ "status": "error", "message": "Unauthorized" }));
                let mut resp = (StatusCode::UNAUTHORIZED, body).into_response();
                if let Ok(v) = HeaderValue::from_str(&challenge) {
                    resp.headers_mut().insert(header::WWW_AUTHENTICATE, v);
                }
                Err(resp)
            }
        }
    }
}

fn authenticate_cookie(parts: &Parts, state: &Arc<AppState>) -> Result<AuthUser, AppError> {
    let token = session_cookie_value(&parts.headers).ok_or(AppError::Unauthorized)?;
    let claims = verify_jwt(&token, &state.jwt_secret)?;
    let user_id = claims.sub.clone();
    Ok(AuthUser {
        user_id,
        claims: Some(claims),
        token_id: None,
        scopes: "read,write,sync".to_string(),
        privacy_secret: None,
    })
}

/// Pull the raw `wa_session` JWT out of the Cookie header, if present.
fn session_cookie_value(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            cookies
                .split(';')
                .map(|c| c.trim())
                .find_map(|c| c.strip_prefix("wa_session="))
        })
        .map(|s| s.to_string())
}

/// Resolve the logged-in user id from the session cookie without rejecting.
/// Used by the OAuth `/authorize` flow, which redirects to login rather than 401.
pub fn session_sub(headers: &axum::http::HeaderMap, secret: &str) -> Option<String> {
    let token = session_cookie_value(headers)?;
    verify_jwt(&token, secret).ok().map(|c| c.sub)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-secret";
    const AUD: &str = "https://app.example.com/mcp";

    #[test]
    fn access_token_roundtrips_with_correct_audience() {
        let t = create_access_jwt(SECRET, "https://app.example.com", "user-1", "client-1", "read,write", AUD, 60).unwrap();
        let claims = verify_access_jwt(&t, SECRET, AUD).unwrap();
        assert_eq!(claims.sub, "user-1");
        assert_eq!(claims.scope, "read,write");
        assert_eq!(claims.token_type, "oauth_access");
    }

    #[test]
    fn access_token_rejected_for_wrong_audience() {
        let t = create_access_jwt(SECRET, "https://app.example.com", "u", "c", "read", AUD, 60).unwrap();
        assert!(verify_access_jwt(&t, SECRET, "https://evil.test/mcp").is_err());
    }

    #[test]
    fn scope_checks() {
        let user = |scopes: &str| AuthUser {
            user_id: "u".into(),
            claims: None,
            token_id: Some("pat_x".into()),
            scopes: scopes.into(),
            privacy_secret: None,
        };
        assert!(user("read,write,sync").require_scope("write").is_ok());
        assert!(user("read, write").require_scope("write").is_ok()); // tolerates spaces
        assert!(user("read").require_scope("write").is_err());
        assert!(user("read").require_scope("sync").is_err());
        assert!(user("").require_scope("read").is_err());
        // A token must never pass the web-session gate.
        assert!(user("read,write,sync").require_web_session().is_err());
    }

    #[test]
    fn session_and_access_tokens_are_not_interchangeable() {
        // A session JWT must not validate as an OAuth access token (different aud,
        // and no oauth_access type), and vice versa.
        let session = create_jwt("user-1", SECRET).unwrap();
        assert!(verify_access_jwt(&session, SECRET, AUD).is_err());

        let access = create_access_jwt(SECRET, "https://app.example.com", "user-1", "c", "read", AUD, 60).unwrap();
        assert!(verify_jwt(&access, SECRET).is_err());
    }
}
