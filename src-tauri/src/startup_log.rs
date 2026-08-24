//! Minimal, dependency-light startup logging.
//!
//! Every call opens, appends, writes and flushes in one go — nothing is
//! buffered in memory — so that if the process crashes or aborts
//! ungracefully partway through startup (a WebView2 init failure on
//! Windows, or a panic that unwinds across an FFI boundary into Tauri's
//! runtime and aborts instead of unwinding cleanly — Rust panics are UB,
//! and typically abort, across a `extern "C"` callback boundary), whatever
//! got logged before that point is already durably on disk, not lost with
//! the process.
//!
//! A release build has no attached console, so without this, "the app
//! opens and closes" was completely silent — this is the paper trail for
//! exactly that class of failure. Two locations, tried in order every call
//! rather than picked once: `std::env::temp_dir()` (always writable, no
//! `AppHandle` needed, so this works from the very first line of `run()`,
//! before an app instance — and therefore `app_data_dir()` — exists at
//! all) and, once [`set_log_dir`] has been called, the real
//! `app_data_dir` — the location a client/support call would actually be
//! told to go check. Both are written to once the real directory is
//! known, so the full timeline is never split across two files.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const LOG_FILE_NAME: &str = "pos-startup.log";

static LOG_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

fn temp_log_path() -> PathBuf {
    std::env::temp_dir().join(LOG_FILE_NAME)
}

/// Call once `app_data_dir` is resolved — every `log()` after this also
/// (still) writes to the temp-dir copy, so the timeline never has a gap at
/// the exact moment of the switch.
pub fn set_log_dir(dir: &Path) {
    if let Ok(mut guard) = LOG_DIR.lock() {
        *guard = Some(dir.to_path_buf());
    }
}

fn append(path: &Path, line: &str) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
        let _ = file.flush();
    }
}

/// The log path most likely to have the freshest entries right now — the
/// real `app_data_dir` copy once [`set_log_dir`] has been called, the
/// temp-dir fallback before that. For pointing a user at the right file
/// from the fatal-startup-error dialog.
pub fn active_log_path() -> PathBuf {
    LOG_DIR
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
        .map(|dir| dir.join(LOG_FILE_NAME))
        .unwrap_or_else(temp_log_path)
}

/// Appends one timestamped line, best-effort — a failure to write the log
/// itself must never be a reason to fail startup, so every I/O error here
/// is silently swallowed.
pub fn log(message: &str) {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let line = format!("[{now}] {message}");

    append(&temp_log_path(), &line);
    if let Some(dir) = LOG_DIR.lock().ok().and_then(|guard| guard.clone()) {
        append(&dir.join(LOG_FILE_NAME), &line);
    }
}
