use crate::client::{TxListParams, WealthClient};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use std::sync::Arc;

fn ok(v: impl serde::Serialize) -> String {
    serde_json::to_string_pretty(&v).unwrap_or_else(|e| format!("{{\"error\":\"serialize:{e}\"}}"))
}

fn err(msg: impl std::fmt::Display) -> String {
    format!("{{\"error\":\"{msg}\"}}")
}

#[derive(Clone)]
pub struct WealthMcpServer {
    client: Arc<WealthClient>,
}

impl WealthMcpServer {
    pub fn new(client: WealthClient) -> Self {
        Self { client: Arc::new(client) }
    }
}

// ── Parameter structs ─────────────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct TxListInput {
    #[schemars(description = "Earliest date inclusive, YYYY-MM-DD")]
    pub since: Option<String>,
    #[schemars(description = "Latest date inclusive, YYYY-MM-DD")]
    pub until: Option<String>,
    #[schemars(description = "Filter to one account by its id")]
    pub account_id: Option<String>,
    #[schemars(description = "Plaid primary category, e.g. FOOD_AND_DRINK")]
    pub category: Option<String>,
    #[schemars(description = "Only transactions that have this tag")]
    pub tag: Option<String>,
    #[schemars(description = "Substring match on merchant, description, or note")]
    pub search: Option<String>,
    #[schemars(description = "true = pending only; false = cleared only; omit = both")]
    pub pending: Option<bool>,
    #[schemars(description = "Max rows (default 200, max 1000)")]
    pub limit: Option<i32>,
    #[schemars(description = "Pagination offset (default 0)")]
    pub offset: Option<i32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct TxTagInput {
    #[schemars(description = "Transaction IDs to operate on (1–500)")]
    pub transaction_ids: Vec<String>,
    #[schemars(description = "Operation: add | remove | clear_all")]
    pub op: String,
    #[schemars(description = "Tag name — required for add/remove, ignored for clear_all")]
    pub value: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct TxNoteInput {
    #[schemars(description = "Transaction IDs to operate on (1–500)")]
    pub transaction_ids: Vec<String>,
    #[schemars(description = "Note text to set, or null/omit to clear")]
    pub value: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct AccountRenameInput {
    #[schemars(description = "Account id to rename")]
    pub account_id: String,
    #[schemars(description = "New display name (null to clear custom name)")]
    pub custom_name: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GainsInput {
    #[schemars(description = "Tax year (e.g. 2025). Omit for current year.")]
    pub year: Option<i32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EmptyInput {}

// ── Tool implementations ──────────────────────────────────────────────────────

#[tool_router(server_handler)]
impl WealthMcpServer {
    #[tool(description = "\
List the user's bank/credit card transactions. \
Amounts are in cents; negative = expense/outflow. \
Returns up to 1000 rows per page — use filters to narrow results. \
Dates are YYYY-MM-DD strings.")]
    async fn wealth_tx_list(
        &self,
        Parameters(p): Parameters<TxListInput>,
    ) -> String {
        let params = TxListParams {
            since: p.since,
            until: p.until,
            account_id: p.account_id,
            category: p.category,
            tag: p.tag,
            search: p.search,
            pending: p.pending,
            limit: p.limit,
            offset: p.offset,
        };
        match self.client.list_transactions(&params).await {
            Ok(page) => ok(&page),
            Err(e) => err(e),
        }
    }

    #[tool(description = "\
List all connected bank and brokerage accounts with current balances. \
Balances are in cents. account_type is 'asset' or 'liability'.")]
    async fn wealth_accounts_list(
        &self,
        Parameters(_): Parameters<EmptyInput>,
    ) -> String {
        match self.client.list_accounts().await {
            Ok(accounts) => ok(&accounts),
            Err(e) => err(e),
        }
    }

    #[tool(description = "\
Get the capital gains report for a given tax year (or current year if omitted). \
Includes realized lots (FIFO + manual), unknown-basis sales, unrealized positions, and summary totals. \
All monetary values are in cents.")]
    async fn wealth_gains(
        &self,
        Parameters(p): Parameters<GainsInput>,
    ) -> String {
        match self.client.get_capital_gains(p.year).await {
            Ok(report) => ok(&report),
            Err(e) => err(e),
        }
    }

    #[tool(description = "\
Show who the current API token belongs to, including scopes.")]
    async fn wealth_whoami(
        &self,
        Parameters(_): Parameters<EmptyInput>,
    ) -> String {
        match self.client.whoami().await {
            Ok(info) => ok(&info),
            Err(e) => err(e),
        }
    }

    #[tool(description = "\
Add, remove, or clear tags on one or more transactions. \
THIS WRITES DATA — confirm with the user before operations affecting > 50 transactions. \
op must be 'add', 'remove', or 'clear_all'. \
value is required for add/remove (the tag name). \
Accepts 1–500 transaction IDs.")]
    async fn wealth_tx_tag(
        &self,
        Parameters(p): Parameters<TxTagInput>,
    ) -> String {
        if p.transaction_ids.is_empty() || p.transaction_ids.len() > 500 {
            return err("transaction_ids must contain 1–500 IDs");
        }
        let (action, value): (&str, Option<&str>) = match p.op.as_str() {
            "add"       => ("add_tag",    p.value.as_deref()),
            "remove"    => ("remove_tag", p.value.as_deref()),
            "clear_all" => ("remove_tag", None), // backend handles clear differently
            other => return err(format!("unknown op '{}'; use add | remove | clear_all", other)),
        };

        // For clear_all, the backend doesn't have a single "clear tags" action,
        // so we'd need to do it per-txn. For now, map to a no-op clear approach.
        // The frontend uses 'remove_tag' iteratively; we document clear_all as "remove all tags".
        // A proper implementation would require a backend change. For now return a helpful message.
        if p.op == "clear_all" {
            return ok(serde_json::json!({
                "status": "note",
                "message": "clear_all is not yet supported in bulk. Use remove to remove specific tags."
            }));
        }

        match self.client.bulk_update_transactions(p.transaction_ids.clone(), action, value).await {
            Ok(_) => ok(serde_json::json!({
                "status": "success",
                "updated": p.transaction_ids.len()
            })),
            Err(e) => err(e),
        }
    }

    #[tool(description = "\
Set or clear a note on one or more transactions. \
THIS WRITES DATA. \
Provide value to set; omit or null to clear. \
Accepts 1–500 transaction IDs.")]
    async fn wealth_tx_note(
        &self,
        Parameters(p): Parameters<TxNoteInput>,
    ) -> String {
        if p.transaction_ids.is_empty() || p.transaction_ids.len() > 500 {
            return err("transaction_ids must contain 1–500 IDs");
        }
        let (action, value): (&str, Option<&str>) = match &p.value {
            Some(v) => ("set_note", Some(v.as_str())),
            None    => ("clear_note", None),
        };
        match self.client.bulk_update_transactions(p.transaction_ids.clone(), action, value).await {
            Ok(_) => ok(serde_json::json!({
                "status": "success",
                "updated": p.transaction_ids.len()
            })),
            Err(e) => err(e),
        }
    }

    #[tool(description = "\
Rename an account by setting a custom display name. \
Pass null for custom_name to revert to the Plaid-provided name.")]
    async fn wealth_accounts_rename(
        &self,
        Parameters(p): Parameters<AccountRenameInput>,
    ) -> String {
        match self.client.update_account(&p.account_id, p.custom_name.as_deref()).await {
            Ok(_) => ok(serde_json::json!({ "status": "success" })),
            Err(e) => err(e),
        }
    }

    #[tool(description = "\
Trigger a Plaid data sync to pull the latest transactions, holdings, and balances. \
Blocks until the sync completes (usually 10–30 seconds). \
Only run this when the user explicitly requests a refresh.")]
    async fn wealth_sync(
        &self,
        Parameters(_): Parameters<EmptyInput>,
    ) -> String {
        match self.client.sync().await {
            Ok(_) => ok(serde_json::json!({ "status": "success", "message": "Sync complete" })),
            Err(e) => err(e),
        }
    }
}

