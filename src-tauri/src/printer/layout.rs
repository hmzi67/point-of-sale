//! Shared ESC/POS text-layout primitives — column alignment, dividers, money
//! formatting — used by every template in `printer::escpos` (customer
//! receipt, refund receipt, shift close-out, category-wise sale) so each one
//! only has to describe *what* goes in each row, not re-derive how to pad a
//! column to line up on 80mm paper.
//!
//! `LINE_WIDTH` (48) is the standard character count for Font A on 80mm
//! thermal paper (Epson's own TM-series default) — `receiptPdf.ts`'s
//! `PAGE_WIDTH_MM = 80` on the PDF side targets the same physical paper
//! width, just via millimeters instead of character columns, since a PDF
//! lays out proportionally rather than in a fixed character grid.

/// Character columns per line at normal size on 80mm paper, Font A.
pub const LINE_WIDTH: usize = 48;

/// Right-hand column width used by [`two_col`] — wide enough for
/// `"1,234,567.89"`-scale amounts with a couple of characters of breathing
/// room, without eating too much of the line back from the label.
const VALUE_WIDTH: usize = 16;

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

/// Minor units -> a plain "12.34" string, without a currency symbol (every
/// template prints the symbol once per section, not per line).
pub fn format_minor(minor: i64) -> String {
    format!("{}.{:02}", minor / 100, (minor % 100).abs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_col_lines_up_the_value_at_the_right_edge() {
        let line = two_col("Subtotal", "123.45");
        assert_eq!(line.chars().count(), LINE_WIDTH);
        assert!(line.ends_with("123.45"));
        assert!(line.starts_with("Subtotal"));
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
