//! Date parsing shared by every per-area query module that takes a date
//! range or a month from the frontend (attendance, expenses, reports,
//! salary). Each returns a plain message; callers wrap it in their own
//! error variant so the user-facing text stays identical everywhere.

use chrono::{Datelike, NaiveDate};

/// The `YYYY-MM-DD` format every date argument and `*_date` column uses.
pub const DATE_FORMAT: &str = "%Y-%m-%d";

/// The one timestamp format stored in every `*_at` TEXT column.
pub const TIMESTAMP_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

/// Parses a single `YYYY-MM-DD` date.
pub fn parse_date(date: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(date, DATE_FORMAT).map_err(|_| format!("Invalid date: {date}"))
}

/// Parses a `YYYY-MM-DD` start/end pair and checks start <= end.
pub fn parse_date_range(start_date: &str, end_date: &str) -> Result<(NaiveDate, NaiveDate), String> {
    let start = NaiveDate::parse_from_str(start_date, DATE_FORMAT)
        .map_err(|_| format!("Invalid start date: {start_date}"))?;
    let end =
        NaiveDate::parse_from_str(end_date, DATE_FORMAT).map_err(|_| format!("Invalid end date: {end_date}"))?;
    if start > end {
        return Err("Start date must not be after the end date".into());
    }
    Ok((start, end))
}

/// Parses a `YYYY-MM` month into its first day.
pub fn parse_month(month: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(&format!("{month}-01"), DATE_FORMAT)
        .map_err(|_| format!("Invalid month: {month} (expected YYYY-MM)"))
}

/// The last day of the month `first_of_month` falls in.
pub fn last_day_of_month(first_of_month: NaiveDate) -> NaiveDate {
    let (year, month) = if first_of_month.month() == 12 {
        (first_of_month.year() + 1, 1)
    } else {
        (first_of_month.year(), first_of_month.month() + 1)
    };
    // `from_ymd_opt` only returns `None` outside chrono's representable year
    // range (tens of thousands of years out) — never for a real business month.
    NaiveDate::from_ymd_opt(year, month, 1).expect("first-of-month always constructs") - chrono::Duration::days(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_range_accepts_a_same_day_range_and_rejects_reversed_or_malformed() {
        assert!(parse_date_range("2026-03-05", "2026-03-05").is_ok());
        assert_eq!(parse_date_range("2026-03-06", "2026-03-05").unwrap_err(), "Start date must not be after the end date");
        assert_eq!(parse_date_range("2026-3-x", "2026-03-05").unwrap_err(), "Invalid start date: 2026-3-x");
        assert_eq!(parse_date_range("2026-03-05", "nope").unwrap_err(), "Invalid end date: nope");
    }

    #[test]
    fn month_parsing_and_month_length_handle_leap_years_and_december() {
        assert_eq!(parse_month("2026-02").unwrap(), NaiveDate::from_ymd_opt(2026, 2, 1).unwrap());
        assert!(parse_month("2026-13").is_err());
        assert_eq!(last_day_of_month(parse_month("2024-02").unwrap()).day(), 29);
        assert_eq!(last_day_of_month(parse_month("2026-02").unwrap()).day(), 28);
        assert_eq!(last_day_of_month(parse_month("2026-12").unwrap()), NaiveDate::from_ymd_opt(2026, 12, 31).unwrap());
    }
}
