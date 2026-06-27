# Session Storage

## Goal

Load local `otherone-agent` sessions when the desktop app starts, and create a session only when the user actually sends the first message of a new conversation.

## Framework Behavior

`otherone = "0.1.2"` localfile storage uses:

```text
<configured_localfile_root>/.otherone/storage/otherone-storage.json
```

The published crate does not expose a configurable localfile root. The framework data model is:

- `StorageFile`
  - `sessions: Vec<StorageSession>`
- `StorageSession`
  - `session_id`
  - `status`
  - `create_at`
  - `entries`
  - `compacted_entries`
- `Entry`
  - `entry_id`
  - `session_id`
  - `role`
  - `content`
  - `tools`
  - `token_consumption`
  - `status`
  - `create_at`
  - `is_compaction`

`invoke_agent_stream` writes the user entry before loading context and calling the model. `write_entry_to_file` creates the session automatically when that `session_id` is not found.

## Proposed App API

### `load_sessions`

Returns sidebar/session list data.

Derived fields:

- `id`: framework `session_id`
- `title`: first user entry content, trimmed and truncated
- `createdAt`: framework `create_at`
- `updatedAt`: latest entry `create_at`, falling back to session `create_at`
- `lastMessage`: latest entry content preview
- `messageCount`: number of active entries

### `read_session`

Returns a single session detail.

Mapping:

- `role == "user"` -> user message group
- `role == "assistant"` or `role == "ai"` -> AI text item
- `role == "tool"` -> tool item using `tools` JSON when available
- `compacted_entries` must not be returned as visible message UI data. It is only used by the backend when composing context for a send/retry request.

### `update_session_title`

Stores the editable title in SQLite `session_metadata`.

The framework localfile session record is not modified.

## New Conversation Lifecycle

1. User clicks `新对话`.
2. Frontend enters a draft state with no `sessionId`.
3. User sends the first prompt.
4. Frontend sends a session id with the first real prompt, or backend generates one when it is absent.
5. Backend starts `Otherone::invoke_agent_stream`.
6. Framework writes the first user entry and creates the session in localfile.
7. Frontend receives stream events and binds the new `sessionId` to the active conversation.

Do not call `Otherone::create_new_session()` on `新对话`, because that creates empty sessions.

## Recommended Storage Path Strategy

Decision: use the configured dialogue data directory as the active storage root, then call `Otherone::set_localfile_root(dialogueRoot)` before framework localfile access.

Reason:

- It keeps localfile data beside `otherone.sqlite`.
- It avoids writing production user data into the install directory or development workspace.
- It preserves compatibility with the published framework without adding a local path dependency.

Implementation options:

- Preferred: serialize framework localfile access through a backend adapter that sets the framework localfile root while holding a process-wide mutex.
- Legacy fallback: old framework versions required temporarily setting process `current_dir`.

The preferred adapter is safer because it avoids changing process `current_dir`, which can affect relative file behavior in unrelated code.

Current implementation: session read commands use the preferred adapter for synchronous localfile reads. The adapter serializes access with a process-wide mutex, sets `Otherone::set_localfile_root(dialogueRoot)`, then calls the `otherone` API.

The active framework path is:

```text
{dialogueRoot}/.otherone/storage/otherone-storage.json
```

## Storage Migration

App settings store three roots:

- `dataRoot`: app-owned SQLite and general app data.
- `artifactRoot`: generated files and run artifacts.
- `dialogueRoot`: all `otherone-agent` localfile conversation data.

Migration behavior:

1. Block migration while a chat stream is active.
2. Copy managed data to the new roots.
3. Verify SQLite and localfile JSON after copy.
4. Save the new paths only after verification succeeds.
5. Preserve old managed data; users can manually delete old directories after verifying the new paths work.

The frontend must warn users that migration copies data and switches paths, while old data is retained for manual cleanup after verification.

## Real Chat Stream API

`send_chat_message` starts a backend stream and emits `chat_stream_event` payloads:

- `user_entry`
- `assistant_delta`
- `tool_calls`
- `complete`
- `error`

The frontend appends a user message and a running AI text item immediately, streams `assistant_delta` into that item, then reloads the session from localfile on `complete`.

Backend stream mapping extracts visible text from common chunk delta fields. Answer text is emitted as `assistant_delta`; thinking text fields such as `reasoning_content`, `reasoningContent`, `reasoning`, `thinking`, and `thought` are emitted as `assistant_thinking_delta`. Empty chunk events are ignored.

The frontend keeps a small pending stream-event buffer per session. If a chunk arrives before React has committed the newly created session state, the event is replayed once that session becomes active. Stream events are targeted to the Tauri `main` window first, then fall back to a global emit if needed.

## Encryption

`otherone-storage` includes `otherone::storage::encrypt::Encryptor`, backed by AES-256-GCM.

Current framework state:

- `Encryptor::generate_key()` creates a base64 256-bit key.
- `Encryptor::encrypt()` returns `base64_nonce.base64_ciphertext`.
- `Encryptor::decrypt()` restores plaintext.
- The default `localfile::reader` and `localfile::writer` do not call this encryptor.
- Therefore `otherone-storage.json` is plaintext unless the app adds an encrypted adapter or the framework changes.

Decision: reuse the framework encryptor where possible. If the published `otherone = "0.1.2"` path cannot be transparently encrypted without changing the framework, the app should add its own localfile encryption wrapper at the adapter boundary.

## Session Metadata

Decision: store editable session metadata in SQLite.

- Initial title is derived from the first user message.
- Frontend can edit a session title by double-clicking the title in the session list.
- SQLite metadata should remain app-owned and should reference framework `session_id`.

Implemented table:

- `session_metadata(session_id, title, pinned, archived, created_at, updated_at)`

## File Artifacts

Decision: store app-owned file artifact metadata in SQLite, scoped by framework `session_id`.

Implemented table:

- `file_artifacts(id, session_id, action, tool_name, file_path, file_name, extension, patch_json, created_at)`

Current behavior:

- `edit_file` is wrapped at runtime for each chat session; the Agent-visible tool schema and tool list stay unchanged.
- Successful `edit_file` results create or update one `edited` artifact per `(session_id, action, file_path)`.
- The frontend queries artifacts through `list_file_artifacts(session_id)` and listens for `file_artifact_event`.
- Added and deleted file artifacts are not implemented yet.

## Failed First Send

Decision: keep the user-only session if a model request fails after the first user entry is written.

Reason: the later UI can support retry/resend from that prompt.

## Compacted Context

Decision: compressed summaries in `compacted_entries` are hidden from the session message panel.

Reason: they are internal context recovery data for outbound message composition, not conversation content the user should read.

The `read_session` API maps active `entries` into visible messages. In `otherone = "0.1.2"`, normal user and assistant entries written by localfile storage may have `is_compaction: 1`; compacted summaries are kept in `compacted_entries`. Therefore the app must not hide visible messages by filtering `entries` with `is_compaction == 0`.

A later send-message API may read `compacted_entries` internally when rebuilding context before invoking `otherone-agent`.

## Decisions Needed

No open decisions for the first session read API.

## Risks

- Framework localfile root is configurable in `otherone = 0.1.2`.
- The storage file is one JSON document, so concurrent writes must be serialized.
- Session list title and updated time are derived until app-owned SQLite metadata exists.
- Default framework localfile storage is plaintext. Encryption must be added at the app adapter boundary unless the framework is changed.

## Rollback

Session read support is additive. If the path strategy is wrong, remove the Tauri session commands and keep existing API model config storage unchanged.
