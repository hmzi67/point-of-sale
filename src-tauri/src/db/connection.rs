//! Connection handling for the local SQLite database.
//!
//! The database file lives in the app's data directory so it survives
//! reinstalls and stays per-installation (each client runs fully offline).

use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;

pub const DB_FILE_NAME: &str = "pos.db";

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("storage error: {0}")]
    Io(String),
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

    /// Goes through the real `Db::open` (file on disk, pragmas, migrations),
    /// not `schema::test_conn`, which sets foreign_keys itself.
    #[test]
    fn open_enables_foreign_keys_and_wal_on_a_real_file() {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("diwan-open-test-{}-{nanos}", std::process::id()));

        let db = Db::open(dir.clone()).unwrap();
        let (foreign_keys, journal_mode): (i64, String) = db
            .with_conn(|conn| {
                Ok((
                    conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?,
                    conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))?,
                ))
            })
            .unwrap();
        assert_eq!(foreign_keys, 1);
        assert_eq!(journal_mode, "wal");

        // Enforced, not just reported: an order line for a missing order is refused.
        let orphan = db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO order_items (order_id, item_id, qty, price_at_order_minor)
                 VALUES (999999, (SELECT id FROM items LIMIT 1), 1, 100)",
                [],
            )
        });
        assert!(orphan.is_err());

        drop(db);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Migrates a COPY of a real install's database in place:
    /// `DIWAN_MIGRATE_DIR=/path/to/dir-with-pos.db cargo test migrate_a_real_db_copy -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn migrate_a_real_db_copy() {
        let dir = PathBuf::from(std::env::var("DIWAN_MIGRATE_DIR").expect("set DIWAN_MIGRATE_DIR"));
        let tables = ["sales", "sale_items", "tokens", "token_items", "table_orders", "refunds"];
        let counts = |conn: &Connection| -> Vec<i64> {
            tables
                .iter()
                .map(|t| conn.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |row| row.get(0)).unwrap())
                .collect()
        };

        let before = {
            let conn = Connection::open(dir.join(DB_FILE_NAME)).unwrap();
            let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0)).unwrap();
            println!("user_version before: {version}");
            counts(&conn)
        };
        let db = Db::open(dir).unwrap();
        let (after, version, fk_problems) = db
            .with_conn(|conn| {
                let fk_problems = conn.prepare("PRAGMA foreign_key_check")?.query_map([], |_| Ok(()))?.count();
                Ok((counts(conn), conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?, fk_problems))
            })
            .unwrap();
        for ((table, b), a) in tables.iter().zip(&before).zip(&after) {
            println!("{table:>12}: {b} -> {a}");
        }
        println!("user_version after: {version}, foreign_key_check violations: {fk_problems}");
        assert_eq!(before, after);
        assert_eq!(version, crate::db::schema::SCHEMA_VERSION);
        assert_eq!(fk_problems, 0);
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
