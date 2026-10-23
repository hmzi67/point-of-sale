//! Suppliers, purchase orders, goods received notes (GRN) and supplier
//! payments — see schema.sql's "Purchasing" section for the relationships.
//!
//! A purchase order never touches stock; receiving goods does. A GRN adds
//! each line's qty to `items.stock_qty`, makes its unit cost the item's
//! `cost_minor` (latest cost wins), writes a `grn` stock movement, and — when
//! received against a PO — advances that PO to `partial` or `received`.
//!
//! What the shop owes a supplier is derived: GRN totals minus payments.

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

use crate::db::customers::{fold_statement, LedgerEntry, Statement};
use crate::db::{clean_text, dates, sales, stock};

#[derive(Debug, thiserror::Error)]
pub enum PurchasingError {
    #[error("Supplier not found")]
    SupplierNotFound,
    #[error("Purchase order #{0} not found")]
    PoNotFound(i64),
    #[error("A supplier named \"{0}\" already exists")]
    DuplicateSupplier(String),
    #[error("{0}")]
    Invalid(String),
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Supplier {
    pub id: i64,
    pub name: String,
    pub contact_person: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub address: Option<String>,
    pub notes: Option<String>,
    pub is_active: bool,
    pub created_at: String,
    pub purchased_minor: i64,
    pub paid_minor: i64,
    /// `purchased - paid`: what the shop still owes.
    pub balance_minor: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupplierInput {
    pub name: String,
    pub contact_person: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub address: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PoLine {
    pub id: i64,
    pub item_id: i64,
    pub item_name: String,
    pub unit: Option<String>,
    pub qty_ordered: f64,
    pub qty_received: f64,
    pub unit_cost_minor: i64,
    pub line_total_minor: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseOrder {
    pub id: i64,
    pub supplier_id: i64,
    pub supplier_name: String,
    pub status: String,
    pub order_date: String,
    pub expected_date: Option<String>,
    pub notes: Option<String>,
    pub created_by_name: Option<String>,
    pub created_at: String,
    pub total_minor: i64,
    pub lines: Vec<PoLine>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PoLineInput {
    pub item_id: i64,
    pub qty: f64,
    pub unit_cost_minor: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PoInput {
    pub supplier_id: i64,
    pub order_date: String,
    pub expected_date: Option<String>,
    pub notes: Option<String>,
    pub lines: Vec<PoLineInput>,
    /// Create straight into `ordered` rather than `draft`.
    #[serde(default)]
    pub place: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrnLineInput {
    pub item_id: i64,
    pub po_item_id: Option<i64>,
    pub qty: f64,
    pub unit_cost_minor: i64,
    pub batch_no: Option<String>,
    pub expiry_date: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrnInput {
    pub supplier_id: i64,
    pub po_id: Option<i64>,
    pub received_date: String,
    pub invoice_ref: Option<String>,
    pub notes: Option<String>,
    pub lines: Vec<GrnLineInput>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrnLine {
    pub item_id: i64,
    pub item_name: String,
    pub unit: Option<String>,
    pub qty: f64,
    pub unit_cost_minor: i64,
    pub line_total_minor: i64,
    pub batch_no: Option<String>,
    pub expiry_date: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoodsReceipt {
    pub id: i64,
    pub supplier_id: i64,
    pub supplier_name: String,
    pub po_id: Option<i64>,
    pub received_date: String,
    pub invoice_ref: Option<String>,
    pub notes: Option<String>,
    pub total_cost_minor: i64,
    pub received_by_name: Option<String>,
    pub created_at: String,
    pub lines: Vec<GrnLine>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupplierPaymentInput {
    pub supplier_id: i64,
    pub amount_minor: i64,
    pub method: String,
    pub reference: Option<String>,
    pub note: Option<String>,
    pub paid_date: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupplierPayment {
    pub id: i64,
    pub supplier_id: i64,
    pub supplier_name: String,
    pub amount_minor: i64,
    pub method: String,
    pub reference: Option<String>,
    pub note: Option<String>,
    pub paid_date: String,
    pub created_by_name: Option<String>,
}

/// An ordered line not yet fully received — stock on its way in.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomingLine {
    pub po_id: i64,
    pub po_item_id: i64,
    pub supplier_name: String,
    pub item_id: i64,
    pub item_name: String,
    pub unit: Option<String>,
    pub qty_outstanding: f64,
    pub unit_cost_minor: i64,
    pub expected_date: Option<String>,
}

use crate::db::payment_methods::SUPPLIER as SUPPLIER_PAYMENT_METHODS;

fn check_date(date: &str) -> Result<(), PurchasingError> {
    dates::parse_date(date)
        .map(|_| ())
        .map_err(PurchasingError::Invalid)
}

// ---------------------------------------------------------------------------
// Suppliers
// ---------------------------------------------------------------------------

const SUPPLIER_SELECT: &str = "
    SELECT s.id, s.name, s.contact_person, s.phone, s.email, s.address, s.notes, s.is_active, s.created_at,
           (SELECT COALESCE(SUM(total_cost_minor), 0) FROM goods_receipts WHERE supplier_id = s.id),
           (SELECT COALESCE(SUM(amount_minor), 0) FROM supplier_payments WHERE supplier_id = s.id)
      FROM suppliers s";

fn supplier_from_row(row: &rusqlite::Row) -> rusqlite::Result<Supplier> {
    let purchased_minor: i64 = row.get(9)?;
    let paid_minor: i64 = row.get(10)?;
    Ok(Supplier {
        id: row.get(0)?,
        name: row.get(1)?,
        contact_person: row.get(2)?,
        phone: row.get(3)?,
        email: row.get(4)?,
        address: row.get(5)?,
        notes: row.get(6)?,
        is_active: row.get::<_, i64>(7)? != 0,
        created_at: row.get(8)?,
        purchased_minor,
        paid_minor,
        balance_minor: purchased_minor - paid_minor,
    })
}

pub fn list_suppliers(
    conn: &Connection,
    include_inactive: bool,
) -> Result<Vec<Supplier>, rusqlite::Error> {
    let mut stmt = conn.prepare(&format!(
        "{SUPPLIER_SELECT} WHERE (?1 = 1 OR s.is_active = 1) ORDER BY s.name COLLATE NOCASE"
    ))?;
    let rows = stmt.query_map(params![include_inactive as i64], supplier_from_row)?;
    rows.collect()
}

pub fn get_supplier(conn: &Connection, id: i64) -> Result<Supplier, PurchasingError> {
    conn.query_row(
        &format!("{SUPPLIER_SELECT} WHERE s.id = ?1"),
        params![id],
        supplier_from_row,
    )
    .optional()?
    .ok_or(PurchasingError::SupplierNotFound)
}

fn supplier_write_error(e: rusqlite::Error, name: &str) -> PurchasingError {
    match e {
        rusqlite::Error::SqliteFailure(err, _)
            if err.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            PurchasingError::DuplicateSupplier(name.to_string())
        }
        other => other.into(),
    }
}

pub fn add_supplier(conn: &Connection, input: SupplierInput) -> Result<Supplier, PurchasingError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(PurchasingError::Invalid("Supplier name is required".into()));
    }
    conn.execute(
        "INSERT INTO suppliers (name, contact_person, phone, email, address, notes) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            name,
            clean_text(&input.contact_person),
            clean_text(&input.phone),
            clean_text(&input.email),
            clean_text(&input.address),
            clean_text(&input.notes)
        ],
    )
    .map_err(|e| supplier_write_error(e, name))?;
    get_supplier(conn, conn.last_insert_rowid())
}

pub fn update_supplier(
    conn: &Connection,
    id: i64,
    input: SupplierInput,
) -> Result<Supplier, PurchasingError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(PurchasingError::Invalid("Supplier name is required".into()));
    }
    let changed = conn
        .execute(
            "UPDATE suppliers SET name = ?1, contact_person = ?2, phone = ?3, email = ?4, address = ?5, notes = ?6
              WHERE id = ?7",
            params![
                name,
                clean_text(&input.contact_person),
                clean_text(&input.phone),
                clean_text(&input.email),
                clean_text(&input.address),
                clean_text(&input.notes),
                id
            ],
        )
        .map_err(|e| supplier_write_error(e, name))?;
    if changed == 0 {
        return Err(PurchasingError::SupplierNotFound);
    }
    get_supplier(conn, id)
}

pub fn set_supplier_active(
    conn: &Connection,
    id: i64,
    active: bool,
) -> Result<Supplier, PurchasingError> {
    let changed = conn.execute(
        "UPDATE suppliers SET is_active = ?1 WHERE id = ?2",
        params![active as i64, id],
    )?;
    if changed == 0 {
        return Err(PurchasingError::SupplierNotFound);
    }
    get_supplier(conn, id)
}

// ---------------------------------------------------------------------------
// Purchase orders
// ---------------------------------------------------------------------------

const PO_SELECT: &str = "
    SELECT p.id, p.supplier_id, s.name, p.status, p.order_date, p.expected_date, p.notes, u.name, p.created_at
      FROM purchase_orders p
      JOIN suppliers s ON s.id = p.supplier_id
      LEFT JOIN users u ON u.id = p.created_by";

fn po_from_row(row: &rusqlite::Row) -> rusqlite::Result<PurchaseOrder> {
    Ok(PurchaseOrder {
        id: row.get(0)?,
        supplier_id: row.get(1)?,
        supplier_name: row.get(2)?,
        status: row.get(3)?,
        order_date: row.get(4)?,
        expected_date: row.get(5)?,
        notes: row.get(6)?,
        created_by_name: row.get(7)?,
        created_at: row.get(8)?,
        total_minor: 0,
        lines: Vec::new(),
    })
}

fn load_po_lines(conn: &Connection, po_id: i64) -> Result<Vec<PoLine>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT l.id, l.item_id, i.name, i.unit, l.qty_ordered, l.qty_received, l.unit_cost_minor
           FROM purchase_order_items l JOIN items i ON i.id = l.item_id
          WHERE l.po_id = ?1 ORDER BY l.id",
    )?;
    let rows = stmt.query_map(params![po_id], |row| {
        let qty_ordered: f64 = row.get(4)?;
        let unit_cost_minor: i64 = row.get(6)?;
        Ok(PoLine {
            id: row.get(0)?,
            item_id: row.get(1)?,
            item_name: row.get(2)?,
            unit: row.get(3)?,
            qty_ordered,
            qty_received: row.get(5)?,
            unit_cost_minor,
            line_total_minor: sales::line_total_minor(unit_cost_minor, qty_ordered),
        })
    })?;
    rows.collect()
}

pub fn get_po(conn: &Connection, id: i64) -> Result<PurchaseOrder, PurchasingError> {
    let mut po = conn
        .query_row(
            &format!("{PO_SELECT} WHERE p.id = ?1"),
            params![id],
            po_from_row,
        )
        .optional()?
        .ok_or(PurchasingError::PoNotFound(id))?;
    po.lines = load_po_lines(conn, id)?;
    po.total_minor = po.lines.iter().map(|l| l.line_total_minor).sum();
    Ok(po)
}

pub fn list_pos(
    conn: &Connection,
    status: Option<&str>,
    limit: i64,
) -> Result<Vec<PurchaseOrder>, PurchasingError> {
    let ids: Vec<i64> = {
        let mut stmt =
            conn.prepare("SELECT id FROM purchase_orders WHERE (?1 IS NULL OR status = ?1) ORDER BY id DESC LIMIT ?2")?;
        let rows = stmt.query_map(params![status, limit.clamp(1, 1000)], |row| row.get(0))?;
        rows.collect::<Result<_, _>>()?
    };
    ids.into_iter().map(|id| get_po(conn, id)).collect()
}

fn validate_po(conn: &Connection, input: &PoInput) -> Result<(), PurchasingError> {
    get_supplier(conn, input.supplier_id)?;
    check_date(&input.order_date)?;
    if let Some(expected) = clean_text(&input.expected_date) {
        check_date(&expected)?;
    }
    if input.lines.is_empty() {
        return Err(PurchasingError::Invalid(
            "Add at least one item to the order".into(),
        ));
    }
    for line in &input.lines {
        if !line.qty.is_finite() || line.qty <= 0.0 {
            return Err(PurchasingError::Invalid(
                "Order quantities must be greater than zero".into(),
            ));
        }
        if line.unit_cost_minor < 0 {
            return Err(PurchasingError::Invalid(
                "Unit cost cannot be negative".into(),
            ));
        }
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM items WHERE id = ?1)",
            params![line.item_id],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(PurchasingError::Invalid(format!(
                "Item {} no longer exists",
                line.item_id
            )));
        }
    }
    Ok(())
}

fn insert_po_lines(
    tx: &Transaction,
    po_id: i64,
    lines: &[PoLineInput],
) -> Result<(), rusqlite::Error> {
    for line in lines {
        tx.execute(
            "INSERT INTO purchase_order_items (po_id, item_id, qty_ordered, unit_cost_minor) VALUES (?1, ?2, ?3, ?4)",
            params![po_id, line.item_id, line.qty, line.unit_cost_minor],
        )?;
    }
    Ok(())
}

pub fn create_po(
    tx: &Transaction,
    input: PoInput,
    user_id: Option<i64>,
) -> Result<PurchaseOrder, PurchasingError> {
    validate_po(tx, &input)?;
    tx.execute(
        "INSERT INTO purchase_orders (supplier_id, status, order_date, expected_date, notes, created_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            input.supplier_id,
            if input.place { "ordered" } else { "draft" },
            input.order_date,
            clean_text(&input.expected_date),
            clean_text(&input.notes),
            user_id
        ],
    )?;
    let id = tx.last_insert_rowid();
    insert_po_lines(tx, id, &input.lines)?;
    get_po(tx, id)
}

