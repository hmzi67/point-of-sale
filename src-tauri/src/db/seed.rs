//! Demo data for development and client demos.
//!
//! Runs only when the operational tables are completely empty, and only in a
//! debug build or when `POS_SEED_DEMO=1` is set — a client's fresh install must
//! never come pre-loaded with fake stock. Everything is written in one
//! transaction, so a failure leaves an empty database rather than half a shop.
//!
//! Dates are generated relative to today, so the demo always looks like a shop
//! that traded this week no matter when the app is run.

use chrono::{Duration, Local, NaiveDate};
use rusqlite::{params, Connection};

/// Whole rupees to minor units. Demo figures read naturally at the call site.
const fn rupees(amount: i64) -> i64 {
    amount * 100
}

/// Tax rate the demo shop charges, used only when the installation has not
/// configured one of its own.
const DEMO_TAX_PERCENT: f64 = 5.0;

/// (name, barcode, price, cost, stock, category index, low-stock threshold)
const ITEMS: &[(&str, &str, i64, i64, i64, usize, i64)] = &[
    // Beverages
    ("Cola 500ml", "8901234500011", rupees(80), rupees(58), 48, 0, 12),
    ("Orange Juice 1L", "8901234500028", rupees(220), rupees(170), 20, 0, 6),
    ("Mineral Water 1.5L", "8901234500035", rupees(60), rupees(40), 72, 0, 24),
    ("Green Tea (25 bags)", "8901234500042", rupees(340), rupees(255), 14, 0, 4),
    ("Energy Drink 250ml", "8901234500059", rupees(150), rupees(115), 30, 0, 8),
    // Snacks
    ("Salted Chips 60g", "8901234500066", rupees(50), rupees(34), 96, 1, 24),
    ("Chocolate Bar 40g", "8901234500073", rupees(120), rupees(88), 60, 1, 15),
    ("Biscuits Family Pack", "8901234500080", rupees(180), rupees(132), 36, 1, 10),
    ("Roasted Peanuts 100g", "8901234500097", rupees(90), rupees(62), 40, 1, 10),
    ("Instant Noodles", "8901234500103", rupees(70), rupees(52), 5, 1, 20), // deliberately low
    // Grocery
    ("Basmati Rice 5kg", "8901234500110", rupees(2450), rupees(2100), 12, 2, 3),
    ("Cooking Oil 1L", "8901234500127", rupees(650), rupees(560), 18, 2, 6),
    ("Sugar 1kg", "8901234500134", rupees(160), rupees(138), 25, 2, 8),
    ("Wheat Flour 10kg", "8901234500141", rupees(1350), rupees(1180), 8, 2, 4),
    ("Tea Leaves 400g", "8901234500158", rupees(980), rupees(830), 2, 2, 5), // deliberately low
];

const CATEGORIES: &[&str] = &["Beverages", "Snacks", "Grocery"];

/// (name, role, contact, monthly base salary)
const EMPLOYEES: &[(&str, &str, &str, i64)] = &[
    ("Ahmed Raza", "Cashier", "0300-1234567", rupees(45_000)),
    ("Sana Khalid", "Sales Staff", "0301-2345678", rupees(38_000)),
    ("Bilal Ahmed", "Helper", "0302-3456789", rupees(30_000)),
];

/// (days ago, hour, payment method, discount, [(item index, qty)])
const SALES: &[(i64, u32, &str, i64, &[(usize, i64)])] = &[
    (12, 10, "cash", 0, &[(0, 2), (5, 3)]),
    (12, 17, "card", 0, &[(10, 1), (12, 2)]),
    (9, 11, "cash", rupees(20), &[(6, 4), (8, 2), (2, 3)]),
    (9, 19, "cash", 0, &[(13, 1)]),
    (6, 9, "card", 0, &[(11, 2), (12, 1), (7, 1)]),
    (6, 14, "cash", 0, &[(3, 1), (1, 2)]),
    (6, 20, "other", rupees(50), &[(10, 1), (11, 1), (14, 1)]),
    (3, 12, "cash", 0, &[(4, 2), (6, 2)]),
    (3, 18, "card", 0, &[(9, 3), (5, 2), (0, 1)]),
    (1, 13, "cash", 0, &[(7, 2), (8, 1), (2, 2)]),
    (1, 16, "cash", rupees(15), &[(12, 3), (5, 4)]),
    (0, 11, "card", 0, &[(11, 1), (0, 3), (6, 1)]),
];

