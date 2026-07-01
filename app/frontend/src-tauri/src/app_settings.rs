use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use walkdir::WalkDir;

use otherone::Otherone;
use otherone_backend_core::settings as core_settings;

use crate::{chat, plugins, session, weixin_clawbot};

pub use core_settings::{
    AppSettings, EngineSettings, RuntimePaths, SaveEngineSettingsRequest, StorageSettings,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrateStorageSettingsRequest {
    pub storage: StorageSettings,
    pub acknowledged_data_loss_risk: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearAllOtheroneDataRequest {
    pub acknowledged_data_loss_risk: bool,
}

#[tauri::command]
pub fn load_app_settings(app: AppHandle) -> Result<AppSettings, String> {
    load_settings(&app)
}

#[tauri::command]
pub fn save_engine_settings(
    app: AppHandle,
    request: SaveEngineSettingsRequest,
) -> Result<AppSettings, String> {
    core_settings::save_engine_settings(&runtime_paths(&app)?, request)
}

#[tauri::command]
pub fn migrate_storage_settings(
    app: AppHandle,
    request: MigrateStorageSettingsRequest,
) -> Result<AppSettings, String> {
    if !request.acknowledged_data_loss_risk {
        return Err("需要先确认已经了解迁移风险，并建议手动备份旧数据。".to_string());
    }

    if chat::has_active_chat() {
        return Err("当前有对话正在执行，不能迁移存储位置。".to_string());
    }

    let paths = runtime_paths(&app)?;
    let mut settings = core_settings::load_settings(&paths)?;
    let previous = settings.storage.clone();
    let next = core_settings::normalize_storage_settings(&paths, request.storage)?;

    copy_managed_data(&previous, &next)?;
    verify_storage_targets(&next)?;

    settings.storage = next.clone();
    core_settings::save_settings(&paths, &settings)?;

    Ok(settings)
}

#[tauri::command]
pub fn clear_all_otherone_data(
    app: AppHandle,
    request: ClearAllOtheroneDataRequest,
) -> Result<AppSettings, String> {
    if !request.acknowledged_data_loss_risk {
        return Err("需要先确认会清空本地 otherone 数据。".to_string());
    }

    if chat::has_active_chat() {
        return Err("当前有对话正在执行，不能清空本地数据。".to_string());
    }

    let settings = load_settings(&app)?;

    weixin_clawbot::reset_weixin_runtime_state()?;
    clear_managed_data(&app, &settings.storage)?;
    plugins::reset_plugin_runtime_state()?;

    Ok(settings)
}

pub(crate) fn load_settings(app: &AppHandle) -> Result<AppSettings, String> {
    core_settings::load_settings(&runtime_paths(app)?)
}

pub(crate) fn data_root(app: &AppHandle) -> Result<PathBuf, String> {
    core_settings::data_root(&runtime_paths(app)?)
}

pub(crate) fn dialogue_root(app: &AppHandle) -> Result<PathBuf, String> {
    core_settings::dialogue_root(&runtime_paths(app)?)
}

fn runtime_paths(app: &AppHandle) -> Result<RuntimePaths, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    Ok(RuntimePaths::new(
        app_data_dir.join("settings.json"),
        app_data_dir,
    ))
}

fn copy_managed_data(previous: &StorageSettings, next: &StorageSettings) -> Result<(), String> {
    let previous_data = PathBuf::from(&previous.data_root);
    let next_data = PathBuf::from(&next.data_root);

    fs::create_dir_all(&next_data).map_err(|error| error.to_string())?;
    copy_file_if_exists(
        &previous_data.join("otherone.sqlite"),
        &next_data.join("otherone.sqlite"),
    )?;
    copy_dir_if_exists(&previous_data.join("plugins"), &next_data.join("plugins"))?;

    copy_dir_if_exists(
        &PathBuf::from(&previous.dialogue_root),
        &PathBuf::from(&next.dialogue_root),
    )?;
    fs::create_dir_all(&next.artifact_root).map_err(|error| error.to_string())?;
    copy_dir_if_exists(
        &PathBuf::from(&previous.artifact_root),
        &PathBuf::from(&next.artifact_root),
    )?;

    Ok(())
}

fn clear_managed_data(app: &AppHandle, storage: &StorageSettings) -> Result<(), String> {
    let data_root = PathBuf::from(&storage.data_root);
    let artifact_root = PathBuf::from(&storage.artifact_root);
    let dialogue_root = PathBuf::from(&storage.dialogue_root);
    let data_root_absolute = absolute_target_path(&data_root)?;

    fs::create_dir_all(&data_root).map_err(|error| error.to_string())?;
    clear_sqlite_files(&data_root)?;
    clear_directory_contents_if_exists(&data_root.join("plugins"))?;

    ensure_clear_root_is_safe(app, "产物存储目录", &artifact_root, &data_root_absolute)?;
    ensure_clear_root_is_safe(app, "对话数据目录", &dialogue_root, &data_root_absolute)?;

    clear_directory_contents(&artifact_root)?;
    clear_directory_contents(&dialogue_root)?;

    fs::create_dir_all(&data_root).map_err(|error| error.to_string())?;
    fs::create_dir_all(&artifact_root).map_err(|error| error.to_string())?;
    fs::create_dir_all(&dialogue_root).map_err(|error| error.to_string())?;

    let _lock = session::LOCALFILE_STORAGE_LOCK
        .lock()
        .map_err(|_| "无法锁定 otherone localfile 存储。".to_string())?;
    Otherone::set_localfile_root(&dialogue_root);
    otherone::memory::set_memory_storage_root(&dialogue_root);

    Ok(())
}

fn clear_sqlite_files(data_root: &Path) -> Result<(), String> {
    for file_name in [
        "otherone.sqlite",
        "otherone.sqlite-wal",
        "otherone.sqlite-shm",
        "otherone.sqlite-journal",
    ] {
        remove_file_if_exists(&data_root.join(file_name))?;
    }

    Ok(())
}

fn ensure_clear_root_is_safe(
    app: &AppHandle,
    label: &str,
    root: &Path,
    data_root_absolute: &Path,
) -> Result<PathBuf, String> {
    let root_absolute = absolute_target_path(root)?;

    if root_absolute.file_name().is_none() {
        return Err(format!("{label} 不能是磁盘根目录。"));
    }

    if root_absolute == data_root_absolute {
        return Err(format!(
            "{label} 不能与数据存储路径相同，避免误删 settings.json。请先迁移到单独目录。"
        ));
    }

    if data_root_absolute.starts_with(&root_absolute) {
        return Err(format!(
            "{label} 不能包含数据存储路径，避免误删 SQLite 和设置文件。请先迁移到单独目录。"
        ));
    }

    let settings_absolute = absolute_target_path(settings_path(app)?.as_path())?;
    if settings_absolute.starts_with(&root_absolute) {
        return Err(format!(
            "{label} 不能包含 settings.json，避免清空后丢失存储路径配置。"
        ));
    }

    Ok(root_absolute)
}

fn clear_directory_contents_if_exists(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    clear_directory_contents(path)
}

fn clear_directory_contents(path: &Path) -> Result<(), String> {
    if !path.exists() {
        fs::create_dir_all(path).map_err(|error| error.to_string())?;
        return Ok(());
    }

    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() {
        return Err(format!("不能清空符号链接目录：{}", path.display()));
    }

    if !metadata.is_dir() {
        return Err(format!("目标不是目录：{}", path.display()));
    }

    for entry_result in WalkDir::new(path)
        .min_depth(1)
        .contents_first(true)
        .follow_links(false)
    {
        let entry = entry_result.map_err(|error| error.to_string())?;
        let entry_path = entry.path();
        let file_type = entry.file_type();

        if file_type.is_dir() {
            fs::remove_dir(entry_path).map_err(|error| error.to_string())?;
        } else {
            remove_file_if_exists(entry_path)?;
        }
    }

    Ok(())
}

fn remove_file_if_exists(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    fs::remove_file(path).map_err(|error| error.to_string())
}

fn verify_storage_targets(storage: &StorageSettings) -> Result<(), String> {
    fs::create_dir_all(&storage.data_root).map_err(|error| error.to_string())?;
    fs::create_dir_all(&storage.artifact_root).map_err(|error| error.to_string())?;
    fs::create_dir_all(&storage.dialogue_root).map_err(|error| error.to_string())?;

    let db_path = PathBuf::from(&storage.data_root).join("otherone.sqlite");
    if db_path.exists() {
        let conn = rusqlite::Connection::open(&db_path).map_err(|error| error.to_string())?;
        let check_result: String = conn
            .query_row("PRAGMA quick_check", [], |row| row.get(0))
            .map_err(|error| error.to_string())?;
        if !check_result.eq_ignore_ascii_case("ok") {
            return Err(format!("SQLite 校验失败：{check_result}"));
        }
    }

    let localfile = PathBuf::from(&storage.dialogue_root)
        .join(".otherone")
        .join("storage")
        .join("otherone-storage.json");
    if localfile.exists() {
        let raw = fs::read_to_string(localfile).map_err(|error| error.to_string())?;
        serde_json::from_str::<serde_json::Value>(&raw).map_err(|error| error.to_string())?;
    }

    Ok(())
}

fn copy_file_if_exists(source: &Path, target: &Path) -> Result<(), String> {
    if source == target || !source.exists() {
        return Ok(());
    }

    copy_file(source, target)
}

fn copy_file(source: &Path, target: &Path) -> Result<(), String> {
    let parent = target
        .parent()
        .ok_or_else(|| "无法解析目标文件目录。".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    fs::copy(source, target).map_err(|error| error.to_string())?;
    Ok(())
}

fn copy_dir_if_exists(source: &Path, target: &Path) -> Result<(), String> {
    if source == target {
        fs::create_dir_all(target).map_err(|error| error.to_string())?;
        return Ok(());
    }

    if !source.exists() {
        return Ok(());
    }

    ensure_target_is_not_inside_source(source, target)?;

    fs::create_dir_all(target).map_err(|error| error.to_string())?;

    for entry_result in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry_result.map_err(|error| error.to_string())?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());

        if source_path.is_dir() {
            copy_dir_if_exists(&source_path, &target_path)?;
        } else {
            copy_file(&source_path, &target_path)?;
        }
    }

    Ok(())
}

fn ensure_target_is_not_inside_source(source: &Path, target: &Path) -> Result<(), String> {
    let source_absolute = absolute_existing_path(source)?;
    let target_absolute = absolute_target_path(target)?;

    if target_absolute.starts_with(&source_absolute) {
        return Err("新目录不能位于旧目录内部，否则会导致递归复制。".to_string());
    }

    Ok(())
}

fn absolute_existing_path(path: &Path) -> Result<PathBuf, String> {
    path.canonicalize().map_err(|error| error.to_string())
}

fn absolute_target_path(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        return path.canonicalize().map_err(|error| error.to_string());
    }

    let parent = path
        .parent()
        .ok_or_else(|| "无法解析目标目录。".to_string())?;
    let parent_absolute = if parent.exists() {
        parent.canonicalize().map_err(|error| error.to_string())?
    } else {
        absolute_target_path(parent)?
    };

    Ok(path
        .file_name()
        .map(|name| parent_absolute.join(name))
        .unwrap_or(parent_absolute))
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(runtime_paths(app)?.settings_path().to_path_buf())
}
