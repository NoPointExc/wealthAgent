import React, { createContext, useContext, useState, useCallback, useEffect } from 'react';

const STORAGE_KEY = 'wealth_agent_privacy';

/** Placeholder shown in place of a hidden dollar amount. */
export const MONEY_MASK = '•••••';

interface PrivacyCtx {
  /** When true, sensitive dollar amounts are masked. */
  hidden: boolean;
  toggle: () => void;
  setHidden: (v: boolean) => void;
}

const Ctx = createContext<PrivacyCtx>({ hidden: true, toggle: () => {}, setHidden: () => {} });

export const PrivacyProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  // Default to hidden so a fresh session (e.g. a live demo) never leaks numbers
  // before the user explicitly reveals them.
  const [hidden, setHiddenState] = useState<boolean>(() => {
    const v = localStorage.getItem(STORAGE_KEY);
    return v === null ? true : v === '1';
  });

  useEffect(() => {
    localStorage.setItem(STORAGE_KEY, hidden ? '1' : '0');
  }, [hidden]);

  const toggle = useCallback(() => setHiddenState(h => !h), []);
  const setHidden = useCallback((v: boolean) => setHiddenState(v), []);

  return <Ctx.Provider value={{ hidden, toggle, setHidden }}>{children}</Ctx.Provider>;
};

export const usePrivacy = () => useContext(Ctx);
