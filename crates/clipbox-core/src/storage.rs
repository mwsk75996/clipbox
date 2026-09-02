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
                app_icon TEXT
            );",
        )?;

        // Keep databases created by earlier versions usable.
        for (column, definition) in [
            ("source_app", "source_app TEXT"),
            ("source_process", "source_process TEXT"),
            ("window_title", "window_title TEXT"),
            ("app_icon", "app_icon TEXT"),
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
                (content, copied_at, source_app, source_process, window_title, app_icon)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                content_to_store,
                copied_at,
                metadata.source_app.as_deref(),
                metadata.source_process.as_deref(),
                metadata.window_title.as_deref(),
                metadata.app_icon.as_deref(),
            ],
        )?;

        Ok(self.connection.last_insert_rowid())
    }

    /// Return the newest stored entries first.
    pub fn recent_entries(&self, limit: u32) -> Result<Vec<ClipboardEntry>> {
        let mut statement = self.connection.prepare(
            "SELECT id, content, copied_at, source_app, source_process, window_title, app_icon
             FROM clipboard_entries
             ORDER BY id DESC
             LIMIT ?1",
        )?;
        let entries = statement.query_map(params![i64::from(limit)], |row| {
            Ok(ClipboardEntry {
                id: row.get(0)?,
                content: row.get(1)?,
                copied_at: row.get(2)?,
                source_app: row.get(3)?,
                source_process: row.get(4)?,
                window_title: row.get(5)?,
                app_icon: row.get(6)?,
            })
        })?;

        entries.collect()
    }

    // ----------
    // Clear History Storage
    // Description: Deletes all stored clipboard entries from the SQLite database.
    // ----------
    /// Delete all stored clipboard records and return the number of deleted rows.
    pub fn clear_entries(&self) -> Result<usize> {
        self.connection.execute("DELETE FROM clipboard_entries", [])
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

    use super::ClipboardStore;

    #[test]
    fn creates_schema_and_stores_text() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");

        let metadata = super::ClipboardMetadata {
            source_app: Some("Notepad".into()),
            source_process: Some("notepad.exe".into()),
            window_title: Some("Notes".into()),
            app_icon: Some("data:image/bmp;base64,abc".into()),
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
}
