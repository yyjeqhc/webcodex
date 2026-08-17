use super::Database;
use anyhow::Context;
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
            window_projects: Mutex::new(std::collections::HashMap::new()),
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
                principal_kind TEXT,
                principal_user_id TEXT,
                oauth_client_id TEXT,
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
                mode TEXT NOT NULL CHECK(mode IN ('normal', 'inspect', 'read_only')),
                status TEXT NOT NULL
                    CHECK(status IN ('active', 'ready_for_review', 'accepted', 'rejected')),
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

            CREATE TABLE IF NOT EXISTS wc_run_contexts (
                run_id TEXT PRIMARY KEY,
                target_executor_ref TEXT NOT NULL,
                execution_executor_ref TEXT NOT NULL,
                target_root TEXT NOT NULL,
                execution_root TEXT NOT NULL,
                baseline_commit TEXT,
                baseline_tree TEXT,
                isolated INTEGER NOT NULL CHECK(isolated IN (0, 1)),
                created_at INTEGER NOT NULL,
                CHECK(isolated = 0 OR (baseline_commit IS NOT NULL AND baseline_tree IS NOT NULL)),
                FOREIGN KEY(run_id) REFERENCES wc_runs(id)
            );

            CREATE TABLE IF NOT EXISTS wc_task_results (
                id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL UNIQUE,
                run_id TEXT NOT NULL UNIQUE,
                summary TEXT NOT NULL,
                patch_artifact TEXT,
                patch_sha256 TEXT,
                patch_bytes INTEGER NOT NULL CHECK(patch_bytes >= 0),
                changed_paths_json TEXT NOT NULL,
                validation_json TEXT NOT NULL,
                warnings_json TEXT NOT NULL,
                decision_status TEXT NOT NULL
                    CHECK(decision_status IN ('pending', 'accepted', 'rejected')),
                decided_by TEXT,
                decided_at INTEGER,
                cleanup_warning TEXT,
                created_at INTEGER NOT NULL,
                CHECK(
                    (patch_bytes = 0 AND patch_artifact IS NULL AND patch_sha256 IS NULL)
                    OR
                    (patch_bytes > 0 AND patch_artifact IS NOT NULL AND patch_sha256 IS NOT NULL)
                ),
                FOREIGN KEY(task_id) REFERENCES wc_tasks(id),
                FOREIGN KEY(run_id) REFERENCES wc_runs(id)
            );
            CREATE INDEX IF NOT EXISTS idx_wc_task_results_decision
                ON wc_task_results(decision_status, created_at DESC);

            CREATE TABLE IF NOT EXISTS wc_result_decision_intents (
                task_id TEXT PRIMARY KEY,
                result_id TEXT NOT NULL,
                decision TEXT NOT NULL CHECK(decision IN ('accepted', 'rejected')),
                actor TEXT NOT NULL,
                started_at INTEGER NOT NULL,
                state TEXT NOT NULL DEFAULT 'pending'
                    CHECK(state IN ('pending', 'needs_attention')),
                error_code TEXT,
                error_message TEXT,
                last_attempt_at INTEGER,
                FOREIGN KEY(task_id) REFERENCES wc_task_results(task_id),
                FOREIGN KEY(result_id) REFERENCES wc_task_results(id)
            );

            CREATE TABLE IF NOT EXISTS wc_approvals (
                id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                run_id TEXT NOT NULL,
                action_kind TEXT NOT NULL,
                action_hash TEXT NOT NULL,
                action_summary TEXT NOT NULL,
                state TEXT NOT NULL
                    CHECK(state IN ('pending', 'approved', 'denied', 'consumed', 'expired')),
                requested_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                decided_by TEXT,
                decided_at INTEGER,
                consumed_at INTEGER,
                CHECK(expires_at > requested_at),
                UNIQUE(task_id, run_id, action_hash),
                FOREIGN KEY(task_id) REFERENCES wc_tasks(id),
                FOREIGN KEY(run_id) REFERENCES wc_runs(id)
            );
            CREATE INDEX IF NOT EXISTS idx_wc_approvals_task_state
                ON wc_approvals(task_id, state, requested_at DESC);

            CREATE TABLE IF NOT EXISTS wc_edit_operations (
                task_id TEXT NOT NULL,
                operation_id TEXT NOT NULL,
                request_sha256 TEXT NOT NULL,
                state TEXT NOT NULL CHECK(state IN ('pending', 'completed', 'failed')),
                result_json TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY(task_id, operation_id),
                CHECK(
                    (state IN ('pending', 'failed') AND result_json IS NULL)
                    OR (state = 'completed' AND result_json IS NOT NULL)
                ),
                FOREIGN KEY(task_id) REFERENCES wc_tasks(id)
            );

            CREATE TABLE IF NOT EXISTS wc_executions (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL CHECK(kind IN ('command', 'check')),
                task_id TEXT NOT NULL,
                run_id TEXT NOT NULL,
                state TEXT NOT NULL CHECK(state IN (
                    'accepted', 'queued', 'starting', 'running', 'cancel_requested',
                    'succeeded', 'failed', 'cancelled', 'interrupted', 'unknown'
                )),
                submitted_at INTEGER NOT NULL,
                queued_at INTEGER,
                queue_deadline INTEGER NOT NULL,
                started_at INTEGER,
                last_output_at INTEGER,
                finished_at INTEGER,
                stdout_cursor INTEGER NOT NULL DEFAULT 1 CHECK(stdout_cursor >= 1),
                stderr_cursor INTEGER NOT NULL DEFAULT 1 CHECK(stderr_cursor >= 1),
                exit_code INTEGER,
                failure_source TEXT,
                failure_code TEXT,
                cancel_requested_at INTEGER,
                terminal_reason TEXT,
                operation_id TEXT NOT NULL,
                request_sha256 TEXT NOT NULL,
                executor_reference TEXT,
                first_status_failure_at INTEGER,
                last_successful_observation_at INTEGER,
                status_failure_code TEXT,
                check_plan TEXT,
                check_recipe_json TEXT,
                check_completed INTEGER NOT NULL DEFAULT 0 CHECK(check_completed >= 0),
                check_workspace_sha256 TEXT,
                validated_workspace_sha256 TEXT,
                failed_check TEXT,
                assertion_evidence_json TEXT,
                UNIQUE(task_id, run_id, operation_id),
                CHECK(
                    (kind = 'command' AND check_plan IS NULL)
                    OR (kind = 'check' AND check_plan IS NOT NULL)
                ),
                FOREIGN KEY(task_id) REFERENCES wc_tasks(id),
                FOREIGN KEY(run_id) REFERENCES wc_runs(id)
            );
            CREATE INDEX IF NOT EXISTS idx_wc_executions_task_submitted
                ON wc_executions(task_id, submitted_at DESC);
            CREATE INDEX IF NOT EXISTS idx_wc_executions_run_state
                ON wc_executions(run_id, state, submitted_at DESC);

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

            CREATE TABLE IF NOT EXISTS wc_window_project_contexts (
                window_key TEXT NOT NULL,
                window_source TEXT NOT NULL,
                project_id TEXT NOT NULL,
                owner_subject_id TEXT NOT NULL,
                project_root_sha256 TEXT NOT NULL,
                task_id TEXT NOT NULL UNIQUE,
                target_path TEXT NOT NULL DEFAULT '',
                fingerprint_json TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY(
                    window_key,
                    project_id,
                    owner_subject_id,
                    project_root_sha256
                ),
                FOREIGN KEY(project_id) REFERENCES wc_projects(id),
                FOREIGN KEY(task_id) REFERENCES wc_tasks(id)
            );

            CREATE TABLE IF NOT EXISTS admin_project_lifecycle_audit (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at INTEGER NOT NULL,
                correlation_id TEXT NOT NULL,
                subject_type TEXT NOT NULL,
                subject_id TEXT NOT NULL,
                operation TEXT NOT NULL,
                project TEXT NOT NULL,
                client_id TEXT,
                outcome TEXT NOT NULL,
                changed INTEGER NOT NULL,
                reason_code TEXT,
                idempotency_digest TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_admin_project_lifecycle_audit_created
                ON admin_project_lifecycle_audit(created_at DESC);

            CREATE TABLE IF NOT EXISTS admin_project_idempotency (
                subject TEXT NOT NULL,
                action TEXT NOT NULL,
                target TEXT NOT NULL,
                key_hash TEXT NOT NULL,
                request_hash TEXT NOT NULL,
                http_status INTEGER NOT NULL,
                response_json TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                PRIMARY KEY(subject, action, target, key_hash)
            );
            CREATE INDEX IF NOT EXISTS idx_admin_project_idempotency_created
                ON admin_project_idempotency(created_at DESC);

            CREATE TABLE IF NOT EXISTS workspace_activity (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at INTEGER NOT NULL,
                project TEXT,
                tool TEXT NOT NULL,
                surface TEXT NOT NULL,
                success INTEGER NOT NULL,
                session_id TEXT,
                client TEXT,
                command_preview TEXT,
                paths_json TEXT NOT NULL DEFAULT '[]',
                error_summary TEXT,
                -- Attribution fixed at write time. 'legacy_unscoped' marks rows
                -- from before this column existed, whose owner cannot be proven.
                scope_kind TEXT NOT NULL DEFAULT 'legacy_unscoped',
                scope_id TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_workspace_activity_id
                ON workspace_activity(id DESC);
            ",
        )?;

        // Optional additive columns for older single-file DBs that predate the
        // current CREATE TABLE definitions. OAuth subject shape is not migrated:
        // tables are always created with the current schema, and pre-subject
        // layouts are unsupported (recreate the OAuth tables if needed).
        Self::ensure_users_and_api_key_columns(&conn)?;
        Self::ensure_action_event_attribution_columns(&conn)?;
        Self::ensure_connector_execution_columns(&conn)?;
        Self::ensure_connector_task_columns(&conn)?;
        Self::ensure_connector_task_modes(&conn)?;
        Self::ensure_activity_scope_columns(&conn)?;
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

    /// Add exact authenticated-caller attribution to `action_events` on older
    /// databases. Historical rows intentionally remain NULL: caller identity
    /// cannot be reconstructed safely from session, target project, or client.
    fn ensure_action_event_attribution_columns(conn: &Connection) -> anyhow::Result<()> {
        let columns = table_columns(conn, "action_events")?;
        for column in ["principal_kind", "principal_user_id", "oauth_client_id"] {
            if !columns.iter().any(|existing| existing == column) {
                conn.execute(
                    &format!("ALTER TABLE action_events ADD COLUMN {} TEXT", column),
                    [],
                )?;
            }
        }
        // Created after the additive migration because an existing table may not
        // have these columns while the base CREATE TABLE batch is running.
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_action_events_principal_user_started
             ON action_events(principal_user_id, started_at DESC)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_action_events_oauth_client_started
             ON action_events(oauth_client_id, started_at DESC)",
            [],
        )?;
        Ok(())
    }

    /// Add durable attribution to `workspace_activity` on existing databases.
    ///
    /// Rows that predate these columns keep `legacy_unscoped`: which grant
    /// produced them cannot be established after the fact, and guessing from
    /// the client id is exactly the reinterpretation this is meant to stop. So
    /// they stay readable to bootstrap/admin and invisible to any project
    /// credential rather than being deleted or re-attributed.
    fn ensure_activity_scope_columns(conn: &Connection) -> anyhow::Result<()> {
        let columns = table_columns(conn, "workspace_activity")?;
        if !columns.iter().any(|existing| existing == "scope_kind") {
            conn.execute(
                "ALTER TABLE workspace_activity
                 ADD COLUMN scope_kind TEXT NOT NULL DEFAULT 'legacy_unscoped'",
                [],
            )?;
        }
        if !columns.iter().any(|existing| existing == "scope_id") {
            conn.execute(
                "ALTER TABLE workspace_activity ADD COLUMN scope_id TEXT",
                [],
            )?;
        }
        // Created here rather than in the base DDL: on an existing database the
        // columns do not exist until the ALTERs above have run.
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_workspace_activity_scope
             ON workspace_activity(scope_kind, scope_id, id DESC)",
            [],
        )?;
        Ok(())
    }

    fn ensure_connector_task_columns(conn: &Connection) -> anyhow::Result<()> {
        let columns = table_columns(conn, "wc_tasks")?;
        // Watermark for deliver-once human guidance: capability responses
        // attach guidance events above this sequence, then advance it.
        if !columns
            .iter()
            .any(|existing| existing == "guidance_seen_seq")
        {
            conn.execute(
                "ALTER TABLE wc_tasks ADD COLUMN guidance_seen_seq INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        let activity_columns = table_columns(conn, "workspace_activity")?;
        if !activity_columns.iter().any(|existing| existing == "client") {
            conn.execute("ALTER TABLE workspace_activity ADD COLUMN client TEXT", [])?;
        }
        let approval_columns = table_columns(conn, "wc_approvals")?;
        if !approval_columns
            .iter()
            .any(|existing| existing == "decision_reason")
        {
            conn.execute(
                "ALTER TABLE wc_approvals ADD COLUMN decision_reason TEXT",
                [],
            )?;
        }
        Ok(())
    }

    /// Expand the connector task-mode constraint on databases created before
    /// `inspect` became a persisted mode. SQLite cannot alter a CHECK
    /// constraint in place, so rebuild only this table while preserving rows
    /// and the stable table name referenced by child tables.
    fn ensure_connector_task_modes(conn: &Connection) -> anyhow::Result<()> {
        let table_sql: String = conn.query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'wc_tasks'",
            [],
            |row| row.get(0),
        )?;
        if table_sql.contains("'inspect'") {
            return Ok(());
        }

        let migration = match conn.execute_batch("PRAGMA foreign_keys = OFF;") {
            Ok(()) => (|| -> anyhow::Result<()> {
                conn.execute_batch("BEGIN IMMEDIATE;")
                    .context("begin connector task mode migration")?;
                conn.execute_batch(
                    "
                    CREATE TABLE wc_tasks_inspect_migration (
                        id TEXT PRIMARY KEY,
                        project_id TEXT NOT NULL,
                        owner_subject_id TEXT NOT NULL,
                        goal TEXT NOT NULL,
                        mode TEXT NOT NULL CHECK(mode IN ('normal', 'inspect', 'read_only')),
                        status TEXT NOT NULL
                            CHECK(status IN ('active', 'ready_for_review', 'accepted', 'rejected')),
                        created_at INTEGER NOT NULL,
                        updated_at INTEGER NOT NULL,
                        guidance_seen_seq INTEGER NOT NULL DEFAULT 0,
                        FOREIGN KEY(project_id) REFERENCES wc_projects(id)
                    );
                    INSERT INTO wc_tasks_inspect_migration
                        (id, project_id, owner_subject_id, goal, mode, status,
                         created_at, updated_at, guidance_seen_seq)
                    SELECT id, project_id, owner_subject_id, goal, mode, status,
                           created_at, updated_at, guidance_seen_seq
                    FROM wc_tasks;
                    DROP TABLE wc_tasks;
                    ALTER TABLE wc_tasks_inspect_migration RENAME TO wc_tasks;
                    CREATE INDEX idx_wc_tasks_owner_project
                        ON wc_tasks(owner_subject_id, project_id, updated_at DESC);
                    ",
                )
                .context("rebuild wc_tasks for inspect task mode")?;

                let foreign_key_error = {
                    let mut statement = conn
                        .prepare("PRAGMA foreign_key_check")
                        .context("prepare connector task mode foreign key check")?;
                    statement
                        .exists([])
                        .context("query connector task mode foreign key check")?
                };
                if foreign_key_error {
                    anyhow::bail!(
                        "connector task mode migration foreign_key_check found a violation"
                    );
                }
                conn.execute_batch("COMMIT;")
                    .context("commit connector task mode migration")?;
                Ok(())
            })(),
            Err(error) => Err(anyhow::Error::new(error)
                .context("disable foreign keys for connector task mode migration")),
        };

        let migration = match migration {
            Ok(()) => Ok(()),
            Err(error) => {
                let rollback = conn.execute_batch("ROLLBACK;");
                if let Err(rollback_error) = rollback {
                    if !conn.is_autocommit() {
                        Err(error.context(format!(
                            "connector task mode rollback also failed: {rollback_error}"
                        )))
                    } else {
                        Err(error)
                    }
                } else {
                    Err(error)
                }
            }
        };
        let restore = restore_foreign_keys(conn);
        match (migration, restore) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), Ok(())) => Err(error),
            (Ok(()), Err(restore_error)) => Err(restore_error),
            (Err(error), Err(restore_error)) => Err(error.context(format!(
                "restoring foreign_keys after connector task mode migration also failed: \
                 {restore_error:#}"
            ))),
        }
    }

    fn ensure_connector_execution_columns(conn: &Connection) -> anyhow::Result<()> {
        let columns = table_columns(conn, "wc_executions")?;
        for (column, declaration) in [
            ("check_plan", "TEXT"),
            ("check_recipe_json", "TEXT"),
            ("check_completed", "INTEGER NOT NULL DEFAULT 0"),
            ("check_workspace_sha256", "TEXT"),
            ("validated_workspace_sha256", "TEXT"),
            ("failed_check", "TEXT"),
            ("assertion_evidence_json", "TEXT"),
        ] {
            if !columns.iter().any(|existing| existing == column) {
                conn.execute(
                    &format!("ALTER TABLE wc_executions ADD COLUMN {column} {declaration}"),
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

fn restore_foreign_keys(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .context("restore foreign_keys after connector task mode migration")?;
    let enabled: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .context("verify foreign_keys after connector task mode migration")?;
    if enabled != 1 {
        anyhow::bail!(
            "foreign_keys remained disabled after connector task mode migration (value {enabled})"
        );
    }
    Ok(())
}

#[cfg(test)]
mod connector_task_mode_tests {
    use super::*;

    #[test]
    fn connector_task_mode_migration_rolls_back_foreign_key_failure() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "
            PRAGMA foreign_keys = OFF;
            CREATE TABLE wc_projects (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE wc_tasks (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                owner_subject_id TEXT NOT NULL,
                goal TEXT NOT NULL,
                mode TEXT NOT NULL CHECK(mode IN ('normal', 'read_only')),
                status TEXT NOT NULL
                    CHECK(status IN ('active', 'ready_for_review', 'accepted', 'rejected')),
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                guidance_seen_seq INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY(project_id) REFERENCES wc_projects(id)
            );
            CREATE INDEX idx_wc_tasks_owner_project
                ON wc_tasks(owner_subject_id, project_id, updated_at DESC);
            INSERT INTO wc_tasks
                (id, project_id, owner_subject_id, goal, mode, status,
                 created_at, updated_at, guidance_seen_seq)
            VALUES ('task', 'missing-project', 'owner', 'keep me', 'read_only',
                    'active', 1, 1, 9);
            PRAGMA foreign_keys = ON;
            ",
        )
        .unwrap();
        assert_eq!(
            conn.query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );

        let error = Database::ensure_connector_task_modes(&conn).unwrap_err();
        assert!(
            format!("{error:#}").contains("foreign_key_check"),
            "{error:#}"
        );
        let schema: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'wc_tasks'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!schema.contains("'inspect'"), "{schema}");
        assert_eq!(
            conn.query_row(
                "SELECT project_id, goal, guidance_seen_seq FROM wc_tasks WHERE id = 'task'",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .unwrap(),
            ("missing-project".to_string(), "keep me".to_string(), 9)
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name = 'wc_tasks_inspect_migration'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            0
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'index' AND name = 'idx_wc_tasks_owner_project'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );

        conn.execute(
            "INSERT INTO wc_projects VALUES ('missing-project', 'Project', 1, 1)",
            [],
        )
        .unwrap();
        Database::ensure_connector_task_modes(&conn).unwrap();
        let schema: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'wc_tasks'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(schema.contains("'inspect'"), "{schema}");
        assert_eq!(
            conn.query_row(
                "SELECT mode, goal, guidance_seen_seq FROM wc_tasks WHERE id = 'task'",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .unwrap(),
            ("read_only".to_string(), "keep me".to_string(), 9)
        );
        assert_eq!(
            conn.query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
}
