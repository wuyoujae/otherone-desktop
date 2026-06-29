# Frontend Desktop App Bootstrap

## Approach
- Create a minimal Vite + React + TypeScript frontend under `app/frontend`.
- Add a minimal Tauri v2 shell configuration in `app/frontend/src-tauri`.
- Rebuild `resources/propertypes/index.html` as the initial desktop app screen.

## Checklist
- [x] Add package, TypeScript, Vite, and Tauri config files.
- [x] Implement the prototype UI as the React entry view.
- [x] Port prototype styles with scoped app assets and interactions.
- [x] Install dependencies and verify the frontend build.
- [x] Run the app in browser and compare against the prototype.

## Key Decisions
- Keep the implementation as a single prototype screen until real routes/data are required.
- Use `lucide-react` instead of the prototype CDN script so the desktop app works without external runtime script loading.

## Message Panel Detail

### Approach
- Add a React message panel for the chat area based on `resources/propertypes/messages.html`.
- Keep the structure as panel -> message -> item.
- Support text, static tool, collapsible tool, and nested agent message items.

### Checklist
- [x] Add message panel component with mock user/AI data.
- [x] Add message item action icons by role.
- [x] Add running/completed styles and shimmer effect.
- [x] Add collapsible tools and nested agent panel behavior.
- [x] Verify build and rendered chat page.

### Key Decisions
- Use local component state for collapsible demo items.
- Message panel now renders backend session data. Mock copy has been removed from the component.

## Session Read Integration

### Approach
- Read local sessions through Tauri commands backed by `otherone-agent` localfile storage.
- Render sidebar history from backend summaries instead of hard-coded prototype items.
- Read session detail on sidebar click and pass backend message groups into `MessagePanel`.

### Checklist
- [x] Add frontend session types and Tauri service wrapper.
- [x] Replace hard-coded sidebar history items with backend session summaries.
- [x] Replace `MessagePanel` mock data with props-driven rendering.
- [x] Add empty/loading/error states for session reads.
- [x] Add double-click title editing through SQLite session metadata.
- [ ] Connect real send-message/new-session stream API.

## Chat, Storage, And Engine Configuration

### Approach

- Follow `docs/CHAT_STORAGE_AND_ENGINE_PLAN.md`.
- Replace frontend-only storage path state with backend-loaded settings.
- Restore complete model config fields in API key settings.
- Add framework-level Agent/context controls in the model and engine page.
- Connect prompt send to the backend stream API after the event contract is confirmed.

### Checklist

- [x] Confirm chat/storage/engine plan with the user.
- [x] Add `不要思考` reasoning option in the prompt.
- [x] Restore full model fields in API model blocks.
- [x] Add model and engine settings component.
- [x] Wire storage settings page to backend commands.
- [x] Connect chat send button and `Ctrl+Enter` to real stream events.

### Key Decisions
- Clicking "新对话" still only opens the draft new-chat page and does not create a framework session.
- Browser preview does not use mock session data; real sessions are available in the Tauri runtime.

## Streaming Message Rendering Fix

### Approach

- Keep using the current Tauri `chat_stream_event` contract.
- Make frontend stream updates resilient when the temporary AI group is missing.
- Add a collapsible thinking item that reuses the existing tool-message interaction style.
- Separate thinking deltas from final assistant text deltas.

### Checklist

- [x] Add thinking item types and message panel rendering.
- [x] Emit backend thinking deltas separately from assistant text deltas.
- [x] Update frontend stream reducer to create or update active stream groups robustly.
- [x] Verify Rust and frontend builds.

## API Model Settings

### Approach
- Recreate `resources/propertypes/model-setting.html` inside the existing settings page.
- Keep the structure as provider config block -> model config block.
- Add mock interactions for creating providers, creating models, editing fields, toggling API key visibility, and setting the default model.

### Checklist
- [x] Add settings sub-navigation state.
- [x] Add API/model configuration component with mock provider data.
- [x] Add provider-level fields for provider type, official URL, base URL, and API key.
- [x] Add model-level fields matching `otherone-agent` options.
- [x] Add general settings storage path UI for future localfile + SQLite migration.
- [x] Verify frontend build.
- [x] Replace native select, checkbox, and range visuals with custom controls.
- [x] Add collapsible provider config blocks.
- [x] Scroll to the new provider block after adding it.
- [x] Feed configured models into the new-chat model selector.
- [x] Add sidebar collapse behavior and move collapse control into the logo area.
- [x] Add prompt textarea auto-height behavior.
- [x] Persist API/model config through Tauri SQLite commands.
- [x] Add provider test action and global toast queue for AI config feedback.
- [x] Move framework-level options out of model blocks and recreate the prompt function panel.
- [x] Wire storage path selection and migration to Tauri backend commands.

