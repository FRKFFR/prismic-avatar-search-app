import React, { useState, useEffect, useCallback, useRef } from 'react';
import { Download, X } from 'lucide-react';
import { api } from './api';
import type {
  AvatarEntry,
  DatabaseStatus,
  SearchFilter,
  SearchResult,
  SyncProgress,
  UpdateInfo,
  UserProfile,
} from './types';
import { Header } from './components/Header';
import { VrcSearchDashboard } from './components/VrcSearchDashboard';
import { AvatarGrid } from './components/AvatarGrid';
import { AvatarModal } from './components/AvatarModal';
import { LoginModal } from './components/LoginModal';
import { DatabaseModal } from './components/DatabaseModal';
import { SettingsModal } from './components/SettingsModal';
import { ToastProvider, useToast } from './components/Toast';

const DEFAULT_FILTER: SearchFilter = {
  query: '',
  name: '',
  name_search_method: 'match_all_words',
  name_include_desc_tags: true,
  author: '',
  author_search_method: 'match_all_words',
  description: undefined,
  platform_pc: false,
  platform_quest: false,
  platform_ios: false,
  include_impostors: false,
  include_marketplace: true,
  pc_rating: undefined,
  quest_rating: undefined,
  ios_rating: undefined,
  selected_styles: [],
  allowed_warnings: 31,
  marketplace_only: false,
  favorites_only: false,
  page: 0,
  page_size: 24,
};

