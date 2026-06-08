# Chat, Storage, and Engine Configuration Plan

## Scope

Implement real AI conversation through `otherone-agent`, configurable storage paths, and complete model/engine settings.

This is a cross-module change touching:

- `app/frontend/src/App.tsx`
- `app/frontend/src/components/ModelSettings.tsx`
- `app/frontend/src/types/apiConfig.ts`
- `app/frontend/src-tauri/src/storage.rs`
- `app/frontend/src-tauri/src/session.rs`
- new backend chat/storage/config modules as needed
- `docs/API_MODEL_SETTINGS.md`
- `docs/SESSION_STORAGE.md`

## Current Findings

### Framework Parameters

`otherone-agent` has two relevant configuration layers.

Model/provider layer: `AiOptions`

- `provider`
- `api_key`
- `base_url`
- `model`
- `user_prompt`
- `system_prompt`
- `messages`
- `context_length`
- `temperature`
- `top_p`
- `tools`
- `tools_realize`
- `tool_choice`
- `parallel_tool_calls`
- `stream`
- `other`

Agent/context layer: `InputOptions` and `CombineContextOptions`

- `session_id`
- `context_load_type`
- `storage_type`
- `database_config`
- `context_window`
- `threshold_percentage`
- `max_iterations`
- compaction LLM config through `AiOptions.other`

### Current App State

- API/provider/model config is stored in SQLite.
- Session reads use `otherone` localfile by setting `Otherone::set_localfile_root(dialogueRoot)`.
- Settings already has a storage page with three visible paths:
  - data storage path
  - artifact storage path
  - dialogue data storage path
- These paths are still frontend-only state and do not migrate data.
- Prompt reasoning options are `High`, `Medium`, and `Low`.
- Real send-message/new-session stream API is not implemented.

## Decisions To Apply

### Storage Paths

Use three app settings:

- `dataRoot`: app-owned SQLite and general app data.
- `artifactRoot`: generated files, reports, images, and run artifacts.
- `dialogueRoot`: `otherone-agent` localfile root for all conversation data.

The active localfile path should become:

```text
{dialogueRoot}/.otherone/storage/otherone-storage.json
```

The backend localfile adapter should use `dialogueRoot` instead of hard-coded `app_data_dir/agent`.

### Migration

Add Tauri commands:

- `load_app_settings()`
- `save_engine_settings(payload)`
- `migrate_storage_settings(payload)`

Migration should be explicit and backend-owned:

1. Validate target directory.
2. Serialize all storage operations behind a mutex.
3. Copy SQLite database when `dataRoot` changes.
4. Copy localfile directory when `dialogueRoot` changes.
5. Create artifact directory when `artifactRoot` changes.
6. Verify copied files can be opened/read.
7. Save the new active paths only after verification.
8. Delete old managed data after the migration succeeds.

The frontend must show an explicit warning before migration: if migration or cleanup causes data loss, the deleted old data cannot be recovered by the app, so users should manually back up the old storage directories first.

### API Model Blocks

Restore complete model configuration in API key settings:

- model name
- context length
- context window
- temperature
- top P
- stream
- parallel tool calls
- tool choice policy
- default model
- provider-specific extra JSON passed through `AiOptions.other`

Keep API key/base URL/provider fields at provider level.

### Model And Engine Settings

Add framework-level config to the `模型与引擎` settings page:

- system prompt
- max Agent iterations
- compression threshold percentage
- compaction keep ratio if exposed through app-owned config
- default context load/storage mode display, locked to LocalFile for now
- compact LLM source:
  - use selected conversation model
  - or select a configured model for compaction
- default conversation reasoning mode

Do not put `max_iterations` and `threshold_percentage` inside each model block.

### Reasoning Mode

Update prompt reasoning options:

- `None` / `不要思考`
- `Low`
- `Medium`
- `High`

`None` should omit reasoning parameters from `AiOptions.other`.

For non-`None`, pass reasoning as provider-compatible extra config through `AiOptions.other`. The exact key should be app-owned and mapped per provider when needed; first version can pass both:

- `reasoning_effort`
- `reasoningEffort`

### Real Chat

Add Tauri command:

- `send_chat_message(payload) -> starts stream and emits window events`

Frontend flow:

1. User sends message from new draft or existing session.
2. If no `sessionId`, backend generates a new UUID.
3. Backend builds `InputOptions` from engine config and selected model config.
4. Backend builds `AiOptions` from provider/model/API config and prompt controls.
5. Backend invokes `Otherone::invoke_agent_stream`.
6. Backend emits stream events to frontend:
   - `user_entry`
   - `assistant_delta`
   - `tool_calls`
   - `tool_result`
   - `complete`
   - `error`
7. Frontend appends the user message immediately, streams AI text into the current AI message item, then reloads session summary/detail after completion.

The first send creates the session only when `otherone-agent` writes the first user entry. Do not call `Otherone::create_new_session()` on clicking `新对话`.

## Technical Approach

### Backend

- Add `app_settings.rs` for storage paths and engine settings.
- Change `session.rs` localfile adapter to read `dialogueRoot`.
- Add `chat.rs` for `send_chat_message`.
- Reuse existing `storage.rs` SQLite connection pattern.
- Keep a backend-level mutex around localfile operations and path migration.
- Do not log API keys or return provider secrets from chat commands.

### Frontend

- Add storage settings service for Tauri commands.
- Replace frontend-only path state with loaded backend settings.
- Add a real `EngineSettings` component under `模型与引擎`.
- Restore full model fields in `ModelSettings`.
- Add `none` to `ReasoningEffort`.
- Connect send button and `Ctrl+Enter` to `send_chat_message`.
- Subscribe to chat stream events and update `MessagePanel` state.

Visual thesis: keep the settings surface dense, quiet, and tool-like, matching the current desktop app rather than adding marketing-style panels.

Content plan:

- Storage page: three path sections with status and migration action.
- Model settings: provider blocks plus full model blocks.
- Engine settings: Agent loop, context compression, system prompt, and compaction model controls.
- Prompt: reasoning dropdown includes `不要思考`.

Interaction thesis:

- Storage migration shows explicit running/success/error status.
- Engine/model sections use existing custom controls and collapsible blocks.
- Chat streaming updates the current AI message in place instead of replacing the whole panel.

## Risks

- `otherone = 0.1.2` exposes configurable localfile root; stream calls no longer need temporary `current_dir` switching.
- Safer path for chat streaming is to set the process current directory to `dialogueRoot` for the app runtime after settings load, or to require no path changes during active streams.
- API keys are still stored in SQLite plaintext. Encryption remains a separate required hardening task.
- Storage migration while a chat stream is running can corrupt or split data unless blocked.
- `AiOptions.other` is loose JSON, so provider-specific reasoning keys may need refinement after testing real providers.

## Rollback

- Storage migration saves the new active paths only after copied data is verified.
- After successful migration, old managed data is deleted, so rollback depends on the user's manual backup.
- If new storage settings fail, keep using the previous saved paths.
- If chat stream fails, keep the user-only session, matching the existing retry decision.
- If engine config migration fails, fall back to conservative defaults:
  - max iterations: 8
  - threshold percentage: 0.8
  - stream: true
  - reasoning: `medium`

## Confirmed Decisions

1. `dialogueRoot` is the only root used for `otherone-agent` localfile session/context data.
2. Storage migration copies all managed data to the new locations, verifies it, saves the new paths, then deletes old managed data.
3. Chat streaming can block storage path changes while a stream is active.
4. Reasoning `不要思考` means omit reasoning fields from `AiOptions.other`.
