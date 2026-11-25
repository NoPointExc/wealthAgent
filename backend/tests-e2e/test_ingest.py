"""End-to-end test of the Muse ingest/export MCP tools against a throwaway stack."""
import base64, hashlib, hmac, json, subprocess, sys, time, urllib.request

BASE = "http://127.0.0.1:18099"
SECRET = "e2e-jwt-secret"
PUBLIC_URL = "http://test.local"
PG = "wa-e2e-pg"

failures = []


def b64(d):
    return base64.urlsafe_b64encode(d).rstrip(b"=").decode()


def access_token(sub, scope):
    header = b64(json.dumps({"alg": "HS256", "typ": "JWT"}).encode())
    claims = b64(json.dumps({
        "sub": sub, "iss": PUBLIC_URL, "aud": PUBLIC_URL + "/mcp", "scope": scope,
        "client_id": "muse-test", "exp": int(time.time()) + 3600, "token_type": "oauth_access",
    }).encode())
    sig = b64(hmac.new(SECRET.encode(), f"{header}.{claims}".encode(), hashlib.sha256).digest())
    return f"{header}.{claims}.{sig}"


TOK = access_token("u1", "read,ingest")
TOK_READ = access_token("u1", "read")
TOK_U2 = access_token("u2", "read,ingest")


def call(tool, args=None, tok=TOK):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                       "params": {"name": tool, "arguments": args or {}}}).encode()
    req = urllib.request.Request(BASE + "/mcp", data=body, method="POST", headers={
        "Content-Type": "application/json", "Authorization": f"Bearer {tok}"})
    with urllib.request.urlopen(req) as r:
        resp = json.load(r)
    text = resp["result"]["content"][0]["text"]
    return json.loads(text)


def sql(q):
    out = subprocess.run(["docker", "exec", PG, "psql", "-U", "wealthagent", "-d", "wealthagent",
                          "-tAc", q], capture_output=True, text=True, check=True).stdout.strip()
    return out


def check(name, cond, detail=""):
    print(("PASS " if cond else "FAIL ") + name + ("" if cond else f"  -> {detail}"))
    if not cond:
        failures.append(name)


# ── tools/list ──────────────────────────────────────────────────────────────
req = urllib.request.Request(BASE + "/mcp", data=json.dumps({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}).encode(),
                             method="POST", headers={"Content-Type": "application/json", "Authorization": f"Bearer {TOK}"})
names = {t["name"] for t in json.load(urllib.request.urlopen(req))["result"]["tools"]}
check("tools/list has new tools", {"wealth_ingest_accounts", "wealth_ingest_transactions", "wealth_ingest_holdings",
       "wealth_ingest_investment_transactions", "wealth_ingest_map", "wealth_ingest_status",
       "wealth_export_transactions", "wealth_export_snapshots", "wealth_export_holdings",
       "wealth_export_investment_transactions"} <= names, names)

ACCOUNTS = {
    "institution_name": "Chase",
    "accounts": [
        {"account_id": "muse_chk", "name": "Total Checking", "mask": "1111", "type": "depository", "subtype": "checking",
         "balances": {"current": 1234.56, "available": 1200.0}},
        {"account_id": "muse_cc", "name": "Sapphire", "mask": "2222", "type": "credit",
         "balances": {"current": 321.09}},
        {"account_id": "muse_brk", "name": "Individual", "mask": "3333", "type": "investment",
         "institution_name": "Fidelity", "balances": {"current": 5100.00}},
        {"account_id": "muse_sav", "name": "Savings", "mask": "4444", "type": "depository",
         "institution_name": "Ally", "balances": {"current": 150.00}},
        {"account_id": "muse_new", "name": "Brand New", "mask": "9999", "type": "depository",
         "balances": {"current": 10.00}, "extra_unknown_field": {"ignored": True}},
    ],
}

# ── scope gate ──────────────────────────────────────────────────────────────
r = call("wealth_ingest_accounts", ACCOUNTS, tok=TOK_READ)
check("read-only token cannot ingest", "error" in r and "ingest" in r["error"], r)
r = call("wealth_sync")
check("wealth_sync blocked by scope for Muse token", "error" in r, r)

