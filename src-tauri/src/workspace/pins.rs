//! Staff PIN hashes leave a counter encrypted.
//!
//! Every counter of a business must check every staff PIN, so the argon2
//! hashes travel with the `users` rows (and the sensitive-area PIN with the
//! settings). They are sealed before they are sent and opened when they
//! arrive, with a key derived from the licence key: every counter of the
//! business has that key (it activated with it), while the server keeps only
//! a hash of it. A copy of the server's database or backups therefore holds
//! no PIN hash anyone can attack offline, and connecting a counter needs no
//! extra step.
//!
//! A sealed value is `enc1:<key id>:<base64 nonce + ciphertext>`. A value
//! sealed with another key (the licence key was replaced) cannot be opened
//! here: the local hash is kept, and this counter sends its own sealed copy
//! back so the business converges on the current key. A plain hash from an
//! older POS build is accepted as it is.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, CHACHA20_POLY1305, NONCE_LEN};
use ring::hkdf::{KeyType, Salt, HKDF_SHA256};
use ring::rand::{SecureRandom, SystemRandom};
use rusqlite::{Connection, OptionalExtension};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

const PREFIX: &str = "enc1:";
const SALT: &[u8] = b"diwan-pin-protect-v1";

/// The columns holding a PIN hash, per shared table.
pub fn protected_columns(table: &str) -> &'static [&'static str] {
    match table {
        "users" => &["pin_hash"],
        "app_config" => &["sensitive_area_pin_hash"],
        _ => &[],
    }
}

struct Bytes32;
impl KeyType for Bytes32 {
    fn len(&self) -> usize {
        32
    }
}

/// The key and its id, derived from the licence key.
pub struct PinKey {
    key: LessSafeKey,
    id: String,
}

impl PinKey {
    pub fn from_license_key(license_key: &str) -> Self {
        let normalized = license_key.trim().to_ascii_uppercase();
        let prk = Salt::new(HKDF_SHA256, SALT).extract(normalized.as_bytes());
        let mut bytes = [0u8; 32];
        prk.expand(&[b"staff-pin-hashes"], Bytes32).and_then(|okm| okm.fill(&mut bytes)).expect("32 bytes is a valid HKDF length");
        let id = Sha256::digest(bytes).iter().take(4).map(|b| format!("{b:02x}")).collect();
        let key = LessSafeKey::new(UnboundKey::new(&CHACHA20_POLY1305, &bytes).expect("32-byte key"));
        Self { key, id }
    }

    /// The key of the licence activated on this device, if any.
    pub fn load(conn: &Connection) -> Result<Option<Self>, rusqlite::Error> {
        let key: Option<String> =
            conn.query_row("SELECT license_key FROM license_status WHERE id = 1", [], |row| row.get(0)).optional()?;
        Ok(key.filter(|k| !k.trim().is_empty()).map(|k| Self::from_license_key(&k)))
    }

    #[cfg(test)]
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn seal(&self, plain: &str) -> String {
        let mut nonce = [0u8; NONCE_LEN];
        SystemRandom::new().fill(&mut nonce).expect("the OS random source");
        let mut sealed = plain.as_bytes().to_vec();
        self.key
            .seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::from(self.id.as_bytes()), &mut sealed)
            .expect("sealing cannot fail for a short value");
        let mut out = nonce.to_vec();
        out.extend(sealed);
        format!("{PREFIX}{}:{}", self.id, BASE64.encode(out))
    }

    /// `None` when the value was sealed with another key or is damaged.
    pub fn open(&self, sealed: &str) -> Option<String> {
        let (id, body) = sealed.strip_prefix(PREFIX)?.split_once(':')?;
        if id != self.id {
            return None;
        }
        let bytes = BASE64.decode(body).ok()?;
        if bytes.len() < NONCE_LEN {
            return None;
        }
        let (nonce, sealed) = bytes.split_at(NONCE_LEN);
        let mut buffer = sealed.to_vec();
        let nonce = Nonce::try_assume_unique_for_key(nonce).ok()?;
        let plain = self.key.open_in_place(nonce, Aad::from(id.as_bytes()), &mut buffer).ok()?;
        String::from_utf8(plain.to_vec()).ok()
    }
}

pub fn is_sealed(value: &str) -> bool {
    value.starts_with(PREFIX)
}

