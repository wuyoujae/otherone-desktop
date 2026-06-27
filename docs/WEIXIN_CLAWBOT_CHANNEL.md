# Weixin ClawBot Channel

## Summary

Weixin ClawBot is implemented as a backend-owned external message channel.
It is not installed through the existing Agent plugin registry.

The first version supports one connected Weixin ClawBot account, QR login, text direct-message polling, per-sender Agent sessions, safe Agent invocation, text replies, and basic runtime diagnostics.

Reference implementation:

- [SiverKing/weixin-ClawBot-API](https://github.com/SiverKing/weixin-ClawBot-API)
- Key protocol lesson: replies must use the inbound `context_token` and send SDK-compatible `sendmessage` fields.

## User Decisions

- Add `微信 ClawBot` as a first-level sidebar tab.
- Place it after `个性化`; it is not content inside the `个性化` tab.
- Keep visual style consistent with existing operational pages such as `WorkflowPage` and `PluginsPage`.
- Do not require users to install OpenClaw Gateway for this first-party integration.
- First version is text-only direct chat.

## Implemented Paths

Frontend:

- `app/frontend/src/App.tsx`
  - Adds `weixinClawbot` view.
  - Adds the `微信 ClawBot` sidebar item after `个性化`.
  - Renders `WeixinClawbotPage`.
- `app/frontend/src/components/WeixinClawbotPage.tsx`
  - Status strip, QR login, start/stop controls, safety policy, recent events.
  - Renders QR codes defensively from `data:image`, SVG, base64 image, HTTP image converted by backend, or plain QR payload.
- `app/frontend/src/services/weixinClawbotService.ts`
  - Tauri command wrappers.
- `app/frontend/src/types/weixinClawbot.ts`
  - Frontend-safe status, QR, login check, and event types.
- `app/frontend/src/styles.css`
  - Page, panel, status, QR, and event-list styles.

Backend:

- `app/frontend/src-tauri/src/weixin_clawbot.rs`
  - SQLite schema.
  - Tauri commands.
  - iLink HTTP client helpers.
  - QR login and login status polling.
  - Runs login/status network calls through blocking tasks so the UI is not held by synchronous IPC work.
  - Converts HTTP/SVG QR image content into frontend-safe image data URLs when possible.
  - Long-poll runtime.
  - Inbound text parsing.
  - Agent session mapping.
  - `getconfig`, `sendtyping`, and text `sendmessage`.
- `app/frontend/src-tauri/src/chat.rs`
  - Adds `invoke_channel_agent` for non-UI channel calls.
- `app/frontend/src-tauri/src/tools.rs`
  - Adds `build_weixin_safe_tools`.
- `app/frontend/src-tauri/src/main.rs`
  - Registers commands and initializes tables.

## Commands

- `weixin_clawbot_status`
- `weixin_clawbot_begin_login`
- `weixin_clawbot_check_login`
- `weixin_clawbot_start`
- `weixin_clawbot_stop`
- `weixin_clawbot_list_events`

## Runtime Flow

1. User opens `微信 ClawBot`.
2. User generates a login QR.
3. Backend requests the QR from iLink and returns display-safe QR content.
4. User scans and confirms in Weixin.
5. Backend stores token/base URL and can start long polling.
6. Inbound text message arrives from `getupdates`.
7. Backend maps the sender to an `otherone-agent` session.
8. Backend requests typing config and sends typing status `1`.
9. Backend invokes the Agent through `invoke_channel_agent`.
10. Backend sends the final assistant text with the current inbound `context_token`.
11. Backend sends typing status `2` and records an event.

## QR Payload Rules

The current iLink QR response has two different fields:

- `qrcode`: opaque login status token, currently shaped like a 32-character hex string. It is used by `get_qrcode_status` and must not be rendered as the scan QR.
- `qrcode_img_content`: scan/open payload. Current responses can be a `https://liteapp.weixin.qq.com/q/...?...` URL, image data, SVG, or base64 image content.

Frontend QR generation must use `qrcode_img_content`. If it is missing, show an error instead of generating a fake QR from `qrcode`.

## Database

Use the existing SQLite file at `dataRoot/otherone.sqlite`.

Schema is additive:

```sql
CREATE TABLE IF NOT EXISTS weixin_clawbot_accounts (
  id TEXT PRIMARY KEY,
  bot_user_id TEXT NOT NULL DEFAULT '',
  ilink_user_id TEXT NOT NULL DEFAULT '',
  bot_token TEXT NOT NULL DEFAULT '',
  base_url TEXT NOT NULL DEFAULT '',
  get_updates_buf TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT 'disconnected',
  login_expires_at TEXT,
  last_connected_at TEXT,
  last_poll_at TEXT,
  last_error TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS weixin_clawbot_sessions (
  account_id TEXT NOT NULL,
  from_user_id TEXT NOT NULL,
  agent_session_id TEXT NOT NULL,
  last_context_token TEXT NOT NULL DEFAULT '',
  last_message_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (account_id, from_user_id)
);

CREATE TABLE IF NOT EXISTS weixin_clawbot_events (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL,
  direction TEXT NOT NULL,
  from_user_id TEXT NOT NULL DEFAULT '',
  summary TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT '',
  error TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

First-version compromise:

- `bot_token` is stored in SQLite, matching the current local API-key storage posture.
- The frontend never receives `bot_token`, `Authorization`, raw request bodies, or full raw user IDs.

## Agent Safety

Weixin-originated Agent runs do not use the full desktop chat tool surface.

Allowed tools:

- `web_fetch`
- `web_search`
- `skill`
- `plugin_tool`

Not allowed by default:

- shell execution
- file write/edit
- local filesystem reads
- desktop-only artifact capture

## Dependencies

- `qrcode@^1.5.4`
  - Purpose: render Weixin QR payloads locally when iLink returns non-image content or when an external QR image cannot be displayed by the Tauri WebView.
  - Rationale: QR generation is a correctness-heavy standard with encoding and error correction; a mature library is safer than a custom implementation.
- `@types/qrcode@^1.5.6`
  - Purpose: TypeScript type coverage for the frontend wrapper.

## First-Version Done

- [x] Confirm first-version scope and safety defaults.
- [x] Add `微信 ClawBot` sidebar entry as a sibling of `个性化`.
- [x] Keep page style consistent with the desktop app.
- [x] Add page shell with status, QR login, controls, and event list.
- [x] Add frontend service/types.
- [x] Add iLink HTTP helpers.
- [x] Add SQLite table initialization.
- [x] Add QR login commands and frontend QR display flow.
- [x] Add login status polling with redirect and verify-code support.
- [x] Add long-poll runtime with start/stop guard.
- [x] Add text inbound parsing and current `context_token` capture.
- [x] Add per-sender Agent session mapping.
- [x] Add Weixin-safe Agent invocation helper.
- [x] Add sendtyping before/after Agent runs.
- [x] Add text sendmessage with SDK-compatible fields.
- [x] Add basic runtime events for UI diagnostics.
- [x] Verify frontend build and Rust check.

## Later TODO

- [ ] Move `bot_token` from SQLite plaintext to platform secure storage.
- [ ] Support multiple Weixin accounts.
- [ ] Add allowlist/owner controls for who can talk to the Agent.
- [ ] Add explicit confirmation flow for sensitive tools.
- [ ] Support message splitting for long replies.
- [ ] Support image input and image output.
- [ ] Support voice transcription and voice replies.
- [ ] Support file upload/download through Weixin CDN.
- [ ] Add group-chat detection and an explicit group policy.
- [ ] Add richer delivery/error telemetry.
- [ ] Add rate limiting and abuse controls per sender.
- [ ] Add export/cleanup controls for channel events.
- [ ] Track upstream `@tencent-weixin/openclaw-weixin` protocol changes.

## Risks

- iLink API can change without notice.
- Current token storage is local SQLite plaintext.
- Any Weixin sender who can message the connected account can trigger the channel unless allowlists are added.
- First version is text-only; media and group behavior are intentionally not handled.
- Agent replies may exceed Weixin message size expectations until splitting is added.
