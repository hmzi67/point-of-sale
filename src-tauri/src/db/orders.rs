//! Restaurant orders: the order lifecycle on top of `orders`, `order_items`
//! and `order_status_events` (see `schema.sql`'s `orders` section).
//!
//! An order runs on two tracks. Fulfillment is `orders.status`, moved only
//! through the functions here and only along `can_transition`. Payment is
//! never stored: a line is paid once it has a `sale_id`, and `pay_lines`
//! is an ordinary sale (`sales::insert_priced_sale`) at the server-set
//! `price_at_order_minor`. An order is completed when it is served and
//! every active line is paid; `completed_at` is stamped then and the order
//! is closed to further changes.
//!
//! Every write takes the order's `expected_version` and is refused with
//! `Stale` if another write landed first, so two devices can never
//! silently overwrite each other. The caller's role is checked by the Tauri
//! commands (`require_role`); an `approver` passed in is a user whose PIN the
//! command has verified, and is still refused here unless that user is an
//! active Owner or Admin.

// shortcut: nothing calls this module until the orders Tauri commands land
// (checklist item 3); remove this allow then.
#![cfg_attr(not(test), allow(dead_code))]

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

use super::sales::{self, PricedLine, PricedSale, Sale, SaleError};
use super::tokens;

pub const ORDER_TYPES: &[&str] = &["dine_in", "takeaway", "delivery", "third_party_delivery"];

/// Own-rider and third-party delivery orders both go through the delivery
/// status flow rather than straight to served.
pub fn is_delivery(order_type: &str) -> bool {
    matches!(order_type, "delivery" | "third_party_delivery")
}

/// Every fulfillment status, in lifecycle order.
pub const DRAFT: &str = "draft";
pub const SENT: &str = "sent";
pub const PREPARING: &str = "preparing";
pub const READY: &str = "ready";
pub const SERVED: &str = "served";
pub const HELD: &str = "held";
pub const CANCELLED: &str = "cancelled";
pub const STATUSES: &[&str] = &[DRAFT, SENT, PREPARING, READY, SERVED, HELD, CANCELLED];

/// Delivery lifecycle statuses. Kept completely separate from fulfillment status.
pub const DELIVERY_PLACED: &str = "placed";
pub const DELIVERY_DISPATCHED: &str = "dispatched";
pub const DELIVERY_DELIVERED: &str = "delivered";
pub const DELIVERY_CANCELLED: &str = "cancelled";
pub const DELIVERY_STATUSES: &[&str] = &[
    DELIVERY_PLACED,
    DELIVERY_DISPATCHED,
    DELIVERY_DELIVERED,
    DELIVERY_CANCELLED,
];

/// `order_items.status`. SQL text spells these as literals, matching the
/// schema's CHECK constraints.
pub const LINE_ACTIVE: &str = "active";
pub const LINE_VOIDED: &str = "voided";

