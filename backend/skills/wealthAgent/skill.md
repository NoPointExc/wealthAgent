# WealthAgent MCP — Agent Skill Reference

> **MCP server version:** 1.0 (Streamable HTTP)
> **skill.md version:** 1.1 · 2026-06-19
> **Changelog:** v1.1 — tags field changed from string to `string[]`; `clear_all` op removed (never implemented); T9/T10 `tax_year` reference corrected to top-level field; tests renumbered T22–T27.

> This document is the canonical reference for an AI agent using the WealthAgent MCP server.
> Read it in full before calling any tool. All data conventions, error shapes, and test cases are here.

---

## 1. What this server does

The WealthAgent MCP server exposes a user's personal financial data — bank transactions, investment accounts, capital gains — to AI agents over the Model Context Protocol (**HTTP transport**). The server endpoint is `https://<your-app-domain>/mcp` (the hosted service uses `https://app.texasnetworth.com/mcp`) and is authenticated with a personal API token (Bearer auth). No local binary required.

The server is **read-mostly**: 5 read tools, 5 write tools. Write tools mutate tags, notes, account names, and cost basis overrides; they do not delete data or move money.

---

## 2. Configuration

No local binary to install. The MCP server runs on the production server. All you need is a personal API token from **Settings → API Tokens** in the web app.

**Token format:** `wa_pat_<44 base64url chars>` — create one in the web app (Connect tab → Connected agents → Advanced).

**Claude Desktop config** (`~/Library/Application Support/Claude/claude_desktop_config.json`):
```json
{
  "mcpServers": {
    "wealthagent": {
      "type": "http",
      "url": "https://<your-app-domain>/mcp",
      "headers": {
        "Authorization": "Bearer wa_pat_..."
      }
    }
  }
}
```

**Cursor** (`.cursor/mcp.json`):
```json
{
  "mcpServers": {
    "wealthagent": {
      "type": "http",
      "url": "https://<your-app-domain>/mcp",
      "headers": {
        "Authorization": "Bearer wa_pat_..."
      }
    }
  }
}
```

**Protocol:** Streamable HTTP (MCP 2024-11-05). Every request is an HTTP POST to `/mcp` with a JSON-RPC 2.0 body and `Authorization: Bearer wa_pat_...` header. The server is stateless — no session ID required.

---

## 3. Data conventions — read these before interpreting any result

| Convention | Detail |
|---|---|
| **Amounts** | Always integers in **cents**. `amount: -8742` = −$87.42. Negative = outflow/expense; positive = inflow/income. |
| **Dates** | `"YYYY-MM-DD"` strings. Never timestamps, never epoch integers. |
| **Balances** | Cents. An account balance of `1234567` = $12,345.67. |
| **Tags** | JSON array of strings: `["groceries", "subscription"]`. Empty array `[]` when no tags. |
| **Null vs absent** | Optional fields are `null` when no value exists (e.g. `merchant_name: null`). |
| **Pagination** | `total` = total matching rows, `offset` = current page start, `items` = current page. Use `limit`+`offset` to page. |
| **Error shape** | On failure, the tool returns a JSON string: `{"error": "description"}`. Parse and surface this to the user. |

---

## 4. Tool reference

All tools return a **JSON string** (not a structured object). Parse it with `JSON.parse` or equivalent. All monetary values inside are in cents.

---

### 4.1 `wealth_whoami` — identity check

**Parameters:** none

**Returns:**
```json
{
  "user_id": "usr_abc123",
  "email": "user@example.com",
  "name": "Jane Doe",
  "scopes": "read,write,sync"
}
```

**Use for:** Verifying the token is valid and which user it belongs to. Always call this first in a test suite.

---

### 4.2 `wealth_accounts_list` — list accounts

**Parameters:** none

**Returns:** array of account objects
```json
[
  {
    "id": "acc_abc",
    "name": "Plaid Checking",
    "custom_name": "Chase Checking",
    "account_type": "asset",
    "balance": 482310,
    "trend_pct": 2.4,
    "plaid_item_id": "item_xyz"
  }
]
```

