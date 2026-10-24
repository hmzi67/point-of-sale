//! Product photos and the logo, shared by a business's counters.
//!
//! The records carry only a file name (`items.image_path`,
//! `app_config.logo_path`); the files travel separately, through the
//! business server's media store:
//!
//! - **Up, before the push.** A file this counter's records use and that the
//!   server is not known to have is offered (`check`), and the ones it lacks
//!   are uploaded — before the records that name them are pushed, so another
//!   counter almost never sees a name before its file.
//! - **Down, after the pull.** A name this counter's records use with no file
//!   here is downloaded. One the server does not have yet is retried a minute
//!   later (`retry_after`).
//! - **Kept.** Files stay on the device, so photos show offline. A file no
//!   record uses any more is deleted a day later (`sweep`), which also cleans
//!   up a photo picked in the item form but never saved.
//!
//! Names are content hashes (`images`), so the same photo is one file on
//! every counter and a retried upload changes nothing.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use rusqlite::{params, Connection};

use crate::images;

/// The two directories a file can live in.
#[derive(Debug, Clone)]
pub struct MediaDirs {
    pub items: PathBuf,
    pub logo: PathBuf,
}

impl MediaDirs {
    pub fn new(app_data_dir: &Path) -> Self {
        Self { items: images::images_dir(app_data_dir), logo: images::logo_dir(app_data_dir) }
    }
}

/// A file a record uses, and where it lives on this counter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaFile {
    pub name: String,
    pub path: PathBuf,
}

/// How long a name the server did not have waits before it is asked for again.
const RETRY_AFTER: &str = "+1 minute";
/// Files to download in one round (the rest follow next round).
pub const DOWNLOAD_BATCH: usize = 40;

/// Every valid file name this counter's records use, with its local path.
pub fn referenced(conn: &Connection, dirs: &MediaDirs) -> Result<Vec<MediaFile>, rusqlite::Error> {
    let mut files = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT DISTINCT image_path, 0 FROM items WHERE image_path IS NOT NULL
         UNION SELECT logo_path, 1 FROM app_config WHERE logo_path IS NOT NULL",
    )?;
    let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)))?;
    for row in rows {
        let (name, is_logo) = row?;
        if images::is_valid_name(&name) {
            let dir = if is_logo { &dirs.logo } else { &dirs.items };
            files.push(MediaFile { path: dir.join(&name), name });
        }
    }
    Ok(files)
}

/// Files here that the server is not known to have.
pub fn to_upload(conn: &Connection, dirs: &MediaDirs) -> Result<Vec<MediaFile>, rusqlite::Error> {
    let known = names_where(conn, "on_server = 1")?;
    Ok(referenced(conn, dirs)?.into_iter().filter(|f| !known.contains(&f.name) && f.path.is_file()).collect())
}

/// Files the records name that are not here, and are due to be asked for.
pub fn to_download(conn: &Connection, dirs: &MediaDirs) -> Result<Vec<MediaFile>, rusqlite::Error> {
    let waiting = names_where(conn, "retry_after > datetime('now')")?;
    Ok(referenced(conn, dirs)?
        .into_iter()
        .filter(|f| !waiting.contains(&f.name) && !f.path.exists())
        .take(DOWNLOAD_BATCH)
        .collect())
}

fn names_where(conn: &Connection, condition: &str) -> Result<HashSet<String>, rusqlite::Error> {
    let mut stmt = conn.prepare(&format!("SELECT name FROM sync_media WHERE {condition}"))?;
    let names = stmt.query_map([], |row| row.get(0))?.collect::<Result<HashSet<String>, _>>()?;
    Ok(names)
}

pub fn mark_on_server(conn: &Connection, name: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO sync_media (name, on_server, retry_after) VALUES (?1, 1, NULL)
         ON CONFLICT (name) DO UPDATE SET on_server = 1, retry_after = NULL",
        params![name],
    )?;
    Ok(())
}

/// The server does not have it (yet): ask again a minute from now.
pub fn mark_unavailable(conn: &Connection, name: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        &format!(
            "INSERT INTO sync_media (name, on_server, retry_after) VALUES (?1, 0, datetime('now', '{RETRY_AFTER}'))
             ON CONFLICT (name) DO UPDATE SET retry_after = datetime('now', '{RETRY_AFTER}')"
        ),
        params![name],
    )?;
    Ok(())
}

