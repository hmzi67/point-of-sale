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

/// One tender of a sale. `credit` is the pay-later part, owed by the sale's
/// customer — see schema.sql's "Customers and pay-later" section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentPart {
    pub method: String,
    pub amount_minor: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSaleInput {
    pub items: Vec<CartLine>,
    pub discount_minor: i64,
    pub tax_minor: i64,
    pub payment_method: String,
    /// Always overwritten by `billing_create_sale` with the signed-in
    /// caller — never trusted from the client (see `session.rs`).
    #[serde(default)]
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
    /// Like `cashier_id`, `billing_create_sale` derives this from the
    /// session (the caller's own open shift) rather than the client.
    #[serde(default)]
    pub shift_id: Option<i64>,
    #[serde(default)]
    pub tendered_minor: Option<i64>,
    #[serde(default)]
    pub change_minor: Option<i64>,
    /// The tender breakdown. Empty means one part: all of the total under
    /// `payment_method` (every caller from before split payments).
    #[serde(default)]
    pub payments: Vec<PaymentPart>,
    #[serde(default)]
    pub customer_id: Option<i64>,
    /// Computed client-side from `AppConfig::service_charge_percent`, and
    /// trusted the same way `tax_minor` is (see CLAUDE.md).
    #[serde(default)]
    pub service_charge_minor: i64,
    /// Required when any part is `credit`.
    #[serde(default)]
    pub due_date: Option<String>,
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
    pub tendered_minor: Option<i64>,
    pub change_minor: Option<i64>,
    pub order_id: Option<i64>,
    pub order_no: Option<i64>,
    pub created_at: String,
    pub items: Vec<SaleLine>,
    #[serde(default)]
    pub order_type: Option<String>,
    #[serde(default)]
    pub customer_name: Option<String>,
    #[serde(default)]
    pub customer_phone: Option<String>,
    #[serde(default)]
    pub delivery_address: Option<String>,
    #[serde(default)]
    pub delivery_rider_name: Option<String>,
    #[serde(default)]
    pub delivery_rider_phone: Option<String>,
    #[serde(default)]
    pub delivery_platform_name: Option<String>,
    #[serde(default)]
    pub platform_order_ref: Option<String>,
    #[serde(default)]
    pub delivery_charge_minor: Option<i64>,
    pub service_charge_minor: i64,
    /// Tender breakdown; one line for a single-method sale.
    pub payments: Vec<PaymentPart>,
    pub customer_id: Option<i64>,
    pub customer_email: Option<String>,
    pub due_date: Option<String>,
    /// What the receipt prints as the sale number: `C2-12` once the business
    /// has several counters (the same on every counter), else the id.
    pub receipt_no: String,
    /// The counter that rang it up, when the business has several.
    pub till_name: Option<String>,
}

use crate::db::payment_methods::{SALE as PAYMENT_METHODS, TENDER as TENDER_METHODS};

