//! SQLite authority for the project-bound connector task model.
//!
//! These tables deliberately do not mirror or dual-write the legacy workflow
//! session ledger. A connector task is the product-level unit of work; a run is
//! one executor attempt; events are its bounded, ordered audit trail.

use super::Database;
use rusqlite::{params, OptionalExtension, Transaction};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

const CONNECTOR_CAPABILITIES_JSON: &str = r#"["task_start","files_read","files_search","edits_apply","checks_run","commands_run","task_review","task_finish"]"#;

pub(crate) struct ConnectorBinding<'a> {
    pub project_id: &'a str,
    pub project_name: &'a str,
    pub workspace_id: &'a str,
    pub executor_ref: &'a str,
    pub subject_id: &'a str,
    pub profile: &'a str,
    pub now: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(crate) struct ConnectorTaskSnapshot {
    pub task_id: String,
    pub run_id: String,
    pub project_id: String,
    pub workspace_id: String,
    #[serde(skip_serializing)]
    pub owner_subject_id: String,
    pub goal: String,
    pub mode: String,
    pub task_status: String,
    pub run_status: String,
    pub event_cursor: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(crate) struct ConnectorTaskEvent {
    pub event_id: String,
    pub sequence: i64,
    pub kind: String,
    pub payload: Value,
    pub created_at: i64,
}

#[derive(Debug)]
pub(crate) enum ConnectorTaskStoreError {
    NotFound,
    InvalidState(String),
    Storage(anyhow::Error),
}

impl std::fmt::Display for ConnectorTaskStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "task not found"),
            Self::InvalidState(message) => write!(f, "{message}"),
            Self::Storage(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ConnectorTaskStoreError {}

impl From<rusqlite::Error> for ConnectorTaskStoreError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Storage(value.into())
    }
}

impl From<serde_json::Error> for ConnectorTaskStoreError {
    fn from(value: serde_json::Error) -> Self {
        Self::Storage(value.into())
    }
}

