use reqwest::Client;
use serde_json::json;
use crate::error::AppError;
use super::models::*;
use std::env;

pub struct PlaidClient {
    http: Client,
    client_id: String,
    secret: String,
    base_url: String,
}

impl PlaidClient {
    pub fn new() -> Self {
        let client_id = env::var("PLAID_CLIENT_ID").expect("PLAID_CLIENT_ID not set");
        let secret = env::var("PLAID_SECRET").expect("PLAID_SECRET not set");
        let env_name = env::var("PLAID_ENV").unwrap_or_else(|_| "sandbox".to_string());
        
        let base_url = match env_name.as_str() {
            "sandbox" => "https://sandbox.plaid.com",
            "development" => "https://production.plaid.com", // Plaid Dev uses Prod URLs
            "production" => "https://production.plaid.com",
            _ => "https://sandbox.plaid.com",
        }.to_string();

        Self {
            http: Client::new(),
            client_id,
            secret,
            base_url,
        }
    }

    async fn post<T: serde::de::DeserializeOwned>(&self, path: &str, body: serde_json::Value) -> Result<T, AppError> {
        let mut body = body;
        if let Some(obj) = body.as_object_mut() {
            obj.insert("client_id".to_string(), json!(self.client_id));
            obj.insert("secret".to_string(), json!(self.secret));
        }

        let res = self.http.post(format!("{}{}", self.base_url, path))
            .json(&body)
            .send()
            .await?;

        if !res.status().is_success() {
            let err_text = res.text().await?;
            return Err(AppError::PlaidError(err_text));
        }

        Ok(res.json::<T>().await?)
    }

    pub async fn create_link_token(&self, user_id: &str) -> Result<String, AppError> {
        // Only products an institution supports ALL of let it appear in Link; anything
        // else is filtered out (or shows "Connectivity not supported" after OAuth select).
        // So `products` holds only the universal product (transactions) and the rest go in
        // `required_if_supported_products` — initialized when available, ignored otherwise.
        // This is what lets banks/Amex (no investments) and brokerages (no auth) all link.
        let split_env = |key: &str| -> Vec<String> {
            env::var(key)
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        };

        // Plaid requires a stable per-user identifier here; our opaque user id
        // (a UUID, not an email) is exactly that.
        let mut body = json!({
            "user": { "client_user_id": user_id },
            "client_name": "WealthAgent",
            "products": split_env("PLAID_PRODUCTS"),
            "country_codes": split_env("PLAID_COUNTRY_CODES"),
            "language": "en",
            "transactions": {
                "days_requested": 730
            }
        });

        let rins = split_env("PLAID_REQUIRED_IF_SUPPORTED_PRODUCTS");
        if !rins.is_empty() {
            body["required_if_supported_products"] = json!(rins);
        }

        // OAuth institutions (Amex, Chase, Capital One, Wells Fargo, BofA, …) require a
        // redirect_uri that is also registered in the Plaid Dashboard. Without it Link
        // cannot complete the bank's hosted-login handoff. Optional so sandbox/local dev
        // (no registered URI) keeps working.
        if let Ok(uri) = env::var("PLAID_REDIRECT_URI") {
            if !uri.trim().is_empty() {
                body["redirect_uri"] = json!(uri.trim());
            }
        }

        let res: LinkTokenCreateResponse = self.post("/link/token/create", body).await?;
        Ok(res.link_token)
    }

    /// Sandbox only: mint a public_token for a pre-canned sandbox institution
    /// without going through Link. Used by `admin demo seed` to build the demo
    /// template. `institution_id` defaults to `ins_109512` (First Platypus
    /// Bank, which carries both transactions and investments sandbox data).
    pub async fn sandbox_create_public_token(
        &self,
        institution_id: &str,
        products: &[&str],
    ) -> Result<String, AppError> {
        let body = json!({
            "institution_id": institution_id,
            "initial_products": products,
        });
        let res: SandboxPublicTokenCreateResponse =
            self.post("/sandbox/public_token/create", body).await?;
        Ok(res.public_token)
    }

