//! SQLite access layer. Every query in the app lives under this module —
//! Tauri commands in `commands.rs` call into here, and nothing outside
//! `src-tauri` ever touches the database.

pub mod attendance;
pub mod config;
pub mod connection;
pub mod counters;
pub mod csv_import;
pub mod customers;
pub mod dashboard;
pub mod dates;
pub mod delivery_platforms;
pub mod expenses;
pub mod full_report;
pub mod insights;
pub mod items;
pub mod license;
pub mod modules;
pub mod orders;
pub mod product_owner;
pub mod purchasing;
pub mod refunds;
pub mod reports;
pub mod sales;
pub mod salary;
pub mod schema;
pub mod security_pin;
pub mod seed;
pub mod shifts;
pub mod stock;
pub mod sync;
pub mod tables;
#[cfg(test)]
pub mod test_support;
pub mod tokens;
pub mod users;

#[allow(unused_imports)]
pub use connection::{Db, DbError};

/// `12345` -> `"123.45"`, for amounts quoted inside an error message.
pub fn format_minor_plain(minor: i64) -> String {
    let sign = if minor < 0 { "-" } else { "" };
    let abs = minor.unsigned_abs();
    format!("{sign}{}.{:02}", abs / 100, abs % 100)
}

/// Logs a PIN/password hashing failure's detail and returns it, so the
/// error shown to the user can stay generic (internal parser text means
/// nothing to a cashier and says something about the stored hash).
pub(crate) fn hash_failure(context: &str, error: impl std::fmt::Display) -> String {
    let detail = crate::startup_log::clip(&error.to_string());
    crate::startup_log::warn(&format!("{context}: hash processing failed: {detail}"));
    detail
}

/// An optional text field trimmed, with blank stored as NULL — the
/// normalization every optional text column uses.
pub(crate) fn clean_text(value: &Option<String>) -> Option<String> {
    value.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

/// Every payment-method list, in one place — each mirrors a column's CHECK
/// constraint in `schema.sql`.
pub mod payment_methods {
    /// A sale's single `payment_method` as a caller may name it
    /// (`split`/`credit` are derived, never sent).
    pub const SALE: &[&str] = &["cash", "card", "other", "platform"];
    /// One `sale_payments` tender; `credit` is the pay-later part.
    pub const TENDER: &[&str] = &["cash", "card", "other", "platform", "credit"];
    /// A pay-later installment (`customer_payments.method`).
    pub const COLLECTION: &[&str] = &["cash", "card", "other"];
    /// A payment to a supplier (`supplier_payments.method`).
    pub const SUPPLIER: &[&str] = &["cash", "card", "bank", "other"];
}
