use otherone::storage::types::{Entry, StorageSession};
use otherone::Otherone;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::sync::{LazyLock, Mutex};
use tauri::AppHandle;

use crate::app_settings;
use crate::storage;

pub(crate) static LOCALFILE_STORAGE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSessionSummary {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub last_message: String,
    pub message_count: usize,
    pub pinned: bool,
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSessionDetail {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub messages: Vec<AppMessageGroup>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppMessageGroup {
    pub id: String,
    pub role: String,
    pub items: Vec<AppMessageItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppMessageItem {
    pub id: String,
    #[serde(rename = "type")]
    pub item_type: String,
    pub content: Option<String>,
    pub label: Option<String>,
    pub status: String,
    pub detail: Option<String>,
    pub entry_id: String,
    pub source_role: String,
    pub tools: Option<Value>,
    pub token_consumption: Option<u32>,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionTitleRequest {
    pub session_id: String,
    pub title: String,
}

#[derive(Debug, Clone)]
struct SessionMetadata {
    title: Option<String>,
    pinned: bool,
    archived: bool,
}

#[tauri::command]
pub fn load_sessions(app: AppHandle) -> Result<Vec<AppSessionSummary>, String> {
    let conn = open_session_database(&app)?;
    let metadata = load_session_metadata(&conn)?;
    let storage = with_otherone_localfile(&app, Otherone::read_storage_file)?;

    let mut sessions = storage
        .sessions
        .iter()
        .filter(|session| session.status == 0)
        .filter_map(|session| session_to_summary(session, metadata.get(&session.session_id)))
        .filter(|session| !session.archived)
        .collect::<Vec<_>>();

    sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Ok(sessions)
}

#[tauri::command]
pub fn read_session(app: AppHandle, session_id: String) -> Result<AppSessionDetail, String> {
    if session_id.trim().is_empty() {
        return Err("session_id is required".to_string());
    }

    let conn = open_session_database(&app)?;
    let metadata = load_session_metadata(&conn)?;
    let data = with_otherone_localfile(&app, || Otherone::read_session_data(&session_id))?;
    let session = data
        .session
        .ok_or_else(|| "Session not found".to_string())?;

    let title = metadata
        .get(&session.session_id)
        .and_then(|item| item.title.clone())
        .unwrap_or_else(|| derive_title(&data.entries));
    let updated_at = data
        .entries
        .last()
        .map(|entry| entry.create_at.clone())
        .unwrap_or_else(|| session.create_at.clone());

    Ok(AppSessionDetail {
        id: session.session_id,
        title,
        created_at: session.create_at,
        updated_at,
        messages: entries_to_messages(data.entries),
    })
}

#[tauri::command]
pub fn update_session_title(
    app: AppHandle,
    payload: UpdateSessionTitleRequest,
) -> Result<(), String> {
    let session_id = payload.session_id.trim();
    let title = payload.title.trim();

    if session_id.is_empty() {
        return Err("session_id is required".to_string());
    }

    if title.is_empty() {
        return Err("title is required".to_string());
    }

    let conn = open_session_database(&app)?;
    conn.execute(
        "
        INSERT INTO session_metadata (session_id, title, updated_at)
        VALUES (?1, ?2, CURRENT_TIMESTAMP)
        ON CONFLICT(session_id) DO UPDATE SET
            title = excluded.title,
            updated_at = CURRENT_TIMESTAMP
        ",
        params![session_id, title],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

fn session_to_summary(
    session: &StorageSession,
    metadata: Option<&SessionMetadata>,
) -> Option<AppSessionSummary> {
    if metadata.map(|item| item.archived).unwrap_or(false) {
        return None;
    }

    let updated_at = session
        .entries
        .last()
        .map(|entry| entry.create_at.clone())
        .unwrap_or_else(|| session.create_at.clone());
    let title = metadata
        .and_then(|item| item.title.clone())
        .unwrap_or_else(|| derive_title(&session.entries));
    let last_message = session
        .entries
        .last()
        .map(|entry| preview_text(&entry.content, 56))
        .unwrap_or_default();
    let message_count = session
        .entries
        .iter()
        .filter(|entry| entry.status == 0)
        .count();

    Some(AppSessionSummary {
        id: session.session_id.clone(),
        title,
        created_at: session.create_at.clone(),
        updated_at,
        last_message,
        message_count,
        pinned: metadata.map(|item| item.pinned).unwrap_or(false),
        archived: false,
    })
}

fn entries_to_messages(entries: Vec<Entry>) -> Vec<AppMessageGroup> {
    entries
        .into_iter()
        .filter(|entry| entry.status == 0)
        .map(|entry| {
            let role = message_role(&entry.role);
            let item = entry_to_item(&entry);

            AppMessageGroup {
                id: entry.entry_id.clone(),
                role,
                items: vec![item],
            }
        })
        .collect()
}

fn entry_to_item(entry: &Entry) -> AppMessageItem {
    let is_tool = entry.role.eq_ignore_ascii_case("tool");
    let is_thinking =
        entry.role.eq_ignore_ascii_case("thinking") || entry.role.eq_ignore_ascii_case("reasoning");
    let detail = entry
        .tools
        .as_ref()
        .and_then(|tools| serde_json::to_string_pretty(tools).ok());

    AppMessageItem {
        id: format!("item-{}", entry.entry_id),
        item_type: if is_tool {
            "tool"
        } else if is_thinking {
            "thinking"
        } else {
            "text"
        }
        .to_string(),
        content: if is_tool {
            None
        } else {
            Some(entry.content.clone())
        },
        label: if is_tool {
            Some(tool_label(entry))
        } else if is_thinking {
            Some("深度思考".to_string())
        } else {
            None
        },
        status: if entry.status == 0 {
            "completed"
        } else {
            "running"
        }
        .to_string(),
        detail,
        entry_id: entry.entry_id.clone(),
        source_role: entry.role.clone(),
        tools: entry.tools.clone(),
        token_consumption: entry.token_consumption,
        created_at: entry.create_at.clone(),
    }
}

fn message_role(role: &str) -> String {
    if role.eq_ignore_ascii_case("user") {
        "user".to_string()
    } else {
        "ai".to_string()
    }
}

fn tool_label(entry: &Entry) -> String {
    let content = entry.content.trim();

    if content.is_empty() {
        "工具调用已完成".to_string()
    } else {
        content.to_string()
    }
}

fn derive_title(entries: &[Entry]) -> String {
    entries
        .iter()
        .find(|entry| entry.role.eq_ignore_ascii_case("user") && !entry.content.trim().is_empty())
        .or_else(|| {
            entries
                .iter()
                .find(|entry| !entry.content.trim().is_empty())
        })
        .map(|entry| preview_text(&entry.content, 24))
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| "未命名会话".to_string())
}

fn preview_text(content: &str, max_chars: usize) -> String {
    let normalized = content.split_whitespace().collect::<Vec<_>>().join(" ");

    if normalized.chars().count() <= max_chars {
        return normalized;
    }

    let preview = normalized.chars().take(max_chars).collect::<String>();
    format!("{preview}...")
}

fn open_session_database(app: &AppHandle) -> Result<Connection, String> {
    let conn = storage::open_database(app)?;
    storage::init_database(&conn)?;
    init_session_metadata(&conn)?;
    Ok(conn)
}

fn init_session_metadata(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS session_metadata (
            session_id TEXT PRIMARY KEY,
            title TEXT NOT NULL DEFAULT '',
            pinned INTEGER NOT NULL DEFAULT 0,
            archived INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        ",
    )
    .map_err(|error| error.to_string())
}

fn load_session_metadata(conn: &Connection) -> Result<HashMap<String, SessionMetadata>, String> {
    let mut stmt = conn
        .prepare("SELECT session_id, title, pinned, archived FROM session_metadata")
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let session_id: String = row.get(0)?;
            let title: String = row.get(1)?;
            let pinned: i64 = row.get(2)?;
            let archived: i64 = row.get(3)?;

            Ok((
                session_id,
                SessionMetadata {
                    title: if title.trim().is_empty() {
                        None
                    } else {
                        Some(title)
                    },
                    pinned: pinned != 0,
                    archived: archived != 0,
                },
            ))
        })
        .map_err(|error| error.to_string())?;

    let mut metadata = HashMap::new();

    for row in rows {
        let (session_id, item) = row.map_err(|error| error.to_string())?;
        metadata.insert(session_id, item);
    }

    Ok(metadata)
}

fn with_otherone_localfile<T>(
    app: &AppHandle,
    operation: impl FnOnce() -> Result<T, otherone::storage::error::StorageError>,
) -> Result<T, String> {
    let _guard = LOCALFILE_STORAGE_LOCK
        .lock()
        .map_err(|_| "Failed to lock localfile storage".to_string())?;
    let storage_root = agent_storage_root(app)?;

    fs::create_dir_all(&storage_root).map_err(|error| error.to_string())?;
    Otherone::set_localfile_root(&storage_root);

    operation().map_err(|error| error.to_string())
}

pub(crate) fn agent_storage_root(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app_settings::dialogue_root(app)
}
