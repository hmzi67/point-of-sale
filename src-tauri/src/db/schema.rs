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

use crate::db::modules::key;
use crate::db::users;

const SCHEMA_SQL: &str = include_str!("schema.sql");

/// Bumped whenever `schema.sql` gains tables. Stored in `PRAGMA user_version`.
pub const SCHEMA_VERSION: i64 = 13;

/// Every table the app expects to exist after `apply()`. Checked afterwards so
/// a typo in `schema.sql` fails loudly at startup instead of at first query.
pub const EXPECTED_TABLES: &[&str] = &[
    // Phase 1 — configuration and accounts
    "app_config",
    "modules",
    "enabled_modules",
    "users",
    "product_owner_account",
    "license_status",
    "pos_sync_meta",
    "sync_state",
    "sync_outbox",
    "sync_parked",
    "sync_issues",
    "sync_central_stock",
    "sync_reservations",
    "sync_reservation_lines",
    "sync_terminals",
    "sync_staging",
    "sync_staging_stock",
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
    "orders",
    "order_items",
    "order_status_events",
    "employees",
    "attendance",
    "salary_payments",
    "expenses",
    "delivery_platforms",
    // v13 — customers/credit, purchasing, stock control
    "customers",
    "sale_payments",
    "customer_payments",
    "suppliers",
    "purchase_orders",
    "purchase_order_items",
    "goods_receipts",
    "goods_receipt_items",
    "supplier_payments",
    "stock_movements",
    "stock_takes",
    "stock_take_lines",
];

/// Default PIN for the seeded owner account. Surfaced on the login screen on
/// first run; the client is expected to change it during onboarding.
pub const DEFAULT_OWNER_NAME: &str = "Owner";
pub const DEFAULT_OWNER_PIN: &str = "1234";

/// Default value for the shared "sensitive area" PIN (see
/// `db::security_pin`) — gates Attendance, Expenses, Salary, Employees and
/// Reports for every role. Deliberately different from `DEFAULT_OWNER_PIN`
/// so the two are never confused; an owner/admin is expected to change it
/// from Settings once the client is live, same as the owner login PIN.
pub const DEFAULT_AREA_PIN: &str = "0000";

