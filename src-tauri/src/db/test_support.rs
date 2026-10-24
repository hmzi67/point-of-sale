//! Fixtures shared by the per-area `#[cfg(test)]` modules. Every test DB
//! comes from `schema::test_conn()`, which seeds the fixed demo catalogue
//! ("Cola 500ml" at 8000, "Instant Noodles", …), so these look items up by
//! that name rather than hard-coding ids.

use rusqlite::{params, Connection, Transaction};

use crate::db::items::ItemInput;
use crate::db::sales::{create_sale, CartLine, CreateSaleInput};

/// Runs `f` in a transaction and commits only on `Ok`, like
/// `Db::try_transaction` — for domain functions that take a `&Transaction`.
pub fn in_tx<T, E>(conn: &mut Connection, f: impl FnOnce(&Transaction) -> Result<T, E>) -> Result<T, E> {
    let tx = conn.transaction().unwrap();
    let result = f(&tx);
    if result.is_ok() {
        tx.commit().unwrap();
    }
    result
}

pub fn item_id(conn: &Connection, name: &str) -> i64 {
    conn.query_row("SELECT id FROM items WHERE name = ?1", params![name], |row| row.get(0))
        .unwrap()
}

pub fn table_id(conn: &Connection, name: &str) -> i64 {
    conn.query_row("SELECT id FROM tables WHERE name = ?1", params![name], |row| row.get(0))
        .unwrap()
}

pub fn employee_id(conn: &Connection, name: &str) -> i64 {
    conn.query_row("SELECT id FROM employees WHERE name = ?1", params![name], |row| row.get(0))
        .unwrap()
}

/// The single integer a `SELECT COUNT(*) …`-style query returns.
pub fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |row| row.get(0)).unwrap()
}

/// A plain per-piece item at 100.00 (cost 80.00), uncategorised, no barcode.
pub fn item_input(name: &str, stock_qty: f64) -> ItemInput {
    ItemInput {
        name: name.to_string(),
        barcode: None,
        short_code: None,
        description: None,
        price_minor: 10000,
        cost_minor: 8000,
        stock_qty,
        category_id: None,
        low_stock_threshold: 2,
        image_path: None,
        sold_by_amount: false,
        unit: None,
        counter_id: None,
        original_stock_qty: None,
    }
}

/// The single seeded Owner account.
pub fn owner_id(conn: &Connection) -> i64 {
    conn.query_row("SELECT id FROM users WHERE role = 'owner'", [], |row| row.get(0))
        .unwrap()
}

pub fn stock(conn: &Connection, name: &str) -> f64 {
    conn.query_row("SELECT stock_qty FROM items WHERE name = ?1", params![name], |row| row.get(0))
        .unwrap()
}

/// A one-line, no-discount, no-tax cash sale input — override fields with
/// struct-update syntax (`CreateSaleInput { table_id: …, ..one_line_sale(…) }`).
pub fn one_line_sale(item_id: i64, qty: f64) -> CreateSaleInput {
    CreateSaleInput {
        items: vec![CartLine { item_id, qty, notes: None }],
        discount_minor: 0,
        tax_minor: 0,
        payment_method: "cash".into(),
        cashier_id: None,
        table_id: None,
        shift_id: None,
        tendered_minor: None,
        change_minor: None,
        ..Default::default()
    }
}

/// Commits a one-line, no-discount, no-tax sale of `qty` × `item` and
/// returns its id.
pub fn ring_up(conn: &mut Connection, item: &str, qty: f64, payment_method: &str, shift_id: Option<i64>) -> i64 {
    let tx = conn.transaction().unwrap();
    let item_id = item_id(&tx, item);
    let sale = create_sale(
        &tx,
        CreateSaleInput { payment_method: payment_method.into(), shift_id, ..one_line_sale(item_id, qty) },
    )
    .unwrap();
    tx.commit().unwrap();
    sale.id
}

/// The single `sale_items` row of a one-line sale.
pub fn sale_item_id(conn: &Connection, sale_id: i64) -> i64 {
    conn.query_row("SELECT id FROM sale_items WHERE sale_id = ?1", params![sale_id], |row| row.get(0))
        .unwrap()
}

/// `ring_up` for a cash sale, plus its one `sale_items` id — for tests that
/// hang a refund off a real line.
pub fn ring_up_with_line(conn: &mut Connection, item: &str, qty: f64) -> (i64, i64) {
    let sale_id = ring_up(conn, item, qty, "cash", None);
    (sale_id, sale_item_id(conn, sale_id))
}

/// Moves a sale (and its refunds, if any) to `timestamp`, so a report test
/// can query a fixed day instead of "today" — which a run crossing midnight
/// would split.
pub fn backdate_sale(conn: &Connection, sale_id: i64, timestamp: &str) {
    conn.execute("UPDATE sales SET created_at = ?1 WHERE id = ?2", params![timestamp, sale_id]).unwrap();
}

/// Today's local date, `YYYY-MM-DD`.
pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// A local date `days` before today, `YYYY-MM-DD`.
pub fn days_ago(days: i64) -> String {
    (chrono::Local::now().date_naive() - chrono::Duration::days(days)).format("%Y-%m-%d").to_string()
}

/// The current local month, `YYYY-MM`.
pub fn this_month() -> String {
    chrono::Local::now().format("%Y-%m").to_string()
}
