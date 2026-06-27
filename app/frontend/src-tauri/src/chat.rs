use crate::tools;
use otherone::agent::types::{AiOptions, ContextLoadType, InputOptions, StorageType};
use otherone::agent::{AgentStreamCommand, StreamAgentEvent};
use otherone::ai::types::{ProviderType, ToolChoice};
use otherone::storage::types::{StorageType as EntryStorageType, WriteEntryOptions};
use otherone::Otherone;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

/// 取消令牌映射：session_id → oneshot Sender
/// 前端调用 cancel_chat_message 时触发对应 session 的取消信号
static CANCEL_TOKENS: LazyLock<Mutex<HashMap<String, tokio::sync::oneshot::Sender<()>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static ACTIVE_CHAT_COMMANDS: LazyLock<
    Mutex<HashMap<String, tokio::sync::mpsc::Sender<AgentStreamCommand>>>,
> = LazyLock::new(|| Mutex::new(HashMap::new()));
const DESKTOP_LONG_TERM_MEMORY_RECALL_MAX_TYPES: usize = 5;

// ---------------------------------------------------------------------------
// 基础 System Prompt（内置，用户不可修改）
// ---------------------------------------------------------------------------

const BASE_SYSTEM_PROMPT: &str = r#"You are otherone-agent — also known as 豆仔 (Douzi).
You are a capable, friendly desktop AI assistant running inside the otherone application.

Your job: help the user with any task they ask for — reading files, writing code,
answering questions, reasoning through problems, or automating multi-step work.

You are running on ${SYSTEM_INFO}. The current time is ${CURRENT_TIME}.

You have access to tools. When a task requires reading a file or writing one, use
them — do not guess from memory when you can read the real thing. Work inside
${USERTOORPATH} by default unless the user specifies another directory.

${AVAILABLE_SKILLS}

Be concise. Be direct. Be helpful. Speak the user's language."#;

use crate::app_settings;
use crate::session;
use crate::storage::{self, ModelConfig, ProviderConfig};

