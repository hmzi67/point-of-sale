//! SQLite access layer. Every query in the app lives under this module —
//! Tauri commands in `commands.rs` call into here, and nothing outside
//! `src-tauri` ever touches the database.

pub mod config;
pub mod connection;
pub mod items;
pub mod modules;
pub mod reports;
pub mod sales;
pub mod schema;
pub mod seed;
pub mod users;

#[allow(unused_imports)]
pub use connection::{Db, DbError};
