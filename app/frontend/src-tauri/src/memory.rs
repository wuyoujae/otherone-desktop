use tauri::AppHandle;

pub use otherone_backend_core::memory::MemoryTreeResponse;

#[tauri::command]
pub fn read_memory_tree(app: AppHandle) -> Result<MemoryTreeResponse, String> {
    let dialogue_root = crate::app_settings::dialogue_root(&app)?;
    otherone_backend_core::memory::read_memory_tree(&dialogue_root)
}
