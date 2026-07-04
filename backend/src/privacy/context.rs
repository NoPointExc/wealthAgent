//! Per-request crypto context and field-level seal/reveal helpers.

use std::sync::Arc;

use crate::{auth::AuthUser, db, error::AppError, AppState};

use super::crypto::{seal, unseal};

/// Shown in place of sealed fields when no key is available to open them.
pub const LOCKED_PLACEHOLDER: &str = "[locked]";

pub enum UserCrypto {
    /// User never opted in — all columns are plaintext.
    Plaintext,
    /// User is encrypted but no key is available on this request: sealed fields
    /// come back as [`LOCKED_PLACEHOLDER`]; new writes can still be sealed.
    Locked { public_key: [u8; 32] },
    Unlocked { public_key: [u8; 32], secret: [u8; 32] },
}

impl UserCrypto {
    pub fn is_plaintext(&self) -> bool {
        matches!(self, UserCrypto::Plaintext)
    }

    /// Public key for sealing writes — present whenever the user is encrypted.
    pub fn public_key(&self) -> Option<&[u8; 32]> {
        match self {
            UserCrypto::Plaintext => None,
            UserCrypto::Locked { public_key } | UserCrypto::Unlocked { public_key, .. } => Some(public_key),
        }
    }

    pub fn secret(&self) -> Option<&[u8; 32]> {
        match self {
            UserCrypto::Unlocked { secret, .. } => Some(secret),
            _ => None,
        }
    }

    fn open(&self, blob: &[u8]) -> String {
        match self.secret() {
            Some(sec) => unseal(sec, blob).unwrap_or_else(|_| LOCKED_PLACEHOLDER.to_string()),
            None => LOCKED_PLACEHOLDER.to_string(),
        }
    }
}

/// Resolve the crypto context for a request: PAT-carried key first (unwrapped
/// during auth), then the unlock cache, else locked.
pub async fn user_crypto(state: &Arc<AppState>, auth: &AuthUser) -> Result<UserCrypto, AppError> {
    let Some(keys) = db::get_privacy_keys(&state.pool, &auth.user_id).await? else {
        return Ok(UserCrypto::Plaintext);
    };
    let public_key: [u8; 32] = keys.public_key.as_slice().try_into()
        .map_err(|_| AppError::InternalServerError("privacy: bad public key length".into()))?;

    if let Some(sec) = &auth.privacy_secret {
        return Ok(UserCrypto::Unlocked { public_key, secret: sec.0 });
    }
    if let Some(secret) = state.unlocked_keys.get(&auth.user_id).await {
        return Ok(UserCrypto::Unlocked { public_key, secret });
    }
    Ok(UserCrypto::Locked { public_key })
}

// ── Field seal/reveal primitives ──────────────────────────────────────────────

/// Symbols that never get sealed: the synthetic cash/debt rows and no-ticker
/// placeholders reveal nothing about the user's positions, and the capital
/// gains engine special-cases them by value.
pub fn is_sentinel_symbol(s: &str) -> bool {
    matches!(s, "CASH" | "DEBT" | "N/A") || s.is_empty()
}

/// Seal a NOT NULL text column: plaintext becomes `""` when the user is encrypted.
pub fn seal_required(pk: Option<&[u8; 32]>, plain: &str) -> Result<(String, Option<Vec<u8>>), AppError> {
    match pk {
        Some(pk) => Ok((String::new(), Some(seal(pk, plain.as_bytes())?))),
        None => Ok((plain.to_string(), None)),
    }
}

/// Seal a nullable text column: plaintext becomes NULL when the user is encrypted.
pub fn seal_optional(pk: Option<&[u8; 32]>, plain: Option<String>) -> Result<(Option<String>, Option<Vec<u8>>), AppError> {
    match (pk, plain) {
        (Some(pk), Some(v)) => Ok((None, Some(seal(pk, v.as_bytes())?))),
        (_, v) => Ok((v, None)),
    }
}

/// Seal a symbol column: sentinels pass through in plaintext, real tickers are sealed.
pub fn seal_symbol(pk: Option<&[u8; 32]>, symbol: Option<String>) -> Result<(Option<String>, Option<Vec<u8>>), AppError> {
    match &symbol {
        Some(s) if !is_sentinel_symbol(s) => seal_optional(pk, symbol),
        _ => Ok((symbol, None)),
    }
}

