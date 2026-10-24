//! The module-configuration layer — the piece the rest of the product is built
//! on. Nothing here is client-specific: a client's setup is entirely rows in
//! `enabled_modules`, which is why onboarding never needs a code change.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// Every module key, as stored in `modules.key`. Commands pass these to
/// `require_module`, so a typo is a compile error rather than a command that
/// silently refuses to run.
pub mod key {
    pub const DASHBOARD: &str = "dashboard";
    pub const BILLING: &str = "billing";
    pub const INVENTORY: &str = "inventory";
    pub const REPORTS: &str = "reports";
    pub const TABLES: &str = "tables";
    pub const ATTENDANCE: &str = "attendance";
    pub const EXPENSES: &str = "expenses";
    pub const SALARY: &str = "salary";
    pub const SHIFTS: &str = "shifts";
    pub const EMPLOYEES: &str = "employees";
    pub const USERS: &str = "users";
    pub const SETTINGS: &str = "settings";
    pub const DELIVERY: &str = "delivery";
    pub const CUSTOMERS: &str = "customers";
    pub const PURCHASING: &str = "purchasing";
    pub const LEDGER: &str = "ledger";
}

/// Which installation surface a visibility flag applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Desktop,
    Android,
}

impl Platform {
    /// The `enabled_modules` column backing this platform's visibility.
    fn column(self) -> &'static str {
        match self {
            Platform::Desktop => "desktop_enabled",
            Platform::Android => "android_enabled",
        }
    }

    /// The `enabled_modules` column backing this platform's product-owner
    /// lock — see schema.sql's comment on `enabled_modules`.
    fn lock_column(self) -> &'static str {
        match self {
            Platform::Desktop => "desktop_locked",
            Platform::Android => "android_locked",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "desktop" => Some(Platform::Desktop),
            "android" => Some(Platform::Android),
            _ => None,
        }
    }

    /// The platform this binary was actually built for — for the handful of
    /// backend checks (e.g. `commands::billing_print_receipt_thermal`'s
    /// `tables_module_enabled` lookup) that need "what platform is this",
    /// not "what platform did the frontend say" (the frontend already knows
    /// its own platform via `IS_ANDROID`/`PLATFORM` in `types.ts`, but a few
    /// backend-only decisions — like which printer transport to use — have
    /// no frontend-supplied value to read instead).
    ///
    /// iOS shares the `Android` columns: "android" in `enabled_modules` has
    /// always meant "the phone/tablet build" (the frontend's `PLATFORM`
    /// makes the same call), and a vendor configuring a mobile install
    /// shouldn't have to configure it twice.
    pub fn current() -> Self {
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            Platform::Android
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            Platform::Desktop
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
    /// Whether the product owner has locked *this platform's* visibility —
    /// derived the same way `enabled` is (from the requested platform's
    /// column), for the same reason: the Settings screen shouldn't have to
    /// re-derive which of `desktop_locked`/`android_locked` applies to the
    /// platform it's already rendering for.
    pub locked: bool,
    pub desktop_locked: bool,
    pub android_locked: bool,
    /// Whether the licence's plan includes this module (always true here;
    /// `commands::config_get_enabled_modules` fills it from the signed
    /// entitlements). A module can be enabled but not entitled.
    pub entitled: bool,
    /// The cheapest plan that includes it, when `entitled` is false.
    pub required_plan: Option<String>,
}

/// Every module in the catalogue with its stored visibility, ordered for the
/// sidebar. Returns all modules (not just enabled ones) so the Settings screen
/// can render the full toggle list from the same call.
pub fn list(conn: &Connection, platform: Platform) -> Result<Vec<ModuleState>, rusqlite::Error> {
    // The platform column is chosen from a closed enum, never from user input.
    let sql = format!(
        "SELECT m.id, m.key, m.name, m.is_core, m.sort_order,
                e.{} AS enabled, e.desktop_enabled, e.android_enabled,
                e.{} AS locked, e.desktop_locked, e.android_locked
           FROM modules m
           JOIN enabled_modules e ON e.module_id = m.id
          ORDER BY m.sort_order",
        platform.column(),
        platform.lock_column(),
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
            locked: row.get::<_, i64>("locked")? != 0,
            desktop_locked: row.get::<_, i64>("desktop_locked")? != 0,
            android_locked: row.get::<_, i64>("android_locked")? != 0,
            entitled: true,
            required_plan: None,
        })
    })?;

    rows.collect()
}

