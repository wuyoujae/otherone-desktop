use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveTime, TimeZone, Timelike, Utc};
use otherone::ai::types::{
    FunctionDefinition, Message, MessageContent, ProviderType, Tool, ToolChoice,
};
use otherone::Otherone;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::storage::{self, ModelConfig, ProviderConfig};

static WORKFLOW_TASK_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, Deserialize)]
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
pub struct WorkflowTodoCreateToolInput {
    pub tasks: Vec<WorkflowTodoTaskToolInput>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowTodoTaskToolInput {
    pub prompt: Option<String>,
    pub title: Option<String>,
    pub content: Option<String>,
    pub start_at: Option<String>,
    pub scheduled_at: Option<String>,
    pub end_at: Option<String>,
    pub occurrence_date: Option<String>,
    pub time_text: Option<String>,
    pub repeat_start_date: Option<String>,
    pub repeat_end_date: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowTodoListToolInput {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub status: Option<String>,
    pub search: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowTodoUpdateToolInput {
    pub id: String,
    pub prompt: Option<String>,
    pub title: Option<String>,
    pub content: Option<String>,
    pub start_at: Option<String>,
    pub scheduled_at: Option<String>,
    pub end_at: Option<String>,
    pub occurrence_date: Option<String>,
    pub time_text: Option<String>,
    pub repeat_start_date: Option<String>,
    pub repeat_end_date: Option<String>,
    pub metadata: Option<Value>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowTodoDeleteToolInput {
    pub id: String,
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

pub fn open_workflow_database(data_root: &Path) -> Result<Connection, String> {
    let conn = storage::open_database(data_root)?;
    storage::init_database(&conn)?;
    init_workflow_database(&conn)?;
    Ok(conn)
}

pub fn init_workflow_database(conn: &Connection) -> Result<(), String> {
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

pub async fn create_workflow_task(
    data_root: &Path,
    request: CreateWorkflowTaskRequest,
    providers: &[ProviderConfig],
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

    let (provider, model) = select_model_with_fallback(providers, model_id)?;
    let model_response = invoke_task_create_model(&provider, &model, &prompt).await?;
    let parsed = parse_task_update_response(&model_response)?;

    if parsed.tasks.is_empty() {
        return Err("AI returned no tasks.".to_string());
    }

    let mut conn = open_workflow_database(data_root)?;
    let now = Utc::now().to_rfc3339();
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let series_id = insert_series_if_needed(&tx, &prompt, &parsed.tasks, &model_response, &now)?;
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

pub async fn update_workflow_task(
    data_root: &Path,
    request: ModifyWorkflowTaskRequest,
    providers: &[ProviderConfig],
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

    let mut conn = open_workflow_database(data_root)?;
    let current_task = read_workflow_task(&conn, &id)?;
    let (provider, model) = select_model_with_fallback(providers, model_id)?;
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
            current_task.series_id.clone(),
            edit_prompt.clone(),
            &parsed.tasks[0],
            current_task.status.clone(),
            current_task.created_at.clone(),
            now.clone(),
        )?;
        update_workflow_task_row(&tx, &id, &task, &model_response)?;
        updated_tasks.push(task);
    } else {
        tx.execute(
            "DELETE FROM workflow_tasks WHERE id = ?1",
            params![id.as_str()],
        )
        .map_err(|error| error.to_string())?;
        let series_id =
            insert_series_if_needed(&tx, &edit_prompt, &parsed.tasks, &model_response, &now)?
                .ok_or_else(|| "AI returned no updated tasks.".to_string())?;

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

pub fn list_workflow_tasks(data_root: &Path) -> Result<Vec<WorkflowTask>, String> {
    let conn = open_workflow_database(data_root)?;
    read_workflow_tasks(&conn)
}

pub fn list_workflow_tasks_for_range(
    data_root: &Path,
    request: ListWorkflowTasksForRangeRequest,
) -> Result<Vec<WorkflowTask>, String> {
    let start = request.start_date.trim().to_string();
    let end = request.end_date.trim().to_string();

    if start.is_empty() || end.is_empty() {
        return Err("Date range cannot be empty.".to_string());
    }

    let tasks = list_workflow_tasks(data_root)?;
    Ok(tasks
        .into_iter()
        .filter(|task| task_intersects_date_range(task, &start, &end))
        .collect())
}

pub fn update_workflow_task_status(
    data_root: &Path,
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

    let conn = open_workflow_database(data_root)?;
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

pub fn delete_workflow_task(
    data_root: &Path,
    request: DeleteWorkflowTaskRequest,
) -> Result<(), String> {
    let id = request.id.trim();

    if id.is_empty() {
        return Err("Task id cannot be empty.".to_string());
    }

    let conn = open_workflow_database(data_root)?;
    let changed = conn
        .execute("DELETE FROM workflow_tasks WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;

    if changed == 0 {
        return Err("Task does not exist.".to_string());
    }

    Ok(())
}

pub fn create_workflow_tasks_from_tool(
    data_root: &Path,
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

    let mut conn = open_workflow_database(data_root)?;
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let model_response = serde_json::json!({
        "source": "chat_agent",
        "tool": "create_todo"
    })
    .to_string();

    if let Some(series_id) = series_id.as_deref() {
        let first = created_tasks
            .first()
            .ok_or_else(|| "Todo tool requires at least one task.".to_string())?;
        let mut dates: Vec<&str> = created_tasks
            .iter()
            .filter_map(|task| task.occurrence_date.as_deref())
            .collect();
        dates.sort_unstable();
        insert_workflow_series(
            &tx,
            series_id,
            &first.prompt,
            &first.title,
            &first.content,
            dates.first().copied(),
            dates.last().copied(),
            &first.metadata_json,
            &model_response,
            &now,
            "chat-agent-expanded",
        )?;
    }

    for task in &created_tasks {
        insert_workflow_task(&tx, task, &model_response)?;
    }

    tx.commit().map_err(|error| error.to_string())?;
    Ok(created_tasks)
}

pub fn list_workflow_tasks_for_tool(
    data_root: &Path,
    request: WorkflowTodoListToolInput,
) -> Result<Vec<WorkflowTask>, String> {
    let mut tasks = list_workflow_tasks(data_root)?;
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

pub fn update_workflow_task_from_tool(
    data_root: &Path,
    request: WorkflowTodoUpdateToolInput,
) -> Result<WorkflowTask, String> {
    let id = request.id.trim().to_string();

    if id.is_empty() {
        return Err("Task id cannot be empty.".to_string());
    }

    let conn = open_workflow_database(data_root)?;
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
    let task = WorkflowTask {
        id: id.clone(),
        series_id: current.series_id,
        prompt,
        title,
        content,
        scheduled_at: start_at.clone(),
        start_at,
        end_at,
        occurrence_date,
        time_text,
        repeat_start_date,
        repeat_end_date,
        metadata_json,
        status,
        created_at: current.created_at,
        updated_at: now,
    };

    update_workflow_task_row(&conn, &id, &task, &model_response)?;
    Ok(task)
}

pub fn delete_workflow_task_from_tool(
    data_root: &Path,
    request: WorkflowTodoDeleteToolInput,
) -> Result<String, String> {
    let id = request.id.trim().to_string();

    if id.is_empty() {
        return Err("Task id cannot be empty.".to_string());
    }

    delete_workflow_task(data_root, DeleteWorkflowTaskRequest { id: id.clone() })?;
    Ok(id)
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

fn update_workflow_task_row(
    conn: &Connection,
    id: &str,
    task: &WorkflowTask,
    model_response: &str,
) -> Result<(), String> {
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
                task.updated_at.as_str(),
                id,
            ],
        )
        .map_err(|error| error.to_string())?;

    if changed == 0 {
        return Err("Task does not exist.".to_string());
    }

    Ok(())
}

fn insert_series_if_needed(
    conn: &Connection,
    prompt: &str,
    tasks: &[AiTaskUpdateItem],
    model_response: &str,
    now: &str,
) -> Result<Option<String>, String> {
    if tasks.len() <= 1 {
        return Ok(None);
    }

    let series_id = build_workflow_series_id();
    let first = tasks
        .first()
        .ok_or_else(|| "AI returned no tasks.".to_string())?;
    let title = first.title.as_deref().unwrap_or("Workflow task");
    let content = first.content.as_deref().unwrap_or("");
    let metadata_json = serde_json::to_string(&first.metadata).unwrap_or_else(|_| "{}".to_string());

    insert_workflow_series(
        conn,
        &series_id,
        prompt,
        title,
        content,
        tasks
            .first()
            .and_then(|task| task.occurrence_date.as_deref()),
        tasks
            .last()
            .and_then(|task| task.occurrence_date.as_deref()),
        &metadata_json,
        model_response,
        now,
        "ai-expanded",
    )?;

    Ok(Some(series_id))
}

#[allow(clippy::too_many_arguments)]
fn insert_workflow_series(
    conn: &Connection,
    series_id: &str,
    prompt: &str,
    title: &str,
    content: &str,
    start_date: Option<&str>,
    end_date: Option<&str>,
    metadata_json: &str,
    model_response: &str,
    now: &str,
    schedule_kind: &str,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO workflow_task_series
         (id, prompt, title, content, schedule_kind, start_date, end_date, weekdays_json, start_time, end_time, timezone, metadata_json, model_response, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, '[]', NULL, NULL, 'local', ?8, ?9, ?10, ?11)",
        params![
            series_id,
            prompt,
            title,
            content,
            schedule_kind,
            start_date,
            end_date,
            metadata_json,
            model_response,
            now,
            now,
        ],
    )
    .map_err(|error| error.to_string())?;
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
    let mut occurrence_date = item
        .occurrence_date
        .as_deref()
        .or(item.date.as_deref())
        .filter(|value| is_date_key(value))
        .map(ToString::to_string);
    let start_at = normalize_ai_datetime(
        item.start_at
            .as_deref()
            .or(item.scheduled_at.as_deref())
            .or(item.start_time.as_deref()),
        occurrence_date.as_deref(),
    );
    let end_at = normalize_ai_datetime(
        item.end_at.as_deref().or(item.end_time.as_deref()),
        start_at
            .as_deref()
            .and_then(rfc3339_date_key)
            .or(occurrence_date.as_deref()),
    );

    if occurrence_date.is_none() {
        occurrence_date = start_at
            .as_deref()
            .and_then(rfc3339_date_key)
            .map(ToString::to_string);
    }
    if occurrence_date.is_none() {
        occurrence_date = Some(Local::now().format("%Y-%m-%d").to_string());
    }

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
                                "title": { "type": "string", "description": "Short task title." },
                                "content": { "type": "string", "description": "Markdown list content." },
                                "startAt": { "type": ["string", "null"], "description": "RFC3339 start datetime." },
                                "scheduledAt": { "type": ["string", "null"], "description": "Alias for startAt." },
                                "endAt": { "type": ["string", "null"], "description": "RFC3339 end datetime." },
                                "occurrenceDate": { "type": ["string", "null"], "description": "YYYY-MM-DD date." },
                                "timeText": { "type": ["string", "null"], "description": "Original time phrase." },
                                "repeatStartDate": { "type": ["string", "null"] },
                                "repeatEndDate": { "type": ["string", "null"] },
                                "metadata": { "type": ["object", "null"] }
                            },
                            "required": ["title", "content"]
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
        return Err("The selected Todo model has tool calling disabled.".to_string());
    }

    Ok(parse_tool_choice(&model.tool_choice))
}

async fn invoke_task_create_model(
    provider: &ProviderConfig,
    model: &ModelConfig,
    prompt: &str,
) -> Result<String, String> {
    let provider_type = parse_provider(&provider.provider)?;
    let api_key = require_text(&provider.api_key, "API Key")?;
    let base_url = require_text(&provider.base_url, "Base URL")?;
    let model_name = require_text(&model.name, "Model name")?;
    let tool_choice = workflow_create_tool_choice(model)?;
    let now = Local::now().format("%Y-%m-%d %H:%M:%S %:z").to_string();
    let user_prompt = format!(
        r#"Current local time: {now}

User todo instruction:
{prompt}

Rules:
- You must call the create_todo tool exactly once.
- Convert the user instruction into concrete todo task instances.
- For recurring instructions, expand every occurrence into its own task.
- If the user instruction contains a time, startAt must be RFC3339.
- If the user instruction contains a time range, endAt must be RFC3339.
- Use RFC3339 datetimes with local timezone offset.
- Keep title concise."#
    );
    let mut config = serde_json::json!({
        "model": model_name,
        "messages": [
            Message {
                role: "system".to_string(),
                content: MessageContent::Text("You are a todo scheduling parser. Call create_todo with concrete task instances.".to_string()),
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
        "tools": [create_todo_tool_def()],
    });
    merge_object_params(&mut config, build_other_params(&model.extra_params, None)?);
    config["parallelToolCalls"] = serde_json::json!(model.parallel_tool_calls);
    config["stream"] = serde_json::json!(false);
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
- Merge the existing task and the edit instruction.
- The edit instruction overrides conflicting old data.
- For recurring instructions, expand every occurrence into its own task.
- Use RFC3339 datetimes with local timezone offset.
- Do not include explanations or markdown fences."#
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
    merge_object_params(&mut config, build_other_params(&model.extra_params, None)?);
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

fn parse_first_time(text: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(text.trim(), "%H:%M")
        .ok()
        .or_else(|| NaiveTime::parse_from_str(text.trim(), "%H:%M:%S").ok())
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

fn select_model_with_fallback(
    providers: &[ProviderConfig],
    model_id: Option<&str>,
) -> Result<(ProviderConfig, ModelConfig), String> {
    if let Some(model_id) = model_id {
        for provider in providers {
            if let Some(model) = provider.models.iter().find(|model| model.id == model_id) {
                return Ok((provider.clone(), model.clone()));
            }
        }
        return Err("Selected model configuration was not found.".to_string());
    }

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

    first_available.ok_or_else(|| "Please configure an available model first.".to_string())
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

fn require_text<'a>(value: &'a str, field: &str) -> Result<&'a str, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(format!("{field} cannot be empty."))
    } else {
        Ok(trimmed)
    }
}

fn parse_tool_choice(value: &str) -> Option<ToolChoice> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "default" {
        None
    } else {
        Some(ToolChoice::String(trimmed.to_string()))
    }
}

fn build_other_params(
    extra_params: &str,
    reasoning_effort: Option<String>,
) -> Result<Option<Value>, String> {
    let mut object = if extra_params.trim().is_empty() {
        serde_json::Map::new()
    } else {
        match serde_json::from_str::<Value>(extra_params).map_err(|error| error.to_string())? {
            Value::Object(object) => object,
            _ => return Err("Model extra params must be a JSON object.".to_string()),
        }
    };

    if let Some(value) = reasoning_effort
        .as_deref()
        .map(str::trim)
        .filter(|value| matches!(*value, "low" | "medium" | "high"))
    {
        object.insert(
            "reasoning_effort".to_string(),
            Value::String(value.to_string()),
        );
        object.insert(
            "reasoningEffort".to_string(),
            Value::String(value.to_string()),
        );
    }

    if object.is_empty() {
        Ok(None)
    } else {
        Ok(Some(Value::Object(object)))
    }
}

fn merge_object_params(target: &mut Value, params: Option<Value>) {
    let (Value::Object(target), Some(Value::Object(params))) = (target, params) else {
        return;
    };

    for (key, value) in params {
        target.insert(key, value);
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
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

    fn insert_task(conn: &Connection, id: &str, occurrence_date: &str) {
        conn.execute(
            "INSERT INTO workflow_tasks
             (id, prompt, title, content, occurrence_date, metadata_json, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, '{}', 'pending', '2026-07-01T00:00:00Z', '2026-07-01T00:00:00Z')",
            params![id, "write tests", "Write tests", "- Add coverage", occurrence_date],
        )
        .expect("insert task");
    }

    #[test]
    fn lists_updates_and_deletes_workflow_tasks() {
        let data_root = test_dir("workflow");
        let conn = open_workflow_database(&data_root).expect("open workflow db");
        insert_task(&conn, "task-1", "2026-07-01");

        let tasks = list_workflow_tasks(&data_root).expect("list tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "task-1");

        let range = list_workflow_tasks_for_range(
            &data_root,
            ListWorkflowTasksForRangeRequest {
                start_date: "2026-07-01".to_string(),
                end_date: "2026-07-01".to_string(),
            },
        )
        .expect("list range");
        assert_eq!(range.len(), 1);

        let updated = update_workflow_task_status(
            &data_root,
            UpdateWorkflowTaskStatusRequest {
                id: "task-1".to_string(),
                status: "completed".to_string(),
            },
        )
        .expect("update status");
        assert_eq!(updated.status, "completed");

        delete_workflow_task(
            &data_root,
            DeleteWorkflowTaskRequest {
                id: "task-1".to_string(),
            },
        )
        .expect("delete task");
        assert!(list_workflow_tasks(&data_root)
            .expect("list after delete")
            .is_empty());
    }
}
