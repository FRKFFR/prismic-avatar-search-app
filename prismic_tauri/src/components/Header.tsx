import React from 'react';
import { Search, Database, User, LogIn, LogOut, RefreshCw, X, Sparkles, Settings, SlidersHorizontal, Star } from 'lucide-react';
import type { DatabaseStatus, UserProfile } from '../types';

interface HeaderProps {
  searchQuery: string;
  onSearchChange: (query: string) => void;
  onSearchSubmit: () => void;
  dbStatus: DatabaseStatus | null;
  isSyncing: boolean;
  onOpenDbModal: () => void;
  currentUser: UserProfile | null;
  onOpenLoginModal: () => void;
  onLogout: () => void;
  onOpenSettingsModal: () => void;
  isDashboardOpen?: boolean;
  onToggleDashboard?: () => void;
  activeFilterCount?: number;
  favoritesCount?: number;
  isFavoritesOnly?: boolean;
  onToggleFavorites?: () => void;
}

export const Header: React.FC<HeaderProps> = ({
  searchQuery,
  onSearchChange,
  onSearchSubmit,
  dbStatus,
  isSyncing,
  onOpenDbModal,
  currentUser,
  onOpenLoginModal,
  onLogout,
  onOpenSettingsModal,
  isDashboardOpen,
  onToggleDashboard,
  activeFilterCount = 0,
  favoritesCount = 0,
  isFavoritesOnly = false,
  onToggleFavorites,
}) => {
  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      onSearchSubmit();
    }
  };

  return (
    <header className="sticky top-0 z-30 bg-[#0d121f]/90 backdrop-blur-md border-b border-gray-800/80 px-4 lg:px-6 py-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        {/* Brand Logo & Title */}
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-xl bg-gradient-to-tr from-cyan-500 to-purple-600 flex items-center justify-center shadow-lg shadow-cyan-500/20 ring-1 ring-white/10">
            <Sparkles className="w-5 h-5 text-white animate-pulse" />
          </div>
          <div>
            <h1 className="text-base font-bold bg-gradient-to-r from-cyan-400 via-teal-300 to-purple-400 bg-clip-text text-transparent leading-tight tracking-wide">
              Prismic Avatar Search
            </h1>
            <span className="text-[11px] text-gray-400 font-medium tracking-wider uppercase">
              VRChat Avatar Browser
            </span>
          </div>
        </div>

        {/* Global Search Bar */}
        <div className="flex-1 max-w-xl min-w-[280px]">
          <div className="relative flex items-center">
            <Search className="absolute left-3.5 w-4 h-4 text-gray-400 pointer-events-none" />
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => onSearchChange(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Search avatars by name, author, tags, or description..."
              className="w-full pl-10 pr-10 py-2 rounded-xl bg-gray-900/90 border border-gray-700/80 text-sm text-gray-100 placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-cyan-500/50 focus:border-cyan-500 transition-all shadow-inner"
            />
            {searchQuery && (
              <button
                onClick={() => onSearchChange('')}
                className="absolute right-3 p-0.5 rounded-md hover:bg-gray-800 text-gray-400 hover:text-gray-200 transition-colors"
                title="Clear search"
              >
                <X className="w-4 h-4" />
              </button>
            )}
          </div>
        </div>

        {/* Favorites Quick Toggle Button */}
        {onToggleFavorites && (
          <button
            onClick={onToggleFavorites}
            className={`flex items-center gap-1.5 px-3 py-2 rounded-xl text-xs font-bold transition-all border ${
              isFavoritesOnly
                ? 'bg-amber-950/80 text-amber-300 border-amber-500/80 shadow-md shadow-amber-500/20'
                : favoritesCount > 0
                ? 'bg-gray-900/80 text-amber-300/80 hover:text-amber-300 border-gray-800 hover:border-amber-800/50'
                : 'bg-gray-900/80 text-gray-400 hover:text-gray-200 border-gray-800/80 hover:bg-gray-800'
            }`}
            title={isFavoritesOnly ? 'Show All Avatars' : 'Show Favorites Only'}
          >
            <Star
              className={`w-3.5 h-3.5 ${
                isFavoritesOnly || favoritesCount > 0 ? 'fill-amber-400 text-amber-400' : 'text-gray-400'
              }`}
            />
            <span>Favorites</span>
            {favoritesCount > 0 && (
              <span className="px-1.5 py-0.2 rounded-full text-[10px] font-extrabold bg-amber-500 text-black">
                {favoritesCount}
              </span>
            )}
          </button>
        )}

        {/* Search Dashboard Toggle Button */}
        {onToggleDashboard && (
          <button
            onClick={onToggleDashboard}
            className={`flex items-center gap-2 px-3.5 py-2 rounded-xl text-xs font-bold transition-all border ${
              isDashboardOpen
                ? 'bg-cyan-500/20 text-cyan-300 border-cyan-500/50 shadow-md shadow-cyan-500/20'
                : activeFilterCount > 0
                ? 'bg-cyan-950/50 text-cyan-400 border-cyan-800 hover:bg-cyan-900/40'
                : 'bg-gray-900/80 text-gray-400 hover:text-gray-200 border-gray-800/80 hover:bg-gray-800'
            }`}
            title="Toggle Search Dashboard"
          >
            <SlidersHorizontal className="w-3.5 h-3.5 text-cyan-400" />
            <span>Filters</span>
            {activeFilterCount > 0 && (
              <span className="px-1.5 py-0.2 rounded-full text-[10px] font-extrabold bg-cyan-500 text-black">
                {activeFilterCount}
              </span>
            )}
          </button>
        )}

        {/* Action Controls: Database Status & Auth */}
        <div className="flex items-center gap-2">
          {/* Database Status Button */}
          <button
            onClick={onOpenDbModal}
            className="flex items-center gap-2 px-3 py-2 rounded-xl bg-gray-900/80 hover:bg-gray-800/90 border border-gray-800/80 hover:border-gray-700 text-xs font-medium text-gray-300 transition-all"
            title="Database Status and Sync"
          >
            {isSyncing ? (
              <RefreshCw className="w-4 h-4 text-cyan-400 animate-spin" />
            ) : (
              <Database className="w-4 h-4 text-cyan-400" />
            )}
            <span className="hidden sm:inline">
              {isSyncing
                ? 'Syncing DB...'
                : dbStatus?.ready
                ? `${dbStatus.avatar_count.toLocaleString()} Avatars`
                : 'Setup Database'}
            </span>
          </button>

          {/* Settings Button */}
          <button
            onClick={onOpenSettingsModal}
            className="p-2 rounded-xl bg-gray-900/80 hover:bg-gray-800/90 border border-gray-800/80 hover:border-gray-700 text-gray-400 hover:text-cyan-300 transition-all"
            title="Settings & Cache"
          >
            <Settings className="w-4 h-4" />
          </button>

          {/* User Account / Login Button */}
          {currentUser ? (
            <div className="flex items-center gap-2 pl-2 border-l border-gray-800">
              <div className="flex items-center gap-2 px-3 py-1.5 rounded-xl bg-cyan-950/40 border border-cyan-800/50 text-xs text-cyan-200">
                <User className="w-3.5 h-3.5 text-cyan-400" />
                <span className="font-semibold max-w-[120px] truncate">
                  {currentUser.display_name || currentUser.username}
                </span>
              </div>
              <button
                onClick={onLogout}
                className="p-2 rounded-xl bg-gray-900/80 hover:bg-rose-950/40 hover:text-rose-300 border border-gray-800 hover:border-rose-800/50 text-gray-400 transition-all"
                title="Log Out"
              >
                <LogOut className="w-4 h-4" />
              </button>
            </div>
          ) : (
            <button
              onClick={onOpenLoginModal}
              className="flex items-center gap-2 px-3.5 py-2 rounded-xl bg-gradient-to-r from-cyan-600 to-teal-600 hover:from-cyan-500 hover:to-teal-500 text-white text-xs font-semibold shadow-md shadow-cyan-600/20 transition-all"
            >
              <LogIn className="w-4 h-4" />
              <span>Log In</span>
            </button>
          )}
        </div>
      </div>
    </header>
  );
};
