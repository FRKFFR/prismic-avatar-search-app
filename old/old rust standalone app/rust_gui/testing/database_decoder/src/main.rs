use std::{
    fs,
    path::Path,
};
use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

const URLS: &[&str] = &[
    "https://gist.githubusercontent.com/Mwr247/a80c1f9060fc4fd46a8f00d589c47c5a/raw/pasavtrdb.txt",
    "https://gist.githubusercontent.com/Mwr247/a80c1f9060fc4fd46a8f00d589c47c5a/raw/pasavtrdb_qst.txt", 
    "https://gist.githubusercontent.com/Mwr247/a80c1f9060fc4fd46a8f00d589c47c5a/raw/pasavtrdb_ios.txt"
];

const STATIC_BYTES: &[u8] = &[208, 29, 107, 36, 251, 69, 122, 14, 67, 204, 171, 246, 106, 38, 183, 224];

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

fn default_avatar_id() -> String {
    "unknown".to_string()
}

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

    fn read_bytes(&mut self, count: usize) -> Result<&[u8]> {
        if self.position + count > self.data.len() {
            return Err(anyhow!("Not enough bytes: need {}, have {}", count, self.remaining()));
        }
        let bytes = &self.data[self.position..self.position + count];
        self.position += count;
        Ok(bytes)
    }

    fn read_u32(&mut self) -> Result<u32> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_i32(&mut self) -> Result<i32> {
        let bytes = self.read_bytes(4)?;
        Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_int24(&mut self) -> Result<u32> {
        if self.position + 3 > self.data.len() {
            return Err(anyhow!("Attempted to read beyond end of data"));
        }
        
        let bytes = self.read_bytes(3)?;
        Ok((bytes[0] as u32) << 16 | (bytes[1] as u32) << 8 | bytes[2] as u32)
    }
}

fn decode_avatar_id(crypt: &[u8], iv: &[u8]) -> String {
    let mut crypt = crypt.to_vec();
    
    // XOR decode process (reverse of the encoding)
    for i in (0..crypt.len()).rev() {
        let k = crypt[i] ^ crypt[(i + crypt.len() - 1) % crypt.len()] ^ iv[i];
        crypt[i] = k;
    }
    
    // Convert to hex string
    crypt.iter().map(|b| format!("{:02x}", b)).collect()
}