/// Whether a single module (by key) is enabled for `platform` — a lighter
/// check than `list()` for a caller that only needs one flag, not the whole
/// catalogue (e.g. the receipt printer deciding "Takeaway" vs "Counter
/// Sale"). An unknown key is treated as not-enabled rather than an error —
/// there's nothing sensible for a caller to do differently either way.
pub fn is_enabled(conn: &Connection, key: &str, platform: Platform) -> Result<bool, rusqlite::Error> {
    let sql = format!(
        "SELECT e.{} FROM modules m JOIN enabled_modules e ON e.module_id = m.id WHERE m.key = ?1",
        platform.column()
    );
    match conn.query_row(&sql, params![key], |row| row.get::<_, i64>(0)) {
        Ok(v) => Ok(v != 0),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
        Err(e) => Err(e),
    }
}

/// Shared reads may serve more than one feature; at least one must be enabled.
pub fn require_enabled(conn: &Connection, keys: &[&str], platform: Platform) -> Result<(), String> {
    for key in keys {
        if is_enabled(conn, key, platform).map_err(|error| error.to_string())? {
            return Ok(());
        }
    }
    Err(format!("{} is disabled on this device", keys.join(" / ")))
}

#[derive(Debug, thiserror::Error)]
pub enum ToggleError {
    #[error("unknown module: {0}")]
    UnknownModule(String),
    #[error("{0} is a core module and cannot be disabled")]
    CoreModule(String),
    /// The product owner has locked this module on this platform — see
    /// schema.sql's comment on `enabled_modules`. Distinct from
    /// `CoreModule` because the fix is different: a core module can never
    /// be unlocked (it's architectural), a locked-by-product-owner module
    /// is unlocked by the product owner, not by anything the client does.
    #[error("{0} is locked by the product administrator and cannot be changed here")]
    LockedByProductOwner(String),
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// Looks up a module by key, returning `(module_id, is_core)`.
fn find_module(conn: &Connection, module_key: &str) -> Result<(i64, bool), ToggleError> {
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
    module.ok_or_else(|| ToggleError::UnknownModule(module_key.to_string()))
}

/// Sets one module's visibility on one platform — the client-facing path
/// (Settings screen, Owner/Admin). Core modules are refused rather than
/// silently ignored, so a bad Settings UI shows an error instead of
/// appearing to work; a module the product owner has locked on this
/// platform is refused the same way, with a message that says why, rather
/// than either silently no-op'ing (looks like a bug) or letting the
/// client's toggle quietly overwrite what the product owner set.
pub fn set_enabled(
    conn: &Connection,
    module_key: &str,
    platform: Platform,
    enabled: bool,
) -> Result<(), ToggleError> {
    let (module_id, is_core) = find_module(conn, module_key)?;

    if is_core && !enabled {
        return Err(ToggleError::CoreModule(module_key.to_string()));
    }

    let locked: bool = conn.query_row(
        &format!("SELECT {} FROM enabled_modules WHERE module_id = ?1", platform.lock_column()),
        params![module_id],
        |row| Ok(row.get::<_, i64>(0)? != 0),
    )?;
    if locked {
        return Err(ToggleError::LockedByProductOwner(module_key.to_string()));
    }

    let sql = format!(
        "UPDATE enabled_modules SET {} = ?1 WHERE module_id = ?2",
        platform.column()
    );
    conn.execute(&sql, params![enabled as i64, module_id])?;

    Ok(())
}

/// The product-owner override: sets `enabled` and/or `locked` for one
/// module on one platform, independently — `None` leaves that half
/// unchanged, so the product owner can lock a module at its current state
/// without necessarily flipping it, or unlock one without necessarily
/// changing whether it's on. Bypasses `set_enabled`'s lock check entirely
/// (this *is* the lock-setting authority), and may disable any section, including billing. The UI provides an
/// unavailable landing page when no enabled section is accessible.
pub fn set_by_product_owner(
    conn: &Connection,
    module_key: &str,
    platform: Platform,
    enabled: Option<bool>,
    locked: Option<bool>,
) -> Result<(), ToggleError> {
    let (module_id, _) = find_module(conn, module_key)?;

    if let Some(enabled) = enabled {
        let sql = format!("UPDATE enabled_modules SET {} = ?1 WHERE module_id = ?2", platform.column());
        conn.execute(&sql, params![enabled as i64, module_id])?;
    }
    if let Some(locked) = locked {
        let sql = format!("UPDATE enabled_modules SET {} = ?1 WHERE module_id = ?2", platform.lock_column());
        conn.execute(&sql, params![locked as i64, module_id])?;
    }

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
        assert_eq!(list(&conn, Platform::Desktop).unwrap().len(), 16);
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

    #[test]
    fn a_module_locked_by_the_product_owner_refuses_the_client_s_toggle() {
        let conn = test_conn();
        set_by_product_owner(&conn, "expenses", Platform::Desktop, Some(false), Some(true)).unwrap();

        let err = set_enabled(&conn, "expenses", Platform::Desktop, true).unwrap_err();
        assert!(matches!(err, ToggleError::LockedByProductOwner(_)));
        // The client's attempted change must not have applied either.
        assert!(!enabled_keys(&conn, Platform::Desktop).contains(&"expenses".to_string()));
    }

    #[test]
    fn unlocking_lets_the_client_toggle_again() {
        let conn = test_conn();
        set_by_product_owner(&conn, "expenses", Platform::Desktop, Some(false), Some(true)).unwrap();
        assert!(set_enabled(&conn, "expenses", Platform::Desktop, true).is_err());

        set_by_product_owner(&conn, "expenses", Platform::Desktop, None, Some(false)).unwrap();
        set_enabled(&conn, "expenses", Platform::Desktop, true).unwrap();
        assert!(enabled_keys(&conn, Platform::Desktop).contains(&"expenses".to_string()));
    }

    #[test]
    fn locking_is_per_platform_not_shared() {
        let conn = test_conn();
        set_by_product_owner(&conn, "expenses", Platform::Android, None, Some(true)).unwrap();

        // Android is locked...
        assert!(matches!(
            set_enabled(&conn, "expenses", Platform::Android, true),
            Err(ToggleError::LockedByProductOwner(_))
        ));
        // ...but desktop is untouched and still freely toggleable.
        set_enabled(&conn, "expenses", Platform::Desktop, false).unwrap();
        assert!(!enabled_keys(&conn, Platform::Desktop).contains(&"expenses".to_string()));
    }

    #[test]
    fn product_owner_can_lock_without_changing_the_enabled_state() {
        let conn = test_conn();
        let before = list(&conn, Platform::Desktop).unwrap().into_iter().find(|m| m.key == "expenses").unwrap();
        assert!(!before.locked);

        set_by_product_owner(&conn, "expenses", Platform::Desktop, None, Some(true)).unwrap();

        let after = list(&conn, Platform::Desktop).unwrap().into_iter().find(|m| m.key == "expenses").unwrap();
        assert!(after.locked);
        assert_eq!(after.enabled, before.enabled, "locking alone must not flip the enabled state");
    }

    #[test]
    fn vendor_can_disable_billing_and_the_choice_survives_restart() {
        let conn = test_conn();
        set_by_product_owner(&conn, "billing", Platform::Desktop, Some(false), Some(true)).unwrap();
        crate::db::schema::apply(&conn).unwrap();
        assert!(!is_enabled(&conn, "billing", Platform::Desktop).unwrap());
        assert!(is_enabled(&conn, "billing", Platform::Android).unwrap());
    }

    #[test]
    fn all_administration_modules_support_independent_vendor_controls() {
        let conn = test_conn();
        for key in ["users", "employees", "settings", "salary"] {
            set_by_product_owner(&conn, key, Platform::Desktop, Some(false), Some(true)).unwrap();
            set_by_product_owner(&conn, key, Platform::Android, Some(true), Some(false)).unwrap();
            assert!(require_enabled(&conn, &[key], Platform::Desktop).is_err());
            assert!(require_enabled(&conn, &[key], Platform::Android).is_ok());
            assert!(matches!(set_enabled(&conn, key, Platform::Desktop, true), Err(ToggleError::LockedByProductOwner(_))));
            crate::db::schema::apply(&conn).unwrap();
            assert!(!is_enabled(&conn, key, Platform::Desktop).unwrap());
            assert!(list(&conn, Platform::Desktop).unwrap().iter().find(|module| module.key == key).unwrap().locked);
        }
    }

    #[test]
    fn upgrade_adds_administration_without_overwriting_existing_preferences() {
        let conn = test_conn();
        set_by_product_owner(&conn, "salary", Platform::Desktop, Some(false), Some(true)).unwrap();
        conn.execute("DELETE FROM enabled_modules WHERE module_id IN (SELECT id FROM modules WHERE key IN ('employees', 'users', 'settings'))", []).unwrap();
        conn.execute("DELETE FROM modules WHERE key IN ('employees', 'users', 'settings')", []).unwrap();
        crate::db::schema::apply(&conn).unwrap();
        for key in ["employees", "users", "settings"] {
            assert!(is_enabled(&conn, key, Platform::Desktop).unwrap());
            assert!(is_enabled(&conn, key, Platform::Android).unwrap());
        }
        assert!(!is_enabled(&conn, "salary", Platform::Desktop).unwrap());
    }

    #[test]
    fn product_owner_override_rejects_an_unknown_module() {
        let conn = test_conn();
        let err = set_by_product_owner(&conn, "loyalty", Platform::Desktop, Some(true), None).unwrap_err();
        assert!(matches!(err, ToggleError::UnknownModule(_)));
    }

    #[test]
    fn is_enabled_matches_list_for_a_single_module() {
        let conn = test_conn();
        assert!(is_enabled(&conn, "billing", Platform::Desktop).unwrap());
        // `tables` defaults off on Android (see MODULE_CATALOGUE) but on for desktop.
        assert!(!is_enabled(&conn, "tables", Platform::Android).unwrap());
        assert!(is_enabled(&conn, "tables", Platform::Desktop).unwrap());

        set_enabled(&conn, "tables", Platform::Desktop, false).unwrap();
        assert!(!is_enabled(&conn, "tables", Platform::Desktop).unwrap());
    }

    #[test]
    fn is_enabled_treats_an_unknown_key_as_not_enabled() {
        let conn = test_conn();
        assert!(!is_enabled(&conn, "loyalty", Platform::Desktop).unwrap());
    }

    #[test]
    fn require_enabled_passes_when_any_one_of_several_keys_is_enabled() {
        let conn = test_conn();
        set_enabled(&conn, "reports", Platform::Desktop, false).unwrap();
        assert!(require_enabled(&conn, &["reports", "inventory"], Platform::Desktop).is_ok());
        assert!(require_enabled(&conn, &["inventory", "reports"], Platform::Desktop).is_ok());
    }

    #[test]
    fn require_enabled_fails_naming_every_key_when_none_is_enabled() {
        let conn = test_conn();
        set_enabled(&conn, "reports", Platform::Desktop, false).unwrap();
        set_enabled(&conn, "inventory", Platform::Desktop, false).unwrap();
        assert_eq!(
            require_enabled(&conn, &["reports", "inventory"], Platform::Desktop).unwrap_err(),
            "reports / inventory is disabled on this device"
        );
    }

    #[test]
    fn require_enabled_checks_only_the_given_platform() {
        let conn = test_conn();
        set_enabled(&conn, "reports", Platform::Desktop, false).unwrap();
        assert!(require_enabled(&conn, &["reports"], Platform::Desktop).is_err());
        assert!(require_enabled(&conn, &["reports"], Platform::Android).is_ok());
    }

    #[test]
    fn require_enabled_treats_a_vendor_disabled_core_module_as_disabled() {
        let conn = test_conn();
        set_by_product_owner(&conn, "billing", Platform::Desktop, Some(false), Some(true)).unwrap();
        assert!(require_enabled(&conn, &["billing"], Platform::Desktop).is_err());
    }
}
