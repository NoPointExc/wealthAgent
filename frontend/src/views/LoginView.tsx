import React, { useState } from 'react';
import { GoogleLogin } from '@react-oauth/google';
import type { CredentialResponse } from '@react-oauth/google';
import { Shield, AlertCircle, Loader2 } from 'lucide-react';
import { apiClient, ConsentRequiredError } from '../api/client';

interface Props {
  onLogin: () => void;
  /** If set, navigate here after a successful login instead of entering the app
   *  (used by the OAuth consent bridge to return to /authorize). */
  postLoginRedirect?: string;
  /** Demo instance: replace Google sign-in with a one-click "Try the demo". */
  demoMode?: boolean;
}

const SUPPORT_EMAIL = 'support@texasnetworth.com';
const TERMS_URL = 'https://www.texasnetworth.com/terms';
const PRIVACY_URL = 'https://www.texasnetworth.com/privacy';

const LoginView: React.FC<Props> = ({ onLogin, postLoginRedirect, demoMode }) => {
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [demoLoading, setDemoLoading] = useState(false);
  const [agreed, setAgreed] = useState(false);
  // Set when the server asks for first-time consent (HTTP 428). Holds the Google
  // credential so we can retry the same sign-in once the box is checked.
  const [pendingCredential, setPendingCredential] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const copyEmail = async () => {
    try {
      await navigator.clipboard.writeText(SUPPORT_EMAIL);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* clipboard unavailable — no-op */
    }
  };

  // Render the error message, turning any occurrence of the support email into
  // a click-to-copy highlight.
  const renderError = (msg: string): React.ReactNode => {
    const idx = msg.indexOf(SUPPORT_EMAIL);
    if (idx === -1) return msg;
    return (
      <>
        {msg.slice(0, idx)}
        <button
          type="button"
          onClick={copyEmail}
          title="Click to copy"
          className="font-semibold text-amber-100 underline decoration-dotted underline-offset-2 hover:text-white transition-colors"
        >
          {SUPPORT_EMAIL}
        </button>
        {msg.slice(idx + SUPPORT_EMAIL.length)}
        {copied && <span className="ml-1 font-medium text-emerald-300">Copied!</span>}
      </>
    );
  };

  const enterApp = (user: unknown) => {
    // Cookie is set by the server (HttpOnly) — only store display info locally
    localStorage.setItem('wealth_agent_user', JSON.stringify(user));
    if (postLoginRedirect) {
      window.location.replace(postLoginRedirect);
      return;
    }
    onLogin();
  };

  // Try to sign in. Returning users who already accepted are logged straight in;
  // a first-time user gets a 428, and we reveal the one-time consent step while
  // holding their credential so a single extra click finishes the sign-in.
  const attemptLogin = async (credential: string, accepted: boolean) => {
    setError(null);
    setSubmitting(true);
    try {
      const data = await apiClient.googleLogin(credential, accepted);
      enterApp(data.user);
    } catch (e: unknown) {
      if (e instanceof ConsentRequiredError) {
        setPendingCredential(credential);
      } else {
        setPendingCredential(null);
        setError(e instanceof Error ? e.message : 'Login failed');
      }
    } finally {
      setSubmitting(false);
    }
  };

  const handleSuccess = (response: CredentialResponse) => {
    if (!response.credential) return;
    attemptLogin(response.credential, false);
  };

  const handleDemo = async () => {
    setError(null);
    setDemoLoading(true);
    try {
      const data = await apiClient.demoLogin();
      enterApp(data.user);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Could not start the demo');
    } finally {
      setDemoLoading(false);
    }
  };

  return (
    <div className="flex h-screen items-center justify-center bg-slate-950">
      <div className="bg-slate-900 border border-slate-800 rounded-2xl p-10 flex flex-col items-center gap-6 w-full max-w-sm shadow-2xl">
        <div className="flex items-center gap-3">
          <Shield className="w-8 h-8 text-blue-400" />
          <span className="text-2xl font-bold text-slate-100 tracking-wider">WealthAgent</span>
        </div>
        <p className="text-sm text-slate-400 text-center leading-relaxed">
          {demoMode
            ? 'Explore WealthAgent with sample data — no sign-up.'
            : pendingCredential
              ? 'One quick step before we set up your account.'
              : 'Sign in to access your personal wealth dashboard'}
        </p>
        <div className="w-full flex flex-col items-center gap-3">
          {demoMode ? (
            <button
              onClick={handleDemo}
              disabled={demoLoading}
              className="w-full px-4 py-3 bg-blue-600 hover:bg-blue-500 disabled:opacity-50 text-sm font-bold text-white rounded-xl transition-all"
            >
              {demoLoading ? 'Starting demo…' : 'Try the demo'}
            </button>
          ) : pendingCredential ? (
            /* One-time consent step: shown only when the server (428) says this
               account hasn't accepted the current Terms yet. */
            <>
              <label className="flex items-start gap-2.5 w-full cursor-pointer select-none">
                <input
                  type="checkbox"
                  checked={agreed}
                  onChange={(e) => setAgreed(e.target.checked)}
                  className="mt-0.5 h-4 w-4 shrink-0 rounded border-slate-600 bg-slate-800 text-blue-500 focus:ring-blue-500 focus:ring-offset-slate-900 cursor-pointer"
                />
                <span className="text-xs text-slate-400 leading-relaxed">
                  I agree to the{' '}
                  <a href={TERMS_URL} target="_blank" rel="noopener noreferrer" className="text-blue-400 hover:text-blue-300 underline underline-offset-2">
                    Terms of Service
                  </a>{' '}
                  and{' '}
                  <a href={PRIVACY_URL} target="_blank" rel="noopener noreferrer" className="text-blue-400 hover:text-blue-300 underline underline-offset-2">
                    Privacy Policy
                  </a>.
                </span>
              </label>
              <button
                onClick={() => attemptLogin(pendingCredential, true)}
                disabled={!agreed || submitting}
                className="w-full flex items-center justify-center gap-2 px-4 py-3 bg-blue-600 hover:bg-blue-500 disabled:opacity-50 disabled:cursor-not-allowed text-sm font-bold text-white rounded-xl transition-all"
              >
                {submitting && <Loader2 className="w-4 h-4 animate-spin" />}
                Agree &amp; Continue
              </button>
              <button
                onClick={() => { setPendingCredential(null); setAgreed(false); setError(null); }}
                className="text-[11px] text-slate-500 hover:text-slate-300 transition-colors"
              >
                Use a different account
              </button>
            </>
          ) : (
            <>
              <GoogleLogin
                onSuccess={handleSuccess}
                onError={() => console.error('Google login failed')}
                theme="filled_black"
                shape="rectangular"
                size="large"
              />
              <p className="text-[11px] text-slate-500 text-center leading-relaxed">
                By continuing you agree to our{' '}
                <a href={TERMS_URL} target="_blank" rel="noopener noreferrer" className="text-slate-400 hover:text-slate-300 underline underline-offset-2">Terms</a>
                {' '}and{' '}
                <a href={PRIVACY_URL} target="_blank" rel="noopener noreferrer" className="text-slate-400 hover:text-slate-300 underline underline-offset-2">Privacy Policy</a>.
              </p>
            </>
          )}
        </div>
        {demoMode && (
          <p className="text-xs text-slate-500 text-center">
            Sample data from Plaid Sandbox. Your demo account is temporary and resets daily.
          </p>
        )}
        {error && (
          <div className="w-full flex items-start gap-2 p-3 bg-amber-500/10 border border-amber-500/30 rounded-lg text-xs text-amber-300">
            <AlertCircle className="w-4 h-4 shrink-0 mt-0.5" />
            <span>{renderError(error)}</span>
          </div>
        )}
      </div>
    </div>
  );
};

export default LoginView;
