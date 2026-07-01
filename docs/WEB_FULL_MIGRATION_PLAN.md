# Web Full Migration Plan

## Goal
- Make the existing product usable from Web and Desktop.
- Keep Desktop behavior working through Tauri.
- Add a real Web backend so browser runtime is not limited to preview mode.
- Reuse business logic across Desktop IPC and Web HTTP/SSE instead of maintaining two implementations.

## Assumptions
- First Web version is self-hosted or single-tenant by default.
- Web Agent tools operate on the Web server workspace, not on the browser user's local machine.
- Desktop remains local-first with SQLite + localfile.
- Web backend can initially use SQLite + localfile on the server to stay close to existing storage behavior.
- Public multi-user SaaS requires a follow-up security phase for user isolation, rate limits, secrets, and admin controls.

## Complete Paths
- Frontend app: `app/frontend`
- Tauri backend shell: `app/frontend/src-tauri`
- Shared backend core target: `app/backend/core`
- Web backend target: `app/backend/web`
- Web migration docs: `docs/WEB_FULL_MIGRATION_PLAN.md`
- Existing adapter architecture: `docs/WEB_DESKTOP_PLATFORM_ARCHITECTURE.md`
- Project development manual: `docs/PROJECT.md`

## Technical Approach

### Phase 1: Backend Core Extraction
- Create a shared Rust backend core crate under `app/backend/core`.
- Move platform-independent structs and service functions out of Tauri modules.
- Introduce a runtime context abstraction for:
  - `data_root`
  - `artifact_root`
  - `dialogue_root`
  - event publishing
  - notification dispatch
  - managed file roots
- Keep Tauri commands as thin wrappers around shared core services.
- Keep Web handlers as thin wrappers around the same services.

### Phase 2: Web HTTP/SSE Backend
- Create `app/backend/web` as a Rust HTTP service.
- Add API routes matching the existing frontend Web adapter:
  - Sessions
  - Chat send/enqueue/cancel
  - Chat stream SSE
  - API config
  - App settings
  - AI model test
  - Artifacts and artifact stream SSE
  - Memory tree
  - Workflow tasks
  - Plugin/Skill/MCP management
  - Weixin ClawBot status/login/start/stop/reset/events
- Use SSE for stream events so it matches the current frontend adapter.
- Store Web data under an explicit server data root such as `OTHERONE_WEB_DATA_ROOT`.

### Phase 3: Frontend Web Runtime Completion
- Keep React components unchanged where possible.
- Complete all `src/services/*` Web API paths.
- Add Web-specific disabled states for native-only actions:
  - directory picker
  - open directory in OS
  - reveal file in OS
  - desktop notification permission
- Add a dev command for Web runtime with API URL configured.

### Phase 4: Feature Parity Pass
- Chat:
  - send message
  - stream delta/thinking/tool events
  - enqueue mid-run prompt
  - cancel run
  - load/read/rename sessions
- Settings:
  - provider/model config
  - engine settings
  - model connection test
- Workflow:
  - create/list/update/delete tasks
  - AI task modification
  - reminders as server-side Web notifications/log events first
- Artifacts:
  - list generated artifacts
  - publish artifact events over SSE
  - expose download/open behavior through Web endpoints, not local OS reveal
- Memory:
  - read memory tree from server localfile root
- Plugins/Skills/MCP:
  - import from URL and JSON
  - local directory import remains Desktop-only unless Web file upload is added
- Weixin ClawBot:
  - QR login
  - start/stop/reset
  - event list
  - generated file delivery from server-side artifact root

### Phase 5: Security Hardening
- Add auth/session model before public deployment.
- Add user/tenant isolation to SQLite rows and localfile roots.
- Restrict high-risk Agent tools for non-admin users.
- Add server workspace root allowlist.
- Add request size limits and rate limits.
- Move API keys and tokens to encrypted server-side storage.

## User Decisions Required
- Deployment model:
  - Recommended first step: single-user/self-hosted Web.
  - Later step: multi-user SaaS with full auth and isolation.
- Web tools permission:
  - Recommended first step: full tools only for trusted self-hosted server.
  - Safer public option: no shell and no arbitrary file access until permissions are implemented.
- File behavior:
  - Recommended first step: Web downloads artifacts from server.
  - Desktop keeps native file reveal/open behavior.
- Weixin behavior:
  - Recommended first step: Web backend owns one ClawBot runtime per server.
  - Multi-user Weixin accounts need user-scoped runtime state later.

## Rationale
- Existing Tauri backend is tightly coupled to `tauri::AppHandle`.
- Copying Tauri logic into a Web server would create two divergent backends.
- A shared backend core keeps Desktop stable while making Web feature parity realistic.
- SQLite + localfile keeps the first Web backend close to existing behavior and avoids a database redesign before product validation.

## Risks
- Refactoring `AppHandle` out of current modules touches core behavior and can regress Desktop.
- Web full tools execute on the server machine, which is a different security model from Desktop.
- Existing SQLite tables are mostly single-user; public Web needs tenant/user scoping before production.
- Streaming chat currently publishes through Tauri events; Web needs SSE fanout and cancellation state.
- Weixin ClawBot currently assumes one local runtime; multi-user Web needs runtime isolation.

## Rollback Strategy
- Keep current Tauri commands intact until each shared core service is verified.
- Migrate one service group at a time and keep wrappers thin.
- If Web backend breaks, Desktop can keep using existing Tauri IPC service wrappers.
- Do not remove existing SQLite/localfile data paths during migration.
- Feature-flag Web-only routes until parity checks pass.

## Verification
- Frontend: `npm run build` from `app/frontend`.
- Desktop backend: `cargo check` from a Visual Studio developer shell or through the existing Tauri script environment.
- Web backend: `cargo check` and route smoke tests under `app/backend/web`.
- Manual smoke checks:
  - Desktop chat still streams.
  - Browser chat streams through SSE.
  - Sessions load in both runtimes.
  - Settings save/load in both runtimes.
  - Workflow CRUD works in both runtimes.
  - Artifacts and memory tree load in both runtimes.

## Initial Checklist
- [ ] Confirm first Web version is single-user/self-hosted.
- [ ] Confirm Web full tools are allowed on the server workspace for first version.
- [ ] Create shared backend core crate.
- [ ] Add Web backend crate and HTTP/SSE routing.
- [ ] Move settings/storage/session services into shared core.
- [ ] Move chat streaming into shared core with runtime event publisher.
- [ ] Move workflow, artifacts, memory, plugins, and Weixin behind runtime context.
- [ ] Complete frontend Web-specific states for Desktop-only actions.
- [ ] Verify Desktop build and Web build.
