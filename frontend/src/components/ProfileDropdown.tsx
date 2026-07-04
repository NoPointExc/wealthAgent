import React, { useState, useRef, useEffect } from 'react';
import { User, ShieldAlert, LogOut } from 'lucide-react';
import PlaidLinkButton from './PlaidLinkButton';
import { apiClient } from '../api/client';
import type { AuthUser } from '../App';

interface ProfileDropdownProps {
  user: AuthUser | null;
  onRefreshNeeded: () => void;
  onLogout: () => void;
}

const ProfileDropdown: React.FC<ProfileDropdownProps> = ({ user, onRefreshNeeded, onLogout }) => {
  const [isOpen, setIsOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
        setIsOpen(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  const handleReset = async () => {
    if (confirm("Are you sure you want to wipe all your linked accounts and data? You will need to re-link your accounts.")) {
      await apiClient.resetDatabase();
      onRefreshNeeded();
      setIsOpen(false);
    }
  };

  const handleLogout = () => {
    setIsOpen(false);
    onLogout();
  };

  return (
    <div className="relative" ref={dropdownRef}>
      <button
        onClick={() => setIsOpen(!isOpen)}
        className="w-10 h-10 rounded-full bg-slate-800 border border-slate-700 flex items-center justify-center hover:bg-slate-700 transition-colors"
      >
        <User className="w-5 h-5 text-slate-300" />
      </button>

      {isOpen && (
        <div className="absolute right-0 mt-2 w-64 bg-slate-900 border border-slate-700 rounded-xl shadow-2xl overflow-hidden z-50">
          <div className="p-4 border-b border-slate-800 bg-slate-950/50">
            <p className="text-sm font-semibold text-slate-200 truncate">{user?.name ?? 'User'}</p>
            <p className="text-xs text-slate-400 mt-0.5 truncate">{user?.email ?? ''}</p>
          </div>

          <div className="py-2">
            <PlaidLinkButton onSuccess={() => { setIsOpen(false); onRefreshNeeded(); }} />
          </div>

          <div className="border-t border-slate-800 py-2">
            <button
              onClick={handleReset}
              className="flex items-center gap-2 px-4 py-2 w-full hover:bg-rose-950/50 text-xs text-rose-400 transition-all text-left"
            >
              <ShieldAlert className="w-4 h-4" /> Reset My Data
            </button>
            <button
              onClick={handleLogout}
              className="flex items-center gap-2 px-4 py-2 w-full hover:bg-slate-800 text-xs text-slate-400 transition-all text-left"
            >
              <LogOut className="w-4 h-4" /> Sign Out
            </button>
          </div>
        </div>
      )}
    </div>
  );
};

export default ProfileDropdown;
