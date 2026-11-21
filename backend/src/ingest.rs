//! Push-based ingest and history export for an external agent (Muse) that
//! pulls from its **own** Plaid connection and pushes the results over MCP.
//!
//! Payloads use Plaid's own response shapes and field names, so the agent can
//! forward `/accounts/get`, `/transactions/sync` (or `/transactions/get`),
//! `/investments/holdings/get` and `/investments/transactions/get` bodies
//! unchanged. Unknown fields are ignored.
//!
//! A second Plaid connection issues different account and transaction ids, so
//! every pushed id is resolved through `external_id_map` before writing:
//!
//! 1. an existing mapping for the pushed id;
//! 2. a row here with the very same id (same connection);
//! 3. accounts only: one unclaimed account with the same mask and type;
//!    transactions only: the pending row named by `pending_transaction_id`,
//!    then a fuzzy match (same account and amount, date within
//!    [`FUZZY_DAYS`], preferring the same description);
//! 4. otherwise a new row whose id is the pushed id.
//!
//! Every resolved id is recorded in the map, so an existing row is claimed by
//! at most one pushed id. Each call runs in one DB transaction; `dry_run`
//! rolls it back, so the report shows exactly what a real push would do.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{Datelike, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgConnection;

use crate::{
    error::AppError,
    models::{AccountType, InvestmentTransaction, TransactionQuery},
    plaid::normalize,
    privacy::{self, Sealed, UserCrypto},
    AppState,
};

/// Max rows written per call (also keeps a call well inside the 30 s request
/// timeout and the 1 MB body limit).
pub const MAX_ROWS: usize = 500;
/// Max entries listed per report section; counts are always complete.
const MAX_REPORTED: usize = 100;
/// Date window for fuzzy transaction matching (pending → posted can shift).
const FUZZY_DAYS: i32 = 3;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Account,
    Transaction,
    InvestmentTransaction,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Account => "account",
            Kind::Transaction => "transaction",
            Kind::InvestmentTransaction => "investment_transaction",
        }
    }

    pub fn parse(s: &str) -> Option<Kind> {
        match s {
            "account" => Some(Kind::Account),
            "transaction" => Some(Kind::Transaction),
            "investment_transaction" => Some(Kind::InvestmentTransaction),
            _ => None,
        }
    }

    fn owned_sql(self) -> &'static str {
        match self {
            Kind::Account => {
                "SELECT EXISTS(SELECT 1 FROM accounts a JOIN plaid_items pi ON a.plaid_item_id = pi.id
                 WHERE a.id = $1 AND pi.user_id = $2)"
            }
            Kind::Transaction => {
                "SELECT EXISTS(SELECT 1 FROM transactions t JOIN accounts a ON t.account_id = a.id
                 JOIN plaid_items pi ON a.plaid_item_id = pi.id WHERE t.id = $1 AND pi.user_id = $2)"
            }
            Kind::InvestmentTransaction => {
                "SELECT EXISTS(SELECT 1 FROM investment_transactions it JOIN accounts a ON it.account_id = a.id
                 JOIN plaid_items pi ON a.plaid_item_id = pi.id WHERE it.id = $1 AND pi.user_id = $2)"
            }
        }
    }

    fn exists_sql(self) -> &'static str {
        match self {
            Kind::Account => "SELECT EXISTS(SELECT 1 FROM accounts WHERE id = $1)",
            Kind::Transaction => "SELECT EXISTS(SELECT 1 FROM transactions WHERE id = $1)",
            Kind::InvestmentTransaction => "SELECT EXISTS(SELECT 1 FROM investment_transactions WHERE id = $1)",
        }
    }
}

// ── Report ────────────────────────────────────────────────────────────────────

#[derive(Default, Serialize)]
pub struct Report {
    pub dry_run: bool,
    pub received: usize,
    pub inserted: usize,
    /// Rows updated in place. Includes `matched`.
    pub updated: usize,
    /// Pushed ids newly linked to a pre-existing row (mask, pending or fuzzy match).
    pub matched: usize,
    pub removed: usize,
    pub skipped: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub accounts: Vec<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub matches: Vec<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skips: Vec<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

impl Report {
    fn skip(&mut self, id: &str, reason: impl Into<String>) {
        self.skipped += 1;
        if self.skips.len() < MAX_REPORTED {
            self.skips.push(json!({ "id": id, "reason": reason.into() }));
        }
    }

    fn matched(&mut self, external_id: &str, internal_id: &str, how: &str) {
        self.matched += 1;
        if self.matches.len() < MAX_REPORTED {
            self.matches.push(json!({ "external_id": external_id, "internal_id": internal_id, "how": how }));
        }
    }
}

fn bad(msg: impl Into<String>) -> AppError {
    AppError::BadRequest(msg.into())
}

fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

fn parse_payload<T: serde::de::DeserializeOwned>(args: &Value) -> Result<T, AppError> {
    serde_json::from_value(args.clone()).map_err(|e| bad(format!("invalid payload: {e}")))
}

// ── Shared context ────────────────────────────────────────────────────────────

struct Ctx<'a> {
    user_id: &'a str,
    pubkey: Option<[u8; 32]>,
}

impl Ctx<'_> {
    fn pk(&self) -> Option<&[u8; 32]> {
        self.pubkey.as_ref()
    }
}

async fn load_ctx<'a>(state: &Arc<AppState>, user_id: &'a str) -> Result<Ctx<'a>, AppError> {
    let pk: Option<Vec<u8>> = sqlx::query_scalar("SELECT public_key FROM user_privacy_keys WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(&state.pool)
        .await?;
    let pubkey = pk
        .map(|k| {
            k.as_slice()
                .try_into()
                .map_err(|_| AppError::InternalServerError("privacy: bad public key length".into()))
        })
        .transpose()?;
    Ok(Ctx { user_id, pubkey })
}

/// Commit (or roll back for a dry run / error) and write an ingest_log row.
async fn finish(
    state: &Arc<AppState>,
    user_id: &str,
    tool: &str,
    dry_run: bool,
    tx: sqlx::Transaction<'_, sqlx::Postgres>,
    result: Result<Report, AppError>,
) -> Result<Report, AppError> {
    let result = match result {
        Ok(mut report) => {
            report.dry_run = dry_run;
            if dry_run {
                tx.rollback().await?;
            } else {
                tx.commit().await?;
            }
            Ok(report)
        }
        Err(e) => {
            tx.rollback().await.ok();
            Err(e)
        }
    };
    let (ok, summary) = match &result {
        Ok(r) => (true, json!({
            "received": r.received, "inserted": r.inserted, "updated": r.updated,
            "matched": r.matched, "removed": r.removed, "skipped": r.skipped,
        })),
        Err(e) => (false, json!({ "error": e.to_string() })),
    };
    if let Err(e) = sqlx::query("INSERT INTO ingest_log (user_id, tool, dry_run, ok, summary) VALUES ($1, $2, $3, $4, $5::jsonb)")
        .bind(user_id).bind(tool).bind(dry_run).bind(ok).bind(summary.to_string())
        .execute(&state.pool)
        .await
    {
        tracing::warn!(user_id = %user_id, "ingest_log insert failed: {e}");
    }
    tracing::info!(user_id = %user_id, tool, dry_run, ok, summary = %summary, "ingest");
    result
}

// ── Id resolution ─────────────────────────────────────────────────────────────

enum Resolved {
    /// (internal id, whether a map row already exists for the pushed id)
    Found(String, bool),
    /// The id exists but belongs to another user.
    Foreign,
    /// The id is a row here that another pushed id has already claimed.
    Superseded,
    NotFound,
}

