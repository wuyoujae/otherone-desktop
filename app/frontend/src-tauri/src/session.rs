use std::path::PathBuf;
use tauri::AppHandle;

use crate::app_settings;

pub(crate) use otherone_backend_core::session::LOCALFILE_STORAGE_LOCK;
pub use otherone_backend_core::session::{
    AppSessionDetail, AppSessionSummary, UpdateSessionTitleRequest,
};

#[tauri::command]
pub fn load_sessions(app: AppHandle) -> Result<Vec<AppSessionSummary>, String> {
    let data_root = app_settings::data_root(&app)?;
    let dialogue_root = agent_storage_root(&app)?;
    otherone_backend_core::session::load_sessions(&data_root, &dialogue_root)
}

#[tauri::command]
pub fn read_session(app: AppHandle, session_id: String) -> Result<AppSessionDetail, String> {
    let data_root = app_settings::data_root(&app)?;
    let dialogue_root = agent_storage_root(&app)?;
    otherone_backend_core::session::read_session(&data_root, &dialogue_root, &session_id)
}

#[tauri::command]
pub fn update_session_title(
    app: AppHandle,
    payload: UpdateSessionTitleRequest,
) -> Result<(), String> {
    let data_root = app_settings::data_root(&app)?;
    otherone_backend_core::session::update_session_title(&data_root, payload)
}

pub(crate) fn agent_storage_root(app: &AppHandle) -> Result<PathBuf, String> {
    app_settings::dialogue_root(app)
}
