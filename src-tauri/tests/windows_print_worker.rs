//! No printer required: exercise the installed executable's worker entry
//! point and verify that failures return an error without starting Tauri.
#![cfg(target_os = "windows")]

use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn run_worker(args: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_tauri-app"))
        .arg("--diwan-print-worker")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch print worker");
    let deadline = Instant::now() + Duration::from_secs(15);
    while child.try_wait().expect("check worker status").is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("print worker did not exit; it may have started the app UI");
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    child.wait_with_output().expect("read worker result")
}

#[test]
fn missing_printer_name_returns_an_error_without_opening_the_app() {
    let output = run_worker(&[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("missing printer name"));
}

#[test]
fn unavailable_printer_returns_an_error_instead_of_crashing() {
    let output = run_worker(&["Diwan_nonexistent_printer_7ac6d290"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("could not open Windows printer"));
}