async fn owned(conn: &mut PgConnection, ctx: &Ctx<'_>, kind: Kind, id: &str) -> Result<bool, AppError> {
    Ok(sqlx::query_scalar(kind.owned_sql()).bind(id).bind(ctx.user_id).fetch_one(&mut *conn).await?)
}

async fn resolve(conn: &mut PgConnection, ctx: &Ctx<'_>, kind: Kind, ext: &str) -> Result<Resolved, AppError> {
    let mapped: Option<String> = sqlx::query_scalar(
        "SELECT internal_id FROM external_id_map WHERE user_id = $1 AND kind = $2 AND external_id = $3",
    )
    .bind(ctx.user_id).bind(kind.as_str()).bind(ext)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(internal) = mapped {
        if owned(conn, ctx, kind, &internal).await? {
            return Ok(Resolved::Found(internal, true));
        }
        // The mapped row is gone (e.g. removed); forget the stale mapping.
        unmap_external(conn, ctx, kind, ext).await?;
    }

    let claimed_by_other: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM external_id_map
         WHERE user_id = $1 AND kind = $2 AND internal_id = $3 AND external_id <> $3)",
    )
    .bind(ctx.user_id).bind(kind.as_str()).bind(ext)
    .fetch_one(&mut *conn)
    .await?;
    if claimed_by_other {
        return Ok(Resolved::Superseded);
    }

    let exists: bool = sqlx::query_scalar(kind.exists_sql()).bind(ext).fetch_one(&mut *conn).await?;
    if !exists {
        return Ok(Resolved::NotFound);
    }
    if owned(conn, ctx, kind, ext).await? {
        Ok(Resolved::Found(ext.to_string(), false))
    } else {
        Ok(Resolved::Foreign)
    }
}

async fn map_insert(
    conn: &mut PgConnection,
    ctx: &Ctx<'_>,
    kind: Kind,
    ext: &str,
    internal: &str,
    method: &str,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO external_id_map (user_id, kind, external_id, internal_id, match_method)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (user_id, kind, external_id) DO UPDATE
           SET internal_id = EXCLUDED.internal_id, match_method = EXCLUDED.match_method",
    )
    .bind(ctx.user_id).bind(kind.as_str()).bind(ext).bind(internal).bind(method)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

