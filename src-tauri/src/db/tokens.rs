//! KOT (Kitchen Order Ticket) tokens — a kitchen/counter instruction printed
//! when an order is taken, separate from and printed well before the bill.
//!
//! Tracking "what's already been sent to the kitchen" is a per-*item*
//! running total against `table_order_id`, not a foreign key to a specific
//! cart line — see `schema.sql`'s `tokens`/`token_items` doc comment for
//! why: the in-progress cart (`table_orders.cart_json`) has no stable
//! per-line identity to reference, and a per-item aggregate is both simpler
//! and sufficient for what a kitchen ticket actually needs.
//!
//! The physical print always happens *before* a token is ever written to
//! the database (see `commands::tokens_print` on the Tauri-command side,
//! which owns that ordering) — so a printer failure can never leave items
//! silently marked as sent to the kitchen when they weren't. Nothing in
//! this module sends bytes to a printer itself; it only computes what
//! *should* print and records what already did.
//!
//! A Takeaway sale has no `table_orders` row at all — it's never parked the
//! way a dine-in table's order is — so it can't carry token history the
//! same way. `table_order_id` is therefore nullable (see `schema.sql`'s
//! comment on that column) and every "ad hoc" function below (`ad_hoc_
//! token_groups`, and `insert_token` called with `table_order_id: None`)
//! exists specifically to let a Takeaway order still print and record a
//! token, just without a delta to diff against: every ad hoc print sends
//! the full quantity handed to it, not "what's new since last time" — see
//! `commands::tokens_print_adhoc`'s doc comment for the trade-off that implies.

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

use super::tables::{self, TableError};

/// One line a token (or a "what would print" preview) shows.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenLine {
    pub item_id: i64,
    pub item_name: String,
    pub qty: f64,
    pub unit: Option<String>,
}

/// Everything still un-tokenized for one counter, on one table order.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingCounterGroup {
    pub counter_id: i64,
    pub counter_name: String,
    pub items: Vec<TokenLine>,
}

/// A previously (or just-now) printed token, with everything needed to
/// either display it in a "tokens for this table" list or reprint it —
/// `table_name` is read fresh from the order's *current* table, not
/// whatever table it was on at print time, so a reprint after `shift_table_
/// order` correctly shows where the customer is now.
///
/// `table_order_id`/`table_id`/`table_name` are `None` for an ad hoc
/// (Takeaway) token, which was never attached to any `table_orders` row —
/// see this module's doc comment.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenSummary {
    pub id: i64,
    pub token_number: i64,
    pub counter_id: i64,
    pub counter_name: String,
    pub table_order_id: Option<i64>,
    pub table_id: Option<i64>,
    pub table_name: Option<String>,
    pub printed_at: String,
    pub printed_by: Option<i64>,
    pub printed_by_name: Option<String>,
    pub status: String,
    pub items: Vec<TokenLine>,
}

/// One cart line to print an ad hoc (Takeaway) token for — the frontend's
/// live billing cart, sent straight across since there's no parked order to
/// read it back from.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdhocTokenLine {
    pub item_id: i64,
    pub qty: f64,
}

/// What happened when `commands::tokens_print` tried to print one
/// counter's token — internally tagged so the frontend can discriminate on
/// `status` directly. Every counter the cashier selected gets exactly one
/// of these back, even the ones that failed or had nothing to print, so
/// there's never a silent gap between "I selected 3 counters" and "1
/// result came back".
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum PrintOutcome {
    /// The physical print succeeded and the token is now recorded.
    Printed { token: TokenSummary },
    /// Nothing was pending for this counter (e.g. another cashier already
    /// tokenized it a moment earlier) — not an error, just nothing to do.
    NothingPending,
    /// The physical print failed — see this module's doc comment: nothing
    /// was recorded, so these items remain pending and the cashier can
    /// simply retry once the printer issue is fixed.
    Failed { error: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CounterPrintResult {
    pub counter_id: i64,
    pub counter_name: String,
    pub outcome: PrintOutcome,
}

#[derive(Debug)]
pub enum TokenError {
    OrderNotFound,
    TokenNotFound,
    Corrupt(String),
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenError::OrderNotFound => write!(f, "That table order is no longer open"),
            TokenError::TokenNotFound => write!(f, "Token not found"),
            TokenError::Corrupt(msg) => write!(f, "Could not read the parked order: {}", msg),
            TokenError::Sqlite(err) => write!(f, "database error: {}", err),
        }
    }
}

