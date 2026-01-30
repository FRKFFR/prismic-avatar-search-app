mod database_downloader_gui;
mod login_gui;
mod avatar_browser_gui;

use database_downloader_gui::{AppState as DatabaseState, UiMessage as DatabaseMessage};
use login_gui::{AppState as LoginState, WorkerResponse as LoginMessage};
use avatar_browser_gui::{AvatarBrowserApp as BrowserState};

use eframe::egui;
use std::path::Path;

enum AppState {
    DatabaseDownloader(DatabaseState),
    Login(LoginState),
    AvatarBrowser(BrowserState),
}

#[derive(Debug, Clone)]
enum UiMessage {
    Database(DatabaseMessage),
    Login(LoginMessage),
}

struct UnifiedAvatarManager {
    state: AppState,
    rx: flume::Receiver<UiMessage>,
    tx: flume::Sender<UiMessage>,
    initialization_complete: bool,
}

impl UnifiedAvatarManager {
    fn new() -> Self {
        let (tx, rx) = flume::unbounded();
        
        // Check initialization requirements
        let (initial_state, init_complete) = Self::determine_initial_state();
        
        Self { 
            state: initial_state, 
            rx, 
            tx,
            initialization_complete: init_complete,
        }
    }
    
    fn determine_initial_state() -> (AppState, bool) {
        // Check if database files exist
        let cache_dir = Path::new("cache");
        let main_db_exists = cache_dir.join("main_database.json").exists();
        let aux_db_exists = cache_dir.join("aux_database.json").exists();
        
        // Check if config exists and has auth cookie
        let config_exists = Path::new("config.json").exists();
        let mut has_auth = false;
        
        if config_exists {
            if let Ok(content) = std::fs::read_to_string("config.json") {
                if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
                    has_auth = config.get("auth_cookie")
                        .and_then(|v| v.as_str())
                        .map(|s| !s.is_empty())
                        .unwrap_or(false);
                }
            }
        }
        
        // Determine what needs to be done first
        if !main_db_exists || !aux_db_exists {
            // Need to run database downloader first
            let (db_tx, db_rx) = flume::unbounded();
            let state = AppState::DatabaseDownloader(DatabaseState::new(db_rx));
            database_downloader_gui::run_worker(db_tx);
            (state, false)
        } else if !has_auth {
            // Need to login first
            let (login_tx, login_rx) = login_gui::spawn_worker();
            let state = AppState::Login(LoginState::new(login_tx, login_rx));
            (state, false)
        } else {
            // Everything is ready, start with avatar browser
            let state = AppState::AvatarBrowser(BrowserState::new());
            (state, true)
        }
    }
    
    fn check_and_transition(&mut self) {
        if self.initialization_complete {
            return;
        }
        
        match &self.state {
            AppState::DatabaseDownloader(state) => {
                // Check if database download is complete
                if state.is_complete() {
                    // Database done, move to login
                    let (login_tx, login_rx) = login_gui::spawn_worker();
                    self.state = AppState::Login(LoginState::new(login_tx, login_rx));
                }
            }
            AppState::Login(state) => {
                // Check if login is complete
                if state.is_logged_in() {
                    // Login done, move to avatar browser
                    self.state = AppState::AvatarBrowser(BrowserState::new());
                    self.initialization_complete = true;
                }
            }
            AppState::AvatarBrowser(_) => {
                self.initialization_complete = true;
            }
        }
    }
}

impl eframe::App for UnifiedAvatarManager {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for automatic transitions during initialization
        self.check_and_transition();
        
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("VRChat Unified Avatar Manager");
                ui.separator();
                
                let current_mode = match &self.state {
                    AppState::DatabaseDownloader(_) => "Database Downloader",
                    AppState::Login(_) => "Login", 
                    AppState::AvatarBrowser(_) => "Avatar Browser",
                };
                
                ui.label(format!("Current: {}", current_mode));
                
                // Show initialization status
                if !self.initialization_complete {
                    ui.separator();
                    ui.label("🔄 Initializing...");
                    ui.label("(Required setup in progress)");
                }
                
                ui.separator();
                
                // Only allow manual switching after initialization is complete
                if self.initialization_complete {
                    if ui.button("Database Downloader").clicked() {
                        let (db_tx, db_rx) = flume::unbounded();
                        self.state = AppState::DatabaseDownloader(DatabaseState::new(db_rx));
                        database_downloader_gui::run_worker(db_tx);
                    }
                    if ui.button("Login").clicked() {
                        let (login_tx, login_rx) = login_gui::spawn_worker();
                        self.state = AppState::Login(LoginState::new(login_tx, login_rx));
                    }
                    if ui.button("Avatar Browser").clicked() {
                        self.state = AppState::AvatarBrowser(BrowserState::new());
                    }
                } else {
                    // Show what's happening during initialization
                    match &self.state {
                        AppState::DatabaseDownloader(_) => {
                            ui.label("📥 Downloading avatar databases...");
                        }
                        AppState::Login(_) => {
                            ui.label("🔐 Please login to continue...");
                        }
                        AppState::AvatarBrowser(_) => {
                            ui.label("🎭 Loading avatar browser...");
                        }
                    }
                }
            });
        });
        
        egui::CentralPanel::default().show(ctx, |_ui| {
            match &mut self.state {
                AppState::DatabaseDownloader(state) => {
                    // Database downloader implements eframe::App directly
                    state.update(ctx, _frame);
                }
                AppState::Login(state) => {
                    // Login implements eframe::App directly
                    state.update(ctx, _frame);
                }
                AppState::AvatarBrowser(state) => {
                    // Avatar browser doesn't use the message system, render directly
                    state.update(ctx, _frame);
                }
            }
        });
    }
}

fn main() -> Result<(), eframe::Error> {
    env_logger::init();
    
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 600.0]),
        ..Default::default()
    };
    
    eframe::run_native(
        "VRChat Unified Avatar Manager",
        options,
        Box::new(|_cc| Box::new(UnifiedAvatarManager::new())),
    )
}
