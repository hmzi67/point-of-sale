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
//!
//! One narrow exception that looks like a modification but isn't: `items.
//! stock_qty`, `sale_items.qty` and `refund_items.qty_refunded` changed
//! their *declared* type from INTEGER to REAL (for `sold_by_amount`
//! items' fractional quantities) with no guarded migration step. This is
//! safe because SQLite's columns are dynamically typed — an existing
//! INTEGER-affinity column already stores a genuine fractional value as
//! REAL rather than truncating it (verified directly: `INSERT INTO t
//! (qty) VALUES (0.4)` into an `INTEGER`-declared column round-trips as
//! `0.4`, not `0`). So an existing database's already-created columns keep
//! working exactly the same regardless of what `schema.sql`'s text
//! declares now; the type-declaration edit only changes what a *fresh*
//! install's `CREATE TABLE` says, which is documentation, not behavior.

use rusqlite::{params, Connection};

use crate::db::users;

const SCHEMA_SQL: &str = include_str!("schema.sql");

/// Bumped whenever `schema.sql` gains tables. Stored in `PRAGMA user_version`.
pub const SCHEMA_VERSION: i64 = 10;

/// Every table the app expects to exist after `apply()`. Checked afterwards so
/// a typo in `schema.sql` fails loudly at startup instead of at first query.
pub const EXPECTED_TABLES: &[&str] = &[
    // Phase 1 — configuration and accounts
    "app_config",
    "modules",
    "enabled_modules",
    "users",
    "product_owner_account",
    // Phase 2 — operations
    "categories",
    "counters",
    "items",
    "tables",
    "sales",
    "sale_items",
    "refunds",
    "refund_items",
    "shifts",
    "table_orders",
    "tokens",
    "token_items",
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
    // Cash-drawer reconciliation is genuinely optional — a small
    // single-cashier shop may never want the "declare your cash count"
    // step, so this is toggleable like everything else non-core, not
    // wired on unconditionally. Gates the standalone shift-history page
    // (Owner/Admin, like Reports) — the inline Open/Close Shift controls in
    // Billing check this same flag directly, the same way `tables` gates
    // its inline billing-screen UI without needing its own role tier.
    ("shifts", "Shifts", false, 9, false),
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
    crate::startup_log::log(&format!(
        "db: schema currently at v{previous_version} (target v{SCHEMA_VERSION}), {} table(s) missing before apply",
        missing_before.len()
    ));

    if !missing_before.is_empty() {
        println!(
            "[db] schema v{} -> v{}: creating {} table(s): {}",
            previous_version,
            SCHEMA_VERSION,
            missing_before.len(),
            missing_before.join(", ")
        );
    }

    relax_tokens_table_order_id(conn)?;
    crate::startup_log::log("db: v9->v10 tokens.table_order_id nullability migration checked");

    conn.execute_batch(SCHEMA_SQL)?;
    crate::startup_log::log("db: schema.sql applied (CREATE TABLE/INDEX IF NOT EXISTS)");
    add_missing_columns(conn)?;
    crate::startup_log::log("db: added-columns migration applied");

    // If this ever trips, `schema.sql` and EXPECTED_TABLES have drifted apart.
    let missing_after = missing_tables(conn)?;
    if !missing_after.is_empty() {
        crate::startup_log::log(&format!(
            "db: FATAL — schema still incomplete after apply, missing: {}",
            missing_after.join(", ")
        ));
        return Err(rusqlite::Error::InvalidParameterName(format!(
            "schema incomplete after apply, missing: {}",
            missing_after.join(", ")
        )));
    }

    conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    crate::startup_log::log(&format!("db: schema now at v{SCHEMA_VERSION}"));

    seed_app_config(conn)?;
    seed_modules(conn)?;
    seed_owner(conn)?;
    crate::startup_log::log("db: app_config/modules/owner seeded (or already present)");
    crate::db::seed::seed_demo_data(conn)?;
    crate::startup_log::log("db: apply() complete");

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
    ("app_config", "working_days_per_month", "INTEGER NOT NULL DEFAULT 26"),
    // Deliberately DEFAULT 1 here, unlike `schema.sql`'s fresh-install
    // DEFAULT 0 above it: an install upgrading onto this column already has
    // an `app_config` row from before the wizard existed, so it's already
    // "configured" by definition — only a genuinely brand-new database (no
    // pre-existing row at all) should be routed through onboarding.
    ("app_config", "onboarding_completed", "INTEGER NOT NULL DEFAULT 1"),
    ("items", "description", "TEXT"),
    ("sale_items", "notes", "TEXT"),
    // Present directly in schema.sql's `sales` CREATE TABLE for a fresh
    // install; an install upgrading from before shifts existed needs this
    // ALTER instead. References `shifts`, which by this point always
    // exists — `apply()` runs `execute_batch(SCHEMA_SQL)` (creating
    // `shifts`) before calling this function.
    ("sales", "shift_id", "INTEGER REFERENCES shifts (id) ON DELETE SET NULL"),
    // Product-owner module locks — see schema.sql's comment on
    // `enabled_modules` for what these mean.
    ("enabled_modules", "desktop_locked", "INTEGER NOT NULL DEFAULT 0"),
    ("enabled_modules", "android_locked", "INTEGER NOT NULL DEFAULT 0"),
    // Printer selection (Settings' "Select printer" step) — see
    // `db::config::AppConfig`'s doc comments and `printer::escpos::send_to_printer`.
    // All NULL until a cashier/owner has actually chosen one, which is
    // deliberately indistinguishable from "not configured yet".
    ("app_config", "printer_connection_type", "TEXT"),
    ("app_config", "printer_bluetooth_address", "TEXT"),
    ("app_config", "printer_bluetooth_name", "TEXT"),
    // Windows printer selection (Print Spooler, by installed printer name)
    // — see `db::config::AppConfig::printer_windows_name` and
    // `printer::windows_spool`. Same "NULL until chosen" convention as the
    // Bluetooth fields above.
    ("app_config", "printer_windows_name", "TEXT"),
    // Business contact phone — shown in Settings and printed on receipts.
    // NULL until an owner/admin sets one via Settings' Store Identity
    // section; see `db::config::AppConfig::phone`.
    ("app_config", "phone", "TEXT"),
    // Delivery/dispatch contact number — shown in Settings and printed on
    // receipts only when set. NULL until an owner/admin sets one; see
    // `db::config::AppConfig::delivery_number`.
    ("app_config", "delivery_number", "TEXT"),
    // "Sold by amount" (loose/weighed items) — see `db::items::Item::
    // sold_by_amount`'s doc comment and `schema.sql`'s `items` table for
    // the full reasoning, including why `stock_qty`/`qty`/`qty_refunded`
    // changing declared type to REAL elsewhere in this file needs no
    // migration entry here (SQLite already stores fractional values in
    // those existing INTEGER-affinity columns without one).
    ("items", "sold_by_amount", "INTEGER NOT NULL DEFAULT 0"),
    ("items", "unit", "TEXT"),
    // Which kitchen/prep counter (if any) this item's KOT token belongs on
    // — NULL is meaningful, not "not set yet": an item with no counter
    // never appears on any token at all (e.g. roti, if a client's kitchen
    // workflow doesn't need one for it). See `schema.sql`'s `counters`
    // table doc comment for why this is a separate concept from
    // `category_id`.
    ("items", "counter_id", "INTEGER REFERENCES counters (id) ON DELETE SET NULL"),
];