# ── accounts: dry run ───────────────────────────────────────────────────────
r = call("wealth_ingest_accounts", {**ACCOUNTS, "dry_run": True})
hows = {a["external_id"]: a["how"] for a in r.get("accounts", [])}
check("dry run: checking matched by mask", hows.get("muse_chk") == "matched_by_mask", r)
check("dry run: credit card matched by mask", hows.get("muse_cc") == "matched_by_mask", r)
check("dry run: brokerage matched by mask", hows.get("muse_brk") == "matched_by_mask", r)
check("dry run: new account would be created", hows.get("muse_new") == "created", r)
check("dry run: ambiguous savings skipped", any(s["id"] == "muse_sav" and "ambiguous" in s["reason"] for s in r.get("skips", [])), r)
check("dry run flag echoed", r.get("dry_run") is True, r)
check("dry run wrote nothing", sql("SELECT count(*) FROM external_id_map") == "0"
      and sql("SELECT balance FROM accounts WHERE id='old_chk'") == "100000"
      and sql("SELECT count(*) FROM accounts WHERE id='muse_new'") == "0")
check("dry run logged", sql("SELECT count(*) FROM ingest_log WHERE dry_run") == "1")

# dry-run transactions before any account link exists: without accounts every row is unknown...
DRY_TX = {"added": [{"transaction_id": "m1", "account_id": "muse_chk", "amount": 12.50, "date": "2026-11-10",
                     "name": "STARBUCKS 123", "pending": False}], "dry_run": True}
r = call("wealth_ingest_transactions", DRY_TX)
check("dry tx without accounts: unknown account", r.get("skipped") == 1, r)
# ...with the bank's accounts in the same call, it matches like a live push would
r = call("wealth_ingest_transactions", {**DRY_TX, "accounts": ACCOUNTS["accounts"], "institution_name": "Chase"})
check("dry tx with accounts prelude matches history",
      any(x["external_id"] == "m1" and x["internal_id"] == "t_old1" for x in r.get("matches", [])) and r.get("skipped") == 0
      and any("accounts step" in n for n in r.get("notes", [])), r)
check("prelude reports ambiguous account", any(s["id"] == "muse_sav" and s["reason"].startswith("account:") for s in r.get("skips", [])), r)
check("prelude dry run wrote nothing", sql("SELECT count(*) FROM external_id_map") == "0")
r = call("wealth_ingest_holdings", {"holdings": [{"account_id": "muse_brk", "security_id": "s", "institution_value": 1}],
                                    "accounts": ACCOUNTS["accounts"], "dry_run": True})
check("dry holdings with accounts prelude", r.get("inserted") == 1 and r.get("skipped") == 0, r)

# ── manual link for the ambiguous one, then real push ───────────────────────
r = call("wealth_ingest_map", {"action": "set", "kind": "account", "external_id": "muse_sav", "internal_id": "old_sav2"})
check("map set ambiguous savings", r.get("status") == "success", r)
r = call("wealth_ingest_map", {"action": "set", "kind": "account", "external_id": "x", "internal_id": "foreign_acc"})
check("map set refuses foreign row", "error" in r, r)

r = call("wealth_ingest_accounts", ACCOUNTS)
hows = {a["external_id"]: a for a in r.get("accounts", [])}
check("push: all 5 accounts handled", r.get("inserted") == 1 and r.get("updated") == 4 and r.get("skipped") == 0, r)
check("push: savings uses manual link", hows.get("muse_sav", {}).get("internal_id") == "old_sav2", r)
check("checking balance in cents", sql("SELECT balance FROM accounts WHERE id='old_chk'") == "123456")
check("credit card stored negative", sql("SELECT balance FROM accounts WHERE id='old_cc'") == "-32109")
check("cash holding updated", sql("SELECT asset_value FROM holdings WHERE security_id='CASH_old_chk'") == "123456")
check("snapshot written for today", sql("SELECT count(*) FROM account_snapshots WHERE account_id='old_chk' AND snapshot_date=(now() AT TIME ZONE 'America/New_York')::date") == "1")
check("new account under muse item", sql("SELECT pi.source || '/' || a.name FROM accounts a JOIN plaid_items pi ON a.plaid_item_id=pi.id WHERE a.id='muse_new'") == "muse/Chase Brand New - 9999")
check("muse item has no token", sql("SELECT access_token_enc IS NULL FROM plaid_items WHERE source='muse'") == "t")

