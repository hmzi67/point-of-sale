//! Two (or three) real counter databases kept in step through a stand-in
//! server that follows the same rules as `Diwan\Workspace\SyncEngine`: last
//! write per field of each (entity, gid) wins, versions in order, stock moves only by
//! keyed changes (a replayed key is a no-op), and reservations take stock
//! all-or-nothing. The real server is covered by its own tests and by
//! `live_two_counters` below.

use std::collections::{BTreeMap, HashMap, HashSet};

use rusqlite::{params, Connection};
use serde_json::{Map, Value};

use super::engine::{self, PullPage, PushResponse, RemoteChange, Rejected};
use super::{local, runtime};
use crate::db::sales::{self, CartLine, CreateSaleInput};
use crate::db::schema::test_conn;

type StoredRow = (Option<Map<String, Value>>, i64);

#[derive(Default)]
struct Server {
    version: i64,
    /// (entity, gid) → (data, or None for a tombstone; version)
    rows: BTreeMap<(String, String), StoredRow>,
    stock: BTreeMap<String, (f64, i64)>,
    ledger: HashSet<(String, String)>,
    reservations: HashMap<String, BTreeMap<String, f64>>,
    released: HashSet<String>,
}

impl Server {
    fn push(&mut self, body: &Value) -> PushResponse {
        let mut response = PushResponse::default();
        for change in body["changes"].as_array().unwrap() {
            let id = change["id"].as_i64().unwrap();
            let entity = change["entity"].as_str().unwrap().to_string();
            if !super::spec::TABLES.iter().any(|t| t.name == entity) {
                response.rejected.push(Rejected { id, error: "Unknown record type.".into() });
                continue;
            }
            self.version += 1;
            let key = (entity, change["gid"].as_str().unwrap().to_string());
            // Like the real server: a change lists the fields it edited and
            // replaces only those; without a list it replaces what it carries.
            let data = (change["op"] == "upsert").then(|| {
                let incoming = change["data"].as_object().unwrap();
                let mut merged = match self.rows.get(&key) {
                    Some((Some(current), _)) => current.clone(),
                    _ => Map::new(),
                };
                let fields: Vec<String> = match change.get("fields").and_then(Value::as_array) {
                    Some(list) => list.iter().map(|f| f.as_str().unwrap().to_string()).collect(),
                    None => incoming.keys().cloned().collect(),
                };
                for field in fields {
                    if let Some(value) = incoming.get(&field) {
                        merged.insert(field, value.clone());
                    }
                }
                merged
            });
            self.rows.insert(key, (data, self.version));
            response.accepted.push(id);
        }
        for entry in body["stock"].as_array().unwrap() {
            let key = entry["key"].as_str().unwrap().to_string();
            let gid = entry["item_gid"].as_str().unwrap().to_string();
            if self.ledger.insert((key.clone(), gid.clone())) {
                self.version += 1;
                let balance = self.stock.get(&gid).map_or(0.0, |s| s.0) + entry["qty"].as_f64().unwrap();
                self.stock.insert(gid, (balance, self.version));
            }
            response.stock_accepted.push(key);
        }
        for key in body["releases"].as_array().unwrap() {
            let key = key.as_str().unwrap().to_string();
            if let Some(lines) = self.reservations.get(&key).cloned() {
                if self.released.insert(key.clone()) {
                    for (gid, qty) in lines {
                        self.version += 1;
                        let balance = self.stock[&gid].0 + qty;
                        self.stock.insert(gid, (balance, self.version));
                    }
                }
            }
            response.releases_accepted.push(key);
        }
        response.stock_accepted.dedup();
        response
    }

    fn pull(&self, since: i64, limit: usize) -> PullPage {
        let mut rows: Vec<_> = self.rows.iter().filter(|(_, (_, v))| *v > since).collect();
        rows.sort_by_key(|(_, (_, v))| *v);
        rows.truncate(limit);
        let cursor = if rows.len() == limit { (rows.last().unwrap().1).1 } else { self.version };
        PullPage {
            changes: rows
                .into_iter()
                .map(|((entity, gid), (data, _))| RemoteChange { entity: entity.clone(), gid: gid.clone(), deleted: data.is_none(), data: data.clone() })
                .collect(),
            stock: self.stock.iter().filter(|(_, (_, v))| *v > since && *v <= cursor).map(|(g, (q, _))| (g.clone(), *q)).collect(),
            cursor,
            more: cursor < self.version,
            terminals: vec![],
            business: None,
        }
    }

    /// `None` = insufficient.
    fn reserve(&mut self, key: &str, lines: &[(String, f64)]) -> Option<(BTreeMap<String, f64>, BTreeMap<String, f64>)> {
        let known: Vec<_> = lines.iter().filter(|(g, _)| self.stock.contains_key(g)).cloned().collect();
        if known.iter().any(|(g, q)| self.stock[g].0 < *q) {
            return None;
        }
        let mut taken = BTreeMap::new();
        let mut balances = BTreeMap::new();
        for (gid, qty) in known {
            self.version += 1;
            let balance = self.stock[&gid].0 - qty;
            self.stock.insert(gid.clone(), (balance, self.version));
            self.ledger.insert((key.to_string(), gid.clone()));
            taken.insert(gid.clone(), qty);
            balances.insert(gid, balance);
        }
        self.reservations.insert(key.to_string(), taken.clone());
        Some((taken, balances))
    }

    fn balance(&self, gid: &str) -> f64 {
        self.stock[gid].0
    }
}

/// Push everything, then pull everything — one round of `runtime::round`.
fn sync(conn: &mut Connection, server: &mut Server) {
    loop {
        let plan = engine::build_push(conn, None).unwrap();
        if plan.is_empty() {
            break;
        }
        let response = server.push(&plan.body);
        let tx = conn.transaction().unwrap();
        engine::ack_push(&tx, &plan, &response).unwrap();
        tx.commit().unwrap();
    }
    conn.execute("UPDATE sync_state SET seed_pending = 0 WHERE id = 1", []).unwrap();
    loop {
        let since: i64 = conn.query_row("SELECT cursor FROM sync_state", [], |r| r.get(0)).unwrap();
        let page = server.pull(since, 50);
        let tx = conn.transaction().unwrap();
        engine::apply_page(&tx, &page).unwrap();
        tx.commit().unwrap();
        if !page.more {
            break;
        }
    }
}

