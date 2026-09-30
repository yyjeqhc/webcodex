//! Durable detached Job handoff, exact recovery ownership and live observation.

use super::lifecycle::{runner_job_is_active, runner_job_is_terminal};
use super::retained::job_update_from_snapshot;
use super::*;
pub(super) fn validate_detached_recovery_context(
    context: &ShellJobContext,
    client_id: &str,
) -> Result<(), String> {
    const MAX_CONTEXT_FIELD_CHARS: usize = 1_024;
    const MAX_COMMAND_PREVIEW_CHARS: usize = 121;
    let bounded =
        |value: &str, max_chars: usize| !value.contains('\0') && value.chars().count() <= max_chars;
    if !bounded(&context.command_preview, MAX_COMMAND_PREVIEW_CHARS)
        || context.command_preview.contains(['\r', '\n'])
    {
        return Err("detached Job recovery command_preview is invalid or oversized".to_string());
    }
    for (name, value) in [
        ("ssh_resource", context.ssh_resource.as_deref()),
        ("project_cwd", context.project_cwd.as_deref()),
        ("cwd", context.cwd.as_deref()),
        ("purpose", context.purpose.as_deref()),
        ("shell", context.shell.as_deref()),
    ] {
        if value.is_some_and(|value| !bounded(value, MAX_CONTEXT_FIELD_CHARS)) {
            return Err(format!(
                "detached Job recovery context {name} is invalid or oversized"
            ));
        }
    }
    if context.ssh_resource.is_some() && context.workflow_session_id.is_none() {
        return Err("detached Job recovery SSH resource requires a Workflow Session".to_string());
    }
    if context.purpose.as_deref().is_some_and(|purpose| {
        !matches!(
            purpose,
            "validation"
                | "test"
                | "build"
                | "format"
                | "release"
                | "diagnostic"
                | "operation"
                | "other"
        )
    }) {
        return Err("detached Job recovery purpose is invalid".to_string());
    }
    if context.shell.as_deref().is_some_and(|shell| {
        !matches!(
            shell,
            "sh" | "bash" | "powershell" | "configured" | "custom" | "remote" | "direct_argv"
        )
    }) {
        return Err("detached Job recovery shell is invalid".to_string());
    }
    if !context.validation_steps.is_empty() {
        if !(1..=3).contains(&context.validation_steps.len())
            || context
                .validation_steps
                .iter()
                .collect::<HashSet<_>>()
                .len()
                != context.validation_steps.len()
            || context
                .validation_steps
                .iter()
                .any(|step| !matches!(step.as_str(), "format" | "check" | "test"))
        {
            return Err("detached Job recovery validation_steps are invalid".to_string());
        }
    }
    if context.validation.as_ref().is_some_and(|metadata| {
        !metadata.is_valid()
            || metadata
                .steps
                .iter()
                .map(|step| step.name.clone())
                .collect::<Vec<_>>()
                != context.validation_steps
    }) {
        return Err("detached Job recovery validation metadata is invalid".to_string());
    }
    if context
        .structured_execution
        .as_ref()
        .is_some_and(|metadata| !metadata.is_valid())
    {
        return Err("detached Job recovery structured execution metadata is invalid".to_string());
    }
    if let Some(project_id) = context.runtime_project_id.as_deref() {
        let prefix = format!("agent:{client_id}:");
        if !bounded(project_id, MAX_CONTEXT_FIELD_CHARS)
            || project_id
                .strip_prefix(&prefix)
                .is_none_or(|suffix| suffix.is_empty())
        {
            return Err("detached Job recovery project does not match the runner".to_string());
        }
    }
    if let Some(session_id) = context.workflow_session_id.as_deref() {
        if context.runtime_project_id.is_none()
            || !webcodex_core::workflow_session_contract::is_valid_session_id(session_id)
        {
            return Err("detached Job recovery Workflow Session is invalid".to_string());
        }
    }
    Ok(())
}
impl JobManager {
    pub(super) fn detached_store_for_start(
        &self,
        client_id: &str,
    ) -> Result<DetachedJobStore, String> {
        #[cfg(test)]
        if let Some(root) = lock_unpoison(&self.detached_store_root_override).clone() {
            return Ok(DetachedJobStore::new(root));
        }
        DetachedJobStore::default_root_for_runner(client_id, &self.detached_profile_server_url)
            .map(DetachedJobStore::new)
    }

    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub(in crate::webcodex_runner) fn recover_detached_jobs(
        &self,
        store: DetachedJobStore,
        client_id: &str,
        runner_instance_id: &str,
    ) -> Result<usize, String> {
        let records = store.scan_for_client(client_id)?;
        let mut recoverable = Vec::new();
        for record in records {
            let Some(record) = store.reconcile_after_runner_restart(record)? else {
                continue;
            };
            validate_detached_recovery_context(&record.context, client_id)?;
            let snapshot = snapshot_from_detached_record(&record)?;
            if runner_job_is_terminal(&snapshot.status)
                && snapshot.ended_at.is_some_and(|ended| {
                    chrono::Utc::now().timestamp().saturating_sub(ended)
                        >= JOB_TERMINAL_RETENTION_SECS
                })
            {
                continue;
            }
            recoverable.push((record, snapshot));
        }
        let active_count = recoverable
            .iter()
            .filter(|(_, snapshot)| runner_job_is_active(&snapshot.status))
            .count();
        if active_count > JOB_INVENTORY_MAX_ACTIVE_JOBS {
            return Err(format!(
                "detached Job recovery found {active_count} active records; maximum is {JOB_INVENTORY_MAX_ACTIVE_JOBS}"
            ));
        }

        let mut observers = Vec::new();
        {
            let _lifecycle = lock_unpoison(&self.lifecycle);
            let mut detached_jobs = lock_unpoison(&self.detached_jobs);
            let mut jobs = lock_unpoison(&self.jobs);
            for (record, snapshot) in recoverable {
                if jobs.contains_key(&record.job_id) || detached_jobs.contains_key(&record.job_id) {
                    return Err(format!(
                        "detached Job recovery conflicts with existing local job {}",
                        record.job_id
                    ));
                }
                let active = runner_job_is_active(&snapshot.status);
                let detached = DetachedJobRef {
                    store: store.clone(),
                    execution_id: record.execution_id.clone(),
                };
                let job_id = record.job_id.clone();
                jobs.insert(
                    job_id.clone(),
                    RunningJob {
                        client_id: client_id.to_string(),
                        runner_instance_id: runner_instance_id.to_string(),
                        snapshot,
                        child: None,
                        stop_requested: Arc::new(AtomicBool::new(record.stop_requested)),
                        slot_reserved: active,
                    },
                );
                if active {
                    detached_jobs.insert(job_id.clone(), detached.clone());
                    observers.push((job_id, detached));
                }
            }
        }
        let recovered = observers.len();
        for (job_id, detached) in observers {
            if let Err(error) = self.spawn_detached_observer(job_id.clone(), detached) {
                // Observation is best-effort after exact durable ownership has
                // been recovered. Keep the durable control reference and Job
                // projection so shutdown exclusion and normal stop routing
                // remain correct even when live observation is degraded.
                tracing::error!(job_id = %job_id, error = %error, "detached Job observer startup failed; durable control retained");
            }
        }
        Ok(recovered)
    }

    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub(super) fn spawn_detached_observer(
        &self,
        job_id: String,
        detached: DetachedJobRef,
    ) -> Result<(), String> {
        #[cfg(test)]
        if self.fail_detached_observer_spawn.load(Ordering::SeqCst) {
            return Err("test-injected detached observer startup failure".to_string());
        }
        let manager = self.clone_for_worker();
        let shutting_down = Arc::clone(&self.shutting_down);
        let worker_guard = self.workers.enter();
        let observer_job_id = job_id.clone();
        std::thread::Builder::new()
            .name("webcodex-detached-job-observer".to_string())
            .spawn(move || {
                let _worker_guard = worker_guard;
                loop {
                    if shutting_down.load(Ordering::SeqCst) {
                        return;
                    }
                    let record = match detached.store.read(&observer_job_id) {
                        Ok(record) => record,
                        Err(error) => {
                            tracing::error!(job_id = %observer_job_id, error = %error, "detached Job durable observer failed closed");
                            return;
                        }
                    };
                    let record = match detached.store.reconcile_after_runner_restart(record) {
                        Ok(Some(record)) => record,
                        Ok(None) => {
                            tracing::error!(job_id = %observer_job_id, "detached Job observer found a pre-accept record after recovery");
                            return;
                        }
                        Err(error) => {
                            tracing::error!(job_id = %observer_job_id, error = %error, "detached Job liveness reconciliation failed closed");
                            return;
                        }
                    };
                    if record.execution_id != detached.execution_id {
                        tracing::error!(job_id = %observer_job_id, "detached Job durable observer saw execution identity replacement");
                        return;
                    }
                    let terminal = record.phase == crate::webcodex_runner::detached_job::DetachedJobPhase::Terminal;
                    match manager.sync_detached_record(&observer_job_id, &record) {
                        Ok(_) => {}
                        Err(error) => {
                            tracing::error!(job_id = %observer_job_id, error = %error, "detached Job inventory sync failed closed");
                            return;
                        }
                    }
                    if terminal {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            })
            .map(|_| ())
            .map_err(|error| format!("failed to start detached Job observer: {error}"))
    }

    pub(super) fn sync_detached_record(
        &self,
        job_id: &str,
        record: &crate::webcodex_runner::detached_job::DetachedJobRecord,
    ) -> Result<bool, String> {
        let snapshot = snapshot_from_detached_record(record)?;
        let delivery_order = lock_unpoison(&self.job_update_delivery_order);
        let (update, terminal, semantic) = {
            let mut jobs = lock_unpoison(&self.jobs);
            let job = jobs
                .get_mut(job_id)
                .ok_or_else(|| format!("unknown recovered detached Job: {job_id}"))?;
            if snapshot.request_id != job.snapshot.request_id
                || snapshot.context != job.snapshot.context
            {
                return Err("detached Job durable ownership context changed".to_string());
            }
            if snapshot.update_seq < job.snapshot.update_seq {
                return Err("detached Job durable update sequence regressed".to_string());
            }
            if snapshot.update_seq == job.snapshot.update_seq {
                if snapshot != job.snapshot {
                    return Err(
                        "detached Job state changed without advancing update sequence".to_string(),
                    );
                }
                return Ok(runner_job_is_terminal(&snapshot.status));
            }
            let semantic = snapshot.status != job.snapshot.status
                || snapshot.started_at != job.snapshot.started_at
                || snapshot.ended_at != job.snapshot.ended_at
                || snapshot.exit_code != job.snapshot.exit_code
                || snapshot.duration_ms != job.snapshot.duration_ms
                || snapshot.error != job.snapshot.error
                || snapshot.command_execution_state != job.snapshot.command_execution_state
                || snapshot.validation_progress != job.snapshot.validation_progress;
            job.snapshot = snapshot;
            job.stop_requested
                .store(record.stop_requested, Ordering::SeqCst);
            let terminal = runner_job_is_terminal(&job.snapshot.status);
            if terminal {
                job.slot_reserved = false;
                job.child = None;
            }
            (
                job_update_from_snapshot(&job.client_id, &job.runner_instance_id, &job.snapshot),
                terminal,
                semantic || terminal,
            )
        };
        self.queue_recorded_update(update, terminal || semantic);
        drop(delivery_order);
        if terminal {
            lock_unpoison(&self.detached_jobs).remove(job_id);
            self.start_available_queued();
        }
        Ok(terminal)
    }

    pub(super) fn start_detached_process_job(&self, start: PendingJobStart) {
        let PendingJobStart {
            generation,
            policy,
            shell,
            project_registry_dir,
            metadata,
            operation,
            ..
        } = start;
        let job_id = operation.job_id().to_string();
        if !matches!(operation, RunnerJobOperation::StartDetachedProcess(_)) {
            unreachable!("detached Job starter received non-detached operation");
        }
        let (stop_requested, runner_instance_id) = {
            let _lifecycle = lock_unpoison(&self.lifecycle);
            if self.shutting_down.load(Ordering::SeqCst) {
                (None, None)
            } else {
                let mut jobs = lock_unpoison(&self.jobs);
                let Some(job) = jobs.get_mut(&job_id) else {
                    return;
                };
                job.slot_reserved = true;
                (
                    Some(Arc::clone(&job.stop_requested)),
                    Some(job.runner_instance_id.clone()),
                )
            }
        };
        let (Some(stop_requested), Some(runner_instance_id)) = (stop_requested, runner_instance_id)
        else {
            self.shutdown_rejection(&operation);
            return;
        };
        let manager = self.clone_for_worker();
        let worker_guard = self.workers.enter();
        std::thread::spawn(move || {
            let _worker_guard = worker_guard;
            let RunnerJobOperation::StartDetachedProcess(request) = &operation else {
                unreachable!("detached Job starter received non-detached operation");
            };
            let prepared = match prepare_detached_process_launch(
                generation,
                &policy,
                &shell,
                &project_registry_dir,
                &manager.prepared_profiles,
                request.cwd.as_deref(),
                &request.process.executable,
                &request.process.args,
                request.timeout_secs,
                Some(stop_requested.as_ref()),
            ) {
                Ok(prepared) => prepared,
                Err(error) => {
                    manager.fail_job(&operation, error, None);
                    manager.start_available_queued();
                    return;
                }
            };
            if stop_requested.load(Ordering::SeqCst) || manager.shutting_down.load(Ordering::SeqCst)
            {
                manager.update_and_send(
                    &job_id,
                    RunnerJobDelta {
                        status: "stopped".to_string(),
                        duration_ms: Some(0),
                        error: Some(
                            "detached process Job stopped before ownership acceptance".to_string(),
                        ),
                        command_execution_state: Some(ShellCommandExecutionState::NotStarted),
                        finished: true,
                        ..Default::default()
                    },
                );
                manager.start_available_queued();
                return;
            }
            let store = match manager.detached_store_for_start(&metadata.client_id) {
                Ok(store) => store,
                Err(error) => {
                    manager.fail_job(&operation, error, None);
                    manager.start_available_queued();
                    return;
                }
            };
            let detached_request = DetachedStartRequest {
                job_id: job_id.clone(),
                request_id: metadata.request_id.clone(),
                client_id: metadata.client_id.clone(),
                runner_instance_id,
                context: request.context.clone(),
                launch: DetachedLaunchSpec {
                    process: prepared.process,
                    cwd: Some(prepared.cwd),
                    stdin: request.stdin.clone(),
                    env: prepared.env,
                    timeout_secs: prepared.timeout_secs,
                },
            };
            let outcome = match handoff_detached_job(&store, detached_request) {
                Ok(outcome) => outcome,
                Err(error) => {
                    match store.read(&job_id) {
                        Ok(record) => {
                            if let Err(sync_error) = manager.sync_detached_record(&job_id, &record)
                            {
                                tracing::error!(job_id = %job_id, error = %sync_error, "detached Job failed-start durable sync failed closed");
                            }
                        }
                        Err(_) => manager.fail_job(&operation, error, None),
                    }
                    manager.start_available_queued();
                    return;
                }
            };
            let (execution_id, record, observe) = match outcome {
                DetachedHandoffOutcome::Accepted {
                    execution_id,
                    record,
                    ..
                }
                | DetachedHandoffOutcome::Existing {
                    execution_id,
                    record,
                }
                | DetachedHandoffOutcome::OutcomeUnknown {
                    execution_id,
                    record,
                } => (execution_id, record, true),
                DetachedHandoffOutcome::PreAcceptFailed {
                    execution_id,
                    record,
                } => (execution_id, record, false),
            };
            let detached = DetachedJobRef {
                store: store.clone(),
                execution_id,
            };
            if observe {
                lock_unpoison(&manager.detached_jobs).insert(job_id.clone(), detached.clone());
            }
            match manager.sync_detached_record(&job_id, &record) {
                Ok(terminal) if !terminal && observe => {
                    if let Err(error) = manager.spawn_detached_observer(job_id.clone(), detached) {
                        tracing::error!(job_id = %job_id, error = %error, "detached Job observer startup failed after ownership handoff; durable control retained");
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::error!(job_id = %job_id, error = %error, "detached Job durable sync failed after ownership handoff; durable control retained");
                }
            }
            manager.start_available_queued();
        });
    }
}
