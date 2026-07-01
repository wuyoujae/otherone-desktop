# Auth Database Foundation

## Approach
- Add a backend-owned auth database initializer for local SQLite.
- Keep this task limited to schema foundation for email/password registration and email verification.
- Keep third-party login database design provider-agnostic without implementing any OAuth provider flow.

## Checklist
- [x] Add user, identity, and email verification tables.
- [x] Register auth database initialization on app startup.
- [x] Document schema and first-version decisions.
- [x] Verify schema SQL and key uniqueness constraints.
- [ ] Verify Rust check after MSVC linker or target-path cache issue is fixed.

## Key Decisions
- Store auth data in the existing `otherone.sqlite` under the configured data root.
- Email registration requires email verification.
- Do not implement registration commands or frontend UI in this step.
