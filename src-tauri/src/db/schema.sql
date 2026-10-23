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
--   Orders (restaurant order management — see the `orders` section below)
--     tables 1──* orders 1──* order_items *──1 items
--     orders 1──* sales (sales.order_id; split bill = several sales)
--     order_items *──?1 sales (order_items.sale_id: which bill paid it)
--     orders 1──* tokens 1──* token_items *──?1 order_items
--     orders 1──* order_status_events
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
    -- Unused as of the switch to calendar-based salary calculation (see
    -- db/salary.rs) — salary now divides by the actual number of days in
    -- the month being paid (28/29/30/31), not a fixed configured divisor.
    -- Column kept only because migrations are additive-only (CLAUDE.md) and
    -- an existing install may already have a row with this column set; no
    -- code reads or writes it anymore.
    working_days_per_month INTEGER NOT NULL DEFAULT 26 CHECK (working_days_per_month > 0),
    -- Set once the first-time setup wizard (Phase 14) finishes. A fresh
    -- install always seeds a row here (see `seed_app_config` in schema.rs) so
    -- "app_config is empty" can't be tested with a row-existence check — this
    -- flag is what actually distinguishes "never configured" from "configured
    -- with defaults nobody's looked at yet".
    onboarding_completed INTEGER NOT NULL DEFAULT 0 CHECK (onboarding_completed IN (0, 1)),
    -- UI colour theme for this install. Dark is the default because it is
    -- the product's primary visual identity (matches the marketing site).
    -- A device-level display preference, not a business setting, so it is
    -- read/written through its own `config_get_theme`/`config_set_theme`
    -- commands (any signed-in user) rather than the Owner/Admin-gated
    -- `config_update` patch — see `db::config::{get_theme, set_theme}`.
    theme           TEXT    NOT NULL DEFAULT 'dark' CHECK (theme IN ('dark', 'light')),
    -- Desktop sidebar collapsed to icon-only. Same class of preference as
    -- `theme` (device-level, any signed-in user) and read/written through
    -- the same `config_get_ui_prefs` / `config_set_sidebar_collapsed`
    -- commands. Android has no sidebar and never reads it.
    sidebar_collapsed INTEGER NOT NULL DEFAULT 0 CHECK (sidebar_collapsed IN (0, 1))
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
    -- never by the client-facing `config_toggle_module` command. When true for a
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
    role       TEXT    NOT NULL CHECK (role IN ('owner', 'admin', 'cashier', 'delivery')),
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

-- ---------------------------------------------------------------------------
-- Licence status — the single-row cache of the server-signed licence payload
-- (see src/license/mod.rs). Deliberately stores the SIGNED BYTES, not the
-- fields inside them: every launch re-verifies `payload_b64` under the
-- compiled-in public key and re-derives status/expiry from the result, so
-- editing this row buys nothing (the signature stops matching) and there is
-- never a "parsed" copy that could drift from the signed one.
--
-- The three INTEGER clocks are unix seconds and exist for the offline
-- clock-rollback and check-in checks (src/license/state.rs):
--   server_issued_at     — `issued_at` from the payload; local time must never
--                          be earlier than this, and a newer payload must
--                          never be replaced by an older one
--   high_water_local_ts  — the latest local time this install has observed;
--                          a launch that finds the clock behind it by more
--                          than a small tolerance is treated as tampering
--   last_online_check    — local time of the last successful refresh from
--                          the server; the payload's max_offline_days runs
--                          from here
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS license_status (
    id                  INTEGER PRIMARY KEY CHECK (id = 1),
    license_key         TEXT    NOT NULL,
    payload_b64         TEXT    NOT NULL,
    signature_b64       TEXT    NOT NULL,
    kid                 TEXT    NOT NULL,
    server_issued_at    INTEGER NOT NULL,
    high_water_local_ts INTEGER NOT NULL,
    last_online_check   INTEGER NOT NULL,
    updated_at          TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);

