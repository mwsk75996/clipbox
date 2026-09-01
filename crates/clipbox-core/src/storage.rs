use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, Result};

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
                copied_at INTEGER NOT NULL
            );",
        )?;

        Ok(Self { connection })
    }

    #[cfg(test)]
    fn in_memory() -> Result<Self> {
        Self::initialize(Connection::open_in_memory()?)
    }

    /// Add a text clipboard entry and return its database id.
    pub fn add_text(&self, content: &str) -> Result<i64> {
        let copied_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        self.connection.execute(
            "INSERT INTO clipboard_entries (content, copied_at) VALUES (?1, ?2)",
            params![content, copied_at],
        )?;

        Ok(self.connection.last_insert_rowid())
    }
}

#[cfg(test)]
mod tests {
    use super::ClipboardStore;

    #[test]
    fn creates_schema_and_stores_text() {
        let store = ClipboardStore::in_memory().expect("in-memory database should open");

        let id = store
            .add_text("hello from Clipbox")
            .expect("text should be stored");
        let content: String = store
            .connection
            .query_row(
                "SELECT content FROM clipboard_entries WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .expect("stored text should be queryable");

        assert_eq!(content, "hello from Clipbox");
    }
}
