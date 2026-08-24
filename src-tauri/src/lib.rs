mod commands;
mod db;
mod images;
mod printer;
mod product_owner_session;
mod session;

use tauri::Manager;

use crate::db::Db;
use crate::product_owner_session::ProductOwnerSession;
use crate::session::Session;

/// Splash duration bounds. Shared with the Android-side splash
/// (`SplashGate.tsx`'s `MIN_SPLASH_MS` / `MAX_SPLASH_MS`) so both platforms
/// feel the same, even though the mechanism differs (a real second window
/// here, an in-app overlay there — Tauri's secondary-window pattern isn't
/// available on mobile).
const MIN_SPLASH_MS: u64 = 400;
const MAX_SPLASH_MS: u64 = 3000;

/// Process-launch timestamp, managed as app state so both the
/// `splashscreen_ready` command (invoked by the splash's own script — see
/// `public/splashscreen.html`) and the `MAX_SPLASH_MS` watchdog thread below
/// measure the same clock.
struct LaunchedAt(std::time::Instant);

/// Guards `reveal_main_window` against running twice — the splash's "I've
/// painted" signal and the `MAX_SPLASH_MS` ceiling both call it, and
/// whichever fires first must win outright, not both.
struct SplashRevealed(std::sync::atomic::AtomicBool);

