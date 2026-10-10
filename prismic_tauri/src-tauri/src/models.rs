use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvatarEntry {
    pub id: String,
    pub name: String,
    pub author: String,
    pub description: String,
    pub avatar_id: String,
    pub tags: Vec<String>,
    pub quest: bool,
    pub ios: bool,
    pub platform: u8,
    pub impostor_platform: u8,
    pub pc_rating: u8,
    pub quest_rating: u8,
    pub ios_rating: u8,
    pub content_warnings: u8,
    pub style_filter: u16,
    pub marketplace: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchFilter {
    pub query: Option<String>,
    pub name: Option<String>,
    pub name_search_method: Option<String>,
    #[serde(default)]
    pub name_include_desc_tags: bool,
    pub author: Option<String>,
    pub author_search_method: Option<String>,
    pub description: Option<String>,
    pub platform_pc: bool,
    pub platform_quest: bool,
    pub platform_ios: bool,
    #[serde(default)]
    pub include_impostors: bool,
    #[serde(default = "default_true")]
    pub include_marketplace: bool,
    pub pc_rating: Option<u8>,
    pub quest_rating: Option<u8>,
    pub ios_rating: Option<u8>,
    #[serde(default)]
    pub selected_styles: Vec<u16>,
    pub allowed_warnings: Option<u8>,
    pub marketplace_only: bool,
    #[serde(default)]
    pub favorites_only: bool,
    pub page: usize,
    pub page_size: usize,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub avatars: Vec<AvatarEntry>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseStatus {
    pub ready: bool,
    pub avatar_count: usize,
    pub author_count: usize,
    pub last_updated: Option<String>,
    pub last_sync: Option<String>,
    pub is_stale: bool,
    pub file_size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncProgress {
    pub stage: String,
    pub step: usize,
    pub total_steps: usize,
    pub percent: f32,
    pub message: String,
    pub complete: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub current_avatar_id: Option<String>,
    pub current_avatar_name: Option<String>,
    pub current_avatar_image_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub success: bool,
    pub requires_2fa: bool,
    pub two_factor_type: Option<String>,
    pub auth_cookie: Option<String>,
    pub user: Option<UserProfile>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvatarOnlineDetails {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author_id: String,
    pub author_name: String,
    pub image_url: Option<String>,
    pub thumbnail_image_url: Option<String>,
    pub release_status: String,
    pub tags: Vec<String>,
    pub images: Vec<String>,
    #[serde(default)]
    pub images_count: usize,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub show_old_images: bool,
}
