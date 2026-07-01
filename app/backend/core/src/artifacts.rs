use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::storage;

static ARTIFACT_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileArtifact {
    pub id: String,
    pub session_id: String,
    pub action: String,
    pub tool_name: String,
    pub name: String,
    pub path: String,
    pub extension: String,
    pub file_path: String,
    pub patch_json: String,
    pub created_at: String,
}

pub fn open_artifact_database(data_root: &Path) -> Result<Connection, String> {
    let conn = storage::open_database(data_root)?;
    storage::init_database(&conn)?;
    init_artifact_database(&conn)?;
    Ok(conn)
}

pub fn init_artifact_database(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS file_artifacts (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            action TEXT NOT NULL,
            tool_name TEXT NOT NULL,
            file_path TEXT NOT NULL,
            file_name TEXT NOT NULL,
            extension TEXT NOT NULL DEFAULT '',
            patch_json TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE INDEX IF NOT EXISTS idx_file_artifacts_session_created
            ON file_artifacts(session_id, created_at DESC);
        ",
    )
    .map_err(|error| error.to_string())
}

pub fn list_file_artifacts(
    data_root: &Path,
    session_id: &str,
) -> Result<Vec<FileArtifact>, String> {
    let session_id = session_id.trim();

    if session_id.is_empty() {
        return Err("Session id cannot be empty.".to_string());
    }

    let conn = open_artifact_database(data_root)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, action, tool_name, file_path, file_name, extension, patch_json, created_at
             FROM file_artifacts
             WHERE session_id = ?1
             ORDER BY created_at DESC, rowid DESC",
        )
        .map_err(|error| error.to_string())?;

    let rows = stmt
        .query_map(params![session_id], row_to_file_artifact)
        .map_err(|error| error.to_string())?;

    let mut artifacts = Vec::new();
    for row in rows {
        artifacts.push(row.map_err(|error| error.to_string())?);
    }

    Ok(artifacts)
}

pub fn read_file_artifact(data_root: &Path, artifact_id: &str) -> Result<FileArtifact, String> {
    let artifact_id = artifact_id.trim();

    if artifact_id.is_empty() {
        return Err("Artifact id cannot be empty.".to_string());
    }

    let conn = open_artifact_database(data_root)?;
    conn.query_row(
        "SELECT id, session_id, action, tool_name, file_path, file_name, extension, patch_json, created_at
         FROM file_artifacts
         WHERE id = ?1",
        params![artifact_id],
        row_to_file_artifact,
    )
    .map_err(|error| error.to_string())
}

pub fn record_file_artifact(
    data_root: &Path,
    session_id: &str,
    action: &str,
    tool_name: &str,
    file_path: &str,
    patch_json: String,
) -> Result<FileArtifact, String> {
    let mut artifact = build_file_artifact(session_id, action, tool_name, file_path, patch_json);
    let conn = open_artifact_database(data_root)?;
    let existing_id = conn
        .query_row(
            "SELECT id FROM file_artifacts
             WHERE session_id = ?1 AND action = ?2 AND file_path = ?3
             LIMIT 1",
            params![artifact.session_id, artifact.action, artifact.file_path],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;

    if let Some(existing_id) = existing_id {
        artifact.id = existing_id;
        conn.execute(
            "UPDATE file_artifacts
             SET tool_name = ?2, file_name = ?3, extension = ?4, patch_json = ?5, created_at = ?6
             WHERE id = ?1",
            params![
                artifact.id,
                artifact.tool_name,
                artifact.name,
                artifact.extension,
                artifact.patch_json,
                artifact.created_at
            ],
        )
        .map_err(|error| error.to_string())?;
    } else {
        conn.execute(
            "INSERT INTO file_artifacts
             (id, session_id, action, tool_name, file_path, file_name, extension, patch_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                artifact.id,
                artifact.session_id,
                artifact.action,
                artifact.tool_name,
                artifact.file_path,
                artifact.name,
                artifact.extension,
                artifact.patch_json,
                artifact.created_at
            ],
        )
        .map_err(|error| error.to_string())?;
    }

    Ok(artifact)
}

fn row_to_file_artifact(row: &rusqlite::Row<'_>) -> rusqlite::Result<FileArtifact> {
    let file_path: String = row.get(4)?;
    Ok(FileArtifact {
        id: row.get(0)?,
        session_id: row.get(1)?,
        action: row.get(2)?,
        tool_name: row.get(3)?,
        name: row.get(5)?,
        path: file_path.clone(),
        extension: row.get(6)?,
        file_path,
        patch_json: row.get(7)?,
        created_at: row.get(8)?,
    })
}

fn build_file_artifact(
    session_id: &str,
    action: &str,
    tool_name: &str,
    file_path: &str,
    patch_json: String,
) -> FileArtifact {
    let path = PathBuf::from(file_path);
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| file_path.to_string());
    let extension = path
        .extension()
        .map(|value| value.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let created_at = Utc::now().to_rfc3339();
    let counter = ARTIFACT_COUNTER.fetch_add(1, Ordering::Relaxed);

    FileArtifact {
        id: format!("file-artifact-{}-{counter}", Utc::now().timestamp_millis()),
        session_id: session_id.to_string(),
        action: action.to_string(),
        tool_name: tool_name.to_string(),
        name,
        path: file_path.to_string(),
        extension,
        file_path: file_path.to_string(),
        patch_json,
        created_at,
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
    fn records_and_lists_file_artifacts() {
        let data_root = test_dir("artifacts");
        let artifact = record_file_artifact(
            &data_root,
            "session-1",
            "edited",
            "edit_file",
            "C:\\work\\demo.rs",
            "[]".to_string(),
        )
        .expect("record artifact");

        assert_eq!(artifact.name, "demo.rs");
        assert_eq!(artifact.extension, "rs");

        let artifacts = list_file_artifacts(&data_root, "session-1").expect("list artifacts");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].id, artifact.id);

        let loaded = read_file_artifact(&data_root, &artifact.id).expect("read artifact");
        assert_eq!(loaded.file_path, artifact.file_path);
    }
}
