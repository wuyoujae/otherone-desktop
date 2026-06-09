mod ai_test;
mod app_settings;
mod chat;
mod plugin_registry;
mod plugins;
mod session;
mod storage;
mod tools;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            ai_test::test_ai_model,
            app_settings::load_app_settings,
            app_settings::save_engine_settings,
            app_settings::migrate_storage_settings,
            chat::send_chat_message,
            chat::cancel_chat_message,
            plugins::load_plugin_list,
            plugins::install_plugin,
            plugins::uninstall_plugin,
            storage::load_api_configs,
            storage::save_api_configs,
            session::load_sessions,
            session::read_session,
            session::update_session_title
        ])
        .setup(|app| {
            // 初始化插件数据库表
            if let Ok(db) = crate::storage::open_database(&app.handle()) {
                let _ = crate::plugins::init_plugin_db(&db);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
