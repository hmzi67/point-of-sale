//! Billing: creating a sale atomically (line items + stock decrement) and
//! reading one back for receipt reprint.
//!
//! Restaurant table lifecycle (seating, parking a cart, clearing) lives in
//! `db::tables` — this module only calls into it once, at the very end of
//! `create_sale`, to close out the table's order when a dine-in sale
//! completes.

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

use super::tables;

// ---------------------------------------------------------------------------
// Sales
// ---------------------------------------------------------------------------

/// One cart line as submitted by the client. Only `item_id` and `qty` are
/// trusted — price is always re-read from `items` inside the transaction, so
/// a tampered client payload can never write an arbitrary price to history.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CartLine {
    pub item_id: i64,
    /// `f64`, not `i64` — a `sold_by_amount` item's qty is usually
    /// fractional (computed client-side as amount ÷ price). See
    /// `schema.rs`'s module doc comment for why `sale_items.qty`'s
    /// declared-type change needed no migration.
    pub qty: f64,
    /// A cashier's free-text note on this line (e.g. "no onions") — purely
    /// informational, never affects pricing or stock. `#[serde(default)]` so
    /// existing callers that don't send it still deserialize fine.
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSaleInput {
    pub items: Vec<CartLine>,
    pub discount_minor: i64,
    pub tax_minor: i64,
    pub payment_method: String,
    pub cashier_id: Option<i64>,
    /// Dine-in table this sale belongs to, if any. Only meaningful when the
    /// `tables` module is enabled — the frontend never sends it otherwise.
    pub table_id: Option<i64>,
    /// The cashier's currently-open shift, if any. Only meaningful when the
    /// `shifts` module is enabled; `None` either because that module is off
    /// or because this cashier hasn't opened one. Attributing the sale here
    /// (rather than back-filling it after the fact) is what lets
    /// `shifts::get_shift_summary` scope "sales during this shift" with a
    /// plain `WHERE shift_id = ?` instead of a time-window join.
    pub shift_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaleLine {
    pub item_id: i64,
    pub item_name: String,
    pub qty: f64,
    /// Denormalized from `items.unit` at read time (like `item_name`) —
    /// display-only, for showing e.g. "0.77 kg" on a `sold_by_amount`
    /// line's receipt row. `None` for a normal per-piece item, or if the
    /// item was deleted/its unit cleared since this sale.
    pub unit: Option<String>,
    pub price_at_sale_minor: i64,
    pub line_total_minor: i64,
    pub notes: Option<String>,
}

/// A completed sale plus everything a receipt needs — one round trip covers
/// both "just completed" and "reprint an old one".
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sale {
    pub id: i64,
    pub subtotal_minor: i64,
    pub discount_minor: i64,
    pub tax_minor: i64,
    pub total_minor: i64,
    pub payment_method: String,
    pub cashier_id: Option<i64>,
    pub cashier_name: Option<String>,
    pub table_id: Option<i64>,
    pub table_name: Option<String>,
    pub shift_id: Option<i64>,
    pub created_at: String,
    pub items: Vec<SaleLine>,
}

const PAYMENT_METHODS: &[&str] = &["cash", "card", "other"];

#[derive(Debug)]
pub enum SaleError {
    EmptyCart,
    InvalidQuantity,
    ItemNotFound(i64),
    ItemInactive(String),
    InsufficientStock { name: String, available: f64, requested: f64 },
    InvalidDiscount,
    InvalidPaymentMethod(String),
    NotFound,
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for SaleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaleError::EmptyCart => write!(f, "Cart is empty"),
            SaleError::InvalidQuantity => write!(f, "Quantity must be at least 1"),
            SaleError::ItemNotFound(id) => write!(f, "Item {} no longer exists", id),
            SaleError::ItemInactive(name) => {
                write!(f, "{} is no longer sold and cannot be added to a sale", name)
            }
            SaleError::InsufficientStock { name, available, requested } => write!(
                f,
                "Only {} left of {}, but {} requested",
                available, name, requested
            ),
            SaleError::InvalidDiscount => write!(f, "Discount cannot exceed the subtotal"),
            SaleError::InvalidPaymentMethod(method) => {
                write!(f, "Unknown payment method: {}", method)
            }
            SaleError::NotFound => write!(f, "Sale not found"),
            SaleError::Sqlite(err) => write!(f, "database error: {}", err),
        }
    }
}

