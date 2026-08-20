//! The vendor/developer account — deliberately not a `users` row.
//!
//! `product_owner_account` is a single-row table (`id = 1`, same pattern as
//! `app_config`) that does not exist until the vendor sets a credential on
//! this specific install. Nothing here is joined against, exposed by, or
//! reachable from any client-facing query (Manage Users, the login
//! screen's account list, `db::users` in general) — this module is the
//! entire surface for this account, and `commands.rs` is careful to gate
//! every command backed by it with a session distinct from the client
//! staff session (`product_owner_session.rs`), never `session::Session`.
//!
//! There is deliberately no password-reset path here reachable by any
//! client role — see SUPPORT.md for what recovering a forgotten credential
//! on a given install actually looks like (manual, vendor-side, requires
//! filesystem access to that install).
//!
//! Hashing reuses the exact same `argon2` setup `db::users::hash_pin` uses
//! (same crate, same default parameters) — kept as a small local function
//! rather than calling into `db::users` directly, so this module has zero
//! dependency on the `users` table or its queries, matching "separate
//! storage" in spirit as well as in the schema.

use argon2::password_hash::{PasswordHash, PasswordVerifier};
use argon2::Argon2;
use rusqlite::{params, Connection, OptionalExtension};

use crate::db::users::hash_pin;

/// Deliberately not digit-only / not capped at 6 like a client PIN — this
/// credential is typed by the vendor, not a cashier under time pressure, so
/// it can and should be a real password. A floor, not a ceiling: argon2
/// handles arbitrarily long input fine, so the only reason to cap it at all
/// is to reject an accidental paste of something absurd.
pub const MIN_PASSWORD_LEN: usize = 8;
pub const MAX_PASSWORD_LEN: usize = 128;

#[derive(Debug)]
pub enum ProductOwnerError {
    /// `setup` was called but a credential already exists on this install —
    /// refused rather than silently overwriting it, so discovering the
    /// hidden entry point on an already-configured install can never be
    /// used to hijack the account by "setting" a new password over it.
    AlreadyConfigured,
    /// `verify`/login was attempted before any credential has been set.
    NotConfigured,
    InvalidLength,
    InvalidPassword,
    Hash(String),
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for ProductOwnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProductOwnerError::AlreadyConfigured => {
                write!(f, "A credential is already set on this install")
            }
            ProductOwnerError::NotConfigured => write!(f, "No credential has been set on this install yet"),
            ProductOwnerError::InvalidLength => {
                write!(f, "Password must be {}-{} characters", MIN_PASSWORD_LEN, MAX_PASSWORD_LEN)
            }
            ProductOwnerError::InvalidPassword => write!(f, "Incorrect password"),
            ProductOwnerError::Hash(msg) => write!(f, "{}", msg),
            ProductOwnerError::Sqlite(err) => write!(f, "database error: {}", err),
        }
    }
}

impl From<rusqlite::Error> for ProductOwnerError {
    fn from(err: rusqlite::Error) -> Self {
        ProductOwnerError::Sqlite(err)
    }
}

fn validate_password(password: &str) -> Result<(), ProductOwnerError> {
    let len = password.chars().count();
    if len < MIN_PASSWORD_LEN || len > MAX_PASSWORD_LEN {
        return Err(ProductOwnerError::InvalidLength);
    }
    Ok(())
}

/// Whether a credential has been set on this install yet — what the hidden
/// entry point's UI uses to decide "show a setup form" vs "show a login
/// form". Deliberately unauthenticated (there's nothing to authenticate
/// against yet on the "no row" branch, and the answer reveals nothing about
/// what the credential is).
pub fn has_account(conn: &Connection) -> Result<bool, rusqlite::Error> {
    conn.query_row("SELECT 1 FROM product_owner_account WHERE id = 1", [], |_| Ok(()))
        .optional()
        .map(|row| row.is_some())
}

/// Sets the initial credential for this install. Refuses if one already
/// exists — see `ProductOwnerError::AlreadyConfigured`.
pub fn setup(conn: &Connection, password: &str) -> Result<(), ProductOwnerError> {
    if has_account(conn)? {
        return Err(ProductOwnerError::AlreadyConfigured);
    }
    validate_password(password)?;

    let hash = hash_pin(password).map_err(ProductOwnerError::Hash)?;
    conn.execute("INSERT INTO product_owner_account (id, pin_hash) VALUES (1, ?1)", params![hash])?;
    Ok(())
}

/// Verifies `password` against the stored hash. `Ok(())` on success; the
/// caller (the Tauri command layer) is what actually stamps the elevated
/// session — this function only ever answers "is this credential correct",
/// never touches session state itself.
pub fn verify(conn: &Connection, password: &str) -> Result<(), ProductOwnerError> {
    let hash: Option<String> =
        conn.query_row("SELECT pin_hash FROM product_owner_account WHERE id = 1", [], |row| row.get(0)).optional()?;
    let hash = hash.ok_or(ProductOwnerError::NotConfigured)?;

    let parsed = PasswordHash::new(&hash).map_err(|e| ProductOwnerError::Hash(e.to_string()))?;
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .map_err(|_| ProductOwnerError::InvalidPassword)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::test_conn;

    #[test]
    fn a_fresh_install_has_no_account() {
        let conn = test_conn();
        assert!(!has_account(&conn).unwrap());
        assert!(matches!(verify(&conn, "whatever123"), Err(ProductOwnerError::NotConfigured)));
    }

    #[test]
    fn setup_then_verify_round_trips() {
        let conn = test_conn();
        setup(&conn, "correct-horse-battery").unwrap();
        assert!(has_account(&conn).unwrap());

        assert!(verify(&conn, "correct-horse-battery").is_ok());
        assert!(matches!(verify(&conn, "wrong-password"), Err(ProductOwnerError::InvalidPassword)));
    }

    #[test]
    fn setup_refuses_to_overwrite_an_existing_credential() {
        let conn = test_conn();
        setup(&conn, "first-password-123").unwrap();

        let err = setup(&conn, "second-password-456").unwrap_err();
        assert!(matches!(err, ProductOwnerError::AlreadyConfigured));
        // The original credential must still be the one that verifies.
        assert!(verify(&conn, "first-password-123").is_ok());
    }

    #[test]
    fn rejects_a_too_short_password() {
        let conn = test_conn();
        assert!(matches!(setup(&conn, "short"), Err(ProductOwnerError::InvalidLength)));
    }

    #[test]
    fn the_password_is_never_stored_in_plaintext() {
        let conn = test_conn();
        setup(&conn, "correct-horse-battery").unwrap();
        let stored: String =
            conn.query_row("SELECT pin_hash FROM product_owner_account WHERE id = 1", [], |row| row.get(0)).unwrap();
        assert_ne!(stored, "correct-horse-battery");
        assert!(stored.starts_with("$argon2"), "must be an argon2 hash, got: {}", stored);
    }
}
