import React, { useState, useEffect, useCallback, useRef } from 'react';
import {
  Terminal, MonitorSmartphone, MessageSquare, MousePointer2,
  Plug, Copy, Check, ChevronLeft, ShieldCheck, Trash2,
  AlertTriangle, ChevronDown, ChevronRight, Loader2, RefreshCw, Plus,
} from 'lucide-react';
import { apiClient } from '../api/client';
import type { ApiToken, CreateTokenResponse, OAuthGrant } from '../types/models';

// Derived from the page origin so self-hosted deployments hand out their own
// URL in connect snippets. Note for agents on other machines: this is the URL
// as *your browser* reaches the app — if you browse via a tunnel/proxy,
// substitute the address the agent's machine can reach.
const MCP_URL = `${window.location.origin}/mcp`;

type InstallKind = 'terminal' | 'json' | 'clickpath' | 'oauth';

interface ClientDef {
  id: string;
  label: string;
  sub: string;
  icon: React.ComponentType<{ className?: string }>;
  installKind: InstallKind;
  /** Path/location the JSON snippet goes into (json clients). */
  configPath?: string;
  /** Click-path / OAuth connector steps. */
  steps?: string[];
  /** Produces the copy-paste snippet from a raw token. Omitted for OAuth clients,
   *  which authenticate by sign-in rather than a pasted token. */
  snippet?: (token: string) => string;
  /** Optional secondary install path shown as a collapsible fallback. */
  alt?: {
    label: string;
    configPath: string;
    /** Static fallback snippet — uses a token placeholder (no token minted for
     *  OAuth clients). */
    text: string;
  };
}

const CLIENTS: ClientDef[] = [
  {
    id: 'claude-code',
    label: 'Claude Code',
    sub: 'CLI',
    icon: Terminal,
    installKind: 'terminal',
    snippet: (t) =>
      `claude mcp add wealth ${MCP_URL} --transport http --header "Authorization: Bearer ${t}"`,
  },
  {
    id: 'claude-desktop',
    label: 'Claude Desktop',
    sub: 'Mac / Windows',
    icon: MonitorSmartphone,
    installKind: 'oauth',
    steps: [
      'Open Claude Desktop → Settings → Connectors',
      'Click "Add custom connector"',
      'Name it "WealthAgent" and paste the URL below',
      'Click Connect — a WealthAgent sign-in window opens',
      'Sign in with Google and click Authorize, then start a new chat',
    ],
    alt: {
      label: 'On an older Claude Desktop without the Connectors menu, or prefer a manual token? Add this to your config file (create a token under "Connected agents → Advanced" below and paste it in place of YOUR_TOKEN):',
      configPath: 'claude_desktop_config.json  (Settings → Developer → Edit Config)',
      text: JSON.stringify(
        {
          mcpServers: {
            wealth: {
              command: 'npx',
              args: ['-y', 'mcp-remote', MCP_URL, '--header', 'Authorization: Bearer YOUR_TOKEN'],
            },
          },
        },
        null,
        2,
      ),
    },
  },
  {
    id: 'cursor',
    label: 'Cursor',
    sub: 'Editor',
    icon: MousePointer2,
    installKind: 'json',
    configPath: '~/.cursor/mcp.json',
    snippet: (t) =>
      JSON.stringify(
        {
          mcpServers: {
            wealth: {
              url: MCP_URL,
              headers: { Authorization: `Bearer ${t}` },
            },
          },
        },
        null,
        2,
      ),
  },
  {
    id: 'chatgpt-desktop',
    label: 'ChatGPT Desktop',
    sub: 'Mac / Windows',
    icon: MessageSquare,
    installKind: 'oauth',
    steps: [
      'Open ChatGPT Desktop → Settings → Connectors',
      'Click "Add" → choose "MCP Server"',
      'Server URL: paste the URL below',
      'Choose OAuth authentication and click Connect',
      'Sign in with Google and click Authorize, then start a new chat',
    ],
  },
];

function fmtDate(iso: string | null): string {
  if (!iso) return '—';
  return new Date(iso).toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' });
}

function relTime(iso: string | null): string {
  if (!iso) return 'never';
  const s = Math.floor((Date.now() - new Date(iso).getTime()) / 1000);
  if (s < 5) return 'just now';
  if (s < 60) return `${s}s ago`;
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
  return `${Math.floor(s / 86400)}d ago`;
}

