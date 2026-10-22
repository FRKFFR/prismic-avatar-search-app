use std::collections::HashMap;
use std::path::Path;
use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use reqwest::Client;
use rusqlite::{params, Connection};

use crate::models::{AvatarEntry, SyncProgress};

const URL_MAIN: &str = "https://gist.githubusercontent.com/Prismic247/930d08f34c61e4282992cdb3afbafca0/raw/pasavtrdb.txt";
const URL_QUEST: &str = "https://gist.githubusercontent.com/Prismic247/930d08f34c61e4282992cdb3afbafca0/raw/pasavtrdb_qst.txt";
const URL_IOS: &str = "https://gist.githubusercontent.com/Prismic247/930d08f34c61e4282992cdb3afbafca0/raw/pasavtrdb_ios.txt";

const BACKUP_URL_MAIN: &str = "https://prismic.net/vrc/pas/pasavtrdb.txt";
const BACKUP_URL_QUEST: &str = "https://prismic.net/vrc/pas/pasavtrdb_qst.txt";
const BACKUP_URL_IOS: &str = "https://prismic.net/vrc/pas/pasavtrdb_ios.txt";

const STATIC_BYTES: [u8; 16] = [208, 29, 107, 36, 251, 69, 122, 14, 67, 204, 171, 246, 106, 38, 183, 224];

pub struct Reader {
    data: Vec<u8>,
    position: usize,
}

impl Reader {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data, position: 0 }
    }

    pub fn read_bytes(&mut self, n: usize) -> Result<Vec<u8>> {
        if self.position + n > self.data.len() {
            return Err(anyhow!("Attempted to read beyond end of data"));
        }
        let bytes = self.data[self.position..self.position + n].to_vec();
        self.position += n;
        Ok(bytes)
    }

    pub fn read_byte(&mut self) -> Result<u8> {
        if self.position >= self.data.len() {
            return Err(anyhow!("Attempted to read beyond end of data"));
        }
        let byte = self.data[self.position];
        self.position += 1;
        Ok(byte)
    }

    pub fn read_int24(&mut self) -> Result<u32> {
        let bytes = self.read_bytes(3)?;
        Ok(((bytes[0] as u32) << 16) | ((bytes[1] as u32) << 8) | bytes[2] as u32)
    }

    pub fn read_int_array(&mut self, n: usize) -> Result<Vec<i32>> {
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

    pub fn remaining(&self) -> usize {
        self.data.len() - self.position
    }
}

pub fn decode_avatar_id(mut crypt: Vec<u8>, iv: &[u8]) -> Result<String> {
    if crypt.len() != 16 || iv.len() != 16 {
        return Err(anyhow!("Avatar ID blocks must be 16 bytes"));
    }

    for i in (0..crypt.len()).rev() {
        let prev = if i == 0 { crypt.len() - 1 } else { i - 1 };
        let k = crypt[i] ^ crypt[prev] ^ iv[i];
        crypt[i] = k;
    }

    let hex_string: String = crypt.iter().map(|b| format!("{:02x}", b)).collect();
    let reversed: String = hex_string.chars().rev().collect();

    if reversed.len() != 32 {
        return Err(anyhow!("Decoded avatar ID must be 32 hex chars, got {}", reversed.len()));
    }

    let mut formatted = reversed;
    formatted.insert(8, '-');
    formatted.insert(13, '-');
    formatted.insert(18, '-');
    formatted.insert(23, '-');

    Ok(format!("avtr_{}", formatted))
}

