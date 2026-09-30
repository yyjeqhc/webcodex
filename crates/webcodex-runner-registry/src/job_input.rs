//! Exact existing-Job input admission. No new Job, target lookup heuristic, or replay.
use super::access_control::{assert_runner_access, job_visible_to_access};
use super::requests::{enqueue_pending_request_locked, notify_runner_locked};
use crate::{RunnerAccess, RunnerFeature, RunnerRegistry, ShellJobVisibility};
use tokio::sync::oneshot;
use webcodex_core::job_input::{validate_input, JobInputRequest};
use webcodex_core::runner_operation::{RunnerInvocationMetadata, RunnerOperation};
use webcodex_core::runner_protocol::{RunnerRequest, ShellRunResponse};

/// Recheck under the dequeue lock, before command/input bytes cross transport.
/// The Runner's second incarnation check is not a substitute for preventing
/// delivery of those bytes to a replacement registration/owner.
pub(super) fn dispatch_rejection(
    inner: &super::state::RunnerRegistryInner,
    operation: &RunnerOperation,
    client: &str,
) -> Option<&'static str> {
    use webcodex_core::runner_operation::RunnerJobOperation;
    let (id, expected, project) = match operation {
        RunnerOperation::JobInput(input) => (
            &input.job_id,
            Some(input.runner_instance_id.as_str()),
            Some(input.project.as_str()),
        ),
        RunnerOperation::Job(RunnerJobOperation::StartInteractiveProcess(start)) => {
            (&start.job_id, None, None)
        }
        _ => return None,
    };
    let Some(job) = inner.jobs_by_id.get(id) else {
        return Some("job_input_unavailable");
    };
    let Some(runner) = inner.runners.get(client) else {
        return Some("job_input_unavailable");
    };
    if runner.runner_instance_id != job.runner_instance_id
        || expected.is_some_and(|v| v != runner.runner_instance_id)
        || runner.owner != job.owner_at_admission
        || runner.auth_group != job.auth_group
    {
        return Some("job_input_instance_changed");
    }
    if !runner
        .runner_features
        .supports(RunnerFeature::JobProcessInput)
    {
        return Some("job_input_capability_unavailable");
    }
    if job.client_id != client || project.is_some_and(|p| job.project_id.as_deref() != Some(p)) {
        return Some("job_input_target_mismatch");
    }
    let prefix = format!("agent:{client}:");
    let local = job
        .project_id
        .as_deref()
        .and_then(|p| p.strip_prefix(&prefix));
    if !runner
        .projects
        .iter()
        .any(|p| !p.disabled && Some(p.id.as_str()) == local)
    {
        return Some("job_input_target_mismatch");
    }
    None
}

impl RunnerRegistry {
    pub async fn enqueue_job_input(
        &self,
        access: Option<&RunnerAccess>,
        project: &str,
        job_id: &str,
        input_id: String,
        data: String,
        close: bool,
    ) -> Result<(String, oneshot::Receiver<ShellRunResponse>), String> {
        validate_input(&input_id, &data, close)?;
        let mut inner = self.inner.lock().await;
        let job = inner
            .jobs_by_id
            .get(job_id)
            .ok_or("job_input_unavailable")?;
        if job.visibility != ShellJobVisibility::Public
            || !job_visible_to_access(access, &inner, job)
            || job.project_id.as_deref() != Some(project)
            || job.recovery_active()
            || !job
                .structured_execution
                .as_ref()
                .is_some_and(|metadata| metadata.execution_source == "run_process_interactive")
        {
            return Err("job_input_unavailable".into());
        }
        let client = job.client_id.clone();
        let instance = job.runner_instance_id.clone();
        let runner = inner.runners.get(&client).ok_or("job_input_unavailable")?;
        assert_runner_access(access, runner).map_err(|_| "job_input_unavailable")?;
        if runner.runner_instance_id != instance {
            return Err("job_input_instance_changed".into());
        }
        if !runner
            .runner_features
            .supports(RunnerFeature::JobProcessInput)
        {
            return Err("job_input_capability_unavailable".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let request = RunnerRequest::from_operation(
            RunnerInvocationMetadata {
                request_id: id.clone(),
                client_id: client.clone(),
                requested_by: "tool_runtime".into(),
                created_at: crate::now_ts(),
            },
            RunnerOperation::JobInput(JobInputRequest {
                job_id: job_id.into(),
                runner_instance_id: instance,
                project: project.into(),
                input_id,
                data,
                close,
            }),
        )?;
        let (tx, rx) = oneshot::channel();
        // The typed payload also fences Runner incarnation at execution time,
        // so reconnect cannot silently forward input to a replacement process.
        enqueue_pending_request_locked(
            self.telemetry.as_ref(),
            &mut inner,
            &client,
            id.clone(),
            request,
            Some(tx),
            None,
        )
        .map_err(|_| "job_input_unavailable")?;
        notify_runner_locked(&inner, &client);
        Ok((id, rx))
    }
}
