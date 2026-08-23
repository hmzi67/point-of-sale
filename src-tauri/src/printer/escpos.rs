//! ESC/POS thermal printer output.
//!
//! [`build_receipt_bytes`] — the byte-sequence construction from a completed
//! [`Sale`] — is complete, pure and hardware-independent, so it is fully
//! testable without a printer attached.
//!
//! [`send_to_printer`] dispatches by platform and by what's stored in
//! `AppConfig`'s `printer_connection_type`/`printer_bluetooth_address`
//! (set from Settings' "Select printer" step — see `commands::printer_*`):
//!
//! - **Desktop**: USB, unchanged from before — it looks for any attached
//!   device that exposes a standard USB Printer-class (0x07) interface and
//!   writes the receipt bytes to its bulk OUT endpoint, no vendor/product ID
//!   configuration needed. Serial and network (raw bytes to TCP port 9100)
//!   are still not implemented.
//! - **Android**: Bluetooth Classic (SPP) to whichever *already-paired*
//!   device was selected in Settings — see `printer::android_bt`. There is
//!   deliberately no "select any nearby device" scan step (that needs
//!   `BLUETOOTH_SCAN` + a location permission on older Android versions on
//!   top of `BLUETOOTH_CONNECT`, for a use case — a shop's one till printer
//!   — that's always already paired via the OS Bluetooth settings anyway).
//!   USB-on-Android was tried first and dropped: raw libusb was never
//!   designed for Android's USB-permission model, and calling it there
//!   crashed the whole app natively (a libusb-level fault, not a catchable
//!   Rust panic) — see `Cargo.toml`'s doc comment on why `rusb` is now
//!   `cfg`'d out of Android builds entirely, not just unused there.
//!
//! If no printer has been selected at all, this returns
//! [`PrinterError::NoPrinterSelected`] immediately, on every platform,
//! without touching any transport-specific code — the safe default until a
//! cashier/owner has actually gone through Settings once.
//!
//! Callers must treat any error here as "fall back to the PDF receipt",
//! never as a reason to fail the sale, which is already committed to the
//! database by the time printing is attempted.

use crate::db::config::AppConfig;
use crate::db::reports::{CategorySalesReport, TableSalesSummary};
use crate::db::refunds::Refund;
use crate::db::sales::Sale;
use crate::db::shifts::ShiftSummary;
use crate::printer::layout::{divider, double_divider, format_minor, row, two_col, LINE_WIDTH};
#[cfg(not(target_os = "android"))]
use rusb::{Direction, TransferType};
#[cfg(not(target_os = "android"))]
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

// ---------------------------------------------------------------------------
// Logo raster — the customer receipt's optional store logo.
// ---------------------------------------------------------------------------

/// A decoded, thresholded monochrome bitmap ready for the ESC/POS raster
/// image command (`GS v 0`). Built once per print (see [`build_logo_raster`])
/// from whatever raw file bytes `app_config.logo_path` points at, so
/// [`build_receipt_bytes`] itself stays pure/synchronous and never touches
/// the filesystem or an image decoder directly.
pub struct LogoRaster {
    width_px: usize,
    height_px: usize,
    /// Row-major, MSB-first 1-bit-per-pixel rows, each row padded out to a
    /// whole byte — exactly the layout `GS v 0` expects.
    bitmap: Vec<u8>,
}

/// Widest a logo is allowed to print at, in dots — well under the ~576-dot
/// printable width of 80mm paper at normal density, so a logo reads as a
/// modest mark at the top of the receipt, not a banner.
const LOGO_MAX_WIDTH_PX: u32 = 200;

