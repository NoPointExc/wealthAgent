use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    auth::{AuthUser, McpAuthUser},
    db, encryption,
    inner_bulk_update, inner_get_capital_gains, inner_get_transactions,
    models::{BulkAction, BulkUpdateRequest, TransactionQuery},
    privacy, AppState,
};

// ── JSON-RPC types ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct McpRequest {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Serialize)]
struct McpResponse {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

#[derive(Serialize)]
struct RpcError {
    code: i32,
    message: String,
}

fn ok(id: Option<Value>, result: Value) -> McpResponse {
    McpResponse { jsonrpc: "2.0", id, result: Some(result), error: None }
}

fn rpc_err(id: Option<Value>, code: i32, msg: &str) -> McpResponse {
    McpResponse { jsonrpc: "2.0", id, result: None, error: Some(RpcError { code, message: msg.to_string() }) }
}

fn tool_text(id: Option<Value>, text: String) -> McpResponse {
    ok(id, json!({ "content": [{ "type": "text", "text": text }] }))
}

fn tool_json(id: Option<Value>, v: &impl Serialize) -> McpResponse {
    let text = serde_json::to_string_pretty(v)
        .unwrap_or_else(|e| format!("{{\"error\":\"serialize:{e}\"}}"));
    tool_text(id, text)
}

fn tool_err(id: Option<Value>, msg: impl std::fmt::Display) -> McpResponse {
    tool_text(id, format!("{{\"error\":\"{msg}\"}}"))
}

// ── Server instructions ─────────────────────────────────────────────────────
// Returned in the `initialize` result. MCP-compliant clients (Claude Code,
// Claude Desktop, …) inject this into the model's context automatically on
// connect — so the agent self-orients without the user pasting a bootstrap
// prompt into every conversation.
const SERVER_INSTRUCTIONS: &str = "\
WealthAgent gives you read/write access to THIS user's real financial data \
(bank + investment accounts, transactions, capital gains). Treat it as \
authoritative and private.

Data conventions — follow exactly:
- All money is integer CENTS (e.g. 123456 = $1,234.56). Never assume dollars.
- Dates are 'YYYY-MM-DD' strings.
- Transaction `tags` are a string array; `category` is Plaid's taxonomy.
- Capital gains: `tax_year` is top-level; amounts that need a user-supplied \
cost basis appear in `unknown_basis_sales` until resolved, then move into \
`realized_lots` with source 'user_input'.

How to work:
1. Start with wealth_whoami to confirm the account and your token scopes.
2. Use the read tools (wealth_accounts_list, wealth_tx_list, wealth_gains) to \
gather facts before answering. Prefer server-side filters over fetching all rows.
3. Write tools (wealth_tx_tag, wealth_tx_note, wealth_accounts_rename, \
wealth_set_cost_basis, wealth_delete_cost_basis) MODIFY the user's data — \
confirm with the user before any bulk change (>50 rows) or any cost-basis edit.
4. wealth_sync refreshes from the bank; it is slow and rate-limited — call it \
at most once and never in a loop.

Be precise with numbers, cite the accounts/dates you used, and never invent \
figures you did not read from a tool.";

// ── Main handler ──────────────────────────────────────────────────────────────

pub async fn mcp_handler(
    State(state): State<Arc<AppState>>,
    McpAuthUser(auth): McpAuthUser,
    Json(req): Json<McpRequest>,
) -> Response {
    match req.method.as_str() {
        "initialize" => {
            let mut headers = HeaderMap::new();
            if let Ok(v) = HeaderValue::from_str(&Uuid::new_v4().to_string()) {
                headers.insert("Mcp-Session-Id", v);
            }
            let body = Json(ok(req.id, json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "WealthAgent MCP", "version": "1.0.0" },
                "instructions": SERVER_INSTRUCTIONS
            })));
            (StatusCode::OK, headers, body).into_response()
        }
        "notifications/initialized" | "ping" => StatusCode::NO_CONTENT.into_response(),
        "tools/list" => Json(tools_list(req.id)).into_response(),
        "tools/call" => Json(dispatch(&state, &auth, req.id, req.params).await).into_response(),
        _ => Json(rpc_err(req.id, -32601, "Method not found")).into_response(),
    }
}

