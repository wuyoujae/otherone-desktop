# PROJECT.md

该项目是基于 otherone-agent 框架搭建的 AGENT 智能体应用。当前已有桌面端形态，前端使用 Tauri + React，后端使用 Rust，基于 otherone-agent 进行开发，数据库使用 SQLite + localfile。

当前前端已加入 Web/Desktop 平台适配层：桌面端继续通过 Tauri IPC 调用 Rust 后端；Web 运行时可通过 `VITE_OTHERONE_WEB_API_BASE_URL` 接入 HTTP/SSE 后端。未配置 Web API 时，浏览器仅作为安全预览环境，不提供完整 Agent、SQLite/localfile 和本地系统能力。

## 双端开发手册

### 基本原则
- 所有新功能默认必须同时考虑 Desktop 和 Web 两种运行时，不能只在 Tauri 命令里实现业务逻辑。
- 业务逻辑必须放在可复用的 backend core 或前端平台无关模块里，Desktop/Web 只做传输层适配。
- React 组件不能直接调用 `@tauri-apps/*` 或裸 `fetch`，必须通过 `src/services/*` 服务门面。
- 服务门面必须保持 Desktop/Web 行为语义一致：相同入参、相同返回结构、相同错误含义。
- 平台能力差异必须显式建模，不能用空实现掩盖。例如目录选择、打开系统文件夹、桌面通知属于 Desktop-only；Web 必须有 Web 等价交互或明确禁用状态。

### 前端开发规则
- 组件层只关心业务动作，不关心当前运行环境。
- Desktop 调用放在 `src/services/platform/tauri.ts` 及其上层服务门面内。
- Web 调用放在 `src/services/platform/webApi.ts` 及其上层服务门面内。
- Web API 地址通过 `VITE_OTHERONE_WEB_API_BASE_URL` 配置，不能在代码中硬编码。
- 新增服务时必须先定义 TypeScript request/response 类型，再分别接 Desktop IPC 和 Web HTTP/SSE。
- 流式能力在 Desktop 使用 Tauri event，在 Web 使用 SSE；事件 payload 必须保持兼容。

### 后端开发规则
- 业务函数不得直接依赖 Tauri `AppHandle`。需要应用目录、事件发送、通知、数据库路径时，应通过运行时上下文接口注入。
- Desktop backend 是 Tauri IPC 传输层；Web backend 是 HTTP/SSE 传输层；两者必须复用同一套业务函数。
- SQLite schema、localfile 会话数据、产物记录、Workflow、插件和记忆逻辑必须由共享 backend core 维护。
- Web 后端必须进行身份、权限和输入校验；不能把 Desktop 本地信任模型直接搬到公网 Web。
- 文件、Shell、插件、MCP、微信 ClawBot 等高权限能力在 Web 端必须默认受限，并通过权限模型明确放行。

### 数据和迁移规则
- 新增表和字段必须同时评估 Desktop 本地 SQLite 与 Web 服务端 SQLite/后续云数据库的兼容性。
- 不允许把 Desktop-only 路径写入 Web API 合同。
- Web 用户数据必须具备用户隔离字段或等价隔离机制；Desktop 单用户数据不能直接作为 Web 多用户模型。
- localfile、memory、artifact 的根目录必须由运行时上下文决定，不能散落在业务代码里。

### 验证规则
- 每次涉及业务服务的改动，至少验证 `npm run build`。
- 涉及 Rust backend core、Tauri IPC 或 Web API 时，必须验证对应 Rust check/build。
- 新增 Web API 时必须补充请求路径、请求体、响应体、错误语义和 Desktop 对应命令。
- 完成任务后检查文档是否需要更新，核心模块变化必须更新本文件或对应 docs 文档。

## Web 全量迁移计划

Web 全量迁移按 `docs/WEB_FULL_MIGRATION_PLAN.md` 执行。任何涉及共享 backend core、Web API、权限模型、文件系统、Shell、插件、MCP 或微信 ClawBot 的改动，都必须先检查该计划，并保持 Desktop 与 Web 的行为边界一致。

otherone-agent框架的代码在C:\Users\jae\Desktop\OmniBuild\otherone\otherone-agent

前端代码的样式必须要100%按照/resource/propertypes中的原型图来复现和开发

当前后端已接入 crates.io 发布的 `otherone = "0.1.2"`。API 模型测试使用 Tauri 命令调用 `otherone::ai::invoke_model_stream`，以首个流式 chunk 的返回时间作为连接测试响应时间；会话 localfile 读取与写入通过框架的 `Otherone::set_localfile_root` 指定对话数据目录，不再切换进程 current_dir。

## 已知问题排查记录

- [[TAURI_EVENT_DEBUG]] — 流式对话前端无响应问题排查（2026-06-08）
  - 根因1：`emit_to("main")` 与前端全局 `listen()` 跨频道不匹配
  - 根因2：`capabilities/default.json` 缺少 `core:event:default` 权限，`listen()` IPC 被 Tauri v2 ACL 静默拦截
  - 确认 `otherone-agent` 框架无问题，两个 bug 均在桌面应用集成层
