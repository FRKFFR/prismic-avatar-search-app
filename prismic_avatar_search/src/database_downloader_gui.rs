use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    thread,
    time::Instant,
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

// Database is updated only once per day at 07:35:09 UTC+1
// This means we can safely cache for 24 hours and only check for updates once daily

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
    tags: Vec<String>,
    quest: bool,
    ios: bool,
    
    // Complete flag information from JavaScript analysis
    platform: u8,           // 1-PC, 2-Quest, 4-iOS
    impostor_platform: u8,  // Platform support for impostors
    pc_rating: u8,          // 0-5 rating (Unknown to Very Poor)
    quest_rating: u8,       // 0-5 rating
    ios_rating: u8,         // 0-5 rating
    content_warnings: u8,   // Content warning flags
    style_filter: u16,      // Style filter (actual tags!)
    marketplace: bool,      // In marketplace?
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvatarData {
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

// Helper functions for flag decoding based on JavaScript analysis
fn decode_style_filter(style_filter: u16) -> Vec<String> {
    let mut tags = Vec::new();
    
    if style_filter & 1 != 0 { tags.push("Pop Culture".to_string()); }
    if style_filter & 2 != 0 { tags.push("Furry".to_string()); }
    if style_filter & 4 != 0 { tags.push("Sci-Fi".to_string()); }
    if style_filter & 8 != 0 { tags.push("Anime".to_string()); }
    if style_filter & 16 != 0 { tags.push("Cartoon".to_string()); }
    if style_filter & 32 != 0 { tags.push("Objects".to_string()); }
    if style_filter & 64 != 0 { tags.push("Human".to_string()); }
    if style_filter & 128 != 0 { tags.push("Realistic".to_string()); }
    if style_filter & 256 != 0 { tags.push("Animal".to_string()); }
    if style_filter & 512 != 0 { tags.push("Fantasy".to_string()); }
    if style_filter & 1024 != 0 { tags.push("Fashion".to_string()); }
    
    tags
}

fn decode_content_warnings(content_warnings: u8) -> Vec<String> {
    let mut warnings = Vec::new();
    
    if content_warnings & 1 != 0 { warnings.push("Sexually suggestive".to_string()); }
    if content_warnings & 2 != 0 { warnings.push("Adult Language".to_string()); }
    if content_warnings & 4 != 0 { warnings.push("Graphic Violence".to_string()); }
    if content_warnings & 8 != 0 { warnings.push("Excessive Gore".to_string()); }
    if content_warnings & 16 != 0 { warnings.push("Extreme Horror".to_string()); }
    
    warnings
}

fn rating_to_string(rating: u8) -> String {
    match rating {
        0 => "Unknown".to_string(),
        1 => "Excellent".to_string(),
        2 => "Good".to_string(),
        3 => "Medium".to_string(),
        4 => "Poor".to_string(),
        5 => "Very Poor".to_string(),
        _ => "Invalid".to_string(),
    }
}

fn platform_to_string(platform: u8) -> String {
    match platform {
        1 => "PC".to_string(),
        2 => "Quest".to_string(),
        4 => "iOS".to_string(),
        _ => "Unknown".to_string(),
    }
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
    let flags = reader.read_int_array(file_avatars)?; // Capture flags for decoding
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

    // Process from newest to oldest (like JavaScript implementation)
    for i in (0..file_avatars).rev() {
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

        // Decode flags based on JavaScript analysis
        let f = flags[i] as u32;
        let avatar_flags = [
            (f >> 29) & 7,           // Platform
            (f >> 26) & 7,           // Impostor
            (f >> 17) & 7,           // PC Rating
            (f >> 20) & 7,           // Quest Rating
            (f >> 23) & 7,           // IOS Rating
            (f >> 12) & 31,          // Content Warnings
            (f >> 1) & 2047,         // Style Filter (TAGS!)
            (f) & 1                  // Marketplace
        ];

        // Decode style filter to get actual tags
        let decoded_tags = decode_style_filter(avatar_flags[6] as u16);
        
        entries.push(AvatarEntry {
            name,
            author,
            description,
            avatar_id: avatar_id.clone(),
            tags: decoded_tags, // Use decoded tags from flags
            quest: false,
            ios: false,
            
            // Complete flag information
            platform: avatar_flags[0] as u8,
            impostor_platform: avatar_flags[1] as u8,
            pc_rating: avatar_flags[2] as u8,
            quest_rating: avatar_flags[3] as u8,
            ios_rating: avatar_flags[4] as u8,
            content_warnings: avatar_flags[5] as u8,
            style_filter: avatar_flags[6] as u16,
            marketplace: avatar_flags[7] != 0,
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
    
    println!("Looking for cache files:");
    println!("  Data path: {}", data_path.display());
    println!("  Meta path: {}", meta_path.display());
    println!("  Data exists: {}", data_path.exists());
    println!("  Meta exists: {}", meta_path.exists());
    
    if !data_path.exists() || !meta_path.exists() {
        println!("Cache files don't exist");
        return Ok(None);
    }

    let meta: CacheMeta = serde_json::from_slice(&fs::read(&meta_path)?)?;
    println!("Cache meta: version={}, saved_at={}", meta.gist_version, meta.saved_at);
    println!("Expected version: {}", gist_version);
    
    if meta.gist_version != gist_version {
        println!("Version mismatch: cache={} vs expected={}", meta.gist_version, gist_version);
        return Ok(None);
    }

    // Check if cache is older than today's update time (07:35:09 UTC+1)
    match is_cache_older_than_daily_update(&meta.saved_at) {
        Ok(is_older) => {
            println!("Cache time check: is_older={}", is_older);
            if is_older {
                println!("Cache is older than daily update time");
                return Ok(None);
            }
        }
        Err(e) => {
            println!("Error checking cache time: {}", e);
            return Ok(None);
        }
    }

    let data: AvatarData = serde_json::from_slice(&fs::read(&data_path)?)?;
    println!("Cache loaded successfully");
    Ok(Some(data))
}

fn is_cache_older_than_daily_update(cache_time: &str) -> Result<bool> {
    use chrono::{DateTime, Utc, NaiveTime};
    
    // Parse cache time with better error handling
    let cache_dt: DateTime<Utc> = cache_time.parse()
        .with_context(|| format!("Failed to parse cache time: '{}'", cache_time))?;
    
    // Get current time
    let now_utc = Utc::now();
    
    // Define update time (07:35:09 UTC+1 = 06:35:09 UTC)
    let update_hour = 6; // UTC time (07:35:09 UTC+1)
    let update_minute = 35;
    
    // Get today's update time in UTC
    let today_update = now_utc.date_naive()
        .and_hms_opt(update_hour, update_minute, 9)
        .ok_or_else(|| anyhow!("Invalid time"))?
        .and_utc();
    
    // If current time is after today's update, check if cache is from before today's update
    if now_utc.time() > NaiveTime::from_hms_opt(update_hour, update_minute, 9).unwrap() {
        // After update time today - cache must be from after today's update
        Ok(cache_dt < today_update)
    } else {
        // Before update time today - cache must be from after yesterday's update
        let yesterday_update = (today_update.date_naive() - chrono::Duration::days(1))
            .and_hms_opt(update_hour, update_minute, 9)
            .ok_or_else(|| anyhow!("Invalid time"))?
            .and_utc();
        
        Ok(cache_dt < yesterday_update)
    }
}

fn download_bytes_with_speed(client: &Client, url: &str, tx: &Sender<UiMessage>) -> Result<Vec<u8>> {
    let start_time = Instant::now();
    
    // Send initial speed update
    let _ = tx.send(UiMessage::DownloadSpeed {
        speed: 0.0,
        downloaded: 0,
        total: 0,
    });
    
    let resp = client
        .get(url)
        .send()
        .with_context(|| format!("GET {}", url))?;
    
    if !resp.status().is_success() {
        return Err(anyhow!("Failed to download {}: status {}", url, resp.status()));
    }
    
    let total_size = resp.content_length().unwrap_or(0);
    let bytes = resp.bytes()?;
    let downloaded = bytes.len() as u64;
    
    // Calculate and send final speed
    let elapsed = start_time.elapsed().as_secs_f64();
    let speed = if elapsed > 0.0 { downloaded as f64 / elapsed } else { 0.0 };
    
    let _ = tx.send(UiMessage::DownloadSpeed {
        speed,
        downloaded,
        total: total_size as u64,
    });
    
    Ok(bytes.to_vec())
}

fn download_with_fallback(client: &Client, index: usize, tx: &Sender<UiMessage>) -> Result<Vec<u8>> {
    let primary = URLS[index];
    match download_bytes_with_speed(client, primary, tx) {
        Ok(bytes) => Ok(bytes),
        Err(err) => {
            warn!("Primary download failed ({}): {}", primary, err);
            warn!("Trying backup URL...");
            match download_bytes_with_speed(client, BACKUP_URLS[index], tx) {
                Ok(bytes) => Ok(bytes),
                Err(backup_err) => {
                    Err(anyhow!("Both primary and backup downloads failed. Primary: {}, Backup: {}", err, backup_err))
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum UiMessage {
    Status(String),
    Progress { step: usize, value: f32 },
    DownloadSpeed { speed: f64, downloaded: u64, total: u64 }, // New: download speed info
    Completed { data: AvatarData, cached: bool },
    Error(String),
}

pub fn run_worker(tx: Sender<UiMessage>) {
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
    tx: &Sender<UiMessage>,
    gist_version: Option<String>  // Add gist_version parameter
) -> Result<(u32, u32, String)> {
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
    let last_update = processed.data.last_update.clone(); // Use actual date from database
    let total_entries = processed.data.entries.len();

    for (i, entry) in processed.data.entries.into_iter().enumerate() {
        // Update progress periodically
        if i % 10000 == 0 {
            let progress = (i as f32) / (total_entries as f32);
            let _ = tx.send(UiMessage::Progress { step: 3, value: progress });
        }

        author_set.insert(entry.author.clone());
        
        serde_json::to_writer(&mut writer, &entry)?;
        writer.write_all(b"\n")?;
    }
    
    writer.flush()?;
    
    // Calculate counts for cache files
    let avatar_count = processed.data.avatar_count;
    let author_count = author_set.len() as u32;
    
    // Load sample entries from the file for display
    let entries_path = cache_dir.join("avatar_entries.ndjson");
    let sample_entries = if entries_path.exists() {
        let content = fs::read_to_string(&entries_path)?;
        content.lines()
            .take(50) // Only load first 50 entries
            .filter_map(|line| serde_json::from_str::<AvatarEntry>(line).ok())
            .collect()
    } else {
        Vec::new()
    };
    
    // Create cache files BEFORE sending completion message
    let data_path = cache_dir.join("avatar_data.json");
    let avatar_data = AvatarData {
        avatar_count,
        author_count,
        last_update: last_update.clone(),
        entries: Vec::new(), // Don't store all entries in cache file
    };
    
    // Write avatar_data.json
    let json_bytes = serde_json::to_vec_pretty(&avatar_data)?;
    fs::write(data_path, json_bytes)?;
    
    // Create cache_meta.json BEFORE sending completion message
    if let Some(ref version) = gist_version {
        let meta = CacheMeta {
            gist_version: version.to_string(),
            saved_at: Utc::now().to_rfc3339(),
        };
        let meta_path = cache_dir.join("cache_meta.json");
        let meta_bytes = serde_json::to_vec_pretty(&meta)?;
        fs::write(meta_path, meta_bytes)?;
    }
    
    println!("All cache files created successfully");
    
    // Send completion with sample data
    tx.send(UiMessage::Completed {
        data: AvatarData {
            avatar_count,
            author_count,
            last_update: last_update.clone(),
            entries: sample_entries,
        },
        cached: false,
    })?;
    
    Ok((processed.data.avatar_count, author_set.len() as u32, last_update))
}

fn fetch_and_process(tx: &Sender<UiMessage>) -> Result<()> {
    tx.send(UiMessage::Status("Initializing client".into()))?;
    let client = Client::builder()
        .user_agent("vrchat-avatar-gui/0.1")
        .build()?;

    let cache_dir = PathBuf::from("cache");
    println!("Current working directory: {:?}", std::env::current_dir());
    println!("Cache directory: {}", cache_dir.display());
    
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
    let main_bytes = download_with_fallback(&client, 0, &tx)?;
    tx.send(UiMessage::Progress { step: 0, value: 1.0 })?;

    tx.send(UiMessage::Status("Downloading quest database".into()))?;
    let quest_bytes = download_with_fallback(&client, 1, &tx)?;
    tx.send(UiMessage::Progress { step: 1, value: 1.0 })?;

    tx.send(UiMessage::Status("Downloading iOS database".into()))?;
    let ios_bytes = download_with_fallback(&client, 2, &tx)?;
    tx.send(UiMessage::Progress { step: 2, value: 1.0 })?;

    tx.send(UiMessage::Status("Processing data".into()))?;
    
    // Process and stream directly to files to reduce memory usage
    let (avatar_count, author_count, last_update) = process_and_stream_databases(
        &cache_dir, 
        main_bytes, 
        quest_bytes, 
        ios_bytes,
        &tx,
        gist_version.clone()  // Pass gist_version
    )?;
    
    tx.send(UiMessage::Progress { step: 3, value: 1.0 })?;

    // All cache files are already created in process_and_stream_databases
    println!("Database processing and caching complete");

    Ok(())
}

#[derive(Debug)]
pub struct AppState {
    receiver: Receiver<UiMessage>,
    status: String,
    progress: [f32; 4],
    result: Option<AvatarData>,
    error: Option<String>,
    cached: bool,
    download_speed: f64,      // bytes per second
    downloaded_bytes: u64,   // total downloaded bytes
    total_bytes: u64,       // total file size
}

impl AppState {
    pub fn new(receiver: Receiver<UiMessage>) -> Self {
        Self {
            receiver,
            status: "Initializing...".into(),
            progress: [0.0; 4],
            result: None,
            error: None,
            cached: false,
            download_speed: 0.0,
            downloaded_bytes: 0,
            total_bytes: 0,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.result.is_some()
    }
    
    pub fn poll_messages(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.receiver.try_recv() {
            match msg {
                UiMessage::Status(text) => self.status = text,
                UiMessage::Progress { step, value } => {
                    if step < self.progress.len() {
                        self.progress[step] = value.max(0.0).min(1.0);
                    }
                }
                UiMessage::DownloadSpeed { speed, downloaded, total } => {
                    self.download_speed = speed;
                    self.downloaded_bytes = downloaded;
                    self.total_bytes = total;
                }
                UiMessage::Completed { data, cached } => {
                    self.result = Some(data);
                    self.cached = cached;
                    self.status = if cached {
                        "Loaded from cache.".into()
                    } else {
                        "Download complete.".into()
                    };
                    // Reset download speed when complete
                    self.download_speed = 0.0;
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
            
            // Show download speed information when downloading
            if self.download_speed > 0.0 {
                ui.horizontal(|ui| {
                    let speed_text = if self.download_speed >= 1_000_000.0 {
                        format!("{:.1} MB/s", self.download_speed / 1_000_000.0)
                    } else if self.download_speed >= 1_000.0 {
                        format!("{:.1} KB/s", self.download_speed / 1_000.0)
                    } else {
                        format!("{:.0} B/s", self.download_speed)
                    };
                    
                    let progress_text = if self.total_bytes > 0 {
                        let percent = (self.downloaded_bytes as f64 / self.total_bytes as f64) * 100.0;
                        format!("{} / {} ({:.1}%)", 
                            format_bytes(self.downloaded_bytes), 
                            format_bytes(self.total_bytes), 
                            percent)
                    } else {
                        format_bytes(self.downloaded_bytes)
                    };
                    
                    ui.label(format!("⬇️ {} | {}", speed_text, progress_text));
                });
            }

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
                egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                    ui.add_space(20.0);
                    TableBuilder::new(ui)
                        .striped(true)
                        .column(Column::initial(200.0)) // Name
                        .column(Column::initial(150.0)) // Author
                        .column(Column::initial(100.0)) // Platforms
                        .column(Column::initial(120.0)) // Tags
                        .column(Column::initial(80.0))  // PC Rating
                        .column(Column::initial(80.0))  // Quest Rating
                        .column(Column::initial(80.0))  // iOS Rating
                        .column(Column::initial(100.0)) // Content Warnings
                        .column(Column::initial(60.0))  // Marketplace
                        .header(25.0, |mut header| {
                            header.col(|ui| {
                                ui.strong("Name");
                            });
                            header.col(|ui| {
                                ui.strong("Author");
                            });
                            header.col(|ui| {
                                ui.strong("Platforms");
                            });
                            header.col(|ui| {
                                ui.strong("Tags");
                            });
                            header.col(|ui| {
                                ui.strong("PC Rating");
                            });
                            header.col(|ui| {
                                ui.strong("Quest Rating");
                            });
                            header.col(|ui| {
                                ui.strong("iOS Rating");
                            });
                            header.col(|ui| {
                                ui.strong("Content Warnings");
                            });
                            header.col(|ui| {
                                ui.strong("Marketplace");
                            });
                        })
                        .body(|mut body| {
                            for entry in result.entries.iter().take(50) {
                                body.row(30.0, |mut row| {
                                    // Name
                                    row.col(|ui| {
                                        ui.label(&entry.name);
                                    });
                                    
                                    // Author
                                    row.col(|ui| {
                                        ui.label(&entry.author);
                                    });
                                    
                                    // Platforms
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
                                    
                                    // Tags
                                    row.col(|ui| {
                                        let tags_preview = if entry.tags.len() > 3 {
                                            format!("{}, ...", entry.tags[..3].join(", "))
                                        } else {
                                            entry.tags.join(", ")
                                        };
                                        ui.label(tags_preview);
                                    });
                                    
                                    // PC Rating
                                    row.col(|ui| {
                                        ui.label(rating_to_string(entry.pc_rating));
                                    });
                                    
                                    // Quest Rating
                                    row.col(|ui| {
                                        ui.label(rating_to_string(entry.quest_rating));
                                    });
                                    
                                    // iOS Rating
                                    row.col(|ui| {
                                        ui.label(rating_to_string(entry.ios_rating));
                                    });
                                    
                                    // Content Warnings
                                    row.col(|ui| {
                                        let warnings = decode_content_warnings(entry.content_warnings);
                                        let warnings_preview = if warnings.len() > 2 {
                                            format!("{}, ...", warnings[..2].join(", "))
                                        } else {
                                            warnings.join(", ")
                                        };
                                        ui.label(warnings_preview);
                                    });
                                    
                                    // Marketplace
                                    row.col(|ui| {
                                        ui.label(if entry.marketplace { "Yes" } else { "No" });
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

// Helper function to format bytes into human readable format
fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;
    
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    
    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}