/// Replaces a draft order's header and lines. Placed orders are fixed.
pub fn update_po(
    tx: &Transaction,
    id: i64,
    input: PoInput,
) -> Result<PurchaseOrder, PurchasingError> {
    let current = get_po(tx, id)?;
    if current.status != "draft" {
        return Err(PurchasingError::Invalid(
            "Only a draft order can be edited".into(),
        ));
    }
    validate_po(tx, &input)?;
    tx.execute(
        "UPDATE purchase_orders SET supplier_id = ?1, order_date = ?2, expected_date = ?3, notes = ?4, status = ?5
          WHERE id = ?6",
        params![
            input.supplier_id,
            input.order_date,
            clean_text(&input.expected_date),
            clean_text(&input.notes),
            if input.place { "ordered" } else { "draft" },
            id
        ],
    )?;
    tx.execute(
        "DELETE FROM purchase_order_items WHERE po_id = ?1",
        params![id],
    )?;
    insert_po_lines(tx, id, &input.lines)?;
    get_po(tx, id)
}

/// `ordered` (from draft) or `cancelled` (from draft, ordered or partial —
/// what was already received stays received).
pub fn set_po_status(
    tx: &Transaction,
    id: i64,
    status: &str,
) -> Result<PurchaseOrder, PurchasingError> {
    let current = get_po(tx, id)?;
    let allowed = match status {
        "ordered" => current.status == "draft",
        "cancelled" => matches!(current.status.as_str(), "draft" | "ordered" | "partial"),
        _ => false,
    };
    if !allowed {
        return Err(PurchasingError::Invalid(format!(
            "An order that is {} cannot become {status}",
            current.status
        )));
    }
    tx.execute(
        "UPDATE purchase_orders SET status = ?1 WHERE id = ?2",
        params![status, id],
    )?;
    get_po(tx, id)
}