fn parse_main_database(bytes: Vec<u8>) -> Result<Vec<u8>> {
    let mut reader = Reader::new(bytes);
    
    // Read header
    let _version = reader.read_u32()?;
    let _timestamp = reader.read_u32()?;
    let avatar_count = reader.read_u32()?;
    let author_count = reader.read_u32()?;
    let _platform_count = reader.read_u32()?;
    
    // Read avatar IDs (16 bytes each) - use conservative approach
    let mut avatar_ids = Vec::new();
    for _ in 0..avatar_count {
        if reader.remaining() < 32 { // 16 bytes encrypted + 16 bytes IV
            break;
        }
        let encrypted_id = reader.read_bytes(16)?.to_vec();
        let iv = reader.read_bytes(16)?.to_vec();
        let decoded_id = decode_avatar_id(&encrypted_id, &iv);
        avatar_ids.push(decoded_id);
    }
    
    // Read author IDs (16 bytes each) - use conservative approach
    let mut author_ids = Vec::new();
    for _ in 0..author_count {
        if reader.remaining() < 32 { // 16 bytes encrypted + 16 bytes IV
            break;
        }
        let encrypted_id = reader.read_bytes(16)?.to_vec();
        let iv = reader.read_bytes(16)?.to_vec();
        let decoded_id = decode_avatar_id(&encrypted_id, &iv);
        author_ids.push(decoded_id);
    }
    
    // Read string block - use conservative approach
    let string_block_size = reader.read_u32()? as usize;
    let raw_string_bytes = if string_block_size > reader.remaining() {
        println!("Warning: String block size {} exceeds remaining bytes {}, using available data", 
                 string_block_size, reader.remaining());
        // Use whatever bytes are available
        reader.read_bytes(reader.remaining())?
    } else {
        reader.read_bytes(string_block_size)?
    };
    
    // Parse strings using UTF-8 decoding and reversal (matching the working Python script)
    let strings = String::from_utf8(raw_string_bytes.to_vec())?;
    let mut parts = strings.split('\n');
    
    let authors_block = parts.next().unwrap_or("");
    let avatars_block = parts.next().unwrap_or("");
    
    // If we don't have proper string blocks, create a basic output
    if authors_block.is_empty() || avatars_block.is_empty() {
        println!("Warning: Incomplete string block data, creating basic output");
        
        // Create basic output with available avatar IDs
        let mut output = Vec::new();
        output.push(format!("Main Database (Incomplete Data)"));
        output.push(format!("Avatar Count: {}", avatar_ids.len()));
        output.push(format!("Author Count: {}", author_ids.len()));
        output.push(format!(""));
        
        for (i, avatar_id) in avatar_ids.iter().enumerate() {
            output.push(format!("Avatar {}: {}", i + 1, avatar_id));
            output.push(format!("  Author: Unknown"));
            output.push(format!("  Description: Not available"));
            output.push(format!(""));
        }
        
        return Ok(output.join("\n").into_bytes());
    }
    
    let author_names: Vec<String> = authors_block
        .split('\r')
        .map(|s: &str| s.chars().rev().collect::<String>())
        .collect();
    
    let avatar_lines: Vec<&str> = avatars_block.split('\r').collect();
    
    // Create decoded output
    let mut output = Vec::new();
    
    // Add header information
    output.push(format!("Main Database"));
    output.push(format!("Avatar Count: {}", avatar_count));
    output.push(format!("Author Count: {}", author_count));
    output.push(format!(""));
    
    // Add avatar information
    for (i, avatar_line) in avatar_lines.iter().enumerate() {
        if i >= avatar_ids.len() {
            break;
        }
        
        let parts: Vec<&str> = avatar_line.split('|').collect();
        let name = if parts.len() > 0 { parts[0] } else { "Unknown" };
        let description = if parts.len() > 1 { parts[1] } else { "" };
        let author_index = if parts.len() > 2 { parts[2].parse::<usize>().unwrap_or(0) } else { 0 };
        
        let author_name = if author_index < author_names.len() {
            &author_names[author_index]
        } else {
            "Unknown"
        };
        
        output.push(format!("Avatar: {}", name));
        output.push(format!("  ID: {}", avatar_ids[i]));
        output.push(format!("  Author: {}", author_name));
        output.push(format!("  Description: {}", description));
        output.push(format!(""));
    }
    
    Ok(output.join("\n").into_bytes())
}

