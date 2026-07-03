use crate::models::PairingCodeRecord;
#[cfg(test)]
use crate::models::{
    ApiKeyRecord, OAuthAccessTokenRecord, OAuthAuthorizationCodeRecord, OAuthClientRecord,
    OAuthRefreshTokenRecord, UserRecord,
};
use crate::{
    ActionEventRecord, ActionSessionRecord, Channel, CodexGoalRecord, CommandAuditRecord, Message,
    MessageKind,
};
use rusqlite::{params, Connection};
use std::sync::Mutex;

mod accounts;
mod agents;
mod oauth;
mod schema;

pub use self::oauth::RotateResult;

#[cfg(test)]
use self::schema::{has_column, oauth_user_id_is_nullable, table_column_info, table_columns};

pub struct Database {
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone)]
pub enum PairingConsumeResult {
    NotFound,
    Consumed(PairingCodeRecord),
    AlreadyUsed(PairingCodeRecord),
    Expired(PairingCodeRecord),
    ClientMismatch(PairingCodeRecord),
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

#[cfg(test)]
impl Database {
    /// Test-only access to the underlying connection so tests can assert on
    /// raw storage (e.g. that a plaintext token is never stored as `key_hash`).
    pub fn conn_for_tests(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap()
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

    // -----------------------------------------------------------------------
    // Phase 2a: OAuth2 database tests
    // -----------------------------------------------------------------------

    fn oauth_seed_user(db: &Database, username: &str) -> UserRecord {
        let now = chrono::Utc::now().timestamp();
        let user = UserRecord {
            id: format!("u-{}", username),
            username: username.to_string(),
            created_at: now,
            disabled: 0,
            display_name: None,
            role: "user".to_string(),
            disabled_at: None,
            updated_at: Some(now),
        };
        db.create_user(&user).unwrap();
        user
    }

    fn oauth_seed_client(
        db: &Database,
        user: &UserRecord,
        name: &str,
    ) -> (OAuthClientRecord, String) {
        let now = chrono::Utc::now().timestamp();
        let plaintext_secret = crate::auth::generate_oauth_client_secret();
        let secret_hash = crate::auth::hash_token(&plaintext_secret);
        let record = OAuthClientRecord {
            id: uuid::Uuid::new_v4().to_string(),
            client_id: crate::auth::generate_oauth_client_id(),
            client_secret_hash: secret_hash.clone(),
            name: name.to_string(),
            owner_user_id: user.id.clone(),
            redirect_uris: "https://example.com/callback".to_string(),
            allowed_scopes: "runtime:read project:read".to_string(),
            created_at: now,
            revoked_at: None,
        };
        db.insert_oauth_client(&record).unwrap();
        (record, plaintext_secret)
    }

    fn assert_oauth_subject_columns(conn: &Connection, table: &str) {
        let cols = table_column_info(conn, table).unwrap();
        assert!(has_column(&cols, "subject_kind"), "{table} subject_kind");
        assert!(has_column(&cols, "subject_id"), "{table} subject_id");
        assert!(
            has_column(&cols, "shared_key_hash"),
            "{table} shared_key_hash"
        );
        assert!(
            oauth_user_id_is_nullable(&cols),
            "{table} user_id should allow NULL"
        );
    }

    #[test]
    fn fresh_database_creates_oauth_tables() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let conn = db.conn_for_tests();
        // All four OAuth2 tables must exist.
        for table in [
            "oauth_clients",
            "oauth_authorization_codes",
            "oauth_access_tokens",
            "oauth_refresh_tokens",
        ] {
            let count: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {}", table), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0, "table {} should be empty", table);
        }
        for table in [
            "oauth_authorization_codes",
            "oauth_access_tokens",
            "oauth_refresh_tokens",
        ] {
            assert_oauth_subject_columns(&conn, table);
        }
    }

    #[test]
    fn can_insert_and_get_oauth_client() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _secret) = oauth_seed_client(&db, &user, "Test App");

