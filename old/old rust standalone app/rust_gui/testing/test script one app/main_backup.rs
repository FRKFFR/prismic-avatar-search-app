use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs::{self, File},
    io::{BufRead, BufReader, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use anyhow::{anyhow, Context, Result};
use eframe::egui::{self, ColorImage, Image, Vec2};
use flume::{Receiver, Sender};
use image::ImageReader;
use open::that as open_url;
use regex::Regex;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

const API_BASE: &str = "https://api.vrchat.cloud/api/1";
const CONFIG_FILE: &str = "config.json";
const AVATAR_STREAM: &str = "cache/avatar_entries.ndjson";
const MAX_PENDING: usize = 48;
const MAX_TEXTURE_CACHE: usize = 256;
const PAGE_SIZE: usize = 200;
const USER_AGENT: &str = "VRChatAvatarBrowser/0.1";
const CARD_WIDTH: f32 = 240.0;
const CARD_GAP: f32 = 12.0;
const CARD_IMAGE_WIDTH: f32 = 140.0;
const CARD_IMAGE_HEIGHT: f32 = 105.0;
const WORKER_COUNT: usize = 4;

const URLS: [&str; 3] = [
    "https://gist.githubusercontent.com/Mwr247/a80c1f9060fc4fd46a8f00d589c47c5a/raw/pasavtrdb.txt",
    "https://gist.githubusercontent.com/Mwr247/a80c1f9060fc4fd46a8f00d589c47c5a/raw/pasavtrdb_qst.txt",
    "https://gist.githubusercontent.com/Mwr247/a80c1f9060fc4fd46a8f00d589c47c5a/raw/pasavtrdb_ios.txt",
];

const BACKUP_URLS: [&str; 3] = [
    "https://prismic.net/vrc/pasavtrdb.txt",
    "https://prismic.net/vrc/pasavtrdb_qst.txt",
    "https://prismic.net/vrc/pasavtrdb_ios.txt",
];

const STATIC_BYTES: [u8; 16] = [208, 29, 107, 36, 251, 69, 122, 14, 67, 204, 171, 246, 106, 38, 183, 224];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AvatarEntry {
    #[serde(default)]
    name: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    description: String,
    #[serde(default = "default_avatar_id")]
    avatar_id: String,
    #[serde(default)]
    platforms: Vec<String>,
    #[serde(default)]
    quest: bool,
    #[serde(default)]
    ios: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AvatarData {
    avatar_count: u32,
    author_count: u32,
    last_update: String,
    entries: Vec<AvatarEntry>,
}

struct ProcessedData {
    data: AvatarData,
    index: HashMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Config {
    #[serde(default)]
    auth_cookie: String,
    #[serde(default)]
    user_id: String,
    #[serde(default)]
    remember_me: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DatabaseInfo {
    url: String,
    platform: String,
    total_avatars: usize,
    downloaded: usize,
    status: String,
}

#[derive(Debug)]
enum AppState {
    CheckingFiles,
    DatabaseDownloader,
    Login,
    AvatarBrowser,
}

#[derive(Debug)]
enum DatabaseRequest {
    StartDownload,
    Cancel,
}

#[derive(Debug)]
enum DatabaseResponse {
    Progress {
        platform: String,
        downloaded: usize,
        total: usize,
        status: String,
    },
    Complete,
    Error(String),
}

#[derive(Debug, Clone)]
struct TwoFactorState {
    auth_cookie: String,
    two_factor_type: String,
    remember_me: bool,
}

#[derive(Debug, Clone)]
enum LoginRequest {
    Login {
        username: String,
        password: String,
        remember_me: bool,
    },
    VerifyTwoFactor {
        code: String,
        auth_cookie: String,
        two_factor_type: String,
        remember_me: bool,
    },
    CheckStored,
}

#[derive(Debug, Clone)]
enum LoginResponse {
    TwoFactorRequired {
        auth_cookie: String,
        two_factor_type: String,
        remember_me: bool,
    },
    Success {
        auth_cookie: String,
        user_id: String,
    },
    Error(String),
}

#[derive(Debug)]
enum WorkerRequest {
    FetchAvatar { avatar_id: String },
    FetchCurrentAvatar,
    SelectAvatar { avatar_id: String },
}

#[derive(Debug)]
enum WorkerResponse {
    AvatarImage { 
        avatar_id: String, 
        image: Option<Vec<u8>>,
        status: Option<String>,
    },
    CurrentAvatar {
        avatar_id: String,
        name: String,
        image: Option<Vec<u8>>,
    },
    SelectResult {
        avatar_id: String,
        success: bool,
        message: String,
    },
    Error(String),
}

struct AvatarDetail {
    image: Option<egui::TextureHandle>,
    status: Option<String>,
}

struct UnifiedApp {
    state: AppState,
    
    // Database downloader state
    databases: Vec<DatabaseInfo>,
    db_sender: Option<Sender<DatabaseRequest>>,
    db_receiver: Option<Receiver<DatabaseResponse>>,
    db_downloading: bool,
    db_progress: HashMap<String, (usize, usize, String)>,
    
    // Login state
    login_username: String,
    login_password: String,
    login_remember_me: bool,
    login_status: String,
    login_loading: bool,
    login_sender: Option<Sender<LoginRequest>>,
    login_receiver: Option<Receiver<LoginResponse>>,
    two_factor_code: String,
    two_factor_state: Option<TwoFactorState>,
    
    // Avatar browser state
    avatars: Vec<AvatarEntry>,
    filtered: Vec<usize>,
    search_name: String,
    search_author: String,
    search_description: String,
    search_name_lower: String,
    search_author_lower: String,
    search_description_lower: String,
    filter_pc: bool,
    filter_quest: bool,
    filter_ios: bool,
    details: HashMap<String, AvatarDetail>,
    pending: HashSet<String>,
    sender: Option<Sender<WorkerRequest>>,
    receiver: Option<Receiver<WorkerResponse>>,
    status: String,
    texture_order: VecDeque<String>,
    stream_offset: u64,
    page: usize,
    total_avatars: usize,
    current_avatar_id: Option<String>,
    current_avatar_name: Option<String>,
    current_avatar_image: Option<egui::TextureHandle>,
    selection_message: Option<String>,
    show_debug: bool,
    debug_log: Vec<String>,
}

impl UnifiedApp {
    fn new() -> Self {
        let mut app = Self {
            state: AppState::CheckingFiles,
            
            databases: vec![
                DatabaseInfo {
                    url: URLS[0].to_string(),
                    platform: "PC".to_string(),
                    total_avatars: 0,
                    downloaded: 0,
                    status: "Pending".to_string(),
                },
                DatabaseInfo {
                    platform: "Quest".to_string(),
                    url: URLS[1].to_string(),
                    total_avatars: 0,
                    downloaded: 0,
                    status: "Pending".to_string(),
                },
                DatabaseInfo {
                    platform: "iOS".to_string(),
                    url: URLS[2].to_string(),
                    total_avatars: 0,
                    downloaded: 0,
                    status: "Pending".to_string(),
                },
            ],
            db_sender: None,
            db_receiver: None,
            db_downloading: false,
            db_progress: HashMap::new(),
            
            login_username: String::new(),
            login_password: String::new(),
            login_remember_me: false,
            login_status: "Enter your VRChat credentials".to_string(),
            login_loading: false,
            login_sender: None,
            login_receiver: None,
            two_factor_code: String::new(),
            two_factor_state: None,
            
            avatars: Vec::new(),
            filtered: Vec::new(),
            search_name: String::new(),
            search_author: String::new(),
            search_description: String::new(),
            search_name_lower: String::new(),
            search_author_lower: String::new(),
            search_description_lower: String::new(),
            filter_pc: false,
            filter_quest: false,
            filter_ios: false,
            details: HashMap::new(),
            pending: HashSet::new(),
            sender: None,
            receiver: None,
            status: "Loading...".into(),
            texture_order: VecDeque::new(),
            stream_offset: 0,
            page: 0,
            total_avatars: 0,
            current_avatar_id: None,
            current_avatar_name: None,
            current_avatar_image: None,
            selection_message: None,
            show_debug: false,
            debug_log: Vec::new(),
        };
        app.log("Unified VRChat Avatar Manager initialized");
        app
    }
    
    fn log(&mut self, message: &str) {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let seconds = timestamp / 1000;
        let millis = timestamp % 1000;
        let time_str = format!("{:02}:{:02}:{:02}.{:03}", 
            (seconds / 3600) % 24,
            (seconds / 60) % 60,
            seconds % 60,
            millis
        );
        self.debug_log.push(format!("[{}] {}", time_str, message));
        
        if self.debug_log.len() > 1000 {
            self.debug_log.remove(0);
        }
    }
    
    fn check_files(&mut self) {
        self.log("Checking required files...");
        
        if !Path::new(AVATAR_STREAM).exists() {
            self.log("Database file not found, starting database downloader");
            self.state = AppState::DatabaseDownloader;
            self.start_database_download();
            return;
        }
        
        let config = load_config();
        if config.auth_cookie.is_empty() {
            self.log("No auth cookie found, starting login");
            self.state = AppState::Login;
            self.start_login();
            return;
        }
        
        self.log("All files found, starting avatar browser");
        self.state = AppState::AvatarBrowser;
        self.start_avatar_browser(&config.auth_cookie);
    }
    
    fn start_database_download(&mut self) {
        self.log("Starting database download worker");
        let (tx, _rx) = flume::unbounded();
        self.db_sender = Some(tx);
        // Create response channel for database progress
        let (response_tx, response_rx) = flume::unbounded();
        self.db_receiver = Some(response_rx);
        self.db_downloading = true;
        
        let databases = self.databases.clone();
        
        thread::spawn(move || {
            Self::download_databases(databases, response_tx);
        });
    }
    
    fn download_databases(_databases: Vec<DatabaseInfo>, response_sender: Sender<DatabaseResponse>) {
        // Send initial log
        let _ = response_sender.send(DatabaseResponse::Progress {
            platform: "Init".to_string(),
            downloaded: 0,
            total: 0,
            status: "Starting database download...".to_string(),
        });
        
        if let Err(_e) = fs::create_dir_all("cache") {
            let _ = response_sender.send(DatabaseResponse::Error("Failed to create cache directory".to_string()));
            return;
        }
        
        // Create decoded directory for examination
        if let Err(_e) = fs::create_dir_all("cache/decoded") {
            let _ = response_sender.send(DatabaseResponse::Error("Failed to create cache/decoded directory".to_string()));
            return;
        }
        
        let client = match Client::builder()
            .timeout(Duration::from_secs(120))  // Increased from 30 to 120 seconds
            .user_agent(USER_AGENT)
            .build() {
                Ok(client) => client,
                Err(e) => {
                    let _ = response_sender.send(DatabaseResponse::Error(format!("Failed to create HTTP client: {}", e)));
                    return;
                }
            };
        
        // Download main database
        let _ = response_sender.send(DatabaseResponse::Progress {
            platform: "PC".to_string(),
            downloaded: 0,
            total: 0,
            status: "Downloading main database...".to_string(),
        });
        
        eprintln!("DEBUG: Starting main database download");
        let main_bytes = match download_with_fallback(&client, 0) {
            Ok(bytes) => {
                eprintln!("DEBUG: Main database downloaded: {} bytes", bytes.len());
                eprintln!("DEBUG: First 50 bytes: {:?}", &bytes[..bytes.len().min(50)]);
                
                // Save raw file for examination
                if let Err(e) = fs::write("cache/decoded/main_raw.pas", &bytes) {
                    eprintln!("DEBUG: Failed to save main raw file: {}", e);
                } else {
                    eprintln!("DEBUG: Saved main raw file to cache/decoded/main_raw.pas");
                }
                
                let _ = response_sender.send(DatabaseResponse::Progress {
                    platform: "PC".to_string(),
                    downloaded: bytes.len(),
                    total: bytes.len(),
                    status: format!("Main database downloaded ({} bytes)", bytes.len()),
                });
                bytes
            },
            Err(e) => {
                eprintln!("DEBUG: Failed to download main database: {}", e);
                let _ = response_sender.send(DatabaseResponse::Error(format!("Failed to download main database: {}", e)));
                return;
            }
        };
        eprintln!("DEBUG: Main download complete, proceeding to quest");
        
        // Download quest database
        let _ = response_sender.send(DatabaseResponse::Progress {
            platform: "Quest".to_string(),
            downloaded: 0,
            total: 0,
            status: "Downloading quest database...".to_string(),
        });
        
        eprintln!("DEBUG: Starting quest database download");
        let quest_bytes = match download_with_fallback(&client, 1) {
            Ok(bytes) => {
                eprintln!("DEBUG: Quest database downloaded: {} bytes", bytes.len());
                
                // Save raw file for examination
                if let Err(e) = fs::write("cache/decoded/quest_raw.pas", &bytes) {
                    eprintln!("DEBUG: Failed to save quest raw file: {}", e);
                } else {
                    eprintln!("DEBUG: Saved quest raw file to cache/decoded/quest_raw.pas");
                }
                
                let _ = response_sender.send(DatabaseResponse::Progress {
                    platform: "Quest".to_string(),
                    downloaded: bytes.len(),
                    total: bytes.len(),
                    status: format!("Quest database downloaded ({} bytes)", bytes.len()),
                });
                bytes
            },
            Err(e) => {
                eprintln!("DEBUG: Failed to download quest database: {}", e);
                let _ = response_sender.send(DatabaseResponse::Error(format!("Failed to download quest database: {}", e)));
                return;
            }
        };
        eprintln!("DEBUG: Quest download complete, proceeding to iOS");
        
        // Download iOS database
        let _ = response_sender.send(DatabaseResponse::Progress {
            platform: "iOS".to_string(),
            downloaded: 0,
            total: 0,
            status: "Downloading iOS database...".to_string(),
        });
        
        eprintln!("DEBUG: Starting iOS database download");
        let ios_bytes = match download_with_fallback(&client, 2) {
            Ok(bytes) => {
                eprintln!("DEBUG: iOS database downloaded: {} bytes", bytes.len());
                
                // Save raw file for examination
                if let Err(e) = fs::write("cache/decoded/ios_raw.pas", &bytes) {
                    eprintln!("DEBUG: Failed to save iOS raw file: {}", e);
                } else {
                    eprintln!("DEBUG: Saved iOS raw file to cache/decoded/ios_raw.pas");
                }
                
                let _ = response_sender.send(DatabaseResponse::Progress {
                    platform: "iOS".to_string(),
                    downloaded: bytes.len(),
                    total: bytes.len(),
                    status: format!("iOS database downloaded ({} bytes)", bytes.len()),
                });
                bytes
            },
            Err(e) => {
                eprintln!("DEBUG: Failed to download iOS database: {}", e);
                let _ = response_sender.send(DatabaseResponse::Error(format!("Failed to download iOS database: {}", e)));
                return;
            }
        };
        eprintln!("DEBUG: iOS download complete, proceeding to processing");
        
        // Process databases
        let _ = response_sender.send(DatabaseResponse::Progress {
            platform: "Processing".to_string(),
            downloaded: 0,
            total: 0,
            status: "Processing databases...".to_string(),
        });
        
        eprintln!("DEBUG: Starting database processing...");
        match Self::process_databases(main_bytes, quest_bytes, ios_bytes) {
            Ok(_total_count) => {
                eprintln!("DEBUG: Successfully processed databases");
                let _ = response_sender.send(DatabaseResponse::Progress {
                    platform: "Complete".to_string(),
                    downloaded: _total_count,
                    total: _total_count,
                    status: format!("Processed {} avatars", _total_count),
                });
                let _ = response_sender.send(DatabaseResponse::Complete);
            }
            Err(e) => {
                eprintln!("DEBUG: Failed to process databases: {}", e);
                let _ = response_sender.send(DatabaseResponse::Error(format!("Failed to process databases: {}", e)));
            }
        }
    }
    
    fn process_databases(main_bytes: Vec<u8>, quest_bytes: Vec<u8>, ios_bytes: Vec<u8>) -> Result<usize> {
        eprintln!("DEBUG: process_databases called");
        eprintln!("DEBUG: main_bytes len: {}, quest_bytes len: {}, ios_bytes len: {}", 
                 main_bytes.len(), quest_bytes.len(), ios_bytes.len());
        
        // Parse main database
        let mut processed = match parse_main_database(main_bytes) {
            Ok(processed) => {
                eprintln!("Parsed main database: {} entries", processed.data.entries.len());
                processed
            },
            Err(e) => {
                eprintln!("Failed to parse main database: {}", e);
                return Err(e);
            }
        };
        
        // Parse quest database and mark platforms
        eprintln!("Starting quest database parsing...");
        let quest_ids = match parse_aux_database(quest_bytes) {
            Ok(ids) => {
                eprintln!("Parsed quest database: {} IDs", ids.len());
                ids
            },
            Err(e) => {
                eprintln!("Failed to parse quest database: {}", e);
                return Err(e);
            }
        };
        eprintln!("Marking quest platforms...");
        mark_platform(&mut processed, &quest_ids, true, false);
        eprintln!("Finished marking quest platforms");
        
        // Parse iOS database and mark platforms
        eprintln!("Starting iOS database parsing...");
        let ios_ids = match parse_aux_database(ios_bytes) {
            Ok(ids) => {
                eprintln!("Parsed iOS database: {} IDs", ids.len());
                ids
            },
            Err(e) => {
                eprintln!("Failed to parse iOS database: {}", e);
                return Err(e);
            }
        };
        eprintln!("Marking iOS platforms...");
        mark_platform(&mut processed, &ios_ids, false, true);
        eprintln!("Finished marking iOS platforms");
        
        // Stream to file
        let output_path = PathBuf::from(AVATAR_STREAM);
        eprintln!("Creating output file: {:?}", output_path);
        
        let mut file = match fs::File::create(&output_path) {
            Ok(file) => {
                eprintln!("Created output file successfully");
                file
            },
            Err(e) => {
                eprintln!("Failed to create output file: {}", e);
                return Err(anyhow!("Failed to create output file: {}", e));
            }
        };
        
        let total_count = processed.data.entries.len();
        eprintln!("Writing {} entries to file", total_count);
        
        for (i, entry) in processed.data.entries.iter().enumerate() {
            if i % 10000 == 0 {
                eprintln!("Written {} entries...", i);
            }
            let json_str = serde_json::to_string(entry)?;
            writeln!(file, "{}", json_str)?;
        }
        
        eprintln!("Finished writing {} entries", total_count);
        Ok(total_count)
    }
    
    fn start_login(&mut self) {
        self.log("Starting login worker");
        let (tx, _rx) = flume::unbounded();
        self.login_sender = Some(tx);
        // Create response channel for login results
        let (_response_tx, response_rx) = flume::unbounded();
        self.login_receiver = Some(response_rx);
    }
    
    fn attempt_login(&mut self) {
        if self.login_username.is_empty() || self.login_password.is_empty() {
            self.login_status = "Please enter username and password".to_string();
            return;
        }
        
        self.login_loading = true;
        self.login_status = "Logging in...".to_string();
        
        let username = self.login_username.clone();
        let password = self.login_password.clone();
        let remember_me = self.login_remember_me;
        
        // We need to create a new channel for this login attempt
        let (response_tx, response_rx) = flume::unbounded();
        self.login_receiver = Some(response_rx);
        let response_sender = response_tx;
        
        thread::spawn(move || {
            match Self::perform_login(&username, &password) {
                Ok(LoginResponse::TwoFactorRequired { auth_cookie, two_factor_type, .. }) => {
                    let _ = response_sender.send(LoginResponse::TwoFactorRequired {
                        auth_cookie,
                        two_factor_type,
                        remember_me,
                    });
                }
                Ok(LoginResponse::Success { auth_cookie, user_id }) => {
                    if remember_me {
                        let config = Config {
                            auth_cookie: auth_cookie.clone(),
                            user_id: user_id.clone(),
                            remember_me: true,
                        };
                        if let Ok(json) = serde_json::to_string_pretty(&config) {
                            let _ = fs::write(CONFIG_FILE, json);
                        }
                    }
                    
                    let _ = response_sender.send(LoginResponse::Success { auth_cookie, user_id });
                }
                Ok(LoginResponse::Error(err)) => {
                    let _ = response_sender.send(LoginResponse::Error(err));
                }
                Err(e) => {
                    let _ = response_sender.send(LoginResponse::Error(e.to_string()));
                }
            }
        });
    }
    
    fn verify_two_factor_code(&mut self) {
        if let Some(ref state) = self.two_factor_state {
            if self.two_factor_code.trim().is_empty() {
                self.login_status = "Please enter the 2FA code".to_string();
                return;
            }
            
            self.login_loading = true;
            self.login_status = "Verifying 2FA code...".to_string();
            
            let auth_cookie = state.auth_cookie.clone();
            let two_factor_type = state.two_factor_type.clone();
            let remember_me = state.remember_me;
            let code = self.two_factor_code.clone();
            
            // Create new channel for 2FA verification
            let (response_tx, response_rx) = flume::unbounded();
            self.login_receiver = Some(response_rx);
            let response_sender = response_tx;
            
            thread::spawn(move || {
                match Self::verify_two_factor(&auth_cookie, &code, &two_factor_type) {
                    Ok((auth_cookie, user_id)) => {
                        if remember_me {
                            let config = Config {
                                auth_cookie: auth_cookie.clone(),
                                user_id: user_id.clone(),
                                remember_me: true,
                            };
                            if let Ok(json) = serde_json::to_string_pretty(&config) {
                                let _ = fs::write(CONFIG_FILE, json);
                            }
                        }
                        
                        let _ = response_sender.send(LoginResponse::Success { auth_cookie, user_id });
                    }
                    Err(e) => {
                        let _ = response_sender.send(LoginResponse::Error(e.to_string()));
                    }
                }
            });
        }
    }
    
    fn perform_login(username: &str, password: &str) -> Result<LoginResponse> {
        let client = Client::builder()
            .cookie_store(true)
            .timeout(Duration::from_secs(30))
            .user_agent(USER_AGENT)
            .build()?;
        
        // Use API-based authentication instead of web scraping
        let response = client
            .get(format!("{}/auth/user", API_BASE))
            .basic_auth(username, Some(password))
            .send()?;
        
        if !response.status().is_success() {
            return Err(anyhow!("Login failed: {}", response.status()));
        }
        
        let auth_cookie = response
            .cookies()
            .find(|cookie| cookie.name() == "auth")
            .map(|cookie| cookie.value().to_string())
            .ok_or_else(|| anyhow!("Auth cookie not found in response"))?;
        
        let payload: serde_json::Value = response.json()?;
        
        // Check if 2FA is required
        if let Some(reqs) = payload
            .get("requiresTwoFactorAuth")
            .and_then(|v| v.as_array()) {
            if let Some(two_factor_type) = reqs.first().and_then(|v| v.as_str()) {
                return Ok(LoginResponse::TwoFactorRequired {
                    auth_cookie,
                    two_factor_type: two_factor_type.to_string(),
                    remember_me: false, // Will be set by caller
                });
            }
        }
        
        let user_id = payload
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("No user ID found"))?
            .to_string();
        
        Ok(LoginResponse::Success {
            auth_cookie,
            user_id,
        })
    }
    
    fn verify_two_factor(auth_cookie: &str, code: &str, two_factor_type: &str) -> Result<(String, String)> {
        let client = Client::builder()
            .cookie_store(true)
            .timeout(Duration::from_secs(30))
            .user_agent(USER_AGENT)
            .build()?;
        
        if code.trim().is_empty() {
            return Err(anyhow!("2FA code cannot be empty"));
        }
        
        let endpoint = match two_factor_type {
            "emailOtp" => format!("{}/auth/twofactorauth/emailotp/verify", API_BASE),
            _ => format!("{}/auth/twofactorauth/totp/verify", API_BASE),
        };
        
        let verify_resp = client
            .post(endpoint)
            .header("Cookie", format!("auth={}", auth_cookie))
            .json(&serde_json::json!({ "code": code }))
            .send()?;
        
        if !verify_resp.status().is_success() {
            return Err(anyhow!("Invalid two-factor code"));
        }
        
        let payload: serde_json::Value = verify_resp.json()?;
        let verified = payload
            .get("verified")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        
        if !verified {
            return Err(anyhow!("Two-factor verification failed"));
        }
        
        // Get user info with verified session
        let user_resp = client
            .get(format!("{}/auth/user", API_BASE))
            .header("Cookie", format!("auth={}", auth_cookie))
            .send()?;
        
        if !user_resp.status().is_success() {
            return Err(anyhow!("Failed to verify session: {}", user_resp.status()));
        }
        
        let user_payload: serde_json::Value = user_resp.json()?;
        let user_id = user_payload
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("No user ID found"))?
            .to_string();
        
        Ok((auth_cookie.to_string(), user_id))
    }
    
    fn start_avatar_browser(&mut self, auth_cookie: &str) {
        self.log("Starting avatar browser");
        let (tx, rx) = spawn_worker(auth_cookie.to_string());
        self.sender = Some(tx);
        self.receiver = Some(rx);
        
        if let Err(err) = self.load_page(true) {
            self.log(&format!("Initialization error: {}", err));
        }
        
        if self.sender.is_some() {
            self.request_current_avatar();
        } else {
            self.status = "Login required to view images and wear avatars.".into();
        }
    }
    
    fn combine_database_files(&self) -> Result<()> {
        let platforms = ["pc", "quest", "ios"];
        let mut output_file = fs::File::create(AVATAR_STREAM)?;
        
        for platform in &platforms {
            let input_path = format!("cache/avatar_entries_{}.ndjson", platform);
            if Path::new(&input_path).exists() {
                let content = fs::read_to_string(&input_path)?;
                output_file.write_all(content.as_bytes())?;
            }
        }
        
        Ok(())
    }
    
    fn poll_workers(&mut self, ctx: &egui::Context) {
        // Collect database responses first
        let db_responses: Vec<DatabaseResponse> = if let Some(receiver) = &self.db_receiver {
            let mut responses = Vec::new();
            while let Ok(response) = receiver.try_recv() {
                responses.push(response);
            }
            responses
        } else {
            Vec::new()
        };
        
        // Process database responses
        for response in db_responses {
            match response {
                DatabaseResponse::Progress { platform, downloaded, total, status } => {
                    self.db_progress.insert(platform, (downloaded, total, status));
                }
                DatabaseResponse::Complete => {
                    self.db_downloading = false;
                    self.log("Database download complete");
                    self.log("Checking for authentication tokens...");
                    let config = load_config();
                    if config.auth_cookie.is_empty() {
                        self.log("No auth cookie found, starting login");
                        self.state = AppState::Login;
                        self.start_login();
                    } else {
                        self.log("Auth cookie found, starting avatar browser");
                        self.state = AppState::AvatarBrowser;
                        self.start_avatar_browser(&config.auth_cookie);
                    }
                }
                DatabaseResponse::Error(err) => {
                    self.db_downloading = false;
                    self.log(&format!("Database download error: {}", err));
                }
            }
        }
        
        // Collect login responses
        let login_responses: Vec<LoginResponse> = if let Some(receiver) = &self.login_receiver {
            let mut responses = Vec::new();
            while let Ok(response) = receiver.try_recv() {
                responses.push(response);
            }
            responses
        } else {
            Vec::new()
        };
        
        // Process login responses
        for response in login_responses {
            match response {
                LoginResponse::TwoFactorRequired { auth_cookie, two_factor_type, remember_me } => {
                    self.login_loading = false;
                    self.two_factor_state = Some(TwoFactorState {
                        auth_cookie,
                        two_factor_type: two_factor_type.clone(),
                        remember_me,
                    });
                    self.two_factor_code.clear();
                    self.login_status = format!("Two-factor authentication required ({})", two_factor_type);
                }
                LoginResponse::Success { auth_cookie, user_id } => {
                    self.login_loading = false;
                    self.login_status = format!("Logged in as {}", user_id);
                    self.two_factor_state = None;
                    self.two_factor_code.clear();
                    self.login_password.clear();
                    self.log(&format!("Login successful: {}", user_id));
                    self.state = AppState::AvatarBrowser;
                    self.start_avatar_browser(&auth_cookie);
                }
                LoginResponse::Error(err) => {
                    self.login_loading = false;
                    self.login_status = "Login failed".to_string();
                    self.log(&format!("Login error: {}", err));
                }
            }
        }
        
        // Collect avatar worker responses
        let avatar_responses: Vec<WorkerResponse> = if let Some(receiver) = &self.receiver {
            receiver.try_iter().collect()
        } else {
            Vec::new()
        };
        
        // Process avatar worker responses
        for response in avatar_responses {
            match response {
                WorkerResponse::AvatarImage { avatar_id, image, status } => {
                    self.pending.remove(&avatar_id);
                    if let Some(bytes) = image {
                        match texture_from_bytes(ctx, &avatar_id, &bytes) {
                            Ok(texture) => {
                                self.log(&format!("Successfully loaded image for avatar {}", avatar_id));
                                self.details.insert(
                                    avatar_id.clone(),
                                    AvatarDetail {
                                        image: Some(texture),
                                        status: status,
                                    },
                                );
                                self.texture_order.push_back(avatar_id);
                                self.prune_texture_cache();
                            }
                            Err(_) => {
                                let status_msg = status.unwrap_or_else(|| "Unavailable".to_string());
                                self.details.insert(
                                    avatar_id.clone(),
                                    AvatarDetail {
                                        image: None,
                                        status: Some(status_msg),
                                    },
                                );
                                self.texture_order.push_back(avatar_id);
                                self.prune_texture_cache();
                            }
                        }
                    } else {
                        let status_msg = status.unwrap_or_else(|| "Unavailable".to_string());
                        self.details.insert(
                            avatar_id.clone(),
                            AvatarDetail {
                                image: None,
                                status: Some(status_msg),
                            },
                        );
                    }
                }
                WorkerResponse::CurrentAvatar { avatar_id, name, image } => {
                    self.current_avatar_id = Some(avatar_id.clone());
                    self.current_avatar_name = Some(name.clone());
                    if let Some(bytes) = image {
                        if let Ok(texture) = texture_from_bytes(ctx, &avatar_id, &bytes) {
                            self.current_avatar_image = Some(texture);
                        }
                    }
                }
                WorkerResponse::SelectResult { avatar_id, success, message } => {
                    if success {
                        self.current_avatar_id = Some(avatar_id);
                        self.request_current_avatar();
                    }
                    self.selection_message = Some(message);
                }
                WorkerResponse::Error(_) => {}
            }
        }
        
        ctx.request_repaint();
    }
    
    fn request_current_avatar(&self) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(WorkerRequest::FetchCurrentAvatar);
        }
    }

    fn request_avatar(&mut self, avatar_id: &str) {
        if self.pending.len() >= MAX_PENDING {
            return;
        }
        if self.pending.contains(avatar_id) {
            return;
        }
        if let Some(detail) = self.details.get(avatar_id) {
            if detail.image.is_some() || detail.status.as_ref().map_or(false, |s| {
                s == "Unavailable" || s == "Decode Failed" || s == "Deleted" || s == "No Image"
            }) {
                return;
            }
        }
        if let Some(sender) = &self.sender {
            if sender
                .send(WorkerRequest::FetchAvatar {
                    avatar_id: avatar_id.to_string(),
                })
                .is_ok()
            {
                self.pending.insert(avatar_id.to_string());
                self.log(&format!("Requested image for avatar {}", avatar_id));
            }
        }
    }
    
    fn reset_stream(&mut self) {
        self.stream_offset = 0;
        self.page = 0;
        self.total_avatars = 0;
        self.avatars.clear();
        self.filtered.clear();
        
        if let Ok(file) = fs::File::open(AVATAR_STREAM) {
            let reader = BufReader::new(file);
            for line in reader.lines() {
                let line = line.unwrap_or_default();
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(mut entry) = serde_json::from_str::<AvatarEntry>(trimmed) {
                    normalize_platforms(&mut entry);
                    if self.matches_query(&entry) {
                        self.total_avatars += 1;
                    }
                }
            }
        }
        
        self.log(&format!("Found {} matching avatars out of total database", self.total_avatars));
    }
    
    fn prune_texture_cache(&mut self) {
        while self.texture_order.len() > MAX_TEXTURE_CACHE {
            if let Some(old_id) = self.texture_order.pop_front() {
                if let Some(detail) = self.details.get(&old_id) {
                    if detail.image.is_some() {
                        self.details.remove(&old_id);
                        continue;
                    }
                }
                self.details.remove(&old_id);
            }
        }
    }
    
    fn update_query_terms(&mut self) {
        self.search_name_lower = self.search_name.to_lowercase();
        self.search_author_lower = self.search_author.to_lowercase();
        self.search_description_lower = self.search_description.to_lowercase();
    }

    fn load_page(&mut self, reset: bool) -> Result<()> {
        if reset {
            self.stream_offset = 0;
            self.page = 0;
            self.total_avatars = 0;
            self.avatars.clear();
            self.filtered.clear();
            self.details.clear();
            self.pending.clear();
        }

        // Always clear avatars for true pagination
        self.avatars.clear();
        self.filtered.clear();

        let loaded = self.load_next_page()?;
        self.filtered = (0..self.avatars.len()).collect();

        let total_pages = (self.total_avatars + PAGE_SIZE - 1) / PAGE_SIZE;
        if loaded == 0 {
            self.status = "No avatars match your filters.".into();
        } else {
            self.status = format!(
                "Page {} of {} ({} avatars)",
                self.page + 1,
                total_pages,
                self.avatars.len()
            );
        }

        Ok(())
    }

    fn load_next_page(&mut self) -> Result<usize> {
        let mut file = File::open(AVATAR_STREAM)
            .with_context(|| format!("Failed to open {}. Run the downloader to generate it.", AVATAR_STREAM))?;
        
        // Start from beginning for each page load
        file.seek(SeekFrom::Start(0))?;
        let mut reader = BufReader::new(file);
        
        let mut loaded = 0usize;
        let mut total_checked = 0usize;
        let mut matching_found = 0usize;

        // Read through the file, counting matching avatars and loading when we reach our page
        while loaded < PAGE_SIZE {
            let mut line = String::new();
            let bytes = reader.read_line(&mut line)?;
            if bytes == 0 {
                break;
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let mut entry: AvatarEntry = serde_json::from_str(trimmed)?;
            normalize_platforms(&mut entry);
            total_checked += 1;
            
            if self.matches_query(&entry) {
                matching_found += 1;
                
                // Check if this matching avatar is on our current page
                let page_start = self.page * PAGE_SIZE;
                let page_end = page_start + PAGE_SIZE;
                
                if matching_found > page_start && matching_found <= page_end {
                    self.avatars.push(entry);
                    loaded += 1;
                }
            }
        }

        self.log(&format!("Checked {} avatars, found {} matching, loaded {} for page {}", 
            total_checked, matching_found, loaded, self.page + 1));
        Ok(loaded)
    }

    fn matches_query(&self, avatar: &AvatarEntry) -> bool {
        if self.filter_pc && !avatar.platforms.iter().any(|p| p == "PC") {
            return false;
        }
        if self.filter_quest && !avatar.platforms.iter().any(|p| p == "Quest") {
            return false;
        }
        if self.filter_ios && !avatar.platforms.iter().any(|p| p == "iOS") {
            return false;
        }

        if !self.search_name_lower.is_empty()
            && !avatar
                .name
                .to_lowercase()
                .contains(&self.search_name_lower)
        {
            return false;
        }

        if !self.search_author_lower.is_empty()
            && !avatar
                .author
                .to_lowercase()
                .contains(&self.search_author_lower)
        {
            return false;
        }

        if !self.search_description_lower.is_empty()
            && !avatar
                .description
                .to_lowercase()
                .contains(&self.search_description_lower)
        {
            return false;
        }

        true
    }

    fn apply_filters(&mut self) {
        self.log(&format!("Applying filters: name='{}', author='{}', desc='{}', PC={}, Quest={}, iOS={}", 
            self.search_name, self.search_author, self.search_description, 
            self.filter_pc, self.filter_quest, self.filter_ios));
        self.update_query_terms();
        if let Err(err) = self.load_page(true) {
            self.log(&format!("Error applying filters: {}", err));
        } else {
            self.log(&format!("Filters applied: {} results", self.filtered.len()));
        }
    }

    fn next_page(&mut self) {
        let total_pages = (self.total_avatars + PAGE_SIZE - 1) / PAGE_SIZE;
        if self.page + 1 >= total_pages {
            self.log("Already on last page");
            return;
        }
        self.page += 1;
        self.log(&format!("Navigating to page {} of {}", self.page + 1, total_pages));
        if let Err(err) = self.load_page(false) {
            self.log(&format!("Error loading page: {}", err));
        }
    }

    fn prev_page(&mut self) {
        if self.page == 0 {
            self.log("Already on first page");
            return;
        }
        self.page -= 1;
        self.log(&format!("Navigating to page {}", self.page + 1));
        // Load from the beginning up to the target page
        self.reset_stream();
        for _ in 0..self.page {
            let _ = self.load_next_page(); // Skip pages
        }
        // Load the target page
        if let Err(err) = self.load_page(false) {
            self.log(&format!("Error loading page: {}", err));
        }
    }

    fn render_avatar_card(&mut self, ui: &mut egui::Ui, avatar: &AvatarEntry) {
        let avatar_id = avatar.avatar_id.clone();
        self.request_avatar(&avatar_id);

        let is_removed = self.details.get(&avatar_id)
            .and_then(|d| d.status.as_ref())
            .map_or(false, |s| s == "Deleted");

        let frame_stroke = if is_removed {
            egui::Stroke::new(3.0, egui::Color32::RED)
        } else {
            egui::Stroke::new(1.0, egui::Color32::from_gray(60))
        };

        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(10.0))
            .stroke(frame_stroke)
            .show(ui, |ui| {
                ui.set_width(CARD_WIDTH);
                ui.vertical(|ui| {
                    ui.set_width(CARD_WIDTH - 8.0);
                    
                    let mut image_loaded = false;
                    ui.horizontal(|ui| {
                        if let Some(detail) = self.details.get(&avatar_id) {
                            if let Some(texture) = &detail.image {
                                let img = Image::new(texture)
                                    .fit_to_exact_size(Vec2::new(CARD_IMAGE_WIDTH, CARD_IMAGE_HEIGHT));
                                ui.add(img);
                                image_loaded = true;
                            } else {
                                let is_deleted = detail.status.as_ref().map_or(false, |s| s == "Deleted");
                                
                                if is_deleted {
                                    let cross_rect = egui::Rect::from_min_size(
                                        ui.cursor().min,
                                        Vec2::new(CARD_IMAGE_WIDTH, CARD_IMAGE_HEIGHT)
                                    );
                                    
                                    ui.painter().rect_filled(
                                        cross_rect,
                                        4.0,
                                        egui::Color32::from_gray(40)
                                    );
                                    
                                    let center = cross_rect.center();
                                    let size = 30.0;
                                    let stroke = egui::Stroke::new(4.0, egui::Color32::RED);
                                    
                                    ui.painter().line_segment(
                                        [center - egui::Vec2::new(size, size), center + egui::Vec2::new(size, size)],
                                        stroke
                                    );
                                    ui.painter().line_segment(
                                        [center - egui::Vec2::new(size, -size), center + egui::Vec2::new(size, -size)],
                                        stroke
                                    );
                                    
                                    let text_pos = egui::Pos2::new(center.x, center.y + size + 15.0);
                                    let text_color = egui::Color32::RED;
                                    ui.painter().text(
                                        text_pos,
                                        egui::Align2::CENTER_CENTER,
                                        "(Removed)",
                                        egui::FontId::default(),
                                        text_color,
                                    );
                                    
                                    ui.allocate_exact_size(Vec2::new(CARD_IMAGE_WIDTH, CARD_IMAGE_HEIGHT), egui::Sense::hover());
                                } else {
                                    ui.label(detail.status.clone().unwrap_or_else(|| "Unavailable".into()));
                                }
                                image_loaded = true;
                            }
                        } else {
                            let loading_rect = egui::Rect::from_min_size(
                                ui.cursor().min,
                                Vec2::new(CARD_IMAGE_WIDTH, CARD_IMAGE_HEIGHT)
                            );
                            ui.painter().rect_filled(
                                loading_rect,
                                4.0,
                                egui::Color32::from_gray(40)
                            );
                            
                            let time = ui.ctx().animate_value_with_time(
                                egui::Id::new(format!("loading_{}", avatar_id)),
                                0.0,
                                2.0
                            );
                            let center = loading_rect.center();
                            let radius = 12.0;
                            let start_angle = time * std::f32::consts::TAU;
                            let _end_angle = start_angle + std::f32::consts::FRAC_PI_2;
                            
                            ui.painter().circle_stroke(
                                center,
                                radius,
                                egui::Stroke::new(3.0, egui::Color32::WHITE),
                            );
                            
                            ui.allocate_exact_size(Vec2::new(CARD_IMAGE_WIDTH, CARD_IMAGE_HEIGHT), egui::Sense::hover());
                        }
                        
                        ui.vertical(|ui| {
                            for platform in &avatar.platforms {
                                let color = match platform.as_str() {
                                    "PC" => egui::Color32::from_rgb(0, 255, 0),
                                    "Quest" => egui::Color32::from_rgb(255, 165, 0),
                                    "iOS" => egui::Color32::from_rgb(0, 122, 255),
                                    _ => egui::Color32::GRAY,
                                };
                                ui.colored_label(color, platform);
                            }
                        });
                    });

                    ui.label(egui::RichText::new(&avatar.name).strong());
                    ui.label(&avatar.author);
                    ui.label(
                        egui::RichText::new(&avatar.description)
                            .italics()
                            .small(),
                    );

                    ui.horizontal(|ui| {
                        let is_removed = self.details.get(&avatar_id)
                            .and_then(|d| d.status.as_ref())
                            .map_or(false, |s| s == "Deleted");
                        
                        ui.add_enabled(!is_removed, egui::Button::new("Open Web"))
                            .clicked()
                            .then(|| {
                                let url = format!("https://vrchat.com/home/avatar/{}", avatar_id);
                                self.log(&format!("Opening web page for avatar: {}", url));
                                let _ = open_url(url);
                            });
                        if image_loaded && ui.button("Refresh").clicked() {
                            self.log(&format!("Refreshing image for avatar {}", avatar_id));
                            self.details.remove(&avatar_id);
                            self.request_avatar(&avatar_id);
                        }
                        if self.sender.is_some() {
                            ui.add_enabled(!is_removed, egui::Button::new("Wear"))
                                .clicked()
                                .then(|| {
                                    self.log(&format!("Attempting to wear avatar: {}", avatar_id));
                                    if let Some(sender) = &self.sender {
                                        let _ = sender.send(WorkerRequest::SelectAvatar {
                                            avatar_id: avatar_id.to_string(),
                                        });
                                    }
                                    self.selection_message =
                                        Some(format!("Equipping {}...", &avatar.name));
                                });
                        }
                    });
                });
            });
    }
}

