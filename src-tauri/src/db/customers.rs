//! Customer accounts and pay-later (credit) sales.
//!
//! A sale's pay-later part is its `sale_payments` row with method `credit`.
//! What is still owed on it is derived, never stored:
//!
//!   outstanding = max(0, credit - installments - refunds)
//!
//! Refunds count against the unpaid credit first — a customer returning
//! goods they have not paid for owes less, and gets cash back only for what
//! exceeds that (`refund_cash_out`, which shift reconciliation uses).
//!
//! Installments always name the sale they pay: an amount received "on
//! account" is spread over the customer's outstanding sales, oldest due date
//! first, one `customer_payments` row per sale it touches.

use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

use crate::db::{clean_text, dates};

#[derive(Debug, thiserror::Error)]
pub enum CustomerError {
    #[error("Customer name is required")]
    NameRequired,
    #[error("Customer not found")]
    NotFound,
    #[error("{0}")]
    Invalid(String),
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Customer {
    pub id: i64,
    pub name: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub address: Option<String>,
    pub notes: Option<String>,
    pub is_active: bool,
    pub created_at: String,
    /// Sum of `outstanding_minor` over this customer's credit sales.
    pub balance_minor: i64,
    /// The part of `balance_minor` that is past its due date.
    pub overdue_minor: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomerInput {
    pub name: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub address: Option<String>,
    pub notes: Option<String>,
}

/// One sale with a pay-later part.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreditSale {
    pub sale_id: i64,
    pub customer_id: i64,
    pub customer_name: String,
    pub customer_phone: Option<String>,
    pub created_at: String,
    pub due_date: Option<String>,
    pub total_minor: i64,
    pub credit_minor: i64,
    pub paid_minor: i64,
    pub refunded_minor: i64,
    pub outstanding_minor: i64,
    /// Days past `due_date` (0 when not yet due or settled).
    pub days_overdue: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomerPayment {
    pub id: i64,
    pub customer_id: i64,
    pub sale_id: Option<i64>,
    pub amount_minor: i64,
    pub method: String,
    pub note: Option<String>,
    pub received_by_name: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverdueSummary {
    pub sale_count: i64,
    pub customer_count: i64,
    pub amount_minor: i64,
    /// Not yet overdue, but due within the next 3 days.
    pub due_soon_count: i64,
}

/// One line of a customer statement. `debit` raises what they owe, `credit`
/// lowers it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerEntry {
    pub date: String,
    pub kind: String,
    pub reference: String,
    pub description: String,
    pub debit_minor: i64,
    pub credit_minor: i64,
    pub balance_minor: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Statement {
    pub opening_balance_minor: i64,
    pub entries: Vec<LedgerEntry>,
    pub closing_balance_minor: i64,
}

use crate::db::payment_methods::COLLECTION as PAYMENT_METHODS;

fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

/// Per-sale credit math for every sale with a pay-later part, optionally for
/// one customer. Sorted oldest due date first (the order installments on
/// account are applied in).
const CREDIT_SALES_SQL: &str = "
    SELECT s.id, s.customer_id, c.name, c.phone, s.created_at, s.due_date, s.total_minor,
           (SELECT COALESCE(SUM(amount_minor), 0) FROM sale_payments
             WHERE sale_id = s.id AND method = 'credit') AS credit_minor,
           (SELECT COALESCE(SUM(amount_minor), 0) FROM customer_payments WHERE sale_id = s.id) AS paid_minor,
           (SELECT COALESCE(SUM(total_refund_amount_minor), 0) FROM refunds
             WHERE original_sale_id = s.id) AS refunded_minor
      FROM sales s
      JOIN customers c ON c.id = s.customer_id
     WHERE EXISTS (SELECT 1 FROM sale_payments sp WHERE sp.sale_id = s.id AND sp.method = 'credit')
       AND (?1 IS NULL OR s.customer_id = ?1)
     ORDER BY COALESCE(s.due_date, '9999-12-31'), s.id";

pub fn list_credit_sales(
    conn: &Connection,
    customer_id: Option<i64>,
    only_outstanding: bool,
) -> Result<Vec<CreditSale>, rusqlite::Error> {
    let today = today();
    let mut stmt = conn.prepare(CREDIT_SALES_SQL)?;
    let rows = stmt.query_map(params![customer_id], |row| {
        let credit_minor: i64 = row.get(7)?;
        let paid_minor: i64 = row.get(8)?;
        let refunded_minor: i64 = row.get(9)?;
        let due_date: Option<String> = row.get(5)?;
        let outstanding_minor = (credit_minor - paid_minor - refunded_minor).max(0);
        let days_overdue = match due_date.as_deref().and_then(|d| dates::parse_date(d).ok()) {
            Some(due) if outstanding_minor > 0 && due < today => (today - due).num_days(),
            _ => 0,
        };
        Ok(CreditSale {
            sale_id: row.get(0)?,
            customer_id: row.get(1)?,
            customer_name: row.get(2)?,
            customer_phone: row.get(3)?,
            created_at: row.get(4)?,
            due_date,
            total_minor: row.get(6)?,
            credit_minor,
            paid_minor,
            refunded_minor,
            outstanding_minor,
            days_overdue,
        })
    })?;
    let mut list = rows.collect::<Result<Vec<_>, _>>()?;
    if only_outstanding {
        list.retain(|sale| sale.outstanding_minor > 0);
    }
    Ok(list)
}

/// Every outstanding credit sale past its due date, most overdue first.
pub fn list_overdue(conn: &Connection) -> Result<Vec<CreditSale>, rusqlite::Error> {
    let mut list = list_credit_sales(conn, None, true)?;
    list.retain(|sale| sale.days_overdue > 0);
    list.sort_by_key(|sale| std::cmp::Reverse(sale.days_overdue));
    Ok(list)
}

pub fn overdue_summary(conn: &Connection) -> Result<OverdueSummary, rusqlite::Error> {
    let soon = today() + chrono::Duration::days(3);
    let outstanding = list_credit_sales(conn, None, true)?;
    let overdue: Vec<&CreditSale> = outstanding.iter().filter(|s| s.days_overdue > 0).collect();
    let mut customers: Vec<i64> = overdue.iter().map(|s| s.customer_id).collect();
    customers.sort_unstable();
    customers.dedup();
    let due_soon_count = outstanding
        .iter()
        .filter(|s| s.days_overdue == 0)
        .filter(|s| {
            s.due_date
                .as_deref()
                .and_then(|d| dates::parse_date(d).ok())
                .is_some_and(|d| d <= soon)
        })
        .count() as i64;
    Ok(OverdueSummary {
        sale_count: overdue.len() as i64,
        customer_count: customers.len() as i64,
        amount_minor: overdue.iter().map(|s| s.outstanding_minor).sum(),
        due_soon_count,
    })
}

fn balances(
    conn: &Connection,
) -> Result<std::collections::HashMap<i64, (i64, i64)>, rusqlite::Error> {
    let mut map = std::collections::HashMap::new();
    for sale in list_credit_sales(conn, None, true)? {
        let entry = map.entry(sale.customer_id).or_insert((0, 0));
        entry.0 += sale.outstanding_minor;
        if sale.days_overdue > 0 {
            entry.1 += sale.outstanding_minor;
        }
    }
    Ok(map)
}

const CUSTOMER_SELECT: &str =
    "SELECT id, name, phone, email, address, notes, is_active, created_at FROM customers";

fn customer_from_row(row: &rusqlite::Row) -> rusqlite::Result<Customer> {
    Ok(Customer {
        id: row.get(0)?,
        name: row.get(1)?,
        phone: row.get(2)?,
        email: row.get(3)?,
        address: row.get(4)?,
        notes: row.get(5)?,
        is_active: row.get::<_, i64>(6)? != 0,
        created_at: row.get(7)?,
        balance_minor: 0,
        overdue_minor: 0,
    })
}

/// Customers matching `search` (name, phone or email), name order. Archived
/// customers are left out unless `include_inactive`.
pub fn list_customers(
    conn: &Connection,
    search: Option<&str>,
    include_inactive: bool,
    limit: i64,
) -> Result<Vec<Customer>, rusqlite::Error> {
    let pattern = search
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| format!("%{s}%"));
    let mut stmt = conn.prepare(&format!(
        "{CUSTOMER_SELECT}
          WHERE (?1 IS NULL OR name LIKE ?1 OR phone LIKE ?1 OR email LIKE ?1)
            AND (?2 = 1 OR is_active = 1)
          ORDER BY name COLLATE NOCASE
          LIMIT ?3"
    ))?;
    let mut list = stmt
        .query_map(
            params![pattern, include_inactive as i64, limit.clamp(1, 5000)],
            customer_from_row,
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let balances = balances(conn)?;
    for customer in &mut list {
        if let Some((balance, overdue)) = balances.get(&customer.id) {
            customer.balance_minor = *balance;
            customer.overdue_minor = *overdue;
        }
    }
    Ok(list)
}

pub fn get_customer(conn: &Connection, id: i64) -> Result<Customer, CustomerError> {
    let mut customer = conn
        .query_row(
            &format!("{CUSTOMER_SELECT} WHERE id = ?1"),
            params![id],
            customer_from_row,
        )
        .optional()?
        .ok_or(CustomerError::NotFound)?;
    for sale in list_credit_sales(conn, Some(id), true)? {
        customer.balance_minor += sale.outstanding_minor;
        if sale.days_overdue > 0 {
            customer.overdue_minor += sale.outstanding_minor;
        }
    }
    Ok(customer)
}

fn validate(input: &CustomerInput) -> Result<String, CustomerError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(CustomerError::NameRequired);
    }
    if let Some(email) = clean_text(&input.email) {
        if !email.contains('@') || email.contains(' ') {
            return Err(CustomerError::Invalid(
                "That email address doesn't look right".into(),
            ));
        }
    }
    Ok(name.to_string())
}

pub fn add_customer(conn: &Connection, input: CustomerInput) -> Result<Customer, CustomerError> {
    let name = validate(&input)?;
    conn.execute(
        "INSERT INTO customers (name, phone, email, address, notes) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            name,
            clean_text(&input.phone),
            clean_text(&input.email),
            clean_text(&input.address),
            clean_text(&input.notes)
        ],
    )?;
    get_customer(conn, conn.last_insert_rowid())
}

