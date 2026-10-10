use std::fs;
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};

use crate::models::{AvatarEntry, DatabaseStatus, SearchFilter, SearchResult};

pub struct DbManager {
    db_path: std::path::PathBuf,
}

impl DbManager {
    pub fn new(db_path: std::path::PathBuf) -> Self {
        Self { db_path }
    }

    pub fn get_connection(&self) -> Result<Connection> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch("
            PRAGMA cache_size = -64000;
            PRAGMA mmap_size = 268435456;
            PRAGMA temp_store = MEMORY;
        ")?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS favorites (
                avatar_id TEXT PRIMARY KEY,
                name TEXT,
                author TEXT,
                thumbnail_url TEXT,
                added_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;
        Ok(conn)
    }

    pub fn get_status(&self) -> DatabaseStatus {
        if !self.db_path.exists() {
            return DatabaseStatus {
                ready: false,
                avatar_count: 0,
                author_count: 0,
                last_updated: None,
                last_sync: None,
                is_stale: false,
                file_size_bytes: 0,
            };
        }

        let file_size_bytes = fs::metadata(&self.db_path)
            .map(|m| m.len())
            .unwrap_or(0);

        let conn = match self.get_connection() {
            Ok(c) => c,
            Err(_) => {
                return DatabaseStatus {
                    ready: false,
                    avatar_count: 0,
                    author_count: 0,
                    last_updated: None,
                    last_sync: None,
                    is_stale: false,
                    file_size_bytes,
                };
            }
        };

        let avatar_count: usize = conn
            .query_row("SELECT COUNT(*) FROM avatars", [], |r| r.get(0))
            .unwrap_or(0);

        let author_count: usize = conn
            .query_row("SELECT COUNT(DISTINCT author) FROM avatars", [], |r| r.get(0))
            .unwrap_or(0);

        let last_updated: Option<String> = conn
            .query_row("SELECT value FROM meta WHERE key = 'last_updated'", [], |r| r.get(0))
            .optional()
            .unwrap_or(None);

        let last_sync: Option<String> = conn
            .query_row("SELECT value FROM meta WHERE key = 'synced_at'", [], |r| r.get(0))
            .optional()
            .unwrap_or(None);

        let ready = avatar_count > 0;
        let today = chrono::Utc::now().date_naive();
        let is_stale = ready
            && match &last_updated {
                Some(d) => chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
                    .map(|dt| dt < today)
                    .unwrap_or(true),
                None => true,
            };

        DatabaseStatus {
            ready,
            avatar_count,
            author_count,
            last_updated,
            last_sync,
            is_stale,
            file_size_bytes,
        }
    }

