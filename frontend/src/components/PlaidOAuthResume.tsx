import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { usePlaidLink } from 'react-plaid-link';
import type { PlaidLinkOptions } from 'react-plaid-link';
import { apiClient } from '../api/client';
import { watchInitialSync } from '../hooks/usePlaidLinkFlow';
import { Loader2, AlertCircle } from 'lucide-react';

interface PlaidOAuthResumeProps {
  /** Called after the public token is exchanged so the app can re-fetch data. */
  onLinked: () => void;
}

/**
 * Resumes a Plaid Link OAuth flow after the bank redirects the browser back to
 * /oauth/plaid?oauth_state_id=... .
 *
 * OAuth institutions (Amex, Chase, Capital One, …) navigate the WHOLE tab to the
 * bank's hosted login, so the original <PlaidLinkButton> (which lives inside a
 * closed dropdown) is gone by the time we return. This component is always mounted
 * while logged in, detects the redirect, and re-opens Link with:
 *   - the SAME link_token that started the flow (persisted in localStorage), and
 *   - receivedRedirectUri = the full return URL,
 * which is exactly what Plaid requires to finish an OAuth handoff.
 */
const PlaidOAuthResume: React.FC<PlaidOAuthResumeProps> = ({ onLinked }) => {
  // Capture once on mount — usePlaidLink must see a stable config.
  const isOAuthReturn = useMemo(
    () => new URLSearchParams(window.location.search).has('oauth_state_id'),
    []
  );
  const storedToken = useMemo(
    () => (isOAuthReturn ? localStorage.getItem('plaid_link_token') : null),
    [isOAuthReturn]
  );

  const [status, setStatus] = useState<'idle' | 'resuming' | 'exchanging' | 'error'>(
    isOAuthReturn ? 'resuming' : 'idle'
  );
  const [error, setError] = useState<string | null>(null);

  // Strip the OAuth query params and return to the app shell.
  const clearUrl = useCallback(() => {
    window.history.replaceState({}, '', '/');
  }, []);

  const handleSuccess = useCallback(
    async (public_token: string) => {
      setStatus('exchanging');
      try {
        await apiClient.exchangePublicToken(public_token);
        localStorage.removeItem('plaid_link_token');
        clearUrl();
        setStatus('idle');
        onLinked();
        void watchInitialSync(onLinked);
      } catch (e: unknown) {
        setError(e instanceof Error ? e.message : String(e));
        setStatus('error');
      }
    },
    [clearUrl, onLinked]
  );

  const handleExit = useCallback(() => {
    // User aborted or Plaid errored out of the OAuth flow.
    localStorage.removeItem('plaid_link_token');
    clearUrl();
    setStatus('idle');
  }, [clearUrl]);

  const config: PlaidLinkOptions = {
    token: storedToken,
    receivedRedirectUri: isOAuthReturn ? window.location.href : undefined,
    onSuccess: handleSuccess,
    onExit: handleExit,
  };
  const { open, ready } = usePlaidLink(config);

  // Auto-reopen Link to complete the handoff as soon as it's ready.
  useEffect(() => {
    if (isOAuthReturn && storedToken && ready) {
      open();
    }
  }, [isOAuthReturn, storedToken, ready, open]);

  // Lost the original token (e.g. localStorage cleared) — can't resume; bail out.
  useEffect(() => {
    if (isOAuthReturn && !storedToken) {
      setError('Could not resume the bank connection. Please start linking again.');
      setStatus('error');
      const t = setTimeout(clearUrl, 4000);
      return () => clearTimeout(t);
    }
  }, [isOAuthReturn, storedToken, clearUrl]);

  if (status === 'idle') return null;

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-slate-950/80 backdrop-blur">
      <div className="flex flex-col items-center gap-3 text-center px-6">
        {status === 'error' ? (
          <>
            <AlertCircle className="w-8 h-8 text-rose-400" />
            <p className="text-sm text-rose-300 max-w-xs">{error}</p>
          </>
        ) : (
          <>
            <Loader2 className="w-8 h-8 animate-spin text-blue-400" />
            <p className="text-sm text-slate-300">
              {status === 'exchanging' ? 'Finishing up…' : 'Completing bank connection…'}
            </p>
          </>
        )}
      </div>
    </div>
  );
};

export default PlaidOAuthResume;