# backfill an older date: snapshot only, balance untouched
r = call("wealth_ingest_accounts", {"accounts": [ACCOUNTS["accounts"][0] | {"balances": {"current": 1.0}}], "as_of": "2026-11-11"})
check("backfill as_of adds snapshot", sql("SELECT balance_cents FROM account_snapshots WHERE account_id='old_chk' AND snapshot_date='2026-11-11'") == "100")
check("backfill does not rewind balance", sql("SELECT balance FROM accounts WHERE id='old_chk'") == "123456")

# foreign ownership: u2 pushing u1's ids
r = call("wealth_ingest_accounts", {"accounts": [{"account_id": "old_chk", "name": "x", "type": "depository", "balances": {"current": 1}}]}, tok=TOK_U2)
check("other user cannot overwrite account", r.get("skipped") == 1 and sql("SELECT balance FROM accounts WHERE id='old_chk'") == "123456", r)

# ── transactions ────────────────────────────────────────────────────────────
TX1 = {
    "added": [
        {"transaction_id": "m1", "account_id": "muse_chk", "amount": 12.50, "date": "2026-11-10", "name": "STARBUCKS 123",
         "merchant_name": "Starbucks", "pending": False, "personal_finance_category": {"primary": "FOOD_AND_DRINK", "detailed": "FOOD_AND_DRINK_COFFEE"}},
        {"transaction_id": "m2", "account_id": "muse_chk", "amount": 45.99, "date": "2026-11-13", "name": "AMAZON MKTPL",
         "pending": False, "pending_transaction_id": "never_seen"},
        {"transaction_id": "m3", "account_id": "muse_chk", "amount": 80.00, "date": "2026-11-19", "name": "SHELL OIL", "pending": False},
        {"transaction_id": "m4", "account_id": "muse_chk", "amount": 5.00, "date": "2026-11-12", "name": "STARBUCKS 123", "pending": False},
        {"transaction_id": "mp1", "account_id": "muse_chk", "amount": 20.00, "date": "2026-11-19", "name": "UBER PENDING", "pending": True},
        {"transaction_id": "m_unknown_acct", "account_id": "nope", "amount": 1, "date": "2026-11-19", "name": "X"},
        {"transaction_id": "foreign_txn", "account_id": "muse_chk", "amount": 1, "date": "2026-11-11", "name": "X"},
        {"transaction_id": "m_bad_date", "account_id": "muse_chk", "amount": 1, "date": "11/19/2026", "name": "X"},
    ],
}
before = sql("SELECT count(*) FROM transactions")
r = call("wealth_ingest_transactions", TX1)
m = {x["external_id"]: (x["internal_id"], x["how"]) for x in r.get("matches", [])}
check("tx: starbucks fuzzy-matched to old row", m.get("m1") == ("t_old1", "fuzzy"), r)
check("tx: pending amazon matched across dates", m.get("m2") == ("t_old2", "fuzzy"), r)
check("tx: second starbucks matched to the other row", m.get("m4") == ("t_old3", "fuzzy"), r)
check("tx: 2 created, 3 updated, 3 skipped", (r.get("inserted"), r.get("updated"), r.get("skipped")) == (2, 3, 3), r)
check("tx: tags + note preserved on matched row", sql("SELECT tags || '|' || note FROM transactions WHERE id='t_old1'") == "coffee|client meeting")
check("tx: category filled", sql("SELECT plaid_primary_category FROM transactions WHERE id='t_old1'") == "FOOD_AND_DRINK")
check("tx: amazon now posted with posted date", sql("SELECT pending::text || ' ' || txn_date FROM transactions WHERE id='t_old2'") == "false 2026-11-13")
check("tx: foreign row untouched", sql("SELECT raw_string FROM transactions WHERE id='foreign_txn'") == "OTHER")
check("tx: row count +2", int(sql("SELECT count(*) FROM transactions")) == int(before) + 2)

# idempotent re-push
r = call("wealth_ingest_transactions", TX1)
check("tx re-push is idempotent", r.get("inserted") == 0 and r.get("matched") == 0 and int(sql("SELECT count(*) FROM transactions")) == int(before) + 2, r)