impl From<rusqlite::Error> for SaleError {
    fn from(err: rusqlite::Error) -> Self {
        SaleError::Sqlite(err)
    }
}

/// Builds and writes a sale inside `tx`: re-prices every line against the
/// live `items` row, checks stock, inserts `sales` + `sale_items`, decrements
/// stock, and — if a table was given — closes that table's parked order (if
/// any) and frees the table. Every step happens on the same transaction, so
/// the caller committing or rolling back `tx` is what actually makes this
/// atomic; this function itself never partially applies its writes because
/// any `?` bails out before `tx.commit()` is ever reached by the caller.
pub fn create_sale(tx: &Transaction, input: CreateSaleInput) -> Result<Sale, SaleError> {
    if input.items.is_empty() {
        return Err(SaleError::EmptyCart);
    }
    if !PAYMENT_METHODS.contains(&input.payment_method.as_str()) {
        return Err(SaleError::InvalidPaymentMethod(input.payment_method));
    }
    if input.discount_minor < 0 || input.tax_minor < 0 {
        return Err(SaleError::InvalidDiscount);
    }

    // Defensive merge in case the client ever sends the same item twice —
    // keeps exactly one sale_items row per item per sale.
    let mut merged: Vec<CartLine> = Vec::with_capacity(input.items.len());
    for line in &input.items {
        if line.qty <= 0.0 {
            return Err(SaleError::InvalidQuantity);
        }
        match merged.iter_mut().find(|l: &&mut CartLine| l.item_id == line.item_id) {
            Some(existing) => {
                existing.qty += line.qty;
                // Duplicate lines for the same item are a defensive-merge
                // edge case (the cart UI never sends two), so there's no
                // real "which note wins" question in practice — first
                // non-empty one found is kept.
                if existing.notes.is_none() {
                    existing.notes = line.notes.clone();
                }
            }
            None => merged.push(line.clone()),
        }
    }

    let mut subtotal_minor: i64 = 0;
    // (item_id, name, qty, price_minor, notes)
    let mut resolved: Vec<(i64, String, f64, i64, Option<String>)> = Vec::with_capacity(merged.len());

    for line in &merged {
        let row: Option<(String, i64, f64, i64)> = tx
            .query_row(
                "SELECT name, price_minor, stock_qty, is_active FROM items WHERE id = ?1",
                params![line.item_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get::<_, i64>(3)?)),
            )
            .optional()?;
        let (name, price_minor, stock_qty, is_active) =
            row.ok_or(SaleError::ItemNotFound(line.item_id))?;

        if is_active == 0 {
            return Err(SaleError::ItemInactive(name));
        }
        if stock_qty < line.qty {
            return Err(SaleError::InsufficientStock {
                name,
                available: stock_qty,
                requested: line.qty,
            });
        }

        // price_minor (integer) × qty (possibly fractional) rounded back to
        // whole minor units — the same "round straight back to whole minor
        // units" convention CLAUDE.md documents for tax-percent-derived
        // amounts, applied here because qty can now be fractional too.
        subtotal_minor += (price_minor as f64 * line.qty).round() as i64;
        resolved.push((line.item_id, name, line.qty, price_minor, line.notes.clone()));
    }

    if input.discount_minor > subtotal_minor {
        return Err(SaleError::InvalidDiscount);
    }

    let total_minor = subtotal_minor - input.discount_minor + input.tax_minor;

    tx.execute(
        "INSERT INTO sales (total_minor, discount_minor, tax_minor, payment_method, cashier_id, table_id, shift_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            total_minor,
            input.discount_minor,
            input.tax_minor,
            input.payment_method,
            input.cashier_id,
            input.table_id,
            input.shift_id
        ],
    )?;
    let sale_id = tx.last_insert_rowid();

    for (item_id, name, qty, price_minor, notes) in &resolved {
        // Blank notes are stored as NULL, not "", same normalization every
        // other optional text field in the schema uses.
        let notes = notes.as_deref().map(str::trim).filter(|s| !s.is_empty());
        tx.execute(
            "INSERT INTO sale_items (sale_id, item_id, qty, price_at_sale_minor, notes)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![sale_id, item_id, qty, price_minor, notes],
        )?;

        // The `stock_qty >= ?1` guard makes this a compare-and-swap: if stock
        // moved under us since the read above (impossible today with the
        // single mutex-guarded connection, but this is what stays correct if
        // that ever changes), the update matches zero rows and we bail out —
        // the whole transaction rolls back, so no partial sale is ever saved.
        let changed = tx.execute(
            "UPDATE items SET stock_qty = stock_qty - ?1 WHERE id = ?2 AND stock_qty >= ?1",
            params![qty, item_id],
        )?;
        if changed == 0 {
            return Err(SaleError::InsufficientStock {
                name: name.clone(),
                available: 0.0,
                requested: *qty,
            });
        }
    }

    if let Some(table_id) = input.table_id {
        tables::close_table_order_for_sale(tx, table_id, sale_id)?;
    }

    load_sale(tx, sale_id)?.ok_or(SaleError::NotFound)
}

