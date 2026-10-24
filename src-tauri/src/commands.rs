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
use crate::db::modules::{key as module_key, ModuleState, Platform};
use crate::license::entitlements;
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
use crate::db::dates::TIMESTAMP_FORMAT;
use crate::db::users::{ManagedUser, Role, User};
use crate::db::{
    attendance, config, counters, csv_import, customers, dashboard, expenses, full_report, insights, items, modules,
    product_owner, purchasing, refunds, reports, salary, sales, security_pin, shifts, stock, tables, tokens, users,
    orders, delivery_platforms, Db,
};
use crate::area_access_session::AreaAccessSession;
use crate::attempt_limiter::AttemptLimiter;
use crate::iap::{self, AppleAccount, AppleAccountInfo, IapProduct, PurchaseResult};
use crate::images;
use crate::license::runtime::LicenseRuntime;
use crate::license::state::LicenseState;
use crate::product_owner_session::{require_product_owner, ProductOwnerSession};
use crate::vendor_gate::{self, VendorGate};
use crate::workspace::{self, runtime::WorkspaceInfo};
use crate::session::{require_role, require_signed_in, Session};

/// Owner and Admin are this product's two "full access" roles — see
/// `src/utils/permissions.ts` for the frontend mirror of this matrix, and
/// the module doc comment on `session.rs` for why the check has to live here
/// too and not just there. There is deliberately no third privileged tier;
/// Cashier is the only other role, and it never appears in this list.
const ADMIN_ROLES: &[Role] = &[Role::Owner, Role::Admin];

/// Fails a command unless this install's licence currently allows the till
/// to run — the Rust half of the licence lockout. The frontend's lock
/// screen is only presentation (see `vendor_gate.rs` for why that can't be
/// the boundary); this is what actually stops a sale, a refund or a
/// kitchen token from being recorded on an expired, revoked or
/// clock-rolled-back install. Reads (reports, inventory, settings) are
/// deliberately not gated: the owner must still be able to see how to
/// renew, export their data, and enter a new key.
fn require_license(license: &LicenseRuntime) -> Result<(), String> {
    match license_block_reason(&license.current()) {
        None => Ok(()),
        Some(reason) => Err(reason),
    }?;
    // The phone/tablet app is itself a plan feature.
    if cfg!(any(target_os = "android", target_os = "ios")) {
        require_capability(entitlements::capability::MOBILE)?;
    }
    Ok(())
}

/// Fails a command unless the licence's plan includes `capability` — a
/// feature inside a module (stock takes, advanced reports, extra staff
/// logins). Checked against the signed entitlements (`license::entitlements`),
/// never anything the frontend sends.
fn require_capability(capability: &str) -> Result<(), String> {
    entitlements::require_any(&[capability])
}

/// `require_license`'s policy without the managed state, so every
/// `LicenseState` can be tested directly: `None` when billing may proceed,
/// otherwise the message the till shows.
fn license_block_reason(state: &LicenseState) -> Option<String> {
    if state.allows_billing() {
        return None;
    }
    Some(match state {
        LicenseState::Missing => "This installation has no licence. Enter a licence key in Settings.".into(),
        LicenseState::Expired { .. } => "This licence has expired. Renew it to continue billing.".into(),
        LicenseState::Revoked => "This licence has been revoked. Contact support.".into(),
        LicenseState::Suspended => "This licence has been suspended. Contact the Diwan team to restore it.".into(),
        LicenseState::DeviceMismatch => "This licence is bound to a different device. Contact support.".into(),
        LicenseState::ClockTampered { .. } => {
            "This device's clock appears to have been set back. Correct it, or connect to the internet and re-check the licence.".into()
        }
        LicenseState::CheckInRequired { .. } => {
            "This installation has been offline too long. Connect to the internet and re-check the licence.".into()
        }
        LicenseState::Invalid { message } => format!("Licence problem: {message}"),
        LicenseState::Ok { .. } => unreachable!("allows_billing() is true for Ok"),
    })
}

/// Shown when a sensitive-area command runs without a live area-PIN grant.
const AREA_PIN_REQUIRED: &str = "Enter the area PIN to open this screen";

/// Runs a domain query and flattens both failure layers (the connection
/// lock and the query's own error) into the message the frontend shows.
fn query<T, E: std::fmt::Display>(
    db: &Db,
    f: impl FnOnce(&rusqlite::Connection) -> Result<T, E>,
) -> Result<T, String> {
    db.with_conn(|conn| Ok(f(conn))).map_err(|e| e.to_string())?.map_err(|e| e.to_string())
}

/// `query` for a multi-table write: one transaction, committed only when
/// the domain function succeeds (see `Db::try_transaction`).
fn transact<T, E: std::fmt::Display>(
    db: &Db,
    f: impl FnOnce(&rusqlite::Transaction) -> Result<T, E>,
) -> Result<T, String> {
    db.try_transaction(f).map_err(|e| e.to_string())?.map_err(|e| e.to_string())
}

/// Enforce module availability on the compiled platform, independently of UI routing.
fn require_module(db: &Db, keys: &[&str]) -> Result<(), String> {
    // The plan comes first: a module the plan doesn't include is refused
    // with an upgrade message even if someone switched it on in Settings.
    entitlements::require_any(keys)?;
    db.with_conn(|conn| Ok(modules::require_enabled(conn, keys, Platform::current())))
        .map_err(|e| e.to_string())?
}

/// The module check plus the Owner/Admin check most admin screens' commands open with.
fn require_module_admin(db: &Db, keys: &[&str], session: &Session) -> Result<User, String> {
    require_module(db, keys)?;
    require_role(session, ADMIN_ROLES)
}

/// The module check plus the sensitive-area PIN grant (see `require_area_access`).
fn require_module_area(db: &Db, keys: &[&str], session: &Session, area_access: &AreaAccessSession) -> Result<User, String> {
    require_module(db, keys)?;
    require_area_access(session, area_access)
}

/// One module flag for this platform, for commands whose behaviour (not access)
/// depends on it. A read error fails the command rather than reading as "off".
fn module_enabled(db: &Db, key: &str) -> Result<bool, String> {
    db.with_conn(|conn| modules::is_enabled(conn, key, Platform::current()))
        .map_err(|e| e.to_string())
}

/// Fails a command unless someone is signed in *and* currently holds a valid
/// `AreaAccessSession` grant (see that module's doc comment) — the check
/// behind every command in the Attendance/Expenses/Salary/Reports sections
/// below. Deliberately role-independent: a correct "sensitive area" PIN is
/// the authorization, not the caller's role — Owner, Admin and Cashier all
/// go through the exact same gate here, matching `db::security_pin`'s
/// module doc comment. Still requires a live `Session` (not just "any grant
/// exists anywhere"), so a command can't succeed for literally nobody
/// signed in even if a grant happens to be live.
fn require_area_access(session: &Session, area_access: &AreaAccessSession) -> Result<User, String> {
    let user = require_signed_in(session)?;
    if area_access.is_valid() {
        Ok(user)
    } else {
        Err(AREA_PIN_REQUIRED.to_string())
    }
}

/// `reports_get_sales_over_time` specifically: shared between the
/// (area-PIN-gated) Reports screen and the Dashboard's trend chart —
/// Dashboard is Owner/Admin-only (`dashboard_get_summary`'s own
/// `require_role` check) and was never meant to need the area PIN at all,
/// so this command can't use a plain `require_area_access` the way the rest
/// of the Reports section does (that would newly break Dashboard for every
/// role, PIN or not) nor a plain `require_role(ADMIN_ROLES)` (that would
/// reject a Cashier who correctly unlocked Reports with the PIN). Owner/Admin
/// pass unconditionally, same as before this feature existed; a Cashier
/// needs a valid `AreaAccessSession` grant, same as the rest of Reports.
fn require_admin_or_area_access(
    session: &Session,
    area_access: &AreaAccessSession,
) -> Result<User, String> {
    let user = require_signed_in(session)?;
    if ADMIN_ROLES.contains(&user.role) || area_access.is_valid() {
        Ok(user)
    } else {
        Err(AREA_PIN_REQUIRED.to_string())
    }
}

/// Roles `caller_role` may assign when creating an account or changing an
/// existing one's role — never Owner, for anyone: there is exactly one
/// Owner account per installation (seeded at first run, see
/// `db::schema::seed_owner`), so it is never a role this UI hands out.
/// Mirrors `src/utils/permissions.ts`'s `assignableRoles`.
fn assignable_roles(caller_role: Role) -> &'static [Role] {
    match caller_role {
        Role::Owner => &[Role::Admin, Role::Cashier, Role::Delivery],
        Role::Admin => &[Role::Cashier, Role::Delivery],
        Role::Cashier | Role::Delivery => &[],
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
        Role::Admin => matches!(target_role, Role::Cashier | Role::Delivery),
        Role::Cashier | Role::Delivery => false,
    }
}

/// Pure authorization decision for `users_create`, factored out of the
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

/// Pure authorization decision for `users_update`. `target_current_role` is
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

/// Pure authorization decision for `users_set_active`.
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

/// Pure authorization decision for `users_set_pin`. Changing your own PIN is
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

/// Writes a frontend crash (caught by `RootErrorBoundary`) into the same
/// persistent log Rust startup failures go to, so support has one file to
/// ask for. Unauthenticated on purpose — a render crash can happen before
/// anyone signs in — and the text is clipped, since it came from the webview.
#[tauri::command]
pub fn app_log_frontend_error(message: String) {
    crate::startup_log::error(&format!("frontend: {}", crate::startup_log::clip(&message)));
}

/// `app_log_frontend_error`'s counterpart for a failure the app recovered
/// from (a preference that didn't save and was rolled back, a sign-out that
/// needed a retry): logged at WARN, not ERROR. Same clipping, same reason
/// for being unauthenticated.
#[tauri::command]
pub fn app_log_frontend_warning(message: String) {
    crate::startup_log::warn(&format!("frontend: {}", crate::startup_log::clip(&message)));
}

/// The splash window's own script finished loading and painting — a
/// diagnostic marker only: if a startup failure's log lacks this line, the
/// splash's assets never resolved. Revealing `main` waits on
/// `app_main_ready` instead, so the splash covers the app's own load.
/// `async` so it runs off the main thread (sync commands run on it).
#[tauri::command]
pub async fn app_splashscreen_ready(app: AppHandle) {
    crate::startup_log::log("app_splashscreen_ready invoked — splash content loaded and painted");
    if let Ok(mut painted) = app.state::<crate::SplashPaintedAt>().0.lock() {
        painted.get_or_insert_with(std::time::Instant::now);
    }
}

/// The main window's frontend has rendered its first screen: close the
/// splash and show `main`, once the splash has been up for `MIN_SPLASH_MS`
/// (counted from launch if the splash never reported painting). A no-op on
/// mobile and once already revealed (by the `MAX_SPLASH_MS` ceiling). `async`
/// because it may wait — a sync command would block the main thread, and
/// with it the very window it's about to show.
#[tauri::command]
pub async fn app_main_ready(app: AppHandle) {
    if app.get_webview_window("splashscreen").is_none() {
        return;
    }
    crate::startup_log::log("app_main_ready invoked — main window rendered its first screen");
    let shown_since = app
        .state::<crate::SplashPaintedAt>()
        .0
        .lock()
        .ok()
        .and_then(|painted| *painted)
        .unwrap_or(app.state::<crate::LaunchedAt>().0);
    let remaining = crate::MIN_SPLASH_MS.saturating_sub(shown_since.elapsed().as_millis() as u64);
    if remaining > 0 {
        tokio::time::sleep(std::time::Duration::from_millis(remaining)).await;
    }
    let handle = app.clone();
    // `reveal_main_window` polls with blocking sleeps; keep them off the
    // async runtime's worker.
    let _ = tauri::async_runtime::spawn_blocking(move || crate::reveal_main_window(&handle)).await;
}

/// The user tables currently present. Used to verify a migration landed.
#[tauri::command]
pub fn app_db_tables(session: State<'_, Session>, db: State<'_, Db>) -> Result<Vec<String>, String> {
    require_role(&session, ADMIN_ROLES)?;
    query(&db, crate::db::schema::table_names)
}

/// Current SQLite schema version, as written by the migration runner.
#[tauri::command]
pub fn app_db_version(session: State<'_, Session>, db: State<'_, Db>) -> Result<i64, String> {
    require_role(&session, ADMIN_ROLES)?;
    query(&db, |conn| conn.query_row("PRAGMA user_version", [], |row| row.get(0)))
}

// ---------------------------------------------------------------------------
// App config
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn config_get(db: State<'_, Db>) -> Result<AppConfig, String> {
    query(&db, config::get)
}

/// Read before login too — the login and lock screens are themed, and the
/// window has already painted `index.html`'s static `data-theme="dark"` by
/// the time this resolves, so a light-theme install sees at most one
/// repaint. See `db::config::UiPrefs` for why these aren't on `AppConfig`.
#[tauri::command]
pub fn config_get_ui_prefs(db: State<'_, Db>) -> Result<config::UiPrefs, String> {
    query(&db, config::get_ui_prefs)
}

/// Any signed-in user — the sidebar's own collapse toggle, desktop only.
#[tauri::command]
pub fn config_set_sidebar_collapsed(
    db: State<'_, Db>,
    session: State<'_, Session>,
    collapsed: bool,
) -> Result<(), String> {
    require_signed_in(&session)?;
    query(&db, |conn| config::set_sidebar_collapsed(conn, collapsed))
}

/// Any signed-in user, not just Owner/Admin: a display preference on the
/// till's own screen, with no business consequence.
#[tauri::command]
pub fn config_set_theme(db: State<'_, Db>, session: State<'_, Session>, theme: String) -> Result<(), String> {
    require_signed_in(&session)?;
    if theme != "dark" && theme != "light" {
        return Err(format!("Unknown theme '{theme}' — expected 'dark' or 'light'"));
    }
    query(&db, |conn| config::set_theme(conn, &theme))
}

/// Settings-only write — Owner/Admin, same as the screen it backs.
#[tauri::command]
pub fn config_update(
    db: State<'_, Db>,
    session: State<'_, Session>,
    vendor_gate: State<'_, VendorGate>,
    license: State<'_, LicenseRuntime>,
    patch: AppConfigUpdate,
) -> Result<AppConfig, String> {
    require_module_admin(&db, &[module_key::SETTINGS], &session)?;
    if let Some(tax) = patch.tax_percent {
        // Every sale's tax line is derived from this, so reject nonsense here.
        if !(0.0..=100.0).contains(&tax) {
            return Err("Tax percent must be between 0 and 100".into());
        }
    }
    if let Some(service) = patch.service_charge_percent {
        if !(0.0..=100.0).contains(&service) {
            return Err("Service charge must be between 0 and 100 percent".into());
        }
    }
    if let Some(prefix) = &patch.scale_barcode_prefix {
        if prefix.is_empty() || prefix.len() > 3 || !prefix.chars().all(|c| c.is_ascii_digit()) {
            return Err("Scale barcode prefix must be 1 to 3 digits".into());
        }
    }
    if let Some(digits) = patch.scale_item_code_digits {
        if !(3..=6).contains(&digits) {
            return Err("Scale item code must be 3 to 6 digits".into());
        }
    }
    if let Some(policy) = &patch.offline_sales {
        if policy != "allow" && policy != "block" {
            return Err("Offline sales must be \"allow\" or \"block\"".into());
        }
    }
    if let Some(kind) = &patch.scale_value_kind {
        if kind != "weight" && kind != "price" {
            return Err("Scale value must be weight or price".into());
        }
    }
    if let Some(decimals) = patch.scale_value_decimals {
        if !(0..=3).contains(&decimals) {
            return Err("Scale decimals must be 0 to 3".into());
        }
    }

    // The vendor gate's actual enforcement point. Completing first-run setup
    // is the single transition that turns a fresh copy of the installer into
    // a working store, so that is what's gated — not the wizard's UI, which
    // is only a route guard and (IPC being reachable from devtools) bypassable by
    // calling this command straight from a devtools console.
    //
    // Only the false -> true transition is gated. Once an install is set up,
    // `onboarding_completed` is already true, this branch never fires again,
    // and ordinary config edits are untouched — the gate is a setup-time
    // check, not a recurring login.
    if patch.onboarding_completed == Some(true) {
        let already_onboarded = db
            .with_conn(|conn| config::get(conn))
            .map_err(|e| e.to_string())?
            .onboarding_completed;
        if vendor_gate::setup_is_blocked(true, already_onboarded, vendor_gate.is_authorized()) {
            return Err("Vendor authorization is required to set up this installation".into());
        }
        // Same shape as the vendor gate, same reason it lives here: a fresh
        // install can't finish setup without a licence that verifies for
        // this device. The wizard's licence step is the UI; this is the
        // check.
        if !already_onboarded {
            require_license(&license).map_err(|e| format!("A valid licence is required to set up this installation. {e}"))?;
        }
    }

    query(&db, |conn| config::update(conn, patch))
}

