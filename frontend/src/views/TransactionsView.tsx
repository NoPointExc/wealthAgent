import React, { useEffect, useState, useMemo, useRef, useCallback } from 'react';
import { createPortal } from 'react-dom';
import type { Transaction, SavedSearch, InvestmentTransaction, CapitalGainsReport } from '../types/models';
import { apiClient } from '../api/client';
import { usePrivacy, MONEY_MASK } from '../context/PrivacyContext';
import LockedValue, { isLocked } from '../components/LockedValue';
import { UnlockDialog } from '../components/PrivacyLockButton';
import {
  Search, CreditCard, Filter, RefreshCw, Smartphone, Globe,
  ShoppingBag, Landmark, Download, Calendar, X, CheckSquare, Square,
  DollarSign, Bookmark, BookmarkPlus, Trash2, Tag, NotebookPen, Pencil,
  ChevronUp, ChevronDown, ChevronsUpDown, TrendingUp,
  TrendingDown, BarChart3, AlertCircle, Leaf, Lock
} from 'lucide-react';

// ── Sortable column header ─────────────────────────────────────────────────
type SortDir = 'asc' | 'desc';

interface SortableThProps {
  col: string;
  label: string;
  activeCol: string | null;
  dir: SortDir;
  onSort: (col: string) => void;
  className?: string;
  align?: 'left' | 'right' | 'center';
}

const SortableTh: React.FC<SortableThProps> = ({ col, label, activeCol, dir, onSort, className = '', align = 'left' }) => {
  const active = activeCol === col;
  return (
    <th
      className={`p-5 cursor-pointer select-none group hover:text-slate-200 transition-colors ${className}`}
      onClick={() => onSort(col)}
    >
      <span className={`flex items-center gap-1 ${align === 'right' ? 'justify-end' : align === 'center' ? 'justify-center' : ''}`}>
        {label}
        {active
          ? dir === 'asc'
            ? <ChevronUp className="w-3 h-3 text-blue-400 shrink-0" />
            : <ChevronDown className="w-3 h-3 text-blue-400 shrink-0" />
          : <ChevronsUpDown className="w-3 h-3 text-slate-600 opacity-0 group-hover:opacity-100 transition-opacity shrink-0" />
        }
      </span>
    </th>
  );
};

// ── Bulk action tray — owns its own input state so typing doesn't re-render the table ──
interface BulkActionTrayProps {
  selectedCount: number;
  onTagAction: (action: 'add_tag' | 'remove_tag', value: string) => Promise<void>;
  onNoteAction: (action: 'set_note' | 'clear_note', value?: string) => Promise<void>;
  onClearSelection: () => void;
}

const BulkActionTray: React.FC<BulkActionTrayProps> = ({ selectedCount, onTagAction, onNoteAction, onClearSelection }) => {
  const [tagInput, setTagInput] = useState('');
  const [noteInput, setNoteInput] = useState('');

  const commitTag = async (action: 'add_tag' | 'remove_tag') => {
    const v = tagInput.trim();
    if (!v) return;
    await onTagAction(action, v);
    setTagInput('');
  };

  const commitNote = async (action: 'set_note' | 'clear_note') => {
    if (action === 'set_note' && !noteInput.trim()) return;
    await onNoteAction(action, action === 'set_note' ? noteInput.trim() : undefined);
    setNoteInput('');
  };

  return (
    <div className="fixed bottom-6 left-1/2 -translate-x-1/2 bg-slate-950 border border-slate-800 border-b-2 border-b-blue-500 px-5 py-3 rounded-3xl shadow-2xl z-50 flex items-center gap-4 transition-all duration-300">
      <span className="text-xs font-bold text-slate-300 pr-4 border-r border-slate-800 shrink-0">
        {selectedCount} selected
      </span>

      {/* Tags */}
      <div className="flex items-center gap-2">
        <Tag className="w-3.5 h-3.5 text-purple-400 shrink-0" />
        <input
          type="text"
          placeholder="Tag name..."
          value={tagInput}
          onChange={e => setTagInput(e.target.value)}
          onKeyDown={e => { if (e.key === 'Enter' && tagInput.trim()) commitTag('add_tag'); }}
          className="bg-slate-900 border border-slate-800 rounded-xl px-3 py-1.5 text-xs text-white outline-none focus:border-purple-500/40 w-36 placeholder-slate-600"
        />
        <button onClick={() => commitTag('add_tag')} disabled={!tagInput.trim()}
          className="px-3 py-1.5 bg-purple-600 hover:bg-purple-500 disabled:opacity-40 text-xs font-bold rounded-xl text-white transition-all">
          + Add
        </button>
        <button onClick={() => commitTag('remove_tag')} disabled={!tagInput.trim()}
          className="px-3 py-1.5 bg-slate-800 hover:bg-slate-700 border border-slate-700 disabled:opacity-40 text-xs font-bold rounded-xl text-rose-400 transition-all">
          − Remove
        </button>
      </div>

      <div className="h-6 w-px bg-slate-800 shrink-0" />

      {/* Notes */}
      <div className="flex items-center gap-2">
        <NotebookPen className="w-3.5 h-3.5 text-blue-400 shrink-0" />
        <input
          type="text"
          placeholder="Note text..."
          value={noteInput}
          onChange={e => setNoteInput(e.target.value)}
          onKeyDown={e => { if (e.key === 'Enter' && noteInput.trim()) commitNote('set_note'); }}
          className="bg-slate-900 border border-slate-800 rounded-xl px-3 py-1.5 text-xs text-white outline-none focus:border-blue-500/40 w-36 placeholder-slate-600"
        />
        <button onClick={() => commitNote('set_note')} disabled={!noteInput.trim()}
          className="px-3 py-1.5 bg-blue-600 hover:bg-blue-500 disabled:opacity-40 text-xs font-bold rounded-xl text-white transition-all">
          Set
        </button>
        <button onClick={() => commitNote('clear_note')}
          className="px-3 py-1.5 bg-slate-800 hover:bg-slate-700 border border-slate-700 text-xs font-bold rounded-xl text-slate-400 transition-all">
          Clear
        </button>
      </div>

      <button onClick={onClearSelection}
        className="p-1.5 hover:bg-slate-800 rounded-xl text-slate-500 hover:text-slate-300 transition-all ml-1 shrink-0">
        <X className="w-4 h-4" />
      </button>
    </div>
  );
};

// ── Shared inline-edit cell for Note ──────────────────────────────────────
interface NoteCellProps {
  txnId: string;
  note?: string;
  onSave: (id: string, note: string | null) => Promise<void>;
}

const NoteCell: React.FC<NoteCellProps> = ({ txnId, note, onSave }) => {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(note ?? '');
  const inputRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => { if (editing) inputRef.current?.focus(); }, [editing]);

  // A locked note must not open the editor: saving would overwrite the real
  // (encrypted) note with the literal sentinel text.
  if (isLocked(note)) return <LockedValue value={note} />;

  const commit = async () => {
    setEditing(false);
    const trimmed = draft.trim();
    const newNote = trimmed === '' ? null : trimmed;
    if (newNote !== (note ?? null)) await onSave(txnId, newNote);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); commit(); }
    if (e.key === 'Escape') { setDraft(note ?? ''); setEditing(false); }
  };

  if (editing) {
    return (
      <textarea
        ref={inputRef}
        value={draft}
        onChange={e => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={handleKeyDown}
        rows={2}
        className="w-full bg-slate-800 border border-blue-500/40 rounded-lg px-2 py-1.5 text-xs text-slate-200 outline-none resize-none placeholder-slate-600 font-sans"
        placeholder="Add a note..."
      />
    );
  }

  return (
    <button
      onClick={() => { setDraft(note ?? ''); setEditing(true); }}
      className="group flex items-start gap-1.5 w-full text-left min-h-[28px]"
    >
      {note ? (
        <>
          <NotebookPen className="w-3 h-3 text-slate-500 group-hover:text-blue-400 shrink-0 mt-0.5 transition-colors" />
          <span className="text-[11px] text-slate-400 group-hover:text-slate-200 leading-snug line-clamp-2 transition-colors">{note}</span>
        </>
      ) : (
        <span className="text-[11px] text-slate-700 group-hover:text-slate-500 italic transition-colors flex items-center gap-1">
          <NotebookPen className="w-3 h-3 opacity-0 group-hover:opacity-100 transition-opacity" /> Add note...
        </span>
      )}
    </button>
  );
};

// ── Shared popover-edit cell for Tags ─────────────────────────────────────
interface TagCellProps {
  txnId: string;
  tags?: string;
  onSave: (id: string, tags: string[]) => Promise<void>;
}

const TagCell: React.FC<TagCellProps> = ({ txnId, tags, onSave }) => {
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState<string[]>([]);
  const [input, setInput] = useState('');
  const [pos, setPos] = useState({ top: 0, left: 0 });
  const triggerRef = useRef<HTMLButtonElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);

  const existingTags = tags ? tags.split(',').map(t => t.trim()).filter(Boolean) : [];

  const openPopover = () => {
    const rect = triggerRef.current?.getBoundingClientRect();
    if (rect) setPos({ top: rect.bottom + 4, left: rect.left });
    setDraft([...existingTags]);
    setOpen(true);
  };

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (!popoverRef.current?.contains(e.target as Node) && !triggerRef.current?.contains(e.target as Node)) {
        save();
      }
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [open, draft]);

  const save = async () => {
    setOpen(false);
    setInput('');
    await onSave(txnId, draft);
  };

  const addTag = () => {
    const t = input.trim();
    if (t && !draft.includes(t)) setDraft(prev => [...prev, t]);
    setInput('');
  };

  return (
    <>
      <button
        ref={triggerRef}
        onClick={openPopover}
        className="group flex flex-wrap gap-1 items-center min-h-[28px] max-w-[150px] text-left"
      >
        {existingTags.length > 0 ? (
          <>
            {existingTags.map(tag => (
              <span key={tag} className="px-2 py-0.5 bg-purple-500/10 text-purple-400 border border-purple-500/20 rounded-md text-[9px] font-bold inline-flex items-center gap-1">
                <Tag className="w-2 h-2 shrink-0" /> {tag}
              </span>
            ))}
            <Pencil className="w-3 h-3 text-slate-600 opacity-0 group-hover:opacity-100 transition-opacity ml-0.5 shrink-0" />
          </>
        ) : (
          <span className="text-[11px] text-slate-700 group-hover:text-slate-500 italic flex items-center gap-1 transition-colors">
            <Tag className="w-3 h-3 opacity-0 group-hover:opacity-100 transition-opacity" /> Add tags...
          </span>
        )}
      </button>

      {open && createPortal(
        <div
          ref={popoverRef}
          style={{ position: 'fixed', top: pos.top, left: pos.left, zIndex: 9999 }}
          className="w-56 bg-slate-950 border border-slate-700 rounded-xl shadow-2xl p-3 space-y-2.5"
        >
          <span className="text-[10px] font-black uppercase tracking-widest text-slate-500 block">Edit Tags</span>
          <div className="flex flex-wrap gap-1 min-h-[20px]">
            {draft.length === 0 && <span className="text-[10px] text-slate-600 italic">No tags yet</span>}
            {draft.map(tag => (
              <button
                key={tag}
                onClick={() => setDraft(prev => prev.filter(t => t !== tag))}
                className="px-2 py-0.5 bg-purple-500/10 text-purple-400 border border-purple-500/20 rounded-md text-[9px] font-bold inline-flex items-center gap-1 hover:bg-rose-500/10 hover:text-rose-400 hover:border-rose-500/20 transition-colors"
              >
                <Tag className="w-2 h-2" /> {tag} <X className="w-2.5 h-2.5" />
              </button>
            ))}
          </div>
          <div className="flex gap-1.5">
            <input
              autoFocus
              type="text"
              value={input}
              onChange={e => setInput(e.target.value)}
              onKeyDown={e => {
                if (e.key === 'Enter') { e.preventDefault(); addTag(); }
                if (e.key === 'Escape') { setOpen(false); setInput(''); }
              }}
              placeholder="New tag, then Enter"
              className="flex-1 bg-slate-900 border border-slate-700 rounded-lg px-2 py-1 text-xs text-white outline-none focus:border-blue-500/40 placeholder-slate-600"
            />
            <button onClick={addTag} className="px-2.5 py-1 bg-blue-600 hover:bg-blue-500 rounded-lg text-xs font-bold text-white transition-colors">+</button>
          </div>
          <button onClick={save} className="w-full py-1.5 bg-slate-800 hover:bg-slate-700 rounded-lg text-xs font-bold text-slate-300 transition-colors">
            Save
          </button>
        </div>,
        document.body
      )}
    </>
  );
};