#[derive(Debug, thiserror::Error)]
pub enum SaleError {
    #[error("Cart is empty")]
    EmptyCart,
    #[error("Quantity must be at least 1")]
    InvalidQuantity,
    #[error("Item {0} no longer exists")]
    ItemNotFound(i64),
    #[error("{0} is no longer sold and cannot be added to a sale")]
    ItemInactive(String),
    #[error("Only {available} left of {name}, but {requested} requested")]
    InsufficientStock { name: String, available: f64, requested: f64 },
    #[error("Discount cannot exceed the subtotal")]
    InvalidDiscount,
    #[error("Unknown payment method: {0}")]
    InvalidPaymentMethod(String),
    #[error("{0}")]
    InvalidPayments(String),
    #[error("Sale not found")]
    NotFound,
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
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
        if !line.qty.is_finite() || line.qty <= 0.0 {
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

    let mut priced: Vec<PricedLine> = Vec::with_capacity(merged.len());
    for line in &merged {
        let row: Option<(String, i64, i64)> = tx
            .query_row(
                "SELECT name, price_minor, is_active FROM items WHERE id = ?1",
                params![line.item_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let (name, price_minor, is_active) = row.ok_or(SaleError::ItemNotFound(line.item_id))?;
        if is_active == 0 {
            return Err(SaleError::ItemInactive(name));
        }
        priced.push(PricedLine { item_id: line.item_id, qty: line.qty, price_minor, notes: line.notes.clone() });
    }

    let sale = insert_priced_sale(
        tx,
        PricedSale {
            lines: &priced,
            discount_minor: input.discount_minor,
            tax_minor: input.tax_minor,
            payment_method: &input.payment_method,
            cashier_id: input.cashier_id,
            table_id: input.table_id,
            shift_id: input.shift_id,
            order_id: None,
            tendered_minor: input.tendered_minor,
            change_minor: input.change_minor,
            payments: &input.payments,
            customer_id: input.customer_id,
            service_charge_minor: input.service_charge_minor,
            due_date: input.due_date.as_deref(),
        },
    )?;

    if let Some(table_id) = input.table_id {
        tables::close_table_order_for_sale(tx, table_id, sale.id)?;
    }

    Ok(sale)
}

/// One line's money: price_minor (integer) × qty (possibly fractional)
/// rounded back to whole minor units — the same "round straight back to whole
/// minor units" convention CLAUDE.md documents for tax-percent-derived
/// amounts. Every sale, order, refund and token amount goes through this, and
/// the frontend's `billingTotals.lineTotalMinor` mirrors it.
pub fn line_total_minor(price_minor: i64, qty: f64) -> i64 {
    (price_minor as f64 * qty).round() as i64
}

/// A sale line whose price the server has already fixed: re-read from the
/// live item by `create_sale`, or copied from `order_items` by
/// `orders::pay_lines` — never taken from the client.
#[derive(Debug, Clone)]
pub struct PricedLine {
    pub item_id: i64,
    pub qty: f64,
    pub price_minor: i64,
    pub notes: Option<String>,
}

pub struct PricedSale<'a> {
    pub lines: &'a [PricedLine],
    pub discount_minor: i64,
    pub tax_minor: i64,
    pub payment_method: &'a str,
    pub cashier_id: Option<i64>,
    pub table_id: Option<i64>,
    pub shift_id: Option<i64>,
    pub order_id: Option<i64>,
    pub tendered_minor: Option<i64>,
    pub change_minor: Option<i64>,
    pub payments: &'a [PaymentPart],
    pub customer_id: Option<i64>,
    pub service_charge_minor: i64,
    pub due_date: Option<&'a str>,
}

/// Resolves a sale's tenders against its total: no parts means the whole
/// total under `payment_method`. Returns the parts plus the value stored in
/// `sales.payment_method` (`split` for several tenders).
fn resolve_payments(
    payment_method: &str,
    parts: &[PaymentPart],
    total_minor: i64,
) -> Result<(Vec<PaymentPart>, String), SaleError> {
    if parts.is_empty() {
        if !PAYMENT_METHODS.contains(&payment_method) {
            return Err(SaleError::InvalidPaymentMethod(payment_method.to_string()));
        }
        let parts = if total_minor > 0 {
            vec![PaymentPart { method: payment_method.to_string(), amount_minor: total_minor }]
        } else {
            Vec::new()
        };
        return Ok((parts, payment_method.to_string()));
    }

    let mut merged: Vec<PaymentPart> = Vec::new();
    for part in parts {
        if !TENDER_METHODS.contains(&part.method.as_str()) {
            return Err(SaleError::InvalidPaymentMethod(part.method.clone()));
        }
        if part.amount_minor < 0 {
            return Err(SaleError::InvalidPayments("A payment amount cannot be negative".into()));
        }
        if part.amount_minor == 0 {
            continue;
        }
        match merged.iter_mut().find(|p| p.method == part.method) {
            Some(existing) => existing.amount_minor += part.amount_minor,
            None => merged.push(part.clone()),
        }
    }
    let paid: i64 = merged.iter().map(|p| p.amount_minor).sum();
    if paid != total_minor {
        return Err(SaleError::InvalidPayments(format!(
            "Payments add up to {} but the total is {}",
            crate::db::format_minor_plain(paid),
            crate::db::format_minor_plain(total_minor)
        )));
    }
    let stored = match merged.as_slice() {
        [] => payment_method.to_string(),
        [only] => only.method.clone(),
        _ => "split".to_string(),
    };
    Ok((merged, stored))
}

/// The write half of a sale, shared by `create_sale` and `orders::pay_lines`:
/// checks stock, inserts `sales` + one `sale_items` row per line, and
/// decrements stock. Lines are not merged — an order may bill the same item
/// twice at two prices (a price change between rounds).
pub fn insert_priced_sale(tx: &Transaction, sale: PricedSale) -> Result<Sale, SaleError> {
    if sale.lines.is_empty() {
        return Err(SaleError::EmptyCart);
    }
    if sale.discount_minor < 0 || sale.tax_minor < 0 || sale.service_charge_minor < 0 {
        return Err(SaleError::InvalidDiscount);
    }

    let mut subtotal_minor: i64 = 0;
    for line in sale.lines {
        if !line.qty.is_finite() || line.qty <= 0.0 {
            return Err(SaleError::InvalidQuantity);
        }
        subtotal_minor += line_total_minor(line.price_minor, line.qty);
    }

    if sale.discount_minor > subtotal_minor {
        return Err(SaleError::InvalidDiscount);
    }

    let total_minor = subtotal_minor - sale.discount_minor + sale.tax_minor + sale.service_charge_minor;
    let (parts, stored_method) = resolve_payments(sale.payment_method, sale.payments, total_minor)?;

    let credit_minor: i64 = parts.iter().filter(|p| p.method == "credit").map(|p| p.amount_minor).sum();
    let due_date = sale.due_date.map(str::trim).filter(|d| !d.is_empty());
    if credit_minor > 0 {
        let customer_id = sale
            .customer_id
            .ok_or_else(|| SaleError::InvalidPayments("Pick a customer for a pay-later sale".into()))?;
        let active: Option<i64> = tx
            .query_row("SELECT is_active FROM customers WHERE id = ?1", params![customer_id], |row| row.get(0))
            .optional()?;
        if active != Some(1) {
            return Err(SaleError::InvalidPayments("That customer no longer exists".into()));
        }
        let due = due_date.ok_or_else(|| SaleError::InvalidPayments("Set a due date for the pay-later amount".into()))?;
        crate::db::dates::parse_date(due).map_err(SaleError::InvalidPayments)?;
    }

    tx.execute(
        "INSERT INTO sales (total_minor, discount_minor, tax_minor, payment_method, cashier_id, table_id, shift_id,
                            order_id, tendered_minor, change_minor, customer_id, service_charge_minor, due_date)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            total_minor,
            sale.discount_minor,
            sale.tax_minor,
            stored_method,
            sale.cashier_id,
            sale.table_id,
            sale.shift_id,
            sale.order_id,
            sale.tendered_minor,
            sale.change_minor,
            sale.customer_id,
            sale.service_charge_minor,
            if credit_minor > 0 { due_date } else { None },
        ],
    )?;
    let sale_id = tx.last_insert_rowid();
    for part in &parts {
        tx.execute(
            "INSERT INTO sale_payments (sale_id, method, amount_minor) VALUES (?1, ?2, ?3)",
            params![sale_id, part.method, part.amount_minor],
        )?;
    }

