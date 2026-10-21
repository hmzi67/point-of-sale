//! The shared "sensitive area" PIN — one installation-wide PIN, distinct
//! from any individual staff account's login PIN, that gates Attendance,
//! Expenses, Salary, Employees and Reports for every role (Owner, Admin,
//! Cashier) on every visit. A Cashier who doesn't otherwise have access to
//! these screens can still open one for a one-off task if whoever knows the
//! PIN (typically an Owner/Admin) enters it — the PIN itself is the
//! authorization, not the caller's role. See `commands::require_area_access`
//! (the server-side check every command behind these five screens actually
//! runs) and `area_access_session::AreaAccessSession` (the short-lived grant
//! a successful `verify` call produces, since re-checking a PIN on literally
//! every data-fetch call would mean sending it, in the clear, over and over).
//!
//! Stored as an argon2 hash on `app_config.sensitive_area_pin_hash` (see
//! `db/config.rs`; hashing reuses `db::users::hash_pin`/`validate_pin` — same
//! 4–6-digit-numeric policy as a login PIN, different secret). Seeded to
//! `db::schema::DEFAULT_AREA_PIN` for every install (`seed_area_pin`), the
//! same "ships with a known default, client changes it" convention as the
//! Owner's login PIN — see `set` for how Settings changes it (old PIN,
//! new PIN, no separate "confirm" server-side; matching the two typed PINs
//! is a frontend-only check, same as any other password-confirmation field).

use argon2::password_hash::{PasswordHash, PasswordVerifier};
use argon2::Argon2;
use rusqlite::{params, Connection};

use crate::db::users::{self, AuthError};

#[derive(Debug, thiserror::Error)]
pub enum SecurityPinError {
    #[error("Incorrect PIN")]
    InvalidPin,
    /// PIN is not 4–6 digits — same rule as a login PIN (`db::users`).
    #[error("PIN must be 4-6 digits")]
    MalformedPin,
    #[error("Could not process PIN: {0}")]
    Hash(String),
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

fn validate(pin: &str) -> Result<(), SecurityPinError> {
    users::validate_pin(pin).map_err(|e| match e {
        AuthError::MalformedPin => SecurityPinError::MalformedPin,
        // `validate_pin` only ever actually returns `MalformedPin` — this
        // arm exists so a future variant added there doesn't silently
        // vanish here instead of failing to compile.
        other => SecurityPinError::Hash(other.to_string()),
    })
}

/// The current hash — always present after `db::schema::seed_area_pin` has
/// run once (every install, fresh or upgrading), so callers don't need an
/// explicit "not configured yet" case; `expect` documents that invariant
/// rather than silently swallowing a schema bug as an auth failure.
fn stored_hash(conn: &Connection) -> Result<String, rusqlite::Error> {
    conn.query_row(
        "SELECT sensitive_area_pin_hash FROM app_config WHERE id = 1",
        [],
        |row| row.get(0),
    )
    .map(|hash: Option<String>| {
        hash.expect("sensitive_area_pin_hash must be seeded by db::schema::seed_area_pin before first use")
    })
}

/// Verifies `pin` against the stored hash. This is the actual authorization
/// check behind `commands::security_verify_area_pin` — a correct PIN grants
/// `AreaAccessSession`, an incorrect one just returns `InvalidPin`.
pub fn verify(conn: &Connection, pin: &str) -> Result<(), SecurityPinError> {
    validate(pin)?;
    let hash = stored_hash(conn)?;
    let parsed = PasswordHash::new(&hash).map_err(|e| SecurityPinError::Hash(e.to_string()))?;
    Argon2::default()
        .verify_password(pin.as_bytes(), &parsed)
        .map_err(|_| SecurityPinError::InvalidPin)
}

/// Changes the shared PIN — requires the current one first (Settings' "old
/// PIN, new PIN, confirm new PIN" form; the confirm field never reaches
/// here, it's checked against "new PIN" client-side before this is called,
/// same as any other password-confirmation UI). Reachable only by
/// Owner/Admin at the command layer (`commands::security_set_area_pin`) —
/// Settings itself is admin-only, per `CLAUDE.md`.
pub fn set(conn: &Connection, old_pin: &str, new_pin: &str) -> Result<(), SecurityPinError> {
    verify(conn, old_pin)?;
    validate(new_pin)?;
    let pin_hash = users::hash_pin(new_pin).map_err(SecurityPinError::Hash)?;
    conn.execute("UPDATE app_config SET sensitive_area_pin_hash = ?1 WHERE id = 1", params![pin_hash])?;
    Ok(())
}

/// Which of the two shipped-with-a-known-value credentials are still at
/// that value — drives the "change your default PINs" warning. Worked out by
/// checking the stored hashes against the known defaults, so it needs no
/// extra column and can't drift out of step with the real PINs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultCredentials {
    pub owner_pin_is_default: bool,
    pub area_pin_is_default: bool,
}

pub fn default_credentials(conn: &Connection) -> Result<DefaultCredentials, rusqlite::Error> {
    use crate::db::schema::{DEFAULT_AREA_PIN, DEFAULT_OWNER_PIN};
    let owner_id: Option<i64> = rusqlite::OptionalExtension::optional(conn.query_row(
        "SELECT id FROM users WHERE role = 'owner'",
        [],
        |row| row.get(0),
    ))?;
    let owner_pin_is_default =
        owner_id.is_some_and(|id| users::authenticate(conn, id, DEFAULT_OWNER_PIN).is_ok());
    let area_pin_is_default = verify(conn, DEFAULT_AREA_PIN).is_ok();
    Ok(DefaultCredentials { owner_pin_is_default, area_pin_is_default })
}

