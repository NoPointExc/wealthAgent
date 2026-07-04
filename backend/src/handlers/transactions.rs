//! Transaction handlers: listing, single and bulk edits, investment
//! transactions, and saved searches.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};

use crate::models::{CreateSavedSearchRequest, SavedSearch, SuccessResponse, TransactionPage};
use crate::{auth::AuthUser, db, error::AppError, models, privacy, AppState};

pub async fn get_transactions(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<models::TransactionQuery>,
) -> Result<Json<SuccessResponse<TransactionPage>>, AppError> {
    auth.require_scope("read")?;
    let crypto = privacy::user_crypto(&state, &auth).await?;
    let data = db::inner_get_transactions(&state.pool, &auth.user_id, &params, &crypto).await?;
    Ok(Json(SuccessResponse { status: "success".to_string(), data }))
}

pub async fn get_investment_transactions(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<models::TransactionQuery>,
) -> Result<Json<SuccessResponse<models::InvestmentTransactionPage>>, AppError> {
    auth.require_scope("read")?;
    let pool = &state.pool;
    let limit = params.limit.unwrap_or(200).min(1000);
    let offset = params.offset.unwrap_or(0);

    let total = db::count_investment_transactions(pool, &auth.user_id).await?;
    let mut items = db::list_investment_transactions(pool, &auth.user_id, limit, offset).await?;

    let crypto = privacy::user_crypto(&state, &auth).await?;
    privacy::reveal_all(&mut items, &crypto);

    Ok(Json(SuccessResponse {
        status: "success".to_string(),
        data: models::InvestmentTransactionPage { items, total, offset },
    }))
}

pub async fn update_transaction(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    auth.require_scope("write")?;
    let pool = &state.pool;

    if !db::transaction_owned(pool, &id, &auth.user_id).await? {
        return Err(AppError::NotFound("Transaction not found".to_string()));
    }

    if body.get("note").is_some() {
        let note = body["note"].as_str().map(|s| s.to_string());
        let crypto = privacy::user_crypto(&state, &auth).await?;
        let (plain, enc) = privacy::seal_field(&crypto, note)?;
        db::set_transaction_note(pool, plain, enc, &id).await?;
    }
    if let Some(tags_val) = body.get("tags") {
        let tags_str = tags_val.as_array().and_then(|arr| {
            let tags: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
            if tags.is_empty() { None } else { Some(tags.join(",")) }
        });
        db::set_transaction_tags(pool, tags_str, &id).await?;
    }

    Ok(Json(serde_json::json!({ "status": "success" })))
}

pub async fn inner_bulk_update(
    pool: &sqlx::PgPool,
    user_id: &str,
    req: &models::BulkUpdateRequest,
    crypto: &privacy::UserCrypto,
) -> Result<u64, AppError> {
    let mut total_affected: u64 = 0;
    for id in &req.transaction_ids {
        match req.action {
            models::BulkAction::AddTag | models::BulkAction::RemoveTag => {
                let target_tag = req.value.as_deref().unwrap_or("").trim().to_string();
                if target_tag.is_empty() { continue; }

                let current_tags = db::get_txn_tags_owned(pool, id, user_id).await?;

                let mut tags_vec: Vec<String> = current_tags
                    .map(|t| t.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
                    .unwrap_or_default();

                if req.action == models::BulkAction::AddTag {
                    if !tags_vec.contains(&target_tag) { tags_vec.push(target_tag); }
                } else { tags_vec.retain(|t| t != &target_tag); }

                let new_tags = if tags_vec.is_empty() { None } else { Some(tags_vec.join(",")) };
                total_affected += db::update_txn_tags_owned(pool, new_tags, id, user_id).await?;
            }
            models::BulkAction::SetNote => {
                let (plain, enc) = privacy::seal_field(crypto, req.value.clone())?;
                total_affected += db::update_txn_note_owned(pool, &plain, enc, id, user_id).await?;
            }
            models::BulkAction::ClearNote => {
                total_affected += db::clear_txn_note_owned(pool, id, user_id).await?;
            }
        }
    }
    Ok(total_affected)
}

pub async fn bulk_update_transactions(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<models::BulkUpdateRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    auth.require_scope("write")?;
    if req.transaction_ids.len() > 500 {
        return Err(AppError::BadRequest("Too many transaction IDs (max 500)".to_string()));
    }
    if req.transaction_ids.len() > 50 {
        tracing::warn!(user_id = %auth.user_id, count = req.transaction_ids.len(), "Large bulk_update");
    }
    let crypto = privacy::user_crypto(&state, &auth).await?;
    let updated = inner_bulk_update(&state.pool, &auth.user_id, &req, &crypto).await?;
    Ok(Json(serde_json::json!({ "status": "success", "updated": updated })))
}

pub async fn get_saved_searches(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<SuccessResponse<Vec<SavedSearch>>>, AppError> {
    auth.require_scope("read")?;
    let searches = db::list_saved_searches(&state.pool, &auth.user_id).await?;
    Ok(Json(SuccessResponse { status: "success".to_string(), data: searches }))
}

pub async fn create_saved_search(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<CreateSavedSearchRequest>,
) -> Result<Json<SuccessResponse<SavedSearch>>, AppError> {
    auth.require_scope("write")?;
    let saved = db::create_saved_search(&state.pool, &req, &auth.user_id).await?;
    Ok(Json(SuccessResponse { status: "success".to_string(), data: saved }))
}

pub async fn delete_saved_search(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<i32>,
) -> Result<Json<serde_json::Value>, AppError> {
    auth.require_scope("write")?;
    db::delete_saved_search(&state.pool, id, &auth.user_id).await?;
    Ok(Json(serde_json::json!({ "status": "success" })))
}
