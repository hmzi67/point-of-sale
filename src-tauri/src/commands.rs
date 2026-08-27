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

use crate::db::attendance::{AttendanceRecord, Employee, ManagedEmployee, MonthlySummary};
use crate::db::config::{AppConfig, AppConfigUpdate};
use crate::db::dashboard::DashboardSummary;
use crate::db::expenses::{CategoryTotal, Expense};
use crate::db::salary::SalaryCalculation;
use crate::db::items::{Category, DeleteOutcome, Item, ItemInput, ItemQuery};
use crate::db::modules::{ModuleState, Platform};
use crate::db::full_report::FullReport;
use crate::db::refunds::{CreateRefundInput, Refund, RefundLineInput, RefundableSale};
use crate::db::reports::{
    CategorySalesReport, DailySales, ProductSalesSummaryReport, RefundsSummary, SalesSummary, TableSalesSummary,
    TopItem, TopItemSort,
};
use crate::db::sales::{CreateSaleInput, Sale, SaleListItem};
use crate::db::shifts::{Shift, ShiftSummary};
use crate::db::tables::{ParkedCartLine, ParkedOrder, TableSummary};
use crate::db::tokens::{CounterPrintResult, PendingCounterGroup, PrintOutcome, TokenSummary};
use crate::db::counters::Counter;
use crate::db::users::{ManagedUser, Role, User};
use crate::db::{
    attendance, config, counters, csv_import, dashboard, expenses, full_report, items, modules, product_owner,
    refunds, reports, salary, sales, shifts, tables, tokens, users, Db,
};
use crate::images;
use crate::product_owner_session::{require_product_owner, ProductOwnerSession};
use crate::session::{require_role, Session};

/// Owner and Admin are this product's two "full access" roles — see
/// `src/utils/permissions.ts` for the frontend mirror of this matrix, and
/// the module doc comment on `session.rs` for why the check has to live here
/// too and not just there. There is deliberately no third privileged tier;
/// Cashier is the only other role, and it never appears in this list.
const STAFF_ROLES: &[Role] = &[Role::Owner, Role::Admin];

/// Roles `caller_role` may assign when creating an account or changing an
/// existing one's role — never Owner, for anyone: there is exactly one
/// Owner account per installation (seeded at first run, see
/// `db::schema::seed_owner`), so it is never a role this UI hands out.
/// Mirrors `src/utils/permissions.ts`'s `assignableRoles`.
fn assignable_roles(caller_role: Role) -> &'static [Role] {
    match caller_role {
        Role::Owner => &[Role::Admin, Role::Cashier],
        Role::Admin => &[Role::Cashier],
        Role::Cashier => &[],
    }
}

/// Whether `caller_role` may edit or deactivate an account whose *current*
/// role is `target_role` — assumes `target_role` is never Owner (that case
/// is handled separately by each caller, since editing is allowed as a
/// self-only exception while deactivating never is). Mirrors
/// `src/utils/permissions.ts`'s `roleCanManageAccount`.
fn caller_may_manage(caller_role: Role, target_role: Role) -> bool {
    match caller_role {
        Role::Owner => true,
        Role::Admin => matches!(target_role, Role::Cashier),
        Role::Cashier => false,
    }
}

/// Pure authorization decision for `create_user`, factored out of the
/// `#[tauri::command]` so the Owner/Admin hierarchy is testable without a
/// live Tauri `State`/`Session` — see the `mod tests` block below for the
/// scenarios this closes off (an Admin can never create another Admin or an
/// Owner, even by calling this command directly).
fn authorize_create_user(caller_role: Role, new_role: Role) -> Result<(), String> {
    if matches!(new_role, Role::Owner) {
        return Err("Only one Owner account is allowed on this installation".to_string());
    }
    if !assignable_roles(caller_role).contains(&new_role) {
        return Err("You don't have permission to create an account with that role".to_string());
    }
    Ok(())
}

/// Pure authorization decision for `update_user`. `target_current_role` is
/// the account's role *before* this edit; `new_role` is what the caller is
/// asking to set it to.
fn authorize_update_user(
    caller: &User,
    target_user_id: i64,
    target_current_role: Role,
    new_role: Role,
) -> Result<(), String> {
    if target_current_role == Role::Owner {
        // The Owner account (there is only ever one) may only edit itself,
        // and its role can never change — no second Owner is assignable,
        // and self-demotion would silently lock the installation out of
        // Owner-only actions.
        if target_user_id != caller.id || caller.role != Role::Owner {
            return Err("Only the Owner can edit their own account".to_string());
        }
        if new_role != Role::Owner {
            return Err("The Owner account's role can't be changed".to_string());
        }
        return Ok(());
    }

    let is_self = target_user_id == caller.id;
    if !is_self && !caller_may_manage(caller.role, target_current_role) {
        return Err("You don't have permission to manage this account".to_string());
    }
    // A no-op role (editing your own name without touching role) is always
    // fine even if that role wouldn't otherwise be assignable by you — e.g.
    // an Admin renaming themselves without becoming a Cashier.
    if new_role != target_current_role && !assignable_roles(caller.role).contains(&new_role) {
        return Err("You don't have permission to assign that role".to_string());
    }
    Ok(())
}

/// Pure authorization decision for `set_user_active`.
fn authorize_set_active(
    caller: &User,
    target_user_id: i64,
    target_current_role: Role,
    is_active: bool,
) -> Result<(), String> {
    if !is_active && target_user_id == caller.id {
        return Err("You can't deactivate your own account".to_string());
    }
    if target_current_role == Role::Owner {
        return Err("The Owner account can't be deactivated".to_string());
    }
    if target_user_id != caller.id && !caller_may_manage(caller.role, target_current_role) {
        return Err("You don't have permission to manage this account".to_string());
    }
    Ok(())
}

