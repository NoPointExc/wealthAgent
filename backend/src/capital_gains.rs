//! FIFO capital-gains engine.
//!
//! Pure computation over already-fetched, already-decrypted rows: no SQL, no
//! crypto keys, no async. Fetching and decryption live in
//! [`crate::handlers::gains::inner_get_capital_gains`], which makes everything
//! here unit-testable without a database.

use std::collections::{HashMap, VecDeque};

use chrono::NaiveDate;

use crate::{db, models, privacy};

const EPSILON: f64 = 1e-6;

struct FifoLot {
    symbol: Option<String>,
    security_name: Option<String>,
    open_date: NaiveDate,
    quantity: f64,
    cost_basis_cents: i64,
}

/// Aggregate holdings per symbol after decryption: symbol → (value, quantity).
/// Replaces the SQL GROUP BY that sealed symbols made impossible; sentinel
/// symbols (CASH/DEBT/N/A) are skipped like the old NOT IN filter.
pub fn build_holdings_map(rows: Vec<db::HoldingForCG>, crypto: &privacy::UserCrypto) -> HashMap<String, (i64, f64)> {
    let mut map: HashMap<String, (i64, f64)> = HashMap::new();
    for mut h in rows {
        privacy::open_into(&mut h.symbol, &h.symbol_enc, crypto);
        if privacy::is_sentinel_symbol(&h.symbol) {
            continue;
        }
        let entry = map.entry(h.symbol).or_insert((0, 0.0));
        entry.0 += h.asset_value;
        entry.1 += h.quantity.unwrap_or(0.0);
    }
    map
}

/// Walk date-ordered transactions, matching sells against buy lots FIFO.
/// Returns realized lots, sells with no tracked buy lot, and the still-open
/// lot queues (per symbol) for the unrealized table.
fn match_fifo_lots(
    txns: &[db::CapGainsTxnRow],
) -> (Vec<models::RealizedLot>, Vec<models::UnknownBasisSale>, HashMap<String, VecDeque<FifoLot>>) {
    let mut lot_queues: HashMap<String, VecDeque<FifoLot>> = HashMap::new();
    let mut realized_lots: Vec<models::RealizedLot> = Vec::new();
    let mut unknown_basis_sales: Vec<models::UnknownBasisSale> = Vec::new();

    for txn in txns {
        let symbol = match &txn.symbol {
            Some(s) if !matches!(s.as_str(), "CASH" | "DEBT" | "N/A") => s.clone(),
            _ => continue,
        };
        let quantity = match txn.quantity {
            Some(q) if q > EPSILON => q,
            _ => continue,
        };
        let txn_date = match NaiveDate::parse_from_str(&txn.date, "%Y-%m-%d") {
            Ok(d) => d,
            Err(_) => continue,
        };

        let is_buy = txn.txn_type == "buy" || txn.subtype.as_deref() == Some("buy");
        let is_sell = txn.txn_type == "sell" || txn.subtype.as_deref() == Some("sell");

        if is_buy {
            let cost_basis = txn.amount.abs() + txn.fees.unwrap_or(0).abs();
            lot_queues.entry(symbol.clone()).or_default().push_back(FifoLot {
                symbol: txn.symbol.clone(),
                security_name: txn.security_name.clone(),
                open_date: txn_date,
                quantity,
                cost_basis_cents: cost_basis,
            });
        } else if is_sell {
            let net_proceeds = txn.amount.abs().saturating_sub(txn.fees.unwrap_or(0).abs());
            let mut remaining_sell_qty = quantity;
            let close_date = txn_date;

            let queue = lot_queues.entry(symbol.clone()).or_default();
            while remaining_sell_qty > EPSILON && !queue.is_empty() {
                let lot = queue.front_mut().unwrap();
                let matched_qty = remaining_sell_qty.min(lot.quantity);

                let fraction_of_lot = matched_qty / lot.quantity;
                let matched_cost = (lot.cost_basis_cents as f64 * fraction_of_lot).round() as i64;
                let fraction_of_sell = matched_qty / quantity;
                let matched_proceeds = (net_proceeds as f64 * fraction_of_sell).round() as i64;

                let gain_cents = matched_proceeds - matched_cost;
                let hold_days = (close_date - lot.open_date).num_days();
                let is_long_term = hold_days >= 365;

                realized_lots.push(models::RealizedLot {
                    symbol: lot.symbol.clone(),
                    security_name: lot.security_name.clone(),
                    open_date: lot.open_date.to_string(),
                    close_date: close_date.to_string(),
                    quantity: matched_qty,
                    cost_basis_cents: matched_cost,
                    proceeds_cents: matched_proceeds,
                    gain_cents,
                    is_long_term,
                    source: "fifo".to_string(),
                    txn_id: None,
                });

                lot.quantity -= matched_qty;
                lot.cost_basis_cents -= matched_cost;
                remaining_sell_qty -= matched_qty;

                if lot.quantity < EPSILON {
                    queue.pop_front();
                }
            }

            // Any remaining sell qty has no tracked buy lot (purchased before sync window)
            if remaining_sell_qty > EPSILON {
                let fraction_unmatched = remaining_sell_qty / quantity;
                let unmatched_proceeds = (net_proceeds as f64 * fraction_unmatched).round() as i64;
                unknown_basis_sales.push(models::UnknownBasisSale {
                    symbol: txn.symbol.clone(),
                    security_name: txn.security_name.clone(),
                    close_date: close_date.to_string(),
                    quantity: remaining_sell_qty,
                    proceeds_cents: unmatched_proceeds,
                    txn_id: txn.id.clone(),
                    user_cost_basis_cents: None,
                    user_is_long_term: None,
                });
            }
        }
    }

    (realized_lots, unknown_basis_sales, lot_queues)
}

