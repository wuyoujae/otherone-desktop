# Web Backend Migration

## Approach
- Build a shared Rust backend core in `app/backend/core`.
- Keep Tauri commands and Web HTTP handlers as transport wrappers over the shared core.
- Migrate features one group at a time and verify each group before continuing.
- Exclude Weixin ClawBot from the first full Web parity pass unless explicitly resumed later.

## Checklist
- [x] Add shared backend core crate.
- [x] Add Web HTTP API crate.
- [x] Migrate app settings and API model config to shared core.
- [x] Expose Web API routes for app settings and API model config.
- [x] Migrate session list/read/title update.
- [x] Migrate AI model test.
- [x] Migrate workflow list/range/status/delete data APIs.
- [x] Migrate artifacts list API and Web SSE event channel.
- [x] Add artifact download API and frontend Web download action.
- [x] Migrate memory tree read.
- [x] Migrate chat send/enqueue/cancel and SSE transport.
- [x] Migrate server-side agent tools for Web chat.
- [x] Migrate workflow AI task create/update.
- [x] Connect artifact recording events to Web chat stream.
- [x] Migrate plugins, Skill import URL, and MCP import.
- [x] Add Web-specific frontend disabled states for native-only file-system actions.
- [x] Run Desktop and Web verification for each migrated feature group.

## Key Decisions
- Web first version uses a server-side data root configured by `OTHERONE_WEB_DATA_ROOT`.
- Web full tools operate on the server workspace, not on the browser user's local machine.
- `axum` is used for the Web API because the project needs typed Rust HTTP handlers and SSE.
- `tower-http` is used only for CORS so the existing Vite frontend can call the Web API in development.