impl Database {
    pub(crate) fn ensure_connector_binding(
        &self,
        binding: ConnectorBinding<'_>,
    ) -> Result<(), ConnectorTaskStoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO wc_projects (id, name, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, updated_at = excluded.updated_at",
            params![binding.project_id, binding.project_name, binding.now],
        )?;
        tx.execute(
            "INSERT INTO wc_workspaces (id, project_id, executor_ref, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?4)
             ON CONFLICT(id) DO UPDATE SET
                 project_id = excluded.project_id,
                 executor_ref = excluded.executor_ref,
                 updated_at = excluded.updated_at",
            params![
                binding.workspace_id,
                binding.project_id,
                binding.executor_ref,
                binding.now
            ],
        )?;
        tx.execute(
            "INSERT INTO wc_connector_grants
                (id, project_id, subject_id, profile, capabilities_json, created_at, updated_at, revoked_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, NULL)
             ON CONFLICT(project_id, subject_id) DO UPDATE SET
                 profile = excluded.profile,
                 capabilities_json = excluded.capabilities_json,
                 updated_at = excluded.updated_at,
                 revoked_at = NULL",
            params![
                new_id("wc_cgr"),
                binding.project_id,
                binding.subject_id,
                binding.profile,
                CONNECTOR_CAPABILITIES_JSON,
                binding.now
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn start_connector_task(
        &self,
        project_id: &str,
        workspace_id: &str,
        subject_id: &str,
        goal: &str,
        mode: &str,
        now: i64,
    ) -> Result<ConnectorTaskSnapshot, ConnectorTaskStoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let granted = tx
            .query_row(
                "SELECT 1 FROM wc_connector_grants
                 WHERE project_id = ?1 AND subject_id = ?2 AND revoked_at IS NULL",
                params![project_id, subject_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !granted {
            return Err(ConnectorTaskStoreError::NotFound);
        }

        let task_id = new_id("wc_task");
        let run_id = new_id("wc_run");
        tx.execute(
            "INSERT INTO wc_tasks
                (id, project_id, owner_subject_id, goal, mode, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6, ?6)",
            params![task_id, project_id, subject_id, goal, mode, now],
        )?;
        tx.execute(
            "INSERT INTO wc_runs (id, task_id, workspace_id, status, started_at, finished_at)
             VALUES (?1, ?2, ?3, 'running', ?4, NULL)",
            params![run_id, task_id, workspace_id, now],
        )?;
        insert_event(
            &tx,
            &task_id,
            &run_id,
            1,
            "task_started",
            &serde_json::json!({ "goal": goal, "mode": mode }),
            now,
        )?;
        tx.commit()?;

        Ok(ConnectorTaskSnapshot {
            task_id,
            run_id,
            project_id: project_id.to_string(),
            workspace_id: workspace_id.to_string(),
            owner_subject_id: subject_id.to_string(),
            goal: goal.to_string(),
            mode: mode.to_string(),
            task_status: "active".to_string(),
            run_status: "running".to_string(),
            event_cursor: 1,
        })
    }

    pub(crate) fn connector_task(
        &self,
        task_id: &str,
        project_id: &str,
        subject_id: &str,
    ) -> Result<ConnectorTaskSnapshot, ConnectorTaskStoreError> {
        let conn = self.conn.lock().unwrap();
        load_task(&conn, task_id, project_id, subject_id)?.ok_or(ConnectorTaskStoreError::NotFound)
    }

    pub(crate) fn append_connector_task_event(
        &self,
        task_id: &str,
        project_id: &str,
        subject_id: &str,
        kind: &str,
        payload: &Value,
        now: i64,
    ) -> Result<i64, ConnectorTaskStoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let task = load_task(&tx, task_id, project_id, subject_id)?
            .ok_or(ConnectorTaskStoreError::NotFound)?;
        require_running(&task)?;
        let sequence = task.event_cursor + 1;
        insert_event(
            &tx,
            &task.task_id,
            &task.run_id,
            sequence,
            kind,
            payload,
            now,
        )?;
        tx.execute(
            "UPDATE wc_tasks SET updated_at = ?1 WHERE id = ?2",
            params![now, task_id],
        )?;
        tx.commit()?;
        Ok(sequence)
    }

    pub(crate) fn finish_connector_task(
        &self,
        task_id: &str,
        project_id: &str,
        subject_id: &str,
        payload: &Value,
        now: i64,
    ) -> Result<i64, ConnectorTaskStoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let task = load_task(&tx, task_id, project_id, subject_id)?
            .ok_or(ConnectorTaskStoreError::NotFound)?;
        require_running(&task)?;
        let sequence = task.event_cursor + 1;
        insert_event(
            &tx,
            &task.task_id,
            &task.run_id,
            sequence,
            "task_finished",
            payload,
            now,
        )?;
        tx.execute(
            "UPDATE wc_runs SET status = 'completed', finished_at = ?1 WHERE id = ?2",
            params![now, task.run_id],
        )?;
        tx.execute(
            "UPDATE wc_tasks SET status = 'ready_for_review', updated_at = ?1 WHERE id = ?2",
            params![now, task.task_id],
        )?;
        tx.commit()?;
        Ok(sequence)
    }

    pub(crate) fn connector_task_events(
        &self,
        task_id: &str,
        project_id: &str,
        subject_id: &str,
        limit: usize,
    ) -> Result<Vec<ConnectorTaskEvent>, ConnectorTaskStoreError> {
        let conn = self.conn.lock().unwrap();
        if load_task(&conn, task_id, project_id, subject_id)?.is_none() {
            return Err(ConnectorTaskStoreError::NotFound);
        }
        let mut statement = conn.prepare(
            "SELECT id, sequence, kind, payload_json, created_at
             FROM wc_task_events
             WHERE task_id = ?1
             ORDER BY sequence DESC
             LIMIT ?2",
        )?;
        let rows = statement.query_map(params![task_id, limit as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?;
        let mut events = Vec::new();
        for row in rows {
            let (event_id, sequence, kind, payload_json, created_at) = row?;
            events.push(ConnectorTaskEvent {
                event_id,
                sequence,
                kind,
                payload: serde_json::from_str(&payload_json)?,
                created_at,
            });
        }
        events.reverse();
        Ok(events)
    }
}

fn load_task(
    conn: &rusqlite::Connection,
    task_id: &str,
    project_id: &str,
    subject_id: &str,
) -> Result<Option<ConnectorTaskSnapshot>, rusqlite::Error> {
    conn.query_row(
        "SELECT t.id, r.id, t.project_id, r.workspace_id, t.owner_subject_id, t.goal, t.mode,
                t.status, r.status, COALESCE(MAX(e.sequence), 0)
         FROM wc_tasks t
         JOIN wc_runs r ON r.task_id = t.id
         LEFT JOIN wc_task_events e ON e.task_id = t.id AND e.run_id = r.id
         WHERE t.id = ?1 AND t.project_id = ?2 AND t.owner_subject_id = ?3
         GROUP BY t.id, r.id
         ORDER BY r.started_at DESC
         LIMIT 1",
        params![task_id, project_id, subject_id],
        |row| {
            Ok(ConnectorTaskSnapshot {
                task_id: row.get(0)?,
                run_id: row.get(1)?,
                project_id: row.get(2)?,
                workspace_id: row.get(3)?,
                owner_subject_id: row.get(4)?,
                goal: row.get(5)?,
                mode: row.get(6)?,
                task_status: row.get(7)?,
                run_status: row.get(8)?,
                event_cursor: row.get(9)?,
            })
        },
    )
    .optional()
}

fn require_running(task: &ConnectorTaskSnapshot) -> Result<(), ConnectorTaskStoreError> {
    if task.task_status != "active" || task.run_status != "running" {
        return Err(ConnectorTaskStoreError::InvalidState(format!(
            "task {} is {}, run is {}; start a new task for more work",
            task.task_id, task.task_status, task.run_status
        )));
    }
    Ok(())
}

fn insert_event(
    tx: &Transaction<'_>,
    task_id: &str,
    run_id: &str,
    sequence: i64,
    kind: &str,
    payload: &Value,
    now: i64,
) -> Result<(), ConnectorTaskStoreError> {
    tx.execute(
        "INSERT INTO wc_task_events
            (id, task_id, run_id, sequence, kind, payload_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            new_id("wc_evt"),
            task_id,
            run_id,
            sequence,
            kind,
            serde_json::to_string(payload)?,
            now
        ],
    )?;
    Ok(())
}

fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database() -> (tempfile::TempDir, Database) {
        let temp = tempfile::tempdir().unwrap();
        let db = Database::open(&temp.path().join("task-kernel.db")).unwrap();
        (temp, db)
    }

    fn bind(db: &Database, subject: &str) {
        db.ensure_connector_binding(ConnectorBinding {
            project_id: "wc_proj_demo",
            project_name: "demo",
            workspace_id: "wc_ws_demo",
            executor_ref: "agent:hosted:demo",
            subject_id: subject,
            profile: "personal",
            now: 100,
        })
        .unwrap();
    }

    #[test]
    fn start_creates_task_run_and_first_monotonic_event() {
        let (_temp, db) = database();
        bind(&db, "user:one");
        let task = db
            .start_connector_task(
                "wc_proj_demo",
                "wc_ws_demo",
                "user:one",
                "fix the parser",
                "normal",
                101,
            )
            .unwrap();
        assert!(task.task_id.starts_with("wc_task_"));
        assert!(task.run_id.starts_with("wc_run_"));
        assert_eq!(task.event_cursor, 1);

        let cursor = db
            .append_connector_task_event(
                &task.task_id,
                "wc_proj_demo",
                "user:one",
                "files_read",
                &serde_json::json!({ "ok": true, "file_count": 2 }),
                102,
            )
            .unwrap();
        assert_eq!(cursor, 2);
        let events = db
            .connector_task_events(&task.task_id, "wc_proj_demo", "user:one", 20)
            .unwrap();
        assert_eq!(
            events.iter().map(|e| e.sequence).collect::<Vec<_>>(),
            vec![1, 2]
        );
    }

    #[test]
    fn task_access_is_subject_and_project_scoped() {
        let (_temp, db) = database();
        bind(&db, "user:one");
        bind(&db, "user:two");
        let task = db
            .start_connector_task(
                "wc_proj_demo",
                "wc_ws_demo",
                "user:one",
                "private task",
                "normal",
                101,
            )
            .unwrap();
        assert!(matches!(
            db.connector_task(&task.task_id, "wc_proj_demo", "user:two"),
            Err(ConnectorTaskStoreError::NotFound)
        ));
        assert!(matches!(
            db.connector_task(&task.task_id, "wc_proj_other", "user:one"),
            Err(ConnectorTaskStoreError::NotFound)
        ));
    }

    #[test]
    fn finish_is_atomic_and_prevents_more_events() {
        let (_temp, db) = database();
        bind(&db, "user:one");
        let task = db
            .start_connector_task(
                "wc_proj_demo",
                "wc_ws_demo",
                "user:one",
                "finish me",
                "normal",
                101,
            )
            .unwrap();
        let cursor = db
            .finish_connector_task(
                &task.task_id,
                "wc_proj_demo",
                "user:one",
                &serde_json::json!({ "summary": "done" }),
                102,
            )
            .unwrap();
        assert_eq!(cursor, 2);
        let snapshot = db
            .connector_task(&task.task_id, "wc_proj_demo", "user:one")
            .unwrap();
        assert_eq!(snapshot.task_status, "ready_for_review");
        assert_eq!(snapshot.run_status, "completed");
        assert!(matches!(
            db.append_connector_task_event(
                &task.task_id,
                "wc_proj_demo",
                "user:one",
                "files_read",
                &serde_json::json!({}),
                103,
            ),
            Err(ConnectorTaskStoreError::InvalidState(_))
        ));
    }
}
