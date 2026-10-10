use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    thread,
    time::Instant,
};

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Duration, Utc};
use eframe::egui::{self, ProgressBar};
use egui_extras::{Column, TableBuilder};
use flume::{Receiver, Sender};
use log::warn;
use regex::Regex;
use reqwest::Client;  // Async client by default
use rusqlite::{Connection, params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::io::Read;
use zstd::{encode_all, decode_all};

const URLS: [&str; 3] = [
    "https://gist.githubusercontent.com/Prismic247/930d08f34c61e4282992cdb3afbafca0/raw/pasavtrdb.txt",
    "https://gist.githubusercontent.com/Prismic247/930d08f34c61e4282992cdb3afbafca0/raw/pasavtrdb_qst.txt",
    "https://gist.githubusercontent.com/Prismic247/930d08f34c61e4282992cdb3afbafca0/raw/pasavtrdb_ios.txt",
];

// Database is updated only once per day at 07:35:09 UTC+1
// This means we can safely cache for 24 hours and only check for updates once daily

// Backup URLs temporarily disabled
const BACKUP_URLS: [&str; 3] = [
    "", // "https://prismic.net/vrc/pasavtrdb.txt",
    "", // "https://prismic.net/vrc/pasavtrdb_qst.txt",
    "", // "https://prismic.net/vrc/pasavtrdb.txt",
];

const STATIC_BYTES: [u8; 16] = [208, 29, 107, 36, 251, 69, 122, 14, 67, 204, 171, 246, 106, 38, 183, 224];

const STEP_LABELS: [&str; 6] = [
    "Downloading Main Database",
    "Downloading Quest Database",
    "Downloading iOS Database",
    "Parsing Data",
    "Bulk Insert",
    "Database Integrity Check",
];

// Configuration structures for future-proof features
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    pub max_age_hours: u32,        // Default: 24h
    pub auto_cleanup: bool,         // Auto-delete old cache
    pub refresh_on_startup: bool,   // Check for updates automatically
    pub backup_count: u32,          // Keep last N backups
    pub compression_enabled: bool,  // Compress backups
    pub compression_level: u32,     // 0-9 compression level (0=fastest, 9=best)
    pub compression_threads: u32,   // Number of threads for compression (0=auto)
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_age_hours: 24,
            auto_cleanup: true,
            refresh_on_startup: true,
            backup_count: 5,
            compression_enabled: true,
            compression_level: 3, // Fast compression by default
            compression_threads: 0, // Auto-detect number of threads
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub batch_size: usize,
    pub enable_indexes: bool,
    pub mmap_size: u64,            // Memory mapping size in bytes
    pub cache_size_mb: i32,       // SQLite cache size in MB
    pub synchronous_mode: String, // OFF, NORMAL, FULL
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            batch_size: 10000,
            enable_indexes: true,
            mmap_size: 268435456, // 256MB
            cache_size_mb: 2000,
            synchronous_mode: "OFF".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    pub check_integrity: bool,      // Verify SQLite consistency
    pub check_duplicates: bool,     // Find duplicate entries
    pub check_orphans: bool,        // Find broken references
    pub auto_repair: bool,          // Fix issues automatically
    pub schedule_hours: u32,        // Run health check every N hours
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            check_integrity: true,
            check_duplicates: false,
            check_orphans: false,
            auto_repair: true,
            schedule_hours: 24,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub cache: CacheConfig,
    pub database: DatabaseConfig,
    pub health_check: HealthCheckConfig,
    pub ui_theme: String,
    pub auto_refresh: bool,
    pub log_level: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            cache: CacheConfig::default(),
            database: DatabaseConfig::default(),
            health_check: HealthCheckConfig::default(),
            ui_theme: "dark".to_string(),
            auto_refresh: true,
            log_level: "info".to_string(),
        }
    }
}

// Database versioning and migration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseVersion {
    pub version: u32,
    pub migration_date: DateTime<Utc>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationScript {
    pub from_version: u32,
    pub to_version: u32,
    pub sql: String,
    pub description: String,
}

// Backup and restore
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupMetadata {
    pub backup_date: DateTime<Utc>,
    pub database_version: u32,
    pub avatar_count: u64,
    pub author_count: u64,
    pub compressed: bool,
    pub file_size_bytes: u64,
}

// Health check results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    pub check_date: DateTime<Utc>,
    pub integrity_ok: bool,
    pub duplicate_count: u64,
    pub orphan_count: u64,
    pub total_entries: u64,
    pub errors: Vec<String>,
    pub repairs_made: Vec<String>,
}

// Data source configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSource {
    pub name: String,
    pub url: String,
    pub priority: u8,
    pub enabled: bool,
    pub is_fallback: bool,
}

// Cache types for different strategies
#[derive(Debug, Clone, PartialEq)]
pub enum CacheType {
    AvatarData,     // Long-term cache
    Metadata,       // Medium-term cache  
    UserPrefs,      // Short-term cache
    TempData,       // No cache
}

impl CacheType {
    pub fn default_ttl(&self) -> Duration {
        match self {
            CacheType::AvatarData => Duration::hours(24),
            CacheType::Metadata => Duration::hours(12),
            CacheType::UserPrefs => Duration::hours(1),
            CacheType::TempData => Duration::minutes(30),
        }
    }
}

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
    database_version: u32,
    config: AppConfig,
}

// Configuration management functions
fn load_config() -> AppConfig {
    let config_path = PathBuf::from("config.toml");
    
    if config_path.exists() {
        match fs::read_to_string(&config_path) {
            Ok(content) => match toml::from_str::<AppConfig>(&content) {
                Ok(config) => {
                    println!("Configuration loaded from {}", config_path.display());
                    return config;
                }
                Err(e) => {
                    println!("Failed to parse config file: {}", e);
                }
            }
            Err(e) => {
                println!("Failed to read config file: {}", e);
            }
        }
    }
    
    // Return default config if file doesn't exist or is invalid
    let default_config = AppConfig::default();
    save_config(&default_config);
    default_config
}

fn save_config(config: &AppConfig) {
    let config_path = PathBuf::from("config.toml");
    
    // Create a comprehensive configuration file with comments
    let content = format!(
        "# ========================================\n\
         # VRChat Avatar Database Configuration\n\
         # ========================================\n\
         # This file controls all application settings.\n\
         # The application will automatically create this file with defaults if it doesn't exist.\n\
         # Changes take effect on application restart.\n\
         \n\
         # ==================== GLOBAL SETTINGS ====================\n\
         # These settings control the overall application behavior\n\
         \n\
         ui_theme = \"{}\"        # UI appearance: \"dark\", \"light\", or \"auto\"\n\
         auto_refresh = {}      # Automatically refresh data when updates are available\n\
         log_level = \"{}\"       # Logging verbosity: \"error\", \"warn\", \"info\", \"debug\", \"trace\"\n\
         \n\
         # ==================== CACHE SETTINGS ====================\n\
         # These settings control how the application manages cached data\n\
         \n\
         [cache]\n\
         max_age_hours = {}        # Cache expires after this many hours (24 = 1 day)\n\
         auto_cleanup = {}       # Automatically delete old cache files on startup\n\
         refresh_on_startup = {} # Check for database updates when application starts\n\
         backup_count = {}          # Keep the last N backup files (older ones are deleted)\n\
         compression_enabled = {} # Compress backup files to save disk space\n\
         compression_level = {}     # Zstandard compression level (0=fastest, 9=best compression)\n\
         compression_threads = {}   # Number of threads for compression (0=auto-detect CPU cores)\n\
         \n\
         # ==================== DATABASE SETTINGS ====================\n\
         # These settings control SQLite database performance and behavior\n\
         \n\
         [database]\n\
         batch_size = {}        # Number of records to process in each batch (affects memory usage)\n\
         enable_indexes = {}     # Create database indexes for faster queries (slower writes)\n\
         mmap_size = {}     # Memory-mapped file size in bytes (256MB = 268,435,456)\n\
         cache_size_mb = {}      # SQLite cache size in megabytes (affects query performance)\n\
         synchronous_mode = \"{}\"  # SQLite write safety: \"OFF\" (fastest), \"NORMAL\", \"FULL\" (safest)\n\
         \n\
         # ==================== HEALTH CHECK SETTINGS ====================\n\
         # These settings control automatic database maintenance and integrity checks\n\
         \n\
         [health_check]\n\
         check_integrity = {}    # Verify SQLite database consistency on startup\n\
         check_duplicates = {}  # Scan for duplicate avatar entries (slow operation)\n\
         check_orphans = {}     # Find broken references between tables (slow operation)\n\
         auto_repair = {}        # Automatically fix issues found during health checks\n\
         schedule_hours = {}       # Run health check automatically every N hours (24 = daily)\n\
         \n\
         # ==================== HOW IT WORKS ====================\n\
         # 1. Application starts and loads this configuration file\n\
         # 2. If file doesn't exist or is invalid, defaults are used and saved\n\
         # 3. Settings are applied throughout the application\n\
         # 4. Changes to this file require application restart to take effect\n\
         #\n\
         # Performance Impact:\n\
         # - Higher compression_level = slower compression but smaller files\n\
         # - More backup_count = more disk space usage\n\
         # - Larger cache_size_mb = more RAM usage but faster queries\n\
         # - synchronous_mode \"OFF\" = fastest writes but less crash safety\n",
        config.ui_theme,
        config.auto_refresh,
        config.log_level,
        config.cache.max_age_hours,
        config.cache.auto_cleanup,
        config.cache.refresh_on_startup,
        config.cache.backup_count,
        config.cache.compression_enabled,
        config.cache.compression_level,
        config.cache.compression_threads,
        config.database.batch_size,
        config.database.enable_indexes,
        config.database.mmap_size,
        config.database.cache_size_mb,
        config.database.synchronous_mode,
        config.health_check.check_integrity,
        config.health_check.check_duplicates,
        config.health_check.check_orphans,
        config.health_check.auto_repair,
        config.health_check.schedule_hours
    );
    
    if let Err(e) = fs::write(&config_path, content) {
        println!("Failed to save config file: {}", e);
    } else {
        println!("Configuration saved to {}", config_path.display());
    }
}

