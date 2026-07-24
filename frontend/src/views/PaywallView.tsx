import React, { useState } from 'react';
import { Check, CreditCard, LogOut, Loader2 } from 'lucide-react';
import { apiClient } from '../api/client';
import type { BillingStatus } from '../api/client';
import type { AuthUser } from '../App';

interface PaywallViewProps {
  user: AuthUser | null;
  billing: BillingStatus;
  /** Just returned from Stripe Checkout — webhook confirmation pending. */
  activating?: boolean;
  onLogout: () => void;
}

const FEATURES = [
  'All bank & investment accounts in one dashboard',
  'Transaction search, tags and notes',
  'Capital gains & tax reports',
  'Connect Claude and other AI agents (MCP)',
  'Daily automatic sync from your institutions',
];

/** Full-screen subscription gate shown to logged-in users without an active
 *  subscription on BILLING=on deployments. Checkout and billing management
 *  both happen on Stripe-hosted pages (Apple Pay / Google Pay included). */
const PaywallView: React.FC<PaywallViewProps> = ({ user, billing, activating, onLogout }) => {
  const [plan, setPlan] = useState<'monthly' | 'annual'>('annual');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const startCheckout = async () => {
    setBusy(true);
    setError(null);
    try {
      window.location.href = await apiClient.billingCheckout(plan);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Could not start checkout');
      setBusy(false);
    }
  };

  const openPortal = async () => {
    setBusy(true);
    setError(null);
    try {
      window.location.href = await apiClient.billingPortal();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Could not open the billing portal');
      setBusy(false);
    }
  };

  if (activating) {
    return (
      <div className="flex h-screen items-center justify-center bg-slate-950 text-slate-300">
        <div className="text-center">
          <Loader2 className="w-8 h-8 mx-auto animate-spin text-blue-400" />
          <p className="mt-4 text-sm">Activating your subscription…</p>
          <p className="mt-1 text-xs text-slate-500">This usually takes a few seconds.</p>
        </div>
      </div>
    );
  }

  const expired = billing.status !== 'none';

  return (
    <div className="flex h-screen items-center justify-center bg-slate-950 text-slate-100 font-sans antialiased p-6">
      <div className="w-full max-w-md">
        <div className="text-center mb-8">
          <h1 className="text-2xl font-bold text-slate-100">WealthAgent</h1>
          <p className="mt-2 text-sm text-slate-400">
            {expired
              ? 'Your subscription has ended. Resubscribe to pick up right where you left off — your data is untouched.'
              : 'One plan, everything included.'}
          </p>
        </div>

        <div className="bg-slate-900 border border-slate-800 rounded-2xl p-6 shadow-2xl">
          {/* Plan toggle */}
          <div className="grid grid-cols-2 gap-2 mb-6">
            <button
              onClick={() => setPlan('monthly')}
              className={`rounded-xl border px-4 py-3 text-left transition-all ${
                plan === 'monthly'
                  ? 'border-blue-500 bg-blue-500/10'
                  : 'border-slate-700 bg-slate-800/50 hover:border-slate-600'
              }`}
            >
              <p className="text-xs text-slate-400">Monthly</p>
              <p className="text-lg font-bold text-slate-100">$8<span className="text-xs font-normal text-slate-400">/mo</span></p>
            </button>
            <button
              onClick={() => setPlan('annual')}
              className={`relative rounded-xl border px-4 py-3 text-left transition-all ${
                plan === 'annual'
                  ? 'border-blue-500 bg-blue-500/10'
                  : 'border-slate-700 bg-slate-800/50 hover:border-slate-600'
              }`}
            >
              <span className="absolute -top-2 right-2 text-[10px] font-bold bg-emerald-500/20 text-emerald-300 border border-emerald-500/30 rounded-full px-2 py-0.5">
                2 months free
              </span>
              <p className="text-xs text-slate-400">Annual</p>
              <p className="text-lg font-bold text-slate-100">$80<span className="text-xs font-normal text-slate-400">/yr · ≈$6.67/mo</span></p>
            </button>
          </div>

          <ul className="space-y-2 mb-6">
            {FEATURES.map(f => (
              <li key={f} className="flex items-start gap-2 text-sm text-slate-300">
                <Check className="w-4 h-4 mt-0.5 text-emerald-400 shrink-0" /> {f}
              </li>
            ))}
          </ul>

          <button
            onClick={startCheckout}
            disabled={busy}
            className="w-full flex items-center justify-center gap-2 px-4 py-3 bg-blue-600 hover:bg-blue-500 disabled:opacity-50 text-sm font-bold rounded-xl transition-all"
          >
            {busy ? <Loader2 className="w-4 h-4 animate-spin" /> : <CreditCard className="w-4 h-4" />}
            {expired ? 'Resubscribe' : 'Subscribe'} — {plan === 'monthly' ? '$8/month' : '$80/year'}
          </button>

          {error && <p className="mt-3 text-xs text-rose-400 text-center">{error}</p>}

          <p className="mt-4 text-[11px] text-slate-500 text-center">
            Secure checkout by Stripe · Apple Pay & Google Pay supported · Cancel anytime.
            Sales tax added where required.
          </p>

          {billing.has_customer && (
            <button
              onClick={openPortal}
              disabled={busy}
              className="mt-4 w-full text-xs text-slate-400 hover:text-slate-200 transition-colors"
            >
              Manage billing & invoices
            </button>
          )}
        </div>

        <div className="mt-6 flex items-center justify-center gap-2 text-xs text-slate-500">
          <span className="truncate">{user?.email}</span>
          <span>·</span>
          <button onClick={onLogout} className="flex items-center gap-1 hover:text-slate-300 transition-colors">
            <LogOut className="w-3 h-3" /> Sign out
          </button>
        </div>
      </div>
    </div>
  );
};

export default PaywallView;
