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
                disabled INTEGER NOT NULL DEFAULT 0
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
        let mut stmt = conn
            .prepare("SELECT id, username, created_at, disabled FROM users WHERE username = ?1")?;
        let mut rows = stmt.query_map(params![username], |row| {
            Ok(UserRecord {
                id: row.get(0)?,
                username: row.get(1)?,
                created_at: row.get(2)?,
                disabled: row.get(3)?,
            })
        })?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn create_user(&self, user: &UserRecord) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO users (id, username, created_at, disabled)
             VALUES (?1, ?2, ?3, ?4)",
            params![user.id, user.username, user.created_at, user.disabled],
        )?;
        Ok(())
    }

    pub fn get_api_key_by_hash(&self, hash: &str) -> anyhow::Result<Option<ApiKeyRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, name, key_prefix, created_at, last_used_at, revoked_at
             FROM api_keys
             WHERE key_hash = ?1 AND revoked_at IS NULL",
        )?;
        let mut rows = stmt.query_map(params![hash], |row| {
            Ok(ApiKeyRecord {
                id: row.get(0)?,
                user_id: row.get(1)?,
                name: row.get(2)?,
                key_prefix: row.get(3)?,
                created_at: row.get(4)?,
                last_used_at: row.get(5)?,
                revoked_at: row.get(6)?,
            })
        })?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn insert_api_key(&self, key: &ApiKeyRecord, key_hash: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO api_keys (id, user_id, name, key_hash, key_prefix, created_at, last_used_at, revoked_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                key.id,
                key.user_id,
                key.name,
                key_hash,
                key.key_prefix,
                key.created_at,
                key.last_used_at,
                key.revoked_at,
            ],
        )?;
        Ok(())
    }

    pub fn list_api_keys_by_user(&self, user_id: &str) -> anyhow::Result<Vec<ApiKeyRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, name, key_prefix, created_at, last_used_at, revoked_at
             FROM api_keys WHERE user_id = ?1",
        )?;
        let rows = stmt.query_map(params![user_id], |row| {
            Ok(ApiKeyRecord {
                id: row.get(0)?,
                user_id: row.get(1)?,
                name: row.get(2)?,
                key_prefix: row.get(3)?,
                created_at: row.get(4)?,
                last_used_at: row.get(5)?,
                revoked_at: row.get(6)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get_user_by_id(&self, id: &str) -> anyhow::Result<Option<UserRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT id, username, created_at, disabled FROM users WHERE id = ?1")?;
        let mut rows = stmt.query_map(params![id], |row| {
            Ok(UserRecord {
                id: row.get(0)?,
                username: row.get(1)?,
                created_at: row.get(2)?,
                disabled: row.get(3)?,
            })
        })?;
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
        };
        db.create_user(&user).unwrap();

        assert_eq!(
            db.get_user_by_username("alice").unwrap().unwrap().id,
            "user-1"
        );
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
        };
        db.insert_api_key(&key, "hash-1").unwrap();
        assert_eq!(
            db.get_api_key_by_hash("hash-1").unwrap().unwrap().name,
            "main"
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
    }
}