fn parse_aux_database(bytes: Vec<u8>, platform: &str) -> Result<Vec<u8>> {
    let mut reader = Reader::new(bytes);
    
    // Read header
    let _version = reader.read_u32()?;
    let _timestamp = reader.read_u32()?;
    let avatar_count = reader.read_u32()?;
    
    // Read avatar IDs (16 bytes each) - use conservative approach
    let mut avatar_ids = Vec::new();
    for _ in 0..avatar_count {
        if reader.remaining() < 32 { // 16 bytes encrypted + 16 bytes IV
            break;
        }
        let encrypted_id = reader.read_bytes(16)?.to_vec();
        let iv = reader.read_bytes(16)?.to_vec();
        let decoded_id = decode_avatar_id(&encrypted_id, &iv);
        avatar_ids.push(decoded_id);
    }
    
    // Read string block - use conservative approach
    let string_block_size = reader.read_u32()? as usize;
    let raw_string_bytes = if string_block_size > reader.remaining() {
        println!("Warning: Aux database string block size {} exceeds remaining bytes {}, using available data", 
                 string_block_size, reader.remaining());
        // Use whatever bytes are available
        reader.read_bytes(reader.remaining())?
    } else {
        reader.read_bytes(string_block_size)?
    };
    
    // Parse strings using UTF-8 decoding and reversal
    let strings = String::from_utf8(raw_string_bytes.to_vec())?;
    let avatar_lines: Vec<&str> = strings.split('\r').collect();
    
    // If we don't have proper avatar data, create a basic output
    if avatar_lines.is_empty() || avatar_lines[0].is_empty() {
        println!("Warning: Incomplete aux database string data, creating basic output");
        
        // Create basic output with available avatar IDs
        let mut output = Vec::new();
        output.push(format!("Aux Database - {} (Incomplete Data)", platform));
        output.push(format!("Avatar Count: {}", avatar_ids.len()));
        output.push(format!(""));
        
        for (i, avatar_id) in avatar_ids.iter().enumerate() {
            output.push(format!("Avatar {}: {}", i + 1, avatar_id));
            output.push(format!("  Description: Not available"));
            output.push(format!(""));
        }
        
        return Ok(output.join("\n").into_bytes());
    }
    
    // Create decoded output
    let mut output = Vec::new();
    
    // Add header information
    output.push(format!("Aux Database - {}", platform));
    output.push(format!("Avatar Count: {}", avatar_count));
    output.push(format!(""));
    
    // Add avatar information
    for (i, avatar_line) in avatar_lines.iter().enumerate() {
        if i >= avatar_ids.len() {
            break;
        }
        
        let parts: Vec<&str> = avatar_line.split('|').collect();
        let name = if parts.len() > 0 { parts[0] } else { "Unknown" };
        let description = if parts.len() > 1 { parts[1] } else { "" };
        
        output.push(format!("Avatar: {}", name));
        output.push(format!("  ID: {}", avatar_ids[i]));
        output.push(format!("  Description: {}", description));
        output.push(format!(""));
    }
    
    Ok(output.join("\n").into_bytes())
}

fn download_bytes(client: &Client, url: &str) -> Result<Vec<u8>> {
    let response = client
        .get(url)
        .send()
        .with_context(|| format!("GET {}", url))?;
    
    if !response.status().is_success() {
        return Err(anyhow!("HTTP {}: {}", response.status(), response.text().unwrap_or_default()));
    }
    
    response
        .bytes()
        .with_context(|| format!("Failed to read response body from {}", url))
        .map(|b| b.to_vec())
}

fn main() -> Result<()> {
    println!("Avatar Database Decoder");
    println!("========================");
    
    // Create cache directory
    let cache_dir = Path::new("cache");
    fs::create_dir_all(cache_dir)?;
    
    // Create HTTP client
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("AvatarDatabaseDecoder/1.0")
        .build()?;
    
    // Download and decode databases
    for (index, &url) in URLS.iter().enumerate() {
        println!("Downloading database {}...", index + 1);
        
        match download_bytes(&client, url) {
            Ok(bytes) => {
                println!("Downloaded {} bytes", bytes.len());
                
                // Parse and decode
                let decoded = if index == 0 {
                    parse_main_database(bytes)?
                } else {
                    let platform = if index == 1 { "Quest" } else { "iOS" };
                    parse_aux_database(bytes, platform)?
                };
                
                // Save decoded file
                let filename = if index == 0 {
                    "main_database_decoded.txt"
                } else if index == 1 {
                    "quest_database_decoded.txt"
                } else {
                    "ios_database_decoded.txt"
                };
                
                let output_path = cache_dir.join(filename);
                fs::write(&output_path, decoded)?;
                
                println!("Decoded and saved to: {}", output_path.display());
            }
            Err(err) => {
                eprintln!("Failed to download database {}: {}", index + 1, err);
            }
        }
        
        println!();
    }
    
    println!("Decoding complete!");
    println!("Check the 'cache' directory for decoded files:");
    println!("- main_database_decoded.txt");
    println!("- quest_database_decoded.txt"); 
    println!("- ios_database_decoded.txt");
    
    Ok(())
}
