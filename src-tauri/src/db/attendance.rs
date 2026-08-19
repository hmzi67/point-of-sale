//! Employee attendance: check-in/check-out logging and the monthly summary
//! that Phase 9 (Salary) will consume.
//!
//! One row per employee per shift (`attendance`, keyed loosely by
//! `(employee_id, work_date)` — there is no UNIQUE constraint in the schema,
//! but every write in this module goes through `find_open_or_todays_row`
//! first, so a day never grows a second row on its own). A shift with a
//! `check_in` and no `check_out` is "incomplete" — most often just today's
//! shift still in progress, but it can also be a employee who genuinely
//! forgot to clock out. Either way it is never dropped silently: it counts
//! toward `days_present`, is excluded from `total_hours` (there is no end
//! time to measure), and is called out via `incomplete_days` so the UI and,
//! later, payroll can flag it instead of silently under- or over-counting.

use chrono::{Datelike, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

#[derive(Debug)]
pub enum AttendanceError {
    EmployeeNotFound,
    NotCheckedIn,
    InvalidMonth(String),
    InvalidDateRange(String),
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for AttendanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttendanceError::EmployeeNotFound => write!(f, "Employee not found"),
            AttendanceError::NotCheckedIn => {
                write!(f, "This employee has not checked in today")
            }
            AttendanceError::InvalidMonth(msg) => write!(f, "{}", msg),
            AttendanceError::InvalidDateRange(msg) => write!(f, "{}", msg),
            AttendanceError::Sqlite(err) => write!(f, "database error: {}", err),
        }
    }
}

impl From<rusqlite::Error> for AttendanceError {
    fn from(err: rusqlite::Error) -> Self {
        AttendanceError::Sqlite(err)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Employee {
    pub id: i64,
    pub name: String,
    pub role: String,
    pub contact: Option<String>,
    pub base_salary_minor: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttendanceRecord {
    pub id: i64,
    pub employee_id: i64,
    pub employee_name: String,
    /// `YYYY-MM-DD`.
    pub work_date: String,
    pub check_in: Option<String>,
    pub check_out: Option<String>,
    /// `None` whenever `check_out` is `None` — there is nothing to measure yet.
    pub hours_worked: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthlySummary {
    pub employee_id: i64,
    pub employee_name: String,
    pub days_present: i64,
    pub days_absent: i64,
    /// Shifts with a `check_in` but no `check_out` — already counted in
    /// `days_present`, called out separately so payroll doesn't quietly treat
    /// a forgotten clock-out as a zero-hour day.
    pub incomplete_days: i64,
    pub total_hours: f64,
}

fn hours_between(check_in: &str, check_out: &str) -> Option<f64> {
    const FMT: &str = "%Y-%m-%d %H:%M:%S";
    let start = chrono::NaiveDateTime::parse_from_str(check_in, FMT).ok()?;
    let end = chrono::NaiveDateTime::parse_from_str(check_out, FMT).ok()?;
    let minutes = (end - start).num_minutes();
    if minutes < 0 {
        return None;
    }
    Some((minutes as f64 / 60.0 * 100.0).round() / 100.0)
}

fn row_to_record(row: &rusqlite::Row) -> rusqlite::Result<AttendanceRecord> {
    let check_in: Option<String> = row.get(3)?;
    let check_out: Option<String> = row.get(4)?;
    let hours_worked = match (&check_in, &check_out) {
        (Some(ci), Some(co)) => hours_between(ci, co),
        _ => None,
    };
    Ok(AttendanceRecord {
        id: row.get(0)?,
        employee_id: row.get(1)?,
        employee_name: row.get(2)?,
        work_date: row.get(5)?,
        check_in,
        check_out,
        hours_worked,
    })
}

const RECORD_COLUMNS: &str =
    "a.id, a.employee_id, e.name, a.check_in, a.check_out, a.work_date";

fn employee_exists(conn: &Connection, employee_id: i64) -> Result<bool, rusqlite::Error> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM employees WHERE id = ?1 AND is_active = 1)",
        params![employee_id],
        |row| row.get(0),
    )
}

/// Active employees, for the check-in/out screen and the monthly summary.
pub fn list_employees(conn: &Connection) -> Result<Vec<Employee>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, name, role, contact, base_salary_minor
           FROM employees
          WHERE is_active = 1
          ORDER BY name",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Employee {
            id: row.get(0)?,
            name: row.get(1)?,
            role: row.get(2)?,
            contact: row.get(3)?,
            base_salary_minor: row.get(4)?,
        })
    })?;
    rows.collect()
}