### Key Decisions
- The API settings page owns model configuration because each provider can have multiple models.
- API/model config is stored through Tauri commands in local SQLite for the desktop runtime.
- Browser preview does not write API keys to Web Storage.
- The app storage plan is localfile plus SQLite: localfile for framework session/context data, SQLite for app config, provider records, model records, and metadata.
- AI model testing should use the desktop backend and show user feedback through the app toast system.

## Chat Batch Prompt Backend

### Approach
- Keep `send_chat_message` backward-compatible with the existing single `prompt` field.
- Add an optional `prompts` array for delayed frontend batches.
- For multi-prompt batches, write each prompt as its own localfile user entry before invoking the Agent.
- Start the Agent with `user_prompt: None` after prewriting, so the framework does not duplicate a merged user message.

### Checklist
- [x] Add `prompts` to the frontend chat service contract.
- [x] Normalize backend prompt batches with single-prompt fallback.
- [x] Prewrite multi-prompt batches as separate user entries.
- [x] Preserve single-prompt and Weixin channel behavior.
- [x] Verify frontend build and Rust check.

### Key Decisions
- Localfile remains the source used by `otherone-agent` to build model messages.
- No new dependency is needed because `otherone::storage` re-exports the storage writer.

## Workflow AI Task Modification

### Approach
- Reuse the existing task prompt surface for editing selected workflow tasks.
- Send the current task plus the user's natural-language edit instruction to the configured workflow model.
- Persist either one updated concrete task or multiple expanded concrete tasks when the edit becomes recurring.

### Checklist
- [x] Add a Tauri command for AI task modification.
- [x] Wire the task edit prompt to the command from `TaskView`.
- [x] Refresh the selected-date task list after modification.
- [x] Verify frontend and Rust builds.

### Key Decisions
- A single AI result updates the existing task row in place.
- Multiple AI results replace the original concrete task with a generated series of task rows.

## Mid-Run Chat Message Insertion

### Approach
- Use the `otherone-agent` interactive stream command sender for active chat runs.
- Keep the first-message remorse window unchanged.
- When a session is already streaming, enqueue prompts instead of starting another run.
- Switch the frontend active AI group only after the backend emits `queued_user_prompts`.

### Checklist
- [x] Add frontend service wrapper for `enqueue_chat_message`.
- [x] Add Tauri command and active command sender map.
- [x] Switch main chat stream to `invoke_agent_stream_interactive`.
- [x] Render optimistic queued user messages and pending AI group.
- [x] Verify frontend build and Rust check.

### Key Decisions
- Queued prompts are persisted by the framework at safe provider-ordering boundaries.
- If the current AI group has not emitted content yet, extra prompts are inserted before that same empty group.
- The desktop crate now depends on the published `otherone = "0.3.0"` package.

## Weixin ClawBot Multi-Message Channel

### Approach
- Keep Weixin ClawBot as a backend-owned channel.
- Process each inbound Weixin text immediately, matching the upstream sample's one-message/one-token flow.
- Reuse one channel Agent session per Weixin sender within the current ClawBot connection.
- Keep the channel safety instruction in the Agent system prompt, not inside each stored user message.
- Reply to Weixin with that inbound message's exact `context_token`.

### Checklist
- [x] Add channel invocation helper in `chat.rs`.
- [x] Replace per-sender debounce with immediate per-message processing in `weixin_clawbot.rs`.
- [x] Reuse the existing Weixin-safe tool scope.
- [x] Reuse the sender's Weixin Agent session across messages in the same connection.
- [x] Refresh the Weixin ClawBot page layout and remove the recent-message and safety-boundary cards.
- [x] Add a reset command and UI action for clearing stale token, polling cursor, queue, active Agent runs, and sender session mapping.
- [x] Rotate stale Weixin Agent sessions when old localfile history has invalid tool-call ordering.
- [x] Simplify the Weixin ClawBot page into one primary next-step action plus contextual reset.
- [x] Move polling cursor persistence after fetched messages are queued.
- [x] Retry outbound Weixin text sends before recording delivery failure.
- [x] Avoid debounce/batching so replies follow the upstream sample's one-message/one-token flow.
- [x] Detect nested iLink business errors such as `base_resp.ret` before marking replies as sent.
- [x] Treat iLink session timeout/token expiry as a disconnected state that requires a fresh QR login.
- [x] Add a timeout for Weixin Agent runs so typing state does not hang forever.
- [x] Update Weixin channel documentation.
- [x] Verify frontend build and Rust check.

