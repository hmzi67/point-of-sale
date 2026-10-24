//! The network half: connecting this counter to its business, the first
//! upload or the first download, the background sync loop, and the stock
//! reservation an online sale takes before it is recorded.
//!
//! Nothing here holds the database lock across an `await`: every step reads
//! or writes in its own short `Db` call, so a slow connection never blocks
//! the till.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;

use super::client::{self, WsError};
use super::engine;
use super::lan;
use super::local;
use super::media::{self, MediaDirs};
use super::vault;
use crate::db::Db;
use crate::license::runtime::LicenseRuntime;
use crate::session::Session;

/// `WorkspaceInfo` after anything changed.
pub const STATUS_EVENT: &str = "workspace:status";
/// Downloaded changes landed: open screens should reload their data.
pub const APPLIED_EVENT: &str = "workspace:applied";
/// This counter's data was replaced by the business data: sign out.
pub const REPLACED_EVENT: &str = "workspace:replaced";

const PULL_LIMIT: i64 = 1000;
/// A file no record uses is deleted after this long (`media::sweep`).
const MEDIA_GRACE: Duration = Duration::from_secs(24 * 3600);

static ROUND_RUNNING: AtomicBool = AtomicBool::new(false);
/// Whether the last round reached the server.
static ONLINE: AtomicBool = AtomicBool::new(false);
/// A connect / seed / download in progress (the loop stays out of its way).
static SETUP_RUNNING: AtomicBool = AtomicBool::new(false);

fn wake() -> &'static Notify {
    static WAKE: OnceLock<Notify> = OnceLock::new();
    WAKE.get_or_init(Notify::new)
}

fn started() -> Instant {
    static STARTED: OnceLock<Instant> = OnceLock::new();
    *STARTED.get_or_init(Instant::now)
}

/// Milliseconds after `started()` of the last sign of activity: a local
/// write, downloaded changes, or a nudge from a counter nearby.
static LAST_ACTIVITY_MS: AtomicU64 = AtomicU64::new(0);

fn note_activity() {
    LAST_ACTIVITY_MS.store(started().elapsed().as_millis() as u64, Ordering::Relaxed);
}

/// How long the loop waits when nothing wakes it: every 2 seconds while the
/// business is busy, slowing to 30 seconds after an hour of quiet. Most of a
/// day is quiet, so a shared server answers far fewer polls, and a busy
/// counter still sees the others' sales within seconds.
fn poll_interval(idle: Duration) -> Duration {
    match idle.as_secs() {
        0..=59 => Duration::from_secs(2),
        60..=599 => Duration::from_secs(5),
        600..=3599 => Duration::from_secs(15),
        _ => Duration::from_secs(30),
    }
}

fn idle_for() -> Duration {
    started().elapsed().saturating_sub(Duration::from_millis(LAST_ACTIVITY_MS.load(Ordering::Relaxed)))
}

/// What the UI shows: Settings → Business & counters, and the top bar.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInfo {
    /// standalone | pending | connected
    pub mode: String,
    /// synced | pending | offline | error | seeding | standalone | setup
    pub state: String,
    pub business_name: Option<String>,
    pub counter_name: Option<String>,
    pub counter_code: Option<String>,
    pub pending: i64,
    pub parked: i64,
    pub last_ok_at: Option<String>,
    pub last_error: Option<String>,
    pub seeding: bool,
    pub counters: Vec<CounterInfo>,
    pub issues: Vec<Issue>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CounterInfo {
    pub code: String,
    pub name: String,
    pub active: bool,
    pub last_seen_at: Option<String>,
    pub this_counter: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub id: i64,
    pub kind: String,
    pub detail: String,
    pub created_at: String,
}

struct StateRow {
    mode: String,
    business_name: Option<String>,
    counter_name: Option<String>,
    counter_code: Option<String>,
    seeding: bool,
    last_ok_at: Option<String>,
    last_error: Option<String>,
    install_id: String,
}