fn default_avatar_id() -> String {
    "unknown".to_string()
}

#[derive(Debug)]
struct Reader {
    data: Vec<u8>,
    position: usize,
}

impl Reader {
    fn new(data: Vec<u8>) -> Self {
        Self { data, position: 0 }
    }
    
    fn remaining(&self) -> usize {
        self.data.len() - self.position
    }
    
    fn read_bytes(&mut self, amount: usize) -> Result<&[u8]> {
        if self.position + amount > self.data.len() {
            eprintln!("DEBUG: read_bytes failed - need {} bytes, have {} bytes, position {}, total {}", 
                     amount, self.data.len() - self.position, self.position, self.data.len());
            return Err(anyhow!("Attempted to read beyond end of data"));
        }
        let bytes = &self.data[self.position..self.position + amount];
        self.position += amount;
        Ok(bytes)
    }
    
    fn read_byte(&mut self) -> Result<u8> {
        let bytes = self.read_bytes(1)?;
        Ok(bytes[0])
    }
    
    fn read_int24(&mut self) -> Result<u32> {
        let bytes = self.read_bytes(3)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0]))
    }
    
    fn read_u32(&mut self) -> Result<u32> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }
    
    fn read_int_array(&mut self, count: usize) -> Result<Vec<u32>> {
        let mut result = Vec::with_capacity(count);
        for _ in 0..count {
            if self.position + 4 > self.data.len() {
                eprintln!("DEBUG: Not enough bytes for i32, breaking at position {}", self.position);
                break;
            }
            let slice = &self.data[self.position..self.position + 4];
            result.push(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]));
            self.position += 4;
        }
        Ok(result)
    }
}