### Key Decisions
- Weixin channel deliberately avoids batching because iLink delivery is sensitive to replying with the exact inbound `context_token`.
- Each Weixin sender keeps one Agent session per active ClawBot connection so normal follow-up messages have context.
- Polling cursor persistence happens after local enqueue so a stopped or failed loop does not skip unqueued Weixin messages.
- Weixin avoids mid-run Agent insertion because the upstream ClawBot examples use one inbound message per outbound reply.
- Outbound text sends use a small retry loop; final failure is recorded as an outbound error event.
- `session timeout` / expired token is not retried indefinitely; the stored token is cleared and the UI returns to QR login.
- Weixin Agent runs have a bounded wait time and send a fallback instead of leaving Weixin in typing state forever.
- Reset is intentionally local-only: it stops polling, cancels active channel Agent runs, clears stored channel state, and requires a fresh QR login.
- After reset, delayed Weixin work from the old connection is dropped and the next inbound message creates a fresh Agent session.

## Desktop Long-Term Memory Enablement

### Approach
- Enable framework long-term memory for the main desktop chat Agent path.
- Store memory beside localfile dialogue data under the configured dialogue root.
- Keep external channel invocations memory-disabled for the first version.

### Checklist
- [x] Configure the framework memory storage root before desktop chat runs.
- [x] Enable long-term memory in desktop chat `InputOptions`.
- [x] Keep channel Agent calls memory-disabled.
- [x] Verify Rust check, or document any external dependency blocker.

### Key Decisions
- Use the framework default recall budget of five memory types for the first desktop version.
- Memory behavior is enabled in the backend, while user-facing controls are introduced first as a frontend personalization surface.

## Personalization Memory Controls

### Approach
- Add the missing main-sidebar Personalization page.
- Place a top navigation bar in that page only.
- Let the left side select the model used by memory assistance.
- Let the right side toggle memory on or off without changing existing memory data.

### Checklist
- [x] Add the Personalization page route and sidebar activation.
- [x] Add the memory model selector using configured API model options.
- [x] Add the memory feature toggle.
- [x] Verify frontend build.

### Key Decisions
- This first version stores the selector and toggle in frontend state only.
- The toggle is wired to new desktop chat runs through `send_chat_message`; turning it off disables long-term memory prompts/tools for that run without mutating or deleting existing memory data.

## Personalization Memory Tree Visualization

### Approach
- Move the memory controls into a top toolbar and remove the standalone "Memory" title/icon.
- Read the framework memory tree from the configured dialogue root.
- Render the tree below the toolbar with Three.js, based on `resources/propertypes/memory.html`.
- Map real memory points to 3D nodes and show point metadata in a hover panel.

### Checklist
- [x] Add a backend command that returns memory points from `long-term-memory.json`.
- [x] Add frontend memory tree types and a storage service wrapper.
- [x] Add Three.js as a frontend dependency.
- [x] Port the prototype tree into a React component with resize and cleanup.
- [x] Replace mock nodes with real `MemoryPoint` data.
- [x] Add hover hit-testing and metadata panel for `types`, `storage`, status, and timestamps.
- [x] Verify frontend build and Tauri Rust check.
- [x] Verify the Three.js canvas is nonblank and correctly framed on desktop and the current app minimum width.
- [ ] If true mobile support is required later, revisit the global `app-shell` 820px minimum width and sidebar behavior.

### Key Decisions
- The headless memory point should be rendered as the core/root anchor, even though it has no user memory content.
- The first visual version is read-only and must not mutate or delete existing memory data.
- Large trees should be capped or progressively rendered later; first version can render all current points from local storage.
- The current desktop shell has a global 820px minimum width, so the visual verification uses desktop and 820px supported-width screenshots; 390px viewport is clipped by the global shell, not by the memory tree component.

## Workflow System Notifications

### Approach
- Add the official Tauri notification plugin for desktop system notifications.
- Start a backend reminder loop when the Tauri app starts.
- Scan pending workflow tasks whose start time is within the next three minutes.
- Mark each reminded task so the same task does not notify repeatedly.

### Checklist
- [x] Add notification dependency and plugin registration.
- [x] Add reminder tracking fields/indexes to workflow task storage.
- [x] Implement backend reminder scan and notification send.
- [x] Document notification behavior.
- [x] Verify frontend build and Rust check.

### Key Decisions
- The first reminder version is backend-owned and does not add a new UI setting.
- Completed tasks are skipped.
- A task with no `start_at` or `scheduled_at` is not eligible for reminders.

## Workflow Todo Model Selection

