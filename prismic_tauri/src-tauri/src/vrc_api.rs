use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use anyhow::{anyhow, Result};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use reqwest::header::{HeaderMap, HeaderValue, COOKIE, USER_AGENT};
use reqwest::Client;
use serde_json::{json, Value};
use tokio::sync::Semaphore;

use crate::models::{AvatarOnlineDetails, LoginResponse, UserProfile};

const API_BASE: &str = "https://api.vrchat.cloud/api/1";
const APP_USER_AGENT: &str = "VRChatAvatarGUI/0.1";

// Same set as JavaScript's encodeURIComponent; VRChat requires credentials encoded this way before Basic auth.
const URI_COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'!')
    .remove(b'~')
    .remove(b'*')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')');

fn encode_credential(value: &str) -> String {
    utf8_percent_encode(value, URI_COMPONENT).to_string()
}

async fn api_error_message(res: reqwest::Response) -> String {
    let status = res.status();
    let message = res
        .json::<Value>()
        .await
        .ok()
        .and_then(|v| v.pointer("/error/message").and_then(|m| m.as_str()).map(|m| m.trim_matches('"').to_string()));
    match message {
        Some(m) if !m.is_empty() => format!("{} (HTTP {})", m, status.as_u16()),
        _ => format!("HTTP {}", status),
    }
}

pub struct VrcApiClient {
    client: Client,
    cache_dir: PathBuf,
    semaphore: Arc<Semaphore>,
}

impl VrcApiClient {
    pub fn new(cache_dir: PathBuf) -> Result<Self> {
        let _ = fs::create_dir_all(&cache_dir);
        let img_dir = cache_dir.join("images");
        let _ = fs::create_dir_all(&img_dir);
        let details_dir = cache_dir.join("details");
        let _ = fs::create_dir_all(&details_dir);

        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .cookie_store(true)
            .build()?;

        let semaphore = Arc::new(Semaphore::new(4));

        Ok(Self { client, cache_dir, semaphore })
    }