fn load_sale(conn: &Connection, id: i64) -> Result<Option<Sale>, rusqlite::Error> {
    let sale = conn
        .query_row(
            "SELECT s.id, s.discount_minor, s.tax_minor, s.total_minor, s.payment_method,
                    s.cashier_id, u.name AS cashier_name, s.table_id, t.name AS table_name,
                    s.shift_id, s.created_at
               FROM sales s
               LEFT JOIN users u ON u.id = s.cashier_id
               LEFT JOIN tables t ON t.id = s.table_id
              WHERE s.id = ?1",
            params![id],
            |row| {
                let discount_minor: i64 = row.get(1)?;
                let tax_minor: i64 = row.get(2)?;
                let total_minor: i64 = row.get(3)?;
                Ok(Sale {
                    id: row.get(0)?,
                    // subtotal = total - tax + discount, recovered from the
                    // three stored amounts rather than a fourth column.
                    subtotal_minor: total_minor - tax_minor + discount_minor,
                    discount_minor,
                    tax_minor,
                    total_minor,
                    payment_method: row.get(4)?,
                    cashier_id: row.get(5)?,
                    cashier_name: row.get(6)?,
                    table_id: row.get(7)?,
                    table_name: row.get(8)?,
                    shift_id: row.get(9)?,
                    created_at: row.get(10)?,
                    items: Vec::new(),
                })
            },
        )
        .optional()?;

    let Some(mut sale) = sale else { return Ok(None) };

    let mut stmt = conn.prepare(
        "SELECT si.item_id, i.name, si.qty, si.price_at_sale_minor, si.notes, i.unit
           FROM sale_items si
           JOIN items i ON i.id = si.item_id
          WHERE si.sale_id = ?1
          ORDER BY si.id",
    )?;
    sale.items = stmt
        .query_map(params![id], |row| {
            let qty: f64 = row.get(2)?;
            let price_at_sale_minor: i64 = row.get(3)?;
            Ok(SaleLine {
                item_id: row.get(0)?,
                item_name: row.get(1)?,
                qty,
                unit: row.get(5)?,
                price_at_sale_minor,
                line_total_minor: (price_at_sale_minor as f64 * qty).round() as i64,
                notes: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Some(sale))
}

/// Fetches a completed sale for receipt reprint.
pub fn get_sale(conn: &Connection, id: i64) -> Result<Sale, SaleError> {
    load_sale(conn, id)?.ok_or(SaleError::NotFound)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaleListItem {
    pub id: i64,
    pub total_minor: i64,
    pub payment_method: String,
    pub cashier_name: Option<String>,
    pub created_at: String,
}

/// The most recent sales, newest first — the refund flow's "pick the
/// original sale" list, for a cashier searching by memory/receipt number
/// rather than typing an exact id.
pub fn list_recent(conn: &Connection, limit: i64) -> Result<Vec<SaleListItem>, rusqlite::Error> {
    let limit = limit.clamp(1, 200);
    let mut stmt = conn.prepare(
        "SELECT s.id, s.total_minor, s.payment_method, u.name, s.created_at
           FROM sales s
           LEFT JOIN users u ON u.id = s.cashier_id
          ORDER BY s.id DESC
          LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |row| {
        Ok(SaleListItem {
            id: row.get(0)?,
            total_minor: row.get(1)?,
            payment_method: row.get(2)?,
            cashier_name: row.get(3)?,
            created_at: row.get(4)?,
        })
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::items::{self, ItemInput};
    use crate::db::schema::test_conn;

    fn item_id(conn: &Connection, name: &str) -> i64 {
        conn.query_row("SELECT id FROM items WHERE name = ?1", params![name], |row| row.get(0))
            .unwrap()
    }

    fn stock(conn: &Connection, name: &str) -> f64 {
        conn.query_row(
            "SELECT stock_qty FROM items WHERE name = ?1",
            params![name],
            |row| row.get(0),
        )
        .unwrap()
    }

    fn basic_input(conn: &Connection) -> CreateSaleInput {
        CreateSaleInput {
            items: vec![CartLine { item_id: item_id(conn, "Cola 500ml"), qty: 2.0, notes: None }],
            discount_minor: 0,
            tax_minor: 0,
            payment_method: "cash".into(),
            cashier_id: None,
            table_id: None,
            shift_id: None,
        }
    }

    #[test]
    fn a_cart_lines_note_is_persisted_and_read_back_on_the_sale() {
        let mut conn = test_conn();
        let tx = conn.transaction().unwrap();
        let input = CreateSaleInput {
            items: vec![CartLine {
                item_id: item_id(&tx, "Cola 500ml"),
                qty: 1.0,
                notes: Some("  extra cold, no straw  ".into()),
            }],
            ..basic_input(&tx)
        };
        let sale = create_sale(&tx, input).unwrap();
        tx.commit().unwrap();

        assert_eq!(sale.items[0].notes.as_deref(), Some("extra cold, no straw"), "must be trimmed");

        let reloaded = get_sale(&conn, sale.id).unwrap();
        assert_eq!(reloaded.items[0].notes.as_deref(), Some("extra cold, no straw"));
    }

    #[test]
    fn a_blank_note_is_stored_as_none_not_an_empty_string() {
        let mut conn = test_conn();
        let tx = conn.transaction().unwrap();
        let input = CreateSaleInput {
            items: vec![CartLine { item_id: item_id(&tx, "Cola 500ml"), qty: 1.0, notes: Some("   ".into()) }],
            ..basic_input(&tx)
        };
        let sale = create_sale(&tx, input).unwrap();
        assert_eq!(sale.items[0].notes, None);
    }

    /// A "sold by amount" line end to end: fractional qty, money rounded
    /// back to whole minor units, stock decremented by exactly that
    /// fraction — not rounded to a whole unit anywhere in the path.
    #[test]
    fn a_fractional_quantity_line_rounds_money_and_decrements_stock_precisely() {
        let mut conn = test_conn();
        let cola = item_id(&conn, "Cola 500ml"); // price_minor 8000
        let cola_stock_before = stock(&conn, "Cola 500ml");

        let tx = conn.transaction().unwrap();
        // 100 (10000 minor) worth of an item priced at 8000/unit -> 1.25 units.
        let input = CreateSaleInput {
            items: vec![CartLine { item_id: cola, qty: 1.25, notes: None }],
            ..basic_input(&tx)
        };
        let sale = create_sale(&tx, input).unwrap();
        tx.commit().unwrap();

        assert_eq!(sale.items[0].qty, 1.25);
        assert_eq!(sale.items[0].line_total_minor, 10_000, "8000 * 1.25 = 10000 exactly");
        assert_eq!(sale.subtotal_minor, 10_000);
        assert_eq!(stock(&conn, "Cola 500ml"), cola_stock_before - 1.25, "stock must decrement by the exact fraction");

        // A case that does NOT divide evenly, to prove the rounding is a
        // real `.round()`, not truncation: 8000 * 0.333 = 2664.0 exactly
        // already, so pick a rate that lands mid-paisa instead.
        let noodles = item_id(&conn, "Instant Noodles");
        let noodles_price_minor: i64 =
            conn.query_row("SELECT price_minor FROM items WHERE id = ?1", params![noodles], |row| row.get(0)).unwrap();
        let tx = conn.transaction().unwrap();
        let input = CreateSaleInput {
            items: vec![CartLine { item_id: noodles, qty: 0.333, notes: None }],
            ..basic_input(&tx)
        };
        let sale = create_sale(&tx, input).unwrap();
        let expected = (noodles_price_minor as f64 * 0.333).round() as i64;
        assert_eq!(sale.items[0].line_total_minor, expected);
    }

    /// The full "sold by amount" acceptance scenario: mark an item eligible,
    /// give it a per-unit price, sell it by a computed fractional qty, and
    /// confirm the receipt data (qty, unit, line total) and stock
    /// decrement are all correct. `db::items::` owns eligibility/unit;
    /// this module never re-derives amount ÷ price itself — the frontend
    /// computes qty client-side and sends it like any other cart line,
    /// same trust boundary CLAUDE.md documents for every other qty.
    #[test]
    fn a_sold_by_amount_item_sells_at_a_computed_fractional_qty_with_its_unit_on_the_receipt() {
        let mut conn = test_conn();
        let item = items::add_item(
            &conn,
            ItemInput {
                name: "Loose Channa".into(),
                barcode: None,
                description: None,
                price_minor: 12_987, // PKR 129.87/kg
                cost_minor: 10_000,
                stock_qty: 50.0,
                category_id: None,
                low_stock_threshold: 2,
                image_path: None,
                sold_by_amount: true,
                unit: Some("kg".into()),
            },
        )
        .unwrap();
        assert!(item.sold_by_amount);

        // Customer asks for "100 rupees worth": 10000 / 12987 ≈ 0.7700...
        let amount_minor = 10_000;
        let qty = ((amount_minor as f64 / item.price_minor as f64) * 100.0).round() / 100.0;
        assert_eq!(qty, 0.77);

        let tx = conn.transaction().unwrap();
        let sale = create_sale(
            &tx,
            CreateSaleInput {
                items: vec![CartLine { item_id: item.id, qty, notes: None }],
                discount_minor: 0,
                tax_minor: 0,
                payment_method: "cash".into(),
                cashier_id: None,
                table_id: None,
                shift_id: None,
            },
        )
        .unwrap();
        tx.commit().unwrap();

        assert_eq!(sale.items[0].qty, 0.77);
        assert_eq!(sale.items[0].unit.as_deref(), Some("kg"));
        assert_eq!(
            sale.items[0].line_total_minor,
            (item.price_minor as f64 * 0.77).round() as i64,
            "server re-derives price * qty, same as every other line"
        );

        let stock_after: f64 =
            conn.query_row("SELECT stock_qty FROM items WHERE id = ?1", params![item.id], |row| row.get(0)).unwrap();
        assert_eq!(stock_after, 50.0 - 0.77, "stock must decrement by the exact fractional qty");

        // And it reloads correctly for a receipt reprint too, unit included.
        let reloaded = get_sale(&conn, sale.id).unwrap();
        assert_eq!(reloaded.items[0].unit.as_deref(), Some("kg"));
    }

    #[test]
    fn completes_a_sale_and_decrements_stock() {
        let mut conn = test_conn();
        let cola_before = stock(&conn, "Cola 500ml");

        let tx = conn.transaction().unwrap();
        let sale = create_sale(&tx, basic_input(&tx)).unwrap();
        tx.commit().unwrap();

        assert_eq!(sale.items.len(), 1);
        assert_eq!(sale.items[0].qty, 2.0);
        assert_eq!(sale.subtotal_minor, 2 * 8000);
        assert_eq!(sale.total_minor, 2 * 8000);
        assert_eq!(stock(&conn, "Cola 500ml"), cola_before - 2.0);

        // And it can be read back for reprint.
        let reloaded = get_sale(&conn, sale.id).unwrap();
        assert_eq!(reloaded.id, sale.id);
        assert_eq!(reloaded.items[0].item_name, "Cola 500ml");
    }

    #[test]
    fn totals_account_for_discount_and_tax() {
        let mut conn = test_conn();
        let tx = conn.transaction().unwrap();
        let mut input = basic_input(&tx);
        input.discount_minor = 1000;
        input.tax_minor = 500;

        let sale = create_sale(&tx, input).unwrap();
        tx.commit().unwrap();

        assert_eq!(sale.subtotal_minor, 16000);
        assert_eq!(sale.discount_minor, 1000);
        assert_eq!(sale.tax_minor, 500);
        assert_eq!(sale.total_minor, 16000 - 1000 + 500);
    }

    #[test]
    fn rejects_an_empty_cart() {
        let mut conn = test_conn();
        let tx = conn.transaction().unwrap();
        let mut input = basic_input(&tx);
        input.items = vec![];
        assert!(matches!(create_sale(&tx, input), Err(SaleError::EmptyCart)));
    }

    #[test]
    fn rejects_a_non_positive_quantity() {
        let mut conn = test_conn();
        let tx = conn.transaction().unwrap();
        let cola = item_id(&tx, "Cola 500ml");
        let input = CreateSaleInput {
            items: vec![CartLine { item_id: cola, qty: 0.0, notes: None }],
            ..basic_input(&tx)
        };
        assert!(matches!(create_sale(&tx, input), Err(SaleError::InvalidQuantity)));
    }

    #[test]
    fn rejects_discount_larger_than_subtotal() {
        let mut conn = test_conn();
        let tx = conn.transaction().unwrap();
        let mut input = basic_input(&tx);
        input.discount_minor = 999_999;
        assert!(matches!(create_sale(&tx, input), Err(SaleError::InvalidDiscount)));
    }

    #[test]
    fn rejects_an_unknown_payment_method() {
        let mut conn = test_conn();
        let tx = conn.transaction().unwrap();
        let mut input = basic_input(&tx);
        input.payment_method = "crypto".into();
        assert!(matches!(
            create_sale(&tx, input),
            Err(SaleError::InvalidPaymentMethod(_))
        ));
    }

    #[test]
    fn insufficient_stock_rolls_back_the_whole_sale() {
        let mut conn = test_conn();
        let sales_before: i64 =
            conn.query_row("SELECT COUNT(*) FROM sales", [], |row| row.get(0)).unwrap();
        let cola_before = stock(&conn, "Cola 500ml");
        let noodles_before = stock(&conn, "Instant Noodles"); // seeded at 2

        {
            let tx = conn.transaction().unwrap();
            // First line is satisfiable and would decrement stock if it
            // committed; the second is not (only 2 in stock) and must abort
            // the whole transaction, not just its own line.
            let input = CreateSaleInput {
                items: vec![
                    CartLine { item_id: item_id(&tx, "Cola 500ml"), qty: 1.0, notes: None },
                    CartLine { item_id: item_id(&tx, "Instant Noodles"), qty: 999.0, notes: None },
                ],
                discount_minor: 0,
                tax_minor: 0,
                payment_method: "cash".into(),
                cashier_id: None,
                table_id: None,
                shift_id: None,
            };
            let err = create_sale(&tx, input).unwrap_err();
            assert!(matches!(err, SaleError::InsufficientStock { .. }));
            // tx drops here without commit — implicit rollback.
        }

        let sales_after: i64 =
            conn.query_row("SELECT COUNT(*) FROM sales", [], |row| row.get(0)).unwrap();
        assert_eq!(sales_after, sales_before, "no sale row should have been left behind");
        assert_eq!(stock(&conn, "Cola 500ml"), cola_before, "the satisfiable line must not have applied either");
        assert_eq!(stock(&conn, "Instant Noodles"), noodles_before);
    }

    /// The exact boundary of `insufficient_stock_rolls_back_the_whole_sale`
    /// above: selling precisely what's left must succeed and land stock at
    /// exactly zero, never negative.
    #[test]
    fn selling_exactly_the_remaining_stock_succeeds_and_leaves_zero() {
        let mut conn = test_conn();
        let noodles_before = stock(&conn, "Instant Noodles");

        let tx = conn.transaction().unwrap();
        let input = CreateSaleInput {
            items: vec![CartLine { item_id: item_id(&tx, "Instant Noodles"), qty: noodles_before, notes: None }],
            ..basic_input(&tx)
        };
        create_sale(&tx, input).unwrap();
        tx.commit().unwrap();

        assert_eq!(stock(&conn, "Instant Noodles"), 0.0);
    }

    /// One unit past the boundary above: must be blocked, and — the point of
    /// the whole check — stock must not be allowed to go negative.
    #[test]
    fn selling_one_more_than_available_stock_is_blocked_not_negative() {
        let mut conn = test_conn();
        let noodles_before = stock(&conn, "Instant Noodles");

        let tx = conn.transaction().unwrap();
        let input = CreateSaleInput {
            items: vec![CartLine { item_id: item_id(&tx, "Instant Noodles"), qty: noodles_before + 1.0, notes: None }],
            ..basic_input(&tx)
        };
        let err = create_sale(&tx, input).unwrap_err();
        assert!(matches!(err, SaleError::InsufficientStock { available, requested, .. }
            if available == noodles_before && requested == noodles_before + 1.0));
        drop(tx); // rolls back

        assert_eq!(stock(&conn, "Instant Noodles"), noodles_before, "stock must be untouched, not negative");
    }

    /// Selling against an item already sitting at zero stock is just the
    /// `requested > 0, available = 0` case of the same guard — worth its own
    /// test since "already empty" is the state low-stock alerts exist for.
    #[test]
    fn selling_an_out_of_stock_item_is_blocked() {
        let mut conn = test_conn();
        let noodles = item_id(&conn, "Instant Noodles");
        conn.execute("UPDATE items SET stock_qty = 0 WHERE id = ?1", params![noodles]).unwrap();

        let tx = conn.transaction().unwrap();
        let input = CreateSaleInput { items: vec![CartLine { item_id: noodles, qty: 1.0, notes: None }], ..basic_input(&tx) };
        let err = create_sale(&tx, input).unwrap_err();
        assert!(matches!(err, SaleError::InsufficientStock { available, requested, .. } if available == 0.0 && requested == 1.0));
    }

    #[test]
    fn rejects_negative_discount_and_negative_tax() {
        let mut conn = test_conn();
        {
            let tx = conn.transaction().unwrap();
            let mut input = basic_input(&tx);
            input.discount_minor = -1;
            assert!(matches!(create_sale(&tx, input), Err(SaleError::InvalidDiscount)));
        }

        let tx = conn.transaction().unwrap();
        let mut input = basic_input(&tx);
        input.tax_minor = -1;
        assert!(matches!(create_sale(&tx, input), Err(SaleError::InvalidDiscount)));
    }

    /// The other boundary from `rejects_discount_larger_than_subtotal`: a
    /// discount exactly equal to the subtotal is a legitimate 100%-off sale,
    /// not an error — only tax is left in the total.
    #[test]
    fn discount_equal_to_the_subtotal_is_allowed() {
        let mut conn = test_conn();
        let tx = conn.transaction().unwrap();
        let mut input = basic_input(&tx); // Cola x2 @ 8000 = 16000 subtotal
        input.discount_minor = 16000;
        input.tax_minor = 500;

        let sale = create_sale(&tx, input).unwrap();
        assert_eq!(sale.total_minor, 500);
    }

    #[test]
    fn rejects_an_archived_item() {
        let mut conn = test_conn();
        let cola = item_id(&conn, "Cola 500ml");
        conn.execute("UPDATE items SET is_active = 0 WHERE id = ?1", params![cola]).unwrap();

        let tx = conn.transaction().unwrap();
        let input = CreateSaleInput {
            items: vec![CartLine { item_id: cola, qty: 1.0, notes: None }],
            discount_minor: 0,
            tax_minor: 0,
            payment_method: "cash".into(),
            cashier_id: None,
            table_id: None,
            shift_id: None,
        };
        assert!(matches!(create_sale(&tx, input), Err(SaleError::ItemInactive(_))));
    }

    #[test]
    fn duplicate_cart_lines_for_the_same_item_are_merged() {
        let mut conn = test_conn();
        let tx = conn.transaction().unwrap();
        let cola = item_id(&tx, "Cola 500ml");
        let input = CreateSaleInput {
            items: vec![
                CartLine { item_id: cola, qty: 2.0, notes: None },
                CartLine { item_id: cola, qty: 3.0, notes: None },
            ],
            discount_minor: 0,
            tax_minor: 0,
            payment_method: "cash".into(),
            cashier_id: None,
            table_id: None,
            shift_id: None,
        };
        let sale = create_sale(&tx, input).unwrap();
        tx.commit().unwrap();

        assert_eq!(sale.items.len(), 1, "one merged line, not two");
        assert_eq!(sale.items[0].qty, 5.0);
        assert_eq!(stock(&conn, "Cola 500ml"), 42.0 - 5.0);
    }

    #[test]
    fn get_sale_reports_not_found_for_an_unknown_id() {
        let conn = test_conn();
        assert!(matches!(get_sale(&conn, 999_999), Err(SaleError::NotFound)));
    }

    #[test]
    fn list_recent_returns_sales_newest_first_and_respects_the_limit() {
        let conn = test_conn();
        let all = list_recent(&conn, 200).unwrap();
        assert_eq!(all.len(), 12, "seed data has 12 sales");
        for pair in all.windows(2) {
            assert!(pair[0].id > pair[1].id, "must be newest first");
        }

        let limited = list_recent(&conn, 3).unwrap();
        assert_eq!(limited.len(), 3);
        assert_eq!(limited, all[..3]);
    }

    // Table-linked sales (closing the parked order, freeing the table) are
    // covered in db::tables::tests, alongside the rest of the table lifecycle.
}