    pub async fn exchange_public_token(&self, public_token: &str) -> Result<String, AppError> {
        let body = json!({
            "public_token": public_token
        });

        let res: ItemPublicTokenExchangeResponse = self.post("/item/public_token/exchange", body).await?;
        Ok(res.access_token)
    }

    pub async fn get_accounts(&self, access_token: &str) -> Result<Vec<PlaidAccount>, AppError> {
        let body = json!({
            "access_token": access_token
        });

        let res: AccountsGetResponse = self.post("/accounts/get", body).await?;
        Ok(res.accounts)
    }

    pub async fn get_transactions(&self, access_token: &str, start_date: &str, end_date: &str) -> Result<Vec<PlaidTransaction>, AppError> {
        let mut all_transactions = Vec::new();
        let mut offset = 0;
        let count = 500; // Fetch maximum page size of 500 to minimize API requests

        loop {
            let body = json!({
                "access_token": access_token,
                "start_date": start_date,
                "end_date": end_date,
                "options": {
                    "count": count,
                    "offset": offset
                }
            });

            let res: TransactionsGetResponse = self.post("/transactions/get", body).await?;
            let num_fetched = res.transactions.len();
            
            all_transactions.extend(res.transactions);

            // Break the loop if we have fetched all total transactions,
            // or if the current page returned 0 transactions (end of data)
            if all_transactions.len() >= res.total_transactions as usize || num_fetched == 0 {
                break;
            }

            offset += num_fetched;
        }

        Ok(all_transactions)
    }

    pub async fn get_holdings(&self, access_token: &str) -> Result<HoldingsGetResponse, AppError> {
        let body = json!({
            "access_token": access_token
        });

        // Some accounts don't have investments, this might return an error, we should handle it cleanly in main.rs
        Ok(self.post("/investments/holdings/get", body).await?)
    }

    pub async fn get_item_info(&self, access_token: &str) -> Result<serde_json::Value, AppError> {
        let body = json!({
            "access_token": access_token
        });

        Ok(self.post("/item/get", body).await?)
    }

    pub async fn get_institution(&self, institution_id: &str) -> Result<serde_json::Value, AppError> {
        let body = json!({
            "institution_id": institution_id,
            "country_codes": ["US"]
        });

        Ok(self.post("/institutions/get_by_id", body).await?)
    }

    pub async fn get_investment_transactions(
        &self,
        access_token: &str,
        start_date: &str,
        end_date: &str,
    ) -> Result<(Vec<PlaidInvestmentTransaction>, Vec<PlaidSecurity>), AppError> {
        use std::collections::HashSet;
        let mut all_txns = Vec::new();
        let mut securities = Vec::new();
        let mut seen_security_ids: HashSet<String> = HashSet::new();
        let mut offset = 0usize;
        let count = 500usize;

        loop {
            let body = serde_json::json!({
                "access_token": access_token,
                "start_date": start_date,
                "end_date": end_date,
                "options": { "count": count, "offset": offset }
            });
            let res: InvestmentTransactionsGetResponse =
                self.post("/investments/transactions/get", body).await?;
            let fetched = res.investment_transactions.len();
            // Plaid returns only the securities referenced by THIS page —
            // accumulate across pages or txns beyond the first page lose
            // their symbol/security_name.
            for sec in res.securities {
                if seen_security_ids.insert(sec.security_id.clone()) {
                    securities.push(sec);
                }
            }
            all_txns.extend(res.investment_transactions);
            if all_txns.len() >= res.total_investment_transactions as usize || fetched == 0 {
                break;
            }
            offset += fetched;
        }
        Ok((all_txns, securities))
    }

    pub async fn remove_item(&self, access_token: &str) -> Result<(), AppError> {
        let body = json!({ "access_token": access_token });
        let _: serde_json::Value = self.post("/item/remove", body).await?;
        Ok(())
    }
}
