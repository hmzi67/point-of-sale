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

use rusqlite::{params, Connection};
use serde::Serialize;
use crate::db::dates;

#[derive(Debug, thiserror::Error)]
pub enum ExpenseError {
    #[error("{0}")]
    InvalidDate(String),
    #[error("{0}")]
    InvalidDateRange(String),
    #[error("Category cannot be empty")]
    EmptyCategory,
    #[error("Amount must be greater than zero")]
    InvalidAmount,
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
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
    dates::parse_date(date).map(|_| ()).map_err(ExpenseError::InvalidDate)
}

fn validate_date_range(start_date: &str, end_date: &str) -> Result<(), ExpenseError> {
    dates::parse_date_range(start_date, end_date).map(|_| ()).map_err(ExpenseError::InvalidDateRange)
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
        // Counted from the table rather than hard-coded, so a change to the
        // seed data doesn't break this test.
        add_expense(&conn, "2026-01-05", "Supplies", 1000, None).unwrap();
        let count = |sql: &str| -> usize { conn.query_row(sql, [], |row| row.get::<_, i64>(0)).unwrap() as usize };

        let all = get_expenses(&conn, "2000-01-01", "2100-01-01", None).unwrap();
        assert_eq!(all.len(), count("SELECT COUNT(*) FROM expenses"));

        let supplies_only = get_expenses(&conn, "2000-01-01", "2100-01-01", Some("Supplies")).unwrap();
        assert!(supplies_only.iter().all(|e| e.category == "Supplies"));
        assert_eq!(supplies_only.len(), count("SELECT COUNT(*) FROM expenses WHERE category = 'Supplies'"));
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

        // Each category's count/sum matches the raw rows, whatever the seed.
        for total in &totals {
            let (count, sum): (i64, i64) = conn
                .query_row(
                    "SELECT COUNT(*), SUM(amount_minor) FROM expenses WHERE category = ?1",
                    params![total.category],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(total.count, count, "{}", total.category);
            assert_eq!(total.total_minor, sum, "{}", total.category);
        }

        // Highest total first.
        assert!(totals.windows(2).all(|pair| pair[0].total_minor >= pair[1].total_minor));
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
