//! All `#[tauri::command]` entry points.
//!
//! Commands stay thin: validate input, delegate to `db`, and map errors to
//! `String` (which surfaces on the JS side as a rejected promise that the
//! service layer wraps in a `TauriCommandError`).
//!
//! Naming: feature-module commands are prefixed with their module
//! (`inventory_add_item`, `billing_create_sale`); the cross-cutting config and
//! auth commands below use bare verbs.

use tauri::{AppHandle, Manager, State};

use crate::db::attendance::{AttendanceRecord, Employee, MonthlySummary};
use crate::db::config::{AppConfig, AppConfigUpdate};
use crate::db::dashboard::DashboardSummary;
use crate::db::expenses::{CategoryTotal, Expense};
use crate::db::salary::SalaryCalculation;
use crate::db::items::{Category, DeleteOutcome, Item, ItemInput, ItemQuery};
use crate::db::modules::{ModuleState, Platform};
use crate::db::reports::{DailySales, SalesSummary, TopItem, TopItemSort};
use crate::db::sales::{CreateSaleInput, Sale};
use crate::db::tables::{ParkedCartLine, ParkedOrder, TableSummary};
use crate::db::users::{ManagedUser, Role, User};
use crate::db::{attendance, config, dashboard, expenses, items, modules, reports, salary, sales, tables, users, Db};
use crate::images;
use crate::session::{require_role, Session};

/// Owner and Admin are this product's two "full access" roles — see
/// `src/utils/permissions.ts` for the frontend mirror of this matrix, and
/// the module doc comment on `session.rs` for why the check has to live here
/// too and not just there. There is deliberately no third privileged tier;
/// Cashier is the only other role, and it never appears in this list.
const STAFF_ROLES: &[Role] = &[Role::Owner, Role::Admin];

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

/// Health check used by `tauriClient.ping()` to prove the IPC bridge is live.
#[tauri::command]
pub fn app_ping() -> String {
    "pong".to_string()
}

/// The user tables currently present. Used to verify a migration landed.
#[tauri::command]
pub fn app_db_tables(db: State<'_, Db>) -> Result<Vec<String>, String> {
    db.with_conn(crate::db::schema::table_names)
        .map_err(|e| e.to_string())
}

/// Current SQLite schema version, as written by the migration runner.
#[tauri::command]
pub fn app_db_version(db: State<'_, Db>) -> Result<i64, String> {
    db.with_conn(|conn| conn.query_row("PRAGMA user_version", [], |row| row.get(0)))
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// App config
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_app_config(db: State<'_, Db>) -> Result<AppConfig, String> {
    db.with_conn(config::get).map_err(|e| e.to_string())
}

/// Settings-only write — Owner/Admin, same as the screen it backs.
#[tauri::command]
pub fn update_app_config(
    db: State<'_, Db>,
    session: State<'_, Session>,
    patch: AppConfigUpdate,
) -> Result<AppConfig, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| config::update(conn, patch))
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Module configuration
// ---------------------------------------------------------------------------

fn parse_platform(platform: &str) -> Result<Platform, String> {
    Platform::parse(platform).ok_or_else(|| format!("unknown platform: {}", platform))
}

/// Every module with its visibility for `platform`. The sidebar filters on
/// `enabled`; the Settings screen uses the full list to render toggles.
#[tauri::command]
pub fn get_enabled_modules(
    db: State<'_, Db>,
    platform: String,
) -> Result<Vec<ModuleState>, String> {
    let platform = parse_platform(&platform)?;
    db.with_conn(|conn| modules::list(conn, platform))
        .map_err(|e| e.to_string())
}