// ── Investment Transactions Panel ──────────────────────────────────────────
const InvestmentTransactionsPanel: React.FC = () => {
  const { hidden } = usePrivacy();
  const [items, setItems] = useState<InvestmentTransaction[]>([]);
  const [loading, setLoading] = useState(true);
  const [totalCount, setTotalCount] = useState(0);
  const [syncing, setSyncing] = useState(false);
  const loadGenRef = useRef(0);

  const [searchQuery, setSearchQuery] = useState('');
  const [selectedAccount, setSelectedAccount] = useState('all');
  const [selectedType, setSelectedType] = useState('all');
  const [startDate, setStartDate] = useState('');
  const [endDate, setEndDate] = useState('');
  const [sortCol, setSortCol] = useState<string | null>(null);
  const [sortDir, setSortDir] = useState<SortDir>('asc');

  const INITIAL_LIMIT = 500;
  const PAGE_SIZE = 500;

  useEffect(() => { load(); }, []);

  const load = async () => {
    const gen = ++loadGenRef.current;
    setLoading(true);
    setItems([]);
    try {
      const first = await apiClient.getInvestmentTransactions(0, INITIAL_LIMIT);
      if (gen !== loadGenRef.current) return;
      setItems(first.items);
      setTotalCount(first.total);
      setLoading(false);
      if (first.total > INITIAL_LIMIT) {
        let offset = INITIAL_LIMIT;
        while (offset < first.total) {
          const page = await apiClient.getInvestmentTransactions(offset, PAGE_SIZE);
          if (gen !== loadGenRef.current) return;
          setItems(prev => [...prev, ...page.items]);
          offset += PAGE_SIZE;
        }
      }
    } catch (e) {
      if (gen !== loadGenRef.current) return;
      console.error('Failed to load investment transactions', e);
      setLoading(false);
    }
  };

  const handleSync = async () => {
    setSyncing(true);
    try {
      await apiClient.syncPlaidData();
      await load();
    } catch (e) {
      console.error('Investment sync failed', e);
    } finally {
      setSyncing(false);
    }
  };

  const handleSort = (col: string) => {
    if (sortCol !== col) { setSortCol(col); setSortDir('asc'); }
    else if (sortDir === 'asc') setSortDir('desc');
    else setSortCol(null);
  };

  const formatCurrency = (cents: number) =>
    hidden ? MONEY_MASK : (Math.abs(cents) / 100).toLocaleString('en-US', { style: 'currency', currency: 'USD' });

  const uniqueAccounts = useMemo(() => {
    const s = new Set<string>();
    items.forEach(t => { if (t.account_name) s.add(t.account_name); });
    return Array.from(s).sort();
  }, [items]);

  const uniqueTypes = useMemo(() => {
    const s = new Set<string>();
    items.forEach(t => { if (t.txn_type) s.add(t.txn_type); });
    return Array.from(s).sort();
  }, [items]);

  const filtered = useMemo(() => {
    return items.filter(t => {
      if (searchQuery.trim()) {
        const q = searchQuery.toLowerCase();
        const match = t.name.toLowerCase().includes(q)
          || (t.symbol?.toLowerCase().includes(q) ?? false)
          || (t.security_name?.toLowerCase().includes(q) ?? false)
          || t.account_name.toLowerCase().includes(q);
        if (!match) return false;
      }
      if (selectedAccount !== 'all' && t.account_name !== selectedAccount) return false;
      if (selectedType !== 'all' && t.txn_type !== selectedType) return false;
      if (startDate && t.date < startDate) return false;
      if (endDate && t.date > endDate) return false;
      return true;
    });
  }, [items, searchQuery, selectedAccount, selectedType, startDate, endDate]);

  const sorted = useMemo(() => {
    if (!sortCol) return filtered;
    return [...filtered].sort((a, b) => {
      let av: string | number = '';
      let bv: string | number = '';
      switch (sortCol) {
        case 'date':    av = a.date;    bv = b.date;    break;
        case 'amount':  av = a.amount;  bv = b.amount;  break;
        case 'account': av = a.account_name.toLowerCase(); bv = b.account_name.toLowerCase(); break;
        case 'symbol':  av = (a.symbol || '').toLowerCase(); bv = (b.symbol || '').toLowerCase(); break;
        case 'type':    av = a.txn_type.toLowerCase(); bv = b.txn_type.toLowerCase(); break;
      }
      if (av < bv) return sortDir === 'asc' ? -1 : 1;
      if (av > bv) return sortDir === 'asc' ? 1 : -1;
      return 0;
    });
  }, [filtered, sortCol, sortDir]);

  const summaryMetrics = useMemo(() => {
    let buys = 0, sells = 0, dividends = 0;
    filtered.forEach(t => {
      const type = (t.subtype || t.txn_type).toLowerCase();
      if (type === 'buy') buys += t.amount;
      else if (type === 'sell') sells += Math.abs(t.amount);
      else if (type === 'dividend') dividends += Math.abs(t.amount);
    });
    return { count: filtered.length, buys, sells, dividends };
  }, [filtered]);

  const handleExportCSV = () => {
    if (filtered.length === 0) return;
    const headers = ['Date', 'Account', 'Description', 'Symbol', 'Security Name', 'Type', 'Subtype', 'Quantity', 'Price', 'Fees', 'Amount'];
    const escape = (v?: string | null) => `"${(v ?? '').replace(/"/g, '""')}"`;
    const rows = [headers.join(','), ...sorted.map(t => [
      t.date,
      escape(t.account_name),
      escape(t.name),
      escape(t.symbol),
      escape(t.security_name),
      t.txn_type,
      escape(t.subtype),
      t.quantity ?? '',
      t.price != null ? (t.price / 100).toFixed(2) : '',
      t.fees != null ? (t.fees / 100).toFixed(2) : '',
      (t.amount / 100).toFixed(2),
    ].join(','))];
    const blob = new Blob([rows.join('\n')], { type: 'text/csv;charset=utf-8;' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.setAttribute('download', `investment_transactions_${new Date().toISOString().split('T')[0]}.csv`);
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
  };

  const typeColor = (type: string) => {
    switch ((type).toLowerCase()) {
      case 'buy':      return 'text-blue-400 bg-blue-500/10 border-blue-500/20';
      case 'sell':     return 'text-emerald-400 bg-emerald-500/10 border-emerald-500/20';
      case 'dividend': return 'text-purple-400 bg-purple-500/10 border-purple-500/20';
      case 'cash':     return 'text-slate-400 bg-slate-700/30 border-slate-600/20';
      default:         return 'text-slate-400 bg-slate-700/30 border-slate-600/20';
    }
  };

  const hasFilters = searchQuery !== '' || selectedAccount !== 'all' || selectedType !== 'all' || startDate !== '' || endDate !== '';

  return (
    <div className="space-y-6 pb-24">
      {/* Filter bar */}
      <div className="bg-slate-900 border border-slate-800 p-6 rounded-3xl shadow-2xl flex flex-col gap-4">
        <div className="flex flex-col md:flex-row justify-between items-center gap-4">
          <div className="flex-1 w-full relative">
            <Search className="w-4 h-4 text-slate-500 absolute left-4 top-3.5" />
            <input
              type="text"
              placeholder="Search by name, symbol, or account..."
              value={searchQuery}
              onChange={e => setSearchQuery(e.target.value)}
              className="w-full bg-slate-950/80 border border-slate-800 rounded-2xl py-3 pl-11 pr-4 text-xs text-white placeholder-slate-500 outline-none focus:border-blue-500/50 transition-all font-medium"
            />
          </div>
          <div className="flex items-center gap-3 shrink-0">
            {hasFilters && (
              <button onClick={() => { setSearchQuery(''); setSelectedAccount('all'); setSelectedType('all'); setStartDate(''); setEndDate(''); }}
                className="flex items-center gap-1.5 px-4 py-2.5 bg-slate-800 hover:bg-slate-700 text-xs font-bold rounded-2xl text-slate-300 border border-slate-700">
                <X className="w-3.5 h-3.5" /> Clear
              </button>
            )}
            <button onClick={handleExportCSV} disabled={filtered.length === 0}
              className="flex items-center gap-2 px-5 py-2.5 bg-emerald-600/10 hover:bg-emerald-600/20 disabled:opacity-40 text-xs font-bold rounded-2xl border border-emerald-500/20 text-emerald-400">
              <Download className="w-3.5 h-3.5" /> Export CSV
            </button>
            <button onClick={handleSync} disabled={syncing}
              className="flex items-center gap-2 px-5 py-2.5 bg-blue-600 hover:bg-blue-500 disabled:opacity-50 text-xs font-black rounded-2xl border border-blue-500/10 shadow-lg shadow-blue-900/20 text-white">
              <RefreshCw className={`w-3.5 h-3.5 ${syncing ? 'animate-spin' : ''}`} />
              {syncing ? 'Syncing...' : 'Sync Trades'}
            </button>
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-3 pt-2 border-t border-slate-800/40">
          <div className="flex items-center gap-2 bg-slate-950/40 px-3 py-1.5 rounded-2xl border border-slate-800/80">
            <CreditCard className="w-3.5 h-3.5 text-slate-500" />
            <select value={selectedAccount} onChange={e => setSelectedAccount(e.target.value)}
              className="bg-transparent text-xs text-slate-300 outline-none border-none font-semibold cursor-pointer max-w-[160px]">
              <option value="all" className="bg-slate-950">All Accounts</option>
              {uniqueAccounts.map(a => <option key={a} value={a} className="bg-slate-950">{a}</option>)}
            </select>
          </div>

          <div className="flex items-center gap-2 bg-slate-950/40 px-3 py-1.5 rounded-2xl border border-slate-800/80">
            <Filter className="w-3.5 h-3.5 text-slate-500" />
            <select value={selectedType} onChange={e => setSelectedType(e.target.value)}
              className="bg-transparent text-xs text-slate-300 outline-none border-none font-semibold cursor-pointer">
              <option value="all" className="bg-slate-950">All Types</option>
              {uniqueTypes.map(t => <option key={t} value={t} className="bg-slate-950 capitalize">{t}</option>)}
            </select>
          </div>

          <div className="flex items-center gap-2 bg-slate-950/40 px-3 py-1.5 rounded-2xl border border-slate-800/80">
            <Calendar className="w-3.5 h-3.5 text-slate-500" />
            <input type="date" value={startDate} onChange={e => setStartDate(e.target.value)}
              className="bg-transparent text-xs text-slate-300 outline-none border-none font-semibold cursor-pointer py-0.5" />
            <span className="text-slate-600 text-[10px] font-bold">to</span>
            <input type="date" value={endDate} onChange={e => setEndDate(e.target.value)}
              className="bg-transparent text-xs text-slate-300 outline-none border-none font-semibold cursor-pointer py-0.5" />
          </div>
        </div>
      </div>

      {/* Summary metrics */}
      {!loading && (
        <div className="grid grid-cols-2 lg:grid-cols-4 gap-4">
          <div className="bg-slate-900 border border-slate-800 p-4 rounded-2xl">
            <span className="text-[9px] font-black text-slate-500 uppercase tracking-widest block mb-1">Trades</span>
            <span className="text-xl font-bold text-slate-200">{summaryMetrics.count} <span className="text-xs text-slate-500 font-medium">Records</span></span>
          </div>
          <div className="bg-slate-900 border border-slate-800 p-4 rounded-2xl">
            <span className="text-[9px] font-black text-slate-500 uppercase tracking-widest block mb-1">Bought</span>
            <span className="text-xl font-bold text-blue-400">− {formatCurrency(summaryMetrics.buys)}</span>
          </div>
          <div className="bg-slate-900 border border-slate-800 p-4 rounded-2xl">
            <span className="text-[9px] font-black text-slate-500 uppercase tracking-widest block mb-1">Sold</span>
            <span className="text-xl font-bold text-emerald-400">+ {formatCurrency(summaryMetrics.sells)}</span>
          </div>
          <div className="bg-slate-900 border border-slate-800 p-4 rounded-2xl">
            <span className="text-[9px] font-black text-slate-500 uppercase tracking-widest block mb-1">Dividends</span>
            <span className="text-xl font-bold text-purple-400">+ {formatCurrency(summaryMetrics.dividends)}</span>
          </div>
        </div>
      )}

      {/* Table */}
      <div className="bg-slate-900 border border-slate-800 rounded-3xl overflow-hidden shadow-2xl">
        <div className="overflow-x-auto">
          <table className="w-full text-left border-collapse">
            <thead className="bg-slate-950/40 text-[10px] text-slate-500 uppercase tracking-widest font-black border-b border-slate-800/60">
              <tr>
                <SortableTh col="date"    label="Date"        activeCol={sortCol} dir={sortDir} onSort={handleSort} className="w-28" />
                <SortableTh col="account" label="Account"     activeCol={sortCol} dir={sortDir} onSort={handleSort} className="w-44" />
                <th className="p-5">Description</th>
                <SortableTh col="symbol"  label="Symbol"      activeCol={sortCol} dir={sortDir} onSort={handleSort} className="w-28" />
                <SortableTh col="type"    label="Type"        activeCol={sortCol} dir={sortDir} onSort={handleSort} className="w-32" />
                <th className="p-5 w-24 text-right">Qty</th>
                <th className="p-5 w-28 text-right">Price</th>
                <th className="p-5 w-24 text-right">Fees</th>
                <SortableTh col="amount" label="Amount" activeCol={sortCol} dir={sortDir} onSort={handleSort} className="w-36" align="right" />
              </tr>
            </thead>
            <tbody className="text-xs divide-y divide-slate-800/50 text-slate-300">
              {loading ? (
                <tr><td colSpan={9} className="p-16 text-center text-slate-500 italic animate-pulse">Loading investment transactions...</td></tr>
              ) : sorted.length === 0 ? (
                <tr><td colSpan={9} className="p-16 text-center text-slate-500 italic">
                  {totalCount === 0
                    ? 'No investment transactions found. Click Refresh in the top bar to sync.'
                    : 'No transactions match your filters.'}
                </td></tr>
              ) : sorted.map(t => {
                const isBuy = t.amount > 0;
                return (
                  <tr key={t.id} className="hover:bg-slate-800/20 transition-colors">
                    <td className="p-5 text-slate-500 font-bold tracking-wider font-mono text-[10px]">
                      {(() => { const [y, m, d] = t.date.split('-'); return `${m}/${d}/${y}`; })()}
                    </td>
                    <td className="p-5">
                      <span className="px-2.5 py-1 bg-slate-950 text-slate-400 font-semibold border border-slate-800/80 rounded-xl max-w-[160px] truncate block text-[10px]" title={isLocked(t.account_name) ? undefined : t.account_name}>
                        <LockedValue value={t.account_name} />
                      </span>
                    </td>
                    <td className="p-5 max-w-[240px]">
                      <div className="font-bold text-slate-100 text-sm truncate"><LockedValue value={t.name} /></div>
                      {t.security_name && t.security_name !== t.name && (
                        <div className="text-[10px] text-slate-500 truncate font-mono"><LockedValue value={t.security_name} /></div>
                      )}
                    </td>
                    <td className="p-5">
                      {isLocked(t.symbol) ? (
                        <LockedValue value={t.symbol} />
                      ) : t.symbol ? (
                        <span className="px-2 py-1 bg-blue-500/10 text-blue-300 border border-blue-500/20 rounded-lg text-[10px] font-black tracking-wider">
                          {t.symbol}
                        </span>
                      ) : <span className="text-slate-600 italic text-[10px]">—</span>}
                    </td>
                    <td className="p-5">
                      <span className={`px-2 py-0.5 rounded border text-[10px] font-black uppercase tracking-wider ${typeColor(t.subtype || t.txn_type)}`}>
                        {t.subtype || t.txn_type}
                      </span>
                    </td>
                    <td className="p-5 text-right font-mono text-[11px] text-slate-300">
                      {t.quantity != null ? t.quantity.toLocaleString('en-US', { maximumFractionDigits: 4 }) : '—'}
                    </td>
                    <td className="p-5 text-right font-mono text-[11px] text-slate-300">
                      {t.price != null ? `$${(t.price / 100).toFixed(2)}` : '—'}
                    </td>
                    <td className="p-5 text-right font-mono text-[11px] text-slate-500">
                      {t.fees != null && t.fees !== 0 ? `$${(t.fees / 100).toFixed(2)}` : '—'}
                    </td>
                    <td className="p-5 text-right">
                      <span className={`text-sm font-black font-mono tracking-tight ${isBuy ? 'text-slate-100' : 'text-emerald-400'}`}>
                        {isBuy ? '−' : '+'}{formatCurrency(t.amount)}
                      </span>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
};

// ── Bank Transactions View ─────────────────────────────────────────────────
const TransactionsView: React.FC = () => {
  const { hidden } = usePrivacy();
  const [transactions, setTransactions] = useState<Transaction[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [totalCount, setTotalCount] = useState(0);
  const [syncing, setSyncing] = useState(false);
  const loadGenRef = useRef(0);

  // Filters state
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedAccount, setSelectedAccount] = useState('all');
  const [selectedCategories, setSelectedCategories] = useState<string[]>([]); // Multi-select
  const [selectedTags, setSelectedTags] = useState<string[]>([]); // Multi-select
  const [startDate, setStartDate] = useState('');
  const [endDate, setEndDate] = useState('');
  const [showPendingOnly, setShowPendingOnly] = useState(false);

  // Amount Filter state
  const [amountMode, setAmountMode] = useState<'all' | 'less' | 'greater' | 'equal' | 'between'>('all');
  const [amountVal, setAmountVal] = useState('');
  const [amountMin, setAmountMin] = useState('');
  const [amountMax, setAmountMax] = useState('');

  // Sort state — null = default (server order: date desc)
  const [sortCol, setSortCol] = useState<string | null>(null);
  const [sortDir, setSortDir] = useState<SortDir>('asc');

  const handleSort = (col: string) => {
    if (sortCol !== col) { setSortCol(col); setSortDir('asc'); }
    else if (sortDir === 'asc') { setSortDir('desc'); }
    else { setSortCol(null); } // 3rd click: reset to default
  };

  // Bulk edit state
  const [selectedTxnIds, setSelectedTxnIds] = useState<Set<string>>(new Set());

  // Saved Searches state
  const [savedSearches, setSavedSearches] = useState<SavedSearch[]>([]);
  const [showSaveModal, setShowSaveModal] = useState(false);
  const [newSearchName, setNewSearchName] = useState('');

  // Dropdown UI states
  const [showCategoryDropdown, setShowCategoryDropdown] = useState(false);
  const [showAmountDropdown, setShowAmountDropdown] = useState(false);
  const [showSavedDropdown, setShowSavedDropdown] = useState(false);
  const [showTagDropdown, setShowTagDropdown] = useState(false);
  
  const categoryDropdownRef = useRef<HTMLDivElement>(null);
  const amountDropdownRef = useRef<HTMLDivElement>(null);
  const savedDropdownRef = useRef<HTMLDivElement>(null);
  const tagDropdownRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    loadTransactions();
    loadSavedSearches();
  }, []);

  // Click outside to close dropdowns
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (categoryDropdownRef.current && !categoryDropdownRef.current.contains(event.target as Node)) {
        setShowCategoryDropdown(false);
      }
      if (amountDropdownRef.current && !amountDropdownRef.current.contains(event.target as Node)) {
        setShowAmountDropdown(false);
      }
      if (savedDropdownRef.current && !savedDropdownRef.current.contains(event.target as Node)) {
        setShowSavedDropdown(false);
      }
      if (tagDropdownRef.current && !tagDropdownRef.current.contains(event.target as Node)) {
        setShowTagDropdown(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  const INITIAL_LIMIT = 200;
  const PAGE_SIZE = 500;

  const loadTransactions = async () => {
    const gen = ++loadGenRef.current;
    setLoading(true);
    setTransactions([]);
    try {
      const first = await apiClient.getTransactions(0, INITIAL_LIMIT);
      if (gen !== loadGenRef.current) return;
      setTransactions(first.items);
      setTotalCount(first.total);
      setLoading(false);

      if (first.total > INITIAL_LIMIT) {
        setLoadingMore(true);
        let offset = INITIAL_LIMIT;
        while (offset < first.total) {
          const page = await apiClient.getTransactions(offset, PAGE_SIZE);
          if (gen !== loadGenRef.current) return;
          setTransactions(prev => [...prev, ...page.items]);
          offset += PAGE_SIZE;
        }
        setLoadingMore(false);
      }
    } catch (e) {
      if (gen !== loadGenRef.current) return;
      console.error("Failed to load transactions", e);
      setLoading(false);
      setLoadingMore(false);
    }
  };

  const loadSavedSearches = async () => {
    try {
      const data = await apiClient.getSavedSearches();
      setSavedSearches(data);
    } catch (e) {
      console.error("Failed to load saved searches", e);
    }
  };

  const handleSync = async () => {
    setSyncing(true);
    try {
      await apiClient.syncPlaidData();
      await loadTransactions();
    } catch (e) {
      console.error("Sync failed", e);
    } finally {
      setSyncing(false);
    }
  };

  const formatCurrency = (cents: number) => {
    if (hidden) return MONEY_MASK;
    const value = Math.abs(cents) / 100;
    return value.toLocaleString('en-US', { style: 'currency', currency: 'USD' });
  };

  // Extract unique accounts & categories for filter options
  const uniqueAccounts = useMemo(() => {
    const accounts = new Set<string>();
    transactions.forEach(t => {
      if (t.account_name) accounts.add(t.account_name);
    });
    return Array.from(accounts).sort();
  }, [transactions]);

  const uniqueCategories = useMemo(() => {
    const categories = new Set<string>();
    transactions.forEach(t => {
      if (t.plaid_primary_category) categories.add(t.plaid_primary_category);
    });
    return Array.from(categories).sort();
  }, [transactions]);

  const uniqueTags = useMemo(() => {
    const tagsSet = new Set<string>();
    transactions.forEach(t => {
      if (t.tags) {
        t.tags.split(',').forEach(tag => {
          const trimmed = tag.trim();
          if (trimmed) tagsSet.add(trimmed);
        });
      }
    });
    return Array.from(tagsSet).sort();
  }, [transactions]);

  // Apply filters
  const filteredTransactions = useMemo(() => {
    return transactions.filter(t => {
      // 1. PowerSearch Parser (AND word match, comma/OR logical OR split)
      if (searchQuery.trim()) {
        const queryText = searchQuery.trim().toLowerCase();
        
        // Split by case-insensitive ' OR ' or comma ',' for Logical OR matching
        const orTerms = queryText.split(/\s+or\s+|,/i).map(term => term.trim()).filter(Boolean);
        
        if (orTerms.length > 0) {
          const matchesAnyOrTerm = orTerms.some(term => {
            // Split terms by whitespace for multi-word Logical AND matching within an OR block.
            // Skip single-char tokens (e.g. "m") — too short to be meaningful and cause false positives.
            const andWords = term.split(/\s+/).filter(w => w.length >= 2);
            if (andWords.length === 0) return false;

            return andWords.every(word => {
              // Use word-boundary prefix match so "us" doesn't hit "business" / "purchase".
              const escaped = word.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
              const re = new RegExp(`\\b${escaped}`, 'i');
              return re.test(t.raw_string)
                || (t.merchant_name ? re.test(t.merchant_name) : false)
                || (t.plaid_category ? re.test(t.plaid_category) : false)
                || (t.tags ? re.test(t.tags) : false);
            });
          });
          
          if (!matchesAnyOrTerm) return false;
        }
      }

      // 2. Account Filter
      if (selectedAccount !== 'all' && t.account_name !== selectedAccount) return false;

      // 3. Multi-Select Category Filter
      if (selectedCategories.length > 0 && (!t.plaid_primary_category || !selectedCategories.includes(t.plaid_primary_category))) return false;

      // 3.5. Multi-Select Tags Filter
      if (selectedTags.length > 0) {
        if (!t.tags) return false;
        const rowTags = t.tags.split(',').map(s => s.trim().toLowerCase());
        const hasAllSelected = selectedTags.every(tag => rowTags.includes(tag.toLowerCase()));
        if (!hasAllSelected) return false;
      }

      // 4. Date Range Filter
      if (startDate && t.txn_date < startDate) return false;
      if (endDate && t.txn_date > endDate) return false;

      // 5. Amount Filter (Compare against displayed signs: positive = income, negative = expense)
      const amtCents = -t.amount;
      if (amountMode !== 'all') {
        if (amountMode === 'less' && amountVal) {
          const centsVal = Math.round(parseFloat(amountVal) * 100);
          if (amtCents >= centsVal) return false;
        } else if (amountMode === 'greater' && amountVal) {
          const centsVal = Math.round(parseFloat(amountVal) * 100);
          if (amtCents <= centsVal) return false;
        } else if (amountMode === 'equal' && amountVal) {
          const centsVal = Math.round(parseFloat(amountVal) * 100);
          if (amtCents !== centsVal) return false;
        } else if (amountMode === 'between') {
          if (amountMin) {
            const minCents = Math.round(parseFloat(amountMin) * 100);
            if (amtCents < minCents) return false;
          }
          if (amountMax) {
            const maxCents = Math.round(parseFloat(amountMax) * 100);
            if (amtCents > maxCents) return false;
          }
        }
      }

      // 6. Pending Filter
      if (showPendingOnly && !t.pending) return false;

      return true;
    });
  }, [transactions, searchQuery, selectedAccount, selectedCategories, selectedTags, startDate, endDate, amountMode, amountVal, amountMin, amountMax, showPendingOnly]);

  const sortedTransactions = useMemo(() => {
    if (!sortCol) return filteredTransactions;
    return [...filteredTransactions].sort((a, b) => {
      let av: string | number = '';
      let bv: string | number = '';
      switch (sortCol) {
        case 'date':     av = a.txn_date; bv = b.txn_date; break;
        case 'amount':   av = a.amount;   bv = b.amount;   break;
        case 'account':  av = (a.account_name || '').toLowerCase(); bv = (b.account_name || '').toLowerCase(); break;
        case 'merchant': av = (a.merchant_name || a.raw_string).toLowerCase(); bv = (b.merchant_name || b.raw_string).toLowerCase(); break;
        case 'category': av = (a.plaid_primary_category || '').toLowerCase(); bv = (b.plaid_primary_category || '').toLowerCase(); break;
      }
      if (av < bv) return sortDir === 'asc' ? -1 : 1;
      if (av > bv) return sortDir === 'asc' ? 1 : -1;
      return 0;
    });
  }, [filteredTransactions, sortCol, sortDir]);

  // Real-time summary metrics calculated on displayed signs (+ for income, - for expense)
  const summaryMetrics = useMemo(() => {
    let inflow = 0;
    let outflow = 0;
    
    filteredTransactions.forEach(t => {
      const displayAmount = -t.amount; // flipped sign to match UI
      if (displayAmount > 0) {
        inflow += displayAmount;
      } else {
        outflow += displayAmount; // already negative
      }
    });

    const net = inflow + outflow;

    return {
      count: filteredTransactions.length,
      inflow,
      outflow: Math.abs(outflow), // keep absolute for formatted display
      net
    };
  }, [filteredTransactions]);

  // Selection Logic for Checkboxes
  const isAllSelected = useMemo(() => {
    if (filteredTransactions.length === 0) return false;
    return filteredTransactions.every(t => selectedTxnIds.has(t.id));
  }, [filteredTransactions, selectedTxnIds]);

  const handleSelectAllToggle = () => {
    if (isAllSelected) {
      const next = new Set(selectedTxnIds);
      filteredTransactions.forEach(t => next.delete(t.id));
      setSelectedTxnIds(next);
    } else {
      const next = new Set(selectedTxnIds);
      filteredTransactions.forEach(t => next.add(t.id));
      setSelectedTxnIds(next);
    }
  };

  const handleSelectRow = (id: string) => {
    const next = new Set(selectedTxnIds);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setSelectedTxnIds(next);
  };

  const handleBulkTagAction = async (action: 'add_tag' | 'remove_tag', value: string) => {
    if (selectedTxnIds.size === 0) return;
    try {
      await apiClient.bulkUpdateTransactions(Array.from(selectedTxnIds), action, value);
      setSelectedTxnIds(new Set());
      loadTransactions();
    } catch (e) {
      console.error("Bulk tag action failed", e);
    }
  };

  const handleBulkNoteAction = async (action: 'set_note' | 'clear_note', value?: string) => {
    if (selectedTxnIds.size === 0) return;
    try {
      await apiClient.bulkUpdateTransactions(Array.from(selectedTxnIds), action, value);
      setSelectedTxnIds(new Set());
      loadTransactions();
    } catch (e) {
      console.error("Bulk note action failed", e);
    }
  };

  // CSV Export Logic
  const handleExportCSV = () => {
    if (filteredTransactions.length === 0) return;

    const headers = [
      'Date',
      'Account',
      'Merchant Name',
      'Raw Description',
      'Amount (USD)',
      'Type',
      'Primary Category',
      'Detailed Category',
      'Payment Channel',
      'Status',
      'Tags'
    ];

    const escapeCSV = (val?: string | null) => {
      if (val === undefined || val === null) return '""';
      const clean = val.replace(/"/g, '""');
      return `"${clean}"`;
    };

    const csvRows = [headers.join(',')];

    filteredTransactions.forEach(t => {
      const isExpense = t.amount > 0;
      const amountDollars = (Math.abs(t.amount) / 100).toFixed(2);
      const row = [
        t.txn_date,
        escapeCSV(t.account_name),
        escapeCSV(t.merchant_name || t.raw_string),
        escapeCSV(t.raw_string),
        isExpense ? `-${amountDollars}` : `+${amountDollars}`,
        isExpense ? 'Expense' : 'Income/Refund',
        escapeCSV(t.plaid_primary_category),
        escapeCSV(t.plaid_detailed_category),
        escapeCSV(t.payment_channel),
        t.pending ? 'Pending' : 'Posted',
        escapeCSV(t.tags)
      ];
      csvRows.push(row.join(','));
    });

    const blob = new Blob([csvRows.join('\n')], { type: 'text/csv;charset=utf-8;' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.setAttribute('download', `wealthagent_transactions_${new Date().toISOString().split('T')[0]}.csv`);
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
  };

  const toggleCategorySelection = (category: string) => {
    setSelectedCategories(prev => {
      if (prev.includes(category)) {
        return prev.filter(c => c !== category);
      } else {
        return [...prev, category];
      }
    });
  };

  const toggleTagSelection = (tag: string) => {
    setSelectedTags(prev => {
      if (prev.includes(tag)) {
        return prev.filter(t => t !== tag);
      } else {
        return [...prev, tag];
      }
    });
  };

  const handleSaveCurrentSearch = async () => {
    if (!newSearchName.trim()) return;
    try {
      await apiClient.createSavedSearch({
        name: newSearchName.trim(),
        search_query: searchQuery || undefined,
        selected_account: selectedAccount !== 'all' ? selectedAccount : undefined,
        selected_categories: selectedCategories.length > 0 ? JSON.stringify(selectedCategories) : undefined,
        selected_tags: selectedTags.length > 0 ? JSON.stringify(selectedTags) : undefined,
        start_date: startDate || undefined,
        end_date: endDate || undefined,
        amount_mode: amountMode !== 'all' ? amountMode : undefined,
        amount_val: amountVal || undefined,
        amount_min: amountMin || undefined,
        amount_max: amountMax || undefined,
        show_pending_only: showPendingOnly,
      });
      setNewSearchName('');
      setShowSaveModal(false);
      loadSavedSearches();
    } catch (e) {
      console.error("Failed to save search", e);
    }
  };

  const handleApplySavedSearch = (s: SavedSearch) => {
    setSearchQuery(s.search_query || '');
    setSelectedAccount(s.selected_account || 'all');
    setSelectedCategories(s.selected_categories ? JSON.parse(s.selected_categories) : []);
    setSelectedTags(s.selected_tags ? JSON.parse(s.selected_tags) : []);
    setStartDate(s.start_date || '');
    setEndDate(s.end_date || '');
    setAmountMode((s.amount_mode as any) || 'all');
    setAmountVal(s.amount_val || '');
    setAmountMin(s.amount_min || '');
    setAmountMax(s.amount_max || '');
    setShowPendingOnly(s.show_pending_only);
    setShowSavedDropdown(false);
  };

  const handleDeleteSavedSearch = async (e: React.MouseEvent, id: number) => {
    e.stopPropagation();
    try {
      await apiClient.deleteSavedSearch(id);
      loadSavedSearches();
    } catch (e) {
      console.error("Failed to delete saved search", e);
    }
  };

  const clearFilters = () => {
    setSearchQuery('');
    setSelectedAccount('all');
    setSelectedCategories([]);
    setSelectedTags([]);
    setStartDate('');
    setEndDate('');
    setShowPendingOnly(false);
    setAmountMode('all');
    setAmountVal('');
    setAmountMin('');
    setAmountMax('');
  };

  const hasActiveFilters = useMemo(() => {
    return searchQuery !== '' || 
           selectedAccount !== 'all' || 
           selectedCategories.length > 0 || 
           selectedTags.length > 0 || 
           startDate !== '' || 
           endDate !== '' || 
           showPendingOnly ||
           amountMode !== 'all';
  }, [searchQuery, selectedAccount, selectedCategories, selectedTags, startDate, endDate, showPendingOnly, amountMode]);

  const renderChannelIcon = (channel?: string) => {
    switch (channel?.toLowerCase()) {
      case 'online':
        return (
          <span title="Online Purchase">
            <Globe className="w-3.5 h-3.5 text-blue-400" />
          </span>
        );
      case 'in store':
        return (
          <span title="In-Store Purchase">
            <ShoppingBag className="w-3.5 h-3.5 text-emerald-400" />
          </span>
        );
      case 'mobile':
        return (
          <span title="Mobile App">
            <Smartphone className="w-3.5 h-3.5 text-purple-400" />
          </span>
        );
      default:
        return (
          <span title="Other / Electronic">
            <Landmark className="w-3.5 h-3.5 text-slate-500" />
          </span>
        );
    }
  };

  const getCleanCategoryName = (cat: string) => {
    return cat.replace(/_/g, ' ').toLowerCase();
  };

  const formatAmountLabel = (val: string) => {
    if (!val) return '';
    const num = parseFloat(val);
    if (isNaN(num)) return '';
    const sign = num < 0 ? '-' : '';
    return `${sign}$${Math.abs(num).toLocaleString('en-US', { minimumFractionDigits: 0, maximumFractionDigits: 2 })}`;
  };

  const handleNoteChange = useCallback(async (id: string, note: string | null) => {
    await apiClient.updateTransaction(id, { note });
    setTransactions(prev => prev.map(t => t.id === id ? { ...t, note: note ?? undefined } : t));
  }, []);

  const handleTagsChange = useCallback(async (id: string, tags: string[]) => {
    await apiClient.updateTransaction(id, { tags });
    const tagsStr = tags.length > 0 ? tags.join(',') : undefined;
    setTransactions(prev => prev.map(t => t.id === id ? { ...t, tags: tagsStr } : t));
  }, []);

  return (
    <div className="space-y-6 pb-24 relative">
      {/* Search & Filter Bar */}
      <div className="bg-slate-900 border border-slate-800 p-6 rounded-3xl backdrop-blur-xl shadow-2xl flex flex-col gap-4 relative z-20">
        
        {/* Row 1: Search & Primary Buttons */}
        <div className="flex flex-col md:flex-row justify-between items-center gap-4">
          <div className="flex-1 w-full relative">
            <Search className="w-4 h-4 text-slate-500 absolute left-4 top-3.5" />
            <input
              type="text"
              placeholder="Search transactions, tags, categories..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="w-full bg-slate-950/80 border border-slate-800 rounded-2xl py-3 pl-11 pr-4 text-xs text-white placeholder-slate-500 outline-none focus:border-blue-500/50 transition-all font-medium"
            />
          </div>

          <div className="flex items-center gap-3 w-full md:w-auto justify-end shrink-0">
            {/* Saved Searches Dropdown */}
            <div className="relative" ref={savedDropdownRef}>
              <button
                onClick={() => setShowSavedDropdown(!showSavedDropdown)}
                className="flex items-center gap-1.5 px-4 py-2.5 bg-slate-800 hover:bg-slate-700 text-xs font-bold rounded-2xl text-slate-300 transition-all border border-slate-700 font-sans"
              >
                <Bookmark className="w-3.5 h-3.5" /> Saved Searches
              </button>

              {showSavedDropdown && (
                <div className="absolute right-0 mt-2 w-64 bg-slate-950 border border-slate-800 rounded-2xl shadow-2xl p-3 z-50 max-h-72 overflow-y-auto font-sans">
                  <span className="text-[10px] font-black uppercase tracking-widest text-slate-500 block mb-2 px-1">Your Searches</span>
                  {savedSearches.length === 0 ? (
                    <span className="text-xs text-slate-500 italic block p-4 text-center">No saved searches yet.</span>
                  ) : (
                    <div className="space-y-1">
                      {savedSearches.map(s => (
                        <div
                          key={s.id}
                          onClick={() => handleApplySavedSearch(s)}
                          className="flex items-center justify-between px-2 py-1.5 w-full hover:bg-slate-900 rounded-xl text-left text-xs transition-all text-slate-300 hover:text-white cursor-pointer"
                        >
                          <span className="truncate font-bold max-w-[180px]">{s.name}</span>
                          <button
                            onClick={(e) => handleDeleteSavedSearch(e, s.id)}
                            className="p-1 text-slate-500 hover:text-rose-400 rounded-lg hover:bg-rose-500/10 transition-all shrink-0"
                            title="Delete Saved Search"
                          >
                            <Trash2 className="w-3 h-3" />
                          </button>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              )}
            </div>

            {/* Save Current Search Toggle */}
            <button
              onClick={() => setShowSaveModal(true)}
              className="flex items-center gap-1.5 px-4 py-2.5 bg-slate-800 hover:bg-slate-700 text-xs font-bold rounded-2xl text-slate-300 transition-all border border-slate-700"
              title="Save current filter configuration"
            >
              <BookmarkPlus className="w-3.5 h-3.5" /> Save Search
            </button>

            {hasActiveFilters && (
              <button
                onClick={clearFilters}
                className="flex items-center gap-1.5 px-4 py-2.5 bg-slate-800 hover:bg-slate-700 text-xs font-bold rounded-2xl text-slate-300 transition-all border border-slate-700 animate-fadeIn"
              >
                <X className="w-3.5 h-3.5" /> Clear Filters
              </button>
            )}

            {/* Export CSV Button */}
            <button
              onClick={handleExportCSV}
              disabled={filteredTransactions.length === 0}
              className="flex items-center gap-2 px-5 py-2.5 bg-emerald-600/10 hover:bg-emerald-600/20 disabled:opacity-40 text-xs font-bold rounded-2xl border border-emerald-500/20 text-emerald-400 transition-all"
            >
              <Download className="w-3.5 h-3.5" /> Export CSV
            </button>

            {/* Sync Button */}
            <button
              onClick={handleSync}
              disabled={syncing}
              className="flex items-center gap-2 px-5 py-2.5 bg-blue-600 hover:bg-blue-500 disabled:opacity-50 text-xs font-black rounded-2xl transition-all border border-blue-500/10 shadow-lg shadow-blue-900/20 text-white"
            >
              <RefreshCw className={`w-3.5 h-3.5 ${syncing ? 'animate-spin' : ''}`} />
              {syncing ? 'Syncing...' : 'Sync Transactions'}
            </button>
          </div>
        </div>

        {/* Saved Search Modal */}
        {showSaveModal && (
          <div className="absolute top-16 right-6 w-80 bg-slate-950 border border-slate-800 p-4 rounded-2xl shadow-2xl z-50 flex flex-col gap-3 font-sans">
            <div className="flex justify-between items-center border-b border-slate-800 pb-2">
              <span className="text-[10px] font-black uppercase tracking-widest text-slate-500">Save Active Filters</span>
              <button onClick={() => setShowSaveModal(false)} className="text-slate-400 hover:text-white">
                <X className="w-3.5 h-3.5" />
              </button>
            </div>
            <div className="flex flex-col gap-1.5">
              <label className="text-[9px] font-black text-slate-500 uppercase">Search Name</label>
              <input
                type="text"
                placeholder="e.g., Grocery over $100"
                value={newSearchName}
                onChange={(e) => setNewSearchName(e.target.value)}
                className="bg-slate-900 border border-slate-800 rounded-xl p-2 text-xs text-white outline-none focus:border-blue-500/40"
              />
            </div>
            <button
              onClick={handleSaveCurrentSearch}
              disabled={!newSearchName.trim()}
              className="w-full bg-blue-600 hover:bg-blue-500 disabled:opacity-50 text-xs font-bold py-2 rounded-xl text-white transition-all"
            >
              Save Configuration
            </button>
          </div>
        )}

        {/* Row 2: Dense Filter Controls */}
        <div className="flex flex-wrap items-center gap-3 pt-2 border-t border-slate-800/40">
          
          {/* Account Filter */}
          <div className="flex items-center gap-2 bg-slate-950/40 px-3 py-1.5 rounded-2xl border border-slate-800/80">
            <CreditCard className="w-3.5 h-3.5 text-slate-500" />
            <select
              value={selectedAccount}
              onChange={(e) => setSelectedAccount(e.target.value)}
              className="bg-transparent text-xs text-slate-300 outline-none border-none font-semibold cursor-pointer max-w-[150px]"
            >
              <option value="all" className="bg-slate-950 text-slate-300">All Accounts</option>
              {uniqueAccounts.map(acc => (
                <option key={acc} value={acc} className="bg-slate-950 text-slate-300">{acc}</option>
              ))}
            </select>
          </div>

          {/* Custom Category Multi-Select Dropdown */}
          <div className="relative" ref={categoryDropdownRef}>
            <button
              onClick={() => setShowCategoryDropdown(!showCategoryDropdown)}
              className="flex items-center gap-2 bg-slate-950/40 px-3 py-1.5 rounded-2xl border border-slate-800/80 text-xs text-slate-300 font-semibold transition-all hover:border-slate-700"
            >
              <Filter className="w-3.5 h-3.5 text-slate-500" />
              <span>
                {selectedCategories.length === 0
                  ? 'All Categories'
                  : `Categories (${selectedCategories.length})`}
              </span>
            </button>

            {showCategoryDropdown && (
              <div className="absolute left-0 mt-2 w-64 bg-slate-950 border border-slate-800 rounded-2xl shadow-2xl p-3 z-50 max-h-72 overflow-y-auto font-sans">
                <div className="flex justify-between items-center mb-2 pb-2 border-b border-slate-800/60 font-sans">
                  <span className="text-[10px] font-black uppercase tracking-widest text-slate-500">Select Categories</span>
                  {selectedCategories.length > 0 && (
                    <button 
                      onClick={() => setSelectedCategories([])}
                      className="text-[9px] text-blue-400 font-bold hover:underline"
                    >
                      Clear
                    </button>
                  )}
                </div>
                <div className="space-y-1.5 font-sans">
                  {uniqueCategories.map(cat => {
                    const isSelected = selectedCategories.includes(cat);
                    return (
                      <button
                        key={cat}
                        onClick={() => toggleCategorySelection(cat)}
                        className="flex items-center gap-2 px-2 py-1.5 w-full hover:bg-slate-900 rounded-xl text-left text-xs transition-all text-slate-300 hover:text-white"
                      >
                        {isSelected ? (
                          <CheckSquare className="w-4 h-4 text-blue-500 shrink-0" />
                        ) : (
                          <Square className="w-4 h-4 text-slate-700 shrink-0" />
                        )}
                        <span className="capitalize truncate">{getCleanCategoryName(cat)}</span>
                      </button>
                    );
                  })}
                </div>
              </div>
            )}
          </div>

          {/* Custom Tags Multi-Select Dropdown */}
          <div className="relative" ref={tagDropdownRef}>
            <button
              onClick={() => setShowTagDropdown(!showTagDropdown)}
              className="flex items-center gap-2 bg-slate-950/40 px-3 py-1.5 rounded-2xl border border-slate-800/80 text-xs text-slate-300 font-semibold transition-all hover:border-slate-700 font-sans"
            >
              <Tag className="w-3.5 h-3.5 text-slate-500" />
              <span>
                {selectedTags.length === 0
                  ? 'All Tags'
                  : `Tags (${selectedTags.length})`}
              </span>
            </button>

            {showTagDropdown && (
              <div className="absolute left-0 mt-2 w-64 bg-slate-950 border border-slate-800 rounded-2xl shadow-2xl p-3 z-50 max-h-72 overflow-y-auto font-sans">
                <div className="flex justify-between items-center mb-2 pb-2 border-b border-slate-800/60 font-sans">
                  <span className="text-[10px] font-black uppercase tracking-widest text-slate-500">Select Tags</span>
                  {selectedTags.length > 0 && (
                    <button 
                      onClick={() => setSelectedTags([])}
                      className="text-[9px] text-blue-400 font-bold hover:underline"
                    >
                      Clear
                    </button>
                  )}
                </div>
                <div className="space-y-1.5 font-sans">
                  {uniqueTags.map(tag => {
                    const isSelected = selectedTags.includes(tag);
                    return (
                      <button
                        key={tag}
                        onClick={() => toggleTagSelection(tag)}
                        className="flex items-center gap-2 px-2 py-1.5 w-full hover:bg-slate-900 rounded-xl text-left text-xs transition-all text-slate-300 hover:text-white"
                      >
                        {isSelected ? (
                          <CheckSquare className="w-4 h-4 text-blue-500 shrink-0" />
                        ) : (
                          <Square className="w-4 h-4 text-slate-700 shrink-0" />
                        )}
                        <span className="truncate">{tag}</span>
                      </button>
                    );
                  })}
                  {uniqueTags.length === 0 && (
                    <span className="text-xs text-slate-500 italic block p-4 text-center">No active tags found.</span>
                  )}
                </div>
              </div>
            )}
          </div>

          {/* Amount Filter Dropdown */}
          <div className="relative" ref={amountDropdownRef}>
            <button
              onClick={() => setShowAmountDropdown(!showAmountDropdown)}
              className="flex items-center gap-2 bg-slate-950/40 px-3 py-1.5 rounded-2xl border border-slate-800/80 text-xs text-slate-300 font-semibold transition-all hover:border-slate-700 font-sans"
            >
              <DollarSign className="w-3.5 h-3.5 text-slate-500" />
              <span>
                {amountMode === 'all' && 'All Amounts'}
                {amountMode === 'less' && amountVal && `< ${formatAmountLabel(amountVal)}`}
                {amountMode === 'greater' && amountVal && `> ${formatAmountLabel(amountVal)}`}
                {amountMode === 'equal' && amountVal && `= ${formatAmountLabel(amountVal)}`}
                {amountMode === 'between' && (amountMin || amountMax) && (
                  `${amountMin ? formatAmountLabel(amountMin) : '$0'} - ${amountMax ? formatAmountLabel(amountMax) : '∞'}`
                )}
                {amountMode !== 'all' && (!amountVal && amountMode !== 'between') && 'Amount...'}
              </span>
            </button>

            {showAmountDropdown && (
              <div className="absolute left-0 mt-2 w-64 bg-slate-950 border border-slate-800 rounded-2xl shadow-2xl p-4 z-50 space-y-3 font-sans">
                <div className="flex justify-between items-center pb-2 border-b border-slate-800/60 font-sans">
                  <span className="text-[10px] font-black uppercase tracking-widest text-slate-500">Filter by Amount</span>
                  {amountMode !== 'all' && (
                    <button 
                      onClick={() => { setAmountMode('all'); setAmountVal(''); setAmountMin(''); setAmountMax(''); }}
                      className="text-[9px] text-blue-400 font-bold hover:underline"
                    >
                      Reset
                    </button>
                  )}
                </div>
                
                <div className="flex flex-col gap-1.5">
                  <label className="text-[9px] font-black text-slate-500 uppercase">Operator</label>
                  <select
                    value={amountMode}
                    onChange={(e) => setAmountMode(e.target.value as any)}
                    className="w-full bg-slate-900 border border-slate-800 rounded-xl p-2 text-xs text-slate-200 outline-none cursor-pointer"
                  >
                    <option value="all">All Amounts</option>
                    <option value="less">Less Than (&lt;)</option>
                    <option value="greater">Greater Than (&gt;)</option>
                    <option value="equal">Equal To (=)</option>
                    <option value="between">Between Range</option>
                  </select>
                </div>

                {amountMode !== 'all' && amountMode !== 'between' && (
                  <div className="flex flex-col gap-1.5">
                    <label className="text-[9px] font-black text-slate-500 uppercase">Dollar Value ($)</label>
                    <input
                      type="number"
                      step="0.01"
                      placeholder="100.00"
                      value={amountVal}
                      onChange={(e) => setAmountVal(e.target.value)}
                      className="w-full bg-slate-900 border border-slate-800 rounded-xl p-2 text-xs text-white outline-none focus:border-blue-500/40"
                    />
                  </div>
                )}

                {amountMode === 'between' && (
                  <div className="grid grid-cols-2 gap-2">
                    <div className="flex flex-col gap-1.5">
                      <label className="text-[9px] font-black text-slate-500 uppercase">Min ($)</label>
                      <input
                        type="number"
                        step="0.01"
                        placeholder="0.00"
                        value={amountMin}
                        onChange={(e) => setAmountMin(e.target.value)}
                        className="w-full bg-slate-900 border border-slate-800 rounded-xl p-2 text-xs text-white outline-none focus:border-blue-500/40"
                      />
                    </div>
                    <div className="flex flex-col gap-1.5">
                      <label className="text-[9px] font-black text-slate-500 uppercase">Max ($)</label>
                      <input
                        type="number"
                        step="0.01"
                        placeholder="500.00"
                        value={amountMax}
                        onChange={(e) => setAmountMax(e.target.value)}
                        className="w-full bg-slate-900 border border-slate-800 rounded-xl p-2 text-xs text-white outline-none focus:border-blue-500/40"
                      />
                    </div>
                  </div>
                )}
              </div>
            )}
          </div>

          {/* Date Range Selector */}
          <div className="flex items-center gap-2 bg-slate-950/40 px-3 py-1.5 rounded-2xl border border-slate-800/80 font-sans">
            <Calendar className="w-3.5 h-3.5 text-slate-500" />
            <div className="flex items-center gap-1.5">
              <input
                type="date"
                value={startDate}
                onChange={(e) => setStartDate(e.target.value)}
                className="bg-transparent text-xs text-slate-300 outline-none border-none font-semibold cursor-pointer py-0.5"
                title="Start Date"
              />
              <span className="text-slate-600 text-[10px] font-bold">to</span>
              <input
                type="date"
                value={endDate}
                onChange={(e) => setEndDate(e.target.value)}
                className="bg-transparent text-xs text-slate-300 outline-none border-none font-semibold cursor-pointer py-0.5"
                title="End Date"
              />
            </div>
          </div>

          {/* Pending Toggle */}
          <button
            onClick={() => setShowPendingOnly(!showPendingOnly)}
            className={`px-4 py-1.5 rounded-2xl border text-xs font-semibold transition-all ${
              showPendingOnly
                ? 'bg-amber-500/10 border-amber-500/40 text-amber-400 shadow-lg shadow-amber-950/20'
                : 'bg-slate-950/40 border-slate-800/80 text-slate-400 hover:text-slate-300'
            }`}
          >
            Pending Only
          </button>
        </div>
      </div>

      {/* Dynamic Summary Metrics */}
      {!loading && (
        <>
          {loadingMore && (
            <div className="flex items-center gap-2.5 text-xs text-slate-400 bg-slate-900 border border-slate-800 rounded-2xl px-4 py-2.5 w-fit">
              <RefreshCw className="w-3.5 h-3.5 animate-spin text-blue-400 shrink-0" />
              <span>Loading <span className="text-slate-200 font-semibold">{transactions.length.toLocaleString()}</span> of <span className="text-slate-200 font-semibold">{totalCount.toLocaleString()}</span> transactions — numbers will update when complete</span>
            </div>
          )}
        <div className={`grid grid-cols-2 lg:grid-cols-4 gap-4 transition-opacity duration-300 ${loadingMore ? 'opacity-40 pointer-events-none' : 'animate-fadeIn'}`}>
          {/* Card 1: Count */}
          <div className="bg-slate-900 border border-slate-800 p-4 rounded-2xl flex flex-col justify-center">
            <span className="text-[9px] font-black text-slate-500 uppercase tracking-widest mb-1">Volume</span>
            <span className="text-xl font-bold text-slate-200">
              {summaryMetrics.count} <span className="text-xs text-slate-500 font-medium">Record{summaryMetrics.count !== 1 ? 's' : ''}</span>
            </span>
          </div>

          {/* Card 2: Inflow (Deposits) */}
          <div className="bg-slate-900 border border-slate-800 p-4 rounded-2xl flex flex-col justify-center">
            <span className="text-[9px] font-black text-slate-500 uppercase tracking-widest mb-1">Total Inflow</span>
            <span className="text-xl font-bold text-emerald-400">
              + {formatCurrency(summaryMetrics.inflow)}
            </span>
          </div>

          {/* Card 3: Outflow (Expenses) */}
          <div className="bg-slate-900 border border-slate-800 p-4 rounded-2xl flex flex-col justify-center">
            <span className="text-[9px] font-black text-slate-500 uppercase tracking-widest mb-1">Total Outflow</span>
            <span className="text-xl font-bold text-slate-400">
              - {formatCurrency(summaryMetrics.outflow)}
            </span>
          </div>

          {/* Card 4: Net Flow */}
          <div className="bg-slate-900 border border-slate-800 p-4 rounded-2xl flex flex-col justify-center">
            <span className="text-[9px] font-black text-slate-500 uppercase tracking-widest mb-1">Net Flow</span>
            <span className={`text-xl font-bold ${summaryMetrics.net >= 0 ? 'text-emerald-400' : 'text-rose-500'}`}>
              {summaryMetrics.net < 0 ? '-' : '+'} {formatCurrency(Math.abs(summaryMetrics.net))}
            </span>
          </div>
        </div>
        </>
      )}

      {/* Main Transactions Table */}
      <div className="bg-slate-900 border border-slate-800 rounded-3xl overflow-hidden shadow-2xl relative">
        <div className="overflow-x-auto">
          <table className="w-full text-left border-collapse">
            <thead className="bg-slate-950/40 text-[10px] text-slate-500 uppercase tracking-widest font-black border-b border-slate-800/60">
              <tr>
                <th className="p-5 w-12 text-center">
                  <button
                    onClick={handleSelectAllToggle}
                    className="p-1 hover:bg-slate-800 rounded-lg text-slate-500 hover:text-slate-300"
                    title="Toggle select all filtered transactions"
                  >
                    {isAllSelected ? <CheckSquare className="w-4 h-4 text-blue-500" /> : <Square className="w-4 h-4" />}
                  </button>
                </th>
                <SortableTh col="date"     label="Date"     activeCol={sortCol} dir={sortDir} onSort={handleSort} className="w-28" />
                <SortableTh col="account"  label="Account"  activeCol={sortCol} dir={sortDir} onSort={handleSort} className="w-44" />
                <SortableTh col="merchant" label="Transaction Details" activeCol={sortCol} dir={sortDir} onSort={handleSort} />
                <SortableTh col="category" label="Plaid Category"      activeCol={sortCol} dir={sortDir} onSort={handleSort} className="w-48" />
                <th className="p-5 w-40">Tags</th>
                <th className="p-5 w-52">Note</th>
                <th className="p-5 w-16 text-center">Type</th>
                <SortableTh col="amount" label="Amount" activeCol={sortCol} dir={sortDir} onSort={handleSort} className="w-36" align="right" />
              </tr>
            </thead>
            <tbody className="text-xs divide-y divide-slate-800/50 text-slate-300">
              {loading ? (
                <tr>
                  <td colSpan={9} className="p-16 text-center text-slate-500 font-semibold italic animate-pulse">
                    Retrieving secure Plaid transaction logs...
                  </td>
                </tr>
              ) : sortedTransactions.length === 0 ? (
                <tr>
                  <td colSpan={9} className="p-16 text-center text-slate-500 font-medium italic">
                    No matching transactions found. Try adjusting your filters or click Sync!
                  </td>
                </tr>
              ) : (
                sortedTransactions.map((t) => {
                  const isExpense = t.amount > 0;
                  const displayMerchant = t.merchant_name || t.raw_string;
                  const showSubName = t.merchant_name && t.merchant_name !== t.raw_string;
                  const isRowChecked = selectedTxnIds.has(t.id);

                  return (
                    <tr 
                      key={t.id} 
                      className={`transition-all duration-150 border-l-[3px] ${
                        isRowChecked 
                          ? 'bg-blue-600/5 border-l-blue-500' 
                          : 'hover:bg-slate-800/20 border-l-transparent'
                      }`}
                    >
                      {/* Checkbox */}
                      <td className="p-5 text-center">
                        <button 
                          onClick={() => handleSelectRow(t.id)}
                          className={`p-1 hover:bg-slate-800/60 rounded-lg transition-all ${
                            isRowChecked ? 'text-blue-500 animate-scaleIn' : 'text-slate-600 hover:text-slate-400'
                          }`}
                        >
                          {isRowChecked ? (
                            <CheckSquare className="w-4 h-4" />
                          ) : (
                            <Square className="w-4 h-4" />
                          )}
                        </button>
                      </td>

                      {/* Date */}
                      <td className="p-5 text-slate-500 font-bold tracking-wider font-mono text-[10px]">
                        {(() => {
                          const [year, month, day] = t.txn_date.split('-');
                          return `${month}/${day}/${year}`;
                        })()}
                      </td>

                      {/* Account */}
                      <td className="p-5">
                        <span className="px-2.5 py-1 bg-slate-950 text-slate-400 font-semibold border border-slate-800/80 rounded-xl max-w-[160px] truncate block text-[10px]" title={isLocked(t.account_name) ? undefined : t.account_name}>
                          <LockedValue value={t.account_name} />
                        </span>
                      </td>

                      {/* Details (Merchant + Raw String) */}
                      <td className="p-5">
                        <div className="flex flex-col gap-0.5 max-w-md">
                          <div className="flex items-center gap-2">
                            <span className="font-bold text-slate-100 text-sm tracking-tight"><LockedValue value={displayMerchant} /></span>
                            {t.pending && (
                              <span className="px-1.5 py-0.5 bg-amber-500/10 text-amber-400 border border-amber-500/20 text-[8px] font-black uppercase rounded tracking-wider">
                                Pending
                              </span>
                            )}
                          </div>
                          {showSubName && (
                            <span className="text-[10px] text-slate-500 truncate leading-tight font-mono" title={t.raw_string}>
                              {t.raw_string}
                            </span>
                          )}
                        </div>
                      </td>

                      {/* Plaid Category */}
                      <td className="p-5">
                        <div className="flex flex-col gap-1">
                          {t.plaid_primary_category ? (
                            <span className="text-[10px] text-blue-400 font-black uppercase tracking-wider">
                              {getCleanCategoryName(t.plaid_primary_category)}
                            </span>
                          ) : (
                            <span className="text-[10px] text-slate-600 italic">Uncategorized</span>
                          )}
                          {t.plaid_detailed_category && (
                            <span className="text-[9px] text-slate-500 truncate capitalize max-w-[180px]" title={getCleanCategoryName(t.plaid_detailed_category)}>
                              {getCleanCategoryName(t.plaid_detailed_category)}
                            </span>
                          )}
                        </div>
                      </td>

                      {/* Tags */}
                      <td className="p-5">
                        <TagCell txnId={t.id} tags={t.tags} onSave={handleTagsChange} />
                      </td>

                      {/* Note */}
                      <td className="p-5">
                        <NoteCell
                          txnId={t.id}
                          note={t.note}
                          onSave={handleNoteChange}
                        />
                      </td>

                      {/* Payment Channel Icon */}
                      <td className="p-5 text-center">
                        <div className="inline-flex p-2 bg-slate-950/60 rounded-xl border border-slate-800/80">
                          {renderChannelIcon(t.payment_channel)}
                        </div>
                      </td>

                      {/* Amount */}
                      <td className="p-5 text-right">
                        <span className={`text-sm font-black font-mono tracking-tight ${
                          isExpense ? 'text-slate-100' : 'text-emerald-400'
                        }`}>
                          {isExpense ? '-' : '+'}{formatCurrency(t.amount)}
                        </span>
                      </td>
                    </tr>
                  );
                })
              )}
            </tbody>
          </table>
        </div>
      </div>

      {/* Floating Bulk Actions Tray */}
      {selectedTxnIds.size > 0 && (
        <BulkActionTray
          selectedCount={selectedTxnIds.size}
          onTagAction={handleBulkTagAction}
          onNoteAction={handleBulkNoteAction}
          onClearSelection={() => setSelectedTxnIds(new Set())}
        />
      )}
    </div>
  );
};

// ── Capital Gains Panel ───────────────────────────────────────────────────
const CapitalGainsPanel: React.FC = () => {
  const { hidden } = usePrivacy();
  const currentYear = new Date().getFullYear();
  const [year, setYear] = useState(currentYear);
  const [report, setReport] = useState<CapitalGainsReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [lockedGains, setLockedGains] = useState(false);
  const [showUnlock, setShowUnlock] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [realizedSortCol, setRealizedSortCol] = useState<string | null>('close_date');
  const [realizedSortDir, setRealizedSortDir] = useState<SortDir>('desc');
  const [unrealizedSortCol, setUnrealizedSortCol] = useState<string | null>('gain');
  const [unrealizedSortDir, setUnrealizedSortDir] = useState<SortDir>('asc');
  const [editingTxnId, setEditingTxnId] = useState<string | null>(null);
  const [cbInput, setCbInput] = useState('');
  const [isLtInput, setIsLtInput] = useState(false);
  const [savingCb, setSavingCb] = useState(false);

  const loadReport = (y: number) => {
    setLoading(true);
    setReport(null);
    setLockedGains(false);
    apiClient.getCapitalGains(y)
      .then(r => { setReport(r); setLoading(false); })
      .catch(e => {
        // FIFO matching needs decrypted tickers, so a locked Privacy Lock
        // session gets a 403 with an "unlock" message instead of a report.
        const msg = e instanceof Error ? e.message : '';
        if (/unlock|locked/i.test(msg)) setLockedGains(true);
        else console.error('Capital gains load failed', e);
        setLoading(false);
      });
  };

  useEffect(() => { loadReport(year); }, [year]);

  const handleSync = async () => {
    setSyncing(true);
    try {
      await apiClient.syncPlaidData();
      loadReport(year);
    } catch (e) {
      console.error('Sync failed', e);
      setLoading(false);
    } finally {
      setSyncing(false);
    }
  };

  const startEditCb = (txnId: string, existing: number | null, existingLt: boolean | null) => {
    setEditingTxnId(txnId);
    setCbInput(existing !== null ? (existing / 100).toFixed(2) : '');
    setIsLtInput(existingLt ?? false);
  };

  const cancelEditCb = () => { setEditingTxnId(null); setCbInput(''); };

  const saveCb = async (txnId: string) => {
    const cents = Math.round(parseFloat(cbInput) * 100);
    if (isNaN(cents) || cents < 0) return;
    setSavingCb(true);
    try {
      await apiClient.setCostBasis(txnId, cents, isLtInput);
      setEditingTxnId(null);
      setCbInput('');
      loadReport(year);
    } catch (e) { console.error('Save cost basis failed', e); }
    finally { setSavingCb(false); }
  };

  const clearCb = async (txnId: string) => {
    try {
      await apiClient.deleteCostBasis(txnId);
      loadReport(year);
    } catch (e) { console.error('Clear cost basis failed', e); }
  };

  const sourceBadge = (source: string) => {
    if (source === 'user_input') return (
      <span className="px-1.5 py-0.5 bg-blue-500/10 text-blue-400 border border-blue-500/20 rounded text-[9px] font-black tracking-wide">Manual</span>
    );
    if (source === '1099b') return (
      <span className="px-1.5 py-0.5 bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 rounded text-[9px] font-black tracking-wide">1099-B</span>
    );
    return (
      <span className="px-1.5 py-0.5 bg-slate-700/40 text-slate-500 border border-slate-700/60 rounded text-[9px] font-black tracking-wide">FIFO</span>
    );
  };

  const fmt = (cents: number) =>
    hidden ? MONEY_MASK : (Math.abs(cents) / 100).toLocaleString('en-US', { style: 'currency', currency: 'USD' });

  const fmtDate = (d: string) => {
    if (!d || d === 'N/A') return '—';
    const [y, m, day] = d.split('-');
    return `${m}/${day}/${y}`;
  };

  const gainColor = (cents: number) =>
    cents >= 0 ? 'text-emerald-400' : 'text-red-400';

  const gainSign = (cents: number) => cents >= 0 ? '+' : '−';

  const handleRealizedSort = (col: string) => {
    if (realizedSortCol !== col) { setRealizedSortCol(col); setRealizedSortDir('desc'); }
    else if (realizedSortDir === 'desc') setRealizedSortDir('asc');
    else setRealizedSortCol(null);
  };

  const handleUnrealizedSort = (col: string) => {
    if (unrealizedSortCol !== col) { setUnrealizedSortCol(col); setUnrealizedSortDir('asc'); }
    else if (unrealizedSortDir === 'asc') setUnrealizedSortDir('desc');
    else setUnrealizedSortCol(null);
  };

  const sortedRealized = useMemo(() => {
    if (!report) return [];
    const lots = [...report.realized_lots];
    if (!realizedSortCol) return lots;
    return lots.sort((a, b) => {
      let av: string | number = '';
      let bv: string | number = '';
      switch (realizedSortCol) {
        case 'close_date': av = a.close_date; bv = b.close_date; break;
        case 'open_date':  av = a.open_date;  bv = b.open_date;  break;
        case 'symbol':     av = (a.symbol || '').toLowerCase(); bv = (b.symbol || '').toLowerCase(); break;
        case 'gain':       av = a.gain_cents; bv = b.gain_cents; break;
        case 'cost':       av = a.cost_basis_cents; bv = b.cost_basis_cents; break;
      }
      if (av < bv) return realizedSortDir === 'asc' ? -1 : 1;
      if (av > bv) return realizedSortDir === 'asc' ? 1 : -1;
      return 0;
    });
  }, [report, realizedSortCol, realizedSortDir]);

  const sortedUnrealized = useMemo(() => {
    if (!report) return [];
    const positions = [...report.unrealized_positions];
    if (!unrealizedSortCol) return positions;
    return positions.sort((a, b) => {
      let av: string | number = '';
      let bv: string | number = '';
      switch (unrealizedSortCol) {
        case 'symbol': av = (a.symbol || '').toLowerCase(); bv = (b.symbol || '').toLowerCase(); break;
        case 'gain':   av = a.gain_cents ?? -Infinity; bv = b.gain_cents ?? -Infinity; break;
        case 'cost':   av = a.cost_basis_cents; bv = b.cost_basis_cents; break;
        case 'since':  av = a.oldest_lot_date;  bv = b.oldest_lot_date;  break;
      }
      if (av < bv) return unrealizedSortDir === 'asc' ? -1 : 1;
      if (av > bv) return unrealizedSortDir === 'asc' ? 1 : -1;
      return 0;
    });
  }, [report, unrealizedSortCol, unrealizedSortDir]);

  const harvestCount = useMemo(() =>
    report?.unrealized_positions.filter(p => (p.gain_cents ?? 0) < 0).length ?? 0,
    [report]
  );

  const handleExportCSV = () => {
    if (!report || report.realized_lots.length === 0) return;
    const headers = ['Close Date', 'Open Date', 'Symbol', 'Description', 'Qty', 'Cost Basis', 'Proceeds', 'Gain/Loss', 'Term'];
    const escape = (v?: string | null) => `"${(v ?? '').replace(/"/g, '""')}"`;
    const rows = [headers.join(','), ...sortedRealized.map(lot => [
      lot.close_date, lot.open_date,
      escape(lot.symbol), escape(lot.security_name),
      lot.quantity.toFixed(4),
      (lot.cost_basis_cents / 100).toFixed(2),
      (lot.proceeds_cents / 100).toFixed(2),
      (lot.gain_cents / 100).toFixed(2),
      lot.is_long_term ? 'Long-Term' : 'Short-Term',
    ].join(','))];
    const blob = new Blob([rows.join('\n')], { type: 'text/csv;charset=utf-8;' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.setAttribute('download', `capital_gains_${year}.csv`);
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
  };

  const yearOptions = [currentYear, currentYear - 1, currentYear - 2];

  const s = report?.summary;

  if (lockedGains) {
    return (
      <div className="flex flex-col items-center justify-center gap-4 py-24 text-center">
        <div className="p-4 bg-amber-500/10 border border-amber-500/30 rounded-2xl">
          <Lock className="w-8 h-8 text-amber-400" />
        </div>
        <div>
          <h3 className="text-sm font-black text-slate-100">Capital gains are locked</h3>
          <p className="text-xs text-slate-500 mt-1 max-w-sm">
            Gains are computed from your encrypted tickers and trade history.
            Enter your Privacy Key to unlock this session and view the report.
          </p>
        </div>
        <button
          onClick={() => setShowUnlock(true)}
          className="flex items-center gap-2 px-5 py-2 bg-amber-600 hover:bg-amber-500 text-xs font-black rounded-2xl text-white"
        >
          <Lock className="w-3.5 h-3.5" /> Unlock
        </button>
        {showUnlock && <UnlockDialog onClose={() => setShowUnlock(false)} />}
      </div>
    );
  }

  return (
    <div className="space-y-6 pb-24">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="flex items-center gap-2 bg-slate-900 border border-slate-800 px-3 py-1.5 rounded-2xl">
            <Calendar className="w-3.5 h-3.5 text-slate-500" />
            <select value={year} onChange={e => setYear(Number(e.target.value))}
              className="bg-transparent text-xs text-slate-300 outline-none border-none font-black cursor-pointer">
              {yearOptions.map(y => <option key={y} value={y} className="bg-slate-950">{y}</option>)}
            </select>
          </div>
          <button onClick={handleSync} disabled={syncing || loading}
            className="flex items-center gap-2 px-5 py-2 bg-blue-600 hover:bg-blue-500 disabled:opacity-50 text-xs font-black rounded-2xl border border-blue-500/10 shadow-lg shadow-blue-900/20 text-white">
            <RefreshCw className={`w-3.5 h-3.5 ${syncing ? 'animate-spin' : ''}`} />
            {syncing ? 'Syncing...' : 'Sync Now'}
          </button>
          {!loading && report && (
            <button onClick={handleExportCSV} disabled={report.realized_lots.length === 0}
              className="flex items-center gap-2 px-4 py-1.5 bg-emerald-600/10 hover:bg-emerald-600/20 disabled:opacity-40 text-xs font-bold rounded-2xl border border-emerald-500/20 text-emerald-400">
              <Download className="w-3.5 h-3.5" /> Export CSV
            </button>
          )}
        </div>
        <div className="flex items-center gap-1.5 text-[10px] text-slate-600 font-medium">
          <AlertCircle className="w-3 h-3" />
          FIFO method · 730-day history · wash sale not applied · not tax advice
        </div>
      </div>

      {/* Summary cards */}
      {!loading && s && (
        <div className="grid grid-cols-2 lg:grid-cols-4 gap-4">
          <div className="bg-slate-900 border border-slate-800 p-4 rounded-2xl">
            <span className="text-[9px] font-black text-slate-500 uppercase tracking-widest block mb-1">{year} Net Realized</span>
            <span className={`text-xl font-black font-mono ${gainColor(s.ytd_net_cents)}`}>
              {gainSign(s.ytd_net_cents)}{fmt(s.ytd_net_cents)}
            </span>
          </div>
          <div className="bg-slate-900 border border-slate-800 p-4 rounded-2xl">
            <span className="text-[9px] font-black text-slate-500 uppercase tracking-widest block mb-0.5">Short-Term</span>
            <span className={`text-base font-black font-mono ${gainColor(s.short_term_net_cents)}`}>
              {gainSign(s.short_term_net_cents)}{fmt(s.short_term_net_cents)}
            </span>
            <span className="text-[9px] text-slate-600 block mt-0.5">Taxed as ordinary income</span>
          </div>
          <div className="bg-slate-900 border border-slate-800 p-4 rounded-2xl">
            <span className="text-[9px] font-black text-slate-500 uppercase tracking-widest block mb-0.5">Long-Term</span>
            <span className={`text-base font-black font-mono ${gainColor(s.long_term_net_cents)}`}>
              {gainSign(s.long_term_net_cents)}{fmt(s.long_term_net_cents)}
            </span>
            <span className="text-[9px] text-slate-600 block mt-0.5">Preferential rate ≤ 20%</span>
          </div>
          <div className={`p-4 rounded-2xl border ${harvestCount > 0 ? 'bg-emerald-950/30 border-emerald-800/40' : 'bg-slate-900 border-slate-800'}`}>
            <span className="text-[9px] font-black text-slate-500 uppercase tracking-widest block mb-0.5">Harvest Opportunities</span>
            <div className="flex items-center gap-2">
              {harvestCount > 0 && <Leaf className="w-4 h-4 text-emerald-400 shrink-0" />}
              <span className={`text-xl font-black ${harvestCount > 0 ? 'text-emerald-400' : 'text-slate-500'}`}>
                {harvestCount}
              </span>
              <span className="text-[10px] text-slate-500 font-medium">positions at a loss</span>
            </div>
            {s.unrealized_loss_cents != null && s.unrealized_loss_cents < 0 && (
              <span className="text-[9px] text-red-400 block mt-0.5">
                {fmt(s.unrealized_loss_cents)} available to harvest
              </span>
            )}
          </div>
        </div>
      )}

      {/* Realized Gains / Losses */}
      <div className="bg-slate-900 border border-slate-800 rounded-3xl overflow-hidden shadow-2xl">
        <div className="px-6 py-4 border-b border-slate-800/60 flex items-center justify-between">
          <div className="flex items-center gap-2">
            <BarChart3 className="w-4 h-4 text-slate-400" />
            <span className="text-sm font-black text-slate-200">Realized Gains &amp; Losses — {year}</span>
            {report && <span className="text-[10px] text-slate-500 font-medium ml-1">{report.realized_lots.length} events</span>}
          </div>
        </div>
        <div className="overflow-x-auto">
          <table className="w-full text-left border-collapse">
            <thead className="bg-slate-950/40 text-[10px] text-slate-500 uppercase tracking-widest font-black border-b border-slate-800/60">
              <tr>
                <SortableTh col="close_date" label="Sold"      activeCol={realizedSortCol} dir={realizedSortDir} onSort={handleRealizedSort} className="w-28" />
                <SortableTh col="open_date"  label="Bought"    activeCol={realizedSortCol} dir={realizedSortDir} onSort={handleRealizedSort} className="w-28" />
                <SortableTh col="symbol"     label="Symbol"    activeCol={realizedSortCol} dir={realizedSortDir} onSort={handleRealizedSort} className="w-28" />
                <th className="p-5">Description</th>
                <th className="p-5 text-right w-16">Qty</th>
                <th className="p-5 text-right w-32">Term</th>
                <SortableTh col="cost" label="Cost Basis" activeCol={realizedSortCol} dir={realizedSortDir} onSort={handleRealizedSort} className="w-32" align="right" />
                <th className="p-5 text-right w-32">Proceeds</th>
                <SortableTh col="gain" label="Gain / Loss" activeCol={realizedSortCol} dir={realizedSortDir} onSort={handleRealizedSort} className="w-32" align="right" />
              </tr>
            </thead>
            <tbody className="text-xs divide-y divide-slate-800/50 text-slate-300">
              {loading ? (
                <tr><td colSpan={9} className="p-16 text-center text-slate-500 italic animate-pulse">Computing capital gains...</td></tr>
              ) : sortedRealized.length === 0 ? (
                <tr><td colSpan={9} className="p-16 text-center text-slate-500 italic">No realized gains or losses in {year}.</td></tr>
              ) : sortedRealized.map((lot, i) => {
                const isManual = lot.source === 'user_input' && !!lot.txn_id;
                const isEditing = isManual && editingTxnId === lot.txn_id;
                return (
                <tr key={lot.txn_id ?? `fifo-${i}`} className="hover:bg-slate-800/20 transition-colors">
                  <td className="p-5 text-slate-500 font-mono text-[10px] font-bold">{fmtDate(lot.close_date)}</td>
                  <td className="p-5 text-slate-600 font-mono text-[10px]">{fmtDate(lot.open_date)}</td>
                  <td className="p-5">
                    {lot.symbol ? (
                      <span className="px-2 py-1 bg-blue-500/10 text-blue-300 border border-blue-500/20 rounded-lg text-[10px] font-black tracking-wider">
                        {lot.symbol}
                      </span>
                    ) : <span className="text-slate-600 italic text-[10px]">—</span>}
                  </td>
                  <td className="p-5 max-w-[200px] truncate text-slate-400 text-[11px]">{lot.security_name ?? '—'}</td>
                  <td className="p-5 text-right font-mono text-[11px]">{lot.quantity.toLocaleString('en-US', { maximumFractionDigits: 4 })}</td>
                  <td className="p-5 text-right">
                    <div className="flex flex-col items-end gap-1">
                      <span className={`px-2 py-0.5 rounded border text-[10px] font-black uppercase tracking-wider ${lot.is_long_term ? 'text-purple-400 bg-purple-500/10 border-purple-500/20' : 'text-amber-400 bg-amber-500/10 border-amber-500/20'}`}>
                        {lot.is_long_term ? 'Long' : 'Short'}
                      </span>
                      {sourceBadge(lot.source)}
                    </div>
                  </td>
                  {/* Cost Basis — editable for Manual lots */}
                  <td className="p-5 text-right">
                    {isEditing ? (
                      <div className="flex flex-col items-end gap-1.5">
                        <div className="flex items-center gap-1.5">
                          <span className="text-slate-500 text-[10px]">$</span>
                          <input
                            type="number"
                            min="0"
                            step="0.01"
                            value={cbInput}
                            onChange={e => setCbInput(e.target.value)}
                            placeholder="0.00"
                            className="w-28 bg-slate-800 border border-slate-600 rounded-lg px-2 py-1 text-[11px] text-slate-200 font-mono text-right focus:outline-none focus:border-blue-500"
                            autoFocus
                          />
                        </div>
                        <select
                          value={isLtInput ? 'lt' : 'st'}
                          onChange={e => setIsLtInput(e.target.value === 'lt')}
                          className="bg-slate-800 border border-slate-600 rounded-lg px-2 py-1 text-[10px] text-slate-300 focus:outline-none focus:border-blue-500"
                        >
                          <option value="st">Short-term</option>
                          <option value="lt">Long-term</option>
                        </select>
                      </div>
                    ) : (
                      <span className="font-mono text-[11px] text-slate-400">{fmt(lot.cost_basis_cents)}</span>
                    )}
                  </td>
                  <td className="p-5 text-right font-mono text-[11px] text-slate-400">{fmt(lot.proceeds_cents)}</td>
                  <td className="p-5 text-right">
                    {isEditing ? (
                      <div className="flex items-center justify-end gap-1.5">
                        <button
                          onClick={() => saveCb(lot.txn_id!)}
                          disabled={savingCb || !cbInput}
                          className="px-2.5 py-1 bg-blue-600 hover:bg-blue-500 disabled:opacity-40 text-white text-[10px] font-black rounded-lg"
                        >Save</button>
                        <button
                          onClick={cancelEditCb}
                          className="px-2.5 py-1 bg-slate-700 hover:bg-slate-600 text-slate-300 text-[10px] font-bold rounded-lg"
                        >Cancel</button>
                      </div>
                    ) : (
                      <div className="flex flex-col items-end gap-1">
                        <span className={`text-sm font-black font-mono ${gainColor(lot.gain_cents)}`}>
                          {gainSign(lot.gain_cents)}{fmt(lot.gain_cents)}
                        </span>
                        {isManual && (
                          <div className="flex items-center gap-1">
                            <button
                              onClick={() => startEditCb(lot.txn_id!, lot.cost_basis_cents, lot.is_long_term)}
                              className="text-[9px] text-blue-500 hover:text-blue-400 font-bold"
                            >✎ Edit</button>
                            <span className="text-slate-700">·</span>
                            <button
                              onClick={() => clearCb(lot.txn_id!)}
                              className="text-[9px] text-slate-500 hover:text-red-400 font-bold"
                            >Clear</button>
                          </div>
                        )}
                      </div>
                    )}
                  </td>
                </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </div>

      {/* Sales with Unknown Cost Basis */}
      {!loading && report && report.unknown_basis_sales.length > 0 && (
        <div className="bg-slate-900 border border-amber-800/30 rounded-3xl overflow-hidden shadow-2xl">
          <div className="px-6 py-4 border-b border-amber-800/20 flex items-center justify-between bg-amber-950/20">
            <div className="flex items-center gap-2">
              <AlertCircle className="w-4 h-4 text-amber-400" />
              <span className="text-sm font-black text-amber-300">Sales with Unknown Cost Basis — {year}</span>
              <span className="text-[10px] text-amber-600 font-medium ml-1">
                {report.unknown_basis_sales.length} unresolved
              </span>
            </div>
            <span className="text-[10px] text-amber-700">Cost basis not in Plaid history · enter manually or check 1099-B</span>
          </div>
          <div className="overflow-x-auto">
            <table className="w-full text-left border-collapse">
              <thead className="bg-slate-950/40 text-[10px] text-slate-500 uppercase tracking-widest font-black border-b border-slate-800/60">
                <tr>
                  <th className="p-5 w-28">Sold</th>
                  <th className="p-5 w-28">Symbol</th>
                  <th className="p-5">Description</th>
                  <th className="p-5 text-right w-20">Qty</th>
                  <th className="p-5 text-right w-36">Proceeds</th>
                  <th className="p-5 text-right w-52">Cost Basis</th>
                  <th className="p-5 text-right w-36">Gain / Loss</th>
                </tr>
              </thead>
              <tbody className="text-xs divide-y divide-slate-800/50 text-slate-300">
                {report.unknown_basis_sales.map((s) => {
                  const isEditing = editingTxnId === s.txn_id;
                  return (
                    <tr key={s.txn_id} className="hover:bg-slate-800/20 transition-colors">
                      <td className="p-5 text-slate-500 font-mono text-[10px] font-bold">{fmtDate(s.close_date)}</td>
                      <td className="p-5">
                        {s.symbol ? (
                          <span className="px-2 py-1 bg-amber-500/10 text-amber-300 border border-amber-500/20 rounded-lg text-[10px] font-black tracking-wider">
                            {s.symbol}
                          </span>
                        ) : <span className="text-slate-600 italic text-[10px]">—</span>}
                      </td>
                      <td className="p-5 max-w-[180px] truncate text-slate-400 text-[11px]">{s.security_name ?? '—'}</td>
                      <td className="p-5 text-right font-mono text-[11px]">{s.quantity.toLocaleString('en-US', { maximumFractionDigits: 4 })}</td>
                      <td className="p-5 text-right font-mono text-[11px] text-slate-300">{fmt(s.proceeds_cents)}</td>

                      {/* Cost Basis — editable */}
                      <td className="p-5 text-right">
                        {isEditing ? (
                          <div className="flex flex-col items-end gap-1.5">
                            <div className="flex items-center gap-1.5">
                              <span className="text-slate-500 text-[10px]">$</span>
                              <input
                                type="number"
                                min="0"
                                step="0.01"
                                value={cbInput}
                                onChange={e => setCbInput(e.target.value)}
                                placeholder="0.00"
                                className="w-28 bg-slate-800 border border-slate-600 rounded-lg px-2 py-1 text-[11px] text-slate-200 font-mono text-right focus:outline-none focus:border-blue-500"
                                autoFocus
                              />
                            </div>
                            <select
                              value={isLtInput ? 'lt' : 'st'}
                              onChange={e => setIsLtInput(e.target.value === 'lt')}
                              className="bg-slate-800 border border-slate-600 rounded-lg px-2 py-1 text-[10px] text-slate-300 focus:outline-none focus:border-blue-500"
                            >
                              <option value="st">Short-term</option>
                              <option value="lt">Long-term</option>
                            </select>
                          </div>
                        ) : (
                          <span className="text-amber-500/70 italic text-[10px]">Unknown</span>
                        )}
                      </td>

                      {/* Gain / Loss + action buttons */}
                      <td className="p-5 text-right">
                        {isEditing ? (
                          <div className="flex items-center justify-end gap-1.5">
                            <button
                              onClick={() => saveCb(s.txn_id)}
                              disabled={savingCb || !cbInput}
                              className="px-2.5 py-1 bg-blue-600 hover:bg-blue-500 disabled:opacity-40 text-white text-[10px] font-black rounded-lg"
                            >Save</button>
                            <button
                              onClick={cancelEditCb}
                              className="px-2.5 py-1 bg-slate-700 hover:bg-slate-600 text-slate-300 text-[10px] font-bold rounded-lg"
                            >Cancel</button>
                          </div>
                        ) : (
                          <button
                            onClick={() => startEditCb(s.txn_id, null, null)}
                            className="px-3 py-1 bg-amber-600/20 hover:bg-amber-600/30 border border-amber-600/30 text-amber-400 text-[10px] font-black rounded-xl"
                          >Enter cost basis</button>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
          <div className="px-6 py-3 bg-amber-950/10 border-t border-amber-800/20 flex items-center gap-2 text-[10px] text-amber-700">
            <AlertCircle className="w-3 h-3 shrink-0" />
            Total proceeds: <span className="font-black text-amber-500 ml-1">{fmt(report.unknown_basis_sales.reduce((sum, s) => sum + s.proceeds_cents, 0))}</span>
            <span className="ml-2">· Once you enter a cost basis, the sale moves up to the Realized Gains table with a Manual tag and counts toward your YTD totals</span>
          </div>
        </div>
      )}

      {/* Unrealized Positions */}
      <div className="bg-slate-900 border border-slate-800 rounded-3xl overflow-hidden shadow-2xl">
        <div className="px-6 py-4 border-b border-slate-800/60 flex items-center justify-between">
          <div className="flex items-center gap-2">
            <TrendingDown className="w-4 h-4 text-slate-400" />
            <span className="text-sm font-black text-slate-200">Unrealized Positions</span>
            {report && <span className="text-[10px] text-slate-500 font-medium ml-1">{report.unrealized_positions.length} positions</span>}
          </div>
          <span className="text-[10px] text-slate-600">Current value as of last sync · FIFO remaining lots</span>
        </div>
        <div className="overflow-x-auto">
          <table className="w-full text-left border-collapse">
            <thead className="bg-slate-950/40 text-[10px] text-slate-500 uppercase tracking-widest font-black border-b border-slate-800/60">
              <tr>
                <SortableTh col="symbol" label="Symbol"       activeCol={unrealizedSortCol} dir={unrealizedSortDir} onSort={handleUnrealizedSort} className="w-28" />
                <th className="p-5">Description</th>
                <SortableTh col="since"  label="Since"        activeCol={unrealizedSortCol} dir={unrealizedSortDir} onSort={handleUnrealizedSort} className="w-28" />
                <th className="p-5 text-right w-20">Qty</th>
                <SortableTh col="cost"   label="Cost Basis"   activeCol={unrealizedSortCol} dir={unrealizedSortDir} onSort={handleUnrealizedSort} className="w-32" align="right" />
                <th className="p-5 text-right w-32">Mkt Value*</th>
                <SortableTh col="gain"   label="Unrealized G/L" activeCol={unrealizedSortCol} dir={unrealizedSortDir} onSort={handleUnrealizedSort} className="w-36" align="right" />
                <th className="p-5 text-center w-24">If Sold Today</th>
                <th className="p-5 text-center w-20">Harvest</th>
              </tr>
            </thead>
            <tbody className="text-xs divide-y divide-slate-800/50 text-slate-300">
              {loading ? (
                <tr><td colSpan={9} className="p-16 text-center text-slate-500 italic animate-pulse">Loading positions...</td></tr>
              ) : sortedUnrealized.length === 0 ? (
                <tr><td colSpan={9} className="p-16 text-center text-slate-500 italic">No tracked open positions. Sync investment data first.</td></tr>
              ) : sortedUnrealized.map((pos, i) => {
                const isHarvestCandidate = (pos.gain_cents ?? 0) < 0;
                return (
                  <tr key={i} className={`hover:bg-slate-800/20 transition-colors ${isHarvestCandidate ? 'bg-red-950/10' : ''}`}>
                    <td className="p-5">
                      <div className="flex items-center gap-1.5">
                        {pos.symbol ? (
                          <span className="px-2 py-1 bg-blue-500/10 text-blue-300 border border-blue-500/20 rounded-lg text-[10px] font-black tracking-wider">
                            {pos.symbol}
                          </span>
                        ) : <span className="text-slate-600 italic text-[10px]">—</span>}
                        {pos.has_unknown_basis && (
                          <span title="Holdings exceed tracked buy history — cost basis is partial" className="text-amber-500 cursor-help">
                            <AlertCircle className="w-3 h-3" />
                          </span>
                        )}
                      </div>
                    </td>
                    <td className="p-5 max-w-[200px] truncate text-slate-400 text-[11px]">{pos.security_name ?? '—'}</td>
                    <td className="p-5 font-mono text-[10px] text-slate-500">{fmtDate(pos.oldest_lot_date)}</td>
                    <td className="p-5 text-right font-mono text-[11px]">{pos.quantity.toLocaleString('en-US', { maximumFractionDigits: 4 })}</td>
                    <td className="p-5 text-right font-mono text-[11px] text-slate-400">{fmt(pos.cost_basis_cents)}</td>
                    <td className="p-5 text-right font-mono text-[11px] text-slate-400">
                      {pos.current_value_cents != null ? fmt(pos.current_value_cents) : <span className="text-slate-600 italic">—sync needed—</span>}
                    </td>
                    <td className="p-5 text-right">
                      {pos.gain_cents != null ? (
                        <span className={`text-sm font-black font-mono ${gainColor(pos.gain_cents)}`}>
                          {gainSign(pos.gain_cents)}{fmt(pos.gain_cents)}
                        </span>
                      ) : <span className="text-slate-600 italic text-[10px]">—</span>}
                    </td>
                    <td className="p-5 text-center">
                      <span className={`px-2 py-0.5 rounded border text-[10px] font-black uppercase tracking-wider ${pos.is_long_term_if_sold_today ? 'text-purple-400 bg-purple-500/10 border-purple-500/20' : 'text-amber-400 bg-amber-500/10 border-amber-500/20'}`}>
                        {pos.is_long_term_if_sold_today ? 'Long' : 'Short'}
                      </span>
                    </td>
                    <td className="p-5 text-center">
                      {isHarvestCandidate ? (
                        <div className="flex items-center justify-center">
                          <span title="Selling this position would realize a loss to offset gains" className="flex items-center gap-1 px-2 py-0.5 bg-emerald-500/10 border border-emerald-500/20 rounded-lg text-[10px] font-black text-emerald-400">
                            <Leaf className="w-3 h-3" /> Yes
                          </span>
                        </div>
                      ) : (
                        <span className="text-slate-700 text-[10px]">—</span>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </div>

      {/* Disclaimer */}
      <div className="flex items-start gap-2 px-4 py-3 bg-slate-900/50 border border-slate-800/40 rounded-2xl text-[10px] text-slate-600 leading-relaxed">
        <AlertCircle className="w-3 h-3 mt-0.5 shrink-0 text-slate-600" />
        <span>
          Cost basis calculated using <strong className="text-slate-500">FIFO</strong> (first-in, first-out) method based on Plaid transaction history (up to 730 days).
          Positions purchased before this window show <AlertCircle className="w-2.5 h-2.5 inline text-amber-500 mx-0.5" /> and may have partial cost basis.
          <strong className="text-slate-500"> Wash sale adjustments are not applied.</strong>{' '}
          Market values reflect your last sync. This is an <strong className="text-slate-500">estimation tool only</strong> — consult a tax professional and your brokerage 1099-B for filing.
        </span>
      </div>
    </div>
  );
};

interface TransactionsViewWrapperProps {
  mode: 'bank' | 'investments';
}

const TransactionsViewWrapper: React.FC<TransactionsViewWrapperProps> = ({ mode }) => {
  const [activeTab, setActiveTab] = useState<'investments' | 'capital_gains'>('investments');

  if (mode === 'bank') {
    return <TransactionsView />;
  }

  return (
    <div className="space-y-6">
      {/* Sub-tab toggle */}
      <div className="flex items-center gap-1 bg-slate-900 border border-slate-800 p-1.5 rounded-2xl w-fit">
        <button
          onClick={() => setActiveTab('investments')}
          className={`flex items-center gap-2 px-5 py-2 text-xs font-black rounded-xl transition-all ${
            activeTab === 'investments'
              ? 'bg-blue-600 text-white shadow-lg shadow-blue-900/20'
              : 'text-slate-500 hover:text-slate-300'
          }`}
        >
          <TrendingUp className="w-3.5 h-3.5" /> Investment Trades
        </button>
        <button
          onClick={() => setActiveTab('capital_gains')}
          className={`flex items-center gap-2 px-5 py-2 text-xs font-black rounded-xl transition-all ${
            activeTab === 'capital_gains'
              ? 'bg-blue-600 text-white shadow-lg shadow-blue-900/20'
              : 'text-slate-500 hover:text-slate-300'
          }`}
        >
          <BarChart3 className="w-3.5 h-3.5" /> Capital Gains
        </button>
      </div>

      {activeTab === 'investments' && <InvestmentTransactionsPanel />}
      {activeTab === 'capital_gains' && <CapitalGainsPanel />}
    </div>
  );
};

export default TransactionsViewWrapper;