function MainApp() {
  const { showToast } = useToast();

  // Search & Filter State
  const [searchQuery, setSearchQuery] = useState('');
  const [filter, setFilter] = useState<SearchFilter>(DEFAULT_FILTER);
  const [searchResult, setSearchResult] = useState<SearchResult>({
    avatars: [],
    total: 0,
    page: 0,
    page_size: 24,
    total_pages: 0,
  });
  const [isLoading, setIsLoading] = useState(false);

  // Database & Sync State
  const [dbStatus, setDbStatus] = useState<DatabaseStatus | null>(null);
  const [syncProgress, setSyncProgress] = useState<SyncProgress | null>(null);
  const [isSyncing, setIsSyncing] = useState(false);

  // User State
  const [currentUser, setCurrentUser] = useState<UserProfile | null>(null);

  // UI Modals & Drawers
  const [selectedAvatar, setSelectedAvatar] = useState<AvatarEntry | null>(null);
  const [isDbModalOpen, setIsDbModalOpen] = useState(false);
  const [isLoginModalOpen, setIsLoginModalOpen] = useState(false);
  const [isSettingsModalOpen, setIsSettingsModalOpen] = useState(false);
  const [isDashboardOpen, setIsDashboardOpen] = useState(true);
  const [showOldImages, setShowOldImages] = useState(false);
  const [favoriteIds, setFavoriteIds] = useState<Set<string>>(new Set());
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);

  // Search execution
  const searchSeq = useRef(0);
  const executeSearch = useCallback(async (currentFilter: SearchFilter) => {
    const seq = ++searchSeq.current;
    setIsLoading(true);
    try {
      const result = await api.searchAvatars(currentFilter);
      if (seq === searchSeq.current) {
        setSearchResult(result);
      }
    } catch (err: any) {
      if (seq === searchSeq.current) {
        showToast(`Search error: ${err?.message || err}`, 'error');
      }
    } finally {
      if (seq === searchSeq.current) {
        setIsLoading(false);
      }
    }
  }, [showToast]);

  // Initial setup: check auth, check db status, listen to sync events
  useEffect(() => {
    let unlisten: (() => void) | undefined;

    (async () => {
      // 1. Listen for DB sync events
      unlisten = await api.onSyncProgress((progress) => {
        setSyncProgress(progress);
        if (progress.complete) {
          setIsSyncing(false);
          showToast('Database sync completed!', 'success');
          // Refresh database status and search results
          api.getDbStatus().then((status) => {
            setDbStatus(status);
            executeSearch(DEFAULT_FILTER);
          });
        } else if (progress.error) {
          setIsSyncing(false);
          showToast(`Sync failed: ${progress.error}`, 'error');
        } else {
          setIsSyncing(true);
        }
      });

      // 2. Check Auth State
      try {
        const user = await api.getAuthState();
        if (user) {
          setCurrentUser(user);
        }
      } catch (_) {}

      // 3. Load app settings & favorites
      api.getAppSettings().then((s) => setShowOldImages(s.show_old_images)).catch(() => {});
      api.getFavoriteIds().then((ids) => setFavoriteIds(new Set(ids))).catch(() => {});

      // 4. Check for a newer release (silent on failure)
      api
        .checkForUpdates()
        .then((info) => {
          if (info.update_available) setUpdateInfo(info);
        })
        .catch(() => {});

      // 3. Check DB Status
      try {
        const status = await api.getDbStatus();
        setDbStatus(status);
        if (!status.ready || status.avatar_count === 0) {
          // Open Database Modal on first start if no database exists
          setIsDbModalOpen(true);
        } else {
          executeSearch(DEFAULT_FILTER);
          // Auto-update when the database is older than one day (max once per day)
          const todayUtc = new Date().toISOString().slice(0, 10);
          const lastSyncDate = status.last_sync?.slice(0, 10);
          if (status.is_stale && lastSyncDate !== todayUtc) {
            showToast('Database is older than one day — updating automatically...', 'info');
            setIsDbModalOpen(true);
            setIsSyncing(true);
            api.syncDatabase().catch((err) => {
              setIsSyncing(false);
              showToast(`Failed to start database sync: ${err?.message || err}`, 'error');
            });
          }
        }
      } catch (_) {}
    })();

    return () => {
      if (unlisten) unlisten();
    };
  }, [executeSearch, showToast]);

  // Synchronized search handling
  const filterTimeoutRef = useRef<number | null>(null);

  const handleFilterChange = (updated: Partial<SearchFilter>) => {
    if ('name' in updated && updated.name !== undefined && updated.name !== searchQuery) {
      setSearchQuery(updated.name);
    }

    const nextFilter = { ...filter, ...updated };
    setFilter(nextFilter);

    const isTextInput = 'name' in updated || 'author' in updated || 'description' in updated;
    if (isTextInput) {
      if (filterTimeoutRef.current) {
        window.clearTimeout(filterTimeoutRef.current);
      }
      filterTimeoutRef.current = window.setTimeout(() => {
        executeSearch(nextFilter);
      }, 350);
    } else {
      executeSearch(nextFilter);
    }
  };

  const handleSearchChange = (query: string) => {
    setSearchQuery(query);
    handleFilterChange({ name: query, page: 0 });
  };

  const handleResetFilters = () => {
    setSearchQuery('');
    setFilter(DEFAULT_FILTER);
    executeSearch(DEFAULT_FILTER);
  };

  const handlePageChange = (newPage: number) => {
    const nextFilter = { ...filter, page: newPage };
    setFilter(nextFilter);
    executeSearch(nextFilter);
    window.scrollTo({ top: 0, behavior: 'smooth' });
  };

  const handleWearAvatar = async (avatarId: string) => {
    await api.wearAvatar(avatarId);
    showToast('Avatar selected successfully in VRChat!', 'success');
  };

  const handleLogout = async () => {
    await api.logout();
    setCurrentUser(null);
    showToast('Logged out of VRChat.', 'info');
  };

  const handleToggleOldImages = async (enabled: boolean) => {
    setShowOldImages(enabled);
    try {
      await api.updateAppSettings({ show_old_images: enabled });
      showToast(
        enabled ? 'Historical image revisions enabled' : 'Historical image revisions disabled',
        'info'
      );
    } catch (err: any) {
      setShowOldImages(!enabled);
      showToast(`Failed to update settings: ${err?.message || err}`, 'error');
    }
  };

  const handleToggleFavorite = async (avatar: AvatarEntry) => {
    try {
      const added = await api.toggleFavorite(
        avatar.avatar_id,
        avatar.name,
        avatar.author,
        undefined
      );
      setFavoriteIds((prev) => {
        const next = new Set(prev);
        if (added) {
          next.add(avatar.avatar_id);
        } else {
          next.delete(avatar.avatar_id);
        }
        return next;
      });
      showToast(
        added ? `Added "${avatar.name}" to favorites!` : `Removed "${avatar.name}" from favorites.`,
        'info'
      );
      if (filter.favorites_only) {
        executeSearch(filter);
      }
    } catch (err: any) {
      showToast(`Failed to update favorite: ${err?.message || err}`, 'error');
    }
  };

  const handleStartSync = async () => {
    try {
      setIsSyncing(true);
      await api.syncDatabase();
    } catch (err: any) {
      setIsSyncing(false);
      showToast(`Failed to start database sync: ${err?.message || err}`, 'error');
    }
  };

  const activeFilterCount =
    (filter.platform_pc ? 1 : 0) +
    (filter.platform_quest ? 1 : 0) +
    (filter.platform_ios ? 1 : 0) +
    (filter.include_impostors ? 1 : 0) +
    (filter.include_marketplace ? 0 : 1) +
    (filter.pc_rating ? 1 : 0) +
    (filter.quest_rating ? 1 : 0) +
    (filter.ios_rating ? 1 : 0) +
    (filter.selected_styles?.length || 0) +
    (filter.allowed_warnings !== undefined && filter.allowed_warnings !== 31 ? 1 : 0) +
    (filter.favorites_only ? 1 : 0) +
    (filter.author ? 1 : 0);

  return (
    <div className="min-h-screen bg-[#0b0f19] text-gray-100 flex flex-col selection:bg-cyan-500/30">
      {/* Top Application Header */}
      <Header
        searchQuery={searchQuery}
        onSearchChange={handleSearchChange}
        onSearchSubmit={() => executeSearch(filter)}
        dbStatus={dbStatus}
        isSyncing={isSyncing}
        onOpenDbModal={() => setIsDbModalOpen(true)}
        currentUser={currentUser}
        onOpenLoginModal={() => setIsLoginModalOpen(true)}
        onLogout={handleLogout}
        onOpenSettingsModal={() => setIsSettingsModalOpen(true)}
        isDashboardOpen={isDashboardOpen}
        onToggleDashboard={() => setIsDashboardOpen(!isDashboardOpen)}
        activeFilterCount={activeFilterCount}
        favoritesCount={favoriteIds.size}
        isFavoritesOnly={filter.favorites_only}
        onToggleFavorites={() => handleFilterChange({ favorites_only: !filter.favorites_only, page: 0 })}
      />

      {/* Update Available Banner */}
      {updateInfo && (
        <div className="bg-cyan-950/60 border-b border-cyan-800/50 px-4 py-2 flex items-center justify-center gap-3 text-xs text-cyan-200">
          <Download className="w-3.5 h-3.5 text-cyan-400" />
          <span>
            Update available: <span className="font-bold">v{updateInfo.latest_version}</span>
            <span className="text-gray-400"> (you have v{updateInfo.current_version})</span>
          </span>
          <button
            onClick={() => api.openInBrowser(updateInfo.release_url)}
            className="px-3 py-1 rounded-lg bg-cyan-600 hover:bg-cyan-500 text-white font-bold transition-colors"
          >
            Download
          </button>
          <button
            onClick={() => setUpdateInfo(null)}
            className="p-1 rounded-md hover:bg-gray-800 text-gray-400 hover:text-gray-200 transition-colors"
            title="Dismiss"
          >
            <X className="w-3.5 h-3.5" />
          </button>
        </div>
      )}

      {/* VRChat 3-Column Search Dashboard */}
      <VrcSearchDashboard
        filter={filter}
        onChange={handleFilterChange}
        onReset={handleResetFilters}
        onSearch={() => executeSearch(filter)}
        isOpen={isDashboardOpen}
        onToggle={() => setIsDashboardOpen(!isDashboardOpen)}
        totalResults={searchResult.total}
        showOldImages={showOldImages}
        onToggleOldImages={handleToggleOldImages}
      />

      {/* Main Content Area: Full-width avatar grid */}
      <div className="flex-1 flex overflow-hidden">
        <AvatarGrid
          avatars={searchResult.avatars}
          isLoading={isLoading}
          total={searchResult.total}
          page={searchResult.page}
          pageSize={searchResult.page_size}
          totalPages={searchResult.total_pages}
          onPageChange={handlePageChange}
          onSelect={setSelectedAvatar}
          onWear={handleWearAvatar}
          isLoggedIn={!!currentUser}
          showOldImages={showOldImages}
          favoriteIds={favoriteIds}
          onToggleFavorite={handleToggleFavorite}
        />
      </div>

      {/* Modals */}
      <AvatarModal
        avatar={selectedAvatar}
        onClose={() => setSelectedAvatar(null)}
        onWear={handleWearAvatar}
        isLoggedIn={!!currentUser}
        isFavorite={selectedAvatar ? favoriteIds.has(selectedAvatar.avatar_id) : false}
        onToggleFavorite={handleToggleFavorite}
      />

      <LoginModal
        isOpen={isLoginModalOpen}
        onClose={() => setIsLoginModalOpen(false)}
        onLoginSuccess={(user) => setCurrentUser(user)}
      />

      <DatabaseModal
        isOpen={isDbModalOpen}
        onClose={() => setIsDbModalOpen(false)}
        status={dbStatus}
        progress={syncProgress}
        isSyncing={isSyncing}
        onStartSync={handleStartSync}
      />

      <SettingsModal
        isOpen={isSettingsModalOpen}
        onClose={() => setIsSettingsModalOpen(false)}
        onSettingsSaved={() => {
          // Refresh the setting so cards refetch the correct details variant
          api.getAppSettings().then((s) => setShowOldImages(s.show_old_images)).catch(() => {});
        }}
      />
    </div>
  );
}

interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
}

class ErrorBoundary extends React.Component<{ children: React.ReactNode }, ErrorBoundaryState> {
  constructor(props: { children: React.ReactNode }) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
    console.error('[ErrorBoundary caught error]:', error, errorInfo);
  }

  render() {
    if (this.state.hasError) {
      return (
        <div className="min-h-screen bg-[#0a0d14] text-white flex flex-col items-center justify-center p-6 text-center">
          <div className="max-w-md p-6 rounded-2xl bg-gray-900 border border-red-500/30 space-y-4">
            <h2 className="text-lg font-bold text-red-400">Something went wrong</h2>
            <p className="text-xs text-gray-400 font-mono overflow-auto max-h-32 p-2 bg-black/50 rounded">
              {this.state.error?.message || 'An unexpected error occurred.'}
            </p>
            <button
              onClick={() => {
                this.setState({ hasError: false, error: null });
                window.location.reload();
              }}
              className="px-4 py-2 rounded-xl bg-cyan-600 hover:bg-cyan-500 text-xs font-semibold text-white transition-colors"
            >
              Reload Application
            </button>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}

export default function App() {
  return (
    <ErrorBoundary>
      <ToastProvider>
        <MainApp />
      </ToastProvider>
    </ErrorBoundary>
  );
}
