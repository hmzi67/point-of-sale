// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "windows")]
    if let Some(code) = tauri_app_lib::run_print_worker_if_requested() {
        std::process::exit(code);
    }

    tauri_app_lib::run()
}
