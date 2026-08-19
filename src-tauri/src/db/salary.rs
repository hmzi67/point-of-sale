//! Salary calculation and payment tracking.
//!
//! One row per employee per month in `salary_payments` (`UNIQUE(employee_id,
//! month)`): `calculated_amount_minor` is derived from attendance —
//! `base_salary / working_days_per_month * days_present`, re-derived (and the
//! stored row refreshed) every time `calculate_salary` runs, so it always
//! reflects the latest attendance data — and `paid_amount_minor` accumulates
//! whatever has actually been paid out, since a month's pay is often settled
//! in more than one instalment. `working_days_per_month` lives on
//! `app_config` (see `db/config.rs`) rather than a hardcoded 26 or 30, since
//! that varies by client and by local labour convention.

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::db::attendance;

#[derive(Debug)]
pub enum SalaryError {
    EmployeeNotFound,
    InvalidMonth(String),
    InvalidAmount,
    InvalidDate(String),
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for SalaryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SalaryError::EmployeeNotFound => write!(f, "Employee not found"),
            SalaryError::InvalidMonth(msg) => write!(f, "{}", msg),
            SalaryError::InvalidAmount => write!(f, "Payment amount must be greater than zero"),
            SalaryError::InvalidDate(msg) => write!(f, "{}", msg),
            SalaryError::Sqlite(err) => write!(f, "database error: {}", err),
        }
    }
}

