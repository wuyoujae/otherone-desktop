use crate::tools;
use otherone::agent::types::{AiOptions, ContextLoadType, InputOptions, StorageType};
use otherone::ai::types::{ProviderType, ToolChoice};
use otherone::Otherone;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use otherone::agent::StreamAgentEvent;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

/// 取消令牌映射：session_id → oneshot Sender
/// 前端调用 cancel_chat_message 时触发对应 session 的取消信号
static CANCEL_TOKENS: LazyLock<Mutex<HashMap<String, tokio::sync::oneshot::Sender<()>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

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

Be concise. Be direct. Be helpful. Speak the user's language."#;

use crate::app_settings;
use crate::session;
use crate::storage::{self, ModelConfig, ProviderConfig};

static ACTIVE_CHATS: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendChatMessageRequest {
    pub session_id: Option<String>,
    pub model_id: String,
    pub prompt: String,
    pub reasoning_effort: Option<String>,
    pub context_compression_enabled: bool,
    pub branch_mode_enabled: bool,
    pub target_mode_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendChatMessageResponse {
    pub session_id: String,
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
    /// 工具执行状态: "completed" | "error"
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

    Ok(())
}

#[tauri::command]
pub fn send_chat_message(
    app: AppHandle,
    request: SendChatMessageRequest,
) -> Result<SendChatMessageResponse, String> {
    let prompt = request.prompt.trim().to_string();
    if prompt.is_empty() {
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
            prompt,
            provider,
            model,
            settings,
            dialogue_root,
        );

        if let Err(error) = result {
            emit_event(
                &app,
                ChatStreamEvent::plain(
                    event_session_id,
                    "error",
                    String::new(),
                    None,
                    Some(error),
                ),
            );
        }
    });

    Ok(SendChatMessageResponse { session_id })
}

fn run_chat_stream(
    app: AppHandle,
    request: SendChatMessageRequest,
    session_id: String,
    prompt: String,
    provider: ProviderConfig,
    model: ModelConfig,
    settings: app_settings::AppSettings,
    dialogue_root: std::path::PathBuf,
) -> Result<(), String> {
    // 仅在初始化 localfile 根目录时持锁，流式对话期间释放
    // 确保多个 session 可以并发执行互不阻塞
    {
        let _lock = session::LOCALFILE_STORAGE_LOCK
            .lock()
            .map_err(|_| "无法锁定对话存储。".to_string())?;

        std::fs::create_dir_all(&dialogue_root).map_err(|error| error.to_string())?;
        Otherone::set_localfile_root(&dialogue_root);
    }

    eprintln!("[chat_stream] session={} 开始流式对话", session_id);

    tauri::async_runtime::block_on(async {
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

        let input = build_input_options(
            &session_id,
            &model,
            &settings.engine,
            request.context_compression_enabled,
        );
        let ai = build_ai_options(
            &provider,
            &model,
            &settings.engine,
            &prompt,
            request.reasoning_effort,
        )?;
        let mut stream = Otherone::invoke_agent_stream(input, ai)
            .await
            .map_err(|error| error.to_string())?;

        eprintln!("[chat_stream] session={} Agent stream 已创建,开始接收事件", session_id);

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
                    process_stream_event(
                        &app,
                        &session_id,
                        event,
                        &mut pending_raw_thinking,
                    );
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

        if cancelled {
            eprintln!(
                "[chat_stream] session={} 已被用户取消,共收到 {} 个事件",
                session_id, event_count
            );
            emit_event(
                &app,
                ChatStreamEvent::plain(
                    session_id.clone(),
                    "cancelled",
                    String::new(),
                    None,
                    None,
                ),
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
            ChatStreamEvent::tool(
                session_id.to_string(),
                label,
                expandable,
                detail,
                "completed",
            ),
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
    }
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

    let mut result = raw.to_owned();
    for (key, value) in &vars {
        let placeholder = format!("${{{}}}", key);
        result = result.replace(&placeholder, value);
    }
    result
}

fn build_ai_options(
    provider: &ProviderConfig,
    model: &ModelConfig,
    engine: &app_settings::EngineSettings,
    prompt: &str,
    reasoning_effort: Option<String>,
) -> Result<AiOptions, String> {
    let api_key = require_text(&provider.api_key, "API Key")?.to_string();
    let base_url = require_text(&provider.base_url, "Base URL")?.to_string();
    let provider_type = parse_provider(&provider.provider)?;
    // In otherone 0.1.x, AiOptions.context_length is serialized as `contextLength`
    // and then mapped to OpenAI `max_tokens`. The app's model.context_length means
    // model capacity, so using it here can create invalid requests like max_tokens=128000.
    let context_length = None;
    // 构建 system prompt：base（内置不可改）+ skills 列表 + engine（用户可配）
    let base = resolve_prompt(BASE_SYSTEM_PROMPT);
    let skills_xml = crate::plugins::format_skills_for_prompt();
    let system_prompt = if engine.system_prompt.trim().is_empty() {
        if skills_xml.is_empty() { Some(base) } else { Some(format!("{}{}", base, skills_xml)) }
    } else {
        if skills_xml.is_empty() {
            Some(format!("{}\n\n{}", base, engine.system_prompt.trim()))
        } else {
            Some(format!("{}{}\n\n{}", base, skills_xml, engine.system_prompt.trim()))
        }
    };

    let (tools_list, tools_map) = tools::build_tools();

    // 对 user prompt 也做变量替换
    let resolved_user_prompt = resolve_prompt(prompt);

    Ok(AiOptions {
        provider: provider_type,
        api_key,
        base_url,
        model: require_text(&model.name, "模型名称")?.to_string(),
        user_prompt: Some(resolved_user_prompt),
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
