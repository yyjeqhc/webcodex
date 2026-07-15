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

pub(crate) struct NewConnectorTask<'a> {
    pub task_id: &'a str,
    pub run_id: &'a str,
    pub project_id: &'a str,
    pub workspace_id: &'a str,
    pub subject_id: &'a str,
    pub goal: &'a str,
    pub mode: &'a str,
    pub target_executor_ref: &'a str,
    pub execution_executor_ref: &'a str,
    pub target_root: &'a str,
    pub execution_root: &'a str,
    pub baseline_commit: Option<&'a str>,
    pub baseline_tree: Option<&'a str>,
    pub isolated: bool,
    pub now: i64,
}

pub(crate) struct NewConnectorResult<'a> {
    pub result_id: &'a str,
    pub summary: &'a str,
    pub patch_artifact: Option<&'a str>,
    pub patch_sha256: Option<&'a str>,
    pub patch_bytes: usize,
    pub changed_paths: &'a [String],
    pub validation: &'a Value,
    pub warnings: &'a [String],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConnectorPreservedWorkspace {
    pub task_id: String,
    pub run_id: String,
    pub execution_root: String,
    pub execution_executor_ref: String,
    pub baseline_commit: Option<String>,
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
    #[serde(skip_serializing)]
    pub target_executor_ref: String,
    #[serde(skip_serializing)]
    pub execution_executor_ref: String,
    #[serde(skip_serializing)]
    pub target_root: String,
    #[serde(skip_serializing)]
    pub execution_root: String,
    #[serde(skip_serializing)]
    pub baseline_commit: Option<String>,
    #[serde(skip_serializing)]
    pub baseline_tree: Option<String>,
    pub isolated: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(crate) struct ConnectorTaskResult {
    pub result_id: String,
    pub task_id: String,
    pub run_id: String,
    pub summary: String,
    #[serde(skip_serializing)]
    pub patch_artifact: Option<String>,
    pub patch_sha256: Option<String>,
    pub patch_bytes: usize,
    pub changed_paths: Vec<String>,
    pub validation: Value,
    pub warnings: Vec<String>,
    pub decision_status: String,
    pub decided_by: Option<String>,
    pub decided_at: Option<i64>,
    pub cleanup_warning: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(crate) struct ConnectorApproval {
    pub approval_id: String,
    pub task_id: String,
    pub run_id: String,
    pub action_kind: String,
    pub action_hash: String,
    pub action_summary: String,
    pub state: String,
    pub requested_at: i64,
    pub expires_at: i64,
    pub decided_by: Option<String>,
    pub decided_at: Option<i64>,
    pub consumed_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ConnectorApprovalGate {
    Pending(ConnectorApproval),
    Denied(ConnectorApproval),
    Expired(ConnectorApproval),
    Consumed(ConnectorApproval),
    Authorized(ConnectorApproval),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ConnectorEditOperationGate {
    Started,
    Replay(Value),
    Pending,
    Conflict,
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
        task: NewConnectorTask<'_>,
    ) -> Result<ConnectorTaskSnapshot, ConnectorTaskStoreError> {
        match task.mode {
            "normal"
                if !task.isolated
                    || task.baseline_commit.is_none()
                    || task.baseline_tree.is_none()
                    || task.execution_root == task.target_root =>
            {
                return Err(ConnectorTaskStoreError::InvalidState(
                    "normal tasks require an isolated execution root and Git baseline".to_string(),
                ));
            }
            "read_only" if task.isolated || task.execution_root != task.target_root => {
                return Err(ConnectorTaskStoreError::InvalidState(
                    "read_only tasks must use the target workspace without isolation".to_string(),
                ));
            }
            "normal" | "read_only" => {}
            _ => {
                return Err(ConnectorTaskStoreError::InvalidState(
                    "task mode must be normal or read_only".to_string(),
                ))
            }
        }
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let granted = tx
            .query_row(
                "SELECT 1 FROM wc_connector_grants
                 WHERE project_id = ?1 AND subject_id = ?2 AND revoked_at IS NULL",
                params![task.project_id, task.subject_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !granted {
            return Err(ConnectorTaskStoreError::NotFound);
        }

        tx.execute(
            "INSERT INTO wc_tasks
                (id, project_id, owner_subject_id, goal, mode, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6, ?6)",
            params![
                task.task_id,
                task.project_id,
                task.subject_id,
                task.goal,
                task.mode,
                task.now
            ],
        )?;
        tx.execute(
            "INSERT INTO wc_runs (id, task_id, workspace_id, status, started_at, finished_at)
             VALUES (?1, ?2, ?3, 'running', ?4, NULL)",
            params![task.run_id, task.task_id, task.workspace_id, task.now],
        )?;
        tx.execute(
            "INSERT INTO wc_run_contexts
                (run_id, target_executor_ref, execution_executor_ref, target_root,
                 execution_root, baseline_commit, baseline_tree, isolated, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                task.run_id,
                task.target_executor_ref,
                task.execution_executor_ref,
                task.target_root,
                task.execution_root,
                task.baseline_commit,
                task.baseline_tree,
                i64::from(task.isolated),
                task.now
            ],
        )?;
        insert_event(
            &tx,
            task.task_id,
            task.run_id,
            1,
            "task_started",
            &serde_json::json!({
                "goal": task.goal,
                "mode": task.mode,
                "isolated": task.isolated,
                "baseline_commit": task.baseline_commit
            }),
            task.now,
        )?;
        tx.commit()?;

        Ok(ConnectorTaskSnapshot {
            task_id: task.task_id.to_string(),
            run_id: task.run_id.to_string(),
            project_id: task.project_id.to_string(),
            workspace_id: task.workspace_id.to_string(),
            owner_subject_id: task.subject_id.to_string(),
            goal: task.goal.to_string(),
            mode: task.mode.to_string(),
            task_status: "active".to_string(),
            run_status: "running".to_string(),
            event_cursor: 1,
            target_executor_ref: task.target_executor_ref.to_string(),
            execution_executor_ref: task.execution_executor_ref.to_string(),
            target_root: task.target_root.to_string(),
            execution_root: task.execution_root.to_string(),
            baseline_commit: task.baseline_commit.map(str::to_string),
            baseline_tree: task.baseline_tree.map(str::to_string),
            isolated: task.isolated,
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

    pub(crate) fn begin_connector_edit_operation(
        &self,
        task_id: &str,
        project_id: &str,
        subject_id: &str,
        operation_id: &str,
        request_sha256: &str,
        now: i64,
    ) -> Result<ConnectorEditOperationGate, ConnectorTaskStoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let task = load_task(&tx, task_id, project_id, subject_id)?
            .ok_or(ConnectorTaskStoreError::NotFound)?;
        require_running(&task)?;
        let existing = tx
            .query_row(
                "SELECT request_sha256, state, result_json
                 FROM wc_edit_operations
                 WHERE task_id = ?1 AND operation_id = ?2",
                params![task_id, operation_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()?;
        if let Some((stored_hash, state, result_json)) = existing {
            tx.commit()?;
            if stored_hash != request_sha256 {
                return Ok(ConnectorEditOperationGate::Conflict);
            }
            return match (state.as_str(), result_json) {
                ("pending", None) => Ok(ConnectorEditOperationGate::Pending),
                ("completed", Some(result_json)) => Ok(ConnectorEditOperationGate::Replay(
                    serde_json::from_str(&result_json)?,
                )),
                ("failed", None) => {
                    let updated = conn.execute(
                        "UPDATE wc_edit_operations SET state = 'pending', updated_at = ?1
                         WHERE task_id = ?2 AND operation_id = ?3 AND request_sha256 = ?4
                           AND state = 'failed'",
                        params![now, task_id, operation_id, request_sha256],
                    )?;
                    if updated == 1 {
                        Ok(ConnectorEditOperationGate::Started)
                    } else {
                        Ok(ConnectorEditOperationGate::Pending)
                    }
                }
                _ => Err(ConnectorTaskStoreError::InvalidState(
                    "edit operation state is inconsistent".to_string(),
                )),
            };
        }
        tx.execute(
            "INSERT INTO wc_edit_operations
                (task_id, operation_id, request_sha256, state, result_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'pending', NULL, ?4, ?4)",
            params![task_id, operation_id, request_sha256, now],
        )?;
        tx.commit()?;
        Ok(ConnectorEditOperationGate::Started)
    }

    pub(crate) fn complete_connector_edit_operation(
        &self,
        task_id: &str,
        project_id: &str,
        subject_id: &str,
        operation_id: &str,
        request_sha256: &str,
        result: &Value,
        now: i64,
    ) -> Result<(), ConnectorTaskStoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let task = load_task(&tx, task_id, project_id, subject_id)?
            .ok_or(ConnectorTaskStoreError::NotFound)?;
        require_running(&task)?;
        let updated = tx.execute(
            "UPDATE wc_edit_operations
             SET state = 'completed', result_json = ?1, updated_at = ?2
             WHERE task_id = ?3 AND operation_id = ?4 AND request_sha256 = ?5
               AND state = 'pending'",
            params![
                serde_json::to_string(result)?,
                now,
                task_id,
                operation_id,
                request_sha256
            ],
        )?;
        if updated != 1 {
            return Err(ConnectorTaskStoreError::InvalidState(
                "edit operation could not be completed".to_string(),
            ));
        }
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn fail_connector_edit_operation(
        &self,
        task_id: &str,
        operation_id: &str,
        request_sha256: &str,
        now: i64,
    ) -> Result<(), ConnectorTaskStoreError> {
        let conn = self.conn.lock().unwrap();
        let updated = conn.execute(
            "UPDATE wc_edit_operations SET state = 'failed', updated_at = ?1
             WHERE task_id = ?2 AND operation_id = ?3 AND request_sha256 = ?4
               AND state = 'pending'",
            params![now, task_id, operation_id, request_sha256],
        )?;
        if updated != 1 {
            return Err(ConnectorTaskStoreError::InvalidState(
                "edit operation could not be marked failed".to_string(),
            ));
        }
        Ok(())
    }

    pub(crate) fn finish_connector_task(
        &self,
        task_id: &str,
        project_id: &str,
        subject_id: &str,
        result: NewConnectorResult<'_>,
        now: i64,
    ) -> Result<i64, ConnectorTaskStoreError> {
        let patch_metadata_valid = match result.patch_bytes {
            0 => result.patch_artifact.is_none() && result.patch_sha256.is_none(),
            _ => result.patch_artifact.is_some() && result.patch_sha256.is_some(),
        };
        if !patch_metadata_valid {
            return Err(ConnectorTaskStoreError::InvalidState(
                "task result patch metadata is inconsistent".to_string(),
            ));
        }
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
            &serde_json::json!({
                "result_id": result.result_id,
                "summary": result.summary,
                "patch_sha256": result.patch_sha256,
                "patch_bytes": result.patch_bytes,
                "changed_file_count": result.changed_paths.len(),
                "warning_count": result.warnings.len()
            }),
            now,
        )?;
        tx.execute(
            "INSERT INTO wc_task_results
                (id, task_id, run_id, summary, patch_artifact, patch_sha256, patch_bytes,
                 changed_paths_json, validation_json, warnings_json, decision_status,
                 decided_by, decided_at, cleanup_warning, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'pending',
                     NULL, NULL, NULL, ?11)",
            params![
                result.result_id,
                task.task_id,
                task.run_id,
                result.summary,
                result.patch_artifact,
                result.patch_sha256,
                result.patch_bytes as i64,
                serde_json::to_string(result.changed_paths)?,
                serde_json::to_string(result.validation)?,
                serde_json::to_string(result.warnings)?,
                now
            ],
        )?;
        tx.execute(
            "UPDATE wc_runs SET status = 'completed', finished_at = ?1 WHERE id = ?2",
            params![now, task.run_id],
        )?;
        tx.execute(
            "UPDATE wc_tasks SET status = 'ready_for_review', updated_at = ?1 WHERE id = ?2",
            params![now, task.task_id],
        )?;
        tx.execute(
            "UPDATE wc_approvals SET state = 'expired'
             WHERE task_id = ?1 AND state IN ('pending', 'approved')",
            params![task.task_id],
        )?;
        tx.commit()?;
        Ok(sequence)
    }

    pub(crate) fn record_connector_workspace_release(
        &self,
        task_id: &str,
        project_id: &str,
        subject_id: &str,
        released: bool,
        cleanup_warning: Option<&str>,
        now: i64,
    ) -> Result<i64, ConnectorTaskStoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let task = load_task(&tx, task_id, project_id, subject_id)?
            .ok_or(ConnectorTaskStoreError::NotFound)?;
        if !matches!(
            task.task_status.as_str(),
            "ready_for_review" | "accepted" | "rejected"
        ) || task.run_status != "completed"
        {
            return Err(ConnectorTaskStoreError::InvalidState(
                "workspace release can only follow a completed task result".to_string(),
            ));
        }
        if load_result(&tx, task_id)?.is_none() {
            return Err(ConnectorTaskStoreError::InvalidState(
                "workspace release requires a stable task result".to_string(),
            ));
        }
        if let Some(warning) = cleanup_warning {
            tx.execute(
                "UPDATE wc_task_results
                 SET cleanup_warning = CASE
                     WHEN cleanup_warning IS NULL THEN ?1
                     ELSE cleanup_warning || '; ' || ?1
                 END
                 WHERE task_id = ?2",
                params![warning, task_id],
            )?;
        } else if released {
            tx.execute(
                "UPDATE wc_task_results SET cleanup_warning = NULL WHERE task_id = ?1",
                params![task_id],
            )?;
        }
        let sequence = task.event_cursor + 1;
        insert_event(
            &tx,
            task_id,
            &task.run_id,
            sequence,
            "workspace_release",
            &serde_json::json!({
                "released": released,
                "cleanup_warning": cleanup_warning
            }),
            now,
        )?;
        tx.execute(
            "UPDATE wc_tasks SET updated_at = ?1 WHERE id = ?2",
            params![now, task_id],
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

    pub(crate) fn connector_task_result(
        &self,
        task_id: &str,
        project_id: &str,
        subject_id: &str,
    ) -> Result<Option<ConnectorTaskResult>, ConnectorTaskStoreError> {
        let conn = self.conn.lock().unwrap();
        if load_task(&conn, task_id, project_id, subject_id)?.is_none() {
            return Err(ConnectorTaskStoreError::NotFound);
        }
        load_result(&conn, task_id)
    }

    pub(crate) fn interrupt_connector_runs(
        &self,
        project_id: &str,
        now: i64,
    ) -> Result<usize, ConnectorTaskStoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let runs = {
            let mut statement = tx.prepare(
                "SELECT t.id, r.id, COALESCE(MAX(e.sequence), 0)
                 FROM wc_tasks t
                 JOIN wc_runs r ON r.task_id = t.id
                 LEFT JOIN wc_task_events e ON e.task_id = t.id
                 WHERE t.project_id = ?1 AND r.status = 'running'
                 GROUP BY t.id, r.id",
            )?;
            let rows = statement.query_map(params![project_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        for (task_id, run_id, cursor) in &runs {
            tx.execute(
                "UPDATE wc_runs SET status = 'interrupted', finished_at = ?1 WHERE id = ?2",
                params![now, run_id],
            )?;
            insert_event(
                &tx,
                task_id,
                run_id,
                cursor + 1,
                "run_interrupted",
                &serde_json::json!({
                    "reason": "runtime_restarted",
                    "recoverable": true
                }),
                now,
            )?;
            tx.execute(
                "UPDATE wc_tasks SET updated_at = ?1 WHERE id = ?2",
                params![now, task_id],
            )?;
            tx.execute(
                "UPDATE wc_approvals SET state = 'expired'
                 WHERE task_id = ?1 AND state IN ('pending', 'approved')",
                params![task_id],
            )?;
        }
        tx.commit()?;
        Ok(runs.len())
    }

    pub(crate) fn connector_preserved_workspaces(
        &self,
        project_id: &str,
    ) -> Result<Vec<ConnectorPreservedWorkspace>, ConnectorTaskStoreError> {
        let conn = self.conn.lock().unwrap();
        let mut statement = conn.prepare(
            "SELECT t.id, r.id, ctx.execution_root, ctx.execution_executor_ref,
                    ctx.baseline_commit
             FROM wc_tasks t
             JOIN wc_runs r ON r.task_id = t.id
             JOIN wc_run_contexts ctx ON ctx.run_id = r.id
             WHERE t.project_id = ?1 AND r.status = 'interrupted' AND ctx.isolated = 1
             ORDER BY r.started_at ASC",
        )?;
        let rows = statement.query_map(params![project_id], |row| {
            Ok(ConnectorPreservedWorkspace {
                task_id: row.get(0)?,
                run_id: row.get(1)?,
                execution_root: row.get(2)?,
                execution_executor_ref: row.get(3)?,
                baseline_commit: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub(crate) fn request_or_consume_connector_approval(
        &self,
        task_id: &str,
        project_id: &str,
        subject_id: &str,
        action_kind: &str,
        action_hash: &str,
        action_summary: &str,
        now: i64,
        expires_at: i64,
    ) -> Result<ConnectorApprovalGate, ConnectorTaskStoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let task = load_task(&tx, task_id, project_id, subject_id)?
            .ok_or(ConnectorTaskStoreError::NotFound)?;
        require_running(&task)?;
        let mut approval = load_approval_by_hash(&tx, task_id, &task.run_id, action_hash)?;
        if approval.is_none() {
            let approval_id = new_id("wc_apr");
            tx.execute(
                "INSERT INTO wc_approvals
                    (id, task_id, run_id, action_kind, action_hash, action_summary, state,
                     requested_at, expires_at, decided_by, decided_at, consumed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', ?7, ?8, NULL, NULL, NULL)",
                params![
                    approval_id,
                    task_id,
                    task.run_id,
                    action_kind,
                    action_hash,
                    action_summary,
                    now,
                    expires_at
                ],
            )?;
            insert_event(
                &tx,
                task_id,
                &task.run_id,
                task.event_cursor + 1,
                "approval_requested",
                &serde_json::json!({
                    "approval_id": approval_id,
                    "action_kind": action_kind,
                    "action_hash": action_hash,
                    "expires_at": expires_at
                }),
                now,
            )?;
            tx.execute(
                "UPDATE wc_tasks SET updated_at = ?1 WHERE id = ?2",
                params![now, task_id],
            )?;
            approval = load_approval_by_hash(&tx, task_id, &task.run_id, action_hash)?;
        }
        let mut approval = approval.expect("approval inserted or loaded");
        let gate = match approval.state.as_str() {
            "pending" if approval.expires_at <= now => {
                tx.execute(
                    "UPDATE wc_approvals SET state = 'expired' WHERE id = ?1 AND state = 'pending'",
                    params![approval.approval_id],
                )?;
                approval.state = "expired".to_string();
                ConnectorApprovalGate::Expired(approval)
            }
            "pending" => ConnectorApprovalGate::Pending(approval),
            "approved" if approval.expires_at <= now => {
                tx.execute(
                    "UPDATE wc_approvals SET state = 'expired' WHERE id = ?1 AND state = 'approved'",
                    params![approval.approval_id],
                )?;
                approval.state = "expired".to_string();
                ConnectorApprovalGate::Expired(approval)
            }
            "approved" => {
                let updated = tx.execute(
                    "UPDATE wc_approvals SET state = 'consumed', consumed_at = ?1
                     WHERE id = ?2 AND state = 'approved'",
                    params![now, approval.approval_id],
                )?;
                if updated != 1 {
                    return Err(ConnectorTaskStoreError::InvalidState(
                        "approval was already consumed".to_string(),
                    ));
                }
                approval.state = "consumed".to_string();
                approval.consumed_at = Some(now);
                insert_event(
                    &tx,
                    task_id,
                    &task.run_id,
                    task.event_cursor + 1,
                    "approval_consumed",
                    &serde_json::json!({
                        "approval_id": approval.approval_id,
                        "action_hash": approval.action_hash
                    }),
                    now,
                )?;
                tx.execute(
                    "UPDATE wc_tasks SET updated_at = ?1 WHERE id = ?2",
                    params![now, task_id],
                )?;
                ConnectorApprovalGate::Authorized(approval)
            }
            "denied" => ConnectorApprovalGate::Denied(approval),
            "expired" => {
                tx.execute(
                    "UPDATE wc_approvals
                     SET state = 'pending', requested_at = ?1, expires_at = ?2,
                         decided_by = NULL, decided_at = NULL, consumed_at = NULL
                     WHERE id = ?3 AND state = 'expired'",
                    params![now, expires_at, approval.approval_id],
                )?;
                insert_event(
                    &tx,
                    task_id,
                    &task.run_id,
                    task.event_cursor + 1,
                    "approval_requested",
                    &serde_json::json!({
                        "approval_id": approval.approval_id,
                        "action_kind": action_kind,
                        "action_hash": action_hash,
                        "expires_at": expires_at,
                        "renewed": true
                    }),
                    now,
                )?;
                tx.execute(
                    "UPDATE wc_tasks SET updated_at = ?1 WHERE id = ?2",
                    params![now, task_id],
                )?;
                approval.state = "pending".to_string();
                approval.requested_at = now;
                approval.expires_at = expires_at;
                approval.decided_by = None;
                approval.decided_at = None;
                approval.consumed_at = None;
                ConnectorApprovalGate::Pending(approval)
            }
            "consumed" => ConnectorApprovalGate::Consumed(approval),
            other => {
                return Err(ConnectorTaskStoreError::InvalidState(format!(
                    "unknown approval state {other}"
                )))
            }
        };
        tx.commit()?;
        Ok(gate)
    }

    pub(crate) fn local_connector_tasks(
        &self,
        project_id: &str,
        limit: usize,
    ) -> Result<Vec<ConnectorTaskSnapshot>, ConnectorTaskStoreError> {
        let conn = self.conn.lock().unwrap();
        let pairs = {
            let mut statement = conn.prepare(
                "SELECT id, owner_subject_id FROM wc_tasks
                 WHERE project_id = ?1 ORDER BY updated_at DESC LIMIT ?2",
            )?;
            let rows = statement.query_map(params![project_id, limit as i64], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let mut tasks = Vec::with_capacity(pairs.len());
        for (task_id, subject_id) in pairs {
            if let Some(task) = load_task(&conn, &task_id, project_id, &subject_id)? {
                tasks.push(task);
            }
        }
        Ok(tasks)
    }

    pub(crate) fn local_connector_task(
        &self,
        task_id: &str,
        project_id: &str,
    ) -> Result<ConnectorTaskSnapshot, ConnectorTaskStoreError> {
        let conn = self.conn.lock().unwrap();
        let subject_id = conn
            .query_row(
                "SELECT owner_subject_id FROM wc_tasks WHERE id = ?1 AND project_id = ?2",
                params![task_id, project_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or(ConnectorTaskStoreError::NotFound)?;
        load_task(&conn, task_id, project_id, &subject_id)?.ok_or(ConnectorTaskStoreError::NotFound)
    }

    pub(crate) fn local_connector_task_result(
        &self,
        task_id: &str,
        project_id: &str,
    ) -> Result<Option<ConnectorTaskResult>, ConnectorTaskStoreError> {
        let conn = self.conn.lock().unwrap();
        let exists = conn
            .query_row(
                "SELECT 1 FROM wc_tasks WHERE id = ?1 AND project_id = ?2",
                params![task_id, project_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !exists {
            return Err(ConnectorTaskStoreError::NotFound);
        }
        load_result(&conn, task_id)
    }

    pub(crate) fn local_connector_task_events(
        &self,
        task_id: &str,
        project_id: &str,
        limit: usize,
    ) -> Result<Vec<ConnectorTaskEvent>, ConnectorTaskStoreError> {
        let task = self.local_connector_task(task_id, project_id)?;
        self.connector_task_events(task_id, project_id, &task.owner_subject_id, limit)
    }

    pub(crate) fn local_connector_task_approvals(
        &self,
        task_id: &str,
        project_id: &str,
    ) -> Result<Vec<ConnectorApproval>, ConnectorTaskStoreError> {
        let conn = self.conn.lock().unwrap();
        let exists = conn
            .query_row(
                "SELECT 1 FROM wc_tasks WHERE id = ?1 AND project_id = ?2",
                params![task_id, project_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !exists {
            return Err(ConnectorTaskStoreError::NotFound);
        }
        let mut statement = conn.prepare(
            "SELECT id, task_id, run_id, action_kind, action_hash, action_summary, state,
                    requested_at, expires_at, decided_by, decided_at, consumed_at
             FROM wc_approvals WHERE task_id = ?1 ORDER BY requested_at DESC",
        )?;
        let rows = statement.query_map(params![task_id], map_approval)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub(crate) fn resume_connector_task(
        &self,
        task_id: &str,
        project_id: &str,
        actor: &str,
        now: i64,
    ) -> Result<ConnectorTaskSnapshot, ConnectorTaskStoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let subject_id = tx
            .query_row(
                "SELECT owner_subject_id FROM wc_tasks WHERE id = ?1 AND project_id = ?2",
                params![task_id, project_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or(ConnectorTaskStoreError::NotFound)?;
        let task = load_task(&tx, task_id, project_id, &subject_id)?
            .ok_or(ConnectorTaskStoreError::NotFound)?;
        if task.run_status != "interrupted" || task.task_status != "needs_attention" {
            return Err(ConnectorTaskStoreError::InvalidState(
                "only an interrupted task can be resumed".to_string(),
            ));
        }
        if load_result(&tx, task_id)?.is_some() {
            return Err(ConnectorTaskStoreError::InvalidState(
                "a task with a stable result cannot be resumed".to_string(),
            ));
        }
        tx.execute(
            "UPDATE wc_runs SET status = 'running', finished_at = NULL WHERE id = ?1",
            params![task.run_id],
        )?;
        insert_event(
            &tx,
            task_id,
            &task.run_id,
            task.event_cursor + 1,
            "run_resumed",
            &serde_json::json!({ "actor": actor }),
            now,
        )?;
        tx.execute(
            "UPDATE wc_tasks SET updated_at = ?1 WHERE id = ?2",
            params![now, task_id],
        )?;
        tx.commit()?;
        load_task(&conn, task_id, project_id, &subject_id)?.ok_or(ConnectorTaskStoreError::NotFound)
    }

    pub(crate) fn abandon_interrupted_connector_task(
        &self,
        task_id: &str,
        project_id: &str,
        actor: &str,
        now: i64,
    ) -> Result<ConnectorTaskResult, ConnectorTaskStoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let (run_id, cursor) = tx
            .query_row(
                "SELECT r.id, COALESCE(MAX(e.sequence), 0)
                 FROM wc_tasks t
                 JOIN wc_runs r ON r.task_id = t.id
                 LEFT JOIN wc_task_events e ON e.task_id = t.id
                 LEFT JOIN wc_task_results result ON result.task_id = t.id
                 WHERE t.id = ?1 AND t.project_id = ?2
                   AND r.status = 'interrupted' AND result.id IS NULL
                 GROUP BY r.id ORDER BY r.started_at DESC LIMIT 1",
                params![task_id, project_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?
            .ok_or_else(|| {
                ConnectorTaskStoreError::InvalidState(
                    "only an interrupted task without a Result can be abandoned".to_string(),
                )
            })?;
        let result_id = new_id("wc_result");
        let warnings = vec!["interrupted workspace changes were discarded locally"];
        tx.execute(
            "INSERT INTO wc_task_results
                (id, task_id, run_id, summary, patch_artifact, patch_sha256, patch_bytes,
                 changed_paths_json, validation_json, warnings_json, decision_status,
                 decided_by, decided_at, cleanup_warning, created_at)
             VALUES (?1, ?2, ?3, ?4, NULL, NULL, 0, '[]', ?5, ?6, 'rejected',
                     ?7, ?8, NULL, ?8)",
            params![
                result_id,
                task_id,
                run_id,
                "Interrupted task abandoned locally without capturing a patch.",
                serde_json::to_string(&serde_json::json!({"status": "not_run"}))?,
                serde_json::to_string(&warnings)?,
                actor,
                now
            ],
        )?;
        insert_event(
            &tx,
            task_id,
            &run_id,
            cursor + 1,
            "task_abandoned",
            &serde_json::json!({
                "actor": actor,
                "result_id": result_id,
                "changes_captured": false
            }),
            now,
        )?;
        tx.execute(
            "UPDATE wc_runs SET status = 'completed', finished_at = ?1 WHERE id = ?2",
            params![now, run_id],
        )?;
        tx.execute(
            "UPDATE wc_tasks SET status = 'ready_for_review', updated_at = ?1 WHERE id = ?2",
            params![now, task_id],
        )?;
        tx.execute(
            "UPDATE wc_approvals SET state = 'expired'
             WHERE task_id = ?1 AND state IN ('pending', 'approved')",
            params![task_id],
        )?;
        tx.commit()?;
        load_result(&conn, task_id)?
            .ok_or_else(|| ConnectorTaskStoreError::Storage(anyhow::anyhow!("result disappeared")))
    }

    pub(crate) fn decide_connector_result(
        &self,
        task_id: &str,
        project_id: &str,
        decision: &str,
        actor: &str,
        cleanup_warning: Option<&str>,
        now: i64,
    ) -> Result<ConnectorTaskResult, ConnectorTaskStoreError> {
        if !matches!(decision, "accepted" | "rejected") {
            return Err(ConnectorTaskStoreError::InvalidState(
                "result decision must be accepted or rejected".to_string(),
            ));
        }
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let (run_id, cursor) = tx
            .query_row(
                "SELECT r.id, COALESCE(MAX(e.sequence), 0)
                 FROM wc_tasks t
                 JOIN wc_runs r ON r.task_id = t.id
                 LEFT JOIN wc_task_events e ON e.task_id = t.id
                 JOIN wc_task_results result ON result.task_id = t.id
                 WHERE t.id = ?1 AND t.project_id = ?2 AND result.decision_status = 'pending'
                 GROUP BY r.id ORDER BY r.started_at DESC LIMIT 1",
                params![task_id, project_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?
            .ok_or_else(|| {
                ConnectorTaskStoreError::InvalidState(
                    "task result is not pending human review".to_string(),
                )
            })?;
        let updated = tx.execute(
            "UPDATE wc_task_results
             SET decision_status = ?1, decided_by = ?2, decided_at = ?3,
                 cleanup_warning = COALESCE(?4, cleanup_warning)
             WHERE task_id = ?5 AND decision_status = 'pending'",
            params![decision, actor, now, cleanup_warning, task_id],
        )?;
        if updated != 1 {
            return Err(ConnectorTaskStoreError::InvalidState(
                "task result was already decided".to_string(),
            ));
        }
        insert_event(
            &tx,
            task_id,
            &run_id,
            cursor + 1,
            if decision == "accepted" {
                "task_accepted"
            } else {
                "task_rejected"
            },
            &serde_json::json!({
                "decision": decision,
                "actor": actor,
                "cleanup_warning": cleanup_warning
            }),
            now,
        )?;
        tx.execute(
            "UPDATE wc_tasks SET updated_at = ?1 WHERE id = ?2",
            params![now, task_id],
        )?;
        tx.commit()?;
        load_result(&conn, task_id)?
            .ok_or_else(|| ConnectorTaskStoreError::Storage(anyhow::anyhow!("result disappeared")))
    }

    pub(crate) fn decide_connector_approval(
        &self,
        task_id: &str,
        project_id: &str,
        approval_id: &str,
        approve: bool,
        actor: &str,
        now: i64,
    ) -> Result<ConnectorApproval, ConnectorTaskStoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let subject_id = tx
            .query_row(
                "SELECT owner_subject_id FROM wc_tasks WHERE id = ?1 AND project_id = ?2",
                params![task_id, project_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or(ConnectorTaskStoreError::NotFound)?;
        let task = load_task(&tx, task_id, project_id, &subject_id)?
            .ok_or(ConnectorTaskStoreError::NotFound)?;
        require_running(&task)?;
        let approval =
            load_approval(&tx, approval_id, task_id)?.ok_or(ConnectorTaskStoreError::NotFound)?;
        if approval.state != "pending" {
            return Err(ConnectorTaskStoreError::InvalidState(format!(
                "approval is {}; only pending approvals can be decided",
                approval.state
            )));
        }
        if approval.expires_at <= now {
            tx.execute(
                "UPDATE wc_approvals SET state = 'expired' WHERE id = ?1",
                params![approval_id],
            )?;
            tx.commit()?;
            return Err(ConnectorTaskStoreError::InvalidState(
                "approval expired; submit the exact action again to request a new decision"
                    .to_string(),
            ));
        }
        let state = if approve { "approved" } else { "denied" };
        tx.execute(
            "UPDATE wc_approvals SET state = ?1, decided_by = ?2, decided_at = ?3
             WHERE id = ?4 AND state = 'pending'",
            params![state, actor, now, approval_id],
        )?;
        let cursor: i64 = tx.query_row(
            "SELECT COALESCE(MAX(sequence), 0) FROM wc_task_events WHERE task_id = ?1",
            params![task_id],
            |row| row.get(0),
        )?;
        insert_event(
            &tx,
            task_id,
            &approval.run_id,
            cursor + 1,
            if approve {
                "approval_granted"
            } else {
                "approval_denied"
            },
            &serde_json::json!({
                "approval_id": approval_id,
                "action_hash": approval.action_hash,
                "actor": actor
            }),
            now,
        )?;
        tx.execute(
            "UPDATE wc_tasks SET updated_at = ?1 WHERE id = ?2",
            params![now, task_id],
        )?;
        tx.commit()?;
        load_approval(&conn, approval_id, task_id)?.ok_or(ConnectorTaskStoreError::NotFound)
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
                CASE
                    WHEN result.decision_status = 'accepted' THEN 'accepted'
                    WHEN result.decision_status = 'rejected' THEN 'rejected'
                    WHEN r.status = 'interrupted' THEN 'needs_attention'
                    ELSE t.status
                END,
                r.status, COALESCE(MAX(e.sequence), 0),
                ctx.target_executor_ref, ctx.execution_executor_ref,
                ctx.target_root, ctx.execution_root,
                ctx.baseline_commit, ctx.baseline_tree, ctx.isolated
         FROM wc_tasks t
         JOIN wc_runs r ON r.task_id = t.id
         JOIN wc_run_contexts ctx ON ctx.run_id = r.id
         LEFT JOIN wc_task_results result ON result.run_id = r.id
         LEFT JOIN wc_task_events e ON e.task_id = t.id
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
                target_executor_ref: row.get(10)?,
                execution_executor_ref: row.get(11)?,
                target_root: row.get(12)?,
                execution_root: row.get(13)?,
                baseline_commit: row.get(14)?,
                baseline_tree: row.get(15)?,
                isolated: row.get::<_, i64>(16)? != 0,
            })
        },
    )
    .optional()
}

fn load_result(
    conn: &rusqlite::Connection,
    task_id: &str,
) -> Result<Option<ConnectorTaskResult>, ConnectorTaskStoreError> {
    let row = conn
        .query_row(
            "SELECT id, task_id, run_id, summary, patch_artifact, patch_sha256, patch_bytes,
                    changed_paths_json, validation_json, warnings_json, decision_status,
                    decided_by, decided_at, cleanup_warning, created_at
             FROM wc_task_results WHERE task_id = ?1",
            params![task_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, Option<String>>(11)?,
                    row.get::<_, Option<i64>>(12)?,
                    row.get::<_, Option<String>>(13)?,
                    row.get::<_, i64>(14)?,
                ))
            },
        )
        .optional()?;
    let Some((
        result_id,
        task_id,
        run_id,
        summary,
        patch_artifact,
        patch_sha256,
        patch_bytes,
        changed_paths_json,
        validation_json,
        warnings_json,
        decision_status,
        decided_by,
        decided_at,
        cleanup_warning,
        created_at,
    )) = row
    else {
        return Ok(None);
    };
    Ok(Some(ConnectorTaskResult {
        result_id,
        task_id,
        run_id,
        summary,
        patch_artifact,
        patch_sha256,
        patch_bytes: usize::try_from(patch_bytes).map_err(|_| {
            ConnectorTaskStoreError::Storage(anyhow::anyhow!(
                "task result contains an invalid negative patch size"
            ))
        })?,
        changed_paths: serde_json::from_str(&changed_paths_json)?,
        validation: serde_json::from_str(&validation_json)?,
        warnings: serde_json::from_str(&warnings_json)?,
        decision_status,
        decided_by,
        decided_at,
        cleanup_warning,
        created_at,
    }))
}

fn load_approval_by_hash(
    conn: &rusqlite::Connection,
    task_id: &str,
    run_id: &str,
    action_hash: &str,
) -> Result<Option<ConnectorApproval>, rusqlite::Error> {
    conn.query_row(
        "SELECT id, task_id, run_id, action_kind, action_hash, action_summary, state,
                requested_at, expires_at, decided_by, decided_at, consumed_at
         FROM wc_approvals WHERE task_id = ?1 AND run_id = ?2 AND action_hash = ?3",
        params![task_id, run_id, action_hash],
        map_approval,
    )
    .optional()
}

fn load_approval(
    conn: &rusqlite::Connection,
    approval_id: &str,
    task_id: &str,
) -> Result<Option<ConnectorApproval>, rusqlite::Error> {
    conn.query_row(
        "SELECT id, task_id, run_id, action_kind, action_hash, action_summary, state,
                requested_at, expires_at, decided_by, decided_at, consumed_at
         FROM wc_approvals WHERE id = ?1 AND task_id = ?2",
        params![approval_id, task_id],
        map_approval,
    )
    .optional()
}

fn map_approval(row: &rusqlite::Row<'_>) -> Result<ConnectorApproval, rusqlite::Error> {
    Ok(ConnectorApproval {
        approval_id: row.get(0)?,
        task_id: row.get(1)?,
        run_id: row.get(2)?,
        action_kind: row.get(3)?,
        action_hash: row.get(4)?,
        action_summary: row.get(5)?,
        state: row.get(6)?,
        requested_at: row.get(7)?,
        expires_at: row.get(8)?,
        decided_by: row.get(9)?,
        decided_at: row.get(10)?,
        consumed_at: row.get(11)?,
    })
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

    fn start(db: &Database, subject: &str, goal: &str) -> ConnectorTaskSnapshot {
        let task_id = new_id("wc_task");
        let run_id = new_id("wc_run");
        db.start_connector_task(NewConnectorTask {
            task_id: &task_id,
            run_id: &run_id,
            project_id: "wc_proj_demo",
            workspace_id: "wc_ws_demo",
            subject_id: subject,
            goal,
            mode: "normal",
            target_executor_ref: "agent:hosted:demo",
            execution_executor_ref: "agent:hosted:run",
            target_root: "/workspace/demo",
            execution_root: "/workspace/runs/one",
            baseline_commit: Some("0123456789abcdef"),
            baseline_tree: Some("fedcba9876543210"),
            isolated: true,
            now: 101,
        })
        .unwrap()
    }

    #[test]
    fn start_creates_task_run_and_first_monotonic_event() {
        let (_temp, db) = database();
        bind(&db, "user:one");
        let task = start(&db, "user:one", "fix the parser");
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
    fn edit_operation_is_durable_idempotency_authority() {
        let (_temp, db) = database();
        bind(&db, "user:one");
        let task = start(&db, "user:one", "edit atomically");
        let begin = |operation_id: &str, request_sha256: &str| {
            db.begin_connector_edit_operation(
                &task.task_id,
                "wc_proj_demo",
                "user:one",
                operation_id,
                request_sha256,
                102,
            )
            .unwrap()
        };

        assert_eq!(
            begin("edit-1", &"a".repeat(64)),
            ConnectorEditOperationGate::Started
        );
        assert_eq!(
            begin("edit-1", &"a".repeat(64)),
            ConnectorEditOperationGate::Pending
        );
        let result = serde_json::json!({"changed": true, "changed_paths": ["src/lib.rs"]});
        db.complete_connector_edit_operation(
            &task.task_id,
            "wc_proj_demo",
            "user:one",
            "edit-1",
            &"a".repeat(64),
            &result,
            103,
        )
        .unwrap();
        assert_eq!(
            begin("edit-1", &"a".repeat(64)),
            ConnectorEditOperationGate::Replay(result)
        );
        assert_eq!(
            begin("edit-1", &"b".repeat(64)),
            ConnectorEditOperationGate::Conflict
        );

        assert_eq!(
            begin("edit-2", &"c".repeat(64)),
            ConnectorEditOperationGate::Started
        );
        db.fail_connector_edit_operation(&task.task_id, "edit-2", &"c".repeat(64), 103)
            .unwrap();
        assert_eq!(
            begin("edit-2", &"d".repeat(64)),
            ConnectorEditOperationGate::Conflict
        );
        assert_eq!(
            begin("edit-2", &"c".repeat(64)),
            ConnectorEditOperationGate::Started
        );
    }

    #[test]
    fn task_access_is_subject_and_project_scoped() {
        let (_temp, db) = database();
        bind(&db, "user:one");
        bind(&db, "user:two");
        let task = start(&db, "user:one", "private task");
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
        let task = start(&db, "user:one", "finish me");
        let changed_paths = vec!["src/lib.rs".to_string()];
        let warnings = Vec::new();
        let cursor = db
            .finish_connector_task(
                &task.task_id,
                "wc_proj_demo",
                "user:one",
                NewConnectorResult {
                    result_id: "wc_result_0123456789abcdef",
                    summary: "done",
                    patch_artifact: Some("/state/results/task.patch"),
                    patch_sha256: Some("abc123"),
                    patch_bytes: 42,
                    changed_paths: &changed_paths,
                    validation: &serde_json::json!({"checks": []}),
                    warnings: &warnings,
                },
                102,
            )
            .unwrap();
        assert_eq!(cursor, 2);
        let snapshot = db
            .connector_task(&task.task_id, "wc_proj_demo", "user:one")
            .unwrap();
        assert_eq!(snapshot.task_status, "ready_for_review");
        assert_eq!(snapshot.run_status, "completed");
        let result = db
            .connector_task_result(&task.task_id, "wc_proj_demo", "user:one")
            .unwrap()
            .unwrap();
        assert_eq!(result.changed_paths, changed_paths);
        assert_eq!(result.decision_status, "pending");
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

    #[test]
    fn raw_command_approval_is_exact_and_consumed_once() {
        let (_temp, db) = database();
        bind(&db, "user:one");
        let task = start(&db, "user:one", "run a generator");
        let pending = db
            .request_or_consume_connector_approval(
                &task.task_id,
                "wc_proj_demo",
                "user:one",
                "commands_run",
                "exact-action-hash",
                "raw project command (20 bytes)",
                102,
                200,
            )
            .unwrap();
        let ConnectorApprovalGate::Pending(approval) = pending else {
            panic!("first exact action must wait for local approval");
        };
        let approved = db
            .decide_connector_approval(
                &task.task_id,
                "wc_proj_demo",
                &approval.approval_id,
                true,
                "local_cli",
                103,
            )
            .unwrap();
        assert_eq!(approved.state, "approved");

        let authorized = db
            .request_or_consume_connector_approval(
                &task.task_id,
                "wc_proj_demo",
                "user:one",
                "commands_run",
                "exact-action-hash",
                "raw project command (20 bytes)",
                104,
                200,
            )
            .unwrap();
        assert!(matches!(authorized, ConnectorApprovalGate::Authorized(_)));
        let replay = db
            .request_or_consume_connector_approval(
                &task.task_id,
                "wc_proj_demo",
                "user:one",
                "commands_run",
                "exact-action-hash",
                "raw project command (20 bytes)",
                105,
                200,
            )
            .unwrap();
        assert!(matches!(replay, ConnectorApprovalGate::Consumed(_)));
        let events = db
            .connector_task_events(&task.task_id, "wc_proj_demo", "user:one", 20)
            .unwrap();
        assert_eq!(
            events
                .iter()
                .map(|event| event.kind.as_str())
                .collect::<Vec<_>>(),
            vec![
                "task_started",
                "approval_requested",
                "approval_granted",
                "approval_consumed"
            ]
        );
    }

    #[test]
    fn finishing_task_expires_unconsumed_command_approval() {
        let (_temp, db) = database();
        bind(&db, "user:one");
        let task = start(&db, "user:one", "finish safely");
        let pending = db
            .request_or_consume_connector_approval(
                &task.task_id,
                "wc_proj_demo",
                "user:one",
                "commands_run",
                "unconsumed-action",
                "raw project command",
                102,
                200,
            )
            .unwrap();
        let ConnectorApprovalGate::Pending(approval) = pending else {
            panic!("approval must initially be pending");
        };
        db.finish_connector_task(
            &task.task_id,
            "wc_proj_demo",
            "user:one",
            NewConnectorResult {
                result_id: "wc_result_2123456789abcdef",
                summary: "finished without the command",
                patch_artifact: None,
                patch_sha256: None,
                patch_bytes: 0,
                changed_paths: &[],
                validation: &serde_json::json!({"status": "not_run"}),
                warnings: &[],
            },
            103,
        )
        .unwrap();
        let stored = db
            .local_connector_task_approvals(&task.task_id, "wc_proj_demo")
            .unwrap();
        assert_eq!(stored[0].state, "expired");
        assert!(matches!(
            db.decide_connector_approval(
                &task.task_id,
                "wc_proj_demo",
                &approval.approval_id,
                true,
                "local_cli",
                104
            ),
            Err(ConnectorTaskStoreError::InvalidState(_))
        ));
    }

    #[test]
    fn restart_marks_unfinished_runs_for_attention() {
        let (_temp, db) = database();
        bind(&db, "user:one");
        let task = start(&db, "user:one", "survive a restart");
        assert_eq!(db.interrupt_connector_runs("wc_proj_demo", 102).unwrap(), 1);
        let recovered = db
            .connector_task(&task.task_id, "wc_proj_demo", "user:one")
            .unwrap();
        assert_eq!(recovered.task_status, "needs_attention");
        assert_eq!(recovered.run_status, "interrupted");
        let preserved = db.connector_preserved_workspaces("wc_proj_demo").unwrap();
        assert_eq!(preserved.len(), 1);
        assert_eq!(preserved[0].task_id, task.task_id);
        assert_eq!(preserved[0].run_id, task.run_id);
        let events = db
            .connector_task_events(&task.task_id, "wc_proj_demo", "user:one", 20)
            .unwrap();
        assert_eq!(events.last().unwrap().kind, "run_interrupted");
        let resumed = db
            .resume_connector_task(&task.task_id, "wc_proj_demo", "local_cli", 103)
            .unwrap();
        assert_eq!(resumed.task_status, "active");
        assert_eq!(resumed.run_status, "running");
        let events = db
            .connector_task_events(&task.task_id, "wc_proj_demo", "user:one", 20)
            .unwrap();
        assert_eq!(events.last().unwrap().kind, "run_resumed");
    }

    #[test]
    fn interrupted_task_can_be_abandoned_without_capturing_workspace_changes() {
        let (_temp, db) = database();
        bind(&db, "user:one");
        let task = start(&db, "user:one", "abandon after restart");
        db.interrupt_connector_runs("wc_proj_demo", 102).unwrap();

        let result = db
            .abandon_interrupted_connector_task(&task.task_id, "wc_proj_demo", "local_cli", 103)
            .unwrap();
        assert_eq!(result.decision_status, "rejected");
        assert_eq!(result.patch_bytes, 0);
        assert_eq!(result.validation["status"], "not_run");
        assert!(db
            .connector_preserved_workspaces("wc_proj_demo")
            .unwrap()
            .is_empty());
        let decided = db
            .connector_task(&task.task_id, "wc_proj_demo", "user:one")
            .unwrap();
        assert_eq!(decided.task_status, "rejected");
        let cursor = db
            .record_connector_workspace_release(
                &task.task_id,
                "wc_proj_demo",
                "user:one",
                true,
                None,
                104,
            )
            .unwrap();
        assert_eq!(cursor, 4);
        let events = db
            .connector_task_events(&task.task_id, "wc_proj_demo", "user:one", 20)
            .unwrap();
        assert_eq!(events[2].kind, "task_abandoned");
        assert_eq!(events[3].kind, "workspace_release");
    }

    #[test]
    fn local_result_decision_becomes_canonical_task_status() {
        let (_temp, db) = database();
        bind(&db, "user:one");
        let task = start(&db, "user:one", "finish and accept");
        let changed_paths = vec!["src/lib.rs".to_string()];
        db.finish_connector_task(
            &task.task_id,
            "wc_proj_demo",
            "user:one",
            NewConnectorResult {
                result_id: "wc_result_1123456789abcdef",
                summary: "done",
                patch_artifact: None,
                patch_sha256: None,
                patch_bytes: 0,
                changed_paths: &changed_paths,
                validation: &serde_json::json!({"status": "recorded"}),
                warnings: &[],
            },
            102,
        )
        .unwrap();
        let release_cursor = db
            .record_connector_workspace_release(
                &task.task_id,
                "wc_proj_demo",
                "user:one",
                false,
                Some("slot cleanup needs retry"),
                103,
            )
            .unwrap();
        assert_eq!(release_cursor, 3);
        let result = db
            .decide_connector_result(
                &task.task_id,
                "wc_proj_demo",
                "accepted",
                "local_cli",
                None,
                104,
            )
            .unwrap();
        assert_eq!(result.decision_status, "accepted");
        assert_eq!(
            result.cleanup_warning.as_deref(),
            Some("slot cleanup needs retry")
        );
        let decided = db
            .connector_task(&task.task_id, "wc_proj_demo", "user:one")
            .unwrap();
        assert_eq!(decided.task_status, "accepted");
    }
}
