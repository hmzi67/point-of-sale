//! Billing: creating a sale atomically (line items + stock decrement) and
//! reading one back for receipt reprint.
//!
//! Restaurant "park the cart at a table" support lives here too, since it
//! shares the same cart shape. It is intentionally a lightweight stand-in —
//! the parked cart is stored as JSON on `table_orders.cart_json` rather than
//! as real relational line items — that Phase 6 (full Table Management) is
//! expected to replace with a proper draft-order schema once it needs to do
//! more than park-and-resume (e.g. per-line kitchen status).

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

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
    pub qty: i64,
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
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaleLine {
    pub item_id: i64,
    pub item_name: String,
    pub qty: i64,
    pub price_at_sale_minor: i64,
    pub line_total_minor: i64,
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
    InsufficientStock { name: String, available: i64, requested: i64 },
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
        if line.qty <= 0 {
            return Err(SaleError::InvalidQuantity);
        }
        match merged.iter_mut().find(|l: &&mut CartLine| l.item_id == line.item_id) {
            Some(existing) => existing.qty += line.qty,
            None => merged.push(line.clone()),
        }
    }

    let mut subtotal_minor: i64 = 0;
    let mut resolved: Vec<(i64, String, i64, i64)> = Vec::with_capacity(merged.len()); // (item_id, name, qty, price_minor)

    for line in &merged {
        let row: Option<(String, i64, i64, i64)> = tx
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

        subtotal_minor += price_minor * line.qty;
        resolved.push((line.item_id, name, line.qty, price_minor));
    }

    if input.discount_minor > subtotal_minor {
        return Err(SaleError::InvalidDiscount);
    }

    let total_minor = subtotal_minor - input.discount_minor + input.tax_minor;

    tx.execute(
        "INSERT INTO sales (total_minor, discount_minor, tax_minor, payment_method, cashier_id, table_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            total_minor,
            input.discount_minor,
            input.tax_minor,
            input.payment_method,
            input.cashier_id,
            input.table_id
        ],
    )?;
    let sale_id = tx.last_insert_rowid();

    for (item_id, name, qty, price_minor) in &resolved {
        tx.execute(
            "INSERT INTO sale_items (sale_id, item_id, qty, price_at_sale_minor)
             VALUES (?1, ?2, ?3, ?4)",
            params![sale_id, item_id, qty, price_minor],
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
                available: 0,
                requested: *qty,
            });
        }
    }

    if let Some(table_id) = input.table_id {
        close_table_order_for_sale(tx, table_id, sale_id)?;
    }

    load_sale(tx, sale_id)?.ok_or(SaleError::NotFound)
}

