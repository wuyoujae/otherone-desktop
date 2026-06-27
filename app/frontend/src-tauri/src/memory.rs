use std::fs;
use std::path::PathBuf;

use otherone::memory::{
    MemoryPoint, MemoryPointKind, MemoryPointStatus, MemoryStorageFile, MemoryTree,
};
use serde::Serialize;
use serde_json::{Map, Value};
use tauri::AppHandle;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryTreeResponse {
    pub storage_path: String,
    pub points: Vec<MemoryPointDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryPointDto {
    pub point_id: String,
    pub parent_id: Option<String>,
    pub kind: MemoryPointKind,
    pub storage: Option<String>,
    pub types: Option<String>,
    pub status: MemoryPointStatus,
    pub created_at: String,
    pub updated_at: String,
    pub attributes: Map<String, Value>,
}

#[tauri::command]
pub fn read_memory_tree(app: AppHandle) -> Result<MemoryTreeResponse, String> {
    let storage_path = memory_storage_path(&app)?;
    let tree = if storage_path.exists() {
        let content = fs::read_to_string(&storage_path)
            .map_err(|error| format!("读取记忆文件失败：{error}"))?;
        let storage_file: MemoryStorageFile =
            serde_json::from_str(&content).map_err(|error| format!("解析记忆文件失败：{error}"))?;

        MemoryTree::from_points(storage_file.points)
            .map_err(|error| format!("构建记忆树失败：{error}"))?
    } else {
        MemoryTree::new()
    };

    Ok(MemoryTreeResponse {
        storage_path: storage_path.to_string_lossy().to_string(),
        points: tree
            .to_points()
            .into_iter()
            .map(MemoryPointDto::from)
            .collect(),
    })
}

fn memory_storage_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(crate::app_settings::dialogue_root(app)?
        .join(".otherone")
        .join("memory")
        .join("long-term-memory.json"))
}

impl From<MemoryPoint> for MemoryPointDto {
    fn from(point: MemoryPoint) -> Self {
        Self {
            point_id: point.point_id,
            parent_id: point.parent_id,
            kind: point.kind,
            storage: point.storage,
            types: point.types,
            status: point.status,
            created_at: point.created_at,
            updated_at: point.updated_at,
            attributes: point.attributes,
        }
    }
}