// Cache expiration functions
fn is_cache_expired(meta: &CacheMeta, config: &CacheConfig) -> bool {
    if let Ok(saved_at) = DateTime::parse_from_rfc3339(&meta.saved_at) {
        let cache_age = Utc::now() - saved_at.with_timezone(&Utc);
        cache_age > Duration::hours(config.max_age_hours as i64)
    } else {
        true // If we can't parse the date, assume it's expired
    }
}

fn cleanup_old_cache(config: &CacheConfig) -> Result<()> {
    if !config.auto_cleanup {
        return Ok(());
    }
    
    let cache_dir = PathBuf::from("cache");
    if !cache_dir.exists() {
        return Ok(());
    }
    
    println!("Cleaning up old cache files...");
    
    // Clean up old backups
    let backup_dir = cache_dir.join("backups");
    if backup_dir.exists() {
        let mut backups: Vec<_> = fs::read_dir(&backup_dir)?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("db") {
                    Some((path, entry.metadata().ok()?.modified().ok()?))
                } else {
                    None
                }
            })
            .collect();
        
        // Sort by modification time (oldest first)
        backups.sort_by_key(|(_, time)| *time);
        
        // Remove excess backups
        if backups.len() > config.backup_count as usize {
            for (path, _) in backups.iter().take(backups.len() - config.backup_count as usize) {
                if let Err(e) = fs::remove_file(path) {
                    println!("Failed to remove old backup {}: {}", path.display(), e);
                }
            }
        }
    }
    
    Ok(())
}

// Database versioning and migration functions
fn get_current_database_version(conn: &Connection) -> Result<u32> {
    let mut stmt = conn.prepare("SELECT version FROM database_version ORDER BY version DESC LIMIT 1")?;
    let version: Option<u32> = stmt.query_row([], |row| row.get(0)).optional()?;
    Ok(version.unwrap_or(1))
}

fn create_database_version_table(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS database_version (
            version INTEGER PRIMARY KEY,
            migration_date TEXT NOT NULL,
            description TEXT NOT NULL
        )",
        [],
    )?;
    Ok(())
}

fn migrate_database(conn: &mut Connection, from_version: u32, to_version: u32) -> Result<()> {
    let migrations = get_migration_scripts();
    
    for migration in migrations {
        if migration.from_version == from_version && migration.to_version == to_version {
            println!("Applying migration: {}", migration.description);
            
            let tx = conn.transaction()?;
            tx.execute(&migration.sql, [])?;
            
            // Record the migration
            tx.execute(
                "INSERT INTO database_version (version, migration_date, description) VALUES (?, ?, ?)",
                params![to_version, Utc::now().to_rfc3339(), migration.description],
            )?;
            
            tx.commit()?;
            println!("Migration completed successfully");
            return Ok(());
        }
    }
    
    Err(anyhow!("No migration found from version {} to {}", from_version, to_version))
}

fn get_migration_scripts() -> Vec<MigrationScript> {
    vec![
        MigrationScript {
            from_version: 1,
            to_version: 2,
            sql: "ALTER TABLE avatars ADD COLUMN version INTEGER DEFAULT 1".to_string(),
            description: "Add version column to avatars table".to_string(),
        },
        MigrationScript {
            from_version: 2,
            to_version: 3,
            sql: "CREATE INDEX IF NOT EXISTS idx_version ON avatars(version)".to_string(),
            description: "Add index for version column".to_string(),
        },
    ]
}

// Backup and restore functions
fn create_backup(conn: &Connection, config: &CacheConfig) -> Result<PathBuf> {
    let backup_dir = PathBuf::from("cache/backups");
    fs::create_dir_all(&backup_dir)?;
    
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
    let backup_path = backup_dir.join(format!("backup_{}.db", timestamp));
    
    println!("Creating backup: {}", backup_path.display());
    
    // Close the connection and copy the file directly (safer than VACUUM INTO)
    let db_path = "cache/avatars.db";
    fs::copy(db_path, &backup_path)?;
    
    // Get database statistics for metadata
    let (avatar_count, author_count) = get_database_stats(conn)?;
    let current_version = get_current_database_version(conn)?;
    
    let original_size = fs::metadata(&backup_path)?.len();
    
    // Create metadata immediately (before compression)
    let metadata = BackupMetadata {
        backup_date: Utc::now(),
        database_version: current_version,
        avatar_count: avatar_count.into(),
        author_count: author_count.into(),
        compressed: config.compression_enabled,
        file_size_bytes: original_size, // Will be updated after compression
    };
    
    // Save backup metadata immediately
    let metadata_path = backup_dir.join(format!("backup_{}.json", timestamp));
    fs::write(metadata_path, serde_json::to_string_pretty(&metadata)?)?;
    
    if config.compression_enabled {
        // Start background compression
        let backup_path_clone = backup_path.clone();
        let timestamp_str = timestamp.to_string();
        let compression_level = config.compression_level.min(22).max(1) as i32;
        
        thread::spawn(move || {
            if let Err(e) = compress_backup_in_background(&backup_path_clone, &timestamp_str, compression_level) {
                eprintln!("Background compression failed: {}", e);
            }
        });
        
        println!("Backup created successfully, compressing in background...");
        Ok(backup_path) // Return uncompressed path for immediate use
    } else {
        println!("Backup created successfully");
        Ok(backup_path)
    }
}

fn compress_backup_in_background(backup_path: &Path, timestamp: &str, compression_level: i32) -> Result<()> {
    println!("Starting background compression...");
    
    // Read the database file
    let mut db_file = fs::File::open(backup_path)?;
    let mut db_data = Vec::new();
    db_file.read_to_end(&mut db_data)?;
    
    let original_size = db_data.len();
    
    // Create compressed file with .zst extension
    let backup_dir = backup_path.parent().unwrap_or_else(|| Path::new("."));
    let compressed_path = backup_dir.join(format!("backup_{}.zst", timestamp));
    
    // Use Zstandard compression
    let compressed_data = encode_all(&db_data[..], compression_level)?;
    let compressed_size = compressed_data.len();
    
    // Write compressed data
    fs::write(&compressed_path, compressed_data)?;
    let compression_ratio = (1.0 - (compressed_size as f64 / original_size as f64)) * 100.0;
    println!("Background compression completed: {:.1}% space saved ({} MB -> {} MB)", 
        compression_ratio,
        original_size / (1024 * 1024),
        compressed_size / (1024 * 1024)
    );
    
    // Remove the uncompressed backup after successful compression
    fs::remove_file(backup_path)?;
    
    // Update metadata with compressed file size
    let metadata_path = backup_dir.join(format!("backup_{}.json", timestamp));
    if let Ok(metadata_content) = fs::read_to_string(&metadata_path) {
        if let Ok(mut metadata) = serde_json::from_str::<BackupMetadata>(&metadata_content) {
            metadata.file_size_bytes = compressed_size as u64;
            if let Ok(updated_metadata) = serde_json::to_string_pretty(&metadata) {
                let _ = fs::write(&metadata_path, updated_metadata);
            }
        }
    }
    
    println!("Background compression finished successfully");
    Ok(())
}

