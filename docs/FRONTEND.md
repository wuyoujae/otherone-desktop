# Complete Frontend Development Standards (Core Generic)

## Table of Contents
1. Development Standards (Project Structure, Routing, i18n, UI/UX, State Management, etc.)
2. Security Standards (Generic Principles)
3. Dependency Management Standards (Generic Principles)

---

## Part 1: Development Standards (Generic Behavior Principles)

### 1. Project Structure & Existing Conventions
- Understand the project’s directory organization, file naming rules, component layering, styling approach, request encapsulation, routing scheme, and state management pattern.
- New code must follow the existing structure. Do not introduce parallel architectures or arbitrarily move/refactor existing files.
- If structural changes are needed, propose the plan and obtain confirmation first.

### 2. Routing, Entry Points & Visibility
- New pages must have routes configured to be accessible.
- New components must be integrated into actual pages so users can see them.
- For multi‑entry projects (admin dashboards, mini‑program pages, plugin popup/options/background, desktop windows/menus/trays), clearly define how each entry is accessed.
- After completion, provide clear access paths or usage instructions.

### 3. Internationalization (i18n)
- Never hard‑code user‑visible strings.User-facing production copy must be internationalized. Test fixtures, internal identifiers, and dev-only diagnostics may be excluded.
- Use the project’s existing i18n solution. New strings must be added to the corresponding language resources (at minimum, the base languages, e.g., Chinese and English).
- Key names should be clear and hierarchical (e.g., `user.profile.save`).
- Account for different string lengths across languages to avoid layout breakage.
- Localize dates, times, numbers, currencies, percentages, etc.
- Handle plurals, genders, and placeholders according to language rules.
- If RTL (right‑to‑left) language support is required, reserve compatibility space.
- Error messages, empty states, buttons, and form validation messages must all be internationalized.
- Do not scatter business copy inside component logic.

### 4. Mobile Adaptation
- Mobile layouts must adapt to different screen sizes; ensure touch target size (at least 44px).
- Handle orientation changes, notches, and gesture areas (safe area).
- Scroll container experience: avoid nested scrolling causing jank; manage virtual keyboard obscuring inputs.
- Do not rely on hover states on touch devices; use touch feedback.
- Pay attention to performance on weak networks and low‑end devices.

### 5. Responsive & Adaptive Layout
- Pages must adapt to different screen widths (desktop, tablet, phone), pixel densities, and container sizes.
- Handle long text wrapping and table display on small screens (horizontal scroll or card view).
- Use a grid system or container queries (if supported) with well‑defined breakpoints.
- For resizable desktop windows, content should rearrange appropriately; for fixed small‑size contexts (e.g., plugin popups), ensure content does not overflow.

### 6. UI Consistency
- Reuse the project’s existing design language (spacing, font sizes, border radius, shadows, color variables).
- Reuse existing base components (buttons, dialogs, drawers, toasts, form controls).
- Keep loading, empty, error, and success states visually consistent.
- Do not arbitrarily introduce new visual styles or new design systems.

### 7. Themes, Dark Mode & Design Tokens
- If the project supports light/dark themes, use design tokens (CSS variables) for colors, shadows, and borders.
- Prefer design tokens for colors, spacing, shadows, and borders. Avoid hard-coded values unless required by platform constraints or explicitly defined as design constants.
- Ensure images and icons remain visible in dark mode.
- When theme changes, all components (including wrapped third‑party ones) must refresh correctly.
- For desktop or mobile apps that follow the system theme, listen for system changes and adapt accordingly.

### 8. State Management
- Distinguish between local state (component‑internal), page state, global state, and server‑state.
- Do not put temporary form data or page‑only state into global stores.
- Reuse existing stores, contexts, or query caches; avoid creating duplicate global stores.
- Asynchronous states (requests) must have explicit indicators: loading, error, empty, success.
- Form state and server‑returned data state should be managed separately.
- Shared state across multiple pages must consider invalidation and synchronisation strategies.
- For multi‑window, multi‑tab, or plugin contexts (e.g., background vs popup), define clear communication boundaries and synchronisation mechanisms.