    pub fn search(&self, filter: SearchFilter) -> Result<SearchResult> {
        let conn = self.get_connection()?;
        let mut conditions: Vec<String> = Vec::new();
        let mut query_params: Vec<rusqlite::types::Value> = Vec::new();

        if let Some(q) = &filter.query {
            let q_trimmed = q.trim();
            if !q_trimmed.is_empty() {
                let pattern = format!("%{}%", q_trimmed);
                conditions.push("(name LIKE ? OR author LIKE ? OR description LIKE ? OR tags LIKE ?)".to_string());
                query_params.push(pattern.clone().into());
                query_params.push(pattern.clone().into());
                query_params.push(pattern.clone().into());
                query_params.push(pattern.into());
            }
        }

        if let Some(n) = &filter.name {
            let n_trimmed = n.trim();
            if !n_trimmed.is_empty() {
                let method = filter.name_search_method.as_deref().unwrap_or("match_all_words");
                let inc = filter.name_include_desc_tags;
                Self::apply_text_filter("name", inc, method, n_trimmed, &mut conditions, &mut query_params);
            }
        }

        if let Some(a) = &filter.author {
            let a_trimmed = a.trim();
            if !a_trimmed.is_empty() {
                let method = filter.author_search_method.as_deref().unwrap_or("match_all_words");
                Self::apply_text_filter("author", false, method, a_trimmed, &mut conditions, &mut query_params);
            }
        }

        if let Some(d) = &filter.description {
            let d_trimmed = d.trim();
            if !d_trimmed.is_empty() {
                conditions.push("description LIKE ?".to_string());
                query_params.push(format!("%{}%", d_trimmed).into());
            }
        }

        if filter.platform_pc {
            if filter.include_impostors {
                conditions.push("((platform & 1) != 0 OR (impostor_platform & 1) != 0)".to_string());
            } else {
                conditions.push("(platform & 1) != 0".to_string());
            }
        }
        if filter.platform_quest {
            if filter.include_impostors {
                conditions.push("((platform & 2) != 0 OR (impostor_platform & 2) != 0)".to_string());
            } else {
                conditions.push("(platform & 2) != 0".to_string());
            }
        }
        if filter.platform_ios {
            if filter.include_impostors {
                conditions.push("((platform & 4) != 0 OR (impostor_platform & 4) != 0)".to_string());
            } else {
                conditions.push("(platform & 4) != 0".to_string());
            }
        }

        // Minimum Performance: 1 = Excellent .. 5 = Very Poor
        if let Some(pr) = filter.pc_rating {
            if pr > 0 && pr < 5 {
                conditions.push("pc_rating > 0 AND pc_rating <= ?".to_string());
                query_params.push((pr as i64).into());
            }
        }

        if let Some(qr) = filter.quest_rating {
            if qr > 0 && qr < 5 {
                conditions.push("quest_rating > 0 AND quest_rating <= ?".to_string());
                query_params.push((qr as i64).into());
            }
        }

        if let Some(ir) = filter.ios_rating {
            if ir > 0 && ir < 5 {
                conditions.push("ios_rating > 0 AND ios_rating <= ?".to_string());
                query_params.push((ir as i64).into());
            }
        }

        // Style Filter (Max 2)
        if !filter.selected_styles.is_empty() {
            let style_parts: Vec<String> = filter
                .selected_styles
                .iter()
                .filter(|&&s| s > 0)
                .map(|&s| format!("(style_filter & {}) != 0", s))
                .collect();
            if !style_parts.is_empty() {
                conditions.push(format!("({})", style_parts.join(" OR ")));
            }
        }

        // Allowed Content Warnings: bits 1, 2, 4, 8, 16 (mask 31)
        if let Some(allowed) = filter.allowed_warnings {
            let disallowed = 31u8 & (!allowed);
            if disallowed > 0 {
                conditions.push(format!("(content_warnings & {}) = 0", disallowed));
            }
        }

        if filter.marketplace_only {
            conditions.push("marketplace = 1".to_string());
        } else if !filter.include_marketplace {
            conditions.push("marketplace = 0".to_string());
        }

        if filter.favorites_only {
            conditions.push("avatar_id IN (SELECT avatar_id FROM favorites)".to_string());
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        // 1. Total count query
        let count_sql = format!("SELECT COUNT(*) FROM avatars {}", where_clause);
        let params_slice: Vec<&dyn rusqlite::ToSql> = query_params.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

        let total: usize = conn.query_row(&count_sql, params_slice.as_slice(), |r| r.get(0))?;

        let page_size = if filter.page_size == 0 { 24 } else { filter.page_size };
        let offset = filter.page * page_size;
        let total_pages = (total + page_size - 1) / page_size;

        // 2. Paginated data query
        let data_sql = format!(
            "SELECT id, name, author, description, avatar_id, tags, quest, ios, platform, impostor_platform, pc_rating, quest_rating, ios_rating, content_warnings, style_filter, marketplace FROM avatars {} LIMIT ? OFFSET ?",
            where_clause
        );

        let mut data_params = query_params;
        data_params.push((page_size as i64).into());
        data_params.push((offset as i64).into());
        let data_params_slice: Vec<&dyn rusqlite::ToSql> = data_params.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

        let mut stmt = conn.prepare(&data_sql)?;
        let avatar_iter = stmt.query_map(data_params_slice.as_slice(), |row| {
            let tags_json: String = row.get(5).unwrap_or_else(|_| "[]".to_string());
            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();

            Ok(AvatarEntry {
                id: row.get(0)?,
                name: row.get(1)?,
                author: row.get(2)?,
                description: row.get(3).unwrap_or_default(),
                avatar_id: row.get(4)?,
                tags,
                quest: row.get::<_, i32>(6)? != 0,
                ios: row.get::<_, i32>(7)? != 0,
                platform: row.get::<_, i64>(8)? as u8,
                impostor_platform: row.get::<_, i64>(9)? as u8,
                pc_rating: row.get::<_, i64>(10)? as u8,
                quest_rating: row.get::<_, i64>(11)? as u8,
                ios_rating: row.get::<_, i64>(12)? as u8,
                content_warnings: row.get::<_, i64>(13)? as u8,
                style_filter: row.get::<_, i64>(14)? as u16,
                marketplace: row.get::<_, i32>(15)? != 0,
            })
        })?;

        let mut avatars = Vec::with_capacity(page_size);
        for item in avatar_iter {
            avatars.push(item?);
        }

        Ok(SearchResult {
            avatars,
            total,
            page: filter.page,
            page_size,
            total_pages,
        })
    }

    pub fn get_by_avatar_id(&self, avatar_id: &str) -> Result<Option<AvatarEntry>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, author, description, avatar_id, tags, quest, ios, platform, impostor_platform, pc_rating, quest_rating, ios_rating, content_warnings, style_filter, marketplace FROM avatars WHERE avatar_id = ? LIMIT 1"
        )?;

