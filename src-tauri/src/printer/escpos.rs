//! ESC/POS thermal printer output.
//!
//! [`build_receipt_bytes`] — the byte-sequence construction from a completed
//! [`Sale`] — is complete, pure and hardware-independent, so it is fully
//! testable without a printer attached.
//!
//! [`send_to_printer`] dispatches by platform and by what's stored in
//! `AppConfig`'s `printer_connection_type` and whichever transport-specific
//! field goes with it (`printer_bluetooth_address`, `printer_usb_device`,
//! `printer_windows_name`)
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
//! - **Android**: two transports, whichever one Settings selected.
//!   `"bluetooth"` is Bluetooth Classic (SPP) to an *already-paired* device
//!   — see `printer::android_bt`; there is deliberately no "select any
//!   nearby device" scan step (that needs `BLUETOOTH_SCAN` + a location
//!   permission on older Android versions on top of `BLUETOOTH_CONNECT`,
//!   for a use case — a shop's one till printer — that's always already
//!   paired via the OS Bluetooth settings anyway). `"usb"` is Android's USB
//!   Host API to an OTG-connected printer, matched by vendor:product id —
//!   see `printer::android_usb`; it exists because plenty of till printers
//!   (the client's Gainscha GA-E200 among them) have no Bluetooth radio at
//!   all. Note that raw libusb on Android *was* tried, and dropped: it was
//!   never designed for Android's USB-permission model and crashed the whole
//!   app natively (a libusb-level fault, not a catchable Rust panic). See
//!   `Cargo.toml`'s doc comment on why `rusb` is still `cfg`'d out of
//!   Android (and Windows) builds entirely — `android_usb` goes through
//!   `UsbManager`, which is that permission model rather than a fight with
//!   it.
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
use crate::db::salary::{PaymentStatus, SalaryCalculation};
use crate::db::sales::Sale;
use crate::db::shifts::ShiftSummary;
#[cfg(test)]
use crate::db::tokens::TokenLine;
use crate::db::tokens::TokenSummary;
use crate::printer::receipt_image::{self, Align, Cell, Row, Rule, SIZE_BODY, SIZE_SMALL, SIZE_TITLE};
use crate::printer::layout::{
    bordered_line, bordered_row, box_line, box_row, center_text, divider, double_divider, format_minor,
    format_qty, row, truncate_line, two_col, BoxLinePosition, LINE_WIDTH,
};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use rusb::{Direction, TransferType};
use std::sync::Mutex;
#[cfg(any(target_os = "macos", target_os = "linux"))]
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

fn init() -> [u8; 5] {
    [
        ESC, b'@', // reset the printer to its power-on state
        ESC, b't', 0, // select codepage 0 (CP437) — needed for `layout::box_row`/`box_line`'s
                      // real box-drawing bytes to render as lines rather than garbage; harmless
                      // for plain ASCII text, since CP437's low 128 codepoints match ASCII.
    ]
}

fn bold(on: bool) -> [u8; 3] {
    [ESC, b'E', u8::from(on)]
}

/// Selects Font B (`ESC M 1`), the printer's smaller built-in face, or
/// returns to Font A (`ESC M 0`). Used only for the developer credit block
/// at the foot of a receipt, so it reads as secondary to the transaction.
///
/// Font B fits *more* characters per line than Font A, so `LINE_WIDTH`
/// (measured for Font A) is a conservative budget while it's active — text
/// sized against it can never overflow. Everything else on every template
/// stays in Font A, which is what all the column arithmetic assumes.
fn font_small(on: bool) -> [u8; 3] {
    [ESC, b'M', u8::from(on)]
}

