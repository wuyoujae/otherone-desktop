# Skill Import

## Scope

- The desktop app supports importing a local Skill directory or remote `SKILL.md` URL from the Skill tab.
- The selected directory must contain `SKILL.md` at its root.
- The full directory is copied into `dataRoot/skills/imported/<skill-name>`.
- A URL import saves the downloaded text as `dataRoot/skills/imported/<skill-name>/SKILL.md`.
- Imported skills are installed through the existing `plugin_installs` table with `kind = 'skill'`.

## Required Structure

```text
skill-name/
  SKILL.md
  scripts/
  references/
  assets/
```

Only `SKILL.md` is required. Supporting folders are copied if present.

`SKILL.md` must include frontmatter:

```markdown
---
name: my-skill
description: Use when ...
---
```

Rules:

- `name` must be 1-64 characters.
- `name` may contain lowercase letters, numbers, and hyphens.
- `description` is required and must be 1024 characters or fewer.
- Symlinks are rejected during import.

## First-Version Limits

- URL import supports a direct `SKILL.md` text file only.
- GitHub `blob/.../SKILL.md` URLs are converted to raw GitHub URLs.
- Zip import is not implemented.
- Duplicate skill names are rejected, including names already used by bundled skills.
- Import copies files only; it does not execute scripts.