fn restore_backup(backup_path: &Path, target_path: &Path) -> Result<()> {
    println!("Restoring backup from {} to {}", backup_path.display(), target_path.display());
    
    // Check if backup is compressed (has .zst extension)
    if backup_path.extension().and_then(|s| s.to_str()) == Some("zst") {
        println!("Decompressing Zstandard backup...");
        
        // Read compressed backup
        let compressed_data = fs::read(backup_path)?;
        
        // Decompress with Zstandard
        let decompressed_data = decode_all(&compressed_data[..])?;
        
        // Write decompressed database
        fs::write(target_path, decompressed_data)?;
        
        println!("Backup decompressed and restored successfully");
    } else {
        // Copy uncompressed backup file to target location
        fs::copy(backup_path, target_path)?;
        println!("Backup restored successfully");
    }
    
    Ok(())
}

// Health check functions
fn run_background_health_check(db_path: &Path, config: &HealthCheckConfig, tx: &Sender<UiMessage>) -> Result<()> {
    let _ = tx.send(UiMessage::Status("Background health check started".into()));
    let _ = tx.send(UiMessage::Progress { step: 5, value: 0.6 });

    let conn = Connection::open(db_path)?;

    // Ensure database is properly checkpointed before health check
    let _ = tx.send(UiMessage::Status("Optimizing database before health check...".into()));
    let _ = tx.send(UiMessage::Progress { step: 5, value: 0.7 });
    if let Err(e) = conn.query_row("PRAGMA wal_checkpoint(RESTART)", [], |row| row.get::<_, i64>(0)) {
        println!("WAL checkpoint warning: {}", e);
    }

    let result = run_health_check(&conn, config, tx)?;

    let _ = tx.send(UiMessage::Progress { step: 5, value: 0.9 });
    if !result.integrity_ok {
        println!("Warning: Database health check found issues");
        let _ = tx.send(UiMessage::Status("Health check found issues".into()));
    } else {
        let _ = tx.send(UiMessage::Status("Health check passed".into()));
    }

    let _ = tx.send(UiMessage::Progress { step: 5, value: 1.0 });
    Ok(())
}

fn run_health_check(conn: &Connection, config: &HealthCheckConfig, tx: &Sender<UiMessage>) -> Result<HealthCheckResult> {
    println!("Running database health check...");
    tx.send(UiMessage::Status("Running database health check...".into()))?;
    
    let mut result = HealthCheckResult {
        check_date: Utc::now(),
        integrity_ok: true,
        duplicate_count: 0,
        orphan_count: 0,
        total_entries: 0,
        errors: Vec::new(),
        repairs_made: Vec::new(),
    };
    
    // Check integrity
    if config.check_integrity {
        tx.send(UiMessage::Status("Checking database integrity...".into()))?;
        
        // Use a more comprehensive integrity check
        let integrity_result: Result<String, _> = conn.query_row(
            "PRAGMA integrity_check(1)",  // Limit to 1 error for faster check
            [],
            |row| row.get(0)
        );
        
        match integrity_result {
            Ok(check_result) => {
                if check_result == "ok" {
                    result.repairs_made.push("Database integrity check passed".to_string());
                    tx.send(UiMessage::Status("Integrity check passed".into()))?;
                } else {
                    result.integrity_ok = false;
                    result.errors.push(format!("Integrity issues found: {}", check_result));
                    tx.send(UiMessage::Status(format!("Integrity issues: {}", check_result)))?;
                    
                    if config.auto_repair {
                        tx.send(UiMessage::Status("Attempting auto-repair...".into()))?;
                        
                        // Try to fix common integrity issues
                        if let Err(e) = conn.execute("PRAGMA wal_checkpoint(TRUNCATE)", []) {
                            println!("WAL checkpoint failed: {}", e);
                        }
                        
                        // Re-check after repair attempt
                        match conn.query_row("PRAGMA integrity_check(1)", [], |row| row.get::<_, String>(0)) {
                            Ok(recheck_result) if recheck_result == "ok" => {
                                result.repairs_made.push("Database integrity repaired successfully".to_string());
                                result.integrity_ok = true;
                                tx.send(UiMessage::Status("Auto-repair successful".into()))?;
                            }
                            Ok(recheck_result) => {
                                result.repairs_made.push(format!("Auto-repair attempted. Remaining issues: {}", recheck_result));
                                tx.send(UiMessage::Status("Auto-repair completed with warnings".into()))?;
                            }
                            Err(e) => {
                                result.repairs_made.push(format!("Auto-repair failed: {}", e));
                                tx.send(UiMessage::Status("Auto-repair failed".into()))?;
                            }
                        }
                    }
                }
            }
            Err(e) => {
                result.integrity_ok = false;
                result.errors.push(format!("Integrity check failed: {}", e));
                tx.send(UiMessage::Status("Integrity check failed".into()))?;
                if config.auto_repair {
                    tx.send(UiMessage::Status("Attempting auto-repair...".into()))?;
                    result.repairs_made.push("Attempted auto-repair for integrity issues".to_string());
                    tx.send(UiMessage::Status("Auto-repair completed".into()))?;
                }
            }
        }
    }
    
    // Check for duplicates
    if config.check_duplicates {
        tx.send(UiMessage::Status("Checking for duplicate entries...".into()))?;
        let duplicate_count: u64 = conn.query_row(
            "SELECT COUNT(*) - COUNT(DISTINCT avatar_id) FROM avatars",
            [],
            |row| row.get(0),
        ).optional()?.unwrap_or(0);
        
        result.duplicate_count = duplicate_count;
        if duplicate_count > 0 {
            result.errors.push(format!("Found {} duplicate avatar IDs", duplicate_count));
            tx.send(UiMessage::Status(format!("Found {} duplicates", duplicate_count)))?;
            if config.auto_repair {
                tx.send(UiMessage::Status("Removing duplicate entries...".into()))?;
                // Remove duplicates (keep first occurrence)
                let removed = conn.execute(
                    "DELETE FROM avatars WHERE rowid NOT IN (
                        SELECT MIN(rowid) FROM avatars GROUP BY avatar_id
                    )",
                    [],
                )?;
                result.repairs_made.push(format!("Removed {} duplicate entries", removed));
                tx.send(UiMessage::Status(format!("Removed {} duplicates", removed)))?;
            }
        } else {
            tx.send(UiMessage::Status("No duplicates found".into()))?;
        }
    }
    
    // Get total entry count
    tx.send(UiMessage::Status("Counting database entries...".into()))?;
    result.total_entries = conn.query_row("SELECT COUNT(*) FROM avatars", [], |row| row.get(0)).optional()?.unwrap_or(0);
    
    println!("Health check completed. Issues found: {}", result.errors.len());
    tx.send(UiMessage::Status("Health check completed".into()))?;
    Ok(result)
}

// Multi-source support functions
fn get_data_sources() -> Vec<DataSource> {
    vec![
        DataSource {
            name: "Primary GitHub".to_string(),
            url: URLS[0].to_string(),
            priority: 1,
            enabled: true,
            is_fallback: false,
        },
        DataSource {
            name: "Backup Prismic".to_string(),
            url: BACKUP_URLS[0].to_string(),
            priority: 2,
            enabled: true,
            is_fallback: true,
        },
    ]
}