-- ---------------------------------------------------------------------------
-- pos_sync_meta — cloud synchronization tracking and server account ID.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS pos_sync_meta (
    id                  INTEGER PRIMARY KEY CHECK (id = 1),
    last_synced_sale_id INTEGER NOT NULL DEFAULT 0,
    last_synced_at      TEXT,
    server_customer_id  INTEGER,
    updated_at          TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
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

-- Kitchen/preparation counters (KOT tokens, Tables module) — physical
-- stations ("Channa Counter", "Tandoor", "Drinks Counter"), NOT the same
-- concept as `categories` above: categories group items for browsing on the
-- billing screen, counters group items by where they're actually prepared.
-- A client may serve one category from several counters or vice versa, so
-- these are deliberately separate tables, never coupled. Defined here,
-- before `items`, so `items.counter_id` (an `ADDED_COLUMNS` entry — see
-- `schema.rs`) can reference it.
CREATE TABLE IF NOT EXISTS counters (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT    NOT NULL UNIQUE,
    is_active  INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
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
    -- REAL, not INTEGER: a `sold_by_amount` item's stock decrements by a
    -- fractional quantity (e.g. 0.4 kg). SQLite's dynamic typing already
    -- stores a REAL correctly in an INTEGER-affinity column (verified: it
    -- only coerces to INTEGER when the value is losslessly convertible), so
    -- this declared-type change is purely documentation for fresh installs
    -- — it does not require (and is not paired with) a migration step for
    -- existing databases, whose already-created columns keep accepting
    -- fractional values exactly the same way regardless of what's declared
    -- here. See `schema.rs`'s module doc comment for the fuller version of
    -- this reasoning.
    stock_qty           REAL    NOT NULL DEFAULT 0,
    category_id         INTEGER REFERENCES categories (id) ON DELETE SET NULL,
    low_stock_threshold INTEGER NOT NULL DEFAULT 0 CHECK (low_stock_threshold >= 0),
    -- Retire an item rather than deleting it, so its sale history survives.
    is_active           INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    -- Filename only (not a full path) of the product photo under the app's
    -- image store — see db/images.rs. Portable across reinstalls because it
    -- never encodes the app data directory itself.
    image_path           TEXT,
    -- "Sold by amount": a loose/weighed item (channa, rice, dry fruits) a
    -- cashier sells by typing a rupee amount rather than a quantity — the
    -- billing screen divides that amount by `price_minor` to get the qty.
    -- Not every item qualifies (a bottled soft drink doesn't), so this is
    -- opt-in per item, set from the Inventory add/edit form. See
    -- `db::items::Item::sold_by_amount`.
    sold_by_amount      INTEGER NOT NULL DEFAULT 0 CHECK (sold_by_amount IN (0, 1)),
    -- Display unit for a `sold_by_amount` item's quantity (e.g. "kg", "g",
    -- "ltr") — purely presentational, shown on the cart line and receipt
    -- ("0.77 kg"). NULL for a normal per-piece item, where a bare count
    -- ("x2") needs no unit label.
    unit                TEXT,
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
    -- 'split' = more than one tender, 'credit' = entirely pay-later; the
    -- per-tender breakdown of every sale lives in `sale_payments` (an
    -- install upgrading from v12 gets the widened CHECK from
    -- `schema.rs::widen_sales_payment_methods`).
    payment_method TEXT    NOT NULL DEFAULT 'cash'
                           CHECK (payment_method IN ('cash', 'card', 'other', 'platform', 'split', 'credit')),
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
    -- The order this sale paid for (some or all of its lines), if any; NULL
    -- for a plain counter sale. One order can have several sales (split
    -- bill). Added via ADDED_COLUMNS on an existing install, like shift_id.
    order_id       INTEGER REFERENCES orders (id) ON DELETE SET NULL,
    -- Amount given by customer and change returned (paisa/cents), if cash.
    tendered_minor INTEGER,
    change_minor   INTEGER,
    created_at     TEXT    NOT NULL DEFAULT (datetime('now', 'localtime')),
    -- The customer this sale was rung up for, if any — required when any
    -- part of it is on credit. Added via ADDED_COLUMNS on an existing install.
    customer_id    INTEGER REFERENCES customers (id) ON DELETE SET NULL,
    -- Part of total_minor: subtotal - discount + tax + service charge.
    service_charge_minor INTEGER NOT NULL DEFAULT 0 CHECK (service_charge_minor >= 0),
    -- 'YYYY-MM-DD' the credit (pay-later) part is due; NULL without one.
    due_date       TEXT
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
    -- REAL, not INTEGER — a `sold_by_amount` line's qty is usually
    -- fractional (see `items.stock_qty`'s comment above for why this
    -- declared-type change needs no migration for existing databases).
    qty                  REAL    NOT NULL CHECK (qty > 0),
    -- Price as of checkout — never read the live item price for an old bill.
    price_at_sale_minor  INTEGER NOT NULL CHECK (price_at_sale_minor >= 0),
    -- A cashier's free-text note on this line ("no onions"). Purely
    -- informational — never affects pricing, stock or reporting.
    notes                TEXT,
    -- items.cost_minor as of checkout, for profit reports. NULL on lines
    -- sold before this column existed — reports fall back to the live cost.
    cost_at_sale_minor   INTEGER
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
    -- REAL to match `sale_items.qty` — a refund against a `sold_by_amount`
    -- line refunds a fractional quantity too.
    qty_refunded           REAL    NOT NULL CHECK (qty_refunded > 0),
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
-- KOT (Kitchen Order Ticket) tokens — a token is a kitchen/counter
-- instruction printed when an order is taken, separate from and printed
-- well before the bill (which happens at payment, via `sales`/`sale_items`
-- as already existed). References `table_order_id`, not `table_id` —
-- tokens belong to the *order*, so `tables::shift_table_order` moving an
-- order to a different physical table carries its tokens along for free,
-- with no extra code, since nothing here ever points at a table directly.
--
-- `token_items` deliberately does NOT reference `sale_items` — no such
-- per-line row exists yet for an in-progress (unbilled) table order;
-- `table_orders.cart_json` is still a JSON blob with no stable per-line
-- identity (see that column's own comment above). Tracking "what's already
-- been sent to the kitchen" is instead a per-*item* running total against
-- `table_order_id`: `db::tokens::get_pending_token_items` sums
-- `token_items.qty_on_token` per `item_id` for a table order and subtracts
-- it from that item's current cart quantity, so a repeat order only tokens
-- the newly-added amount. This is a deliberate, disclosed deviation from a
-- literal per-line foreign key — it's simpler, and correct for what a
-- kitchen ticket actually needs ("item X, qty Y"), without requiring the
-- larger, riskier rearchitecture of promoting the parked cart to real
-- relational line items just for this feature.
--
-- `table_order_id` is nullable: a Takeaway sale has no `table_orders` row
-- at all (it's never parked the way a dine-in order is), so an "ad hoc"
-- token printed for one (`db::tokens::ad_hoc_token_groups`/`commands::
-- tokens_print_adhoc`) records with `table_order_id = NULL` — still
-- consuming the same shared daily `token_number` sequence, just with
-- nothing to attach to. (v9 installs had this column NOT NULL; see
-- `db::schema::relax_tokens_table_order_id`'s guarded migration.)
CREATE TABLE IF NOT EXISTS tokens (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    -- Resets daily (see `db::tokens::next_token_number_for_today`) — unique
    -- per calendar day across the whole install, not per counter, so staff
    -- can say "token 14" unambiguously regardless of which counter it's for.
    token_number   INTEGER NOT NULL,
    table_order_id INTEGER REFERENCES table_orders (id) ON DELETE CASCADE,
    -- RESTRICT: a counter with token history can't be deleted out from under
    -- it — deactivate instead (`counters.is_active`), same convention as
    -- `sale_items.item_id`'s own RESTRICT.
    counter_id     INTEGER NOT NULL REFERENCES counters (id) ON DELETE RESTRICT,
    printed_at     TEXT    NOT NULL DEFAULT (datetime('now', 'localtime')),
    -- Kept even if the account is later removed — same rationale as
    -- sales.cashier_id/refunds.refunded_by.
    printed_by     INTEGER REFERENCES users (id) ON DELETE SET NULL,
    -- 'cancelled' is reserved for a future "void a token" action — nothing
    -- in this feature sets it yet; every token this module creates is
    -- 'printed' (the physical print already succeeded by the time the row
    -- exists at all — see `get_pending_token_items`'s doc comment on why a
    -- failed print never reaches this table).
    status         TEXT    NOT NULL DEFAULT 'printed' CHECK (status IN ('printed', 'cancelled')),
    -- The order this ticket belongs to (orders flow); NULL for tokens from
    -- the older table_orders / ad hoc paths, which the kitchen display
    -- never shows. Added via ADDED_COLUMNS on an existing install.
    order_id       INTEGER REFERENCES orders (id) ON DELETE SET NULL,
    shift_id       INTEGER REFERENCES shifts (id) ON DELETE SET NULL,
    -- Kitchen-display progress of this ticket at its counter.
    kitchen_status TEXT    NOT NULL DEFAULT 'queued'
                           CHECK (kitchen_status IN ('queued', 'preparing', 'ready')),
    ready_at       TEXT
);

CREATE INDEX IF NOT EXISTS idx_tokens_table_order ON tokens (table_order_id);
-- `next_token_number_for_shift_or_today` scans by shift_id or date(printed_at) for max.
CREATE INDEX IF NOT EXISTS idx_tokens_printed_at  ON tokens (printed_at);

CREATE TABLE IF NOT EXISTS token_items (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    token_id      INTEGER NOT NULL REFERENCES tokens (id) ON DELETE CASCADE,
    -- RESTRICT, matching sale_items.item_id — an item with token history
    -- can't be hard-deleted, only archived.
    item_id       INTEGER NOT NULL REFERENCES items (id) ON DELETE RESTRICT,
    -- REAL, matching `sale_items.qty`/`items.stock_qty` — a `sold_by_amount`
    -- item's token quantity is a real-world weight too (e.g. "0.77 kg"),
    -- not necessarily a whole number.
    qty_on_token  REAL    NOT NULL CHECK (qty_on_token > 0),
    -- The exact order line this ticket line came from (orders flow); NULL
    -- for the older per-item-diff tokens. Added via ADDED_COLUMNS.
    order_item_id INTEGER REFERENCES order_items (id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_token_items_token ON token_items (token_id);
CREATE INDEX IF NOT EXISTS idx_token_items_item  ON token_items (item_id);
-- Indexes on sales.order_id, tokens.order_id, tokens.shift_id, orders.shift_id
-- and token_items.order_item_id live in schema.rs's ADDED_COLUMN_INDEXES (an
-- upgrading install only has those columns after add_missing_columns runs).

-- ---------------------------------------------------------------------------
-- Delivery platforms (Foodpanda, Bykea, in-house, etc.) for third-party deliveries
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS delivery_platforms (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT    NOT NULL UNIQUE,
    is_active  INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);

-- ---------------------------------------------------------------------------
-- Orders (restaurant order management). An order runs on two independent
-- tracks: fulfillment (`orders.status`, below) and payment, which is never
-- stored — an order is paid when every non-voided `order_items` row has a
-- `sale_id`. Completed = paid AND served; `completed_at` is stamped when
-- both first hold. Sales stay the money ledger: paying lines is an ordinary
-- `create_sale` whose `sales.order_id` points back here, so receipts,
-- refunds and shift reconciliation are unchanged. Replaces `table_orders`
-- (and its `cart_json` stand-in) for new orders.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS orders (
    id                      INTEGER PRIMARY KEY AUTOINCREMENT,
    -- Shown to staff and printed on tickets; resets daily or per shift.
    order_no                INTEGER NOT NULL CHECK (order_no > 0),
    order_type              TEXT    NOT NULL
                            CHECK (order_type IN ('dine_in', 'takeaway', 'delivery', 'third_party_delivery')),
    table_id                INTEGER REFERENCES tables (id) ON DELETE SET NULL,
    status                  TEXT    NOT NULL DEFAULT 'draft'
                            CHECK (status IN ('draft', 'sent', 'preparing', 'ready',
                                              'served', 'held', 'cancelled')),
    -- What `held` resumes to; set only while status = 'held'.
    held_from_status        TEXT    CHECK (held_from_status IN ('draft', 'sent', 'preparing',
                                                                 'ready', 'served')),
    -- Bumped on every change; a write naming a stale version is refused, so
    -- two devices can never silently overwrite each other.
    version                 INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    -- Client-generated UUID, made once per order: a retried create returns
    -- the existing order instead of a duplicate.
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

-- The active-orders board filters on status.
CREATE INDEX IF NOT EXISTS idx_orders_status     ON orders (status);
CREATE INDEX IF NOT EXISTS idx_orders_table      ON orders (table_id);
CREATE INDEX IF NOT EXISTS idx_orders_created_at ON orders (created_at);

CREATE TABLE IF NOT EXISTS order_items (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    order_id             INTEGER NOT NULL REFERENCES orders (id) ON DELETE CASCADE,
    -- RESTRICT, matching sale_items.item_id — archive, never delete.
    item_id              INTEGER NOT NULL REFERENCES items (id) ON DELETE RESTRICT,
    -- REAL, matching sale_items.qty (sold_by_amount lines are fractional).
    qty                  REAL    NOT NULL CHECK (qty > 0),
    -- Copied from the live item by the server when the line is added — never
    -- sent by the client — so paying can copy it as price_at_sale_minor.
    price_at_order_minor INTEGER NOT NULL CHECK (price_at_order_minor >= 0),
    notes                TEXT,
    -- When the kitchen was told about this line (Send); NULL = not sent yet,
    -- so it may still be changed or removed freely. Set even for an item
    -- with no counter, which never appears on a token.
    sent_at              TEXT,
    -- A line the kitchen was told about is voided, never deleted.
    status               TEXT    NOT NULL DEFAULT 'active'
                                 CHECK (status IN ('active', 'voided')),
    voided_by            INTEGER REFERENCES users (id) ON DELETE SET NULL,
    void_reason          TEXT,
    voided_at            TEXT,
    -- The sale that paid this line; NULL = unpaid. RESTRICT for the same
    -- historic-immutability reason refunds.original_sale_id is.
    sale_id              INTEGER REFERENCES sales (id) ON DELETE RESTRICT,
    added_by             INTEGER REFERENCES users (id) ON DELETE SET NULL,
    added_at             TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE INDEX IF NOT EXISTS idx_order_items_order ON order_items (order_id);
CREATE INDEX IF NOT EXISTS idx_order_items_sale  ON order_items (sale_id);
CREATE INDEX IF NOT EXISTS idx_order_items_item  ON order_items (item_id);

-- Append-only: one row per fulfillment status change (from_status NULL on
-- creation). Feeds the board's timers, the history timeline and the void
-- audit trail.
CREATE TABLE IF NOT EXISTS order_status_events (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    order_id    INTEGER NOT NULL REFERENCES orders (id) ON DELETE CASCADE,
    from_status TEXT,
    to_status   TEXT    NOT NULL,
    user_id     INTEGER REFERENCES users (id) ON DELETE SET NULL,
    at          TEXT    NOT NULL DEFAULT (datetime('now', 'localtime')),
    detail      TEXT
);

CREATE INDEX IF NOT EXISTS idx_order_status_events_order ON order_status_events (order_id);

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

-- ---------------------------------------------------------------------------
-- Customers and pay-later (credit) sales
--
--   customers 1──* sales (sales.customer_id)
--   sales 1──* sale_payments          one row per tender; method 'credit' is
--                                     the pay-later part, owed by the sale's
--                                     customer until customer_payments cover it
--   customers 1──* customer_payments *──?1 sales (installments; sale_id NULL
--                                     = on account, applied oldest-due first)
--
-- A sale's outstanding balance is derived, never stored: its credit part
-- minus installments against it minus refunds (applied to credit first).
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS customers (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT    NOT NULL,
    phone      TEXT,
    email      TEXT,
    address    TEXT,
    notes      TEXT,
    is_active  INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE INDEX IF NOT EXISTS idx_customers_name  ON customers (name);
CREATE INDEX IF NOT EXISTS idx_customers_phone ON customers (phone);

CREATE TABLE IF NOT EXISTS sale_payments (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    sale_id      INTEGER NOT NULL REFERENCES sales (id) ON DELETE CASCADE,
    method       TEXT    NOT NULL
                         CHECK (method IN ('cash', 'card', 'other', 'platform', 'credit')),
    amount_minor INTEGER NOT NULL CHECK (amount_minor > 0)
);

CREATE INDEX IF NOT EXISTS idx_sale_payments_sale ON sale_payments (sale_id);

CREATE TABLE IF NOT EXISTS customer_payments (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    customer_id  INTEGER NOT NULL REFERENCES customers (id) ON DELETE RESTRICT,
    sale_id      INTEGER REFERENCES sales (id) ON DELETE RESTRICT,
    amount_minor INTEGER NOT NULL CHECK (amount_minor > 0),
    method       TEXT    NOT NULL CHECK (method IN ('cash', 'card', 'other')),
    note         TEXT,
    received_by  INTEGER REFERENCES users (id) ON DELETE SET NULL,
    -- The receiver's open shift, so cash collected lands in that drawer's
    -- reconciliation.
    shift_id     INTEGER REFERENCES shifts (id) ON DELETE SET NULL,
    created_at   TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE INDEX IF NOT EXISTS idx_customer_payments_customer ON customer_payments (customer_id);
CREATE INDEX IF NOT EXISTS idx_customer_payments_sale     ON customer_payments (sale_id);
CREATE INDEX IF NOT EXISTS idx_customer_payments_shift    ON customer_payments (shift_id);

-- ---------------------------------------------------------------------------
-- Purchasing (suppliers, purchase orders, goods received notes)
--
--   suppliers 1──* purchase_orders 1──* purchase_order_items *──1 items
--   suppliers 1──* goods_receipts 1──* goods_receipt_items *──1 items
--   goods_receipts *──?1 purchase_orders (a GRN may receive against a PO or
--                                         stand alone)
--   suppliers 1──* supplier_payments
--
-- A GRN is what moves stock: each line adds qty to items.stock_qty, updates
-- items.cost_minor to the latest unit cost, and writes a stock_movements row.
-- Supplier balance (payable) = GRN totals - supplier payments.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS suppliers (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    name           TEXT    NOT NULL UNIQUE,
    contact_person TEXT,
    phone          TEXT,
    email          TEXT,
    address        TEXT,
    notes          TEXT,
    is_active      INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at     TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE TABLE IF NOT EXISTS purchase_orders (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    supplier_id   INTEGER NOT NULL REFERENCES suppliers (id) ON DELETE RESTRICT,
    -- draft -> ordered -> partial -> received; draft/ordered/partial -> cancelled.
    status        TEXT    NOT NULL DEFAULT 'draft'
                          CHECK (status IN ('draft', 'ordered', 'partial', 'received', 'cancelled')),
    order_date    TEXT    NOT NULL,
    expected_date TEXT,
    notes         TEXT,
    created_by    INTEGER REFERENCES users (id) ON DELETE SET NULL,
    created_at    TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE INDEX IF NOT EXISTS idx_purchase_orders_supplier ON purchase_orders (supplier_id);
CREATE INDEX IF NOT EXISTS idx_purchase_orders_status   ON purchase_orders (status);

CREATE TABLE IF NOT EXISTS purchase_order_items (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    po_id           INTEGER NOT NULL REFERENCES purchase_orders (id) ON DELETE CASCADE,
    item_id         INTEGER NOT NULL REFERENCES items (id) ON DELETE RESTRICT,
    qty_ordered     REAL    NOT NULL CHECK (qty_ordered > 0),
    unit_cost_minor INTEGER NOT NULL CHECK (unit_cost_minor >= 0),
    qty_received    REAL    NOT NULL DEFAULT 0 CHECK (qty_received >= 0)
);

CREATE INDEX IF NOT EXISTS idx_purchase_order_items_po   ON purchase_order_items (po_id);
CREATE INDEX IF NOT EXISTS idx_purchase_order_items_item ON purchase_order_items (item_id);

CREATE TABLE IF NOT EXISTS goods_receipts (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    supplier_id      INTEGER NOT NULL REFERENCES suppliers (id) ON DELETE RESTRICT,
    po_id            INTEGER REFERENCES purchase_orders (id) ON DELETE SET NULL,
    received_date    TEXT    NOT NULL,
    -- The supplier's own invoice/delivery-note number.
    invoice_ref      TEXT,
    notes            TEXT,
    total_cost_minor INTEGER NOT NULL CHECK (total_cost_minor >= 0),
    received_by      INTEGER REFERENCES users (id) ON DELETE SET NULL,
    created_at       TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE INDEX IF NOT EXISTS idx_goods_receipts_supplier ON goods_receipts (supplier_id);
CREATE INDEX IF NOT EXISTS idx_goods_receipts_date     ON goods_receipts (received_date);

CREATE TABLE IF NOT EXISTS goods_receipt_items (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    grn_id          INTEGER NOT NULL REFERENCES goods_receipts (id) ON DELETE CASCADE,
    item_id         INTEGER NOT NULL REFERENCES items (id) ON DELETE RESTRICT,
    po_item_id      INTEGER REFERENCES purchase_order_items (id) ON DELETE SET NULL,
    qty             REAL    NOT NULL CHECK (qty > 0),
    unit_cost_minor INTEGER NOT NULL CHECK (unit_cost_minor >= 0),
    batch_no        TEXT,
    -- 'YYYY-MM-DD'; feeds the product expiry report.
    expiry_date     TEXT
);

CREATE INDEX IF NOT EXISTS idx_goods_receipt_items_grn    ON goods_receipt_items (grn_id);
CREATE INDEX IF NOT EXISTS idx_goods_receipt_items_item   ON goods_receipt_items (item_id);
CREATE INDEX IF NOT EXISTS idx_goods_receipt_items_expiry ON goods_receipt_items (expiry_date);

CREATE TABLE IF NOT EXISTS supplier_payments (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    supplier_id  INTEGER NOT NULL REFERENCES suppliers (id) ON DELETE RESTRICT,
    amount_minor INTEGER NOT NULL CHECK (amount_minor > 0),
    method       TEXT    NOT NULL CHECK (method IN ('cash', 'card', 'bank', 'other')),
    reference    TEXT,
    note         TEXT,
    paid_date    TEXT    NOT NULL,
    created_by   INTEGER REFERENCES users (id) ON DELETE SET NULL,
    created_at   TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE INDEX IF NOT EXISTS idx_supplier_payments_supplier ON supplier_payments (supplier_id);

-- ---------------------------------------------------------------------------
-- Stock movements and stock takes
--
-- stock_movements is the audit trail for every stock change that is not a
-- sale or a refund (those have their own ledgers): goods received, manual
-- additions, damage write-offs, corrections and stock-take variances.
-- qty_change is signed (+ in, - out).
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS stock_movements (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    item_id      INTEGER NOT NULL REFERENCES items (id) ON DELETE CASCADE,
    qty_change   REAL    NOT NULL,
    reason       TEXT    NOT NULL
                         CHECK (reason IN ('grn', 'add', 'damage', 'correction', 'stock_take')),
    -- goods_receipts.id for 'grn', stock_takes.id for 'stock_take'.
    reference_id INTEGER,
    note         TEXT,
    user_id      INTEGER REFERENCES users (id) ON DELETE SET NULL,
    created_at   TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE INDEX IF NOT EXISTS idx_stock_movements_item       ON stock_movements (item_id);
CREATE INDEX IF NOT EXISTS idx_stock_movements_created_at ON stock_movements (created_at);

CREATE TABLE IF NOT EXISTS stock_takes (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    note       TEXT,
    created_by INTEGER REFERENCES users (id) ON DELETE SET NULL,
    created_at TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE TABLE IF NOT EXISTS stock_take_lines (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    stock_take_id   INTEGER NOT NULL REFERENCES stock_takes (id) ON DELETE CASCADE,
    item_id         INTEGER NOT NULL REFERENCES items (id) ON DELETE CASCADE,
    expected_qty    REAL    NOT NULL,
    counted_qty     REAL    NOT NULL CHECK (counted_qty >= 0),
    -- items.cost_minor at count time, so the variance value stays fixed.
    unit_cost_minor INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_stock_take_lines_take ON stock_take_lines (stock_take_id);
