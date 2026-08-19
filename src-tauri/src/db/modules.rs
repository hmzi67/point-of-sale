//! The module-configuration layer — the piece the rest of the product is built
//! on. Nothing here is client-specific: a client's setup is entirely rows in
//! `enabled_modules`, which is why onboarding never needs a code change.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// Which installation surface a visibility flag applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Desktop,
    Android,
}

impl Platform {
    /// The `enabled_modules` column backing this platform.
    fn column(self) -> &'static str {
        match self {
            Platform::Desktop => "desktop_enabled",
            Platform::Android => "android_enabled",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "desktop" => Some(Platform::Desktop),
            "android" => Some(Platform::Android),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleState {
    pub id: i64,
    pub key: String,
    pub name: String,
    pub is_core: bool,
    pub sort_order: i64,
    /// Visibility on the platform this list was requested for.
    pub enabled: bool,
    pub desktop_enabled: bool,
    pub android_enabled: bool,
}

/// Every module in the catalogue with its stored visibility, ordered for the
/// sidebar. Returns all modules (not just enabled ones) so the Settings screen
/// can render the full toggle list from the same call.
pub fn list(conn: &Connection, platform: Platform) -> Result<Vec<ModuleState>, rusqlite::Error> {
    // The platform column is chosen from a closed enum, never from user input.
    let sql = format!(
        "SELECT m.id, m.key, m.name, m.is_core, m.sort_order,
                e.{} AS enabled, e.desktop_enabled, e.android_enabled
           FROM modules m
           JOIN enabled_modules e ON e.module_id = m.id
          ORDER BY m.sort_order",
        platform.column()
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        Ok(ModuleState {
            id: row.get("id")?,
            key: row.get("key")?,
            name: row.get("name")?,
            is_core: row.get::<_, i64>("is_core")? != 0,
            sort_order: row.get("sort_order")?,
            enabled: row.get::<_, i64>("enabled")? != 0,
            desktop_enabled: row.get::<_, i64>("desktop_enabled")? != 0,
            android_enabled: row.get::<_, i64>("android_enabled")? != 0,
        })
    })?;

    rows.collect()
}

#[derive(Debug)]
pub enum ToggleError {
    UnknownModule(String),
    CoreModule(String),
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for ToggleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToggleError::UnknownModule(key) => write!(f, "unknown module: {}", key),
            ToggleError::CoreModule(key) => {
                write!(f, "{} is a core module and cannot be disabled", key)
            }
            ToggleError::Sqlite(err) => write!(f, "database error: {}", err),
        }
    }
}

impl From<rusqlite::Error> for ToggleError {
    fn from(err: rusqlite::Error) -> Self {
        ToggleError::Sqlite(err)
    }
}

/// Sets one module's visibility on one platform. Core modules are refused
/// rather than silently ignored, so a bad Settings UI shows an error instead of
/// appearing to work.
pub fn set_enabled(
    conn: &Connection,
    module_key: &str,
    platform: Platform,
    enabled: bool,
) -> Result<(), ToggleError> {
    let module: Option<(i64, bool)> = conn
        .query_row(
            "SELECT id, is_core FROM modules WHERE key = ?1",
            params![module_key],
            |row| Ok((row.get(0)?, row.get::<_, i64>(1)? != 0)),
        )
        .map(Some)
        .or_else(|err| match err {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;

    let (module_id, is_core) =
        module.ok_or_else(|| ToggleError::UnknownModule(module_key.to_string()))?;

    if is_core && !enabled {
        return Err(ToggleError::CoreModule(module_key.to_string()));
    }

    let sql = format!(
        "UPDATE enabled_modules SET {} = ?1 WHERE module_id = ?2",
        platform.column()
    );
    conn.execute(&sql, params![enabled as i64, module_id])?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::test_conn;

    fn enabled_keys(conn: &Connection, platform: Platform) -> Vec<String> {
        list(conn, platform)
            .unwrap()
            .into_iter()
            .filter(|m| m.enabled)
            .map(|m| m.key)
            .collect()
    }

    #[test]
    fn disabling_a_module_removes_it_from_the_enabled_set() {
        let conn = test_conn();
        assert!(enabled_keys(&conn, Platform::Desktop).contains(&"inventory".to_string()));

        set_enabled(&conn, "inventory", Platform::Desktop, false).unwrap();

        assert!(!enabled_keys(&conn, Platform::Desktop).contains(&"inventory".to_string()));
        // The module itself still exists — only its visibility changed.
        assert_eq!(list(&conn, Platform::Desktop).unwrap().len(), 8);
    }

    #[test]
    fn platforms_toggle_independently() {
        let conn = test_conn();
        set_enabled(&conn, "reports", Platform::Desktop, false).unwrap();

        assert!(!enabled_keys(&conn, Platform::Desktop).contains(&"reports".to_string()));
        assert!(enabled_keys(&conn, Platform::Android).contains(&"reports".to_string()));
    }

    #[test]
    fn core_modules_cannot_be_disabled() {
        let conn = test_conn();
        let err = set_enabled(&conn, "billing", Platform::Desktop, false).unwrap_err();
        assert!(matches!(err, ToggleError::CoreModule(_)));
        assert!(enabled_keys(&conn, Platform::Desktop).contains(&"billing".to_string()));
    }

    #[test]
    fn unknown_module_is_rejected() {
        let conn = test_conn();
        let err = set_enabled(&conn, "loyalty", Platform::Desktop, true).unwrap_err();
        assert!(matches!(err, ToggleError::UnknownModule(_)));
    }
}