    fn auth_headers(auth_cookie: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(APP_USER_AGENT));
        if !auth_cookie.is_empty() {
            if let Ok(val) = HeaderValue::from_str(&format!("auth={}", auth_cookie)) {
                headers.insert(COOKIE, val);
            }
        }
        headers
    }

    pub async fn login(&self, username: &str, password: &str) -> Result<LoginResponse> {
        let res = self
            .client
            .get(format!("{}/auth/user", API_BASE))
            .header(USER_AGENT, APP_USER_AGENT)
            .basic_auth(encode_credential(username), Some(encode_credential(password)))
            .send()
            .await?;

        if !res.status().is_success() {
            return Ok(LoginResponse {
                success: false,
                requires_2fa: false,
                two_factor_type: None,
                auth_cookie: None,
                user: None,
                error: Some(format!("Login failed: {}", api_error_message(res).await)),
            });
        }

        let auth_cookie = res
            .cookies()
            .find(|c| c.name() == "auth")
            .map(|c| c.value().to_string())
            .unwrap_or_default();

        let json_body: Value = res.json().await?;

        // Check if 2FA is needed
        if let Some(reqs) = json_body.get("requiresTwoFactorAuth").and_then(|v| v.as_array()) {
            if let Some(first) = reqs.first().and_then(|v| v.as_str()) {
                return Ok(LoginResponse {
                    success: false,
                    requires_2fa: true,
                    two_factor_type: Some(first.to_string()),
                    auth_cookie: Some(auth_cookie),
                    user: None,
                    error: None,
                });
            }
        }

        // Login succeeded without 2FA
        let user = self.parse_user_profile(&json_body);

        Ok(LoginResponse {
            success: true,
            requires_2fa: false,
            two_factor_type: None,
            auth_cookie: Some(auth_cookie),
            user: Some(user),
            error: None,
        })
    }

    pub async fn verify_2fa(&self, code: &str, auth_cookie: &str, two_factor_type: &str) -> Result<LoginResponse> {
        let endpoint = match two_factor_type {
            "emailOtp" => format!("{}/auth/twofactorauth/emailotp/verify", API_BASE),
            "otp" | "totp" => format!("{}/auth/twofactorauth/totp/verify", API_BASE),
            _ => format!("{}/auth/twofactorauth/totp/verify", API_BASE),
        };

        let res = self
            .client
            .post(&endpoint)
            .headers(Self::auth_headers(auth_cookie))
            .json(&json!({ "code": code.trim() }))
            .send()
            .await?;

        if !res.status().is_success() {
            return Ok(LoginResponse {
                success: false,
                requires_2fa: true,
                two_factor_type: Some(two_factor_type.to_string()),
                auth_cookie: Some(auth_cookie.to_string()),
                user: None,
                error: Some(format!("Invalid 2FA code: {}", api_error_message(res).await)),
            });
        }

        let body: Value = res.json().await?;
        let verified = body.get("verified").and_then(|v| v.as_bool()).unwrap_or(false);

        if !verified {
            return Ok(LoginResponse {
                success: false,
                requires_2fa: true,
                two_factor_type: Some(two_factor_type.to_string()),
                auth_cookie: Some(auth_cookie.to_string()),
                user: None,
                error: Some("Two-factor verification rejected.".to_string()),
            });
        }

        // Retrieve full user profile
        let user_profile = self.get_current_user(auth_cookie).await?;

        Ok(LoginResponse {
            success: true,
            requires_2fa: false,
            two_factor_type: None,
            auth_cookie: Some(auth_cookie.to_string()),
            user: Some(user_profile),
            error: None,
        })
    }

    pub async fn get_current_user(&self, auth_cookie: &str) -> Result<UserProfile> {
        let res = self
            .client
            .get(format!("{}/auth/user", API_BASE))
            .headers(Self::auth_headers(auth_cookie))
            .send()
            .await?;

        if !res.status().is_success() {
            return Err(anyhow!("Failed to get user profile: HTTP {}", res.status()));
        }

        let body: Value = res.json().await?;
        Ok(self.parse_user_profile(&body))
    }

    fn parse_user_profile(&self, json_body: &Value) -> UserProfile {
        let id = json_body.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let username = json_body.get("username").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let display_name = json_body.get("displayName").and_then(|v| v.as_str()).unwrap_or(&username).to_string();
        let current_avatar_id = json_body.get("currentAvatar").and_then(|v| v.as_str()).map(|s| s.to_string());
        let current_avatar_image_url = json_body.get("currentAvatarImageUrl").and_then(|v| v.as_str()).map(|s| s.to_string());

        UserProfile {
            id,
            username,
            display_name,
            current_avatar_id,
            current_avatar_name: None,
            current_avatar_image_url,
        }
    }

    /// Extract the image URL of the newest completed version of a gallery file.
    /// Falls back to the newest version with any URL when none is complete.
    fn latest_gallery_version_url(item: &Value) -> Option<String> {
        let versions = item.get("versions").and_then(|v| v.as_array())?;
        let mut fallback: Option<String> = None;
        for v in versions.iter().rev() {
            if let Some(url) = v.get("file").and_then(|f| f.get("url")).and_then(|u| u.as_str()) {
                if v.get("status").and_then(|s| s.as_str()) == Some("complete") {
                    return Some(url.to_string());
                }
                if fallback.is_none() {
                    fallback = Some(url.to_string());
                }
            }
        }
        fallback
    }

    pub async fn select_avatar(&self, auth_cookie: &str, avatar_id: &str) -> Result<()> {        let url = format!("{}/avatars/{}/select", API_BASE, avatar_id);
        let res = self
            .client
            .put(&url)
            .headers(Self::auth_headers(auth_cookie))
            .send()
            .await?;

        if !res.status().is_success() {
            return Err(anyhow!("Failed to select avatar: HTTP {}", res.status()));
        }

        Ok(())
    }

    pub async fn get_avatar_details(&self, auth_cookie: &str, avatar_id: &str, show_old_images: bool) -> Result<AvatarOnlineDetails> {
        let suffix = if show_old_images { "_all_v2.json" } else { "_v2.json" };
        let cached_file = self.cache_dir.join("details").join(format!("{}{}", avatar_id, suffix));
        if cached_file.exists() {
            if let Ok(data) = fs::read_to_string(&cached_file) {
                if let Ok(details) = serde_json::from_str::<AvatarOnlineDetails>(&data) {
                    return Ok(details);
                }
            }
        }

        let _permit = self.semaphore.acquire().await.ok();
        let url = format!("{}/avatars/{}", API_BASE, avatar_id);
        let res = self
            .client
            .get(&url)
            .headers(Self::auth_headers(auth_cookie))
            .send()
            .await?;

        if !res.status().is_success() {
            return Err(anyhow!("Failed to fetch avatar details: HTTP {}", res.status()));
        }

        let body: Value = res.json().await?;
        let id = body.get("id").and_then(|v| v.as_str()).unwrap_or(avatar_id).to_string();
        let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("Unknown").to_string();
        let description = body.get("description").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let author_id = body.get("authorId").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let author_name = body.get("authorName").and_then(|v| v.as_str()).unwrap_or("Unknown").to_string();
        let image_url = body.get("imageUrl").and_then(|v| v.as_str()).map(|s| s.to_string());
        let thumbnail_image_url = body.get("thumbnailImageUrl").and_then(|v| v.as_str()).map(|s| s.to_string());
        let release_status = body.get("releaseStatus").and_then(|v| v.as_str()).unwrap_or("public").to_string();
        let updated_at = body
            .get("updated_at")
            .or_else(|| body.get("created_at"))
            .and_then(|v| v.as_str())
            .map(|s| {
                if s.len() >= 10 {
                    s[..10].to_string()
                } else {
                    s.to_string()
                }
            });

        let tags: Vec<String> = body
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|t| t.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default();

        // Collect images: current primary image + active gallery pictures
        let mut images = Vec::new();
        let mut visited = std::collections::HashSet::new();
        if let Some(u) = &image_url {
            if visited.insert(u.clone()) {
                images.push(u.clone());
            }
        }

        // Whether the gallery lookup actually ran to completion this fetch.
        // If it failed we must NOT cache the details, or a transient network
        // error would permanently hide the avatar's gallery.
        let mut gallery_checked = false;

        // Embedded gallery is authoritative when present
        if let Some(gallery) = body.get("gallery").and_then(|v| v.as_array()) {
            gallery_checked = true;
            for item in gallery {
                if let Some(file_url) = Self::latest_gallery_version_url(item) {
                    if visited.insert(file_url.clone()) {
                        images.push(file_url);
                    }
                }
            }
        }

        // Dedicated VRChat files endpoint as fallback / supplement
        let gallery_url = format!("{}/files?tag=avatargallery&galleryId={}&n=100", API_BASE, avatar_id);
        match self.client.get(&gallery_url).headers(Self::auth_headers(auth_cookie)).send().await {
            Ok(gallery_res) if gallery_res.status().is_success() => {
                gallery_checked = true;
                if let Ok(gallery_items) = gallery_res.json::<Value>().await {
                    if let Some(items) = gallery_items.as_array() {
                        for item in items {
                            if let Some(file_url) = Self::latest_gallery_version_url(item) {
                                if visited.insert(file_url.clone()) {
                                    images.push(file_url);
                                }
                            }
                        }
                    }
                }
            }
            Ok(gallery_res) => {
                log::warn!("Gallery files request for {} failed: HTTP {}", avatar_id, gallery_res.status());
            }
            Err(e) => {
                log::warn!("Gallery files request for {} errored: {}", avatar_id, e);
            }
        }

        // Capture the official gallery image count (current image + active gallery pictures)
        let images_count = images.len();

        // Whether the historical-version lookup completed (when enabled)
        let mut history_ok = true;

        // If enabled: fetch older superseded file versions from avatar upload history
        if show_old_images {
            if let Some(url) = &image_url {
                let parts: Vec<&str> = url.split('/').collect();
                if let Some(file_id) = parts.into_iter().find(|p| p.starts_with("file_")) {
                    let meta_url = format!("{}/file/{}", API_BASE, file_id);
                    match self.client.get(&meta_url).headers(Self::auth_headers(auth_cookie)).send().await {
                        Ok(meta_res) if meta_res.status().is_success() => {
                            if let Ok(meta_body) = meta_res.json::<Value>().await {
                                if let Some(versions) = meta_body.get("versions").and_then(|v| v.as_array()) {
                                    for v in versions {
                                        if let Some(file_url) = v.get("file").and_then(|f| f.get("url")).and_then(|u| u.as_str()) {
                                            if visited.insert(file_url.to_string()) {
                                                images.push(file_url.to_string());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        _ => {
                            history_ok = false;
                        }
                    }
                }
            }
        }

        let details = AvatarOnlineDetails {
            id,
            name,
            description,
            author_id,
            author_name,
            image_url,
            thumbnail_image_url,
            release_status,
            tags,
            images,
            images_count,
            updated_at,
        };

        // Only persist fully-fetched details: a failed gallery or history
        // lookup must be retried next time instead of cached forever.
        if gallery_checked && history_ok {
            if let Ok(json_str) = serde_json::to_string(&details) {
                let _ = fs::write(&cached_file, json_str);
            }
        }

        Ok(details)
    }

    fn detect_mime(bytes: &[u8]) -> &'static str {
        if bytes.len() >= 4 && bytes[0] == 0x89 && bytes[1] == 0x50 {
            "image/png"
        } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
            "image/webp"
        } else if bytes.len() >= 4 && &bytes[0..4] == b"GIF8" {
            "image/gif"
        } else {
            "image/jpeg"
        }
    }

    pub async fn fetch_image_base64(&self, auth_cookie: &str, image_url: &str) -> Result<String> {
        // Sanitize cache filename
        let hash = format!("{:x}", md5_hash(image_url.as_bytes()));
        let cached_file = self.cache_dir.join("images").join(format!("{}.img", hash));

        if cached_file.exists() {
            if let Ok(bytes) = fs::read(&cached_file) {
                return Ok(format!("data:{};base64,{}", Self::detect_mime(&bytes), BASE64.encode(bytes)));
            }
        }

        let res = self
            .client
            .get(image_url)
            .headers(Self::auth_headers(auth_cookie))
            .send()
            .await?;

        if !res.status().is_success() {
            return Err(anyhow!("Failed to fetch image: HTTP {}", res.status()));
        }

        let mime = res
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("image/jpeg")
            .to_string();

        let bytes = res.bytes().await?.to_vec();
        let _ = fs::write(&cached_file, &bytes);

        Ok(format!("data:{};base64,{}", mime, BASE64.encode(bytes)))
    }

    pub async fn get_avatar_thumbnail(&self, auth_cookie: &str, avatar_id: &str) -> Result<Option<String>> {
        let cached_file = self.cache_dir.join("images").join(format!("{}.img", avatar_id));
        let none_marker = self.cache_dir.join("images").join(format!("{}.none", avatar_id));

        if none_marker.exists() {
            return Ok(None);
        }

        if cached_file.exists() {
            if let Ok(bytes) = fs::read(&cached_file) {
                return Ok(Some(format!("data:{};base64,{}", Self::detect_mime(&bytes), BASE64.encode(bytes))));
            }
        }

        let _permit = self.semaphore.acquire().await.ok();

        let url = format!("{}/avatars/{}", API_BASE, avatar_id);
        let res = self
            .client
            .get(&url)
            .headers(Self::auth_headers(auth_cookie))
            .send()
            .await?;

        if !res.status().is_success() {
            if res.status().as_u16() == 404 {
                let _ = fs::write(&none_marker, b"");
            }
            return Ok(None);
        }

        let body: Value = res.json().await?;
        let img_url = body
            .get("imageUrl")
            .and_then(|v| v.as_str())
            .or_else(|| body.get("thumbnailImageUrl").and_then(|v| v.as_str()));

        let target_url = match img_url {
            Some(u) => u,
            None => {
                let _ = fs::write(&none_marker, b"");
                return Ok(None);
            }
        };

        let img_res = self
            .client
            .get(target_url)
            .headers(Self::auth_headers(auth_cookie))
            .send()
            .await?;

        if !img_res.status().is_success() {
            return Ok(None);
        }

        let mime = img_res
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("image/jpeg")
            .to_string();

        let bytes = img_res.bytes().await?.to_vec();
        let _ = fs::write(&cached_file, &bytes);

        Ok(Some(format!("data:{};base64,{}", mime, BASE64.encode(bytes))))
    }

    pub fn clear_cache(&self) -> Result<()> {
        let img_dir = self.cache_dir.join("images");
        let details_dir = self.cache_dir.join("details");
        let _ = fs::remove_dir_all(&img_dir);
        let _ = fs::remove_dir_all(&details_dir);
        let _ = fs::create_dir_all(&img_dir);
        let _ = fs::create_dir_all(&details_dir);
        Ok(())
    }
}

// Simple deterministic hash for file caching
fn md5_hash(bytes: &[u8]) -> u128 {
    let mut hash: u128 = 0xcbf29ce484222325;
    for &b in bytes {
        hash ^= b as u128;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::encode_credential;

    #[test]
    fn test_encode_credential_matches_encode_uri_component() {
        assert_eq!(encode_credential("plainUser_1.-"), "plainUser_1.-");
        assert_eq!(encode_credential("p@ss:w0rd+&%"), "p%40ss%3Aw0rd%2B%26%25");
        assert_eq!(encode_credential("a b!~*'()"), "a%20b!~*'()");
        assert_eq!(encode_credential("é"), "%C3%A9");
    }
}
