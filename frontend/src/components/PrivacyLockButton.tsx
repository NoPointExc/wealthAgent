import React, { useState, useEffect, useCallback } from 'react';
import { Lock, LockOpen, Loader2 } from 'lucide-react';
import { apiClient } from '../api/client';
import type { PrivacyStatus } from '../api/client';
import { PRIVACY_CHANGED_EVENT } from './PrivacyEncryptionCard';

/**
 * Header "Privacy Lock" badge. Three states:
 *  - Off (grey, unlocked icon): encryption not set up — click goes to the
 *    Privacy Lock tab to set a Privacy Key.
 *  - On (green, locked icon): data encrypted at rest, readable this session.
 *  - Locked (amber, locked icon): data encrypted and the session has no key —
 *    click opens the unlock dialog.
 * Hidden when the server runs with PRIVACY_ENCRYPTION=off.
 */
const PrivacyLockButton: React.FC<{ onGoToPrivacyTab: () => void }> = ({ onGoToPrivacyTab }) => {
  const [status, setStatus] = useState<PrivacyStatus | null>(null);
  const [showUnlock, setShowUnlock] = useState(false);

  const load = useCallback(async () => {
    try {
      setStatus(await apiClient.getPrivacyStatus());
    } catch {
      setStatus(null);
    }
  }, []);

  useEffect(() => { load(); }, [load]);
  useEffect(() => {
    window.addEventListener(PRIVACY_CHANGED_EVENT, load);
    return () => window.removeEventListener(PRIVACY_CHANGED_EVENT, load);
  }, [load]);

  if (!status || !status.enabled) return null;

  const state: 'off' | 'on' | 'locked' =
    !status.configured ? 'off' : status.unlocked ? 'on' : 'locked';

  const looks = {
    off: {
      cls: 'bg-slate-800/60 hover:bg-slate-700 text-slate-500 border-slate-700',
      icon: <LockOpen className="w-3.5 h-3.5" />,
      label: 'Privacy Lock · Off',
      tip: 'Privacy Lock is off: your data is not encrypted. Click to set up your Privacy Key.',
    },
    on: {
      cls: 'bg-emerald-500/10 hover:bg-emerald-500/20 text-emerald-300 border-emerald-500/30',
      icon: <Lock className="w-3.5 h-3.5" />,
      label: 'Privacy Lock · On',
      tip: 'Privacy Lock is on: your data is encrypted with your Privacy Key (readable in this session, auto-locks after 12h). Click to manage.',
    },
    locked: {
      cls: 'bg-amber-500/10 hover:bg-amber-500/20 text-amber-300 border-amber-500/30',
      icon: <Lock className="w-3.5 h-3.5" />,
      label: 'Privacy Lock · Locked',
      tip: 'Privacy Lock is on and locked: enter your Privacy Key to read your data. Click to unlock.',
    },
  }[state];

  return (
    <>
      <button
        onClick={() => (state === 'locked' ? setShowUnlock(true) : onGoToPrivacyTab())}
        title={looks.tip}
        className={`flex items-center gap-2 px-3 sm:px-4 py-2 text-xs font-bold rounded-xl transition-all border ${looks.cls}`}
      >
        {looks.icon}
        <span className="hidden sm:inline">{looks.label}</span>
      </button>

      {showUnlock && <UnlockDialog onClose={() => setShowUnlock(false)} />}
    </>
  );
};

/** Modal asking for the Privacy Key. Shared by the header badge and the
 *  login-time gate. Reloads the app on success so every view refetches. */
