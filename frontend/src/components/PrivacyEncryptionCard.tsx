import React, { useState, useEffect, useCallback } from 'react';
import { Lock, LockOpen, ShieldCheck, AlertTriangle, Loader2, ChevronDown, ChevronRight } from 'lucide-react';
import { apiClient } from '../api/client';
import type { PrivacyStatus, PrivacySetupResult } from '../api/client';

/** Fired whenever encryption state changes so other parts of the app can react. */
export const PRIVACY_CHANGED_EVENT = 'privacy-encryption:changed';

function notifyChanged() {
  window.dispatchEvent(new CustomEvent(PRIVACY_CHANGED_EVENT));
}

/**
 * Management card for Privacy Lock (operator-blind encryption): opt-in setup
 * with a Privacy Key, and lock/unlock for the current server session. Renders
 * a notice when the server runs with PRIVACY_ENCRYPTION=off.
 */
const PrivacyEncryptionCard: React.FC = () => {
  const [status, setStatus] = useState<PrivacyStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setStatus(await apiClient.getPrivacyStatus());
    } catch {
      // Older backend without the endpoint — treat as feature-off.
      setStatus({ enabled: false, configured: false, unlocked: false });
    }
  }, []);

  useEffect(() => { load(); }, [load]);
  useEffect(() => {
    window.addEventListener(PRIVACY_CHANGED_EVENT, load);
    return () => window.removeEventListener(PRIVACY_CHANGED_EVENT, load);
  }, [load]);

  if (!status) return null;

  return (
    <div className="bg-slate-900 border border-slate-800 rounded-2xl overflow-hidden">
      <div className="px-4 py-3 border-b border-slate-800/60 flex items-center justify-between">
        <span className="text-xs font-black text-slate-200 uppercase tracking-wider flex items-center gap-2">
          <ShieldCheck className="w-4 h-4 text-slate-400" /> Privacy Lock
        </span>
        {status.configured && (
          <span className={`flex items-center gap-1 text-[10px] font-bold ${status.unlocked ? 'text-emerald-400' : 'text-amber-400'}`}>
            <Lock className="w-3 h-3" />
            {status.unlocked ? 'On · readable this session' : 'On · locked'}
          </span>
        )}
      </div>

      <div className="p-4 space-y-3">
        {error && (
          <div className="p-2 bg-red-950 border border-red-800 rounded text-[11px] text-red-300">{error}</div>
        )}
        {!status.enabled
          ? <p className="text-[11px] text-slate-500">
              Privacy Lock is not enabled on this server. The operator can turn it on by setting{' '}
              <code className="text-slate-300">PRIVACY_ENCRYPTION=on</code>.
            </p>
          : !status.configured
            ? <SetupSection onError={setError} onDone={notifyChanged} />
            : status.unlocked
              ? <UnlockedSection onError={setError} />
              : <LockedSection onError={setError} />}
      </div>
    </div>
  );
};

const CONSEQUENCES: [string, string][] = [
  ['No recovery — ever.',
   'Your Privacy Key is never stored anywhere. If you forget it, nobody — including the server operator — can decrypt your data. The only way back is wiping and re-syncing from your bank, which loses your tags, notes, and manual cost-basis entries.'],
  ["You can't change the key later.",
   'There is currently no way to change or rotate your Privacy Key once set. Pick something you can remember (a password manager helps).'],
  ['What gets encrypted:',
   'Transaction descriptions, merchant names, your notes, and account names. Amounts, dates, tags, categories, holdings, and investment trades stay unencrypted so charts, totals, and tax math keep working.'],
  ['Connected AI agents need reconnecting.',
   'Agent tokens created before you enable Privacy Lock cannot decrypt — disconnect and reconnect each agent afterwards so it gets a fresh token.'],
  ['Auto-locks after 12 hours (and on server restart).',
   "You'll re-enter your Privacy Key from time to time. While locked, encrypted fields show as [locked] and text search is unavailable."],
];

