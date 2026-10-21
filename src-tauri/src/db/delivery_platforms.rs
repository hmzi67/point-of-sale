//! Delivery platform management (Foodpanda, Bykea, etc.) for third-party delivery orders.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryPlatform {
    pub id: i64,
    pub name: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentRider {
    pub name: String,
    pub phone: String,
}

#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("Platform not found")]
    NotFound,
    #[error("A platform named '{0}' already exists")]
    DuplicateName(String),
    #[error("Platform name cannot be empty")]
    EmptyName,
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

fn from_row(row: &rusqlite::Row) -> Result<DeliveryPlatform, rusqlite::Error> {
    Ok(DeliveryPlatform {
        id: row.get("id")?,
        name: row.get("name")?,
        is_active: row.get::<_, i64>("is_active")? != 0,
    })
}

/// Lists all platforms, active first, then alphabetically.
pub fn list_platforms(conn: &Connection) -> Result<Vec<DeliveryPlatform>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, name, is_active FROM delivery_platforms
         ORDER BY is_active DESC, name"
    )?;
    let rows = stmt.query_map([], from_row)?;
    rows.collect()
}

/// Lists only active platforms for the delivery order form.
pub fn list_active_platforms(conn: &Connection) -> Result<Vec<DeliveryPlatform>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, name, is_active FROM delivery_platforms
         WHERE is_active = 1
         ORDER BY name"
    )?;
    let rows = stmt.query_map([], from_row)?;
    rows.collect()
}

/// Adds a new platform.
pub fn add_platform(conn: &Connection, name: &str) -> Result<DeliveryPlatform, PlatformError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(PlatformError::EmptyName);
    }
    
    let existing: Option<String> = conn
        .query_row(
            "SELECT name FROM delivery_platforms WHERE LOWER(name) = LOWER(?1)",
            params![name],
            |row| row.get(0),
        )
        .optional()?;
    
    if let Some(existing_name) = existing {
        return Err(PlatformError::DuplicateName(existing_name));
    }
    
    conn.execute(
        "INSERT INTO delivery_platforms (name) VALUES (?1)",
        params![name],
    )?;
    let id = conn.last_insert_rowid();
    
    Ok(DeliveryPlatform {
        id,
        name: name.to_string(),
        is_active: true,
    })
}

/// Updates a platform's name.
pub fn update_platform(conn: &Connection, id: i64, name: &str) -> Result<DeliveryPlatform, PlatformError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(PlatformError::EmptyName);
    }
    
    // Check if platform exists
    let exists: bool = conn
        .query_row("SELECT 1 FROM delivery_platforms WHERE id = ?1", params![id], |_| Ok(true))
        .optional()?
        .unwrap_or(false);
    
    if !exists {
        return Err(PlatformError::NotFound);
    }
    
    // Check for duplicate name (excluding this platform)
    let duplicate: Option<String> = conn
        .query_row(
            "SELECT name FROM delivery_platforms WHERE LOWER(name) = LOWER(?1) AND id != ?2",
            params![name, id],
            |row| row.get(0),
        )
        .optional()?;
    
    if let Some(dup_name) = duplicate {
        return Err(PlatformError::DuplicateName(dup_name));
    }
    
    conn.execute(
        "UPDATE delivery_platforms SET name = ?1 WHERE id = ?2",
        params![name, id],
    )?;
    
    let platform = conn.query_row(
        "SELECT id, name, is_active FROM delivery_platforms WHERE id = ?1",
        params![id],
        from_row,
    )?;
    
    Ok(platform)
}

/// Toggles a platform's active status.
pub fn set_platform_active(conn: &Connection, id: i64, is_active: bool) -> Result<(), PlatformError> {
    let changed = conn.execute(
        "UPDATE delivery_platforms SET is_active = ?1 WHERE id = ?2",
        params![is_active as i64, id],
    )?;
    
    if changed == 0 {
        return Err(PlatformError::NotFound);
    }
    
    Ok(())
}

