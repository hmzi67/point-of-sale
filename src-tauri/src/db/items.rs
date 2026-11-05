//! Inventory: categories and items.
//!
//! Money is stored (and passed across IPC) as integer minor units — the
//! frontend converts to/from a decimal amount only at the input/display
//! boundary, never here.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Categories
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: i64,
    pub name: String,
    /// Generated photo filename in the product-images folder, like `items.image_path`.
    pub image_path: Option<String>,
    /// A topping group — see `Item::topping_category_id`.
    pub is_topping: bool,
    /// Off the billing screen, with its items, while the owner has it inactive.
    pub is_active: bool,
}

/// A catalog entry the item form can pick (managed from the web dashboard).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub id: i64,
    pub name: String,
    /// The category a sub category belongs to.
    pub category_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub sub_categories: Vec<CatalogEntry>,
    pub manufacturers: Vec<CatalogEntry>,
    pub locations: Vec<CatalogEntry>,
}

/// The active entries, for the item form's dropdowns.
pub fn list_catalog(conn: &Connection) -> Result<Catalog, rusqlite::Error> {
    let entries = |sql: &str| -> Result<Vec<CatalogEntry>, rusqlite::Error> {
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([], |row| Ok(CatalogEntry { id: row.get(0)?, name: row.get(1)?, category_id: row.get(2)? }))?;
        rows.collect()
    };
    Ok(Catalog {
        sub_categories: entries("SELECT id, name, category_id FROM sub_categories WHERE is_active = 1 ORDER BY name")?,
        manufacturers: entries("SELECT id, name, NULL FROM manufacturers WHERE is_active = 1 ORDER BY name")?,
        locations: entries("SELECT id, name, NULL FROM locations WHERE is_active = 1 ORDER BY name")?,
    })
}

/// Separate from `update_item` so `ItemInput`, which CSV import and every
/// other caller also build, stays as it is.
pub fn set_item_catalog(
    conn: &Connection,
    id: i64,
    sub_category_id: Option<i64>,
    manufacturer_id: Option<i64>,
    location_id: Option<i64>,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE items SET sub_category_id = ?1, manufacturer_id = ?2, location_id = ?3 WHERE id = ?4",
        params![sub_category_id, manufacturer_id, location_id, id],
    )?;
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum CategoryError {
    #[error("A category named \"{0}\" already exists")]
    DuplicateName(String),
    #[error("{0}")]
    Validation(String),
    #[error("Category not found")]
    NotFound,
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

pub fn list_categories(conn: &Connection) -> Result<Vec<Category>, rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT id, name, image_path, is_topping, is_active FROM categories ORDER BY name")?;
    let rows = stmt.query_map([], category_from_row)?;
    rows.collect()
}

fn category_from_row(row: &Row) -> rusqlite::Result<Category> {
    Ok(Category {
        id: row.get("id")?,
        name: row.get("name")?,
        image_path: row.get("image_path")?,
        is_topping: row.get::<_, i64>("is_topping")? != 0,
        is_active: row.get::<_, i64>("is_active")? != 0,
    })
}

pub fn get_category(conn: &Connection, id: i64) -> Result<Category, CategoryError> {
    conn.query_row("SELECT id, name, image_path, is_topping, is_active FROM categories WHERE id = ?1", [id], category_from_row)
        .optional()?
        .ok_or(CategoryError::NotFound)
}

fn category_name(name: &str) -> Result<&str, CategoryError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CategoryError::Validation("Category name cannot be empty".into()));
    }
    Ok(name)
}

fn map_duplicate(name: &str) -> impl Fn(rusqlite::Error) -> CategoryError + '_ {
    move |err| match err {
        rusqlite::Error::SqliteFailure(e, _) if e.extended_code == 2067 => CategoryError::DuplicateName(name.to_string()),
        other => CategoryError::Sqlite(other),
    }
}

pub fn add_category(conn: &Connection, name: &str, image_path: Option<&str>, is_topping: bool) -> Result<Category, CategoryError> {
    let name = category_name(name)?;
    conn.execute(
        "INSERT INTO categories (name, image_path, is_topping) VALUES (?1, ?2, ?3)",
        params![name, image_path, is_topping as i64],
    )
    .map_err(map_duplicate(name))?;

    Ok(Category {
        id: conn.last_insert_rowid(),
        name: name.to_string(),
        image_path: image_path.map(str::to_string),
        is_topping,
        is_active: true,
    })
}

/// Full replace of the editable fields (name, photo, topping group), like `update_item`.
pub fn update_category(
    conn: &Connection,
    id: i64,
    name: &str,
    image_path: Option<&str>,
    is_topping: bool,
) -> Result<Category, CategoryError> {
    let name = category_name(name)?;
    let changed = conn
        .execute(
            "UPDATE categories SET name = ?1, image_path = ?2, is_topping = ?3 WHERE id = ?4",
            params![name, image_path, is_topping as i64, id],
        )
        .map_err(map_duplicate(name))?;
    if changed == 0 {
        return Err(CategoryError::NotFound);
    }
    get_category(conn, id)
}