async fn download_from_source(client: &Client, url: &str, _step: usize, tx: &Sender<UiMessage>) -> Result<Vec<u8>> {
    // Use existing download logic but with specific URL
    download_bytes_with_speed(client, url, tx).await
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

    // Match the JavaScript implementation exactly
    for i in (0..crypt.len()).rev() {
        let prev = if i == 0 { crypt.len() - 1 } else { i - 1 };
        let k = crypt[i] ^ crypt[prev] ^ iv[i];
        crypt[i] = k;
    }

    // Convert to hex string and reverse the entire string, then insert dashes
    let hex_string: String = crypt.iter().map(|b| format!("{:02x}", b)).collect();
    let mut reversed: String = hex_string.chars().rev().collect();

    if reversed.len() != 32 {
        return Err(anyhow!("Decoded avatar ID must be 32 hex chars, got {}", reversed.len()));
    }

    let mut formatted = reversed;
    // Insert dashes at positions matching JavaScript splice indices
    // After reversing 32 chars: splice at 8, 13, 18, 23
    formatted.insert(8, '-');
    formatted.insert(13, '-');
    formatted.insert(18, '-');
    formatted.insert(23, '-');

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

#[allow(dead_code)]
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

    let meta_bytes = reader.read_bytes(2)?; // platform + format version
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
    reader.read_int24()?; // file authors, not needed

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

async fn fetch_gist_version(client: &Client, gist_id: &str) -> Option<String> {
    let url = format!("https://api.github.com/gists/{}", gist_id);
    
    match client.get(&url).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                match resp.json::<serde_json::Value>().await {
                    Ok(json) => {
                        if let Some(commits) = json.get("commits").and_then(|commits: &serde_json::Value| commits.get(0)) {
                            if let Some(version) = commits.get("version").and_then(|v: &serde_json::Value| v.as_str()) {
                                return Some(version.to_string());
                            }
                        }
                    }
                    Err(_) => {}
                }
            }
        }
        Err(_) => {}
    }
    
    None
}

fn cache_paths(cache_dir: &Path) -> (PathBuf, PathBuf, PathBuf) {
    (
        cache_dir.join("avatars.db"),      // SQLite database
        cache_dir.join("cache_meta.json"),  // Metadata still JSON
        cache_dir.join("avatar_entries.ndjson"), // Keep for compatibility
    )
}

fn load_cache(cache_dir: &Path, gist_version: Option<&str>, tx: &Sender<UiMessage>) -> Result<Option<AvatarData>> {
    let config = load_config();
    let (db_path, meta_path, _) = cache_paths(cache_dir);
    
    println!("Looking for cache files:");
    println!("  Database path: {}", db_path.display());
    println!("  Meta path: {}", meta_path.display());
    println!("  Database exists: {}", db_path.exists());
    println!("  Meta exists: {}", meta_path.exists());
    
    if !db_path.exists() {
        println!("Database file doesn't exist");
        return Ok(None);
    }

    // If metadata exists, use it for version and age checking
    if meta_path.exists() {
        let meta: CacheMeta = serde_json::from_slice(&fs::read(&meta_path)?)?;
        println!("Cache meta: version={}, saved_at={}", meta.gist_version, meta.saved_at);
        
        // Check version if we have one (GitHub API succeeded)
        if let Some(version) = gist_version {
            println!("Expected version: {}", version);
            if meta.gist_version != version {
                println!("Version mismatch: cache={} vs expected={}", meta.gist_version, version);
                return Ok(None);
            }
        } else {
            println!("No version available (GitHub API failed), checking cache age only");
        }

        // Check if cache is expired based on configuration
        if is_cache_expired(&meta, &config.cache) {
            println!("Cache is expired (older than {} hours)", config.cache.max_age_hours);
            return Ok(None);
        }
    } else {
        // Metadata doesn't exist, check database file modification time
        println!("Metadata file missing, checking database file age");
        let db_modified = fs::metadata(&db_path)?.modified()?;
        let cache_age = Utc::now() - DateTime::<Utc>::from(db_modified);
        
        if cache_age > Duration::hours(config.cache.max_age_hours as i64) {
            println!("Database file is expired (older than {} hours)", config.cache.max_age_hours);
            return Ok(None);
        }
        println!("Database file age: {} hours (within {} hour limit)",
            cache_age.num_hours(), config.cache.max_age_hours);
    }

    // Get last_update from cache metadata or database file modification time
    let last_update = if meta_path.exists() {
        let meta: CacheMeta = serde_json::from_slice(&fs::read(&meta_path)?)?;
        meta.saved_at[..10].to_string() // Extract date part from RFC3339
    } else {
        let db_modified = fs::metadata(&db_path)?.modified()?;
        DateTime::<Utc>::from(db_modified).format("%Y-%m-%d").to_string()
    };

    // Load data from database
    let conn = Connection::open(&db_path)?;
    let (avatar_count, author_count) = get_database_stats(&conn)?;
    let sample_entries = get_avatar_sample(&conn, 50)?;

    // Run health check in background if configured
    if config.health_check.check_integrity {
        let db_path = db_path.clone();
        let tx_clone = tx.clone();
        let health_config = config.health_check.clone();

        tokio::task::spawn_blocking(move || {
            if let Err(e) = run_background_health_check(&db_path, &health_config, &tx_clone) {
                // Ignore channel closure errors - expected when main thread finishes
                if !e.to_string().contains("sending on a closed channel") {
                    println!("Background health check error: {}", e);
                }
            }
        });

        println!("Database health check running in background...");
        tx.send(UiMessage::Status("Database health check running in background...".into()))?;
    }

    println!("Cache loaded successfully from SQLite database");
    Ok(Some(AvatarData {
        avatar_count,
        author_count,
        last_update,
        entries: sample_entries,
    }))
}

fn create_sqlite_database(db_path: &Path) -> Result<Connection> {
    let config = load_config();
    let conn = Connection::open(db_path)?;
    
    // Create database version table
    create_database_version_table(&conn)?;
    
    // Drop and recreate table to ensure correct schema
    conn.execute("DROP TABLE IF EXISTS avatars", [])?;
    
    // Create avatars table WITHOUT indexes for ultra-fast bulk insert
    conn.execute(
        "CREATE TABLE avatars (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            author TEXT NOT NULL,
            description TEXT,
            avatar_id TEXT NOT NULL,
            tags TEXT,  -- JSON array for flexibility
            quest INTEGER DEFAULT FALSE,      -- INTEGER for boolean compatibility
            ios INTEGER DEFAULT FALSE,        -- INTEGER for boolean compatibility
            platform INTEGER DEFAULT 0,
            impostor_platform INTEGER DEFAULT 0,
            pc_rating INTEGER DEFAULT 0,
            quest_rating INTEGER DEFAULT 0,
            ios_rating INTEGER DEFAULT 0,
            content_warnings INTEGER DEFAULT 0,
            style_filter INTEGER DEFAULT 0,
            marketplace INTEGER DEFAULT FALSE, -- INTEGER for boolean compatibility
            version INTEGER DEFAULT 1,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    )?;
    
    // Enable WAL mode for better performance
    let _journal_mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
    
    // Optimize for maximum bulk insert speed using configuration
    let sync_result = match config.database.synchronous_mode.as_str() {
        "OFF" => conn.execute("PRAGMA synchronous = OFF", [])?,
        "NORMAL" => conn.execute("PRAGMA synchronous = NORMAL", [])?,
        "FULL" => conn.execute("PRAGMA synchronous = FULL", [])?,
        _ => conn.execute("PRAGMA synchronous = OFF", [])?,
    };
    // Use the result to avoid unused variable warning
    let _ = sync_result;
    
    conn.execute(&format!("PRAGMA cache_size = -{}", config.database.cache_size_mb), [])?;
    conn.execute("PRAGMA temp_store = MEMORY", [])?;
    let _mmap_size: i64 = conn.query_row(&format!("PRAGMA mmap_size = {}", config.database.mmap_size), [], |row| row.get(0))?;
    
    // Record initial database version
    conn.execute(
        "INSERT OR REPLACE INTO database_version (version, migration_date, description) VALUES (?, ?, ?)",
        params![1, Utc::now().to_rfc3339(), "Initial database creation"],
    )?;
    
    println!("SQLite database created without indexes for ultra-fast bulk insert");
    Ok(conn)
}

