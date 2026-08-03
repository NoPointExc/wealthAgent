use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum AccountType {
    Asset,
    Liability,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub custom_name: Option<String>,
    pub balance: i64,
    pub trend_pct: Option<f64>,
    pub plaid_item_id: Option<String>,
    pub account_type: AccountType,
    // Sealed variants for privacy-encrypted users; opened into the plaintext
    // fields by privacy::reveal_account, never serialized to clients.
    #[serde(skip)]
    pub name_enc: Option<Vec<u8>>,
    #[serde(skip)]
    pub custom_name_enc: Option<Vec<u8>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAccountRequest {
    pub custom_name: Option<String>,
    pub account_type: Option<AccountType>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct AccountSnapshot {
    pub account_id: String,
    pub snapshot_date: chrono::NaiveDate,
    pub balance_cents: i64,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Holding {
    pub symbol: String,
    pub account_id: String,
    pub name: String,
    pub asset_value: i64,
    pub return_str: String,
    pub performance_type: String,
    #[serde(skip)]
    #[sqlx(default)]
    pub symbol_enc: Option<Vec<u8>>,
    #[serde(skip)]
    #[sqlx(default)]
    pub name_enc: Option<Vec<u8>>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct EnrichedTransaction {
    pub id: String,
    pub account_id: String,
    pub account_name: String,
    pub txn_date: String,
    pub raw_string: String,
    pub merchant_name: Option<String>,
    pub amount: i64,
    pub pending: bool,
    pub payment_channel: Option<String>,
    pub plaid_category: Option<String>,
    pub plaid_primary_category: Option<String>,
    pub plaid_detailed_category: Option<String>,
    pub tags: Option<String>,
    pub note: Option<String>,
    // Sealed variants for privacy-encrypted users; opened into the plaintext
    // fields by privacy::reveal_transaction, never serialized to clients.
    #[serde(skip)]
    pub raw_string_enc: Option<Vec<u8>>,
    #[serde(skip)]
    pub merchant_name_enc: Option<Vec<u8>>,
    #[serde(skip)]
    pub note_enc: Option<Vec<u8>>,
    #[serde(skip)]
    pub account_name_enc: Option<Vec<u8>>,
    #[serde(skip)]
    pub account_custom_name_enc: Option<Vec<u8>>,
}

#[derive(Debug, Deserialize)]
pub struct AggregateRequest {
    pub account_ids: Vec<String>,
    pub time_range: TimeRange,
}

#[derive(Debug, Deserialize)]
pub struct TimeRange {
    pub start_date: String,
    pub end_date: String,
    pub interval: String,
}

#[derive(Debug, Serialize)]
pub struct AggregateData {
    pub combined_history: Vec<TimelinePoint>,
    pub account_histories: std::collections::HashMap<String, Vec<TimelinePoint>>,
    pub holdings_breakdown: std::collections::HashMap<String, Vec<Holding>>,
    pub snapshot_counts: std::collections::HashMap<String, i64>,
}

#[derive(Debug, Serialize)]
pub struct TimelinePoint {
    pub date: String,
    pub value: i64,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct PlaidItem {
    pub id: String,
    pub access_token_enc: Option<Vec<u8>>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, Clone)]
pub struct SavedSearch {
    pub id: i32,
    pub name: String,
    pub search_query: Option<String>,
    pub selected_account: Option<String>,
    pub selected_categories: Option<String>,
    pub selected_tags: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub amount_mode: Option<String>,
    pub amount_val: Option<String>,
    pub amount_min: Option<String>,
    pub amount_max: Option<String>,
    pub show_pending_only: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateSavedSearchRequest {
    pub name: String,
    pub search_query: Option<String>,
    pub selected_account: Option<String>,
    pub selected_categories: Option<String>,
    pub selected_tags: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub amount_mode: Option<String>,
    pub amount_val: Option<String>,
    pub amount_min: Option<String>,
    pub amount_max: Option<String>,
    pub show_pending_only: bool,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BulkAction {
    AddTag,
    RemoveTag,
    SetNote,
    ClearNote,
}

#[derive(Debug, Deserialize)]
pub struct BulkUpdateRequest {
    pub transaction_ids: Vec<String>,
    pub action: BulkAction,
    pub value: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SuccessResponse<T> {
    pub status: String,
    pub data: T,
}

#[derive(Debug, Serialize)]
pub struct TransactionPage {
    pub items: Vec<EnrichedTransaction>,
    pub total: i64,
    pub offset: i64,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct TransactionQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    // Server-side filters for MCP/CLI access
    pub since: Option<String>,
    pub until: Option<String>,
    pub account_id: Option<String>,
    pub category: Option<String>,
    pub tag: Option<String>,
    pub search: Option<String>,
    pub pending: Option<bool>,
}

// ── API token management ──────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ApiTokenListItem {
    pub id: String,
    pub name: String,
    pub scopes: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTokenRequest {
    pub name: String,
    pub scopes: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateTokenResponse {
    pub id: String,
    pub token: String,
    pub name: String,
    pub scopes: String,
}

#[derive(Debug, Serialize)]
pub struct WhoamiResponse {
    pub user_id: String,
    pub email: String,
    pub name: String,
    pub scopes: String,
}

// ── OAuth 2.1 authorization server ────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
pub struct OAuthClient {
    pub client_id: String,
    pub client_name: String,
    pub redirect_uris: Vec<String>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct OAuthCodeRow {
    pub id: String,
    pub code_hash: String,
    pub client_id: String,
    pub user_id: String,
    pub redirect_uri: String,
    pub scope: String,
    pub resource: Option<String>,
    pub code_challenge: String,
}

/// One active OAuth connection (a user+client grant) for the "Connected agents" UI.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct OAuthGrantListItem {
    pub client_id: String,
    pub client_name: String,
    pub scopes: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct OAuthRefreshRow {
    pub id: String,
    pub token_hash: String,
    pub user_id: String,
    pub client_id: String,
    pub scope: String,
    pub resource: Option<String>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct InvestmentTransaction {
    pub id: String,
    pub account_id: String,
    pub account_name: String,
    pub date: String,
    pub name: String,
    pub amount: i64,
    pub quantity: Option<f64>,
    pub price: Option<i64>,
    pub fees: Option<i64>,
    pub txn_type: String,
    pub subtype: Option<String>,
    pub symbol: Option<String>,
    pub security_name: Option<String>,
    #[serde(skip)]
    #[sqlx(default)]
    pub name_enc: Option<Vec<u8>>,
    #[serde(skip)]
    #[sqlx(default)]
    pub symbol_enc: Option<Vec<u8>>,
    #[serde(skip)]
    #[sqlx(default)]
    pub security_name_enc: Option<Vec<u8>>,
    #[serde(skip)]
    #[sqlx(default)]
    pub account_name_enc: Option<Vec<u8>>,
    #[serde(skip)]
    #[sqlx(default)]
    pub account_custom_name_enc: Option<Vec<u8>>,
}

#[derive(Debug, Serialize)]
pub struct InvestmentTransactionPage {
    pub items: Vec<InvestmentTransaction>,
    pub total: i64,
    pub offset: i64,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: String,
    pub google_id: String,
    pub email: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct GoogleAuthRequest {
    pub credential: String,
    /// The user checked "I agree to the Terms of Service and Privacy Policy" at
    /// sign-in. Required for all real sign-ins; recorded against the account.
    #[serde(default)]
    pub accepted_terms: bool,
}

#[derive(Debug, Serialize)]
pub struct RealizedLot {
    pub symbol: Option<String>,
    pub security_name: Option<String>,
    pub open_date: String,
    pub close_date: String,
    pub quantity: f64,
    pub cost_basis_cents: i64,
    pub proceeds_cents: i64,
    pub gain_cents: i64,
    pub is_long_term: bool,
    /// "fifo" | "user_input" | "1099b"
    pub source: String,
    /// Plaid investment_transaction id — only set for user_input lots so the
    /// frontend can edit/clear the manual cost basis from the realized table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub txn_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UnrealizedPosition {
    pub symbol: Option<String>,
    pub security_name: Option<String>,
    pub oldest_lot_date: String,
    pub quantity: f64,
    pub cost_basis_cents: i64,
    pub current_value_cents: Option<i64>,
    pub gain_cents: Option<i64>,
    pub is_long_term_if_sold_today: bool,
    pub has_unknown_basis: bool,
}

#[derive(Debug, Serialize)]
pub struct CapitalGainsSummary {
    pub ytd_realized_gain_cents: i64,
    pub ytd_realized_loss_cents: i64,
    pub ytd_net_cents: i64,
    pub unrealized_gain_cents: Option<i64>,
    pub unrealized_loss_cents: Option<i64>,
    pub short_term_net_cents: i64,
    pub long_term_net_cents: i64,
}

#[derive(Debug, Serialize)]
pub struct UnknownBasisSale {
    pub symbol: Option<String>,
    pub security_name: Option<String>,
    pub close_date: String,
    pub quantity: f64,
    pub proceeds_cents: i64,
    /// Plaid investment_transaction id — used to key cost_basis_overrides
    pub txn_id: String,
    /// Populated if the user has already entered a manual cost basis
    pub user_cost_basis_cents: Option<i64>,
    pub user_is_long_term: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct SetCostBasisRequest {
    pub cost_basis_cents: i64,
    pub is_long_term: bool,
}

#[derive(Debug, Serialize)]
pub struct CapitalGainsReport {
    pub realized_lots: Vec<RealizedLot>,
    pub unknown_basis_sales: Vec<UnknownBasisSale>,
    pub unrealized_positions: Vec<UnrealizedPosition>,
    pub summary: CapitalGainsSummary,
    pub tax_year: i32,
}

#[derive(Debug, Deserialize, Default)]
pub struct CapitalGainsQuery {
    pub year: Option<i32>,
}