async fn unmap_external(conn: &mut PgConnection, ctx: &Ctx<'_>, kind: Kind, ext: &str) -> Result<(), AppError> {
    sqlx::query("DELETE FROM external_id_map WHERE user_id = $1 AND kind = $2 AND external_id = $3")
        .bind(ctx.user_id).bind(kind.as_str()).bind(ext)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Resolve a pushed account id that must already be known (for transactions
/// and holdings). Records a same-id mapping on first sight.
async fn known_account(conn: &mut PgConnection, ctx: &Ctx<'_>, ext: &str) -> Result<Option<String>, AppError> {
    match resolve(conn, ctx, Kind::Account, ext).await? {
        Resolved::Found(id, has_map) => {
            if !has_map {
                map_insert(conn, ctx, Kind::Account, ext, &id, "same_id").await?;
            }
            Ok(Some(id))
        }
        _ => Ok(None),
    }
}

const UNKNOWN_ACCOUNT: &str = "unknown account_id; push it with wealth_ingest_accounts first";

// ── Accounts ──────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct AccountsPayload {
    accounts: Vec<InAccount>,
    #[serde(default)]
    institution_name: Option<String>,
    #[serde(default)]
    as_of: Option<String>,
}

#[derive(Deserialize)]
struct InAccount {
    account_id: String,
    name: String,
    #[serde(default)]
    mask: Option<String>,
    #[serde(rename = "type")]
    plaid_type: String,
    balances: InBalances,
    #[serde(default)]
    institution_name: Option<String>,
}

#[derive(Deserialize)]
struct InBalances {
    #[serde(default)]
    current: Option<f64>,
    #[serde(default)]
    available: Option<f64>,
}

enum AutoMatch {
    One(String),
    Many(Vec<String>),
    None,
}

async fn automatch_account(
    conn: &mut PgConnection,
    ctx: &Ctx<'_>,
    acc: &InAccount,
    institution: &str,
    account_type: AccountType,
) -> Result<AutoMatch, AppError> {
    let Some(mask) = acc.mask.as_deref().filter(|m| !m.is_empty()) else {
        return Ok(AutoMatch::None);
    };
    let mut cands: Vec<(String, String)> = sqlx::query_as(
        "SELECT a.id, a.name FROM accounts a JOIN plaid_items pi ON a.plaid_item_id = pi.id
         WHERE pi.user_id = $1 AND a.account_type = $2
           AND right(a.name, length($3) + 3) = ' - ' || $3
           AND NOT EXISTS (SELECT 1 FROM external_id_map m
                           WHERE m.user_id = $1 AND m.kind = 'account' AND m.internal_id = a.id)
         ORDER BY a.id",
    )
    .bind(ctx.user_id).bind(account_type).bind(mask)
    .fetch_all(&mut *conn)
    .await?;

    // Narrow ambiguous candidates by institution, then by account name.
    for needle in [institution.to_lowercase(), acc.name.to_lowercase()] {
        if cands.len() <= 1 || needle.is_empty() {
            break;
        }
        let narrowed: Vec<_> = cands.iter().filter(|(_, n)| n.to_lowercase().contains(&needle)).cloned().collect();
        if !narrowed.is_empty() {
            cands = narrowed;
        }
    }
    Ok(match cands.len() {
        0 => AutoMatch::None,
        1 => AutoMatch::One(cands.remove(0).0),
        _ => AutoMatch::Many(cands.into_iter().map(|(id, name)| format!("{id} ({name})")).collect()),
    })
}

async fn ensure_muse_item(conn: &mut PgConnection, ctx: &Ctx<'_>) -> Result<String, AppError> {
    let item_id = format!("muse_{}", ctx.user_id);
    sqlx::query(
        "INSERT INTO plaid_items (id, user_id, source, access_token_enc) VALUES ($1, $2, 'muse', NULL)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(&item_id).bind(ctx.user_id)
    .execute(&mut *conn)
    .await?;
    let owner: Option<String> = sqlx::query_scalar("SELECT user_id FROM plaid_items WHERE id = $1")
        .bind(&item_id)
        .fetch_one(&mut *conn)
        .await?;
    if owner.as_deref() != Some(ctx.user_id) {
        return Err(AppError::InternalServerError("muse item owner mismatch".into()));
    }
    Ok(item_id)
}

pub async fn ingest_accounts(state: &Arc<AppState>, user_id: &str, args: &Value) -> Result<Report, AppError> {
    let payload: AccountsPayload = parse_payload(args)?;
    let dry_run = args["dry_run"].as_bool().unwrap_or(false);
    if payload.accounts.is_empty() || payload.accounts.len() > MAX_ROWS {
        return Err(bad(format!("accounts must contain 1-{MAX_ROWS} entries")));
    }
    let ctx = load_ctx(state, user_id).await?;
    let mut tx = state.pool.begin().await?;
    let result = ingest_accounts_inner(&mut tx, &ctx, &payload).await;
    finish(state, user_id, "wealth_ingest_accounts", dry_run, tx, result).await
}

async fn ingest_accounts_inner(conn: &mut PgConnection, ctx: &Ctx<'_>, p: &AccountsPayload) -> Result<Report, AppError> {
    let today: NaiveDate = sqlx::query_scalar("SELECT (now() AT TIME ZONE 'America/New_York')::date")
        .fetch_one(&mut *conn)
        .await?;
    let as_of = match p.as_of.as_deref() {
        Some(s) => parse_date(s).ok_or_else(|| bad("as_of must be YYYY-MM-DD"))?,
        None => today,
    };
    if as_of > today.succ_opt().unwrap_or(today) {
        return Err(bad("as_of is in the future"));
    }

    let mut r = Report { received: p.accounts.len(), ..Default::default() };
    let mut seen = HashSet::new();
    for acc in &p.accounts {
        let ext = acc.account_id.as_str();
        if !seen.insert(ext) {
            r.skip(ext, "duplicate account_id in this call");
            continue;
        }
        let Some(current) = acc.balances.current.or(acc.balances.available) else {
            r.skip(ext, "balances.current and balances.available are both null");
            continue;
        };
        let institution = acc
            .institution_name
            .as_deref()
            .or(p.institution_name.as_deref())
            .filter(|s| !s.is_empty())
            .unwrap_or("Bank");

        let (internal, how, created) = match resolve(conn, ctx, Kind::Account, ext).await? {
            Resolved::Found(id, has_map) => {
                if !has_map {
                    map_insert(conn, ctx, Kind::Account, ext, &id, "same_id").await?;
                }
                (id, "existing", false)
            }
            Resolved::Foreign => {
                r.skip(ext, "account_id belongs to another user");
                continue;
            }
            Resolved::Superseded => {
                r.skip(ext, "account_id already reused for another pushed account; see wealth_ingest_map");
                continue;
            }
            Resolved::NotFound => {
                let inferred = normalize::classify_account(&acc.plaid_type, &acc.name, None);
                match automatch_account(conn, ctx, acc, institution, inferred).await? {
                    AutoMatch::One(id) => {
                        map_insert(conn, ctx, Kind::Account, ext, &id, "mask").await?;
                        r.matched(ext, &id, "mask");
                        (id, "matched_by_mask", false)
                    }
                    AutoMatch::Many(c) => {
                        r.skip(ext, format!(
                            "ambiguous: several existing accounts end in this mask: {}. Link one with wealth_ingest_map.",
                            c.join(", ")
                        ));
                        continue;
                    }
                    AutoMatch::None => {
                        let item_id = ensure_muse_item(conn, ctx).await?;
                        let display = normalize::display_name(institution, &acc.name, acc.mask.as_deref());
                        let (name_plain, name_enc) = privacy::seal_required(ctx.pk(), &display)?;
                        let balance = normalize::signed_balance_cents(current, inferred);
                        sqlx::query(
                            "INSERT INTO accounts (id, name, name_enc, balance, trend_pct, plaid_item_id, account_type)
                             VALUES ($1, $2, $3, $4, 0, $5, $6)",
                        )
                        .bind(ext).bind(&name_plain).bind(name_enc).bind(balance).bind(&item_id).bind(inferred)
                        .execute(&mut *conn)
                        .await?;
                        map_insert(conn, ctx, Kind::Account, ext, ext, "created").await?;
                        (ext.to_string(), "created", true)
                    }
                }
            }
        };

        let (balance_before, existing_type): (i64, AccountType) =
            sqlx::query_as("SELECT balance, account_type FROM accounts WHERE id = $1")
                .bind(&internal)
                .fetch_one(&mut *conn)
                .await?;
        let account_type = normalize::classify_account(&acc.plaid_type, &acc.name, Some(existing_type));
        let balance = normalize::signed_balance_cents(current, account_type);
        let is_investment = normalize::is_investment(&acc.plaid_type);

        // A backfilled (older) as_of only adds a snapshot; it never rewinds
        // the account's current balance.
        let latest: Option<NaiveDate> =
            sqlx::query_scalar("SELECT max(snapshot_date) FROM account_snapshots WHERE account_id = $1")
                .bind(&internal)
                .fetch_one(&mut *conn)
                .await?;
        let is_latest = latest.map_or(true, |d| as_of >= d);
        if is_latest {
            sqlx::query("UPDATE accounts SET balance = $1 WHERE id = $2")
                .bind(balance).bind(&internal)
                .execute(&mut *conn)
                .await?;
            if !is_investment {
                let (symbol, holding_name, security_id) =
                    normalize::cash_holding(&acc.plaid_type, &internal, account_type);
                sqlx::query(
                    "INSERT INTO holdings (symbol, account_id, name, asset_value, return_str, performance_type, security_id)
                     VALUES ($1, $2, $3, $4, '0%', 'neu', $5)
                     ON CONFLICT (security_id, account_id) DO UPDATE SET asset_value = EXCLUDED.asset_value",
                )
                .bind(symbol).bind(&internal).bind(holding_name).bind(balance).bind(&security_id)
                .execute(&mut *conn)
                .await?;
            }
        }

        // Markets are closed on weekends; a snapshot would repeat Friday's close.
        let weekend = matches!(as_of.weekday(), Weekday::Sat | Weekday::Sun);
        let snapshot_date = if is_investment && weekend {
            None
        } else {
            sqlx::query(
                "INSERT INTO account_snapshots (account_id, snapshot_date, balance_cents) VALUES ($1, $2, $3)
                 ON CONFLICT (account_id, snapshot_date) DO UPDATE SET balance_cents = EXCLUDED.balance_cents",
            )
            .bind(&internal).bind(as_of).bind(balance)
            .execute(&mut *conn)
            .await?;
            Some(as_of)
        };

        if created {
            r.inserted += 1;
        } else {
            r.updated += 1;
        }
        r.accounts.push(json!({
            "external_id": ext,
            "internal_id": internal,
            "how": how,
            "account_type": account_type,
            "balance_before": if created { Value::Null } else { json!(balance_before) },
            "balance_after": if is_latest { balance } else { balance_before },
            "balance_updated": is_latest,
            "snapshot_date": snapshot_date.map(|d| d.to_string()),
        }));
    }
    Ok(r)
}

// ── Accounts prelude ──────────────────────────────────────────────────────────
//
// Each call is its own DB transaction, so a dry-run accounts push leaves no
// account links behind and a following dry-run transactions push would see
// every account as unknown. Dependent calls therefore accept the bank's
// `accounts` (+ institution_name / as_of) and run the accounts step first,
// inside the same transaction. Harmless on live pushes too.

fn accounts_prelude(args: &Value) -> Result<Option<AccountsPayload>, AppError> {
    match args.get("accounts") {
        None | Some(Value::Null) => Ok(None),
        Some(_) => {
            let p: AccountsPayload = parse_payload(args)?;
            if p.accounts.len() > MAX_ROWS {
                return Err(bad(format!("accounts must contain at most {MAX_ROWS} entries")));
            }
            Ok(Some(p).filter(|p| !p.accounts.is_empty()))
        }
    }
}

async fn run_prelude(
    conn: &mut PgConnection,
    ctx: &Ctx<'_>,
    p: Option<&AccountsPayload>,
) -> Result<Option<Report>, AppError> {
    match p {
        Some(p) => Ok(Some(ingest_accounts_inner(conn, ctx, p).await?)),
        None => Ok(None),
    }
}

fn merge_prelude(mut r: Report, pre: Option<Report>) -> Report {
    if let Some(pre) = pre {
        r.notes.push(format!(
            "accounts step: {} received, {} inserted, {} updated ({} matched), {} skipped",
            pre.received, pre.inserted, pre.updated, pre.matched, pre.skipped
        ));
        r.accounts = pre.accounts;
        for s in pre.skips {
            if r.skips.len() < MAX_REPORTED {
                r.skips.push(json!({ "id": s["id"], "reason": format!("account: {}", s["reason"].as_str().unwrap_or("")) }));
            }
        }
    }
    r
}

// ── Bank transactions ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TxPayload {
    #[serde(default)]
    added: Vec<InTxn>,
    #[serde(default)]
    modified: Vec<InTxn>,
    /// `/transactions/get` shape; treated like `added`.
    #[serde(default)]
    transactions: Vec<InTxn>,
    #[serde(default)]
    removed: Vec<RemovedRef>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RemovedRef {
    Id(String),
    Obj { transaction_id: String },
}

impl RemovedRef {
    fn id(&self) -> &str {
        match self {
            RemovedRef::Id(s) | RemovedRef::Obj { transaction_id: s } => s,
        }
    }
}

#[derive(Deserialize)]
struct InTxn {
    transaction_id: String,
    account_id: String,
    amount: f64,
    date: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    original_description: Option<String>,
    #[serde(default)]
    pending: bool,
    #[serde(default)]
    pending_transaction_id: Option<String>,
    #[serde(default)]
    merchant_name: Option<String>,
    #[serde(default)]
    payment_channel: Option<String>,
    #[serde(default)]
    category: Option<Vec<String>>,
    #[serde(default)]
    personal_finance_category: Option<InPfc>,
}

#[derive(Deserialize)]
struct InPfc {
    primary: String,
    detailed: String,
}

impl InTxn {
    fn description(&self) -> String {
        [&self.name, &self.original_description, &self.merchant_name]
            .into_iter()
            .flatten()
            .find(|s| !s.is_empty())
            .cloned()
            .unwrap_or_default()
    }
}

pub async fn ingest_transactions(state: &Arc<AppState>, user_id: &str, args: &Value) -> Result<Report, AppError> {
    let payload: TxPayload = parse_payload(args)?;
    let dry_run = args["dry_run"].as_bool().unwrap_or(false);
    let upserts = payload.added.len() + payload.modified.len() + payload.transactions.len();
    if upserts > MAX_ROWS || payload.removed.len() > MAX_ROWS {
        return Err(bad(format!("at most {MAX_ROWS} added+modified+transactions and {MAX_ROWS} removed per call")));
    }
    if upserts == 0 && payload.removed.is_empty() {
        return Err(bad("nothing to ingest: pass added / modified / transactions / removed"));
    }
    let prelude = accounts_prelude(args)?;
    let ctx = load_ctx(state, user_id).await?;
    let mut tx = state.pool.begin().await?;
    let result = match run_prelude(&mut tx, &ctx, prelude.as_ref()).await {
        Ok(pre) => ingest_transactions_inner(&mut tx, &ctx, &payload).await.map(|r| merge_prelude(r, pre)),
        Err(e) => Err(e),
    };
    finish(state, user_id, "wealth_ingest_transactions", dry_run, tx, result).await
}

async fn fuzzy_txn(
    conn: &mut PgConnection,
    ctx: &Ctx<'_>,
    account: &str,
    amount: i64,
    date: NaiveDate,
    description: &str,
    pending: bool,
) -> Result<Option<String>, AppError> {
    // txn_date is TEXT; the CASE guards the cast against any malformed legacy value.
    Ok(sqlx::query_scalar(
        "SELECT id FROM (
           SELECT t.id, t.raw_string, t.pending,
                  abs((CASE WHEN t.txn_date ~ '^[0-9]{4}-[0-9]{2}-[0-9]{2}$' THEN t.txn_date::date END) - $3) AS dd
           FROM transactions t
           WHERE t.account_id = $1 AND t.amount = $2
             AND NOT EXISTS (SELECT 1 FROM external_id_map m
                             WHERE m.user_id = $4 AND m.kind = 'transaction' AND m.internal_id = t.id)
         ) c
         WHERE dd <= $5
         ORDER BY (raw_string = $6) DESC, dd, (pending = $7) DESC, id
         LIMIT 1",
    )
    .bind(account).bind(amount).bind(date).bind(ctx.user_id).bind(FUZZY_DAYS).bind(description).bind(pending)
    .fetch_optional(&mut *conn)
    .await?)
}

async fn ingest_transactions_inner(conn: &mut PgConnection, ctx: &Ctx<'_>, p: &TxPayload) -> Result<Report, AppError> {
    let mut r = Report {
        received: p.added.len() + p.modified.len() + p.transactions.len() + p.removed.len(),
        ..Default::default()
    };
    let mut accounts: HashMap<String, Option<String>> = HashMap::new();

    for t in p.added.iter().chain(&p.modified).chain(&p.transactions) {
        let ext = t.transaction_id.as_str();
        let Some(date) = parse_date(&t.date) else {
            r.skip(ext, "date must be YYYY-MM-DD");
            continue;
        };
        let account = match accounts.get(&t.account_id) {
            Some(a) => a.clone(),
            None => {
                let a = known_account(conn, ctx, &t.account_id).await?;
                accounts.insert(t.account_id.clone(), a.clone());
                a
            }
        };
        let Some(account) = account else {
            r.skip(ext, UNKNOWN_ACCOUNT);
            continue;
        };
        let amount = normalize::cents(t.amount);
        let description = t.description();

        let (internal, is_new) = match resolve(conn, ctx, Kind::Transaction, ext).await? {
            Resolved::Found(id, has_map) => {
                if !has_map {
                    map_insert(conn, ctx, Kind::Transaction, ext, &id, "same_id").await?;
                }
                (id, false)
            }
            Resolved::Foreign => {
                r.skip(ext, "transaction_id belongs to another user");
                continue;
            }
            Resolved::Superseded => {
                r.skip(ext, "transaction_id was already replaced by its posted transaction");
                continue;
            }
            Resolved::NotFound => {
                let mut found = None;
                // Posted transaction replacing a pending one: reuse the pending
                // row so its tags and notes carry over.
                if let Some(pid) = t.pending_transaction_id.as_deref().filter(|s| !s.is_empty()) {
                    if let Resolved::Found(pending_row, _) = resolve(conn, ctx, Kind::Transaction, pid).await? {
                        unmap_external(conn, ctx, Kind::Transaction, pid).await?;
                        found = Some((pending_row, "pending_posted"));
                    }
                }
                if found.is_none() {
                    if let Some(id) = fuzzy_txn(conn, ctx, &account, amount, date, &description, t.pending).await? {
                        found = Some((id, "fuzzy"));
                    }
                }
                match found {
                    Some((id, how)) => {
                        map_insert(conn, ctx, Kind::Transaction, ext, &id, how).await?;
                        r.matched(ext, &id, how);
                        (id, false)
                    }
                    None => {
                        map_insert(conn, ctx, Kind::Transaction, ext, ext, "created").await?;
                        (ext.to_string(), true)
                    }
                }
            }
        };

        let (raw_plain, raw_enc) = privacy::seal_required(ctx.pk(), &description)?;
        let (merchant_plain, merchant_enc) = privacy::seal_optional(ctx.pk(), t.merchant_name.clone())?;
        let category = t.category.as_ref().map(|c| c.join(", "));
        let pfc_primary = t.personal_finance_category.as_ref().map(|c| c.primary.clone());
        let pfc_detailed = t.personal_finance_category.as_ref().map(|c| c.detailed.clone());

        // account_id is deliberately not updated: a matched row stays where it is.
        sqlx::query(
            "INSERT INTO transactions (id, account_id, txn_date, raw_string, raw_string_enc, merchant_name,
                merchant_name_enc, amount, pending, payment_channel, plaid_category,
                plaid_primary_category, plaid_detailed_category)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
             ON CONFLICT (id) DO UPDATE SET
                txn_date = EXCLUDED.txn_date,
                raw_string = EXCLUDED.raw_string, raw_string_enc = EXCLUDED.raw_string_enc,
                merchant_name = EXCLUDED.merchant_name, merchant_name_enc = EXCLUDED.merchant_name_enc,
                amount = EXCLUDED.amount, pending = EXCLUDED.pending,
                payment_channel = COALESCE(EXCLUDED.payment_channel, transactions.payment_channel),
                plaid_category = COALESCE(EXCLUDED.plaid_category, transactions.plaid_category),
                plaid_primary_category = COALESCE(EXCLUDED.plaid_primary_category, transactions.plaid_primary_category),
                plaid_detailed_category = COALESCE(EXCLUDED.plaid_detailed_category, transactions.plaid_detailed_category)",
        )
        .bind(&internal).bind(&account).bind(date.to_string())
        .bind(&raw_plain).bind(raw_enc).bind(&merchant_plain).bind(merchant_enc)
        .bind(amount).bind(t.pending).bind(&t.payment_channel)
        .bind(category).bind(pfc_primary).bind(pfc_detailed)
        .execute(&mut *conn)
        .await?;

        if is_new {
            r.inserted += 1;
        } else {
            r.updated += 1;
        }
    }

    for rem in &p.removed {
        let ext = rem.id();
        match resolve(conn, ctx, Kind::Transaction, ext).await? {
            Resolved::Found(id, _) => {
                sqlx::query("DELETE FROM transactions WHERE id = $1").bind(&id).execute(&mut *conn).await?;
                sqlx::query("DELETE FROM external_id_map WHERE user_id = $1 AND kind = 'transaction' AND internal_id = $2")
                    .bind(ctx.user_id).bind(&id)
                    .execute(&mut *conn)
                    .await?;
                r.removed += 1;
            }
            Resolved::Foreign => r.skip(ext, "removed id belongs to another user"),
            // Already replaced by its posted transaction, or never seen: nothing to delete.
            Resolved::Superseded | Resolved::NotFound => {}
        }
    }
    Ok(r)
}

