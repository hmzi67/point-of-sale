//! ESC/POS thermal printer output.
//!
//! [`build_receipt_bytes`] — the byte-sequence construction from a completed
//! [`Sale`] — is complete, pure and hardware-independent, so it is fully
//! testable without a printer attached.
//!
//! [`send_to_printer`] dispatches by platform and by what's stored in
//! `AppConfig`'s `printer_connection_type` and whichever transport-specific
//! field goes with it (`printer_bluetooth_address`, `printer_windows_name`)
//! — set from Settings' "Select printer" step, see `commands::printer_*`:
//!
//! - **Windows**: the Print Spooler (`winspool`), RAW datatype, to whichever
//!   *installed* printer was selected in Settings by name — see
//!   `printer::windows_spool`. Not raw USB: a printer installed the normal
//!   Windows way (driver present, visible in "Devices and Printers") has its
//!   driver's own service bound to its USB interface, which blocks
//!   libusb/WinUSB from ever claiming it — see that module's doc comment
//!   for the full story. This is why Windows needs an explicit name-based
//!   selection step, the same as Android's Bluetooth picker, rather than the
//!   old "just look for a USB Printer-class device" auto-detect.
//! - **macOS/Linux**: USB, unchanged from before — it looks for any
//!   attached device that exposes a standard USB Printer-class (0x07)
//!   interface and writes the receipt bytes to its bulk OUT endpoint, no
//!   vendor/product ID configuration or selection step needed. Serial and
//!   network (raw bytes to TCP port 9100) are still not implemented. (macOS
//!   has its own OS-level print system, CUPS, with a "raw" queue concept
//!   analogous to Windows' RAW datatype — but that's unbuilt; this crate's
//!   raw-USB path is what macOS gets today, same as before this fix, which
//!   was scoped to Windows specifically.)
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
//!   `cfg`'d out of Android (and Windows) builds entirely, not just unused
//!   there.
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
use crate::db::dashboard::DashboardSummary;
use crate::db::full_report::FullReport;
use crate::db::reports::{CategorySalesReport, ProductSalesSummaryReport, RefundsSummary, TableSalesSummary};
use crate::db::refunds::Refund;
use crate::db::sales::Sale;
use crate::db::shifts::ShiftSummary;
use crate::printer::layout::{
    bordered_line, bordered_row, divider, double_divider, format_minor, row, truncate_line, two_col,
};
#[cfg(all(not(target_os = "android"), not(target_os = "windows")))]
use rusb::{Direction, TransferType};
#[cfg(all(not(target_os = "android"), not(target_os = "windows")))]
use std::time::Duration;

// Control bytes from the ESC/POS command reference:
// https://reference.epson-biz.com/modules/ref_escpos/index.php
const ESC: u8 = 0x1B;
const GS: u8 = 0x1D;

/// A handful of leading NUL bytes, sent before every real command below.
/// Several USB thermal printers swallow the very first byte written after
/// the bulk connection is opened while they finish waking from idle — for
/// most of a receipt's byte stream that's invisible (whatever got dropped
/// was mid-command), but when it's the very first byte overall it eats the
/// `ESC` of [`init`], and the printer treats the next byte — `init`'s `'@'`
/// (or `align_center`'s `'a'`, before `init` was added here at all) — as a
/// literal printable character instead of the second half of a command.
/// That's the root cause of the stray "@"/"a" seen at the very top of a
/// printed receipt, right where the logo/header begins: not a bug in the
/// logo raster encoding itself, but in what byte the printer was actually
/// awake to receive first. A NUL is silent if dropped the same way, so
/// sacrificing a few of them absorbs the swallowed byte instead of a real
/// command's.
fn wake_padding() -> [u8; 8] {
    [0; 8]
}

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

/// Lines to feed (via the cut command's own built-in feed, not manual
/// `\n`s) before the blade fires — enough that the last printed line is
/// well clear of the cut point rather than sitting right on it.
const CUT_FEED_LINES: u8 = 6;

