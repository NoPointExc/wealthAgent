import React, { createContext, useContext, useEffect, useState } from 'react';
import { apiClient } from '../api/client';

interface ConfigCtx {
  /** Public no-login demo instance: Google login + bank linking disabled. */
  demoMode: boolean;
  /** Server-side PRIVACY_ENCRYPTION flag. */
  privacyEnabled: boolean;
  /** Server-side BILLING flag: subscription paywall active. */
  billingEnabled: boolean;
}

const defaults: ConfigCtx = { demoMode: false, privacyEnabled: false, billingEnabled: false };

const Ctx = createContext<ConfigCtx>(defaults);

/** Fetches the deployment's runtime config once so a single frontend image can
 *  serve both the prod and the demo instances. */
export const ConfigProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [config, setConfig] = useState<ConfigCtx>(defaults);

  useEffect(() => {
    apiClient.getConfig()
      .then(c => setConfig({
        demoMode: c.demo_mode,
        privacyEnabled: c.privacy_enabled,
        billingEnabled: c.billing_enabled ?? false,
      }))
      .catch(() => {}); // non-fatal: fall back to non-demo defaults
  }, []);

  return <Ctx.Provider value={config}>{children}</Ctx.Provider>;
};

export const useConfig = () => useContext(Ctx);