fn load_sale(conn: &Connection, id: i64) -> Result<Option<Sale>, rusqlite::Error> {
    let sale = conn
        .query_row(
            "SELECT s.id, s.discount_minor, s.tax_minor, s.total_minor, s.payment_method,
                    s.cashier_id, u.name AS cashier_name, s.table_id, t.name AS table_name, s.created_at
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
                    created_at: row.get(9)?,
                    items: Vec::new(),
                })
            },
        )
        .optional()?;

    let Some(mut sale) = sale else { return Ok(None) };

    let mut stmt = conn.prepare(
        "SELECT si.item_id, i.name, si.qty, si.price_at_sale_minor
           FROM sale_items si
           JOIN items i ON i.id = si.item_id
          WHERE si.sale_id = ?1
          ORDER BY si.id",
    )?;
    sale.items = stmt
        .query_map(params![id], |row| {
            let qty: i64 = row.get(2)?;
            let price_at_sale_minor: i64 = row.get(3)?;
            Ok(SaleLine {
                item_id: row.get(0)?,
                item_name: row.get(1)?,
                qty,
                price_at_sale_minor,
                line_total_minor: price_at_sale_minor * qty,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Some(sale))
}

/// Fetches a completed sale for receipt reprint.
pub fn get_sale(conn: &Connection, id: i64) -> Result<Sale, SaleError> {
    load_sale(conn, id)?.ok_or(SaleError::NotFound)
}

// ---------------------------------------------------------------------------
// Restaurant: park a cart at a table, and resume it later
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParkedCartLine {
    pub item_id: i64,
    pub qty: i64,
}

/// The JSON payload stored in `table_orders.cart_json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ParkedCart {
    items: Vec<ParkedCartLine>,
    discount_minor: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableSummary {
    pub id: i64,
    pub name: String,
    pub status: String,
    pub has_parked_order: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParkedOrder {
    pub items: Vec<ParkedCartLine>,
    pub discount_minor: i64,
}

#[derive(Debug)]
pub enum TableOrderError {
    TableNotFound,
    EmptyCart,
    Corrupt(String),
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for TableOrderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TableOrderError::TableNotFound => write!(f, "Table not found"),
            TableOrderError::EmptyCart => write!(f, "Cart is empty"),
            TableOrderError::Corrupt(msg) => write!(f, "Could not read the parked order: {}", msg),
            TableOrderError::Sqlite(err) => write!(f, "database error: {}", err),
        }
    }
}

impl From<rusqlite::Error> for TableOrderError {
    fn from(err: rusqlite::Error) -> Self {
        TableOrderError::Sqlite(err)
    }
}

/// Tables with their status and whether one already has a cart parked on it,
/// for the billing screen's table picker.
pub fn list_tables(conn: &Connection) -> Result<Vec<TableSummary>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name, t.status,
                EXISTS (SELECT 1 FROM table_orders o
                         WHERE o.table_id = t.id AND o.status = 'open') AS has_parked
           FROM tables t
          ORDER BY t.name",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(TableSummary {
            id: row.get(0)?,
            name: row.get(1)?,
            status: row.get(2)?,
            has_parked_order: row.get::<_, i64>(3)? != 0,
        })
    })?;
    rows.collect()
}

/// Parks (creates, or replaces the existing open one for) a table's draft
/// order and marks the table occupied. Used by "Save to table" on the billing
/// screen — the sale itself is not created until the table is billed.
pub fn attach_cart_to_table(
    conn: &Connection,
    table_id: i64,
    items: &[ParkedCartLine],
    discount_minor: i64,
) -> Result<(), TableOrderError> {
    if items.is_empty() {
        return Err(TableOrderError::EmptyCart);
    }

    let table_exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM tables WHERE id = ?1)",
        params![table_id],
        |row| row.get(0),
    )?;
    if !table_exists {
        return Err(TableOrderError::TableNotFound);
    }

    let cart_json = serde_json::to_string(&ParkedCart { items: items.to_vec(), discount_minor })
        .map_err(|e| TableOrderError::Corrupt(e.to_string()))?;

    let existing_order_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM table_orders WHERE table_id = ?1 AND status = 'open'",
            params![table_id],
            |row| row.get(0),
        )
        .optional()?;

    match existing_order_id {
        Some(order_id) => {
            conn.execute(
                "UPDATE table_orders SET cart_json = ?1 WHERE id = ?2",
                params![cart_json, order_id],
            )?;
        }
        None => {
            conn.execute(
                "INSERT INTO table_orders (table_id, status, cart_json) VALUES (?1, 'open', ?2)",
                params![table_id, cart_json],
            )?;
        }
    }

    conn.execute("UPDATE tables SET status = 'occupied' WHERE id = ?1", params![table_id])?;
    Ok(())
}

