//! The database half of a sync round. Pure SQLite, no network: `runtime`
//! sends what `build_push` prepares and hands back what the server answered,
//! which keeps every step here testable against a stand-in server.
//!
//! Push: the outbox is folded into one change per row (its current state, or
//! its deletion), plus the stock deltas — a reserved sale's deltas grouped
//! under the reservation key, every other delta under its own key — plus
//! releases. Only what the server acknowledges leaves the outbox.
//!
//! Pull: a page is applied in one transaction, table by table in dependency
//! order. A row whose referenced record has not arrived yet, or that a local
//! rule refuses, is parked and retried on every later pull, and the page's
//! central stock balances are folded into each item's local stock.

use std::collections::{BTreeMap, HashMap};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::codec::{self, Applied, ApplyError};
use super::local;
use super::spec::{self, TABLES};

/// Rows per push. The server takes up to 500; smaller keeps a request quick
/// on a slow connection.
pub const PUSH_BATCH: usize = 300;
const STOCK_BATCH: usize = 4000;

#[derive(Debug, Default)]
pub struct PushPlan {
    pub body: Value,
    /// change id sent → the outbox rows it stands for
    changes: HashMap<i64, Vec<i64>>,
    stock: HashMap<String, Vec<i64>>,
    releases: HashMap<String, Vec<i64>>,
}