// ── Tool list ─────────────────────────────────────────────────────────────────

fn tools_list(id: Option<Value>) -> McpResponse {
    ok(id, json!({ "tools": [
        {
            "name": "wealth_whoami",
            "description": "Show who the current API token belongs to, including scopes.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "wealth_accounts_list",
            "description": "List all connected bank and brokerage accounts with current balances. Balances in cents. account_type is 'asset' or 'liability'.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "wealth_tx_list",
            "description": "List transactions. Amounts in cents; negative = expense. Returns up to 1000 rows per page. Dates are YYYY-MM-DD.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "since":      { "type": "string", "description": "Earliest date inclusive, YYYY-MM-DD" },
                    "until":      { "type": "string", "description": "Latest date inclusive, YYYY-MM-DD" },
                    "account_id": { "type": "string", "description": "Filter to one account by its id" },
                    "category":   { "type": "string", "description": "Plaid primary category, e.g. FOOD_AND_DRINK" },
                    "tag":        { "type": "string", "description": "Only transactions that have this tag" },
                    "search":     { "type": "string", "description": "Substring match on merchant, description, or note" },
                    "pending":    { "type": "boolean", "description": "true=pending only; false=cleared only; omit=both" },
                    "limit":      { "type": "integer", "description": "Max rows (default 200, max 1000)" },
                    "offset":     { "type": "integer", "description": "Pagination offset (default 0)" }
                }
            }
        },
        {
            "name": "wealth_gains",
            "description": "Capital gains report for a tax year. All monetary values in cents. Includes realized lots (FIFO), unknown-basis sales, unrealized positions, and summary totals.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "year": { "type": "integer", "description": "Tax year e.g. 2025. Omit for current year." }
                }
            }
        },
        {
            "name": "wealth_tx_tag",
            "description": "Add or remove a tag on transactions. THIS WRITES DATA — confirm with user before affecting > 50 transactions. op: add | remove. value required.",
            "inputSchema": {
                "type": "object",
                "required": ["transaction_ids", "op"],
                "properties": {
                    "transaction_ids": { "type": "array", "items": { "type": "string" }, "description": "1-500 transaction IDs" },
                    "op":    { "type": "string", "enum": ["add", "remove"] },
                    "value": { "type": "string", "description": "Tag name (required)" }
                }
            }
        },
        {
            "name": "wealth_tx_note",
            "description": "Set or clear a note on transactions. THIS WRITES DATA. Provide value to set; omit to clear.",
            "inputSchema": {
                "type": "object",
                "required": ["transaction_ids"],
                "properties": {
                    "transaction_ids": { "type": "array", "items": { "type": "string" }, "description": "1-500 transaction IDs" },
                    "value": { "type": "string", "description": "Note text; omit or null to clear" }
                }
            }
        },
        {
            "name": "wealth_accounts_rename",
            "description": "Set a custom display name on an account. Pass null for custom_name to revert to the Plaid-provided name.",
            "inputSchema": {
                "type": "object",
                "required": ["account_id"],
                "properties": {
                    "account_id":   { "type": "string" },
                    "custom_name":  { "type": "string", "description": "New name, or null to clear" }
                }
            }
        },
        {
            "name": "wealth_sync",
            "description": "Trigger a Plaid data sync. Returns immediately; the refresh runs in the background and usually completes within a couple of minutes. Only run when user explicitly requests a refresh.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "wealth_set_cost_basis",
            "description": "Set the cost basis for an unknown-basis sale. Use txn_id from wealth_gains unknown_basis_sales[].txn_id. cost_basis_cents is total cost in cents. THIS WRITES DATA.",
            "inputSchema": {
                "type": "object",
                "required": ["txn_id", "cost_basis_cents", "is_long_term"],
                "properties": {
                    "txn_id":           { "type": "string", "description": "investment_transaction id from unknown_basis_sales" },
                    "cost_basis_cents": { "type": "integer", "description": "Total cost basis in cents (positive)" },
                    "is_long_term":     { "type": "boolean", "description": "true if held > 1 year" }
                }
            }
        },
        {
            "name": "wealth_delete_cost_basis",
            "description": "Remove a previously set cost basis override, reverting the sale to unknown basis.",
            "inputSchema": {
                "type": "object",
                "required": ["txn_id"],
                "properties": {
                    "txn_id": { "type": "string", "description": "investment_transaction id" }
                }
            }
        }
    ]}))
}

