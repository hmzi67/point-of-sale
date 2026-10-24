//! Where the counter's sync token is kept.
//!
//! The token is the counter's only credential with the business server, so
//! it goes in the operating system's credential store where the app has one:
//! the Windows Credential Manager, and the Keychain on iOS and on a release
//! build for macOS. `sync_state.token` then holds only the marker
//! [`IN_VAULT`], so a copy of the database file (a backup, a support upload)
//! carries no credential.
//!
//! Elsewhere it stays in `sync_state.token`: Linux (the Secret Service needs
//! a desktop session and D-Bus), Android, and debug builds on macOS, whose
//! unsigned binary changes with every build and would make the Keychain ask
//! for permission each time. If the store refuses a write, the token is kept
//! in the database rather than lost.

use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};

/// `sync_state.token` when the token is in the credential store.
pub const IN_VAULT: &str = "@os-credential-store";
const SERVICE: &str = "com.diwan.pos.workspace";
const ACCOUNT: &str = "counter-token";

/// The token as last read from or written to the store: one read per launch.
static CACHE: Mutex<Option<String>> = Mutex::new(None);

fn cached() -> Option<String> {
    CACHE.lock().ok().and_then(|c| c.clone())
}

fn remember(token: Option<&str>) {
    if let Ok(mut cache) = CACHE.lock() {
        *cache = token.map(str::to_string);
    }
}

/// The counter's token, wherever it is kept. `None` when not connected, or
/// when the store lost it (the counter then has to connect again).
pub fn read(conn: &Connection) -> Result<Option<String>, rusqlite::Error> {
    let stored: Option<String> =
        conn.query_row("SELECT token FROM sync_state WHERE id = 1", [], |row| row.get(0)).optional()?.flatten();
    match stored.as_deref() {
        Some(IN_VAULT) => Ok(cached().or_else(|| {
            let token = os::get(SERVICE, ACCOUNT);
            remember(token.as_deref());
            token
        })),
        _ => Ok(stored),
    }
}

/// Stores the token (or forgets it, with `None`).
pub fn write(conn: &Connection, token: Option<&str>) -> Result<(), rusqlite::Error> {
    let Some(token) = token else {
        os::delete(SERVICE, ACCOUNT);
        remember(None);
        conn.execute("UPDATE sync_state SET token = NULL WHERE id = 1", [])?;
        return Ok(());
    };
    let kept = match os::set(SERVICE, ACCOUNT, token) {
        Ok(()) => {
            remember(Some(token));
            IN_VAULT
        }
        Err(e) => {
            if os::AVAILABLE {
                crate::startup_log::warn(&format!("workspace: credential store refused the token, keeping it in the database ({e})"));
            }
            token
        }
    };
    conn.execute("UPDATE sync_state SET token = ?1 WHERE id = 1", params![kept])?;
    Ok(())
}

/// Moves a token an older build left in the database into the store.
pub fn migrate(conn: &Connection) -> Result<(), rusqlite::Error> {
    if !os::AVAILABLE {
        return Ok(());
    }
    let stored: Option<String> =
        conn.query_row("SELECT token FROM sync_state WHERE id = 1", [], |row| row.get(0)).optional()?.flatten();
    match stored {
        Some(token) if token != IN_VAULT => write(conn, Some(&token)),
        _ => Ok(()),
    }
}

#[cfg(any(target_os = "ios", all(target_os = "macos", not(debug_assertions))))]
mod os {
    use security_framework::passwords::{delete_generic_password, get_generic_password, set_generic_password};

    pub const AVAILABLE: bool = true;

    pub fn get(service: &str, account: &str) -> Option<String> {
        get_generic_password(service, account).ok().and_then(|bytes| String::from_utf8(bytes).ok())
    }

    pub fn set(service: &str, account: &str, secret: &str) -> Result<(), String> {
        set_generic_password(service, account, secret.as_bytes()).map_err(|e| e.to_string())
    }

