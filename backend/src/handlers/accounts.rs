//! Account handlers: listing, renaming/retyping, and portfolio aggregation.

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};

use crate::models::{Account, AggregateData, AggregateRequest, SuccessResponse, TimelinePoint};
use crate::{auth::AuthUser, db, error::AppError, models, privacy, AppState};

pub async fn get_accounts(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<SuccessResponse<Vec<Account>>>, AppError> {
    auth.require_scope("read")?;
    let crypto = privacy::user_crypto(&state, &auth).await?;
    let mut accounts = db::get_accounts_with_trend(&state.pool, &auth.user_id).await?;
    privacy::reveal_all(&mut accounts, &crypto);
    Ok(Json(SuccessResponse { status: "success".to_string(), data: accounts }))
}

pub async fn update_account(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<models::UpdateAccountRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    auth.require_scope("write")?;
    let pool = &state.pool;

    if !db::account_owned(pool, &id, &auth.user_id).await? {
        return Err(AppError::NotFound("Account not found".to_string()));
    }

    if let Some(new_account_type) = req.account_type {
        if let Some((current_balance, current_type)) = db::get_account_balance_and_type(pool, &id).await? {
            if current_type != new_account_type {
                db::set_account_balance_and_type(pool, -current_balance, new_account_type, &id).await?;
                db::negate_account_snapshots(pool, &id).await?;
                db::negate_account_holdings(pool, &id).await?;
            }
        }
    }

    if let Some(custom_name) = &req.custom_name {
        let crypto = privacy::user_crypto(&state, &auth).await?;
        let (plain, enc) = privacy::seal_field(&crypto, Some(custom_name.clone()))?;
        db::set_account_custom_name(pool, plain, enc, &id).await?;
    } else if req.custom_name.is_none() {
        db::clear_account_custom_name(pool, &id).await?;
    }

    Ok(Json(serde_json::json!({ "status": "success" })))
}

pub async fn aggregate_portfolio(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<AggregateRequest>,
) -> Result<Json<SuccessResponse<AggregateData>>, AppError> {
    auth.require_scope("read")?;
    if req.account_ids.len() > 100 {
        return Err(AppError::BadRequest("Too many account IDs (max 100)".to_string()));
    }
    use chrono::{Datelike, Duration, NaiveDate};
    let pool = &state.pool;

    let start_date = NaiveDate::parse_from_str(&req.time_range.start_date, "%Y-%m-%d")
        .map_err(|e| AppError::BadRequest(format!("Invalid start date: {}", e)))?;
    let end_date = NaiveDate::parse_from_str(&req.time_range.end_date, "%Y-%m-%d")
        .map_err(|e| AppError::BadRequest(format!("Invalid end date: {}", e)))?;

    let mut account_histories: HashMap<String, Vec<TimelinePoint>> = HashMap::new();
    let mut history_map: HashMap<String, i64> = HashMap::new();
    let mut snapshot_counts: HashMap<String, i64> = HashMap::new();

    for account_id in &req.account_ids {
        // Verify ownership
        if !db::account_owned(pool, account_id, &auth.user_id).await? { continue; }

        // Fetch snapshots in range. Also look one snapshot outside the range
        // so we can backward-fill accounts linked after range start.
        let snapshots = db::get_account_snapshots_through(pool, account_id, end_date).await?;

        let in_range_count = snapshots.iter().filter(|s| s.snapshot_date >= start_date).count();
        snapshot_counts.insert(account_id.clone(), in_range_count as i64);

        // Sorted list for binary-search lookups: find value at or before a date
        let sorted: Vec<(NaiveDate, i64)> = snapshots.iter()
            .map(|s| (s.snapshot_date, s.balance_cents))
            .collect();

        let value_at = |d: NaiveDate| -> i64 {
            if sorted.is_empty() { return 0; }
            let pos = sorted.partition_point(|(sd, _)| *sd <= d);
            if pos > 0 {
                sorted[pos - 1].1   // carry forward from last known snapshot
            } else {
                0                   // before first snapshot — no data, don't fabricate history
            }
        };

        // Build output dates: monthly = 1st of each month, always include end_date
        let mut dates: Vec<NaiveDate> = Vec::new();
        if req.time_range.interval == "daily" {
            let mut d = start_date;
            while d <= end_date { dates.push(d); d += Duration::days(1); }
        } else {
            // First day of each month in range
            let mut d = NaiveDate::from_ymd_opt(start_date.year(), start_date.month(), 1).unwrap();
            while d <= end_date {
                if d >= start_date { dates.push(d); }
                d = if d.month() == 12 {
                    NaiveDate::from_ymd_opt(d.year() + 1, 1, 1).unwrap()
                } else {
                    NaiveDate::from_ymd_opt(d.year(), d.month() + 1, 1).unwrap()
                };
            }
            // Always include end_date so today's snapshot is always visible
            if dates.last() != Some(&end_date) { dates.push(end_date); }
        }

        let mut current_points = Vec::new();
        for d in &dates {
            let val = value_at(*d);
            let date_str = d.format("%Y-%m-%d").to_string();
            current_points.push(TimelinePoint { date: date_str.clone(), value: val });
            *history_map.entry(date_str).or_insert(0) += val;
        }
        account_histories.insert(account_id.clone(), current_points);
    }

    let mut combined_history: Vec<TimelinePoint> = history_map.into_iter()
        .map(|(date, value)| TimelinePoint { date, value })
        .collect();
    combined_history.sort_by(|a, b| a.date.cmp(&b.date));

    let crypto = privacy::user_crypto(&state, &auth).await?;
    let mut holdings_breakdown = HashMap::new();
    for account_id in &req.account_ids {
        let mut holdings = db::get_account_holdings(pool, account_id, &auth.user_id).await?;
        privacy::reveal_all(&mut holdings, &crypto);
        holdings_breakdown.insert(account_id.clone(), holdings);
    }

    Ok(Json(SuccessResponse {
        status: "success".to_string(),
        data: AggregateData { combined_history, account_histories, holdings_breakdown, snapshot_counts },
    }))
}