pub fn decode_style_filter(style_filter: u16) -> Vec<String> {
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

#[allow(dead_code)]
pub struct ProcessedData {
    pub entries: Vec<AvatarEntry>,
    pub index: HashMap<String, usize>,
    pub last_update: String,
    pub avatar_count: u32,
    pub author_count: u32,
}

pub fn parse_main_database(bytes: Vec<u8>) -> Result<ProcessedData> {
    let mut reader = Reader::new(bytes);
    if reader.remaining() == 0 {
        return Err(anyhow!("Data has length zero"));
    }

    if reader.read_bytes(3)? != b"PAS" {
        return Err(anyhow!("PAS Header not found"));
    }

    let meta_bytes = reader.read_bytes(2)?;
    let version = (meta_bytes[0] >> 5) & 31;
    let _platform = meta_bytes[0] & 7;
    let avatar_count = reader.read_int24()?;
    let author_count = reader.read_int24()?;

    let date_arr = reader.read_bytes(2)?;
    let date_num = (((date_arr[0] as u32) << 8) + date_arr[1] as u32) >> 3;
    let year = ((date_num >> 9) + 16) as u32;
    let month = ((date_num >> 5) & 15) as u32;
    let day = (date_num & 31) as u32;
    let last_update = format!("20{year:02}-{month:02}-{day:02}");

    let file_avatars = reader.read_int24()? as usize;
    reader.read_int24()?; // file authors

    let flag_size = reader.read_byte()? as usize;
    let random_bytes = reader.read_bytes(16)?;
    let dynamic_bytes: Vec<u8> = if version == 0 {
        random_bytes
            .iter()
            .zip(STATIC_BYTES.iter())
            .map(|(a, b)| a ^ b)
            .collect()
    } else {
        random_bytes
            .iter()
            .enumerate()
            .map(|(i, a)| a ^ (STATIC_BYTES[(i + version as usize) % 16].wrapping_add((flag_size % 256) as u8)))
            .collect()
    };

    let avatar_ids = reader.read_bytes(file_avatars * 16)?;
    let flags = reader.read_int_array(file_avatars)?;
    let author_ids = reader.read_int_array(file_avatars)?;

    let strings = String::from_utf8(reader.read_bytes(reader.remaining())?)?;
    let mut parts = strings.split('\n');
    let authors_block = parts.next().ok_or_else(|| anyhow!("Malformed authors block"))?;
    let avatars_block = parts.next().ok_or_else(|| anyhow!("Malformed avatars block"))?;

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

        let f = flags[i] as u32;
        let avatar_flags = [
            (f >> 29) & 7,   // Platform
            (f >> 26) & 7,   // Impostor
            (f >> 17) & 7,   // PC Rating
            (f >> 20) & 7,   // Quest Rating
            (f >> 23) & 7,   // IOS Rating
            (f >> 12) & 31,  // Content Warnings
            (f >> 1) & 2047, // Style Filter (TAGS)
            (f) & 1,         // Marketplace
        ];

        let decoded_tags = decode_style_filter(avatar_flags[6] as u16);

        entries.push(AvatarEntry {
            id: avatar_id.clone(),
            name,
            author,
            description,
            avatar_id: avatar_id.clone(),
            tags: decoded_tags,
            quest: false,
            ios: false,
            platform: avatar_flags[0] as u8,
            impostor_platform: avatar_flags[1] as u8,
            pc_rating: avatar_flags[2] as u8,
            quest_rating: avatar_flags[3] as u8,
            ios_rating: avatar_flags[4] as u8,
            content_warnings: avatar_flags[5] as u8,
            style_filter: avatar_flags[6] as u16,
            marketplace: avatar_flags[7] != 0,
        });
        index.insert(avatar_id, entries.len() - 1);
    }

    Ok(ProcessedData {
        entries,
        index,
        last_update,
        avatar_count,
        author_count,
    })
}

pub fn parse_aux_database(bytes: Vec<u8>) -> Result<Vec<String>> {
    let mut reader = Reader::new(bytes);
    if reader.remaining() == 0 {
        return Err(anyhow!("Data has length zero"));
    }

    if reader.read_bytes(3)? != b"PAS" {
        return Err(anyhow!("PAS Header not found"));
    }

    let meta_bytes = reader.read_bytes(2)?;
    let version = (meta_bytes[0] >> 5) & 31;
    let _platform = meta_bytes[0] & 7;

    reader.read_bytes(3 + 3 + 2)?; // avatars + authors + date
    let file_avatars = reader.read_int24()? as usize;
    reader.read_bytes(3)?; // file_authors
    let flag_size = reader.read_byte()? as usize;

    let random_bytes = reader.read_bytes(16)?;
    let dynamic_bytes: Vec<u8> = if version == 0 {
        random_bytes
            .iter()
            .zip(STATIC_BYTES.iter())
            .map(|(a, b)| a ^ b)
            .collect()
    } else {
        random_bytes
            .iter()
            .enumerate()
            .map(|(i, a)| a ^ (STATIC_BYTES[(i + version as usize) % 16].wrapping_add((flag_size % 256) as u8)))
            .collect()
    };

    let avatar_ids = reader.read_bytes(file_avatars * 16)?;

    let mut ids = Vec::with_capacity(file_avatars);
    for i in 0..file_avatars {
        let block = avatar_ids[i * 16..(i + 1) * 16].to_vec();
        ids.push(decode_avatar_id(block, &dynamic_bytes)?);
    }

    Ok(ids)
}