fn feed_and_cut() -> [u8; 4] {
    // GS V m n — Function B: feed n lines, then cut, as one command. m=66
    // ('B') selects a partial cut (a small tab left uncut so the ticket
    // doesn't fall loose) with the pre-cut feed built in, so the printer
    // times the cut off its own feed instead of us guessing how many
    // manual '\n's are "enough". The previous version here —
    // `[b'\n', b'\n', b'\n', GS, b'V']` — both under-fed (3 lines is
    // borderline on some printers' feed-to-cut-blade distance) and, worse,
    // sent an incomplete `GS V` command with no `m` parameter at all: not
    // a valid ESC/POS command, so the cut point was left to whatever a
    // given printer's firmware happened to default an unterminated `GS V`
    // to, rather than one we actually chose.
    [GS, b'V', 66, CUT_FEED_LINES]
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

/// Item-row column widths shared by every line-item table below: Item(18) /
/// Qty(4) / Rate(8) / Amount(12) = 42 = `LINE_WIDTH`. A public const (not a
/// magic number repeated per template) so the header row and every line
/// stay in lockstep if this ever needs retuning for a differently-measured
/// printer — see `layout`'s module doc comment for how 42 was arrived at.
const ITEM_COLS: [usize; 4] = [18, 4, 8, 12];

fn item_row(desc: &str, qty: &str, rate: &str, amount: &str) -> String {
    row(&[(desc, ITEM_COLS[0], false), (qty, ITEM_COLS[1], true), (rate, ITEM_COLS[2], true), (amount, ITEM_COLS[3], true)])
}

/// Content-only column widths for the customer receipt's bordered item
/// table (below): Item(18) / Qty(3) / Rate(7) / Amount(9) = 37, plus the
/// 5 `|` separators [`bordered_row`]/[`bordered_line`] add = 42 =
/// `LINE_WIDTH` — same total budget as [`ITEM_COLS`] above, just split
/// differently to leave room for the border characters. Rate(7) exactly
/// fits "1300.00" with no padding; Amount(9) fits "3900.00" with 2 spaces
/// to spare — both come from `escpos::tests::
/// build_receipt_bytes_fits_the_measured_42_column_regression_row`, the
/// exact row a real client receipt showed wrapping mid-digit before this
/// column budget was corrected from an assumed 48 down to a measured 42.
const ITEM_TABLE_COLS: [usize; 4] = [18, 3, 7, 9];

fn item_table_row(desc: &str, qty: &str, rate: &str, amount: &str) -> String {
    bordered_row(&[
        (desc, ITEM_TABLE_COLS[0], false),
        (qty, ITEM_TABLE_COLS[1], true),
        (rate, ITEM_TABLE_COLS[2], true),
        (amount, ITEM_TABLE_COLS[3], true),
    ])
}

/// The header block every template shares: business name (bold, centered),
/// then `subtitle_lines` centered underneath it, then a blank line.
fn header_block(out: &mut Vec<u8>, business_name: &str, subtitle_lines: &[String]) {
    out.extend(align_center());
    out.extend(bold(true));
    out.extend(truncate_line(business_name).as_bytes());
    out.push(b'\n');
    out.extend(bold(false));
    for line in subtitle_lines {
        out.extend(truncate_line(line).as_bytes());
        out.push(b'\n');
    }
    out.push(b'\n');
    out.extend(align_left());
}

fn footer_block(out: &mut Vec<u8>, footer: &str) {
    if !footer.trim().is_empty() {
        out.push(b'\n');
        out.extend(align_center());
        out.extend(truncate_line(footer).as_bytes());
        out.push(b'\n');
    }
}

/// Content-only column widths for a generic 3-column bordered table —
/// "label / count / amount" — shared by the Category Wise Sale (Item / Qty
/// / Revenue) and Table Wise Sales (Table / Txns / Amount) reports: 22 + 6
/// + 10 = 38, plus the 4 `|` separators = 42 = `LINE_WIDTH`.
const THREE_COL_TABLE: [usize; 3] = [22, 6, 10];

fn three_col_row(a: &str, b: &str, c: &str) -> String {
    bordered_row(&[(a, THREE_COL_TABLE[0], false), (b, THREE_COL_TABLE[1], true), (c, THREE_COL_TABLE[2], true)])
}

fn three_col_border() -> String {
    bordered_line(&THREE_COL_TABLE)
}

/// Every template's closer: footer text (if any), then two blank lines of
/// margin *on top of* the cut command's own built-in feed
/// ([`CUT_FEED_LINES`]) — belt-and-suspenders so even a printer with a
/// shorter physical feed-to-blade distance than expected doesn't clip the
/// last printed line (the "Thank you"/footer, or the last totals row when
/// there's no footer configured), then the feed-and-cut command itself.
fn close_out(out: &mut Vec<u8>, footer: &str) {
    footer_block(out, footer);
    out.push(b'\n');
    out.push(b'\n');
    out.extend(feed_and_cut());
}

/// A prominent bold banner between double rules — "MERCHANT COPY" printed
/// right after the logo, before the business name, so the merchant's copy
/// is never mistaken for the customer's at a glance. Same ASCII
/// double-rule convention `double_divider` already uses elsewhere (above
/// the grand total) — no new visual language introduced just for this.
fn copy_label_banner(out: &mut Vec<u8>, label: &str) {
    out.extend(align_center());
    out.extend(double_divider().as_bytes());
    out.push(b'\n');
    out.extend(bold(true));
    out.extend(truncate_line(label).as_bytes());
    out.push(b'\n');
    out.extend(bold(false));
    out.extend(double_divider().as_bytes());
    out.push(b'\n');
    out.extend(align_left());
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
    build_receipt_bytes_for_copy(sale, config, logo, tables_module_enabled, None)
}

/// Same receipt as [`build_receipt_bytes`] — same header, business info,
/// logo, totals block, and cut margin — with two differences when
/// `copy_label` is `Some`: a bold banner (see [`copy_label_banner`]) appears
/// right after the logo, and the itemized item table is skipped entirely —
/// the merchant copy is a condensed totals-only slip, not a duplicate of
/// the full customer receipt, so it prints shorter and faster. This is the
/// *only* receipt template; [`print_receipt`] calls it twice (`None`, then
/// `Some("MERCHANT COPY")`), which is what guarantees the merchant copy can
/// never drift from the customer copy in shared content (header, totals
/// math, footer) or in any of the print-quality fixes (borders, wake
/// padding, cut feed) applied here — only the item table and banner differ,
/// both explicitly gated on `copy_label`.
fn build_receipt_bytes_for_copy(
    sale: &Sale,
    config: &AppConfig,
    logo: Option<&LogoRaster>,
    tables_module_enabled: bool,
    copy_label: Option<&str>,
) -> Vec<u8> {
    let mut out = Vec::new();
    let currency = &config.currency;

    out.extend(wake_padding());
    out.extend(init());

    if let Some(logo) = logo {
        print_logo(&mut out, logo);
    }

    if let Some(label) = copy_label {
        copy_label_banner(&mut out, label);
    }

    let mut subtitle = Vec::new();
    if let Some(phone) = &config.phone {
        if !phone.trim().is_empty() {
            subtitle.push(phone.clone());
        }
    }
    subtitle.push(format!("Sale #{}", sale.id));
    subtitle.push(sale.created_at.clone());
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

    // Merchant copy is a condensed totals-only slip — the itemized table is
    // customer-copy-only (see this fn's doc comment).
    if copy_label.is_none() {
        out.extend(bordered_line(&ITEM_TABLE_COLS).as_bytes());
        out.push(b'\n');
        out.extend(bold(true));
        out.extend(item_table_row("Item", "Qty", "Rate", "Amount").as_bytes());
        out.push(b'\n');
        out.extend(bold(false));
        out.extend(bordered_line(&ITEM_TABLE_COLS).as_bytes());
        out.push(b'\n');
        for item in &sale.items {
            out.extend(
                item_table_row(
                    &item.item_name,
                    &item.qty.to_string(),
                    &format_minor(item.price_at_sale_minor),
                    &format_minor(item.line_total_minor),
                )
                .as_bytes(),
            );
            out.push(b'\n');
        }
        out.extend(bordered_line(&ITEM_TABLE_COLS).as_bytes());
        out.push(b'\n');
    }

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
    out.extend(truncate_line(&format!("Paid by: {}", sale.payment_method)).as_bytes());
    out.push(b'\n');

    close_out(&mut out, &config.receipt_footer);
    out
}

/// The "Refund Details" receipt: Vno (original sale id) / item / amount
/// lines, Total Refund at the bottom — same header/footer/divider
/// conventions as the customer receipt, built from `Refund` (see
/// `db::refunds::create_refund`) rather than `Sale`.
pub fn build_refund_bytes(refund: &Refund, config: &AppConfig) -> Vec<u8> {
    let mut out = Vec::new();
    let currency = &config.currency;

    out.extend(wake_padding());
    out.extend(init());

    let mut subtitle = Vec::new();
    if let Some(phone) = &config.phone {
        if !phone.trim().is_empty() {
            subtitle.push(phone.clone());
        }
    }
    subtitle.push("REFUND".to_string());
    subtitle.push(format!("Refund #{}", refund.id));
    subtitle.push(format!("Vno: {}", refund.original_sale_id));
    subtitle.push(refund.created_at.clone());
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
        out.extend(truncate_line(&format!("Reason: {}", reason)).as_bytes());
        out.push(b'\n');
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

    close_out(&mut out, &config.receipt_footer);
    out
}

/// The "Counter-N Sale Details" shift close-out receipt: opening balance,
/// cash/card/credit/other sale, total sale, declared amount, and the
/// Short/Over difference.
pub fn build_shift_summary_bytes(summary: &ShiftSummary, config: &AppConfig) -> Vec<u8> {
    let mut out = Vec::new();
    let currency = &config.currency;
    let money = |m: i64| format!("{} {}", currency, format_minor(m));

    out.extend(wake_padding());
    out.extend(init());

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

    close_out(&mut out, &config.receipt_footer);
    out
}

/// The "Category Wise Sale" report: one section per category (bold header,
/// item rows, subtotal), grand total at the end.
pub fn build_category_sales_bytes(report: &CategorySalesReport, config: &AppConfig) -> Vec<u8> {
    let mut out = Vec::new();

    out.extend(wake_padding());
    out.extend(init());

    let subtitle =
        vec!["Category Wise Sale".to_string(), format!("{} to {}", report.start_date, report.end_date)];
    header_block(&mut out, &config.business_name, &subtitle);

    write_category_sales_section(&mut out, report, &config.currency);

    close_out(&mut out, &config.receipt_footer);
    out
}

/// One category-per-group bordered breakdown (bold category name, a
/// `three_col_row` table of its items, a subtotal) followed by the grand
/// total rule — the body [`build_category_sales_bytes`] wraps in its own
/// header/footer, and [`build_full_report_bytes`] reuses verbatim under its
/// own "CATEGORY WISE SALE" section heading so the two never drift apart.
fn write_category_sales_section(out: &mut Vec<u8>, report: &CategorySalesReport, currency: &str) {
    for group in &report.groups {
        out.extend(bold(true));
        out.extend(truncate_line(&group.category_name).as_bytes());
        out.push(b'\n');
        out.extend(bold(false));

        out.extend(three_col_border().as_bytes());
        out.push(b'\n');
        out.extend(bold(true));
        out.extend(three_col_row("Item", "Qty", "Revenue").as_bytes());
        out.push(b'\n');
        out.extend(bold(false));
        out.extend(three_col_border().as_bytes());
        out.push(b'\n');
        for item in &group.items {
            out.extend(
                three_col_row(&item.item_name, &item.qty_sold.to_string(), &format_minor(item.revenue_minor))
                    .as_bytes(),
            );
            out.push(b'\n');
        }
        out.extend(three_col_border().as_bytes());
        out.push(b'\n');

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
}

/// The Table Wise Sales report: one row per table (plus "Counter /
/// Takeaway"), grand total at the end — the same flat list-with-grand-total
/// shape as the reference receipt, built from `TableSalesSummary` (see
/// `db::reports::get_table_sales_summary`).
pub fn build_table_sales_bytes(report: &TableSalesSummary, config: &AppConfig) -> Vec<u8> {
    let mut out = Vec::new();

    out.extend(wake_padding());
    out.extend(init());

    let subtitle =
        vec!["Table Wise Sales".to_string(), format!("{} to {}", report.start_date, report.end_date)];
    header_block(&mut out, &config.business_name, &subtitle);

    write_table_sales_section(&mut out, report, &config.currency);

    close_out(&mut out, &config.receipt_footer);
    out
}

/// The bordered Table/Txns/Amount grid plus grand-total rule — reused by
/// both [`build_table_sales_bytes`] and [`build_full_report_bytes`], same
/// reasoning as [`write_category_sales_section`].
fn write_table_sales_section(out: &mut Vec<u8>, report: &TableSalesSummary, currency: &str) {
    out.extend(three_col_border().as_bytes());
    out.push(b'\n');
    out.extend(bold(true));
    out.extend(three_col_row("Table / Counter", "Txns", "Amount").as_bytes());
    out.push(b'\n');
    out.extend(bold(false));
    out.extend(three_col_border().as_bytes());
    out.push(b'\n');
    for line in &report.rows {
        out.extend(
            three_col_row(&line.label, &line.transaction_count.to_string(), &format_minor(line.total_minor))
                .as_bytes(),
        );
        out.push(b'\n');
    }
    out.extend(three_col_border().as_bytes());
    out.push(b'\n');

    out.extend(double_divider().as_bytes());
    out.push(b'\n');
    out.extend(bold(true));
    out.extend(
        two_col("GRAND TOTAL", &format!("{} {}", currency, format_minor(report.grand_total_minor))).as_bytes(),
    );
    out.push(b'\n');
    out.extend(bold(false));
}

/// The Refunds report: one block per refund — Vno (original sale id),
/// timestamp, who processed it, a bordered Item/Qty/Amount grid (the same
/// 3-column border style the other report sections use), the reason if one
/// was given, and that refund's own subtotal — followed by a grand total
/// across every refund in the range. This is the itemized "Vno/Details/
/// Amount rows, Total Refund at the bottom" shape the single-refund receipt
/// (`build_refund_bytes`) already uses, just repeated for every refund in
/// the period instead of one. Reused by both [`build_refunds_summary_bytes`]
/// and [`build_full_report_bytes`], same reasoning as
/// [`write_table_sales_section`].
fn write_refunds_section(out: &mut Vec<u8>, report: &RefundsSummary, currency: &str) {
    if report.refunds.is_empty() {
        out.extend(b"No refunds in this period\n");
        return;
    }

    for (i, refund) in report.refunds.iter().enumerate() {
        if i > 0 {
            out.push(b'\n');
        }

        out.extend(bold(true));
        out.extend(truncate_line(&format!("Refund #{}  Vno: {}", refund.id, refund.original_sale_id)).as_bytes());
        out.push(b'\n');
        out.extend(bold(false));
        out.extend(truncate_line(&refund.created_at).as_bytes());
        out.push(b'\n');
        if let Some(name) = &refund.refunded_by_name {
            out.extend(truncate_line(&format!("By: {}", name)).as_bytes());
            out.push(b'\n');
        }

        out.extend(three_col_border().as_bytes());
        out.push(b'\n');
        out.extend(bold(true));
        out.extend(three_col_row("Item", "Qty", "Amount").as_bytes());
        out.push(b'\n');
        out.extend(bold(false));
        out.extend(three_col_border().as_bytes());
        out.push(b'\n');
        for item in &refund.items {
            out.extend(
                three_col_row(&item.item_name, &item.qty_refunded.to_string(), &format_minor(item.amount_refunded_minor))
                    .as_bytes(),
            );
            out.push(b'\n');
        }
        out.extend(three_col_border().as_bytes());
        out.push(b'\n');

        if let Some(reason) = &refund.reason {
            out.extend(truncate_line(&format!("Reason: {}", reason)).as_bytes());
            out.push(b'\n');
        }

        out.extend(
            two_col("Refund Total", &format!("{} {}", currency, format_minor(refund.total_refund_amount_minor)))
                .as_bytes(),
        );
        out.push(b'\n');
    }

    out.push(b'\n');
    out.extend(double_divider().as_bytes());
    out.push(b'\n');
    out.extend(bold(true));
    out.extend(
        two_col("TOTAL REFUNDED", &format!("{} {}", currency, format_minor(report.grand_total_refunded_minor)))
            .as_bytes(),
    );
    out.push(b'\n');
    out.extend(bold(false));
}

/// The standalone "Refunds Report" thermal print/PDF — every refund in the
/// range via [`write_refunds_section`], with its own header/footer.
pub fn build_refunds_summary_bytes(report: &RefundsSummary, config: &AppConfig) -> Vec<u8> {
    let mut out = Vec::new();

    out.extend(wake_padding());
    out.extend(init());

    let subtitle = vec!["Refunds Report".to_string(), format!("{} to {}", report.start_date, report.end_date)];
    header_block(&mut out, &config.business_name, &subtitle);

    write_refunds_section(&mut out, report, &config.currency);

    close_out(&mut out, &config.receipt_footer);
    out
}

/// The Product Wise Sales bordered grid — rank-prefixed item name / qty /
/// revenue, same 3-column border style as the other two sections. Only
/// used inside [`build_full_report_bytes`] — Product Wise Sales has no
/// standalone thermal-print button of its own (PDF/CSV only), same as it
/// is on-screen.
fn write_product_sales_section(out: &mut Vec<u8>, report: &ProductSalesSummaryReport) {
    out.extend(three_col_border().as_bytes());
    out.push(b'\n');
    out.extend(bold(true));
    out.extend(three_col_row("Item", "Qty", "Revenue").as_bytes());
    out.push(b'\n');
    out.extend(bold(false));
    out.extend(three_col_border().as_bytes());
    out.push(b'\n');
    for item in &report.rows {
        let label = format!("{}. {}", item.rank, item.item_name);
        out.extend(three_col_row(&label, &item.qty_sold.to_string(), &format_minor(item.revenue_minor)).as_bytes());
        out.push(b'\n');
    }
    out.extend(three_col_border().as_bytes());
    out.push(b'\n');
}

/// The Overview section: total sales, refunds, transactions, average sale,
/// then expenses/salary paid (only the ones tracked for this platform —
/// same `Option` convention `DashboardSummary` uses), then net profit
/// under a double rule — the same figures and the same math the Dashboard
/// screen shows, via [`DashboardSummary`], not recomputed here.
fn write_overview_section(out: &mut Vec<u8>, overview: &DashboardSummary, average_sale_minor: i64, currency: &str) {
    let money = |m: i64| format!("{} {}", currency, format_minor(m));

    out.extend(two_col("Total Sales", &money(overview.total_sales_minor)).as_bytes());
    out.push(b'\n');
    out.extend(two_col("Refunds", &money(overview.refunds_minor)).as_bytes());
    out.push(b'\n');
    out.extend(two_col("Transactions", &overview.transaction_count.to_string()).as_bytes());
    out.push(b'\n');
    out.extend(two_col("Average Sale", &money(average_sale_minor)).as_bytes());
    out.push(b'\n');
    if let Some(expenses) = overview.total_expenses_minor {
        out.extend(two_col("Expenses", &money(expenses)).as_bytes());
        out.push(b'\n');
    }
    if let Some(salary) = overview.total_salary_paid_minor {
        out.extend(two_col("Salary Paid", &money(salary)).as_bytes());
        out.push(b'\n');
    }

    out.extend(double_divider().as_bytes());
    out.push(b'\n');
    out.extend(bold(true));
    out.extend(two_col("NET PROFIT", &money(overview.net_profit_minor)).as_bytes());
    out.push(b'\n');
    out.extend(bold(false));
}

/// A bold, left-aligned section banner — "OVERVIEW", "CATEGORY WISE SALE",
/// etc. — separating [`build_full_report_bytes`]'s sections from each other.
fn section_banner(out: &mut Vec<u8>, title: &str) {
    out.extend(bold(true));
    out.extend(truncate_line(title).as_bytes());
    out.push(b'\n');
    out.extend(bold(false));
    out.extend(divider().as_bytes());
    out.push(b'\n');
}

/// Builds the "Generate Full Report" consolidated document: Overview
/// (including net profit), Refunds, Category Wise Sale, Product Wise
/// Sales, and — when the `tables` module was enabled at the time this
/// [`FullReport`] was assembled (see `db::full_report::get_full_report`) —
/// Table Wise Sales, all for one date range in a single print job.
/// Distinct from the individual per-report prints above: "everything for
/// this period, one document," not a stand-in for them.
pub fn build_full_report_bytes(report: &FullReport, config: &AppConfig) -> Vec<u8> {
    let mut out = Vec::new();
    let currency = &config.currency;

    out.extend(wake_padding());
    out.extend(init());

    header_block(
        &mut out,
        &config.business_name,
        &["Full Sales Report".to_string(), format!("{} to {}", report.start_date, report.end_date)],
    );

    section_banner(&mut out, "OVERVIEW");
    write_overview_section(&mut out, &report.overview, report.average_sale_minor, currency);
    out.push(b'\n');
    out.push(b'\n');

    section_banner(&mut out, "REFUNDS");
    write_refunds_section(&mut out, &report.refunds, currency);
    out.push(b'\n');
    out.push(b'\n');

    section_banner(&mut out, "CATEGORY WISE SALE");
    write_category_sales_section(&mut out, &report.category_sales, currency);
    out.push(b'\n');
    out.push(b'\n');

    section_banner(&mut out, "PRODUCT WISE SALES");
    write_product_sales_section(&mut out, &report.product_sales);
    out.push(b'\n');
    out.push(b'\n');

    if let Some(table_sales) = &report.table_sales {
        section_banner(&mut out, "TABLE WISE SALES");
        write_table_sales_section(&mut out, table_sales, currency);
        out.push(b'\n');
        out.push(b'\n');
    }

    close_out(&mut out, &config.receipt_footer);
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
    #[cfg(target_os = "windows")]
    {
        match config.printer_connection_type.as_deref() {
            Some("windows") => {
                let name = config.printer_windows_name.as_deref().ok_or(PrinterError::NoPrinterSelected)?;
                crate::printer::windows_spool::send(name, bytes)
            }
            // Same reasoning as Android's `_` arm above: nothing chosen yet
            // in Settings, not a hardware failure. Windows deliberately
            // never falls back to auto-detected raw USB here — see this
            // module's doc comment for why that never reliably found an
            // installed printer in the first place.
            _ => Err(PrinterError::NoPrinterSelected),
        }
    }
    #[cfg(all(not(target_os = "android"), not(target_os = "windows")))]
    {
        let _ = config; // macOS/Linux's USB auto-detect doesn't need a stored selection (see module doc)
        send_to_printer_usb(bytes)
    }
}

/// The standard USB device-class code for printers (USB.org base class
/// 0x07) — this is what lets a compatible printer be found without any
/// vendor/product ID configuration.
#[cfg(all(not(target_os = "android"), not(target_os = "windows")))]
const USB_PRINTER_CLASS: u8 = 0x07;

/// How long to wait for the printer to accept the write before giving up.
/// A receipt is small (well under 1 KB of ESC/POS bytes), so this only
/// needs to cover a slow/busy printer, not a large transfer.
#[cfg(all(not(target_os = "android"), not(target_os = "windows")))]
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
/// macOS/Linux only — see this module's doc comment and `Cargo.toml` for
/// why this (and the `rusb` dependency it needs) is `cfg`'d out of Android
/// and Windows entirely; `windows_spool` is Windows' equivalent.
#[cfg(all(not(target_os = "android"), not(target_os = "windows")))]
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

/// Builds and sends **two** receipts for `sale`, back to back through the
/// same printer in one write: the customer copy unchanged, immediately
/// followed by a merchant copy carrying the "MERCHANT COPY" banner (see
/// [`build_receipt_bytes_for_copy`]). Concatenating two complete,
/// independently-cut byte streams — rather than printing twice via two
/// separate calls — is what makes "back to back" literal: each copy still
/// ends with its own full feed-and-cut command, so the cutter fires once
/// per copy while the whole job goes out as a single transport write.
/// Every fix applied to the shared builder (bordered item table, wake
/// padding, pre-cut feed) automatically covers both copies, since both are
/// built by the exact same function.
pub fn print_receipt(
    sale: &Sale,
    config: &AppConfig,
    logo: Option<&LogoRaster>,
    tables_module_enabled: bool,
) -> Result<(), PrinterError> {
    let mut bytes = build_receipt_bytes_for_copy(sale, config, logo, tables_module_enabled, None);
    bytes.extend(build_receipt_bytes_for_copy(sale, config, logo, tables_module_enabled, Some("MERCHANT COPY")));
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

/// Builds and prints the Refunds report.
pub fn print_refunds_summary(report: &RefundsSummary, config: &AppConfig) -> Result<(), PrinterError> {
    let bytes = build_refunds_summary_bytes(report, config);
    send_to_printer(&bytes, config)
}

/// Builds and prints the consolidated Full Report.
pub fn print_full_report(report: &FullReport, config: &AppConfig) -> Result<(), PrinterError> {
    let bytes = build_full_report_bytes(report, config);
    send_to_printer(&bytes, config)
}

/// Candidate line widths the diagnostic print checks — spans comfortably
/// either side of [`layout::LINE_WIDTH`] (42) so a printer that turns out
/// to genuinely be a 32-column or a 48-column model still shows a clean
/// line somewhere in the list, not just a wall of wrapped ones.
const DIAGNOSTIC_WIDTHS: [usize; 7] = [28, 32, 36, 40, 42, 44, 48];

/// A printable ruler + a row of digits at each width in [`DIAGNOSTIC_WIDTHS`]
/// — exists because this file's `LINE_WIDTH` has already been wrong once
/// (48, the Epson TM-series datasheet number) and been corrected once (42,
/// measured off a real client receipt); if a *third* printer needs a *third*
/// number, this prints the evidence directly rather than inferring it from
/// a photo of a wrapped row again. Read it off the physical paper: every
/// "N=.." line up to and including the widest one that does NOT wrap is a
/// candidate for the real `LINE_WIDTH`; the first one that *does* wrap
/// confirms the ceiling. `printer_print_diagnostic` (`commands.rs`) is the
/// command Settings' printer section calls to send this.
fn build_diagnostic_bytes() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(wake_padding());
    out.extend(init());
    out.extend(align_left());

    out.extend(bold(true));
    out.extend(b"DIWAN PRINTER WIDTH TEST\n");
    out.extend(bold(false));
    out.extend(b"Read the widest line below that\n");
    out.extend(b"prints on ONE physical line only.\n\n");

    let ruler: String = (0..60).map(|i| std::char::from_digit((i % 10) as u32, 10).unwrap()).collect();
    out.extend(ruler.as_bytes());
    out.push(b'\n');
    out.push(b'\n');

    for width in DIAGNOSTIC_WIDTHS {
        out.extend(format!("N={width}\n").as_bytes());
        let fill: String = "1234567890".chars().cycle().take(width).collect();
        out.extend(fill.as_bytes());
        out.push(b'\n');
        out.push(b'\n');
    }

    out.extend(feed_and_cut());
    out
}

/// Sends the diagnostic print to whatever printer transport is currently
/// configured — same dispatch every real receipt goes through
/// ([`send_to_printer`]), so this measures the actual print path, not a
/// simulation of it.
pub fn print_diagnostic(config: &AppConfig) -> Result<(), PrinterError> {
    send_to_printer(&build_diagnostic_bytes(), config)
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
            phone: None,
            working_days_per_month: 26,
            onboarding_completed: true,
            printer_connection_type: None,
            printer_bluetooth_address: None,
            printer_bluetooth_name: None,
            printer_windows_name: None,
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

    fn sample_refunds_summary() -> RefundsSummary {
        RefundsSummary {
            start_date: "2026-01-01".into(),
            end_date: "2026-01-31".into(),
            refunds: vec![sample_refund()],
            grand_total_refunded_minor: 500,
        }
    }

    #[test]
    fn build_refunds_summary_bytes_includes_vno_items_reason_and_grand_total() {
        let bytes = build_refunds_summary_bytes(&sample_refunds_summary(), &sample_config());
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("Refunds Report"));
        assert!(text.contains("Vno: 42"), "original sale id must appear as Vno: {text}");
        assert!(text.contains("Cola 500ml"));
        assert!(text.contains("Customer changed mind"));
        assert!(text.contains("Owner"), "processed-by name must appear: {text}");
        assert!(text.contains("TOTAL REFUNDED"));
        assert!(text.contains("PKR 5.00"));
        assert!(text.contains(three_col_border().as_str()), "expected a bordered item grid: {text}");
        assert!(text.contains("|Cola 500ml"), "item row must be pipe-bordered: {text}");
    }

    #[test]
    fn build_refunds_summary_bytes_reports_no_refunds_cleanly() {
        let mut report = sample_refunds_summary();
        report.refunds = vec![];
        report.grand_total_refunded_minor = 0;
        let bytes = build_refunds_summary_bytes(&report, &sample_config());
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("No refunds in this period"));
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
        // Same "not just floating text" bar as the receipt's item table —
        // a real `+---+` grid, not bare dashes.
        assert!(text.contains(three_col_border().as_str()), "expected a bordered grid: {text}");
        assert!(text.contains("|Table 1"), "row must be pipe-bordered: {text}");
    }

    #[test]
    fn build_receipt_bytes_ends_with_a_complete_feed_and_cut_command() {
        let bytes = build_receipt_bytes(&sample_sale(), &sample_config(), None, true);
        // GS V m n — a complete Function B cut command, not the previous
        // `[.., GS, b'V']` with no `m`/`n` at all, which left the printer
        // to guess a cut point rather than us choosing one.
        assert_eq!(
            &bytes[bytes.len() - 4..],
            &[GS, b'V', 66, CUT_FEED_LINES],
            "receipt must end with a complete GS V feed-and-cut command"
        );
    }

    #[test]
    fn build_receipt_bytes_starts_with_wake_padding_then_a_real_init() {
        let bytes = build_receipt_bytes(&sample_sale(), &sample_config(), None, true);
        // NULs absorb a printer swallowing the very first byte off the
        // wire; ESC '@' (init) must still be intact right after them, not
        // itself be the byte that gets eaten.
        assert!(bytes.starts_with(&[0; 8]), "receipt must lead with NUL wake padding");
        assert_eq!(&bytes[8..10], &[ESC, b'@'], "init must immediately follow the wake padding");
    }

    #[test]
    fn build_receipt_bytes_never_shows_a_merchant_copy_banner() {
        let bytes = build_receipt_bytes(&sample_sale(), &sample_config(), None, true);
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains("MERCHANT COPY"), "the plain customer-copy builder must stay unlabeled: {text}");
    }

    #[test]
    fn build_receipt_bytes_for_copy_shows_the_merchant_copy_banner_when_labeled() {
        let bytes = build_receipt_bytes_for_copy(&sample_sale(), &sample_config(), None, true, Some("MERCHANT COPY"));
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("MERCHANT COPY"), "expected the banner: {text}");
        // Condensed: totals shared with the customer copy, but the
        // itemized table is intentionally dropped — see this fn's doc
        // comment.
        assert!(text.contains("TOTAL"));
        assert!(!text.contains("Cola 500ml"), "merchant copy must not list line items: {text}");
    }

    #[test]
    fn print_receipt_prints_two_copies_back_to_back_each_with_their_own_cut() {
        // Reproduces print_receipt's own byte assembly — print_receipt
        // itself isn't unit-testable without a real printer attached
        // (send_to_printer needs hardware) — to prove the *shape* of what
        // it actually sends: two complete, independently bordered/cut
        // receipts concatenated into one transport write, second one
        // merchant-labeled.
        let sale = sample_sale();
        let config = sample_config();
        let customer = build_receipt_bytes_for_copy(&sale, &config, None, true, None);
        let merchant = build_receipt_bytes_for_copy(&sale, &config, None, true, Some("MERCHANT COPY"));
        let mut combined = customer.clone();
        combined.extend(merchant.clone());

        // One feed-and-cut command per copy, not one for the whole job —
        // this is what makes it two receipts, not one long one.
        let cut = [GS, b'V', 66, CUT_FEED_LINES];
        let cut_count = combined.windows(cut.len()).filter(|w| *w == cut).count();
        assert_eq!(cut_count, 2, "each copy must end with its own feed-and-cut command");

        assert!(!String::from_utf8_lossy(&customer).contains("MERCHANT COPY"), "first copy must be the plain customer copy");
        assert!(String::from_utf8_lossy(&merchant).contains("MERCHANT COPY"), "second copy must carry the banner");

        // The customer copy keeps the recent print-quality fixes —
        // bordered item table and full item content; the merchant copy is
        // deliberately condensed (totals only, no item table) — see
        // `build_receipt_bytes_for_copy`'s doc comment.
        let combined_text = String::from_utf8_lossy(&combined);
        assert_eq!(combined_text.matches("Cola 500ml").count(), 1, "only the customer copy lists items");
        assert_eq!(
            combined_text.matches(bordered_line(&ITEM_TABLE_COLS).as_str()).count(),
            3,
            "only the customer copy draws the bordered item table (3 border lines)"
        );
    }

    #[test]
    fn build_receipt_bytes_never_lets_a_long_business_name_or_phone_spill_onto_another_line() {
        use crate::printer::layout::{truncate_line, LINE_WIDTH};

        let long_name = "A Very Long Business Name That Would Otherwise Wrap Onto A Second Physical Line";
        let long_phone = "+92 300 1234567 (also a needlessly long phone line to test with)";
        assert!(long_name.len() > LINE_WIDTH && long_phone.len() > LINE_WIDTH, "fixture must actually be too long");

        let mut config = sample_config();
        config.business_name = long_name.into();
        config.phone = Some(long_phone.into());

        let bytes = build_receipt_bytes(&sample_sale(), &config, None, true);
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains(long_name), "the untruncated business name must not appear at all");
        assert!(!text.contains(long_phone), "the untruncated phone must not appear at all");
        assert!(text.contains(&truncate_line(long_name)), "the name must still appear, cut to one line's width");
        assert!(text.contains(&truncate_line(long_phone)), "the phone must still appear, cut to one line's width");
    }

    #[test]
    fn build_refund_bytes_never_lets_a_long_reason_spill_onto_another_line() {
        use crate::printer::layout::{truncate_line, LINE_WIDTH};

        let long_reason = "A very long refund reason that would otherwise wrap onto a second physical line";
        assert!(long_reason.len() > LINE_WIDTH, "fixture must actually be too long");

        let mut refund = sample_refund();
        refund.reason = Some(long_reason.into());

        let bytes = build_refund_bytes(&refund, &sample_config());
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains(long_reason), "the untruncated reason must not appear at all");
        assert!(
            text.contains(&truncate_line(&format!("Reason: {long_reason}"))),
            "the reason must still appear, cut to one line's width"
        );
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

    /// The exact row a real client receipt showed physically wrapping
    /// mid-digit — `3900.00` split into `39` / `00.00` — proving the
    /// printer's true width was 42 columns, not the 48 this file used to
    /// assume. Char-count math, spelled out explicitly per-column rather
    /// than trusted to just add up:
    ///   - desc:   "mutton paye (full)" is exactly 18 chars,
    ///     ITEM_TABLE_COLS[0]=18 → exact fit, 0 padding, nothing truncated.
    ///   - qty:    "3" is 1 char, right-padded into 3 → "  3".
    ///   - rate:   "1300.00" is exactly 7 chars, ITEM_TABLE_COLS[2]=7 →
    ///     0 padding, exact fit.
    ///   - amount: "3900.00" is exactly 7 chars, ITEM_TABLE_COLS[3]=9 →
    ///     2 spaces padding, fits with room to spare.
    ///   - total printed row: 1 (`|`) + 18 + 1 (`|`) + 3 + 1 (`|`) + 7 +
    ///     1 (`|`) + 9 + 1 (`|`) = 42 = `LINE_WIDTH` exactly — the real,
    ///     measured printer width. The exact string is asserted below
    ///     rather than just its length, so this can't silently drift; every
    ///     substring's length is verified independently in the test body
    ///     too, not just eyeballed in this comment.
    #[test]
    fn build_diagnostic_bytes_contains_a_fill_line_of_exactly_each_candidate_width() {
        let bytes = build_diagnostic_bytes();
        let text = String::from_utf8_lossy(&bytes);
        for width in DIAGNOSTIC_WIDTHS {
            assert!(text.contains(&format!("N={width}")), "missing the N={width} label");
            let expected_fill: String = "1234567890".chars().cycle().take(width).collect();
            assert!(
                text.contains(&expected_fill),
                "missing a fill line of exactly {width} characters for N={width}"
            );
        }
    }

    #[test]
    fn build_receipt_bytes_fits_the_measured_42_column_regression_row() {
        use crate::printer::layout::LINE_WIDTH;
        assert_eq!(LINE_WIDTH, 42, "this test's math is only valid against the measured 42-column width");
        assert_eq!("mutton paye (full)".chars().count(), 18);
        assert_eq!("1300.00".chars().count(), 7);
        assert_eq!("3900.00".chars().count(), 7);

        let row = item_table_row("mutton paye (full)", "3", "1300.00", "3900.00");
        assert_eq!(row.chars().count(), LINE_WIDTH, "the row itself must be exactly one physical printed line");
        assert_eq!(row, "|mutton paye (full)|  3|1300.00|  3900.00|");
    }

    #[test]
    fn build_receipt_bytes_lines_up_the_item_table_columns() {
        use crate::printer::layout::LINE_WIDTH;
        let bytes = build_receipt_bytes(&sample_sale(), &sample_config(), None, true);
        let text = String::from_utf8_lossy(&bytes);
        // Every item row (and the header) must be exactly LINE_WIDTH
        // characters, and bordered on both sides with `|` — that's what
        // "columns line up" with visible grid lines actually verifies, not
        // just that the right substrings appear somewhere.
        for line in text.lines() {
            if line.contains("Cola 500ml") || line.contains("|Item") {
                // Strip any bold-toggle control bytes a formatting command
                // may have left at the start of this line (no visible ink
                // on the real printer, but real characters in this lossy
                // string) — the pipe border is what should actually open
                // and close the visible row.
                let visible = &line[line.find('|').expect("row must be pipe-bordered")..];
                assert_eq!(visible.chars().count(), LINE_WIDTH, "misaligned row: {:?}", visible);
                assert!(visible.starts_with('|') && visible.ends_with('|'), "row must be pipe-bordered: {:?}", visible);
            }
        }
    }

    #[test]
    fn build_receipt_bytes_draws_a_bordered_item_table() {
        let bytes = build_receipt_bytes(&sample_sale(), &sample_config(), None, true);
        let text = String::from_utf8_lossy(&bytes);
        // A real ASCII grid — `+---+` rules top/bottom of the header and
        // bottom of the last item, `|` column dividers on every row in
        // between — not just floating text, since this has to read as a
        // table on thermal paper with no CSS to fall back on. Matched as a
        // substring (not per-line) since a bold-toggle command's raw bytes
        // can sit right before a border line with no `\n` between them —
        // invisible ink-wise on the real printer, but it would break a
        // naive `line.starts_with('+')` check.
        let expected_border = bordered_line(&ITEM_TABLE_COLS);
        let border_count = text.matches(expected_border.as_str()).count();
        assert!(border_count >= 3, "expected top/header/bottom border rules, got: {text}");
        assert!(text.contains("|Item"), "header row must be pipe-bordered: {text}");
        assert!(text.contains("|Cola 500ml"), "item row must be pipe-bordered: {text}");
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
        assert!(text.contains(three_col_border().as_str()), "expected a bordered grid: {text}");
        assert!(text.contains("|Cola 500ml"), "row must be pipe-bordered: {text}");
    }

    fn sample_product_sales() -> ProductSalesSummaryReport {
        use crate::db::reports::ProductSalesRow;
        ProductSalesSummaryReport {
            start_date: "2026-01-01".into(),
            end_date: "2026-01-31".into(),
            sort_by: crate::db::reports::TopItemSort::Revenue,
            rows: vec![ProductSalesRow {
                item_id: 1,
                item_name: "Cola 500ml".into(),
                category_id: Some(1),
                category_name: "Beverages".into(),
                qty_sold: 10,
                revenue_minor: 80_000,
                rank: 1,
            }],
            no_sales_items: vec![],
        }
    }

    fn sample_full_report() -> FullReport {
        use crate::db::dashboard::DashboardSummary;
        FullReport {
            start_date: "2026-01-01".into(),
            end_date: "2026-01-31".into(),
            overview: DashboardSummary {
                start_date: "2026-01-01".into(),
                end_date: "2026-01-31".into(),
                total_sales_minor: 80_000,
                refunds_minor: 0,
                transaction_count: 10,
                total_expenses_minor: Some(5_000),
                total_salary_paid_minor: None,
                net_profit_minor: 75_000,
                low_stock_item_count: None,
                top_table_by_sales: None,
            },
            average_sale_minor: 8_000,
            refunds: sample_refunds_summary(),
            category_sales: sample_category_report(),
            product_sales: sample_product_sales(),
            table_sales: Some(sample_table_sales()),
        }
    }

    #[test]
    fn build_full_report_bytes_includes_every_section() {
        let bytes = build_full_report_bytes(&sample_full_report(), &sample_config());
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("Full Sales Report"));
        assert!(text.contains("OVERVIEW"));
        assert!(text.contains("NET PROFIT"));
        assert!(text.contains("PKR 750.00"), "net profit must appear formatted: {text}");
        assert!(text.contains("REFUNDS"));
        assert!(text.contains("TOTAL REFUNDED"));
        assert!(text.contains("CATEGORY WISE SALE"));
        assert!(text.contains("Beverages"));
        assert!(text.contains("PRODUCT WISE SALES"));
        assert!(text.contains("1. Cola 500ml"), "product row must be rank-prefixed: {text}");
        assert!(text.contains("TABLE WISE SALES"));
        assert!(text.contains("Table 1"));
        assert!(text.contains(three_col_border().as_str()), "expected bordered grids: {text}");
    }

    #[test]
    fn build_full_report_bytes_omits_table_wise_sales_when_tables_is_disabled() {
        let mut report = sample_full_report();
        report.table_sales = None;
        let bytes = build_full_report_bytes(&report, &sample_config());
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains("TABLE WISE SALES"), "must not show a table section with no data: {text}");
    }

    #[test]
    fn build_full_report_bytes_ends_with_a_complete_feed_and_cut_command() {
        let bytes = build_full_report_bytes(&sample_full_report(), &sample_config());
        assert_eq!(&bytes[bytes.len() - 4..], &[GS, b'V', 66, CUT_FEED_LINES]);
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