/// Ordered lines still waiting on delivery, soonest expected first.
pub fn incoming_stock(conn: &Connection) -> Result<Vec<IncomingLine>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT p.id, l.id, s.name, l.item_id, i.name, i.unit, l.qty_ordered - l.qty_received,
                l.unit_cost_minor, p.expected_date
           FROM purchase_order_items l
           JOIN purchase_orders p ON p.id = l.po_id
           JOIN suppliers s ON s.id = p.supplier_id
           JOIN items i ON i.id = l.item_id
          WHERE p.status IN ('ordered', 'partial') AND l.qty_received < l.qty_ordered
          ORDER BY COALESCE(p.expected_date, '9999-12-31'), p.id, l.id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(IncomingLine {
            po_id: row.get(0)?,
            po_item_id: row.get(1)?,
            supplier_name: row.get(2)?,
            item_id: row.get(3)?,
            item_name: row.get(4)?,
            unit: row.get(5)?,
            qty_outstanding: row.get(6)?,
            unit_cost_minor: row.get(7)?,
            expected_date: row.get(8)?,
        })
    })?;
    rows.collect()
}

// ---------------------------------------------------------------------------
// Goods received
// ---------------------------------------------------------------------------

pub fn receive_goods(
    tx: &Transaction,
    input: GrnInput,
    user_id: Option<i64>,
) -> Result<GoodsReceipt, PurchasingError> {
    get_supplier(tx, input.supplier_id)?;
    check_date(&input.received_date)?;
    if input.lines.is_empty() {
        return Err(PurchasingError::Invalid(
            "Add at least one received item".into(),
        ));
    }
    if let Some(po_id) = input.po_id {
        let po = get_po(tx, po_id)?;
        if po.supplier_id != input.supplier_id {
            return Err(PurchasingError::Invalid(
                "That purchase order is for a different supplier".into(),
            ));
        }
        if !matches!(po.status.as_str(), "ordered" | "partial") {
            return Err(PurchasingError::Invalid(format!(
                "Purchase order #{po_id} is {} — place it first",
                po.status
            )));
        }
    }

    let mut total: i64 = 0;
    for line in &input.lines {
        if !line.qty.is_finite() || line.qty <= 0.0 {
            return Err(PurchasingError::Invalid(
                "Received quantities must be greater than zero".into(),
            ));
        }
        if line.unit_cost_minor < 0 {
            return Err(PurchasingError::Invalid(
                "Unit cost cannot be negative".into(),
            ));
        }
        if let Some(expiry) = clean_text(&line.expiry_date) {
            check_date(&expiry)?;
        }
        total += sales::line_total_minor(line.unit_cost_minor, line.qty);
    }

    tx.execute(
        "INSERT INTO goods_receipts (supplier_id, po_id, received_date, invoice_ref, notes, total_cost_minor, received_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            input.supplier_id,
            input.po_id,
            input.received_date,
            clean_text(&input.invoice_ref),
            clean_text(&input.notes),
            total,
            user_id
        ],
    )?;
    let grn_id = tx.last_insert_rowid();

    for line in &input.lines {
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM items WHERE id = ?1)",
            params![line.item_id],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(PurchasingError::Invalid(format!(
                "Item {} no longer exists",
                line.item_id
            )));
        }
        if let Some(po_item_id) = line.po_item_id {
            let belongs: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM purchase_order_items WHERE id = ?1 AND po_id IS ?2 AND item_id = ?3)",
                params![po_item_id, input.po_id, line.item_id],
                |r| r.get(0),
            )?;
            if !belongs {
                return Err(PurchasingError::Invalid(
                    "A received line doesn't match its purchase order".into(),
                ));
            }
            tx.execute(
                "UPDATE purchase_order_items SET qty_received = qty_received + ?1 WHERE id = ?2",
                params![line.qty, po_item_id],
            )?;
        }
        tx.execute(
            "INSERT INTO goods_receipt_items (grn_id, item_id, po_item_id, qty, unit_cost_minor, batch_no, expiry_date)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                grn_id,
                line.item_id,
                line.po_item_id,
                line.qty,
                line.unit_cost_minor,
                clean_text(&line.batch_no),
                clean_text(&line.expiry_date)
            ],
        )?;
        tx.execute(
            "UPDATE items SET cost_minor = ?1 WHERE id = ?2",
            params![line.unit_cost_minor, line.item_id],
        )?;
        stock::apply_movement(
            tx,
            line.item_id,
            line.qty,
            "grn",
            Some(grn_id),
            None,
            user_id,
        )?;
    }

    if let Some(po_id) = input.po_id {
        let open: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM purchase_order_items WHERE po_id = ?1 AND qty_received < qty_ordered)",
            params![po_id],
            |r| r.get(0),
        )?;
        tx.execute(
            "UPDATE purchase_orders SET status = ?1 WHERE id = ?2",
            params![if open { "partial" } else { "received" }, po_id],
        )?;
    }

    get_grn(tx, grn_id)
}

