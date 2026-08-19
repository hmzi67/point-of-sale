//! Expense tracking: quick-add entries and the category breakdown that,
//! together with `sales` and `salary_payments`, feeds the dashboard's profit
//! figure (Phase 10).
//!
//! `expenses.category` is a plain text column, not a foreign key into a
//! categories table (unlike `items.category_id`) — a shop's expense
//! categories are a handful of free-text labels ("Rent", "Utilities", …)
//! that don't need their own CRUD screen. "Add a new category" from the
//! frontend is therefore just typing a new label into the form; there is
//! nothing to insert ahead of time. `get_categories` lists whatever labels
//! are already in use so the dropdown has something to offer.

use chrono::NaiveDate;
use rusqlite::{params, Connection};
use serde::Serialize;

#[derive(Debug)]
pub enum ExpenseError {
    InvalidDate(String),
    InvalidDateRange(String),
    EmptyCategory,
    InvalidAmount,
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for ExpenseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExpenseError::InvalidDate(msg) => write!(f, "{}", msg),
            ExpenseError::InvalidDateRange(msg) => write!(f, "{}", msg),
            ExpenseError::EmptyCategory => write!(f, "Category cannot be empty"),
            ExpenseError::InvalidAmount => write!(f, "Amount must be greater than zero"),
            ExpenseError::Sqlite(err) => write!(f, "database error: {}", err),
        }
    }
}