/// Decodes `image_bytes` (PNG/JPEG — the only formats the `image` crate is
/// built with here) and thresholds it to 1-bit black/white, scaling down to
/// fit [`LOGO_MAX_WIDTH_PX`] if needed. Returns `None` on anything that isn't
/// a decodable raster image — notably an SVG logo, which `app_config` allows
/// as an on-screen logo (see `images::LOGO_ALLOWED_EXTENSIONS`) but which
/// there is no SVG rasterizer wired in here to handle — so the receipt is
/// built exactly as if no logo were set at all, rather than erroring out or
/// sending a corrupt print job.
pub fn build_logo_raster(image_bytes: &[u8]) -> Option<LogoRaster> {
    let img = image::load_from_memory(image_bytes).ok()?;
    let (width, height) = (img.width(), img.height());
    if width == 0 || height == 0 {
        return None;
    }

    let scale = if width > LOGO_MAX_WIDTH_PX { LOGO_MAX_WIDTH_PX as f64 / width as f64 } else { 1.0 };
    let new_width = (((width as f64) * scale).round().max(1.0)) as u32;
    let new_height = (((height as f64) * scale).round().max(1.0)) as u32;
    let gray = img.resize(new_width, new_height, image::imageops::FilterType::Triangle).into_luma8();

    let width_bytes = (new_width as usize + 7) / 8;
    let mut bitmap = vec![0u8; width_bytes * new_height as usize];
    for y in 0..new_height {
        for x in 0..new_width {
            let lum = gray.get_pixel(x, y).0[0];
            // A simple fixed threshold — good enough for the flat black/white
            // shop logos this feeds (line art, wordmarks), not photographs.
            if lum < 160 {
                let idx = y as usize * width_bytes + (x as usize / 8);
                bitmap[idx] |= 0x80 >> (x as usize % 8);
            }
        }
    }

    Some(LogoRaster { width_px: new_width as usize, height_px: new_height as usize, bitmap })
}

/// Emits the `GS v 0` raster bit-image command, centered — the one ESC/POS
/// "print a raw monochrome bitmap" command essentially every printer that
/// speaks ESC/POS at all supports, unlike vendor-specific NV-logo commands.
fn print_logo(out: &mut Vec<u8>, logo: &LogoRaster) {
    let width_bytes = (logo.width_px + 7) / 8;
    out.extend(align_center());
    out.extend([GS, b'v', b'0', 0]);
    out.push((width_bytes & 0xFF) as u8);
    out.push(((width_bytes >> 8) & 0xFF) as u8);
    out.push((logo.height_px & 0xFF) as u8);
    out.push(((logo.height_px >> 8) & 0xFF) as u8);
    out.extend(&logo.bitmap);
    out.push(b'\n');
    out.extend(align_left());
}

/// Item-row column widths shared by every line-item table below: Item(22) /
/// Qty(6) / Rate(8) / Amount(12) = 48 = `LINE_WIDTH`. A public const (not a
/// magic number repeated per template) so the header row and every line
/// stay in lockstep if this ever needs retuning for a 58mm target.
const ITEM_COLS: [usize; 4] = [22, 6, 8, 12];

fn item_row(desc: &str, qty: &str, rate: &str, amount: &str) -> String {
    row(&[(desc, ITEM_COLS[0], false), (qty, ITEM_COLS[1], true), (rate, ITEM_COLS[2], true), (amount, ITEM_COLS[3], true)])
}

/// The header block every template shares: business name (bold, centered),
/// then `subtitle_lines` centered underneath it, then a blank line.
fn header_block(out: &mut Vec<u8>, business_name: &str, subtitle_lines: &[String]) {
    out.extend(align_center());
    out.extend(bold(true));
    out.extend(business_name.as_bytes());
    out.push(b'\n');
    out.extend(bold(false));
    for line in subtitle_lines {
        out.extend(line.as_bytes());
        out.push(b'\n');
    }
    out.push(b'\n');
    out.extend(align_left());
}

fn footer_block(out: &mut Vec<u8>, footer: &str) {
    if !footer.trim().is_empty() {
        out.push(b'\n');
        out.extend(align_center());
        out.extend(footer.as_bytes());
        out.push(b'\n');
    }
}

