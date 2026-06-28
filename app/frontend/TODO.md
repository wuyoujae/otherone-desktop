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
- Debounce inbound Weixin text messages per sender for a short window before invoking the Agent.
- Send each collected Weixin text as its own user prompt to the channel Agent path.
- Use the interactive Agent command sender for messages that arrive while the Weixin Agent run is still active.
- Keep the channel safety instruction in the Agent system prompt, not inside each stored user message.
- Reply once to Weixin with the latest inbound `context_token` after the batch Agent run completes.

### Checklist
- [x] Add channel batch invocation helper in `chat.rs`.
- [x] Add per-sender pending-message debounce in `weixin_clawbot.rs`.
- [x] Reuse the existing Weixin-safe tool scope.
- [x] Insert mid-run Weixin messages into the active Agent run at framework safe points.
- [x] Refresh the Weixin ClawBot page layout and remove the recent-message and safety-boundary cards.
- [x] Add a reset command and UI action for clearing stale token, polling cursor, queue, active Agent runs, and sender session mapping.
- [x] Rotate stale Weixin Agent sessions when old localfile history has invalid tool-call ordering.
- [x] Simplify the Weixin ClawBot page into one primary next-step action plus contextual reset.
- [x] Move polling cursor persistence after fetched messages are queued.
- [x] Retry outbound Weixin text sends before recording delivery failure.
- [x] Keep mid-run Weixin inserts pending until the framework emits `queued_user_prompts`.
- [x] Detect nested iLink business errors such as `base_resp.ret` before marking replies as sent.
- [x] Update Weixin channel documentation.
- [x] Verify frontend build and Rust check.

### Key Decisions
- First version batches rapid Weixin messages before an Agent run, then enqueues later messages into that active run instead of starting a second stream.
- The batch window uses the same product behavior as the desktop remorse window: short delay, then send the collected prompts together.
- Final Weixin reply uses the latest context token seen for that sender during the active run.
- Polling cursor persistence happens after local enqueue so a stopped or failed loop does not skip unqueued Weixin messages.
- Mid-run context tokens are advanced only after `queued_user_prompts` confirms the prompts were written by the active Agent.
- Outbound text sends use a small retry loop; final failure is recorded as an outbound error event.
- Reset is intentionally local-only: it stops polling, cancels active channel Agent runs, clears stored channel state, and requires a fresh QR login.
- After reset, delayed Weixin batches from the old connection are dropped and the next inbound message creates a fresh Agent session.
- New sender session mappings use a unique session id so reset/retry does not reuse a poisoned localfile history.

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
