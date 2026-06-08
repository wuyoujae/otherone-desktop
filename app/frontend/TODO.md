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