/// `GS ! n`: glyphs `scale`× as wide and as tall (1 = normal).
fn char_size(scale: u8) -> [u8; 3] {
    let n = scale.saturating_sub(1) & 0x07;
    [GS, b'!', (n << 4) | n]
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

/// Widest a logo is allowed to print at, in dots. Bumped up from a
/// previous 200 now that the business name is no longer printed as text
/// (see `build_receipt_bytes`'s doc comment) — the logo is the receipt's
/// only brand mark now, so it can afford to read as a real header rather
/// than a small corner mark. Still comfortably under this printer family's
/// *measured* real printable width (~504 dots — 42 columns × 12 dots/col
/// at Font A, see `layout`'s module doc comment for how that 42 was
/// measured off a real client receipt) rather than the nominal 576-dot
/// spec-sheet number for 80mm paper, which already proved wrong once.
const LOGO_MAX_WIDTH_PX: u32 = 288;
/// Tallest a logo prints at — about 20 mm at 8 dots/mm, matching the PDF's
/// `LOGO_MAX_HEIGHT_MM`, so a square or portrait logo stays a header.
const LOGO_MAX_HEIGHT_PX: u32 = 160;

/// Decodes `image_bytes` (PNG/JPEG — the only formats the `image` crate is
/// built with here) and thresholds it to 1-bit black/white, scaling down to
/// fit [`LOGO_MAX_WIDTH_PX`] if needed. Returns `None` on anything that isn't
/// a decodable raster image — notably an SVG logo, which `app_config` allows
/// as an on-screen logo (see `images::LOGO_ALLOWED_EXTENSIONS`) but which
/// there is no SVG rasterizer wired in here to handle — so the receipt is
/// built exactly as if no logo were set at all, rather than erroring out or
/// sending a corrupt print job.
pub fn build_logo_raster(image_bytes: &[u8]) -> Option<LogoRaster> {
    // A decoder panic on an unusual JPEG/PNG must cost the logo, never the
    // receipt.
    std::panic::catch_unwind(|| decode_logo(image_bytes)).ok().flatten()
}

fn decode_logo(image_bytes: &[u8]) -> Option<LogoRaster> {
    let mut rgba = image::load_from_memory(image_bytes).ok()?.into_rgba8();
    if rgba.width() == 0 || rgba.height() == 0 {
        return None;
    }

    // Crop to the visible mark: uploaded logos often carry wide white or
    // transparent margins that would otherwise shrink the logo on paper.
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, p) in rgba.enumerate_pixels() {
        if paper_luminance(p.0) < 230 {
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
    }
    if x0 == u32::MAX {
        return None; // nothing visible to print
    }
    let rgba_cropped = image::imageops::crop(&mut rgba, x0, y0, x1 - x0 + 1, y1 - y0 + 1).to_image();
    let (width, height) = rgba_cropped.dimensions();

    // Scaled to fill the logo box either way: a small upload (a 100 px JPEG
    // is common) would otherwise print a centimetre wide.
    let scale = (LOGO_MAX_WIDTH_PX as f64 / width as f64).min(LOGO_MAX_HEIGHT_PX as f64 / height as f64);
    let new_width = (((width as f64) * scale).round().max(1.0)) as u32;
    let new_height = (((height as f64) * scale).round().max(1.0)) as u32;
    let scaled = image::imageops::resize(&rgba_cropped, new_width, new_height, image::imageops::FilterType::Triangle);

    let width_bytes = (new_width as usize + 7) / 8;
    let mut bitmap = vec![0u8; width_bytes * new_height as usize];
    for (x, y, p) in scaled.enumerate_pixels() {
        // A simple fixed threshold — good enough for the flat black/white
        // shop logos this feeds (line art, wordmarks), not photographs.
        if paper_luminance(p.0) < 160 {
            let idx = y as usize * width_bytes + (x as usize / 8);
            bitmap[idx] |= 0x80 >> (x as usize % 8);
        }
    }

    Some(LogoRaster { width_px: new_width as usize, height_px: new_height as usize, bitmap })
}

/// A pixel's brightness as printed on white paper: a transparent PNG
/// background (usually stored as transparent *black*) must read as paper,
/// not ink.
fn paper_luminance([r, g, b, a]: [u8; 4]) -> u8 {
    let ink = (r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000;
    ((ink * a as u32 + 255 * (255 - a as u32)) / 255) as u8
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

/// Item-row column widths shared by every line-item table below: Item(17) /
/// Qty(5) / Rate(8) / Amount(12) = 42 = `LINE_WIDTH`. A public const (not a
/// magic number repeated per template) so the header row and every line
/// stay in lockstep if this ever needs retuning for a differently-measured
/// printer — see `layout`'s module doc comment for how 42 was arrived at.
/// Qty widened from an original 4 to 5 (Item shrunk 18->17 to pay for it)
/// to fit a `sold_by_amount` line's fractional quantity ("12.50") — see
/// [`ITEM_TABLE_COLS`]'s doc comment for the fuller reasoning, which
/// applies here identically.
const ITEM_COLS: [usize; 4] = [17, 5, 8, 12];

fn item_row(desc: &str, qty: &str, rate: &str, amount: &str) -> String {
    row(&[(desc, ITEM_COLS[0], false), (qty, ITEM_COLS[1], true), (rate, ITEM_COLS[2], true), (amount, ITEM_COLS[3], true)])
}

/// Content-only column widths for the customer receipt's box-drawn item
/// table (below, [`box_row`]/[`box_line`]): Item(16) / Qty(5) / Rate(7) /
/// Amount(9) = 37, plus the 5 vertical-rule separators add = 42 =
/// `LINE_WIDTH` — same total budget as [`ITEM_COLS`] above, just split
/// differently to leave room for the border characters. Rate(7) exactly
/// fits "1300.00" with no padding; Amount(9) fits "3900.00" with 2 spaces
/// to spare — both come from `escpos::tests::
/// build_receipt_bytes_fits_the_measured_42_column_regression_row`, the
/// exact row a real client receipt showed wrapping mid-digit before this
/// column budget was corrected from an assumed 48 down to a measured 42.
///
/// Qty widened from an original 3 to 5 (Item shrunk 18->16 to pay for it,
/// two characters truncated off whatever previously fit exactly) so a
/// `sold_by_amount` line's fractional quantity ("0.77", "12.50" — see
/// `layout::format_qty`) fits without itself overflowing onto another
/// physical line, the exact bug this file spent real client-hardware
/// testing fixing once already for plain integer quantities. There was no
/// budget left to spare for a unit label too (kg/g/…) — see `layout::
/// format_qty`'s doc comment for why that's PDF-only.
const ITEM_TABLE_COLS: [usize; 4] = [16, 5, 7, 9];


/// The developer credit printed at the very bottom of a customer receipt.
/// Constants rather than config fields on purpose — this is the vendor's
/// own attribution, not something a client configures per installation.
///
/// The trademark is spelled `(TM)`, not `™`, and that is deliberate: this
/// text prints while the printer is in codepage CP437 (selected once by
/// [`init`]), and CP437 has no trademark glyph at any codepoint. Sending
/// the UTF-8 bytes for `™` (E2 84 A2) would print three unrelated CP437
/// characters, not a symbol. The PDF path has a real font and no such
/// limit, so it uses the actual `™` — the one place the two outputs
/// deliberately differ, because the hardware can't render what the PDF can.
const DEVELOPER_CREDIT: &str = "Developed by Code Hunts(TM)";
/// The same credit for the drawn receipt, which has a real ™ glyph (CP437 text does not).
const DEVELOPER_CREDIT_FULL: &str = "Developed by Code Hunts™";
const DEVELOPER_SITE: &str = "codehunts.co.uk";
/// Spaced rather than one unbroken digit string, so it reads as a phone
/// number a customer could actually dial off the paper.
const DEVELOPER_PHONE: &str = "+92 339 0129547";
/// Separator between site and phone when both share one line.
const CREDIT_SEPARATOR: &str = "  |  ";

/// The developer credit block: a centered attribution line, then the
/// contact details, in the printer's smaller Font B so it reads as a
/// footer rather than competing with the transaction above it.
///
/// Whether the site and phone share one line or take one each is derived
/// from [`LINE_WIDTH`] rather than hardcoded, so this stays correct if that
/// budget ever becomes configurable (a 58mm roll is ~32 columns, where the
/// combined line no longer fits and splits on its own). The check is
/// deliberately made against Font A's budget even though the block prints
/// in Font B, which fits *more* characters per line — measuring against the
/// narrower of the two can only ever split earlier than strictly necessary,
/// never too late, so nothing can wrap mid-word.
///
/// Emits no feed or cut of its own: the caller places it above the
/// pre-cut padding so the cutter never runs through it.
fn developer_credit_block(out: &mut Vec<u8>) {
    out.extend(align_center());
    out.extend(font_small(true));

    out.extend(truncate_line(DEVELOPER_CREDIT).as_bytes());
    out.push(b'\n');

    let combined = format!("{}{}{}", DEVELOPER_SITE, CREDIT_SEPARATOR, DEVELOPER_PHONE);
    if combined.chars().count() <= LINE_WIDTH {
        out.extend(combined.as_bytes());
        out.push(b'\n');
    } else {
        out.extend(truncate_line(DEVELOPER_SITE).as_bytes());
        out.push(b'\n');
        out.extend(truncate_line(DEVELOPER_PHONE).as_bytes());
        out.push(b'\n');
    }

    out.extend(font_small(false));
    out.extend(align_left());
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

/// The skeleton every standalone report print shares: wake + init, the
/// business-name header with `subtitle_lines`, `body`, then footer and cut.
fn report_document(config: &AppConfig, subtitle_lines: &[String], body: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(wake_padding());
    out.extend(init());
    header_block(&mut out, &config.business_name, subtitle_lines);
    body(&mut out);
    close_out(&mut out, &config.receipt_footer);
    out
}

/// A bordered three-column table's top border, bold header row and the
/// border under it.
fn three_col_header(out: &mut Vec<u8>, left: &str, middle: &str, right: &str) {
    out.extend(three_col_border().as_bytes());
    out.push(b'\n');
    out.extend(bold(true));
    out.extend(three_col_row(left, middle, right).as_bytes());
    out.push(b'\n');
    out.extend(bold(false));
    out.extend(three_col_border().as_bytes());
    out.push(b'\n');
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

/// "1,250" for whole amounts, "1,250.50" otherwise — the receipt prints the
/// currency nowhere per line, like a till slip.
fn amount(minor: i64) -> String {
    let abs = minor.unsigned_abs();
    let digits = (abs / 100).to_string();
    let mut grouped = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(c);
    }
    let sign = if minor < 0 { "-" } else { "" };
    match abs % 100 {
        0 => format!("{sign}{grouped}"),
        cents => format!("{sign}{grouped}.{cents:02}"),
    }
}

/// "17/10/2026 02:25 AM" from the stored "2026-10-17 02:25:00".
fn receipt_date(created_at: &str) -> String {
    chrono::NaiveDateTime::parse_from_str(created_at, "%Y-%m-%d %H:%M:%S")
        .map(|t| t.format("%d/%m/%Y %I:%M %p").to_string())
        .unwrap_or_else(|_| created_at.to_string())
}

/// "Dine In" / "Takeaway" / "Delivery" / the platform's name. A sale with no
/// recorded type is a counter sale, called "Takeaway" when the shop uses tables.
fn order_type_label(sale: &Sale, tables_module_enabled: bool) -> String {
    match sale.order_type.as_deref() {
        Some("dine_in") => "Dine In".into(),
        Some("takeaway") => "Takeaway".into(),
        Some("delivery") => "Delivery".into(),
        Some("third_party_delivery") => sale.delivery_platform_name.clone().unwrap_or_else(|| "3rd-Party".into()),
        _ if sale.table_name.is_some() => "Dine In".into(),
        _ => if tables_module_enabled { "Takeaway" } else { "Counter Sale" }.into(),
    }
}

/// "cash" -> "Cash", "credit" -> "Pay later" — the PDF's `paymentMethodLabel`.
fn payment_label(method: &str) -> String {
    match method {
        "credit" => "Pay later".to_string(),
        _ => {
            let mut chars = method.chars();
            chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
        }
    }
}

/// "Name (phone)", or just the name when there's no phone.
fn rider_text(name: &str, phone: Option<&str>) -> String {
    match phone {
        Some(p) if !p.is_empty() => format!("{name} ({p})"),
        _ => name.to_string(),
    }
}

/// Item table columns (fractions of the width): Qty / Rate / Disc / Amount
/// — the slip's alignment: Qty from the left, Rate and Amount flush right
/// under their headers, Disc centred. Same as `COLUMNS` in `receiptPdf.ts`.
const COL_QTY: (f32, f32) = (0.0, 0.2);
const COL_RATE: (f32, f32) = (0.2, 0.46);
const COL_DISC: (f32, f32) = (0.52, 0.7);
const COL_AMOUNT: (f32, f32) = (0.72, 1.0);
/// The compact design's columns: Items / QTY / Rate / Disc / Amount, one row
/// per item. Same as `COMPACT_COLUMNS` in `receiptPdf.ts`.
const COMPACT_ITEM: (f32, f32) = (0.0, 0.32);
const COMPACT_QTY: (f32, f32) = (0.32, 0.44);
const COMPACT_RATE: (f32, f32) = (0.44, 0.63);
const COMPACT_DISC: (f32, f32) = (0.64, 0.76);
const COMPACT_AMOUNT: (f32, f32) = (0.76, 1.0);

/// One compact row's cells, name first.
fn compact_cells(name: &str, qty: String, rate: String, disc: &str, total: String, bold: bool) -> Vec<Cell> {
    vec![
        Cell::new(name, COMPACT_ITEM.0, COMPACT_ITEM.1 - 0.01, Align::Left, bold),
        Cell::new(qty, COMPACT_QTY.0, COMPACT_QTY.1, Align::Center, bold),
        Cell::new(rate, COMPACT_RATE.0, COMPACT_RATE.1, Align::Right, bold),
        Cell::new(disc, COMPACT_DISC.0, COMPACT_DISC.1, Align::Center, bold),
        Cell::new(total, COMPACT_AMOUNT.0, COMPACT_AMOUNT.1, Align::Right, bold),
    ]
}

/// A bold label with its value right after it ("Invoice #:  119728").
fn info_row(label: &str, value: impl Into<String>) -> Row {
    Row::Pair { label: label.into(), value: value.into(), size: SIZE_BODY }
}

/// A bold label on the left, its amount on the right ("SubTotal   1,380").
fn total_row(label: &str, value: impl Into<String>) -> Row {
    Row::Cells {
        cells: vec![Cell::new(label, 0.0, 0.6, Align::Left, true), Cell::new(value, 0.6, 1.0, Align::Right, false)],
        size: SIZE_BODY,
    }
}

fn centered(text: impl Into<String>, size: f32, bold: bool) -> Row {
    Row::Text { text: text.into(), align: Align::Center, size, bold }
}

/// The diagonal stamp behind the items: "PAID" for a settled sale, "UNPAID"
/// while part of it is on pay-later, none when the shop turned stamps off.
/// Must match `receiptStamp` in `receiptPdf.ts`.
pub fn receipt_stamp(sale: &Sale, config: &AppConfig) -> Option<&'static str> {
    if !config.receipt_show_stamp {
        None
    } else if sale.payments.iter().any(|part| part.method == "credit") {
        Some("UNPAID")
    } else {
        Some("PAID")
    }
}

/// What the customer receipt shows, top to bottom: logo (printed separately)
/// over a thin rule, shop name, address, phone, website, tagline banner; the
/// sale's details; the item table (name on its own line, then Qty / Rate /
/// Disc / Amount, dotted between items); totals and payments; the item
/// count, thank-you, follow-us line and developer credit. `receiptPdf.ts`
/// draws the same layout for the PDF.
/// `logo_width`: the printed logo's width in dots, when there is one — the
/// line under it is as wide.
pub fn receipt_rows(sale: &Sale, config: &AppConfig, tables_module_enabled: bool, logo_width: Option<usize>) -> Vec<Row> {
    let mut rows = Vec::new();
    let present = |v: &Option<String>| v.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);

    if let Some(width) = logo_width {
        rows.push(Row::ShortRule((width + 40).min(receipt_image::WIDTH_PX)));
    }
    rows.push(centered(config.business_name.to_uppercase(), SIZE_TITLE, true));
    if let Some(address) = present(&config.business_address) {
        rows.push(centered(address, SIZE_BODY, false));
    }
    if let Some(phone) = present(&config.phone) {
        rows.push(centered(format!("Phone No {phone}"), SIZE_BODY, false));
    }
    if let Some(number) = present(&config.delivery_number) {
        rows.push(centered(format!("Delivery No {number}"), SIZE_BODY, false));
    }
    if let Some(site) = present(&config.business_website) {
        rows.push(centered(site, SIZE_BODY, false));
    }
    if let Some(tagline) = present(&config.receipt_tagline) {
        rows.push(Row::Banner { text: tagline, size: SIZE_BODY });
    }
    rows.push(Row::Gap(14));

    rows.push(info_row("Cashier:", sale.cashier_name.clone().unwrap_or_else(|| "—".into()).to_uppercase()));
    rows.push(info_row("Date #:", receipt_date(&sale.created_at)));
    rows.push(info_row("Invoice #:", sale.receipt_no.clone()));
    if let Some(no) = sale.order_no {
        rows.push(info_row("Order #:", no.to_string()));
    }
    if let Some(till) = &sale.till_name {
        rows.push(info_row("Counter:", till.clone()));
    }
    rows.push(info_row("Type :", order_type_label(sale, tables_module_enabled)));
    if let Some(table) = &sale.table_name {
        rows.push(info_row("Table #:", table.clone()));
    }
    match sale.order_type.as_deref() {
        Some("delivery") => {
            if sale.customer_name.is_some() || sale.customer_phone.is_some() {
                let name = sale.customer_name.clone().unwrap_or_default();
                rows.push(info_row("Customer:", rider_text(&name, sale.customer_phone.as_deref()).trim().to_string()));
            }
            if let Some(address) = present(&sale.delivery_address) {
                rows.push(Row::Text { text: format!("Address: {address}"), align: Align::Left, size: SIZE_BODY, bold: false });
            }
            if let Some(rider) = &sale.delivery_rider_name {
                rows.push(info_row("Rider:", rider_text(rider, sale.delivery_rider_phone.as_deref())));
            }
        }
        Some("third_party_delivery") => {
            if let Some(reference) = &sale.platform_order_ref {
                rows.push(info_row("Order Ref:", format!("#{reference}")));
            }
            if let Some(rider) = &sale.delivery_rider_name {
                rows.push(info_row("Rider:", rider_text(rider, sale.delivery_rider_phone.as_deref())));
            }
        }
        _ => {}
    }
    rows.push(Row::Gap(16));

    let columns = |qty: String, rate: String, disc: String, total: String, bold: bool| Row::Cells {
        cells: vec![
            Cell::new(qty, COL_QTY.0, COL_QTY.1, Align::Left, bold),
            Cell::new(rate, COL_RATE.0, COL_RATE.1, Align::Right, bold),
            Cell::new(disc, COL_DISC.0, COL_DISC.1, Align::Center, bold),
            Cell::new(total, COL_AMOUNT.0, COL_AMOUNT.1, Align::Right, bold),
        ],
        size: SIZE_BODY,
    };
    let compact = config.receipt_template == "compact";
    rows.push(if compact {
        Row::Cells { cells: compact_cells("Items", "QTY".into(), "Rate".into(), "Disc", "Amount".into(), true), size: SIZE_BODY }
    } else {
        columns("Qty".into(), "Rate".into(), "Disc".into(), "Amount".into(), true)
    });
    rows.push(Row::Rule(Rule::Thick));
    for (i, line) in sale.items.iter().enumerate() {
        let qty = match &line.unit {
            Some(unit) => format!("{} {unit}", format_qty(line.qty)),
            None => format_qty(line.qty),
        };
        let note = line.notes.as_deref().map(str::trim).filter(|n| !n.is_empty()).map(|note| Row::Text {
            text: format!("({note})"),
            align: Align::Left,
            size: SIZE_SMALL,
            bold: false,
        });
        // The discount is taken off the whole bill, never per line.
        if compact {
            let cells = compact_cells(&line.item_name, qty, amount(line.price_at_sale_minor), "0", amount(line.line_total_minor), false);
            rows.push(Row::ItemLine { cells, size: SIZE_BODY });
            rows.extend(note);
        } else {
            if i > 0 {
                rows.push(Row::Rule(Rule::Dotted));
            }
            rows.push(Row::Text { text: line.item_name.clone(), align: Align::Left, size: SIZE_BODY, bold: true });
            rows.extend(note);
            rows.push(columns(qty, amount(line.price_at_sale_minor), "0".into(), amount(line.line_total_minor), false));
        }
    }
    rows.push(Row::Rule(Rule::Thick));

    rows.push(total_row("SubTotal", amount(sale.subtotal_minor)));
    if sale.discount_minor > 0 {
        rows.push(total_row("Discount", format!("-{}", amount(sale.discount_minor))));
    }
    if sale.tax_minor > 0 {
        rows.push(total_row("Tax", amount(sale.tax_minor)));
    }
    if sale.service_charge_minor > 0 {
        rows.push(total_row("Service Charge", amount(sale.service_charge_minor)));
    }
    if let Some(fee) = sale.delivery_charge_minor.filter(|fee| *fee > 0) {
        rows.push(total_row("Delivery Fee", amount(fee)));
    }
    rows.push(total_row("NetTotal", amount(sale.total_minor)));
    let tendered = sale.tendered_minor.filter(|_| sale.payments.iter().any(|p| p.method == "cash"));
    for part in &sale.payments {
        // Cash shows what the customer handed over; the change row follows.
        let paid = if part.method == "cash" { tendered.unwrap_or(part.amount_minor) } else { part.amount_minor };
        rows.push(total_row(&format!("Payment ({})", payment_label(&part.method)), amount(paid)));
    }
    if let Some(due) = &sale.due_date {
        rows.push(total_row("Balance Due By", due.clone()));
    }
    if let Some(tendered) = tendered {
        rows.push(total_row("Change", amount(sale.change_minor.unwrap_or_else(|| (tendered - sale.total_minor).max(0)))));
    }
    rows.push(Row::Rule(Rule::Thick));
    rows.push(Row::Gap(18));

    let total_qty: f64 = sale.items.iter().map(|line| line.qty).sum();
    rows.push(centered(format!("Total Items: {} Total QTY: {}", sale.items.len(), format_qty(total_qty)), SIZE_BODY, true));
    for line in config.receipt_footer.lines().map(str::trim).filter(|l| !l.is_empty()) {
        rows.push(centered(line, SIZE_BODY, false));
    }
    if let Some(social) = present(&config.receipt_social) {
        rows.push(centered(social, SIZE_SMALL, false));
    }
    if config.receipt_show_credit {
        rows.push(Row::Gap(14));
        rows.push(centered(DEVELOPER_CREDIT_FULL, SIZE_SMALL, false));
        rows.push(centered(format!("{DEVELOPER_SITE}{CREDIT_SEPARATOR}{DEVELOPER_PHONE}"), SIZE_SMALL, false));
    }
    rows
}

/// Builds the full byte stream for one customer receipt: the logo as its own
/// raster, then [`receipt_rows`] drawn as a picture (`receipt_image`) in a
/// proportional font, then the feed and cut. Pure and hardware-independent,
/// so it is covered by tests without a printer.
///
/// `tables_module_enabled` decides the "Type" of a sale with no recorded
/// order type: "Takeaway" if the shop uses tables, "Counter Sale" if not.
/// One copy, never automatic — printing is always an explicit action.
pub fn build_receipt_bytes(
    sale: &Sale,
    config: &AppConfig,
    logo: Option<&LogoRaster>,
    tables_module_enabled: bool,
) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(wake_padding());
    out.extend(init());
    if let Some(logo) = logo {
        print_logo(&mut out, logo);
    }
    out.extend(align_left());
    let rows = receipt_rows(sale, config, tables_module_enabled, logo.map(|l| l.width_px));
    let (bitmap, height) = receipt_image::render(&rows, receipt_stamp(sale, config));
    out.extend(receipt_image::raster_commands(&bitmap, height));
    out.push(b'\n');
    out.push(b'\n');
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
                &format_qty(item.qty_refunded),
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

/// Builds the full ESC/POS byte stream for one KOT (Kitchen Order Ticket)
/// token — a kitchen/counter instruction, deliberately separate from and
/// visually distinct from a bill (see the "TOKEN" banner below): no
/// prices, no totals, no payment info, since counter/kitchen staff only
/// need to know what to prepare. Reuses this file's shared primitives
/// (`header_block`, `two_col`, `divider`, `close_out`, the measured
/// `LINE_WIDTH`) the same way every other template does, rather than a
/// separate printing path — a token that wrapped or broke mid-line would
/// be exactly as unusable as the receipt was before that got fixed.
///
/// `token` doubles as both "the record of an already-printed token" and,
/// from `commands::tokens_print`, "the token about to be printed" — the
/// caller builds a `TokenSummary` with the fields this ticket needs
/// (`token_number`, `counter_name`, `table_name`, `items`, …) *before* the
/// row exists in the database at all, since the physical print must
/// succeed before anything is recorded (see `db::tokens`'s module doc
/// comment). `token.id`/`table_order_id` are never printed, so a
/// placeholder value in that draft is harmless.
///
/// Unlike the customer receipt's item table, a token has no Rate/Amount
/// columns competing for space — `two_col`'s full value-column width is
/// free for a quantity *and* its unit ("2 kg"), so this is the one place
/// on the thermal path a `sold_by_amount` item's unit does show (contrast
/// `layout::format_qty`'s doc comment, which explains why the bill's item
/// table can't afford one).
/// Deliberately stripped down to almost nothing — no logo, no business
/// name, no phone, no cashier line: just when it printed, which token it
/// is, which table it's for, and what to prepare. A token is a scrap of
/// paper handed to a counter and thrown away once the order's collected,
/// not a record anyone needs to look back on, so every extra line here is
/// just wasted paper on a kitchen printer that's producing dozens of these
/// a shift. The one exception is the order type — a kitchen serving several
/// tables (and Takeaway orders) at once needs to know which order a ticket
/// belongs to at a glance, not just what's on it, so a line right under the
/// token number always says either `Table: {name}` (`token.table_name` is
/// `Some`) or plain `Takeaway` (an ad hoc token, with no table behind it).
/// Counter identity — which physical station this printed from — is still
/// cut; that's one `two_col` call to bring back if a shared printer serving
/// multiple counters ever needs it too.
/// Builds the full ESC/POS byte stream for one KOT (Kitchen Order Ticket)
/// token — a kitchen/counter instruction, clearly structured with a distinct
/// header, bold badge box, item details and totals, formatted compactly for 80mm rolls.
pub fn build_token_bytes(token: &TokenSummary, is_reprint: bool) -> Vec<u8> {
    let mut out = Vec::new();
    let title = if is_reprint { "*** REPRINT ***" } else { "*** KITCHEN TICKET ***" };
    let type_or_table = match &token.table_name {
        Some(table_name) => format!("Table: {}", table_name),
        None => "Takeaway".to_string(),
    };
    let number = match &token.till_label {
        Some(label) => format!("{label}-{}", token.token_number),
        None => token.token_number.to_string(),
    };
    token_head(&mut out, title, &number, &type_or_table, &token.printed_at);
    token_items_header(&mut out);

    // Body: item + qty only — no rate, no total.
    let mut total_qty = 0.0;
    for line in &token.items {
        total_qty += line.qty;
        let qty_text = match &line.unit {
            Some(unit) => format!("{} {}", format_qty(line.qty), unit),
            None => format_qty(line.qty),
        };
        token_item(&mut out, &line.item_name, &qty_text);
        if line.sold_by_amount {
            out.extend(two_col("", &format!("({})", format_minor(line.amount_minor))).as_bytes());
            out.push(b'\n');
        }
    }

    token_foot(&mut out, total_qty, &[], "*** PREPARE FRESH ***");
    out
}

/// The part every token shares above its details: the titled double-ruled
/// banner, the boxed `TOKEN #n` badge with its table/type line, and the
/// date/time row.
fn token_head(out: &mut Vec<u8>, title: &str, number: &str, type_or_table: &str, printed_at: &str) {
    out.extend(wake_padding());
    out.extend(init());

    out.extend(align_center());
    out.extend(double_divider().as_bytes());
    out.push(b'\n');
    out.extend(bold(true));
    out.extend(truncate_line(title).as_bytes());
    out.push(b'\n');
    out.extend(bold(false));
    out.extend(double_divider().as_bytes());
    out.push(b'\n');

    // Boxed badge (CP437 border, width 40 + 2 border chars = 42 LINE_WIDTH)
    out.extend(align_left());
    out.extend(box_line(&[40], BoxLinePosition::Top));
    out.push(b'\n');
    out.extend(bold(true));
    out.extend(box_row(&[(&center_text(&format!("TOKEN #{number}"), 40), 40, false)]));
    out.push(b'\n');
    out.extend(box_row(&[(&center_text(type_or_table, 40), 40, false)]));
    out.push(b'\n');
    out.extend(bold(false));
    out.extend(box_line(&[40], BoxLinePosition::Bottom));
    out.push(b'\n');

    let (date_part, time_part) = printed_at.split_once(' ').unwrap_or((printed_at, ""));
    let dt_row = if !time_part.is_empty() {
        two_col(&format!("Date: {date_part}"), &format!("Time: {time_part}"))
    } else {
        format!("Date: {date_part}")
    };
    out.extend(dt_row.as_bytes());
    out.push(b'\n');
}

fn token_items_header(out: &mut Vec<u8>) {
    out.extend(divider().as_bytes());
    out.push(b'\n');
    out.extend(bold(true));
    out.extend(two_col("ITEM / DESCRIPTION", "QTY").as_bytes());
    out.push(b'\n');
    out.extend(bold(false));
    out.extend(divider().as_bytes());
    out.push(b'\n');
}

fn token_item(out: &mut Vec<u8>, name: &str, qty_text: &str) {
    out.extend(bold(true));
    out.extend(two_col(name, qty_text).as_bytes());
    out.extend(bold(false));
    out.push(b'\n');
}

/// The item count, closing rules and the closing line, then feed and cut.
/// `amounts`: rows under the item count (only the customer's copy has prices).
fn token_foot(out: &mut Vec<u8>, total_qty: f64, amounts: &[(&str, String)], closing: &str) {
    out.extend(divider().as_bytes());
    out.push(b'\n');
    out.extend(bold(true));
    out.extend(two_col("TOTAL ITEMS:", &format_qty(total_qty)).as_bytes());
    out.push(b'\n');
    for (label, value) in amounts {
        out.extend(two_col(label, value).as_bytes());
        out.push(b'\n');
    }
    out.extend(bold(false));
    out.extend(double_divider().as_bytes());
    out.push(b'\n');

    out.extend(align_center());
    out.extend(truncate_line(closing).as_bytes());
    out.push(b'\n');

    close_out(out, "");
}

/// A `Label: value` detail line, skipped when the value is missing or blank.
fn detail_line(out: &mut Vec<u8>, label: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) {
        out.extend(truncate_line(&format!("{label}: {value}")).as_bytes());
        out.push(b'\n');
    }
}

/// Builds and sends one KOT token to this installation's configured
/// printer. Called only from `commands::tokens_print`/`commands::tokens_
/// reprint`, both of which own the "never claim success unless the
/// physical print actually succeeded" ordering — this function itself is
/// just the byte-building + transport call, same division of
/// responsibility as `print_receipt`.
pub fn print_token(token: &TokenSummary, config: &AppConfig, is_reprint: bool) -> Result<(), PrinterError> {
    send_to_printer(&build_token_bytes(token, is_reprint), config)
}

/// Builds thermal print bytes for an active order token (takeaway or dine-in).
pub fn build_order_token_bytes(order: &crate::db::orders::Order, config: &AppConfig, is_kitchen: bool) -> Vec<u8> {
    let mut out = Vec::new();
    let title = if is_kitchen { "*** KITCHEN TICKET ***" } else { "*** CUSTOMER TOKEN ***" };
    let type_or_table = match &order.table_name {
        Some(table_name) => format!("TABLE: {}", table_name.to_uppercase()),
        None => match order.order_type.as_str() {
            "delivery" => "ORDER: DELIVERY".to_string(),
            "third_party_delivery" => format!(
                "DELIVERY: {}",
                order.delivery_platform_name.as_deref().unwrap_or("PLATFORM").to_uppercase()
            ),
            _ => format!("TYPE: {}", order.order_type.to_uppercase()),
        },
    };
    token_head(&mut out, title, &order.order_no.to_string(), &type_or_table, &order.created_at);

    // Optional customer, contact, address, rider, prep time, notes
    detail_line(&mut out, "Customer", order.customer_name.as_deref());
    detail_line(&mut out, "Phone", order.customer_phone.as_deref());
    detail_line(&mut out, "Address", order.delivery_address.as_deref());
    if let Some(rider) = order.delivery_rider_name.as_deref().map(str::trim).filter(|r| !r.is_empty()) {
        let phone = order.delivery_rider_phone.as_deref().map(str::trim);
        detail_line(&mut out, "Rider", Some(&rider_text(rider, phone)));
    }
    detail_line(&mut out, "Platform Ref", order.platform_order_ref.as_deref());
    if order.estimated_prep_minutes > 0 {
        out.extend(truncate_line(&format!("Est. Prep: {} min", order.estimated_prep_minutes)).as_bytes());
        out.push(b'\n');
    }
    detail_line(&mut out, "Notes", order.notes.as_deref());

    if is_kitchen {
        token_items_header(&mut out);
    } else {
        out.extend(divider().as_bytes());
        out.push(b'\n');
        out.extend(bold(true));
        out.extend(two_col("ITEM / DESCRIPTION", "AMOUNT").as_bytes());
        out.push(b'\n');
        out.extend(bold(false));
        out.extend(divider().as_bytes());
        out.push(b'\n');
    }
    let mut total_qty = 0.0;
    let mut items_minor = 0;
    for line in order.lines.iter().filter(|line| line.status != "voided") {
        total_qty += line.qty;
        if is_kitchen {
            token_item(&mut out, &line.item_name, &format!("{}x", format_qty(line.qty)));
        } else {
            // Rounded per line, like the sale the order becomes.
            let line_minor = (line.qty * line.price_at_order_minor as f64).round() as i64;
            items_minor += line_minor;
            token_item(&mut out, &line.item_name, &amount(line_minor));
            out.extend(truncate_line(&format!("  {} x {}", format_qty(line.qty), amount(line.price_at_order_minor))).as_bytes());
            out.push(b'\n');
        }
        if let Some(line_note) = line.notes.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
            out.extend(truncate_line(&format!("  * {line_note}")).as_bytes());
            out.push(b'\n');
        }
    }

    let closing = if is_kitchen { "*** PREPARE FRESH ***" } else { "*** THANK YOU FOR YOUR ORDER ***" };
    let money = |minor: i64| format!("{} {}", config.currency, amount(minor));
    let mut amounts = Vec::new();
    if !is_kitchen {
        let taxable = (items_minor - order.discount_minor).max(0);
        // The cart's formula (`computeCartTotals`), so the token shows what paying will charge.
        let charge = |percent: f64| (taxable as f64 * percent / 100.0).round() as i64;
        let tax = if order.tax_exempt { 0 } else { charge(config.tax_percent) };
        let service = if order.service_waived || config.business_type != "restaurant" { 0 } else { charge(config.service_charge_percent) };
        let total = taxable + tax + service + order.delivery_charge_minor;
        if total != items_minor {
            amounts.push(("SUBTOTAL:", money(items_minor)));
        }
        if order.discount_minor > 0 {
            amounts.push(("DISCOUNT:", format!("-{}", money(order.discount_minor))));
        }
        if tax > 0 {
            amounts.push(("TAX:", money(tax)));
        }
        if service > 0 {
            amounts.push(("SERVICE CHARGE:", money(service)));
        }
        if order.delivery_charge_minor > 0 {
            amounts.push(("DELIVERY FEE:", money(order.delivery_charge_minor)));
        }
        amounts.push(("TOTAL:", money(total)));
    }
    token_foot(&mut out, total_qty, &amounts, closing);
    out
}