async fn download_with_progress<F>(
    client: &Client,
    url: &str,
    step_label: &str,
    step: usize,
    total_steps: usize,
    mut on_progress: F,
) -> Result<Vec<u8>>
where
    F: FnMut(SyncProgress),
{
    on_progress(SyncProgress {
        stage: step_label.to_string(),
        step,
        total_steps,
        percent: 0.0,
        message: format!("Connecting to download {}...", step_label),
        complete: false,
        error: None,
    });

    let res = client.get(url).send().await?;
    if !res.status().is_success() {
        return Err(anyhow!("Failed to download {}: HTTP {}", url, res.status()));
    }

    let total_size = res.content_length().unwrap_or(0);
    let mut stream = res.bytes_stream();
    let mut bytes = Vec::with_capacity(if total_size > 0 { total_size as usize } else { 1024 * 1024 });
    let mut downloaded = 0u64;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        downloaded += chunk.len() as u64;
        bytes.extend_from_slice(&chunk);

        let percent = if total_size > 0 {
            (downloaded as f32 / total_size as f32) * 100.0
        } else {
            50.0
        };

        on_progress(SyncProgress {
            stage: step_label.to_string(),
            step,
            total_steps,
            percent,
            message: format!("{}: {:.1} MB / {:.1} MB", step_label, downloaded as f64 / 1_048_576.0, total_size as f64 / 1_048_576.0),
            complete: false,
            error: None,
        });
    }

    Ok(bytes)
}

