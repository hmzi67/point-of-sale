//! Shared ESC/POS text-layout primitives — column alignment, dividers, money
//! formatting — used by every template in `printer::escpos` (customer
//! receipt, refund receipt, shift close-out, category-wise sale) so each one
//! only has to describe *what* goes in each row, not re-derive how to pad a
//! column to line up on 80mm paper.
//!
//! `LINE_WIDTH` — the character columns per line at Font A on 80mm thermal
//! paper — used to be 48, the number genuine Epson TM-series printers use
//! and the one every ESC/POS spec sheet quotes. It was wrong for real
//! client hardware: a generic/clone 80mm thermal printer (common on the
//! low-spec, local-market hardware this app targets — see `projectGoal.md`)
//! physically prints fewer columns at its default font, because its
//! margins eat more of the 80mm width than the spec implies. 48 was never
//! measured against real hardware; it was copied from a datasheet.
//!
//! The current value, 42, *was* measured — against a physical receipt from
//! an affected client printer where every row (four independently-built
//! layouts: a `two_col` row, a bordered table header, a bordered table
//! data row, another `two_col` row) broke at the exact same column, cutting
//! `3900.00` into `39` / `00.00` mid-digit — proof the printer was hard-
//! wrapping at a fixed dot position, not word-wrapping. `printer::escpos`'s
//! test suite includes that exact row (item "mutton paye (full)", qty 3,
//! rate 1300.00, amount 3900.00) as a regression case so this can't quietly
//! drift back to an unmeasured number. See `printer::escpos::send` for the
//! diagnostic-print command that exists so a *different* printer's real
//! width can be measured the same way instead of guessed at again.
//!
//! `receiptPdf.ts`'s `PAGE_WIDTH_MM = 80` on the PDF side is unaffected —
//! it lays out proportionally in millimeters, not a fixed character grid,
//! so it has no equivalent hardcoded-columns assumption to get wrong.

/// Character columns per line — see the module doc comment above for where
/// this number actually comes from (a measured printer, not a datasheet).
pub const LINE_WIDTH: usize = 42;

/// Right-hand column width used by [`two_col`] — wide enough for
/// `"PKR 123456.78"`-scale amounts with a couple of characters of breathing
/// room, without eating too much of the line back from the label. Shrunk
/// alongside `LINE_WIDTH`'s 48→42 correction, proportionally.
const VALUE_WIDTH: usize = 14;

/// Left/right-pads `text` to exactly `width` visible characters, truncating
/// (never panicking on a multi-byte boundary) if it doesn't fit — a column
/// must stay a fixed width for the rest of the row to line up, so a value
/// too long to fit is cut off rather than pushed onto its own line.
fn pad(text: &str, width: usize, right_align: bool) -> String {
    let truncated: String = text.chars().take(width).collect();
    let pad_len = width.saturating_sub(truncated.chars().count());
    let padding = " ".repeat(pad_len);
    if right_align {
        format!("{padding}{truncated}")
    } else {
        format!("{truncated}{padding}")
    }
}

/// A full-width divider line — the section separator every template uses
/// between, e.g., line items and totals.
pub fn divider() -> String {
    "-".repeat(LINE_WIDTH)
}

/// A double-ruled divider — reserved for the strongest separation a
/// template needs (above a grand total), so it reads as heavier than a
/// plain [`divider`] without needing bold toggling mid-line.
pub fn double_divider() -> String {
    "=".repeat(LINE_WIDTH)
}

/// A dashed divider: a dash every other column, so it reads as a lighter
/// separation than [`divider`]'s solid run without dropping to invisibility.
/// A fixed character grid has no line weight to vary — the *density* of ink
/// along the row is the only lever this trades in.
pub fn dashed_divider() -> String {
    (0..LINE_WIDTH).map(|i| if i % 2 == 0 { '-' } else { ' ' }).collect()
}

/// `label` left-aligned, `value` right-aligned in the last [`VALUE_WIDTH`]
/// columns — the "Subtotal ... 123.45"-style row every totals block uses.
pub fn two_col(label: &str, value: &str) -> String {
    let label_width = LINE_WIDTH - VALUE_WIDTH;
    format!("{}{}", pad(label, label_width, false), pad(value, VALUE_WIDTH, true))
}

/// A row of arbitrary columns, each `(text, width, right_align)`. Widths are
/// the caller's responsibility to sum to [`LINE_WIDTH`] (they're constants
/// per template, not computed here) — this only handles padding/truncating
/// each cell.
pub fn row(cols: &[(&str, usize, bool)]) -> String {
    cols.iter().map(|(text, width, right_align)| pad(text, *width, *right_align)).collect()
}