/// Gets previously used rider names and phones for a specific platform.
/// Used for autosuggest in the third-party delivery modal.
pub fn get_recent_riders_for_platform(
    conn: &Connection,
    platform_id: i64,
    limit: usize,
) -> Result<Vec<RecentRider>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT delivery_rider_name, delivery_rider_phone
         FROM orders
         WHERE delivery_platform_id = ?1
           AND rider_source = 'manual'
           AND delivery_rider_name IS NOT NULL
           AND delivery_rider_phone IS NOT NULL
         ORDER BY created_at DESC
         LIMIT ?2"
    )?;
    
    let rows = stmt.query_map(params![platform_id, limit as i64], |row| {
        Ok(RecentRider {
            name: row.get(0)?,
            phone: row.get(1)?,
        })
    })?;
    
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::test_conn;
    
    #[test]
    fn add_list_and_deactivate_platform() {
        let conn = test_conn();
        
        // Add a platform
        let bykea = add_platform(&conn, "Bykea").unwrap();
        assert_eq!(bykea.name, "Bykea");
        assert!(bykea.is_active);
        
        // List should include it (Foodpanda seeded + Bykea)
        let platforms = list_platforms(&conn).unwrap();
        assert_eq!(platforms.len(), 2);
        
        // Duplicate name should fail
        assert!(matches!(
            add_platform(&conn, "  Bykea  "),
            Err(PlatformError::DuplicateName(_))
        ));
        
        // Deactivate
        set_platform_active(&conn, bykea.id, false).unwrap();
        let active = list_active_platforms(&conn).unwrap();
        assert!(!active.iter().any(|p| p.id == bykea.id));
    }
    
    #[test]
    fn rejects_empty_name() {
        let conn = test_conn();
        assert!(matches!(add_platform(&conn, ""), Err(PlatformError::EmptyName)));
        assert!(matches!(add_platform(&conn, "   "), Err(PlatformError::EmptyName)));
    }
    
    #[test]
    fn update_renames_and_rejects_duplicates() {
        let conn = test_conn();
        
        let p1 = add_platform(&conn, "Bykea").unwrap();
        let p2 = add_platform(&conn, "Cheetay").unwrap();
        
        // Rename works
        let updated = update_platform(&conn, p1.id, "Bykea Express").unwrap();
        assert_eq!(updated.name, "Bykea Express");
        
        // Duplicate fails
        assert!(matches!(
            update_platform(&conn, p2.id, "Bykea Express"),
            Err(PlatformError::DuplicateName(_))
        ));
        
        // Unknown id fails
        assert!(matches!(
            update_platform(&conn, 999999, "Unknown"),
            Err(PlatformError::NotFound)
        ));
    }

    #[test]
    fn gets_recent_riders_for_platform() {
        let conn = test_conn();
        let p = add_platform(&conn, "Indrive").unwrap();

        // Insert orders with manual riders for this platform
        conn.execute(
            "INSERT INTO orders (order_no, order_type, client_ref, delivery_platform_id, rider_source, delivery_rider_name, delivery_rider_phone)
             VALUES (1, 'third_party_delivery', 'ref-r1', ?1, 'manual', 'Tariq', '03001234567')",
            params![p.id],
        ).unwrap();

        conn.execute(
            "INSERT INTO orders (order_no, order_type, client_ref, delivery_platform_id, rider_source, delivery_rider_name, delivery_rider_phone)
             VALUES (2, 'third_party_delivery', 'ref-r2', ?1, 'manual', 'Rashid', '03117654321')",
            params![p.id],
        ).unwrap();

        let riders = get_recent_riders_for_platform(&conn, p.id, 10).unwrap();
        assert_eq!(riders.len(), 2);
        assert!(riders.iter().any(|r| r.name == "Tariq" && r.phone == "03001234567"));
        assert!(riders.iter().any(|r| r.name == "Rashid" && r.phone == "03117654321"));
    }
}