/// Resolve unknown-basis sales that have a user-supplied cost basis: they move
/// into `realized_lots` with source "user_input" (keeping the txn_id so the
/// frontend can still edit/clear the manual basis) and drop out of
/// `unknown_basis_sales`, which should only list sales still needing a basis.
fn apply_cost_basis_overrides(
    realized_lots: &mut Vec<models::RealizedLot>,
    unknown_basis_sales: &mut Vec<models::UnknownBasisSale>,
    overrides: &HashMap<String, (i64, bool)>,
) {
    for sale in unknown_basis_sales.iter_mut() {
        if let Some(&(cb_cents, is_lt)) = overrides.get(&sale.txn_id) {
            sale.user_cost_basis_cents = Some(cb_cents);
            sale.user_is_long_term = Some(is_lt);
            let gain = sale.proceeds_cents - cb_cents;
            realized_lots.push(models::RealizedLot {
                symbol: sale.symbol.clone(),
                security_name: sale.security_name.clone(),
                open_date: "N/A".to_string(),
                close_date: sale.close_date.clone(),
                quantity: sale.quantity,
                cost_basis_cents: cb_cents,
                proceeds_cents: sale.proceeds_cents,
                gain_cents: gain,
                is_long_term: is_lt,
                source: "user_input".to_string(),
                txn_id: Some(sale.txn_id.clone()),
            });
        }
    }
    unknown_basis_sales.retain(|s| s.user_cost_basis_cents.is_none());
}

/// Group the still-open lots per symbol and price them against current
/// holdings. Sorted losses-first (harvesting candidates at top).
fn build_unrealized_positions(
    lot_queues: &HashMap<String, VecDeque<FifoLot>>,
    holdings_map: &HashMap<String, (i64, f64)>,
    today: NaiveDate,
) -> Vec<models::UnrealizedPosition> {
    // symbol → (security_name, oldest_open_date, total_qty, total_cost)
    let mut unrealized_map: HashMap<String, (Option<String>, NaiveDate, f64, i64)> = HashMap::new();

    for (symbol, queue) in lot_queues {
        for lot in queue {
            if lot.quantity < EPSILON { continue; }
            let entry = unrealized_map.entry(symbol.clone()).or_insert_with(|| {
                (lot.security_name.clone(), lot.open_date, 0.0, 0)
            });
            if lot.open_date < entry.1 { entry.1 = lot.open_date; }
            entry.2 += lot.quantity;
            entry.3 += lot.cost_basis_cents;
        }
    }

    let mut unrealized_positions: Vec<models::UnrealizedPosition> = unrealized_map
        .into_iter()
        .map(|(symbol, (security_name, oldest_date, total_qty, cost_basis))| {
            let current_value_cents = holdings_map.get(&symbol).and_then(|(total_val, total_qty_h)| {
                if *total_qty_h < EPSILON { return None; }
                let fraction = (total_qty / total_qty_h).min(1.0);
                Some((*total_val as f64 * fraction).round() as i64)
            });
            let gain_cents = current_value_cents.map(|cv| cv - cost_basis);
            let hold_days = (today - oldest_date).num_days();
            let is_long_term_if_sold_today = hold_days >= 365;
            let has_unknown_basis = holdings_map
                .get(&symbol)
                .map(|(_, hq)| *hq > total_qty + EPSILON)
                .unwrap_or(false);

            models::UnrealizedPosition {
                symbol: Some(symbol),
                security_name,
                oldest_lot_date: oldest_date.to_string(),
                quantity: total_qty,
                cost_basis_cents: cost_basis,
                current_value_cents,
                gain_cents,
                is_long_term_if_sold_today,
                has_unknown_basis,
            }
        })
        .collect();

    unrealized_positions.sort_by(|a, b| {
        a.gain_cents.unwrap_or(i64::MAX).cmp(&b.gain_cents.unwrap_or(i64::MAX))
    });
    unrealized_positions
}

