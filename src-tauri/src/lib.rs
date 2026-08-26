mod commands;
mod db;
mod images;
mod printer;
mod product_owner_session;
mod session;
mod startup_log;

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

/// Guards `reveal_main_window` against actually revealing the window twice
/// — the splash's "I've painted" signal and the `MAX_SPLASH_MS` ceiling both
/// call it, and whichever one *successfully* reveals the window must win
/// outright, not both. Deliberately only flipped to `true` once a reveal has
/// actually happened (see `reveal_main_window`) — flipping it on a call that
/// merely raced `setup()` and found no window yet would permanently lock the
/// other caller out too, which is exactly the bug this used to have (see
/// that fn's doc comment).
struct SplashRevealed(std::sync::atomic::AtomicBool);

/// `main`'s configured size (must match the `WebviewWindowBuilder` call in
/// `setup()`) — used to force the window back to a sane size on reveal
/// rather than trusting whatever restore/default size the OS handed it.
const MAIN_WINDOW_WIDTH: f64 = 1280.0;
const MAIN_WINDOW_HEIGHT: f64 = 800.0;

/// How long `reveal_main_window` will poll for "main" to exist before giving
/// up. Confirmed on Windows: `splashscreen_ready`'s IPC call can land before
/// `setup()` finishes `WebviewWindowBuilder::build()` for "main" — WebView2
/// can paint the splash and fire its `load` handler faster than `setup()`
/// gets through `Db::open()` + the window build on a slow disk/AV-scanned
/// first launch. Rather than treat a `None` from `get_webview_window` as
/// permanent failure, retry for a bit; `setup()` finishing is normally
/// ~100ms away at that point, so this essentially never has to wait the
/// full ceiling out in practice.
const REVEAL_POLL_INTERVAL_MS: u64 = 25;
const REVEAL_POLL_TIMEOUT_MS: u64 = 2000;

/// Closes the "splashscreen" window and reveals "main", polling for "main"
/// to exist first (see `REVEAL_POLL_TIMEOUT_MS`) rather than assuming
/// `setup()` has already built it. Idempotent — safe to call from both the
/// ready-signal path and the timeout path; only the call that actually
/// succeeds in finding and showing the window does anything, and only that
/// one flips `SplashRevealed`. A no-op on Android, where there is no
/// "splashscreen" window at all (see `run()`'s `#[cfg(not(target_os =
/// "android"))]` branch — "main" there is built already-visible, with no
/// separate secondary-window splash; Tauri's secondary-window pattern isn't
/// supported on mobile) — the Android splash experience is instead the
/// in-app `SplashGate` overlay on the frontend side, timed independently.
fn reveal_main_window(app: &tauri::AppHandle) {
    let already_revealed = app.state::<SplashRevealed>();
    if already_revealed.0.load(std::sync::atomic::Ordering::SeqCst) {
        return;
    }

    let mut waited_ms = 0u64;
    loop {
        // Show `main` *before* closing `splashscreen` — with only one
        // window open at a time, Tauri's default "exit when the last window
        // closes" behavior means closing splashscreen first, if showing
        // main were ever to silently no-op (a `None` from
        // `get_webview_window`, a `.show()` that errors), would leave the
        // app with zero visible windows and the whole process would exit —
        // exactly "splash appears, then the app closes entirely, never
        // reaching the main window." Every step below is logged with its
        // actual `Result`/state so a future regression shows up as a
        // specific failed call instead of a silent no-op.
        match app.get_webview_window("main") {
            Some(main) => {
                // Someone else (the other caller — ready-signal vs.
                // timeout) already won the race while we were polling.
                if already_revealed.0.swap(true, std::sync::atomic::Ordering::SeqCst) {
                    return;
                }

                let shown = main.show();
                let unminimized = main.unminimize();
                // Force back to the configured size/position rather than
                // trusting whatever the OS restored — confirmed on Windows
                // that a window left in this state can end up visible but
                // sized ~15x15px in the top-left corner.
                let resized = main.set_size(tauri::Size::Logical(tauri::LogicalSize::new(
                    MAIN_WINDOW_WIDTH,
                    MAIN_WINDOW_HEIGHT,
                )));
                let centered = main.center();
                let focused = main.set_focus();
                let visible_after = main.is_visible();
                let size_after = main.inner_size();
                startup_log::log(&format!(
                    "reveal_main_window: show={shown:?} unminimize={unminimized:?} \
                     set_size={resized:?} center={centered:?} focus={focused:?} \
                     is_visible={visible_after:?} inner_size={size_after:?}"
                ));

                match app.get_webview_window("splashscreen") {
                    Some(splash) => {
                        let closed = splash.close();
                        startup_log::log(&format!("reveal_main_window: splashscreen close={closed:?}"));
                    }
                    None => startup_log::log(
                        "reveal_main_window: no \"splashscreen\" window found to close (already closed?)",
                    ),
                }
                return;
            }
            None => {
                if waited_ms >= REVEAL_POLL_TIMEOUT_MS {
                    startup_log::log(&format!(
                        "reveal_main_window: gave up after {waited_ms}ms — \"main\" window never appeared"
                    ));
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(REVEAL_POLL_INTERVAL_MS));
                waited_ms += REVEAL_POLL_INTERVAL_MS;
            }
        }
    }
}

