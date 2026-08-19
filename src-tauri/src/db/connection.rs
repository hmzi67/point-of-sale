//! Connection handling for the local SQLite database.
//!
//! The database file lives in the app's data directory so it survives
//! reinstalls and stays per-installation (each client runs fully offline).

use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;

pub const DB_FILE_NAME: &str = "pos.db";

#[derive(Debug)]
pub enum DbError {
    Sqlite(rusqlite::Error),
    Io(String),
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::Sqlite(err) => write!(f, "database error: {}", err),
            DbError::Io(msg) => write!(f, "storage error: {}", msg),
        }
    }
}

impl std::error::Error for DbError {}

impl From<rusqlite::Error> for DbError {
    fn from(err: rusqlite::Error) -> Self {
        DbError::Sqlite(err)
    }
}

/// Managed application state holding the single SQLite connection.
///
/// A single guarded connection is enough for a one-till desktop app and keeps
/// write ordering simple; if a later phase needs concurrency, swap the mutex
/// for a small pool without changing the command signatures.
pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    /// Opens (creating if needed) the database at `dir/pos.db`.
    pub fn open(dir: PathBuf) -> Result<Self, DbError> {
        std::fs::create_dir_all(&dir).map_err(|e| DbError::Io(e.to_string()))?;
        let conn = Connection::open(dir.join(DB_FILE_NAME))?;

        // WAL keeps reads fast while a sale is being written, and the foreign
        // key enforcement matters once Phase 2 lands the relational schema.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;

        // Idempotent: creates anything missing and seeds the module catalogue.
        crate::db::schema::apply(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Runs `f` against the connection.
    pub fn with_conn<T, F>(&self, f: F) -> Result<T, DbError>
    where
        F: FnOnce(&Connection) -> Result<T, rusqlite::Error>,
    {
        let conn = self
            .conn
            .lock()
            .map_err(|_| DbError::Io("database lock poisoned".into()))?;
        f(&conn).map_err(DbError::from)
    }

    /// Runs `f` inside a SQLite transaction: committed if `f` returns `Ok`,
    /// rolled back (via `Transaction`'s drop) on `Err` or on panic unwind. Use
    /// this for any write that touches more than one table — a sale plus its
    /// line items plus the stock decrement must land together or not at all.
    pub fn with_transaction<T, F>(&self, f: F) -> Result<T, DbError>
    where
        F: FnOnce(&rusqlite::Transaction) -> Result<T, rusqlite::Error>,
    {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| DbError::Io("database lock poisoned".into()))?;
        let tx = conn.transaction()?;
        let result = f(&tx)?;
        tx.commit()?;
        Ok(result)
    }
}
