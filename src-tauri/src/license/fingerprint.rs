//! The device fingerprint a licence is bound to.
//!
//! What the server stores and compares is `sha256(fingerprint)`; what the
//! app sends is `fingerprint = hex(sha256(machine id))`. The raw machine id
//! never leaves the device (it is a real identifier — a hardware UUID on
//! macOS, `/etc/machine-id` on Linux, `MachineGuid` on Windows — and the
//! server has no need for it). Hashing here means a database leak on the
//! server, or a payload copied off this machine, reveals nothing about
//! the hardware.
//!
//! The payload echoes this value back (`device_fingerprint`) and
//! `license::state::evaluate` compares it, so the fingerprint has to be
//! *stable* across launches, reboots and reinstalls of the app — that is
//! why it derives from the OS machine id rather than anything the app
//! generates. Where no OS machine id exists (Android), a random id is
//! generated once and persisted in the app data directory; on that
//! platform, wiping app data = a new device, which support resolves the
//! same way as any hardware move. iOS has no machine id either, but it has
//! the Keychain, whose items outlive an uninstall: the random id lives
//! there, so deleting and reinstalling the app is still the same device.
//! (A genuinely new iPhone is a new device; an App Store licence moves to
//! it through "Restore purchases" — see `iap.rs`.)

use std::path::Path;

use sha2::{Digest, Sha256};

/// Filename of the mobile fallback id, under `app_data_dir()`.
#[cfg_attr(not(any(target_os = "android", target_os = "ios")), allow(dead_code))]
const INSTALL_ID_FILE: &str = "device-id";

/// Keychain service/account holding the iOS device id. Renaming either
/// would silently re-fingerprint every iOS install.
#[cfg(target_os = "ios")]
const KEYCHAIN_SERVICE: &str = "com.diwan.pos.license";
#[cfg(target_os = "ios")]
const KEYCHAIN_ACCOUNT: &str = "device-id";

#[derive(Debug)]
pub enum FingerprintError {
    Unavailable(String),
}

impl std::fmt::Display for FingerprintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FingerprintError::Unavailable(msg) => write!(f, "Could not identify this device: {msg}"),
        }
    }
}

/// `hex(sha256(machine id))` — 64 lowercase hex chars, exactly what
/// `activate-license.php` / `license-status.php` validate against.
pub fn device_fingerprint(app_data_dir: &Path) -> Result<String, FingerprintError> {
    let machine_id = machine_id(app_data_dir)?;
    Ok(hex_sha256(machine_id.trim().as_bytes()))
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn machine_id(_app_data_dir: &Path) -> Result<String, FingerprintError> {
    machine_uid::get().map_err(|e| FingerprintError::Unavailable(e.to_string()))
}

/// Android: no OS-level machine id is reachable without JNI into
/// `Settings.Secure.ANDROID_ID`, so use a per-install random id instead.
#[cfg(target_os = "android")]
fn machine_id(app_data_dir: &Path) -> Result<String, FingerprintError> {
    install_id_file(app_data_dir)
}

/// iOS: a random id kept in the Keychain (see the module doc for why not a
/// file). If the Keychain is unusable the install-id file is the fallback —
/// still stable for this install, just not across a reinstall.
#[cfg(target_os = "ios")]
fn machine_id(app_data_dir: &Path) -> Result<String, FingerprintError> {
    use security_framework::passwords::{get_generic_password, set_generic_password};

    if let Ok(bytes) = get_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT) {
        if let Ok(existing) = String::from_utf8(bytes) {
            if existing.trim().len() >= 32 {
                return Ok(existing);
            }
        }
    }
    // Carry an id this install already has over into the Keychain, so a
    // licence activated before the Keychain was reachable stays bound.
    let id = install_id_file(app_data_dir)?;
    if let Err(e) = set_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT, id.trim().as_bytes()) {
        crate::startup_log::warn(&format!(
            "fingerprint: Keychain unavailable, using the install id file ({})",
            crate::startup_log::clip(&e.to_string())
        ));
    }
    Ok(id)
}

/// Reads — or generates once and persists — a random per-install id.
/// Persisted as a file (not in SQLite) so it is independent of the
/// database's lifecycle — same reasoning as `images.rs`.
#[cfg(any(target_os = "android", target_os = "ios"))]
fn install_id_file(app_data_dir: &Path) -> Result<String, FingerprintError> {
    let path = app_data_dir.join(INSTALL_ID_FILE);
    if let Ok(existing) = std::fs::read_to_string(&path) {
        if existing.trim().len() >= 32 {
            return Ok(existing);
        }
    }
    // 32 random bytes from the OS; no extra crate needed.
    let mut raw = [0u8; 32];
    getrandom_fill(&mut raw).map_err(FingerprintError::Unavailable)?;
    let id = raw.iter().map(|b| format!("{b:02x}")).collect::<String>();
    std::fs::create_dir_all(app_data_dir).map_err(|e| FingerprintError::Unavailable(e.to_string()))?;
    std::fs::write(&path, &id).map_err(|e| FingerprintError::Unavailable(e.to_string()))?;
    Ok(id)
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn getrandom_fill(buf: &mut [u8]) -> Result<(), String> {
    use std::io::Read;
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(buf))
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fingerprint_is_64_lowercase_hex_chars_and_stable() {
        let dir = std::env::temp_dir();
        let a = device_fingerprint(&dir).expect("this CI/dev machine must expose a machine id");
        let b = device_fingerprint(&dir).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    /// Pins the exact transform the server relies on.
    #[test]
    fn hex_sha256_matches_a_known_vector() {
        assert_eq!(hex_sha256(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn install_id_file_name_is_stable() {
        // Renaming this would silently re-fingerprint every Android install.
        assert_eq!(INSTALL_ID_FILE, "device-id");
    }
}