/// The forced first-login change: replaces whichever of the two shipped
/// PINs are still at their default. A PIN somebody already chose is never
/// touched, so this needs no "current PIN", and a new PIN equal to the
/// default is refused. Both are checked before either is written.
pub fn replace_defaults(conn: &Connection, owner_pin: Option<&str>, area_pin: Option<&str>) -> Result<(), String> {
    use crate::db::schema::{DEFAULT_AREA_PIN, DEFAULT_OWNER_PIN};
    let current = default_credentials(conn).map_err(|e| e.to_string())?;
    for (pin, default) in [(owner_pin, DEFAULT_OWNER_PIN), (area_pin, DEFAULT_AREA_PIN)] {
        if let Some(pin) = pin {
            users::validate_pin(pin).map_err(|e| e.to_string())?;
            if pin == default {
                return Err("Choose a PIN other than the default".into());
            }
        }
    }
    if let Some(pin) = owner_pin {
        if !current.owner_pin_is_default {
            return Err("The Owner PIN has already been changed".into());
        }
        let owner_id: i64 =
            conn.query_row("SELECT id FROM users WHERE role = 'owner'", [], |row| row.get(0)).map_err(|e| e.to_string())?;
        users::set_pin(conn, owner_id, pin).map_err(|e| e.to_string())?;
    }
    if let Some(pin) = area_pin {
        if !current.area_pin_is_default {
            return Err("The area PIN has already been changed".into());
        }
        set(conn, DEFAULT_AREA_PIN, pin).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::{test_conn, DEFAULT_AREA_PIN};

    #[test]
    fn replace_defaults_changes_only_pins_still_at_their_default() {
        let conn = test_conn();
        assert!(replace_defaults(&conn, None, Some(DEFAULT_AREA_PIN)).is_err(), "the default itself is refused");
        assert!(replace_defaults(&conn, Some("12"), None).is_err(), "malformed PINs are refused");

        replace_defaults(&conn, Some("8642"), Some("5678")).unwrap();
        assert_eq!(
            default_credentials(&conn).unwrap(),
            DefaultCredentials { owner_pin_is_default: false, area_pin_is_default: false }
        );
        verify(&conn, "5678").unwrap();

        // Once chosen, neither can be overwritten through this path.
        assert!(replace_defaults(&conn, Some("1111"), None).is_err());
        assert!(replace_defaults(&conn, None, Some("2222")).is_err());
        verify(&conn, "5678").unwrap();
    }

    #[test]
    fn a_fresh_install_reports_both_pins_as_default_until_changed() {
        let conn = test_conn();
        assert_eq!(
            default_credentials(&conn).unwrap(),
            DefaultCredentials { owner_pin_is_default: true, area_pin_is_default: true }
        );

        set(&conn, DEFAULT_AREA_PIN, "5678").unwrap();
        let owner_id: i64 = conn.query_row("SELECT id FROM users WHERE role = 'owner'", [], |r| r.get(0)).unwrap();
        users::set_pin(&conn, owner_id, "8642").unwrap();
        assert_eq!(
            default_credentials(&conn).unwrap(),
            DefaultCredentials { owner_pin_is_default: false, area_pin_is_default: false }
        );
    }

    #[test]
    fn a_fresh_install_verifies_with_the_default_pin() {
        let conn = test_conn();
        verify(&conn, DEFAULT_AREA_PIN).unwrap();
    }

    #[test]
    fn a_wrong_pin_is_rejected() {
        let conn = test_conn();
        assert!(matches!(verify(&conn, "9999"), Err(SecurityPinError::InvalidPin)));
    }

    #[test]
    fn a_malformed_pin_is_rejected_without_touching_the_hash() {
        let conn = test_conn();
        assert!(matches!(verify(&conn, "12"), Err(SecurityPinError::MalformedPin)));
        assert!(matches!(verify(&conn, "abcd"), Err(SecurityPinError::MalformedPin)));
    }

    #[test]
    fn set_changes_the_pin_and_the_old_one_stops_working() {
        let conn = test_conn();
        set(&conn, DEFAULT_AREA_PIN, "5678").unwrap();

        verify(&conn, "5678").unwrap();
        assert!(matches!(verify(&conn, DEFAULT_AREA_PIN), Err(SecurityPinError::InvalidPin)));
    }

    #[test]
    fn set_rejects_a_wrong_old_pin_without_changing_anything() {
        let conn = test_conn();
        assert!(matches!(set(&conn, "9999", "5678"), Err(SecurityPinError::InvalidPin)));

        // Still the original default — the failed attempt didn't partially apply.
        verify(&conn, DEFAULT_AREA_PIN).unwrap();
    }

    #[test]
    fn set_rejects_a_malformed_new_pin() {
        let conn = test_conn();
        assert!(matches!(
            set(&conn, DEFAULT_AREA_PIN, "12"),
            Err(SecurityPinError::MalformedPin)
        ));
    }
}
