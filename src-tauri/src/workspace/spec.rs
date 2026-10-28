//! Which tables a business shares between its counters, in what order, and
//! which of their columns stay on the device.
//!
//! Every shared row carries a `gid` — its global id, `<install id>:<local
//! id>` of the counter that created it — and foreign keys travel as the
//! referenced row's gid, so each counter keeps its own local integer ids
//! (nothing in the 40-table schema or its 150 INSERTs had to change).
//!
//! `TABLES` is in dependency order: a table only references tables before
//! it (`tests::tables_are_in_dependency_order` checks this against SQLite's
//! own foreign-key list), which is the order a downloaded batch is applied.
//! The list must match `Diwan\Workspace\SyncEngine::ENTITIES` on the server.

pub struct TableSpec {
    pub name: &'static str,
    pub pk: &'static str,
    /// Columns that belong to this device: never sent, never overwritten.
    pub local_columns: &'static [&'static str],
}

const fn table(name: &'static str) -> TableSpec {
    TableSpec { name, pk: "id", local_columns: &[] }
}

pub const APP_CONFIG: &str = "app_config";
pub const ENABLED_MODULES: &str = "enabled_modules";
pub const ITEMS: &str = "items";

/// The one `app_config` row's gid.
pub const CONFIG_GID: &str = "config";

pub const TABLES: &[TableSpec] = &[
    // Business settings only — see CONFIG_COLUMNS.
    TableSpec { name: APP_CONFIG, pk: "id", local_columns: &[] },
    // Keyed by module key (`module:<key>`): module ids are per install.
    TableSpec { name: ENABLED_MODULES, pk: "module_id", local_columns: &[] },
    table("users"),
    table("categories"),
    table("counters"),
    table("delivery_platforms"),
    table("tables"),
    table("customers"),
    table("suppliers"),
    table("employees"),
    // Stock is central (signed changes, see `local`), so it never travels
    // with the row. The photo's file name does; the file goes through
    // `media`.
    TableSpec { name: ITEMS, pk: "id", local_columns: &["stock_qty"] },
    table("shifts"),
    table("expenses"),
    table("orders"),
    table("sales"),
    table("sale_items"),
    table("sale_payments"),
    table("order_items"),
    table("order_status_events"),
    table("table_orders"),
    table("tokens"),
    table("token_items"),
    table("refunds"),
    table("refund_items"),
    table("customer_payments"),
    table("attendance"),
    table("salary_payments"),
    table("purchase_orders"),
    table("purchase_order_items"),
    table("goods_receipts"),
    table("goods_receipt_items"),
    table("supplier_payments"),
    table("stock_takes"),
    table("stock_take_lines"),
    table("stock_movements"),
];

/// The `app_config` columns that are business settings. Everything else on
/// that row (printer, theme, sidebar, onboarding) is per device. The logo's
/// file goes through `media`, like product photos.
pub const CONFIG_COLUMNS: &[&str] = &[
    "business_name",
    "logo_path",
    "offline_sales",
    "business_type",
    "currency",
    "tax_percent",
    "receipt_footer",
    "working_days_per_month",
    "phone",
    "delivery_number",
    "business_address",
    "business_website",
    "sensitive_area_pin_hash",
    "service_charge_percent",
    "scale_barcode_enabled",
    "scale_barcode_prefix",
    "scale_item_code_digits",
    "scale_value_kind",
    "scale_value_decimals",
];

pub fn find(name: &str) -> Option<&'static TableSpec> {
    TABLES.iter().find(|t| t.name == name)
}

/// Position in `TABLES`, the order a batch is applied in.
pub fn rank(name: &str) -> usize {
    TABLES.iter().position(|t| t.name == name).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_are_in_dependency_order() {
        let conn = crate::db::schema::test_conn();
        for (index, spec) in TABLES.iter().enumerate() {
            let mut stmt = conn.prepare(&format!("SELECT \"table\" FROM pragma_foreign_key_list('{}')", spec.name)).unwrap();
            let parents: Vec<String> = stmt.query_map([], |row| row.get(0)).unwrap().map(Result::unwrap).collect();
            for parent in parents {
                if parent == "modules" {
                    continue; // resolved by module key, not shared
                }
                let parent_rank = rank(&parent);
                assert!(parent_rank < index, "{} references {parent}, which is not earlier in TABLES", spec.name);
            }
        }
    }

    #[test]
    fn every_operational_table_is_either_shared_or_deliberately_local() {
        const DEVICE_ONLY: &[&str] = &["modules", "license_status", "product_owner_account", "pos_sync_meta"];
        let conn = crate::db::schema::test_conn();
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE 'sync_%'")
            .unwrap();
        let tables: Vec<String> = stmt.query_map([], |row| row.get(0)).unwrap().map(Result::unwrap).collect();
        for name in tables {
            assert!(
                find(&name).is_some() || DEVICE_ONLY.contains(&name.as_str()),
                "new table {name}: add it to workspace::spec::TABLES (and the server's SyncEngine::ENTITIES) or to DEVICE_ONLY"
            );
        }
    }

    #[test]
    fn config_columns_exist() {
        let conn = crate::db::schema::test_conn();
        for column in CONFIG_COLUMNS {
            conn.prepare(&format!("SELECT {column} FROM app_config")).unwrap();
        }
    }
}
