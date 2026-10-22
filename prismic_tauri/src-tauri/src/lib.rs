mod commands;
mod config;
mod db;
mod downloader;
mod models;
mod vrc_api;

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::Manager;

use commands::AppState;
use config::ConfigManager;
use db::DbManager;
use vrc_api::VrcApiClient;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // Determine data / cache directory
            let app_data = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("cache"));
            let _ = fs::create_dir_all(&app_data);

            let db_path = app_data.join("avatars.db");
            let cache_dir = app_data.join("cache");
            let _ = fs::create_dir_all(&cache_dir);

            let db = Arc::new(DbManager::new(db_path.clone()));
            let vrc = Arc::new(VrcApiClient::new(cache_dir)?);
            let config = Arc::new(ConfigManager::new(app_data));
            let is_syncing = Arc::new(AtomicBool::new(false));

            app.manage(AppState {
                db,
                vrc,
                config,
                db_path,
                is_syncing,
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_db_status,
            commands::sync_database,
            commands::search_avatars,
            commands::get_avatar_by_id,
            commands::login,
            commands::verify_2fa,
            commands::get_auth_state,
            commands::logout,
            commands::wear_avatar,
            commands::get_avatar_details,
            commands::fetch_avatar_image,
            commands::get_avatar_thumbnail,
            commands::open_in_browser,
            commands::get_app_settings,
            commands::update_app_settings,
            commands::check_for_updates,
            commands::clear_image_cache,
            commands::toggle_favorite,
            commands::get_favorite_ids,
            commands::is_favorite,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