# pending -> posted with tags carried over; removed pending id must not delete the posted row
sql("UPDATE transactions SET tags='work' WHERE id='mp1'")
r = call("wealth_ingest_transactions", {
    "added": [{"transaction_id": "m5", "account_id": "muse_chk", "amount": 21.00, "date": "2026-11-20",
               "name": "UBER TRIP", "pending": False, "pending_transaction_id": "mp1"}],
    "removed": [{"transaction_id": "mp1"}, "never_existed"],
})
check("pending->posted reuses the pending row", any(x["external_id"] == "m5" and x["internal_id"] == "mp1" and x["how"] == "pending_posted" for x in r.get("matches", [])), r)
check("posted row kept tags, updated amount", sql("SELECT tags || ' ' || amount || ' ' || pending::text FROM transactions WHERE id='mp1'") == "work 2100 false")
check("removed pending id did not delete posted row", r.get("removed") == 0 and sql("SELECT count(*) FROM transactions WHERE id='mp1'") == "1", r)
r = call("wealth_ingest_transactions", {"modified": [{"transaction_id": "mp1", "account_id": "muse_chk", "amount": 1, "date": "2026-11-20", "name": "X"}]})
check("superseded pending id is not resurrected", r.get("skipped") == 1, r)

# Plaid re-issued a bank transaction id: removed(old) + added(new) in one sync -> new id takes over the row, tags kept
sql("UPDATE transactions SET tags='latte' WHERE id='t_old3'")
before_tx = sql("SELECT count(*) FROM transactions")
r = call("wealth_ingest_transactions", {"added": [{"transaction_id": "m4_v2", "account_id": "muse_chk", "amount": 5.00,
          "date": "2026-11-12", "name": "STARBUCKS 123", "pending": False}], "removed": ["m4"]})
check("bank id change takes over row", any(x["external_id"] == "m4_v2" and x["internal_id"] == "t_old3" for x in r.get("matches", [])) and r.get("removed") == 0, r)
check("bank id change keeps tags and count", sql("SELECT tags FROM transactions WHERE id='t_old3'") == "latte" and sql("SELECT count(*) FROM transactions") == before_tx)

# real removal
r = call("wealth_ingest_transactions", {"removed": ["m3"]})
check("removed deletes row + link", r.get("removed") == 1 and sql("SELECT count(*) FROM transactions WHERE id='m3'") == "0"
      and sql("SELECT count(*) FROM external_id_map WHERE internal_id='m3'") == "0", r)

# ── investment transactions ─────────────────────────────────────────────────
r = call("wealth_ingest_investment_transactions", {
    "investment_transactions": [
        {"investment_transaction_id": "mi_sell", "account_id": "muse_brk", "security_id": "sec_tsla", "date": "2026-10-01",
         "name": "SELL TSLA", "quantity": -2, "amount": -600.00, "price": 300.0, "fees": 0, "type": "sell", "subtype": "sell"},
        {"investment_transaction_id": "mi_new", "account_id": "muse_brk", "security_id": "sec_aapl", "date": "2026-11-15",
         "name": "BUY AAPL", "quantity": 1, "amount": 230.00, "price": 230.0, "type": "buy", "subtype": "buy"},
    ],
    "securities": [{"security_id": "sec_tsla", "ticker_symbol": "TSLA", "name": "Tesla Inc"},
                   {"security_id": "sec_aapl", "ticker_symbol": "AAPL", "name": "Apple Inc"}],
})
check("inv: sell fuzzy-matched to old row", any(x["external_id"] == "mi_sell" and x["internal_id"] == "old_inv_sell" for x in r.get("matches", [])), r)
check("inv: new buy created with cents", r.get("inserted") == 1 and sql("SELECT amount || ' ' || price || ' ' || symbol FROM investment_transactions WHERE id='mi_new'") == "23000 23000 AAPL", r)
check("inv: cost-basis override still attached", sql("SELECT count(*) FROM cost_basis_overrides WHERE investment_transaction_id='old_inv_sell'") == "1")
r = call("wealth_gains", {"year": 2026})
check("capital gains still computes", "error" not in r, r)

# Plaid re-issued an investment id: same sell under a new id, old id absent -> relink, no new row, override kept
before_inv = sql("SELECT count(*) FROM investment_transactions")
SELL_V2 = {"investment_transaction_id": "mi_sell_v2", "account_id": "muse_brk", "security_id": "sec_tsla", "date": "2026-10-01",
           "name": "SELL TSLA", "quantity": -2, "amount": -600.00, "price": 300.0, "fees": 0, "type": "sell", "subtype": "sell"}