const BOOTSTRAP_PROMPT = `You have a WealthAgent MCP server with my real financial data.
Before answering any financial question, call wealth_whoami, then use the
read tools (wealth_accounts_list, wealth_tx_list, wealth_gains) to gather
facts. All money is in integer cents; dates are YYYY-MM-DD. Confirm with me
before any write or cost-basis change.

Now: <my question>`;

/** Small copy-to-clipboard button with a clipboard-API fallback. */
const CopyButton: React.FC<{ text: string; label?: string; className?: string }> = ({
  text,
  label = 'Copy',
  className = '',
}) => {
  const [copied, setCopied] = useState(false);
  const onCopy = async () => {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      // Fallback for browsers without the async clipboard API.
      const ta = document.createElement('textarea');
      ta.value = text;
      ta.style.position = 'fixed';
      ta.style.opacity = '0';
      document.body.appendChild(ta);
      ta.select();
      try { document.execCommand('copy'); } catch { /* user can select manually */ }
      document.body.removeChild(ta);
    }
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };
  return (
    <button
      onClick={onCopy}
      className={`flex items-center gap-1.5 px-3 py-2 text-xs font-bold rounded-lg transition-colors shrink-0 ${className || 'bg-blue-600 hover:bg-blue-500 text-white'}`}
    >
      {copied ? <Check className="w-3.5 h-3.5" /> : <Copy className="w-3.5 h-3.5" />}
      {copied ? 'Copied' : label}
    </button>
  );
};