pub fn update_customer(
    conn: &Connection,
    id: i64,
    input: CustomerInput,
) -> Result<Customer, CustomerError> {
    let name = validate(&input)?;
    let changed = conn.execute(
        "UPDATE customers SET name = ?1, phone = ?2, email = ?3, address = ?4, notes = ?5 WHERE id = ?6",
        params![name, clean_text(&input.phone), clean_text(&input.email), clean_text(&input.address), clean_text(&input.notes), id],
    )?;
    if changed == 0 {
        return Err(CustomerError::NotFound);
    }
    get_customer(conn, id)
}

/// Archives or restores a customer. One with money still owing stays
/// active, so the debt never drops out of sight.
pub fn set_active(conn: &Connection, id: i64, active: bool) -> Result<Customer, CustomerError> {
    let customer = get_customer(conn, id)?;
    if !active && customer.balance_minor > 0 {
        return Err(CustomerError::Invalid(
            "This customer still owes money — settle the balance before archiving".into(),
        ));
    }
    conn.execute(
        "UPDATE customers SET is_active = ?1 WHERE id = ?2",
        params![active as i64, id],
    )?;
    get_customer(conn, id)
}

pub struct PaymentInput<'a> {
    pub customer_id: i64,
    /// `None` spreads the amount over outstanding sales, oldest due first.
    pub sale_id: Option<i64>,
    pub amount_minor: i64,
    pub method: &'a str,
    pub note: Option<&'a str>,
    pub received_by: Option<i64>,
    pub shift_id: Option<i64>,
}

