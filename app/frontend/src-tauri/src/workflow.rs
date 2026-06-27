use chrono::{DateTime, Local, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::AppHandle;

use crate::storage;

static WORKFLOW_TASK_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowTask {
    pub id: String,
    pub series_id: Option<String>,
    pub prompt: String,
    pub title: String,
    pub content: String,
    pub scheduled_at: Option<String>,
    pub start_at: Option<String>,
    pub end_at: Option<String>,
    pub occurrence_date: Option<String>,
    pub time_text: Option<String>,
    pub repeat_start_date: Option<String>,
    pub repeat_end_date: Option<String>,
    pub metadata_json: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkflowTaskRequest {
    pub prompt: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWorkflowTaskStatusRequest {
    pub id: String,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteWorkflowTaskRequest {
    pub id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListWorkflowTasksForRangeRequest {
    pub start_date: String,
    pub end_date: String,
}

pub(crate) fn init_workflow_database(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS workflow_tasks (
            id TEXT PRIMARY KEY,
            series_id TEXT,
            prompt TEXT NOT NULL,
            title TEXT NOT NULL DEFAULT '',
            content TEXT NOT NULL DEFAULT '',
            scheduled_at TEXT,
            start_at TEXT,
            end_at TEXT,
            occurrence_date TEXT,
            time_text TEXT,
            repeat_start_date TEXT,
            repeat_end_date TEXT,
            metadata_json TEXT NOT NULL DEFAULT '{}',
            model_response TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'pending',
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS workflow_task_series (
            id TEXT PRIMARY KEY,
            prompt TEXT NOT NULL,
            title TEXT NOT NULL DEFAULT '',
            content TEXT NOT NULL DEFAULT '',
            schedule_kind TEXT NOT NULL DEFAULT 'single',
            start_date TEXT,
            end_date TEXT,
            weekdays_json TEXT NOT NULL DEFAULT '[]',
            start_time TEXT,
            end_time TEXT,
            timezone TEXT NOT NULL DEFAULT 'local',
            metadata_json TEXT NOT NULL DEFAULT '{}',
            model_response TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE INDEX IF NOT EXISTS idx_workflow_tasks_created
            ON workflow_tasks(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_workflow_tasks_scheduled
            ON workflow_tasks(scheduled_at);
        CREATE INDEX IF NOT EXISTS idx_workflow_tasks_start
            ON workflow_tasks(start_at);
        CREATE INDEX IF NOT EXISTS idx_workflow_tasks_occurrence
            ON workflow_tasks(occurrence_date);
        CREATE INDEX IF NOT EXISTS idx_workflow_tasks_series
            ON workflow_tasks(series_id);
        ",
    )
    .map_err(|error| error.to_string())?;

    ensure_column(conn, "workflow_tasks", "series_id", "ALTER TABLE workflow_tasks ADD COLUMN series_id TEXT")?;
    ensure_column(conn, "workflow_tasks", "title", "ALTER TABLE workflow_tasks ADD COLUMN title TEXT NOT NULL DEFAULT ''")?;
    ensure_column(conn, "workflow_tasks", "content", "ALTER TABLE workflow_tasks ADD COLUMN content TEXT NOT NULL DEFAULT ''")?;
    ensure_column(conn, "workflow_tasks", "scheduled_at", "ALTER TABLE workflow_tasks ADD COLUMN scheduled_at TEXT")?;
    ensure_column(conn, "workflow_tasks", "start_at", "ALTER TABLE workflow_tasks ADD COLUMN start_at TEXT")?;
    ensure_column(conn, "workflow_tasks", "end_at", "ALTER TABLE workflow_tasks ADD COLUMN end_at TEXT")?;
    ensure_column(conn, "workflow_tasks", "occurrence_date", "ALTER TABLE workflow_tasks ADD COLUMN occurrence_date TEXT")?;
    ensure_column(conn, "workflow_tasks", "time_text", "ALTER TABLE workflow_tasks ADD COLUMN time_text TEXT")?;
    ensure_column(conn, "workflow_tasks", "repeat_start_date", "ALTER TABLE workflow_tasks ADD COLUMN repeat_start_date TEXT")?;
    ensure_column(conn, "workflow_tasks", "repeat_end_date", "ALTER TABLE workflow_tasks ADD COLUMN repeat_end_date TEXT")?;
    ensure_column(conn, "workflow_tasks", "metadata_json", "ALTER TABLE workflow_tasks ADD COLUMN metadata_json TEXT NOT NULL DEFAULT '{}'")?;
    ensure_column(conn, "workflow_tasks", "model_response", "ALTER TABLE workflow_tasks ADD COLUMN model_response TEXT NOT NULL DEFAULT ''")?;

    Ok(())
}

#[tauri::command]
pub fn create_workflow_task(
    app: AppHandle,
    request: CreateWorkflowTaskRequest,
) -> Result<WorkflowTask, String> {
    let prompt = request.prompt.trim().to_string();

    if prompt.is_empty() {
        return Err("Task prompt cannot be empty.".to_string());
    }

    let conn = storage::open_database(&app)?;
    storage::init_database(&conn)?;
    init_workflow_database(&conn)?;

    let now = Utc::now().to_rfc3339();
    let task = WorkflowTask {
        id: build_workflow_task_id(),
        series_id: None,
        title: fallback_title(&prompt),
        content: normalize_markdown_list(&prompt),
        prompt,
        scheduled_at: None,
        start_at: None,
        end_at: None,
        occurrence_date: Some(Local::now().format("%Y-%m-%d").to_string()),
        time_text: None,
        repeat_start_date: None,
        repeat_end_date: None,
        metadata_json: "{}".to_string(),
        status: "pending".to_string(),
        created_at: now.clone(),
        updated_at: now,
    };

    insert_workflow_task(&conn, &task, "")?;
    Ok(task)
}

#[tauri::command]
pub fn list_workflow_tasks(app: AppHandle) -> Result<Vec<WorkflowTask>, String> {
    let conn = storage::open_database(&app)?;
    storage::init_database(&conn)?;
    init_workflow_database(&conn)?;

    read_workflow_tasks(&conn)
}

#[tauri::command]
pub fn list_workflow_tasks_for_range(
    app: AppHandle,
    request: ListWorkflowTasksForRangeRequest,
) -> Result<Vec<WorkflowTask>, String> {
    let start = request.start_date.trim().to_string();
    let end = request.end_date.trim().to_string();

    if start.is_empty() || end.is_empty() {
        return Err("Date range cannot be empty.".to_string());
    }

    let tasks = list_workflow_tasks(app)?;
    Ok(tasks
        .into_iter()
        .filter(|task| task_intersects_date_range(task, &start, &end))
        .collect())
}

#[tauri::command]
pub fn update_workflow_task_status(
    app: AppHandle,
    request: UpdateWorkflowTaskStatusRequest,
) -> Result<WorkflowTask, String> {
    let id = request.id.trim();
    let status = request.status.trim();

    if id.is_empty() {
        return Err("Task id cannot be empty.".to_string());
    }

    if !matches!(status, "pending" | "completed") {
        return Err("Unsupported task status.".to_string());
    }

    let conn = storage::open_database(&app)?;
    storage::init_database(&conn)?;
    init_workflow_database(&conn)?;

    let now = Utc::now().to_rfc3339();
    let changed = conn
        .execute(
            "UPDATE workflow_tasks SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![status, now, id],
        )
        .map_err(|error| error.to_string())?;

    if changed == 0 {
        return Err("Task does not exist.".to_string());
    }

    read_workflow_task(&conn, id)
}

#[tauri::command]
pub fn delete_workflow_task(
    app: AppHandle,
    request: DeleteWorkflowTaskRequest,
) -> Result<(), String> {
    let id = request.id.trim();

    if id.is_empty() {
        return Err("Task id cannot be empty.".to_string());
    }

    let conn = storage::open_database(&app)?;
    storage::init_database(&conn)?;
    init_workflow_database(&conn)?;

    let changed = conn
        .execute("DELETE FROM workflow_tasks WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;

    if changed == 0 {
        return Err("Task does not exist.".to_string());
    }

    Ok(())
}

fn read_workflow_tasks(conn: &Connection) -> Result<Vec<WorkflowTask>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, series_id, prompt, title, content, scheduled_at, start_at, end_at, occurrence_date, time_text, repeat_start_date, repeat_end_date, metadata_json, status, created_at, updated_at
             FROM workflow_tasks
             ORDER BY COALESCE(start_at, scheduled_at, created_at) DESC, rowid DESC",
        )
        .map_err(|error| error.to_string())?;

    let rows = stmt
        .query_map([], row_to_workflow_task)
        .map_err(|error| error.to_string())?;
    let mut tasks = Vec::new();

    for row in rows {
        tasks.push(row.map_err(|error| error.to_string())?);
    }

    Ok(tasks)
}

fn read_workflow_task(conn: &Connection, id: &str) -> Result<WorkflowTask, String> {
    conn.query_row(
        "SELECT id, series_id, prompt, title, content, scheduled_at, start_at, end_at, occurrence_date, time_text, repeat_start_date, repeat_end_date, metadata_json, status, created_at, updated_at
         FROM workflow_tasks
         WHERE id = ?1",
        params![id],
        row_to_workflow_task,
    )
    .map_err(|error| error.to_string())
}

fn row_to_workflow_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkflowTask> {
    let prompt: String = row.get(2)?;
    let title: String = row.get(3)?;
    let content: String = row.get(4)?;

    Ok(WorkflowTask {
        id: row.get(0)?,
        series_id: row.get(1)?,
        prompt: prompt.clone(),
        title: if title.trim().is_empty() {
            fallback_title(&prompt)
        } else {
            title
        },
        content: if content.trim().is_empty() {
            normalize_markdown_list(&prompt)
        } else {
            content
        },
        scheduled_at: row.get(5)?,
        start_at: row.get(6)?,
        end_at: row.get(7)?,
        occurrence_date: row.get(8)?,
        time_text: row.get(9)?,
        repeat_start_date: row.get(10)?,
        repeat_end_date: row.get(11)?,
        metadata_json: row.get(12)?,
        status: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
    })
}

fn insert_workflow_task(
    conn: &Connection,
    task: &WorkflowTask,
    model_response: &str,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO workflow_tasks
         (id, series_id, prompt, title, content, scheduled_at, start_at, end_at, occurrence_date, time_text, repeat_start_date, repeat_end_date, metadata_json, model_response, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        params![
            task.id.as_str(),
            task.series_id.as_deref(),
            task.prompt.as_str(),
            task.title.as_str(),
            task.content.as_str(),
            task.scheduled_at.as_deref(),
            task.start_at.as_deref(),
            task.end_at.as_deref(),
            task.occurrence_date.as_deref(),
            task.time_text.as_deref(),
            task.repeat_start_date.as_deref(),
            task.repeat_end_date.as_deref(),
            task.metadata_json.as_str(),
            model_response,
            task.status.as_str(),
            task.created_at.as_str(),
            task.updated_at.as_str(),
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn task_intersects_date_range(task: &WorkflowTask, start: &str, end: &str) -> bool {
    let date = task
        .occurrence_date
        .as_deref()
        .or_else(|| task.start_at.as_deref().and_then(rfc3339_date_key))
        .or_else(|| task.scheduled_at.as_deref().and_then(rfc3339_date_key));

    if let Some(date) = date {
        return date >= start && date <= end;
    }

    if let (Some(repeat_start), Some(repeat_end)) = (
        task.repeat_start_date.as_deref(),
        task.repeat_end_date.as_deref(),
    ) {
        return repeat_start <= end && repeat_end >= start;
    }

    false
}

fn rfc3339_date_key(value: &str) -> Option<&str> {
    DateTime::parse_from_rfc3339(value).ok()?;
    value.get(0..10)
}

fn fallback_title(prompt: &str) -> String {
    let title = prompt
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Untitled task");

    title.chars().take(32).collect()
}

fn normalize_markdown_list(value: &str) -> String {
    let lines = value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            if line.starts_with("- ") {
                line.to_string()
            } else {
                format!("- {line}")
            }
        })
        .collect::<Vec<_>>();

    if lines.is_empty() {
        "- Empty task".to_string()
    } else {
        lines.join("\n")
    }
}

fn ensure_column(
    conn: &Connection,
    table_name: &str,
    column_name: &str,
    alter_statement: &str,
) -> Result<(), String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table_name})"))
        .map_err(|error| error.to_string())?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?;

    for column in columns {
        if column.map_err(|error| error.to_string())? == column_name {
            return Ok(());
        }
    }

    conn.execute(alter_statement, [])
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn build_workflow_task_id() -> String {
    let counter = WORKFLOW_TASK_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("workflow-task-{}-{counter}", Utc::now().timestamp_millis())
}
