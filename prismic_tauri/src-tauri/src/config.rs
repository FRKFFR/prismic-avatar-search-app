use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub auth_cookie: String,
    pub user_id: String,
    pub username: String,
    pub last_login: Option<String>,
    pub remember_me: bool,
    #[serde(default)]
    pub show_old_images: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            auth_cookie: String::new(),
            user_id: String::new(),
            username: String::new(),
            last_login: None,
            remember_me: false,
            show_old_images: false,
        }
    }
}

struct SessionAuth {
    auth_cookie: String,
    user_id: String,
    username: String,
}

pub struct ConfigManager {
    file_path: PathBuf,
    /// Auth for "remember me = off" logins; never written to disk.
    session_auth: Mutex<Option<SessionAuth>>,
}

impl ConfigManager {
    pub fn new(app_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&app_dir);
        let file_path = app_dir.join("config.json");
        Self {
            file_path,
            session_auth: Mutex::new(None),
        }
    }

    /// Config as persisted on disk (no in-memory session overlay).
    fn load_file(&self) -> AppConfig {
        if !self.file_path.exists() {
            let default_cfg = AppConfig::default();
            let _ = self.save(&default_cfg);
            return default_cfg;
        }

        match fs::read_to_string(&self.file_path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => AppConfig::default(),
        }
    }

    pub fn load(&self) -> AppConfig {
        let mut cfg = self.load_file();
        if let Ok(guard) = self.session_auth.lock() {
            if let Some(auth) = guard.as_ref() {
                cfg.auth_cookie = auth.auth_cookie.clone();
                cfg.user_id = auth.user_id.clone();
                cfg.username = auth.username.clone();
            }
        }
        cfg
    }

    pub fn save(&self, config: &AppConfig) -> Result<()> {
        let json = serde_json::to_string_pretty(config)?;
        fs::write(&self.file_path, json)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&self.file_path, fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    pub fn set_auth(&self, auth_cookie: &str, user_id: &str, username: &str, remember_me: bool) -> Result<()> {
        let mut cfg = self.load_file();
        if remember_me {
            if let Ok(mut session) = self.session_auth.lock() {
                *session = None;
            }
            cfg.auth_cookie = auth_cookie.to_string();
            cfg.user_id = user_id.to_string();
            cfg.username = username.to_string();
            cfg.last_login = Some(Utc::now().to_rfc3339());
            cfg.remember_me = true;
        } else {
            // Session-only login: keep credentials in memory, never on disk.
            if let Ok(mut session) = self.session_auth.lock() {
                *session = Some(SessionAuth {
                    auth_cookie: auth_cookie.to_string(),
                    user_id: user_id.to_string(),
                    username: username.to_string(),
                });
            }
            cfg.auth_cookie.clear();
            cfg.user_id.clear();
            cfg.username.clear();
            cfg.last_login = None;
            cfg.remember_me = false;
        }
        self.save(&cfg)
    }

    pub fn clear_auth(&self) -> Result<()> {
        if let Ok(mut session) = self.session_auth.lock() {
            *session = None;
        }
        let mut cfg = self.load_file();
        cfg.auth_cookie.clear();
        cfg.user_id.clear();
        cfg.username.clear();
        cfg.last_login = None;
        cfg.remember_me = false;
        self.save(&cfg)
    }

    pub fn set_show_old_images(&self, show: bool) -> Result<()> {
        let mut cfg = self.load_file();
        cfg.show_old_images = show;
        self.save(&cfg)
    }
}