### 9. Data Requests & API Integration
- Use a unified request client (with built‑in interceptors, authentication, error handling, loading indicators).
- Do not scatter raw request calls (e.g., direct `fetch`/`axios`).
- Handle timeouts, cancellation (AbortController), and retry strategies.
- Handle concurrent requests (e.g., request deduplication) and duplicate submissions (debounce/throttle/token).
- Implement common patterns for pagination, filtering, and sorting.
- Handle missing or `null` fields in API responses to avoid runtime crashes.
- Handle API version differences (e.g., compatibility of old/new fields).
- Support switching between mock data and real endpoints.

### 10. Form Development
- Provide validation rules with clear error messages.
- Distinguish required, read‑only, and disabled states.
- Handle default values and reset logic.
- Show submitting state on submit and prevent duplicate submission.
- Format inputs (e.g., phone numbers, amounts) and enforce length limits.
- On mobile, show appropriate keyboards (numeric, text, email).
- Support browser/system autocomplete.
- If unsaved data exists, prompt for confirmation before leaving.
- Multi‑step forms must retain entered data.

### 11. Error, Empty & Loading States
- Provide loading, skeleton, empty, error, and retry entry for asynchronous operations.
- Handle timeout, permission denied, offline, and partial success states.
- Show hints on weak network; rollback optimistic updates on failure and notify the user.
- All state presentations should be visually consistent.

### 12. Interaction Feedback
- Provide clear feedback for clickable elements (hover, active, focus, touch feedback).
- Use toast, message, confirm components to deliver operation results.
- Dangerous actions (deletion, irreversible changes) must have a confirmation dialog.
- Prefer reversible actions (e.g., “Undo”) over irreversible ones.
- Show progress or loading for long‑running operations.
- If the project uses keyboard shortcuts, document them or provide hints.

### 13. Animations & Transitions
- Use animations purposefully (direct attention, feedback, smooth transitions); avoid decorative animations.
- Provide smooth transitions for page navigation, modal open/close, and list insert/delete.
- Prefer performant properties (transform, opacity) to avoid layout thrashing.
- Respect `prefers-reduced-motion` media query to reduce or disable animations.

### 14. Accessibility (a11y)
- Use semantic HTML tags (or equivalent custom components).
- Ensure keyboard operability (Tab focus, Enter/Space activation).
- Focus management: trap focus inside modals/drawers; restore on close.
- Provide appropriate ARIA attributes (`aria-label`, `aria-describedby`, etc.).
- Form controls must be associated with `<label>`.
- Images must have `alt` attributes (empty for decorative images).
- Foreground/background contrast must meet WCAG requirements (at least 4.5:1).
- Do not rely solely on color to convey state (e.g., use text in addition to red color).
- Error messages must be perceivable by screen readers (e.g., `role="alert"`).
- Animations should have a disable option.

### 15. Performance
- First paint: reduce blocking resources, code‑split at route or component level.
- Bundle size control: analyse bundle, remove dead code, lazy load non‑critical modules.
- Image optimisation: compress, use modern formats (webp/avif), lazy load.
- Long lists: use virtual scrolling (render only visible area).
- Avoid unnecessary re‑renders (use memo, dependency comparison wisely).
- Avoid long main‑thread tasks (break up work, use Web Workers for heavy computation).
- Debounce or throttle high‑frequency events (scroll, input).
- Use appropriate caching strategies (HTTP cache, server cache, local cache).
- Optimise specifically for low‑end mobile devices, mini‑program size limits, plugin popup open speed, and desktop startup time/memory usage.

### 16. Resource Handling
- Choose appropriate image formats (SVG for icons, JPEG/PNG for photos).
- Avoid importing entire icon libraries. Use tree-shakable icon packages, project-approved icon sets, SVG sprites, or component-based icons.
- Lazy load font files; use system fonts as fallback.
- Load video/audio on demand, provide poster images.
- Serve static assets via CDN with sensible cache policies.
- Support base path deployment (e.g., under `/app/`).
- For offline resources (PWA), handle cache versioning correctly.
- Provide 2x/3x images for high‑DPI screens, or use vector formats.
- In dark mode, resources may need to be swapped (e.g., SVG colour, image variants).
- Organise language‑specific images by locale.

