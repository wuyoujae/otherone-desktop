# Authentication

## Scope

First version authentication uses local SQLite and focuses on email/password registration with email verification. Third-party login is not implemented yet, but the schema keeps identity bindings provider-agnostic.

## Decisions

- Auth data is stored in the existing `otherone.sqlite` database under the configured data root.
- Email registration requires email verification before the user becomes active.
- Passwords are stored only as password hashes. The hash string should include algorithm parameters and salt when registration logic is added.
- OAuth provider tokens are not stored in the first schema.

## Tables

### `users`

| Column | Type | Notes |
| --- | --- | --- |
| `id` | `TEXT PRIMARY KEY` | App-generated stable user id. |
| `email` | `TEXT NOT NULL COLLATE NOCASE` | Login email. Unique case-insensitively. |
| `password_hash` | `TEXT` | Nullable so future external-only identities can exist. |
| `display_name` | `TEXT NOT NULL DEFAULT ''` | User-visible name. |
| `avatar_url` | `TEXT NOT NULL DEFAULT ''` | Optional profile image URL/path. |
| `status` | `TEXT NOT NULL DEFAULT 'pending_verification'` | `pending_verification`, `active`, or `disabled`. |
| `email_verified_at` | `TEXT` | Verification timestamp. |
| `last_login_at` | `TEXT` | Last successful login timestamp. |
| `created_at` | `TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP` | Creation time. |
| `updated_at` | `TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP` | Last update time. |

Indexes:
- `idx_users_email_unique` unique on `email COLLATE NOCASE`.
- `idx_users_status_created` on `(status, created_at DESC)`.

### `user_auth_identities`

| Column | Type | Notes |
| --- | --- | --- |
| `id` | `TEXT PRIMARY KEY` | App-generated identity row id. |
| `user_id` | `TEXT NOT NULL` | References `users(id)`. |
| `provider` | `TEXT NOT NULL` | Provider key such as future `github`, `google`, or `x`. |
| `provider_user_id` | `TEXT NOT NULL` | Stable user id from the provider. |
| `provider_email` | `TEXT NOT NULL DEFAULT ''` | Provider-reported email if available. |
| `created_at` | `TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP` | Binding creation time. |
| `updated_at` | `TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP` | Binding update time. |

Indexes:
- `idx_user_auth_identities_provider_user` unique on `(provider, provider_user_id)`.
- `idx_user_auth_identities_user` on `user_id`.

### `email_verification_codes`

| Column | Type | Notes |
| --- | --- | --- |
| `id` | `TEXT PRIMARY KEY` | App-generated code row id. |
| `email` | `TEXT NOT NULL COLLATE NOCASE` | Target email. |
| `code_hash` | `TEXT NOT NULL` | Verification code hash, not plaintext. |
| `purpose` | `TEXT NOT NULL` | `registration` or `login`. |
| `expires_at` | `TEXT NOT NULL` | Expiration timestamp. |
| `consumed_at` | `TEXT` | Set after successful use. |
| `created_at` | `TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP` | Code creation time. |

Indexes:
- `idx_email_verification_codes_lookup` on `(email COLLATE NOCASE, purpose, consumed_at, expires_at)`.

## Registration Flow Target

1. User submits email and password.
2. Backend validates email/password and creates or refreshes a `registration` verification code.
3. Backend sends the plaintext code through the selected email channel and stores only `code_hash`.
4. User submits the code.
5. Backend verifies the hash and expiry, creates or activates the `users` row, sets `email_verified_at`, and consumes the code.

Actual commands, email delivery, password hashing dependency, and frontend UI are intentionally left for the next implementation step.