fn decode_avatar_id(mut crypt: Vec<u8>, iv: &[u8]) -> Result<String> {
    if crypt.len() != 16 || iv.len() != 16 {
        return Err(anyhow!("Avatar ID blocks must be 16 bytes"));
    }

    for i in (0..crypt.len()).rev() {
        let prev = if i == 0 { crypt.len() - 1 } else { i - 1 };
        let k = crypt[i] ^ crypt[prev] ^ iv[i];
        crypt[i] = k;
    }

    for byte in crypt.iter_mut() {
        *byte = (*byte >> 4) | (*byte << 4);
    }

    let mut hex_bytes: Vec<String> = crypt.iter().map(|b| format!("{:02x}", b)).collect();
    hex_bytes.reverse();
    let hex_string = hex_bytes.join("");

    if hex_string.len() != 32 {
        return Err(anyhow!("Decoded avatar ID must be 32 hex chars, got {}", hex_string.len()));
    }

    let formatted = format!(
        "{}-{}-{}-{}-{}",
        &hex_string[0..8],
        &hex_string[8..12],
        &hex_string[12..16],
        &hex_string[16..20],
        &hex_string[20..32]
    );

    Ok(format!("avtr_{}", formatted))
}

fn parse_main_database(bytes: Vec<u8>) -> Result<ProcessedData> {
    eprintln!("DEBUG: parse_main_database called with {} bytes", bytes.len());
    
    let mut reader = Reader::new(bytes);
    if reader.remaining() == 0 {
        return Err(anyhow!("Data has length zero"));
    }

    if reader.read_bytes(3)? != b"PAS" {
        return Err(anyhow!("PAS Header not found"));
    }

    reader.read_bytes(2)?; // platform + format version
    let avatar_count = reader.read_int24()?;
    let author_count = reader.read_int24()?;
    eprintln!("DEBUG: avatar_count: {}, author_count: {}", avatar_count, author_count);

    let date_arr = reader.read_bytes(2)?;
    let date_num = (((date_arr[0] as u32) << 8) + date_arr[1] as u32) >> 3;
    let year = ((date_num >> 9) + 16) as u32;
    let month = ((date_num >> 5) & 15) as u32;
    let day = (date_num & 31) as u32;
    let last_update = format!("20{year:02}-{month:02}-{day:02}");

    let file_avatars = reader.read_int24()? as usize;
    eprintln!("DEBUG: file_avatars from header: {}", file_avatars);
    
    // Calculate actual avatars based on remaining bytes (be very conservative)
    let remaining_bytes = reader.remaining();
    // Reserve space for: flags (4 bytes each) + author_ids (4 bytes each) + string data (estimate 100 bytes each)
    let bytes_per_avatar = 16 + 4 + 4 + 100; // avatar_id + flag + author_id + estimated string data
    let max_possible_avatars = remaining_bytes / bytes_per_avatar;
    let actual_avatars = std::cmp::min(file_avatars, max_possible_avatars);
    
    eprintln!("DEBUG: remaining bytes: {}, bytes per avatar: {}, max possible avatars: {}, actual avatars: {}", 
             remaining_bytes, bytes_per_avatar, max_possible_avatars, actual_avatars);
    
    reader.read_int24()?; // file authors, not needed

    let flag_size = reader.read_byte()? as usize;
    let random_bytes = reader.read_bytes(16)?;
    let dynamic_bytes: Vec<u8> = random_bytes
        .iter()
        .zip(STATIC_BYTES.iter())
        .map(|(a, b)| a ^ b)
        .collect();

    // Use actual avatar count instead of header count
    let avatar_ids = reader.read_bytes(actual_avatars * 16)?.to_vec();
    eprintln!("DEBUG: Read {} avatar_ids bytes", avatar_ids.len());
    
    // Calculate how many flags we can actually read
    let available_for_flags = reader.remaining();
    let max_flags = available_for_flags / 4;
    let actual_flags = std::cmp::min(actual_avatars, max_flags);
    let _flags = reader.read_int_array(actual_flags)?; // retained for parity with JS
    eprintln!("DEBUG: Read {} flags", _flags.len());
    
    // Calculate how many author_ids we can actually read
    let available_for_authors = reader.remaining();
    let max_authors = available_for_authors / 4;
    let actual_authors = std::cmp::min(actual_avatars, max_authors);
    let author_ids = reader.read_int_array(actual_authors)?;
    eprintln!("DEBUG: Read {} author_ids", author_ids.len());

    let remaining_after_authors = reader.remaining();
    eprintln!("DEBUG: Remaining bytes after author_ids: {}", remaining_after_authors);
    
    // Save the raw remaining bytes for examination
    let raw_string_bytes = reader.read_bytes(reader.remaining())?;
    eprintln!("DEBUG: Raw string bytes length: {}", raw_string_bytes.len());
    eprintln!("DEBUG: First 50 raw bytes: {:?}", &raw_string_bytes[..raw_string_bytes.len().min(50)]);
    
    // Parse string data exactly like the working old script
    let strings = String::from_utf8(raw_string_bytes.to_vec())?;
    let mut parts = strings.split('\n');
    let authors_block = parts
        .next()
        .ok_or_else(|| anyhow!("Malformed string block: authors missing"))?;
    let avatars_block = parts
        .next()
        .ok_or_else(|| anyhow!("Malformed string block: avatars missing"))?;

    let author_names: Vec<String> = authors_block
        .split('\r')
        .map(|s| s.chars().rev().collect())
        .collect();
    let avatar_lines: Vec<&str> = avatars_block.split('\r').collect();

    if avatar_lines.len() < actual_avatars {
        eprintln!("DEBUG: Avatar list shorter than expected ({} < {}), using available", avatar_lines.len(), actual_avatars);
    }

    let mut entries = Vec::new();
    let mut index = HashMap::new();

    // Use actual_avatars to avoid out of bounds
    for i in 0..std::cmp::min(actual_avatars, std::cmp::min(avatar_lines.len(), 1000)) { // Limit to first 1000 for testing
        let block = avatar_ids[i * 16..(i + 1) * 16].to_vec();
        let avatar_id = match decode_avatar_id(block, &dynamic_bytes) {
            Ok(id) => id,
            Err(e) => {
                eprintln!("DEBUG: Failed to decode avatar ID at index {}: {}", i, e);
                break;
            }
        };

        let name_desc: Vec<&str> = avatar_lines[i].split('\t').collect();
        let name = name_desc
            .get(0)
            .map(|s| s.chars().rev().collect())
            .unwrap_or_default();
        let description = name_desc
            .get(1)
            .map(|s| s.chars().rev().collect())
            .unwrap_or_default();

        let author_idx = if i < author_ids.len() { (author_ids[i] & 524_287) as usize } else { 0 };
        let author = if author_idx < author_names.len() {
            author_names[author_idx].clone()
        } else {
            format!("Author_{}", author_idx)
        };

        let mut entry = AvatarEntry {
            name,
            author,
            description,
            avatar_id: avatar_id.clone(),
            platforms: vec!["PC".to_string()],
            quest: false,
            ios: false,
        };
        normalize_platforms(&mut entry);
        entries.push(entry);
        index.insert(avatar_id, i);
    }
    
    eprintln!("DEBUG: Successfully processed {} avatars with proper string reversal", entries.len());
    
    // Save the decoded entries
    eprintln!("DEBUG: Saving decoded entries to cache/decoded/");
    if let Err(e) = fs::write("cache/decoded/main_decoded.json", serde_json::to_string_pretty(&entries)?) {
        eprintln!("DEBUG: Failed to save decoded entries: {}", e);
    } else {
        eprintln!("DEBUG: Saved {} decoded entries to cache/decoded/main_decoded.json", entries.len());
    }
    
    // Also save the raw string data for examination
    if let Err(e) = fs::write("cache/decoded/raw_string_data.txt", &strings) {
        eprintln!("DEBUG: Failed to save raw string data: {}", e);
    } else {
        eprintln!("DEBUG: Saved raw string data to cache/decoded/raw_string_data.txt");
    }

    Ok(ProcessedData {
        data: AvatarData {
            avatar_count,
            author_count,
            last_update,
            entries,
        },
        index,
    })
}

