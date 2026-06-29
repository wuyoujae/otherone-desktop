use otherone::ai::types::{ProviderType, ToolChoice};
use serde_json::{Map, Value};

use crate::storage::{ModelConfig, ProviderConfig};

pub fn parse_provider(provider: &str) -> Result<ProviderType, String> {
    match provider {
        "OpenAI" | "OpenAI Compatible" => Ok(ProviderType::OpenAI),
        "Anthropic" => Ok(ProviderType::Anthropic),
        "OpenRouter" => Ok(ProviderType::OpenRouter),
        "Fetch" => Ok(ProviderType::Fetch),
        "Local" => Ok(ProviderType::Local),
        _ => Err("暂不支持这个 API 供应商类型。".to_string()),
    }
}

pub fn require_text<'a>(value: &'a str, label: &str) -> Result<&'a str, String> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        Err(format!("{label} cannot be empty."))
    } else {
        Ok(trimmed)
    }
}

pub fn parse_tool_choice(value: &str) -> Option<ToolChoice> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "default" {
        None
    } else {
        Some(ToolChoice::String(trimmed.to_string()))
    }
}

pub fn build_other_params(
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

pub fn merge_object_params(target: &mut Value, params: Option<Value>) {
    let Some(Value::Object(params)) = params else {
        return;
    };
    let Value::Object(target) = target else {
        return;
    };

    for (key, value) in params {
        target.insert(key, value);
    }
}

pub fn find_model(
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

pub fn select_default_model(
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

pub fn select_model_with_fallback(
    providers: &[ProviderConfig],
    model_id: Option<&str>,
) -> Result<(ProviderConfig, ModelConfig), String> {
    if let Some(model_id) = model_id {
        return find_model(providers, model_id);
    }

    select_default_model(providers)
}