        let fetched = db
            .get_oauth_client_by_client_id(&client.client_id)
            .unwrap()
            .unwrap();
        assert_eq!(fetched.name, "Test App");
        assert_eq!(fetched.owner_user_id, user.id);
        assert!(!fetched.is_revoked());
        assert_eq!(
            fetched.redirect_uris_vec(),
            vec!["https://example.com/callback"]
        );
    }

    #[test]
    fn verify_oauth_client_secret_works() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, plaintext_secret) = oauth_seed_client(&db, &user, "Test App");

        // Correct secret verifies.
        assert!(db
            .verify_oauth_client_secret(&client.client_id, &plaintext_secret)
            .unwrap());
        // Wrong secret rejects.
        assert!(!db
            .verify_oauth_client_secret(&client.client_id, "wrong-secret")
            .unwrap());
        // Unknown client_id rejects.
        assert!(!db
            .verify_oauth_client_secret("wc_client_nonexistent", &plaintext_secret)
            .unwrap());
    }

    #[test]
    fn revoked_oauth_client_not_returned_by_lookup() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");

        db.revoke_oauth_client(&client.id, 100).unwrap();
        // get_oauth_client_by_client_id filters revoked clients.
        assert!(db
            .get_oauth_client_by_client_id(&client.client_id)
            .unwrap()
            .is_none());
        // get_oauth_client_by_id still returns it.
        let revoked = db.get_oauth_client_by_id(&client.id).unwrap().unwrap();
        assert!(revoked.is_revoked());
    }

    #[test]
    fn can_insert_and_get_authorization_code_by_hash() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");

        let plaintext_code = crate::auth::generate_oauth_authorization_code();
        let code_hash = crate::auth::hash_token(&plaintext_code);
        let now = chrono::Utc::now().timestamp();
        let record = OAuthAuthorizationCodeRecord {
            id: uuid::Uuid::new_v4().to_string(),
            code_hash: code_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            redirect_uri: "https://example.com/callback".to_string(),
            scopes: "runtime:read".to_string(),
            code_challenge: Some("E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM".to_string()),
            code_challenge_method: Some("S256".to_string()),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 300,
            used_at: None,
            revoked_at: None,
        };
        db.insert_oauth_authorization_code(&record, &code_hash)
            .unwrap();

        let fetched = db
            .get_oauth_authorization_code_by_hash(&code_hash)
            .unwrap()
            .unwrap();
        assert_eq!(fetched.client_id, client.client_id);
        assert_eq!(fetched.subject_kind, "managed_user");
        assert_eq!(fetched.subject_id, user.id);
        assert_eq!(fetched.user_id, Some(user.id.clone()));
        assert!(!fetched.is_used());
        assert!(!fetched.is_expired(now));
        assert!(fetched.is_expired(now + 301));
        assert_eq!(
            fetched.code_challenge.as_deref(),
            Some("E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM")
        );
        assert_eq!(fetched.code_challenge_method.as_deref(), Some("S256"));
        assert!(fetched.shared_key_hash.is_none());
    }

    #[test]
    fn can_mark_authorization_code_used() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");

        let plaintext_code = crate::auth::generate_oauth_authorization_code();
        let code_hash = crate::auth::hash_token(&plaintext_code);
        let now = chrono::Utc::now().timestamp();
        let record = OAuthAuthorizationCodeRecord {
            id: uuid::Uuid::new_v4().to_string(),
            code_hash: code_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            redirect_uri: "https://example.com/callback".to_string(),
            scopes: "runtime:read".to_string(),
            code_challenge: None,
            code_challenge_method: None,
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 300,
            used_at: None,
            revoked_at: None,
        };
        db.insert_oauth_authorization_code(&record, &code_hash)
            .unwrap();

        // Mark as used.
        db.mark_oauth_authorization_code_used(&record.id, now + 10)
            .unwrap();
        let fetched = db
            .get_oauth_authorization_code_by_hash(&code_hash)
            .unwrap()
            .unwrap();
        assert!(fetched.is_used());
        assert_eq!(fetched.used_at, Some(now + 10));
    }

    #[test]
    fn can_insert_and_get_access_token_by_hash() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");

        let plaintext_token = crate::auth::generate_oauth_access_token();
        let token_hash = crate::auth::hash_token(&plaintext_token);
        let now = chrono::Utc::now().timestamp();
        let record = OAuthAccessTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: token_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 3600,
            revoked_at: None,
            last_used_at: None,
        };
        db.insert_oauth_access_token(&record).unwrap();

        let fetched = db
            .get_oauth_access_token_by_hash(&token_hash)
            .unwrap()
            .unwrap();
        assert_eq!(fetched.client_id, client.client_id);
        assert_eq!(fetched.subject_kind, "managed_user");
        assert_eq!(fetched.subject_id, user.id);
        assert_eq!(fetched.user_id, Some(user.id.clone()));
        assert!(!fetched.is_revoked());
        assert!(!fetched.is_expired(now));
        assert!(fetched.is_expired(now + 3601));
        assert!(fetched.last_used_at.is_none());
        assert!(fetched.shared_key_hash.is_none());
    }

    #[test]
    fn can_update_access_token_last_used() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");

        let plaintext_token = crate::auth::generate_oauth_access_token();
        let token_hash = crate::auth::hash_token(&plaintext_token);
        let now = chrono::Utc::now().timestamp();
        let record = OAuthAccessTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: token_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 3600,
            revoked_at: None,
            last_used_at: None,
        };
        db.insert_oauth_access_token(&record).unwrap();

        db.update_oauth_access_token_last_used(&record.id, now + 60)
            .unwrap();
        let fetched = db
            .get_oauth_access_token_by_hash(&token_hash)
            .unwrap()
            .unwrap();
        assert_eq!(fetched.last_used_at, Some(now + 60));
    }

    #[test]
    fn can_revoke_access_token() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");

        let plaintext_token = crate::auth::generate_oauth_access_token();
        let token_hash = crate::auth::hash_token(&plaintext_token);
        let now = chrono::Utc::now().timestamp();
        let record = OAuthAccessTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: token_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 3600,
            revoked_at: None,
            last_used_at: None,
        };
        db.insert_oauth_access_token(&record).unwrap();

        db.revoke_oauth_access_token(&record.id, now + 100).unwrap();
        // Revoked token is not returned by hash lookup.
        assert!(db
            .get_oauth_access_token_by_hash(&token_hash)
            .unwrap()
            .is_none());
    }

    #[test]
    fn can_insert_and_get_refresh_token_by_hash() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");

        let plaintext_token = crate::auth::generate_oauth_refresh_token();
        let token_hash = crate::auth::hash_token(&plaintext_token);
        let now = chrono::Utc::now().timestamp();
        let record = OAuthRefreshTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: token_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 2_592_000,
            revoked_at: None,
            last_used_at: None,
            rotated_from_id: None,
        };
        db.insert_oauth_refresh_token(&record).unwrap();

        let fetched = db
            .get_oauth_refresh_token_by_hash(&token_hash)
            .unwrap()
            .unwrap();
        assert_eq!(fetched.client_id, client.client_id);
        assert_eq!(fetched.subject_kind, "managed_user");
        assert_eq!(fetched.subject_id, user.id);
        assert_eq!(fetched.user_id, Some(user.id.clone()));
        assert!(!fetched.is_revoked());
        assert!(!fetched.is_expired(now));
        assert!(fetched.rotated_from_id.is_none());
        assert!(fetched.shared_key_hash.is_none());
    }

    #[test]
    fn oauth_shared_key_subject_records_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");
        let now = chrono::Utc::now().timestamp();

        let plaintext_ac = crate::auth::generate_oauth_authorization_code();
        let auth_code = OAuthAuthorizationCodeRecord {
            id: uuid::Uuid::new_v4().to_string(),
            code_hash: crate::auth::hash_token(&plaintext_ac),
            client_id: client.client_id.clone(),
            subject_kind: "shared_key".to_string(),
            subject_id: "hash-a".to_string(),
            user_id: None,
            redirect_uri: "https://example.com/callback".to_string(),
            scopes: "runtime:read".to_string(),
            code_challenge: None,
            code_challenge_method: None,
            resource: None,
            shared_key_hash: Some("hash-a".to_string()),
            created_at: now,
            expires_at: now + 300,
            used_at: None,
            revoked_at: None,
        };
        db.insert_oauth_authorization_code(&auth_code, &auth_code.code_hash)
            .unwrap();
        let fetched_auth_code = db
            .get_oauth_authorization_code_by_hash(&auth_code.code_hash)
            .unwrap()
            .unwrap();
        assert_eq!(fetched_auth_code.subject_kind, "shared_key");
        assert_eq!(fetched_auth_code.subject_id, "hash-a");
        assert_eq!(fetched_auth_code.user_id, None);
        assert_eq!(fetched_auth_code.shared_key_hash.as_deref(), Some("hash-a"));

        let plaintext_at = crate::auth::generate_oauth_access_token();
        let access = OAuthAccessTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: crate::auth::hash_token(&plaintext_at),
            client_id: client.client_id.clone(),
            subject_kind: "shared_key".to_string(),
            subject_id: "hash-a".to_string(),
            user_id: None,
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: Some("hash-a".to_string()),
            created_at: now,
            expires_at: now + 3600,
            revoked_at: None,
            last_used_at: None,
        };
        db.insert_oauth_access_token(&access).unwrap();
        let fetched_access = db
            .get_oauth_access_token_by_hash(&access.token_hash)
            .unwrap()
            .unwrap();
        assert_eq!(fetched_access.subject_kind, "shared_key");
        assert_eq!(fetched_access.subject_id, "hash-a");
        assert_eq!(fetched_access.user_id, None);
        assert_eq!(fetched_access.shared_key_hash.as_deref(), Some("hash-a"));

        let plaintext_rt = crate::auth::generate_oauth_refresh_token();
        let refresh = OAuthRefreshTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: crate::auth::hash_token(&plaintext_rt),
            client_id: client.client_id.clone(),
            subject_kind: "shared_key".to_string(),
            subject_id: "hash-a".to_string(),
            user_id: None,
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: Some("hash-a".to_string()),
            created_at: now,
            expires_at: now + 2_592_000,
            revoked_at: None,
            last_used_at: None,
            rotated_from_id: None,
        };
        db.insert_oauth_refresh_token(&refresh).unwrap();
        let fetched_refresh = db
            .get_oauth_refresh_token_by_hash(&refresh.token_hash)
            .unwrap()
            .unwrap();
        assert_eq!(fetched_refresh.subject_kind, "shared_key");
        assert_eq!(fetched_refresh.subject_id, "hash-a");
        assert_eq!(fetched_refresh.user_id, None);
        assert_eq!(fetched_refresh.shared_key_hash.as_deref(), Some("hash-a"));
    }

    #[test]
    fn oauth_subject_validation_rejects_invalid_combinations() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");
        let now = chrono::Utc::now().timestamp();

        let valid = OAuthAccessTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: "hash-valid".to_string(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 3600,
            revoked_at: None,
            last_used_at: None,
        };

        let mut record = valid.clone();
        record.id = uuid::Uuid::new_v4().to_string();
        record.token_hash = "hash-shared-with-user".to_string();
        record.subject_kind = "shared_key".to_string();
        record.subject_id = "hash-a".to_string();
        record.user_id = Some(user.id.clone());
        record.shared_key_hash = Some("hash-a".to_string());
        assert!(db.insert_oauth_access_token(&record).is_err());

        let mut record = valid.clone();
        record.id = uuid::Uuid::new_v4().to_string();
        record.token_hash = "hash-shared-missing-hash".to_string();
        record.subject_kind = "shared_key".to_string();
        record.subject_id = "hash-a".to_string();
        record.user_id = None;
        record.shared_key_hash = None;
        assert!(db.insert_oauth_access_token(&record).is_err());

        let mut record = valid.clone();
        record.id = uuid::Uuid::new_v4().to_string();
        record.token_hash = "hash-managed-missing-user".to_string();
        record.user_id = None;
        assert!(db.insert_oauth_access_token(&record).is_err());

        let mut record = valid.clone();
        record.id = uuid::Uuid::new_v4().to_string();
        record.token_hash = "hash-managed-mismatch".to_string();
        record.subject_id = "other-user".to_string();
        assert!(db.insert_oauth_access_token(&record).is_err());

        let mut record = valid;
        record.id = uuid::Uuid::new_v4().to_string();
        record.token_hash = "hash-unknown-kind".to_string();
        record.subject_kind = "unknown".to_string();
        assert!(db.insert_oauth_access_token(&record).is_err());
    }

    #[test]
    fn oauth_bridge_shared_key_hash_columns_are_migrated() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("legacy-oauth.db");
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch(
                "
                CREATE TABLE oauth_authorization_codes (
                    id TEXT PRIMARY KEY,
                    code_hash TEXT NOT NULL UNIQUE,
                    client_id TEXT NOT NULL,
                    user_id TEXT NOT NULL,
                    redirect_uri TEXT NOT NULL,
                    scopes TEXT NOT NULL DEFAULT '',
                    code_challenge TEXT,
                    code_challenge_method TEXT,
                    resource TEXT,
                    shared_key_hash TEXT,
                    created_at INTEGER NOT NULL,
                    expires_at INTEGER NOT NULL,
                    used_at INTEGER,
                    revoked_at INTEGER
                );
                CREATE TABLE oauth_access_tokens (
                    id TEXT PRIMARY KEY,
                    token_hash TEXT NOT NULL UNIQUE,
                    client_id TEXT NOT NULL,
                    user_id TEXT NOT NULL,
                    scopes TEXT NOT NULL DEFAULT '',
                    resource TEXT,
                    shared_key_hash TEXT,
                    created_at INTEGER NOT NULL,
                    expires_at INTEGER NOT NULL,
                    revoked_at INTEGER,
                    last_used_at INTEGER
                );
                CREATE TABLE oauth_refresh_tokens (
                    id TEXT PRIMARY KEY,
                    token_hash TEXT NOT NULL UNIQUE,
                    client_id TEXT NOT NULL,
                    user_id TEXT NOT NULL,
                    scopes TEXT NOT NULL DEFAULT '',
                    resource TEXT,
                    shared_key_hash TEXT,
                    created_at INTEGER NOT NULL,
                    expires_at INTEGER NOT NULL,
                    revoked_at INTEGER,
                    last_used_at INTEGER,
                    rotated_from_id TEXT
                );
                INSERT INTO oauth_authorization_codes (
                    id, code_hash, client_id, user_id, redirect_uri, scopes,
                    code_challenge, code_challenge_method, resource, shared_key_hash,
                    created_at, expires_at, used_at, revoked_at
                ) VALUES (
                    'legacy-code', 'legacy-code-hash', 'legacy-client', 'legacy-user',
                    'https://example.com/callback', 'runtime:read', NULL, NULL, NULL, 'legacy-hash',
                    1, 301, NULL, NULL
                );
                INSERT INTO oauth_access_tokens (
                    id, token_hash, client_id, user_id, scopes, resource, shared_key_hash,
                    created_at, expires_at, revoked_at, last_used_at
                ) VALUES (
                    'legacy-access', 'legacy-access-hash', 'legacy-client', 'legacy-user',
                    'runtime:read', NULL, 'legacy-hash', 1, 3601, NULL, NULL
                );
                INSERT INTO oauth_refresh_tokens (
                    id, token_hash, client_id, user_id, scopes, resource, shared_key_hash,
                    created_at, expires_at, revoked_at, last_used_at, rotated_from_id
                ) VALUES (
                    'legacy-refresh', 'legacy-refresh-hash', 'legacy-client', 'legacy-user',
                    'runtime:read', NULL, 'legacy-hash', 1, 2592001, NULL, NULL, NULL
                );
                ",
            )
            .unwrap();
        }

        let db = Database::open(&path).unwrap();
        let conn = db.conn.lock().unwrap();
        let auth_code_cols = table_columns(&conn, "oauth_authorization_codes").unwrap();
        let access_cols = table_columns(&conn, "oauth_access_tokens").unwrap();
        let refresh_cols = table_columns(&conn, "oauth_refresh_tokens").unwrap();
        assert!(auth_code_cols.iter().any(|c| c == "shared_key_hash"));
        assert!(access_cols.iter().any(|c| c == "shared_key_hash"));
        assert!(refresh_cols.iter().any(|c| c == "shared_key_hash"));
        assert_oauth_subject_columns(&conn, "oauth_authorization_codes");
        assert_oauth_subject_columns(&conn, "oauth_access_tokens");
        assert_oauth_subject_columns(&conn, "oauth_refresh_tokens");
        let auth_subject: (String, String, Option<String>) = conn
            .query_row(
                "SELECT subject_kind, subject_id, user_id FROM oauth_authorization_codes WHERE id = 'legacy-code'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        let access_subject: (String, String, Option<String>) = conn
            .query_row(
                "SELECT subject_kind, subject_id, user_id FROM oauth_access_tokens WHERE id = 'legacy-access'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        let refresh_subject: (String, String, Option<String>) = conn
            .query_row(
                "SELECT subject_kind, subject_id, user_id FROM oauth_refresh_tokens WHERE id = 'legacy-refresh'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            auth_subject,
            (
                "managed_user".to_string(),
                "legacy-user".to_string(),
                Some("legacy-user".to_string())
            )
        );
        assert_eq!(access_subject, auth_subject);
        assert_eq!(refresh_subject, auth_subject);
        let auth_code_shared_key_hash: Option<String> = conn
            .query_row(
                "SELECT shared_key_hash FROM oauth_authorization_codes WHERE id = 'legacy-code'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let access_shared_key_hash: Option<String> = conn
            .query_row(
                "SELECT shared_key_hash FROM oauth_access_tokens WHERE id = 'legacy-access'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let refresh_shared_key_hash: Option<String> = conn
            .query_row(
                "SELECT shared_key_hash FROM oauth_refresh_tokens WHERE id = 'legacy-refresh'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(auth_code_shared_key_hash.as_deref(), Some("legacy-hash"));
        assert_eq!(access_shared_key_hash.as_deref(), Some("legacy-hash"));
        assert_eq!(refresh_shared_key_hash.as_deref(), Some("legacy-hash"));
        drop(conn);
        drop(db);

        let db = Database::open(&path).unwrap();
        let conn = db.conn.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM oauth_access_tokens", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 1, "subject migration should be idempotent");
    }

    #[test]
    fn can_revoke_refresh_token() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");

        let plaintext_token = crate::auth::generate_oauth_refresh_token();
        let token_hash = crate::auth::hash_token(&plaintext_token);
        let now = chrono::Utc::now().timestamp();
        let record = OAuthRefreshTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: token_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 2_592_000,
            revoked_at: None,
            last_used_at: None,
            rotated_from_id: None,
        };
        db.insert_oauth_refresh_token(&record).unwrap();

        db.revoke_oauth_refresh_token(&record.id, now + 100)
            .unwrap();
        // Revoked token is not returned by hash lookup.
        assert!(db
            .get_oauth_refresh_token_by_hash(&token_hash)
            .unwrap()
            .is_none());
    }

    #[test]
    fn oauth_plaintext_tokens_are_never_stored() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");

        // Client secret: only hash stored.
        let (client, plaintext_secret) = oauth_seed_client(&db, &user, "Test App");
        let conn = db.conn_for_tests();
        let stored_secret_hash: String = conn
            .query_row(
                "SELECT client_secret_hash FROM oauth_clients WHERE id = ?1",
                rusqlite::params![client.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(stored_secret_hash, plaintext_secret);
        assert_eq!(
            stored_secret_hash,
            crate::auth::hash_token(&plaintext_secret)
        );
        drop(conn);

        // Access token: only hash stored.
        let plaintext_at = crate::auth::generate_oauth_access_token();
        let at_hash = crate::auth::hash_token(&plaintext_at);
        let now = chrono::Utc::now().timestamp();
        let at_record = OAuthAccessTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: at_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 3600,
            revoked_at: None,
            last_used_at: None,
        };
        db.insert_oauth_access_token(&at_record).unwrap();
        let conn = db.conn_for_tests();
        let stored_at_hash: String = conn
            .query_row(
                "SELECT token_hash FROM oauth_access_tokens WHERE id = ?1",
                rusqlite::params![at_record.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(stored_at_hash, plaintext_at);
        assert_eq!(stored_at_hash, at_hash);
        drop(conn);

        // Refresh token: only hash stored.
        let plaintext_rt = crate::auth::generate_oauth_refresh_token();
        let rt_hash = crate::auth::hash_token(&plaintext_rt);
        let rt_record = OAuthRefreshTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: rt_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 2_592_000,
            revoked_at: None,
            last_used_at: None,
            rotated_from_id: None,
        };
        db.insert_oauth_refresh_token(&rt_record).unwrap();
        let conn = db.conn_for_tests();
        let stored_rt_hash: String = conn
            .query_row(
                "SELECT token_hash FROM oauth_refresh_tokens WHERE id = ?1",
                rusqlite::params![rt_record.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(stored_rt_hash, plaintext_rt);
        assert_eq!(stored_rt_hash, rt_hash);
        drop(conn);

        // Authorization code: only hash stored.
        let plaintext_ac = crate::auth::generate_oauth_authorization_code();
        let ac_hash = crate::auth::hash_token(&plaintext_ac);
        let ac_record = OAuthAuthorizationCodeRecord {
            id: uuid::Uuid::new_v4().to_string(),
            code_hash: ac_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            redirect_uri: "https://example.com/callback".to_string(),
            scopes: "runtime:read".to_string(),
            code_challenge: None,
            code_challenge_method: None,
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 300,
            used_at: None,
            revoked_at: None,
        };
        db.insert_oauth_authorization_code(&ac_record, &ac_hash)
            .unwrap();
        let conn = db.conn_for_tests();
        let stored_ac_hash: String = conn
            .query_row(
                "SELECT code_hash FROM oauth_authorization_codes WHERE id = ?1",
                rusqlite::params![ac_record.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(stored_ac_hash, plaintext_ac);
        assert_eq!(stored_ac_hash, ac_hash);
    }

    // -----------------------------------------------------------------------
    // consume_oauth_authorization_code_by_hash tests
    // -----------------------------------------------------------------------

    #[test]
    fn consume_authorization_code_succeeds_once() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");

        let plaintext_code = crate::auth::generate_oauth_authorization_code();
        let code_hash = crate::auth::hash_token(&plaintext_code);
        let now = chrono::Utc::now().timestamp();
        let record = OAuthAuthorizationCodeRecord {
            id: uuid::Uuid::new_v4().to_string(),
            code_hash: code_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            redirect_uri: "https://example.com/callback".to_string(),
            scopes: "runtime:read".to_string(),
            code_challenge: None,
            code_challenge_method: None,
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 300,
            used_at: None,
            revoked_at: None,
        };
        db.insert_oauth_authorization_code(&record, &code_hash)
            .unwrap();

        // First consume succeeds.
        let consumed = db
            .consume_oauth_authorization_code_by_hash(&code_hash, now + 10)
            .unwrap();
        let consumed = consumed.expect("first consume should succeed");
        assert_eq!(consumed.used_at, Some(now + 10));
        assert_eq!(consumed.id, record.id);
    }

    #[test]
    fn consume_authorization_code_second_consume_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");

        let plaintext_code = crate::auth::generate_oauth_authorization_code();
        let code_hash = crate::auth::hash_token(&plaintext_code);
        let now = chrono::Utc::now().timestamp();
        let record = OAuthAuthorizationCodeRecord {
            id: uuid::Uuid::new_v4().to_string(),
            code_hash: code_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            redirect_uri: "https://example.com/callback".to_string(),
            scopes: "runtime:read".to_string(),
            code_challenge: None,
            code_challenge_method: None,
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 300,
            used_at: None,
            revoked_at: None,
        };
        db.insert_oauth_authorization_code(&record, &code_hash)
            .unwrap();

        // First consume succeeds.
        db.consume_oauth_authorization_code_by_hash(&code_hash, now + 10)
            .unwrap()
            .expect("first consume should succeed");

        // Second consume returns None.
        let result = db
            .consume_oauth_authorization_code_by_hash(&code_hash, now + 20)
            .unwrap();
        assert!(result.is_none(), "second consume should return None");
    }

    #[test]
    fn consume_authorization_code_expired_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");

        let plaintext_code = crate::auth::generate_oauth_authorization_code();
        let code_hash = crate::auth::hash_token(&plaintext_code);
        let now = chrono::Utc::now().timestamp();
        let record = OAuthAuthorizationCodeRecord {
            id: uuid::Uuid::new_v4().to_string(),
            code_hash: code_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            redirect_uri: "https://example.com/callback".to_string(),
            scopes: "runtime:read".to_string(),
            code_challenge: None,
            code_challenge_method: None,
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 300,
            used_at: None,
            revoked_at: None,
        };
        db.insert_oauth_authorization_code(&record, &code_hash)
            .unwrap();

        // Consume after expiration returns None.
        let result = db
            .consume_oauth_authorization_code_by_hash(&code_hash, now + 301)
            .unwrap();
        assert!(result.is_none(), "expired code should return None");
    }

    #[test]
    fn consume_authorization_code_revoked_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");

        let plaintext_code = crate::auth::generate_oauth_authorization_code();
        let code_hash = crate::auth::hash_token(&plaintext_code);
        let now = chrono::Utc::now().timestamp();
        let record = OAuthAuthorizationCodeRecord {
            id: uuid::Uuid::new_v4().to_string(),
            code_hash: code_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            redirect_uri: "https://example.com/callback".to_string(),
            scopes: "runtime:read".to_string(),
            code_challenge: None,
            code_challenge_method: None,
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 300,
            used_at: None,
            revoked_at: None,
        };
        db.insert_oauth_authorization_code(&record, &code_hash)
            .unwrap();

        // Revoke, then consume returns None.
        db.revoke_oauth_authorization_code(&record.id, now + 5)
            .unwrap();
        let result = db
            .consume_oauth_authorization_code_by_hash(&code_hash, now + 10)
            .unwrap();
        assert!(result.is_none(), "revoked code should return None");
    }

    #[test]
    fn consume_authorization_code_unknown_hash_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let now = chrono::Utc::now().timestamp();

        let result = db
            .consume_oauth_authorization_code_by_hash("nonexistent-hash", now)
            .unwrap();
        assert!(result.is_none(), "unknown hash should return None");
    }

    #[test]
    fn exchange_authorization_code_rejects_subject_mismatch_with_consumed_code() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");
        let now = chrono::Utc::now().timestamp();
        let code_hash = "code-hash-subject-mismatch".to_string();
        let code = OAuthAuthorizationCodeRecord {
            id: uuid::Uuid::new_v4().to_string(),
            code_hash: code_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "shared_key".to_string(),
            subject_id: "hash-a".to_string(),
            user_id: None,
            redirect_uri: "https://example.com/callback".to_string(),
            scopes: "runtime:read".to_string(),
            code_challenge: None,
            code_challenge_method: None,
            resource: None,
            shared_key_hash: Some("hash-a".to_string()),
            created_at: now,
            expires_at: now + 300,
            used_at: None,
            revoked_at: None,
        };
        db.insert_oauth_authorization_code(&code, &code_hash)
            .unwrap();

        let access = OAuthAccessTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: "access-hash-mismatch".to_string(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 3600,
            revoked_at: None,
            last_used_at: None,
        };
        let refresh = OAuthRefreshTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: "refresh-hash-mismatch".to_string(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 2_592_000,
            revoked_at: None,
            last_used_at: None,
            rotated_from_id: None,
        };

        let err = db
            .exchange_oauth_authorization_code_for_tokens(&code_hash, now + 10, &access, &refresh)
            .expect_err("subject mismatch must abort exchange");
        assert!(err.to_string().contains("OAuth token subjects must match"));
        let conn = db.conn_for_tests();
        let used_at: Option<i64> = conn
            .query_row(
                "SELECT used_at FROM oauth_authorization_codes WHERE id = ?1",
                [&code.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(used_at, None);
        let access_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM oauth_access_tokens WHERE id = ?1",
                [&access.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(access_count, 0);
    }

    #[test]
    fn rotate_refresh_token_rejects_subject_mismatch_with_old_refresh() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");
        let now = chrono::Utc::now().timestamp();
        let old = OAuthRefreshTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: "old-refresh-hash".to_string(),
            client_id: client.client_id.clone(),
            subject_kind: "shared_key".to_string(),
            subject_id: "hash-a".to_string(),
            user_id: None,
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: Some("hash-a".to_string()),
            created_at: now,
            expires_at: now + 2_592_000,
            revoked_at: None,
            last_used_at: None,
            rotated_from_id: None,
        };
        db.insert_oauth_refresh_token(&old).unwrap();

        let access = OAuthAccessTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: "new-access-hash".to_string(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 3600,
            revoked_at: None,
            last_used_at: None,
        };
        let refresh = OAuthRefreshTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: "new-refresh-hash".to_string(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 2_592_000,
            revoked_at: None,
            last_used_at: None,
            rotated_from_id: Some(old.id.clone()),
        };

        let err = db
            .rotate_oauth_refresh_token(
                &old.token_hash,
                &client.client_id,
                now + 10,
                &access,
                &refresh,
            )
            .expect_err("subject mismatch must abort rotation");
        assert!(err.to_string().contains("OAuth token subjects must match"));
        let fetched_old = db
            .get_oauth_refresh_token_by_hash(&old.token_hash)
            .unwrap()
            .unwrap();
        assert_eq!(fetched_old.revoked_at, None);
        assert_eq!(fetched_old.last_used_at, None);
        let conn = db.conn_for_tests();
        let access_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM oauth_access_tokens WHERE id = ?1",
                [&access.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(access_count, 0);
    }
}
