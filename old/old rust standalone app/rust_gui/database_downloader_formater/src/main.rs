use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use eframe::egui::{self, ProgressBar};
use egui_extras::{Column, TableBuilder};
use flume::{Receiver, Sender};
use log::warn;
use regex::Regex;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

const URLS: [&str; 3] = [
    "https://gist.githubusercontent.com/Mwr247/a80c1f9060fc4fd46a8f00d589c47c5a/raw/pasavtrdb.txt",
    "https://gist.githubusercontent.com/Mwr247/a80c1f9060fc4fd46a8f00d589c47c5a/raw/pasavtrdb_qst.txt",
    "https://gist.githubusercontent.com/Mwr247/a80c1f9060fc4fd46a8f00d589c47c5a/raw/pasavtrdb_ios.txt",
];

const BACKUP_URLS: [&str; 3] = [
    "https://prismic.net/vrc/pasavtrdb.txt",
    "https://prismic.net/vrc/pasavtrdb_qst.txt",
    "https://prismic.net/vrc/pasavtrdb.txt",
];

const STATIC_BYTES: [u8; 16] = [208, 29, 107, 36, 251, 69, 122, 14, 67, 204, 171, 246, 106, 38, 183, 224];

const STEP_LABELS: [&str; 4] = [
    "Downloading Main Database",
    "Downloading Quest Database",
    "Downloading iOS Database",
    "Processing Data",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AvatarEntry {
    name: String,
    author: String,
    description: String,
    avatar_id: String,
    quest: bool,
    ios: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AvatarData {
    avatar_count: u32,
    author_count: u32,
    last_update: String,
    entries: Vec<AvatarEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheMeta {
    gist_version: String,
    saved_at: String,
}

struct ProcessedData {
    data: AvatarData,
    index: HashMap<String, usize>,
}

struct Reader {
    data: Vec<u8>,
    position: usize,
}

impl Reader {
    fn new(data: Vec<u8>) -> Self {
        Self { data, position: 0 }
    }

    fn read_byte(&mut self) -> Result<u8> {
        if self.position >= self.data.len() {
            return Err(anyhow!("Attempted to read beyond end of data"));
        }
        let value = self.data[self.position];
        self.position += 1;
        Ok(value)
    }

    fn read_bytes(&mut self, amount: usize) -> Result<Vec<u8>> {
        if self.position + amount > self.data.len() {
            return Err(anyhow!("Attempted to read beyond end of data"));
        }
        let bytes = self.data[self.position..self.position + amount].to_vec();
        self.position += amount;
        Ok(bytes)
    }

    fn read_int24(&mut self) -> Result<u32> {
        let bytes = self.read_bytes(3)?;
        Ok(((bytes[0] as u32) << 16) | ((bytes[1] as u32) << 8) | bytes[2] as u32)
    }

    fn read_int_array(&mut self, n: usize) -> Result<Vec<i32>> {
        if self.position + n * 4 > self.data.len() {
            return Err(anyhow!("Attempted to read beyond end of data"));
        }
        let mut result = Vec::with_capacity(n);
        for _ in 0..n {
            let slice = &self.data[self.position..self.position + 4];
            result.push(i32::from_le_bytes(slice.try_into().unwrap()));
            self.position += 4;
        }
        Ok(result)
    }

    fn remaining(&self) -> usize {
        self.data.len() - self.position
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

    let date_arr = reader.read_bytes(2)?;
    let date_num = (((date_arr[0] as u32) << 8) + date_arr[1] as u32) >> 3;
    let year = ((date_num >> 9) + 16) as u32;
    let month = ((date_num >> 5) & 15) as u32;
    let day = (date_num & 31) as u32;
    let last_update = format!("20{year:02}-{month:02}-{day:02}");

    let file_avatars = reader.read_int24()? as usize;
    reader.read_int24()?; // file authors, not needed

    let flag_size = reader.read_byte()? as usize;
    let random_bytes = reader.read_bytes(16)?;
    let dynamic_bytes: Vec<u8> = random_bytes
        .iter()
        .zip(STATIC_BYTES.iter())
        .map(|(a, b)| a ^ b)
        .collect();

    let avatar_ids = reader.read_bytes(file_avatars * 16)?;
    let _flag_data_size = file_avatars * flag_size;
    let _flags = reader.read_int_array(file_avatars)?; // retained for parity with JS
    let author_ids = reader.read_int_array(file_avatars)?;

    let strings = String::from_utf8(reader.read_bytes(reader.remaining())?)?;
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

    if avatar_lines.len() < file_avatars {
        return Err(anyhow!("Avatar list shorter than expected"));
    }

    let mut entries = Vec::with_capacity(file_avatars);
    let mut index = HashMap::with_capacity(file_avatars);

    for i in 0..file_avatars {
        let block = avatar_ids[i * 16..(i + 1) * 16].to_vec();
        let avatar_id = decode_avatar_id(block, &dynamic_bytes)?;

        let name_desc: Vec<&str> = avatar_lines[i].split('\t').collect();
        let name = name_desc
            .get(0)
            .map(|s| s.chars().rev().collect())
            .unwrap_or_default();
        let description = name_desc
            .get(1)
            .map(|s| s.chars().rev().collect())
            .unwrap_or_default();

        let author_idx = (author_ids[i] & 524_287) as usize;
        let author = author_names
            .get(author_idx)
            .cloned()
            .unwrap_or_else(|| "Unknown".to_string());

        entries.push(AvatarEntry {
            name,
            author,
            description,
            avatar_id: avatar_id.clone(),
            quest: false,
            ios: false,
        });
        index.insert(avatar_id, i);
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
    let mut reader = Reader::new(bytes);
    if reader.remaining() == 0 {
        return Err(anyhow!("Data has length zero"));
    }

    if reader.read_bytes(3)? != b"PAS" {
        return Err(anyhow!("PAS Header not found"));
    }

    reader.read_bytes(2 + 3 + 3 + 2)?; // platform, avatars, authors, date
    let file_avatars = reader.read_int24()? as usize;
    reader.read_bytes(3 + 1)?; // fileAuthors + flagSize

    let random_bytes = reader.read_bytes(16)?;
    let dynamic_bytes: Vec<u8> = random_bytes
        .iter()
        .zip(STATIC_BYTES.iter())
        .map(|(a, b)| a ^ b)
        .collect();
    let avatar_ids = reader.read_bytes(file_avatars * 16)?;

    let mut ids = Vec::with_capacity(file_avatars);
    for i in 0..file_avatars {
        let block = avatar_ids[i * 16..(i + 1) * 16].to_vec();
        ids.push(decode_avatar_id(block, &dynamic_bytes)?);
    }

    Ok(ids)
}

fn mark_platform(processed: &mut ProcessedData, ids: &[String], flag: PlatformFlag) {
    let mut missing = 0;
    for id in ids {
        if let Some(&idx) = processed.index.get(id) {
            let entry = processed.data.entries.get_mut(idx).unwrap();
            match flag {
                PlatformFlag::Quest => entry.quest = true,
                PlatformFlag::Ios => entry.ios = true,
            }
        } else {
            missing += 1;
        }
    }

    if missing > 0 {
        warn!(
            "{} platform IDs missing from primary list", missing
        );
    }
}

enum PlatformFlag {
    Quest,
    Ios,
}

fn gist_id_from_url(url: &str) -> Option<String> {
    let re = Regex::new(r"/([0-9a-fA-F]+)/raw").unwrap();
    re.captures(url).and_then(|caps| caps.get(1)).map(|m| m.as_str().to_string())
}

fn fetch_gist_version(client: &Client, gist_id: &str) -> Option<String> {
    let url = format!("https://api.github.com/gists/{}/commits", gist_id);
    client
        .get(url)
        .send()
        .ok()
        .and_then(|resp| resp.json::<serde_json::Value>().ok())
        .and_then(|commits| commits.get(0)?.get("version")?.as_str().map(|s| s.to_string()))
}

fn cache_paths(cache_dir: &Path) -> (PathBuf, PathBuf, PathBuf) {
    (
        cache_dir.join("avatar_data.json"),
        cache_dir.join("cache_meta.json"),
        cache_dir.join("avatar_entries.ndjson"),
    )
}

fn load_cache(cache_dir: &Path, gist_version: &str) -> Result<Option<AvatarData>> {
    let (data_path, meta_path, _) = cache_paths(cache_dir);
    if !data_path.exists() || !meta_path.exists() {
        return Ok(None);
    }

    let meta: CacheMeta = serde_json::from_slice(&fs::read(&meta_path)?)?;
    if meta.gist_version != gist_version {
        return Ok(None);
    }

    let data: AvatarData = serde_json::from_slice(&fs::read(&data_path)?)?;
    Ok(Some(data))
}

fn download_bytes(client: &Client, url: &str) -> Result<Vec<u8>> {
    let resp = client
        .get(url)
        .send()
        .with_context(|| format!("GET {}", url))?;
    if !resp.status().is_success() {
        return Err(anyhow!("Failed to download {}: status {}", url, resp.status()));
    }
    Ok(resp.bytes()?.to_vec())
}

fn download_with_fallback(client: &Client, index: usize) -> Result<Vec<u8>> {
    let primary = URLS[index];
    match download_bytes(client, primary) {
        Ok(bytes) => Ok(bytes),
        Err(err) => {
            warn!("Primary download failed ({}): {}", primary, err);
            download_bytes(client, BACKUP_URLS[index])
        }
    }
}

enum UiMessage {
    Status(String),
    Progress { step: usize, value: f32 },
    Completed { data: AvatarData, cached: bool },
    Error(String),
}

fn run_worker(tx: Sender<UiMessage>) {
    thread::spawn(move || {
        if let Err(err) = fetch_and_process(&tx) {
            let _ = tx.send(UiMessage::Error(err.to_string()));
        }
    });
}

fn process_and_stream_databases(
    cache_dir: &Path,
    main_bytes: Vec<u8>,
    quest_bytes: Vec<u8>,
    ios_bytes: Vec<u8>,
    tx: &Sender<UiMessage>
) -> Result<()> {
    use std::io::{BufWriter, Write};
    
    // Ensure cache directory exists
    fs::create_dir_all(cache_dir)?;
    
    // Use the existing parsing functions that work
    let mut processed = parse_main_database(main_bytes)?;
    let quest_ids = parse_aux_database(quest_bytes)?;
    let ios_ids = parse_aux_database(ios_bytes)?;

    mark_platform(&mut processed, &quest_ids, PlatformFlag::Quest);
    mark_platform(&mut processed, &ios_ids, PlatformFlag::Ios);

    // Stream entries to file instead of keeping them all in memory
    let entries_path = cache_dir.join("avatar_entries.ndjson");
    let file = fs::File::create(&entries_path)?;
    let mut writer = BufWriter::new(file);

    let mut author_set = std::collections::HashSet::new();
    let mut last_update = String::new();
    let total_entries = processed.data.entries.len();

    for (i, entry) in processed.data.entries.into_iter().enumerate() {
        // Update progress periodically
        if i % 10000 == 0 {
            let progress = (i as f32) / (total_entries as f32);
            let _ = tx.send(UiMessage::Progress { step: 3, value: progress });
        }

        author_set.insert(entry.author.clone());
        // Use a default timestamp since AvatarEntry doesn't have updated_at field
        if last_update.is_empty() {
            last_update = "0".to_string();
        }
        
        serde_json::to_writer(&mut writer, &entry)?;
        writer.write_all(b"\n")?;
    }
    
    writer.flush()?;
    
    // Send completion with minimal data
    tx.send(UiMessage::Completed {
        data: AvatarData {
            avatar_count: processed.data.avatar_count,
            author_count: author_set.len() as u32,
            last_update,
            entries: Vec::new(), // Don't load all entries into memory
        },
        cached: false,
    })?;
    
    Ok(())
}

fn fetch_and_process(tx: &Sender<UiMessage>) -> Result<()> {
    tx.send(UiMessage::Status("Initializing client".into()))?;
    let client = Client::builder()
        .user_agent("vrchat-avatar-gui/0.1")
        .timeout(Duration::from_secs(30))
        .build()?;

    let cache_dir = PathBuf::from("cache");
    let gist_id = gist_id_from_url(URLS[0]).ok_or_else(|| anyhow!("Unable to parse gist ID"))?;
    tx.send(UiMessage::Status("Checking cache".into()))?;
    let gist_version = fetch_gist_version(&client, &gist_id);

    if let Some(ref version) = gist_version {
        if let Some(data) = load_cache(&cache_dir, version)? {
            tx.send(UiMessage::Status("Loaded cached database".into()))?;
            tx.send(UiMessage::Completed { data, cached: true })?;
            return Ok(());
        }
    }

    tx.send(UiMessage::Status("Downloading main database".into()))?;
    let main_bytes = download_with_fallback(&client, 0)?;
    tx.send(UiMessage::Progress { step: 0, value: 1.0 })?;

    tx.send(UiMessage::Status("Downloading quest database".into()))?;
    let quest_bytes = download_with_fallback(&client, 1)?;
    tx.send(UiMessage::Progress { step: 1, value: 1.0 })?;

    tx.send(UiMessage::Status("Downloading iOS database".into()))?;
    let ios_bytes = download_with_fallback(&client, 2)?;
    tx.send(UiMessage::Progress { step: 2, value: 1.0 })?;

    tx.send(UiMessage::Status("Processing data".into()))?;
    
    // Process and stream directly to files to reduce memory usage
    process_and_stream_databases(
        &cache_dir, 
        main_bytes, 
        quest_bytes, 
        ios_bytes,
        &tx
    )?;
    
    tx.send(UiMessage::Progress { step: 3, value: 1.0 })?;

    // Create metadata without loading all entries back into memory
    if let Some(ref version) = gist_version {
        let _entries_path = cache_dir.join("avatar_entries.ndjson");
        let meta = CacheMeta {
            gist_version: version.to_string(),
            saved_at: Utc::now().to_rfc3339(),
        };
        let meta_path = cache_dir.join("cache_meta.json");
        fs::write(meta_path, serde_json::to_vec_pretty(&meta)?)?;
    }

    Ok(())
}

struct AppState {
    receiver: Receiver<UiMessage>,
    status: String,
    progress: [f32; 4],
    result: Option<AvatarData>,
    error: Option<String>,
    cached: bool,
}

impl AppState {
    fn new(receiver: Receiver<UiMessage>) -> Self {
        Self {
            receiver,
            status: "Initializing...".into(),
            progress: [0.0; 4],
            result: None,
            error: None,
            cached: false,
        }
    }

    fn poll_messages(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.receiver.try_recv() {
            match msg {
                UiMessage::Status(text) => self.status = text,
                UiMessage::Progress { step, value } => {
                    if step < self.progress.len() {
                        self.progress[step] = value.max(0.0).min(1.0);
                    }
                }
                UiMessage::Completed { data, cached } => {
                    self.result = Some(data);
                    self.cached = cached;
                    self.status = if cached {
                        "Loaded from cache.".into()
                    } else {
                        "Download complete.".into()
                    };
                }
                UiMessage::Error(err) => self.error = Some(err),
            }
        }
        ctx.request_repaint();
    }
}

impl eframe::App for AppState {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_messages(ctx);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("VRChat Avatar Database Downloader (Rust)");
            ui.separator();
            ui.label(format!("Status: {}", self.status));

            if let Some(err) = &self.error {
                ui.colored_label(egui::Color32::RED, format!("Error: {}", err));
                ui.separator();
            }

            for (idx, label) in STEP_LABELS.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(*label);
                    ui.add(ProgressBar::new(self.progress[idx]).show_percentage());
                });
            }

            ui.separator();
            if let Some(result) = &self.result {
                ui.label(format!("Avatars: {}", result.avatar_count));
                ui.label(format!("Authors: {}", result.author_count));
                ui.label(format!("Last Update: {}", result.last_update));
                ui.label(if self.cached { "Source: Cache" } else { "Source: Fresh" });

                ui.separator();
                ui.label("Sample entries:");
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    ui.add_space(20.0);
                    TableBuilder::new(ui)
                        .striped(true)
                        .column(Column::remainder())
                        .column(Column::remainder())
                        .column(Column::remainder())
                        .header(20.0, |mut header| {
                            header.col(|ui| {
                                ui.strong("Name");
                            });
                            header.col(|ui| {
                                ui.strong("Author");
                            });
                            header.col(|ui| {
                                ui.strong("Platforms");
                            });
                        })
                        .body(|mut body| {
                            for entry in result.entries.iter().take(50) {
                                body.row(20.0, |mut row| {
                                    row.col(|ui| {
                                        ui.label(&entry.name);
                                    });
                                    row.col(|ui| {
                                        ui.label(&entry.author);
                                    });
                                    row.col(|ui| {
                                        let mut platforms = vec!["PC".to_string()];
                                        if entry.quest {
                                            platforms.push("Quest".into());
                                        }
                                        if entry.ios {
                                            platforms.push("iOS".into());
                                        }
                                        ui.label(platforms.join(", "));
                                    });
                                });
                            }
                        });
                });
            }

            ui.separator();
            if ui.button("Close").clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }
}

fn main() {
    env_logger::init();
    let (tx, rx) = flume::bounded(128);
    run_worker(tx);

    let options = eframe::NativeOptions::default();
    if let Err(err) = eframe::run_native(
        "VRChat Avatar Database",
        options,
        Box::new(|_cc| Box::new(AppState::new(rx))),
    ) {
        eprintln!("Failed to launch downloader: {err}");
    }
}