/// The first counter: its data becomes the business data.
fn seed(server: &mut Server) -> Connection {
    let mut conn = test_conn();
    let tx = conn.transaction().unwrap();
    engine::start_seed(&tx).unwrap();
    tx.commit().unwrap();
    sync(&mut conn, server);
    conn
}

/// Another counter: downloads the business and replaces its own data.
fn join(server: &Server) -> Connection {
    let mut conn = test_conn();
    let mut since = 0;
    let last = loop {
        let page = server.pull(since, 50);
        engine::stage_page(&conn, &page).unwrap();
        since = page.cursor;
        if !page.more {
            break page;
        }
    };
    let tx = conn.transaction().unwrap();
    engine::replace_with_staged(&tx, &last).unwrap();
    tx.commit().unwrap();
    conn
}

fn add_item(conn: &Connection, name: &str, stock: f64) -> (i64, String) {
    conn.execute("INSERT INTO items (name, price_minor, stock_qty) VALUES (?1, 1000, ?2)", params![name, stock]).unwrap();
    let id = conn.last_insert_rowid();
    (id, conn.query_row("SELECT gid FROM items WHERE id = ?1", [id], |r| r.get(0)).unwrap())
}

fn item_id(conn: &Connection, gid: &str) -> i64 {
    conn.query_row("SELECT id FROM items WHERE gid = ?1", [gid], |r| r.get(0)).unwrap()
}

fn stock_of(conn: &Connection, gid: &str) -> f64 {
    conn.query_row("SELECT stock_qty FROM items WHERE gid = ?1", [gid], |r| r.get(0)).unwrap()
}

