#![allow(dead_code)]

//! Receipt output.
//!
//! The PDF fallback (default today, since no thermal printer is wired up) is
//! built entirely in the frontend with jsPDF — it needs no Rust support
//! beyond the `Sale` + `AppConfig` data already returned by the billing
//! commands. `escpos` holds the thermal-printer path: byte-sequence
//! construction is complete and tested; only the hardware transport remains
//! a stub. See `escpos`'s module doc for the plan to fill that in.

pub mod escpos;

/// How a completed sale's receipt should be emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiptTarget {
    /// ESC/POS thermal printer over USB or serial.
    Thermal,
    /// PDF written to disk — the fallback when no printer is configured.
    Pdf,
}
