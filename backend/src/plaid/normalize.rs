//! Plaid → WealthAgent value conventions, shared by the pull path
//! ([`super::sync`]) and the push path ([`crate::ingest`]) so both store
//! identical shapes for the same Plaid data.

use crate::models::AccountType;

/// Dollars (Plaid f64) → integer cents. `.round()` matters: 4.56 * 100.0 is
/// 455.999…, and a bare `as i64` would silently shave a cent.
pub fn cents(dollars: f64) -> i64 {
    (dollars * 100.0).round() as i64
}

/// Asset vs liability. An existing classification always wins, so a user's
/// manual flip survives re-syncs.
pub fn classify_account(plaid_type: &str, name: &str, existing: Option<AccountType>) -> AccountType {
    if let Some(t) = existing {
        return t;
    }
    let upper = name.to_uppercase();
    let name_says_liability = upper.contains("CREDIT")
        || upper.contains("MORTGAGE")
        || upper.contains("LOAN")
        || upper.contains("DEBT");
    if plaid_type == "credit" || plaid_type == "loan" || name_says_liability {
        AccountType::Liability
    } else {
        AccountType::Asset
    }
}

/// Stored balance: liabilities are negative.
pub fn signed_balance_cents(current: f64, account_type: AccountType) -> i64 {
    let direction = if account_type == AccountType::Liability { -1.0 } else { 1.0 };
    (current * 100.0 * direction).round() as i64
}

pub fn is_investment(plaid_type: &str) -> bool {
    plaid_type == "investment" || plaid_type == "brokerage"
}

/// "<Institution> <Account> - <mask>", without repeating the institution when
/// the account name already contains it. Ingest auto-matching relies on the
/// " - <mask>" suffix.
pub fn display_name(institution: &str, account_name: &str, mask: Option<&str>) -> String {
    let acc_upper = account_name.to_uppercase();
    let inst_upper = institution.to_uppercase();
    let mut name = if acc_upper.starts_with(&inst_upper) {
        account_name.to_string()
    } else if inst_upper.starts_with(&acc_upper) {
        institution.to_string()
    } else {
        format!("{} {}", institution, account_name)
    };
    if let Some(mask) = mask {
        name = format!("{} - {}", name, mask);
    }
    name
}

/// The synthetic single holding row a non-investment account carries:
/// (symbol, holding name, security_id).
pub fn cash_holding(plaid_type: &str, account_id: &str, account_type: AccountType) -> (&'static str, &'static str, String) {
    let is_liability = account_type == AccountType::Liability;
    let holding_name = match plaid_type {
        "depository" => "Cash Balance",
        "credit" => "Credit Card Debt",
        "loan" => "Loan Balance",
        _ => "Other Balance",
    };
    if is_liability {
        ("DEBT", holding_name, format!("DEBT_{}", account_id))
    } else {
        ("CASH", holding_name, format!("CASH_{}", account_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cents_rounds_instead_of_truncating() {
        assert_eq!(cents(4.56), 456);
        assert_eq!(cents(-12.345), -1235);
    }

    #[test]
    fn liability_classification() {
        assert_eq!(classify_account("credit", "Sapphire", None), AccountType::Liability);
        assert_eq!(classify_account("depository", "Auto Loan", None), AccountType::Liability);
        assert_eq!(classify_account("depository", "Checking", None), AccountType::Asset);
        assert_eq!(classify_account("credit", "X", Some(AccountType::Asset)), AccountType::Asset);
        assert_eq!(signed_balance_cents(10.0, AccountType::Liability), -1000);
    }

    #[test]
    fn display_names() {
        assert_eq!(display_name("Chase", "Checking", Some("1234")), "Chase Checking - 1234");
        assert_eq!(display_name("Chase", "CHASE Sapphire", None), "CHASE Sapphire");
        assert_eq!(display_name("Fidelity Investments", "Fidelity", Some("9")), "Fidelity Investments - 9");
    }
}
