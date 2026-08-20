mod commands;
mod db;
mod images;
mod printer;
mod product_owner_session;
mod session;

use tauri::Manager;

use crate::db::Db;
use crate::product_owner_session::ProductOwnerSession;
use crate::session::Session;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let db = Db::open(dir)?;
            app.manage(db);
            app.manage(Session::new());
            app.manage(ProductOwnerSession::new());
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
            commands::inventory_import_csv,
            commands::inventory_csv_template,
            commands::inventory_upload_image,
            commands::inventory_get_image,
            commands::billing_search_items,
            commands::billing_create_sale,
            commands::billing_get_sale,
            commands::billing_print_receipt_thermal,
            commands::billing_list_recent_sales,
            commands::refund_get_sale,
            commands::refund_create,
            commands::refund_get,
            commands::refund_print_thermal,
            commands::shift_get_open,
            commands::shift_open,
            commands::shift_get_summary,
            commands::shift_close,
            commands::shift_list_recent,
            commands::shift_print_summary,
            commands::reports_get_sales_summary,
            commands::reports_get_top_items,
            commands::reports_get_sales_over_time,
            commands::reports_get_category_sales,
            commands::reports_print_category_sales,
            commands::tables_get_tables,
            commands::tables_add_table,
            commands::tables_update_table_status,
            commands::tables_assign_order_to_table,
            commands::tables_clear_table,
            commands::tables_attach_cart_to_table,
            commands::tables_get_parked_cart,
            commands::attendance_get_employees,
            commands::attendance_check_in,
            commands::attendance_check_out,
            commands::attendance_get_attendance,
            commands::attendance_get_monthly_summary,
            commands::expenses_add_expense,
            commands::expenses_get_expenses,
            commands::expenses_get_expense_categories,
            commands::expenses_get_totals_by_category,
            commands::salary_calculate_salary,
            commands::salary_get_monthly_overview,
            commands::salary_record_payment,
            commands::salary_get_payment_history,
            commands::dashboard_get_summary,
            commands::product_owner_get_status,
            commands::product_owner_setup,
            commands::product_owner_login,
            commands::product_owner_logout,
            commands::product_owner_get_modules,
            commands::product_owner_set_module,
            commands::get_users,
            commands::login,
            commands::logout,
            commands::create_user,
            commands::set_user_pin,
            commands::get_all_users,
            commands::update_user,
            commands::set_user_active
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