/// Builds the full ESC/POS byte stream for one customer receipt. Pure and
/// hardware-independent (the logo, if any, arrives pre-decoded as a
/// [`LogoRaster`] — see that type's doc comment), so it is covered by tests
/// below without needing a printer, a serial port, or a Tauri app context.
///
/// `tables_module_enabled` decides the label under "Order Type" when
/// `sale.table_name` is `None`: "Takeaway" if the shop uses tables at all
/// (this sale just wasn't linked to one), "Counter Sale" if the `tables`
/// module isn't in use for this installation.
pub fn build_receipt_bytes(
    sale: &Sale,
    config: &AppConfig,
    logo: Option<&LogoRaster>,
    tables_module_enabled: bool,
) -> Vec<u8> {
    let mut out = Vec::new();
    let currency = &config.currency;

    if let Some(logo) = logo {
        print_logo(&mut out, logo);
    }

    let subtitle = vec![format!("Sale #{}", sale.id), sale.created_at.clone()];
    header_block(&mut out, &config.business_name, &subtitle);

    // Cashier / table-or-order-type — a left-aligned label/value block, same
    // alignment convention as the totals rows below, rather than folded into
    // the centered masthead prose above.
    let (order_label, order_value) = match &sale.table_name {
        Some(table_name) => ("Table", table_name.clone()),
        None => ("Order Type", if tables_module_enabled { "Takeaway".to_string() } else { "Counter Sale".to_string() }),
    };
    out.extend(two_col("Cashier", sale.cashier_name.as_deref().unwrap_or("—")).as_bytes());
    out.push(b'\n');
    out.extend(two_col(order_label, &order_value).as_bytes());
    out.push(b'\n');
    out.push(b'\n');

    out.extend(bold(true));
    out.extend(item_row("Item", "Qty", "Rate", "Amount").as_bytes());
    out.push(b'\n');
    out.extend(bold(false));
    out.extend(divider().as_bytes());
    out.push(b'\n');
    for item in &sale.items {
        out.extend(
            item_row(
                &item.item_name,
                &item.qty.to_string(),
                &format_minor(item.price_at_sale_minor),
                &format_minor(item.line_total_minor),
            )
            .as_bytes(),
        );
        out.push(b'\n');
    }
    out.extend(divider().as_bytes());
    out.push(b'\n');

    out.extend(two_col("Subtotal", &format!("{} {}", currency, format_minor(sale.subtotal_minor))).as_bytes());
    out.push(b'\n');
    if sale.discount_minor > 0 {
        out.extend(
            two_col("Discount", &format!("-{} {}", currency, format_minor(sale.discount_minor))).as_bytes(),
        );
        out.push(b'\n');
    }
    if sale.tax_minor > 0 {
        out.extend(two_col("Tax", &format!("{} {}", currency, format_minor(sale.tax_minor))).as_bytes());
        out.push(b'\n');
    }
    out.extend(double_divider().as_bytes());
    out.push(b'\n');
    out.extend(bold(true));
    out.extend(two_col("TOTAL", &format!("{} {}", currency, format_minor(sale.total_minor))).as_bytes());
    out.push(b'\n');
    out.extend(bold(false));
    out.extend(format!("Paid by: {}\n", sale.payment_method).as_bytes());

    footer_block(&mut out, &config.receipt_footer);
    out.extend(feed_and_cut());
    out
}

/// The "Refund Details" receipt: Vno (original sale id) / item / amount
/// lines, Total Refund at the bottom — same header/footer/divider
/// conventions as the customer receipt, built from `Refund` (see
/// `db::refunds::create_refund`) rather than `Sale`.
pub fn build_refund_bytes(refund: &Refund, config: &AppConfig) -> Vec<u8> {
    let mut out = Vec::new();
    let currency = &config.currency;

    let mut subtitle = vec![
        "REFUND".to_string(),
        format!("Refund #{}", refund.id),
        format!("Vno: {}", refund.original_sale_id),
        refund.created_at.clone(),
    ];
    if let Some(name) = &refund.refunded_by_name {
        subtitle.push(format!("By: {}", name));
    }
    header_block(&mut out, &config.business_name, &subtitle);

    out.extend(bold(true));
    out.extend(item_row("Item", "Qty", "", "Amount").as_bytes());
    out.push(b'\n');
    out.extend(bold(false));
    out.extend(divider().as_bytes());
    out.push(b'\n');
    for item in &refund.items {
        out.extend(
            item_row(
                &item.item_name,
                &item.qty_refunded.to_string(),
                "",
                &format_minor(item.amount_refunded_minor),
            )
            .as_bytes(),
        );
        out.push(b'\n');
    }
    out.extend(divider().as_bytes());
    out.push(b'\n');

    if let Some(reason) = &refund.reason {
        out.extend(format!("Reason: {}\n", reason).as_bytes());
    }

    out.extend(double_divider().as_bytes());
    out.push(b'\n');
    out.extend(bold(true));
    out.extend(
        two_col("Total Refund", &format!("{} {}", currency, format_minor(refund.total_refund_amount_minor)))
            .as_bytes(),
    );
    out.push(b'\n');
    out.extend(bold(false));

    footer_block(&mut out, &config.receipt_footer);
    out.extend(feed_and_cut());
    out
}