/// One row of an ASCII-bordered table: `|` before the first column and
/// after every column, e.g. `|Cola x2   |  8000|`. Thermal paper has no
/// concept of a CSS border, so a "grid line" has to be drawn as literal
/// `|`/`-`/`+` characters that print like any other text — this is the
/// column-divider half of that, [`bordered_line`] the row-divider half.
/// Same `(text, width, right_align)` contract as [`row`].
pub fn bordered_row(cols: &[(&str, usize, bool)]) -> String {
    let mut out = String::from("|");
    for (text, width, right_align) in cols {
        out.push_str(&pad(text, *width, *right_align));
        out.push('|');
    }
    out
}

/// A `+----+----+` horizontal rule matching [`bordered_row`]'s column
/// widths — call it above the header row, below the header row, and below
/// the last data row to close the grid on every side.
pub fn bordered_line(widths: &[usize]) -> String {
    let mut out = String::from("+");
    for w in widths {
        out.push_str(&"-".repeat(*w));
        out.push('+');
    }
    out
}

// --- CP437 box-drawing borders ------------------------------------------
//
// [`bordered_row`]/[`bordered_line`] above draw a "grid" out of plain
// ASCII (`|`/`-`/`+`) — readable, but it reads as typed punctuation, not
// an actual table. Real box-drawing lines (─│┌┬┐├┼┤└┴┘) are available on
// virtually every ESC/POS thermal printer as part of the IBM/DOS CP437
// character set, one byte each (0xB3/0xC4/0xDA/…) — but they are *not*
// valid standalone UTF-8, so `str`/`String` can't hold them; these
// functions build raw `Vec<u8>` instead of text, unlike everything above.
// The printer must actually be in codepage CP437 for these bytes to render
// as lines rather than accented-letter garbage — `escpos::init()` selects
// it (`ESC t 0`) once at the start of every print job, which is harmless
// for plain ASCII text too since CP437's low 128 codepoints are identical
// to ASCII.
const BOX_H: u8 = 0xC4; // ─
const BOX_V: u8 = 0xB3; // │
const BOX_TOP_LEFT: u8 = 0xDA; // ┌
const BOX_TOP_MID: u8 = 0xC2; // ┬
const BOX_TOP_RIGHT: u8 = 0xBF; // ┐
const BOX_MID_LEFT: u8 = 0xC3; // ├
const BOX_MID_MID: u8 = 0xC5; // ┼
const BOX_MID_RIGHT: u8 = 0xB4; // ┤
const BOX_BOTTOM_LEFT: u8 = 0xC0; // └
const BOX_BOTTOM_MID: u8 = 0xC1; // ┴
const BOX_BOTTOM_RIGHT: u8 = 0xD9; // ┘

/// Which horizontal rule of a box-drawn table this is — the corner/junction
/// glyphs differ by position (`┌┬┐` vs `├┼┤` vs `└┴┘`), unlike
/// [`bordered_line`]'s single `+` used everywhere.
pub enum BoxLinePosition {
    Top,
    Middle,
    Bottom,
}

/// The CP437 equivalent of [`bordered_line`]: a real horizontal box-drawing
/// rule matching [`box_row`]'s column widths. Call it with `Top` above the
/// header row, `Middle` between the header and the first data row, and
/// `Bottom` below the last data row — the same three call sites
/// `bordered_line` used to cover with plain `+`/`-`.
pub fn box_line(widths: &[usize], position: BoxLinePosition) -> Vec<u8> {
    let (left, mid, right) = match position {
        BoxLinePosition::Top => (BOX_TOP_LEFT, BOX_TOP_MID, BOX_TOP_RIGHT),
        BoxLinePosition::Middle => (BOX_MID_LEFT, BOX_MID_MID, BOX_MID_RIGHT),
        BoxLinePosition::Bottom => (BOX_BOTTOM_LEFT, BOX_BOTTOM_MID, BOX_BOTTOM_RIGHT),
    };
    let mut out = vec![left];
    for (i, width) in widths.iter().enumerate() {
        out.extend(std::iter::repeat(BOX_H).take(*width));
        out.push(if i + 1 == widths.len() { right } else { mid });
    }
    out
}

/// The CP437 equivalent of [`bordered_row`]: `│` before the first column
/// and after every column. Same `(text, width, right_align)` contract, and
/// the same underlying [`pad`] (truncate-not-wrap, never panics on a
/// multi-byte boundary) for each cell's content.
pub fn box_row(cols: &[(&str, usize, bool)]) -> Vec<u8> {
    let mut out = vec![BOX_V];
    for (text, width, right_align) in cols {
        out.extend(pad(text, *width, *right_align).into_bytes());
        out.push(BOX_V);
    }
    out
}

/// Truncates `text` to at most [`LINE_WIDTH`] visible characters, exactly
/// as it already is if it's shorter — for the handful of values that print
/// as their own full-width line rather than a fixed-width column (business
/// name, phone, footer, refund reason, category name, banner titles): free
/// text with no length limit of its own, so without this a long value
/// would run past the printer's physical line width and get pushed onto a
/// second physical line — the exact `pad`-truncates-rather-than-wraps
/// convention every column in this file already follows, applied to a
/// whole line instead of one column of one.
pub fn truncate_line(text: &str) -> String {
    text.chars().take(LINE_WIDTH).collect()
}

