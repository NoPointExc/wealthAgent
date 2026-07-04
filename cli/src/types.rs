use serde::{Deserialize, Serialize};

// Mirror of backend models — kept in sync manually or via future codegen.

#[derive(Debug, Serialize, Deserialize)]
pub struct SuccessResponse<T> {
    pub status: String,
    pub data: T,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub custom_name: Option<String>,
    pub account_type: String,
    pub balance: i64,
    pub trend_pct: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Transaction {
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
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TransactionPage {
    pub items: Vec<Transaction>,
    pub total: i64,
    pub offset: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Holding {
    pub symbol: String,
    pub account_id: String,
    pub name: String,
    pub asset_value: i64,
    pub return_str: String,
    pub performance_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
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
    pub source: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UnknownBasisSale {
    pub symbol: Option<String>,
    pub security_name: Option<String>,
    pub close_date: String,
    pub quantity: f64,
    pub proceeds_cents: i64,
    pub txn_id: String,
    pub user_cost_basis_cents: Option<i64>,
    pub user_is_long_term: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct CapitalGainsSummary {
    pub ytd_realized_gain_cents: i64,
    pub ytd_realized_loss_cents: i64,
    pub ytd_net_cents: i64,
    pub unrealized_gain_cents: Option<i64>,
    pub unrealized_loss_cents: Option<i64>,
    pub short_term_net_cents: i64,
    pub long_term_net_cents: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CapitalGainsReport {
    pub realized_lots: Vec<RealizedLot>,
    pub unknown_basis_sales: Vec<UnknownBasisSale>,
    pub unrealized_positions: Vec<UnrealizedPosition>,
    pub summary: CapitalGainsSummary,
    pub tax_year: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WhoamiResponse {
    pub user_id: String,
    pub email: String,
    pub name: String,
    pub scopes: String,
}