const GRN_SELECT: &str = "
    SELECT g.id, g.supplier_id, s.name, g.po_id, g.received_date, g.invoice_ref, g.notes, g.total_cost_minor,
           u.name, g.created_at
      FROM goods_receipts g
      JOIN suppliers s ON s.id = g.supplier_id
      LEFT JOIN users u ON u.id = g.received_by";

fn grn_from_row(row: &rusqlite::Row) -> rusqlite::Result<GoodsReceipt> {
    Ok(GoodsReceipt {
        id: row.get(0)?,
        supplier_id: row.get(1)?,
        supplier_name: row.get(2)?,
        po_id: row.get(3)?,
        received_date: row.get(4)?,
        invoice_ref: row.get(5)?,
        notes: row.get(6)?,
        total_cost_minor: row.get(7)?,
        received_by_name: row.get(8)?,
        created_at: row.get(9)?,
        lines: Vec::new(),
    })
}

pub fn get_grn(conn: &Connection, id: i64) -> Result<GoodsReceipt, PurchasingError> {
    let mut grn = conn
        .query_row(
            &format!("{GRN_SELECT} WHERE g.id = ?1"),
            params![id],
            grn_from_row,
        )
        .optional()?
        .ok_or_else(|| PurchasingError::Invalid(format!("Goods receipt #{id} not found")))?;
    let mut stmt = conn.prepare(
        "SELECT l.item_id, i.name, i.unit, l.qty, l.unit_cost_minor, l.batch_no, l.expiry_date
           FROM goods_receipt_items l JOIN items i ON i.id = l.item_id
          WHERE l.grn_id = ?1 ORDER BY l.id",
    )?;
    grn.lines = stmt
        .query_map(params![id], |row| {
            let qty: f64 = row.get(3)?;
            let unit_cost_minor: i64 = row.get(4)?;
            Ok(GrnLine {
                item_id: row.get(0)?,
                item_name: row.get(1)?,
                unit: row.get(2)?,
                qty,
                unit_cost_minor,
                line_total_minor: sales::line_total_minor(unit_cost_minor, qty),
                batch_no: row.get(5)?,
                expiry_date: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(grn)
}

pub fn list_grns(
    conn: &Connection,
    supplier_id: Option<i64>,
    limit: i64,
) -> Result<Vec<GoodsReceipt>, PurchasingError> {
    let ids: Vec<i64> = {
        let mut stmt = conn
            .prepare("SELECT id FROM goods_receipts WHERE (?1 IS NULL OR supplier_id = ?1) ORDER BY id DESC LIMIT ?2")?;
        let rows = stmt.query_map(params![supplier_id, limit.clamp(1, 1000)], |row| row.get(0))?;
        rows.collect::<Result<_, _>>()?
    };
    ids.into_iter().map(|id| get_grn(conn, id)).collect()
}

// ---------------------------------------------------------------------------
// Supplier payments and statement
// ---------------------------------------------------------------------------

pub fn record_supplier_payment(
    conn: &Connection,
    input: SupplierPaymentInput,
    user_id: Option<i64>,
) -> Result<SupplierPayment, PurchasingError> {
    get_supplier(conn, input.supplier_id)?;
    if input.amount_minor <= 0 {
        return Err(PurchasingError::Invalid(
            "Enter an amount greater than zero".into(),
        ));
    }
    if !SUPPLIER_PAYMENT_METHODS.contains(&input.method.as_str()) {
        return Err(PurchasingError::Invalid(format!(
            "Unknown payment method: {}",
            input.method
        )));
    }
    check_date(&input.paid_date)?;
    conn.execute(
        "INSERT INTO supplier_payments (supplier_id, amount_minor, method, reference, note, paid_date, created_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            input.supplier_id,
            input.amount_minor,
            input.method,
            clean_text(&input.reference),
            clean_text(&input.note),
            input.paid_date,
            user_id
        ],
    )?;
    let id = conn.last_insert_rowid();
    Ok(list_supplier_payments(conn, Some(input.supplier_id), 1000)?
        .into_iter()
        .find(|p| p.id == id)
        .expect("the payment just inserted"))
}

pub fn list_supplier_payments(
    conn: &Connection,
    supplier_id: Option<i64>,
    limit: i64,
) -> Result<Vec<SupplierPayment>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.supplier_id, s.name, p.amount_minor, p.method, p.reference, p.note, p.paid_date, u.name
           FROM supplier_payments p
           JOIN suppliers s ON s.id = p.supplier_id
           LEFT JOIN users u ON u.id = p.created_by
          WHERE (?1 IS NULL OR p.supplier_id = ?1)
          ORDER BY p.paid_date DESC, p.id DESC
          LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![supplier_id, limit.clamp(1, 5000)], |row| {
        Ok(SupplierPayment {
            id: row.get(0)?,
            supplier_id: row.get(1)?,
            supplier_name: row.get(2)?,
            amount_minor: row.get(3)?,
            method: row.get(4)?,
            reference: row.get(5)?,
            note: row.get(6)?,
            paid_date: row.get(7)?,
            created_by_name: row.get(8)?,
        })
    })?;
    rows.collect()
}

/// Running statement of what the shop owes `supplier_id`: goods received
/// are debits (owed more), payments are credits.
pub fn supplier_statement(
    conn: &Connection,
    supplier_id: i64,
    start_date: Option<&str>,
    end_date: Option<&str>,
) -> Result<Statement, PurchasingError> {
    get_supplier(conn, supplier_id)?;
    let mut stmt = conn.prepare(
        "SELECT received_date, 0, 'grn', 'GRN #' || id,
                'Goods received' || COALESCE(' — invoice ' || invoice_ref, '')
                    || COALESCE(' (PO #' || po_id || ')', ''),
                total_cost_minor, 0, id
           FROM goods_receipts WHERE supplier_id = ?1
         UNION ALL
         SELECT paid_date, 1, 'payment', 'Payment #' || id,
                'Paid (' || method || ')' || COALESCE(' — ' || reference, ''),
                0, amount_minor, id
           FROM supplier_payments WHERE supplier_id = ?1
         ORDER BY 1, 2, 8",
    )?;
    let events = stmt
        .query_map(params![supplier_id], |row| {
            Ok(LedgerEntry {
                date: row.get(0)?,
                kind: row.get(2)?,
                reference: row.get(3)?,
                description: row.get(4)?,
                debit_minor: row.get(5)?,
                credit_minor: row.get(6)?,
                balance_minor: 0,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(fold_statement(events, start_date, end_date))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::test_conn;
    use crate::db::test_support::{in_tx, item_id, stock};

    fn supplier(conn: &Connection) -> i64 {
        add_supplier(
            conn,
            SupplierInput {
                name: "Metro Wholesale".into(),
                contact_person: None,
                phone: None,
                email: None,
                address: None,
                notes: None,
            },
        )
        .unwrap()
        .id
    }

    #[test]
    fn receiving_against_a_po_moves_stock_cost_and_status() {
        let mut conn = test_conn();
        let supplier_id = supplier(&conn);
        let cola = item_id(&conn, "Cola 500ml");
        let before = stock(&conn, "Cola 500ml");
        let po = in_tx(&mut conn, |tx| {
            create_po(
                tx,
                PoInput {
                    supplier_id,
                    order_date: "2026-01-01".into(),
                    expected_date: None,
                    notes: None,
                    lines: vec![PoLineInput {
                        item_id: cola,
                        qty: 24.0,
                        unit_cost_minor: 6000,
                    }],
                    place: true,
                },
                None,
            )
        })
        .unwrap();
        assert_eq!(po.status, "ordered");
        assert_eq!(incoming_stock(&conn).unwrap().len(), 1);

        let receive = |qty: f64| GrnInput {
            supplier_id,
            po_id: Some(po.id),
            received_date: "2026-01-02".into(),
            invoice_ref: Some("INV-9".into()),
            notes: None,
            lines: vec![GrnLineInput {
                item_id: cola,
                po_item_id: Some(po.lines[0].id),
                qty,
                unit_cost_minor: 6500,
                batch_no: None,
                expiry_date: Some("2027-01-01".into()),
            }],
        };
        in_tx(&mut conn, |tx| receive_goods(tx, receive(10.0), None)).unwrap();
        assert_eq!(get_po(&conn, po.id).unwrap().status, "partial");
        in_tx(&mut conn, |tx| receive_goods(tx, receive(14.0), None)).unwrap();
        assert_eq!(get_po(&conn, po.id).unwrap().status, "received");
        assert!(incoming_stock(&conn).unwrap().is_empty());

        assert_eq!(stock(&conn, "Cola 500ml"), before + 24.0);
        let cost: i64 = conn
            .query_row("SELECT cost_minor FROM items WHERE id = ?1", [cola], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(cost, 6500);

        let owed = get_supplier(&conn, supplier_id).unwrap().balance_minor;
        assert_eq!(owed, 24 * 6500);
        record_supplier_payment(
            &conn,
            SupplierPaymentInput {
                supplier_id,
                amount_minor: 50_000,
                method: "bank".into(),
                reference: None,
                note: None,
                paid_date: "2026-01-03".into(),
            },
            None,
        )
        .unwrap();
        let statement = supplier_statement(&conn, supplier_id, None, None).unwrap();
        assert_eq!(statement.entries.len(), 3);
        assert_eq!(statement.closing_balance_minor, owed - 50_000);
    }

    #[test]
    fn a_draft_po_cannot_be_received_and_a_received_one_cannot_be_cancelled() {
        let mut conn = test_conn();
        let supplier_id = supplier(&conn);
        let cola = item_id(&conn, "Cola 500ml");
        let po = in_tx(&mut conn, |tx| {
            create_po(
                tx,
                PoInput {
                    supplier_id,
                    order_date: "2026-01-01".into(),
                    expected_date: None,
                    notes: None,
                    lines: vec![PoLineInput {
                        item_id: cola,
                        qty: 1.0,
                        unit_cost_minor: 100,
                    }],
                    place: false,
                },
                None,
            )
        })
        .unwrap();
        let grn = GrnInput {
            supplier_id,
            po_id: Some(po.id),
            received_date: "2026-01-02".into(),
            invoice_ref: None,
            notes: None,
            lines: vec![GrnLineInput {
                item_id: cola,
                po_item_id: Some(po.lines[0].id),
                qty: 1.0,
                unit_cost_minor: 100,
                batch_no: None,
                expiry_date: None,
            }],
        };
        let draft = in_tx(&mut conn, |tx| receive_goods(tx, grn.clone(), None));
        assert!(
            matches!(draft, Err(PurchasingError::Invalid(ref m)) if m.contains("draft")),
            "{draft:?}"
        );
        in_tx(&mut conn, |tx| set_po_status(tx, po.id, "ordered")).unwrap();
        in_tx(&mut conn, |tx| receive_goods(tx, grn.clone(), None)).unwrap();
        let cancel = in_tx(&mut conn, |tx| set_po_status(tx, po.id, "cancelled"));
        assert!(
            matches!(cancel, Err(PurchasingError::Invalid(ref m)) if m.contains("received")),
            "{cancel:?}"
        );
    }

    fn supplier_input(name: &str) -> SupplierInput {
        SupplierInput {
            name: name.into(),
            contact_person: None,
            phone: None,
            email: None,
            address: None,
            notes: None,
        }
    }

    #[test]
    fn suppliers_update_archive_and_refuse_duplicate_names() {
        let conn = test_conn();
        let id = supplier(&conn);
        let updated = update_supplier(
            &conn,
            id,
            SupplierInput {
                phone: Some("  0300 1111111 ".into()),
                contact_person: Some("   ".into()),
                ..supplier_input("Metro Cash")
            },
        )
        .unwrap();
        assert_eq!(updated.name, "Metro Cash");
        assert_eq!(updated.phone.as_deref(), Some("0300 1111111"), "trimmed");
        assert_eq!(updated.contact_person, None, "blank is stored as NULL");

        add_supplier(&conn, supplier_input("Other")).unwrap();
        assert!(matches!(
            add_supplier(&conn, supplier_input("Other")),
            Err(PurchasingError::DuplicateSupplier(_))
        ));
        assert!(matches!(
            update_supplier(&conn, id, supplier_input("Other")),
            Err(PurchasingError::DuplicateSupplier(_))
        ));
        assert!(matches!(
            add_supplier(&conn, supplier_input("  ")),
            Err(PurchasingError::Invalid(_))
        ));

        assert!(!set_supplier_active(&conn, id, false).unwrap().is_active);
        assert_eq!(
            list_suppliers(&conn, false)
                .unwrap()
                .iter()
                .filter(|s| s.id == id)
                .count(),
            0
        );
        assert_eq!(
            list_suppliers(&conn, true)
                .unwrap()
                .iter()
                .filter(|s| s.id == id)
                .count(),
            1
        );
        assert!(matches!(
            set_supplier_active(&conn, 999_999, true),
            Err(PurchasingError::SupplierNotFound)
        ));
    }

    #[test]
    fn only_a_draft_po_can_be_edited() {
        let mut conn = test_conn();
        let supplier_id = supplier(&conn);
        let cola = item_id(&conn, "Cola 500ml");
        let input = |qty: f64, place: bool| PoInput {
            supplier_id,
            order_date: "2026-01-01".into(),
            expected_date: None,
            notes: None,
            lines: vec![PoLineInput {
                item_id: cola,
                qty,
                unit_cost_minor: 250,
            }],
            place,
        };
        let po = in_tx(&mut conn, |tx| create_po(tx, input(2.0, false), None)).unwrap();
        let edited = in_tx(&mut conn, |tx| update_po(tx, po.id, input(6.0, true))).unwrap();
        assert_eq!(edited.status, "ordered");
        assert_eq!(edited.lines.len(), 1);
        assert_eq!(edited.total_minor, 1500);
        let again = in_tx(&mut conn, |tx| update_po(tx, po.id, input(1.0, false)));
        assert!(matches!(again, Err(PurchasingError::Invalid(ref m)) if m.contains("draft")));

        let empty = PoInput {
            lines: vec![],
            ..input(1.0, false)
        };
        assert!(matches!(
            in_tx(&mut conn, |tx| create_po(tx, empty, None)),
            Err(PurchasingError::Invalid(_))
        ));
        let bad_qty = input(0.0, false);
        assert!(matches!(
            in_tx(&mut conn, |tx| create_po(tx, bad_qty, None)),
            Err(PurchasingError::Invalid(_))
        ));
    }

    #[test]
    fn supplier_payments_are_validated_and_the_statement_carries_an_opening_balance() {
        let mut conn = test_conn();
        let supplier_id = supplier(&conn);
        let cola = item_id(&conn, "Cola 500ml");
        let grn = |date: &str| GrnInput {
            supplier_id,
            po_id: None,
            received_date: date.into(),
            invoice_ref: Some("INV".into()),
            notes: None,
            lines: vec![GrnLineInput {
                item_id: cola,
                po_item_id: None,
                qty: 10.0,
                unit_cost_minor: 1000,
                batch_no: None,
                expiry_date: None,
            }],
        };
        let first = in_tx(&mut conn, |tx| receive_goods(tx, grn("2026-01-05"), None)).unwrap();
        in_tx(&mut conn, |tx| receive_goods(tx, grn("2026-02-05"), None)).unwrap();
        assert_eq!(get_grn(&conn, first.id).unwrap().total_cost_minor, 10_000);
        assert_eq!(list_grns(&conn, Some(supplier_id), 10).unwrap().len(), 2);

        let pay = |amount: i64, method: &str, date: &str| SupplierPaymentInput {
            supplier_id,
            amount_minor: amount,
            method: method.into(),
            reference: None,
            note: None,
            paid_date: date.into(),
        };
        assert!(matches!(
            record_supplier_payment(&conn, pay(0, "cash", "2026-01-10"), None),
            Err(PurchasingError::Invalid(_))
        ));
        assert!(matches!(
            record_supplier_payment(&conn, pay(100, "cheque", "2026-01-10"), None),
            Err(PurchasingError::Invalid(_))
        ));
        assert!(matches!(
            record_supplier_payment(&conn, pay(100, "cash", "10/01/2026"), None),
            Err(PurchasingError::Invalid(_))
        ));
        record_supplier_payment(&conn, pay(4_000, "bank", "2026-01-10"), None).unwrap();

        let february =
            supplier_statement(&conn, supplier_id, Some("2026-02-01"), Some("2026-02-28")).unwrap();
        assert_eq!(
            february.opening_balance_minor, 6_000,
            "January: 10,000 received - 4,000 paid"
        );
        assert_eq!(february.entries.len(), 1);
        assert_eq!(february.closing_balance_minor, 16_000);
        assert_eq!(
            get_supplier(&conn, supplier_id).unwrap().balance_minor,
            16_000
        );
    }
}
