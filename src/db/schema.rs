use super::Database;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;

impl Database {
    pub fn open(db_path: &PathBuf) -> anyhow::Result<Self> {
        let conn = Connection::open(db_path)?;
        // Single-operator deployment: prefer durability + predictable locking
        // over multi-writer shared-cache gymnastics. WAL lets readers (CLI
        // inspect, sqlite3) coexist with the server without default BUSY.
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA busy_timeout = 5000;
            PRAGMA foreign_keys = ON;
            ",
        )?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_tables()?;
        // Personal-use instance: reclaim dead auth rows on every open rather
        // than running a background reaper.
        let now = chrono::Utc::now().timestamp();
        db.purge_stale_auth_rows(now)?;
        Ok(db)
    }

    /// Delete expired / used / revoked auth material that can never be used
    /// again. Safe to call repeatedly; returns the total number of deleted rows.
    pub fn purge_stale_auth_rows(&self, now: i64) -> anyhow::Result<usize> {
        let conn = self.conn.lock().unwrap();
        let mut deleted = 0usize;
        deleted += conn.execute(
            "DELETE FROM oauth_authorization_codes
             WHERE expires_at <= ?1 OR used_at IS NOT NULL OR revoked_at IS NOT NULL",
            rusqlite::params![now],
        )?;
        deleted += conn.execute(
            "DELETE FROM oauth_access_tokens
             WHERE expires_at <= ?1 OR revoked_at IS NOT NULL",
            rusqlite::params![now],
        )?;
        deleted += conn.execute(
            "DELETE FROM oauth_refresh_tokens
             WHERE expires_at <= ?1 OR revoked_at IS NOT NULL",
            rusqlite::params![now],
        )?;
        deleted += conn.execute(
            "DELETE FROM pairing_codes
             WHERE expires_at <= ?1 OR used_at IS NOT NULL",
            rusqlite::params![now],
        )?;
        deleted += conn.execute(
            "DELETE FROM api_keys
             WHERE revoked_at IS NOT NULL
                OR (expires_at IS NOT NULL AND expires_at <= ?1)",
            rusqlite::params![now],
        )?;
        deleted += conn.execute(
            "DELETE FROM account_credentials
             WHERE revoked_at IS NOT NULL",
            [],
        )?;
        Ok(deleted)
    }

    fn init_tables(&self) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        // Drop prototype tables that no longer have product callers.
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

            CREATE TABLE IF NOT EXISTS wc_projects (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS wc_workspaces (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                executor_ref TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                FOREIGN KEY(project_id) REFERENCES wc_projects(id)
            );
            CREATE INDEX IF NOT EXISTS idx_wc_workspaces_project
                ON wc_workspaces(project_id);

            CREATE TABLE IF NOT EXISTS wc_connector_grants (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                subject_id TEXT NOT NULL,
                profile TEXT NOT NULL,
                capabilities_json TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                revoked_at INTEGER,
                UNIQUE(project_id, subject_id),
                FOREIGN KEY(project_id) REFERENCES wc_projects(id)
            );
            CREATE INDEX IF NOT EXISTS idx_wc_connector_grants_subject
                ON wc_connector_grants(subject_id, revoked_at);

            CREATE TABLE IF NOT EXISTS wc_tasks (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                owner_subject_id TEXT NOT NULL,
                goal TEXT NOT NULL,
                mode TEXT NOT NULL CHECK(mode IN ('normal', 'read_only')),
                status TEXT NOT NULL CHECK(status IN ('active', 'ready_for_review')),
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                FOREIGN KEY(project_id) REFERENCES wc_projects(id)
            );
            CREATE INDEX IF NOT EXISTS idx_wc_tasks_owner_project
                ON wc_tasks(owner_subject_id, project_id, updated_at DESC);

            CREATE TABLE IF NOT EXISTS wc_runs (
                id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                workspace_id TEXT NOT NULL,
                status TEXT NOT NULL CHECK(status IN ('running', 'completed', 'interrupted')),
                started_at INTEGER NOT NULL,
                finished_at INTEGER,
                FOREIGN KEY(task_id) REFERENCES wc_tasks(id),
                FOREIGN KEY(workspace_id) REFERENCES wc_workspaces(id)
            );
            CREATE INDEX IF NOT EXISTS idx_wc_runs_task_started
                ON wc_runs(task_id, started_at DESC);

            CREATE TABLE IF NOT EXISTS wc_task_events (
                id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                run_id TEXT NOT NULL,
                sequence INTEGER NOT NULL CHECK(sequence > 0),
                kind TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                UNIQUE(task_id, sequence),
                FOREIGN KEY(task_id) REFERENCES wc_tasks(id),
                FOREIGN KEY(run_id) REFERENCES wc_runs(id)
            );
            CREATE INDEX IF NOT EXISTS idx_wc_task_events_task_sequence
                ON wc_task_events(task_id, sequence);
            ",
        )?;

        // Optional additive columns for older single-file DBs that predate the
        // current CREATE TABLE definitions. OAuth subject shape is not migrated:
        // tables are always created with the current schema, and pre-subject
        // layouts are unsupported (recreate the OAuth tables if needed).
        Self::ensure_users_and_api_key_columns(&conn)?;
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

    /// Ensure `users` / `api_keys` carry the current additive columns. Fresh DBs
    /// already declare them in CREATE TABLE; this only backfills missing columns
    /// on older files without rewriting rows.
    fn ensure_users_and_api_key_columns(conn: &Connection) -> anyhow::Result<()> {
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

fn table_columns(conn: &Connection, table: &str) -> anyhow::Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let mut cols = Vec::new();
    for row in rows {
        cols.push(row?);
    }
    Ok(cols)
}