fn insert_avatar_batch(tx: &mut rusqlite::Transaction, avatars: &[AvatarEntry]) -> Result<()> {
    if avatars.is_empty() {
        return Ok(());
    }
    
    // Use prepared statement for maximum speed
    let mut stmt = tx.prepare(
        "INSERT OR REPLACE INTO avatars (
            id, name, author, description, avatar_id, tags, quest, ios,
            platform, impostor_platform, pc_rating, quest_rating, ios_rating,
            content_warnings, style_filter, marketplace
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )?;
    
    // Execute all in one go using rusqlite's optimized batch execution
    for avatar in avatars {
        let tags_json = serde_json::to_string(&avatar.tags)?;
        
        stmt.execute((
            &avatar.avatar_id,
            &avatar.name,
            &avatar.author,
            &avatar.description,
            &avatar.avatar_id,
            &tags_json,
            &(if avatar.quest { 1i32 } else { 0i32 }),
            &(if avatar.ios { 1i32 } else { 0i32 }),
            &(avatar.platform as i64),
            &(avatar.impostor_platform as i64),
            &(avatar.pc_rating as i64),
            &(avatar.quest_rating as i64),
            &(avatar.ios_rating as i64),
            &(avatar.content_warnings as i64),
            &(avatar.style_filter as i64),
            &(if avatar.marketplace { 1i32 } else { 0i32 })
        ))?;
    }
    
    Ok(())
}

fn get_avatar_sample(conn: &Connection, limit: i64) -> Result<Vec<AvatarEntry>> {
    let mut stmt = conn.prepare(
        "SELECT name, author, description, avatar_id, tags, 
                CASE WHEN quest = 1 THEN 'true' ELSE 'false' END as quest_str,
                CASE WHEN ios = 1 THEN 'true' ELSE 'false' END as ios_str,
                platform, impostor_platform, pc_rating, quest_rating, ios_rating,
                content_warnings, style_filter,
                CASE WHEN marketplace = 1 THEN 'true' ELSE 'false' END as marketplace_str
         FROM avatars 
         ORDER BY created_at DESC 
         LIMIT ?"
    )?;
    
    let avatar_iter = stmt.query_map([limit], |row| {
        let tags_json: String = row.get(5)?;
        let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
        
        Ok(AvatarEntry {
            name: row.get(0)?,
            author: row.get(1)?,
            description: row.get(2)?,
            avatar_id: row.get(3)?,
            tags,
            quest: row.get::<_, String>(5)? == "true",  // quest_str at index 5
            ios: row.get::<_, String>(6)? == "true",    // ios_str at index 6
            platform: row.get::<_, i64>(7)? as u8,     // platform at index 7
            impostor_platform: row.get::<_, i64>(8)? as u8, // impostor_platform at index 8
            pc_rating: row.get::<_, i64>(9)? as u8,   // pc_rating at index 9
            quest_rating: row.get::<_, i64>(10)? as u8, // quest_rating at index 10
            ios_rating: row.get::<_, i64>(11)? as u8,   // ios_rating at index 11
            content_warnings: row.get::<_, i64>(12)? as u8, // content_warnings at index 12
            style_filter: row.get::<_, i64>(13)? as u16,  // style_filter at index 13
            marketplace: row.get::<_, String>(14)? == "true", // marketplace_str at index 14
        })
    })?;
    
    let mut avatars = Vec::new();
    for avatar in avatar_iter {
        avatars.push(avatar?);
    }
    
    Ok(avatars)
}

fn get_database_stats(conn: &Connection) -> Result<(u32, u32)> {
    let avatar_count: i64 = conn.query_row("SELECT COUNT(*) FROM avatars", [], |row| row.get(0))?;
    let author_count: i64 = conn.query_row("SELECT COUNT(DISTINCT author) FROM avatars", [], |row| row.get(0))?;
    
    Ok((avatar_count as u32, author_count as u32))
}

#[allow(dead_code)]
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

#[allow(dead_code)]
async fn download_bytes_with_speed(client: &Client, url: &str, tx: &Sender<UiMessage>) -> Result<Vec<u8>> {
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
        .await
        .map_err(|e| anyhow!("GET {}: {}", url, e))?;
    
    if !resp.status().is_success() {
        return Err(anyhow!("Failed to download {}: status {}", url, resp.status()));
    }
    
    let total_size = resp.content_length().unwrap_or(0);
    let bytes = resp.bytes().await?;
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

async fn download_with_fallback_async(client: &Client, step: usize, tx: &Sender<UiMessage>) -> Result<Vec<u8>> {
    let sources = get_data_sources();
    let mut last_error = None;
    
    for source in sources.iter().filter(|s| s.enabled) {
        match download_bytes_with_speed_async(client, &source.url, tx).await {
            Ok(data) => {
                if !source.is_fallback {
                    let _ = tx.send(UiMessage::Status(format!("Downloaded {} from {}", 
                        ["PC", "Quest", "iOS"][step], source.name)));
                }
                return Ok(data);
            }
            Err(e) => {
                last_error = Some(e);
                continue;
            }
        }
    }
    
    Err(last_error.unwrap_or_else(|| anyhow!("All download sources failed")))
}

async fn download_bytes_with_speed_async(client: &Client, url: &str, tx: &Sender<UiMessage>) -> Result<Vec<u8>> {
    let start_time = std::time::Instant::now();
    
    // Send initial speed update
    let _ = tx.send(UiMessage::DownloadSpeed {
        speed: 0.0,
        downloaded: 0,
        total: 0,
    });
    
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| anyhow!("GET {}: {}", url, e))?;
    
    if !resp.status().is_success() {
        return Err(anyhow!("Failed to download {}: status {}", url, resp.status()));
    }
    
    let total_size = resp.content_length().unwrap_or(0);
    let bytes = resp.bytes().await?;
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

#[allow(dead_code)]
async fn download_with_fallback(client: &Client, _step: usize, tx: &Sender<UiMessage>) -> Result<Vec<u8>> {
    let sources = get_data_sources();
    let mut last_error = None;
    
    for source in sources.iter().filter(|s| s.enabled) {
        match download_bytes_with_speed_async(client, &source.url, tx).await {
            Ok(data) => {
                if !source.is_fallback {
                    tx.send(UiMessage::Status(format!("Downloaded from {}", source.name)))?;
                }
                return Ok(data);
            }
            Err(e) => {
                last_error = Some(e);
                if !source.is_fallback {
                    tx.send(UiMessage::Status(format!("Failed to download from {}, trying fallback...", source.name)))?;
                }
                continue;
            }
        }
    }
    
    Err(last_error.unwrap_or_else(|| anyhow!("All sources failed")))
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
        // Create tokio runtime inside this thread
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        rt.block_on(async {
            if let Err(err) = fetch_and_process(&tx).await {
                let _ = tx.send(UiMessage::Error(err.to_string()));
            }
        });
    });
}

