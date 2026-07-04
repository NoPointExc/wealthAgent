export interface Account {
  id: string;
  name: string;
  custom_name?: string;
  account_type: 'asset' | 'liability';
  balance: number; // Cents
  trend_pct: number | null; // null = fewer than 2 snapshots, show N/A
}

export interface HistoricalPoint {
  account_id: string;
  month_idx: number;
  value: number;
}

export interface Holding {
  symbol: string;
  account_id: string;
  name: string;
  asset_value: number;
  return_str: string;
  performance_type: string;
}

export interface Transaction {
  id: string;
  account_id: string;
  account_name: string;
  txn_date: string;
  raw_string: string;
  merchant_name?: string;
  amount: number; // Cents
  pending: boolean;
  payment_channel?: string;
  plaid_category?: string;
  plaid_primary_category?: string;
  plaid_detailed_category?: string;
  tags?: string; // Comma-separated list of tags
  note?: string;
}

export interface SavedSearch {
  id: number;
  name: string;
  search_query?: string;
  selected_account?: string;
  selected_categories?: string; // Serialized string[]
  selected_tags?: string; // Serialized string[]
  start_date?: string;
  end_date?: string;
  amount_mode?: string;
  amount_val?: string;
  amount_min?: string;
  amount_max?: string;
  show_pending_only: boolean;
}

export interface CreateSavedSearchRequest {
  name: string;
  search_query?: string;
  selected_account?: string;
  selected_categories?: string; // Serialized string[]
  selected_tags?: string; // Serialized string[]
  start_date?: string;
  end_date?: string;
  amount_mode?: string;
  amount_val?: string;
  amount_min?: string;
  amount_max?: string;
  show_pending_only: boolean;
}

export interface AggregateRequest {
  account_ids: string[];
  time_range: {
    start_date: string;
    end_date: string;
    interval: string;
  };
}

export interface AggregateData {
  combined_history: TimelinePoint[];
  account_histories: Record<string, TimelinePoint[]>;
  holdings_breakdown: Record<string, Holding[]>;
  snapshot_counts: Record<string, number>; // days of real history per account
}

export interface TimelinePoint {
  date: string;
  value: number;
}

export interface SuccessResponse<T> {
  status: string;
  data: T;
}

export interface TransactionPage {
  items: Transaction[];
  total: number;
  offset: number;
}

export interface InvestmentTransaction {
  id: string;
  account_id: string;
  account_name: string;
  date: string;
  name: string;
  amount: number;        // cents
  quantity: number | null;
  price: number | null;  // cents per share
  fees: number | null;   // cents
  txn_type: string;
  subtype: string | null;
  symbol: string | null;
  security_name: string | null;
}

export interface InvestmentTransactionPage {
  items: InvestmentTransaction[];
  total: number;
  offset: number;
}

export interface RealizedLot {
  symbol: string | null;
  security_name: string | null;
  open_date: string;
  close_date: string;
  quantity: number;
  cost_basis_cents: number;
  proceeds_cents: number;
  gain_cents: number;
  is_long_term: boolean;
  source: string; // "fifo" | "user_input" | "1099b"
  txn_id?: string | null; // set only for user_input lots → enables edit/clear
}

export interface UnrealizedPosition {
  symbol: string | null;
  security_name: string | null;
  oldest_lot_date: string;
  quantity: number;
  cost_basis_cents: number;
  current_value_cents: number | null;
  gain_cents: number | null;
  is_long_term_if_sold_today: boolean;
  has_unknown_basis: boolean;
}

export interface CapitalGainsSummary {
  ytd_realized_gain_cents: number;
  ytd_realized_loss_cents: number;
  ytd_net_cents: number;
  unrealized_gain_cents: number | null;
  unrealized_loss_cents: number | null;
  short_term_net_cents: number;
  long_term_net_cents: number;
}

export interface UnknownBasisSale {
  symbol: string | null;
  security_name: string | null;
  close_date: string;
  quantity: number;
  proceeds_cents: number;
  txn_id: string;
  user_cost_basis_cents: number | null;
  user_is_long_term: boolean | null;
}

export interface CapitalGainsReport {
  realized_lots: RealizedLot[];
  unknown_basis_sales: UnknownBasisSale[];
  unrealized_positions: UnrealizedPosition[];
  summary: CapitalGainsSummary;
  tax_year: number;
}

export interface ApiToken {
  id: string;
  name: string;
  scopes: string;
  created_at: string;
  last_used_at: string | null;
}

export interface CreateTokenResponse {
  id: string;
  token: string; // raw token shown ONCE
  name: string;
  scopes: string;
}

/** An active OAuth connection (a client the user authorized via sign-in). */
export interface OAuthGrant {
  client_id: string;
  client_name: string;
  scopes: string;
  created_at: string;
  last_used_at: string | null;
}

export interface WhoamiResponse {
  user_id: string;
  email: string;
  name: string;
  scopes: string;
}
