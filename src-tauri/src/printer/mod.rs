#![allow(dead_code)]

//! Receipt output.
//!
//! The PDF fallback (the reliable path on every platform, always available
//! regardless of printer setup) is built entirely in the frontend with
//! jsPDF — it needs no Rust support beyond the data already returned by the
//! billing/refund/shift commands. `escpos` holds the thermal-printer path:
//! byte-sequence construction for every template (customer receipt, refund,
//! shift close-out, category-wise sale) is complete and tested, built on
//! the shared column/alignment primitives in `layout`; `android_bt` is the
//! Android-side transport for it (Bluetooth Classic/SPP to an already-paired
//! device), `escpos`'s own USB code is the desktop-side transport — see
//! `escpos`'s module doc comment for why they're two entirely separate code
//! paths rather than one shared one.

pub mod escpos;
pub mod layout;
#[cfg(target_os = "android")]
pub mod android_bt;

/// How a completed sale's receipt should be emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiptTarget {
    /// ESC/POS thermal printer over USB or serial.
    Thermal,
    /// PDF written to disk — the fallback when no printer is configured.
    Pdf,
}