const SetupSection: React.FC<{ onError: (e: string | null) => void; onDone: () => void }> = ({ onError, onDone }) => {
  const [open, setOpen] = useState(false);
  const [pass, setPass] = useState('');
  const [confirmPass, setConfirmPass] = useState('');
  const [acked, setAcked] = useState(false);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<PrivacySetupResult | null>(null);

  const unmet =
    pass.length < 12 ? 'Privacy Key must be at least 12 characters.'
    : !confirmPass ? 'Re-type your Privacy Key in the confirm field.'
    : pass !== confirmPass ? "Privacy Keys don't match."
    : !acked ? 'Check the box to confirm you understand the consequences.'
    : null;
  const canSubmit = !unmet && !busy;

  const submit = async () => {
    if (!canSubmit) return;
    setBusy(true);
    onError(null);
    try {
      setResult(await apiClient.privacySetup(pass));
      setPass('');
      setConfirmPass('');
      onDone();
    } catch (e) {
      onError(e instanceof Error ? e.message : 'Setup failed');
    } finally {
      setBusy(false);
    }
  };

  if (result) {
    return (
      <div className="p-3 bg-emerald-950 border border-emerald-700 rounded-lg space-y-1.5">
        <p className="text-xs font-bold text-emerald-300 flex items-center gap-1.5">
          <ShieldCheck className="w-4 h-4" /> Privacy Lock is on
        </p>
        <p className="text-[11px] text-emerald-400/80">
          Encrypting {result.sealed_transactions} transactions, {result.sealed_holdings} holdings, and{' '}
          {result.sealed_accounts} accounts (finishes in the background). New data from bank syncs
          is encrypted automatically.
        </p>
        <p className="text-[11px] text-amber-400/90 flex items-start gap-1.5">
          <AlertTriangle className="w-3.5 h-3.5 shrink-0 mt-0.5" />
          Reconnect your AI agents now — tokens created before this moment cannot decrypt.
        </p>
      </div>
    );
  }

  return (
    <>
      <p className="text-[11px] text-slate-500">
        Set a <b className="text-slate-300">Privacy Key</b> — separate from your Google login — and the
        identifying details of your finances are encrypted so that not even the server operator can read
        them from the database.
      </p>
      {!open ? (
        <button
          onClick={() => setOpen(true)}
          className="flex items-center gap-1.5 px-3 py-2 text-xs font-bold rounded-lg bg-blue-600 hover:bg-blue-500 text-white transition-colors"
        >
          <Lock className="w-3.5 h-3.5" /> Set up Privacy Lock
        </button>
      ) : (
        <div className="space-y-3">
          <div className="p-3 bg-slate-950 border border-amber-500/30 rounded-lg space-y-2">
            <p className="text-[11px] font-bold text-amber-300 uppercase tracking-wider flex items-center gap-1.5">
              <AlertTriangle className="w-3.5 h-3.5" /> Before you enable — read this
            </p>
            <ul className="space-y-1.5">
              {CONSEQUENCES.map(([title, body]) => (
                <li key={title} className="text-[11px] text-slate-400">
                  <b className="text-slate-200">{title}</b> {body}
                </li>
              ))}
            </ul>
          </div>
          <input
            type="password"
            value={pass}
            onChange={e => setPass(e.target.value)}
            placeholder="Privacy Key (min 12 characters)"
            className="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-xs text-slate-100 placeholder-slate-600 focus:outline-none focus:ring-1 focus:ring-blue-500"
          />
          <input
            type="password"
            value={confirmPass}
            onChange={e => setConfirmPass(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && submit()}
            placeholder="Confirm Privacy Key"
            className="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-xs text-slate-100 placeholder-slate-600 focus:outline-none focus:ring-1 focus:ring-blue-500"
          />
          <label className="flex items-start gap-2 text-[11px] text-amber-400/90 cursor-pointer">
            <input
              type="checkbox"
              checked={acked}
              onChange={e => setAcked(e.target.checked)}
              className="mt-0.5 accent-amber-500"
            />
            <span>
              I understand: my Privacy Key <b>cannot be recovered or changed</b>, losing it means losing
              my tags and notes, and I must reconnect my AI agents after enabling.
            </span>
          </label>
          {(pass || confirmPass) && unmet && (
            <p className="text-[11px] text-amber-400/90">{unmet}</p>
          )}
          <button
            onClick={submit}
            disabled={!canSubmit}
            className="flex items-center gap-1.5 px-3 py-2 text-xs font-bold rounded-lg bg-blue-600 hover:bg-blue-500 disabled:opacity-50 disabled:cursor-not-allowed text-white transition-colors"
          >
            {busy ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Lock className="w-3.5 h-3.5" />}
            {busy ? 'Encrypting your data…' : 'Encrypt my data'}
          </button>
        </div>
      )}
    </>
  );
};

/** Two-line status making the at-rest vs session distinction explicit: the
 *  database is always encrypted once a Privacy Key exists; lock/unlock only
 *  changes whether this session can read it. */
const StateRows: React.FC<{ session: 'readable' | 'locked' }> = ({ session }) => (
  <div className="rounded-lg bg-slate-950 border border-slate-800 divide-y divide-slate-800/60 text-[11px]">
    <div className="flex items-center justify-between px-3 py-2">
      <span className="text-slate-400">Database on the server</span>
      <span className="font-bold text-emerald-400 flex items-center gap-1">
        <Lock className="w-3 h-3" /> Always encrypted
      </span>
    </div>
    <div className="flex items-center justify-between px-3 py-2">
      <span className="text-slate-400">This session (what you see)</span>
      {session === 'readable' ? (
        <span className="font-bold text-emerald-300 flex items-center gap-1">
          <LockOpen className="w-3 h-3" /> Readable — key unlocked
        </span>
      ) : (
        <span className="font-bold text-amber-400 flex items-center gap-1">
          <Lock className="w-3 h-3" /> Locked — key required
        </span>
      )}
    </div>
  </div>
);

const UnlockedSection: React.FC<{ onError: (e: string | null) => void }> = ({ onError }) => {
  const [busy, setBusy] = useState(false);
  const [showDetail, setShowDetail] = useState(false);

  const lock = async () => {
    setBusy(true);
    onError(null);
    try {
      await apiClient.privacyLock();
      // The user just chose to lock — don't greet them with an unlock prompt.
      // The reload (so mounted views drop decrypted data) lands in locked mode.
      sessionStorage.setItem('wa_privacy_just_locked', '1');
      window.location.reload();
    } catch (e) {
      onError(e instanceof Error ? e.message : 'Lock failed');
      setBusy(false);
    }
  };

  return (
    <>
      <StateRows session="readable" />
      <p className="text-[11px] text-slate-500">
        "Locking" only affects this session — your database stays encrypted either way. The session
        re-locks by itself after 12 hours; lock it now if you're stepping away.
      </p>
      <button
        onClick={lock}
        disabled={busy}
        className="flex items-center gap-1.5 px-3 py-2 text-xs font-bold rounded-lg bg-slate-800 hover:bg-slate-700 disabled:opacity-50 text-slate-200 transition-colors"
      >
        {busy ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Lock className="w-3.5 h-3.5" />}
        Lock this session now
      </button>
      <div>
        <button
          onClick={() => setShowDetail(v => !v)}
          className="flex items-center gap-1.5 text-[11px] font-bold text-slate-500 hover:text-slate-300"
        >
          {showDetail ? <ChevronDown className="w-3.5 h-3.5" /> : <ChevronRight className="w-3.5 h-3.5" />}
          What's encrypted?
        </button>
        {showDetail && (
          <p className="mt-1.5 text-[11px] text-slate-500">
            Encrypted: transaction descriptions, merchant names, notes, account names.
            Not encrypted (so charts, totals, and tax math keep working): amounts, dates, tags,
            holdings, and investment transactions. Agents connected with a token created while
            unlocked can decrypt; there is no Privacy Key recovery.
          </p>
        )}
      </div>
    </>
  );
};

const LockedSection: React.FC<{ onError: (e: string | null) => void }> = ({ onError }) => {
  const [pass, setPass] = useState('');
  const [busy, setBusy] = useState(false);

  const unlock = async () => {
    if (!pass || busy) return;
    setBusy(true);
    onError(null);
    try {
      await apiClient.privacyUnlock(pass);
      // Reload so every mounted view refetches decrypted data.
      window.location.reload();
    } catch (e) {
      onError(e instanceof Error ? e.message : 'Unlock failed');
      setBusy(false);
    }
  };

  return (
    <>
      <StateRows session="locked" />
      <p className="text-[11px] text-slate-500">
        Encrypted fields show as <code className="text-slate-300">[locked]</code> and text search is
        unavailable until you unlock this session with your Privacy Key.
      </p>
      <div className="flex gap-2">
        <input
          type="password"
          value={pass}
          onChange={e => setPass(e.target.value)}
          onKeyDown={e => e.key === 'Enter' && unlock()}
          placeholder="Privacy Key"
          className="flex-1 bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-xs text-slate-100 placeholder-slate-600 focus:outline-none focus:ring-1 focus:ring-amber-500"
        />
        <button
          onClick={unlock}
          disabled={!pass || busy}
          className="flex items-center gap-1.5 px-3 py-2 text-xs font-bold rounded-lg bg-amber-600 hover:bg-amber-500 disabled:opacity-50 disabled:cursor-not-allowed text-white transition-colors shrink-0"
        >
          {busy ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <LockOpen className="w-3.5 h-3.5" />}
          Unlock
        </button>
      </div>
    </>
  );
};

export default PrivacyEncryptionCard;
