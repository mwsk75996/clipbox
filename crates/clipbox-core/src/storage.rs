// ----------
// Core Clipboard Storage
// Description: SQLite-backed storage managing clipboard entries with source app metadata, process name, window title, and application icon.
// ----------

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, Connection, Result, TransactionBehavior};

/// Schema version stored in SQLite's `PRAGMA user_version` header.
/// Once migrated, subsequent opens skip DDL and PRAGMA table_info scans.
const SCHEMA_VERSION: i64 = 2;

/// Metadata captured along with a clipboard entry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClipboardMetadata {
    pub source_app: Option<String>,
    pub source_process: Option<String>,
    pub window_title: Option<String>,
    pub app_icon: Option<String>,
    pub source_url: Option<String>,
}

/// A stored clipboard item returned to application frontends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardEntry {
    pub id: i64,
    pub content: String,
    pub copied_at: i64,
    pub source_app: Option<String>,
    pub source_process: Option<String>,
    pub window_title: Option<String>,
    pub app_icon: Option<String>,
    pub is_pinned: bool,
    pub entry_type: String,
    pub image_data: Option<String>,
    pub image_dimensions: Option<String>,
    pub files_data: Option<String>,
    pub source_url: Option<String>,
    pub ocr_text: Option<String>,
    pub ocr_boxes: Option<String>,
}

/// An archived clipboard item awaiting restore, permanent deletion, or purge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletedEntry {
    pub id: i64,
    pub original_id: i64,
    pub content: String,
    pub copied_at: i64,
    pub source_app: Option<String>,
    pub source_process: Option<String>,
    pub window_title: Option<String>,
    pub app_icon: Option<String>,
    pub is_pinned: bool,
    pub entry_type: String,
    pub image_data: Option<String>,
    pub image_dimensions: Option<String>,
    pub files_data: Option<String>,
    pub source_url: Option<String>,
    pub deleted_at: i64,
}

/// Query parameters for paginated, filtered history lookups.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EntryQuery {
    pub search: Option<String>,
    pub source_app: Option<String>,
    pub entry_type: Option<String>,
    pub from_timestamp: Option<i64>,
    pub to_timestamp: Option<i64>,
    pub limit: u32,
    pub offset: u32,
}

/// A single page of summary entries along with the total matching count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryPage {
    pub entries: Vec<ClipboardEntry>,
    pub total_count: usize,
}

/// Escape `%`, `_`, and `\` for a SQLite `LIKE ... ESCAPE '\'` substring match.
fn escape_like_pattern(needle: &str) -> String {
    let mut escaped = String::with_capacity(needle.len() + 2);
    escaped.push('%');
    for ch in needle.chars() {
        match ch {
            '\\' | '%' | '_' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            other => escaped.push(other),
        }
    }
    escaped.push('%');
    escaped
}

/// Current unix timestamp in seconds, used for copied_at and deleted_at.
fn current_unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

// ----------
// Deleted Retention Timespan
// Description: Maps the deleted_retention setting value to a lifetime in seconds.
// "immediately" is handled by the caller (records skip the archive entirely);
// unknown values yield None so nothing is ever purged unexpectedly.
// ----------
pub fn deleted_retention_lifetime_seconds(setting: &str) -> Option<u64> {
    match setting {
        "1hour" => Some(3_600),
        "1day" => Some(86_400),
        "7days" => Some(7 * 86_400),
        "30days" => Some(30 * 86_400),
        _ => None,
    }
}

/// SQLite-backed storage shared by Clipbox frontends.
pub struct ClipboardStore {
    connection: Connection,
}

