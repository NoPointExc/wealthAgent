//! Stripe subscription billing + paywall (BILLING=on deployments only).
//!
//! One plan, two prices ($8/mo, $80/yr). Visitors check out on Stripe's hosted
//! Checkout page (Apple Pay / Google Pay come for free there), manage the
//! subscription through the Stripe Customer Portal, and subscription state is
//! mirrored onto `users` (stripe_customer_id / subscription_status /
//! current_period_end) by the signature-verified webhook.
//!
//! Enforcement happens in ONE place: `enforce_entitlement`, called from the
//! `AuthUser` extractor, so cookie sessions, personal API tokens, and MCP OAuth
//! callers are all gated identically. Unpaid users get 402 Payment Required on
//! everything except the auth/billing/config surface they need to pay.
//!
//! With `BILLING` unset (the default) this module is inert: `state.billing` is
//! `None`, no route behaves differently, and prod is byte-identical.

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::State,
    http::HeaderMap,
    Json,
};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::Sha256;

use crate::{auth::AuthUser, db, error::AppError, AppState};

const STRIPE_API_BASE: &str = "https://api.stripe.com/v1";
/// Max clock skew accepted between the webhook's signed timestamp and now.
const WEBHOOK_TOLERANCE_SECS: i64 = 300;

/// Paths an authenticated-but-unpaid user may still reach: enough to see who
/// they are, pay, manage billing, and leave. Everything else 402s.
const ENTITLEMENT_EXEMPT_PREFIXES: &[&str] = &[
    "/api/auth/",
    "/api/billing/",
    "/api/config",
    "/api/health",
    "/api/privacy/status",
];

// ── Config ──────────────────────────────────────────────────────────────────

pub struct BillingConfig {
    pub secret_key: String,
    pub webhook_secret: String,
    pub price_monthly: String,
    pub price_annual: String,
    /// Optional free trial (STRIPE_TRIAL_DAYS) applied at checkout.
    pub trial_days: Option<u32>,
    /// OWNER_EMAILS bypass the paywall — the operator never pays themselves.
    pub exempt_emails: Vec<String>,
}

