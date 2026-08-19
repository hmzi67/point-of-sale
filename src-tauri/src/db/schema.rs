//! Schema application, migration and first-run seeding.
//!
//! `schema.sql` is embedded in the binary and replayed on every startup. Every
//! statement in it is `CREATE ... IF NOT EXISTS`, so a launch against an
//! existing database creates only what is missing and touches no existing row:
//! there is no DROP and no data rewrite anywhere in the path.
//!
//! The one controlled exception is `ADDED_COLUMNS` / `add_missing_columns`,
//! which runs a guarded `ALTER TABLE ... ADD COLUMN` for a column added to a
//! table after its first release (existing installs never see a new column
//! just by re-running `CREATE TABLE IF NOT EXISTS`). It only ever *adds* a
//! nullable or defaulted column, never renames or drops one.
//!
//! A future change that must genuinely *modify* an existing column (rename it,
//! tighten a constraint) cannot be expressed this way — add a numbered,
//! guarded step keyed off `user_version` for that, and bump `SCHEMA_VERSION`.

use rusqlite::{params, Connection};

use crate::db::users;

const SCHEMA_SQL: &str = include_str!("schema.sql");

/// Bumped whenever `schema.sql` gains tables. Stored in `PRAGMA user_version`.
pub const SCHEMA_VERSION: i64 = 4;

/// Every table the app expects to exist after `apply()`. Checked afterwards so
/// a typo in `schema.sql` fails loudly at startup instead of at first query.
pub const EXPECTED_TABLES: &[&str] = &[
    // Phase 1 — configuration and accounts
    "app_config",
    "modules",
    "enabled_modules",
    "users",
    // Phase 2 — operations
    "categories",
    "items",
    "tables",
    "sales",
    "sale_items",
    "table_orders",
    "employees",
    "attendance",
    "salary_payments",
    "expenses",
];

/// Default PIN for the seeded owner account. Surfaced on the login screen on
/// first run; the client is expected to change it during onboarding.
pub const DEFAULT_OWNER_NAME: &str = "Owner";
pub const DEFAULT_OWNER_PIN: &str = "1234";

/// The fixed module catalogue: (key, display name, is_core, sort_order, android default).
///
/// Billing is the only core module — the minimum viable POS — and can never be
/// switched off. Android defaults follow the product goals: billing, dashboard
/// and reports are the mobile-relevant screens; the rest are desktop tasks.
const MODULE_CATALOGUE: &[(&str, &str, bool, i64, bool)] = &[
    ("dashboard", "Dashboard", false, 1, true),
    ("billing", "Billing", true, 2, true),
    ("inventory", "Inventory", false, 3, false),
    ("reports", "Reports", false, 4, true),
    ("tables", "Tables", false, 5, false),
    ("attendance", "Attendance", false, 6, false),
    ("expenses", "Expenses", false, 7, false),
    ("salary", "Salary", false, 8, false),
];

/// User tables currently present, in name order (SQLite internals excluded).
pub fn table_names(conn: &Connection) -> Result<Vec<String>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master
          WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
          ORDER BY name",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    rows.collect()
}

/// Expected tables that do not exist yet.
pub fn missing_tables(conn: &Connection) -> Result<Vec<&'static str>, rusqlite::Error> {
    let present = table_names(conn)?;
    Ok(EXPECTED_TABLES
        .iter()
        .copied()
        .filter(|expected| !present.iter().any(|name| name == expected))
        .collect())
}

/// Brings the database up to date and seeds anything missing.
///
/// Safe to run on every launch: it reports what was missing beforehand, applies
/// the schema, verifies the result, and leaves existing rows untouched.
pub fn apply(conn: &Connection) -> Result<(), rusqlite::Error> {
    let previous_version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let missing_before = missing_tables(conn)?;

    if !missing_before.is_empty() {
        println!(
            "[db] schema v{} -> v{}: creating {} table(s): {}",
            previous_version,
            SCHEMA_VERSION,
            missing_before.len(),
            missing_before.join(", ")
        );
    }

    conn.execute_batch(SCHEMA_SQL)?;
    add_missing_columns(conn)?;

    // If this ever trips, `schema.sql` and EXPECTED_TABLES have drifted apart.
    let missing_after = missing_tables(conn)?;
    if !missing_after.is_empty() {
        return Err(rusqlite::Error::InvalidParameterName(format!(
            "schema incomplete after apply, missing: {}",
            missing_after.join(", ")
        )));
    }

    conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;

    seed_app_config(conn)?;
    seed_modules(conn)?;
    seed_owner(conn)?;
    crate::db::seed::seed_demo_data(conn)?;

    Ok(())
}

