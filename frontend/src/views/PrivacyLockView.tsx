import React, { useState } from 'react';
import { Lock, KeyRound, Database, Monitor } from 'lucide-react';
import PrivacyEncryptionCard from '../components/PrivacyEncryptionCard';

/**
 * Dedicated Privacy Lock tab: explains how the Privacy Key works (animated
 * data-flow diagram + an interactive operator-vs-you demo), then hosts the
 * setup / unlock / lock management card.
 */
const PrivacyLockView: React.FC = () => (
  <div className="max-w-3xl mx-auto space-y-8">
    <div>
      <h2 className="text-xl font-black text-slate-100 flex items-center gap-2">
        <Lock className="w-5 h-5 text-blue-400" /> Privacy Lock
      </h2>
      <p className="text-sm text-slate-400 mt-1">
        Encrypt the identifying details of your finances with a <b className="text-slate-200">Privacy
        Key</b> only you hold. Whoever runs this server — even with full database access — sees
        ciphertext, not your merchants, descriptions, or notes.
      </p>
    </div>

    <FlowDiagram />
    <PrivacyEncryptionCard />

    <div className="text-[11px] text-slate-500 space-y-1.5">
      <p className="font-bold text-slate-400 uppercase tracking-wider text-[10px]">How it works, honestly</p>
      <p>
        Your Privacy Key is separate from your Google login and never stored anywhere. It protects a
        per-account encryption keypair: the <i>public</i> half lets the server encrypt new bank data even
        while you're away (that's how the nightly sync works); the <i>private</i> half — which alone can
        decrypt — is kept only in wrapped form that your Privacy Key opens. When you unlock, the server
        decrypts in memory for up to 12 hours, then forgets. AI agents connected after you enable get
        their own sealed copy that only their token can open — they never ask for your key.
      </p>
      <p>
        This protects what's <i>stored</i>: the database, disk, and backups hold only ciphertext. The
        server still handles readable data in memory while syncing from your bank and serving your
        session — so it defends against a curious operator or a stolen database, not a fully
        compromised server.
      </p>
    </div>
  </div>
);

const DEMO_ROWS: { date: string; plain: string; cipher: string; amount: string }[] = [
  { date: '2026-06-10', plain: 'Starbucks',           cipher: '8c1eeb33f1e2c1bd…', amount: '$4.33' },
  { date: '2026-06-11', plain: 'Touchstone Climbing', cipher: '6d22626ca7430252…', amount: '$78.50' },
  { date: '2026-06-28', plain: 'United Airlines',     cipher: 'b028befc3a86ecc2…', amount: '$500.00' },
];

/** "What happens to your data": an animated You ←(Privacy Key)— Database lane,
 *  plus the same rows viewed from the database vs with the correct key. */