SECS = [{"security_id": "sec_tsla", "ticker_symbol": "TSLA", "name": "Tesla Inc"}]
r = call("wealth_ingest_investment_transactions", {"investment_transactions": [SELL_V2], "securities": SECS})
check("inv id change relinks existing row", any(x["external_id"] == "mi_sell_v2" and x["internal_id"] == "old_inv_sell" and x["how"] == "id_changed" for x in r.get("matches", [])), r)
check("inv id change: no new row, override kept", sql("SELECT count(*) FROM investment_transactions") == before_inv
      and sql("SELECT count(*) FROM cost_basis_overrides WHERE investment_transaction_id='old_inv_sell'") == "1"
      and sql("SELECT count(*) FROM external_id_map WHERE external_id='mi_sell'") == "0")
# two genuinely identical trades in one push stay two rows
TWIN = {"investment_transaction_id": "mi_twin_a", "account_id": "muse_brk", "security_id": "sec_tsla", "date": "2026-11-18",
        "name": "BUY TSLA", "quantity": 1, "amount": 300.00, "price": 300.0, "type": "buy", "subtype": "buy"}
r = call("wealth_ingest_investment_transactions", {"investment_transactions": [TWIN, TWIN | {"investment_transaction_id": "mi_twin_b"}], "securities": SECS})
check("identical trades in one push both kept", r.get("inserted") == 2, r)
r = call("wealth_ingest_investment_transactions", {"investment_transactions": [TWIN, TWIN | {"investment_transaction_id": "mi_twin_b"}], "securities": SECS})
check("identical trades re-push is idempotent", r.get("inserted") == 0 and r.get("matched") == 0, r)

# ── holdings ────────────────────────────────────────────────────────────────
r = call("wealth_ingest_holdings", {
    "holdings": [{"account_id": "muse_brk", "security_id": "sec_aapl", "institution_value": 3450.00, "quantity": 15}],
    "securities": [{"security_id": "sec_aapl", "ticker_symbol": "AAPL", "name": "Apple Inc"}],
})
acct = (r.get("accounts") or [{}])[0]
check("holdings replaced (TSLA sold off)", sql("SELECT string_agg(security_id, ',' ORDER BY security_id) FROM holdings WHERE account_id='old_brk'") == "UNALLOCATED_old_brk,sec_aapl", r)
check("holdings unallocated = balance - positions", acct.get("unallocated") == 510000 - 345000, r)
check("cash row on checking untouched", sql("SELECT count(*) FROM holdings WHERE security_id='CASH_old_chk'") == "1")

# ── exports ─────────────────────────────────────────────────────────────────
r = call("wealth_export_transactions", {"account_id": "muse_chk", "since": "2026-11-01"})
ext = {i["id"]: i.get("external_id") for i in r.get("items", [])}
check("export tx by external account id", r.get("total", 0) >= 4 and ext.get("t_old1") == "m1", r)
check("export tx has external_account_id", all(i.get("external_account_id") == "muse_chk" for i in r.get("items", [])), r)
check("export tx tags as array", any(i["tags"] == ["coffee"] for i in r.get("items", [])), r)
r = call("wealth_export_snapshots", {"account_id": "old_chk"})
check("export snapshots incl. history", r.get("total") == 4, r)
r = call("wealth_export_holdings", {"account_id": "muse_brk"})
check("export holdings", r.get("total") == 2, r)
r = call("wealth_export_investment_transactions", {"since": "2026-01-01"})
check("export inv txns with external ids", r.get("total") == 5 and any(i.get("id") == "old_inv_sell" and i.get("external_id") == "mi_sell_v2" for i in r["items"]), r)
r = call("wealth_export_transactions", {"account_id": "old_chk"}, tok=TOK_U2)
check("other user's export sees nothing", r.get("total") == 0, r)
r = call("wealth_export_transactions", {"since": "bad"})
check("bad date rejected", "error" in r, r)

# ── status, map list, existing tools ────────────────────────────────────────
r = call("wealth_ingest_status")
check("status reports plaid off + pushes", r.get("plaid_sync_enabled") is False and len(r.get("recent_pushes", [])) > 5, r)
r = call("wealth_ingest_map", {"action": "list", "kind": "account"})
check("map list accounts", r.get("total") == 5, r)
r = call("wealth_accounts_list")
check("accounts_list includes muse-created account", any(a["id"] == "muse_new" for a in r), r)

ALL = access_token("u1", "read,write,sync,ingest")
r = call("wealth_sync", tok=ALL)
check("wealth_sync reports Plaid disabled", "error" in r and "turned off" in r["error"], r)

print()
print(f"{len(failures)} failure(s)" if failures else "ALL PASSED")
sys.exit(1 if failures else 0)