static ACTIVE_CHATS: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Copy)]
enum AgentToolScope {
    FullDesktop,
    WeixinSafe,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendChatMessageRequest {
    pub session_id: Option<String>,
    pub model_id: String,
    pub prompt: String,
    pub prompts: Option<Vec<String>>,
    pub reasoning_effort: Option<String>,
    pub context_compression_enabled: bool,
    pub branch_mode_enabled: bool,
    pub target_mode_enabled: bool,
    #[serde(default = "default_memory_enabled")]
    pub memory_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendChatMessageResponse {
    pub session_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueChatMessageRequest {
    pub session_id: String,
    pub prompt: String,
    pub prompts: Option<Vec<String>>,
}

fn normalized_request_prompts(request: &SendChatMessageRequest) -> Vec<String> {
    request
        .prompts
        .as_ref()
        .map(|prompts| {
            prompts
                .iter()
                .map(|prompt| prompt.trim())
                .filter(|prompt| !prompt.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|prompts| !prompts.is_empty())
        .unwrap_or_else(|| {
            let prompt = request.prompt.trim();
            if prompt.is_empty() {
                Vec::new()
            } else {
                vec![prompt.to_string()]
            }
        })
}

fn normalized_enqueue_prompts(request: &EnqueueChatMessageRequest) -> Vec<String> {
    request
        .prompts
        .as_ref()
        .map(|prompts| {
            prompts
                .iter()
                .map(|prompt| prompt.trim())
                .filter(|prompt| !prompt.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|prompts| !prompts.is_empty())
        .unwrap_or_else(|| {
            let prompt = request.prompt.trim();
            if prompt.is_empty() {
                Vec::new()
            } else {
                vec![prompt.to_string()]
            }
        })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatStreamEvent {
    pub session_id: String,
    pub event_type: String,
    pub content: String,
    pub raw_chunk: Option<Value>,
    pub error: Option<String>,
    /// 工具标签 — 用户可见的友好文本，如 "正在读取 a.txt"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_label: Option<String>,
    /// 工具结果是否可展开
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_expandable: Option<bool>,
    /// 可展开时的详情内容
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_detail: Option<String>,
    /// 工具执行状态: "running" | "completed" | "error"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_status: Option<String>,
}

impl ChatStreamEvent {
    fn plain(
        session_id: String,
        event_type: &str,
        content: String,
        raw_chunk: Option<Value>,
        error: Option<String>,
    ) -> Self {
        ChatStreamEvent {
            session_id,
            event_type: event_type.to_string(),
            content,
            raw_chunk,
            error,
            tool_label: None,
            tool_expandable: None,
            tool_detail: None,
            tool_status: None,
        }
    }

    fn tool(
        session_id: String,
        label: String,
        expandable: bool,
        detail: Option<String>,
        status: &str,
    ) -> Self {
        ChatStreamEvent {
            session_id,
            event_type: "tool_call".to_string(),
            content: String::new(),
            raw_chunk: None,
            error: None,
            tool_label: Some(label),
            tool_expandable: Some(expandable),
            tool_detail: detail,
            tool_status: Some(status.to_string()),
        }
    }
}

struct DeltaSegment {
    event_type: &'static str,
    content: String,
}

pub type ChannelAgentCommandSender = tokio::sync::mpsc::Sender<AgentStreamCommand>;

pub struct ChannelAgentRun {
    pub commands: ChannelAgentCommandSender,
    pub result: std::sync::mpsc::Receiver<Result<String, String>>,
}

struct ActiveChatGuard;

impl Drop for ActiveChatGuard {
    fn drop(&mut self) {
        ACTIVE_CHATS.fetch_sub(1, Ordering::SeqCst);
    }
}

pub(crate) fn has_active_chat() -> bool {
    ACTIVE_CHATS.load(Ordering::SeqCst) > 0
}

/// 取消指定 session 的正在进行的流式对话
#[tauri::command]
pub fn cancel_chat_message(session_id: String) -> Result<(), String> {
    let sid = session_id.trim().to_string();
    if sid.is_empty() {
        return Err("session_id is required".to_string());
    }

    let sender = CANCEL_TOKENS
        .lock()
        .map_err(|_| "无法获取取消令牌锁。".to_string())?
        .remove(&sid);

    if let Some(tx) = sender {
        // oneshot send 可能失败（接收端已关闭），忽略即可
        let _ = tx.send(());
        eprintln!("[chat_stream] session={} 取消信号已发送", sid);
    }

    let _ = ACTIVE_CHAT_COMMANDS
        .lock()
        .map(|mut commands| commands.remove(&sid));

    Ok(())
}

#[tauri::command]
pub fn enqueue_chat_message(request: EnqueueChatMessageRequest) -> Result<(), String> {
    let session_id = require_text(&request.session_id, "session_id")?.to_string();
    let prompts = normalized_enqueue_prompts(&request);
    if prompts.is_empty() {
        return Err("消息内容不能为空。".to_string());
    }

    let sender = ACTIVE_CHAT_COMMANDS
        .lock()
        .map_err(|_| "无法获取运行中对话队列锁。".to_string())?
        .get(&session_id)
        .cloned()
        .ok_or_else(|| "当前会话没有正在运行的 Agent。".to_string())?;

    tauri::async_runtime::block_on(async move {
        sender
            .send(AgentStreamCommand::EnqueueUserPrompts(prompts))
            .await
            .map_err(|_| "运行中对话队列已关闭。".to_string())
    })
}

pub fn enqueue_channel_agent_prompts(
    sender: &ChannelAgentCommandSender,
    prompts: Vec<String>,
) -> Result<(), String> {
    let prompts = prompts
        .into_iter()
        .map(|prompt| prompt.trim().to_string())
        .filter(|prompt| !prompt.is_empty())
        .collect::<Vec<_>>();
    if prompts.is_empty() {
        return Err("外部消息不能为空。".to_string());
    }

    let sender = sender.clone();
    tauri::async_runtime::block_on(async move {
        sender
            .send(AgentStreamCommand::EnqueueUserPrompts(prompts))
            .await
            .map_err(|_| "运行中的外部 Agent 队列已关闭。".to_string())
    })
}

fn write_user_prompt_entries_blocking(session_id: &str, prompts: &[String]) -> Result<(), String> {
    for prompt in prompts {
        tauri::async_runtime::block_on(otherone::storage::write_entry(&WriteEntryOptions {
            storage_type: EntryStorageType::LocalFile,
            session_id: session_id.to_string(),
            role: "user".to_string(),
            content: resolve_prompt(prompt),
            tools: None,
            token_consumption: None,
            create_at: None,
            database_config: None,
        }))
        .map_err(|error| error.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub fn send_chat_message(
    app: AppHandle,
    request: SendChatMessageRequest,
) -> Result<SendChatMessageResponse, String> {
    let prompts = normalized_request_prompts(&request);
    if prompts.is_empty() {
        return Err("消息内容不能为空。".to_string());
    }

    let session_id = request
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(generate_session_id);
    let model_id = require_text(&request.model_id, "模型")?.to_string();
    let providers = storage::load_api_configs(app.clone())?;
    let (provider, model) = find_model(&providers, &model_id)?;
    let settings = app_settings::load_settings(&app)?;
    let dialogue_root = session::agent_storage_root(&app)?;
    let event_session_id = session_id.clone();
    let _reserved_modes = (request.branch_mode_enabled, request.target_mode_enabled);

    ACTIVE_CHATS.fetch_add(1, Ordering::SeqCst);

    std::thread::spawn(move || {
        let _active_guard = ActiveChatGuard;
        let result = run_chat_stream(
            app.clone(),
            request,
            event_session_id.clone(),
            prompts,
            provider,
            model,
            settings,
            dialogue_root,
        );

        if let Err(error) = result {
            emit_event(
                &app,
                ChatStreamEvent::plain(event_session_id, "error", String::new(), None, Some(error)),
            );
        }
    });

    Ok(SendChatMessageResponse { session_id })
}

fn run_chat_stream(
    app: AppHandle,
    request: SendChatMessageRequest,
    session_id: String,
    prompts: Vec<String>,
    provider: ProviderConfig,
    model: ModelConfig,
    settings: app_settings::AppSettings,
    dialogue_root: std::path::PathBuf,
) -> Result<(), String> {
    let prewrite_user_prompts = prompts.len() > 1;
    // 仅在初始化 localfile 根目录时持锁，流式对话期间释放
    // 确保多个 session 可以并发执行互不阻塞
    {
        let _lock = session::LOCALFILE_STORAGE_LOCK
            .lock()
            .map_err(|_| "无法锁定对话存储。".to_string())?;

        std::fs::create_dir_all(&dialogue_root).map_err(|error| error.to_string())?;
        Otherone::set_localfile_root(&dialogue_root);
        otherone::memory::set_memory_storage_root(&dialogue_root);
        if prewrite_user_prompts {
            write_user_prompt_entries_blocking(&session_id, &prompts)?;
        }
    }

    eprintln!("[chat_stream] session={} 开始流式对话", session_id);

    tauri::async_runtime::block_on(async {
        for prompt in &prompts {
            emit_event(
                &app,
                ChatStreamEvent::plain(
                    session_id.clone(),
                    "user_entry",
                    prompt.clone(),
                    None,
                    None,
                ),
            );
        }

        let input = build_input_options(
            &session_id,
            &model,
            &settings.engine,
            request.context_compression_enabled,
            request.memory_enabled,
        );
        let prompt_for_agent = if prewrite_user_prompts {
            None
        } else {
            prompts.first().map(String::as_str)
        };
        let ai = build_ai_options(
            &app,
            &session_id,
            &provider,
            &model,
            &settings.engine,
            prompt_for_agent,
            request.reasoning_effort,
            AgentToolScope::FullDesktop,
        )?;
        let handle = Otherone::invoke_agent_stream_interactive(input, ai, None)
            .await
            .map_err(|error| error.to_string())?;
        let mut stream = handle.events;

        {
            let mut commands = ACTIVE_CHAT_COMMANDS
                .lock()
                .map_err(|_| "无法获取运行中对话队列锁。".to_string())?;
            commands.insert(session_id.clone(), handle.commands);
        }

        eprintln!(
            "[chat_stream] session={} Agent stream 已创建,开始接收事件",
            session_id
        );

        // 注册取消令牌
        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
        {
            let mut tokens = CANCEL_TOKENS
                .lock()
                .map_err(|_| "无法获取取消令牌锁。".to_string())?;
            tokens.insert(session_id.clone(), cancel_tx);
        }

        let mut pending_raw_thinking: Option<String> = None;
        let mut event_count = 0u64;
        let mut cancelled = false;

        loop {
            let event = tokio::select! {
                event = stream.recv() => event,
                _ = &mut cancel_rx => {
                    cancelled = true;
                    None // 取消信号到达，break
                }
            };

            match event {
                Some(event) => {
                    event_count += 1;
                    process_stream_event(&app, &session_id, event, &mut pending_raw_thinking);
                }
                None => break,
            }
        }

        // 清理取消令牌
        {
            let _ = CANCEL_TOKENS
                .lock()
                .map(|mut tokens| tokens.remove(&session_id));
        }

        {
            let _ = ACTIVE_CHAT_COMMANDS
                .lock()
                .map(|mut commands| commands.remove(&session_id));
        }

        if cancelled {
            eprintln!(
                "[chat_stream] session={} 已被用户取消,共收到 {} 个事件",
                session_id, event_count
            );
            emit_event(
                &app,
                ChatStreamEvent::plain(session_id.clone(), "cancelled", String::new(), None, None),
            );
        } else {
            eprintln!(
                "[chat_stream] session={} 流结束,共收到 {} 个事件",
                session_id, event_count
            );
        }

        Ok(())
    })
}

/// 处理单个流式事件，提取为 extract function 以支持 tokio::select! 分支
pub fn start_channel_agent_run(
    app: AppHandle,
    session_id: String,
    prompts: Vec<String>,
    source_name: &str,
) -> Result<ChannelAgentRun, String> {
    let prompts = prompts
        .into_iter()
        .map(|prompt| prompt.trim().to_string())
        .filter(|prompt| !prompt.is_empty())
        .collect::<Vec<_>>();
    if prompts.is_empty() {
        return Err("外部消息不能为空。".to_string());
    }

    let providers = storage::load_api_configs(app.clone())?;
    let (provider, model) = select_default_agent_model(&providers)?;
    let mut settings = app_settings::load_settings(&app)?;
    let dialogue_root = session::agent_storage_root(&app)?;
    let prewrite_user_prompts = prompts.len() > 1;
    let channel_system_prompt = format!(
        "The current user is talking through {source_name}. Return only the plain text that can be sent back to that external user. Do not include internal tool logs, hidden reasoning, channel metadata, or desktop-only instructions in the reply."
    );
    settings.engine.system_prompt = if settings.engine.system_prompt.trim().is_empty() {
        channel_system_prompt
    } else {
        format!(
            "{}\n\n{}",
            settings.engine.system_prompt.trim(),
            channel_system_prompt
        )
    };

    {
        let _lock = session::LOCALFILE_STORAGE_LOCK
            .lock()
            .map_err(|_| "无法锁定对话存储。".to_string())?;

        std::fs::create_dir_all(&dialogue_root).map_err(|error| error.to_string())?;
        Otherone::set_localfile_root(&dialogue_root);
        if prewrite_user_prompts {
            write_user_prompt_entries_blocking(&session_id, &prompts)?;
        }
    }

    let reasoning_effort = Some(settings.engine.default_reasoning_effort.clone());

    let (commands, stream) = tauri::async_runtime::block_on(async {
        let input = build_input_options(&session_id, &model, &settings.engine, true, false);
        let prompt_for_agent = if prewrite_user_prompts {
            None
        } else {
            prompts.first().map(String::as_str)
        };
        let ai = build_ai_options(
            &app,
            &session_id,
            &provider,
            &model,
            &settings.engine,
            prompt_for_agent,
            reasoning_effort,
            AgentToolScope::WeixinSafe,
        )?;
        let handle = Otherone::invoke_agent_stream_interactive(input, ai, None)
            .await
            .map_err(|error| error.to_string())?;
        Ok::<_, String>((handle.commands, handle.events))
    })?;

    let (result_tx, result_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = collect_channel_agent_result(stream);
        let _ = result_tx.send(result);
    });

    Ok(ChannelAgentRun {
        commands,
        result: result_rx,
    })
}

fn collect_channel_agent_result(
    mut stream: tokio::sync::mpsc::Receiver<StreamAgentEvent>,
) -> Result<String, String> {
    tauri::async_runtime::block_on(async move {
        let mut response = String::new();
        let mut fallback_complete = String::new();

        while let Some(event) = stream.recv().await {
            if let Some(error) = event
                .error
                .as_ref()
                .filter(|value| !value.trim().is_empty())
            {
                return Err(error.clone());
            }

            if event.event_type == "queued_user_prompts" {
                response.clear();
                fallback_complete.clear();
                continue;
            }

            if event.event_type == "chunk" {
                if let Some(segment) = extract_delta_segment(event.raw_chunk.as_ref()) {
                    if segment.event_type == "assistant_delta" {
                        response.push_str(&segment.content);
                    }
                } else if !event.content.is_empty() {
                    response.push_str(&event.content);
                }
                continue;
            }

            if event.event_type == "complete" {
                fallback_complete = event.content;
                continue;
            }

            if map_event_type(&event.event_type) == "assistant_delta" && !event.content.is_empty() {
                response.push_str(&event.content);
            }
        }

        let answer = if response.trim().is_empty() {
            fallback_complete.trim().to_string()
        } else {
            response.trim().to_string()
        };

        if answer.is_empty() {
            Err("Agent 没有返回可发送内容。".to_string())
        } else {
            Ok(answer)
        }
    })
}

fn process_stream_event(
    app: &AppHandle,
    session_id: &str,
    event: StreamAgentEvent,
    pending_raw_thinking: &mut Option<String>,
) {
    if event.event_type == "chunk" {
        if let Some(segment) = extract_delta_segment(event.raw_chunk.as_ref()) {
            if segment.event_type == "assistant_thinking_delta" {
                *pending_raw_thinking = Some(segment.content.clone());
            } else {
                *pending_raw_thinking = None;
            }

            emit_event(
                app,
                ChatStreamEvent::plain(
                    session_id.to_string(),
                    segment.event_type,
                    segment.content,
                    event.raw_chunk,
                    event.error,
                ),
            );
        } else if !event.content.is_empty() {
            *pending_raw_thinking = None;
            emit_event(
                app,
                ChatStreamEvent::plain(
                    session_id.to_string(),
                    "assistant_delta",
                    event.content,
                    event.raw_chunk,
                    event.error,
                ),
            );
        }
        return;
    }

    if event.event_type == "thinking" {
        if pending_raw_thinking.as_deref() == Some(event.content.as_str()) {
            *pending_raw_thinking = None;
            return;
        }

        *pending_raw_thinking = None;
        emit_event(
            app,
            ChatStreamEvent::plain(
                session_id.to_string(),
                "assistant_thinking_delta",
                event.content,
                event.raw_chunk,
                event.error,
            ),
        );
        return;
    }

    if event.event_type == "tool_calls" {
        *pending_raw_thinking = None;
        eprintln!(
            "[chat_stream] session={} tool_calls raw: {}",
            session_id, event.content
        );
        emit_tool_call_events(app, session_id, &event.content);
        return;
    }

    *pending_raw_thinking = None;
    emit_event(
        app,
        ChatStreamEvent::plain(
            session_id.to_string(),
            map_event_type(&event.event_type),
            event.content.clone(),
            event.raw_chunk,
            event.error,
        ),
    );
}

/// 解析框架的 tool_calls 事件内容并发出结构化的 tool_call 事件
/// 输入格式: [tool_calls:fn1({"k":"v"}), fn2({...})]
fn emit_tool_call_events(app: &AppHandle, session_id: &str, content: &str) {
    // 去掉 [tool_calls: 前缀和结尾 ]
    let inner = content
        .strip_prefix("[tool_calls:")
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(content);

    let calls = parse_tool_call_entries(inner);
    for (name, args_json) in &calls {
        let args: Value = match serde_json::from_str(args_json) {
            Ok(v) => v,
            Err(_) => Value::Null,
        };
        let label = tools::tool_label(name, &args);
        let expandable = tools::tool_expandable(name);
        let detail = if expandable {
            // 详情在工具执行结果中填充，这里先发调用标签
            None
        } else {
            None
        };

        eprintln!(
            "[chat_stream] session={} tool_call: name={} label={} expandable={}",
            session_id, name, label, expandable
        );

        emit_event(
            app,
            ChatStreamEvent::tool(session_id.to_string(), label, expandable, detail, "running"),
        );
    }
}

/// 解析 tool_calls 列表字符串为 (name, args_json) 数组
/// 输入: read_file({"file_path":"C:/test.txt"}), glob_search({"pattern":"*.rs"})
fn parse_tool_call_entries(input: &str) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // 跳过空白和逗号
        while i < chars.len() && (chars[i] == ' ' || chars[i] == ',') {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }

        // 读取函数名（直到 '(' ）
        let name_start = i;
        while i < chars.len() && chars[i] != '(' {
            i += 1;
        }
        let name: String = chars[name_start..i].iter().collect();
        if i >= chars.len() {
            break;
        }

        // 跳过 '('
        i += 1;

        // 读取参数 JSON（匹配花括号/方括号深度）
        let args_start = i;
        let mut depth = 0i32;
        let mut in_string = false;
        let mut escaped = false;

        while i < chars.len() {
            let ch = chars[i];
            if escaped {
                escaped = false;
                i += 1;
                continue;
            }
            if ch == '\\' && in_string {
                escaped = true;
                i += 1;
                continue;
            }
            if ch == '"' {
                in_string = !in_string;
                i += 1;
                continue;
            }
            if in_string {
                i += 1;
                continue;
            }
            if ch == '{' || ch == '[' {
                depth += 1;
            } else if ch == '}' || ch == ']' {
                depth -= 1;
                if depth <= 0 {
                    i += 1;
                    break;
                }
            }
            i += 1;
        }

        let args_json: String = chars[args_start..i].iter().collect();
        // 去掉结尾的 ')'（如果有，因为深度匹配可能停在 } 后）
        let args_json = args_json.trim_end_matches(')').to_string();

        if !name.is_empty() {
            entries.push((name, args_json));
        }
    }

    entries
}

fn build_input_options(
    session_id: &str,
    model: &ModelConfig,
    engine: &app_settings::EngineSettings,
    context_compression_enabled: bool,
    enable_long_term_memory: bool,
) -> InputOptions {
    let context_window = if model.context_window > 0 {
        model.context_window as u32
    } else {
        engine.context_window
    };

    InputOptions {
        session_id: session_id.to_string(),
        context_load_type: ContextLoadType::LocalFile,
        storage_type: Some(StorageType::LocalFile),
        database_config: None,
        context_window,
        threshold_percentage: Some(if context_compression_enabled {
            engine.threshold_percentage
        } else {
            1.1
        }),
        max_iterations: Some(engine.max_iterations),
        enable_long_term_memory: Some(enable_long_term_memory),
        long_term_memory_recall_max_types: enable_long_term_memory
            .then_some(DESKTOP_LONG_TERM_MEMORY_RECALL_MAX_TYPES),
    }
}

fn default_memory_enabled() -> bool {
    true
}

fn resolve_user_root() -> String {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .ok()
            .unwrap_or_else(|| String::from("C:\\"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME")
            .ok()
            .unwrap_or_else(|| String::from("/"))
    }
}

fn resolve_system_info() -> String {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let family = std::env::consts::FAMILY;
    let os_pretty = match (os, family) {
        ("windows", _) => {
            // Try to get Windows version from cmd
            if let Ok(output) = std::process::Command::new("cmd")
                .args(["/C", "ver"])
                .output()
            {
                let ver_str = String::from_utf8_lossy(&output.stdout);
                let ver_str = ver_str.trim().to_string();
                if !ver_str.is_empty() {
                    return ver_str;
                }
            }
            "Windows".to_string()
        }
        ("macos", _) => "macOS".to_string(),
        ("linux", _) => "Linux".to_string(),
        _ => format!("{} ({})", os, arch),
    };
    format!("{os_pretty} ({arch})")
}

fn resolve_current_time() -> String {
    let now = chrono::Local::now();
    now.format("%Y-%m-%d %H:%M:%S %:z").to_string()
}

/// 替换 prompt 中的 ${VARIABLE} 占位符
fn resolve_prompt(raw: &str) -> String {
    let mut vars: HashMap<&str, String> = HashMap::new();
    vars.insert("USERTOORPATH", resolve_user_root());
    vars.insert("SYSTEM_INFO", resolve_system_info());
    vars.insert("CURRENT_TIME", resolve_current_time());
    vars.insert(
        "AVAILABLE_SKILLS",
        crate::plugins::format_skills_for_prompt(),
    );

    let mut result = raw.to_owned();
    for (key, value) in &vars {
        let placeholder = format!("${{{}}}", key);
        result = result.replace(&placeholder, value);
    }
    result
}

fn build_ai_options(
    app: &AppHandle,
    session_id: &str,
    provider: &ProviderConfig,
    model: &ModelConfig,
    engine: &app_settings::EngineSettings,
    prompt: Option<&str>,
    reasoning_effort: Option<String>,
    tool_scope: AgentToolScope,
) -> Result<AiOptions, String> {
    let api_key = require_text(&provider.api_key, "API Key")?.to_string();
    let base_url = require_text(&provider.base_url, "Base URL")?.to_string();
    let provider_type = parse_provider(&provider.provider)?;
    // In otherone 0.1.x, AiOptions.context_length is serialized as `contextLength`
    // and then mapped to OpenAI `max_tokens`. The app's model.context_length means
    // model capacity, so using it here can create invalid requests like max_tokens=128000.
    let context_length = None;
    // 构建 system prompt：base（内置不可改，${} 变量由 resolve_prompt 替换）+ engine（用户可配）
    let base = resolve_prompt(BASE_SYSTEM_PROMPT);
    let system_prompt = if engine.system_prompt.trim().is_empty() {
        Some(base)
    } else {
        Some(format!("{}\n\n{}", base, engine.system_prompt.trim()))
    };

    let (tools_list, tools_map) = match tool_scope {
        AgentToolScope::FullDesktop => {
            tools::build_tools_for_session(app.clone(), session_id.to_string())
        }
        AgentToolScope::WeixinSafe => tools::build_weixin_safe_tools(),
    };

    // 对 user prompt 也做变量替换
    let resolved_user_prompt = prompt.map(resolve_prompt);

    Ok(AiOptions {
        provider: provider_type,
        api_key,
        base_url,
        model: require_text(&model.name, "模型名称")?.to_string(),
        user_prompt: resolved_user_prompt,
        system_prompt,
        messages: None,
        context_length,
        temperature: Some(model.temperature as f32),
        top_p: Some(model.top_p as f32),
        tools: Some(tools_list),
        tools_realize: Some(tools_map),
        tool_choice: parse_tool_choice(&model.tool_choice),
        parallel_tool_calls: Some(model.parallel_tool_calls),
        stream: Some(true),
        other: build_other_params(&model.extra_params, reasoning_effort)?,
    })
}

fn build_other_params(
    extra_params: &str,
    reasoning_effort: Option<String>,
) -> Result<Option<Value>, String> {
    let mut object = if extra_params.trim().is_empty() {
        Map::new()
    } else {
        match serde_json::from_str::<Value>(extra_params).map_err(|error| error.to_string())? {
            Value::Object(object) => object,
            _ => return Err("模型额外参数必须是 JSON 对象。".to_string()),
        }
    };

    match reasoning_effort
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some("none") => {}
        Some("low" | "medium" | "high") => {
            let value = reasoning_effort.unwrap();
            object.insert("reasoning_effort".to_string(), Value::String(value.clone()));
            object.insert("reasoningEffort".to_string(), Value::String(value));
        }
        Some(_) | None => {}
    }

    if object.is_empty() {
        Ok(None)
    } else {
        Ok(Some(Value::Object(object)))
    }
}

fn find_model(
    providers: &[ProviderConfig],
    model_id: &str,
) -> Result<(ProviderConfig, ModelConfig), String> {
    for provider in providers {
        if let Some(model) = provider.models.iter().find(|model| model.id == model_id) {
            return Ok((provider.clone(), model.clone()));
        }
    }

    Err("没有找到选择的模型配置。".to_string())
}

fn select_default_agent_model(
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

    first_available.ok_or_else(|| "请先配置可用模型。".to_string())
}

fn parse_provider(provider: &str) -> Result<ProviderType, String> {
    match provider {
        "OpenAI" | "OpenAI Compatible" => Ok(ProviderType::OpenAI),
        "Anthropic" => Ok(ProviderType::Anthropic),
        "OpenRouter" => Ok(ProviderType::OpenRouter),
        "Fetch" => Ok(ProviderType::Fetch),
        "Local" => Ok(ProviderType::Local),
        _ => Err("暂不支持这个 API 供应商类型。".to_string()),
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

fn extract_delta_segment(raw_chunk: Option<&Value>) -> Option<DeltaSegment> {
    let chunk = raw_chunk?;
    let choice = chunk.get("choices")?.get(0)?;
    let delta = choice.get("delta");

    for key in [
        "reasoning_content",
        "reasoningContent",
        "reasoning",
        "thinking",
        "thought",
    ] {
        if let Some(text) = delta
            .and_then(|value| value.get(key))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        {
            return Some(DeltaSegment {
                event_type: "assistant_thinking_delta",
                content: text.to_string(),
            });
        }
    }

    for key in ["content", "text"] {
        if let Some(text) = delta
            .and_then(|value| value.get(key))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        {
            return Some(DeltaSegment {
                event_type: "assistant_delta",
                content: text.to_string(),
            });
        }
    }

    choice
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(|content| DeltaSegment {
            event_type: "assistant_delta",
            content: content.to_string(),
        })
}

fn map_event_type(event_type: &str) -> &str {
    match event_type {
        "chunk" => "assistant_delta",
        "thinking" => "assistant_thinking_delta",
        other => other,
    }
}

fn require_text<'a>(value: &'a str, label: &str) -> Result<&'a str, String> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        Err(format!("{label} 不能为空。"))
    } else {
        Ok(trimmed)
    }
}

fn generate_session_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();

    format!("session-{timestamp}")
}

fn emit_event(app: &AppHandle, event: ChatStreamEvent) {
    // 使用 emit() 发送全局事件,而不是 emit_to("main")
    // 因为前端使用的是 import('@tauri-apps/api/event').listen() 全局监听
    // 如果使用 emit_to("main") 发送的是窗口专属事件,前端的全局 listen() 接收不到
    eprintln!(
        "[chat_stream] emit: session={} type={} content_len={}",
        event.session_id,
        event.event_type,
        event.content.len()
    );
    let _ = app.emit("chat_stream_event", event);
}