/// The "Counter-N Sale Details" shift close-out receipt: opening balance,
/// cash/card/credit/other sale, total sale, declared amount, and the
/// Short/Over difference.
pub fn build_shift_summary_bytes(summary: &ShiftSummary, config: &AppConfig) -> Vec<u8> {
    let mut out = Vec::new();
    let currency = &config.currency;
    let money = |m: i64| format!("{} {}", currency, format_minor(m));

    let mut subtitle = vec![
        format!("Counter-{} Sale Details", summary.shift.id),
        format!("Opened: {}", summary.shift.opened_at),
    ];
    if let Some(closed_at) = &summary.shift.closed_at {
        subtitle.push(format!("Closed: {}", closed_at));
    }
    if let Some(name) = &summary.shift.cashier_name {
        subtitle.push(format!("Cashier: {}", name));
    }
    header_block(&mut out, &config.business_name, &subtitle);

    let rows: [(&str, i64); 6] = [
        ("Opening Balance", summary.opening_balance_minor),
        ("Cash Sale", summary.cash_sales_minor),
        ("Card Sale", summary.card_sales_minor),
        ("Credit Sale", summary.credit_sales_minor),
        ("Other Sale", summary.other_sales_minor),
        ("Total Sale", summary.total_sales_minor),
    ];
    for (label, minor) in rows {
        out.extend(two_col(label, &money(minor)).as_bytes());
        out.push(b'\n');
    }
    out.extend(two_col("Discount Today", &money(summary.discount_minor)).as_bytes());
    out.push(b'\n');
    out.extend(two_col("Refund Today", &money(summary.refunds_minor)).as_bytes());
    out.push(b'\n');

    out.extend(divider().as_bytes());
    out.push(b'\n');
    out.extend(two_col("Expected Cash", &money(summary.expected_cash_minor)).as_bytes());
    out.push(b'\n');

    if let Some(declared) = summary.declared_cash_amount_minor {
        out.extend(two_col("Declared Amount", &money(declared)).as_bytes());
        out.push(b'\n');
    }

    if let Some(difference) = summary.difference_minor {
        out.extend(double_divider().as_bytes());
        out.push(b'\n');
        out.extend(bold(true));
        let label = if difference < 0 { "Short" } else { "Over" };
        out.extend(two_col(label, &money(difference.abs())).as_bytes());
        out.push(b'\n');
        out.extend(bold(false));
    }

    footer_block(&mut out, &config.receipt_footer);
    out.extend(feed_and_cut());
    out
}

/// The "Category Wise Sale" report: one section per category (bold header,
/// item rows, subtotal), grand total at the end.
pub fn build_category_sales_bytes(report: &CategorySalesReport, config: &AppConfig) -> Vec<u8> {
    let mut out = Vec::new();
    let currency = &config.currency;

    let subtitle =
        vec!["Category Wise Sale".to_string(), format!("{} to {}", report.start_date, report.end_date)];
    header_block(&mut out, &config.business_name, &subtitle);

    for group in &report.groups {
        out.extend(bold(true));
        out.extend(group.category_name.as_bytes());
        out.push(b'\n');
        out.extend(bold(false));
        out.extend("-".repeat(group.category_name.chars().count().min(LINE_WIDTH)).as_bytes());
        out.push(b'\n');

        for item in &group.items {
            out.extend(
                row(&[
                    (item.item_name.as_str(), 28, false),
                    (&item.qty_sold.to_string(), 6, true),
                    (&format_minor(item.revenue_minor), 14, true),
                ])
                .as_bytes(),
            );
            out.push(b'\n');
        }
        out.extend(two_col("Subtotal", &format!("{} {}", currency, format_minor(group.subtotal_minor))).as_bytes());
        out.push(b'\n');
        out.push(b'\n');
    }

    out.extend(double_divider().as_bytes());
    out.push(b'\n');
    out.extend(bold(true));
    out.extend(
        two_col("GRAND TOTAL", &format!("{} {}", currency, format_minor(report.grand_total_minor))).as_bytes(),
    );
    out.push(b'\n');
    out.extend(bold(false));

    footer_block(&mut out, &config.receipt_footer);
    out.extend(feed_and_cut());
    out
}

