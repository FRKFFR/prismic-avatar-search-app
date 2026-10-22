export interface AvatarEntry {
  id: string;
  name: string;
  author: string;
  description: string;
  avatar_id: string;
  tags: string[];
  quest: boolean;
  ios: boolean;
  platform: number;
  impostor_platform: number;
  pc_rating: number;
  quest_rating: number;
  ios_rating: number;
  content_warnings: number;
  style_filter: number;
  marketplace: boolean;
}

export type TextSearchMethod =
  | 'match_all_words'
  | 'match_phrase'
  | 'exact_phrase'
  | 'starts_with'
  | 'ends_with';

export interface SearchFilter {
  query?: string;
  name?: string;
  name_search_method?: TextSearchMethod;
  name_include_desc_tags?: boolean;
  author?: string;
  author_search_method?: TextSearchMethod;
  description?: string;
  platform_pc: boolean;
  platform_quest: boolean;
  platform_ios: boolean;
  include_impostors: boolean;
  include_marketplace: boolean;
  pc_rating?: number;
  quest_rating?: number;
  ios_rating?: number;
  selected_styles?: number[];
  allowed_warnings?: number;
  marketplace_only: boolean;
  favorites_only?: boolean;
  page: number;
  page_size: number;
}

export interface SearchResult {
  avatars: AvatarEntry[];
  total: number;
  page: number;
  page_size: number;
  total_pages: number;
}

export interface DatabaseStatus {
  ready: boolean;
  avatar_count: number;
  author_count: number;
  last_updated?: string;
  last_sync?: string;
  is_stale: boolean;
  file_size_bytes: number;
}

export interface SyncProgress {
  stage: string;
  step: number;
  total_steps: number;
  percent: number;
  message: string;
  complete: boolean;
  error?: string;
}

export interface UserProfile {
  id: string;
  username: string;
  display_name: string;
  current_avatar_id?: string;
  current_avatar_name?: string;
  current_avatar_image_url?: string;
}

export interface LoginResponse {
  success: boolean;
  requires_2fa: boolean;
  two_factor_type?: string;
  auth_cookie?: string;
  user?: UserProfile;
  error?: string;
}

export interface AvatarOnlineDetails {
  id: string;
  name: string;
  description: string;
  author_id: string;
  author_name: string;
  image_url?: string;
  thumbnail_image_url?: string;
  release_status: string;
  tags: string[];
  images: string[];
  images_count?: number;
  updated_at?: string;
}

export interface AppSettings {
  show_old_images: boolean;
}

export interface UpdateInfo {
  current_version: string;
  latest_version: string;
  release_url: string;
  update_available: boolean;
}
