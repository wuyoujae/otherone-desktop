mod ai_runtime;
mod ai_test;
mod app_settings;
mod artifacts;
mod chat;
mod memory;
mod native_dialog;
mod plugin_registry;
mod plugins;
mod session;
mod storage;
mod tools;
mod weixin_clawbot;
mod workflow;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            ai_test::test_ai_model,
            app_settings::load_app_settings,
            app_settings::save_engine_settings,
            app_settings::migrate_storage_settings,
            app_settings::clear_all_otherone_data,
            chat::send_chat_message,
            chat::enqueue_chat_message,
            chat::cancel_chat_message,
            memory::read_memory_tree,
            native_dialog::open_directory,
            native_dialog::reveal_file,
            native_dialog::select_directory,
            artifacts::list_file_artifacts,
            plugins::load_plugin_list,
            plugins::import_skill_from_directory,
            plugins::import_skill_from_url,
            plugins::import_mcp_servers,
            plugins::import_mcp_servers_from_url,
            plugins::install_plugin,
            plugins::uninstall_plugin,
            workflow::create_workflow_task,
            workflow::delete_workflow_task,
            workflow::list_workflow_tasks,
            workflow::list_workflow_tasks_for_range,
            workflow::update_workflow_task,
            workflow::update_workflow_task_status,
            weixin_clawbot::weixin_clawbot_status,
            weixin_clawbot::weixin_clawbot_begin_login,
            weixin_clawbot::weixin_clawbot_check_login,
            weixin_clawbot::weixin_clawbot_start,
            weixin_clawbot::weixin_clawbot_stop,
            weixin_clawbot::weixin_clawbot_reset,
            weixin_clawbot::weixin_clawbot_list_events,
            storage::load_api_configs,
            storage::save_api_configs,
            session::load_sessions,
            session::read_session,
            session::update_session_title
        ])
        .setup(|app| {
            if let Ok(db) = crate::storage::open_database(&app.handle()) {
                let _ = crate::plugins::init_plugin_db(&db);
                let _ = crate::artifacts::init_artifact_database(&db);
                let _ = crate::workflow::init_workflow_database(&db);
                let _ = crate::weixin_clawbot::init_weixin_clawbot_database(&db);
            }
            crate::workflow::start_workflow_reminder_loop(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
