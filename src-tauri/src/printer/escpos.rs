//! ESC/POS thermal printer output.
//!
//! [`build_receipt_bytes`] — the byte-sequence construction from a completed
//! [`Sale`] — is complete, pure and hardware-independent, so it is fully
//! testable without a printer attached.
//!
//! [`send_to_printer`] talks to the printer over USB: it looks for any
//! attached device that exposes a standard USB Printer-class (0x07)
//! interface — which essentially every ESC/POS thermal printer does — and
//! writes the receipt bytes to its bulk OUT endpoint. No vendor/product ID
//! configuration needed, and no client-specific setup: plug in a compatible
//! USB thermal printer and it's found automatically. Serial and network
//! (raw bytes to TCP port 9100) transports are still not implemented —
//! [`PrinterError::NotConfigured`] means "no compatible USB printer found",
//! which also covers a serial-only or network-only printer today. Adding
//! either later is expected to be another small, additive branch in
//! [`send_to_printer`] — nothing about the byte-building above it, or the
//! billing flow that calls it, needs to change.
//!
//! Callers must treat any error here as "fall back to the PDF receipt",
//! never as a reason to fail the sale, which is already committed to the
//! database by the time printing is attempted.

use crate::db::config::AppConfig;
use crate::db::sales::Sale;
use rusb::{Direction, TransferType};
use std::time::Duration;

// Control bytes from the ESC/POS command reference:
// https://reference.epson-biz.com/modules/ref_escpos/index.php
const ESC: u8 = 0x1B;
const GS: u8 = 0x1D;

fn init() -> [u8; 2] {
    [ESC, b'@'] // reset the printer to its power-on state
}

fn bold(on: bool) -> [u8; 3] {
    [ESC, b'E', u8::from(on)]
}

fn align_center() -> [u8; 3] {
    [ESC, b'a', 1]
}

fn align_left() -> [u8; 3] {
    [ESC, b'a', 0]
}

fn feed_and_cut() -> [u8; 5] {
    // Feed a few lines so the cut doesn't clip the last line, then partial cut.
    [b'\n', b'\n', b'\n', GS, b'V']
}

/// Minor units -> a plain "12.34" string, without a currency symbol (the
/// receipt prints the symbol once, in the totals section).
fn format_minor(minor: i64) -> String {
    format!("{}.{:02}", minor / 100, (minor % 100).abs())
}

/// Builds the full ESC/POS byte stream for one receipt. Pure and
/// hardware-independent, so it is covered by tests below without needing a
/// printer, a serial port, or a Tauri app context.
pub fn build_receipt_bytes(sale: &Sale, config: &AppConfig) -> Vec<u8> {
    let mut out = Vec::new();
    let currency = &config.currency;

    out.extend(init());
    out.extend(align_center());
    out.extend(bold(true));
    out.extend(config.business_name.as_bytes());
    out.push(b'\n');
    out.extend(bold(false));
    out.extend(format!("Sale #{}\n", sale.id).as_bytes());
    out.extend(format!("{}\n", sale.created_at).as_bytes());
    if let Some(table_name) = &sale.table_name {
        out.extend(format!("Table: {}\n", table_name).as_bytes());
    }
    out.push(b'\n');

    out.extend(align_left());
    for item in &sale.items {
        out.extend(
            format!(
                "{} x{}  {} {}\n",
                item.item_name,
                item.qty,
                currency,
                format_minor(item.line_total_minor)
            )
            .as_bytes(),
        );
    }

    out.push(b'\n');
    out.extend(format!("Subtotal: {} {}\n", currency, format_minor(sale.subtotal_minor)).as_bytes());
    if sale.discount_minor > 0 {
        out.extend(format!("Discount: -{} {}\n", currency, format_minor(sale.discount_minor)).as_bytes());
    }
    if sale.tax_minor > 0 {
        out.extend(format!("Tax: {} {}\n", currency, format_minor(sale.tax_minor)).as_bytes());
    }
    out.extend(bold(true));
    out.extend(format!("Total: {} {}\n", currency, format_minor(sale.total_minor)).as_bytes());
    out.extend(bold(false));
    out.extend(format!("Paid by: {}\n", sale.payment_method).as_bytes());

    if !config.receipt_footer.trim().is_empty() {
        out.push(b'\n');
        out.extend(align_center());
        out.extend(config.receipt_footer.as_bytes());
        out.push(b'\n');
    }

    out.extend(feed_and_cut());
    out
}

#[derive(Debug)]
pub enum PrinterError {
    /// No USB device exposing a Printer-class interface was found (or a
    /// serial-/network-only printer, since neither transport exists yet).
    NotConfigured,
    Io(String),
}

impl std::fmt::Display for PrinterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PrinterError::NotConfigured => {
                write!(
                    f,
                    "No USB thermal printer was found — check it's plugged in and powered on, or use the PDF receipt instead"
                )
            }
            PrinterError::Io(msg) => write!(f, "Printer error: {}", msg),
        }
    }
}

/// The standard USB device-class code for printers (USB.org base class
/// 0x07) — this is what lets a compatible printer be found without any
/// vendor/product ID configuration.
const USB_PRINTER_CLASS: u8 = 0x07;