/// Records an installment. Refuses more than is owed, on that sale or in
/// total.
pub fn record_payment(
    tx: &Transaction,
    input: PaymentInput,
) -> Result<Vec<CustomerPayment>, CustomerError> {
    if input.amount_minor <= 0 {
        return Err(CustomerError::Invalid(
            "Enter an amount greater than zero".into(),
        ));
    }
    if !PAYMENT_METHODS.contains(&input.method) {
        return Err(CustomerError::Invalid(format!(
            "Unknown payment method: {}",
            input.method
        )));
    }
    get_customer(tx, input.customer_id)?;
    let mut outstanding = list_credit_sales(tx, Some(input.customer_id), true)?;
    if let Some(sale_id) = input.sale_id {
        outstanding.retain(|sale| sale.sale_id == sale_id);
        if outstanding.is_empty() {
            return Err(CustomerError::Invalid(format!(
                "Sale #{sale_id} has nothing left to pay"
            )));
        }
    }
    let owed: i64 = outstanding.iter().map(|sale| sale.outstanding_minor).sum();
    if input.amount_minor > owed {
        return Err(CustomerError::Invalid(format!(
            "That is more than the {} owed",
            crate::db::format_minor_plain(owed)
        )));
    }

    let note = input.note.map(str::trim).filter(|s| !s.is_empty());
    let mut remaining = input.amount_minor;
    let mut ids = Vec::new();
    for sale in outstanding {
        if remaining == 0 {
            break;
        }
        let applied = remaining.min(sale.outstanding_minor);
        tx.execute(
            "INSERT INTO customer_payments (customer_id, sale_id, amount_minor, method, note, received_by, shift_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![input.customer_id, sale.sale_id, applied, input.method, note, input.received_by, input.shift_id],
        )?;
        ids.push(tx.last_insert_rowid());
        remaining -= applied;
    }
    let all = list_payments(tx, input.customer_id)?;
    Ok(all.into_iter().filter(|p| ids.contains(&p.id)).collect())
}