| Field | Type | Notes |
|---|---|---|
| `id` | string | Use this as `account_id` in filters |
| `name` | string | Plaid-provided name |
| `custom_name` | string\|null | User's display name (overrides `name` in UI) |
| `account_type` | `"asset"` \| `"liability"` | Credit cards = liability |
| `balance` | integer (cents) | Current balance |
| `trend_pct` | number\|null | % change vs prior period; null if < 2 snapshots |

---

### 4.3 `wealth_tx_list` — list transactions

**Parameters (all optional):**

| Parameter | Type | Description |
|---|---|---|
| `since` | string | Start date inclusive, `YYYY-MM-DD` |
| `until` | string | End date inclusive, `YYYY-MM-DD` |
| `account_id` | string | Filter to one account |
| `category` | string | Plaid primary category (e.g. `FOOD_AND_DRINK`, `TRANSPORTATION`, `SHOPPING`) |
| `tag` | string | Only transactions carrying this exact tag |
| `search` | string | Substring match across merchant name, raw description, and note |
| `pending` | boolean | `true` = pending only; `false` = cleared only; omit = both |
| `limit` | integer | Max rows, default 200, max 1000 |
| `offset` | integer | Pagination offset, default 0 |

**Returns:**
```json
{
  "items": [
    {
      "id": "txn_abc123",
      "account_id": "acc_abc",
      "account_name": "Chase Checking",
      "txn_date": "2026-06-15",
      "raw_string": "WHOLE FOODS MARKET #123",
      "merchant_name": "Whole Foods Market",
      "amount": -8742,
      "pending": false,
      "payment_channel": "in_store",
      "plaid_category": "Food and Drink",
      "plaid_primary_category": "FOOD_AND_DRINK",
      "plaid_detailed_category": "FOOD_AND_DRINK_GROCERIES",
      "tags": ["groceries"],
      "note": null
    }
  ],
  "total": 3847,
  "offset": 0
}
```

**Common `plaid_primary_category` values:** `FOOD_AND_DRINK`, `TRANSPORTATION`, `SHOPPING`, `ENTERTAINMENT`, `HEALTH_FITNESS`, `TRAVEL`, `INCOME`, `TRANSFER_IN`, `TRANSFER_OUT`, `LOAN_PAYMENTS`, `BANK_FEES`.

**Pagination pattern:**
```
# Page 1
wealth_tx_list({ limit: 200, offset: 0 })  →  items[0..199], total: 1450

# Page 2
wealth_tx_list({ limit: 200, offset: 200 })  →  items[200..399]
```

---

### 4.4 `wealth_gains` — capital gains report

**Parameters:**

| Parameter | Type | Description |
|---|---|---|
| `year` | integer\|null | Tax year, e.g. `2025`. Omit for current calendar year. |

**Returns:**
```json
{
  "realized_lots": [
    {
      "symbol": "AAPL",
      "security_name": "Apple Inc.",
      "open_date": "2023-04-12",
      "close_date": "2025-11-03",
      "quantity": 10.0,
      "cost_basis_cents": 152000,
      "proceeds_cents": 221500,
      "gain_cents": 69500,
      "is_long_term": true,
      "source": "fifo"
    }
  ],
  "unknown_basis_sales": [
    {
      "symbol": "META",
      "security_name": "Meta Platforms Inc.",
      "close_date": "2025-08-14",
      "quantity": 5.0,
      "proceeds_cents": 302000,
      "txn_id": "inv_txn_xyz",
      "user_cost_basis_cents": null,
      "user_is_long_term": null
    }
  ],
  "unrealized_positions": [
    {
      "symbol": "VOO",
      "security_name": "Vanguard S&P 500 ETF",
      "oldest_lot_date": "2022-01-15",
      "quantity": 25.5,
      "cost_basis_cents": 1124500,
      "current_value_cents": 1387000,
      "gain_cents": 262500,
      "is_long_term_if_sold_today": true,
      "has_unknown_basis": false
    }
  ],
  "summary": {
    "ytd_realized_gain_cents": 69500,
    "ytd_realized_loss_cents": -12000,
    "ytd_net_cents": 57500,
    "unrealized_gain_cents": 262500,
    "unrealized_loss_cents": null,
    "short_term_net_cents": 0,
    "long_term_net_cents": 57500
  },
  "tax_year": 2025
}
```

