//! HTTP server wiring: env/logging init, DB pool + migrations, router, and the
//! nightly background sync. Invoked by the thin `main.rs` binary.

use std::sync::Arc;
use std::time::Duration;

use axum::{
    http::{header, HeaderValue, Method},
    routing::{delete, get, patch, post, put},
    Router,
};
use sqlx::postgres::PgPoolOptions;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{fmt, EnvFilter};

use crate::plaid::client::PlaidClient;
use crate::{encryption, error::AppError, handlers::*, mcp, models, oauth, privacy, AppState};

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    fmt().json()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    // Seed the invite allowlist with the owner email(s) on every boot so the owner
    // can never be locked out by the invite-only gate. Idempotent.
    if let Ok(owners) = std::env::var("OWNER_EMAILS") {
        for email in owners.split(',').map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()) {
            crate::db::invite_seed_owner(&pool, &email).await?;
        }
    }

    if std::env::var("WEALTHAGENT_PURGE_MOCK").as_deref() == Ok("1") {
        crate::db::purge_mock_data(&pool).await?;
    }

    let plaid_env = std::env::var("PLAID_ENV").unwrap_or_else(|_| "sandbox".to_string());
    if plaid_env == "development" || plaid_env == "production" {
        tracing::warn!("PLAID_ENV={} routes to production.plaid.com — real bank data in use", plaid_env);
    }

    let jwt_secret = std::env::var("JWT_SECRET").expect("JWT_SECRET must be set");
    let google_client_id = std::env::var("GOOGLE_CLIENT_ID").expect("GOOGLE_CLIENT_ID must be set");
    let allowed_origin = std::env::var("ALLOWED_ORIGIN")
        .unwrap_or_else(|_| "http://localhost:5173".to_string());
    let encryption_key = encryption::load_encryption_key()
        .expect("Failed to load APP_ENCRYPTION_KEY_PATH — run: openssl rand -base64 32 > /etc/wealthagent/keyfile");

    let public_url = std::env::var("PUBLIC_URL")
        .unwrap_or_else(|_| "https://app.texasnetworth.com".to_string())
        .trim_end_matches('/')
        .to_string();

    let privacy_enabled = matches!(
        std::env::var("PRIVACY_ENCRYPTION").as_deref(),
        Ok("on") | Ok("true") | Ok("1")
    );
    if privacy_enabled {
        tracing::info!("Privacy encryption enabled — users can opt into operator-blind storage");
    }

    let state = Arc::new(AppState {
        pool,
        plaid: PlaidClient::new(),
        jwt_secret,
        google_client_id,
        http_client: reqwest::Client::new(),
        encryption_key,
        public_url,
        privacy_enabled,
        unlocked_keys: privacy::KeyCache::new(),
        active_syncs: tokio::sync::Mutex::new(std::collections::HashMap::new()),
        limiters: crate::ratelimit::Limiters::new(),
    });

    let cors = CorsLayer::new()
        .allow_origin(allowed_origin.parse::<HeaderValue>().expect("Invalid ALLOWED_ORIGIN"))
        .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE, Method::PUT])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::COOKIE])
        .allow_credentials(true)
        .max_age(Duration::from_secs(600));

    let app = Router::new()
        .route("/api/health", get(|| async { "ok" }))
        .route("/api/auth/google", post(auth_google))
        .route("/api/auth/refresh", post(refresh_session))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/whoami", get(whoami))
        .route("/api/auth/tokens", post(create_api_token).get(list_api_tokens))
        .route("/api/auth/tokens/:id", delete(revoke_api_token))
        .route("/api/auth/oauth_grants", get(list_oauth_grants))
        .route("/api/auth/oauth_grants/:client_id", delete(revoke_oauth_grant))
        .route("/api/accounts", get(get_accounts))
        .route("/api/accounts/:id", patch(update_account))
        .route("/api/portfolio/aggregate", post(aggregate_portfolio))
        .route("/api/transactions", get(get_transactions))
        .route("/api/transactions/:id", patch(update_transaction))
        .route("/api/transactions/bulk_update", post(bulk_update_transactions))
        .route("/api/plaid/create_link_token", post(create_link_token))
        .route("/api/plaid/exchange_public_token", post(exchange_public_token))
        .route("/api/plaid/sync", post(sync_plaid_data))
        .route("/api/plaid/sync_status", get(plaid_sync_status))
        .route("/api/system/reset", post(reset_database))
        .route("/api/investment_transactions", get(get_investment_transactions))
        .route("/api/capital_gains", get(get_capital_gains))
        .route("/api/capital_gains/cost_basis/:txn_id", put(set_cost_basis).delete(delete_cost_basis))
        .route("/api/saved_searches", get(get_saved_searches).post(create_saved_search))
        .route("/api/saved_searches/:id", delete(delete_saved_search))
        .route("/api/privacy/status", get(privacy::privacy_status))
        .route("/api/privacy/setup", post(privacy::privacy_setup))
        .route("/api/privacy/unlock", post(privacy::privacy_unlock))
        .route("/api/privacy/lock", post(privacy::privacy_lock))
        .route("/mcp", post(mcp::mcp_handler))
        // OAuth 2.1 discovery metadata (RFC 9728 / RFC 8414)
        .route("/.well-known/oauth-protected-resource", get(oauth::protected_resource_metadata))
        .route("/.well-known/oauth-authorization-server", get(oauth::authorization_server_metadata))
        .route("/register", post(oauth::register))
        .route("/authorize", get(oauth::authorize).post(oauth::authorize_decision))
        .route("/token", post(oauth::token))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .layer(TimeoutLayer::new(Duration::from_secs(30)))
        .layer(RequestBodyLimitLayer::new(1 * 1024 * 1024))
        .with_state(Arc::clone(&state));

    tokio::spawn(daily_sync_task(state));

    let bind = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!("Backend listening on {}", bind);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn daily_sync_task(state: Arc<AppState>) {
    loop {
        // Sleep until next 2 AM UTC
        let now = chrono::Utc::now();
        let tomorrow = now.date_naive() + chrono::Duration::days(1);
        let next_run = tomorrow.and_hms_opt(2, 0, 0).unwrap().and_utc();
        let secs = (next_run - now).num_seconds().max(0) as u64;
        tracing::info!(next_run = %next_run, "Nightly sync scheduled in {}h {}m", secs / 3600, (secs % 3600) / 60);
        tokio::time::sleep(tokio::time::Duration::from_secs(secs)).await;

        tracing::info!("Nightly sync starting");
        match sync_all_users(&state).await {
            Ok(n) => tracing::info!(synced_items = n, "Nightly sync complete"),
            Err(e) => tracing::error!("Nightly sync failed: {}", e),
        }
    }
}

async fn sync_all_users(state: &Arc<AppState>) -> Result<usize, AppError> {
    let items = sqlx::query_as::<_, models::PlaidItem>(
        "SELECT id, access_token_enc FROM plaid_items WHERE revoked_at IS NULL"
    )
    .fetch_all(&state.pool).await?;

    let count = items.len();
    for item in &items {
        let blob = match &item.access_token_enc {
            Some(b) => b,
            None => { tracing::warn!(item_id = %item.id, "Skipping item with no encrypted token"); continue; }
        };
        match encryption::decrypt_token(&state.encryption_key, blob) {
            Ok(token) => {
                if let Err(e) = crate::plaid::sync::sync_item(state, &token, &item.id).await {
                    // Log and continue — one bad item shouldn't abort all others
                    tracing::error!(item_id = %item.id, "sync_item failed: {}", e);
                }
            }
            Err(e) => tracing::error!(item_id = %item.id, "Decrypt failed: {}", e),
        }
    }
    Ok(count)
}
