use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use otherone::agent::types::{AiOptions, ContextLoadType, InputOptions, StorageType};
use otherone::agent::{AgentStreamCommand, StreamAgentEvent};
use otherone::ai::types::{ProviderType, ToolChoice};
use otherone::storage::types::{StorageType as EntryStorageType, WriteEntryOptions};
use otherone::Otherone;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::artifacts::FileArtifact;
use crate::session::LOCALFILE_STORAGE_LOCK;
use crate::settings::{AppSettings, EngineSettings};
use crate::storage::{ModelConfig, ProviderConfig};
use crate::tools::{self, ArtifactSink};

const WEB_LONG_TERM_MEMORY_RECALL_MAX_TYPES: usize = 5;

const WEB_BASE_SYSTEM_PROMPT: &str = r#"You are otherone-agent, running in the OtherOne web backend.

You help the user with conversation, reasoning, planning, writing, and analysis.
You are running on ${SYSTEM_INFO}. The current time is ${CURRENT_TIME}.

When server-side tools are available, use them only against the configured server workspace and backend data root.
Do not claim access to the browser user's local filesystem.

Be concise. Be direct. Speak the user's language."#;

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatStreamEvent {
    pub session_id: String,
    pub event_type: String,
    pub content: String,
    pub raw_chunk: Option<Value>,
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_expandable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_detail: Option<String>,
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
        Self {
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
        Self {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatErrorKind {
    Validation,
    Conflict,
    Upstream,
    Internal,
}

#[derive(Debug, Clone)]
pub struct ChatError {
    kind: ChatErrorKind,
    message: String,
}

impl ChatError {
    pub fn kind(&self) -> ChatErrorKind {
        self.kind
    }

    fn validation(message: impl Into<String>) -> Self {
        Self {
            kind: ChatErrorKind::Validation,
            message: message.into(),
        }
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self {
            kind: ChatErrorKind::Conflict,
            message: message.into(),
        }
    }

    fn upstream(message: impl Into<String>) -> Self {
        Self {
            kind: ChatErrorKind::Upstream,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            kind: ChatErrorKind::Internal,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ChatError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ChatError {}

struct DeltaSegment {
    event_type: &'static str,
    content: String,
}

#[derive(Clone)]
pub struct ChatRuntime {
    event_sink: Arc<dyn Fn(ChatStreamEvent) + Send + Sync>,
    artifact_sink: Option<ArtifactSink>,
    cancel_tokens: Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<()>>>>,
    active_commands: Arc<Mutex<HashMap<String, tokio::sync::mpsc::Sender<AgentStreamCommand>>>>,
}

impl ChatRuntime {
    pub fn new(event_sink: impl Fn(ChatStreamEvent) + Send + Sync + 'static) -> Self {
        Self {
            event_sink: Arc::new(event_sink),
            artifact_sink: None,
            cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
            active_commands: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_artifact_sink(
        event_sink: impl Fn(ChatStreamEvent) + Send + Sync + 'static,
        artifact_sink: impl Fn(FileArtifact) + Send + Sync + 'static,
    ) -> Self {
        Self {
            event_sink: Arc::new(event_sink),
            artifact_sink: Some(Arc::new(artifact_sink)),
            cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
            active_commands: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn send_chat_message(
        &self,
        request: SendChatMessageRequest,
        settings: AppSettings,
        providers: Vec<ProviderConfig>,
        data_root: PathBuf,
        dialogue_root: PathBuf,
    ) -> Result<SendChatMessageResponse, ChatError> {
        let prompts = normalized_request_prompts(&request);
        if prompts.is_empty() {
            return Err(ChatError::validation("消息内容不能为空。"));
        }

        let session_id = request
            .session_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .unwrap_or_else(generate_session_id);
        let model_id = require_text(&request.model_id, "模型")?.to_string();
        let (provider, model) = find_model(&providers, &model_id)?;
        let event_session_id = session_id.clone();
        let runtime = self.clone();
        let _reserved_modes = (request.branch_mode_enabled, request.target_mode_enabled);

        tokio::spawn(async move {
            if let Err(error) = runtime
                .run_chat_stream(
                    request,
                    event_session_id.clone(),
                    prompts,
                    provider,
                    model,
                    settings,
                    data_root,
                    dialogue_root,
                )
                .await
            {
                runtime.emit(ChatStreamEvent::plain(
                    event_session_id,
                    "error",
                    String::new(),
                    None,
                    Some(error.to_string()),
                ));
            }
        });

        Ok(SendChatMessageResponse { session_id })
    }

    pub async fn enqueue_chat_message(
        &self,
        request: EnqueueChatMessageRequest,
    ) -> Result<(), ChatError> {
        let session_id = require_text(&request.session_id, "session_id")?.to_string();
        let prompts = normalized_enqueue_prompts(&request);
        if prompts.is_empty() {
            return Err(ChatError::validation("消息内容不能为空。"));
        }

        let sender = self
            .active_commands
            .lock()
            .map_err(|_| ChatError::internal("无法获取运行中对话队列锁。"))?
            .get(&session_id)
            .cloned()
            .ok_or_else(|| ChatError::conflict("当前会话没有正在运行的 Agent。"))?;

        sender
            .send(AgentStreamCommand::EnqueueUserPrompts(prompts))
            .await
            .map_err(|_| ChatError::conflict("运行中对话队列已关闭。"))
    }

    pub fn cancel_chat_message(&self, session_id: String) -> Result<(), ChatError> {
        let sid = require_text(&session_id, "session_id")?.to_string();
        let sender = self
            .cancel_tokens
            .lock()
            .map_err(|_| ChatError::internal("无法获取取消令牌锁。"))?
            .remove(&sid);

        if let Some(sender) = sender {
            let _ = sender.send(());
        }

        let _ = self
            .active_commands
            .lock()
            .map(|mut commands| commands.remove(&sid));

        Ok(())
    }

    async fn run_chat_stream(
        &self,
        request: SendChatMessageRequest,
        session_id: String,
        prompts: Vec<String>,
        provider: ProviderConfig,
        model: ModelConfig,
        settings: AppSettings,
        data_root: PathBuf,
        dialogue_root: PathBuf,
    ) -> Result<(), ChatError> {
        let prewrite_user_prompts = prompts.len() > 1;
        {
            let _lock = LOCALFILE_STORAGE_LOCK
                .lock()
                .map_err(|_| ChatError::internal("无法锁定对话存储。"))?;

            std::fs::create_dir_all(&dialogue_root)
                .map_err(|error| ChatError::internal(format!("创建对话目录失败：{error}")))?;
            Otherone::set_localfile_root(&dialogue_root);
            otherone::memory::set_memory_storage_root(&dialogue_root);
        }

        if prewrite_user_prompts {
            write_user_prompt_entries(&session_id, &prompts).await?;
        }

        for prompt in &prompts {
            self.emit(ChatStreamEvent::plain(
                session_id.clone(),
                "user_entry",
                prompt.clone(),
                None,
                None,
            ));
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
            &data_root,
            &session_id,
            &provider,
            &model,
            &settings.engine,
            prompt_for_agent,
            request.reasoning_effort,
            self.artifact_sink.clone(),
        )?;
        let handle = Otherone::invoke_agent_stream_interactive(input, ai, None)
            .await
            .map_err(|error| ChatError::upstream(error.to_string()))?;
        let mut stream = handle.events;

        {
            let mut commands = self
                .active_commands
                .lock()
                .map_err(|_| ChatError::internal("无法获取运行中对话队列锁。"))?;
            commands.insert(session_id.clone(), handle.commands);
        }

        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
        {
            let mut tokens = self
                .cancel_tokens
                .lock()
                .map_err(|_| ChatError::internal("无法获取取消令牌锁。"))?;
            tokens.insert(session_id.clone(), cancel_tx);
        }

        let mut pending_raw_thinking: Option<String> = None;
        let mut cancelled = false;

        loop {
            let event = tokio::select! {
                event = stream.recv() => event,
                _ = &mut cancel_rx => {
                    cancelled = true;
                    None
                }
            };

            match event {
                Some(event) => {
                    self.process_stream_event(&session_id, event, &mut pending_raw_thinking);
                }
                None => break,
            }
        }

        {
            let _ = self
                .cancel_tokens
                .lock()
                .map(|mut tokens| tokens.remove(&session_id));
        }

        {
            let _ = self
                .active_commands
                .lock()
                .map(|mut commands| commands.remove(&session_id));
        }

        if cancelled {
            self.emit(ChatStreamEvent::plain(
                session_id,
                "cancelled",
                String::new(),
                None,
                None,
            ));
        }

        Ok(())
    }

    fn process_stream_event(
        &self,
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

                self.emit(ChatStreamEvent::plain(
                    session_id.to_string(),
                    segment.event_type,
                    segment.content,
                    event.raw_chunk,
                    event.error,
                ));
            } else if !event.content.is_empty() {
                *pending_raw_thinking = None;
                self.emit(ChatStreamEvent::plain(
                    session_id.to_string(),
                    "assistant_delta",
                    event.content,
                    event.raw_chunk,
                    event.error,
                ));
            }
            return;
        }

        if event.event_type == "thinking" {
            if pending_raw_thinking.as_deref() == Some(event.content.as_str()) {
                *pending_raw_thinking = None;
                return;
            }

            *pending_raw_thinking = None;
            self.emit(ChatStreamEvent::plain(
                session_id.to_string(),
                "assistant_thinking_delta",
                event.content,
                event.raw_chunk,
                event.error,
            ));
            return;
        }

        if event.event_type == "tool_calls" || event.event_type == "tool_call" {
            *pending_raw_thinking = None;
            self.emit_tool_call_events(session_id, &event.content);
            return;
        }

        *pending_raw_thinking = None;
        self.emit(ChatStreamEvent::plain(
            session_id.to_string(),
            map_event_type(&event.event_type),
            event.content,
            event.raw_chunk,
            event.error,
        ));
    }

    fn emit(&self, event: ChatStreamEvent) {
        (self.event_sink)(event);
    }

    fn emit_tool_call_events(&self, session_id: &str, content: &str) {
        let inner = content
            .strip_prefix("[tool_calls:")
            .and_then(|value| value.strip_suffix(']'))
            .unwrap_or(content);

        for (name, args_json) in parse_tool_call_entries(inner) {
            let args = serde_json::from_str::<Value>(&args_json).unwrap_or(Value::Null);
            self.emit(ChatStreamEvent::tool(
                session_id.to_string(),
                tools::tool_label(&name, &args),
                tools::tool_expandable(&name),
                None,
                "running",
            ));
        }
    }
}

async fn write_user_prompt_entries(session_id: &str, prompts: &[String]) -> Result<(), ChatError> {
    for prompt in prompts {
        otherone::storage::write_entry(&WriteEntryOptions {
            storage_type: EntryStorageType::LocalFile,
            session_id: session_id.to_string(),
            role: "user".to_string(),
            content: resolve_prompt(prompt),
            tools: None,
            token_consumption: None,
            create_at: None,
            database_config: None,
        })
        .await
        .map_err(|error| ChatError::internal(error.to_string()))?;
    }

    Ok(())
}

fn normalized_request_prompts(request: &SendChatMessageRequest) -> Vec<String> {
    request
        .prompts
        .as_ref()
        .map(|prompts| normalized_prompts(prompts))
        .filter(|prompts| !prompts.is_empty())
        .unwrap_or_else(|| normalized_prompts(std::slice::from_ref(&request.prompt)))
}

fn normalized_enqueue_prompts(request: &EnqueueChatMessageRequest) -> Vec<String> {
    request
        .prompts
        .as_ref()
        .map(|prompts| normalized_prompts(prompts))
        .filter(|prompts| !prompts.is_empty())
        .unwrap_or_else(|| normalized_prompts(std::slice::from_ref(&request.prompt)))
}

fn normalized_prompts(prompts: &[String]) -> Vec<String> {
    prompts
        .iter()
        .map(|prompt| prompt.trim())
        .filter(|prompt| !prompt.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn build_input_options(
    session_id: &str,
    model: &ModelConfig,
    engine: &EngineSettings,
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
            .then_some(WEB_LONG_TERM_MEMORY_RECALL_MAX_TYPES),
    }
}

fn build_ai_options(
    data_root: &Path,
    session_id: &str,
    provider: &ProviderConfig,
    model: &ModelConfig,
    engine: &EngineSettings,
    prompt: Option<&str>,
    reasoning_effort: Option<String>,
    artifact_sink: Option<ArtifactSink>,
) -> Result<AiOptions, ChatError> {
    let api_key = require_text(&provider.api_key, "API Key")?.to_string();
    let base_url = require_text(&provider.base_url, "Base URL")?.to_string();
    let provider_type = parse_provider(&provider.provider)?;
    let mut base = resolve_prompt(WEB_BASE_SYSTEM_PROMPT);
    if let Ok(skills_prompt) = crate::plugins::format_skills_for_prompt(data_root) {
        if !skills_prompt.trim().is_empty() {
            base.push_str("\n\n");
            base.push_str(skills_prompt.trim());
        }
    }
    let system_prompt = if engine.system_prompt.trim().is_empty() {
        Some(base)
    } else {
        Some(format!("{}\n\n{}", base, engine.system_prompt.trim()))
    };
    let resolved_user_prompt = prompt.map(resolve_prompt);
    let (tools_list, tools_map) = tools::build_tools_for_web_session(
        data_root.to_path_buf(),
        session_id.to_string(),
        artifact_sink,
    );

    Ok(AiOptions {
        provider: provider_type,
        api_key,
        base_url,
        model: require_text(&model.name, "模型名称")?.to_string(),
        user_prompt: resolved_user_prompt,
        system_prompt,
        messages: None,
        context_length: None,
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
) -> Result<Option<Value>, ChatError> {
    let mut object = if extra_params.trim().is_empty() {
        Map::new()
    } else {
        match serde_json::from_str::<Value>(extra_params)
            .map_err(|error| ChatError::validation(error.to_string()))?
        {
            Value::Object(object) => object,
            _ => return Err(ChatError::validation("模型额外参数必须是 JSON 对象。")),
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
) -> Result<(ProviderConfig, ModelConfig), ChatError> {
    for provider in providers {
        if let Some(model) = provider.models.iter().find(|model| model.id == model_id) {
            return Ok((provider.clone(), model.clone()));
        }
    }

    Err(ChatError::validation("没有找到选择的模型配置。"))
}

fn parse_provider(provider: &str) -> Result<ProviderType, ChatError> {
    match provider {
        "OpenAI" | "OpenAI Compatible" => Ok(ProviderType::OpenAI),
        "Anthropic" => Ok(ProviderType::Anthropic),
        "OpenRouter" => Ok(ProviderType::OpenRouter),
        "Fetch" => Ok(ProviderType::Fetch),
        "Local" => Ok(ProviderType::Local),
        _ => Err(ChatError::validation("暂不支持这个 API 供应商类型。")),
    }
}

#[allow(dead_code)]
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

fn parse_tool_call_entries(input: &str) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut index = 0;

    while index < chars.len() {
        while index < chars.len() && (chars[index] == ' ' || chars[index] == ',') {
            index += 1;
        }
        if index >= chars.len() {
            break;
        }

        let name_start = index;
        while index < chars.len() && chars[index] != '(' {
            index += 1;
        }
        let name = chars[name_start..index]
            .iter()
            .collect::<String>()
            .trim()
            .to_string();
        if index >= chars.len() {
            break;
        }

        index += 1;
        let args_start = index;
        let mut depth = 0i32;
        let mut in_string = false;
        let mut escaped = false;

        while index < chars.len() {
            let ch = chars[index];
            if escaped {
                escaped = false;
                index += 1;
                continue;
            }
            if ch == '\\' && in_string {
                escaped = true;
                index += 1;
                continue;
            }
            if ch == '"' {
                in_string = !in_string;
                index += 1;
                continue;
            }
            if in_string {
                index += 1;
                continue;
            }
            if ch == '{' || ch == '[' {
                depth += 1;
            } else if ch == '}' || ch == ']' {
                depth -= 1;
                if depth <= 0 {
                    index += 1;
                    break;
                }
            }
            index += 1;
        }

        let args_json = chars[args_start..index]
            .iter()
            .collect::<String>()
            .trim_end_matches(')')
            .to_string();

        if !name.is_empty() {
            entries.push((name, args_json));
        }
    }

    entries
}

fn resolve_prompt(raw: &str) -> String {
    let mut vars: HashMap<&str, String> = HashMap::new();
    vars.insert("SYSTEM_INFO", resolve_system_info());
    vars.insert("CURRENT_TIME", resolve_current_time());

    let mut result = raw.to_owned();
    for (key, value) in &vars {
        let placeholder = format!("${{{}}}", key);
        result = result.replace(&placeholder, value);
    }
    result
}

fn resolve_system_info() -> String {
    format!(
        "{} ({}, {})",
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::env::consts::FAMILY
    )
}

fn resolve_current_time() -> String {
    chrono::Local::now()
        .format("%Y-%m-%d %H:%M:%S %:z")
        .to_string()
}

fn require_text<'a>(value: &'a str, label: &str) -> Result<&'a str, ChatError> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        Err(ChatError::validation(format!("{label} 不能为空。")))
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

fn default_memory_enabled() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_prompt_batches() {
        let request = SendChatMessageRequest {
            session_id: None,
            model_id: "model-1".to_string(),
            prompt: " ignored ".to_string(),
            prompts: Some(vec![" one ".to_string(), "".to_string(), "two".to_string()]),
            reasoning_effort: None,
            context_compression_enabled: true,
            branch_mode_enabled: false,
            target_mode_enabled: false,
            memory_enabled: true,
        };

        assert_eq!(normalized_request_prompts(&request), vec!["one", "two"]);
    }

    #[test]
    fn validates_required_text() {
        let error = require_text(" ", "模型").expect_err("rejects blank");
        assert_eq!(error.kind(), ChatErrorKind::Validation);
        assert!(error.to_string().contains("模型"));
    }
}
