mod ai_test;
mod app_settings;
mod chat;
mod session;
mod storage;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            ai_test::test_ai_model,
            app_settings::load_app_settings,
            app_settings::save_engine_settings,
            app_settings::migrate_storage_settings,
            chat::send_chat_message,
            storage::load_api_configs,
            storage::save_api_configs,
            session::load_sessions,
            session::read_session,
            session::update_session_title
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