impl From<rusqlite::Error> for ExpenseError {
    fn from(err: rusqlite::Error) -> Self {
        ExpenseError::Sqlite(err)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Expense {
    pub id: i64,
    /// `YYYY-MM-DD`.
    pub expense_date: String,
    pub category: String,
    pub amount_minor: i64,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryTotal {
    pub category: String,
    pub total_minor: i64,
    pub count: i64,
}

fn validate_date(date: &str) -> Result<(), ExpenseError> {
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map(|_| ())
        .map_err(|_| ExpenseError::InvalidDate(format!("Invalid date: {}", date)))
}

fn validate_date_range(start_date: &str, end_date: &str) -> Result<(), ExpenseError> {
    let start = NaiveDate::parse_from_str(start_date, "%Y-%m-%d")
        .map_err(|_| ExpenseError::InvalidDateRange(format!("Invalid start date: {}", start_date)))?;
    let end = NaiveDate::parse_from_str(end_date, "%Y-%m-%d")
        .map_err(|_| ExpenseError::InvalidDateRange(format!("Invalid end date: {}", end_date)))?;
    if start > end {
        return Err(ExpenseError::InvalidDateRange(
            "Start date must not be after the end date".into(),
        ));
    }
    Ok(())
}

fn row_to_expense(row: &rusqlite::Row) -> rusqlite::Result<Expense> {
    Ok(Expense {
        id: row.get(0)?,
        expense_date: row.get(1)?,
        category: row.get(2)?,
        amount_minor: row.get(3)?,
        note: row.get(4)?,
    })
}

/// Logs one expense. `note` is trimmed to `None` if blank, so an empty note
/// field never round-trips as an empty string.
pub fn add_expense(
    conn: &Connection,
    date: &str,
    category: &str,
    amount_minor: i64,
    note: Option<&str>,
) -> Result<Expense, ExpenseError> {
    validate_date(date)?;
    let category = category.trim();
    if category.is_empty() {
        return Err(ExpenseError::EmptyCategory);
    }
    if amount_minor <= 0 {
        return Err(ExpenseError::InvalidAmount);
    }
    let note = note.map(str::trim).filter(|s| !s.is_empty());

    conn.execute(
        "INSERT INTO expenses (expense_date, category, amount_minor, note) VALUES (?1, ?2, ?3, ?4)",
        params![date, category, amount_minor, note],
    )?;
    let id = conn.last_insert_rowid();

    conn.query_row(
        "SELECT id, expense_date, category, amount_minor, note FROM expenses WHERE id = ?1",
        params![id],
        row_to_expense,
    )
    .map_err(ExpenseError::from)
}

/// Expenses in a date range (inclusive), optionally scoped to one category —
/// `category = None` returns every category.
pub fn get_expenses(
    conn: &Connection,
    start_date: &str,
    end_date: &str,
    category: Option<&str>,
) -> Result<Vec<Expense>, ExpenseError> {
    validate_date_range(start_date, end_date)?;

    let mut stmt = conn.prepare(
        "SELECT id, expense_date, category, amount_minor, note
           FROM expenses
          WHERE expense_date BETWEEN ?1 AND ?2
            AND (?3 IS NULL OR category = ?3)
          ORDER BY expense_date DESC, id DESC",
    )?;
    let rows = stmt.query_map(params![start_date, end_date, category], row_to_expense)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// Every distinct category already in use, for the quick-add form's dropdown.
pub fn get_categories(conn: &Connection) -> Result<Vec<String>, rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT DISTINCT category FROM expenses ORDER BY category")?;
    let rows = stmt.query_map([], |row| row.get(0))?;
    rows.collect()
}

/// Category-wise totals for a date range, highest spend first — the
/// breakdown view, and the figure Phase 10's profit calc sums over.
pub fn get_totals_by_category(
    conn: &Connection,
    start_date: &str,
    end_date: &str,
) -> Result<Vec<CategoryTotal>, ExpenseError> {
    validate_date_range(start_date, end_date)?;

    let mut stmt = conn.prepare(
        "SELECT category, SUM(amount_minor), COUNT(*)
           FROM expenses
          WHERE expense_date BETWEEN ?1 AND ?2
          GROUP BY category
          ORDER BY SUM(amount_minor) DESC",
    )?;
    let rows = stmt.query_map(params![start_date, end_date], |row| {
        Ok(CategoryTotal { category: row.get(0)?, total_minor: row.get(1)?, count: row.get(2)? })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::test_conn;

    #[test]
    fn add_expense_inserts_and_returns_the_row() {
        let conn = test_conn();
        let expense = add_expense(&conn, "2026-01-05", "Repairs", 1500, Some("  Door lock  ")).unwrap();
        assert_eq!(expense.category, "Repairs");
        assert_eq!(expense.amount_minor, 1500);
        assert_eq!(expense.note.as_deref(), Some("Door lock"), "note should be trimmed");
    }

    #[test]
    fn add_expense_treats_a_blank_note_as_none() {
        let conn = test_conn();
        let expense = add_expense(&conn, "2026-01-05", "Repairs", 500, Some("   ")).unwrap();
        assert!(expense.note.is_none());
    }

    #[test]
    fn add_expense_rejects_bad_input() {
        let conn = test_conn();
        assert!(matches!(add_expense(&conn, "not-a-date", "Rent", 100, None), Err(ExpenseError::InvalidDate(_))));
        assert!(matches!(add_expense(&conn, "2026-01-05", "  ", 100, None), Err(ExpenseError::EmptyCategory)));
        assert!(matches!(add_expense(&conn, "2026-01-05", "Rent", 0, None), Err(ExpenseError::InvalidAmount)));
        assert!(matches!(add_expense(&conn, "2026-01-05", "Rent", -5, None), Err(ExpenseError::InvalidAmount)));
    }

    #[test]
    fn get_expenses_filters_by_date_range_and_category() {
        let conn = test_conn();
        // Seed data already has 6 expenses spread over the last 12 days.
        let all = get_expenses(&conn, "2000-01-01", "2100-01-01", None).unwrap();
        assert_eq!(all.len(), 6);

        let supplies_only = get_expenses(&conn, "2000-01-01", "2100-01-01", Some("Supplies")).unwrap();
        assert!(supplies_only.iter().all(|e| e.category == "Supplies"));
        assert_eq!(supplies_only.len(), 2);
    }

    #[test]
    fn get_expenses_rejects_a_backwards_range() {
        let conn = test_conn();
        assert!(matches!(
            get_expenses(&conn, "2026-02-01", "2026-01-01", None),
            Err(ExpenseError::InvalidDateRange(_))
        ));
    }

    #[test]
    fn get_categories_returns_distinct_labels_in_use() {
        let conn = test_conn();
        let categories = get_categories(&conn).unwrap();
        assert!(categories.contains(&"Rent".to_string()));
        assert!(categories.contains(&"Supplies".to_string()));
        // Supplies appears twice in the seed data but only once here.
        assert_eq!(categories.iter().filter(|c| *c == "Supplies").count(), 1);
    }

    #[test]
    fn get_totals_by_category_sums_and_counts_per_category_highest_first() {
        let conn = test_conn();
        let totals = get_totals_by_category(&conn, "2000-01-01", "2100-01-01").unwrap();

        // Seed data: Supplies = 2150 + 950 rupees, in minor units.
        let supplies = totals.iter().find(|t| t.category == "Supplies").unwrap();
        assert_eq!(supplies.count, 2);
        assert_eq!(supplies.total_minor, (2150 + 950) * 100);

        // Highest total first: Rent (35,000) leads the seeded categories.
        assert_eq!(totals[0].category, "Rent");
    }

    #[test]
    fn full_loop_add_several_expenses_and_read_them_back() {
        let conn = test_conn();
        add_expense(&conn, "2026-01-01", "Marketing", 5000, None).unwrap();
        add_expense(&conn, "2026-01-02", "Marketing", 3000, Some("Flyers")).unwrap();
        add_expense(&conn, "2026-01-03", "Repairs", 12000, None).unwrap();

        let jan = get_expenses(&conn, "2026-01-01", "2026-01-03", None).unwrap();
        assert_eq!(jan.len(), 3);

        let totals = get_totals_by_category(&conn, "2026-01-01", "2026-01-03").unwrap();
        let marketing = totals.iter().find(|t| t.category == "Marketing").unwrap();
        assert_eq!(marketing.count, 2);
        assert_eq!(marketing.total_minor, 8000);

        let categories = get_categories(&conn).unwrap();
        assert!(categories.contains(&"Marketing".to_string()));
        assert!(categories.contains(&"Repairs".to_string()));
    }
}
