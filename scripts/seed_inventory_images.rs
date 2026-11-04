//! Fills a local install with a demo-ready inventory: adds the products and
//! categories in `inventory-catalog.json` that are missing, and gives every
//! record its photo through the app's own `images::save_image`.
//!
//!   cd src-tauri
//!   cargo run --example seed_inventory_images -- "<app data dir, the folder holding pos.db>"
//!
//! Close the app first. Safe to re-run: records are matched by name or barcode,
//! photos are named by content, and a record that already has a good photo keeps
//! it. Only a missing file or a photo in `IRRELEVANT_PHOTOS` is replaced. Exits
//! non-zero if any product or category still has no readable photo.

use std::collections::HashSet;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, GenericImageView};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tauri_app_lib::db::items::{self, ItemInput};
use tauri_app_lib::images;

/// Business logos that were uploaded as product photos on the dev install.
const IRRELEVANT_PHOTOS: &[&str] = &["item-5eac284671d318f2aa5bce9e405e58ce.jpg", "item-1792182241899495000-0.png"];
const PHOTO_SIDE: u32 = 800;
const JPEG_QUALITY: u8 = 85;

#[derive(Deserialize)]
struct Catalog {
    categories: Vec<CategoryEntry>,
    items: Vec<ItemEntry>,
}

#[derive(Deserialize)]
struct CategoryEntry {
    name: String,
    #[serde(default)]
    is_topping: bool,
    image: Photo,
}

/// `new` holds the fields for a product this script creates; an entry without
/// it only supplies a photo for a product that already exists.
#[derive(Deserialize)]
struct ItemEntry {
    name: String,
    barcode: Option<String>,
    image: Photo,
    new: Option<NewItem>,
}

#[derive(Deserialize)]
struct NewItem {
    category: String,
    description: String,
    /// Whole rupees, like the demo figures in `db/seed.rs`.
    price: i64,
    cost: i64,
    stock: f64,
    low_stock: i64,
    unit: Option<String>,
    toppings: Option<String>,
}

/// `page`, `license` and `author` are the attribution record, not read here.
#[derive(Deserialize)]
struct Photo {
    url: String,
}