/// A quantity -> a plain string, rounded to 2 decimal places and with
/// trailing zeros trimmed ("2" for a whole unit, "0.77" for a
/// `sold_by_amount` line) — the printed counterpart of the cart's own
/// display rounding. Never shows a unit label (kg/g/…): that needs more
/// horizontal room than this file's narrow, hardware-measured column
/// budgets reliably have (see `LINE_WIDTH`'s doc comment on how little
/// slack 42 columns actually leaves) — the PDF receipt shows the unit,
/// this one doesn't.
pub fn format_qty(qty: f64) -> String {
    let rounded = (qty * 100.0).round() / 100.0;
    if rounded.fract() == 0.0 {
        format!("{rounded:.0}")
    } else {
        // Trim a trailing zero from e.g. "1.20" -> "1.2", never from "1.25".
        let s = format!("{rounded:.2}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Minor units -> a plain "12.34" string, without a currency symbol (every
/// template prints the symbol once per section, not per line).
pub fn format_minor(minor: i64) -> String {
    format!("{}.{:02}", minor / 100, (minor % 100).abs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dashed_rule_fills_exactly_one_line_and_stays_lighter_than_a_solid_divider() {
        // A rule wider than LINE_WIDTH would wrap onto a second physical
        // line on the printer — the one failure mode these can have.
        assert_eq!(dashed_divider().chars().count(), LINE_WIDTH, "the dotted rule must fill exactly one line");
        // Ink density is the only "line weight" a fixed character grid has,
        // so the ordering solid > dotted > light dotted is what actually
        // makes these read as a hierarchy on paper.
        let ink = |s: &str| s.chars().filter(|c| !c.is_whitespace()).count();
        assert!(
            ink(&divider()) > ink(&dashed_divider()),
            "a dashed rule must be lighter than a solid one"
        );
    }

    #[test]
    fn two_col_lines_up_the_value_at_the_right_edge() {
        let line = two_col("Subtotal", "123.45");
        assert_eq!(line.chars().count(), LINE_WIDTH);
        assert!(line.ends_with("123.45"));
        assert!(line.starts_with("Subtotal"));
    }

    #[test]
    fn format_qty_shows_whole_numbers_bare_and_fractions_to_two_places() {
        assert_eq!(format_qty(2.0), "2");
        assert_eq!(format_qty(0.4), "0.4");
        assert_eq!(format_qty(0.77), "0.77");
        assert_eq!(format_qty(1.2), "1.2");
        assert_eq!(format_qty(1.257), "1.26", "rounds to 2 places");
        assert_eq!(format_qty(10.0), "10");
    }

    #[test]
    fn truncate_line_leaves_a_short_line_untouched() {
        assert_eq!(truncate_line("Diwan Store"), "Diwan Store");
    }

    #[test]
    fn truncate_line_cuts_a_line_longer_than_the_printer_width_down_to_it() {
        let long_name = "A".repeat(LINE_WIDTH + 20);
        let truncated = truncate_line(&long_name);
        assert_eq!(truncated.chars().count(), LINE_WIDTH, "must never exceed one physical printed line");
        assert_eq!(truncated, "A".repeat(LINE_WIDTH));
    }

    #[test]
    fn row_pads_and_aligns_every_column() {
        let line = row(&[("Cola", 20, false), ("x2", 6, true), ("8000", 22, true)]);
        assert_eq!(line.chars().count(), 48);
        assert!(line.starts_with("Cola"));
        assert!(line.ends_with("8000"));
    }

    #[test]
    fn bordered_row_wraps_every_column_in_pipes() {
        let line = bordered_row(&[("Cola", 10, false), ("x2", 4, true), ("8000", 8, true)]);
        assert_eq!(line, "|Cola      |  x2|    8000|");
        assert!(line.starts_with('|'));
        assert!(line.ends_with('|'));
    }

    #[test]
    fn bordered_line_matches_bordered_row_width() {
        let widths = [10usize, 4, 8];
        let border = bordered_line(&widths);
        let row = bordered_row(&[("Cola", 10, false), ("x2", 4, true), ("8000", 8, true)]);
        assert_eq!(border.chars().count(), row.chars().count());
        assert_eq!(border, "+----------+----+--------+");
    }

    #[test]
    fn an_overlong_value_is_truncated_not_wrapped() {
        let line = pad("a very very very long label indeed", 10, false);
        assert_eq!(line.chars().count(), 10);
    }

    #[test]
    fn format_minor_matches_the_receipt_convention() {
        assert_eq!(format_minor(945), "9.45");
        assert_eq!(format_minor(5), "0.05");
    }
}