export const UnlockDialog: React.FC<{
  onClose?: () => void;
  onSkip?: () => void;
  intro?: string;
}> = ({ onClose, onSkip, intro }) => {
  const [pass, setPass] = useState('');
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const unlock = async () => {
    if (!pass || busy) return;
    setBusy(true);
    setErr(null);
    try {
      await apiClient.privacyUnlock(pass);
      window.location.reload();
    } catch (e) {
      setErr(e instanceof Error ? e.message : 'Unlock failed');
      setBusy(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-sm flex items-center justify-center p-4"
      onClick={() => !busy && onClose?.()}
    >
      <div
        className="bg-slate-900 border border-slate-700 rounded-2xl p-5 w-full max-w-sm space-y-3"
        onClick={e => e.stopPropagation()}
      >
        <h3 className="text-sm font-black text-slate-100 flex items-center gap-2">
          <Lock className="w-4 h-4 text-amber-400" /> Enter your Privacy Key
        </h3>
        <p className="text-[11px] text-slate-500">
          {intro ?? 'Your data is encrypted. Enter your Privacy Key to decrypt transaction descriptions, merchant names, notes, and account names for this session.'}
        </p>
        {err && <div className="p-2 bg-red-950 border border-red-800 rounded text-[11px] text-red-300">{err}</div>}
        <input
          type="password"
          autoFocus
          value={pass}
          onChange={e => setPass(e.target.value)}
          onKeyDown={e => e.key === 'Enter' && unlock()}
          placeholder="Privacy Key"
          className="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-xs text-slate-100 placeholder-slate-600 focus:outline-none focus:ring-1 focus:ring-amber-500"
        />
        <div className="flex justify-end gap-2">
          {onSkip && (
            <button
              onClick={onSkip}
              disabled={busy}
              className="px-3 py-2 text-xs font-bold rounded-lg text-slate-500 hover:text-slate-300 transition-colors"
            >
              Continue locked
            </button>
          )}
          {onClose && (
            <button
              onClick={onClose}
              disabled={busy}
              className="px-3 py-2 text-xs font-bold rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 transition-colors"
            >
              Cancel
            </button>
          )}
          <button
            onClick={unlock}
            disabled={!pass || busy}
            className="flex items-center gap-1.5 px-3 py-2 text-xs font-bold rounded-lg bg-amber-600 hover:bg-amber-500 disabled:opacity-50 disabled:cursor-not-allowed text-white transition-colors"
          >
            {busy ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <LockOpen className="w-3.5 h-3.5" />}
            Unlock
          </button>
        </div>
      </div>
    </div>
  );
};

/**
 * Login-time gate: before the dashboard mounts (and starts fetching data),
 * check Privacy Lock state.
 *  - Key set + session locked → ask for the key up front ("Continue locked"
 *    lets them in with [locked] placeholders).
 *  - No key yet → offer to set one up; "Yes" jumps to the Privacy Lock tab,
 *    "Later" dismisses for this browser session (asks again on next login).
 */
export const PrivacyGate: React.FC<{
  children: React.ReactNode;
  onSetUp?: () => void;
}> = ({ children, onSetUp }) => {
  const [gate, setGate] = useState<'checking' | 'ask' | 'offer' | 'open'>('checking');

  useEffect(() => {
    // An intentional "Lock this session now" reloads into locked mode — asking
    // for the key right after the user chose to lock would feel circular.
    if (sessionStorage.getItem('wa_privacy_just_locked')) {
      sessionStorage.removeItem('wa_privacy_just_locked');
      setGate('open');
      return;
    }
    let cancelled = false;
    apiClient.getPrivacyStatus()
      .then(s => {
        if (cancelled) return;
        if (s.enabled && s.configured && !s.unlocked) setGate('ask');
        else if (s.enabled && !s.configured && !sessionStorage.getItem('wa_privacy_setup_later')) setGate('offer');
        else setGate('open');
      })
      .catch(() => { if (!cancelled) setGate('open'); });
    return () => { cancelled = true; };
  }, []);

  // Either choice stops the prompt from reappearing on every refresh this
  // session; a fresh login (new browser session) offers again.
  const dismissOffer = () => {
    sessionStorage.setItem('wa_privacy_setup_later', '1');
    setGate('open');
  };

  if (gate === 'checking') {
    return (
      <div className="flex h-screen items-center justify-center bg-slate-950 text-slate-500 text-sm gap-2">
        <Loader2 className="w-4 h-4 animate-spin" /> Checking Privacy Lock…
      </div>
    );
  }
  if (gate === 'ask') {
    return (
      <div className="h-screen bg-slate-950">
        <UnlockDialog
          intro="Welcome back. Your data is protected by Privacy Lock — enter your Privacy Key to decrypt it for this session."
          onSkip={() => setGate('open')}
        />
      </div>
    );
  }
  if (gate === 'offer') {
    return (
      <>
        {children}
        <div className="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-sm flex items-center justify-center p-4">
          <div className="bg-slate-900 border border-slate-700 rounded-2xl p-5 w-full max-w-sm space-y-3">
            <h3 className="text-sm font-black text-slate-100 flex items-center gap-2">
              <Lock className="w-4 h-4 text-emerald-400" /> Set up Privacy Lock?
            </h3>
            <p className="text-[11px] text-slate-500">
              Privacy Lock encrypts your transaction descriptions, merchant names,
              account names, and holdings with a key only you hold — even someone
              with full access to the server's database cannot read them.
            </p>
            <p className="text-[11px] text-slate-500">
              You can set it up any time from the Privacy Lock tab.
            </p>
            <div className="flex justify-end gap-2">
              <button
                onClick={dismissOffer}
                className="px-3 py-2 text-xs font-bold rounded-lg text-slate-500 hover:text-slate-300 transition-colors"
              >
                Later
              </button>
              <button
                onClick={() => { dismissOffer(); onSetUp?.(); }}
                className="flex items-center gap-1.5 px-3 py-2 text-xs font-bold rounded-lg bg-emerald-600 hover:bg-emerald-500 text-white transition-colors"
              >
                <Lock className="w-3.5 h-3.5" /> Set up Privacy Lock
              </button>
            </div>
          </div>
        </div>
      </>
    );
  }
  return <>{children}</>;
};

export default PrivacyLockButton;