/// The bytes to upload for a file: a large JPEG/PNG from before photos were
/// shrunk on saving is shrunk now (under the same name, which is only an id).
pub fn upload_bytes(file: &MediaFile) -> Result<Vec<u8>, images::ImageError> {
    let bytes = fs::read(&file.path).map_err(|e| images::ImageError::Io(e.to_string()))?;
    let extension = file.name.rsplit_once('.').map_or("", |(_, e)| e);
    if bytes.len() <= images::MAX_SHARED_BYTES / 2 {
        return Ok(bytes);
    }
    match images::shrink(&bytes, extension, images::MAX_DIMENSION)? {
        // Keep the name's format: the server checks the bytes against it.
        (shrunk, ext) if ext == extension || (ext == "jpg" && extension == "jpeg") => Ok(shrunk),
        _ => Ok(bytes),
    }
}

/// Deletes files in the two directories that no record uses and that are
/// older than `grace`. Returns how many were deleted.
pub fn sweep(conn: &Connection, dirs: &MediaDirs, grace: Duration) -> Result<usize, rusqlite::Error> {
    let used: HashSet<PathBuf> = referenced(conn, dirs)?.into_iter().map(|f| f.path).collect();
    let cutoff = SystemTime::now().checked_sub(grace).unwrap_or(SystemTime::UNIX_EPOCH);
    let mut deleted = 0;
    for dir in [&dirs.items, &dirs.logo] {
        let Ok(entries) = fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let old = entry.metadata().and_then(|m| m.modified()).is_ok_and(|t| t <= cutoff);
            if old && path.is_file() && !used.contains(&path) && fs::remove_file(&path).is_ok() {
                deleted += 1;
            }
        }
    }
    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::test_conn;

    fn dirs(label: &str) -> MediaDirs {
        let root = std::env::temp_dir().join(format!("pos-media-{label}-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = fs::remove_dir_all(&root);
        MediaDirs::new(&root)
    }

    #[test]
    fn files_go_up_once_and_missing_ones_come_down_with_a_pause_after_a_miss() {
        let conn = test_conn();
        let dirs = dirs("updown");
        conn.execute("INSERT INTO items (name, price_minor, image_path) VALUES ('Tea', 100, 'item-aa.jpg')", []).unwrap();
        conn.execute("INSERT INTO items (name, price_minor, image_path) VALUES ('Bad', 100, '../escape.jpg')", []).unwrap();
        conn.execute("UPDATE app_config SET logo_path = 'logo-bb.png'", []).unwrap();

        let names = |files: Vec<MediaFile>| files.into_iter().map(|f| f.name).collect::<Vec<_>>();
        assert!(to_upload(&conn, &dirs).unwrap().is_empty(), "nothing on disk yet");
        let mut down = names(to_download(&conn, &dirs).unwrap());
        down.sort();
        assert_eq!(down, ["item-aa.jpg", "logo-bb.png"], "an invalid name is never fetched");

        images::write_atomically(&dirs.items.join("item-aa.jpg"), b"x").unwrap();
        assert_eq!(names(to_upload(&conn, &dirs).unwrap()), ["item-aa.jpg"]);
        mark_on_server(&conn, "item-aa.jpg").unwrap();
        assert!(to_upload(&conn, &dirs).unwrap().is_empty(), "uploaded once");

        mark_unavailable(&conn, "logo-bb.png").unwrap();
        assert!(to_download(&conn, &dirs).unwrap().is_empty(), "a miss waits before asking again");
        conn.execute("UPDATE sync_media SET retry_after = datetime('now', '-1 second')", []).unwrap();
        assert_eq!(names(to_download(&conn, &dirs).unwrap()), ["logo-bb.png"]);
        assert_eq!(to_download(&conn, &dirs).unwrap()[0].path, dirs.logo.join("logo-bb.png"), "the logo goes in its own folder");
    }

    #[test]
    fn sweep_deletes_old_unused_files_only() {
        let conn = test_conn();
        let dirs = dirs("sweep");
        conn.execute("INSERT INTO items (name, price_minor, image_path) VALUES ('Tea', 100, 'item-used.jpg')", []).unwrap();
        for name in ["item-used.jpg", "item-unused.jpg"] {
            images::write_atomically(&dirs.items.join(name), b"x").unwrap();
        }
        assert_eq!(sweep(&conn, &dirs, Duration::from_secs(3600)).unwrap(), 0, "a fresh file may be a photo not saved yet");
        assert_eq!(sweep(&conn, &dirs, Duration::ZERO).unwrap(), 1);
        assert!(dirs.items.join("item-used.jpg").exists());
        assert!(!dirs.items.join("item-unused.jpg").exists());
    }
}
