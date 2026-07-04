//! Plaid → DB sync for a single Item: accounts, balances/snapshots, holdings,
//! bank transactions, and investment transactions. SQL is delegated to [`crate::db`].

use std::collections::HashMap;
use std::sync::Arc;

use crate::{db, error::AppError, models, privacy, AppState};

pub async fn sync_item(state: &Arc<AppState>, access_token: &str, item_id: &str) -> Result<(), AppError> {
    let pool = &state.pool;

    // Privacy encryption: if the item's owner opted in, seal PII text fields to
    // their public key at ingest — works even while the user is offline.
    let privacy_pubkey: Option<[u8; 32]> = db::get_privacy_pubkey_for_item(pool, item_id)
        .await?
        .map(|k| {
            k.as_slice().try_into()
                .map_err(|_| AppError::InternalServerError("privacy: bad public key length".to_string()))
        })
        .transpose()?;

    let mut inst_name = "Bank".to_string();
    if let Ok(item_res) = state.plaid.get_item_info(access_token).await {
        if let Some(inst_id) = item_res["item"]["institution_id"].as_str() {
            match state.plaid.get_institution(inst_id).await {
                Ok(inst_res) => {
                    if let Some(name) = inst_res["institution"]["name"].as_str() {
                        inst_name = name.to_string();
                    }
                }
                Err(e) => tracing::warn!("Could not fetch institution name: {}", e),
            }
        }
    } else {
        tracing::warn!(item_id = %item_id, "get_item_info failed — using 'Bank' as institution name");
    }

    let mut investment_account_ids = Vec::new();

    let plaid_accounts = state.plaid.get_accounts(access_token).await?;
    for acc in plaid_accounts {
        let acc_name_upper = acc.name.to_uppercase();
        let is_name_liability = acc_name_upper.contains("CREDIT")
            || acc_name_upper.contains("MORTGAGE")
            || acc_name_upper.contains("LOAN")
            || acc_name_upper.contains("DEBT");

        let existing_type = db::get_account_type(pool, &acc.account_id).await?;

        let account_type = if let Some(t) = existing_type { t }
            else if acc.r#type == "credit" || acc.r#type == "loan" || is_name_liability { models::AccountType::Liability }
            else { models::AccountType::Asset };

        let is_liability = account_type == models::AccountType::Liability;
        let direction = if is_liability { -1.0 } else { 1.0 };
        // .round(): f64 dollars*100 lands just under the integer (4.56 → 455.999…)
        // and `as i64` truncates, silently shaving a cent.
        let balance_cents = (acc.balances.current * 100.0 * direction).round() as i64;

        let inst_name_upper = inst_name.to_uppercase();
        let mut display_name = if acc_name_upper.starts_with(&inst_name_upper) { acc.name.clone() }
            else if inst_name_upper.starts_with(&acc_name_upper) { inst_name.clone() }
            else { format!("{} {}", inst_name, acc.name) };
        if let Some(mask) = &acc.mask { display_name = format!("{} - {}", display_name, mask); }

        let (name_plain, name_enc) = privacy::seal_required(privacy_pubkey.as_ref(), &display_name)?;
        db::upsert_account(pool, &acc.account_id, &name_plain, name_enc, balance_cents, item_id, account_type).await?;

        let is_investment = acc.r#type == "investment" || acc.r#type == "brokerage";

        // Write real daily snapshot — replaces synthesised history. Dated by the
        // ET trading day; for investment accounts, skip weekends (market closed).
        db::insert_account_snapshot(pool, &acc.account_id, balance_cents, is_investment).await?;

        if is_investment { investment_account_ids.push(acc.account_id.clone()); }

        if !is_investment {
            let holding_name = match acc.r#type.as_str() {
                "depository" => "Cash Balance",
                "credit" => "Credit Card Debt",
                "loan" => "Loan Balance",
                _ => "Other Balance",
            };
            let security_id = if is_liability { format!("DEBT_{}", acc.account_id) } else { format!("CASH_{}", acc.account_id) };
            let symbol = if is_liability { "DEBT" } else { "CASH" };
            db::upsert_holding(pool, symbol, &acc.account_id, holding_name, balance_cents, "0%", "neu", &security_id).await?;
        }
    }

    // Sync Transactions
    let end_date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let bank_start_date = (chrono::Local::now() - chrono::Duration::days(730)).format("%Y-%m-%d").to_string();
    // Investment txns pulled over a longer window so older cost-basis lots aren't lost.
    // Most brokerages cap at 24 months regardless, but Plaid will return whatever's available.
    let inv_start_date = (chrono::Local::now() - chrono::Duration::days(1825)).format("%Y-%m-%d").to_string();

    if let Ok(plaid_txns) = state.plaid.get_transactions(access_token, &bank_start_date, &end_date).await {
        for txn in plaid_txns {
            let amount_cents = (txn.amount * 100.0).round() as i64;
            let plaid_category_str = txn.category.map(|c| c.join(", "));
            let pfc_primary = txn.personal_finance_category.as_ref().map(|p| p.primary.clone());
            let pfc_detailed = txn.personal_finance_category.as_ref().map(|p| p.detailed.clone());

            let (raw_plain, raw_enc) = privacy::seal_required(privacy_pubkey.as_ref(), &txn.name)?;
            let (merchant_plain, merchant_enc) =
                privacy::seal_optional(privacy_pubkey.as_ref(), txn.merchant_name.clone())?;

            db::upsert_bank_transaction(
                pool,
                &txn.transaction_id,
                &txn.account_id,
                &txn.date,
                &raw_plain,
                raw_enc,
                &merchant_plain,
                merchant_enc,
                amount_cents,
                txn.pending,
                &txn.payment_channel,
                plaid_category_str,
                pfc_primary,
                pfc_detailed,
            ).await?;
        }
    }

    // Sync Holdings
    let mut allocated_per_account: HashMap<String, i64> = HashMap::new();
    for id in &investment_account_ids { allocated_per_account.insert(id.clone(), 0); }

    if let Ok(holdings_res) = state.plaid.get_holdings(access_token).await {
        for holding in holdings_res.holdings {
            let security = holdings_res.securities.iter().find(|s| s.security_id == holding.security_id);
            let name = security.and_then(|s| s.name.clone()).unwrap_or_else(|| "Unknown Security".to_string());
            let symbol = security.and_then(|s| s.ticker_symbol.clone()).unwrap_or_else(|| "N/A".to_string());
            let val_cents = (holding.institution_value * 100.0).round() as i64;
            *allocated_per_account.entry(holding.account_id.clone()).or_insert(0) += val_cents;

            // Real tickers/names are sealed; sentinel symbols ("N/A") stay plaintext.
            let (symbol_plain, symbol_enc) = if privacy::is_sentinel_symbol(&symbol) {
                (symbol.clone(), None)
            } else {
                privacy::seal_required(privacy_pubkey.as_ref(), &symbol)?
            };
            let (name_plain, name_enc) = privacy::seal_required(privacy_pubkey.as_ref(), &name)?;

            db::upsert_holding_with_qty(
                pool, &symbol_plain, symbol_enc, &holding.account_id, &name_plain, name_enc, val_cents,
                "Real-time", "pos", &holding.security_id, holding.quantity,
            ).await?;
        }
    }

    for (acc_id, allocated_cents) in allocated_per_account {
        if let Ok(account_balance) = db::get_account_balance(pool, &acc_id).await {
            let unallocated = account_balance - allocated_cents;
            if unallocated.abs() > 100 {
                let security_id = format!("UNALLOCATED_{}", acc_id);
                db::upsert_holding(pool, "CASH", &acc_id, "Unallocated Cash/Other", unallocated, "0%", "neu", &security_id).await?;
            }
        }
    }

    // Sync Investment Transactions
    match state.plaid.get_investment_transactions(access_token, &inv_start_date, &end_date).await {
        Ok((inv_txns, securities)) => {
            let sec_map: HashMap<String, (Option<String>, Option<String>)> = securities.into_iter()
                .map(|s| (s.security_id.clone(), (s.ticker_symbol, s.name)))
                .collect();

            for txn in inv_txns {
                let (symbol, security_name) = txn.security_id.as_deref()
                    .and_then(|sid| sec_map.get(sid))
                    .map(|(sym, name)| (sym.clone(), name.clone()))
                    .unwrap_or((None, None));

                let amount_cents = (txn.amount * 100.0).round() as i64;
                let price_cents = txn.price.map(|p| (p * 100.0).round() as i64);
                let fees_cents = txn.fees.map(|f| (f * 100.0).round() as i64);

                let (inv_name_plain, inv_name_enc) = privacy::seal_required(privacy_pubkey.as_ref(), &txn.name)?;
                let (symbol_plain, symbol_enc) = privacy::seal_symbol(privacy_pubkey.as_ref(), symbol)?;
                let (security_name_plain, security_name_enc) =
                    privacy::seal_optional(privacy_pubkey.as_ref(), security_name)?;

                db::upsert_investment_transaction(
                    pool,
                    &txn.investment_transaction_id,
                    &txn.account_id,
                    &txn.date,
                    &inv_name_plain,
                    inv_name_enc,
                    amount_cents,
                    txn.quantity,
                    price_cents,
                    fees_cents,
                    txn.r#type.as_deref().unwrap_or("unknown"),
                    &txn.subtype,
                    symbol_plain,
                    symbol_enc,
                    security_name_plain,
                    security_name_enc,
                ).await?;
            }
        }
        Err(e) => tracing::error!("Investment transactions failed for item {}: {:?}", item_id, e),
    }

    Ok(())
}