/// The Table Wise Sales report: one row per table (plus "Counter /
/// Takeaway"), grand total at the end — the same flat list-with-grand-total
/// shape as the reference receipt, built from `TableSalesSummary` (see
/// `db::reports::get_table_sales_summary`).
pub fn build_table_sales_bytes(report: &TableSalesSummary, config: &AppConfig) -> Vec<u8> {
    let mut out = Vec::new();
    let currency = &config.currency;

    let subtitle =
        vec!["Table Wise Sales".to_string(), format!("{} to {}", report.start_date, report.end_date)];
    header_block(&mut out, &config.business_name, &subtitle);

    out.extend(bold(true));
    out.extend(row(&[("Table / Counter", 28, false), ("Txns", 6, true), ("Amount", 14, true)]).as_bytes());
    out.push(b'\n');
    out.extend(bold(false));
    out.extend(divider().as_bytes());
    out.push(b'\n');
    for line in &report.rows {
        out.extend(
            row(&[
                (line.label.as_str(), 28, false),
                (&line.transaction_count.to_string(), 6, true),
                (&format_minor(line.total_minor), 14, true),
            ])
            .as_bytes(),
        );
        out.push(b'\n');
    }
    out.extend(divider().as_bytes());
    out.push(b'\n');

    out.extend(double_divider().as_bytes());
    out.push(b'\n');
    out.extend(bold(true));
    out.extend(
        two_col("GRAND TOTAL", &format!("{} {}", currency, format_minor(report.grand_total_minor))).as_bytes(),
    );
    out.push(b'\n');
    out.extend(bold(false));

    footer_block(&mut out, &config.receipt_footer);
    out.extend(feed_and_cut());
    out
}

#[derive(Debug)]
pub enum PrinterError {
    /// No printer has been selected in Settings yet (or, on desktop, none
    /// was auto-detected on USB). The safe, do-nothing-native default.
    NotConfigured,
    /// Selected in Settings, but that selection is missing required data
    /// (e.g. `printer_connection_type = "bluetooth"` with no stored
    /// address) — shouldn't happen through the UI, but a stored config can
    /// outlive the code that wrote it.
    NoPrinterSelected,
    /// Android only: the OS hasn't granted the Bluetooth permission this
    /// build needs. Never a reason to touch the Bluetooth APIs anyway —
    /// checked before any of them are called.
    PermissionDenied,
    Io(String),
}

impl std::fmt::Display for PrinterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PrinterError::NotConfigured => {
                write!(
                    f,
                    "No thermal printer was found — check it's plugged in and powered on, or use the PDF receipt instead"
                )
            }
            PrinterError::NoPrinterSelected => {
                write!(f, "No printer is set up yet — choose one in Settings, or use the PDF receipt instead")
            }
            PrinterError::PermissionDenied => {
                write!(
                    f,
                    "Bluetooth permission wasn't granted — allow it in Settings to print, or use the PDF receipt instead"
                )
            }
            PrinterError::Io(msg) => write!(f, "Printer error: {} — use the PDF receipt instead", msg),
        }
    }
}

/// Routes to the right transport for this platform and this installation's
/// stored printer selection (`AppConfig.printer_connection_type` /
/// `printer_bluetooth_address`, set via Settings — see `commands::printer_*`
/// and `db::config`). See this module's doc comment for why Android and
/// desktop use entirely different transports rather than both trying USB.
///
/// Wrapped in `catch_unwind`: every fallible step on both transports already
/// returns a proper `Result` (nothing here should ever panic), but this is
/// hardware I/O reached through either raw libusb (desktop) or hand-written
/// JNI (Android, see `android_bt`'s doc comment on why that one especially
/// can't be made panic-proof by construction the way pure Rust can) —
/// belt-and-braces so a bug in either one becomes "print failed, use the PDF
/// receipt instead" rather than ever taking the whole app down with it. This
/// is the one and only place that guarantee needs to live: every
/// `commands::*print*` command funnels through here.
fn send_to_printer(bytes: &[u8], config: &AppConfig) -> Result<(), PrinterError> {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| send_to_printer_dispatch(bytes, config)));
    result.unwrap_or_else(|_| {
        Err(PrinterError::Io("the printer driver hit an internal error".to_string()))
    })
}