impl From<rusqlite::Error> for TokenError {
    fn from(err: rusqlite::Error) -> Self {
        TokenError::Sqlite(err)
    }
}

impl From<TableError> for TokenError {
    fn from(err: TableError) -> Self {
        match err {
            TableError::Corrupt(msg) => TokenError::Corrupt(msg),
            TableError::Sqlite(e) => TokenError::Sqlite(e),
            // `get_open_order_by_id` (the only `tables` call this module
            // makes) never actually produces any other variant — "not
            // found" comes back as `Ok(None)`, not an `Err` — but every
            // variant still needs *some* mapping to keep this exhaustive.
            other => TokenError::Corrupt(other.to_string()),
        }
    }
}

/// Sums `token_items.qty_on_token` per `item_id`, across every `printed`
/// token already recorded for this table order — the "how much of this
/// item has already reached the kitchen" running total that `get_pending_
/// token_items` subtracts the live cart quantity against.
fn already_tokenized(conn: &Connection, table_order_id: i64) -> Result<HashMap<i64, f64>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT ti.item_id, SUM(ti.qty_on_token)
           FROM token_items ti
           JOIN tokens t ON t.id = ti.token_id
          WHERE t.table_order_id = ?1 AND t.status = 'printed'
          GROUP BY ti.item_id",
    )?;
    let rows = stmt.query_map(params![table_order_id], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, f64>(1)?)))?;
    rows.collect()
}

/// A tiny tolerance for float qty comparisons — `sold_by_amount` items
/// carry fractional quantities, so "pending == 0" needs a margin rather
/// than exact equality to avoid a spurious near-zero line surviving.
const QTY_EPSILON: f64 = 1e-9;