/// Its items stay and become uncategorised (`items.category_id` is
/// `ON DELETE SET NULL`), so sale history is untouched.
pub fn delete_category(conn: &Connection, id: i64) -> Result<(), CategoryError> {
    if conn.execute("DELETE FROM categories WHERE id = ?1", [id])? == 0 {
        return Err(CategoryError::NotFound);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Items
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: i64,
    pub name: String,
    pub barcode: Option<String>,
    /// Short (2-4 digit) PLU-style code for keyboard-first billing — a
    /// cashier types this into the billing screen's quick-entry buffer
    /// instead of scanning a barcode. Deliberately a separate field from
    /// `barcode`: not every item needs one, and lookup tries `barcode`
    /// first, falling back to this — see `db::items::search_items`'s sibling
    /// `find_by_code` used by the billing quick-entry command.
    pub short_code: Option<String>,
    /// Short blurb shown on the billing screen's item detail modal.
    pub description: Option<String>,
    pub price_minor: i64,
    pub cost_minor: i64,
    /// `f64`, not `i64` — a `sold_by_amount` item's stock decrements by a
    /// fractional quantity (e.g. 0.4 kg). See `schema.rs`'s module doc
    /// comment for why the declared SQL column type change this pairs with
    /// needed no migration.
    pub stock_qty: f64,
    pub category_id: Option<i64>,
    /// Denormalized for the list view so it doesn't need a client-side join.
    pub category_name: Option<String>,
    pub low_stock_threshold: i64,
    pub is_active: bool,
    /// `stock_qty <= low_stock_threshold` — computed here so the frontend
    /// never has to duplicate the comparison (or get it wrong at a boundary).
    pub is_low_stock: bool,
    /// Filename under the product-image store (see `images.rs`), not a full
    /// path. `None` means no photo has been added yet.
    pub image_path: Option<String>,
    /// Sold by typing a rupee amount rather than a quantity on the billing
    /// screen (loose groceries — channa, rice, dry fruits) — see
    /// `schema.sql`'s `items.sold_by_amount` doc comment.
    pub sold_by_amount: bool,
    /// Display unit for this item's quantity (e.g. "kg"), shown on the cart
    /// line/receipt. `None` for a normal per-piece item.
    pub unit: Option<String>,
    /// Which kitchen/prep counter this item's KOT token belongs on.
    /// `None` is meaningful, not "unset" — this item never appears on any
    /// token at all (e.g. roti, if a client's kitchen doesn't token it).
    /// See `schema.sql`'s `counters` table doc comment for why this is a
    /// separate concept from `category_id`.
    pub counter_id: Option<i64>,
    /// Denormalized alongside `counter_id`, same reasoning as `category_name`.
    pub counter_name: Option<String>,
    /// The topping group (a category with `is_topping`) whose items the
    /// billing screen offers when this item is added. Toppings are ordinary
    /// items, so they are priced, stocked and printed like any other line.
    pub topping_category_id: Option<i64>,
    /// This item is itself a topping (its category is a topping group), so
    /// the billing grid leaves it out.
    pub is_topping: bool,
    pub sub_category_id: Option<i64>,
    pub manufacturer_id: Option<i64>,
    pub location_id: Option<i64>,
    /// Sold without counting stock; `stock_qty` then reads `SERVICE_STOCK`.
    pub is_service: bool,
}

/// What a service's `stock_qty` reads as, so the billing screen's stock limits
/// never stop it. The column itself stays 0 and sales leave it alone.
const SERVICE_STOCK: f64 = 1_000_000_000.0;

/// Everything the add/edit form submits. Used for both create and update —
/// an edit always replaces the full editable row, so there is no partial-patch
/// ambiguity around clearing `category_id` (or `image_path`) back to unset.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemInput {
    pub name: String,
    pub barcode: Option<String>,
    pub short_code: Option<String>,
    pub description: Option<String>,
    pub price_minor: i64,
    pub cost_minor: i64,
    pub stock_qty: f64,
    pub category_id: Option<i64>,
    pub low_stock_threshold: i64,
    pub image_path: Option<String>,
    pub sold_by_amount: bool,
    pub unit: Option<String>,
    pub counter_id: Option<i64>,
    #[serde(default)]
    pub topping_category_id: Option<i64>,
    /// On an edit: the stock the form was loaded with. When given, the
    /// update applies the *difference* the person typed (`stock + (new -
    /// original)`) instead of overwriting, so sales made meanwhile — on this
    /// counter or, synced, on another — are not undone by saving a name
    /// change. Ignored on create.
    #[serde(default)]
    pub original_stock_qty: Option<f64>,
}