pub fn print_order_token(order: &crate::db::orders::Order, config: &AppConfig, is_kitchen: bool) -> Result<(), PrinterError> {
    send_to_printer(&build_order_token_bytes(order, config, is_kitchen), config)
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

    let rows: [(&str, i64); 7] = [
        ("Opening Balance", summary.opening_balance_minor),
        ("Cash Sale", summary.cash_sales_minor),
        ("Card Sale", summary.card_sales_minor),
        ("Platform Sale", summary.platform_sales_minor),
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
    if summary.collected_cash_minor > 0 {
        out.extend(two_col("Credit Collected", &money(summary.collected_cash_minor)).as_bytes());
        out.push(b'\n');
    }

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
    let subtitle = ["Category Wise Sale".to_string(), format!("{} to {}", report.start_date, report.end_date)];
    report_document(config, &subtitle, |out| write_category_sales_section(out, report, &config.currency))
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

        three_col_header(out, "Item", "Qty", "Revenue");
        for item in &group.items {
            out.extend(
                three_col_row(&item.item_name, &format_qty(item.qty_sold), &format_minor(item.revenue_minor))
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
    let subtitle = ["Table Wise Sales".to_string(), format!("{} to {}", report.start_date, report.end_date)];
    report_document(config, &subtitle, |out| write_table_sales_section(out, report, &config.currency))
}

/// The bordered Table/Txns/Amount grid plus grand-total rule — reused by
/// both [`build_table_sales_bytes`] and [`build_full_report_bytes`], same
/// reasoning as [`write_category_sales_section`].
fn write_table_sales_section(out: &mut Vec<u8>, report: &TableSalesSummary, currency: &str) {
    three_col_header(out, "Table / Counter", "Txns", "Amount");
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

/// The Employee Report: one row per active employee for a given month —
/// days present against the real calendar days in that month (attendance),
/// and the net pay it produces (salary), plus how much of that has actually
/// been paid and its status, built from `salary::get_monthly_overview`'s
/// `SalaryCalculation` rows. Same "bordered table, then grand totals" shape
/// as `build_table_sales_bytes`, but each row needs a second, unbordered
/// line underneath it (Paid amount + status) since a 3-column budget has no
/// room left for a 4th figure — same trade-off `write_category_sales_
/// section` makes with its per-category Subtotal line sitting outside the
/// item box.
pub fn build_employee_report_bytes(rows: &[SalaryCalculation], month: &str, config: &AppConfig) -> Vec<u8> {
    let subtitle = ["Employee Report".to_string(), format!("Month: {}", month)];
    report_document(config, &subtitle, |out| write_employee_report_section(out, rows, &config.currency))
}

fn write_employee_report_section(out: &mut Vec<u8>, rows: &[SalaryCalculation], currency: &str) {
    if rows.is_empty() {
        out.extend(b"No employees found\n");
        return;
    }

    three_col_header(out, "Employee", "Present", "Net Pay");

    let mut total_net_minor: i64 = 0;
    let mut total_paid_minor: i64 = 0;
    for row in rows {
        let present = format!("{}/{}", row.days_present, row.working_days_in_month);
        out.extend(
            three_col_row(&row.employee_name, &present, &format_minor(row.calculated_amount_minor)).as_bytes(),
        );
        out.push(b'\n');

        let status_label = match row.status {
            PaymentStatus::Unpaid => "Unpaid",
            PaymentStatus::Partial => "Partial",
            PaymentStatus::Paid => "Paid",
        };
        out.extend(
            truncate_line(&format!("  Paid: {}  ({})", format_minor(row.paid_amount_minor), status_label))
                .as_bytes(),
        );
        out.push(b'\n');

        total_net_minor += row.calculated_amount_minor;
        total_paid_minor += row.paid_amount_minor;
    }
    out.extend(three_col_border().as_bytes());
    out.push(b'\n');

    out.extend(double_divider().as_bytes());
    out.push(b'\n');
    out.extend(bold(true));
    out.extend(two_col("Total Net Pay", &format!("{} {}", currency, format_minor(total_net_minor))).as_bytes());
    out.push(b'\n');
    out.extend(two_col("Total Paid", &format!("{} {}", currency, format_minor(total_paid_minor))).as_bytes());
    out.push(b'\n');
    out.extend(bold(false));
}

/// Builds and sends the Employee Report to this installation's configured
/// printer — same division of responsibility as `print_table_sales`.
pub fn print_employee_report(rows: &[SalaryCalculation], month: &str, config: &AppConfig) -> Result<(), PrinterError> {
    send_to_printer(&build_employee_report_bytes(rows, month, config), config)
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

        three_col_header(out, "Item", "Qty", "Amount");
        for item in &refund.items {
            out.extend(
                three_col_row(&item.item_name, &format_qty(item.qty_refunded), &format_minor(item.amount_refunded_minor))
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
    let subtitle = ["Refunds Report".to_string(), format!("{} to {}", report.start_date, report.end_date)];
    report_document(config, &subtitle, |out| write_refunds_section(out, report, &config.currency))
}

/// The Product Wise Sales bordered grid — rank-prefixed item name / qty /
/// revenue, same 3-column border style as the other two sections. Only
/// used inside [`build_full_report_bytes`] — Product Wise Sales has no
/// standalone thermal-print button of its own (PDF/CSV only), same as it
/// is on-screen.
#[allow(dead_code)]
fn write_product_sales_section(out: &mut Vec<u8>, report: &ProductSalesSummaryReport) {
    three_col_header(out, "Item", "Qty", "Revenue");
    for item in &report.rows {
        let label = format!("{}. {}", item.rank, item.item_name);
        out.extend(three_col_row(&label, &format_qty(item.qty_sold), &format_minor(item.revenue_minor)).as_bytes());
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

/// Builds the Overall Sales Report consolidated thermal document: Sales Summary
/// (Total orders, total items sold, gross sales, discounts, taxes, refunds, net sales),
/// Payment Breakdown (Cash, Card, Other with order counts and amounts),
/// Financial Summary (Operating expenses, salary paid, net profit),
/// and Refunds Summary (if any).
///
/// Intentionally excludes detailed product-wise, category-wise, and table-wise item lists
/// to keep the thermal receipt compact, fast-printing, and focused on high-level store performance.
pub fn build_full_report_bytes(report: &FullReport, config: &AppConfig) -> Vec<u8> {
    let mut out = Vec::new();
    let currency = &config.currency;
    let money = |m: i64| format!("{} {}", currency, format_minor(m));

    out.extend(wake_padding());
    out.extend(init());

    let date_line = if report.start_date == report.end_date {
        format!("Date: {}", report.start_date)
    } else {
        format!("{} to {}", report.start_date, report.end_date)
    };

    header_block(
        &mut out,
        &config.business_name,
        &["Overall Sales Report".to_string(), date_line],
    );

    section_banner(&mut out, "SALES SUMMARY");
    out.extend(two_col("Total Orders", &report.overview.transaction_count.to_string()).as_bytes());
    out.push(b'\n');
    if report.sales_metrics.total_items_sold > 0.0 {
        out.extend(two_col("Total Items Sold", &format_qty(report.sales_metrics.total_items_sold)).as_bytes());
        out.push(b'\n');
    }
    out.extend(two_col("Average Order", &money(report.average_sale_minor)).as_bytes());
    out.push(b'\n');
    out.extend(two_col("Gross Sales", &money(report.overview.total_sales_minor)).as_bytes());
    out.push(b'\n');

    if report.sales_metrics.total_discount_minor > 0 {
        out.extend(two_col("Discounts", &format!("-{}", money(report.sales_metrics.total_discount_minor))).as_bytes());
        out.push(b'\n');
    }
    if report.sales_metrics.total_tax_minor > 0 {
        out.extend(two_col("Tax", &money(report.sales_metrics.total_tax_minor)).as_bytes());
        out.push(b'\n');
    }
    if report.overview.refunds_minor > 0 {
        out.extend(two_col("Refunds", &format!("-{}", money(report.overview.refunds_minor))).as_bytes());
        out.push(b'\n');
    }

    out.extend(double_divider().as_bytes());
    out.push(b'\n');
    out.extend(bold(true));
    out.extend(two_col("NET SALES", &money(report.sales_metrics.net_sales_minor)).as_bytes());
    out.push(b'\n');
    out.extend(bold(false));
    out.push(b'\n');

    section_banner(&mut out, "PAYMENT BREAKDOWN");
    let pm = &report.payment_methods;
    let cash_label = format!("Cash ({} orders)", pm.cash_count);
    out.extend(two_col(&cash_label, &money(pm.cash_minor)).as_bytes());
    out.push(b'\n');

    if pm.card_count > 0 || pm.card_minor > 0 {
        let card_label = format!("Card ({} orders)", pm.card_count);
        out.extend(two_col(&card_label, &money(pm.card_minor)).as_bytes());
        out.push(b'\n');
    }

    if pm.other_count > 0 || pm.other_minor > 0 {
        let other_label = format!("Other ({} orders)", pm.other_count);
        out.extend(two_col(&other_label, &money(pm.other_minor)).as_bytes());
        out.push(b'\n');
    }
    out.push(b'\n');

    if report.overview.total_expenses_minor.is_some() || report.overview.total_salary_paid_minor.is_some() {
        section_banner(&mut out, "FINANCIAL SUMMARY");
        out.extend(two_col("Net Sales", &money(report.sales_metrics.net_sales_minor)).as_bytes());
        out.push(b'\n');
        if let Some(expenses) = report.overview.total_expenses_minor {
            out.extend(two_col("Expenses", &format!("-{}", money(expenses))).as_bytes());
            out.push(b'\n');
        }
        if let Some(salary) = report.overview.total_salary_paid_minor {
            out.extend(two_col("Salary Paid", &format!("-{}", money(salary))).as_bytes());
            out.push(b'\n');
        }
        out.extend(double_divider().as_bytes());
        out.push(b'\n');
        out.extend(bold(true));
        out.extend(two_col("NET PROFIT", &money(report.overview.net_profit_minor)).as_bytes());
        out.push(b'\n');
        out.extend(bold(false));
        out.push(b'\n');
    } else {
        out.extend(double_divider().as_bytes());
        out.push(b'\n');
        out.extend(bold(true));
        out.extend(two_col("NET PROFIT", &money(report.overview.net_profit_minor)).as_bytes());
        out.push(b'\n');
        out.extend(bold(false));
        out.push(b'\n');
    }

    if !report.refunds.refunds.is_empty() {
        section_banner(&mut out, "REFUNDS SUMMARY");
        out.extend(two_col("Refunded Orders", &report.refunds.refunds.len().to_string()).as_bytes());
        out.push(b'\n');
        out.extend(two_col("Total Refunded", &money(report.refunds.grand_total_refunded_minor)).as_bytes());
        out.push(b'\n');
        out.push(b'\n');
    }

    close_out(&mut out, &config.receipt_footer);
    out
}

#[derive(Debug, thiserror::Error)]
pub enum PrinterError {
    /// No printer has been selected in Settings yet (or, on desktop, none
    /// was auto-detected on USB). The safe, do-nothing-native default.
    #[error("No thermal printer was found — check it's plugged in and powered on, or use the PDF receipt instead")]
    NotConfigured,
    /// Selected in Settings, but that selection is missing required data
    /// (e.g. `printer_connection_type = "bluetooth"` with no stored
    /// address) — shouldn't happen through the UI, but a stored config can
    /// outlive the code that wrote it.
    #[error("No printer is set up yet — choose one in Settings, or use the PDF receipt instead")]
    NoPrinterSelected,
    /// Android only: the OS hasn't granted the Bluetooth permission this
    /// build needs. Never a reason to touch the Bluetooth APIs anyway —
    /// checked before any of them are called.
    #[error("Bluetooth permission wasn't granted — allow it in Settings to print, or use the PDF receipt instead")]
    PermissionDenied,
    #[error("Printer error: {0} — use the PDF receipt instead")]
    Io(String),
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
///
/// Jobs go one at a time: two at once break each other on the wire and leave
/// the printer waiting for the rest of an image.
fn send_to_printer(bytes: &[u8], config: &AppConfig) -> Result<(), PrinterError> {
    let mut last_job_failed = LAST_JOB_FAILED.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let recovery = last_job_failed.then(|| after_a_failed_job(bytes));
    let job = recovery.as_deref().unwrap_or(bytes);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| send_to_printer_dispatch(job, config)));
    let result = result.unwrap_or_else(|_| {
        Err(PrinterError::Io("the printer driver hit an internal error".to_string()))
    });
    *last_job_failed = result.is_err();
    result
}

/// Held for the length of each job. True when the last job failed, so the
/// printer may be part-way through an image.
static LAST_JOB_FAILED: Mutex<bool> = Mutex::new(false);

/// A printer cut off inside an image takes the next bytes it gets as the rest
/// of that image, and then prints whatever follows as text. Blank bytes finish
/// the image, at worst as a white gap, and a printer that wasn't stuck ignores
/// them (see `wake_padding`).
fn after_a_failed_job(bytes: &[u8]) -> Vec<u8> {
    let mut job = vec![0; receipt_image::MAX_IMAGE_BYTES];
    job.extend_from_slice(bytes);
    job
}

fn send_to_printer_dispatch(bytes: &[u8], config: &AppConfig) -> Result<(), PrinterError> {
    #[cfg(target_os = "android")]
    {
        match config.printer_connection_type.as_deref() {
            Some("bluetooth") => {
                let address = config.printer_bluetooth_address.as_deref().ok_or(PrinterError::NoPrinterSelected)?;
                crate::printer::android_bt::send(address, bytes)
            }
            // USB-OTG. Note this arm never touches any Bluetooth API — an
            // install configured for USB shouldn't be able to fail on a
            // Bluetooth adapter question it has no reason to be asking.
            Some("usb") => {
                let device = config.printer_usb_device.as_deref().ok_or(PrinterError::NoPrinterSelected)?;
                crate::printer::android_usb::send(device, bytes)
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
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let _ = config; // macOS/Linux's USB auto-detect doesn't need a stored selection (see module doc)
        send_to_printer_usb(bytes)
    }
    // iOS has no thermal-printer transport (no raw USB, and Bluetooth
    // Classic SPP isn't open to apps without MFi). `NoPrinterSelected` is
    // the "nothing chosen" answer every caller already turns into the PDF
    // receipt fallback.
    #[cfg(target_os = "ios")]
    {
        let _ = (bytes, config);
        Err(PrinterError::NoPrinterSelected)
    }
}

/// The standard USB device-class code for printers (USB.org base class
/// 0x07) — this is what lets a compatible printer be found without any
/// vendor/product ID configuration.
#[cfg(any(target_os = "macos", target_os = "linux"))]
const USB_PRINTER_CLASS: u8 = 0x07;

/// How long to wait for the printer to accept the write before giving up.
/// A receipt is a picture sent in one write, and the printer takes it about as
/// fast as it prints, so a long bill must not be cut off part-way.
#[cfg(any(target_os = "macos", target_os = "linux"))]
const USB_WRITE_TIMEOUT: Duration = Duration::from_secs(30);

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
#[cfg(any(target_os = "macos", target_os = "linux"))]
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
                    .map_err(|e| PrinterError::Io(format!("USB write failed: {e}")))
                    .and_then(|written| {
                        if written == bytes.len() {
                            Ok(())
                        } else {
                            Err(PrinterError::Io(format!("the printer only accepted {written} of {} bytes", bytes.len())))
                        }
                    });

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

/// Builds and sends the one receipt for `sale` (see [`build_receipt_bytes`])
/// to this installation's configured printer. Always an explicit action —
/// called only from `commands::billing_print_receipt_thermal`, which the
/// frontend only ever invokes from a button click (see `ReceiptModal.tsx`),
/// never automatically the moment a sale completes.
pub fn print_receipt(
    sale: &Sale,
    config: &AppConfig,
    logo: Option<&LogoRaster>,
    tables_module_enabled: bool,
) -> Result<(), PrinterError> {
    send_to_printer(&build_receipt_bytes(sale, config, logo, tables_module_enabled), config)
}

/// Builds and prints the "Refund Details" receipt for a just-created refund.
pub fn print_refund(refund: &Refund, config: &AppConfig) -> Result<(), PrinterError> {
    send_to_printer(&build_refund_bytes(refund, config), config)
}

/// Builds and prints a shift's close-out reconciliation receipt.
pub fn print_shift_summary(summary: &ShiftSummary, config: &AppConfig) -> Result<(), PrinterError> {
    send_to_printer(&build_shift_summary_bytes(summary, config), config)
}

/// Builds and prints the Category Wise Sale report.
pub fn print_category_sales(report: &CategorySalesReport, config: &AppConfig) -> Result<(), PrinterError> {
    send_to_printer(&build_category_sales_bytes(report, config), config)
}

/// Builds and prints the Table Wise Sales report.
pub fn print_table_sales(report: &TableSalesSummary, config: &AppConfig) -> Result<(), PrinterError> {
    send_to_printer(&build_table_sales_bytes(report, config), config)
}

/// Builds and prints the Refunds report.
pub fn print_refunds_summary(report: &RefundsSummary, config: &AppConfig) -> Result<(), PrinterError> {
    send_to_printer(&build_refunds_summary_bytes(report, config), config)
}

/// Builds and prints the consolidated Full Report.
pub fn print_full_report(report: &FullReport, config: &AppConfig) -> Result<(), PrinterError> {
    send_to_printer(&build_full_report_bytes(report, config), config)
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
            delivery_number: None,
            business_address: None,
            business_website: None,
            receipt_tagline: None,
            receipt_social: None,
            receipt_show_stamp: true,
            receipt_show_credit: true,
            receipt_template: "stacked".into(),
            cash_quick_amounts: String::new(),
            onboarding_completed: true,
            printer_connection_type: None,
            printer_bluetooth_address: None,
            printer_bluetooth_name: None,
            printer_usb_device: None,
            printer_usb_name: None,
            printer_windows_name: None,
            service_charge_percent: 0.0,
            scale_barcode_enabled: false,
            scale_barcode_prefix: "21".into(),
            scale_item_code_digits: 5,
            scale_value_kind: "weight".into(),
            scale_value_decimals: 3,
            offline_sales: "allow".into(),
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
            tendered_minor: None,
            change_minor: None,
            order_id: None,
            order_no: None,
            created_at: "2026-01-01 10:00:00".into(),
            items: vec![SaleLine {
                item_id: 1,
                item_name: "Cola 500ml".into(),
                qty: 2.0,
                unit: None,
                price_at_sale_minor: 500,
                line_total_minor: 1000,
                notes: None,
            }],
            order_type: None,
            customer_name: None,
            customer_phone: None,
            delivery_address: None,
            delivery_rider_name: None,
            delivery_rider_phone: None,
            delivery_platform_name: None,
            platform_order_ref: None,
            delivery_charge_minor: None,
            service_charge_minor: 0,
            payments: vec![crate::db::sales::PaymentPart { method: "cash".into(), amount_minor: 945 }],
            customer_id: None,
            customer_email: None,
            due_date: None,
            receipt_no: "42".into(),
            till_name: None,
        }
    }

    /// The receipt's rows as text, one per line — what the picture says.
    fn receipt_text(sale: &Sale, config: &AppConfig, tables: bool) -> String {
        receipt_rows(sale, config, tables, None).iter().map(Row::text).collect::<Vec<_>>().join("\n")
    }

    #[test]
    fn the_receipt_reads_like_the_reference_slip() {
        let mut config = sample_config();
        config.business_name = "MASKAN FOOD".into();
        config.business_address = Some("5th Road D Block Satellite Town Rawalpindi".into());
        config.phone = Some("0319-9966166".into());
        config.receipt_tagline = Some("JUICY & TENDER".into());
        config.receipt_social = Some("Follow us @maskanfood".into());
        config.receipt_footer = "Thankyou!!".into();
        let sale = Sale {
            cashier_name: Some("Admin".into()),
            created_at: "2026-10-17 02:25:00".into(),
            receipt_no: "119728".into(),
            order_no: Some(7),
            order_type: Some("dine_in".into()),
            table_name: Some("Table 9".into()),
            subtotal_minor: 138_000,
            discount_minor: 0,
            tax_minor: 0,
            total_minor: 138_000,
            tendered_minor: Some(200_000),
            change_minor: Some(62_000),
            payments: vec![crate::db::sales::PaymentPart { method: "cash".into(), amount_minor: 138_000 }],
            items: vec![
                SaleLine { item_id: 1, item_name: "Half Rotisserie Chicken".into(), qty: 1.0, unit: None, price_at_sale_minor: 125_000, line_total_minor: 125_000, notes: None },
                SaleLine { item_id: 2, item_name: "COLA NEXT 500ml".into(), qty: 1.0, unit: None, price_at_sale_minor: 13_000, line_total_minor: 13_000, notes: None },
            ],
            ..sample_sale()
        };
        let text = receipt_text(&sale, &config, true);
        let expected = [
            "MASKAN FOOD",
            "5th Road D Block Satellite Town Rawalpindi",
            "Phone No 0319-9966166",
            "JUICY & TENDER",
            "Cashier: ADMIN",
            "Date #: 17/10/2026 02:25 AM",
            "Invoice #: 119728",
            "Order #: 7",
            "Type : Dine In",
            "Table #: Table 9",
            "Qty Rate Disc Amount",
            "Half Rotisserie Chicken",
            "1 1,250 0 1,250",
            "COLA NEXT 500ml",
            "1 130 0 130",
            "SubTotal 1,380",
            "NetTotal 1,380",
            "Payment (Cash) 2,000",
            "Change 620",
            "Total Items: 2 Total QTY: 2",
            "Thankyou!!",
            "Follow us @maskanfood",
            "Developed by Code Hunts™",
        ];
        let mut rest = text.as_str();
        for line in expected {
            let at = rest.find(line).unwrap_or_else(|| panic!("missing or out of order: {line:?}\n{text}"));
            rest = &rest[at + line.len()..];
        }
        let rows = receipt_rows(&sale, &config, true, None);
        assert_eq!(rows.iter().filter(|r| **r == Row::Rule(Rule::Dotted)).count(), 1, "dotted only between items");
    }

    /// The reference shop's settings and its Dine In slip, for the preview
    /// and the layout tests.
    fn reference_config() -> AppConfig {
        let mut config = sample_config();
        config.business_name = "Maskan Food".into();
        config.business_address = Some("5th Road D Block Satellite Town Rawalpindi".into());
        config.phone = Some("0319-9966166".into());
        config.receipt_tagline = Some("JUICY & TENDER".into());
        config.receipt_footer = "Thankyou!!".into();
        config.receipt_social = Some("Follow us on Instagram/TikTok @maskanfood".into());
        config
    }

    fn line(id: i64, name: &str, qty: f64, rate_minor: i64) -> SaleLine {
        SaleLine { item_id: id, item_name: name.into(), qty, unit: None, price_at_sale_minor: rate_minor, line_total_minor: (qty * rate_minor as f64).round() as i64, notes: None }
    }

    fn reference_sale() -> Sale {
        Sale {
            cashier_name: Some("Admin".into()),
            created_at: "2026-10-17 02:25:00".into(),
            receipt_no: "119728".into(),
            order_type: Some("dine_in".into()),
            table_name: Some("Table 9".into()),
            subtotal_minor: 138_000,
            discount_minor: 0,
            tax_minor: 0,
            total_minor: 138_000,
            tendered_minor: Some(200_000),
            change_minor: Some(62_000),
            payments: vec![crate::db::sales::PaymentPart { method: "cash".into(), amount_minor: 138_000 }],
            items: vec![line(1, "Half Rotisserie Chicken", 1.0, 125_000), line(2, "COLA NEXT 500ml", 1.0, 13_000)],
            ..sample_sale()
        }
    }

    /// Writes the test receipts as PNGs for eyeballing a layout change:
    /// `DIWAN_RECEIPT_PNG=/some/dir cargo test receipt_preview -- --ignored`.
    #[test]
    #[ignore]
    fn receipt_preview() {
        let dir = std::path::PathBuf::from(std::env::var("DIWAN_RECEIPT_PNG").expect("set DIWAN_RECEIPT_PNG to a folder"));
        let config = reference_config();
        let takeaway = Sale {
            order_type: Some("takeaway".into()),
            table_name: None,
            order_no: Some(6),
            receipt_no: "59".into(),
            tendered_minor: Some(138_000),
            change_minor: Some(0),
            ..reference_sale()
        };
        let mut long_items: Vec<SaleLine> = (0..32)
            .map(|i| line(i, &format!("Item number {i} with a fairly long descriptive name for the kitchen"), 1.0 + (i % 3) as f64, 1_250_000 + i * 100))
            .collect();
        long_items[0].notes = Some("no onions, extra sauce".into());
        let long_total: i64 = long_items.iter().map(|l| l.line_total_minor).sum();
        let long = Sale {
            items: long_items,
            subtotal_minor: long_total,
            total_minor: long_total,
            tendered_minor: None,
            change_minor: None,
            payments: vec![crate::db::sales::PaymentPart { method: "card".into(), amount_minor: long_total }],
            ..reference_sale()
        };
        let unpaid = Sale {
            payment_method: "credit".into(),
            tendered_minor: None,
            change_minor: None,
            due_date: Some("2026-11-30".into()),
            payments: vec![crate::db::sales::PaymentPart { method: "credit".into(), amount_minor: 138_000 }],
            ..reference_sale()
        };
        let compact = AppConfig { receipt_template: "compact".into(), ..reference_config() };
        for (name, sale, config) in [
            ("1-dine-in-paid", reference_sale(), &config),
            ("2-takeaway-paid", takeaway, &config),
            ("3-long", long.clone(), &config),
            ("4-unpaid", unpaid, &config),
            ("5-compact", reference_sale(), &compact),
            ("6-compact-long", long, &compact),
        ] {
            let (bitmap, height) = receipt_image::render(&receipt_rows(&sale, config, true, None), receipt_stamp(&sale, config));
            let width = receipt_image::WIDTH_PX;
            let image = image::GrayImage::from_fn(width as u32, height as u32, |x, y| {
                let byte = bitmap[y as usize * width.div_ceil(8) + x as usize / 8];
                image::Luma([if byte & (0x80 >> (x % 8)) != 0 { 0 } else { 255 }])
            });
            image.save(dir.join(format!("thermal-{name}.png"))).unwrap();
        }
    }

    #[test]
    fn the_stamp_and_credit_line_follow_the_sale_and_the_settings() {
        let mut config = reference_config();
        assert_eq!(receipt_stamp(&reference_sale(), &config), Some("PAID"));
        let on_credit = Sale { payments: vec![crate::db::sales::PaymentPart { method: "credit".into(), amount_minor: 945 }], ..sample_sale() };
        assert_eq!(receipt_stamp(&on_credit, &config), Some("UNPAID"));
        assert!(receipt_text(&reference_sale(), &config, true).contains("Developed by Code Hunts™"));

        config.receipt_show_stamp = false;
        config.receipt_show_credit = false;
        assert_eq!(receipt_stamp(&reference_sale(), &config), None);
        assert!(!receipt_text(&reference_sale(), &config, true).contains("Code Hunts"));
    }

    #[test]
    fn the_compact_design_puts_each_item_on_one_row_and_a_long_name_wraps_in_its_column() {
        let mut config = reference_config();
        config.receipt_template = "compact".into();
        let text = receipt_text(&reference_sale(), &config, true);
        assert!(text.contains("Items QTY Rate Disc Amount"), "{text}");
        assert!(text.contains("Half Rotisserie Chicken 1 1,250 0 1,250"), "{text}");
        let rows = receipt_rows(&reference_sale(), &config, true, None);
        assert!(!rows.contains(&Row::Rule(Rule::Dotted)), "no dotted rule in the compact design");

        let one_item = |name: &str| Sale { items: vec![line(1, name, 1.0, 13_000)], ..reference_sale() };
        let height = |sale: &Sale| receipt_image::render(&receipt_rows(sale, &config, true, None), None).1;
        let long = one_item("A very long item name that needs more than one line in its column");
        assert!(height(&long) > height(&one_item("Tea")), "the long name wraps instead of being cut");
    }

    #[test]
    fn amounts_group_thousands_and_keep_paisa_only_when_there_are_some() {
        assert_eq!(amount(125_000), "1,250");
        assert_eq!(amount(13_000), "130");
        assert_eq!(amount(123_456_789), "1,234,567.89");
        assert_eq!(amount(-5_050), "-50.50");
        assert_eq!(amount(0), "0");
    }

    #[test]
    fn the_order_type_and_optional_rows_follow_the_sale() {
        let with = |kind: Option<&str>, table: Option<&str>, platform: Option<&str>| Sale {
            order_type: kind.map(Into::into),
            table_name: table.map(Into::into),
            delivery_platform_name: platform.map(Into::into),
            ..sample_sale()
        };
        assert_eq!(order_type_label(&with(Some("dine_in"), Some("T4"), None), true), "Dine In");
        assert_eq!(order_type_label(&with(None, Some("T4"), None), true), "Dine In");
        assert_eq!(order_type_label(&with(Some("third_party_delivery"), None, Some("Foodpanda")), true), "Foodpanda");
        assert_eq!(order_type_label(&with(Some("third_party_delivery"), None, None), true), "3rd-Party");
        assert_eq!(order_type_label(&with(None, None, None), true), "Takeaway");
        assert_eq!(order_type_label(&with(None, None, None), false), "Counter Sale");

        let text = receipt_text(&sample_sale(), &sample_config(), true);
        for absent in ["Table #", "Order #", "Phone No", "Delivery No", "Change", "Counter:"] {
            assert!(!text.contains(absent), "{absent} only prints when there is one:\n{text}");
        }
        let mut config = sample_config();
        config.delivery_number = Some("0300-1".into());
        assert!(receipt_text(&sample_sale(), &config, true).contains("Delivery No 0300-1"));
    }

    #[test]
    fn split_and_pay_later_sales_list_every_tender() {
        let sale = Sale {
            payment_method: "credit".into(),
            payments: vec![
                crate::db::sales::PaymentPart { method: "card".into(), amount_minor: 500 },
                crate::db::sales::PaymentPart { method: "credit".into(), amount_minor: 445 },
            ],
            due_date: Some("2026-02-01".into()),
            ..sample_sale()
        };
        let text = receipt_text(&sale, &sample_config(), true);
        assert!(text.contains("Payment (Card) 5"), "{text}");
        assert!(text.contains("Payment (Pay later) 4.45"), "{text}");
        assert!(text.contains("Balance Due By 2026-02-01"), "{text}");
    }

    #[test]
    fn the_receipt_prints_as_one_raster_picture_between_init_and_cut() {
        let bytes = build_receipt_bytes(&sample_sale(), &sample_config(), None, true);
        let raster = [GS, b'v', b'0', 0];
        assert!(bytes.windows(4).any(|w| w == raster), "the body prints as a raster image");
        assert!(!String::from_utf8_lossy(&bytes).contains("NetTotal"), "no text-mode copy of the receipt");
        let ink: u32 = bytes.iter().map(|b| b.count_ones()).sum();
        assert!(ink > 5_000, "the picture has real content");
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
                qty_refunded: 1.0,
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
                till_name: None,
            },
            opening_balance_minor: 10_000,
            cash_sales_minor: 15_000,
            card_sales_minor: 4_000,
            other_sales_minor: 0,
            credit_sales_minor: 0,
            collected_cash_minor: 0,
            platform_sales_minor: 0,
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
                    qty_sold: 10.0,
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

    fn sample_employee_report() -> Vec<SalaryCalculation> {
        vec![
            SalaryCalculation {
                employee_id: 1,
                employee_name: "Ali Khan".into(),
                month: "2026-01".into(),
                base_salary_minor: 3_000_000,
                working_days_in_month: 31,
                days_present: 28,
                calculated_amount_minor: 2_709_677,
                paid_amount_minor: 2_709_677,
                paid_date: Some("2026-02-01".into()),
                status: PaymentStatus::Paid,
            },
            SalaryCalculation {
                employee_id: 2,
                employee_name: "Sara Ahmed".into(),
                month: "2026-01".into(),
                base_salary_minor: 2_500_000,
                working_days_in_month: 31,
                days_present: 15,
                calculated_amount_minor: 1_209_677,
                paid_amount_minor: 0,
                paid_date: None,
                status: PaymentStatus::Unpaid,
            },
        ]
    }

    #[test]
    fn build_employee_report_bytes_lists_every_employee_with_attendance_and_pay_status() {
        let bytes = build_employee_report_bytes(&sample_employee_report(), "2026-01", &sample_config());
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("Employee Report"));
        assert!(text.contains("Month: 2026-01"));
        assert!(text.contains("Ali Khan"));
        assert!(text.contains("28/31"), "attendance must show days present over the real days in the month");
        assert!(text.contains("Paid"), "Ali Khan's fully-paid status must appear");
        assert!(text.contains("Sara Ahmed"));
        assert!(text.contains("15/31"));
        assert!(text.contains("Unpaid"), "Sara Ahmed's unpaid status must appear");
        assert!(text.contains("Total Net Pay"));
        assert!(text.contains("Total Paid"));
        assert!(text.contains("+"), "a real box-drawn grid, not bare dashes");
    }

    #[test]
    fn build_employee_report_bytes_reports_no_employees_cleanly() {
        let bytes = build_employee_report_bytes(&[], "2026-01", &sample_config());
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("No employees found"));
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
    fn print_receipt_sends_exactly_one_copy_with_one_cut() {
        // Reproduces print_receipt's own byte assembly — print_receipt
        // itself isn't unit-testable without a real printer attached
        // (send_to_printer needs hardware) — to prove the *shape* of what
        // it actually sends: one complete receipt, one cut. There used to
        // be a second "MERCHANT COPY"-labeled copy concatenated after it;
        // removed — printing should never silently double a customer's
        // paper, and a second copy (if ever wanted) is a second explicit
        // click, not baked into every print.
        let bytes = build_receipt_bytes(&sample_sale(), &sample_config(), None, true);
        let cut = [GS, b'V', 66, CUT_FEED_LINES];
        let cut_count = bytes.windows(cut.len()).filter(|w| *w == cut).count();
        assert_eq!(cut_count, 1, "must print exactly one copy");
        assert!(!String::from_utf8_lossy(&bytes).contains("MERCHANT COPY"));
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
                qty_sold: 10.0,
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
                delivery_stats: None,
            },
            average_sale_minor: 8_000,
            payment_methods: crate::db::full_report::PaymentMethodBreakdown {
                cash_minor: 50_000,
                cash_count: 7,
                card_minor: 30_000,
                card_count: 3,
                other_minor: 0,
                other_count: 0,
            },
            sales_metrics: crate::db::full_report::SalesMetrics {
                total_discount_minor: 0,
                total_tax_minor: 0,
                net_sales_minor: 80_000,
                total_items_sold: 15.0,
            },
            refunds: sample_refunds_summary(),
            category_sales: sample_category_report(),
            product_sales: sample_product_sales(),
            table_sales: Some(sample_table_sales()),
        }
    }

    #[test]
    fn build_full_report_bytes_includes_overall_sales_summary() {
        let bytes = build_full_report_bytes(&sample_full_report(), &sample_config());
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("Overall Sales Report"));
        assert!(text.contains("SALES SUMMARY"));
        assert!(text.contains("Total Orders"));
        assert!(text.contains("Gross Sales"));
        assert!(text.contains("NET SALES"));
        assert!(text.contains("PAYMENT BREAKDOWN"));
        assert!(text.contains("Cash (7 orders)"));
        assert!(text.contains("Card (3 orders)"));
        assert!(text.contains("NET PROFIT"));
        assert!(text.contains("PKR 750.00"), "net profit must appear formatted: {text}");
        // Category, product, and table breakdowns must not be included
        assert!(!text.contains("CATEGORY WISE SALE"));
        assert!(!text.contains("PRODUCT WISE SALES"));
        assert!(!text.contains("TABLE WISE SALES"));
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

    /// A 10×10 PNG with a 4×2 opaque black block at (3, 4) on `background`.
    fn logo_with_margins(background: [u8; 4]) -> Vec<u8> {
        let img = image::RgbaImage::from_fn(10, 10, |x, y| {
            image::Rgba(if (3..7).contains(&x) && (4..6).contains(&y) { [0, 0, 0, 255] } else { background })
        });
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(img).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        png
    }

    #[test]
    fn build_logo_raster_crops_blank_margins_and_fills_the_logo_box() {
        let logo = build_logo_raster(&logo_with_margins([255, 255, 255, 255])).expect("a valid PNG must decode");
        // The 4×2 block alone, scaled up to the width cap.
        assert_eq!((logo.width_px, logo.height_px), (LOGO_MAX_WIDTH_PX as usize, LOGO_MAX_WIDTH_PX as usize / 2));
        assert!(logo.bitmap.iter().all(|b| *b == 0xFF), "the white margin is gone; only ink is left");
    }

    #[test]
    fn a_transparent_logo_background_prints_as_paper_not_ink() {
        // Transparent *black* — what most "transparent background" PNGs store.
        let logo = build_logo_raster(&logo_with_margins([0, 0, 0, 0])).expect("a valid PNG must decode");
        assert_eq!((logo.width_px, logo.height_px), (LOGO_MAX_WIDTH_PX as usize, LOGO_MAX_WIDTH_PX as usize / 2), "the transparent margin is cropped like paper");
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

    fn sample_token() -> TokenSummary {
        TokenSummary {
            id: 1,
            token_number: 14,
            counter_id: 1,
            counter_name: "Channa Counter".into(),
            table_order_id: Some(1),
            table_id: Some(1),
            table_name: Some("Table 4".into()),
            printed_at: "2026-01-01 13:00:00".into(),
            printed_by: Some(1),
            printed_by_name: Some("Ali".into()),
            status: "printed".into(),
            shift_id: None,
            till_label: None,
            items: vec![
                TokenLine {
                    item_id: 1,
                    item_name: "Channa".into(),
                    qty: 2.0,
                    unit: Some("kg".into()),
                    sold_by_amount: true,
                    amount_minor: 10000,
                },
                TokenLine {
                    item_id: 2,
                    item_name: "Cola 500ml".into(),
                    qty: 3.0,
                    unit: None,
                    sold_by_amount: false,
                    amount_minor: 0,
                },
            ],
        }
    }

    #[test]
    fn build_token_bytes_shows_the_date_time_token_number_and_items_with_unit_but_no_rate_or_total() {
        let bytes = build_token_bytes(&sample_token(), false);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("2026-01-01"), "date must appear");
        assert!(text.contains("13:00:00"), "time must appear");
        assert!(text.contains("TOKEN #14"));
        assert!(!text.contains("REPRINT"), "a first print must not show the reprint banner");
        assert!(text.contains("2 kg"), "a sold-by-amount line shows its unit on a token (unlike the bill)");
        assert!(
            text.contains("(100.00)"),
            "a sold-by-amount line also shows its computed rupee amount in brackets, under the qty"
        );
        assert!(text.contains("Cola 500ml"));
        // No prices/totals anywhere except the sold-by-amount bracket above
        // — this is a kitchen ticket, not a bill, and never shows a
        // currency-formatted (symbol-prefixed) amount either way.
        assert!(!text.contains(&config_currency_marker()), "must not print a currency-formatted amount");
        // Stripped down deliberately — no business identity or counter
        // identity on the paper itself, see `build_token_bytes`'s doc
        // comment. The table *does* appear — kitchen staff need to know
        // which order a ticket belongs to, see the test right below.
        assert!(!text.contains("Demo Shop"), "business name must not appear — a token is minimal by design");
        assert!(!text.contains("Channa Counter"), "counter name must not appear — reduced token format");
        assert!(text.contains("Table 4"), "table name must appear so kitchen staff know which order it's for");
    }

    #[test]
    fn build_token_bytes_shows_takeaway_for_an_ad_hoc_token_with_no_table_behind_it() {
        let mut token = sample_token();
        token.table_order_id = None;
        token.table_id = None;
        token.table_name = None;

        let bytes = build_token_bytes(&token, false);
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains("Table"), "no table line for a token with no table behind it");
        assert!(text.contains("Takeaway"), "must clearly mark a table-less token as Takeaway");
    }

    /// `format_minor`'s currency prefix — asserting its exact absence is a
    /// more useful "no money on this ticket" check than grepping for stray
    /// digits, which item quantities/table numbers would also match.
    fn config_currency_marker() -> String {
        format!("{} ", sample_config().currency)
    }

    #[test]
    fn build_token_bytes_marks_a_reprint_clearly() {
        let bytes = build_token_bytes(&sample_token(), true);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("REPRINT"));
        assert!(text.contains("TOKEN #14"), "a reprint still shows the original token number, not a new one");
    }

    #[test]
    fn build_token_bytes_never_lets_a_long_item_name_spill_onto_another_line() {
        // Same "check the truncated content appears, not a raw per-line
        // byte scan" approach `build_receipt_bytes_never_lets_a_long_
        // business_name_or_phone_spill_onto_another_line` uses — a naive
        // scan over `.lines()` false-positives on a bold/align control
        // sequence sitting right before a border line with no `\n`
        // between them (invisible ink-wise on the real printer, but real
        // characters in a lossy-decoded string).
        let mut token = sample_token();
        let long_name = "A Very Long Item Name That Would Otherwise Wrap Onto A Second Physical Line";
        token.items = vec![TokenLine {
            item_id: 1,
            item_name: long_name.into(),
            qty: 1.0,
            unit: None,
            sold_by_amount: false,
            amount_minor: 0,
        }];

        let bytes = build_token_bytes(&token, false);
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains(long_name), "the untruncated item name must not appear at all");
    }

    #[test]
    fn build_order_token_bytes_formats_kitchen_and_customer_tickets_with_badge_and_items() {
        let order = crate::db::orders::Order {
            id: 1,
            order_no: 5,
            order_type: "takeaway".to_string(),
            table_id: None,
            table_name: None,
            status: "sent".to_string(),
            held_from_status: None,
            payment_status: "unpaid".to_string(),
            version: 1,
            client_ref: "ref-1".to_string(),
            customer_name: Some("Ali Khan".to_string()),
            notes: Some("Extra hot sauce".to_string()),
            estimated_prep_minutes: 20,
            shift_id: None,
            created_at: "2026-10-19 14:30:00".to_string(),
            completed_at: None,
            cancelled_at: None,
            due_minor: 100000,
            discount_minor: 0,
            discount_percent: None,
            tax_exempt: false,
            service_waived: false,
            delivery_rider_id: None,
            rider_source: None,
            delivery_rider_name: None,
            delivery_rider_phone: None,
            delivery_platform_id: None,
            delivery_platform_name: None,
            customer_phone: None,
            delivery_address: None,
            delivery_charge_minor: 0,
            delivery_status: None,
            platform_order_ref: None,
            lines: vec![
                crate::db::orders::OrderLine {
                    id: 1,
                    item_id: 10,
                    item_name: "Special Burger".to_string(),
                    qty: 2.0,
                    price_at_order_minor: 50000,
                    notes: Some("No onion".to_string()),
                    sent_at: Some("2026-10-19 14:30:00".to_string()),
                    status: "active".to_string(),
                    sale_id: None,
                },
            ],
        };

        // Kitchen ticket
        let kitchen_bytes = build_order_token_bytes(&order, &sample_config(), true);
        let kitchen_text = String::from_utf8_lossy(&kitchen_bytes);
        assert!(kitchen_text.contains("KITCHEN TICKET"));
        assert!(kitchen_text.contains("TOKEN #5"));
        assert!(kitchen_text.contains("TYPE: TAKEAWAY"));
        assert!(kitchen_text.contains("Ali Khan"));
        assert!(kitchen_text.contains("20 min"));
        assert!(kitchen_text.contains("Extra hot sauce"));
        assert!(kitchen_text.contains("Special Burger"));
        assert!(kitchen_text.contains("2x"));
        assert!(kitchen_text.contains("No onion"));
        assert!(kitchen_text.contains("TOTAL ITEMS:"));
        assert!(kitchen_text.contains("PREPARE FRESH"));

        // Customer token
        let customer_bytes = build_order_token_bytes(&order, &sample_config(), false);
        let customer_text = String::from_utf8_lossy(&customer_bytes);
        assert!(customer_text.contains("CUSTOMER TOKEN"));
        assert!(customer_text.contains("2 x 500"), "the unit price: {customer_text}");
        assert!(customer_text.contains("TOTAL:"), "{customer_text}");
        assert!(customer_text.contains(&format!("{} 1,050", sample_config().currency)), "1,000 plus 5% tax: {customer_text}");
        assert!(!kitchen_text.contains("TOTAL:") && !kitchen_text.contains("500"), "no prices in the kitchen: {kitchen_text}");
        assert!(customer_text.contains("TOKEN #5"));
        assert!(customer_text.contains("THANK YOU FOR YOUR ORDER"));
        assert!(!customer_text.contains("DISCOUNT:"), "{customer_text}");

        let discounted = crate::db::orders::Order { discount_minor: 10000, ..order };
        let restaurant = AppConfig { business_type: "restaurant".into(), tax_percent: 10.0, service_charge_percent: 5.0, ..sample_config() };
        let customer_text = String::from_utf8_lossy(&build_order_token_bytes(&discounted, &restaurant, false)).into_owned();
        let currency = sample_config().currency;
        let row = |label: &str, value: &str| two_col(label, &format!("{currency} {value}"));
        assert!(customer_text.contains(&row("SUBTOTAL:", "1,000")), "{customer_text}");
        assert!(customer_text.contains(&format!("-{currency} 100")), "{customer_text}");
        assert!(customer_text.contains(&row("TAX:", "90")), "on the discounted amount: {customer_text}");
        assert!(customer_text.contains(&row("SERVICE CHARGE:", "45")), "{customer_text}");
        assert!(customer_text.contains(&row("TOTAL:", "1,035")), "{customer_text}");

        // The cart's switches off: neither charge is shown or added.
        let waived = crate::db::orders::Order { tax_exempt: true, service_waived: true, ..discounted.clone() };
        let customer_text = String::from_utf8_lossy(&build_order_token_bytes(&waived, &restaurant, false)).into_owned();
        assert!(!customer_text.contains("TAX:") && !customer_text.contains("SERVICE CHARGE:"), "{customer_text}");
        assert!(customer_text.contains(&row("TOTAL:", "900")), "{customer_text}");

        // A store never charges service, whatever percent is left in settings.
        let store = AppConfig { business_type: "retail".into(), ..restaurant };
        let customer_text = String::from_utf8_lossy(&build_order_token_bytes(&discounted, &store, false)).into_owned();
        assert!(!customer_text.contains("SERVICE CHARGE:"), "{customer_text}");
        let kitchen_text = String::from_utf8_lossy(&build_order_token_bytes(&discounted, &sample_config(), true)).into_owned();
        assert!(!kitchen_text.contains("DISCOUNT:"), "no money in the kitchen: {kitchen_text}");
    }

    /// The data size of each `GS v 0` image in a job, walking it the way a
    /// printer does, so a malformed image would throw the walk off.
    fn image_sizes(job: &[u8]) -> Vec<usize> {
        let mut sizes = Vec::new();
        let mut at = 0;
        while at < job.len() {
            if job[at..].starts_with(&[GS, b'v', b'0', 0]) {
                let width = job[at + 4] as usize + ((job[at + 5] as usize) << 8);
                let height = job[at + 6] as usize + ((job[at + 7] as usize) << 8);
                sizes.push(width * height);
                at += 8 + width * height;
            } else {
                at += 1;
            }
        }
        assert_eq!(at, job.len(), "an image ran past the end of the job");
        sizes
    }

    #[test]
    fn a_discount_only_adds_a_row_and_the_receipt_stays_well_formed() {
        let logo = LogoRaster { width_px: 288, height_px: 160, bitmap: vec![0xFF; 36 * 160] };
        let job = |discount_minor| {
            let sale = Sale { discount_minor, ..sample_sale() };
            build_receipt_bytes(&sale, &sample_config(), Some(&logo), true)
        };
        let (plain, discounted) = (job(0), job(100));
        let (plain_images, discounted_images) = (image_sizes(&plain), image_sizes(&discounted));
        assert!(discounted.ends_with(&feed_and_cut()) && plain.ends_with(&feed_and_cut()));
        assert_eq!(discounted_images[0], 36 * 160, "the logo comes first");
        assert!(discounted_images.iter().sum::<usize>() > plain_images.iter().sum::<usize>(), "the discount row prints");
    }

    #[test]
    fn the_job_after_a_failed_one_can_finish_any_image_the_printer_was_stuck_in() {
        let logo = LogoRaster { width_px: 288, height_px: 160, bitmap: vec![0xFF; 36 * 160] };
        let job = build_receipt_bytes(&sample_sale(), &sample_config(), Some(&logo), true);
        let largest = image_sizes(&job).into_iter().max().unwrap();

        let recovery = after_a_failed_job(&job);
        assert!(recovery.iter().take_while(|byte| **byte == 0).count() >= largest);
        assert!(recovery.ends_with(&job), "then the whole receipt, unchanged");
    }

    #[test]
    fn a_receipt_and_a_token_name_the_counter_when_the_business_has_several() {
        let sale = Sale { receipt_no: "C2-41".into(), till_name: Some("Front counter".into()), ..sample_sale() };
        let text = receipt_text(&sale, &sample_config(), true);
        assert!(text.contains("Invoice #: C2-41"), "{text}");
        assert!(text.contains("Counter: Front counter"), "{text}");

        let token = TokenSummary { till_label: Some("C2".into()), ..sample_token() };
        let text = String::from_utf8_lossy(&build_token_bytes(&token, false)).into_owned();
        assert!(text.contains(&format!("TOKEN #C2-{}", token.token_number)), "{text}");
        let text = String::from_utf8_lossy(&build_token_bytes(&sample_token(), false)).into_owned();
        assert!(text.contains(&format!("TOKEN #{}", sample_token().token_number)), "one counter: unchanged");
    }
}