/// Extracts a printable message from a `catch_unwind` payload — panics can
/// carry either a `&str` (the common `panic!("literal")` case) or a
/// `String` (`panic!("{}", x)`/`.expect(...)`), and anything else prints as
/// a fixed placeholder rather than nothing at all.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "panicked with a non-string payload".to_string()
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let launched_at = std::time::Instant::now();
    startup_log::log("run() entered");

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
            // The whole body is wrapped in `catch_unwind`: every fallible
            // step below already returns a proper `Result` via `?` (nothing
            // here should ever panic), but this runs once, at startup,
            // where a panic is the single worst place for one to happen —
            // unwinding out of `setup()` and back into Tauri's own runtime
            // internals is exactly the kind of cross-callback-boundary
            // unwind that can abort the process outright on some platforms
            // instead of surfacing as an `Err` (see `startup_log`'s doc
            // comment), which would make even this logging useless. Belt
            // and braces, same pattern `printer::escpos::send_to_printer`
            // already uses for hardware I/O.
            let app_ref = &*app;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                move || -> Result<(), Box<dyn std::error::Error>> {
                    startup_log::log("setup(): entered");

                    let dir = app_ref.path().app_data_dir()?;
                    startup_log::set_log_dir(&dir);
                    startup_log::log(&format!("setup(): app_data_dir resolved to {}", dir.display()));

                    let db = Db::open(dir)?;
                    startup_log::log("setup(): database opened and migrated");
                    app_ref.manage(db);

                    // The "main" window is built here — *after* `db` is
                    // managed — rather than left in `tauri.conf.json`'s
                    // static `windows` list. A statically-declared window's
                    // webview starts loading (and can fire its first
                    // `invoke()`) the moment the window is created, which
                    // happens before `.setup()` runs at all — so on a slow
                    // first launch (seen in practice on Windows: a fresh
                    // `app_data_dir`, antivirus scanning the newly-written
                    // SQLite file, a cold disk) the frontend's very first
                    // command call could land before `Db::open()` above had
                    // finished, failing with "state not managed for field
                    // `db` on command `get_users`" even though the app
                    // would have worked fine a moment later. Creating the
                    // window down here instead makes that ordering
                    // impossible: the webview simply doesn't exist yet for
                    // any JS inside it to run.
                    // Only reassigned on desktop (see below) — Android's
                    // branch never mutates it, hence `#[allow(unused_mut)]`
                    // rather than a `mut` that'd warn on that target.
                    #[allow(unused_mut)]
                    let mut main_window = tauri::WebviewWindowBuilder::new(
                        app_ref,
                        "main",
                        tauri::WebviewUrl::App("index.html".into()),
                    )
                    .title("Diwan")
                    .inner_size(1280.0, 800.0);
                    // Desktop only: mobile has no splash *window* to reveal
                    // past (see `reveal_main_window`'s doc comment) —
                    // Android's "main" is shown immediately, at its config's
                    // default size, with no minimum-size constraint of its
                    // own.
                    #[cfg(not(target_os = "android"))]
                    {
                        main_window = main_window.min_inner_size(1024.0, 640.0).visible(false);
                    }
                    main_window.build()?;
                    startup_log::log("setup(): \"main\" window built");

                    // Absolute ceiling, independent of whether the splash's
                    // own "I've painted" signal (`splashscreen_ready`, the
                    // primary path) ever arrives — so a splash that somehow
                    // never gets to render still can't hang the app open on
                    // it forever. A no-op on Android: no "splashscreen"
                    // window exists there to reveal past.
                    if app_ref.get_webview_window("splashscreen").is_some() {
                        let app_handle = app_ref.handle().clone();
                        std::thread::spawn(move || {
                            let remaining = MAX_SPLASH_MS.saturating_sub(launched_at.elapsed().as_millis() as u64);
                            std::thread::sleep(std::time::Duration::from_millis(remaining));
                            startup_log::log("MAX_SPLASH_MS ceiling reached — revealing main window");
                            reveal_main_window(&app_handle);
                        });
                    }

                    startup_log::log("setup(): complete");
                    Ok(())
                },
            ));

            match result {
                Ok(inner) => inner.map_err(|e| {
                    startup_log::log(&format!("setup() returned an error: {e}"));
                    e
                }),
                Err(panic) => {
                    let message = panic_message(&*panic);
                    startup_log::log(&format!("setup() PANICKED: {message}"));
                    Err(message.into())
                }
            }
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
            commands::printer_list_windows_printers,
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
            commands::reports_get_refunds_summary,
            commands::reports_print_refunds_summary,
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
        .unwrap_or_else(|error| {
            startup_log::log(&format!("run() returned a fatal error: {error}"));
            // A release build has no attached console, so a fatal startup
            // failure — no writable app-data folder, a database that won't
            // open, a missing WebView2 runtime on Windows, `setup()` erroring
            // for any reason — used to be completely invisible: the process
            // would just exit right after the splash window appeared, with
            // nothing on screen to say why (this is what "opens a dialog and
            // closes automatically, no idea why" on a Windows install turned
            // out to be — not the dialog itself failing, the *app* dying
            // moments later with the real reason going nowhere).
            //
            // `rfd` (not `tauri_plugin_dialog`) specifically because it's
            // fully standalone — it doesn't need a running Tauri app/event
            // loop, so it still works even when the failure happens this
            // early, before `.run()` ever gets an app instance off the
            // ground. Desktop-only: `rfd` has no backend implementation for
            // Android at all (a compile error, not a runtime gap — see
            // Cargo.toml's doc comment), so this falls back to `eprintln!`
            // there, which reaches `adb logcat`.
            #[cfg(not(target_os = "android"))]
            rfd::MessageDialog::new()
                .set_title("Diwan failed to start")
                .set_description(&format!(
                    "{error}\n\nDetails were written to:\n{}",
                    startup_log::active_log_path().display()
                ))
                .set_level(rfd::MessageLevel::Error)
                .show();
            #[cfg(target_os = "android")]
            eprintln!(
                "Diwan failed to start: {error}\n\nDetails were written to: {}",
                startup_log::active_log_path().display()
            );
            std::process::exit(1);
        });
}
