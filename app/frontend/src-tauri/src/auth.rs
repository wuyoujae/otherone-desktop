use rusqlite::Connection;

pub(crate) fn init_auth_database(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY,
            email TEXT NOT NULL COLLATE NOCASE,
            password_hash TEXT,
            display_name TEXT NOT NULL DEFAULT '',
            avatar_url TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'pending_verification'
                CHECK (status IN ('pending_verification', 'active', 'disabled')),
            email_verified_at TEXT,
            last_login_at TEXT,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE UNIQUE INDEX IF NOT EXISTS idx_users_email_unique
            ON users(email COLLATE NOCASE);
        CREATE INDEX IF NOT EXISTS idx_users_status_created
            ON users(status, created_at DESC);

        CREATE TABLE IF NOT EXISTS user_auth_identities (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            provider TEXT NOT NULL,
            provider_user_id TEXT NOT NULL,
            provider_email TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
        );

        CREATE UNIQUE INDEX IF NOT EXISTS idx_user_auth_identities_provider_user
            ON user_auth_identities(provider, provider_user_id);
        CREATE INDEX IF NOT EXISTS idx_user_auth_identities_user
            ON user_auth_identities(user_id);

        CREATE TABLE IF NOT EXISTS email_verification_codes (
            id TEXT PRIMARY KEY,
            email TEXT NOT NULL COLLATE NOCASE,
            code_hash TEXT NOT NULL,
            purpose TEXT NOT NULL CHECK (purpose IN ('registration', 'login')),
            expires_at TEXT NOT NULL,
            consumed_at TEXT,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE INDEX IF NOT EXISTS idx_email_verification_codes_lookup
            ON email_verification_codes(email COLLATE NOCASE, purpose, consumed_at, expires_at);
        ",
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_auth_database_creates_auth_tables() {
        let conn = Connection::open_in_memory().unwrap();

        init_auth_database(&conn).unwrap();

        let table_count: i64 = conn
            .query_row(
                "SELECT COUNT(*)
                 FROM sqlite_master
                 WHERE type = 'table'
                   AND name IN ('users', 'user_auth_identities', 'email_verification_codes')",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(table_count, 3);
    }

    #[test]
    fn user_email_uniqueness_is_case_insensitive() {
        let conn = Connection::open_in_memory().unwrap();
        init_auth_database(&conn).unwrap();

        conn.execute(
            "INSERT INTO users (id, email, password_hash, status)
             VALUES ('user-1', 'Owner@Example.com', 'hash', 'active')",
            [],
        )
        .unwrap();

        let duplicate_result = conn.execute(
            "INSERT INTO users (id, email, password_hash, status)
             VALUES ('user-2', 'owner@example.com', 'hash', 'active')",
            [],
        );

        assert!(duplicate_result.is_err());
    }

    #[test]
    fn auth_identity_is_unique_per_provider_user() {
        let conn = Connection::open_in_memory().unwrap();
        init_auth_database(&conn).unwrap();

        conn.execute(
            "INSERT INTO users (id, email, password_hash, status)
             VALUES ('user-1', 'owner@example.com', 'hash', 'active')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO user_auth_identities (id, user_id, provider, provider_user_id)
             VALUES ('identity-1', 'user-1', 'github', 'provider-user-1')",
            [],
        )
        .unwrap();

        let duplicate_result = conn.execute(
            "INSERT INTO user_auth_identities (id, user_id, provider, provider_user_id)
             VALUES ('identity-2', 'user-1', 'github', 'provider-user-1')",
            [],
        );

        assert!(duplicate_result.is_err());
    }
}