/// Checks an employee in for today. Idempotent: if today's row already
/// exists (a double-tap, or a re-check-in after an earlier check-out), its
/// `check_in` is updated in place rather than inserting a second row for the
/// same day.
pub fn check_in(conn: &Connection, employee_id: i64) -> Result<AttendanceRecord, AttendanceError> {
    if !employee_exists(conn, employee_id)? {
        return Err(AttendanceError::EmployeeNotFound);
    }

    let existing_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM attendance
              WHERE employee_id = ?1 AND work_date = date('now', 'localtime')",
            params![employee_id],
            |row| row.get(0),
        )
        .optional()?;

    let row_id = match existing_id {
        Some(id) => {
            conn.execute(
                "UPDATE attendance SET check_in = datetime('now', 'localtime') WHERE id = ?1",
                params![id],
            )?;
            id
        }
        None => {
            conn.execute(
                "INSERT INTO attendance (employee_id, work_date, check_in)
                 VALUES (?1, date('now', 'localtime'), datetime('now', 'localtime'))",
                params![employee_id],
            )?;
            conn.last_insert_rowid()
        }
    };

    load_record(conn, row_id)
}

/// Checks an employee out for today. Idempotent the same way `check_in` is —
/// repeated taps update the same row's `check_out`. Fails clearly rather than
/// fabricating a row if the employee never checked in today.
pub fn check_out(conn: &Connection, employee_id: i64) -> Result<AttendanceRecord, AttendanceError> {
    if !employee_exists(conn, employee_id)? {
        return Err(AttendanceError::EmployeeNotFound);
    }

    let existing_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM attendance
              WHERE employee_id = ?1 AND work_date = date('now', 'localtime')
                AND check_in IS NOT NULL",
            params![employee_id],
            |row| row.get(0),
        )
        .optional()?;

    let Some(row_id) = existing_id else {
        return Err(AttendanceError::NotCheckedIn);
    };

    conn.execute(
        "UPDATE attendance SET check_out = datetime('now', 'localtime') WHERE id = ?1",
        params![row_id],
    )?;

    load_record(conn, row_id)
}

fn load_record(conn: &Connection, id: i64) -> Result<AttendanceRecord, AttendanceError> {
    conn.query_row(
        &format!(
            "SELECT {} FROM attendance a JOIN employees e ON e.id = a.employee_id WHERE a.id = ?1",
            RECORD_COLUMNS
        ),
        params![id],
        row_to_record,
    )
    .map_err(AttendanceError::from)
}

fn validate_date_range(start_date: &str, end_date: &str) -> Result<(), AttendanceError> {
    let start = NaiveDate::parse_from_str(start_date, "%Y-%m-%d")
        .map_err(|_| AttendanceError::InvalidDateRange(format!("Invalid start date: {}", start_date)))?;
    let end = NaiveDate::parse_from_str(end_date, "%Y-%m-%d")
        .map_err(|_| AttendanceError::InvalidDateRange(format!("Invalid end date: {}", end_date)))?;
    if start > end {
        return Err(AttendanceError::InvalidDateRange(
            "Start date must not be after the end date".into(),
        ));
    }
    Ok(())
}

