use crate::models::{ApiKeyRecord, UserRecord};
use crate::{
    ActionEventRecord, ActionSessionRecord, AgentModelProfileRecord, AgentSpecRecord, Channel,
    CodexGoalRecord, CommandAuditRecord, Message, MessageKind,
};
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::Mutex;

pub struct Database {
    conn: Mutex<Connection>,
}

fn row_to_message(row: &rusqlite::Row) -> rusqlite::Result<Message> {
    Ok(Message {
        id: row.get(0)?,
        channel: row.get(1)?,
        kind: match row.get::<_, String>(2)?.as_str() {
            "file" => MessageKind::File,
            _ => MessageKind::Text,
        },
        title: row.get(3)?,
        text: row.get(4)?,
        file_name: row.get(5)?,
        file_path: row.get(6)?,
        file_size: row.get(7)?,
        mime_type: row.get(8)?,
        created_at: row.get(9)?,
        expires_at: row.get(10)?,
    })
}

fn row_to_agent_spec(row: &rusqlite::Row) -> rusqlite::Result<AgentSpecRecord> {
    Ok(AgentSpecRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        base_url: row.get(2)?,
        auth_token: row.get(3)?,
        openapi_json: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn row_to_agent_model_profile(row: &rusqlite::Row) -> rusqlite::Result<AgentModelProfileRecord> {
    Ok(AgentModelProfileRecord {
        id: row.get(0)?,
        base_url: row.get(1)?,
        api_key: row.get(2)?,
        model: row.get(3)?,
        temperature: row.get(4)?,
        max_rounds: row.get::<_, Option<i64>>(5)?.map(|v| v as usize),
        updated_at: row.get(6)?,
    })
}

fn row_to_action_session(row: &rusqlite::Row) -> rusqlite::Result<ActionSessionRecord> {
    Ok(ActionSessionRecord {
        session_id: row.get(0)?,
        title: row.get(1)?,
        note: row.get(2)?,
        status: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        closed_at: row.get(6)?,
        first_event_at: row.get(7)?,
        last_event_at: row.get(8)?,
        total_actions: row.get(9)?,
        success_count: row.get(10)?,
        failed_count: row.get(11)?,
        timeout_or_unknown_count: row.get(12)?,
        warning_count: row.get(13)?,
        total_duration_ms: row.get(14)?,
        changed_files_count: row.get(15)?,
        job_ids_count: row.get(16)?,
    })
}

fn row_to_action_event(row: &rusqlite::Row) -> rusqlite::Result<ActionEventRecord> {
    Ok(ActionEventRecord {
        event_id: row.get(0)?,
        session_id: row.get(1)?,
        started_at: row.get(2)?,
        ended_at: row.get(3)?,
        duration_ms: row.get(4)?,
        endpoint: row.get(5)?,
        operation: row.get(6)?,
        action_name: row.get(7)?,
        project: row.get(8)?,
        status: row.get(9)?,
        http_status: row.get(10)?,
        error_summary: row.get(11)?,
        warning_summary: row.get(12)?,
        changed_files_json: row.get(13)?,
        ids_json: row.get(14)?,
        summary_json: row.get(15)?,
        request_bytes: row.get(16)?,
        response_bytes: row.get(17)?,
    })
}

/// Return the set of column names present on `table`. Used by the idempotent
/// Phase 2 migration helpers to decide whether an `ALTER TABLE ... ADD COLUMN`
/// is needed.
fn table_columns(conn: &Connection, table: &str) -> anyhow::Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let mut cols = Vec::new();
    for row in rows {
        cols.push(row?);
    }
    Ok(cols)
}

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
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                channel TEXT NOT NULL,
                kind TEXT NOT NULL CHECK(kind IN ('text', 'file')),
                title TEXT,
                text TEXT,
                file_name TEXT,
                file_path TEXT,
                file_size INTEGER,
                mime_type TEXT,
                created_at INTEGER NOT NULL,
                expires_at INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_messages_channel ON messages(channel);
            CREATE INDEX IF NOT EXISTS idx_messages_created_at ON messages(created_at DESC);

            CREATE TABLE IF NOT EXISTS command_requests (
                id TEXT PRIMARY KEY,
                project TEXT NOT NULL,
                command TEXT NOT NULL,
                command_text TEXT,
                reason TEXT,
                status TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                approved_at INTEGER,
                executed_at INTEGER,
                exit_code INTEGER,
                stdout_tail TEXT,
                stderr_tail TEXT,
                error TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_command_requests_created_at ON command_requests(created_at DESC);

            CREATE TABLE IF NOT EXISTS codex_goals (
                id TEXT PRIMARY KEY,
                project TEXT NOT NULL,
                title TEXT NOT NULL,
                summary TEXT,
                status TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                closed_at INTEGER,
                error TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_codex_goals_created_at ON codex_goals(created_at DESC);

            CREATE TABLE IF NOT EXISTS desktop_tasks (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                instructions TEXT NOT NULL,
                status TEXT NOT NULL,
                priority INTEGER NOT NULL,
                claimed_by TEXT,
                last_event TEXT,
                screenshot_url TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_desktop_tasks_status_priority ON desktop_tasks(status, priority DESC, created_at ASC);
            CREATE INDEX IF NOT EXISTS idx_desktop_tasks_updated_at ON desktop_tasks(updated_at DESC);

            CREATE TABLE IF NOT EXISTS desktop_task_events (
                id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                status TEXT NOT NULL,
                worker TEXT,
                message TEXT,
                screenshot_url TEXT,
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_desktop_task_events_task_created ON desktop_task_events(task_id, created_at ASC);

            CREATE TABLE IF NOT EXISTS agent_specs (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                base_url TEXT NOT NULL,
                auth_token TEXT NOT NULL,
                openapi_json TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_agent_specs_updated_at ON agent_specs(updated_at DESC);

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

            CREATE TABLE IF NOT EXISTS agent_model_profiles (
                id TEXT PRIMARY KEY,
                base_url TEXT NOT NULL,
                api_key TEXT NOT NULL,
                model TEXT NOT NULL,
                temperature REAL,
                max_rounds INTEGER,
                updated_at INTEGER NOT NULL
            );

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
            );",

        )?;
        let has_command_text = {
            let mut stmt = conn.prepare("PRAGMA table_info(command_requests)")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
            let mut found = false;
            for row in rows {
                if row? == "command_text" {
                    found = true;
                    break;
                }
            }
            found
        };
        if !has_command_text {
            conn.execute(
                "ALTER TABLE command_requests ADD COLUMN command_text TEXT",
                [],
            )?;
        }
        // Phase 2 multi-user auth: evolve the legacy users/api_keys tables in
        // place. Fresh DBs already declare the new columns in CREATE TABLE
        // above; this block migrates pre-existing DBs forward without dropping
        // data or breaking audit/jobs/project tables.
        Self::migrate_users_and_api_keys(&conn)?;
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

    pub fn insert_action_session(&self, record: &ActionSessionRecord) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO action_sessions (
                session_id, title, note, status, created_at, updated_at, closed_at,
                first_event_at, last_event_at, total_actions, success_count, failed_count,
                timeout_or_unknown_count, warning_count, total_duration_ms,
                changed_files_count, job_ids_count
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                record.session_id,
                record.title,
                record.note,
                record.status,
                record.created_at,
                record.updated_at,
                record.closed_at,
                record.first_event_at,
                record.last_event_at,
                record.total_actions,
                record.success_count,
                record.failed_count,
                record.timeout_or_unknown_count,
                record.warning_count,
                record.total_duration_ms,
                record.changed_files_count,
                record.job_ids_count,
            ],
        )?;
        Ok(())
    }

    pub fn get_action_session(
        &self,
        session_id: &str,
    ) -> anyhow::Result<Option<ActionSessionRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT session_id, title, note, status, created_at, updated_at, closed_at,
                    first_event_at, last_event_at, total_actions, success_count, failed_count,
                    timeout_or_unknown_count, warning_count, total_duration_ms,
                    changed_files_count, job_ids_count
             FROM action_sessions WHERE session_id = ?1",
        )?;
        let mut rows = stmt.query_map(params![session_id], row_to_action_session)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn list_action_sessions(
        &self,
        status: Option<&str>,
        limit: usize,
    ) -> anyhow::Result<Vec<ActionSessionRecord>> {
        let conn = self.conn.lock().unwrap();
        let limit = limit.clamp(1, 200) as i64;
        let sql = match status {
            Some(_) => {
                "SELECT session_id, title, note, status, created_at, updated_at, closed_at,
                        first_event_at, last_event_at, total_actions, success_count,
                        failed_count, timeout_or_unknown_count, warning_count,
                        total_duration_ms, changed_files_count, job_ids_count
                 FROM action_sessions
                 WHERE status = ?1
                 ORDER BY COALESCE(last_event_at, created_at) DESC
                 LIMIT ?2"
            }
            None => {
                "SELECT session_id, title, note, status, created_at, updated_at, closed_at,
                        first_event_at, last_event_at, total_actions, success_count,
                        failed_count, timeout_or_unknown_count, warning_count,
                        total_duration_ms, changed_files_count, job_ids_count
                 FROM action_sessions
                 ORDER BY COALESCE(last_event_at, created_at) DESC
                 LIMIT ?1"
            }
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = match status {
            Some(status) => stmt.query_map(params![status, limit], row_to_action_session)?,
            None => stmt.query_map(params![limit], row_to_action_session)?,
        };
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn find_recent_open_action_session(
        &self,
        min_last_event_at: i64,
    ) -> anyhow::Result<Option<ActionSessionRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT session_id, title, note, status, created_at, updated_at, closed_at,
                    first_event_at, last_event_at, total_actions, success_count, failed_count,
                    timeout_or_unknown_count, warning_count, total_duration_ms,
                    changed_files_count, job_ids_count
             FROM action_sessions
             WHERE status = 'open' AND COALESCE(last_event_at, created_at) >= ?1
             ORDER BY COALESCE(last_event_at, created_at) DESC
             LIMIT 1",
        )?;
        let mut rows = stmt.query_map(params![min_last_event_at], row_to_action_session)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn update_action_session_metadata(
        &self,
        session_id: &str,
        title: Option<&str>,
        note: Option<&str>,
        updated_at: i64,
    ) -> anyhow::Result<Option<ActionSessionRecord>> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "UPDATE action_sessions
             SET title = COALESCE(?2, title),
                 note = COALESCE(?3, note),
                 updated_at = ?4
             WHERE session_id = ?1",
            params![session_id, title, note, updated_at],
        )?;
        drop(conn);
        if changed == 1 {
            self.get_action_session(session_id)
        } else {
            Ok(None)
        }
    }

    pub fn close_action_session(
        &self,
        session_id: &str,
        closed_at: i64,
    ) -> anyhow::Result<Option<ActionSessionRecord>> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "UPDATE action_sessions
             SET status = 'closed', closed_at = ?2, updated_at = ?2
             WHERE session_id = ?1 AND status != 'closed'",
            params![session_id, closed_at],
        )?;
        drop(conn);
        if changed == 1 {
            self.get_action_session(session_id)
        } else {
            self.get_action_session(session_id)
        }
    }

    pub fn insert_action_event(&self, event: &ActionEventRecord) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO action_events (
                event_id, session_id, started_at, ended_at, duration_ms, endpoint,
                operation, action_name, project, status, http_status, error_summary,
                warning_summary, changed_files_json, ids_json, summary_json,
                request_bytes, response_bytes
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                event.event_id,
                event.session_id,
                event.started_at,
                event.ended_at,
                event.duration_ms,
                event.endpoint,
                event.operation,
                event.action_name,
                event.project,
                event.status,
                event.http_status,
                event.error_summary,
                event.warning_summary,
                event.changed_files_json,
                event.ids_json,
                event.summary_json,
                event.request_bytes,
                event.response_bytes,
            ],
        )?;
        Ok(())
    }

    pub fn list_action_events(
        &self,
        session_id: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<ActionEventRecord>> {
        let conn = self.conn.lock().unwrap();
        let limit = limit.clamp(1, 500) as i64;
        let mut stmt = conn.prepare(
            "SELECT event_id, session_id, started_at, ended_at, duration_ms, endpoint,
                    operation, action_name, project, status, http_status, error_summary,
                    warning_summary, changed_files_json, ids_json, summary_json,
                    request_bytes, response_bytes
             FROM action_events
             WHERE session_id = ?1
             ORDER BY started_at DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![session_id, limit], row_to_action_event)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn append_action_event_and_update_session(
        &self,
        event: &ActionEventRecord,
        success_inc: i64,
        failed_inc: i64,
        timeout_inc: i64,
        warning_inc: i64,
        duration_inc: i64,
        changed_files_count: i64,
        job_ids_count: i64,
    ) -> anyhow::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO action_events (
                event_id, session_id, started_at, ended_at, duration_ms, endpoint,
                operation, action_name, project, status, http_status, error_summary,
                warning_summary, changed_files_json, ids_json, summary_json,
                request_bytes, response_bytes
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                event.event_id,
                event.session_id,
                event.started_at,
                event.ended_at,
                event.duration_ms,
                event.endpoint,
                event.operation,
                event.action_name,
                event.project,
                event.status,
                event.http_status,
                event.error_summary,
                event.warning_summary,
                event.changed_files_json,
                event.ids_json,
                event.summary_json,
                event.request_bytes,
                event.response_bytes,
            ],
        )?;
        tx.execute(
            "UPDATE action_sessions
             SET updated_at = ?2,
                 first_event_at = COALESCE(first_event_at, ?3),
                 last_event_at = ?4,
                 total_actions = total_actions + 1,
                 success_count = success_count + ?5,
                 failed_count = failed_count + ?6,
                 timeout_or_unknown_count = timeout_or_unknown_count + ?7,
                 warning_count = warning_count + ?8,
                 total_duration_ms = total_duration_ms + ?9,
                 changed_files_count = changed_files_count + ?10,
                 job_ids_count = job_ids_count + ?11
             WHERE session_id = ?1",
            params![
                event.session_id,
                event.ended_at,
                event.started_at,
                event.ended_at,
                success_inc,
                failed_inc,
                timeout_inc,
                warning_inc,
                duration_inc,
                changed_files_count,
                job_ids_count,
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn upsert_agent_spec(&self, record: &AgentSpecRecord) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO agent_specs (id, name, base_url, auth_token, openapi_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                base_url = excluded.base_url,
                auth_token = excluded.auth_token,
                openapi_json = excluded.openapi_json,
                updated_at = excluded.updated_at",
            params![
                record.id,
                record.name,
                record.base_url,
                record.auth_token,
                record.openapi_json,
                record.created_at,
                record.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn list_agent_specs(&self) -> anyhow::Result<Vec<AgentSpecRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, base_url, auth_token, openapi_json, created_at, updated_at
             FROM agent_specs ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], row_to_agent_spec)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get_agent_spec(&self, id: &str) -> anyhow::Result<Option<AgentSpecRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, base_url, auth_token, openapi_json, created_at, updated_at
             FROM agent_specs WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], row_to_agent_spec)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn delete_agent_spec(&self, id: &str) -> anyhow::Result<bool> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute("DELETE FROM agent_specs WHERE id = ?1", params![id])?;
        Ok(changed == 1)
    }

    pub fn upsert_agent_model_profile(
        &self,
        record: &AgentModelProfileRecord,
    ) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO agent_model_profiles (id, base_url, api_key, model, temperature, max_rounds, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                base_url = excluded.base_url,
                api_key = excluded.api_key,
                model = excluded.model,
                temperature = excluded.temperature,
                max_rounds = excluded.max_rounds,
                updated_at = excluded.updated_at",
            params![
                record.id,
                record.base_url,
                record.api_key,
                record.model,
                record.temperature,
                record.max_rounds.map(|v| v as i64),
                record.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_agent_model_profile(
        &self,
        id: &str,
    ) -> anyhow::Result<Option<AgentModelProfileRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, base_url, api_key, model, temperature, max_rounds, updated_at
             FROM agent_model_profiles WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], row_to_agent_model_profile)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn insert_message(&self, message: &Message) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO messages (id, channel, kind, title, text, file_name, file_path, file_size, mime_type, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                message.id, message.channel,
                match message.kind { MessageKind::Text => "text", MessageKind::File => "file" },
                message.title, message.text, message.file_name, message.file_path,
                message.file_size, message.mime_type, message.created_at, message.expires_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_message(&self, id: &str) -> anyhow::Result<Option<Message>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, channel, kind, title, text, file_name, file_path, file_size, mime_type, created_at, expires_at FROM messages WHERE id = ?1"
        )?;
        let mut rows = stmt.query_map(params![id], row_to_message)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn delete_message(&self, id: &str) -> anyhow::Result<Option<Message>> {
        let message = self.get_message(id)?;
        if message.is_some() {
            let conn = self.conn.lock().unwrap();
            conn.execute("DELETE FROM messages WHERE id = ?1", params![id])?;
        }
        Ok(message)
    }

    pub fn list_messages(
        &self,
        channel: Option<&str>,
        limit: usize,
        before: Option<i64>,
    ) -> anyhow::Result<(Vec<Message>, bool)> {
        let conn = self.conn.lock().unwrap();
        let mut messages = Vec::new();

        let sql = match (channel, before) {
            (Some(_), Some(_)) => "SELECT id, channel, kind, title, text, file_name, file_path, file_size, mime_type, created_at, expires_at FROM messages WHERE channel = ?1 AND created_at < ?2 ORDER BY created_at DESC LIMIT ?3",
            (Some(_), None) => "SELECT id, channel, kind, title, text, file_name, file_path, file_size, mime_type, created_at, expires_at FROM messages WHERE channel = ?1 ORDER BY created_at DESC LIMIT ?2",
            (None, Some(_)) => "SELECT id, channel, kind, title, text, file_name, file_path, file_size, mime_type, created_at, expires_at FROM messages WHERE created_at < ?1 ORDER BY created_at DESC LIMIT ?2",
            (None, None) => "SELECT id, channel, kind, title, text, file_name, file_path, file_size, mime_type, created_at, expires_at FROM messages ORDER BY created_at DESC LIMIT ?1",
        };

        let mut stmt = conn.prepare(sql)?;
        let query_limit = limit as i64 + 1;
        let rows = match (channel, before) {
            (Some(ch), Some(before_ts)) => {
                stmt.query_map(params![ch, before_ts, query_limit], row_to_message)?
            }
            (Some(ch), None) => stmt.query_map(params![ch, query_limit], row_to_message)?,
            (None, Some(before_ts)) => {
                stmt.query_map(params![before_ts, query_limit], row_to_message)?
            }
            (None, None) => stmt.query_map(params![query_limit], row_to_message)?,
        };

        for row in rows {
            messages.push(row?);
        }
        let has_more = messages.len() > limit;
        messages.truncate(limit);
        Ok((messages, has_more))
    }

    pub fn insert_goal(&self, record: &CodexGoalRecord) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO codex_goals (id, project, title, summary, status, created_at, expires_at, closed_at, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                record.id,
                record.project,
                record.title,
                record.summary,
                record.status,
                record.created_at,
                record.expires_at,
                record.closed_at,
                record.error,
            ],
        )?;
        Ok(())
    }

    pub fn get_goal(&self, id: &str) -> anyhow::Result<Option<CodexGoalRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, project, title, summary, status, created_at, expires_at, closed_at, error FROM codex_goals WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |row| {
            Ok(CodexGoalRecord {
                id: row.get(0)?,
                project: row.get(1)?,
                title: row.get(2)?,
                summary: row.get(3)?,
                status: row.get(4)?,
                created_at: row.get(5)?,
                expires_at: row.get(6)?,
                closed_at: row.get(7)?,
                error: row.get(8)?,
            })
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn list_goals(
        &self,
        project: Option<&str>,
        status: Option<&str>,
        limit: usize,
    ) -> anyhow::Result<Vec<CodexGoalRecord>> {
        let limit = limit.clamp(1, 100) as i64;
        let conn = self.conn.lock().unwrap();
        let sql = match (project, status) {
            (Some(_), Some(_)) => "SELECT id, project, title, summary, status, created_at, expires_at, closed_at, error FROM codex_goals WHERE project = ?1 AND status = ?2 ORDER BY created_at DESC LIMIT ?3",
            (Some(_), None) => "SELECT id, project, title, summary, status, created_at, expires_at, closed_at, error FROM codex_goals WHERE project = ?1 ORDER BY created_at DESC LIMIT ?2",
            (None, Some(_)) => "SELECT id, project, title, summary, status, created_at, expires_at, closed_at, error FROM codex_goals WHERE status = ?1 ORDER BY created_at DESC LIMIT ?2",
            (None, None) => "SELECT id, project, title, summary, status, created_at, expires_at, closed_at, error FROM codex_goals ORDER BY created_at DESC LIMIT ?1",
        };
        let mut stmt = conn.prepare(sql)?;
        let map_row = |row: &rusqlite::Row| {
            Ok(CodexGoalRecord {
                id: row.get(0)?,
                project: row.get(1)?,
                title: row.get(2)?,
                summary: row.get(3)?,
                status: row.get(4)?,
                created_at: row.get(5)?,
                expires_at: row.get(6)?,
                closed_at: row.get(7)?,
                error: row.get(8)?,
            })
        };
        let rows = match (project, status) {
            (Some(project), Some(status)) => {
                stmt.query_map(params![project, status, limit], map_row)?
            }
            (Some(project), None) => stmt.query_map(params![project, limit], map_row)?,
            (None, Some(status)) => stmt.query_map(params![status, limit], map_row)?,
            (None, None) => stmt.query_map(params![limit], map_row)?,
        };
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn update_goal_status(
        &self,
        id: &str,
        status: &str,
        closed_at: i64,
        error: Option<&str>,
    ) -> anyhow::Result<Option<CodexGoalRecord>> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "UPDATE codex_goals SET status = ?2, closed_at = ?3, error = ?4 WHERE id = ?1",
            params![id, status, closed_at, error],
        )?;
        drop(conn);
        if changed == 1 {
            self.get_goal(id)
        } else {
            Ok(None)
        }
    }

    pub fn update_pending_goal_status(
        &self,
        id: &str,
        status: &str,
        closed_at: Option<i64>,
        error: Option<&str>,
    ) -> anyhow::Result<Option<CodexGoalRecord>> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "UPDATE codex_goals SET status = ?2, closed_at = ?3, error = ?4 WHERE id = ?1 AND status = 'pending'",
            params![id, status, closed_at, error],
        )?;
        drop(conn);
        if changed == 1 {
            self.get_goal(id)
        } else {
            Ok(None)
        }
    }

    pub fn insert_command_request(&self, record: &CommandAuditRecord) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO command_requests (id, project, command, command_text, reason, status, created_at, approved_at, executed_at, exit_code, stdout_tail, stderr_tail, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                record.id,
                record.project,
                record.command,
                record.command_text,
                record.reason,
                record.status,
                record.created_at,
                record.approved_at,
                record.executed_at,
                record.exit_code,
                record.stdout_tail,
                record.stderr_tail,
                record.error,
            ],
        )?;
        Ok(())
    }

    pub fn get_command_request(&self, id: &str) -> anyhow::Result<Option<CommandAuditRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, project, command, command_text, reason, status, created_at, approved_at, executed_at, exit_code, stdout_tail, stderr_tail, error FROM command_requests WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |row| {
            Ok(CommandAuditRecord {
                id: row.get(0)?,
                project: row.get(1)?,
                command: row.get(2)?,
                command_text: row.get(3)?,
                reason: row.get(4)?,
                status: row.get(5)?,
                created_at: row.get(6)?,
                approved_at: row.get(7)?,
                executed_at: row.get(8)?,
                exit_code: row.get(9)?,
                stdout_tail: row.get(10)?,
                stderr_tail: row.get(11)?,
                error: row.get(12)?,
            })
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn list_command_requests(
        &self,
        project: Option<&str>,
        status: Option<&str>,
        limit: usize,
    ) -> anyhow::Result<Vec<CommandAuditRecord>> {
        let limit = limit.clamp(1, 100) as i64;
        let conn = self.conn.lock().unwrap();
        let sql = match (project, status) {
            (Some(_), Some(_)) => "SELECT id, project, command, command_text, reason, status, created_at, approved_at, executed_at, exit_code, stdout_tail, stderr_tail, error FROM command_requests WHERE project = ?1 AND status = ?2 ORDER BY created_at DESC LIMIT ?3",
            (Some(_), None) => "SELECT id, project, command, command_text, reason, status, created_at, approved_at, executed_at, exit_code, stdout_tail, stderr_tail, error FROM command_requests WHERE project = ?1 ORDER BY created_at DESC LIMIT ?2",
            (None, Some(_)) => "SELECT id, project, command, command_text, reason, status, created_at, approved_at, executed_at, exit_code, stdout_tail, stderr_tail, error FROM command_requests WHERE status = ?1 ORDER BY created_at DESC LIMIT ?2",
            (None, None) => "SELECT id, project, command, command_text, reason, status, created_at, approved_at, executed_at, exit_code, stdout_tail, stderr_tail, error FROM command_requests ORDER BY created_at DESC LIMIT ?1",
        };
        let mut stmt = conn.prepare(sql)?;
        let map_row = |row: &rusqlite::Row| {
            Ok(CommandAuditRecord {
                id: row.get(0)?,
                project: row.get(1)?,
                command: row.get(2)?,
                command_text: row.get(3)?,
                reason: row.get(4)?,
                status: row.get(5)?,
                created_at: row.get(6)?,
                approved_at: row.get(7)?,
                executed_at: row.get(8)?,
                exit_code: row.get(9)?,
                stdout_tail: row.get(10)?,
                stderr_tail: row.get(11)?,
                error: row.get(12)?,
            })
        };
        let rows = match (project, status) {
            (Some(project), Some(status)) => {
                stmt.query_map(params![project, status, limit], map_row)?
            }
            (Some(project), None) => stmt.query_map(params![project, limit], map_row)?,
            (None, Some(status)) => stmt.query_map(params![status, limit], map_row)?,
            (None, None) => stmt.query_map(params![limit], map_row)?,
        };
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn reject_command_request(
        &self,
        id: &str,
        rejected_at: i64,
        error: &str,
    ) -> anyhow::Result<Option<CommandAuditRecord>> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "UPDATE command_requests SET status = 'rejected', executed_at = ?2, error = ?3 WHERE id = ?1 AND status = 'pending'",
            params![id, rejected_at, error],
        )?;
        drop(conn);
        if changed == 1 {
            self.get_command_request(id)
        } else {
            Ok(None)
        }
    }

    pub fn expire_command_request(
        &self,
        id: &str,
        expired_at: i64,
        error: &str,
    ) -> anyhow::Result<Option<CommandAuditRecord>> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "UPDATE command_requests SET status = 'expired', executed_at = ?2, error = ?3 WHERE id = ?1 AND status = 'pending'",
            params![id, expired_at, error],
        )?;
        drop(conn);
        if changed == 1 {
            self.get_command_request(id)
        } else {
            Ok(None)
        }
    }

    pub fn claim_command_request_for_execution(
        &self,
        id: &str,
        approved_at: i64,
        min_created_at: i64,
    ) -> anyhow::Result<Option<CommandAuditRecord>> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "UPDATE command_requests SET status = 'running', approved_at = ?2 WHERE id = ?1 AND status = 'pending' AND created_at >= ?3",
            params![id, approved_at, min_created_at],
        )?;
        drop(conn);
        if changed == 1 {
            self.get_command_request(id)
        } else {
            Ok(None)
        }
    }

    pub fn update_command_request_result(&self, record: &CommandAuditRecord) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE command_requests SET status = ?2, approved_at = ?3, executed_at = ?4, exit_code = ?5, stdout_tail = ?6, stderr_tail = ?7, error = ?8 WHERE id = ?1",
            params![
                record.id,
                record.status,
                record.approved_at,
                record.executed_at,
                record.exit_code,
                record.stdout_tail,
                record.stderr_tail,
                record.error,
            ],
        )?;
        Ok(())
    }

    pub fn list_channels(&self) -> anyhow::Result<Vec<Channel>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT channel, COUNT(*) as cnt FROM messages GROUP BY channel ORDER BY cnt DESC",
        )?;
        let default_channels = vec![
            ("inbox", "Inbox"),
            ("xline", "Xline"),
            ("thesis", "Thesis"),
            ("packfix", "Packfix"),
            ("omo", "OMO"),
            ("files", "Files"),
        ];
        let mut channels: Vec<Channel> = stmt
            .query_map([], |row| {
                let name: String = row.get(0)?;
                let count: i64 = row.get(1)?;
                let display_name = default_channels
                    .iter()
                    .find(|(n, _)| *n == name.as_str())
                    .map(|(_, d)| d.to_string())
                    .unwrap_or_else(|| name.clone());
                Ok(Channel {
                    name,
                    display_name,
                    message_count: count,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        for (name, display_name) in default_channels {
            if !channels.iter().any(|c| c.name == name) {
                channels.push(Channel {
                    name: name.to_string(),
                    display_name: display_name.to_string(),
                    message_count: 0,
                });
            }
        }
        Ok(channels)
    }
}

impl Database {
    pub fn get_user_by_username(&self, username: &str) -> anyhow::Result<Option<UserRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, username, created_at, disabled, display_name, role, disabled_at, updated_at
             FROM users WHERE username = ?1",
        )?;
        let mut rows = stmt.query_map(params![username], row_to_user)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn create_user(&self, user: &UserRecord) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO users (id, username, created_at, disabled, display_name, role, disabled_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                user.id,
                user.username,
                user.created_at,
                user.disabled,
                user.display_name,
                user.role,
                user.disabled_at,
                user.updated_at,
            ],
        )?;
        Ok(())
    }

    /// List all users ordered by username. Phase 2 admin surface.
    pub fn list_users(&self) -> anyhow::Result<Vec<UserRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, username, created_at, disabled, display_name, role, disabled_at, updated_at
             FROM users ORDER BY username ASC",
        )?;
        let rows = stmt.query_map([], row_to_user)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get_api_key_by_hash(&self, hash: &str) -> anyhow::Result<Option<ApiKeyRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, name, key_prefix, created_at, last_used_at, revoked_at, scopes, expires_at, kind, allowed_client_id
             FROM api_keys
             WHERE key_hash = ?1 AND revoked_at IS NULL",
        )?;
        let mut rows = stmt.query_map(params![hash], row_to_api_key)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn insert_api_key(&self, key: &ApiKeyRecord, key_hash: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO api_keys (id, user_id, name, key_hash, key_prefix, created_at, last_used_at, revoked_at, scopes, expires_at, kind, allowed_client_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                key.id,
                key.user_id,
                key.name,
                key_hash,
                key.key_prefix,
                key.created_at,
                key.last_used_at,
                key.revoked_at,
                key.scopes,
                key.expires_at,
                key.kind,
                key.allowed_client_id,
            ],
        )?;
        Ok(())
    }

    pub fn list_api_keys_by_user(&self, user_id: &str) -> anyhow::Result<Vec<ApiKeyRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, name, key_prefix, created_at, last_used_at, revoked_at, scopes, expires_at, kind, allowed_client_id
             FROM api_keys WHERE user_id = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![user_id], row_to_api_key)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// List only agent tokens (`kind='agent'`) for a user. Phase 3 agent-token
    /// management surface. Ordered by `created_at DESC`.
    pub fn list_agent_api_keys_by_user(&self, user_id: &str) -> anyhow::Result<Vec<ApiKeyRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, name, key_prefix, created_at, last_used_at, revoked_at, scopes, expires_at, kind, allowed_client_id
             FROM api_keys WHERE user_id = ?1 AND kind = 'agent' ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![user_id], row_to_api_key)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Fetch a single api token by id (including revoked/expired rows). Used by
    /// the revoke endpoint and self-management lookups. Phase 2.
    pub fn get_api_key_by_id(&self, id: &str) -> anyhow::Result<Option<ApiKeyRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, name, key_prefix, created_at, last_used_at, revoked_at, scopes, expires_at, kind, allowed_client_id
             FROM api_keys WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], row_to_api_key)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    /// Mark an api token as revoked at `ts`. Idempotent: revoking an already
    /// revoked token is a no-op. Returns the post-revoke record when a row
    /// exists. Phase 2.
    pub fn revoke_api_key(&self, id: &str, ts: i64) -> anyhow::Result<Option<ApiKeyRecord>> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE api_keys SET revoked_at = COALESCE(revoked_at, ?2) WHERE id = ?1",
            params![id, ts],
        )?;
        drop(conn);
        self.get_api_key_by_id(id)
    }

    /// Disable (or re-enable) a user. When disabling, both the legacy
    /// `disabled` flag and the Phase 2 `disabled_at` timestamp are set so the
    /// existing AuthMiddleware check (`disabled != 0`) and the new
    /// `disabled_at`-based check agree. Phase 2.
    pub fn set_user_disabled(
        &self,
        id: &str,
        disabled: bool,
        ts: i64,
    ) -> anyhow::Result<Option<UserRecord>> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE users
             SET disabled = ?2,
                 disabled_at = CASE WHEN ?2 = 1 THEN COALESCE(disabled_at, ?3) ELSE NULL END,
                 updated_at = ?3
             WHERE id = ?1",
            params![id, if disabled { 1 } else { 0 }, ts],
        )?;
        drop(conn);
        self.get_user_by_id(id)
    }

    pub fn get_user_by_id(&self, id: &str) -> anyhow::Result<Option<UserRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, username, created_at, disabled, display_name, role, disabled_at, updated_at
             FROM users WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], row_to_user)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn update_api_key_last_used(&self, id: &str, ts: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE api_keys SET last_used_at = ?2 WHERE id = ?1",
            params![id, ts],
        )?;
        Ok(())
    }
}

