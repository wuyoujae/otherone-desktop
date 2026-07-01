# Message Panel

## Scope
- The chat area uses a message panel model: panel -> message group -> message item.
- The current implementation lives in `app/frontend/src/components/MessagePanel.tsx`.
- Styles live in `app/frontend/src/styles.css`.

## Dependency
- `react-virtuoso@^4.18.10` is declared in `app/frontend/package.json` for long message list virtualization.
- Rationale: virtual scrolling is correctness-sensitive UI infrastructure, and the component already uses `Virtuoso`.
- Alternative considered: a custom windowing implementation; rejected because it would add risk for a single existing use case.

## Message Groups
- `user` messages show edit and copy actions.
- `ai` messages show edit, copy, and create branch actions.
- Nested agent messages reuse the same panel structure without outer action buttons.

## Item Types
- Text item: markdown-like rendered text with paragraphs, lists, headings, inline code, and code blocks.
- Static tool item: icon plus smaller muted text, no expansion.
- Collapsible tool item: static tool header plus chevron and animated expandable content.
- Agent item: collapsible item whose expanded content contains a nested message panel.

## States
- `running`: shimmer text animation plus spinning icon.
- `completed`: muted gray text and icon.

## Current Data
- Mock data is embedded in `MessagePanel.tsx`.
- Replace the mock data with the real message model once backend message streaming is introduced.
