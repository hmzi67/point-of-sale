//! Cashier shift open/close and the cash-drawer reconciliation summary.
//!
//! Sales are split by tender (`sale_payment_lines`, so a split sale's cash
//! part counts as cash and its card part as card). `credit_sales_minor` is
//! the pay-later part — sold, but no money in the drawer yet; installments
//! collected in cash during the shift (`customer_payments.shift_id`) are.

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum ShiftError {
    #[error("You already have an open shift (#{existing_shift_id})")]
    AlreadyOpen { existing_shift_id: i64 },
    #[error("Shift {0} not found")]
    NotFound(i64),
    #[error("Shift {0} is already closed")]
    AlreadyClosed(i64),
    #[error("Opening balance cannot be negative")]
    InvalidOpeningBalance,
    #[error("Declared cash amount cannot be negative")]
    InvalidDeclaredAmount,
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
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
    /// The counter whose drawer it is, when the business has several.
    pub till_name: Option<String>,
}

/// The SELECT list + join every `Shift` read uses, in `shift_from_row`'s
/// column order. Callers append their own WHERE/ORDER BY.
const SHIFT_SELECT: &str = "SELECT s.id, s.cashier_id, u.name, s.opened_at, s.closed_at,
        s.opening_balance_minor, s.declared_cash_amount_minor, s.notes, st.name
   FROM shifts s
   LEFT JOIN users u ON u.id = s.cashier_id
   LEFT JOIN sync_terminals st ON st.install_id = substr(s.gid, 1, 16) AND (SELECT COUNT(*) FROM sync_terminals) > 1";

fn shift_from_row(row: &rusqlite::Row) -> rusqlite::Result<Shift> {
    Ok(Shift {
        id: row.get(0)?,
        cashier_id: row.get(1)?,
        cashier_name: row.get(2)?,
        opened_at: row.get(3)?,
        closed_at: row.get(4)?,
        opening_balance_minor: row.get(5)?,
        declared_cash_amount_minor: row.get(6)?,
        notes: row.get(7)?,
        till_name: row.get(8)?,
    })
}

fn load_shift(conn: &Connection, id: i64) -> Result<Option<Shift>, rusqlite::Error> {
    conn.query_row(&format!("{SHIFT_SELECT} WHERE s.id = ?1"), params![id], shift_from_row).optional()
}

/// A shift is one cash drawer on one counter. On a counter connected to its
/// business the other counters' shifts are here too (for reports), so
/// "the open shift" only ever means one opened on this counter — its gid
/// carries this installation's prefix.
pub const OPENED_HERE: &str = "substr(gid, 1, 16) = (SELECT install_id FROM sync_state WHERE id = 1)";

/// The cashier's currently-open shift, if any — used both to block opening a
/// second one and to let the billing screen know whether to show "Open
/// Shift" or "Close Shift".
pub fn get_open_shift_for_cashier(conn: &Connection, cashier_id: i64) -> Result<Option<Shift>, ShiftError> {
    let id: Option<i64> = conn
        .query_row(
            &format!("SELECT id FROM shifts WHERE cashier_id = ?1 AND closed_at IS NULL AND {OPENED_HERE}"),
            params![cashier_id],
            |row| row.get(0),
        )
        .optional()?;
    match id {
        Some(id) => Ok(load_shift(conn, id)?),
        None => Ok(None),
    }
}

