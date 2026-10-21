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
//! exactly that class of failure. Until [`set_log_dir`] is called the log
//! goes to `std::env::temp_dir()` (always writable, no `AppHandle` needed,
//! so this works from the very first line of `run()`, before an app
//! instance — and therefore `app_data_dir()` — exists at all); from then on
//! it goes only to the real `app_data_dir`, the location a client/support
//! call would actually be told to go check. Licence and IAP events land
//! here for the whole session, so they stay out of the shared temp dir.
//!
//! Each file is rotated to `pos-startup.log.1` once it passes
//! [`MAX_LOG_BYTES`], so a long-running till never grows it without bound.
//! Every line carries a level ([`log`] = INFO, [`warn`], [`error`]);
//! [`debug`] lines are written by debug builds only. Text that came from
//! outside the app (a server's refusal message, a transport error) goes
//! through [`clip`] first so one oversized response can't flood the file.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const LOG_FILE_NAME: &str = "pos-startup.log";
const ROTATED_FILE_NAME: &str = "pos-startup.log.1";
/// Size at which the log is rotated to [`ROTATED_FILE_NAME`].
const MAX_LOG_BYTES: u64 = 1024 * 1024;
/// Longest piece of externally-supplied text [`clip`] lets through.
const MAX_EXTERNAL_TEXT_CHARS: usize = 200;

static LOG_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

fn temp_log_path() -> PathBuf {
    std::env::temp_dir().join(LOG_FILE_NAME)
}

/// Call once `app_data_dir` is resolved — every line after this goes to
/// the app-data copy instead of the temp-dir one. The first line written
/// there points back at the temp file, which holds everything before it.
pub fn set_log_dir(dir: &Path) {
    if let Ok(mut guard) = LOG_DIR.lock() {
        *guard = Some(dir.to_path_buf());
    }
    debug(&format!("log continues from {}", temp_log_path().display()));
}

fn append(path: &Path, line: &str) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if std::fs::metadata(path).is_ok_and(|m| m.len() >= MAX_LOG_BYTES) {
        let _ = std::fs::rename(path, path.with_file_name(ROTATED_FILE_NAME));
    }
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    // Owner-only: before `set_log_dir` this lives in the temp dir, which is
    // shared between users on Linux. (Windows' and macOS' temp dirs are per-user.)
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    if let Ok(mut file) = options.open(path) {
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
fn write(level: &str, message: &str) {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let line = format!("[{now}] {level:<5} {message}");
    append(&active_log_path(), &line);
}

/// A routine startup/progress line.
pub fn log(message: &str) {
    write("INFO", message);
}

/// Something went wrong but the app carries on (a skipped refresh, a fallback).
pub fn warn(message: &str) {
    write("WARN", message);
}

/// A failure the app could not recover from, or a panic.
pub fn error(message: &str) {
    write("ERROR", message);
}

/// Detail only worth having while developing — full filesystem paths and
/// the like. Compiled to a no-op in release builds.
pub fn debug(message: &str) {
    if cfg!(debug_assertions) {
        write("DEBUG", message);
    }
}

/// Shortens text that came from outside the app before it is logged, and
/// flattens control characters so it can't forge extra log lines.
pub fn clip(text: &str) -> String {
    let mut chars = text.chars().map(|c| if c.is_control() { ' ' } else { c });
    let clipped: String = chars.by_ref().take(MAX_EXTERNAL_TEXT_CHARS).collect();
    if chars.next().is_some() { format!("{clipped}…") } else { clipped }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_leaves_short_text_alone_and_cuts_long_text() {
        assert_eq!(clip("refused"), "refused");
        let long = "x".repeat(MAX_EXTERNAL_TEXT_CHARS + 50);
        let clipped = clip(&long);
        assert_eq!(clipped.chars().count(), MAX_EXTERNAL_TEXT_CHARS + 1);
        assert!(clipped.ends_with('…'));
        assert_eq!(clip("a\n[ERROR] forged\r"), "a [ERROR] forged ");
    }
}
