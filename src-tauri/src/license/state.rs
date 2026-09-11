//! Turns a verified payload plus the clocks into the one [`LicenseState`]
//! the rest of the app acts on. Pure: every input is a value, so every
//! branch below is pinned by a test with fixed timestamps.
//!
//! ## Clock tampering — what this does and does not catch
//!
//! A fully offline app cannot prove what time it is. What it can do is
//! refuse to believe a clock that contradicts things it has already seen:
//!
//! - **Server floor**: local time earlier than the payload's `issued_at`
//!   is impossible — the server signed that timestamp and we received it.
//! - **High-water mark**: local time earlier than the latest local time
//!   this install has already observed (`high_water_local_ts`, advanced on
//!   every launch) means the clock went backwards.
//! - **Check-in**: more than `max_offline_days` since the last successful
//!   online refresh locks billing until one happens. This is the bound on
//!   how long a clock frozen just before expiry can keep a lapsed licence
//!   alive.
//!
//! All three resolve the same way — go online, refresh — because a fresh
//! payload carries the server's time, which resets the floor and the mark.
//! What none of this stops: patching the binary. See `vendor_gate.rs`'s
//! LIMITS note; the same honesty applies here.

use serde::Serialize;

use super::verify::{verify_signed, LicensePayload, LicenseStatus};
use crate::db::license::StoredLicense;

/// How far ahead of expiry the in-app warning starts showing.
pub const EXPIRY_WARNING_WINDOW_SECS: i64 = 24 * 60 * 60;

/// Slack allowed before a backwards clock counts as tampering. Covers a
/// user correcting a clock that was a few minutes fast, NTP adjustments,
/// and the ordinary jitter between two machines' idea of "now" — but not a
/// day. Applied to both the server floor and the high-water mark.
pub const CLOCK_TOLERANCE_SECS: i64 = 10 * 60;