pub async fn run_sync_pipeline<F>(
    db_path: &Path,
    mut on_progress: F,
) -> Result<()>
where
    F: FnMut(SyncProgress) + Send + 'static,
{
    let client = Client::builder()
        .user_agent("VRChatPrismicAvatarSearch/2.0")
        .build()?;

    // Step 1: Download Main DB (with fallback to backup URL)
    let main_bytes = match download_with_progress(&client, URL_MAIN, "Main Database", 1, 6, &mut on_progress).await {
        Ok(bytes) => bytes,
        Err(_) => download_with_progress(&client, BACKUP_URL_MAIN, "Main Database (Fallback)", 1, 6, &mut on_progress).await?,
    };

    // Step 2: Download Quest DB (with fallback to backup URL)
    let quest_bytes = match download_with_progress(&client, URL_QUEST, "Quest Database", 2, 6, &mut on_progress).await {
        Ok(bytes) => bytes,
        Err(_) => download_with_progress(&client, BACKUP_URL_QUEST, "Quest Database (Fallback)", 2, 6, &mut on_progress).await?,
    };

    // Step 3: Download iOS DB (with fallback to backup URL)
    let ios_bytes = match download_with_progress(&client, URL_IOS, "iOS Database", 3, 6, &mut on_progress).await {
        Ok(bytes) => bytes,
        Err(_) => download_with_progress(&client, BACKUP_URL_IOS, "iOS Database (Fallback)", 3, 6, &mut on_progress).await?,
    };

    // Step 4: Parse Databases
    on_progress(SyncProgress {
        stage: "Parsing Databases".to_string(),
        step: 4,
        total_steps: 6,
        percent: 0.0,
        message: "Decrypting and parsing avatar records...".to_string(),
        complete: false,
        error: None,
    });

    let mut processed = parse_main_database(main_bytes)?;
    let quest_ids = parse_aux_database(quest_bytes)?;
    let ios_ids = parse_aux_database(ios_bytes)?;

    // Mark quest platforms
    for id in &quest_ids {
        if let Some(&idx) = processed.index.get(id) {
            if let Some(entry) = processed.entries.get_mut(idx) {
                entry.quest = true;
                entry.platform |= 2;
            }
        }
    }

    // Mark ios platforms
    for id in &ios_ids {
        if let Some(&idx) = processed.index.get(id) {
            if let Some(entry) = processed.entries.get_mut(idx) {
                entry.ios = true;
                entry.platform |= 4;
            }
        }
    }

    // Step 5: Bulk Insert into SQLite
    on_progress(SyncProgress {
        stage: "Bulk Insert".to_string(),
        step: 5,
        total_steps: 6,
        percent: 0.0,
        message: format!("Inserting {} avatars into SQLite database...", processed.entries.len()),
        complete: false,
        error: None,
    });

    if let Some(parent) = db_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mut conn = Connection::open(db_path)?;
    conn.execute_batch("
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = OFF;
        PRAGMA cache_size = -128000;
        PRAGMA temp_store = MEMORY;
    ")?;

    let total_entries = processed.entries.len();
    {
        // Rebuild inside one transaction so readers keep seeing the old table until commit.
        let tx = conn.transaction()?;
        tx.execute("DROP TABLE IF EXISTS avatars", [])?;
        tx.execute(
            "CREATE TABLE avatars (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                author TEXT NOT NULL,
                description TEXT,
                avatar_id TEXT NOT NULL,
                tags TEXT,
                quest INTEGER DEFAULT 0,
                ios INTEGER DEFAULT 0,
                platform INTEGER DEFAULT 0,
                impostor_platform INTEGER DEFAULT 0,
                pc_rating INTEGER DEFAULT 0,
                quest_rating INTEGER DEFAULT 0,
                ios_rating INTEGER DEFAULT 0,
                content_warnings INTEGER DEFAULT 0,
                style_filter INTEGER DEFAULT 0,
                marketplace INTEGER DEFAULT 0
            )",
            [],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO avatars (
                    id, name, author, description, avatar_id, tags,
                    quest, ios, platform, impostor_platform,
                    pc_rating, quest_rating, ios_rating,
                    content_warnings, style_filter, marketplace
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            )?;

            for (idx, avatar) in processed.entries.iter().enumerate() {
                let tags_json = serde_json::to_string(&avatar.tags).unwrap_or_else(|_| "[]".to_string());
                stmt.execute(params![
                    avatar.id,
                    avatar.name,
                    avatar.author,
                    avatar.description,
                    avatar.avatar_id,
                    tags_json,
                    if avatar.quest { 1 } else { 0 },
                    if avatar.ios { 1 } else { 0 },
                    avatar.platform as i64,
                    avatar.impostor_platform as i64,
                    avatar.pc_rating as i64,
                    avatar.quest_rating as i64,
                    avatar.ios_rating as i64,
                    avatar.content_warnings as i64,
                    avatar.style_filter as i64,
                    if avatar.marketplace { 1 } else { 0 }
                ])?;

                if idx % 10000 == 0 || idx == total_entries - 1 {
                    let percent = ((idx + 1) as f32 / total_entries as f32) * 100.0;
                    on_progress(SyncProgress {
                        stage: "Bulk Insert".to_string(),
                        step: 5,
                        total_steps: 6,
                        percent,
                        message: format!("Inserted {} of {} records ({:.0}%)", idx + 1, total_entries, percent),
                        complete: false,
                        error: None,
                    });
                }
            }
        }
        tx.commit()?;
    }

    // Step 6: Create Indexes
    on_progress(SyncProgress {
        stage: "Building Indexes".to_string(),
        step: 6,
        total_steps: 6,
        percent: 0.0,
        message: "Creating SQLite indexes for instant search...".to_string(),
        complete: false,
        error: None,
    });

    conn.execute("CREATE INDEX IF NOT EXISTS idx_avatars_name ON avatars(name)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_avatars_author ON avatars(author)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_avatars_avatar_id ON avatars(avatar_id)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_avatars_platform ON avatars(platform)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_avatars_marketplace ON avatars(marketplace)", [])?;

    // Record metadata table
    conn.execute("CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT)", [])?;
    conn.execute("INSERT OR REPLACE INTO meta (key, value) VALUES ('last_updated', ?1)", params![processed.last_update])?;
    conn.execute("INSERT OR REPLACE INTO meta (key, value) VALUES ('synced_at', ?1)", params![chrono::Utc::now().to_rfc3339()])?;
    conn.execute("INSERT OR REPLACE INTO meta (key, value) VALUES ('total_avatars', ?1)", params![total_entries.to_string()])?;

    on_progress(SyncProgress {
        stage: "Complete".to_string(),
        step: 6,
        total_steps: 6,
        percent: 100.0,
        message: format!("Successfully synced {} avatars!", total_entries),
        complete: true,
        error: None,
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_avatar_id_format() {
        let block = vec![1u8; 16];
        let iv = vec![2u8; 16];
        let id = decode_avatar_id(block, &iv).unwrap();
        assert!(id.starts_with("avtr_"));
        assert_eq!(id.len(), 5 + 32 + 4); // "avtr_" + 32 hex chars + 4 dashes = 41 chars
    }

    #[test]
    fn test_decode_style_filter() {
        let tags = decode_style_filter(1 | 4 | 8);
        assert_eq!(tags, vec!["Pop Culture", "Sci-Fi", "Anime"]);
    }
}
