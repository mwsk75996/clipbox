// ----------
// Core Clipboard Storage
// Description: SQLite-backed storage managing clipboard entries with source app metadata, process name, window title, and application icon.
// ----------

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, Result};

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
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS clipboard_entries (
                id INTEGER PRIMARY KEY,
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
                source_url TEXT
            );
            CREATE TABLE IF NOT EXISTS app_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS deleted_entries (
                id INTEGER PRIMARY KEY,
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

        // Keep databases created by earlier versions usable.
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
            if !Self::has_column(&connection, column)? {
                connection.execute(
                    &format!("ALTER TABLE clipboard_entries ADD COLUMN {definition}"),
                    [],
                )?;
            }
        }

        Ok(Self { connection })
    }

    fn has_column(connection: &Connection, column: &str) -> Result<bool> {
        let mut statement = connection.prepare("PRAGMA table_info(clipboard_entries)")?;
        let mut rows = statement.query([])?;

        while let Some(row) = rows.next()? {
            if row.get::<_, String>(1)? == column {
                return Ok(true);
            }
        }

        Ok(false)
    }

    #[cfg(test)]
    fn in_memory() -> Result<Self> {
        Self::initialize(Connection::open_in_memory()?)
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

        self.connection.execute(
            "INSERT INTO clipboard_entries
                (content, copied_at, source_app, source_process, window_title, app_icon, source_url)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                content_to_store,
                copied_at,
                metadata.source_app.as_deref(),
                metadata.source_process.as_deref(),
                metadata.window_title.as_deref(),
                metadata.app_icon.as_deref(),
                metadata.source_url.as_deref(),
            ],
        )?;

        let last_id = self.connection.last_insert_rowid();

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

        self.connection.execute(
            "INSERT INTO clipboard_entries (
                content, copied_at, source_app, source_process, window_title, app_icon, is_pinned, entry_type, image_data, image_dimensions, source_url
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 'image', ?7, ?8, ?9)",
            params![
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

        let last_id = self.connection.last_insert_rowid();

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

        self.connection.execute(
            "INSERT INTO clipboard_entries (
                content, copied_at, source_app, source_process, window_title, app_icon, is_pinned, entry_type, files_data, source_url
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 'file', ?7, ?8)",
            params![
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

        let last_id = self.connection.last_insert_rowid();

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

        let deleted = self.connection.execute(
            "DELETE FROM clipboard_entries
             WHERE is_pinned = 0 AND id NOT IN (
                 SELECT id FROM clipboard_entries WHERE is_pinned = 0 ORDER BY id DESC LIMIT ?1
             )",
            params![limit as i64],
        )?;

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
        let entries = statement.query_map(params![i64::from(limit)], |row| {
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

        entries.collect()
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
    // Clear History Storage
    // Description: Moves all stored clipboard entries into the Recently Deleted archive (safety net) and returns the number of archived rows.
    // ----------
    /// Move all stored clipboard records into the archive and return the archived count.
    pub fn clear_entries(&self) -> Result<usize> {
        let archived = self.connection.execute(
            "INSERT INTO deleted_entries
                (original_id, content, copied_at, source_app, source_process, window_title, app_icon, is_pinned, entry_type, image_data, image_dimensions, files_data, source_url, deleted_at)
             SELECT id, content, copied_at, source_app, source_process, window_title, app_icon, is_pinned, entry_type, image_data, image_dimensions, files_data, source_url, ?1
             FROM clipboard_entries",
            params![current_unix_timestamp()],
        )?;

        self.connection
            .execute("DELETE FROM clipboard_entries", [])?;

        Ok(archived)
    }

    // ----------
    // Delete Single Entry
    // Description: Deletes an individual clipboard history entry by its primary key id and re-sequences higher IDs to maintain contiguous history numbering.
    // ----------
    /// Delete a single clipboard record by id. Re-sequences higher IDs so numbers remain contiguous.
    pub fn delete_entry(&self, id: i64) -> Result<bool> {
        let deleted = self.connection.execute(
            "DELETE FROM clipboard_entries WHERE id = ?1",
            params![id],
        )?;

        if deleted > 0 {
            // Re-sequence IDs greater than the deleted id so numbers remain contiguous.
            // Use negative temporary values to avoid any unique constraint conflicts.
            self.connection.execute(
                "UPDATE clipboard_entries SET id = -id WHERE id > ?1",
                params![id],
            )?;
            self.connection.execute(
                "UPDATE clipboard_entries SET id = -id - 1 WHERE id < 0",
                [],
            )?;
            // Keep sqlite_sequence in sync so next autoincrement ID matches
            let _ = self.connection.execute(
                "UPDATE sqlite_sequence SET seq = (SELECT COALESCE(MAX(id), 0) FROM clipboard_entries) WHERE name = 'clipboard_entries'",
                [],
            );
        }

        Ok(deleted > 0)
    }

    // ----------
    // Recently Deleted Archive
    // Description: Soft-delete safety net. Deleted records move to a dedicated archive
    // table (keeping the active table's ID resequencing intact) until restored,
    // hard-deleted, or purged past the configured retention timespan.
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

    /// Archive a single active record. The active table keeps its existing ID
    /// resequencing; the archive row keeps the original id for reference.
    /// Returns false when no active record has `id`.
    pub fn soft_delete_entry(&self, id: i64) -> Result<bool> {
        let entry = match self.get_entry(id)? {
            Some(entry) => entry,
            None => return Ok(false),
        };

        self.connection.execute(
            "INSERT INTO deleted_entries
                (original_id, content, copied_at, source_app, source_process, window_title, app_icon, is_pinned, entry_type, image_data, image_dimensions, files_data, source_url, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                entry.id,
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
                current_unix_timestamp(),
            ],
        )?;

        // Remove from the active table (reuses the hard-delete resequencing).
        self.delete_entry(id)
    }

    /// Return a single archived record by its archive id.
    fn get_deleted_entry(&self, id: i64) -> Result<Option<DeletedEntry>> {
        let mut statement = self.connection.prepare(
            "SELECT id, original_id, content, copied_at, source_app, source_process, window_title, app_icon, is_pinned, entry_type, image_data, image_dimensions, files_data, source_url, deleted_at
             FROM deleted_entries
             WHERE id = ?1",
        )?;
        let mut rows = statement.query_map(params![id], Self::map_deleted_entry)?;

        if let Some(entry) = rows.next() {
            Ok(Some(entry?))
        } else {
            Ok(None)
        }
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

    /// Restore an archived record into the active feed with its original
    /// content and timestamp. Returns the new active id, or None when the
    /// archive id is missing.
    pub fn restore_deleted_entry(&self, id: i64) -> Result<Option<i64>> {
        let archived = match self.get_deleted_entry(id)? {
            Some(archived) => archived,
            None => return Ok(None),
        };

        self.connection.execute(
            "INSERT INTO clipboard_entries
                (content, copied_at, source_app, source_process, window_title, app_icon, is_pinned, entry_type, image_data, image_dimensions, files_data, source_url)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                archived.content,
                archived.copied_at,
                archived.source_app.as_deref(),
                archived.source_process.as_deref(),
                archived.window_title.as_deref(),
                archived.app_icon.as_deref(),
                i32::from(archived.is_pinned),
                archived.entry_type,
                archived.image_data.as_deref(),
                archived.image_dimensions.as_deref(),
                archived.files_data.as_deref(),
                archived.source_url.as_deref(),
            ],
        )?;
        let new_id = self.connection.last_insert_rowid();

        self.connection
            .execute("DELETE FROM deleted_entries WHERE id = ?1", params![id])?;

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

    /// Ids of image entries never scanned for text, oldest first.
    pub fn images_missing_ocr_text(&self, limit: u32) -> Result<Vec<i64>> {
        let mut statement = self.connection.prepare(
            "SELECT id FROM clipboard_entries
             WHERE entry_type = 'image' AND ocr_text IS NULL
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
    fn deletes_a_single_entry_by_id() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        let _id1 = store.add_text("first").unwrap(); // 1
        let id2 = store.add_text("second").unwrap(); // 2
        let _id3 = store.add_text("third").unwrap(); // 3

        assert_eq!(store.recent_entries(10).unwrap().len(), 3);

        // Delete id2 ("second") in the middle
        let deleted = store.delete_entry(id2).unwrap();
        assert!(deleted);

        let entries = store.recent_entries(10).unwrap();
        assert_eq!(entries.len(), 2);
        // "third" originally had id 3, now re-sequenced to id 2
        assert_eq!(entries[0].content, "third");
        assert_eq!(entries[0].id, 2);
        // "first" originally had id 1, remains id 1
        assert_eq!(entries[1].content, "first");
        assert_eq!(entries[1].id, 1);

        // Subsequent additions get next sequential id (3)
        let id_new = store.add_text("fourth").unwrap();
        assert_eq!(id_new, 3);
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
    fn soft_delete_moves_record_to_archive_and_resequences_active() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");
        store
            .add_text("first")
            .expect("first text should be stored");
        store
            .add_text("second")
            .expect("second text should be stored");
        store
            .add_text("third")
            .expect("third text should be stored");

        assert!(store
            .soft_delete_entry(2)
            .expect("soft delete should succeed"));

        let entries = store
            .recent_entries(10)
            .expect("recent entries should be queryable");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].content, "third");
        assert_eq!(entries[0].id, 2);
        assert_eq!(entries[1].content, "first");
        assert_eq!(entries[1].id, 1);

        let deleted = store
            .recent_deleted_entries(10)
            .expect("deleted entries should be queryable");
        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0].content, "second");
        assert_eq!(deleted[0].original_id, 2);
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
        store
            .add_text("first")
            .expect("first text should be stored");
        store
            .add_text("second")
            .expect("second text should be stored");
        assert!(store
            .soft_delete_entry(1)
            .expect("soft delete should succeed"));
        assert!(store
            .soft_delete_entry(1)
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
            store.images_missing_ocr_text(10).expect("missing scan should query"),
            vec![id]
        );

        store
            .set_ocr_text(id, "hello world")
            .expect("ocr text should store");
        assert!(store
            .images_missing_ocr_text(10)
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
        assert!(store
            .images_missing_ocr_text(10)
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
            .set_ocr_boxes(id, boxes)
            .expect("ocr boxes should store");
        let entry = store
            .get_entry(id)
            .expect("entry should be queryable")
            .expect("entry should exist");
        assert_eq!(entry.ocr_boxes.as_deref(), Some(boxes));
    }
}