/// The cart currently parked on a table, if any — used to resume billing it.
pub fn get_parked_cart(conn: &Connection, table_id: i64) -> Result<Option<ParkedOrder>, TableOrderError> {
    let cart_json: Option<String> = conn
        .query_row(
            "SELECT cart_json FROM table_orders WHERE table_id = ?1 AND status = 'open'",
            params![table_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();

    let Some(cart_json) = cart_json else { return Ok(None) };
    let parsed: ParkedCart =
        serde_json::from_str(&cart_json).map_err(|e| TableOrderError::Corrupt(e.to_string()))?;

    Ok(Some(ParkedOrder { items: parsed.items, discount_minor: parsed.discount_minor }))
}

/// Called from inside `create_sale` when the completed sale has a table: any
/// still-open parked order for that table is closed out against the new
/// sale, and the table is freed for the next customers.
fn close_table_order_for_sale(tx: &Transaction, table_id: i64, sale_id: i64) -> Result<(), rusqlite::Error> {
    let existing_order_id: Option<i64> = tx
        .query_row(
            "SELECT id FROM table_orders WHERE table_id = ?1 AND status = 'open'",
            params![table_id],
            |row| row.get(0),
        )
        .optional()?;

    match existing_order_id {
        Some(order_id) => {
            tx.execute(
                "UPDATE table_orders
                    SET sale_id = ?1, status = 'billed', closed_at = datetime('now', 'localtime')
                  WHERE id = ?2",
                params![sale_id, order_id],
            )?;
        }
        None => {
            tx.execute(
                "INSERT INTO table_orders (table_id, sale_id, status, closed_at)
                 VALUES (?1, ?2, 'billed', datetime('now', 'localtime'))",
                params![table_id, sale_id],
            )?;
        }
    }

    tx.execute("UPDATE tables SET status = 'free' WHERE id = ?1", params![table_id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::test_conn;

    fn item_id(conn: &Connection, name: &str) -> i64 {
        conn.query_row("SELECT id FROM items WHERE name = ?1", params![name], |row| row.get(0))
            .unwrap()
    }

    fn stock(conn: &Connection, name: &str) -> i64 {
        conn.query_row(
            "SELECT stock_qty FROM items WHERE name = ?1",
            params![name],
            |row| row.get(0),
        )
        .unwrap()
    }

    fn table_id(conn: &Connection, name: &str) -> i64 {
        conn.query_row("SELECT id FROM tables WHERE name = ?1", params![name], |row| row.get(0))
            .unwrap()
    }

    fn basic_input(conn: &Connection) -> CreateSaleInput {
        CreateSaleInput {
            items: vec![CartLine { item_id: item_id(conn, "Cola 500ml"), qty: 2 }],
            discount_minor: 0,
            tax_minor: 0,
            payment_method: "cash".into(),
            cashier_id: None,
            table_id: None,
        }
    }

    #[test]
    fn completes_a_sale_and_decrements_stock() {
        let mut conn = test_conn();
        let cola_before = stock(&conn, "Cola 500ml");

        let tx = conn.transaction().unwrap();
        let sale = create_sale(&tx, basic_input(&tx)).unwrap();
        tx.commit().unwrap();

        assert_eq!(sale.items.len(), 1);
        assert_eq!(sale.items[0].qty, 2);
        assert_eq!(sale.subtotal_minor, 2 * 8000);
        assert_eq!(sale.total_minor, 2 * 8000);
        assert_eq!(stock(&conn, "Cola 500ml"), cola_before - 2);

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
            items: vec![CartLine { item_id: cola, qty: 0 }],
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
                    CartLine { item_id: item_id(&tx, "Cola 500ml"), qty: 1 },
                    CartLine { item_id: item_id(&tx, "Instant Noodles"), qty: 999 },
                ],
                discount_minor: 0,
                tax_minor: 0,
                payment_method: "cash".into(),
                cashier_id: None,
                table_id: None,
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

    #[test]
    fn rejects_an_archived_item() {
        let mut conn = test_conn();
        let cola = item_id(&conn, "Cola 500ml");
        conn.execute("UPDATE items SET is_active = 0 WHERE id = ?1", params![cola]).unwrap();

        let tx = conn.transaction().unwrap();
        let input = CreateSaleInput {
            items: vec![CartLine { item_id: cola, qty: 1 }],
            discount_minor: 0,
            tax_minor: 0,
            payment_method: "cash".into(),
            cashier_id: None,
            table_id: None,
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
                CartLine { item_id: cola, qty: 2 },
                CartLine { item_id: cola, qty: 3 },
            ],
            discount_minor: 0,
            tax_minor: 0,
            payment_method: "cash".into(),
            cashier_id: None,
            table_id: None,
        };
        let sale = create_sale(&tx, input).unwrap();
        tx.commit().unwrap();

        assert_eq!(sale.items.len(), 1, "one merged line, not two");
        assert_eq!(sale.items[0].qty, 5);
        assert_eq!(stock(&conn, "Cola 500ml"), 42 - 5);
    }

    #[test]
    fn get_sale_reports_not_found_for_an_unknown_id() {
        let conn = test_conn();
        assert!(matches!(get_sale(&conn, 999_999), Err(SaleError::NotFound)));
    }

    #[test]
    fn a_sale_with_a_table_closes_the_parked_order_and_frees_the_table() {
        let mut conn = test_conn();
        let cola = item_id(&conn, "Cola 500ml");
        let t1 = table_id(&conn, "Table 1"); // seeded status: occupied

        attach_cart_to_table(&conn, t1, &[ParkedCartLine { item_id: cola, qty: 2 }], 0).unwrap();
        let status_before: String =
            conn.query_row("SELECT status FROM tables WHERE id = ?1", params![t1], |row| row.get(0)).unwrap();
        assert_eq!(status_before, "occupied");

        let tx = conn.transaction().unwrap();
        let input = CreateSaleInput {
            items: vec![CartLine { item_id: cola, qty: 2 }],
            discount_minor: 0,
            tax_minor: 0,
            payment_method: "cash".into(),
            cashier_id: None,
            table_id: Some(t1),
        };
        let sale = create_sale(&tx, input).unwrap();
        tx.commit().unwrap();

        let (order_status, order_sale_id): (String, Option<i64>) = conn
            .query_row(
                "SELECT status, sale_id FROM table_orders WHERE table_id = ?1",
                params![t1],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(order_status, "billed");
        assert_eq!(order_sale_id, Some(sale.id));

        let status_after: String =
            conn.query_row("SELECT status FROM tables WHERE id = ?1", params![t1], |row| row.get(0)).unwrap();
        assert_eq!(status_after, "free");

        assert!(get_parked_cart(&conn, t1).unwrap().is_none(), "parked cart should be gone once billed");
    }

    #[test]
    fn attaching_a_cart_twice_updates_the_same_open_order() {
        let conn = test_conn();
        let cola = item_id(&conn, "Cola 500ml");
        let t2 = table_id(&conn, "Table 2"); // seeded status: free

        attach_cart_to_table(&conn, t2, &[ParkedCartLine { item_id: cola, qty: 1 }], 0).unwrap();
        attach_cart_to_table(&conn, t2, &[ParkedCartLine { item_id: cola, qty: 4 }], 200).unwrap();

        let order_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM table_orders WHERE table_id = ?1",
                params![t2],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(order_count, 1, "second attach should update, not duplicate");

        let parked = get_parked_cart(&conn, t2).unwrap().unwrap();
        assert_eq!(parked.items[0].qty, 4);
        assert_eq!(parked.discount_minor, 200);

        let status: String =
            conn.query_row("SELECT status FROM tables WHERE id = ?1", params![t2], |row| row.get(0)).unwrap();
        assert_eq!(status, "occupied");
    }

    #[test]
    fn attach_cart_rejects_empty_items_and_unknown_table() {
        let conn = test_conn();
        let cola = item_id(&conn, "Cola 500ml");
        let t2 = table_id(&conn, "Table 2");

        assert!(matches!(
            attach_cart_to_table(&conn, t2, &[], 0),
            Err(TableOrderError::EmptyCart)
        ));
        assert!(matches!(
            attach_cart_to_table(&conn, 999_999, &[ParkedCartLine { item_id: cola, qty: 1 }], 0),
            Err(TableOrderError::TableNotFound)
        ));
    }

    #[test]
    fn list_tables_reports_status_and_parked_flag() {
        let conn = test_conn();
        let cola = item_id(&conn, "Cola 500ml");
        let t3 = table_id(&conn, "Table 3");
        attach_cart_to_table(&conn, t3, &[ParkedCartLine { item_id: cola, qty: 1 }], 0).unwrap();

        let tables = list_tables(&conn).unwrap();
        assert_eq!(tables.len(), 6, "all six seeded tables should be listed");
        let table3 = tables.iter().find(|t| t.id == t3).unwrap();
        assert!(table3.has_parked_order);
        assert_eq!(table3.status, "occupied");
    }
}
