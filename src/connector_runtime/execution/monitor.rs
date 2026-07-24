use super::*;

struct MonitorRegistration {
    execution_id: String,
    registry: Arc<Mutex<HashSet<String>>>,
}

impl Drop for MonitorRegistration {
    fn drop(&mut self) {
        if let Ok(mut registry) = self.registry.lock() {
            registry.remove(&self.execution_id);
        }
    }
}

impl ExecutionService {
    pub(super) fn spawn_monitor(
        &self,
        task: ConnectorTaskSnapshot,
        execution_id: String,
        auth: AuthContext,
    ) -> bool {
        {
            let mut monitors = self.monitors.lock().unwrap();
            if !monitors.insert(execution_id.clone()) {
                return false;
            }
        }
        #[cfg(test)]
        self.monitor_starts
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let service = self.clone();
        let registration = MonitorRegistration {
            execution_id: execution_id.clone(),
            registry: self.monitors.clone(),
        };
        tokio::spawn(async move {
            let _registration = registration;
            service.monitor(task, execution_id, auth).await;
        });
        true
    }

    async fn monitor(&self, task: ConnectorTaskSnapshot, execution_id: String, auth: AuthContext) {
        let mut status_failures = 0_u32;
        let mut first_status_failure = None;
        loop {
            let execution = match self.db.connector_execution(&execution_id) {
                Ok(execution) => execution,
                Err(error) => {
                    tracing::warn!(execution_id, error = %error, "execution monitor lost durable state");
                    return;
                }
            };
            let now = chrono::Utc::now().timestamp();
            if execution.is_terminal() {
                if execution.state == "cancelled" {
                    self.release_cancelled_workspace(task).await;
                }
                return;
            }
            let current = if execution.state == "queued" && now >= execution.queue_deadline {
                self.db
                    .request_connector_queue_timeout(&execution_id, now)
                    .unwrap_or(execution)
            } else {
                execution
            };
            if current.state == "cancel_requested" {
                let _ = self.dispatch_cancel(&task, &current, &auth).await;
            }
            match self.refresh_once(&task, &execution_id, &auth).await {
                Ok(updated) => {
                    status_failures = 0;
                    first_status_failure = None;
                    if updated.is_terminal() {
                        if updated.state == "cancelled" {
                            self.release_cancelled_workspace(task).await;
                        }
                        return;
                    }
                }
                Err((failure_code, error)) => {
                    status_failures = status_failures.saturating_add(1);
                    let failure_started = first_status_failure.get_or_insert_with(Instant::now);
                    let degraded = match self.db.record_connector_execution_status_failure(
                        &execution_id,
                        failure_code,
                        chrono::Utc::now().timestamp(),
                    ) {
                        Ok(execution) => execution,
                        Err(store_error) => {
                            tracing::warn!(
                                execution_id,
                                error = %store_error,
                                "execution monitor could not persist degraded status"
                            );
                            return;
                        }
                    };
                    if failure_started.elapsed() >= self.monitor_timing.grace {
                        tracing::warn!(
                            execution_id,
                            error,
                            "executor terminal state became unknown"
                        );
                        let _ = self.db.finish_connector_execution(
                            &execution_id,
                            ConnectorExecutionFailure::Unknown(failure_code),
                            chrono::Utc::now().timestamp(),
                        );
                        return;
                    }
                    let delay = self.monitor_delay(&degraded, status_failures);
                    tokio::time::sleep(delay).await;
                    continue;
                }
            }
            let current = match self.db.connector_execution(&execution_id) {
                Ok(execution) => execution,
                Err(_) => return,
            };
            tokio::time::sleep(self.monitor_delay(&current, status_failures)).await;
        }
    }

    fn monitor_delay(&self, execution: &ConnectorExecution, status_failures: u32) -> Duration {
        if status_failures > 0 {
            let multiplier = 1_u32 << status_failures.saturating_sub(1).min(4);
            return (self.monitor_timing.fast_poll * multiplier)
                .min(self.monitor_timing.failure_poll_max);
        }
        if matches!(
            execution.state.as_str(),
            "accepted" | "starting" | "queued" | "cancel_requested"
        ) {
            return self.monitor_timing.fast_poll;
        }
        let now = chrono::Utc::now().timestamp();
        let last_progress = execution
            .last_output_at
            .or(execution.started_at)
            .unwrap_or(execution.submitted_at);
        if now.saturating_sub(last_progress) >= 10 {
            self.monitor_timing.silent_poll
        } else {
            self.monitor_timing.running_poll
        }
    }

