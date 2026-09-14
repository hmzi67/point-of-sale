//! Offline-capable, tamper-resistant licence validation.
//!
//! The server (`api/license-status.php`) issues a small JSON payload —
//! licence key, status, issue time, expiry, device fingerprint, offline
//! policy — signed with an Ed25519 private key that exists only in the
//! server's `.env`. The app stores that payload as-is and re-verifies the
//! signature against a compiled-in public key on every launch, so a shop
//! that activated once and then ran offline for weeks still knows, without
//! phoning home, whether its licence has lapsed. Editing the stored expiry
//! breaks the signature; extracting the public key from the binary yields
//! nothing that can produce a new one.
//!
//! Why asymmetric and not an HMAC: an HMAC's verifying key *is* its signing
//! key, so an offline verifier would have to carry the secret, and anyone
//! who pulled it out of the binary could mint an "active until 2099"
//! payload. With Ed25519 the binary carries only the half that can check.
//!
//! Layout (built up over several stages):
//! - [`verify`] — pure signature/shape checking, no I/O. Fully unit-tested
//!   with an in-test keypair so the tests never depend on the real key.
//! - [`state`] — turns a verified payload plus the clocks into the single
//!   `LicenseState` the rest of the app acts on (warning window, expiry,
//!   clock-rollback detection, max-offline check-in).
//! - [`fingerprint`] — the stable, hashed device id the licence binds to.
//! - [`client`] — the HTTP calls to activate-license.php / license-status.php.
//! - [`runtime`] — the managed process-wide state, launch load, online
//!   refresh/activate, and the background refresh loop.
//! - `db::license` — the single-row SQLite cache of the signed bytes.
//!   The table itself is documented in `db/schema.sql`.
//!
//! Enforcement lives in Rust, mirroring `vendor_gate`: the frontend only
//! renders the state it is told; the money-moving commands refuse to run
//! while the licence is not OK, so bypassing the UI lock changes nothing.

pub mod client;
pub mod fingerprint;
pub mod runtime;
pub mod state;
pub mod verify;