/// The fixed module catalogue: (key, display name, is_core, sort_order, desktop default, android default).
///
/// Billing is the only core module — the minimum viable POS — and can never be
/// switched off. Android defaults follow the product goals: billing, dashboard
/// and reports are the mobile-relevant screens; the rest are desktop tasks.
/// Delivery is disabled by default across all platforms.
const MODULE_CATALOGUE: &[(&str, &str, bool, i64, bool, bool)] = &[
    (key::DASHBOARD, "Dashboard", false, 1, true, true),
    (key::BILLING, "Billing", true, 2, true, true),
    (key::INVENTORY, "Inventory", false, 3, true, false),
    (key::REPORTS, "Reports", false, 4, true, true),
    (key::TABLES, "Tables", false, 5, true, false),
    (key::ATTENDANCE, "Attendance", false, 6, true, false),
    (key::EXPENSES, "Expenses", false, 7, true, false),
    (key::SALARY, "Salary", false, 8, true, false),
    // Cash-drawer reconciliation is genuinely optional — a small
    // single-cashier shop may never want the "declare your cash count"
    // step, so this is toggleable like everything else non-core, not
    // wired on unconditionally. Gates the standalone shift-history page
    // (Owner/Admin, like Reports) — the inline Open/Close Shift controls in
    // Billing check this same flag directly, the same way `tables` gates
    // its inline billing-screen UI without needing its own role tier.
    (key::SHIFTS, "Shifts", false, 9, true, false),
    (key::EMPLOYEES, "Employees", false, 10, true, true),
    (key::USERS, "Users", false, 11, true, true),
    (key::SETTINGS, "Settings", false, 12, true, true),
    (key::DELIVERY, "Delivery", false, 13, false, false),
    // Customer accounts, pay-later sales and installments.
    (key::CUSTOMERS, "Customers", false, 14, true, false),
    // Suppliers, purchase orders and goods received notes (GRN).
    (key::PURCHASING, "Purchasing", false, 15, true, false),
    // Receivables/payables dashboard and statements over the two above.
    (key::LEDGER, "Ledger", false, 16, true, false),
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
        crate::startup_log::log(&format!(
            "db: schema v{previous_version} -> v{SCHEMA_VERSION}: creating {} table(s): {}",
            missing_before.len(),
            missing_before.join(", ")
        ));
    }

    // Recreated below, once every table rebuild has run — SQLite refuses to
    // rename a rebuilt table into place while a view names a missing one.
    conn.execute_batch("DROP VIEW IF EXISTS sale_payment_lines;")?;
    relax_tokens_table_order_id(conn)?;
    crate::startup_log::log("db: v9->v10 tokens.table_order_id nullability migration checked");

    conn.execute_batch(SCHEMA_SQL)?;
    crate::startup_log::log("db: schema.sql applied (CREATE TABLE/INDEX IF NOT EXISTS)");
    add_missing_columns(conn)?;
    crate::startup_log::log("db: added-columns migration applied");

    add_delivery_support(conn)?;
    crate::startup_log::log("db: v11->v12 delivery support migration checked");

    // Again: the v12 rebuild above copies only the `sales` columns it knew
    // about, so an install upgrading straight from v11 needs v13's re-added.
    add_missing_columns(conn)?;
    widen_sales_payment_methods(conn)?;
    crate::startup_log::log("db: v12->v13 sales payment-method migration checked");
    conn.execute_batch(SALE_PAYMENT_LINES_VIEW)?;

    // If this ever trips, `schema.sql` and EXPECTED_TABLES have drifted apart.
    let missing_after = missing_tables(conn)?;
    if !missing_after.is_empty() {
        crate::startup_log::error(&format!(
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

    // A counter connected to a business gets its owner, platforms and data
    // from the business, never from first-run defaults (they would be sent
    // to every other counter).
    let connected = crate::workspace::local::is_connected(conn)?;
    seed_app_config(conn)?;
    seed_area_pin(conn)?;
    seed_modules(conn)?;
    if !connected {
        seed_delivery_platforms(conn)?;
        seed_owner(conn)?;
    }
    crate::startup_log::log("db: app_config/modules/owner seeded (or already present)");
    if !connected {
        crate::db::seed::seed_demo_data(conn)?;
    }
    // After every table rebuild above (a rebuild drops triggers) and after
    // seeding (seeded rows get their gids here).
    crate::workspace::local::install(conn)?;
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
    // UI colour theme — see `schema.sql`'s comment on the column and
    // `db::config::{get_theme, set_theme}`. Dark by default on upgrade as
    // well as fresh install: dark is the brand's primary look, and an
    // upgrading install has never expressed a preference either way.
    ("app_config", "theme", "TEXT NOT NULL DEFAULT 'dark'"),
    // Desktop sidebar collapsed state — see `schema.sql` and
    // `db::config::{get_ui_prefs, set_sidebar_collapsed}`.
    ("app_config", "sidebar_collapsed", "INTEGER NOT NULL DEFAULT 0"),
    // Android USB-OTG printer selection (vendor:product id + display label)
    // — see `db::config::AppConfig::printer_usb_device` and
    // `printer::android_usb`. Same "NULL until chosen" convention as the
    // Bluetooth/Windows columns above; an install already configured for
    // Bluetooth simply never sets these.
    ("app_config", "printer_usb_device", "TEXT"),
    ("app_config", "printer_usb_name", "TEXT"),
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
    // The shared "sensitive area" PIN's argon2 hash — see
    // `db::security_pin`'s module doc comment. NULL until `seed_area_pin`
    // backfills it below (can't express an argon2 hash, which is randomly
    // salted per call, as a static SQL `DEFAULT` literal the way e.g.
    // `onboarding_completed` above does).
    ("app_config", "sensitive_area_pin_hash", "TEXT"),
    // Short (2-4 digit) PLU-style code for keyboard-first billing (see
    // `db::items::suggest_next_short_code`) — a separate concept from
    // `barcode`: a cashier can type this straight into the billing screen's
    // quick-entry buffer instead of scanning. NULL for any item that hasn't
    // been given one (most items, until an Owner/Admin sets one), which
    // simply means it stays reachable only via search/tap/barcode, same as
    // today. Uniqueness is enforced by `idx_items_short_code` below, not a
    // column constraint — `ALTER TABLE ... ADD COLUMN` can't add a UNIQUE
    // constraint, only `CREATE UNIQUE INDEX` can, hence the two-step setup.
    ("items", "short_code", "TEXT"),
    // Orders links — see schema.sql's `orders` section. Each references a
    // table `execute_batch(SCHEMA_SQL)` has just created, and is nullable
    // (SQLite only allows ADD COLUMN ... REFERENCES with a NULL default).
    ("sales", "order_id", "INTEGER REFERENCES orders (id) ON DELETE SET NULL"),
    ("tokens", "order_id", "INTEGER REFERENCES orders (id) ON DELETE SET NULL"),
    // Existing tokens become 'queued', which is harmless: they have no
    // order_id, and the kitchen display only lists tokens that do.
    (
        "tokens",
        "kitchen_status",
        "TEXT NOT NULL DEFAULT 'queued' CHECK (kitchen_status IN ('queued', 'preparing', 'ready'))",
    ),
    ("tokens", "ready_at", "TEXT"),
    ("token_items", "order_item_id", "INTEGER REFERENCES order_items (id) ON DELETE SET NULL"),
    // Added to `order_items` after v12 reached origin/main (1f542e7, never
    // tagged), so a database first opened by that commit gets it here.
    ("order_items", "sent_at", "TEXT"),
    ("orders", "estimated_prep_minutes", "INTEGER NOT NULL DEFAULT 15"),
    ("sales", "tendered_minor", "INTEGER"),
    ("sales", "change_minor", "INTEGER"),
    ("orders", "shift_id", "INTEGER REFERENCES shifts (id) ON DELETE SET NULL"),
    ("tokens", "shift_id", "INTEGER REFERENCES shifts (id) ON DELETE SET NULL"),
    // v13 — see schema.sql's `sales`/`sale_items` comments.
    ("sales", "customer_id", "INTEGER REFERENCES customers (id) ON DELETE SET NULL"),
    ("sales", "service_charge_minor", "INTEGER NOT NULL DEFAULT 0"),
    ("sales", "due_date", "TEXT"),
    ("sale_items", "cost_at_sale_minor", "INTEGER"),
    // Service charge added to every sale when > 0 — see `db::config::AppConfig`.
    ("app_config", "service_charge_percent", "REAL NOT NULL DEFAULT 0"),
    // Weight-scale (price/weight-embedded) barcodes — see `db::items::parse_scale_barcode`.
    ("app_config", "scale_barcode_enabled", "INTEGER NOT NULL DEFAULT 0"),
    ("app_config", "scale_barcode_prefix", "TEXT NOT NULL DEFAULT '21'"),
    ("app_config", "scale_item_code_digits", "INTEGER NOT NULL DEFAULT 5"),
    ("app_config", "scale_value_kind", "TEXT NOT NULL DEFAULT 'weight'"),
    ("app_config", "scale_value_decimals", "INTEGER NOT NULL DEFAULT 3"),
    // Multi-counter follow-ups — see schema.sql's sync_outbox / sync_state.
    ("sync_outbox", "fields", "TEXT"),
    ("sync_outbox", "base", "INTEGER"),
    ("sync_state", "media_queued", "INTEGER NOT NULL DEFAULT 0"),
    ("sync_state", "pins_sealed", "INTEGER NOT NULL DEFAULT 0"),
    ("sync_terminals", "seq", "INTEGER"),
    // A connected counter that cannot reach the server: keep selling (stock
    // is checked centrally when it reconnects) or refuse sales until then.
    // Shared by the business (`workspace::spec::CONFIG_COLUMNS`).
    ("app_config", "offline_sales", "TEXT NOT NULL DEFAULT 'allow' CHECK (offline_sales IN ('allow', 'block'))"),
    // Printed under the logo on receipts. Shared by the business.
    ("app_config", "business_address", "TEXT"),
    ("app_config", "business_website", "TEXT"),
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
    ("idx_sales_order", "sales", "order_id"),
    ("idx_tokens_order", "tokens", "order_id"),
    ("idx_token_items_order_item", "token_items", "order_item_id"),
    ("idx_orders_shift", "orders", "shift_id"),
    ("idx_tokens_shift", "tokens", "shift_id"),
    ("idx_sales_customer", "sales", "customer_id"),
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

    // `items.short_code` needs a UNIQUE index, not a plain one (same trap
    // `ADDED_COLUMN_INDEXES` above documents: can't live in `schema.sql`
    // since the column doesn't exist there on an upgrading install), and a
    // plain `CREATE INDEX` can't be upgraded to unique in place — so this
    // gets its own statement instead of an `ADDED_COLUMN_INDEXES` entry.
    // SQLite's UNIQUE index already allows any number of NULLs, matching
    // `barcode`'s same nullable-but-unique convention.
    conn.execute_batch("CREATE UNIQUE INDEX IF NOT EXISTS idx_items_short_code ON items (short_code)")?;

    // Drop old unconditional idx_orders_day_no if present and ensure shift/day unique constraints
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_orders_day_no;
         CREATE UNIQUE INDEX IF NOT EXISTS idx_orders_shift_no ON orders (shift_id, order_no) WHERE shift_id IS NOT NULL;
         CREATE UNIQUE INDEX IF NOT EXISTS idx_orders_day_no ON orders (date(created_at), order_no) WHERE shift_id IS NULL;",
    )?;

    Ok(())
}

/// Whether `table` exists yet — every guarded migration step starts here.
fn table_exists(conn: &Connection, table: &str) -> Result<bool, rusqlite::Error> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        params![table],
        |row| row.get(0),
    )
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
    let exists = table_exists(conn, "tokens")?;
    if !exists {
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

/// v11 -> v12: Adds delivery support. Rebuilds `users` table to add 'delivery'
/// role to CHECK constraint, rebuilds `orders` table to add 'third_party_delivery'
/// to order_type CHECK constraint, adds all delivery-related columns to
/// `orders`, and rebuilds `sales` table to add 'platform' to payment_method CHECK.
/// Creates `delivery_platforms` table if missing. Idempotent and safe on existing data.
fn add_delivery_support(conn: &Connection) -> Result<(), rusqlite::Error> {
    // Check if orders table exists and whether it already has delivery_status column
    let orders_exists = table_exists(conn, "orders")?;

    if orders_exists {
        let mut stmt = conn.prepare("PRAGMA table_info(orders)")?;
        let has_delivery_status = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .any(|col| col == "delivery_status");

        if has_delivery_status {
            // Already migrated
            return Ok(());
        }
    } else {
        // Fresh install: SCHEMA_SQL creates the tables with latest schema
        return Ok(());
    }

    conn.pragma_update(None, "foreign_keys", "OFF")?;

    let result = (|| -> Result<(), rusqlite::Error> {
        // 1. Create delivery_platforms table if not exists
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS delivery_platforms (
                 id         INTEGER PRIMARY KEY AUTOINCREMENT,
                 name       TEXT    NOT NULL UNIQUE,
                 is_active  INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
                 created_at TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
             );
             INSERT OR IGNORE INTO delivery_platforms (id, name) VALUES (1, 'Foodpanda');",
        )?;

        // 2. Rebuild users table with 'delivery' role
        let users_exists = table_exists(conn, "users")?;

        if users_exists {
            conn.execute_batch(
                "CREATE TABLE users_v12 (
                     id         INTEGER PRIMARY KEY AUTOINCREMENT,
                     name       TEXT    NOT NULL UNIQUE,
                     pin_hash   TEXT    NOT NULL,
                     role       TEXT    NOT NULL CHECK (role IN ('owner', 'admin', 'cashier', 'delivery')),
                     is_active  INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
                     created_at TEXT    NOT NULL DEFAULT (datetime('now'))
                 );
                 INSERT INTO users_v12 (id, name, pin_hash, role, is_active, created_at)
                     SELECT id, name, pin_hash, role, is_active, created_at FROM users;
                 DROP TABLE users;
                 ALTER TABLE users_v12 RENAME TO users;
                 CREATE INDEX idx_users_active ON users (is_active);",
            )?;
        }

        // 3. Rebuild orders table with delivery columns and third_party_delivery
        conn.execute_batch(
            "CREATE TABLE orders_v12 (
                 id                      INTEGER PRIMARY KEY AUTOINCREMENT,
                 order_no                INTEGER NOT NULL CHECK (order_no > 0),
                 order_type              TEXT    NOT NULL
                                         CHECK (order_type IN ('dine_in', 'takeaway', 'delivery', 'third_party_delivery')),
                 table_id                INTEGER REFERENCES tables (id) ON DELETE SET NULL,
                 status                  TEXT    NOT NULL DEFAULT 'draft'
                                         CHECK (status IN ('draft', 'sent', 'preparing', 'ready',
                                                           'served', 'held', 'cancelled')),
                 held_from_status        TEXT    CHECK (held_from_status IN ('draft', 'sent', 'preparing',
                                                                              'ready', 'served')),
                 version                 INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
                 client_ref              TEXT    NOT NULL UNIQUE,
                 customer_name           TEXT,
                 notes                   TEXT,
                 estimated_prep_minutes  INTEGER NOT NULL DEFAULT 15,
                 created_by              INTEGER REFERENCES users (id) ON DELETE SET NULL,
                 shift_id                INTEGER REFERENCES shifts (id) ON DELETE SET NULL,
                 created_at              TEXT    NOT NULL DEFAULT (datetime('now', 'localtime')),
                 sent_at                 TEXT,
                 ready_at                TEXT,
                 served_at               TEXT,
                 completed_at            TEXT,
                 cancelled_at            TEXT,
                 cancel_reason           TEXT,
                 delivery_rider_id       INTEGER REFERENCES users (id) ON DELETE SET NULL,
                 rider_source            TEXT    CHECK (rider_source IN ('user', 'manual')),
                 delivery_rider_name     TEXT,
                 delivery_rider_phone    TEXT,
                 delivery_platform_id    INTEGER REFERENCES delivery_platforms (id) ON DELETE SET NULL,
                 delivery_platform_name  TEXT,
                 customer_phone          TEXT,
                 delivery_address        TEXT,
                 delivery_charge_minor   INTEGER NOT NULL DEFAULT 0,
                 delivery_status         TEXT    CHECK (delivery_status IN ('placed', 'dispatched', 'delivered', 'cancelled')),
                 platform_order_ref      TEXT
             );
             INSERT INTO orders_v12 (
                 id, order_no, order_type, table_id, status, held_from_status, version,
                 client_ref, customer_name, notes, estimated_prep_minutes, created_by,
                 shift_id, created_at, sent_at, ready_at, served_at, completed_at,
                 cancelled_at, cancel_reason
             )
             SELECT 
                 id, order_no, order_type, table_id, status, held_from_status, version,
                 client_ref, customer_name, notes, estimated_prep_minutes, created_by,
                 shift_id, created_at, sent_at, ready_at, served_at, completed_at,
                 cancelled_at, cancel_reason
             FROM orders;
             DROP TABLE orders;
             ALTER TABLE orders_v12 RENAME TO orders;
             CREATE INDEX idx_orders_status     ON orders (status);
             CREATE INDEX idx_orders_table      ON orders (table_id);
             CREATE INDEX idx_orders_created_at ON orders (created_at);
             CREATE INDEX idx_orders_shift      ON orders (shift_id);
             CREATE UNIQUE INDEX idx_orders_shift_no ON orders (shift_id, order_no) WHERE shift_id IS NOT NULL;
             CREATE UNIQUE INDEX idx_orders_day_no   ON orders (date(created_at), order_no) WHERE shift_id IS NULL;",
        )?;

        // 4. Rebuild sales table to add 'platform' to payment_method CHECK constraint
        let sales_exists = table_exists(conn, "sales")?;
        if sales_exists {
            conn.execute_batch(
                "CREATE TABLE sales_v12 (
                     id             INTEGER PRIMARY KEY AUTOINCREMENT,
                     total_minor    INTEGER NOT NULL CHECK (total_minor >= 0),
                     discount_minor INTEGER NOT NULL DEFAULT 0 CHECK (discount_minor >= 0),
                     tax_minor      INTEGER NOT NULL DEFAULT 0 CHECK (tax_minor >= 0),
                     payment_method TEXT    NOT NULL DEFAULT 'cash'
                                            CHECK (payment_method IN ('cash', 'card', 'other', 'platform')),
                     cashier_id     INTEGER REFERENCES users (id) ON DELETE SET NULL,
                     table_id       INTEGER REFERENCES tables (id) ON DELETE SET NULL,
                     shift_id       INTEGER REFERENCES shifts (id) ON DELETE SET NULL,
                     order_id       INTEGER REFERENCES orders (id) ON DELETE SET NULL,
                     tendered_minor INTEGER,
                     change_minor   INTEGER,
                     created_at     TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
                 );
                 INSERT INTO sales_v12 (id, total_minor, discount_minor, tax_minor, payment_method, cashier_id, table_id, shift_id, order_id, tendered_minor, change_minor, created_at)
                     SELECT id, total_minor, discount_minor, tax_minor, payment_method, cashier_id, table_id, shift_id, order_id, tendered_minor, change_minor, created_at FROM sales;
                 DROP TABLE sales;
                 ALTER TABLE sales_v12 RENAME TO sales;
                 CREATE INDEX idx_sales_created_at ON sales (created_at);
                 CREATE INDEX idx_sales_cashier    ON sales (cashier_id);
                 CREATE INDEX idx_sales_table      ON sales (table_id);
                 CREATE INDEX idx_sales_shift      ON sales (shift_id);
                 CREATE INDEX idx_sales_order      ON sales (order_id);",
            )?;
        }

        Ok(())
    })();

    // Always restore foreign keys
    conn.pragma_update(None, "foreign_keys", "ON")?;
    result
}

