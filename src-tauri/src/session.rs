//! Server-side session — the source of truth for "who is currently signed
//! in", independent of anything the frontend claims.
//!
//! There is no token and no persistence (same rationale as `db::users`: this
//! is a single-machine offline till, and closing the app ends the shift), but
//! the session itself must still live here in Rust, not just in the
//! frontend's Zustand store. If a command trusted a `role` string the caller
//! sent in, any stray IPC call — a compromised renderer, a script poking
//! `invoke()` directly, or just a bug — could claim to be an owner. Instead
//! `login` is the only way to populate this state, `logout` the only way to
//! clear it, and every sensitive command calls `require_role` against
//! whatever this managed state actually holds.

use std::sync::Mutex;

use crate::db::users::{Role, User};

pub struct Session {
    current: Mutex<Option<User>>,
}

impl Session {
    pub fn new() -> Self {
        Self { current: Mutex::new(None) }
    }

    pub fn set(&self, user: User) {
        *self.current.lock().expect("session lock poisoned") = Some(user);
    }

    pub fn clear(&self) {
        *self.current.lock().expect("session lock poisoned") = None;
    }

    pub fn current(&self) -> Option<User> {
        self.current.lock().expect("session lock poisoned").clone()
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

/// Fails a command unless someone is signed in *and* holds one of `allowed`'s
/// roles — checked against the session Rust itself set during `login`, never
/// against a value the JS side passed in. Returns the caller so a command
/// that needs to know who's acting (e.g. "only an Owner may create another
/// Owner") doesn't have to look the session up a second time.
pub fn require_role(session: &Session, allowed: &[Role]) -> Result<User, String> {
    let user = session.current().ok_or_else(|| "Not signed in".to_string())?;
    if allowed.contains(&user.role) {
        Ok(user)
    } else {
        Err("You don't have permission to do this".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner() -> User {
        User { id: 1, name: "Owner".into(), role: Role::Owner }
    }
    fn cashier() -> User {
        User { id: 2, name: "Cashier".into(), role: Role::Cashier }
    }

    #[test]
    fn require_role_rejects_when_nobody_is_signed_in() {
        let session = Session::new();
        assert!(require_role(&session, &[Role::Owner, Role::Admin]).is_err());
    }

    #[test]
    fn require_role_rejects_a_role_not_in_the_allowed_list() {
        let session = Session::new();
        session.set(cashier());
        assert!(require_role(&session, &[Role::Owner, Role::Admin]).is_err());
    }

    #[test]
    fn require_role_accepts_an_allowed_role_and_returns_the_user() {
        let session = Session::new();
        session.set(owner());
        let user = require_role(&session, &[Role::Owner, Role::Admin]).unwrap();
        assert_eq!(user.id, 1);
    }

    #[test]
    fn logout_clears_the_session() {
        let session = Session::new();
        session.set(owner());
        session.clear();
        assert!(session.current().is_none());
    }
}