impl From<rusqlite::Error> for SalaryError {
    fn from(err: rusqlite::Error) -> Self {
        SalaryError::Sqlite(err)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PaymentStatus {
    Unpaid,
    Partial,
    Paid,
}

fn status_for(calculated_minor: i64, paid_minor: i64) -> PaymentStatus {
    if paid_minor <= 0 {
        PaymentStatus::Unpaid
    } else if paid_minor < calculated_minor {
        PaymentStatus::Partial
    } else {
        PaymentStatus::Paid
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SalaryCalculation {
    pub employee_id: i64,
    pub employee_name: String,
    /// `YYYY-MM`.
    pub month: String,
    pub base_salary_minor: i64,
    pub working_days_in_month: i64,
    pub days_present: i64,
    pub calculated_amount_minor: i64,
    pub paid_amount_minor: i64,
    pub paid_date: Option<String>,
    pub status: PaymentStatus,
}

fn validate_month(month: &str) -> Result<(), SalaryError> {
    chrono::NaiveDate::parse_from_str(&format!("{}-01", month), "%Y-%m-%d")
        .map(|_| ())
        .map_err(|_| SalaryError::InvalidMonth(format!("Invalid month: {} (expected YYYY-MM)", month)))
}

fn validate_date(date: &str) -> Result<(), SalaryError> {
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map(|_| ())
        .map_err(|_| SalaryError::InvalidDate(format!("Invalid date: {}", date)))
}

struct EmployeeRow {
    id: i64,
    name: String,
    base_salary_minor: i64,
}

fn find_active_employee(conn: &Connection, employee_id: i64) -> Result<EmployeeRow, SalaryError> {
    conn.query_row(
        "SELECT id, name, base_salary_minor FROM employees WHERE id = ?1 AND is_active = 1",
        params![employee_id],
        |row| {
            Ok(EmployeeRow { id: row.get(0)?, name: row.get(1)?, base_salary_minor: row.get(2)? })
        },
    )
    .optional()?
    .ok_or(SalaryError::EmployeeNotFound)
}

/// Days present for `employee_id` in `month`. Always resolves — the
/// underlying monthly summary is a `LEFT JOIN` over every active employee, so
/// an employee with zero attendance that month simply comes back as 0, never
/// missing.
fn days_present_for(conn: &Connection, employee_id: i64, month: &str) -> Result<i64, SalaryError> {
    let summaries = attendance::get_monthly_summary(conn, month)
        .map_err(|e| SalaryError::InvalidMonth(e.to_string()))?;
    Ok(summaries.iter().find(|s| s.employee_id == employee_id).map(|s| s.days_present).unwrap_or(0))
}

fn existing_payment_row(
    conn: &Connection,
    employee_id: i64,
    month: &str,
) -> Result<Option<(i64, Option<String>)>, rusqlite::Error> {
    conn.query_row(
        "SELECT paid_amount_minor, paid_date FROM salary_payments WHERE employee_id = ?1 AND month = ?2",
        params![employee_id, month],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
}

/// Computes `base_salary / working_days_per_month * days_present`, rounded to
/// the nearest minor unit, and refreshes `calculated_amount_minor` on the
/// month's `salary_payments` row (creating it, with no payment yet, if this
/// is the first calculation) without touching any `paid_amount_minor`
/// already recorded.
pub fn calculate_salary(
    conn: &Connection,
    employee_id: i64,
    month: &str,
) -> Result<SalaryCalculation, SalaryError> {
    validate_month(month)?;
    let employee = find_active_employee(conn, employee_id)?;
    let working_days = crate::db::config::get(conn)?.working_days_per_month;
    let days_present = days_present_for(conn, employee.id, month)?;

    let calculated_minor = ((employee.base_salary_minor as f64) / (working_days as f64)
        * (days_present as f64))
        .round() as i64;

    conn.execute(
        "INSERT INTO salary_payments (employee_id, month, calculated_amount_minor)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(employee_id, month)
         DO UPDATE SET calculated_amount_minor = excluded.calculated_amount_minor",
        params![employee.id, month, calculated_minor],
    )?;

    let (paid_minor, paid_date) = existing_payment_row(conn, employee.id, month)?.unwrap_or((0, None));

    Ok(SalaryCalculation {
        employee_id: employee.id,
        employee_name: employee.name,
        month: month.to_string(),
        base_salary_minor: employee.base_salary_minor,
        working_days_in_month: working_days,
        days_present,
        calculated_amount_minor: calculated_minor,
        paid_amount_minor: paid_minor,
        paid_date,
        status: status_for(calculated_minor, paid_minor),
    })
}

/// The overview table: `calculate_salary` for every active employee, so the
/// screen always reflects the latest attendance the moment it loads.
pub fn get_monthly_overview(conn: &Connection, month: &str) -> Result<Vec<SalaryCalculation>, SalaryError> {
    validate_month(month)?;
    let employees = attendance::list_employees(conn).map_err(SalaryError::from)?;
    employees.iter().map(|e| calculate_salary(conn, e.id, month)).collect()
}

/// Records a payment against `month`'s salary — added to whatever has
/// already been paid that month (pay is often settled in more than one
/// instalment), with `paid_date` updated to this payment's date. Calculates
/// the month first if it hasn't been (e.g. a payment recorded before anyone
/// opened the overview screen), so `calculated_amount_minor` is never left at
/// its default of 0.
pub fn record_payment(
    conn: &Connection,
    employee_id: i64,
    month: &str,
    paid_amount_minor: i64,
    paid_date: &str,
) -> Result<SalaryCalculation, SalaryError> {
    validate_month(month)?;
    validate_date(paid_date)?;
    if paid_amount_minor <= 0 {
        return Err(SalaryError::InvalidAmount);
    }
    let employee = find_active_employee(conn, employee_id)?;

    // Ensures a row exists with an up-to-date calculated amount before
    // adding the payment on top of it.
    calculate_salary(conn, employee.id, month)?;

    conn.execute(
        "UPDATE salary_payments
            SET paid_amount_minor = paid_amount_minor + ?1,
                paid_date = ?2
          WHERE employee_id = ?3 AND month = ?4",
        params![paid_amount_minor, paid_date, employee.id, month],
    )?;

    calculate_salary(conn, employee.id, month)
}

/// Every month with a salary record for this employee, most recent first.
pub fn get_payment_history(conn: &Connection, employee_id: i64) -> Result<Vec<SalaryCalculation>, SalaryError> {
    let employee = find_active_employee(conn, employee_id)?;

    let mut stmt = conn.prepare(
        "SELECT month, calculated_amount_minor, paid_amount_minor, paid_date
           FROM salary_payments
          WHERE employee_id = ?1
          ORDER BY month DESC",
    )?;
    let rows: Vec<(String, i64, i64, Option<String>)> = stmt
        .query_map(params![employee.id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let working_days = crate::db::config::get(conn)?.working_days_per_month;

    rows.into_iter()
        .map(|(month, calculated_minor, paid_minor, paid_date)| {
            let days_present = days_present_for(conn, employee.id, &month)?;
            Ok(SalaryCalculation {
                employee_id: employee.id,
                employee_name: employee.name.clone(),
                month,
                base_salary_minor: employee.base_salary_minor,
                working_days_in_month: working_days,
                days_present,
                calculated_amount_minor: calculated_minor,
                paid_amount_minor: paid_minor,
                paid_date,
                status: status_for(calculated_minor, paid_minor),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::test_conn;

    fn employee_id(conn: &Connection, name: &str) -> i64 {
        conn.query_row("SELECT id FROM employees WHERE name = ?1", params![name], |row| row.get(0))
            .unwrap()
    }

    fn this_month() -> String {
        chrono::Local::now().format("%Y-%m").to_string()
    }

    #[test]
    fn calculate_salary_uses_base_salary_working_days_and_attendance() {
        let conn = test_conn();
        let id = employee_id(&conn, "Ahmed Raza");
        let month = this_month();

        let calc = calculate_salary(&conn, id, &month).unwrap();
        assert_eq!(calc.working_days_in_month, 26, "default working days per month");
        assert!(calc.days_present > 0, "seed data gives Ahmed attendance this month");
        let expected =
            ((calc.base_salary_minor as f64) / 26.0 * (calc.days_present as f64)).round() as i64;
        assert_eq!(calc.calculated_amount_minor, expected);
        assert_eq!(calc.paid_amount_minor, 0);
        assert!(matches!(calc.status, PaymentStatus::Unpaid));
    }

    /// A known-value check (not a self-referential recompute of the same
    /// formula) that picks numbers where rounding and truncation actually
    /// disagree: 100000 * 5 / 7 = 71428.5714…, which rounds to 71429 but
    /// would truncate to 71428 — proving the result is genuinely rounded to
    /// the nearest minor unit, not floored.
    #[test]
    fn calculate_salary_rounds_a_fractional_result_to_the_nearest_minor_unit() {
        let conn = test_conn();
        conn.execute("UPDATE app_config SET working_days_per_month = 7 WHERE id = 1", [])
            .unwrap();
        conn.execute(
            "INSERT INTO employees (name, role, base_salary_minor) VALUES ('Rounding Test', 'Staff', 100000)",
            [],
        )
        .unwrap();
        let id = conn.last_insert_rowid();

        let month = this_month();
        for day in 0..5 {
            conn.execute(
                "INSERT INTO attendance (employee_id, work_date, check_in, check_out)
                 VALUES (?1, date('now', 'localtime', 'start of month', ?2), '09:00:00', '18:00:00')",
                params![id, format!("+{} days", day)],
            )
            .unwrap();
        }

        let calc = calculate_salary(&conn, id, &month).unwrap();
        assert_eq!(calc.days_present, 5);
        assert_eq!(calc.working_days_in_month, 7);
        assert_eq!(calc.calculated_amount_minor, 71429, "71428.57... must round up, not truncate down to 71428");
    }

    #[test]
    fn calculate_salary_respects_a_configured_working_days_value() {
        let conn = test_conn();
        conn.execute("UPDATE app_config SET working_days_per_month = 22 WHERE id = 1", [])
            .unwrap();
        let id = employee_id(&conn, "Ahmed Raza");
        let calc = calculate_salary(&conn, id, &this_month()).unwrap();
        assert_eq!(calc.working_days_in_month, 22);
    }

    #[test]
    fn calculate_salary_changes_when_attendance_changes() {
        let conn = test_conn();
        let id = employee_id(&conn, "Ahmed Raza");
        let month = this_month();

        let before = calculate_salary(&conn, id, &month).unwrap();

        // Add an extra day present this month — the 1st, so it's always
        // inside the current month regardless of today's date. Cleared first
        // in case the seed data's "last 10 days" window already happens to
        // reach the 1st (days_present counts distinct work_date, so a
        // leftover duplicate row would silently not move the count).
        conn.execute(
            "DELETE FROM attendance WHERE employee_id = ?1 AND work_date = date('now', 'localtime', 'start of month')",
            params![id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO attendance (employee_id, work_date, check_in, check_out)
             VALUES (?1, date('now', 'localtime', 'start of month'), '09:00:00', '18:00:00')",
            params![id],
        )
        .unwrap();
        let after_add = calculate_salary(&conn, id, &month).unwrap();
        assert_eq!(after_add.days_present, before.days_present + 1);
        assert!(after_add.calculated_amount_minor > before.calculated_amount_minor);

        // Remove all of this employee's attendance for the month.
        conn.execute(
            "DELETE FROM attendance WHERE employee_id = ?1 AND substr(work_date, 1, 7) = ?2",
            params![id, month],
        )
        .unwrap();
        let after_remove = calculate_salary(&conn, id, &month).unwrap();
        assert_eq!(after_remove.days_present, 0);
        assert_eq!(after_remove.calculated_amount_minor, 0);
    }

    #[test]
    fn calculate_salary_rejects_an_unknown_employee_or_month() {
        let conn = test_conn();
        assert!(matches!(calculate_salary(&conn, 999_999, "2026-01"), Err(SalaryError::EmployeeNotFound)));
        let id = employee_id(&conn, "Ahmed Raza");
        assert!(matches!(calculate_salary(&conn, id, "not-a-month"), Err(SalaryError::InvalidMonth(_))));
    }

    #[test]
    fn get_monthly_overview_covers_every_active_employee() {
        let conn = test_conn();
        let overview = get_monthly_overview(&conn, &this_month()).unwrap();
        assert_eq!(overview.len(), 3);
    }

    #[test]
    fn record_payment_accumulates_and_updates_status() {
        let conn = test_conn();
        let id = employee_id(&conn, "Sana Khalid");
        let month = this_month();

        let calc = calculate_salary(&conn, id, &month).unwrap();
        assert!(calc.calculated_amount_minor > 0);

        let half = calc.calculated_amount_minor / 2;
        let after_first = record_payment(&conn, id, &month, half, "2026-01-10").unwrap();
        assert_eq!(after_first.paid_amount_minor, half);
        assert!(matches!(after_first.status, PaymentStatus::Partial));

        let after_second =
            record_payment(&conn, id, &month, calc.calculated_amount_minor - half, "2026-01-28").unwrap();
        assert_eq!(after_second.paid_amount_minor, calc.calculated_amount_minor);
        assert!(matches!(after_second.status, PaymentStatus::Paid));
        assert_eq!(after_second.paid_date.as_deref(), Some("2026-01-28"));
    }

    #[test]
    fn record_payment_rejects_bad_input() {
        let conn = test_conn();
        let id = employee_id(&conn, "Sana Khalid");
        let month = this_month();
        assert!(matches!(record_payment(&conn, id, &month, 0, "2026-01-10"), Err(SalaryError::InvalidAmount)));
        assert!(matches!(
            record_payment(&conn, id, &month, 100, "not-a-date"),
            Err(SalaryError::InvalidDate(_))
        ));
        assert!(matches!(
            record_payment(&conn, 999_999, &month, 100, "2026-01-10"),
            Err(SalaryError::EmployeeNotFound)
        ));
    }

    #[test]
    fn get_payment_history_includes_last_months_seeded_payment() {
        let conn = test_conn();
        let id = employee_id(&conn, "Bilal Ahmed");
        let history = get_payment_history(&conn, id).unwrap();
        // Seed data pays last month's salary in full.
        assert!(!history.is_empty());
        assert!(history.iter().any(|h| matches!(h.status, PaymentStatus::Paid)));
    }

    #[test]
    fn full_loop_calculate_then_pay_then_see_it_in_history() {
        let conn = test_conn();
        let id = employee_id(&conn, "Ahmed Raza");
        let month = this_month();

        let calc = calculate_salary(&conn, id, &month).unwrap();
        record_payment(&conn, id, &month, calc.calculated_amount_minor, "2026-01-15").unwrap();

        let history = get_payment_history(&conn, id).unwrap();
        let this_month_row = history.iter().find(|h| h.month == month).unwrap();
        assert!(matches!(this_month_row.status, PaymentStatus::Paid));
        assert_eq!(this_month_row.paid_amount_minor, calc.calculated_amount_minor);
    }
}