impl PushPlan {
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty() && self.stock.is_empty() && self.releases.is_empty()
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PushResponse {
    #[serde(default)]
    pub accepted: Vec<i64>,
    #[serde(default)]
    pub rejected: Vec<Rejected>,
    #[serde(default)]
    pub stock_accepted: Vec<String>,
    #[serde(default)]
    pub releases_accepted: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rejected {
    pub id: i64,
    pub error: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RemoteChange {
    pub entity: String,
    pub gid: String,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub data: Option<Map<String, Value>>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RemoteTerminal {
    pub code: String,
    pub name: String,
    pub install_id: String,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub last_seen_at: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PullPage {
    #[serde(default)]
    pub changes: Vec<RemoteChange>,
    #[serde(default)]
    pub stock: BTreeMap<String, f64>,
    pub cursor: i64,
    #[serde(default)]
    pub more: bool,
    #[serde(default)]
    pub terminals: Vec<RemoteTerminal>,
    #[serde(default)]
    pub business: Option<BusinessName>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct BusinessName {
    pub name: String,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyStats {
    pub applied: usize,
    pub kept_local: usize,
    pub parked: usize,
}

/// Rows, stock and releases waiting in the outbox (what the status bar shows).
pub fn pending_count(conn: &Connection) -> Result<i64, rusqlite::Error> {
    conn.query_row(
        "SELECT (SELECT COUNT(*) FROM (SELECT 1 FROM sync_outbox WHERE kind = 'row' GROUP BY entity, row_id))
              + (SELECT COUNT(*) FROM sync_outbox WHERE kind <> 'row')",
        [],
        |row| row.get(0),
    )
}

pub fn build_push(conn: &Connection, last_error: Option<&str>) -> Result<PushPlan, rusqlite::Error> {
    let install_id = local::install_id(conn)?;
    let mut plan = PushPlan::default();
    let mut changes = Vec::new();

    // One change per row, oldest first. When every queued entry of a row is
    // an update, only the fields they changed are sent (with the oldest
    // cursor they were made at); an insert among them sends the whole row.
    let mut groups = conn.prepare(
        "SELECT entity, row_id, group_concat(id), MAX(id),
                (SELECT gid FROM sync_outbox d WHERE d.entity = o.entity AND d.row_id = o.row_id AND d.op = 'delete'
                  ORDER BY d.id DESC LIMIT 1),
                CASE WHEN MAX(fields IS NULL) = 0 THEN group_concat(fields, '') END, MIN(base)
           FROM sync_outbox o WHERE kind = 'row'
          GROUP BY entity, row_id ORDER BY MIN(id) LIMIT ?1",
    )?;
    type Group = (String, i64, String, i64, Option<String>, Option<String>, Option<i64>);
    let rows = groups
        .query_map(params![PUSH_BATCH as i64], |row| -> rusqlite::Result<Group> {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (entity, row_id, ids, change_id, deleted_gid, fields, base) in rows {
        let ids: Vec<i64> = ids.split(',').filter_map(|s| s.parse().ok()).collect();
        let Some(table) = spec::find(&entity) else {
            plan.changes.insert(change_id, ids); // unknown now: acknowledge away
            continue;
        };
        match codec::read_row(conn, table, row_id)? {
            Some((gid, data)) => {
                let mut change = json!({ "id": change_id, "entity": entity, "gid": gid, "op": "upsert", "data": data });
                if let Some(fields) = fields {
                    let mut names: Vec<&str> = fields.split(',').filter(|f| !f.is_empty()).collect();
                    names.sort_unstable();
                    names.dedup();
                    change["fields"] = json!(names);
                    change["base"] = json!(base.unwrap_or(0));
                }
                changes.push(change);
            }
            // No delete recorded: created and deleted between two pushes, nothing to send.
            None => {
                if let Some(gid) = deleted_gid {
                    changes.push(json!({ "id": change_id, "entity": entity, "gid": gid, "op": "delete" }));
                }
            }
        }
        plan.changes.insert(change_id, ids);
    }

    // Stock deltas. A tagged (sale) group is keyed by its reservation key so
    // the server recognises lines it already took; every other delta is
    // keyed by this install and its outbox id.
    let mut stock_rows = conn.prepare(
        "SELECT o.id, o.qty, o.tag, i.gid FROM sync_outbox o LEFT JOIN items i ON i.id = o.row_id
          WHERE o.kind = 'stock' ORDER BY o.id LIMIT ?1",
    )?;
    let deltas = stock_rows
        .query_map(params![STOCK_BATCH as i64], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, f64>(1)?, row.get::<_, Option<String>>(2)?, row.get::<_, Option<String>>(3)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut grouped: BTreeMap<(String, String), (f64, Vec<i64>)> = BTreeMap::new();
    for (id, qty, tag, item_gid) in deltas {
        let Some(item_gid) = item_gid else {
            plan.stock.entry(format!("gone:{id}")).or_default().push(id); // item deleted: nothing to send
            continue;
        };
        let key = tag.unwrap_or_else(|| format!("d:{install_id}:{id}"));
        let entry = grouped.entry((key, item_gid)).or_insert((0.0, Vec::new()));
        entry.0 += qty;
        entry.1.push(id);
    }
    let mut stock = Vec::new();
    for ((key, item_gid), (qty, ids)) in grouped {
        stock.push(json!({ "key": key, "item_gid": item_gid, "qty": (qty * 10_000.0).round() / 10_000.0 }));
        plan.stock.entry(key).or_default().extend(ids);
    }

    let mut release_rows = conn.prepare("SELECT id, tag FROM sync_outbox WHERE kind = 'release' AND tag IS NOT NULL")?;
    let releases = release_rows
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    for (id, key) in releases {
        plan.releases.entry(key).or_default().push(id);
    }

    plan.body = json!({
        "changes": changes,
        "stock": stock,
        "releases": plan.releases.keys().collect::<Vec<_>>(),
        "pending": pending_count(conn)?,
        "last_error": last_error,
    });
    Ok(plan)
}

/// Clears what the server took. A change it rejected outright is recorded
/// as an issue and dropped, so one bad row never blocks the rest.
pub fn ack_push(conn: &Connection, plan: &PushPlan, response: &PushResponse) -> Result<usize, rusqlite::Error> {
    let mut done = Vec::new();
    for id in &response.accepted {
        done.extend(plan.changes.get(id).into_iter().flatten().copied());
    }
    for rejected in &response.rejected {
        if let Some(ids) = plan.changes.get(&rejected.id) {
            done.extend(ids.iter().copied());
            conn.execute(
                "INSERT INTO sync_issues (kind, detail) VALUES ('rejected', ?1)",
                params![format!("The server refused a change: {}", rejected.error)],
            )?;
        }
    }
    // A group that had nothing to send (row created and deleted between two
    // pushes, or an unknown table) is done. Anything sent but neither
    // accepted nor rejected stays queued.
    for (id, ids) in &plan.changes {
        let sent = plan.body["changes"].as_array().is_some_and(|c| c.iter().any(|v| v["id"].as_i64() == Some(*id)));
        if !sent {
            done.extend(ids.iter().copied()); // nothing to send for it
        }
    }
    for key in &response.stock_accepted {
        done.extend(plan.stock.get(key).into_iter().flatten().copied());
    }
    for (key, ids) in &plan.stock {
        if key.starts_with("gone:") {
            done.extend(ids.iter().copied());
        }
    }
    for key in &response.releases_accepted {
        done.extend(plan.releases.get(key).into_iter().flatten().copied());
        conn.execute("UPDATE sync_reservations SET status = 'failed' WHERE key = ?1", params![key])?;
    }
    let mut delete = conn.prepare("DELETE FROM sync_outbox WHERE id = ?1")?;
    for id in &done {
        delete.execute(params![id])?;
    }
    Ok(done.len())
}

/// Applies one downloaded page (call inside a transaction).
pub fn apply_page(conn: &Connection, page: &PullPage) -> Result<ApplyStats, rusqlite::Error> {
    local::set_applying(conn, true)?;
    let result = apply_page_inner(conn, page);
    local::set_applying(conn, false)?;
    result
}

fn apply_page_inner(conn: &Connection, page: &PullPage) -> Result<ApplyStats, rusqlite::Error> {
    // Parked rows first, then the page; a newer copy of a parked row wins.
    let mut pending: Vec<RemoteChange> = parked(conn)?;
    for change in &page.changes {
        pending.retain(|p| !(p.entity == change.entity && p.gid == change.gid));
        pending.push(change.clone());
    }
    let mut stats = ApplyStats::default();
    let mut touched_items = Vec::new();

    // Dependency order; a stable sort keeps a row's own versions in order.
    pending.sort_by_key(|c| spec::rank(&c.entity));
    // Unresolved rows get another chance once the rest of the batch is in.
    for pass in 0..3 {
        let mut waiting = Vec::new();
        let mut progress = false;
        for change in pending {
            match apply_change(conn, &change) {
                Ok(Applied::KeptLocal) => {
                    stats.kept_local += 1;
                    unpark(conn, &change)?;
                }
                Ok(applied) => {
                    stats.applied += 1;
                    progress = true;
                    unpark(conn, &change)?;
                    if change.entity == spec::ITEMS {
                        if let Applied::Inserted(id) | Applied::Updated(id) = applied {
                            touched_items.push(id);
                        }
                    }
                }
                Err(ApplyError::Sql(e)) => return Err(e),
                Err(error) => waiting.push((change, error)),
            }
        }
        if waiting.is_empty() || (!progress && pass > 0) || pass == 2 {
            for (change, error) in &waiting {
                park(conn, change, &error.to_string())?;
            }
            stats.parked = waiting.len();
            break;
        }
        pending = waiting.into_iter().map(|(change, _)| change).collect();
    }

    let mut upsert = conn.prepare(
        "INSERT INTO sync_central_stock (item_gid, qty) VALUES (?1, ?2)
         ON CONFLICT (item_gid) DO UPDATE SET qty = excluded.qty",
    )?;
    for (gid, qty) in &page.stock {
        upsert.execute(params![gid, qty])?;
        if let Some(id) = codec::local_id(conn, spec::ITEMS, gid)? {
            touched_items.push(id);
        }
    }
    touched_items.sort_unstable();
    touched_items.dedup();
    for id in touched_items {
        local::recompute_stock(conn, id)?;
    }

    if !page.terminals.is_empty() {
        conn.execute("DELETE FROM sync_terminals", [])?;
        let mut insert = conn.prepare(
            "INSERT INTO sync_terminals (code, name, install_id, active, last_seen_at, seq) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        // The server lists them in the order they joined, disconnected ones
        // included, so a counter's number never changes.
        for (seq, t) in page.terminals.iter().enumerate() {
            insert.execute(params![t.code, t.name, t.install_id, t.active as i64, t.last_seen_at, seq as i64 + 1])?;
        }
    }
    if let Some(business) = &page.business {
        conn.execute("UPDATE sync_state SET business_name = ?1 WHERE id = 1", params![business.name])?;
    }
    conn.execute("UPDATE sync_state SET cursor = ?1 WHERE id = 1", params![page.cursor])?;
    Ok(stats)
}

fn apply_change(conn: &Connection, change: &RemoteChange) -> Result<Applied, ApplyError> {
    let Some(table) = spec::find(&change.entity) else {
        return Ok(Applied::Ignored); // a table a newer counter shares: not ours yet
    };
    if change.deleted {
        return codec::apply_delete(conn, table, &change.gid);
    }
    let empty = Map::new();
    codec::apply_row(conn, table, &change.gid, change.data.as_ref().unwrap_or(&empty))
}

fn parked(conn: &Connection) -> Result<Vec<RemoteChange>, rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT entity, gid, data, deleted FROM sync_parked ORDER BY first_seen")?;
    let rows = stmt.query_map([], |row| {
        let data: Option<String> = row.get(2)?;
        Ok(RemoteChange {
            entity: row.get(0)?,
            gid: row.get(1)?,
            deleted: row.get(3)?,
            data: data.and_then(|d| serde_json::from_str(&d).ok()),
        })
    })?;
    rows.collect()
}

fn park(conn: &Connection, change: &RemoteChange, reason: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO sync_parked (entity, gid, data, deleted, reason) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (entity, gid) DO UPDATE SET data = excluded.data, deleted = excluded.deleted,
                                                reason = excluded.reason, attempts = attempts + 1",
        params![
            change.entity,
            change.gid,
            change.data.as_ref().map(|d| Value::Object(d.clone()).to_string()),
            change.deleted,
            reason
        ],
    )?;
    Ok(())
}

fn unpark(conn: &Connection, change: &RemoteChange) -> Result<(), rusqlite::Error> {
    conn.execute("DELETE FROM sync_parked WHERE entity = ?1 AND gid = ?2", params![change.entity, change.gid])?;
    Ok(())
}

/// This counter's existing data becomes the business's first data: every
/// shared row and every item's stock (as an opening balance) is queued.
/// Call inside a transaction, after the server granted the seed.
pub fn start_seed(conn: &Connection) -> Result<i64, rusqlite::Error> {
    local::assign_missing_gids(conn)?;
    conn.execute("DELETE FROM sync_outbox", [])?;
    for table in TABLES {
        conn.execute(
            &format!("INSERT INTO sync_outbox (kind, entity, row_id) SELECT 'row', ?1, {pk} FROM {t} ORDER BY {pk}", pk = table.pk, t = table.name),
            params![table.name],
        )?;
    }
    conn.execute(
        "INSERT INTO sync_outbox (kind, entity, row_id, qty, tag)
         SELECT 'stock', 'items', id, round(stock_qty, 4), 'open:' || gid FROM items WHERE stock_qty <> 0",
        [],
    )?;
    conn.execute(
        "UPDATE sync_state SET mode = 'connected', seed_pending = 1, cursor = 0, media_queued = 1, pins_sealed = 1, connected_at = datetime('now', 'localtime') WHERE id = 1",
        [],
    )?;
    pending_count(conn)
}

/// A counter about to register gets a new install id, and its own rows
/// (which no server has seen under this database's current connection) are
/// renamed to match. A copied database therefore never registers with its
/// original's id, and the id the server records is the one the counter's
/// records will carry. Only for a counter that is not connected (call
/// inside a transaction).
pub fn fresh_identity(conn: &Connection) -> Result<String, rusqlite::Error> {
    let old = local::install_id(conn)?;
    let new = local::random_hex16();
    for table in TABLES.iter().filter(|t| t.name != spec::APP_CONFIG && t.name != spec::ENABLED_MODULES) {
        conn.execute(
            &format!("UPDATE {t} SET gid = ?1 || substr(gid, 17) WHERE substr(gid, 1, 17) = ?2 || ':'", t = table.name),
            params![new, old],
        )?;
    }
    conn.execute("UPDATE sync_state SET install_id = ?1 WHERE id = 1", params![new])?;
    Ok(new)
}

/// Holds one downloaded page until the whole business has arrived.
pub fn stage_page(conn: &Connection, page: &PullPage) -> Result<(), rusqlite::Error> {
    let mut insert = conn.prepare("INSERT INTO sync_staging (entity, gid, data, deleted) VALUES (?1, ?2, ?3, ?4)")?;
    for change in &page.changes {
        insert.execute(params![
            change.entity,
            change.gid,
            change.data.as_ref().map(|d| Value::Object(d.clone()).to_string()),
            change.deleted
        ])?;
    }
    let mut stock = conn.prepare(
        "INSERT INTO sync_staging_stock (item_gid, qty) VALUES (?1, ?2) ON CONFLICT (item_gid) DO UPDATE SET qty = excluded.qty",
    )?;
    for (gid, qty) in &page.stock {
        stock.execute(params![gid, qty])?;
    }
    Ok(())
}

pub fn clear_staging(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch("DELETE FROM sync_staging; DELETE FROM sync_staging_stock;")
}

#[derive(Debug, thiserror::Error)]
pub enum ReplaceError {
    #[error("The business has no staff accounts yet, so nobody could sign in here. Nothing was changed.")]
    NoUsers,
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
}

/// Replaces this counter's shared data with the downloaded business data,
/// in one transaction (call inside one). The install id stays: it is the
/// one the server registered (see `fresh_identity`), and the local ids it
/// prefixes only ever grow, so no gid minted before can come back.
pub fn replace_with_staged(conn: &Connection, last: &PullPage) -> Result<ApplyStats, ReplaceError> {
    local::set_applying(conn, true)?;
    conn.execute_batch("PRAGMA defer_foreign_keys = ON")?;
    for table in TABLES.iter().rev() {
        if table.name != spec::APP_CONFIG && table.name != spec::ENABLED_MODULES {
            conn.execute(&format!("DELETE FROM {}", table.name), [])?;
        }
    }
    conn.execute_batch(
        "DELETE FROM sync_outbox; DELETE FROM sync_parked; DELETE FROM sync_central_stock;
         DELETE FROM sync_reservation_lines; DELETE FROM sync_reservations;",
    )?;
    conn.execute(
        "UPDATE sync_state SET mode = 'connected', seed_pending = 0, cursor = 0, media_queued = 1, pins_sealed = 1,
                               connected_at = datetime('now', 'localtime') WHERE id = 1",
        [],
    )?;

    let mut staged = Vec::new();
    {
        let mut stmt = conn.prepare("SELECT entity, gid, data, deleted FROM sync_staging ORDER BY seq")?;
        let rows = stmt.query_map([], |row| {
            let data: Option<String> = row.get(2)?;
            Ok(RemoteChange { entity: row.get(0)?, gid: row.get(1)?, deleted: row.get(3)?, data: data.and_then(|d| serde_json::from_str(&d).ok()) })
        })?;
        for row in rows {
            staged.push(row?);
        }
    }
    let mut stock = BTreeMap::new();
    {
        let mut stmt = conn.prepare("SELECT item_gid, qty FROM sync_staging_stock")?;
        for row in stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?)))? {
            let (gid, qty) = row?;
            stock.insert(gid, qty);
        }
    }
    let page = PullPage {
        changes: staged,
        stock,
        cursor: last.cursor,
        more: false,
        terminals: last.terminals.clone(),
        business: last.business.clone(),
    };
    let stats = apply_page_inner(conn, &page)?;
    local::set_applying(conn, false)?;
    clear_staging(conn)?;

    let users: i64 = conn.query_row("SELECT COUNT(*) FROM users WHERE is_active = 1", [], |row| row.get(0))?;
    if users == 0 {
        return Err(ReplaceError::NoUsers);
    }
    Ok(stats)
}

/// Turns the result of a stock reservation into local state before the
/// sale is recorded (call inside a transaction). `taken` lines are already
/// out of the central balance: the item's local stock becomes "central
/// before this sale", so the sale's own decrement lands on the right
/// figure, and the sale's tagged deltas are not counted twice afterwards.
pub fn confirm_reservation(
    conn: &Connection,
    key: &str,
    taken: &BTreeMap<String, f64>,
    balances: &BTreeMap<String, f64>,
) -> Result<(), rusqlite::Error> {
    local::set_applying(conn, true)?;
    conn.execute("UPDATE sync_reservations SET status = 'confirmed' WHERE key = ?1", params![key])?;
    for (gid, qty) in taken {
        conn.execute(
            "INSERT OR REPLACE INTO sync_reservation_lines (key, item_gid, qty) VALUES (?1, ?2, ?3)",
            params![key, gid, qty],
        )?;
        let Some(balance) = balances.get(gid) else { continue };
        conn.execute(
            "INSERT INTO sync_central_stock (item_gid, qty) VALUES (?1, ?2) ON CONFLICT (item_gid) DO UPDATE SET qty = excluded.qty",
            params![gid, balance],
        )?;
        if let Some(id) = codec::local_id(conn, spec::ITEMS, gid)? {
            let pending = local::pending_stock(conn, id)?;
            conn.execute("UPDATE items SET stock_qty = ?1 WHERE id = ?2", params![balance + qty + pending, id])?;
        }
    }
    local::set_applying(conn, false)
}

/// The central balances a refused reservation reported, so this counter
/// shows what is really left.
pub fn refresh_balances(conn: &Connection, balances: &BTreeMap<String, f64>) -> Result<(), rusqlite::Error> {
    local::set_applying(conn, true)?;
    for (gid, qty) in balances {
        conn.execute(
            "INSERT INTO sync_central_stock (item_gid, qty) VALUES (?1, ?2) ON CONFLICT (item_gid) DO UPDATE SET qty = excluded.qty",
            params![gid, qty],
        )?;
        if let Some(id) = codec::local_id(conn, spec::ITEMS, gid)? {
            local::recompute_stock(conn, id)?;
        }
    }
    local::set_applying(conn, false)
}

/// A reservation whose sale never got recorded (the sale failed, or the app
/// stopped in between): its stock goes back to the business.
pub fn release_reservation(conn: &Connection, key: &str) -> Result<(), rusqlite::Error> {
    let status: Option<String> =
        conn.query_row("SELECT status FROM sync_reservations WHERE key = ?1", params![key], |row| row.get(0)).optional()?;
    if matches!(status.as_deref(), Some("confirmed") | Some("pending")) {
        conn.execute("INSERT INTO sync_outbox (kind, tag) VALUES ('release', ?1)", params![key])?;
        conn.execute("UPDATE sync_reservations SET status = 'failed' WHERE key = ?1", params![key])?;
    }
    Ok(())
}

/// Reservations left behind by a crash between taking stock and recording
/// the sale (anything not `used` after two minutes) are given back.
pub fn sweep_stale_reservations(conn: &Connection) -> Result<(), rusqlite::Error> {
    let keys: Vec<String> = conn
        .prepare(
            "SELECT key FROM sync_reservations
              WHERE status IN ('confirmed', 'pending') AND created_at < datetime('now', 'localtime', '-2 minutes')",
        )?
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    for key in keys {
        release_reservation(conn, &key)?;
    }
    // Finished reservations are only needed while their deltas wait to be sent.
    conn.execute(
        "DELETE FROM sync_reservations WHERE status IN ('used', 'failed', 'offline')
            AND created_at < datetime('now', 'localtime', '-2 days')
            AND NOT EXISTS (SELECT 1 FROM sync_outbox o WHERE o.tag = sync_reservations.key)",
        [],
    )?;
    Ok(())
}

/// The (item gid, qty) lines of a cart, merged per item, for a reservation.
pub fn reservation_lines(conn: &Connection, lines: &[(i64, f64)]) -> Result<Vec<(String, f64)>, rusqlite::Error> {
    let mut merged: BTreeMap<String, f64> = BTreeMap::new();
    for (item_id, qty) in lines {
        if let Some(gid) = conn
            .query_row("SELECT gid FROM items WHERE id = ?1", params![item_id], |row| row.get::<_, Option<String>>(0))
            .optional()?
            .flatten()
        {
            *merged.entry(gid).or_default() += qty;
        }
    }
    Ok(merged.into_iter().collect())
}
