//! Refunds against a completed sale — full or partial, line-by-line.
//!
//! A refund never edits the original sale/sale_items rows (those stay
//! immutable, same as everywhere else in the schema); it is its own ledger
//! entry that references them. `create_refund` mirrors `sales::create_sale`
//! in shape: re-validate everything against the live database inside one
//! transaction, insert the refund + its lines, and put the refunded
//! quantity back onto `items.stock_qty` — the exact inverse of what
//! `create_sale` decremented.

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Refundable-sale lookup — what the refund UI shows before anything is
// submitted, accounting for whatever a previous partial refund already took.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefundableLine {
    pub sale_item_id: i64,
    pub item_id: i64,
    pub item_name: String,
    pub qty: i64,
    pub price_at_sale_minor: i64,
    /// Summed from every previous refund against this line.
    pub qty_already_refunded: i64,
    /// `qty - qty_already_refunded` — never negative; the UI's upper bound
    /// on how much of this line can still be refunded.
    pub qty_refundable: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefundableSale {
    pub sale_id: i64,
    pub created_at: String,
    pub total_minor: i64,
    pub payment_method: String,
    pub items: Vec<RefundableLine>,
}

#[derive(Debug)]
pub enum RefundError {
    SaleNotFound(i64),
    EmptyRefund,
    SaleItemNotFound(i64),
    /// `sale_item_id` exists but belongs to a different sale than the one
    /// the refund claims to be against — never trust the client to have
    /// paired them correctly.
    SaleItemMismatch { sale_item_id: i64, expected_sale_id: i64 },
    InvalidQuantity,
    OverRefund { sale_item_id: i64, refundable: i64, requested: i64 },
    InvalidAmount { sale_item_id: i64, max_allowed_minor: i64, requested_minor: i64 },
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for RefundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RefundError::SaleNotFound(id) => write!(f, "Sale {} not found", id),
            RefundError::EmptyRefund => write!(f, "Select at least one item to refund"),
            RefundError::SaleItemNotFound(id) => write!(f, "Sale line {} no longer exists", id),
            RefundError::SaleItemMismatch { sale_item_id, expected_sale_id } => write!(
                f,
                "Sale line {} does not belong to sale {}",
                sale_item_id, expected_sale_id
            ),
            RefundError::InvalidQuantity => write!(f, "Refund quantity must be at least 1"),
            RefundError::OverRefund { sale_item_id, refundable, requested } => write!(
                f,
                "Only {} left refundable on line {}, but {} requested",
                refundable, sale_item_id, requested
            ),
            RefundError::InvalidAmount { sale_item_id, max_allowed_minor, requested_minor } => write!(
                f,
                "Refund amount for line {} cannot exceed {} (requested {})",
                sale_item_id, max_allowed_minor, requested_minor
            ),
            RefundError::Sqlite(err) => write!(f, "database error: {}", err),
        }
    }
}

impl From<rusqlite::Error> for RefundError {
    fn from(err: rusqlite::Error) -> Self {
        RefundError::Sqlite(err)
    }
}

/// Sums already-refunded qty per `sale_item_id`, for the running "how much
/// is left" check both `get_sale_for_refund` (display) and `create_refund`
/// (validation) need.
fn already_refunded(conn: &Connection, sale_item_id: i64) -> Result<i64, rusqlite::Error> {
    conn.query_row(
        "SELECT COALESCE(SUM(qty_refunded), 0) FROM refund_items WHERE sale_item_id = ?1",
        params![sale_item_id],
        |row| row.get(0),
    )
}

