//! Durable execution lifecycle facts; the existing job manager remains the
//! process and output authority.

use super::execution_model::{
    execution_event_kind, latest_execution, latest_execution_by_kind, load_execution,
    load_execution_by_operation, observed_state, ConnectorExecution, ConnectorExecutionFailure,
    ConnectorExecutionObservation, ConnectorExecutionReservation,
};
use super::task_kernel::{
    expire_task_approvals, insert_event, load_task, require_running, touch_task,
};
use super::{ConnectorTaskSnapshot, ConnectorTaskStoreError, Database};
use rusqlite::{params, Transaction};
use serde_json::json;

impl Database {
    pub(crate) fn reserve_connector_execution(
        &self,
        task: &ConnectorTaskSnapshot,
        kind: &str,
        operation_id: &str,
        request_sha256: &str,
        check_plan: &[String],
        check_recipe: Option<&serde_json::Value>,
        check_workspace_sha256: Option<&str>,
        queue_deadline: i64,
        now: i64,
    ) -> Result<ConnectorExecutionReservation, ConnectorTaskStoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let current = load_task(&tx, &task.task_id, &task.project_id, &task.owner_subject_id)?
            .ok_or(ConnectorTaskStoreError::NotFound)?;
        require_running(&current)?;
        if let Some(existing) =
            load_execution_by_operation(&tx, &task.task_id, &task.run_id, operation_id)?
        {
            tx.commit()?;
            if existing.request_sha256 != request_sha256 {
                return Err(ConnectorTaskStoreError::OperationIdConflict(
                    operation_id.to_string(),
                ));
            }
            return Ok(ConnectorExecutionReservation::Existing(existing));
        }
        if let Some(active) =
            latest_execution(&tx, &task.task_id)?.filter(ConnectorExecution::blocks_finish)
        {
            return Err(ConnectorTaskStoreError::InvalidState(format!(
                "execution {} is {}; review or cancel it before starting another execution",
                active.execution_id, active.state
            )));
        }
        if !matches!(kind, "command" | "check")
            || (kind == "command" && !check_plan.is_empty())
            || (kind == "check" && check_plan.is_empty())
            || (kind == "command" && check_workspace_sha256.is_some())
            || (kind == "check" && check_workspace_sha256.is_none())
            || (kind == "command" && check_recipe.is_some())
            || (kind == "check" && check_recipe.is_none())
        {
            return Err(ConnectorTaskStoreError::InvalidState(
                "execution kind and check plan do not match".to_string(),
            ));
        }
        let execution_id = format!("wc_exec_{}", uuid::Uuid::new_v4().simple());
        let check_plan = (kind == "check").then(|| check_plan.join(","));
        let check_recipe_json = check_recipe
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| ConnectorTaskStoreError::InvalidState(error.to_string()))?;
        tx.execute(
            "INSERT INTO wc_executions
                (id, kind, task_id, run_id, state, submitted_at, queue_deadline,
                 stdout_cursor, stderr_cursor, operation_id, request_sha256, check_plan,
                 check_recipe_json, check_workspace_sha256)
             VALUES (?1, ?2, ?3, ?4, 'accepted', ?5, ?6, 1, 1, ?7, ?8, ?9, ?10, ?11)",
            params![
                execution_id,
                kind,
                task.task_id,
                task.run_id,
                now,
                queue_deadline,
                operation_id,
                request_sha256,
                check_plan,
                check_recipe_json,
                check_workspace_sha256
            ],
        )?;
        let execution =
            load_execution(&tx, &execution_id)?.ok_or(ConnectorTaskStoreError::NotFound)?;
        append_execution_event(&tx, &execution, "accepted", now)?;
        touch_task(&tx, &task.task_id, now)?;
        tx.commit()?;
        Ok(ConnectorExecutionReservation::Created(execution))
    }

    pub(crate) fn start_connector_execution(
        &self,
        execution_id: &str,
        now: i64,
    ) -> Result<ConnectorExecution, ConnectorTaskStoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let execution =
            load_execution(&tx, execution_id)?.ok_or(ConnectorTaskStoreError::NotFound)?;
        if execution.state != "accepted" {
            tx.commit()?;
            return Ok(execution);
        }
        tx.execute(
            "UPDATE wc_executions SET state = 'starting' WHERE id = ?1",
            params![execution_id],
        )?;
        append_execution_event(&tx, &execution, "starting", now)?;
        touch_task(&tx, &execution.task_id, now)?;
        commit_execution(tx, execution_id)
    }

    pub(crate) fn connector_execution(
        &self,
        execution_id: &str,
    ) -> Result<ConnectorExecution, ConnectorTaskStoreError> {
        let conn = self.conn.lock().unwrap();
        load_execution(&conn, execution_id)?.ok_or(ConnectorTaskStoreError::NotFound)
    }

    pub(crate) fn latest_connector_execution(
        &self,
        task_id: &str,
        project_id: &str,
        subject_id: &str,
        operation_id: Option<&str>,
    ) -> Result<Option<ConnectorExecution>, ConnectorTaskStoreError> {
        let conn = self.conn.lock().unwrap();
        let task = load_task(&conn, task_id, project_id, subject_id)?
            .ok_or(ConnectorTaskStoreError::NotFound)?;
        match operation_id {
            Some(operation_id) => {
                load_execution_by_operation(&conn, task_id, &task.run_id, operation_id)
            }
            None => latest_execution(&conn, task_id),
        }
        .map_err(ConnectorTaskStoreError::from)
    }

    pub(crate) fn latest_connector_execution_by_kind(
        &self,
        task_id: &str,
        project_id: &str,
        subject_id: &str,
        kind: &str,
    ) -> Result<Option<ConnectorExecution>, ConnectorTaskStoreError> {
        let conn = self.conn.lock().unwrap();
        load_task(&conn, task_id, project_id, subject_id)?
            .ok_or(ConnectorTaskStoreError::NotFound)?;
        latest_execution_by_kind(&conn, task_id, kind).map_err(ConnectorTaskStoreError::from)
    }

    pub(crate) fn attach_connector_executor(
        &self,
        execution_id: &str,
        executor_reference: &str,
        executor_status: &str,
        now: i64,
    ) -> Result<ConnectorExecution, ConnectorTaskStoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let execution =
            load_execution(&tx, execution_id)?.ok_or(ConnectorTaskStoreError::NotFound)?;
        if execution
            .executor_reference
            .as_deref()
            .is_some_and(|existing| existing != executor_reference)
        {
            return Err(ConnectorTaskStoreError::InvalidState(format!(
                "execution {} is already attached to a different executor reference",
                execution.execution_id
            )));
        }
        if execution.is_terminal() {
            tx.execute(
                "UPDATE wc_executions
                 SET executor_reference = COALESCE(executor_reference, ?1)
                 WHERE id = ?2",
                params![executor_reference, execution_id],
            )?;
            touch_task(&tx, &execution.task_id, now)?;
            return commit_execution(tx, execution_id);
        }
        let recognized = ConnectorExecution::executor_status_recognized(executor_status);
        let state = if execution.state == "cancel_requested" {
            "cancel_requested"
        } else if matches!(executor_status, "queued" | "agent_queued") {
            "queued"
        } else if matches!(executor_status, "running" | "started") {
            "running"
        } else {
            execution.state.as_str()
        };
        tx.execute(
            "UPDATE wc_executions SET executor_reference = ?1, state = ?2,
                    queued_at = CASE WHEN ?2 = 'queued' THEN COALESCE(queued_at, ?3) ELSE queued_at END,
                    started_at = CASE WHEN ?2 = 'running' THEN COALESCE(started_at, ?3) ELSE started_at END,
                    first_status_failure_at = CASE WHEN ?4 THEN first_status_failure_at
                        ELSE COALESCE(first_status_failure_at, ?3) END,
                    status_failure_code = CASE WHEN ?4 THEN status_failure_code
                        ELSE 'executor_status_unrecognized' END
             WHERE id = ?5",
            params![executor_reference, state, now, recognized, execution_id],
        )?;
        if execution.state != state {
            append_execution_event(&tx, &execution, state, now)?;
        }
        touch_task(&tx, &execution.task_id, now)?;
        commit_execution(tx, execution_id)
    }

    pub(crate) fn record_connector_execution_status_failure(
        &self,
        execution_id: &str,
        failure_code: &str,
        now: i64,
    ) -> Result<ConnectorExecution, ConnectorTaskStoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let execution =
            load_execution(&tx, execution_id)?.ok_or(ConnectorTaskStoreError::NotFound)?;
        if execution.is_terminal() {
            tx.commit()?;
            return Ok(execution);
        }
        tx.execute(
            "UPDATE wc_executions
             SET first_status_failure_at = COALESCE(first_status_failure_at, ?1),
                 status_failure_code = ?2
             WHERE id = ?3",
            params![now, failure_code, execution_id],
        )?;
        touch_task(&tx, &execution.task_id, now)?;
        commit_execution(tx, execution_id)
    }

    pub(crate) fn observe_connector_execution(
        &self,
        execution_id: &str,
        observation: ConnectorExecutionObservation<'_>,
    ) -> Result<ConnectorExecution, ConnectorTaskStoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let execution =
            load_execution(&tx, execution_id)?.ok_or(ConnectorTaskStoreError::NotFound)?;
        if execution.is_terminal() {
            tx.commit()?;
            return Ok(execution);
        }
        if !ConnectorExecution::executor_status_recognized(observation.executor_status) {
            tx.execute(
                "UPDATE wc_executions
                 SET first_status_failure_at = COALESCE(first_status_failure_at, ?1),
                     status_failure_code = 'executor_status_unrecognized'
                 WHERE id = ?2",
                params![observation.now, execution_id],
            )?;
            touch_task(&tx, &execution.task_id, observation.now)?;
            return commit_execution(tx, execution_id);
        }
        let stdout_cursor = execution.stdout_cursor.max(observation.stdout_cursor);
        let stderr_cursor = execution.stderr_cursor.max(observation.stderr_cursor);
        let output_advanced =
            stdout_cursor > execution.stdout_cursor || stderr_cursor > execution.stderr_cursor;
        let (state, source, code, reason) = observed_state(&execution, &observation);
        if observation
            .check_completed
            .is_some_and(|completed| completed > execution.check_plan.len())
        {
            return Err(ConnectorTaskStoreError::InvalidState(
                "validation progress exceeds its durable check plan".to_string(),
            ));
        }
        if observation
            .check_completed
            .is_some_and(|completed| completed < execution.check_completed)
        {
            return Err(ConnectorTaskStoreError::InvalidState(
                "validation progress cannot move backwards".to_string(),
            ));
        }
        if execution.kind == "command"
            && (observation.check_completed.is_some()
                || observation.failed_check.is_some()
                || observation.validated_workspace_sha256.is_some())
        {
            return Err(ConnectorTaskStoreError::InvalidState(
                "ordinary execution cannot record validation progress".to_string(),
            ));
        }
        if observation.failed_check.is_some_and(|failed| {
            execution
                .check_plan
                .get(
                    observation
                        .check_completed
                        .unwrap_or(execution.check_completed),
                )
                .map(String::as_str)
                != Some(failed)
        }) {
            return Err(ConnectorTaskStoreError::InvalidState(
                "failed check does not match durable validation progress".to_string(),
            ));
        }
        let evidence_json = observation
            .assertion_evidence
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| ConnectorTaskStoreError::InvalidState(error.to_string()))?;
        if evidence_json
            .as_ref()
            .is_some_and(|evidence| evidence.len() > super::MAX_ASSERTION_EVIDENCE_BYTES)
        {
            return Err(ConnectorTaskStoreError::InvalidState(
                "assertion evidence exceeds its durable size limit".to_string(),
            ));
        }
        if observation.assertion_evidence.is_some()
            && (observation.failed_check.is_none() || state != "failed" || source != Some("check"))
        {
            return Err(ConnectorTaskStoreError::InvalidState(
                "assertion evidence requires trusted failed-check progress".to_string(),
            ));
        }
        if state == "succeeded"
            && execution.kind == "check"
            && (observation.check_completed != Some(execution.check_plan.len())
                || observation.failed_check.is_some()
                || observation.validated_workspace_sha256.is_none()
                || execution.check_workspace_sha256.as_deref()
                    != observation.validated_workspace_sha256)
        {
            return Err(ConnectorTaskStoreError::InvalidState(
                "successful check requires complete progress and matching workspace provenance"
                    .to_string(),
            ));
        }
        if observation.validated_workspace_sha256.is_some()
            && !(state == "succeeded" && execution.kind == "check")
        {
            return Err(ConnectorTaskStoreError::InvalidState(
                "validated workspace provenance requires a successful check".to_string(),
            ));
        }
        let validated_workspace = observation.validated_workspace_sha256;
        let state_changed = state != execution.state;
        let terminal = !ConnectorExecution::state_is_active(state);
        tx.execute(
            "UPDATE wc_executions SET state = ?1, stdout_cursor = ?2, stderr_cursor = ?3,
                    last_output_at = CASE WHEN ?4 THEN ?5 ELSE last_output_at END,
                    started_at = COALESCE(started_at, ?6),
                    finished_at = CASE WHEN ?7 THEN COALESCE(?8, ?5) ELSE NULL END,
                    exit_code = COALESCE(?9, exit_code),
                    failure_source = COALESCE(?10, failure_source),
                    failure_code = COALESCE(?11, failure_code),
                    terminal_reason = COALESCE(?12, terminal_reason),
                    first_status_failure_at = NULL,
                    last_successful_observation_at = ?5,
                    status_failure_code = NULL,
                    check_completed = CASE
                        WHEN ?14 IS NOT NULL THEN ?14
                        ELSE check_completed
                    END,
                    failed_check = CASE WHEN ?1 = 'failed' AND kind = 'check' AND ?10 = 'check'
                        THEN ?15 ELSE failed_check END,
                    assertion_evidence_json = CASE WHEN ?1 = 'failed' AND kind = 'check'
                        AND ?10 = 'check'
                        THEN ?16 ELSE assertion_evidence_json END,
                    validated_workspace_sha256 = CASE WHEN ?1 = 'succeeded' AND kind = 'check'
                        THEN ?17 ELSE NULL END
                    WHERE id = ?13",
            params![
                state,
                stdout_cursor as i64,
                stderr_cursor as i64,
                output_advanced,
                observation.now,
                observation.started_at,
                terminal,
                observation.finished_at,
                observation.exit_code,
                source,
                code,
                reason,
                execution_id,
                observation.check_completed.map(|count| count as i64),
                observation.failed_check,
                evidence_json,
                validated_workspace
            ],
        )?;
        if state_changed {
            append_execution_event(&tx, &execution, state, observation.now)?;
        }
        if state == "unknown" {
            interrupt_run(
                &tx,
                &execution.run_id,
                &execution.task_id,
                "execution_terminal_unknown",
                observation.now,
            )?;
        } else if state == "cancelled" {
            finalize_task_cancel(
                &tx,
                &execution.task_id,
                &execution.run_id,
                &json!({ "execution_id": execution.execution_id }),
                observation.now,
            )?;
        }
        touch_task(&tx, &execution.task_id, observation.now)?;
        commit_execution(tx, execution_id)
    }

    pub(crate) fn request_connector_execution_cancel(
        &self,
        task: &ConnectorTaskSnapshot,
        reason: Option<&str>,
        now: i64,
    ) -> Result<Option<ConnectorExecution>, ConnectorTaskStoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let current = load_task(&tx, &task.task_id, &task.project_id, &task.owner_subject_id)?
            .ok_or(ConnectorTaskStoreError::NotFound)?;
        if current.task_status == "cancelled" {
            let execution = latest_execution(&tx, &task.task_id)?;
            tx.commit()?;
            return Ok(execution);
        }
        require_running(&current)?;
        if let Some(execution) = latest_execution(&tx, &task.task_id)? {
            if execution.is_terminal() {
                finalize_task_cancel(
                    &tx,
                    &current.task_id,
                    &current.run_id,
                    &json!({ "execution_id": null, "reason": reason }),
                    now,
                )?;
                tx.commit()?;
                return Ok(Some(execution));
            }
            let immediate = execution.state == "accepted" && execution.executor_reference.is_none();
            let state = immediate
                .then_some("cancelled")
                .unwrap_or("cancel_requested");
            tx.execute(
                "UPDATE wc_executions SET state = ?1,
                        cancel_requested_at = COALESCE(cancel_requested_at, ?2),
                        terminal_reason = 'user_cancelled',
                        finished_at = CASE WHEN ?3 THEN ?2 ELSE finished_at END
                 WHERE id = ?4",
                params![state, now, immediate, execution.execution_id],
            )?;
            if execution.state != "cancel_requested" {
                append_execution_event(&tx, &execution, state, now)?;
            }
            if immediate {
                finalize_task_cancel(
                    &tx,
                    &execution.task_id,
                    &execution.run_id,
                    &json!({ "execution_id": execution.execution_id }),
                    now,
                )?;
            } else if let (false, Some(reason)) = (execution.state == "cancel_requested", reason) {
                insert_event(
                    &tx,
                    &task.task_id,
                    &task.run_id,
                    next_event_sequence(&tx, &task.task_id)?,
                    "task_cancel_requested",
                    &json!({ "reason": reason }),
                    now,
                )?;
            }
            touch_task(&tx, &task.task_id, now)?;
            return commit_execution(tx, &execution.execution_id).map(Some);
        }
        finalize_task_cancel(
            &tx,
            &current.task_id,
            &current.run_id,
            &json!({ "execution_id": null, "reason": reason }),
            now,
        )?;
        tx.commit()?;
        Ok(None)
    }

    pub(crate) fn request_connector_queue_timeout(
        &self,
        execution_id: &str,
        now: i64,
    ) -> Result<ConnectorExecution, ConnectorTaskStoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let execution =
            load_execution(&tx, execution_id)?.ok_or(ConnectorTaskStoreError::NotFound)?;
        if execution.state != "queued" {
            tx.commit()?;
            return Ok(execution);
        }
        tx.execute(
            "UPDATE wc_executions SET state = 'cancel_requested',
                    cancel_requested_at = ?1, failure_source = 'queue',
                    failure_code = 'queue_deadline', terminal_reason = 'queue_timeout_requested'
             WHERE id = ?2",
            params![now, execution_id],
        )?;
        append_execution_event(&tx, &execution, "cancel_requested", now)?;
        touch_task(&tx, &execution.task_id, now)?;
        commit_execution(tx, execution_id)
    }

    pub(crate) fn connector_finish_blocker(
        &self,
        task_id: &str,
    ) -> Result<Option<ConnectorExecution>, ConnectorTaskStoreError> {
        let conn = self.conn.lock().unwrap();
        Ok(latest_execution(&conn, task_id)?.filter(ConnectorExecution::blocks_finish))
    }

    pub(crate) fn reconcile_connector_executions(
        &self,
        project_id: &str,
        now: i64,
    ) -> Result<(usize, usize), ConnectorTaskStoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let recoveries = {
            let mut statement = tx.prepare(
                "SELECT r.id, t.id,
                    (SELECT e.id FROM wc_executions e WHERE e.run_id = r.id
                     AND e.state IN ('accepted','queued','starting','running','cancel_requested')
                     ORDER BY e.submitted_at DESC LIMIT 1)
                 FROM wc_runs r JOIN wc_tasks t ON t.id = r.task_id
                 WHERE t.project_id = ?1 AND r.status = 'running'",
            )?;
            let rows = statement
                .query_map(params![project_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        let mut executions_interrupted = 0;
        for (run_id, task_id, execution_id) in &recoveries {
            if let Some(execution_id) = execution_id {
                let execution =
                    load_execution(&tx, execution_id)?.ok_or(ConnectorTaskStoreError::NotFound)?;
                tx.execute(
                    "UPDATE wc_executions SET state = 'interrupted', finished_at = ?1,
                     failure_source = 'runtime', failure_code = 'executor_handle_unverified',
                     terminal_reason = 'runtime_restarted' WHERE id = ?2",
                    params![now, execution_id],
                )?;
                append_execution_event(&tx, &execution, "interrupted", now)?;
                executions_interrupted += 1;
            }
            interrupt_run(&tx, run_id, task_id, "runtime_restarted", now)?;
        }
        tx.commit()?;
        Ok((recoveries.len(), executions_interrupted))
    }

    pub(crate) fn finish_connector_execution(
        &self,
        execution_id: &str,
        failure: ConnectorExecutionFailure,
        now: i64,
    ) -> Result<ConnectorExecution, ConnectorTaskStoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let execution =
            load_execution(&tx, execution_id)?.ok_or(ConnectorTaskStoreError::NotFound)?;
        if !execution.is_active() {
            tx.commit()?;
            return Ok(execution);
        }
        let (state, source, code, reason) = match failure {
            ConnectorExecutionFailure::Submission(_) if execution.state == "cancel_requested" => (
                "cancelled",
                "cancellation",
                "cancelled_before_submission",
                "user_cancelled",
            ),
            ConnectorExecutionFailure::Submission(code) => {
                ("failed", "submission", code, "submission_failed")
            }
            ConnectorExecutionFailure::Unknown(code) => {
                ("unknown", "transport", code, "executor_terminal_unknown")
            }
        };
        tx.execute(
            "UPDATE wc_executions SET state = ?1, finished_at = ?2, exit_code = ?3,
                        failure_source = ?4, failure_code = ?5, terminal_reason = ?6
             WHERE id = ?7",
            params![
                state,
                now,
                Option::<i32>::None,
                source,
                code,
                reason,
                execution.execution_id
            ],
        )?;
        append_execution_event(&tx, &execution, state, now)?;
        if state == "cancelled" {
            finalize_task_cancel(
                &tx,
                &execution.task_id,
                &execution.run_id,
                &json!({ "execution_id": execution.execution_id }),
                now,
            )?;
        } else if state == "unknown" {
            interrupt_run(
                &tx,
                &execution.run_id,
                &execution.task_id,
                "execution_terminal_unknown",
                now,
            )?;
        }
        touch_task(&tx, &execution.task_id, now)?;
        commit_execution(tx, execution_id)
    }
}

