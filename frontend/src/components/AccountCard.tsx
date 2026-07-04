import React, { useState, useRef, useEffect } from 'react';
import type { Account } from '../types/models';
import { hexToRgba } from '../utils/colors';
import { Edit2, Check, X, MoreVertical, ArrowLeftRight } from 'lucide-react';
import { apiClient } from '../api/client';
import { usePrivacy, MONEY_MASK } from '../context/PrivacyContext';
import LockedValue, { isLocked } from './LockedValue';

interface AccountCardProps {
  account: Account;
  isSelected: boolean;
  onToggle: (id: string) => void;
  color: string;
  dynamicTrend?: number | null;
  onAccountUpdated: () => void;
}

const AccountCard: React.FC<AccountCardProps> = ({ account, isSelected, onToggle, color, dynamicTrend, onAccountUpdated }) => {
  const isLiability = account.account_type === 'liability';
  const trend = dynamicTrend !== undefined ? dynamicTrend : account.trend_pct;
  
  const [isEditing, setIsEditing] = useState(false);
  const [showMenu, setShowMenu] = useState(false);
  const [editValue, setEditValue] = useState(account.custom_name || account.name);
  const displayName = account.custom_name || account.name;
  const menuRef = useRef<HTMLDivElement>(null);
  const { hidden } = usePrivacy();

  const formatCurrency = (cents: number) => {
    if (hidden) return MONEY_MASK;
    return (Math.abs(cents) / 100).toLocaleString('en-US', { style: 'currency', currency: 'USD' });
  };

  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
        setShowMenu(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  const handleSave = async (e: React.MouseEvent) => {
    e.stopPropagation();
    try {
        const newName = editValue.trim() === '' || editValue === account.name ? null : editValue;
        await apiClient.updateAccount(account.id, newName);
        setIsEditing(false);
        onAccountUpdated();
    } catch (err) {
        console.error("Failed to update account name", err);
    }
  };

  const handleCancel = (e: React.MouseEvent) => {
    e.stopPropagation();
    setEditValue(displayName);
    setIsEditing(false);
  };

  const handleToggleLiability = async (e: React.MouseEvent) => {
    e.stopPropagation();
    try {
        const targetType = isLiability ? 'asset' : 'liability';
        await apiClient.updateAccount(account.id, account.custom_name || null, targetType);
        setShowMenu(false);
        onAccountUpdated();
    } catch (err) {
        console.error("Failed to toggle liability state", err);
    }
  };

  return (
    <div
      onClick={() => {
        if (!isEditing) onToggle(account.id);
      }}
      style={{
        borderColor: isSelected && !isEditing ? color : 'transparent',
        backgroundColor: isSelected ? hexToRgba(color, 0.1) : 'rgb(15, 23, 42)',
        boxShadow: isSelected && !isEditing ? `0 0 0 1px ${color}` : 'none',
      }}
      className={`border bg-slate-900 border-slate-800 p-3 rounded-xl ${!isEditing ? 'cursor-pointer hover:border-slate-600' : ''} transition-all group h-full flex flex-col justify-between relative`}
    >
      <div className="flex justify-between items-start mb-1">
        <div className="flex items-center gap-2">
           <div 
             className={`w-2.5 h-2.5 rounded-full border-[1.5px]`} 
             style={{ 
               borderColor: color, 
               backgroundColor: isSelected ? color : 'transparent' 
             }}
           />
           <span className="text-[9px] text-slate-500 uppercase tracking-widest font-black">
             {isLiability ? 'Liability' : 'Asset'}
           </span>
        </div>
        <div className="flex items-center gap-1.5" ref={menuRef}>
          {trend !== null ? (
            <span
              className={`text-[9px] ${
                trend >= 0 ? 'text-emerald-400' : 'text-rose-400'
              } font-bold mr-1`}
            >
              {trend >= 0 ? '↑' : '↓'} {Math.abs(trend).toFixed(1)}%
            </span>
          ) : (
            <span className="text-[9px] text-slate-600 font-bold mr-1 uppercase tracking-tighter">
              N/A
            </span>
          )}
          
          <button 
            onClick={(e) => { e.stopPropagation(); setShowMenu(!showMenu); }}
            className="p-1 hover:bg-slate-800 rounded text-slate-500 hover:text-slate-300 transition-all opacity-0 group-hover:opacity-100"
          >
            <MoreVertical className="w-3.5 h-3.5" />
          </button>

          {showMenu && (
            <div className="absolute right-3 top-9 w-44 bg-slate-950 border border-slate-700 rounded-lg shadow-2xl py-1 z-50">
              {/* Renaming while locked would save the literal "[locked]" sentinel. */}
              {!isLocked(displayName) && (
                <button
                  onClick={(e) => { e.stopPropagation(); setIsEditing(true); setShowMenu(false); }}
                  className="flex items-center gap-2 px-3 py-2 w-full hover:bg-slate-800 text-slate-300 text-left text-xs"
                >
                  <Edit2 className="w-3 h-3 text-slate-500" /> Edit Nickname
                </button>
              )}
              <button 
                onClick={handleToggleLiability}
                className="flex items-center gap-2 px-3 py-2 w-full hover:bg-slate-800 text-slate-300 text-left text-xs"
              >
                <ArrowLeftRight className="w-3 h-3 text-slate-500" />
                {isLiability ? 'Move to Assets' : 'Move to Liabilities'}
              </button>
            </div>
          )}
        </div>
      </div>
      <div>
        {isEditing ? (
            <div className="flex items-center gap-1 mt-1 mb-1">
                <input 
                    type="text" 
                    value={editValue}
                    onChange={(e) => setEditValue(e.target.value)}
                    className="w-full bg-slate-950 border border-slate-700 rounded px-2 py-1 text-xs text-white outline-none focus:border-blue-500"
                    autoFocus
                    onClick={(e) => e.stopPropagation()}
                />
                <button onClick={handleSave} className="p-1 text-emerald-400 hover:bg-emerald-400/20 rounded">
                    <Check className="w-3 h-3" />
                </button>
                <button onClick={handleCancel} className="p-1 text-rose-400 hover:bg-rose-400/20 rounded">
                    <X className="w-3 h-3" />
                </button>
            </div>
        ) : (
            <div className="flex items-center justify-between">
                <h3 className="text-sm font-bold text-slate-200 group-hover:text-white transition-colors truncate leading-tight pr-2" title={isLocked(displayName) ? undefined : displayName}>
                    <LockedValue value={displayName} />
                </h3>
            </div>
        )}
        <p className={`text-base font-mono mt-0.5 ${isLiability ? 'text-rose-400' : 'text-slate-400'}`}>
          {isLiability ? '-' : ''}{formatCurrency(account.balance)}
        </p>
      </div>
    </div>
  );
};

export default AccountCard;
