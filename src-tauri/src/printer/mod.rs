#![allow(dead_code)]

//! Receipt output.
//!
//! The PDF fallback (the reliable path on every platform, always available
//! regardless of printer setup) is built entirely in the frontend with
//! jsPDF — it needs no Rust support beyond the data already returned by the
//! billing/refund/shift commands. `escpos` holds the thermal-printer path:
//! byte-sequence construction for every template (customer receipt, refund,
//! shift close-out, category-wise sale) is complete and tested, built on
//! the shared column/alignment primitives in `layout` — three entirely
//! separate transports send those same bytes to hardware, because "how you
//! reach a printer" differs enough per platform that sharing one
//! implementation would mean neither one fitting its platform well:
//! `android_bt` (Bluetooth Classic/SPP to an already-paired device),
//! `windows_spool` (the Print Spooler, RAW datatype, by installed printer
//! name), and `escpos`'s own raw-USB code (macOS/Linux only — see
//! `windows_spool`'s module doc comment for why that approach doesn't work
//! on Windows once a printer driver is installed).

pub mod escpos;
pub mod layout;
#[cfg(target_os = "android")]
pub mod android_bt;
#[cfg(target_os = "windows")]
pub mod windows_spool;

/// How a completed sale's receipt should be emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiptTarget {
    /// ESC/POS thermal printer over USB or serial.
    Thermal,
    /// PDF written to disk — the fallback when no printer is configured.
    Pdf,
}
