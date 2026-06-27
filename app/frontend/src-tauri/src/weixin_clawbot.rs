use crate::{chat, storage};
use chrono::{Duration as ChronoDuration, Utc};
use reqwest::blocking::{Client, RequestBuilder};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;
use std::time::{Duration as StdDuration, SystemTime, UNIX_EPOCH};
use tauri::AppHandle;

const DEFAULT_ACCOUNT_ID: &str = "default";
const DEFAULT_BASE_URL: &str = "https://ilinkai.weixin.qq.com";
const CHANNEL_VERSION: &str = "2.4.3";
const ILINK_APP_ID: &str = "bot";
const ILINK_APP_CLIENT_VERSION: &str = "132099";
const BOT_AGENT: &str = "OtherOne/0.1.0 (weixin-clawbot)";

static RUNTIME: LazyLock<Mutex<Option<WeixinRuntime>>> = LazyLock::new(|| Mutex::new(None));
static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

struct WeixinRuntime {
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

#[derive(Debug, Clone)]
struct WeixinAccount {
    bot_user_id: String,
    ilink_user_id: String,
    bot_token: String,
    base_url: String,
    get_updates_buf: String,
    status: String,
    login_expires_at: Option<String>,
    last_connected_at: Option<String>,
    last_poll_at: Option<String>,
    last_error: String,
}

#[derive(Debug, Clone)]
struct IncomingText {
    from_user_id: String,
    context_token: String,
    text: String,
}

#[derive(Debug, Clone)]
struct LoginQrPayload {
    qrcode: String,
    qrcode_img_content: String,
}

#[derive(Debug, Clone)]
struct LoginStatusPayload {
    status: String,
    bot_token: Option<String>,
    bot_user_id: Option<String>,
    ilink_user_id: Option<String>,
    base_url: Option<String>,
    redirect_base: Option<String>,
    verify_code_required: bool,
    expired: bool,
    already_connected: bool,
    verify_code_blocked: bool,
}

#[derive(Debug, Clone)]
struct GetUpdatesPayload {
    get_updates_buf: String,
    timeout_ms: Option<u64>,
    messages: Vec<IncomingText>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeixinClawbotStatus {
    configured: bool,
    running: bool,
    status: String,
    bot_user_id: String,
    ilink_user_id: String,
    base_url: String,
    has_token: bool,
    login_expires_at: Option<String>,
    last_connected_at: Option<String>,
    last_poll_at: Option<String>,
    last_error: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeixinLoginQr {
    qrcode: String,
    qrcode_img_content: String,
    base_url: String,
    status: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeixinLoginCheckRequest {
    qrcode: String,
    base_url: Option<String>,
    verify_code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeixinLoginCheckResponse {
    status: String,
    message: String,
    base_url: String,
    confirmed: bool,
    verify_code_required: bool,
    expired: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeixinClawbotEvent {
    id: String,
    direction: String,
    from_user_id: String,
    summary: String,
    status: String,
    error: String,
    created_at: String,
}

pub(crate) fn init_weixin_clawbot_database(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS weixin_clawbot_accounts (
            id TEXT PRIMARY KEY,
            bot_user_id TEXT NOT NULL DEFAULT '',
            ilink_user_id TEXT NOT NULL DEFAULT '',
            bot_token TEXT NOT NULL DEFAULT '',
            base_url TEXT NOT NULL DEFAULT '',
            get_updates_buf TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'disconnected',
            login_expires_at TEXT,
            last_connected_at TEXT,
            last_poll_at TEXT,
            last_error TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS weixin_clawbot_sessions (
            account_id TEXT NOT NULL,
            from_user_id TEXT NOT NULL,
            agent_session_id TEXT NOT NULL,
            last_context_token TEXT NOT NULL DEFAULT '',
            last_message_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (account_id, from_user_id)
        );

        CREATE TABLE IF NOT EXISTS weixin_clawbot_events (
            id TEXT PRIMARY KEY,
            account_id TEXT NOT NULL,
            direction TEXT NOT NULL,
            from_user_id TEXT NOT NULL DEFAULT '',
            summary TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT '',
            error TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE INDEX IF NOT EXISTS idx_weixin_clawbot_events_created
            ON weixin_clawbot_events(created_at DESC);
        ",
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn weixin_clawbot_status(app: AppHandle) -> Result<WeixinClawbotStatus, String> {
    build_status(&app)
}

#[tauri::command]
pub async fn weixin_clawbot_begin_login(app: AppHandle) -> Result<WeixinLoginQr, String> {
    tauri::async_runtime::spawn_blocking(move || begin_login_blocking(app))
        .await
        .map_err(|error| error.to_string())?
}

fn begin_login_blocking(app: AppHandle) -> Result<WeixinLoginQr, String> {
    let conn = open_weixin_db(&app)?;
    let existing = load_account_from_conn(&conn, DEFAULT_ACCOUNT_ID)?;
    let base_url = existing
        .as_ref()
        .map(|account| normalize_base_url(&account.base_url))
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    let local_token_list = existing
        .as_ref()
        .filter(|account| !account.bot_token.trim().is_empty())
        .map(|account| vec![account.bot_token.clone()])
        .unwrap_or_default();

    let client = IlinkClient::new(
        &base_url,
        existing.as_ref().map(|account| account.bot_token.as_str()),
    )?;
    let qr = client.fetch_login_qr(local_token_list)?;

    upsert_login_pending(&conn, &base_url)?;

    Ok(WeixinLoginQr {
        qrcode: qr.qrcode,
        qrcode_img_content: qr.qrcode_img_content,
        base_url,
        status: "login_pending".to_string(),
    })
}

#[tauri::command]
pub async fn weixin_clawbot_check_login(
    app: AppHandle,
    request: WeixinLoginCheckRequest,
) -> Result<WeixinLoginCheckResponse, String> {
    tauri::async_runtime::spawn_blocking(move || check_login_blocking(app, request))
        .await
        .map_err(|error| error.to_string())?
}

fn check_login_blocking(
    app: AppHandle,
    request: WeixinLoginCheckRequest,
) -> Result<WeixinLoginCheckResponse, String> {
    let base_url = request
        .base_url
        .as_deref()
        .map(normalize_base_url)
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    let client = IlinkClient::new(&base_url, None)?;
    let login = client.check_login_status(&request.qrcode, request.verify_code.as_deref())?;
    let effective_base_url = login
        .base_url
        .clone()
        .or(login.redirect_base.clone())
        .map(|value| normalize_base_url(&value))
        .unwrap_or(base_url);

    if let Some(bot_token) = login
        .bot_token
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        let conn = open_weixin_db(&app)?;
        let now = Utc::now().to_rfc3339();
        let expires_at = (Utc::now() + ChronoDuration::hours(24)).to_rfc3339();
        conn.execute(
            "INSERT INTO weixin_clawbot_accounts
             (id, bot_user_id, ilink_user_id, bot_token, base_url, get_updates_buf, status, login_expires_at, last_connected_at, last_error, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, '', 'connected', ?6, ?7, '', ?7, ?7)
             ON CONFLICT(id) DO UPDATE SET
                bot_user_id=excluded.bot_user_id,
                ilink_user_id=excluded.ilink_user_id,
                bot_token=excluded.bot_token,
                base_url=excluded.base_url,
                status='connected',
                login_expires_at=excluded.login_expires_at,
                last_connected_at=excluded.last_connected_at,
                last_error='',
                updated_at=excluded.updated_at",
                params![
                    DEFAULT_ACCOUNT_ID,
                    login.bot_user_id.clone().unwrap_or_default(),
                    login.ilink_user_id.clone().unwrap_or_default(),
                    bot_token,
                    effective_base_url,
                    expires_at,
                now,
            ],
        )
        .map_err(|error| error.to_string())?;
    }

    Ok(WeixinLoginCheckResponse {
        status: login.status.clone(),
        message: login_status_message(&login),
        base_url: effective_base_url,
        confirmed: login.bot_token.is_some(),
        verify_code_required: login.verify_code_required,
        expired: login.expired,
    })
}

#[tauri::command]
pub fn weixin_clawbot_start(app: AppHandle) -> Result<WeixinClawbotStatus, String> {
    let conn = open_weixin_db(&app)?;
    let account = load_account_from_conn(&conn, DEFAULT_ACCOUNT_ID)?
        .ok_or_else(|| "请先扫码连接微信 ClawBot。".to_string())?;

    if account.bot_token.trim().is_empty() {
        return Err("请先扫码连接微信 ClawBot。".to_string());
    }

    {
        let mut runtime = RUNTIME
            .lock()
            .map_err(|_| "无法锁定微信 ClawBot 运行状态。".to_string())?;
        cleanup_finished_runtime(&mut runtime);

        if runtime_is_running(&runtime) {
            return build_status(&app);
        }

        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);
        let app_thread = app.clone();
        let handle = thread::spawn(move || {
            poll_loop(app_thread, DEFAULT_ACCOUNT_ID.to_string(), stop_thread)
        });

        *runtime = Some(WeixinRuntime {
            stop,
            handle: Some(handle),
        });
    }

    update_account_status(&conn, "running", "")?;
    build_status(&app)
}

#[tauri::command]
pub fn weixin_clawbot_stop(app: AppHandle) -> Result<WeixinClawbotStatus, String> {
    {
        let mut runtime = RUNTIME
            .lock()
            .map_err(|_| "无法锁定微信 ClawBot 运行状态。".to_string())?;
        if let Some(mut current) = runtime.take() {
            current.stop.store(true, Ordering::SeqCst);
            if current
                .handle
                .as_ref()
                .map(|handle| handle.is_finished())
                .unwrap_or(false)
            {
                if let Some(handle) = current.handle.take() {
                    let _ = handle.join();
                }
            }
        }
    }

    let conn = open_weixin_db(&app)?;
    update_account_status(&conn, "stopped", "")?;
    build_status(&app)
}

#[tauri::command]
pub fn weixin_clawbot_list_events(app: AppHandle) -> Result<Vec<WeixinClawbotEvent>, String> {
    let conn = open_weixin_db(&app)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, direction, from_user_id, summary, status, error, created_at
             FROM weixin_clawbot_events
             ORDER BY created_at DESC, rowid DESC
             LIMIT 80",
        )
        .map_err(|error| error.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            let raw_from_user_id: String = row.get(2)?;
            Ok(WeixinClawbotEvent {
                id: row.get(0)?,
                direction: row.get(1)?,
                from_user_id: mask_weixin_id(&raw_from_user_id),
                summary: row.get(3)?,
                status: row.get(4)?,
                error: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(|error| error.to_string())?;

    let mut events = Vec::new();
    for row in rows {
        events.push(row.map_err(|error| error.to_string())?);
    }
    Ok(events)
}

fn poll_loop(app: AppHandle, account_id: String, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::SeqCst) {
        let account = match load_account(&app, &account_id) {
            Ok(Some(account)) => account,
            Ok(None) => {
                thread::sleep(StdDuration::from_secs(3));
                continue;
            }
            Err(error) => {
                record_event(
                    &app,
                    &account_id,
                    "system",
                    "",
                    "读取账号失败",
                    "error",
                    &error,
                );
                thread::sleep(StdDuration::from_secs(5));
                continue;
            }
        };

        if account.bot_token.trim().is_empty() {
            record_event(
                &app,
                &account_id,
                "system",
                "",
                "缺少 bot token",
                "error",
                "",
            );
            thread::sleep(StdDuration::from_secs(5));
            continue;
        }

        let client = match IlinkClient::new_polling(&account.base_url, Some(&account.bot_token)) {
            Ok(client) => client,
            Err(error) => {
                record_event(
                    &app,
                    &account_id,
                    "system",
                    "",
                    "创建 iLink 客户端失败",
                    "error",
                    &error,
                );
                thread::sleep(StdDuration::from_secs(5));
                continue;
            }
        };

        match client.get_updates(&account.get_updates_buf) {
            Ok(payload) => {
                if let Err(error) = update_poll_state(&app, &account_id, &payload.get_updates_buf) {
                    record_event(
                        &app,
                        &account_id,
                        "system",
                        "",
                        "更新轮询状态失败",
                        "error",
                        &error,
                    );
                }

                let message_count = payload.messages.len();
                for message in payload.messages {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    handle_incoming_text(&app, &account_id, &client, message);
                }

                if message_count == 0 {
                    let sleep_ms = payload.timeout_ms.unwrap_or(250).min(1000);
                    thread::sleep(StdDuration::from_millis(sleep_ms));
                }
            }
            Err(error) => {
                let _ = update_runtime_error(&app, &account_id, &error);
                record_event(
                    &app,
                    &account_id,
                    "system",
                    "",
                    "轮询微信消息失败",
                    "error",
                    &error,
                );
                thread::sleep(StdDuration::from_secs(5));
            }
        }
    }

    if let Ok(conn) = open_weixin_db(&app) {
        let _ = update_account_status(&conn, "stopped", "");
    }
}

fn handle_incoming_text(
    app: &AppHandle,
    account_id: &str,
    client: &IlinkClient,
    message: IncomingText,
) {
    record_event(
        app,
        account_id,
        "inbound",
        &message.from_user_id,
        &message.text,
        "received",
        "",
    );

    let agent_session_id = match get_or_create_agent_session(
        app,
        account_id,
        &message.from_user_id,
        &message.context_token,
    ) {
        Ok(session_id) => session_id,
        Err(error) => {
            record_event(
                app,
                account_id,
                "system",
                &message.from_user_id,
                "创建对话失败",
                "error",
                &error,
            );
            return;
        }
    };

    let typing_ticket = client
        .get_typing_ticket(&message.from_user_id, &message.context_token)
        .unwrap_or_default();

    if !typing_ticket.is_empty() {
        let _ = client.send_typing(&message.from_user_id, &typing_ticket, 1);
    }

    let reply = match chat::invoke_channel_agent(
        app.clone(),
        agent_session_id,
        message.text.clone(),
        "微信 ClawBot",
    ) {
        Ok(reply) => reply,
        Err(error) => {
            record_event(
                app,
                account_id,
                "system",
                &message.from_user_id,
                "Agent 调用失败",
                "error",
                &error,
            );
            "当前无法生成回复，请稍后再试。".to_string()
        }
    };

    match client.send_message(&message.from_user_id, &message.context_token, &reply) {
        Ok(()) => record_event(
            app,
            account_id,
            "outbound",
            &message.from_user_id,
            &reply,
            "sent",
            "",
        ),
        Err(error) => record_event(
            app,
            account_id,
            "outbound",
            &message.from_user_id,
            &reply,
            "error",
            &error,
        ),
    }

    if !typing_ticket.is_empty() {
        let _ = client.send_typing(&message.from_user_id, &typing_ticket, 2);
    }
}

fn build_status(app: &AppHandle) -> Result<WeixinClawbotStatus, String> {
    let conn = open_weixin_db(app)?;
    let account = load_account_from_conn(&conn, DEFAULT_ACCOUNT_ID)?;
    let running = is_runtime_running();

    Ok(match account {
        Some(account) => {
            let has_token = !account.bot_token.trim().is_empty();
            WeixinClawbotStatus {
                configured: has_token,
                running,
                status: if running {
                    "running".to_string()
                } else {
                    account.status
                },
                bot_user_id: mask_weixin_id(&account.bot_user_id),
                ilink_user_id: mask_weixin_id(&account.ilink_user_id),
                base_url: normalize_base_url(&account.base_url),
                has_token,
                login_expires_at: account.login_expires_at,
                last_connected_at: account.last_connected_at,
                last_poll_at: account.last_poll_at,
                last_error: account.last_error,
            }
        }
        None => WeixinClawbotStatus {
            configured: false,
            running,
            status: "not_configured".to_string(),
            bot_user_id: String::new(),
            ilink_user_id: String::new(),
            base_url: DEFAULT_BASE_URL.to_string(),
            has_token: false,
            login_expires_at: None,
            last_connected_at: None,
            last_poll_at: None,
            last_error: String::new(),
        },
    })
}

fn is_runtime_running() -> bool {
    let mut runtime = match RUNTIME.lock() {
        Ok(runtime) => runtime,
        Err(_) => return false,
    };
    cleanup_finished_runtime(&mut runtime);
    runtime_is_running(&runtime)
}

fn runtime_is_running(runtime: &Option<WeixinRuntime>) -> bool {
    runtime
        .as_ref()
        .map(|current| {
            !current.stop.load(Ordering::SeqCst)
                && current
                    .handle
                    .as_ref()
                    .map(|handle| !handle.is_finished())
                    .unwrap_or(false)
        })
        .unwrap_or(false)
}

fn cleanup_finished_runtime(runtime: &mut Option<WeixinRuntime>) {
    let finished = runtime
        .as_ref()
        .and_then(|current| current.handle.as_ref())
        .map(|handle| handle.is_finished())
        .unwrap_or(false);

    if finished {
        if let Some(mut current) = runtime.take() {
            if let Some(handle) = current.handle.take() {
                let _ = handle.join();
            }
        }
    }
}

fn open_weixin_db(app: &AppHandle) -> Result<Connection, String> {
    let conn = storage::open_database(app)?;
    storage::init_database(&conn)?;
    init_weixin_clawbot_database(&conn)?;
    Ok(conn)
}

fn load_account(app: &AppHandle, account_id: &str) -> Result<Option<WeixinAccount>, String> {
    let conn = open_weixin_db(app)?;
    load_account_from_conn(&conn, account_id)
}

fn load_account_from_conn(
    conn: &Connection,
    account_id: &str,
) -> Result<Option<WeixinAccount>, String> {
    conn.query_row(
        "SELECT bot_user_id, ilink_user_id, bot_token, base_url, get_updates_buf, status,
                login_expires_at, last_connected_at, last_poll_at, last_error
         FROM weixin_clawbot_accounts
         WHERE id = ?1",
        params![account_id],
        |row| {
            Ok(WeixinAccount {
                bot_user_id: row.get(0)?,
                ilink_user_id: row.get(1)?,
                bot_token: row.get(2)?,
                base_url: row.get(3)?,
                get_updates_buf: row.get(4)?,
                status: row.get(5)?,
                login_expires_at: row.get(6)?,
                last_connected_at: row.get(7)?,
                last_poll_at: row.get(8)?,
                last_error: row.get(9)?,
            })
        },
    )
    .optional()
    .map_err(|error| error.to_string())
}

fn upsert_login_pending(conn: &Connection, base_url: &str) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO weixin_clawbot_accounts
         (id, base_url, status, created_at, updated_at)
         VALUES (?1, ?2, 'login_pending', ?3, ?3)
         ON CONFLICT(id) DO UPDATE SET
            base_url=excluded.base_url,
            status='login_pending',
            last_error='',
            updated_at=excluded.updated_at",
        params![DEFAULT_ACCOUNT_ID, base_url, now],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn update_account_status(conn: &Connection, status: &str, error: &str) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE weixin_clawbot_accounts
         SET status = ?1, last_error = ?2, updated_at = ?3
         WHERE id = ?4",
        params![status, error, now, DEFAULT_ACCOUNT_ID],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn update_poll_state(
    app: &AppHandle,
    account_id: &str,
    get_updates_buf: &str,
) -> Result<(), String> {
    let conn = open_weixin_db(app)?;
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE weixin_clawbot_accounts
         SET get_updates_buf = ?1, status = 'running', last_poll_at = ?2, last_error = '', updated_at = ?2
         WHERE id = ?3",
        params![get_updates_buf, now, account_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn update_runtime_error(app: &AppHandle, account_id: &str, error: &str) -> Result<(), String> {
    let conn = open_weixin_db(app)?;
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE weixin_clawbot_accounts
         SET status = 'error', last_error = ?1, updated_at = ?2
         WHERE id = ?3",
        params![error, now, account_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn get_or_create_agent_session(
    app: &AppHandle,
    account_id: &str,
    from_user_id: &str,
    context_token: &str,
) -> Result<String, String> {
    let conn = open_weixin_db(app)?;
    if let Some(session_id) = conn
        .query_row(
            "SELECT agent_session_id
             FROM weixin_clawbot_sessions
             WHERE account_id = ?1 AND from_user_id = ?2",
            params![account_id, from_user_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
    {
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE weixin_clawbot_sessions
             SET last_context_token = ?1, last_message_at = ?2
             WHERE account_id = ?3 AND from_user_id = ?4",
            params![context_token, now, account_id, from_user_id],
        )
        .map_err(|error| error.to_string())?;
        return Ok(session_id);
    }

    let session_id = build_agent_session_id(account_id, from_user_id);
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO weixin_clawbot_sessions
         (account_id, from_user_id, agent_session_id, last_context_token, last_message_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![account_id, from_user_id, session_id, context_token, now],
    )
    .map_err(|error| error.to_string())?;
    Ok(session_id)
}

fn record_event(
    app: &AppHandle,
    account_id: &str,
    direction: &str,
    from_user_id: &str,
    summary: &str,
    status: &str,
    error: &str,
) {
    let Ok(conn) = open_weixin_db(app) else {
        return;
    };

    let now = Utc::now().to_rfc3339();
    let _ = conn.execute(
        "INSERT INTO weixin_clawbot_events
         (id, account_id, direction, from_user_id, summary, status, error, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            build_event_id(),
            account_id,
            direction,
            from_user_id,
            truncate_text(summary, 240),
            status,
            truncate_text(error, 240),
            now,
        ],
    );
}

struct IlinkClient {
    client: Client,
    base_url: String,
    bot_token: Option<String>,
}

impl IlinkClient {
    fn new(base_url: &str, bot_token: Option<&str>) -> Result<Self, String> {
        Self::with_timeout(base_url, bot_token, StdDuration::from_secs(15))
    }

    fn new_polling(base_url: &str, bot_token: Option<&str>) -> Result<Self, String> {
        Self::with_timeout(base_url, bot_token, StdDuration::from_secs(45))
    }

    fn with_timeout(
        base_url: &str,
        bot_token: Option<&str>,
        timeout: StdDuration,
    ) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|error| error.to_string())?;

        Ok(Self {
            client,
            base_url: normalize_base_url(base_url),
            bot_token: bot_token
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned),
        })
    }

    fn fetch_login_qr(&self, local_token_list: Vec<String>) -> Result<LoginQrPayload, String> {
        let body = json!({ "local_token_list": local_token_list });
        let value = self.post_json("ilink/bot/get_bot_qrcode?bot_type=3", &body)?;
        ensure_api_success(&value)?;

        let qrcode = find_string_any(&value, &["qrcode", "qr_code", "qrcode_id"])
            .ok_or_else(|| "微信 ClawBot 未返回二维码。".to_string())?;
        let raw_qrcode_img_content =
            find_string_any(&value, &["qrcode_img_content", "qrcode_url", "qrcodeUrl"])
                .unwrap_or_else(|| qrcode.clone());
        let qrcode_img_content = self.normalize_qrcode_display_content(&raw_qrcode_img_content);

        Ok(LoginQrPayload {
            qrcode,
            qrcode_img_content,
        })
    }

    fn check_login_status(
        &self,
        qrcode: &str,
        verify_code: Option<&str>,
    ) -> Result<LoginStatusPayload, String> {
        let mut url = reqwest::Url::parse(&self.url_for_path("ilink/bot/get_qrcode_status"))
            .map_err(|error| error.to_string())?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("qrcode", qrcode);
            if let Some(code) = verify_code.map(str::trim).filter(|value| !value.is_empty()) {
                query.append_pair("verify_code", code);
            }
        }

        let value = self.get_json(url)?;
        let raw_status = find_string_any(
            &value,
            &[
                "status",
                "qrcode_status",
                "qr_status",
                "login_status",
                "state",
            ],
        )
        .unwrap_or_else(|| "unknown".to_string());
        let status = raw_status.to_lowercase();
        let redirect_base = find_string_any(&value, &["redirect_base", "baseurl", "base_url"])
            .or_else(|| find_string_any(&value, &["redirect_host", "redirectHost"]))
            .map(|value| normalize_base_url(&value));
        let bot_token = find_string_any(&value, &["bot_token", "botToken", "token"]);

        Ok(LoginStatusPayload {
            status: status.clone(),
            bot_token,
            bot_user_id: find_string_any(&value, &["bot_user_id", "botUserId"]),
            ilink_user_id: find_string_any(&value, &["ilink_user_id", "ilinkUserId"]),
            base_url: find_string_any(&value, &["baseurl", "base_url", "baseUrl"]),
            redirect_base,
            verify_code_required: contains_status(&status, &["need_verifycode", "verify"]),
            expired: contains_status(&status, &["expired"]),
            already_connected: contains_status(&status, &["already_connected", "binded"]),
            verify_code_blocked: contains_status(&status, &["verify_code_blocked", "blocked"]),
        })
    }

    fn get_updates(&self, get_updates_buf: &str) -> Result<GetUpdatesPayload, String> {
        let body = json!({
            "get_updates_buf": get_updates_buf,
            "base_info": base_info(),
        });
        let value = self.post_json("ilink/bot/getupdates", &body)?;
        ensure_api_success(&value)?;

        let next_buf = find_string_any(&value, &["get_updates_buf", "next_get_updates_buf"])
            .unwrap_or_else(|| get_updates_buf.to_string());
        let timeout_ms = find_u64_any(&value, &["longpolling_timeout_ms", "timeout_ms"]);
        let mut messages = Vec::new();
        collect_text_messages(&value, &mut messages);

        Ok(GetUpdatesPayload {
            get_updates_buf: next_buf,
            timeout_ms,
            messages,
        })
    }

    fn get_typing_ticket(&self, from_user_id: &str, context_token: &str) -> Result<String, String> {
        let body = json!({
            "ilink_user_id": from_user_id,
            "context_token": context_token,
            "base_info": base_info(),
        });
        let value = self.post_json("ilink/bot/getconfig", &body)?;
        ensure_api_success(&value)?;
        Ok(find_string_any(&value, &["typing_ticket", "typingTicket"]).unwrap_or_default())
    }

    fn send_typing(
        &self,
        from_user_id: &str,
        typing_ticket: &str,
        status: i64,
    ) -> Result<(), String> {
        let body = json!({
            "ilink_user_id": from_user_id,
            "typing_ticket": typing_ticket,
            "status": status,
            "base_info": base_info(),
        });
        let value = self.post_json("ilink/bot/sendtyping", &body)?;
        ensure_api_success(&value)
    }

    fn send_message(
        &self,
        to_user_id: &str,
        context_token: &str,
        text: &str,
    ) -> Result<(), String> {
        let body = json!({
            "msg": {
                "from_user_id": "",
                "to_user_id": to_user_id,
                "client_id": format!("openclaw-weixin-{}", random_hex(8)),
                "message_type": 2,
                "message_state": 2,
                "context_token": context_token,
                "item_list": [
                    {
                        "type": 1,
                        "text_item": {
                            "text": text,
                        },
                    },
                ],
            },
            "base_info": base_info(),
        });
        let value = self.post_json("ilink/bot/sendmessage", &body)?;
        ensure_api_success(&value)
    }

    fn normalize_qrcode_display_content(&self, content: &str) -> String {
        let trimmed = content.trim();
        if trimmed.is_empty() || trimmed.starts_with("data:image/") {
            return trimmed.to_string();
        }

        if trimmed.starts_with("<svg") {
            return format!(
                "data:image/svg+xml;base64,{}",
                base64_encode(trimmed.as_bytes())
            );
        }

        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            return self
                .fetch_remote_qrcode_image(trimmed)
                .unwrap_or_else(|_| trimmed.to_string());
        }

        trimmed.to_string()
    }

    fn fetch_remote_qrcode_image(&self, url: &str) -> Result<String, String> {
        let response = self
            .with_headers(self.client.get(url))
            .send()
            .map_err(|error| error.to_string())?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!("二维码图片 HTTP {}", status.as_u16()));
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .map(str::trim)
            .map(ToOwned::to_owned);
        let bytes = response.bytes().map_err(|error| error.to_string())?;
        if bytes.is_empty() {
            return Err("二维码图片为空。".to_string());
        }

        let mime = content_type
            .as_deref()
            .filter(|value| value.starts_with("image/"))
            .map(ToOwned::to_owned)
            .or_else(|| sniff_image_mime(&bytes).map(ToOwned::to_owned))
            .ok_or_else(|| "二维码地址返回的不是图片内容。".to_string())?;

        Ok(format!("data:{mime};base64,{}", base64_encode(&bytes)))
    }

    fn post_json(&self, path: &str, body: &Value) -> Result<Value, String> {
        let request = self.client.post(self.url_for_path(path)).json(body);
        let response = self
            .with_headers(request)
            .send()
            .map_err(|error| error.to_string())?;
        parse_response(response)
    }

    fn get_json(&self, url: reqwest::Url) -> Result<Value, String> {
        let request = self.client.get(url);
        let response = self
            .with_headers(request)
            .send()
            .map_err(|error| error.to_string())?;
        parse_response(response)
    }

    fn url_for_path(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    fn with_headers(&self, request: RequestBuilder) -> RequestBuilder {
        let uin = next_uin().to_string();
        let mut request = request
            .header("Content-Type", "application/json")
            .header("AuthorizationType", "ilink_bot_token")
            .header("X-WECHAT-UIN", base64_encode(uin.as_bytes()))
            .header("iLink-App-Id", ILINK_APP_ID)
            .header("iLink-App-ClientVersion", ILINK_APP_CLIENT_VERSION);

        if let Some(token) = self.bot_token.as_ref() {
            request = request.header("Authorization", format!("Bearer {token}"));
        }

        request
    }
}

fn parse_response(response: reqwest::blocking::Response) -> Result<Value, String> {
    let status = response.status();
    let text = response.text().map_err(|error| error.to_string())?;

    if !status.is_success() {
        return Err(format!(
            "iLink HTTP {}: {}",
            status.as_u16(),
            truncate_text(&text, 200)
        ));
    }

    if text.trim().is_empty() {
        return Ok(Value::Null);
    }

    serde_json::from_str(&text).map_err(|error| format!("iLink 返回不是有效 JSON：{error}"))
}

fn ensure_api_success(value: &Value) -> Result<(), String> {
    let code = value
        .get("ret")
        .and_then(Value::as_i64)
        .or_else(|| value.get("errcode").and_then(Value::as_i64));

    if let Some(code) = code.filter(|code| *code != 0) {
        let message = find_string_any(value, &["errmsg", "message", "err_msg"]).unwrap_or_default();
        return Err(format!("iLink 返回错误 {code}: {message}"));
    }

    Ok(())
}

fn base_info() -> Value {
    json!({
        "channel_version": CHANNEL_VERSION,
        "bot_agent": BOT_AGENT,
    })
}

fn collect_text_messages(value: &Value, out: &mut Vec<IncomingText>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_text_messages(item, out);
            }
        }
        Value::Object(object) => {
            if let Some(items) = object.get("item_list").and_then(Value::as_array) {
                let message_type = object_value_string(object, &["message_type", "messageType"]);
                let is_text_message = message_type
                    .as_deref()
                    .map(|value| value == "1")
                    .unwrap_or(true);

                if is_text_message {
                    if let (Some(from_user_id), Some(context_token), Some(text)) = (
                        object_value_string(object, &["from_user_id", "fromUserId"]),
                        object_value_string(object, &["context_token", "contextToken"]),
                        extract_text_item(items),
                    ) {
                        if !text.trim().is_empty() {
                            out.push(IncomingText {
                                from_user_id,
                                context_token,
                                text,
                            });
                        }
                    }
                }
            }

            for item in object.values() {
                collect_text_messages(item, out);
            }
        }
        _ => {}
    }
}

fn extract_text_item(items: &[Value]) -> Option<String> {
    for item in items {
        let Value::Object(object) = item else {
            continue;
        };
        let item_type = object_value_string(object, &["type"]).unwrap_or_else(|| "1".to_string());
        if item_type != "1" {
            continue;
        }
        if let Some(text) = object
            .get("text_item")
            .or_else(|| object.get("textItem"))
            .and_then(|item| item.get("text"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
        {
            return Some(text.to_string());
        }
    }
    None
}

fn object_value_string(object: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = object.get(*key).and_then(value_to_string) {
            return Some(value);
        }
    }
    None
}

fn find_string_any(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = find_value_by_key(value, key).and_then(value_to_string) {
            return Some(value);
        }
    }
    None
}

fn find_u64_any(value: &Value, keys: &[&str]) -> Option<u64> {
    for key in keys {
        if let Some(value) = find_value_by_key(value, key).and_then(Value::as_u64) {
            return Some(value);
        }
    }
    None
}

fn find_value_by_key<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(object) => {
            if let Some(value) = object.get(key) {
                return Some(value);
            }
            for child in object.values() {
                if let Some(found) = find_value_by_key(child, key) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(items) => {
            for item in items {
                if let Some(found) = find_value_by_key(item, key) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn sniff_image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if bytes.starts_with(b"\xff\xd8\xff") {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes
        .iter()
        .copied()
        .skip_while(u8::is_ascii_whitespace)
        .take(4)
        .collect::<Vec<u8>>()
        .as_slice()
        == b"<svg"
    {
        return Some("image/svg+xml");
    }
    None
}

fn contains_status(status: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| status.contains(needle))
}

fn login_status_message(login: &LoginStatusPayload) -> String {
    if login.bot_token.is_some() {
        return "微信 ClawBot 已连接。".to_string();
    }
    if login.verify_code_required {
        return "需要输入手机微信显示的配对码。".to_string();
    }
    if login.verify_code_blocked {
        return "配对码错误次数过多，请重新生成二维码。".to_string();
    }
    if login.expired {
        return "二维码已过期，请重新生成。".to_string();
    }
    if login.already_connected {
        return "服务端提示该 OpenClaw 端已连接过，请重新生成二维码。".to_string();
    }
    if contains_status(&login.status, &["scan"]) {
        return "已扫码，等待手机端确认。".to_string();
    }
    "等待微信扫码确认。".to_string()
}

fn normalize_base_url(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        DEFAULT_BASE_URL.to_string()
    } else if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{}", trimmed.trim_start_matches('/'))
    }
}

fn build_agent_session_id(account_id: &str, from_user_id: &str) -> String {
    let mut hasher = DefaultHasher::new();
    account_id.hash(&mut hasher);
    from_user_id.hash(&mut hasher);
    format!("weixin-{account_id}-{:016x}", hasher.finish())
}

fn build_event_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let counter = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("weixin-event-{timestamp}-{counter}")
}

fn next_uin() -> u32 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or_default();
    let counter = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    ((now ^ counter) & 0xFFFF_FFFF) as u32
}

fn random_hex(len: usize) -> String {
    format!("{:016x}", next_uin()).chars().take(len).collect()
}

fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::new();
    let mut index = 0;

    while index < input.len() {
        let b0 = input[index];
        let b1 = input.get(index + 1).copied().unwrap_or(0);
        let b2 = input.get(index + 2).copied().unwrap_or(0);

        output.push(TABLE[(b0 >> 2) as usize] as char);
        output.push(TABLE[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);

        if index + 1 < input.len() {
            output.push(TABLE[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            output.push('=');
        }

        if index + 2 < input.len() {
            output.push(TABLE[(b2 & 0b0011_1111) as usize] as char);
        } else {
            output.push('=');
        }

        index += 3;
    }

    output
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    let mut result = String::new();
    for (index, character) in value.chars().enumerate() {
        if index >= max_chars {
            result.push_str("...");
            break;
        }
        result.push(character);
    }
    result
}

fn mask_weixin_id(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= 8 {
        return value.to_string();
    }

    let prefix: String = chars.iter().take(4).collect();
    let suffix: String = chars.iter().skip(chars.len().saturating_sub(4)).collect();
    format!("{prefix}...{suffix}")
}
