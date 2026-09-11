//! The process-wide licence state: computed once from the database before
//! the window exists, refreshed from the server in the background, and
//! read by every command that has to decide whether the till may run.
//!
//! Launch is never blocked on the network. `load` is synchronous and
//! offline (verify the cached row, evaluate it); `refresh` is async and
//! failure-tolerant (offline just means "keep what we have"). Every change
//! is pushed to the frontend as a `license:changed` event so an open
//! billing screen learns about a renewal — or a revocation — without a
//! restart.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use super::client::{self, ClientError};
use super::fingerprint::device_fingerprint;
use super::state::{accepts_replacement, evaluate, evaluate_stored, Clocks, LicenseState};
use super::verify::verify_signed;
use crate::db::{self, Db};

/// Event name the frontend subscribes to. Payload: the new `LicenseState`.
pub const CHANGED_EVENT: &str = "license:changed";

/// How often a running app re-checks with the server. A till can stay open
/// for days; this is how a renewal (or a revocation) reaches it without a
/// restart, and how `last_online_check` keeps advancing so the max-offline
/// bound never trips on a machine that is actually online.
const BACKGROUND_REFRESH_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
/// Delay before the first background refresh after launch, so it never
/// competes with the splash/first paint for the network or the DB lock.
const FIRST_REFRESH_DELAY: Duration = Duration::from_secs(3);

pub struct LicenseRuntime {
    state: Mutex<LicenseState>,
    /// Computed once; `Err` means this machine couldn't be fingerprinted at
    /// all, in which case nothing can ever verify (`DeviceMismatch` against
    /// an empty string) and activation refuses with the real reason.
    fingerprint: Result<String, String>,
}

#[derive(Debug)]
pub enum LicenseError {
    /// Nobody can fingerprint this machine — surfaced verbatim.
    NoFingerprint(String),
    /// The server refused (wrong key, bound elsewhere, rate limited…).
    Refused(String),
    /// Couldn't reach the server or it misbehaved. For `activate` this is
    /// fatal (there is nothing to fall back to); for `refresh` the caller
    /// keeps the cached state and reports this only as a hint.
    Unreachable(String),
    /// The server's response failed signature/shape verification, or was
    /// signed for a different device. Should never happen against a
    /// correctly configured server; when it does the message says why.
    Untrusted(String),
    Db(String),
}

impl std::fmt::Display for LicenseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LicenseError::NoFingerprint(m) => write!(f, "{m}"),
            LicenseError::Refused(m) => write!(f, "{m}"),
            LicenseError::Unreachable(m) => write!(f, "Could not reach the licence server — {m}"),
            LicenseError::Untrusted(m) => write!(f, "Licence response could not be verified: {m}"),
            LicenseError::Db(m) => write!(f, "{m}"),
        }
    }
}

fn now_unix() -> i64 {
    chrono::Utc::now().timestamp()
}

impl LicenseRuntime {
    /// Offline evaluation of whatever is cached. Also advances the
    /// high-water mark — this launch is now the latest time we've seen.
    pub fn load(db: &Db, app_data_dir: PathBuf) -> Self {
        let fingerprint = device_fingerprint(&app_data_dir).map_err(|e| e.to_string());
        let now = now_unix();
        let state = match &fingerprint {
            Ok(fp) => {
                let fp = fp.clone();
                db.with_conn(|conn| {
                    db::license::touch_high_water(conn, now)?;
                    let stored = db::license::get(conn)?;
                    Ok(evaluate_stored(stored.as_ref(), &fp, now))
                })
                .unwrap_or_else(|e| LicenseState::Invalid { message: e.to_string() })
            }
            Err(e) => LicenseState::Invalid { message: e.clone() },
        };
        crate::startup_log::log(&format!("license: loaded as {}", kind_of(&state)));
        Self { state: Mutex::new(state), fingerprint }
    }

    pub fn current(&self) -> LicenseState {
        self.state.lock().expect("license state lock poisoned").clone()
    }

