use super::Database;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;

impl Database {
    pub fn open(db_path: &PathBuf) -> anyhow::Result<Self> {
        let conn = Connection::open(db_path)?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_tables()?;
        Ok(db)
    }

    fn init_tables(&self) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        // Drop prototype tables that no longer have product callers. Safe on
        // fresh DBs (IF EXISTS) and cleans long-lived installs once.
        Self::drop_legacy_tables(&conn)?;

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                username TEXT NOT NULL UNIQUE,
                created_at INTEGER NOT NULL,
                disabled INTEGER NOT NULL DEFAULT 0,
                display_name TEXT,
                role TEXT NOT NULL DEFAULT 'user',
                disabled_at INTEGER,
                updated_at INTEGER
            );

            CREATE TABLE IF NOT EXISTS api_keys (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                name TEXT NOT NULL,
                key_hash TEXT NOT NULL UNIQUE,
                key_prefix TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                last_used_at INTEGER,
                revoked_at INTEGER,
                scopes TEXT NOT NULL DEFAULT '',
                expires_at INTEGER,
                kind TEXT NOT NULL DEFAULT 'user',
                allowed_client_id TEXT,
                FOREIGN KEY(user_id) REFERENCES users(id)
            );
            CREATE INDEX IF NOT EXISTS idx_api_keys_hash ON api_keys(key_hash);
            CREATE INDEX IF NOT EXISTS idx_api_keys_user_id ON api_keys(user_id);

            CREATE TABLE IF NOT EXISTS account_credentials (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                credential_hash TEXT NOT NULL UNIQUE,
                credential_prefix TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                last_used_at INTEGER,
                revoked_at INTEGER,
                FOREIGN KEY(user_id) REFERENCES users(id)
            );
            CREATE INDEX IF NOT EXISTS idx_account_credentials_hash ON account_credentials(credential_hash);
            CREATE INDEX IF NOT EXISTS idx_account_credentials_user_id ON account_credentials(user_id);

            CREATE TABLE IF NOT EXISTS pairing_codes (
                id TEXT PRIMARY KEY,
                code_hash TEXT NOT NULL UNIQUE,
                user_id TEXT NOT NULL,
                username TEXT NOT NULL,
                client_id TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                used_at INTEGER,
                user_token_name TEXT,
                agent_token_name TEXT,
                FOREIGN KEY(user_id) REFERENCES users(id)
            );
            CREATE INDEX IF NOT EXISTS idx_pairing_codes_hash ON pairing_codes(code_hash);
            CREATE INDEX IF NOT EXISTS idx_pairing_codes_expires_at ON pairing_codes(expires_at);

            CREATE TABLE IF NOT EXISTS action_sessions (
                session_id TEXT PRIMARY KEY,
                title TEXT,
                note TEXT,
                status TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                closed_at INTEGER,
                first_event_at INTEGER,
                last_event_at INTEGER,
                total_actions INTEGER NOT NULL DEFAULT 0,
                success_count INTEGER NOT NULL DEFAULT 0,
                failed_count INTEGER NOT NULL DEFAULT 0,
                timeout_or_unknown_count INTEGER NOT NULL DEFAULT 0,
                warning_count INTEGER NOT NULL DEFAULT 0,
                total_duration_ms INTEGER NOT NULL DEFAULT 0,
                changed_files_count INTEGER NOT NULL DEFAULT 0,
                job_ids_count INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_action_sessions_status_last_event
                ON action_sessions(status, last_event_at DESC, updated_at DESC);

            CREATE TABLE IF NOT EXISTS action_events (
                event_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                started_at INTEGER NOT NULL,
                ended_at INTEGER NOT NULL,
                duration_ms INTEGER NOT NULL,
                endpoint TEXT NOT NULL,
                operation TEXT,
                action_name TEXT NOT NULL,
                project TEXT,
                status TEXT NOT NULL,
                http_status INTEGER,
                error_summary TEXT,
                warning_summary TEXT,
                changed_files_json TEXT NOT NULL,
                ids_json TEXT NOT NULL,
                summary_json TEXT NOT NULL,
                request_bytes INTEGER,
                response_bytes INTEGER,
                FOREIGN KEY(session_id) REFERENCES action_sessions(session_id)
            );
            CREATE INDEX IF NOT EXISTS idx_action_events_session_started
                ON action_events(session_id, started_at DESC);

            CREATE TABLE IF NOT EXISTS oauth_clients (
                id TEXT PRIMARY KEY,
                client_id TEXT NOT NULL UNIQUE,
                client_secret_hash TEXT NOT NULL,
                name TEXT NOT NULL,
                owner_user_id TEXT NOT NULL,
                redirect_uris TEXT NOT NULL DEFAULT '',
                allowed_scopes TEXT NOT NULL DEFAULT '',
                created_at INTEGER NOT NULL,
                revoked_at INTEGER,
                FOREIGN KEY(owner_user_id) REFERENCES users(id)
            );
            CREATE INDEX IF NOT EXISTS idx_oauth_clients_client_id ON oauth_clients(client_id);
            CREATE INDEX IF NOT EXISTS idx_oauth_clients_owner ON oauth_clients(owner_user_id);

            CREATE TABLE IF NOT EXISTS oauth_authorization_codes (
                id TEXT PRIMARY KEY,
                code_hash TEXT NOT NULL UNIQUE,
                client_id TEXT NOT NULL,
                subject_kind TEXT NOT NULL DEFAULT 'managed_user',
                subject_id TEXT NOT NULL,
                user_id TEXT,
                redirect_uri TEXT NOT NULL,
                scopes TEXT NOT NULL DEFAULT '',
                code_challenge TEXT,
                code_challenge_method TEXT,
                resource TEXT,
                shared_key_hash TEXT,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                used_at INTEGER,
                revoked_at INTEGER,
                FOREIGN KEY(client_id) REFERENCES oauth_clients(client_id),
                FOREIGN KEY(user_id) REFERENCES users(id)
            );
            CREATE INDEX IF NOT EXISTS idx_oauth_auth_codes_hash ON oauth_authorization_codes(code_hash);
            CREATE INDEX IF NOT EXISTS idx_oauth_auth_codes_client ON oauth_authorization_codes(client_id);

            CREATE TABLE IF NOT EXISTS oauth_access_tokens (
                id TEXT PRIMARY KEY,
                token_hash TEXT NOT NULL UNIQUE,
                client_id TEXT NOT NULL,
                subject_kind TEXT NOT NULL DEFAULT 'managed_user',
                subject_id TEXT NOT NULL,
                user_id TEXT,
                scopes TEXT NOT NULL DEFAULT '',
                resource TEXT,
                shared_key_hash TEXT,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                revoked_at INTEGER,
                last_used_at INTEGER,
                FOREIGN KEY(client_id) REFERENCES oauth_clients(client_id),
                FOREIGN KEY(user_id) REFERENCES users(id)
            );
            CREATE INDEX IF NOT EXISTS idx_oauth_access_tokens_hash ON oauth_access_tokens(token_hash);
            CREATE INDEX IF NOT EXISTS idx_oauth_access_tokens_client ON oauth_access_tokens(client_id);
            CREATE INDEX IF NOT EXISTS idx_oauth_access_tokens_user ON oauth_access_tokens(user_id);

            CREATE TABLE IF NOT EXISTS oauth_refresh_tokens (
                id TEXT PRIMARY KEY,
                token_hash TEXT NOT NULL UNIQUE,
                client_id TEXT NOT NULL,
                subject_kind TEXT NOT NULL DEFAULT 'managed_user',
                subject_id TEXT NOT NULL,
                user_id TEXT,
                scopes TEXT NOT NULL DEFAULT '',
                resource TEXT,
                shared_key_hash TEXT,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                revoked_at INTEGER,
                last_used_at INTEGER,
                rotated_from_id TEXT,
                FOREIGN KEY(client_id) REFERENCES oauth_clients(client_id),
                FOREIGN KEY(user_id) REFERENCES users(id)
            );
            CREATE INDEX IF NOT EXISTS idx_oauth_refresh_tokens_hash ON oauth_refresh_tokens(token_hash);
            CREATE INDEX IF NOT EXISTS idx_oauth_refresh_tokens_client ON oauth_refresh_tokens(client_id);
            CREATE INDEX IF NOT EXISTS idx_oauth_refresh_tokens_user ON oauth_refresh_tokens(user_id);
            ",
        )?;

        // Evolve pre-existing DBs in place (fresh DBs already have full columns
        // from CREATE TABLE above).
        Self::migrate_oauth_bridge_columns(&conn)?;
        Self::migrate_users_and_api_keys(&conn)?;
        Ok(())
    }

    /// Remove tables that belonged to retired product surfaces (inbox messages,
    /// codex goals/commands, outbound agent specs with plaintext secrets, and
    /// desktop task prototypes). No remaining code path reads or writes these.
    fn drop_legacy_tables(conn: &Connection) -> anyhow::Result<()> {
        conn.execute_batch(
            "
            DROP TABLE IF EXISTS messages;
            DROP TABLE IF EXISTS command_requests;
            DROP TABLE IF EXISTS codex_goals;
            DROP TABLE IF EXISTS agent_specs;
            DROP TABLE IF EXISTS agent_model_profiles;
            DROP TABLE IF EXISTS desktop_tasks;
            DROP TABLE IF EXISTS desktop_task_events;
            ",
        )?;
        Ok(())
    }

    fn migrate_oauth_bridge_columns(conn: &Connection) -> anyhow::Result<()> {
        Self::migrate_oauth_authorization_codes_subject(conn)?;
        Self::migrate_oauth_access_tokens_subject(conn)?;
        Self::migrate_oauth_refresh_tokens_subject(conn)?;
        conn.execute_batch(
            "
            CREATE INDEX IF NOT EXISTS idx_oauth_auth_codes_hash ON oauth_authorization_codes(code_hash);
            CREATE INDEX IF NOT EXISTS idx_oauth_auth_codes_client ON oauth_authorization_codes(client_id);
            CREATE INDEX IF NOT EXISTS idx_oauth_access_tokens_hash ON oauth_access_tokens(token_hash);
            CREATE INDEX IF NOT EXISTS idx_oauth_access_tokens_client ON oauth_access_tokens(client_id);
            CREATE INDEX IF NOT EXISTS idx_oauth_access_tokens_user ON oauth_access_tokens(user_id);
            CREATE INDEX IF NOT EXISTS idx_oauth_refresh_tokens_hash ON oauth_refresh_tokens(token_hash);
            CREATE INDEX IF NOT EXISTS idx_oauth_refresh_tokens_client ON oauth_refresh_tokens(client_id);
            CREATE INDEX IF NOT EXISTS idx_oauth_refresh_tokens_user ON oauth_refresh_tokens(user_id);
            ",
        )?;
        Ok(())
    }

    fn oauth_subject_migration_needed(cols: &[TableColumnInfo]) -> bool {
        !has_column(cols, "subject_kind")
            || !has_column(cols, "subject_id")
            || !has_column(cols, "shared_key_hash")
            || !oauth_user_id_is_nullable(cols)
    }

    fn migrate_oauth_authorization_codes_subject(conn: &Connection) -> anyhow::Result<()> {
        let cols = table_column_info(conn, "oauth_authorization_codes")?;
        if !Self::oauth_subject_migration_needed(&cols) {
            return Ok(());
        }
        let subject_kind = subject_kind_expr(&cols);
        let subject_id = subject_id_expr(&cols);
        let shared_key_hash = column_expr(&cols, "shared_key_hash", "NULL");
        conn.execute_batch("PRAGMA foreign_keys=OFF;")?;
        conn.execute_batch(
            "
            ALTER TABLE oauth_authorization_codes RENAME TO oauth_authorization_codes_legacy_subject_migration;
            CREATE TABLE oauth_authorization_codes (
                id TEXT PRIMARY KEY,
                code_hash TEXT NOT NULL UNIQUE,
                client_id TEXT NOT NULL,
                subject_kind TEXT NOT NULL DEFAULT 'managed_user',
                subject_id TEXT NOT NULL,
                user_id TEXT,
                redirect_uri TEXT NOT NULL,
                scopes TEXT NOT NULL DEFAULT '',
                code_challenge TEXT,
                code_challenge_method TEXT,
                resource TEXT,
                shared_key_hash TEXT,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                used_at INTEGER,
                revoked_at INTEGER,
                FOREIGN KEY(client_id) REFERENCES oauth_clients(client_id),
                FOREIGN KEY(user_id) REFERENCES users(id)
            );
            ",
        )?;
        conn.execute(
            &format!(
                "INSERT INTO oauth_authorization_codes (
                    id, code_hash, client_id, subject_kind, subject_id, user_id,
                    redirect_uri, scopes, code_challenge, code_challenge_method,
                    resource, shared_key_hash, created_at, expires_at, used_at, revoked_at
                 )
                 SELECT id, code_hash, client_id, {}, {}, user_id,
                    redirect_uri, scopes, code_challenge, code_challenge_method,
                    resource, {}, created_at, expires_at, used_at, revoked_at
                 FROM oauth_authorization_codes_legacy_subject_migration",
                subject_kind, subject_id, shared_key_hash
            ),
            [],
        )?;
        conn.execute_batch(
            "
            DROP TABLE oauth_authorization_codes_legacy_subject_migration;
            PRAGMA foreign_keys=ON;
            ",
        )?;
        Ok(())
    }

    fn migrate_oauth_access_tokens_subject(conn: &Connection) -> anyhow::Result<()> {
        let cols = table_column_info(conn, "oauth_access_tokens")?;
        if !Self::oauth_subject_migration_needed(&cols) {
            return Ok(());
        }
        let subject_kind = subject_kind_expr(&cols);
        let subject_id = subject_id_expr(&cols);
        let shared_key_hash = column_expr(&cols, "shared_key_hash", "NULL");
        conn.execute_batch("PRAGMA foreign_keys=OFF;")?;
        conn.execute_batch(
            "
            ALTER TABLE oauth_access_tokens RENAME TO oauth_access_tokens_legacy_subject_migration;
            CREATE TABLE oauth_access_tokens (
                id TEXT PRIMARY KEY,
                token_hash TEXT NOT NULL UNIQUE,
                client_id TEXT NOT NULL,
                subject_kind TEXT NOT NULL DEFAULT 'managed_user',
                subject_id TEXT NOT NULL,
                user_id TEXT,
                scopes TEXT NOT NULL DEFAULT '',
                resource TEXT,
                shared_key_hash TEXT,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                revoked_at INTEGER,
                last_used_at INTEGER,
                FOREIGN KEY(client_id) REFERENCES oauth_clients(client_id),
                FOREIGN KEY(user_id) REFERENCES users(id)
            );
            ",
        )?;
        conn.execute(
            &format!(
                "INSERT INTO oauth_access_tokens (
                    id, token_hash, client_id, subject_kind, subject_id, user_id,
                    scopes, resource, shared_key_hash, created_at, expires_at,
                    revoked_at, last_used_at
                 )
                 SELECT id, token_hash, client_id, {}, {}, user_id,
                    scopes, resource, {}, created_at, expires_at, revoked_at, last_used_at
                 FROM oauth_access_tokens_legacy_subject_migration",
                subject_kind, subject_id, shared_key_hash
            ),
            [],
        )?;
        conn.execute_batch(
            "
            DROP TABLE oauth_access_tokens_legacy_subject_migration;
            PRAGMA foreign_keys=ON;
            ",
        )?;
        Ok(())
    }

    fn migrate_oauth_refresh_tokens_subject(conn: &Connection) -> anyhow::Result<()> {
        let cols = table_column_info(conn, "oauth_refresh_tokens")?;
        if !Self::oauth_subject_migration_needed(&cols) {
            return Ok(());
        }
        let subject_kind = subject_kind_expr(&cols);
        let subject_id = subject_id_expr(&cols);
        let shared_key_hash = column_expr(&cols, "shared_key_hash", "NULL");
        conn.execute_batch("PRAGMA foreign_keys=OFF;")?;
        conn.execute_batch(
            "
            ALTER TABLE oauth_refresh_tokens RENAME TO oauth_refresh_tokens_legacy_subject_migration;
            CREATE TABLE oauth_refresh_tokens (
                id TEXT PRIMARY KEY,
                token_hash TEXT NOT NULL UNIQUE,
                client_id TEXT NOT NULL,
                subject_kind TEXT NOT NULL DEFAULT 'managed_user',
                subject_id TEXT NOT NULL,
                user_id TEXT,
                scopes TEXT NOT NULL DEFAULT '',
                resource TEXT,
                shared_key_hash TEXT,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                revoked_at INTEGER,
                last_used_at INTEGER,
                rotated_from_id TEXT,
                FOREIGN KEY(client_id) REFERENCES oauth_clients(client_id),
                FOREIGN KEY(user_id) REFERENCES users(id)
            );
            ",
        )?;
        conn.execute(
            &format!(
                "INSERT INTO oauth_refresh_tokens (
                    id, token_hash, client_id, subject_kind, subject_id, user_id,
                    scopes, resource, shared_key_hash, created_at, expires_at,
                    revoked_at, last_used_at, rotated_from_id
                 )
                 SELECT id, token_hash, client_id, {}, {}, user_id,
                    scopes, resource, {}, created_at, expires_at, revoked_at,
                    last_used_at, rotated_from_id
                 FROM oauth_refresh_tokens_legacy_subject_migration",
                subject_kind, subject_id, shared_key_hash
            ),
            [],
        )?;
        conn.execute_batch(
            "
            DROP TABLE oauth_refresh_tokens_legacy_subject_migration;
            PRAGMA foreign_keys=ON;
            ",
        )?;
        Ok(())
    }

    /// Add Phase 2 columns (`display_name`, `role`, `disabled_at`,
    /// `updated_at` on `users`; `scopes`, `expires_at` on `api_keys`) to older
    /// databases. Each ALTER is guarded by a `PRAGMA table_info` check so it
    /// is idempotent and safe to run on every startup.
    ///
    /// Phase 3 extends `api_keys` with `kind` (default `"user"`) and
    /// `allowed_client_id` (nullable). Existing personal API tokens are
    /// preserved as `kind="user"` via the column default; agent tokens must be
    /// created explicitly through the agent-token management endpoints.
    fn migrate_users_and_api_keys(conn: &Connection) -> anyhow::Result<()> {
        let user_cols = table_columns(conn, "users")?;
        for (col, decl) in [
            ("display_name", "TEXT"),
            ("role", "TEXT NOT NULL DEFAULT 'user'"),
            ("disabled_at", "INTEGER"),
            ("updated_at", "INTEGER"),
        ] {
            if !user_cols.iter().any(|c| c == col) {
                conn.execute(
                    &format!("ALTER TABLE users ADD COLUMN {} {}", col, decl),
                    [],
                )?;
            }
        }
        let key_cols = table_columns(conn, "api_keys")?;
        for (col, decl) in [
            ("scopes", "TEXT NOT NULL DEFAULT ''"),
            ("expires_at", "INTEGER"),
            // Phase 3: agent token kind + bound client_id. `kind` defaults to
            // `"user"` so legacy rows continue to behave as personal API
            // tokens. `allowed_client_id` is nullable and only set on agent
            // tokens.
            ("kind", "TEXT NOT NULL DEFAULT 'user'"),
            ("allowed_client_id", "TEXT"),
        ] {
            if !key_cols.iter().any(|c| c == col) {
                conn.execute(
                    &format!("ALTER TABLE api_keys ADD COLUMN {} {}", col, decl),
                    [],
                )?;
            }
        }
        Ok(())
    }
}

