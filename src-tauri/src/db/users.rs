//! Local accounts and PIN authentication.
//!
//! There is no server and no token: the frontend asks Rust to verify a PIN,
//! and on success holds the resulting user in memory for the session. The hash
//! never leaves this module.

use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use rusqlite::{params, Connection, OptionalExtension, Row};
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

/// A user as shown on the user management screen — includes whether the
/// account is active, unlike `User` (the login screen's `list_active` only
/// ever returns accounts that are).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedUser {
    pub id: i64,
    pub name: String,
    pub role: Role,
    pub is_active: bool,
}

#[derive(Debug)]
pub enum AuthError {
    InvalidPin,
    /// PIN is not 4–6 digits.
    MalformedPin,
    UnknownUser,
    DuplicateName(String),
    EmptyName,
    /// Refused: there is exactly one Owner account per installation (the one
    /// seeded at first run, see `db::schema::seed_owner`) — this path is
    /// never a legitimate way to create or promote one. Kept here as a
    /// second line of defense; the actual gate a normal caller hits is the
    /// role-assignment check in `commands::create_user`/`update_user`.
    OwnerRoleNotAssignable,
    /// Refused: the Owner account can never be deactivated, by anyone,
    /// including itself — there being exactly one means there is no other
    /// account left who could create a new one.
    OwnerCannotBeDeactivated,
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
            AuthError::EmptyName => write!(f, "Name cannot be empty"),
            AuthError::OwnerRoleNotAssignable => {
                write!(f, "Only one Owner account is allowed on this installation")
            }
            AuthError::OwnerCannotBeDeactivated => {
                write!(f, "The Owner account can't be deactivated")
            }
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

/// Every account, active or not — the user management screen, where
/// deactivated staff still need to be visible (and reactivatable).
pub fn list_all(conn: &Connection) -> Result<Vec<ManagedUser>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, name, role, is_active FROM users ORDER BY
           CASE role WHEN 'owner' THEN 0 WHEN 'admin' THEN 1 ELSE 2 END, name",
    )?;
    let rows = stmt.query_map([], |row| {
        let role: String = row.get("role")?;
        Ok(ManagedUser {
            id: row.get("id")?,
            name: row.get("name")?,
            role: Role::parse(&role).unwrap_or(Role::Cashier),
            is_active: row.get::<_, i64>("is_active")? != 0,
        })
    })?;
    rows.collect()
}

/// The current role of an account, if it exists — lets the command layer
/// authorize who may edit/deactivate whom (see `commands::update_user` /
/// `set_user_active`) without pulling a whole row.
pub fn role_of(conn: &Connection, user_id: i64) -> Result<Option<Role>, rusqlite::Error> {
    conn.query_row("SELECT role FROM users WHERE id = ?1", params![user_id], |row| {
        let role: String = row.get(0)?;
        Ok(Role::parse(&role).unwrap_or(Role::Cashier))
    })
    .optional()
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
    if matches!(role, Role::Owner) {
        return Err(AuthError::OwnerRoleNotAssignable);
    }
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

/// Renames a user and/or changes their role — a full-row replace of the
/// editable fields, same convention `inventory::update_item` uses. Does not
/// touch the PIN; that's `set_pin`'s job, kept separate so "reset PIN" and
/// "edit profile" can't accidentally clobber each other on the frontend.
pub fn update(conn: &Connection, user_id: i64, name: &str, role: Role) -> Result<User, AuthError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AuthError::EmptyName);
    }

    let changed = conn
        .execute(
            "UPDATE users SET name = ?1, role = ?2 WHERE id = ?3",
            params![name, role.as_str(), user_id],
        )
        .map_err(|err| match err {
            rusqlite::Error::SqliteFailure(e, _) if e.extended_code == 2067 => {
                AuthError::DuplicateName(name.to_string())
            }
            other => AuthError::Sqlite(other),
        })?;

    if changed == 0 {
        return Err(AuthError::UnknownUser);
    }
    Ok(User { id: user_id, name: name.to_string(), role })
}