fn parse_aux_database(bytes: Vec<u8>) -> Result<Vec<String>> {
    eprintln!("DEBUG: parse_aux_database called with {} bytes", bytes.len());
    
    let mut reader = Reader::new(bytes);
    if reader.remaining() == 0 {
        return Err(anyhow!("Data has length zero"));
    }

    if reader.read_bytes(3)? != b"PAS" {
        return Err(anyhow!("PAS Header not found"));
    }

    reader.read_bytes(2 + 3 + 3 + 2)?; // platform, avatars, authors, date
    let file_avatars = reader.read_int24()? as usize;
    eprintln!("DEBUG: aux file_avatars from header: {}", file_avatars);
    
    reader.read_bytes(3 + 1)?; // fileAuthors + flagSize

    let random_bytes = reader.read_bytes(16)?;
    let dynamic_bytes: Vec<u8> = random_bytes
        .iter()
        .zip(STATIC_BYTES.iter())
        .map(|(a, b)| a ^ b)
        .collect();
    
    // Calculate actual avatars based on available bytes (be conservative)
    let remaining_bytes = reader.remaining();
    let max_possible_avatars = remaining_bytes / 16; // Each avatar needs 16 bytes for ID
    let actual_avatars = std::cmp::min(file_avatars, max_possible_avatars);
    
    eprintln!("DEBUG: aux remaining bytes: {}, max possible avatars: {}, actual avatars: {}", 
             remaining_bytes, max_possible_avatars, actual_avatars);
    
    let avatar_ids = reader.read_bytes(actual_avatars * 16)?;

    let mut ids = Vec::with_capacity(actual_avatars);
    for i in 0..actual_avatars {
        let block = avatar_ids[i * 16..(i + 1) * 16].to_vec();
        match decode_avatar_id(block, &dynamic_bytes) {
            Ok(id) => ids.push(id),
            Err(e) => {
                eprintln!("DEBUG: Failed to decode aux avatar ID at index {}: {}", i, e);
                break;
            }
        }
    }

    eprintln!("DEBUG: Successfully decoded {} aux avatar IDs", ids.len());
    Ok(ids)
}