// ── Investment transactions ───────────────────────────────────────────────────

#[derive(Deserialize)]
struct InvPayload {
    investment_transactions: Vec<InInvTxn>,
    #[serde(default)]
    securities: Vec<InSecurity>,
}

#[derive(Deserialize)]
struct InInvTxn {
    investment_transaction_id: String,
    account_id: String,
    #[serde(default)]
    security_id: Option<String>,
    date: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    quantity: Option<f64>,
    amount: f64,
    #[serde(default)]
    price: Option<f64>,
    #[serde(default)]
    fees: Option<f64>,
    #[serde(rename = "type", default)]
    txn_type: Option<String>,
    #[serde(default)]
    subtype: Option<String>,
}

#[derive(Deserialize)]
struct InSecurity {
    security_id: String,
    #[serde(default)]
    ticker_symbol: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

fn security_map(securities: &[InSecurity]) -> HashMap<&str, &InSecurity> {
    securities.iter().map(|s| (s.security_id.as_str(), s)).collect()
}

pub async fn ingest_investment_transactions(state: &Arc<AppState>, user_id: &str, args: &Value) -> Result<Report, AppError> {
    let payload: InvPayload = parse_payload(args)?;
    let dry_run = args["dry_run"].as_bool().unwrap_or(false);
    if payload.investment_transactions.is_empty() || payload.investment_transactions.len() > MAX_ROWS {
        return Err(bad(format!("investment_transactions must contain 1-{MAX_ROWS} entries")));
    }
    let prelude = accounts_prelude(args)?;
    let ctx = load_ctx(state, user_id).await?;
    let mut tx = state.pool.begin().await?;
    let result = match run_prelude(&mut tx, &ctx, prelude.as_ref()).await {
        Ok(pre) => ingest_inv_inner(&mut tx, &ctx, &payload).await.map(|r| merge_prelude(r, pre)),
        Err(e) => Err(e),
    };
    finish(state, user_id, "wealth_ingest_investment_transactions", dry_run, tx, result).await
}

async fn ingest_inv_inner(conn: &mut PgConnection, ctx: &Ctx<'_>, p: &InvPayload) -> Result<Report, AppError> {
    let mut r = Report { received: p.investment_transactions.len(), ..Default::default() };
    let secs = security_map(&p.securities);
    let mut accounts: HashMap<String, Option<String>> = HashMap::new();

    for t in &p.investment_transactions {
        let ext = t.investment_transaction_id.as_str();
        let Some(date) = parse_date(&t.date) else {
            r.skip(ext, "date must be YYYY-MM-DD");
            continue;
        };
        let account = match accounts.get(&t.account_id) {
            Some(a) => a.clone(),
            None => {
                let a = known_account(conn, ctx, &t.account_id).await?;
                accounts.insert(t.account_id.clone(), a.clone());
                a
            }
        };
        let Some(account) = account else {
            r.skip(ext, UNKNOWN_ACCOUNT);
            continue;
        };
        let amount = normalize::cents(t.amount);
        let txn_type = t.txn_type.clone().unwrap_or_else(|| "unknown".to_string());

        let (internal, is_new) = match resolve(conn, ctx, Kind::InvestmentTransaction, ext).await? {
            Resolved::Found(id, has_map) => {
                if !has_map {
                    map_insert(conn, ctx, Kind::InvestmentTransaction, ext, &id, "same_id").await?;
                }
                (id, false)
            }
            Resolved::Foreign => {
                r.skip(ext, "investment_transaction_id belongs to another user");
                continue;
            }
            Resolved::Superseded => {
                r.skip(ext, "investment_transaction_id already reused for another pushed row");
                continue;
            }
            Resolved::NotFound => {
                let fuzzy: Option<String> = sqlx::query_scalar(
                    "SELECT id FROM (
                       SELECT it.id, it.name, abs(it.date - $3) AS dd
                       FROM investment_transactions it
                       WHERE it.account_id = $1 AND it.amount = $2 AND it.txn_type = $4
                         AND ((it.quantity IS NULL AND $5::float8 IS NULL) OR abs(it.quantity - $5::float8) < 0.000001)
                         AND NOT EXISTS (SELECT 1 FROM external_id_map m WHERE m.user_id = $6
                                         AND m.kind = 'investment_transaction' AND m.internal_id = it.id)
                     ) c
                     WHERE dd <= $7
                     ORDER BY (name = $8) DESC, dd, id
                     LIMIT 1",
                )
                .bind(&account).bind(amount).bind(date).bind(&txn_type).bind(t.quantity)
                .bind(ctx.user_id).bind(FUZZY_DAYS).bind(&t.name)
                .fetch_optional(&mut *conn)
                .await?;
                match fuzzy {
                    Some(id) => {
                        map_insert(conn, ctx, Kind::InvestmentTransaction, ext, &id, "fuzzy").await?;
                        r.matched(ext, &id, "fuzzy");
                        (id, false)
                    }
                    None => {
                        map_insert(conn, ctx, Kind::InvestmentTransaction, ext, ext, "created").await?;
                        (ext.to_string(), true)
                    }
                }
            }
        };

        let sec = t.security_id.as_deref().and_then(|id| secs.get(id));
        let symbol = sec.and_then(|s| s.ticker_symbol.clone());
        let security_name = sec.and_then(|s| s.name.clone());
        let (name_plain, name_enc) = privacy::seal_required(ctx.pk(), &t.name)?;
        let (symbol_plain, symbol_enc) = privacy::seal_symbol(ctx.pk(), symbol)?;
        let (sec_plain, sec_enc) = privacy::seal_optional(ctx.pk(), security_name)?;

        // Symbol / security name only overwrite when this push carries them,
        // so a push without `securities` doesn't blank existing values.
        sqlx::query(
            "INSERT INTO investment_transactions
               (id, account_id, date, name, name_enc, amount, quantity, price, fees, txn_type, subtype,
                symbol, symbol_enc, security_name, security_name_enc)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
             ON CONFLICT (id) DO UPDATE SET
               date = EXCLUDED.date, name = EXCLUDED.name, name_enc = EXCLUDED.name_enc,
               amount = EXCLUDED.amount, quantity = EXCLUDED.quantity, price = EXCLUDED.price,
               fees = EXCLUDED.fees, txn_type = EXCLUDED.txn_type, subtype = EXCLUDED.subtype,
               symbol = CASE WHEN EXCLUDED.symbol IS NOT NULL OR EXCLUDED.symbol_enc IS NOT NULL
                             THEN EXCLUDED.symbol ELSE investment_transactions.symbol END,
               symbol_enc = CASE WHEN EXCLUDED.symbol IS NOT NULL OR EXCLUDED.symbol_enc IS NOT NULL
                                 THEN EXCLUDED.symbol_enc ELSE investment_transactions.symbol_enc END,
               security_name = CASE WHEN EXCLUDED.security_name IS NOT NULL OR EXCLUDED.security_name_enc IS NOT NULL
                                    THEN EXCLUDED.security_name ELSE investment_transactions.security_name END,
               security_name_enc = CASE WHEN EXCLUDED.security_name IS NOT NULL OR EXCLUDED.security_name_enc IS NOT NULL
                                        THEN EXCLUDED.security_name_enc ELSE investment_transactions.security_name_enc END",
        )
        .bind(&internal).bind(&account).bind(date).bind(&name_plain).bind(name_enc).bind(amount)
        .bind(t.quantity).bind(t.price.map(normalize::cents)).bind(t.fees.map(normalize::cents))
        .bind(&txn_type).bind(&t.subtype)
        .bind(symbol_plain).bind(symbol_enc).bind(sec_plain).bind(sec_enc)
        .execute(&mut *conn)
        .await?;

        if is_new {
            r.inserted += 1;
        } else {
            r.updated += 1;
        }
    }
    Ok(r)
}