/// Overwrite a plaintext field from its sealed column when one is present.
pub fn open_into(field: &mut String, enc: &Option<Vec<u8>>, crypto: &UserCrypto) {
    if let Some(blob) = enc {
        *field = crypto.open(blob);
    }
}

pub fn open_into_opt(field: &mut Option<String>, enc: &Option<Vec<u8>>, crypto: &UserCrypto) {
    if let Some(blob) = enc {
        *field = Some(crypto.open(blob));
    }
}

/// Split a user-supplied text into (plaintext_column, sealed_column) depending
/// on whether the user is encrypted. Used for notes and custom names.
pub fn seal_field(crypto: &UserCrypto, value: Option<String>) -> Result<(Option<String>, Option<Vec<u8>>), AppError> {
    seal_optional(crypto.public_key(), value)
}

// ── Sealed rows (decrypt after fetch) ─────────────────────────────────────────

/// Models whose rows carry sealed columns. `reveal` decrypts them in place.
/// This is a per-request operation, not a hydration hook: the key lives only
/// in the request's [`UserCrypto`] (or nowhere, when locked) — a process-wide
/// transparent-decryption layer is impossible by design.
pub trait Sealed {
    fn reveal(&mut self, crypto: &UserCrypto);
}

/// Decrypt a whole fetch result. No-op for plaintext users.
pub fn reveal_all<T: Sealed>(items: &mut [T], crypto: &UserCrypto) {
    if crypto.is_plaintext() {
        return;
    }
    for item in items {
        item.reveal(crypto);
    }
}

impl Sealed for crate::models::Account {
    fn reveal(&mut self, crypto: &UserCrypto) {
        open_into(&mut self.name, &self.name_enc, crypto);
        open_into_opt(&mut self.custom_name, &self.custom_name_enc, crypto);
    }
}

impl Sealed for crate::models::EnrichedTransaction {
    fn reveal(&mut self, crypto: &UserCrypto) {
        open_into(&mut self.raw_string, &self.raw_string_enc, crypto);
        open_into_opt(&mut self.merchant_name, &self.merchant_name_enc, crypto);
        open_into_opt(&mut self.note, &self.note_enc, crypto);
        // account_name is COALESCE(custom_name, name) — mirror that on sealed cols.
        if let Some(enc) = self.account_custom_name_enc.as_ref().or(self.account_name_enc.as_ref()) {
            self.account_name = crypto.open(enc);
        }
    }
}

impl Sealed for crate::models::InvestmentTransaction {
    fn reveal(&mut self, crypto: &UserCrypto) {
        open_into(&mut self.name, &self.name_enc, crypto);
        open_into_opt(&mut self.symbol, &self.symbol_enc, crypto);
        open_into_opt(&mut self.security_name, &self.security_name_enc, crypto);
        // account_name is COALESCE(custom_name, name) — mirror that on sealed cols.
        if let Some(enc) = self.account_custom_name_enc.as_ref().or(self.account_name_enc.as_ref()) {
            self.account_name = crypto.open(enc);
        }
    }
}