impl ClipboardStore {
    /// Open or create the database at `path` and apply the initial schema.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::initialize(Connection::open(path)?)
    }

    fn initialize(connection: Connection) -> Result<Self> {
        // Wait briefly on concurrent writes (monitor, OCR backfill, UI) instead
        // of failing immediately with SQLITE_BUSY.
        connection.busy_timeout(Duration::from_secs(5))?;
        let _ = connection.query_row("PRAGMA journal_mode = WAL", [], |row| {
            row.get::<_, String>(0)
        });
        let _ = connection.execute_batch("PRAGMA synchronous = NORMAL;");

        let user_version: i64 =
            connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;

        if user_version < SCHEMA_VERSION {
            connection.execute_batch(
                "CREATE TABLE IF NOT EXISTS clipboard_entries (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    content TEXT NOT NULL,
                    copied_at INTEGER NOT NULL,
                    source_app TEXT,
                    source_process TEXT,
                    window_title TEXT,
                    app_icon TEXT,
                    is_pinned INTEGER NOT NULL DEFAULT 0,
                    entry_type TEXT NOT NULL DEFAULT 'text',
                    image_data TEXT,
                    image_dimensions TEXT,
                    files_data TEXT,
                    source_url TEXT,
                    ocr_text TEXT,
                    ocr_boxes TEXT
                );
                CREATE TABLE IF NOT EXISTS app_settings (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS entry_id_seq (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    last_id INTEGER NOT NULL DEFAULT 0
                );
                CREATE TABLE IF NOT EXISTS deleted_entries (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    original_id INTEGER NOT NULL,
                    content TEXT NOT NULL,
                    copied_at INTEGER NOT NULL,
                    source_app TEXT,
                    source_process TEXT,
                    window_title TEXT,
                    app_icon TEXT,
                    is_pinned INTEGER NOT NULL DEFAULT 0,
                    entry_type TEXT NOT NULL DEFAULT 'text',
                    image_data TEXT,
                    image_dimensions TEXT,
                    files_data TEXT,
                    source_url TEXT,
                    deleted_at INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_deleted_entries_deleted_at
                    ON deleted_entries(deleted_at);",
            )?;

            // Keep databases created by earlier versions usable with a single
            // PRAGMA table_info pass instead of querying per column.
            let existing_columns = Self::table_columns(&connection)?;
            for (column, definition) in [
                ("source_app", "source_app TEXT"),
                ("source_process", "source_process TEXT"),
                ("window_title", "window_title TEXT"),
                ("app_icon", "app_icon TEXT"),
                ("is_pinned", "is_pinned INTEGER NOT NULL DEFAULT 0"),
                ("entry_type", "entry_type TEXT NOT NULL DEFAULT 'text'"),
                ("image_data", "image_data TEXT"),
                ("image_dimensions", "image_dimensions TEXT"),
                ("files_data", "files_data TEXT"),
                ("source_url", "source_url TEXT"),
                ("ocr_text", "ocr_text TEXT"),
                ("ocr_boxes", "ocr_boxes TEXT"),
            ] {
                if !existing_columns.contains(column) {
                    connection.execute(
                        &format!("ALTER TABLE clipboard_entries ADD COLUMN {definition}"),
                        [],
                    )?;
                }
            }

            // Composite feed-order index avoids full-table scans and temp B-tree
            // sorts over inline image blobs on every open/focus/tray refresh.
            // Also seed the monotonic entry ID sequence from existing rows so
            // migrated tables never reuse deleted or cleared IDs.
            connection.execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_clipboard_entries_order
                    ON clipboard_entries(is_pinned DESC, copied_at DESC, id DESC);
                CREATE INDEX IF NOT EXISTS idx_clipboard_entries_type_id
                    ON clipboard_entries(entry_type, id DESC);
                INSERT INTO entry_id_seq (id, last_id)
                    VALUES (1, (SELECT COALESCE(MAX(id), 0) FROM clipboard_entries))
                    ON CONFLICT(id) DO UPDATE SET last_id = MAX(last_id, excluded.last_id);",
            )?;

            connection.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))?;
        }

        Ok(Self { connection })
    }

    fn table_columns(connection: &Connection) -> Result<HashSet<String>> {
        let mut statement = connection.prepare("PRAGMA table_info(clipboard_entries)")?;
        let mut rows = statement.query([])?;
        let mut columns = HashSet::new();

        while let Some(row) = rows.next()? {
            columns.insert(row.get::<_, String>(1)?);
        }

        Ok(columns)
    }

    #[cfg(test)]
    fn in_memory() -> Result<Self> {
        Self::initialize(Connection::open_in_memory()?)
    }

    // ----------
    // Immutable Monotonic Entry ID Allocation
    // Description: Tracks a persistent high-water mark in `entry_id_seq` so
    // clipboard entry IDs are never shifted or reused after deletes or Clear
    // History, even on legacy tables created without AUTOINCREMENT.
    // ----------
    fn begin_immediate(&self) -> Result<rusqlite::Transaction<'_>> {
        rusqlite::Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
    }

    fn record_entry_id_high_water(connection: &Connection) -> Result<()> {
        connection.execute(
            "INSERT INTO entry_id_seq (id, last_id)
             VALUES (1, (SELECT COALESCE(MAX(id), 0) FROM clipboard_entries))
             ON CONFLICT(id) DO UPDATE SET last_id = MAX(last_id, excluded.last_id)",
            [],
        )?;
        Ok(())
    }

    fn allocate_next_entry_id(connection: &Connection) -> Result<i64> {
        let current_max: i64 = connection.query_row(
            "SELECT MAX(
                 COALESCE((SELECT last_id FROM entry_id_seq WHERE id = 1), 0),
                 COALESCE((SELECT MAX(id) FROM clipboard_entries), 0)
             )",
            [],
            |row| row.get(0),
        )?;
        let next_id = current_max + 1;
        connection.execute(
            "INSERT INTO entry_id_seq (id, last_id) VALUES (1, ?1)
             ON CONFLICT(id) DO UPDATE SET last_id = MAX(last_id, excluded.last_id)",
            params![next_id],
        )?;
        Ok(next_id)
    }

    /// Add a text clipboard entry and return its database id.
    pub fn add_text(&self, content: &str) -> Result<i64> {
        self.add_entry(content, &ClipboardMetadata::default())
    }

    /// Add a text clipboard entry with its source metadata.
    pub fn add_entry(&self, content: &str, metadata: &ClipboardMetadata) -> Result<i64> {
        let cleaned = strip_leading_empty_lines(content);
        let content_to_store = if cleaned.is_empty() && !content.is_empty() {
            content
        } else {
            cleaned
        };

        let copied_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let tx = self.begin_immediate()?;
        let last_id = Self::allocate_next_entry_id(&tx)?;
        tx.execute(
            "INSERT INTO clipboard_entries
                (id, content, copied_at, source_app, source_process, window_title, app_icon, source_url)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                last_id,
                content_to_store,
                copied_at,
                metadata.source_app.as_deref(),
                metadata.source_process.as_deref(),
                metadata.window_title.as_deref(),
                metadata.app_icon.as_deref(),
                metadata.source_url.as_deref(),
            ],
        )?;
        tx.commit()?;

        // Enforce retention limit if configured
        if let Ok(Some(limit_str)) = self.get_setting("retention_limit") {
            if let Ok(limit) = limit_str.parse::<usize>() {
                if limit > 0 {
                    let _ = self.prune_entries(limit);
                }
            }
        }

        Ok(last_id)
    }

    // ----------
    // Add Image Clipboard Entry
    // Description: Stores a captured image clipboard item with thumbnail data URL, dimensions, and source metadata.
    // ----------
    pub fn add_image_entry(
        &self,
        image_data: &str,
        dimensions: &str,
        metadata: &ClipboardMetadata,
    ) -> Result<i64> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let content_label = format!("Image ({dimensions})");

        let tx = self.begin_immediate()?;
        let last_id = Self::allocate_next_entry_id(&tx)?;
        tx.execute(
            "INSERT INTO clipboard_entries (
                id, content, copied_at, source_app, source_process, window_title, app_icon, is_pinned, entry_type, image_data, image_dimensions, source_url
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, 'image', ?8, ?9, ?10)",
            params![
                last_id,
                content_label,
                now,
                metadata.source_app.as_deref(),
                metadata.source_process.as_deref(),
                metadata.window_title.as_deref(),
                metadata.app_icon.as_deref(),
                image_data,
                dimensions,
                metadata.source_url.as_deref(),
            ],
        )?;
        tx.commit()?;

        // Enforce retention limit if configured
        if let Ok(Some(limit_str)) = self.get_setting("retention_limit") {
            if let Ok(limit) = limit_str.parse::<usize>() {
                if limit > 0 {
                    let _ = self.prune_entries(limit);
                }
            }
        }

        Ok(last_id)
    }

    // ----------
    // File Clipboard Storage Entry
    // Description: Persists copied file descriptors, summary text, and structured JSON metadata (names, paths, sizes, extensions) into SQLite.
    // ----------
    pub fn add_file_entry(
        &self,
        display_summary: &str,
        files_json: &str,
        metadata: &ClipboardMetadata,
    ) -> Result<i64> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let tx = self.begin_immediate()?;
        let last_id = Self::allocate_next_entry_id(&tx)?;
        tx.execute(
            "INSERT INTO clipboard_entries (
                id, content, copied_at, source_app, source_process, window_title, app_icon, is_pinned, entry_type, files_data, source_url
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, 'file', ?8, ?9)",
            params![
                last_id,
                display_summary,
                now,
                metadata.source_app.as_deref(),
                metadata.source_process.as_deref(),
                metadata.window_title.as_deref(),
                metadata.app_icon.as_deref(),
                files_json,
                metadata.source_url.as_deref(),
            ],
        )?;
        tx.commit()?;

        // Enforce retention limit if configured
        if let Ok(Some(limit_str)) = self.get_setting("retention_limit") {
            if let Ok(limit) = limit_str.parse::<usize>() {
                if limit > 0 {
                    let _ = self.prune_entries(limit);
                }
            }
        }

        Ok(last_id)
    }

    // ----------
    // Retention Limit Pruning
    // Description: Deletes surplus records exceeding the configured history retention limit, keeping only the newest unpinned entries. Pinned entries are preserved.
    // ----------
    /// Retain only the most recent `limit` unpinned records, deleting older ones.
    /// Pinned entries are exempt from retention pruning.
    /// If `limit == 0`, no records are deleted (unlimited retention).
    pub fn prune_entries(&self, limit: usize) -> Result<usize> {
        if limit == 0 {
            return Ok(0);
        }

        let tx = self.begin_immediate()?;
        Self::record_entry_id_high_water(&tx)?;
        let deleted = tx.execute(
            "DELETE FROM clipboard_entries
             WHERE is_pinned = 0 AND id NOT IN (
                 SELECT id FROM clipboard_entries WHERE is_pinned = 0 ORDER BY id DESC LIMIT ?1
             )",
            params![limit as i64],
        )?;
        tx.commit()?;

        Ok(deleted)
    }

    /// Return the newest stored entries first, prioritizing pinned entries.
    pub fn recent_entries(&self, limit: u32) -> Result<Vec<ClipboardEntry>> {
        let mut statement = self.connection.prepare(
            "SELECT id, content, copied_at, source_app, source_process, window_title, app_icon, is_pinned, entry_type, image_data, image_dimensions, files_data, source_url, ocr_text, ocr_boxes
             FROM clipboard_entries
             ORDER BY is_pinned DESC, copied_at DESC, id DESC
             LIMIT ?1",
        )?;
        let entries = statement.query_map(params![i64::from(limit)], Self::map_clipboard_entry)?;

        entries.collect()
    }

    // ----------
    // Lightweight Feed, Tray, and Image Queries
    // Description: Avoids loading full-resolution base64 image_data blobs during
    // feed listing, tray menu updates, and row counting. Images are fetched on
    // demand by primary key when rendered, copied, or saved.
    // ----------

    /// Return the newest entries without inline `image_data` blobs for fast IPC.
    pub fn recent_entry_summaries(&self, limit: u32) -> Result<Vec<ClipboardEntry>> {
        let mut statement = self.connection.prepare(
            "SELECT id, content, copied_at, source_app, source_process, window_title, app_icon, is_pinned, entry_type, NULL, image_dimensions, files_data, source_url, ocr_text, ocr_boxes
             FROM clipboard_entries
             ORDER BY is_pinned DESC, copied_at DESC, id DESC
             LIMIT ?1",
        )?;
        let entries = statement.query_map(params![i64::from(limit)], Self::map_clipboard_entry)?;

        entries.collect()
    }

    /// Return only `(id, content)` for the newest entries, used by the tray menu
    /// without touching overflow image pages.
    pub fn recent_tray_entries(&self, limit: u32) -> Result<Vec<(i64, String)>> {
        let mut statement = self.connection.prepare(
            "SELECT id, content
             FROM clipboard_entries
             ORDER BY is_pinned DESC, copied_at DESC, id DESC
             LIMIT ?1",
        )?;
        let rows = statement.query_map(params![i64::from(limit)], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;

        rows.collect()
    }

    /// Return only the `image_data` payload for a single entry by ID.
    pub fn get_entry_image(&self, id: i64) -> Result<Option<String>> {
        let mut statement = self
            .connection
            .prepare("SELECT image_data FROM clipboard_entries WHERE id = ?1")?;
        let mut rows = statement.query(params![id])?;
        if let Some(row) = rows.next()? {
            row.get(0)
        } else {
            Ok(None)
        }
    }

    /// Count active entries without materializing row payloads.
    pub fn entry_count(&self) -> Result<usize> {
        let count: i64 =
            self.connection
                .query_row("SELECT COUNT(*) FROM clipboard_entries", [], |row| {
                    row.get(0)
                })?;
        Ok(count.max(0) as usize)
    }

    /// Count archived entries without materializing row payloads.
    pub fn deleted_entry_count(&self) -> Result<usize> {
        let count: i64 =
            self.connection
                .query_row("SELECT COUNT(*) FROM deleted_entries", [], |row| {
                    row.get(0)
                })?;
        Ok(count.max(0) as usize)
    }

    // ----------
    // Full-History Paginated & Filtered Queries
    // Description: Queries summary rows (without inline image_data blobs) across
    // the entire database using SQLite LIMIT/OFFSET and parameterized filters so
    // users can browse and search thousands of entries with millisecond latency.
    // ----------

    /// Return a single page of entry summaries matching `query`, plus the exact
    /// total number of matching rows across the entire history.
    pub fn query_entry_summaries(&self, query: &EntryQuery) -> Result<EntryPage> {
        let limit = query.limit.max(1);
        let offset = query.offset;

        let search_trimmed = query
            .search
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let source_app_filter = query
            .source_app
            .as_deref()
            .filter(|s| !s.is_empty() && *s != "all");
        let entry_type_filter = query
            .entry_type
            .as_deref()
            .filter(|s| !s.is_empty() && *s != "all");

        let has_filters = search_trimmed.is_some()
            || source_app_filter.is_some()
            || entry_type_filter.is_some()
            || query.from_timestamp.is_some()
            || query.to_timestamp.is_some();

        // Fast unfiltered path: uses idx_clipboard_entries_order directly with
        // LIMIT/OFFSET and avoids touching rows outside the requested page.
        if !has_filters {
            let total_count = self.entry_count()?;
            let mut statement = self.connection.prepare(
                "SELECT id, content, copied_at, source_app, source_process, window_title, app_icon, is_pinned, entry_type, NULL, image_dimensions, files_data, source_url, ocr_text, ocr_boxes
                 FROM clipboard_entries
                 ORDER BY is_pinned DESC, copied_at DESC, id DESC
                 LIMIT ?1 OFFSET ?2",
            )?;
            let entries = statement
                .query_map(
                    params![i64::from(limit), i64::from(offset)],
                    Self::map_clipboard_entry,
                )?
                .collect::<Result<Vec<_>>>()?;
            return Ok(EntryPage {
                entries,
                total_count,
            });
        }

        let mut conditions: Vec<&str> = Vec::new();
        let mut values: Vec<SqlValue> = Vec::new();

        if let Some(entry_type) = entry_type_filter {
            match entry_type {
                "image" => conditions.push("entry_type = 'image'"),
                "file" => conditions.push("entry_type = 'file'"),
                _ => conditions.push("entry_type NOT IN ('image', 'file')"),
            }
        }

        if let Some(app_label) = source_app_filter {
            conditions.push(
                "COALESCE(NULLIF(source_app, ''), NULLIF(source_process, ''), NULLIF(window_title, ''), 'Unknown source') = ?",
            );
            values.push(SqlValue::Text(app_label.to_string()));
        }

        if let Some(from_ts) = query.from_timestamp {
            conditions.push("copied_at >= ?");
            values.push(SqlValue::Integer(from_ts));
        }

        if let Some(to_ts) = query.to_timestamp {
            conditions.push("copied_at <= ?");
            values.push(SqlValue::Integer(to_ts));
        }

        if let Some(needle) = search_trimmed {
            let pattern = escape_like_pattern(&needle.to_lowercase());
            conditions.push(
                "(LOWER(content) LIKE ? ESCAPE '\\' \
                  OR LOWER(COALESCE(source_app, '')) LIKE ? ESCAPE '\\' \
                  OR LOWER(COALESCE(source_process, '')) LIKE ? ESCAPE '\\' \
                  OR LOWER(COALESCE(window_title, '')) LIKE ? ESCAPE '\\' \
                  OR LOWER(COALESCE(source_url, '')) LIKE ? ESCAPE '\\' \
                  OR LOWER(COALESCE(ocr_text, '')) LIKE ? ESCAPE '\\')",
            );
            for _ in 0..6 {
                values.push(SqlValue::Text(pattern.clone()));
            }
        }

        // Single pass collects ordered matching IDs for both exact total count
        // and the page slice, avoiding scanning the table twice.
        let id_sql = format!(
            "SELECT id FROM clipboard_entries WHERE {} ORDER BY is_pinned DESC, copied_at DESC, id DESC",
            conditions.join(" AND ")
        );
        let mut id_stmt = self.connection.prepare(&id_sql)?;
        let matching_ids: Vec<i64> = id_stmt
            .query_map(params_from_iter(values.iter()), |row| row.get(0))?
            .collect::<Result<Vec<_>>>()?;

        let total_count = matching_ids.len();
        let page_ids: Vec<i64> = matching_ids
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect();

        if page_ids.is_empty() {
            return Ok(EntryPage {
                entries: Vec::new(),
                total_count,
            });
        }

        let placeholders = vec!["?"; page_ids.len()].join(", ");
        let fetch_sql = format!(
            "SELECT id, content, copied_at, source_app, source_process, window_title, app_icon, is_pinned, entry_type, NULL, image_dimensions, files_data, source_url, ocr_text, ocr_boxes
             FROM clipboard_entries
             WHERE id IN ({placeholders})"
        );
        let mut fetch_stmt = self.connection.prepare(&fetch_sql)?;
        let mut by_id: HashMap<i64, ClipboardEntry> = fetch_stmt
            .query_map(params_from_iter(page_ids.iter()), Self::map_clipboard_entry)?
            .map(|res| res.map(|entry| (entry.id, entry)))
            .collect::<Result<HashMap<_, _>>>()?;

        let entries = page_ids
            .into_iter()
            .filter_map(|id| by_id.remove(&id))
            .collect();

        Ok(EntryPage {
            entries,
            total_count,
        })
    }

    /// Return all distinct source application labels across the full history,
    /// sorted alphabetically, matching the frontend's `sourceLabel` fallback chain.
    pub fn list_source_apps(&self) -> Result<Vec<String>> {
        let mut statement = self.connection.prepare(
            "SELECT DISTINCT COALESCE(NULLIF(source_app, ''), NULLIF(source_process, ''), NULLIF(window_title, ''), 'Unknown source') AS app_label
             FROM clipboard_entries
             ORDER BY app_label ASC",
        )?;
        let apps = statement.query_map([], |row| row.get(0))?;
        apps.collect()
    }

    /// Return a single stored entry by its ID.
    pub fn get_entry(&self, id: i64) -> Result<Option<ClipboardEntry>> {
        let mut statement = self.connection.prepare(
            "SELECT id, content, copied_at, source_app, source_process, window_title, app_icon, is_pinned, entry_type, image_data, image_dimensions, files_data, source_url, ocr_text, ocr_boxes
             FROM clipboard_entries
             WHERE id = ?1",
        )?;
        let mut rows = statement.query_map(params![id], |row| {
            let is_pinned_int: i32 = row.get(7)?;
            Ok(ClipboardEntry {
                id: row.get(0)?,
                content: row.get(1)?,
                copied_at: row.get(2)?,
                source_app: row.get(3)?,
                source_process: row.get(4)?,
                window_title: row.get(5)?,
                app_icon: row.get(6)?,
                is_pinned: is_pinned_int != 0,
                entry_type: row.get(8).unwrap_or_else(|_| "text".into()),
                image_data: row.get(9)?,
                image_dimensions: row.get(10)?,
                files_data: row.get(11)?,
                source_url: row.get(12)?,
                ocr_text: row.get(13)?,
                ocr_boxes: row.get(14)?,
            })
        })?;

        if let Some(entry) = rows.next() {
            Ok(Some(entry?))
        } else {
            Ok(None)
        }
    }

    // ----------
    // Duplicate Entry Detection & Timestamp Bump
    // Description: Queries existing entries matching text, image, or files payload and bumps their copied_at timestamp to bring them to the top of the history feed without duplicate clutter.
    // ----------

    pub fn find_existing_text(&self, content: &str) -> Result<Option<i64>> {
        let cleaned = strip_leading_empty_lines(content);
        let mut statement = self.connection.prepare(
            "SELECT id FROM clipboard_entries WHERE entry_type = 'text' AND content = ?1 ORDER BY id DESC LIMIT 1",
        )?;
        let mut rows = statement.query([cleaned])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn find_existing_image(&self, data_url: &str) -> Result<Option<i64>> {
        let mut statement = self.connection.prepare(
            "SELECT id FROM clipboard_entries WHERE entry_type = 'image' AND image_data = ?1 ORDER BY id DESC LIMIT 1",
        )?;
        let mut rows = statement.query([data_url])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn find_existing_file(&self, files_json: &str) -> Result<Option<i64>> {
        let mut statement = self.connection.prepare(
            "SELECT id FROM clipboard_entries WHERE entry_type = 'file' AND files_data = ?1 ORDER BY id DESC LIMIT 1",
        )?;
        let mut rows = statement.query([files_json])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn bump_entry(&self, id: i64, metadata: &ClipboardMetadata) -> Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        self.connection.execute(
            "UPDATE clipboard_entries
             SET copied_at = MAX(?1, (SELECT COALESCE(MAX(copied_at), 0) FROM clipboard_entries) + 1),
                 source_app = COALESCE(?2, source_app),
                 source_process = COALESCE(?3, source_process),
                 window_title = COALESCE(?4, window_title),
                 app_icon = COALESCE(?5, app_icon),
                 source_url = COALESCE(?6, source_url)
             WHERE id = ?7",
            params![
                now,
                metadata.source_app.as_deref(),
                metadata.source_process.as_deref(),
                metadata.window_title.as_deref(),
                metadata.app_icon.as_deref(),
                metadata.source_url.as_deref(),
                id,
            ],
        )?;

        Ok(())
    }

    // ----------
    // Pin Clipboard Entry
    // Description: Toggles or sets the is_pinned state of a clipboard entry by ID, anchoring it to the top of the feed.
    // ----------
    /// Set the pinned status of an entry by ID.
    pub fn set_pinned(&self, id: i64, pinned: bool) -> Result<bool> {
        let updated = self.connection.execute(
            "UPDATE clipboard_entries SET is_pinned = ?1 WHERE id = ?2",
            params![pinned as i32, id],
        )?;
        Ok(updated > 0)
    }

    /// Toggle the pinned status of an entry by ID and return the new status.
    pub fn toggle_pinned(&self, id: i64) -> Result<bool> {
        let mut statement = self.connection.prepare(
            "SELECT is_pinned FROM clipboard_entries WHERE id = ?1",
        )?;
        let mut rows = statement.query(params![id])?;
        if let Some(row) = rows.next()? {
            let is_pinned_int: i32 = row.get(0)?;
            let new_pinned = is_pinned_int == 0;
            self.set_pinned(id, new_pinned)?;
            Ok(new_pinned)
        } else {
            Ok(false)
        }
    }

    // ----------
    // Atomic Clear History Storage
    // Description: Atomically moves all stored clipboard entries into the
    // Recently Deleted archive inside a BEGIN IMMEDIATE transaction so
    // concurrent captures cannot be lost between INSERT and DELETE.
    // ----------
    /// Move all stored clipboard records into the archive and return the archived count.
    pub fn clear_entries(&self) -> Result<usize> {
        let tx = self.begin_immediate()?;
        Self::record_entry_id_high_water(&tx)?;
        let archived = tx.execute(
            "INSERT INTO deleted_entries
                (original_id, content, copied_at, source_app, source_process, window_title, app_icon, is_pinned, entry_type, image_data, image_dimensions, files_data, source_url, deleted_at)
             SELECT id, content, copied_at, source_app, source_process, window_title, app_icon, is_pinned, entry_type, image_data, image_dimensions, files_data, source_url, ?1
             FROM clipboard_entries",
            params![current_unix_timestamp()],
        )?;

        tx.execute("DELETE FROM clipboard_entries", [])?;
        tx.commit()?;

        Ok(archived)
    }

    // ----------
    // Delete Single Entry (Immutable IDs)
    // Description: Deletes an individual clipboard history entry by primary key
    // without resequencing other rows, keeping all remaining IDs stable for
    // background OCR, tray quick-paste slots, and lazy image lookups.
    // ----------
    /// Delete a single clipboard record by id without mutating other entry IDs.
    pub fn delete_entry(&self, id: i64) -> Result<bool> {
        let tx = self.begin_immediate()?;
        Self::record_entry_id_high_water(&tx)?;
        let deleted = tx.execute("DELETE FROM clipboard_entries WHERE id = ?1", params![id])?;
        tx.commit()?;

        Ok(deleted > 0)
    }

    // ----------
    // Recently Deleted Archive (Atomic Moves)
    // Description: Soft-delete safety net moving records atomically between
    // clipboard_entries and deleted_entries inside BEGIN IMMEDIATE transactions
    // using INSERT ... SELECT without round-tripping image blobs through Rust.
    // ----------
    fn map_deleted_entry(row: &rusqlite::Row) -> rusqlite::Result<DeletedEntry> {
        let is_pinned_int: i32 = row.get(8)?;
        Ok(DeletedEntry {
            id: row.get(0)?,
            original_id: row.get(1)?,
            content: row.get(2)?,
            copied_at: row.get(3)?,
            source_app: row.get(4)?,
            source_process: row.get(5)?,
            window_title: row.get(6)?,
            app_icon: row.get(7)?,
            is_pinned: is_pinned_int != 0,
            entry_type: row.get(9).unwrap_or_else(|_| "text".into()),
            image_data: row.get(10)?,
            image_dimensions: row.get(11)?,
            files_data: row.get(12)?,
            source_url: row.get(13)?,
            deleted_at: row.get(14)?,
        })
    }

    /// Atomically archive a single active record without mutating other entry IDs.
    /// Returns false when no active record has `id`.
    pub fn soft_delete_entry(&self, id: i64) -> Result<bool> {
        let tx = self.begin_immediate()?;
        Self::record_entry_id_high_water(&tx)?;
        let archived = tx.execute(
            "INSERT INTO deleted_entries
                (original_id, content, copied_at, source_app, source_process, window_title, app_icon, is_pinned, entry_type, image_data, image_dimensions, files_data, source_url, deleted_at)
             SELECT id, content, copied_at, source_app, source_process, window_title, app_icon, is_pinned, entry_type, image_data, image_dimensions, files_data, source_url, ?2
             FROM clipboard_entries
             WHERE id = ?1",
            params![id, current_unix_timestamp()],
        )?;

        if archived == 0 {
            tx.commit()?;
            return Ok(false);
        }

        tx.execute("DELETE FROM clipboard_entries WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(true)
    }

    /// Return the newest archived records first.
    pub fn recent_deleted_entries(&self, limit: u32) -> Result<Vec<DeletedEntry>> {
        let mut statement = self.connection.prepare(
            "SELECT id, original_id, content, copied_at, source_app, source_process, window_title, app_icon, is_pinned, entry_type, image_data, image_dimensions, files_data, source_url, deleted_at
             FROM deleted_entries
             ORDER BY deleted_at DESC, id DESC
             LIMIT ?1",
        )?;
        let entries = statement.query_map(params![i64::from(limit)], Self::map_deleted_entry)?;

        entries.collect()
    }

    /// Return the newest archived records without inline `image_data` blobs.
    pub fn recent_deleted_entry_summaries(&self, limit: u32) -> Result<Vec<DeletedEntry>> {
        let mut statement = self.connection.prepare(
            "SELECT id, original_id, content, copied_at, source_app, source_process, window_title, app_icon, is_pinned, entry_type, NULL, image_dimensions, files_data, source_url, deleted_at
             FROM deleted_entries
             ORDER BY deleted_at DESC, id DESC
             LIMIT ?1",
        )?;
        let entries = statement.query_map(params![i64::from(limit)], Self::map_deleted_entry)?;

        entries.collect()
    }

    /// Atomically restore an archived record into the active feed with a fresh
    /// never-reused ID and its original content and timestamp. Returns the new
    /// active id, or None when the archive id is missing.
    pub fn restore_deleted_entry(&self, id: i64) -> Result<Option<i64>> {
        let tx = self.begin_immediate()?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM deleted_entries WHERE id = ?1)",
            params![id],
            |row| row.get(0),
        )?;
        if !exists {
            tx.commit()?;
            return Ok(None);
        }

        let new_id = Self::allocate_next_entry_id(&tx)?;
        tx.execute(
            "INSERT INTO clipboard_entries
                (id, content, copied_at, source_app, source_process, window_title, app_icon, is_pinned, entry_type, image_data, image_dimensions, files_data, source_url)
             SELECT ?2, content, copied_at, source_app, source_process, window_title, app_icon, is_pinned, entry_type, image_data, image_dimensions, files_data, source_url
             FROM deleted_entries
             WHERE id = ?1",
            params![id, new_id],
        )?;
        tx.execute("DELETE FROM deleted_entries WHERE id = ?1", params![id])?;
        tx.commit()?;

        Ok(Some(new_id))
    }

    /// Permanently delete a single archived record. Returns false when missing.
    pub fn hard_delete_entry(&self, id: i64) -> Result<bool> {
        let deleted = self
            .connection
            .execute("DELETE FROM deleted_entries WHERE id = ?1", params![id])?;

        Ok(deleted > 0)
    }

    /// Permanently delete archived records deleted before `cutoff_unix`.
    pub fn purge_deleted_entries_older_than(&self, cutoff_unix: i64) -> Result<usize> {
        let purged = self.connection.execute(
            "DELETE FROM deleted_entries WHERE deleted_at < ?1",
            params![cutoff_unix],
        )?;

        Ok(purged)
    }

    // ----------
    // Image OCR Text
    // Description: Stores text recognized in image entries (empty string when
    // nothing was found, so NULL keeps meaning "never scanned") and lists
    // unscanned images for background backfills.
    // ----------
    /// Store recognized text for an entry. Empty string marks a completed
    /// scan that found nothing.
    pub fn set_ocr_text(&self, id: i64, ocr_text: &str) -> Result<()> {
        self.connection.execute(
            "UPDATE clipboard_entries SET ocr_text = ?1 WHERE id = ?2",
            params![ocr_text, id],
        )?;
        Ok(())
    }

    /// Store word bounding boxes (JSON array) for an entry.
    pub fn set_ocr_boxes(&self, id: i64, ocr_boxes: &str) -> Result<()> {
        self.connection.execute(
            "UPDATE clipboard_entries SET ocr_boxes = ?1 WHERE id = ?2",
            params![ocr_boxes, id],
        )?;
        Ok(())
    }

    /// Ids of image entries missing OCR text or word boxes, oldest first.
    /// Rows scanned before boxes existed (text present, boxes missing) heal here.
    pub fn images_missing_ocr_data(&self, limit: u32) -> Result<Vec<i64>> {
        let mut statement = self.connection.prepare(
            "SELECT id FROM clipboard_entries
             WHERE entry_type = 'image' AND (ocr_text IS NULL OR ocr_boxes IS NULL)
             ORDER BY id ASC
             LIMIT ?1",
        )?;
        let rows = statement.query_map(params![i64::from(limit)], |row| row.get(0))?;

        rows.collect()
    }

    // ----------
    // App Settings Storage
    // Description: Key-value persistence for user preferences such as start minimized, always on top, and retention policy.
    // ----------
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let mut statement = self.connection.prepare(
            "SELECT value FROM app_settings WHERE key = ?1",
        )?;
        let mut rows = statement.query([key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.connection.execute(
            "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    // ----------
    // Backup Export & Import
    // Description: Full-library dump helpers plus a transactional merge import. Identical records are skipped (pin upgraded when the backup copy is pinned), timestamps and OCR data are preserved, settings are restored verbatim, and retention pruning is deliberately not applied to imported rows.
    // ----------

    /// Every stored entry, oldest first, for backup export.
    pub fn all_entries(&self) -> Result<Vec<ClipboardEntry>> {
        let mut statement = self.connection.prepare(
            "SELECT id, content, copied_at, source_app, source_process, window_title, app_icon, is_pinned, entry_type, image_data, image_dimensions, files_data, source_url, ocr_text, ocr_boxes
             FROM clipboard_entries
             ORDER BY id ASC",
        )?;
        let entries = statement.query_map([], Self::map_clipboard_entry)?;

        entries.collect()
    }

    /// Every archived record, oldest first, for backup export.
    pub fn all_deleted_entries(&self) -> Result<Vec<DeletedEntry>> {
        let mut statement = self.connection.prepare(
            "SELECT id, original_id, content, copied_at, source_app, source_process, window_title, app_icon, is_pinned, entry_type, image_data, image_dimensions, files_data, source_url, deleted_at
             FROM deleted_entries
             ORDER BY id ASC",
        )?;
        let entries = statement.query_map([], Self::map_deleted_entry)?;

        entries.collect()
    }

    /// Every app setting ordered by key, for backup export.
    pub fn all_settings(&self) -> Result<Vec<(String, String)>> {
        let mut statement = self
            .connection
            .prepare("SELECT key, value FROM app_settings ORDER BY key ASC")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;

        rows.collect()
    }

    /// Rough byte size of the live + archive text payloads, shown as the
    /// pre-export size note (images ride inline as base64 in the file).
    pub fn backup_payload_bytes(&self) -> Result<i64> {
        let live: Option<i64> = self.connection.query_row(
            "SELECT SUM(LENGTH(content) + LENGTH(COALESCE(image_data, '')) + LENGTH(COALESCE(files_data, '')) + LENGTH(COALESCE(ocr_text, '')) + LENGTH(COALESCE(ocr_boxes, ''))) FROM clipboard_entries",
            [],
            |row| row.get(0),
        )?;
        let archived: Option<i64> = self.connection.query_row(
            "SELECT SUM(LENGTH(content) + LENGTH(COALESCE(image_data, '')) + LENGTH(COALESCE(files_data, ''))) FROM deleted_entries",
            [],
            |row| row.get(0),
        )?;
        Ok(live.unwrap_or(0) + archived.unwrap_or(0))
    }

    fn map_clipboard_entry(row: &rusqlite::Row<'_>) -> Result<ClipboardEntry> {
        let is_pinned_int: i32 = row.get(7)?;
        Ok(ClipboardEntry {
            id: row.get(0)?,
            content: row.get(1)?,
            copied_at: row.get(2)?,
            source_app: row.get(3)?,
            source_process: row.get(4)?,
            window_title: row.get(5)?,
            app_icon: row.get(6)?,
            is_pinned: is_pinned_int != 0,
            entry_type: row.get(8).unwrap_or_else(|_| "text".into()),
            image_data: row.get(9)?,
            image_dimensions: row.get(10)?,
            files_data: row.get(11)?,
            source_url: row.get(12)?,
            ocr_text: row.get(13)?,
            ocr_boxes: row.get(14)?,
        })
    }

    /// Duplicate lookup against the archive using the same matchers as the
    /// live tables: stripped text content, exact image payload, exact files JSON.
    pub fn find_existing_deleted(&self, entry: &DeletedEntry) -> Result<Option<i64>> {
        Self::find_backup_match(
            &self.connection,
            "deleted_entries",
            &entry.entry_type,
            &entry.content,
            entry.image_data.as_deref(),
            entry.files_data.as_deref(),
        )
    }

    /// Shared backup matcher over either live or archive table. `IS` (not `=`)
    /// keeps NULL payloads comparable so corrupt rows fail closed as new.
    fn find_backup_match(
        connection: &Connection,
        table: &str,
        entry_type: &str,
        content: &str,
        image_data: Option<&str>,
        files_data: Option<&str>,
    ) -> Result<Option<i64>> {
        let (predicate, payload): (&str, &str) = match entry_type {
            "image" => ("entry_type = 'image' AND image_data IS ?1", image_data.unwrap_or("")),
            "file" => ("entry_type = 'file' AND files_data IS ?1", files_data.unwrap_or("")),
            _ => ("entry_type = 'text' AND content = ?1", content),
        };
        // Mirror the live text matcher normalization so re-imports dedupe.
        let normalized;
        let payload = if entry_type != "image" && entry_type != "file" {
            normalized = strip_leading_empty_lines(payload).to_string();
            normalized.as_str()
        } else {
            payload
        };
        let sql = format!("SELECT id FROM {table} WHERE {predicate} ORDER BY id DESC LIMIT 1");
        let mut statement = connection.prepare(&sql)?;
        let mut rows = statement.query([payload])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    /// Merge backup records into this database inside a single transaction.
    /// Returns per-table added/skipped counts. `original_id` values are kept
    /// verbatim as provenance (nothing queries them functionally).
    pub fn import_backup(
        &mut self,
        entries: &[ClipboardEntry],
        deleted: &[DeletedEntry],
        settings: &[(String, String)],
    ) -> Result<ImportCounts> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut counts = ImportCounts::default();

        for entry in entries {
            let existing = Self::find_backup_match(
                &transaction,
                "clipboard_entries",
                &entry.entry_type,
                &entry.content,
                entry.image_data.as_deref(),
                entry.files_data.as_deref(),
            )?;
            match existing {
                Some(id) => {
                    counts.entries_skipped += 1;
                    if entry.is_pinned {
                        transaction.execute(
                            "UPDATE clipboard_entries SET is_pinned = 1 WHERE id = ?1 AND is_pinned = 0",
                            [id],
                        )?;
                    }
                }
                None => {
                    let new_id = Self::allocate_next_entry_id(&transaction)?;
                    transaction.execute(
                        "INSERT INTO clipboard_entries
                            (id, content, copied_at, source_app, source_process, window_title, app_icon, is_pinned, entry_type, image_data, image_dimensions, files_data, source_url, ocr_text, ocr_boxes)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                        params![
                            new_id,
                            entry.content,
                            entry.copied_at,
                            entry.source_app.as_deref(),
                            entry.source_process.as_deref(),
                            entry.window_title.as_deref(),
                            entry.app_icon.as_deref(),
                            i32::from(entry.is_pinned),
                            entry.entry_type,
                            entry.image_data.as_deref(),
                            entry.image_dimensions.as_deref(),
                            entry.files_data.as_deref(),
                            entry.source_url.as_deref(),
                            entry.ocr_text.as_deref(),
                            entry.ocr_boxes.as_deref(),
                        ],
                    )?;
                    counts.entries_added += 1;
                }
            }
        }

        for record in deleted {
            let existing = Self::find_backup_match(
                &transaction,
                "deleted_entries",
                &record.entry_type,
                &record.content,
                record.image_data.as_deref(),
                record.files_data.as_deref(),
            )?;
            match existing {
                Some(_) => counts.deleted_skipped += 1,
                None => {
                    transaction.execute(
                        "INSERT INTO deleted_entries
                            (original_id, content, copied_at, source_app, source_process, window_title, app_icon, is_pinned, entry_type, image_data, image_dimensions, files_data, source_url, deleted_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                        params![
                            record.original_id,
                            record.content,
                            record.copied_at,
                            record.source_app.as_deref(),
                            record.source_process.as_deref(),
                            record.window_title.as_deref(),
                            record.app_icon.as_deref(),
                            i32::from(record.is_pinned),
                            record.entry_type,
                            record.image_data.as_deref(),
                            record.image_dimensions.as_deref(),
                            record.files_data.as_deref(),
                            record.source_url.as_deref(),
                            record.deleted_at,
                        ],
                    )?;
                    counts.deleted_added += 1;
                }
            }
        }

        for (key, value) in settings {
            transaction.execute(
                "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [key, value],
            )?;
            counts.settings_restored += 1;
        }

        transaction.commit()?;
        Ok(counts)
    }
}

/// Outcome counts for a backup import: rows merged versus skipped as duplicates.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ImportCounts {
    pub entries_added: usize,
    pub entries_skipped: usize,
    pub deleted_added: usize,
    pub deleted_skipped: usize,
    pub settings_restored: usize,
}

// ----------
// Leading Empty Line Sanitizer
// Description: Strips leading blank or newline characters from captured text so entries do not start with unwanted empty lines.
// ----------
pub fn strip_leading_empty_lines(text: &str) -> &str {
    let mut remaining = text;
    while let Some(line_end) = remaining.find('\n') {
        let first_line = &remaining[..line_end];
        if first_line.trim().is_empty() {
            remaining = &remaining[line_end + 1..];
        } else {
            break;
        }
    }
    remaining
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::{ClipboardMetadata, ClipboardStore};

    #[test]
    fn creates_schema_and_stores_text() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");

        let metadata = super::ClipboardMetadata {
            source_app: Some("Notepad".into()),
            source_process: Some("notepad.exe".into()),
            window_title: Some("Notes".into()),
            app_icon: Some("data:image/bmp;base64,abc".into()),
            source_url: None,
        };
        let id = store
            .add_entry("hello from Clipbox", &metadata)
            .expect("text should be stored");
        let entry: (String, Option<String>, Option<String>, Option<String>, Option<String>) = store
            .connection
            .query_row(
                "SELECT content, source_app, source_process, window_title, app_icon
                 FROM clipboard_entries WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .expect("stored entry should be queryable");

        assert_eq!(
            entry,
            (
                "hello from Clipbox".into(),
                Some("Notepad".into()),
                Some("notepad.exe".into()),
                Some("Notes".into()),
                Some("data:image/bmp;base64,abc".into()),
            )
        );
    }

    #[test]
    fn migrates_a_legacy_database_before_storing_metadata() {
        let connection = Connection::open_in_memory().expect("in-memory database should open");
        connection
            .execute_batch(
                "CREATE TABLE clipboard_entries (
                    id INTEGER PRIMARY KEY,
                    content TEXT NOT NULL,
                    copied_at INTEGER NOT NULL
                );",
            )
            .expect("legacy schema should be created");

        let store = ClipboardStore::initialize(connection).expect("legacy schema should migrate");
        let metadata = super::ClipboardMetadata {
            source_app: Some("Browser".into()),
            source_process: Some("browser.exe".into()),
            window_title: Some("A page".into()),
            app_icon: Some("data:image/bmp;base64,xyz".into()),
            source_url: None,
        };

        let id = store
            .add_entry("legacy-compatible text", &metadata)
            .expect("metadata should be stored after migration");
        let source_app: Option<String> = store
            .connection
            .query_row(
                "SELECT source_app FROM clipboard_entries WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .expect("migrated column should be queryable");

        assert_eq!(source_app.as_deref(), Some("Browser"));
    }

    #[test]
    fn returns_recent_entries_newest_first() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        store
            .add_text("first")
            .expect("first text should be stored");
        store
            .add_text("second")
            .expect("second text should be stored");

        let entries = store
            .recent_entries(10)
            .expect("recent entries should be queryable");

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].content, "second");
        assert_eq!(entries[1].content, "first");
    }

    #[test]
    fn clears_all_stored_entries() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        store.add_text("first").expect("first text should be stored");
        store.add_text("second").expect("second text should be stored");

        let deleted = store.clear_entries().expect("entries should be cleared");
        assert_eq!(deleted, 2);

        let entries = store.recent_entries(10).expect("recent entries should be queryable");
        assert!(entries.is_empty());
    }

    #[test]
    fn strips_leading_empty_lines_when_storing() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        store
            .add_text("\r\n\r\nHello world\nSecond line")
            .expect("text should be stored");

        let entries = store
            .recent_entries(10)
            .expect("recent entries should be queryable");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content, "Hello world\nSecond line");
    }

    #[test]
    fn stores_and_retrieves_app_settings() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        assert_eq!(store.get_setting("start_minimized").unwrap(), None);

        store.set_setting("start_minimized", "true").unwrap();
        assert_eq!(
            store.get_setting("start_minimized").unwrap(),
            Some("true".into())
        );

        store.set_setting("start_minimized", "false").unwrap();
        assert_eq!(
            store.get_setting("start_minimized").unwrap(),
            Some("false".into())
        );
    }

    #[test]
    fn prunes_entries_exceeding_retention_limit() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        for i in 1..=10 {
            store.add_text(&format!("entry {i}")).unwrap();
        }

        assert_eq!(store.recent_entries(20).unwrap().len(), 10);

        let pruned = store.prune_entries(5).unwrap();
        assert_eq!(pruned, 5);

        let remaining = store.recent_entries(20).unwrap();
        assert_eq!(remaining.len(), 5);
        assert_eq!(remaining[0].content, "entry 10");
        assert_eq!(remaining[4].content, "entry 6");
    }

    #[test]
    fn automatically_enforces_retention_limit_on_add() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        store.set_setting("retention_limit", "3").unwrap();

        for i in 1..=5 {
            store.add_text(&format!("item {i}")).unwrap();
        }

        let remaining = store.recent_entries(20).unwrap();
        assert_eq!(remaining.len(), 3);
        assert_eq!(remaining[0].content, "item 5");
        assert_eq!(remaining[2].content, "item 3");
    }

    #[test]
    fn deletes_a_single_entry_by_id_without_resequencing() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        let id1 = store.add_text("first").unwrap();
        let id2 = store.add_text("second").unwrap();
        let id3 = store.add_text("third").unwrap();

        assert_eq!(store.recent_entries(10).unwrap().len(), 3);

        // Delete id2 ("second") in the middle; remaining IDs stay immutable.
        let deleted = store.delete_entry(id2).unwrap();
        assert!(deleted);

        let entries = store.recent_entries(10).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].content, "third");
        assert_eq!(entries[0].id, id3);
        assert_eq!(entries[1].content, "first");
        assert_eq!(entries[1].id, id1);

        // Deleting the highest ID also never causes ID reuse.
        assert!(store.delete_entry(id3).unwrap());
        let id_new = store.add_text("fourth").unwrap();
        assert!(id_new > id3);
    }

    #[test]
    fn pins_and_unpins_entries_sorting_pinned_to_top() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        let id1 = store.add_text("first").unwrap();
        let id2 = store.add_text("second").unwrap();
        let id3 = store.add_text("third").unwrap();

        // Default order: newest first
        let entries = store.recent_entries(10).unwrap();
        assert_eq!(entries[0].id, id3);
        assert_eq!(entries[1].id, id2);
        assert_eq!(entries[2].id, id1);
        assert!(!entries[2].is_pinned);

        // Pin the oldest entry (id1)
        let new_state = store.toggle_pinned(id1).unwrap();
        assert!(new_state);

        let entries = store.recent_entries(10).unwrap();
        assert_eq!(entries[0].id, id1);
        assert!(entries[0].is_pinned);
        assert_eq!(entries[1].id, id3);
        assert_eq!(entries[2].id, id2);

        // Unpin entry (id1)
        let new_state = store.toggle_pinned(id1).unwrap();
        assert!(!new_state);

        let entries = store.recent_entries(10).unwrap();
        assert_eq!(entries[0].id, id3);
        assert_eq!(entries[1].id, id2);
        assert_eq!(entries[2].id, id1);
    }

    #[test]
    fn pinned_entries_survive_retention_pruning() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        let id1 = store.add_text("entry 1").unwrap();
        let _id2 = store.add_text("entry 2").unwrap();
        let _id3 = store.add_text("entry 3").unwrap();
        let _id4 = store.add_text("entry 4").unwrap();
        let _id5 = store.add_text("entry 5").unwrap();

        // Pin entry 1
        store.set_pinned(id1, true).unwrap();

        // Prune to limit 2 (unpinned limit)
        let pruned = store.prune_entries(2).unwrap();
        assert_eq!(pruned, 2); // entries 2 and 3 pruned

        let entries = store.recent_entries(10).unwrap();
        assert_eq!(entries.len(), 3);
        // Entry 1 is pinned and at top
        assert_eq!(entries[0].id, id1);
        assert!(entries[0].is_pinned);
        // Entries 5 and 4 are the 2 newest unpinned entries
        assert_eq!(entries[1].content, "entry 5");
        assert_eq!(entries[2].content, "entry 4");
    }

    #[test]
    fn stores_and_retrieves_image_entry() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        let metadata = ClipboardMetadata {
            source_app: Some("SnippingTool".into()),
            source_process: Some("SnippingTool.exe".into()),
            window_title: Some("Snipping Tool".into()),
            app_icon: None,
            source_url: None,
        };

        let fake_data_url = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
        let id = store.add_image_entry(fake_data_url, "800x600", &metadata).unwrap();
        assert_eq!(id, 1);

        let entries = store.recent_entries(10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].entry_type, "image");
        assert_eq!(entries[0].image_data.as_deref(), Some(fake_data_url));
        assert_eq!(entries[0].image_dimensions.as_deref(), Some("800x600"));
        assert_eq!(entries[0].source_app.as_deref(), Some("SnippingTool"));
    }

    #[test]
    fn stores_and_retrieves_file_entry() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        let metadata = ClipboardMetadata {
            source_app: Some("File Explorer".into()),
            source_process: Some("explorer.exe".into()),
            window_title: Some("Documents".into()),
            app_icon: None,
            source_url: None,
        };

        let fake_json = r#"[{"name":"test.txt","path":"C:\\test.txt","extension":"txt","size":128,"is_directory":false}]"#;
        let id = store.add_file_entry("test.txt (128 B)", fake_json, &metadata).unwrap();
        assert_eq!(id, 1);

        let entries = store.recent_entries(10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].entry_type, "file");
        assert_eq!(entries[0].content, "test.txt (128 B)");
        assert_eq!(entries[0].files_data.as_deref(), Some(fake_json));
        assert_eq!(entries[0].source_app.as_deref(), Some("File Explorer"));
    }

    #[test]
    fn finds_existing_entry_and_bumps_timestamp() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        let id1 = store.add_text("First item").unwrap();
        let _id2 = store.add_text("Second item").unwrap();

        // Finding existing text
        let found = store.find_existing_text("First item").unwrap();
        assert_eq!(found, Some(id1));

        let not_found = store.find_existing_text("Nonexistent").unwrap();
        assert_eq!(not_found, None);

        // Bump id1
        let meta = ClipboardMetadata {
            source_app: Some("Updated App".into()),
            ..Default::default()
        };
        store.bump_entry(id1, &meta).unwrap();

        let updated = store.get_entry(id1).unwrap().unwrap();
        assert_eq!(updated.source_app.as_deref(), Some("Updated App"));

        // When listing recent entries, the bumped entry appears first
        let entries = store.recent_entries(10).unwrap();
        assert_eq!(entries[0].id, id1);
        assert_eq!(entries[0].content, "First item");
    }

    #[test]
    fn stores_and_retrieves_browser_source_url() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        let metadata = ClipboardMetadata {
            source_app: Some("Brave Browser".into()),
            source_process: Some("brave.exe".into()),
            window_title: Some("GitHub Issue #3".into()),
            app_icon: None,
            source_url: Some("https://github.com/mwsk75996/clipbox/issues/3".into()),
        };

        let id = store.add_entry("Copied web text", &metadata).unwrap();
        let entry = store.get_entry(id).unwrap().unwrap();

        assert_eq!(
            entry.source_url.as_deref(),
            Some("https://github.com/mwsk75996/clipbox/issues/3")
        );
        assert_eq!(entry.source_app.as_deref(), Some("Brave Browser"));

        // Plain text copy without URL
        let plain_meta = ClipboardMetadata {
            source_app: Some("Notepad".into()),
            source_process: Some("notepad.exe".into()),
            window_title: Some("Untitled".into()),
            app_icon: None,
            source_url: None,
        };
        let id2 = store.add_entry("Plain text", &plain_meta).unwrap();
        let entry2 = store.get_entry(id2).unwrap().unwrap();
        assert_eq!(entry2.source_url, None);
    }

    #[test]
    fn soft_delete_moves_record_to_archive_with_immutable_ids() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        let id1 = store
            .add_text("first")
            .expect("first text should be stored");
        let id2 = store
            .add_text("second")
            .expect("second text should be stored");
        let id3 = store
            .add_text("third")
            .expect("third text should be stored");

        assert!(store
            .soft_delete_entry(id2)
            .expect("soft delete should succeed"));

        let entries = store
            .recent_entries(10)
            .expect("recent entries should be queryable");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].content, "third");
        assert_eq!(entries[0].id, id3);
        assert_eq!(entries[1].content, "first");
        assert_eq!(entries[1].id, id1);

        let deleted = store
            .recent_deleted_entries(10)
            .expect("deleted entries should be queryable");
        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0].content, "second");
        assert_eq!(deleted[0].original_id, id2);
    }

    #[test]
    fn soft_delete_missing_id_returns_false() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        assert!(!store
            .soft_delete_entry(99)
            .expect("missing id should return false"));
        assert!(store
            .recent_deleted_entries(10)
            .expect("deleted entries should be queryable")
            .is_empty());
    }

    #[test]
    fn restore_reinserts_with_original_content_and_timestamp() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        let first_id = store
            .add_text("first")
            .expect("first text should be stored");
        let first = store
            .get_entry(first_id)
            .expect("entry should be queryable")
            .expect("entry should exist");
        store
            .add_text("second")
            .expect("second text should be stored");

        assert!(store
            .soft_delete_entry(first_id)
            .expect("soft delete should succeed"));
        let archived = store.recent_deleted_entries(10).unwrap();
        assert_eq!(archived.len(), 1);

        let new_id = store
            .restore_deleted_entry(archived[0].id)
            .expect("restore should succeed")
            .expect("restored id should be returned");
        let restored = store
            .get_entry(new_id)
            .expect("entry should be queryable")
            .expect("restored entry should exist");
        assert_eq!(restored.content, "first");
        assert_eq!(restored.copied_at, first.copied_at);
        assert!(store
            .recent_deleted_entries(10)
            .expect("deleted entries should be queryable")
            .is_empty());
    }

    #[test]
    fn restore_missing_id_returns_none() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        assert_eq!(
            store
                .restore_deleted_entry(99)
                .expect("missing id should return none"),
            None
        );
    }

    #[test]
    fn hard_delete_removes_only_the_archived_record() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        let id1 = store
            .add_text("first")
            .expect("first text should be stored");
        let id2 = store
            .add_text("second")
            .expect("second text should be stored");
        assert!(store
            .soft_delete_entry(id1)
            .expect("soft delete should succeed"));
        assert!(store
            .soft_delete_entry(id2)
            .expect("soft delete should succeed"));

        let deleted = store.recent_deleted_entries(10).unwrap();
        assert_eq!(deleted.len(), 2);

        assert!(store
            .hard_delete_entry(deleted[0].id)
            .expect("hard delete should succeed"));
        let remaining = store.recent_deleted_entries(10).unwrap();
        assert_eq!(remaining.len(), 1);

        // Active feed is untouched by archive hard-deletes.
        assert_eq!(store.recent_entries(10).unwrap().len(), 0);
        assert!(!store
            .hard_delete_entry(999)
            .expect("missing id should return false"));
    }

    #[test]
    fn purge_removes_only_expired_archive_records() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        store
            .connection
            .execute(
                "INSERT INTO deleted_entries
                    (original_id, content, copied_at, deleted_at)
                 VALUES (1, 'old', 1000, 1000), (2, 'fresh', 2000, 9000)",
                [],
            )
            .expect("archive rows should be insertable");

        let purged = store
            .purge_deleted_entries_older_than(5000)
            .expect("purge should succeed");
        assert_eq!(purged, 1);

        let remaining = store.recent_deleted_entries(10).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].content, "fresh");
    }

    #[test]
    fn clear_archives_instead_of_destroying() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        store
            .add_text("first")
            .expect("first text should be stored");
        store
            .add_text("second")
            .expect("second text should be stored");

        let archived = store.clear_entries().expect("clear should succeed");
        assert_eq!(archived, 2);
        assert!(store
            .recent_entries(10)
            .expect("recent entries should be queryable")
            .is_empty());

        let deleted = store.recent_deleted_entries(10).unwrap();
        assert_eq!(deleted.len(), 2);
    }

    #[test]
    fn maps_deleted_retention_setting_to_lifetimes() {
        assert_eq!(
            super::deleted_retention_lifetime_seconds("1hour"),
            Some(3_600)
        );
        assert_eq!(
            super::deleted_retention_lifetime_seconds("1day"),
            Some(86_400)
        );
        assert_eq!(
            super::deleted_retention_lifetime_seconds("7days"),
            Some(7 * 86_400)
        );
        assert_eq!(
            super::deleted_retention_lifetime_seconds("30days"),
            Some(30 * 86_400)
        );
        assert_eq!(
            super::deleted_retention_lifetime_seconds("immediately"),
            None
        );
        assert_eq!(super::deleted_retention_lifetime_seconds("bogus"), None);
    }

    #[test]
    fn stores_and_queries_ocr_text() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        let fake_data_url = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
        let metadata = super::ClipboardMetadata::default();
        let id = store
            .add_image_entry(fake_data_url, "1x1", &metadata)
            .expect("image should be stored");

        assert_eq!(
            store.images_missing_ocr_data(10).expect("missing scan should query"),
            vec![id]
        );

        store
            .set_ocr_text(id, "hello world")
            .expect("ocr text should store");
        store
            .set_ocr_boxes(id, "[]")
            .expect("ocr boxes should store");
        assert!(store
            .images_missing_ocr_data(10)
            .expect("missing scan should query")
            .is_empty());

        let entry = store
            .get_entry(id)
            .expect("entry should be queryable")
            .expect("entry should exist");
        assert_eq!(entry.ocr_text.as_deref(), Some("hello world"));
    }

    #[test]
    fn empty_ocr_text_marks_completed_scans() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        let fake_data_url = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
        let metadata = super::ClipboardMetadata::default();
        let id = store
            .add_image_entry(fake_data_url, "1x1", &metadata)
            .expect("image should be stored");

        // Empty string (found nothing) still counts as scanned.
        store.set_ocr_text(id, "").expect("ocr text should store");
        store.set_ocr_boxes(id, "[]").expect("ocr boxes should store");
        assert!(store
            .images_missing_ocr_data(10)
            .expect("missing scan should query")
            .is_empty());
    }

    #[test]
    fn stores_and_queries_ocr_boxes() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        let fake_data_url = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
        let metadata = super::ClipboardMetadata::default();
        let id = store
            .add_image_entry(fake_data_url, "1x1", &metadata)
            .expect("image should be stored");

        let boxes = r#"[{"t":"hi","x":0.1,"y":0.2,"w":0.3,"h":0.05}]"#;
        store
            .set_ocr_text(id, "hi")
            .expect("ocr text should store");
        // Text present but boxes missing still counts as unscanned (heals
        // rows archived before boxes existed).
        assert_eq!(
            store.images_missing_ocr_data(10).expect("missing scan should query"),
            vec![id]
        );
        store
            .set_ocr_boxes(id, boxes)
            .expect("ocr boxes should store");
        assert!(store
            .images_missing_ocr_data(10)
            .expect("missing scan should query")
            .is_empty());
        let entry = store
            .get_entry(id)
            .expect("entry should be queryable")
            .expect("entry should exist");
        assert_eq!(entry.ocr_boxes.as_deref(), Some(boxes));
    }

    #[test]
    fn backup_import_round_trips_entries_archive_and_settings() {
        let source = ClipboardStore::in_memory().expect("in-memory database should open");
        let metadata = ClipboardMetadata::default();
        let text_id = source
            .add_entry("back me up", &metadata)
            .expect("text should be stored");
        source
            .set_pinned(text_id, true)
            .expect("pin should apply");
        let image_url = "data:image/png;base64,aW1hZ2U=";
        let image_id = source
            .add_image_entry(image_url, "2x2", &metadata)
            .expect("image should be stored");
        source
            .set_ocr_text(image_id, "hello")
            .expect("ocr text should store");
        source
            .add_file_entry("2 files", r#"{"names":["a.txt"]}"#, &metadata)
            .expect("file entry should be stored");
        source
            .soft_delete_entry(text_id)
            .expect("soft delete should archive");
        source
            .set_setting("close_behavior", "hide")
            .expect("setting should store");

        let entries = source.all_entries().expect("entries should dump");
        let deleted = source
            .all_deleted_entries()
            .expect("archive should dump");
        let settings = source.all_settings().expect("settings should dump");
        assert_eq!(entries.len(), 2);
        assert_eq!(deleted.len(), 1);
        assert!(settings.contains(&("close_behavior".into(), "hide".into())));

        let mut target = ClipboardStore::in_memory().expect("in-memory database should open");
        let counts = target
            .import_backup(&entries, &deleted, &settings)
            .expect("import should succeed");
        assert_eq!(
            counts,
            super::ImportCounts {
                entries_added: 2,
                entries_skipped: 0,
                deleted_added: 1,
                deleted_skipped: 0,
                settings_restored: settings.len(),
            }
        );

        // Pins, timestamps, OCR, archive state, and settings survive.
        let live = target.all_entries().expect("entries should read back");
        assert!(live.iter().any(|e| e.content == "2 files"));
        let image = live
            .iter()
            .find(|e| e.entry_type == "image")
            .expect("image should import");
        assert_eq!(image.image_data.as_deref(), Some(image_url));
        assert_eq!(image.ocr_text.as_deref(), Some("hello"));
        let archive = target
            .all_deleted_entries()
            .expect("archive should read back");
        assert_eq!(archive.len(), 1);
        assert!(archive[0].is_pinned);
        assert_eq!(
            target
                .get_setting("close_behavior")
                .expect("setting should read back"),
            Some("hide".into())
        );

        // Re-importing the same file is a no-op (pin upgrade path included).
        let again = target
            .import_backup(&entries, &deleted, &settings)
            .expect("re-import should succeed");
        assert_eq!(again.entries_added, 0);
        assert_eq!(again.entries_skipped, 2);
        assert_eq!(again.deleted_added, 0);
        assert_eq!(again.deleted_skipped, 1);
    }

    #[test]
    fn backup_import_upgrades_pin_on_duplicate() {
        let mut store = ClipboardStore::in_memory().expect("in-memory database should open");
        let metadata = ClipboardMetadata::default();
        store
            .add_entry("pin me later", &metadata)
            .expect("text should be stored");

        let mut backup = store.all_entries().expect("entries should dump");
        assert_eq!(backup.len(), 1);
        backup[0].is_pinned = true;

        let counts = store
            .import_backup(&backup, &[], &[])
            .expect("import should succeed");
        assert_eq!(counts.entries_added, 0);
        assert_eq!(counts.entries_skipped, 1);
        let live = store.all_entries().expect("entries should read back");
        assert!(live[0].is_pinned);
    }

    #[test]
    fn recent_queries_use_order_index_without_temp_btree() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        store.add_text("hello").expect("entry should store");

        let mut stmt = store
            .connection
            .prepare(
                "EXPLAIN QUERY PLAN
                 SELECT id, content
                 FROM clipboard_entries
                 ORDER BY is_pinned DESC, copied_at DESC, id DESC
                 LIMIT 100",
            )
            .expect("explain should prepare");
        let plans: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(3))
            .expect("explain should query")
            .collect::<Result<_, _>>()
            .expect("plan rows should decode");

        assert!(
            plans.iter().any(|p| p.contains("idx_clipboard_entries_order")),
            "expected idx_clipboard_entries_order in query plan, got {plans:?}"
        );
        assert!(
            !plans.iter().any(|p| p.contains("TEMP B-TREE")),
            "expected no temp B-tree sort in query plan, got {plans:?}"
        );
    }

    #[test]
    fn summaries_omit_image_blobs_and_get_entry_image_loads_on_demand() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        let metadata = ClipboardMetadata::default();
        let data_url = "data:image/png;base64,aW1hZ2U=";
        let img_id = store
            .add_image_entry(data_url, "640x480", &metadata)
            .expect("image should store");
        let txt_id = store.add_text("plain text").expect("text should store");

        let summaries = store
            .recent_entry_summaries(10)
            .expect("summaries should query");
        assert_eq!(summaries.len(), 2);
        let img_summary = summaries
            .iter()
            .find(|e| e.id == img_id)
            .expect("image summary should exist");
        assert_eq!(img_summary.entry_type, "image");
        assert_eq!(img_summary.image_dimensions.as_deref(), Some("640x480"));
        assert_eq!(img_summary.image_data, None);

        assert_eq!(
            store.get_entry_image(img_id).expect("image lookup should work"),
            Some(data_url.to_string())
        );
        assert_eq!(
            store.get_entry_image(txt_id).expect("text lookup should work"),
            None
        );
        assert_eq!(
            store.get_entry_image(9999).expect("missing lookup should work"),
            None
        );

        let tray = store
            .recent_tray_entries(5)
            .expect("tray entries should query");
        assert_eq!(tray.len(), 2);
        assert_eq!(tray[0], (txt_id, "plain text".to_string()));
        assert_eq!(tray[1], (img_id, "Image (640x480)".to_string()));
        assert_eq!(store.entry_count().expect("entry_count should work"), 2);
        assert_eq!(
            store
                .deleted_entry_count()
                .expect("deleted_entry_count should work"),
            0
        );

        store.soft_delete_entry(img_id).expect("soft delete should succeed");
        assert_eq!(store.entry_count().expect("entry_count should work"), 1);
        assert_eq!(
            store
                .deleted_entry_count()
                .expect("deleted_entry_count should work"),
            1
        );
        let deleted_summaries = store
            .recent_deleted_entry_summaries(10)
            .expect("deleted summaries should query");
        assert_eq!(deleted_summaries.len(), 1);
        assert_eq!(deleted_summaries[0].image_data, None);
        assert_eq!(
            deleted_summaries[0].image_dimensions.as_deref(),
            Some("640x480")
        );
    }

    #[test]
    fn query_entry_summaries_paginates_beyond_100_and_filters_full_history() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        let browser_meta = ClipboardMetadata {
            source_app: Some("Brave".into()),
            source_process: Some("brave.exe".into()),
            window_title: Some("Docs".into()),
            app_icon: None,
            source_url: Some("https://example.com/guide".into()),
        };
        let editor_meta = ClipboardMetadata {
            source_app: Some("Code".into()),
            source_process: Some("Code.exe".into()),
            window_title: Some("main.rs".into()),
            app_icon: None,
            source_url: None,
        };

        for i in 1..=120 {
            let meta = if i % 2 == 0 { &browser_meta } else { &editor_meta };
            store
                .add_entry(&format!("clip_item_{i:03}"), meta)
                .expect("text should store");
        }
        let img_id = store
            .add_image_entry("data:image/png;base64,aW1hZ2U=", "100x50", &browser_meta)
            .expect("image should store");
        store
            .set_ocr_text(img_id, "screenshot needle 100%_match")
            .expect("ocr should store");

        // Page 1 and page 5 across 121 total records.
        let page1 = store
            .query_entry_summaries(&super::EntryQuery {
                limit: 25,
                offset: 0,
                ..Default::default()
            })
            .expect("page 1 should query");
        assert_eq!(page1.total_count, 121);
        assert_eq!(page1.entries.len(), 25);
        assert_eq!(page1.entries[0].id, img_id);
        assert_eq!(page1.entries[0].image_data, None);

        let page5 = store
            .query_entry_summaries(&super::EntryQuery {
                limit: 25,
                offset: 100,
                ..Default::default()
            })
            .expect("page 5 should query");
        assert_eq!(page5.total_count, 121);
        assert_eq!(page5.entries.len(), 21);
        assert_eq!(page5.entries.last().unwrap().content, "clip_item_001");

        // Search matches OCR text and escapes literal `%` and `_` in LIKE.
        let ocr_hits = store
            .query_entry_summaries(&super::EntryQuery {
                search: Some("100%_match".into()),
                limit: 25,
                offset: 0,
                ..Default::default()
            })
            .expect("ocr search should query");
        assert_eq!(ocr_hits.total_count, 1);
        assert_eq!(ocr_hits.entries[0].id, img_id);

        // Source app and entry type filters operate across the full table.
        let code_text = store
            .query_entry_summaries(&super::EntryQuery {
                source_app: Some("Code".into()),
                entry_type: Some("text".into()),
                limit: 25,
                offset: 0,
                ..Default::default()
            })
            .expect("filtered query should succeed");
        assert_eq!(code_text.total_count, 60);
        assert_eq!(code_text.entries.len(), 25);

        let apps = store
            .list_source_apps()
            .expect("distinct apps should query");
        assert_eq!(apps, vec!["Brave".to_string(), "Code".to_string()]);
    }

    #[test]
    fn clear_and_restore_preserve_never_reused_ids() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        let id1 = store.add_text("alpha").unwrap();
        let id2 = store.add_text("beta").unwrap();
        let id3 = store.add_text("gamma").unwrap();

        assert_eq!(store.clear_entries().unwrap(), 3);
        assert!(store.recent_entries(10).unwrap().is_empty());

        // New capture after clearing must never reuse IDs 1..=3.
        let id4 = store.add_text("delta").unwrap();
        assert!(id4 > id3, "expected id4 ({id4}) > id3 ({id3})");

        // Restoring an archived entry allocates a fresh ID > id4.
        let archived = store.recent_deleted_entries(10).unwrap();
        assert_eq!(archived.len(), 3);
        let restored_id = store
            .restore_deleted_entry(archived[0].id)
            .unwrap()
            .expect("restore should succeed");
        assert!(
            restored_id > id4,
            "expected restored_id ({restored_id}) > id4 ({id4})"
        );
        // Deleting id1/id2 again is a no-op since they are already gone.
        assert!(!store.delete_entry(id1).unwrap());
        assert!(!store.soft_delete_entry(id2).unwrap());
    }
}
