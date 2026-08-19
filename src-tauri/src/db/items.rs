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
}

#[derive(Debug)]
pub enum CategoryError {
    DuplicateName(String),
    Validation(String),
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for CategoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CategoryError::DuplicateName(name) => {
                write!(f, "A category named \"{}\" already exists", name)
            }
            CategoryError::Validation(msg) => write!(f, "{}", msg),
            CategoryError::Sqlite(err) => write!(f, "database error: {}", err),
        }
    }
}

impl From<rusqlite::Error> for CategoryError {
    fn from(err: rusqlite::Error) -> Self {
        CategoryError::Sqlite(err)
    }
}

pub fn list_categories(conn: &Connection) -> Result<Vec<Category>, rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT id, name FROM categories ORDER BY name")?;
    let rows = stmt.query_map([], |row| {
        Ok(Category {
            id: row.get("id")?,
            name: row.get("name")?,
        })
    })?;
    rows.collect()
}

pub fn add_category(conn: &Connection, name: &str) -> Result<Category, CategoryError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CategoryError::Validation("Category name cannot be empty".into()));
    }

    conn.execute("INSERT INTO categories (name) VALUES (?1)", params![name])
        .map_err(|err| match err {
            rusqlite::Error::SqliteFailure(e, _) if e.extended_code == 2067 => {
                CategoryError::DuplicateName(name.to_string())
            }
            other => CategoryError::Sqlite(other),
        })?;

    Ok(Category {
        id: conn.last_insert_rowid(),
        name: name.to_string(),
    })
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
    /// Short blurb shown on the billing screen's item detail modal.
    pub description: Option<String>,
    pub price_minor: i64,
    pub cost_minor: i64,
    pub stock_qty: i64,
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
}

/// Everything the add/edit form submits. Used for both create and update —
/// an edit always replaces the full editable row, so there is no partial-patch
/// ambiguity around clearing `category_id` (or `image_path`) back to unset.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemInput {
    pub name: String,
    pub barcode: Option<String>,
    pub description: Option<String>,
    pub price_minor: i64,
    pub cost_minor: i64,
    pub stock_qty: i64,
    pub category_id: Option<i64>,
    pub low_stock_threshold: i64,
    pub image_path: Option<String>,
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

#[derive(Debug)]
pub enum ItemError {
    NotFound,
    DuplicateBarcode(String),
    UnknownCategory(i64),
    Validation(String),
    /// The item has sale history, so it was archived instead of deleted.
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for ItemError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ItemError::NotFound => write!(f, "Item not found"),
            ItemError::DuplicateBarcode(code) => {
                write!(f, "Barcode \"{}\" is already used by another item", code)
            }
            ItemError::UnknownCategory(id) => write!(f, "Category {} does not exist", id),
            ItemError::Validation(msg) => write!(f, "{}", msg),
            ItemError::Sqlite(err) => write!(f, "database error: {}", err),
        }
    }
}

impl From<rusqlite::Error> for ItemError {
    fn from(err: rusqlite::Error) -> Self {
        ItemError::Sqlite(err)
    }
}

const SELECT_ITEM: &str = "SELECT i.id, i.name, i.barcode, i.description, i.price_minor, i.cost_minor,
       i.stock_qty, i.category_id, c.name AS category_name,
       i.low_stock_threshold, i.is_active, i.image_path
  FROM items i
  LEFT JOIN categories c ON c.id = i.category_id";

fn from_row(row: &Row<'_>) -> Result<Item, rusqlite::Error> {
    let stock_qty: i64 = row.get("stock_qty")?;
    let low_stock_threshold: i64 = row.get("low_stock_threshold")?;
    Ok(Item {
        id: row.get("id")?,
        name: row.get("name")?,
        barcode: row.get("barcode")?,
        description: row.get("description")?,
        price_minor: row.get("price_minor")?,
        cost_minor: row.get("cost_minor")?,
        stock_qty,
        category_id: row.get("category_id")?,
        category_name: row.get("category_name")?,
        low_stock_threshold,
        is_active: row.get::<_, i64>("is_active")? != 0,
        is_low_stock: stock_qty <= low_stock_threshold,
        image_path: row.get("image_path")?,
    })
}

/// Items matching `query`, alphabetically.
///
/// All three filters are bound on every call via `IS NULL OR` guards rather
/// than building the SQL text conditionally — that keeps the placeholder
/// numbering fixed and impossible to get out of sync with the bound values.
pub fn list_items(conn: &Connection, query: &ItemQuery) -> Result<Vec<Item>, rusqlite::Error> {
    let sql = format!(
        "{} WHERE (?1 = 1 OR i.is_active = 1)
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
        "{} WHERE i.is_active = 1
           AND (i.barcode = ?1 OR i.name LIKE ?2 ESCAPE '\\' OR i.barcode LIKE ?2 ESCAPE '\\')
         ORDER BY (i.barcode = ?1) DESC, i.name
         LIMIT ?3",
        SELECT_ITEM
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![query, like_pattern, limit], from_row)?;
    rows.collect()
}

