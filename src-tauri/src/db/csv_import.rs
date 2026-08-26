//! Bulk-loading an existing stock list into Inventory (Phase 14) — the
//! alternative to typing in every item by hand when onboarding a client who
//! already has a spreadsheet of their stock.
//!
//! Deliberately **not** one all-or-nothing transaction the way a sale is:
//! each row is an independent item creation, not one indivisible business
//! event, so a typo in row 40 of a 200-row file should still leave the other
//! 199 imported — the caller gets a per-row error list back instead. Each
//! row's own `items::add_item` call is still atomic on its own (a single
//! `INSERT`), so nothing is ever half-written for any individual item.

use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

use super::items::{self, ItemInput};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRowError {
    /// 1-based, counting the header as row 1 — matches what a spreadsheet
    /// program shows, so "row 5" means the same thing to the shop owner
    /// looking at their file as it does in this message.
    pub row: i64,
    pub message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub imported: i64,
    pub errors: Vec<ImportRowError>,
}

#[derive(Debug)]
pub enum ImportError {
    MissingColumn(&'static str),
    Csv(String),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::MissingColumn(name) => {
                write!(f, "The CSV must have a \"{}\" column", name)
            }
            ImportError::Csv(msg) => write!(f, "Could not read the CSV: {}", msg),
        }
    }
}

/// A ready-to-download example file, so a shop owner knows exactly what
/// headers and format are expected before they build their own. Column
/// order doesn't matter to the parser (headers are matched by name), but
/// this is the order shown so the example reads naturally.
pub const TEMPLATE_CSV: &str = "\
name,barcode,category,price,cost,stock,low_stock_threshold
Cola 500ml,8901234500011,Beverages,80,58,48,12
Salted Chips 60g,,Snacks,50,34,96,24
";

fn parse_money(raw: &str) -> Result<i64, String> {
    let amount: f64 = raw
        .trim()
        .parse()
        .map_err(|_| format!("\"{}\" isn't a valid amount", raw))?;
    if amount < 0.0 {
        return Err("Amount cannot be negative".into());
    }
    Ok((amount * 100.0).round() as i64)
}

fn parse_int(raw: &str) -> Result<i64, String> {
    let value: i64 = raw.trim().parse().map_err(|_| format!("\"{}\" isn't a whole number", raw))?;
    if value < 0 {
        return Err("Cannot be negative".into());
    }
    Ok(value)
}

/// Finds a category by name (case-insensitively — "Beverages" and
/// "beverages" in the same file shouldn't create two categories), creating
/// it if this is the first row to mention it. Cached per import so a
/// category used across many rows is only looked up/created once.
fn resolve_category(
    conn: &Connection,
    cache: &mut HashMap<String, i64>,
    name: &str,
) -> Result<i64, String> {
    let key = name.to_lowercase();
    if let Some(id) = cache.get(&key) {
        return Ok(*id);
    }

    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM categories WHERE name = ?1 COLLATE NOCASE",
            [name],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;

    let id = match existing {
        Some(id) => id,
        None => items::add_category(conn, name).map_err(|e| e.to_string())?.id,
    };
    cache.insert(key, id);
    Ok(id)
}

