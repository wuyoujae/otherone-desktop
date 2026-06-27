# Workflow Tasks

## Scope
- Workflow tasks are created from the task prompt box.
- Creation now runs through the configured default AI model before persistence.
- Execution scheduling and parameter presets are intentionally out of scope.
- Desktop system reminders are supported for concrete task start times.

## Storage
- SQLite database: `{dataRoot}/otherone.sqlite`
- Tables: `workflow_tasks`, `workflow_task_series`

### `workflow_tasks`

`workflow_tasks` is the concrete task-instance table. Calendar and task-list UI read this table only.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | `TEXT PRIMARY KEY` | App-generated task id. |
| `series_id` | `TEXT` | Optional parent series id for generated recurring instances. |
| `prompt` | `TEXT NOT NULL` | Raw user prompt from the task prompt box, retained for traceability. |
| `title` | `TEXT NOT NULL DEFAULT ''` | AI-normalized task title. |
| `content` | `TEXT NOT NULL DEFAULT ''` | AI-normalized markdown task body. Every non-empty line is normalized to `- ` syntax. |
| `scheduled_at` | `TEXT` | Backward-compatible concrete RFC3339 task start time, e.g. `2026-06-28T09:00:00+08:00`. New reads prefer `start_at ?? scheduled_at`. |
| `start_at` | `TEXT` | Concrete RFC3339 task start time. Calendar grouping and task-list clock display use this first. |
| `end_at` | `TEXT` | Optional concrete RFC3339 task end time for time-range tasks. Calendar still places the task in the start-time block only. |
| `occurrence_date` | `TEXT` | Optional concrete instance date in `YYYY-MM-DD` form. Generated recurring instances use this for exact calendar placement. |
| `time_text` | `TEXT` | Original user time phrase, e.g. `明天早上9点`; diagnostic/display hint only. |
| `repeat_start_date` | `TEXT` | Optional parent repeat start date, retained on generated instances for traceability. |
| `repeat_end_date` | `TEXT` | Optional parent repeat end date, retained on generated instances for traceability. |
| `metadata_json` | `TEXT NOT NULL DEFAULT '{}'` | AI-extracted metadata such as priority, summary, tags, original prompt, plus mirrored time fields. |
| `model_response` | `TEXT NOT NULL DEFAULT ''` | Raw model response used to create the task. |
| `reminder_notified_at` | `TEXT` | UTC RFC3339 timestamp for the last successful desktop reminder. |
| `reminder_target_at` | `TEXT` | Start datetime that was already reminded. If the task start time changes, it can be reminded again. |
| `status` | `TEXT NOT NULL DEFAULT 'pending'` | Task lifecycle state. Supported values are `pending` and `completed`. |
| `created_at` | `TEXT NOT NULL` | UTC RFC3339 timestamp from backend creation. |
| `updated_at` | `TEXT NOT NULL` | UTC RFC3339 timestamp from backend creation. |

### `workflow_task_series`

`workflow_task_series` stores one natural-language repeated intent. The backend expands it into concrete rows in `workflow_tasks`.

| Column | Type | Notes |
| --- | --- | --- |
| `id` | `TEXT PRIMARY KEY` | App-generated series id. |
| `prompt` | `TEXT NOT NULL` | Raw user repeated-task prompt. |
| `title` | `TEXT NOT NULL DEFAULT ''` | Series title, e.g. `数据库设计课`. |
| `content` | `TEXT NOT NULL DEFAULT ''` | Series task content copied to generated instances. |
| `schedule_kind` | `TEXT NOT NULL DEFAULT 'single'` | First version uses `weekly` for expanded recurring tasks. |
| `start_date` | `TEXT` | Inclusive series start date. |
| `end_date` | `TEXT` | Inclusive series end date. |
| `weekdays_json` | `TEXT NOT NULL DEFAULT '[]'` | Weekday list, Monday-based. |
| `start_time` | `TEXT` | Local start clock, e.g. `18:00`. |
| `end_time` | `TEXT` | Optional local end clock, e.g. `21:00`. |
| `timezone` | `TEXT NOT NULL DEFAULT 'local'` | Current implementation uses local machine timezone. |
| `metadata_json` | `TEXT NOT NULL DEFAULT '{}'` | Metadata copied from normalization. |
| `model_response` | `TEXT NOT NULL DEFAULT ''` | Raw model response. |
| `created_at` | `TEXT NOT NULL` | UTC RFC3339 timestamp from backend creation. |
| `updated_at` | `TEXT NOT NULL` | UTC RFC3339 timestamp from backend creation. |