| Section | Notes |
|---|---|
| `realized_lots` | Closed positions matched by FIFO or manual cost basis. `source`: `"fifo"` \| `"user_input"` \| `"1099b"`. |
| `unknown_basis_sales` | Sells where basis can't be FIFO-computed (RSU vests, pre-history-cap sells). Entries **persist after `wealth_set_cost_basis`** — `user_cost_basis_cents` becomes non-null and the sale also appears in `realized_lots` with `source:"user_input"`. Think of it as the "input queue" vs. `realized_lots` as the computed output. |
| `unrealized_positions` | Open lots aggregated by symbol. `has_unknown_basis: true` means some buys for this symbol fall outside the brokerage's Plaid sync window — the displayed basis is partial, not zero. |
| `summary` | YTD totals for the requested `tax_year`. `ytd_realized_loss_cents` is negative or zero. All in cents. Note: `unrealized_gain_cents` / `unrealized_loss_cents` are always a current snapshot, regardless of the `year` param. |
| `tax_year` | Top-level integer (e.g. `2025`). Not inside `summary`. |

---

### 4.5 `wealth_tx_tag` — add or remove a tag ⚠️ WRITES DATA

**Parameters:**

| Parameter | Type | Required | Description |
|---|---|---|---|
| `transaction_ids` | string[] | **Yes** | 1–500 transaction IDs |
| `op` | `"add"` \| `"remove"` | **Yes** | Operation to perform |
| `value` | string | **Yes** | Tag name (e.g. `"groceries"`) |

**Notes:**
- Adding an existing tag is idempotent (no-op). Removing a tag that isn't present is also a no-op.
- `updated` in the response reflects actual DB rows changed, not input count. A fabricated ID returns `updated: 0`.
- **Confirm with the user before tagging > 50 transactions.**

**Returns on success:**
```json
{ "status": "success", "updated": 3 }
```

**Returns on error:**
```json
{ "error": "transaction_ids must contain 1–500 IDs" }
```

---

### 4.6 `wealth_tx_note` — set or clear a note ⚠️ WRITES DATA

**Parameters:**

| Parameter | Type | Required | Description |
|---|---|---|---|
| `transaction_ids` | string[] | **Yes** | 1–500 transaction IDs |
| `value` | string\|null | No | Note text; omit or `null` to clear |

**Returns on success:**
```json
{ "status": "success", "updated": 1 }
```

---

### 4.7 `wealth_accounts_rename` — set custom account name ⚠️ WRITES DATA

**Parameters:**

| Parameter | Type | Required | Description |
|---|---|---|---|
| `account_id` | string | **Yes** | Account `id` from `wealth_accounts_list` |
| `custom_name` | string\|null | No | New display name; `null` reverts to Plaid name |

**Returns on success:**
```json
{ "status": "success" }
```

---

### 4.8 `wealth_sync` — trigger Plaid refresh ⚠️ SIDE EFFECT

**Parameters:** none

**Behavior:** Blocks until the backend finishes syncing all Plaid items (~10–30 seconds). Only call when the user explicitly asks to refresh their data. Do not loop or retry — Plaid enforces per-item rate limits and excess calls will silently return stale data.

**Returns on success:**
```json
{ "status": "success", "synced": 3 }
```

---

### 4.9 `wealth_set_cost_basis` — set cost basis on an unknown sale ⚠️ WRITES DATA

Use this to resolve entries in `unknown_basis_sales` returned by `wealth_gains`. The `txn_id` comes directly from `unknown_basis_sales[].txn_id`.

