import React, { useState, useEffect, useCallback } from 'react';
import { RefreshCw, Eye, EyeOff, Menu } from 'lucide-react';
import { apiClient } from './api/client';
import type { BillingStatus } from './api/client';
import { usePrivacy } from './context/PrivacyContext';
import { useConfig } from './context/ConfigContext';
import Sidebar from './components/Sidebar';
import PortfolioView from './views/PortfolioView';
import TransactionsView from './views/TransactionsView';
import ConnectView from './views/ConnectView';
import LoginView from './views/LoginView';
import PaywallView from './views/PaywallView';
import ProfileDropdown from './components/ProfileDropdown';
import PlaidOAuthResume from './components/PlaidOAuthResume';
import PrivacyLockButton, { PrivacyGate } from './components/PrivacyLockButton';
import PrivacyLockView from './views/PrivacyLockView';

export interface AuthUser {
  id: string;
  email: string;
  name: string;
}

const App: React.FC = () => {
  // Dev-login bootstrap (local testing only): a GET /api/auth/dev_login redirect
  // lands here with the user JSON in ?dev_user= (the session cookie is already
  // set). Seed the localStorage hint the SPA uses so we render logged-in, then
  // strip the param. No-op in normal use where the param is absent.
  (() => {
    const sp = new URLSearchParams(window.location.search);
    const du = sp.get('dev_user');
    if (!du) return;
    try { localStorage.setItem('wealth_agent_user', du); } catch { /* ignore */ }
    sp.delete('dev_user');
    const qs = sp.toString();
    window.history.replaceState({}, '', window.location.pathname + (qs ? `?${qs}` : ''));
  })();

  // 'settings' was merged into the Connect tab ('advisory'); migrate any stale stored value.
  const initialTab = (() => {
    const stored = localStorage.getItem('wealth_agent_tab');
    // 'settings' was merged into Connect ('advisory') and 'tax' was removed.
    return stored === 'settings' || stored === 'tax' || !stored ? 'portfolio' : stored;
  })();
  const [activeTab, setActiveTab] = useState(initialTab);
  const [mountedTabs, setMountedTabs] = useState<Set<string>>(() => new Set([initialTab]));
  const [refreshKey, setRefreshKey] = useState(0);
  const [syncing, setSyncing] = useState(false);
  // Mobile nav drawer (< md); the desktop hover-rail ignores this.
  const [navOpen, setNavOpen] = useState(false);
  const { hidden, toggle: togglePrivacy } = usePrivacy();

  const handleManualSync = useCallback(async () => {
    setSyncing(true);
    try {
      await apiClient.syncPlaidData();
      setRefreshKey(k => k + 1);
    } catch (e) {
      console.error('Manual sync failed', e);
    } finally {
      setSyncing(false);
    }
  }, []);

  const handleTabChange = (tab: string) => {
    setActiveTab(tab);
    setMountedTabs(prev => new Set([...prev, tab]));
    localStorage.setItem('wealth_agent_tab', tab);
  };
  // Auth is the HttpOnly cookie (unreadable by JS). Use localStorage user info as
  // a hint for "is logged in" — a 401 from any API call will fire auth:expired and
  // clear this, showing the login screen.
  const [isLoggedIn, setIsLoggedIn] = useState(() => !!localStorage.getItem('wealth_agent_user'));
  const [user, setUser] = useState<AuthUser | null>(() => {
    const raw = localStorage.getItem('wealth_agent_user');
    return raw ? JSON.parse(raw) : null;
  });

  // Runtime deployment config (so one frontend image serves prod and demo).
  const { demoMode, billingEnabled } = useConfig();

  // Subscription paywall (BILLING=on deployments). null = status not loaded yet.
  const [billing, setBilling] = useState<BillingStatus | null>(null);
  // Just returned from Stripe Checkout — poll until the webhook confirms.
  const [activating, setActivating] = useState(
    () => new URLSearchParams(window.location.search).get('billing') === 'success'
  );

  useEffect(() => {
    if (!isLoggedIn || !billingEnabled) return;
    let cancelled = false;
    const load = async (): Promise<BillingStatus | null> => {
      try {
        const s = await apiClient.getBillingStatus();
        if (!cancelled) setBilling(s);
        return s;
      } catch {
        return null;
      }
    };
    if (!activating) {
      load();
      return () => { cancelled = true; };
    }
    // Post-checkout: subscription state arrives via webhook, so poll briefly
    // even if the user paid and the tab raced the webhook.
    (async () => {
      for (let i = 0; i < 15 && !cancelled; i++) {
        const s = await load();
        if (s?.entitled) break;
        await new Promise(r => setTimeout(r, 2000));
      }
      if (!cancelled) {
        setActivating(false);
        window.history.replaceState({}, '', window.location.pathname);
      }
    })();
    return () => { cancelled = true; };
  }, [isLoggedIn, billingEnabled, activating]);

  // Any API call that 402s flips us to the paywall (e.g. subscription lapsed
  // while the tab was open).
  useEffect(() => {
    const onRequired = () => setBilling(b =>
      b ? { ...b, entitled: false }
        : { enabled: true, entitled: false, status: 'none', has_customer: false, current_period_end: null }
    );
    window.addEventListener('billing:required', onRequired);
    return () => window.removeEventListener('billing:required', onRequired);
  }, []);

  const openBillingPortal = useCallback(async () => {
    try {
      window.location.href = await apiClient.billingPortal();
    } catch (e) {
      console.error('Could not open billing portal', e);
    }
  }, []);

  // OAuth consent bridge: the backend /authorize endpoint bounces unauthenticated
  // (or cross-site first-hop) requests here with `_consent=1` and the original
  // query. Once the user has a session, navigate back to /authorize so the backend
  // can render the consent screen (a real navigation, so the cookie is sent).
  const consentTarget = (() => {
    const sp = new URLSearchParams(window.location.search);
    if (sp.get('_consent') !== '1') return null;
    sp.delete('_consent');
    return `/authorize?${sp.toString()}`;
  })();

  const handleLogin = () => {
    const raw = localStorage.getItem('wealth_agent_user');
    setUser(raw ? JSON.parse(raw) : null);
    setIsLoggedIn(true);
  };

  const handleLogout = useCallback(async () => {
    try { await apiClient.logout(); } catch {}
    localStorage.removeItem('wealth_agent_user');
    setUser(null);
    setIsLoggedIn(false);
  }, []);

  // Listen for session expiry fired by the API client on 401 — avoids page reload
  useEffect(() => {
    window.addEventListener('auth:expired', handleLogout);
    return () => window.removeEventListener('auth:expired', handleLogout);
  }, [handleLogout]);

  // Silently renew the session while the app is open.
  // Runs once on mount (catches stale-but-valid JWTs from previous visits) then
  // every 12 hours so an always-open tab never hits the 24-hour sliding window.
  // A 401 from refresh dispatches auth:expired and shows the login screen.
  useEffect(() => {
    if (!isLoggedIn) return;
    apiClient.refreshSession().catch(() => {}); // best-effort on mount
    const id = setInterval(() => {
      apiClient.refreshSession().catch(() => {});
    }, 12 * 60 * 60 * 1000); // 12 hours
    return () => clearInterval(id);
  }, [isLoggedIn]);

  // Redirect to the consent screen once authenticated (avoids flashing the dashboard).
  useEffect(() => {
    if (consentTarget && isLoggedIn) {
      window.location.replace(consentTarget);
    }
  }, [consentTarget, isLoggedIn]);

  if (!isLoggedIn) {
    return <LoginView onLogin={handleLogin} postLoginRedirect={consentTarget ?? undefined} demoMode={demoMode} />;
  }

  // Subscription gate (BILLING=on deployments; demo instances are never gated).
  if (billingEnabled && !demoMode) {
    if (activating) {
      return (
        <PaywallView
          user={user}
          billing={billing ?? { enabled: true, entitled: false, status: 'none', has_customer: false, current_period_end: null }}
          activating
          onLogout={handleLogout}
        />
      );
    }
    if (!billing) {
      // Don't flash the dashboard (and fire a burst of 402s) before we know.
      return (
        <div className="flex h-screen items-center justify-center bg-slate-950 text-slate-400 text-sm">
          Loading…
        </div>
      );
    }
    if (!billing.entitled) {
      return <PaywallView user={user} billing={billing} onLogout={handleLogout} />;
    }
  }

  if (consentTarget) {
    return (
      <div className="flex h-screen items-center justify-center bg-slate-950 text-slate-400 text-sm">
        Connecting…
      </div>
    );
  }

  const show = (tab: string) => activeTab === tab ? '' : 'hidden';

  const titles: Record<string, string> = {
    portfolio: 'Account Hierarchy & Holdings',
    transactions: 'Bank Transaction Logs',
    investments: 'Investment Trades & Capital Gains',
    advisory: 'MCP: Connect your AI',
    privacy: 'Privacy Lock',
  };

  return (
    <PrivacyGate onSetUp={() => handleTabChange('privacy')}>
    <div className="flex h-screen overflow-hidden bg-slate-950 text-slate-100 font-sans antialiased">
      <PlaidOAuthResume onLinked={() => setRefreshKey(k => k + 1)} />
      <Sidebar
        activeTab={activeTab}
        onTabChange={handleTabChange}
        mobileOpen={navOpen}
        onMobileClose={() => setNavOpen(false)}
      />

      <main className="flex-1 flex flex-col min-w-0 bg-slate-950 overflow-y-auto">
        {demoMode && (
          <div className="bg-amber-500/15 border-b border-amber-500/30 text-amber-200 text-xs font-medium px-4 md:px-8 py-2 text-center shrink-0">
            Demo · sample data from Plaid Sandbox, refreshed daily. Bank linking isn't available here — sign up to connect your own accounts.
          </div>
        )}
        {billingEnabled && billing?.status === 'past_due' && (
          <div className="bg-rose-500/15 border-b border-rose-500/30 text-rose-200 text-xs font-medium px-4 md:px-8 py-2 text-center shrink-0">
            Your last payment failed — access ends at the period end unless it's fixed.{' '}
            <button onClick={openBillingPortal} className="underline font-bold hover:text-rose-100">
              Update payment method
            </button>
          </div>
        )}
        <header className="h-16 border-b border-slate-800 bg-slate-900/50 backdrop-blur px-4 md:px-8 flex items-center justify-between gap-2 shrink-0 sticky top-0 z-30">
          <div className="flex items-center gap-2 min-w-0">
            <button
              onClick={() => setNavOpen(true)}
              aria-label="Open menu"
              className="md:hidden p-2 -ml-1 rounded-lg text-slate-300 hover:bg-slate-800 transition-colors shrink-0"
            >
              <Menu className="w-5 h-5" />
            </button>
            <h2 className="text-base md:text-lg font-semibold text-slate-200 truncate">{titles[activeTab]}</h2>
          </div>
          <div className="flex items-center gap-2 sm:gap-3 shrink-0">
            <PrivacyLockButton onGoToPrivacyTab={() => handleTabChange('privacy')} />
            <button
              onClick={togglePrivacy}
              title={hidden ? 'Show dollar amounts' : 'Hide dollar amounts (demo mode)'}
              aria-pressed={hidden}
              className={`flex items-center gap-2 px-3 sm:px-4 py-2 text-xs font-bold rounded-xl transition-all border ${
                hidden
                  ? 'bg-amber-500/10 hover:bg-amber-500/20 text-amber-300 border-amber-500/30'
                  : 'bg-slate-800 hover:bg-slate-700 text-slate-300 border-slate-700'
              }`}
            >
              {hidden ? <EyeOff className="w-3.5 h-3.5" /> : <Eye className="w-3.5 h-3.5" />}
              <span className="hidden sm:inline">{hidden ? 'Amounts hidden' : 'Amounts shown'}</span>
            </button>
            {activeTab === 'portfolio' && !demoMode && (
              <button
                onClick={handleManualSync}
                disabled={syncing}
                className="flex items-center gap-2 px-3 sm:px-4 py-2 bg-slate-800 hover:bg-slate-700 disabled:opacity-50 text-xs font-bold rounded-xl transition-all border border-slate-700"
              >
                <RefreshCw className={`w-3.5 h-3.5 ${syncing ? 'animate-spin' : ''}`} />
                <span className="hidden sm:inline">{syncing ? 'Syncing...' : 'Refresh'}</span>
              </button>
            )}
            <ProfileDropdown
              user={user}
              onRefreshNeeded={() => setRefreshKey(k => k + 1)}
              onLogout={handleLogout}
              demoMode={demoMode}
              onManageBilling={billingEnabled && billing?.has_customer ? openBillingPortal : undefined}
            />
          </div>
        </header>

        <div className="p-4 md:p-8 flex-1">
          {/* Each view mounts once on first visit and stays mounted (keep-alive).
              Tab switches only toggle visibility — no re-fetches. */}
          <div className={show('portfolio')}>
            <PortfolioView key={refreshKey} />
          </div>
          {mountedTabs.has('transactions') && (
            <div className={show('transactions')}>
              <TransactionsView mode="bank" />
            </div>
          )}
          {mountedTabs.has('investments') && (
            <div className={show('investments')}>
              <TransactionsView mode="investments" />
            </div>
          )}
          {mountedTabs.has('advisory') && (
            <div className={show('advisory')}>
              <ConnectView />
            </div>
          )}
          {mountedTabs.has('privacy') && (
            <div className={show('privacy')}>
              <PrivacyLockView />
            </div>
          )}
        </div>
      </main>
    </div>
    </PrivacyGate>
  );
};

export default App;