## Tauri Commands
- `create_workflow_task({ request: { prompt } })`
  - Trims `prompt`.
  - Rejects empty task content.
  - Selects the configured default model, or the first configured model when no default exists.
  - Sends the raw prompt to the model with a JSON-only normalization prompt.
  - Accepts plain JSON, fenced JSON, or surrounding text that contains a JSON object.
  - Requires model `scheduledAt` to be a concrete RFC3339 datetime when the task contains time.
  - If the model returns relative text such as `明天早上9点`, backend falls back to deterministic local parsing.
  - Supported backend time fallbacks include `明天早上9点`, `明早9点`, `后天晚上8点`, `下周一下午3点`, `10分钟后`, `30分钟后`, `半小时后`, and `1小时30分钟后`.
  - Rejects unparsable model output and does not write a task.
  - Creates a `pending` task from normalized `title`, `content`, `startAt`/`scheduledAt`, optional `endAt`, repeat date span, `timeText`, and `metadata`.
  - When normalization or backend fallback detects a weekly date-span recurrence, creates one `workflow_task_series` row and expands concrete `workflow_tasks` rows for every matching date.
  - Example: `我7月份每周三下午6点都要上课，上到晚上9点钟，要上数据库设计这一门课` expands to five concrete July Wednesday task instances named `数据库设计课`.
- `list_workflow_tasks()`
  - Returns saved workflow tasks ordered by `COALESCE(start_at, scheduled_at, created_at)` first, then creation order.
- `list_workflow_tasks_for_range({ request: { startDate, endDate } })`
  - Accepts inclusive `YYYY-MM-DD` dates.
  - Returns tasks whose concrete `occurrence_date`/`start_at` is inside the range, or legacy non-expanded rows whose repeat date span intersects the range.
  - Preserves old rows by treating `scheduled_at` as the start time when `start_at` is empty.
- `update_workflow_task_status({ request: { id, status } })`
  - Accepts `pending` or `completed`.
  - Updates `status` and `updated_at`, then returns the updated task.
- `delete_workflow_task({ request: { id } })`
  - Deletes one concrete task instance from `workflow_tasks`.
  - Does not delete the parent `workflow_task_series` or sibling generated instances.
- `update_workflow_task({ request: { id, prompt } })`
  - Reads the selected concrete task and sends it with the natural-language edit instruction to the configured workflow model.
  - Requires a JSON object with a `tasks` array.
  - When one task is returned, updates the selected row in place and preserves its status.
  - When multiple tasks are returned, deletes the selected row, creates a new `workflow_task_series`, and inserts one concrete row per returned task.
  - Supports edits that turn a simple task into a recurring set, such as changing `tomorrow 09:00 class` into `every Monday 09:00-18:00 class`.

## Desktop Reminders
- The Tauri backend starts a workflow reminder loop during app setup.
- Every 30 seconds it scans pending tasks with `start_at` or legacy `scheduled_at`.
- If the task starts within the next three minutes, the backend sends a desktop system notification.
- Completed tasks and tasks without concrete start times are skipped.
- The backend writes `reminder_target_at` after sending, so the same task/start-time pair only reminds once.
- If an edited task gets a new start time, it becomes eligible for a new reminder.

## Frontend
- `TaskView` loads the selected header date through `loadWorkflowTasksForRangeFromStorage(selectedDate, selectedDate)`.
- The task sidebar heading is `今日任务`, and its count is scoped to the selected header date.
- When no task is selected, the prompt send button calls `createWorkflowTaskInStorage`.
- Selecting a sidebar task switches the right panel to task editing mode.
- Hovering or focusing a sidebar task reveals a delete icon. Deleting removes only that concrete task instance.
- Task editing mode shows the same natural-language prompt surface with the placeholder `修改你的任务，通过自然语言`.
- Task editing mode displays title, scheduled time, priority, and markdown content as the primary task surface.
- Secondary fields, including updated time, summary, tags, and original prompt, are grouped into a default-collapsed "更多信息" panel.
- The task status pill and sidebar checkbox toggle `pending`/`completed`; completed sidebar items dim with a CSS transition.
- Natural-language task modification calls `update_workflow_task` and refreshes the selected-date task list after persistence.
- Task list time omits the date because the workflow header already shows the selected date.
- Task list time uses different icons for inferred time kinds: point time, time range, repeated point time, and repeated time range.
- Time-kind inference uses `startAt`, `endAt`, repeat dates, `scheduledAt`, `timeText`, metadata, and the raw prompt.
- Calendar view loads the visible 7-day window through `list_workflow_tasks_for_range`.
- Calendar columns no longer render default 24-hour slots. Each day renders only real task start-time blocks such as `20:45`.
- Multiple tasks with the same start clock share the same calendar block.
- Time-range tasks render in their start-time block only, with the end time shown inside the task item.
- Generated recurring tasks are displayed as independent task instances, so each occurrence can be completed independently.
- Empty days show a no-task state (`今天没有任务` for today, otherwise `暂无任务`).
- While the model is processing, the send icon switches to a loading spinner.
- Success and failure feedback uses the app toast system in the bottom-right corner.
- Parameter preset fields remain create-mode placeholders and are hidden while editing an existing task.