**Parameters:**

| Parameter | Type | Required | Description |
|---|---|---|---|
| `txn_id` | string | **Yes** | `investment_transaction` id from `unknown_basis_sales[].txn_id` |
| `cost_basis_cents` | integer | **Yes** | Total cost basis in cents (positive). E.g. 10 shares at $12.50 = `12500`. |
| `is_long_term` | boolean | **Yes** | `true` if held > 1 year (long-term capital gains rate). |

**Returns on success:**
```json
{ "status": "success", "txn_id": "inv_txn_xyz", "cost_basis_cents": 150000, "is_long_term": true }
```

**Workflow:**
1. Call `wealth_gains` to get the report.
2. Find entries in `unknown_basis_sales` where `user_cost_basis_cents` is `null`.
3. Ask the user for the cost basis and whether it's long-term.
4. Call `wealth_set_cost_basis` with the `txn_id` from step 2.
5. Call `wealth_gains` again to verify: the sale should now appear in `realized_lots` with `source: "user_input"` AND in `unknown_basis_sales` with `user_cost_basis_cents` non-null. Both are expected — the entry stays in `unknown_basis_sales` as a record of what you entered.

---

### 4.10 `wealth_delete_cost_basis` — remove a cost basis override ⚠️ WRITES DATA

Reverts a previously set cost basis, returning the sale to `unknown_basis_sales`.

**Parameters:**

| Parameter | Type | Required | Description |
|---|---|---|---|
| `txn_id` | string | **Yes** | Same `txn_id` used when setting the basis |

**Returns on success:**
```json
{ "status": "success", "txn_id": "inv_txn_xyz" }
```

---

## 5. Error handling

All tools return a JSON string. On failure, the string has an `"error"` key:

```json
{ "error": "description of what went wrong" }
```

HTTP-level errors (before a tool is called) return standard HTTP status codes:
- `401 Unauthorized` — missing or invalid Bearer token
- `404 Not Found` — wrong URL
- `500 Internal Server Error` — backend crash

| Error content | Cause | Action |
|---|---|---|
| contains "Unauthorized" | Token missing, revoked, or expired | Create a new token in Settings |
| contains "scope error" | Token's scope doesn't permit this operation (e.g. read-only token calling a write tool) | Create a token with the required scope in Settings |
| contains "not found" | Resource doesn't exist | Verify the ID came from a prior list call |
| contains "1–500" | Empty or oversized `transaction_ids` array | Fix the array length |
| contains "unknown op" | Bad `op` value in `wealth_tx_tag` | Use `add` or `remove` |

---

## 6. Safety rules

1. **Never invent IDs.** Transaction IDs, account IDs, and `txn_id` values for cost basis must come from a prior tool call. Fabricated IDs will silently no-op or return errors.
2. **Confirm bulk writes.** Before calling `wealth_tx_tag` or `wealth_tx_note` on > 50 transactions, summarize what will happen and ask the user to confirm.
3. **`wealth_sync` is slow.** Only trigger it when the user asks. It blocks for 10–30 seconds and hits Plaid's rate limits.
4. **Read-only by default.** For analysis questions, use only the read tools. Never call write tools speculatively.
5. **Amounts are cents.** Convert for display: `amount / 100` with sign flip for expenses (`-8742` → `$87.42 expense`).
6. **Cost basis is permanent until deleted.** `wealth_set_cost_basis` uses upsert — calling it again overwrites the previous value. Confirm the numbers with the user before writing.

---

## 7. Test checklist

An agent testing this MCP should exercise the following cases in order. Each step depends on data from the prior step.

### Phase 1 — connectivity

- [ ] **T1** Call `wealth_whoami`. Expect a JSON object with `user_id`, `email`, `name`, `scopes`. Fail if `{"error": ...}`.
- [ ] **T2** Call `wealth_accounts_list`. Expect a non-empty JSON array. Each item has `id`, `balance` (integer), `account_type`.

### Phase 2 — read transactions