/// Search/filter options for the list view.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemQuery {
    /// Matched against name and barcode, case-insensitively.
    pub search: Option<String>,
    pub category_id: Option<i64>,
    /// Archived items are hidden by default — a cashier's search should never
    /// surface a retired item.
    #[serde(default)]
    pub include_inactive: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ItemError {
    #[error("Item not found")]
    NotFound,
    #[error("Barcode \"{0}\" is already used by another item")]
    DuplicateBarcode(String),
    #[error("Code \"{0}\" is already used by another item")]
    DuplicateShortCode(String),
    #[error("Category {0} does not exist")]
    UnknownCategory(i64),
    #[error("Counter {0} does not exist")]
    UnknownCounter(i64),
    #[error("{0}")]
    Validation(String),
    /// The item has sale history, so it was archived instead of deleted.
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

const SELECT_ITEM: &str = "SELECT i.id, i.name, i.barcode, i.short_code, i.description, i.price_minor, i.cost_minor,
       i.stock_qty, i.category_id, c.name AS category_name,
       i.low_stock_threshold, i.is_active, i.image_path, i.sold_by_amount, i.unit,
       i.counter_id, k.name AS counter_name, i.topping_category_id, COALESCE(c.is_topping, 0) AS is_topping,
       i.sub_category_id, i.manufacturer_id, i.location_id, i.is_service
  FROM items i
  LEFT JOIN categories c ON c.id = i.category_id
  LEFT JOIN counters   k ON k.id = i.counter_id";

fn from_row(row: &Row<'_>) -> Result<Item, rusqlite::Error> {
    let is_service = row.get::<_, i64>("is_service")? != 0;
    let stock_qty: f64 = if is_service { SERVICE_STOCK } else { row.get("stock_qty")? };
    let low_stock_threshold: i64 = row.get("low_stock_threshold")?;
    Ok(Item {
        id: row.get("id")?,
        name: row.get("name")?,
        barcode: row.get("barcode")?,
        short_code: row.get("short_code")?,
        description: row.get("description")?,
        price_minor: row.get("price_minor")?,
        cost_minor: row.get("cost_minor")?,
        stock_qty,
        category_id: row.get("category_id")?,
        category_name: row.get("category_name")?,
        low_stock_threshold,
        is_active: row.get::<_, i64>("is_active")? != 0,
        is_low_stock: stock_qty <= low_stock_threshold as f64,
        image_path: row.get("image_path")?,
        sold_by_amount: row.get::<_, i64>("sold_by_amount")? != 0,
        unit: row.get("unit")?,
        counter_id: row.get("counter_id")?,
        counter_name: row.get("counter_name")?,
        topping_category_id: row.get("topping_category_id")?,
        is_topping: row.get::<_, i64>("is_topping")? != 0,
        sub_category_id: row.get("sub_category_id")?,
        manufacturer_id: row.get("manufacturer_id")?,
        location_id: row.get("location_id")?,
        is_service,
    })
}

/// Items matching `query`, alphabetically.
///
/// All three filters are bound on every call via `IS NULL OR` guards rather
/// than building the SQL text conditionally — that keeps the placeholder
/// numbering fixed and impossible to get out of sync with the bound values.
pub fn list_items(conn: &Connection, query: &ItemQuery) -> Result<Vec<Item>, rusqlite::Error> {
    let sql = format!(
        "{} WHERE (?1 = 1 OR (i.is_active = 1 AND COALESCE(c.is_active, 1) = 1))
           AND (?2 IS NULL OR i.name LIKE ?2 ESCAPE '\\' OR i.barcode LIKE ?2 ESCAPE '\\')
           AND (?3 IS NULL OR i.category_id = ?3)
         ORDER BY i.name",
        SELECT_ITEM
    );

    // Escape SQL LIKE wildcards in free-text input so a barcode containing
    // '%' or '_' cannot be used to match unrelated rows.
    let like_pattern = query.search.as_ref().map(|s| {
        format!(
            "%{}%",
            s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
        )
    });

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        params![query.include_inactive as i64, like_pattern, query.category_id],
        from_row,
    )?;

    rows.collect()
}

/// Billing-screen search: active items only, matched by name or barcode,
/// with an exact barcode match sorted first. That ordering is what lets a
/// barcode scan (which types the code and sends Enter) reliably add "the top
/// result" — an exact scan must never be outranked by an unrelated item whose
/// name happens to sort earlier alphabetically.
///
/// Both the barcode equality check and the barcode UNIQUE index make an exact
/// scan an index hit; the LIKE half falls back to the `idx_items_name` index
/// for a prefix search but degrades to a scan for a mid-string match — an
/// accepted tradeoff for a catalogue sized for a single shop, not a warehouse.
pub fn search_items(conn: &Connection, query: &str, limit: i64) -> Result<Vec<Item>, rusqlite::Error> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let like_pattern = format!(
        "%{}%",
        query.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
    );

    let sql = format!(
        "{} WHERE i.is_active = 1 AND COALESCE(c.is_active, 1) = 1
           AND (i.barcode = ?1 OR i.name LIKE ?2 ESCAPE '\\' OR i.barcode LIKE ?2 ESCAPE '\\')
         ORDER BY (i.barcode = ?1) DESC, i.name
         LIMIT ?3",
        SELECT_ITEM
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![query, like_pattern, limit], from_row)?;
    rows.collect()
}

/// The next unused short code for the "Add item" form to pre-fill — an
/// Owner/Admin can still type over it. Picks one past the highest
/// numerically-valued existing code (so codes stay roughly assignment-order,
/// not reused after a gap from a deleted item), starting at "01" for the
/// first item ever given one, and always zero-padded to at least 2 digits
/// (the format's floor — see `schema.sql`'s doc comment on the column). Once
/// past 99 it grows to 3 then 4 digits naturally, and stops suggesting
/// (returns `None`) once even a 4-digit code space would be exhausted —
/// vanishingly unlikely for a single shop's catalogue, but a suggestion is
/// only ever a starting point the form lets the caller override anyway.
pub fn suggest_next_short_code(conn: &Connection) -> Result<Option<String>, rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT short_code FROM items WHERE short_code IS NOT NULL")?;
    let max_used = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .filter_map(|code| code.ok())
        .filter_map(|code| code.parse::<u32>().ok())
        .max()
        .unwrap_or(0);

    let next = max_used + 1;
    if next > 9999 {
        return Ok(None);
    }
    Ok(Some(format!("{:02}", next)))
}

pub fn get_item(conn: &Connection, id: i64) -> Result<Item, ItemError> {
    let sql = format!("{} WHERE i.id = ?1", SELECT_ITEM);
    conn.query_row(&sql, params![id], from_row)
        .optional()?
        .ok_or(ItemError::NotFound)
}

