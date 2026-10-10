use std::fs;
use std::path::PathBuf;
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

pub struct ConfigManager {
    file_path: PathBuf,
}

impl ConfigManager {
    pub fn new(app_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&app_dir);
        let file_path = app_dir.join("config.json");
        Self { file_path }
    }

    pub fn load(&self) -> AppConfig {
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

    pub fn save(&self, config: &AppConfig) -> Result<()> {
        let json = serde_json::to_string_pretty(config)?;
        fs::write(&self.file_path, json)?;
        Ok(())
    }

    pub fn set_auth(&self, auth_cookie: &str, user_id: &str, username: &str, remember_me: bool) -> Result<()> {
        let mut cfg = self.load();
        if remember_me {
            cfg.auth_cookie = auth_cookie.to_string();
            cfg.user_id = user_id.to_string();
            cfg.username = username.to_string();
            cfg.last_login = Some(Utc::now().to_rfc3339());
            cfg.remember_me = true;
        } else {
            cfg.auth_cookie = auth_cookie.to_string();
            cfg.user_id = user_id.to_string();
            cfg.username = username.to_string();
            cfg.last_login = Some(Utc::now().to_rfc3339());
            cfg.remember_me = false;
        }
        self.save(&cfg)
    }

    pub fn clear_auth(&self) -> Result<()> {
        let mut cfg = self.load();
        cfg.auth_cookie.clear();
        cfg.user_id.clear();
        cfg.username.clear();
        cfg.last_login = None;
        cfg.remember_me = false;
        self.save(&cfg)
    }

    pub fn set_show_old_images(&self, show: bool) -> Result<()> {
        let mut cfg = self.load();
        cfg.show_old_images = show;
        self.save(&cfg)
    }
}