// ── Holdings ──────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct HoldingsPayload {
    holdings: Vec<InHolding>,
    #[serde(default)]
    securities: Vec<InSecurity>,
    /// Extra accounts to treat as fully covered by this push (e.g. an account
    /// whose positions were all sold, so it has no holdings left).
    #[serde(default)]
    account_ids: Vec<String>,
}

#[derive(Deserialize)]
struct InHolding {
    account_id: String,
    security_id: String,
    institution_value: f64,
    #[serde(default)]
    quantity: Option<f64>,
}

pub async fn ingest_holdings(state: &Arc<AppState>, user_id: &str, args: &Value) -> Result<Report, AppError> {
    let payload: HoldingsPayload = parse_payload(args)?;
    let dry_run = args["dry_run"].as_bool().unwrap_or(false);
    if payload.holdings.len() > MAX_ROWS {
        return Err(bad(format!("at most {MAX_ROWS} holdings per call")));
    }
    if payload.holdings.is_empty() && payload.account_ids.is_empty() {
        return Err(bad("nothing to ingest: pass holdings and/or account_ids"));
    }
    let prelude = accounts_prelude(args)?;
    let ctx = load_ctx(state, user_id).await?;
    let mut tx = state.pool.begin().await?;
    let result = match run_prelude(&mut tx, &ctx, prelude.as_ref()).await {
        Ok(pre) => ingest_holdings_inner(&mut tx, &ctx, &payload).await.map(|r| merge_prelude(r, pre)),
        Err(e) => Err(e),
    };
    finish(state, user_id, "wealth_ingest_holdings", dry_run, tx, result).await
}