    fn set(&self, app: &AppHandle, next: LicenseState) -> LicenseState {
        {
            let mut state = self.state.lock().expect("license state lock poisoned");
            if *state != next {
                crate::startup_log::log(&format!("license: {} -> {}", kind_of(&state), kind_of(&next)));
            }
            *state = next.clone();
        }
        // A failed emit (no window yet) is harmless — the frontend reads
        // the state fresh via `license_get_state` when it mounts.
        let _ = app.emit(CHANGED_EVENT, &next);
        next
    }

    fn fingerprint(&self) -> Result<&str, LicenseError> {
        self.fingerprint.as_deref().map_err(|e| LicenseError::NoFingerprint(e.clone()))
    }

    /// Re-evaluates the cached row against the current clock — what the
    /// periodic tick does between server refreshes so a licence that
    /// expires while the app is open flips to `Expired` on time.
    pub fn reevaluate(&self, app: &AppHandle, db: &Db) -> LicenseState {
        let now = now_unix();
        let next = match self.fingerprint() {
            Ok(fp) => db
                .with_conn(|conn| {
                    db::license::touch_high_water(conn, now)?;
                    Ok(evaluate_stored(db::license::get(conn)?.as_ref(), fp, now))
                })
                .unwrap_or_else(|e| LicenseState::Invalid { message: e.to_string() }),
            Err(e) => LicenseState::Invalid { message: e.to_string() },
        };
        self.set(app, next)
    }

    /// Fetches a fresh signed status for the stored key, verifies it, and
    /// — if it is at least as new as what's cached — stores and adopts it.
    ///
    /// Offline / server trouble → `Err(Unreachable)`, state untouched.
    /// A 409 ("not activated") means support reset the binding for a
    /// device move: re-activate on this device and fetch again. A 403
    /// means the server no longer recognises this key for this device
    /// (moved elsewhere, or the key was deleted): the cache is cleared so
    /// the install locks and asks for a key, rather than coasting on a
    /// payload the server has disowned.
    pub async fn refresh(&self, app: &AppHandle, db: &Db) -> Result<LicenseState, LicenseError> {
        let fp = self.fingerprint()?.to_string();
        let stored = db.with_conn(db::license::get).map_err(|e| LicenseError::Db(e.to_string()))?;
        let Some(stored) = stored else {
            return Ok(self.set(app, LicenseState::Missing));
        };

        let signed = match client::fetch_status(&stored.license_key, &fp).await {
            Ok(signed) => signed,
            Err(ClientError::Refused { status: 409, .. }) => {
                client::activate(&stored.license_key, &fp).await.map_err(map_client_error)?;
                client::fetch_status(&stored.license_key, &fp).await.map_err(map_client_error)?
            }
            Err(ClientError::Refused { status: 403, message }) => {
                db.with_conn(db::license::clear).map_err(|e| LicenseError::Db(e.to_string()))?;
                self.set(app, LicenseState::Missing);
                return Err(LicenseError::Refused(message));
            }
            Err(e) => return Err(map_client_error(e)),
        };

        self.adopt(app, db, &stored.license_key, &fp, signed, Some(stored.server_issued_at))
    }

    /// First-run and settings-screen path: bind `license_key` to this
    /// device on the server, fetch its signed status, store and adopt it.
    /// Requires the network; there is no offline fallback for activation.
    pub async fn activate(&self, app: &AppHandle, db: &Db, license_key: &str) -> Result<LicenseState, LicenseError> {
        let fp = self.fingerprint()?.to_string();
        let key = license_key.trim().to_uppercase();
        if key.is_empty() {
            return Err(LicenseError::Refused("Please enter a licence key".into()));
        }
        client::activate(&key, &fp).await.map_err(map_client_error)?;
        let signed = client::fetch_status(&key, &fp).await.map_err(map_client_error)?;
        // A different key replaces whatever was cached regardless of age;
        // the same key still obeys the never-go-backwards rule.
        let stored_issued_at = db
            .with_conn(db::license::get)
            .map_err(|e| LicenseError::Db(e.to_string()))?
            .filter(|s| s.license_key == key)
            .map(|s| s.server_issued_at);
        self.adopt(app, db, &key, &fp, signed, stored_issued_at)
    }

