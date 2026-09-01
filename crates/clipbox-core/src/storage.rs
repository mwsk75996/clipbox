use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, Result};

/// Metadata captured along with a clipboard entry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClipboardMetadata {
    pub source_app: Option<String>,
    pub source_process: Option<String>,
    pub window_title: Option<String>,
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
                window_title TEXT
            );",
        )?;

        // Keep databases created by earlier versions usable.
        for (column, definition) in [
            ("source_app", "source_app TEXT"),
            ("source_process", "source_process TEXT"),
            ("window_title", "window_title TEXT"),
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
        let copied_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        self.connection.execute(
            "INSERT INTO clipboard_entries
                (content, copied_at, source_app, source_process, window_title)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                content,
                copied_at,
                metadata.source_app.as_deref(),
                metadata.source_process.as_deref(),
                metadata.window_title.as_deref(),
            ],
        )?;

        Ok(self.connection.last_insert_rowid())
    }
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
        };
        let id = store
            .add_entry("hello from Clipbox", &metadata)
            .expect("text should be stored");
        let entry: (String, Option<String>, Option<String>, Option<String>) = store
            .connection
            .query_row(
                "SELECT content, source_app, source_process, window_title
                 FROM clipboard_entries WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("stored entry should be queryable");

        assert_eq!(
            entry,
            (
                "hello from Clipbox".into(),
                Some("Notepad".into()),
                Some("notepad.exe".into()),
                Some("Notes".into()),
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
}
