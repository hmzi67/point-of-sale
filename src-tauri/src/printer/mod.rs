#![allow(dead_code)]

//! Receipt output.
//!
//! The PDF fallback (default today, since USB is the only wired-up printer
//! transport) is built entirely in the frontend with jsPDF — it needs no
//! Rust support beyond the data already returned by the billing/refund/
//! shift commands. `escpos` holds the thermal-printer path: byte-sequence
//! construction for every template (customer receipt, refund, shift
//! close-out, category-wise sale) is complete and tested, built on the
//! shared column/alignment primitives in `layout`.

pub mod escpos;
pub mod layout;

/// How a completed sale's receipt should be emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiptTarget {
    /// ESC/POS thermal printer over USB or serial.
    Thermal,
    /// PDF written to disk — the fallback when no printer is configured.
    Pdf,
}
