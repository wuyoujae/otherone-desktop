use chrono::{DateTime, Duration as ChronoDuration, Local, Utc};
use otherone::ai::types::{Message, MessageContent, ProviderType};
use otherone::Otherone;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration as StdDuration;
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::storage::{self, ModelConfig, ProviderConfig};

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
pub struct ModifyWorkflowTaskRequest {
    pub id: String,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AiTaskUpdateResponse {
    tasks: Vec<AiTaskUpdateItem>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AiTaskUpdateItem {
    title: Option<String>,
    content: Option<String>,
    start_at: Option<String>,
    end_at: Option<String>,
    occurrence_date: Option<String>,
    time_text: Option<String>,
    repeat_start_date: Option<String>,
    repeat_end_date: Option<String>,
    metadata: Option<Value>,
}

struct WorkflowReminderCandidate {
    id: String,
    title: String,
    content: String,
    target_at: String,
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
            reminder_notified_at TEXT,
            reminder_target_at TEXT,
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

    ensure_column(
        conn,
        "workflow_tasks",
        "series_id",
        "ALTER TABLE workflow_tasks ADD COLUMN series_id TEXT",
    )?;
    ensure_column(
        conn,
        "workflow_tasks",
        "title",
        "ALTER TABLE workflow_tasks ADD COLUMN title TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        conn,
        "workflow_tasks",
        "content",
        "ALTER TABLE workflow_tasks ADD COLUMN content TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        conn,
        "workflow_tasks",
        "scheduled_at",
        "ALTER TABLE workflow_tasks ADD COLUMN scheduled_at TEXT",
    )?;
    ensure_column(
        conn,
        "workflow_tasks",
        "start_at",
        "ALTER TABLE workflow_tasks ADD COLUMN start_at TEXT",
    )?;
    ensure_column(
        conn,
        "workflow_tasks",
        "end_at",
        "ALTER TABLE workflow_tasks ADD COLUMN end_at TEXT",
    )?;
    ensure_column(
        conn,
        "workflow_tasks",
        "occurrence_date",
        "ALTER TABLE workflow_tasks ADD COLUMN occurrence_date TEXT",
    )?;
    ensure_column(
        conn,
        "workflow_tasks",
        "time_text",
        "ALTER TABLE workflow_tasks ADD COLUMN time_text TEXT",
    )?;
    ensure_column(
        conn,
        "workflow_tasks",
        "repeat_start_date",
        "ALTER TABLE workflow_tasks ADD COLUMN repeat_start_date TEXT",
    )?;
    ensure_column(
        conn,
        "workflow_tasks",
        "repeat_end_date",
        "ALTER TABLE workflow_tasks ADD COLUMN repeat_end_date TEXT",
    )?;
    ensure_column(
        conn,
        "workflow_tasks",
        "metadata_json",
        "ALTER TABLE workflow_tasks ADD COLUMN metadata_json TEXT NOT NULL DEFAULT '{}'",
    )?;
    ensure_column(
        conn,
        "workflow_tasks",
        "model_response",
        "ALTER TABLE workflow_tasks ADD COLUMN model_response TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        conn,
        "workflow_tasks",
        "reminder_notified_at",
        "ALTER TABLE workflow_tasks ADD COLUMN reminder_notified_at TEXT",
    )?;
    ensure_column(
        conn,
        "workflow_tasks",
        "reminder_target_at",
        "ALTER TABLE workflow_tasks ADD COLUMN reminder_target_at TEXT",
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_workflow_tasks_reminder
         ON workflow_tasks(status, reminder_target_at)",
        [],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

pub(crate) fn start_workflow_reminder_loop(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(StdDuration::from_secs(30));

        loop {
            interval.tick().await;

            if let Err(error) = scan_and_send_workflow_reminders(&app) {
                eprintln!("workflow reminder scan failed: {error}");
            }
        }
    });
}

fn scan_and_send_workflow_reminders(app: &AppHandle) -> Result<(), String> {
    let conn = storage::open_database(app)?;
    storage::init_database(&conn)?;
    init_workflow_database(&conn)?;

    let now = Utc::now();
    let reminders = read_due_workflow_reminders(&conn, now)?;

    for reminder in reminders {
        send_workflow_reminder(app, &reminder)?;
        mark_workflow_reminder_sent(&conn, &reminder)?;
    }

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
pub async fn update_workflow_task(
    app: AppHandle,
    request: ModifyWorkflowTaskRequest,
) -> Result<Vec<WorkflowTask>, String> {
    let id = request.id.trim().to_string();
    let edit_prompt = request.prompt.trim().to_string();

    if id.is_empty() {
        return Err("Task id cannot be empty.".to_string());
    }

    if edit_prompt.is_empty() {
        return Err("Task edit prompt cannot be empty.".to_string());
    }

    let mut conn = storage::open_database(&app)?;
    storage::init_database(&conn)?;
    init_workflow_database(&conn)?;
    let current_task = read_workflow_task(&conn, &id)?;

    let providers = storage::load_api_configs(app.clone())?;
    let (provider, model) = select_workflow_model(&providers)?;
    let model_response =
        invoke_task_update_model(&provider, &model, &current_task, &edit_prompt).await?;
    let parsed = parse_task_update_response(&model_response)?;

    if parsed.tasks.is_empty() {
        return Err("AI returned no updated tasks.".to_string());
    }

    let now = Utc::now().to_rfc3339();
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let mut updated_tasks = Vec::new();

    if parsed.tasks.len() == 1 {
        let task = build_task_from_ai_item(
            id.clone(),
            None,
            edit_prompt.clone(),
            &parsed.tasks[0],
            current_task.status.clone(),
            current_task.created_at.clone(),
            now.clone(),
        )?;

        tx.execute(
            "UPDATE workflow_tasks
             SET series_id = ?1,
                 prompt = ?2,
                 title = ?3,
                 content = ?4,
                 scheduled_at = ?5,
                 start_at = ?6,
                 end_at = ?7,
                 occurrence_date = ?8,
                 time_text = ?9,
                 repeat_start_date = ?10,
                 repeat_end_date = ?11,
                 metadata_json = ?12,
                 model_response = ?13,
                 status = ?14,
                 updated_at = ?15
             WHERE id = ?16",
            params![
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
                model_response.as_str(),
                task.status.as_str(),
                task.updated_at.as_str(),
                id.as_str(),
            ],
        )
        .map_err(|error| error.to_string())?;
        updated_tasks.push(task);
    } else {
        let series_id = build_workflow_series_id();
        tx.execute(
            "DELETE FROM workflow_tasks WHERE id = ?1",
            params![id.as_str()],
        )
        .map_err(|error| error.to_string())?;
        tx.execute(
            "INSERT INTO workflow_task_series
             (id, prompt, title, content, schedule_kind, start_date, end_date, weekdays_json, start_time, end_time, timezone, metadata_json, model_response, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'ai-expanded', ?5, ?6, '[]', NULL, NULL, 'local', ?7, ?8, ?9, ?10)",
            params![
                series_id.as_str(),
                edit_prompt.as_str(),
                parsed.tasks[0].title.as_deref().unwrap_or("Updated task"),
                parsed.tasks[0].content.as_deref().unwrap_or(""),
                parsed.tasks.first().and_then(|task| task.occurrence_date.as_deref()),
                parsed.tasks.last().and_then(|task| task.occurrence_date.as_deref()),
                serde_json::to_string(&parsed.tasks[0].metadata).unwrap_or_else(|_| "{}".to_string()),
                model_response.as_str(),
                now.as_str(),
                now.as_str(),
            ],
        )
        .map_err(|error| error.to_string())?;

        for item in &parsed.tasks {
            let task = build_task_from_ai_item(
                build_workflow_task_id(),
                Some(series_id.clone()),
                edit_prompt.clone(),
                item,
                "pending".to_string(),
                now.clone(),
                now.clone(),
            )?;
            insert_workflow_task(&tx, &task, &model_response)?;
            updated_tasks.push(task);
        }
    }

    tx.commit().map_err(|error| error.to_string())?;
    Ok(updated_tasks)
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

fn read_due_workflow_reminders(
    conn: &Connection,
    now: DateTime<Utc>,
) -> Result<Vec<WorkflowReminderCandidate>, String> {
    let window_end = now + ChronoDuration::minutes(3);
    let mut stmt = conn
        .prepare(
            "SELECT id, title, content, prompt, COALESCE(start_at, scheduled_at) AS target_at
             FROM workflow_tasks
             WHERE status = 'pending'
               AND COALESCE(start_at, scheduled_at) IS NOT NULL
               AND (reminder_target_at IS NULL OR reminder_target_at != COALESCE(start_at, scheduled_at))
             ORDER BY COALESCE(start_at, scheduled_at) ASC",
        )
        .map_err(|error| error.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            let prompt: String = row.get(3)?;
            let title: String = row.get(1)?;
            Ok(WorkflowReminderCandidate {
                id: row.get(0)?,
                title: if title.trim().is_empty() {
                    fallback_title(&prompt)
                } else {
                    title
                },
                content: row.get(2)?,
                target_at: row.get(4)?,
            })
        })
        .map_err(|error| error.to_string())?;
    let mut reminders = Vec::new();

    for row in rows {
        let reminder = row.map_err(|error| error.to_string())?;
        let Some(target_at) = parse_reminder_target(&reminder.target_at) else {
            continue;
        };

        if target_at >= now && target_at <= window_end {
            reminders.push(reminder);
        }
    }

    Ok(reminders)
}

fn send_workflow_reminder(
    app: &AppHandle,
    reminder: &WorkflowReminderCandidate,
) -> Result<(), String> {
    app.notification()
        .builder()
        .title("任务提醒")
        .body(format_workflow_reminder_body(reminder))
        .show()
        .map_err(|error| error.to_string())
}

fn mark_workflow_reminder_sent(
    conn: &Connection,
    reminder: &WorkflowReminderCandidate,
) -> Result<(), String> {
    let notified_at = Utc::now().to_rfc3339();

    conn.execute(
        "UPDATE workflow_tasks
         SET reminder_notified_at = ?1,
             reminder_target_at = ?2
         WHERE id = ?3
           AND status = 'pending'
           AND COALESCE(start_at, scheduled_at) = ?2",
        params![
            notified_at,
            reminder.target_at.as_str(),
            reminder.id.as_str(),
        ],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

fn parse_reminder_target(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.with_timezone(&Utc))
}

fn format_workflow_reminder_body(reminder: &WorkflowReminderCandidate) -> String {
    let time = DateTime::parse_from_rfc3339(&reminder.target_at)
        .ok()
        .map(|date| date.with_timezone(&Local).format("%H:%M").to_string())
        .unwrap_or_else(|| "马上".to_string());
    let content = reminder
        .content
        .lines()
        .map(|line| line.trim().trim_start_matches("- "))
        .find(|line| !line.is_empty());

    if let Some(content) = content {
        format!("{time} 开始：{} - {content}", reminder.title)
    } else {
        format!("{time} 开始：{}", reminder.title)
    }
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

fn build_task_from_ai_item(
    id: String,
    series_id: Option<String>,
    prompt: String,
    item: &AiTaskUpdateItem,
    status: String,
    created_at: String,
    updated_at: String,
) -> Result<WorkflowTask, String> {
    let metadata_json = serde_json::to_string(&item.metadata.clone().unwrap_or(Value::Null))
        .map_err(|error| error.to_string())?;
    let title = item
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| fallback_title(&prompt));
    let content = item
        .content
        .as_deref()
        .map(normalize_markdown_list)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| normalize_markdown_list(&prompt));
    let start_at = item.start_at.as_deref().and_then(normalize_rfc3339);
    let end_at = item.end_at.as_deref().and_then(normalize_rfc3339);
    let occurrence_date = item
        .occurrence_date
        .as_deref()
        .filter(|value| is_date_key(value))
        .map(ToString::to_string)
        .or_else(|| {
            start_at
                .as_deref()
                .and_then(rfc3339_date_key)
                .map(ToString::to_string)
        })
        .or_else(|| Some(Local::now().format("%Y-%m-%d").to_string()));

    Ok(WorkflowTask {
        id,
        series_id,
        prompt,
        title,
        content,
        scheduled_at: start_at.clone(),
        start_at,
        end_at,
        occurrence_date,
        time_text: item.time_text.clone(),
        repeat_start_date: item
            .repeat_start_date
            .clone()
            .filter(|value| is_date_key(value)),
        repeat_end_date: item
            .repeat_end_date
            .clone()
            .filter(|value| is_date_key(value)),
        metadata_json,
        status,
        created_at,
        updated_at,
    })
}

async fn invoke_task_update_model(
    provider: &ProviderConfig,
    model: &ModelConfig,
    current_task: &WorkflowTask,
    edit_prompt: &str,
) -> Result<String, String> {
    let provider_type = parse_provider(&provider.provider)?;
    let api_key = require_text(&provider.api_key, "API Key")?;
    let base_url = require_text(&provider.base_url, "Base URL")?;
    let model_name = require_text(&model.name, "Model name")?;
    let now = Local::now().format("%Y-%m-%d %H:%M:%S %:z").to_string();
    let current_json = serde_json::to_string(current_task).map_err(|error| error.to_string())?;
    let user_prompt = format!(
        r#"Current local time: {now}

Existing task JSON:
{current_json}

User edit instruction:
{edit_prompt}

Return JSON only:
{{
  "tasks": [
    {{
      "title": "short title",
      "content": "- markdown list item",
      "startAt": "RFC3339 datetime or null",
      "endAt": "RFC3339 datetime or null",
      "occurrenceDate": "YYYY-MM-DD",
      "timeText": "original time phrase or null",
      "repeatStartDate": "YYYY-MM-DD or null",
      "repeatEndDate": "YYYY-MM-DD or null",
      "metadata": {{"priority":"high|medium|low|none","summary":"string","tags":["string"]}}
    }}
  ]
}}

Rules:
- Merge the existing task and the edit instruction into final tasks.
- The edit instruction overrides conflicting old data.
- For recurring instructions, expand every occurrence into a concrete task in the tasks array.
- For single tasks, return exactly one task.
- Use RFC3339 datetimes with local timezone offset.
- Do not include explanations, markdown fences, or extra text."#
    );
    let max_tokens = if model.context_window > 0 {
        Some((model.context_window as u32).min(4096))
    } else {
        Some(2048)
    };
    let config = serde_json::json!({
        "model": model_name,
        "messages": [
            Message {
                role: "system".to_string(),
                content: MessageContent::Text("You convert todo edit instructions into strict JSON.".to_string()),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
            Message {
                role: "user".to_string(),
                content: MessageContent::Text(user_prompt),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            }
        ],
        "maxTokens": max_tokens,
        "max_tokens": max_tokens,
        "temperature": Some(0.2_f32),
        "topP": Some(model.top_p as f32),
        "parallelToolCalls": Some(false),
        "stream": Some(false),
    });

    let response = Otherone::invoke_model(provider_type, api_key, base_url, config)
        .await
        .map_err(|error| format!("Task update model call failed: {error}"))?;

    response
        .choices
        .first()
        .and_then(|choice| choice.message.as_ref())
        .and_then(|message| message.content.as_deref())
        .map(str::trim)
        .filter(|content| !content.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| "Task update model returned empty content.".to_string())
}

fn parse_task_update_response(raw: &str) -> Result<AiTaskUpdateResponse, String> {
    let json_text = extract_json_object(raw)
        .ok_or_else(|| "Task update failed: no JSON object found.".to_string())?;
    serde_json::from_str(&json_text)
        .map_err(|error| format!("Task update failed: invalid model JSON: {error}"))
}

fn extract_json_object(raw: &str) -> Option<String> {
    let trimmed = raw.trim();

    if serde_json::from_str::<Value>(trimmed).is_ok() {
        return Some(trimmed.to_string());
    }

    let chars: Vec<char> = trimmed.chars().collect();
    let mut start = None;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;

    for (index, ch) in chars.iter().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }

        if *ch == '\\' && in_string {
            escaped = true;
            continue;
        }

        if *ch == '"' {
            in_string = !in_string;
            continue;
        }

        if in_string {
            continue;
        }

        if *ch == '{' {
            if depth == 0 {
                start = Some(index);
            }
            depth += 1;
            continue;
        }

        if *ch == '}' && depth > 0 {
            depth -= 1;
            if depth == 0 {
                let start_index = start?;
                let candidate: String = chars[start_index..=index].iter().collect();
                if serde_json::from_str::<Value>(&candidate).is_ok() {
                    return Some(candidate);
                }
                start = None;
            }
        }
    }

    None
}

fn select_workflow_model(
    providers: &[ProviderConfig],
) -> Result<(ProviderConfig, ModelConfig), String> {
    let mut first_available: Option<(ProviderConfig, ModelConfig)> = None;

    for provider in providers {
        for model in &provider.models {
            if first_available.is_none() {
                first_available = Some((provider.clone(), model.clone()));
            }

            if model.default_model {
                return Ok((provider.clone(), model.clone()));
            }
        }
    }

    first_available.ok_or_else(|| "Please configure an available workflow model first.".to_string())
}

fn parse_provider(provider: &str) -> Result<ProviderType, String> {
    match provider {
        "OpenAI" | "OpenAI Compatible" => Ok(ProviderType::OpenAI),
        "Anthropic" => Ok(ProviderType::Anthropic),
        "OpenRouter" => Ok(ProviderType::OpenRouter),
        "Fetch" => Ok(ProviderType::Fetch),
        "Local" => Ok(ProviderType::Local),
        _ => Err("Unsupported API provider type.".to_string()),
    }
}

fn require_text<'a>(value: &'a str, label: &str) -> Result<&'a str, String> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        Err(format!("{label} cannot be empty."))
    } else {
        Ok(trimmed)
    }
}

fn normalize_rfc3339(value: &str) -> Option<String> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.to_rfc3339())
}

fn is_date_key(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
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

fn build_workflow_series_id() -> String {
    let counter = WORKFLOW_TASK_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "workflow-series-{}-{counter}",
        Utc::now().timestamp_millis()
    )
}