impl Sealed for crate::models::Holding {
    fn reveal(&mut self, crypto: &UserCrypto) {
        open_into(&mut self.symbol, &self.symbol_enc, crypto);
        open_into(&mut self.name, &self.name_enc, crypto);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privacy::crypto::test_keypair;

    #[test]
    fn sentinel_symbols() {
        for s in ["CASH", "DEBT", "N/A", ""] {
            assert!(is_sentinel_symbol(s), "{s:?} should be a sentinel");
        }
        for s in ["AAPL", "BRK.B", "cash", "VTSAX"] {
            assert!(!is_sentinel_symbol(s), "{s:?} should not be a sentinel");
        }
    }

    #[test]
    fn seal_required_both_modes() {
        let (sk, pk) = test_keypair();
        // Plaintext user: value passes through, no ciphertext.
        assert_eq!(seal_required(None, "Chase Checking").unwrap(), ("Chase Checking".to_string(), None));
        // Encrypted user: column emptied, blob decrypts to the original.
        let (plain, enc) = seal_required(Some(&pk), "Chase Checking").unwrap();
        assert_eq!(plain, "");
        assert_eq!(unseal(&sk, &enc.unwrap()).unwrap(), "Chase Checking");
    }

    #[test]
    fn seal_optional_both_modes() {
        let (sk, pk) = test_keypair();
        assert_eq!(seal_optional(None, Some("note".into())).unwrap(), (Some("note".to_string()), None));
        assert_eq!(seal_optional(Some(&pk), None).unwrap(), (None, None));
        let (plain, enc) = seal_optional(Some(&pk), Some("note".into())).unwrap();
        assert_eq!(plain, None);
        assert_eq!(unseal(&sk, &enc.unwrap()).unwrap(), "note");
    }

    #[test]
    fn seal_symbol_passes_sentinels_seals_tickers() {
        let (sk, pk) = test_keypair();
        // Sentinels stay plaintext even for encrypted users.
        for s in ["CASH", "DEBT", "N/A"] {
            assert_eq!(seal_symbol(Some(&pk), Some(s.into())).unwrap(), (Some(s.to_string()), None));
        }
        assert_eq!(seal_symbol(Some(&pk), None).unwrap(), (None, None));
        // Real tickers are sealed.
        let (plain, enc) = seal_symbol(Some(&pk), Some("ACHN".into())).unwrap();
        assert_eq!(plain, None);
        assert_eq!(unseal(&sk, &enc.unwrap()).unwrap(), "ACHN");
        // Plaintext users keep everything as-is.
        assert_eq!(seal_symbol(None, Some("ACHN".into())).unwrap(), (Some("ACHN".to_string()), None));
    }

    #[test]
    fn open_into_unlocked_locked_and_untouched() {
        let (sk, pk) = test_keypair();
        let unlocked = UserCrypto::Unlocked { public_key: pk, secret: sk };
        let locked = UserCrypto::Locked { public_key: pk };
        let blob = Some(seal(&pk, b"Achillion Pharmaceuticals Inc.").unwrap());

        let mut field = String::new();
        open_into(&mut field, &blob, &unlocked);
        assert_eq!(field, "Achillion Pharmaceuticals Inc.");

        let mut field = String::new();
        open_into(&mut field, &blob, &locked);
        assert_eq!(field, LOCKED_PLACEHOLDER);

        // No sealed column → plaintext field is left alone.
        let mut field = "CASH".to_string();
        open_into(&mut field, &None, &unlocked);
        assert_eq!(field, "CASH");

        let mut opt = None;
        open_into_opt(&mut opt, &blob, &unlocked);
        assert_eq!(opt.as_deref(), Some("Achillion Pharmaceuticals Inc."));
        let mut opt = Some("plain".to_string());
        open_into_opt(&mut opt, &None, &locked);
        assert_eq!(opt.as_deref(), Some("plain"));
    }

    #[test]
    fn sealed_trait_roundtrip_investment_transaction() {
        let (sk, pk) = test_keypair();
        let make = || crate::models::InvestmentTransaction {
            id: "t1".into(),
            account_id: "a1".into(),
            account_name: "".into(),
            date: "2026-01-02".into(),
            name: "".into(),
            amount: 12345,
            quantity: Some(2.0),
            price: Some(6000),
            fees: Some(100),
            txn_type: "buy".into(),
            subtype: None,
            symbol: None,
            security_name: None,
            name_enc: Some(seal(&pk, b"BUY Achillion Pharmaceuticals Inc.").unwrap()),
            symbol_enc: Some(seal(&pk, b"ACHN").unwrap()),
            security_name_enc: Some(seal(&pk, b"Achillion Pharmaceuticals Inc.").unwrap()),
            account_name_enc: Some(seal(&pk, b"Fidelity Brokerage - 1234").unwrap()),
            account_custom_name_enc: None,
        };

        let mut t = make();
        t.reveal(&UserCrypto::Unlocked { public_key: pk, secret: sk });
        assert_eq!(t.name, "BUY Achillion Pharmaceuticals Inc.");
        assert_eq!(t.symbol.as_deref(), Some("ACHN"));
        assert_eq!(t.security_name.as_deref(), Some("Achillion Pharmaceuticals Inc."));
        assert_eq!(t.account_name, "Fidelity Brokerage - 1234");

        let mut t = make();
        t.reveal(&UserCrypto::Locked { public_key: pk });
        assert_eq!(t.name, LOCKED_PLACEHOLDER);
        assert_eq!(t.symbol.as_deref(), Some(LOCKED_PLACEHOLDER));
    }
}
