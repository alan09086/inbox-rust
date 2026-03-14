use std::path::Path;

use rusqlite::Connection;
use tracing::info;

use crate::error::Result;
use crate::migrations;

/// SQLite metadata store for Inboxly.
///
/// Wraps a single `rusqlite::Connection`. Not `Send` — callers must
/// ensure single-threaded access or wrap in a `Mutex` if shared.
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open (or create) the database at the given path.
    ///
    /// Enables WAL mode, foreign keys, and runs any pending migrations.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::configure(&conn)?;
        let mut store = Self { conn };
        migrations::run(&mut store)?;
        info!("Store opened at {}", path.display());
        Ok(store)
    }

    /// Open an in-memory database (for tests).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::configure(&conn)?;
        let mut store = Self { conn };
        migrations::run(&mut store)?;
        info!("Store opened in-memory");
        Ok(store)
    }

    /// Access the raw connection (for modules that need it).
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Execute a closure inside a transaction. Commits on Ok, rolls back on Err.
    pub fn transaction<T, F>(&mut self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T>,
    {
        let tx = self.conn.transaction()?;
        let result = f(&tx);
        match result {
            Ok(val) => {
                tx.commit()?;
                Ok(val)
            }
            Err(e) => {
                // tx drops here, auto-rollback
                Err(e)
            }
        }
    }

    /// Configure connection pragmas.
    fn configure(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;
             PRAGMA synchronous = NORMAL;",
        )?;
        Ok(())
    }
}