/// Whether the first-run vendor gate still stands in the way on this
/// install, and whether this process has already cleared it.
///
/// `required` is false once setup has completed, which is what makes this a
/// one-time gate rather than a login: after that the frontend never shows
/// the prompt again. Deliberately unauthenticated and deliberately
/// uninformative — it reveals only whether setup has happened, which is
/// already obvious from the app showing a setup wizard.
#[tauri::command]
pub fn vendor_gate_status(
    db: State<'_, Db>,
    vendor_gate: State<'_, VendorGate>,
) -> Result<VendorGateStatus, String> {
    let onboarded = query(&db, |conn| config::get(conn))?.onboarding_completed;
    Ok(VendorGateStatus { required: !onboarded, authorized: vendor_gate.is_authorized() })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VendorGateStatus {
    pub required: bool,
    pub authorized: bool,
}

/// Verifies the vendor authorization password against the hash compiled
/// into the binary, recording a grant for the rest of this process on
/// success. See `vendor_gate`'s module doc comment for why verification
/// lives in Rust and what this does and doesn't protect against.
///
/// A wrong password costs a delay that grows with consecutive failures
/// (applied inside `VendorGate::verify`, before this returns), so scripted
/// guessing is impractical. Neither the attempt nor the hash is logged.
#[tauri::command]
pub fn vendor_gate_verify(vendor_gate: State<'_, VendorGate>, password: String) -> Result<(), String> {
    vendor_gate.verify(&password).map_err(|e| e.to_string())
}

/// The licence state as computed by Rust — see `license::state`. Read on
/// mount and whenever a `license:changed` event arrives. Unauthenticated:
/// it reveals only what the lock screen/banner is about to show anyway.
#[tauri::command]
pub fn license_get_state(license: State<'_, LicenseRuntime>) -> LicenseState {
    license.current()
}

/// What the settings screen shows alongside the state. The key is masked
/// to its first two groups — enough to match against the purchase email,
/// not enough to copy off a shared screen.
///
/// Unauthenticated on purpose, like every licence read: the lock screen
/// must be able to show it before anyone can sign in. Nothing here is
/// secret — a masked key and the public licence-server URL.
#[tauri::command]
pub fn license_get_info(db: State<'_, Db>) -> Result<LicenseInfo, String> {
    let stored = query(&db, crate::db::license::get)?;
    Ok(LicenseInfo {
        key_prefix: stored.as_ref().map(|s| mask_license_key(&s.license_key)),
        last_online_check: stored.as_ref().map(|s| s.last_online_check),
        api_base: crate::license::client::api_base(),
    })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseInfo {
    pub key_prefix: Option<String>,
    pub last_online_check: Option<i64>,
    pub api_base: String,
}

fn mask_license_key(key: &str) -> String {
    let mut groups = key.split('-');
    match (groups.next(), groups.next()) {
        (Some(a), Some(b)) => format!("{a}-{b}-••••-••••-••••"),
        _ => "••••".into(),
    }
}

/// "Re-check now": fetches a fresh signed status for the stored key.
/// Offline is not an error the user did anything about, so it comes back
/// as `Ok` with the unchanged state plus a `hint`; only a server *refusal*
/// (or a response that fails verification) rejects. Any signed-in user
/// may call it — it needs no credentials and only ever makes the state
/// more accurate.
#[tauri::command]
pub async fn license_refresh(
    app: AppHandle,
    db: State<'_, Db>,
    license: State<'_, LicenseRuntime>,
    session: State<'_, Session>,
) -> Result<LicenseRefreshOutcome, String> {
    require_signed_in(&session)?;
    match license.refresh(&app, &db).await {
        Ok(state) => Ok(LicenseRefreshOutcome { state, hint: None }),
        Err(
            crate::license::runtime::LicenseError::Unreachable(msg)
            | crate::license::runtime::LicenseError::ServerError(msg),
        ) => {
            Ok(LicenseRefreshOutcome { state: license.current(), hint: Some(msg) })
        }
        Err(e) => Err(e.to_string()),
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseRefreshOutcome {
    pub state: LicenseState,
    /// Set when the server couldn't be reached — the state shown is the
    /// cached one.
    pub hint: Option<String>,
}

/// Enters (or replaces) the licence key: activates it for this device on
/// the server, fetches and verifies its signed status, and stores it.
/// Owner/Admin only — it is a settings-screen and setup-wizard action —
/// and it needs the network; the error says so if there is none.
#[tauri::command]
pub async fn license_activate(
    app: AppHandle,
    db: State<'_, Db>,
    license: State<'_, LicenseRuntime>,
    session: State<'_, Session>,
    license_key: String,
) -> Result<LicenseState, String> {
    require_role(&session, ADMIN_ROLES)?;
    license.activate(&app, &db, &license_key).await.map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Cloud Sync (diwan.codehunts.co.uk) — push sales & metrics to cloud portal
// ---------------------------------------------------------------------------

/// Triggers an immediate cloud synchronization with diwan.codehunts.co.uk.
#[tauri::command]
pub async fn sync_now(app: AppHandle, session: State<'_, Session>) -> Result<crate::db::sync::SyncResult, String> {
    require_role(&session, ADMIN_ROLES)?;
    crate::sync::perform_sync(&app).await
}

/// Retrieves the current sync tracking info and statistics.
#[tauri::command]
pub fn sync_get_status(app: AppHandle, session: State<'_, Session>) -> Result<crate::db::sync::SyncStatusInfo, String> {
    require_signed_in(&session)?;
    crate::sync::get_status(&app)
}

// ---------------------------------------------------------------------------
// Business workspace — this counter's place among the business's counters
// (`workspace/`). Connecting, uploading the first data and replacing this
// counter's data with the business data are Owner/Admin actions; the status
// is for everyone (the top bar shows it).
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn workspace_get_info(db: State<'_, Db>, session: State<'_, Session>) -> Result<WorkspaceInfo, String> {
    require_signed_in(&session)?;
    query(&db, workspace::runtime::info)
}

/// Registers this device as a counter of its licence's business and says
/// what to do next (`upload`, `download`, `wait` or already `connected`).
#[tauri::command]
pub async fn workspace_connect(
    app: AppHandle,
    session: State<'_, Session>,
    counter_name: Option<String>,
) -> Result<workspace::runtime::ConnectOutcome, String> {
    require_role(&session, ADMIN_ROLES)?;
    workspace::runtime::connect(&app, counter_name).await
}

/// The business is empty: this counter's data becomes the business data.
#[tauri::command]
pub async fn workspace_upload(app: AppHandle, session: State<'_, Session>) -> Result<WorkspaceInfo, String> {
    require_role(&session, ADMIN_ROLES)?;
    workspace::runtime::upload(&app).await
}

/// Replaces this counter's data with the business data (backed up first;
/// returns the backup's path). Signs everyone out.
#[tauri::command]
pub async fn workspace_download(app: AppHandle, session: State<'_, Session>) -> Result<String, String> {
    require_role(&session, ADMIN_ROLES)?;
    workspace::runtime::download(&app).await
}

#[tauri::command]
pub fn workspace_disconnect(db: State<'_, Db>, session: State<'_, Session>) -> Result<(), String> {
    require_role(&session, ADMIN_ROLES)?;
    workspace::runtime::disconnect(&db)
}

#[tauri::command]
pub async fn workspace_rename_counter(app: AppHandle, session: State<'_, Session>, name: String) -> Result<WorkspaceInfo, String> {
    require_role(&session, ADMIN_ROLES)?;
    workspace::runtime::rename(&app, name).await
}

/// Syncs now and returns the result (the loop also runs every few seconds).
#[tauri::command]
pub async fn workspace_sync_now(app: AppHandle, db: State<'_, Db>, session: State<'_, Session>) -> Result<WorkspaceInfo, String> {
    require_signed_in(&session)?;
    // A failed round is recorded in the info's lastError, which the UI shows.
    let _ = workspace::runtime::round(&app).await;
    query(&db, workspace::runtime::info)
}

#[tauri::command]
pub fn workspace_dismiss_issue(db: State<'_, Db>, session: State<'_, Session>, id: i64) -> Result<WorkspaceInfo, String> {
    require_role(&session, ADMIN_ROLES)?;
    query(&db, |conn| {
        conn.execute("UPDATE sync_issues SET resolved_at = datetime('now', 'localtime') WHERE id = ?1", [id])?;
        workspace::runtime::info(conn)
    })
}

// ---------------------------------------------------------------------------
// App Store (iOS) — buying, restoring and moving a licence through Apple.
// All the logic is in `iap.rs`; these are the role-checked entry points.
// Owner/Admin only, like `license_activate`: a purchase replaces the
// installation's licence. Off iOS every one of them refuses.
// ---------------------------------------------------------------------------

/// The licence products, priced by the App Store for this user's storefront.
#[tauri::command]
pub async fn iap_get_products(app: AppHandle, session: State<'_, Session>) -> Result<Vec<IapProduct>, String> {
    require_role(&session, ADMIN_ROLES)?;
    iap::products(&app).await
}

/// Buys a licence and activates it on this device — see `iap.rs` for the
/// purchase → verify → activate → finish order that keeps this safe.
#[tauri::command]
pub async fn iap_purchase(
    app: AppHandle,
    db: State<'_, Db>,
    license: State<'_, LicenseRuntime>,
    account: State<'_, AppleAccount>,
    session: State<'_, Session>,
    product_id: String,
) -> Result<PurchaseResult, String> {
    require_role(&session, ADMIN_ROLES)?;
    iap::purchase(&app, &db, &license, &account, &product_id).await
}

/// "Restore purchases" — also how an App Store licence moves to a new device.
#[tauri::command]
pub async fn iap_restore(
    app: AppHandle,
    db: State<'_, Db>,
    license: State<'_, LicenseRuntime>,
    account: State<'_, AppleAccount>,
    session: State<'_, Session>,
) -> Result<LicenseState, String> {
    require_role(&session, ADMIN_ROLES)?;
    iap::restore(&app, &db, &license, &account).await
}

/// Optional Sign in with Apple, to link purchases to a Diwan web account.
/// `None` = the user dismissed Apple's sheet.
#[tauri::command]
pub async fn iap_apple_sign_in(
    app: AppHandle,
    license: State<'_, LicenseRuntime>,
    account: State<'_, AppleAccount>,
    session: State<'_, Session>,
) -> Result<Option<AppleAccountInfo>, String> {
    require_role(&session, ADMIN_ROLES)?;
    iap::sign_in(&app, &license, &account).await
}

/// Who is signed in with Apple in this app run, if anyone.
#[tauri::command]
pub fn iap_apple_account(account: State<'_, AppleAccount>) -> Option<AppleAccountInfo> {
    account.info()
}

#[tauri::command]
pub fn iap_apple_sign_out(account: State<'_, AppleAccount>) {
    account.sign_out();
}

/// Deletes the Diwan account behind Sign in with Apple (re-authenticates
/// first). `false` = dismissed. The licence on this device keeps working.
#[tauri::command]
pub async fn iap_delete_account(
    app: AppHandle,
    account: State<'_, AppleAccount>,
    session: State<'_, Session>,
) -> Result<bool, String> {
    require_role(&session, ADMIN_ROLES)?;
    iap::delete_account(&app, &account).await
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
    require_module_admin(&db, &[module_key::SETTINGS], &session)?;
    let previous_logo = query(&db, config::get)?.logo_path;

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
/// `<img src>` — same shape, and same sign-in check, as `inventory_get_image`
/// (every caller — sidebar, top bar, settings, receipt — is past login).
#[tauri::command]
pub fn config_get_logo(app: AppHandle, session: State<'_, Session>, file_name: String) -> Result<String, String> {
    require_signed_in(&session)?;
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
pub fn config_get_enabled_modules(
    db: State<'_, Db>,
    platform: String,
) -> Result<Vec<ModuleState>, String> {
    let platform = parse_platform(&platform)?;
    let mut list = query(&db, |conn| modules::list(conn, platform))?;
    for module in &mut list {
        module.entitled = entitlements::allows(&module.key);
        module.required_plan = entitlements::upgrade_for(&module.key);
    }
    Ok(list)
}

/// The licence's plan and what it unlocks, for the upgrade messages and the
/// licence screen. Presentation only: every command re-checks in Rust.
#[tauri::command]
pub fn license_get_plan() -> entitlements::Snapshot {
    entitlements::snapshot()
}

/// Settings-only write — Owner/Admin, same as the screen it backs.
#[tauri::command]
pub fn config_toggle_module(
    db: State<'_, Db>,
    session: State<'_, Session>,
    module_key: String,
    platform: String,
    enabled: bool,
) -> Result<Vec<ModuleState>, String> {
    require_module_admin(&db, &[module_key::SETTINGS], &session)?;
    let platform = parse_platform(&platform)?;

    // The domain error (core module, unknown key) reaches the Settings screen verbatim.
    query(&db, |conn| modules::set_enabled(conn, &module_key, platform, enabled))?;
    query(&db, |conn| modules::list(conn, platform))
}

// ---------------------------------------------------------------------------
// Inventory
// ---------------------------------------------------------------------------

/// Items matching the search/category filter. Archived items are excluded
/// unless the caller asks for them. No `require_module(&["inventory"])` on
/// purpose: Billing, Tables and Reports read the catalogue too, so it must
/// stay readable with the Inventory screen switched off. Only writes check it.
#[tauri::command]
pub fn inventory_get_items(session: State<'_, Session>, db: State<'_, Db>, query: ItemQuery) -> Result<Vec<Item>, String> {
    require_signed_in(&session)?;
    // `self::` because the `query` argument shadows the helper.
    self::query(&db, |conn| items::list_items(conn, &query))
}

/// Write — Cashiers get read-only Inventory (see `permissions.ts`), so
/// this and every other inventory write below require Owner/Admin.
#[tauri::command]
pub fn inventory_add_item(
    db: State<'_, Db>,
    session: State<'_, Session>,
    input: ItemInput,
) -> Result<Item, String> {
    require_module_admin(&db, &[module_key::INVENTORY], &session)?;
    query(&db, |conn| items::add_item(conn, input))
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
    require_module_admin(&db, &[module_key::INVENTORY], &session)?;
    let previous_image = current_image_path(&db, id);

    let updated = query(&db, |conn| items::update_item(conn, id, input))?;

    if let Some(old_image) = previous_image {
        if updated.image_path.as_deref() != Some(old_image.as_str()) {
            delete_photo_if_unused(&app, &db, &old_image)?;
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
    require_module_admin(&db, &[module_key::INVENTORY], &session)?;
    let existing_image = current_image_path(&db, id);

    let outcome = query(&db, |conn| items::delete_item(conn, id))?;

    if outcome == DeleteOutcome::Deleted {
        if let Some(image) = existing_image {
            delete_photo_if_unused(&app, &db, &image)?;
        }
    }

    Ok(outcome)
}

/// The item's stored photo filename before an edit/delete, so the command
/// can remove the file afterwards. `None` if the item or photo is missing —
/// the write itself reports a missing item.
fn current_image_path(db: &Db, id: i64) -> Option<String> {
    query(db, |conn| items::get_item(conn, id)).ok().and_then(|item| item.image_path)
}

/// Photos are named by their content, so two products can share one file:
/// it goes only when no product uses it any more.
fn delete_photo_if_unused(app: &AppHandle, db: &Db, file_name: &str) -> Result<(), String> {
    let in_use: bool = query(db, |conn| {
        conn.query_row("SELECT EXISTS (SELECT 1 FROM items WHERE image_path = ?1)", [file_name], |row| row.get(0))
    })?;
    if !in_use {
        images::delete_image(&images_dir(app)?, file_name);
    }
    Ok(())
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
    require_role(&session, ADMIN_ROLES)?;
    images::save_image(&images_dir(&app)?, &data_base64, &extension).map_err(|e| e.to_string())
}

/// Reads a stored image back as a `data:` URL for direct use as an `<img src>`.
#[tauri::command]
pub fn inventory_get_image(session: State<'_, Session>, app: AppHandle, file_name: String) -> Result<String, String> {
    require_signed_in(&session)?;
    images::read_image_data_url(&images_dir(&app)?, &file_name).map_err(|e| e.to_string())
}

/// The next unused short code, for the "Add item" form to pre-fill — an
/// Owner/Admin can still type over it. `None` once even a 4-digit code
/// space is exhausted (see `items::suggest_next_short_code`'s doc comment).
#[tauri::command]
pub fn inventory_suggest_short_code(session: State<'_, Session>, db: State<'_, Db>) -> Result<Option<String>, String> {
    require_signed_in(&session)?;
    query(&db, items::suggest_next_short_code)
}

/// Module-free like `inventory_get_items` (Billing's category pills need it).
#[tauri::command]
pub fn inventory_get_categories(session: State<'_, Session>, db: State<'_, Db>) -> Result<Vec<Category>, String> {
    require_signed_in(&session)?;
    query(&db, items::list_categories)
}

#[tauri::command]
pub fn inventory_add_category(
    db: State<'_, Db>,
    session: State<'_, Session>,
    name: String,
) -> Result<Category, String> {
    require_module_admin(&db, &[module_key::INVENTORY], &session)?;
    query(&db, |conn| items::add_category(conn, &name))
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
    require_module_admin(&db, &[module_key::INVENTORY], &session)?;
    require_capability(entitlements::capability::INVENTORY_ADVANCED)?;
    query(&db, |conn| csv_import::import_csv(conn, &csv_content))
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
pub fn inventory_get_best_selling_item_ids(session: State<'_, Session>, 
    db: State<'_, Db>,
    period_days: i64,
    limit: i64,
) -> Result<Vec<i64>, String> {
    require_signed_in(&session)?;
    query(&db, |conn| reports::get_best_selling_item_ids(conn, period_days, limit))
}

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

/// Accounts to offer on the login screen. Never includes PIN hashes. Called
/// before anyone is signed in, so — unlike everything below it — this one
/// deliberately has no `require_role` check.
#[tauri::command]
pub fn users_list_active(db: State<'_, Db>) -> Result<Vec<User>, String> {
    query(&db, users::list_active)
}

/// The only place `Session` is ever written to from a value the JS side
/// supplied — and even here, `users::authenticate` (an argon2 check against
/// the stored hash) is what decides success, not the caller's say-so.
#[tauri::command]
pub fn auth_login(
    db: State<'_, Db>,
    session: State<'_, Session>,
    limiter: State<'_, AttemptLimiter>,
    user_id: i64,
    pin: String,
) -> Result<User, String> {
    let user = limiter.attempt(&format!("login:{user_id}"), || {
        query(&db, |conn| users::authenticate(conn, user_id, &pin))
    })?;
    // A single-user plan signs in the Owner only. Checked after the PIN, so
    // it never tells anyone without the PIN which accounts exist.
    if user.role != Role::Owner {
        require_capability(entitlements::capability::MULTI_USER)?;
    }
    session.set(user.clone());
    Ok(user)
}

/// Clears the server-side session. Safe to call whether or not anyone was
/// signed in. Also revokes any live `AreaAccessSession` grant — a
/// PIN-unlocked visit to one of the five gated screens must not survive a
/// different account signing in on the same till right after.
#[tauri::command]
pub fn auth_logout(session: State<'_, Session>, area_access: State<'_, AreaAccessSession>) {
    session.clear();
    area_access.revoke();
}

/// Every account, active or not — the user management screen. Owner/Admin
/// only, like every other command in this block from here down.
#[tauri::command]
pub fn users_list_all(db: State<'_, Db>, session: State<'_, Session>) -> Result<Vec<ManagedUser>, String> {
    require_role(&session, ADMIN_ROLES)?;
    query(&db, users::list_all)
}

/// Creates a staff account. Owner is never an assignable role here — there
/// is exactly one Owner account per installation, seeded at first run (see
/// `db::schema::seed_owner`) — and an Admin may only create Cashier
/// accounts, never a peer Admin. Checked against the caller `require_role`
/// actually returned, not a role the client claims, so this holds even for
/// a direct `invoke()` call that skips the UI entirely.
#[tauri::command]
pub fn users_create(
    db: State<'_, Db>,
    session: State<'_, Session>,
    name: String,
    pin: String,
    role: Role,
) -> Result<User, String> {
    let caller = require_module_admin(&db, &[module_key::USERS], &session)?;
    require_capability(entitlements::capability::MULTI_USER)?;
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
pub fn users_update(
    db: State<'_, Db>,
    session: State<'_, Session>,
    user_id: i64,
    name: String,
    role: Role,
) -> Result<User, String> {
    let caller = require_module_admin(&db, &[module_key::USERS], &session)?;
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
pub fn users_set_active(
    db: State<'_, Db>,
    session: State<'_, Session>,
    user_id: i64,
    is_active: bool,
) -> Result<(), String> {
    let caller = require_module_admin(&db, &[module_key::USERS], &session)?;
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
/// `users_update`: changing your own PIN is always allowed regardless of
/// role, but resetting someone else's follows the same Owner/Admin
/// hierarchy — an Admin may reset a Cashier's PIN, never the Owner's or
/// another Admin's.
#[tauri::command]
pub fn users_set_pin(
    db: State<'_, Db>,
    session: State<'_, Session>,
    user_id: i64,
    pin: String,
) -> Result<(), String> {
    let caller = require_module_admin(&db, &[module_key::USERS], &session)?;
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
pub fn billing_search_items(session: State<'_, Session>, db: State<'_, Db>, query: String) -> Result<Vec<Item>, String> {
    require_module(&db, &[module_key::BILLING])?;
    require_signed_in(&session)?;
    // `self::` because the `query` argument shadows the helper.
    self::query(&db, |conn| items::search_items(conn, &query, 20))
}

/// Completes a sale: re-prices every line against live stock, writes
/// `sales` + `sale_items`, decrements stock, and (if a table was given)
/// closes that table's parked order and frees the table — all inside one
/// SQLite transaction, so a failure at any step leaves nothing behind.
///
/// The cashier and shift are derived from the session, never from the
/// client — the same "never trust a client-claimed identity" rule as
/// `refunds_create`, so a sale can't be attributed to another cashier or
/// booked against someone else's cash drawer.
#[tauri::command]
///
/// On a counter connected to its business, the sale's stock is first taken
/// from the central balance (`workspace::runtime::reserve_for_sale`), so two
/// counters cannot both sell the last unit; with no answer from the server
/// the sale is recorded anyway and its stock syncs when the counter is back
/// online.
pub async fn billing_create_sale(
    app: AppHandle,
    db: State<'_, Db>,
    session: State<'_, Session>,
    license: State<'_, LicenseRuntime>,
    mut input: CreateSaleInput,
) -> Result<Sale, String> {
    require_module(&db, &[module_key::BILLING])?;
    let caller = require_signed_in(&session)?;
    require_license(&license)?;
    input.cashier_id = Some(caller.id);
    let shifts_enabled = module_enabled(&db, module_key::SHIFTS)?;
    input.shift_id = if shifts_enabled {
        query(&db, |conn| shifts::get_open_shift_for_cashier(conn, caller.id))?
            .map(|shift| shift.id)
    } else {
        None
    };
    let lines: Vec<(i64, f64)> = input.items.iter().map(|line| (line.item_id, line.qty)).collect();
    let reservation = workspace::runtime::reserve_for_sale(&db, &lines).await?;
    let sale = transact(&db, |tx| workspace::runtime::record_sale(tx, reservation.as_deref(), |tx| sales::create_sale(tx, input)));
    if let (Err(_), Some(key)) = (&sale, &reservation) {
        workspace::runtime::abandon_sale(&db, key);
    }
    let sale = sale?;
    crate::sync::spawn_after_write(&app);
    Ok(sale)
}

/// Fetches a completed sale — used for receipt reprint.
#[tauri::command]
pub fn billing_get_sale(session: State<'_, Session>, db: State<'_, Db>, id: i64) -> Result<Sale, String> {
    require_module(&db, &[module_key::BILLING])?;
    require_signed_in(&session)?;
    query(&db, |conn| sales::get_sale(conn, id))
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
pub fn billing_print_receipt_thermal(session: State<'_, Session>, app: AppHandle, db: State<'_, Db>, sale_id: i64) -> Result<(), String> {
    require_module(&db, &[module_key::BILLING])?;
    require_signed_in(&session)?;
    let sale = query(&db, |conn| sales::get_sale(conn, sale_id))?;
    let app_config = query(&db, config::get)?;
    let tables_module_enabled = module_enabled(&db, module_key::TABLES)?;

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
// Android (two transports — Bluetooth and USB-OTG) and Windows have real
// pickers; macOS/Linux's USB transport is auto-detected (see
// `printer::escpos`'s module doc comment) and needs no selection step, so
// every command here simply reports "not applicable" off its own platform
// rather than being registered conditionally, which would mean the frontend
// service layer needing to know at compile/call time which platform it's
// running on before even trying (it already knows via `IS_ANDROID`, but
// keeping the command surface uniform across platforms is one less thing to
// get wrong).
// ---------------------------------------------------------------------------

/// Every Bluetooth device already paired through the OS settings — the
/// candidate list for Settings' printer picker. Android only; always empty
/// on desktop.
#[tauri::command]
pub fn printer_list_bluetooth_devices(session: State<'_, Session>) -> Result<Vec<crate::db::config::BluetoothDeviceOption>, String> {
    require_signed_in(&session)?;
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
pub fn printer_bluetooth_permission_granted(session: State<'_, Session>) -> Result<bool, String> {
    require_signed_in(&session)?;
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
pub fn printer_request_bluetooth_permission(session: State<'_, Session>) -> Result<(), String> {
    require_signed_in(&session)?;
    #[cfg(target_os = "android")]
    {
        catch_panic(|| crate::printer::android_bt::request_permission().map_err(|e| e.to_string()))
    }
    #[cfg(not(target_os = "android"))]
    {
        Ok(())
    }
}

/// Every USB device currently attached (Android USB-OTG) — the candidate
/// list for Settings' printer picker in USB mode. Android only; always empty
/// elsewhere, same "uniform command surface, empty result off-platform"
/// convention as the Bluetooth commands above.
///
/// Nothing is filtered out by device class: many thermal printers, the
/// client's Gainscha GA-E200 included, report a vendor-specific interface
/// rather than the standard Printer class, so `isPrinterClass` sorts and
/// labels the list instead of gating it — see `printer::android_usb`.
#[tauri::command]
pub fn printer_list_usb_devices(session: State<'_, Session>) -> Result<Vec<crate::db::config::UsbDeviceOption>, String> {
    require_signed_in(&session)?;
    #[cfg(target_os = "android")]
    {
        catch_panic(|| {
            crate::printer::android_usb::list_devices()
                .map(|devices| {
                    devices
                        .into_iter()
                        .map(|d| crate::db::config::UsbDeviceOption {
                            name: d.name,
                            id: d.id,
                            is_printer_class: d.is_printer_class,
                            has_permission: d.has_permission,
                        })
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

/// Whether the user has approved this specific attached USB device.
///
/// Per *device*, unlike `printer_bluetooth_permission_granted`'s app-wide
/// answer — that's Android's USB model, not a quirk of ours: the grant is
/// tied to the device and is dropped when it's unplugged. Always `true` on
/// desktop (nothing to grant).
#[tauri::command]
pub fn printer_usb_permission_granted(session: State<'_, Session>, device_id: String) -> Result<bool, String> {
    require_signed_in(&session)?;
    #[cfg(target_os = "android")]
    {
        catch_panic(|| crate::printer::android_usb::permission_granted(&device_id).map_err(|e| e.to_string()))
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = device_id;
        Ok(true)
    }
}

/// Fires Android's system "Allow access to the USB device?" dialog for this
/// device. Fire-and-forget: nothing receives the grant broadcast, so the
/// frontend re-checks `printer_usb_permission_granted` afterwards — see
/// `android_usb::request_permission`'s doc comment for why that's the right
/// shape rather than a limitation. A no-op on desktop.
#[tauri::command]
pub fn printer_request_usb_permission(session: State<'_, Session>, device_id: String) -> Result<(), String> {
    require_signed_in(&session)?;
    #[cfg(target_os = "android")]
    {
        catch_panic(|| crate::printer::android_usb::request_permission(&device_id).map_err(|e| e.to_string()))
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = device_id;
        Ok(())
    }
}

/// Every printer Windows currently has installed — the candidate list for
/// Settings' printer picker on Windows (the Windows equivalent of
/// `printer_list_bluetooth_devices`). Windows only; always empty elsewhere,
/// same "uniform command surface, empty result off-platform" convention as
/// the Bluetooth commands above.
#[tauri::command]
pub fn printer_list_windows_printers(session: State<'_, Session>) -> Result<Vec<crate::db::config::WindowsPrinterOption>, String> {
    require_signed_in(&session)?;
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
    require_module_admin(&db, &[module_key::SETTINGS], &session)?;
    let config = query(&db, config::get)?;
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
    require_module_admin(&db, &[module_key::BILLING], &session)?;
    query(&db, |conn| sales::list_recent(conn, limit))
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
pub fn refunds_get_sale(db: State<'_, Db>, session: State<'_, Session>, sale_id: i64) -> Result<RefundableSale, String> {
    require_module_admin(&db, &[module_key::BILLING], &session)?;
    query(&db, |conn| refunds::get_sale_for_refund(conn, sale_id))
}

/// Creates a refund: re-validates every line against the live sale/prior
/// refunds, writes `refunds` + `refund_items`, and puts the refunded
/// quantity back onto `items.stock_qty` — all inside one transaction (see
/// `db::refunds::create_refund`). `refundedBy` is deliberately not a
/// parameter here — it's always the signed-in caller, the same
/// "never trust a client-claimed identity" rule `session.rs`'s module doc
/// explains for role checks.
#[tauri::command]
pub fn refunds_create(
    app: AppHandle,
    db: State<'_, Db>,
    session: State<'_, Session>,
    license: State<'_, LicenseRuntime>,
    sale_id: i64,
    items: Vec<RefundLineInput>,
    reason: Option<String>,
) -> Result<Refund, String> {
    let caller = require_module_admin(&db, &[module_key::BILLING], &session)?;
    require_license(&license)?;
    let refund = transact(&db, |tx| {
        refunds::create_refund(
            tx,
            CreateRefundInput { sale_id, items, reason, refunded_by: Some(caller.id) },
        )
    })?;
    crate::sync::spawn_after_write(&app);
    Ok(refund)
}

/// Re-fetches a previously created refund — used to reprint its receipt.
#[tauri::command]
pub fn refunds_get(db: State<'_, Db>, session: State<'_, Session>, refund_id: i64) -> Result<Refund, String> {
    require_module_admin(&db, &[module_key::BILLING], &session)?;
    query(&db, |conn| refunds::get_refund(conn, refund_id))
}

/// Prints the "Refund Details" receipt on a USB thermal printer — same
/// auto-detect/fall-back-to-PDF contract as `billing_print_receipt_thermal`.
#[tauri::command]
pub fn refunds_print_thermal(db: State<'_, Db>, session: State<'_, Session>, refund_id: i64) -> Result<(), String> {
    require_module_admin(&db, &[module_key::BILLING], &session)?;
    let refund = query(&db, |conn| refunds::get_refund(conn, refund_id))?;
    let app_config = query(&db, config::get)?;
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

/// The signed-in cashier's currently-open shift, if any — lets the billing
/// screen decide whether to show "Open Shift" or "Close Shift".
#[tauri::command]
pub fn shifts_get_open(db: State<'_, Db>, session: State<'_, Session>) -> Result<Option<Shift>, String> {
    require_module(&db, &[module_key::SHIFTS])?;
    let caller = require_signed_in(&session)?;
    query(&db, |conn| shifts::get_open_shift_for_cashier(conn, caller.id))
}

/// Opens a new shift for the signed-in cashier. Refuses if they already
/// have one open.
#[tauri::command]
pub fn shifts_open(
    db: State<'_, Db>,
    session: State<'_, Session>,
    opening_balance_minor: i64,
) -> Result<Shift, String> {
    require_module(&db, &[module_key::SHIFTS])?;
    let caller = require_signed_in(&session)?;
    query(&db, |conn| shifts::open_shift(conn, caller.id, opening_balance_minor))
}

/// Reconciliation breakdown for `shiftId`. `declaredCashAmountMinor` lets
/// the caller preview "if I declared this much, what would Short/Over be"
/// before confirming a close (see `db::shifts::get_shift_summary`) — pass
/// `None` to read what's actually stored (still-open: nothing declared yet;
/// closed: what was recorded at close time, for reprinting a past shift).
/// The shift's own cashier may read it (the close-shift confirmation step
/// needs it before anything is closed), as may Owner/Admin — never another
/// cashier, since it is that drawer's cash figures.
#[tauri::command]
pub fn shifts_get_summary(
    db: State<'_, Db>,
    session: State<'_, Session>,
    shift_id: i64,
    declared_cash_amount_minor: Option<i64>,
) -> Result<ShiftSummary, String> {
    require_module(&db, &[module_key::SHIFTS])?;
    let caller = require_signed_in(&session)?;
    let summary = query(&db, |conn| shifts::get_shift_summary(conn, shift_id, declared_cash_amount_minor))?;
    if !may_act_on_shift(&caller, summary.shift.cashier_id) {
        return Err("You don't have permission to view this shift".to_string());
    }
    Ok(summary)
}

/// A cashier may act on (read, print, close) only their own shift;
/// Owner/Admin may act on anyone's — e.g. closing out for a cashier who
/// already left.
fn may_act_on_shift(caller: &User, shift_cashier_id: Option<i64>) -> bool {
    shift_cashier_id == Some(caller.id) || ADMIN_ROLES.contains(&caller.role)
}

/// Closes `shiftId` with the cashier's declared cash count. The shift's own
/// cashier may close it, or an Owner/Admin may close it on their behalf
/// (e.g. the cashier already left) — anyone else is refused.
#[tauri::command]
pub fn shifts_close(
    db: State<'_, Db>,
    session: State<'_, Session>,
    shift_id: i64,
    declared_cash_amount_minor: i64,
) -> Result<ShiftSummary, String> {
    require_module(&db, &[module_key::SHIFTS])?;
    let caller = require_signed_in(&session)?;
    let shift = query(&db, |conn| shifts::get_shift_summary(conn, shift_id, None))?
        .shift;
    if !may_act_on_shift(&caller, shift.cashier_id) {
        return Err("You don't have permission to close this shift".to_string());
    }

    transact(&db, |tx| shifts::close_shift(tx, shift_id, declared_cash_amount_minor))
}

/// Shift history for the Shifts page — Owner/Admin only, the standalone
/// page's tier (see MODULE_CATALOGUE's doc comment on the `shifts` module).
#[tauri::command]
pub fn shifts_list_recent(db: State<'_, Db>, session: State<'_, Session>, limit: i64) -> Result<Vec<Shift>, String> {
    require_module_admin(&db, &[module_key::SHIFTS], &session)?;
    query(&db, |conn| shifts::list_shifts(conn, limit))
}

/// Every cash drawer of one day, across all counters of the business, with
/// totals — Owner/Admin, like the rest of the Shifts page.
#[tauri::command]
pub fn shifts_get_day_summary(db: State<'_, Db>, session: State<'_, Session>, day: String) -> Result<shifts::DrawerDay, String> {
    require_module_admin(&db, &[module_key::SHIFTS], &session)?;
    if chrono::NaiveDate::parse_from_str(&day, "%Y-%m-%d").is_err() {
        return Err("Pick a valid date".to_string());
    }
    query(&db, |conn| shifts::day_summary(conn, &day))
}

/// Prints a shift's close-out reconciliation receipt.
#[tauri::command]
pub fn shifts_print_summary(db: State<'_, Db>, session: State<'_, Session>, shift_id: i64) -> Result<(), String> {
    require_module(&db, &[module_key::SHIFTS])?;
    let caller = require_signed_in(&session)?;
    let summary = query(&db, |conn| shifts::get_shift_summary(conn, shift_id, None))?;
    if !may_act_on_shift(&caller, summary.shift.cashier_id) {
        return Err("You don't have permission to print this shift".to_string());
    }
    let app_config = query(&db, config::get)?;
    crate::printer::escpos::print_shift_summary(&summary, &app_config).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Reports. Gated by the shared area PIN (see the "Sensitive area PIN"
// section further down), not by role — Owner, Admin and Cashier all reach
// these once `AreaAccessSession` has a valid grant. A cashier hitting any
// of these directly (bypassing the UI, which already gates the whole
// screen behind `AreaPinGate`) without ever having verified the PIN still
// gets refused here.
// ---------------------------------------------------------------------------

/// Total sales, transaction count and average sale value for `start_date`..
/// `end_date` (inclusive, `YYYY-MM-DD`).
#[tauri::command]
pub fn reports_get_sales_summary(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    start_date: String,
    end_date: String,
) -> Result<SalesSummary, String> {
    require_module_area(&db, &[module_key::REPORTS], &session, &area_access)?;
    query(&db, |conn| reports::get_sales_summary(conn, &start_date, &end_date))
}

/// Best-selling items in the range, ranked by quantity or revenue.
#[tauri::command]
pub fn reports_get_top_items(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    start_date: String,
    end_date: String,
    limit: i64,
    sort_by: TopItemSort,
) -> Result<Vec<TopItem>, String> {
    require_module_area(&db, &[module_key::REPORTS], &session, &area_access)?;
    query(&db, |conn| reports::get_top_items(conn, &start_date, &end_date, limit, sort_by))
}

/// One row per calendar day in the range (zero-filled where there were no
/// sales), for the reports chart. Also used by the Dashboard's trend chart —
/// see `require_admin_or_area_access`'s doc comment for why this one
/// command needs a different check than the rest of this section.
#[tauri::command]
pub fn reports_get_sales_over_time(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    start_date: String,
    end_date: String,
) -> Result<Vec<DailySales>, String> {
    require_module(&db, &[module_key::REPORTS, module_key::DASHBOARD])?;
    require_admin_or_area_access(&session, &area_access)?;
    query(&db, |conn| reports::get_sales_over_time(conn, &start_date, &end_date))
}

/// The Category Wise Sale report: every item sold in the range, grouped by
/// category with a subtotal per category and a grand total.
#[tauri::command]
pub fn reports_get_category_sales(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    start_date: String,
    end_date: String,
) -> Result<CategorySalesReport, String> {
    require_module_area(&db, &[module_key::REPORTS], &session, &area_access)?;
    query(&db, |conn| reports::get_category_sales(conn, &start_date, &end_date))
}

/// The shared body of every `reports_print_*` command: the Reports module
/// and area-PIN checks, the report fetch, then the thermal print.
fn print_area_report<R, E: std::fmt::Display, P: std::fmt::Display>(
    db: &Db,
    session: &Session,
    area_access: &AreaAccessSession,
    fetch: impl FnOnce(&rusqlite::Connection) -> Result<R, E>,
    print: impl FnOnce(&R, &AppConfig) -> Result<(), P>,
) -> Result<(), String> {
    require_module(db, &[module_key::REPORTS])?;
    require_area_access(session, area_access)?;
    let report = query(db, fetch)?;
    let app_config = query(&db, config::get)?;
    print(&report, &app_config).map_err(|e| e.to_string())
}

/// Prints the Category Wise Sale report on a USB thermal printer.
#[tauri::command]
pub fn reports_print_category_sales(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    start_date: String,
    end_date: String,
) -> Result<(), String> {
    print_area_report(
        &db,
        &session,
        &area_access,
        |conn| reports::get_category_sales(conn, &start_date, &end_date),
        crate::printer::escpos::print_category_sales,
    )
}

/// The Table Wise Sales report: one row per table plus a "Counter /
/// Takeaway" row for sales with no table, sorted by amount and summing to
/// the same gross total `reports_get_sales_summary` reports for the same
/// range. Gated on the `reports` module like the rest of this section, but
/// deliberately not on `tables`: historic table sales stay reportable after
/// the module is switched off — the Reports screen simply doesn't offer
/// this view unless `tables` is on.
#[tauri::command]
pub fn reports_get_table_sales_summary(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    start_date: String,
    end_date: String,
) -> Result<TableSalesSummary, String> {
    require_module_area(&db, &[module_key::REPORTS], &session, &area_access)?;
    query(&db, |conn| reports::get_table_sales_summary(conn, &start_date, &end_date))
}

/// "Counter Wise Sales": the range's sales per counter (POS terminal) and per
/// cashier. On a counter connected to its business this covers every
/// counter, since each holds the whole business.
#[tauri::command]
pub fn reports_get_counter_sales_summary(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    start_date: String,
    end_date: String,
) -> Result<reports::CounterSalesSummary, String> {
    require_module_area(&db, &[module_key::REPORTS], &session, &area_access)?;
    query(&db, |conn| reports::get_counter_sales_summary(conn, &start_date, &end_date))
}

/// The "Product Wise Sales" report: every item sold in the range, ranked by
/// revenue or quantity (`sort_by`), optionally narrowed to one category, plus
/// a "no sales this period" list of active items that sold zero units — so
/// slow-moving stock stays visible rather than just absent from the report.
#[tauri::command]
pub fn reports_get_product_sales_summary(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    start_date: String,
    end_date: String,
    category_id: Option<i64>,
    sort_by: TopItemSort,
) -> Result<ProductSalesSummaryReport, String> {
    require_module_area(&db, &[module_key::REPORTS], &session, &area_access)?;
    query(&db, |conn| reports::get_product_sales_summary(conn, &start_date, &end_date, category_id, sort_by))
}

/// Prints the Table Wise Sales report on a USB thermal printer.
#[tauri::command]
pub fn reports_print_table_sales_summary(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    start_date: String,
    end_date: String,
) -> Result<(), String> {
    print_area_report(
        &db,
        &session,
        &area_access,
        |conn| reports::get_table_sales_summary(conn, &start_date, &end_date),
        crate::printer::escpos::print_table_sales,
    )
}

/// The "Refunds" report: every refund recorded in the range (by refund
/// date, not the date of the original sale), most recent first, itemized
/// with the original sale/receipt number (`originalSaleId`), refunded
/// item(s), reason, who processed it, and a grand total refunded for the
/// period.
#[tauri::command]
pub fn reports_get_refunds_summary(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    start_date: String,
    end_date: String,
) -> Result<RefundsSummary, String> {
    require_module_area(&db, &[module_key::REPORTS], &session, &area_access)?;
    query(&db, |conn| reports::get_refunds_summary(conn, &start_date, &end_date))
}

/// Prints the Refunds report on a USB thermal printer.
#[tauri::command]
pub fn reports_print_refunds_summary(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    start_date: String,
    end_date: String,
) -> Result<(), String> {
    print_area_report(
        &db,
        &session,
        &area_access,
        |conn| reports::get_refunds_summary(conn, &start_date, &end_date),
        crate::printer::escpos::print_refunds_summary,
    )
}

/// Delivery report: summaries for in-house and third-party deliveries, platform breakdown,
/// rider breakdown (grouped by normalized phone for manual riders), and detailed orders.
#[tauri::command]
pub fn reports_get_delivery_report(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    start_date: String,
    end_date: String,
) -> Result<reports::DeliveryReport, String> {
    require_module_area(&db, &[module_key::DELIVERY], &session, &area_access)?;
    query(&db, |conn| reports::get_delivery_report(conn, &start_date, &end_date))
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
    area_access: State<'_, AreaAccessSession>,
    start_date: String,
    end_date: String,
) -> Result<FullReport, String> {
    require_module_area(&db, &[module_key::REPORTS], &session, &area_access)?;
    let platform = Platform::current();
    query(&db, |conn| full_report::get_full_report(conn, &start_date, &end_date, platform))
}

/// Prints the same consolidated Full Report on a USB thermal printer.
#[tauri::command]
pub fn reports_print_full_report(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    start_date: String,
    end_date: String,
) -> Result<(), String> {
    let platform = Platform::current();
    print_area_report(
        &db,
        &session,
        &area_access,
        |conn| full_report::get_full_report(conn, &start_date, &end_date, platform),
        crate::printer::escpos::print_full_report,
    )
}

// ---------------------------------------------------------------------------
// Tables (restaurant floor view — only called when the `tables` module is
// enabled; the frontend hides all table UI otherwise)
// ---------------------------------------------------------------------------

/// Every table with its status and whether it has a cart parked on it, for
/// the floor view and the billing screen's table picker.
#[tauri::command]
pub fn tables_get_tables(session: State<'_, Session>, db: State<'_, Db>) -> Result<Vec<TableSummary>, String> {
    require_module(&db, &[module_key::TABLES])?;
    require_signed_in(&session)?;
    query(&db, tables::list_tables)
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
    require_module_admin(&db, &[module_key::TABLES], &session)?;
    query(&db, |conn| tables::add_table(conn, &name, seats))
}

/// Directly sets a table's status (e.g. marking one reserved for a booking).
#[tauri::command]
pub fn tables_update_table_status(
    db: State<'_, Db>,
    session: State<'_, Session>,
    table_id: i64,
    status: String,
) -> Result<TableSummary, String> {
    require_module_admin(&db, &[module_key::TABLES], &session)?;
    query(&db, |conn| tables::update_table_status(conn, table_id, &status))
}

/// Updates a table name and seat count.
#[tauri::command]
pub fn tables_update_table(
    db: State<'_, Db>,
    session: State<'_, Session>,
    table_id: i64,
    name: String,
    seats: i64,
) -> Result<TableSummary, String> {
    require_module_admin(&db, &[module_key::TABLES], &session)?;
    query(&db, |conn| tables::update_table(conn, table_id, &name, seats))
}

/// Removes a physical table from the floor entirely. Owner/Admin only, same
/// tier as `tables_add_table`. Refused (see `TableError::HasOpenOrder`)
/// while the table has an order parked on it, so an in-progress cart can
/// never be silently discarded from this command.
#[tauri::command]
pub fn tables_delete_table(db: State<'_, Db>, session: State<'_, Session>, table_id: i64) -> Result<(), String> {
    require_module_admin(&db, &[module_key::TABLES], &session)?;
    query(&db, |conn| tables::delete_table(conn, table_id))
}

/// Seats a table: starts an empty draft order (if none is already open) and
/// marks it occupied, so the billing screen has an order to attach a cart to
/// as items are added. Idempotent for a table that's already mid-order. Only
/// reachable from the floor view today, hence the same Owner/Admin gate.
///
/// `start_table_order` is two writes (the order upsert, then the table's
/// status flip) — run inside a transaction so a crash between them can never
/// leave a table marked occupied with no order behind it, or vice versa
/// (Phase 13 transaction audit; see `docs/TESTING_CHECKLIST.md`).
#[tauri::command]
pub fn tables_assign_order_to_table(
    db: State<'_, Db>,
    session: State<'_, Session>,
    table_id: i64,
) -> Result<TableSummary, String> {
    require_module_admin(&db, &[module_key::TABLES], &session)?;
    transact(&db, |tx| tables::start_table_order(tx, table_id))
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
    require_module_admin(&db, &[module_key::TABLES], &session)?;
    transact(&db, |tx| tables::clear_table(tx, table_id))
}

/// Parks the current cart on a table ("Save to table") instead of completing
/// the sale immediately.
///
/// Two writes (park the cart, mark the table occupied) — transactional for
/// the same reason as the two commands above; this one in particular is a
/// Cashier-reachable path (via Billing), so it's exercised far more often.
#[tauri::command]
pub fn tables_attach_cart_to_table(
    session: State<'_, Session>,
    db: State<'_, Db>,
    table_id: i64,
    items: Vec<ParkedCartLine>,
    discount_minor: i64,
) -> Result<(), String> {
    require_module(&db, &[module_key::TABLES])?;
    require_signed_in(&session)?;
    transact(&db, |tx| tables::attach_cart_to_table(tx, table_id, &items, discount_minor))
}

/// The cart parked on a table, if any — used to resume billing it.
#[tauri::command]
pub fn tables_get_parked_cart(session: State<'_, Session>, db: State<'_, Db>, table_id: i64) -> Result<Option<ParkedOrder>, String> {
    require_module(&db, &[module_key::TABLES])?;
    require_signed_in(&session)?;
    query(&db, |conn| tables::get_parked_cart(conn, table_id))
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
    require_module_admin(&db, &[module_key::TABLES], &session)?;
    transact(&db, |tx| tables::shift_table_order(tx, from_table_id, to_table_id))
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
pub fn counters_get_counters(session: State<'_, Session>, db: State<'_, Db>, include_inactive: bool) -> Result<Vec<Counter>, String> {
    require_signed_in(&session)?;
    query(&db, |conn| counters::list_counters(conn, include_inactive))
}

#[tauri::command]
pub fn counters_add_counter(db: State<'_, Db>, session: State<'_, Session>, name: String) -> Result<Counter, String> {
    require_module_admin(&db, &[module_key::SETTINGS], &session)?;
    query(&db, |conn| counters::add_counter(conn, &name))
}

#[tauri::command]
pub fn counters_update_counter(
    db: State<'_, Db>,
    session: State<'_, Session>,
    id: i64,
    name: String,
) -> Result<Counter, String> {
    require_module_admin(&db, &[module_key::SETTINGS], &session)?;
    query(&db, |conn| counters::update_counter(conn, id, &name))
}

#[tauri::command]
pub fn counters_set_active(
    db: State<'_, Db>,
    session: State<'_, Session>,
    id: i64,
    is_active: bool,
) -> Result<Counter, String> {
    require_module_admin(&db, &[module_key::SETTINGS], &session)?;
    query(&db, |conn| counters::set_counter_active(conn, id, is_active))
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
pub fn tokens_get_pending_items(session: State<'_, Session>, db: State<'_, Db>, table_order_id: i64) -> Result<Vec<PendingCounterGroup>, String> {
    require_signed_in(&session)?;
    query(&db, |conn| tokens::get_pending_token_items(conn, table_order_id))
}

/// Every token ever printed for this table order, newest first — "tokens
/// for this table", with a reprint option per row.
#[tauri::command]
pub fn tokens_get_for_order(session: State<'_, Session>, db: State<'_, Db>, table_order_id: i64) -> Result<Vec<TokenSummary>, String> {
    require_signed_in(&session)?;
    query(&db, |conn| tokens::list_tokens_for_order(conn, table_order_id))
}

/// What a kitchen token is printed for: a parked dine-in table order
/// (printed as a delta against what that order already sent) or an ad hoc
/// Takeaway cart with no order row behind it (always the full quantity —
/// see `tokens_print_adhoc`).
enum TokenSource<'a> {
    TableOrder(i64),
    Adhoc(&'a [tokens::AdhocTokenLine]),
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
    source: &TokenSource<'_>,
    counter: &Counter,
    printed_by: Option<i64>,
    printed_by_name: Option<String>,
    shift_id: Option<i64>,
    config: &AppConfig,
) -> PrintOutcome {
    let result = db.with_transaction(|tx| {
        let pending = match source {
            TokenSource::TableOrder(table_order_id) => {
                tokens::pending_items_for_counter(tx, *table_order_id, counter.id)
            }
            TokenSource::Adhoc(items) => tokens::ad_hoc_pending_for_counter(tx, items, counter.id),
        };
        let pending = match pending {
            Ok(p) => p,
            Err(tokens::TokenError::Sqlite(e)) => return Err(e),
            Err(other) => return Ok(Err(other.to_string())),
        };
        if pending.is_empty() {
            return Ok(Ok(None));
        }

        let (table_order_id, table_id, table_name) = match source {
            TokenSource::TableOrder(table_order_id) => match tables::get_open_order_by_id(tx, *table_order_id) {
                Ok(Some(ctx)) => (Some(*table_order_id), Some(ctx.table_id), Some(ctx.table_name)),
                Ok(None) => return Ok(Err("That table order is no longer open".to_string())),
                Err(tables::TableError::Sqlite(e)) => return Err(e),
                Err(other) => return Ok(Err(other.to_string())),
            },
            TokenSource::Adhoc(_) => (None, None, None),
        };

        let token_number = tokens::next_token_number_for_shift_or_today(tx, shift_id)?;

        // A draft, not-yet-persisted `TokenSummary` — everything the
        // ticket itself needs to print, built *before* any row exists.
        // `id`/`table_order_id` are never printed, so the placeholder `id`
        // is harmless.
        let draft = TokenSummary {
            id: 0,
            token_number,
            counter_id: counter.id,
            counter_name: counter.name.clone(),
            table_order_id,
            table_id,
            table_name,
            printed_at: chrono::Local::now().format(TIMESTAMP_FORMAT).to_string(),
            printed_by,
            printed_by_name: printed_by_name.clone(),
            status: "printed".to_string(),
            shift_id,
            items: pending.clone(),
            till_label: crate::workspace::local::this_till(tx)?.map(|(label, _)| label),
        };

        // The critical ordering: if this fails, we return `Ok(Err(..))`
        // with *no* `tx.execute` having run yet — an empty transaction
        // commits harmlessly, and the items stay pending for a retry.
        if let Err(print_err) = crate::printer::escpos::print_token(&draft, config, false) {
            return Ok(Err(print_err.to_string()));
        }

        let token = match tokens::insert_token(tx, table_order_id, counter.id, token_number, printed_by, shift_id, &pending) {
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

/// The shared body of `tokens_print` and `tokens_print_adhoc`: licence and
/// session checks, then one independent print per requested counter.
fn print_tokens_for_counters(
    db: &Db,
    session: &Session,
    license: &LicenseRuntime,
    source: TokenSource<'_>,
    counter_ids: Vec<i64>,
) -> Result<Vec<CounterPrintResult>, String> {
    require_license(license)?;
    let caller = require_signed_in(session)?;
    let printed_by = Some(caller.id);
    let printed_by_name = Some(caller.name);
    let config = query(&db, config::get)?;
    let all_counters = query(&db, |conn| counters::list_counters(conn, true))?;

    let shifts_enabled = module_enabled(db, module_key::SHIFTS)?;
    let shift_id = if shifts_enabled {
        query(&db, |conn| shifts::get_effective_open_shift_id(conn, Some(caller.id)))?
    } else {
        None
    };

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

        let outcome = print_one_counter_token(db, &source, counter, printed_by, printed_by_name.clone(), shift_id, &config);
        results.push(CounterPrintResult { counter_id: counter.id, counter_name: counter.name.clone(), outcome });
    }

    Ok(results)
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
    license: State<'_, LicenseRuntime>,
    table_order_id: i64,
    counter_ids: Vec<i64>,
) -> Result<Vec<CounterPrintResult>, String> {
    print_tokens_for_counters(&db, &session, &license, TokenSource::TableOrder(table_order_id), counter_ids)
}

/// Reprints an existing token exactly as originally recorded — no new
/// token number, no change to what's tokenized, clearly marked "REPRINT"
/// on the output (see `printer::escpos::build_token_bytes`) so the counter
/// never mistakes it for a second, separate order. Writes nothing to the
/// database; a failed reprint just means "try again", same as any other
/// print.
#[tauri::command]
pub fn tokens_reprint(
    db: State<'_, Db>,
    session: State<'_, Session>,
    license: State<'_, LicenseRuntime>,
    token_id: i64,
) -> Result<(), String> {
    // Licensed like every other print path (`print_tokens_for_counters`).
    require_license(&license)?;
    require_signed_in(&session)?;
    let token = query(&db, |conn| tokens::get_token(conn, token_id))?;
    let config = query(&db, config::get)?;
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
    session: State<'_, Session>,
    items: Vec<tokens::AdhocTokenLine>,
) -> Result<Vec<PendingCounterGroup>, String> {
    require_signed_in(&session)?;
    query(&db, |conn| tokens::ad_hoc_token_groups(conn, &items))
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
    license: State<'_, LicenseRuntime>,
    items: Vec<tokens::AdhocTokenLine>,
    counter_ids: Vec<i64>,
) -> Result<Vec<CounterPrintResult>, String> {
    print_tokens_for_counters(&db, &session, &license, TokenSource::Adhoc(&items), counter_ids)
}

// ---------------------------------------------------------------------------
// Orders (Active restaurant orders lifecycle: takeaway and dine-in)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAndHoldOrderInput {
    pub client_ref: String,
    pub order_type: String,
    pub table_id: Option<i64>,
    pub customer_name: Option<String>,
    pub notes: Option<String>,
    pub estimated_prep_minutes: Option<i64>,
    pub delivery: Option<orders::DeliveryDetails>,
    pub lines: Vec<orders::NewLine>,
}

#[tauri::command]
pub fn orders_create_and_hold(
    db: State<'_, Db>,
    session: State<'_, Session>,
    input: CreateAndHoldOrderInput,
) -> Result<orders::Order, String> {
    let user = require_role(&session, &[Role::Owner, Role::Admin, Role::Cashier])?;
    let shifts_enabled = module_enabled(&db, module_key::SHIFTS)?;
    let shift_id = if shifts_enabled {
        query(&db, |conn| shifts::get_effective_open_shift_id(conn, Some(user.id)))?
    } else {
        None
    };
    transact(&db, |tx| {
        let order = orders::create_order(
            tx,
            &input.client_ref,
            &input.order_type,
            input.table_id,
            Some(user.id),
            input.customer_name.as_deref(),
            input.notes.as_deref(),
            input.estimated_prep_minutes,
            shift_id,
            input.delivery,
        )?;
        let order = orders::add_lines(tx, order.id, order.version, &input.lines, Some(user.id))?;
        orders::hold(tx, order.id, order.version, Some(user.id))
    })
}

#[tauri::command]
pub fn orders_list_active(
    db: State<'_, Db>,
    session: State<'_, Session>,
) -> Result<Vec<orders::Order>, String> {
    require_role(&session, &[Role::Owner, Role::Admin, Role::Cashier])?;
    query(&db, |conn| orders::list_active_orders(conn))
}

#[tauri::command]
pub fn orders_get(
    db: State<'_, Db>,
    session: State<'_, Session>,
    order_id: i64,
) -> Result<orders::Order, String> {
    require_role(&session, &[Role::Owner, Role::Admin, Role::Cashier])?;
    query(&db, |conn| orders::get_order(conn, order_id))
}

#[tauri::command]
pub fn orders_resume(
    db: State<'_, Db>,
    session: State<'_, Session>,
    order_id: i64,
    version: i64,
) -> Result<orders::Order, String> {
    let user = require_role(&session, &[Role::Owner, Role::Admin, Role::Cashier])?;
    transact(&db, |tx| orders::resume(tx, order_id, version, Some(user.id)))
}

#[tauri::command]
pub fn orders_transition(
    db: State<'_, Db>,
    session: State<'_, Session>,
    order_id: i64,
    version: i64,
    to_status: String,
) -> Result<orders::Order, String> {
    let user = require_role(&session, &[Role::Owner, Role::Admin, Role::Cashier])?;
    transact(&db, |tx| orders::transition(tx, order_id, version, &to_status, Some(user.id)))
}

#[tauri::command]
pub fn orders_update_prep_time(
    db: State<'_, Db>,
    session: State<'_, Session>,
    order_id: i64,
    prep_minutes: i64,
) -> Result<orders::Order, String> {
    require_role(&session, &[Role::Owner, Role::Admin, Role::Cashier])?;
    transact(&db, |tx| orders::update_prep_time(tx, order_id, prep_minutes))
}

#[tauri::command]
pub fn orders_cancel(
    db: State<'_, Db>,
    session: State<'_, Session>,
    order_id: i64,
    version: i64,
    reason: Option<String>,
) -> Result<orders::Order, String> {
    let user = require_role(&session, &[Role::Owner, Role::Admin, Role::Cashier])?;
    transact(&db, |tx| {
        orders::cancel(
            tx,
            order_id,
            version,
            reason.as_deref().unwrap_or("Cancelled by cashier"),
            Some(user.id),
            if user.role == Role::Owner || user.role == Role::Admin { Some(user.id) } else { None },
        )
    })
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrdersPayInput {
    pub order_id: i64,
    pub version: i64,
    pub order_item_ids: Vec<i64>,
    pub payment_method: String,
    pub discount_minor: i64,
    pub tax_minor: i64,
    #[serde(default)]
    pub tendered_minor: Option<i64>,
    #[serde(default)]
    pub change_minor: Option<i64>,
    #[serde(default)]
    pub payments: Vec<sales::PaymentPart>,
    #[serde(default)]
    pub customer_id: Option<i64>,
    #[serde(default)]
    pub service_charge_minor: i64,
    #[serde(default)]
    pub due_date: Option<String>,
}

#[tauri::command]
pub async fn orders_pay(
    app: AppHandle,
    db: State<'_, Db>,
    session: State<'_, Session>,
    license: State<'_, LicenseRuntime>,
    input: OrdersPayInput,
) -> Result<orders::Paid, String> {
    let user = require_role(&session, &[Role::Owner, Role::Admin, Role::Cashier])?;
    require_license(&license)?;
    let shifts_enabled = module_enabled(&db, module_key::SHIFTS)?;
    let shift_id = if shifts_enabled {
        query(&db, |conn| shifts::get_effective_open_shift_id(conn, Some(user.id)))?
    } else {
        None
    };
    // The lines being paid take their stock centrally first, like a counter sale.
    let lines = query(&db, |conn| orders::stock_lines(conn, input.order_id, &input.order_item_ids))?;
    let reservation = workspace::runtime::reserve_for_sale(&db, &lines).await?;
    let paid = transact(&db, |tx| workspace::runtime::record_sale(tx, reservation.as_deref(), |tx| {
        orders::pay_lines(
            tx,
            input.order_id,
            input.version,
            orders::PayInput {
                order_item_ids: input.order_item_ids,
                payment_method: input.payment_method,
                discount_minor: input.discount_minor,
                tax_minor: input.tax_minor,
                cashier_id: Some(user.id),
                shift_id,
                tendered_minor: input.tendered_minor,
                change_minor: input.change_minor,
                payments: input.payments,
                customer_id: input.customer_id,
                service_charge_minor: input.service_charge_minor,
                due_date: input.due_date,
            },
        )
    }));
    if let (Err(_), Some(key)) = (&paid, &reservation) {
        workspace::runtime::abandon_sale(&db, key);
    }
    let paid = paid?;
    crate::sync::spawn_after_write(&app);
    Ok(paid)
}

#[tauri::command]
pub fn orders_print_token(
    db: State<'_, Db>,
    session: State<'_, Session>,
    license: State<'_, LicenseRuntime>,
    order_id: i64,
    is_kitchen: Option<bool>,
) -> Result<(), String> {
    require_license(&license)?;
    require_signed_in(&session)?;
    let order = query(&db, |conn| orders::get_order(conn, order_id))?;
    let config = query(&db, config::get)?;
    crate::printer::escpos::print_order_token(&order, &config, is_kitchen.unwrap_or(false))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn orders_update_delivery_status(
    db: State<'_, Db>,
    session: State<'_, Session>,
    order_id: i64,
    delivery_status: String,
) -> Result<orders::Order, String> {
    let user = require_role(&session, &[Role::Owner, Role::Admin, Role::Cashier, Role::Delivery])?;
    transact(&db, |tx| {
        orders::update_delivery_status(tx, order_id, &delivery_status, Some(user.id))
    })
}

// ---------------------------------------------------------------------------
// Delivery Platforms (Foodpanda, etc. and recent rider suggestions)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn delivery_platforms_list(
    db: State<'_, Db>,
    session: State<'_, Session>,
) -> Result<Vec<delivery_platforms::DeliveryPlatform>, String> {
    require_role(&session, ADMIN_ROLES)?;
    query(&db, |conn| delivery_platforms::list_platforms(conn))
}

#[tauri::command]
pub fn delivery_platforms_list_active(
    db: State<'_, Db>,
    session: State<'_, Session>,
) -> Result<Vec<delivery_platforms::DeliveryPlatform>, String> {
    require_signed_in(&session)?;
    query(&db, |conn| delivery_platforms::list_active_platforms(conn))
}

#[tauri::command]
pub fn delivery_platforms_add(
    db: State<'_, Db>,
    session: State<'_, Session>,
    name: String,
) -> Result<delivery_platforms::DeliveryPlatform, String> {
    require_role(&session, ADMIN_ROLES)?;
    query(&db, |conn| delivery_platforms::add_platform(conn, &name))
}

#[tauri::command]
pub fn delivery_platforms_update(
    db: State<'_, Db>,
    session: State<'_, Session>,
    id: i64,
    name: String,
) -> Result<delivery_platforms::DeliveryPlatform, String> {
    require_role(&session, ADMIN_ROLES)?;
    query(&db, |conn| delivery_platforms::update_platform(conn, id, &name))
}

#[tauri::command]
pub fn delivery_platforms_set_active(
    db: State<'_, Db>,
    session: State<'_, Session>,
    id: i64,
    is_active: bool,
) -> Result<(), String> {
    require_role(&session, ADMIN_ROLES)?;
    query(&db, |conn| delivery_platforms::set_platform_active(conn, id, is_active))
}

#[tauri::command]
pub fn delivery_platforms_get_recent_riders(
    db: State<'_, Db>,
    session: State<'_, Session>,
    platform_id: i64,
    limit: Option<i64>,
) -> Result<Vec<delivery_platforms::RecentRider>, String> {
    require_signed_in(&session)?;
    query(&db, |conn| {
        delivery_platforms::get_recent_riders_for_platform(
            conn,
            platform_id,
            limit.unwrap_or(10).try_into().unwrap_or(10),
        )
    })
}


// ---------------------------------------------------------------------------
// Sensitive area PIN — the shared, role-independent PIN gate in front of
// Attendance, Expenses, Salary, Employees and Reports (see
// `db::security_pin`'s module doc comment for the full picture). Every
// command in the four sections below this one calls `require_area_access`
// instead of `require_role(&session, ADMIN_ROLES)`.
// ---------------------------------------------------------------------------

/// Verifies `pin` against the shared area PIN and, on success, grants
/// `AreaAccessSession` — the frontend's `AreaPinGate` calls this once per
/// visit to one of the five gated screens, before rendering their content.
/// Any signed-in role may call this (the PIN itself is the authorization);
/// an incorrect PIN never grants anything, it just returns an error.
#[tauri::command]
pub fn security_verify_area_pin(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    limiter: State<'_, AttemptLimiter>,
    pin: String,
) -> Result<(), String> {
    require_signed_in(&session)?;
    limiter.attempt("area_pin", || {
        db.with_conn(move |conn| Ok(security_pin::verify(conn, &pin)))
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())
    })?;
    area_access.grant();
    Ok(())
}

/// Ends the current `AreaAccessSession` grant immediately, without waiting
/// for its safety-net timeout — the frontend's `AreaPinGate` calls this on
/// unmount (leaving a gated screen), which is what actually makes "every
/// single visit needs its own PIN" true in practice rather than just "until
/// the timeout happens to lapse".
#[tauri::command]
pub fn security_revoke_area_access(area_access: State<'_, AreaAccessSession>) {
    area_access.revoke();
}

/// Whether the Owner's login PIN and the area PIN are still the values every
/// install ships with — Owner/Admin only, since it says something about
/// how guessable this till's credentials are. The app shows a persistent
/// warning to Owner/Admin until both are changed.
#[tauri::command]
pub fn security_get_default_credentials(
    db: State<'_, Db>,
    session: State<'_, Session>,
) -> Result<security_pin::DefaultCredentials, String> {
    require_role(&session, ADMIN_ROLES)?;
    query(&db, security_pin::default_credentials)
}

/// The forced change of the shipped PINs (`ChangeDefaultPinsDialog`). Only
/// replaces a PIN still at its default, so it needs no current PIN; the
/// Owner PIN only by the Owner, who is the one person outranking it.
/// Deliberately not behind the Users/Settings modules — a vendor may switch
/// those off, and the defaults must still be changeable.
#[tauri::command]
pub fn security_replace_default_pins(
    db: State<'_, Db>,
    session: State<'_, Session>,
    owner_pin: Option<String>,
    area_pin: Option<String>,
) -> Result<(), String> {
    let caller = require_role(&session, ADMIN_ROLES)?;
    if owner_pin.is_some() && caller.role != Role::Owner {
        return Err("Only the Owner can change the Owner PIN".into());
    }
    transact(&db, |tx| security_pin::replace_defaults(tx, owner_pin.as_deref(), area_pin.as_deref()))
}

/// Changes the shared area PIN — Owner/Admin only (Settings itself is
/// admin-only; this mirrors that, it doesn't loosen it). Requires the
/// current PIN; "confirm new PIN" is a frontend-only check (the two typed
/// values must match before this is even called), same as any other
/// password-confirmation field.
#[tauri::command]
pub fn security_set_area_pin(
    db: State<'_, Db>,
    session: State<'_, Session>,
    old_pin: String,
    new_pin: String,
) -> Result<(), String> {
    require_role(&session, ADMIN_ROLES)?;
    db.with_conn(move |conn| Ok(security_pin::set(conn, &old_pin, &new_pin)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Attendance (only called when the `attendance` module is enabled; the
// frontend hides all attendance UI otherwise). Gated by the shared area PIN
// (see the section above), not by role — see `db::security_pin`'s module
// doc comment for why Owner, Admin and Cashier all go through the same
// `require_area_access` check on every command here.
// ---------------------------------------------------------------------------

/// Active staff, for the check-in/out screen and the monthly summary.
#[tauri::command]
pub fn attendance_get_employees(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
) -> Result<Vec<Employee>, String> {
    require_module_area(&db, &[module_key::ATTENDANCE, module_key::SALARY], &session, &area_access)?;
    query(&db, attendance::list_employees)
}

/// Every employee, active or not — the employee management screen.
#[tauri::command]
pub fn attendance_get_all_employees(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
) -> Result<Vec<ManagedEmployee>, String> {
    require_module_area(&db, &[module_key::EMPLOYEES], &session, &area_access)?;
    query(&db, attendance::list_all_employees)
}

/// Adds a new employee (payroll/attendance record, not a login account —
/// see `db::attendance`'s module doc comment). Shows up in the check-in
/// screen and the monthly summary immediately, no restart needed, since both
/// re-query `employees` on every load.
#[tauri::command]
pub fn attendance_add_employee(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    name: String,
    role: String,
    contact: Option<String>,
    base_salary_minor: i64,
) -> Result<ManagedEmployee, String> {
    require_module_area(&db, &[module_key::EMPLOYEES], &session, &area_access)?;
    db.with_conn(move |conn| Ok(attendance::add_employee(conn, &name, &role, contact.as_deref(), base_salary_minor)))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Renames/re-roles/updates the contact and base salary of an employee.
#[tauri::command]
pub fn attendance_update_employee(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    employee_id: i64,
    name: String,
    role: String,
    contact: Option<String>,
    base_salary_minor: i64,
) -> Result<ManagedEmployee, String> {
    require_module_area(&db, &[module_key::EMPLOYEES], &session, &area_access)?;
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
    area_access: State<'_, AreaAccessSession>,
    employee_id: i64,
    is_active: bool,
) -> Result<(), String> {
    require_module_area(&db, &[module_key::EMPLOYEES], &session, &area_access)?;
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
    area_access: State<'_, AreaAccessSession>,
    employee_id: i64,
) -> Result<AttendanceRecord, String> {
    require_module_area(&db, &[module_key::ATTENDANCE], &session, &area_access)?;
    query(&db, |conn| attendance::check_in(conn, employee_id))
}

/// Checks an employee out for today. Fails clearly (rather than fabricating a
/// row) if they never checked in today.
#[tauri::command]
pub fn attendance_check_out(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    employee_id: i64,
) -> Result<AttendanceRecord, String> {
    require_module_area(&db, &[module_key::ATTENDANCE], &session, &area_access)?;
    query(&db, |conn| attendance::check_out(conn, employee_id))
}

/// The attendance log for a date range, optionally scoped to one employee —
/// omit `employeeId` for every employee's shifts in the range.
#[tauri::command]
pub fn attendance_get_attendance(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    employee_id: Option<i64>,
    start_date: String,
    end_date: String,
) -> Result<Vec<AttendanceRecord>, String> {
    require_module_area(&db, &[module_key::ATTENDANCE], &session, &area_access)?;
    query(&db, |conn| attendance::get_attendance(conn, employee_id, &start_date, &end_date))
}

/// Days present/absent and total hours per employee for `month` (`YYYY-MM`) —
/// feeds Phase 9's salary calculation.
#[tauri::command]
pub fn attendance_get_monthly_summary(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    month: String,
) -> Result<Vec<MonthlySummary>, String> {
    require_module_area(&db, &[module_key::ATTENDANCE], &session, &area_access)?;
    query(&db, |conn| attendance::get_monthly_summary(conn, &month))
}

// ---------------------------------------------------------------------------
// Expenses (only called when the `expenses` module is enabled; the frontend
// hides all expense UI otherwise). Gated by the shared area PIN, same as
// Attendance/Salary/Reports above/below — not by role.
// ---------------------------------------------------------------------------

/// Logs one expense.
#[tauri::command]
pub fn expenses_add_expense(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    date: String,
    category: String,
    amount_minor: i64,
    note: Option<String>,
) -> Result<Expense, String> {
    require_module_area(&db, &[module_key::EXPENSES], &session, &area_access)?;
    query(&db, |conn| expenses::add_expense(conn, &date, &category, amount_minor, note.as_deref()))
}

/// Expenses in a date range (inclusive), optionally scoped to one category.
#[tauri::command]
pub fn expenses_get_expenses(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    start_date: String,
    end_date: String,
    category: Option<String>,
) -> Result<Vec<Expense>, String> {
    require_module_area(&db, &[module_key::EXPENSES], &session, &area_access)?;
    query(&db, |conn| expenses::get_expenses(conn, &start_date, &end_date, category.as_deref()))
}

/// Every distinct category already in use, for the quick-add form's dropdown.
#[tauri::command]
pub fn expenses_get_expense_categories(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
) -> Result<Vec<String>, String> {
    require_module_area(&db, &[module_key::EXPENSES], &session, &area_access)?;
    query(&db, expenses::get_categories)
}

/// Category-wise totals for a date range, highest spend first — feeds the
/// breakdown view and, later, the dashboard's profit calculation.
#[tauri::command]
pub fn expenses_get_totals_by_category(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    start_date: String,
    end_date: String,
) -> Result<Vec<CategoryTotal>, String> {
    require_module_area(&db, &[module_key::EXPENSES], &session, &area_access)?;
    query(&db, |conn| expenses::get_totals_by_category(conn, &start_date, &end_date))
}

// ---------------------------------------------------------------------------
// Salary (only called when the `salary` module is enabled; the frontend
// hides all salary UI otherwise). The most sensitive data in the product —
// gated by the shared area PIN, same as Attendance/Expenses/Reports, not by
// role. See `db::security_pin`'s module doc comment.
// ---------------------------------------------------------------------------

/// `base_salary / days_in_month * days_present` for one employee and month
/// (`days_in_month` is the real calendar day count, 28–31), refreshed
/// against the latest attendance every call.
#[tauri::command]
pub fn salary_calculate_salary(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    employee_id: i64,
    month: String,
) -> Result<SalaryCalculation, String> {
    require_module_area(&db, &[module_key::SALARY], &session, &area_access)?;
    query(&db, |conn| salary::calculate_salary(conn, employee_id, &month))
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
    area_access: State<'_, AreaAccessSession>,
    month: String,
) -> Result<Vec<SalaryCalculation>, String> {
    require_module_area(&db, &[module_key::SALARY], &session, &area_access)?;
    transact(&db, |tx| salary::get_monthly_overview(tx, &month))
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
    area_access: State<'_, AreaAccessSession>,
    employee_id: i64,
    month: String,
    paid_amount_minor: i64,
    paid_date: String,
) -> Result<SalaryCalculation, String> {
    require_module_area(&db, &[module_key::SALARY], &session, &area_access)?;
    transact(&db, |tx| salary::record_payment(tx, employee_id, &month, paid_amount_minor, &paid_date))
}

/// Prints the Employee Report (every active employee's attendance and
/// salary for `month`, same rows `salary_get_monthly_overview` returns) on
/// a USB thermal printer.
#[tauri::command]
pub fn salary_print_monthly_report(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    month: String,
) -> Result<(), String> {
    require_module_area(&db, &[module_key::SALARY], &session, &area_access)?;
    let rows = transact(&db, |tx| salary::get_monthly_overview(tx, &month))?;
    let app_config = query(&db, config::get)?;
    crate::printer::escpos::print_employee_report(&rows, &month, &app_config).map_err(|e| e.to_string())
}

/// Every month with a salary record for one employee, most recent first.
#[tauri::command]
pub fn salary_get_payment_history(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    employee_id: i64,
) -> Result<Vec<SalaryCalculation>, String> {
    require_module_area(&db, &[module_key::SALARY], &session, &area_access)?;
    query(&db, |conn| salary::get_payment_history(conn, employee_id))
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
    require_role(&session, ADMIN_ROLES)?;
    let platform = parse_platform(&platform)?;
    query(&db, |conn| dashboard::get_summary(conn, &start_date, &end_date, platform))
}

// ---------------------------------------------------------------------------
// Product owner (vendor) — a hidden, separate account for the product's
// developer/vendor, not for any client's staff. Never appears in Manage
// Users or any client-facing query; gated by `ProductOwnerSession`, never
// `session::Session`, so reaching it neither requires nor disturbs whatever
// staff account is currently signed in. See `db::product_owner`'s module
// doc for the full rationale, and docs/SUPPORT.md for the (deliberately
// non-in-app) credential-recovery process.
// ---------------------------------------------------------------------------

/// Whether a credential has already been set on this install — the hidden
/// entry point's UI uses this to decide whether to show a setup form or a
/// login form. Unauthenticated on purpose: there's nothing to gate yet on
/// the "no account" branch, and the answer alone reveals nothing about the
/// credential itself.
#[tauri::command]
pub fn product_owner_get_status(db: State<'_, Db>) -> Result<bool, String> {
    query(&db, product_owner::has_account)
}

/// Sets the initial credential for this install and immediately grants an
/// elevated session (so setting it and using it is one step, not two).
/// Refuses if a credential already exists — see
/// `product_owner::ProductOwnerError::AlreadyConfigured`.
///
/// Requires the vendor password (checked by `VendorGate::verify`, with its
/// retry backoff): until a credential exists, "first caller wins", so
/// without this anyone with devtools on an install the vendor hadn't yet
/// configured could claim the elevated account.
#[tauri::command]
pub fn product_owner_setup(
    db: State<'_, Db>,
    po_session: State<'_, ProductOwnerSession>,
    vendor_gate: State<'_, VendorGate>,
    vendor_password: String,
    password: String,
) -> Result<(), String> {
    vendor_gate.verify(&vendor_password).map_err(|e| e.to_string())?;
    query(&db, |conn| product_owner::setup(conn, &password))?;
    po_session.grant();
    Ok(())
}

/// Verifies `password` and, on success, grants an elevated session — see
/// `ProductOwnerSession` for its (short, idle-based) lifetime.
#[tauri::command]
pub fn product_owner_login(
    db: State<'_, Db>,
    po_session: State<'_, ProductOwnerSession>,
    limiter: State<'_, AttemptLimiter>,
    password: String,
) -> Result<(), String> {
    limiter.attempt("product_owner", || {
        query(&db, |conn| product_owner::verify(conn, &password))
    })?;
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
    let mut list = query(&db, |conn| modules::list(conn, platform))?;
    for module in &mut list {
        module.entitled = entitlements::allows(&module.key);
        module.required_plan = entitlements::upgrade_for(&module.key);
    }
    Ok(list)
}

/// Sets a module's `enabled` and/or `locked` state on one platform,
/// independently (`None` leaves that half unchanged) — see
/// `db::modules::set_by_product_owner`. This is the higher-privilege path:
/// it bypasses the client-facing `config_toggle_module`'s lock check entirely
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
    query(&db, |conn| modules::set_by_product_owner(conn, &module_key, platform, enabled, locked))?;
    query(&db, |conn| modules::list(conn, platform))
}

// ---------------------------------------------------------------------------
// Owner/Admin/Cashier account hierarchy — the `authorize_*` functions above
// are the actual gate (`users_create`/`users_update`/`users_set_active`/
// `users_set_pin` are thin wrappers around them), so exercising them directly
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

    // -- users_create -------------------------------------------------------

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

    // -- users_update ---------------------------------------------------------

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

    // -- users_set_active -----------------------------------------------------

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

    // -- a Cashier caller -------------------------------------------------------
    // Unreachable through the commands today (each calls `require_role(&session,
    // ADMIN_ROLES)` first); these pin down the second layer on its own.

    #[test]
    fn a_cashier_can_create_no_account() {
        for role in [Role::Owner, Role::Admin, Role::Cashier] {
            assert!(authorize_create_user(Role::Cashier, role).is_err(), "cashier created {role:?}");
        }
    }

    #[test]
    fn a_cashier_cannot_promote_themselves_but_may_rename_themselves() {
        let cashier = user(5, Role::Cashier);
        assert!(authorize_update_user(&cashier, cashier.id, Role::Cashier, Role::Admin).is_err());
        assert!(authorize_update_user(&cashier, cashier.id, Role::Cashier, Role::Owner).is_err());
        assert!(authorize_update_user(&cashier, cashier.id, Role::Cashier, Role::Cashier).is_ok());
    }

    #[test]
    fn a_cashier_cannot_manage_another_cashier() {
        let cashier = user(5, Role::Cashier);
        assert!(authorize_update_user(&cashier, 6, Role::Cashier, Role::Cashier).is_err());
        assert!(authorize_set_active(&cashier, 6, Role::Cashier, false).is_err());
        assert!(authorize_set_pin(&cashier, 6, Role::Cashier).is_err());
    }

    // -- users_set_pin ----------------------------------------------------------

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

    #[test]
    fn delivery_role_is_assignable_and_manageable_but_manages_nobody() {
        assert!(authorize_create_user(Role::Owner, Role::Delivery).is_ok());
        assert!(authorize_create_user(Role::Admin, Role::Delivery).is_ok());
        for role in [Role::Owner, Role::Admin, Role::Cashier, Role::Delivery] {
            assert!(authorize_create_user(Role::Delivery, role).is_err(), "delivery created {role:?}");
        }
        let admin = user(2, Role::Admin);
        assert!(authorize_update_user(&admin, 6, Role::Delivery, Role::Cashier).is_ok());
        assert!(authorize_set_active(&admin, 6, Role::Delivery, false).is_ok());
        let rider = user(6, Role::Delivery);
        assert!(authorize_update_user(&rider, rider.id, Role::Delivery, Role::Admin).is_err());
        assert!(authorize_set_active(&rider, 5, Role::Cashier, false).is_err());
    }
}

#[cfg(test)]
mod guard_tests {
    use super::*;
    use crate::license::state::ClockProblem;

    fn signed_in(role: Role) -> Session {
        let session = Session::new();
        session.set(User { id: 7, name: "Test".into(), role });
        session
    }

    fn granted() -> AreaAccessSession {
        let area = AreaAccessSession::new();
        area.grant();
        area
    }

    // -- module gating ------------------------------------------------------

    fn db_with(disabled: &[&str]) -> Db {
        let conn = crate::db::schema::test_conn();
        for key in disabled {
            modules::set_by_product_owner(&conn, key, Platform::current(), Some(false), None).unwrap();
        }
        Db::from_connection(conn)
    }

    #[test]
    fn require_module_refuses_a_disabled_module_and_accepts_any_one_enabled_key() {
        let db = db_with(&[module_key::PURCHASING]);
        assert!(require_module(&db, &[module_key::PURCHASING]).is_err());
        assert!(require_module(&db, &[module_key::PURCHASING, module_key::LEDGER]).is_ok());
        assert!(require_module(&db, &[module_key::CUSTOMERS]).is_ok());
    }

    #[test]
    fn require_module_admin_needs_both_the_module_and_an_admin_role() {
        let db = db_with(&[module_key::LEDGER]);
        assert!(require_module_admin(&db, &[module_key::PURCHASING], &signed_in(Role::Admin)).is_ok());
        assert!(require_module_admin(&db, &[module_key::PURCHASING], &signed_in(Role::Cashier)).is_err());
        assert!(require_module_admin(&db, &[module_key::PURCHASING], &Session::new()).is_err());
        assert!(require_module_admin(&db, &[module_key::LEDGER], &signed_in(Role::Owner)).is_err());
    }

    #[test]
    fn require_module_area_needs_the_module_and_a_live_grant() {
        let db = db_with(&[]);
        let cashier = signed_in(Role::Cashier);
        assert!(require_module_area(&db, &[module_key::REPORTS], &cashier, &granted()).is_ok());
        assert_eq!(
            require_module_area(&db, &[module_key::REPORTS], &cashier, &AreaAccessSession::new()).unwrap_err(),
            AREA_PIN_REQUIRED
        );
        let off = db_with(&[module_key::REPORTS]);
        assert!(require_module_area(&off, &[module_key::REPORTS], &cashier, &granted()).is_err());
    }

    // -- plan entitlements (license::entitlements) --------------------------

    /// Installs the BASIC plan's entitlements exactly as the server signs
    /// them: a payload verified with the test key, then adopted. Per-thread
    /// in tests, so it never leaks into another test.
    fn install_signed_basic_plan() {
        use crate::license::verify::{tests::{sample_json, sign_json, test_keyring}, verify_with_keys};
        let json = sample_json().replace(
            r#""max_offline_days":30}"#,
            r#""max_offline_days":30,"plan":{"code":"basic","name":"BASIC"},"entitlements":["billing","dashboard","expenses","inventory","reports","settings","shifts","users"],"upgrade":{"customers":"ALL-IN-ONE","inventory_advanced":"ADVANCE","multi_user":"ADVANCE","purchasing":"ADVANCE","reports_advanced":"ADVANCE"}}"#,
        );
        let (payload, signature) = sign_json(&json);
        let verified = verify_with_keys(&test_keyring(), &payload, &signature).unwrap();
        entitlements::install(Some(entitlements::Entitlements::from_payload(&verified)));
    }

    #[test]
    fn a_module_outside_the_plan_is_refused_even_when_switched_on() {
        install_signed_basic_plan();
        let db = db_with(&[]);
        let err = require_module(&db, &[module_key::PURCHASING]).unwrap_err();
        assert!(err.contains("BASIC") && err.contains("ADVANCE"), "{err}");
        assert!(require_module(&db, &[module_key::BILLING]).is_ok());
        assert!(require_module(&db, &[module_key::PURCHASING, module_key::INVENTORY]).is_ok(), "any one key is enough");
        assert!(require_module_admin(&db, &[module_key::CUSTOMERS], &signed_in(Role::Owner)).is_err());
        assert!(require_customers(&db, &signed_in(Role::Owner)).is_err(), "customers is ALL-IN-ONE");
        entitlements::install(None);
        assert!(require_module(&db, &[module_key::PURCHASING]).is_ok(), "a pre-plan licence keeps every module");
    }

    #[test]
    fn capabilities_inside_a_module_follow_the_plan() {
        install_signed_basic_plan();
        for capability in [
            entitlements::capability::INVENTORY_ADVANCED,
            entitlements::capability::REPORTS_ADVANCED,
            entitlements::capability::MULTI_USER,
            entitlements::capability::MOBILE,
        ] {
            assert!(require_capability(capability).is_err(), "{capability} is not in BASIC");
        }
        let db = db_with(&[]);
        assert!(require_module_area(&db, &[module_key::REPORTS], &signed_in(Role::Owner), &granted()).is_ok(), "basic reports stay open");
        entitlements::install(None);
        assert!(require_capability(entitlements::capability::MULTI_USER).is_ok());
    }

    #[test]
    fn require_customers_lets_a_cashier_in_only_while_the_module_is_on() {
        assert_eq!(require_customers(&db_with(&[]), &signed_in(Role::Cashier)).unwrap().role, Role::Cashier);
        assert!(require_customers(&db_with(&[]), &Session::new()).is_err());
        assert!(require_customers(&db_with(&[module_key::CUSTOMERS]), &signed_in(Role::Owner)).is_err());
    }

    // -- require_license ---------------------------------------------------

    #[test]
    fn an_ok_licence_allows_billing_including_when_expiring_soon_or_perpetual() {
        assert_eq!(license_block_reason(&LicenseState::Ok { expires_at: Some(1), expiring_soon: true }), None);
        assert_eq!(license_block_reason(&LicenseState::Ok { expires_at: None, expiring_soon: false }), None);
    }

    #[test]
    fn every_locked_licence_state_blocks_billing_with_its_own_message() {
        let locked = [
            (LicenseState::Missing, "no licence"),
            (LicenseState::Expired { expired_at: Some(1) }, "expired"),
            (LicenseState::Revoked, "revoked"),
            (LicenseState::DeviceMismatch, "different device"),
            (LicenseState::ClockTampered { reason: ClockProblem::BehindLastSeenTime }, "clock"),
            (LicenseState::CheckInRequired { last_online_check: 0, max_offline_days: 7 }, "offline too long"),
            (LicenseState::Invalid { message: "bad signature".into() }, "bad signature"),
        ];
        for (state, expected) in locked {
            let reason = license_block_reason(&state).unwrap_or_else(|| panic!("{state:?} must block billing"));
            assert!(reason.contains(expected), "{state:?} -> {reason:?} should mention {expected:?}");
        }
    }

    // -- require_area_access -----------------------------------------------

    #[test]
    fn area_access_needs_someone_signed_in_even_with_a_live_grant() {
        assert_eq!(require_area_access(&Session::new(), &granted()).unwrap_err(), crate::session::NOT_SIGNED_IN);
    }

    #[test]
    fn area_access_needs_a_grant_for_every_role_including_owner() {
        for role in [Role::Owner, Role::Admin, Role::Cashier] {
            assert!(require_area_access(&signed_in(role), &AreaAccessSession::new()).is_err(), "{role:?}");
            assert_eq!(require_area_access(&signed_in(role), &granted()).unwrap().role, role);
        }
    }

    #[test]
    fn a_revoked_grant_stops_area_access() {
        let area = granted();
        area.revoke();
        assert!(require_area_access(&signed_in(Role::Cashier), &area).is_err());
    }

    // -- require_admin_or_area_access --------------------------------------

    #[test]
    fn staff_or_area_access_lets_owner_and_admin_through_without_a_grant() {
        for role in [Role::Owner, Role::Admin] {
            assert!(require_admin_or_area_access(&signed_in(role), &AreaAccessSession::new()).is_ok(), "{role:?}");
        }
    }

    #[test]
    fn staff_or_area_access_needs_a_grant_for_a_cashier() {
        assert!(require_admin_or_area_access(&signed_in(Role::Cashier), &AreaAccessSession::new()).is_err());
        assert!(require_admin_or_area_access(&signed_in(Role::Cashier), &granted()).is_ok());
    }

    #[test]
    fn staff_or_area_access_needs_someone_signed_in() {
        assert!(require_admin_or_area_access(&Session::new(), &granted()).is_err());
    }

    // -- shift ownership ---------------------------------------------------

    #[test]
    fn a_cashier_may_act_only_on_their_own_shift() {
        let cashier = User { id: 7, name: "A".into(), role: Role::Cashier };
        assert!(may_act_on_shift(&cashier, Some(7)));
        assert!(!may_act_on_shift(&cashier, Some(8)), "another cashier's shift");
        assert!(!may_act_on_shift(&cashier, None), "a shift with no recorded cashier");
    }

    #[test]
    fn owner_and_admin_may_act_on_any_shift() {
        for role in [Role::Owner, Role::Admin] {
            let staff = User { id: 1, name: "S".into(), role };
            assert!(may_act_on_shift(&staff, Some(99)), "{role:?}");
            assert!(may_act_on_shift(&staff, None), "{role:?}");
        }
    }
}

// ---------------------------------------------------------------------------
// Customers and pay-later (`customers` module). Cashiers take installments at
// the till, so these need a signed-in user, not an admin — except archiving.
// ---------------------------------------------------------------------------

fn require_customers(db: &Db, session: &Session) -> Result<User, String> {
    require_module(db, &[module_key::CUSTOMERS])?;
    require_signed_in(session)
}

/// The caller's open shift, when the shifts module is on — so cash taken
/// outside a sale still lands in that drawer's reconciliation.
fn caller_open_shift(db: &Db, user: &User) -> Result<Option<i64>, String> {
    if !module_enabled(db, module_key::SHIFTS)? {
        return Ok(None);
    }
    Ok(query(db, |conn| shifts::get_open_shift_for_cashier(conn, user.id))?.map(|shift| shift.id))
}

#[tauri::command]
pub fn customers_list(
    db: State<'_, Db>,
    session: State<'_, Session>,
    search: Option<String>,
    include_inactive: Option<bool>,
) -> Result<Vec<customers::Customer>, String> {
    require_customers(&db, &session)?;
    query(&db, |conn| customers::list_customers(conn, search.as_deref(), include_inactive.unwrap_or(false), 2000))
}

/// The billing screen's customer picker: active customers only, few rows.
#[tauri::command]
pub fn customers_search(
    db: State<'_, Db>,
    session: State<'_, Session>,
    search: String,
) -> Result<Vec<customers::Customer>, String> {
    require_customers(&db, &session)?;
    query(&db, |conn| customers::list_customers(conn, Some(&search), false, 12))
}

#[tauri::command]
pub fn customers_get(db: State<'_, Db>, session: State<'_, Session>, id: i64) -> Result<customers::Customer, String> {
    require_customers(&db, &session)?;
    query(&db, |conn| customers::get_customer(conn, id))
}

#[tauri::command]
pub fn customers_add(
    db: State<'_, Db>,
    session: State<'_, Session>,
    input: customers::CustomerInput,
) -> Result<customers::Customer, String> {
    require_customers(&db, &session)?;
    query(&db, |conn| customers::add_customer(conn, input))
}

#[tauri::command]
pub fn customers_update(
    db: State<'_, Db>,
    session: State<'_, Session>,
    id: i64,
    input: customers::CustomerInput,
) -> Result<customers::Customer, String> {
    require_customers(&db, &session)?;
    query(&db, |conn| customers::update_customer(conn, id, input))
}

#[tauri::command]
pub fn customers_set_active(
    db: State<'_, Db>,
    session: State<'_, Session>,
    id: i64,
    active: bool,
) -> Result<customers::Customer, String> {
    require_module_admin(&db, &[module_key::CUSTOMERS], &session)?;
    query(&db, |conn| customers::set_active(conn, id, active))
}

#[tauri::command]
pub fn customers_get_credit_sales(
    db: State<'_, Db>,
    session: State<'_, Session>,
    customer_id: Option<i64>,
    only_outstanding: bool,
) -> Result<Vec<customers::CreditSale>, String> {
    require_module(&db, &[module_key::CUSTOMERS, module_key::LEDGER])?;
    require_signed_in(&session)?;
    query(&db, |conn| customers::list_credit_sales(conn, customer_id, only_outstanding))
}

#[tauri::command]
pub fn customers_get_payments(
    db: State<'_, Db>,
    session: State<'_, Session>,
    customer_id: i64,
) -> Result<Vec<customers::CustomerPayment>, String> {
    require_customers(&db, &session)?;
    query(&db, |conn| customers::list_payments(conn, customer_id))
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomerPaymentArgs {
    pub customer_id: i64,
    pub sale_id: Option<i64>,
    pub amount_minor: i64,
    pub method: String,
    pub note: Option<String>,
}

#[tauri::command]
pub fn customers_record_payment(
    db: State<'_, Db>,
    session: State<'_, Session>,
    input: CustomerPaymentArgs,
) -> Result<Vec<customers::CustomerPayment>, String> {
    let user = require_customers(&db, &session)?;
    let shift_id = caller_open_shift(&db, &user)?;
    transact(&db, |tx| {
        customers::record_payment(
            tx,
            customers::PaymentInput {
                customer_id: input.customer_id,
                sale_id: input.sale_id,
                amount_minor: input.amount_minor,
                method: &input.method,
                note: input.note.as_deref(),
                received_by: Some(user.id),
                shift_id,
            },
        )
    })
}

#[tauri::command]
pub fn customers_get_statement(
    db: State<'_, Db>,
    session: State<'_, Session>,
    customer_id: i64,
    start_date: Option<String>,
    end_date: Option<String>,
) -> Result<customers::Statement, String> {
    require_module(&db, &[module_key::CUSTOMERS, module_key::LEDGER])?;
    require_signed_in(&session)?;
    query(&db, |conn| customers::statement(conn, customer_id, start_date.as_deref(), end_date.as_deref()))
}

/// Pay-later sales past their due date — the payment alerts.
#[tauri::command]
pub fn customers_get_overdue(
    db: State<'_, Db>,
    session: State<'_, Session>,
) -> Result<Vec<customers::CreditSale>, String> {
    require_customers(&db, &session)?;
    query(&db, customers::list_overdue)
}

#[tauri::command]
pub fn customers_get_overdue_summary(
    db: State<'_, Db>,
    session: State<'_, Session>,
) -> Result<customers::OverdueSummary, String> {
    require_customers(&db, &session)?;
    query(&db, customers::overdue_summary)
}

// ---------------------------------------------------------------------------
// Purchasing (`purchasing` module) — Owner/Admin.
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn purchasing_list_suppliers(
    db: State<'_, Db>,
    session: State<'_, Session>,
    include_inactive: Option<bool>,
) -> Result<Vec<purchasing::Supplier>, String> {
    require_module(&db, &[module_key::PURCHASING, module_key::LEDGER])?;
    require_role(&session, ADMIN_ROLES)?;
    query(&db, |conn| purchasing::list_suppliers(conn, include_inactive.unwrap_or(false)))
}

#[tauri::command]
pub fn purchasing_add_supplier(
    db: State<'_, Db>,
    session: State<'_, Session>,
    input: purchasing::SupplierInput,
) -> Result<purchasing::Supplier, String> {
    require_module_admin(&db, &[module_key::PURCHASING], &session)?;
    query(&db, |conn| purchasing::add_supplier(conn, input))
}

#[tauri::command]
pub fn purchasing_update_supplier(
    db: State<'_, Db>,
    session: State<'_, Session>,
    id: i64,
    input: purchasing::SupplierInput,
) -> Result<purchasing::Supplier, String> {
    require_module_admin(&db, &[module_key::PURCHASING], &session)?;
    query(&db, |conn| purchasing::update_supplier(conn, id, input))
}

#[tauri::command]
pub fn purchasing_set_supplier_active(
    db: State<'_, Db>,
    session: State<'_, Session>,
    id: i64,
    active: bool,
) -> Result<purchasing::Supplier, String> {
    require_module_admin(&db, &[module_key::PURCHASING], &session)?;
    query(&db, |conn| purchasing::set_supplier_active(conn, id, active))
}

#[tauri::command]
pub fn purchasing_list_orders(
    db: State<'_, Db>,
    session: State<'_, Session>,
    status: Option<String>,
) -> Result<Vec<purchasing::PurchaseOrder>, String> {
    require_module_admin(&db, &[module_key::PURCHASING], &session)?;
    query(&db, |conn| purchasing::list_pos(conn, status.as_deref(), 500))
}

#[tauri::command]
pub fn purchasing_create_order(
    db: State<'_, Db>,
    session: State<'_, Session>,
    input: purchasing::PoInput,
) -> Result<purchasing::PurchaseOrder, String> {
    let user = require_module_admin(&db, &[module_key::PURCHASING], &session)?;
    transact(&db, |tx| purchasing::create_po(tx, input, Some(user.id)))
}

#[tauri::command]
pub fn purchasing_update_order(
    db: State<'_, Db>,
    session: State<'_, Session>,
    id: i64,
    input: purchasing::PoInput,
) -> Result<purchasing::PurchaseOrder, String> {
    require_module_admin(&db, &[module_key::PURCHASING], &session)?;
    transact(&db, |tx| purchasing::update_po(tx, id, input))
}

#[tauri::command]
pub fn purchasing_set_order_status(
    db: State<'_, Db>,
    session: State<'_, Session>,
    id: i64,
    status: String,
) -> Result<purchasing::PurchaseOrder, String> {
    require_module_admin(&db, &[module_key::PURCHASING], &session)?;
    transact(&db, |tx| purchasing::set_po_status(tx, id, &status))
}

#[tauri::command]
pub fn purchasing_get_incoming(
    db: State<'_, Db>,
    session: State<'_, Session>,
) -> Result<Vec<purchasing::IncomingLine>, String> {
    require_module_admin(&db, &[module_key::PURCHASING], &session)?;
    query(&db, purchasing::incoming_stock)
}

#[tauri::command]
pub fn purchasing_receive_goods(
    db: State<'_, Db>,
    session: State<'_, Session>,
    input: purchasing::GrnInput,
) -> Result<purchasing::GoodsReceipt, String> {
    let user = require_module_admin(&db, &[module_key::PURCHASING], &session)?;
    transact(&db, |tx| purchasing::receive_goods(tx, input, Some(user.id)))
}

#[tauri::command]
pub fn purchasing_list_receipts(
    db: State<'_, Db>,
    session: State<'_, Session>,
    supplier_id: Option<i64>,
) -> Result<Vec<purchasing::GoodsReceipt>, String> {
    require_module_admin(&db, &[module_key::PURCHASING], &session)?;
    query(&db, |conn| purchasing::list_grns(conn, supplier_id, 500))
}

#[tauri::command]
pub fn purchasing_record_payment(
    db: State<'_, Db>,
    session: State<'_, Session>,
    input: purchasing::SupplierPaymentInput,
) -> Result<purchasing::SupplierPayment, String> {
    require_module(&db, &[module_key::PURCHASING, module_key::LEDGER])?;
    let user = require_role(&session, ADMIN_ROLES)?;
    query(&db, |conn| purchasing::record_supplier_payment(conn, input, Some(user.id)))
}

#[tauri::command]
pub fn purchasing_list_payments(
    db: State<'_, Db>,
    session: State<'_, Session>,
    supplier_id: Option<i64>,
) -> Result<Vec<purchasing::SupplierPayment>, String> {
    require_module(&db, &[module_key::PURCHASING, module_key::LEDGER])?;
    require_role(&session, ADMIN_ROLES)?;
    query(&db, |conn| purchasing::list_supplier_payments(conn, supplier_id, 1000))
}

#[tauri::command]
pub fn purchasing_get_statement(
    db: State<'_, Db>,
    session: State<'_, Session>,
    supplier_id: i64,
    start_date: Option<String>,
    end_date: Option<String>,
) -> Result<customers::Statement, String> {
    require_module(&db, &[module_key::PURCHASING, module_key::LEDGER])?;
    require_role(&session, ADMIN_ROLES)?;
    query(&db, |conn| purchasing::supplier_statement(conn, supplier_id, start_date.as_deref(), end_date.as_deref()))
}

/// The Ledger screen's receivables/payables overview.
#[tauri::command]
pub fn ledger_get_summary(db: State<'_, Db>, session: State<'_, Session>) -> Result<insights::LedgerSummary, String> {
    require_module_admin(&db, &[module_key::LEDGER], &session)?;
    query(&db, insights::ledger_summary)
}

// ---------------------------------------------------------------------------
// Stock control (`inventory` module) — Owner/Admin, like item edits.
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn stock_adjust(
    db: State<'_, Db>,
    session: State<'_, Session>,
    input: stock::AdjustmentInput,
) -> Result<Item, String> {
    let user = require_module_admin(&db, &[module_key::INVENTORY], &session)?;
    transact(&db, |tx| stock::adjust(tx, input, Some(user.id)))
}

#[tauri::command]
pub fn stock_list_movements(
    db: State<'_, Db>,
    session: State<'_, Session>,
    item_id: Option<i64>,
    start_date: Option<String>,
    end_date: Option<String>,
    reason: Option<String>,
) -> Result<Vec<stock::StockMovement>, String> {
    require_module_admin(&db, &[module_key::INVENTORY], &session)?;
    query(&db, |conn| {
        stock::list_movements(conn, item_id, start_date.as_deref(), end_date.as_deref(), reason.as_deref(), 1000)
    })
}

#[tauri::command]
pub fn stock_submit_take(
    db: State<'_, Db>,
    session: State<'_, Session>,
    note: Option<String>,
    lines: Vec<stock::CountLine>,
) -> Result<stock::StockTake, String> {
    let user = require_module_admin(&db, &[module_key::INVENTORY], &session)?;
    require_capability(entitlements::capability::INVENTORY_ADVANCED)?;
    transact(&db, |tx| stock::submit_stock_take(tx, note.as_deref(), &lines, Some(user.id)))
}

#[tauri::command]
pub fn stock_list_takes(db: State<'_, Db>, session: State<'_, Session>) -> Result<Vec<stock::StockTake>, String> {
    require_module_admin(&db, &[module_key::INVENTORY], &session)?;
    require_capability(entitlements::capability::INVENTORY_ADVANCED)?;
    query(&db, |conn| stock::list_stock_takes(conn, 100))
}

#[tauri::command]
pub fn stock_get_take(db: State<'_, Db>, session: State<'_, Session>, id: i64) -> Result<stock::StockTake, String> {
    require_module_admin(&db, &[module_key::INVENTORY], &session)?;
    require_capability(entitlements::capability::INVENTORY_ADVANCED)?;
    query(&db, |conn| stock::get_stock_take(conn, id))
}

/// Assigns in-store EAN-13 barcodes to the given items (or every active item
/// without one); returns how many were assigned.
#[tauri::command]
pub fn stock_generate_barcodes(
    db: State<'_, Db>,
    session: State<'_, Session>,
    item_ids: Option<Vec<i64>>,
) -> Result<i64, String> {
    require_module_admin(&db, &[module_key::INVENTORY], &session)?;
    require_capability(entitlements::capability::INVENTORY_ADVANCED)?;
    transact(&db, |tx| stock::generate_barcodes(tx, item_ids.as_deref()))
}

/// A scanned weight-scale barcode, resolved to its item and quantity —
/// `None` when the code isn't one (or scale barcodes are off).
#[tauri::command]
pub fn billing_lookup_scale_barcode(
    db: State<'_, Db>,
    session: State<'_, Session>,
    code: String,
) -> Result<Option<stock::ScaleMatch>, String> {
    require_module(&db, &[module_key::BILLING])?;
    require_signed_in(&session)?;
    query(&db, |conn| {
        let config = config::get(conn)?;
        stock::lookup_scale_barcode(conn, &code, &config)
    })
}

// ---------------------------------------------------------------------------
// Insight reports (Reports screen, area-PIN gated like the rest of it).
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn reports_get_profit_loss(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    start_date: String,
    end_date: String,
    grouping: String,
) -> Result<insights::ProfitLoss, String> {
    require_module_area(&db, &[module_key::REPORTS], &session, &area_access)?;
    require_capability(entitlements::capability::REPORTS_ADVANCED)?;
    let include_expenses = module_enabled(&db, module_key::EXPENSES)?;
    let include_salaries = module_enabled(&db, module_key::SALARY)?;
    query(&db, |conn| {
        insights::profit_loss(conn, &start_date, &end_date, &grouping, include_expenses, include_salaries)
    })
}

#[tauri::command]
pub fn reports_get_order_profit(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    start_date: String,
    end_date: String,
) -> Result<Vec<insights::OrderProfitRow>, String> {
    require_module_area(&db, &[module_key::REPORTS], &session, &area_access)?;
    require_capability(entitlements::capability::REPORTS_ADVANCED)?;
    query(&db, |conn| insights::order_profit(conn, &start_date, &end_date))
}

#[tauri::command]
pub fn reports_get_payment_types(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    start_date: String,
    end_date: String,
) -> Result<insights::PaymentTypeReport, String> {
    require_module_area(&db, &[module_key::REPORTS], &session, &area_access)?;
    require_capability(entitlements::capability::REPORTS_ADVANCED)?;
    query(&db, |conn| insights::payment_types(conn, &start_date, &end_date))
}

#[tauri::command]
pub fn reports_get_tax(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    start_date: String,
    end_date: String,
) -> Result<insights::TaxReport, String> {
    require_module_area(&db, &[module_key::REPORTS], &session, &area_access)?;
    require_capability(entitlements::capability::REPORTS_ADVANCED)?;
    query(&db, |conn| insights::tax_report(conn, &start_date, &end_date))
}

#[tauri::command]
pub fn reports_get_voids(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    start_date: String,
    end_date: String,
) -> Result<insights::VoidReport, String> {
    require_module_area(&db, &[module_key::REPORTS], &session, &area_access)?;
    require_capability(entitlements::capability::REPORTS_ADVANCED)?;
    query(&db, |conn| insights::void_report(conn, &start_date, &end_date))
}

#[tauri::command]
pub fn reports_get_stock_levels(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
) -> Result<insights::StockLevelReport, String> {
    require_module_area(&db, &[module_key::REPORTS], &session, &area_access)?;
    require_capability(entitlements::capability::REPORTS_ADVANCED)?;
    query(&db, insights::stock_levels)
}

#[tauri::command]
pub fn reports_get_inventory_usage(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    start_date: String,
    end_date: String,
) -> Result<Vec<insights::UsageRow>, String> {
    require_module_area(&db, &[module_key::REPORTS], &session, &area_access)?;
    require_capability(entitlements::capability::REPORTS_ADVANCED)?;
    query(&db, |conn| insights::inventory_usage(conn, &start_date, &end_date))
}

#[tauri::command]
pub fn reports_get_expiry(
    db: State<'_, Db>,
    session: State<'_, Session>,
    area_access: State<'_, AreaAccessSession>,
    within_days: i64,
) -> Result<Vec<insights::ExpiryRow>, String> {
    require_module_area(&db, &[module_key::REPORTS], &session, &area_access)?;
    require_capability(entitlements::capability::REPORTS_ADVANCED)?;
    query(&db, |conn| insights::expiry_report(conn, within_days))
}