fn main() {
    let data_dir = PathBuf::from(std::env::args().nth(1).expect("usage: seed_inventory_images <app data dir>"));
    let db_path = data_dir.join("pos.db");
    assert!(db_path.exists(), "no pos.db in {}", data_dir.display());
    let photos_dir = images::images_dir(&data_dir);

    let conn = Connection::open(&db_path).expect("open pos.db");
    conn.pragma_update(None, "foreign_keys", "ON").expect("foreign keys");
    conn.busy_timeout(std::time::Duration::from_secs(5)).expect("busy timeout");
    // Demo products must never reach a real business's other counters.
    let mode: String = conn.query_row("SELECT mode FROM sync_state WHERE id = 1", [], |r| r.get(0)).expect("sync_state");
    assert_ne!(mode, "connected", "this counter is connected to a business; refusing to add demo data");

    let catalog: Catalog = serde_json::from_str(include_str!("inventory-catalog.json")).expect("inventory-catalog.json");
    let mut failures = Vec::new();

    for entry in &catalog.categories {
        let found: Option<(i64, Option<String>)> = conn
            .query_row("SELECT id, image_path FROM categories WHERE name = ?1", [&entry.name], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()
            .expect("read category");
        let current = found.as_ref().and_then(|(_, path)| path.as_deref());
        if found.is_some() && !needs_photo(&photos_dir, current) {
            continue;
        }
        let photo = match store_photo(&entry.image, &photos_dir) {
            Ok(photo) => photo,
            Err(e) => {
                failures.push(format!("category {}: {e}", entry.name));
                continue;
            }
        };
        match found {
            Some((id, _)) => {
                conn.execute("UPDATE categories SET image_path = ?1 WHERE id = ?2", params![photo, id]).expect("set category photo");
            }
            None => {
                items::add_category(&conn, &entry.name, Some(&photo), entry.is_topping).map_err(|e| e.to_string()).expect("add category");
            }
        }
        println!("category {}: {photo}", entry.name);
    }

    for entry in &catalog.items {
        let found: Option<(i64, Option<String>)> = conn
            .query_row(
                "SELECT id, image_path FROM items WHERE name = ?1 OR (?2 IS NOT NULL AND barcode = ?2)",
                params![entry.name, entry.barcode],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .expect("read item");
        let current = found.as_ref().and_then(|(_, path)| path.as_deref());
        if found.is_some() && !needs_photo(&photos_dir, current) {
            continue;
        }
        let photo = match store_photo(&entry.image, &photos_dir) {
            Ok(photo) => photo,
            Err(e) => {
                failures.push(format!("item {}: {e}", entry.name));
                continue;
            }
        };
        match (found, &entry.new) {
            (Some((id, _)), _) => {
                conn.execute(
                    "UPDATE items SET image_path = ?1, updated_at = datetime('now', 'localtime') WHERE id = ?2",
                    params![photo, id],
                )
                .expect("set item photo");
            }
            (None, Some(new)) => add_item(&conn, entry, new, &photo),
            (None, None) => {
                failures.push(format!("item {}: not in this database and the catalog has no details to create it", entry.name));
                continue;
            }
        }
        println!("item {}: {photo}", entry.name);
    }

    failures.extend(verify(&conn, &photos_dir));
    if !failures.is_empty() {
        eprintln!("\nNOT DONE:");
        failures.iter().for_each(|failure| eprintln!("  {failure}"));
        std::process::exit(1);
    }
    let count = |table: &str| -> i64 { conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).expect("count") };
    println!("\nOK: {} categories and {} products, every one with a readable photo", count("categories"), count("items"));
}

fn needs_photo(photos_dir: &Path, current: Option<&str>) -> bool {
    match current {
        None => true,
        Some(name) => IRRELEVANT_PHOTOS.contains(&name) || !photos_dir.join(name).exists(),
    }
}

fn add_item(conn: &Connection, entry: &ItemEntry, new: &NewItem, photo: &str) {
    let category_id = |name: &str| -> Option<i64> {
        Some(conn.query_row("SELECT id FROM categories WHERE name = ?1", [name], |r| r.get(0)).expect("catalog names an unknown category"))
    };
    let input = ItemInput {
        name: entry.name.clone(),
        barcode: entry.barcode.clone(),
        short_code: None,
        description: Some(new.description.clone()),
        price_minor: new.price * 100,
        cost_minor: new.cost * 100,
        stock_qty: new.stock,
        category_id: category_id(&new.category),
        low_stock_threshold: new.low_stock,
        image_path: Some(photo.to_string()),
        sold_by_amount: new.unit.is_some(),
        unit: new.unit.clone(),
        counter_id: None,
        topping_category_id: new.toppings.as_deref().and_then(category_id),
        original_stock_qty: None,
    };
    items::add_item(conn, input).map_err(|e| e.to_string()).expect("add item");
}

/// Downloads (once), crops to a centred square and saves through the app's
/// upload path, returning the generated filename.
fn store_photo(photo: &Photo, photos_dir: &Path) -> Result<String, String> {
    let bytes = download(&photo.url)?;
    let image = image::load_from_memory(&bytes).map_err(|e| format!("{}: {e}", photo.url))?;
    let (width, height) = image.dimensions();
    let side = width.min(height);
    let square = image.crop_imm((width - side) / 2, (height - side) / 2, side, side);
    let square = if side > PHOTO_SIDE { square.resize_exact(PHOTO_SIDE, PHOTO_SIDE, image::imageops::FilterType::Lanczos3) } else { square };
    let mut jpeg = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(square.to_rgb8())
        .write_with_encoder(JpegEncoder::new_with_quality(&mut jpeg, JPEG_QUALITY))
        .map_err(|e| e.to_string())?;
    images::save_image(photos_dir, &BASE64.encode(jpeg.into_inner()), "jpg").map_err(|e| e.to_string())
}

fn download(url: &str) -> Result<Vec<u8>, String> {
    let dir = std::env::temp_dir().join("diwan-inventory-images");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{:x}", Sha256::digest(url)));
    if !path.exists() {
        let partial = path.with_extension("partial");
        let status = Command::new("curl")
            .args(["-fsSL", "--retry", "3", "--max-time", "60", "-A", "DiwanDemoSeed/1.0", "-o"])
            .arg(&partial)
            .arg(url)
            .status()
            .map_err(|e| format!("could not run curl: {e}"))?;
        if !status.success() {
            return Err(format!("download failed ({status}): {url}"));
        }
        std::fs::rename(&partial, &path).map_err(|e| e.to_string())?;
    }
    std::fs::read(&path).map_err(|e| e.to_string())
}

/// Every product and category, in the catalog or not, must have a photo that decodes.
fn verify(conn: &Connection, photos_dir: &Path) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT 'category', name, image_path FROM categories UNION ALL SELECT 'item', name, image_path FROM items")
        .expect("prepare verify");
    let rows: Vec<(String, String, Option<String>)> =
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).expect("verify").map(|row| row.expect("row")).collect();
    let mut read_ok = HashSet::new();
    let mut failures = Vec::new();
    for (kind, name, path) in rows {
        let Some(path) = path else {
            failures.push(format!("{kind} {name}: no photo"));
            continue;
        };
        if read_ok.contains(&path) {
            continue;
        }
        match image::open(photos_dir.join(&path)) {
            Ok(_) => {
                read_ok.insert(path);
            }
            Err(e) => failures.push(format!("{kind} {name}: {path} is not readable: {e}")),
        }
    }
    failures
}