/// What the app is allowed to do right now, and why. Serialised to the
/// frontend as `{ kind: "...", ...fields }`; the frontend only ever
/// renders this, it never recomputes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LicenseState {
    /// No licence has ever been stored on this install.
    Missing,
    /// Entitled. `expiring_soon` is the 1-day warning; `expires_at` is
    /// `None` for a perpetual licence.
    #[serde(rename_all = "camelCase")]
    Ok {
        expires_at: Option<i64>,
        expiring_soon: bool,
    },
    /// Past `expires_at`, or the server said so. Locks billing.
    #[serde(rename_all = "camelCase")]
    Expired { expired_at: Option<i64> },
    /// The server reported the licence revoked. Locks billing.
    Revoked,
    /// The stored payload was issued to a different machine. Locks billing.
    DeviceMismatch,
    /// The local clock is behind something we have already seen. Locks
    /// billing until an online refresh resets the reference points.
    #[serde(rename_all = "camelCase")]
    ClockTampered { reason: ClockProblem },
    /// Too long since the last successful online refresh. Locks billing
    /// until one succeeds.
    #[serde(rename_all = "camelCase")]
    CheckInRequired {
        last_online_check: i64,
        max_offline_days: u32,
    },
    /// The stored bytes failed verification (edited, wrong key, newer
    /// format). Locks billing; the message says which.
    Invalid { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ClockProblem {
    BehindServerTime,
    BehindLastSeenTime,
}

impl LicenseState {
    /// The single question the enforcing commands ask.
    pub fn allows_billing(&self) -> bool {
        matches!(self, LicenseState::Ok { .. })
    }
}

/// The local-clock facts `evaluate` needs, separated from the payload so a
/// test can move each one independently.
#[derive(Debug, Clone, Copy)]
pub struct Clocks {
    /// Local unix time now.
    pub now: i64,
    /// `license_status.high_water_local_ts`.
    pub high_water: i64,
    /// `license_status.last_online_check`.
    pub last_online_check: i64,
}

/// Evaluates an already-verified payload for this device at this moment.
///
/// Check order is deliberate: the two conditions that make *every other
/// field untrustworthy* (wrong device, lying clock) come first; then the
/// server's own verdict; then expiry, which is what the clock feeds into;
/// then the check-in bound; and only a payload that clears all of that is
/// `Ok`. An expired licence therefore reports `Expired`, not
/// `CheckInRequired`, even when both are true — the more actionable answer.
pub fn evaluate(payload: &LicensePayload, this_device_fingerprint: &str, clocks: Clocks) -> LicenseState {
    if payload.device_fingerprint != this_device_fingerprint {
        return LicenseState::DeviceMismatch;
    }
    if clocks.now < payload.issued_at - CLOCK_TOLERANCE_SECS {
        return LicenseState::ClockTampered { reason: ClockProblem::BehindServerTime };
    }
    if clocks.now < clocks.high_water - CLOCK_TOLERANCE_SECS {
        return LicenseState::ClockTampered { reason: ClockProblem::BehindLastSeenTime };
    }

    match payload.status {
        LicenseStatus::Revoked | LicenseStatus::Unknown => return LicenseState::Revoked,
        LicenseStatus::Expired => return LicenseState::Expired { expired_at: payload.expires_at },
        LicenseStatus::Active => {}
    }
    if let Some(expires_at) = payload.expires_at {
        if clocks.now >= expires_at {
            return LicenseState::Expired { expired_at: Some(expires_at) };
        }
    }

    let max_offline_secs = i64::from(payload.max_offline_days) * 24 * 60 * 60;
    if clocks.now - clocks.last_online_check > max_offline_secs {
        return LicenseState::CheckInRequired {
            last_online_check: clocks.last_online_check,
            max_offline_days: payload.max_offline_days,
        };
    }

    LicenseState::Ok {
        expires_at: payload.expires_at,
        expiring_soon: payload
            .expires_at
            .is_some_and(|expires_at| expires_at - clocks.now <= EXPIRY_WARNING_WINDOW_SECS),
    }
}

/// The launch-time path: verify what is in the database, then evaluate it.
/// `None` (no row) is `Missing`; a row that fails verification is
/// `Invalid` with the verifier's message.
pub fn evaluate_stored(stored: Option<&StoredLicense>, this_device_fingerprint: &str, now: i64) -> LicenseState {
    let Some(stored) = stored else {
        return LicenseState::Missing;
    };
    match verify_signed(&stored.payload_b64, &stored.signature_b64) {
        Ok(payload) => evaluate(
            &payload,
            this_device_fingerprint,
            Clocks { now, high_water: stored.high_water_local_ts, last_online_check: stored.last_online_check },
        ),
        Err(err) => LicenseState::Invalid { message: err.to_string() },
    }
}

/// Whether a freshly fetched (verified) payload may replace the stored one.
/// A payload is never replaced by an older one: otherwise a backup of the
/// database taken while the licence was active could be restored over a
/// later "revoked" payload — or, more simply, a stale response could undo
/// a newer one. Equal `issued_at` is accepted (idempotent re-fetch).
pub fn accepts_replacement(stored_issued_at: Option<i64>, incoming_issued_at: i64) -> bool {
    stored_issued_at.is_none_or(|stored| incoming_issued_at >= stored)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FP: &str = "fingerprint-of-this-device";
    const DAY: i64 = 24 * 60 * 60;
    const ISSUED: i64 = 1_760_000_000;

    fn payload() -> LicensePayload {
        LicensePayload {
            v: 1,
            kid: "test".into(),
            license_key: "DIWAN-A".into(),
            status: LicenseStatus::Active,
            issued_at: ISSUED,
            expires_at: Some(ISSUED + 30 * DAY),
            device_fingerprint: FP.into(),
            max_offline_days: 30,
        }
    }

    /// A clock consistent with the payload having just arrived.
    fn clocks_at(now: i64) -> Clocks {
        Clocks { now, high_water: now.min(ISSUED), last_online_check: ISSUED }
    }

    // --- the happy path and the warning window ---------------------------------

    #[test]
    fn a_fresh_active_licence_is_ok_and_not_expiring() {
        assert_eq!(
            evaluate(&payload(), FP, clocks_at(ISSUED + 1)),
            LicenseState::Ok { expires_at: Some(ISSUED + 30 * DAY), expiring_soon: false }
        );
    }

    #[test]
    fn the_warning_starts_exactly_one_day_before_expiry() {
        let expires = ISSUED + 30 * DAY;
        let just_before_window = clocks_at(expires - DAY - 1);
        let at_window = clocks_at(expires - DAY);
        assert!(matches!(evaluate(&payload(), FP, just_before_window), LicenseState::Ok { expiring_soon: false, .. }));
        assert!(matches!(evaluate(&payload(), FP, at_window), LicenseState::Ok { expiring_soon: true, .. }));
    }

    #[test]
    fn a_perpetual_licence_never_warns_and_never_expires() {
        let mut p = payload();
        p.expires_at = None;
        let far_future = clocks_at(ISSUED + 20 * 365 * DAY);
        // keep check-in satisfied so only expiry is under test
        let clocks = Clocks { last_online_check: far_future.now, ..far_future };
        assert_eq!(evaluate(&p, FP, clocks), LicenseState::Ok { expires_at: None, expiring_soon: false });
    }

    // --- expiry: the offline case the whole feature is for ------------------------

    /// Activated once, used offline, the stored expiry passes: locked,
    /// with no server involved.
    #[test]
    fn a_locally_stored_expiry_in_the_past_locks_without_phoning_home() {
        let expires = ISSUED + 30 * DAY;
        assert!(matches!(evaluate(&payload(), FP, clocks_at(expires - 1)), LicenseState::Ok { .. }));
        assert_eq!(evaluate(&payload(), FP, clocks_at(expires)), LicenseState::Expired { expired_at: Some(expires) });
    }

    #[test]
    fn the_servers_expired_verdict_is_honoured_regardless_of_the_local_clock() {
        let mut p = payload();
        p.status = LicenseStatus::Expired;
        assert!(matches!(evaluate(&p, FP, clocks_at(ISSUED + 1)), LicenseState::Expired { .. }));
    }

    #[test]
    fn revoked_and_unknown_statuses_both_lock_as_revoked() {
        for status in [LicenseStatus::Revoked, LicenseStatus::Unknown] {
            let mut p = payload();
            p.status = status;
            assert_eq!(evaluate(&p, FP, clocks_at(ISSUED + 1)), LicenseState::Revoked);
        }
    }

    #[test]
    fn expiry_wins_over_check_in_required_because_it_is_more_actionable() {
        // 60 days on: past a 30-day expiry AND past a 30-day check-in.
        let clocks = Clocks { now: ISSUED + 60 * DAY, high_water: ISSUED, last_online_check: ISSUED };
        assert!(matches!(evaluate(&payload(), FP, clocks), LicenseState::Expired { .. }));
    }

    // --- device binding ------------------------------------------------------------

    #[test]
    fn a_payload_for_another_machine_is_refused_before_anything_else() {
        let mut p = payload();
        p.status = LicenseStatus::Revoked; // would otherwise be Revoked
        assert_eq!(evaluate(&p, "some-other-machine", clocks_at(ISSUED + 1)), LicenseState::DeviceMismatch);
    }

    // --- clock rollback ------------------------------------------------------------

    #[test]
    fn a_clock_behind_the_servers_issue_time_is_tampering() {
        let clocks = Clocks { now: ISSUED - CLOCK_TOLERANCE_SECS - 1, high_water: 0, last_online_check: 0 };
        assert_eq!(
            evaluate(&payload(), FP, clocks),
            LicenseState::ClockTampered { reason: ClockProblem::BehindServerTime }
        );
    }

    #[test]
    fn a_clock_behind_the_high_water_mark_is_tampering() {
        // Ran yesterday at `seen`; today the clock reads two days earlier.
        let seen = ISSUED + 10 * DAY;
        let clocks = Clocks { now: seen - 2 * DAY, high_water: seen, last_online_check: ISSUED };
        assert_eq!(
            evaluate(&payload(), FP, clocks),
            LicenseState::ClockTampered { reason: ClockProblem::BehindLastSeenTime }
        );
    }

    /// The tolerance exists so a legitimately corrected few-minutes-fast
    /// clock does not lock a shop out.
    #[test]
    fn a_small_backwards_step_within_tolerance_is_not_tampering() {
        let seen = ISSUED + 10 * DAY;
        let clocks = Clocks { now: seen - CLOCK_TOLERANCE_SECS, high_water: seen, last_online_check: ISSUED };
        assert!(matches!(evaluate(&payload(), FP, clocks), LicenseState::Ok { .. }));
        let clocks = Clocks { now: ISSUED - CLOCK_TOLERANCE_SECS, high_water: 0, last_online_check: ISSUED };
        assert!(matches!(evaluate(&payload(), FP, clocks), LicenseState::Ok { .. }));
    }

    /// Freezing the clock to dodge expiry runs into the check-in bound.
    #[test]
    fn a_frozen_clock_eventually_hits_check_in_required() {
        // The clock is held one second before expiry, but the LAST online
        // check was 31 days ago by that same clock.
        let expires = ISSUED + 30 * DAY;
        let clocks = Clocks { now: expires - 1, high_water: expires - 1, last_online_check: expires - 1 - 31 * DAY };
        assert_eq!(
            evaluate(&payload(), FP, clocks),
            LicenseState::CheckInRequired { last_online_check: clocks.last_online_check, max_offline_days: 30 }
        );
    }

    #[test]
    fn check_in_is_due_only_after_max_offline_days_has_fully_elapsed() {
        let mut p = payload();
        p.expires_at = None;
        let at_limit = Clocks { now: ISSUED + 30 * DAY, high_water: ISSUED, last_online_check: ISSUED };
        let past_limit = Clocks { now: ISSUED + 30 * DAY + 1, ..at_limit };
        assert!(matches!(evaluate(&p, FP, at_limit), LicenseState::Ok { .. }));
        assert!(matches!(evaluate(&p, FP, past_limit), LicenseState::CheckInRequired { .. }));
    }

    // --- stored-row wrapper --------------------------------------------------------

    #[test]
    fn no_stored_row_is_missing() {
        assert_eq!(evaluate_stored(None, FP, ISSUED), LicenseState::Missing);
    }

    #[test]
    fn a_stored_row_that_fails_verification_is_invalid_not_a_panic() {
        let stored = StoredLicense {
            license_key: "K".into(),
            payload_b64: "bm90IGpzb24=".into(), // "not json"
            signature_b64: "AAAA".into(),
            kid: "x".into(),
            server_issued_at: 0,
            high_water_local_ts: 0,
            last_online_check: 0,
        };
        assert!(matches!(evaluate_stored(Some(&stored), FP, ISSUED), LicenseState::Invalid { .. }));
    }

    #[test]
    fn a_stored_row_signed_by_the_test_key_is_invalid_under_the_real_keyring() {
        // Belt and braces with verify.rs's own test: the stored path must
        // route through the production keyring, not a test one.
        let (payload_b64, signature_b64) = super::super::verify::tests::sign_json(&super::super::verify::tests::sample_json());
        let stored = StoredLicense {
            license_key: "K".into(),
            payload_b64,
            signature_b64,
            kid: "test".into(),
            server_issued_at: 0,
            high_water_local_ts: 0,
            last_online_check: 0,
        };
        assert!(matches!(evaluate_stored(Some(&stored), FP, ISSUED), LicenseState::Invalid { .. }));
    }

    // --- replacement ordering ------------------------------------------------------

    #[test]
    fn an_older_payload_never_replaces_a_newer_one() {
        assert!(accepts_replacement(None, 5));
        assert!(accepts_replacement(Some(5), 5));
        assert!(accepts_replacement(Some(5), 6));
        assert!(!accepts_replacement(Some(6), 5));
    }

    #[test]
    fn only_ok_allows_billing() {
        assert!(LicenseState::Ok { expires_at: None, expiring_soon: true }.allows_billing());
        for locked in [
            LicenseState::Missing,
            LicenseState::Expired { expired_at: None },
            LicenseState::Revoked,
            LicenseState::DeviceMismatch,
            LicenseState::ClockTampered { reason: ClockProblem::BehindServerTime },
            LicenseState::CheckInRequired { last_online_check: 0, max_offline_days: 30 },
            LicenseState::Invalid { message: String::new() },
        ] {
            assert!(!locked.allows_billing(), "{locked:?}");
        }
    }

    /// The frontend switches on `kind`; pin the wire shape.
    #[test]
    fn serialises_with_a_kind_tag_and_camel_case_fields() {
        let json = serde_json::to_string(&LicenseState::Ok { expires_at: Some(1), expiring_soon: true }).unwrap();
        assert_eq!(json, r#"{"kind":"ok","expiresAt":1,"expiringSoon":true}"#);
        let json = serde_json::to_string(&LicenseState::ClockTampered { reason: ClockProblem::BehindServerTime }).unwrap();
        assert_eq!(json, r#"{"kind":"clockTampered","reason":"behindServerTime"}"#);
    }
}