/// Columns added to an already-existing table after its initial release.
///
/// `CREATE TABLE IF NOT EXISTS` only helps a table that doesn't exist yet — an
/// install that already has `items` from v2 never sees a column added to that
/// `CREATE TABLE` in `schema.sql`. Each entry here is applied with `ALTER
/// TABLE ... ADD COLUMN`, guarded by a `PRAGMA table_info` check so it is safe
/// to run on every launch (including a fresh v3+ install, where `schema.sql`
/// already created the column and this becomes a no-op).
const ADDED_COLUMNS: &[(&str, &str, &str)] = &[
    // (table, column, column definition for ALTER TABLE ... ADD COLUMN)
    ("items", "image_path", "TEXT"),
    ("table_orders", "cart_json", "TEXT"),
];

fn add_missing_columns(conn: &Connection) -> Result<(), rusqlite::Error> {
    for (table, column, definition) in ADDED_COLUMNS {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table))?;
        let has_column = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .any(|name| name == column);

        if !has_column {
            println!("[db] adding column {}.{}", table, column);
            conn.execute_batch(&format!(
                "ALTER TABLE {} ADD COLUMN {} {}",
                table, column, definition
            ))?;
        }
    }
    Ok(())
}

fn seed_app_config(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute("INSERT OR IGNORE INTO app_config (id) VALUES (1)", [])?;
    Ok(())
}

/// Inserts any catalogue module that is not in the table yet, along with its
/// default per-platform visibility. Existing rows — and any toggles the client
/// has already made — are left untouched.
fn seed_modules(conn: &Connection) -> Result<(), rusqlite::Error> {
    for (key, name, is_core, sort_order, android_default) in MODULE_CATALOGUE {
        conn.execute(
            "INSERT INTO modules (key, name, is_core, sort_order)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (key) DO UPDATE SET name = excluded.name,
                                             is_core = excluded.is_core,
                                             sort_order = excluded.sort_order",
            params![key, name, *is_core as i64, sort_order],
        )?;

        conn.execute(
            "INSERT OR IGNORE INTO enabled_modules (module_id, desktop_enabled, android_enabled)
             SELECT id, 1, ?2 FROM modules WHERE key = ?1",
            params![key, *android_default as i64],
        )?;
    }

    // Core modules are enabled on every platform, no matter what is stored.
    conn.execute(
        "UPDATE enabled_modules
            SET desktop_enabled = 1, android_enabled = 1
          WHERE module_id IN (SELECT id FROM modules WHERE is_core = 1)",
        [],
    )?;

    Ok(())
}

/// Creates the default owner account so a fresh install is actually loginable.
fn seed_owner(conn: &Connection) -> Result<(), rusqlite::Error> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))?;
    if count > 0 {
        return Ok(());
    }

    let pin_hash = users::hash_pin(DEFAULT_OWNER_PIN)
        .map_err(|e| rusqlite::Error::InvalidParameterName(format!("pin hash failed: {}", e)))?;

    conn.execute(
        "INSERT INTO users (name, pin_hash, role) VALUES (?1, ?2, 'owner')",
        params![DEFAULT_OWNER_NAME, pin_hash],
    )?;

    Ok(())
}

