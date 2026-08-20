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
--   Refunds (against any sale, whether or not a shift is in use)
--     sales 1──* refunds 1──* refund_items *──1 sale_items
--     A refund always references the original sale; refund_items is
--     line-level so a partial refund (some items/quantities, not the whole
--     sale) is representable, and each refunded qty puts stock back via
--     `items.stock_qty` — the mirror of what `create_sale` decremented.
--
--   Cashier shifts (only when the shifts module is enabled)
--     shifts 1──* sales (sales.shift_id, nullable — a sale rung up with no
--     shift open, or before this feature existed, has none). Closing a
--     shift reconciles its declared cash count against cash sales minus
--     refunds recorded during the shift — see `db::shifts`.
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
    android_enabled INTEGER NOT NULL DEFAULT 0 CHECK (android_enabled IN (0, 1)),
    -- Set only by the product-owner override path (`db::product_owner`),
    -- never by the client-facing `toggle_module` command. When true for a
    -- platform, the client's own Owner/Admin cannot change that platform's
    -- `*_enabled` value at all — `modules::set_enabled` checks this first
    -- and refuses with a clear error rather than silently no-op'ing. Two
    -- columns (not one shared flag) because a lock is per-platform, same
    -- split as `*_enabled` itself: the product owner may lock Android's
    -- visibility while leaving desktop free to toggle, or vice versa.
    desktop_locked  INTEGER NOT NULL DEFAULT 0 CHECK (desktop_locked IN (0, 1)),
    android_locked  INTEGER NOT NULL DEFAULT 0 CHECK (android_locked IN (0, 1))
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

-- ---------------------------------------------------------------------------
-- The vendor/developer account — deliberately NOT a row in `users`, and
-- never joined against or exposed by any client-facing query (Manage Users,
-- the login screen's account list, etc). One row, pinned `id = 1`, exactly
-- like `app_config`; absent until the vendor sets a credential on this
-- specific install via the hidden entry point (`db::product_owner`) — there
-- is no default/shared password baked into the binary. See SUPPORT.md for
-- what recovering a forgotten credential actually looks like, since there
-- is deliberately no in-app reset path for this account.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS product_owner_account (
    id         INTEGER PRIMARY KEY CHECK (id = 1),
    pin_hash   TEXT    NOT NULL,
    created_at TEXT    NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);

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
    -- Short blurb shown on the billing screen's item detail modal. Purely
    -- presentational — never read by pricing/stock/reporting logic.
    description         TEXT,
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
-- Cashier shifts (only meaningful when the `shifts` module is enabled —
-- opening/closing one is optional per client, see MODULE_CATALOGUE). A shift
-- is opened with a declared opening cash float and closed with a declared
-- cash count; `db::shifts::build_summary` is what turns the sales/refunds
-- that happened in between into the Short/Over reconciliation figure.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS shifts (
    id                         INTEGER PRIMARY KEY AUTOINCREMENT,
    -- Kept even if the cashier's account is later removed, same rationale
    -- as sales.cashier_id below.
    cashier_id                 INTEGER REFERENCES users (id) ON DELETE SET NULL,
    opened_at                  TEXT    NOT NULL DEFAULT (datetime('now', 'localtime')),
    -- NULL while the shift is still open; set once by close_shift.
    closed_at                  TEXT,
    opening_balance_minor      INTEGER NOT NULL DEFAULT 0 CHECK (opening_balance_minor >= 0),
    -- NULL until close_shift records what the cashier actually counted.
    declared_cash_amount_minor INTEGER CHECK (declared_cash_amount_minor >= 0),
    notes                      TEXT
);

CREATE INDEX IF NOT EXISTS idx_shifts_cashier ON shifts (cashier_id);
-- "does this cashier already have an open shift" is a lookup on exactly
-- these two columns together.
CREATE INDEX IF NOT EXISTS idx_shifts_cashier_open ON shifts (cashier_id, closed_at);

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
    -- NULL for a sale rung up with no shift open (the `shifts` module is
    -- itself optional — see MODULE_CATALOGUE) or once its shift's cashier
    -- account is removed. Present on a fresh install directly; an existing
    -- install gets it via ADDED_COLUMNS in schema.rs, since a table that
    -- already exists never sees a column added to this CREATE TABLE.
    shift_id       INTEGER REFERENCES shifts (id) ON DELETE SET NULL,
    created_at     TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);

