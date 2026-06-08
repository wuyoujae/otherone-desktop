use otherone::agent::types::{AiOptions, ContextLoadType, InputOptions, StorageType};
use otherone::ai::types::{ProviderType, ToolChoice};
use otherone::Otherone;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

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
                ChatStreamEvent {
                    session_id: event_session_id,
                    event_type: "error".to_string(),
                    content: String::new(),
                    raw_chunk: None,
                    error: Some(error),
                },
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
    let _lock = session::LOCALFILE_STORAGE_LOCK
        .lock()
        .map_err(|_| "无法锁定对话存储。".to_string())?;

    std::fs::create_dir_all(&dialogue_root).map_err(|error| error.to_string())?;
    Otherone::set_localfile_root(&dialogue_root);

    tauri::async_runtime::block_on(async {
        emit_event(
            &app,
            ChatStreamEvent {
                session_id: session_id.clone(),
                event_type: "user_entry".to_string(),
                content: prompt.clone(),
                raw_chunk: None,
                error: None,
            },
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

        let mut pending_raw_thinking: Option<String> = None;

        while let Some(event) = stream.recv().await {
            if event.event_type == "chunk" {
                if let Some(segment) = extract_delta_segment(event.raw_chunk.as_ref()) {
                    if segment.event_type == "assistant_thinking_delta" {
                        pending_raw_thinking = Some(segment.content.clone());
                    } else {
                        pending_raw_thinking = None;
                    }

                    emit_event(
                        &app,
                        ChatStreamEvent {
                            session_id: session_id.clone(),
                            event_type: segment.event_type.to_string(),
                            content: segment.content,
                            raw_chunk: event.raw_chunk,
                            error: event.error,
                        },
                    );
                } else if !event.content.is_empty() {
                    pending_raw_thinking = None;
                    emit_event(
                        &app,
                        ChatStreamEvent {
                            session_id: session_id.clone(),
                            event_type: "assistant_delta".to_string(),
                            content: event.content,
                            raw_chunk: event.raw_chunk,
                            error: event.error,
                        },
                    );
                }
                continue;
            }

            if event.event_type == "thinking" {
                if pending_raw_thinking.as_deref() == Some(event.content.as_str()) {
                    pending_raw_thinking = None;
                    continue;
                }

                pending_raw_thinking = None;
                emit_event(
                    &app,
                    ChatStreamEvent {
                        session_id: session_id.clone(),
                        event_type: "assistant_thinking_delta".to_string(),
                        content: event.content,
                        raw_chunk: event.raw_chunk,
                        error: event.error,
                    },
                );
                continue;
            }

            pending_raw_thinking = None;
            emit_event(
                &app,
                ChatStreamEvent {
                    session_id: session_id.clone(),
                    event_type: map_event_type(&event.event_type).to_string(),
                    content: event.content.clone(),
                    raw_chunk: event.raw_chunk,
                    error: event.error,
                },
            );
        }

        Ok::<(), String>(())
    })
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
    let system_prompt = engine
        .system_prompt
        .trim()
        .is_empty()
        .then_some(None)
        .unwrap_or_else(|| Some(engine.system_prompt.clone()));

    Ok(AiOptions {
        provider: provider_type,
        api_key,
        base_url,
        model: require_text(&model.name, "模型名称")?.to_string(),
        user_prompt: Some(prompt.to_string()),
        system_prompt,
        messages: None,
        context_length,
        temperature: Some(model.temperature as f32),
        top_p: Some(model.top_p as f32),
        tools: None,
        tools_realize: None,
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
    if app.emit_to("main", "chat_stream_event", &event).is_err() {
        let _ = app.emit("chat_stream_event", event);
    }
}
