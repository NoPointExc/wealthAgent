use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct LinkTokenCreateResponse {
    pub link_token: String,
    pub expiration: String,
    pub request_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ItemPublicTokenExchangeResponse {
    pub access_token: String,
    pub item_id: String,
    pub request_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SandboxPublicTokenCreateResponse {
    pub public_token: String,
    pub request_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PlaidAccount {
    pub account_id: String,
    pub balances: PlaidBalances,
    pub name: String,
    pub official_name: Option<String>,
    pub mask: Option<String>,
    pub r#type: String,
    pub subtype: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PlaidBalances {
    pub available: Option<f64>,
    pub current: f64,
    pub limit: Option<f64>,
    pub iso_currency_code: Option<String>,
    pub unofficial_currency_code: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AccountsGetResponse {
    pub accounts: Vec<PlaidAccount>,
    pub request_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PlaidPersonalFinanceCategory {
    pub primary: String,
    pub detailed: String,
    pub confidence_level: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PlaidTransaction {
    pub transaction_id: String,
    pub account_id: String,
    pub amount: f64,
    pub date: String,
    pub name: String,
    pub pending: bool,
    pub merchant_name: Option<String>,
    pub payment_channel: Option<String>,
    pub category: Option<Vec<String>>,
    pub personal_finance_category: Option<PlaidPersonalFinanceCategory>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TransactionsGetResponse {
    pub transactions: Vec<PlaidTransaction>,
    pub total_transactions: i32,
    pub request_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PlaidHolding {
    pub account_id: String,
    pub security_id: String,
    pub institution_value: f64,
    pub institution_price: f64,
    pub quantity: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PlaidSecurity {
    pub security_id: String,
    pub ticker_symbol: Option<String>,
    pub name: Option<String>,
    pub r#type: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HoldingsGetResponse {
    pub holdings: Vec<PlaidHolding>,
    pub securities: Vec<PlaidSecurity>,
    pub request_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PlaidInvestmentTransaction {
    pub investment_transaction_id: String,
    pub account_id: String,
    pub security_id: Option<String>,
    pub date: String,
    pub name: String,
    pub quantity: Option<f64>,
    pub amount: f64,
    pub price: Option<f64>,
    pub fees: Option<f64>,
    pub r#type: Option<String>,
    pub subtype: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InvestmentTransactionsGetResponse {
    pub investment_transactions: Vec<PlaidInvestmentTransaction>,
    pub securities: Vec<PlaidSecurity>,
    pub total_investment_transactions: i32,
    pub request_id: String,
}