### 17. Compatibility
- Define the target browser/runtime versions (e.g., Chrome version, iOS version, Node version).
- Use feature detection instead of user agent sniffing.
- Load polyfills only for unsupported APIs (on demand).
- Be aware of WebView differences (especially Android vs iOS).
- Desktop: account for system behaviour differences (file paths, shortcuts) on Windows/macOS/Linux.
- Mini‑programs: different platform APIs (WeChat, Alipay, etc.); use a unified adapter layer.
- Browser extensions: Manifest V2/V3 differences; develop for the target version.
- SSR/CSR/SSG differences: ensure components do not access browser‑only APIs during server rendering.
- Touch vs mouse: support both `click` and touch events; avoid hover dependency.
- High‑DPI scaling: test layout under different zoom levels.
- System font differences: use fallback font families.

### 18. Multi‑Platform Differences
Projects may run on many environments: web pages, H5, PWA, desktop apps, hybrid apps, React Native/Flutter, mini‑programs, browser extensions, embedded WebViews. Each platform has different capabilities, permissions, lifecycles, routing, storage, network behaviour, and review requirements.
- Do not assume an implementation for one platform works directly on another.
- Extract core business logic into platform‑agnostic modules, and adapt via platform adapters.
- Handle each platform’s specific entry points, lifecycle, and storage limits separately.

### 19. Offline, Weak Network & Recovery
- Detect network status (online/offline); show hints when offline and cache user actions.
- Automatically retry failed requests (configurable retries, backoff strategies).
- When network recovers, automatically refresh data or replay cached actions.
- Cache important content (e.g., drafts) locally to prevent accidental loss.
- Support resumable uploads and recoverable downloads.
- Use Service Workers (or equivalent offline caching) to provide basic offline experience.
- On mobile network transitions (WiFi ↔ cellular), handle request interruptions gracefully.

### 20. Lifecycle & Cleanup
- On component unmount: clean up event listeners, timers, subscriptions, WebSocket/SSE connections.
- Cancel pending requests (AbortController).
- Release object URLs (`revokeObjectURL`).
- Disconnect Observers (IntersectionObserver, MutationObserver, etc.).
- On page visibility change (`visibilitychange`) or mobile app foreground/background switch, pause polling or animations.
- On desktop window close, save necessary state and release resources.
- On plugin popup close, stop background tasks (or migrate to background if needed).
- On mini‑program page lifecycle (`onShow`/`onHide`/`onUnload`), manage resource loading and release.

---

## Part 2: Security Standards (Generic Principles)

### 1. No Sensitive Data on the Client
- Credentials, private identifiers, PII, internal URLs, keys, feature flags, etc., must **never** appear in client code, build artifacts, logs, error reports, URLs, local storage, or caches.
- Any data that reaches the client can potentially be exposed; treat it as such.

### 2. Do Not Directly Execute Untrusted Content
- All external strings (user input, URL parameters, backend data, third‑party content) are untrusted.
- Prohibit direct use for HTML parsing, script execution, style injection, or redirect targets.
- If rich text must be rendered, use a validated sanitisation library and restrict allowed tags/attributes.

### 3. Validate Redirects and Resource Loads
- Any programmatic navigation, link opening, or resource loading must whitelist the target URL.
- Forbid dangerous schemes: `javascript:`, `data:`, `file:`.
- When opening external pages (new window/tab), isolate access (disable `opener` access).

### 4. Do Not Rely on Client‑Side Access Control
- UI elements (buttons, menus, routes) are UX only – **not a security boundary**.
- All sensitive operations must be re‑authorised on the backend.
- Client‑stored roles, permissions, and flags can be tampered with or become stale.

### 5. Session and Credential Security
- Session tokens must use secure transport and never appear in URLs or logs.
- Use the most secure session storage mechanism available for the target platform, such as httpOnly Secure SameSite cookies on Web, secure storage on Mobile/Desktop, or platform-approved session mechanisms.
- On logout or session expiry, actively clear all client‑stored sensitive data and caches.
- All state‑changing requests must carry anti‑CSRF tokens (or rely on same‑origin + secure cookie attributes).

### 6. Minimise Local Storage and Caches
- Client storage (including but not limited to Web Storage, IndexedDB, file system, Keychain, SQLite) **must not** hold sensitive information.
- Caches must have reasonable expiration and be cleared on logout.
- Avoid storing full backend objects; keep only necessary fields.

