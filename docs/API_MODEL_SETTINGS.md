# API Model Settings

## Scope

The settings page now includes an API and model configuration prototype based on `resources/propertypes/model-setting.html`.

The UI structure is:

- Settings page
- API key settings section
- Provider config block
- Model config block

Each provider config block contains shared provider data:

- API provider name
- Provider type: `OpenAI`, `Anthropic`, `OpenRouter`, `Fetch`, `Local`, or `OpenAI Compatible`
- Official URL
- Base URL
- API key

Each model config block contains model-specific data:

- Model name
- Context length
- Context window
- Temperature
- Top P
- Streaming response toggle
- Parallel tool calls toggle
- Tool choice policy
- Provider-specific extra JSON passed through `AiOptions.other`
- Default model marker

## Storage Decision

The app should use localfile and SQLite together.

- localfile: framework session entries, context history, and data directly managed by `otherone-agent`.
- SQLite: app configuration, provider records, model records, storage path preference, session indexes, and other app metadata.
- Current implementation stores API/model config in `{dataRoot}/otherone.sqlite`.

Current SQLite tables:

- `api_providers`: provider name, provider type, official URL, base URL, API key, and sort order.
- `api_models`: model name, context parameters, sampling parameters, stream/tool toggles, tool choice, provider extra JSON, default marker, and sort order.

The storage settings page calls backend migration commands. Changing any of the three paths migrates the full managed storage set:

1. Validate the new directory.
2. Close active readers/writers.
3. Copy localfile data.
4. Copy or recreate the SQLite database.
5. Verify migrated data.
6. Update the saved storage path only after migration succeeds.
7. Delete old managed data after successful verification.

The UI warns users to manually back up data first because deleted old data cannot be recovered by the app.

## Security

API/model config is saved through Tauri commands in the desktop runtime. Browser preview does not write API keys into Web Storage.

Current limitation: API keys are stored in local SQLite. Before production use, add encryption or platform secure storage for the `api_key` field.

## UI Behavior

- Provider config blocks are collapsible.
- Adding a provider appends a new block and scrolls the settings content area to that block.
- Selects, segmented choices, toggles, and sliders use custom app controls with CSS transitions.
- The new-chat model selector reads from the current provider/model config state.
- Sidebar collapse is controlled from the `otherone` logo area and hides chat history items while keeping the logo, new chat, settings, and primary feature icons.
- The prompt textarea grows with multiline input until its max height, then scrolls internally.
- The prompt box owns conversation-time thinking depth with `不要思考`, `High`, `Medium`, and `Low` options.
- The prompt plus button expands the function panel from `resources/propertypes/function-pannel.html`.
- Current prompt function panel keeps four actions: upload attachment, compress context, create branch, and target mode.
- Provider config headers include a model test icon before the delete icon.
- The model test action uses the provider default model, or the first model in that provider block when no default is set.
- Test feedback uses the global toast queue in the lower-right corner.
- Toast queue behavior: the newest message immediately becomes the visible flowing-glow item. Older messages are pushed behind it as masked shadow heads with a damped pop-up position transition. When the newest message expires, the next older message returns to the front and starts its own 3-second timer.

## External Links

Provider official website buttons should open URLs with the system default browser. The desktop app uses `@tauri-apps/plugin-opener` and `tauri-plugin-opener` for this behavior, with only `http://` and `https://` URLs accepted by the frontend helper.

## Backend Mapping

Provider and model data should map into `otherone-agent` as:

- provider type -> `ProviderType`
- base URL -> `AiOptions.base_url`
- API key -> `AiOptions.api_key`
- model name -> `AiOptions.model`
- context length/model capacity -> `InputOptions.context_window`
- temperature -> `AiOptions.temperature`
- top P -> `AiOptions.top_p`
- stream -> `AiOptions.stream`
- parallel tool calls -> `AiOptions.parallel_tool_calls`

Framework-level options such as max Agent iterations and compaction threshold are intentionally not configured inside model blocks. They live in the model and engine settings surface.

The model and engine settings surface stores:

- system prompt
- max Agent iterations
- context window default
- compression threshold percentage
- compaction keep ratio
- compact model id
- default conversation reasoning mode

Implementation note: in `otherone = "0.1.2"`, `AiOptions.context_length` is still request output capacity, not model context capacity. The desktop app should not pass model capacity into `AiOptions.context_length`; otherwise models with 64k/128k context can produce invalid requests such as `max_tokens: 128000`.

## Model Test Command

The desktop backend exposes `test_ai_model`.

Behavior:

1. Validate provider type, Base URL, API Key, and model name.
2. Build an `otherone` AI config with one short user message and `stream: true`.
3. Call `otherone::ai::invoke_model_stream`.
4. Measure latency from request start to the first returned stream chunk.
5. Drop the stream immediately after the first chunk and return `{ latencyMs }`.

Browser preview does not run real model tests. It shows a toast telling the user to run the test in the Tauri desktop runtime.
