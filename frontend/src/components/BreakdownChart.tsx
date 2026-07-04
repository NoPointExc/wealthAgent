import React from 'react';
import { Doughnut } from 'react-chartjs-2';
import {
  Chart as ChartJS,
  ArcElement,
  Tooltip,
} from 'chart.js';
import type { Holding } from '../types/models';
import { getAccountColor } from '../utils/colors';
import { MONEY_MASK } from '../context/PrivacyContext';
import { isLocked } from './LockedValue';
import { Lock } from 'lucide-react';

ChartJS.register(ArcElement, Tooltip);

interface BreakdownChartProps {
  holdingsBreakdown: Record<string, Holding[]>;
  accountNames: Record<string, string>;
  accountColors: Record<string, string>;
  hidden?: boolean;
}

interface BreakdownItem {
  label: string;
  valueDollars: number;
  pct: number;
  color: string;
}

const BreakdownChart: React.FC<BreakdownChartProps> = ({ holdingsBreakdown, accountNames, accountColors, hidden = false }) => {
  const accountIds = Object.keys(holdingsBreakdown);
  const isSingleAccount = accountIds.length === 1;

  const formatFull = (dollars: number) =>
    hidden ? MONEY_MASK : Math.abs(dollars).toLocaleString('en-US', { style: 'currency', currency: 'USD', maximumFractionDigits: 0 });

  const formatCompact = (dollars: number) => {
    if (hidden) return MONEY_MASK;
    const abs = Math.abs(dollars);
    if (abs >= 1_000_000) return `$${(abs / 1_000_000).toFixed(1)}M`;
    if (abs >= 1_000) return `$${Math.round(abs / 1_000)}K`;
    return `$${Math.round(abs)}`;
  };

  let items: BreakdownItem[] = [];
  let totalDollars = 0;

  if (isSingleAccount) {
    const accId = accountIds[0];
    const holdings = (holdingsBreakdown[accId] || [])
      .sort((a, b) => Math.abs(b.asset_value) - Math.abs(a.asset_value));

    const isLiability = (holdings[0]?.asset_value || 0) < 0;
    totalDollars = holdings.reduce((sum, h) => sum + Math.abs(h.asset_value) / 100, 0);

    items = holdings.map((h, i) => ({
      label: h.symbol || h.name,
      valueDollars: Math.abs(h.asset_value) / 100,
      pct: totalDollars > 0 ? (Math.abs(h.asset_value) / 100 / totalDollars) * 100 : 0,
      color: getAccountColor(i, isLiability),
    }));
  } else {
    const sortedIds = [...accountIds].sort((a, b) =>
      (accountNames[a] || a).localeCompare(accountNames[b] || b)
    );

    sortedIds.forEach(id => {
      const sum = (holdingsBreakdown[id] || []).reduce((s, h) => s + Math.abs(h.asset_value), 0) / 100;
      totalDollars += sum;
    });

    items = sortedIds
      .map(id => {
        const valueDollars = (holdingsBreakdown[id] || []).reduce((s, h) => s + Math.abs(h.asset_value), 0) / 100;
        return {
          label: accountNames[id] || id,
          valueDollars,
          pct: totalDollars > 0 ? (valueDollars / totalDollars) * 100 : 0,
          color: accountColors[id] || '#3b82f6',
        };
      })
      .filter(item => item.valueDollars > 0);
  }

  if (items.length === 0) {
    return <div className="text-slate-500 italic text-xs text-center p-4">No data to display.</div>;
  }

  // Chart labels are plain canvas strings and can't host the clickable
  // Privacy Lock chip — show a notice instead of N identical [locked] slices.
  if (items.some(item => isLocked(item.label))) {
    return (
      <div className="flex flex-col items-center justify-center gap-2 p-6 text-center">
        <Lock className="w-5 h-5 text-amber-400" />
        <p className="text-xs text-slate-400 font-bold">Holdings breakdown is encrypted</p>
        <p className="text-[10px] text-slate-600">
          Unlock with your Privacy Key (header badge) to view it.
        </p>
      </div>
    );
  }

  const chartData = {
    labels: items.map(item => item.label),
    datasets: [{
      data: items.map(item => item.valueDollars),
      backgroundColor: items.map(item => item.color),
      borderColor: '#0f172a',
      borderWidth: 2,
    }],
  };

  const options = {
    responsive: true,
    maintainAspectRatio: false,
    cutout: '68%',
    plugins: {
      legend: { display: false },
      tooltip: {
        callbacks: {
          label: (context: any) => {
            const item = items[context.dataIndex];
            return ` ${formatFull(item.valueDollars)}  (${item.pct.toFixed(1)}%)`;
          },
        },
      },
    },
  };

  return (
    <div className="w-full flex flex-col items-center gap-4">
      {/* Ring chart with total in center */}
      <div className="relative flex-shrink-0" style={{ width: 180, height: 180 }}>
        <Doughnut data={chartData} options={options as any} />
        <div className="absolute inset-0 flex flex-col items-center justify-center pointer-events-none">
          <span className="text-[10px] font-semibold uppercase tracking-widest text-slate-500">Total</span>
          <span className="text-lg font-black text-white leading-tight">{formatCompact(totalDollars)}</span>
        </div>
      </div>

      {/* Scrollable breakdown list */}
      <div className="w-full overflow-y-auto max-h-[160px] space-y-px pr-0.5">
        {items.map((item, i) => (
          <div key={i} className="flex items-center justify-between gap-2 py-1 px-1 rounded hover:bg-slate-800/50 transition-colors">
            <div className="flex items-center gap-2 min-w-0">
              <span
                className="w-2.5 h-2.5 rounded-sm flex-shrink-0"
                style={{ backgroundColor: item.color }}
              />
              <span className="text-xs font-bold text-slate-200 truncate">{item.label}</span>
            </div>
            <div className="flex items-center gap-1.5 flex-shrink-0 text-xs">
              <span className="font-mono text-slate-300">{formatFull(item.valueDollars)}</span>
              <span className="text-slate-500 w-[46px] text-right">({item.pct.toFixed(1)}%)</span>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};

export default BreakdownChart;
