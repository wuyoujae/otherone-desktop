use crate::{artifacts, chat, storage};
use aes::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyInit};
use aes::Aes128;
use chrono::{Duration as ChronoDuration, Utc};
use reqwest::blocking::{Client, RequestBuilder};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{hash_map::DefaultHasher, HashMap, HashSet};
use std::fs;
#[cfg(debug_assertions)]
use std::fs::OpenOptions;
use std::hash::{Hash, Hasher};
#[cfg(debug_assertions)]
use std::io::Write;
use std::path::Path;
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
const BOT_AGENT: &str = "weixin-ClawBot-API/1.0.1 (python)";
const MESSAGE_SEND_RETRY_ATTEMPTS: usize = 3;
const MESSAGE_SEND_RETRY_DELAY_MS: u64 = 800;
const CHANNEL_AGENT_RESULT_TIMEOUT_SECS: u64 = 120;
const WEIXIN_CDN_BASE_URL: &str = "https://novac2c.cdn.weixin.qq.com/c2c";
const WEIXIN_FILE_MEDIA_TYPE: i64 = 3;
const WEIXIN_FILE_ITEM_TYPE: i64 = 4;
const MAX_WEIXIN_FILE_ATTACHMENT_BYTES: u64 = 25 * 1024 * 1024;
const CDN_UPLOAD_RETRY_ATTEMPTS: usize = 3;

#[cfg(debug_assertions)]
pub(crate) fn weixin_debug(message: impl AsRef<str>) {
    let line = format!(
        "[weixin_clawbot][{}] {}",
        Utc::now().to_rfc3339(),
        message.as_ref()
    );
    eprintln!("{line}");

    let path = std::env::temp_dir().join("otherone-weixin-clawbot-debug.log");
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}

#[cfg(not(debug_assertions))]
pub(crate) fn weixin_debug(_message: impl AsRef<str>) {}

static RUNTIME: LazyLock<Mutex<Option<WeixinRuntime>>> = LazyLock::new(|| Mutex::new(None));
static MESSAGE_STATE: LazyLock<Mutex<WeixinMessageState>> =
    LazyLock::new(|| Mutex::new(WeixinMessageState::default()));
static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