- [ ] **T3** Call `wealth_tx_list` with no params. Expect `total > 0`, `items` array non-empty, each item has `id`, `amount` (integer), `txn_date` (YYYY-MM-DD string).
- [ ] **T4** Call `wealth_tx_list` with `since: "2026-01-01"` and `until: "2026-06-30"`. Verify all returned `txn_date` values fall within that range.
- [ ] **T5** Call `wealth_tx_list` with `limit: 5`. Verify `items.length <= 5`.
- [ ] **T6** Call `wealth_tx_list` with `search: "amazon"` (case-insensitive). Verify returned transactions have "amazon" in `raw_string` or `merchant_name`.
- [ ] **T7** Call `wealth_tx_list` with `pending: true`. Verify all returned items have `pending: true`.
- [ ] **T8** Call `wealth_tx_list` with an `account_id` from T2. Verify all returned items have that `account_id`.

### Phase 3 — capital gains

- [ ] **T9** Call `wealth_gains` with no params (current year). Verify the response has `realized_lots`, `unknown_basis_sales`, `unrealized_positions`, `summary`, and a top-level `tax_year` integer. Verify `tax_year` equals the current calendar year.
- [ ] **T10** Call `wealth_gains` with `year: 2025`. Verify the top-level `tax_year == 2025` (not inside `summary`).
- [ ] **T11** Verify that in the summary, `ytd_net_cents == ytd_realized_gain_cents + ytd_realized_loss_cents` (where loss is negative). This validates the backend math.

### Phase 4 — write operations (use a throwaway transaction)

Pick **one** transaction ID from T3 (`items[0].id`) for all write tests. Call it `TEST_ID`.

- [ ] **T12** Call `wealth_tx_tag` with `transaction_ids: [TEST_ID]`, `op: "add"`, `value: "mcp-test"`. Expect `{"status":"success","updated":1}`.
- [ ] **T13** Call `wealth_tx_list` with `tag: "mcp-test"`. Verify `TEST_ID` appears in results.
- [ ] **T14** Call `wealth_tx_tag` with `op: "remove"`, `value: "mcp-test"` on `TEST_ID`. Expect `{"status":"success","updated":1}`.
- [ ] **T15** Call `wealth_tx_list` with `tag: "mcp-test"`. Verify `TEST_ID` is no longer in results (tag removed).
- [ ] **T16** Call `wealth_tx_note` with `transaction_ids: [TEST_ID]`, `value: "added by mcp-test agent"`. Expect `{"status":"success","updated":1}`.
- [ ] **T17** Call `wealth_tx_list` with `search: "added by mcp-test agent"`. Verify `TEST_ID` appears (note is searchable).
- [ ] **T18** Call `wealth_tx_note` with `transaction_ids: [TEST_ID]`, `value: null` (clear note). Expect success.
- [ ] **T19** Call `wealth_tx_list` with `search: "added by mcp-test agent"`. Verify `TEST_ID` no longer appears.

### Phase 5 — error paths

- [ ] **T20** Call `wealth_tx_tag` with `transaction_ids: []` (empty array). Expect `{"error": "..."}` containing "1–500".
- [ ] **T21** Call `wealth_tx_tag` with a fabricated ID like `txn_doesnotexist`, `op: "add"`, `value: "foo"`. Expect either success (idempotent no-op) or a non-crash error — NOT a server 500.

### Phase 5b — cost basis (requires investment data)

Skip if `unknown_basis_sales` is empty after T9.

- [ ] **T22** From T9's result, find an entry in `unknown_basis_sales` where `user_cost_basis_cents` is `null`. Note its `txn_id` as `TEST_TXN_ID`.
- [ ] **T23** Call `wealth_set_cost_basis` with `txn_id: TEST_TXN_ID`, `cost_basis_cents: 10000`, `is_long_term: false`. Expect `{"status":"success",...}`.
- [ ] **T24** Call `wealth_gains` again (same year). Verify `TEST_TXN_ID`'s entry in `unknown_basis_sales` now has `user_cost_basis_cents: 10000` (entry stays — expected). Verify it also appears in `realized_lots` with `source: "user_input"`.
- [ ] **T25** Call `wealth_delete_cost_basis` with `txn_id: TEST_TXN_ID`. Expect `{"status":"success",...}`.
- [ ] **T26** Call `wealth_gains` again. Verify `TEST_TXN_ID` is back in `unknown_basis_sales` with `user_cost_basis_cents: null`.