/// The archived category that holds `outside_item` rows.
pub const OUTSIDE_CATEGORY: &str = "Outside items";

/// Something not on the menu that the customer wants on the bill (brought or
/// bought from outside), typed at the till as a name and a price. It is an
/// ordinary item, so pricing, receipts, tokens, refunds and sync need nothing
/// new: a service (no stock) in an archived category, which keeps it off the
/// menu, search and Inventory while it stays sellable. The same name and
/// price reuse one row.
pub fn outside_item(conn: &Connection, name: &str, price_minor: i64) -> Result<Item, ItemError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ItemError::Validation("Item name cannot be empty".into()));
    }
    if price_minor < 0 {
        return Err(ItemError::Validation("Price cannot be negative".into()));
    }
    conn.execute("INSERT OR IGNORE INTO categories (name, is_active) VALUES (?1, 0)", [OUTSIDE_CATEGORY])?;
    let category_id: i64 = conn.query_row("SELECT id FROM categories WHERE name = ?1", [OUTSIDE_CATEGORY], |row| row.get(0))?;
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM items WHERE category_id = ?1 AND name = ?2 COLLATE NOCASE AND price_minor = ?3
               AND is_service = 1 AND is_active = 1 LIMIT 1",
            params![category_id, name, price_minor],
            |row| row.get(0),
        )
        .optional()?;
    let id = match existing {
        Some(id) => id,
        None => {
            conn.execute(
                "INSERT INTO items (name, price_minor, category_id, is_service) VALUES (?1, ?2, ?3, 1)",
                params![name, price_minor, category_id],
            )?;
            conn.last_insert_rowid()
        }
    };
    get_item(conn, id)
}

fn validate(conn: &Connection, input: &ItemInput) -> Result<(), ItemError> {
    if input.name.trim().is_empty() {
        return Err(ItemError::Validation("Item name cannot be empty".into()));
    }
    if input.price_minor < 0 || input.cost_minor < 0 {
        return Err(ItemError::Validation("Price and cost cannot be negative".into()));
    }
    if !input.stock_qty.is_finite() || input.stock_qty < 0.0 {
        return Err(ItemError::Validation("Stock quantity cannot be negative".into()));
    }
    if input.low_stock_threshold < 0 {
        return Err(ItemError::Validation("Low-stock threshold cannot be negative".into()));
    }

    if let Some(code) = input.short_code.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let is_2_to_4_digits = (2..=4).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_digit());
        if !is_2_to_4_digits {
            return Err(ItemError::Validation("Short code must be 2-4 digits".into()));
        }
    }

    if let Some(category_id) = input.category_id {
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM categories WHERE id = ?1)",
            params![category_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(ItemError::UnknownCategory(category_id));
        }
    }

    if let Some(group) = input.topping_category_id {
        let is_group: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM categories WHERE id = ?1 AND is_topping = 1)",
            params![group],
            |row| row.get(0),
        )?;
        if !is_group {
            return Err(ItemError::Validation("Pick a topping group for the toppings".into()));
        }
    }

    if let Some(counter_id) = input.counter_id {
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM counters WHERE id = ?1)",
            params![counter_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(ItemError::UnknownCounter(counter_id));
        }
    }

    Ok(())
}

/// Maps a UNIQUE-constraint violation (SQLite extended code 2067) on either
/// `items.barcode` or `items.short_code` to the matching typed error — the
/// message text SQLite attaches names the column that actually conflicted
/// (`"UNIQUE constraint failed: items.short_code"`), so that's what
/// disambiguates which of the two this was, since both fire the same
/// extended code.
fn map_unique_conflict(err: rusqlite::Error, barcode: &Option<String>, short_code: &Option<String>) -> ItemError {
    match err {
        rusqlite::Error::SqliteFailure(e, ref msg) if e.extended_code == 2067 => {
            if msg.as_deref().is_some_and(|m| m.contains("short_code")) {
                ItemError::DuplicateShortCode(short_code.clone().unwrap_or_default())
            } else {
                ItemError::DuplicateBarcode(barcode.clone().unwrap_or_default())
            }
        }
        other => ItemError::Sqlite(other),
    }
}

pub fn add_item(conn: &Connection, input: ItemInput) -> Result<Item, ItemError> {
    validate(conn, &input)?;
    let name = input.name.trim();
    // Empty string is not a meaningful "no barcode" — normalize to NULL so it
    // never collides with another blank barcode under the UNIQUE constraint.
    let barcode = input.barcode.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let short_code = input.short_code.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let description = input.description.as_deref().map(str::trim).filter(|s| !s.is_empty());

    let unit = input.unit.as_deref().map(str::trim).filter(|s| !s.is_empty());

    conn.execute(
        "INSERT INTO items
             (name, barcode, short_code, description, price_minor, cost_minor, stock_qty, category_id,
              low_stock_threshold, image_path, sold_by_amount, unit, counter_id, topping_category_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            name,
            barcode,
            short_code,
            description,
            input.price_minor,
            input.cost_minor,
            input.stock_qty,
            input.category_id,
            input.low_stock_threshold,
            input.image_path,
            input.sold_by_amount as i64,
            unit,
            input.counter_id,
            input.topping_category_id,
        ],
    )
    .map_err(|err| map_unique_conflict(err, &barcode.map(str::to_string), &short_code.map(str::to_string)))?;

    get_item(conn, conn.last_insert_rowid())
}

