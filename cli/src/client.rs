use crate::{config::Config, error::ClientError, types::*};
use reqwest::{Client, StatusCode};
use serde_json::Value;

pub struct WealthClient {
    http: Client,
    base_url: String,
    token: String,
}

impl WealthClient {
    pub fn new(cfg: &Config) -> Self {
        Self {
            http: Client::new(),
            base_url: cfg.base_url.trim_end_matches('/').to_string(),
            token: cfg.token.clone(),
        }
    }

    async fn get(&self, path: &str) -> Result<Value, ClientError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.http.get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        match resp.status() {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                return Err(ClientError::Auth("Token invalid or revoked. Run `wealth auth login` to set a new token.".to_string()));
            }
            StatusCode::NOT_FOUND => {
                return Err(ClientError::NotFound(format!("Not found: {}", path)));
            }
            s if !s.is_success() => {
                let body = resp.text().await.unwrap_or_default();
                return Err(ClientError::Server(format!("HTTP {}: {}", s, body)));
            }
            _ => {}
        }

        resp.json::<Value>().await.map_err(|e| ClientError::Server(e.to_string()))
    }

    async fn post_empty(&self, path: &str) -> Result<Value, ClientError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.http.post(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            if status == StatusCode::UNAUTHORIZED {
                return Err(ClientError::Auth("Unauthorized".to_string()));
            }
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Server(format!("HTTP {}: {}", status, body)));
        }
        resp.json::<Value>().await.map_err(|e| ClientError::Server(e.to_string()))
    }

    pub async fn whoami(&self) -> Result<WhoamiResponse, ClientError> {
        let v = self.get("/api/auth/whoami").await?;
        serde_json::from_value(v["data"].clone()).map_err(|e| ClientError::Server(e.to_string()))
    }

    pub async fn list_accounts(&self) -> Result<Vec<Account>, ClientError> {
        let v = self.get("/api/accounts").await?;
        serde_json::from_value(v["data"].clone()).map_err(|e| ClientError::Server(e.to_string()))
    }

    pub async fn list_transactions(&self, params: &TxListParams) -> Result<TransactionPage, ClientError> {
        let mut qs = format!("/api/transactions?limit={}&offset={}", params.limit.unwrap_or(200), params.offset.unwrap_or(0));
        if let Some(v) = &params.since    { qs.push_str(&format!("&since={}", v)); }
        if let Some(v) = &params.until    { qs.push_str(&format!("&until={}", v)); }
        if let Some(v) = &params.account_id { qs.push_str(&format!("&account_id={}", v)); }
        if let Some(v) = &params.category { qs.push_str(&format!("&category={}", v)); }
        if let Some(v) = &params.tag      { qs.push_str(&format!("&tag={}", urlencoding::encode(v))); }
        if let Some(v) = &params.search   { qs.push_str(&format!("&search={}", urlencoding::encode(v))); }
        if let Some(v) = params.pending   { qs.push_str(&format!("&pending={}", v)); }
        let v = self.get(&qs).await?;
        serde_json::from_value(v["data"].clone()).map_err(|e| ClientError::Server(e.to_string()))
    }

    pub async fn get_capital_gains(&self, year: Option<i32>) -> Result<CapitalGainsReport, ClientError> {
        let path = match year {
            Some(y) => format!("/api/capital_gains?year={}", y),
            None => "/api/capital_gains".to_string(),
        };
        let v = self.get(&path).await?;
        serde_json::from_value(v["data"].clone()).map_err(|e| ClientError::Server(e.to_string()))
    }

    pub async fn sync(&self) -> Result<(), ClientError> {
        self.post_empty("/api/plaid/sync").await?;
        Ok(())
    }

    pub async fn bulk_update_transactions(
        &self,
        ids: Vec<String>,
        action: &str,
        value: Option<&str>,
    ) -> Result<(), ClientError> {
        let url = format!("{}/api/transactions/bulk_update", self.base_url);
        let body = serde_json::json!({
            "transaction_ids": ids,
            "action": action,
            "value": value,
        });
        let resp = self.http.post(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Server(body));
        }
        Ok(())
    }

    pub async fn update_account(&self, id: &str, custom_name: Option<&str>) -> Result<(), ClientError> {
        let url = format!("{}/api/accounts/{}", self.base_url, id);
        let body = serde_json::json!({ "custom_name": custom_name });
        let resp = self.http.patch(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Server(body));
        }
        Ok(())
    }
}

// Query params struct used by list_transactions
#[derive(Debug, Default)]
pub struct TxListParams {
    pub since: Option<String>,
    pub until: Option<String>,
    pub account_id: Option<String>,
    pub category: Option<String>,
    pub tag: Option<String>,
    pub search: Option<String>,
    pub pending: Option<bool>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

// Simple percent-encoding for query values (avoids a full dep)
mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut out = String::new();
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    out.push(b as char)
                }
                _ => out.push_str(&format!("%{:02X}", b)),
            }
        }
        out
    }
}
