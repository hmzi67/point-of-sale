//! Local accounts and PIN authentication.
//!
//! There is no server and no token: the frontend asks Rust to verify a PIN,
//! and on success holds the resulting user in memory for the session. The hash
//! never leaves this module.

use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};

pub const MIN_PIN_LEN: usize = 4;
pub const MAX_PIN_LEN: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Owner,
    Admin,
    Cashier,
}

impl Role {
    fn as_str(self) -> &'static str {
        match self {
            Role::Owner => "owner",
            Role::Admin => "admin",
            Role::Cashier => "cashier",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "owner" => Some(Role::Owner),
            "admin" => Some(Role::Admin),
            "cashier" => Some(Role::Cashier),
            _ => None,
        }
    }
}

/// A user as the frontend sees it — no hash, ever.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: i64,
    pub name: String,
    pub role: Role,
}

#[derive(Debug)]
pub enum AuthError {
    InvalidPin,
    /// PIN is not 4–6 digits.
    MalformedPin,
    UnknownUser,
    DuplicateName(String),
    Hash(String),
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::InvalidPin => write!(f, "Incorrect PIN"),
            AuthError::MalformedPin => write!(
                f,
                "PIN must be {}-{} digits",
                MIN_PIN_LEN, MAX_PIN_LEN
            ),
            AuthError::UnknownUser => write!(f, "That user no longer exists"),
            AuthError::DuplicateName(name) => write!(f, "A user named {} already exists", name),
            AuthError::Hash(msg) => write!(f, "Could not process PIN: {}", msg),
            AuthError::Sqlite(err) => write!(f, "database error: {}", err),
        }
    }
}

impl From<rusqlite::Error> for AuthError {
    fn from(err: rusqlite::Error) -> Self {
        AuthError::Sqlite(err)
    }
}

fn validate_pin(pin: &str) -> Result<(), AuthError> {
    let len = pin.chars().count();
    if len < MIN_PIN_LEN || len > MAX_PIN_LEN || !pin.chars().all(|c| c.is_ascii_digit()) {
        return Err(AuthError::MalformedPin);
    }
    Ok(())
}

pub fn hash_pin(pin: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(pin.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| e.to_string())
}

fn from_row(row: &Row<'_>) -> Result<User, rusqlite::Error> {
    let role: String = row.get("role")?;
    Ok(User {
        id: row.get("id")?,
        name: row.get("name")?,
        // The CHECK constraint on the column makes an unknown role impossible;
        // defaulting to the least-privileged role is the safe fallback anyway.
        role: Role::parse(&role).unwrap_or(Role::Cashier),
    })
}

/// Accounts offered on the login screen.
pub fn list_active(conn: &Connection) -> Result<Vec<User>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, name, role FROM users WHERE is_active = 1 ORDER BY
           CASE role WHEN 'owner' THEN 0 WHEN 'admin' THEN 1 ELSE 2 END, name",
    )?;
    let rows = stmt.query_map([], from_row)?;
    rows.collect()
}

/// Verifies a PIN against the stored argon2 hash.
pub fn authenticate(conn: &Connection, user_id: i64, pin: &str) -> Result<User, AuthError> {
    validate_pin(pin)?;

    let (user, pin_hash) = conn
        .query_row(
            "SELECT id, name, role, pin_hash FROM users WHERE id = ?1 AND is_active = 1",
            params![user_id],
            |row| Ok((from_row(row)?, row.get::<_, String>("pin_hash")?)),
        )
        .map_err(|err| match err {
            rusqlite::Error::QueryReturnedNoRows => AuthError::UnknownUser,
            other => AuthError::Sqlite(other),
        })?;

    let parsed = PasswordHash::new(&pin_hash).map_err(|e| AuthError::Hash(e.to_string()))?;
    Argon2::default()
        .verify_password(pin.as_bytes(), &parsed)
        .map_err(|_| AuthError::InvalidPin)?;

    Ok(user)
}

pub fn create(conn: &Connection, name: &str, pin: &str, role: Role) -> Result<User, AuthError> {
    validate_pin(pin)?;
    let pin_hash = hash_pin(pin).map_err(AuthError::Hash)?;

    conn.execute(
        "INSERT INTO users (name, pin_hash, role) VALUES (?1, ?2, ?3)",
        params![name, pin_hash, role.as_str()],
    )
    .map_err(|err| match err {
        rusqlite::Error::SqliteFailure(e, _) if e.extended_code == 2067 => {
            AuthError::DuplicateName(name.to_string())
        }
        other => AuthError::Sqlite(other),
    })?;

    Ok(User {
        id: conn.last_insert_rowid(),
        name: name.to_string(),
        role,
    })
}

/// Changes a user's PIN. Used by onboarding to replace the seeded default.
pub fn set_pin(conn: &Connection, user_id: i64, new_pin: &str) -> Result<(), AuthError> {
    validate_pin(new_pin)?;
    let pin_hash = hash_pin(new_pin).map_err(AuthError::Hash)?;

    let changed = conn.execute(
        "UPDATE users SET pin_hash = ?1 WHERE id = ?2",
        params![pin_hash, user_id],
    )?;

    if changed == 0 {
        return Err(AuthError::UnknownUser);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::{test_conn, DEFAULT_OWNER_PIN};

    fn owner_id(conn: &Connection) -> i64 {
        conn.query_row("SELECT id FROM users WHERE role = 'owner'", [], |row| {
            row.get(0)
        })
        .unwrap()
    }

    #[test]
    fn seeded_owner_logs_in_with_the_default_pin() {
        let conn = test_conn();
        let user = authenticate(&conn, owner_id(&conn), DEFAULT_OWNER_PIN).unwrap();
        assert_eq!(user.role, Role::Owner);
    }

    #[test]
    fn wrong_pin_is_rejected() {
        let conn = test_conn();
        let err = authenticate(&conn, owner_id(&conn), "9999").unwrap_err();
        assert!(matches!(err, AuthError::InvalidPin));
    }

    #[test]
    fn pin_must_be_four_to_six_digits() {
        let conn = test_conn();
        assert!(matches!(
            authenticate(&conn, owner_id(&conn), "12").unwrap_err(),
            AuthError::MalformedPin
        ));
        assert!(matches!(
            create(&conn, "Ali", "abcd", Role::Cashier).unwrap_err(),
            AuthError::MalformedPin
        ));
    }

    #[test]
    fn pin_is_never_stored_in_plaintext() {
        let conn = test_conn();
        let hash: String = conn
            .query_row("SELECT pin_hash FROM users WHERE role = 'owner'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(hash.starts_with("$argon2"), "expected an argon2 hash");
        assert!(!hash.contains(DEFAULT_OWNER_PIN));
    }

    #[test]
    fn created_user_can_log_in_and_change_pin() {
        let conn = test_conn();
        let cashier = create(&conn, "Sara", "4321", Role::Cashier).unwrap();
        assert_eq!(authenticate(&conn, cashier.id, "4321").unwrap().role, Role::Cashier);

        set_pin(&conn, cashier.id, "778899").unwrap();
        assert!(matches!(
            authenticate(&conn, cashier.id, "4321").unwrap_err(),
            AuthError::InvalidPin
        ));
        assert!(authenticate(&conn, cashier.id, "778899").is_ok());
    }

    #[test]
    fn duplicate_names_are_rejected() {
        let conn = test_conn();
        create(&conn, "Sara", "4321", Role::Cashier).unwrap();
        assert!(matches!(
            create(&conn, "Sara", "4321", Role::Cashier).unwrap_err(),
            AuthError::DuplicateName(_)
        ));
    }
}
