use anyhow::{bail, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub base_url: String,
    pub token: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let token = std::env::var("WEALTH_API_TOKEN")
            .unwrap_or_default();
        // Point WEALTH_BASE_URL at your deployment; the default assumes a
        // backend running locally (see ops/deploy.sh for the server-side URL).
        let base_url = std::env::var("WEALTH_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:8080".to_string());

        if token.is_empty() {
            bail!(
                "WEALTH_API_TOKEN is not set.\n\
                 Set it to a token from Settings → API Tokens at {}/settings.",
                base_url
            );
        }
        if !token.starts_with("wa_pat_") {
            bail!("WEALTH_API_TOKEN must start with 'wa_pat_'. Got an unexpected format.");
        }
        Ok(Self { base_url, token })
    }
}