/// Every sale's tender breakdown: its `sale_payments` rows, or — for a sale
/// written before v13 (or by a path that records none) — one line for the
/// whole total under `sales.payment_method`. Shift reconciliation and the
/// payment-type reports read this rather than `sales.payment_method`, which
/// is only `split`/`credit` for a multi-tender or pay-later sale. Created
/// here rather than in `schema.sql` because `apply()` drops it before any
/// table rebuild and recreates it afterwards.
const SALE_PAYMENT_LINES_VIEW: &str = "CREATE VIEW IF NOT EXISTS sale_payment_lines AS
    SELECT sp.sale_id, sp.method, sp.amount_minor FROM sale_payments sp
    UNION ALL
    SELECT s.id, s.payment_method, s.total_minor FROM sales s
     WHERE s.total_minor > 0
       AND NOT EXISTS (SELECT 1 FROM sale_payments sp WHERE sp.sale_id = s.id);";

/// v12 -> v13: `sales.payment_method` gains `split` and `credit`. Same
/// rebuild-and-rename as `add_delivery_support`'s step 4 (a CHECK can't be
/// altered in place); a no-op once the stored definition already has them,
/// which includes every fresh install.
fn widen_sales_payment_methods(conn: &Connection) -> Result<(), rusqlite::Error> {
    let sql: Option<String> = conn
        .query_row("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'sales'", [], |row| row.get(0))
        .ok();
    let Some(sql) = sql else { return Ok(()) };
    if sql.contains("'split'") {
        return Ok(());
    }

    conn.pragma_update(None, "foreign_keys", "OFF")?;
    let result = conn.execute_batch(
        "BEGIN;
         CREATE TABLE sales_v13 (
             id             INTEGER PRIMARY KEY AUTOINCREMENT,
             total_minor    INTEGER NOT NULL CHECK (total_minor >= 0),
             discount_minor INTEGER NOT NULL DEFAULT 0 CHECK (discount_minor >= 0),
             tax_minor      INTEGER NOT NULL DEFAULT 0 CHECK (tax_minor >= 0),
             payment_method TEXT    NOT NULL DEFAULT 'cash'
                                    CHECK (payment_method IN ('cash', 'card', 'other', 'platform', 'split', 'credit')),
             cashier_id     INTEGER REFERENCES users (id) ON DELETE SET NULL,
             table_id       INTEGER REFERENCES tables (id) ON DELETE SET NULL,
             shift_id       INTEGER REFERENCES shifts (id) ON DELETE SET NULL,
             order_id       INTEGER REFERENCES orders (id) ON DELETE SET NULL,
             tendered_minor INTEGER,
             change_minor   INTEGER,
             created_at     TEXT    NOT NULL DEFAULT (datetime('now', 'localtime')),
             customer_id    INTEGER REFERENCES customers (id) ON DELETE SET NULL,
             service_charge_minor INTEGER NOT NULL DEFAULT 0 CHECK (service_charge_minor >= 0),
             due_date       TEXT
         );
         INSERT INTO sales_v13 (id, total_minor, discount_minor, tax_minor, payment_method, cashier_id, table_id,
                                shift_id, order_id, tendered_minor, change_minor, created_at, customer_id,
                                service_charge_minor, due_date)
             SELECT id, total_minor, discount_minor, tax_minor, payment_method, cashier_id, table_id,
                    shift_id, order_id, tendered_minor, change_minor, created_at, customer_id,
                    service_charge_minor, due_date FROM sales;
         DROP TABLE sales;
         ALTER TABLE sales_v13 RENAME TO sales;
         CREATE INDEX idx_sales_created_at ON sales (created_at);
         CREATE INDEX idx_sales_cashier    ON sales (cashier_id);
         CREATE INDEX idx_sales_table      ON sales (table_id);
         CREATE INDEX idx_sales_shift      ON sales (shift_id);
         CREATE INDEX idx_sales_order      ON sales (order_id);
         CREATE INDEX idx_sales_customer   ON sales (customer_id);
         COMMIT;",
    );
    if result.is_err() {
        let _ = conn.execute_batch("ROLLBACK;");
    }
    conn.pragma_update(None, "foreign_keys", "ON")?;
    result
}