### Approach
- Add a Todo model selector to the workflow top bar using configured API models.
- Persist the selected model in existing engine settings as `workflowModelId`.
- Pass the selected model id into workflow create and update commands.
- Use the selected model for both Todo creation and natural-language modification.

### Checklist
- [x] Add persisted `workflowModelId` setting.
- [x] Add workflow top-bar Todo model selector.
- [x] Pass `modelId` through frontend workflow storage calls.
- [x] Use selected model in backend workflow model selection.
- [x] Verify frontend build and Rust check.

### Key Decisions
- If no Todo model is selected, the backend falls back to the default model, then the first configured model.
- Creating a Todo now uses the workflow model instead of the fallback-only local parser.

## Workflow Todo AI Config Reuse

### Approach
- Add a shared Tauri backend AI runtime helper module for provider parsing, text validation, tool choice parsing, model selection, and extra parameter merging.
- Keep Workflow Todo creation as a workflow-owned one-shot model call, but reuse the same saved model configuration semantics as chat.
- Register only the `create_todo` tool for Todo creation.

### Checklist
- [x] Add `ai_runtime` shared backend module.
- [x] Move chat provider/tool-choice/extra-param helpers to the shared module.
- [x] Update Workflow Todo creation and modification model calls to use saved model temperature, top P, tool choice, parallel tool setting, and extra params.
- [x] Fail fast when the selected Todo model has tool choice set to `none`.
- [x] Verify Rust check and frontend build.

### Key Decisions
- Do not reuse the chat session/streaming path for Todo creation because Todo needs a synchronous persisted task result and a narrow tool surface.
- Reuse model configuration semantics so Workflow Todo behaves consistently with the normal AI chat model settings.

## Workflow Todo Agent Tools

### Approach
- Add backend Todo CRUD helpers in the workflow module for normal chat Agent tools.
- Register `create_todo`, `list_todos`, `update_todo`, and `delete_todo` through the existing `build_tools_for_session` path.
- Keep the Workflow page and normal chat Agent reading and writing the same `workflow_tasks` rows.

### Checklist
- [x] Add workflow-layer CRUD helpers for Agent tools.
- [x] Add Todo tool definitions and runtime handlers to the main Agent tool registry.
- [x] Update Workflow Todo documentation with the Agent tool contract.
- [x] Verify Rust check and frontend build.

### Key Decisions
- Todo tools accept concrete normalized task data; the Agent expands recurring natural-language requests into multiple concrete tasks before calling `create_todo`.
- `update_todo` updates one concrete task. If one task becomes multiple recurring tasks, the Agent should call `delete_todo` and then `create_todo`.

## Windows Custom Title Bar

### Approach
- Replace the native Tauri window decoration with a lightweight React title bar.
- Keep only the draggable area and Windows window controls.
- Use system color-scheme variables so the title bar background follows Windows light/dark preference while matching the app's restrained surface style.

### Checklist
- [x] Disable native window decoration in Tauri.
- [x] Add custom minimize, maximize/restore, and close controls.
- [x] Add draggable and double-click maximize behavior.
- [x] Verify frontend build.

### Key Decisions
- No new dependency is needed; reuse `@tauri-apps/api/window` and `lucide-react`.
- The title bar is shell-only and does not add new navigation or business state.

## Clear All Otherone Data

### Approach
- Wire the storage settings danger action to a Tauri command.
- Clear only app-managed local data: SQLite files, installed plugin files, dialogue localfile/memory data, and artifact directory contents.
- Reset frontend runtime caches after the backend clear succeeds.

### Checklist
- [x] Add backend clear command with confirmation and active-chat guard.
- [x] Reset Weixin channel and plugin in-memory state during clear.
- [x] Connect the storage settings button to a danger confirmation dialog.
- [x] Clear frontend sessions, artifacts, stream buffers, and pending send timers after success.

### Key Decisions
- Keep `settings.json` and current storage paths so the app can recreate empty data in the same configured locations.
- Do not wipe unknown files directly under `dataRoot`; only known managed files and directories are removed there.

## Skill Import

### Approach
- Let the Skill market import either a local directory that contains `SKILL.md` or a remote `SKILL.md` URL.
- Copy local directories into `dataRoot/skills/imported/<skill-name>`; save URL imports as `dataRoot/skills/imported/<skill-name>/SKILL.md`.
- Install imported skills through the existing `plugin_installs` table.
- Keep imported skills discoverable beside bundled resource skills.

### Checklist
- [x] Add backend validation, copy, discovery, and install behavior for imported skill directories.
- [x] Connect the Skill tab import card to directory selection and refresh.
- [x] Add remote `SKILL.md` URL download, validation, and install behavior.
- [x] Add a Skill import panel for URL input plus local directory selection.
- [x] Document the local skill import path and first-version limits.
- [x] Verify frontend build and Rust check.

