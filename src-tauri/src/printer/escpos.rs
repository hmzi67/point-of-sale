//! ESC/POS thermal printer output.
//!
//! Not wired to real hardware yet. [`build_receipt_bytes`] — the byte-sequence
//! construction from a completed [`Sale`] — is complete, pure and
//! hardware-independent, so it is fully testable without a printer attached.
//! [`send_to_printer`] is the one function a later phase needs to fill in
//! with a real transport (USB via a crate like `rusb`, serial via
//! `serialport`, or network by writing raw bytes to a printer listening on
//! TCP port 9100). Plugging that in is expected to be a small, additive
//! change to this one function — nothing about the byte-building above it, or
//! the billing flow that calls it, needs to change.
//!
//! Until real hardware is wired up, [`print_receipt`] always returns
//! [`PrinterError::NotConfigured`] — callers must treat that as "fall back to
//! the PDF receipt", never as a reason to fail the sale, which is already
//! committed to the database by the time printing is attempted.

use crate::db::config::AppConfig;
use crate::db::sales::Sale;

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
    /// No thermal printer transport has been implemented/configured yet.
    NotConfigured,
    #[allow(dead_code)] // used once a real transport lands
    Io(String),
}

impl std::fmt::Display for PrinterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PrinterError::NotConfigured => {
                write!(f, "No thermal printer is configured — use the PDF receipt instead")
            }
            PrinterError::Io(msg) => write!(f, "Printer error: {}", msg),
        }
    }
}

/// TODO(hardware): the real transport. Pick one printer interface (USB,
/// serial, or network) and write `bytes` to it here — everything that
/// produces `bytes` (`build_receipt_bytes`) already needs no changes.
fn send_to_printer(_bytes: &[u8]) -> Result<(), PrinterError> {
    Err(PrinterError::NotConfigured)
}

/// Builds a receipt for `sale` and attempts to print it. Always fails today
/// (see [`send_to_printer`]) — present so the calling code path, the error
/// type, and the byte format are all real and already exercised by tests,
/// leaving only the transport to fill in later.
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
    fn send_to_printer_is_not_configured_until_hardware_is_wired_up() {
        let err = print_receipt(&sample_sale(), &sample_config()).unwrap_err();
        assert!(matches!(err, PrinterError::NotConfigured));
    }
}
