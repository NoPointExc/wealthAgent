import type { Account, AggregateData, SuccessResponse, TransactionPage, InvestmentTransactionPage, SavedSearch, CreateSavedSearchRequest, CapitalGainsReport, ApiToken, CreateTokenResponse, OAuthGrant, WhoamiResponse } from '../types/models';

const API_BASE = (import.meta.env.VITE_API_BASE_URL as string | undefined) ?? '/api';

/** Thrown by googleLogin when the account must accept the current Terms/Privacy
 *  before a session is issued (HTTP 428). The login view catches this to reveal
 *  the one-time consent step. */
export class ConsentRequiredError extends Error {
  constructor(message = 'Consent required') {
    super(message);
    this.name = 'ConsentRequiredError';
  }
}

async function apiFetch(url: string, options: RequestInit = {}): Promise<Response> {
  const res = await fetch(url, {
    ...options,
    credentials: 'include',
    headers: {
      ...(options.headers as Record<string, string> || {}),
    },
  });
  if (res.status === 401) {
    localStorage.removeItem('wealth_agent_user');
    window.dispatchEvent(new CustomEvent('auth:expired'));
    throw new Error('Session expired');
  }
  if (res.status === 402) {
    // Paywalled deployment and this account has no active subscription —
    // App listens for this and swaps in the paywall view.
    window.dispatchEvent(new CustomEvent('billing:required'));
    throw new Error('Subscription required');
  }
  if (!res.ok) {
    let detail = `HTTP ${res.status}`;
    try {
      const body = await res.clone().json();
      if (body.message) detail = body.message;
    } catch {}
    throw new Error(detail);
  }
  return res;
}

export interface AuthUser {
  id: string;
  email: string;
  name: string;
}

export interface PrivacyStatus {
  /** Server-side PRIVACY_ENCRYPTION flag. */
  enabled: boolean;
  /** This user has a keypair (opted in). */
  configured: boolean;
  /** Private key is currently cached server-side for this user. */
  unlocked: boolean;
}

export interface PrivacySetupResult {
  sealed_accounts: number;
  sealed_transactions: number;
  sealed_holdings: number;
  note: string;
}

export interface AppConfig {
  /** Public no-login demo instance: Google login + bank linking disabled. */
  demo_mode: boolean;
  /** Server-side PRIVACY_ENCRYPTION flag. */
  privacy_enabled: boolean;
  /** Server-side BILLING flag: subscription paywall active. */
  billing_enabled: boolean;
}

export interface BillingStatus {
  /** Server-side BILLING flag; when false, entitled is always true. */
  enabled: boolean;
  /** This user may use the app (subscriber, owner, or billing disabled). */
  entitled: boolean;
  /** Stripe subscription status ('none' until first checkout). */
  status: string;
  /** A Stripe customer exists — the portal is available. */
  has_customer: boolean;
  /** Paid-through timestamp (RFC3339) or null. */
  current_period_end: string | null;
}

