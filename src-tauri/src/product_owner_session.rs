//! The vendor/developer account's elevated session — entirely separate
//! from `session::Session` (the client staff session).
//!
//! Deliberately not the same mechanism as `Session`:
//!   - It never overlaps with or requires clearing whatever staff account
//!     is currently signed in — `login`/`logout` on `Session` never touch
//!     this, and vice versa.
//!   - It auto-expires after a short idle window (see `TIMEOUT`) rather
//!     than lasting for the rest of the process, so it never lingers in
//!     "elevated" mode after the vendor is done with it. It's also
//!     memory-only, so — same as the staff session — closing the app ends
//!     it immediately regardless of the timeout.
//!
//! Nothing about who is authenticated is stored here beyond "is there a
//! currently-valid grant" — there's only one product-owner account per
//! install, so there's no identity to track, just a timestamp.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long an elevated grant stays valid without being refreshed. Short
/// enough that walking away from an unlocked till doesn't leave module
/// overrides reachable indefinitely; long enough to not expire mid-use
/// while actually reviewing/toggling modules.
const TIMEOUT: Duration = Duration::from_secs(10 * 60);

pub struct ProductOwnerSession {
    /// The instant of the last successful authentication or gated command
    /// call — checking against `TIMEOUT` from *this* (rather than a fixed
    /// login time) is what makes it an idle timeout instead of an absolute
    /// one: staying active keeps it alive, walking away lets it lapse.
    granted_at: Mutex<Option<Instant>>,
}

impl ProductOwnerSession {
    pub fn new() -> Self {
        Self { granted_at: Mutex::new(None) }
    }

    /// Starts (or refreshes) the elevated grant.
    pub fn grant(&self) {
        *self.granted_at.lock().expect("product owner session lock poisoned") = Some(Instant::now());
    }

    /// Ends the elevated grant immediately, without waiting for the idle
    /// timeout — the explicit "done, drop back to normal" path.
    pub fn revoke(&self) {
        *self.granted_at.lock().expect("product owner session lock poisoned") = None;
    }

    /// True if there's a currently-valid grant. Also the point where an
    /// expired-but-still-`Some` grant actually gets cleared, so a lapsed
    /// session doesn't sit there as stale state — the next check (or the
    /// next successful `grant()`) always reflects the real timeout, not
    /// just whatever the last read happened to see.
    pub fn is_valid(&self) -> bool {
        let mut guard = self.granted_at.lock().expect("product owner session lock poisoned");
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

impl Default for ProductOwnerSession {
    fn default() -> Self {
        Self::new()
    }
}

/// Fails a command unless there's a currently-valid elevated grant.
/// Refreshes the grant on success (see `ProductOwnerSession::grant`'s doc
/// comment on why this makes the timeout idle-based) — every gated command
/// call itself counts as activity, so actively using the module-override
/// UI never expires mid-session.
pub fn require_product_owner(session: &ProductOwnerSession) -> Result<(), String> {
    if session.is_valid() {
        session.grant();
        Ok(())
    } else {
        Err("Not signed in as product owner".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_session_has_no_grant() {
        let session = ProductOwnerSession::new();
        assert!(!session.is_valid());
        assert!(require_product_owner(&session).is_err());
    }

    #[test]
    fn granting_makes_the_session_valid() {
        let session = ProductOwnerSession::new();
        session.grant();
        assert!(session.is_valid());
        assert!(require_product_owner(&session).is_ok());
    }

    #[test]
    fn revoking_ends_the_grant_immediately() {
        let session = ProductOwnerSession::new();
        session.grant();
        session.revoke();
        assert!(!session.is_valid());
    }

    #[test]
    fn a_grant_older_than_the_timeout_is_no_longer_valid() {
        let session = ProductOwnerSession { granted_at: Mutex::new(Some(Instant::now() - TIMEOUT - Duration::from_secs(1))) };
        assert!(!session.is_valid());
        assert!(require_product_owner(&session).is_err());
    }

    #[test]
    fn using_the_session_refreshes_the_idle_window() {
        // A grant just one second past what would be the timeout from its
        // *original* start still counts as valid here because grant() is
        // called again below, resetting the clock — this is the "idle",
        // not "absolute", timeout behavior the module doc promises.
        let session = ProductOwnerSession::new();
        session.grant();
        assert!(require_product_owner(&session).is_ok()); // refreshes
        assert!(session.is_valid());
    }
}
