-- ===========================================================================
-- POS schema.
--
-- Applied on every startup; every statement is idempotent (IF NOT EXISTS), so
-- replaying it creates whatever is missing and never touches existing rows.
-- See db/schema.rs for the runner.
--
-- Conventions
--   * Booleans are INTEGER 0/1 (SQLite has no bool type).
--   * Timestamps are TEXT in SQLite datetime format ('YYYY-MM-DD HH:MM:SS',
--     local time); plain dates are TEXT 'YYYY-MM-DD'; months are 'YYYY-MM'.
--   * MONEY IS STORED AS INTEGER MINOR UNITS (paisa/cents), never floats —
--     a till must not accumulate rounding drift. Columns holding money carry a
--     `_minor` suffix so the unit is impossible to misread at a call site.
--     Divide by 100 only at the presentation/print boundary.
--
-- ---------------------------------------------------------------------------
-- Entity relationships
-- ---------------------------------------------------------------------------
--
--   Selling
--     categories 1──* items
--     A sale is a header plus its lines:
--       sales 1──* sale_items *──1 items
--     `sale_items.price_at_sale_minor` is copied from the item at checkout, so
--     a later price change never rewrites history. Deleting a sale cascades to
--     its lines; deleting an item that appears on any sale is REFUSED
--     (ON DELETE RESTRICT) — retire an item with `items.is_active = 0` instead.
--     `sales.cashier_id` -> users (who rang it up).
--
--   Restaurant flow (only when the tables module is enabled)
--     tables 1──* table_orders *──1 sales
--     A table is opened, an order row links it to a draft sale, and the sale
--     is completed at payment. `sales.table_id` is NULL for retail counter
--     sales, which is why the whole flow is optional.
--
--   Staff, pay and costs
--     employees 1──* attendance          (one row per shift; days worked =
--                                         COUNT(DISTINCT work_date))
--     employees 1──* salary_payments     (one row per employee per month;
--                                         calculated_amount_minor is derived
--                                         FROM attendance, paid_amount_minor
--                                         is what actually went out)
--     expenses stands alone (date/category/amount) and, with sales and
--     salary_payments, feeds the dashboard profit figure.
--
--   employees and users are deliberately separate: `users` are login accounts
--   (owner/admin/cashier PINs), `employees` are payroll records. A shop may
--   pay staff who never log in, and vice versa.
-- ===========================================================================

-- ---------------------------------------------------------------------------
-- Installation config: one row, id pinned to 1.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS app_config (
    id              INTEGER PRIMARY KEY CHECK (id = 1),
    business_name   TEXT    NOT NULL DEFAULT 'My Business',
    business_type   TEXT    NOT NULL DEFAULT 'retail'
                            CHECK (business_type IN ('retail', 'restaurant', 'other')),
    logo_path       TEXT,
    currency        TEXT    NOT NULL DEFAULT 'PKR',
    tax_percent     REAL    NOT NULL DEFAULT 0 CHECK (tax_percent >= 0),
    receipt_footer  TEXT    NOT NULL DEFAULT 'Thank you for your purchase!',
    -- Divisor for salary calculation (base_salary / working_days_per_month *
    -- days_present) — configurable per installation rather than assuming a
    -- fixed 26- or 30-day month. See db/salary.rs.
    working_days_per_month INTEGER NOT NULL DEFAULT 26 CHECK (working_days_per_month > 0),
    -- Set once the first-time setup wizard (Phase 14) finishes. A fresh
    -- install always seeds a row here (see `seed_app_config` in schema.rs) so
    -- "app_config is empty" can't be tested with a row-existence check — this
    -- flag is what actually distinguishes "never configured" from "configured
    -- with defaults nobody's looked at yet".
    onboarding_completed INTEGER NOT NULL DEFAULT 0 CHECK (onboarding_completed IN (0, 1))
);

-- ---------------------------------------------------------------------------
-- Fixed catalogue of every module the product can ship. Rows are seeded once
-- and never client-specific — what varies per client lives in enabled_modules.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS modules (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    key         TEXT    NOT NULL UNIQUE,
    name        TEXT    NOT NULL,
    is_core     INTEGER NOT NULL DEFAULT 0 CHECK (is_core IN (0, 1)),
    sort_order  INTEGER NOT NULL DEFAULT 0
);