fn sell(conn: &mut Connection, item: i64, qty: f64, key: Option<&str>) -> i64 {
    let tx = conn.transaction().unwrap();
    let input = CreateSaleInput {
        items: vec![CartLine { item_id: item, qty, notes: None }],
        payment_method: "cash".into(),
        ..Default::default()
    };
    let sale = runtime::record_sale(&tx, key, |tx| sales::create_sale(tx, input)).unwrap();
    tx.commit().unwrap();
    sale.id
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

#[test]
fn scenario_a_a_second_counter_gets_the_same_products_stock_staff_and_settings() {
    let mut server = Server::default();
    let mut first = test_conn();
    let (_, cola) = add_item(&first, "Cola", 100.0);
    first.execute("UPDATE app_config SET tax_percent = 16, theme = 'light'", []).unwrap();
    crate::db::users::create(&first, "Sana", "4321", crate::db::users::Role::Cashier).unwrap();
    let tx = first.transaction().unwrap();
    engine::start_seed(&tx).unwrap();
    tx.commit().unwrap();
    sync(&mut first, &mut server);
    assert_eq!(server.balance(&cola), 100.0, "opening stock uploaded");

    let mut second = test_conn();
    second.execute("INSERT INTO items (name, price_minor) VALUES ('Only on the new till', 1)", []).unwrap();
    let local_owner_gid: String = second.query_row("SELECT gid FROM users WHERE role = 'owner'", [], |r| r.get(0)).unwrap();
    let mut since = 0;
    let last = loop {
        let page = server.pull(since, 25);
        engine::stage_page(&second, &page).unwrap();
        since = page.cursor;
        if !page.more {
            break page;
        }
    };
    let tx = second.transaction().unwrap();
    engine::replace_with_staged(&tx, &last).unwrap();
    tx.commit().unwrap();

    assert_eq!(stock_of(&second, &cola), 100.0);
    assert_eq!(count(&second, "SELECT COUNT(*) FROM items WHERE name = 'Only on the new till'"), 0, "its own data was replaced");
    assert_eq!(count(&second, "SELECT COUNT(*) FROM items"), count(&first, "SELECT COUNT(*) FROM items"));
    assert_eq!(count(&second, "SELECT COUNT(*) FROM categories"), count(&first, "SELECT COUNT(*) FROM categories"));
    let sana: String = second.query_row("SELECT role FROM users WHERE name = 'Sana'", [], |r| r.get(0)).unwrap();
    assert_eq!(sana, "cashier", "staff accounts are the business's");
    let sana_id: i64 = second.query_row("SELECT id FROM users WHERE name = 'Sana'", [], |r| r.get(0)).unwrap();
    crate::db::users::authenticate(&second, sana_id, "4321").expect("her PIN works on the other counter");
    assert_eq!(count(&second, &format!("SELECT COUNT(*) FROM users WHERE gid = '{local_owner_gid}'")), 0, "its own seeded owner is gone");
    let (tax, theme): (f64, String) = second.query_row("SELECT tax_percent, theme FROM app_config", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!((tax, theme.as_str()), (16.0, "dark"), "business settings shared, device settings not");
    assert_ne!(local::install_id(&second).unwrap(), local::install_id(&first).unwrap());

    // A product added afterwards on the second counter reaches the first.
    let (_, tea) = add_item(&second, "Tea", 30.0);
    sync(&mut second, &mut server);
    sync(&mut first, &mut server);
    assert_eq!(stock_of(&first, &tea), 30.0);
}

#[test]
fn scenario_b_and_c_sales_on_two_counters_at_once_end_at_92_everywhere() {
    let mut server = Server::default();
    let mut first = seed(&mut server);
    let (cola_here, cola) = add_item(&first, "Cola", 100.0);
    sync(&mut first, &mut server);
    let mut second = join(&server);

    // Both sell before either syncs (offline, or simply at the same moment).
    sell(&mut first, cola_here, 5.0, None);
    let cola_there = item_id(&second, &cola);
    sell(&mut second, cola_there, 3.0, None);
    assert_eq!((stock_of(&first, &cola), stock_of(&second, &cola)), (95.0, 97.0));

    sync(&mut first, &mut server);
    sync(&mut second, &mut server);
    sync(&mut first, &mut server);
    assert_eq!(server.balance(&cola), 92.0);
    assert_eq!((stock_of(&first, &cola), stock_of(&second, &cola)), (92.0, 92.0));
    let sales_first = count(&first, "SELECT COUNT(*) FROM sales");
    assert_eq!(sales_first, count(&second, "SELECT COUNT(*) FROM sales"), "both counters hold both sales");
    let total = "SELECT COUNT(*) FROM sale_items si JOIN items i ON i.id = si.item_id WHERE i.name = 'Cola'";
    assert_eq!((count(&first, total), count(&second, total)), (2, 2));

    // Syncing again changes nothing (no echo, no double count).
    sync(&mut second, &mut server);
    sync(&mut first, &mut server);
    assert_eq!((server.balance(&cola), stock_of(&first, &cola), stock_of(&second, &cola)), (92.0, 92.0, 92.0));
}

#[test]
fn a_reserved_sale_is_counted_once_locally_and_centrally() {
    let mut server = Server::default();
    let mut first = seed(&mut server);
    let (cola_here, cola) = add_item(&first, "Cola", 10.0);
    sync(&mut first, &mut server);
    let mut second = join(&server);

    // Counter 2 takes 4 centrally; counter 1 has not heard of it yet.
    let key = "sale:test:0001";
    second.execute("INSERT INTO sync_reservations (key, status) VALUES (?1, 'pending')", [key]).unwrap();
    let (taken, balances) = server.reserve(key, &[(cola.clone(), 4.0)]).unwrap();
    engine::confirm_reservation(&second, key, &taken, &balances).unwrap();
    let cola_there = item_id(&second, &cola);
    sell(&mut second, cola_there, 4.0, Some(key));
    assert_eq!(stock_of(&second, &cola), 6.0, "local stock right before the sale syncs");

    // Counter 1 still thinks there are 10, but the server says no to 7.
    assert!(server.reserve("sale:test:0002", &[(cola.clone(), 7.0)]).is_none());
    assert_eq!(server.balance(&cola), 6.0, "nothing taken by the refusal");

    sync(&mut second, &mut server);
    assert_eq!(server.balance(&cola), 6.0, "the sale's own stock change was not applied twice");
    assert_eq!(stock_of(&second, &cola), 6.0);
    sync(&mut first, &mut server);
    assert_eq!(stock_of(&first, &cola), 6.0);
    let _ = cola_here;
}

#[test]
fn a_reservation_whose_sale_failed_is_given_back() {
    let mut server = Server::default();
    let mut first = seed(&mut server);
    let (_, cola) = add_item(&first, "Cola", 10.0);
    sync(&mut first, &mut server);
    let key = "sale:test:0003";
    first.execute("INSERT INTO sync_reservations (key, status) VALUES (?1, 'pending')", [key]).unwrap();
    let (taken, balances) = server.reserve(key, &[(cola.clone(), 3.0)]).unwrap();
    engine::confirm_reservation(&first, key, &taken, &balances).unwrap();
    engine::release_reservation(&first, key).unwrap();
    sync(&mut first, &mut server);
    assert_eq!(server.balance(&cola), 10.0);
    assert_eq!(stock_of(&first, &cola), 10.0);
}

#[test]
fn an_edit_on_either_counter_reaches_the_other_and_the_last_to_arrive_wins() {
    let mut server = Server::default();
    let mut first = seed(&mut server);
    let (here, cola) = add_item(&first, "Cola", 1.0);
    sync(&mut first, &mut server);
    let mut second = join(&server);
    first.execute("UPDATE items SET price_minor = 1200 WHERE id = ?1", [here]).unwrap();
    second.execute("UPDATE items SET price_minor = 1300, name = 'Cola 1.5L' WHERE gid = ?1", [&cola]).unwrap();
    sync(&mut first, &mut server);
    sync(&mut second, &mut server); // arrives last
    sync(&mut first, &mut server);
    let read = |c: &Connection| -> (String, i64) {
        c.query_row("SELECT name, price_minor FROM items WHERE gid = ?1", [&cola], |r| Ok((r.get(0)?, r.get(1)?))).unwrap()
    };
    assert_eq!(read(&first), ("Cola 1.5L".to_string(), 1300));
    assert_eq!(read(&second), read(&first));
}

#[test]
fn a_record_that_arrives_before_what_it_points_at_waits_and_then_applies() {
    let mut server = Server::default();
    let mut first = seed(&mut server);
    let mut second = join(&server);
    first.execute("INSERT INTO categories (name) VALUES ('Desserts')", []).unwrap();
    let category = first.last_insert_rowid();
    first.execute("INSERT INTO items (name, price_minor, category_id) VALUES ('Kheer', 200, ?1)", [category]).unwrap();
    sync(&mut first, &mut server);
    // Rename the category afterwards: it now has a later version than the item.
    first.execute("UPDATE categories SET name = 'Sweets' WHERE id = ?1", [category]).unwrap();
    sync(&mut first, &mut server);

    // Download in pages of one: the item comes before its category.
    loop {
        let since: i64 = count(&second, "SELECT cursor FROM sync_state");
        let page = server.pull(since, 1);
        let tx = second.transaction().unwrap();
        engine::apply_page(&tx, &page).unwrap();
        tx.commit().unwrap();
        if !page.more {
            break;
        }
    }
    let (item, category): (String, String) = second
        .query_row("SELECT i.name, c.name FROM items i JOIN categories c ON c.id = i.category_id WHERE i.name = 'Kheer'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((item.as_str(), category.as_str()), ("Kheer", "Sweets"));
    assert_eq!(count(&second, "SELECT COUNT(*) FROM sync_parked"), 0, "nothing left waiting");
}

#[test]
fn a_duplicate_name_from_another_counter_waits_visibly_until_it_is_resolved() {
    let mut server = Server::default();
    let mut first = seed(&mut server);
    let mut second = join(&server);
    first.execute("INSERT INTO categories (name) VALUES ('Specials')", []).unwrap();
    second.execute("INSERT INTO categories (name) VALUES ('Specials')", []).unwrap();
    sync(&mut first, &mut server);
    sync(&mut second, &mut server);
    assert_eq!(count(&second, "SELECT COUNT(*) FROM sync_parked WHERE entity = 'categories'"), 1);
    let info = runtime::info(&second).unwrap();
    assert!(info.issues.iter().any(|i| i.kind == "waiting"), "shown to the person");

    second.execute("UPDATE categories SET name = 'Specials (till 2)' WHERE name = 'Specials'", []).unwrap();
    sync(&mut second, &mut server);
    sync(&mut first, &mut server);
    assert_eq!(count(&second, "SELECT COUNT(*) FROM sync_parked"), 0);
    for conn in [&first, &second] {
        assert_eq!(count(conn, "SELECT COUNT(*) FROM categories WHERE name IN ('Specials', 'Specials (till 2)')"), 2);
    }
}

#[test]
fn deleting_a_product_another_counter_has_sold_archives_it_instead() {
    let mut server = Server::default();
    let mut first = seed(&mut server);
    let (here, cola) = add_item(&first, "Cola", 5.0);
    sync(&mut first, &mut server);
    let mut second = join(&server);
    let cola_there = item_id(&second, &cola);
    sell(&mut second, cola_there, 1.0, None); // not synced yet
    first.execute("DELETE FROM items WHERE id = ?1", [here]).unwrap(); // never sold on counter 1
    sync(&mut first, &mut server);
    sync(&mut second, &mut server);
    assert_eq!(count(&second, &format!("SELECT is_active FROM items WHERE gid = '{cola}'")), 0, "archived, not deleted");
    sync(&mut second, &mut server); // sends the archived product back
    sync(&mut first, &mut server);
    assert_eq!(count(&first, &format!("SELECT COUNT(*) FROM items WHERE gid = '{cola}' AND is_active = 0")), 1, "and back on counter 1, archived");
    assert_eq!(
        count(&first, "SELECT COUNT(*) FROM sale_items si JOIN items i ON i.id = si.item_id WHERE i.name = 'Cola'"),
        1,
        "with its sale"
    );
}

#[test]
fn a_parked_table_order_opens_on_another_counter_with_the_right_items() {
    let mut server = Server::default();
    let mut first = seed(&mut server);
    let (tea, tea_gid) = add_item(&first, "Tea", 20.0);
    first.execute("INSERT INTO tables (name, seats) VALUES ('Garden 1', 4)", []).unwrap();
    let table = first.last_insert_rowid();
    sync(&mut first, &mut server);
    let mut second = join(&server);
    // Give the second counter different local ids for everything.
    second.execute("INSERT INTO categories (name) VALUES ('pad')", []).unwrap();

    let line = crate::db::tables::ParkedCartLine { item_id: tea, qty: 2.0 };
    crate::db::tables::attach_cart_to_table(&first, table, &[line], 0).unwrap();
    sync(&mut first, &mut server);
    sync(&mut second, &mut server);
    let table_there: i64 = second.query_row("SELECT id FROM tables WHERE name = 'Garden 1'", [], |r| r.get(0)).unwrap();
    let parked = crate::db::tables::get_parked_cart(&second, table_there).unwrap().expect("the order is there");
    assert_eq!(parked.items[0].item_id, item_id(&second, &tea_gid));
    assert_eq!(parked.items[0].qty, 2.0);
}

#[test]
fn a_change_the_server_refuses_is_reported_and_does_not_block_the_rest() {
    let mut server = Server::default();
    let mut first = seed(&mut server);
    first.execute("INSERT INTO sync_outbox (kind, entity, row_id) VALUES ('row', 'categories', 1)", []).unwrap();
    let mut plan = engine::build_push(&first, None).unwrap();
    // Pretend the server refused it.
    let id = plan.body["changes"][0]["id"].as_i64().unwrap();
    plan.body["changes"][0]["entity"] = Value::String("nonsense".into());
    let response = server.push(&plan.body);
    assert_eq!(response.rejected[0].id, id);
    engine::ack_push(&first, &plan, &response).unwrap();
    assert_eq!(engine::pending_count(&first).unwrap(), 0);
    assert_eq!(count(&first, "SELECT COUNT(*) FROM sync_issues WHERE kind = 'rejected'"), 1);
    sync(&mut first, &mut server);
}

#[test]
fn a_lost_reply_and_a_resend_never_apply_stock_twice() {
    let mut server = Server::default();
    let mut first = seed(&mut server);
    let (here, cola) = add_item(&first, "Cola", 50.0);
    sync(&mut first, &mut server);
    sell(&mut first, here, 2.0, None);
    let plan = engine::build_push(&first, None).unwrap();
    server.push(&plan.body); // the reply is lost
    sync(&mut first, &mut server); // so it is all sent again
    assert_eq!(server.balance(&cola), 48.0);
    assert_eq!(stock_of(&first, &cola), 48.0);
}

#[test]
fn reports_split_sales_by_counter_and_cashier_and_add_up() {
    let mut server = Server::default();
    let mut first = seed(&mut server);
    let (here, cola) = add_item(&first, "Cola", 100.0);
    sync(&mut first, &mut server);
    let mut second = join(&server);
    for (conn, code, name) in [(&first, "AAAA1111", "Counter 1"), (&second, "BBBB2222", "Counter 2")] {
        let install = local::install_id(conn).unwrap();
        for target in [&first, &second] {
            target
                .execute("INSERT OR REPLACE INTO sync_terminals (code, name, install_id) VALUES (?1, ?2, ?3)", params![code, name, install])
                .unwrap();
        }
    }
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    // Demo data includes today's sales on the first counter: measure from here.
    let before = crate::db::reports::get_counter_sales_summary(&first, &today, &today).unwrap();
    let (base_total, base_count) = before.counters.iter().find(|c| c.label == "Counter 1").map_or((0, 0), |c| (c.total_minor, c.transaction_count));
    sell(&mut first, here, 2.0, None);
    sell(&mut first, here, 1.0, None);
    let cola_there = item_id(&second, &cola);
    sell(&mut second, cola_there, 4.0, None);
    sync(&mut first, &mut server);
    sync(&mut second, &mut server);
    sync(&mut first, &mut server);

    for conn in [&first, &second] {
        let report = crate::db::reports::get_counter_sales_summary(conn, &today, &today).unwrap();
        let by_name: HashMap<_, _> = report.counters.iter().map(|c| (c.label.as_str(), (c.total_minor, c.transaction_count))).collect();
        assert_eq!(by_name["Counter 1"], (base_total + 3000, base_count + 2));
        assert_eq!(by_name["Counter 2"], (4000, 1));
        let summary = crate::db::reports::get_sales_summary(conn, &today, &today).unwrap();
        assert_eq!(report.grand_total_minor, summary.total_sales_minor, "per-counter totals add up to the business total");
        assert_eq!(report.cashiers.iter().map(|c| c.transaction_count).sum::<i64>(), base_count + 3);
    }
}

// ---------------------------------------------------------------------------
// End to end against the real server: two counter databases on disk, the
// real `pos-*.php` endpoints over HTTP, the real MySQL behind them.
//
//   DIWAN_LICENSE_API_BASE=http://127.0.0.1:8099/api DIWAN_LIVE_LICENSE_KEY=DIWAN-… \
//     cargo test live_two_counters -- --ignored --nocapture
//
// The key must allow at least 3 devices. Run `php -S` with
// PHP_CLI_SERVER_WORKERS=4 so the concurrent sales really are concurrent.
// ---------------------------------------------------------------------------


#[test]
fn two_counters_editing_different_fields_of_one_product_both_keep_their_edit() {
    let mut server = Server::default();
    let mut first = seed(&mut server);
    let (here, cola) = add_item(&first, "Cola", 10.0);
    sync(&mut first, &mut server);
    let mut second = join(&server);

    // Both edit before either syncs: the price here, the name there.
    first.execute("UPDATE items SET price_minor = 1500 WHERE id = ?1", [here]).unwrap();
    second.execute("UPDATE items SET name = 'Cola Zero' WHERE gid = ?1", [&cola]).unwrap();
    sync(&mut first, &mut server);
    sync(&mut second, &mut server);
    sync(&mut first, &mut server);
    for conn in [&first, &second] {
        let (name, price): (String, i64) =
            conn.query_row("SELECT name, price_minor FROM items WHERE gid = ?1", [&cola], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((name.as_str(), price), ("Cola Zero", 1500), "both edits survive on every counter");
    }
}

#[test]
fn an_update_queues_the_fields_it_changed_and_a_no_op_update_queues_nothing() {
    let mut server = Server::default();
    let mut conn = seed(&mut server);
    let (id, _) = add_item(&conn, "Tea", 5.0);
    sync(&mut conn, &mut server);
    conn.execute("UPDATE items SET price_minor = price_minor WHERE id = ?1", [id]).unwrap();
    assert_eq!(engine::pending_count(&conn).unwrap(), 0, "nothing changed, nothing to send");

    conn.execute("UPDATE items SET price_minor = 2000, name = 'Green tea' WHERE id = ?1", [id]).unwrap();
    conn.execute("UPDATE items SET description = 'Hot' WHERE id = ?1", [id]).unwrap();
    let plan = engine::build_push(&conn, None).unwrap();
    let change = &plan.body["changes"][0];
    assert_eq!(change["fields"], serde_json::json!(["description", "name", "price_minor"]));
    let cursor: i64 = conn.query_row("SELECT cursor FROM sync_state", [], |r| r.get(0)).unwrap();
    assert_eq!(change["base"], cursor, "the cursor the edits were made at");
}

#[test]
fn a_new_product_is_sent_whole() {
    let mut server = Server::default();
    let conn = seed(&mut server);
    add_item(&conn, "Coffee", 5.0);
    conn.execute("UPDATE items SET price_minor = 999 WHERE name = 'Coffee'", []).unwrap();
    let plan = engine::build_push(&conn, None).unwrap();
    let change = plan.body["changes"].as_array().unwrap().iter().find(|c| c["data"]["name"] == "Coffee").unwrap();
    assert!(change.get("fields").is_none(), "an insert among the queued entries sends the whole row");
}

/// The business's media store, as the stand-in server keeps it.
type MediaStore = BTreeMap<String, Vec<u8>>;

/// What `runtime::upload_media` does, against the stand-in store.
fn upload_photos(conn: &Connection, dirs: &super::media::MediaDirs, store: &mut MediaStore) {
    for file in super::media::to_upload(conn, dirs).unwrap() {
        store.entry(file.name.clone()).or_insert_with(|| super::media::upload_bytes(&file).unwrap());
        super::media::mark_on_server(conn, &file.name).unwrap();
    }
}

/// What `runtime::download_media` does, against the stand-in store.
fn download_photos(conn: &Connection, dirs: &super::media::MediaDirs, store: &MediaStore) -> usize {
    let mut arrived = 0;
    for file in super::media::to_download(conn, dirs).unwrap() {
        match store.get(&file.name) {
            Some(bytes) => {
                crate::images::write_atomically(&file.path, bytes).unwrap();
                super::media::mark_on_server(conn, &file.name).unwrap();
                arrived += 1;
            }
            None => super::media::mark_unavailable(conn, &file.name).unwrap(),
        }
    }
    arrived
}

fn media_dirs(label: &str) -> super::media::MediaDirs {
    let root = std::env::temp_dir().join(format!("pos-ws-media-{label}-{}", local::random_hex16()));
    super::media::MediaDirs::new(&root)
}

#[test]
fn a_photo_and_the_logo_set_on_one_counter_appear_on_the_other() {
    use base64::Engine;
    let mut server = Server::default();
    let mut store = MediaStore::new();
    let (dirs_one, dirs_two) = (media_dirs("one"), media_dirs("two"));
    let mut first = seed(&mut server);
    let (here, cola) = add_item(&first, "Cola", 10.0);
    sync(&mut first, &mut server);
    let mut second = join(&server);

    let photo = image::RgbImage::from_pixel(40, 30, image::Rgb([10, 120, 200]));
    let mut png = std::io::Cursor::new(Vec::new());
    photo.write_to(&mut png, image::ImageFormat::Png).unwrap();
    let encoded = base64::engine::general_purpose::STANDARD.encode(png.get_ref());
    let name = crate::images::save_image(&dirs_one.items, &encoded, "png").unwrap();
    let logo = crate::images::save_logo(&dirs_one.logo, &encoded, "png").unwrap();
    first.execute("UPDATE items SET image_path = ?1 WHERE id = ?2", params![name, here]).unwrap();
    first.execute("UPDATE app_config SET logo_path = ?1", [&logo]).unwrap();

    // Counter 1: files first, then records. Counter 2: records, then files.
    upload_photos(&first, &dirs_one, &mut store);
    sync(&mut first, &mut server);
    sync(&mut second, &mut server);
    assert_eq!(download_photos(&second, &dirs_two, &store), 2);

    let there: Option<String> = second.query_row("SELECT image_path FROM items WHERE gid = ?1", [&cola], |r| r.get(0)).unwrap();
    assert_eq!(there.as_deref(), Some(name.as_str()), "the record names the same file");
    assert_eq!(std::fs::read(dirs_two.items.join(&name)).unwrap(), std::fs::read(dirs_one.items.join(&name)).unwrap());
    assert!(dirs_two.logo.join(&logo).is_file(), "the logo too, in the logo folder");
    assert!(super::media::to_upload(&second, &dirs_two).unwrap().is_empty(), "a downloaded file is not sent back");
    assert!(super::media::to_download(&second, &dirs_two).unwrap().is_empty(), "nor fetched again");
}

fn with_licence(conn: &Connection, key: &str) {
    conn.execute(
        "INSERT OR REPLACE INTO license_status (id, license_key, payload_b64, signature_b64, kid, server_issued_at, high_water_local_ts, last_online_check)
         VALUES (1, ?1, '', '', 'k', 0, 0, 0)",
        [key],
    )
    .unwrap();
}

#[test]
fn staff_pins_reach_the_server_sealed_and_still_work_on_every_counter() {
    let mut server = Server::default();
    let mut first = test_conn();
    with_licence(&first, "DIWAN-SAME-KEY");
    let sana = crate::db::users::create(&first, "Sana", "4321", crate::db::users::Role::Cashier).unwrap();
    let tx = first.transaction().unwrap();
    engine::start_seed(&tx).unwrap();
    tx.commit().unwrap();
    sync(&mut first, &mut server);

    let gid: String = first.query_row("SELECT gid FROM users WHERE id = ?1", [sana.id], |r| r.get(0)).unwrap();
    let stored = server.rows[&("users".to_string(), gid.clone())].0.clone().unwrap();
    let on_server = stored["pin_hash"].as_str().unwrap();
    assert!(on_server.starts_with("enc1:") && !on_server.contains("argon2"), "the server holds no PIN hash: {on_server}");
    let area = server.rows[&("app_config".to_string(), "config".to_string())].0.clone().unwrap();
    assert!(area["sensitive_area_pin_hash"].as_str().is_none_or(|v| v.starts_with("enc1:")), "nor the area PIN's");

    let mut second = test_conn();
    with_licence(&second, "diwan-same-key");
    let mut since = 0;
    let last = loop {
        let page = server.pull(since, 50);
        engine::stage_page(&second, &page).unwrap();
        since = page.cursor;
        if !page.more {
            break page;
        }
    };
    let tx = second.transaction().unwrap();
    engine::replace_with_staged(&tx, &last).unwrap();
    tx.commit().unwrap();
    let there: i64 = second.query_row("SELECT id FROM users WHERE gid = ?1", [&gid], |r| r.get(0)).unwrap();
    crate::db::users::authenticate(&second, there, "4321").expect("her PIN works on the other counter");

    // A counter holding a different licence key keeps its own hash and sends it back.
    with_licence(&second, "DIWAN-NEW-KEY");
    first.execute("UPDATE users SET pin_hash = pin_hash || '' , name = 'Sana K' WHERE id = ?1", [sana.id]).unwrap();
    sync(&mut first, &mut server);
    sync(&mut second, &mut server);
    crate::db::users::authenticate(&second, there, "4321").expect("still signs in with the hash it had");
    let name: String = second.query_row("SELECT name FROM users WHERE id = ?1", [there], |r| r.get(0)).unwrap();
    assert_eq!(name, "Sana K", "the rest of the record still applies");
    sync(&mut second, &mut server);
    let resealed = server.rows[&("users".to_string(), gid)].0.clone().unwrap();
    assert_eq!(
        PinsProbe::key_id(resealed["pin_hash"].as_str().unwrap()),
        PinsProbe::key_id(&super::pins::PinKey::from_license_key("DIWAN-NEW-KEY").seal("x")),
        "re-sealed with the current key"
    );
}

struct PinsProbe;
impl PinsProbe {
    fn key_id(sealed: &str) -> String {
        sealed.split(':').nth(1).unwrap().to_string()
    }
}

#[test]
fn a_counter_connected_before_photo_sync_sends_its_photo_names_once_as_that_field_only() {
    let mut server = Server::default();
    let mut first = seed(&mut server);
    let (here, cola) = add_item(&first, "Cola", 10.0);
    sync(&mut first, &mut server);
    // As an older build left it: a photo that was never sent.
    local::set_applying(&first, true).unwrap();
    first.execute("UPDATE items SET image_path = 'item-legacy-1.jpg' WHERE id = ?1", [here]).unwrap();
    local::set_applying(&first, false).unwrap();
    first.execute("UPDATE sync_state SET media_queued = 0", []).unwrap();

    local::install(&first).unwrap();
    local::install(&first).unwrap(); // a second launch queues nothing more
    let plan = engine::build_push(&first, None).unwrap();
    let changes = plan.body["changes"].as_array().unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0]["fields"], serde_json::json!(["image_path"]), "only the photo, never a stale copy of the rest");
    sync(&mut first, &mut server);
    let second = join(&server);
    let there: Option<String> = second.query_row("SELECT image_path FROM items WHERE gid = ?1", [&cola], |r| r.get(0)).unwrap();
    assert_eq!(there.as_deref(), Some("item-legacy-1.jpg"));
}

mod live {
    use super::*;
    use crate::db::Db;
    use crate::workspace::client;

    fn temp_db(label: &str) -> (Db, std::path::PathBuf) {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("diwan-live-{label}-{}-{nanos}", std::process::id()));
        (Db::open(dir.clone()).unwrap(), dir)
    }

    fn fingerprint() -> String {
        format!("{}{}{}{}", local::random_hex16(), local::random_hex16(), local::random_hex16(), local::random_hex16())
    }

    async fn register(db: &Db, key: &str, fp: &str, name: &str) -> client::Registration {
        crate::license::client::activate(key, fp).await.expect("activate a device slot");
        // As `LicenseRuntime` keeps it after activating (PIN hashes are sealed with it).
        db.with_conn(|c| Ok(with_licence(c, key))).unwrap();
        // As `runtime::connect` does for a counter that is not connected yet.
        let install = db.with_transaction(|tx| engine::fresh_identity(tx)).unwrap();
        let reg = client::register(key, fp, &install, Some(name)).await.expect("register");
        db.with_conn(|c| {
            c.execute(
                "UPDATE sync_state SET token = ?1, mode = 'pending', business_name = ?2, terminal_name = ?3, terminal_code = ?4 WHERE id = 1",
                params![reg.token, reg.business.name, reg.terminal.name, reg.terminal.code],
            )
        })
        .unwrap();
        reg
    }

    async fn download(db: &Db, token: &str) {
        db.with_conn(engine::clear_staging).unwrap();
        let mut since = 0;
        let last = loop {
            let page = client::pull(token, since, 200).await.unwrap();
            db.with_transaction(|tx| engine::stage_page(tx, &page)).unwrap();
            since = page.cursor;
            if !page.more {
                break page;
            }
        };
        db.try_transaction(|tx| engine::replace_with_staged(tx, &last)).unwrap().unwrap();
        client::action(token, "joined", None).await.unwrap();
    }

    async fn sale(db: &Db, item_gid: &str, qty: f64, online: bool) -> Result<i64, String> {
        let item: i64 = db.with_conn(|c| c.query_row("SELECT id FROM items WHERE gid = ?1", [item_gid], |r| r.get(0))).unwrap();
        let key = if online { runtime::reserve_for_sale(db, &[(item, qty)]).await? } else { None };
        let input = CreateSaleInput { items: vec![CartLine { item_id: item, qty, notes: None }], payment_method: "cash".into(), ..Default::default() };
        db.try_transaction(|tx| runtime::record_sale(tx, key.as_deref(), |tx| sales::create_sale(tx, input)))
            .unwrap()
            .map(|s| s.id)
            .map_err(|e| e.to_string())
    }

    /// Runs two sales on two OS threads at the same time — two counters.
    fn at_once<A, B>(a: impl FnOnce() -> A + Send, b: impl FnOnce() -> B + Send) -> (A::Output, B::Output)
    where
        A: std::future::Future,
        B: std::future::Future,
        A::Output: Send,
        B::Output: Send,
    {
        std::thread::scope(|scope| {
            let a = scope.spawn(|| tauri::async_runtime::block_on(a()));
            let b = scope.spawn(|| tauri::async_runtime::block_on(b()));
            (a.join().unwrap(), b.join().unwrap())
        })
    }

    fn token(db: &Db) -> String {
        db.with_conn(|c| c.query_row("SELECT token FROM sync_state", [], |r| r.get(0))).unwrap()
    }

    fn stock(db: &Db, gid: &str) -> f64 {
        db.with_conn(|c| c.query_row("SELECT stock_qty FROM items WHERE gid = ?1", [gid], |r| r.get(0))).unwrap()
    }

    fn scalar(db: &Db, sql: &str) -> i64 {
        db.with_conn(|c| c.query_row(sql, [], |r| r.get(0))).unwrap()
    }

    #[test]
    #[ignore]
    fn live_two_counters() {
        let key = std::env::var("DIWAN_LIVE_LICENSE_KEY").expect("set DIWAN_LIVE_LICENSE_KEY");
        tauri::async_runtime::block_on(async move {
            // Counter 1 starts the business with what it already has.
            let (one, dir1) = temp_db("one");
            let fp1 = fingerprint();
            let reg1 = register(&one, &key, &fp1, "Front counter").await;
            assert!(!reg1.business.has_data, "a fresh business");
            let t1 = token(&one);
            client::action(&t1, "claim_seed", None).await.unwrap();
            one.with_transaction(|tx| engine::start_seed(tx)).unwrap();
            let (_, cola) = one.with_conn(|c| Ok(add_item(c, "Live Cola", 100.0))).unwrap();
            let (_, cake) = one.with_conn(|c| Ok(add_item(c, "Last Cake", 1.0))).unwrap();
            let sana = one.with_conn(|c| Ok(crate::db::users::create(c, "Live Sana", "4321", crate::db::users::Role::Cashier).unwrap())).unwrap();
            runtime::sync_with(&one, &t1).await.unwrap();
            println!("counter 1 seeded the business ({} records)", scalar(&one, "SELECT COUNT(*) FROM items"));

            // Scenario A: counter 2 joins and sees the same products and stock.
            let (two, dir2) = temp_db("two");
            let reg2 = register(&two, &key, &fingerprint(), "Takeaway counter").await;
            assert_eq!(reg2.business.id, reg1.business.id, "same business");
            assert!(reg2.business.has_data);
            let t2 = token(&two);
            download(&two, &t2).await;
            assert_eq!(stock(&two, &cola), 100.0);
            assert_eq!(scalar(&two, "SELECT COUNT(*) FROM items"), scalar(&one, "SELECT COUNT(*) FROM items"));
            println!("A: counter 2 holds the same {} products, Live Cola stock 100", scalar(&two, "SELECT COUNT(*) FROM items"));
            let page = client::pull(&t2, 0, 1000).await.unwrap();
            let hashes: Vec<String> = page
                .changes
                .iter()
                .filter(|c| c.entity == "users")
                .filter_map(|c| c.data.as_ref()?.get("pin_hash")?.as_str().map(str::to_string))
                .collect();
            assert!(!hashes.is_empty() && hashes.iter().all(|h| h.starts_with("enc1:")), "the server holds only sealed PIN hashes: {hashes:?}");
            let sana_gid: String = one.with_conn(|c| c.query_row("SELECT gid FROM users WHERE id = ?1", [sana.id], |r| r.get(0))).unwrap();
            let sana_there: i64 = two.with_conn(|c| c.query_row("SELECT id FROM users WHERE gid = ?1", [&sana_gid], |r| r.get(0))).unwrap();
            two.with_conn(|c| Ok(crate::db::users::authenticate(c, sana_there, "4321").expect("her PIN works on counter 2"))).unwrap();
            println!("Security: {} staff PIN hash(es) on the server, all sealed; Live Sana signs in on counter 2", hashes.len());

            // Scenario C: both counters sell Live Cola at the same moment.
            let (a, b) = at_once(|| sale(&one, &cola, 5.0, true), || sale(&two, &cola, 3.0, true));
            a.unwrap();
            b.unwrap();
            // And both try to sell the one Last Cake.
            let (a, b) = at_once(|| sale(&one, &cake, 1.0, true), || sale(&two, &cake, 1.0, true));
            assert!(a.is_ok() != b.is_ok(), "exactly one counter sold the last cake: {a:?} / {b:?}");
            println!("C: last cake — one sale ok, the other refused: {}", a.err().or(b.err()).unwrap());
            for _ in 0..2 {
                runtime::sync_with(&one, &t1).await.unwrap();
                runtime::sync_with(&two, &t2).await.unwrap();
            }
            assert_eq!((stock(&one, &cola), stock(&two, &cola)), (92.0, 92.0), "100 - 5 - 3 on both counters");
            assert_eq!((stock(&one, &cake), stock(&two, &cake)), (0.0, 0.0));
            println!("C: Live Cola is 92 on both counters");

            // Scenario B: each counter's records hold both sales, once.
            let sql = format!("SELECT COUNT(*) FROM sale_items si JOIN items i ON i.id = si.item_id WHERE i.gid = '{cola}'");
            assert_eq!((scalar(&one, &sql), scalar(&two, &sql)), (2, 2));
            println!("B: both Live Cola sales are on both counters");
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            for db in [&one, &two] {
                let report = db.with_conn(|c| Ok(crate::db::reports::get_counter_sales_summary(c, &today, &today).unwrap())).unwrap();
                let names: Vec<&str> = report.counters.iter().map(|c| c.label.as_str()).collect();
                assert!(names.contains(&"Front counter") && names.contains(&"Takeaway counter"), "sales attributed by counter: {names:?}");
            }
            println!("B: the counter report on both counters splits the sales by Front counter / Takeaway counter");

            // Photos: set on counter 1, the file reaches counter 2.
            use base64::Engine;
            let (dirs1, dirs2) = (super::super::media::MediaDirs::new(&dir1), super::super::media::MediaDirs::new(&dir2));
            let mut png = std::io::Cursor::new(Vec::new());
            image::RgbImage::from_pixel(64, 48, image::Rgb([200, 90, 10])).write_to(&mut png, image::ImageFormat::Png).unwrap();
            let photo = crate::images::save_image(&dirs1.items, &base64::engine::general_purpose::STANDARD.encode(png.get_ref()), "png").unwrap();
            one.with_conn(|c| c.execute("UPDATE items SET image_path = ?1 WHERE gid = ?2", params![photo, cola])).unwrap();
            runtime::upload_media(&one, &t1, &dirs1).await.unwrap();
            runtime::sync_with(&one, &t1).await.unwrap();
            runtime::sync_with(&two, &t2).await.unwrap();
            assert_eq!(runtime::download_media(&two, &t2, &dirs2).await, 1, "counter 2 downloads the photo");
            let arrived = image::open(dirs2.items.join(&photo)).expect("a real image arrived");
            assert_eq!((arrived.width(), arrived.height()), (64, 48));
            println!("Photos: {photo} set on counter 1 is on counter 2");

            // Different fields of one product, edited on both counters before either syncs.
            one.with_conn(|c| c.execute("UPDATE items SET price_minor = 2500 WHERE gid = ?1", [&cola])).unwrap();
            two.with_conn(|c| c.execute("UPDATE items SET name = 'Live Cola Zero' WHERE gid = ?1", [&cola])).unwrap();
            for _ in 0..2 {
                runtime::sync_with(&one, &t1).await.unwrap();
                runtime::sync_with(&two, &t2).await.unwrap();
            }
            for db in [&one, &two] {
                let (name, price): (String, i64) = db
                    .with_conn(|c| c.query_row("SELECT name, price_minor FROM items WHERE gid = ?1", [&cola], |r| Ok((r.get(0)?, r.get(1)?))))
                    .unwrap();
                assert_eq!((name.as_str(), price), ("Live Cola Zero", 2500), "both edits kept");
            }
            println!("Merge: a price edit on counter 1 and a name edit on counter 2 both survive");

            // Scenario G: counter 2 goes offline (no reservation) and sells 2
            // more that counter 1 already sold online.
            let (_, rice) = one.with_conn(|c| Ok(add_item(c, "Live Rice", 2.0))).unwrap();
            runtime::sync_with(&one, &t1).await.unwrap();
            runtime::sync_with(&two, &t2).await.unwrap();
            sale(&one, &rice, 2.0, true).await.unwrap();
            runtime::sync_with(&one, &t1).await.unwrap();
            sale(&two, &rice, 2.0, false).await.expect("offline sale against stale local stock");
            for _ in 0..2 {
                runtime::sync_with(&two, &t2).await.unwrap();
                runtime::sync_with(&one, &t1).await.unwrap();
            }
            assert_eq!((stock(&one, &rice), stock(&two, &rice)), (-2.0, -2.0), "the oversell is kept and visible, not overwritten");
            println!("G: offline oversell applied once and flagged: Live Rice is -2 on both counters");

            // Scenario D: counter 1 is reinstalled (fresh database, same device).
            let (again, dir3) = temp_db("again");
            let reg3 = register(&again, &key, &fp1, "Front counter").await;
            assert_eq!(reg3.business.id, reg1.business.id, "same business after a reinstall");
            assert!(client::push(&token(&again), &serde_json::json!({"changes": [], "stock": [], "releases": []})).await.unwrap_err().code() == Some("not_joined"),
                "a fresh database may not push before it has the business data");
            let t3 = token(&again);
            download(&again, &t3).await;
            assert_eq!(stock(&again, &cola), 92.0);
            assert_eq!(scalar(&again, &sql), 2);
            println!("D: the reinstalled counter got the same business back: Live Cola 92, both sales");
            assert!(client::pull(&t1, 0, 1).await.is_err(), "the old installation's token stopped working");

            for dir in [dir1, dir2, dir3] {
                let _ = std::fs::remove_dir_all(dir);
            }
        });
    }
}