fn summarize(
    realized_lots: &[models::RealizedLot],
    unrealized_positions: &[models::UnrealizedPosition],
    year_start: NaiveDate,
    year_end: NaiveDate,
) -> models::CapitalGainsSummary {
    let mut ytd_gain: i64 = 0;
    let mut ytd_loss: i64 = 0;
    let mut st_net:   i64 = 0;
    let mut lt_net:   i64 = 0;

    for lot in realized_lots {
        let cd = NaiveDate::parse_from_str(&lot.close_date, "%Y-%m-%d").unwrap_or_default();
        if cd >= year_start && cd <= year_end {
            if lot.gain_cents >= 0 { ytd_gain += lot.gain_cents; } else { ytd_loss += lot.gain_cents; }
            if lot.is_long_term { lt_net += lot.gain_cents; } else { st_net += lot.gain_cents; }
        }
    }

    let unrealized_gain_cents = unrealized_positions.iter().filter_map(|p| p.gain_cents).filter(|g| *g >= 0).reduce(|a, b| a + b);
    let unrealized_loss_cents = unrealized_positions.iter().filter_map(|p| p.gain_cents).filter(|g| *g <  0).reduce(|a, b| a + b);

    models::CapitalGainsSummary {
        ytd_realized_gain_cents: ytd_gain,
        ytd_realized_loss_cents: ytd_loss,
        ytd_net_cents: ytd_gain + ytd_loss,
        unrealized_gain_cents,
        unrealized_loss_cents,
        short_term_net_cents: st_net,
        long_term_net_cents: lt_net,
    }
}

