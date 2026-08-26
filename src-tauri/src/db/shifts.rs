//! Cashier shift open/close and the cash-drawer reconciliation summary.
//!
//! No credit/tab sales exist in this schema (`sales.payment_method` is
//! `cash` | `card` | `other` — see `schema.sql`), so `credit_sales_minor`
//! below is always `0`; it's still a real field (not omitted) so a receipt
//! template that shows it doesn't need a special case for "this product
//! doesn't have that concept" versus "it happened to be zero this shift".

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::Serialize;

#[derive(Debug)]
pub enum ShiftError {
    AlreadyOpen { existing_shift_id: i64 },
    NotFound(i64),
    AlreadyClosed(i64),
    InvalidOpeningBalance,
    InvalidDeclaredAmount,
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for ShiftError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShiftError::AlreadyOpen { existing_shift_id } => {
                write!(f, "You already have an open shift (#{})", existing_shift_id)
            }
            ShiftError::NotFound(id) => write!(f, "Shift {} not found", id),
            ShiftError::AlreadyClosed(id) => write!(f, "Shift {} is already closed", id),
            ShiftError::InvalidOpeningBalance => write!(f, "Opening balance cannot be negative"),
            ShiftError::InvalidDeclaredAmount => write!(f, "Declared cash amount cannot be negative"),
            ShiftError::Sqlite(err) => write!(f, "database error: {}", err),
        }
    }
}