### 7. Network Request Security
- Enforce TLS (HTTPS/WSS) – no downgrade to plain text.
- Never disable certificate validation in production.
- Do not send sensitive data unnecessarily. Never put sensitive data in URLs. Use approved secure channels, headers, or request bodies only when required by the API contract.
- For idempotent or duplicate‑sensitive endpoints, implement client‑side debouncing or token control.

### 8. Error and Log Sanitisation
- User‑visible errors must not contain stack traces, file paths, SQL, internal service names, etc.
- Console logs, analytics, and error monitoring must not output tokens, passwords, phone numbers, or emails.
- Production source maps must not be publicly accessible. If needed, upload them only to private error-monitoring infrastructure or debug panels.

### 9. File Handling Security
- Uploaded files must be validated by type (MIME + extension consistency) and size; re‑validate on backend.
- Image files should strip EXIF metadata (e.g., GPS coordinates).
- For downloads or previews, sanitise filenames to prevent path traversal or injection.
- Release temporary resources (Blob URLs, local temp files) explicitly after use.

### 10. Isolate Third‑Party Content
- Third‑party scripts, SDKs, and embedded content (ads, support, maps, etc.) are untrusted.
- Restrict their API access and storage scope; do not grant excessive privileges.
- Use Subresource Integrity (SRI) for third‑party resources, or self‑host critical ones.

---

## Part 3: Dependency Management Standards (Generic Principles)

### 1. Decision Process for Adding a Dependency
Before adding any new dependency, answer sequentially:
- Does the project already have similar capability? (avoid duplication)
- Can native APIs suffice? (avoid worthless dependencies)
- Is a lightweight self‑built solution feasible and maintainable?
- Will this feature be frequently changed or customised? (if yes, self‑build is more flexible)
- Is it tightly coupled with core business logic? (if yes, self‑build gives more control)

**Golden rule**: The closer to business, the more customisation needed, the more frequent changes → the less you should rely on third‑party.

### 2. No Duplicate Capabilities
- The project must not have two or more libraries solving the same category (e.g., multiple UI kits, state managers, date utilities).
- Before adding a new library, check if an existing one already serves the purpose.

### 3. Evaluate Library Health
- Maintenance activity (update frequency within the last year)
- Open source license (prefer MIT, Apache 2.0)
- Issue resolution track record
- TypeScript support quality
- Known security vulnerabilities (use audit tools)
- Dependency tree depth (many transitive dependencies can be problematic)

### 4. Runtime Cost Control
- Estimate bundle size impact (use analysis tools)
- Does it support tree shaking? Can it be imported on demand?
- Impact on first‑paint performance or low‑end devices?
- For multi‑platform projects (mini‑programs, extensions), does it exceed platform size limits?

### 5. Encapsulation Boundary and Replaceability
- Do not scatter third‑party API calls across business code.
- Wrap third‑party functionality behind a **facade** or **adapter** to ease future replacement or upgrades.
- Unify error handling, logging, and analytics inside the wrapper.

### 6. Dependency Locking and Consistency
- The team must use a single package manager (npm / yarn / pnpm / bun) consistently.
- Use lockfiles to pin all dependency versions; never edit lockfiles manually.
- Run security audits regularly (monthly); perform minor version upgrades only when necessary.

### 7. Boundaries of Self‑Built Solutions

**Prefer self‑built** for:
- Simple UI components (buttons, modals, tabs, etc.)
- General utilities (deep clone, throttle, formatting)
- Lightweight state management (pub‑sub, context wrapper)

**Do not self‑build** for:
- Cryptography, hashing, random number generation (security‑critical)
- Complex rich‑text editors
- Complex chart engines
- High‑difficulty virtual scrolling or drag‑and‑drop
- Accessibility primitive components (must follow standards)

**Judgment rule**: High complexity, high correctness requirements, high standardisation → mature dependency; High business customisation, low complexity → self‑built.

---

## Enforcement
- All code must pass static checks (ESLint / Prettier, etc., as configured per project) before commit.
- Pull Requests must include a self‑check against Security (Part 2) and Dependency (Part 3).
- Any new third‑party dependency must be documented with purpose, version, rationale, and alternatives considered.
- Run security audits and dependency updates regularly (monthly).