/// Settings-only write — Owner/Admin, same as the screen it backs.
#[tauri::command]
pub fn toggle_module(
    db: State<'_, Db>,
    session: State<'_, Session>,
    module_key: String,
    platform: String,
    enabled: bool,
) -> Result<Vec<ModuleState>, String> {
    require_role(&session, STAFF_ROLES)?;
    let platform = parse_platform(&platform)?;

    // `with_conn` surfaces the domain error (core module, unknown key) as a
    // string so the Settings screen can show it verbatim.
    db.with_conn(|conn| {
        Ok(modules::set_enabled(conn, &module_key, platform, enabled)
            .map_err(|e| e.to_string())
            .and_then(|()| modules::list(conn, platform).map_err(|e| e.to_string())))
    })
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// Inventory
// ---------------------------------------------------------------------------

/// Items matching the search/category filter. Archived items are excluded
/// unless the caller asks for them.
#[tauri::command]
pub fn inventory_get_items(db: State<'_, Db>, query: ItemQuery) -> Result<Vec<Item>, String> {
    db.with_conn(|conn| items::list_items(conn, &query))
        .map_err(|e| e.to_string())
}

/// Write — Cashiers get read-only Inventory (see `permissions.ts`), so
/// this and every other inventory write below require Owner/Admin.
#[tauri::command]
pub fn inventory_add_item(
    db: State<'_, Db>,
    session: State<'_, Session>,
    input: ItemInput,
) -> Result<Item, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(items::add_item(conn, input)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Replaces the full editable row. If this swaps in a different photo (or
/// clears one), the previous image file — if any — is deleted from disk after
/// the database write succeeds, so a failed update never orphans an old photo
/// and a successful one never leaves the replaced one behind.
#[tauri::command]
pub fn inventory_update_item(
    app: AppHandle,
    db: State<'_, Db>,
    session: State<'_, Session>,
    id: i64,
    input: ItemInput,
) -> Result<Item, String> {
    require_role(&session, STAFF_ROLES)?;
    let previous_image = db
        .with_conn(|conn| items::get_item(conn, id).map_err(|_| rusqlite::Error::QueryReturnedNoRows))
        .ok()
        .and_then(|item| item.image_path);

    let updated = db
        .with_conn(|conn| Ok(items::update_item(conn, id, input)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

    if let Some(old_image) = previous_image {
        if updated.image_path.as_deref() != Some(old_image.as_str()) {
            images::delete_image(&images_dir(&app)?, &old_image);
        }
    }

    Ok(updated)
}

/// Deletes the item outright, unless it has sale history — in that case it is
/// archived (`is_active = 0`) instead so past reports stay accurate, and its
/// photo is left in place. The returned outcome tells the frontend which one
/// happened. A photo is only ever removed from disk when its item is actually
/// deleted (never on archive), since an archived item can still be looked up.
#[tauri::command]
pub fn inventory_delete_item(
    app: AppHandle,
    db: State<'_, Db>,
    session: State<'_, Session>,
    id: i64,
) -> Result<DeleteOutcome, String> {
    require_role(&session, STAFF_ROLES)?;
    let existing_image = db
        .with_conn(|conn| items::get_item(conn, id).map_err(|_| rusqlite::Error::QueryReturnedNoRows))
        .ok()
        .and_then(|item| item.image_path);

    let outcome = db
        .with_conn(|conn| Ok(items::delete_item(conn, id)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

    if outcome == DeleteOutcome::Deleted {
        if let Some(image) = existing_image {
            images::delete_image(&images_dir(&app)?, &image);
        }
    }

    Ok(outcome)
}

fn images_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(images::images_dir(&data_dir))
}

/// Validates, saves and returns the filename to store as `Item.imagePath`.
/// The frontend reads the picked file with `FileReader`, so bytes arrive here
/// already base64-encoded — no filesystem-access plugin/capability needed.
#[tauri::command]
pub fn inventory_upload_image(
    app: AppHandle,
    session: State<'_, Session>,
    data_base64: String,
    extension: String,
) -> Result<String, String> {
    require_role(&session, STAFF_ROLES)?;
    images::save_image(&images_dir(&app)?, &data_base64, &extension).map_err(|e| e.to_string())
}

/// Reads a stored image back as a `data:` URL for direct use as an `<img src>`.
#[tauri::command]
pub fn inventory_get_image(app: AppHandle, file_name: String) -> Result<String, String> {
    images::read_image_data_url(&images_dir(&app)?, &file_name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn inventory_get_categories(db: State<'_, Db>) -> Result<Vec<Category>, String> {
    db.with_conn(items::list_categories).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn inventory_add_category(
    db: State<'_, Db>,
    session: State<'_, Session>,
    name: String,
) -> Result<Category, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(items::add_category(conn, &name)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

/// Accounts to offer on the login screen. Never includes PIN hashes. Called
/// before anyone is signed in, so — unlike everything below it — this one
/// deliberately has no `require_role` check.
#[tauri::command]
pub fn get_users(db: State<'_, Db>) -> Result<Vec<User>, String> {
    db.with_conn(users::list_active).map_err(|e| e.to_string())
}

/// The only place `Session` is ever written to from a value the JS side
/// supplied — and even here, `users::authenticate` (an argon2 check against
/// the stored hash) is what decides success, not the caller's say-so.
#[tauri::command]
pub fn login(db: State<'_, Db>, session: State<'_, Session>, user_id: i64, pin: String) -> Result<User, String> {
    let user = db
        .with_conn(|conn| Ok(users::authenticate(conn, user_id, &pin)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    session.set(user.clone());
    Ok(user)
}

/// Clears the server-side session. Safe to call whether or not anyone was
/// signed in.
#[tauri::command]
pub fn logout(session: State<'_, Session>) {
    session.clear();
}

/// Every account, active or not — the user management screen. Owner/Admin
/// only, like every other command in this block from here down.
#[tauri::command]
pub fn get_all_users(db: State<'_, Db>, session: State<'_, Session>) -> Result<Vec<ManagedUser>, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(users::list_all).map_err(|e| e.to_string())
}

/// Creates a staff account. Only an Owner may create another Owner account —
/// an Admin creating a peer Owner would be a privilege escalation the UI
/// alone can't be trusted to prevent, so it's checked here against the
/// caller `require_role` actually returned, not a role the client claims.
#[tauri::command]
pub fn create_user(
    db: State<'_, Db>,
    session: State<'_, Session>,
    name: String,
    pin: String,
    role: Role,
) -> Result<User, String> {
    let caller = require_role(&session, STAFF_ROLES)?;
    if matches!(role, Role::Owner) && !matches!(caller.role, Role::Owner) {
        return Err("Only an Owner can create another Owner account".to_string());
    }
    db.with_conn(|conn| Ok(users::create(conn, &name, &pin, role)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Renames a staff account and/or changes their role. Same Owner-creating-
/// Owner restriction as `create_user` applies to promotions.
#[tauri::command]
pub fn update_user(
    db: State<'_, Db>,
    session: State<'_, Session>,
    user_id: i64,
    name: String,
    role: Role,
) -> Result<User, String> {
    let caller = require_role(&session, STAFF_ROLES)?;
    if matches!(role, Role::Owner) && !matches!(caller.role, Role::Owner) {
        return Err("Only an Owner can promote someone to Owner".to_string());
    }
    db.with_conn(|conn| Ok(users::update(conn, user_id, &name, role)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Deactivates or reactivates a staff account. Refuses to let the last
/// active Owner deactivate themselves out of the system, and refuses to let
/// anyone deactivate the account they're currently signed in as — both
/// checked here, not left to the UI to remember to disable a button.
#[tauri::command]
pub fn set_user_active(
    db: State<'_, Db>,
    session: State<'_, Session>,
    user_id: i64,
    is_active: bool,
) -> Result<(), String> {
    let caller = require_role(&session, STAFF_ROLES)?;
    if !is_active && user_id == caller.id {
        return Err("You can't deactivate your own account".to_string());
    }
    db.with_conn(|conn| Ok(users::set_active(conn, user_id, is_active)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Resets a staff member's PIN. Owner/Admin only — this is the management
/// screen's "reset PIN" action, not self-service.
#[tauri::command]
pub fn set_user_pin(
    db: State<'_, Db>,
    session: State<'_, Session>,
    user_id: i64,
    pin: String,
) -> Result<(), String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(users::set_pin(conn, user_id, &pin)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Billing
// ---------------------------------------------------------------------------

/// Active items matching `query` by name or barcode, exact-barcode-first, for
/// the billing search bar. An empty query returns no results — the cashier
/// must type or scan something rather than being handed the whole catalogue.
#[tauri::command]
pub fn billing_search_items(db: State<'_, Db>, query: String) -> Result<Vec<Item>, String> {
    db.with_conn(|conn| items::search_items(conn, &query, 20))
        .map_err(|e| e.to_string())
}

/// Completes a sale: re-prices every line against live stock, writes
/// `sales` + `sale_items`, decrements stock, and (if a table was given)
/// closes that table's parked order and frees the table — all inside one
/// SQLite transaction, so a failure at any step leaves nothing behind.
#[tauri::command]
pub fn billing_create_sale(db: State<'_, Db>, input: CreateSaleInput) -> Result<Sale, String> {
    db.with_transaction(|tx| Ok(sales::create_sale(tx, input)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Fetches a completed sale — used for receipt reprint.
#[tauri::command]
pub fn billing_get_sale(db: State<'_, Db>, id: i64) -> Result<Sale, String> {
    db.with_conn(|conn| Ok(sales::get_sale(conn, id)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Attempts to print a receipt on a thermal printer. Always fails with a
/// friendly "not configured" message until real hardware is wired up in
/// `printer::escpos::send_to_printer` — the PDF receipt (built entirely in
/// the frontend with jsPDF) is what actually gets used until then.
#[tauri::command]
pub fn billing_print_receipt_thermal(db: State<'_, Db>, sale_id: i64) -> Result<(), String> {
    let sale = db
        .with_conn(|conn| Ok(sales::get_sale(conn, sale_id)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let app_config = db.with_conn(config::get).map_err(|e| e.to_string())?;

    crate::printer::escpos::print_receipt(&sale, &app_config).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Reports
// ---------------------------------------------------------------------------

/// Total sales, transaction count and average sale value for `start_date`..
/// `end_date` (inclusive, `YYYY-MM-DD`). Reports is not in a Cashier's module
/// list (see `permissions.ts`), so this and the other two report queries
/// below are Owner/Admin only — a cashier hitting this command directly
/// (bypassing the UI, which already hides the whole screen) still gets
/// refused here.
#[tauri::command]
pub fn reports_get_sales_summary(
    db: State<'_, Db>,
    session: State<'_, Session>,
    start_date: String,
    end_date: String,
) -> Result<SalesSummary, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(reports::get_sales_summary(conn, &start_date, &end_date)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Best-selling items in the range, ranked by quantity or revenue.
#[tauri::command]
pub fn reports_get_top_items(
    db: State<'_, Db>,
    session: State<'_, Session>,
    start_date: String,
    end_date: String,
    limit: i64,
    sort_by: TopItemSort,
) -> Result<Vec<TopItem>, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(reports::get_top_items(conn, &start_date, &end_date, limit, sort_by)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// One row per calendar day in the range (zero-filled where there were no
/// sales), for the reports chart. Also used by the Dashboard's trend chart —
/// fine to share the same gate, since Dashboard is Owner/Admin-only too.
#[tauri::command]
pub fn reports_get_sales_over_time(
    db: State<'_, Db>,
    session: State<'_, Session>,
    start_date: String,
    end_date: String,
) -> Result<Vec<DailySales>, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(reports::get_sales_over_time(conn, &start_date, &end_date)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Tables (restaurant floor view — only called when the `tables` module is
// enabled; the frontend hides all table UI otherwise)
// ---------------------------------------------------------------------------

/// Every table with its status and whether it has a cart parked on it, for
/// the floor view and the billing screen's table picker.
#[tauri::command]
pub fn tables_get_tables(db: State<'_, Db>) -> Result<Vec<TableSummary>, String> {
    db.with_conn(tables::list_tables).map_err(|e| e.to_string())
}

/// Adds a new physical table to the floor. Owner/admin only, like the rest
/// of the floor-management screen these belong to — the `tables` module is
/// not in a Cashier's list (see `permissions.ts`); the cart-level actions
/// below (`attach_cart_to_table`/`get_parked_cart`) stay open because those
/// are the ones a Cashier's Billing screen calls directly.
#[tauri::command]
pub fn tables_add_table(
    db: State<'_, Db>,
    session: State<'_, Session>,
    name: String,
    seats: i64,
) -> Result<TableSummary, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(tables::add_table(conn, &name, seats)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Directly sets a table's status (e.g. marking one reserved for a booking).
#[tauri::command]
pub fn tables_update_table_status(
    db: State<'_, Db>,
    session: State<'_, Session>,
    table_id: i64,
    status: String,
) -> Result<TableSummary, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(tables::update_table_status(conn, table_id, &status)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Seats a table: starts an empty draft order (if none is already open) and
/// marks it occupied, so the billing screen has an order to attach a cart to
/// as items are added. Idempotent for a table that's already mid-order. Only
/// reachable from the floor view today, hence the same Owner/Admin gate.
#[tauri::command]
pub fn tables_assign_order_to_table(
    db: State<'_, Db>,
    session: State<'_, Session>,
    table_id: i64,
) -> Result<TableSummary, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(tables::start_table_order(conn, table_id)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Manually releases a table — cancels its open order (if any) and frees it.
/// A completed sale frees a table automatically; this is for the other
/// paths (a cancelled order, a guest who left without paying).
#[tauri::command]
pub fn tables_clear_table(
    db: State<'_, Db>,
    session: State<'_, Session>,
    table_id: i64,
) -> Result<TableSummary, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(tables::clear_table(conn, table_id)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Parks the current cart on a table ("Save to table") instead of completing
/// the sale immediately.
#[tauri::command]
pub fn tables_attach_cart_to_table(
    db: State<'_, Db>,
    table_id: i64,
    items: Vec<ParkedCartLine>,
    discount_minor: i64,
) -> Result<(), String> {
    db.with_conn(|conn| Ok(tables::attach_cart_to_table(conn, table_id, &items, discount_minor)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// The cart parked on a table, if any — used to resume billing it.
#[tauri::command]
pub fn tables_get_parked_cart(db: State<'_, Db>, table_id: i64) -> Result<Option<ParkedOrder>, String> {
    db.with_conn(|conn| Ok(tables::get_parked_cart(conn, table_id)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Attendance (only called when the `attendance` module is enabled; the
// frontend hides all attendance UI otherwise). Attendance is Owner/Admin
// only — it's payroll-adjacent staff data, and isn't in a Cashier's module
// list — so every command here checks the session before touching anything.
// ---------------------------------------------------------------------------

/// Active staff, for the check-in/out screen and the monthly summary.
#[tauri::command]
pub fn attendance_get_employees(db: State<'_, Db>, session: State<'_, Session>) -> Result<Vec<Employee>, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(attendance::list_employees).map_err(|e| e.to_string())
}

/// Checks an employee in for today. Idempotent — a repeated call the same day
/// updates the existing row rather than creating a second one.
#[tauri::command]
pub fn attendance_check_in(
    db: State<'_, Db>,
    session: State<'_, Session>,
    employee_id: i64,
) -> Result<AttendanceRecord, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(attendance::check_in(conn, employee_id)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Checks an employee out for today. Fails clearly (rather than fabricating a
/// row) if they never checked in today.
#[tauri::command]
pub fn attendance_check_out(
    db: State<'_, Db>,
    session: State<'_, Session>,
    employee_id: i64,
) -> Result<AttendanceRecord, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(attendance::check_out(conn, employee_id)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// The attendance log for a date range, optionally scoped to one employee —
/// omit `employeeId` for every employee's shifts in the range.
#[tauri::command]
pub fn attendance_get_attendance(
    db: State<'_, Db>,
    session: State<'_, Session>,
    employee_id: Option<i64>,
    start_date: String,
    end_date: String,
) -> Result<Vec<AttendanceRecord>, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(attendance::get_attendance(conn, employee_id, &start_date, &end_date)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Days present/absent and total hours per employee for `month` (`YYYY-MM`) —
/// feeds Phase 9's salary calculation.
#[tauri::command]
pub fn attendance_get_monthly_summary(
    db: State<'_, Db>,
    session: State<'_, Session>,
    month: String,
) -> Result<Vec<MonthlySummary>, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(attendance::get_monthly_summary(conn, &month)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Expenses (only called when the `expenses` module is enabled; the frontend
// hides all expense UI otherwise). Financial data — Owner/Admin only, same
// gate on every command here.
// ---------------------------------------------------------------------------

/// Logs one expense.
#[tauri::command]
pub fn expenses_add_expense(
    db: State<'_, Db>,
    session: State<'_, Session>,
    date: String,
    category: String,
    amount_minor: i64,
    note: Option<String>,
) -> Result<Expense, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(expenses::add_expense(conn, &date, &category, amount_minor, note.as_deref())))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Expenses in a date range (inclusive), optionally scoped to one category.
#[tauri::command]
pub fn expenses_get_expenses(
    db: State<'_, Db>,
    session: State<'_, Session>,
    start_date: String,
    end_date: String,
    category: Option<String>,
) -> Result<Vec<Expense>, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(expenses::get_expenses(conn, &start_date, &end_date, category.as_deref())))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Every distinct category already in use, for the quick-add form's dropdown.
#[tauri::command]
pub fn expenses_get_expense_categories(
    db: State<'_, Db>,
    session: State<'_, Session>,
) -> Result<Vec<String>, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(expenses::get_categories).map_err(|e| e.to_string())
}

/// Category-wise totals for a date range, highest spend first — feeds the
/// breakdown view and, later, the dashboard's profit calculation.
#[tauri::command]
pub fn expenses_get_totals_by_category(
    db: State<'_, Db>,
    session: State<'_, Session>,
    start_date: String,
    end_date: String,
) -> Result<Vec<CategoryTotal>, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(expenses::get_totals_by_category(conn, &start_date, &end_date)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Salary (only called when the `salary` module is enabled; the frontend
// hides all salary UI otherwise). The most sensitive data in the product —
// Owner/Admin only, no exceptions.
// ---------------------------------------------------------------------------

/// `base_salary / working_days_per_month * days_present` for one employee and
/// month, refreshed against the latest attendance every call.
#[tauri::command]
pub fn salary_calculate_salary(
    db: State<'_, Db>,
    session: State<'_, Session>,
    employee_id: i64,
    month: String,
) -> Result<SalaryCalculation, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(salary::calculate_salary(conn, employee_id, &month)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// The monthly overview table: every active employee's calculated salary,
/// amount paid so far, and status for `month`.
#[tauri::command]
pub fn salary_get_monthly_overview(
    db: State<'_, Db>,
    session: State<'_, Session>,
    month: String,
) -> Result<Vec<SalaryCalculation>, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(salary::get_monthly_overview(conn, &month)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Records a payment against `month`'s salary — added to whatever has
/// already been paid that month, not a replacement (pay is often settled in
/// more than one instalment).
#[tauri::command]
pub fn salary_record_payment(
    db: State<'_, Db>,
    session: State<'_, Session>,
    employee_id: i64,
    month: String,
    paid_amount_minor: i64,
    paid_date: String,
) -> Result<SalaryCalculation, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(salary::record_payment(conn, employee_id, &month, paid_amount_minor, &paid_date)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Every month with a salary record for one employee, most recent first.
#[tauri::command]
pub fn salary_get_payment_history(
    db: State<'_, Db>,
    session: State<'_, Session>,
    employee_id: i64,
) -> Result<Vec<SalaryCalculation>, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(salary::get_payment_history(conn, employee_id)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Dashboard
// ---------------------------------------------------------------------------

/// Sales, expenses and salary payouts for a date range, scoped to whichever
/// optional modules are enabled for `platform` — a disabled module's figure
/// comes back `null`, not `0`, and is left out of `netProfitMinor` entirely,
/// so old data from a module a client has since turned off never quietly
/// re-enters their profit number. Dashboard is Owner/Admin only, same as
/// Reports and Salary, whose figures it aggregates.
#[tauri::command]
pub fn dashboard_get_summary(
    db: State<'_, Db>,
    session: State<'_, Session>,
    start_date: String,
    end_date: String,
    platform: String,
) -> Result<DashboardSummary, String> {
    require_role(&session, STAFF_ROLES)?;
    let platform = parse_platform(&platform)?;
    db.with_conn(|conn| Ok(dashboard::get_summary(conn, &start_date, &end_date, platform)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}