/// (days ago, category, amount, note)
const EXPENSES: &[(i64, &str, i64, &str)] = &[
    (12, "Rent", rupees(35_000), "Shop rent — current month"),
    (9, "Utilities", rupees(8_400), "Electricity bill"),
    (7, "Supplies", rupees(2_150), "Carry bags and receipt rolls"),
    (5, "Transport", rupees(1_800), "Stock pickup from wholesale market"),
    (3, "Maintenance", rupees(3_500), "Chiller servicing"),
    (1, "Supplies", rupees(950), "Cleaning supplies"),
];

/// True when the operational tables hold nothing at all.
fn is_empty(conn: &Connection) -> Result<bool, rusqlite::Error> {
    let count: i64 = conn.query_row(
        "SELECT (SELECT COUNT(*) FROM items)
              + (SELECT COUNT(*) FROM sales)
              + (SELECT COUNT(*) FROM employees)
              + (SELECT COUNT(*) FROM expenses)",
        [],
        |row| row.get(0),
    )?;
    Ok(count == 0)
}

/// Demo data is a development convenience, never something a paying client's
/// fresh install should show.
fn demo_seeding_allowed() -> bool {
    cfg!(debug_assertions) || std::env::var("POS_SEED_DEMO").as_deref() == Ok("1")
}

/// Tax on a taxable base. The rate is inherently fractional, but the result is
/// rounded back to whole minor units immediately so no fraction is ever stored.
fn tax_on(base_minor: i64, tax_percent: f64) -> i64 {
    (base_minor as f64 * tax_percent / 100.0).round() as i64
}

fn date_days_ago(days: i64) -> NaiveDate {
    (Local::now() - Duration::days(days)).date_naive()
}

fn timestamp(days_ago: i64, hour: u32) -> String {
    format!("{} {:02}:00:00", date_days_ago(days_ago), hour)
}