### Phase 6 — optional / destructive (skip in read-only environments)

- [ ] **T27** Call `wealth_sync`. Expect `{"status":"success","synced":N}` within 60 seconds.

---

## 8. Common workflows

Recipes for the high-value chained-call patterns. Always start with `wealth_whoami` to confirm token validity.

---

### "Categorize last quarter's unlabeled spending"

```
1. wealth_tx_list({ since: "YYYY-01-01", until: "YYYY-03-31", category: "FOOD_AND_DRINK" })
   → filter items where tags == []
2. Confirm with user which transactions to tag.
3. wealth_tx_tag({ transaction_ids: [...], op: "add", value: "groceries" })
4. wealth_tx_list({ tag: "groceries" }) → verify.
```

---

### "Resolve all unknown-basis sales for a tax year"

```
1. wealth_gains({ year: 2024 })
   → collect unknown_basis_sales[] entries where user_cost_basis_cents == null
2. For each entry: show user symbol, close_date, proceeds; ask for basis + long-term flag.
3. wealth_set_cost_basis({ txn_id, cost_basis_cents, is_long_term }) per entry.
4. wealth_gains({ year: 2024 }) → verify all resolved entries appear in realized_lots.
```

> After step 3, each resolved entry stays in `unknown_basis_sales` with `user_cost_basis_cents` filled in — that's expected, not a bug.

---

### "Import a 1099-B end-to-end" *(requires `wealth_cost_basis_import` — coming in Slice 1)*

```
1. User pastes or uploads their brokerage 1099-B.
2. You extract all realized lots (symbol, quantity, cost_basis, open_date, close_date, proceeds, is_long_term).
3. wealth_cost_basis_import({ lots: [...], source_detail: "Schwab 1099-B 2024 PDF" })
4. wealth_gains({ year: 2024 }) → verify summary matches 1099-B totals.
```

---

### "Refresh data and check new transactions"

```
1. wealth_sync()                           ← only when user asks; blocks 10–30s
2. wealth_tx_list({ since: "today - 7d" }) ← check recent imports
3. wealth_accounts_list()                  ← verify balances updated
```

---

## 9. Known limitations

| Limitation | Detail |
|---|---|
| RSU cost basis | META and TSLA RSU vests come from Plaid as `transfer` type, not `buy`, so they appear in `unknown_basis_sales` with no FIFO cost basis. Use `wealth_set_cost_basis` to enter the basis manually. |
| Schwab history cap | Schwab limits Plaid to ~24 months of investment history regardless of the sync window configured on the backend. |
| `wealth_tx_list` single tag | The `tag` filter matches one tag at a time. For multi-tag AND filtering, make multiple calls and intersect client-side. |
| `has_unknown_basis: true` | Means some buys for that symbol fall outside the brokerage's Plaid history cap — the basis shown is partial, not zero. Does not mean all basis is missing. |
| `unknown_basis_sales` persists after basis set | After `wealth_set_cost_basis`, the entry stays in `unknown_basis_sales` with `user_cost_basis_cents` filled in. It also appears in `realized_lots`. This is by design — don't loop trying to "fix" it. |
| No holdings tool | Investment holdings are not yet exposed via MCP. Use `wealth_gains` for unrealized positions (which includes current value). |
| No `wealth_tx_get` | There is no single-transaction lookup by ID. Use `wealth_tx_list` with `search` or a narrow date range to find a specific transaction. |
| HTTP transport only | This MCP server uses HTTP POST (no SSE/streaming). Server-initiated notifications are not supported, which is fine for tool-only use. |