const FlowDiagram: React.FC = () => {
  const [asDatabase, setAsDatabase] = useState(true);
  return (
    <div className="bg-slate-900 border border-slate-800 rounded-2xl overflow-hidden select-none">
      <style>{`
        @keyframes wa-key { 0%, 38% { transform: scale(1); } 44% { transform: scale(1.35); } 50%, 100% { transform: scale(1); } }
        @keyframes wa-lane {
          0%   { left: 76%; opacity: 0; }
          6%   { left: 76%; opacity: 1; }
          38%  { left: 41%; }
          50%  { left: 41%; }
          88%  { left: 6%;  opacity: 1; }
          94%  { left: 6%;  opacity: 0; }
          100% { left: 6%;  opacity: 0; }
        }
        @keyframes wa-cipher { 0%, 44% { opacity: 1; } 50%, 100% { opacity: 0; } }
        @keyframes wa-plain  { 0%, 44% { opacity: 0; } 50%, 100% { opacity: 1; } }
      `}</style>

      <div className="p-5 pb-4">
        <p className="text-[10px] font-bold text-slate-500 uppercase tracking-wider mb-4">
          What happens to your data
        </p>

        <div className="mb-1 flex justify-between text-[10px] text-slate-500 font-bold">
          <span className="flex items-center gap-1 w-24"><Monitor className="w-3.5 h-3.5" /> You</span>
          <span className="flex items-center gap-1 text-blue-400"><KeyRound className="w-3.5 h-3.5" /> Your Privacy Key unlocks</span>
          <span className="flex items-center gap-1 w-24 justify-end"><Database className="w-3.5 h-3.5" /> Database</span>
        </div>
        <div className="relative h-10">
          <div className="absolute inset-x-2 top-1/2 border-t border-dashed border-slate-700" />
          <div
            className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 z-10 text-blue-400"
            style={{ animation: 'wa-key 6s linear infinite' }}
          >
            <KeyRound className="w-4 h-4" />
          </div>
          <div
            className="absolute top-1/2 -translate-y-1/2 z-20"
            style={{ animation: 'wa-lane 6s linear infinite', left: '76%' }}
          >
            <div className="relative whitespace-nowrap rounded-md border border-slate-700 bg-slate-950 px-2 py-1 text-[10px] font-mono">
              <span className="text-fuchsia-400" style={{ animation: 'wa-cipher 6s linear infinite' }}>
                8c1e…74d0 · $4.33
              </span>
              <span
                className="absolute inset-0 flex items-center justify-center text-emerald-300"
                style={{ animation: 'wa-plain 6s linear infinite', opacity: 0 }}
              >
                Starbucks · $4.33
              </span>
            </div>
          </div>
        </div>
      </div>

      {/* Same rows, two viewers */}
      <div className="px-4 py-3 border-y border-slate-800/60 flex items-center justify-between gap-2">
        <span className="text-[10px] font-bold text-slate-500 uppercase tracking-wider">
          Same rows, two viewers
        </span>
        <div className="flex rounded-lg overflow-hidden border border-slate-700 text-[11px] font-bold">
          <button
            onClick={() => setAsDatabase(true)}
            className={`flex items-center gap-1.5 px-3 py-1.5 transition-colors ${asDatabase ? 'bg-fuchsia-500/20 text-fuchsia-300' : 'text-slate-500 hover:text-slate-300'}`}
          >
            <Database className="w-3.5 h-3.5" /> Database
          </button>
          <button
            onClick={() => setAsDatabase(false)}
            className={`flex items-center gap-1.5 px-3 py-1.5 transition-colors ${!asDatabase ? 'bg-emerald-500/20 text-emerald-300' : 'text-slate-500 hover:text-slate-300'}`}
          >
            <KeyRound className="w-3.5 h-3.5" /> You with the correct Privacy Key
          </button>
        </div>
      </div>
      <table className="w-full text-[11px]">
        <thead>
          <tr className="text-left text-slate-600 border-b border-slate-800/60">
            <th className="px-4 py-2 font-bold">Date</th>
            <th className="px-4 py-2 font-bold">Description</th>
            <th className="px-4 py-2 font-bold text-right">Amount</th>
          </tr>
        </thead>
        <tbody>
          {DEMO_ROWS.map(r => (
            <tr key={r.date} className="border-b border-slate-800/40 last:border-0">
              <td className="px-4 py-2 text-slate-400">{r.date}</td>
              <td className={`px-4 py-2 font-mono ${asDatabase ? 'text-fuchsia-400' : 'text-emerald-300'}`}>
                {asDatabase ? r.cipher : r.plain}
              </td>
              <td className="px-4 py-2 text-right text-slate-300">{r.amount}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="px-4 py-2.5 text-[11px] text-slate-500 border-t border-slate-800/60">
        {asDatabase
          ? 'This is what the database holds — descriptions are unreadable ciphertext. Amounts and dates stay visible so your charts and totals still work.'
          : 'With your Privacy Key, the same rows decrypt back into readable data — only for you.'}
      </p>
    </div>
  );
};

export default PrivacyLockView;