fn mark_platform(processed: &mut ProcessedData, ids: &[String], quest: bool, ios: bool) {
    let mut missing = 0;
    for id in ids {
        if let Some(&idx) = processed.index.get(id) {
            let entry = processed.data.entries.get_mut(idx).unwrap();
            if quest {
                entry.quest = true;
            }
            if ios {
                entry.ios = true;
            }
        } else {
            missing += 1;
        }
    }

    if missing > 0 {
        eprintln!("{} platform IDs missing from primary list", missing);
    }
}

fn download_bytes(client: &Client, url: &str) -> Result<Vec<u8>> {
    eprintln!("DEBUG: Attempting to download from: {}", url);
    
    let resp = client
        .get(url)
        .send()
        .with_context(|| format!("GET {}", url))?;
        
    if !resp.status().is_success() {
        return Err(anyhow!("Failed to download {}: status {}", url, resp.status()));
    }
    
    let content_length = resp.content_length();
    eprintln!("DEBUG: Content-Length header: {:?}", content_length);
    
    let bytes = resp.bytes()?.to_vec();
    eprintln!("DEBUG: Downloaded {} bytes", bytes.len());
    
    // Check if download is complete
    if let Some(expected_len) = content_length {
        if bytes.len() != expected_len as usize {
            eprintln!("WARNING: Incomplete download! Got {} bytes, expected {}", bytes.len(), expected_len);
            return Err(anyhow!("Incomplete download: got {} bytes, expected {}", bytes.len(), expected_len));
        }
    }
    
    Ok(bytes)
}