/// The attendance log for a date range, optionally scoped to one employee —
/// `employee_id = None` returns every employee's shifts in the range.
pub fn get_attendance(
    conn: &Connection,
    employee_id: Option<i64>,
    start_date: &str,
    end_date: &str,
) -> Result<Vec<AttendanceRecord>, AttendanceError> {
    validate_date_range(start_date, end_date)?;

    let sql = format!(
        "SELECT {} FROM attendance a JOIN employees e ON e.id = a.employee_id
          WHERE a.work_date BETWEEN ?1 AND ?2
            AND (?3 IS NULL OR a.employee_id = ?3)
          ORDER BY a.work_date DESC, e.name",
        RECORD_COLUMNS
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![start_date, end_date, employee_id], row_to_record)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// Days present/absent and total hours per employee for `month` (`YYYY-MM`).
///
/// "Present" is any calendar day with a `check_in` recorded, whether or not
/// the shift has a `check_out` yet — an incomplete shift still means the
/// employee showed up. "Absent" is every other day from the 1st of the month
/// up to today (or the month's last day, for a month that has already
/// finished) — there is no shift-schedule table to say which days an
/// employee was actually expected in, so every day the shop has been open
/// this month counts.
pub fn get_monthly_summary(conn: &Connection, month: &str) -> Result<Vec<MonthlySummary>, AttendanceError> {
    let month_start = NaiveDate::parse_from_str(&format!("{}-01", month), "%Y-%m-%d")
        .map_err(|_| AttendanceError::InvalidMonth(format!("Invalid month: {} (expected YYYY-MM)", month)))?;

    let today = chrono::Local::now().date_naive();
    let month_end = last_day_of_month(month_start);
    // Never count days that haven't happened yet.
    let range_end = month_end.min(today);
    let days_in_range = if range_end < month_start {
        0
    } else {
        (range_end - month_start).num_days() + 1
    };

    let mut stmt = conn.prepare(
        "SELECT
             e.id,
             e.name,
             COUNT(DISTINCT a.work_date)                                            AS days_present,
             COUNT(DISTINCT CASE WHEN a.check_in IS NOT NULL AND a.check_out IS NULL
                                  THEN a.work_date END)                             AS incomplete_days,
             a.check_in,
             a.check_out
           FROM employees e
           LEFT JOIN attendance a
                  ON a.employee_id = e.id
                 AND substr(a.work_date, 1, 7) = ?1
          WHERE e.is_active = 1
          GROUP BY e.id",
    )?;

    // Hours can't be summed in the same GROUP BY as the CASE-based
    // incomplete count without double-joining, so they're computed with a
    // second, simpler pass keyed by employee.
    let mut hours_stmt = conn.prepare(
        "SELECT a.check_in, a.check_out
           FROM attendance a
          WHERE a.employee_id = ?1
            AND substr(a.work_date, 1, 7) = ?2
            AND a.check_in IS NOT NULL
            AND a.check_out IS NOT NULL",
    )?;

    let employees: Vec<(i64, String, i64, i64)> = stmt
        .query_map(params![month], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut summaries = Vec::with_capacity(employees.len());
    for (employee_id, employee_name, days_present, incomplete_days) in employees {
        let total_hours: f64 = hours_stmt
            .query_map(params![employee_id, month], |row| {
                let check_in: String = row.get(0)?;
                let check_out: String = row.get(1)?;
                Ok(hours_between(&check_in, &check_out).unwrap_or(0.0))
            })?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .sum();

        summaries.push(MonthlySummary {
            employee_id,
            employee_name,
            days_present,
            days_absent: (days_in_range - days_present).max(0),
            incomplete_days,
            total_hours: (total_hours * 100.0).round() / 100.0,
        });
    }

    Ok(summaries)
}

fn last_day_of_month(first_of_month: NaiveDate) -> NaiveDate {
    let next_month = if first_of_month.month() == 12 {
        NaiveDate::from_ymd_opt(first_of_month.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(first_of_month.year(), first_of_month.month() + 1, 1)
    };
    // `from_ymd_opt` only returns `None` outside chrono's representable year
    // range (tens of thousands of years out) — never for a real business
    // month, and `month` is always a valid 1..=12 constructed just above.
    next_month.unwrap() - chrono::Duration::days(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::test_conn;

    fn employee_id(conn: &Connection, name: &str) -> i64 {
        conn.query_row("SELECT id FROM employees WHERE name = ?1", params![name], |row| row.get(0))
            .unwrap()
    }

    /// The demo seed gives every employee a shift for today (see
    /// `db/seed.rs`), so a test that wants to start from "hasn't clocked in
    /// yet" has to clear that row first.
    fn clear_todays_seeded_shift(conn: &Connection, employee_id: i64) {
        conn.execute(
            "DELETE FROM attendance WHERE employee_id = ?1 AND work_date = date('now', 'localtime')",
            params![employee_id],
        )
        .unwrap();
    }

    #[test]
    fn list_employees_returns_the_seeded_staff() {
        let conn = test_conn();
        let employees = list_employees(&conn).unwrap();
        assert_eq!(employees.len(), 3);
        assert!(employees.iter().any(|e| e.name == "Ahmed Raza"));
    }

    #[test]
    fn check_in_creates_a_row_for_today() {
        let conn = test_conn();
        let id = employee_id(&conn, "Sana Khalid");
        clear_todays_seeded_shift(&conn, id);
        let record = check_in(&conn, id).unwrap();
        assert_eq!(record.employee_id, id);
        assert!(record.check_in.is_some());
        assert!(record.check_out.is_none());

        let today: String = conn.query_row("SELECT date('now', 'localtime')", [], |row| row.get(0)).unwrap();
        assert_eq!(record.work_date, today);
    }

    #[test]
    fn checking_in_twice_the_same_day_updates_the_same_row_not_a_second_one() {
        let conn = test_conn();
        let id = employee_id(&conn, "Sana Khalid");
        clear_todays_seeded_shift(&conn, id);
        check_in(&conn, id).unwrap();
        check_in(&conn, id).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM attendance WHERE employee_id = ?1 AND work_date = date('now', 'localtime')",
                params![id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "a duplicate check-in must update, not insert a second row");
    }

    #[test]
    fn check_out_completes_todays_shift() {
        let conn = test_conn();
        let id = employee_id(&conn, "Sana Khalid");
        clear_todays_seeded_shift(&conn, id);
        check_in(&conn, id).unwrap();
        let record = check_out(&conn, id).unwrap();
        assert!(record.check_out.is_some());
        assert!(record.hours_worked.is_some());
    }

    #[test]
    fn checking_out_without_checking_in_first_is_a_clear_error_not_a_crash() {
        let conn = test_conn();
        let id = employee_id(&conn, "Sana Khalid");
        clear_todays_seeded_shift(&conn, id);
        assert!(matches!(check_out(&conn, id), Err(AttendanceError::NotCheckedIn)));
    }

    #[test]
    fn check_in_and_out_reject_an_unknown_employee() {
        let conn = test_conn();
        assert!(matches!(check_in(&conn, 999_999), Err(AttendanceError::EmployeeNotFound)));
        assert!(matches!(check_out(&conn, 999_999), Err(AttendanceError::EmployeeNotFound)));
    }

    #[test]
    fn get_attendance_filters_by_employee_and_date_range() {
        let conn = test_conn();
        let ahmed = employee_id(&conn, "Ahmed Raza");

        let all = get_attendance(&conn, None, "2000-01-01", "2100-01-01").unwrap();
        assert!(!all.is_empty(), "seed data should have populated attendance");

        let just_ahmed = get_attendance(&conn, Some(ahmed), "2000-01-01", "2100-01-01").unwrap();
        assert!(just_ahmed.iter().all(|r| r.employee_id == ahmed));
        assert!(just_ahmed.len() < all.len());
    }

    #[test]
    fn get_attendance_rejects_a_backwards_range() {
        let conn = test_conn();
        assert!(matches!(
            get_attendance(&conn, None, "2026-02-01", "2026-01-01"),
            Err(AttendanceError::InvalidDateRange(_))
        ));
    }

    /// The seed data leaves the first employee's shift open today (no
    /// check-out) specifically to exercise this: the monthly summary must not
    /// crash or silently drop the day — it should still count as present and
    /// show up flagged as incomplete, with hours excluded.
    #[test]
    fn monthly_summary_flags_an_incomplete_shift_instead_of_crashing() {
        let conn = test_conn();
        let month = chrono::Local::now().format("%Y-%m").to_string();
        let summaries = get_monthly_summary(&conn, &month).unwrap();

        let ahmed_summary = summaries
            .iter()
            .find(|s| s.employee_name == "Ahmed Raza")
            .expect("Ahmed Raza should be in this month's summary");
        assert!(ahmed_summary.days_present >= 1);
        assert!(ahmed_summary.incomplete_days >= 1, "today's open shift should be flagged incomplete");
    }

    #[test]
    fn monthly_summary_accounts_for_present_and_absent_days() {
        let conn = test_conn();
        let month = chrono::Local::now().format("%Y-%m").to_string();
        let summaries = get_monthly_summary(&conn, &month).unwrap();
        assert_eq!(summaries.len(), 3);

        for s in &summaries {
            assert!(s.days_present >= 0);
            assert!(s.days_absent >= 0);
            assert!(s.total_hours >= 0.0);
        }
    }

    #[test]
    fn monthly_summary_rejects_a_malformed_month() {
        let conn = test_conn();
        assert!(matches!(get_monthly_summary(&conn, "not-a-month"), Err(AttendanceError::InvalidMonth(_))));
    }

    #[test]
    fn full_loop_check_in_then_out_shows_up_in_the_log_and_the_monthly_summary() {
        let conn = test_conn();
        let id = employee_id(&conn, "Bilal Ahmed");
        clear_todays_seeded_shift(&conn, id);
        let today: String = conn.query_row("SELECT date('now', 'localtime')", [], |row| row.get(0)).unwrap();

        check_in(&conn, id).unwrap();
        check_out(&conn, id).unwrap();

        let log = get_attendance(&conn, Some(id), &today, &today).unwrap();
        assert_eq!(log.len(), 1);
        assert!(log[0].hours_worked.is_some());

        let month = chrono::Local::now().format("%Y-%m").to_string();
        let summary = get_monthly_summary(&conn, &month)
            .unwrap()
            .into_iter()
            .find(|s| s.employee_id == id)
            .unwrap();
        assert!(summary.days_present >= 1);
    }
}