    /// Verify → device check → ordering check → store → evaluate → publish.
    fn adopt(
        &self,
        app: &AppHandle,
        db: &Db,
        license_key: &str,
        fp: &str,
        signed: client::SignedStatus,
        stored_issued_at: Option<i64>,
    ) -> Result<LicenseState, LicenseError> {
        let payload = verify_signed(&signed.payload, &signed.signature).map_err(|e| LicenseError::Untrusted(e.to_string()))?;
        if payload.device_fingerprint != fp {
            return Err(LicenseError::Untrusted("issued for a different device".into()));
        }
        if !payload.license_key.eq_ignore_ascii_case(license_key) {
            return Err(LicenseError::Untrusted("issued for a different licence key".into()));
        }
        if !accepts_replacement(stored_issued_at, payload.issued_at) {
            // Older than what we have (a stale response). Keep ours; report
            // the current state rather than an error — nothing is wrong.
            return Ok(self.reevaluate(app, db));
        }

        let now = now_unix();
        db.with_conn(|conn| {
            db::license::save_signed(
                conn,
                &payload.license_key,
                &signed.payload,
                &signed.signature,
                &signed.kid,
                payload.issued_at,
                now,
            )
        })
        .map_err(|e| LicenseError::Db(e.to_string()))?;

        let next = evaluate(
            &payload,
            fp,
            Clocks { now, high_water: now.max(payload.issued_at), last_online_check: now },
        );
        Ok(self.set(app, next))
    }
}

fn map_client_error(e: ClientError) -> LicenseError {
    match e {
        ClientError::Refused { message, .. } => LicenseError::Refused(message),
        ClientError::Offline(m) | ClientError::Server(m) => LicenseError::Unreachable(m),
    }
}

/// Short label for the startup log — never the message text, which for
/// `Invalid` could in principle echo attacker-controlled bytes.
fn kind_of(state: &LicenseState) -> &'static str {
    match state {
        LicenseState::Missing => "missing",
        LicenseState::Ok { expiring_soon: true, .. } => "ok (expiring soon)",
        LicenseState::Ok { .. } => "ok",
        LicenseState::Expired { .. } => "expired",
        LicenseState::Revoked => "revoked",
        LicenseState::DeviceMismatch => "device mismatch",
        LicenseState::ClockTampered { .. } => "clock tampered",
        LicenseState::CheckInRequired { .. } => "check-in required",
        LicenseState::Invalid { .. } => "invalid",
    }
}

/// Background loop: one refresh shortly after launch, then every
/// `BACKGROUND_REFRESH_INTERVAL`; between refreshes, a re-evaluation every
/// minute so an expiry that falls while the app is open takes effect on
/// time and the warning banner's "expiring soon" flips over without a
/// restart. Never panics, never blocks anything else — every failure is
/// logged and waited out.
pub fn spawn_background_refresh(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_REFRESH_DELAY).await;
        let mut since_refresh = BACKGROUND_REFRESH_INTERVAL; // force one now
        loop {
            let runtime = app.state::<LicenseRuntime>();
            let db = app.state::<Db>();
            if since_refresh >= BACKGROUND_REFRESH_INTERVAL {
                since_refresh = Duration::ZERO;
                match runtime.refresh(&app, &db).await {
                    Ok(_) => crate::startup_log::log("license: background refresh ok"),
                    Err(e) => crate::startup_log::log(&format!("license: background refresh skipped: {e}")),
                }
            } else {
                runtime.reevaluate(&app, &db);
            }
            let tick = Duration::from_secs(60);
            since_refresh += tick;
            tokio::time::sleep(tick).await;
        }
    });
}