-- ---------------------------------------------------------------------------
-- Per-installation, per-platform visibility. One row per module; desktop and
-- android are independent so a client can run everything on the till but only
-- Billing + Reports on staff phones.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS enabled_modules (
    module_id       INTEGER PRIMARY KEY
                    REFERENCES modules (id) ON DELETE CASCADE,
    desktop_enabled INTEGER NOT NULL DEFAULT 1 CHECK (desktop_enabled IN (0, 1)),
    android_enabled INTEGER NOT NULL DEFAULT 0 CHECK (android_enabled IN (0, 1))
);

-- ---------------------------------------------------------------------------
-- Local-only accounts. Auth is a PIN checked against an argon2 hash; there are
-- no tokens or sessions on disk because the app is single-machine and offline.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS users (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT    NOT NULL UNIQUE,
    pin_hash   TEXT    NOT NULL,
    role       TEXT    NOT NULL CHECK (role IN ('owner', 'admin', 'cashier')),
    is_active  INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_users_active ON users (is_active);

-- ###########################################################################
-- Phase 2 — operational schema
-- ###########################################################################

-- ---------------------------------------------------------------------------
-- Inventory
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS categories (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT    NOT NULL UNIQUE,
    created_at TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE TABLE IF NOT EXISTS items (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    name                TEXT    NOT NULL,
    -- Nullable: not every shop barcodes everything. UNIQUE still allows many
    -- NULLs in SQLite, so unbarcoded items do not collide.
    barcode             TEXT    UNIQUE,
    price_minor         INTEGER NOT NULL CHECK (price_minor >= 0),
    cost_minor          INTEGER NOT NULL DEFAULT 0 CHECK (cost_minor >= 0),
    stock_qty           INTEGER NOT NULL DEFAULT 0,
    category_id         INTEGER REFERENCES categories (id) ON DELETE SET NULL,
    low_stock_threshold INTEGER NOT NULL DEFAULT 0 CHECK (low_stock_threshold >= 0),
    -- Retire an item rather than deleting it, so its sale history survives.
    is_active           INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    -- Filename only (not a full path) of the product photo under the app's
    -- image store — see db/images.rs. Portable across reinstalls because it
    -- never encodes the app data directory itself.
    image_path           TEXT,
    created_at          TEXT    NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at          TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);

-- Scanner lookups happen on every item added to a cart, so barcode must be an
-- index hit; the UNIQUE constraint above already provides that index, so no
-- explicit one is declared here.
CREATE INDEX IF NOT EXISTS idx_items_category ON items (category_id);
CREATE INDEX IF NOT EXISTS idx_items_active   ON items (is_active);
-- Name search in the billing screen.
CREATE INDEX IF NOT EXISTS idx_items_name     ON items (name);

-- ---------------------------------------------------------------------------
-- Restaurant tables (only used when the tables module is enabled)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS tables (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT    NOT NULL UNIQUE,
    seats      INTEGER NOT NULL DEFAULT 4 CHECK (seats > 0),
    status     TEXT    NOT NULL DEFAULT 'free'
                       CHECK (status IN ('free', 'occupied', 'reserved')),
    created_at TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE INDEX IF NOT EXISTS idx_tables_status ON tables (status);

-- ---------------------------------------------------------------------------
-- Sales
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS sales (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    -- What the customer actually paid: subtotal - discount + tax.
    total_minor    INTEGER NOT NULL CHECK (total_minor >= 0),
    discount_minor INTEGER NOT NULL DEFAULT 0 CHECK (discount_minor >= 0),
    tax_minor      INTEGER NOT NULL DEFAULT 0 CHECK (tax_minor >= 0),
    payment_method TEXT    NOT NULL DEFAULT 'cash'
                           CHECK (payment_method IN ('cash', 'card', 'other')),
    -- Kept even if the cashier's account is later removed, so a bill always
    -- has a total; ON DELETE SET NULL rather than losing the sale.
    cashier_id     INTEGER REFERENCES users (id) ON DELETE SET NULL,
    -- NULL for ordinary counter sales; set only in restaurant mode.
    table_id       INTEGER REFERENCES tables (id) ON DELETE SET NULL,
    created_at     TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);

-- Every report is a date-range scan over this column.
CREATE INDEX IF NOT EXISTS idx_sales_created_at ON sales (created_at);
CREATE INDEX IF NOT EXISTS idx_sales_cashier    ON sales (cashier_id);
CREATE INDEX IF NOT EXISTS idx_sales_table      ON sales (table_id);

CREATE TABLE IF NOT EXISTS sale_items (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    sale_id              INTEGER NOT NULL REFERENCES sales (id) ON DELETE CASCADE,
    -- RESTRICT: an item that has ever been sold cannot be deleted out from
    -- under its history. Deactivate it instead.
    item_id              INTEGER NOT NULL REFERENCES items (id) ON DELETE RESTRICT,
    qty                  INTEGER NOT NULL CHECK (qty > 0),
    -- Price as of checkout — never read the live item price for an old bill.
    price_at_sale_minor  INTEGER NOT NULL CHECK (price_at_sale_minor >= 0)
);

CREATE INDEX IF NOT EXISTS idx_sale_items_sale ON sale_items (sale_id);
-- "Top selling items" joins the other way.
CREATE INDEX IF NOT EXISTS idx_sale_items_item ON sale_items (item_id);

CREATE TABLE IF NOT EXISTS table_orders (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    table_id   INTEGER NOT NULL REFERENCES tables (id) ON DELETE CASCADE,
    -- The draft sale this table is accumulating; cleared if the sale is voided.
    sale_id    INTEGER REFERENCES sales (id) ON DELETE SET NULL,
    status     TEXT    NOT NULL DEFAULT 'open'
                       CHECK (status IN ('open', 'billed', 'closed', 'cancelled')),
    -- JSON snapshot of the cart parked on this table while `status = 'open'`
    -- (a Phase 4 stand-in for real draft line items — see db/sales.rs). NULL
    -- once the order is billed.
    cart_json  TEXT,
    opened_at  TEXT    NOT NULL DEFAULT (datetime('now', 'localtime')),
    closed_at  TEXT
);

CREATE INDEX IF NOT EXISTS idx_table_orders_table  ON table_orders (table_id);
CREATE INDEX IF NOT EXISTS idx_table_orders_sale   ON table_orders (sale_id);
CREATE INDEX IF NOT EXISTS idx_table_orders_status ON table_orders (status);

-- ---------------------------------------------------------------------------
-- Staff, attendance and pay
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS employees (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    name              TEXT    NOT NULL,
    role              TEXT    NOT NULL DEFAULT 'staff',
    contact           TEXT,
    base_salary_minor INTEGER NOT NULL DEFAULT 0 CHECK (base_salary_minor >= 0),
    is_active         INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at        TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE INDEX IF NOT EXISTS idx_employees_active ON employees (is_active);

CREATE TABLE IF NOT EXISTS attendance (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    employee_id INTEGER NOT NULL REFERENCES employees (id) ON DELETE CASCADE,
    -- 'YYYY-MM-DD'. Named work_date so it never shadows SQLite's date()
    -- function in queries like date(work_date).
    work_date   TEXT    NOT NULL,
    check_in    TEXT,
    check_out   TEXT,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);

-- Both the per-employee log and the monthly summary filter on these two
-- columns together, so they share one composite index.
CREATE INDEX IF NOT EXISTS idx_attendance_employee_date ON attendance (employee_id, work_date);
CREATE INDEX IF NOT EXISTS idx_attendance_date          ON attendance (work_date);

CREATE TABLE IF NOT EXISTS salary_payments (
    id                      INTEGER PRIMARY KEY AUTOINCREMENT,
    employee_id             INTEGER NOT NULL REFERENCES employees (id) ON DELETE CASCADE,
    -- 'YYYY-MM'. One row per employee per month.
    month                   TEXT    NOT NULL,
    -- Derived from attendance at the time of calculation.
    calculated_amount_minor INTEGER NOT NULL DEFAULT 0 CHECK (calculated_amount_minor >= 0),
    paid_amount_minor       INTEGER NOT NULL DEFAULT 0 CHECK (paid_amount_minor >= 0),
    paid_date               TEXT,
    created_at              TEXT    NOT NULL DEFAULT (datetime('now', 'localtime')),
    UNIQUE (employee_id, month)
);

CREATE INDEX IF NOT EXISTS idx_salary_payments_employee_month
    ON salary_payments (employee_id, month);

-- ---------------------------------------------------------------------------
-- Expenses
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS expenses (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    expense_date TEXT    NOT NULL,
    category     TEXT    NOT NULL,
    amount_minor INTEGER NOT NULL CHECK (amount_minor >= 0),
    note         TEXT,
    created_at   TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE INDEX IF NOT EXISTS idx_expenses_date     ON expenses (expense_date);
CREATE INDEX IF NOT EXISTS idx_expenses_category ON expenses (category);
