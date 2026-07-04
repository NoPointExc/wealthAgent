import React from 'react';
import { Line } from 'react-chartjs-2';
import {
  Chart as ChartJS,
  CategoryScale,
  LinearScale,
  PointElement,
  LineElement,
  Title,
  Tooltip,
  Legend,
  Filler,
} from 'chart.js';
import type { TimelinePoint } from '../types/models';
import { MONEY_MASK } from '../context/PrivacyContext';

ChartJS.register(
  CategoryScale,
  LinearScale,
  PointElement,
  LineElement,
  Title,
  Tooltip,
  Legend,
  Filler
);

interface PortfolioChartProps {
  accountHistories: Record<string, TimelinePoint[]>;
  accountNames: Record<string, string>;
  accountColors: Record<string, string>;
  hidden?: boolean;
}

const PortfolioChart: React.FC<PortfolioChartProps> = ({ accountHistories, accountNames, accountColors, hidden = false }) => {
  // Sort accounts so that those with HIGHER values are at the BOTTOM of the stack
  // In Chart.js, the first dataset in the array is the bottom-most layer.
  const accountIds = Object.keys(accountHistories).sort((a, b) => {
    const pointsA = accountHistories[a];
    const pointsB = accountHistories[b];
    const valA = pointsA[pointsA.length - 1]?.value || 0;
    const valB = pointsB[pointsB.length - 1]?.value || 0;
    
    // Descending order: highest balance first (bottom of stack)
    if (Math.abs(valB) !== Math.abs(valA)) return Math.abs(valB) - Math.abs(valA);
    return (accountNames[a] || a).localeCompare(accountNames[b] || b);
  });

  const firstAccountId = accountIds[0];
  const points = firstAccountId ? accountHistories[firstAccountId] : [];
  
  // Parse YYYY-MM-DD as local time (not UTC) to avoid day-shift in US timezones.
  const parseLocalDate = (s: string) => {
    const [y, m, d] = s.split('-').map(Number);
    return new Date(y, m - 1, d);
  };

  // Decide label format based on the number of points (rough proxy for daily vs monthly)
  const labels = points.map((p) => {
    const date = parseLocalDate(p.date);
    if (points.length > 31) {
      return date.toLocaleDateString('en-US', { month: 'short', year: '2-digit' });
    }
    return date.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
  });

  const datasets = accountIds.map((id) => {
    const accountPoints = accountHistories[id];
    const color = accountColors[id] || '#3b82f6';
    // Convert points to values, replacing leading zeros with null so Chart.js
    // draws a gap instead of a misleading flat line before the first real snapshot.
    const rawValues = accountPoints.map((p) => p.value / 100);
    const firstReal = rawValues.findIndex(v => v !== 0);
    const data: (number | null)[] = rawValues.map((v, i) =>
      firstReal >= 0 && i < firstReal ? null : v
    );
    return {
      label: accountNames[id] || id,
      data,
      borderColor: color,
      backgroundColor: color,
      fill: true,
      tension: 0,
      pointRadius: 3,
      pointBackgroundColor: color,
      pointBorderColor: '#0f172a',
      pointBorderWidth: 1,
      hoverRadius: 6,
      borderWidth: 1,
      spanGaps: false,
    };
  });

  const chartData = {
    labels,
    datasets,
  };

  const options = {
    responsive: true,
    maintainAspectRatio: false,
    plugins: {
      legend: {
        display: false,
      },
      tooltip: {
        mode: 'index' as const,
        intersect: false,
        callbacks: {
          title: (tooltipItems: any[]) => {
            if (!tooltipItems.length) return '';
            const idx = tooltipItems[0].dataIndex;
            const point = points[idx];
            if (!point) return '';
            const date = parseLocalDate(point.date);
            return date.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' });
          },
          label: (item: any) => {
            const label = item.dataset.label || '';
            if (hidden) return `${label}: ${MONEY_MASK}`;
            return `${label}: $${Number(item.raw).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
          },
          footer: (tooltipItems: any) => {
            if (hidden) return `Total: ${MONEY_MASK}`;
            let sum = 0;
            tooltipItems.forEach((item: any) => {
              sum += item.raw;
            });
            return `Total: $${sum.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
          }
        }
      },
    },
    scales: {
      x: {
        grid: {
          display: false,
        },
        ticks: {
          color: '#64748b',
          maxRotation: 0,
          autoSkip: true,
          maxTicksLimit: 12,
        },
      },
      y: {
        stacked: true,
        grid: {
          color: '#1e293b',
        },
        ticks: {
          color: '#64748b',
          callback: (value: any) => {
            if (hidden) return MONEY_MASK;
            const abs = Math.abs(value);
            const sign = value < 0 ? '-' : '';
            if (abs >= 1000) return `${sign}$${(abs / 1000).toFixed(0)}k`;
            return `${sign}$${abs}`;
          },
        },
      },
    },
  };

  return <Line data={chartData} options={options} />;
};

export default PortfolioChart;
