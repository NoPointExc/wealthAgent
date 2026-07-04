# Anomaly Payments — Agent Skill Reference

> **skill.md version:** 1.0 · 2026-06-19
> **Depends on:** [`../wealthAgent/skill.md`](../wealthAgent/skill.md) (WealthAgent MCP)
> **Purpose:** Find junk subscriptions, ghost recurring charges, and FTC-style "intercepted bill-pay" fees in a user's transaction history. Surface them for the user to review — never auto-cancel.

---

## 1. Mission

The user wants you to scan their bank/credit-card transactions and find charges they probably did not intentionally sign up for, or that match the pattern of FTC-flagged "junk fee" schemes (e.g. **doxoPLUS**, see [FTC v. doxo, 2024-04](https://www.ftc.gov/news-events/news/press-releases/2024/04/ftc-takes-action-against-bill-payment-company-doxo-misleading-consumers-tacking-millions-junk-fees)).

The output is a **tiered report** the user can act on, not an automated cancellation. You are advisory.

---

## 2. Definitions

| Tier | Meaning | Examples |
|---|---|---|
| **Tier 1 — junk** | Matches an FTC-named scheme or an explicit "bill-pay-intercept" pattern. High confidence the user did not knowingly subscribe. | `doxo*`, `doxoPLUS`, `Plastiq` surcharge on a bill the user pays directly, post-checkout "Webloyalty / Complete Savings / Reservation Rewards" enrolments. |
| **Tier 2 — avoidable bank/card junk** | Real fees from the user's own bank that are usually waivable. | Monthly savings-account service fee, foreign-transaction fee on a domestic transaction, "paper statement" fees, dormant-account fees. |
| **Tier 3 — verify** | Looks like a low-dollar recurring charge that may be a forgotten free-trial auto-renewal or a Prime-Video channel add-on. Could be wanted. | `Amazon Prime $5.40/mo` while the user already paid the $150 annual; tiny `Apple.com/Bill` or `Google *YouTube` outside known apps; `Audible`, `Patreon`. |
| **Tier 4 — clean** | Recurring but plausibly intentional. Mention only briefly. | Gym, kids' enrichment programs, pet autoship, eyewear refills, phone bill. |

**Do not** put a real service into Tier 1 just because it recurs. The bar for Tier 1 is *"this charge exists to extract money the user did not knowingly authorize."*

---

## 3. Data conventions (from WealthAgent MCP)

Read [`../wealthAgent/skill.md`](../wealthAgent/skill.md) §4.3 for the full `wealth_tx_list` schema. Key conventions for this skill:

- **Sign convention:** `amount > 0` = money leaving the account (debit). `amount < 0` = money entering (credit, refund, payroll). All scans here filter `amount > 0`.
- **Cents:** every `amount` is integer cents.
- **Merchant key:** prefer `merchant_name`; fall back to `raw_string`.
- **Pending:** scan both — junk fees can sit pending for days.
- **Categories worth pulling in full:**
  - `BANK_FEES` — every row is a candidate.
  - `GENERAL_SERVICES_OTHER_GENERAL_SERVICES` — Plaid's catch-all where doxo, fee-aggregator, and many ghost-subs land.
  - `ENTERTAINMENT_VIDEO_GAMES`, `ENTERTAINMENT_TV_AND_MOVIES` — frequent home of forgotten trials.

---

## 4. Method

Follow these steps in order. Stop only when the user asks you to.

### 4.1 Pull a 12–14 month window

```text
wealth_tx_list({ since: "<today - 14mo>", until: "<today>", limit: 200, offset: 0 })
```

Page until `offset >= total`. Cap at ~2000 rows. If `total` > 2000, sample the most recent 14 months — junk fees almost always continue into the present.

### 4.2 Known-bad-actor scan (Tier 1)

Match `merchant_name + " " + raw_string` (case-insensitive) against this regex. **Anything that hits is Tier 1.**

```
doxo|doxoPLUS|plastiq.*fee|webloyalty|complete savings|reservation rewards|
trilegiant|simple escapes|hotwireloyalty|safekey|identityguard|lifelock|
experian credit|equifax credit|myfico premium|reputation defender|
spokeo|whitepages premium|peoplelookup
```

Maintain this list. New bad actors appear; FTC enforcement actions are the best source — when the user (or a search) names a new one, add it here.

**Special case — doxo:**
- `doxo*<biller name>` *is* legitimate when the user actually uses doxo to pay a biller that doesn't take cards directly.
- `dox*Bill Pay - doxoPLUS` is **always** Tier 1 — it is the membership upsell, not a payment.
- If you see both patterns for the same user, flag only the `doxoPLUS` rows, but note that the user is paying doxo's convenience fee on the biller rows too.

### 4.3 Bank-fee scan (Tier 2)

Pull every row with `plaid_primary_category == "BANK_FEES"`. Bucket by `raw_string`:

| `raw_string` contains | Action |
|---|---|
| `MONTHLY SERVICE FEE`, `MONTHLY MAINTENANCE` | Tier 2. Note the bank and which account. Most banks waive these with a min balance or auto-transfer. |
| `ANNUAL MEMBERSHIP FEE` | Tier 2 *info only* — it's a real card fee. Ask the user whether the rewards outpace the fee. |
| `OVERDRAFT`, `RETURNED ITEM`, `NSF` | Tier 2. These are recoverable: most banks refund 1–2/yr on request. |
| `FOREIGN TRANSACTION FEE` on a card that should waive them | Tier 2. Possible mis-categorisation by Plaid; verify. |
| `WIRE FEE`, `STOP PAYMENT FEE` | Tier 4 unless recurring — usually intentional. |
| `INTEREST CHARGE`, `FEDERAL INTEREST WITHHELD` | **Not a fee.** Skip. (Tax withholding on interest income.) |

### 4.4 Recurring-charge clustering (Tier 3 candidates)

Group all `amount > 0 AND amount < $200` rows by `(merchant_name OR raw_string)` and select clusters where:

- `count >= 3`
- distinct amounts ≤ 2 (allows for one mid-period price hike)
- `last_date` is within the last 90 days (still active)

For each cluster:

1. If amount is plausibly a **streaming/channel/add-on** ($1.99–$14.99, monthly cadence, merchant is Apple/Google/Amazon/Roku) → Tier 3. Ask the user to open the app/account's "subscriptions" page.
2. If amount is a **clean monthly** $10–$80 charge to a merchant the user has *not* mentioned in recent natural-language context → Tier 3.
3. If merchant matches an obvious real-life service (gym, daycare, phone carrier, insurance, pet autoship) → Tier 4.

### 4.5 Duplicate-charge scan

Find pairs of rows where:
- same `merchant_name`
- same `amount`
- `txn_date` within 1 day of each other
- not both `pending` (one cleared, one pending is fine — that's just settlement)

These can be genuine double-charges. Report them with both `id`s so the user can dispute one.

### 4.6 Sub-$2 micro-charge scan

A `$0.01`–`$1.99` charge from an unfamiliar merchant is often a **stolen-card test charge** before a larger fraud attempt. Pull all rows with `amount > 0 AND amount < 200` and a merchant the user hasn't transacted with before. Flag the user immediately if any cleared in the last 7 days.

---

## 5. Output format

Always produce a Markdown report with exactly these sections, in order. Skip empty sections — don't pad.

```markdown
## Tier 1 — Real junk (cancel now)

| Charge (raw_string) | Amount | Frequency | Total bled | Account | First seen |

…with one row per distinct merchant. Include the FTC link or other authority if relevant.
Tell the user how to cancel and whether to dispute prior charges.

## Tier 2 — Avoidable bank/card junk

| Charge | Amount | When | How to fix |

## Tier 3 — Verify

| Charge | Pattern | Why I flagged it |

End each Tier-3 row with a concrete check the user can run in 60 seconds
(open amazon.com → Memberships, log into the carrier app, etc.).

## Tier 4 — Clean (informational)

Brief paragraph; don't list every row.

## What I did not find

Single sentence ruling out the categories of junk you searched for but did
not see (LifeLock, Truebill, post-purchase upsells, etc.). This is signal:
the user learns their footprint is unusually clean in those areas.

## Bottom line

One sentence with the dollar amount of confirmed Tier 1 + Tier 2 annual bleed.
If you're confident a 60-second action recovers most of it, say so.
```

---

## 6. Hard rules

- **Never auto-cancel** anything. You are not authorised to call merchant APIs or write to the bank. Surface, don't act.
- **Never list a merchant in Tier 1 without naming the specific pattern** ("matches FTC-flagged doxoPLUS scheme") or the user will rightly distrust the report.
- **Never dispute on the user's behalf.** The cancel/dispute steps go in the report as instructions for them.
- **Respect privacy.** Don't paste raw transactions into external systems. Summarise.
- **Only flag merchants that recur or that match a known bad-actor pattern.** A one-off $30 charge to a merchant you don't recognise is not anomalous — it's lunch.

---

## 7. Reference run — the doxoPLUS case (2026-06)

Concrete example of what a good Tier 1 entry looks like.

```
Search: wealth_tx_list({ search: "doxo", limit: 50 })
Hits:   22 rows, 2024-09-20 through 2026-06-18
        all $6.39, raw_string = "dox*Bill Pay - doxoPLUS"
        account = Chase CC ·7908
        plaid_detailed_category = GENERAL_SERVICES_OTHER_GENERAL_SERVICES

Report row:
| `dox*Bill Pay - doxoPLUS` | $6.39 | monthly, 22 charges | $140.58 | Chase CC ·7908 | 2024-09-20 |

Action text:
"Matches the doxoPLUS membership called out in the FTC's April 2024 action
 against doxo. Cancel at doxo.com → Account → doxoPLUS. Then dispute the
 22 prior charges with Chase as unauthorised recurring — the FTC ruling
 supports the dispute."
```

---

## 8. Test cases

Use these to sanity-check changes to this skill. Run against a user with at least 12 months of transactions.

- [ ] **A1** Find `doxoPLUS` if it exists. Must land in Tier 1 with the exact action text from §7.
- [ ] **A2** Find a recurring `MONTHLY SERVICE FEE` from a bank account. Must land in Tier 2 with a waiver suggestion specific to that bank.
- [ ] **A3** Recurring `ANNUAL MEMBERSHIP FEE` on a credit card. Must land in Tier 2 *info only* (don't tell the user to cancel a card they may want).
- [ ] **A4** A user-known recurring service (e.g. gym, kids' class). Must land in Tier 4, not Tier 1.
- [ ] **A5** A possible duplicate charge (same merchant, same amount, ≤ 1 day apart). Must appear in Tier 3 with both transaction IDs.
- [ ] **A6** A cleared sub-$2 charge from an unknown merchant in the last 7 days. Must be flagged urgently — possible card-test fraud.
- [ ] **A7** `FEDERAL INTEREST WITHHELD` rows must **not** appear in any tier. They are tax withholding on interest income, not a fee.

---

## 9. Known limitations

| Limitation | Implication |
|---|---|
| Plaid's `merchant_name` is sometimes wrong (e.g. doxo shows up as `merchant_name: "DOX"` on a hospital payment row). | Always also match against `raw_string` — don't trust merchant alone. |
| `wealth_tx_list` `search` matches substring across merchant, raw, and note. It does not regex. | For regex/keyword fan-out, pull a wide window and filter in your own analysis pass. |
| No `wealth_tx_get` single-row lookup. | When reporting a duplicate, include the `id` from the cluster scan; the user can find it in the app. |
| No bulk-tag in MCP yet. | If the user asks you to tag every doxoPLUS row, loop `wealth_tx_tag({ txn_id, op: "add", tags: ["junk"] })` over the cluster. |

---

## 10. Changelog

- **v1.0 (2026-06-19):** Initial. Built on the doxoPLUS case found in a real user's transactions.
