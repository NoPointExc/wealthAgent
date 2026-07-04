import React, { useState, useEffect, useCallback } from 'react';
import { RefreshCw, Eye, EyeOff } from 'lucide-react';
import { apiClient } from './api/client';
import { usePrivacy } from './context/PrivacyContext';
import Sidebar from './components/Sidebar';
import PortfolioView from './views/PortfolioView';
import TransactionsView from './views/TransactionsView';
import TaxView from './views/TaxView';
import ConnectView from './views/ConnectView';
import LoginView from './views/LoginView';
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
  // 'settings' was merged into the Connect tab ('advisory'); migrate any stale stored value.
  const initialTab = (() => {
    const stored = localStorage.getItem('wealth_agent_tab');
    return stored === 'settings' || !stored ? 'portfolio' : stored;
  })();
  const [activeTab, setActiveTab] = useState(initialTab);
  const [mountedTabs, setMountedTabs] = useState<Set<string>>(() => new Set([initialTab]));
  const [refreshKey, setRefreshKey] = useState(0);
  const [syncing, setSyncing] = useState(false);
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
    return <LoginView onLogin={handleLogin} postLoginRedirect={consentTarget ?? undefined} />;
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
    tax: 'Automated Tax Preparation',
    advisory: 'Connect your AI',
    privacy: 'Privacy Lock',
  };

  return (
    <PrivacyGate onSetUp={() => handleTabChange('privacy')}>
    <div className="flex h-screen overflow-hidden bg-slate-950 text-slate-100 font-sans antialiased">
      <PlaidOAuthResume onLinked={() => setRefreshKey(k => k + 1)} />
      <Sidebar activeTab={activeTab} onTabChange={handleTabChange} />

      <main className="flex-1 flex flex-col min-w-0 bg-slate-950 overflow-y-auto">
        <header className="h-16 border-b border-slate-800 bg-slate-900/50 backdrop-blur px-8 flex items-center justify-between shrink-0 sticky top-0 z-30">
          <h2 className="text-lg font-semibold text-slate-200">{titles[activeTab]}</h2>
          <div className="flex items-center gap-3">
            <PrivacyLockButton onGoToPrivacyTab={() => handleTabChange('privacy')} />
            <button
              onClick={togglePrivacy}
              title={hidden ? 'Show dollar amounts' : 'Hide dollar amounts (demo mode)'}
              aria-pressed={hidden}
              className={`flex items-center gap-2 px-4 py-2 text-xs font-bold rounded-xl transition-all border ${
                hidden
                  ? 'bg-amber-500/10 hover:bg-amber-500/20 text-amber-300 border-amber-500/30'
                  : 'bg-slate-800 hover:bg-slate-700 text-slate-300 border-slate-700'
              }`}
            >
              {hidden ? <EyeOff className="w-3.5 h-3.5" /> : <Eye className="w-3.5 h-3.5" />}
              {hidden ? 'Amounts hidden' : 'Amounts shown'}
            </button>
            {activeTab === 'portfolio' && (
              <button
                onClick={handleManualSync}
                disabled={syncing}
                className="flex items-center gap-2 px-4 py-2 bg-slate-800 hover:bg-slate-700 disabled:opacity-50 text-xs font-bold rounded-xl transition-all border border-slate-700"
              >
                <RefreshCw className={`w-3.5 h-3.5 ${syncing ? 'animate-spin' : ''}`} />
                {syncing ? 'Syncing...' : 'Refresh'}
              </button>
            )}
            <ProfileDropdown
              user={user}
              onRefreshNeeded={() => setRefreshKey(k => k + 1)}
              onLogout={handleLogout}
            />
          </div>
        </header>

        <div className="p-8 flex-1">
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
          {mountedTabs.has('tax') && (
            <div className={show('tax')}>
              <TaxView />
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