        let result = stmt.query_row(params![avatar_id], |row| {
            let tags_json: String = row.get(5).unwrap_or_else(|_| "[]".to_string());
            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();

            Ok(AvatarEntry {
                id: row.get(0)?,
                name: row.get(1)?,
                author: row.get(2)?,
                description: row.get(3).unwrap_or_default(),
                avatar_id: row.get(4)?,
                tags,
                quest: row.get::<_, i32>(6)? != 0,
                ios: row.get::<_, i32>(7)? != 0,
                platform: row.get::<_, i64>(8)? as u8,
                impostor_platform: row.get::<_, i64>(9)? as u8,
                pc_rating: row.get::<_, i64>(10)? as u8,
                quest_rating: row.get::<_, i64>(11)? as u8,
                ios_rating: row.get::<_, i64>(12)? as u8,
                content_warnings: row.get::<_, i64>(13)? as u8,
                style_filter: row.get::<_, i64>(14)? as u16,
                marketplace: row.get::<_, i32>(15)? != 0,
            })
        }).optional()?;

        Ok(result)
    }

    pub fn toggle_favorite(
        &self,
        avatar_id: &str,
        name: Option<&str>,
        author: Option<&str>,
        thumbnail_url: Option<&str>,
    ) -> Result<bool> {
        let conn = self.get_connection()?;
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM favorites WHERE avatar_id = ?",
                params![avatar_id],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);

        if exists {
            conn.execute("DELETE FROM favorites WHERE avatar_id = ?", params![avatar_id])?;
            Ok(false)
        } else {
            conn.execute(
                "INSERT OR REPLACE INTO favorites (avatar_id, name, author, thumbnail_url) VALUES (?, ?, ?, ?)",
                params![avatar_id, name, author, thumbnail_url],
            )?;
            Ok(true)
        }
    }

    pub fn get_favorite_ids(&self) -> Result<Vec<String>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare("SELECT avatar_id FROM favorites ORDER BY added_at DESC")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        let mut ids = Vec::new();
        for r in rows {
            ids.push(r?);
        }
        Ok(ids)
    }

    pub fn is_favorite(&self, avatar_id: &str) -> Result<bool> {
        let conn = self.get_connection()?;
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM favorites WHERE avatar_id = ?",
                params![avatar_id],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);
        Ok(exists)
    }

    fn apply_text_filter(
        field: &str,
        include_desc_tags: bool,
        method: &str,
        input: &str,
        conditions: &mut Vec<String>,
        query_params: &mut Vec<rusqlite::types::Value>,
    ) {
        match method {
            "exact_phrase" => {
                if include_desc_tags {
                    conditions.push("(LOWER(name) = LOWER(?) OR LOWER(description) = LOWER(?) OR tags LIKE ?)".to_string());
                    query_params.push(input.to_string().into());
                    query_params.push(input.to_string().into());
                    query_params.push(format!("%\"{}\"%", input).into());
                } else {
                    conditions.push(format!("LOWER({}) = LOWER(?)", field));
                    query_params.push(input.to_string().into());
                }
            }
            "starts_with" => {
                let pattern = format!("{}%", input);
                if include_desc_tags {
                    conditions.push("(name LIKE ? OR description LIKE ? OR tags LIKE ?)".to_string());
                    query_params.push(pattern.clone().into());
                    query_params.push(pattern.clone().into());
                    query_params.push(pattern.into());
                } else {
                    conditions.push(format!("{} LIKE ?", field));
                    query_params.push(pattern.into());
                }
            }
            "ends_with" => {
                let pattern = format!("%{}", input);
                if include_desc_tags {
                    conditions.push("(name LIKE ? OR description LIKE ? OR tags LIKE ?)".to_string());
                    query_params.push(pattern.clone().into());
                    query_params.push(pattern.clone().into());
                    query_params.push(pattern.into());
                } else {
                    conditions.push(format!("{} LIKE ?", field));
                    query_params.push(pattern.into());
                }
            }
            "match_phrase" => {
                let pattern = format!("%{}%", input);
                if include_desc_tags {
                    conditions.push("(name LIKE ? OR description LIKE ? OR tags LIKE ?)".to_string());
                    query_params.push(pattern.clone().into());
                    query_params.push(pattern.clone().into());
                    query_params.push(pattern.into());
                } else {
                    conditions.push(format!("{} LIKE ?", field));
                    query_params.push(pattern.into());
                }
            }
            _ /* "match_all_words" */ => {
                let words: Vec<&str> = input.split_whitespace().collect();
                if words.is_empty() {
                    let pattern = format!("%{}%", input);
                    if include_desc_tags {
                        conditions.push("(name LIKE ? OR description LIKE ? OR tags LIKE ?)".to_string());
                        query_params.push(pattern.clone().into());
                        query_params.push(pattern.clone().into());
                        query_params.push(pattern.into());
                    } else {
                        conditions.push(format!("{} LIKE ?", field));
                        query_params.push(pattern.into());
                    }
                } else {
                    for word in words {
                        let pattern = format!("%{}%", word);
                        if include_desc_tags {
                            conditions.push("(name LIKE ? OR description LIKE ? OR tags LIKE ?)".to_string());
                            query_params.push(pattern.clone().into());
                            query_params.push(pattern.clone().into());
                            query_params.push(pattern.into());
                        } else {
                            conditions.push(format!("{} LIKE ?", field));
                            query_params.push(pattern.into());
                        }
                    }
                }
            }
        }
    }
}