const ConnectView: React.FC = () => {
  const [tokens, setTokens] = useState<ApiToken[]>([]);
  const [grants, setGrants] = useState<OAuthGrant[]>([]);
  const [selected, setSelected] = useState<ClientDef | null>(null);
  const [minted, setMinted] = useState<CreateTokenResponse | null>(null);
  const [minting, setMinting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showBootstrap, setShowBootstrap] = useState(false);
  const pollRef = useRef<number | null>(null);

  const loadTokens = useCallback(async () => {
    try {
      const [t, g] = await Promise.all([apiClient.listTokens(), apiClient.listOAuthGrants()]);
      setTokens(t);
      setGrants(g);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to load connected agents');
    }
  }, []);

  useEffect(() => { loadTokens(); }, [loadTokens]);

  // The connection status for the just-minted token, derived from the live list.
  const mintedRow = minted ? tokens.find(t => t.id === minted.id) : undefined;
  const connected = !!mintedRow?.last_used_at;

  // Poll for first use after minting, until connected or a 2-minute timeout.
  useEffect(() => {
    if (!minted || connected) {
      if (pollRef.current) { window.clearInterval(pollRef.current); pollRef.current = null; }
      return;
    }
    const started = Date.now();
    pollRef.current = window.setInterval(() => {
      if (Date.now() - started > 120_000) {
        if (pollRef.current) { window.clearInterval(pollRef.current); pollRef.current = null; }
        return;
      }
      loadTokens();
    }, 3000);
    return () => {
      if (pollRef.current) { window.clearInterval(pollRef.current); pollRef.current = null; }
    };
  }, [minted, connected, loadTokens]);

  const pickClient = async (client: ClientDef) => {
    setSelected(client);
    setMinted(null);
    setError(null);
    setShowBootstrap(false);
    // OAuth clients sign in through the connector — no token is minted here.
    if (client.installKind === 'oauth') {
      setMinting(false);
      return;
    }
    setMinting(true);
    try {
      const name = `${client.label} · ${new Date().toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' })}`;
      const res = await apiClient.createToken(name); // default scope: read,write,sync
      setMinted(res);
      await loadTokens();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to create token');
    } finally {
      setMinting(false);
    }
  };

  const back = () => { setSelected(null); setMinted(null); setError(null); };

  const handleRevoke = async (id: string, name: string) => {
    if (!confirm(`Disconnect "${name}"? That agent will immediately lose access.`)) return;
    try {
      await apiClient.revokeToken(id);
      await loadTokens();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to revoke');
    }
  };

  const handleRevokeGrant = async (clientId: string, name: string) => {
    if (!confirm(`Disconnect "${name}"? It will lose access within the hour (and can't refresh).`)) return;
    try {
      await apiClient.revokeOAuthGrant(clientId);
      await loadTokens();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to disconnect');
    }
  };

  return (
    <div className="max-w-3xl mx-auto space-y-8">
      {!selected && (
        <>
          <div>
            <h2 className="text-xl font-black text-slate-100 flex items-center gap-2">
              <Plug className="w-5 h-5 text-blue-400" /> Connect your AI
            </h2>
            <p className="text-sm text-slate-400 mt-1">
              Give your AI assistant secure, read-write access to your accounts in under two minutes.
              Pick the app you use:
            </p>
          </div>

          {error && (
            <div className="p-3 bg-red-950 border border-red-800 rounded-lg text-sm text-red-300">{error}</div>
          )}

          {/* Client picker */}
          <div className="grid grid-cols-2 gap-3">
            {CLIENTS.map(c => (
              <button
                key={c.id}
                onClick={() => pickClient(c)}
                className="flex items-center gap-3 p-4 bg-slate-900 border border-slate-800 hover:border-blue-500/50 hover:bg-slate-800/60 rounded-2xl text-left transition-all group"
              >
                <div className="w-10 h-10 rounded-xl bg-slate-800 group-hover:bg-blue-500/10 flex items-center justify-center shrink-0 transition-colors">
                  <c.icon className="w-5 h-5 text-slate-300 group-hover:text-blue-400" />
                </div>
                <div className="min-w-0">
                  <div className="text-sm font-bold text-slate-100">{c.label}</div>
                  <div className="text-[11px] text-slate-500">{c.sub}</div>
                </div>
                <ChevronRight className="w-4 h-4 text-slate-600 ml-auto group-hover:text-blue-400" />
              </button>
            ))}
          </div>

          <p className="text-[11px] text-slate-500">
            Using ChatGPT on the web or the Gemini app? Those don't support custom MCP servers yet —
            use the desktop / CLI versions above. More clients coming soon.
          </p>

          <ManagePanel
            tokens={tokens}
            grants={grants}
            onRevoke={handleRevoke}
            onRevokeGrant={handleRevokeGrant}
            onRefresh={loadTokens}
            onCreate={async (name) => {
              const res = await apiClient.createToken(name);
              await loadTokens();
              return res;
            }}
          />
        </>
      )}

      {selected && (
        <div className="space-y-6">
          <button onClick={back} className="flex items-center gap-1.5 text-xs font-bold text-slate-400 hover:text-slate-200">
            <ChevronLeft className="w-4 h-4" /> All clients
          </button>

          <div className="flex items-center gap-3">
            <div className="w-11 h-11 rounded-xl bg-blue-500/10 flex items-center justify-center shrink-0">
              <selected.icon className="w-6 h-6 text-blue-400" />
            </div>
            <div>
              <h2 className="text-lg font-black text-slate-100">Connect {selected.label}</h2>
              <p className="text-xs text-slate-500">One paste into the app — then start chatting.</p>
            </div>
          </div>

          {error && (
            <div className="p-3 bg-red-950 border border-red-800 rounded-lg text-sm text-red-300">{error}</div>
          )}

          {minting && (
            <div className="flex items-center gap-2 text-sm text-slate-400 p-4">
              <Loader2 className="w-4 h-4 animate-spin" /> Generating your secure token…
            </div>
          )}

          {selected.installKind === 'oauth' && <OAuthConnectCard client={selected} />}

          {minted && (
            <>
              {/* Step 1: install snippet (token embedded) */}
              <InstallCard client={selected} token={minted.token} />

              {/* Step 2: connection status */}
              <div className={`p-4 rounded-2xl border flex items-center gap-3 ${connected ? 'bg-emerald-950/40 border-emerald-700/50' : 'bg-slate-900 border-slate-800'}`}>
                {connected ? (
                  <>
                    <Check className="w-5 h-5 text-emerald-400 shrink-0" />
                    <div className="text-sm">
                      <span className="font-bold text-emerald-300">{selected.label} is connected</span>
                      <span className="text-emerald-500/80 ml-2 text-xs">first seen {relTime(mintedRow!.last_used_at)}</span>
                    </div>
                  </>
                ) : (
                  <>
                    <Loader2 className="w-5 h-5 text-slate-500 animate-spin shrink-0" />
                    <div className="text-sm text-slate-400">
                      Waiting for {selected.label} to connect… run the snippet above, then send any message.
                    </div>
                  </>
                )}
              </div>

              {/* Bootstrap fallback (collapsible) */}
              <div className="bg-slate-900/60 border border-slate-800 rounded-2xl">
                <button
                  onClick={() => setShowBootstrap(v => !v)}
                  className="w-full flex items-center gap-2 p-4 text-left text-xs font-bold text-slate-400 hover:text-slate-200"
                >
                  {showBootstrap ? <ChevronDown className="w-4 h-4" /> : <ChevronRight className="w-4 h-4" />}
                  Agent doesn't see your data? Paste this into the chat (optional)
                </button>
                {showBootstrap && (
                  <div className="px-4 pb-4 space-y-2">
                    <p className="text-[11px] text-slate-500">
                      Most clients pick up WealthAgent automatically. If yours doesn't, paste this once —
                      it contains no token.
                    </p>
                    <div className="relative">
                      <pre className="bg-slate-950 border border-slate-800 rounded-lg p-3 text-[11px] text-slate-300 whitespace-pre-wrap">{BOOTSTRAP_PROMPT}</pre>
                      <div className="absolute top-2 right-2">
                        <CopyButton text={BOOTSTRAP_PROMPT} className="bg-slate-800 hover:bg-slate-700 text-slate-200" />
                      </div>
                    </div>
                  </div>
                )}
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
};

const InstallCard: React.FC<{ client: ClientDef; token: string }> = ({ client, token }) => {
  const snippet = client.snippet ? client.snippet(token) : '';
  const [showAlt, setShowAlt] = useState(false);
  return (
    <div className="bg-slate-900 border border-slate-800 rounded-2xl overflow-hidden">
      <div className="px-4 py-3 border-b border-slate-800/60 flex items-center justify-between">
        <span className="text-xs font-black text-slate-200 uppercase tracking-wider">
          {client.installKind === 'terminal' ? 'Run this in your terminal'
            : client.installKind === 'json' ? 'Add this to your config'
            : 'Add the connector'}
        </span>
        <span className="flex items-center gap-1 text-[10px] text-amber-500 font-bold">
          <AlertTriangle className="w-3 h-3" /> Contains your token — don't paste into a chat
        </span>
      </div>

      <div className="p-4 space-y-3">
        {client.installKind === 'json' && client.configPath && (
          <p className="text-[11px] text-slate-500">File: <code className="text-slate-300">{client.configPath}</code></p>
        )}

        {client.installKind === 'clickpath' ? (
          <>
            <ol className="text-xs text-slate-300 space-y-1.5 list-decimal list-inside">
              {client.steps!.map((s, i) => <li key={i}>{s}</li>)}
            </ol>
            <div className="space-y-2 pt-1">
              <FieldRow label="Server URL" value={MCP_URL} />
              <FieldRow label="Authorization header value" value={snippet} mono />
            </div>
          </>
        ) : (
          <div className="relative">
            <pre className="bg-slate-950 border border-slate-800 rounded-lg p-3 pr-12 text-[11px] text-emerald-200 font-mono overflow-x-auto whitespace-pre-wrap break-all">{snippet}</pre>
            <div className="absolute top-2 right-2">
              <CopyButton text={snippet} />
            </div>
          </div>
        )}

        {client.alt && (
          <div className="pt-1">
            <button
              onClick={() => setShowAlt(v => !v)}
              className="flex items-center gap-1.5 text-[11px] font-bold text-slate-500 hover:text-slate-300"
            >
              {showAlt ? <ChevronDown className="w-3.5 h-3.5" /> : <ChevronRight className="w-3.5 h-3.5" />}
              Alternative setup
            </button>
            {showAlt && (
              <div className="mt-2 space-y-2">
                <p className="text-[11px] text-slate-500">{client.alt.label}</p>
                <p className="text-[11px] text-slate-500">File: <code className="text-slate-300">{client.alt.configPath}</code></p>
                <div className="relative">
                  <pre className="bg-slate-950 border border-slate-800 rounded-lg p-3 pr-12 text-[11px] text-emerald-200 font-mono overflow-x-auto whitespace-pre-wrap break-all">{client.alt.text}</pre>
                  <div className="absolute top-2 right-2">
                    <CopyButton text={client.alt.text} />
                  </div>
                </div>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
};

/** OAuth connector clients (Claude Desktop, ChatGPT): no token is minted — the
 *  user signs in through the connector. Shows the steps, the server URL, and a
 *  collapsible manual (mcp-remote) fallback. */
const OAuthConnectCard: React.FC<{ client: ClientDef }> = ({ client }) => {
  const [showAlt, setShowAlt] = useState(false);
  return (
    <div className="bg-slate-900 border border-slate-800 rounded-2xl overflow-hidden">
      <div className="px-4 py-3 border-b border-slate-800/60 flex items-center justify-between">
        <span className="text-xs font-black text-slate-200 uppercase tracking-wider">Add the connector</span>
        <span className="flex items-center gap-1 text-[10px] text-emerald-400 font-bold">
          <ShieldCheck className="w-3 h-3" /> Secure sign-in — no token to copy
        </span>
      </div>
      <div className="p-4 space-y-3">
        <ol className="text-xs text-slate-300 space-y-1.5 list-decimal list-inside">
          {client.steps!.map((s, i) => <li key={i}>{s}</li>)}
        </ol>
        <FieldRow label="Server URL" value={MCP_URL} />
        <p className="text-[11px] text-slate-500">
          After you click Connect, a WealthAgent sign-in window opens. Approve access and the{' '}
          <code className="text-slate-300">wealth_*</code> tools appear in a new chat.
        </p>
        {client.alt && (
          <div className="pt-1">
            <button
              onClick={() => setShowAlt(v => !v)}
              className="flex items-center gap-1.5 text-[11px] font-bold text-slate-500 hover:text-slate-300"
            >
              {showAlt ? <ChevronDown className="w-3.5 h-3.5" /> : <ChevronRight className="w-3.5 h-3.5" />}
              Manual setup (older clients)
            </button>
            {showAlt && (
              <div className="mt-2 space-y-2">
                <p className="text-[11px] text-slate-500">{client.alt.label}</p>
                <p className="text-[11px] text-slate-500">File: <code className="text-slate-300">{client.alt.configPath}</code></p>
                <div className="relative">
                  <pre className="bg-slate-950 border border-slate-800 rounded-lg p-3 pr-12 text-[11px] text-emerald-200 font-mono overflow-x-auto whitespace-pre-wrap break-all">{client.alt.text}</pre>
                  <div className="absolute top-2 right-2">
                    <CopyButton text={client.alt.text} className="bg-slate-800 hover:bg-slate-700 text-slate-200" />
                  </div>
                </div>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
};

const FieldRow: React.FC<{ label: string; value: string; mono?: boolean }> = ({ label, value, mono }) => (
  <div>
    <div className="text-[10px] text-slate-500 font-bold uppercase tracking-wider mb-1">{label}</div>
    <div className="flex items-center gap-2">
      <code className={`flex-1 bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-[11px] text-slate-200 truncate select-all ${mono ? 'font-mono' : ''}`}>{value}</code>
      <CopyButton text={value} className="bg-slate-800 hover:bg-slate-700 text-slate-200" />
    </div>
  </div>
);

const ManagePanel: React.FC<{
  tokens: ApiToken[];
  grants: OAuthGrant[];
  onRevoke: (id: string, name: string) => void;
  onRevokeGrant: (clientId: string, name: string) => void;
  onRefresh: () => void;
  onCreate: (name: string) => Promise<CreateTokenResponse>;
}> = ({ tokens, grants, onRevoke, onRevokeGrant, onRefresh, onCreate }) => (
  <div className="bg-slate-900 border border-slate-800 rounded-2xl overflow-hidden">
    <div className="px-4 py-3 border-b border-slate-800/60 flex items-center justify-between">
      <span className="text-xs font-black text-slate-200 uppercase tracking-wider flex items-center gap-2">
        <ShieldCheck className="w-4 h-4 text-slate-400" /> Connected agents
      </span>
      <button onClick={onRefresh} className="text-slate-500 hover:text-slate-300" title="Refresh">
        <RefreshCw className="w-3.5 h-3.5" />
      </button>
    </div>
    {grants.length === 0 && tokens.length === 0 ? (
      <p className="p-4 text-sm text-slate-500">No agents connected yet.</p>
    ) : (
      <div className="divide-y divide-slate-800/60">
        {/* OAuth connections (signed in) */}
        {grants.map(g => (
          <div key={g.client_id} className="flex items-center gap-3 px-4 py-3">
            <div className="min-w-0 flex-1">
              <div className="text-sm font-bold text-slate-200 truncate flex items-center gap-2">
                {g.client_name}
                <span className="text-[9px] font-bold uppercase tracking-wider text-emerald-400 bg-emerald-500/10 border border-emerald-500/20 rounded px-1.5 py-0.5">
                  signed in
                </span>
              </div>
              <div className="text-[11px] text-slate-500">
                <span className="text-slate-400">{g.scopes}</span> · added {fmtDate(g.created_at)} · last active {relTime(g.last_used_at)}
              </div>
            </div>
            <button
              onClick={() => onRevokeGrant(g.client_id, g.client_name)}
              className="text-slate-500 hover:text-red-400 transition-colors shrink-0"
              title="Disconnect"
            >
              <Trash2 className="w-4 h-4" />
            </button>
          </div>
        ))}
        {/* Personal access tokens (manual / CLI) */}
        {tokens.map(t => (
          <div key={t.id} className="flex items-center gap-3 px-4 py-3">
            <div className="min-w-0 flex-1">
              <div className="text-sm font-bold text-slate-200 truncate flex items-center gap-2">
                {t.name}
                <span className="text-[9px] font-bold uppercase tracking-wider text-slate-400 bg-slate-500/10 border border-slate-500/20 rounded px-1.5 py-0.5">
                  token
                </span>
              </div>
              <div className="text-[11px] text-slate-500">
                <span className="text-slate-400">{t.scopes}</span> · added {fmtDate(t.created_at)} · last active {relTime(t.last_used_at)}
              </div>
            </div>
            <button
              onClick={() => onRevoke(t.id, t.name)}
              className="text-slate-500 hover:text-red-400 transition-colors shrink-0"
              title="Disconnect"
            >
              <Trash2 className="w-4 h-4" />
            </button>
          </div>
        ))}
      </div>
    )}
    <AdvancedCreate onCreate={onCreate} />
  </div>
);

/** Collapsible manual-token creation, for scripts / CI / clients not in the picker. */
const AdvancedCreate: React.FC<{ onCreate: (name: string) => Promise<CreateTokenResponse> }> = ({ onCreate }) => {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState('');
  const [creating, setCreating] = useState(false);
  const [minted, setMinted] = useState<CreateTokenResponse | null>(null);
  const [err, setErr] = useState<string | null>(null);

  const create = async () => {
    const n = name.trim();
    if (!n) return;
    setCreating(true);
    setErr(null);
    try {
      const res = await onCreate(n);
      setMinted(res);
      setName('');
    } catch (e) {
      setErr(e instanceof Error ? e.message : 'Failed to create token');
    } finally {
      setCreating(false);
    }
  };

  return (
    <div className="border-t border-slate-800/60">
      <button
        onClick={() => setOpen(v => !v)}
        className="w-full flex items-center gap-2 px-4 py-3 text-left text-[11px] font-bold text-slate-500 hover:text-slate-300"
      >
        {open ? <ChevronDown className="w-3.5 h-3.5" /> : <ChevronRight className="w-3.5 h-3.5" />}
        Advanced — create a custom token
      </button>
      {open && (
        <div className="px-4 pb-4 space-y-3">
          <p className="text-[11px] text-slate-500">
            For a script, CI job, or a client not listed above. Use it directly as a{' '}
            <code className="text-slate-300">Bearer</code> credential against{' '}
            <code className="text-slate-300">{MCP_URL}</code>.
          </p>
          {minted ? (
            <div className="p-3 bg-emerald-950 border border-emerald-700 rounded-lg space-y-2">
              <p className="text-xs font-bold text-emerald-300">Token created — copy it now</p>
              <p className="text-[11px] text-emerald-400/80">Shown once and cannot be retrieved again.</p>
              <div className="flex items-center gap-2">
                <code className="flex-1 text-[11px] bg-slate-950 px-3 py-2 rounded border border-slate-800 text-emerald-200 truncate select-all">{minted.token}</code>
                <CopyButton text={minted.token} className="bg-emerald-700 hover:bg-emerald-600 text-white" />
              </div>
              <button onClick={() => setMinted(null)} className="text-[11px] text-emerald-500 hover:text-emerald-300">Dismiss</button>
            </div>
          ) : (
            <>
              {err && <div className="p-2 bg-red-950 border border-red-800 rounded text-[11px] text-red-300">{err}</div>}
              <div className="flex gap-2">
                <input
                  type="text"
                  value={name}
                  onChange={e => setName(e.target.value)}
                  onKeyDown={e => e.key === 'Enter' && create()}
                  placeholder="Token name (e.g. ci-pipeline)"
                  className="flex-1 bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-xs text-slate-100 placeholder-slate-600 focus:outline-none focus:ring-1 focus:ring-blue-500"
                />
                <button
                  onClick={create}
                  disabled={creating || !name.trim()}
                  className="flex items-center gap-1.5 px-3 py-2 text-xs font-bold rounded-lg bg-blue-600 hover:bg-blue-500 disabled:opacity-50 disabled:cursor-not-allowed text-white transition-colors shrink-0"
                >
                  <Plus className="w-3.5 h-3.5" />
                  {creating ? 'Creating…' : 'Create'}
                </button>
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
};

export default ConnectView;
