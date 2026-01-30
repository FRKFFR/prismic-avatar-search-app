use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    io::{BufRead, BufReader, Seek, SeekFrom},
    path::PathBuf,
    thread,
    time::Duration,
};

use std::fs::File;

use anyhow::{anyhow, Context, Result};
use eframe::egui::{self, Align, ColorImage, Frame, Image, Layout, Margin, Vec2};
use flume::{Receiver, Sender};
use image::ImageReader;
use open::that as open_url;
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
const CARD_IMAGE_HEIGHT: f32 = 105.0; // 4:3 ratio (140 * 3/4 = 105)
const WORKER_COUNT: usize = 4;

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

fn fetch_current_avatar_info(
    client: &Client,
    auth_cookie: &str,
) -> Result<(String, String, Option<Vec<u8>>)> {
    let user_resp = client
        .get(format!("{API_BASE}/auth/user"))
        .header("Cookie", format!("auth={}", auth_cookie))
        .header("User-Agent", USER_AGENT)
        .send()
        .context("GET auth/user")?;
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
        .header("User-Agent", USER_AGENT)
        .send()
        .with_context(|| format!("GET avatar {}", current_id))?;
    if !avatar_resp.status().is_success() {
        return Err(anyhow!(
            "Failed to fetch current avatar data: {}",
            avatar_resp.status()
        ));
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
                .header("User-Agent", USER_AGENT)
                .send()
                .with_context(|| format!("GET image {}", url))?
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
        .header("User-Agent", USER_AGENT)
        .send()
        .with_context(|| format!("PUT select avatar {}", avatar_id))?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(anyhow!("Select avatar failed: {}", resp.status()))
    }
}

impl AvatarBrowserApp {
    fn update_query_terms(&mut self) {
        self.search_name_lower = self.search_name.to_lowercase();
        self.search_author_lower = self.search_author.to_lowercase();
        self.search_description_lower = self.search_description.to_lowercase();
    }