fn download_with_fallback(client: &Client, index: usize) -> Result<Vec<u8>> {
    let primary = URLS[index];
    match download_bytes(client, primary) {
        Ok(bytes) => Ok(bytes),
        Err(err) => {
            eprintln!("Primary download failed ({}): {}", primary, err);
            download_bytes(client, BACKUP_URLS[index])
        }
    }
}

fn load_config() -> Config {
    PathBuf::from(CONFIG_FILE)
        .exists()
        .then(|| fs::read_to_string(CONFIG_FILE).ok())
        .flatten()
        .and_then(|text| serde_json::from_str::<Config>(&text).ok())
        .unwrap_or_default()
}

fn normalize_platforms(entry: &mut AvatarEntry) {
    if entry.platforms.is_empty() {
        let mut platforms = vec!["PC".to_string()];
        if entry.quest {
            platforms.push("Quest".to_string());
        }
        if entry.ios {
            platforms.push("iOS".to_string());
        }
        entry.platforms = platforms;
    }
}

fn texture_from_bytes(
    ctx: &egui::Context,
    key: &str,
    bytes: &[u8],
) -> Result<egui::TextureHandle> {
    let guessed = image::guess_format(bytes).unwrap_or(image::ImageFormat::Png);
    let reader = ImageReader::with_format(std::io::Cursor::new(bytes), guessed);
    let image = reader.decode().context("Failed to decode image")?;
    let size = [image.width() as usize, image.height() as usize];
    let rgba = image.to_rgba8();
    let color_image = ColorImage::from_rgba_unmultiplied(size, rgba.as_flat_samples().as_slice());
    Ok(ctx.load_texture(key.to_string(), color_image, egui::TextureOptions::LINEAR))
}

