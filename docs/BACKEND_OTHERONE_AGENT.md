# Backend Otherone Agent Notes

## Package

- Use published crate: `otherone = "0.1.2"`.
- Do not use a local path dependency for this app unless explicitly debugging framework changes.
- The local framework source at `C:\Users\jae\Desktop\OmniBuild\otherone\otherone-agent` is for reading and behavior verification.
- Verified through the desktop `cargo check` dependency graph; the facade crate is consumed from crates.io as version `0.1.2`, MIT licensed, with docs at `https://docs.rs/otherone`.

Recommended dependency:

```toml
[dependencies]
otherone = "0.1.2"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
```

## Main API

The app should import from the facade crate:

```rust
use otherone::{
    agent::types::{AiOptions, ContextLoadType, InputOptions},
    ai::types::ProviderType,
    Otherone,
};
```

Core calls:

- `Otherone::invoke_agent(&input, &mut ai)` returns one final `ParsedResponse`.
- `Otherone::invoke_agent_stream(input, ai)` returns a `tokio::sync::mpsc::Receiver<StreamAgentEvent>`.
- `Otherone::invoke_model(provider, api_key, base_url, config)` directly calls a model without the agent loop.
- `Otherone::process_tools(tool_calls, tools_realize)` can execute framework-style tool calls directly.

For this desktop app, `invoke_agent_stream` is the better default because the frontend already has message items for normal text, running tools, completed tools, collapsible tool details, and agent progress.

## Required Options

`InputOptions` controls session and context behavior:

```rust
let input = InputOptions {
    session_id,
    context_load_type: ContextLoadType::LocalFile,
    storage_type: None,
    context_window: 8192,
    threshold_percentage: Some(0.8),
    max_iterations: Some(8),
    database_config: None,
};
```

`AiOptions` controls provider/model/tool behavior:

```rust
let ai = AiOptions {
    provider: ProviderType::OpenAI,
    api_key,
    base_url,
    model,
    user_prompt: Some(user_input),
    system_prompt,
    messages: None,
    context_length: Some(8192),
    temperature: Some(0.7),
    top_p: None,
    tools,
    tools_realize,
    tool_choice: None,
    parallel_tool_calls: None,
    stream: Some(true),
    other: None,
};
```

Supported providers in the framework type layer are `OpenAI`, `Anthropic`, `Fetch`, `OpenRouter`, and `Local`. OpenAI-compatible providers use the same style of `base_url` and model configuration.

## Agent Loop Behavior

The framework agent loop does this:

1. Write the current user prompt into storage.
2. Combine and deduplicate tool definitions.
3. Load session context from local file or database.
4. Compact context when token usage crosses the configured threshold.
5. Call the selected model provider.
6. Persist assistant output.
7. If tool calls exist, execute `tools_realize`, persist tool results, then continue another iteration.
8. If no tool calls exist, return the final response or stream completion.

Streaming events currently use string event types:

- `chunk`: raw model streaming chunk.
- `thinking`: reserved by the framework event type contract.
- `tool_calls`: one or more tool calls have been requested.
- `complete`: final assistant content is complete.
- `error`: framework, provider, context, or tool error.

## Tools

Tools have two parts:

- `tools`: model-visible function definitions.
- `tools_realize`: Rust implementations keyed by function name.

Current implementation signature:

```rust
HashMap<String, Box<dyn Fn(Vec<String>) -> String + Send + Sync>>
```

Important limitation: tool arguments are parsed from the model's JSON argument string into `Vec<String>`. Object arguments are collected from JSON object values, so parameter order should not be relied on for complex tools. For our backend, each tool should either accept a single JSON string argument or have a thin adapter that reconstructs named arguments safely.

The tool function is synchronous. If a future app tool needs async work, the app backend needs an adapter strategy before exposing it as a framework tool.

## Storage

Project requirement: use SQLite for app data.

Framework state in `otherone = 0.1.2`:

- Agent `InputOptions` exposes `StorageType::LocalFile` and `StorageType::Database`.
- Lower-level storage exposes `LocalFile`, `Database`, `Redis`, `Mysql`, and `Mongodb`.
- `DatabaseConfig` is host/port/user/password based and is not a SQLite connection config.

Project decision:

- Use localfile and SQLite together.
- localfile stores framework-managed session/context data.
- SQLite stores app-managed configuration, provider records, model records, storage path preference, session indexes, and metadata.
- API/model config currently uses `app_data_dir/otherone.sqlite` through Tauri commands `load_api_configs` and `save_api_configs`.
- Model connectivity testing uses Tauri command `test_ai_model`. It calls `otherone::ai::invoke_model_stream`, waits for the first stream chunk with a 30-second timeout, returns first-chunk latency in milliseconds, then drops the stream.

The backend should keep a clear adapter between these two stores. A storage path change must migrate both localfile data and the SQLite database before saving the new path as active.

## Skills And MCP

`otherone-skills` can discover `SKILL.md` files from default paths and format them into prompt text. The app can use this to enrich `system_prompt`.

`otherone-mcp` exposes `McpManager` for connecting to stdio, SSE, and streamable HTTP MCP servers, listing tools, and calling tools. If we expose MCP tools through the agent, we need an adapter from async MCP calls to the current synchronous `tools_realize` interface.

## Recommended Backend Shape

Keep the app-specific backend boundaries explicit:

- `agent_service`: builds `InputOptions` and `AiOptions`, invokes `Otherone::invoke_agent_stream`.
- `model_config`: reads provider, base URL, model, and API key from backend-only config.
- `tool_registry`: owns app tools and converts them into framework `Tool` definitions plus `tools_realize`.
- `session_repository`: owns SQLite metadata and indexes while framework session/context entries remain in localfile storage.
- `tauri_commands`: validates frontend input and maps stream events to frontend message-panel events.

The frontend should not receive API keys, provider credentials, local storage paths, or raw internal errors.

Current exception: API keys are still present in the settings form state because the API settings page is the editing surface. Backend commands should not log or echo them.

## Open Decisions

- Define the exact localfile + SQLite directory layout.
- Confirm default provider and configuration source.
- Confirm stream event payload shape for the frontend message panel.
- Confirm which first-party tools should exist in the initial backend.