/// An index on a column that only exists after `add_missing_columns` runs
/// (i.e. an `ADDED_COLUMNS` column) can't be declared as a plain `CREATE
/// INDEX` in `schema.sql`: `execute_batch(SCHEMA_SQL)` runs *before*
/// `add_missing_columns`, so on an install upgrading from before that
/// column existed, the index statement would fail with "no such column" —
/// on a fresh install the column is already there (it's in the literal
/// `CREATE TABLE`), so this trap only bites upgrades, not new installs.
/// Applied here, after `add_missing_columns`, so the column always exists
/// by the time each of these runs, on both paths.
const ADDED_COLUMN_INDEXES: &[(&str, &str, &str)] = &[
    // (index name, table, column)
    ("idx_sales_shift", "sales", "shift_id"),
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
            crate::startup_log::log(&format!("db: adding column {table}.{column}"));
            conn.execute_batch(&format!(
                "ALTER TABLE {} ADD COLUMN {} {}",
                table, column, definition
            ))?;
        }
    }

    for (index_name, table, column) in ADDED_COLUMN_INDEXES {
        conn.execute_batch(&format!(
            "CREATE INDEX IF NOT EXISTS {} ON {} ({})",
            index_name, table, column
        ))?;
    }

    Ok(())
}

/// v9 -> v10: `tokens.table_order_id` goes from `NOT NULL` to nullable, so a
/// Takeaway sale (which has no `table_orders` row at all) can still record
/// an ad hoc token — see `schema.sql`'s comment on that column, and `db::
/// tokens`'s module doc comment for why. SQLite can't `ALTER ... DROP NOT
/// NULL` in place, so this rebuilds the table: creates it fresh under a
/// temporary name (with the new, nullable definition), copies every
/// existing row across unchanged, drops the old table, then renames the new
/// one into place.
///
/// Deliberately does *not* `ALTER TABLE tokens RENAME` (the more obvious
/// order) — SQLite auto-rewrites any other table's foreign key clause that
/// references a table being renamed (here, `token_items.token_id`'s FK
/// would silently start pointing at the renamed-away copy), which would
/// leave `token_items` referencing a table this migration is about to drop.
/// Building the replacement under its own name and renaming *that* into the
/// vacated `tokens` name instead avoids ever renaming the live, referenced
/// table, so `token_items`'s FK clause (which has always just said
/// `REFERENCES tokens`) keeps meaning the right table throughout.
///
/// A no-op on a fresh install (no `tokens` table exists yet — `schema.sql`
/// creates the nullable shape directly) and a no-op if `tokens` already has
/// the nullable shape (idempotent, safe to check on every launch).
fn relax_tokens_table_order_id(conn: &Connection) -> Result<(), rusqlite::Error> {
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'tokens'",
        [],
        |row| row.get(0),
    )?;
    if exists == 0 {
        return Ok(());
    }

    let not_null: i64 = conn.query_row(
        "SELECT \"notnull\" FROM pragma_table_info('tokens') WHERE name = 'table_order_id'",
        [],
        |row| row.get(0),
    )?;
    if not_null == 0 {
        return Ok(()); // already migrated
    }

    conn.pragma_update(None, "foreign_keys", "OFF")?;
    let result = conn.execute_batch(
        "CREATE TABLE tokens_v10 (
             id             INTEGER PRIMARY KEY AUTOINCREMENT,
             token_number   INTEGER NOT NULL,
             table_order_id INTEGER REFERENCES table_orders (id) ON DELETE CASCADE,
             counter_id     INTEGER NOT NULL REFERENCES counters (id) ON DELETE RESTRICT,
             printed_at     TEXT    NOT NULL DEFAULT (datetime('now', 'localtime')),
             printed_by     INTEGER REFERENCES users (id) ON DELETE SET NULL,
             status         TEXT    NOT NULL DEFAULT 'printed' CHECK (status IN ('printed', 'cancelled'))
         );
         INSERT INTO tokens_v10 (id, token_number, table_order_id, counter_id, printed_at, printed_by, status)
             SELECT id, token_number, table_order_id, counter_id, printed_at, printed_by, status FROM tokens;
         DROP TABLE tokens;
         ALTER TABLE tokens_v10 RENAME TO tokens;",
    );
    // Restore enforcement before propagating either outcome — never leave
    // the connection with foreign keys silently off for the rest of the run.
    conn.pragma_update(None, "foreign_keys", "ON")?;
    result
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
    fn relax_tokens_table_order_id_upgrades_a_v9_install_and_preserves_existing_rows() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        // Minimal v9-shaped schema: just enough for `tokens`'s own FKs to
        // resolve, plus a seeded row (with a `token_items` child row) to
        // prove the migration preserves existing data, not just the shape.
        conn.execute_batch(
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);
             CREATE TABLE tables (id INTEGER PRIMARY KEY, name TEXT);
             CREATE TABLE table_orders (id INTEGER PRIMARY KEY, table_id INTEGER);
             CREATE TABLE counters (id INTEGER PRIMARY KEY, name TEXT);
             CREATE TABLE tokens (
                 id             INTEGER PRIMARY KEY AUTOINCREMENT,
                 token_number   INTEGER NOT NULL,
                 table_order_id INTEGER NOT NULL REFERENCES table_orders (id) ON DELETE CASCADE,
                 counter_id     INTEGER NOT NULL REFERENCES counters (id) ON DELETE RESTRICT,
                 printed_at     TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
                 printed_by     INTEGER,
                 status         TEXT NOT NULL DEFAULT 'printed'
             );
             CREATE TABLE token_items (
                 id           INTEGER PRIMARY KEY AUTOINCREMENT,
                 token_id     INTEGER NOT NULL REFERENCES tokens (id) ON DELETE CASCADE,
                 item_id      INTEGER NOT NULL,
                 qty_on_token REAL NOT NULL
             );
             INSERT INTO counters (id, name) VALUES (1, 'Kitchen');
             INSERT INTO tables (id, name) VALUES (1, 'Table 1');
             INSERT INTO table_orders (id, table_id) VALUES (1, 1);
             INSERT INTO tokens (id, token_number, table_order_id, counter_id) VALUES (1, 5, 1, 1);
             INSERT INTO token_items (id, token_id, item_id, qty_on_token) VALUES (1, 1, 1, 2.0);",
        )
        .unwrap();

        relax_tokens_table_order_id(&conn).unwrap();

        // The column must now accept NULL — this is the whole point.
        conn.execute("INSERT INTO tokens (id, token_number, table_order_id, counter_id) VALUES (2, 6, NULL, 1)", [])
            .expect("table_order_id must now be nullable");

        // The pre-existing row must survive with its data untouched.
        let (token_number, table_order_id): (i64, Option<i64>) = conn
            .query_row("SELECT token_number, table_order_id FROM tokens WHERE id = 1", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(token_number, 5);
        assert_eq!(table_order_id, Some(1));

        // `token_items`'s foreign key must still resolve to the rebuilt
        // `tokens` table, not a dangling reference to a renamed-away copy.
        let item_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM token_items WHERE token_id = 1", [], |row| row.get(0)).unwrap();
        assert_eq!(item_count, 1, "token_items must still resolve to the migrated tokens table");

        // Idempotent — a second run (e.g. a second app launch) must be a no-op.
        relax_tokens_table_order_id(&conn).unwrap();
    }

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

    /// Mirrors `adds_image_path_to_an_existing_items_table_without_losing_rows`
    /// above, but for a column whose ALTER references a table (`shifts`)
    /// that also has to be freshly created by this same `apply()` call —
    /// the real-world case, since every existing install predates `shifts`.
    #[test]
    fn adds_shift_id_to_an_existing_sales_table_and_indexes_it() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();

        // A hand-built stand-in for a pre-shifts `sales` table — no
        // `shift_id` column, and none of the tables it would reference.
        conn.execute_batch(
            "CREATE TABLE users (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE,
                 pin_hash TEXT NOT NULL, role TEXT NOT NULL, is_active INTEGER NOT NULL DEFAULT 1,
                 created_at TEXT NOT NULL DEFAULT (datetime('now')));
             CREATE TABLE sales (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 total_minor INTEGER NOT NULL DEFAULT 0,
                 discount_minor INTEGER NOT NULL DEFAULT 0,
                 tax_minor INTEGER NOT NULL DEFAULT 0,
                 payment_method TEXT NOT NULL DEFAULT 'cash',
                 cashier_id INTEGER,
                 table_id INTEGER,
                 created_at TEXT NOT NULL DEFAULT (datetime('now'))
             );",
        )
        .unwrap();
        conn.execute("INSERT INTO sales (id, total_minor) VALUES (1, 500)", [])
            .unwrap();

        apply(&conn).unwrap();

        let has_column = conn
            .prepare("PRAGMA table_info(sales)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .iter()
            .any(|name| name == "shift_id");
        assert!(has_column, "shift_id column should have been added");

        let (total_minor, shift_id): (i64, Option<i64>) = conn
            .query_row("SELECT total_minor, shift_id FROM sales WHERE id = 1", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(total_minor, 500, "pre-existing sale must survive the migration");
        assert_eq!(shift_id, None);

        let plan: String = conn
            .query_row(
                "EXPLAIN QUERY PLAN SELECT id FROM sales WHERE shift_id = 1",
                [],
                |row| row.get(3),
            )
            .unwrap();
        assert!(plan.contains("USING") && plan.contains("INDEX"), "shift_id must be indexed: {}", plan);

        // Re-applying (the next app launch) must not error on an already-added column/index.
        apply(&conn).unwrap();
    }

    /// The other `ADDED_COLUMNS` case that needs its own test: an install
    /// that predates the product-owner lock feature already has an
    /// `enabled_modules` row per module — those rows, and any toggle the
    /// client already made, must survive with the new columns defaulting
    /// to "unlocked".
    #[test]
    fn adds_lock_columns_to_an_existing_enabled_modules_table_without_losing_toggles() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();

        conn.execute_batch(
            "CREATE TABLE modules (id INTEGER PRIMARY KEY AUTOINCREMENT, key TEXT NOT NULL UNIQUE,
                 name TEXT NOT NULL, is_core INTEGER NOT NULL DEFAULT 0, sort_order INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE enabled_modules (
                 module_id INTEGER PRIMARY KEY REFERENCES modules (id) ON DELETE CASCADE,
                 desktop_enabled INTEGER NOT NULL DEFAULT 1,
                 android_enabled INTEGER NOT NULL DEFAULT 0
             );
             INSERT INTO modules (key, name) VALUES ('expenses', 'Expenses');
             INSERT INTO enabled_modules (module_id, desktop_enabled, android_enabled) VALUES (1, 0, 0);",
        )
        .unwrap();

        apply(&conn).unwrap();

        let has_columns = conn
            .prepare("PRAGMA table_info(enabled_modules)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(has_columns.iter().any(|c| c == "desktop_locked"));
        assert!(has_columns.iter().any(|c| c == "android_locked"));

        let (desktop_enabled, desktop_locked, android_locked): (i64, i64, i64) = conn
            .query_row(
                "SELECT desktop_enabled, desktop_locked, android_locked FROM enabled_modules WHERE module_id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(desktop_enabled, 0, "the client's existing toggle must survive the migration");
        assert_eq!(desktop_locked, 0, "an upgraded install must default to unlocked, not locked");
        assert_eq!(android_locked, 0);

        apply(&conn).unwrap(); // must not error re-applying
    }

    /// An install that predates the onboarding-wizard column already has a
    /// row in `app_config` — that row must come out of the migration marked
    /// `onboarding_completed = 1`, not `0`, or every existing client would
    /// be dropped into the first-time setup wizard on their next launch.
    /// Contrast with `seeds_exactly_one_config_row_and_one_owner` below,
    /// which covers the genuinely-fresh-install case (`0`, wizard runs).
    #[test]
    fn upgrading_an_existing_install_marks_onboarding_already_complete() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();

        // A hand-built stand-in for a pre-Phase-14 `app_config` — has
        // `working_days_per_month` (an earlier migration) but not
        // `onboarding_completed`.
        conn.execute_batch(
            "CREATE TABLE app_config (
                 id INTEGER PRIMARY KEY CHECK (id = 1),
                 business_name TEXT NOT NULL DEFAULT 'My Business',
                 business_type TEXT NOT NULL DEFAULT 'retail',
                 logo_path TEXT,
                 currency TEXT NOT NULL DEFAULT 'PKR',
                 tax_percent REAL NOT NULL DEFAULT 0,
                 receipt_footer TEXT NOT NULL DEFAULT 'Thank you for your purchase!',
                 working_days_per_month INTEGER NOT NULL DEFAULT 26
             );",
        )
        .unwrap();
        conn.execute("INSERT INTO app_config (id, business_name) VALUES (1, 'Existing Shop')", [])
            .unwrap();

        apply(&conn).unwrap();

        let (business_name, onboarding_completed): (String, i64) = conn
            .query_row("SELECT business_name, onboarding_completed FROM app_config WHERE id = 1", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(business_name, "Existing Shop", "the shop's real config must survive the migration");
        assert_eq!(onboarding_completed, 1, "an upgraded pre-existing install must not be routed through setup");
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