    pub fn delete(service: &str, account: &str) {
        let _ = delete_generic_password(service, account);
    }
}

#[cfg(target_os = "windows")]
mod os {
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_FLAGS, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    pub const AVAILABLE: bool = true;

    fn target(service: &str, account: &str) -> Vec<u16> {
        format!("{service}/{account}").encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn get(service: &str, account: &str) -> Option<String> {
        let name = target(service, account);
        let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: `name` is NUL-terminated and outlives the call; on success
        // `credential` points at a block the OS allocated, read once and
        // released with CredFree below.
        unsafe {
            CredReadW(PCWSTR(name.as_ptr()), CRED_TYPE_GENERIC, None, &mut credential).ok()?;
            let blob = std::slice::from_raw_parts((*credential).CredentialBlob, (*credential).CredentialBlobSize as usize);
            let secret = String::from_utf8(blob.to_vec()).ok();
            CredFree(credential as *const _);
            secret
        }
    }

    pub fn set(service: &str, account: &str, secret: &str) -> Result<(), String> {
        let mut name = target(service, account);
        let mut user: Vec<u16> = account.encode_utf16().chain(std::iter::once(0)).collect();
        let mut blob = secret.as_bytes().to_vec();
        let credential = CREDENTIALW {
            Flags: CRED_FLAGS(0),
            Type: CRED_TYPE_GENERIC,
            TargetName: PWSTR(name.as_mut_ptr()),
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            UserName: PWSTR(user.as_mut_ptr()),
            ..Default::default()
        };
        // SAFETY: every pointer in `credential` borrows a local buffer that
        // lives until after the call; CredWriteW copies what it keeps.
        unsafe { CredWriteW(&credential, 0) }.map_err(|e| e.to_string())
    }

    pub fn delete(service: &str, account: &str) {
        let name = target(service, account);
        // SAFETY: `name` is NUL-terminated and outlives the call.
        let _ = unsafe { CredDeleteW(PCWSTR(name.as_ptr()), CRED_TYPE_GENERIC, None) };
    }
}

#[cfg(not(any(target_os = "ios", target_os = "windows", all(target_os = "macos", not(debug_assertions)))))]
mod os {
    pub const AVAILABLE: bool = false;

    pub fn get(_service: &str, _account: &str) -> Option<String> {
        None
    }

    pub fn set(_service: &str, _account: &str, _secret: &str) -> Result<(), String> {
        Err("no credential store on this platform".into())
    }

    pub fn delete(_service: &str, _account: &str) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_credential_store_the_token_stays_in_the_database() {
        if os::AVAILABLE {
            return; // covered by `the_credential_store_round_trip`
        }
        let conn = crate::db::schema::test_conn();
        write(&conn, Some("dwt_secret")).unwrap();
        assert_eq!(read(&conn).unwrap().as_deref(), Some("dwt_secret"));
        migrate(&conn).unwrap();
        assert_eq!(read(&conn).unwrap().as_deref(), Some("dwt_secret"));
        write(&conn, None).unwrap();
        assert_eq!(read(&conn).unwrap(), None);
    }

    /// Writes to the real store of the machine running it (and removes the
    /// entry again): run by hand on Windows, iOS or a macOS release build.
    #[test]
    #[ignore]
    fn the_credential_store_round_trip() {
        let conn = crate::db::schema::test_conn();
        conn.execute("UPDATE sync_state SET token = 'dwt_old_plain' WHERE id = 1", []).unwrap();
        migrate(&conn).unwrap();
        let stored: String = conn.query_row("SELECT token FROM sync_state", [], |r| r.get(0)).unwrap();
        assert_eq!(stored, IN_VAULT, "the database no longer holds the token");
        remember(None);
        assert_eq!(read(&conn).unwrap().as_deref(), Some("dwt_old_plain"), "read back from the store");
        write(&conn, None).unwrap();
        remember(None);
        assert_eq!(os::get(SERVICE, ACCOUNT), None);
    }
}