async fn ingest_holdings_inner(conn: &mut PgConnection, ctx: &Ctx<'_>, p: &HoldingsPayload) -> Result<Report, AppError> {
    let mut r = Report { received: p.holdings.len(), ..Default::default() };
    let secs = security_map(&p.securities);

    // External account id → its holdings, keeping first-seen order.
    let mut order: Vec<&str> = Vec::new();
    let mut by_account: HashMap<&str, Vec<&InHolding>> = HashMap::new();
    for ext in &p.account_ids {
        if !by_account.contains_key(ext.as_str()) {
            order.push(ext);
            by_account.insert(ext, Vec::new());
        }
    }
    for h in &p.holdings {
        if !by_account.contains_key(h.account_id.as_str()) {
            order.push(&h.account_id);
        }
        by_account.entry(&h.account_id).or_default().push(h);
    }

    for ext in order {
        let rows = &by_account[ext];
        let Some(account) = known_account(conn, ctx, ext).await? else {
            r.skip(ext, UNKNOWN_ACCOUNT);
            r.skipped += rows.len().saturating_sub(1);
            continue;
        };

        // A holdings push is a full snapshot of the account: positions it no
        // longer lists are gone. The synthetic cash/debt row of a
        // non-investment account is owned by wealth_ingest_accounts.
        let deleted = sqlx::query(
            "DELETE FROM holdings WHERE account_id = $1
               AND security_id <> ('CASH_' || $1) AND security_id <> ('DEBT_' || $1)",
        )
        .bind(&account)
        .execute(&mut *conn)
        .await?
        .rows_affected();

        let mut total = 0i64;
        for h in rows {
            let sec = secs.get(h.security_id.as_str());
            let name = sec.and_then(|s| s.name.clone()).unwrap_or_else(|| "Unknown Security".to_string());
            let symbol = sec.and_then(|s| s.ticker_symbol.clone()).unwrap_or_else(|| "N/A".to_string());
            let value = normalize::cents(h.institution_value);
            total += value;
            let (symbol_plain, symbol_enc) = if privacy::is_sentinel_symbol(&symbol) {
                (symbol.clone(), None)
            } else {
                privacy::seal_required(ctx.pk(), &symbol)?
            };
            let (name_plain, name_enc) = privacy::seal_required(ctx.pk(), &name)?;
            sqlx::query(
                "INSERT INTO holdings (symbol, symbol_enc, account_id, name, name_enc, asset_value,
                    return_str, performance_type, security_id, quantity)
                 VALUES ($1, $2, $3, $4, $5, $6, 'Real-time', 'pos', $7, $8)
                 ON CONFLICT (security_id, account_id) DO UPDATE SET
                    asset_value = holdings.asset_value + EXCLUDED.asset_value,
                    quantity = COALESCE(holdings.quantity, 0) + COALESCE(EXCLUDED.quantity, 0)",
            )
            .bind(&symbol_plain).bind(symbol_enc).bind(&account).bind(&name_plain).bind(name_enc)
            .bind(value).bind(&h.security_id).bind(h.quantity)
            .execute(&mut *conn)
            .await?;
            r.inserted += 1;
        }

        let balance: i64 = sqlx::query_scalar("SELECT balance FROM accounts WHERE id = $1")
            .bind(&account)
            .fetch_one(&mut *conn)
            .await?;
        let unallocated = balance - total;
        let has_cash_row: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM holdings WHERE account_id = $1
               AND security_id IN ('CASH_' || $1, 'DEBT_' || $1))",
        )
        .bind(&account)
        .fetch_one(&mut *conn)
        .await?;
        let wrote_unallocated = !has_cash_row && unallocated.abs() > 100;
        if wrote_unallocated {
            sqlx::query(
                "INSERT INTO holdings (symbol, account_id, name, asset_value, return_str, performance_type, security_id)
                 VALUES ('CASH', $1, 'Unallocated Cash/Other', $2, '0%', 'neu', $3)",
            )
            .bind(&account).bind(unallocated).bind(format!("UNALLOCATED_{}", account))
            .execute(&mut *conn)
            .await?;
        }

        r.accounts.push(json!({
            "external_id": ext,
            "internal_id": account,
            "positions": rows.len(),
            "positions_replaced": deleted,
            "holdings_value": total,
            "account_balance": balance,
            "unallocated": if wrote_unallocated { json!(unallocated) } else { Value::Null },
        }));
    }
    Ok(r)
}

