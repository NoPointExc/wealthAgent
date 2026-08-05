import React, { useState } from 'react';
import { BarChart3, Receipt, Shield, Plug, TrendingUp, Lock, X } from 'lucide-react';

interface SidebarProps {
  activeTab: string;
  onTabChange: (tabId: string) => void;
  /** Mobile drawer open state (ignored on md+ where the hover-rail is used). */
  mobileOpen: boolean;
  onMobileClose: () => void;
}

const tabs = [
  { id: 'portfolio', label: 'Portfolio View', icon: BarChart3 },
  { id: 'transactions', label: 'Bank Transactions', icon: Receipt },
  { id: 'investments', label: 'Investment Transactions', icon: TrendingUp },
  { id: 'advisory', label: 'MCP: Connect your AI', icon: Plug },
  { id: 'privacy', label: 'Privacy Lock', icon: Lock },
];

const Sidebar: React.FC<SidebarProps> = ({ activeTab, onTabChange, mobileOpen, onMobileClose }) => {
  // Desktop: collapsed to an icon rail by default; expands to full labels while
  // the mouse is over it, then folds away again — reclaims horizontal room for
  // the wide transaction tables. Hover doesn't exist on touch, so on mobile
  // (< md) this rail is hidden and a slide-in drawer is used instead.
  const [expanded, setExpanded] = useState(false);

  // Shared fade for any label text on the desktop rail — hidden while collapsed.
  const labelCls = `whitespace-nowrap transition-opacity duration-200 ${expanded ? 'opacity-100' : 'opacity-0'}`;

  // `fade` = desktop rail, where labels fade in only while expanded. In the
  // mobile drawer labels are always shown.
  const inner = (fade: boolean) => (
    <div>
      <div className="h-[89px] px-5 border-b border-slate-800 flex flex-col justify-center">
        <h1 className="text-xl font-bold tracking-wider text-blue-400 flex items-center gap-2">
          <Shield className="w-6 h-6 shrink-0" />
          <span className={fade ? labelCls : 'whitespace-nowrap'}>WealthAgent</span>
        </h1>
        <p className={`text-xs text-slate-400 mt-1 pl-8 ${fade ? labelCls : 'whitespace-nowrap'}`}>Texas Net Wealth</p>
      </div>
      <nav className="mt-6 space-y-1 px-3">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            onClick={() => { onTabChange(tab.id); onMobileClose(); }}
            title={tab.label}
            className={`w-full flex items-center gap-3 px-[13px] py-3 text-sm font-medium rounded-lg transition-all ${
              activeTab === tab.id
                ? 'active-tab'
                : 'text-slate-400 hover:bg-slate-800 hover:text-slate-200'
            }`}
          >
            <tab.icon className="w-4 h-4 shrink-0" />
            <span className={fade ? labelCls : 'whitespace-nowrap'}>{tab.label}</span>
          </button>
        ))}
      </nav>
    </div>
  );

  return (
    <>
      {/* Desktop hover-rail (md+ only) — unchanged behavior. */}
      <aside
        onMouseEnter={() => setExpanded(true)}
        onMouseLeave={() => setExpanded(false)}
        className={`hidden md:flex ${expanded ? 'w-64' : 'w-16'} bg-slate-900 border-r border-slate-800 flex-col justify-between shrink-0 overflow-hidden transition-[width] duration-200 ease-out`}
      >
        {inner(true)}
      </aside>

      {/* Mobile drawer (< md) — backdrop + slide-in panel, opened by the header
          hamburger. */}
      <div
        onClick={onMobileClose}
        aria-hidden
        className={`md:hidden fixed inset-0 z-40 bg-black/60 transition-opacity duration-200 ${
          mobileOpen ? 'opacity-100' : 'opacity-0 pointer-events-none'
        }`}
      />
      <aside
        className={`md:hidden fixed inset-y-0 left-0 z-50 w-64 max-w-[80vw] bg-slate-900 border-r border-slate-800 flex flex-col justify-between overflow-y-auto transition-transform duration-200 ease-out ${
          mobileOpen ? 'translate-x-0' : '-translate-x-full'
        }`}
        style={{ paddingTop: 'env(safe-area-inset-top)', paddingBottom: 'env(safe-area-inset-bottom)' }}
      >
        <button
          onClick={onMobileClose}
          aria-label="Close menu"
          className="absolute top-4 right-3 p-1.5 rounded-lg text-slate-500 hover:text-slate-200 hover:bg-slate-800 transition-colors"
        >
          <X className="w-5 h-5" />
        </button>
        {inner(false)}
      </aside>
    </>
  );
};

export default Sidebar;
