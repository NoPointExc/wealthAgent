import { useCallback, useEffect, useRef, useState } from 'react';
import { usePlaidLink } from 'react-plaid-link';
import { apiClient } from '../api/client';

export interface PlaidLinkFlow {
  /** Start linking: mints a link token on first call, then opens Plaid Link. */
  open: () => void;
  /** True while a token is being minted or a public token is being exchanged. */
  busy: boolean;
  error: string | null;
}

/**
 * Shared Plaid Link flow with LAZY token minting.
 *
 * Unlike fetching a link token on mount, this only calls createLinkToken when the
 * user actually clicks — so rendering several entry points (dropdown + the "+"
 * tiles in each Portfolio grid) costs zero extra Plaid calls per page load.
 *
 * The minted token is persisted to localStorage before opening so an OAuth bank
 * (Amex, Chase, …) can resume the same flow via <PlaidOAuthResume> after it
 * redirects the whole tab away and back.
 */
/**
 * The initial import runs server-side in the background after linking
 * (exchange returns immediately so Plaid Link never looks stuck). Poll
 * /api/plaid/sync_status and refresh the app as data lands: once shortly
 * after linking (accounts appear early in the import) and once when the
 * import finishes (transactions/holdings complete).
 */
export async function watchInitialSync(onUpdate: () => void, maxPolls = 60, intervalMs = 3000): Promise<void> {
  let refreshedEarly = false;
  for (let i = 0; i < maxPolls; i++) {
    await new Promise(r => setTimeout(r, intervalMs));
    try {
      const syncing = await apiClient.getPlaidSyncStatus();
      if (!syncing) {
        onUpdate();
        return;
      }
      if (!refreshedEarly && i >= 1) {
        refreshedEarly = true;
        onUpdate(); // accounts/balances are usually in by now
      }
    } catch {
      // transient — keep polling
    }
  }
  onUpdate(); // gave up waiting; refresh with whatever made it
}

export function usePlaidLinkFlow(onSuccess: () => void): PlaidLinkFlow {
  const [linkToken, setLinkToken] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [fetching, setFetching] = useState(false);
  const [exchanging, setExchanging] = useState(false);
  // Set when a token was minted by a click and we should open as soon as ready.
  const shouldOpen = useRef(false);

  const handleSuccess = useCallback(async (public_token: string) => {
    setError(null);
    setExchanging(true);
    try {
      await apiClient.exchangePublicToken(public_token);
      localStorage.removeItem('plaid_link_token');
      onSuccess();
      void watchInitialSync(onSuccess);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error('exchangePublicToken failed:', msg);
      setError(msg);
    } finally {
      setExchanging(false);
    }
  }, [onSuccess]);

  const { open: plaidOpen, ready } = usePlaidLink({ token: linkToken, onSuccess: handleSuccess });

  // A freshly-minted token flips `ready` true on the next render — open then.
  useEffect(() => {
    if (ready && shouldOpen.current) {
      shouldOpen.current = false;
      plaidOpen();
    }
  }, [ready, plaidOpen]);

  const open = useCallback(async () => {
    setError(null);
    // Token already live (e.g. user opened once, exited, clicks again) — open now.
    if (linkToken && ready) {
      plaidOpen();
      return;
    }
    setFetching(true);
    try {
      const token = await apiClient.createLinkToken();
      if (!token) throw new Error('Empty link token returned');
      // Persist for OAuth resume — Plaid rejects a resume on a different token.
      localStorage.setItem('plaid_link_token', token);
      shouldOpen.current = true;
      setLinkToken(token);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error('Failed to fetch link token:', msg);
      setError(msg);
    } finally {
      setFetching(false);
    }
  }, [linkToken, ready, plaidOpen]);

  return { open, busy: fetching || exchanging, error };
}
