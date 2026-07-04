//! Data-access layer. Every SQL statement lives here as a typed wrapper so the
//! handlers, sync, and CLI stay free of inline SQL. SQL strings are kept verbatim
//! from the previous inline versions to preserve behaviour.

use sqlx::PgPool;

use crate::error::AppError;
use crate::models::{
    Account, AccountSnapshot, AccountType, ApiTokenListItem, CreateSavedSearchRequest,
    EnrichedTransaction, Holding, InvestmentTransaction, OAuthClient, OAuthCodeRow,
    OAuthGrantListItem, OAuthRefreshRow, PlaidItem, SavedSearch, TransactionPage, TransactionQuery,
    User,
};

// ── Invite allowlist ────────────────────────────────────────────────────────

pub async fn invite_seed_owner(pool: &PgPool, email: &str) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO invited_emails (email, note) VALUES ($1, 'owner')
         ON CONFLICT (email) DO NOTHING"
    )
    .bind(email)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn is_invited(pool: &PgPool, email: &str) -> Result<bool, AppError> {
    let invited: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM invited_emails WHERE email = $1)"
    )
    .bind(email)
    .fetch_one(pool)
    .await?;
    Ok(invited)
}

pub async fn invite_add(pool: &PgPool, email: &str, note: Option<&str>) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO invited_emails (email, note) VALUES ($1, $2)
         ON CONFLICT (email) DO UPDATE SET note = COALESCE(EXCLUDED.note, invited_emails.note)"
    )
    .bind(email)
    .bind(note)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn invite_remove(pool: &PgPool, email: &str) -> Result<u64, AppError> {
    let res = sqlx::query("DELETE FROM invited_emails WHERE email = $1")
        .bind(email)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

pub async fn invite_list(
    pool: &PgPool,
) -> Result<Vec<(String, Option<String>, chrono::DateTime<chrono::Utc>)>, AppError> {
    let rows = sqlx::query_as::<_, (String, Option<String>, chrono::DateTime<chrono::Utc>)>(
        "SELECT email, note, created_at FROM invited_emails ORDER BY created_at"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ── Users ───────────────────────────────────────────────────────────────────

pub async fn upsert_user(
    pool: &PgPool,
    new_id: &str,
    google_id: &str,
    email: &str,
    name: &str,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO users (id, google_id, email, name) VALUES ($1, $2, $3, $4)
         ON CONFLICT(google_id) DO UPDATE SET email = EXCLUDED.email, name = EXCLUDED.name"
    )
    .bind(new_id).bind(google_id).bind(email).bind(name)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_user_id_by_google(pool: &PgPool, google_id: &str) -> Result<String, AppError> {
    let id: String = sqlx::query_scalar("SELECT id FROM users WHERE google_id = $1")
        .bind(google_id)
        .fetch_one(pool)
        .await?;
    Ok(id)
}

pub async fn get_user(pool: &PgPool, user_id: &str) -> Result<User, AppError> {
    let user = sqlx::query_as::<_, User>(
        "SELECT id, google_id, email, name FROM users WHERE id = $1"
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(user)
}

// ── API tokens ──────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub async fn create_api_token(
    pool: &PgPool,
    id: &str,
    user_id: &str,
    name: &str,
    prefix: &str,
    token_hash: &str,
    scopes: &str,
    wrapped_private_key: Option<Vec<u8>>,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO personal_api_tokens (id, user_id, name, token_prefix, token_hash, scopes, wrapped_private_key)
         VALUES ($1, $2, $3, $4, $5, $6, $7)"
    )
    .bind(id).bind(user_id).bind(name).bind(prefix).bind(token_hash).bind(scopes).bind(wrapped_private_key)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_api_tokens(pool: &PgPool, user_id: &str) -> Result<Vec<ApiTokenListItem>, AppError> {
    let tokens = sqlx::query_as::<_, ApiTokenListItem>(
        "SELECT id, name, scopes, created_at, last_used_at
         FROM personal_api_tokens
         WHERE user_id = $1 AND revoked_at IS NULL
         ORDER BY created_at DESC"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(tokens)
}

pub async fn revoke_api_token(pool: &PgPool, id: &str, user_id: &str) -> Result<(), AppError> {
    sqlx::query(
        // Drop the wrapped privacy key too: a revoked token's raw value must
        // never be able to recover the user's private key from a DB copy.
        "UPDATE personal_api_tokens SET revoked_at = NOW(), wrapped_private_key = NULL
         WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL"
    )
    .bind(id).bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

// ── OAuth clients ─────────────────────────────────────────────────────────────

pub async fn oauth_create_client(
    pool: &PgPool,
    client_id: &str,
    client_name: &str,
    redirect_uris: &[String],
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO oauth_clients (client_id, client_name, redirect_uris)
         VALUES ($1, $2, $3)"
    )
    .bind(client_id).bind(client_name).bind(redirect_uris)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn oauth_get_client(pool: &PgPool, client_id: &str) -> Result<Option<OAuthClient>, AppError> {
    let client = sqlx::query_as::<_, OAuthClient>(
        "SELECT client_id, client_name, redirect_uris FROM oauth_clients WHERE client_id = $1"
    )
    .bind(client_id)
    .fetch_optional(pool)
    .await?;
    Ok(client)
}

// ── OAuth authorization codes ─────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub async fn oauth_insert_code(
    pool: &PgPool,
    id: &str,
    code_prefix: &str,
    code_hash: &str,
    client_id: &str,
    user_id: &str,
    redirect_uri: &str,
    scope: &str,
    resource: Option<&str>,
    code_challenge: &str,
    ttl_secs: i64,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO oauth_auth_codes
           (id, code_prefix, code_hash, client_id, user_id, redirect_uri, scope, resource, code_challenge, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW() + ($10 || ' seconds')::interval)"
    )
    .bind(id).bind(code_prefix).bind(code_hash).bind(client_id).bind(user_id)
    .bind(redirect_uri).bind(scope).bind(resource).bind(code_challenge)
    .bind(ttl_secs.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

/// Fetch all unconsumed, unexpired codes matching a prefix (hash verified by caller).
pub async fn oauth_find_codes(pool: &PgPool, code_prefix: &str) -> Result<Vec<OAuthCodeRow>, AppError> {
    let rows = sqlx::query_as::<_, OAuthCodeRow>(
        "SELECT id, code_hash, client_id, user_id, redirect_uri, scope, resource, code_challenge
         FROM oauth_auth_codes
         WHERE code_prefix = $1 AND consumed_at IS NULL AND expires_at > NOW()"
    )
    .bind(code_prefix)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Atomically mark a code consumed. Returns true only if this call did the consuming
/// (false if already consumed — guards against authorization-code replay).
pub async fn oauth_consume_code(pool: &PgPool, id: &str) -> Result<bool, AppError> {
    let res = sqlx::query(
        "UPDATE oauth_auth_codes SET consumed_at = NOW() WHERE id = $1 AND consumed_at IS NULL"
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() == 1)
}

// ── OAuth refresh tokens ──────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub async fn oauth_insert_refresh(
    pool: &PgPool,
    id: &str,
    token_prefix: &str,
    token_hash: &str,
    user_id: &str,
    client_id: &str,
    scope: &str,
    resource: Option<&str>,
    ttl_secs: i64,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO oauth_refresh_tokens
           (id, token_prefix, token_hash, user_id, client_id, scope, resource, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, NOW() + ($8 || ' seconds')::interval)"
    )
    .bind(id).bind(token_prefix).bind(token_hash).bind(user_id).bind(client_id)
    .bind(scope).bind(resource).bind(ttl_secs.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

/// Fetch all refresh tokens matching a prefix, including revoked ones (hash verified
/// by caller). Revoked matches are needed for reuse detection.
pub async fn oauth_find_refresh(pool: &PgPool, token_prefix: &str) -> Result<Vec<OAuthRefreshRow>, AppError> {
    let rows = sqlx::query_as::<_, OAuthRefreshRow>(
        "SELECT id, token_hash, user_id, client_id, scope, resource, revoked_at, expires_at
         FROM oauth_refresh_tokens
         WHERE token_prefix = $1"
    )
    .bind(token_prefix)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Mark one refresh token revoked, recording which token it rotated into.
pub async fn oauth_rotate_refresh(pool: &PgPool, id: &str, rotated_to: &str) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE oauth_refresh_tokens SET revoked_at = NOW(), rotated_to = $2
         WHERE id = $1 AND revoked_at IS NULL"
    )
    .bind(id).bind(rotated_to)
    .execute(pool)
    .await?;
    Ok(())
}

/// Revoke every refresh token for a (user, client) pair — used on reuse detection
/// and on user-initiated disconnect.
pub async fn oauth_revoke_grant(pool: &PgPool, user_id: &str, client_id: &str) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE oauth_refresh_tokens SET revoked_at = NOW()
         WHERE user_id = $1 AND client_id = $2 AND revoked_at IS NULL"
    )
    .bind(user_id).bind(client_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// List a user's active OAuth connections (one row per client), for the
/// "Connected agents" UI. `created_at` is the original authorization time;
/// `last_used_at` is the most recent MCP request on the grant.
pub async fn oauth_list_grants(pool: &PgPool, user_id: &str) -> Result<Vec<OAuthGrantListItem>, AppError> {
    let grants = sqlx::query_as::<_, OAuthGrantListItem>(
        "SELECT r.client_id,
                c.client_name,
                MAX(r.scope)        AS scopes,
                MIN(r.created_at)   AS created_at,
                MAX(r.last_used_at) AS last_used_at
         FROM oauth_refresh_tokens r
         JOIN oauth_clients c ON c.client_id = r.client_id
         WHERE r.user_id = $1
         GROUP BY r.client_id, c.client_name
         HAVING bool_or(r.revoked_at IS NULL AND r.expires_at > NOW())
         ORDER BY MIN(r.created_at) DESC"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(grants)
}

/// Mark the active token of a (user, client) grant as just used. Best-effort.
pub async fn oauth_touch_grant(pool: &PgPool, user_id: &str, client_id: &str) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE oauth_refresh_tokens SET last_used_at = NOW()
         WHERE user_id = $1 AND client_id = $2 AND revoked_at IS NULL"
    )
    .bind(user_id).bind(client_id)
    .execute(pool)
    .await?;
    Ok(())
}

// ── Plaid items ─────────────────────────────────────────────────────────────

pub async fn insert_plaid_item(
    pool: &PgPool,
    id: &str,
    access_token_enc: &[u8],
    user_id: &str,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO plaid_items (id, access_token_enc, user_id) VALUES ($1, $2, $3)")
        .bind(id).bind(access_token_enc).bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_user_plaid_items(pool: &PgPool, user_id: &str) -> Result<Vec<PlaidItem>, AppError> {
    let items = sqlx::query_as::<_, PlaidItem>(
        "SELECT id, access_token_enc FROM plaid_items WHERE user_id = $1 AND revoked_at IS NULL"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(items)
}

pub async fn get_active_plaid_items(pool: &PgPool) -> Result<Vec<PlaidItem>, AppError> {
    let items = sqlx::query_as::<_, PlaidItem>(
        "SELECT id, access_token_enc FROM plaid_items WHERE revoked_at IS NULL"
    )
    .fetch_all(pool)
    .await?;
    Ok(items)
}

pub async fn revoke_plaid_item(pool: &PgPool, id: &str) -> Result<(), AppError> {
    sqlx::query("UPDATE plaid_items SET revoked_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ── Accounts ────────────────────────────────────────────────────────────────

pub async fn get_accounts_with_trend(pool: &PgPool, user_id: &str) -> Result<Vec<Account>, AppError> {
    let accounts = sqlx::query_as::<_, Account>(
        "SELECT a.id, a.name, a.custom_name, a.name_enc, a.custom_name_enc,
            a.balance, a.account_type, a.plaid_item_id,
            (SELECT CASE
                WHEN s_first.balance_cents IS NOT NULL AND s_first.balance_cents != 0
                THEN ROUND(100.0 * (s_last.balance_cents - s_first.balance_cents)::NUMERIC / s_first.balance_cents::NUMERIC, 2)::FLOAT8
                ELSE NULL END
             FROM
                (SELECT balance_cents FROM account_snapshots WHERE account_id = a.id ORDER BY snapshot_date DESC LIMIT 1) s_last,
                (SELECT balance_cents FROM account_snapshots WHERE account_id = a.id ORDER BY snapshot_date ASC LIMIT 1) s_first
            ) AS trend_pct
         FROM accounts a
         JOIN plaid_items pi ON a.plaid_item_id = pi.id
         WHERE pi.user_id = $1"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(accounts)
}

pub async fn account_owned(pool: &PgPool, account_id: &str, user_id: &str) -> Result<bool, AppError> {
    let owned: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM accounts a JOIN plaid_items pi ON a.plaid_item_id = pi.id WHERE a.id = $1 AND pi.user_id = $2)"
    )
    .bind(account_id).bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(owned)
}

pub async fn get_account_balance_and_type(
    pool: &PgPool,
    id: &str,
) -> Result<Option<(i64, AccountType)>, AppError> {
    let row = sqlx::query_as::<_, (i64, AccountType)>(
        "SELECT balance, account_type FROM accounts WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn set_account_balance_and_type(
    pool: &PgPool,
    balance: i64,
    account_type: AccountType,
    id: &str,
) -> Result<(), AppError> {
    sqlx::query("UPDATE accounts SET balance = $1, account_type = $2 WHERE id = $3")
        .bind(balance).bind(account_type).bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn negate_account_snapshots(pool: &PgPool, account_id: &str) -> Result<(), AppError> {
    sqlx::query("UPDATE account_snapshots SET balance_cents = -balance_cents WHERE account_id = $1")
        .bind(account_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn negate_account_holdings(pool: &PgPool, account_id: &str) -> Result<(), AppError> {
    sqlx::query("UPDATE holdings SET asset_value = -asset_value WHERE account_id = $1")
        .bind(account_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_account_custom_name(
    pool: &PgPool,
    custom_name: Option<String>,
    custom_name_enc: Option<Vec<u8>>,
    id: &str,
) -> Result<(), AppError> {
    sqlx::query("UPDATE accounts SET custom_name = $1, custom_name_enc = $2 WHERE id = $3")
        .bind(custom_name).bind(custom_name_enc).bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn clear_account_custom_name(pool: &PgPool, id: &str) -> Result<(), AppError> {
    sqlx::query("UPDATE accounts SET custom_name = NULL, custom_name_enc = NULL WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_account_type(pool: &PgPool, account_id: &str) -> Result<Option<AccountType>, AppError> {
    let t: Option<AccountType> = sqlx::query_scalar("SELECT account_type FROM accounts WHERE id = $1")
        .bind(account_id)
        .fetch_optional(pool)
        .await?;
    Ok(t)
}

#[allow(clippy::too_many_arguments)]
pub async fn upsert_account(
    pool: &PgPool,
    id: &str,
    name: &str,
    name_enc: Option<Vec<u8>>,
    balance: i64,
    plaid_item_id: &str,
    account_type: AccountType,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO accounts (id, name, name_enc, balance, trend_pct, plaid_item_id, account_type)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT(id) DO UPDATE SET
            name = EXCLUDED.name, name_enc = EXCLUDED.name_enc, balance = EXCLUDED.balance,
            plaid_item_id = EXCLUDED.plaid_item_id,
            account_type = COALESCE(accounts.account_type, EXCLUDED.account_type)"
    )
    .bind(id).bind(name).bind(name_enc).bind(balance).bind(0.0_f64).bind(plaid_item_id).bind(account_type)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_account_balance(pool: &PgPool, account_id: &str) -> Result<i64, AppError> {
    let bal: i64 = sqlx::query_scalar("SELECT balance FROM accounts WHERE id = $1")
        .bind(account_id)
        .fetch_one(pool)
        .await?;
    Ok(bal)
}

// ── Snapshots ───────────────────────────────────────────────────────────────

/// Record a daily balance snapshot, dated by the **US-Eastern trading day**.
///
/// The nightly sync runs at 02:00 UTC, which is the prior evening in ET (after the
/// 16:00 ET market close), so `(now() AT TIME ZONE 'America/New_York')::date` is
/// exactly the trading day whose close we captured — i.e. "yesterday" relative to
/// UTC. This is DST-safe and does not rely on the DB session timezone.
/// NOTE: this assumes the job runs in the evening-ET window (≈02:00–04:59 UTC); if
/// the run time ever moves into ET morning, the date would shift forward by a day.
///
/// When `skip_market_holiday` is set (investment accounts), no row is written if the
/// ET date is a weekend — markets are closed, so a snapshot would just repeat the
/// prior close as a spurious point. (Calendar holidays are not yet excluded.)
pub async fn insert_account_snapshot(
    pool: &PgPool,
    account_id: &str,
    balance_cents: i64,
    skip_market_holiday: bool,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO account_snapshots (account_id, snapshot_date, balance_cents)
         SELECT $1, m.trading_date, $2
         FROM (SELECT (now() AT TIME ZONE 'America/New_York')::date AS trading_date) m
         WHERE NOT $3 OR EXTRACT(DOW FROM m.trading_date) NOT IN (0, 6)
         ON CONFLICT (account_id, snapshot_date) DO UPDATE SET balance_cents = EXCLUDED.balance_cents"
    )
    .bind(account_id).bind(balance_cents).bind(skip_market_holiday)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_account_snapshots_through(
    pool: &PgPool,
    account_id: &str,
    end_date: chrono::NaiveDate,
) -> Result<Vec<AccountSnapshot>, AppError> {
    let snapshots = sqlx::query_as::<_, AccountSnapshot>(
        "SELECT account_id, snapshot_date, balance_cents FROM account_snapshots
         WHERE account_id = $1 AND snapshot_date <= $2
         ORDER BY snapshot_date ASC"
    )
    .bind(account_id).bind(end_date)
    .fetch_all(pool)
    .await?;
    Ok(snapshots)
}

// ── Holdings ────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub async fn upsert_holding(
    pool: &PgPool,
    symbol: &str,
    account_id: &str,
    name: &str,
    asset_value: i64,
    return_str: &str,
    performance_type: &str,
    security_id: &str,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO holdings (symbol, account_id, name, asset_value, return_str, performance_type, security_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT(security_id, account_id) DO UPDATE SET asset_value = EXCLUDED.asset_value"
    )
    .bind(symbol).bind(account_id).bind(name).bind(asset_value)
    .bind(return_str).bind(performance_type).bind(security_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn upsert_holding_with_qty(
    pool: &PgPool,
    symbol: &str,
    symbol_enc: Option<Vec<u8>>,
    account_id: &str,
    name: &str,
    name_enc: Option<Vec<u8>>,
    asset_value: i64,
    return_str: &str,
    performance_type: &str,
    security_id: &str,
    quantity: f64,
) -> Result<(), AppError> {
    // The conflict branch also rewrites symbol/name (+enc): a row synced before
    // the user opted into privacy heals to sealed form on the next sync.
    sqlx::query(
        "INSERT INTO holdings (symbol, symbol_enc, account_id, name, name_enc, asset_value, return_str, performance_type, security_id, quantity)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         ON CONFLICT(security_id, account_id) DO UPDATE SET
           asset_value = EXCLUDED.asset_value, quantity = EXCLUDED.quantity,
           symbol = EXCLUDED.symbol, symbol_enc = EXCLUDED.symbol_enc,
           name = EXCLUDED.name, name_enc = EXCLUDED.name_enc"
    )
    .bind(symbol).bind(symbol_enc).bind(account_id).bind(name).bind(name_enc).bind(asset_value)
    .bind(return_str).bind(performance_type).bind(security_id).bind(quantity)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_account_holdings(pool: &PgPool, account_id: &str, user_id: &str) -> Result<Vec<Holding>, AppError> {
    let holdings = sqlx::query_as::<_, Holding>(
        "SELECT h.symbol, h.symbol_enc, h.account_id, h.name, h.name_enc, h.asset_value, h.return_str, h.performance_type
         FROM holdings h
         JOIN accounts a ON h.account_id = a.id
         JOIN plaid_items pi ON a.plaid_item_id = pi.id
         WHERE h.account_id = $1 AND pi.user_id = $2"
    )
    .bind(account_id).bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(holdings)
}

// ── Bank transactions ───────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub async fn upsert_bank_transaction(
    pool: &PgPool,
    id: &str,
    account_id: &str,
    txn_date: &str,
    raw_string: &str,
    raw_string_enc: Option<Vec<u8>>,
    merchant_name: &Option<String>,
    merchant_name_enc: Option<Vec<u8>>,
    amount: i64,
    pending: bool,
    payment_channel: &Option<String>,
    plaid_category: Option<String>,
    plaid_primary_category: Option<String>,
    plaid_detailed_category: Option<String>,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO transactions (id, account_id, txn_date, raw_string, raw_string_enc, merchant_name, merchant_name_enc, amount, pending, payment_channel, plaid_category, plaid_primary_category, plaid_detailed_category)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
         ON CONFLICT(id) DO UPDATE SET
            pending = EXCLUDED.pending, amount = EXCLUDED.amount,
            merchant_name = EXCLUDED.merchant_name, merchant_name_enc = EXCLUDED.merchant_name_enc"
    )
    .bind(id).bind(account_id).bind(txn_date)
    .bind(raw_string).bind(raw_string_enc).bind(merchant_name).bind(merchant_name_enc).bind(amount)
    .bind(pending).bind(payment_channel)
    .bind(plaid_category).bind(plaid_primary_category).bind(plaid_detailed_category)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn transaction_owned(pool: &PgPool, id: &str, user_id: &str) -> Result<bool, AppError> {
    let owned: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM transactions t JOIN accounts a ON t.account_id = a.id JOIN plaid_items pi ON a.plaid_item_id = pi.id WHERE t.id = $1 AND pi.user_id = $2)"
    )
    .bind(id).bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(owned)
}

pub async fn set_transaction_note(
    pool: &PgPool,
    note: Option<String>,
    note_enc: Option<Vec<u8>>,
    id: &str,
) -> Result<(), AppError> {
    sqlx::query("UPDATE transactions SET note = $1, note_enc = $2 WHERE id = $3")
        .bind(note).bind(note_enc).bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_transaction_tags(pool: &PgPool, tags: Option<String>, id: &str) -> Result<(), AppError> {
    sqlx::query("UPDATE transactions SET tags = $1 WHERE id = $2")
        .bind(tags).bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// Bulk-update helpers (per-row, ownership-checked)
pub async fn get_txn_tags_owned(pool: &PgPool, id: &str, user_id: &str) -> Result<Option<String>, AppError> {
    let tags: Option<String> = sqlx::query_scalar(
        "SELECT t.tags FROM transactions t JOIN accounts a ON t.account_id = a.id JOIN plaid_items pi ON a.plaid_item_id = pi.id WHERE t.id = $1 AND pi.user_id = $2"
    )
    .bind(id).bind(user_id)
    .fetch_optional(pool)
    .await?
    .flatten();
    Ok(tags)
}

pub async fn update_txn_tags_owned(pool: &PgPool, tags: Option<String>, id: &str, user_id: &str) -> Result<u64, AppError> {
    let res = sqlx::query(
        "UPDATE transactions SET tags = $1 WHERE id = $2 AND account_id IN (SELECT a.id FROM accounts a JOIN plaid_items pi ON a.plaid_item_id = pi.id WHERE pi.user_id = $3)"
    )
    .bind(tags).bind(id).bind(user_id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

pub async fn update_txn_note_owned(
    pool: &PgPool,
    note: &Option<String>,
    note_enc: Option<Vec<u8>>,
    id: &str,
    user_id: &str,
) -> Result<u64, AppError> {
    let res = sqlx::query(
        "UPDATE transactions SET note = $1, note_enc = $2 WHERE id = $3 AND account_id IN (SELECT a.id FROM accounts a JOIN plaid_items pi ON a.plaid_item_id = pi.id WHERE pi.user_id = $4)"
    )
    .bind(note).bind(note_enc).bind(id).bind(user_id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

pub async fn clear_txn_note_owned(pool: &PgPool, id: &str, user_id: &str) -> Result<u64, AppError> {
    let res = sqlx::query(
        "UPDATE transactions SET note = NULL, note_enc = NULL WHERE id = $1 AND account_id IN (SELECT a.id FROM accounts a JOIN plaid_items pi ON a.plaid_item_id = pi.id WHERE pi.user_id = $2)"
    )
    .bind(id).bind(user_id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

// Dynamic, filtered transaction listing (count + page).
macro_rules! apply_tx_filters {
    ($builder:expr, $params:expr) => {
        if let Some(since) = &$params.since {
            $builder.push(" AND t.txn_date >= ");
            $builder.push_bind(since.clone());
        }
        if let Some(until) = &$params.until {
            $builder.push(" AND t.txn_date <= ");
            $builder.push_bind(until.clone());
        }
        if let Some(account_id) = &$params.account_id {
            $builder.push(" AND t.account_id = ");
            $builder.push_bind(account_id.clone());
        }
        if let Some(category) = &$params.category {
            $builder.push(" AND t.plaid_primary_category = ");
            $builder.push_bind(category.clone());
        }
        if let Some(tag) = &$params.tag {
            $builder.push(" AND (',' || COALESCE(t.tags, '') || ',' LIKE '%,' || ");
            $builder.push_bind(tag.clone());
            $builder.push(" || ',%')");
        }
        if let Some(search) = &$params.search {
            let pat = format!("%{}%", search);
            $builder.push(" AND (t.raw_string ILIKE ");
            $builder.push_bind(pat.clone());
            $builder.push(" OR t.merchant_name ILIKE ");
            $builder.push_bind(pat.clone());
            $builder.push(" OR t.note ILIKE ");
            $builder.push_bind(pat);
            $builder.push(")");
        }
        if let Some(pending) = $params.pending {
            $builder.push(" AND t.pending = ");
            $builder.push_bind(pending);
        }
    };
}

pub async fn inner_get_transactions(
    pool: &PgPool,
    user_id: &str,
    params: &TransactionQuery,
    crypto: &crate::privacy::UserCrypto,
) -> Result<TransactionPage, AppError> {
    let limit = params.limit.unwrap_or(200).min(1000);
    let offset = params.offset.unwrap_or(0);

    // Encrypted users can't use SQL ILIKE on sealed columns — text search moves
    // app-side: fetch (capped), decrypt, filter, then paginate in Rust.
    let app_side_search = params.search.is_some() && !crypto.is_plaintext();
    if app_side_search && crypto.secret().is_none() {
        return Err(AppError::Forbidden(
            "Privacy encryption is locked — unlock to search transactions.".to_string(),
        ));
    }
    let mut sql_params = params.clone();
    if app_side_search {
        sql_params.search = None;
    }

    let mut count_q = sqlx::QueryBuilder::new(
        "SELECT COUNT(*) FROM transactions t
         JOIN accounts a ON t.account_id = a.id
         JOIN plaid_items pi ON a.plaid_item_id = pi.id
         WHERE pi.user_id = "
    );
    count_q.push_bind(user_id.to_string());
    apply_tx_filters!(count_q, &sql_params);
    let total: i64 = count_q.build_query_scalar().fetch_one(pool).await?;

    let mut data_q = sqlx::QueryBuilder::new(
        "SELECT t.id, t.account_id, COALESCE(a.custom_name, a.name) as account_name,
                t.txn_date, t.raw_string, t.merchant_name, t.amount, t.pending,
                t.payment_channel, t.plaid_category, t.plaid_primary_category,
                t.plaid_detailed_category, t.tags, t.note,
                t.raw_string_enc, t.merchant_name_enc, t.note_enc,
                a.custom_name_enc AS account_custom_name_enc, a.name_enc AS account_name_enc
         FROM transactions t
         JOIN accounts a ON t.account_id = a.id
         JOIN plaid_items pi ON a.plaid_item_id = pi.id
         WHERE pi.user_id = "
    );
    data_q.push_bind(user_id.to_string());
    apply_tx_filters!(data_q, &sql_params);
    data_q.push(" ORDER BY t.txn_date DESC LIMIT ");
    if app_side_search {
        data_q.push_bind(crate::privacy::app_search_max_rows());
    } else {
        data_q.push_bind(limit);
        data_q.push(" OFFSET ");
        data_q.push_bind(offset);
    }

    let mut items = data_q
        .build_query_as::<EnrichedTransaction>()
        .fetch_all(pool)
        .await?;

    crate::privacy::reveal_all(&mut items, crypto);

    if app_side_search {
        let needle = params.search.as_deref().unwrap_or("").to_lowercase();
        items.retain(|t| {
            t.raw_string.to_lowercase().contains(&needle)
                || t.merchant_name.as_deref().is_some_and(|m| m.to_lowercase().contains(&needle))
                || t.note.as_deref().is_some_and(|n| n.to_lowercase().contains(&needle))
        });
        let total = items.len() as i64;
        let items: Vec<EnrichedTransaction> =
            items.into_iter().skip(offset.max(0) as usize).take(limit.max(0) as usize).collect();
        return Ok(TransactionPage { items, total, offset });
    }

    Ok(TransactionPage { items, total, offset })
}

// ── Investment transactions ─────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub async fn upsert_investment_transaction(
    pool: &PgPool,
    id: &str,
    account_id: &str,
    date: &str,
    name: &str,
    name_enc: Option<Vec<u8>>,
    amount: i64,
    quantity: Option<f64>,
    price: Option<i64>,
    fees: Option<i64>,
    txn_type: &str,
    subtype: &Option<String>,
    symbol: Option<String>,
    symbol_enc: Option<Vec<u8>>,
    security_name: Option<String>,
    security_name_enc: Option<Vec<u8>>,
) -> Result<(), AppError> {
    // The conflict branch rewrites the identity fields (+enc) too: a row synced
    // before the user opted into privacy heals to sealed form on the next sync.
    sqlx::query(
        "INSERT INTO investment_transactions
           (id, account_id, date, name, name_enc, amount, quantity, price, fees, txn_type, subtype,
            symbol, symbol_enc, security_name, security_name_enc)
         VALUES ($1, $2, $3::date, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
         ON CONFLICT(id) DO UPDATE SET
           amount = EXCLUDED.amount, price = EXCLUDED.price, fees = EXCLUDED.fees,
           name = EXCLUDED.name, name_enc = EXCLUDED.name_enc,
           symbol = EXCLUDED.symbol, symbol_enc = EXCLUDED.symbol_enc,
           security_name = EXCLUDED.security_name, security_name_enc = EXCLUDED.security_name_enc"
    )
    .bind(id).bind(account_id).bind(date).bind(name).bind(name_enc).bind(amount)
    .bind(quantity).bind(price).bind(fees).bind(txn_type).bind(subtype)
    .bind(symbol).bind(symbol_enc).bind(security_name).bind(security_name_enc)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn count_investment_transactions(pool: &PgPool, user_id: &str) -> Result<i64, AppError> {
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM investment_transactions it
         JOIN accounts a ON it.account_id = a.id
         JOIN plaid_items pi ON a.plaid_item_id = pi.id
         WHERE pi.user_id = $1"
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(total)
}

pub async fn list_investment_transactions(
    pool: &PgPool,
    user_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<InvestmentTransaction>, AppError> {
    let items = sqlx::query_as::<_, InvestmentTransaction>(
        "SELECT it.id, it.account_id, COALESCE(a.custom_name, a.name) as account_name,
                it.date::TEXT, it.name, it.name_enc, it.amount, it.quantity, it.price, it.fees,
                it.txn_type, it.subtype, it.symbol, it.symbol_enc, it.security_name, it.security_name_enc,
                a.custom_name_enc AS account_custom_name_enc, a.name_enc AS account_name_enc
         FROM investment_transactions it
         JOIN accounts a ON it.account_id = a.id
         JOIN plaid_items pi ON a.plaid_item_id = pi.id
         WHERE pi.user_id = $1
         ORDER BY it.date DESC
         LIMIT $2 OFFSET $3"
    )
    .bind(user_id).bind(limit).bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(items)
}

// ── Capital gains (queries only; FIFO logic lives in handlers) ───────────────

#[derive(sqlx::FromRow)]
pub struct CapGainsTxnRow {
    pub id: String,
    pub date: String,
    pub symbol: Option<String>,
    pub security_name: Option<String>,
    pub txn_type: String,
    pub subtype: Option<String>,
    pub quantity: Option<f64>,
    pub fees: Option<i64>,
    pub amount: i64,
    pub symbol_enc: Option<Vec<u8>>,
    pub security_name_enc: Option<Vec<u8>>,
}

#[derive(sqlx::FromRow)]
pub struct HoldingForCG {
    pub symbol: String,
    pub asset_value: i64,
    pub quantity: Option<f64>,
    pub symbol_enc: Option<Vec<u8>>,
}

#[derive(sqlx::FromRow)]
pub struct OverrideRow {
    pub investment_transaction_id: String,
    pub cost_basis_cents: i64,
    pub is_long_term: bool,
}

pub async fn list_capgains_txns(pool: &PgPool, user_id: &str) -> Result<Vec<CapGainsTxnRow>, AppError> {
    let txns = sqlx::query_as::<_, CapGainsTxnRow>(
        "SELECT it.id, it.date::TEXT as date, it.symbol, it.symbol_enc, it.security_name, it.security_name_enc,
                it.txn_type, it.subtype, it.quantity, it.fees, it.amount
         FROM investment_transactions it
         JOIN accounts a ON it.account_id = a.id
         JOIN plaid_items pi ON a.plaid_item_id = pi.id
         WHERE pi.user_id = $1
         ORDER BY it.date ASC, it.amount ASC"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(txns)
}

/// Raw per-row holdings for the capital-gains engine. Sentinel filtering and
/// the per-symbol aggregation happen in Rust after decryption (symbols may be
/// sealed) — see capital_gains::build_holdings_map.
pub async fn list_holdings_for_capgains(pool: &PgPool, user_id: &str) -> Result<Vec<HoldingForCG>, AppError> {
    let rows = sqlx::query_as::<_, HoldingForCG>(
        "SELECT h.symbol, h.symbol_enc, h.asset_value, h.quantity
         FROM holdings h
         JOIN accounts a ON h.account_id = a.id
         JOIN plaid_items pi ON a.plaid_item_id = pi.id
         WHERE pi.user_id = $1"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn list_cost_basis_overrides(pool: &PgPool, user_id: &str) -> Result<Vec<OverrideRow>, AppError> {
    let overrides = sqlx::query_as::<_, OverrideRow>(
        "SELECT investment_transaction_id, cost_basis_cents, is_long_term
         FROM cost_basis_overrides WHERE user_id = $1"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(overrides)
}

/// Whether an investment transaction belongs to this user. Guards cost-basis
/// writes: without it a caller could key an override to another user's txn id.
pub async fn investment_txn_owned(pool: &PgPool, txn_id: &str, user_id: &str) -> Result<bool, AppError> {
    let owned: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1
             FROM investment_transactions it
             JOIN accounts a     ON it.account_id    = a.id
             JOIN plaid_items pi ON a.plaid_item_id = pi.id
             WHERE it.id = $1 AND pi.user_id = $2)"
    )
    .bind(txn_id).bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(owned)
}

pub async fn set_cost_basis(
    pool: &PgPool,
    user_id: &str,
    txn_id: &str,
    cost_basis_cents: i64,
    is_long_term: bool,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO cost_basis_overrides
             (user_id, investment_transaction_id, cost_basis_cents, is_long_term)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (user_id, investment_transaction_id) DO UPDATE
             SET cost_basis_cents = EXCLUDED.cost_basis_cents,
                 is_long_term     = EXCLUDED.is_long_term,
                 updated_at       = NOW()"
    )
    .bind(user_id).bind(txn_id).bind(cost_basis_cents).bind(is_long_term)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_cost_basis(pool: &PgPool, user_id: &str, txn_id: &str) -> Result<(), AppError> {
    sqlx::query(
        "DELETE FROM cost_basis_overrides
         WHERE user_id = $1 AND investment_transaction_id = $2"
    )
    .bind(user_id).bind(txn_id)
    .execute(pool)
    .await?;
    Ok(())
}

// ── Saved searches ──────────────────────────────────────────────────────────

pub async fn list_saved_searches(pool: &PgPool, user_id: &str) -> Result<Vec<SavedSearch>, AppError> {
    let searches = sqlx::query_as::<_, SavedSearch>(
        "SELECT * FROM saved_searches WHERE user_id = $1 ORDER BY name ASC"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(searches)
}

pub async fn create_saved_search(
    pool: &PgPool,
    req: &CreateSavedSearchRequest,
    user_id: &str,
) -> Result<SavedSearch, AppError> {
    let id: i32 = sqlx::query_scalar(
        "INSERT INTO saved_searches (name, search_query, selected_account, selected_categories, selected_tags, start_date, end_date, amount_mode, amount_val, amount_min, amount_max, show_pending_only, user_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) RETURNING id"
    )
    .bind(&req.name).bind(&req.search_query).bind(&req.selected_account)
    .bind(&req.selected_categories).bind(&req.selected_tags)
    .bind(&req.start_date).bind(&req.end_date)
    .bind(&req.amount_mode).bind(&req.amount_val).bind(&req.amount_min)
    .bind(&req.amount_max).bind(req.show_pending_only).bind(user_id)
    .fetch_one(pool)
    .await?;

    let saved = sqlx::query_as::<_, SavedSearch>("SELECT * FROM saved_searches WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await?;
    Ok(saved)
}

pub async fn delete_saved_search(pool: &PgPool, id: i32, user_id: &str) -> Result<(), AppError> {
    sqlx::query("DELETE FROM saved_searches WHERE id = $1 AND user_id = $2")
        .bind(id).bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

// ── Reset / purge ───────────────────────────────────────────────────────────

pub async fn delete_user_snapshots(pool: &PgPool, user_id: &str) -> Result<(), AppError> {
    sqlx::query(
        "DELETE FROM account_snapshots WHERE account_id IN (
            SELECT a.id FROM accounts a JOIN plaid_items pi ON a.plaid_item_id = pi.id WHERE pi.user_id = $1
        )"
    ).bind(user_id).execute(pool).await?;
    Ok(())
}

pub async fn delete_user_holdings(pool: &PgPool, user_id: &str) -> Result<(), AppError> {
    sqlx::query(
        "DELETE FROM holdings WHERE account_id IN (
            SELECT a.id FROM accounts a JOIN plaid_items pi ON a.plaid_item_id = pi.id WHERE pi.user_id = $1
        )"
    ).bind(user_id).execute(pool).await?;
    Ok(())
}

pub async fn delete_user_transactions(pool: &PgPool, user_id: &str) -> Result<(), AppError> {
    sqlx::query(
        "DELETE FROM transactions WHERE account_id IN (
            SELECT a.id FROM accounts a JOIN plaid_items pi ON a.plaid_item_id = pi.id WHERE pi.user_id = $1
        )"
    ).bind(user_id).execute(pool).await?;
    Ok(())
}

pub async fn delete_user_accounts(pool: &PgPool, user_id: &str) -> Result<(), AppError> {
    sqlx::query(
        "DELETE FROM accounts WHERE plaid_item_id IN (SELECT id FROM plaid_items WHERE user_id = $1)"
    ).bind(user_id).execute(pool).await?;
    Ok(())
}

pub async fn delete_user_plaid_items(pool: &PgPool, user_id: &str) -> Result<(), AppError> {
    sqlx::query("DELETE FROM plaid_items WHERE user_id = $1").bind(user_id).execute(pool).await?;
    Ok(())
}

pub async fn delete_user_saved_searches(pool: &PgPool, user_id: &str) -> Result<(), AppError> {
    sqlx::query("DELETE FROM saved_searches WHERE user_id = $1").bind(user_id).execute(pool).await?;
    Ok(())
}

/// Drop the privacy keypair (and any token-wrapped copies) so a user who lost
/// their passphrase can reset, re-setup with a new passphrase, and re-sync.
pub async fn delete_user_privacy_keys(pool: &PgPool, user_id: &str) -> Result<(), AppError> {
    sqlx::query("DELETE FROM user_privacy_keys WHERE user_id = $1").bind(user_id).execute(pool).await?;
    sqlx::query("UPDATE personal_api_tokens SET wrapped_private_key = NULL WHERE user_id = $1")
        .bind(user_id).execute(pool).await?;
    Ok(())
}

pub async fn purge_mock_data(pool: &PgPool) -> Result<(), AppError> {
    tracing::info!("Purging non-Plaid mock data");
    sqlx::query("DELETE FROM account_snapshots WHERE account_id NOT IN (SELECT id FROM accounts WHERE plaid_item_id IS NOT NULL)").execute(pool).await?;
    sqlx::query("DELETE FROM holdings WHERE account_id NOT IN (SELECT id FROM accounts WHERE plaid_item_id IS NOT NULL)").execute(pool).await?;
    sqlx::query("DELETE FROM accounts WHERE plaid_item_id IS NULL").execute(pool).await?;
    Ok(())
}

// ── Privacy encryption (see privacy module) ─────────────────────────────────

#[derive(sqlx::FromRow)]
pub struct PrivacyKeyRow {
    pub public_key: Vec<u8>,
    pub wrapped_private_key: Vec<u8>,
    pub kdf_salt: Vec<u8>,
}

pub async fn get_privacy_keys(pool: &PgPool, user_id: &str) -> Result<Option<PrivacyKeyRow>, AppError> {
    let row = sqlx::query_as::<_, PrivacyKeyRow>(
        "SELECT public_key, wrapped_private_key, kdf_salt FROM user_privacy_keys WHERE user_id = $1"
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn insert_privacy_keys(
    pool: &PgPool,
    user_id: &str,
    public_key: &[u8],
    wrapped_private_key: &[u8],
    kdf_salt: &[u8],
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO user_privacy_keys (user_id, public_key, wrapped_private_key, kdf_salt)
         VALUES ($1, $2, $3, $4)"
    )
    .bind(user_id).bind(public_key).bind(wrapped_private_key).bind(kdf_salt)
    .execute(pool)
    .await?;
    Ok(())
}

/// Public key of the item's owner, if that user opted into privacy encryption.
/// The sync path uses this to seal incoming Plaid data — no private key needed.
pub async fn get_privacy_pubkey_for_item(pool: &PgPool, item_id: &str) -> Result<Option<Vec<u8>>, AppError> {
    let key: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT k.public_key FROM user_privacy_keys k
         JOIN plaid_items pi ON pi.user_id = k.user_id
         WHERE pi.id = $1"
    )
    .bind(item_id)
    .fetch_optional(pool)
    .await?;
    Ok(key)
}

// One-time sealing of pre-existing rows when a user opts in (privacy_setup).

pub async fn list_account_rows_for_privacy(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<(String, String, Option<String>)>, AppError> {
    let rows = sqlx::query_as::<_, (String, String, Option<String>)>(
        "SELECT a.id, a.name, a.custom_name FROM accounts a
         JOIN plaid_items pi ON a.plaid_item_id = pi.id
         WHERE pi.user_id = $1 AND a.name_enc IS NULL"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn seal_account_row(
    pool: &PgPool,
    id: &str,
    name_enc: Vec<u8>,
    custom_name_enc: Option<Vec<u8>>,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE accounts SET name = '', name_enc = $1, custom_name = NULL, custom_name_enc = $2 WHERE id = $3"
    )
    .bind(name_enc).bind(custom_name_enc).bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_txn_rows_for_privacy(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<(String, String, Option<String>, Option<String>)>, AppError> {
    let rows = sqlx::query_as::<_, (String, String, Option<String>, Option<String>)>(
        "SELECT t.id, t.raw_string, t.merchant_name, t.note FROM transactions t
         JOIN accounts a ON t.account_id = a.id
         JOIN plaid_items pi ON a.plaid_item_id = pi.id
         WHERE pi.user_id = $1 AND t.raw_string_enc IS NULL"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Investment-txn rows with at least one field still in plaintext. Each field
/// is projected only when it still needs sealing (its _enc is NULL), so a row
/// sealed by an earlier version (name done, symbol not) gets exactly the
/// missing fields sealed. Sentinel symbols (CASH/DEBT/N/A) stay plaintext.
pub async fn list_inv_txn_rows_for_privacy(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<(String, Option<String>, Option<String>, Option<String>)>, AppError> {
    let rows = sqlx::query_as::<_, (String, Option<String>, Option<String>, Option<String>)>(
        "SELECT it.id,
                CASE WHEN it.name_enc IS NULL THEN it.name END,
                CASE WHEN it.symbol_enc IS NULL AND it.symbol NOT IN ('CASH','DEBT','N/A') AND it.symbol <> '' THEN it.symbol END,
                CASE WHEN it.security_name_enc IS NULL THEN it.security_name END
         FROM investment_transactions it
         JOIN accounts a ON it.account_id = a.id
         JOIN plaid_items pi ON a.plaid_item_id = pi.id
         WHERE pi.user_id = $1
           AND (it.name_enc IS NULL
             OR (it.symbol_enc IS NULL AND it.symbol IS NOT NULL AND it.symbol NOT IN ('CASH','DEBT','N/A') AND it.symbol <> '')
             OR (it.security_name_enc IS NULL AND it.security_name IS NOT NULL))"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Seal only the fields passed as Some — the others keep their current state.
pub async fn seal_inv_txn_row(
    pool: &PgPool,
    id: &str,
    name_enc: Option<Vec<u8>>,
    symbol_enc: Option<Vec<u8>>,
    security_name_enc: Option<Vec<u8>>,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE investment_transactions SET
           name = CASE WHEN $1::bytea IS NOT NULL THEN '' ELSE name END,
           name_enc = COALESCE($1, name_enc),
           symbol = CASE WHEN $2::bytea IS NOT NULL THEN NULL ELSE symbol END,
           symbol_enc = COALESCE($2, symbol_enc),
           security_name = CASE WHEN $3::bytea IS NOT NULL THEN NULL ELSE security_name END,
           security_name_enc = COALESCE($3, security_name_enc)
         WHERE id = $4"
    )
    .bind(name_enc).bind(symbol_enc).bind(security_name_enc).bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Holdings with a real (non-sentinel) plaintext symbol or a plaintext name.
/// Synthetic cash/debt rows are excluded entirely — they stay plaintext.
pub async fn list_holding_rows_for_privacy(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<(i64, Option<String>, Option<String>)>, AppError> {
    let rows = sqlx::query_as::<_, (i64, Option<String>, Option<String>)>(
        "SELECT h.id,
                CASE WHEN h.symbol_enc IS NULL AND h.symbol NOT IN ('N/A') AND h.symbol <> '' THEN h.symbol END,
                CASE WHEN h.name_enc IS NULL THEN h.name END
         FROM holdings h
         JOIN accounts a ON h.account_id = a.id
         JOIN plaid_items pi ON a.plaid_item_id = pi.id
         WHERE pi.user_id = $1
           AND h.symbol NOT IN ('CASH','DEBT')
           AND ((h.symbol_enc IS NULL AND h.symbol NOT IN ('N/A') AND h.symbol <> '') OR h.name_enc IS NULL)"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Seal only the fields passed as Some — the others keep their current state.
pub async fn seal_holding_row(
    pool: &PgPool,
    id: i64,
    symbol_enc: Option<Vec<u8>>,
    name_enc: Option<Vec<u8>>,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE holdings SET
           symbol = CASE WHEN $1::bytea IS NOT NULL THEN '' ELSE symbol END,
           symbol_enc = COALESCE($1, symbol_enc),
           name = CASE WHEN $2::bytea IS NOT NULL THEN '' ELSE name END,
           name_enc = COALESCE($2, name_enc)
         WHERE id = $3"
    )
    .bind(symbol_enc).bind(name_enc).bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn seal_txn_row(
    pool: &PgPool,
    id: &str,
    raw_string_enc: Vec<u8>,
    merchant_name_enc: Option<Vec<u8>>,
    note_enc: Option<Vec<u8>>,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE transactions SET raw_string = '', raw_string_enc = $1,
             merchant_name = NULL, merchant_name_enc = $2,
             note = NULL, note_enc = $3
         WHERE id = $4"
    )
    .bind(raw_string_enc).bind(merchant_name_enc).bind(note_enc).bind(id)
    .execute(pool)
    .await?;
    Ok(())
}
