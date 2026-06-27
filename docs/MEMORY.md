# Memory Module

## Scope

- Desktop chat memory is stored by `otherone-agent` under the configured dialogue root:
  `.otherone/memory/long-term-memory.json`.
- The first Personalization page version is read-only. It visualizes existing memory data and must not create, mutate, deactivate, or delete memory points.
- The memory model selector is currently UI state only.
- The memory toggle controls long-term memory for new desktop chat runs. Turning it off keeps existing memory data intact but sends `enable_long_term_memory: false` to `otherone-agent`.

## Current Implementation

- Backend command: `read_memory_tree`
  - Resolves the current dialogue root from app settings.
  - Reads and validates `long-term-memory.json` when it exists.
  - Returns a DTO with camelCase fields for the frontend.
  - Returns a headless-only tree when the file does not exist, without creating the file.
- Frontend page: `PersonalizationPage`
  - Keeps the model selector and memory toggle in the top toolbar.
  - Loads memory tree data through `readMemoryTreeFromStorage`.
  - Renders the tree through `MemoryTreeScene`.
- Chat execution:
  - `memoryEnabled` is captured when the user starts a pending desktop chat send.
  - The value is sent with `send_chat_message` and mapped to `InputOptions.enable_long_term_memory`.
  - Mid-run queued messages use the current Agent run, so a toggle change takes effect on the next new run.
- Visualization:
  - Uses Three.js.
  - Renders the headless point as the core anchor.
  - Places real `MemoryPoint` nodes deterministically from parent-child relationships.
  - Hovering a node shows type, storage, status, depth, and timestamps.

## Known Limits

- The desktop shell currently has a global 820px minimum width, so true phone-width layout is clipped by the app shell.
- Large memory trees are rendered all at once in this version. Add capping or progressive rendering if real trees become large.
- Refresh is manual. Live updates can be added when memory writes become user-visible.
