use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

use crate::config::{AppConfig, ConfigManager};
use crate::db::DbManager;
use crate::downloader::run_sync_pipeline;
use crate::models::{
    AppSettings, AvatarEntry, AvatarOnlineDetails, DatabaseStatus, LoginResponse, SearchFilter, SearchResult,
    SyncProgress, UserProfile,
};
use crate::vrc_api::VrcApiClient;

pub struct AppState {
    pub db: Arc<DbManager>,
    pub vrc: Arc<VrcApiClient>,
    pub config: Arc<ConfigManager>,
    pub db_path: PathBuf,
    pub is_syncing: Arc<AtomicBool>,
}

/// `ConfigManager::load` does blocking file IO; keep it off the async runtime.
async fn load_config(state: &State<'_, AppState>) -> Result<AppConfig, String> {
    let config = Arc::clone(&state.config);
    tauri::async_runtime::spawn_blocking(move || config.load())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_db_status(state: State<'_, AppState>) -> Result<DatabaseStatus, String> {
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || db.get_status())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_database(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if state
        .is_syncing
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("Database synchronization is already in progress.".into());
    }

    let db_path = state.db_path.clone();
    let is_syncing = state.is_syncing.clone();

    tauri::async_runtime::spawn(async move {
        let app_clone = app.clone();
        let res = run_sync_pipeline(&db_path, move |progress: SyncProgress| {
            let _ = app_clone.emit("db-sync-progress", progress);
        })
        .await;

        is_syncing.store(false, Ordering::SeqCst);

        if let Err(e) = res {
            let err_msg = e.to_string();
            let _ = app.emit(
                "db-sync-progress",
                SyncProgress {
                    stage: "Error".to_string(),
                    step: 0,
                    total_steps: 6,
                    percent: 0.0,
                    message: format!("Sync failed: {}", err_msg),
                    complete: false,
                    error: Some(err_msg),
                },
            );
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn search_avatars(
    filter: SearchFilter,
    state: State<'_, AppState>,
) -> Result<SearchResult, String> {
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || db.search(filter))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_avatar_by_id(
    avatar_id: String,
    state: State<'_, AppState>,
) -> Result<Option<AvatarEntry>, String> {
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || db.get_by_avatar_id(&avatar_id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn login(
    username: String,
    password: String,
    remember_me: bool,
    state: State<'_, AppState>,
) -> Result<LoginResponse, String> {
    let res = state
        .vrc
        .login(&username, &password)
        .await
        .map_err(|e| e.to_string())?;

    if res.success {
        if let Some(cookie) = res.auth_cookie.clone() {
            let config = Arc::clone(&state.config);
            let uid = res.user.as_ref().map(|u| u.id.clone()).unwrap_or_default();
            let uname = username.clone();
            let _ = tauri::async_runtime::spawn_blocking(move || {
                config.set_auth(&cookie, &uid, &uname, remember_me)
            })
            .await;
        }
    }

    Ok(res)
}

#[tauri::command]
pub async fn verify_2fa(
    code: String,
    auth_cookie: String,
    two_factor_type: String,
    remember_me: bool,
    state: State<'_, AppState>,
) -> Result<LoginResponse, String> {
    let res = state
        .vrc
        .verify_2fa(&code, &auth_cookie, &two_factor_type)
        .await
        .map_err(|e| e.to_string())?;

    if res.success {
        if let Some(cookie) = res.auth_cookie.clone() {
            let config = Arc::clone(&state.config);
            let uid = res.user.as_ref().map(|u| u.id.clone()).unwrap_or_default();
            let uname = res.user.as_ref().map(|u| u.username.clone()).unwrap_or_default();
            let _ = tauri::async_runtime::spawn_blocking(move || {
                config.set_auth(&cookie, &uid, &uname, remember_me)
            })
            .await;
        }
    }

    Ok(res)
}

#[tauri::command]
pub async fn get_auth_state(state: State<'_, AppState>) -> Result<Option<UserProfile>, String> {
    let cfg = load_config(&state).await?;
    if cfg.auth_cookie.is_empty() {
        return Ok(None);
    }

    match state.vrc.get_current_user(&cfg.auth_cookie).await {
        Ok(Some(profile)) => Ok(Some(profile)),
        Ok(None) => {
            // Cookie expired/rejected, clear stored auth
            let config = Arc::clone(&state.config);
            let _ = tauri::async_runtime::spawn_blocking(move || config.clear_auth()).await;
            Ok(None)
        }
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn logout(state: State<'_, AppState>) -> Result<(), String> {
    let config = Arc::clone(&state.config);
    tauri::async_runtime::spawn_blocking(move || config.clear_auth())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn wear_avatar(
    avatar_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let cfg = load_config(&state).await?;
    if cfg.auth_cookie.is_empty() {
        return Err("You must be logged in to wear avatars.".into());
    }

    state
        .vrc
        .select_avatar(&cfg.auth_cookie, &avatar_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_avatar_details(
    avatar_id: String,
    state: State<'_, AppState>,
) -> Result<AvatarOnlineDetails, String> {
    let cfg = load_config(&state).await?;
    state
        .vrc
        .get_avatar_details(&cfg.auth_cookie, &avatar_id, cfg.show_old_images)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_app_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    let cfg = load_config(&state).await?;
    Ok(AppSettings {
        show_old_images: cfg.show_old_images,
    })
}

#[tauri::command]
pub async fn update_app_settings(
    settings: AppSettings,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let config = Arc::clone(&state.config);
    tauri::async_runtime::spawn_blocking(move || {
        config.set_show_old_images(settings.show_old_images)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_image_cache(state: State<'_, AppState>) -> Result<(), String> {
    let vrc = Arc::clone(&state.vrc);
    tauri::async_runtime::spawn_blocking(move || vrc.clear_cache())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn fetch_avatar_image(
    image_url: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let cfg = load_config(&state).await?;
    state
        .vrc
        .fetch_image_base64(&cfg.auth_cookie, &image_url)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_avatar_thumbnail(
    avatar_id: String,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let cfg = load_config(&state).await?;
    if cfg.auth_cookie.is_empty() {
        return Ok(None);
    }
    state
        .vrc
        .get_avatar_thumbnail(&cfg.auth_cookie, &avatar_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_in_browser(url: String) -> Result<(), String> {
    let parsed = reqwest::Url::parse(&url).map_err(|e| e.to_string())?;
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Err("Only http(s) URLs may be opened".into());
    }
    open::that(parsed.as_str()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn toggle_favorite(
    avatar_id: String,
    name: Option<String>,
    author: Option<String>,
    thumbnail_url: Option<String>,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || {
        db.toggle_favorite(&avatar_id, name.as_deref(), author.as_deref(), thumbnail_url.as_deref())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_favorite_ids(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || db.get_favorite_ids())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn is_favorite(avatar_id: String, state: State<'_, AppState>) -> Result<bool, String> {
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || db.is_favorite(&avatar_id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}