/// Full report for one tax year from decrypted inputs:
/// FIFO-match → apply user cost-basis overrides → price open lots → summarize
/// → filter the realized/unknown tables to the requested year.
pub fn compute_report(
    txns: &[db::CapGainsTxnRow],
    holdings_map: &HashMap<String, (i64, f64)>,
    overrides: &HashMap<String, (i64, bool)>,
    tax_year: i32,
    today: NaiveDate,
) -> models::CapitalGainsReport {
    let (mut realized_lots, mut unknown_basis_sales, lot_queues) = match_fifo_lots(txns);
    apply_cost_basis_overrides(&mut realized_lots, &mut unknown_basis_sales, overrides);
    let unrealized_positions = build_unrealized_positions(&lot_queues, holdings_map, today);

    let year_start = NaiveDate::from_ymd_opt(tax_year, 1, 1).unwrap_or_default();
    let year_end   = NaiveDate::from_ymd_opt(tax_year, 12, 31).unwrap_or_default();

    let summary = summarize(&realized_lots, &unrealized_positions, year_start, year_end);

    let in_year = |close_date: &str| {
        let cd = NaiveDate::parse_from_str(close_date, "%Y-%m-%d").unwrap_or_default();
        cd >= year_start && cd <= year_end
    };
    let ytd_realized: Vec<models::RealizedLot> = realized_lots
        .into_iter()
        .filter(|lot| in_year(&lot.close_date))
        .collect();
    let ytd_unknown: Vec<models::UnknownBasisSale> = unknown_basis_sales
        .into_iter()
        .filter(|s| in_year(&s.close_date))
        .collect();

    models::CapitalGainsReport {
        realized_lots: ytd_realized,
        unknown_basis_sales: ytd_unknown,
        unrealized_positions,
        summary,
        tax_year,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cg_row(symbol: &str, value: i64, qty: Option<f64>, symbol_enc: Option<Vec<u8>>) -> db::HoldingForCG {
        db::HoldingForCG { symbol: symbol.to_string(), asset_value: value, quantity: qty, symbol_enc }
    }

    fn txn(id: &str, date: &str, symbol: &str, txn_type: &str, quantity: f64, amount: i64) -> db::CapGainsTxnRow {
        db::CapGainsTxnRow {
            id: id.to_string(),
            date: date.to_string(),
            symbol: Some(symbol.to_string()),
            security_name: None,
            txn_type: txn_type.to_string(),
            subtype: None,
            quantity: Some(quantity),
            fees: None,
            amount,
            symbol_enc: None,
            security_name_enc: None,
        }
    }

    /// Parity with the SQL this replaced:
    /// SUM(asset_value), SUM(quantity) GROUP BY symbol, NOT IN ('CASH','DEBT','N/A').
    #[test]
    fn build_holdings_map_groups_and_skips_sentinels() {
        let rows = vec![
            cg_row("AAPL", 10_000, Some(2.0), None),
            cg_row("AAPL", 5_000, Some(1.0), None),
            cg_row("VTI", 7_000, None, None),
            cg_row("CASH", 99_999, Some(1.0), None),
            cg_row("DEBT", -5_000, None, None),
            cg_row("N/A", 123, None, None),
        ];
        let map = build_holdings_map(rows, &privacy::UserCrypto::Plaintext);
        assert_eq!(map.len(), 2);
        assert_eq!(map["AAPL"], (15_000, 3.0));
        assert_eq!(map["VTI"], (7_000, 0.0));
    }

    #[test]
    fn build_holdings_map_decrypts_sealed_symbols() {
        use x25519_dalek::{PublicKey, StaticSecret};
        let sk = StaticSecret::random_from_rng(chacha20poly1305::aead::OsRng);
        let pk = PublicKey::from(&sk);
        let (sk, pk) = (sk.to_bytes(), pk.to_bytes());

        // Same ticker sealed twice (randomized blobs) must still group together.
        let rows = vec![
            cg_row("", 10_000, Some(2.0), Some(privacy::seal(&pk, b"ACHN").unwrap())),
            cg_row("", 5_000, Some(1.0), Some(privacy::seal(&pk, b"ACHN").unwrap())),
            cg_row("CASH", 99_999, Some(1.0), None), // synthetic rows stay plaintext
        ];
        let unlocked = privacy::UserCrypto::Unlocked { public_key: pk, secret: sk };
        let map = build_holdings_map(rows, &unlocked);
        assert_eq!(map.len(), 1);
        assert_eq!(map["ACHN"], (15_000, 3.0));
    }

    #[test]
    fn fifo_splits_sell_across_lots_with_term_per_lot() {
        // Two AAPL buys, one sell of 15 spanning both lots.
        let txns = vec![
            txn("b1", "2024-01-02", "AAPL", "buy", 10.0, 100_000),
            txn("b2", "2025-06-01", "AAPL", "buy", 10.0, 150_000),
            txn("s1", "2026-01-15", "AAPL", "sell", 15.0, 300_000),
        ];
        let today = NaiveDate::from_ymd_opt(2026, 7, 4).unwrap();
        let holdings = HashMap::from([("AAPL".to_string(), (250_000i64, 5.0f64))]);
        let report = compute_report(&txns, &holdings, &HashMap::new(), 2026, today);

        assert_eq!(report.realized_lots.len(), 2);
        let lot1 = &report.realized_lots[0]; // full first lot, held > 1 year
        assert_eq!((lot1.quantity, lot1.cost_basis_cents, lot1.proceeds_cents), (10.0, 100_000, 200_000));
        assert!(lot1.is_long_term);
        let lot2 = &report.realized_lots[1]; // 5 of the second lot, held < 1 year
        assert_eq!((lot2.quantity, lot2.cost_basis_cents, lot2.proceeds_cents), (5.0, 75_000, 100_000));
        assert!(!lot2.is_long_term);

        assert!(report.unknown_basis_sales.is_empty());
        assert_eq!(report.summary.ytd_net_cents, 125_000);
        assert_eq!(report.summary.long_term_net_cents, 100_000);
        assert_eq!(report.summary.short_term_net_cents, 25_000);

        // 5 shares of lot 2 remain open, priced from the holdings map.
        assert_eq!(report.unrealized_positions.len(), 1);
        let pos = &report.unrealized_positions[0];
        assert_eq!((pos.quantity, pos.cost_basis_cents), (5.0, 75_000));
        assert_eq!(pos.current_value_cents, Some(250_000));
        assert_eq!(pos.gain_cents, Some(175_000));
    }

    #[test]
    fn sell_without_lot_needs_basis_until_override_resolves_it() {
        let txns = vec![txn("s2", "2026-02-01", "VTI", "sell", 5.0, 50_000)];
        let today = NaiveDate::from_ymd_opt(2026, 7, 4).unwrap();
        let holdings = HashMap::new();

        // No override: the sale sits in unknown_basis_sales.
        let report = compute_report(&txns, &holdings, &HashMap::new(), 2026, today);
        assert_eq!(report.realized_lots.len(), 0);
        assert_eq!(report.unknown_basis_sales.len(), 1);
        assert_eq!(report.unknown_basis_sales[0].proceeds_cents, 50_000);

        // With an override it moves into realized_lots as a manual entry.
        let overrides = HashMap::from([("s2".to_string(), (40_000i64, true))]);
        let report = compute_report(&txns, &holdings, &overrides, 2026, today);
        assert!(report.unknown_basis_sales.is_empty());
        assert_eq!(report.realized_lots.len(), 1);
        let lot = &report.realized_lots[0];
        assert_eq!((lot.gain_cents, lot.is_long_term, lot.source.as_str()), (10_000, true, "user_input"));
        assert_eq!(lot.txn_id.as_deref(), Some("s2"));
    }
}