/// Returns the open shift id for `cashier_id` if present, falling back to any
/// currently open shift in the till (e.g. storeowner ringing up while cashier shift is active).
pub fn get_effective_open_shift_id(conn: &Connection, cashier_id: Option<i64>) -> Result<Option<i64>, ShiftError> {
    if let Some(cid) = cashier_id {
        if let Some(shift) = get_open_shift_for_cashier(conn, cid)? {
            return Ok(Some(shift.id));
        }
    }
    let any_id: Option<i64> = conn
        .query_row(
            &format!("SELECT id FROM shifts WHERE closed_at IS NULL AND {OPENED_HERE} ORDER BY id DESC LIMIT 1"),
            [],
            |row| row.get(0),
        )
        .optional()?;
    Ok(any_id)
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
    /// Pay-later part of this shift's sales — see the module doc comment.
    pub credit_sales_minor: i64,
    #[serde(default)]
    pub platform_sales_minor: i64,
    pub total_sales_minor: i64,
    /// Sum of `sale_items` discount applied across this shift's sales —
    /// "today's discount total" on the close-out receipt.
    pub discount_minor: i64,
    /// Refunds recorded (by `created_at`, not by which shift the *original*
    /// sale happened in) during this shift's open window.
    pub refunds_minor: i64,
    /// Pay-later installments received in cash during this shift.
    #[serde(default)]
    pub collected_cash_minor: i64,
    /// `opening_balance + cash_sales + collected_cash - refunds paid out` —
    /// refunds are assumed to come back out of the cash drawer regardless of
    /// how the original sale was paid, same as a real till: a card sale
    /// refunded in cash still empties the drawer by that amount. The part of
    /// a refund that only cancels unpaid credit pays nothing out (see
    /// `customers::refund_cash_out`). Third-party platform sales are excluded.
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
struct Aggregate {
    cash: i64,
    card: i64,
    other: i64,
    platform: i64,
    credit: i64,
    discount: i64,
    refunds: i64,
    refunds_paid_out: i64,
    collected_cash: i64,
}

fn aggregate(conn: &Connection, shift: &Shift) -> Result<Aggregate, rusqlite::Error> {
    // Sales are attributed to a shift directly (`sales.shift_id`, set at
    // checkout time — see `sales::CreateSaleInput`), so this is an exact
    // match, not a time-window guess.
    let (cash, card, other, platform, credit): (i64, i64, i64, i64, i64) = conn.query_row(
        "SELECT
             COALESCE(SUM(CASE WHEN p.method = 'cash'     THEN p.amount_minor ELSE 0 END), 0),
             COALESCE(SUM(CASE WHEN p.method = 'card'     THEN p.amount_minor ELSE 0 END), 0),
             COALESCE(SUM(CASE WHEN p.method = 'other'    THEN p.amount_minor ELSE 0 END), 0),
             COALESCE(SUM(CASE WHEN p.method = 'platform' THEN p.amount_minor ELSE 0 END), 0),
             COALESCE(SUM(CASE WHEN p.method = 'credit'   THEN p.amount_minor ELSE 0 END), 0)
           FROM sale_payment_lines p
           JOIN sales s ON s.id = p.sale_id
          WHERE s.shift_id = ?1",
        params![shift.id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
    )?;
    let discount: i64 = conn.query_row(
        "SELECT COALESCE(SUM(discount_minor), 0) FROM sales WHERE shift_id = ?1",
        params![shift.id],
        |row| row.get(0),
    )?;
    let collected_cash: i64 = conn.query_row(
        "SELECT COALESCE(SUM(amount_minor), 0) FROM customer_payments WHERE shift_id = ?1 AND method = 'cash'",
        params![shift.id],
        |row| row.get(0),
    )?;

    // Refunds have no shift_id of their own (a refund can be processed by
    // whoever's on shift, against a sale that may have happened on a
    // different one entirely) — scoped by falling inside this shift's open
    // window instead. An open shift reconciles everything up to "now"; a
    // closed one is bounded by its own closed_at, so re-viewing a closed
    // shift's summary later never picks up a refund recorded after it closed.
    let end_bound = shift.closed_at.clone().unwrap_or_else(|| crate::db::dates::OPEN_END_TIMESTAMP.to_string());
    // On a counter connected to its business, other counters' refunds are
    // here too: only those recorded on this shift's counter (the same gid
    // prefix) came out of this drawer.
    let mut stmt = conn.prepare(
        "SELECT id, total_refund_amount_minor
           FROM refunds
          WHERE created_at >= ?1 AND created_at <= ?2
            AND COALESCE(substr(gid, 1, 16), '') = COALESCE((SELECT substr(gid, 1, 16) FROM shifts WHERE id = ?3), '')",
    )?;
    let refund_rows = stmt
        .query_map(params![shift.opened_at, end_bound, shift.id], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let refunds = refund_rows.iter().map(|(_, amount)| amount).sum();
    let mut refunds_paid_out = 0;
    for (id, _) in &refund_rows {
        refunds_paid_out += crate::db::customers::refund_cash_out(conn, *id)?;
    }

    Ok(Aggregate { cash, card, other, platform, credit, discount, refunds, refunds_paid_out, collected_cash })
}

fn build_summary(
    conn: &Connection,
    shift: Shift,
    declared_override: Option<i64>,
) -> Result<ShiftSummary, ShiftError> {
    let totals = aggregate(conn, &shift)?;
    let (cash_sales_minor, card_sales_minor, other_sales_minor, platform_sales_minor, discount_minor, refunds_minor) =
        (totals.cash, totals.card, totals.other, totals.platform, totals.discount, totals.refunds);
    let total_sales_minor =
        cash_sales_minor + card_sales_minor + other_sales_minor + platform_sales_minor + totals.credit;
    let expected_cash_minor =
        shift.opening_balance_minor + cash_sales_minor + totals.collected_cash - totals.refunds_paid_out;

    let declared_cash_amount_minor = declared_override.or(shift.declared_cash_amount_minor);
    let difference_minor = declared_cash_amount_minor.map(|declared| declared - expected_cash_minor);

    Ok(ShiftSummary {
        opening_balance_minor: shift.opening_balance_minor,
        cash_sales_minor,
        card_sales_minor,
        other_sales_minor,
        credit_sales_minor: totals.credit,
        collected_cash_minor: totals.collected_cash,
        platform_sales_minor,
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
    let mut stmt = conn.prepare(&format!("{SHIFT_SELECT} ORDER BY s.id DESC LIMIT ?1"))?;
    let rows = stmt.query_map(params![limit], shift_from_row)?;
    rows.collect()
}

/// One day across every cash drawer of the business: each counter's
/// shifts and the totals, for closing the day. (A connected counter has
/// every counter's shifts and sales, so this is business-wide.)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawerDay {
    pub day: String,
    pub drawers: Vec<ShiftSummary>,
    pub open_drawers: i64,
    pub opening_balance_minor: i64,
    pub cash_sales_minor: i64,
    pub card_sales_minor: i64,
    pub credit_sales_minor: i64,
    pub other_sales_minor: i64,
    pub total_sales_minor: i64,
    pub refunds_minor: i64,
    pub expected_cash_minor: i64,
    /// Counted cash of the drawers already closed, and how far that is
    /// from what they should hold.
    pub declared_cash_minor: i64,
    pub difference_minor: i64,
}

/// Every shift opened on `day` (YYYY-MM-DD), on any counter.
pub fn day_summary(conn: &Connection, day: &str) -> Result<DrawerDay, ShiftError> {
    let mut stmt = conn.prepare(&format!("{SHIFT_SELECT} WHERE date(s.opened_at) = ?1 ORDER BY st.seq, s.id"))?;
    let shifts = stmt.query_map(params![day], shift_from_row)?.collect::<Result<Vec<_>, _>>()?;
    let mut totals = DrawerDay {
        day: day.to_string(),
        drawers: Vec::new(),
        open_drawers: 0,
        opening_balance_minor: 0,
        cash_sales_minor: 0,
        card_sales_minor: 0,
        credit_sales_minor: 0,
        other_sales_minor: 0,
        total_sales_minor: 0,
        refunds_minor: 0,
        expected_cash_minor: 0,
        declared_cash_minor: 0,
        difference_minor: 0,
    };
    for shift in shifts {
        let summary = build_summary(conn, shift, None)?;
        totals.opening_balance_minor += summary.opening_balance_minor;
        totals.cash_sales_minor += summary.cash_sales_minor;
        totals.card_sales_minor += summary.card_sales_minor;
        totals.credit_sales_minor += summary.credit_sales_minor;
        totals.other_sales_minor += summary.other_sales_minor + summary.platform_sales_minor;
        totals.total_sales_minor += summary.total_sales_minor;
        totals.refunds_minor += summary.refunds_minor;
        totals.expected_cash_minor += summary.expected_cash_minor;
        match (summary.shift.closed_at.is_some(), summary.declared_cash_amount_minor, summary.difference_minor) {
            (true, Some(declared), Some(difference)) => {
                totals.declared_cash_minor += declared;
                totals.difference_minor += difference;
            }
            _ => totals.open_drawers += 1,
        }
        totals.drawers.push(summary);
    }
    Ok(totals)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::{owner_id, ring_up};
    use crate::db::refunds::{create_refund, CreateRefundInput, RefundLineInput};
    use crate::db::schema::test_conn;



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

        ring_up(&mut conn, "Cola 500ml", 2.0, "cash", Some(shift.id)); // 2 * 8000 = 16000

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

        let sale_id = ring_up(&mut conn, "Cola 500ml", 2.0, "cash", Some(shift.id)); // 16000 in

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
    fn a_refund_rung_up_on_another_counter_is_not_taken_from_this_drawer() {
        let mut conn = test_conn();
        let cashier = owner_id(&conn);
        let shift = open_shift(&conn, cashier, 0).unwrap();
        let sale_id = ring_up(&mut conn, "Cola 500ml", 2.0, "cash", Some(shift.id));
        let sale_item_id: i64 =
            conn.query_row("SELECT id FROM sale_items WHERE sale_id = ?1", params![sale_id], |row| row.get(0)).unwrap();
        let tx = conn.transaction().unwrap();
        let refund = create_refund(
            &tx,
            CreateRefundInput { sale_id, items: vec![RefundLineInput { sale_item_id, qty: 1.0, amount_minor: 8000 }], reason: None, refunded_by: None },
        )
        .unwrap();
        tx.commit().unwrap();
        // As if it had arrived from another counter of the business.
        conn.execute("UPDATE refunds SET gid = 'ffff00000000ffff:3' WHERE id = ?1", params![refund.id]).unwrap();

        let summary = get_shift_summary(&conn, shift.id, None).unwrap();
        assert_eq!(summary.refunds_minor, 0, "that cash left the other counter's drawer");
        assert_eq!(summary.expected_cash_minor, 16_000);
    }

    #[test]
    fn the_day_summary_adds_up_every_drawer_and_counts_the_open_ones() {
        let mut conn = test_conn();
        let owner = owner_id(&conn);
        let cashier = crate::db::users::create(&conn, "Sana", "4321", crate::db::users::Role::Cashier).unwrap().id;
        let first = open_shift(&conn, owner, 1_000).unwrap();
        let second = open_shift(&conn, cashier, 500).unwrap();
        ring_up(&mut conn, "Cola 500ml", 1.0, "cash", Some(first.id)); // 8000
        ring_up(&mut conn, "Cola 500ml", 2.0, "card", Some(second.id)); // 16000
        let tx = conn.transaction().unwrap();
        close_shift(&tx, first.id, 8_900).unwrap(); // expected 9000: 100 short
        tx.commit().unwrap();

        let day: String = conn.query_row("SELECT date(opened_at) FROM shifts WHERE id = ?1", params![first.id], |r| r.get(0)).unwrap();
        let summary = day_summary(&conn, &day).unwrap();
        assert_eq!(summary.drawers.len(), 2);
        assert_eq!(summary.open_drawers, 1);
        assert_eq!((summary.opening_balance_minor, summary.cash_sales_minor, summary.card_sales_minor), (1_500, 8_000, 16_000));
        assert_eq!(summary.total_sales_minor, 24_000);
        assert_eq!(summary.expected_cash_minor, 9_000 + 500);
        assert_eq!((summary.declared_cash_minor, summary.difference_minor), (8_900, -100), "only closed drawers are counted");
        assert!(day_summary(&conn, "1999-01-01").unwrap().drawers.is_empty());
    }

    #[test]
    fn declaring_less_than_expected_reports_a_negative_short_difference() {
        let mut conn = test_conn();
        let cashier = owner_id(&conn);
        let shift = open_shift(&conn, cashier, 0).unwrap();
        ring_up(&mut conn, "Cola 500ml", 1.0, "cash", Some(shift.id)); // 8000 expected

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
        ring_up(&mut conn, "Cola 500ml", 1.0, "cash", Some(shift.id));

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
        ring_up(&mut conn, "Cola 500ml", 1.0, "cash", Some(shift.id));

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

        ring_up(&mut conn, "Cola 500ml", 1.0, "card", Some(shift.id)); // 8000
        ring_up(&mut conn, "Cola 500ml", 1.0, "other", Some(shift.id)); // 8000

        let summary = get_shift_summary(&conn, shift.id, None).unwrap();
        assert_eq!(summary.cash_sales_minor, 0);
        assert_eq!(summary.card_sales_minor, 8_000);
        assert_eq!(summary.other_sales_minor, 8_000);
        assert_eq!(summary.credit_sales_minor, 0);
        assert_eq!(summary.total_sales_minor, 16_000);
        assert_eq!(summary.expected_cash_minor, 1_000, "card/other sales never touch the cash drawer");
    }

    #[test]
    fn platform_sales_are_tracked_separately_and_never_affect_expected_cash() {
        let mut conn = test_conn();
        let cashier = owner_id(&conn);
        let shift = open_shift(&conn, cashier, 5_000).unwrap();

        ring_up(&mut conn, "Cola 500ml", 1.0, "platform", Some(shift.id)); // 8000

        let summary = get_shift_summary(&conn, shift.id, None).unwrap();
        assert_eq!(summary.cash_sales_minor, 0);
        assert_eq!(summary.platform_sales_minor, 8_000);
        assert_eq!(summary.total_sales_minor, 8_000);
        assert_eq!(summary.expected_cash_minor, 5_000, "platform sales never touch the cash drawer");
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

    #[test]
    fn split_tenders_count_by_part_and_cash_installments_land_in_the_drawer() {
        use crate::db::customers::{add_customer, record_payment, CustomerInput, PaymentInput};
        use crate::db::sales::{create_sale, CreateSaleInput, PaymentPart};
        use crate::db::test_support::{in_tx, item_id, one_line_sale};

        let mut conn = test_conn();
        let cashier = owner_id(&conn);
        let shift = open_shift(&conn, cashier, 1000).unwrap();
        let cola = item_id(&conn, "Cola 500ml");
        let customer = add_customer(
            &conn,
            CustomerInput { name: "Ayesha".into(), phone: None, email: None, address: None, notes: None },
        )
        .unwrap();

        // 8000 total: 3000 cash, 2000 card, 3000 on credit.
        in_tx(&mut conn, |tx| {
            create_sale(
                tx,
                CreateSaleInput {
                    shift_id: Some(shift.id),
                    customer_id: Some(customer.id),
                    due_date: Some("2099-01-01".into()),
                    payments: vec![
                        PaymentPart { method: "cash".into(), amount_minor: 3000 },
                        PaymentPart { method: "card".into(), amount_minor: 2000 },
                        PaymentPart { method: "credit".into(), amount_minor: 3000 },
                    ],
                    ..one_line_sale(cola, 1.0)
                },
            )
        })
        .unwrap();
        in_tx(&mut conn, |tx| {
            record_payment(
                tx,
                PaymentInput {
                    customer_id: customer.id,
                    sale_id: None,
                    amount_minor: 500,
                    method: "cash",
                    note: None,
                    received_by: Some(cashier),
                    shift_id: Some(shift.id),
                },
            )
        })
        .unwrap();

        let summary = get_shift_summary(&conn, shift.id, None).unwrap();
        assert_eq!(summary.cash_sales_minor, 3000);
        assert_eq!(summary.card_sales_minor, 2000);
        assert_eq!(summary.credit_sales_minor, 3000);
        assert_eq!(summary.total_sales_minor, 8000);
        assert_eq!(summary.collected_cash_minor, 500);
        assert_eq!(summary.expected_cash_minor, 1000 + 3000 + 500);
    }
}