impl From<rusqlite::Error> for ShiftError {
    fn from(err: rusqlite::Error) -> Self {
        ShiftError::Sqlite(err)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Shift {
    pub id: i64,
    pub cashier_id: Option<i64>,
    pub cashier_name: Option<String>,
    pub opened_at: String,
    pub closed_at: Option<String>,
    pub opening_balance_minor: i64,
    pub declared_cash_amount_minor: Option<i64>,
    pub notes: Option<String>,
}

fn load_shift(conn: &Connection, id: i64) -> Result<Option<Shift>, rusqlite::Error> {
    conn.query_row(
        "SELECT s.id, s.cashier_id, u.name, s.opened_at, s.closed_at,
                s.opening_balance_minor, s.declared_cash_amount_minor, s.notes
           FROM shifts s
           LEFT JOIN users u ON u.id = s.cashier_id
          WHERE s.id = ?1",
        params![id],
        |row| {
            Ok(Shift {
                id: row.get(0)?,
                cashier_id: row.get(1)?,
                cashier_name: row.get(2)?,
                opened_at: row.get(3)?,
                closed_at: row.get(4)?,
                opening_balance_minor: row.get(5)?,
                declared_cash_amount_minor: row.get(6)?,
                notes: row.get(7)?,
            })
        },
    )
    .optional()
}

/// The cashier's currently-open shift, if any — used both to block opening a
/// second one and to let the billing screen know whether to show "Open
/// Shift" or "Close Shift".
pub fn get_open_shift_for_cashier(conn: &Connection, cashier_id: i64) -> Result<Option<Shift>, ShiftError> {
    let id: Option<i64> = conn
        .query_row(
            "SELECT id FROM shifts WHERE cashier_id = ?1 AND closed_at IS NULL",
            params![cashier_id],
            |row| row.get(0),
        )
        .optional()?;
    match id {
        Some(id) => Ok(load_shift(conn, id)?),
        None => Ok(None),
    }
}

/// Opens a new shift for `cashier_id`. Refuses if that cashier already has
/// one open — a cashier's cash drawer is reconciled one shift at a time,
/// never two overlapping ones.
pub fn open_shift(conn: &Connection, cashier_id: i64, opening_balance_minor: i64) -> Result<Shift, ShiftError> {
    if opening_balance_minor < 0 {
        return Err(ShiftError::InvalidOpeningBalance);
    }
    if let Some(existing) = get_open_shift_for_cashier(conn, cashier_id)? {
        return Err(ShiftError::AlreadyOpen { existing_shift_id: existing.id });
    }

    conn.execute(
        "INSERT INTO shifts (cashier_id, opening_balance_minor) VALUES (?1, ?2)",
        params![cashier_id, opening_balance_minor],
    )?;
    let id = conn.last_insert_rowid();
    load_shift(conn, id)?.ok_or(ShiftError::NotFound(id))
}

// ---------------------------------------------------------------------------
// Reconciliation summary
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShiftSummary {
    pub shift: Shift,
    pub opening_balance_minor: i64,
    pub cash_sales_minor: i64,
    pub card_sales_minor: i64,
    pub other_sales_minor: i64,
    /// Always 0 — see the module doc comment. Present so a template that
    /// shows it (matching a reference receipt with a "Credit Sale" line)
    /// never needs a special case for this product not having the concept.
    pub credit_sales_minor: i64,
    pub total_sales_minor: i64,
    /// Sum of `sale_items` discount applied across this shift's sales —
    /// "today's discount total" on the close-out receipt.
    pub discount_minor: i64,
    /// Refunds recorded (by `created_at`, not by which shift the *original*
    /// sale happened in) during this shift's open window.
    pub refunds_minor: i64,
    /// `opening_balance + cash_sales - refunds` — refunds are assumed to
    /// come back out of the cash drawer regardless of how the original sale
    /// was paid, same as a real till: a card sale refunded in cash still
    /// empties the drawer by that amount.
    pub expected_cash_minor: i64,
    /// `None` until the shift is closed (or a live preview amount is
    /// supplied — see `get_shift_summary`).
    pub declared_cash_amount_minor: Option<i64>,
    /// `declared - expected`: negative is Short, positive is Over, matching
    /// the reference receipt's labeling. `None` alongside
    /// `declared_cash_amount_minor`.
    pub difference_minor: Option<i64>,
}

/// The half of `ShiftSummary` that's pure aggregation over `sales`/
/// `refunds` — shared by `get_shift_summary` (reads the shift's own
/// `opened_at`/`closed_at` window) and `close_shift` (same query, inside
/// the transaction that's about to persist the closing values).
fn aggregate(conn: &Connection, shift: &Shift) -> Result<(i64, i64, i64, i64, i64), rusqlite::Error> {
    // Sales are attributed to a shift directly (`sales.shift_id`, set at
    // checkout time — see `sales::CreateSaleInput`), so this is an exact
    // match, not a time-window guess.
    let (cash, card, other, discount): (i64, i64, i64, i64) = conn.query_row(
        "SELECT
             COALESCE(SUM(CASE WHEN payment_method = 'cash'  THEN total_minor ELSE 0 END), 0),
             COALESCE(SUM(CASE WHEN payment_method = 'card'  THEN total_minor ELSE 0 END), 0),
             COALESCE(SUM(CASE WHEN payment_method = 'other' THEN total_minor ELSE 0 END), 0),
             COALESCE(SUM(discount_minor), 0)
           FROM sales
          WHERE shift_id = ?1",
        params![shift.id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;

    // Refunds have no shift_id of their own (a refund can be processed by
    // whoever's on shift, against a sale that may have happened on a
    // different one entirely) — scoped by falling inside this shift's open
    // window instead. An open shift reconciles everything up to "now"; a
    // closed one is bounded by its own closed_at, so re-viewing a closed
    // shift's summary later never picks up a refund recorded after it closed.
    let end_bound = shift.closed_at.clone().unwrap_or_else(|| "9999-12-31 23:59:59".to_string());
    let refunds: i64 = conn.query_row(
        "SELECT COALESCE(SUM(total_refund_amount_minor), 0)
           FROM refunds
          WHERE created_at >= ?1 AND created_at <= ?2",
        params![shift.opened_at, end_bound],
        |row| row.get(0),
    )?;

    Ok((cash, card, other, discount, refunds))
}

fn build_summary(
    conn: &Connection,
    shift: Shift,
    declared_override: Option<i64>,
) -> Result<ShiftSummary, ShiftError> {
    let (cash_sales_minor, card_sales_minor, other_sales_minor, discount_minor, refunds_minor) =
        aggregate(conn, &shift)?;
    let total_sales_minor = cash_sales_minor + card_sales_minor + other_sales_minor;
    let expected_cash_minor = shift.opening_balance_minor + cash_sales_minor - refunds_minor;

    let declared_cash_amount_minor = declared_override.or(shift.declared_cash_amount_minor);
    let difference_minor = declared_cash_amount_minor.map(|declared| declared - expected_cash_minor);

    Ok(ShiftSummary {
        opening_balance_minor: shift.opening_balance_minor,
        cash_sales_minor,
        card_sales_minor,
        other_sales_minor,
        credit_sales_minor: 0,
        total_sales_minor,
        discount_minor,
        refunds_minor,
        expected_cash_minor,
        declared_cash_amount_minor,
        difference_minor,
        shift,
    })
}

/// Full reconciliation breakdown for `shift_id`. `declared_override` lets
/// the caller preview "if I declared this much, what would Short/Over be"
/// before actually closing (the Billing screen's close-shift confirmation
/// step) without writing anything; omit it to read whatever is actually
/// stored (a still-open shift with nothing declared yet comes back with
/// `declared_cash_amount_minor: None`; a closed one shows what was recorded
/// at close time — used for reprinting a past shift's receipt).
pub fn get_shift_summary(
    conn: &Connection,
    shift_id: i64,
    declared_override: Option<i64>,
) -> Result<ShiftSummary, ShiftError> {
    let shift = load_shift(conn, shift_id)?.ok_or(ShiftError::NotFound(shift_id))?;
    build_summary(conn, shift, declared_override)
}

/// Closes `shift_id`: records `declared_cash_amount_minor` and `closed_at`,
/// then returns the final summary computed against that now-fixed window.
/// Refuses an already-closed shift rather than silently letting a second
/// close-out overwrite the first (and skew what "during this shift" means
/// for the sales/refunds aggregate).
pub fn close_shift(
    tx: &Transaction,
    shift_id: i64,
    declared_cash_amount_minor: i64,
) -> Result<ShiftSummary, ShiftError> {
    if declared_cash_amount_minor < 0 {
        return Err(ShiftError::InvalidDeclaredAmount);
    }
    let shift = load_shift(tx, shift_id)?.ok_or(ShiftError::NotFound(shift_id))?;
    if shift.closed_at.is_some() {
        return Err(ShiftError::AlreadyClosed(shift_id));
    }

    tx.execute(
        "UPDATE shifts SET closed_at = datetime('now', 'localtime'), declared_cash_amount_minor = ?1
          WHERE id = ?2",
        params![declared_cash_amount_minor, shift_id],
    )?;

    let closed_shift = load_shift(tx, shift_id)?.ok_or(ShiftError::NotFound(shift_id))?;
    build_summary(tx, closed_shift, None)
}

/// Most recent shifts (open or closed), newest first — the Shifts history
/// page's list, and a source of shift ids to reprint a past close-out from.
pub fn list_shifts(conn: &Connection, limit: i64) -> Result<Vec<Shift>, rusqlite::Error> {
    let limit = limit.clamp(1, 500);
    // Ordered by id, not opened_at: opened_at has only second resolution,
    // so two shifts opened in the same second would otherwise tie and sort
    // arbitrarily — id is monotonic and always reflects creation order.
    let mut stmt = conn.prepare(
        "SELECT s.id, s.cashier_id, u.name, s.opened_at, s.closed_at,
                s.opening_balance_minor, s.declared_cash_amount_minor, s.notes
           FROM shifts s
           LEFT JOIN users u ON u.id = s.cashier_id
          ORDER BY s.id DESC
          LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |row| {
        Ok(Shift {
            id: row.get(0)?,
            cashier_id: row.get(1)?,
            cashier_name: row.get(2)?,
            opened_at: row.get(3)?,
            closed_at: row.get(4)?,
            opening_balance_minor: row.get(5)?,
            declared_cash_amount_minor: row.get(6)?,
            notes: row.get(7)?,
        })
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::refunds::{create_refund, CreateRefundInput, RefundLineInput};
    use crate::db::sales::{create_sale, CartLine, CreateSaleInput};
    use crate::db::schema::test_conn;

    fn owner_id(conn: &Connection) -> i64 {
        conn.query_row("SELECT id FROM users LIMIT 1", [], |row| row.get(0)).unwrap()
    }

    fn item_id(conn: &Connection, name: &str) -> i64 {
        conn.query_row("SELECT id FROM items WHERE name = ?1", params![name], |row| row.get(0))
            .unwrap()
    }

    fn sell(conn: &mut Connection, shift_id: i64, item: &str, qty: i64, method: &str) -> i64 {
        let tx = conn.transaction().unwrap();
        let item_id = item_id(&tx, item);
        let sale = create_sale(
            &tx,
            CreateSaleInput {
                items: vec![CartLine { item_id, qty: qty as f64, notes: None }],
                discount_minor: 0,
                tax_minor: 0,
                payment_method: method.into(),
                cashier_id: None,
                table_id: None,
                shift_id: Some(shift_id),
            },
        )
        .unwrap();
        tx.commit().unwrap();
        sale.id
    }

    #[test]
    fn opening_a_second_shift_for_the_same_cashier_is_refused() {
        let conn = test_conn();
        let cashier = owner_id(&conn);
        let first = open_shift(&conn, cashier, 5000).unwrap();

        let err = open_shift(&conn, cashier, 1000).unwrap_err();
        assert!(matches!(err, ShiftError::AlreadyOpen { existing_shift_id } if existing_shift_id == first.id));
    }

    #[test]
    fn closes_with_exact_cash_and_reports_zero_difference() {
        let mut conn = test_conn();
        let cashier = owner_id(&conn);
        let shift = open_shift(&conn, cashier, 10_000).unwrap();

        sell(&mut conn, shift.id, "Cola 500ml", 2, "cash"); // 2 * 8000 = 16000

        let tx = conn.transaction().unwrap();
        let summary = close_shift(&tx, shift.id, 10_000 + 16_000).unwrap();
        tx.commit().unwrap();

        assert_eq!(summary.cash_sales_minor, 16_000);
        assert_eq!(summary.expected_cash_minor, 10_000 + 16_000);
        assert_eq!(summary.difference_minor, Some(0));
    }

    #[test]
    fn a_refund_during_the_shift_reduces_expected_cash() {
        let mut conn = test_conn();
        let cashier = owner_id(&conn);
        let shift = open_shift(&conn, cashier, 0).unwrap();

        let sale_id = sell(&mut conn, shift.id, "Cola 500ml", 2, "cash"); // 16000 in

        let sale_item_id: i64 = conn
            .query_row("SELECT id FROM sale_items WHERE sale_id = ?1", params![sale_id], |row| row.get(0))
            .unwrap();
        {
            let tx = conn.transaction().unwrap();
            create_refund(
                &tx,
                CreateRefundInput {
                    sale_id,
                    items: vec![RefundLineInput { sale_item_id, qty: 1.0, amount_minor: 8000 }],
                    reason: None,
                    refunded_by: None,
                },
            )
            .unwrap();
            tx.commit().unwrap();
        }

        let summary = get_shift_summary(&conn, shift.id, None).unwrap();
        assert_eq!(summary.cash_sales_minor, 16_000, "gross cash sales are untouched by the refund");
        assert_eq!(summary.refunds_minor, 8_000);
        assert_eq!(summary.expected_cash_minor, 16_000 - 8_000, "expected cash nets the refund out");
    }

    #[test]
    fn declaring_less_than_expected_reports_a_negative_short_difference() {
        let mut conn = test_conn();
        let cashier = owner_id(&conn);
        let shift = open_shift(&conn, cashier, 0).unwrap();
        sell(&mut conn, shift.id, "Cola 500ml", 1, "cash"); // 8000 expected

        let tx = conn.transaction().unwrap();
        let summary = close_shift(&tx, shift.id, 7_500).unwrap(); // cashier counted less
        tx.commit().unwrap();

        assert_eq!(summary.difference_minor, Some(7_500 - 8_000), "negative means Short");
        assert!(summary.difference_minor.unwrap() < 0);
    }

    #[test]
    fn declaring_more_than_expected_reports_a_positive_over_difference() {
        let mut conn = test_conn();
        let cashier = owner_id(&conn);
        let shift = open_shift(&conn, cashier, 0).unwrap();
        sell(&mut conn, shift.id, "Cola 500ml", 1, "cash");

        let tx = conn.transaction().unwrap();
        let summary = close_shift(&tx, shift.id, 8_500).unwrap();
        tx.commit().unwrap();

        assert_eq!(summary.difference_minor, Some(8_500 - 8_000));
        assert!(summary.difference_minor.unwrap() > 0);
    }

    #[test]
    fn a_preview_declared_amount_never_persists() {
        let mut conn = test_conn();
        let cashier = owner_id(&conn);
        let shift = open_shift(&conn, cashier, 0).unwrap();
        sell(&mut conn, shift.id, "Cola 500ml", 1, "cash");

        let preview = get_shift_summary(&conn, shift.id, Some(999_999)).unwrap();
        assert_eq!(preview.declared_cash_amount_minor, Some(999_999));

        // Still open, nothing written — a fresh read with no override shows
        // no declared amount at all.
        let after = get_shift_summary(&conn, shift.id, None).unwrap();
        assert_eq!(after.declared_cash_amount_minor, None);
        assert_eq!(after.shift.closed_at, None);
    }

    #[test]
    fn closing_an_already_closed_shift_is_refused() {
        let mut conn = test_conn();
        let cashier = owner_id(&conn);
        let shift = open_shift(&conn, cashier, 0).unwrap();

        {
            let tx = conn.transaction().unwrap();
            close_shift(&tx, shift.id, 0).unwrap();
            tx.commit().unwrap();
        }

        let tx = conn.transaction().unwrap();
        assert!(matches!(close_shift(&tx, shift.id, 0), Err(ShiftError::AlreadyClosed(_))));
    }

    #[test]
    fn card_and_other_sales_are_tracked_separately_and_never_affect_expected_cash() {
        let mut conn = test_conn();
        let cashier = owner_id(&conn);
        let shift = open_shift(&conn, cashier, 1_000).unwrap();

        sell(&mut conn, shift.id, "Cola 500ml", 1, "card"); // 8000
        sell(&mut conn, shift.id, "Cola 500ml", 1, "other"); // 8000

        let summary = get_shift_summary(&conn, shift.id, None).unwrap();
        assert_eq!(summary.cash_sales_minor, 0);
        assert_eq!(summary.card_sales_minor, 8_000);
        assert_eq!(summary.other_sales_minor, 8_000);
        assert_eq!(summary.credit_sales_minor, 0);
        assert_eq!(summary.total_sales_minor, 16_000);
        assert_eq!(summary.expected_cash_minor, 1_000, "card/other sales never touch the cash drawer");
    }

    #[test]
    fn opening_with_a_negative_balance_is_rejected() {
        let conn = test_conn();
        let cashier = owner_id(&conn);
        assert!(matches!(open_shift(&conn, cashier, -1), Err(ShiftError::InvalidOpeningBalance)));
    }

    #[test]
    fn list_shifts_returns_every_shift_newest_first() {
        let conn = test_conn();
        let cashier = owner_id(&conn);
        let first = open_shift(&conn, cashier, 0).unwrap();
        {
            let tx = conn.unchecked_transaction().unwrap();
            close_shift(&tx, first.id, 0).unwrap();
            tx.commit().unwrap();
        }
        let second = open_shift(&conn, cashier, 500).unwrap();

        let shifts = list_shifts(&conn, 10).unwrap();
        assert_eq!(shifts.len(), 2);
        assert_eq!(shifts[0].id, second.id, "most recently opened shift must come first");
        assert_eq!(shifts[1].id, first.id);
    }
}