/// Seeds demo data if the database is empty. Returns whether it wrote anything.
pub fn seed_demo_data(conn: &Connection) -> Result<bool, rusqlite::Error> {
    if !demo_seeding_allowed() || !is_empty(conn)? {
        return Ok(false);
    }

    let tx = conn.unchecked_transaction()?;

    // Seeding can happen on an install that already went through onboarding
    // (Phase 1 config exists, operational tables are still empty), so never
    // overwrite a rate the client has already set — only fill in a missing one.
    let configured_tax: f64 =
        tx.query_row("SELECT tax_percent FROM app_config WHERE id = 1", [], |row| {
            row.get(0)
        })?;
    let tax_percent = if configured_tax > 0.0 {
        configured_tax
    } else {
        tx.execute(
            "UPDATE app_config SET tax_percent = ?1 WHERE id = 1",
            params![DEMO_TAX_PERCENT],
        )?;
        DEMO_TAX_PERCENT
    };

    // --- Categories and items ------------------------------------------------
    let mut category_ids = Vec::new();
    for name in CATEGORIES {
        tx.execute("INSERT INTO categories (name) VALUES (?1)", params![name])?;
        category_ids.push(tx.last_insert_rowid());
    }

    let mut item_ids = Vec::new();
    for (name, barcode, price, cost, stock, category, threshold) in ITEMS {
        tx.execute(
            "INSERT INTO items
                 (name, barcode, price_minor, cost_minor, stock_qty, category_id, low_stock_threshold)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![name, barcode, price, cost, stock, category_ids[*category], threshold],
        )?;
        item_ids.push(tx.last_insert_rowid());
    }

    // --- Restaurant tables ---------------------------------------------------
    // Seeded even for a retail demo so the tables module has something to show
    // the moment a client switches it on.
    for (index, status) in ["occupied", "free", "free", "reserved", "free", "occupied"]
        .iter()
        .enumerate()
    {
        tx.execute(
            "INSERT INTO tables (name, seats, status) VALUES (?1, ?2, ?3)",
            params![format!("Table {}", index + 1), 4, status],
        )?;
    }

    // --- Employees -----------------------------------------------------------
    let mut employee_ids = Vec::new();
    for (name, role, contact, base_salary) in EMPLOYEES {
        tx.execute(
            "INSERT INTO employees (name, role, contact, base_salary_minor)
             VALUES (?1, ?2, ?3, ?4)",
            params![name, role, contact, base_salary],
        )?;
        employee_ids.push(tx.last_insert_rowid());
    }

    // --- Sales, their lines, and the matching stock movement ------------------
    let cashier_id: Option<i64> = tx
        .query_row(
            "SELECT id FROM users ORDER BY id LIMIT 1",
            [],
            |row| row.get(0),
        )
        .ok();

    for (days_ago, hour, payment_method, discount, lines) in SALES {
        let subtotal: i64 = lines
            .iter()
            .map(|(index, qty)| ITEMS[*index].2 * qty)
            .sum();
        let taxable = (subtotal - discount).max(0);
        let tax = tax_on(taxable, tax_percent);

        tx.execute(
            "INSERT INTO sales
                 (total_minor, discount_minor, tax_minor, payment_method, cashier_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                taxable + tax,
                discount,
                tax,
                payment_method,
                cashier_id,
                timestamp(*days_ago, *hour)
            ],
        )?;
        let sale_id = tx.last_insert_rowid();

        for (index, qty) in lines.iter() {
            tx.execute(
                "INSERT INTO sale_items (sale_id, item_id, qty, price_at_sale_minor)
                 VALUES (?1, ?2, ?3, ?4)",
                params![sale_id, item_ids[*index], qty, ITEMS[*index].2],
            )?;
            // Stock on hand must agree with the sales history, or the demo's
            // inventory screen contradicts its reports.
            tx.execute(
                "UPDATE items SET stock_qty = stock_qty - ?1 WHERE id = ?2",
                params![qty, item_ids[*index]],
            )?;
        }
    }

    // --- Attendance: last 10 days, one shift a day, with a couple of absences -
    for (employee_index, employee_id) in employee_ids.iter().enumerate() {
        for days_ago in 0..10 {
            // A staggered absence per employee, so the monthly summary is not
            // a wall of identical rows.
            if days_ago % (4 + employee_index as i64) == 0 && days_ago != 0 {
                continue;
            }

            let date = date_days_ago(days_ago);
            let check_in = format!("{} 09:{:02}:00", date, employee_index * 5);
            // Today's shift is still open for the first employee.
            let check_out = if days_ago == 0 && employee_index == 0 {
                None
            } else {
                Some(format!("{} 18:{:02}:00", date, 10 + employee_index * 5))
            };

            tx.execute(
                "INSERT INTO attendance (employee_id, work_date, check_in, check_out)
                 VALUES (?1, ?2, ?3, ?4)",
                params![employee_id, date.to_string(), check_in, check_out],
            )?;
        }
    }

    // --- Expenses ------------------------------------------------------------
    for (days_ago, category, amount, note) in EXPENSES {
        tx.execute(
            "INSERT INTO expenses (expense_date, category, amount_minor, note)
             VALUES (?1, ?2, ?3, ?4)",
            params![date_days_ago(*days_ago).to_string(), category, amount, note],
        )?;
    }

    // --- Last month's payroll, already paid ----------------------------------
    let last_month = (Local::now() - Duration::days(30)).format("%Y-%m").to_string();
    for (index, employee_id) in employee_ids.iter().enumerate() {
        let base = EMPLOYEES[index].3;
        tx.execute(
            "INSERT INTO salary_payments
                 (employee_id, month, calculated_amount_minor, paid_amount_minor, paid_date)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                employee_id,
                last_month,
                base,
                base,
                date_days_ago(25).to_string()
            ],
        )?;
    }

    tx.commit()?;
    println!("[db] seeded demo data ({} items, {} sales)", ITEMS.len(), SALES.len());

    Ok(true)
}