    async fn refresh_once(
        &self,
        task: &ConnectorTaskSnapshot,
        execution_id: &str,
        auth: &AuthContext,
    ) -> Result<ConnectorExecution, (&'static str, String)> {
        let execution = self
            .db
            .connector_execution(execution_id)
            .map_err(|error| ("task_store_error", error.to_string()))?;
        let job_id = execution.executor_reference.as_deref().ok_or_else(|| {
            (
                "executor_reference_pending",
                "execution has no executor reference".to_string(),
            )
        })?;
        let (job, _, _, stdout_cursor, stderr_cursor) = self
            .tools
            .shell_clients
            .job_log_for_auth(
                Some(auth),
                job_id,
                Some(execution.stdout_cursor),
                Some(execution.stderr_cursor),
                None,
            )
            .await
            .map_err(|error| ("executor_status_unavailable", error))?;
        if job.status == "lost" {
            return Err((
                "executor_status_unavailable",
                job.error
                    .unwrap_or_else(|| "executor job authority was lost".to_string()),
            ));
        }
        if !ConnectorExecution::executor_status_recognized(&job.status) {
            return Err((
                "executor_status_unrecognized",
                format!("executor returned unrecognized status '{}'", job.status),
            ));
        }
        let progress = job.validation_progress.as_ref();
        let check_completed = progress.map(|progress| progress.completed);
        let failed_check = progress.and_then(|progress| progress.failed_step.as_deref());
        let executor_failure_code = job.error.as_deref().and_then(executor_failure_code);
        let assertion_evidence = if execution.kind == "check" && failed_check.is_some() {
            let (_, full_stdout, full_stderr, _, _) = self
                .tools
                .shell_clients
                .job_log_for_auth(Some(auth), job_id, None, None, None)
                .await
                .unwrap_or_else(|_| (job.clone(), None, None, 1, 1));
            failed_check.map(|check| {
                durable_assertion_evidence(
                    check,
                    job.exit_code,
                    full_stdout.as_deref().unwrap_or_default(),
                    full_stderr.as_deref().unwrap_or_default(),
                )
            })
        } else {
            None
        };
        let validated_workspace_sha256 = if execution.kind == "check"
            && job.status == "completed"
            && job.exit_code == Some(0)
            && progress.is_some_and(|progress| {
                progress.completed == execution.check_plan.len()
                    && progress.current_step.is_none()
                    && progress.failed_step.is_none()
            }) {
            let manager = self.workspace.clone();
            let task = task.clone();
            tokio::task::spawn_blocking(move || manager.action_precondition(&task))
                .await
                .ok()
                .and_then(Result::ok)
                .filter(|current| {
                    execution.check_workspace_sha256.as_deref() == Some(current.as_str())
                })
        } else {
            None
        };
        self.db
            .observe_connector_execution(
                execution_id,
                ConnectorExecutionObservation {
                    executor_status: &job.status,
                    stdout_cursor,
                    stderr_cursor,
                    exit_code: job.exit_code,
                    started_at: job.started_at,
                    finished_at: job.ended_at,
                    check_completed,
                    failed_check,
                    assertion_evidence: assertion_evidence.as_ref(),
                    validated_workspace_sha256: validated_workspace_sha256.as_deref(),
                    executor_failure_code,
                    now: chrono::Utc::now().timestamp(),
                },
            )
            .map_err(|error| ("task_store_error", error.to_string()))
    }

    pub(super) async fn dispatch_cancel(
        &self,
        task: &ConnectorTaskSnapshot,
        execution: &ConnectorExecution,
        auth: &AuthContext,
    ) -> CancelDispatch {
        let Some(job_id) = execution.executor_reference.as_ref() else {
            return CancelDispatch::ReferencePending;
        };
        if self
            .tools
            .stop_job_model_facing(
                task.execution_executor_ref.clone(),
                job_id.clone(),
                None,
                true,
                Some(auth),
            )
            .await
            .success
        {
            CancelDispatch::Sent
        } else {
            CancelDispatch::Failed
        }
    }
}

fn validation_protocol_failure_code(error: &str) -> Option<&'static str> {
    let code = error
        .strip_prefix("executor protocol violation: ")?
        .split(':')
        .next()?;
    match code {
        "validation_progress_missing" => Some("validation_progress_missing"),
        "validation_progress_unexpected" => Some("validation_progress_unexpected"),
        "validation_progress_incomplete" => Some("validation_progress_incomplete"),
        "validation_progress_invalid" => Some("validation_progress_invalid"),
        "validation_plan_invalid" => Some("validation_plan_invalid"),
        _ => Some("validation_progress_invalid"),
    }
}

fn executor_failure_code(error: &str) -> Option<&'static str> {
    if error == crate::shell_protocol::VALIDATION_STEP_SPAWN_FAILED_CODE {
        Some(crate::shell_protocol::VALIDATION_STEP_SPAWN_FAILED_CODE)
    } else {
        validation_protocol_failure_code(error)
    }
}
