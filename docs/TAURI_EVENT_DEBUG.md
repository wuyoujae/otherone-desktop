# Tauri v2 事件流式传输排查

## 问题现象

发送消息后 Agent 后端正常执行并存储数据到 localfile，但前端看不到任何流式回复。只有切换 session 重新从 localfile 加载后才能看到消息。

## 根因

经过系统诊断（诊断测试位于 `app/frontend/src-tauri/tests/stream_diagnostics.rs`，全部 9 个测试通过），确认：

- ✅ `otherone-agent` 框架的流式事件产出完全正确（后端日志确认收到了 298 个 streaming event）
- ✅ `extract_delta_segment` 对 OpenAI/Anthropic 等各种 raw_chunk 格式的提取逻辑正确
- ❌ **两个 bug 都在桌面应用的集成层**，不在框架

### Bug #1：Tauri v2 事件 API 误用 — `emit_to` vs `emit`

**文件**：`app/frontend/src-tauri/src/chat.rs:449`

**错误代码**：
```rust
fn emit_event(app: &AppHandle, event: ChatStreamEvent) {
    if app.emit_to("main", "chat_stream_event", &event).is_err() {
        let _ = app.emit("chat_stream_event", event);
    }
}
```

**问题分析**：

Tauri 有两层事件系统：
| API | 作用 | 前端接收 |
|---|---|---|
| `app.emit(event, payload)` | 全局广播 | `listen(event, ...)` — 全局监听 |
| `app.emit_to(target, event, payload)` | 窗口定向发送 | `window.listen(event, ...)` — 窗口实例监听 |

后端用 `emit_to("main")` 发**窗口定向事件**，但前端 `chatStorage.ts` 用的是：
```typescript
import { listen } from '@tauri-apps/api/event';
listen<ChatStreamEvent>('chat_stream_event', handler);
```
这是**全局监听**，不同 scope。

而且 `emit_to("main")` 返回了 `Ok`（窗口 "main" 确实存在），所以没走 fallback `emit()`。事件被发送到了窗口 "main" 的专属通道，前端的全局 listener 收不到，**跨频道静默丢失**。

**修复**：
```rust
fn emit_event(app: &AppHandle, event: ChatStreamEvent) {
    let _ = app.emit("chat_stream_event", event);
}
```

### Bug #2：Tauri v2 ACL 权限缺失 — `listen()` 被 IPC 鉴权拦截

**文件**：`app/frontend/src-tauri/capabilities/default.json`

**错误配置**：
```json
{
  "permissions": ["opener:default"]
}
```

**问题分析**：

Tauri v2 引入了 ACL（Access Control List）机制，所有通过 `invoke()` 调用的 IPC 命令（包括插件命令）都需要在 capabilities 中显式授权。

前端 `listen()` 的实现路径（`@tauri-apps/api/event.js:71`）：
```javascript
async function listen(event, handler, options) {
    // ...
    return invoke('plugin:event|listen', { ... });
}
```

它调用的是 `plugin:event|listen`，这是 Tauri 内置 `core` 插件提供的 IPC 命令。**没有 `core:event:default` 权限时，Tauri 的 ACL 会静默拒绝这个 IPC 调用，监听器注册失败。**

关键点：这个拒绝是**静默**的——
- Rust 后端 `app.emit()` 本身不需要 ACL，照常发送
- 后端日志正常 → 给人"后端在正常工作"的错觉
- 但 JS 端 `listen()` 的 Promise 可能被 reject 但没有 catch → 前端安静不动

**修复**：
```json
{
  "permissions": ["core:default", "opener:default"]
}
```

`core:default` 包含了所有 core 插件的基础权限（event、window、path、webview 等）。

## 如何避免类似问题

### 调试技巧

1. **Rust 端加 `eprintln!` 日志**：在 `emit_event` 里打印每次 emit，确认后端确实在发事件
2. **前端加 `console.log`**：在 `listenToChatStream` 注册前后、事件到达时打印日志
3. **检查 console 报错**：如果 `listen()` 因 ACL 被拒，Tauri 可能在 JS console 输出 IPC 拒绝警告

### 开发规范

1. 新增 Tauri API 调用时，确认 `capabilities/default.json` 包含对应权限：
   - `listen()/emit()` → 需要 `core:event:default`
   - `invoke()` 自定义命令 → 需要在 capabilities 中声明命令权限或使用 `core:default`
2. **统一事件发送方式**：使用 `app.emit()`（全局广播），除非明确需要窗口级隔离
3. **前端统一使用 `listen()`**（全局监听），不与 `emit_to` 混用

### 当前权限配置（2026-06-08）

```json
{
  "permissions": ["core:default", "opener:default"]
}
```

若后续引入其他 Tauri 插件（如 fs、dialog、notification），需追加对应权限。

## 文件变更清单

| 文件 | 变更 | 原因 |
|---|---|---|
| `chat.rs` emit_event | `emit_to("main")` → `emit()` | Bug #1 |
| `chat.rs` run_chat_stream | 添加 `eprintln!` 调试日志 | 排查需要 |
| `capabilities/default.json` | 添加 `"core:default"` | Bug #2 |
| `chatStorage.ts` listenToChatStream | 添加 `console.log` 注册/事件到达日志 | 排查需要 |
| `App.tsx` updateStreamingMessageV2 | 添加 `console.log` 事件处理日志 | 排查需要 |

## 相关文档

- Tauri v2 Event System: https://v2.tauri.app/develop/calling-frontend/#event-system
- Tauri v2 Capabilities: https://v2.tauri.app/security/capabilities/
- `otherone-agent` 集成：[[BACKEND_OTHERONE_AGENT]]
