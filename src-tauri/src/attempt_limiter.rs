//! Lockout after repeated failed credential checks — staff PIN login, the
//! shared sensitive-area PIN, and the product-owner password.
//!
//! Staff and area PINs are only 4–6 digits, so without a limit a script
//! calling the command over IPC (`withGlobalTauri` makes that a devtools
//! one-liner) could walk the whole space in minutes. After
//! [`FREE_ATTEMPTS`] consecutive failures on a key, further attempts on it
//! are refused outright — without even checking the credential — until a
//! lockout that doubles with every further failure has passed.
//!
//! It refuses rather than sleeps: these are synchronous commands, and a
//! sleep would freeze the UI for the honest user as well. The lockout is
//! memory-only, like every session in this app, so restarting the app
//! clears it; a restart costs seconds per round, which keeps a brute force
//! impractical without punishing a cashier who fat-fingers a PIN.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Consecutive failures allowed on a key before it is locked.
pub const FREE_ATTEMPTS: u32 = 5;
/// Lockout after the first failure past [`FREE_ATTEMPTS`]; doubles with
/// each further failure up to [`MAX_LOCKOUT`].
const BASE_LOCKOUT: Duration = Duration::from_secs(30);
const MAX_LOCKOUT: Duration = Duration::from_secs(15 * 60);

#[derive(Default)]
struct Entry {
    failures: u32,
    locked_until: Option<Instant>,
}

#[derive(Default)]
pub struct AttemptLimiter {
    entries: Mutex<HashMap<String, Entry>>,
}

impl AttemptLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs `check` unless `key` is locked out, recording its outcome.
    /// Any `Err` from `check` counts as a failed attempt.
    pub fn attempt<T>(&self, key: &str, check: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        self.attempt_at(key, Instant::now(), check)
    }

    fn attempt_at<T>(&self, key: &str, now: Instant, check: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        if let Some(wait) = self.remaining_lockout(key, now) {
            return Err(format!("Too many incorrect attempts. Try again in {}.", describe(wait)));
        }
        let result = check();
        let mut entries = self.entries.lock().expect("attempt limiter lock poisoned");
        match &result {
            Ok(_) => {
                entries.remove(key);
            }
            Err(_) => {
                let entry = entries.entry(key.to_string()).or_default();
                entry.failures = entry.failures.saturating_add(1);
                if entry.failures >= FREE_ATTEMPTS {
                    let doublings = (entry.failures - FREE_ATTEMPTS).min(16);
                    let lockout = BASE_LOCKOUT.saturating_mul(1 << doublings).min(MAX_LOCKOUT);
                    entry.locked_until = Some(now + lockout);
                }
            }
        }
        result
    }

    fn remaining_lockout(&self, key: &str, now: Instant) -> Option<Duration> {
        let entries = self.entries.lock().expect("attempt limiter lock poisoned");
        let until = entries.get(key)?.locked_until?;
        until.checked_duration_since(now).filter(|d| !d.is_zero())
    }
}

fn describe(wait: Duration) -> String {
    let secs = wait.as_secs().max(1);
    if secs < 60 {
        format!("{secs} seconds")
    } else {
        let mins = secs.div_ceil(60);
        if mins == 1 { "1 minute".into() } else { format!("{mins} minutes") }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fail() -> Result<(), String> {
        Err("wrong".into())
    }

    fn pass() -> Result<(), String> {
        Ok(())
    }

    #[test]
    fn allows_free_attempts_then_locks_without_running_the_check() {
        let limiter = AttemptLimiter::new();
        let t0 = Instant::now();
        for _ in 0..FREE_ATTEMPTS {
            assert_eq!(limiter.attempt_at("k", t0, fail).unwrap_err(), "wrong");
        }
        let mut ran = false;
        let err = limiter
            .attempt_at("k", t0, || {
                ran = true;
                pass()
            })
            .unwrap_err();
        assert!(!ran, "a locked key must not even reach the credential check");
        assert!(err.starts_with("Too many incorrect attempts"));
    }

    #[test]
    fn lockout_expires_and_then_doubles_on_the_next_failure() {
        let limiter = AttemptLimiter::new();
        let t0 = Instant::now();
        for _ in 0..FREE_ATTEMPTS {
            let _ = limiter.attempt_at("k", t0, fail);
        }
        assert!(limiter.attempt_at("k", t0 + BASE_LOCKOUT - Duration::from_secs(1), pass).is_err());

        let t1 = t0 + BASE_LOCKOUT;
        assert_eq!(limiter.attempt_at("k", t1, fail).unwrap_err(), "wrong");
        assert!(limiter.attempt_at("k", t1 + BASE_LOCKOUT, pass).is_err(), "second lockout is twice as long");
        assert!(limiter.attempt_at("k", t1 + BASE_LOCKOUT * 2, pass).is_ok());
    }

    #[test]
    fn success_resets_the_count() {
        let limiter = AttemptLimiter::new();
        let t0 = Instant::now();
        for _ in 0..FREE_ATTEMPTS - 1 {
            let _ = limiter.attempt_at("k", t0, fail);
        }
        assert!(limiter.attempt_at("k", t0, pass).is_ok());
        for _ in 0..FREE_ATTEMPTS - 1 {
            let _ = limiter.attempt_at("k", t0, fail);
        }
        assert!(limiter.attempt_at("k", t0, pass).is_ok(), "count restarted after the success");
    }

    #[test]
    fn keys_are_independent_and_lockout_is_capped() {
        let limiter = AttemptLimiter::new();
        let t0 = Instant::now();
        for _ in 0..100 {
            let _ = limiter.attempt_at("a", t0, fail);
        }
        assert!(limiter.attempt_at("b", t0, pass).is_ok());
        assert!(limiter.attempt_at("a", t0 + MAX_LOCKOUT, pass).is_ok());
    }
}