fn interrupt_run(
    tx: &Transaction<'_>,
    run_id: &str,
    task_id: &str,
    reason: &str,
    now: i64,
) -> Result<(), ConnectorTaskStoreError> {
    tx.execute(
        "UPDATE wc_runs SET status = 'interrupted', finished_at = ?1
         WHERE id = ?2 AND status = 'running'",
        params![now, run_id],
    )?;
    insert_event(
        tx,
        task_id,
        run_id,
        next_event_sequence(tx, task_id)?,
        "run_interrupted",
        &json!({ "reason": reason, "recoverable": true }),
        now,
    )?;
    expire_task_approvals(tx, task_id)?;
    touch_task(tx, task_id, now)?;
    Ok(())
}

fn finalize_task_cancel(
    tx: &Transaction<'_>,
    task_id: &str,
    run_id: &str,
    payload: &serde_json::Value,
    now: i64,
) -> Result<(), ConnectorTaskStoreError> {
    tx.execute(
        "UPDATE wc_runs SET status = 'interrupted', finished_at = ?1 WHERE id = ?2",
        params![now, run_id],
    )?;
    insert_event(
        tx,
        task_id,
        run_id,
        next_event_sequence(tx, task_id)?,
        "task_cancelled",
        payload,
        now,
    )?;
    expire_task_approvals(tx, task_id)?;
    touch_task(tx, task_id, now)?;
    Ok(())
}

fn append_execution_event(
    tx: &Transaction<'_>,
    execution: &ConnectorExecution,
    state: &str,
    now: i64,
) -> Result<(), ConnectorTaskStoreError> {
    insert_event(
        tx,
        &execution.task_id,
        &execution.run_id,
        next_event_sequence(tx, &execution.task_id)?,
        execution_event_kind(state),
        &json!({
            "execution_id": execution.execution_id,
            "kind": execution.kind,
            "state": state
        }),
        now,
    )
}

fn next_event_sequence(tx: &Transaction<'_>, task_id: &str) -> rusqlite::Result<i64> {
    tx.query_row(
        "SELECT COALESCE(MAX(sequence), 0) + 1 FROM wc_task_events WHERE task_id = ?1",
        params![task_id],
        |row| row.get(0),
    )
}

fn commit_execution(
    tx: Transaction<'_>,
    execution_id: &str,
) -> Result<ConnectorExecution, ConnectorTaskStoreError> {
    let execution = load_execution(&tx, execution_id)?.ok_or(ConnectorTaskStoreError::NotFound)?;
    tx.commit()?;
    Ok(execution)
}
