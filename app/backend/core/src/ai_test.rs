use std::time::{Duration, Instant};

use futures_util::StreamExt;
use otherone::ai::types::{Message, MessageContent, ProviderType};
use serde::{Deserialize, Serialize};
use tokio::time::timeout;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestAiModelRequest {
    pub provider: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub context_length: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub parallel_tool_calls: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestAiModelResponse {
    pub latency_ms: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestAiModelErrorKind {
    Validation,
    Timeout,
    Upstream,
}

#[derive(Debug, Clone)]
pub struct TestAiModelError {
    kind: TestAiModelErrorKind,
    message: String,
}

impl TestAiModelError {
    pub fn kind(&self) -> TestAiModelErrorKind {
        self.kind
    }

    fn validation(message: String) -> Self {
        Self {
            kind: TestAiModelErrorKind::Validation,
            message,
        }
    }

    fn timeout(message: String) -> Self {
        Self {
            kind: TestAiModelErrorKind::Timeout,
            message,
        }
    }

    fn upstream(message: String) -> Self {
        Self {
            kind: TestAiModelErrorKind::Upstream,
            message,
        }
    }
}

impl std::fmt::Display for TestAiModelError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for TestAiModelError {}

pub async fn test_ai_model(
    request: TestAiModelRequest,
) -> Result<TestAiModelResponse, TestAiModelError> {
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
        .map_err(|_| {
            TestAiModelError::timeout("模型测试超时：30 秒内没有收到首个流式响应。".to_string())
        })?;

    match first_chunk {
        Some(Ok(_)) => Ok(TestAiModelResponse {
            latency_ms: started_at.elapsed().as_millis(),
        }),
        Some(Err(error)) => Err(to_user_error(error)),
        None => Err(TestAiModelError::upstream(
            "模型测试失败：服务端没有返回流式内容。".to_string(),
        )),
    }
}

fn parse_provider(provider: &str) -> Result<ProviderType, TestAiModelError> {
    match provider {
        "OpenAI" | "OpenAI Compatible" => Ok(ProviderType::OpenAI),
        "Anthropic" => Ok(ProviderType::Anthropic),
        "OpenRouter" => Ok(ProviderType::OpenRouter),
        "Fetch" => Ok(ProviderType::Fetch),
        "Local" => Ok(ProviderType::Local),
        _ => Err(TestAiModelError::validation(
            "暂不支持这个 API 供应商类型。".to_string(),
        )),
    }
}

fn require_text<'a>(value: &'a str, label: &str) -> Result<&'a str, TestAiModelError> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        Err(TestAiModelError::validation(format!("{label} 不能为空。")))
    } else {
        Ok(trimmed)
    }
}

fn to_user_error(error: impl std::fmt::Display) -> TestAiModelError {
    TestAiModelError::upstream(format!("模型测试失败：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_provider() {
        let error = parse_provider("Unknown").expect_err("rejects provider");
        assert_eq!(error.kind(), TestAiModelErrorKind::Validation);
        assert!(error.to_string().contains("供应商"));
    }

    #[test]
    fn rejects_blank_required_text() {
        let error = require_text(" ", "Base URL").expect_err("rejects empty value");
        assert_eq!(error.kind(), TestAiModelErrorKind::Validation);
        assert!(error.to_string().contains("Base URL"));
    }
}
