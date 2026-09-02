//! The vendor authorization gate on first-run setup.
//!
//! A fresh install (no `app_config.onboarding_completed`) has to clear this
//! gate before the setup wizard can finish. The intent is commercial, not
//! security-critical: it stops a copy of the installer being used to stand
//! up an unauthorized second store without the vendor handing over the
//! password. See this module's `LIMITS` note below for an honest account of
//! what that does and does not achieve.
//!
//! **Only the argon2 hash is compiled in — never the plaintext.** A string
//! constant in a shipped binary is readable with `strings`, so embedding
//! the password itself would defeat the entire point. The hash below was
//! generated with the same `db::users::hash_pin` (same crate, same default
//! parameters) used for staff PINs and the product-owner credential.
//!
//! Verification lives here, in Rust, and never in the frontend: the app
//! runs with `withGlobalTauri: true`, so anything the JS bundle can decide
//! on its own can be re-decided from a devtools console. The gate's real
//! teeth are in `commands::update_app_config`, which refuses to flip
//! `onboarding_completed` to true without a grant recorded here — a route
//! guard alone would be bypassable by invoking the command directly.
//!
//! ## LIMITS (read before relying on this)
//!
//! This is a deterrent against casual copying, not a licensing system.
//! Every check runs locally, on hardware the user controls, with no
//! server-side validation. Someone willing to patch the binary, edit the
//! SQLite file to set `onboarding_completed = 1` directly, or copy an
//! already-configured install's app data directory gets past it. What it
//! does stop is the realistic case: someone handed the installer who simply
//! does not have the password.

use std::sync::Mutex;
use std::time::Duration;

use argon2::password_hash::{PasswordHash, PasswordVerifier};
use argon2::Argon2;

/// argon2id hash of the vendor authorization password. Generated once with
/// `db::users::hash_pin`; the plaintext exists only in the vendor's own
/// records and is deliberately not recoverable from this repository or from
/// any binary built out of it.
///
/// Rotating it means generating a new hash the same way and replacing this
/// constant — there is no runtime path that changes it, on purpose: a
/// client-side "change the vendor password" flow would hand the gate's own
/// key to the person it gates.
const VENDOR_PASSWORD_HASH: &str =
    "$argon2id$v=19$m=19456,t=2,p=1$PUPnNHq85vf4UFiogtqXFg$1LCilzChXK+LfS4CsUXh/0iI5d+gh8/2wqUZSc6393I";

/// Base delay applied after a failed attempt, growing by this much per
/// consecutive failure up to [`MAX_FAILURE_DELAY`]. Present to make
/// scripted guessing impractical rather than to inconvenience a vendor
/// mistyping once — the first failure costs a beat, a hundredth costs real
/// time. argon2's own cost already makes each attempt slow; this stacks on
/// top of it.
const FAILURE_DELAY_STEP: Duration = Duration::from_millis(1200);
const MAX_FAILURE_DELAY: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub enum VendorGateError {
    /// The entered password did not match. Deliberately the only failure
    /// the caller can distinguish — nothing here reports *why*, hints at
    /// the password's shape, or says whether a previous attempt was closer.
    Incorrect,
    /// The compiled-in hash could not be parsed. Only reachable if
    /// [`VENDOR_PASSWORD_HASH`] were edited into something malformed, which
    /// the test below exists to catch before a build ships.
    Malformed,
}

impl std::fmt::Display for VendorGateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Intentionally generic: no hint about the password, no contact
            // route, no "forgot password" affordance.
            VendorGateError::Incorrect => write!(f, "Incorrect authorization password"),
            VendorGateError::Malformed => write!(f, "Authorization is unavailable in this build"),
        }
    }
}

/// Whether this process has cleared the vendor gate, plus the consecutive
/// failure count driving the retry delay.
///
/// Memory-only and process-scoped, like every other session in this app:
/// closing the app drops the grant. That is deliberate and costs nothing —
/// the grant only has to survive long enough to finish the setup wizard,
/// and once `onboarding_completed` is true the gate is never consulted
/// again on that install.
pub struct VendorGate {
    authorized: Mutex<bool>,
    consecutive_failures: Mutex<u32>,
}

impl VendorGate {
    pub fn new() -> Self {
        Self { authorized: Mutex::new(false), consecutive_failures: Mutex::new(0) }
    }

    pub fn is_authorized(&self) -> bool {
        *self.authorized.lock().expect("vendor gate lock poisoned")
    }

