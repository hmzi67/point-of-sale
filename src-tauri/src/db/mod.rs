//! SQLite access layer. Every query in the app lives under this module —
//! Tauri commands in `commands.rs` call into here, and nothing outside
//! `src-tauri` ever touches the database.

pub mod attendance;
pub mod config;
pub mod connection;
pub mod csv_import;
pub mod dashboard;
pub mod expenses;
pub mod items;
pub mod modules;
pub mod refunds;
pub mod reports;
pub mod sales;
pub mod salary;
pub mod schema;
pub mod seed;
pub mod shifts;
pub mod tables;
pub mod users;

#[allow(unused_imports)]
pub use connection::{Db, DbError};