/// How long to wait for the printer to accept the write before giving up.
/// A receipt is small (well under 1 KB of ESC/POS bytes), so this only
/// needs to cover a slow/busy printer, not a large transfer.
const USB_WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// Finds the first attached USB device that exposes a Printer-class (0x07)
/// interface with a bulk OUT endpoint, and writes `bytes` to it.
///
/// Tries every matching device/interface it finds in turn (rather than
/// giving up on the first one) — a machine can have other unrelated
/// Printer-class devices (e.g. a label printer) that fail to open or
/// claim, and the first failure shouldn't stop a working receipt printer
/// further down the device list from being tried.
fn send_to_printer(bytes: &[u8]) -> Result<(), PrinterError> {
    let devices = rusb::devices().map_err(|e| PrinterError::Io(format!("USB enumeration failed: {e}")))?;

    for device in devices.iter() {
        let Ok(config) = device.active_config_descriptor() else {
            continue;
        };

        for interface in config.interfaces() {
            for descriptor in interface.descriptors() {
                if descriptor.class_code() != USB_PRINTER_CLASS {
                    continue;
                }
                let Some(endpoint) = descriptor
                    .endpoint_descriptors()
                    .find(|e| e.direction() == Direction::Out && e.transfer_type() == TransferType::Bulk)
                else {
                    continue;
                };

                let Ok(handle) = device.open() else {
                    continue; // e.g. permission denied — try the next device
                };

                let iface_num = descriptor.interface_number();
                // Only Linux kernels attach a driver to Printer-class
                // interfaces by default; on other platforms this reports
                // "unsupported" and can be safely ignored.
                let had_kernel_driver = handle.kernel_driver_active(iface_num).unwrap_or(false);
                if had_kernel_driver {
                    let _ = handle.detach_kernel_driver(iface_num);
                }

                if handle.claim_interface(iface_num).is_err() {
                    if had_kernel_driver {
                        let _ = handle.attach_kernel_driver(iface_num);
                    }
                    continue;
                }

                let result = handle
                    .write_bulk(endpoint.address(), bytes, USB_WRITE_TIMEOUT)
                    .map(|_| ())
                    .map_err(|e| PrinterError::Io(format!("USB write failed: {e}")));

                let _ = handle.release_interface(iface_num);
                if had_kernel_driver {
                    let _ = handle.attach_kernel_driver(iface_num);
                }

                return result;
            }
        }
    }

    Err(PrinterError::NotConfigured)
}

/// Builds a receipt for `sale` and sends it to the first compatible USB
/// thermal printer found (see [`send_to_printer`]).
pub fn print_receipt(sale: &Sale, config: &AppConfig) -> Result<(), PrinterError> {
    let bytes = build_receipt_bytes(sale, config);
    send_to_printer(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::sales::SaleLine;

    fn sample_config() -> AppConfig {
        AppConfig {
            business_name: "Demo Shop".into(),
            business_type: "retail".into(),
            logo_path: None,
            currency: "PKR".into(),
            tax_percent: 5.0,
            receipt_footer: "Thank you!".into(),
            working_days_per_month: 26,
            onboarding_completed: true,
        }
    }

    fn sample_sale() -> Sale {
        Sale {
            id: 42,
            subtotal_minor: 1000,
            discount_minor: 100,
            tax_minor: 45,
            total_minor: 945,
            payment_method: "cash".into(),
            cashier_id: Some(1),
            cashier_name: Some("Owner".into()),
            table_id: None,
            table_name: None,
            created_at: "2026-01-01 10:00:00".into(),
            items: vec![SaleLine {
                item_id: 1,
                item_name: "Cola 500ml".into(),
                qty: 2,
                price_at_sale_minor: 500,
                line_total_minor: 1000,
                notes: None,
            }],
        }
    }

    #[test]
    fn build_receipt_bytes_includes_business_and_line_items() {
        let bytes = build_receipt_bytes(&sample_sale(), &sample_config());
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("Demo Shop"));
        assert!(text.contains("Sale #42"));
        assert!(text.contains("Cola 500ml x2"));
        assert!(text.contains("Total: PKR 9.45"));
        assert!(text.contains("Thank you!"));
    }

    #[test]
    fn format_minor_pads_single_digit_cents() {
        assert_eq!(format_minor(945), "9.45");
        assert_eq!(format_minor(900), "9.00");
        assert_eq!(format_minor(5), "0.05");
    }

    #[test]
    #[ignore = "performs real USB I/O — physically prints a demo receipt on \
                any attached USB thermal printer; run explicitly with \
                `cargo test -- --ignored` on a machine you know has none \
                attached, never as part of the default test run"]
    fn send_to_printer_fails_cleanly_when_no_usb_printer_is_attached() {
        // On a machine with no USB thermal printer attached (and a
        // sandboxed CI runner may not even have USB bus access at all),
        // this must return an error, never panic, so the billing flow's
        // "fall back to PDF" path is safe to rely on. Which specific
        // variant depends on the environment (no matching device vs. no
        // permission to enumerate at all), so this only asserts that it
        // fails, not which error it fails with.
        //
        // Deliberately #[ignore]d: on a machine that *does* have a
        // compatible USB printer attached, `print_receipt` succeeds and
        // actually writes the demo sale's ESC/POS bytes to it — this is
        // real hardware I/O, not a pure function, so it must never run
        // unattended as part of `cargo test`.
        assert!(print_receipt(&sample_sale(), &sample_config()).is_err());
    }
}