/// The original sale plus, per line, how much of it is still refundable.
pub fn get_sale_for_refund(conn: &Connection, sale_id: i64) -> Result<RefundableSale, RefundError> {
    let (created_at, total_minor, payment_method): (String, i64, String) = conn
        .query_row(
            "SELECT created_at, total_minor, payment_method FROM sales WHERE id = ?1",
            params![sale_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or(RefundError::SaleNotFound(sale_id))?;

    let mut stmt = conn.prepare(
        "SELECT si.id, si.item_id, i.name, si.qty, si.price_at_sale_minor
           FROM sale_items si
           JOIN items i ON i.id = si.item_id
          WHERE si.sale_id = ?1
          ORDER BY si.id",
    )?;
    let rows: Vec<(i64, i64, String, i64, i64)> = stmt
        .query_map(params![sale_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut items = Vec::with_capacity(rows.len());
    for (sale_item_id, item_id, item_name, qty, price_at_sale_minor) in rows {
        let qty_already_refunded = already_refunded(conn, sale_item_id)?;
        items.push(RefundableLine {
            sale_item_id,
            item_id,
            item_name,
            qty,
            price_at_sale_minor,
            qty_already_refunded,
            qty_refundable: (qty - qty_already_refunded).max(0),
        });
    }

    Ok(RefundableSale { sale_id, created_at, total_minor, payment_method, items })
}

// ---------------------------------------------------------------------------
// Creating a refund
// ---------------------------------------------------------------------------

/// One line the client wants refunded. `amount_refunded_minor` is trusted
/// only up to `qty_refunded * price_at_sale_minor` — the same "never let a
/// client-sent number exceed what the priced-at-checkout row actually
/// allows" rule `create_sale` applies to price, applied here to refund
/// amount. Trusted below that ceiling (rather than always recomputing the
/// full proportional amount) so a manual reduction — a restocking fee, a
/// partial goodwill refund — is representable without a separate field.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefundLineInput {
    pub sale_item_id: i64,
    pub qty: i64,
    pub amount_minor: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRefundInput {
    pub sale_id: i64,
    pub items: Vec<RefundLineInput>,
    pub reason: Option<String>,
    pub refunded_by: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefundLine {
    pub sale_item_id: i64,
    pub item_id: i64,
    pub item_name: String,
    pub qty_refunded: i64,
    pub amount_refunded_minor: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Refund {
    pub id: i64,
    pub original_sale_id: i64,
    pub refunded_by: Option<i64>,
    pub refunded_by_name: Option<String>,
    pub reason: Option<String>,
    pub total_refund_amount_minor: i64,
    pub created_at: String,
    pub items: Vec<RefundLine>,
}

/// Validates and writes a refund inside `tx`: re-checks every line against
/// the live `sale_items` + prior-refunds state, inserts `refunds` +
/// `refund_items`, and puts the refunded quantity back onto
/// `items.stock_qty`. Every step happens on the same transaction, so — same
/// guarantee as `create_sale` — any `?` bailing out here leaves nothing
/// behind once the caller's `tx` is dropped without a commit.
pub fn create_refund(tx: &Transaction, input: CreateRefundInput) -> Result<Refund, RefundError> {
    if input.items.is_empty() {
        return Err(RefundError::EmptyRefund);
    }

    let sale_exists: bool = tx
        .query_row("SELECT 1 FROM sales WHERE id = ?1", params![input.sale_id], |_| Ok(true))
        .optional()?
        .unwrap_or(false);
    if !sale_exists {
        return Err(RefundError::SaleNotFound(input.sale_id));
    }

    // (sale_item_id, item_id, item_name, qty_refunded, amount_refunded_minor)
    let mut resolved: Vec<(i64, i64, String, i64, i64)> = Vec::with_capacity(input.items.len());
    let mut total_refund_amount_minor: i64 = 0;

    for line in &input.items {
        if line.qty <= 0 {
            return Err(RefundError::InvalidQuantity);
        }

        let row: Option<(i64, i64, String, i64, i64)> = tx
            .query_row(
                "SELECT si.sale_id, si.item_id, i.name, si.qty, si.price_at_sale_minor
                   FROM sale_items si
                   JOIN items i ON i.id = si.item_id
                  WHERE si.id = ?1",
                params![line.sale_item_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .optional()?;
        let (sale_id, item_id, item_name, qty, price_at_sale_minor) =
            row.ok_or(RefundError::SaleItemNotFound(line.sale_item_id))?;

        if sale_id != input.sale_id {
            return Err(RefundError::SaleItemMismatch {
                sale_item_id: line.sale_item_id,
                expected_sale_id: input.sale_id,
            });
        }

        let already = already_refunded(tx, line.sale_item_id)?;
        let refundable = qty - already;
        if line.qty > refundable {
            return Err(RefundError::OverRefund {
                sale_item_id: line.sale_item_id,
                refundable,
                requested: line.qty,
            });
        }

        let max_allowed_minor = price_at_sale_minor * line.qty;
        if line.amount_minor < 0 || line.amount_minor > max_allowed_minor {
            return Err(RefundError::InvalidAmount {
                sale_item_id: line.sale_item_id,
                max_allowed_minor,
                requested_minor: line.amount_minor,
            });
        }

        total_refund_amount_minor += line.amount_minor;
        resolved.push((line.sale_item_id, item_id, item_name, line.qty, line.amount_minor));
    }

    let reason = input.reason.as_deref().map(str::trim).filter(|s| !s.is_empty());
    tx.execute(
        "INSERT INTO refunds (original_sale_id, refunded_by, reason, total_refund_amount_minor)
         VALUES (?1, ?2, ?3, ?4)",
        params![input.sale_id, input.refunded_by, reason, total_refund_amount_minor],
    )?;
    let refund_id = tx.last_insert_rowid();

    let mut items = Vec::with_capacity(resolved.len());
    for (sale_item_id, item_id, item_name, qty_refunded, amount_refunded_minor) in resolved {
        tx.execute(
            "INSERT INTO refund_items (refund_id, sale_item_id, qty_refunded, amount_refunded_minor)
             VALUES (?1, ?2, ?3, ?4)",
            params![refund_id, sale_item_id, qty_refunded, amount_refunded_minor],
        )?;

        // The mirror of create_sale's stock decrement.
        tx.execute(
            "UPDATE items SET stock_qty = stock_qty + ?1 WHERE id = ?2",
            params![qty_refunded, item_id],
        )?;

        items.push(RefundLine { sale_item_id, item_id, item_name, qty_refunded, amount_refunded_minor });
    }

    let refunded_by_name = match input.refunded_by {
        Some(id) => tx
            .query_row("SELECT name FROM users WHERE id = ?1", params![id], |row| row.get(0))
            .optional()?,
        None => None,
    };
    let created_at: String =
        tx.query_row("SELECT created_at FROM refunds WHERE id = ?1", params![refund_id], |row| row.get(0))?;

    Ok(Refund {
        id: refund_id,
        original_sale_id: input.sale_id,
        refunded_by: input.refunded_by,
        refunded_by_name,
        reason: reason.map(str::to_string),
        total_refund_amount_minor,
        created_at,
        items,
    })
}

/// Re-fetches a previously created refund (by id) — used for reprinting a
/// refund receipt, the same "take an id, reload from the database" shape
/// `sales::get_sale` uses for the customer receipt.
pub fn get_refund(conn: &Connection, refund_id: i64) -> Result<Refund, RefundError> {
    let row: Option<(i64, Option<i64>, Option<String>, Option<String>, i64, String)> = conn
        .query_row(
            "SELECT r.original_sale_id, r.refunded_by, u.name, r.reason,
                    r.total_refund_amount_minor, r.created_at
               FROM refunds r
               LEFT JOIN users u ON u.id = r.refunded_by
              WHERE r.id = ?1",
            params![refund_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
        )
        .optional()?;
    let (original_sale_id, refunded_by, refunded_by_name, reason, total_refund_amount_minor, created_at) =
        row.ok_or(RefundError::SaleNotFound(refund_id))?;

    let mut stmt = conn.prepare(
        "SELECT ri.sale_item_id, si.item_id, i.name, ri.qty_refunded, ri.amount_refunded_minor
           FROM refund_items ri
           JOIN sale_items si ON si.id = ri.sale_item_id
           JOIN items i ON i.id = si.item_id
          WHERE ri.refund_id = ?1
          ORDER BY ri.id",
    )?;
    let items = stmt
        .query_map(params![refund_id], |row| {
            Ok(RefundLine {
                sale_item_id: row.get(0)?,
                item_id: row.get(1)?,
                item_name: row.get(2)?,
                qty_refunded: row.get(3)?,
                amount_refunded_minor: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Refund { id: refund_id, original_sale_id, refunded_by, refunded_by_name, reason, total_refund_amount_minor, created_at, items })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::sales::{create_sale, CartLine, CreateSaleInput};
    use crate::db::schema::test_conn;

    fn item_id(conn: &Connection, name: &str) -> i64 {
        conn.query_row("SELECT id FROM items WHERE name = ?1", params![name], |row| row.get(0))
            .unwrap()
    }

    fn stock(conn: &Connection, name: &str) -> i64 {
        conn.query_row("SELECT stock_qty FROM items WHERE name = ?1", params![name], |row| row.get(0))
            .unwrap()
    }

    /// Rings up a fresh Cola x4 sale and returns its id — a clean, known
    /// starting point every test below builds a refund against.
    fn seed_sale(conn: &mut Connection) -> i64 {
        let tx = conn.transaction().unwrap();
        let cola = item_id(&tx, "Cola 500ml");
        let sale = create_sale(
            &tx,
            CreateSaleInput {
                items: vec![CartLine { item_id: cola, qty: 4, notes: None }],
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
        sale.id
    }

    fn sale_item_id(conn: &Connection, sale_id: i64) -> i64 {
        conn.query_row("SELECT id FROM sale_items WHERE sale_id = ?1", params![sale_id], |row| row.get(0))
            .unwrap()
    }

    #[test]
    fn a_partial_refund_restores_exactly_the_refunded_stock() {
        let mut conn = test_conn();
        let cola_before_sale = stock(&conn, "Cola 500ml");
        let sale_id = seed_sale(&mut conn);
        assert_eq!(stock(&conn, "Cola 500ml"), cola_before_sale - 4);

        let si = sale_item_id(&conn, sale_id);
        let tx = conn.transaction().unwrap();
        let refund = create_refund(
            &tx,
            CreateRefundInput {
                sale_id,
                items: vec![RefundLineInput { sale_item_id: si, qty: 2, amount_minor: 2 * 8000 }],
                reason: Some("Customer changed mind".into()),
                refunded_by: None,
            },
        )
        .unwrap();
        tx.commit().unwrap();

        assert_eq!(refund.total_refund_amount_minor, 16000);
        assert_eq!(stock(&conn, "Cola 500ml"), cola_before_sale - 4 + 2, "only the refunded qty comes back");
    }

    #[test]
    fn get_sale_for_refund_accounts_for_a_prior_partial_refund() {
        let mut conn = test_conn();
        let sale_id = seed_sale(&mut conn);
        let si = sale_item_id(&conn, sale_id);

        {
            let tx = conn.transaction().unwrap();
            create_refund(
                &tx,
                CreateRefundInput {
                    sale_id,
                    items: vec![RefundLineInput { sale_item_id: si, qty: 1, amount_minor: 8000 }],
                    reason: None,
                    refunded_by: None,
                },
            )
            .unwrap();
            tx.commit().unwrap();
        }

        let refundable = get_sale_for_refund(&conn, sale_id).unwrap();
        assert_eq!(refundable.items.len(), 1);
        assert_eq!(refundable.items[0].qty, 4);
        assert_eq!(refundable.items[0].qty_already_refunded, 1);
        assert_eq!(refundable.items[0].qty_refundable, 3, "3 of the original 4 remain refundable");
    }

    #[test]
    fn refunding_more_than_remains_is_rejected_and_rolls_back() {
        let mut conn = test_conn();
        let sale_id = seed_sale(&mut conn);
        let si = sale_item_id(&conn, sale_id);
        let stock_before = stock(&conn, "Cola 500ml");

        let tx = conn.transaction().unwrap();
        let err = create_refund(
            &tx,
            CreateRefundInput {
                sale_id,
                items: vec![RefundLineInput { sale_item_id: si, qty: 5, amount_minor: 5 * 8000 }],
                reason: None,
                refunded_by: None,
            },
        )
        .unwrap_err();
        assert!(matches!(err, RefundError::OverRefund { refundable: 4, requested: 5, .. }));
        drop(tx);

        assert_eq!(stock(&conn, "Cola 500ml"), stock_before, "a rejected refund must not touch stock");
    }

    #[test]
    fn two_partial_refunds_cannot_together_exceed_the_original_quantity() {
        let mut conn = test_conn();
        let sale_id = seed_sale(&mut conn);
        let si = sale_item_id(&conn, sale_id);

        {
            let tx = conn.transaction().unwrap();
            create_refund(
                &tx,
                CreateRefundInput {
                    sale_id,
                    items: vec![RefundLineInput { sale_item_id: si, qty: 3, amount_minor: 3 * 8000 }],
                    reason: None,
                    refunded_by: None,
                },
            )
            .unwrap();
            tx.commit().unwrap();
        }

        // Only 1 remains refundable; asking for 2 more must fail.
        let tx = conn.transaction().unwrap();
        let err = create_refund(
            &tx,
            CreateRefundInput {
                sale_id,
                items: vec![RefundLineInput { sale_item_id: si, qty: 2, amount_minor: 2 * 8000 }],
                reason: None,
                refunded_by: None,
            },
        )
        .unwrap_err();
        assert!(matches!(err, RefundError::OverRefund { refundable: 1, requested: 2, .. }));
    }

    #[test]
    fn refund_amount_cannot_exceed_the_line_s_original_value() {
        let mut conn = test_conn();
        let sale_id = seed_sale(&mut conn);
        let si = sale_item_id(&conn, sale_id);

        let tx = conn.transaction().unwrap();
        let err = create_refund(
            &tx,
            CreateRefundInput {
                sale_id,
                // 1 unit is worth 8000, not 999999 — a tampered/buggy client
                // value must be rejected, never trusted onto the ledger.
                items: vec![RefundLineInput { sale_item_id: si, qty: 1, amount_minor: 999_999 }],
                reason: None,
                refunded_by: None,
            },
        )
        .unwrap_err();
        assert!(matches!(err, RefundError::InvalidAmount { max_allowed_minor: 8000, .. }));
    }

    #[test]
    fn a_sale_item_from_a_different_sale_is_rejected() {
        let mut conn = test_conn();
        let sale_a = seed_sale(&mut conn);
        let sale_b = seed_sale(&mut conn);
        let si_from_b = sale_item_id(&conn, sale_b);

        let tx = conn.transaction().unwrap();
        let err = create_refund(
            &tx,
            CreateRefundInput {
                // Claims to be against sale_a, but the line actually
                // belongs to sale_b.
                sale_id: sale_a,
                items: vec![RefundLineInput { sale_item_id: si_from_b, qty: 1, amount_minor: 8000 }],
                reason: None,
                refunded_by: None,
            },
        )
        .unwrap_err();
        assert!(matches!(err, RefundError::SaleItemMismatch { .. }));
    }

    #[test]
    fn rejects_an_empty_refund() {
        let mut conn = test_conn();
        let sale_id = seed_sale(&mut conn);
        let tx = conn.transaction().unwrap();
        let err = create_refund(
            &tx,
            CreateRefundInput { sale_id, items: vec![], reason: None, refunded_by: None },
        )
        .unwrap_err();
        assert!(matches!(err, RefundError::EmptyRefund));
    }

    #[test]
    fn rejects_an_unknown_sale() {
        let conn = test_conn();
        assert!(matches!(get_sale_for_refund(&conn, 999_999), Err(RefundError::SaleNotFound(999_999))));
    }

    #[test]
    fn a_created_refund_can_be_reloaded_by_id_for_reprint() {
        let mut conn = test_conn();
        let sale_id = seed_sale(&mut conn);
        let si = sale_item_id(&conn, sale_id);

        let created = {
            let tx = conn.transaction().unwrap();
            let refund = create_refund(
                &tx,
                CreateRefundInput {
                    sale_id,
                    items: vec![RefundLineInput { sale_item_id: si, qty: 1, amount_minor: 8000 }],
                    reason: Some("Wrong item".into()),
                    refunded_by: None,
                },
            )
            .unwrap();
            tx.commit().unwrap();
            refund
        };

        let reloaded = get_refund(&conn, created.id).unwrap();
        assert_eq!(reloaded.original_sale_id, sale_id);
        assert_eq!(reloaded.total_refund_amount_minor, 8000);
        assert_eq!(reloaded.reason.as_deref(), Some("Wrong item"));
        assert_eq!(reloaded.items.len(), 1);
        assert_eq!(reloaded.items[0].item_name, "Cola 500ml");
    }
}
