//! The single-row `license_status` cache. Stores the server's signed bytes
//! untouched plus the three clock bookkeeping columns — see `schema.sql`'s
//! comment on the table for what each is for, and `license::state` for how
//! they are read. Nothing here verifies anything; that is `license::verify`.

use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredLicense {
    pub license_key: String,
    pub payload_b64: String,
    pub signature_b64: String,
    pub kid: String,
    pub server_issued_at: i64,
    pub high_water_local_ts: i64,
    pub last_online_check: i64,
}

pub fn get(conn: &Connection) -> Result<Option<StoredLicense>, rusqlite::Error> {
    conn.query_row(
        "SELECT license_key, payload_b64, signature_b64, kid, server_issued_at,
                high_water_local_ts, last_online_check
           FROM license_status WHERE id = 1",
        [],
        |row| {
            Ok(StoredLicense {
                license_key: row.get(0)?,
                payload_b64: row.get(1)?,
                signature_b64: row.get(2)?,
                kid: row.get(3)?,
                server_issued_at: row.get(4)?,
                high_water_local_ts: row.get(5)?,
                last_online_check: row.get(6)?,
            })
        },
    )
    .optional()
}

/// Replaces the cached payload after a successful, *already verified*
/// fetch. `now_local` is the local clock at the moment of the fetch: it
/// becomes `last_online_check`, and the high-water mark is reset to
/// `max(now_local, server_issued_at)` — the server's clock is authoritative
/// and just told us what time it is, so any earlier local reading (an
/// over-set clock that was since corrected) is forgiven here and only here.
///
/// The caller is responsible for the "never replace a newer payload with an
/// older one" rule (`license::state::accepts_replacement`); this function
/// writes whatever it is given.
pub fn save_signed(
    conn: &Connection,
    license_key: &str,
    payload_b64: &str,
    signature_b64: &str,
    kid: &str,
    server_issued_at: i64,
    now_local: i64,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO license_status
            (id, license_key, payload_b64, signature_b64, kid, server_issued_at,
             high_water_local_ts, last_online_check, updated_at)
         VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now', 'localtime'))
         ON CONFLICT (id) DO UPDATE SET
            license_key         = excluded.license_key,
            payload_b64         = excluded.payload_b64,
            signature_b64       = excluded.signature_b64,
            kid                 = excluded.kid,
            server_issued_at    = excluded.server_issued_at,
            high_water_local_ts = excluded.high_water_local_ts,
            last_online_check   = excluded.last_online_check,
            updated_at          = excluded.updated_at",
        params![
            license_key,
            payload_b64,
            signature_b64,
            kid,
            server_issued_at,
            now_local.max(server_issued_at),
            now_local
        ],
    )?;
    Ok(())
}

/// Advances the high-water mark to `now_local` if that is later than what
/// is stored. Called on every launch and periodic tick; never moves it
/// backwards (only `save_signed` may, with the server's word for it).
/// A no-op when there is no row yet.
pub fn touch_high_water(conn: &Connection, now_local: i64) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE license_status
            SET high_water_local_ts = MAX(high_water_local_ts, ?1)
          WHERE id = 1",
        params![now_local],
    )?;
    Ok(())
}

/// Forgets the cached licence entirely — used when the user replaces the
/// key from the settings screen and the new one fails to activate, so the
/// install doesn't keep running on a key its owner just tried to retire.
pub fn clear(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute("DELETE FROM license_status WHERE id = 1", [])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::test_conn;

    #[test]
    fn a_fresh_database_has_no_licence() {
        assert_eq!(get(&test_conn()).unwrap(), None);
    }

    #[test]
    fn save_then_get_round_trips_and_a_second_save_replaces() {
        let conn = test_conn();
        save_signed(&conn, "DIWAN-A", "p1", "s1", "k", 1_000, 1_500).unwrap();
        let first = get(&conn).unwrap().unwrap();
        assert_eq!(first.license_key, "DIWAN-A");
        assert_eq!(first.payload_b64, "p1");
        assert_eq!(first.server_issued_at, 1_000);
        assert_eq!(first.last_online_check, 1_500);
        assert_eq!(first.high_water_local_ts, 1_500, "high water = max(now, issued)");

        save_signed(&conn, "DIWAN-B", "p2", "s2", "k", 2_000, 1_900).unwrap();
        let second = get(&conn).unwrap().unwrap();
        assert_eq!(second.license_key, "DIWAN-B");
        assert_eq!(second.payload_b64, "p2");
        assert_eq!(second.high_water_local_ts, 2_000, "server time ahead of local wins");
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM license_status", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
    }

    /// The server's clock is the one authority allowed to move the mark back.
    #[test]
    fn a_refresh_may_lower_the_high_water_mark_but_a_touch_never_does() {
        let conn = test_conn();
        save_signed(&conn, "K", "p", "s", "k", 1_000, 9_000).unwrap();
        touch_high_water(&conn, 5_000).unwrap();
        assert_eq!(get(&conn).unwrap().unwrap().high_water_local_ts, 9_000);
        touch_high_water(&conn, 9_500).unwrap();
        assert_eq!(get(&conn).unwrap().unwrap().high_water_local_ts, 9_500);
        save_signed(&conn, "K", "p", "s", "k", 6_000, 6_000).unwrap();
        assert_eq!(get(&conn).unwrap().unwrap().high_water_local_ts, 6_000);
    }

    #[test]
    fn touch_without_a_row_is_a_harmless_no_op() {
        let conn = test_conn();
        touch_high_water(&conn, 1).unwrap();
        assert_eq!(get(&conn).unwrap(), None);
    }

    #[test]
    fn clear_removes_the_row() {
        let conn = test_conn();
        save_signed(&conn, "K", "p", "s", "k", 1, 1).unwrap();
        clear(&conn).unwrap();
        assert_eq!(get(&conn).unwrap(), None);
    }
}