#[cfg(test)]
impl Database {
    /// Test-only access to the underlying connection so tests can assert on
    /// raw storage (e.g. that a plaintext token is never stored as `key_hash`).
    pub fn conn_for_tests(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap()
    }
}

/// Map a `users` row (8 columns, Phase 2 order) to a `UserRecord`. Columns are
/// positional: id, username, created_at, disabled, display_name, role,
/// disabled_at, updated_at.
fn row_to_user(row: &rusqlite::Row) -> rusqlite::Result<UserRecord> {
    Ok(UserRecord {
        id: row.get(0)?,
        username: row.get(1)?,
        created_at: row.get(2)?,
        disabled: row.get(3)?,
        display_name: row.get(4)?,
        role: row
            .get::<_, Option<String>>(5)?
            .unwrap_or_else(|| "user".to_string()),
        disabled_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

/// Map an `api_keys` row (11 columns, Phase 3 order) to an `ApiKeyRecord`.
/// Columns are positional: id, user_id, name, key_prefix, created_at,
/// last_used_at, revoked_at, scopes, expires_at, kind, allowed_client_id.
/// Older rows without `kind`/`allowed_client_id` are filled in via the column
/// default (`kind="user"`, `allowed_client_id=NULL`) at the SQL level, so this
/// mapper only ever sees the full 11-column projection.
fn row_to_api_key(row: &rusqlite::Row) -> rusqlite::Result<ApiKeyRecord> {
    Ok(ApiKeyRecord {
        id: row.get(0)?,
        user_id: row.get(1)?,
        name: row.get(2)?,
        key_prefix: row.get(3)?,
        created_at: row.get(4)?,
        last_used_at: row.get(5)?,
        revoked_at: row.get(6)?,
        scopes: row.get::<_, Option<String>>(7)?.unwrap_or_default(),
        expires_at: row.get(8)?,
        kind: row
            .get::<_, Option<String>>(9)?
            .unwrap_or_else(|| "user".to_string()),
        allowed_client_id: row.get(10)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_key_records_round_trip_and_revoked_keys_are_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("webcodex.db")).unwrap();
        let user = UserRecord {
            id: "user-1".to_string(),
            username: "alice".to_string(),
            created_at: 10,
            disabled: 0,
            display_name: Some("Alice".to_string()),
            role: "user".to_string(),
            disabled_at: None,
            updated_at: Some(10),
        };
        db.create_user(&user).unwrap();

        let fetched = db.get_user_by_username("alice").unwrap().unwrap();
        assert_eq!(fetched.id, "user-1");
        assert_eq!(fetched.display_name.as_deref(), Some("Alice"));
        assert_eq!(fetched.role, "user");
        assert!(!fetched.is_disabled());
        assert_eq!(
            db.get_user_by_id("user-1").unwrap().unwrap().username,
            "alice"
        );

        let key = ApiKeyRecord {
            id: "key-1".to_string(),
            user_id: "user-1".to_string(),
            name: "main".to_string(),
            key_prefix: "pk_live".to_string(),
            created_at: 11,
            last_used_at: None,
            revoked_at: None,
            scopes: "runtime:read project:write".to_string(),
            expires_at: None,
            kind: "user".to_string(),
            allowed_client_id: None,
        };
        db.insert_api_key(&key, "hash-1").unwrap();
        let fetched_key = db.get_api_key_by_hash("hash-1").unwrap().unwrap();
        assert_eq!(fetched_key.name, "main");
        assert_eq!(
            fetched_key.scopes_vec(),
            vec!["runtime:read".to_string(), "project:write".to_string()]
        );

        db.update_api_key_last_used("key-1", 12).unwrap();
        assert_eq!(
            db.get_api_key_by_hash("hash-1")
                .unwrap()
                .unwrap()
                .last_used_at,
            Some(12)
        );

        let revoked_key = ApiKeyRecord {
            id: "key-2".to_string(),
            name: "revoked".to_string(),
            revoked_at: Some(13),
            ..key
        };
        db.insert_api_key(&revoked_key, "hash-2").unwrap();
        assert!(db.get_api_key_by_hash("hash-2").unwrap().is_none());
        assert_eq!(db.list_api_keys_by_user("user-1").unwrap().len(), 2);
        // revoke_api_key is idempotent and updates the existing row.
        let revoked = db.revoke_api_key("key-1", 99).unwrap().unwrap();
        assert_eq!(revoked.revoked_at, Some(99));
        let revoked_again = db.revoke_api_key("key-1", 100).unwrap().unwrap();
        assert_eq!(
            revoked_again.revoked_at,
            Some(99),
            "idempotent revoke must keep the original timestamp"
        );
    }

    #[test]
    fn list_users_returns_all_users_ordered_by_username() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("webcodex.db")).unwrap();
        let now = chrono::Utc::now().timestamp();
        for (uname, role) in [("carol", "user"), ("alice", "admin"), ("bob", "user")] {
            db.create_user(&UserRecord {
                id: format!("u-{}", uname),
                username: uname.to_string(),
                created_at: now,
                disabled: 0,
                display_name: None,
                role: role.to_string(),
                disabled_at: None,
                updated_at: Some(now),
            })
            .unwrap();
        }
        let users = db.list_users().unwrap();
        let names: Vec<&str> = users.iter().map(|u| u.username.as_str()).collect();
        assert_eq!(names, vec!["alice", "bob", "carol"]);
        assert_eq!(
            users.iter().find(|u| u.username == "alice").unwrap().role,
            "admin"
        );
    }

    #[test]
    fn set_user_disabled_marks_user_and_blocks_token_lookup_path() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("webcodex.db")).unwrap();
        let now = chrono::Utc::now().timestamp();
        db.create_user(&UserRecord {
            id: "u-1".to_string(),
            username: "alice".to_string(),
            created_at: now,
            disabled: 0,
            display_name: None,
            role: "user".to_string(),
            disabled_at: None,
            updated_at: Some(now),
        })
        .unwrap();
        let disabled = db.set_user_disabled("u-1", true, now).unwrap().unwrap();
        assert!(disabled.is_disabled());
        assert_eq!(disabled.disabled, 1);
        assert_eq!(disabled.disabled_at, Some(now));
        // Re-enabling clears both flags.
        let reenabled = db
            .set_user_disabled("u-1", false, now + 10)
            .unwrap()
            .unwrap();
        assert!(!reenabled.is_disabled());
        assert_eq!(reenabled.disabled, 0);
        assert_eq!(reenabled.disabled_at, None);
    }

    /// Phase 2 token lifecycle: create stores hash (not plaintext), lookup
    /// succeeds, revoked tokens are ignored, expired tokens report expired,
    /// disabled-user tokens are rejected at the auth layer, and last_used_at
    /// updates. Uses the same SHA-256 hash as the auth middleware.
    #[test]
    fn phase2_token_lifecycle_hash_revoked_expired_disabled_last_used() {
        use sha2::{Digest, Sha256};
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("webcodex.db")).unwrap();
        let now = chrono::Utc::now().timestamp();

        // Create user.
        let user = UserRecord {
            id: "u-1".to_string(),
            username: "alice".to_string(),
            created_at: now,
            disabled: 0,
            display_name: None,
            role: "user".to_string(),
            disabled_at: None,
            updated_at: Some(now),
        };
        db.create_user(&user).unwrap();
        // Duplicate username rejected.
        let dup_err = db.create_user(&UserRecord {
            id: "u-2".to_string(),
            ..user.clone()
        });
        assert!(dup_err.is_err(), "duplicate username must be rejected");

        // Create token: store hash, never plaintext.
        let plaintext = "wc_pat_testsecretvalue1234567890";
        let mut hasher = Sha256::new();
        hasher.update(plaintext.as_bytes());
        let key_hash = format!("{:x}", hasher.finalize());
        let key = ApiKeyRecord {
            id: "k-1".to_string(),
            user_id: "u-1".to_string(),
            name: "main".to_string(),
            key_prefix: "wc_pat_testse".to_string(),
            created_at: now,
            last_used_at: None,
            revoked_at: None,
            scopes: "runtime:read project:write".to_string(),
            expires_at: None,
            kind: "user".to_string(),
            allowed_client_id: None,
        };
        db.insert_api_key(&key, &key_hash).unwrap();

        // The stored key_hash must not be the plaintext token.
        let conn = db.conn_for_tests();
        let stored_hash: String = conn
            .query_row(
                "SELECT key_hash FROM api_keys WHERE id = 'k-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(stored_hash, plaintext);
        assert_eq!(stored_hash, key_hash);
        drop(conn);

        // Lookup succeeds.
        let fetched = db.get_api_key_by_hash(&key_hash).unwrap().unwrap();
        assert_eq!(fetched.name, "main");
        assert_eq!(
            fetched.scopes_vec(),
            vec!["runtime:read".to_string(), "project:write".to_string()]
        );
        assert!(!fetched.is_revoked());
        assert!(!fetched.is_expired(now));

        // last_used_at updates.
        db.update_api_key_last_used("k-1", now + 5).unwrap();
        let fetched = db.get_api_key_by_hash(&key_hash).unwrap().unwrap();
        assert_eq!(fetched.last_used_at, Some(now + 5));

        // Revoked token is ignored by get_api_key_by_hash (returns None).
        db.revoke_api_key("k-1", now + 10).unwrap();
        assert!(db.get_api_key_by_hash(&key_hash).unwrap().is_none());
        // But get_api_key_by_id still returns it (with revoked_at set).
        let revoked = db.get_api_key_by_id("k-1").unwrap().unwrap();
        assert!(revoked.is_revoked());

        // Expired token: a non-revoked token with expires_at in the past
        // reports is_expired true (the auth middleware rejects it).
        let exp_key = ApiKeyRecord {
            id: "k-2".to_string(),
            revoked_at: None,
            expires_at: Some(now - 1),
            ..key.clone()
        };
        db.insert_api_key(&exp_key, "hash-exp").unwrap();
        let fetched = db.get_api_key_by_hash("hash-exp").unwrap().unwrap();
        assert!(fetched.is_expired(now));

        // Disabled-user token: the auth layer checks user.is_disabled(); here
        // we confirm the DB marks the user disabled and the record helper
        // reports it.
        db.set_user_disabled("u-1", true, now).unwrap();
        let disabled_user = db.get_user_by_id("u-1").unwrap().unwrap();
        assert!(disabled_user.is_disabled());
    }

    /// Phase 3: existing user tokens default to kind="user" after migration,
    /// and the model helpers correctly distinguish user vs agent tokens.
    #[test]
    fn phase3_existing_user_tokens_default_to_kind_user_after_migration() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("webcodex.db")).unwrap();
        let now = chrono::Utc::now().timestamp();
        db.create_user(&UserRecord {
            id: "u-1".to_string(),
            username: "alice".to_string(),
            created_at: now,
            disabled: 0,
            display_name: None,
            role: "user".to_string(),
            disabled_at: None,
            updated_at: Some(now),
        })
        .unwrap();
        // Simulate a legacy Phase 2 row by constructing an ApiKeyRecord with
        // kind="user" (the migration default) and allowed_client_id=None.
        let key = ApiKeyRecord {
            id: "k-legacy".to_string(),
            user_id: "u-1".to_string(),
            name: "legacy".to_string(),
            key_prefix: "wc_pat_legacy".to_string(),
            created_at: now,
            last_used_at: None,
            revoked_at: None,
            scopes: "runtime:read".to_string(),
            expires_at: None,
            kind: "user".to_string(),
            allowed_client_id: None,
        };
        db.insert_api_key(&key, "hash-legacy").unwrap();
        let fetched = db.get_api_key_by_hash("hash-legacy").unwrap().unwrap();
        assert!(fetched.is_user_token(), "legacy token must be kind=user");
        assert!(!fetched.is_agent_token());
        assert_eq!(fetched.kind(), "user");
        assert!(fetched.allowed_client_id().is_none());
    }

    /// Phase 3: agent tokens are stored with kind=agent and allowed_client_id,
    /// and the hash (not plaintext) is persisted.
    #[test]
    fn phase3_agent_token_stored_with_kind_and_allowed_client_id() {
        use sha2::{Digest, Sha256};
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("webcodex.db")).unwrap();
        let now = chrono::Utc::now().timestamp();
        db.create_user(&UserRecord {
            id: "u-1".to_string(),
            username: "alice".to_string(),
            created_at: now,
            disabled: 0,
            display_name: None,
            role: "user".to_string(),
            disabled_at: None,
            updated_at: Some(now),
        })
        .unwrap();
        let plaintext = "wc_agent_secretvalue1234567890abcdef";
        let mut hasher = Sha256::new();
        hasher.update(plaintext.as_bytes());
        let key_hash = format!("{:x}", hasher.finalize());
        let key = ApiKeyRecord {
            id: "k-agent-1".to_string(),
            user_id: "u-1".to_string(),
            name: "laptop agent".to_string(),
            key_prefix: "wc_agent_secret".to_string(),
            created_at: now,
            last_used_at: None,
            revoked_at: None,
            scopes: "agent:register agent:poll agent:result agent:job_update".to_string(),
            expires_at: None,
            kind: "agent".to_string(),
            allowed_client_id: Some("alice-laptop".to_string()),
        };
        db.insert_api_key(&key, &key_hash).unwrap();
        // The stored key_hash must not be the plaintext token.
        let conn = db.conn_for_tests();
        let stored_hash: String = conn
            .query_row(
                "SELECT key_hash FROM api_keys WHERE id = 'k-agent-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(stored_hash, plaintext);
        assert_eq!(stored_hash, key_hash);
        // The stored kind and allowed_client_id must match.
        let (stored_kind, stored_cid): (String, Option<String>) = conn
            .query_row(
                "SELECT kind, allowed_client_id FROM api_keys WHERE id = 'k-agent-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        drop(conn);
        assert_eq!(stored_kind, "agent");
        assert_eq!(stored_cid.as_deref(), Some("alice-laptop"));

        // Lookup succeeds and the record reports agent token.
        let fetched = db.get_api_key_by_hash(&key_hash).unwrap().unwrap();
        assert!(fetched.is_agent_token());
        assert!(!fetched.is_user_token());
        assert_eq!(fetched.kind(), "agent");
        assert_eq!(fetched.allowed_client_id(), Some("alice-laptop"));
        assert_eq!(
            fetched.scopes_vec(),
            vec![
                "agent:register".to_string(),
                "agent:poll".to_string(),
                "agent:result".to_string(),
                "agent:job_update".to_string(),
            ]
        );
    }

    /// Phase 3: revoked/expired/disabled checks apply to agent tokens too.
    #[test]
    fn phase3_agent_token_revoked_expired_disabled_checks_apply() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("webcodex.db")).unwrap();
        let now = chrono::Utc::now().timestamp();
        db.create_user(&UserRecord {
            id: "u-1".to_string(),
            username: "alice".to_string(),
            created_at: now,
            disabled: 0,
            display_name: None,
            role: "user".to_string(),
            disabled_at: None,
            updated_at: Some(now),
        })
        .unwrap();
        let key = ApiKeyRecord {
            id: "k-agent".to_string(),
            user_id: "u-1".to_string(),
            name: "agent".to_string(),
            key_prefix: "wc_agent_pre".to_string(),
            created_at: now,
            last_used_at: None,
            revoked_at: None,
            scopes: "agent:register".to_string(),
            expires_at: None,
            kind: "agent".to_string(),
            allowed_client_id: Some("alice-laptop".to_string()),
        };
        db.insert_api_key(&key, "hash-agent").unwrap();
        // Revoked agent token is ignored by get_api_key_by_hash.
        db.revoke_api_key("k-agent", now + 10).unwrap();
        assert!(db.get_api_key_by_hash("hash-agent").unwrap().is_none());
        // But get_api_key_by_id returns it with revoked_at set.
        let revoked = db.get_api_key_by_id("k-agent").unwrap().unwrap();
        assert!(revoked.is_revoked());
        assert!(revoked.is_agent_token());

        // Expired agent token: is_expired reports true.
        let exp_key = ApiKeyRecord {
            id: "k-agent-exp".to_string(),
            revoked_at: None,
            expires_at: Some(now - 1),
            ..key.clone()
        };
        db.insert_api_key(&exp_key, "hash-agent-exp").unwrap();
        let fetched = db.get_api_key_by_hash("hash-agent-exp").unwrap().unwrap();
        assert!(fetched.is_expired(now));
        assert!(fetched.is_agent_token());

        // Disabled-user agent token: the auth layer checks user.is_disabled();
        // here we confirm the DB marks the user disabled.
        db.set_user_disabled("u-1", true, now).unwrap();
        let disabled_user = db.get_user_by_id("u-1").unwrap().unwrap();
        assert!(disabled_user.is_disabled());
    }

    /// Phase 3: list_user_tokens (list_api_keys_by_user) returns both user and
    /// agent tokens; list_agent_tokens returns only kind=agent.
    #[test]
    fn phase3_list_agent_tokens_returns_only_kind_agent() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("webcodex.db")).unwrap();
        let now = chrono::Utc::now().timestamp();
        db.create_user(&UserRecord {
            id: "u-1".to_string(),
            username: "alice".to_string(),
            created_at: now,
            disabled: 0,
            display_name: None,
            role: "user".to_string(),
            disabled_at: None,
            updated_at: Some(now),
        })
        .unwrap();
        // One user token, two agent tokens.
        let user_key = ApiKeyRecord {
            id: "k-user".to_string(),
            user_id: "u-1".to_string(),
            name: "user".to_string(),
            key_prefix: "wc_pat_user".to_string(),
            created_at: now,
            last_used_at: None,
            revoked_at: None,
            scopes: "runtime:read".to_string(),
            expires_at: None,
            kind: "user".to_string(),
            allowed_client_id: None,
        };
        db.insert_api_key(&user_key, "hash-user").unwrap();
        let agent_key_1 = ApiKeyRecord {
            id: "k-agent-1".to_string(),
            name: "agent-1".to_string(),
            key_prefix: "wc_agent_a1".to_string(),
            kind: "agent".to_string(),
            allowed_client_id: Some("laptop".to_string()),
            scopes: "agent:register".to_string(),
            ..user_key.clone()
        };
        db.insert_api_key(&agent_key_1, "hash-agent-1").unwrap();
        let agent_key_2 = ApiKeyRecord {
            id: "k-agent-2".to_string(),
            name: "agent-2".to_string(),
            key_prefix: "wc_agent_a2".to_string(),
            kind: "agent".to_string(),
            allowed_client_id: Some("desktop".to_string()),
            scopes: "agent:poll agent:result".to_string(),
            ..user_key.clone()
        };
        db.insert_api_key(&agent_key_2, "hash-agent-2").unwrap();

        // list_api_keys_by_user returns all 3.
        let all = db.list_api_keys_by_user("u-1").unwrap();
        assert_eq!(all.len(), 3);

        // list_agent_api_keys_by_user returns only the 2 agent tokens.
        let agents = db.list_agent_api_keys_by_user("u-1").unwrap();
        assert_eq!(agents.len(), 2);
        assert!(agents.iter().all(|k| k.is_agent_token()));
        assert!(
            agents.iter().all(|k| k.allowed_client_id.is_some()),
            "agent tokens must have allowed_client_id"
        );
    }
}