    /// Checks `password` against the compiled-in hash, recording a grant on
    /// success. On failure it sleeps for a delay that grows with the
    /// consecutive-failure count *before* returning, so a scripted caller
    /// can't retry faster than the delay allows.
    ///
    /// Neither the password nor the hash is ever logged — the only thing
    /// that leaves this function is success or [`VendorGateError`].
    pub fn verify(&self, password: &str) -> Result<(), VendorGateError> {
        let parsed = PasswordHash::new(VENDOR_PASSWORD_HASH).map_err(|_| VendorGateError::Malformed)?;

        if Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok() {
            *self.authorized.lock().expect("vendor gate lock poisoned") = true;
            *self.consecutive_failures.lock().expect("vendor gate lock poisoned") = 0;
            return Ok(());
        }

        let delay = {
            let mut failures = self.consecutive_failures.lock().expect("vendor gate lock poisoned");
            *failures = failures.saturating_add(1);
            FAILURE_DELAY_STEP.saturating_mul(*failures).min(MAX_FAILURE_DELAY)
        };
        // The lock is released above before sleeping — holding it across the
        // delay would serialize every concurrent attempt behind this one and
        // make the backoff behave unpredictably.
        std::thread::sleep(delay);

        Err(VendorGateError::Incorrect)
    }
}

impl Default for VendorGate {
    fn default() -> Self {
        Self::new()
    }
}

/// The gate's entire policy, as one pure function so it can be tested
/// directly — `commands::update_app_config` is the only caller, and a
/// `#[tauri::command]` taking `State` can't be unit-tested without an app
/// handle, which would leave the one security-critical decision in this
/// feature untested.
///
/// Reads as: block only the transition that turns an unconfigured install
/// into a configured one, and only when this process has no grant.
/// Everything else — ordinary config edits, and every write on an install
/// that is already set up — passes straight through, which is what makes
/// this a setup-time gate rather than a recurring login.
pub fn setup_is_blocked(completing_onboarding: bool, already_onboarded: bool, authorized: bool) -> bool {
    completing_onboarding && !already_onboarded && !authorized
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The compiled-in constant must be a parseable argon2 hash. Without
    /// this, a typo in the constant would ship a build where *no* password
    /// works and the gate reports "unavailable" on a client's fresh install.
    #[test]
    fn the_embedded_hash_is_a_wellformed_argon2_hash() {
        assert!(VENDOR_PASSWORD_HASH.starts_with("$argon2id$"), "expected an argon2id hash");
        assert!(PasswordHash::new(VENDOR_PASSWORD_HASH).is_ok(), "the embedded hash must parse");
    }

    /// The whole point of hashing: the binary must not contain anything a
    /// `strings` dump could read the password out of.
    #[test]
    fn the_embedded_constant_is_a_hash_not_the_plaintext() {
        // An argon2 PHC string is `$argon2id$v=..$m=..$<salt>$<digest>`. The
        // digest is the only password-derived part and is not reversible.
        let parsed = PasswordHash::new(VENDOR_PASSWORD_HASH).unwrap();
        assert!(parsed.hash.is_some(), "the constant must carry an actual digest");
        assert!(parsed.salt.is_some(), "the constant must be salted");
    }

    #[test]
    fn a_fresh_gate_is_not_authorized() {
        assert!(!VendorGate::new().is_authorized());
    }

    #[test]
    fn a_wrong_password_is_rejected_and_grants_nothing() {
        let gate = VendorGate::new();
        assert!(matches!(gate.verify("definitely-not-it"), Err(VendorGateError::Incorrect)));
        assert!(!gate.is_authorized(), "a failed attempt must never leave a grant behind");
    }

    /// The bypass this whole feature exists to prevent: a fresh install
    /// with no grant must not be able to complete setup, no matter how the
    /// attempt arrives (the wizard, a direct `invoke` from a devtools
    /// console, a patched frontend bundle — they all land on this policy).
    #[test]
    fn completing_setup_on_a_fresh_install_without_a_grant_is_blocked() {
        assert!(setup_is_blocked(true, false, false));
    }

    #[test]
    fn a_granted_process_may_complete_setup() {
        assert!(!setup_is_blocked(true, false, true));
    }

    /// Once an install is set up the gate is done forever — including for a
    /// process that never entered the password, which is what makes this a
    /// one-time gate rather than a login the client would face repeatedly.
    #[test]
    fn an_already_onboarded_install_is_never_gated_again() {
        assert!(!setup_is_blocked(true, true, false));
        assert!(!setup_is_blocked(true, true, true));
    }

    /// Ordinary config edits (a patch that doesn't touch
    /// `onboarding_completed`) must never be caught by the gate, even on a
    /// fresh install mid-wizard.
    #[test]
    fn config_edits_that_do_not_complete_setup_are_never_gated() {
        assert!(!setup_is_blocked(false, false, false));
        assert!(!setup_is_blocked(false, true, false));
    }
}