// ── Tool dispatcher ───────────────────────────────────────────────────────────

async fn dispatch(
    state: &Arc<AppState>,
    auth: &AuthUser,
    id: Option<Value>,
    params: Option<Value>,
) -> McpResponse {
    let p = params.unwrap_or(Value::Object(Default::default()));
    let name = match p.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return rpc_err(id, -32602, "Missing tool name"),
    };
    let args = p.get("arguments").cloned().unwrap_or(json!({}));

    // Scope gate: the token's granted scopes bound what each tool may do.
    let required_scope = match name {
        "wealth_whoami" => None,
        "wealth_accounts_list" | "wealth_tx_list" | "wealth_gains" => Some("read"),
        "wealth_tx_tag" | "wealth_tx_note" | "wealth_accounts_rename"
        | "wealth_set_cost_basis" | "wealth_delete_cost_basis" => Some("write"),
        "wealth_sync" => Some("sync"),
        _ => None,
    };
    if let Some(scope) = required_scope {
        if let Err(e) = auth.require_scope(scope) {
            return tool_err(id, e);
        }
    }

    match name {
        "wealth_whoami"           => tool_whoami(state, auth, id).await,
        "wealth_accounts_list"    => tool_accounts_list(state, auth, id).await,
        "wealth_tx_list"          => tool_tx_list(state, auth, id, &args).await,
        "wealth_gains"            => tool_gains(state, auth, id, &args).await,
        "wealth_tx_tag"           => tool_tx_tag(state, auth, id, &args).await,
        "wealth_tx_note"          => tool_tx_note(state, auth, id, &args).await,
        "wealth_accounts_rename"  => tool_accounts_rename(state, auth, id, &args).await,
        "wealth_sync"             => tool_sync(state, auth, id).await,
        "wealth_set_cost_basis"   => tool_set_cost_basis(state, auth, id, &args).await,
        "wealth_delete_cost_basis"=> tool_delete_cost_basis(state, auth, id, &args).await,
        other => rpc_err(id, -32602, &format!("Unknown tool: {other}")),
    }
}

// ── Tool implementations ──────────────────────────────────────────────────────

async fn tool_whoami(state: &Arc<AppState>, auth: &AuthUser, id: Option<Value>) -> McpResponse {
    match db::get_user(&state.pool, &auth.user_id).await {
        Ok(u) => tool_json(id, &json!({
            "user_id": u.id,
            "email": u.email,
            "name": u.name,
            "scopes": auth.scopes,
        })),
        Err(e) => tool_err(id, e),
    }
}

async fn tool_accounts_list(state: &Arc<AppState>, auth: &AuthUser, id: Option<Value>) -> McpResponse {
    let crypto = match privacy::user_crypto(state, auth).await {
        Ok(c) => c,
        Err(e) => return tool_err(id, e),
    };
    match db::get_accounts_with_trend(&state.pool, &auth.user_id).await {
        Ok(mut accounts) => {
            privacy::reveal_all(&mut accounts, &crypto);
            tool_json(id, &accounts)
        }
        Err(e) => tool_err(id, e),
    }
}