/// The shared grouping step behind both `get_pending_token_items` (which
/// first subtracts what's already been tokenized) and `ad_hoc_token_groups`
/// (which has no history to subtract against, so every line prints as
/// given) — looks up each item's name/unit/counter, skips anything with a
/// non-positive quantity or no counter assigned (by design — see this
/// module's doc comment), and buckets what's left by counter.
fn group_by_counter(
    conn: &Connection,
    lines: impl Iterator<Item = (i64, f64)>,
) -> Result<Vec<PendingCounterGroup>, rusqlite::Error> {
    let mut groups: HashMap<i64, (String, Vec<TokenLine>)> = HashMap::new();

    for (item_id, qty) in lines {
        if qty <= QTY_EPSILON {
            continue;
        }

        let item: Option<(String, Option<String>, Option<i64>, Option<String>)> = conn
            .query_row(
                "SELECT i.name, i.unit, i.counter_id, c.name
                   FROM items i
                   LEFT JOIN counters c ON c.id = i.counter_id
                  WHERE i.id = ?1",
                params![item_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;

        // Item deleted since the cart was parked (only possible if it was
        // never sold, per `items::delete_item`'s soft-delete rule) — same
        // "skip it, don't fail the whole read" the frontend's own
        // `loadParkedCart` already does for this exact situation.
        let Some((item_name, unit, Some(counter_id), Some(counter_name))) = item else { continue };

        groups.entry(counter_id).or_insert_with(|| (counter_name, Vec::new())).1.push(TokenLine {
            item_id,
            item_name,
            qty,
            unit,
        });
    }

    let mut result: Vec<PendingCounterGroup> = groups
        .into_iter()
        .map(|(counter_id, (counter_name, mut items))| {
            items.sort_by(|a, b| a.item_name.cmp(&b.item_name));
            PendingCounterGroup { counter_id, counter_name, items }
        })
        .collect();
    result.sort_by(|a, b| a.counter_name.cmp(&b.counter_name));
    Ok(result)
}

/// Everything still un-tokenized for `table_order_id`, grouped by counter —
/// what the "Print Token" dialog shows before anything is printed. An item
/// with `counter_id = NULL` never appears here at all, by design (see this
/// module's doc comment) — it simply has nothing to contribute.
pub fn get_pending_token_items(
    conn: &Connection,
    table_order_id: i64,
) -> Result<Vec<PendingCounterGroup>, TokenError> {
    let ctx = tables::get_open_order_by_id(conn, table_order_id)?.ok_or(TokenError::OrderNotFound)?;
    let already = already_tokenized(conn, table_order_id)?;

    let lines = ctx.cart.items.iter().map(|line| {
        let cart_qty = line.qty as f64;
        let already_qty = already.get(&line.item_id).copied().unwrap_or(0.0);
        (line.item_id, cart_qty - already_qty)
    });

    Ok(group_by_counter(conn, lines)?)
}

/// The pending lines for one specific counter — `get_pending_token_items`
/// filtered down, for the print flow, which handles one counter at a time.
pub fn pending_items_for_counter(
    conn: &Connection,
    table_order_id: i64,
    counter_id: i64,
) -> Result<Vec<TokenLine>, TokenError> {
    Ok(get_pending_token_items(conn, table_order_id)?
        .into_iter()
        .find(|g| g.counter_id == counter_id)
        .map(|g| g.items)
        .unwrap_or_default())
}

/// The ad hoc (Takeaway) counterpart of `get_pending_token_items` — same
/// grouping, same no-counter-means-skip rule, but computed directly from
/// `lines` (the live billing cart, handed across as-is) instead of a
/// `table_orders.cart_json` snapshot, and with no "already tokenized"
/// history to subtract: there is no order row to have recorded it against.
/// Every call reports the full quantity given, every time.
pub fn ad_hoc_token_groups(
    conn: &Connection,
    lines: &[AdhocTokenLine],
) -> Result<Vec<PendingCounterGroup>, TokenError> {
    Ok(group_by_counter(conn, lines.iter().map(|l| (l.item_id, l.qty)))?)
}

/// `ad_hoc_token_groups` filtered down to one counter — the ad hoc print
/// flow's per-counter counterpart of `pending_items_for_counter`.
pub fn ad_hoc_pending_for_counter(
    conn: &Connection,
    lines: &[AdhocTokenLine],
    counter_id: i64,
) -> Result<Vec<TokenLine>, TokenError> {
    Ok(ad_hoc_token_groups(conn, lines)?
        .into_iter()
        .find(|g| g.counter_id == counter_id)
        .map(|g| g.items)
        .unwrap_or_default())
}

/// The next sequential token number for *today* — resets daily (see this
/// module's doc comment and `schema.sql`'s `tokens.token_number` comment):
/// unique per calendar day across every counter, not per-counter, so staff
/// can say "token 14" unambiguously. There is no existing daily-resetting
/// sequence anywhere else in this codebase (`sales.id` is a plain
/// forever-incrementing autoincrement) — this is the first one, deliberately
/// scoped to `tokens` only rather than generalized, since nothing else
/// needs it yet.
pub fn next_token_number_for_today(conn: &Connection) -> Result<i64, rusqlite::Error> {
    conn.query_row(
        "SELECT COALESCE(MAX(token_number), 0) + 1
           FROM tokens
          WHERE date(printed_at) = date('now', 'localtime')",
        [],
        |row| row.get(0),
    )
}

/// Records a token that has *already physically printed* — the caller
/// (`commands::tokens_print`/`commands::tokens_print_adhoc`) must only ever
/// call this after a successful `send_to_printer`, never before or
/// speculatively; that ordering, not anything in this function, is what
/// guarantees a failed print never leaves items marked as sent to the
/// kitchen. Takes a `&Transaction` since the token row and its item rows
/// must land together or not at all. `table_order_id: None` records an ad
/// hoc (Takeaway) token — see this module's doc comment.
pub fn insert_token(
    tx: &Transaction,
    table_order_id: Option<i64>,
    counter_id: i64,
    token_number: i64,
    printed_by: Option<i64>,
    items: &[TokenLine],
) -> Result<TokenSummary, TokenError> {
    tx.execute(
        "INSERT INTO tokens (token_number, table_order_id, counter_id, printed_by, status)
         VALUES (?1, ?2, ?3, ?4, 'printed')",
        params![token_number, table_order_id, counter_id, printed_by],
    )?;
    let token_id = tx.last_insert_rowid();

    for line in items {
        tx.execute(
            "INSERT INTO token_items (token_id, item_id, qty_on_token) VALUES (?1, ?2, ?3)",
            params![token_id, line.item_id, line.qty],
        )?;
    }

    load_token(tx, token_id)
}

fn load_token(conn: &Connection, token_id: i64) -> Result<TokenSummary, TokenError> {
    #[allow(clippy::type_complexity)]
    let row: Option<(i64, i64, String, Option<i64>, Option<i64>, Option<String>, String, Option<i64>, Option<String>, String)> = conn
        .query_row(
            "SELECT tk.token_number, tk.counter_id, c.name, tk.table_order_id, o.table_id, t.name,
                    tk.printed_at, tk.printed_by, u.name, tk.status
               FROM tokens tk
               JOIN counters c ON c.id = tk.counter_id
               LEFT JOIN table_orders o ON o.id = tk.table_order_id
               LEFT JOIN tables t ON t.id = o.table_id
               LEFT JOIN users u ON u.id = tk.printed_by
              WHERE tk.id = ?1",
            params![token_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                ))
            },
        )
        .optional()?;
    let Some((
        token_number,
        counter_id,
        counter_name,
        table_order_id,
        table_id,
        table_name,
        printed_at,
        printed_by,
        printed_by_name,
        status,
    )) = row
    else {
        return Err(TokenError::TokenNotFound);
    };

    let mut stmt = conn.prepare(
        "SELECT ti.item_id, i.name, ti.qty_on_token, i.unit
           FROM token_items ti
           JOIN items i ON i.id = ti.item_id
          WHERE ti.token_id = ?1
          ORDER BY ti.id",
    )?;
    let items = stmt
        .query_map(params![token_id], |row| {
            Ok(TokenLine { item_id: row.get(0)?, item_name: row.get(1)?, qty: row.get(2)?, unit: row.get(3)? })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(TokenSummary {
        id: token_id,
        token_number,
        counter_id,
        counter_name,
        table_order_id,
        table_id,
        table_name,
        printed_at,
        printed_by,
        printed_by_name,
        status,
        items,
    })
}

/// Re-fetches a token by id — used for reprint, and by `list_tokens_for_
/// order` below.
pub fn get_token(conn: &Connection, token_id: i64) -> Result<TokenSummary, TokenError> {
    load_token(conn, token_id)
}

/// Every token ever printed for this table order, newest first — the
/// "previously printed tokens" list with a reprint option per row.
pub fn list_tokens_for_order(conn: &Connection, table_order_id: i64) -> Result<Vec<TokenSummary>, TokenError> {
    let mut stmt =
        conn.prepare("SELECT id FROM tokens WHERE table_order_id = ?1 ORDER BY id DESC")?;
    let ids: Vec<i64> = stmt.query_map(params![table_order_id], |row| row.get(0))?.collect::<Result<_, _>>()?;
    ids.into_iter().map(|id| load_token(conn, id)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::counters;
    use crate::db::items::{self, ItemInput};
    use crate::db::schema::test_conn;
    use crate::db::tables::{attach_cart_to_table, start_table_order, ParkedCartLine};

    fn item_id(conn: &Connection, name: &str) -> i64 {
        conn.query_row("SELECT id FROM items WHERE name = ?1", params![name], |row| row.get(0)).unwrap()
    }

    fn table_id(conn: &Connection, name: &str) -> i64 {
        conn.query_row("SELECT id FROM tables WHERE name = ?1", params![name], |row| row.get(0)).unwrap()
    }

    fn order_id_for_table(conn: &Connection, table_id: i64) -> i64 {
        conn.query_row(
            "SELECT id FROM table_orders WHERE table_id = ?1 AND status = 'open'",
            params![table_id],
            |row| row.get(0),
        )
        .unwrap()
    }

    fn basic_item_input(name: &str) -> ItemInput {
        ItemInput {
            name: name.to_string(),
            barcode: None,
            short_code: None,
            description: None,
            price_minor: 10000,
            cost_minor: 8000,
            stock_qty: 100.0,
            category_id: None,
            low_stock_threshold: 2,
            image_path: None,
            sold_by_amount: false,
            unit: None,
            counter_id: None,
        }
    }

    /// Sets up: two counters, one item on each, one item (roti-analog) with
    /// no counter at all. Returns (channa_counter_id, drinks_counter_id,
    /// channa_item_id, cola_item_id [already seeded, no counter], roti_id).
    fn seed_counters_and_items(conn: &Connection) -> (i64, i64, i64, i64) {
        let channa_counter = counters::add_counter(conn, "Channa Counter").unwrap();
        let drinks_counter = counters::add_counter(conn, "Drinks Counter").unwrap();

        let mut channa_input = basic_item_input("Channa");
        channa_input.counter_id = Some(channa_counter.id);
        let channa = items::add_item(conn, channa_input).unwrap();

        let mut roti_input = basic_item_input("Roti"); // deliberately no counter
        roti_input.counter_id = None;
        let roti = items::add_item(conn, roti_input).unwrap();

        (channa_counter.id, drinks_counter.id, channa.id, roti.id)
    }

    #[test]
    fn pending_items_excludes_items_with_no_counter_and_groups_by_counter() {
        let conn = test_conn();
        let (channa_counter, _drinks_counter, channa, roti) = seed_counters_and_items(&conn);
        let cola = item_id(&conn, "Cola 500ml"); // seeded, no counter assigned

        let t1 = table_id(&conn, "Table 1");
        start_table_order(&conn, t1).unwrap();
        attach_cart_to_table(
            &conn,
            t1,
            &[
                ParkedCartLine { item_id: channa, qty: 2.0 },
                ParkedCartLine { item_id: roti, qty: 5.0 },
                ParkedCartLine { item_id: cola, qty: 1.0 },
            ],
            0,
        )
        .unwrap();
        let order_id = order_id_for_table(&conn, t1);

        let groups = get_pending_token_items(&conn, order_id).unwrap();
        assert_eq!(groups.len(), 1, "only channa's counter has anything pending — roti and cola have no counter");
        assert_eq!(groups[0].counter_id, channa_counter);
        assert_eq!(groups[0].items.len(), 1);
        assert_eq!(groups[0].items[0].item_id, channa);
        assert_eq!(groups[0].items[0].qty, 2.0);
    }

    #[test]
    fn printing_a_token_removes_those_items_from_pending_and_a_repeat_order_shows_only_the_delta() {
        let mut conn = test_conn();
        let (channa_counter, _drinks, channa, _roti) = seed_counters_and_items(&conn);
        let t1 = table_id(&conn, "Table 1");
        start_table_order(&conn, t1).unwrap();
        attach_cart_to_table(&conn, t1, &[ParkedCartLine { item_id: channa, qty: 3.0 }], 0).unwrap();
        let order_id = order_id_for_table(&conn, t1);

        // "Print" the first token (bypassing the printer — this module never
        // touches one; `commands::tokens_print` is what actually gates
        // insertion on a real print succeeding).
        let pending = pending_items_for_counter(&conn, order_id, channa_counter).unwrap();
        assert_eq!(pending[0].qty, 3.0);
        let number = next_token_number_for_today(&conn).unwrap();
        {
            let tx = conn.transaction().unwrap();
            insert_token(&tx, Some(order_id), channa_counter, number, None, &pending).unwrap();
            tx.commit().unwrap();
        }

        // Nothing left pending right after.
        assert!(get_pending_token_items(&conn, order_id).unwrap().is_empty());

        // Customer orders 2 more channa — cart qty goes 3 -> 5.
        attach_cart_to_table(&conn, t1, &[ParkedCartLine { item_id: channa, qty: 5.0 }], 0).unwrap();
        let repeat_pending = pending_items_for_counter(&conn, order_id, channa_counter).unwrap();
        assert_eq!(repeat_pending.len(), 1);
        assert_eq!(repeat_pending[0].qty, 2.0, "only the newly-added 2 units, not all 5");
    }

    #[test]
    fn token_numbers_are_sequential_and_reset_relative_to_today_only() {
        let mut conn = test_conn();
        let (channa_counter, _drinks, channa, _roti) = seed_counters_and_items(&conn);
        let t1 = table_id(&conn, "Table 1");
        start_table_order(&conn, t1).unwrap();
        attach_cart_to_table(&conn, t1, &[ParkedCartLine { item_id: channa, qty: 10.0 }], 0).unwrap();
        let order_id = order_id_for_table(&conn, t1);

        let first_number = next_token_number_for_today(&conn).unwrap();
        {
            let items = pending_items_for_counter(&conn, order_id, channa_counter).unwrap();
            let tx = conn.transaction().unwrap();
            insert_token(&tx, Some(order_id), channa_counter, first_number, None, &items[..1].to_vec()).unwrap();
            tx.commit().unwrap();
        }
        let second_number = next_token_number_for_today(&conn).unwrap();
        assert_eq!(second_number, first_number + 1);
    }

    #[test]
    fn get_pending_token_items_rejects_a_closed_or_unknown_order() {
        let conn = test_conn();
        assert!(matches!(get_pending_token_items(&conn, 999_999), Err(TokenError::OrderNotFound)));
    }

    #[test]
    fn list_tokens_for_order_and_reprint_reflect_the_orders_current_table_after_a_shift() {
        let mut conn = test_conn();
        let (channa_counter, _drinks, channa, _roti) = seed_counters_and_items(&conn);
        let t1 = table_id(&conn, "Table 1");
        let t4 = table_id(&conn, "Table 4"); // seeded reserved, must free it first
        crate::db::tables::update_table_status(&conn, t4, "free").unwrap();

        start_table_order(&conn, t1).unwrap();
        attach_cart_to_table(&conn, t1, &[ParkedCartLine { item_id: channa, qty: 1.0 }], 0).unwrap();
        let order_id = order_id_for_table(&conn, t1);

        let items = pending_items_for_counter(&conn, order_id, channa_counter).unwrap();
        let number = next_token_number_for_today(&conn).unwrap();
        let token = {
            let tx = conn.transaction().unwrap();
            let t = insert_token(&tx, Some(order_id), channa_counter, number, None, &items).unwrap();
            tx.commit().unwrap();
            t
        };
        assert_eq!(token.table_name.as_deref(), Some("Table 1"));

        // Shift the order to Table 4 — the token (already printed) must now
        // report the new table, since it belongs to the order, not the
        // physical table it started on.
        let tx = conn.transaction().unwrap();
        crate::db::tables::shift_table_order(&tx, t1, t4).unwrap();
        tx.commit().unwrap();

        let reloaded = get_token(&conn, token.id).unwrap();
        assert_eq!(reloaded.table_name.as_deref(), Some("Table 4"), "reprint must show the order's current table, not the old one");

        let listed = list_tokens_for_order(&conn, order_id).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, token.id);
    }

    #[test]
    fn ad_hoc_token_groups_excludes_items_with_no_counter_same_as_the_table_flow() {
        let conn = test_conn();
        let (channa_counter, _drinks, channa, roti) = seed_counters_and_items(&conn);
        let cola = item_id(&conn, "Cola 500ml"); // seeded, no counter assigned

        let lines = vec![
            AdhocTokenLine { item_id: channa, qty: 2.0 },
            AdhocTokenLine { item_id: roti, qty: 5.0 },
            AdhocTokenLine { item_id: cola, qty: 1.0 },
        ];
        let groups = ad_hoc_token_groups(&conn, &lines).unwrap();
        assert_eq!(groups.len(), 1, "only channa's counter has anything to token — roti and cola have no counter");
        assert_eq!(groups[0].counter_id, channa_counter);
        assert_eq!(groups[0].items[0].qty, 2.0);
    }

    #[test]
    fn ad_hoc_prints_have_no_delta_tracking_and_record_with_a_null_table_order() {
        let mut conn = test_conn();
        let (channa_counter, _drinks, channa, _roti) = seed_counters_and_items(&conn);
        let lines = vec![AdhocTokenLine { item_id: channa, qty: 3.0 }];

        let pending = ad_hoc_pending_for_counter(&conn, &lines, channa_counter).unwrap();
        assert_eq!(pending[0].qty, 3.0);
        let number = next_token_number_for_today(&conn).unwrap();
        let token = {
            let tx = conn.transaction().unwrap();
            let t = insert_token(&tx, None, channa_counter, number, None, &pending).unwrap();
            tx.commit().unwrap();
            t
        };

        // No history to diff against — an ad hoc token has no table order,
        // so it's simply never a candidate for delta tracking at all.
        assert_eq!(token.table_order_id, None);
        assert_eq!(token.table_id, None);
        assert_eq!(token.table_name, None);

        // Unlike the table-order flow, asking again reports the exact same
        // full quantity — there is nothing to have subtracted it from.
        let repeat = ad_hoc_pending_for_counter(&conn, &lines, channa_counter).unwrap();
        assert_eq!(repeat[0].qty, 3.0, "an ad hoc token has no order to record a delta against");

        // Re-fetching by id must still round-trip correctly with a NULL
        // table order (the LEFT JOINs in `load_token` must not choke on it).
        let reloaded = get_token(&conn, token.id).unwrap();
        assert_eq!(reloaded.id, token.id);
        assert_eq!(reloaded.items[0].qty, 3.0);
    }
}