fn spawn_worker(auth_cookie: String) -> (Sender<WorkerRequest>, Receiver<WorkerResponse>) {
    let (tx, rx) = flume::unbounded();
    let (rtx, rrx) = flume::unbounded();
    
    thread::spawn(move || {
        let client = match Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(USER_AGENT)
            .build() {
                Ok(client) => client,
                Err(_) => return,
            };
        
        while let Ok(req) = rx.try_recv() {
            match req {
                WorkerRequest::FetchAvatar { avatar_id } => {
                    match fetch_avatar_image(&client, &auth_cookie, &avatar_id) {
                        Ok((image, status)) => {
                            let outcome = WorkerResponse::AvatarImage { 
                                avatar_id, 
                                image, 
                                status,
                            };
                            let _ = rtx.send(outcome);
                        }
                        Err(_) => {
                            let outcome = WorkerResponse::AvatarImage { 
                                avatar_id, 
                                image: None, 
                                status: Some("Error".to_string()),
                            };
                            let _ = rtx.send(outcome);
                        }
                    }
                }
                WorkerRequest::FetchCurrentAvatar => {
                    match fetch_current_avatar_info(&client, &auth_cookie) {
                        Ok((avatar_id, name, image)) => {
                            let outcome = WorkerResponse::CurrentAvatar {
                                avatar_id,
                                name,
                                image,
                            };
                            let _ = rtx.send(outcome);
                        }
                        Err(_) => {
                            let _ = rtx.send(WorkerResponse::Error("Failed to fetch current avatar".to_string()));
                        }
                    }
                }
                WorkerRequest::SelectAvatar { avatar_id } => {
                    let outcome = select_avatar_remote(&client, &auth_cookie, &avatar_id)
                        .map(|_| WorkerResponse::SelectResult {
                            avatar_id: avatar_id.clone(),
                            success: true,
                            message: "Avatar equipped".into(),
                        })
                        .unwrap_or_else(|_| WorkerResponse::SelectResult {
                            avatar_id: avatar_id.clone(),
                            success: false,
                            message: "Failed to equip avatar".to_string(),
                        });
                    let _ = rtx.send(outcome);
                }
            }
        }
    });

    (tx, rrx)
}

fn fetch_avatar_image(
    client: &Client,
    auth_cookie: &str,
    avatar_id: &str,
) -> Result<(Option<Vec<u8>>, Option<String>)> {
    let response = client
        .get(format!("{API_BASE}/avatars/{avatar_id}"))
        .header("Cookie", format!("auth={}", auth_cookie))
        .send()?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok((None, Some("Deleted".to_string())));
    }

    if !response.status().is_success() {
        return Err(anyhow!("API returned status {}", response.status()));
    }

    let json: serde_json::Value = response.json()?;
    let image_url = json
        .get("imageUrl")
        .and_then(|v| v.as_str())
        .or_else(|| json.get("thumbnailImageUrl").and_then(|v| v.as_str()));
    
    if image_url.is_none() {
        return Ok((None, Some("No Image".to_string())));
    }

    let bytes = client
        .get(image_url.unwrap())
        .header("Cookie", format!("auth={}", auth_cookie))
        .send()?
        .bytes()?;

    Ok((Some(bytes.to_vec()), Some("Loaded".to_string())))
}

fn fetch_current_avatar_info(
    client: &Client,
    auth_cookie: &str,
) -> Result<(String, String, Option<Vec<u8>>)> {
    let user_resp = client
        .get(format!("{API_BASE}/auth/user"))
        .header("Cookie", format!("auth={}", auth_cookie))
        .send()?;
    if !user_resp.status().is_success() {
        return Err(anyhow!("Failed to fetch user profile: {}", user_resp.status()));
    }
    let user_json: serde_json::Value = user_resp.json()?;
    let current_id = user_json
        .get("currentAvatar")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("User profile missing current avatar"))?
        .to_string();

    let avatar_resp = client
        .get(format!("{API_BASE}/avatars/{current_id}"))
        .header("Cookie", format!("auth={}", auth_cookie))
        .send()?;
    if !avatar_resp.status().is_success() {
        return Err(anyhow!("Failed to fetch current avatar data: {}", avatar_resp.status()));
    }
    let avatar_json: serde_json::Value = avatar_resp.json()?;
    let name = avatar_json
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown")
        .to_string();
    let image_url = avatar_json
        .get("imageUrl")
        .and_then(|v| v.as_str())
        .or_else(|| avatar_json.get("thumbnailImageUrl").and_then(|v| v.as_str()));
    let image = if let Some(url) = image_url {
        Some(
            client
                .get(url)
                .header("Cookie", format!("auth={}", auth_cookie))
                .send()?
                .bytes()?
                .to_vec(),
        )
    } else {
        None
    };

    Ok((current_id, name, image))
}

