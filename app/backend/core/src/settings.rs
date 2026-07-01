use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct RuntimePaths {
    settings_path: PathBuf,
    default_data_root: PathBuf,
}

impl RuntimePaths {
    pub fn new(settings_path: impl Into<PathBuf>, default_data_root: impl Into<PathBuf>) -> Self {
        Self {
            settings_path: settings_path.into(),
            default_data_root: default_data_root.into(),
        }
    }

    pub fn settings_path(&self) -> &Path {
        &self.settings_path
    }

    pub fn default_data_root(&self) -> &Path {
        &self.default_data_root
    }
}

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
    #[serde(default)]
    pub workflow_model_id: String,
    #[serde(default = "default_todo_reminder_lead_minutes")]
    pub todo_reminder_lead_minutes: u32,
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

pub fn load_settings(paths: &RuntimePaths) -> Result<AppSettings, String> {
    let path = paths.settings_path();

    if !path.exists() {
        let settings = default_settings(paths)?;
        save_settings(paths, &settings)?;
        return Ok(settings);
    }

    let raw = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut settings =
        serde_json::from_str::<AppSettings>(&raw).map_err(|error| error.to_string())?;
    settings.storage = normalize_storage_settings(paths, settings.storage)?;
    settings.engine = normalize_engine_settings(settings.engine);
    Ok(settings)
}

pub fn save_settings(paths: &RuntimePaths, settings: &AppSettings) -> Result<(), String> {
    let path = paths.settings_path();
    let parent = path
        .parent()
        .ok_or_else(|| "无法解析设置目录。".to_string())?;

    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let raw = serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?;
    fs::write(path, raw).map_err(|error| error.to_string())
}

pub fn save_engine_settings(
    paths: &RuntimePaths,
    request: SaveEngineSettingsRequest,
) -> Result<AppSettings, String> {
    let mut settings = load_settings(paths)?;
    settings.engine = normalize_engine_settings(request.engine);
    save_settings(paths, &settings)?;
    Ok(settings)
}

pub fn data_root(paths: &RuntimePaths) -> Result<PathBuf, String> {
    Ok(PathBuf::from(load_settings(paths)?.storage.data_root))
}

pub fn dialogue_root(paths: &RuntimePaths) -> Result<PathBuf, String> {
    Ok(PathBuf::from(load_settings(paths)?.storage.dialogue_root))
}

pub fn default_settings(paths: &RuntimePaths) -> Result<AppSettings, String> {
    let data_root = absolute_path(paths.default_data_root())?;

    Ok(AppSettings {
        storage: StorageSettings {
            data_root: path_to_string(&data_root),
            artifact_root: path_to_string(&data_root.join("artifacts")),
            dialogue_root: path_to_string(&data_root.join("agent")),
        },
        engine: EngineSettings {
            system_prompt: String::new(),
            max_iterations: 8,
            context_window: 16_000,
            threshold_percentage: 0.8,
            compaction_keep_ratio: 0.35,
            compact_model_id: String::new(),
            workflow_model_id: String::new(),
            todo_reminder_lead_minutes: default_todo_reminder_lead_minutes(),
            default_reasoning_effort: "medium".to_string(),
        },
    })
}

pub fn normalize_storage_settings(
    paths: &RuntimePaths,
    storage: StorageSettings,
) -> Result<StorageSettings, String> {
    let defaults = default_settings(paths)?.storage;

    Ok(StorageSettings {
        data_root: normalize_path_or_default(&storage.data_root, &defaults.data_root)?,
        artifact_root: normalize_path_or_default(&storage.artifact_root, &defaults.artifact_root)?,
        dialogue_root: normalize_path_or_default(&storage.dialogue_root, &defaults.dialogue_root)?,
    })
}

pub fn normalize_engine_settings(engine: EngineSettings) -> EngineSettings {
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
        workflow_model_id: engine.workflow_model_id,
        todo_reminder_lead_minutes: engine.todo_reminder_lead_minutes.clamp(1, 60),
        default_reasoning_effort,
    }
}

fn default_todo_reminder_lead_minutes() -> u32 {
    3
}

fn normalize_path_or_default(value: &str, default_value: &str) -> Result<String, String> {
    let raw = if value.trim().is_empty() {
        default_value
    } else {
        value.trim()
    };

    Ok(path_to_string(&absolute_path(Path::new(raw))?))
}

fn absolute_path(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }

    std::env::current_dir()
        .map_err(|error| error.to_string())
        .map(|cwd| cwd.join(path))
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_dir(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("otherone-core-{name}-{suffix}"));
        std::fs::create_dir_all(&path).expect("create test dir");
        path
    }

    #[test]
    fn load_settings_creates_default_file() {
        let temp = test_dir("settings-default");
        let paths = RuntimePaths::new(temp.join("settings.json"), temp.join("data"));

        let settings = load_settings(&paths).expect("load settings");

        assert!(paths.settings_path().exists());
        assert!(settings.storage.data_root.ends_with("data"));
        assert!(
            settings.storage.artifact_root.ends_with("data\\artifacts")
                || settings.storage.artifact_root.ends_with("data/artifacts")
        );
        assert_eq!(settings.engine.max_iterations, 8);
    }

    #[test]
    fn save_engine_settings_clamps_unsafe_values() {
        let temp = test_dir("settings-engine");
        let paths = RuntimePaths::new(temp.join("settings.json"), temp.join("data"));
        let mut engine = default_settings(&paths).expect("default").engine;
        engine.max_iterations = 0;
        engine.context_window = 12;
        engine.threshold_percentage = 4.0;
        engine.compaction_keep_ratio = 0.0;
        engine.todo_reminder_lead_minutes = 999;
        engine.default_reasoning_effort = "invalid".to_string();

        let settings = save_engine_settings(&paths, SaveEngineSettingsRequest { engine })
            .expect("save engine");

        assert_eq!(settings.engine.max_iterations, 1);
        assert_eq!(settings.engine.context_window, 1024);
        assert_eq!(settings.engine.threshold_percentage, 0.98);
        assert_eq!(settings.engine.compaction_keep_ratio, 0.05);
        assert_eq!(settings.engine.todo_reminder_lead_minutes, 60);
        assert_eq!(settings.engine.default_reasoning_effort, "medium");
    }
}
