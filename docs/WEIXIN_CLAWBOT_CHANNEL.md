# Weixin ClawBot Channel

## Summary

Weixin ClawBot is implemented as a backend-owned external message channel.
It is not installed through the existing Agent plugin registry.

The current version supports one connected Weixin ClawBot account, QR login, text direct-message polling, per-message Agent runs, full desktop Agent tools, text replies, generated-file attachment delivery, and basic runtime diagnostics.

Reference implementation:

- [SiverKing/weixin-ClawBot-API](https://github.com/SiverKing/weixin-ClawBot-API)
- Key protocol lesson: replies must use the inbound `context_token` and send SDK-compatible `sendmessage` fields.

## User Decisions

- Add `微信 ClawBot` as a first-level sidebar tab.
- Place it after `个性化`; it is not content inside the `个性化` tab.
- Keep visual style consistent with existing operational pages such as `WorkflowPage` and `PluginsPage`.
- Do not require users to install OpenClaw Gateway for this first-party integration.
- Weixin ClawBot uses the normal desktop Agent tool surface by default.
- File delivery is limited to files created/edited by the current Agent run or explicitly marked with `send_file_to_weixin`.

## Implemented Paths

Frontend:

- `app/frontend/src/App.tsx`
  - Adds `weixinClawbot` view.
  - Adds the `微信 ClawBot` sidebar item after `个性化`.
  - Renders `WeixinClawbotPage`.
- `app/frontend/src/components/WeixinClawbotPage.tsx`
  - Status strip, QR login, one primary next-step action, contextual reset, and a channel runtime console.
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
  - Detects iLink session timeout/token expiry and returns the account to QR login instead of retrying stale credentials forever.
  - Immediate per-message Agent invocation that replies with the same inbound `context_token`.
  - Persists the polling cursor only after fetched messages have been accepted locally.
  - Active Agent cancel tracking for reset/session-expiry cleanup.
  - Per-sender Agent session reuse within the current ClawBot connection, with reset/session-expiry clearing the mapping.
  - `getconfig`, `sendtyping`, and retried text `sendmessage`.
  - Weixin CDN file attachment upload and outbound `FILE` item delivery for current-run artifacts.
- `app/frontend/src-tauri/src/chat.rs`
  - Adds `start_channel_agent_run` for non-UI external channel multi-prompt calls.
- `app/frontend/src-tauri/src/tools.rs`
  - Provides the normal desktop tool registry used by both desktop chat and Weixin channel runs.
  - Records successful `write_file` and `edit_file` outputs as file artifacts for session-scoped delivery.
  - Adds a Weixin-only `send_file_to_weixin` tool for existing files and files created by shell/PowerShell/REPL/Python/Office tooling.
- `app/frontend/src-tauri/src/main.rs`
  - Registers commands and initializes tables.

## Commands

- `weixin_clawbot_status`
- `weixin_clawbot_begin_login`
- `weixin_clawbot_check_login`
- `weixin_clawbot_start`
- `weixin_clawbot_stop`
- `weixin_clawbot_reset`
- `weixin_clawbot_list_events`

## Runtime Flow

1. User opens `微信 ClawBot`.
2. User generates a login QR.
3. Backend requests the QR from iLink and returns display-safe QR content.
4. User scans and confirms in Weixin.
5. Backend stores token/base URL and can start long polling.
6. Inbound text messages arrive from `getupdates`.
7. Backend records each inbound message and accepts it for local processing.
8. Backend persists `get_updates_buf` only after all messages from that payload have been accepted locally.
9. Backend reuses the sender's `otherone-agent` session for the current ClawBot connection, or creates one if none exists.
10. Backend requests typing config and sends typing status `1`.
11. Backend starts a full-desktop-tool Agent run through `start_channel_agent_run`.
12. If the Agent run does not finish within the channel timeout, backend cancels it and sends a fallback instead of leaving Weixin in typing state forever.
13. Backend sends one final assistant text with that inbound message's exact `context_token`; transient `sendmessage` failures are retried, and nested iLink business errors such as `base_resp.ret` are treated as failures.
14. Backend collects file artifacts created, updated, or explicitly attached by that Agent run and sends each eligible file as a Weixin file attachment.
15. Backend sends typing status `2` and records an event.
16. If iLink returns session timeout/token expiry, backend clears the stored token, marks the account disconnected, and requires a fresh QR login.
17. Reset increments the in-memory channel generation, cancels active Agent runs, deletes sender-session mappings, and drops stale delayed work so the next inbound message starts a fresh Agent session.

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

## Agent Permissions

Weixin-originated Agent runs use the same full desktop tool surface as normal OtherOne chat.

Included capabilities:

- local file read/write/edit
- file and content search
- web fetch/search
- shell and PowerShell execution
- REPL execution
- Skill and plugin loading

Current file-delivery boundary:

- only files created or edited by the current Agent run are sent back to Weixin;
- existing local files are sent only when the Agent explicitly calls `send_file_to_weixin`;
- arbitrary local paths mentioned in model text are not auto-sent unless they are marked by that tool;
- first file-delivery version sends all eligible outputs as regular Weixin file attachments, not image/video preview messages;
- files larger than 25 MB are skipped and recorded as outbound errors.

Risk:

- any Weixin sender who can message the connected account can indirectly trigger local tools through the model;
- future allowlist or confirmation controls should be added before using this channel for untrusted senders.

## File Attachment Delivery

The Weixin file-send path follows `@tencent-weixin/openclaw-weixin@2.4.6`:

1. Read the local artifact file and compute plaintext size and MD5.
2. Generate a random `filekey` and AES-128 key.
3. Call `/ilink/bot/getuploadurl` with `media_type=3` (`FILE`), plaintext metadata, ciphertext size, `no_need_thumb=true`, and hex AES key.
4. Encrypt the file with AES-128-ECB and PKCS7 padding.
5. POST ciphertext to `upload_full_url`, or to the CDN fallback URL built from `upload_param` and `filekey`.
6. Read `x-encrypted-param` from the CDN response.
7. Send one `sendmessage` request with a single `item_list` entry of type `4` (`FILE`) and `file_item.media`.

Eligibility rules:

- Successful `write_file` / `edit_file` artifacts are automatically eligible for delivery.
- Files created by shell commands, Python scripts, Office tools, or other external processes must be marked with `send_file_to_weixin` before they can be delivered.
- If the user asks to send an existing desktop/local file, the Agent should resolve the path and call `send_file_to_weixin` with the absolute path.
- Delivery sends the text reply first, then sends each file as a separate Weixin file message.

## Dependencies

- `qrcode@^1.5.4`
  - Purpose: render Weixin QR payloads locally when iLink returns non-image content or when an external QR image cannot be displayed by the Tauri WebView.
  - Rationale: QR generation is a correctness-heavy standard with encoding and error correction; a mature library is safer than a custom implementation.
- `@types/qrcode@^1.5.6`
  - Purpose: TypeScript type coverage for the frontend wrapper.
- `aes = "0.8"` and `ecb = "0.1"`
  - Purpose: implement Weixin CDN AES-128-ECB/PKCS7 file encryption.
  - Rationale: CDN media encryption must match the official protocol; use maintained RustCrypto crates instead of custom crypto primitives.
- `getrandom = "0.2"`
  - Purpose: generate file upload `filekey` and AES keys.
  - Rationale: protocol keys require OS-backed randomness.
- `md5 = "0.7"`
  - Purpose: compute `rawfilemd5` required by `getuploadurl`.
  - Rationale: this is a Weixin protocol checksum field, not a security boundary.

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
- [x] Add per-message Agent session creation.
- [x] Add external channel Agent invocation helper.
- [x] Process each inbound Weixin text immediately with its own `context_token`.
- [x] Add sendtyping before/after Agent runs.
- [x] Add text sendmessage with SDK-compatible fields.
- [x] Use the normal full desktop tool surface for Weixin Agent runs.
- [x] Record current-run `write_file` and `edit_file` artifacts.
- [x] Add `send_file_to_weixin` for existing local files and externally generated files.
- [x] Send current-run generated or edited files as Weixin file attachments after the text reply.
- [x] Generate a unique `msg.client_id` for every outbound reply; repeated IDs can return HTTP 200 with `{}` while Weixin silently drops the message.
- [x] Add basic runtime events for UI diagnostics.
- [x] Add reset flow for clearing token, polling cursor, stale local work, active Agent runs, and sender session mapping before reconnecting.
- [x] Persist polling cursor only after fetched messages are accepted locally.
- [x] Retry outbound Weixin text sends before recording delivery failure.
- [x] Avoid debounce/batching so replies follow the upstream sample's one-message/one-token flow.
- [x] Detect nested iLink business errors before marking outbound replies as sent.
- [x] Stop stale-token polling on iLink session timeout and return to QR login.
- [x] Add Weixin Agent run timeout to prevent permanent typing state.
- [x] Refresh the Weixin page layout and remove the recent-message and safety-boundary cards.
- [x] Simplify the Weixin page into one primary next-step action plus contextual reset.
- [x] Rotate stale Agent sessions when old localfile history has invalid tool-call ordering.
- [x] Verify frontend build and Rust check.

## Later TODO

- [ ] Move `bot_token` from SQLite plaintext to platform secure storage.
- [ ] Support multiple Weixin accounts.
- [ ] Add allowlist/owner controls for who can talk to the Agent.
- [ ] Add explicit confirmation flow for sensitive tools.
- [ ] Support message splitting for long replies.
- [ ] Support image input and image output.
- [ ] Support voice transcription and voice replies.
- [ ] Support inbound Weixin file download and model ingestion.
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
