import React from 'react';
import { Plus, Loader2, AlertCircle } from 'lucide-react';
import { usePlaidLinkFlow } from '../hooks/usePlaidLinkFlow';

interface PlaidLinkButtonProps {
  onSuccess: () => void;
}

/** Dropdown-row entry point for linking an institution (secondary to the
 *  in-grid "+" tiles). Shares the lazy Plaid flow, so it costs no token until
 *  clicked. */
const PlaidLinkButton: React.FC<PlaidLinkButtonProps> = ({ onSuccess }) => {
  const { open, busy, error } = usePlaidLinkFlow(onSuccess);

  if (error) {
    return (
      <div className="px-4 py-2 text-xs text-rose-400 flex items-start gap-2">
        <AlertCircle className="w-3.5 h-3.5 shrink-0 mt-0.5" />
        <span>Link failed: {error}</span>
      </div>
    );
  }

  return (
    <button
      onClick={() => open()}
      disabled={busy}
      className="flex items-center gap-2 px-4 py-2 w-full hover:bg-slate-800 disabled:opacity-50 text-xs text-slate-300 transition-all text-left"
    >
      {busy ? <Loader2 className="w-4 h-4 animate-spin" /> : <Plus className="w-4 h-4" />}
      {busy ? 'Linking…' : 'Link New Institution'}
    </button>
  );
};

export default PlaidLinkButton;