### Key Decisions
- URL import supports direct `SKILL.md` text only, not zip packages.
- Imported skill names must come from `SKILL.md` frontmatter and use lowercase letters, numbers, and hyphens.

## MCP Import

### Approach
- Let the MCP market import server configuration from pasted JSON or a remote JSON URL.
- Accept the common `mcpServers` wrapper and a single-server object with `name`.
- Store imported server definitions in SQLite `mcp_servers`; keep enabled state in `plugin_installs` with `kind = 'mcp'`.
- Keep this feature scoped to configuration management; runtime MCP tool execution remains a later adapter task.

### Checklist
- [x] Add backend MCP JSON parsing, validation, URL download, and SQLite persistence.
- [x] Add MCP install/uninstall behavior using the existing plugin install table.
- [x] Load imported MCP entries into the plugin manager and MCP tab.
- [x] Connect the MCP tab import card to URL and JSON import actions.
- [x] Document supported import formats, validation, persistence, and runtime limits.
- [x] Verify frontend build and Rust check.

### Key Decisions
- Support `stdio`, `http`, and `sse` transports because the project docs already call out stdio, SSE, and streamable HTTP as the future `otherone-mcp` path.
- URL import supports direct UTF-8 JSON only, not zip packages.
- Imported MCP configs may contain secrets in `env` or `headers`; they are stored locally as plain JSON until the app gets encrypted secret storage.

## Weixin ClawBot Full Tools And File Delivery

### Approach
- Reuse the normal desktop Agent tool surface for Weixin channel runs only after an explicit permission decision.
- Keep one Agent session per Weixin sender so tool results and follow-up questions share context.
- Record both `write_file` and `edit_file` results as app file artifacts for channel sessions.
- Provide a Weixin-only `send_file_to_weixin` tool for existing files or files generated by shell/PowerShell/REPL.
- After the channel Agent run completes, inspect new file artifacts from that run and send eligible files back through the Weixin iLink media pipeline.
- Implement Weixin file delivery from the official `@tencent-weixin/openclaw-weixin@2.4.6` flow: `getuploadurl` with `media_type=FILE`, AES-128-ECB/PKCS7 encrypt, CDN `POST application/octet-stream`, read `x-encrypted-param`, then `sendmessage` with a single `FILE` item.

### Checklist
- [x] Confirm whether Weixin ClawBot should default to full desktop tools or use a visible full-permission toggle.
- [x] Switch channel Agent tool scope from `WeixinSafe` to the confirmed full-permission behavior.
- [x] Add artifact recording for successful `write_file` results without changing the Agent-visible tool schema.
- [x] Track artifact IDs seen before a Weixin run and collect only newly created artifacts after the run.
- [x] Add a Weixin-only tool for marking existing local files to be sent back.
- [x] Add Weixin CDN upload helpers for file attachments.
- [x] Add Weixin `send_file` / file item message send with retry and sanitized debug logs.
- [x] Use collision-resistant client IDs for separate text/file `sendmessage` calls.
- [x] Send the text reply first, then send each generated file as a separate Weixin file message.
- [x] Update Weixin channel documentation and session artifact documentation.
- [x] Verify Rust format/check and frontend build.
- [ ] Run one manual Weixin file-send test after扫码连接.

### User Decisions
- Permission mode: default full tools for all Weixin senders.
- File sending scope: send only files created/edited by the current Agent run, not arbitrary local paths the model happens to mention.
- File type scope: first version sends regular file attachments only; image/video-specific rendering can follow after attachment delivery is stable.

### Rationale
- Reusing `FullDesktop` keeps Weixin behavior aligned with the normal OtherOne client and avoids maintaining a parallel tool registry.
- Sending only current-run artifacts prevents accidental exfiltration of unrelated local files while still satisfying "AI generated a file and sends it back".
- Official iLink media sending uses one item per `sendmessage`; following that structure avoids silent delivery failures.

### Risks
- Full desktop tools allow Weixin-originated messages to read/write local files and execute commands through the model.
- CDN media upload may fail because Tencent can change iLink response fields or rate limits.
- Large generated files can make Agent runs and Weixin delivery slow; the first implementation should cap file size.

### Rollback
- Restore Weixin channel runs to `AgentToolScope::WeixinSafe`.
- Disable the post-run artifact-to-Weixin delivery step while keeping normal text replies.
- Keep new artifact records harmless in SQLite; they only affect file panels and optional file delivery.