async fn tool_tx_list(state: &Arc<AppState>, auth: &AuthUser, id: Option<Value>, args: &Value) -> McpResponse {
    let params = TransactionQuery {
        since:      args["since"].as_str().map(str::to_string),
        until:      args["until"].as_str().map(str::to_string),
        account_id: args["account_id"].as_str().map(str::to_string),
        category:   args["category"].as_str().map(str::to_string),
        tag:        args["tag"].as_str().map(str::to_string),
        search:     args["search"].as_str().map(str::to_string),
        pending:    args["pending"].as_bool(),
        limit:      args["limit"].as_i64().map(|v| v as i64),
        offset:     args["offset"].as_i64().map(|v| v as i64),
    };
    let crypto = match privacy::user_crypto(state, auth).await {
        Ok(c) => c,
        Err(e) => return tool_err(id, e),
    };
    match inner_get_transactions(&state.pool, &auth.user_id, &params, &crypto).await {
        Ok(page) => {
            let mut v = serde_json::to_value(&page).unwrap_or(Value::Null);
            if let Some(items) = v["items"].as_array_mut() {
                for item in items.iter_mut() {
                    item["tags"] = match item["tags"].as_str() {
                        Some(s) => json!(s.split(',').map(str::trim).filter(|t| !t.is_empty()).collect::<Vec<_>>()),
                        None    => json!([]),
                    };
                }
            }
            let text = serde_json::to_string_pretty(&v)
                .unwrap_or_else(|e| format!("{{\"error\":\"serialize:{e}\"}}"));
            tool_text(id, text)
        }
        Err(e) => tool_err(id, e),
    }
}

async fn tool_gains(state: &Arc<AppState>, auth: &AuthUser, id: Option<Value>, args: &Value) -> McpResponse {
    let year = args["year"].as_i64().map(|y| y as i32);
    let crypto = match privacy::user_crypto(state, auth).await {
        Ok(c) => c,
        Err(e) => return tool_err(id, e),
    };
    match inner_get_capital_gains(&state.pool, &auth.user_id, year, &crypto).await {
        Ok(report) => tool_json(id, &report),
        Err(e) => tool_err(id, e),
    }
}

async fn tool_tx_tag(state: &Arc<AppState>, auth: &AuthUser, id: Option<Value>, args: &Value) -> McpResponse {
    let ids: Vec<String> = match args["transaction_ids"].as_array() {
        Some(arr) => arr.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
        None => return tool_err(id, "transaction_ids must be an array"),
    };
    if ids.is_empty() || ids.len() > 500 {
        return tool_err(id, "transaction_ids must contain 1-500 IDs");
    }

    let op = args["op"].as_str().unwrap_or("");
    let action = match op {
        "add"    => BulkAction::AddTag,
        "remove" => BulkAction::RemoveTag,
        other    => return tool_err(id, format!("unknown op '{other}'; use add | remove")),
    };
    let value = args["value"].as_str().map(str::to_string);
    let req = BulkUpdateRequest { transaction_ids: ids, action, value };
    let crypto = match privacy::user_crypto(state, auth).await {
        Ok(c) => c,
        Err(e) => return tool_err(id, e),
    };
    match inner_bulk_update(&state.pool, &auth.user_id, &req, &crypto).await {
        Ok(updated) => tool_text(id, json!({"status":"success","updated":updated}).to_string()),
        Err(e) => tool_err(id, e),
    }
}

async fn tool_tx_note(state: &Arc<AppState>, auth: &AuthUser, id: Option<Value>, args: &Value) -> McpResponse {
    let ids: Vec<String> = match args["transaction_ids"].as_array() {
        Some(arr) => arr.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
        None => return tool_err(id, "transaction_ids must be an array"),
    };
    if ids.is_empty() || ids.len() > 500 {
        return tool_err(id, "transaction_ids must contain 1-500 IDs");
    }
    let (action, value) = match args["value"].as_str() {
        Some(v) => (BulkAction::SetNote, Some(v.to_string())),
        None    => (BulkAction::ClearNote, None),
    };
    let req = BulkUpdateRequest { transaction_ids: ids, action, value };
    let crypto = match privacy::user_crypto(state, auth).await {
        Ok(c) => c,
        Err(e) => return tool_err(id, e),
    };
    match inner_bulk_update(&state.pool, &auth.user_id, &req, &crypto).await {
        Ok(updated) => tool_text(id, json!({"status":"success","updated":updated}).to_string()),
        Err(e) => tool_err(id, e),
    }
}