pub fn update_item(conn: &Connection, id: i64, input: ItemInput) -> Result<Item, ItemError> {
    validate(conn, &input)?;
    let name = input.name.trim();
    let barcode = input.barcode.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let short_code = input.short_code.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let description = input.description.as_deref().map(str::trim).filter(|s| !s.is_empty());

    let unit = input.unit.as_deref().map(str::trim).filter(|s| !s.is_empty());

    let changed = conn
        .execute(
            "UPDATE items
                SET name = ?1, barcode = ?2, short_code = ?3, description = ?4, price_minor = ?5, cost_minor = ?6,
                    stock_qty = CASE WHEN ?15 IS NULL THEN ?7 ELSE round(stock_qty + (?7 - ?15), 4) END,
                    category_id = ?8, low_stock_threshold = ?9,
                    image_path = ?10, sold_by_amount = ?11, unit = ?12, counter_id = ?13,
                    topping_category_id = ?16, updated_at = datetime('now', 'localtime')
              WHERE id = ?14",
            params![
                name,
                barcode,
                short_code,
                description,
                input.price_minor,
                input.cost_minor,
                input.stock_qty,
                input.category_id,
                input.low_stock_threshold,
                input.image_path,
                input.sold_by_amount as i64,
                unit,
                input.counter_id,
                id,
                input.original_stock_qty,
                input.topping_category_id,
            ],
        )
        .map_err(|err| map_unique_conflict(err, &barcode.map(str::to_string), &short_code.map(str::to_string)))?;

    if changed == 0 {
        return Err(ItemError::NotFound);
    }

    get_item(conn, id)
}

/// What actually happened to the row — an item that has ever appeared on a
/// sale is kept for reporting history and archived instead of removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeleteOutcome {
    Deleted,
    Archived,
}