-- Every report is a date-range scan over this column.
CREATE INDEX IF NOT EXISTS idx_sales_created_at ON sales (created_at);
CREATE INDEX IF NOT EXISTS idx_sales_cashier    ON sales (cashier_id);
CREATE INDEX IF NOT EXISTS idx_sales_table      ON sales (table_id);
-- idx_sales_shift is NOT declared here — see schema.rs's comment on
-- `ADDED_COLUMN_INDEXES` for why an index on an ADDED_COLUMNS column can't
-- live in this file.

CREATE TABLE IF NOT EXISTS sale_items (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    sale_id              INTEGER NOT NULL REFERENCES sales (id) ON DELETE CASCADE,
    -- RESTRICT: an item that has ever been sold cannot be deleted out from
    -- under its history. Deactivate it instead.
    item_id              INTEGER NOT NULL REFERENCES items (id) ON DELETE RESTRICT,
    qty                  INTEGER NOT NULL CHECK (qty > 0),
    -- Price as of checkout — never read the live item price for an old bill.
    price_at_sale_minor  INTEGER NOT NULL CHECK (price_at_sale_minor >= 0),
    -- A cashier's free-text note on this line ("no onions"). Purely
    -- informational — never affects pricing, stock or reporting.
    notes                TEXT
);

CREATE INDEX IF NOT EXISTS idx_sale_items_sale ON sale_items (sale_id);
-- "Top selling items" joins the other way.
CREATE INDEX IF NOT EXISTS idx_sale_items_item ON sale_items (item_id);

-- ---------------------------------------------------------------------------
-- Refunds — a refund always references the original sale it came from,
-- never a bare adjustment; refund_items is line-level so a refund can cover
-- some items/quantities from a sale without needing to void the whole
-- thing. Both original_sale_id and sale_item_id are ON DELETE RESTRICT for
-- the same "historic data is immutable" reason sale_items.item_id is: this
-- product has no "delete a sale" command today, but if one is ever added, a
-- sale a refund references must not be able to vanish out from under that
-- refund's history.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS refunds (
    id                        INTEGER PRIMARY KEY AUTOINCREMENT,
    original_sale_id          INTEGER NOT NULL REFERENCES sales (id) ON DELETE RESTRICT,
    -- Kept even if the account is later removed — same rationale as
    -- sales.cashier_id.
    refunded_by               INTEGER REFERENCES users (id) ON DELETE SET NULL,
    reason                    TEXT,
    -- Sum of refund_items.amount_refunded_minor, computed and stored at
    -- creation time (never trusted from the client) so a receipt reprint
    -- never has to re-derive it from the line items.
    total_refund_amount_minor INTEGER NOT NULL CHECK (total_refund_amount_minor >= 0),
    created_at                TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE INDEX IF NOT EXISTS idx_refunds_sale       ON refunds (original_sale_id);
-- Reports/shift reconciliation scan refunds by date, same as sales.
CREATE INDEX IF NOT EXISTS idx_refunds_created_at ON refunds (created_at);

CREATE TABLE IF NOT EXISTS refund_items (
    id                     INTEGER PRIMARY KEY AUTOINCREMENT,
    refund_id              INTEGER NOT NULL REFERENCES refunds (id) ON DELETE CASCADE,
    sale_item_id           INTEGER NOT NULL REFERENCES sale_items (id) ON DELETE RESTRICT,
    qty_refunded           INTEGER NOT NULL CHECK (qty_refunded > 0),
    amount_refunded_minor  INTEGER NOT NULL CHECK (amount_refunded_minor >= 0)
);

CREATE INDEX IF NOT EXISTS idx_refund_items_refund     ON refund_items (refund_id);
-- "how much of this sale_item has already been refunded" (partial-refund
-- guard) is a lookup on this column.
CREATE INDEX IF NOT EXISTS idx_refund_items_sale_item  ON refund_items (sale_item_id);

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
