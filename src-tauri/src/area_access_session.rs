//! The server-side grant behind the "sensitive area" PIN
//! (`db::security_pin`) — a short-lived, role-independent unlock for
//! Attendance, Expenses, Salary, Employees and Reports.
//!
//! Deliberately not folded into `session::Session`: signing in/out never
//! touches this (a Cashier's PIN-unlocked visit to Salary shouldn't survive
//! a different Cashier logging in on the same till a minute later — see
//! `commands::logout`, which revokes this alongside clearing the session),
//! and it carries no identity of its own, just "is there currently a valid
//! grant" — same one-bit shape as `product_owner_session::ProductOwnerSession`,
//! which this otherwise mirrors closely. Two real differences from that one:
//!
//!  - It's revoked explicitly when the frontend's `AreaPinGate` unmounts
//!    (leaving one of the five gated screens), not just left to expire —
//!    the whole point of the product decision behind this ("every single
//!    visit" needs its own PIN entry, no session persistence) is that the
//!    unlock shouldn't outlive being on the gated screen, not that it's
//!    merely short-lived.
//!  - `TIMEOUT` is still enforced as a safety net (a crash, a killed
//!    process, or any other path that skips the unmount-triggered revoke)
//!    so a grant can never linger indefinitely — but it's deliberately
//!    shorter than `ProductOwnerSession`'s, since normal use should always
//!    hit the explicit revoke long before the timeout ever matters.

use std::sync::Mutex;
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(5 * 60);

pub struct AreaAccessSession {
    granted_at: Mutex<Option<Instant>>,
}

impl AreaAccessSession {
    pub fn new() -> Self {
        Self { granted_at: Mutex::new(None) }
    }

    /// Starts (or refreshes) the grant — called once a correct PIN is
    /// verified.
    pub fn grant(&self) {
        *self.granted_at.lock().expect("area access session lock poisoned") = Some(Instant::now());
    }

    /// Ends the grant immediately. Called by `commands::security_revoke_area_access`
    /// (the frontend's `AreaPinGate` calls this on unmount) and by `logout`.
    pub fn revoke(&self) {
        *self.granted_at.lock().expect("area access session lock poisoned") = None;
    }

    /// True if there's a currently-valid grant. Also clears an
    /// expired-but-still-`Some` grant on read, same as
    /// `ProductOwnerSession::is_valid` — see that one's doc comment for why.
    pub fn is_valid(&self) -> bool {
        let mut guard = self.granted_at.lock().expect("area access session lock poisoned");
        match *guard {
            Some(when) if when.elapsed() < TIMEOUT => true,
            Some(_) => {
                *guard = None;
                false
            }
            None => false,
        }
    }
}

impl Default for AreaAccessSession {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_session_has_no_grant() {
        let session = AreaAccessSession::new();
        assert!(!session.is_valid());
    }

    #[test]
    fn granting_makes_the_session_valid() {
        let session = AreaAccessSession::new();
        session.grant();
        assert!(session.is_valid());
    }

    #[test]
    fn revoking_ends_the_grant_immediately() {
        let session = AreaAccessSession::new();
        session.grant();
        session.revoke();
        assert!(!session.is_valid());
    }
}
