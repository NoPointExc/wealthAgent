import React, { useEffect, useState, useMemo } from 'react';
import type { Account, AggregateData } from '../types/models';
import { apiClient } from '../api/client';
import AccountCard from '../components/AccountCard';
import AddInstitutionCard from '../components/AddInstitutionCard';
import PortfolioChart from '../components/PortfolioChart';
import BreakdownChart from '../components/BreakdownChart';
import { Calendar, Layers, TrendingUp, TrendingDown, LayoutDashboard } from 'lucide-react';
import { getAccountColor } from '../utils/colors';
import { usePrivacy, MONEY_MASK } from '../context/PrivacyContext';
import { useConfig } from '../context/ConfigContext';

const PortfolioView: React.FC = () => {
  const { demoMode } = useConfig();
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  
  // Date-range controls persist in localStorage so a chosen range (e.g. 1M)
  // survives a page refresh instead of snapping back to the default.
  const [rangeType, setRangeType] = useState(() => {
    const saved = localStorage.getItem('wealth_agent_range');
    return saved && ['1m', '3m', '6m', 'ytd', '1y', 'custom'].includes(saved) ? saved : 'ytd';
  });
  const [customDates, setCustomDates] = useState<{ start: string; end: string }>(() => {
    try {
      const saved = localStorage.getItem('wealth_agent_custom_dates');
      if (saved) return JSON.parse(saved);
    } catch { /* fall through to default */ }
    return { start: '2026-01-01', end: '2026-06-01' };
  });
  const [granularity, setGranularity] = useState<'daily' | 'monthly' | 'auto'>(() => {
    const saved = localStorage.getItem('wealth_agent_granularity');
    return saved === 'daily' || saved === 'monthly' || saved === 'auto' ? saved : 'auto';
  });

  useEffect(() => { localStorage.setItem('wealth_agent_range', rangeType); }, [rangeType]);
  useEffect(() => { localStorage.setItem('wealth_agent_granularity', granularity); }, [granularity]);
  useEffect(() => { localStorage.setItem('wealth_agent_custom_dates', JSON.stringify(customDates)); }, [customDates]);
  
  const [aggregateData, setAggregateData] = useState<AggregateData | null>(null);
  const [loading, setLoading] = useState(true);

  // Grouping
  const assetAccounts = useMemo(() => accounts.filter(a => a.account_type === 'asset'), [accounts]);
  const liabilityAccounts = useMemo(() => accounts.filter(a => a.account_type === 'liability'), [accounts]);

  const isValidDate = (dateStr: string) => {
    const reg = /^\d{4}-\d{2}-\d{2}$/;
    if (!reg.test(dateStr)) return false;
    const d = new Date(dateStr);
    if (!(d instanceof Date && !isNaN(d.getTime()))) return false;
    return d.getFullYear() >= 2000 && d <= new Date();
  };

  const datesAreValid = useMemo(() => {
    if (rangeType !== 'custom') return true;
    if (!isValidDate(customDates.start) || !isValidDate(customDates.end)) return false;
    return new Date(customDates.start) <= new Date(customDates.end);
  }, [rangeType, customDates]);

  useEffect(() => {
    loadAccounts();
  }, []);

  const activeTimeParams = useMemo(() => {
    if (!datesAreValid) return null;
    const end = new Date();
    let start = new Date();
    if (rangeType === '1m') start.setMonth(end.getMonth() - 1);
    else if (rangeType === '3m') start.setMonth(end.getMonth() - 3);
    else if (rangeType === '6m') start.setMonth(end.getMonth() - 6);
    else if (rangeType === 'ytd') start = new Date(end.getFullYear(), 0, 1);
    else if (rangeType === '1y') start.setFullYear(end.getFullYear() - 1);
    else if (rangeType === 'custom') {
      return { start_date: customDates.start, end_date: customDates.end, interval: granularity === 'auto' ? 'monthly' : granularity };
    }
    const startStr = start.toISOString().split('T')[0];
    const endStr = end.toISOString().split('T')[0];
    let interval = granularity as string;
    if (granularity === 'auto') {
      const diffMonths = (end.getFullYear() - start.getFullYear()) * 12 + (end.getMonth() - start.getMonth());
      interval = diffMonths >= 6 ? 'monthly' : 'daily';
    }
    return { start_date: startStr, end_date: endStr, interval };
  }, [rangeType, customDates, granularity, datesAreValid]);

  useEffect(() => {
    if (accounts.length > 0 && activeTimeParams) {
      loadAggregateData();
    }
  }, [accounts, activeTimeParams?.start_date, activeTimeParams?.end_date, activeTimeParams?.interval]);

  const loadAccounts = async () => {
    try {
      const data = await apiClient.getAccounts();
      const sortedData = data.sort((a, b) => a.name.localeCompare(b.name));
      setAccounts(sortedData);
      if (selectedIds.size === 0 && sortedData.length > 0) {
          setSelectedIds(new Set(sortedData.map(a => a.id)));
      }
    } catch (e) {
      console.error("Failed to load accounts", e);
    } finally {
      setLoading(false);
    }
  };

  const loadAggregateData = async () => {
    setLoading(true);
    try {
      const allIds = accounts.map(a => a.id);
      if (allIds.length === 0) return;
      const data = await apiClient.aggregatePortfolio(allIds, activeTimeParams!);
      setAggregateData(data);
    } catch (e) {
      console.error("Failed to load aggregate data", e);
    } finally {
      setLoading(false);
    }
  };

  const getDynamicTrend = (id: string) => {
    if (!aggregateData || !aggregateData.account_histories[id] || !activeTimeParams) return undefined;
    const points = aggregateData.account_histories[id];
    if (points.length < 2) return 0;
    const targetStartDate = new Date(activeTimeParams.start_date).getTime();
    // Baseline = first in-range point with real data; skip leading zeros
    // (backend reports 0 before an account's first snapshot).
    let firstValidPoint = points[points.length - 1];
    for (let i = 0; i < points.length; i++) {
        if (new Date(points[i].date).getTime() >= targetStartDate && points[i].value !== 0) { firstValidPoint = points[i]; break; }
    }
    const first = firstValidPoint.value;
    const last = points[points.length - 1].value;
    
    if (first === 0) return null;
    return ((last - first) / Math.abs(first)) * 100;
  };

  const toggleAccount = (id: string) => {
    const next = new Set(selectedIds);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setSelectedIds(next);
  };

  const selectGroup = (target: Account[]) => {
    const next = new Set(selectedIds);
    target.forEach(a => next.add(a.id));
    setSelectedIds(next);
  };

  const deselectGroup = (target: Account[]) => {
    const next = new Set(selectedIds);
    target.forEach(a => next.delete(a.id));
    setSelectedIds(next);
  };

  const { hidden } = usePrivacy();

  const formatCurrency = (cents: number) => {
    if (hidden) return MONEY_MASK;
    return (Math.abs(cents) / 100).toLocaleString('en-US', { style: 'currency', currency: 'USD' });
  };

  const accountNames = useMemo(() => {
    return accounts.reduce((map, acc) => {
      map[acc.id] = acc.custom_name || acc.name;
      return map;
    }, {} as Record<string, string>);
  }, [accounts]);

  const accountColors = useMemo(() => {
    const colors: Record<string, string> = {};
    assetAccounts.forEach((acc, i) => { colors[acc.id] = getAccountColor(i, false); });
    liabilityAccounts.forEach((acc, i) => { colors[acc.id] = getAccountColor(i, true); });
    return colors;
  }, [assetAccounts, liabilityAccounts]);

  const getSectionData = (targetAccounts: Account[]) => {
    const targetIds = targetAccounts.map(a => a.id);
    const selectedInTarget = targetAccounts.filter(a => selectedIds.has(a.id));
    const currentTotal = selectedInTarget.reduce((sum, a) => sum + a.balance, 0);
    const filteredHistories = aggregateData ? Object.fromEntries(
        Object.entries(aggregateData.account_histories).filter(([id]) => targetIds.includes(id) && selectedIds.has(id))
    ) : {};
    const filteredHoldings = aggregateData ? Object.fromEntries(
        Object.entries(aggregateData.holdings_breakdown).filter(([id]) => targetIds.includes(id) && selectedIds.has(id))
    ) : {};

    // Calculate aggregate trend
    let groupTrend: number | null | undefined = undefined;
    if (aggregateData && activeTimeParams && Object.keys(filteredHistories).length > 0) {
        let sumFirst = 0;
        let sumLast = 0;
        const targetStartDate = new Date(activeTimeParams.start_date).getTime();
        for (const id in filteredHistories) {
            const points = filteredHistories[id];
            if (points.length < 2) continue;
            // Baseline = first in-range point with real data; skip leading zeros
            // (backend reports 0 before an account's first snapshot).
            let firstValidPoint = points[points.length - 1];
            for (let i = 0; i < points.length; i++) {
                if (new Date(points[i].date).getTime() >= targetStartDate && points[i].value !== 0) { firstValidPoint = points[i]; break; }
            }
            sumFirst += firstValidPoint.value;
            sumLast += points[points.length - 1].value;
        }
        if (sumFirst !== 0) groupTrend = ((sumLast - sumFirst) / Math.abs(sumFirst)) * 100;
        else groupTrend = null;
    }
    return { currentTotal, filteredHistories, filteredHoldings, groupTrend };
  };

  const assetData = getSectionData(assetAccounts);
  const liabilityData = getSectionData(liabilityAccounts);
  const netWorth = assetAccounts.filter(a => selectedIds.has(a.id)).reduce((sum, a) => sum + a.balance, 0) + 
                   liabilityAccounts.filter(a => selectedIds.has(a.id)).reduce((sum, a) => sum + a.balance, 0);

  const globalTrend = useMemo(() => {
    if (!aggregateData || !activeTimeParams || selectedIds.size === 0) return undefined;
    let sumFirst = 0;
    let sumLast = 0;
    const targetStartDate = new Date(activeTimeParams.start_date).getTime();
    for (const id of Array.from(selectedIds)) {
        const points = aggregateData.account_histories[id];
        if (!points || points.length < 2) continue;
        // Baseline = first in-range point that actually has data. Skip leading
        // zeros: the backend reports 0 for any date before an account's first
        // snapshot, so anchoring to the raw range start (e.g. Jan 1 for YTD)
        // would make the baseline $0 and force the badge to N/A.
        let firstValidPoint = points[points.length - 1];
        for (let i = 0; i < points.length; i++) {
            if (new Date(points[i].date).getTime() >= targetStartDate && points[i].value !== 0) { firstValidPoint = points[i]; break; }
        }
        sumFirst += firstValidPoint.value;
        sumLast += points[points.length - 1].value;
    }
    if (sumFirst === 0) return null;
    return ((sumLast - sumFirst) / Math.abs(sumFirst)) * 100;
  }, [aggregateData, activeTimeParams, selectedIds]);

  const renderTrendBadge = (trend?: number | null) => {
    if (trend === undefined) return null;
    if (trend === null) {
        return (
            <span className="ml-3 text-[10px] px-2 py-0.5 rounded bg-slate-800 text-slate-500 font-bold uppercase tracking-tighter">
                N/A
            </span>
        );
    }
    const isPositive = trend >= 0;
    return (
        <span className={`ml-3 text-sm px-2.5 py-1 rounded-lg font-bold tracking-wider ${isPositive ? 'bg-emerald-500/10 text-emerald-400' : 'bg-rose-500/10 text-rose-400'}`}>
            {isPositive ? '↑' : '↓'} {Math.abs(trend).toFixed(1)}%
        </span>
    );
  };

  return (
    <div className="space-y-12 pb-20">
      <div className="bg-slate-900 border border-slate-800 p-8 rounded-3xl backdrop-blur-xl shadow-2xl flex flex-col md:flex-row justify-between items-center gap-8">
        <div className="flex items-center gap-6">
            <div className="w-16 h-16 bg-blue-600/10 rounded-2xl flex items-center justify-center border border-blue-500/20">
                <LayoutDashboard className="w-8 h-8 text-blue-400" />
            </div>
            <div>
                <h3 className="text-xs font-bold text-slate-500 uppercase tracking-[0.2em] mb-1">Global Net Worth</h3>
                <div className="flex items-center">
                    <h2 className={`text-5xl font-black tracking-tighter ${netWorth >= 0 ? 'text-white' : 'text-rose-500'}`}>
                        {netWorth < 0 ? '-' : ''}{formatCurrency(netWorth)}
                    </h2>
                    {!loading && renderTrendBadge(globalTrend)}
                </div>
            </div>
        </div>

        <div className="flex flex-col items-end gap-4">
            <div className="flex flex-col items-end gap-2">
                <div className="flex items-center gap-3 bg-slate-950/50 p-1.5 rounded-2xl border border-slate-800">
                    <div className="flex items-center gap-1 border-r border-slate-800 pr-3 mr-1">
                        <Calendar className="w-3.5 h-3.5 text-slate-500 ml-2" />
                        {['1m', '3m', '6m', 'ytd', '1y', 'custom'].map((range) => (
                            <button key={range} onClick={() => setRangeType(range)} className={`px-3 py-1.5 text-[10px] font-black rounded-xl transition-all ${rangeType === range ? 'bg-blue-600 text-white shadow-lg shadow-blue-900/20' : 'text-slate-500 hover:text-slate-300'}`}>{range.toUpperCase()}</button>
                        ))}
                    </div>
                    <div className="flex items-center gap-1">
                        <Layers className="w-3.5 h-3.5 text-slate-500 ml-1" />
                        {['auto', 'daily', 'monthly'].map((g) => (
                            <button key={g} onClick={() => setGranularity(g as any)} className={`px-3 py-1.5 text-[10px] font-black rounded-xl transition-all ${granularity === g ? 'bg-slate-700 text-white' : 'text-slate-500 hover:text-slate-400'}`}>{g.toUpperCase()}</button>
                        ))}
                    </div>
                </div>
                {rangeType === 'custom' && (
                    <div className="flex flex-col items-end gap-1">
                        <div className={`flex gap-2 items-center bg-slate-950 border ${datesAreValid ? 'border-slate-800' : 'border-rose-500'} p-2 rounded-xl`}>
                            <input type="date" value={customDates.start} onChange={(e) => setCustomDates(prev => ({...prev, start: e.target.value}))} className="bg-transparent text-[10px] text-slate-300 outline-none" />
                            <span className="text-slate-600 text-[10px]">to</span>
                            <input type="date" value={customDates.end} onChange={(e) => setCustomDates(prev => ({...prev, end: e.target.value}))} className="bg-transparent text-[10px] text-slate-300 outline-none" />
                        </div>
                    </div>
                )}
            </div>
        </div>
      </div>

      <section className="space-y-6">
          <div className="flex items-center justify-between border-b border-slate-800 pb-4">
              <div className="flex items-center gap-3">
                  <div className="p-2 bg-emerald-500/10 rounded-lg"><TrendingUp className="w-5 h-5 text-emerald-400" /></div>
                  <h3 className="text-xl font-bold text-slate-100">Assets Portfolio</h3>
                  <div className="flex items-center">
                      <span className="ml-2 text-2xl font-mono text-emerald-500">{formatCurrency(assetData.currentTotal)}</span>
                      {!loading && renderTrendBadge(assetData.groupTrend)}
                  </div>
              </div>
              <div className="flex gap-4">
                  <button onClick={() => selectGroup(assetAccounts)} className="text-xs font-bold text-blue-400 hover:underline">Select All</button>
                  <button onClick={() => deselectGroup(assetAccounts)} className="text-xs font-bold text-slate-500 hover:underline">Clear</button>
              </div>
          </div>
          <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-5 gap-3">
            {assetAccounts.map((acc) => (
              <AccountCard key={acc.id} account={acc} isSelected={selectedIds.has(acc.id)} onToggle={toggleAccount} color={accountColors[acc.id]} dynamicTrend={getDynamicTrend(acc.id)} onAccountUpdated={loadAccounts} />
            ))}
            {!loading && !demoMode && <AddInstitutionCard onLinked={loadAccounts} />}
          </div>
          <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
              <div className="lg:col-span-2 bg-slate-900 border border-slate-800 p-8 rounded-3xl min-h-[450px]">
                  <h4 className="text-xs font-black text-slate-500 uppercase tracking-widest mb-6">Asset History</h4>
                  <div className="h-full max-h-[350px]">
                    <PortfolioChart accountHistories={assetData.filteredHistories} accountNames={accountNames} accountColors={accountColors} hidden={hidden} />
                  </div>
              </div>
              <div className="bg-slate-900 border border-slate-800 p-8 rounded-3xl flex flex-col items-center">
                  <h4 className="text-xs font-black text-slate-500 uppercase tracking-widest mb-6 self-start">Asset Breakdown</h4>
                  <div className="w-full">
                    <BreakdownChart holdingsBreakdown={assetData.filteredHoldings} accountNames={accountNames} accountColors={accountColors} hidden={hidden} />
                  </div>
              </div>
          </div>
      </section>

      <section className="space-y-6 pt-10 border-t border-slate-800/50">
          <div className="flex items-center justify-between border-b border-slate-800 pb-4">
              <div className="flex items-center gap-3">
                  <div className="p-2 bg-rose-500/10 rounded-lg"><TrendingDown className="w-5 h-5 text-rose-400" /></div>
                  <h3 className="text-xl font-bold text-slate-100">Liabilities & Debt</h3>
                  <div className="flex items-center">
                      <span className="ml-2 text-2xl font-mono text-rose-500">-{formatCurrency(liabilityData.currentTotal)}</span>
                      {!loading && renderTrendBadge(liabilityData.groupTrend)}
                  </div>
              </div>
              <div className="flex gap-4">
                  <button onClick={() => selectGroup(liabilityAccounts)} className="text-xs font-bold text-blue-400 hover:underline">Select All</button>
                  <button onClick={() => deselectGroup(liabilityAccounts)} className="text-xs font-bold text-slate-500 hover:underline">Clear</button>
              </div>
          </div>
          <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-5 gap-3">
            {liabilityAccounts.map((acc) => (
              <AccountCard key={acc.id} account={acc} isSelected={selectedIds.has(acc.id)} onToggle={toggleAccount} color={accountColors[acc.id]} dynamicTrend={getDynamicTrend(acc.id)} onAccountUpdated={loadAccounts} />
            ))}
            {!loading && !demoMode && <AddInstitutionCard onLinked={loadAccounts} />}
          </div>
          <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
              <div className="lg:col-span-2 bg-slate-900 border border-slate-800 p-8 rounded-3xl min-h-[450px]">
                  <h4 className="text-xs font-black text-slate-500 uppercase tracking-widest mb-6">Liability History</h4>
                  <div className="h-full max-h-[350px]">
                    <PortfolioChart accountHistories={liabilityData.filteredHistories} accountNames={accountNames} accountColors={accountColors} hidden={hidden} />
                  </div>
              </div>
              <div className="bg-slate-900 border border-slate-800 p-8 rounded-3xl flex flex-col items-center">
                  <h4 className="text-xs font-black text-slate-500 uppercase tracking-widest mb-6 self-start">Liability Breakdown</h4>
                  <div className="w-full">
                    <BreakdownChart holdingsBreakdown={liabilityData.filteredHoldings} accountNames={accountNames} accountColors={accountColors} hidden={hidden} />
                  </div>
              </div>
          </div>
      </section>
    </div>
  );
};

export default PortfolioView;
