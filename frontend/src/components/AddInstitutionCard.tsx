import React from 'react';
import { Plus, Loader2, AlertCircle } from 'lucide-react';
import { usePlaidLinkFlow } from '../hooks/usePlaidLinkFlow';

interface AddInstitutionCardProps {
  /** Re-fetch accounts once a new institution is linked. */
  onLinked: () => void;
}

/**
 * Dashed "+" tile rendered at the end of each Portfolio grid. The primary,
 * discoverable way to link a bank — one Plaid flow imports whichever accounts
 * (assets and/or liabilities) the institution returns, so it lives in both
 * grids and the label is institution-level, not "add asset".
 */
const AddInstitutionCard: React.FC<AddInstitutionCardProps> = ({ onLinked }) => {
  const { open, busy, error } = usePlaidLinkFlow(onLinked);

  return (
    <button
      onClick={() => open()}
      disabled={busy}
      className="border-2 border-dashed border-slate-700 hover:border-blue-500/60 hover:bg-blue-500/5 disabled:opacity-60 disabled:cursor-wait p-3 rounded-xl transition-all group h-full min-h-[96px] flex flex-col items-center justify-center gap-1.5 text-center"
    >
      {busy ? (
        <Loader2 className="w-5 h-5 text-slate-400 animate-spin" />
      ) : error ? (
        <AlertCircle className="w-5 h-5 text-rose-400" />
      ) : (
        <div className="w-8 h-8 rounded-full bg-slate-800 group-hover:bg-blue-500/15 flex items-center justify-center transition-colors">
          <Plus className="w-4 h-4 text-slate-400 group-hover:text-blue-400" />
        </div>
      )}
      <span className={`text-[11px] font-bold ${error ? 'text-rose-400' : 'text-slate-400 group-hover:text-slate-200'}`}>
        {busy ? 'Connecting…' : error ? 'Link failed — retry' : 'Link institution'}
      </span>
      {error && <span className="text-[9px] text-rose-400/70 px-1 line-clamp-2">{error}</span>}
    </button>
  );
};

export default AddInstitutionCard;
