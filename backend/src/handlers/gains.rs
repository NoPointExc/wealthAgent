//! Capital-gains handlers. Orchestration only: fetch rows via [`crate::db`],
//! decrypt via [`crate::privacy`], then hand off to the pure FIFO engine in
//! [`crate::capital_gains`].

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};

use crate::models::SuccessResponse;
use crate::{auth::AuthUser, capital_gains, db, error::AppError, models, privacy, AppState};

pub async fn inner_get_capital_gains(
    pool: &sqlx::PgPool,
    user_id: &str,
    year: Option<i32>,
    crypto: &privacy::UserCrypto,
) -> Result<models::CapitalGainsReport, AppError> {
    use chrono::Datelike;

    // FIFO lots match on decrypted symbols — without the key there is nothing
    // to compute. Mirrors the locked-search behavior in inner_get_transactions.
    if !crypto.is_plaintext() && crypto.secret().is_none() {
        return Err(AppError::Forbidden(
            "Privacy encryption is locked — unlock to compute capital gains.".to_string(),
        ));
    }

    let today = chrono::Utc::now().date_naive();
    let tax_year = year.unwrap_or(today.year());

    // All investment transactions for this user, date-ordered (FIFO groups by
    // symbol in the lot queues, so only date order matters).
    let mut txns = db::list_capgains_txns(pool, user_id).await?;
    if !crypto.is_plaintext() {
        for t in &mut txns {
            privacy::open_into_opt(&mut t.symbol, &t.symbol_enc, crypto);
            privacy::open_into_opt(&mut t.security_name, &t.security_name_enc, crypto);
        }
    }

    // Current holdings for market-value lookup (quantity only populated after re-sync)
    let holdings_rows = db::list_holdings_for_capgains(pool, user_id).await?;
    let holdings_map = capital_gains::build_holdings_map(holdings_rows, crypto);

    let overrides = db::list_cost_basis_overrides(pool, user_id).await?;
    let override_map: HashMap<String, (i64, bool)> = overrides
        .into_iter()
        .map(|o| (o.investment_transaction_id, (o.cost_basis_cents, o.is_long_term)))
        .collect();

    Ok(capital_gains::compute_report(&txns, &holdings_map, &override_map, tax_year, today))
}

pub async fn get_capital_gains(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<models::CapitalGainsQuery>,
) -> Result<Json<SuccessResponse<models::CapitalGainsReport>>, AppError> {
    auth.require_scope("read")?;
    let crypto = privacy::user_crypto(&state, &auth).await?;
    let data = inner_get_capital_gains(&state.pool, &auth.user_id, params.year, &crypto).await?;
    Ok(Json(SuccessResponse { status: "success".to_string(), data }))
}

pub async fn set_cost_basis(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(txn_id): Path<String>,
    Json(req): Json<models::SetCostBasisRequest>,
) -> Result<Json<SuccessResponse<()>>, AppError> {
    auth.require_scope("write")?;
    if !db::investment_txn_owned(&state.pool, &txn_id, &auth.user_id).await? {
        return Err(AppError::NotFound("Investment transaction not found".to_string()));
    }
    db::set_cost_basis(&state.pool, &auth.user_id, &txn_id, req.cost_basis_cents, req.is_long_term).await?;
    Ok(Json(SuccessResponse { status: "success".to_string(), data: () }))
}

pub async fn delete_cost_basis(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(txn_id): Path<String>,
) -> Result<Json<SuccessResponse<()>>, AppError> {
    auth.require_scope("write")?;
    db::delete_cost_basis(&state.pool, &auth.user_id, &txn_id).await?;
    Ok(Json(SuccessResponse { status: "success".to_string(), data: () }))
}
