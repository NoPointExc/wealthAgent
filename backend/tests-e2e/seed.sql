-- Two users. u1 has "old Plaid connection" history; u2 owns a foreign account.
INSERT INTO users (id, google_id, email, name) VALUES
  ('u1', 'g1', 'owner@test.local', 'Owner'),
  ('u2', 'g2', 'other@test.local', 'Other');

INSERT INTO plaid_items (id, user_id, access_token_enc) VALUES
  ('item_old', 'u1', '\x00'::bytea),
  ('item_u2',  'u2', '\x00'::bytea);

INSERT INTO accounts (id, name, balance, trend_pct, plaid_item_id, account_type) VALUES
  ('old_chk',  'Chase Total Checking - 1111', 100000, 0, 'item_old', 'asset'),
  ('old_cc',   'Chase Sapphire - 2222',       -50000, 0, 'item_old', 'liability'),
  ('old_brk',  'Fidelity Individual - 3333',  500000, 0, 'item_old', 'asset'),
  ('old_sav1', 'Ally Savings - 4444',          10000, 0, 'item_old', 'asset'),
  ('old_sav2', 'Ally Joint Savings - 4444',    20000, 0, 'item_old', 'asset'),
  ('foreign_acc', 'Other Bank - 1111',          7000, 0, 'item_u2', 'asset');

INSERT INTO account_snapshots (account_id, snapshot_date, balance_cents) VALUES
  ('old_chk', '2026-11-10', 90000),
  ('old_chk', '2026-11-18', 100000);

INSERT INTO holdings (symbol, account_id, name, asset_value, return_str, performance_type, security_id, quantity) VALUES
  ('CASH', 'old_chk', 'Cash Balance', 100000, '0%', 'neu', 'CASH_old_chk', NULL),
  ('AAPL', 'old_brk', 'Apple Inc', 300000, 'Real-time', 'pos', 'sec_old_aapl', 10),
  ('TSLA', 'old_brk', 'Tesla Inc', 200000, 'Real-time', 'pos', 'sec_old_tsla', 5);

INSERT INTO transactions (id, account_id, txn_date, raw_string, merchant_name, amount, pending, tags, note) VALUES
  ('t_old1', 'old_chk', '2026-11-10', 'STARBUCKS 123', 'Starbucks', 1250, false, 'coffee', 'client meeting'),
  ('t_old2', 'old_chk', '2026-11-12', 'AMAZON MKTPL',  'Amazon',    4599, true,  'shopping', NULL),
  ('t_old3', 'old_chk', '2026-11-12', 'STARBUCKS 123', 'Starbucks',  500, false, NULL, NULL),
  ('foreign_txn', 'foreign_acc', '2026-11-11', 'OTHER', NULL, 100, false, NULL, NULL);

INSERT INTO investment_transactions (id, account_id, date, name, amount, quantity, price, fees, txn_type, subtype, symbol, security_name) VALUES
  ('old_inv_buy',  'old_brk', '2026-03-02', 'BUY TSLA',  100000, 5,  20000, 0, 'buy',  'buy',  'TSLA', 'Tesla Inc'),
  ('old_inv_sell', 'old_brk', '2026-10-01', 'SELL TSLA', -60000, -2, 30000, 0, 'sell', 'sell', 'TSLA', 'Tesla Inc');

INSERT INTO cost_basis_overrides (user_id, investment_transaction_id, cost_basis_cents, is_long_term)
  VALUES ('u1', 'old_inv_sell', 40000, false);