/// Closes the "splashscreen" window and reveals "main". Idempotent — safe to
/// call from both the ready-signal path and the timeout path; only the
/// first call actually does anything. A no-op on Android, where there is no
/// "splashscreen" window at all (see `run()`'s `#[cfg(not(target_os =
/// "android"))]` branch — "main" there is built already-visible, with no
/// separate secondary-window splash; Tauri's secondary-window pattern isn't
/// supported on mobile) — the Android splash experience is instead the
/// in-app `SplashGate` overlay on the frontend side, timed independently.
fn reveal_main_window(app: &tauri::AppHandle) {
    let already_revealed = app.state::<SplashRevealed>();
    if already_revealed.0.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }

    if let Some(splash) = app.get_webview_window("splashscreen") {
        let _ = splash.close();
    }
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.show();
        let _ = main.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let launched_at = std::time::Instant::now();

    tauri::Builder::default()
        // `Session`/`ProductOwnerSession`/`LaunchedAt`/`SplashRevealed` need
        // nothing but their own constructors — no `AppHandle`, no filesystem
        // access — so they're managed here, directly on the builder, before
        // any window (and therefore any webview, and therefore any frontend
        // JS that might `invoke()` a command) exists at all. That's what
        // actually closes the "state not managed" race described below for
        // these four; `Db` still has to wait for `.setup()` since resolving
        // `app_data_dir()` needs an `AppHandle`.
        .manage(Session::new())
        .manage(ProductOwnerSession::new())
        .manage(LaunchedAt(launched_at))
        .manage(SplashRevealed(std::sync::atomic::AtomicBool::new(false)))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_os::init())
        // Native "Save As" flow for exports (CSV/PDF reports) — the dialog
        // plugin shows the OS save picker (Storage Access Framework on
        // Android, the native panel on desktop) and returns a path; the fs
        // plugin then writes bytes to it. Replaces the old `<a download>`
        // blob trick, which desktop browsers handle but Android's WebView
        // does not (see `src/services/fileExportService.ts`).
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(move |app| {
            let dir = app.path().app_data_dir()?;
            let db = Db::open(dir)?;
            app.manage(db);

            // The "main" window is built here — *after* `db` is managed —
            // rather than left in `tauri.conf.json`'s static `windows` list.
            // A statically-declared window's webview starts loading (and can
            // fire its first `invoke()`) the moment the window is created,
            // which happens before `.setup()` runs at all — so on a slow
            // first launch (seen in practice on Windows: a fresh
            // `app_data_dir`, antivirus scanning the newly-written SQLite
            // file, a cold disk) the frontend's very first command call
            // could land before `Db::open()` above had finished, failing
            // with "state not managed for field `db` on command `get_users`"
            // even though the app would have worked fine a moment later.
            // Creating the window down here instead makes that ordering
            // impossible: the webview simply doesn't exist yet for any JS
            // inside it to run.
            let mut main_window =
                tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("index.html".into()))
                    .title("POS")
                    .inner_size(1280.0, 800.0);
            // Desktop only: mobile has no splash *window* to reveal past
            // (see `reveal_main_window`'s doc comment) — Android's "main" is
            // shown immediately, at its config's default size, with no
            // minimum-size constraint of its own.
            #[cfg(not(target_os = "android"))]
            {
                main_window = main_window.min_inner_size(1024.0, 640.0).visible(false);
            }
            main_window.build()?;

            // Absolute ceiling, independent of whether the splash's own
            // "I've painted" signal (`splashscreen_ready`, the primary path)
            // ever arrives — so a splash that somehow never gets to render
            // still can't hang the app open on it forever. A no-op on
            // Android: no "splashscreen" window exists there to reveal past.
            if app.get_webview_window("splashscreen").is_some() {
                let app_handle = app.handle().clone();
                std::thread::spawn(move || {
                    let remaining = MAX_SPLASH_MS.saturating_sub(launched_at.elapsed().as_millis() as u64);
                    std::thread::sleep(std::time::Duration::from_millis(remaining));
                    reveal_main_window(&app_handle);
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_ping,
            commands::splashscreen_ready,
            commands::app_db_version,
            commands::app_db_tables,
            commands::get_app_config,
            commands::update_app_config,
            commands::config_upload_logo,
            commands::config_get_logo,
            commands::get_enabled_modules,
            commands::toggle_module,
            commands::inventory_get_items,
            commands::inventory_add_item,
            commands::inventory_update_item,
            commands::inventory_delete_item,
            commands::inventory_get_categories,
            commands::inventory_add_category,
            commands::inventory_import_csv,
            commands::inventory_csv_template,
            commands::inventory_upload_image,
            commands::inventory_get_image,
            commands::inventory_get_best_selling_item_ids,
            commands::billing_search_items,
            commands::billing_create_sale,
            commands::billing_get_sale,
            commands::billing_print_receipt_thermal,
            commands::printer_list_bluetooth_devices,
            commands::printer_bluetooth_permission_granted,
            commands::printer_request_bluetooth_permission,
            commands::billing_list_recent_sales,
            commands::refund_get_sale,
            commands::refund_create,
            commands::refund_get,
            commands::refund_print_thermal,
            commands::shift_get_open,
            commands::shift_open,
            commands::shift_get_summary,
            commands::shift_close,
            commands::shift_list_recent,
            commands::shift_print_summary,
            commands::reports_get_sales_summary,
            commands::reports_get_top_items,
            commands::reports_get_sales_over_time,
            commands::reports_get_category_sales,
            commands::reports_print_category_sales,
            commands::reports_get_table_sales_summary,
            commands::reports_print_table_sales_summary,
            commands::reports_get_full_report,
            commands::reports_print_full_report,
            commands::reports_get_product_sales_summary,
            commands::tables_get_tables,
            commands::tables_add_table,
            commands::tables_update_table_status,
            commands::tables_assign_order_to_table,
            commands::tables_clear_table,
            commands::tables_attach_cart_to_table,
            commands::tables_get_parked_cart,
            commands::attendance_get_employees,
            commands::attendance_get_all_employees,
            commands::attendance_add_employee,
            commands::attendance_update_employee,
            commands::attendance_set_employee_active,
            commands::attendance_check_in,
            commands::attendance_check_out,
            commands::attendance_get_attendance,
            commands::attendance_get_monthly_summary,
            commands::expenses_add_expense,
            commands::expenses_get_expenses,
            commands::expenses_get_expense_categories,
            commands::expenses_get_totals_by_category,
            commands::salary_calculate_salary,
            commands::salary_get_monthly_overview,
            commands::salary_record_payment,
            commands::salary_get_payment_history,
            commands::dashboard_get_summary,
            commands::product_owner_get_status,
            commands::product_owner_setup,
            commands::product_owner_login,
            commands::product_owner_logout,
            commands::product_owner_get_modules,
            commands::product_owner_set_module,
            commands::get_users,
            commands::login,
            commands::logout,
            commands::create_user,
            commands::set_user_pin,
            commands::get_all_users,
            commands::update_user,
            commands::set_user_active
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
