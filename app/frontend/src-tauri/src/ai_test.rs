use std::time::{Duration, Instant};

use futures_util::StreamExt;
use otherone::ai::types::{Message, MessageContent, ProviderType};
use serde::{Deserialize, Serialize};
use tokio::time::timeout;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestAiModelRequest {
    provider: String,
    base_url: String,
    api_key: String,
    model: String,
    context_length: Option<u32>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    parallel_tool_calls: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestAiModelResponse {
    latency_ms: u128,
}

#[tauri::command]
pub async fn test_ai_model(request: TestAiModelRequest) -> Result<TestAiModelResponse, String> {
    let provider = parse_provider(&request.provider)?;
    let api_key = require_text(&request.api_key, "API Key")?;
    let base_url = require_text(&request.base_url, "Base URL")?;
    let model = require_text(&request.model, "模型名称")?;

    let output_token_limit = request.context_length.map(|value| value.min(4096));
    let config = serde_json::json!({
        "model": model,
        "messages": [Message {
            role: "user".to_string(),
            content: MessageContent::Text("请只回复 ok。".to_string()),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }],
        "contextLength": output_token_limit,
        "temperature": request.temperature,
        "topP": request.top_p,
        "parallelToolCalls": request.parallel_tool_calls,
        "stream": true,
    });

    let started_at = Instant::now();
    let mut stream = otherone::ai::invoke_model_stream(provider, api_key, base_url, config)
        .await
        .map_err(to_user_error)?;

    let first_chunk = timeout(Duration::from_secs(30), stream.next())
        .await
        .map_err(|_| "模型测试超时：30 秒内没有收到首个流式响应。".to_string())?;

    match first_chunk {
        Some(Ok(_)) => Ok(TestAiModelResponse {
            latency_ms: started_at.elapsed().as_millis(),
        }),
        Some(Err(error)) => Err(to_user_error(error)),
        None => Err("模型测试失败：服务端没有返回流式内容。".to_string()),
    }
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

fn require_text<'a>(value: &'a str, label: &str) -> Result<&'a str, String> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        Err(format!("{label} 不能为空。"))
    } else {
        Ok(trimmed)
    }
}

fn to_user_error(error: impl std::fmt::Display) -> String {
    format!("模型测试失败：{error}")
}