/// Seals the PIN hashes of a row about to be sent. Without a licence (which
/// a connected counter always has) they are left as they are.
pub fn seal_row(conn: &Connection, table: &str, data: &mut Map<String, Value>) -> Result<(), rusqlite::Error> {
    let columns = protected_columns(table);
    if columns.is_empty() {
        return Ok(());
    }
    let Some(key) = PinKey::load(conn)? else { return Ok(()) };
    for column in columns {
        if let Some(Value::String(plain)) = data.get(*column) {
            if !is_sealed(plain) {
                let sealed = key.seal(plain);
                data.insert((*column).to_string(), Value::String(sealed));
            }
        }
    }
    Ok(())
}

/// Opens the PIN hashes of a downloaded row in place. Returns the columns
/// that could not be opened here (removed from `data`, so the local value
/// stays).
pub fn open_row(conn: &Connection, table: &str, data: &mut Map<String, Value>) -> Result<Vec<&'static str>, rusqlite::Error> {
    let columns = protected_columns(table);
    let sealed: Vec<&'static str> =
        columns.iter().copied().filter(|c| matches!(data.get(*c), Some(Value::String(v)) if is_sealed(v))).collect();
    if sealed.is_empty() {
        return Ok(Vec::new());
    }
    let key = PinKey::load(conn)?;
    let mut unreadable = Vec::new();
    for column in sealed {
        let opened = match (&key, data.get(column)) {
            (Some(key), Some(Value::String(value))) => key.open(value),
            _ => None,
        };
        match opened {
            Some(plain) => {
                data.insert(column.to_string(), Value::String(plain));
            }
            None => {
                data.remove(column);
                unreadable.push(column);
            }
        }
    }
    Ok(unreadable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sealed_hash_opens_with_the_same_licence_key_only() {
        let key = PinKey::from_license_key("DIWAN-ABCD-1234");
        let same = PinKey::from_license_key("  diwan-abcd-1234 ");
        let other = PinKey::from_license_key("DIWAN-ZZZZ-9999");
        let hash = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA";

        let sealed = key.seal(hash);
        assert!(is_sealed(&sealed) && !sealed.contains("argon2"), "nothing of the hash shows: {sealed}");
        assert_ne!(sealed, key.seal(hash), "a fresh nonce every time");
        assert_eq!(same.open(&sealed).as_deref(), Some(hash), "the key is the licence key, however it was typed");
        assert_eq!(other.open(&sealed), None);
        assert_eq!(key.open(&sealed.replace(key.id(), other.id())), None, "the key id is authenticated");
        let (head, body) = sealed.rsplit_once(':').unwrap();
        let mut bytes = BASE64.decode(body).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        assert_eq!(key.open(&format!("{head}:{}", BASE64.encode(bytes))), None, "a changed byte fails authentication");
        assert_ne!(key.id(), other.id());
    }

    #[test]
    fn rows_are_sealed_on_the_way_out_and_opened_on_the_way_in() {
        let conn = crate::db::schema::test_conn();
        conn.execute(
            "INSERT INTO license_status (id, license_key, payload_b64, signature_b64, kid, server_issued_at, high_water_local_ts, last_online_check)
             VALUES (1, 'DIWAN-TEST-KEY', '', '', 'k', 0, 0, 0)",
            [],
        )
        .unwrap();
        let mut data = Map::new();
        data.insert("name".into(), Value::String("Sana".into()));
        data.insert("pin_hash".into(), Value::String("$argon2id$plain".into()));
        seal_row(&conn, "users", &mut data).unwrap();
        let sealed = data["pin_hash"].as_str().unwrap().to_string();
        assert!(is_sealed(&sealed));
        assert_eq!(data["name"], "Sana", "other columns untouched");
        seal_row(&conn, "users", &mut data).unwrap();
        assert_eq!(data["pin_hash"].as_str().unwrap(), sealed, "never sealed twice");

        assert!(open_row(&conn, "users", &mut data).unwrap().is_empty());
        assert_eq!(data["pin_hash"], "$argon2id$plain");

        let mut foreign = Map::new();
        foreign.insert("pin_hash".into(), Value::String(PinKey::from_license_key("DIWAN-OTHER").seal("$argon2id$x")));
        assert_eq!(open_row(&conn, "users", &mut foreign).unwrap(), ["pin_hash"]);
        assert!(!foreign.contains_key("pin_hash"), "an unreadable hash is dropped, so the local one stays");

        let mut legacy = Map::new();
        legacy.insert("pin_hash".into(), Value::String("$argon2id$from-an-older-build".into()));
        assert!(open_row(&conn, "users", &mut legacy).unwrap().is_empty(), "a plain hash is accepted");
        assert_eq!(legacy["pin_hash"], "$argon2id$from-an-older-build");
    }
}
