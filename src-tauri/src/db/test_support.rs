//! Fixtures shared by the per-area `#[cfg(test)]` modules. Every test DB
//! comes from `schema::test_conn()`, which seeds the fixed demo catalogue
//! ("Cola 500ml" at 8000, "Instant Noodles", …), so these look items up by
//! that name rather than hard-coding ids.

use rusqlite::{params, Connection};

use crate::db::sales::{create_sale, CartLine, CreateSaleInput};

pub fn item_id(conn: &Connection, name: &str) -> i64 {
    conn.query_row("SELECT id FROM items WHERE name = ?1", params![name], |row| row.get(0))
        .unwrap()
}

pub fn table_id(conn: &Connection, name: &str) -> i64 {
    conn.query_row("SELECT id FROM tables WHERE name = ?1", params![name], |row| row.get(0))
        .unwrap()
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
