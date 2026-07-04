import React, { useState } from 'react';
import { Lock } from 'lucide-react';
import { UnlockDialog } from './PrivacyLockButton';

/** Sentinel the backend substitutes for encrypted text while the session is locked. */
export const LOCKED_SENTINEL = '[locked]';

export const isLocked = (v: string | null | undefined): boolean => v === LOCKED_SENTINEL;

/**
 * Renders a possibly-encrypted text value. Normally passes the text through;
 * when the backend returned the locked sentinel, shows a "🔒 Privacy Lock"
 * chip that opens the unlock dialog on click (unlock reloads the app so every
 * view refetches decrypted data; closing the dialog leaves the session locked).
 */
const LockedValue: React.FC<{ value?: string | null; className?: string }> = ({ value, className = '' }) => {
  const [showUnlock, setShowUnlock] = useState(false);

  if (!isLocked(value)) return <>{value}</>;

  return (
    <>
      <button
        onClick={e => { e.stopPropagation(); setShowUnlock(true); }}
        title="Encrypted with your Privacy Key. Click to unlock this session."
        className={`inline-flex items-center gap-1 px-1.5 py-0.5 bg-amber-500/10 hover:bg-amber-500/20 text-amber-300 border border-amber-500/30 rounded-md text-[10px] font-bold whitespace-nowrap transition-colors ${className}`}
      >
        <Lock className="w-3 h-3 shrink-0" /> Privacy Lock
      </button>
      {showUnlock && <UnlockDialog onClose={() => setShowUnlock(false)} />}
    </>
  );
};

export default LockedValue;
