import React, { useState } from 'react';
import { BarChart3, Receipt, Shield, Plug, TrendingUp, Lock } from 'lucide-react';

interface SidebarProps {
  activeTab: string;
  onTabChange: (tabId: string) => void;
}

const Sidebar: React.FC<SidebarProps> = ({ activeTab, onTabChange }) => {
  // Collapsed to an icon rail by default; expands to full labels while the
  // mouse is over it, then folds away again — reclaims horizontal room for the
  // wide transaction tables.
  const [expanded, setExpanded] = useState(false);

  const tabs = [
    { id: 'portfolio', label: 'Portfolio View', icon: BarChart3 },
    { id: 'transactions', label: 'Bank Transactions', icon: Receipt },
    { id: 'investments', label: 'Investment Transactions', icon: TrendingUp },
    { id: 'advisory', label: 'Connect your AI', icon: Plug },
    { id: 'privacy', label: 'Privacy Lock', icon: Lock },
  ];

  // Shared fade for any label text — hidden while collapsed.
  const labelCls = `whitespace-nowrap transition-opacity duration-200 ${expanded ? 'opacity-100' : 'opacity-0'}`;

  return (
    <aside
      onMouseEnter={() => setExpanded(true)}
      onMouseLeave={() => setExpanded(false)}
      className={`${expanded ? 'w-64' : 'w-16'} bg-slate-900 border-r border-slate-800 flex flex-col justify-between shrink-0 overflow-hidden transition-[width] duration-200 ease-out`}
    >
      <div>
        <div className="h-[89px] px-5 border-b border-slate-800 flex flex-col justify-center">
          <h1 className="text-xl font-bold tracking-wider text-blue-400 flex items-center gap-2">
            <Shield className="w-6 h-6 shrink-0" />
            <span className={labelCls}>WealthAgent</span>
          </h1>
          <p className={`text-xs text-slate-400 mt-1 pl-8 ${labelCls}`}>Texas Net Wealth</p>
        </div>
        <nav className="mt-6 space-y-1 px-3">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              onClick={() => onTabChange(tab.id)}
              title={tab.label}
              className={`w-full flex items-center gap-3 px-[13px] py-3 text-sm font-medium rounded-lg transition-all ${
                activeTab === tab.id
                  ? 'active-tab'
                  : 'text-slate-400 hover:bg-slate-800 hover:text-slate-200'
              }`}
            >
              <tab.icon className="w-4 h-4 shrink-0" />
              <span className={labelCls}>{tab.label}</span>
            </button>
          ))}
        </nav>
      </div>
    </aside>
  );
};

export default Sidebar;