#[cfg(test)]
pub(crate) fn test_conn() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory db");
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    apply(&conn).expect("apply schema");
    conn
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_the_full_module_catalogue_once() {
        let conn = test_conn();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM modules", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, MODULE_CATALOGUE.len() as i64);

        // Re-applying must not duplicate rows or reset toggles.
        conn.execute(
            "UPDATE enabled_modules SET desktop_enabled = 0
              WHERE module_id = (SELECT id FROM modules WHERE key = 'salary')",
            [],
        )
        .unwrap();
        apply(&conn).unwrap();

        let count_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM modules", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count_after, count);

        let salary_enabled: i64 = conn
            .query_row(
                "SELECT desktop_enabled FROM enabled_modules
                   WHERE module_id = (SELECT id FROM modules WHERE key = 'salary')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(salary_enabled, 0, "a client's toggle must survive restart");
    }

    #[test]
    fn every_expected_table_exists_after_apply() {
        let conn = test_conn();
        assert!(missing_tables(&conn).unwrap().is_empty());
        assert_eq!(
            conn.query_row::<i64, _, _>("PRAGMA user_version", [], |row| row.get(0))
                .unwrap(),
            SCHEMA_VERSION
        );
    }

    #[test]
    fn re_applying_never_destroys_data() {
        let conn = test_conn();
        conn.execute("INSERT INTO categories (name) VALUES ('Handmade')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO expenses (expense_date, category, amount_minor)
             VALUES ('2026-01-05', 'Rent', 3500000)",
            [],
        )
        .unwrap();
        let items_before: i64 = conn
            .query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0))
            .unwrap();

        // Simulates the next three app launches.
        for _ in 0..3 {
            apply(&conn).unwrap();
        }

        assert_eq!(
            conn.query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM categories WHERE name = 'Handmade'",
                [],
                |row| row.get(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row::<i64, _, _>("SELECT COUNT(*) FROM expenses", [], |row| row.get(0))
                .unwrap(),
            super::super::seed::demo_expense_count() + 1
        );
        // Demo data is seeded once, not on every launch.
        assert_eq!(
            conn.query_row::<i64, _, _>("SELECT COUNT(*) FROM items", [], |row| row.get(0))
                .unwrap(),
            items_before
        );
    }

    #[test]
    fn sale_lines_cascade_but_sold_items_cannot_be_deleted() {
        let conn = test_conn();
        let (sale_id, item_id): (i64, i64) = conn
            .query_row(
                "SELECT sale_id, item_id FROM sale_items LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        // An item with sales history must not be deletable.
        assert!(conn
            .execute("DELETE FROM items WHERE id = ?1", [item_id])
            .is_err());

        // Deleting a sale takes its lines with it.
        conn.execute("DELETE FROM sales WHERE id = ?1", [sale_id])
            .unwrap();
        let orphans: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sale_items WHERE sale_id = ?1",
                [sale_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(orphans, 0);
    }

    #[test]
    fn removing_an_employee_clears_their_attendance_and_payslips() {
        let conn = test_conn();
        let employee_id: i64 = conn
            .query_row("SELECT id FROM employees LIMIT 1", [], |row| row.get(0))
            .unwrap();

        conn.execute("DELETE FROM employees WHERE id = ?1", [employee_id])
            .unwrap();

        for table in ["attendance", "salary_payments"] {
            let remaining: i64 = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM {} WHERE employee_id = ?1", table),
                    [employee_id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(remaining, 0, "{} should have been cascaded", table);
        }
    }

    #[test]
    fn hot_path_queries_are_index_backed() {
        let conn = test_conn();
        // If these ever fall back to SCAN, billing search and report date
        // filters degrade on a shop with a year of history.
        for query in [
            "EXPLAIN QUERY PLAN SELECT id FROM items WHERE barcode = '8901234500011'",
            "EXPLAIN QUERY PLAN SELECT id FROM sales WHERE created_at >= '2026-01-01'",
            "EXPLAIN QUERY PLAN SELECT id FROM attendance WHERE employee_id = 1 AND work_date = '2026-01-01'",
            "EXPLAIN QUERY PLAN SELECT id FROM expenses WHERE expense_date >= '2026-01-01'",
        ] {
            let plan: String = conn.query_row(query, [], |row| row.get(3)).unwrap();
            assert!(
                plan.contains("USING") && plan.contains("INDEX") && !plan.contains("SCAN "),
                "expected an index lookup, got: {}\nfor: {}",
                plan,
                query
            );
        }
    }

    #[test]
    fn adds_image_path_to_an_existing_items_table_without_losing_rows() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();

        // A hand-built stand-in for a v2 `items` table — no `image_path`
        // column — to simulate an install that predates this migration.
        conn.execute_batch(
            "CREATE TABLE items (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 name TEXT NOT NULL,
                 barcode TEXT UNIQUE,
                 price_minor INTEGER NOT NULL DEFAULT 0,
                 cost_minor INTEGER NOT NULL DEFAULT 0,
                 stock_qty INTEGER NOT NULL DEFAULT 0,
                 category_id INTEGER,
                 low_stock_threshold INTEGER NOT NULL DEFAULT 0,
                 is_active INTEGER NOT NULL DEFAULT 1,
                 created_at TEXT NOT NULL DEFAULT (datetime('now')),
                 updated_at TEXT NOT NULL DEFAULT (datetime('now'))
             );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO items (name, price_minor) VALUES ('Pre-existing Item', 500)",
            [],
        )
        .unwrap();

        apply(&conn).unwrap();

        let has_column = conn
            .prepare("PRAGMA table_info(items)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .iter()
            .any(|name| name == "image_path");
        assert!(has_column, "image_path column should have been added");

        let (name, image_path): (String, Option<String>) = conn
            .query_row(
                "SELECT name, image_path FROM items WHERE name = 'Pre-existing Item'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(name, "Pre-existing Item", "pre-existing row must survive the migration");
        assert_eq!(image_path, None);

        // Re-applying (the next app launch) must not error on an already-added column.
        apply(&conn).unwrap();
    }

    #[test]
    fn seeds_exactly_one_config_row_and_one_owner() {
        let conn = test_conn();
        apply(&conn).unwrap();

        let configs: i64 = conn
            .query_row("SELECT COUNT(*) FROM app_config", [], |row| row.get(0))
            .unwrap();
        assert_eq!(configs, 1);

        let users: i64 = conn
            .query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
            .unwrap();
        assert_eq!(users, 1);
    }
}