pub fn info(conn: &Connection) -> Result<WorkspaceInfo, rusqlite::Error> {
    let state = conn.query_row(
        "SELECT mode, business_name, terminal_name, terminal_code, seed_pending, last_ok_at, last_error, install_id
           FROM sync_state WHERE id = 1",
        [],
        |row| {
            Ok(StateRow {
                mode: row.get(0)?,
                business_name: row.get(1)?,
                counter_name: row.get(2)?,
                counter_code: row.get(3)?,
                seeding: row.get(4)?,
                last_ok_at: row.get(5)?,
                last_error: row.get(6)?,
                install_id: row.get(7)?,
            })
        },
    )?;
    let StateRow { mode, business_name, counter_name, counter_code, seeding, last_ok_at, last_error, install_id } = state;
    let pending = engine::pending_count(conn)?;
    let parked: i64 = conn.query_row("SELECT COUNT(*) FROM sync_parked", [], |row| row.get(0))?;

    let counters = conn
        .prepare("SELECT code, name, active, last_seen_at, install_id FROM sync_terminals ORDER BY name")?
        .query_map([], |row| {
            Ok(CounterInfo {
                code: row.get(0)?,
                name: row.get(1)?,
                active: row.get(2)?,
                last_seen_at: row.get(3)?,
                this_counter: row.get::<_, String>(4)? == install_id,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut issues = conn
        .prepare("SELECT id, kind, detail, created_at FROM sync_issues WHERE resolved_at IS NULL ORDER BY id DESC LIMIT 50")?
        .query_map([], |row| Ok(Issue { id: row.get(0)?, kind: row.get(1)?, detail: row.get(2)?, created_at: row.get(3)? }))?
        .collect::<Result<Vec<_>, _>>()?;
    let parked_rows = conn
        .prepare("SELECT entity, reason, first_seen FROM sync_parked ORDER BY first_seen LIMIT 20")?
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    for (entity, reason, first_seen) in parked_rows {
        issues.push(Issue {
            id: 0,
            kind: "waiting".into(),
            detail: format!("A {} record from another counter is not applied yet: {reason}.", entity.replace('_', " ")),
            created_at: first_seen,
        });
    }

    let state = match mode.as_str() {
        _ if SETUP_RUNNING.load(Ordering::SeqCst) => "setup",
        "standalone" => "standalone",
        "pending" => "setup",
        _ if seeding => "seeding",
        _ if last_error.is_some() && !ONLINE.load(Ordering::SeqCst) => "offline",
        _ if last_error.is_some() => "error",
        _ if pending > 0 => "pending",
        _ => "synced",
    };
    Ok(WorkspaceInfo {
        mode,
        state: state.into(),
        business_name,
        counter_name,
        counter_code,
        pending,
        parked,
        last_ok_at,
        last_error,
        seeding,
        counters,
        issues,
    })
}

fn db_err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn emit_status(app: &AppHandle) {
    if let Ok(info) = app.state::<Db>().with_conn(info) {
        let _ = app.emit(STATUS_EVENT, &info);
    }
}

fn token(db: &Db) -> Result<Option<String>, String> {
    db.with_conn(vault::read)
        .map_err(db_err)
}

fn require_token(db: &Db) -> Result<String, String> {
    token(db)?.ok_or_else(|| "This counter is not connected to a business yet.".to_string())
}

/// A connected counter's sync was refused for good (its token, its slot or
/// its licence): say so plainly; everything stays queued.
fn record_error(db: &Db, error: &WsError) {
    ONLINE.store(!matches!(error, WsError::Offline(_)), Ordering::SeqCst);
    let message = match error.code() {
        Some("terminal_revoked") | Some("unauthenticated") => {
            "This counter was disconnected from the business. Reconnect it in Settings → Business & counters.".to_string()
        }
        _ => error.to_string(),
    };
    let _ = db.with_conn(|conn| {
        conn.execute(
            "UPDATE sync_state SET last_error = ?1, last_error_at = datetime('now', 'localtime') WHERE id = 1",
            params![crate::startup_log::clip(&message)],
        )
    });
}

/// Outcome of `connect`: what the person has to choose next.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectOutcome {
    /// `upload` (empty business: this counter's data can start it),
    /// `download` (use the business data here), `wait` (another counter is
    /// still uploading), `connected` (already set up — nothing to choose).
    pub next: String,
    pub business_name: String,
    pub counter_name: String,
    pub counters: i64,
    pub max_counters: i64,
    pub plan: Option<String>,
    /// Records on this counter that `download` would replace (they are
    /// backed up first).
    pub local_records: i64,
}

fn local_record_count(conn: &Connection) -> Result<i64, rusqlite::Error> {
    conn.query_row(
        "SELECT (SELECT COUNT(*) FROM items) + (SELECT COUNT(*) FROM sales) + (SELECT COUNT(*) FROM customers)
              + (SELECT COUNT(*) FROM expenses) + (SELECT COUNT(*) FROM suppliers)",
        [],
        |row| row.get(0),
    )
}

/// Registers this device (which must hold an activated licence) as a counter
/// of the licence's business. Nothing local changes yet beyond remembering
/// the connection; the next step is `upload` or `download`.
pub async fn connect(app: &AppHandle, counter_name: Option<String>) -> Result<ConnectOutcome, String> {
    let db = app.state::<Db>();
    let license = app.state::<LicenseRuntime>();
    let license_key = db
        .with_conn(|conn| crate::db::license::get(conn).map(|r| r.map(|r| r.license_key)))
        .map_err(db_err)?
        .ok_or("Activate the licence on this device first (Settings → Licence & Sync).")?;
    let fingerprint = license.device_fingerprint()?;
    let mode: String = db.with_conn(|conn| conn.query_row("SELECT mode FROM sync_state WHERE id = 1", [], |row| row.get(0))).map_err(db_err)?;
    // Connected already: re-registering keeps its place (same install id).
    // Otherwise: a fresh identity first (see `engine::fresh_identity`).
    let install_id = if mode == "connected" {
        db.with_conn(local::install_id).map_err(db_err)?
    } else {
        db.with_transaction(|tx| engine::fresh_identity(tx)).map_err(db_err)?
    };
    let name = counter_name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());

    let registration = client::register(&license_key, &fingerprint, &install_id, name.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    // Still connected and still joined (same database): just a new token.
    let next_mode = if mode == "connected" && registration.terminal.joined { "connected" } else { "pending" };
    let local_records = db
        .with_conn(|conn| {
            vault::write(conn, Some(&registration.token))?;
            conn.execute(
                "UPDATE sync_state SET mode = ?1, business_id = ?2, business_name = ?3,
                                       terminal_code = ?4, terminal_name = ?5, last_error = NULL WHERE id = 1",
                params![
                    next_mode,
                    registration.business.id,
                    registration.business.name,
                    registration.terminal.code,
                    registration.terminal.name
                ],
            )?;
            local_record_count(conn)
        })
        .map_err(db_err)?;
    emit_status(app);
    if next_mode == "connected" {
        nudge();
    }

    let business = registration.business;
    Ok(ConnectOutcome {
        next: if next_mode == "connected" {
            "connected"
        } else if business.has_data {
            "download"
        } else if business.seeding {
            "wait"
        } else {
            "upload"
        }
        .into(),
        business_name: business.name,
        counter_name: registration.terminal.name,
        counters: business.counters,
        max_counters: business.max_counters,
        plan: business.plan,
        local_records,
    })
}

struct SetupGuard;
impl SetupGuard {
    fn take() -> Result<Self, String> {
        if SETUP_RUNNING.swap(true, Ordering::SeqCst) {
            return Err("Already setting this counter up — wait for it to finish.".into());
        }
        Ok(SetupGuard)
    }
}
impl Drop for SetupGuard {
    fn drop(&mut self) {
        SETUP_RUNNING.store(false, Ordering::SeqCst);
    }
}

fn require_mode(db: &Db, wanted: &str) -> Result<(), String> {
    let mode: String = db.with_conn(|conn| conn.query_row("SELECT mode FROM sync_state WHERE id = 1", [], |r| r.get(0))).map_err(db_err)?;
    if mode != wanted {
        return Err(match mode.as_str() {
            "connected" => "This counter is already connected to the business.".into(),
            _ => "Connect this counter to the business first.".into(),
        });
    }
    Ok(())
}

/// The business is empty: this counter's data becomes the business data.
pub async fn upload(app: &AppHandle) -> Result<WorkspaceInfo, String> {
    let _guard = SetupGuard::take()?;
    let db = app.state::<Db>();
    require_mode(&db, "pending")?;
    let token = require_token(&db)?;
    client::action(&token, "claim_seed", None).await.map_err(|e| e.to_string())?;
    let queued = db.with_transaction(|tx| engine::start_seed(tx)).map_err(db_err)?;
    crate::startup_log::log(&format!("workspace: seeding the business with {queued} local changes"));
    drop(_guard);
    nudge();
    emit_status(app);
    db.with_conn(info).map_err(db_err)
}

/// Copies the database file aside before it is replaced.
fn backup(app: &AppHandle, db: &Db) -> Result<String, String> {
    let dir = app.path().app_data_dir().map_err(db_err)?.join("backups");
    std::fs::create_dir_all(&dir).map_err(db_err)?;
    let path = dir.join(format!("pos-before-business-data-{}.db", chrono::Local::now().format("%Y%m%d-%H%M%S")));
    let target = path.to_string_lossy().to_string();
    db.with_conn(|conn| conn.execute("VACUUM INTO ?1", params![target])).map_err(db_err)?;
    Ok(target)
}

/// Replaces this counter's data with the business data. The whole business
/// is downloaded first (into staging tables) and only then applied, in one
/// transaction, after the current database file is backed up. Signs out:
/// the staff accounts are now the business's.
pub async fn download(app: &AppHandle) -> Result<String, String> {
    let _guard = SetupGuard::take()?;
    let db = app.state::<Db>();
    require_mode(&db, "pending")?;
    let token = require_token(&db)?;

    db.with_conn(engine::clear_staging).map_err(db_err)?;
    let mut since = 0;
    let last = loop {
        let page = client::pull(&token, since, PULL_LIMIT).await.map_err(|e| e.to_string())?;
        db.with_transaction(|tx| engine::stage_page(tx, &page)).map_err(db_err)?;
        since = page.cursor;
        if !page.more {
            break page;
        }
    };

    let backup_path = backup(app, &db)?;
    let stats = db
        .try_transaction(|tx| engine::replace_with_staged(tx, &last))
        .map_err(db_err)?
        .map_err(|e| e.to_string())?;
    crate::startup_log::log(&format!(
        "workspace: replaced local data with the business data ({} records, {} waiting); backup at {backup_path}",
        stats.applied, stats.parked
    ));

    if let Err(e) = client::action(&token, "joined", None).await {
        record_error(&db, &e); // retried by the loop
    }
    app.state::<Session>().clear();
    drop(_guard);
    let _ = app.emit(REPLACED_EVENT, ());
    let _ = app.emit(APPLIED_EVENT, ());
    emit_status(app);
    nudge();
    Ok(backup_path)
}

/// Stops syncing. The data stays; only a counter with nothing left to send
/// may disconnect, so no sale is ever stranded.
pub fn disconnect(db: &Db) -> Result<(), String> {
    let pending = db.with_conn(engine::pending_count).map_err(db_err)?;
    if pending > 0 {
        return Err(format!("{pending} change(s) on this counter have not reached the business yet. Sync first, then disconnect."));
    }
    db.with_conn(|conn| {
        vault::write(conn, None)?;
        conn.execute("UPDATE sync_state SET mode = 'standalone', seed_pending = 0, last_error = NULL WHERE id = 1", [])
    })
    .map_err(db_err)?;
    Ok(())
}

pub async fn rename(app: &AppHandle, name: String) -> Result<WorkspaceInfo, String> {
    let db = app.state::<Db>();
    let token = require_token(&db)?;
    let status = client::action(&token, "rename", Some(name.trim())).await.map_err(|e| e.to_string())?;
    db.with_conn(|conn| conn.execute("UPDATE sync_state SET terminal_name = ?1 WHERE id = 1", params![status.terminal.name]))
        .map_err(db_err)?;
    emit_status(app);
    db.with_conn(info).map_err(db_err)
}

/// Wakes the loop for a sync soon (after a local write).
pub fn nudge() {
    note_activity();
    wake().notify_one();
}

pub fn spawn_loop(app: AppHandle) {
    note_activity();
    if let Err(e) = app.state::<Db>().with_conn(vault::migrate) {
        crate::startup_log::warn(&format!("workspace: could not move the counter token to the credential store: {e}"));
    }
    lan::spawn_listener(nudge);
    tauri::async_runtime::spawn(async move {
        let mut last_sweep: Option<Instant> = None;
        loop {
            if last_sweep.is_none_or(|t| t.elapsed() >= Duration::from_secs(3600)) {
                last_sweep = Some(Instant::now());
                sweep_media(&app);
            }
            // Whichever comes first: the interval, or a write.
            let _ = tokio::time::timeout(poll_interval(idle_for()), wake().notified()).await;
            // Let a burst of writes (a sale is several) settle into one push.
            tokio::time::sleep(Duration::from_millis(300)).await;
            if let Err(e) = round(&app).await {
                crate::startup_log::warn(&format!("workspace sync: {}", crate::startup_log::clip(&e)));
            }
        }
    });
}

fn media_dirs(app: &AppHandle) -> Result<MediaDirs, String> {
    Ok(MediaDirs::new(&app.path().app_data_dir().map_err(db_err)?))
}

/// Deletes photo files no record uses any more (standalone counters too).
fn sweep_media(app: &AppHandle) {
    let Ok(dirs) = media_dirs(app) else { return };
    match app.state::<Db>().with_conn(|conn| media::sweep(conn, &dirs, MEDIA_GRACE)) {
        Ok(0) => {}
        Ok(n) => crate::startup_log::log(&format!("workspace: deleted {n} unused image file(s)")),
        Err(e) => crate::startup_log::warn(&format!("workspace: image clean-up failed: {e}")),
    }
}

/// One sync round: send everything queued, then read everything new.
pub async fn round(app: &AppHandle) -> Result<(), String> {
    if SETUP_RUNNING.load(Ordering::SeqCst) || ROUND_RUNNING.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let result = round_inner(app).await;
    ROUND_RUNNING.store(false, Ordering::SeqCst);
    emit_status(app);
    result
}

async fn round_inner(app: &AppHandle) -> Result<(), String> {
    let db = app.state::<Db>();
    let connected = db.with_conn(local::is_connected).map_err(db_err)?;
    if !connected {
        return Ok(());
    }
    let token = require_token(&db)?;
    db.with_conn(engine::sweep_stale_reservations).map_err(db_err)?;
    let (business_id, install_id) = db
        .with_conn(|conn| conn.query_row("SELECT business_id, install_id FROM sync_state WHERE id = 1", [], |r| Ok((r.get(0)?, r.get::<_, String>(1)?))))
        .map_err(db_err)?;
    lan::set_identity(business_id, &install_id);
    let dirs = media_dirs(app)?;

    let outcome = match upload_media(&db, &token, &dirs).await {
        Ok(()) => sync_with(&db, &token).await,
        Err(e) => Err(e),
    };
    match outcome {
        Ok(stats) => {
            ONLINE.store(true, Ordering::SeqCst);
            if stats.pushed > 0 {
                lan::announce();
            }
            let downloaded = download_media(&db, &token, &dirs).await;
            let applied = stats.applied + downloaded;
            if applied > 0 {
                note_activity();
            }
            db.with_conn(|conn| {
                conn.execute(
                    "UPDATE sync_state SET last_ok_at = datetime('now', 'localtime'), last_error = NULL WHERE id = 1",
                    [],
                )
            })
            .map_err(db_err)?;
            if applied > 0 {
                let _ = app.emit(APPLIED_EVENT, ());
            }
            Ok(())
        }
        Err(e) => {
            record_error(&db, &e);
            Err(e.to_string())
        }
    }
}

/// What one `sync_with` did.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct SyncStats {
    /// Changes the server took from this counter.
    pub pushed: usize,
    /// Downloaded records applied here.
    pub applied: usize,
}

/// Offers the server the photos and logo this counter's records use, and
/// uploads the ones it lacks — before the records are pushed. A file the
/// server refuses (not an image, too large) is reported once and not
/// offered again; no answer stops the round like any sync error.
pub(crate) async fn upload_media(db: &Db, token: &str, dirs: &MediaDirs) -> Result<(), WsError> {
    let local_err = |e: crate::db::DbError| WsError::Server(e.to_string());
    let files = db.with_conn(|conn| media::to_upload(conn, dirs)).map_err(local_err)?;
    for chunk in files.chunks(200) {
        let names: Vec<String> = chunk.iter().map(|f| f.name.clone()).collect();
        let missing = client::media_missing(token, &names).await?;
        for file in chunk {
            if missing.contains(&file.name) {
                let outcome = match media::upload_bytes(file) {
                    Ok(bytes) => client::media_upload(token, &file.name, bytes).await,
                    Err(e) => Err(WsError::Refused { status: 422, code: "bad_image".into(), message: e.to_string() }),
                };
                match outcome {
                    Ok(()) => {}
                    Err(WsError::Refused { message, .. }) => {
                        db.with_conn(|conn| {
                            conn.execute(
                                "INSERT INTO sync_issues (kind, detail) VALUES ('media', ?1)",
                                params![format!("A photo could not be shared with the other counters ({}): {message}", file.name)],
                            )
                        })
                        .map_err(local_err)?;
                    }
                    Err(e) => return Err(e),
                }
            }
            db.with_conn(|conn| media::mark_on_server(conn, &file.name)).map_err(local_err)?;
        }
    }
    Ok(())
}

/// Downloads the photos and logo this counter's records name but does not
/// have. Best effort: whatever fails is tried again on a later round.
/// Returns how many files arrived.
pub(crate) async fn download_media(db: &Db, token: &str, dirs: &MediaDirs) -> usize {
    let Ok(files) = db.with_conn(|conn| media::to_download(conn, dirs)) else { return 0 };
    let mut arrived = 0;
    for file in files {
        match client::media_fetch(token, &file.name).await {
            Ok(Some(bytes)) => {
                if let Err(e) = crate::images::write_atomically(&file.path, &bytes) {
                    crate::startup_log::warn(&format!("workspace: could not save {}: {e}", file.name));
                    continue;
                }
                let _ = db.with_conn(|conn| media::mark_on_server(conn, &file.name));
                arrived += 1;
            }
            Ok(None) | Err(WsError::Refused { .. }) => {
                let _ = db.with_conn(|conn| media::mark_unavailable(conn, &file.name));
            }
            Err(_) => break, // offline: next round
        }
    }
    arrived
}

/// Push until the outbox is empty, finish a seed, then pull until caught
/// up.
pub(crate) async fn sync_with(db: &Db, token: &str) -> Result<SyncStats, WsError> {
    let local_err = |e: String| WsError::Server(e);
    let last_error: Option<String> =
        db.with_conn(|conn| conn.query_row("SELECT last_error FROM sync_state WHERE id = 1", [], |r| r.get(0))).map_err(|e| local_err(e.to_string()))?;

    let mut pushed = 0;
    for _ in 0..50 {
        let plan = db.with_conn(|conn| engine::build_push(conn, last_error.as_deref())).map_err(|e| local_err(e.to_string()))?;
        if plan.is_empty() {
            break;
        }
        let response = match client::push(token, &plan.body).await {
            // Only a counter that is connected locally gets here, which means
            // it uploaded or downloaded the business data; if the server
            // missed the "joined" that followed a download, send it again.
            Err(e) if e.code() == Some("not_joined") => {
                client::action(token, "joined", None).await?;
                client::push(token, &plan.body).await?
            }
            other => other?,
        };
        db.with_transaction(|tx| engine::ack_push(tx, &plan, &response)).map_err(|e| local_err(e.to_string()))?;
        pushed += response.accepted.len() + response.stock_accepted.len() + response.releases_accepted.len();
    }

    let (seeding, pending) = db
        .with_conn(|conn| Ok((conn.query_row("SELECT seed_pending FROM sync_state WHERE id = 1", [], |r| r.get::<_, bool>(0))?, engine::pending_count(conn)?)))
        .map_err(|e| local_err(e.to_string()))?;
    if seeding && pending == 0 {
        client::action(token, "complete_seed", None).await?;
        db.with_conn(|conn| conn.execute("UPDATE sync_state SET seed_pending = 0 WHERE id = 1", [])).map_err(|e| local_err(e.to_string()))?;
        crate::startup_log::log("workspace: the business data is uploaded; other counters can join");
    }

    let mut applied = 0;
    for _ in 0..100 {
        let since: i64 = db.with_conn(|conn| conn.query_row("SELECT cursor FROM sync_state WHERE id = 1", [], |r| r.get(0))).map_err(|e| local_err(e.to_string()))?;
        let pending = db.with_conn(engine::pending_count).map_err(|e| local_err(e.to_string()))?;
        let page = client::pull_reporting(token, since, PULL_LIMIT, pending, last_error.as_deref()).await?;
        let stats = db.with_transaction(|tx| engine::apply_page(tx, &page)).map_err(|e| local_err(e.to_string()))?;
        applied += stats.applied;
        if !page.more {
            break;
        }
    }
    Ok(SyncStats { pushed, applied })
}

/// Before an online sale is recorded: takes its stock from the central
/// balance. `Ok(Some(key))` — record the sale under this key (also when the
/// server could not be asked: the sale then goes through offline and its
/// stock syncs later). `Ok(None)` — not connected. `Err` — not enough stock
/// across the business; nothing was taken and nothing should be recorded.
pub async fn reserve_for_sale(db: &Db, lines: &[(i64, f64)]) -> Result<Option<String>, String> {
    let prepared = db
        .with_conn(|conn| {
            if !local::is_connected(conn)? {
                return Ok(None);
            }
            let token = vault::read(conn)?;
            let key = format!("sale:{}:{}", local::install_id(conn)?, local::random_hex16());
            conn.execute("INSERT INTO sync_reservations (key, status) VALUES (?1, 'pending')", params![key])?;
            Ok(Some((key, token, engine::reservation_lines(conn, lines)?)))
        })
        .map_err(db_err)?;
    let Some((key, token, gid_lines)) = prepared else {
        return Ok(None);
    };
    let Some(token) = token.filter(|_| !gid_lines.is_empty()) else {
        mark(db, &key, "offline")?;
        return Ok(Some(key));
    };

    match client::reserve(&token, &key, &gid_lines).await {
        Ok(reply) if reply.status == "ok" => {
            db.with_transaction(|tx| engine::confirm_reservation(tx, &key, &reply.taken, &reply.balances)).map_err(db_err)?;
            Ok(Some(key))
        }
        Ok(reply) if reply.status == "insufficient" => {
            mark(db, &key, "failed")?;
            db.with_transaction(|tx| engine::refresh_balances(tx, &reply.balances)).map_err(db_err)?;
            let detail = db
                .with_conn(|conn| {
                    let mut parts = Vec::new();
                    for shortage in &reply.shortages {
                        let name: Option<String> = conn
                            .query_row("SELECT name FROM items WHERE gid = ?1", params![shortage.item_gid], |r| r.get(0))
                            .optional()?;
                        parts.push(format!("{} has {} left", name.unwrap_or_else(|| "An item".into()), format_qty(shortage.available)));
                    }
                    Ok(parts.join(", "))
                })
                .map_err(db_err)?;
            Err(format!("Not enough stock across the business: {detail}. Another counter may have just sold it."))
        }
        // No answer: record the sale and sync its stock later — unless the
        // business chose to stop selling while a counter is offline.
        Ok(_) | Err(_) => {
            let block: bool = db
                .with_conn(|conn| conn.query_row("SELECT offline_sales = 'block' FROM app_config WHERE id = 1", [], |r| r.get(0)))
                .map_err(db_err)?;
            if block {
                mark(db, &key, "failed")?;
                return Err(OFFLINE_BLOCKED.to_string());
            }
            mark(db, &key, "offline")?;
            Ok(Some(key))
        }
    }
}

pub const OFFLINE_BLOCKED: &str = "This counter can't reach the business server, so stock can't be checked across counters. \
Sales are set to stop while offline (Settings → Business & counters). Check the internet connection and try again.";

fn format_qty(qty: f64) -> String {
    if qty.fract() == 0.0 {
        format!("{qty:.0}")
    } else {
        format!("{qty:.2}")
    }
}

fn mark(db: &Db, key: &str, status: &str) -> Result<(), String> {
    db.with_conn(|conn| conn.execute("UPDATE sync_reservations SET status = ?1 WHERE key = ?2", params![status, key]))
        .map_err(db_err)?;
    Ok(())
}

/// Runs a sale's write with its stock changes tagged by `key` and marks the
/// reservation used, all inside the sale's own transaction.
pub fn record_sale<T, E: From<rusqlite::Error>>(
    tx: &rusqlite::Transaction,
    key: Option<&str>,
    write: impl FnOnce(&rusqlite::Transaction) -> Result<T, E>,
) -> Result<T, E> {
    let Some(key) = key else {
        return write(tx);
    };
    local::set_stock_tag(tx, Some(key))?;
    tx.execute("UPDATE sync_reservations SET status = 'used' WHERE key = ?1", params![key])?;
    let result = write(tx)?;
    local::set_stock_tag(tx, None)?;
    Ok(result)
}

/// The sale was not recorded after all: give its reservation back.
pub fn abandon_sale(db: &Db, key: &str) {
    if let Err(e) = db.with_conn(|conn| engine::release_reservation(conn, key)) {
        crate::startup_log::warn(&format!("workspace: could not queue a stock release: {e}"));
    }
    nudge();
}

#[cfg(test)]
mod poll_tests {
    use super::*;

    #[test]
    fn a_busy_business_is_polled_every_2_seconds_and_a_quiet_one_backs_off_to_30() {
        let at = |secs| poll_interval(Duration::from_secs(secs)).as_secs();
        assert_eq!((at(0), at(59)), (2, 2));
        assert_eq!((at(60), at(599)), (5, 5));
        assert_eq!((at(600), at(3599)), (15, 15));
        assert_eq!(at(3600 * 8), 30);
    }
}