/// Return the set of column names present on `table`. Used by the idempotent
/// Phase 2 migration helpers to decide whether an `ALTER TABLE ... ADD COLUMN`
/// is needed.
pub(super) fn table_columns(conn: &Connection, table: &str) -> anyhow::Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let mut cols = Vec::new();
    for row in rows {
        cols.push(row?);
    }
    Ok(cols)
}

#[derive(Debug)]
pub(super) struct TableColumnInfo {
    name: String,
    notnull: bool,
}

pub(super) fn table_column_info(
    conn: &Connection,
    table: &str,
) -> anyhow::Result<Vec<TableColumnInfo>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table))?;
    let rows = stmt.query_map([], |row| {
        Ok(TableColumnInfo {
            name: row.get(1)?,
            notnull: row.get::<_, i64>(3)? != 0,
        })
    })?;
    let mut cols = Vec::new();
    for row in rows {
        cols.push(row?);
    }
    Ok(cols)
}

pub(super) fn has_column(cols: &[TableColumnInfo], name: &str) -> bool {
    cols.iter().any(|c| c.name == name)
}

fn column_expr(cols: &[TableColumnInfo], name: &str, fallback: &str) -> String {
    if has_column(cols, name) {
        name.to_string()
    } else {
        fallback.to_string()
    }
}

fn subject_kind_expr(cols: &[TableColumnInfo]) -> String {
    if has_column(cols, "subject_kind") {
        "COALESCE(NULLIF(subject_kind, ''), 'managed_user')".to_string()
    } else {
        "'managed_user'".to_string()
    }
}

fn subject_id_expr(cols: &[TableColumnInfo]) -> String {
    if has_column(cols, "subject_id") {
        "COALESCE(NULLIF(subject_id, ''), user_id)".to_string()
    } else {
        "user_id".to_string()
    }
}

pub(super) fn oauth_user_id_is_nullable(cols: &[TableColumnInfo]) -> bool {
    cols.iter()
        .find(|c| c.name == "user_id")
        .map(|c| !c.notnull)
        .unwrap_or(false)
}
