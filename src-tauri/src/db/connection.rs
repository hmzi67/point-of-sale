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
    /// Opens (creating if needed) the database at `dir/pos.db`. Every step
    /// logged (see `startup_log`'s doc comment) — this is the single most
    /// likely place a real client install ever fails at startup (a locked
    /// file, a permissions issue writing to `dir`, a migration that doesn't
    /// like whatever state an existing `pos.db` is actually in), so it's
    /// exactly where a support call needs the log to be most detailed.
    pub fn open(dir: PathBuf) -> Result<Self, DbError> {
        crate::startup_log::log("db: opening connection");
        crate::startup_log::debug(&format!("db: database file is {}", dir.join(DB_FILE_NAME).display()));
        std::fs::create_dir_all(&dir).map_err(|e| DbError::Io(e.to_string()))?;
        let conn = Connection::open(dir.join(DB_FILE_NAME))?;
        crate::startup_log::log("db: connection opened");

        // WAL keeps reads fast while a sale is being written, and the foreign
        // key enforcement matters once Phase 2 lands the relational schema.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;

        // Idempotent: creates anything missing and seeds the module catalogue.
        crate::startup_log::log("db: migrations starting");
        crate::db::schema::apply(&conn)?;
        crate::startup_log::log("db: migrations complete");

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

    /// Like `with_transaction`, for a domain function with its own error
    /// type: commits only when `f` returns `Ok`, and rolls back on a domain
    /// `Err` too. (Wrapping such a call as `with_transaction(|tx|
    /// Ok(domain_fn(tx)))` would *commit* whatever it wrote before failing,
    /// since the closure itself still returns `Ok`.) The outer error is the
    /// lock/begin/commit failing; the inner one is `f`'s own.
    pub fn try_transaction<T, E, F>(&self, f: F) -> Result<Result<T, E>, DbError>
    where
        F: FnOnce(&rusqlite::Transaction) -> Result<T, E>,
    {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| DbError::Io("database lock poisoned".into()))?;
        let tx = conn.transaction()?;
        match f(&tx) {
            Ok(value) => {
                tx.commit()?;
                Ok(Ok(value))
            }
            // Dropping `tx` uncommitted rolls it back.
            Err(error) => Ok(Err(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Db {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE t (v INTEGER NOT NULL)").unwrap();
        Db { conn: Mutex::new(conn) }
    }

    fn count(db: &Db) -> i64 {
        db.with_conn(|conn| conn.query_row("SELECT COUNT(*) FROM t", [], |row| row.get(0))).unwrap()
    }

    #[test]
    fn with_transaction_rolls_back_when_the_closure_fails() {
        let db = test_db();
        let result: Result<(), DbError> = db.with_transaction(|tx| {
            tx.execute("INSERT INTO t (v) VALUES (1)", [])?;
            tx.execute("INSERT INTO t (v) VALUES (NULL)", [])?; // NOT NULL violation
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(count(&db), 0);
    }

    #[test]
    fn try_transaction_commits_on_ok() {
        let db = test_db();
        let result = db.try_transaction(|tx| tx.execute("INSERT INTO t (v) VALUES (1)", []).map(|_| ()));
        assert!(matches!(result, Ok(Ok(()))));
        assert_eq!(count(&db), 1);
    }

    /// The case `with_transaction(|tx| Ok(domain_fn(tx)))` got wrong: a
    /// domain error after a write must not commit that write.
    #[test]
    fn try_transaction_rolls_back_a_domain_error_after_a_write() {
        let db = test_db();
        let result: Result<Result<(), String>, DbError> = db.try_transaction(|tx| {
            tx.execute("INSERT INTO t (v) VALUES (1)", []).unwrap();
            Err("validation failed after the insert".to_string())
        });
        assert!(matches!(result, Ok(Err(_))));
        assert_eq!(count(&db), 0);
    }
}