/// Pure authorization decision for `set_user_pin`. Changing your own PIN is
/// always allowed regardless of role; resetting someone else's follows the
/// same hierarchy as editing.
fn authorize_set_pin(caller: &User, target_user_id: i64, target_current_role: Role) -> Result<(), String> {
    if target_user_id == caller.id {
        return Ok(());
    }
    if target_current_role == Role::Owner || !caller_may_manage(caller.role, target_current_role) {
        return Err("You don't have permission to manage this account".to_string());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

/// Health check used by `tauriClient.ping()` to prove the IPC bridge is
/// live — `main.tsx` calls this unconditionally, immediately on mount, on
/// every platform, purely so its arrival in `pos-startup.log` is proof the
/// "main" window's own JS bundle actually started executing (not just that
/// the window was created — a JS error before this line, or an asset that
/// failed to resolve in a bundled build, would leave the log with "main
/// window shown" but no `app_ping` line after it, pointing squarely at the
/// frontend rather than anything in `setup()`).
#[tauri::command]
pub fn app_ping() -> String {
    crate::startup_log::log("app_ping invoked — main window's frontend JS is executing");
    "pong".to_string()
}

/// Called by the splash window's own inline script (see
/// `public/splashscreen.html`) once its content has actually painted — the
/// real "ready" signal `reveal_main_window` waits on, rather than a guess at
/// how long a freshly-created webview process takes to render its first
/// frame (a cold WKWebView/WebView2 can lag "the window exists" by a
/// meaningful fraction of a second, long enough to eat the whole splash
/// floor if that guess is wrong). Waits out whatever remains of
/// `MIN_SPLASH_MS` from process launch — almost always ~0, since painting
/// itself already took a moment — then closes the splash and reveals
/// `main`. A no-op if that already happened via `MAX_SPLASH_MS`'s ceiling
/// firing first (see `lib.rs::run`).
#[tauri::command]
pub fn splashscreen_ready(app: AppHandle) {
    // Proof the splash window's own JS actually executed (its `load`
    // listener fired and the IPC bridge reached Rust) — if a real startup
    // failure's log is missing this line entirely, the splash's content
    // itself never finished loading, which points at an asset-resolution
    // problem in the bundle rather than anything in `setup()`.
    crate::startup_log::log("splashscreen_ready invoked — splash content loaded and painted");
    let launched_at = app.state::<crate::LaunchedAt>().0;
    let remaining = crate::MIN_SPLASH_MS.saturating_sub(launched_at.elapsed().as_millis() as u64);
    if remaining > 0 {
        std::thread::sleep(std::time::Duration::from_millis(remaining));
    }
    crate::reveal_main_window(&app);
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

/// Saves an uploaded logo to disk (in its own directory, distinct from
/// product photos — see `images::logo_dir`) and points `app_config.logo_path`
/// at the generated filename, replacing whatever was there before. The
/// previous logo file — if any — is deleted from disk only after the
/// database write succeeds, so a failed upload never orphans the old logo
/// and a successful one never leaves it behind; same ordering
/// `inventory_update_item` uses for product photos, for the same reason.
#[tauri::command]
pub fn config_upload_logo(
    app: AppHandle,
    db: State<'_, Db>,
    session: State<'_, Session>,
    data_base64: String,
    extension: String,
) -> Result<AppConfig, String> {
    require_role(&session, STAFF_ROLES)?;
    let previous_logo = db.with_conn(config::get).map_err(|e| e.to_string())?.logo_path;

    let file_name = images::save_logo(&logo_dir(&app)?, &data_base64, &extension).map_err(|e| e.to_string())?;

    let updated = db
        .with_conn(|conn| {
            config::update(conn, AppConfigUpdate { logo_path: Some(file_name.clone()), ..Default::default() })
        })
        .map_err(|e| e.to_string())?;

    if let Some(old) = previous_logo {
        if old != file_name {
            images::delete_image(&logo_dir(&app)?, &old);
        }
    }

    Ok(updated)
}

/// Reads the current logo back as a `data:` URL for direct use as an
/// `<img src>` — same shape as `inventory_get_image`.
#[tauri::command]
pub fn config_get_logo(app: AppHandle, file_name: String) -> Result<String, String> {
    images::read_image_data_url(&logo_dir(&app)?, &file_name).map_err(|e| e.to_string())
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

fn logo_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(images::logo_dir(&data_dir))
}

/// Runs `f`, converting a panic into a plain error message instead of
/// letting it unwind out of a `#[tauri::command]`. Used around the
/// Bluetooth-printer commands below, which call into hand-written JNI (see
/// `printer::android_bt`'s module doc comment) — the same belt-and-braces
/// reasoning as `printer::escpos::send_to_printer`'s own `catch_unwind`,
/// applied here too since these three commands call `android_bt` directly
/// rather than through that function.
#[cfg_attr(not(target_os = "android"), allow(dead_code))] // only reached from the Android printer commands below
fn catch_panic<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
        .unwrap_or_else(|_| Err("Printer command hit an internal error".to_string()))
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

/// Bulk-loads a stock list — the Phase 14 alternative to typing in every
/// item by hand when onboarding a client who already has a spreadsheet.
/// Owner/Admin only, same as every other inventory write. Per-row failures
/// (a bad price, a duplicate barcode) don't abort the whole import — see the
/// module doc comment on `db::csv_import` for why that's the right call
/// here specifically, unlike a sale.
#[tauri::command]
pub fn inventory_import_csv(
    db: State<'_, Db>,
    session: State<'_, Session>,
    csv_content: String,
) -> Result<csv_import::ImportSummary, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(csv_import::import_csv(conn, &csv_content)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// A ready-to-download example CSV, so a shop owner knows the expected
/// column names and format before building their own file.
#[tauri::command]
pub fn inventory_csv_template() -> &'static str {
    csv_import::TEMPLATE_CSV
}

/// Item ids currently qualifying as a "best seller" — ranked by quantity
/// sold in the last `period_days`, capped at `limit`, floored at a minimum
/// total quantity so a shop with little/no sales history gets no badges at
/// all rather than an arbitrary one (see `reports::BEST_SELLER_MIN_QTY`).
/// Recomputed fresh on every call — never cached or stored — so the
/// Inventory/Billing "fire" badge can never go stale. Read-only, so this has
/// no role check, same as `inventory_get_items`.
#[tauri::command]
pub fn inventory_get_best_selling_item_ids(
    db: State<'_, Db>,
    period_days: i64,
    limit: i64,
) -> Result<Vec<i64>, String> {
    db.with_conn(|conn| Ok(reports::get_best_selling_item_ids(conn, period_days, limit)))
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

/// Creates a staff account. Owner is never an assignable role here — there
/// is exactly one Owner account per installation, seeded at first run (see
/// `db::schema::seed_owner`) — and an Admin may only create Cashier
/// accounts, never a peer Admin. Checked against the caller `require_role`
/// actually returned, not a role the client claims, so this holds even for
/// a direct `invoke()` call that skips the UI entirely.
#[tauri::command]
pub fn create_user(
    db: State<'_, Db>,
    session: State<'_, Session>,
    name: String,
    pin: String,
    role: Role,
) -> Result<User, String> {
    let caller = require_role(&session, STAFF_ROLES)?;
    authorize_create_user(caller.role, role)?;
    db.with_conn(move |conn| Ok(users::create(conn, &name, &pin, role)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Renames a staff account and/or changes their role.
///
/// The Owner account (there is only ever one) may only edit *itself*, and
/// its role can never change — no second Owner is ever assignable, and
/// self-demotion would silently lock the installation out of Owner-only
/// actions. Everyone else is gated by `caller_may_manage`: an Admin may
/// touch Cashier accounts (and their own Admin account, unchanged role)
/// but never another Admin or the Owner.
#[tauri::command]
pub fn update_user(
    db: State<'_, Db>,
    session: State<'_, Session>,
    user_id: i64,
    name: String,
    role: Role,
) -> Result<User, String> {
    let caller = require_role(&session, STAFF_ROLES)?;
    let current_role = db
        .with_conn(move |conn| users::role_of(conn, user_id))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "That user no longer exists".to_string())?;

    authorize_update_user(&caller, user_id, current_role, role)?;

    db.with_conn(move |conn| Ok(users::update(conn, user_id, &name, role)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Deactivates or reactivates a staff account.
///
/// The Owner account can never be deactivated, by anyone, including itself
/// — there is exactly one per installation and nobody would be left who
/// could create a new one. Nobody may deactivate the account they're
/// currently signed in as. Otherwise gated by `caller_may_manage`: an Admin
/// may only act on Cashier accounts, never a peer Admin.
#[tauri::command]
pub fn set_user_active(
    db: State<'_, Db>,
    session: State<'_, Session>,
    user_id: i64,
    is_active: bool,
) -> Result<(), String> {
    let caller = require_role(&session, STAFF_ROLES)?;
    let current_role = db
        .with_conn(move |conn| users::role_of(conn, user_id))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "That user no longer exists".to_string())?;

    authorize_set_active(&caller, user_id, current_role, is_active)?;

    db.with_conn(move |conn| Ok(users::set_active(conn, user_id, is_active)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Resets a staff member's PIN — the management screen's "reset PIN"
/// action, or self-service (changing your own). Gated the same as
/// `update_user`: changing your own PIN is always allowed regardless of
/// role, but resetting someone else's follows the same Owner/Admin
/// hierarchy — an Admin may reset a Cashier's PIN, never the Owner's or
/// another Admin's.
#[tauri::command]
pub fn set_user_pin(
    db: State<'_, Db>,
    session: State<'_, Session>,
    user_id: i64,
    pin: String,
) -> Result<(), String> {
    let caller = require_role(&session, STAFF_ROLES)?;
    if user_id != caller.id {
        let current_role = db
            .with_conn(move |conn| users::role_of(conn, user_id))
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "That user no longer exists".to_string())?;
        authorize_set_pin(&caller, user_id, current_role)?;
    }
    db.with_conn(move |conn| Ok(users::set_pin(conn, user_id, &pin)))
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

/// Attempts to print a receipt on this installation's configured thermal
/// printer (USB on desktop, Bluetooth on Android — see
/// `printer::escpos::send_to_printer`). Fails with a friendly message if
/// none is set up/reachable; the PDF receipt (built entirely in the
/// frontend with jsPDF) is the reliable fallback either way.
///
/// Reads the stored logo (if any) from disk and decodes it to a monochrome
/// raster here — `build_receipt_bytes` itself stays pure and never touches
/// the filesystem. An SVG logo (or any decode failure) just means no logo on
/// this receipt, same as no logo being set at all — never a reason to fail
/// the print.
#[tauri::command]
pub fn billing_print_receipt_thermal(app: AppHandle, db: State<'_, Db>, sale_id: i64) -> Result<(), String> {
    let sale = db
        .with_conn(|conn| Ok(sales::get_sale(conn, sale_id)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let app_config = db.with_conn(config::get).map_err(|e| e.to_string())?;
    let tables_module_enabled = db
        .with_conn(|conn| modules::is_enabled(conn, "tables", Platform::current()))
        .map_err(|e| e.to_string())?;

    let logo_dir_path = logo_dir(&app)?;
    let logo = app_config
        .logo_path
        .as_ref()
        .and_then(|file_name| images::read_image_bytes(&logo_dir_path, file_name).ok())
        .and_then(|(bytes, _extension)| crate::printer::escpos::build_logo_raster(&bytes));

    crate::printer::escpos::print_receipt(&sale, &app_config, logo.as_ref(), tables_module_enabled)
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Printer selection (Settings' "Select printer" step)
//
// Android only in practice — desktop's USB transport is auto-detected (see
// `printer::escpos`'s module doc comment) and needs no selection step, so
// these three simply report "not applicable" there rather than being
// registered conditionally, which would mean the frontend service layer
// needing to know at compile/call time which platform it's running on
// before even trying (it already knows via `IS_ANDROID`, but keeping the
// command surface uniform across platforms is one less thing to get wrong).
// ---------------------------------------------------------------------------

/// Every Bluetooth device already paired through the OS settings — the
/// candidate list for Settings' printer picker. Android only; always empty
/// on desktop.
#[tauri::command]
pub fn printer_list_bluetooth_devices() -> Result<Vec<crate::db::config::BluetoothDeviceOption>, String> {
    #[cfg(target_os = "android")]
    {
        catch_panic(|| {
            crate::printer::android_bt::list_bonded_devices()
                .map(|devices| {
                    devices
                        .into_iter()
                        .map(|d| crate::db::config::BluetoothDeviceOption { name: d.name, address: d.address })
                        .collect()
                })
                .map_err(|e| e.to_string())
        })
    }
    #[cfg(not(target_os = "android"))]
    {
        Ok(Vec::new())
    }
}

/// Whether this app currently holds the Bluetooth permission it needs to
/// list/connect to printers. Always `true` on desktop (nothing to grant).
#[tauri::command]
pub fn printer_bluetooth_permission_granted() -> Result<bool, String> {
    #[cfg(target_os = "android")]
    {
        catch_panic(|| crate::printer::android_bt::permission_granted().map_err(|e| e.to_string()))
    }
    #[cfg(not(target_os = "android"))]
    {
        Ok(true)
    }
}

/// Fires the OS Bluetooth-permission dialog if it hasn't been granted yet.
/// Fire-and-forget — see `android_bt::request_permission`'s doc comment for
/// why the frontend re-checks `printer_bluetooth_permission_granted`
/// afterwards rather than this returning the outcome directly. A no-op on
/// desktop.
#[tauri::command]
pub fn printer_request_bluetooth_permission() -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        catch_panic(|| crate::printer::android_bt::request_permission().map_err(|e| e.to_string()))
    }
    #[cfg(not(target_os = "android"))]
    {
        Ok(())
    }
}

/// Every printer Windows currently has installed — the candidate list for
/// Settings' printer picker on Windows (the Windows equivalent of
/// `printer_list_bluetooth_devices`). Windows only; always empty elsewhere,
/// same "uniform command surface, empty result off-platform" convention as
/// the Bluetooth commands above.
#[tauri::command]
pub fn printer_list_windows_printers() -> Result<Vec<crate::db::config::WindowsPrinterOption>, String> {
    #[cfg(target_os = "windows")]
    {
        catch_panic(|| {
            crate::printer::windows_spool::list_printers()
                .map(|printers| {
                    printers
                        .into_iter()
                        .map(|p| crate::db::config::WindowsPrinterOption { name: p.name })
                        .collect()
                })
                .map_err(|e| e.to_string())
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(Vec::new())
    }
}

/// Prints a ruler + rows of known length at several candidate widths to
/// whatever printer transport is currently configured — the direct-
/// measurement tool for a printer's real character-per-line width instead
/// of trusting a datasheet number (see `printer::escpos::build_diagnostic_
/// bytes`'s doc comment for why this exists: `LINE_WIDTH` has already been
/// wrong once). Owner/Admin only, same tier as the rest of Settings'
/// printer controls.
#[tauri::command]
pub fn printer_print_diagnostic(db: State<'_, Db>, session: State<'_, Session>) -> Result<(), String> {
    require_role(&session, STAFF_ROLES)?;
    let config = db.with_conn(config::get).map_err(|e| e.to_string())?;
    crate::printer::escpos::print_diagnostic(&config).map_err(|e| e.to_string())
}

/// The most recent sales — the refund flow's "pick the original sale" list.
/// Owner/Admin only, same as the refund commands below it.
#[tauri::command]
pub fn billing_list_recent_sales(
    db: State<'_, Db>,
    session: State<'_, Session>,
    limit: i64,
) -> Result<Vec<SaleListItem>, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| sales::list_recent(conn, limit)).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Refunds — Owner/Admin only (see PROJECT_GOALS.md's role matrix note in
// permissions.ts): refunds move cash back out and put stock back in, the
// same financial-sensitivity tier as Expenses/Salary, so every command here
// checks the session the same way those do.
// ---------------------------------------------------------------------------

/// The original sale plus, per line, how much of it is still refundable —
/// what the refund UI shows before anything is submitted.
#[tauri::command]
pub fn refund_get_sale(db: State<'_, Db>, session: State<'_, Session>, sale_id: i64) -> Result<RefundableSale, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(refunds::get_sale_for_refund(conn, sale_id)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Creates a refund: re-validates every line against the live sale/prior
/// refunds, writes `refunds` + `refund_items`, and puts the refunded
/// quantity back onto `items.stock_qty` — all inside one transaction (see
/// `db::refunds::create_refund`). `refundedBy` is deliberately not a
/// parameter here — it's always the signed-in caller, the same
/// "never trust a client-claimed identity" rule `session.rs`'s module doc
/// explains for role checks.
#[tauri::command]
pub fn refund_create(
    db: State<'_, Db>,
    session: State<'_, Session>,
    sale_id: i64,
    items: Vec<RefundLineInput>,
    reason: Option<String>,
) -> Result<Refund, String> {
    let caller = require_role(&session, STAFF_ROLES)?;
    db.with_transaction(|tx| {
        Ok(refunds::create_refund(
            tx,
            CreateRefundInput { sale_id, items, reason, refunded_by: Some(caller.id) },
        ))
    })
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

/// Re-fetches a previously created refund — used to reprint its receipt.
#[tauri::command]
pub fn refund_get(db: State<'_, Db>, session: State<'_, Session>, refund_id: i64) -> Result<Refund, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(refunds::get_refund(conn, refund_id)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Prints the "Refund Details" receipt on a USB thermal printer — same
/// auto-detect/fall-back-to-PDF contract as `billing_print_receipt_thermal`.
#[tauri::command]
pub fn refund_print_thermal(db: State<'_, Db>, session: State<'_, Session>, refund_id: i64) -> Result<(), String> {
    require_role(&session, STAFF_ROLES)?;
    let refund = db
        .with_conn(|conn| Ok(refunds::get_refund(conn, refund_id)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let app_config = db.with_conn(config::get).map_err(|e| e.to_string())?;
    crate::printer::escpos::print_refund(&refund, &app_config).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Shifts (only meaningful when the `shifts` module is enabled; the frontend
// hides the shift-history page and skips the open-shift prompt otherwise).
// Opening/closing a shift is *not* Owner/Admin-gated the way the rest of
// this file's writes are — it's a Cashier's own cash drawer, the same way
// Billing itself is Cashier-reachable — but every command here derives the
// acting cashier from the session, never from a client-supplied id, so one
// cashier can never open or close a shift in another's name.
// ---------------------------------------------------------------------------

fn require_signed_in(session: &Session) -> Result<User, String> {
    require_role(session, &[Role::Owner, Role::Admin, Role::Cashier])
}

/// The signed-in cashier's currently-open shift, if any — lets the billing
/// screen decide whether to show "Open Shift" or "Close Shift".
#[tauri::command]
pub fn shift_get_open(db: State<'_, Db>, session: State<'_, Session>) -> Result<Option<Shift>, String> {
    let caller = require_signed_in(&session)?;
    db.with_conn(|conn| Ok(shifts::get_open_shift_for_cashier(conn, caller.id)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Opens a new shift for the signed-in cashier. Refuses if they already
/// have one open.
#[tauri::command]
pub fn shift_open(
    db: State<'_, Db>,
    session: State<'_, Session>,
    opening_balance_minor: i64,
) -> Result<Shift, String> {
    let caller = require_signed_in(&session)?;
    db.with_conn(|conn| Ok(shifts::open_shift(conn, caller.id, opening_balance_minor)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Reconciliation breakdown for `shiftId`. `declaredCashAmountMinor` lets
/// the caller preview "if I declared this much, what would Short/Over be"
/// before confirming a close (see `db::shifts::get_shift_summary`) — pass
/// `None` to read what's actually stored (still-open: nothing declared yet;
/// closed: what was recorded at close time, for reprinting a past shift).
/// Any signed-in user may read this — the close-shift confirmation step
/// needs it before the cashier has actually closed anything yet.
#[tauri::command]
pub fn shift_get_summary(
    db: State<'_, Db>,
    session: State<'_, Session>,
    shift_id: i64,
    declared_cash_amount_minor: Option<i64>,
) -> Result<ShiftSummary, String> {
    require_signed_in(&session)?;
    db.with_conn(|conn| Ok(shifts::get_shift_summary(conn, shift_id, declared_cash_amount_minor)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Closes `shiftId` with the cashier's declared cash count. The shift's own
/// cashier may close it, or an Owner/Admin may close it on their behalf
/// (e.g. the cashier already left) — anyone else is refused.
#[tauri::command]
pub fn shift_close(
    db: State<'_, Db>,
    session: State<'_, Session>,
    shift_id: i64,
    declared_cash_amount_minor: i64,
) -> Result<ShiftSummary, String> {
    let caller = require_signed_in(&session)?;
    let shift = db
        .with_conn(|conn| Ok(shifts::get_shift_summary(conn, shift_id, None)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?
        .shift;
    let is_owner_of_shift = shift.cashier_id == Some(caller.id);
    if !is_owner_of_shift && !STAFF_ROLES.contains(&caller.role) {
        return Err("You don't have permission to close this shift".to_string());
    }

    db.with_transaction(|tx| Ok(shifts::close_shift(tx, shift_id, declared_cash_amount_minor)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Shift history for the Shifts page — Owner/Admin only, the standalone
/// page's tier (see MODULE_CATALOGUE's doc comment on the `shifts` module).
#[tauri::command]
pub fn shift_list_recent(db: State<'_, Db>, session: State<'_, Session>, limit: i64) -> Result<Vec<Shift>, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| shifts::list_shifts(conn, limit)).map_err(|e| e.to_string())
}

/// Prints a shift's close-out reconciliation receipt.
#[tauri::command]
pub fn shift_print_summary(db: State<'_, Db>, session: State<'_, Session>, shift_id: i64) -> Result<(), String> {
    require_signed_in(&session)?;
    let summary = db
        .with_conn(|conn| Ok(shifts::get_shift_summary(conn, shift_id, None)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let app_config = db.with_conn(config::get).map_err(|e| e.to_string())?;
    crate::printer::escpos::print_shift_summary(&summary, &app_config).map_err(|e| e.to_string())
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

/// The Category Wise Sale report: every item sold in the range, grouped by
/// category with a subtotal per category and a grand total.
#[tauri::command]
pub fn reports_get_category_sales(
    db: State<'_, Db>,
    session: State<'_, Session>,
    start_date: String,
    end_date: String,
) -> Result<CategorySalesReport, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(reports::get_category_sales(conn, &start_date, &end_date)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Prints the Category Wise Sale report on a USB thermal printer.
#[tauri::command]
pub fn reports_print_category_sales(
    db: State<'_, Db>,
    session: State<'_, Session>,
    start_date: String,
    end_date: String,
) -> Result<(), String> {
    require_role(&session, STAFF_ROLES)?;
    let report = db
        .with_conn(|conn| Ok(reports::get_category_sales(conn, &start_date, &end_date)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let app_config = db.with_conn(config::get).map_err(|e| e.to_string())?;
    crate::printer::escpos::print_category_sales(&report, &app_config).map_err(|e| e.to_string())
}

/// The Table Wise Sales report: one row per table plus a "Counter /
/// Takeaway" row for sales with no table, sorted by amount and summing to
/// the same gross total `reports_get_sales_summary` reports for the same
/// range. Reachable regardless of whether the `tables` module happens to be
/// enabled — same as every other report command in this file, module
/// visibility is a route/UI concern (`ModuleRoute`), not something each
/// query re-checks — the Reports screen simply doesn't offer this view
/// unless `tables` is on.
#[tauri::command]
pub fn reports_get_table_sales_summary(
    db: State<'_, Db>,
    session: State<'_, Session>,
    start_date: String,
    end_date: String,
) -> Result<TableSalesSummary, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(reports::get_table_sales_summary(conn, &start_date, &end_date)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// The "Product Wise Sales" report: every item sold in the range, ranked by
/// revenue or quantity (`sort_by`), optionally narrowed to one category, plus
/// a "no sales this period" list of active items that sold zero units — so
/// slow-moving stock stays visible rather than just absent from the report.
#[tauri::command]
pub fn reports_get_product_sales_summary(
    db: State<'_, Db>,
    session: State<'_, Session>,
    start_date: String,
    end_date: String,
    category_id: Option<i64>,
    sort_by: TopItemSort,
) -> Result<ProductSalesSummaryReport, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(reports::get_product_sales_summary(conn, &start_date, &end_date, category_id, sort_by)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Prints the Table Wise Sales report on a USB thermal printer.
#[tauri::command]
pub fn reports_print_table_sales_summary(
    db: State<'_, Db>,
    session: State<'_, Session>,
    start_date: String,
    end_date: String,
) -> Result<(), String> {
    require_role(&session, STAFF_ROLES)?;
    let report = db
        .with_conn(|conn| Ok(reports::get_table_sales_summary(conn, &start_date, &end_date)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let app_config = db.with_conn(config::get).map_err(|e| e.to_string())?;
    crate::printer::escpos::print_table_sales(&report, &app_config).map_err(|e| e.to_string())
}

/// The "Refunds" report: every refund recorded in the range (by refund
/// date, not the date of the original sale), most recent first, itemized
/// with the original sale/receipt number (`originalSaleId`), refunded
/// item(s), reason, who processed it, and a grand total refunded for the
/// period. Not module-gated — refunds aren't a toggleable module.
#[tauri::command]
pub fn reports_get_refunds_summary(
    db: State<'_, Db>,
    session: State<'_, Session>,
    start_date: String,
    end_date: String,
) -> Result<RefundsSummary, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(reports::get_refunds_summary(conn, &start_date, &end_date)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Prints the Refunds report on a USB thermal printer.
#[tauri::command]
pub fn reports_print_refunds_summary(
    db: State<'_, Db>,
    session: State<'_, Session>,
    start_date: String,
    end_date: String,
) -> Result<(), String> {
    require_role(&session, STAFF_ROLES)?;
    let report = db
        .with_conn(|conn| Ok(reports::get_refunds_summary(conn, &start_date, &end_date)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let app_config = db.with_conn(config::get).map_err(|e| e.to_string())?;
    crate::printer::escpos::print_refunds_summary(&report, &app_config).map_err(|e| e.to_string())
}

/// The "Generate Full Report" consolidated document: Overview (incl. net
/// profit, via `dashboard::get_summary`), Category Wise Sale, Product Wise
/// Sales, and Table Wise Sales (only when `tables` is enabled) for one date
/// range, assembled once so the PDF download and the thermal print below
/// are guaranteed to show identical numbers.
#[tauri::command]
pub fn reports_get_full_report(
    db: State<'_, Db>,
    session: State<'_, Session>,
    start_date: String,
    end_date: String,
    platform: String,
) -> Result<FullReport, String> {
    require_role(&session, STAFF_ROLES)?;
    let platform = parse_platform(&platform)?;
    db.with_conn(|conn| Ok(full_report::get_full_report(conn, &start_date, &end_date, platform)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Prints the same consolidated Full Report on a USB thermal printer.
#[tauri::command]
pub fn reports_print_full_report(
    db: State<'_, Db>,
    session: State<'_, Session>,
    start_date: String,
    end_date: String,
    platform: String,
) -> Result<(), String> {
    require_role(&session, STAFF_ROLES)?;
    let platform = parse_platform(&platform)?;
    let report = db
        .with_conn(|conn| Ok(full_report::get_full_report(conn, &start_date, &end_date, platform)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let app_config = db.with_conn(config::get).map_err(|e| e.to_string())?;
    crate::printer::escpos::print_full_report(&report, &app_config).map_err(|e| e.to_string())
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
///
/// `start_table_order` is two writes (the order upsert, then the table's
/// status flip) — run inside a transaction so a crash between them can never
/// leave a table marked occupied with no order behind it, or vice versa
/// (Phase 13 transaction audit; see `TESTING_CHECKLIST.md`).
#[tauri::command]
pub fn tables_assign_order_to_table(
    db: State<'_, Db>,
    session: State<'_, Session>,
    table_id: i64,
) -> Result<TableSummary, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_transaction(|tx| Ok(tables::start_table_order(tx, table_id)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Manually releases a table — cancels its open order (if any) and frees it.
/// A completed sale frees a table automatically; this is for the other
/// paths (a cancelled order, a guest who left without paying).
///
/// Two writes (cancel the order, free the table) — transactional for the
/// same reason as `tables_assign_order_to_table` above.
#[tauri::command]
pub fn tables_clear_table(
    db: State<'_, Db>,
    session: State<'_, Session>,
    table_id: i64,
) -> Result<TableSummary, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_transaction(|tx| Ok(tables::clear_table(tx, table_id)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Parks the current cart on a table ("Save to table") instead of completing
/// the sale immediately.
///
/// Two writes (park the cart, mark the table occupied) — transactional for
/// the same reason as the two commands above; this one in particular is a
/// Cashier-reachable path (via Billing), so it's exercised far more often.
#[tauri::command]
pub fn tables_attach_cart_to_table(
    db: State<'_, Db>,
    table_id: i64,
    items: Vec<ParkedCartLine>,
    discount_minor: i64,
) -> Result<(), String> {
    db.with_transaction(|tx| Ok(tables::attach_cart_to_table(tx, table_id, &items, discount_minor)))
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

/// Moves an in-progress table's order to a different, currently-free table
/// (a customer who moves seats) — the cart itself (items, quantities,
/// discount) is untouched, only which table it's parked on changes. Rejects
/// a source with no active order, or a destination that isn't free, rather
/// than silently merging/overwriting (see `tables::shift_table_order`'s doc
/// comment). Two tables' `status` flip together — transactional for the
/// same reason as `tables_clear_table`/`tables_assign_order_to_table`.
/// Owner/Admin only, same tier as the rest of the floor-management commands
/// above (unlike `tables_attach_cart_to_table`, which Cashiers reach from
/// Billing) — the floor view itself is Owner/Admin territory.
#[tauri::command]
pub fn tables_shift_table_order(
    db: State<'_, Db>,
    session: State<'_, Session>,
    from_table_id: i64,
    to_table_id: i64,
) -> Result<tables::ShiftTableResult, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_transaction(|tx| Ok(tables::shift_table_order(tx, from_table_id, to_table_id)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Counters (kitchen/prep stations for KOT tokens — only meaningful when the
// `tables` module is enabled, same "no separate toggle" reasoning `shifts`
// already documents for its own inline-in-Billing UI). Listing is open to
// any signed-in role — a Cashier's "Print Token" dialog needs counter
// names — but managing the list (Settings) is Owner/Admin only, like every
// other small-entity management screen in this codebase.
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn counters_get_counters(db: State<'_, Db>, include_inactive: bool) -> Result<Vec<Counter>, String> {
    db.with_conn(|conn| counters::list_counters(conn, include_inactive)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn counters_add_counter(db: State<'_, Db>, session: State<'_, Session>, name: String) -> Result<Counter, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(counters::add_counter(conn, &name))).map_err(|e| e.to_string())?.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn counters_update_counter(
    db: State<'_, Db>,
    session: State<'_, Session>,
    id: i64,
    name: String,
) -> Result<Counter, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(counters::update_counter(conn, id, &name)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn counters_set_active(
    db: State<'_, Db>,
    session: State<'_, Session>,
    id: i64,
    is_active: bool,
) -> Result<Counter, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(|conn| Ok(counters::set_counter_active(conn, id, is_active)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// KOT tokens — kitchen/counter instructions, printed when an order is
// taken, separate from and printed well before the bill. Cashier-reachable,
// no role gate, same tier as `tables_attach_cart_to_table`/`billing_print_
// receipt_thermal`: this is a normal step in taking an order, not a
// Settings-level action. See `db::tokens`'s module doc comment for why
// tokenization is tracked per-item against the order rather than against a
// specific cart line.
// ---------------------------------------------------------------------------

/// What would print right now for `table_order_id`, grouped by counter —
/// the "Print Token" dialog's contents, computed without printing or
/// writing anything. An item with no counter assigned never appears, by
/// design (see `db::tokens::get_pending_token_items`).
#[tauri::command]
pub fn tokens_get_pending_items(db: State<'_, Db>, table_order_id: i64) -> Result<Vec<PendingCounterGroup>, String> {
    db.with_conn(|conn| Ok(tokens::get_pending_token_items(conn, table_order_id)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Every token ever printed for this table order, newest first — "tokens
/// for this table", with a reprint option per row.
#[tauri::command]
pub fn tokens_get_for_order(db: State<'_, Db>, table_order_id: i64) -> Result<Vec<TokenSummary>, String> {
    db.with_conn(|conn| Ok(tokens::list_tokens_for_order(conn, table_order_id)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Prints one counter's token, only recording it as printed if the
/// physical print actually succeeded — see `db::tokens`'s module doc
/// comment for why this ordering (print, *then* write) is the whole point,
/// not an implementation detail. Runs inside one `with_transaction` per
/// counter: every write in the success path (`insert_token`) happens after
/// the print already succeeded, so a rolled-back transaction on failure
/// never had anything real to lose — see the inline comments below for
/// exactly where that guarantee comes from.
fn print_one_counter_token(
    db: &Db,
    table_order_id: i64,
    counter: &Counter,
    printed_by: Option<i64>,
    printed_by_name: Option<String>,
    config: &AppConfig,
) -> PrintOutcome {
    let result = db.with_transaction(|tx| {
        let pending = match tokens::pending_items_for_counter(tx, table_order_id, counter.id) {
            Ok(p) => p,
            Err(tokens::TokenError::Sqlite(e)) => return Err(e),
            Err(other) => return Ok(Err(other.to_string())),
        };
        if pending.is_empty() {
            return Ok(Ok(None));
        }

        let ctx = match tables::get_open_order_by_id(tx, table_order_id) {
            Ok(Some(ctx)) => ctx,
            Ok(None) => return Ok(Err("That table order is no longer open".to_string())),
            Err(tables::TableError::Sqlite(e)) => return Err(e),
            Err(other) => return Ok(Err(other.to_string())),
        };

        let token_number = tokens::next_token_number_for_today(tx)?;

        // A draft, not-yet-persisted `TokenSummary` — everything the
        // ticket itself needs to print, built *before* any row exists.
        // `id`/`table_order_id` are never printed, so the placeholder `id`
        // is harmless.
        let draft = TokenSummary {
            id: 0,
            token_number,
            counter_id: counter.id,
            counter_name: counter.name.clone(),
            table_order_id: Some(table_order_id),
            table_id: Some(ctx.table_id),
            table_name: Some(ctx.table_name),
            printed_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            printed_by,
            printed_by_name: printed_by_name.clone(),
            status: "printed".to_string(),
            items: pending.clone(),
        };

        // The critical ordering: if this fails, we return `Ok(Err(..))`
        // with *no* `tx.execute` having run yet — an empty transaction
        // commits harmlessly, and the items stay pending for a retry.
        if let Err(print_err) = crate::printer::escpos::print_token(&draft, config, false) {
            return Ok(Err(print_err.to_string()));
        }

        let token = match tokens::insert_token(tx, Some(table_order_id), counter.id, token_number, printed_by, &pending)
        {
            Ok(t) => t,
            Err(tokens::TokenError::Sqlite(e)) => return Err(e),
            Err(other) => return Ok(Err(other.to_string())),
        };
        Ok(Ok(Some(token)))
    });

    match result {
        Ok(Ok(Some(token))) => PrintOutcome::Printed { token },
        Ok(Ok(None)) => PrintOutcome::NothingPending,
        Ok(Err(message)) => PrintOutcome::Failed { error: message },
        Err(db_err) => PrintOutcome::Failed { error: db_err.to_string() },
    }
}

/// Prints a token for each of `counter_ids` — every counter gets its own
/// result (printed / nothing pending / failed) even if another one in the
/// same call fails, so one unreachable printer never hides a successful
/// print on a different counter. See `print_one_counter_token` for the
/// per-counter print-then-record guarantee.
#[tauri::command]
pub fn tokens_print(
    db: State<'_, Db>,
    session: State<'_, Session>,
    table_order_id: i64,
    counter_ids: Vec<i64>,
) -> Result<Vec<CounterPrintResult>, String> {
    let caller = session.current();
    let printed_by = caller.as_ref().map(|u| u.id);
    let printed_by_name = caller.map(|u| u.name);
    let config = db.with_conn(config::get).map_err(|e| e.to_string())?;
    let all_counters = db.with_conn(|conn| counters::list_counters(conn, true)).map_err(|e| e.to_string())?;

    let mut results = Vec::with_capacity(counter_ids.len());
    for counter_id in counter_ids {
        let Some(counter) = all_counters.iter().find(|c| c.id == counter_id) else {
            results.push(CounterPrintResult {
                counter_id,
                counter_name: format!("Counter {counter_id}"),
                outcome: PrintOutcome::Failed { error: "Counter not found".to_string() },
            });
            continue;
        };

        let outcome =
            print_one_counter_token(&db, table_order_id, counter, printed_by, printed_by_name.clone(), &config);
        results.push(CounterPrintResult { counter_id: counter.id, counter_name: counter.name.clone(), outcome });
    }

    Ok(results)
}

/// Reprints an existing token exactly as originally recorded — no new
/// token number, no change to what's tokenized, clearly marked "REPRINT"
/// on the output (see `printer::escpos::build_token_bytes`) so the counter
/// never mistakes it for a second, separate order. Writes nothing to the
/// database; a failed reprint just means "try again", same as any other
/// print.
#[tauri::command]
pub fn tokens_reprint(db: State<'_, Db>, token_id: i64) -> Result<(), String> {
    let token = db
        .with_conn(|conn| Ok(tokens::get_token(conn, token_id)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let config = db.with_conn(config::get).map_err(|e| e.to_string())?;
    crate::printer::escpos::print_token(&token, &config, true).map_err(|e| e.to_string())
}

/// What would print right now for an ad hoc (Takeaway) cart, grouped by
/// counter — the Takeaway counterpart of `tokens_get_pending_items`. Unlike
/// that one, this has no `table_order_id` to read history against, so it
/// always reports the full quantity of every counter-eligible line in
/// `items`, not a delta — see `db::tokens`'s module doc comment.
#[tauri::command]
pub fn tokens_get_adhoc_groups(
    db: State<'_, Db>,
    items: Vec<tokens::AdhocTokenLine>,
) -> Result<Vec<PendingCounterGroup>, String> {
    db.with_conn(|conn| Ok(tokens::ad_hoc_token_groups(conn, &items))).map_err(|e| e.to_string())?.map_err(|e| e.to_string())
}

/// Prints a token per selected counter for a Takeaway cart that has no
/// table (and so no `table_orders` row) behind it at all — same
/// print-then-record ordering and per-counter independence as `tokens_
/// print`, just fed from `items` (the live billing cart) instead of a
/// parked order.
///
/// Deliberate trade-off, disclosed rather than silently accepted: because
/// there is no persisted order to diff against, every call here prints the
/// *entire* quantity of each counter-eligible line in `items`, not just
/// what's new since the last print. Printing tokens for the same Takeaway
/// cart twice in a row (before completing the sale) sends everything to
/// the counter twice — reasonable for the common case (compose the order
/// once, print once, pay), same as a QSR register printing a fresh ticket
/// per "print" press, but unlike the dine-in table flow's delta tracking.
#[tauri::command]
pub fn tokens_print_adhoc(
    db: State<'_, Db>,
    session: State<'_, Session>,
    items: Vec<tokens::AdhocTokenLine>,
    counter_ids: Vec<i64>,
) -> Result<Vec<CounterPrintResult>, String> {
    let caller = session.current();
    let printed_by = caller.as_ref().map(|u| u.id);
    let printed_by_name = caller.map(|u| u.name);
    let config = db.with_conn(config::get).map_err(|e| e.to_string())?;
    let all_counters = db.with_conn(|conn| counters::list_counters(conn, true)).map_err(|e| e.to_string())?;

    let mut results = Vec::with_capacity(counter_ids.len());
    for counter_id in counter_ids {
        let Some(counter) = all_counters.iter().find(|c| c.id == counter_id) else {
            results.push(CounterPrintResult {
                counter_id,
                counter_name: format!("Counter {counter_id}"),
                outcome: PrintOutcome::Failed { error: "Counter not found".to_string() },
            });
            continue;
        };

        let outcome =
            print_one_adhoc_counter_token(&db, &items, counter, printed_by, printed_by_name.clone(), &config);
        results.push(CounterPrintResult { counter_id: counter.id, counter_name: counter.name.clone(), outcome });
    }

    Ok(results)
}

/// The ad hoc (Takeaway) counterpart of `print_one_counter_token` — same
/// print-then-record-only-on-success discipline, just against `items`
/// (the live cart) instead of a table order, and recording with
/// `table_order_id: None`.
fn print_one_adhoc_counter_token(
    db: &Db,
    items: &[tokens::AdhocTokenLine],
    counter: &Counter,
    printed_by: Option<i64>,
    printed_by_name: Option<String>,
    config: &AppConfig,
) -> PrintOutcome {
    let result = db.with_transaction(|tx| {
        let pending = match tokens::ad_hoc_pending_for_counter(tx, items, counter.id) {
            Ok(p) => p,
            Err(tokens::TokenError::Sqlite(e)) => return Err(e),
            Err(other) => return Ok(Err(other.to_string())),
        };
        if pending.is_empty() {
            return Ok(Ok(None));
        }

        let token_number = tokens::next_token_number_for_today(tx)?;

        let draft = TokenSummary {
            id: 0,
            token_number,
            counter_id: counter.id,
            counter_name: counter.name.clone(),
            table_order_id: None,
            table_id: None,
            table_name: None,
            printed_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            printed_by,
            printed_by_name: printed_by_name.clone(),
            status: "printed".to_string(),
            items: pending.clone(),
        };

        // Same critical ordering as `print_one_counter_token`: nothing is
        // written until the physical print has already succeeded.
        if let Err(print_err) = crate::printer::escpos::print_token(&draft, config, false) {
            return Ok(Err(print_err.to_string()));
        }

        let token = match tokens::insert_token(tx, None, counter.id, token_number, printed_by, &pending) {
            Ok(t) => t,
            Err(tokens::TokenError::Sqlite(e)) => return Err(e),
            Err(other) => return Ok(Err(other.to_string())),
        };
        Ok(Ok(Some(token)))
    });

    match result {
        Ok(Ok(Some(token))) => PrintOutcome::Printed { token },
        Ok(Ok(None)) => PrintOutcome::NothingPending,
        Ok(Err(message)) => PrintOutcome::Failed { error: message },
        Err(db_err) => PrintOutcome::Failed { error: db_err.to_string() },
    }
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

/// Every employee, active or not — the employee management screen.
#[tauri::command]
pub fn attendance_get_all_employees(
    db: State<'_, Db>,
    session: State<'_, Session>,
) -> Result<Vec<ManagedEmployee>, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(attendance::list_all_employees).map_err(|e| e.to_string())
}

/// Adds a new employee (payroll/attendance record, not a login account —
/// see `db::attendance`'s module doc comment). Shows up in the check-in
/// screen and the monthly summary immediately, no restart needed, since both
/// re-query `employees` on every load.
#[tauri::command]
pub fn attendance_add_employee(
    db: State<'_, Db>,
    session: State<'_, Session>,
    name: String,
    role: String,
    contact: Option<String>,
    base_salary_minor: i64,
) -> Result<ManagedEmployee, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(move |conn| Ok(attendance::add_employee(conn, &name, &role, contact.as_deref(), base_salary_minor)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Renames/re-roles/updates the contact and base salary of an employee.
#[tauri::command]
pub fn attendance_update_employee(
    db: State<'_, Db>,
    session: State<'_, Session>,
    employee_id: i64,
    name: String,
    role: String,
    contact: Option<String>,
    base_salary_minor: i64,
) -> Result<ManagedEmployee, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(move |conn| {
        Ok(attendance::update_employee(
            conn,
            employee_id,
            &name,
            &role,
            contact.as_deref(),
            base_salary_minor,
        ))
    })
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

/// Deactivates or reactivates an employee (soft — attendance and salary
/// history stay intact). A deactivated employee drops off the check-in
/// screen and the monthly summary immediately.
#[tauri::command]
pub fn attendance_set_employee_active(
    db: State<'_, Db>,
    session: State<'_, Session>,
    employee_id: i64,
    is_active: bool,
) -> Result<(), String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_conn(move |conn| Ok(attendance::set_employee_active(conn, employee_id, is_active)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
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

/// `base_salary / days_in_month * days_present` for one employee and month
/// (`days_in_month` is the real calendar day count, 28–31), refreshed
/// against the latest attendance every call.
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
///
/// One write per employee (each refreshes `calculated_amount_minor`) — run
/// as a single transaction so the whole table reflects one consistent
/// instant rather than some employees' figures possibly being one attendance
/// write ahead of others' if the process died mid-loop (Phase 13 audit).
#[tauri::command]
pub fn salary_get_monthly_overview(
    db: State<'_, Db>,
    session: State<'_, Session>,
    month: String,
) -> Result<Vec<SalaryCalculation>, String> {
    require_role(&session, STAFF_ROLES)?;
    db.with_transaction(|tx| Ok(salary::get_monthly_overview(tx, &month)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Records a payment against `month`'s salary — added to whatever has
/// already been paid that month, not a replacement (pay is often settled in
/// more than one instalment).
///
/// Three writes under the hood (`calculate_salary` to refresh the row,
/// the `paid_amount_minor` update, `calculate_salary` again for the fresh
/// return value) — wrapped in one transaction so a crash between them can
/// never leave a payment partially recorded (Phase 13 audit).
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
    db.with_transaction(|tx| Ok(salary::record_payment(tx, employee_id, &month, paid_amount_minor, &paid_date)))
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

// ---------------------------------------------------------------------------
// Product owner (vendor) — a hidden, separate account for the product's
// developer/vendor, not for any client's staff. Never appears in Manage
// Users or any client-facing query; gated by `ProductOwnerSession`, never
// `session::Session`, so reaching it neither requires nor disturbs whatever
// staff account is currently signed in. See `db::product_owner`'s module
// doc for the full rationale, and SUPPORT.md for the (deliberately
// non-in-app) credential-recovery process.
// ---------------------------------------------------------------------------

/// Whether a credential has already been set on this install — the hidden
/// entry point's UI uses this to decide whether to show a setup form or a
/// login form. Unauthenticated on purpose: there's nothing to gate yet on
/// the "no account" branch, and the answer alone reveals nothing about the
/// credential itself.
#[tauri::command]
pub fn product_owner_get_status(db: State<'_, Db>) -> Result<bool, String> {
    db.with_conn(product_owner::has_account).map_err(|e| e.to_string())
}

/// Sets the initial credential for this install and immediately grants an
/// elevated session (so setting it and using it is one step, not two).
/// Refuses if a credential already exists — see
/// `product_owner::ProductOwnerError::AlreadyConfigured`.
#[tauri::command]
pub fn product_owner_setup(
    db: State<'_, Db>,
    po_session: State<'_, ProductOwnerSession>,
    password: String,
) -> Result<(), String> {
    db.with_conn(|conn| Ok(product_owner::setup(conn, &password)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    po_session.grant();
    Ok(())
}

/// Verifies `password` and, on success, grants an elevated session — see
/// `ProductOwnerSession` for its (short, idle-based) lifetime.
#[tauri::command]
pub fn product_owner_login(
    db: State<'_, Db>,
    po_session: State<'_, ProductOwnerSession>,
    password: String,
) -> Result<(), String> {
    db.with_conn(|conn| Ok(product_owner::verify(conn, &password)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    po_session.grant();
    Ok(())
}

/// Ends the elevated session immediately, without waiting for it to idle out.
#[tauri::command]
pub fn product_owner_logout(po_session: State<'_, ProductOwnerSession>) {
    po_session.revoke();
}

/// Every module's state (including per-platform lock flags) — the
/// module-override UI's list. Requires a currently-valid elevated session.
#[tauri::command]
pub fn product_owner_get_modules(
    db: State<'_, Db>,
    po_session: State<'_, ProductOwnerSession>,
    platform: String,
) -> Result<Vec<ModuleState>, String> {
    require_product_owner(&po_session)?;
    let platform = parse_platform(&platform)?;
    db.with_conn(|conn| modules::list(conn, platform)).map_err(|e| e.to_string())
}

/// Sets a module's `enabled` and/or `locked` state on one platform,
/// independently (`None` leaves that half unchanged) — see
/// `db::modules::set_by_product_owner`. This is the higher-privilege path:
/// it bypasses the client-facing `toggle_module`'s lock check entirely
/// (it's the thing that *sets* the lock), reusing the exact same
/// `enabled_modules` update logic rather than a parallel implementation.
#[tauri::command]
pub fn product_owner_set_module(
    db: State<'_, Db>,
    po_session: State<'_, ProductOwnerSession>,
    module_key: String,
    platform: String,
    enabled: Option<bool>,
    locked: Option<bool>,
) -> Result<Vec<ModuleState>, String> {
    require_product_owner(&po_session)?;
    let platform = parse_platform(&platform)?;
    db.with_conn(|conn| {
        Ok(modules::set_by_product_owner(conn, &module_key, platform, enabled, locked)
            .map_err(|e| e.to_string())
            .and_then(|()| modules::list(conn, platform).map_err(|e| e.to_string())))
    })
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// Owner/Admin/Cashier account hierarchy — the `authorize_*` functions above
// are the actual gate (`create_user`/`update_user`/`set_user_active`/
// `set_user_pin` are thin wrappers around them), so exercising them directly
// here proves the rule holds at the command layer itself — an Admin cannot
// touch the Owner or a peer Admin even by calling the command straight,
// not merely because the UI hides the button.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod user_hierarchy_tests {
    use super::*;

    fn user(id: i64, role: Role) -> User {
        User { id, name: "Test".into(), role }
    }

    // -- create_user -------------------------------------------------------

    #[test]
    fn owner_can_create_admin_and_cashier_but_not_another_owner() {
        assert!(authorize_create_user(Role::Owner, Role::Admin).is_ok());
        assert!(authorize_create_user(Role::Owner, Role::Cashier).is_ok());
        assert!(authorize_create_user(Role::Owner, Role::Owner).is_err());
    }

    #[test]
    fn admin_can_create_cashier_but_not_admin_or_owner() {
        assert!(authorize_create_user(Role::Admin, Role::Cashier).is_ok());
        assert!(authorize_create_user(Role::Admin, Role::Admin).is_err());
        assert!(authorize_create_user(Role::Admin, Role::Owner).is_err());
    }

    // -- update_user ---------------------------------------------------------

    #[test]
    fn admin_cannot_edit_the_owner_account_directly() {
        let admin = user(2, Role::Admin);
        // The Owner's account (id 1) — an Admin calling update_user on it
        // straight, exactly as a direct invoke() would, must be refused.
        let result = authorize_update_user(&admin, 1, Role::Owner, Role::Owner);
        assert!(result.is_err());
    }

    #[test]
    fn admin_cannot_edit_another_admins_account_directly() {
        let admin = user(2, Role::Admin);
        let other_admin_id = 3;
        let result = authorize_update_user(&admin, other_admin_id, Role::Admin, Role::Admin);
        assert!(result.is_err());
    }

    #[test]
    fn admin_can_edit_their_own_admin_account_without_changing_role() {
        let admin = user(2, Role::Admin);
        assert!(authorize_update_user(&admin, admin.id, Role::Admin, Role::Admin).is_ok());
    }

    #[test]
    fn admin_can_edit_a_cashier_account() {
        let admin = user(2, Role::Admin);
        assert!(authorize_update_user(&admin, 5, Role::Cashier, Role::Cashier).is_ok());
    }

    #[test]
    fn nobody_can_promote_anyone_to_owner() {
        let owner = user(1, Role::Owner);
        assert!(authorize_update_user(&owner, 5, Role::Cashier, Role::Owner).is_err());
        assert!(authorize_update_user(&owner, 5, Role::Admin, Role::Owner).is_err());
    }

    #[test]
    fn the_owner_can_edit_their_own_account_without_changing_role() {
        let owner = user(1, Role::Owner);
        assert!(authorize_update_user(&owner, owner.id, Role::Owner, Role::Owner).is_ok());
    }

    #[test]
    fn the_owners_own_role_can_never_be_changed_even_by_themselves() {
        let owner = user(1, Role::Owner);
        let result = authorize_update_user(&owner, owner.id, Role::Owner, Role::Admin);
        assert!(result.is_err());
    }

    #[test]
    fn owner_can_edit_an_admin_or_cashier_account() {
        let owner = user(1, Role::Owner);
        assert!(authorize_update_user(&owner, 2, Role::Admin, Role::Admin).is_ok());
        assert!(authorize_update_user(&owner, 5, Role::Cashier, Role::Admin).is_ok());
    }

    // -- set_user_active -----------------------------------------------------

    #[test]
    fn admin_cannot_deactivate_the_owner_account_directly() {
        let admin = user(2, Role::Admin);
        let result = authorize_set_active(&admin, 1, Role::Owner, false);
        assert!(result.is_err());
    }

    #[test]
    fn nobody_can_deactivate_the_owner_including_the_owner_themselves() {
        let owner = user(1, Role::Owner);
        let result = authorize_set_active(&owner, owner.id, Role::Owner, false);
        assert!(result.is_err());
    }

    #[test]
    fn admin_cannot_deactivate_another_admin_account_directly() {
        let admin = user(2, Role::Admin);
        let result = authorize_set_active(&admin, 3, Role::Admin, false);
        assert!(result.is_err());
    }

    #[test]
    fn admin_can_deactivate_a_cashier_account() {
        let admin = user(2, Role::Admin);
        assert!(authorize_set_active(&admin, 5, Role::Cashier, false).is_ok());
    }

    #[test]
    fn nobody_can_deactivate_their_own_account() {
        let admin = user(2, Role::Admin);
        assert!(authorize_set_active(&admin, admin.id, Role::Admin, false).is_err());
        let owner = user(1, Role::Owner);
        assert!(authorize_set_active(&owner, owner.id, Role::Owner, false).is_err());
    }

    #[test]
    fn owner_can_deactivate_an_admin_account() {
        let owner = user(1, Role::Owner);
        assert!(authorize_set_active(&owner, 2, Role::Admin, false).is_ok());
    }

    // -- set_user_pin ---------------------------------------------------------

    #[test]
    fn admin_cannot_reset_the_owners_pin() {
        let admin = user(2, Role::Admin);
        assert!(authorize_set_pin(&admin, 1, Role::Owner).is_err());
    }

    #[test]
    fn admin_cannot_reset_another_admins_pin() {
        let admin = user(2, Role::Admin);
        assert!(authorize_set_pin(&admin, 3, Role::Admin).is_err());
    }

    #[test]
    fn admin_can_reset_a_cashiers_pin() {
        let admin = user(2, Role::Admin);
        assert!(authorize_set_pin(&admin, 5, Role::Cashier).is_ok());
    }

    #[test]
    fn everyone_can_reset_their_own_pin_regardless_of_role() {
        let owner = user(1, Role::Owner);
        assert!(authorize_set_pin(&owner, owner.id, Role::Owner).is_ok());
        let admin = user(2, Role::Admin);
        assert!(authorize_set_pin(&admin, admin.id, Role::Admin).is_ok());
    }
}