// ── Mapping management ────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
pub struct MapRow {
    pub kind: String,
    pub external_id: String,
    pub internal_id: String,
    pub match_method: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn map_list(
    state: &Arc<AppState>,
    user_id: &str,
    kind: Option<Kind>,
    limit: i64,
    offset: i64,
) -> Result<Value, AppError> {
    let kind_s = kind.map(Kind::as_str);
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM external_id_map WHERE user_id = $1 AND ($2::text IS NULL OR kind = $2)",
    )
    .bind(user_id).bind(kind_s)
    .fetch_one(&state.pool)
    .await?;
    let rows: Vec<MapRow> = sqlx::query_as(
        "SELECT kind, external_id, internal_id, match_method, created_at FROM external_id_map
         WHERE user_id = $1 AND ($2::text IS NULL OR kind = $2)
         ORDER BY kind, created_at, external_id LIMIT $3 OFFSET $4",
    )
    .bind(user_id).bind(kind_s).bind(limit).bind(offset)
    .fetch_all(&state.pool)
    .await?;
    Ok(json!({ "total": total, "offset": offset, "items": rows }))
}

/// Link a pushed id to an existing row by hand (e.g. an ambiguous account).
/// Refuses to steal a row already claimed by another pushed id, and refuses to
/// re-point a pushed id that already created its own row with data in it.
pub async fn map_set(
    state: &Arc<AppState>,
    user_id: &str,
    kind: Kind,
    external_id: &str,
    internal_id: &str,
) -> Result<Value, AppError> {
    let ctx = Ctx { user_id, pubkey: None };
    let mut tx = state.pool.begin().await?;
    if !owned(&mut tx, &ctx, kind, internal_id).await? {
        return Err(AppError::NotFound(format!("{} {internal_id} not found", kind.as_str())));
    }
    let claimed: Option<String> = sqlx::query_scalar(
        "SELECT external_id FROM external_id_map
         WHERE user_id = $1 AND kind = $2 AND internal_id = $3 AND external_id <> $4",
    )
    .bind(user_id).bind(kind.as_str()).bind(internal_id).bind(external_id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(other) = claimed {
        return Err(bad(format!("{internal_id} is already linked to pushed id {other}; delete that link first")));
    }
    let current: Option<String> = sqlx::query_scalar(
        "SELECT internal_id FROM external_id_map WHERE user_id = $1 AND kind = $2 AND external_id = $3",
    )
    .bind(user_id).bind(kind.as_str()).bind(external_id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(cur) = current.filter(|c| c != internal_id) {
        if owned(&mut tx, &ctx, kind, &cur).await? {
            return Err(bad(format!(
                "pushed id {external_id} is already linked to existing row {cur}; delete that link first"
            )));
        }
    }
    map_insert(&mut tx, &ctx, kind, external_id, internal_id, "manual").await?;
    tx.commit().await?;
    Ok(json!({ "status": "success", "kind": kind.as_str(), "external_id": external_id, "internal_id": internal_id }))
}

pub async fn map_delete(state: &Arc<AppState>, user_id: &str, kind: Kind, external_id: &str) -> Result<Value, AppError> {
    let n = sqlx::query("DELETE FROM external_id_map WHERE user_id = $1 AND kind = $2 AND external_id = $3")
        .bind(user_id).bind(kind.as_str()).bind(external_id)
        .execute(&state.pool)
        .await?
        .rows_affected();
    Ok(json!({ "status": "success", "deleted": n }))
}

pub async fn status(state: &Arc<AppState>, user_id: &str) -> Result<Value, AppError> {
    let recent: Vec<(String, bool, bool, String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT tool, dry_run, ok, summary::text, created_at FROM ingest_log
         WHERE user_id = $1 ORDER BY created_at DESC LIMIT 20",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await?;
    let recent: Vec<Value> = recent
        .into_iter()
        .map(|(tool, dry_run, ok, summary, at)| json!({
            "tool": tool, "dry_run": dry_run, "ok": ok, "at": at,
            "summary": serde_json::from_str::<Value>(&summary).unwrap_or(Value::Null),
        }))
        .collect();
    let links: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT kind, match_method, count(*) FROM external_id_map WHERE user_id = $1
         GROUP BY kind, match_method ORDER BY kind, match_method",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await?;
    let items: Vec<(String, String, bool)> = sqlx::query_as(
        "SELECT id, source, revoked_at IS NOT NULL FROM plaid_items WHERE user_id = $1 ORDER BY created_at",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await?;
    let last_snapshot: Option<NaiveDate> = sqlx::query_scalar(
        "SELECT max(s.snapshot_date) FROM account_snapshots s JOIN accounts a ON s.account_id = a.id
         JOIN plaid_items pi ON a.plaid_item_id = pi.id WHERE pi.user_id = $1",
    )
    .bind(user_id)
    .fetch_one(&state.pool)
    .await?;
    Ok(json!({
        "plaid_sync_enabled": state.plaid_enabled,
        "last_snapshot_date": last_snapshot.map(|d| d.to_string()),
        "items": items.into_iter().map(|(id, source, revoked)| json!({"id": id, "source": source, "revoked": revoked})).collect::<Vec<_>>(),
        "id_links": links.into_iter().map(|(k, m, n)| json!({"kind": k, "method": m, "count": n})).collect::<Vec<_>>(),
        "recent_pushes": recent,
    }))
}

// ── History export ────────────────────────────────────────────────────────────

/// Accept either our account id or a pushed (external) one.
async fn internal_account_id(state: &Arc<AppState>, user_id: &str, id: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(id) = id else { return Ok(None) };
    let mapped: Option<String> = sqlx::query_scalar(
        "SELECT internal_id FROM external_id_map WHERE user_id = $1 AND kind = 'account' AND external_id = $2",
    )
    .bind(user_id).bind(id)
    .fetch_optional(&state.pool)
    .await?;
    Ok(Some(mapped.unwrap_or_else(|| id.to_string())))
}

/// internal id → pushed id, for the given rows.
async fn external_ids(state: &Arc<AppState>, user_id: &str, kind: Kind, ids: &[String]) -> Result<HashMap<String, String>, AppError> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT internal_id, external_id FROM external_id_map
         WHERE user_id = $1 AND kind = $2 AND internal_id = ANY($3)",
    )
    .bind(user_id).bind(kind.as_str()).bind(ids)
    .fetch_all(&state.pool)
    .await?;
    Ok(rows.into_iter().collect())
}

/// Add `external_id` / `external_account_id` to exported rows.
async fn annotate(
    state: &Arc<AppState>,
    user_id: &str,
    kind: Option<Kind>,
    items: &mut [Value],
) -> Result<(), AppError> {
    let ids: Vec<String> = items.iter().filter_map(|v| v["id"].as_str().map(str::to_string)).collect();
    let mut accts: Vec<String> = items.iter().filter_map(|v| v["account_id"].as_str().map(str::to_string)).collect();
    accts.sort();
    accts.dedup();
    let row_map = match kind {
        Some(k) => external_ids(state, user_id, k, &ids).await?,
        None => HashMap::new(),
    };
    let acct_map = external_ids(state, user_id, Kind::Account, &accts).await?;
    for v in items.iter_mut() {
        if kind.is_some() {
            let ext = v["id"].as_str().and_then(|id| row_map.get(id)).cloned();
            v["external_id"] = json!(ext);
        }
        let ext_acct = v["account_id"].as_str().and_then(|id| acct_map.get(id)).cloned();
        v["external_account_id"] = json!(ext_acct);
    }
    Ok(())
}

pub struct ExportRange<'a> {
    pub account_id: Option<&'a str>,
    pub since: Option<&'a str>,
    pub until: Option<&'a str>,
    pub limit: i64,
    pub offset: i64,
}

fn range_dates(r: &ExportRange<'_>) -> Result<(Option<NaiveDate>, Option<NaiveDate>), AppError> {
    let since = r.since.map(|s| parse_date(s).ok_or_else(|| bad("since must be YYYY-MM-DD"))).transpose()?;
    let until = r.until.map(|s| parse_date(s).ok_or_else(|| bad("until must be YYYY-MM-DD"))).transpose()?;
    Ok((since, until))
}

pub async fn export_snapshots(state: &Arc<AppState>, user_id: &str, r: &ExportRange<'_>) -> Result<Value, AppError> {
    #[derive(Serialize, sqlx::FromRow)]
    struct SnapRow {
        account_id: String,
        snapshot_date: NaiveDate,
        balance_cents: i64,
    }
    let (since, until) = range_dates(r)?;
    let account = internal_account_id(state, user_id, r.account_id).await?;
    const FROM: &str = "FROM account_snapshots s JOIN accounts a ON s.account_id = a.id
         JOIN plaid_items pi ON a.plaid_item_id = pi.id
         WHERE pi.user_id = $1 AND ($2::text IS NULL OR s.account_id = $2)
           AND ($3::date IS NULL OR s.snapshot_date >= $3) AND ($4::date IS NULL OR s.snapshot_date <= $4)";
    let total: i64 = sqlx::query_scalar(&format!("SELECT count(*) {FROM}"))
        .bind(user_id).bind(&account).bind(since).bind(until)
        .fetch_one(&state.pool)
        .await?;
    let rows: Vec<SnapRow> = sqlx::query_as(&format!(
        "SELECT s.account_id, s.snapshot_date, s.balance_cents {FROM}
         ORDER BY s.account_id, s.snapshot_date LIMIT $5 OFFSET $6"
    ))
    .bind(user_id).bind(&account).bind(since).bind(until).bind(r.limit).bind(r.offset)
    .fetch_all(&state.pool)
    .await?;
    let mut items: Vec<Value> = rows.into_iter().map(|r| json!(r)).collect();
    annotate(state, user_id, None, &mut items).await?;
    Ok(json!({ "total": total, "offset": r.offset, "items": items }))
}

#[derive(Serialize, sqlx::FromRow)]
struct HoldingRow {
    account_id: String,
    security_id: Option<String>,
    symbol: String,
    name: String,
    asset_value: i64,
    quantity: Option<f64>,
    #[serde(skip)]
    symbol_enc: Option<Vec<u8>>,
    #[serde(skip)]
    name_enc: Option<Vec<u8>>,
}

impl Sealed for HoldingRow {
    fn reveal(&mut self, crypto: &UserCrypto) {
        privacy::open_into(&mut self.symbol, &self.symbol_enc, crypto);
        privacy::open_into(&mut self.name, &self.name_enc, crypto);
    }
}

pub async fn export_holdings(
    state: &Arc<AppState>,
    user_id: &str,
    account_id: Option<&str>,
    crypto: &UserCrypto,
) -> Result<Value, AppError> {
    let account = internal_account_id(state, user_id, account_id).await?;
    let mut rows: Vec<HoldingRow> = sqlx::query_as(
        "SELECT h.account_id, h.security_id, h.symbol, h.name, h.asset_value, h.quantity, h.symbol_enc, h.name_enc
         FROM holdings h JOIN accounts a ON h.account_id = a.id JOIN plaid_items pi ON a.plaid_item_id = pi.id
         WHERE pi.user_id = $1 AND ($2::text IS NULL OR h.account_id = $2)
         ORDER BY h.account_id, h.asset_value DESC",
    )
    .bind(user_id).bind(&account)
    .fetch_all(&state.pool)
    .await?;
    privacy::reveal_all(&mut rows, crypto);
    let mut items: Vec<Value> = rows.into_iter().map(|r| json!(r)).collect();
    annotate(state, user_id, None, &mut items).await?;
    Ok(json!({ "total": items.len(), "items": items }))
}

pub async fn export_investment_transactions(
    state: &Arc<AppState>,
    user_id: &str,
    r: &ExportRange<'_>,
    crypto: &UserCrypto,
) -> Result<Value, AppError> {
    let (since, until) = range_dates(r)?;
    let account = internal_account_id(state, user_id, r.account_id).await?;
    const FROM: &str = "FROM investment_transactions it JOIN accounts a ON it.account_id = a.id
         JOIN plaid_items pi ON a.plaid_item_id = pi.id
         WHERE pi.user_id = $1 AND ($2::text IS NULL OR it.account_id = $2)
           AND ($3::date IS NULL OR it.date >= $3) AND ($4::date IS NULL OR it.date <= $4)";
    let total: i64 = sqlx::query_scalar(&format!("SELECT count(*) {FROM}"))
        .bind(user_id).bind(&account).bind(since).bind(until)
        .fetch_one(&state.pool)
        .await?;
    let mut rows: Vec<InvestmentTransaction> = sqlx::query_as(&format!(
        "SELECT it.id, it.account_id, COALESCE(a.custom_name, a.name) as account_name,
                it.date::TEXT, it.name, it.name_enc, it.amount, it.quantity, it.price, it.fees,
                it.txn_type, it.subtype, it.symbol, it.symbol_enc, it.security_name, it.security_name_enc,
                a.custom_name_enc AS account_custom_name_enc, a.name_enc AS account_name_enc
         {FROM}
         ORDER BY it.date DESC, it.id LIMIT $5 OFFSET $6"
    ))
    .bind(user_id).bind(&account).bind(since).bind(until).bind(r.limit).bind(r.offset)
    .fetch_all(&state.pool)
    .await?;
    privacy::reveal_all(&mut rows, crypto);
    let mut items: Vec<Value> = rows.into_iter().map(|r| json!(r)).collect();
    annotate(state, user_id, Some(Kind::InvestmentTransaction), &mut items).await?;
    Ok(json!({ "total": total, "offset": r.offset, "items": items }))
}

pub async fn export_transactions(
    state: &Arc<AppState>,
    user_id: &str,
    r: &ExportRange<'_>,
    crypto: &UserCrypto,
) -> Result<Value, AppError> {
    range_dates(r)?;
    let account = internal_account_id(state, user_id, r.account_id).await?;
    let q = TransactionQuery {
        since: r.since.map(str::to_string),
        until: r.until.map(str::to_string),
        account_id: account,
        category: None,
        tag: None,
        search: None,
        pending: None,
        limit: Some(r.limit),
        offset: Some(r.offset),
    };
    let page = crate::db::inner_get_transactions(&state.pool, user_id, &q, crypto).await?;
    let mut items: Vec<Value> = page.items.iter().map(|t| json!(t)).collect();
    for v in items.iter_mut() {
        v["tags"] = match v["tags"].as_str() {
            Some(s) => json!(s.split(',').map(str::trim).filter(|t| !t.is_empty()).collect::<Vec<_>>()),
            None => json!([]),
        };
    }
    annotate(state, user_id, Some(Kind::Transaction), &mut items).await?;
    Ok(json!({ "total": page.total, "offset": page.offset, "items": items }))
}
