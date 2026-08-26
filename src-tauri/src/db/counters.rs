//! Kitchen/preparation counters — physical stations a KOT token is printed
//! for ("Channa Counter", "Tandoor", "Drinks Counter"). Owner/Admin manage
//! these from Settings; every client defines their own, since the actual
//! stations vary per business. See `schema.sql`'s `counters` table doc
//! comment for why this is a separate concept from `db::items::Category`
//! (browsing/filtering) rather than reusing or coupling to it.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Counter {
    pub id: i64,
    pub name: String,
    pub is_active: bool,
}

#[derive(Debug)]
pub enum CounterError {
    NotFound,
    EmptyName,
    DuplicateName(String),
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for CounterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CounterError::NotFound => write!(f, "Counter not found"),
            CounterError::EmptyName => write!(f, "Counter name cannot be empty"),
            CounterError::DuplicateName(name) => write!(f, "A counter named \"{}\" already exists", name),
            CounterError::Sqlite(err) => write!(f, "database error: {}", err),
        }
    }
}

impl From<rusqlite::Error> for CounterError {
    fn from(err: rusqlite::Error) -> Self {
        CounterError::Sqlite(err)
    }
}

fn from_row(row: &Row<'_>) -> Result<Counter, rusqlite::Error> {
    Ok(Counter {
        id: row.get("id")?,
        name: row.get("name")?,
        is_active: row.get::<_, i64>("is_active")? != 0,
    })
}

fn load_counter(conn: &Connection, id: i64) -> Result<Counter, CounterError> {
    conn.query_row("SELECT id, name, is_active FROM counters WHERE id = ?1", params![id], from_row)
        .optional()?
        .ok_or(CounterError::NotFound)
}

/// Every counter, alphabetically. `include_inactive` mirrors `db::items::
/// ItemQuery`'s convention — Settings' management list wants everything
/// (so a deactivated counter can be reactivated), but the token-printing
/// dialog should only ever offer an active one.
pub fn list_counters(conn: &Connection, include_inactive: bool) -> Result<Vec<Counter>, rusqlite::Error> {
    let sql = if include_inactive {
        "SELECT id, name, is_active FROM counters ORDER BY name"
    } else {
        "SELECT id, name, is_active FROM counters WHERE is_active = 1 ORDER BY name"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], from_row)?;
    rows.collect()
}

fn map_duplicate(err: rusqlite::Error, name: &str) -> CounterError {
    match err {
        rusqlite::Error::SqliteFailure(e, _) if e.extended_code == 2067 => {
            CounterError::DuplicateName(name.to_string())
        }
        other => CounterError::Sqlite(other),
    }
}

pub fn add_counter(conn: &Connection, name: &str) -> Result<Counter, CounterError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CounterError::EmptyName);
    }

    conn.execute("INSERT INTO counters (name) VALUES (?1)", params![name])
        .map_err(|e| map_duplicate(e, name))?;

    load_counter(conn, conn.last_insert_rowid())
}

/// Renames an existing counter. Full-replace, matching every other
/// add/edit pattern in this codebase — there is only one editable field
/// today (`is_active` has its own dedicated `set_counter_active`, not
/// folded into this one, since flipping it is a status action, not an edit).
pub fn update_counter(conn: &Connection, id: i64, name: &str) -> Result<Counter, CounterError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CounterError::EmptyName);
    }

    let changed = conn
        .execute("UPDATE counters SET name = ?1 WHERE id = ?2", params![name, id])
        .map_err(|e| map_duplicate(e, name))?;
    if changed == 0 {
        return Err(CounterError::NotFound);
    }
    load_counter(conn, id)
}

/// Deactivates (or reactivates) a counter — never deleted outright, since a
/// counter with token history is referenced by `tokens.counter_id` with
/// `ON DELETE RESTRICT`. An inactive counter simply stops being offered in
/// the "Print Token" dialog; its past tokens are untouched.
pub fn set_counter_active(conn: &Connection, id: i64, is_active: bool) -> Result<Counter, CounterError> {
    let changed =
        conn.execute("UPDATE counters SET is_active = ?1 WHERE id = ?2", params![is_active as i64, id])?;
    if changed == 0 {
        return Err(CounterError::NotFound);
    }
    load_counter(conn, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::test_conn;

    #[test]
    fn add_list_and_rename_a_counter() {
        let conn = test_conn();
        let created = add_counter(&conn, "  Channa Counter  ").unwrap();
        assert_eq!(created.name, "Channa Counter", "must be trimmed");
        assert!(created.is_active);

        let all = list_counters(&conn, false).unwrap();
        assert!(all.iter().any(|c| c.id == created.id));

        let renamed = update_counter(&conn, created.id, "Chana Counter").unwrap();
        assert_eq!(renamed.name, "Chana Counter");
    }

    #[test]
    fn rejects_a_blank_or_duplicate_name() {
        let conn = test_conn();
        assert!(matches!(add_counter(&conn, "   "), Err(CounterError::EmptyName)));

        add_counter(&conn, "Tandoor").unwrap();
        assert!(matches!(add_counter(&conn, "Tandoor"), Err(CounterError::DuplicateName(_))));
        assert!(matches!(add_counter(&conn, "tandoor"), Ok(_)), "SQLite TEXT uniqueness is case-sensitive by default — a differently-cased name is a distinct row, not a conflict, same as `categories`/`tables` elsewhere in this codebase");
    }

    #[test]
    fn deactivating_hides_it_from_the_active_only_list_but_not_the_full_one() {
        let conn = test_conn();
        let counter = add_counter(&conn, "Drinks Counter").unwrap();

        let deactivated = set_counter_active(&conn, counter.id, false).unwrap();
        assert!(!deactivated.is_active);

        assert!(!list_counters(&conn, false).unwrap().iter().any(|c| c.id == counter.id));
        assert!(list_counters(&conn, true).unwrap().iter().any(|c| c.id == counter.id));

        let reactivated = set_counter_active(&conn, counter.id, true).unwrap();
        assert!(reactivated.is_active);
    }

    #[test]
    fn update_and_deactivate_reject_an_unknown_id() {
        let conn = test_conn();
        assert!(matches!(update_counter(&conn, 999_999, "Ghost"), Err(CounterError::NotFound)));
        assert!(matches!(set_counter_active(&conn, 999_999, false), Err(CounterError::NotFound)));
    }
}