    fn reset_stream(&mut self) {
        self.stream_offset = 0;
        self.page = 0;
        self.total_avatars = 0;
        self.avatars.clear();
        self.filtered.clear();
        
        // Count matching avatars for proper pagination
        if let Ok(file) = File::open(AVATAR_STREAM) {
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

    fn load_page(&mut self, reset: bool) -> Result<()> {
        if reset {
            self.reset_stream();
            self.filtered.clear();
            self.details.clear();
            self.pending.clear();
            self.texture_order.clear();
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
}

fn default_avatar_id() -> String {
    "unknown".to_string()
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

struct AvatarDetail {
    image: Option<egui::TextureHandle>,
    status: Option<String>,
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
        status: Option<String>, // "Loaded", "Unavailable", "Deleted", "No Image", "Decode Failed"
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

fn spawn_worker(auth_cookie: String) -> (Sender<WorkerRequest>, Receiver<WorkerResponse>) {
    let (tx, rx) = flume::unbounded();
    let (rtx, rrx) = flume::unbounded();

    for _ in 0..WORKER_COUNT {
        let rx = rx.clone();
        let rtx = rtx.clone();
        let auth_cookie = auth_cookie.clone();
        thread::spawn(move || {
            let client = match Client::builder()
                .cookie_store(true)
                .user_agent(USER_AGENT)
                .timeout(Duration::from_secs(30))
                .build()
            {
                Ok(client) => client,
                Err(err) => {
                    let _ = rtx.send(WorkerResponse::Error(format!(
                        "HTTP client init failed: {}",
                        err
                    )));
                    return;
                }
            };

            while let Ok(message) = rx.recv() {
                match message {
                    WorkerRequest::FetchAvatar { avatar_id } => {
                        let result = fetch_avatar_image(&client, &auth_cookie, &avatar_id)
                            .map(|(bytes, status)| WorkerResponse::AvatarImage {
                                avatar_id,
                                image: bytes,
                                status,
                            })
                            .unwrap_or_else(|err| WorkerResponse::Error(err.to_string()));
                        let _ = rtx.send(result);
                    }
                    WorkerRequest::FetchCurrentAvatar => {
                        let response = fetch_current_avatar_info(&client, &auth_cookie)
                            .map(|(avatar_id, name, image)| WorkerResponse::CurrentAvatar {
                                avatar_id,
                                name,
                                image,
                            })
                            .unwrap_or_else(|err| WorkerResponse::Error(err.to_string()));
                        let _ = rtx.send(response);
                    }
                    WorkerRequest::SelectAvatar { avatar_id } => {
                        let outcome = select_avatar_remote(&client, &auth_cookie, &avatar_id)
                            .map(|_| WorkerResponse::SelectResult {
                                avatar_id: avatar_id.clone(),
                                success: true,
                                message: "Avatar equipped".into(),
                            })
                            .unwrap_or_else(|err| WorkerResponse::SelectResult {
                                avatar_id: avatar_id.clone(),
                                success: false,
                                message: err.to_string(),
                            });
                        let _ = rtx.send(outcome);
                        if let Ok((id, name, image)) = fetch_current_avatar_info(&client, &auth_cookie)
                        {
                            let _ = rtx.send(WorkerResponse::CurrentAvatar {
                                avatar_id: id,
                                name,
                                image,
                            });
                        }
                    }
                }
            }
        });
    }

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
        .send()
        .with_context(|| format!("GET avatar {}", avatar_id))?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok((None, Some("Deleted".to_string()))); // Avatar deleted/banned/private
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
        return Ok((None, Some("No Image".to_string()))); // Avatar exists but has no image (old avatar)
    }

    let bytes = client
        .get(image_url.unwrap())
        .header("Cookie", format!("auth={}", auth_cookie))
        .send()
        .with_context(|| format!("GET image {}", image_url.unwrap()))?
        .bytes()?;

    Ok((Some(bytes.to_vec()), Some("Loaded".to_string())))
}

pub struct AvatarBrowserApp {
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
    last_error: Option<String>,
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

impl AvatarBrowserApp {
    pub fn new() -> Self {
        let config = load_config();

        let (sender, receiver) = if config.auth_cookie.is_empty() {
            (None, None)
        } else {
            let (tx, rx) = spawn_worker(config.auth_cookie);
            (Some(tx), Some(rx))
        };

        let mut app = Self {
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
            sender,
            receiver,
            status: "Loading...".into(),
            last_error: None,
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

        app.log("Avatar Browser initialized");

        if let Err(err) = app.load_page(true) {
            app.log(&format!("Initialization error: {}", err));
            app.last_error = Some(err.to_string());
        }

        if app.sender.is_some() {
            app.request_current_avatar();
        } else {
            app.status = "Login required to view images and wear avatars.".into();
        }

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
        
        // Keep only last 1000 log entries
        if self.debug_log.len() > 1000 {
            self.debug_log.remove(0);
        }
    }

    fn apply_filters(&mut self) {
        self.log(&format!("Applying filters: name='{}', author='{}', desc='{}', PC={}, Quest={}, iOS={}", 
            self.search_name, self.search_author, self.search_description, 
            self.filter_pc, self.filter_quest, self.filter_ios));
        self.update_query_terms();
        if let Err(err) = self.load_page(true) {
            self.log(&format!("Error applying filters: {}", err));
            self.last_error = Some(err.to_string());
        } else {
            self.log(&format!("Filters applied: {} results", self.filtered.len()));
        }
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
            self.last_error = Some(err.to_string());
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
            self.last_error = Some(err.to_string());
        }
    }

    fn request_current_avatar(&self) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(WorkerRequest::FetchCurrentAvatar);
        }
    }

    fn poll_worker(&mut self, ctx: &egui::Context) {
        let mut needs_repaint = false;
        if let Some(receiver) = &self.receiver {
            let mut drained = Vec::new();
            while let Ok(message) = receiver.try_recv() {
                drained.push(message);
            }

            for message in drained {
                needs_repaint = true;
                match message {
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
                                Err(err) => {
                                    self.log(&format!("Failed to decode image for avatar {}: {}", avatar_id, err));
                                    self.last_error = Some(err.to_string());
                                    // Mark as decode failed to prevent re-requesting
                                    self.details.insert(
                                        avatar_id.clone(),
                                        AvatarDetail {
                                            image: None,
                                            status: Some("Decode Failed".to_string()),
                                        },
                                    );
                                    self.texture_order.push_back(avatar_id);
                                    self.prune_texture_cache();
                                }
                            }
                        } else {
                            // Handle different types of unavailability
                            let status_msg = status.unwrap_or_else(|| "Unavailable".to_string());
                            match status_msg.as_str() {
                                "Deleted" => {
                                    self.log(&format!("Avatar {} has been deleted/banned/private", avatar_id));
                                }
                                "No Image" => {
                                    self.log(&format!("Avatar {} exists but has no image (old avatar)", avatar_id));
                                }
                                _ => {
                                    self.log(&format!("No image available for avatar {}", avatar_id));
                                }
                            }
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
                    WorkerResponse::CurrentAvatar {
                        avatar_id,
                        name,
                        image,
                    } => {
                        self.log(&format!("Current avatar updated: {} ({})", name, avatar_id));
                        self.current_avatar_id = Some(avatar_id.clone());
                        self.current_avatar_name = Some(name.clone());
                        if let Some(bytes) = image {
                            match texture_from_bytes(ctx, &avatar_id, &bytes) {
                                Ok(texture) => {
                                    self.log("Successfully loaded current avatar image");
                                    self.current_avatar_image = Some(texture);
                                }
                                Err(err) => {
                                    self.log(&format!("Failed to decode current avatar image: {}", err));
                                    self.last_error = Some(err.to_string());
                                }
                            }
                        }
                    }
                    WorkerResponse::SelectResult {
                        avatar_id,
                        success,
                        message,
                    } => {
                        self.log(&format!("Avatar selection result for {}: {} - {}", 
                            avatar_id, if success { "SUCCESS" } else { "FAILED" }, message));
                        if success {
                            self.current_avatar_id = Some(avatar_id);
                            self.request_current_avatar(); // Refresh current avatar display
                        }
                        self.selection_message = Some(message);
                    }
                    WorkerResponse::Error(err) => {
                        self.log(&format!("Worker error: {}", err));
                        self.last_error = Some(err);
                    }
                }
            }
        }

        // Also repaint if there are pending images to show loading animation
        if needs_repaint || !self.pending.is_empty() {
            ctx.request_repaint();
        }
    }

    fn request_avatar(&mut self, avatar_id: &str) {
        if self.pending.len() >= MAX_PENDING {
            return;
        }
        if self.pending.contains(avatar_id) {
            return;
        }
        // Check if image is already loaded
        if let Some(detail) = self.details.get(avatar_id) {
            if detail.image.is_some() || detail.status.as_ref().map_or(false, |s| {
                s == "Unavailable" || s == "Decode Failed" || s == "Deleted" || s == "No Image"
            }) {
                return; // Already loaded or known to be unavailable/failed/deleted/no image
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

    fn render_avatar_card(&mut self, ui: &mut egui::Ui, avatar: &AvatarEntry) {
        let avatar_id = avatar.avatar_id.clone();
        self.request_avatar(&avatar_id);

        // Check if avatar is removed to determine frame styling
        let is_removed = self.details.get(&avatar_id)
            .and_then(|d| d.status.as_ref())
            .map_or(false, |s| s == "Deleted");

        let frame_stroke = if is_removed {
            egui::Stroke::new(3.0, egui::Color32::RED)
        } else {
            egui::Stroke::new(1.0, egui::Color32::from_gray(60))
        };

        Frame::group(ui.style())
            .inner_margin(Margin::same(10.0))
            .stroke(frame_stroke)
            .show(ui, |ui| {
                ui.set_width(CARD_WIDTH);
                ui.vertical(|ui| {
                    ui.set_width(CARD_WIDTH - 8.0);
                    
                    // Image with platforms on the right
                    let mut image_loaded = false;
                    ui.horizontal(|ui| {
                        if let Some(detail) = self.details.get(&avatar_id) {
                            if let Some(texture) = &detail.image {
                                let img = Image::new(texture)
                                    .fit_to_exact_size(Vec2::new(CARD_IMAGE_WIDTH, CARD_IMAGE_HEIGHT));
                                ui.add(img);
                                image_loaded = true;
                            } else {
                                // Check if this is a deleted/banned avatar
                                let is_deleted = detail.status.as_ref().map_or(false, |s| s == "Deleted");
                                
                                if is_deleted {
                                    // Show red cross for deleted/banned avatars
                                    let cross_rect = egui::Rect::from_min_size(
                                        ui.cursor().min,
                                        Vec2::new(CARD_IMAGE_WIDTH, CARD_IMAGE_HEIGHT)
                                    );
                                    
                                    // Draw background
                                    ui.painter().rect_filled(
                                        cross_rect,
                                        4.0,
                                        egui::Color32::from_gray(40)
                                    );
                                    
                                    // Draw red X
                                    let center = cross_rect.center();
                                    let size = 30.0;
                                    let stroke = egui::Stroke::new(4.0, egui::Color32::RED);
                                    
                                    // Draw X lines
                                    ui.painter().line_segment(
                                        [center - egui::Vec2::new(size, size), center + egui::Vec2::new(size, size)],
                                        stroke
                                    );
                                    ui.painter().line_segment(
                                        [center - egui::Vec2::new(size, -size), center + egui::Vec2::new(size, -size)],
                                        stroke
                                    );
                                    
                                    // Draw "(Removed)" text on the image
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
                            // YouTube-style loading placeholder
                            let loading_rect = egui::Rect::from_min_size(
                                ui.cursor().min,
                                Vec2::new(CARD_IMAGE_WIDTH, CARD_IMAGE_HEIGHT)
                            );
                            ui.painter().rect_filled(
                                loading_rect,
                                4.0,
                                egui::Color32::from_gray(40)
                            );
                            
                            // Animated loading spinner using circle_stroke
                            let time = ui.ctx().animate_value_with_time(
                                egui::Id::new(format!("loading_{}", avatar_id)),
                                0.0,
                                2.0
                            );
                            let center = loading_rect.center();
                            let radius = 12.0;
                            let start_angle = time * std::f32::consts::TAU;
                            let _end_angle = start_angle + std::f32::consts::FRAC_PI_2;
                            
                            // Draw arc using circle_stroke
                            ui.painter().circle_stroke(
                                center,
                                radius,
                                egui::Stroke::new(3.0, egui::Color32::WHITE),
                            );
                            
                            ui.allocate_exact_size(Vec2::new(CARD_IMAGE_WIDTH, CARD_IMAGE_HEIGHT), egui::Sense::hover());
                        }
                        
                        // Platform tags on the right
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
                        // Check if avatar is removed to disable buttons
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
}

impl eframe::App for AvatarBrowserApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_worker(ctx);

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
                        self.log("Refreshing current avatar");
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
                
                // Check if Enter was pressed in any search field
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
                
                // Add pagination controls here
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
                ui.spacing_mut().item_spacing = _prev_spacing;
                
                if self.avatars.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.label("No avatars match your filters. Try adjusting the search.");
                    });
                }
            });
        });

        egui::TopBottomPanel::bottom("footer").show(ctx, |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Debug").clicked() {
                    self.show_debug = !self.show_debug;
                }
                if ui.button("Quit").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                ui.label(&self.status);
            });
        });

        // Debug console window
        if self.show_debug {
            egui::Window::new("Debug Console")
                .collapsible(true)
                .resizable(true)
                .default_size([800.0, 600.0])
                .fixed_size([800.0, 600.0])
                .show(ctx, |ui| {
                    ui.heading("Debug Console");
                    ui.separator();
                    
                    // Log output area with fixed size
                    egui::ScrollArea::vertical()
                        .stick_to_bottom(true)
                        .max_height(400.0)
                        .show(ui, |ui| {
                            ui.set_min_size(egui::Vec2::new(760.0, 400.0));
                            ui.set_max_size(egui::Vec2::new(760.0, 400.0));
                            
                            // Use vertical layout to ensure proper spacing
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing = egui::Vec2::new(0.0, 2.0);
                                
                                for log_entry in &self.debug_log {
                                    ui.label(
                                        egui::RichText::new(log_entry)
                                            .monospace()
                                            .size(12.0)
                                    );
                                }
                            });
                        });
                    
                    ui.separator();
                    
                    // Control buttons
                    ui.horizontal(|ui| {
                        if ui.button("Clear Log").clicked() {
                            self.debug_log.clear();
                            self.log("Log cleared");
                        }
                        
                        if ui.button("Refresh Status").clicked() {
                            self.log(&format!("Status: {}", self.status));
                            self.log(&format!("Total avatars: {}", self.total_avatars));
                            self.log(&format!("Current page: {}", self.page + 1));
                            self.log(&format!("Filtered count: {}", self.filtered.len()));
                            self.log(&format!("Pending images: {}", self.pending.len()));
                            self.log(&format!("Texture cache size: {}", self.details.len()));
                        }
                        
                        if ui.button("Test Log").clicked() {
                            self.log("Test log message");
                        }
                    });
                    
                    ui.separator();
                    
                    // Manual command input
                    ui.horizontal(|ui| {
                        ui.label("CMD:");
                        let response = ui.text_edit_singleline(&mut String::new());
                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            // For now, just echo the command
                            // You can extend this to handle actual commands
                            self.log(&format!("> Command entered (not implemented yet)"));
                        }
                    });
                });
        }
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

fn main() {
    env_logger::init();
    let options = eframe::NativeOptions::default();
    if let Err(err) = eframe::run_native(
        "VRChat Avatar Browser",
        options,
        Box::new(|_cc| Box::new(AvatarBrowserApp::new())),
    ) {
        eprintln!("Failed to launch avatar browser: {err}");
    }
}
