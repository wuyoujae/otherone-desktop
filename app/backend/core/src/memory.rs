use std::fs;
use std::path::{Path, PathBuf};

use otherone::memory::{
    MemoryPoint, MemoryPointKind, MemoryPointStatus, MemoryStorageFile, MemoryTree,
};
use serde::Serialize;
use serde_json::{Map, Value};

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

pub fn read_memory_tree(dialogue_root: &Path) -> Result<MemoryTreeResponse, String> {
    let storage_path = memory_storage_path(dialogue_root);
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

pub fn memory_storage_path(dialogue_root: &Path) -> PathBuf {
    dialogue_root
        .join(".otherone")
        .join("memory")
        .join("long-term-memory.json")
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
    fn missing_memory_file_returns_empty_tree() {
        let root = test_dir("memory");
        let response = read_memory_tree(&root).expect("read memory");

        assert!(response.storage_path.ends_with("long-term-memory.json"));
        assert!(!response.points.is_empty());
    }
}