/// Deactivates or reactivates an account (soft — never a hard delete, the
/// same convention `inventory::delete_item` uses for items with history;
/// here it's unconditional since a user always "has history" the moment
/// they've rung up a sale). Refuses to deactivate the Owner account,
/// unconditionally — there is exactly one per installation (see
/// `db::schema::seed_owner`), and nobody, including the Owner themselves,
/// may take it offline through this path. Kept here as a second line of
/// defense; the actual gate a normal caller hits is in
/// `commands::set_user_active`.
pub fn set_active(conn: &Connection, user_id: i64, is_active: bool) -> Result<(), AuthError> {
    if !is_active {
        let target_role: Option<String> = conn
            .query_row("SELECT role FROM users WHERE id = ?1", params![user_id], |row| row.get(0))
            .optional()?;

        if target_role.as_deref() == Some(Role::Owner.as_str()) {
            return Err(AuthError::OwnerCannotBeDeactivated);
        }
    }

    let changed = conn.execute(
        "UPDATE users SET is_active = ?1 WHERE id = ?2",
        params![is_active as i64, user_id],
    )?;
    if changed == 0 {
        return Err(AuthError::UnknownUser);
    }
    Ok(())
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

    #[test]
    fn list_all_includes_deactivated_accounts_that_list_active_hides() {
        let conn = test_conn();
        let cashier = create(&conn, "Sara", "4321", Role::Cashier).unwrap();
        set_active(&conn, cashier.id, false).unwrap();

        assert!(!list_active(&conn).unwrap().iter().any(|u| u.id == cashier.id));
        let all = list_all(&conn).unwrap();
        let managed = all.iter().find(|u| u.id == cashier.id).unwrap();
        assert!(!managed.is_active);
    }

    #[test]
    fn update_renames_and_reassigns_role() {
        let conn = test_conn();
        let cashier = create(&conn, "Sara", "4321", Role::Cashier).unwrap();
        let updated = update(&conn, cashier.id, "Sara Khan", Role::Admin).unwrap();
        assert_eq!(updated.name, "Sara Khan");
        assert_eq!(updated.role, Role::Admin);
    }

    #[test]
    fn update_rejects_a_blank_name_or_unknown_user() {
        let conn = test_conn();
        let cashier = create(&conn, "Sara", "4321", Role::Cashier).unwrap();
        assert!(matches!(update(&conn, cashier.id, "   ", Role::Cashier), Err(AuthError::EmptyName)));
        assert!(matches!(update(&conn, 999_999, "Nobody", Role::Cashier), Err(AuthError::UnknownUser)));
    }

    #[test]
    fn set_active_deactivates_and_reactivates_a_non_owner() {
        let conn = test_conn();
        let cashier = create(&conn, "Sara", "4321", Role::Cashier).unwrap();
        set_active(&conn, cashier.id, false).unwrap();
        assert!(matches!(authenticate(&conn, cashier.id, "4321"), Err(AuthError::UnknownUser)));

        set_active(&conn, cashier.id, true).unwrap();
        assert!(authenticate(&conn, cashier.id, "4321").is_ok());
    }

    #[test]
    fn set_active_refuses_to_deactivate_the_owner() {
        let conn = test_conn();
        let err = set_active(&conn, owner_id(&conn), false).unwrap_err();
        assert!(matches!(err, AuthError::OwnerCannotBeDeactivated));
    }

    #[test]
    fn create_rejects_assigning_the_owner_role() {
        let conn = test_conn();
        assert!(matches!(
            create(&conn, "Co-Owner", "5678", Role::Owner).unwrap_err(),
            AuthError::OwnerRoleNotAssignable
        ));
    }

    #[test]
    fn role_of_returns_the_current_role_or_none_for_an_unknown_user() {
        let conn = test_conn();
        assert_eq!(role_of(&conn, owner_id(&conn)).unwrap(), Some(Role::Owner));
        assert_eq!(role_of(&conn, 999_999).unwrap(), None);
    }
}
