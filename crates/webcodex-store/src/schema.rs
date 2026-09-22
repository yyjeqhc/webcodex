use super::Database;
use anyhow::Context;
use rusqlite::Connection;
use std::path::PathBuf;

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
        let state_path = std::fs::canonicalize(db_path).context("resolve database state path")?;
        let db = Self::from_connection(conn, state_path);
        db.init_tables()?;
        // Personal-use instance: reclaim dead auth rows on every open rather
        // than running a background reaper.
        let now = chrono::Utc::now().timestamp();
        db.purge_stale_auth_rows(now)?;
        db.prune_job_receipts(now)?;
        Ok(db)
    }

    /// Delete expired / used / revoked auth material that can never be used
    /// again. Safe to call repeatedly; returns the total number of deleted rows.
    pub fn purge_stale_auth_rows(&self, now: i64) -> anyhow::Result<usize> {
        let conn = self.lock_connection(crate::StoreDomain::Core);
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
        let mut conn = self.lock_connection(crate::StoreDomain::Schema);
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS wc_job_receipts (
                job_id TEXT PRIMARY KEY,
                client_id TEXT NOT NULL,
                runner_instance_id TEXT NOT NULL,
                auth_kind TEXT NOT NULL,
                auth_partition TEXT,
                owner_at_admission TEXT,
                kind TEXT NOT NULL,
                snapshot TEXT NOT NULL,
                terminal_observed_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_job_receipts_expiry ON wc_job_receipts(expires_at);
            CREATE INDEX IF NOT EXISTS idx_job_receipts_runner_history
                ON wc_job_receipts(client_id, terminal_observed_at DESC, job_id DESC);

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
                runner_capabilities INTEGER NOT NULL DEFAULT 0 CHECK(runner_capabilities IN (0, 1)),
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
                client_window_key TEXT,
                client_window_source TEXT,
                server_trace_id TEXT,
                principal_correlation_kind TEXT,
                principal_correlation_id TEXT,
                window_started_at_ms INTEGER,
                window_ended_at_ms INTEGER,
                request_observed_at_ms INTEGER,
                response_handed_at_ms INTEGER,
                window_transition_kind TEXT,
                response_streaming INTEGER CHECK(response_streaming IS NULL OR response_streaming IN (0, 1)),
                window_continuity_eligible INTEGER CHECK(window_continuity_eligible IS NULL OR window_continuity_eligible IN (0, 1)),
                window_meaningful INTEGER NOT NULL DEFAULT 0 CHECK(window_meaningful IN (0, 1)),
                recorder_gap_session_id TEXT,
                FOREIGN KEY(session_id) REFERENCES action_sessions(session_id)
            );
            CREATE INDEX IF NOT EXISTS idx_action_events_session_started
                ON action_events(session_id, started_at DESC);
            CREATE INDEX IF NOT EXISTS idx_action_events_principal_user_started
                ON action_events(principal_user_id, started_at DESC);
            CREATE INDEX IF NOT EXISTS idx_action_events_oauth_client_started
                ON action_events(oauth_client_id, started_at DESC);

            CREATE TABLE IF NOT EXISTS action_event_workflow_links (
                event_id TEXT NOT NULL,
                workflow_session_id TEXT NOT NULL,
                workflow_session_relation TEXT NOT NULL
                    CHECK(workflow_session_relation IN ('recording', 'work_on_project')),
                project TEXT,
                linked_at_ms INTEGER NOT NULL,
                PRIMARY KEY(event_id, workflow_session_id, workflow_session_relation),
                FOREIGN KEY(event_id) REFERENCES action_events(event_id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_action_event_workflow_links_session
                ON action_event_workflow_links(workflow_session_id, linked_at_ms DESC);

            CREATE TABLE IF NOT EXISTS oauth_clients (
                id TEXT PRIMARY KEY,
                client_id TEXT NOT NULL UNIQUE,
                client_secret_hash TEXT NOT NULL,
                name TEXT NOT NULL,
                owner_user_id TEXT,
                owner_project_grant_id TEXT,
                owner_shared_key_hash TEXT,
                redirect_uris TEXT NOT NULL DEFAULT '',
                allowed_scopes TEXT NOT NULL DEFAULT '',
                created_at INTEGER NOT NULL,
                revoked_at INTEGER,
                CHECK (
                    (owner_user_id IS NOT NULL AND owner_project_grant_id IS NULL AND owner_shared_key_hash IS NULL)
                    OR (owner_user_id IS NULL AND owner_project_grant_id IS NOT NULL AND owner_shared_key_hash IS NULL)
                    OR (owner_user_id IS NULL AND owner_project_grant_id IS NULL AND owner_shared_key_hash IS NOT NULL)
                ),
                FOREIGN KEY(owner_user_id) REFERENCES users(id)
            );
            CREATE INDEX IF NOT EXISTS idx_oauth_clients_client_id ON oauth_clients(client_id);
            CREATE INDEX IF NOT EXISTS idx_oauth_clients_owner ON oauth_clients(owner_user_id);
            CREATE INDEX IF NOT EXISTS idx_oauth_clients_project_owner
                ON oauth_clients(owner_project_grant_id);
            CREATE INDEX IF NOT EXISTS idx_oauth_clients_shared_key_owner
                ON oauth_clients(owner_shared_key_hash);

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
            CREATE INDEX IF NOT EXISTS idx_workspace_activity_scope
                ON workspace_activity(scope_kind, scope_id, id DESC);

            CREATE TABLE IF NOT EXISTS project_references (
                principal_key TEXT NOT NULL,
                ref_index INTEGER NOT NULL CHECK(ref_index >= 1),
                canonical_project_id TEXT NOT NULL,
                root_fingerprint TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                PRIMARY KEY(principal_key, ref_index),
                UNIQUE(principal_key, canonical_project_id, root_fingerprint)
            );
            ",
        )?;

        // Preserve the authority of previously issued enrollment codes. Only a
        // newly issued explicit admin grant adds ACP/SSH scopes; old codes stay 0.
        {
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            if !table_columns(&tx, "pairing_codes")?
                .iter()
                .any(|column| column == "runner_capabilities")
            {
                tx.execute_batch("ALTER TABLE pairing_codes ADD COLUMN runner_capabilities INTEGER NOT NULL DEFAULT 0 CHECK(runner_capabilities IN (0, 1));")?;
            }
            tx.commit()?;
        }

        // ActionAudit predates Window correlation. Fresh databases already have
        // the current columns above; existing databases receive the same shape
        // through this additive, idempotent migration.
        Self::ensure_action_event_window_schema(&mut conn)?;
        Self::ensure_action_event_observability_views(&mut conn)?;

        // Durable Agent identity and Conversation state are an independent
        // communication domain. Workflow Session and project Memory ledgers
        // remain separate authoritative stores.
        Self::ensure_communication_schema(&mut conn)?;
        // AgentTask and AgentTaskAttempt are an independent durable work-ownership
        // domain. They reference durable Agents/Conversations for correlation only
        // and deliberately do not bind any execution backend in A3.
        Self::ensure_agent_task_schema(&mut conn)?;

        // AgentWait is a one-shot durable interest in future source facts. Sources
        // reference AgentTasks, while the Wait itself owns no source-domain authority.
        Self::ensure_agent_wait_schema(&mut conn)?;

        // Generic Job terminal attention has its own authority model and bounded
        // one-shot store; it deliberately does not reuse Durable Agent waits.
        Self::ensure_job_terminal_wait_schema(&mut conn)?;

        // Agent Wake is the shared durable continuation/outbox domain. Initialize it
        // after AgentTask and AgentWait so every source foreign key is enforceable.
        Self::ensure_agent_wake_schema(&mut conn)?;

        // Goal is independent high-level durable intent/control state. It may
        // correlate AgentTasks and Workflow Sessions, but owns no execution authority.
        Self::ensure_goal_schema(&mut conn)?;

        // Agent attention is a narrow semantic-fact domain. The first and only
        // event kind records terminal Goal-correlated AgentTask facts; it is not
        // a generic event bus and owns no scheduling or Goal authority.
        Self::ensure_agent_attention_schema(&mut conn)?;

        // Project Memory was introduced after v0.3.9. Only the current schema is
        // supported; development-only intermediate shapes are rejected.
        Self::ensure_project_memory_schema(&mut conn)?;

        Ok(())
    }

    fn ensure_action_event_window_schema(conn: &mut Connection) -> anyhow::Result<()> {
        const COLUMN_ADDITIONS: &[(&str, &str)] = &[
            ("client_window_key", "TEXT"),
            ("client_window_source", "TEXT"),
            ("server_trace_id", "TEXT"),
            ("principal_correlation_kind", "TEXT"),
            ("principal_correlation_id", "TEXT"),
            ("window_started_at_ms", "INTEGER"),
            ("window_ended_at_ms", "INTEGER"),
            ("request_observed_at_ms", "INTEGER"),
            ("response_handed_at_ms", "INTEGER"),
            ("window_transition_kind", "TEXT"),
            (
                "response_streaming",
                "INTEGER CHECK(response_streaming IS NULL OR response_streaming IN (0, 1))",
            ),
            (
                "window_continuity_eligible",
                "INTEGER CHECK(window_continuity_eligible IS NULL OR window_continuity_eligible IN (0, 1))",
            ),
            (
                "window_meaningful",
                "INTEGER NOT NULL DEFAULT 0 CHECK(window_meaningful IN (0, 1))",
            ),
            ("recorder_gap_session_id", "TEXT"),
        ];
        const CHILD_SCHEMA: &str = "
            CREATE TABLE IF NOT EXISTS action_event_workflow_links (
                event_id TEXT NOT NULL,
                workflow_session_id TEXT NOT NULL,
                workflow_session_relation TEXT NOT NULL
                    CHECK(workflow_session_relation IN ('recording', 'work_on_project')),
                project TEXT,
                linked_at_ms INTEGER NOT NULL,
                PRIMARY KEY(event_id, workflow_session_id, workflow_session_relation),
                FOREIGN KEY(event_id) REFERENCES action_events(event_id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_action_event_workflow_links_session
                ON action_event_workflow_links(workflow_session_id, linked_at_ms DESC);
            CREATE INDEX IF NOT EXISTS idx_action_events_window_started
                ON action_events(client_window_key, window_started_at_ms DESC)
                WHERE client_window_key IS NOT NULL;
            CREATE INDEX IF NOT EXISTS idx_action_events_window_completed
                ON action_events(client_window_key, window_ended_at_ms DESC, event_id DESC)
                WHERE window_started_at_ms IS NOT NULL AND window_ended_at_ms IS NOT NULL;
            CREATE INDEX IF NOT EXISTS idx_action_events_window_meaningful_completed
                ON action_events(client_window_key, window_ended_at_ms DESC, event_id DESC)
                WHERE window_meaningful = 1 AND window_started_at_ms IS NOT NULL AND window_ended_at_ms IS NOT NULL;

            CREATE TABLE IF NOT EXISTS window_peer_messages (
                message_id TEXT PRIMARY KEY,
                principal_kind TEXT NOT NULL,
                principal_id TEXT NOT NULL,
                sender_window_key TEXT NOT NULL,
                recipient_window_key TEXT NOT NULL,
                sender_peer_id TEXT NOT NULL,
                recipient_peer_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                priority TEXT NOT NULL,
                message TEXT NOT NULL,
                tags_json TEXT NOT NULL,
                requires_ack INTEGER NOT NULL CHECK(requires_ack IN (0, 1)),
                created_at_ms INTEGER NOT NULL,
                sender_session_id TEXT,
                sender_project TEXT,
                first_projected_at_ms INTEGER,
                last_projected_at_ms INTEGER,
                projection_count INTEGER NOT NULL DEFAULT 0,
                first_ack_observed_at_ms INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_window_peer_messages_recipient
                ON window_peer_messages(
                    principal_kind, principal_id, recipient_window_key,
                    requires_ack, first_projected_at_ms, created_at_ms
                );

            CREATE TABLE IF NOT EXISTS window_peer_discoveries (
                principal_kind TEXT NOT NULL,
                principal_id TEXT NOT NULL,
                observer_window_key TEXT NOT NULL,
                peer_window_key TEXT NOT NULL,
                project TEXT NOT NULL,
                first_projected_at_ms INTEGER NOT NULL,
                PRIMARY KEY(
                    principal_kind, principal_id, observer_window_key,
                    peer_window_key, project
                )
            );";

        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .context("begin ActionAudit Window schema migration")?;
        let mut columns = table_columns(&tx, "action_events")?;
        for (name, definition) in COLUMN_ADDITIONS {
            if columns.iter().any(|column| column == name) {
                continue;
            }
            tx.execute_batch(&format!(
                "ALTER TABLE action_events ADD COLUMN {name} {definition};"
            ))
            .with_context(|| format!("add ActionAudit Window column {name}"))?;
            columns.push((*name).to_string());
        }
        tx.execute_batch(CHILD_SCHEMA)
            .context("create ActionAudit Window correlation schema")?;
        tx.commit()
            .context("commit ActionAudit Window schema migration")?;
        Ok(())
    }

    fn ensure_action_event_observability_views(conn: &mut Connection) -> anyhow::Result<()> {
        // Views are derived observability contracts over the canonical ActionAudit row.
        // Creation is idempotent and introduces no second telemetry write path.
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_action_events_operation_started
                 ON action_events(operation, started_at DESC)
                 WHERE operation IS NOT NULL;
             CREATE VIEW IF NOT EXISTS code_mode_action_traces AS
             SELECT
                 event_id,
                 session_id AS action_session_id,
                 started_at,
                 ended_at,
                 duration_ms AS outer_duration_ms,
                 operation,
                 project,
                 status,
                 http_status,
                 principal_kind,
                 principal_user_id,
                 client_window_key,
                 client_window_source,
                 server_trace_id,
                 request_observed_at_ms,
                 response_handed_at_ms,
                 CASE
                     WHEN request_observed_at_ms IS NOT NULL
                      AND response_handed_at_ms IS NOT NULL
                      AND response_handed_at_ms >= request_observed_at_ms
                     THEN response_handed_at_ms - request_observed_at_ms
                 END AS service_ms,
                 window_transition_kind,
                 response_streaming,
                 window_continuity_eligible,
                 window_meaningful,
                 json_extract(summary_json, '$.model_ergonomics.duration_ms')
                     AS model_runtime_duration_ms,
                 json_extract(summary_json, '$.model_ergonomics.serialized_result_bytes')
                     AS serialized_result_bytes,
                 json_extract(summary_json, '$.model_ergonomics.context_recovery_bytes')
                     AS context_recovery_bytes,
                 json_extract(summary_json, '$.model_ergonomics.session_recovery_event_count')
                     AS session_recovery_event_count,
                 json_extract(summary_json, '$.model_ergonomics.session_recovery_truncated')
                     AS session_recovery_truncated,
                 json_extract(summary_json, '$.model_ergonomics.session_history_lost')
                     AS session_history_lost,
                 CASE
                     WHEN json_type(summary_json, '$.code_mode_composition') = 'object' THEN 1
                     ELSE 0
                 END AS composition_available,
                 json_extract(summary_json, '$.code_mode_composition.input_bytes') AS input_bytes,
                 json_extract(summary_json, '$.code_mode_composition.returned_bytes') AS returned_bytes,
                 json_extract(summary_json, '$.code_mode_composition.nested_raw_result_bytes_total')
                     AS nested_raw_result_bytes_total,
                 json_extract(summary_json, '$.code_mode_composition.nested_calls') AS nested_calls,
                 json_extract(summary_json, '$.code_mode_composition.nested_successes')
                     AS nested_successes,
                 json_extract(summary_json, '$.code_mode_composition.nested_failures')
                     AS nested_failures,
                 json_extract(summary_json, '$.code_mode_composition.max_in_flight')
                     AS max_in_flight,
                 json_extract(summary_json, '$.code_mode_composition.duration_ms')
                     AS composition_duration_ms,
                 json_extract(summary_json, '$.code_mode_composition.slot_wait_ms') AS slot_wait_ms,
                 json_extract(summary_json, '$.code_mode_composition.consequential_calls')
                     AS consequential_calls,
                 json_extract(summary_json, '$.code_mode_composition.known_results') AS known_results,
                 json_extract(summary_json, '$.code_mode_composition.job_handoffs') AS job_handoffs,
                 json_extract(summary_json, '$.code_mode_composition.outcome_unknown') AS outcome_unknown,
                 json_extract(summary_json, '$.code_mode_composition.nested_tool_counts')
                     AS nested_tool_counts_json
             FROM action_events
             WHERE operation IN (
                 'code_mode_exec', 'code_mode_exec_effectful', 'code_mode_exec_mutating'
             );
             CREATE VIEW IF NOT EXISTS code_mode_nested_tool_usage AS
             SELECT
                 traces.event_id,
                 traces.action_session_id,
                 traces.started_at,
                 traces.operation,
                 traces.project,
                 traces.status,
                 tools.key AS tool_name,
                 CAST(tools.value AS INTEGER) AS calls
             FROM code_mode_action_traces AS traces
             JOIN json_each(traces.nested_tool_counts_json) AS tools
             WHERE tools.key IS NOT NULL
               AND tools.type = 'integer'
               AND CAST(tools.value AS INTEGER) >= 0;",
        )
        .context("create ActionAudit observability views")?;
        Ok(())
    }

    fn ensure_project_memory_schema(conn: &mut Connection) -> anyhow::Result<()> {
        const CREATE_MEMORY_TABLE: &str = "
            CREATE TABLE project_memories (
                memory_id TEXT PRIMARY KEY,
                memory_scope_id TEXT NOT NULL,
                memory_key TEXT NOT NULL,
                summary TEXT NOT NULL,
                body TEXT NOT NULL,
                priority TEXT NOT NULL CHECK(priority IN ('high', 'normal', 'low')),
                bootstrap INTEGER NOT NULL CHECK(bootstrap IN (0, 1)),
                tags_json TEXT NOT NULL,
                definition_hash TEXT NOT NULL,
                generation INTEGER NOT NULL CHECK(generation >= 1),
                revision TEXT NOT NULL,
                created_at_unix_ms INTEGER NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL,
                created_by_kind TEXT NOT NULL,
                created_by_principal_digest TEXT NOT NULL,
                updated_by_kind TEXT NOT NULL,
                updated_by_principal_digest TEXT NOT NULL,
                UNIQUE(memory_scope_id, memory_key)
            );";
        const CREATE_SCOPE_TABLE: &str = "
            CREATE TABLE project_memory_scopes (
                memory_scope_id TEXT PRIMARY KEY,
                identity_state TEXT NOT NULL CHECK(identity_state = 'attributed'),
                project_runtime_id TEXT NOT NULL,
                runner_client_id TEXT NOT NULL,
                root_fingerprint TEXT NOT NULL,
                created_at_unix_ms INTEGER NOT NULL,
                last_mutated_at_unix_ms INTEGER NOT NULL
            );";
        const CREATE_INDEXES: &str = "
            CREATE INDEX IF NOT EXISTS idx_project_memories_scope_key
                ON project_memories(memory_scope_id, memory_key);
            CREATE INDEX IF NOT EXISTS idx_project_memories_scope_bootstrap
                ON project_memories(memory_scope_id, bootstrap, priority, memory_key);";

        let transaction = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .context("begin project Memory schema initialization")?;
        let memory_columns = table_columns(&transaction, "project_memories")?;
        let scope_columns = table_columns(&transaction, "project_memory_scopes")?;
        let memory_absent = memory_columns.is_empty();
        let scope_absent = scope_columns.is_empty();

        if memory_absent && scope_absent {
            transaction
                .execute_batch(CREATE_MEMORY_TABLE)
                .context("create current project Memory table")?;
            transaction
                .execute_batch(CREATE_SCOPE_TABLE)
                .context("create current project Memory scope table")?;
        } else if memory_absent || scope_absent {
            anyhow::bail!(
                "unsupported project Memory schema shape; recreate post-v0.3.9 development state"
            );
        } else {
            let memory_schema = table_schema_sql(&transaction, "project_memories")?;
            let scope_schema = table_schema_sql(&transaction, "project_memory_scopes")?;
            let memory_shape_matches = normalize_table_schema_sql(&memory_schema)
                == normalize_table_schema_sql(CREATE_MEMORY_TABLE);
            let scope_shape_matches = normalize_table_schema_sql(&scope_schema)
                == normalize_table_schema_sql(CREATE_SCOPE_TABLE);
            if !memory_shape_matches || !scope_shape_matches {
                anyhow::bail!(
                    "unsupported project Memory schema shape; recreate post-v0.3.9 development state"
                );
            }
        }

        transaction
            .execute_batch(CREATE_INDEXES)
            .context("create project Memory indexes")?;
        transaction
            .commit()
            .context("commit project Memory schema initialization")?;
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

fn table_schema_sql(conn: &Connection, table: &str) -> anyhow::Result<String> {
    conn.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |row| row.get(0),
    )
    .with_context(|| format!("read schema for table {table}"))
}

fn normalize_table_schema_sql(sql: &str) -> String {
    let mut normalized = String::with_capacity(sql.len());
    let mut chars = sql.chars().peekable();
    let mut in_single_quote = false;

    while let Some(ch) = chars.next() {
        if in_single_quote {
            normalized.push(ch);
            if ch == '\'' {
                if chars.peek() == Some(&'\'') {
                    normalized.push(chars.next().expect("peeked escaped single quote"));
                } else {
                    in_single_quote = false;
                }
            }
            continue;
        }

        match ch {
            '\'' => {
                in_single_quote = true;
                normalized.push(ch);
            }
            ch if ch.is_ascii_whitespace() => {}
            ch => normalized.push(ch.to_ascii_lowercase()),
        }
    }

    while normalized.ends_with(';') {
        normalized.pop();
    }
    normalized
}

#[cfg(test)]
mod schema_normalization_tests {
    use super::normalize_table_schema_sql;

    #[test]
    fn normalization_preserves_single_quoted_literal_semantics() {
        assert_eq!(
            normalize_table_schema_sql(
                "CREATE TABLE T (V TEXT CHECK(V = 'Keep Case  And  Space''s'));"
            ),
            "createtablet(vtextcheck(v='Keep Case  And  Space''s'))"
        );
        assert_ne!(
            normalize_table_schema_sql("CHECK(identity_state = 'attributed')"),
            normalize_table_schema_sql("CHECK(identity_state = 'ATTRIBUTED')")
        );
    }
}

#[cfg(test)]
mod action_event_window_migration_tests {
    use super::*;

    #[test]
    fn code_mode_observability_views_flatten_current_and_historical_rows() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("code-mode-observability.db");
        let db = Database::open(&path).unwrap();
        let conn = db.conn_for_tests();
        conn.execute(
            "INSERT INTO action_sessions (session_id, status, created_at, updated_at)
             VALUES ('audit-session', 'open', 1, 1)",
            [],
        )
        .unwrap();
        for (event_id, operation, status, summary_json) in [
            (
                "old-composition",
                "code_mode_exec",
                "success",
                r#"{"transport":"mcp","code_mode_composition":{"nested_calls":2,"nested_successes":2,"nested_failures":0,"max_in_flight":1,"duration_ms":10,"slot_wait_ms":0,"returned_bytes":120,"nested_raw_result_bytes_total":500,"nested_tool_counts":{"read_files":2},"consequential_calls":0,"known_results":0,"job_handoffs":0,"outcome_unknown":0}}"#,
            ),
            (
                "current-composition",
                "code_mode_exec_effectful",
                "success",
                r#"{"transport":"mcp","model_ergonomics":{"duration_ms":19,"serialized_result_bytes":777},"code_mode_composition":{"nested_calls":1,"nested_successes":1,"nested_failures":0,"max_in_flight":1,"duration_ms":20,"slot_wait_ms":3,"input_bytes":42,"returned_bytes":90,"nested_raw_result_bytes_total":250,"nested_tool_counts":{"cargo_check":1},"consequential_calls":1,"known_results":1,"job_handoffs":0,"outcome_unknown":0}}"#,
            ),
            (
                "pre-composition",
                "code_mode_exec_mutating",
                "failed",
                r#"{"transport":"mcp"}"#,
            ),
        ] {
            conn.execute(
                "INSERT INTO action_events (
                    event_id, session_id, started_at, ended_at, duration_ms,
                    endpoint, operation, action_name, status,
                    changed_files_json, ids_json, summary_json
                 ) VALUES (?1, 'audit-session', 10, 20, 10, '/mcp', ?2, 'toolsCall', ?3,
                           '[]', '{}', ?4)",
                rusqlite::params![event_id, operation, status, summary_json],
            )
            .unwrap();
        }

        let aggregate: (i64, i64, i64, i64) = conn
            .query_row(
                "SELECT count(*), sum(composition_available),
                        sum(coalesce(input_bytes, 0)), sum(coalesce(nested_calls, 0))
                 FROM code_mode_action_traces",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(aggregate, (3, 2, 42, 3));
        conn.execute(
            "UPDATE action_events
             SET request_observed_at_ms = 100, response_handed_at_ms = 140
             WHERE event_id = 'current-composition'",
            [],
        )
        .unwrap();
        let outer: (i64, i64, i64) = conn
            .query_row(
                "SELECT service_ms, model_runtime_duration_ms, serialized_result_bytes
                 FROM code_mode_action_traces WHERE event_id = 'current-composition'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(outer, (40, 19, 777));

        let mut statement = conn
            .prepare(
                "SELECT tool_name, calls FROM code_mode_nested_tool_usage
                 ORDER BY tool_name",
            )
            .unwrap();
        let usage = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            usage,
            vec![
                ("cargo_check".to_string(), 1),
                ("read_files".to_string(), 2)
            ]
        );
        drop(statement);
        drop(conn);
        drop(db);

        let reopened = Database::open(&path).unwrap();
        let conn = reopened.conn_for_tests();
        assert_eq!(
            conn.query_row("SELECT count(*) FROM code_mode_action_traces", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            3,
            "derived views must reopen idempotently without losing rows"
        );
    }

    #[test]
    fn legacy_action_events_upgrade_additively_and_remain_readable() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("legacy-window-audit.db");
        // Build every unrelated table with the current schema, then downgrade only
        // ActionAudit's event shape. This models an existing pre-Window database
        // without making this migration fixture responsible for unrelated schemas.
        drop(Database::open(&path).unwrap());
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "PRAGMA foreign_keys = OFF;
                 DROP TABLE action_event_workflow_links;
                 DROP TABLE action_events;
                 CREATE TABLE action_events (
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
                    response_bytes INTEGER
                 );
                 INSERT INTO action_events (
                    event_id, session_id, started_at, ended_at, duration_ms,
                    endpoint, operation, action_name, status,
                    changed_files_json, ids_json, summary_json
                 ) VALUES (
                    'legacy-event', 'legacy-session', 10, 11, 1000,
                    '/mcp', 'read_files', 'toolsCall', 'success',
                    '[]', '{}', '{}'
                 );",
            )
            .unwrap();
        }

        let db = Database::open(&path).unwrap();
        let rows = db.list_action_events("legacy-session", 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].event_id, "legacy-event");
        assert!(rows[0].client_window_key.is_none());
        assert!(rows[0].client_window_source.is_none());
        assert!(rows[0].server_trace_id.is_none());
        assert!(!rows[0].window_meaningful);
        assert!(rows[0].recorder_gap_session_id.is_none());
        {
            let conn = db.conn_for_tests();
            let columns = table_columns(&conn, "action_events").unwrap();
            for expected in [
                "client_window_key",
                "client_window_source",
                "server_trace_id",
                "principal_correlation_kind",
                "principal_correlation_id",
                "window_started_at_ms",
                "window_ended_at_ms",
                "window_meaningful",
                "recorder_gap_session_id",
            ] {
                assert!(
                    columns.iter().any(|column| column == expected),
                    "{expected}"
                );
            }
            assert!(!table_columns(&conn, "action_event_workflow_links")
                .unwrap()
                .is_empty());
        }
        drop(db);

        let reopened = Database::open(&path).unwrap();
        assert_eq!(
            reopened
                .list_action_events("legacy-session", 10)
                .unwrap()
                .len(),
            1,
            "migration must be idempotent and preserve legacy audit rows"
        );
    }
}