fn seed_app_config(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute("INSERT OR IGNORE INTO app_config (id) VALUES (1)", [])?;
    Ok(())
}

fn seed_delivery_platforms(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT OR IGNORE INTO delivery_platforms (id, name) VALUES (1, 'Foodpanda')",
        [],
    )?;
    Ok(())
}

/// Inserts any catalogue module that is not in the table yet, along with its
/// default per-platform visibility. Existing rows — and any toggles the client
/// has already made — are left untouched.
fn seed_modules(conn: &Connection) -> Result<(), rusqlite::Error> {
    for (key, name, is_core, sort_order, desktop_default, android_default) in MODULE_CATALOGUE {
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
             SELECT id, ?2, ?3 FROM modules WHERE key = ?1",
            params![key, *desktop_default as i64, *android_default as i64],
        )?;
    }

    Ok(())
}

/// Creates the default owner account so a fresh install is actually loginable.
/// Backfills the shared "sensitive area" PIN's hash the first time —
/// checked against the column's own value, not a row-existence count like
/// `seed_owner` (unlike `users`, `app_config` already has exactly one row
/// by this point via `seed_app_config`, so "hash is still NULL" is the only
/// signal for "never set". Runs for every install, fresh or upgrading:
/// `ADDED_COLUMNS` can only leave the new column NULL (an argon2 hash can't
/// be a static SQL `DEFAULT`), so this is what actually gives every
/// install — not just fresh ones — a real, working default PIN.
fn seed_area_pin(conn: &Connection) -> Result<(), rusqlite::Error> {
    let already_set: bool = conn.query_row(
        "SELECT sensitive_area_pin_hash IS NOT NULL FROM app_config WHERE id = 1",
        [],
        |row| row.get(0),
    )?;
    if already_set {
        return Ok(());
    }

    let pin_hash = users::hash_pin(DEFAULT_AREA_PIN)
        .map_err(|e| rusqlite::Error::InvalidParameterName(format!("area pin hash failed: {}", e)))?;

    conn.execute("UPDATE app_config SET sensitive_area_pin_hash = ?1 WHERE id = 1", params![pin_hash])?;

    Ok(())
}

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
    use crate::db::test_support::count;

    #[test]
    fn upgrading_a_v12_install_widens_payment_methods_and_keeps_sales() {
        let conn = test_conn();
        conn.execute("INSERT INTO sales (total_minor, payment_method) VALUES (2500, 'card')", []).unwrap();
        let sales_before = count(&conn, "SELECT COUNT(*) FROM sales");

        conn.pragma_update(None, "foreign_keys", "OFF").unwrap();
        conn.execute_batch(
            "DROP VIEW sale_payment_lines;
             DROP TABLE sale_payments;
             CREATE TABLE sales_v12 (
                 id             INTEGER PRIMARY KEY AUTOINCREMENT,
                 total_minor    INTEGER NOT NULL CHECK (total_minor >= 0),
                 discount_minor INTEGER NOT NULL DEFAULT 0 CHECK (discount_minor >= 0),
                 tax_minor      INTEGER NOT NULL DEFAULT 0 CHECK (tax_minor >= 0),
                 payment_method TEXT    NOT NULL DEFAULT 'cash'
                                        CHECK (payment_method IN ('cash', 'card', 'other', 'platform')),
                 cashier_id     INTEGER REFERENCES users (id) ON DELETE SET NULL,
                 table_id       INTEGER REFERENCES tables (id) ON DELETE SET NULL,
                 shift_id       INTEGER REFERENCES shifts (id) ON DELETE SET NULL,
                 order_id       INTEGER REFERENCES orders (id) ON DELETE SET NULL,
                 tendered_minor INTEGER,
                 change_minor   INTEGER,
                 created_at     TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
             );
             INSERT INTO sales_v12 SELECT id, total_minor, discount_minor, tax_minor, payment_method, cashier_id,
                                          table_id, shift_id, order_id, tendered_minor, change_minor, created_at
                                     FROM sales;
             DROP TABLE sales;
             ALTER TABLE sales_v12 RENAME TO sales;
             PRAGMA user_version = 12;",
        )
        .unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();

        apply(&conn).unwrap();

        assert_eq!(count(&conn, "SELECT COUNT(*) FROM sales"), sales_before);
        assert!(has_column(&conn, "sales", "customer_id"));
        assert!(has_column(&conn, "sales", "service_charge_minor"));
        conn.execute("INSERT INTO sales (total_minor, payment_method) VALUES (100, 'split')", []).unwrap();
        assert_eq!(
            count(&conn, "SELECT COUNT(*) FROM sale_payment_lines WHERE method = 'card' AND amount_minor = 2500"),
            1,
            "a sale from before v13 reads as one tender of its whole total"
        );
        apply(&conn).unwrap();
    }

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
    fn delivery_support_migration_upgrades_populated_v11_install_and_preserves_data() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();

        // Setup a pre-v12 schema with populated users, tables, orders, order_items, sales
        conn.execute_batch(
            "CREATE TABLE users (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 name TEXT NOT NULL UNIQUE,
                 pin_hash TEXT NOT NULL,
                 role TEXT NOT NULL CHECK (role IN ('owner', 'admin', 'cashier')),
                 is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
                 created_at TEXT NOT NULL DEFAULT (datetime('now'))
             );
             CREATE TABLE tables (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 name TEXT NOT NULL UNIQUE,
                 seats INTEGER NOT NULL DEFAULT 4,
                 status TEXT NOT NULL DEFAULT 'free',
                 created_at TEXT NOT NULL DEFAULT (datetime('now'))
             );
             CREATE TABLE shifts (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 cashier_id INTEGER REFERENCES users (id),
                 opened_at TEXT NOT NULL DEFAULT (datetime('now')),
                 closed_at TEXT
             );
             CREATE TABLE orders (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 order_no INTEGER NOT NULL,
                 order_type TEXT NOT NULL CHECK (order_type IN ('dine_in', 'takeaway', 'delivery')),
                 table_id INTEGER REFERENCES tables (id),
                 status TEXT NOT NULL DEFAULT 'draft',
                 held_from_status TEXT,
                 version INTEGER NOT NULL DEFAULT 1,
                 client_ref TEXT NOT NULL UNIQUE,
                 customer_name TEXT,
                 notes TEXT,
                 estimated_prep_minutes INTEGER NOT NULL DEFAULT 15,
                 created_by INTEGER REFERENCES users (id),
                 shift_id INTEGER,
                 created_at TEXT NOT NULL DEFAULT (datetime('now')),
                 sent_at TEXT,
                 ready_at TEXT,
                 served_at TEXT,
                 completed_at TEXT,
                 cancelled_at TEXT,
                 cancel_reason TEXT
             );
             CREATE TABLE order_items (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 order_id INTEGER NOT NULL REFERENCES orders (id) ON DELETE CASCADE,
                 item_id INTEGER NOT NULL,
                 qty REAL NOT NULL,
                 price_at_order_minor INTEGER NOT NULL,
                 notes TEXT,
                 sent_at TEXT,
                 status TEXT NOT NULL DEFAULT 'active',
                 voided_by INTEGER,
                 void_reason TEXT,
                 voided_at TEXT,
                 sale_id INTEGER,
                 added_by INTEGER,
                 added_at TEXT NOT NULL DEFAULT (datetime('now'))
             );
             CREATE TABLE sales (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 total_minor INTEGER NOT NULL,
                 discount_minor INTEGER NOT NULL DEFAULT 0,
                 tax_minor INTEGER NOT NULL DEFAULT 0,
                 payment_method TEXT NOT NULL DEFAULT 'cash' CHECK (payment_method IN ('cash', 'card', 'other')),
                 cashier_id INTEGER REFERENCES users (id),
                 table_id INTEGER REFERENCES tables (id),
                 shift_id INTEGER,
                 order_id INTEGER REFERENCES orders (id),
                 tendered_minor INTEGER,
                 change_minor INTEGER,
                 created_at TEXT NOT NULL DEFAULT (datetime('now'))
             );
             INSERT INTO users (id, name, pin_hash, role) VALUES (1, 'Admin', 'hash1', 'admin');
             INSERT INTO users (id, name, pin_hash, role) VALUES (2, 'Cashier 1', 'hash2', 'cashier');
             INSERT INTO tables (id, name) VALUES (1, 'Table 1');
             INSERT INTO orders (id, order_no, order_type, table_id, client_ref, customer_name)
                 VALUES (1, 101, 'dine_in', 1, 'ref-101', 'Alice');
             INSERT INTO order_items (id, order_id, item_id, qty, price_at_order_minor)
                 VALUES (1, 1, 10, 2.0, 500);
             INSERT INTO sales (id, total_minor, payment_method, cashier_id, order_id)
                 VALUES (1, 1000, 'cash', 2, 1);"
        )
        .unwrap();

        // Run migration
        add_delivery_support(&conn).unwrap();

        // 1. Verify foreign key check passes
        let fk_errors: Vec<String> = conn
            .prepare("PRAGMA foreign_key_check")
            .unwrap()
            .query_map([], |row| {
                Ok(format!(
                    "table: {}, rowid: {}, parent: {}",
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?
                ))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(fk_errors.is_empty(), "Foreign key check failed: {:?}", fk_errors);

        // 2. Existing users preserved and new 'delivery' role works
        let user_count: i64 = conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0)).unwrap();
        assert_eq!(user_count, 2);
        conn.execute("INSERT INTO users (name, pin_hash, role) VALUES ('Rider 1', 'hash3', 'delivery')", [])
            .expect("users table must now accept 'delivery' role");

        // 3. Existing orders preserved, new columns present, and third_party_delivery works
        let (order_no, cust_name): (i64, Option<String>) = conn
            .query_row("SELECT order_no, customer_name FROM orders WHERE id = 1", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(order_no, 101);
        assert_eq!(cust_name, Some("Alice".to_string()));

        conn.execute(
            "INSERT INTO orders (order_no, order_type, client_ref, delivery_status, delivery_rider_name)
             VALUES (102, 'third_party_delivery', 'ref-102', 'dispatched', 'Bob Rider')",
            [],
        )
        .expect("orders table must now accept 'third_party_delivery' and delivery columns");

        // 4. Existing sales preserved and 'platform' payment method works
        let sale_total: i64 = conn.query_row("SELECT total_minor FROM sales WHERE id = 1", [], |r| r.get(0)).unwrap();
        assert_eq!(sale_total, 1000);
        conn.execute("INSERT INTO sales (total_minor, payment_method) VALUES (1500, 'platform')", [])
            .expect("sales table must now accept 'platform' payment method");

        // 5. delivery_platforms table exists and seeded
        let platform_name: String = conn.query_row("SELECT name FROM delivery_platforms WHERE id = 1", [], |r| r.get(0)).unwrap();
        assert_eq!(platform_name, "Foodpanda");

        // 6. Idempotent — running again is a no-op
        add_delivery_support(&conn).unwrap();
    }

    fn has_column(conn: &Connection, table: &str, column: &str) -> bool {
        conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2",
            params![table, column],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
            == 1
    }

    /// Inserts a takeaway order and returns its id.
    fn insert_order(conn: &Connection, order_no: i64, client_ref: &str, created_at: &str) -> i64 {
        conn.execute(
            "INSERT INTO orders (order_no, order_type, client_ref, created_at) VALUES (?1, 'takeaway', ?2, ?3)",
            params![order_no, client_ref, created_at],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn upgrading_a_v11_install_adds_the_order_link_columns_and_keeps_existing_rows() {
        let conn = test_conn();
        // A token with a line, so tokens/token_items have rows to carry over.
        conn.execute_batch(
            "INSERT INTO counters (name) VALUES ('Grill');
             INSERT INTO tokens (token_number, counter_id) VALUES (7, last_insert_rowid());
             INSERT INTO token_items (token_id, item_id, qty_on_token)
                 VALUES (last_insert_rowid(), (SELECT id FROM items LIMIT 1), 2);",
        )
        .unwrap();
        let sales_before = count(&conn, "SELECT COUNT(*) FROM sales");
        let tokens_before = count(&conn, "SELECT COUNT(*) FROM tokens");
        let token_items_before = count(&conn, "SELECT COUNT(*) FROM token_items");
        assert!(sales_before > 0);

        // Rewind to the v11 shape: no orders tables, and sales/tokens/
        // token_items rebuilt from their exact v11 definitions (the bundled
        // SQLite can't DROP COLUMN out of these commented CREATE TABLEs).
        conn.pragma_update(None, "foreign_keys", "OFF").unwrap();
        conn.execute_batch(
            "DROP VIEW sale_payment_lines;
             DROP TABLE order_status_events;
             DROP TABLE order_items;
             DROP TABLE orders;
             CREATE TABLE sales_v11 (
                 id             INTEGER PRIMARY KEY AUTOINCREMENT,
                 total_minor    INTEGER NOT NULL CHECK (total_minor >= 0),
                 discount_minor INTEGER NOT NULL DEFAULT 0 CHECK (discount_minor >= 0),
                 tax_minor      INTEGER NOT NULL DEFAULT 0 CHECK (tax_minor >= 0),
                 payment_method TEXT    NOT NULL DEFAULT 'cash'
                                        CHECK (payment_method IN ('cash', 'card', 'other')),
                 cashier_id     INTEGER REFERENCES users (id) ON DELETE SET NULL,
                 table_id       INTEGER REFERENCES tables (id) ON DELETE SET NULL,
                 shift_id       INTEGER REFERENCES shifts (id) ON DELETE SET NULL,
                 created_at     TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
             );
             INSERT INTO sales_v11
                 SELECT id, total_minor, discount_minor, tax_minor, payment_method,
                        cashier_id, table_id, shift_id, created_at FROM sales;
             DROP TABLE sales;
             ALTER TABLE sales_v11 RENAME TO sales;
             CREATE TABLE tokens_v11 (
                 id             INTEGER PRIMARY KEY AUTOINCREMENT,
                 token_number   INTEGER NOT NULL,
                 table_order_id INTEGER REFERENCES table_orders (id) ON DELETE CASCADE,
                 counter_id     INTEGER NOT NULL REFERENCES counters (id) ON DELETE RESTRICT,
                 printed_at     TEXT    NOT NULL DEFAULT (datetime('now', 'localtime')),
                 printed_by     INTEGER REFERENCES users (id) ON DELETE SET NULL,
                 status         TEXT    NOT NULL DEFAULT 'printed' CHECK (status IN ('printed', 'cancelled'))
             );
             INSERT INTO tokens_v11
                 SELECT id, token_number, table_order_id, counter_id, printed_at, printed_by, status FROM tokens;
             DROP TABLE tokens;
             ALTER TABLE tokens_v11 RENAME TO tokens;
             CREATE TABLE token_items_v11 (
                 id            INTEGER PRIMARY KEY AUTOINCREMENT,
                 token_id      INTEGER NOT NULL REFERENCES tokens (id) ON DELETE CASCADE,
                 item_id       INTEGER NOT NULL REFERENCES items (id) ON DELETE RESTRICT,
                 qty_on_token  REAL    NOT NULL CHECK (qty_on_token > 0)
             );
             INSERT INTO token_items_v11 SELECT id, token_id, item_id, qty_on_token FROM token_items;
             DROP TABLE token_items;
             ALTER TABLE token_items_v11 RENAME TO token_items;
             PRAGMA user_version = 11;",
        )
        .unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        assert!(!has_column(&conn, "sales", "order_id"));

        apply(&conn).unwrap();

        for (table, column) in [
            ("sales", "order_id"),
            ("tokens", "order_id"),
            ("tokens", "kitchen_status"),
            ("tokens", "ready_at"),
            ("token_items", "order_item_id"),
        ] {
            assert!(has_column(&conn, table, column), "{table}.{column} missing after upgrade");
        }
        assert!(missing_tables(&conn).unwrap().is_empty());
        assert_eq!(count(&conn, "PRAGMA user_version"), SCHEMA_VERSION);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM sales"), sales_before);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM tokens"), tokens_before);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM token_items"), token_items_before);
        // Existing tokens get the default and no order link.
        assert_eq!(
            count(&conn, "SELECT COUNT(*) FROM tokens WHERE kitchen_status = 'queued' AND order_id IS NULL"),
            tokens_before
        );
        for index in ["idx_sales_order", "idx_tokens_order", "idx_token_items_order_item"] {
            assert_eq!(
                count(&conn, &format!("SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = '{index}'")),
                1,
                "{index} missing after upgrade"
            );
        }
    }

    #[test]
    fn order_numbers_are_unique_per_day_and_client_refs_are_unique() {
        let conn = test_conn();
        insert_order(&conn, 1, "a", "2026-10-17 09:00:00");
        // Same number, same day: refused.
        assert!(conn
            .execute(
                "INSERT INTO orders (order_no, order_type, client_ref, created_at)
                 VALUES (1, 'takeaway', 'b', '2026-10-17 18:00:00')",
                [],
            )
            .is_err());
        // Same number, next day: fine (the sequence resets daily).
        insert_order(&conn, 1, "c", "2026-10-18 09:00:00");
        // A retried create with the same client_ref: refused.
        assert!(conn
            .execute(
                "INSERT INTO orders (order_no, order_type, client_ref) VALUES (2, 'takeaway', 'a')",
                [],
            )
            .is_err());
    }

    #[test]
    fn order_columns_reject_values_outside_their_sets() {
        let conn = test_conn();
        let order_id = insert_order(&conn, 1, "a", "2026-10-17 09:00:00");
        let item_id = count(&conn, "SELECT id FROM items LIMIT 1");
        for bad in [
            "UPDATE orders SET status = 'paid' WHERE id = ?1",
            "UPDATE orders SET order_type = 'drive_thru' WHERE id = ?1",
            "UPDATE orders SET held_from_status = 'held' WHERE id = ?1",
            "UPDATE orders SET version = 0 WHERE id = ?1",
        ] {
            assert!(conn.execute(bad, [order_id]).is_err(), "accepted: {bad}");
        }
        for bad in [
            "INSERT INTO order_items (order_id, item_id, qty, price_at_order_minor) VALUES (?1, ?2, 0, 100)",
            "INSERT INTO order_items (order_id, item_id, qty, price_at_order_minor) VALUES (?1, ?2, 1, -1)",
            "INSERT INTO order_items (order_id, item_id, qty, price_at_order_minor, status)
                 VALUES (?1, ?2, 1, 100, 'gone')",
        ] {
            assert!(conn.execute(bad, params![order_id, item_id]).is_err(), "accepted: {bad}");
        }
        conn.execute_batch(
            "INSERT INTO counters (name) VALUES ('Grill');
             INSERT INTO tokens (token_number, counter_id) VALUES (1, last_insert_rowid());",
        )
        .unwrap();
        assert!(conn.execute("UPDATE tokens SET kitchen_status = 'served'", []).is_err());
    }

    #[test]
    fn a_sale_that_paid_order_lines_cannot_be_deleted_and_orders_cascade_their_lines() {
        let conn = test_conn();
        let order_id = insert_order(&conn, 1, "a", "2026-10-17 09:00:00");
        let (sale_id, item_id): (i64, i64) = conn
            .query_row("SELECT sale_id, item_id FROM sale_items LIMIT 1", [], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap();
        conn.execute(
            "INSERT INTO order_items (order_id, item_id, qty, price_at_order_minor, sale_id)
             VALUES (?1, ?2, 1, 100, ?3)",
            params![order_id, item_id, sale_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO order_status_events (order_id, to_status) VALUES (?1, 'draft')",
            [order_id],
        )
        .unwrap();
        conn.execute("UPDATE sales SET order_id = ?1 WHERE id = ?2", params![order_id, sale_id]).unwrap();

        // Historic data is immutable: the paying sale can't vanish.
        assert!(conn.execute("DELETE FROM sales WHERE id = ?1", [sale_id]).is_err());

        // Deleting the order takes its lines and history; the sale survives, unlinked.
        conn.execute("DELETE FROM orders WHERE id = ?1", [order_id]).unwrap();
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM order_items"), 0);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM order_status_events"), 0);
        let link: Option<i64> =
            conn.query_row("SELECT order_id FROM sales WHERE id = ?1", [sale_id], |row| row.get(0)).unwrap();
        assert_eq!(link, None);
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
