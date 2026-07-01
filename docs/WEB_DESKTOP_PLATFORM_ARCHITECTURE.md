# Web/Desktop Platform Architecture

## Scope
- Keep the existing Tauri desktop app as the fully supported runtime.
- Prepare the React frontend so it can run in a browser and call a future Web backend.
- Do not migrate desktop SQLite/localfile behavior in this phase.
- Do not implement the full Web backend in this phase.

## Runtime Boundary
- Desktop runtime: detected by `window.__TAURI_INTERNALS__`.
- Web runtime: any non-Tauri browser runtime.
- Desktop services call Tauri commands and event listeners through a shared adapter.
- Web services call a configured HTTP/SSE backend through a shared adapter.
- Browser preview without a Web API remains non-destructive: read operations return empty/null where the previous code already did, and real chat/model actions still fail explicitly.

## Configuration
- `VITE_OTHERONE_WEB_API_BASE_URL` points the browser frontend to the Web backend.
- Empty `VITE_OTHERONE_WEB_API_BASE_URL` means browser preview mode only.
- The Web API base URL must not include secrets.

## Frontend Service Layers
- `src/services/platform/runtime.ts`: runtime detection and Web API configuration.
- `src/services/platform/tauri.ts`: desktop IPC/event helpers.
- `src/services/platform/webApi.ts`: Web HTTP request and SSE helpers.
- Existing `src/services/*` files remain the public service facade for components.

## First Web API Contract Draft
- `GET /api/sessions`
- `GET /api/sessions/:sessionId`
- `PATCH /api/sessions/:sessionId/title`
- `POST /api/chat/messages`
- `POST /api/chat/messages/enqueue`
- `POST /api/chat/messages/cancel`
- `GET /api/chat/stream` as SSE
- `GET /api/api-configs`
- `PUT /api/api-configs`
- `GET /api/app-settings`
- `PUT /api/app-settings/engine`
- `POST /api/ai-model-test`
- `GET /api/sessions/:sessionId/artifacts`
- `GET /api/artifacts/stream` as SSE
- `GET /api/memory/tree`
- `GET /api/workflow/tasks`
- `POST /api/workflow/tasks`
- `PATCH /api/workflow/tasks/:taskId`
- `PATCH /api/workflow/tasks/:taskId/status`
- `DELETE /api/workflow/tasks/:taskId`
- `GET /api/plugins`
- `POST /api/plugins/install`
- `POST /api/plugins/uninstall`
- `POST /api/plugins/skills/import-url`
- `POST /api/plugins/mcp/import`
- `POST /api/plugins/mcp/import-url`
- `GET /api/weixin-clawbot/status`
- `POST /api/weixin-clawbot/login/begin`
- `POST /api/weixin-clawbot/login/check`
- `POST /api/weixin-clawbot/start`
- `POST /api/weixin-clawbot/stop`
- `POST /api/weixin-clawbot/reset`
- `GET /api/weixin-clawbot/events`

## Key Decisions
- Keep component imports stable. Components continue importing from `src/services/*`.
- Keep platform-specific code out of React components.
- Keep native filesystem operations desktop-only for now.
- Use the same TypeScript request/response types for desktop and Web service facades.
- No new dependency is needed for the first adapter layer.

## Risks
- Web API contracts are a draft until the real Web backend is implemented.
- SSE streams may need auth/session handling once Web login exists.
- Desktop-only features such as local directory selection cannot be made Web-compatible without product decisions.
- Browser preview can show UI but is not a complete product until the Web backend exists.

## Rollback
- Revert service files to direct Tauri imports.
- Remove `src/services/platform/*`.
- Remove `VITE_OTHERONE_WEB_API_BASE_URL` usage.
- Desktop Rust commands and persisted data remain unchanged.

## Verification
- `npm run build` from `app/frontend`.
- Desktop smoke test through `npm run tauri dev` when runtime verification is needed.
- Browser preview with no `VITE_OTHERONE_WEB_API_BASE_URL` should load without Tauri import failures.

## Current Implementation Status
- Platform adapters are implemented under `app/frontend/src/services/platform`.
- Existing frontend service facades now route through desktop IPC or Web API adapters.
- `npm run build` passes.
- `cargo check` from plain PowerShell is blocked by missing MSVC `link.exe`; use `app/frontend/scripts/run-tauri.cmd` or a Visual Studio developer shell for Rust/Tauri checks.
