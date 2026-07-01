use rusqlite::Connection;
use tauri::AppHandle;

use crate::app_settings;

pub use otherone_backend_core::storage::{ModelConfig, ProviderConfig};

#[tauri::command]
pub fn load_api_configs(app: AppHandle) -> Result<Vec<ProviderConfig>, String> {
    let data_root = app_settings::data_root(&app)?;
    otherone_backend_core::storage::load_api_configs(&data_root)
}

#[tauri::command]
pub fn save_api_configs(app: AppHandle, providers: Vec<ProviderConfig>) -> Result<(), String> {
    let data_root = app_settings::data_root(&app)?;
    otherone_backend_core::storage::save_api_configs(&data_root, providers)
}

pub(crate) fn open_database(app: &AppHandle) -> Result<Connection, String> {
    let data_root = app_settings::data_root(app)?;
    otherone_backend_core::storage::open_database(&data_root)
}

pub(crate) fn init_database(conn: &Connection) -> Result<(), String> {
    otherone_backend_core::storage::init_database(conn)
}