/// Parses `csv_content` and imports every row it can, returning a count of
/// what succeeded plus a list of what didn't and why. Column names are
/// matched case-insensitively and independent of order; only `name` and
/// `price` are required — everything else defaults sensibly (no barcode, no
/// category, zero stock, zero low-stock threshold).
pub fn import_csv(conn: &Connection, csv_content: &str) -> Result<ImportSummary, ImportError> {
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .flexible(true)
        .from_reader(csv_content.as_bytes());

    let headers = reader.headers().map_err(|e| ImportError::Csv(e.to_string()))?.clone();
    let col = |name: &str| headers.iter().position(|h| h.eq_ignore_ascii_case(name));

    let name_col = col("name").ok_or(ImportError::MissingColumn("name"))?;
    let price_col = col("price").ok_or(ImportError::MissingColumn("price"))?;
    let barcode_col = col("barcode");
    let cost_col = col("cost");
    let stock_col = col("stock").or_else(|| col("stock_qty"));
    let category_col = col("category");
    let threshold_col = col("low_stock_threshold").or_else(|| col("threshold"));

    let mut category_cache = HashMap::new();
    let mut imported = 0i64;
    let mut errors = Vec::new();

    for (index, record) in reader.records().enumerate() {
        let row = (index + 2) as i64; // +1 for zero-index, +1 for the header row

        let record = match record {
            Ok(r) => r,
            Err(e) => {
                errors.push(ImportRowError { row, message: e.to_string() });
                continue;
            }
        };

        let field = |i: usize| record.get(i).map(str::trim).filter(|s| !s.is_empty());

        let result = (|| -> Result<(), String> {
            let name = field(name_col).ok_or("Missing name")?.to_string();
            let price_minor = parse_money(field(price_col).ok_or("Missing price")?)?;
            let cost_minor = cost_col.and_then(field).map(parse_money).transpose()?.unwrap_or(0);
            let stock_qty = stock_col.and_then(field).map(parse_int).transpose()?.unwrap_or(0) as f64;
            let low_stock_threshold =
                threshold_col.and_then(field).map(parse_int).transpose()?.unwrap_or(0);
            let barcode = barcode_col.and_then(field).map(str::to_string);

            let category_id = match category_col.and_then(field) {
                Some(cat_name) => Some(resolve_category(conn, &mut category_cache, cat_name)?),
                None => None,
            };

            items::add_item(
                conn,
                ItemInput {
                    name,
                    barcode,
                    // Not a CSV column (out of scope for bulk import) — an
                    // owner can add one later via the item edit form.
                    description: None,
                    price_minor,
                    cost_minor,
                    stock_qty,
                    category_id,
                    low_stock_threshold,
                    image_path: None,
                    // Not CSV columns either (same reasoning as
                    // `description` above) — off by default, set later via
                    // the item edit form if this row turns out to be a
                    // loose/weighed item.
                    sold_by_amount: false,
                    unit: None,
                    // Not a CSV column either — set later via the item edit
                    // form if this row turns out to need a KOT token.
                    counter_id: None,
                },
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })();

        match result {
            Ok(()) => imported += 1,
            Err(message) => errors.push(ImportRowError { row, message }),
        }
    }

    Ok(ImportSummary { imported, errors })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::test_conn;

    #[test]
    fn imports_valid_rows_and_reports_the_bad_ones() {
        let conn = test_conn();
        let csv = "\
name,barcode,category,price,cost,stock,low_stock_threshold
Bulk Rice 25kg,9990000000001,Grocery,4500,3900,10,2
,9990000000002,Grocery,100,80,5,1
Bulk Sugar 10kg,,Grocery,1200,1000,-5,1
Bulk Flour 10kg,,Grocery,not-a-price,900,20,2
";
        let summary = import_csv(&conn, csv).unwrap();
        assert_eq!(summary.imported, 1, "only the fully valid row should import");
        assert_eq!(summary.errors.len(), 3);
        assert_eq!(summary.errors[0].row, 3, "row numbers count the header as row 1");
        assert!(summary.errors[0].message.to_lowercase().contains("name"));
        assert!(summary.errors[1].message.to_lowercase().contains("negative"));
        assert!(summary.errors[2].message.contains("not-a-price"));

        let stored: i64 = conn
            .query_row("SELECT price_minor FROM items WHERE name = 'Bulk Rice 25kg'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(stored, 450_000, "\"4500\" must be parsed as a decimal amount, minor units");
    }

    #[test]
    fn reuses_an_existing_category_case_insensitively_instead_of_duplicating_it() {
        let conn = test_conn();
        let before: i64 = conn.query_row("SELECT COUNT(*) FROM categories", [], |r| r.get(0)).unwrap();

        let csv = "\
name,category,price
Item One,BEVERAGES,10
Item Two,beverages,20
";
        let summary = import_csv(&conn, csv).unwrap();
        assert_eq!(summary.imported, 2);

        let after: i64 = conn.query_row("SELECT COUNT(*) FROM categories", [], |r| r.get(0)).unwrap();
        assert_eq!(after, before, "\"BEVERAGES\" must reuse the seeded \"Beverages\" category, not add one");
    }

    #[test]
    fn creates_a_new_category_the_first_time_it_appears() {
        let conn = test_conn();
        let csv = "name,category,price\nGadget,Electronics,999\n";
        import_csv(&conn, csv).unwrap();

        let exists: bool = conn
            .query_row("SELECT EXISTS(SELECT 1 FROM categories WHERE name = 'Electronics')", [], |r| r.get(0))
            .unwrap();
        assert!(exists);
    }

    #[test]
    fn defaults_are_applied_when_optional_columns_are_missing_entirely() {
        let conn = test_conn();
        // Only the two required columns — no barcode/cost/stock/category/threshold at all.
        let csv = "name,price\nMinimal Item,25\n";
        let summary = import_csv(&conn, csv).unwrap();
        assert_eq!(summary.imported, 1);

        let (stock, cost, threshold): (f64, i64, i64) = conn
            .query_row(
                "SELECT stock_qty, cost_minor, low_stock_threshold FROM items WHERE name = 'Minimal Item'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!((stock, cost, threshold), (0.0, 0, 0));
    }

    #[test]
    fn rejects_a_csv_missing_a_required_column() {
        let conn = test_conn();
        let err = import_csv(&conn, "name,barcode\nThing,123\n").unwrap_err();
        assert!(matches!(err, ImportError::MissingColumn("price")));
    }

    #[test]
    fn a_duplicate_barcode_is_a_row_error_not_a_hard_failure_of_the_whole_import() {
        let conn = test_conn();
        let csv = "\
name,barcode,price
Duplicate Cola,8901234500011,80
Unique Item,,90
";
        // 8901234500011 is Cola 500ml's seeded barcode.
        let summary = import_csv(&conn, csv).unwrap();
        assert_eq!(summary.imported, 1, "the row after the failing one must still import");
        assert_eq!(summary.errors.len(), 1);
        assert_eq!(summary.errors[0].row, 2);
    }

    #[test]
    fn column_order_and_case_do_not_matter() {
        let conn = test_conn();
        // Reordered and mixed-case headers, price before name.
        let csv = "PRICE,Name,Stock\n15.50,Reordered Item,7\n";
        let summary = import_csv(&conn, csv).unwrap();
        assert_eq!(summary.imported, 1);

        let (price, stock): (i64, f64) = conn
            .query_row(
                "SELECT price_minor, stock_qty FROM items WHERE name = 'Reordered Item'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((price, stock), (1550, 7.0));
    }
}
