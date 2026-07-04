import React from 'react';
import { BarChart3, Receipt, FileText, Shield, Plug, TrendingUp, Lock } from 'lucide-react';

interface SidebarProps {
  activeTab: string;
  onTabChange: (tabId: string) => void;
}

const Sidebar: React.FC<SidebarProps> = ({ activeTab, onTabChange }) => {
  const tabs = [
    { id: 'portfolio', label: 'Portfolio View', icon: BarChart3 },
    { id: 'transactions', label: 'Bank Transactions', icon: Receipt },
    { id: 'investments', label: 'Investment Transactions', icon: TrendingUp },
    { id: 'tax', label: 'Tax Preparation', icon: FileText },
    { id: 'advisory', label: 'Connect your AI', icon: Plug },
    { id: 'privacy', label: 'Privacy Lock', icon: Lock },
  ];

  return (
    <aside className="w-64 bg-slate-900 border-r border-slate-800 flex flex-col justify-between shrink-0">
      <div>
        <div className="p-6 border-b border-slate-800">
          <h1 className="text-xl font-bold tracking-wider text-blue-400 flex items-center gap-2">
            <Shield className="w-6 h-6" /> WealthAgent
          </h1>
          <p className="text-xs text-slate-400 mt-1">Texas Net Wealth</p>
        </div>
        <nav className="mt-6 space-y-1 px-3">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              onClick={() => onTabChange(tab.id)}
              className={`w-full flex items-center gap-3 px-4 py-3 text-sm font-medium rounded-lg transition-all ${
                activeTab === tab.id
                  ? 'active-tab'
                  : 'text-slate-400 hover:bg-slate-800 hover:text-slate-200'
              }`}
            >
              <tab.icon className="w-4 h-4" />
              {tab.label}
            </button>
          ))}
        </nav>
      </div>
    </aside>
  );
};

export default Sidebar;
