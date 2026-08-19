mod commands;
mod db;
mod images;
mod printer;

use tauri::Manager;

use crate::db::Db;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let db = Db::open(dir)?;
            app.manage(db);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_ping,
            commands::app_db_version,
            commands::app_db_tables,
            commands::get_app_config,
            commands::update_app_config,
            commands::get_enabled_modules,
            commands::toggle_module,
            commands::inventory_get_items,
            commands::inventory_add_item,
            commands::inventory_update_item,
            commands::inventory_delete_item,
            commands::inventory_get_categories,
            commands::inventory_add_category,
            commands::inventory_upload_image,
            commands::inventory_get_image,
            commands::billing_search_items,
            commands::billing_create_sale,
            commands::billing_get_sale,
            commands::billing_get_tables,
            commands::billing_attach_cart_to_table,
            commands::billing_get_parked_cart,
            commands::billing_print_receipt_thermal,
            commands::reports_get_sales_summary,
            commands::reports_get_top_items,
            commands::reports_get_sales_over_time,
            commands::get_users,
            commands::login,
            commands::create_user,
            commands::set_user_pin
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