pub fn delete_item(conn: &Connection, id: i64) -> Result<DeleteOutcome, ItemError> {
    // Any RESTRICT reference (sales, kitchen tokens, order lines) means the
    // item has history: archive instead of letting the DELETE fail.
    let has_sales: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sale_items WHERE item_id = ?1)
             OR EXISTS(SELECT 1 FROM token_items WHERE item_id = ?1)
             OR EXISTS(SELECT 1 FROM order_items WHERE item_id = ?1)",
        params![id],
        |row| row.get(0),
    )?;

    if has_sales {
        let changed = conn.execute("UPDATE items SET is_active = 0 WHERE id = ?1", params![id])?;
        if changed == 0 {
            return Err(ItemError::NotFound);
        }
        return Ok(DeleteOutcome::Archived);
    }

    let changed = conn.execute("DELETE FROM items WHERE id = ?1", params![id])?;
    if changed == 0 {
        return Err(ItemError::NotFound);
    }
    Ok(DeleteOutcome::Deleted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::{item_id, item_input};
    use crate::db::schema::test_conn;

    fn basic_input(name: &str) -> ItemInput {
        item_input(name, 5.0)
    }

    #[test]
    fn description_is_trimmed_blank_becomes_none_and_round_trips_through_update() {
        let conn = test_conn();
        let mut input = basic_input("Choc Muffin");
        input.description = Some("  Rich, fudgy, baked fresh daily.  ".into());
        let created = add_item(&conn, input).unwrap();
        assert_eq!(created.description.as_deref(), Some("Rich, fudgy, baked fresh daily."));

        let mut update = basic_input("Choc Muffin");
        update.description = Some("   ".into());
        let updated = update_item(&conn, created.id, update).unwrap();
        assert_eq!(updated.description, None, "a blank description must be stored as NULL");
    }

    #[test]
    fn billing_search_ranks_an_exact_barcode_match_first() {
        let conn = test_conn();
        // "Tea Leaves 400g" (barcode ...158) and "Green Tea (25 bags)" both
        // contain "tea"; scanning the exact barcode must still win the top
        // spot over whatever sorts first alphabetically.
        let results = search_items(&conn, "8901234500158", 20).unwrap();
        assert_eq!(results[0].name, "Tea Leaves 400g");
    }

    #[test]
    fn billing_search_matches_a_partial_name() {
        let conn = test_conn();
        let results = search_items(&conn, "cola", 20).unwrap();
        assert!(results.iter().any(|i| i.name == "Cola 500ml"));
    }

    #[test]
    fn billing_search_returns_nothing_for_an_empty_query() {
        let conn = test_conn();
        assert!(search_items(&conn, "", 20).unwrap().is_empty());
        assert!(search_items(&conn, "   ", 20).unwrap().is_empty());
    }

    #[test]
    fn billing_search_excludes_archived_items() {
        let conn = test_conn();
        let cola_id = item_id(&conn, "Cola 500ml");
        conn.execute("UPDATE items SET is_active = 0 WHERE id = ?1", params![cola_id])
            .unwrap();

        // Search on the exact barcode, which only "Cola 500ml" could ever
        // match — unlike the substring "cola", which "Chocolate" also
        // contains ("choCOLAte") and would otherwise make this test pass for
        // the wrong reason.
        let results = search_items(&conn, "8901234500011", 20).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn billing_search_respects_the_limit() {
        let conn = test_conn();
        // Every seeded item's name contains a vowel or space that matches "e"
        // loosely; use a broad pattern known to match many rows.
        let results = search_items(&conn, "e", 3).unwrap();
        assert!(results.len() <= 3);
    }

    #[test]
    fn search_matches_name_or_barcode_case_insensitively() {
        let conn = test_conn();

        let by_name = list_items(
            &conn,
            &ItemQuery { search: Some("cola".into()), ..Default::default() },
        )
        .unwrap();
        assert!(by_name.iter().any(|i| i.name == "Cola 500ml"));

        let by_barcode = list_items(
            &conn,
            &ItemQuery { search: Some("8901234500011".into()), ..Default::default() },
        )
        .unwrap();
        assert_eq!(by_barcode.len(), 1);
        assert_eq!(by_barcode[0].name, "Cola 500ml");
    }

    #[test]
    fn search_escapes_like_wildcards() {
        let conn = test_conn();
        add_category(&conn, "Weird_Category", None, false).unwrap();

        // A literal underscore in the search term must not act as a wildcard
        // and match every category-having item.
        let results = list_items(
            &conn,
            &ItemQuery { search: Some("Weird_Category".into()), ..Default::default() },
        )
        .unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn category_filter_narrows_results() {
        let conn = test_conn();
        let categories = list_categories(&conn).unwrap();
        let beverages = categories.iter().find(|c| c.name == "Beverages").unwrap();

        let results = list_items(
            &conn,
            &ItemQuery { category_id: Some(beverages.id), ..Default::default() },
        )
        .unwrap();
        assert!(!results.is_empty());
        assert!(results.iter().all(|i| i.category_id == Some(beverages.id)));
    }

    #[test]
    fn archived_items_are_hidden_unless_requested() {
        let conn = test_conn();
        let item = list_items(&conn, &ItemQuery::default()).unwrap().remove(0);
        delete_item(&conn, item.id).unwrap();

        let active_only = list_items(&conn, &ItemQuery::default()).unwrap();
        assert!(!active_only.iter().any(|i| i.id == item.id));

        let including_archived = list_items(
            &conn,
            &ItemQuery { include_inactive: true, ..Default::default() },
        )
        .unwrap();
        assert!(including_archived.iter().any(|i| i.id == item.id && !i.is_active));
    }

    #[test]
    fn low_stock_is_computed_from_threshold() {
        let conn = test_conn();
        let items = list_items(&conn, &ItemQuery::default()).unwrap();
        let noodles = items.iter().find(|i| i.name == "Instant Noodles").unwrap();
        assert!(noodles.is_low_stock);
        let cola = items.iter().find(|i| i.name == "Cola 500ml").unwrap();
        assert!(!cola.is_low_stock);
    }

    #[test]
    fn add_item_rejects_duplicate_barcode() {
        let conn = test_conn();
        let mut input = basic_input("New Item");
        input.barcode = Some("8901234500011".into()); // already used by Cola
        let err = add_item(&conn, input).unwrap_err();
        assert!(matches!(err, ItemError::DuplicateBarcode(_)));
    }

    #[test]
    fn short_code_round_trips_and_is_nullable() {
        let conn = test_conn();
        let mut input = basic_input("Coded Item");
        input.short_code = Some("42".into());
        let created = add_item(&conn, input).unwrap();
        assert_eq!(created.short_code.as_deref(), Some("42"));

        // Most items never get one — nullable, no format requirement then.
        let uncoded = add_item(&conn, basic_input("Uncoded Item")).unwrap();
        assert_eq!(uncoded.short_code, None);
    }

    #[test]
    fn add_item_rejects_a_duplicate_short_code_distinctly_from_a_duplicate_barcode() {
        let conn = test_conn();
        let mut first = basic_input("First");
        first.short_code = Some("07".into());
        add_item(&conn, first).unwrap();

        let mut second = basic_input("Second");
        second.short_code = Some("07".into());
        let err = add_item(&conn, second).unwrap_err();
        assert!(matches!(err, ItemError::DuplicateShortCode(code) if code == "07"));
    }

    #[test]
    fn add_item_rejects_a_short_code_outside_2_to_4_digits() {
        let conn = test_conn();
        let mut too_short = basic_input("Too Short");
        too_short.short_code = Some("5".into());
        assert!(matches!(add_item(&conn, too_short).unwrap_err(), ItemError::Validation(_)));

        let mut not_digits = basic_input("Not Digits");
        not_digits.short_code = Some("AB".into());
        assert!(matches!(add_item(&conn, not_digits).unwrap_err(), ItemError::Validation(_)));
    }

    #[test]
    fn suggest_next_short_code_climbs_past_the_highest_used_code() {
        let conn = test_conn();
        assert_eq!(suggest_next_short_code(&conn).unwrap().as_deref(), Some("01"));

        let mut input = basic_input("Coded");
        input.short_code = Some("07".into());
        add_item(&conn, input).unwrap();
        assert_eq!(suggest_next_short_code(&conn).unwrap().as_deref(), Some("08"));
    }

    #[test]
    fn add_item_rejects_unknown_category() {
        let conn = test_conn();
        let mut input = basic_input("New Item");
        input.category_id = Some(999_999);
        let err = add_item(&conn, input).unwrap_err();
        assert!(matches!(err, ItemError::UnknownCategory(999_999)));
    }

    #[test]
    fn image_path_round_trips_through_add_and_update() {
        let conn = test_conn();

        let mut input = basic_input("Photographed Item");
        input.image_path = Some("item-123.png".into());
        let created = add_item(&conn, input).unwrap();
        assert_eq!(created.image_path, Some("item-123.png".into()));

        // A full-form update with image_path: None clears it, matching how
        // category_id clearing works — this is a full replace, not a patch.
        let cleared = update_item(&conn, created.id, basic_input("Photographed Item")).unwrap();
        assert_eq!(cleared.image_path, None);
    }

    #[test]
    fn sold_by_amount_and_unit_round_trip_and_stock_qty_can_be_fractional() {
        let conn = test_conn();
        let mut input = basic_input("Loose Channa");
        input.sold_by_amount = true;
        input.unit = Some("  kg  ".into());
        input.stock_qty = 12.4;
        let created = add_item(&conn, input).unwrap();
        assert!(created.sold_by_amount);
        assert_eq!(created.unit.as_deref(), Some("kg"), "must be trimmed");
        assert_eq!(created.stock_qty, 12.4);

        // A normal per-piece item defaults to not-sold-by-amount, no unit —
        // basic_input's default.
        let piece = add_item(&conn, basic_input("Regular Piece Item")).unwrap();
        assert!(!piece.sold_by_amount);
        assert_eq!(piece.unit, None);
    }

    #[test]
    fn blank_unit_is_stored_as_null() {
        let conn = test_conn();
        let mut input = basic_input("No Unit Really");
        input.unit = Some("   ".into());
        let created = add_item(&conn, input).unwrap();
        assert_eq!(created.unit, None);
    }

    #[test]
    fn counter_id_round_trips_and_none_means_no_token() {
        use crate::db::counters;

        let conn = test_conn();
        let counter = counters::add_counter(&conn, "Tandoor").unwrap();

        let mut with_counter = basic_input("Naan");
        with_counter.counter_id = Some(counter.id);
        let created = add_item(&conn, with_counter).unwrap();
        assert_eq!(created.counter_id, Some(counter.id));
        assert_eq!(created.counter_name.as_deref(), Some("Tandoor"));

        // No counter assigned — deliberately meaningful "never appears on a
        // token", not just "not chosen yet".
        let roti = add_item(&conn, basic_input("Roti")).unwrap();
        assert_eq!(roti.counter_id, None);
        assert_eq!(roti.counter_name, None);
    }

    #[test]
    fn add_item_rejects_unknown_counter() {
        let conn = test_conn();
        let mut input = basic_input("New Item");
        input.counter_id = Some(999_999);
        let err = add_item(&conn, input).unwrap_err();
        assert!(matches!(err, ItemError::UnknownCounter(999_999)));
    }

    #[test]
    fn add_item_rejects_blank_name_and_negative_numbers() {
        let conn = test_conn();
        assert!(matches!(
            add_item(&conn, basic_input("  ")).unwrap_err(),
            ItemError::Validation(_)
        ));

        let mut negative_price = basic_input("Valid Name");
        negative_price.price_minor = -1;
        assert!(matches!(
            add_item(&conn, negative_price).unwrap_err(),
            ItemError::Validation(_)
        ));
    }

    #[test]
    fn blank_barcode_is_stored_as_null_and_never_collides() {
        let conn = test_conn();
        let mut a = basic_input("No Barcode A");
        a.barcode = Some("   ".into());
        let mut b = basic_input("No Barcode B");
        b.barcode = Some("".into());

        let a = add_item(&conn, a).unwrap();
        let b = add_item(&conn, b).unwrap();
        assert_eq!(a.barcode, None);
        assert_eq!(b.barcode, None);
    }

    #[test]
    fn update_item_replaces_the_full_row_including_clearing_category() {
        let conn = test_conn();
        let created = add_item(&conn, basic_input("Editable")).unwrap();

        let categories = list_categories(&conn).unwrap();
        let mut patch = basic_input("Editable Renamed");
        patch.category_id = Some(categories[0].id);
        patch.stock_qty = 42.0;
        let updated = update_item(&conn, created.id, patch).unwrap();
        assert_eq!(updated.name, "Editable Renamed");
        assert_eq!(updated.category_id, Some(categories[0].id));
        assert_eq!(updated.stock_qty, 42.0);

        // A second update with category_id: None must clear it, not leave it
        // unchanged — this is a full replace, not a partial patch.
        let mut clear_category = basic_input("Editable Renamed");
        clear_category.stock_qty = 42.0;
        let cleared = update_item(&conn, created.id, clear_category).unwrap();
        assert_eq!(cleared.category_id, None);
    }

    #[test]
    fn an_edit_applies_the_typed_stock_difference_so_sales_made_meanwhile_stay() {
        let conn = test_conn();
        let created = add_item(&conn, basic_input("Busy Item")).unwrap(); // stock 5
        // The form is open; meanwhile 2 are sold.
        conn.execute("UPDATE items SET stock_qty = stock_qty - 2 WHERE id = ?1", [created.id]).unwrap();
        let mut renamed = basic_input("Busy Item (renamed)");
        renamed.original_stock_qty = Some(5.0);
        assert_eq!(update_item(&conn, created.id, renamed).unwrap().stock_qty, 3.0, "renaming keeps the sale");
        let mut restock = basic_input("Busy Item (renamed)");
        restock.stock_qty = 15.0;
        restock.original_stock_qty = Some(5.0);
        assert_eq!(update_item(&conn, created.id, restock).unwrap().stock_qty, 13.0, "+10 typed, +10 applied");
    }

    #[test]
    fn update_item_rejects_unknown_id() {
        let conn = test_conn();
        let err = update_item(&conn, 999_999, basic_input("Ghost")).unwrap_err();
        assert!(matches!(err, ItemError::NotFound));
    }

    #[test]
    fn deleting_a_never_sold_item_removes_it_outright() {
        let conn = test_conn();
        let created = add_item(&conn, basic_input("Never Sold")).unwrap();
        let outcome = delete_item(&conn, created.id).unwrap();
        assert_eq!(outcome, DeleteOutcome::Deleted);
        assert!(matches!(get_item(&conn, created.id).unwrap_err(), ItemError::NotFound));
    }

    #[test]
    fn deleting_a_sold_item_archives_it_instead() {
        let conn = test_conn();
        // Any item present in seeded sale_items.
        let sold_item_id: i64 = conn
            .query_row("SELECT item_id FROM sale_items LIMIT 1", [], |row| row.get(0))
            .unwrap();

        let outcome = delete_item(&conn, sold_item_id).unwrap();
        assert_eq!(outcome, DeleteOutcome::Archived);

        let item = get_item(&conn, sold_item_id).unwrap();
        assert!(!item.is_active);
        // The sale line itself must still exist — history is untouched.
        let remaining: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sale_items WHERE item_id = ?1",
                [sold_item_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(remaining > 0);
    }

    #[test]
    fn deleting_an_item_on_an_unpaid_order_archives_it_instead_of_failing() {
        let conn = test_conn();
        let created = add_item(&conn, basic_input("On Order")).unwrap();
        conn.execute(
            "INSERT INTO orders (id, order_no, order_type, client_ref) VALUES (1, 1, 'takeaway', 'ref-1')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO order_items (order_id, item_id, qty, price_at_order_minor) VALUES (1, ?1, 1, 10000)",
            [created.id],
        )
        .unwrap();

        assert_eq!(delete_item(&conn, created.id).unwrap(), DeleteOutcome::Archived);
        assert!(!get_item(&conn, created.id).unwrap().is_active);
    }

    #[test]
    fn add_category_rejects_duplicates_and_blank_names() {
        let conn = test_conn();
        assert!(matches!(
            add_category(&conn, "Beverages", None, false).unwrap_err(),
            CategoryError::DuplicateName(_)
        ));
        assert!(matches!(
            add_category(&conn, "   ", None, false).unwrap_err(),
            CategoryError::Validation(_)
        ));

        let created = add_category(&conn, "Household", None, false).unwrap();
        assert_eq!(created.name, "Household");
    }

    #[test]
    fn items_offer_only_a_topping_group_and_toppings_are_flagged() {
        let conn = test_conn();
        let plain = add_category(&conn, "Pizzas", None, false).unwrap();
        let group = add_category(&conn, "Pizza Toppings", None, true).unwrap();

        let mut pizza = crate::db::test_support::item_input("Fajita Pizza", 10.0);
        pizza.topping_category_id = Some(plain.id);
        assert!(matches!(add_item(&conn, pizza.clone()).unwrap_err(), ItemError::Validation(_)));

        pizza.topping_category_id = Some(group.id);
        let pizza = add_item(&conn, pizza).unwrap();
        assert_eq!((pizza.topping_category_id, pizza.is_topping), (Some(group.id), false));

        let mut cheese = crate::db::test_support::item_input("Extra Cheese", 50.0);
        cheese.category_id = Some(group.id);
        assert!(add_item(&conn, cheese).unwrap().is_topping);
    }

    #[test]
    fn update_and_delete_category_keep_items() {
        let conn = test_conn();
        let category = add_category(&conn, "Nimko", Some("item-a.jpg"), false).unwrap();
        assert!(matches!(
            update_category(&conn, category.id, "Beverages", None, false).unwrap_err(),
            CategoryError::DuplicateName(_)
        ));

        let renamed = update_category(&conn, category.id, " Chips ", None, false).unwrap();
        assert_eq!((renamed.name.as_str(), renamed.image_path), ("Chips", None));

        conn.execute("INSERT INTO items (name, price_minor, category_id) VALUES ('Lays', 100, ?1)", [category.id]).unwrap();
        delete_category(&conn, category.id).unwrap();
        let category_id: Option<i64> =
            conn.query_row("SELECT category_id FROM items WHERE name = 'Lays'", [], |r| r.get(0)).unwrap();
        assert_eq!(category_id, None, "the item stays, uncategorised");
        assert!(matches!(delete_category(&conn, category.id).unwrap_err(), CategoryError::NotFound));
    }

    #[test]
    fn an_outside_item_sells_without_stock_but_stays_off_the_menu_and_inventory() {
        let mut conn = test_conn();
        let cake = outside_item(&conn, "  Birthday cake ", 150000).unwrap();
        assert_eq!((cake.name.as_str(), cake.price_minor, cake.is_service), ("Birthday cake", 150000, true));
        assert_eq!(outside_item(&conn, "birthday CAKE", 150000).unwrap().id, cake.id, "same name and price: one row");
        assert_ne!(outside_item(&conn, "Birthday cake", 90000).unwrap().id, cake.id, "another price is another row");

        let listed = list_items(&conn, &ItemQuery::default()).unwrap();
        assert!(!listed.iter().any(|item| item.id == cake.id), "not on the menu or in Inventory");
        assert!(search_items(&conn, "cake", 20).unwrap().is_empty(), "not in billing search");

        let sale_id = crate::db::test_support::ring_up(&mut conn, "Birthday cake", 2.0, "cash", None);
        let total: i64 = conn.query_row("SELECT total_minor FROM sales WHERE id = ?1", [sale_id], |r| r.get(0)).unwrap();
        assert_eq!(total, 300000);

        for (name, price) in [(" ", 100), ("Cake", -1)] {
            assert!(matches!(outside_item(&conn, name, price), Err(ItemError::Validation(_))));
        }
    }
}