struct WeixinRuntime {
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

#[derive(Default)]
struct WeixinMessageState {
    pending: HashMap<String, PendingWeixinBatch>,
    active: HashMap<String, ActiveWeixinRun>,
    reset_generation: u64,
}

struct ActiveWeixinRun {
    cancel: Option<chat::ChannelAgentCancelSender>,
    context_token: String,
    reset_generation: u64,
}

struct PendingWeixinBatch {
    account_id: String,
    from_user_id: String,
    context_token: String,
    messages: Vec<IncomingText>,
    reset_generation: u64,
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

#[derive(Debug, Clone)]
struct UploadedWeixinFile {
    download_encrypted_query_param: String,
    aeskey_hex: String,
    file_size: u64,
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

pub(crate) fn reset_weixin_runtime_state() -> Result<(), String> {
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

    let active_cancels = {
        let mut state = MESSAGE_STATE
            .lock()
            .map_err(|_| "无法锁定微信消息队列。".to_string())?;
        state.pending.clear();
        state.reset_generation = state.reset_generation.saturating_add(1);
        state
            .active
            .drain()
            .filter_map(|(_, mut run)| run.cancel.take())
            .collect::<Vec<_>>()
    };

    for cancel in active_cancels {
        let _ = cancel.send(());
    }

    Ok(())
}

#[tauri::command]
pub fn weixin_clawbot_status(app: AppHandle) -> Result<WeixinClawbotStatus, String> {
    build_status(&app)
}

#[tauri::command]
pub async fn weixin_clawbot_begin_login(app: AppHandle) -> Result<WeixinLoginQr, String> {
    weixin_debug("command begin_login");
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
    weixin_debug(format!(
        "begin_login base_url={} existing_token_count={}",
        base_url,
        local_token_list.len()
    ));

    let client = IlinkClient::new(
        &base_url,
        existing.as_ref().map(|account| account.bot_token.as_str()),
    )?;
    let qr = client.fetch_login_qr(local_token_list)?;
    weixin_debug(format!(
        "begin_login qr received qrcode_len={} image_len={}",
        qr.qrcode.chars().count(),
        qr.qrcode_img_content.chars().count()
    ));

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
    weixin_debug(format!(
        "command check_login qrcode_len={} base_url={}",
        request.qrcode.chars().count(),
        request
            .base_url
            .as_deref()
            .map(normalize_base_url)
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
    ));
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
    weixin_debug(format!(
        "check_login status={} token_present={} bot_user={} ilink_user={} redirect_base={}",
        login.status,
        login.bot_token.is_some(),
        login
            .bot_user_id
            .as_deref()
            .map(mask_weixin_id)
            .unwrap_or_default(),
        login
            .ilink_user_id
            .as_deref()
            .map(mask_weixin_id)
            .unwrap_or_default(),
        login.redirect_base.clone().unwrap_or_default()
    ));
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
    weixin_debug("command start");
    let conn = open_weixin_db(&app)?;
    let account = load_account_from_conn(&conn, DEFAULT_ACCOUNT_ID)?
        .ok_or_else(|| "请先扫码连接微信 ClawBot。".to_string())?;

    weixin_debug(format!(
        "start account status={} base_url={} token_present={}",
        account.status,
        normalize_base_url(&account.base_url),
        !account.bot_token.trim().is_empty()
    ));

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
    weixin_debug("command stop");
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
pub fn weixin_clawbot_reset(app: AppHandle) -> Result<WeixinClawbotStatus, String> {
    weixin_debug("command reset");
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

    let active_cancels = {
        let mut state = MESSAGE_STATE
            .lock()
            .map_err(|_| "无法锁定微信消息队列。".to_string())?;
        state.pending.clear();
        state.reset_generation = state.reset_generation.saturating_add(1);
        state
            .active
            .drain()
            .filter_map(|(_, mut run)| run.cancel.take())
            .collect::<Vec<_>>()
    };

    for cancel in active_cancels {
        let _ = cancel.send(());
    }

    let conn = open_weixin_db(&app)?;
    conn.execute(
        "DELETE FROM weixin_clawbot_sessions WHERE account_id = ?1",
        params![DEFAULT_ACCOUNT_ID],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "DELETE FROM weixin_clawbot_events WHERE account_id = ?1",
        params![DEFAULT_ACCOUNT_ID],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "DELETE FROM weixin_clawbot_accounts WHERE id = ?1",
        params![DEFAULT_ACCOUNT_ID],
    )
    .map_err(|error| error.to_string())?;

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
    let mut keep_final_status = false;

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
                let message_count = payload.messages.len();
                let mut processed_all = true;
                for message in payload.messages {
                    if stop.load(Ordering::SeqCst) {
                        processed_all = false;
                        break;
                    }
                    if !enqueue_incoming_text(&app, &account_id, &client, message) {
                        processed_all = false;
                        break;
                    }
                }

                if processed_all {
                    if let Err(error) =
                        update_poll_state(&app, &account_id, &payload.get_updates_buf)
                    {
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
                }

                if message_count == 0 {
                    let sleep_ms = payload.timeout_ms.unwrap_or(250).min(1000);
                    thread::sleep(StdDuration::from_millis(sleep_ms));
                }
            }
            Err(error) => {
                if is_weixin_session_expired_error(&error) {
                    let message = "微信 ClawBot 登录已过期，请重新生成二维码并扫码连接。";
                    let _ = expire_weixin_session(&app, &account_id, message);
                    record_event(
                        &app,
                        &account_id,
                        "system",
                        "",
                        "微信 ClawBot 登录已过期",
                        "expired",
                        &error,
                    );
                    stop.store(true, Ordering::SeqCst);
                    keep_final_status = true;
                    break;
                }

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

    if !keep_final_status {
        if let Ok(conn) = open_weixin_db(&app) {
            let _ = update_account_status(&conn, "stopped", "");
        }
    }
}

fn enqueue_incoming_text(
    app: &AppHandle,
    account_id: &str,
    client: &IlinkClient,
    message: IncomingText,
) -> bool {
    record_event(
        app,
        account_id,
        "inbound",
        &message.from_user_id,
        &message.text,
        "received",
        "",
    );
    weixin_debug(format!(
        "inbound accepted account={} from={} context={} text_len={} text_preview={:?}",
        account_id,
        mask_weixin_id(&message.from_user_id),
        token_fingerprint(&message.context_token),
        message.text.chars().count(),
        truncate_text(&message.text, 80)
    ));

    let from_user_id = message.from_user_id.clone();
    let context_token = message.context_token.clone();
    let run_key = format!(
        "{}\n{}",
        weixin_message_key(account_id, &from_user_id),
        build_event_id()
    );
    let reset_generation = {
        let Ok(mut state) = MESSAGE_STATE.lock() else {
            record_event(
                app,
                account_id,
                "system",
                &from_user_id,
                "锁定微信消息队列失败",
                "error",
                "",
            );
            return false;
        };

        let reset_generation = state.reset_generation;
        state.active.insert(
            run_key.clone(),
            ActiveWeixinRun {
                cancel: None,
                context_token: context_token.clone(),
                reset_generation,
            },
        );
        reset_generation
    };

    let batch = PendingWeixinBatch {
        account_id: account_id.to_string(),
        from_user_id,
        context_token,
        messages: vec![message],
        reset_generation,
    };
    let app = app.clone();
    let client = client.clone();
    thread::spawn(move || {
        process_incoming_text_batch(&app, &client, batch, run_key);
    });

    true
}

fn prompts_from_batch(batch: &PendingWeixinBatch) -> Vec<String> {
    batch
        .messages
        .iter()
        .map(|message| message.text.trim().to_string())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
}

fn receive_weixin_agent_result(key: &str, run: chat::ChannelAgentRun) -> Result<String, String> {
    weixin_debug(format!(
        "agent wait result key={}",
        truncate_text(&key.replace('\n', "/"), 96)
    ));
    match run
        .result
        .recv_timeout(StdDuration::from_secs(CHANNEL_AGENT_RESULT_TIMEOUT_SECS))
    {
        Ok(result) => {
            match &result {
                Ok(reply) => weixin_debug(format!(
                    "agent result ok key={} reply_len={} reply_preview={:?}",
                    truncate_text(&key.replace('\n', "/"), 96),
                    reply.chars().count(),
                    truncate_text(reply, 120)
                )),
                Err(error) => weixin_debug(format!(
                    "agent result error key={} error={}",
                    truncate_text(&key.replace('\n', "/"), 96),
                    truncate_text(error, 300)
                )),
            }
            result
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            weixin_debug(format!(
                "agent result timeout key={} timeout_secs={}",
                truncate_text(&key.replace('\n', "/"), 96),
                CHANNEL_AGENT_RESULT_TIMEOUT_SECS
            ));
            cancel_weixin_active_run(key);
            Err(format!(
                "Agent 回复超时，已等待 {} 秒并取消本次微信运行。",
                CHANNEL_AGENT_RESULT_TIMEOUT_SECS
            ))
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            weixin_debug(format!(
                "agent result channel disconnected key={}",
                truncate_text(&key.replace('\n', "/"), 96)
            ));
            Err("Agent 结果通道已关闭。".to_string())
        }
    }
}

fn cancel_weixin_active_run(key: &str) {
    let Ok(mut state) = MESSAGE_STATE.lock() else {
        return;
    };
    if let Some(active) = state.active.get_mut(key) {
        if let Some(cancel) = active.cancel.take() {
            let _ = cancel.send(());
        }
    }
}

fn start_weixin_channel_run(
    app: &AppHandle,
    key: &str,
    agent_session_id: String,
    prompts: Vec<String>,
    reset_generation: u64,
) -> Result<chat::ChannelAgentRun, String> {
    if !is_weixin_active_run_current(key, reset_generation) {
        return Err(chat::CHANNEL_AGENT_CANCELLED_ERROR.to_string());
    }

    let mut run =
        chat::start_channel_agent_run(app.clone(), agent_session_id, prompts, "Weixin ClawBot")?;

    let mut attached_to_active_run = false;
    if let Ok(mut state) = MESSAGE_STATE.lock() {
        if let Some(active) = state
            .active
            .get_mut(key)
            .filter(|active| active.reset_generation == reset_generation)
        {
            active.cancel = run.cancel.take();
            attached_to_active_run = true;
        }
    }

    if !attached_to_active_run {
        if let Some(cancel) = run.cancel.take() {
            let _ = cancel.send(());
        }
        return Err(chat::CHANNEL_AGENT_CANCELLED_ERROR.to_string());
    }

    Ok(run)
}

fn process_incoming_text_batch(
    app: &AppHandle,
    client: &IlinkClient,
    batch: PendingWeixinBatch,
    key: String,
) {
    weixin_debug(format!(
        "run start account={} from={} context={} reset_generation={} key={}",
        batch.account_id,
        mask_weixin_id(&batch.from_user_id),
        token_fingerprint(&batch.context_token),
        batch.reset_generation,
        truncate_text(&key.replace('\n', "/"), 96)
    ));

    if !is_weixin_batch_current(batch.reset_generation) {
        weixin_debug(format!(
            "run skipped stale generation key={}",
            truncate_text(&key.replace('\n', "/"), 96)
        ));
        return;
    }

    let prompts = prompts_from_batch(&batch);
    if prompts.is_empty() {
        weixin_debug(format!(
            "run ignored empty prompt key={}",
            truncate_text(&key.replace('\n', "/"), 96)
        ));
        finish_weixin_active_run(
            app,
            client,
            &key,
            &batch.account_id,
            &batch.from_user_id,
            &batch.context_token,
        );
        return;
    }

    let (agent_session_id, agent_session_created) = match get_or_create_agent_session(
        app,
        &batch.account_id,
        &batch.from_user_id,
        &batch.context_token,
    ) {
        Ok(session_id) => session_id,
        Err(error) => {
            weixin_debug(format!(
                "agent session select failed from={} context={} error={}",
                mask_weixin_id(&batch.from_user_id),
                token_fingerprint(&batch.context_token),
                truncate_text(&error, 300)
            ));
            record_event(
                app,
                &batch.account_id,
                "system",
                &batch.from_user_id,
                "创建微信对话失败",
                "error",
                &error,
            );
            finish_weixin_active_run(
                app,
                client,
                &key,
                &batch.account_id,
                &batch.from_user_id,
                &batch.context_token,
            );
            return;
        }
    };
    weixin_debug(format!(
        "agent session selected session={} created={} prompt_count={} first_prompt_len={}",
        agent_session_id,
        agent_session_created,
        prompts.len(),
        prompts
            .first()
            .map(|prompt| prompt.chars().count())
            .unwrap_or(0)
    ));
    let prompts_for_retry = prompts.clone();
    let mut artifact_session_id = agent_session_id.clone();
    let mut artifact_started_at = Utc::now().to_rfc3339();
    let mut artifact_baseline = snapshot_session_file_artifact_ids(app, &artifact_session_id);

    let typing_ticket = match client.get_typing_ticket(&batch.from_user_id, &batch.context_token) {
        Ok(ticket) => {
            weixin_debug(format!(
                "typing ticket ok from={} context={} ticket={}",
                mask_weixin_id(&batch.from_user_id),
                token_fingerprint(&batch.context_token),
                token_fingerprint(&ticket)
            ));
            ticket
        }
        Err(error) => {
            weixin_debug(format!(
                "typing ticket failed from={} context={} error={}",
                mask_weixin_id(&batch.from_user_id),
                token_fingerprint(&batch.context_token),
                truncate_text(&error, 300)
            ));
            String::new()
        }
    };

    if !typing_ticket.is_empty() {
        match client.send_typing(&batch.from_user_id, &typing_ticket, 1) {
            Ok(()) => weixin_debug(format!(
                "typing on ok from={} ticket={}",
                mask_weixin_id(&batch.from_user_id),
                token_fingerprint(&typing_ticket)
            )),
            Err(error) => weixin_debug(format!(
                "typing on failed from={} ticket={} error={}",
                mask_weixin_id(&batch.from_user_id),
                token_fingerprint(&typing_ticket),
                truncate_text(&error, 300)
            )),
        }
    }

    if !is_weixin_batch_current(batch.reset_generation) {
        let _ = finish_weixin_active_run(
            app,
            client,
            &key,
            &batch.account_id,
            &batch.from_user_id,
            &batch.context_token,
        );
        return;
    }

    let run = match start_weixin_channel_run(
        app,
        &key,
        agent_session_id.clone(),
        prompts,
        batch.reset_generation,
    ) {
        Ok(run) => run,
        Err(error) if chat::is_channel_agent_cancelled(&error) => {
            let _ = finish_weixin_active_run(
                app,
                client,
                &key,
                &batch.account_id,
                &batch.from_user_id,
                &batch.context_token,
            );
            return;
        }
        Err(error) if should_rotate_agent_session(&error) => {
            record_event(
                app,
                &batch.account_id,
                "system",
                &batch.from_user_id,
                "微信 Agent 会话历史异常，已切换新会话重试",
                "retry",
                &error,
            );
            match rotate_agent_session(
                app,
                &batch.account_id,
                &batch.from_user_id,
                &batch.context_token,
            ) {
                Ok(next_session_id) => {
                    artifact_session_id = next_session_id.clone();
                    artifact_started_at = Utc::now().to_rfc3339();
                    artifact_baseline =
                        snapshot_session_file_artifact_ids(app, &artifact_session_id);
                    match start_weixin_channel_run(
                        app,
                        &key,
                        next_session_id,
                        prompts_for_retry.clone(),
                        batch.reset_generation,
                    ) {
                        Ok(run) => run,
                        Err(retry_error) if chat::is_channel_agent_cancelled(&retry_error) => {
                            let _ = finish_weixin_active_run(
                                app,
                                client,
                                &key,
                                &batch.account_id,
                                &batch.from_user_id,
                                &batch.context_token,
                            );
                            return;
                        }
                        Err(retry_error) => {
                            record_event(
                                app,
                                &batch.account_id,
                                "system",
                                &batch.from_user_id,
                                "Agent 调用失败",
                                "error",
                                &retry_error,
                            );
                            let reply_context_token = finish_weixin_active_run(
                                app,
                                client,
                                &key,
                                &batch.account_id,
                                &batch.from_user_id,
                                &batch.context_token,
                            );
                            let fallback = "当前无法生成回复，请稍后再试。".to_string();
                            send_weixin_agent_reply(
                                app,
                                client,
                                &batch,
                                &reply_context_token,
                                &fallback,
                                &[],
                            );
                            if !typing_ticket.is_empty() {
                                send_weixin_typing_with_log(
                                    client,
                                    &batch.from_user_id,
                                    &typing_ticket,
                                    2,
                                    "off",
                                );
                            }
                            return;
                        }
                    }
                }
                Err(retry_error) if chat::is_channel_agent_cancelled(&retry_error) => {
                    let _ = finish_weixin_active_run(
                        app,
                        client,
                        &key,
                        &batch.account_id,
                        &batch.from_user_id,
                        &batch.context_token,
                    );
                    return;
                }
                Err(retry_error) => {
                    record_event(
                        app,
                        &batch.account_id,
                        "system",
                        &batch.from_user_id,
                        "Agent 调用失败",
                        "error",
                        &retry_error,
                    );
                    let reply_context_token = finish_weixin_active_run(
                        app,
                        client,
                        &key,
                        &batch.account_id,
                        &batch.from_user_id,
                        &batch.context_token,
                    );
                    let fallback = "当前无法生成回复，请稍后再试。".to_string();
                    send_weixin_agent_reply(
                        app,
                        client,
                        &batch,
                        &reply_context_token,
                        &fallback,
                        &[],
                    );
                    if !typing_ticket.is_empty() {
                        send_weixin_typing_with_log(
                            client,
                            &batch.from_user_id,
                            &typing_ticket,
                            2,
                            "off",
                        );
                    }
                    return;
                }
            }
        }
        Err(error) => {
            record_event(
                app,
                &batch.account_id,
                "system",
                &batch.from_user_id,
                "Agent 调用失败",
                "error",
                &error,
            );
            let reply_context_token = finish_weixin_active_run(
                app,
                client,
                &key,
                &batch.account_id,
                &batch.from_user_id,
                &batch.context_token,
            );
            let fallback = "当前无法生成回复，请稍后再试。".to_string();
            send_weixin_agent_reply(app, client, &batch, &reply_context_token, &fallback, &[]);
            if !typing_ticket.is_empty() {
                send_weixin_typing_with_log(client, &batch.from_user_id, &typing_ticket, 2, "off");
            }
            return;
        }
    };

    let mut reply_result = receive_weixin_agent_result(&key, run);

    if reply_result
        .as_ref()
        .err()
        .is_some_and(|error| chat::is_channel_agent_cancelled(error))
    {
        let _ = finish_weixin_active_run(
            app,
            client,
            &key,
            &batch.account_id,
            &batch.from_user_id,
            &batch.context_token,
        );
        return;
    }

    if let Err(error) = &reply_result {
        if should_rotate_agent_session(error) {
            record_event(
                app,
                &batch.account_id,
                "system",
                &batch.from_user_id,
                "微信 Agent 会话历史异常，已切换新会话重试",
                "retry",
                error,
            );
            reply_result = match rotate_agent_session(
                app,
                &batch.account_id,
                &batch.from_user_id,
                &batch.context_token,
            ) {
                Ok(next_session_id) => {
                    artifact_session_id = next_session_id.clone();
                    artifact_started_at = Utc::now().to_rfc3339();
                    artifact_baseline =
                        snapshot_session_file_artifact_ids(app, &artifact_session_id);
                    match start_weixin_channel_run(
                        app,
                        &key,
                        next_session_id,
                        prompts_for_retry,
                        batch.reset_generation,
                    )
                    .and_then(|retry_run| receive_weixin_agent_result(&key, retry_run))
                    {
                        Ok(reply) => Ok(reply),
                        Err(retry_error) => Err(retry_error),
                    }
                }
                Err(retry_error) => Err(retry_error),
            };
        }
    }

    if reply_result
        .as_ref()
        .err()
        .is_some_and(|error| chat::is_channel_agent_cancelled(error))
    {
        let _ = finish_weixin_active_run(
            app,
            client,
            &key,
            &batch.account_id,
            &batch.from_user_id,
            &batch.context_token,
        );
        return;
    }

    let reply_context_token = finish_weixin_active_run(
        app,
        client,
        &key,
        &batch.account_id,
        &batch.from_user_id,
        &batch.context_token,
    );

    let mut reply_artifacts = Vec::new();
    let reply = match reply_result {
        Ok(reply) => {
            reply_artifacts = collect_new_session_file_artifacts(
                app,
                &artifact_session_id,
                &artifact_baseline,
                &artifact_started_at,
            );
            reply
        }
        Err(error) => {
            record_event(
                app,
                &batch.account_id,
                "system",
                &batch.from_user_id,
                "Agent 调用失败",
                "error",
                &error,
            );
            "当前无法生成回复，请稍后再试。".to_string()
        }
    };

    send_weixin_agent_reply(
        app,
        client,
        &batch,
        &reply_context_token,
        &reply,
        &reply_artifacts,
    );

    if !typing_ticket.is_empty() {
        send_weixin_typing_with_log(client, &batch.from_user_id, &typing_ticket, 2, "off");
    }
}

fn is_weixin_batch_current(reset_generation: u64) -> bool {
    let Ok(state) = MESSAGE_STATE.lock() else {
        return false;
    };

    state.reset_generation == reset_generation
}

fn is_weixin_active_run_current(key: &str, reset_generation: u64) -> bool {
    let Ok(state) = MESSAGE_STATE.lock() else {
        return false;
    };

    state.reset_generation == reset_generation
        && state
            .active
            .get(key)
            .is_some_and(|active| active.reset_generation == reset_generation)
}

fn finish_weixin_active_run(
    _app: &AppHandle,
    _client: &IlinkClient,
    key: &str,
    _account_id: &str,
    _from_user_id: &str,
    fallback_context_token: &str,
) -> String {
    let Ok(mut state) = MESSAGE_STATE.lock() else {
        return fallback_context_token.to_string();
    };

    state
        .active
        .remove(key)
        .map(|run| run.context_token)
        .filter(|token| !token.trim().is_empty())
        .unwrap_or_else(|| fallback_context_token.to_string())
}

fn send_weixin_typing_with_log(
    client: &IlinkClient,
    from_user_id: &str,
    typing_ticket: &str,
    status: i64,
    label: &str,
) {
    match client.send_typing(from_user_id, typing_ticket, status) {
        Ok(()) => weixin_debug(format!(
            "typing {} ok from={} status={} ticket={}",
            label,
            mask_weixin_id(from_user_id),
            status,
            token_fingerprint(typing_ticket)
        )),
        Err(error) => weixin_debug(format!(
            "typing {} failed from={} status={} ticket={} error={}",
            label,
            mask_weixin_id(from_user_id),
            status,
            token_fingerprint(typing_ticket),
            truncate_text(&error, 300)
        )),
    }
}

fn send_weixin_agent_reply(
    app: &AppHandle,
    client: &IlinkClient,
    batch: &PendingWeixinBatch,
    context_token: &str,
    reply: &str,
    file_artifacts: &[artifacts::FileArtifact],
) {
    weixin_debug(format!(
        "send reply begin account={} to={} context={} reply_len={} artifact_count={} reply_preview={:?}",
        batch.account_id,
        mask_weixin_id(&batch.from_user_id),
        token_fingerprint(context_token),
        reply.chars().count(),
        file_artifacts.len(),
        truncate_text(reply, 120)
    ));
    let mut last_error = String::new();
    for attempt in 1..=MESSAGE_SEND_RETRY_ATTEMPTS {
        weixin_debug(format!(
            "send reply attempt={} to={} context={}",
            attempt,
            mask_weixin_id(&batch.from_user_id),
            token_fingerprint(context_token)
        ));
        match client.send_message(&batch.from_user_id, context_token, reply) {
            Ok(()) => {
                weixin_debug(format!(
                    "send reply ok attempt={} to={} context={}",
                    attempt,
                    mask_weixin_id(&batch.from_user_id),
                    token_fingerprint(context_token)
                ));
                record_event(
                    app,
                    &batch.account_id,
                    "outbound",
                    &batch.from_user_id,
                    reply,
                    "sent",
                    "",
                );
                send_weixin_file_artifacts(app, client, batch, context_token, file_artifacts);
                return;
            }
            Err(error) => {
                weixin_debug(format!(
                    "send reply failed attempt={} to={} context={} error={}",
                    attempt,
                    mask_weixin_id(&batch.from_user_id),
                    token_fingerprint(context_token),
                    truncate_text(&error, 500)
                ));
                last_error = error;
                if attempt < MESSAGE_SEND_RETRY_ATTEMPTS {
                    thread::sleep(StdDuration::from_millis(MESSAGE_SEND_RETRY_DELAY_MS));
                }
            }
        }
    }

    record_event(
        app,
        &batch.account_id,
        "outbound",
        &batch.from_user_id,
        reply,
        "error",
        &format!(
            "发送微信消息失败，已重试 {} 次：{}",
            MESSAGE_SEND_RETRY_ATTEMPTS, last_error
        ),
    );
}

fn snapshot_session_file_artifact_ids(app: &AppHandle, session_id: &str) -> HashSet<String> {
    artifacts::list_file_artifacts(app.clone(), session_id.to_string())
        .unwrap_or_default()
        .into_iter()
        .map(|artifact| artifact.id)
        .collect()
}

fn collect_new_session_file_artifacts(
    app: &AppHandle,
    session_id: &str,
    baseline_ids: &HashSet<String>,
    started_at: &str,
) -> Vec<artifacts::FileArtifact> {
    let mut items = artifacts::list_file_artifacts(app.clone(), session_id.to_string())
        .unwrap_or_default()
        .into_iter()
        .filter(|artifact| {
            matches!(artifact.action.as_str(), "added" | "edited" | "attached")
                && !artifact.file_path.trim().is_empty()
                && (!baseline_ids.contains(&artifact.id)
                    || artifact.created_at.as_str() > started_at)
        })
        .collect::<Vec<_>>();

    items.sort_by(|left, right| left.created_at.cmp(&right.created_at));
    items
}

fn send_weixin_file_artifacts(
    app: &AppHandle,
    client: &IlinkClient,
    batch: &PendingWeixinBatch,
    context_token: &str,
    artifacts: &[artifacts::FileArtifact],
) {
    if artifacts.is_empty() {
        return;
    }

    let mut sent_paths = HashSet::new();
    for artifact in artifacts {
        if !sent_paths.insert(artifact.file_path.clone()) {
            continue;
        }

        let file_name = artifact_file_name(artifact);
        weixin_debug(format!(
            "send file artifact begin account={} to={} context={} artifact_id={} action={} name={}",
            batch.account_id,
            mask_weixin_id(&batch.from_user_id),
            token_fingerprint(context_token),
            artifact.id,
            artifact.action,
            file_name
        ));

        let mut last_error = String::new();
        for attempt in 1..=MESSAGE_SEND_RETRY_ATTEMPTS {
            match client.send_file_attachment(
                &batch.from_user_id,
                context_token,
                &artifact.file_path,
                &file_name,
            ) {
                Ok(()) => {
                    weixin_debug(format!(
                        "send file artifact ok attempt={} to={} context={} name={}",
                        attempt,
                        mask_weixin_id(&batch.from_user_id),
                        token_fingerprint(context_token),
                        file_name
                    ));
                    record_event(
                        app,
                        &batch.account_id,
                        "outbound",
                        &batch.from_user_id,
                        &format!("文件：{file_name}"),
                        "sent",
                        "",
                    );
                    last_error.clear();
                    break;
                }
                Err(error) => {
                    last_error = error;
                    weixin_debug(format!(
                        "send file artifact failed attempt={} to={} context={} name={} error={}",
                        attempt,
                        mask_weixin_id(&batch.from_user_id),
                        token_fingerprint(context_token),
                        file_name,
                        truncate_text(&last_error, 500)
                    ));
                    if attempt < MESSAGE_SEND_RETRY_ATTEMPTS {
                        thread::sleep(StdDuration::from_millis(MESSAGE_SEND_RETRY_DELAY_MS));
                    }
                }
            }
        }

        if !last_error.is_empty() {
            record_event(
                app,
                &batch.account_id,
                "outbound",
                &batch.from_user_id,
                &format!("文件：{file_name}"),
                "error",
                &last_error,
            );
        }
    }
}

fn artifact_file_name(artifact: &artifacts::FileArtifact) -> String {
    if !artifact.name.trim().is_empty() {
        return artifact.name.clone();
    }

    Path::new(&artifact.file_path)
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "otherone-file".to_string())
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

fn expire_weixin_session(app: &AppHandle, account_id: &str, message: &str) -> Result<(), String> {
    let active_cancels = {
        let mut state = MESSAGE_STATE
            .lock()
            .map_err(|_| "无法锁定微信消息队列。".to_string())?;
        let key_prefix = format!("{account_id}\n");
        state.pending.retain(|key, _| !key.starts_with(&key_prefix));
        state.reset_generation = state.reset_generation.saturating_add(1);

        let active = std::mem::take(&mut state.active);
        let mut kept_active = HashMap::new();
        let mut cancels = Vec::new();
        for (key, mut run) in active {
            if key.starts_with(&key_prefix) {
                if let Some(cancel) = run.cancel.take() {
                    cancels.push(cancel);
                }
            } else {
                kept_active.insert(key, run);
            }
        }
        state.active = kept_active;
        cancels
    };

    for cancel in active_cancels {
        let _ = cancel.send(());
    }

    let conn = open_weixin_db(app)?;
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE weixin_clawbot_accounts
         SET bot_token = '',
             get_updates_buf = '',
             status = 'disconnected',
             login_expires_at = NULL,
             last_error = ?1,
             updated_at = ?2
         WHERE id = ?3",
        params![message, now, account_id],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "DELETE FROM weixin_clawbot_sessions WHERE account_id = ?1",
        params![account_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn get_or_create_agent_session(
    app: &AppHandle,
    account_id: &str,
    from_user_id: &str,
    context_token: &str,
) -> Result<(String, bool), String> {
    let conn = open_weixin_db(app)?;
    let existing_session = conn
        .query_row(
            "SELECT agent_session_id
             FROM weixin_clawbot_sessions
             WHERE account_id = ?1 AND from_user_id = ?2 AND agent_session_id <> ''",
            params![account_id, from_user_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;

    let now = Utc::now().to_rfc3339();
    if let Some(session_id) = existing_session {
        conn.execute(
            "UPDATE weixin_clawbot_sessions
             SET last_context_token = ?1, last_message_at = ?2
             WHERE account_id = ?3 AND from_user_id = ?4",
            params![context_token, now, account_id, from_user_id],
        )
        .map_err(|error| error.to_string())?;
        return Ok((session_id, false));
    }

    let session_id = build_agent_session_id(account_id, from_user_id);
    conn.execute(
        "INSERT INTO weixin_clawbot_sessions
         (account_id, from_user_id, agent_session_id, last_context_token, last_message_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(account_id, from_user_id) DO UPDATE SET
            agent_session_id=excluded.agent_session_id,
            last_context_token=excluded.last_context_token,
            last_message_at=excluded.last_message_at",
        params![account_id, from_user_id, session_id, context_token, now],
    )
    .map_err(|error| error.to_string())?;
    Ok((session_id, true))
}

fn rotate_agent_session(
    app: &AppHandle,
    account_id: &str,
    from_user_id: &str,
    context_token: &str,
) -> Result<String, String> {
    let conn = open_weixin_db(app)?;
    let session_id = build_agent_session_id(account_id, from_user_id);
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO weixin_clawbot_sessions
         (account_id, from_user_id, agent_session_id, last_context_token, last_message_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(account_id, from_user_id) DO UPDATE SET
            agent_session_id=excluded.agent_session_id,
            last_context_token=excluded.last_context_token,
            last_message_at=excluded.last_message_at",
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

#[derive(Clone)]
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
                "client_id": build_weixin_client_id(),
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

    fn send_file_attachment(
        &self,
        to_user_id: &str,
        context_token: &str,
        file_path: &str,
        file_name: &str,
    ) -> Result<(), String> {
        let path = Path::new(file_path);
        let metadata = fs::metadata(path).map_err(|error| format!("读取文件信息失败：{error}"))?;
        if !metadata.is_file() {
            return Err("只能发送普通文件。".to_string());
        }

        let file_size = metadata.len();
        if file_size == 0 {
            return Err("文件为空，无法发送到微信。".to_string());
        }
        if file_size > MAX_WEIXIN_FILE_ATTACHMENT_BYTES {
            return Err(format!(
                "文件过大，当前限制 {} MB。",
                MAX_WEIXIN_FILE_ATTACHMENT_BYTES / 1024 / 1024
            ));
        }

        let plaintext = fs::read(path).map_err(|error| format!("读取文件失败：{error}"))?;
        let rawfilemd5 = format!("{:x}", md5::compute(&plaintext));
        let filekey = secure_random_hex(16)?;
        let aes_key = secure_random_bytes_16()?;
        let aeskey_hex = hex_encode(&aes_key);
        let ciphertext_size = aes_ecb_padded_size(plaintext.len());

        weixin_debug(format!(
            "file upload prepare to={} name={} rawsize={} ciphertext_size={}",
            mask_weixin_id(to_user_id),
            file_name,
            file_size,
            ciphertext_size
        ));

        let uploaded = self.upload_file_attachment_to_weixin(
            to_user_id,
            &plaintext,
            file_size,
            ciphertext_size,
            &rawfilemd5,
            &filekey,
            &aes_key,
            &aeskey_hex,
        )?;

        let item = json!({
            "type": WEIXIN_FILE_ITEM_TYPE,
            "file_item": {
                "media": {
                    "encrypt_query_param": uploaded.download_encrypted_query_param,
                    "aes_key": base64_encode(uploaded.aeskey_hex.as_bytes()),
                    "encrypt_type": 1,
                },
                "file_name": file_name,
                "len": uploaded.file_size.to_string(),
            },
        });
        self.send_message_item(to_user_id, context_token, item)
    }

    fn upload_file_attachment_to_weixin(
        &self,
        to_user_id: &str,
        plaintext: &[u8],
        file_size: u64,
        ciphertext_size: usize,
        rawfilemd5: &str,
        filekey: &str,
        aes_key: &[u8; 16],
        aeskey_hex: &str,
    ) -> Result<UploadedWeixinFile, String> {
        let body = json!({
            "filekey": filekey,
            "media_type": WEIXIN_FILE_MEDIA_TYPE,
            "to_user_id": to_user_id,
            "rawsize": file_size,
            "rawfilemd5": rawfilemd5,
            "filesize": ciphertext_size,
            "no_need_thumb": true,
            "aeskey": aeskey_hex,
            "base_info": base_info(),
        });
        let value = self.post_json("ilink/bot/getuploadurl", &body)?;
        ensure_api_success(&value)?;

        let upload_full_url = find_string_any(&value, &["upload_full_url", "uploadFullUrl"]);
        let upload_param = find_string_any(&value, &["upload_param", "uploadParam"]);
        let download_encrypted_query_param = self.upload_buffer_to_cdn(
            plaintext,
            upload_full_url.as_deref(),
            upload_param.as_deref(),
            filekey,
            aes_key,
        )?;

        Ok(UploadedWeixinFile {
            download_encrypted_query_param,
            aeskey_hex: aeskey_hex.to_string(),
            file_size,
        })
    }

    fn upload_buffer_to_cdn(
        &self,
        plaintext: &[u8],
        upload_full_url: Option<&str>,
        upload_param: Option<&str>,
        filekey: &str,
        aes_key: &[u8; 16],
    ) -> Result<String, String> {
        let ciphertext = encrypt_aes_128_ecb(plaintext, aes_key)?;
        let cdn_url = build_cdn_upload_url(upload_full_url, upload_param, filekey)?;
        let mut last_error = String::new();

        for attempt in 1..=CDN_UPLOAD_RETRY_ATTEMPTS {
            let response = self
                .client
                .post(&cdn_url)
                .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
                .body(ciphertext.clone())
                .send();

            match response {
                Ok(response) => {
                    let status = response.status();
                    if status.as_u16() == 200 {
                        let download_param = response
                            .headers()
                            .get("x-encrypted-param")
                            .and_then(|value| value.to_str().ok())
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .map(ToOwned::to_owned)
                            .ok_or_else(|| "CDN 上传响应缺少 x-encrypted-param。".to_string())?;
                        return Ok(download_param);
                    }

                    let error_text = response
                        .headers()
                        .get("x-error-message")
                        .and_then(|value| value.to_str().ok())
                        .map(ToOwned::to_owned)
                        .unwrap_or_else(|| format!("HTTP {}", status.as_u16()));
                    last_error = format!("CDN 上传失败：{error_text}");
                    if status.as_u16() >= 400 && status.as_u16() < 500 {
                        break;
                    }
                }
                Err(error) => {
                    last_error = error.to_string();
                }
            }

            if attempt < CDN_UPLOAD_RETRY_ATTEMPTS {
                thread::sleep(StdDuration::from_millis(MESSAGE_SEND_RETRY_DELAY_MS));
            }
        }

        Err(if last_error.is_empty() {
            "CDN 上传失败。".to_string()
        } else {
            last_error
        })
    }

    fn send_message_item(
        &self,
        to_user_id: &str,
        context_token: &str,
        item: Value,
    ) -> Result<(), String> {
        let body = json!({
            "msg": {
                "from_user_id": "",
                "to_user_id": to_user_id,
                "client_id": build_weixin_client_id(),
                "message_type": 2,
                "message_state": 2,
                "context_token": context_token,
                "item_list": [item],
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
        if should_log_ilink_path(path) {
            weixin_debug(format!(
                "HTTP POST {} base_url={} token_present={} body={}",
                path,
                self.base_url,
                self.bot_token.is_some(),
                summarize_debug_json(body)
            ));
        }

        let request = self.client.post(self.url_for_path(path)).json(body);
        let response = self
            .with_headers(request)
            .send()
            .map_err(|error| error.to_string())?;
        parse_response(path, response)
    }

    fn get_json(&self, url: reqwest::Url) -> Result<Value, String> {
        let path = url.path().to_string();
        let request = self.client.get(url);
        let response = self
            .with_headers(request)
            .send()
            .map_err(|error| error.to_string())?;
        parse_response(&path, response)
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

fn parse_response(path: &str, response: reqwest::blocking::Response) -> Result<Value, String> {
    let status = response.status();
    let text = response.text().map_err(|error| error.to_string())?;

    if should_log_ilink_path(path) {
        weixin_debug(format!(
            "HTTP {} response status={} body={}",
            path,
            status.as_u16(),
            summarize_debug_response(path, &text)
        ));
    }

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
    let code = find_i64_any(
        value,
        &["ret", "errcode", "errCode", "error_code", "errorCode"],
    );

    if let Some(code) = code.filter(|code| *code != 0) {
        let message = find_string_any(
            value,
            &[
                "errmsg",
                "message",
                "err_msg",
                "errMsg",
                "error_message",
                "errorMessage",
            ],
        )
        .unwrap_or_default();
        return Err(format!("iLink 返回错误 {code}: {message}"));
    }

    Ok(())
}

fn should_log_ilink_path(path: &str) -> bool {
    let normalized = path.to_ascii_lowercase();
    normalized.contains("getconfig")
        || normalized.contains("sendtyping")
        || normalized.contains("sendmessage")
        || normalized.contains("getuploadurl")
}

fn summarize_debug_json(value: &Value) -> String {
    let mut sanitized = value.clone();
    sanitize_debug_value(&mut sanitized);
    let text =
        serde_json::to_string(&sanitized).unwrap_or_else(|_| "<json serialize failed>".into());
    truncate_text(&text, 1600)
}

fn summarize_debug_response(path: &str, text: &str) -> String {
    if text.trim().is_empty() {
        return "<empty>".to_string();
    }

    if should_log_ilink_path(path) {
        if let Ok(mut value) = serde_json::from_str::<Value>(text) {
            sanitize_debug_value(&mut value);
            return summarize_debug_json(&value);
        }
    }

    truncate_text(text, 1600)
}

fn sanitize_debug_value(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for (key, child) in object.iter_mut() {
                let normalized = key.to_ascii_lowercase();
                if let Some(raw) = child.as_str() {
                    if normalized == "text" {
                        *child = Value::String(format!(
                            "len={} preview={:?}",
                            raw.chars().count(),
                            truncate_text(raw, 120)
                        ));
                        continue;
                    }

                    if normalized.contains("token")
                        || normalized.contains("ticket")
                        || normalized.contains("authorization")
                        || normalized.contains("aeskey")
                        || normalized.contains("filekey")
                        || normalized.contains("upload")
                        || normalized.contains("encrypt")
                        || normalized == "full_url"
                    {
                        *child = Value::String(token_fingerprint(raw));
                        continue;
                    }

                    if normalized == "from_user_id"
                        || normalized == "to_user_id"
                        || normalized == "ilink_user_id"
                    {
                        *child = Value::String(mask_weixin_id(raw));
                        continue;
                    }
                }

                sanitize_debug_value(child);
            }
        }
        Value::Array(items) => {
            for item in items {
                sanitize_debug_value(item);
            }
        }
        _ => {}
    }
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

fn find_i64_any(value: &Value, keys: &[&str]) -> Option<i64> {
    for key in keys {
        if let Some(value) = find_value_by_key(value, key).and_then(value_to_i64) {
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

fn value_to_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Number(value) => value.as_i64(),
        Value::String(value) => value.trim().parse::<i64>().ok(),
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

fn should_rotate_agent_session(error: &str) -> bool {
    let normalized = error.to_lowercase();
    normalized.contains("tool_calls")
        && (normalized.contains("must be followed")
            || normalized.contains("insufficient tool messages")
            || normalized.contains("tool_call_id"))
}

fn is_weixin_session_expired_error(error: &str) -> bool {
    let normalized = error.to_lowercase();
    normalized.contains("session timeout")
        || normalized.contains("返回错误 -14")
        || normalized.contains("invalid token")
        || normalized.contains("token expired")
}

fn build_agent_session_id(account_id: &str, from_user_id: &str) -> String {
    let mut hasher = DefaultHasher::new();
    account_id.hash(&mut hasher);
    from_user_id.hash(&mut hasher);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let counter = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "weixin-{account_id}-{:016x}-{timestamp}-{counter}",
        hasher.finish()
    )
}

fn weixin_message_key(account_id: &str, from_user_id: &str) -> String {
    format!("{account_id}\n{from_user_id}")
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
    let mut output = String::new();
    while output.len() < len {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or_default();
        let counter = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let mixed = now ^ counter.rotate_left(17) ^ next_uin() as u64;
        output.push_str(&format!("{mixed:016x}"));
    }
    output.truncate(len);
    output
}

fn build_weixin_client_id() -> String {
    let suffix = secure_random_hex(8).unwrap_or_else(|_| random_hex(16));
    format!("openclaw-weixin-{suffix}")
}

fn secure_random_bytes_16() -> Result<[u8; 16], String> {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).map_err(|error| format!("生成随机数失败：{error}"))?;
    Ok(bytes)
}

fn secure_random_hex(bytes_len: usize) -> Result<String, String> {
    let mut bytes = vec![0u8; bytes_len];
    getrandom::getrandom(&mut bytes).map_err(|error| format!("生成随机数失败：{error}"))?;
    Ok(hex_encode(&bytes))
}

fn hex_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(input.len() * 2);
    for byte in input {
        output.push(TABLE[(byte >> 4) as usize] as char);
        output.push(TABLE[(byte & 0x0f) as usize] as char);
    }
    output
}

fn aes_ecb_padded_size(plaintext_size: usize) -> usize {
    ((plaintext_size + 1 + 15) / 16) * 16
}

fn encrypt_aes_128_ecb(plaintext: &[u8], aes_key: &[u8; 16]) -> Result<Vec<u8>, String> {
    let mut buffer = plaintext.to_vec();
    let plaintext_len = buffer.len();
    buffer.resize(plaintext_len + 16, 0);

    ecb::Encryptor::<Aes128>::new_from_slice(aes_key)
        .map_err(|error| format!("初始化 AES 加密失败：{error}"))?
        .encrypt_padded_mut::<Pkcs7>(&mut buffer, plaintext_len)
        .map(|ciphertext| ciphertext.to_vec())
        .map_err(|error| format!("AES 加密失败：{error}"))
}

fn build_cdn_upload_url(
    upload_full_url: Option<&str>,
    upload_param: Option<&str>,
    filekey: &str,
) -> Result<String, String> {
    if let Some(url) = upload_full_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(url.to_string());
    }

    let upload_param = upload_param
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "getuploadurl 未返回 upload_full_url 或 upload_param。".to_string())?;
    let mut url = reqwest::Url::parse(&format!(
        "{}/upload",
        WEIXIN_CDN_BASE_URL.trim_end_matches('/')
    ))
    .map_err(|error| error.to_string())?;
    url.query_pairs_mut()
        .append_pair("encrypted_query_param", upload_param)
        .append_pair("filekey", filekey);
    Ok(url.to_string())
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

fn token_fingerprint(value: &str) -> String {
    let chars: Vec<char> = value.trim().chars().collect();
    if chars.is_empty() {
        return "len=0".to_string();
    }

    if chars.len() <= 12 {
        return format!("len={} <redacted>", chars.len());
    }

    let prefix: String = chars.iter().take(6).collect();
    let suffix: String = chars.iter().skip(chars.len().saturating_sub(6)).collect();
    format!("len={} {}...{}", chars.len(), prefix, suffix)
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