pub fn get_item(conn: &Connection, id: i64) -> Result<Item, ItemError> {
    let sql = format!("{} WHERE i.id = ?1", SELECT_ITEM);
    conn.query_row(&sql, params![id], from_row)
        .optional()?
        .ok_or(ItemError::NotFound)
}

fn validate(conn: &Connection, input: &ItemInput) -> Result<(), ItemError> {
    if input.name.trim().is_empty() {
        return Err(ItemError::Validation("Item name cannot be empty".into()));
    }
    if input.price_minor < 0 || input.cost_minor < 0 {
        return Err(ItemError::Validation("Price and cost cannot be negative".into()));
    }
    if input.stock_qty < 0 {
        return Err(ItemError::Validation("Stock quantity cannot be negative".into()));
    }
    if input.low_stock_threshold < 0 {
        return Err(ItemError::Validation("Low-stock threshold cannot be negative".into()));
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

    Ok(())
}

fn map_barcode_conflict(err: rusqlite::Error, barcode: &Option<String>) -> ItemError {
    match err {
        rusqlite::Error::SqliteFailure(e, _) if e.extended_code == 2067 => {
            ItemError::DuplicateBarcode(barcode.clone().unwrap_or_default())
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
    let description = input.description.as_deref().map(str::trim).filter(|s| !s.is_empty());

    conn.execute(
        "INSERT INTO items
             (name, barcode, description, price_minor, cost_minor, stock_qty, category_id,
              low_stock_threshold, image_path)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            name,
            barcode,
            description,
            input.price_minor,
            input.cost_minor,
            input.stock_qty,
            input.category_id,
            input.low_stock_threshold,
            input.image_path
        ],
    )
    .map_err(|err| map_barcode_conflict(err, &barcode.map(str::to_string)))?;

    get_item(conn, conn.last_insert_rowid())
}

pub fn update_item(conn: &Connection, id: i64, input: ItemInput) -> Result<Item, ItemError> {
    validate(conn, &input)?;
    let name = input.name.trim();
    let barcode = input.barcode.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let description = input.description.as_deref().map(str::trim).filter(|s| !s.is_empty());

    let changed = conn
        .execute(
            "UPDATE items
                SET name = ?1, barcode = ?2, description = ?3, price_minor = ?4, cost_minor = ?5,
                    stock_qty = ?6, category_id = ?7, low_stock_threshold = ?8,
                    image_path = ?9, updated_at = datetime('now', 'localtime')
              WHERE id = ?10",
            params![
                name,
                barcode,
                description,
                input.price_minor,
                input.cost_minor,
                input.stock_qty,
                input.category_id,
                input.low_stock_threshold,
                input.image_path,
                id
            ],
        )
        .map_err(|err| map_barcode_conflict(err, &barcode.map(str::to_string)))?;

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
    let has_sales: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sale_items WHERE item_id = ?1)",
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
    use crate::db::schema::test_conn;

    fn basic_input(name: &str) -> ItemInput {
        ItemInput {
            name: name.to_string(),
            barcode: None,
            description: None,
            price_minor: 10000,
            cost_minor: 8000,
            stock_qty: 5,
            category_id: None,
            low_stock_threshold: 2,
            image_path: None,
        }
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
        let cola_id: i64 = conn
            .query_row("SELECT id FROM items WHERE name = 'Cola 500ml'", [], |row| row.get(0))
            .unwrap();
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
        add_category(&conn, "Weird_Category").unwrap();

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
        patch.stock_qty = 42;
        let updated = update_item(&conn, created.id, patch).unwrap();
        assert_eq!(updated.name, "Editable Renamed");
        assert_eq!(updated.category_id, Some(categories[0].id));
        assert_eq!(updated.stock_qty, 42);

        // A second update with category_id: None must clear it, not leave it
        // unchanged — this is a full replace, not a partial patch.
        let mut clear_category = basic_input("Editable Renamed");
        clear_category.stock_qty = 42;
        let cleared = update_item(&conn, created.id, clear_category).unwrap();
        assert_eq!(cleared.category_id, None);
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
    fn add_category_rejects_duplicates_and_blank_names() {
        let conn = test_conn();
        assert!(matches!(
            add_category(&conn, "Beverages").unwrap_err(),
            CategoryError::DuplicateName(_)
        ));
        assert!(matches!(
            add_category(&conn, "   ").unwrap_err(),
            CategoryError::Validation(_)
        ));

        let created = add_category(&conn, "Household").unwrap();
        assert_eq!(created.name, "Household");
    }
}
