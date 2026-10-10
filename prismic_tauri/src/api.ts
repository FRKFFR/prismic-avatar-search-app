import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  AppSettings,
  AvatarEntry,
  AvatarOnlineDetails,
  DatabaseStatus,
  LoginResponse,
  SearchFilter,
  SearchResult,
  SyncProgress,
  UserProfile,
} from './types';

export const api = {
  isAvailable: isTauri,

  async getDbStatus(): Promise<DatabaseStatus> {
    if (!isTauri()) {
      return {
        ready: true,
        avatar_count: 42150,
        author_count: 12400,
        last_updated: '2026-09-28',
        last_sync: '2026-09-28T10:00:00Z',
        is_stale: false,
        file_size_bytes: 52428800,
      };
    }
    return invoke<DatabaseStatus>('get_db_status');
  },

  async syncDatabase(): Promise<void> {
    if (!isTauri()) return;
    return invoke<void>('sync_database');
  },

  onSyncProgress(callback: (progress: SyncProgress) => void): Promise<UnlistenFn> {
    if (!isTauri()) {
      return Promise.resolve(() => {});
    }
    return listen<SyncProgress>('db-sync-progress', (event) => {
      callback(event.payload);
    });
  },

  async searchAvatars(filter: SearchFilter): Promise<SearchResult> {
    if (!isTauri()) {
      // Mock data for browser testing
      return {
        avatars: [
          {
            id: 'avtr_sample_1',
            name: 'Cyberpunk Neon Fox',
            author: 'KitsuneDev',
            description: 'Futuristic glowing fox avatar with custom physbones and expressions.',
            avatar_id: 'avtr_sample_1',
            tags: ['Sci-Fi', 'Animal', 'Furry'],
            quest: true,
            ios: false,
            platform: 3,
            impostor_platform: 0,
            pc_rating: 1,
            quest_rating: 2,
            ios_rating: 0,
            content_warnings: 0,
            style_filter: 6,
            marketplace: true,
          },
          {
            id: 'avtr_sample_2',
            name: 'Aetherial Celestial Mage',
            author: 'StarWeaver',
            description: 'Elegantly dressed celestial sorcerer with dynamic starry particle effects.',
            avatar_id: 'avtr_sample_2',
            tags: ['Fantasy', 'Anime', 'Fashion'],
            quest: true,
            ios: true,
            platform: 7,
            impostor_platform: 0,
            pc_rating: 2,
            quest_rating: 3,
            ios_rating: 3,
            content_warnings: 0,
            style_filter: 1544,
            marketplace: false,
          },
        ],
        total: 2,
        page: 0,
        page_size: 24,
        total_pages: 1,
      };
    }
    return invoke<SearchResult>('search_avatars', { filter });
  },

  async getAvatarById(avatarId: string): Promise<AvatarEntry | null> {
    if (!isTauri()) return null;
    return invoke<AvatarEntry | null>('get_avatar_by_id', { avatarId });
  },

  async login(username: string, password: string, rememberMe: boolean): Promise<LoginResponse> {
    if (!isTauri()) {
      return {
        success: true,
        requires_2fa: false,
        user: {
          id: 'usr_mock_123',
          username,
          display_name: username,
          current_avatar_id: 'avtr_sample_1',
        },
      };
    }
    return invoke<LoginResponse>('login', { username, password, rememberMe });
  },

  async verify2fa(
    code: string,
    authCookie: string,
    twoFactorType: string,
    rememberMe: boolean
  ): Promise<LoginResponse> {
    if (!isTauri()) {
      return {
        success: true,
        requires_2fa: false,
      };
    }
    return invoke<LoginResponse>('verify_2fa', { code, authCookie, twoFactorType, rememberMe });
  },

  async getAuthState(): Promise<UserProfile | null> {
    if (!isTauri()) return null;
    return invoke<UserProfile | null>('get_auth_state');
  },

  async logout(): Promise<void> {
    if (!isTauri()) return;
    return invoke<void>('logout');
  },

  async wearAvatar(avatarId: string): Promise<void> {
    if (!isTauri()) {
      console.log('Mock wear avatar:', avatarId);
      return;
    }
    return invoke<void>('wear_avatar', { avatarId });
  },

  async getAvatarDetails(avatarId: string): Promise<AvatarOnlineDetails> {
    if (!isTauri()) {
      return {
        id: avatarId,
        name: 'Online Avatar Details',
        description: 'VRChat avatar online description preview.',
        author_id: 'usr_author',
        author_name: 'Author',
        release_status: 'public',
        tags: ['sample', 'preview'],
        images: [],
      };
    }
    return invoke<AvatarOnlineDetails>('get_avatar_details', { avatarId });
  },

  async fetchAvatarImage(imageUrl: string): Promise<string> {
    if (!isTauri()) return imageUrl;
    return invoke<string>('fetch_avatar_image', { imageUrl });
  },

  async getAvatarThumbnail(avatarId: string): Promise<string | null> {
    if (!isTauri()) return null;
    return invoke<string | null>('get_avatar_thumbnail', { avatarId });
  },

  async openInBrowser(url: string): Promise<void> {
    if (!isTauri()) {
      window.open(url, '_blank');
      return;
    }
    return invoke<void>('open_in_browser', { url });
  },

  async getAppSettings(): Promise<AppSettings> {
    if (!isTauri()) {
      return { show_old_images: false };
    }
    return invoke<AppSettings>('get_app_settings');
  },

  async updateAppSettings(settings: AppSettings): Promise<void> {
    if (!isTauri()) return;
    return invoke<void>('update_app_settings', { settings });
  },

  async clearImageCache(): Promise<void> {
    if (!isTauri()) return;
    return invoke<void>('clear_image_cache');
  },

  async toggleFavorite(
    avatarId: string,
    name?: string,
    author?: string,
    thumbnailUrl?: string
  ): Promise<boolean> {
    if (!isTauri()) return true;
    return invoke<boolean>('toggle_favorite', {
      avatarId,
      name,
      author,
      thumbnailUrl,
    });
  },

  async getFavoriteIds(): Promise<string[]> {
    if (!isTauri()) return [];
    return invoke<string[]>('get_favorite_ids');
  },

  async isFavorite(avatarId: string): Promise<boolean> {
    if (!isTauri()) return false;
    return invoke<boolean>('is_favorite', { avatarId });
  },
};
