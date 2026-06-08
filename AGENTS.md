## Behavioral Guidelines

### 1. Think Before Coding

**Don't assume. Don't hide confusion. Surface tradeoffs.**

Before implementing:

- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them – don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

### 2. Simplicity First

**Minimum code that solves the problem. Nothing speculative.**

- No features beyond what was asked.
- No abstractions for single‑use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

Ask yourself: "Would a senior engineer say this is overcomplicated?" If yes, simplify.

### 3. Surgical Changes

**Touch only what you must. Clean up only your own mess.**

When editing existing code:

- Don't "improve" adjacent code, comments, or formatting.
- Don't refactor things that aren't broken.
- Match existing style, even if you'd do it differently.
- If you notice unrelated dead code, mention it – don't delete it.

When your changes create orphans:

- Remove imports/variables/functions that YOUR changes made unused.
- Don't remove pre‑existing dead code unless asked.

The test: Every changed line should trace directly to the user's request.

### 4. Goal‑Driven Execution (Tiered TODO)

Maintain `TODO.md` files as part of the workflow, but adapt to task size.

#### Small tasks (e.g., fix button label, CSS bug, rename field, single‑line logic)

- No need to create or update `TODO.md`.
- Change directly, commit with a clear message.

#### Medium / Large features (new page, API, module refactor, multi‑file changes)

- Create or update `TODO.md` in the module’s directory.
- Include: actionable checklist, brief solution approach, key decisions.
- Keep it concise – no need for function‑level design.

#### Cross‑module / High‑risk changes (core architecture, database migration, breaking API, complex multi‑module features)

- Create a detailed `implementation plan` (in `TODO.md` or a separate `design.md`).
- Must include: complete path, technical approach, user decisions, rationale, risks, rollback strategy.
- Obtain user confirmation before coding.

#### General

- Before starting, check if a relevant `TODO.md` exists. If yes and it covers the task, follow it. If not and the task is medium/large or high‑risk, create one.
- Purpose: prevent context loss without over‑engineering small tasks.
- Success criteria must match task size.

---

## Documentation

### Purpose of Documentation

Documentation exists to **bridge the gap between high‑level understanding and code**, and to **reduce the need to keep large amounts of context in working memory**. It should be:

- **Concise** – no fluff, no lengthy prose.
- **Accurate** – must not contradict the actual code. If ambiguity or conflict arises, the code is the source of truth; update the doc to match.
- **Focused on solutions, implementation paths, and key decisions** – not a line‑by‑line code duplicate.

Avoid writing documentation that simply repeats what the code already says. Instead, explain the *why*, the *trade‑offs*, and the *overview*.

### Documentation Maintenance

- Organise docs by **business model**. Each model may include database, page, backend, frontend, etc.
- Name documents clearly based on the feature.
- Keep docs up to date. If a conflict with the current feature arises, ask the user to confirm before updating.
- **After completing each development task**, review whether the docs need updating. If something is missing, create a new doc.

### Documentation Reading

- Use **progressive disclosure**. Only read docs relevant to the current feature.
- Search by keywords or read the document with the most matching name.
- Before developing a feature, check existing docs for relevant knowledge.

### `PROJECT.md`

- **Purpose**: Acts as the **PRD (Product Requirements Document)** of the entire project. It provides a high‑level overview, goals, scope, architecture, and key decisions.
- **When to write/update**: AI should **write or update `PROJECT.md` when completing a core requirement or a major module** – not for every tiny change.
- **Location**: `/docs/PROJECT.md`
- **Content** (example):
  - Project name and description
  - Main goals and non‑goals
  - High‑level architecture (components, data flow)
  - Technology stack
  - Key user workflows
  - Any important constraints or assumptions

---

## Project Directory Structure

The project root contains three core subdirectories:

- **`app/`** – All development code. Can be further organised (e.g., `frontend`, `backend`, `common`).
- **`resources/`** – All project assets and materials, including prototypes (formerly `propertypes`), images, icons, design files, etc.
- **`docs/`** – All project documentation, including model documents, technical decision records, and development规范 files (e.g., `frontend.md`, `backend.md`, etc.).

---

## Response Requirements

- After completing each task, you **must** say:

  > 我的任务完成了杰哥！还有什么指示！

- All responses and explanations must be in **Chinese**. Code and comments may be in English.

---

## Must Require (Safety)

**Never** use destructive recursive deletion commands:

- `del /s`
- `rd /s`
- `rmdir /s`
- `Remove-Item -Recurse`
- `rm -rf`

When deleting files, delete **only one explicitly specified file at a time**.

**Correct example (PowerShell):**

```powershell
Remove-Item "C:\path\to\file.txt"
```

**If you need to delete multiple files, stop and ask the user to do it manually.**

# PROJECT

每次开始项目前你都应该要保证要去阅读docs目录中的PROJECT.md，那个是关于我们项目的所有核心信息

# 开发规范

在你进行开发的时候，你必须要先阅读我们docs目录中的开发规范，前端规范是FRONTEND.md，后端规范是BACKEND.md。在你开发到对应的任务的时候，例如你只开发到前端的时候，就只去阅读FRONTEND.md，非必要不需要阅读完整的信息，数据库设计要看DATABASE.md