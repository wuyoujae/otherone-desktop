# Desktop App Shell

## Scope
- Frontend desktop app lives in `app/frontend`.
- The initial screen is a React rebuild of `resources/propertypes/index.html`.
- Tauri shell config lives in `app/frontend/src-tauri`.

## Tech Path
- Vite + React + TypeScript for the UI.
- Tauri v2 for the desktop window shell.
- `lucide-react` replaces the prototype CDN script so icons render without external runtime scripts.

## Entry Points
- Web dev server: run `npm run dev` from `app/frontend`, then open `http://127.0.0.1:1420/`.
- Production web build: run `npm run build` from `app/frontend`.
- Tauri commands: run `npm run tauri -- <command>` from `app/frontend`.

## Prototype Coverage
- Sidebar navigation, new chat view, chat history view, settings view, dark/light theme, and input send-button state are implemented.
- Initial 1280x800 visual comparison against the prototype is within SVG/antialiasing tolerance.

## Verification
- `npm run build`
- `cargo check` from `app/frontend/src-tauri`
