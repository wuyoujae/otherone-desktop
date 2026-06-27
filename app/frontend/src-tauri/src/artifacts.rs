use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::Value;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Emitter};

use crate::storage;

const FILE_ARTIFACT_EVENT: &str = "file_artifact_event";
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

pub(crate) fn init_artifact_database(conn: &Connection) -> Result<(), String> {
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

pub(crate) fn record_edit_file_artifact(
    app: &AppHandle,
    session_id: &str,
    tool_result: &str,
) -> Result<(), String> {
    let value: Value = serde_json::from_str(tool_result).map_err(|error| error.to_string())?;
    if value.get("error").is_some() {
        return Ok(());
    }

    let Some(file_path) = value
        .get("filePath")
        .or_else(|| value.get("file_path"))
        .and_then(Value::as_str)
        .filter(|path| !path.trim().is_empty())
    else {
        return Ok(());
    };

    let patch_json = value
        .get("structuredPatch")
        .or_else(|| value.get("structured_patch"))
        .map(Value::to_string)
        .unwrap_or_else(|| "[]".to_string());
    let mut artifact =
        build_file_artifact(session_id, "edited", "edit_file", file_path, patch_json);

    let conn = storage::open_database(app)?;
    storage::init_database(&conn)?;
    init_artifact_database(&conn)?;
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

    app.emit(FILE_ARTIFACT_EVENT, artifact)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_file_artifacts(
    app: AppHandle,
    session_id: String,
) -> Result<Vec<FileArtifact>, String> {
    let conn = storage::open_database(&app)?;
    storage::init_database(&conn)?;
    init_artifact_database(&conn)?;

    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, action, tool_name, file_path, file_name, extension, patch_json, created_at
             FROM file_artifacts
             WHERE session_id = ?1
             ORDER BY created_at DESC, rowid DESC",
        )
        .map_err(|error| error.to_string())?;

    let rows = stmt
        .query_map(params![session_id], |row| {
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
        })
        .map_err(|error| error.to_string())?;

    let mut artifacts = Vec::new();
    for row in rows {
        artifacts.push(row.map_err(|error| error.to_string())?);
    }

    Ok(artifacts)
}

fn build_file_artifact(
    session_id: &str,
    action: &str,
    tool_name: &str,
    file_path: &str,
    patch_json: String,
) -> FileArtifact {
    let path = Path::new(file_path);
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