fn process_and_stream_databases(
    cache_dir: &Path,
    main_bytes: Vec<u8>,
    quest_bytes: Vec<u8>,
    ios_bytes: Vec<u8>,
    tx: &Sender<UiMessage>,
    gist_version: Option<String>
) -> Result<(u32, u32, String)> {
    use std::io::{BufWriter, Write};
    
    let config = load_config();
    
    // Ensure cache directory exists
    fs::create_dir_all(cache_dir)?;
    
    // Clean up old cache files if configured
    cleanup_old_cache(&config.cache)?;
    tx.send(UiMessage::Status("Cache cleanup completed".into()))?;
    
    // Use the existing parsing functions that work
    tx.send(UiMessage::Status("Parsing main database...".into()))?;
    tx.send(UiMessage::Progress { step: 3, value: 0.33 })?;
    let mut processed = parse_main_database(main_bytes)?;
    tx.send(UiMessage::Status("Parsing quest database...".into()))?;
    tx.send(UiMessage::Progress { step: 3, value: 0.66 })?;
    let quest_ids = parse_aux_database(quest_bytes)?;
    tx.send(UiMessage::Status("Parsing iOS database...".into()))?;
    tx.send(UiMessage::Progress { step: 3, value: 1.0 })?;
    let ios_ids = parse_aux_database(ios_bytes)?;

    tx.send(UiMessage::Status("Marking platform compatibility...".into()))?;
    mark_platform(&mut processed, &quest_ids, PlatformFlag::Quest);
    mark_platform(&mut processed, &ios_ids, PlatformFlag::Ios);

    tx.send(UiMessage::Status("Creating database structure...".into()))?;
    // Create SQLite database
    let db_path = cache_dir.join("avatars.db");
    let mut conn = create_sqlite_database(&db_path)?;
    
    tx.send(UiMessage::Status("Starting database transaction...".into()))?;
    // Begin transaction for bulk operations
    let mut db_tx = conn.transaction()?;
    
    let mut author_set = std::collections::HashSet::new();
    let last_update = processed.data.last_update.clone();
    let total_entries = processed.data.entries.len();
    
    // Process in batches from configuration
    let batch_size = config.database.batch_size;
    let mut batch = Vec::with_capacity(batch_size);
    
    // Use time-based progress updates for smoothness
    use std::time::{Duration, Instant};
    let start_time = Instant::now();
    let mut last_progress_update = start_time;
    const PROGRESS_UPDATE_INTERVAL: Duration = Duration::from_millis(100); // Update every 100ms

    for (i, entry) in processed.data.entries.into_iter().enumerate() {
        // Update progress based on time for smooth loading bar
        let now = Instant::now();
        if now.duration_since(last_progress_update) >= PROGRESS_UPDATE_INTERVAL {
            let progress = (i as f32) / (total_entries as f32);
            let _ = tx.send(UiMessage::Progress { step: 4, value: progress });
            last_progress_update = now;
        }

        author_set.insert(entry.author.clone());
        
        // Skip NDJSON writing during bulk insert for speed
        // We'll create it after the database is complete
        
        // Add to SQLite batch
        batch.push(entry);
        
        // Insert batch when full
        if batch.len() >= batch_size {
            insert_avatar_batch(&mut db_tx, &batch)?;
            batch.clear();
            
            // Update status less frequently to avoid spam
            if i % 100000 == 0 && i > 0 {
                tx.send(UiMessage::Status(format!("Inserted {} avatars into database...", i + 1)))?;
            }
        }
    }
    
    // Insert remaining entries
    if !batch.is_empty() {
        insert_avatar_batch(&mut db_tx, &batch)?;
    }
    
    // Commit the large transaction first
    tx.send(UiMessage::Status("Committing database transaction...".into()))?;
    db_tx.commit()?;
    
    // Create indexes after bulk insert if configured
    if config.database.enable_indexes {
        tx.send(UiMessage::Status("Creating search indexes...".into()))?;
        println!("Creating indexes for instant searches...");
        conn.execute("CREATE INDEX IF NOT EXISTS idx_name ON avatars(name)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_author ON avatars(author)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_avatar_id ON avatars(avatar_id)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_quest ON avatars(quest)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_ios ON avatars(ios)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_marketplace ON avatars(marketplace)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_pc_rating ON avatars(pc_rating)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_quest_rating ON avatars(quest_rating)", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_ios_rating ON avatars(ios_rating)", [])?;
        println!("Indexes created successfully!");
        tx.send(UiMessage::Status("Indexes created successfully".into()))?;
    }
    
    // Re-enable normal safety settings
    conn.execute("PRAGMA synchronous = NORMAL", [])?;
    
    // Create backup before finalizing if configured
    if config.cache.backup_count > 0 {
        tx.send(UiMessage::Status("Creating backup...".into()))?;
        if let Err(e) = create_backup(&conn, &config.cache) {
            println!("Warning: Failed to create backup: {}", e);
            tx.send(UiMessage::Status("Backup creation failed".into()))?;
        } else {
            tx.send(UiMessage::Status("Backup created successfully".into()))?;
        }
    }
    
    // Run health check in background if configured
    if config.health_check.check_integrity {
        let db_path = db_path.clone();
        let tx_clone = tx.clone();
        let health_config = config.health_check.clone();

        tokio::task::spawn_blocking(move || {
            if let Err(e) = run_background_health_check(&db_path, &health_config, &tx_clone) {
                // Ignore channel closure errors - expected when main thread finishes
                if !e.to_string().contains("sending on a closed channel") {
                    println!("Background health check error: {}", e);
                }
            }
        });

        tx.send(UiMessage::Status("Database health check running in background...".into()))?;
        tx.send(UiMessage::Progress { step: 5, value: 0.5 })?;
    } else {
        tx.send(UiMessage::Progress { step: 5, value: 1.0 })?;
    }
    
    // Get database statistics
    let (avatar_count, author_count) = get_database_stats(&conn)?;
    
    // Get sample entries for UI display
    let sample_entries = get_avatar_sample(&conn, 50)?;
    
    // Create cache metadata
    if let Some(ref version) = gist_version {
        let current_version = get_current_database_version(&conn)?;
        let meta = CacheMeta {
            gist_version: version.to_string(),
            saved_at: Utc::now().to_rfc3339(),
            database_version: current_version,
            config: config.clone(),
        };
        let meta_path = cache_dir.join("cache_meta.json");
        let meta_bytes = serde_json::to_vec_pretty(&meta)?;
        fs::write(meta_path, meta_bytes)?;
    }
    
    println!("SQLite database created with {} avatars from {} authors", avatar_count, author_count);
    
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
    
    Ok((avatar_count, author_count, last_update))
}

async fn fetch_and_process(tx: &Sender<UiMessage>) -> Result<()> {
    tx.send(UiMessage::Status("Initializing client".into()))?;
    let client = Client::builder()
        .user_agent("vrchat-avatar-gui/0.1")
        .build()?;

    let cache_dir = PathBuf::from("cache");
    println!("Current working directory: {:?}", std::env::current_dir());
    println!("Cache directory: {}", cache_dir.display());
    
    let gist_id = gist_id_from_url(URLS[0]).ok_or_else(|| anyhow!("Unable to parse gist ID"))?;
    tx.send(UiMessage::Status("Checking cache".into()))?;
    let gist_version = fetch_gist_version(&client, &gist_id).await;

    // Try to load cache - this will check age and health even if version fetch failed
    if let Some(data) = load_cache(&cache_dir, gist_version.as_deref(), &tx)? {
        tx.send(UiMessage::Status("Loaded cached database".into()))?;
        tx.send(UiMessage::Completed { data, cached: true })?;
        return Ok(());
    }

    tx.send(UiMessage::Status("Downloading all databases in parallel...".into()))?;
    
    // Download all 3 databases in parallel for maximum speed
    let (main_bytes, quest_bytes, ios_bytes) = tokio::join! {
        download_with_fallback_async(&client, 0, &tx),
        download_with_fallback_async(&client, 1, &tx),
        download_with_fallback_async(&client, 2, &tx)
    };
    
    // Handle results
    let main_bytes = main_bytes.context("Failed to download main database")?;
    let quest_bytes = quest_bytes.context("Failed to download quest database")?;
    let ios_bytes = ios_bytes.context("Failed to download iOS database")?;
    
    // Mark all downloads as complete
    tx.send(UiMessage::Progress { step: 0, value: 1.0 })?;
    tx.send(UiMessage::Progress { step: 1, value: 1.0 })?;
    tx.send(UiMessage::Progress { step: 2, value: 1.0 })?;

    tx.send(UiMessage::Status("Processing data".into()))?;

    // Process and stream directly to files to reduce memory usage
    let (_avatar_count, _author_count, _last_update) = process_and_stream_databases(
        &cache_dir,
        main_bytes,
        quest_bytes,
        ios_bytes,
        &tx,
        gist_version.clone()  // Pass gist_version
    )?;

    tx.send(UiMessage::Progress { step: 4, value: 1.0 })?;
    tx.send(UiMessage::Progress { step: 5, value: 1.0 })?;

    // All cache files are already created in process_and_stream_databases
    println!("Database processing and caching complete");

    Ok(())
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct AppState {
    receiver: Receiver<UiMessage>,
    status: String,
    progress: [f32; 6],
    result: Option<AvatarData>,
    error: Option<String>,
    cached: bool,
    download_speed: f64,      // bytes per second
    downloaded_bytes: u64,   // total downloaded bytes
    total_bytes: u64,       // total file size
    config: AppConfig,
    show_settings: bool,
    last_health_check: Option<HealthCheckResult>,
    cache_info: Option<CacheInfo>,
    start_time: std::time::Instant,  // Track when operation started
    end_time: Option<std::time::Instant>,  // Track when operation completed
    
    // Logo animation fields
    logo_rotation: f32,        // Current rotation angle in radians
    logo_position: f32,        // Current Y position (0.0 = bottom, 1.0 = final position)
    logo_animation_time: f32,  // Time elapsed for animation
    logo_visible: bool,        // Whether logo should be shown
    logo_image_data: Option<egui::ColorImage>,  // Loaded logo image data
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct CacheInfo {
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    size_bytes: u64,
    database_version: u32,
    is_expired: bool,
}

impl AppState {
    pub fn new(receiver: Receiver<UiMessage>) -> Self {
        let config = load_config();
        
        // Try to load logo image from assets folder
        let logo_image_data = Self::load_logo_image();
        
        Self {
            receiver,
            status: "Initializing...".into(),
            progress: [0.0; 6],
            result: None,
            error: None,
            cached: false,
            download_speed: 0.0,
            downloaded_bytes: 0,
            total_bytes: 0,
            config,
            show_settings: false,
            last_health_check: None,
            cache_info: None,
            start_time: std::time::Instant::now(),
            end_time: None,  // No end time initially
            
            // Initialize logo animation
            logo_rotation: 0.0,
            logo_position: 0.0,  // Start at bottom
            logo_animation_time: 0.0,
            logo_visible: true,   // Show logo on startup
            logo_image_data,
        }
    }
    
    fn load_logo_image() -> Option<egui::ColorImage> {
        // Embed the logo image directly in the binary at compile time
        // This eliminates the need for external asset files
        const LOGO_DATA: &[u8] = include_bytes!("../assets/logo_prismic.png");
        
        // Load the embedded image data
        if let Ok(image) = image::load_from_memory(LOGO_DATA) {
            let rgba_image = image.to_rgba8();
            let size = [rgba_image.width() as usize, rgba_image.height() as usize];
            
            // Create color image from embedded data
            println!("Loaded embedded logo image");
            Some(egui::ColorImage::from_rgba_unmultiplied(size, &rgba_image.into_raw()))
        } else {
            println!("Failed to load embedded logo image, using fallback");
            None
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
                    // Record completion time
                    self.end_time = Some(std::time::Instant::now());
                }
                UiMessage::Error(err) => self.error = Some(err),
            }
        }
        ctx.request_repaint();
    }
    
    fn draw_animated_logo_at_position(&self, ui: &mut egui::Ui, logo_rect: egui::Rect) {
        // Check if we have a loaded image
        if let Some(image_data) = &self.logo_image_data {
            // Load the image as texture
            let texture_handle = ui.ctx().load_texture(
                "logo",
                image_data.clone(),
                egui::TextureOptions::default()
            );
            
            // Create painter
            let painter = ui.painter();
            
            // Calculate rotated corners for the image
            let center = logo_rect.center();
            let cos_r = self.logo_rotation.cos();
            let sin_r = self.logo_rotation.sin();
            
            let corners = [
                egui::pos2(logo_rect.min.x - center.x, logo_rect.min.y - center.y),
                egui::pos2(logo_rect.max.x - center.x, logo_rect.min.y - center.y),
                egui::pos2(logo_rect.max.x - center.x, logo_rect.max.y - center.y),
                egui::pos2(logo_rect.min.x - center.x, logo_rect.max.y - center.y),
            ];
            
            let rotated_corners = corners
                .iter()
                .map(|&p| egui::pos2(
                    center.x + p.x * cos_r - p.y * sin_r,
                    center.y + p.x * sin_r + p.y * cos_r
                ))
                .collect::<Vec<_>>();
            
            // Draw the rotated image as a mesh
            let uv = [
                egui::pos2(0.0, 0.0),
                egui::pos2(1.0, 0.0),
                egui::pos2(1.0, 1.0),
                egui::pos2(0.0, 1.0),
            ];
            
            let mesh = egui::Mesh {
                vertices: std::iter::zip(rotated_corners, uv)
                    .map(|(pos, uv)| egui::epaint::Vertex {
                        pos,
                        uv,
                        color: egui::Color32::WHITE,
                    })
                    .collect(),
                indices: vec![0, 1, 2, 2, 3, 0],
                texture_id: texture_handle.id(),
            };
            
            painter.add(egui::Shape::mesh(mesh));
        } else {
            // Fallback to custom drawn logo if no image loaded
            self.draw_fallback_logo_at_position(ui, logo_rect);
        }
        
        // Add glow effect
        if self.logo_animation_time < 2.0 {
            let painter = ui.painter();
            let glow_alpha = (1.0 - self.logo_animation_time / 2.0) * 0.3;
            painter.circle_filled(
                logo_rect.center(),
                logo_rect.width() * 0.5,
                egui::Color32::from_rgba_unmultiplied(100, 200, 255, (glow_alpha * 255.0) as u8)
            );
        }
    }
    
    fn draw_fallback_logo_at_position(&self, ui: &mut egui::Ui, logo_rect: egui::Rect) {
        // Create a painter for the logo
        let painter = ui.painter();
        
        // Draw spinning logo using custom shapes
        // Draw a stylized "P" logo (for Prismic)
        let stroke = egui::Stroke::new(4.0, egui::Color32::from_rgb(100, 200, 255)); // Light blue
        let fill = egui::Color32::from_rgb(50, 150, 220); // Darker blue
        
        // Main circle
        painter.circle_filled(logo_rect.center(), logo_rect.width() * 0.4, fill);
        painter.circle_stroke(logo_rect.center(), logo_rect.width() * 0.4, stroke);
        
        // Letter "P" shape - calculate rotated positions
        let p_stroke = egui::Stroke::new(6.0, egui::Color32::WHITE);
        
        // Vertical line of "P" (rotated)
        let cos_r = self.logo_rotation.cos();
        let sin_r = self.logo_rotation.sin();
        
        let p1 = egui::pos2(-logo_rect.width() * 0.15, -logo_rect.width() * 0.25);
        let p2 = egui::pos2(-logo_rect.width() * 0.15, logo_rect.width() * 0.25);
        
        // Rotate points around center
        let rotated_p1 = egui::pos2(
            logo_rect.center().x + p1.x * cos_r - p1.y * sin_r,
            logo_rect.center().y + p1.x * sin_r + p1.y * cos_r
        );
        let rotated_p2 = egui::pos2(
            logo_rect.center().x + p2.x * cos_r - p2.y * sin_r,
            logo_rect.center().y + p2.x * sin_r + p2.y * cos_r
        );
        
        painter.line_segment([rotated_p1, rotated_p2], p_stroke);
        
        // Add decorative spinning elements
        for i in 0..6 {
            let angle = (i as f32) * std::f32::consts::PI / 3.0 + self.logo_rotation;
            let x = logo_rect.width() * 0.5 * angle.cos();
            let y = logo_rect.width() * 0.5 * angle.sin();
            
            let dot_size = 8.0;
            painter.circle_filled(
                egui::pos2(logo_rect.center().x + x, logo_rect.center().y + y),
                dot_size,
                egui::Color32::from_rgba_unmultiplied(255, 255, 255, 180)
            );
        }
    }

    pub fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_messages(ctx);
        
        // Update logo animation
        if self.logo_visible {
            let dt = ctx.input(|i| i.stable_dt) as f32;
            self.logo_animation_time += dt;
            
            // Spin the logo continuously
            self.logo_rotation += dt * 2.0; // 2 radians per second
            
            // Slide up animation (ease-out cubic)
            let animation_duration = 2.0; // 2 seconds
            if self.logo_animation_time < animation_duration {
                let t = self.logo_animation_time / animation_duration;
                let ease_out = 1.0 - (1.0 - t).powf(3.0); // Cubic ease-out
                self.logo_position = ease_out;
            } else {
                self.logo_position = 1.0;
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            // Use the full available space for proper centering
            let available_width = ui.available_width();
            
            // Create a container that takes full width and centers content
            ui.allocate_ui_with_layout(
                egui::Vec2::new(available_width, ui.available_height()),
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    // Add spacing at the top
                    ui.add_space(30.0);
                    
                    // Minimalist header with better centering
                    ui.heading("VRChat Avatar Database");
                    ui.add_space(15.0);
                    
                    // Show elapsed time prominently
                    let elapsed = if let Some(end_time) = self.end_time {
                        end_time.duration_since(self.start_time)
                    } else {
                        self.start_time.elapsed()
                    };
                    let elapsed_text = if elapsed.as_secs() < 60 {
                        format!("⏱️ Time Elapsed: {}s", elapsed.as_secs())
                    } else if elapsed.as_secs() < 3600 {
                        format!("⏱️ Time Elapsed: {}m {}s", 
                            elapsed.as_secs() / 60, 
                            elapsed.as_secs() % 60)
                    } else {
                        format!("⏱️ Time Elapsed: {}h {}m {}s", 
                            elapsed.as_secs() / 3600,
                            (elapsed.as_secs() % 3600) / 60,
                            elapsed.as_secs() % 60)
                    };
                    
                    ui.label(elapsed_text);
                    ui.add_space(10.0);
                    
                    // Show download speed information when downloading - more compact
                    if self.download_speed > 0.0 {
                        ui.add_space(10.0);
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
                    }

                    // Error message - more compact
                    if let Some(err) = &self.error {
                        ui.add_space(15.0);
                        ui.colored_label(egui::Color32::RED, format!("❌ {}", err));
                        ui.add_space(10.0);
                    }

                    // Enhanced progress section with detailed feedback - proper centering
                    ui.add_space(20.0);
                    
                    ui.heading("📊 Database Operations");
                    ui.add_space(15.0);
                    
                    // Show individual step progress with labels
                    for (i, &progress) in self.progress.iter().enumerate() {
                        if progress > 0.0 {
                            let step_label = STEP_LABELS[i];
                            let percentage = (progress * 100.0) as u32;
                            
                            // Simple centered label
                            ui.label(format!("{}: {}%", step_label, percentage));
                            
                            // Centered progress bar
                            let progress_bar = ProgressBar::new(progress)
                                .animate(true);
                            ui.add_sized([500.0, 20.0], progress_bar);
                            ui.add_space(12.0);
                        }
                    }
                    
                    // Show detailed status with timestamp - centered
                    ui.add_space(15.0);
                    ui.label(format!("📌 Status: {}", self.status));
                    
                    // Show operation details - centered
                    if self.download_speed > 0.0 {
                        ui.add_space(15.0);
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
                        
                        ui.label(format!("⬇️ Download: {} | {}", speed_text, progress_text));
                        
                        // Show estimated time remaining for downloads - centered
                        if self.total_bytes > 0 && self.downloaded_bytes < self.total_bytes {
                            let remaining_bytes = self.total_bytes - self.downloaded_bytes;
                            let eta_seconds = remaining_bytes as f64 / self.download_speed;
                            
                            ui.add_space(10.0);
                            let eta_text = if eta_seconds < 60.0 {
                                format!("⏱️ ETA: {}s", eta_seconds as u32)
                            } else if eta_seconds < 3600.0 {
                                format!("⏱️ ETA: {}m {}s", 
                                    (eta_seconds / 60.0) as u32,
                                    (eta_seconds % 60.0) as u32)
                            } else {
                                format!("⏱️ ETA: {}h {}m", 
                                    (eta_seconds / 3600.0) as u32,
                                    ((eta_seconds % 3600.0) / 60.0) as u32)
                            };
                            ui.label(eta_text);
                        }
                    }
                    
                    // Draw animated logo if visible - positioned under the download information
                    if self.logo_visible {
                        ui.add_space(30.0);
                        
                        // Get current cursor position and create logo rect
                        let logo_size = 120.0;
                        let cursor_pos = ui.cursor().min;
                        let logo_rect = egui::Rect::from_center_size(
                            egui::pos2(ui.available_width() / 2.0, cursor_pos.y + logo_size),
                            egui::vec2(logo_size, logo_size)
                        );
                        
                        // Calculate animated position
                        let start_offset = logo_size * 2.0; // Start below
                        let end_offset = 0.0; // End at current position
                        let current_offset = start_offset + (end_offset - start_offset) * self.logo_position;
                        
                        // Adjust the logo rect for animation
                        let animated_rect = egui::Rect::from_center_size(
                            egui::pos2(logo_rect.center().x, logo_rect.center().y + current_offset),
                            logo_rect.size()
                        );
                        
                        // Draw the logo using painter
                        self.draw_animated_logo_at_position(ui, animated_rect);
                        
                        ui.add_space(20.0);
                    }
                    
                    // Show cache information when loading from cache - centered
                    if self.cached {
                        ui.add_space(15.0);
                        ui.label("💾 Source: Cache (Fast Load)");
                    }
                    
                    // Results section - cleaner and more compact with better centering
                    if let Some(result) = &self.result {
                        ui.add_space(30.0);
                        
                        // Separator line
                        ui.separator();
                        ui.add_space(20.0);
                        
                        ui.heading("Database Summary");
                        ui.add_space(15.0);
                        
                        // Results in a centered grid
                        ui.label(format!("📊 Avatars: {}   👥 Authors: {}", result.avatar_count, result.author_count));
                        ui.add_space(8.0);
                        ui.label(format!("📅 Last Update: {}   💾 Source: {}", result.last_update, if self.cached { "Cache" } else { "Fresh" }));

                        // Sample entries - more compact and centered
                        ui.add_space(25.0);
                        ui.label("📋 Sample Entries:");
                        ui.add_space(15.0);
                        
                        // Center the table with fixed width
                        ui.allocate_ui_with_layout(
                            egui::Vec2::new(900.0, 350.0),
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                egui::ScrollArea::vertical()
                                    .max_height(350.0)
                                    .id_source("sample_entries")
                                    .show(ui, |ui| {
                                        TableBuilder::new(ui)
                                            .striped(true)
                                            .column(Column::initial(200.0)) // Name
                                            .column(Column::initial(130.0)) // Author
                                            .column(Column::initial(90.0))  // Platforms
                                            .column(Column::initial(110.0)) // Tags
                                            .column(Column::initial(65.0))  // PC Rating
                                            .column(Column::initial(65.0))  // Quest Rating
                                            .column(Column::initial(65.0))  // iOS Rating
                                            .column(Column::initial(90.0))  // Content Warnings
                                            .column(Column::initial(55.0))  // Marketplace
                                            .header(22.0, |mut header| {
                                                header.col(|ui| {
                                                    ui.strong("Name");
                                                });
                                                header.col(|ui| {
                                                    ui.strong("Author");
                                                });
                                                header.col(|ui| {
                                                    ui.strong("Platform");
                                                });
                                                header.col(|ui| {
                                                    ui.strong("Tags");
                                                });
                                                header.col(|ui| {
                                                    ui.strong("PC");
                                                });
                                                header.col(|ui| {
                                                    ui.strong("Quest");
                                                });
                                                header.col(|ui| {
                                                    ui.strong("iOS");
                                                });
                                                header.col(|ui| {
                                                    ui.strong("Warnings");
                                                });
                                                header.col(|ui| {
                                                    ui.strong("Market");
                                                });
                                            })
                                            .body(|mut body| {
                                                for entry in &result.entries {
                                                    body.row(20.0, |mut row| {
                                                        row.col(|ui| {
                                                            ui.label(&entry.name);
                                                        });
                                                        row.col(|ui| {
                                                            ui.label(&entry.author);
                                                        });
                                                        row.col(|ui| {
                                                            let platforms = if entry.quest && entry.ios {
                                                                "All"
                                                            } else if entry.quest {
                                                                "Quest"
                                                            } else if entry.ios {
                                                                "iOS"
                                                            } else {
                                                                "PC"
                                                            };
                                                            ui.label(platforms);
                                                        });
                                                        row.col(|ui| {
                                                            ui.label(entry.tags.first().unwrap_or(&"None".to_string()));
                                                        });
                                                        row.col(|ui| {
                                                            ui.label(rating_to_string(entry.pc_rating));
                                                        });
                                                        row.col(|ui| {
                                                            ui.label(rating_to_string(entry.quest_rating));
                                                        });
                                                        row.col(|ui| {
                                                            ui.label(rating_to_string(entry.ios_rating));
                                                        });
                                                        row.col(|ui| {
                                                            let warnings = decode_content_warnings(entry.content_warnings);
                                                            ui.label(warnings.first().unwrap_or(&"None".to_string()));
                                                        });
                                                        row.col(|ui| {
                                                            ui.label(if entry.marketplace { "✓" } else { "✗" });
                                                        });
                                                    });
                                                }
                                            });
                                    });
                            }
                        );
                    }
                }
            );
        });
    }
}

impl eframe::App for AppState {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.update(ctx, frame);
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