impl BillingConfig {
    /// `BILLING=on|true|1` enables billing; anything else returns None and the
    /// whole feature is dormant. When on, the Stripe settings are mandatory —
    /// failing fast at boot beats a paywall that can't take payment.
    pub fn from_env() -> Option<Self> {
        let on = matches!(std::env::var("BILLING").as_deref(), Ok("on") | Ok("true") | Ok("1"));
        if !on {
            return None;
        }
        let secret_key = std::env::var("STRIPE_SECRET_KEY")
            .expect("BILLING=on requires STRIPE_SECRET_KEY (or STRIPE_SECRET_KEY_FILE)");
        let webhook_secret = std::env::var("STRIPE_WEBHOOK_SECRET")
            .expect("BILLING=on requires STRIPE_WEBHOOK_SECRET (or STRIPE_WEBHOOK_SECRET_FILE)");
        let price_monthly = std::env::var("STRIPE_PRICE_MONTHLY")
            .expect("BILLING=on requires STRIPE_PRICE_MONTHLY (a Stripe price id)");
        let price_annual = std::env::var("STRIPE_PRICE_ANNUAL")
            .expect("BILLING=on requires STRIPE_PRICE_ANNUAL (a Stripe price id)");
        let trial_days = std::env::var("STRIPE_TRIAL_DAYS")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|d| *d > 0);
        let exempt_emails = std::env::var("OWNER_EMAILS")
            .map(|s| {
                s.split(',')
                    .map(|e| e.trim().to_lowercase())
                    .filter(|e| !e.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        Some(Self { secret_key, webhook_secret, price_monthly, price_annual, trial_days, exempt_emails })
    }
}

// ── Entitlement ─────────────────────────────────────────────────────────────

/// Subscription statuses that grant access. `active` covers
/// cancel-at-period-end (Stripe keeps it active until the period ends, then
/// sends customer.subscription.deleted → 'canceled'). `past_due` keeps access
/// through the already-started period while Stripe dunning retries the card —
/// graceful failed-payment handling without an indefinite free ride.
pub fn is_entitled(status: &str, current_period_end: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    match status {
        "active" | "trialing" => true,
        "past_due" => current_period_end.map(|end| now < end).unwrap_or(false),
        _ => false,
    }
}

/// The paywall chokepoint, called from the `AuthUser` extractor for every
/// authenticated request. No-op unless billing is enabled. Demo users and
/// owner emails bypass; everyone else needs an entitled subscription.
pub async fn enforce_entitlement(
    state: &Arc<AppState>,
    user_id: &str,
    path: &str,
) -> Result<(), AppError> {
    let Some(cfg) = &state.billing else { return Ok(()) };
    if state.demo_mode {
        return Ok(()); // demo instances are free by definition
    }
    if ENTITLEMENT_EXEMPT_PREFIXES.iter().any(|p| path.starts_with(p)) {
        return Ok(());
    }
    let user = db::billing_user(&state.pool, user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    if user.google_id.starts_with("demo:") || user.google_id == db::DEMO_TEMPLATE_GOOGLE_ID {
        return Ok(());
    }
    if cfg.exempt_emails.iter().any(|e| e == &user.email) {
        return Ok(());
    }
    if is_entitled(&user.subscription_status, user.current_period_end, Utc::now()) {
        return Ok(());
    }
    Err(AppError::PaymentRequired(
        "An active subscription is required. Open the app to subscribe.".to_string(),
    ))
}

// ── Stripe REST helpers ─────────────────────────────────────────────────────

async fn stripe_post(
    client: &reqwest::Client,
    cfg: &BillingConfig,
    path: &str,
    form: &[(String, String)],
) -> Result<Value, AppError> {
    let resp = client
        .post(format!("{STRIPE_API_BASE}/{path}"))
        .bearer_auth(&cfg.secret_key)
        .form(form)
        .send()
        .await?;
    stripe_response(resp, path).await
}

async fn stripe_get(
    client: &reqwest::Client,
    cfg: &BillingConfig,
    path: &str,
) -> Result<Value, AppError> {
    let resp = client
        .get(format!("{STRIPE_API_BASE}/{path}"))
        .bearer_auth(&cfg.secret_key)
        .send()
        .await?;
    stripe_response(resp, path).await
}

async fn stripe_response(resp: reqwest::Response, path: &str) -> Result<Value, AppError> {
    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        // Log detail server-side; never forward Stripe errors to the client.
        let msg = body.pointer("/error/message").and_then(|v| v.as_str()).unwrap_or("unknown");
        tracing::error!(%status, path, error = msg, "Stripe API error");
        return Err(AppError::InternalServerError("Payment provider error".to_string()));
    }
    Ok(body)
}

// ── Handlers ────────────────────────────────────────────────────────────────

/// GET /api/billing/status — the frontend's paywall decision. Also served with
/// billing disabled (entitled: true) so one frontend build works everywhere.
pub async fn billing_status(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let Some(cfg) = &state.billing else {
        return Ok(Json(json!({
            "enabled": false, "entitled": true, "status": "none",
            "has_customer": false, "current_period_end": null,
        })));
    };
    let user = db::billing_user(&state.pool, &auth.user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    let exempt = state.demo_mode
        || user.google_id.starts_with("demo:")
        || user.google_id == db::DEMO_TEMPLATE_GOOGLE_ID
        || cfg.exempt_emails.iter().any(|e| e == &user.email);
    let entitled = exempt || is_entitled(&user.subscription_status, user.current_period_end, Utc::now());
    Ok(Json(json!({
        "enabled": true,
        "entitled": entitled,
        "status": user.subscription_status,
        "has_customer": user.stripe_customer_id.is_some(),
        "current_period_end": user.current_period_end.map(|d| d.to_rfc3339()),
    })))
}

#[derive(Deserialize)]
pub struct CheckoutRequest {
    pub plan: String, // "monthly" | "annual"
}

/// POST /api/billing/checkout — create a Stripe Checkout session and hand the
/// hosted-page URL back to the browser. Web sessions only: an API token must
/// never start a payment flow.
pub async fn billing_checkout(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<CheckoutRequest>,
) -> Result<Json<Value>, AppError> {
    let cfg = state.billing.as_ref().ok_or_else(|| AppError::NotFound("Not found".to_string()))?;
    auth.require_web_session()?;
    let user = db::billing_user(&state.pool, &auth.user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    if user.google_id.starts_with("demo:") || user.google_id == db::DEMO_TEMPLATE_GOOGLE_ID {
        return Err(AppError::Forbidden("Demo accounts can't subscribe.".to_string()));
    }
    let price = match req.plan.as_str() {
        "monthly" => &cfg.price_monthly,
        "annual" => &cfg.price_annual,
        _ => return Err(AppError::BadRequest("plan must be 'monthly' or 'annual'".to_string())),
    };

    let mut form: Vec<(String, String)> = vec![
        ("mode".into(), "subscription".into()),
        ("line_items[0][price]".into(), price.clone()),
        ("line_items[0][quantity]".into(), "1".into()),
        // {CHECKOUT_SESSION_ID} is substituted by Stripe, not by us.
        ("success_url".into(), format!("{}/?billing=success&session_id={{CHECKOUT_SESSION_ID}}", state.public_url)),
        ("cancel_url".into(), format!("{}/?billing=canceled", state.public_url)),
        ("client_reference_id".into(), auth.user_id.clone()),
        // Stripe Tax: compute and collect sales tax from the checkout address.
        ("automatic_tax[enabled]".into(), "true".into()),
        ("billing_address_collection".into(), "auto".into()),
        // Stamp the user id on the subscription itself so webhook events can
        // find the user even if they arrive before checkout.session.completed.
        ("subscription_data[metadata][user_id]".into(), auth.user_id.clone()),
    ];
    if let Some(days) = cfg.trial_days {
        form.push(("subscription_data[trial_period_days]".into(), days.to_string()));
    }
    match &user.stripe_customer_id {
        Some(cus) => {
            form.push(("customer".into(), cus.clone()));
            // Required by Stripe when automatic_tax is on with an existing customer.
            form.push(("customer_update[address]".into(), "auto".into()));
        }
        None => form.push(("customer_email".into(), user.email.clone())),
    }

    let session = stripe_post(&state.http_client, cfg, "checkout/sessions", &form).await?;
    let url = session["url"].as_str()
        .ok_or_else(|| AppError::InternalServerError("Checkout session had no URL".to_string()))?;
    tracing::info!(user_id = %auth.user_id, plan = %req.plan, "Billing: checkout session created");
    Ok(Json(json!({ "url": url })))
}

/// POST /api/billing/portal — Stripe Customer Portal (cancel / change plan /
/// update card / invoices). Web sessions only.
pub async fn billing_portal(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let cfg = state.billing.as_ref().ok_or_else(|| AppError::NotFound("Not found".to_string()))?;
    auth.require_web_session()?;
    let user = db::billing_user(&state.pool, &auth.user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    let customer = user.stripe_customer_id.ok_or_else(|| {
        AppError::BadRequest("No billing account yet — subscribe first.".to_string())
    })?;
    let form: Vec<(String, String)> = vec![
        ("customer".into(), customer),
        ("return_url".into(), format!("{}/", state.public_url)),
    ];
    let session = stripe_post(&state.http_client, cfg, "billing_portal/sessions", &form).await?;
    let url = session["url"].as_str()
        .ok_or_else(|| AppError::InternalServerError("Portal session had no URL".to_string()))?;
    Ok(Json(json!({ "url": url })))
}

/// POST /api/billing/webhook — unauthenticated, gated by the Stripe signature.
/// Mirrors subscription state onto the user row. Unknown event types are
/// acknowledged and ignored so the endpoint tolerates extra dashboard events.
pub async fn billing_webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, AppError> {
    let cfg = state.billing.as_ref().ok_or_else(|| AppError::NotFound("Not found".to_string()))?;

    let sig = headers
        .get("stripe-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::BadRequest("Missing Stripe-Signature".to_string()))?;
    if !verify_stripe_signature(&cfg.webhook_secret, &body, sig, Utc::now().timestamp(), WEBHOOK_TOLERANCE_SECS) {
        tracing::warn!("Billing webhook: bad signature");
        return Err(AppError::BadRequest("Invalid signature".to_string()));
    }

    let event: Value = serde_json::from_slice(&body)
        .map_err(|_| AppError::BadRequest("Invalid JSON".to_string()))?;
    let event_type = event["type"].as_str().unwrap_or("");
    let obj = &event["data"]["object"];

    match event_type {
        "checkout.session.completed" => {
            let user_id = obj["client_reference_id"].as_str();
            let customer = obj["customer"].as_str();
            let subscription = obj["subscription"].as_str();
            if let (Some(uid), Some(cus)) = (user_id, customer) {
                db::set_stripe_customer(&state.pool, uid, cus).await?;
            }
            // Pull authoritative status/period from Stripe rather than trusting
            // event ordering; subscription.updated events will correct any miss.
            if let Some(sub_id) = subscription {
                match stripe_get(&state.http_client, cfg, &format!("subscriptions/{sub_id}")).await {
                    Ok(sub) => apply_subscription(&state, &sub).await?,
                    Err(e) => tracing::error!("Billing: subscription fetch after checkout failed: {e}"),
                }
            }
        }
        "customer.subscription.created"
        | "customer.subscription.updated"
        | "customer.subscription.deleted" => {
            apply_subscription(&state, obj).await?;
        }
        "invoice.payment_failed" => {
            // Belt-and-braces: Stripe also sends subscription.updated with
            // past_due, but the spec wants this event handled explicitly.
            if let Some(cus) = obj["customer"].as_str() {
                if let Some(uid) = db::user_id_by_stripe_customer(&state.pool, cus).await? {
                    let period_end = db::billing_user(&state.pool, &uid)
                        .await?
                        .and_then(|u| u.current_period_end);
                    db::set_subscription_state(&state.pool, &uid, "past_due", period_end).await?;
                    tracing::warn!(user_id = %uid, "Billing: invoice payment failed — marked past_due");
                }
            }
        }
        _ => {} // acknowledged, ignored
    }

    Ok(Json(json!({ "received": true })))
}

/// Copy a Stripe subscription object's state onto the owning user. The user is
/// found by stored customer id, falling back to the user_id we stamped in the
/// subscription metadata (covers events that outrun checkout.session.completed).
async fn apply_subscription(state: &Arc<AppState>, sub: &Value) -> Result<(), AppError> {
    let Some(status) = sub["status"].as_str() else { return Ok(()) };
    let customer = sub["customer"].as_str()
        .or_else(|| sub.pointer("/customer/id").and_then(|v| v.as_str()));
    let meta_user = sub.pointer("/metadata/user_id").and_then(|v| v.as_str());
    let period_end = subscription_period_end(sub);

    let user_id = match customer {
        Some(cus) => match db::user_id_by_stripe_customer(&state.pool, cus).await? {
            Some(id) => Some(id),
            None => {
                if let Some(uid) = meta_user {
                    db::set_stripe_customer(&state.pool, uid, cus).await?;
                }
                meta_user.map(String::from)
            }
        },
        None => meta_user.map(String::from),
    };

    match user_id {
        Some(uid) => {
            db::set_subscription_state(&state.pool, &uid, status, period_end).await?;
            tracing::info!(user_id = %uid, status, "Billing: subscription state updated");
        }
        None => tracing::warn!("Billing webhook: subscription event matched no user"),
    }
    Ok(())
}

/// `current_period_end` lives at the top level on older Stripe API versions and
/// on the subscription item since 2025-03-31 — accept either shape.
fn subscription_period_end(sub: &Value) -> Option<DateTime<Utc>> {
    let secs = sub.get("current_period_end")
        .and_then(|v| v.as_i64())
        .or_else(|| sub.pointer("/items/data/0/current_period_end").and_then(|v| v.as_i64()))?;
    DateTime::from_timestamp(secs, 0)
}

// ── Webhook signature (Stripe-Signature: t=…,v1=…) ──────────────────────────

/// HMAC-SHA256 over `"{t}.{payload}"` with the endpoint secret, compared
/// (constant-time, via `Mac::verify_slice`) against every `v1` candidate.
pub fn verify_stripe_signature(
    secret: &str,
    payload: &[u8],
    sig_header: &str,
    now: i64,
    tolerance_secs: i64,
) -> bool {
    let mut timestamp: Option<i64> = None;
    let mut candidates: Vec<&str> = Vec::new();
    for part in sig_header.split(',') {
        match part.trim().split_once('=') {
            Some(("t", v)) => timestamp = v.parse().ok(),
            Some(("v1", v)) => candidates.push(v),
            _ => {}
        }
    }
    let Some(ts) = timestamp else { return false };
    if (now - ts).abs() > tolerance_secs {
        return false;
    }
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(ts.to_string().as_bytes());
    mac.update(b".");
    mac.update(payload);
    candidates.iter().any(|c| {
        hex_decode(c)
            .map(|sig| mac.clone().verify_slice(&sig).is_ok())
            .unwrap_or(false)
    })
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex_encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn sign(secret: &str, ts: i64, payload: &[u8]) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(ts.to_string().as_bytes());
        mac.update(b".");
        mac.update(payload);
        hex_encode(&mac.finalize().into_bytes())
    }

    #[test]
    fn entitlement_matrix() {
        let now = Utc::now();
        let future = Some(now + chrono::Duration::days(10));
        let past = Some(now - chrono::Duration::days(1));
        assert!(is_entitled("active", None, now));
        assert!(is_entitled("active", past, now)); // status wins; Stripe flips it on expiry
        assert!(is_entitled("trialing", None, now));
        // Dunning grace: past_due keeps access only through the current period.
        assert!(is_entitled("past_due", future, now));
        assert!(!is_entitled("past_due", past, now));
        assert!(!is_entitled("past_due", None, now));
        assert!(!is_entitled("canceled", future, now));
        assert!(!is_entitled("unpaid", future, now));
        assert!(!is_entitled("none", None, now));
        assert!(!is_entitled("incomplete", None, now));
    }

    #[test]
    fn webhook_signature_accepts_valid_and_rejects_tampered() {
        let secret = "whsec_test";
        let payload = br#"{"id":"evt_1","type":"customer.subscription.updated"}"#;
        let now = 1_750_000_000;
        let sig = sign(secret, now, payload);

        let header = format!("t={now},v1={sig}");
        assert!(verify_stripe_signature(secret, payload, &header, now, 300));
        // Extra unknown schemes are tolerated.
        let header2 = format!("t={now},v0=garbage,v1={sig}");
        assert!(verify_stripe_signature(secret, payload, &header2, now + 100, 300));

        // Tampered payload, wrong secret, stale timestamp, missing parts → reject.
        assert!(!verify_stripe_signature(secret, b"{}", &header, now, 300));
        assert!(!verify_stripe_signature("whsec_other", payload, &header, now, 300));
        assert!(!verify_stripe_signature(secret, payload, &header, now + 301, 300));
        assert!(!verify_stripe_signature(secret, payload, "v1=deadbeef", now, 300));
        assert!(!verify_stripe_signature(secret, payload, &format!("t={now}"), now, 300));
    }

    #[test]
    fn period_end_parses_top_level_and_item_level() {
        let ts = 1_750_000_000i64;
        let top = serde_json::json!({ "current_period_end": ts });
        assert_eq!(subscription_period_end(&top), DateTime::from_timestamp(ts, 0));

        // 2025-03-31+ API shape: period lives on the subscription item.
        let item = serde_json::json!({ "items": { "data": [ { "current_period_end": ts } ] } });
        assert_eq!(subscription_period_end(&item), DateTime::from_timestamp(ts, 0));

        assert_eq!(subscription_period_end(&serde_json::json!({})), None);
    }

    #[test]
    fn exempt_prefixes_cover_the_payment_surface_only() {
        let exempt = |p: &str| ENTITLEMENT_EXEMPT_PREFIXES.iter().any(|e| p.starts_with(e));
        assert!(exempt("/api/billing/checkout"));
        assert!(exempt("/api/auth/whoami"));
        assert!(exempt("/api/auth/logout"));
        assert!(exempt("/api/config"));
        assert!(exempt("/api/privacy/status"));
        assert!(!exempt("/api/accounts"));
        assert!(!exempt("/api/transactions"));
        assert!(!exempt("/api/plaid/sync"));
        assert!(!exempt("/mcp"));
    }
}