fn select_avatar_remote(client: &Client, auth_cookie: &str, avatar_id: &str) -> Result<()> {
    let resp = client
        .put(format!("{API_BASE}/avatars/{avatar_id}/select"))
        .header("Cookie", format!("auth={}", auth_cookie))
        .send()?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(anyhow!("Select avatar failed: {}", resp.status()))
    }
}

impl eframe::App for UnifiedApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_workers(ctx);
        
        match self.state {
            AppState::CheckingFiles => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.heading("VRChat Avatar Manager");
                        ui.add_space(20.0);
                        ui.label("Checking required files...");
                        ui.spinner();
                        
                        if ctx.frame_nr() == 1 {
                            self.check_files();
                        }
                    });
                });
            }
            
            AppState::DatabaseDownloader => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.heading("VRChat Avatar Database Downloader");
                    ui.separator();
                    
                    // Show overall status
                    ui.label(format!("Status: {}", self.status));
                    
                    // Show database download progress
                    ui.separator();
                    ui.heading("Database Downloads:");
                    
                    for db_info in &self.databases {
                        ui.horizontal(|ui| {
                            ui.label(&db_info.platform);
                            ui.add_space(20.0);
                            
                            if let Some((downloaded, total, status)) = self.db_progress.get(&db_info.platform) {
                                ui.label(status.clone());
                                
                                if *total > 0 {
                                    let progress = *downloaded as f32 / *total as f32;
                                    ui.add_space(10.0);
                                    ui.add(egui::ProgressBar::new(progress).show_percentage());
                                    ui.add_space(10.0);
                                    
                                    // Show file size
                                    let downloaded_mb = *downloaded / 1024 / 1024;
                                    let total_mb = *total / 1024 / 1024;
                                    ui.label(format!("{:.1} MB / {:.1} MB", downloaded_mb as f32 / 1024.0, total_mb as f32 / 1024.0));
                                }
                            } else {
                                ui.label("Pending");
                            }
                        });
                    }
                    
                    ui.separator();
                    
                    // Show processing progress
                    if let Some((downloaded, total, status)) = self.db_progress.get("Processing") {
                        ui.heading("Processing:");
                        ui.label(status.clone());
                        
                        if *total > 0 {
                            let progress = *downloaded as f32 / *total as f32;
                            ui.add_space(10.0);
                            ui.add(egui::ProgressBar::new(progress).show_percentage());
                            ui.add_space(10.0);
                            ui.label(format!("Processed {} / {} avatars", downloaded, total));
                        }
                    }
                    
                    ui.separator();
                    
                    // Show debug log if enabled
                    if self.show_debug {
                        ui.checkbox(&mut self.show_debug, "Show Debug Log");
                        ui.separator();
                        
                        egui::ScrollArea::vertical()
                            .max_height(300.0)
                            .show(ui, |ui| {
                                for log_entry in &self.debug_log {
                                    ui.label(
                                        egui::RichText::new(log_entry)
                                            .monospace()
                                            .size(12.0)
                                    );
                                }
                            });
                    } else {
                        ui.checkbox(&mut self.show_debug, "Show Debug Log");
                    }
                    
                    ui.separator();
                    
                    // Control buttons
                    ui.horizontal(|ui| {
                        if self.db_downloading {
                            if ui.button("Cancel Download").clicked() {
                                self.db_downloading = false;
                                self.log("Download cancelled by user");
                            }
                        } else {
                            if ui.button("Retry Download").clicked() {
                                self.start_database_download();
                            }
                        }
                        
                        if ui.button("Close").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                });
            }
            
            AppState::Login => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.heading("VRChat Login");
                    ui.separator();
                    
                    ui.vertical_centered(|ui| {
                        ui.add_space(20.0);
                        
                        if self.two_factor_state.is_some() {
                            // Show 2FA input
                            ui.label("Two-factor authentication required:");
                            ui.label(format!("Type: {}", self.two_factor_state.as_ref().unwrap().two_factor_type));
                            
                            ui.horizontal(|ui| {
                                ui.label("2FA Code:");
                                ui.add(egui::TextEdit::singleline(&mut self.two_factor_code)
                                    .password(true));
                            });
                            
                            ui.add_space(10.0);
                            
                            ui.horizontal(|ui| {
                                if ui.add_enabled(!self.login_loading, egui::Button::new("Verify")).clicked() {
                                    self.verify_two_factor_code();
                                }
                                if ui.button("Cancel").clicked() {
                                    self.two_factor_state = None;
                                    self.two_factor_code.clear();
                                    self.login_status = "Two-factor verification canceled".to_string();
                                }
                            });
                        } else {
                            // Show regular login
                            ui.horizontal(|ui| {
                                ui.label("Username:");
                                ui.add(egui::TextEdit::singleline(&mut self.login_username));
                            });
                            
                            ui.horizontal(|ui| {
                                ui.label("Password:");
                                ui.add(egui::TextEdit::singleline(&mut self.login_password)
                                    .password(true));
                            });
                            
                            ui.checkbox(&mut self.login_remember_me, "Remember me");
                            
                            ui.add_space(10.0);
                            
                            if ui.add_enabled(!self.login_loading, egui::Button::new("Login")).clicked() {
                                self.attempt_login();
                            }
                        }
                        
                        if !self.login_status.is_empty() {
                            ui.add_space(10.0);
                            ui.label(&self.login_status);
                        }
                        
                        if self.login_loading {
                            ui.add_space(10.0);
                            ui.spinner();
                        }
                    });
                });
            }
            
            AppState::AvatarBrowser => {
                egui::TopBottomPanel::top("filters").show(ctx, |ui| {
                    ui.heading("VRChat Avatar Browser (Rust)");
                    ui.label(&self.status);
                    ui.horizontal(|ui| {
                        ui.group(|ui| {
                            ui.label("Current Avatar");
                            if let Some(texture) = &self.current_avatar_image {
                                let img = Image::new(texture).fit_to_exact_size(egui::vec2(96.0, 96.0));
                                ui.add(img);
                            } else {
                                ui.label("(not loaded)");
                            }
                        });
                        ui.vertical(|ui| {
                            if let Some(name) = &self.current_avatar_name {
                                ui.strong(name);
                            } else {
                                ui.label("Unknown");
                            }
                            if let Some(id) = &self.current_avatar_id {
                                ui.label(id);
                            }
                            if ui.button("Refresh Current").clicked() {
                                self.request_current_avatar();
                            }
                            if let Some(msg) = &self.selection_message {
                                ui.label(msg);
                            }
                        });
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Search:");
                        let name_response = ui.add(
                            egui::TextEdit::singleline(&mut self.search_name)
                                .hint_text("Avatar name..."),
                        );
                        let author_response = ui.add(
                            egui::TextEdit::singleline(&mut self.search_author)
                                .hint_text("Author name..."),
                        );
                        let desc_response = ui.add(
                            egui::TextEdit::singleline(&mut self.search_description)
                                .hint_text("Description..."),
                        );
                        
                        if name_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            self.apply_filters();
                        }
                        if author_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            self.apply_filters();
                        }
                        if desc_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            self.apply_filters();
                        }
                        
                        ui.checkbox(&mut self.filter_pc, "PC");
                        ui.checkbox(&mut self.filter_quest, "Quest");
                        ui.checkbox(&mut self.filter_ios, "iOS");
                        if ui.button("Apply Filters").clicked() {
                            self.apply_filters();
                        }
                        
                        ui.separator();
                        let total_pages = (self.total_avatars + PAGE_SIZE - 1) / PAGE_SIZE;
                        ui.add_enabled(self.page > 0, egui::Button::new("Previous")).clicked().then(|| {
                            self.prev_page();
                        });
                        ui.label(format!("Page {} of {}", self.page + 1, total_pages.max(1)));
                        ui.add_enabled(
                            self.page + 1 < total_pages,
                            egui::Button::new("Next")
                        ).clicked().then(|| {
                            self.next_page();
                        });
                    });
                });

                egui::CentralPanel::default().show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let _prev_spacing = ui.spacing().item_spacing;
                        ui.spacing_mut().item_spacing = Vec2::new(CARD_GAP, CARD_GAP);
                        let filtered_copy: Vec<usize> = self.filtered.iter().copied().collect();
                        let available_width = ui.available_width();
                        let cols = (available_width / (CARD_WIDTH + CARD_GAP)).max(1.0) as usize;
                        for row in filtered_copy.chunks(cols) {
                            ui.horizontal(|ui| {
                                for &idx in row {
                                    if let Some(avatar) = self.avatars.get(idx).cloned() {
                                        self.render_avatar_card(ui, &avatar);
                                    }
                                }
                            });
                        }
                    });
                });
            }
        }
    }
}

fn main() {
    env_logger::init();
    let options = eframe::NativeOptions::default();
    if let Err(err) = eframe::run_native(
        "VRChat Avatar Manager",
        options,
        Box::new(|_cc| Box::new(UnifiedApp::new())),
    ) {
        eprintln!("Failed to launch unified avatar manager: {err}");
    }
}