fn send_to_printer_dispatch(bytes: &[u8], config: &AppConfig) -> Result<(), PrinterError> {
    #[cfg(target_os = "android")]
    {
        match config.printer_connection_type.as_deref() {
            Some("bluetooth") => {
                let address = config.printer_bluetooth_address.as_deref().ok_or(PrinterError::NoPrinterSelected)?;
                crate::printer::android_bt::send(address, bytes)
            }
            // No `printer_connection_type` stored at all — Settings'
            // "Select printer" step has never been used on this install.
            // `NoPrinterSelected`, not `NotConfigured`: the latter's message
            // ("plugged in and powered on") is USB/desktop phrasing that
            // doesn't fit Android's "nothing chosen yet" case.
            _ => Err(PrinterError::NoPrinterSelected),
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = config; // desktop's USB auto-detect doesn't need a stored selection (see module doc)
        send_to_printer_usb(bytes)
    }
}

/// The standard USB device-class code for printers (USB.org base class
/// 0x07) — this is what lets a compatible printer be found without any
/// vendor/product ID configuration.
#[cfg(not(target_os = "android"))]
const USB_PRINTER_CLASS: u8 = 0x07;

/// How long to wait for the printer to accept the write before giving up.
/// A receipt is small (well under 1 KB of ESC/POS bytes), so this only
/// needs to cover a slow/busy printer, not a large transfer.
#[cfg(not(target_os = "android"))]
const USB_WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// Finds the first attached USB device that exposes a Printer-class (0x07)
/// interface with a bulk OUT endpoint, and writes `bytes` to it.
///
/// Tries every matching device/interface it finds in turn (rather than
/// giving up on the first one) — a machine can have other unrelated
/// Printer-class devices (e.g. a label printer) that fail to open or
/// claim, and the first failure shouldn't stop a working receipt printer
/// further down the device list from being tried.
///
/// Desktop only — see this module's doc comment and `Cargo.toml` for why
/// this (and the `rusb` dependency it needs) is `cfg`'d out of Android
/// entirely.
#[cfg(not(target_os = "android"))]
fn send_to_printer_usb(bytes: &[u8]) -> Result<(), PrinterError> {
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

/// Builds a receipt for `sale` and sends it via this installation's
/// configured printer (see [`send_to_printer`]).
pub fn print_receipt(
    sale: &Sale,
    config: &AppConfig,
    logo: Option<&LogoRaster>,
    tables_module_enabled: bool,
) -> Result<(), PrinterError> {
    let bytes = build_receipt_bytes(sale, config, logo, tables_module_enabled);
    send_to_printer(&bytes, config)
}

/// Builds and prints the "Refund Details" receipt for a just-created refund.
pub fn print_refund(refund: &Refund, config: &AppConfig) -> Result<(), PrinterError> {
    let bytes = build_refund_bytes(refund, config);
    send_to_printer(&bytes, config)
}

/// Builds and prints a shift's close-out reconciliation receipt.
pub fn print_shift_summary(summary: &ShiftSummary, config: &AppConfig) -> Result<(), PrinterError> {
    let bytes = build_shift_summary_bytes(summary, config);
    send_to_printer(&bytes, config)
}

/// Builds and prints the Category Wise Sale report.
pub fn print_category_sales(report: &CategorySalesReport, config: &AppConfig) -> Result<(), PrinterError> {
    let bytes = build_category_sales_bytes(report, config);
    send_to_printer(&bytes, config)
}

/// Builds and prints the Table Wise Sales report.
pub fn print_table_sales(report: &TableSalesSummary, config: &AppConfig) -> Result<(), PrinterError> {
    let bytes = build_table_sales_bytes(report, config);
    send_to_printer(&bytes, config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::refunds::RefundLine;
    use crate::db::sales::SaleLine;
    use crate::db::shifts::Shift;

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
            printer_connection_type: None,
            printer_bluetooth_address: None,
            printer_bluetooth_name: None,
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
            shift_id: None,
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

    fn sample_refund() -> Refund {
        Refund {
            id: 7,
            original_sale_id: 42,
            refunded_by: Some(1),
            refunded_by_name: Some("Owner".into()),
            reason: Some("Customer changed mind".into()),
            total_refund_amount_minor: 500,
            created_at: "2026-01-02 09:00:00".into(),
            items: vec![RefundLine {
                sale_item_id: 1,
                item_id: 1,
                item_name: "Cola 500ml".into(),
                qty_refunded: 1,
                amount_refunded_minor: 500,
            }],
        }
    }

    fn sample_shift_summary() -> ShiftSummary {
        ShiftSummary {
            shift: Shift {
                id: 3,
                cashier_id: Some(1),
                cashier_name: Some("Owner".into()),
                opened_at: "2026-01-02 08:00:00".into(),
                closed_at: Some("2026-01-02 20:00:00".into()),
                opening_balance_minor: 10_000,
                declared_cash_amount_minor: Some(25_500),
                notes: None,
            },
            opening_balance_minor: 10_000,
            cash_sales_minor: 15_000,
            card_sales_minor: 4_000,
            other_sales_minor: 0,
            credit_sales_minor: 0,
            total_sales_minor: 19_000,
            discount_minor: 500,
            refunds_minor: 0,
            expected_cash_minor: 25_000,
            declared_cash_amount_minor: Some(25_500),
            difference_minor: Some(500),
        }
    }

    fn sample_category_report() -> CategorySalesReport {
        use crate::db::reports::{CategorySalesGroup, CategorySalesLine};
        CategorySalesReport {
            start_date: "2026-01-01".into(),
            end_date: "2026-01-31".into(),
            groups: vec![CategorySalesGroup {
                category_id: Some(1),
                category_name: "Beverages".into(),
                items: vec![CategorySalesLine {
                    item_id: 1,
                    item_name: "Cola 500ml".into(),
                    qty_sold: 10,
                    revenue_minor: 80_000,
                }],
                subtotal_minor: 80_000,
            }],
            grand_total_minor: 80_000,
        }
    }

    fn sample_table_sales() -> TableSalesSummary {
        use crate::db::reports::TableSalesRow;
        TableSalesSummary {
            start_date: "2026-01-01".into(),
            end_date: "2026-01-31".into(),
            rows: vec![
                TableSalesRow { table_id: Some(1), label: "Table 1".into(), total_minor: 50_000, transaction_count: 4 },
                TableSalesRow { table_id: None, label: "Counter / Takeaway".into(), total_minor: 30_000, transaction_count: 6 },
            ],
            grand_total_minor: 80_000,
        }
    }

    #[test]
    fn build_table_sales_bytes_lists_every_row_and_a_grand_total() {
        let bytes = build_table_sales_bytes(&sample_table_sales(), &sample_config());
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("Table Wise Sales"));
        assert!(text.contains("Table 1"));
        assert!(text.contains("Counter / Takeaway"));
        assert!(text.contains("GRAND TOTAL"));
        assert!(text.contains("PKR 800.00"));
    }

    #[test]
    fn build_receipt_bytes_includes_business_and_line_items() {
        let bytes = build_receipt_bytes(&sample_sale(), &sample_config(), None, true);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("Demo Shop"));
        assert!(text.contains("Sale #42"));
        assert!(text.contains("Cola 500ml"));
        assert!(text.contains("PKR 9.45"), "total must appear formatted with the currency: {text}");
        assert!(text.contains("TOTAL"));
        assert!(text.contains("Thank you!"));
        assert!(text.contains("Cashier"), "cashier row must appear");
        assert!(text.contains("Owner"), "sample_sale's cashier_name must appear");
    }

    #[test]
    fn build_receipt_bytes_shows_the_order_type_when_no_table_is_linked() {
        let mut sale = sample_sale();
        sale.table_name = None;

        let takeaway = build_receipt_bytes(&sale, &sample_config(), None, true);
        assert!(String::from_utf8_lossy(&takeaway).contains("Takeaway"));

        let counter = build_receipt_bytes(&sale, &sample_config(), None, false);
        assert!(String::from_utf8_lossy(&counter).contains("Counter Sale"));
    }

    #[test]
    fn build_receipt_bytes_shows_the_table_name_when_one_is_linked() {
        let mut sale = sample_sale();
        sale.table_name = Some("Table 4".into());
        let bytes = build_receipt_bytes(&sale, &sample_config(), None, true);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("Table 4"));
        assert!(!text.contains("Takeaway"), "a linked table must show the table name, not the takeaway fallback");
    }

    #[test]
    fn build_receipt_bytes_embeds_the_logo_raster_when_given_one() {
        let logo = LogoRaster { width_px: 8, height_px: 8, bitmap: vec![0xFF; 8] };
        let bytes = build_receipt_bytes(&sample_sale(), &sample_config(), Some(&logo), true);
        // GS v 0 with m=0 — the raster command this test asserts got emitted.
        let needle = [GS, b'v', b'0', 0];
        assert!(bytes.windows(needle.len()).any(|w| w == needle), "expected a GS v 0 raster command in the output");
    }

    #[test]
    fn build_receipt_bytes_lines_up_the_item_table_columns() {
        let bytes = build_receipt_bytes(&sample_sale(), &sample_config(), None, true);
        let text = String::from_utf8_lossy(&bytes);
        // Every item row (and the header) must be exactly LINE_WIDTH
        // characters — that's what "columns line up" actually verifies,
        // not just that the right substrings appear somewhere.
        for line in text.lines() {
            if line.contains("Cola 500ml") || line.starts_with("Item") {
                assert_eq!(line.chars().count(), LINE_WIDTH, "misaligned row: {:?}", line);
            }
        }
    }

    #[test]
    fn build_refund_bytes_includes_vno_items_and_total() {
        let bytes = build_refund_bytes(&sample_refund(), &sample_config());
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("REFUND"));
        assert!(text.contains("Vno: 42"), "original sale id must appear as Vno: {text}");
        assert!(text.contains("Cola 500ml"));
        assert!(text.contains("Customer changed mind"));
        assert!(text.contains("Total Refund"));
        assert!(text.contains("PKR 5.00"));
    }

    #[test]
    fn build_shift_summary_bytes_labels_a_positive_difference_as_over() {
        let bytes = build_shift_summary_bytes(&sample_shift_summary(), &sample_config());
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("Counter-3 Sale Details"));
        assert!(text.contains("Opening Balance"));
        assert!(text.contains("Cash Sale"));
        assert!(text.contains("Expected Cash"));
        assert!(text.contains("Declared Amount"));
        assert!(text.contains("Over"), "a positive difference must be labeled Over: {text}");
        assert!(!text.contains("Short"));
    }

    #[test]
    fn build_shift_summary_bytes_labels_a_negative_difference_as_short() {
        let mut summary = sample_shift_summary();
        summary.declared_cash_amount_minor = Some(24_000);
        summary.difference_minor = Some(24_000 - summary.expected_cash_minor);

        let bytes = build_shift_summary_bytes(&summary, &sample_config());
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("Short"), "a negative difference must be labeled Short: {text}");
        assert!(!text.contains("Over"));
    }

    #[test]
    fn build_category_sales_bytes_shows_a_subtotal_per_category_and_a_grand_total() {
        let bytes = build_category_sales_bytes(&sample_category_report(), &sample_config());
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("Beverages"));
        assert!(text.contains("Cola 500ml"));
        assert!(text.contains("Subtotal"));
        assert!(text.contains("GRAND TOTAL"));
        assert!(text.contains("PKR 800.00"));
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
        assert!(print_receipt(&sample_sale(), &sample_config(), None, true).is_err());
    }

    #[test]
    fn build_logo_raster_decodes_a_png_into_a_thresholded_bitmap() {
        // A tiny synthetic checkerboard, round-tripped through the `image`
        // crate's own PNG encoder — no fixture file needed, and it exercises
        // the same decode path a real uploaded logo goes through.
        let img = image::GrayImage::from_fn(4, 4, |x, y| image::Luma([if (x + y) % 2 == 0 { 0 } else { 255 }]));
        let mut png_bytes = Vec::new();
        image::DynamicImage::ImageLuma8(img)
            .write_to(&mut std::io::Cursor::new(&mut png_bytes), image::ImageFormat::Png)
            .unwrap();

        let logo = build_logo_raster(&png_bytes).expect("a valid PNG must decode");
        assert_eq!(logo.width_px, 4);
        assert_eq!(logo.height_px, 4);
        assert_eq!(logo.bitmap.len(), 4, "1 byte per row at width 4 (rounds up to 1 byte), 4 rows");
    }

    #[test]
    fn build_logo_raster_returns_none_for_undecodable_bytes() {
        // Stands in for an SVG logo (or any non-raster/corrupt file) — must
        // fail closed (no logo printed) rather than panic or error out the
        // whole receipt print.
        assert!(build_logo_raster(b"<svg></svg>").is_none());
    }

    #[test]
    fn build_logo_raster_scales_down_an_oversized_logo() {
        let img = image::GrayImage::from_pixel(LOGO_MAX_WIDTH_PX * 2, 100, image::Luma([0]));
        let mut png_bytes = Vec::new();
        image::DynamicImage::ImageLuma8(img)
            .write_to(&mut std::io::Cursor::new(&mut png_bytes), image::ImageFormat::Png)
            .unwrap();

        let logo = build_logo_raster(&png_bytes).expect("a valid PNG must decode");
        assert_eq!(logo.width_px, LOGO_MAX_WIDTH_PX as usize);
        assert_eq!(logo.height_px, 50, "height must scale down proportionally with width");
    }
}