export const apiClient = {
  async getConfig(): Promise<AppConfig> {
    const res = await apiFetch(`${API_BASE}/config`);
    return res.json();
  },

  async demoLogin(): Promise<{ user: AuthUser }> {
    const res = await fetch(`${API_BASE}/auth/demo`, {
      method: 'POST',
      credentials: 'include',
    });
    if (!res.ok) {
      let detail = 'Could not start the demo';
      try {
        const body = await res.json();
        if (body.message) detail = body.message;
      } catch { /* non-JSON error body */ }
      throw new Error(detail);
    }
    return res.json();
  },

  async getBillingStatus(): Promise<BillingStatus> {
    const res = await apiFetch(`${API_BASE}/billing/status`);
    return res.json();
  },

  /** Create a Stripe Checkout session; caller redirects to the returned URL. */
  async billingCheckout(plan: 'monthly' | 'annual'): Promise<string> {
    const res = await apiFetch(`${API_BASE}/billing/checkout`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ plan }),
    });
    const json = await res.json();
    return json.url;
  },

  /** Create a Stripe Customer Portal session; caller redirects to the URL. */
  async billingPortal(): Promise<string> {
    const res = await apiFetch(`${API_BASE}/billing/portal`, { method: 'POST' });
    const json = await res.json();
    return json.url;
  },

  async googleLogin(credential: string, acceptedTerms: boolean): Promise<{ user: AuthUser }> {
    const res = await fetch(`${API_BASE}/auth/google`, {
      method: 'POST',
      credentials: 'include',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ credential, accepted_terms: acceptedTerms }),
    });
    if (res.status === 428) {
      // First-time (or new-version) consent needed — caller shows the checkbox.
      throw new ConsentRequiredError();
    }
    if (!res.ok) {
      let detail = 'Login failed';
      try {
        const body = await res.json();
        if (body.message) detail = body.message;
      } catch { /* non-JSON error body */ }
      throw new Error(detail);
    }
    return res.json();
  },

  async logout(): Promise<void> {
    await fetch(`${API_BASE}/auth/logout`, { method: 'POST', credentials: 'include' });
  },

  async refreshSession(): Promise<void> {
    await apiFetch(`${API_BASE}/auth/refresh`, { method: 'POST' });
  },

  async getAccounts(): Promise<Account[]> {
    const res = await apiFetch(`${API_BASE}/accounts`);
    const json: SuccessResponse<Account[]> = await res.json();
    return json.data;
  },

  async updateAccount(id: string, customName: string | null, accountType?: 'asset' | 'liability'): Promise<void> {
    await apiFetch(`${API_BASE}/accounts/${id}`, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ custom_name: customName, account_type: accountType }),
    });
  },

  async aggregatePortfolio(
    accountIds: string[],
    timeRange: { start_date: string; end_date: string; interval: string }
  ): Promise<AggregateData> {
    const res = await apiFetch(`${API_BASE}/portfolio/aggregate`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ account_ids: accountIds, time_range: timeRange }),
    });
    const json: SuccessResponse<AggregateData> = await res.json();
    return json.data;
  },

  async getTransactions(offset = 0, limit?: number): Promise<TransactionPage> {
    const params = new URLSearchParams({ offset: String(offset) });
    if (limit !== undefined) params.set('limit', String(limit));
    const res = await apiFetch(`${API_BASE}/transactions?${params}`);
    const json: SuccessResponse<TransactionPage> = await res.json();
    return json.data;
  },

  async getInvestmentTransactions(offset = 0, limit?: number): Promise<InvestmentTransactionPage> {
    const params = new URLSearchParams({ offset: String(offset) });
    if (limit !== undefined) params.set('limit', String(limit));
    const res = await apiFetch(`${API_BASE}/investment_transactions?${params}`);
    const json: SuccessResponse<InvestmentTransactionPage> = await res.json();
    return json.data;
  },

  async updateTransaction(id: string, fields: { note?: string | null; tags?: string[] }): Promise<void> {
    await apiFetch(`${API_BASE}/transactions/${id}`, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(fields),
    });
  },

  async bulkUpdateTransactions(
    transactionIds: string[],
    action: 'add_tag' | 'remove_tag' | 'set_note' | 'clear_note',
    value?: string
  ): Promise<void> {
    await apiFetch(`${API_BASE}/transactions/bulk_update`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ transaction_ids: transactionIds, action, value }),
    });
  },

  async getSavedSearches(): Promise<SavedSearch[]> {
    const res = await apiFetch(`${API_BASE}/saved_searches`);
    const json: SuccessResponse<SavedSearch[]> = await res.json();
    return json.data;
  },

  async createSavedSearch(req: CreateSavedSearchRequest): Promise<SavedSearch> {
    const res = await apiFetch(`${API_BASE}/saved_searches`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(req),
    });
    const json: SuccessResponse<SavedSearch> = await res.json();
    return json.data;
  },

  async deleteSavedSearch(id: number): Promise<void> {
    await apiFetch(`${API_BASE}/saved_searches/${id}`, { method: 'DELETE' });
  },

  async createLinkToken(): Promise<string> {
    const res = await apiFetch(`${API_BASE}/plaid/create_link_token`, { method: 'POST' });
    const json = await res.json();
    return json.link_token;
  },

  async exchangePublicToken(publicToken: string): Promise<void> {
    await apiFetch(`${API_BASE}/plaid/exchange_public_token`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ public_token: publicToken }),
    });
  },

  /**
   * Kick off a sync and resolve when it finishes. The server runs the import
   * detached (a large re-sync outlives request/proxy timeouts), so completion
   * is observed by polling sync_status — callers keep their await-then-refresh
   * flow unchanged. Gives up after ~3 minutes and resolves anyway.
   */
  async syncPlaidData(): Promise<void> {
    const res = await apiFetch(`${API_BASE}/plaid/sync`, { method: 'POST' });
    const json = await res.json();
    if (!json.syncing) return; // no linked items
    for (let i = 0; i < 60; i++) {
      await new Promise(r => setTimeout(r, 3000));
      try {
        const syncing = await this.getPlaidSyncStatus();
        if (!syncing) return;
      } catch { /* transient — keep polling */ }
    }
  },

  /** True while a background initial import (started by linking) is running. */
  async getPlaidSyncStatus(): Promise<boolean> {
    const res = await apiFetch(`${API_BASE}/plaid/sync_status`);
    const json = await res.json();
    return Boolean(json.data?.syncing);
  },

  async setCostBasis(txnId: string, costBasisCents: number, isLongTerm: boolean): Promise<void> {
    await apiFetch(`${API_BASE}/capital_gains/cost_basis/${encodeURIComponent(txnId)}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ cost_basis_cents: costBasisCents, is_long_term: isLongTerm }),
    });
  },

  async deleteCostBasis(txnId: string): Promise<void> {
    await apiFetch(`${API_BASE}/capital_gains/cost_basis/${encodeURIComponent(txnId)}`, { method: 'DELETE' });
  },

  async getCapitalGains(year?: number): Promise<CapitalGainsReport> {
    const params = new URLSearchParams();
    if (year !== undefined) params.set('year', String(year));
    const res = await apiFetch(`${API_BASE}/capital_gains?${params}`);
    const json: SuccessResponse<CapitalGainsReport> = await res.json();
    return json.data;
  },

  async resetDatabase(): Promise<void> {
    await apiFetch(`${API_BASE}/system/reset`, { method: 'POST' });
  },

  async whoami(): Promise<WhoamiResponse> {
    const res = await apiFetch(`${API_BASE}/auth/whoami`);
    const json: SuccessResponse<WhoamiResponse> = await res.json();
    return json.data;
  },

  async listTokens(): Promise<ApiToken[]> {
    const res = await apiFetch(`${API_BASE}/auth/tokens`);
    const json: SuccessResponse<ApiToken[]> = await res.json();
    return json.data;
  },

  async createToken(name: string, scopes?: string): Promise<CreateTokenResponse> {
    const res = await apiFetch(`${API_BASE}/auth/tokens`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name, scopes }),
    });
    const json: SuccessResponse<CreateTokenResponse> = await res.json();
    return json.data;
  },

  async revokeToken(id: string): Promise<void> {
    await apiFetch(`${API_BASE}/auth/tokens/${encodeURIComponent(id)}`, { method: 'DELETE' });
  },

  async listOAuthGrants(): Promise<OAuthGrant[]> {
    const res = await apiFetch(`${API_BASE}/auth/oauth_grants`);
    const json: SuccessResponse<OAuthGrant[]> = await res.json();
    return json.data;
  },

  async revokeOAuthGrant(clientId: string): Promise<void> {
    await apiFetch(`${API_BASE}/auth/oauth_grants/${encodeURIComponent(clientId)}`, { method: 'DELETE' });
  },

  async getPrivacyStatus(): Promise<PrivacyStatus> {
    const res = await apiFetch(`${API_BASE}/privacy/status`);
    const json: SuccessResponse<PrivacyStatus> = await res.json();
    return json.data;
  },

  async privacySetup(passphrase: string): Promise<PrivacySetupResult> {
    const res = await apiFetch(`${API_BASE}/privacy/setup`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ passphrase }),
    });
    const json: SuccessResponse<PrivacySetupResult> = await res.json();
    return json.data;
  },

  async privacyUnlock(passphrase: string): Promise<void> {
    // Raw fetch: a wrong passphrase is a 401 here, which must not be treated
    // as session expiry (apiFetch would log the user out).
    const res = await fetch(`${API_BASE}/privacy/unlock`, {
      method: 'POST',
      credentials: 'include',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ passphrase }),
    });
    if (res.status === 401) throw new Error('Wrong passphrase');
    if (!res.ok) {
      let detail = `HTTP ${res.status}`;
      try {
        const body = await res.clone().json();
        if (body.message) detail = body.message;
      } catch {}
      throw new Error(detail);
    }
  },

  async privacyLock(): Promise<void> {
    await apiFetch(`${API_BASE}/privacy/lock`, { method: 'POST' });
  },
};