#[cfg(test)]
pub(crate) fn demo_expense_count() -> i64 {
    EXPENSES.len() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::test_conn;

    fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {}", table), [], |row| {
            row.get(0)
        })
        .unwrap()
    }

    /// Known-value checks (not just "recomputed the same way twice") that
    /// `tax_on` rounds to the nearest minor unit rather than truncating —
    /// money must never accumulate rounding drift by consistently rounding
    /// one direction.
    #[test]
    fn tax_on_rounds_to_the_nearest_minor_unit() {
        assert_eq!(tax_on(1000, 12.5), 125, "exact case: 1000 * 12.5% = 125.0");
        assert_eq!(tax_on(333, 1.5), 5, "333 * 1.5% = 4.995, rounds up to 5");
        assert_eq!(tax_on(100, 7.0), 7, "100 * 7% = 7.0 exactly");
        assert_eq!(tax_on(999, 5.0), 50, "999 * 5% = 49.95, rounds up to 50");
        assert_eq!(tax_on(100, 12.5), 13, "exact .5 tie (100 * 12.5% = 12.5) rounds away from zero, to 13");
        assert_eq!(tax_on(0, 15.0), 0, "no tax on a zero base");
        assert_eq!(tax_on(100, 0.0), 0, "no tax at a zero rate");
    }

    #[test]
    fn seeds_a_shop_worth_of_demo_data() {
        let conn = test_conn();

        assert_eq!(count(&conn, "categories"), CATEGORIES.len() as i64);
        assert_eq!(count(&conn, "items"), ITEMS.len() as i64);
        assert_eq!(count(&conn, "employees"), EMPLOYEES.len() as i64);
        assert_eq!(count(&conn, "sales"), SALES.len() as i64);
        assert_eq!(count(&conn, "expenses"), EXPENSES.len() as i64);
        assert!(count(&conn, "sale_items") > SALES.len() as i64);
        assert!(count(&conn, "attendance") > 0);
        assert!(count(&conn, "salary_payments") > 0);
    }

    /// Schema only — no demo data yet — so a test can set config first.
    fn bare_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        conn.execute_batch(include_str!("schema.sql")).unwrap();
        conn.execute("INSERT INTO app_config (id) VALUES (1)", []).unwrap();
        conn
    }

    #[test]
    fn keeps_a_tax_rate_the_client_already_configured() {
        let conn = bare_conn();
        conn.execute("UPDATE app_config SET tax_percent = 3.0 WHERE id = 1", [])
            .unwrap();

        assert!(seed_demo_data(&conn).unwrap());

        let tax: f64 = conn
            .query_row("SELECT tax_percent FROM app_config", [], |row| row.get(0))
            .unwrap();
        assert_eq!(tax, 3.0, "onboarding config must survive demo seeding");

        // ...and the seeded sales used that rate, not the demo default.
        let (total, discount, tax_minor): (i64, i64, i64) = conn
            .query_row(
                "SELECT total_minor, discount_minor, tax_minor FROM sales ORDER BY id LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        let subtotal = total + discount - tax_minor;
        assert_eq!(tax_minor, tax_on(subtotal - discount, 3.0));
    }

    #[test]
    fn fills_in_a_tax_rate_when_none_is_configured() {
        let conn = bare_conn();
        assert!(seed_demo_data(&conn).unwrap());

        let tax: f64 = conn
            .query_row("SELECT tax_percent FROM app_config", [], |row| row.get(0))
            .unwrap();
        assert_eq!(tax, DEMO_TAX_PERCENT);
    }

    #[test]
    fn does_not_seed_twice() {
        let conn = test_conn();
        assert!(!seed_demo_data(&conn).unwrap(), "second run must be a no-op");
        assert_eq!(count(&conn, "items"), ITEMS.len() as i64);
    }

    #[test]
    fn sale_totals_are_internally_consistent() {
        let conn = test_conn();
        let mismatches: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sales s
                  WHERE s.total_minor <> (
                        SELECT COALESCE(SUM(si.qty * si.price_at_sale_minor), 0)
                          FROM sale_items si WHERE si.sale_id = s.id
                      ) - s.discount_minor + s.tax_minor",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(mismatches, 0, "seeded totals must equal lines - discount + tax");
    }

    #[test]
    fn stock_reflects_the_seeded_sales() {
        let conn = test_conn();
        // Every unit sold was decremented from stock, so the demo's inventory
        // screen and its reports tell the same story.
        for (index, (name, _, _, _, opening_stock, _, _)) in ITEMS.iter().enumerate() {
            let sold: i64 = SALES
                .iter()
                .flat_map(|(_, _, _, _, lines)| lines.iter())
                .filter(|(item_index, _)| *item_index == index)
                .map(|(_, qty)| qty)
                .sum();

            let stock: f64 = conn
                .query_row("SELECT stock_qty FROM items WHERE name = ?1", [name], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(stock, (opening_stock - sold) as f64, "stock wrong for {}", name);
        }
    }

    #[test]
    fn demo_data_includes_low_stock_items_to_exercise_alerts() {
        let conn = test_conn();
        let low: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM items WHERE stock_qty <= low_stock_threshold",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(low > 0, "expected at least one item under its threshold");
    }
}
