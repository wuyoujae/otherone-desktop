use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

use crate::chat;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageSettings {
    pub data_root: String,
    pub artifact_root: String,
    pub dialogue_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineSettings {
    pub system_prompt: String,
    pub max_iterations: u32,
    pub context_window: u32,
    pub threshold_percentage: f32,
    pub compaction_keep_ratio: f32,
    pub compact_model_id: String,
    pub default_reasoning_effort: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub storage: StorageSettings,
    pub engine: EngineSettings,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveEngineSettingsRequest {
    pub engine: EngineSettings,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrateStorageSettingsRequest {
    pub storage: StorageSettings,
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
    let mut settings = load_settings(&app)?;
    settings.engine = normalize_engine_settings(request.engine);
    save_settings(&app, &settings)?;
    Ok(settings)
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

    let mut settings = load_settings(&app)?;
    let previous = settings.storage.clone();
    let next = normalize_storage_settings(&app, request.storage)?;

    copy_managed_data(&previous, &next)?;
    verify_storage_targets(&next)?;

    settings.storage = next.clone();
    save_settings(&app, &settings)?;
    cleanup_previous_storage(&previous, &next)?;

    Ok(settings)
}

pub(crate) fn load_settings(app: &AppHandle) -> Result<AppSettings, String> {
    let path = settings_path(app)?;

    if !path.exists() {
        let settings = default_settings(app)?;
        save_settings(app, &settings)?;
        return Ok(settings);
    }

    let raw = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let mut settings =
        serde_json::from_str::<AppSettings>(&raw).map_err(|error| error.to_string())?;
    settings.storage = normalize_storage_settings(app, settings.storage)?;
    settings.engine = normalize_engine_settings(settings.engine);
    Ok(settings)
}

pub(crate) fn save_settings(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    let path = settings_path(app)?;
    let parent = path
        .parent()
        .ok_or_else(|| "无法解析设置目录。".to_string())?;

    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let raw = serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?;
    fs::write(path, raw).map_err(|error| error.to_string())
}

pub(crate) fn data_root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(PathBuf::from(load_settings(app)?.storage.data_root))
}

pub(crate) fn dialogue_root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(PathBuf::from(load_settings(app)?.storage.dialogue_root))
}

fn default_settings(app: &AppHandle) -> Result<AppSettings, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;

    Ok(AppSettings {
        storage: StorageSettings {
            data_root: path_to_string(&app_data_dir),
            artifact_root: path_to_string(&app_data_dir.join("artifacts")),
            dialogue_root: path_to_string(&app_data_dir.join("agent")),
        },
        engine: EngineSettings {
            system_prompt: String::new(),
            max_iterations: 8,
            context_window: 16_000,
            threshold_percentage: 0.8,
            compaction_keep_ratio: 0.35,
            compact_model_id: String::new(),
            default_reasoning_effort: "medium".to_string(),
        },
    })
}

fn normalize_storage_settings(
    app: &AppHandle,
    storage: StorageSettings,
) -> Result<StorageSettings, String> {
    let defaults = default_settings(app)?.storage;

    Ok(StorageSettings {
        data_root: normalize_path_or_default(&storage.data_root, &defaults.data_root)?,
        artifact_root: normalize_path_or_default(&storage.artifact_root, &defaults.artifact_root)?,
        dialogue_root: normalize_path_or_default(&storage.dialogue_root, &defaults.dialogue_root)?,
    })
}

fn normalize_engine_settings(engine: EngineSettings) -> EngineSettings {
    let max_iterations = engine.max_iterations.clamp(1, 128);
    let context_window = engine.context_window.clamp(1024, 1_000_000);
    let threshold_percentage = engine.threshold_percentage.clamp(0.1, 0.98);
    let compaction_keep_ratio = engine.compaction_keep_ratio.clamp(0.05, 0.95);
    let default_reasoning_effort = match engine.default_reasoning_effort.as_str() {
        "none" | "low" | "medium" | "high" => engine.default_reasoning_effort,
        _ => "medium".to_string(),
    };

    EngineSettings {
        system_prompt: engine.system_prompt,
        max_iterations,
        context_window,
        threshold_percentage,
        compaction_keep_ratio,
        compact_model_id: engine.compact_model_id,
        default_reasoning_effort,
    }
}

fn normalize_path_or_default(value: &str, default_value: &str) -> Result<String, String> {
    let raw = if value.trim().is_empty() {
        default_value
    } else {
        value.trim()
    };

    let path = PathBuf::from(raw);
    let absolute = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(path)
    };

    Ok(path_to_string(&absolute))
}

fn copy_managed_data(previous: &StorageSettings, next: &StorageSettings) -> Result<(), String> {
    let previous_data = PathBuf::from(&previous.data_root);
    let next_data = PathBuf::from(&next.data_root);
    let previous_db = previous_data.join("otherone.sqlite");
    let next_db = next_data.join("otherone.sqlite");

    fs::create_dir_all(&next_data).map_err(|error| error.to_string())?;
    if previous_db.exists() && previous_db != next_db {
        copy_file(&previous_db, &next_db)?;
    }

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

fn cleanup_previous_storage(
    previous: &StorageSettings,
    next: &StorageSettings,
) -> Result<(), String> {
    let previous_data = PathBuf::from(&previous.data_root);
    let next_data = PathBuf::from(&next.data_root);
    let previous_db = previous_data.join("otherone.sqlite");
    let next_db = next_data.join("otherone.sqlite");

    if previous_db.exists() && previous_db != next_db {
        fs::remove_file(&previous_db).map_err(|error| error.to_string())?;
        let _ = fs::remove_dir(&previous_data);
    }

    remove_dir_tree_if_changed(
        &PathBuf::from(&previous.dialogue_root),
        &PathBuf::from(&next.dialogue_root),
    )?;
    remove_dir_tree_if_changed(
        &PathBuf::from(&previous.artifact_root),
        &PathBuf::from(&next.artifact_root),
    )?;

    Ok(())
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
        fs::create_dir_all(target).map_err(|error| error.to_string())?;
        return Ok(());
    }

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

fn remove_dir_tree_if_changed(previous: &Path, next: &Path) -> Result<(), String> {
    if previous == next || !previous.exists() {
        return Ok(());
    }

    fs::remove_dir_all(previous).map_err(|error| error.to_string())
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    Ok(app_data_dir.join("settings.json"))
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}