/// The lifecycle table: may an order move from `from` to `to`? Each
/// operation below checks this; `resume` additionally requires `to` to be
/// the status the order was held from.
pub fn can_transition(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        (DRAFT, SENT | PREPARING | READY | SERVED | HELD | CANCELLED)
            | (SENT, PREPARING | READY | SERVED | HELD | CANCELLED)
            | (PREPARING, READY | SERVED | HELD | CANCELLED)
            | (READY, SENT | SERVED | HELD | CANCELLED)
            | (SERVED, SENT | READY | HELD | CANCELLED)
            | (HELD, DRAFT | SENT | PREPARING | READY | SERVED | CANCELLED)
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryDetails {
    pub delivery_rider_id: Option<i64>,
    pub rider_source: Option<String>,
    pub delivery_rider_name: Option<String>,
    pub delivery_rider_phone: Option<String>,
    pub delivery_platform_id: Option<i64>,
    pub delivery_platform_name: Option<String>,
    pub customer_phone: Option<String>,
    pub delivery_address: Option<String>,
    pub delivery_charge_minor: Option<i64>,
    pub delivery_status: Option<String>,
    pub platform_order_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderLine {
    pub id: i64,
    pub item_id: i64,
    pub item_name: String,
    pub qty: f64,
    pub price_at_order_minor: i64,
    pub notes: Option<String>,
    /// "active" | "voided"
    pub status: String,
    pub sent_at: Option<String>,
    pub sale_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Order {
    pub id: i64,
    pub order_no: i64,
    pub order_type: String,
    pub table_id: Option<i64>,
    pub status: String,
    pub held_from_status: Option<String>,
    pub version: i64,
    pub client_ref: String,
    pub customer_name: Option<String>,
    pub notes: Option<String>,
    pub estimated_prep_minutes: i64,
    pub table_name: Option<String>,
    /// Derived: "unpaid" | "partial" | "paid".
    pub payment_status: String,
    /// What the unpaid active lines still cost (price × qty, rounded).
    pub due_minor: i64,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub cancelled_at: Option<String>,
    pub shift_id: Option<i64>,
    pub delivery_rider_id: Option<i64>,
    pub rider_source: Option<String>,
    pub delivery_rider_name: Option<String>,
    pub delivery_rider_phone: Option<String>,
    pub delivery_platform_id: Option<i64>,
    pub delivery_platform_name: Option<String>,
    pub customer_phone: Option<String>,
    pub delivery_address: Option<String>,
    pub delivery_charge_minor: i64,
    pub delivery_status: Option<String>,
    pub platform_order_ref: Option<String>,
    pub lines: Vec<OrderLine>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewLine {
    pub item_id: i64,
    pub qty: f64,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineChange {
    Qty(f64),
    Remove,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendLine {
    pub order_item_id: i64,
    pub item_id: i64,
    pub item_name: String,
    pub qty: f64,
    pub notes: Option<String>,
}

/// The unsent lines for one kitchen counter.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendGroup {
    pub counter_id: i64,
    pub counter_name: String,
    pub lines: Vec<SendLine>,
}

/// What Send would print. `version` goes back into `record_send`, so a
/// change between preview and record is refused as `Stale`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendPreview {
    pub version: i64,
    pub groups: Vec<SendGroup>,
    /// Lines whose item has no counter: marked sent, never on a token.
    pub unrouted: Vec<SendLine>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sent {
    pub order: Order,
    pub token_ids: Vec<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct PayInput {
    pub order_item_ids: Vec<i64>,
    pub payment_method: String,
    pub discount_minor: i64,
    pub tax_minor: i64,
    pub cashier_id: Option<i64>,
    pub shift_id: Option<i64>,
    pub tendered_minor: Option<i64>,
    pub change_minor: Option<i64>,
    /// As `sales::CreateSaleInput`'s fields of the same names.
    pub payments: Vec<sales::PaymentPart>,
    pub customer_id: Option<i64>,
    pub service_charge_minor: i64,
    pub due_date: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Paid {
    pub order: Order,
    pub sale: Sale,
}

#[derive(Debug, thiserror::Error)]
pub enum OrderError {
    #[error("Order not found")]
    NotFound,
    /// Boxed: an `Order` would make every `Result` here large.
    #[error("This order was just updated on another screen")]
    Stale { current: Box<Order> },
    #[error("An order that is {from} cannot become {to}")]
    BadTransition { from: String, to: String },
    /// Completed or cancelled: nothing may change.
    #[error("This order is already completed or cancelled")]
    Closed,
    #[error("Unknown order type: {0}")]
    InvalidOrderType(String),
    #[error("A dine-in order needs a table")]
    TableRequired,
    #[error("Only a dine-in order can have a table")]
    TableOnlyForDineIn,
    #[error("Table not found")]
    TableNotFound,
    #[error("No items given")]
    NoLines,
    #[error("Quantity must be more than zero")]
    InvalidQuantity,
    #[error("Item {0} no longer exists")]
    ItemNotFound(i64),
    #[error("{0} is no longer sold")]
    ItemInactive(String),
    #[error("Line {0} is not on this order")]
    LineNotFound(i64),
    #[error("That item was already sent to the kitchen; void it instead")]
    LineAlreadySent,
    #[error("That item was never sent to the kitchen; change it instead")]
    LineNotSent,
    #[error("That item is already paid")]
    LinePaid,
    #[error("That item was already voided")]
    LineVoided,
    #[error("Nothing new to send to the kitchen")]
    NothingToSend,
    #[error("A manager must approve this")]
    ApprovalRequired,
    /// The approver is not an active Owner or Admin.
    #[error("Only an Owner or Admin can approve this")]
    ApproverNotAllowed,
    #[error("Refund the paid items before cancelling this order")]
    HasPaidLines,
    #[error("That ticket does not belong to an order")]
    NotAnOrderToken,
    #[error("Customer phone and address are required for delivery orders")]
    DeliveryCustomerInfoRequired,
    #[error("Rider details (name and phone) are required for delivery orders")]
    DeliveryRiderRequired,
    #[error("A delivery platform must be selected for third-party deliveries")]
    DeliveryPlatformRequired,
    #[error("Invalid delivery status: {0}")]
    InvalidDeliveryStatus(String),
    #[error("{0}")]
    Sale(#[from] SaleError),
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// Creates a draft order, or returns the existing one if `client_ref` was
/// already used (a retried submission). Dine-in needs a table, which is
/// marked occupied.
pub fn create_order(
    tx: &Transaction,
    client_ref: &str,
    order_type: &str,
    table_id: Option<i64>,
    user_id: Option<i64>,
    customer_name: Option<&str>,
    notes: Option<&str>,
    estimated_prep_minutes: Option<i64>,
    shift_id: Option<i64>,
    delivery: Option<DeliveryDetails>,
) -> Result<Order, OrderError> {
    if let Some(id) = tx
        .query_row("SELECT id FROM orders WHERE client_ref = ?1", params![client_ref], |row| row.get::<_, i64>(0))
        .optional()?
    {
        return get_order(tx, id);
    }
    if !ORDER_TYPES.contains(&order_type) {
        return Err(OrderError::InvalidOrderType(order_type.to_string()));
    }
    match (order_type, table_id) {
        ("dine_in", None) => return Err(OrderError::TableRequired),
        ("dine_in", Some(table)) => {
            let exists: bool =
                tx.query_row("SELECT EXISTS(SELECT 1 FROM tables WHERE id = ?1)", params![table], |row| row.get(0))?;
            if !exists {
                return Err(OrderError::TableNotFound);
            }
        }
        (_, Some(_)) => return Err(OrderError::TableOnlyForDineIn),
        (_, None) => {}
    }

    let (
        delivery_rider_id,
        rider_source,
        mut delivery_rider_name,
        delivery_rider_phone,
        delivery_platform_id,
        mut delivery_platform_name,
        customer_phone,
        delivery_address,
        delivery_charge_minor,
        delivery_status,
        platform_order_ref,
    ) = match delivery {
        Some(d) => (
            d.delivery_rider_id,
            d.rider_source,
            d.delivery_rider_name,
            d.delivery_rider_phone,
            d.delivery_platform_id,
            d.delivery_platform_name,
            d.customer_phone,
            d.delivery_address,
            d.delivery_charge_minor.unwrap_or(0),
            d.delivery_status,
            d.platform_order_ref,
        ),
        None => (None, None, None, None, None, None, None, None, 0, None, None),
    };

    if order_type == "delivery" {
        let phone_valid = customer_phone.as_deref().map(|s| !s.trim().is_empty()).unwrap_or(false);
        let addr_valid = delivery_address.as_deref().map(|s| !s.trim().is_empty()).unwrap_or(false);
        if !phone_valid || !addr_valid {
            return Err(OrderError::DeliveryCustomerInfoRequired);
        }

        let is_user_rider = rider_source.as_deref() == Some("user") && delivery_rider_id.is_some();
        let is_manual_rider = delivery_rider_name.as_deref().map(|s| !s.trim().is_empty()).unwrap_or(false)
            && delivery_rider_phone.as_deref().map(|s| !s.trim().is_empty()).unwrap_or(false);

        if !is_user_rider && !is_manual_rider {
            return Err(OrderError::DeliveryRiderRequired);
        }

        if is_user_rider {
            if let Some(rid) = delivery_rider_id {
                if delivery_rider_name.is_none() {
                    delivery_rider_name = tx
                        .query_row("SELECT name FROM users WHERE id = ?1", params![rid], |r| r.get(0))
                        .optional()?;
                }
            }
        }
    } else if order_type == "third_party_delivery" {
        if delivery_platform_id.is_none()
            && delivery_platform_name.as_deref().map(|s| s.trim().is_empty()).unwrap_or(true)
        {
            return Err(OrderError::DeliveryPlatformRequired);
        }
        if let Some(pid) = delivery_platform_id {
            if delivery_platform_name.is_none() {
                delivery_platform_name = tx
                    .query_row("SELECT name FROM delivery_platforms WHERE id = ?1", params![pid], |r| r.get(0))
                    .optional()?;
            }
        }
    }

    let final_delivery_status = if is_delivery(order_type) {
        Some(delivery_status.unwrap_or_else(|| DELIVERY_PLACED.to_string()))
    } else {
        None
    };

    // Resolve shift: use explicitly passed shift_id, or look for open shift for user / store
    let effective_shift_id = match shift_id {
        Some(s) => Some(s),
        None => {
            if let Some(uid) = user_id {
                tx.query_row(
                    &format!("SELECT id FROM shifts WHERE cashier_id = ?1 AND closed_at IS NULL AND {}", crate::db::shifts::OPENED_HERE),
                    params![uid],
                    |row| row.get(0),
                )
                .optional()?
                .or_else(|| {
                    tx.query_row(
                        &format!("SELECT id FROM shifts WHERE closed_at IS NULL AND {} ORDER BY id DESC LIMIT 1", crate::db::shifts::OPENED_HERE),
                        [],
                        |row| row.get(0),
                    )
                    .optional()
                    .unwrap_or(None)
                })
            } else {
                tx.query_row(
                    &format!("SELECT id FROM shifts WHERE closed_at IS NULL AND {} ORDER BY id DESC LIMIT 1", crate::db::shifts::OPENED_HERE),
                    [],
                    |row| row.get(0),
                )
                .optional()?
            }
        }
    };

    // Sequential order number starts from 1 when shift starts, next shift starts, or daily
    let order_no: i64 = match effective_shift_id {
        Some(sid) => tx.query_row(
            "SELECT COALESCE(MAX(order_no), 0) + 1 FROM orders WHERE shift_id = ?1",
            params![sid],
            |row| row.get(0),
        )?,
        None => tx.query_row(
            "SELECT COALESCE(MAX(order_no), 0) + 1 FROM orders WHERE shift_id IS NULL AND date(created_at) = date('now', 'localtime')",
            [],
            |row| row.get(0),
        )?,
    };

    let prep_minutes = estimated_prep_minutes.unwrap_or(15);
    tx.execute(
        "INSERT INTO orders (
            order_no, order_type, table_id, client_ref, created_by, customer_name, notes,
            estimated_prep_minutes, shift_id, delivery_rider_id, rider_source, delivery_rider_name,
            delivery_rider_phone, delivery_platform_id, delivery_platform_name, customer_phone,
            delivery_address, delivery_charge_minor, delivery_status, platform_order_ref
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
        params![
            order_no,
            order_type,
            table_id,
            client_ref,
            user_id,
            customer_name,
            notes,
            prep_minutes,
            effective_shift_id,
            delivery_rider_id,
            rider_source,
            delivery_rider_name,
            delivery_rider_phone,
            delivery_platform_id,
            delivery_platform_name,
            customer_phone,
            delivery_address,
            delivery_charge_minor,
            final_delivery_status,
            platform_order_ref,
        ],
    )?;
    let order_id = tx.last_insert_rowid();
    log_event(tx, order_id, None, DRAFT, user_id, None)?;
    if let Some(table) = table_id {
        tx.execute("UPDATE tables SET status = 'occupied' WHERE id = ?1", params![table])?;
    }
    get_order(tx, order_id)
}

pub fn update_delivery_status(
    tx: &Transaction,
    order_id: i64,
    status: &str,
    user_id: Option<i64>,
) -> Result<Order, OrderError> {
    if !DELIVERY_STATUSES.contains(&status) {
        return Err(OrderError::InvalidDeliveryStatus(status.to_string()));
    }
    let order = get_order(tx, order_id)?;
    if order.status == CANCELLED {
        return Err(OrderError::Closed);
    }
    tx.execute(
        "UPDATE orders SET delivery_status = ?1 WHERE id = ?2",
        params![status, order_id],
    )?;
    log_event(tx, order_id, None, &format!("delivery_{}", status), user_id, None)?;
    finish(tx, order_id)
}

pub fn get_order(conn: &Connection, order_id: i64) -> Result<Order, OrderError> {
    let order = conn
        .query_row(
            "SELECT o.id, o.order_no, o.order_type, o.table_id, o.status, o.held_from_status, o.version, o.client_ref,
                    o.customer_name, o.notes, o.estimated_prep_minutes, o.created_at, o.completed_at, o.cancelled_at,
                    t.name, o.shift_id,
                    o.delivery_rider_id, o.rider_source, o.delivery_rider_name, o.delivery_rider_phone,
                    o.delivery_platform_id, o.delivery_platform_name, o.customer_phone, o.delivery_address,
                    o.delivery_charge_minor, o.delivery_status, o.platform_order_ref
               FROM orders o
          LEFT JOIN tables t ON t.id = o.table_id
              WHERE o.id = ?1",
            params![order_id],
            |row| {
                Ok(Order {
                    id: row.get(0)?,
                    order_no: row.get(1)?,
                    order_type: row.get(2)?,
                    table_id: row.get(3)?,
                    status: row.get(4)?,
                    held_from_status: row.get(5)?,
                    version: row.get(6)?,
                    client_ref: row.get(7)?,
                    customer_name: row.get(8)?,
                    notes: row.get(9)?,
                    estimated_prep_minutes: row.get::<_, Option<i64>>(10)?.unwrap_or(15),
                    payment_status: String::new(),
                    due_minor: 0,
                    created_at: row.get(11)?,
                    completed_at: row.get(12)?,
                    cancelled_at: row.get(13)?,
                    table_name: row.get(14)?,
                    shift_id: row.get(15)?,
                    delivery_rider_id: row.get(16)?,
                    rider_source: row.get(17)?,
                    delivery_rider_name: row.get(18)?,
                    delivery_rider_phone: row.get(19)?,
                    delivery_platform_id: row.get(20)?,
                    delivery_platform_name: row.get(21)?,
                    customer_phone: row.get(22)?,
                    delivery_address: row.get(23)?,
                    delivery_charge_minor: row.get::<_, Option<i64>>(24)?.unwrap_or(0),
                    delivery_status: row.get(25)?,
                    platform_order_ref: row.get(26)?,
                    lines: Vec::new(),
                })
            },
        )
        .optional()?;
    let mut order = order.ok_or(OrderError::NotFound)?;

    let mut stmt = conn.prepare(
        "SELECT oi.id, oi.item_id, i.name, oi.qty, oi.price_at_order_minor, oi.notes, oi.status, oi.sent_at, oi.sale_id
           FROM order_items oi JOIN items i ON i.id = oi.item_id
          WHERE oi.order_id = ?1 ORDER BY oi.id",
    )?;
    order.lines = stmt
        .query_map(params![order_id], |row| {
            Ok(OrderLine {
                id: row.get(0)?,
                item_id: row.get(1)?,
                item_name: row.get(2)?,
                qty: row.get(3)?,
                price_at_order_minor: row.get(4)?,
                notes: row.get(5)?,
                status: row.get(6)?,
                sent_at: row.get(7)?,
                sale_id: row.get(8)?,
            })
        })?
        .collect::<Result<_, _>>()?;

    let active: Vec<&OrderLine> = order.lines.iter().filter(|l| l.status == LINE_ACTIVE).collect();
    let paid = active.iter().filter(|l| l.sale_id.is_some()).count();
    order.payment_status = match paid {
        0 => "unpaid",
        n if n == active.len() => "paid",
        _ => "partial",
    }
    .to_string();
    let items_due: i64 = active
        .iter()
        .filter(|l| l.sale_id.is_none())
        .map(|l| sales::line_total_minor(l.price_at_order_minor, l.qty))
        .sum();
    order.due_minor = if items_due > 0 {
        items_due + order.delivery_charge_minor
    } else {
        0
    };
    Ok(order)
}

/// Adds lines at the item's live price, copied server-side.
pub fn add_lines(
    tx: &Transaction,
    order_id: i64,
    expected_version: i64,
    lines: &[NewLine],
    user_id: Option<i64>,
) -> Result<Order, OrderError> {
    if lines.is_empty() {
        return Err(OrderError::NoLines);
    }
    let head = begin(tx, order_id, expected_version)?;
    refuse_if_held(&head, "changed")?;
    for line in lines {
        if !line.qty.is_finite() || line.qty <= 0.0 {
            return Err(OrderError::InvalidQuantity);
        }
        let (name, price_minor, is_active): (String, i64, i64) = tx
            .query_row(
                "SELECT name, price_minor, is_active FROM items WHERE id = ?1",
                params![line.item_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?
            .ok_or(OrderError::ItemNotFound(line.item_id))?;
        if is_active == 0 {
            return Err(OrderError::ItemInactive(name));
        }
        let notes = line.notes.as_deref().map(str::trim).filter(|s| !s.is_empty());
        tx.execute(
            "INSERT INTO order_items (order_id, item_id, qty, price_at_order_minor, notes, added_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![order_id, line.item_id, line.qty, price_minor, notes, user_id],
        )?;
    }
    finish(tx, order_id)
}

/// Changes or removes a line the kitchen has not been told about yet.
pub fn change_unsent_line(
    tx: &Transaction,
    order_id: i64,
    expected_version: i64,
    line_id: i64,
    change: LineChange,
) -> Result<Order, OrderError> {
    let head = begin(tx, order_id, expected_version)?;
    refuse_if_held(&head, "changed")?;
    let line = load_line(tx, order_id, line_id)?;
    if line.status == LINE_VOIDED {
        return Err(OrderError::LineVoided);
    }
    if line.sent_at.is_some() {
        return Err(OrderError::LineAlreadySent);
    }
    if line.sale_id.is_some() {
        return Err(OrderError::LinePaid);
    }
    match change {
        LineChange::Qty(qty) if !qty.is_finite() || qty <= 0.0 => return Err(OrderError::InvalidQuantity),
        LineChange::Qty(qty) => tx.execute("UPDATE order_items SET qty = ?1 WHERE id = ?2", params![qty, line_id])?,
        LineChange::Remove => tx.execute("DELETE FROM order_items WHERE id = ?1", params![line_id])?,
    };
    finish(tx, order_id)
}

pub fn pending_send_groups(conn: &Connection, order_id: i64) -> Result<SendPreview, OrderError> {
    let version = load_head(conn, order_id)?.version;
    let mut stmt = conn.prepare(
        "SELECT oi.id, oi.item_id, i.name, oi.qty, oi.notes, c.id, c.name
           FROM order_items oi
           JOIN items i ON i.id = oi.item_id
           LEFT JOIN counters c ON c.id = i.counter_id AND c.is_active = 1
          WHERE oi.order_id = ?1 AND oi.status = 'active' AND oi.sent_at IS NULL
          ORDER BY c.id, oi.id",
    )?;
    let rows = stmt.query_map(params![order_id], |row| {
        Ok((
            SendLine {
                order_item_id: row.get(0)?,
                item_id: row.get(1)?,
                item_name: row.get(2)?,
                qty: row.get(3)?,
                notes: row.get(4)?,
            },
            row.get::<_, Option<i64>>(5)?,
            row.get::<_, Option<String>>(6)?,
        ))
    })?;

    let mut preview = SendPreview { version, groups: Vec::new(), unrouted: Vec::new() };
    for row in rows {
        let (line, counter_id, counter_name) = row?;
        match (counter_id, counter_name) {
            (Some(counter_id), Some(counter_name)) => match preview.groups.last_mut() {
                Some(group) if group.counter_id == counter_id => group.lines.push(line),
                _ => preview.groups.push(SendGroup { counter_id, counter_name, lines: vec![line] }),
            },
            _ => preview.unrouted.push(line),
        }
    }
    Ok(preview)
}

/// Records a Send that has already printed: one token per counter for the
/// unsent lines, `sent_at` on every unsent line, and draft/ready/served →
/// sent (a new round). Sent or preparing orders keep their status.
pub fn record_send(
    tx: &Transaction,
    order_id: i64,
    expected_version: i64,
    user_id: Option<i64>,
) -> Result<Sent, OrderError> {
    let head = begin(tx, order_id, expected_version)?;
    refuse_if_held(&head, SENT)?;
    let preview = pending_send_groups(tx, order_id)?;
    if preview.groups.is_empty() && preview.unrouted.is_empty() {
        return Err(OrderError::NothingToSend);
    }

    let mut token_ids = Vec::with_capacity(preview.groups.len());
    for group in &preview.groups {
        let token_number = tokens::next_token_number_for_shift_or_today(tx, head.shift_id)?;
        tx.execute(
            "INSERT INTO tokens (token_number, counter_id, printed_by, order_id, shift_id) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![token_number, group.counter_id, user_id, order_id, head.shift_id],
        )?;
        let token_id = tx.last_insert_rowid();
        for line in &group.lines {
            tx.execute(
                "INSERT INTO token_items (token_id, item_id, qty_on_token, order_item_id) VALUES (?1, ?2, ?3, ?4)",
                params![token_id, line.item_id, line.qty, line.order_item_id],
            )?;
        }
        token_ids.push(token_id);
    }
    tx.execute(
        "UPDATE order_items SET sent_at = datetime('now', 'localtime')
          WHERE order_id = ?1 AND status = 'active' AND sent_at IS NULL",
        params![order_id],
    )?;
    if matches!(head.status.as_str(), DRAFT | READY | SERVED) {
        set_status(tx, order_id, &head.status, SENT, user_id)?;
    }
    Ok(Sent { order: finish(tx, order_id)?, token_ids })
}

/// The manual moves: preparing, ready (also marks every token ready — the
/// fallback for a station with no kitchen screen) and served.
pub fn transition(
    tx: &Transaction,
    order_id: i64,
    expected_version: i64,
    to: &str,
    user_id: Option<i64>,
) -> Result<Order, OrderError> {
    if !matches!(to, PREPARING | READY | SERVED) {
        return Err(OrderError::BadTransition { from: "?".into(), to: to.into() });
    }
    let head = begin(tx, order_id, expected_version)?;
    if head.status == HELD {
        tx.execute("UPDATE orders SET held_from_status = NULL WHERE id = ?1", params![order_id])?;
    }
    set_status(tx, order_id, &head.status, to, user_id)?;
    if to == READY {
        tx.execute(
            "UPDATE tokens SET kitchen_status = 'ready', ready_at = COALESCE(ready_at, datetime('now', 'localtime'))
              WHERE order_id = ?1 AND status <> 'cancelled'",
            params![order_id],
        )?;
    }
    if to == SERVED {
        let is_fully_paid: bool = {
            let unpaid_count: i64 = tx.query_row(
                "SELECT COUNT(*) FROM order_items WHERE order_id = ?1 AND status = 'active' AND sale_id IS NULL",
                params![order_id],
                |r| r.get(0),
            )?;
            let total_active: i64 = tx.query_row(
                "SELECT COUNT(*) FROM order_items WHERE order_id = ?1 AND status = 'active'",
                params![order_id],
                |r| r.get(0),
            )?;
            total_active > 0 && unpaid_count == 0
        };
        if is_fully_paid {
            tx.execute("UPDATE orders SET completed_at = datetime('now', 'localtime') WHERE id = ?1", params![order_id])?;
            if let Some(table_id) = head.table_id {
                free_table_if_idle(tx, table_id)?;
            }
        }
    }
    finish(tx, order_id)
}

pub fn hold(tx: &Transaction, order_id: i64, expected_version: i64, user_id: Option<i64>) -> Result<Order, OrderError> {
    let head = begin(tx, order_id, expected_version)?;
    set_status(tx, order_id, &head.status, HELD, user_id)?;
    tx.execute("UPDATE orders SET held_from_status = ?1 WHERE id = ?2", params![head.status, order_id])?;
    finish(tx, order_id)
}

pub fn resume(tx: &Transaction, order_id: i64, expected_version: i64, user_id: Option<i64>) -> Result<Order, OrderError> {
    let head = begin(tx, order_id, expected_version)?;
    let to = match (head.status.as_str(), head.held_from_status.as_deref()) {
        (HELD, Some(to)) => to.to_string(),
        _ => return Err(OrderError::BadTransition { from: head.status, to: "resumed".into() }),
    };
    set_status(tx, order_id, HELD, &to, user_id)?;
    tx.execute("UPDATE orders SET held_from_status = NULL WHERE id = ?1", params![order_id])?;
    finish(tx, order_id)
}

/// Kitchen display bump: forward only, repeating it is a no-op. The order
/// follows: sent → preparing on the first start, → ready when every live
/// token is ready. No version: kitchen screens never edit the order itself.
pub fn bump_token(tx: &Transaction, token_id: i64, to: &str, user_id: Option<i64>) -> Result<Order, OrderError> {
    fn rank(status: &str) -> Option<u8> {
        match status {
            "queued" => Some(0),
            PREPARING => Some(1),
            READY => Some(2),
            _ => None,
        }
    }
    let to_rank = rank(to).filter(|r| *r > 0).ok_or_else(|| OrderError::BadTransition { from: "?".into(), to: to.into() })?;
    let (order_id, current, token_status): (Option<i64>, String, String) = tx
        .query_row("SELECT order_id, kitchen_status, status FROM tokens WHERE id = ?1", params![token_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .optional()?
        .ok_or(OrderError::NotFound)?;
    let order_id = order_id.ok_or(OrderError::NotAnOrderToken)?;
    if token_status == "cancelled" {
        return Err(OrderError::Closed);
    }
    let current_rank = rank(&current).unwrap_or(0);
    if to_rank == current_rank {
        return get_order(tx, order_id);
    }
    if to_rank < current_rank {
        return Err(OrderError::BadTransition { from: current, to: to.into() });
    }
    tx.execute(
        "UPDATE tokens SET kitchen_status = ?1,
                ready_at = CASE WHEN ?1 = 'ready' THEN datetime('now', 'localtime') ELSE ready_at END
          WHERE id = ?2",
        params![to, token_id],
    )?;

    let head = load_head(tx, order_id)?;
    if head.completed_at.is_none() && matches!(head.status.as_str(), SENT | PREPARING) {
        let not_ready: i64 = tx.query_row(
            "SELECT COUNT(*) FROM tokens WHERE order_id = ?1 AND status <> 'cancelled' AND kitchen_status <> 'ready'",
            params![order_id],
            |row| row.get(0),
        )?;
        let next = if not_ready == 0 { READY } else { PREPARING };
        if next != head.status {
            touch(tx, order_id)?;
            set_status(tx, order_id, &head.status, next, user_id)?;
        }
    }
    finish(tx, order_id)
}

/// Voids `qty` of each sent, unpaid line. A partial quantity splits the
/// line: the original keeps the rest, a new voided row holds the voided qty.
pub fn void_lines(
    tx: &Transaction,
    order_id: i64,
    expected_version: i64,
    voids: &[(i64, f64)],
    reason: &str,
    approver_id: i64,
) -> Result<Order, OrderError> {
    if voids.is_empty() {
        return Err(OrderError::NoLines);
    }
    let head = begin(tx, order_id, expected_version)?;
    require_approver(tx, approver_id)?;
    for &(line_id, qty) in voids {
        let line = load_line(tx, order_id, line_id)?;
        if line.status == LINE_VOIDED {
            return Err(OrderError::LineVoided);
        }
        if line.sent_at.is_none() {
            return Err(OrderError::LineNotSent);
        }
        if line.sale_id.is_some() {
            return Err(OrderError::LinePaid);
        }
        if !qty.is_finite() || qty <= 0.0 || qty > line.qty + QTY_EPSILON {
            return Err(OrderError::InvalidQuantity);
        }
        if (line.qty - qty).abs() <= QTY_EPSILON {
            tx.execute(
                "UPDATE order_items SET status = 'voided', voided_by = ?1, void_reason = ?2,
                        voided_at = datetime('now', 'localtime')
                  WHERE id = ?3",
                params![approver_id, reason, line_id],
            )?;
        } else {
            // Split: the original line keeps its id (and token link) with
            // what is left; a sibling row records the voided part.
            tx.execute("UPDATE order_items SET qty = qty - ?1 WHERE id = ?2", params![qty, line_id])?;
            tx.execute(
                "INSERT INTO order_items (order_id, item_id, qty, price_at_order_minor, notes, sent_at, status,
                                          voided_by, void_reason, voided_at, added_by, added_at)
                 SELECT order_id, item_id, ?1, price_at_order_minor, notes, sent_at, 'voided',
                        ?2, ?3, datetime('now', 'localtime'), added_by, added_at
                   FROM order_items WHERE id = ?4",
                params![qty, approver_id, reason, line_id],
            )?;
        }
        log_event(
            tx,
            order_id,
            Some(&head.status),
            &head.status,
            Some(approver_id),
            Some(&format!("void {qty} of line {line_id}: {reason}")),
        )?;
    }
    finish(tx, order_id)
}

/// Cancels the order. Free before anything was sent; afterwards it needs
/// an approver, voids every active line and cancels its tokens. Refused
/// while any line is paid and not fully refunded.
pub fn cancel(
    tx: &Transaction,
    order_id: i64,
    expected_version: i64,
    reason: &str,
    user_id: Option<i64>,
    approver_id: Option<i64>,
) -> Result<Order, OrderError> {
    let head = begin(tx, order_id, expected_version)?;
    
    // Check if there are any paid lines that haven't been fully refunded
    let has_unrefunded_paid_lines: bool = tx.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM order_items oi
            WHERE oi.order_id = ?1 
              AND oi.status = 'active' 
              AND oi.sale_id IS NOT NULL
              AND (
                -- Either the sale_item doesn't exist (shouldn't happen, but safe)
                NOT EXISTS(SELECT 1 FROM sale_items si WHERE si.sale_id = oi.sale_id AND si.item_id = oi.item_id)
                -- Or the line hasn't been fully refunded
                OR EXISTS(
                    SELECT 1 FROM sale_items si
                    WHERE si.sale_id = oi.sale_id 
                      AND si.item_id = oi.item_id
                      AND si.qty > COALESCE(
                          (SELECT SUM(ri.qty_refunded) 
                           FROM refund_items ri 
                           WHERE ri.sale_item_id = si.id), 
                          0
                      )
                )
              )
        )",
        params![order_id],
        |row| row.get(0),
    )?;
    
    if has_unrefunded_paid_lines {
        return Err(OrderError::HasPaidLines);
    }
    let sent: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM order_items WHERE order_id = ?1 AND sent_at IS NOT NULL)",
        params![order_id],
        |row| row.get(0),
    )?;
    if sent && approver_id.is_none() {
        return Err(OrderError::ApprovalRequired);
    }
    if let Some(approver) = approver_id {
        require_approver(tx, approver)?;
    }
    let by = approver_id.or(user_id);
    set_status(tx, order_id, &head.status, CANCELLED, by)?;
    tx.execute(
        "UPDATE order_items SET status = 'voided', voided_by = ?1, void_reason = ?2,
                voided_at = datetime('now', 'localtime')
          WHERE order_id = ?3 AND status = 'active'",
        params![by, reason, order_id],
    )?;
    tx.execute("UPDATE tokens SET status = 'cancelled' WHERE order_id = ?1", params![order_id])?;
    tx.execute(
        "UPDATE orders SET cancel_reason = ?1, held_from_status = NULL WHERE id = ?2",
        params![reason, order_id],
    )?;
    if let Some(table_id) = head.table_id {
        free_table_if_idle(tx, table_id)?;
    }
    get_order(tx, order_id)
}

/// (item id, qty) of the given unpaid lines of an order — the stock paying
/// them will take (see `commands::orders_pay`). Lines that are not
/// payable are skipped here; `pay_lines` refuses them with a reason.
pub fn stock_lines(conn: &Connection, order_id: i64, order_item_ids: &[i64]) -> Result<Vec<(i64, f64)>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT item_id, qty FROM order_items WHERE id = ?1 AND order_id = ?2 AND status <> ?3 AND sale_id IS NULL",
    )?;
    let mut ids = order_item_ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    let mut lines = Vec::new();
    for id in ids {
        if let Some(line) = stmt.query_row(params![id, order_id, LINE_VOIDED], |row| Ok((row.get(0)?, row.get(1)?))).optional()? {
            lines.push(line);
        }
    }
    Ok(lines)
}

/// One sale for the given active, unpaid lines at their order prices.
pub fn pay_lines(tx: &Transaction, order_id: i64, expected_version: i64, input: PayInput) -> Result<Paid, OrderError> {
    let mut ids = input.order_item_ids.clone();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Err(OrderError::NoLines);
    }
    let head = begin(tx, order_id, expected_version)?;
    let mut lines = Vec::with_capacity(ids.len());
    for &line_id in &ids {
        let line = load_line(tx, order_id, line_id)?;
        if line.status == LINE_VOIDED {
            return Err(OrderError::LineVoided);
        }
        if line.sale_id.is_some() {
            return Err(OrderError::LinePaid);
        }
        lines.push(PricedLine {
            item_id: line.item_id,
            qty: line.qty,
            price_minor: line.price_at_order_minor,
            notes: line.notes,
        });
    }
    let sale = sales::insert_priced_sale(
        tx,
        PricedSale {
            lines: &lines,
            discount_minor: input.discount_minor,
            tax_minor: input.tax_minor,
            payment_method: &input.payment_method,
            cashier_id: input.cashier_id,
            table_id: head.table_id,
            shift_id: input.shift_id,
            order_id: Some(order_id),
            tendered_minor: input.tendered_minor,
            change_minor: input.change_minor,
            payments: &input.payments,
            customer_id: input.customer_id,
            service_charge_minor: input.service_charge_minor,
            due_date: input.due_date.as_deref(),
        },
    )?;
    for &line_id in &ids {
        tx.execute("UPDATE order_items SET sale_id = ?1 WHERE id = ?2", params![sale.id, line_id])?;
    }
    Ok(Paid { order: finish(tx, order_id)?, sale })
}

pub fn update_prep_time(
    tx: &Transaction,
    order_id: i64,
    prep_minutes: i64,
) -> Result<Order, OrderError> {
    if prep_minutes <= 0 {
        return Err(OrderError::InvalidQuantity);
    }
    tx.execute(
        "UPDATE orders SET estimated_prep_minutes = ?1 WHERE id = ?2",
        params![prep_minutes, order_id],
    )?;
    get_order(tx, order_id)
}

pub fn list_active_orders(conn: &Connection) -> Result<Vec<Order>, OrderError> {
    let mut stmt = conn.prepare(
        "SELECT id FROM orders
          WHERE status != 'cancelled' AND completed_at IS NULL
          ORDER BY CASE WHEN status = 'held' THEN 1 ELSE 0 END, created_at ASC",
    )?;
    let order_ids: Vec<i64> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;

    let mut orders = Vec::with_capacity(order_ids.len());
    for id in order_ids {
        orders.push(get_order(conn, id)?);
    }
    Ok(orders)
}


// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

/// Two quantities closer than this are the same (qty is REAL).
const QTY_EPSILON: f64 = 1e-9;

struct Head {
    status: String,
    held_from_status: Option<String>,
    version: i64,
    table_id: Option<i64>,
    shift_id: Option<i64>,
    completed_at: Option<String>,
}

fn load_head(conn: &Connection, order_id: i64) -> Result<Head, OrderError> {
    conn.query_row(
        "SELECT status, held_from_status, version, table_id, completed_at, shift_id FROM orders WHERE id = ?1",
        params![order_id],
        |row| {
            Ok(Head {
                status: row.get(0)?,
                held_from_status: row.get(1)?,
                version: row.get(2)?,
                table_id: row.get(3)?,
                completed_at: row.get(4)?,
                shift_id: row.get(5)?,
            })
        },
    )
    .optional()?
    .ok_or(OrderError::NotFound)
}

/// The start of every versioned write: refuses a closed order or a stale
/// version, then bumps the version (rolled back with the rest on error).
fn begin(tx: &Transaction, order_id: i64, expected_version: i64) -> Result<Head, OrderError> {
    let head = load_head(tx, order_id)?;
    if head.status == CANCELLED || head.completed_at.is_some() {
        return Err(OrderError::Closed);
    }
    if head.version != expected_version {
        return Err(OrderError::Stale { current: Box::new(get_order(tx, order_id)?) });
    }
    touch(tx, order_id)?;
    Ok(head)
}

/// Voids and late cancels need an active Owner or Admin as the approver.
fn require_approver(conn: &Connection, approver_id: i64) -> Result<(), OrderError> {
    let allowed: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM users WHERE id = ?1 AND is_active = 1 AND role IN ('owner', 'admin'))",
        params![approver_id],
        |row| row.get(0),
    )?;
    if allowed { Ok(()) } else { Err(OrderError::ApproverNotAllowed) }
}

fn touch(tx: &Transaction, order_id: i64) -> Result<(), OrderError> {
    tx.execute("UPDATE orders SET version = version + 1 WHERE id = ?1", params![order_id])?;
    Ok(())
}

fn refuse_if_held(head: &Head, to: &str) -> Result<(), OrderError> {
    if head.status == HELD {
        return Err(OrderError::BadTransition { from: HELD.into(), to: to.into() });
    }
    Ok(())
}

/// Moves `from` -> `to` along `can_transition`, stamping the milestone
/// column (a resume restores the status without re-stamping it) and
/// logging the event.
fn set_status(tx: &Transaction, order_id: i64, from: &str, to: &str, user_id: Option<i64>) -> Result<(), OrderError> {
    if !can_transition(from, to) {
        return Err(OrderError::BadTransition { from: from.into(), to: to.into() });
    }
    let stamp = from != HELD;
    tx.execute(
        "UPDATE orders SET status = ?1,
                sent_at      = CASE WHEN ?2 AND ?1 = 'sent'      THEN COALESCE(sent_at, datetime('now', 'localtime')) ELSE sent_at END,
                ready_at     = CASE WHEN ?1 = 'ready'            THEN datetime('now', 'localtime') ELSE ready_at END,
                served_at    = CASE WHEN ?1 = 'served'           THEN datetime('now', 'localtime') ELSE served_at END,
                cancelled_at = CASE WHEN ?1 = 'cancelled'        THEN datetime('now', 'localtime') ELSE cancelled_at END
          WHERE id = ?3",
        params![to, stamp, order_id],
    )?;
    log_event(tx, order_id, Some(from), to, user_id, None)
}

fn log_event(
    tx: &Transaction,
    order_id: i64,
    from: Option<&str>,
    to: &str,
    user_id: Option<i64>,
    detail: Option<&str>,
) -> Result<(), OrderError> {
    tx.execute(
        "INSERT INTO order_status_events (order_id, from_status, to_status, user_id, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![order_id, from, to, user_id, detail],
    )?;
    Ok(())
}

fn load_line(conn: &Connection, order_id: i64, line_id: i64) -> Result<OrderLine, OrderError> {
    get_order(conn, order_id)?
        .lines
        .into_iter()
        .find(|l| l.id == line_id)
        .ok_or(OrderError::LineNotFound(line_id))
}

/// The end of every write: stamps completion once the order is served and
/// every active line is paid, and frees the table as soon as payment is complete.
fn finish(tx: &Transaction, order_id: i64) -> Result<Order, OrderError> {
    let order = get_order(tx, order_id)?;
    let has_active = order.lines.iter().any(|l| l.status == LINE_ACTIVE);
    let is_fully_paid = order.payment_status == "paid";
    let mut needs_reload = false;
    
    // Free the table as soon as the order is fully paid, regardless of served status
    // This allows immediate table turnover for paid orders
    if is_fully_paid && order.table_id.is_some() && order.completed_at.is_none() {
        if let Some(table_id) = order.table_id {
            tx.execute("UPDATE tables SET status = 'free' WHERE id = ?1", params![table_id])?;
        }
        needs_reload = true;
    }
    
    // Complete the order if it's served and fully paid, OR if it's a delivery order delivered and fully paid
    let delivery_order = is_delivery(&order.order_type);
    let is_delivered = order.delivery_status.as_deref() == Some(DELIVERY_DELIVERED);
    let should_complete = is_fully_paid
        && order.completed_at.is_none()
        && has_active
        && (order.status == SERVED || (delivery_order && is_delivered));

    if should_complete {
        tx.execute(
            "UPDATE orders SET status = 'served', completed_at = datetime('now', 'localtime'), served_at = COALESCE(served_at, datetime('now', 'localtime')) WHERE id = ?1",
            params![order_id],
        )?;
        if let Some(table_id) = order.table_id {
            free_table_if_idle(tx, table_id)?;
        }
        needs_reload = true;
    }
    
    // Re-read if we made any changes
    if needs_reload {
        return get_order(tx, order_id);
    }
    
    Ok(order)
}

/// Frees a table once nothing open is left on it — neither another order
/// nor a legacy `table_orders` parked cart.
fn free_table_if_idle(tx: &Transaction, table_id: i64) -> Result<(), OrderError> {
    let busy: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM orders
                        WHERE table_id = ?1 AND status <> 'cancelled' AND completed_at IS NULL)
             OR EXISTS(SELECT 1 FROM table_orders WHERE table_id = ?1 AND status = 'open')",
        params![table_id],
        |row| row.get(0),
    )?;
    if !busy {
        tx.execute("UPDATE tables SET status = 'free' WHERE id = ?1", params![table_id])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::test_conn;
    use crate::db::test_support::{count, in_tx as run, item_id, owner_id, table_id};

    fn pay_input(order_item_ids: Vec<i64>, discount_minor: i64, tax_minor: i64) -> PayInput {
        PayInput {
            order_item_ids,
            payment_method: "cash".into(),
            discount_minor,
            tax_minor,
            cashier_id: None,
            shift_id: None,
            tendered_minor: None,
            change_minor: None,
            ..Default::default()
        }
    }

    fn token_for_counter(conn: &Connection, order_id: i64, counter_name: &str) -> i64 {
        conn.query_row(
            "SELECT t.id FROM tokens t JOIN counters c ON c.id = t.counter_id WHERE t.order_id = ?1 AND c.name = ?2",
            params![order_id, counter_name],
            |row| row.get(0),
        )
        .unwrap()
    }

    fn price(conn: &Connection, item: &str) -> i64 {
        conn.query_row("SELECT price_minor FROM items WHERE name = ?1", [item], |row| row.get(0)).unwrap()
    }

    /// Puts Cola on a new "Bar" counter so it lands on a token.
    fn route_cola(conn: &Connection) -> i64 {
        conn.execute("INSERT INTO counters (name) VALUES ('Bar')", []).unwrap();
        let counter = conn.last_insert_rowid();
        conn.execute("UPDATE items SET counter_id = ?1 WHERE name = 'Cola 500ml'", [counter]).unwrap();
        counter
    }

    fn takeaway(conn: &mut Connection, client_ref: &str) -> Order {
        run(conn, |tx| create_order(tx, client_ref, "takeaway", None, None, None, None, None, None, None)).unwrap()
    }

    fn with_cola(conn: &mut Connection, order: &Order, qty: f64) -> Order {
        let cola = item_id(conn, "Cola 500ml");
        run(conn, |tx| add_lines(tx, order.id, order.version, &[NewLine { item_id: cola, qty, notes: None }], None))
            .unwrap()
    }

    fn cola_order(conn: &mut Connection, client_ref: &str, qty: f64) -> Order {
        let order = takeaway(conn, client_ref);
        with_cola(conn, &order, qty)
    }

    fn send(conn: &mut Connection, order: &Order) -> Sent {
        run(conn, |tx| record_send(tx, order.id, order.version, None)).unwrap()
    }

    fn pay_all(conn: &mut Connection, order: &Order) -> Paid {
        let ids = order.lines.iter().filter(|l| l.status == "active" && l.sale_id.is_none()).map(|l| l.id).collect();
        run(conn, |tx| {
            pay_lines(
                tx,
                order.id,
                order.version,
                PayInput {
                    order_item_ids: ids,
                    payment_method: "cash".into(),
                    discount_minor: 0,
                    tax_minor: 0,
                    cashier_id: None,
                    shift_id: None,
                    tendered_minor: None,
                    change_minor: None,
                    ..Default::default()
                },
            )
        })
        .unwrap()
    }

    /// Drives a fresh takeaway order to `status` through the real
    /// operations (cancelled and held included).
    fn order_in(conn: &mut Connection, status: &str, client_ref: &str) -> Order {
        let order = cola_order(conn, client_ref, 1.0);
        let step = |conn: &mut Connection, o: &Order, to: &str| run(conn, |tx| transition(tx, o.id, o.version, to, None)).unwrap();
        match status {
            "draft" => order,
            "sent" => send(conn, &order).order,
            "preparing" => {
                let o = send(conn, &order).order;
                step(conn, &o, "preparing")
            }
            "ready" => {
                let o = send(conn, &order).order;
                step(conn, &o, "ready")
            }
            "served" => {
                let o = send(conn, &order).order;
                let o = step(conn, &o, "ready");
                step(conn, &o, "served")
            }
            "held" => run(conn, |tx| hold(tx, order.id, order.version, None)).unwrap(),
            "cancelled" => run(conn, |tx| cancel(tx, order.id, order.version, "test", None, None)).unwrap(),
            other => panic!("unknown status {other}"),
        }
    }

    // ---- the lifecycle table ------------------------------------------------

    #[test]
    fn can_transition_matches_the_lifecycle_table_exactly() {
        let allowed: &[(&str, &str)] = &[
            ("draft", "sent"),
            ("draft", "preparing"),
            ("draft", "ready"),
            ("draft", "served"),
            ("draft", "held"),
            ("draft", "cancelled"),
            ("sent", "preparing"),
            ("sent", "ready"),
            ("sent", "served"),
            ("sent", "held"),
            ("sent", "cancelled"),
            ("preparing", "ready"),
            ("preparing", "served"),
            ("preparing", "held"),
            ("preparing", "cancelled"),
            ("ready", "sent"),
            ("ready", "served"),
            ("ready", "held"),
            ("ready", "cancelled"),
            ("served", "sent"),
            ("served", "ready"),
            ("served", "held"),
            ("served", "cancelled"),
            ("held", "draft"),
            ("held", "sent"),
            ("held", "preparing"),
            ("held", "ready"),
            ("held", "served"),
            ("held", "cancelled"),
        ];
        for from in STATUSES {
            for to in STATUSES {
                let expected = allowed.contains(&(*from, *to));
                assert_eq!(can_transition(from, to), expected, "{from} -> {to}");
            }
        }
    }

    #[test]
    fn transition_refuses_every_manual_move_the_table_forbids_and_writes_nothing() {
        let mut conn = test_conn();
        for (n, from) in STATUSES.iter().enumerate() {
            let order = order_in(&mut conn, from, &format!("matrix-{n}"));
            assert_eq!(&order.status, from);
            for to in ["preparing", "ready", "served"] {
                if can_transition(from, to) {
                    continue;
                }
                let result = run(&mut conn, |tx| transition(tx, order.id, order.version, to, None));
                assert!(
                    matches!(result, Err(OrderError::BadTransition { .. }) | Err(OrderError::Closed)),
                    "{from} -> {to} was allowed: {result:?}"
                );
                assert_eq!(get_order(&conn, order.id).unwrap().version, order.version, "{from} -> {to} wrote");
            }
        }
    }

    #[test]
    fn every_status_change_is_logged_in_order() {
        let mut conn = test_conn();
        let order = order_in(&mut conn, "served", "log");
        let mut stmt = conn
            .prepare("SELECT COALESCE(from_status, '-'), to_status FROM order_status_events WHERE order_id = ?1 ORDER BY id")
            .unwrap();
        let events: Vec<(String, String)> =
            stmt.query_map([order.id], |row| Ok((row.get(0)?, row.get(1)?))).unwrap().map(Result::unwrap).collect();
        let events: Vec<(&str, &str)> = events.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        assert_eq!(events, [("-", "draft"), ("draft", "sent"), ("sent", "ready"), ("ready", "served")]);
    }

    // ---- create, dedupe, versions -------------------------------------------

    #[test]
    fn create_numbers_orders_daily_and_a_repeated_client_ref_returns_the_same_order() {
        let mut conn = test_conn();
        let first = takeaway(&mut conn, "ref-a");
        let second = takeaway(&mut conn, "ref-b");
        assert_eq!((first.order_no, second.order_no), (1, 2));
        assert_eq!((first.status.as_str(), first.version), ("draft", 1));

        let retried = takeaway(&mut conn, "ref-a");
        assert_eq!(retried.id, first.id);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM orders"), 2);
    }

    #[test]
    fn create_validates_type_and_table_and_seats_a_dine_in_table() {
        let mut conn = test_conn();
        let t2 = table_id(&conn, "Table 2");
        assert!(matches!(run(&mut conn, |tx| create_order(tx, "x1", "drive_thru", None, None, None, None, None, None, None)), Err(OrderError::InvalidOrderType(_))));
        assert!(matches!(run(&mut conn, |tx| create_order(tx, "x2", "dine_in", None, None, None, None, None, None, None)), Err(OrderError::TableRequired)));
        assert!(matches!(run(&mut conn, |tx| create_order(tx, "x3", "takeaway", Some(t2), None, None, None, None, None, None)), Err(OrderError::TableOnlyForDineIn)));
        assert!(matches!(run(&mut conn, |tx| create_order(tx, "x4", "dine_in", Some(999_999), None, None, None, None, None, None)), Err(OrderError::TableNotFound)));

        let order = run(&mut conn, |tx| create_order(tx, "x5", "dine_in", Some(t2), None, None, None, None, None, None)).unwrap();
        assert_eq!(order.table_id, Some(t2));
        let status: String = conn.query_row("SELECT status FROM tables WHERE id = ?1", [t2], |row| row.get(0)).unwrap();
        assert_eq!(status, "occupied");
    }

    #[test]
    fn a_stale_version_is_refused_and_nothing_is_written() {
        let mut conn = test_conn();
        let order = takeaway(&mut conn, "stale");
        let fresh = with_cola(&mut conn, &order, 1.0); // version 1 -> 2
        let cola = item_id(&conn, "Cola 500ml");

        let result = run(&mut conn, |tx| add_lines(tx, order.id, order.version, &[NewLine { item_id: cola, qty: 5.0, notes: None }], None));
        match result {
            Err(OrderError::Stale { current }) => assert_eq!(current.version, fresh.version),
            other => panic!("expected Stale, got {other:?}"),
        }
        assert_eq!(get_order(&conn, order.id).unwrap().lines.len(), 1);
    }

    // ---- lines and prices ---------------------------------------------------

    #[test]
    fn line_prices_are_copied_server_side_and_survive_a_later_price_change() {
        let mut conn = test_conn();
        let cola_price = price(&conn, "Cola 500ml");
        let order = cola_order(&mut conn, "price", 2.0);
        assert_eq!(order.lines[0].price_at_order_minor, cola_price);
        assert_eq!(order.due_minor, cola_price * 2);

        conn.execute("UPDATE items SET price_minor = price_minor * 10 WHERE name = 'Cola 500ml'", []).unwrap();
        let paid = pay_all(&mut conn, &order);
        assert_eq!(paid.sale.total_minor, cola_price * 2, "the quoted price is billed, not the new one");
    }

    #[test]
    fn add_lines_refuses_bad_quantities_unknown_and_archived_items() {
        let mut conn = test_conn();
        let order = takeaway(&mut conn, "bad-lines");
        let cola = item_id(&conn, "Cola 500ml");
        let add = |conn: &mut Connection, line: NewLine| run(conn, |tx| add_lines(tx, order.id, order.version, &[line], None));

        assert!(matches!(run(&mut conn, |tx| add_lines(tx, order.id, order.version, &[], None)), Err(OrderError::NoLines)));
        assert!(matches!(add(&mut conn, NewLine { item_id: cola, qty: 0.0, notes: None }), Err(OrderError::InvalidQuantity)));
        assert!(matches!(add(&mut conn, NewLine { item_id: cola, qty: f64::NAN, notes: None }), Err(OrderError::InvalidQuantity)));
        assert!(matches!(add(&mut conn, NewLine { item_id: 999_999, qty: 1.0, notes: None }), Err(OrderError::ItemNotFound(_))));
        conn.execute("UPDATE items SET is_active = 0 WHERE id = ?1", [cola]).unwrap();
        assert!(matches!(add(&mut conn, NewLine { item_id: cola, qty: 1.0, notes: None }), Err(OrderError::ItemInactive(_))));
    }

    #[test]
    fn unsent_lines_change_freely_but_sent_lines_must_be_voided() {
        let mut conn = test_conn();
        let order = cola_order(&mut conn, "change", 2.0);
        let line = order.lines[0].id;

        let order = run(&mut conn, |tx| change_unsent_line(tx, order.id, order.version, line, LineChange::Qty(3.0))).unwrap();
        assert_eq!(order.lines[0].qty, 3.0);

        let order = send(&mut conn, &order).order;
        let result = run(&mut conn, |tx| change_unsent_line(tx, order.id, order.version, line, LineChange::Remove));
        assert!(matches!(result, Err(OrderError::LineAlreadySent)));
    }

    // ---- send ---------------------------------------------------------------

    #[test]
    fn send_tokens_only_routed_unsent_lines_and_marks_every_line_sent() {
        let mut conn = test_conn();
        let bar = route_cola(&conn);
        let noodles = item_id(&conn, "Instant Noodles"); // no counter
        let order = cola_order(&mut conn, "send", 2.0);
        let order = run(&mut conn, |tx| add_lines(tx, order.id, order.version, &[NewLine { item_id: noodles, qty: 1.0, notes: None }], None)).unwrap();

        let preview = pending_send_groups(&conn, order.id).unwrap();
        assert_eq!(preview.version, order.version);
        assert_eq!(preview.groups.len(), 1);
        assert_eq!(preview.groups[0].counter_id, bar);
        assert_eq!(preview.groups[0].lines[0].qty, 2.0);
        assert_eq!(preview.unrouted.len(), 1);

        let sent = send(&mut conn, &order);
        assert_eq!(sent.order.status, "sent");
        assert_eq!(sent.token_ids.len(), 1);
        assert!(sent.order.lines.iter().all(|l| l.sent_at.is_some()));
        let linked: i64 = conn
            .query_row("SELECT COUNT(*) FROM token_items WHERE token_id = ?1 AND order_item_id IS NOT NULL", [sent.token_ids[0]], |row| row.get(0))
            .unwrap();
        assert_eq!(linked, 1);
        let order_link: Option<i64> = conn.query_row("SELECT order_id FROM tokens WHERE id = ?1", [sent.token_ids[0]], |row| row.get(0)).unwrap();
        assert_eq!(order_link, Some(order.id));

        assert!(matches!(run(&mut conn, |tx| record_send(tx, order.id, sent.order.version, None)), Err(OrderError::NothingToSend)));
    }

    #[test]
    fn a_second_round_on_a_served_order_goes_back_to_sent_and_tokens_only_the_new_lines() {
        let mut conn = test_conn();
        route_cola(&conn);
        let served = order_in(&mut conn, "served", "round2");
        let more = with_cola(&mut conn, &served, 3.0);
        assert_eq!(more.status, "served", "adding alone does not reopen the order");

        let preview = pending_send_groups(&conn, more.id).unwrap();
        assert_eq!(preview.groups[0].lines.len(), 1);
        assert_eq!(preview.groups[0].lines[0].qty, 3.0);

        let sent = send(&mut conn, &more);
        assert_eq!(sent.order.status, "sent");
    }

    #[test]
    fn a_preparing_order_keeps_its_status_when_more_is_sent() {
        let mut conn = test_conn();
        let order = order_in(&mut conn, "preparing", "prep-more");
        let order = with_cola(&mut conn, &order, 1.0);
        assert_eq!(send(&mut conn, &order).order.status, "preparing");
    }

    // ---- kitchen bumps ------------------------------------------------------

    #[test]
    fn bumping_tokens_moves_the_order_to_preparing_then_ready_once_every_token_is_ready() {
        let mut conn = test_conn();
        route_cola(&conn);
        conn.execute("INSERT INTO counters (name) VALUES ('Grill')", []).unwrap();
        let grill = conn.last_insert_rowid();
        conn.execute("UPDATE items SET counter_id = ?1 WHERE name = 'Instant Noodles'", [grill]).unwrap();
        let noodles = item_id(&conn, "Instant Noodles");

        let order = cola_order(&mut conn, "bump", 1.0);
        let order = run(&mut conn, |tx| add_lines(tx, order.id, order.version, &[NewLine { item_id: noodles, qty: 1.0, notes: None }], None)).unwrap();
        send(&mut conn, &order);
        let (bar_token, grill_token) = (token_for_counter(&conn, order.id, "Bar"), token_for_counter(&conn, order.id, "Grill"));

        let order = run(&mut conn, |tx| bump_token(tx, bar_token, "preparing", None)).unwrap();
        assert_eq!(order.status, "preparing");
        let order = run(&mut conn, |tx| bump_token(tx, bar_token, "ready", None)).unwrap();
        assert_eq!(order.status, "preparing", "Grill is not ready yet");
        let repeat = run(&mut conn, |tx| bump_token(tx, bar_token, "ready", None)).unwrap();
        assert_eq!(repeat.version, order.version, "a repeated bump changes nothing");
        assert!(matches!(run(&mut conn, |tx| bump_token(tx, bar_token, "preparing", None)), Err(OrderError::BadTransition { .. })));

        let order = run(&mut conn, |tx| bump_token(tx, grill_token, "ready", None)).unwrap();
        assert_eq!(order.status, "ready");
    }

    #[test]
    fn manual_ready_marks_every_token_ready_for_a_station_without_a_screen() {
        let mut conn = test_conn();
        route_cola(&conn);
        let order = order_in(&mut conn, "ready", "manual-ready");
        assert_eq!(order.status, "ready");
        assert_eq!(count(&conn, &format!("SELECT COUNT(*) FROM tokens WHERE order_id = {} AND kitchen_status <> 'ready'", order.id)), 0);
    }

    // ---- hold / resume ------------------------------------------------------

    #[test]
    fn hold_and_resume_return_to_the_status_the_order_was_held_from() {
        let mut conn = test_conn();
        for from in ["draft", "sent", "preparing", "ready", "served"] {
            let order = order_in(&mut conn, from, &format!("hold-{from}"));
            let held = run(&mut conn, |tx| hold(tx, order.id, order.version, None)).unwrap();
            assert_eq!(held.status, "held");
            assert_eq!(held.held_from_status.as_deref(), Some(from));

            let cola = item_id(&conn, "Cola 500ml");
            let blocked = run(&mut conn, |tx| add_lines(tx, held.id, held.version, &[NewLine { item_id: cola, qty: 1.0, notes: None }], None));
            assert!(matches!(blocked, Err(OrderError::BadTransition { .. })), "added to a held order");

            let resumed = run(&mut conn, |tx| resume(tx, held.id, held.version, None)).unwrap();
            assert_eq!(resumed.status, from);
            assert_eq!(resumed.held_from_status, None);
        }
        let draft = order_in(&mut conn, "draft", "resume-unheld");
        assert!(matches!(run(&mut conn, |tx| resume(tx, draft.id, draft.version, None)), Err(OrderError::BadTransition { .. })));
    }

    #[test]
    fn held_takeaway_order_can_transition_to_ready_and_complete_on_payment() {
        let mut conn = test_conn();
        let order = cola_order(&mut conn, "held-to-ready", 2.0);
        let held = run(&mut conn, |tx| hold(tx, order.id, order.version, None)).unwrap();
        assert_eq!(held.status, "held");

        // Staff clicks "Make Ready" directly on the held order
        let ready = run(&mut conn, |tx| transition(tx, held.id, held.version, "ready", None)).unwrap();
        assert_eq!(ready.status, "ready");
        let ready_at: Option<String> = conn.query_row("SELECT ready_at FROM orders WHERE id = ?1", [ready.id], |r| r.get(0)).unwrap();
        assert!(ready_at.is_some());
        assert!(ready.completed_at.is_none(), "ready but unpaid");

        // Active orders list includes this ready order
        let active = list_active_orders(&conn).unwrap();
        assert!(active.iter().any(|o| o.id == ready.id));

        // Customer arrives and pays
        let paid = pay_all(&mut conn, &ready);
        assert_eq!(paid.order.payment_status, "paid");
        assert_eq!(paid.order.status, "ready");
        assert!(paid.order.completed_at.is_none(), "paid but not yet served");

        // Order is handed over (served)
        let served = run(&mut conn, |tx| transition(tx, paid.order.id, paid.order.version, "served", None)).unwrap();
        assert_eq!(served.status, "served");
        assert!(served.completed_at.is_some(), "served and paid -> completed");

        // Active orders list no longer includes the completed order
        let active_after = list_active_orders(&conn).unwrap();
        assert!(!active_after.iter().any(|o| o.id == ready.id));
    }

    // ---- void / cancel ------------------------------------------------------

    #[test]
    fn void_needs_a_sent_unpaid_line_and_a_partial_qty_splits_it() {
        let mut conn = test_conn();
        let owner = owner_id(&conn);
        let draft = cola_order(&mut conn, "void", 3.0);
        let line = draft.lines[0].id;
        assert!(matches!(
            run(&mut conn, |tx| void_lines(tx, draft.id, draft.version, &[(line, 1.0)], "no", owner)),
            Err(OrderError::LineNotSent)
        ));

        let order = send(&mut conn, &draft).order;
        assert!(matches!(
            run(&mut conn, |tx| void_lines(tx, order.id, order.version, &[(line, 4.0)], "too many", owner)),
            Err(OrderError::InvalidQuantity)
        ));
        let order = run(&mut conn, |tx| void_lines(tx, order.id, order.version, &[(line, 1.0)], "dropped", owner)).unwrap();
        let active: Vec<_> = order.lines.iter().filter(|l| l.status == "active").collect();
        let voided: Vec<_> = order.lines.iter().filter(|l| l.status == "voided").collect();
        assert_eq!((active.len(), active[0].qty), (1, 2.0));
        assert_eq!((voided.len(), voided[0].qty), (1, 1.0));
        assert_eq!(order.due_minor, price(&conn, "Cola 500ml") * 2);

        let order = pay_all(&mut conn, &order).order;
        assert!(matches!(
            run(&mut conn, |tx| void_lines(tx, order.id, order.version, &[(line, 1.0)], "late", owner)),
            Err(OrderError::LinePaid)
        ));
    }

    #[test]
    fn cancel_is_free_before_send_needs_approval_after_and_is_refused_once_paid() {
        let mut conn = test_conn();
        let owner = owner_id(&conn);
        let t2 = table_id(&conn, "Table 2");

        let draft = order_in(&mut conn, "draft", "cancel-draft");
        let cancelled = run(&mut conn, |tx| cancel(tx, draft.id, draft.version, "changed mind", None, None)).unwrap();
        assert_eq!(cancelled.status, "cancelled");
        assert!(cancelled.cancelled_at.is_some());

        route_cola(&conn);
        let dine = run(&mut conn, |tx| create_order(tx, "cancel-sent", "dine_in", Some(t2), None, None, None, None, None, None)).unwrap();
        let dine = with_cola(&mut conn, &dine, 1.0);
        let sent = send(&mut conn, &dine).order;
        assert!(matches!(run(&mut conn, |tx| cancel(tx, sent.id, sent.version, "walked out", None, None)), Err(OrderError::ApprovalRequired)));
        let cancelled = run(&mut conn, |tx| cancel(tx, sent.id, sent.version, "walked out", None, Some(owner))).unwrap();
        assert!(cancelled.lines.iter().all(|l| l.status == "voided"));
        assert_eq!(count(&conn, &format!("SELECT COUNT(*) FROM tokens WHERE order_id = {} AND status <> 'cancelled'", sent.id)), 0);
        let table: String = conn.query_row("SELECT status FROM tables WHERE id = ?1", [t2], |row| row.get(0)).unwrap();
        assert_eq!(table, "free");
        assert!(matches!(run(&mut conn, |tx| hold(tx, cancelled.id, cancelled.version, None)), Err(OrderError::Closed)));

        let to_pay = order_in(&mut conn, "sent", "cancel-paid");
        let paid = pay_all(&mut conn, &to_pay).order;
        assert!(matches!(run(&mut conn, |tx| cancel(tx, paid.id, paid.version, "x", None, Some(owner))), Err(OrderError::HasPaidLines)));
    }

    // ---- pay and complete ---------------------------------------------------

    #[test]
    fn pay_first_takeaway_completes_when_served() {
        let mut conn = test_conn();
        let order = cola_order(&mut conn, "pay-first", 1.0);
        let paid = pay_all(&mut conn, &order);
        assert_eq!(paid.order.payment_status, "paid");
        assert_eq!(paid.order.status, "draft");
        assert!(paid.order.completed_at.is_none(), "paid but not served");
        let sale_order: Option<i64> = conn.query_row("SELECT order_id FROM sales WHERE id = ?1", [paid.sale.id], |row| row.get(0)).unwrap();
        assert_eq!(sale_order, Some(order.id));

        let o = send(&mut conn, &paid.order).order;
        let o = run(&mut conn, |tx| transition(tx, o.id, o.version, "ready", None)).unwrap();
        let o = run(&mut conn, |tx| transition(tx, o.id, o.version, "served", None)).unwrap();
        assert!(o.completed_at.is_some());
        let cola = item_id(&conn, "Cola 500ml");
        assert!(matches!(
            run(&mut conn, |tx| add_lines(tx, o.id, o.version, &[NewLine { item_id: cola, qty: 1.0, notes: None }], None)),
            Err(OrderError::Closed)
        ));
    }

    #[test]
    fn pay_later_dine_in_splits_the_bill_and_completes_and_frees_the_table_on_the_last_payment() {
        let mut conn = test_conn();
        let t2 = table_id(&conn, "Table 2");
        let noodles = item_id(&conn, "Instant Noodles");
        let dine = run(&mut conn, |tx| create_order(tx, "pay-later", "dine_in", Some(t2), None, None, None, None, None, None)).unwrap();
        let o = with_cola(&mut conn, &dine, 1.0);
        let o = run(&mut conn, |tx| add_lines(tx, o.id, o.version, &[NewLine { item_id: noodles, qty: 1.0, notes: None }], None)).unwrap();
        let o = send(&mut conn, &o).order;
        let o = run(&mut conn, |tx| transition(tx, o.id, o.version, "ready", None)).unwrap();
        let o = run(&mut conn, |tx| transition(tx, o.id, o.version, "served", None)).unwrap();
        assert_eq!(o.payment_status, "unpaid");

        let guest_a = PayInput {
            order_item_ids: vec![o.lines[0].id],
            payment_method: "card".into(),
            discount_minor: 0,
            tax_minor: 0,
            cashier_id: None,
            shift_id: None,
            tendered_minor: None,
            change_minor: None,
            ..Default::default()
        };
        let a = run(&mut conn, |tx| pay_lines(tx, o.id, o.version, guest_a.clone())).unwrap();
        assert_eq!(a.order.payment_status, "partial");
        assert_eq!(a.order.due_minor, price(&conn, "Instant Noodles"));
        assert!(a.order.completed_at.is_none());
        assert_eq!(a.sale.table_id, Some(t2), "table-wise reports still see the table");
        assert!(matches!(run(&mut conn, |tx| pay_lines(tx, o.id, a.order.version, guest_a.clone())), Err(OrderError::LinePaid)));

        let b = pay_all(&mut conn, &a.order);
        assert_eq!(b.order.payment_status, "paid");
        assert!(b.order.completed_at.is_some());
        assert_eq!(count(&conn, &format!("SELECT COUNT(*) FROM sales WHERE order_id = {}", o.id)), 2);
        let table: String = conn.query_row("SELECT status FROM tables WHERE id = ?1", [t2], |row| row.get(0)).unwrap();
        assert_eq!(table, "free");
    }

    #[test]
    fn a_failed_payment_rolls_back_and_leaves_the_lines_unpaid() {
        let mut conn = test_conn();
        let order = cola_order(&mut conn, "pay-fail", 1.0);
        let result = run(&mut conn, |tx| {
            pay_lines(
                tx,
                order.id,
                order.version,
                PayInput {
                    order_item_ids: vec![order.lines[0].id],
                    payment_method: "bitcoin".into(),
                    discount_minor: 0,
                    tax_minor: 0,
                    cashier_id: None,
                    shift_id: None,
                    tendered_minor: None,
                    change_minor: None,
                    ..Default::default()
                },
            )
        });
        assert!(matches!(result, Err(OrderError::Sale(SaleError::InvalidPaymentMethod(_)))));
        let after = get_order(&conn, order.id).unwrap();
        assert_eq!((after.payment_status.as_str(), after.version), ("unpaid", order.version));
    }

    // ---- audit follow-ups: money, cross-order lines, numbering -----------------

    #[test]
    fn a_discounted_and_taxed_split_bill_charges_each_guest_correctly() {
        let mut conn = test_conn();
        let noodles = item_id(&conn, "Instant Noodles");
        let (cola_price, noodles_price) = (price(&conn, "Cola 500ml"), price(&conn, "Instant Noodles"));
        let o = cola_order(&mut conn, "split-money", 1.0);
        let o = run(&mut conn, |tx| add_lines(tx, o.id, o.version, &[NewLine { item_id: noodles, qty: 1.0, notes: None }], None)).unwrap();
        let order_total = o.due_minor;

        let a = run(&mut conn, |tx| pay_lines(tx, o.id, o.version, pay_input(vec![o.lines[0].id], 500, 120))).unwrap();
        assert_eq!(a.sale.total_minor, cola_price - 500 + 120);
        assert_eq!(a.order.due_minor, noodles_price, "the other guest still owes their full line");

        let b = run(&mut conn, |tx| pay_lines(tx, o.id, a.order.version, pay_input(vec![o.lines[1].id], 0, 0))).unwrap();
        assert_eq!(b.sale.total_minor, noodles_price);
        // Undiscounted, untaxed parts of the split add back up to the order total.
        assert_eq!((a.sale.total_minor + 500 - 120) + b.sale.total_minor, order_total);
    }

    #[test]
    fn a_discount_bigger_than_the_selected_lines_is_refused_and_nothing_is_paid() {
        let mut conn = test_conn();
        let o = cola_order(&mut conn, "over-discount", 1.0);
        let too_much = o.due_minor + 1;
        let result = run(&mut conn, |tx| pay_lines(tx, o.id, o.version, pay_input(vec![o.lines[0].id], too_much, 0)));
        assert!(matches!(result, Err(OrderError::Sale(SaleError::InvalidDiscount))));
        let after = get_order(&conn, o.id).unwrap();
        assert_eq!((after.payment_status.as_str(), after.version), ("unpaid", o.version));
    }

    #[test]
    fn a_line_from_another_order_is_never_found() {
        let mut conn = test_conn();
        let owner = owner_id(&conn);
        let mine = cola_order(&mut conn, "mine", 1.0);
        let theirs = cola_order(&mut conn, "theirs", 1.0);
        let theirs = send(&mut conn, &theirs).order;
        let foreign = theirs.lines[0].id;

        assert!(matches!(
            run(&mut conn, |tx| change_unsent_line(tx, mine.id, mine.version, foreign, LineChange::Remove)),
            Err(OrderError::LineNotFound(_))
        ));
        assert!(matches!(
            run(&mut conn, |tx| pay_lines(tx, mine.id, mine.version, pay_input(vec![foreign], 0, 0))),
            Err(OrderError::LineNotFound(_))
        ));
        let mine = send(&mut conn, &mine).order;
        assert!(matches!(
            run(&mut conn, |tx| void_lines(tx, mine.id, mine.version, &[(foreign, 1.0)], "x", owner)),
            Err(OrderError::LineNotFound(_))
        ));
        assert_eq!(get_order(&conn, theirs.id).unwrap().version, theirs.version, "the other order was not touched");
    }

    #[test]
    fn paying_a_voided_line_or_no_lines_is_refused() {
        let mut conn = test_conn();
        let owner = owner_id(&conn);
        let o = cola_order(&mut conn, "pay-voided", 1.0);
        let o = send(&mut conn, &o).order;
        let line = o.lines[0].id;
        let o = run(&mut conn, |tx| void_lines(tx, o.id, o.version, &[(line, 1.0)], "dropped", owner)).unwrap();

        assert!(matches!(run(&mut conn, |tx| pay_lines(tx, o.id, o.version, pay_input(vec![line], 0, 0))), Err(OrderError::LineVoided)));
        assert!(matches!(run(&mut conn, |tx| pay_lines(tx, o.id, o.version, pay_input(vec![], 0, 0))), Err(OrderError::NoLines)));
    }

    #[test]
    fn order_numbers_restart_at_one_each_day() {
        let mut conn = test_conn();
        conn.execute(
            "INSERT INTO orders (order_no, order_type, client_ref, created_at)
             VALUES (41, 'takeaway', 'yesterday', datetime('now', 'localtime', '-1 day'))",
            [],
        )
        .unwrap();
        assert_eq!(takeaway(&mut conn, "today").order_no, 1);
    }

    #[test]
    fn order_numbers_restart_at_one_for_each_shift() {
        let mut conn = test_conn();
        conn.execute("INSERT INTO shifts (cashier_id, opening_balance_minor) VALUES (NULL, 0)", []).unwrap();
        let shift1 = conn.last_insert_rowid();

        let shift1_order1 = run(&mut conn, |tx| create_order(tx, "s1-1", "takeaway", None, None, None, None, None, Some(shift1), None)).unwrap();
        let shift1_order2 = run(&mut conn, |tx| create_order(tx, "s1-2", "takeaway", None, None, None, None, None, Some(shift1), None)).unwrap();
        assert_eq!(shift1_order1.order_no, 1);
        assert_eq!(shift1_order2.order_no, 2);

        // Next Shift (Shift 2) starts — order number restarts at 1!
        conn.execute("INSERT INTO shifts (cashier_id, opening_balance_minor) VALUES (NULL, 0)", []).unwrap();
        let shift2 = conn.last_insert_rowid();

        let shift2_order1 = run(&mut conn, |tx| create_order(tx, "s2-1", "takeaway", None, None, None, None, None, Some(shift2), None)).unwrap();
        let shift2_order2 = run(&mut conn, |tx| create_order(tx, "s2-2", "takeaway", None, None, None, None, None, Some(shift2), None)).unwrap();
        assert_eq!(shift2_order1.order_no, 1);
        assert_eq!(shift2_order2.order_no, 2);
    }

    #[test]
    fn only_an_active_owner_or_admin_can_approve_a_void_or_late_cancel() {
        let mut conn = test_conn();
        let pin = crate::db::users::hash_pin("1111").unwrap();
        conn.execute("INSERT INTO users (name, pin_hash, role) VALUES ('Cash', ?1, 'cashier')", [&pin]).unwrap();
        let cashier = conn.last_insert_rowid();
        conn.execute("INSERT INTO users (name, pin_hash, role, is_active) VALUES ('Gone', ?1, 'admin', 0)", [&pin]).unwrap();
        let inactive_admin = conn.last_insert_rowid();

        let o = cola_order(&mut conn, "approver", 2.0);
        let o = send(&mut conn, &o).order;
        let line = o.lines[0].id;
        for approver in [cashier, inactive_admin, 999_999] {
            assert!(matches!(
                run(&mut conn, |tx| void_lines(tx, o.id, o.version, &[(line, 1.0)], "x", approver)),
                Err(OrderError::ApproverNotAllowed)
            ));
            assert!(matches!(
                run(&mut conn, |tx| cancel(tx, o.id, o.version, "x", None, Some(approver))),
                Err(OrderError::ApproverNotAllowed)
            ));
        }
        assert_eq!(get_order(&conn, o.id).unwrap().version, o.version);
    }

    #[test]
    fn a_fully_refunded_order_can_be_cancelled() {
        use crate::db::refunds::{self, CreateRefundInput, RefundLineInput};
        
        let mut conn = test_conn();
        let owner = owner_id(&conn);
        
        // Create and pay an order
        let order = cola_order(&mut conn, "refund-cancel", 2.0);
        let order = send(&mut conn, &order).order;
        let paid = pay_all(&mut conn, &order);
        
        // Cannot cancel while paid
        assert!(matches!(
            run(&mut conn, |tx| cancel(tx, paid.order.id, paid.order.version, "test", None, Some(owner))),
            Err(OrderError::HasPaidLines)
        ));
        
        // Get the sale item and price before the refund transaction
        let line = conn.query_row(
            "SELECT id FROM sale_items WHERE sale_id = ?1", 
            [paid.sale.id], 
            |row| row.get::<_, i64>(0)
        ).unwrap();
        let cola_price = price(&conn, "Cola 500ml");
        
        // Refund the entire sale
        run(&mut conn, |tx| {
            refunds::create_refund(tx, CreateRefundInput {
                sale_id: paid.sale.id,
                items: vec![RefundLineInput {
                    sale_item_id: line,
                    qty: 2.0,
                    amount_minor: cola_price * 2,
                }],
                reason: Some("Customer complaint".into()),
                refunded_by: Some(owner),
            })
        }).unwrap();
        
        // Now the order should be cancellable since it's fully refunded
        let cancelled = run(&mut conn, |tx| cancel(
            tx, 
            paid.order.id, 
            paid.order.version, 
            "Refunded and cancelled", 
            None, 
            Some(owner)
        )).unwrap();
        
        assert_eq!(cancelled.status, "cancelled");
    }

    #[test]
    fn table_is_freed_immediately_when_order_is_fully_paid() {
        let mut conn = test_conn();
        let t2 = table_id(&conn, "Table 2");
        
        // Create a dine-in order
        let dine = run(&mut conn, |tx| create_order(
            tx, "pay-frees-table", "dine_in", Some(t2), None, None, None, None, None, None
        )).unwrap();
        let dine = with_cola(&mut conn, &dine, 1.0);
        
        // Table should be occupied
        let table_status: String = conn.query_row(
            "SELECT status FROM tables WHERE id = ?1", [t2], |row| row.get(0)
        ).unwrap();
        assert_eq!(table_status, "occupied");
        
        // Pay the order (even without sending/serving)
        let paid = pay_all(&mut conn, &dine);
        
        // Table should be freed immediately upon payment
        let table_status: String = conn.query_row(
            "SELECT status FROM tables WHERE id = ?1", [t2], |row| row.get(0)
        ).unwrap();
        assert_eq!(table_status, "free", "Table should be freed as soon as order is paid");
        
        // Order should not be completed yet (not served)
        assert!(paid.order.completed_at.is_none(), "Order not completed until served");
        assert_eq!(paid.order.status, "draft");
    }

    #[test]
    fn in_house_delivery_order_validation_and_lifecycle() {
        let mut conn = test_conn();

        // 1. Missing customer phone/address fails
        let missing_addr = DeliveryDetails {
            delivery_rider_id: None,
            rider_source: Some("manual".into()),
            delivery_rider_name: Some("Ali".into()),
            delivery_rider_phone: Some("03001234567".into()),
            delivery_platform_id: None,
            delivery_platform_name: None,
            customer_phone: Some("03121111111".into()),
            delivery_address: None,
            delivery_charge_minor: Some(15000),
            delivery_status: None,
            platform_order_ref: None,
        };
        assert!(matches!(
            run(&mut conn, |tx| create_order(tx, "del-1", "delivery", None, None, None, None, None, None, Some(missing_addr))),
            Err(OrderError::DeliveryCustomerInfoRequired)
        ));

        // 2. Missing rider details fails
        let missing_rider = DeliveryDetails {
            delivery_rider_id: None,
            rider_source: None,
            delivery_rider_name: None,
            delivery_rider_phone: None,
            delivery_platform_id: None,
            delivery_platform_name: None,
            customer_phone: Some("03121111111".into()),
            delivery_address: Some("Street 10, F-7, Islamabad".into()),
            delivery_charge_minor: Some(15000),
            delivery_status: None,
            platform_order_ref: None,
        };
        assert!(matches!(
            run(&mut conn, |tx| create_order(tx, "del-2", "delivery", None, None, None, None, None, None, Some(missing_rider))),
            Err(OrderError::DeliveryRiderRequired)
        ));

        // 3. Valid in-house delivery order creates with status 'placed'
        let valid_delivery = DeliveryDetails {
            delivery_rider_id: None,
            rider_source: Some("manual".into()),
            delivery_rider_name: Some("Hamza".into()),
            delivery_rider_phone: Some("03009876543".into()),
            delivery_platform_id: None,
            delivery_platform_name: None,
            customer_phone: Some("03121111111".into()),
            delivery_address: Some("Street 10, F-7, Islamabad".into()),
            delivery_charge_minor: Some(20000),
            delivery_status: None,
            platform_order_ref: None,
        };
        let order = run(&mut conn, |tx| {
            create_order(tx, "del-3", "delivery", None, None, Some("Zain"), None, None, None, Some(valid_delivery))
        }).unwrap();

        assert_eq!(order.order_type, "delivery");
        assert_eq!(order.delivery_status, Some("placed".into()));
        assert_eq!(order.customer_phone, Some("03121111111".into()));
        assert_eq!(order.delivery_address, Some("Street 10, F-7, Islamabad".into()));
        assert_eq!(order.delivery_charge_minor, 20000);
        assert_eq!(order.delivery_rider_name, Some("Hamza".into()));

        // 4. Update delivery status to dispatched and then delivered
        let updated = run(&mut conn, |tx| update_delivery_status(tx, order.id, "dispatched", None)).unwrap();
        assert_eq!(updated.delivery_status, Some("dispatched".into()));

        let delivered = run(&mut conn, |tx| update_delivery_status(tx, order.id, "delivered", None)).unwrap();
        assert_eq!(delivered.delivery_status, Some("delivered".into()));
    }

    #[test]
    fn third_party_delivery_order_validation() {
        let mut conn = test_conn();

        // 1. Missing platform fails
        let missing_platform = DeliveryDetails {
            delivery_rider_id: None,
            rider_source: None,
            delivery_rider_name: None,
            delivery_rider_phone: None,
            delivery_platform_id: None,
            delivery_platform_name: None,
            customer_phone: None,
            delivery_address: None,
            delivery_charge_minor: None,
            delivery_status: None,
            platform_order_ref: Some("#FP-9988".into()),
        };
        assert!(matches!(
            run(&mut conn, |tx| create_order(tx, "tp-1", "third_party_delivery", None, None, None, None, None, None, Some(missing_platform))),
            Err(OrderError::DeliveryPlatformRequired)
        ));

        // 2. Valid third party delivery order creates successfully
        let valid_tp = DeliveryDetails {
            delivery_rider_id: None,
            rider_source: Some("manual".into()),
            delivery_rider_name: Some("Rider Ahmed".into()),
            delivery_rider_phone: Some("03331234567".into()),
            delivery_platform_id: Some(1),
            delivery_platform_name: Some("Foodpanda".into()),
            customer_phone: None,
            delivery_address: None,
            delivery_charge_minor: None,
            delivery_status: None,
            platform_order_ref: Some("#FP-1002".into()),
        };
        let order = run(&mut conn, |tx| {
            create_order(tx, "tp-2", "third_party_delivery", None, None, None, None, None, None, Some(valid_tp))
        }).unwrap();

        assert_eq!(order.order_type, "third_party_delivery");
        assert_eq!(order.delivery_platform_name, Some("Foodpanda".into()));
        assert_eq!(order.platform_order_ref, Some("#FP-1002".into()));
        assert_eq!(order.delivery_status, Some("placed".into()));
    }

    #[test]
    fn held_order_resumed_and_paid_completes_and_leaves_active_orders() {
        let mut conn = test_conn();
        let cola = item_id(&conn, "Cola 500ml");

        // 1. Create order and add lines
        let order = takeaway(&mut conn, "hold-resume-pay");
        let order = run(&mut conn, |tx| {
            add_lines(tx, order.id, order.version, &[NewLine { item_id: cola, qty: 2.0, notes: None }], None)
        }).unwrap();

        // 2. Hold order
        let held = run(&mut conn, |tx| hold(tx, order.id, order.version, None)).unwrap();
        assert_eq!(held.status, "held");

        // Appears in active orders
        let active = list_active_orders(&conn).unwrap();
        assert!(active.iter().any(|o| o.id == held.id));

        // 3. Resume order at the checkout counter
        let resumed = run(&mut conn, |tx| resume(tx, held.id, held.version, None)).unwrap();
        assert_eq!(resumed.status, "draft");

        // 4. Pay lines
        let paid = pay_all(&mut conn, &resumed);
        assert_eq!(paid.order.payment_status, "paid");

        // 5. Complete / Hand Over
        let completed = run(&mut conn, |tx| transition(tx, paid.order.id, paid.order.version, "served", None)).unwrap();
        assert_eq!(completed.status, "served");
        assert!(completed.completed_at.is_some());

        // 6. Excluded from active orders
        let active_after = list_active_orders(&conn).unwrap();
        assert!(!active_after.iter().any(|o| o.id == held.id));
    }

    #[test]
    fn delivery_order_paid_and_delivered_completes_automatically() {
        let mut conn = test_conn();
        let cola = item_id(&conn, "Cola 500ml");

        let delivery = DeliveryDetails {
            delivery_rider_id: None,
            rider_source: Some("manual".into()),
            delivery_rider_name: Some("Hamza".into()),
            delivery_rider_phone: Some("03121111111".into()),
            delivery_platform_id: None,
            delivery_platform_name: None,
            customer_phone: Some("03001234567".into()),
            delivery_address: Some("House 12, Street 3, Islamabad".into()),
            delivery_charge_minor: Some(15000),
            delivery_status: None,
            platform_order_ref: None,
        };

        let order = run(&mut conn, |tx| {
            create_order(tx, "del-lifecycle", "delivery", None, None, Some("Usman"), None, None, None, Some(delivery))
        }).unwrap();

        let order = run(&mut conn, |tx| {
            add_lines(tx, order.id, order.version, &[NewLine { item_id: cola, qty: 1.0, notes: None }], None)
        }).unwrap();

        // Active orders includes this delivery order
        let active = list_active_orders(&conn).unwrap();
        assert!(active.iter().any(|o| o.id == order.id));

        // Pay for the order
        let paid = pay_all(&mut conn, &order);
        assert_eq!(paid.order.payment_status, "paid");
        assert!(paid.order.completed_at.is_none(), "not completed until delivered");

        // Dispatch order
        let dispatched = run(&mut conn, |tx| update_delivery_status(tx, paid.order.id, "dispatched", None)).unwrap();
        assert_eq!(dispatched.delivery_status, Some("dispatched".into()));
        assert!(dispatched.completed_at.is_none());

        // Mark delivered -> automatically completes because it is already paid!
        let delivered = run(&mut conn, |tx| update_delivery_status(tx, dispatched.id, "delivered", None)).unwrap();
        assert_eq!(delivered.delivery_status, Some("delivered".into()));
        assert_eq!(delivered.status, "served");
        assert!(delivered.completed_at.is_some());

        // Excluded from active orders
        let active_after = list_active_orders(&conn).unwrap();
        assert!(!active_after.iter().any(|o| o.id == order.id));
    }
}