async fn tool_accounts_rename(state: &Arc<AppState>, auth: &AuthUser, id: Option<Value>, args: &Value) -> McpResponse {
    let account_id = match args["account_id"].as_str() {
        Some(s) => s,
        None => return tool_err(id, "account_id required"),
    };

    match db::account_owned(&state.pool, account_id, &auth.user_id).await {
        Ok(true) => {}
        _ => return tool_err(id, "Account not found"),
    }

    let custom_name = args["custom_name"].as_str();
    let result = if custom_name.is_none() || args["custom_name"].is_null() {
        crate::db::clear_account_custom_name(&state.pool, account_id).await
    } else {
        let crypto = match privacy::user_crypto(state, auth).await {
            Ok(c) => c,
            Err(e) => return tool_err(id, e),
        };
        match privacy::seal_field(&crypto, custom_name.map(str::to_string)) {
            Ok((plain, enc)) => crate::db::set_account_custom_name(&state.pool, plain, enc, account_id).await,
            Err(e) => return tool_err(id, e),
        }
    };

    match result {
        Ok(_) => tool_text(id, json!({"status":"success"}).to_string()),
        Err(e) => tool_err(id, e),
    }
}

async fn tool_sync(state: &Arc<AppState>, auth: &AuthUser, id: Option<Value>) -> McpResponse {
    if state.demo_mode {
        return tool_err(id, "Sync is disabled in the demo — data refreshes automatically.");
    }
    let items = match db::get_user_plaid_items(&state.pool, &auth.user_id).await {
        Ok(v) => v,
        Err(e) => return tool_err(id, e),
    };

    // Detached, like the HTTP sync endpoint: a large re-sync outlives the 30s
    // request timeout, and the MCP client shouldn't sit on a blocked call.
    let mut to_sync = Vec::with_capacity(items.len());
    for item in &items {
        let blob: &[u8] = match item.access_token_enc.as_deref() {
            Some(b) => b,
            None => return tool_err(id, "Item has no encrypted token"),
        };
        match encryption::decrypt_token(&state.encryption_key, blob) {
            Ok(t) => to_sync.push((item.id.clone(), t)),
            Err(e) => return tool_err(id, e),
        };
    }
    let count = to_sync.len();
    if count > 0 {
        crate::handlers::spawn_detached_sync(state, &auth.user_id, to_sync).await;
    }
    tool_text(id, json!({
        "status": "started",
        "items": count,
        "note": "Sync runs in the background and typically finishes within a couple of minutes; re-read data afterwards."
    }).to_string())
}

async fn tool_set_cost_basis(state: &Arc<AppState>, auth: &AuthUser, id: Option<Value>, args: &Value) -> McpResponse {
    let txn_id = match args["txn_id"].as_str() {
        Some(s) => s,
        None => return tool_err(id, "txn_id required"),
    };
    let cost_basis_cents = match args["cost_basis_cents"].as_i64() {
        Some(v) => v,
        None => return tool_err(id, "cost_basis_cents required (integer)"),
    };
    if cost_basis_cents < 0 {
        return tool_err(id, "cost_basis_cents must be >= 0");
    }
    let is_long_term = match args["is_long_term"].as_bool() {
        Some(v) => v,
        None => return tool_err(id, "is_long_term required (boolean)"),
    };

    match db::investment_txn_owned(&state.pool, txn_id, &auth.user_id).await {
        Ok(true) => {}
        _ => return tool_err(id, "txn_id not found for this user"),
    }

    match db::set_cost_basis(&state.pool, &auth.user_id, txn_id, cost_basis_cents, is_long_term).await {
        Ok(()) => tool_text(id, json!({"status":"success","txn_id":txn_id,"cost_basis_cents":cost_basis_cents,"is_long_term":is_long_term}).to_string()),
        Err(e) => tool_err(id, e),
    }
}

async fn tool_delete_cost_basis(state: &Arc<AppState>, auth: &AuthUser, id: Option<Value>, args: &Value) -> McpResponse {
    let txn_id = match args["txn_id"].as_str() {
        Some(s) => s,
        None => return tool_err(id, "txn_id required"),
    };

    match db::delete_cost_basis(&state.pool, &auth.user_id, txn_id).await {
        Ok(()) => tool_text(id, json!({"status":"success","txn_id":txn_id}).to_string()),
        Err(e) => tool_err(id, e),
    }
}
