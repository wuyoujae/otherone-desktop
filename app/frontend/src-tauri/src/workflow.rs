use chrono::{DateTime, Duration as ChronoDuration, Local, Utc};
use chrono::{Datelike, NaiveDate, NaiveTime, TimeZone, Timelike};
use otherone::ai::types::{FunctionDefinition, Message, MessageContent, Tool, ToolChoice};
use otherone::Otherone;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration as StdDuration;
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::ai_runtime;
use crate::app_settings;
use crate::storage::{self, ModelConfig, ProviderConfig};
use crate::weixin_clawbot;

static WORKFLOW_TASK_COUNTER: AtomicU64 = AtomicU64::new(1);
const WEIXIN_REMINDER_LATE_GRACE_MINUTES: i64 = 10;

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
    pub model_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModifyWorkflowTaskRequest {
    pub id: String,
    pub prompt: String,
    pub model_id: Option<String>,
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
pub(crate) struct WorkflowTodoCreateToolInput {
    tasks: Vec<WorkflowTodoTaskToolInput>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkflowTodoTaskToolInput {
    prompt: Option<String>,
    title: Option<String>,
    content: Option<String>,
    start_at: Option<String>,
    scheduled_at: Option<String>,
    end_at: Option<String>,
    occurrence_date: Option<String>,
    time_text: Option<String>,
    repeat_start_date: Option<String>,
    repeat_end_date: Option<String>,
    metadata: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkflowTodoListToolInput {
    start_date: Option<String>,
    end_date: Option<String>,
    status: Option<String>,
    search: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkflowTodoUpdateToolInput {
    id: String,
    prompt: Option<String>,
    title: Option<String>,
    content: Option<String>,
    start_at: Option<String>,
    scheduled_at: Option<String>,
    end_at: Option<String>,
    occurrence_date: Option<String>,
    time_text: Option<String>,
    repeat_start_date: Option<String>,
    repeat_end_date: Option<String>,
    metadata: Option<Value>,
    status: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkflowTodoDeleteToolInput {
    id: String,
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
    scheduled_at: Option<String>,
    end_at: Option<String>,
    start_time: Option<String>,
    end_time: Option<String>,
    occurrence_date: Option<String>,
    date: Option<String>,
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
    reminder_target_at: Option<String>,
    weixin_reminder_target_at: Option<String>,
}

impl WorkflowReminderCandidate {
    fn needs_desktop_reminder(&self) -> bool {
        self.reminder_target_at.as_deref() != Some(self.target_at.as_str())
    }

    fn needs_weixin_reminder(&self) -> bool {
        self.weixin_reminder_target_at.as_deref() != Some(self.target_at.as_str())
    }
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
            weixin_reminder_notified_at TEXT,
            weixin_reminder_target_at TEXT,
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
    ensure_column(
        conn,
        "workflow_tasks",
        "weixin_reminder_notified_at",
        "ALTER TABLE workflow_tasks ADD COLUMN weixin_reminder_notified_at TEXT",
    )?;
    ensure_column(
        conn,
        "workflow_tasks",
        "weixin_reminder_target_at",
        "ALTER TABLE workflow_tasks ADD COLUMN weixin_reminder_target_at TEXT",
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_workflow_tasks_reminder
         ON workflow_tasks(status, reminder_target_at)",
        [],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_workflow_tasks_weixin_reminder
         ON workflow_tasks(status, weixin_reminder_target_at)",
        [],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

pub(crate) fn start_workflow_reminder_loop(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(StdDuration::from_secs(5));

        loop {
            interval.tick().await;

            if let Err(error) = scan_and_send_workflow_reminders(&app).await {
                eprintln!("workflow reminder scan failed: {error}");
            }
        }
    });
}

async fn scan_and_send_workflow_reminders(app: &AppHandle) -> Result<(), String> {
    let settings = app_settings::load_settings(app)?;
    let lead_minutes = settings.engine.todo_reminder_lead_minutes.clamp(1, 60);
    let now = Utc::now();
    let reminders = {
        let conn = open_workflow_database(app)?;
        read_due_workflow_reminders(&conn, now, lead_minutes)?
    };

    for reminder in reminders {
        let target_at = parse_reminder_target(&reminder.target_at);
        let target_is_not_past = target_at.map(|value| value >= now).unwrap_or(false);

        if reminder.needs_desktop_reminder() && target_is_not_past {
            match send_workflow_reminder(app, &reminder) {
                Ok(()) => {
                    let conn = open_workflow_database(app)?;
                    mark_workflow_reminder_sent(&conn, &reminder)?;
                }
                Err(error) => {
                    eprintln!(
                        "workflow desktop reminder send failed for task {}: {error}",
                        reminder.id
                    );
                }
            }
        }

        if reminder.needs_weixin_reminder() {
            weixin_clawbot::weixin_debug(format!(
                "todo reminder due task={} target={} now={} lead_minutes={} late_grace_minutes={}",
                reminder.id,
                reminder.target_at,
                now.to_rfc3339(),
                lead_minutes,
                WEIXIN_REMINDER_LATE_GRACE_MINUTES
            ));
            let body = format_workflow_reminder_body(&reminder);
            let send_result = tokio::time::timeout(
                StdDuration::from_secs(25),
                weixin_clawbot::send_todo_reminder_to_recent_session(app, &body),
            )
            .await
            .map_err(|_| "Weixin Todo reminder send timed out.".to_string());

            match send_result {
                Ok(Ok(true)) => {
                    let conn = open_workflow_database(app)?;
                    mark_workflow_weixin_reminder_sent(&conn, &reminder)?;
                }
                Ok(Ok(false)) => {}
                Ok(Err(error)) | Err(error) => {
                    eprintln!(
                        "workflow weixin reminder send failed for task {}: {error}",
                        reminder.id
                    );
                }
            }
        }
    }

    Ok(())
}

fn open_workflow_database(app: &AppHandle) -> Result<Connection, String> {
    let conn = storage::open_database(app)?;
    storage::init_database(&conn)?;
    init_workflow_database(&conn)?;
    Ok(conn)
}

#[tauri::command]
pub async fn create_workflow_task(
    app: AppHandle,
    request: CreateWorkflowTaskRequest,
) -> Result<WorkflowTask, String> {
    let prompt = request.prompt.trim().to_string();
    let model_id = request
        .model_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    if prompt.is_empty() {
        return Err("Task prompt cannot be empty.".to_string());
    }

    let providers = storage::load_api_configs(app.clone())?;
    let (provider, model) = ai_runtime::select_model_with_fallback(&providers, model_id)?;
    let model_response = invoke_task_create_model(&provider, &model, &prompt).await?;
    let parsed = parse_task_update_response(&model_response)?;

    if parsed.tasks.is_empty() {
        return Err("AI returned no tasks.".to_string());
    }

    let mut conn = storage::open_database(&app)?;
    storage::init_database(&conn)?;
    init_workflow_database(&conn)?;
    let now = Utc::now().to_rfc3339();
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let series_id = if parsed.tasks.len() > 1 {
        let series_id = build_workflow_series_id();
        tx.execute(
            "INSERT INTO workflow_task_series
             (id, prompt, title, content, schedule_kind, start_date, end_date, weekdays_json, start_time, end_time, timezone, metadata_json, model_response, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'ai-expanded', ?5, ?6, '[]', NULL, NULL, 'local', ?7, ?8, ?9, ?10)",
            params![
                series_id.as_str(),
                prompt.as_str(),
                parsed.tasks[0].title.as_deref().unwrap_or("Workflow task"),
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
        Some(series_id)
    } else {
        None
    };
    let mut created_tasks = Vec::new();

    for item in &parsed.tasks {
        let task = build_task_from_ai_item(
            build_workflow_task_id(),
            series_id.clone(),
            prompt.clone(),
            item,
            "pending".to_string(),
            now.clone(),
            now.clone(),
        )?;
        insert_workflow_task(&tx, &task, &model_response)?;
        created_tasks.push(task);
    }

    tx.commit().map_err(|error| error.to_string())?;
    created_tasks
        .into_iter()
        .next()
        .ok_or_else(|| "AI returned no tasks.".to_string())
}

#[tauri::command]
pub async fn update_workflow_task(
    app: AppHandle,
    request: ModifyWorkflowTaskRequest,
) -> Result<Vec<WorkflowTask>, String> {
    let id = request.id.trim().to_string();
    let edit_prompt = request.prompt.trim().to_string();
    let model_id = request
        .model_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

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
    let (provider, model) = ai_runtime::select_model_with_fallback(&providers, model_id)?;
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

pub(crate) fn create_workflow_tasks_from_tool(
    app: &AppHandle,
    request: WorkflowTodoCreateToolInput,
) -> Result<Vec<WorkflowTask>, String> {
    if request.tasks.is_empty() {
        return Err("Todo tool requires at least one task.".to_string());
    }

    let now = Utc::now().to_rfc3339();
    let series_id = if request.tasks.len() > 1 {
        Some(build_workflow_series_id())
    } else {
        None
    };
    let mut created_tasks = Vec::with_capacity(request.tasks.len());

    for input in &request.tasks {
        let prompt = workflow_tool_prompt(input)?;
        let item = workflow_tool_task_to_ai_item(input);
        let task = build_task_from_ai_item(
            build_workflow_task_id(),
            series_id.clone(),
            prompt,
            &item,
            "pending".to_string(),
            now.clone(),
            now.clone(),
        )?;
        created_tasks.push(task);
    }

    let mut conn = storage::open_database(app)?;
    storage::init_database(&conn)?;
    init_workflow_database(&conn)?;
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let model_response = serde_json::json!({
        "source": "chat_agent",
        "tool": "create_todo"
    })
    .to_string();

    if let Some(series_id) = series_id.as_deref() {
        let mut dates: Vec<&str> = created_tasks
            .iter()
            .filter_map(|task| task.occurrence_date.as_deref())
            .collect();
        dates.sort_unstable();
        let first = created_tasks
            .first()
            .ok_or_else(|| "Todo tool requires at least one task.".to_string())?;

        tx.execute(
            "INSERT INTO workflow_task_series
             (id, prompt, title, content, schedule_kind, start_date, end_date, weekdays_json, start_time, end_time, timezone, metadata_json, model_response, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'chat-agent-expanded', ?5, ?6, '[]', NULL, NULL, 'local', ?7, ?8, ?9, ?10)",
            params![
                series_id,
                first.prompt.as_str(),
                first.title.as_str(),
                first.content.as_str(),
                dates.first().copied(),
                dates.last().copied(),
                first.metadata_json.as_str(),
                model_response.as_str(),
                now.as_str(),
                now.as_str(),
            ],
        )
        .map_err(|error| error.to_string())?;
    }

    for task in &created_tasks {
        insert_workflow_task(&tx, task, &model_response)?;
    }

    tx.commit().map_err(|error| error.to_string())?;
    Ok(created_tasks)
}

pub(crate) fn list_workflow_tasks_for_tool(
    app: &AppHandle,
    request: WorkflowTodoListToolInput,
) -> Result<Vec<WorkflowTask>, String> {
    let mut tasks = list_workflow_tasks(app.clone())?;
    let start_date = workflow_tool_date_filter(request.start_date.as_deref(), "startDate")?;
    let end_date = workflow_tool_date_filter(request.end_date.as_deref(), "endDate")?;

    if start_date.is_some() || end_date.is_some() {
        let start = start_date
            .as_deref()
            .or(end_date.as_deref())
            .ok_or_else(|| "Date range cannot be empty.".to_string())?;
        let end = end_date
            .as_deref()
            .or(start_date.as_deref())
            .ok_or_else(|| "Date range cannot be empty.".to_string())?;

        if start > end {
            return Err("startDate cannot be after endDate.".to_string());
        }

        tasks.retain(|task| task_intersects_date_range(task, start, end));
    }

    if let Some(status) = workflow_tool_text(request.status.as_deref()) {
        if !matches!(status.as_str(), "pending" | "completed") {
            return Err("Unsupported task status.".to_string());
        }
        tasks.retain(|task| task.status == status);
    }

    if let Some(search) = workflow_tool_text(request.search.as_deref()) {
        let needle = search.to_lowercase();
        tasks.retain(|task| {
            task.title.to_lowercase().contains(&needle)
                || task.content.to_lowercase().contains(&needle)
                || task.prompt.to_lowercase().contains(&needle)
        });
    }

    let limit = request.limit.unwrap_or(50).clamp(1, 200);
    tasks.truncate(limit);
    Ok(tasks)
}

pub(crate) fn update_workflow_task_from_tool(
    app: &AppHandle,
    request: WorkflowTodoUpdateToolInput,
) -> Result<WorkflowTask, String> {
    let id = request.id.trim().to_string();

    if id.is_empty() {
        return Err("Task id cannot be empty.".to_string());
    }

    let conn = storage::open_database(app)?;
    storage::init_database(&conn)?;
    init_workflow_database(&conn)?;
    let current = read_workflow_task(&conn, &id)?;
    let now = Utc::now().to_rfc3339();
    let prompt = workflow_tool_merge_required(&current.prompt, request.prompt.as_deref());
    let title = workflow_tool_merge_required(&current.title, request.title.as_deref());
    let content = request
        .content
        .as_deref()
        .and_then(workflow_tool_clean_text)
        .map(|value| normalize_markdown_list(&value))
        .unwrap_or_else(|| current.content.clone());
    let occurrence_overridden = request.occurrence_date.is_some();
    let mut occurrence_date = if occurrence_overridden {
        workflow_tool_date_filter(request.occurrence_date.as_deref(), "occurrenceDate")?
    } else {
        current.occurrence_date.clone()
    };
    let start_input = request
        .start_at
        .as_deref()
        .or(request.scheduled_at.as_deref());
    let start_overridden = start_input.is_some();
    let start_at = if let Some(value) = start_input {
        workflow_tool_datetime(value, "startAt", occurrence_date.as_deref())?
    } else {
        current.start_at.clone().or(current.scheduled_at.clone())
    };

    if start_overridden && !occurrence_overridden {
        occurrence_date = start_at
            .as_deref()
            .and_then(rfc3339_date_key)
            .map(ToString::to_string);
    }

    if occurrence_date.is_none() {
        occurrence_date = start_at
            .as_deref()
            .and_then(rfc3339_date_key)
            .map(ToString::to_string);
    }

    let mut end_at = if let Some(value) = request.end_at.as_deref() {
        workflow_tool_datetime(
            value,
            "endAt",
            start_at
                .as_deref()
                .and_then(rfc3339_date_key)
                .or(occurrence_date.as_deref()),
        )?
    } else {
        current.end_at.clone()
    };

    if start_overridden && start_at.is_none() && request.end_at.is_none() {
        end_at = None;
    }

    let time_text = if request.time_text.is_some() {
        workflow_tool_text(request.time_text.as_deref())
    } else {
        current.time_text.clone()
    };
    let repeat_start_date = if request.repeat_start_date.is_some() {
        workflow_tool_date_filter(request.repeat_start_date.as_deref(), "repeatStartDate")?
    } else {
        current.repeat_start_date.clone()
    };
    let repeat_end_date = if request.repeat_end_date.is_some() {
        workflow_tool_date_filter(request.repeat_end_date.as_deref(), "repeatEndDate")?
    } else {
        current.repeat_end_date.clone()
    };
    let metadata_json = if let Some(metadata) = request.metadata.as_ref() {
        serde_json::to_string(metadata).map_err(|error| error.to_string())?
    } else {
        current.metadata_json.clone()
    };
    let status = if let Some(status) = workflow_tool_text(request.status.as_deref()) {
        if !matches!(status.as_str(), "pending" | "completed") {
            return Err("Unsupported task status.".to_string());
        }
        status
    } else {
        current.status.clone()
    };
    let model_response = serde_json::json!({
        "source": "chat_agent",
        "tool": "update_todo"
    })
    .to_string();

    let changed = conn
        .execute(
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
                current.series_id.as_deref(),
                prompt.as_str(),
                title.as_str(),
                content.as_str(),
                start_at.as_deref(),
                start_at.as_deref(),
                end_at.as_deref(),
                occurrence_date.as_deref(),
                time_text.as_deref(),
                repeat_start_date.as_deref(),
                repeat_end_date.as_deref(),
                metadata_json.as_str(),
                model_response.as_str(),
                status.as_str(),
                now.as_str(),
                id.as_str(),
            ],
        )
        .map_err(|error| error.to_string())?;

    if changed == 0 {
        return Err("Task does not exist.".to_string());
    }

    read_workflow_task(&conn, &id)
}

pub(crate) fn delete_workflow_task_from_tool(
    app: &AppHandle,
    request: WorkflowTodoDeleteToolInput,
) -> Result<String, String> {
    let id = request.id.trim().to_string();

    if id.is_empty() {
        return Err("Task id cannot be empty.".to_string());
    }

    delete_workflow_task(app.clone(), DeleteWorkflowTaskRequest { id: id.clone() })?;
    Ok(id)
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
    lead_minutes: u32,
) -> Result<Vec<WorkflowReminderCandidate>, String> {
    let window_end = now + ChronoDuration::minutes(i64::from(lead_minutes));
    let weixin_late_start = now - ChronoDuration::minutes(WEIXIN_REMINDER_LATE_GRACE_MINUTES);
    let mut stmt = conn
        .prepare(
            "SELECT id,
                    title,
                    content,
                    prompt,
                    COALESCE(start_at, scheduled_at) AS target_at,
                    reminder_target_at,
                    weixin_reminder_target_at
             FROM workflow_tasks
             WHERE status = 'pending'
               AND COALESCE(start_at, scheduled_at) IS NOT NULL
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
                reminder_target_at: row.get(5)?,
                weixin_reminder_target_at: row.get(6)?,
            })
        })
        .map_err(|error| error.to_string())?;
    let mut reminders = Vec::new();

    for row in rows {
        let reminder = row.map_err(|error| error.to_string())?;
        let Some(target_at) = parse_reminder_target(&reminder.target_at) else {
            continue;
        };

        let desktop_due =
            reminder.needs_desktop_reminder() && target_at >= now && target_at <= window_end;
        let weixin_due = reminder.needs_weixin_reminder()
            && target_at >= weixin_late_start
            && target_at <= window_end;

        if desktop_due || weixin_due {
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

fn mark_workflow_weixin_reminder_sent(
    conn: &Connection,
    reminder: &WorkflowReminderCandidate,
) -> Result<(), String> {
    let notified_at = Utc::now().to_rfc3339();

    conn.execute(
        "UPDATE workflow_tasks
         SET weixin_reminder_notified_at = ?1,
             weixin_reminder_target_at = ?2
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
        .and_then(floor_datetime_to_minute)
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

fn workflow_tool_prompt(input: &WorkflowTodoTaskToolInput) -> Result<String, String> {
    first_workflow_tool_text([
        input.prompt.as_deref(),
        input.title.as_deref(),
        input.content.as_deref(),
    ])
    .ok_or_else(|| "Todo task requires prompt, title, or content.".to_string())
}

fn workflow_tool_task_to_ai_item(input: &WorkflowTodoTaskToolInput) -> AiTaskUpdateItem {
    AiTaskUpdateItem {
        title: workflow_tool_text(input.title.as_deref()),
        content: workflow_tool_text(input.content.as_deref()),
        start_at: workflow_tool_text(input.start_at.as_deref()),
        scheduled_at: workflow_tool_text(input.scheduled_at.as_deref()),
        end_at: workflow_tool_text(input.end_at.as_deref()),
        start_time: None,
        end_time: None,
        occurrence_date: workflow_tool_text(input.occurrence_date.as_deref()),
        date: None,
        time_text: workflow_tool_text(input.time_text.as_deref()),
        repeat_start_date: workflow_tool_text(input.repeat_start_date.as_deref()),
        repeat_end_date: workflow_tool_text(input.repeat_end_date.as_deref()),
        metadata: input.metadata.clone(),
    }
}

fn first_workflow_tool_text<'a>(
    values: impl IntoIterator<Item = Option<&'a str>>,
) -> Option<String> {
    values
        .into_iter()
        .find_map(|value| value.and_then(workflow_tool_clean_text))
}

fn workflow_tool_text(value: Option<&str>) -> Option<String> {
    value.and_then(workflow_tool_clean_text)
}

fn workflow_tool_clean_text(value: &str) -> Option<String> {
    let value = value.trim();

    if value.is_empty() || value.eq_ignore_ascii_case("null") {
        None
    } else {
        Some(value.to_string())
    }
}

fn workflow_tool_merge_required(current: &str, value: Option<&str>) -> String {
    workflow_tool_text(value).unwrap_or_else(|| current.to_string())
}

fn workflow_tool_date_filter(value: Option<&str>, field: &str) -> Result<Option<String>, String> {
    let Some(value) = workflow_tool_text(value) else {
        return Ok(None);
    };

    if is_date_key(&value) {
        Ok(Some(value))
    } else {
        Err(format!("{field} must be a YYYY-MM-DD date."))
    }
}

fn workflow_tool_datetime(
    value: &str,
    field: &str,
    fallback_date: Option<&str>,
) -> Result<Option<String>, String> {
    let Some(value) = workflow_tool_clean_text(value) else {
        return Ok(None);
    };

    normalize_ai_datetime(Some(&value), fallback_date)
        .map(Some)
        .ok_or_else(|| {
            format!("{field} must be an RFC3339 datetime, or a clock time with an occurrenceDate.")
        })
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
    let occurrence_date = item
        .occurrence_date
        .as_deref()
        .or(item.date.as_deref())
        .filter(|value| is_date_key(value))
        .map(ToString::to_string)
        .or_else(|| {
            parse_date_from_text(item.time_text.as_deref().unwrap_or(&prompt))
                .map(|date| date.format("%Y-%m-%d").to_string())
        })
        .or_else(|| parse_date_from_text(&prompt).map(|date| date.format("%Y-%m-%d").to_string()));
    let start_at = normalize_ai_datetime(
        item.start_at
            .as_deref()
            .or(item.scheduled_at.as_deref())
            .or(item.start_time.as_deref()),
        occurrence_date.as_deref(),
    )
    .or_else(|| {
        parse_start_datetime_from_text(
            &prompt,
            item.time_text.as_deref(),
            occurrence_date.as_deref(),
        )
    });
    let end_at = normalize_ai_datetime(
        item.end_at.as_deref().or(item.end_time.as_deref()),
        start_at
            .as_deref()
            .and_then(rfc3339_date_key)
            .or(occurrence_date.as_deref()),
    )
    .or_else(|| {
        parse_end_datetime_from_text(&prompt, item.time_text.as_deref(), start_at.as_deref())
    });
    let occurrence_date = occurrence_date
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

fn create_todo_tool_def() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: FunctionDefinition {
            name: "create_todo".to_string(),
            description: "Create one or more concrete todo task instances after understanding the user's natural-language instruction.".to_string(),
            parameters: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "tasks": {
                        "type": "array",
                        "minItems": 1,
                        "items": {
                            "type": "object",
                            "properties": {
                                "title": { "type": "string", "description": "Short task title, not the full raw prompt." },
                                "content": { "type": "string", "description": "Markdown list content for the task." },
                                "startAt": { "type": ["string", "null"], "description": "Concrete RFC3339 start datetime with local timezone offset." },
                                "scheduledAt": { "type": ["string", "null"], "description": "Alias for startAt." },
                                "endAt": { "type": ["string", "null"], "description": "Concrete RFC3339 end datetime for time ranges." },
                                "occurrenceDate": { "type": ["string", "null"], "description": "Concrete YYYY-MM-DD date for this task instance." },
                                "timeText": { "type": ["string", "null"], "description": "Original time phrase." },
                                "repeatStartDate": { "type": ["string", "null"] },
                                "repeatEndDate": { "type": ["string", "null"] },
                                "metadata": {
                                    "type": "object",
                                    "properties": {
                                        "priority": { "type": "string", "enum": ["high", "medium", "low", "none"] },
                                        "summary": { "type": "string" },
                                        "tags": { "type": "array", "items": { "type": "string" } }
                                    }
                                }
                            },
                            "required": ["title", "content", "occurrenceDate", "metadata"]
                        }
                    }
                },
                "required": ["tasks"]
            })),
        },
    }
}

fn workflow_create_tool_choice(model: &ModelConfig) -> Result<Option<ToolChoice>, String> {
    if model.tool_choice.trim() == "none" {
        return Err("当前 Todo 模型配置已关闭工具调用，请在模型设置中将 Tool Choice 改成 Auto 或 Required。".to_string());
    }

    Ok(ai_runtime::parse_tool_choice(&model.tool_choice))
}

fn remove_config_key(config: &mut Value, key: &str) {
    if let Value::Object(object) = config {
        object.remove(key);
    }
}

async fn invoke_task_create_model(
    provider: &ProviderConfig,
    model: &ModelConfig,
    prompt: &str,
) -> Result<String, String> {
    let provider_type = ai_runtime::parse_provider(&provider.provider)?;
    let api_key = ai_runtime::require_text(&provider.api_key, "API Key")?;
    let base_url = ai_runtime::require_text(&provider.base_url, "Base URL")?;
    let model_name = ai_runtime::require_text(&model.name, "Model name")?;
    let tool_choice = workflow_create_tool_choice(model)?;
    let now = Local::now().format("%Y-%m-%d %H:%M:%S %:z").to_string();
    let user_prompt = format!(
        r#"Current local time: {now}

User todo instruction:
{prompt}

Rules:
- You must call the create_todo tool exactly once.
- Convert the user instruction into concrete todo task instances.
- For recurring instructions, expand every occurrence into a concrete task in the tasks array.
- For single tasks, return exactly one task.
- If the user instruction contains a time, startAt must be a concrete RFC3339 datetime, not null.
- If the user instruction contains a time range, endAt must be a concrete RFC3339 datetime, not null.
- Use RFC3339 datetimes with local timezone offset.
- Keep title concise and do not copy the full user sentence when a shorter task name is clear.
- Do not include explanations, markdown fences, or extra text."#
    );
    let mut config = serde_json::json!({
        "model": model_name,
        "messages": [
            Message {
                role: "system".to_string(),
                content: MessageContent::Text("You are a todo scheduling parser. Understand the user's natural language and call create_todo with concrete task instances.".to_string()),
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
        "maxTokens": Some(4096_u32),
        "temperature": Some(model.temperature as f32),
        "topP": Some(model.top_p as f32),
        "parallelToolCalls": Some(model.parallel_tool_calls),
        "stream": Some(false),
    });
    ai_runtime::merge_object_params(
        &mut config,
        ai_runtime::build_other_params(&model.extra_params, None)?,
    );
    config["tools"] = serde_json::json!([create_todo_tool_def()]);
    config["parallelToolCalls"] = serde_json::json!(model.parallel_tool_calls);
    config["stream"] = serde_json::json!(false);
    remove_config_key(&mut config, "toolChoice");
    if let Some(tool_choice) = tool_choice {
        config["toolChoice"] =
            serde_json::to_value(tool_choice).map_err(|error| error.to_string())?;
    }

    let response = Otherone::invoke_model(provider_type, api_key, base_url, config)
        .await
        .map_err(|error| format!("Task create model call failed: {error}"))?;

    response
        .choices
        .first()
        .and_then(|choice| choice.message.as_ref())
        .and_then(|message| message.tool_calls.as_ref())
        .and_then(|tool_calls| {
            tool_calls
                .iter()
                .find(|tool_call| tool_call.function.name == "create_todo")
        })
        .map(|tool_call| tool_call.function.arguments.trim().to_string())
        .filter(|arguments| !arguments.is_empty())
        .ok_or_else(|| "Task create model did not call create_todo.".to_string())
}

async fn invoke_task_update_model(
    provider: &ProviderConfig,
    model: &ModelConfig,
    current_task: &WorkflowTask,
    edit_prompt: &str,
) -> Result<String, String> {
    let provider_type = ai_runtime::parse_provider(&provider.provider)?;
    let api_key = ai_runtime::require_text(&provider.api_key, "API Key")?;
    let base_url = ai_runtime::require_text(&provider.base_url, "Base URL")?;
    let model_name = ai_runtime::require_text(&model.name, "Model name")?;
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
- If the final task contains a time, startAt must be a concrete RFC3339 datetime, not null.
- If the final task contains a time range, endAt must be a concrete RFC3339 datetime, not null.
- Use RFC3339 datetimes with local timezone offset.
- Do not include explanations, markdown fences, or extra text."#
    );
    let mut config = serde_json::json!({
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
        "maxTokens": Some(4096_u32),
        "temperature": Some(model.temperature as f32),
        "topP": Some(model.top_p as f32),
        "stream": Some(false),
    });
    ai_runtime::merge_object_params(
        &mut config,
        ai_runtime::build_other_params(&model.extra_params, None)?,
    );
    remove_config_key(&mut config, "tools");
    remove_config_key(&mut config, "toolChoice");
    remove_config_key(&mut config, "parallelToolCalls");
    config["stream"] = serde_json::json!(false);

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

fn normalize_rfc3339(value: &str) -> Option<String> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .and_then(floor_datetime_to_minute)
        .map(|value| value.to_rfc3339())
}

fn floor_datetime_to_minute<Tz: TimeZone>(value: DateTime<Tz>) -> Option<DateTime<Tz>> {
    value.with_second(0)?.with_nanosecond(0)
}

fn normalize_ai_datetime(value: Option<&str>, fallback_date: Option<&str>) -> Option<String> {
    let value = value?.trim();

    if value.is_empty() || value.eq_ignore_ascii_case("null") {
        return None;
    }

    normalize_rfc3339(value).or_else(|| {
        let date = fallback_date.and_then(parse_date_key)?;
        let time = parse_first_time(value)?;
        local_datetime(date, time)
    })
}

fn parse_start_datetime_from_text(
    prompt: &str,
    time_text: Option<&str>,
    fallback_date: Option<&str>,
) -> Option<String> {
    let text = combine_time_text(prompt, time_text);

    if let Some(relative) = parse_relative_after_datetime(&text) {
        return floor_datetime_to_minute(relative).map(|value| value.to_rfc3339());
    }

    let time = parse_first_time(&text)?;
    let date = fallback_date
        .and_then(parse_date_key)
        .or_else(|| parse_date_from_text(&text))
        .unwrap_or_else(|| Local::now().date_naive());

    local_datetime(date, time)
}

fn parse_end_datetime_from_text(
    prompt: &str,
    time_text: Option<&str>,
    start_at: Option<&str>,
) -> Option<String> {
    let start_at = start_at.and_then(|value| DateTime::parse_from_rfc3339(value).ok())?;
    let text = combine_time_text(prompt, time_text);
    let times = parse_time_occurrences(&text);
    let end_time = times.get(1).map(|(time, _)| *time)?;
    let mut end_date = start_at.with_timezone(&Local).date_naive();
    let start_time = start_at.with_timezone(&Local).time();

    if end_time < start_time {
        end_date += ChronoDuration::days(1);
    }

    local_datetime(end_date, end_time)
}

fn combine_time_text(prompt: &str, time_text: Option<&str>) -> String {
    match time_text.map(str::trim).filter(|value| !value.is_empty()) {
        Some(time_text) => format!("{time_text} {prompt}"),
        None => prompt.to_string(),
    }
}

fn parse_relative_after_datetime(text: &str) -> Option<DateTime<Local>> {
    if text.contains("半小时后") || text.contains("半个小时后") {
        return Some(Local::now() + ChronoDuration::minutes(30));
    }

    let re = regex::Regex::new(r"(?:(\d+)\s*小时)?\s*(?:(\d+)\s*分钟)?\s*后").ok()?;
    let caps = re.captures(text)?;
    let hours = caps
        .get(1)
        .and_then(|value| value.as_str().parse::<i64>().ok())
        .unwrap_or(0);
    let minutes = caps
        .get(2)
        .and_then(|value| value.as_str().parse::<i64>().ok())
        .unwrap_or(0);

    if hours == 0 && minutes == 0 {
        None
    } else {
        Some(Local::now() + ChronoDuration::hours(hours) + ChronoDuration::minutes(minutes))
    }
}

fn parse_date_from_text(text: &str) -> Option<NaiveDate> {
    let today = Local::now().date_naive();

    if text.contains("大后天") {
        return Some(today + ChronoDuration::days(3));
    }

    if text.contains("后天") {
        return Some(today + ChronoDuration::days(2));
    }

    if text.contains("明天") || text.contains("明早") || text.contains("明晚") {
        return Some(today + ChronoDuration::days(1));
    }

    if text.contains("今天") || text.contains("今晚") || text.contains("今早") {
        return Some(today);
    }

    if let Some(date) = parse_month_day_from_text(text, today.year()) {
        return Some(date);
    }

    parse_next_weekday_from_text(text, today)
}

fn parse_month_day_from_text(text: &str, current_year: i32) -> Option<NaiveDate> {
    let re =
        regex::Regex::new(r"(?:(\d{4})\s*年)?\s*(\d{1,2})\s*月\s*(\d{1,2})\s*(?:日|号)?").ok()?;
    let caps = re.captures(text)?;
    let year = caps
        .get(1)
        .and_then(|value| value.as_str().parse::<i32>().ok())
        .unwrap_or(current_year);
    let month = caps.get(2)?.as_str().parse::<u32>().ok()?;
    let day = caps.get(3)?.as_str().parse::<u32>().ok()?;

    NaiveDate::from_ymd_opt(year, month, day)
}

fn parse_next_weekday_from_text(text: &str, today: NaiveDate) -> Option<NaiveDate> {
    let re = regex::Regex::new(r"下周\s*([一二三四五六日天])").ok()?;
    let caps = re.captures(text)?;
    let target = chinese_weekday_index(caps.get(1)?.as_str())?;
    let current = today.weekday().num_days_from_monday() as i64;
    let delta = ((target as i64 - current + 7) % 7) + 7;

    Some(today + ChronoDuration::days(delta))
}

fn chinese_weekday_index(value: &str) -> Option<u32> {
    match value {
        "一" => Some(0),
        "二" => Some(1),
        "三" => Some(2),
        "四" => Some(3),
        "五" => Some(4),
        "六" => Some(5),
        "日" | "天" => Some(6),
        _ => None,
    }
}

fn parse_first_time(text: &str) -> Option<NaiveTime> {
    parse_time_occurrences(text)
        .into_iter()
        .next()
        .map(|(time, _)| time)
}

fn parse_time_occurrences(text: &str) -> Vec<(NaiveTime, usize)> {
    let re = match regex::Regex::new(
        r"(凌晨|早上|上午|中午|下午|晚上|晚)?\s*(\d{1,2})(?:\s*([:：点时])\s*(\d{1,2})?)?\s*(半)?",
    ) {
        Ok(re) => re,
        Err(_) => return Vec::new(),
    };
    let mut times = Vec::new();
    let mut last_period = "";

    for caps in re.captures_iter(text) {
        let Some(full_match) = caps.get(0) else {
            continue;
        };
        let period = caps.get(1).map(|value| value.as_str()).unwrap_or("");
        let effective_period = if period.is_empty() {
            last_period
        } else {
            period
        };
        let separator = caps.get(3).map(|value| value.as_str()).unwrap_or("");

        if period.is_empty() && separator.is_empty() {
            continue;
        }

        let Some(mut hour) = caps
            .get(2)
            .and_then(|value| value.as_str().parse::<u32>().ok())
        else {
            continue;
        };
        let minute = caps
            .get(4)
            .and_then(|value| value.as_str().parse::<u32>().ok())
            .unwrap_or_else(|| if caps.get(5).is_some() { 30 } else { 0 });

        if matches!(effective_period, "下午" | "晚上" | "晚") && hour < 12 {
            hour += 12;
        } else if effective_period == "中午" && hour < 11 {
            hour += 12;
        }

        if let Some(time) = NaiveTime::from_hms_opt(hour, minute, 0) {
            times.push((time, full_match.start()));
        }

        if !period.is_empty() {
            last_period = period;
        }
    }

    times
}

fn parse_date_key(value: &str) -> Option<NaiveDate> {
    if !is_date_key(value) {
        return None;
    }

    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}

fn local_datetime(date: NaiveDate, time: NaiveTime) -> Option<String> {
    Local
        .with_ymd_and_hms(
            date.year(),
            date.month(),
            date.day(),
            time.hour(),
            time.minute(),
            0,
        )
        .single()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn first_item(raw: &str) -> AiTaskUpdateItem {
        parse_task_update_response(raw)
            .expect("model JSON should parse")
            .tasks
            .into_iter()
            .next()
            .expect("task should exist")
    }

    fn build_from_prompt(prompt: &str, raw: &str) -> WorkflowTask {
        let item = first_item(raw);
        build_task_from_ai_item(
            "task-test".to_string(),
            None,
            prompt.to_string(),
            &item,
            "pending".to_string(),
            "2026-06-29T00:00:00Z".to_string(),
            "2026-06-29T00:00:00Z".to_string(),
        )
        .expect("task should build")
    }

    #[test]
    fn model_start_at_is_used() {
        let task = build_from_prompt(
            "meeting",
            r#"{"tasks":[{"title":"Meeting","content":"- Talk","startAt":"2026-07-01T09:00:00+08:00","occurrenceDate":"2026-07-01","metadata":{}}]}"#,
        );

        assert!(task.start_at.unwrap().contains("T09:00:00"));
        assert_eq!(task.occurrence_date.as_deref(), Some("2026-07-01"));
    }

    #[test]
    fn model_scheduled_at_alias_is_used() {
        let task = build_from_prompt(
            "meeting",
            r#"{"tasks":[{"title":"Meeting","content":"- Talk","scheduledAt":"2026-07-01T10:15:00+08:00","metadata":{}}]}"#,
        );

        assert!(task.start_at.unwrap().contains("T10:15:00"));
        assert_eq!(task.occurrence_date.as_deref(), Some("2026-07-01"));
    }

    #[test]
    fn model_start_time_with_occurrence_date_is_used() {
        let task = build_from_prompt(
            "meeting",
            r#"{"tasks":[{"title":"Meeting","content":"- Talk","startTime":"09:30","occurrenceDate":"2026-07-02","metadata":{}}]}"#,
        );

        assert!(task.start_at.unwrap().contains("T09:30:00"));
        assert_eq!(task.occurrence_date.as_deref(), Some("2026-07-02"));
    }

    #[test]
    fn prompt_chinese_start_time_fallback_is_used() {
        let prompt = "\u{660e}\u{5929}\u{65e9}\u{4e0a}9\u{70b9}\u{5f00}\u{4f1a}";
        let task = build_from_prompt(
            prompt,
            r#"{"tasks":[{"title":"Meeting","content":"- Talk","timeText":"\u660e\u5929\u65e9\u4e0a9\u70b9","metadata":{}}]}"#,
        );
        let expected_date = (Local::now().date_naive() + ChronoDuration::days(1))
            .format("%Y-%m-%d")
            .to_string();

        assert!(task.start_at.unwrap().contains("T09:00:00"));
        assert_eq!(
            task.occurrence_date.as_deref(),
            Some(expected_date.as_str())
        );
    }

    #[test]
    fn prompt_chinese_time_range_fallback_is_used() {
        let prompt = "\u{540e}\u{5929}\u{4e0b}\u{5348}3\u{70b9}\u{5230}5\u{70b9}\u{4e0a}\u{8bfe}";
        let task = build_from_prompt(
            prompt,
            r#"{"tasks":[{"title":"Class","content":"- Attend class","metadata":{}}]}"#,
        );

        assert!(task.start_at.unwrap().contains("T15:00:00"));
        assert!(task.end_at.unwrap().contains("T17:00:00"));
    }

    #[test]
    fn prompt_month_day_time_range_fallback_is_used() {
        let prompt = "7\u{6708}1\u{53f7}\u{4e0b}\u{5348}6\u{70b9}\u{5230}\u{665a}\u{4e0a}9\u{70b9}\u{4e0a}\u{8bfe}";
        let task = build_from_prompt(
            prompt,
            r#"{"tasks":[{"title":"Class","content":"- Attend class","metadata":{}}]}"#,
        );

        assert!(task.start_at.unwrap().contains("T18:00:00"));
        assert!(task.end_at.unwrap().contains("T21:00:00"));
        assert_eq!(task.occurrence_date.as_deref(), Some("2026-07-01"));
    }

    #[test]
    fn model_expanded_recurring_tasks_keep_concrete_times() {
        let prompt = "\u{0037}\u{6708}\u{4efd}\u{6bcf}\u{5468}\u{4e09}\u{4e0b}\u{5348}6\u{70b9}\u{5230}\u{665a}\u{4e0a}9\u{70b9}\u{4e0a}\u{6570}\u{636e}\u{5e93}\u{8bbe}\u{8ba1}";
        let response = parse_task_update_response(
            r#"{"tasks":[{"title":"Database design class","content":"- Attend class","startAt":"2026-07-01T18:00:00+08:00","endAt":"2026-07-01T21:00:00+08:00","occurrenceDate":"2026-07-01","metadata":{}},{"title":"Database design class","content":"- Attend class","startAt":"2026-07-08T18:00:00+08:00","endAt":"2026-07-08T21:00:00+08:00","occurrenceDate":"2026-07-08","metadata":{}}]}"#,
        )
        .expect("model JSON should parse");

        let tasks = response
            .tasks
            .iter()
            .enumerate()
            .map(|(index, item)| {
                build_task_from_ai_item(
                    format!("task-test-{index}"),
                    None,
                    prompt.to_string(),
                    item,
                    "pending".to_string(),
                    "2026-06-29T00:00:00Z".to_string(),
                    "2026-06-29T00:00:00Z".to_string(),
                )
                .expect("task should build")
            })
            .collect::<Vec<_>>();

        assert_eq!(tasks.len(), 2);
        assert!(tasks[0].start_at.as_deref().unwrap().contains("T18:00:00"));
        assert!(tasks[0].end_at.as_deref().unwrap().contains("T21:00:00"));
        assert_eq!(tasks[0].occurrence_date.as_deref(), Some("2026-07-01"));
        assert_eq!(tasks[1].occurrence_date.as_deref(), Some("2026-07-08"));
    }

    #[test]
    fn relative_minutes_fallback_is_used() {
        let prompt = "30\u{5206}\u{949f}\u{540e}\u{5f00}\u{4f1a}";
        let task = build_from_prompt(
            prompt,
            r#"{"tasks":[{"title":"Meeting","content":"- Talk","metadata":{}}]}"#,
        );

        assert!(task.start_at.is_some());
    }
}