    for line in sale.lines {
        // Read inside the transaction, so an earlier line of the same item
        // in this sale has already been taken off.
        let (name, stock_qty, cost_minor): (String, f64, i64) = tx
            .query_row("SELECT name, stock_qty, cost_minor FROM items WHERE id = ?1", params![line.item_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .optional()?
            .ok_or(SaleError::ItemNotFound(line.item_id))?;
        if stock_qty < line.qty {
            return Err(SaleError::InsufficientStock { name, available: stock_qty, requested: line.qty });
        }

        // Blank notes are stored as NULL, not "", same normalization every
        // other optional text field in the schema uses.
        let notes = line.notes.as_deref().map(str::trim).filter(|s| !s.is_empty());
        tx.execute(
            "INSERT INTO sale_items (sale_id, item_id, qty, price_at_sale_minor, notes, cost_at_sale_minor)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![sale_id, line.item_id, line.qty, line.price_minor, notes, cost_minor],
        )?;

        // The `stock_qty >= ?1` guard makes this a compare-and-swap: if stock
        // moved under us since the read above (impossible today with the
        // single mutex-guarded connection, but this is what stays correct if
        // that ever changes), the update matches zero rows and we bail out —
        // the whole transaction rolls back, so no partial sale is ever saved.
        let changed = tx.execute(
            "UPDATE items SET stock_qty = stock_qty - ?1 WHERE id = ?2 AND stock_qty >= ?1",
            params![line.qty, line.item_id],
        )?;
        if changed == 0 {
            return Err(SaleError::InsufficientStock { name, available: 0.0, requested: line.qty });
        }
    }

    load_sale(tx, sale_id)?.ok_or(SaleError::NotFound)
}

fn load_sale(conn: &Connection, id: i64) -> Result<Option<Sale>, rusqlite::Error> {
    let sale = conn
        .query_row(
            "SELECT s.id, s.discount_minor, s.tax_minor, s.total_minor, s.payment_method,
                    s.cashier_id, u.name AS cashier_name, s.table_id, t.name AS table_name,
                    s.shift_id, s.created_at, s.tendered_minor, s.change_minor,
                    s.order_id, o.order_no,
                    o.order_type, COALESCE(c.name, o.customer_name), COALESCE(c.phone, o.customer_phone),
                    o.delivery_address,
                    o.delivery_rider_name, o.delivery_rider_phone, o.delivery_platform_name,
                    o.platform_order_ref, o.delivery_charge_minor,
                    s.service_charge_minor, s.customer_id, c.email, s.due_date, s.gid
               FROM sales s
               LEFT JOIN users u ON u.id = s.cashier_id
               LEFT JOIN tables t ON t.id = s.table_id
               LEFT JOIN orders o ON o.id = s.order_id
               LEFT JOIN customers c ON c.id = s.customer_id
              WHERE s.id = ?1",
            params![id],
            |row| {
                let discount_minor: i64 = row.get(1)?;
                let tax_minor: i64 = row.get(2)?;
                let total_minor: i64 = row.get(3)?;
                let service_charge_minor: i64 = row.get(24)?;
                let sale = Sale {
                    id: row.get(0)?,
                    // subtotal = total - tax - service + discount, recovered
                    // from the stored amounts rather than another column.
                    subtotal_minor: total_minor - tax_minor - service_charge_minor + discount_minor,
                    discount_minor,
                    tax_minor,
                    total_minor,
                    payment_method: row.get(4)?,
                    cashier_id: row.get(5)?,
                    cashier_name: row.get(6)?,
                    table_id: row.get(7)?,
                    table_name: row.get(8)?,
                    shift_id: row.get(9)?,
                    tendered_minor: row.get(11)?,
                    change_minor: row.get(12)?,
                    order_id: row.get(13)?,
                    order_no: row.get(14)?,
                    created_at: row.get(10)?,
                    items: Vec::new(),
                    order_type: row.get(15)?,
                    customer_name: row.get(16)?,
                    customer_phone: row.get(17)?,
                    delivery_address: row.get(18)?,
                    delivery_rider_name: row.get(19)?,
                    delivery_rider_phone: row.get(20)?,
                    delivery_platform_name: row.get(21)?,
                    platform_order_ref: row.get(22)?,
                    delivery_charge_minor: row.get(23)?,
                    service_charge_minor,
                    payments: Vec::new(),
                    customer_id: row.get(25)?,
                    customer_email: row.get(26)?,
                    due_date: row.get(27)?,
                    receipt_no: String::new(),
                    till_name: None,
                };
                Ok((sale, row.get::<_, Option<String>>(28)?))
            },
        )
        .optional()?;

    let Some((mut sale, gid)) = sale else { return Ok(None) };
    let till = crate::workspace::local::till_of(conn, gid.as_deref())?;
    sale.receipt_no = crate::workspace::local::receipt_number(sale.id, gid.as_deref(), till.as_ref());
    sale.till_name = till.map(|(_, name)| name);

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
                line_total_minor: line_total_minor(price_at_sale_minor, qty),
                notes: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut stmt = conn.prepare(
        "SELECT method, amount_minor FROM sale_payment_lines WHERE sale_id = ?1",
    )?;
    sale.payments = stmt
        .query_map(params![id], |row| Ok(PaymentPart { method: row.get(0)?, amount_minor: row.get(1)? }))?
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

/// Wipes the whole sales history: sales and their lines/tenders, refunds,
/// installments taken against a sale, active/held orders, kitchen tokens and
/// parked table orders; occupied tables are freed. Items, stock, customers,
/// shifts and expenses are kept. Children go first because of the
/// `ON DELETE RESTRICT` references. On a connected counter the delete
/// triggers queue every row, so the reset reaches the whole business.
pub fn reset_all(tx: &Transaction) -> rusqlite::Result<()> {
    tx.execute_batch(
        "DELETE FROM refund_items;
         DELETE FROM refunds;
         DELETE FROM customer_payments WHERE sale_id IS NOT NULL;
         DELETE FROM token_items;
         DELETE FROM tokens;
         DELETE FROM order_items;
         DELETE FROM orders;
         DELETE FROM sale_payments;
         DELETE FROM sale_items;
         DELETE FROM sales;
         DELETE FROM table_orders;
         UPDATE tables SET status = 'free' WHERE status = 'occupied';",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::{item_id, one_line_sale, stock};
    use crate::db::items::{self, ItemInput};
    use crate::db::schema::test_conn;



    fn basic_input(conn: &Connection) -> CreateSaleInput {
        one_line_sale(item_id(conn, "Cola 500ml"), 2.0)
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

    /// A cashier typing a rupee amount must be billed *that* amount, to the
    /// paisa — the whole point of "sell by amount".
    ///
    /// The client turns the typed amount into a qty (`amount ÷ price`, in
    /// `billingStore.addItemByAmount`) and this function turns it back into
    /// money (`round(price × qty)`). That round trip only reduces to the
    /// original amount if the quotient is carried at full precision:
    /// rounding it to 2dp first used to bill 99.00 for a typed 100.00 at
    /// 300.00/unit, and 51.00 for a typed 50.00 — wrong in both directions.
    ///
    /// The rates below are chosen so `amount ÷ price` is a non-terminating
    /// decimal, which is exactly the case that used to break.
    #[test]
    fn an_amount_entered_line_bills_the_typed_amount_exactly() {
        let mut conn = test_conn();
        let cola = item_id(&conn, "Cola 500ml");
        let price_minor: i64 =
            conn.query_row("SELECT price_minor FROM items WHERE id = ?1", params![cola], |row| row.get(0)).unwrap();

        for typed_amount_minor in [10_000_i64, 5_000, 12_345, 100, 1] {
            // Exactly what `addItemByAmount` now sends: the unrounded quotient.
            let qty = typed_amount_minor as f64 / price_minor as f64;

            let tx = conn.transaction().unwrap();
            let input =
                CreateSaleInput { items: vec![CartLine { item_id: cola, qty, notes: None }], ..basic_input(&tx) };
            let sale = create_sale(&tx, input).unwrap();
            tx.commit().unwrap();

            assert_eq!(
                sale.items[0].line_total_minor, typed_amount_minor,
                "a typed amount of {typed_amount_minor} minor units must bill exactly that, not {} — \
                 qty must reach the server unrounded",
                sale.items[0].line_total_minor
            );
            assert_eq!(sale.total_minor, typed_amount_minor, "and the sale total must match it too");
        }
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
                short_code: None,
                description: None,
                price_minor: 12_987, // PKR 129.87/kg
                cost_minor: 10_000,
                stock_qty: 50.0,
                category_id: None,
                low_stock_threshold: 2,
                image_path: None,
                sold_by_amount: true,
                unit: Some("kg".into()),
                counter_id: None,
                original_stock_qty: None,
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
                ..basic_input(&tx)
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
    fn rejects_a_non_positive_or_non_finite_quantity() {
        let mut conn = test_conn();
        let tx = conn.transaction().unwrap();
        let cola = item_id(&tx, "Cola 500ml");
        for qty in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let input = CreateSaleInput {
                items: vec![CartLine { item_id: cola, qty, notes: None }],
                ..basic_input(&tx)
            };
            assert!(matches!(create_sale(&tx, input), Err(SaleError::InvalidQuantity)), "qty {qty} should be rejected");
        }
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
                ..basic_input(&tx)
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
            ..basic_input(&tx)
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
            ..basic_input(&tx)
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

    #[test]
    fn reset_all_deletes_sales_history_but_keeps_stock() {
        let mut conn = test_conn();
        crate::db::test_support::ring_up(&mut conn, "Cola 500ml", 2.0, "cash", None);
        let stock_after_sale = stock(&conn, "Cola 500ml");

        crate::db::test_support::in_tx(&mut conn, reset_all).unwrap();

        for table in ["sales", "sale_items", "sale_payments", "refunds", "orders", "tokens", "table_orders"] {
            assert_eq!(crate::db::test_support::count(&conn, &format!("SELECT COUNT(*) FROM {table}")), 0, "{table}");
        }
        assert_eq!(stock(&conn, "Cola 500ml"), stock_after_sale, "a reset never moves stock");
    }
}