pub fn list_payments(
    conn: &Connection,
    customer_id: i64,
) -> Result<Vec<CustomerPayment>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.customer_id, p.sale_id, p.amount_minor, p.method, p.note, u.name, p.created_at
           FROM customer_payments p
           LEFT JOIN users u ON u.id = p.received_by
          WHERE p.customer_id = ?1
          ORDER BY p.id DESC",
    )?;
    let rows = stmt.query_map(params![customer_id], |row| {
        Ok(CustomerPayment {
            id: row.get(0)?,
            customer_id: row.get(1)?,
            sale_id: row.get(2)?,
            amount_minor: row.get(3)?,
            method: row.get(4)?,
            note: row.get(5)?,
            received_by_name: row.get(6)?,
            created_at: row.get(7)?,
        })
    })?;
    rows.collect()
}

/// How much of refund `refund_id` actually went back to the customer — the
/// part not absorbed by unpaid credit on its sale. Earlier refunds on the
/// same sale absorb first.
pub fn refund_cash_out(conn: &Connection, refund_id: i64) -> Result<i64, rusqlite::Error> {
    let (sale_id, amount): (i64, i64) = conn.query_row(
        "SELECT original_sale_id, total_refund_amount_minor FROM refunds WHERE id = ?1",
        params![refund_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let (credit, paid, earlier): (i64, i64, i64) = conn.query_row(
        "SELECT (SELECT COALESCE(SUM(amount_minor), 0) FROM sale_payments WHERE sale_id = ?1 AND method = 'credit'),
                (SELECT COALESCE(SUM(amount_minor), 0) FROM customer_payments WHERE sale_id = ?1),
                (SELECT COALESCE(SUM(total_refund_amount_minor), 0) FROM refunds
                  WHERE original_sale_id = ?1 AND id < ?2)",
        params![sale_id, refund_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let unpaid_before = (credit - paid - earlier).max(0);
    Ok(amount - amount.min(unpaid_before))
}

/// A running statement from `events` already in date order (each entry's
/// `balance_minor` is filled in here). Entries before `start_date` fold into
/// the opening balance; entries after `end_date` are left off. Shared by
/// the customer and supplier ledgers, compared on the `YYYY-MM-DD` prefix
/// so timestamps and plain dates work alike.
pub(crate) fn fold_statement(
    events: Vec<LedgerEntry>,
    start_date: Option<&str>,
    end_date: Option<&str>,
) -> Statement {
    let mut opening = 0;
    let mut balance = 0;
    let mut entries = Vec::new();
    for mut entry in events {
        let day = entry.date.get(..10).unwrap_or(&entry.date);
        let movement = entry.debit_minor - entry.credit_minor;
        if start_date.is_some_and(|start| day < start) {
            opening += movement;
            balance = opening;
            continue;
        }
        if end_date.is_some_and(|end| day > end) {
            break;
        }
        balance += movement;
        entry.balance_minor = balance;
        entries.push(entry);
    }
    Statement {
        opening_balance_minor: opening,
        closing_balance_minor: balance,
        entries,
    }
}

fn entry(
    date: String,
    kind: &str,
    reference: String,
    description: String,
    debit_minor: i64,
    credit_minor: i64,
) -> LedgerEntry {
    LedgerEntry {
        date,
        kind: kind.into(),
        reference,
        description,
        debit_minor,
        credit_minor,
        balance_minor: 0,
    }
}

/// A customer's running statement. Without a range it covers everything;
/// with one, entries before `start_date` fold into the opening balance.
pub fn statement(
    conn: &Connection,
    customer_id: i64,
    start_date: Option<&str>,
    end_date: Option<&str>,
) -> Result<Statement, CustomerError> {
    get_customer(conn, customer_id)?;

    // (tiebreak within the same second: sale, then payment, then refund)
    let mut events: Vec<(i64, LedgerEntry)> = Vec::new();
    let mut refunds = conn.prepare(
        "SELECT id, created_at, total_refund_amount_minor FROM refunds WHERE original_sale_id = ?1 ORDER BY id",
    )?;
    for sale in list_credit_sales(conn, Some(customer_id), false)? {
        let due = sale
            .due_date
            .as_deref()
            .map(|d| format!(" — due {d}"))
            .unwrap_or_default();
        events.push((
            0,
            entry(
                sale.created_at.clone(),
                "sale",
                format!("Sale #{}", sale.sale_id),
                format!("Pay-later sale{due}"),
                sale.credit_minor,
                0,
            ),
        ));
        let rows = refunds
            .query_map(params![sale.sale_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (refund_id, at, amount) in rows {
            let absorbed = amount - refund_cash_out(conn, refund_id)?;
            if absorbed > 0 {
                events.push((
                    2,
                    entry(
                        at,
                        "refund",
                        format!("Refund #{refund_id}"),
                        format!("Return against sale #{}", sale.sale_id),
                        0,
                        absorbed,
                    ),
                ));
            }
        }
    }
    for payment in list_payments(conn, customer_id)? {
        let note = payment
            .note
            .as_deref()
            .map(|n| format!(" — {n}"))
            .unwrap_or_default();
        let target = payment
            .sale_id
            .map(|id| format!(" for sale #{id}"))
            .unwrap_or_default();
        events.push((
            1,
            entry(
                payment.created_at.clone(),
                "payment",
                format!("Payment #{}", payment.id),
                format!("Installment ({}){target}{note}", payment.method),
                0,
                payment.amount_minor,
            ),
        ));
    }
    events.sort_by(|a, b| (&a.1.date, a.0).cmp(&(&b.1.date, b.0)));
    Ok(fold_statement(
        events.into_iter().map(|(_, e)| e).collect(),
        start_date,
        end_date,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::refunds::{create_refund, CreateRefundInput, RefundLineInput};
    use crate::db::sales::{create_sale, PaymentPart, SaleError};
    use crate::db::schema::test_conn;
    use crate::db::test_support::{in_tx, item_id, one_line_sale};

    fn input(name: &str) -> CustomerInput {
        CustomerInput {
            name: name.into(),
            phone: None,
            email: None,
            address: None,
            notes: None,
        }
    }

    fn walk_in(conn: &Connection) -> i64 {
        add_customer(
            conn,
            CustomerInput {
                name: "Bilal".into(),
                phone: Some("0300 1234567".into()),
                email: None,
                address: None,
                notes: None,
            },
        )
        .unwrap()
        .id
    }

    fn credit_sale(
        conn: &mut Connection,
        customer: i64,
        cash: i64,
        due: &str,
    ) -> crate::db::sales::Sale {
        let cola = item_id(conn, "Cola 500ml");
        in_tx(conn, |tx| {
            let price: i64 = tx
                .query_row("SELECT price_minor FROM items WHERE id = ?1", [cola], |r| {
                    r.get(0)
                })
                .unwrap();
            let mut payments = vec![PaymentPart {
                method: "credit".into(),
                amount_minor: price - cash,
            }];
            if cash > 0 {
                payments.push(PaymentPart {
                    method: "cash".into(),
                    amount_minor: cash,
                });
            }
            create_sale(
                tx,
                crate::db::sales::CreateSaleInput {
                    payments,
                    customer_id: Some(customer),
                    due_date: Some(due.into()),
                    ..one_line_sale(cola, 1.0)
                },
            )
        })
        .unwrap()
    }

    #[test]
    fn a_split_credit_sale_is_owed_until_installments_cover_it() {
        let mut conn = test_conn();
        let customer = walk_in(&conn);
        let sale = credit_sale(&mut conn, customer, 3000, "2099-01-01");
        assert_eq!(sale.payment_method, "split");
        let owed = sale.total_minor - 3000;
        assert_eq!(get_customer(&conn, customer).unwrap().balance_minor, owed);

        in_tx(&mut conn, |tx| {
            record_payment(
                tx,
                PaymentInput {
                    customer_id: customer,
                    sale_id: None,
                    amount_minor: 1000,
                    method: "cash",
                    note: None,
                    received_by: None,
                    shift_id: None,
                },
            )
        })
        .unwrap();
        assert_eq!(
            get_customer(&conn, customer).unwrap().balance_minor,
            owed - 1000
        );

        let too_much = in_tx(&mut conn, |tx| {
            record_payment(
                tx,
                PaymentInput {
                    customer_id: customer,
                    sale_id: Some(sale.id),
                    amount_minor: owed,
                    method: "cash",
                    note: None,
                    received_by: None,
                    shift_id: None,
                },
            )
        });
        assert!(matches!(too_much, Err(CustomerError::Invalid(_))));

        let statement = statement(&conn, customer, None, None).unwrap();
        assert_eq!(statement.entries.len(), 2);
        assert_eq!(statement.closing_balance_minor, owed - 1000);
    }

    #[test]
    fn credit_needs_a_customer_and_a_due_date_and_must_add_up() {
        let mut conn = test_conn();
        let cola = item_id(&conn, "Cola 500ml");
        let no_customer = in_tx(&mut conn, |tx| {
            create_sale(
                tx,
                crate::db::sales::CreateSaleInput {
                    payments: vec![PaymentPart {
                        method: "credit".into(),
                        amount_minor: 8000,
                    }],
                    due_date: Some("2099-01-01".into()),
                    ..one_line_sale(cola, 1.0)
                },
            )
        });
        assert!(matches!(no_customer, Err(SaleError::InvalidPayments(_))));

        let customer = walk_in(&conn);
        let short = in_tx(&mut conn, |tx| {
            create_sale(
                tx,
                crate::db::sales::CreateSaleInput {
                    payments: vec![PaymentPart {
                        method: "cash".into(),
                        amount_minor: 1,
                    }],
                    customer_id: Some(customer),
                    ..one_line_sale(cola, 1.0)
                },
            )
        });
        assert!(matches!(short, Err(SaleError::InvalidPayments(_))));
    }

    #[test]
    fn an_overdue_sale_is_flagged_and_a_refund_reduces_what_is_owed_first() {
        let mut conn = test_conn();
        let customer = walk_in(&conn);
        let sale = credit_sale(&mut conn, customer, 0, "2000-01-01");
        let summary = overdue_summary(&conn).unwrap();
        assert_eq!(summary.sale_count, 1);
        assert_eq!(summary.amount_minor, sale.total_minor);

        let sale_item_id = crate::db::test_support::sale_item_id(&conn, sale.id);
        let refund = in_tx(&mut conn, |tx| {
            create_refund(
                tx,
                CreateRefundInput {
                    sale_id: sale.id,
                    refunded_by: None,
                    reason: None,
                    items: vec![RefundLineInput {
                        sale_item_id,
                        qty: 1.0,
                        amount_minor: sale.total_minor,
                    }],
                },
            )
        })
        .unwrap();
        assert_eq!(
            refund_cash_out(&conn, refund.id).unwrap(),
            0,
            "nothing was paid, so nothing goes back"
        );
        assert_eq!(get_customer(&conn, customer).unwrap().balance_minor, 0);
        assert!(set_active(&conn, customer, false).is_ok());
    }

    fn payment(
        customer_id: i64,
        sale_id: Option<i64>,
        amount_minor: i64,
        method: &'static str,
    ) -> PaymentInput<'static> {
        PaymentInput {
            customer_id,
            sale_id,
            amount_minor,
            method,
            note: Some("  "),
            received_by: None,
            shift_id: None,
        }
    }

    #[test]
    fn customers_validate_update_and_search() {
        let conn = test_conn();
        let blank = CustomerInput {
            name: "  ".into(),
            phone: None,
            email: None,
            address: None,
            notes: None,
        };
        assert!(matches!(
            add_customer(&conn, blank),
            Err(CustomerError::NameRequired)
        ));
        let bad_email = CustomerInput {
            email: Some("not an email".into()),
            ..input("Sana")
        };
        assert!(matches!(
            add_customer(&conn, bad_email),
            Err(CustomerError::Invalid(_))
        ));

        let id = walk_in(&conn);
        let updated = update_customer(
            &conn,
            id,
            CustomerInput {
                email: Some(" bilal@example.com ".into()),
                phone: Some(" ".into()),
                ..input("Bilal Khan")
            },
        )
        .unwrap();
        assert_eq!(updated.name, "Bilal Khan");
        assert_eq!(updated.email.as_deref(), Some("bilal@example.com"));
        assert_eq!(updated.phone, None, "blank is stored as NULL");
        assert!(matches!(
            update_customer(&conn, 999_999, input("X")),
            Err(CustomerError::NotFound)
        ));

        add_customer(&conn, input("Ayesha")).unwrap();
        let by_email = list_customers(&conn, Some("example.com"), false, 50).unwrap();
        assert_eq!(by_email.iter().map(|c| c.id).collect::<Vec<_>>(), [id]);
        set_active(&conn, id, false).unwrap();
        assert!(
            list_customers(&conn, Some("Bilal"), false, 50)
                .unwrap()
                .is_empty(),
            "archived are hidden"
        );
        assert_eq!(
            list_customers(&conn, Some("Bilal"), true, 50)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn installments_refuse_bad_amounts_methods_customers_and_settled_bills() {
        let mut conn = test_conn();
        let customer = walk_in(&conn);
        let sale = credit_sale(&mut conn, customer, 0, "2099-01-01");

        let mut attempt =
            |input: PaymentInput<'static>| in_tx(&mut conn, |tx| record_payment(tx, input));
        assert!(matches!(
            attempt(payment(customer, None, 0, "cash")),
            Err(CustomerError::Invalid(_))
        ));
        assert!(matches!(
            attempt(payment(customer, None, 100, "cheque")),
            Err(CustomerError::Invalid(_))
        ));
        assert!(matches!(
            attempt(payment(999_999, None, 100, "cash")),
            Err(CustomerError::NotFound)
        ));

        let paid = attempt(payment(customer, Some(sale.id), sale.total_minor, "card")).unwrap();
        assert_eq!(paid.len(), 1);
        assert_eq!(paid[0].note, None, "a blank note is not stored");
        let again = attempt(payment(customer, Some(sale.id), 1, "cash"));
        assert!(
            matches!(again, Err(CustomerError::Invalid(ref m)) if m.contains("nothing left")),
            "{again:?}"
        );

        assert_eq!(list_payments(&conn, customer).unwrap().len(), 1);
        assert_eq!(get_customer(&conn, customer).unwrap().balance_minor, 0);
    }

    #[test]
    fn an_on_account_payment_settles_the_oldest_due_bill_first() {
        let mut conn = test_conn();
        let customer = walk_in(&conn);
        let later = credit_sale(&mut conn, customer, 0, "2099-06-01");
        let sooner = credit_sale(&mut conn, customer, 0, "2099-01-01");
        let paid = in_tx(&mut conn, |tx| {
            record_payment(
                tx,
                payment(customer, None, sooner.total_minor + 100, "cash"),
            )
        })
        .unwrap();
        assert_eq!(paid.len(), 2, "spills over onto the next bill");
        let bills = list_credit_sales(&conn, Some(customer), false).unwrap();
        let outstanding = |id: i64| {
            bills
                .iter()
                .find(|b| b.sale_id == id)
                .unwrap()
                .outstanding_minor
        };
        assert_eq!(outstanding(sooner.id), 0);
        assert_eq!(outstanding(later.id), later.total_minor - 100);
    }
}
