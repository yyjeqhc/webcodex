//! Owns Runner Job admission, lifecycle/retained state, and bounded update delivery.
//!
//! Transport supplies a replaceable sink; execution helpers still own process
//! I/O and process-tree primitives. Neither owns the Job registry or its queues.
use super::config::{
    HotRunnerConfig, ReloadableRunnerConfig, RunnerPolicy, ShellConfig, SkillsConfig, SshConfig,
};
use super::detached_job::{
    handoff_detached_job, snapshot_from_detached_record, DetachedHandoffOutcome, DetachedJobStore,
    DetachedLaunchSpec, DetachedStartRequest,
};
use super::output_text::OutputTextSource;
use super::runner_skills::run_skill_resource_with_profiles_and_execution_state;
use super::shell::{
    configured_explicit_shell_command, configured_prepared_shell_job_command,
    configured_shell_job_command, configured_validation_job_command, cwd_allowed,
    prepare_detached_process_launch, resolve_prepared_shell_profile,
    run_process_with_profiles_and_execution_state_with_start_hook,
    run_script_with_profiles_and_execution_state_with_start_hook, PreparedShellProfileCache,
};
use super::shutdown::{lock_unpoison, ActivityTracker};
use super::ssh::{is_transport_failure, SshConnectionPool};
use super::transport::RunnerSink;
use std::collections::{HashMap, HashSet, VecDeque};
#[cfg(all(test, unix))]
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex, Weak};
use std::time::{Duration, Instant};
use webcodex_core::runner_job_lifecycle::RunnerJobLifecycle;
use webcodex_core::runner_operation::{RunnerInvocationMetadata, RunnerJobOperation};
use webcodex_core::runner_protocol::{
    self, RunnerJobUpdateRequest, RunnerRequest, ShellCommandExecutionState, ShellJobActivity,
    ShellJobActivityPhase, ShellJobActivitySource, ShellJobActivityState, ShellJobContext,
    ShellJobInventory, ShellJobLogSnapshot, ShellJobSnapshot, ShellJobStreamSnapshot,
    ShellJobTestCountEvidence, ShellJobValidationProgress, ShellJobValidationStep,
    JOB_INVENTORY_MAX_ACTIVE_JOBS, JOB_INVENTORY_MAX_SERIALIZED_BYTES,
    JOB_INVENTORY_MAX_TERMINAL_JOBS, JOB_SNAPSHOT_STREAM_MAX_BYTES, JOB_TERMINAL_RETENTION_SECS,
    VALIDATION_STEP_SPAWN_FAILED_CODE, VALIDATION_TOOL_UNAVAILABLE_CODE,
};
use webcodex_core::workflow_session_contract::ExecutionShell;
use webcodex_process::ManagedChild;
// Existing process-I/O / execution helpers deliberately remain at their current
// owner. Extracting those independent facilities is outside this refactor.
use crate::{
    cleanup_managed_tree, drain_and_join_reader_threads_until, finish_cargo_test_count_evidence,
    managed_tree_running, observe_cargo_test_count_chunks, reap_managed_direct_child,
    request_terminate_managed_tree, spawn_reader, terminate_managed_tree, validation_failed_step,
    validation_module_available, wait_failure_error, wait_managed_tree_exit, OutputChunk,
};
#[cfg(test)]
use webcodex_core::runner_operation::{self, RunnerOperation};

const JOB_UPDATE_INTERVAL_MS: u64 = 250;

/// At most the validated Job state machine's required semantic transitions are
/// retained for live delivery. Output and advisory current-activity-only
/// updates are coalesced separately below.
const JOB_UPDATE_REQUIRED_PENDING_MAX: usize = 8;
const JOB_UPDATE_DELIVERY_RETRY: Duration = Duration::from_millis(JOB_UPDATE_INTERVAL_MS);

#[derive(Debug, Default)]
struct JobUpdateDeliverySignalState {
    generation: u64,
    closed: bool,
}

#[derive(Debug, Default)]
struct JobUpdateDeliverySignal {
    state: Mutex<JobUpdateDeliverySignalState>,
    wake: Condvar,
}

impl JobUpdateDeliverySignal {
    fn notify(&self) {
        let mut state = lock_unpoison(&self.state);
        state.generation = state.generation.saturating_add(1);
        self.wake.notify_one();
    }

    fn close(&self) {
        let mut state = lock_unpoison(&self.state);
        state.closed = true;
        state.generation = state.generation.saturating_add(1);
        self.wake.notify_all();
    }

    fn generation(&self) -> u64 {
        lock_unpoison(&self.state).generation
    }

    fn wait_for_change(&self, observed: u64, timeout: Duration) -> Option<u64> {
        let mut state = lock_unpoison(&self.state);
        if state.closed {
            return None;
        }
        if state.generation == observed {
            let (next, _) = self
                .wake
                .wait_timeout(state, timeout)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state = next;
        }
        (!state.closed).then_some(state.generation)
    }
}

#[derive(Debug)]
struct JobManagerOwnerLifetime {
    jobs: Weak<Mutex<HashMap<String, RunningJob>>>,
    detached_jobs: Weak<Mutex<HashMap<String, DetachedJobRef>>>,
    shutting_down: Weak<AtomicBool>,
    delivery_signal: Arc<JobUpdateDeliverySignal>,
}

impl Drop for JobManagerOwnerLifetime {
    fn drop(&mut self) {
        self.delivery_signal.close();
        if let Some(shutting_down) = self.shutting_down.upgrade() {
            shutting_down.store(true, Ordering::SeqCst);
        }
        let Some(jobs) = self.jobs.upgrade() else {
            return;
        };
        let detached_ids = self
            .detached_jobs
            .upgrade()
            .map(|detached| {
                lock_unpoison(&detached)
                    .keys()
                    .cloned()
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default();
        let targets = {
            let jobs = lock_unpoison(&jobs);
            jobs.iter()
                .filter(|(job_id, job)| {
                    runner_job_is_active(&job.snapshot.status)
                        && !detached_ids.contains(job_id.as_str())
                })
                .map(|(_, job)| (job.child.clone(), Arc::clone(&job.stop_requested)))
                .collect::<Vec<_>>()
        };
        for (child, stop_requested) in targets {
            stop_requested.store(true, Ordering::SeqCst);
            let Some(child) = child else {
                continue;
            };
            let mut child = match child.try_lock() {
                Ok(child) => child,
                Err(std::sync::TryLockError::WouldBlock) => continue,
                Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            };
            let _ = child.terminate_tree();
        }
    }
}

/// A Job start accepted by `enqueue` and waiting for a slot: the immutable
/// inputs the start pipeline consumes once capacity is available.
///
/// The sink is deliberately not part of this unit. `enqueue` installs it on
/// the manager and reports admission failures through it, but the queued
/// entry and every downstream start path communicate through the installed
/// sink (`current_sink`); a sink carried here would be dead weight for every
/// consumer of the queue.
#[derive(Debug, Clone)]
pub(super) struct PendingJobStart {
    generation: u64,
    policy: RunnerPolicy,
    shell: ShellConfig,
    ssh: SshConfig,
    skills: SkillsConfig,
    client_id: String,
    server_url: String,
    project_registry_dir: PathBuf,
    metadata: RunnerInvocationMetadata,
    operation: RunnerJobOperation,
}

impl PendingJobStart {
    pub(super) fn from_invocation(
        config: &HotRunnerConfig,
        runtime: &ReloadableRunnerConfig,
        project_registry_dir: &Path,
        metadata: RunnerInvocationMetadata,
        operation: RunnerJobOperation,
    ) -> Self {
        Self {
            generation: config.generation,
            policy: config.policy.clone(),
            shell: config.shell.clone(),
            ssh: config.ssh.clone(),
            skills: config.skills.clone(),
            client_id: runtime.client_id().to_string(),
            server_url: runtime.server_url().to_string(),
            project_registry_dir: project_registry_dir.to_path_buf(),
            metadata,
            operation,
        }
    }
}

#[cfg(test)]
impl PendingJobStart {
    pub(super) fn from_wire(
        generation: u64,
        policy: RunnerPolicy,
        shell: ShellConfig,
        ssh: SshConfig,
        project_registry_dir: PathBuf,
        request: RunnerRequest,
    ) -> Self {
        let client_id = request.client_id.clone();
        let invocation = request
            .decode_invocation()
            .expect("test Job wire request must decode to a canonical invocation");
        let RunnerOperation::Job(operation) = invocation.operation else {
            panic!("test PendingJobStart requires a Job operation");
        };
        assert!(
            operation.is_start(),
            "test PendingJobStart requires a start operation"
        );
        Self {
            generation,
            policy,
            shell,
            ssh,
            skills: SkillsConfig::default(),
            client_id,
            server_url: "http://127.0.0.1:1".to_string(),
            project_registry_dir,
            metadata: invocation.metadata,
            operation,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct JobManager {
    max_concurrent: usize,
    jobs: Arc<Mutex<HashMap<String, RunningJob>>>,
    detached_jobs: Arc<Mutex<HashMap<String, DetachedJobRef>>>,
    queued: Arc<Mutex<VecDeque<PendingJobStart>>>,
    prepared_profiles: PreparedShellProfileCache,
    ssh_pool: SshConnectionPool,
    lifecycle: Arc<Mutex<()>>,
    shutting_down: Arc<AtomicBool>,
    workers: ActivityTracker,
    current_sink: Arc<Mutex<Option<RunnerSink>>>,
    pending_job_updates: Arc<Mutex<HashMap<String, JobUpdateDeliveryQueue>>>,
    delivery_signal: Arc<JobUpdateDeliverySignal>,
    owner_lifetime: Option<Arc<JobManagerOwnerLifetime>>,
    detached_profile_server_url: String,
    #[cfg(test)]
    fail_detached_observer_spawn: Arc<AtomicBool>,
    #[cfg(test)]
    detached_store_root_override: Arc<Mutex<Option<PathBuf>>>,
}

impl JobManager {
    pub(crate) fn new(max_concurrent: usize) -> Self {
        let jobs = Arc::new(Mutex::new(HashMap::new()));
        let detached_jobs = Arc::new(Mutex::new(HashMap::new()));
        let shutting_down = Arc::new(AtomicBool::new(false));
        let current_sink = Arc::new(Mutex::new(None));
        let pending_job_updates = Arc::new(Mutex::new(HashMap::new()));
        let delivery_signal = Arc::new(JobUpdateDeliverySignal::default());
        spawn_job_update_delivery_worker(
            Arc::downgrade(&jobs),
            Arc::downgrade(&current_sink),
            Arc::downgrade(&pending_job_updates),
            Arc::clone(&delivery_signal),
        );
        let owner_lifetime = Arc::new(JobManagerOwnerLifetime {
            jobs: Arc::downgrade(&jobs),
            detached_jobs: Arc::downgrade(&detached_jobs),
            shutting_down: Arc::downgrade(&shutting_down),
            delivery_signal: Arc::clone(&delivery_signal),
        });
        Self {
            max_concurrent: max_concurrent.max(1),
            jobs,
            detached_jobs,
            queued: Arc::new(Mutex::new(VecDeque::new())),
            prepared_profiles: PreparedShellProfileCache::default(),
            ssh_pool: SshConnectionPool::default(),
            lifecycle: Arc::new(Mutex::new(())),
            shutting_down,
            workers: ActivityTracker::default(),
            current_sink,
            pending_job_updates,
            delivery_signal,
            owner_lifetime: Some(owner_lifetime),
            detached_profile_server_url: String::new(),
            #[cfg(test)]
            fail_detached_observer_spawn: Arc::new(AtomicBool::new(false)),
            #[cfg(test)]
            detached_store_root_override: Arc::new(Mutex::new(None)),
        }
    }

    pub(super) fn with_detached_profile_identity(mut self, server_url: &str) -> Self {
        self.detached_profile_server_url = server_url.to_string();
        self
    }

    pub(crate) fn prepared_profiles(&self) -> &PreparedShellProfileCache {
        &self.prepared_profiles
    }
    pub(super) fn ssh_pool(&self) -> &SshConnectionPool {
        &self.ssh_pool
    }
    #[cfg(test)]
    pub(super) fn with_ssh_pool_for_test(mut self, pool: SshConnectionPool) -> Self {
        self.ssh_pool = pool;
        self
    }

    fn clone_for_worker(&self) -> Self {
        let mut worker = self.clone();
        worker.owner_lifetime = None;
        worker
    }
}

#[derive(Debug, Clone)]
struct DetachedJobRef {
    store: DetachedJobStore,
    execution_id: String,
}

#[derive(Debug, Clone)]
struct RunningJob {
    client_id: String,
    runner_instance_id: String,
    snapshot: ShellJobSnapshot,
    /// The single owner of the job's process tree. Clones are shared with the
    /// job's worker thread so it can poll the direct child and terminate the
    /// whole tree, but there is never more than one live `ManagedChild`.
    child: Option<Arc<Mutex<ManagedChild>>>,
    stop_requested: Arc<AtomicBool>,
    slot_reserved: bool,
}

#[derive(Debug, Clone)]
struct PendingJobUpdateDelivery {
    update_seq: u64,
    status: String,
    exit_code: Option<i32>,
    duration_ms: Option<u64>,
    error: Option<String>,
    command_execution_state: Option<ShellCommandExecutionState>,
    validation_progress: Option<ShellJobValidationProgress>,
    test_count_evidence: Option<ShellJobTestCountEvidence>,
    activity: Option<ShellJobActivity>,
    finished: bool,
}

impl PendingJobUpdateDelivery {
    fn from_update(update: &RunnerJobUpdateRequest) -> Self {
        Self {
            update_seq: update.update_seq.unwrap_or_default(),
            status: update.status.clone(),
            exit_code: update.exit_code,
            duration_ms: update.duration_ms,
            error: update.error.clone(),
            command_execution_state: update.command_execution_state.clone(),
            validation_progress: update.validation_progress.clone(),
            test_count_evidence: update.test_count_evidence.clone(),
            activity: update.activity,
            finished: update.finished,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct JobUpdateDeliveryQueue {
    required: VecDeque<PendingJobUpdateDelivery>,
    output_only: Option<PendingJobUpdateDelivery>,
    suspended_until_reconciliation: bool,
}

impl JobUpdateDeliveryQueue {
    fn enqueue(&mut self, update: PendingJobUpdateDelivery, semantic: bool) -> bool {
        if self.suspended_until_reconciliation {
            return true;
        }
        if semantic {
            self.output_only = None;
            if let Some(last) = self.required.back_mut() {
                if last.update_seq == update.update_seq {
                    *last = update;
                    return true;
                }
            }
            if self.required.len() >= JOB_UPDATE_REQUIRED_PENDING_MAX {
                self.required.clear();
                self.suspended_until_reconciliation = true;
                return false;
            }
            self.required.push_back(update);
        } else {
            self.output_only = Some(update);
        }
        true
    }

    fn next(&self) -> Option<&PendingJobUpdateDelivery> {
        if self.suspended_until_reconciliation {
            None
        } else {
            self.required.front().or(self.output_only.as_ref())
        }
    }

    fn acknowledge(&mut self, update_seq: u64) {
        if self
            .required
            .front()
            .is_some_and(|update| update.update_seq == update_seq)
        {
            self.required.pop_front();
        } else if self
            .output_only
            .as_ref()
            .is_some_and(|update| update.update_seq == update_seq)
        {
            self.output_only = None;
        }
    }

    fn discard_through(&mut self, update_seq: u64) {
        while self
            .required
            .front()
            .is_some_and(|update| update.update_seq <= update_seq)
        {
            self.required.pop_front();
        }
        if self
            .output_only
            .as_ref()
            .is_some_and(|update| update.update_seq <= update_seq)
        {
            self.output_only = None;
        }
    }

    fn is_empty(&self) -> bool {
        self.required.is_empty() && self.output_only.is_none()
    }
}

fn job_update_from_delivery(
    job: &RunningJob,
    pending: &PendingJobUpdateDelivery,
) -> RunnerJobUpdateRequest {
    let mut update =
        job_update_from_snapshot(&job.client_id, &job.runner_instance_id, &job.snapshot);
    update.update_seq = Some(pending.update_seq);
    update.status = pending.status.clone();
    update.exit_code = pending.exit_code;
    update.duration_ms = pending.duration_ms;
    update.error = pending.error.clone();
    update.command_execution_state = pending.command_execution_state.clone();
    update.validation_progress = pending.validation_progress.clone();
    update.test_count_evidence = pending.test_count_evidence.clone();
    update.activity = pending.activity;
    update.finished = pending.finished;
    update
}

fn spawn_job_update_delivery_worker(
    jobs: Weak<Mutex<HashMap<String, RunningJob>>>,
    current_sink: Weak<Mutex<Option<RunnerSink>>>,
    pending_job_updates: Weak<Mutex<HashMap<String, JobUpdateDeliveryQueue>>>,
    signal: Arc<JobUpdateDeliverySignal>,
) {
    std::thread::spawn(move || {
        let mut observed_generation = signal.generation();
        loop {
            let Some(pending_map) = pending_job_updates.upgrade() else {
                break;
            };
            let candidate = {
                let pending = lock_unpoison(&pending_map);
                pending.iter().find_map(|(job_id, queue)| {
                    queue.next().cloned().map(|update| (job_id.clone(), update))
                })
            };
            let Some((job_id, pending_update)) = candidate else {
                let Some(next) =
                    signal.wait_for_change(observed_generation, Duration::from_secs(60))
                else {
                    break;
                };
                observed_generation = next;
                continue;
            };

            let Some(jobs_map) = jobs.upgrade() else {
                break;
            };
            let update = {
                let jobs = lock_unpoison(&jobs_map);
                jobs.get(&job_id)
                    .map(|job| job_update_from_delivery(job, &pending_update))
            };
            let Some(update) = update else {
                lock_unpoison(&pending_map).remove(&job_id);
                continue;
            };

            let Some(sink_slot) = current_sink.upgrade() else {
                break;
            };
            let sink = lock_unpoison(&sink_slot).clone();
            let Some(sink) = sink else {
                let Some(next) =
                    signal.wait_for_change(observed_generation, Duration::from_secs(60))
                else {
                    break;
                };
                observed_generation = next;
                continue;
            };

            let send_result = if matches!(
                &sink,
                RunnerSink::WebSocket { .. } | RunnerSink::Quic { .. }
            ) {
                // Stream delivery is a non-blocking try_send. Keep candidate
                // validation and enqueue atomic with respect to coalescing: a
                // semantic update may supersede an output/activity-only item
                // after the worker clones it but before channel capacity returns.
                // HTTP remains outside this lock because it performs a bounded
                // synchronous request and must never block update producers.
                let pending = lock_unpoison(&pending_map);
                let still_pending = pending
                    .get(&job_id)
                    .and_then(JobUpdateDeliveryQueue::next)
                    .is_some_and(|current| current.update_seq == pending_update.update_seq);
                if !still_pending {
                    continue;
                }
                sink.try_send_job_update(&update)
            } else {
                sink.try_send_job_update(&update)
            };

            match send_result {
                Ok(true) => {
                    let still_current = lock_unpoison(&sink_slot)
                        .as_ref()
                        .is_some_and(|current| current.same_job_update_target(&sink));
                    if still_current {
                        let mut pending = lock_unpoison(&pending_map);
                        let remove = if let Some(queue) = pending.get_mut(&job_id) {
                            queue.acknowledge(pending_update.update_seq);
                            queue.is_empty() && !queue.suspended_until_reconciliation
                        } else {
                            false
                        };
                        if remove {
                            pending.remove(&job_id);
                        }
                    }
                }
                Ok(false) | Err(_) => {
                    let Some(next) =
                        signal.wait_for_change(observed_generation, JOB_UPDATE_DELIVERY_RETRY)
                    else {
                        break;
                    };
                    observed_generation = next;
                }
            }
        }
    });
}

#[cfg(test)]
fn test_job_snapshot(job_id: &str) -> ShellJobSnapshot {
    ShellJobSnapshot {
        job_id: job_id.to_string(),
        request_id: format!("request-{job_id}"),
        status: "running".to_string(),
        update_seq: 1,
        created_at: chrono::Utc::now().timestamp(),
        started_at: Some(chrono::Utc::now().timestamp()),
        ended_at: None,
        exit_code: None,
        duration_ms: None,
        error: None,
        command_execution_state: None,
        context: runner_protocol::ShellJobContext {
            runtime_project_id: None,
            workflow_session_id: None,
            ssh_resource: None,
            project_cwd: None,
            cwd: None,
            purpose: Some("other".to_string()),
            shell: Some("configured".to_string()),
            command_preview: "test job".to_string(),
            validation_steps: Vec::new(),
            validation: None,
            structured_execution: None,
        },
        stdout: ShellJobStreamSnapshot::default(),
        stderr: ShellJobStreamSnapshot::default(),
        validation_progress: None,
        test_count_evidence: None,
        activity: None,
    }
}

#[cfg(test)]
pub(crate) fn test_job_context(
    cwd: &Path,
    validation_steps: Vec<String>,
) -> runner_protocol::ShellJobContext {
    runner_protocol::ShellJobContext {
        runtime_project_id: None,
        workflow_session_id: None,
        ssh_resource: None,
        project_cwd: None,
        cwd: Some(cwd.to_string_lossy().into_owned()),
        purpose: Some("other".to_string()),
        shell: Some("configured".to_string()),
        command_preview: "test command".to_string(),
        validation_steps,
        validation: None,
        structured_execution: None,
    }
}

#[derive(Clone)]
struct JobShutdownTarget {
    child: Arc<Mutex<ManagedChild>>,
}

pub(super) struct JobShutdownBatch {
    targets: Vec<JobShutdownTarget>,
    running: usize,
    failures: usize,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct JobShutdownOutcome {
    resources: usize,
    timed_out: usize,
    failures: usize,
}

impl JobShutdownBatch {
    pub(super) fn running(&self) -> usize {
        self.running
    }
    pub(super) fn failures(&self) -> usize {
        self.failures
    }
}
impl JobShutdownOutcome {
    pub(super) fn resources(&self) -> usize {
        self.resources
    }
    pub(super) fn timed_out(&self) -> usize {
        self.timed_out
    }
    pub(super) fn failures(&self) -> usize {
        self.failures
    }
}

fn shutdown_target_running(target: &mut JobShutdownTarget) -> bool {
    if managed_tree_running(&target.child) {
        return true;
    }
    // Tree liveness and direct-child reaping are distinct on Unix. Darwin can
    // prove a zombie-only process group non-executable just before waitpid makes
    // the direct child's status observable. Keep the shutdown target pending
    // within the existing global deadline until that direct child is reaped;
    // do not re-signal the already-confirmed-empty process group.
    !reap_managed_direct_child(&target.child, Instant::now()).unwrap_or(false)
}

#[derive(Debug, Default)]
struct RunnerJobDelta {
    status: String,
    stdout_chunk: Option<String>,
    stderr_chunk: Option<String>,
    exit_code: Option<i32>,
    duration_ms: Option<u64>,
    error: Option<String>,
    command_execution_state: Option<ShellCommandExecutionState>,
    stream_limit_bytes: Option<usize>,
    validation_progress: Option<ShellJobValidationProgress>,
    test_count_evidence: Option<ShellJobTestCountEvidence>,
    activity: Option<ShellJobActivity>,
    finished: bool,
}

fn process_running_activity() -> ShellJobActivity {
    ShellJobActivity {
        state: ShellJobActivityState::Working,
        phase: ShellJobActivityPhase::ProcessRunning,
        source: ShellJobActivitySource::RunnerExecution,
    }
}

fn validation_step_activity(step: &ShellJobValidationStep) -> ShellJobActivity {
    let phase = match step.name.as_str() {
        "format" => ShellJobActivityPhase::ValidationFormat,
        "check" => ShellJobActivityPhase::ValidationCheck,
        "test" => ShellJobActivityPhase::ValidationTest,
        _ => unreachable!("canonical validation step name"),
    };
    ShellJobActivity {
        state: ShellJobActivityState::Working,
        phase,
        source: ShellJobActivitySource::ValidationPlan,
    }
}

/// Recognize only a tiny bounded subset of Cargo's own stderr progress while a
/// canonical structured Cargo validation step is running. Clear phase-boundary
/// lines return to the step's canonical validation-plan activity so transient
/// Cargo detail cannot remain sticky after that detail has ended. This is
/// advisory activity provenance, not validation/completion evidence.
fn cargo_activity_from_stderr(
    step: &ShellJobValidationStep,
    stderr: &str,
) -> Option<ShellJobActivity> {
    if step.program != "cargo" || !step.is_canonical() {
        return None;
    }
    let validation_activity = validation_step_activity(step);
    let mut observed = None;
    for line in stderr.lines() {
        let line = line.trim_start();
        let activity = if line.contains("Blocking waiting for file lock on build directory") {
            ShellJobActivity {
                state: ShellJobActivityState::Waiting,
                phase: ShellJobActivityPhase::CargoWaitingForBuildLock,
                source: ShellJobActivitySource::CargoOutput,
            }
        } else if line.starts_with("Compiling ") {
            ShellJobActivity {
                state: ShellJobActivityState::Working,
                phase: ShellJobActivityPhase::CargoCompiling,
                source: ShellJobActivitySource::CargoOutput,
            }
        } else if line.starts_with("Checking ") {
            ShellJobActivity {
                state: ShellJobActivityState::Working,
                phase: ShellJobActivityPhase::CargoChecking,
                source: ShellJobActivitySource::CargoOutput,
            }
        } else if line.starts_with("Finished ")
            || (step.name == "test"
                && (line.starts_with("Running unittests ")
                    || line.starts_with("Running tests/")
                    || line.starts_with("Running benches/")
                    || line.starts_with("Doc-tests ")))
        {
            validation_activity
        } else {
            continue;
        };
        observed = Some(activity);
    }
    observed
}

fn runner_job_is_terminal(status: &str) -> bool {
    RunnerJobLifecycle::from_wire(status).is_ok_and(RunnerJobLifecycle::is_terminal)
}

fn runner_job_is_active(status: &str) -> bool {
    RunnerJobLifecycle::from_wire(status).is_ok_and(RunnerJobLifecycle::is_runner_active)
}

fn job_prestart_lifecycle(operation: &RunnerJobOperation) -> Option<ShellCommandExecutionState> {
    match operation {
        RunnerJobOperation::StartShell(_)
        | RunnerJobOperation::StartProcess(_)
        | RunnerJobOperation::StartDetachedProcess(_)
        | RunnerJobOperation::StartScript(_)
        | RunnerJobOperation::StartSkillResource(_) => Some(ShellCommandExecutionState::NotStarted),
        RunnerJobOperation::StartValidation(_) | RunnerJobOperation::Stop { .. } => None,
    }
}

/// Compatibility-only lifecycle projection for malformed V2 Job requests that
/// fail before a canonical operation can be constructed. Production Job
/// execution never uses this string registry.
pub(crate) fn decode_failure_prestart_lifecycle(
    request: &RunnerRequest,
) -> Option<ShellCommandExecutionState> {
    matches!(
        request.kind.as_str(),
        "start_job"
            | "start_process_job"
            | "start_detached_process_job"
            | "start_script_job"
            | "start_skill_resource_job"
    )
    .then_some(ShellCommandExecutionState::NotStarted)
}

fn post_spawn_interruption_lifecycle(
    operation: &RunnerJobOperation,
) -> Option<ShellCommandExecutionState> {
    matches!(operation, RunnerJobOperation::StartShell(_))
        .then_some(ShellCommandExecutionState::OutcomeUnknown)
}

fn post_spawn_interruption_reason(
    shutting_down: bool,
    stop_requested: bool,
    job_record_present: bool,
) -> Option<&'static str> {
    if shutting_down {
        Some("runner began shutdown after command start")
    } else if stop_requested {
        Some("job stop requested after command start")
    } else if !job_record_present {
        Some("runner lost the Job record after command start")
    } else {
        None
    }
}

fn post_spawn_interruption_delta(
    operation: &RunnerJobOperation,
    duration_ms: u64,
    error: &str,
) -> RunnerJobDelta {
    RunnerJobDelta {
        status: "failed".to_string(),
        exit_code: None,
        duration_ms: Some(duration_ms),
        error: Some(error.to_string()),
        command_execution_state: post_spawn_interruption_lifecycle(operation),
        finished: true,
        ..Default::default()
    }
}

fn raw_shell_job_terminal_lifecycle(
    status: &str,
    exit_code: Option<i32>,
) -> ShellCommandExecutionState {
    match RunnerJobLifecycle::from_wire(status).ok() {
        Some(lifecycle) if lifecycle.is_timed_out() => ShellCommandExecutionState::TimedOut,
        Some(
            RunnerJobLifecycle::Completed
            | RunnerJobLifecycle::Stopped
            | RunnerJobLifecycle::Cancelled,
        ) => ShellCommandExecutionState::Completed,
        Some(RunnerJobLifecycle::Failed) if exit_code.is_some() => {
            ShellCommandExecutionState::Completed
        }
        _ => ShellCommandExecutionState::OutcomeUnknown,
    }
}

fn job_update_from_snapshot(
    client_id: &str,
    runner_instance_id: &str,
    snapshot: &ShellJobSnapshot,
) -> RunnerJobUpdateRequest {
    RunnerJobUpdateRequest {
        client_id: client_id.to_string(),
        runner_instance_id: runner_instance_id.to_string(),
        job_id: snapshot.job_id.clone(),
        request_id: Some(snapshot.request_id.clone()),
        update_seq: Some(snapshot.update_seq),
        status: snapshot.status.clone(),
        stdout_chunk: None,
        stderr_chunk: None,
        log_snapshot: Some(ShellJobLogSnapshot {
            stdout: snapshot.stdout.clone(),
            stderr: snapshot.stderr.clone(),
        }),
        exit_code: snapshot.exit_code,
        duration_ms: snapshot.duration_ms,
        error: snapshot.error.clone(),
        command_execution_state: snapshot.command_execution_state,
        validation_progress: snapshot.validation_progress.clone(),
        test_count_evidence: snapshot.test_count_evidence.clone(),
        activity: snapshot.activity,
        finished: runner_job_is_terminal(&snapshot.status),
    }
}

fn runner_retained_line_count(value: &str) -> usize {
    value.lines().count()
}

fn append_runner_stream(stream: &mut ShellJobStreamSnapshot, chunk: Option<&str>) {
    let Some(chunk) = chunk else {
        return;
    };
    stream.tail.push_str(chunk);
    if stream.tail.len() > JOB_SNAPSHOT_STREAM_MAX_BYTES {
        let observed_next = stream
            .first_retained_line
            .saturating_add(runner_retained_line_count(&stream.tail));
        let mut minimum_start = stream.tail.len() - JOB_SNAPSHOT_STREAM_MAX_BYTES;
        while minimum_start < stream.tail.len() && !stream.tail.is_char_boundary(minimum_start) {
            minimum_start += 1;
        }
        if let Some(relative_newline) = stream.tail[minimum_start..].find('\n') {
            let drop_end = minimum_start + relative_newline + 1;
            let dropped_lines = stream.tail[..drop_end]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count();
            stream.tail.drain(..drop_end);
            stream.first_retained_line = stream.first_retained_line.saturating_add(dropped_lines);
        } else {
            let dropped_lines = stream.tail[..minimum_start]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count();
            stream.tail.drain(..minimum_start);
            stream.first_retained_line = stream.first_retained_line.saturating_add(dropped_lines);
        }
        if stream.tail.is_empty() {
            // The last retained partial line was dropped too. Preserve the
            // absolute next cursor by advancing the empty range to the
            // observed end rather than resetting it backwards.
            stream.first_retained_line = observed_next;
        }
        stream.truncated = true;
    }
    stream.next_line = stream
        .first_retained_line
        .saturating_add(runner_retained_line_count(&stream.tail));
}

fn trim_runner_stream_to(stream: &mut ShellJobStreamSnapshot, max_bytes: usize) {
    if stream.tail.len() <= max_bytes {
        return;
    }
    let observed_next = stream
        .first_retained_line
        .saturating_add(runner_retained_line_count(&stream.tail));
    let mut minimum_start = stream.tail.len().saturating_sub(max_bytes);
    while minimum_start < stream.tail.len() && !stream.tail.is_char_boundary(minimum_start) {
        minimum_start += 1;
    }
    if let Some(relative_newline) = stream.tail[minimum_start..].find('\n') {
        let drop_end = minimum_start + relative_newline + 1;
        let dropped_lines = stream.tail[..drop_end]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count();
        stream.tail.drain(..drop_end);
        stream.first_retained_line = stream.first_retained_line.saturating_add(dropped_lines);
    } else {
        let dropped_lines = stream.tail[..minimum_start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count();
        stream.tail.drain(..minimum_start);
        stream.first_retained_line = stream.first_retained_line.saturating_add(dropped_lines);
    }
    if stream.tail.is_empty() {
        stream.first_retained_line = observed_next;
    }
    stream.truncated = true;
    stream.next_line = stream
        .first_retained_line
        .saturating_add(runner_retained_line_count(&stream.tail));
}

fn bounded_runner_error(error: Option<String>) -> Option<String> {
    error.map(|error| error.chars().take(4_096).collect())
}

/// Retain enough SSH diagnostic output to classify an uncertain transport
/// failure without allowing a chatty remote command to grow worker memory.
fn append_bounded_tail(target: &mut String, next: &str, max_bytes: usize) {
    target.push_str(next);
    if target.len() <= max_bytes {
        return;
    }
    let mut start = target.len().saturating_sub(max_bytes);
    while start < target.len() && !target.is_char_boundary(start) {
        start += 1;
    }
    target.drain(..start);
}

fn validate_detached_recovery_context(
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

fn validate_runner_job_context_operation(
    context: &ShellJobContext,
    operation: &RunnerJobOperation,
    client_id: &str,
) -> Result<(), String> {
    const MAX_CONTEXT_FIELD_CHARS: usize = 1_024;
    const MAX_COMMAND_PREVIEW_CHARS: usize = 121;
    let bounded =
        |value: &str, max_chars: usize| !value.contains('\0') && value.chars().count() <= max_chars;
    if !operation.is_start() {
        return Err("stop_job is not a Job start operation".to_string());
    }
    if operation.context() != Some(context) {
        return Err("job recovery context does not match the typed Job operation".to_string());
    }
    if !bounded(&context.command_preview, MAX_COMMAND_PREVIEW_CHARS)
        || context.command_preview.contains(['\r', '\n'])
    {
        return Err("job recovery context command_preview is invalid or oversized".to_string());
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
                "job recovery context {name} is invalid or oversized"
            ));
        }
    }
    if context.cwd.as_deref() != operation.cwd() {
        return Err("job recovery context cwd does not match the execution request".to_string());
    }
    if context.ssh_resource.is_some() && context.workflow_session_id.is_none() {
        return Err("job recovery context SSH resource requires a Workflow Session".to_string());
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
        return Err("job recovery context purpose is invalid".to_string());
    }
    if context.shell.as_deref().is_some_and(|shell| {
        !matches!(
            shell,
            "sh" | "bash"
                | "powershell"
                | "python"
                | "javascript"
                | "typescript"
                | "configured"
                | "custom"
                | "remote"
                | "direct_argv"
        )
    }) {
        return Err("job recovery context shell is invalid".to_string());
    }

    let validation_steps = match operation {
        RunnerJobOperation::StartValidation(operation) => {
            let names = operation
                .steps
                .iter()
                .map(|step| step.name.clone())
                .collect::<Vec<_>>();
            if !(1..=3).contains(&operation.steps.len())
                || operation.steps.iter().any(|step| !step.is_canonical())
                || operation.steps.iter().enumerate().any(|(index, step)| {
                    operation.steps[..index]
                        .iter()
                        .any(|earlier| earlier.name == step.name)
                })
            {
                return Err("invalid structured validation plan".to_string());
            }
            names
        }
        _ => Vec::new(),
    };
    if context.validation_steps != validation_steps
        || context
            .validation_steps
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != context.validation_steps.len()
        || context
            .validation_steps
            .iter()
            .any(|step| !matches!(step.as_str(), "format" | "check" | "test"))
    {
        return Err("job recovery context validation_steps are invalid".to_string());
    }
    let validation_context = matches!(operation, RunnerJobOperation::StartValidation(_));
    if context.validation.as_ref().is_some_and(|metadata| {
        !validation_context
            || !metadata.is_valid()
            || metadata
                .steps
                .iter()
                .map(|step| step.name.clone())
                .collect::<Vec<_>>()
                != context.validation_steps
    }) {
        return Err("job recovery context validation metadata is invalid".to_string());
    }
    if context
        .structured_execution
        .as_ref()
        .is_some_and(|metadata| !metadata.is_valid())
    {
        return Err("job recovery context structured execution metadata is invalid".to_string());
    }

    match operation {
        RunnerJobOperation::StartShell(operation) => {
            runner_protocol::validate_raw_shell_wire_command(&operation.command)?;
        }
        RunnerJobOperation::StartProcess(operation)
        | RunnerJobOperation::StartDetachedProcess(operation) => {
            if context.ssh_resource.is_some() {
                return Err("typed process Job request shape is invalid".to_string());
            }
            runner_protocol::validate_process_argv(&operation.process)?;
            validate_runner_structured_common(
                operation.cwd.as_deref(),
                operation.stdin.as_deref(),
                operation.timeout_secs,
                runner_protocol::PROCESS_TIMEOUT_MAX_SECS,
            )?;
        }
        RunnerJobOperation::StartScript(operation) => {
            if context.ssh_resource.is_some() {
                return Err("typed script Job request shape is invalid".to_string());
            }
            runner_protocol::validate_script_request(
                &operation.script,
                operation.stdin.as_deref(),
                operation.cwd.as_deref(),
                operation.timeout_secs,
            )?;
        }
        RunnerJobOperation::StartSkillResource(operation) => {
            if context.ssh_resource.is_some() {
                return Err("typed Skill resource Job request shape is invalid".to_string());
            }
            operation
                .request
                .validate()
                .map_err(|error| format!("invalid Runner Skill execution request: {error}"))?;
            validate_runner_structured_common(
                operation.cwd.as_deref(),
                None,
                operation.timeout_secs,
                runner_protocol::STRUCTURED_EXECUTION_TIMEOUT_MAX_SECS,
            )?;
        }
        RunnerJobOperation::StartValidation(_) => {}
        RunnerJobOperation::Stop { .. } => unreachable!("stop rejected above"),
    }

    if context.structured_execution != operation.expected_structured_execution() {
        return Err(
            "job recovery context structured execution metadata does not match request".to_string(),
        );
    }
    if let Some(project_id) = context.runtime_project_id.as_deref() {
        let prefix = format!("agent:{client_id}:");
        if !bounded(project_id, MAX_CONTEXT_FIELD_CHARS)
            || project_id
                .strip_prefix(&prefix)
                .is_none_or(|suffix| suffix.is_empty())
        {
            return Err(
                "job recovery context runtime_project_id does not match the runner".to_string(),
            );
        }
    }
    if let Some(session_id) = context.workflow_session_id.as_deref() {
        if context.runtime_project_id.is_none()
            || !webcodex_core::workflow_session_contract::is_valid_session_id(session_id)
        {
            return Err("job recovery context workflow_session_id is invalid".to_string());
        }
    }
    Ok(())
}

#[cfg(test)]
fn validate_runner_job_context(
    context: &ShellJobContext,
    request: &RunnerRequest,
    client_id: &str,
) -> Result<(), String> {
    let mut request = request.clone();
    request.job_context = Some(context.clone());
    match request.decode_operation()? {
        runner_operation::RunnerOperation::Job(operation) => {
            validate_runner_job_context_operation(context, &operation, client_id)
        }
        _ => Err("request is not a Runner Job operation".to_string()),
    }
}

fn validate_runner_structured_common(
    cwd: Option<&str>,
    stdin: Option<&str>,
    timeout_secs: u64,
    timeout_max_secs: u64,
) -> Result<(), String> {
    if let Some(stdin) = stdin {
        if stdin.len() > runner_protocol::PROCESS_STDIN_MAX_BYTES {
            return Err(format!(
                "stdin is too large; maximum is {} bytes",
                runner_protocol::PROCESS_STDIN_MAX_BYTES
            ));
        }
        if stdin.contains('\0') {
            return Err("stdin cannot contain NUL bytes".to_string());
        }
    }
    if let Some(cwd) = cwd {
        if cwd.len() > runner_protocol::PROCESS_CWD_MAX_BYTES {
            return Err(format!(
                "cwd is too long; maximum is {} bytes",
                runner_protocol::PROCESS_CWD_MAX_BYTES
            ));
        }
        if cwd.contains('\0') {
            return Err("cwd cannot contain NUL bytes".to_string());
        }
    }
    if !(runner_protocol::STRUCTURED_EXECUTION_TIMEOUT_MIN_SECS..=timeout_max_secs)
        .contains(&timeout_secs)
    {
        return Err(format!(
            "timeout_secs must be between {} and {}",
            runner_protocol::STRUCTURED_EXECUTION_TIMEOUT_MIN_SECS,
            timeout_max_secs
        ));
    }
    Ok(())
}

impl JobManager {
    fn detached_store_for_start(&self, client_id: &str) -> Result<DetachedJobStore, String> {
        #[cfg(test)]
        if let Some(root) = lock_unpoison(&self.detached_store_root_override).clone() {
            return Ok(DetachedJobStore::new(root));
        }
        DetachedJobStore::default_root_for_runner(client_id, &self.detached_profile_server_url)
            .map(DetachedJobStore::new)
    }

    pub(crate) fn install_sink(&self, sink: RunnerSink) {
        *lock_unpoison(&self.current_sink) = Some(sink);
        self.delivery_signal.notify();
    }

    fn current_sink(&self) -> Option<RunnerSink> {
        lock_unpoison(&self.current_sink).clone()
    }

    fn queue_recorded_update(&self, update: RunnerJobUpdateRequest, semantic: bool) {
        let job_id = update.job_id.clone();
        let accepted = lock_unpoison(&self.pending_job_updates)
            .entry(job_id.clone())
            .or_default()
            .enqueue(PendingJobUpdateDelivery::from_update(&update), semantic);
        if !accepted {
            tracing::warn!(
                job_id = %job_id,
                limit = JOB_UPDATE_REQUIRED_PENDING_MAX,
                "runner job live delivery backlog exceeded its semantic bound; waiting for reconciliation"
            );
        }
        self.delivery_signal.notify();
    }

    fn record_update(
        &self,
        job_id: &str,
        mut delta: RunnerJobDelta,
    ) -> Option<(RunnerJobUpdateRequest, bool)> {
        let (update, semantic) = {
            let mut jobs = lock_unpoison(&self.jobs);
            let job = jobs.get_mut(job_id)?;
            if runner_job_is_terminal(&job.snapshot.status) {
                // The first locally observed terminal outcome is immutable.
                // In particular, a racing stop request or late output poll
                // must not revive a handle-free retained record.
                return None;
            }
            let previous_status = job.snapshot.status.clone();
            let previous_progress = job.snapshot.validation_progress.clone();
            let explicit_semantic =
                delta.finished || delta.command_execution_state.is_some() || delta.error.is_some();
            let now = chrono::Utc::now().timestamp();
            append_runner_stream(&mut job.snapshot.stdout, delta.stdout_chunk.as_deref());
            append_runner_stream(&mut job.snapshot.stderr, delta.stderr_chunk.as_deref());
            if let Some(max_bytes) = delta.stream_limit_bytes {
                let max_bytes = max_bytes.min(JOB_SNAPSHOT_STREAM_MAX_BYTES);
                trim_runner_stream_to(&mut job.snapshot.stdout, max_bytes);
                trim_runner_stream_to(&mut job.snapshot.stderr, max_bytes);
            }
            job.snapshot.update_seq = job.snapshot.update_seq.saturating_add(1);
            if !delta.status.trim().is_empty() {
                let incoming_status = delta.status.trim();
                let current_lifecycle = RunnerJobLifecycle::from_wire(&job.snapshot.status).ok();
                let incoming_lifecycle = RunnerJobLifecycle::from_wire(incoming_status).ok();
                let would_regress_stop = current_lifecycle
                    == Some(RunnerJobLifecycle::StopRequested)
                    && matches!(
                        incoming_lifecycle,
                        Some(RunnerJobLifecycle::RunnerQueued | RunnerJobLifecycle::Running)
                    );
                let would_regress_running = current_lifecycle == Some(RunnerJobLifecycle::Running)
                    && incoming_lifecycle == Some(RunnerJobLifecycle::RunnerQueued);
                if !would_regress_stop && !would_regress_running {
                    job.snapshot.status = incoming_status.to_string();
                }
            }
            if delta.command_execution_state.is_some() {
                job.snapshot.command_execution_state = delta.command_execution_state;
            }
            if job.snapshot.started_at.is_none()
                && job.snapshot.command_execution_state
                    != Some(ShellCommandExecutionState::NotStarted)
                && RunnerJobLifecycle::from_wire(&job.snapshot.status).is_ok_and(|lifecycle| {
                    matches!(
                        lifecycle,
                        RunnerJobLifecycle::Running
                            | RunnerJobLifecycle::Completed
                            | RunnerJobLifecycle::Failed
                            | RunnerJobLifecycle::Stopped
                            | RunnerJobLifecycle::Timeout
                            | RunnerJobLifecycle::TimedOut
                            | RunnerJobLifecycle::Cancelled
                    )
                })
            {
                job.snapshot.started_at = Some(now);
            }
            if delta.validation_progress.is_some() {
                job.snapshot.validation_progress = delta.validation_progress.clone();
            }
            if delta.test_count_evidence.is_some() {
                job.snapshot.test_count_evidence = delta.test_count_evidence.clone();
            }
            if let Some(activity) = delta.activity {
                debug_assert!(activity.is_canonical());
                job.snapshot.activity = Some(activity);
            }
            if runner_job_is_terminal(&job.snapshot.status) || delta.finished {
                job.snapshot.activity = None;
                job.snapshot.ended_at.get_or_insert(now);
                job.snapshot.exit_code = delta.exit_code;
                job.snapshot.duration_ms = delta.duration_ms;
                job.snapshot.error = bounded_runner_error(delta.error.take());
                job.child = None;
                job.slot_reserved = false;
            } else if delta.error.is_some() {
                job.snapshot.error = bounded_runner_error(delta.error.take());
            }
            let semantic = explicit_semantic
                || job.snapshot.status != previous_status
                || job.snapshot.validation_progress != previous_progress;
            // Each sequenced update carries the current authoritative bounded
            // tails and activity. Delivery may coalesce output/activity-only
            // attempts, but required semantic markers preserve their sequence
            // while using the latest retained authoritative snapshot at send
            // time.
            (
                job_update_from_snapshot(&job.client_id, &job.runner_instance_id, &job.snapshot),
                semantic,
            )
        };
        self.prune_terminal_records();
        Some((update, semantic))
    }

    fn update_and_send(&self, job_id: &str, delta: RunnerJobDelta) {
        if let Some((update, semantic)) = self.record_update(job_id, delta) {
            self.queue_recorded_update(update, semantic);
        }
    }

    pub(super) fn replay_snapshots_since(&self, registered: &ShellJobInventory) {
        if self.current_sink().is_none() {
            return;
        }
        let registered_by_job = registered
            .jobs
            .iter()
            .map(|snapshot| (snapshot.job_id.as_str(), snapshot))
            .collect::<HashMap<_, _>>();
        let snapshots = self.inventory().jobs;
        let mut pending = lock_unpoison(&self.pending_job_updates);
        let mut remove = Vec::new();
        for snapshot in snapshots {
            let registered_snapshot = registered_by_job.get(snapshot.job_id.as_str()).copied();
            let registered_seq = registered_snapshot.map(|item| item.update_seq).unwrap_or(0);
            let queue = pending.entry(snapshot.job_id.clone()).or_default();
            queue.discard_through(registered_seq);

            if queue.suspended_until_reconciliation {
                if registered_seq >= snapshot.update_seq {
                    queue.suspended_until_reconciliation = false;
                } else {
                    continue;
                }
            }

            if snapshot.update_seq > registered_seq && queue.is_empty() {
                let replay_safe = if snapshot.context.validation_steps.is_empty() {
                    true
                } else {
                    let previous_completed = registered_snapshot
                        .and_then(|item| item.validation_progress.as_ref())
                        .map(|progress| progress.completed)
                        .unwrap_or(0);
                    let current_completed = snapshot
                        .validation_progress
                        .as_ref()
                        .map(|progress| progress.completed)
                        .unwrap_or(0);
                    current_completed <= previous_completed.saturating_add(1)
                };
                if replay_safe {
                    let marker = PendingJobUpdateDelivery {
                        update_seq: snapshot.update_seq,
                        status: snapshot.status.clone(),
                        exit_code: snapshot.exit_code,
                        duration_ms: snapshot.duration_ms,
                        error: snapshot.error.clone(),
                        command_execution_state: snapshot.command_execution_state.clone(),
                        validation_progress: snapshot.validation_progress.clone(),
                        test_count_evidence: snapshot.test_count_evidence.clone(),
                        activity: snapshot.activity,
                        finished: runner_job_is_terminal(&snapshot.status),
                    };
                    let _ = queue.enqueue(marker, true);
                } else {
                    queue.suspended_until_reconciliation = true;
                }
            }
            if queue.is_empty() && !queue.suspended_until_reconciliation {
                remove.push(snapshot.job_id);
            }
        }
        for job_id in remove {
            pending.remove(&job_id);
        }
        drop(pending);
        self.delivery_signal.notify();
    }

    fn resend_snapshot(&self, job_id: &str) {
        let update = lock_unpoison(&self.jobs).get(job_id).map(|job| {
            job_update_from_snapshot(&job.client_id, &job.runner_instance_id, &job.snapshot)
        });
        if let Some(update) = update {
            self.queue_recorded_update(update, true);
        }
    }

    fn fail_job(
        &self,
        operation: &RunnerJobOperation,
        error: String,
        validation_progress: Option<ShellJobValidationProgress>,
    ) {
        let job_id = operation.job_id();
        self.update_and_send(
            job_id,
            RunnerJobDelta {
                status: "failed".to_string(),
                duration_ms: Some(0),
                error: Some(error),
                command_execution_state: job_prestart_lifecycle(operation),
                validation_progress,
                finished: true,
                ..Default::default()
            },
        );
        self.start_available_queued();
    }

    fn prune_terminal_records(&self) {
        let now = chrono::Utc::now().timestamp();
        let removed = {
            let mut jobs = lock_unpoison(&self.jobs);
            let mut removed = jobs
                .iter()
                .filter(|(_, job)| {
                    runner_job_is_terminal(&job.snapshot.status)
                        && job.snapshot.ended_at.is_some_and(|ended| {
                            now.saturating_sub(ended) >= JOB_TERMINAL_RETENTION_SECS
                        })
                })
                .map(|(job_id, _)| job_id.clone())
                .collect::<Vec<_>>();
            for job_id in &removed {
                jobs.remove(job_id);
            }
            let mut terminal = jobs
                .iter()
                .filter(|(_, job)| runner_job_is_terminal(&job.snapshot.status))
                .map(|(job_id, job)| {
                    (
                        job_id.clone(),
                        job.snapshot.ended_at.unwrap_or(job.snapshot.created_at),
                    )
                })
                .collect::<Vec<_>>();
            terminal.sort_by_key(|(_, ended_at)| *ended_at);
            let excess = terminal
                .len()
                .saturating_sub(JOB_INVENTORY_MAX_TERMINAL_JOBS);
            for (job_id, _) in terminal.into_iter().take(excess) {
                jobs.remove(&job_id);
                removed.push(job_id);
            }
            removed
        };
        if !removed.is_empty() {
            let mut pending = lock_unpoison(&self.pending_job_updates);
            let mut detached = lock_unpoison(&self.detached_jobs);
            for job_id in removed {
                pending.remove(&job_id);
                detached.remove(&job_id);
            }
        }
    }

    pub(crate) fn inventory(&self) -> ShellJobInventory {
        self.prune_terminal_records();
        let jobs = lock_unpoison(&self.jobs);
        let mut active = jobs
            .values()
            .filter(|job| runner_job_is_active(&job.snapshot.status))
            .map(|job| job.snapshot.clone())
            .collect::<Vec<_>>();
        let mut terminal = jobs
            .values()
            .filter(|job| runner_job_is_terminal(&job.snapshot.status))
            .map(|job| job.snapshot.clone())
            .collect::<Vec<_>>();
        drop(jobs);
        active.sort_by_key(|snapshot| snapshot.created_at);
        terminal.sort_by(|left, right| {
            right
                .ended_at
                .unwrap_or(right.created_at)
                .cmp(&left.ended_at.unwrap_or(left.created_at))
        });
        terminal.truncate(JOB_INVENTORY_MAX_TERMINAL_JOBS);
        let mut inventory = ShellJobInventory {
            active_complete: true,
            jobs: active,
        };

        // Active records are never omitted. Only when active records alone
        // exceed the frame budget do their authoritative tails shrink.
        let mut tail_limit = JOB_SNAPSHOT_STREAM_MAX_BYTES;
        while serde_json::to_vec(&inventory)
            .map(|bytes| bytes.len() > JOB_INVENTORY_MAX_SERIALIZED_BYTES)
            .unwrap_or(true)
            && tail_limit > 0
        {
            tail_limit /= 2;
            for snapshot in &mut inventory.jobs {
                trim_runner_stream_to(&mut snapshot.stdout, tail_limit);
                trim_runner_stream_to(&mut snapshot.stderr, tail_limit);
            }
        }

        // Add newest terminal history only while it fits. Serializing each
        // record once avoids repeatedly encoding a multi-megabyte inventory
        // while preserving the newest-first eviction rule.
        let mut serialized_len = serde_json::to_vec(&inventory)
            .map(|bytes| bytes.len())
            .unwrap_or(JOB_INVENTORY_MAX_SERIALIZED_BYTES.saturating_add(1));
        for snapshot in terminal {
            let Ok(encoded) = serde_json::to_vec(&snapshot) else {
                continue;
            };
            let separator = usize::from(!inventory.jobs.is_empty());
            let added = encoded.len().saturating_add(separator);
            if serialized_len.saturating_add(added) > JOB_INVENTORY_MAX_SERIALIZED_BYTES {
                break;
            }
            serialized_len = serialized_len.saturating_add(added);
            inventory.jobs.push(snapshot);
        }
        inventory
    }

    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub(super) fn recover_detached_jobs(
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
    fn spawn_detached_observer(
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
                    let terminal = record.phase == super::detached_job::DetachedJobPhase::Terminal;
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

    fn sync_detached_record(
        &self,
        job_id: &str,
        record: &super::detached_job::DetachedJobRecord,
    ) -> Result<bool, String> {
        let snapshot = snapshot_from_detached_record(record)?;
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
        if terminal {
            lock_unpoison(&self.detached_jobs).remove(job_id);
            self.start_available_queued();
        }
        Ok(terminal)
    }

    pub(super) fn has_work(&self) -> bool {
        let detached_ids = lock_unpoison(&self.detached_jobs)
            .keys()
            .cloned()
            .collect::<HashSet<_>>();
        lock_unpoison(&self.jobs).iter().any(|(job_id, job)| {
            runner_job_is_active(&job.snapshot.status) && !detached_ids.contains(job_id.as_str())
        }) || !lock_unpoison(&self.queued).is_empty()
    }

    pub(super) fn stop_accepting_work(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
    }

    pub(super) fn cancel_queued_for_shutdown(&self) -> usize {
        let _lifecycle = lock_unpoison(&self.lifecycle);
        self.shutting_down.store(true, Ordering::SeqCst);
        let mut queued = lock_unpoison(&self.queued);
        let cancelled = queued.len();
        queued.clear();
        cancelled
    }

    pub(super) fn signal_all_for_shutdown(&self) -> JobShutdownBatch {
        let detached_ids = lock_unpoison(&self.detached_jobs)
            .keys()
            .cloned()
            .collect::<HashSet<_>>();
        let running = {
            let jobs = lock_unpoison(&self.jobs);
            jobs.iter()
                .filter(|(job_id, job)| {
                    runner_job_is_active(&job.snapshot.status)
                        && !detached_ids.contains(job_id.as_str())
                })
                .map(|(_, job)| (job.child.clone(), Arc::clone(&job.stop_requested)))
                .collect::<Vec<_>>()
        };
        let running_count = running.len();
        let mut targets = Vec::with_capacity(running.len());
        let mut failures = 0;
        for (child, stop_requested) in running {
            stop_requested.store(true, Ordering::SeqCst);
            let Some(child) = child else {
                continue;
            };
            // Graceful tree termination where supported (SIGTERM on Unix);
            // Windows has no graceful Job Object signal and escalates to a
            // force terminate immediately.
            if request_terminate_managed_tree(&child).is_err() {
                failures += 1;
            }
            targets.push(JobShutdownTarget { child });
        }
        JobShutdownBatch {
            running: running_count,
            targets,
            failures,
        }
    }

    pub(super) fn drain_shutdown(
        &self,
        mut batch: JobShutdownBatch,
        deadline: Instant,
    ) -> JobShutdownOutcome {
        const TERM_GRACE: Duration = Duration::from_millis(500);
        let resources = batch.targets.len();
        let grace_deadline = deadline.min(Instant::now() + TERM_GRACE);
        // Phase 1: after the graceful request, wait up to the grace window for
        // each managed tree to empty on its own.
        while Instant::now() < grace_deadline {
            if batch
                .targets
                .iter_mut()
                .all(|target| !shutdown_target_running(target))
            {
                break;
            }
            let remaining = grace_deadline.saturating_duration_since(Instant::now());
            std::thread::sleep(Duration::from_millis(10).min(remaining));
        }

        // Phase 2: force-terminate every tree that is still alive.
        for target in &mut batch.targets {
            if managed_tree_running(&target.child) && terminate_managed_tree(&target.child).is_err()
            {
                batch.failures += 1;
            }
        }

        // Phase 3: wait out the remaining budget for all trees to empty.
        while Instant::now() < deadline {
            if batch
                .targets
                .iter_mut()
                .all(|target| !shutdown_target_running(target))
            {
                break;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            std::thread::sleep(Duration::from_millis(10).min(remaining));
        }
        let mut timed_out = 0;
        for target in &mut batch.targets {
            timed_out += usize::from(shutdown_target_running(target));
        }
        JobShutdownOutcome {
            resources,
            timed_out,
            failures: batch.failures,
        }
    }

    #[cfg(test)]
    fn stop_all(&self) {
        self.stop_accepting_work();
        self.cancel_queued_for_shutdown();
        let batch = self.signal_all_for_shutdown();
        let outcome = self.drain_shutdown(batch, Instant::now() + Duration::from_secs(2));
        if outcome.timed_out > 0 || outcome.failures > 0 {
            eprintln!(
                "webcodex-runner shutdown job cleanup incomplete resources={} timed_out={} failures={}",
                outcome.resources, outcome.timed_out, outcome.failures
            );
        }
    }

    pub(super) fn wait_for_workers(&self, deadline: Instant) -> bool {
        self.workers.wait_until(deadline)
    }

    pub(super) fn worker_count(&self) -> usize {
        self.workers.active()
    }

    fn shutdown_rejection(&self, operation: &RunnerJobOperation) {
        self.fail_job(operation, "runner is shutting down".to_string(), None);
    }

    pub(super) fn enqueue(&self, sink: RunnerSink, start: PendingJobStart) {
        if !start.operation.is_start() {
            return;
        }
        let job_id = start.operation.job_id().to_string();
        let Some(context) = start.operation.context().cloned() else {
            return;
        };
        if let Err(error) =
            validate_runner_job_context_operation(&context, &start.operation, sink.client_id())
        {
            let command_execution_state = job_prestart_lifecycle(&start.operation);
            let _ = sink.send_job_update(&RunnerJobUpdateRequest {
                client_id: sink.client_id().to_string(),
                runner_instance_id: sink.runner_instance_id().to_string(),
                job_id,
                request_id: Some(start.metadata.request_id.clone()),
                update_seq: Some(1),
                status: "failed".to_string(),
                stdout_chunk: None,
                stderr_chunk: None,
                log_snapshot: None,
                exit_code: None,
                duration_ms: Some(0),
                error: Some(error),
                command_execution_state,
                validation_progress: None,
                test_count_evidence: None,
                activity: None,
                finished: true,
            });
            return;
        }
        self.install_sink(sink.clone());
        let client_id = sink.client_id().to_string();
        let runner_instance_id = sink.runner_instance_id().to_string();
        let (queue_locally, immediate_failure) = {
            let _lifecycle = lock_unpoison(&self.lifecycle);
            let shutting_down = self.shutting_down.load(Ordering::SeqCst);
            let mut jobs = lock_unpoison(&self.jobs);
            if jobs.contains_key(&job_id) {
                return;
            }
            let active_count = jobs
                .values()
                .filter(|job| runner_job_is_active(&job.snapshot.status))
                .count();
            let reserved = jobs
                .values()
                .filter(|job| {
                    job.client_id == client_id
                        && job.slot_reserved
                        && runner_job_is_active(&job.snapshot.status)
                })
                .count();
            let inventory_full = active_count >= JOB_INVENTORY_MAX_ACTIVE_JOBS;
            let immediate_failure = if inventory_full {
                Some(format!(
                    "runner active job inventory limit reached ({})",
                    JOB_INVENTORY_MAX_ACTIVE_JOBS
                ))
            } else if shutting_down {
                Some("runner is shutting down".to_string())
            } else {
                None
            };
            let queue_locally = immediate_failure.is_none() && reserved >= self.max_concurrent;
            let slot_reserved = immediate_failure.is_none() && !queue_locally;
            let now = chrono::Utc::now().timestamp();
            let terminal = immediate_failure.is_some();
            jobs.insert(
                job_id.clone(),
                RunningJob {
                    client_id: client_id.clone(),
                    runner_instance_id,
                    snapshot: ShellJobSnapshot {
                        job_id: job_id.clone(),
                        request_id: start.metadata.request_id.clone(),
                        status: if terminal {
                            "failed".to_string()
                        } else {
                            "agent_queued".to_string()
                        },
                        update_seq: u64::from(terminal),
                        created_at: start.metadata.created_at,
                        started_at: None,
                        ended_at: terminal.then_some(now),
                        exit_code: None,
                        duration_ms: terminal.then_some(0),
                        error: immediate_failure.clone(),
                        command_execution_state: terminal
                            .then(|| job_prestart_lifecycle(&start.operation))
                            .flatten(),
                        context,
                        stdout: ShellJobStreamSnapshot::default(),
                        stderr: ShellJobStreamSnapshot::default(),
                        validation_progress: None,
                        test_count_evidence: None,
                        activity: None,
                    },
                    child: None,
                    stop_requested: Arc::new(AtomicBool::new(false)),
                    slot_reserved,
                },
            );
            drop(jobs);
            if queue_locally {
                lock_unpoison(&self.queued).push_back(start.clone());
            }
            (queue_locally, immediate_failure)
        };
        if let Some(error) = immediate_failure {
            debug_assert!(!error.is_empty());
            self.resend_snapshot(&job_id);
            self.prune_terminal_records();
            return;
        }
        self.update_and_send(
            &job_id,
            RunnerJobDelta {
                status: "agent_queued".to_string(),
                ..Default::default()
            },
        );
        if queue_locally {
            return;
        }
        self.start_now(start);
    }

    fn start_now(&self, start: PendingJobStart) {
        if self.shutting_down.load(Ordering::SeqCst) {
            self.shutdown_rejection(&start.operation);
            return;
        }
        match &start.operation {
            RunnerJobOperation::StartDetachedProcess(_) => self.start_detached_process_job(start),
            RunnerJobOperation::StartProcess(_)
            | RunnerJobOperation::StartScript(_)
            | RunnerJobOperation::StartSkillResource(_) => self.start_structured_job(start),
            RunnerJobOperation::StartShell(_) | RunnerJobOperation::StartValidation(_) => {
                self.start_shell_job(start)
            }
            RunnerJobOperation::Stop { .. } => {
                unreachable!("stop Job operation cannot enter the start queue")
            }
        }
    }

    fn start_available_queued(&self) {
        loop {
            if self.shutting_down.load(Ordering::SeqCst) {
                lock_unpoison(&self.queued).clear();
                return;
            }
            let next = {
                let _lifecycle = lock_unpoison(&self.lifecycle);
                if self.shutting_down.load(Ordering::SeqCst) {
                    lock_unpoison(&self.queued).clear();
                    return;
                }
                let mut jobs = lock_unpoison(&self.jobs);
                let mut queued = lock_unpoison(&self.queued);
                let mut selected = None;
                for (idx, queued_start) in queued.iter().enumerate() {
                    let reserved = jobs
                        .values()
                        .filter(|job| {
                            job.client_id == queued_start.metadata.client_id
                                && job.slot_reserved
                                && runner_job_is_active(&job.snapshot.status)
                        })
                        .count();
                    if reserved < self.max_concurrent {
                        selected = Some(idx);
                        break;
                    }
                }
                if let Some(idx) = selected {
                    let job_id = queued[idx].operation.job_id();
                    if let Some(job) = jobs.get_mut(job_id) {
                        job.slot_reserved = true;
                    }
                    queued.remove(idx)
                } else {
                    None
                }
            };
            let Some(start) = next else {
                return;
            };
            self.start_now(start);
        }
    }

    fn start_detached_process_job(&self, start: PendingJobStart) {
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

    fn start_structured_job(&self, start: PendingJobStart) {
        let PendingJobStart {
            generation,
            policy,
            shell,
            skills,
            client_id,
            server_url,
            project_registry_dir,
            operation,
            ..
        } = start;
        let job_id = operation.job_id().to_string();
        if !matches!(
            operation,
            RunnerJobOperation::StartProcess(_)
                | RunnerJobOperation::StartScript(_)
                | RunnerJobOperation::StartSkillResource(_)
        ) {
            unreachable!("structured Job starter received non structured operation");
        }
        let stop_requested = {
            let _lifecycle = lock_unpoison(&self.lifecycle);
            if self.shutting_down.load(Ordering::SeqCst) {
                None
            } else {
                let mut jobs = lock_unpoison(&self.jobs);
                let Some(job) = jobs.get_mut(&job_id) else {
                    return;
                };
                job.slot_reserved = true;
                Some(Arc::clone(&job.stop_requested))
            }
        };
        let Some(stop_requested) = stop_requested else {
            self.shutdown_rejection(&operation);
            return;
        };
        let manager = self.clone_for_worker();
        let worker_guard = self.workers.enter();
        std::thread::spawn(move || {
            let _worker_guard = worker_guard;
            let started_manager = manager.clone_for_worker();
            let started_job_id = job_id.clone();
            let on_started = || {
                started_manager.update_and_send(
                    &started_job_id,
                    RunnerJobDelta {
                        status: "running".to_string(),
                        activity: Some(process_running_activity()),
                        ..Default::default()
                    },
                );
            };
            let result = match &operation {
                RunnerJobOperation::StartProcess(request) => {
                    run_process_with_profiles_and_execution_state_with_start_hook(
                        generation,
                        &policy,
                        &shell,
                        &project_registry_dir,
                        &manager.prepared_profiles,
                        request.cwd.as_deref(),
                        &request.process.executable,
                        &request.process.args,
                        request.stdin.as_deref(),
                        request.timeout_secs,
                        Some(stop_requested.as_ref()),
                        Some(&on_started),
                    )
                }
                RunnerJobOperation::StartScript(request) => {
                    run_script_with_profiles_and_execution_state_with_start_hook(
                        generation,
                        &policy,
                        &shell,
                        &project_registry_dir,
                        &manager.prepared_profiles,
                        request.cwd.as_deref(),
                        &request.script,
                        request.stdin.as_deref(),
                        request.timeout_secs,
                        Some(stop_requested.as_ref()),
                        Some(&on_started),
                    )
                }
                RunnerJobOperation::StartSkillResource(request) => {
                    run_skill_resource_with_profiles_and_execution_state(
                        generation,
                        &skills,
                        &client_id,
                        &server_url,
                        &policy,
                        &shell,
                        &project_registry_dir,
                        &manager.prepared_profiles,
                        request.cwd.as_deref(),
                        &request.request,
                        request.timeout_secs,
                        Some(stop_requested.as_ref()),
                        Some(&on_started),
                    )
                }
                _ => unreachable!("structured Job starter received non structured operation"),
            };
            let execution_state = result.execution_state;
            let stopped = stop_requested.load(Ordering::SeqCst)
                && execution_state == ShellCommandExecutionState::Completed;
            let status = match execution_state {
                ShellCommandExecutionState::NotStarted => "failed",
                ShellCommandExecutionState::OutcomeUnknown => "lost",
                ShellCommandExecutionState::TimedOut => "timeout",
                ShellCommandExecutionState::Completed if stopped => "stopped",
                ShellCommandExecutionState::Completed
                    if result.result.exit_code == Some(0) && result.result.error.is_none() =>
                {
                    "completed"
                }
                ShellCommandExecutionState::Completed => "failed",
            };
            manager.update_and_send(
                &job_id,
                RunnerJobDelta {
                    status: status.to_string(),
                    stdout_chunk: result.result.stdout,
                    stderr_chunk: result.result.stderr,
                    exit_code: result.result.exit_code,
                    duration_ms: result.result.duration_ms,
                    error: result.result.error,
                    command_execution_state: Some(execution_state),
                    finished: true,
                    ..Default::default()
                },
            );
            manager.start_available_queued();
        });
    }

    fn start_shell_job(&self, start: PendingJobStart) {
        let PendingJobStart {
            generation,
            policy,
            shell,
            ssh,
            project_registry_dir,
            metadata: _,
            operation,
            ..
        } = start;
        let (
            job_id,
            cwd,
            raw_command,
            explicit_shell,
            login,
            steps,
            timeout_secs,
            context,
            validation,
        ) = match &operation {
            RunnerJobOperation::StartShell(request) => (
                request.job_id.clone(),
                request.cwd.clone(),
                Some(request.command.clone()),
                request.shell,
                request.login,
                Vec::new(),
                request.timeout_secs,
                request.context.clone(),
                false,
            ),
            RunnerJobOperation::StartValidation(request) => (
                request.job_id.clone(),
                request.cwd.clone(),
                None,
                None,
                false,
                request.steps.clone(),
                request.timeout_secs,
                request.context.clone(),
                true,
            ),
            _ => unreachable!("shell Job starter received non shell/validation operation"),
        };
        let capture_cargo_test_count = context.validation.as_ref().is_some_and(|metadata| {
            metadata.tool == "cargo_test"
                && metadata.kind == "test"
                && metadata.no_run != Some(true)
        });
        if login && explicit_shell != Some(ExecutionShell::Bash) {
            self.fail_job(
                &operation,
                "bash login mode requires shell=bash".to_string(),
                None,
            );
            return;
        }
        if !policy.allow_raw_shell {
            self.fail_job(
                &operation,
                "raw shell is disabled by local Runner policy".to_string(),
                None,
            );
            return;
        }
        if context.ssh_resource.is_some() {
            self.start_ssh_shell_job(generation, policy, ssh, operation);
            return;
        }
        let cwd_path = cwd
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
        if let Err(e) = cwd_allowed(&policy, &cwd_path) {
            self.fail_job(&operation, e, None);
            return;
        }
        let prepared_profile = match resolve_prepared_shell_profile(
            generation,
            &shell,
            &project_registry_dir,
            &cwd_path,
            cwd.is_some(),
            &self.prepared_profiles,
            Some(self.shutting_down.as_ref()),
        ) {
            Ok(profile) => profile,
            Err(e) => {
                self.fail_job(&operation, e, None);
                return;
            }
        };
        if validation
            && steps.iter().any(|step| {
                !validation_module_available(
                    &shell,
                    prepared_profile.as_deref(),
                    &cwd_path,
                    step,
                    Some(self.shutting_down.as_ref()),
                )
            })
        {
            self.fail_job(
                &operation,
                VALIDATION_TOOL_UNAVAILABLE_CODE.to_string(),
                Some(ShellJobValidationProgress {
                    completed: 0,
                    current_step: None,
                    failed_step: None,
                }),
            );
            return;
        }
        let step_count = if validation { steps.len() } else { 1 };
        let mut commands = VecDeque::with_capacity(step_count);
        for index in 0..step_count {
            let configured = if validation {
                configured_validation_job_command(
                    &shell,
                    prepared_profile.as_deref(),
                    &steps[index].program,
                    &steps[index].args,
                    &cwd_path,
                )
            } else {
                let raw_command = raw_command
                    .as_deref()
                    .expect("typed raw shell Job carries command text");
                match explicit_shell {
                    Some(selection) => configured_explicit_shell_command(
                        &shell,
                        prepared_profile.as_deref(),
                        selection,
                        login,
                        raw_command,
                    ),
                    None => match prepared_profile.as_deref() {
                        Some(profile) => {
                            configured_prepared_shell_job_command(profile, raw_command)
                        }
                        None => configured_shell_job_command(&shell, raw_command),
                    },
                }
            };
            let mut command = match configured {
                Ok(command) => command,
                Err(error) => {
                    self.fail_job(&operation, error, None);
                    return;
                }
            };
            if validation {
                command.envs(
                    steps[index]
                        .env
                        .iter()
                        .map(|(key, value)| (key.as_str(), value.as_str())),
                );
            }
            // Raw Shell Jobs and every validation step have no stdin payload.
            // Never inherit the Runner's parent-liveness pipe: it stays open
            // while Desktop is alive and can stall native commands or let a
            // child consume input owned by the Runner. Match the sync path.
            command
                .current_dir(&cwd_path)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            commands.push_back(command);
        }
        let stop_requested = {
            let _lifecycle = lock_unpoison(&self.lifecycle);
            if self.shutting_down.load(Ordering::SeqCst) {
                None
            } else {
                let mut jobs = lock_unpoison(&self.jobs);
                let Some(job) = jobs.get_mut(&job_id) else {
                    return;
                };
                job.slot_reserved = true;
                Some(Arc::clone(&job.stop_requested))
            }
        };
        let Some(stop_requested) = stop_requested else {
            self.shutdown_rejection(&operation);
            return;
        };
        // Preserve the pre-start proof boundary explicitly. A stop/shutdown
        // observed here is still known to precede ManagedChild::spawn; the
        // fence below is intentionally repeated after spawn because that later
        // race can no longer claim NotStarted.
        if stop_requested.load(Ordering::SeqCst) {
            self.fail_job(&operation, "job stopped before start".to_string(), None);
            return;
        }
        if self.shutting_down.load(Ordering::SeqCst) {
            self.shutdown_rejection(&operation);
            return;
        }
        let start = Instant::now();
        let mut command = commands.pop_front().expect("validated non-empty plan");
        let spawn = ManagedChild::spawn(&mut command);
        let mut child = match spawn {
            Ok(child) => child,
            Err(e) => {
                if validation {
                    self.fail_job(
                        &operation,
                        VALIDATION_STEP_SPAWN_FAILED_CODE.to_string(),
                        Some(ShellJobValidationProgress {
                            completed: 0,
                            current_step: None,
                            failed_step: None,
                        }),
                    );
                } else {
                    let error = prepared_profile
                        .as_ref()
                        .map(|profile_name| {
                            format!(
                                "failed to spawn shell profile '{}': {}",
                                profile_name.profile_name, e
                            )
                        })
                        .unwrap_or_else(|| format!("failed to spawn command: {}", e));
                    self.fail_job(&operation, error, None);
                }
                return;
            }
        };
        let mut stdout = child.child_mut().stdout.take();
        let mut stderr = child.child_mut().stderr.take();
        let mut child = Arc::new(Mutex::new(child));
        let post_spawn_rejection = {
            let _lifecycle = lock_unpoison(&self.lifecycle);
            let mut jobs = lock_unpoison(&self.jobs);
            let mut job = jobs.get_mut(&job_id);
            let rejection = post_spawn_interruption_reason(
                self.shutting_down.load(Ordering::SeqCst),
                stop_requested.load(Ordering::SeqCst),
                job.is_some(),
            );
            if rejection.is_none() {
                if let Some(job) = job.as_mut() {
                    job.child = Some(child.clone());
                }
            }
            rejection
        };
        if let Some(error) = post_spawn_rejection {
            let _ = terminate_managed_tree(&child);
            // ManagedChild::spawn succeeded before this fence. Even if the
            // termination request succeeds, the user command may already have
            // executed and we do not wait here for a trustworthy exit status.
            // Raw shell must therefore remain conservatively started/unknown;
            // never recycle the pre-start NotStarted evidence across this
            // spawn boundary.
            self.update_and_send(
                &job_id,
                post_spawn_interruption_delta(
                    &operation,
                    start.elapsed().as_millis() as u64,
                    error,
                ),
            );
            self.start_available_queued();
            return;
        }
        self.update_and_send(
            &job_id,
            RunnerJobDelta {
                status: "running".to_string(),
                validation_progress: validation.then(|| ShellJobValidationProgress {
                    completed: 0,
                    current_step: Some(steps[0].name.clone()),
                    failed_step: None,
                }),
                activity: Some(if validation {
                    validation_step_activity(&steps[0])
                } else {
                    process_running_activity()
                }),
                ..Default::default()
            },
        );
        let jobs = self.jobs.clone();
        let lifecycle = Arc::clone(&self.lifecycle);
        let shutting_down = Arc::clone(&self.shutting_down);
        let manager = self.clone_for_worker();
        let worker_guard = self.workers.enter();
        std::thread::spawn(move || {
            let _worker_guard = worker_guard;
            let timeout_secs = timeout_secs.min(policy.max_timeout_secs).max(1);
            let mut step_index = 0;
            let mut test_count_accumulator = capture_cargo_test_count
                .then(webcodex_core::cargo_test_count::CargoTestRunMetadataAccumulator::default);
            let (final_status, out, err, final_progress) = loop {
                const OUTPUT_CHANNEL_CAPACITY: usize = 64;
                let (tx, rx) = mpsc::sync_channel::<OutputChunk>(OUTPUT_CHANNEL_CAPACITY);
                let mut readers = Vec::new();
                if let Some(stdout) = stdout {
                    readers.push(spawn_reader(
                        stdout,
                        tx.clone(),
                        true,
                        OutputTextSource::LocalProcess,
                    ));
                }
                if let Some(stderr) = stderr {
                    readers.push(spawn_reader(
                        stderr,
                        tx.clone(),
                        false,
                        OutputTextSource::LocalProcess,
                    ));
                }
                drop(tx);
                let step_status = loop {
                    let mut out = String::new();
                    let mut err = String::new();
                    while let Ok(chunk) = rx.try_recv() {
                        match chunk {
                            OutputChunk::Stdout(text) => out.push_str(&text),
                            OutputChunk::Stderr(text) => err.push_str(&text),
                        }
                    }
                    if !out.is_empty() || !err.is_empty() {
                        observe_cargo_test_count_chunks(&mut test_count_accumulator, &out, &err);
                        let activity = validation
                            .then(|| cargo_activity_from_stderr(&steps[step_index], &err))
                            .flatten();
                        manager.update_and_send(
                            &job_id,
                            RunnerJobDelta {
                                status: "running".to_string(),
                                stdout_chunk: (!out.is_empty()).then_some(out),
                                stderr_chunk: (!err.is_empty()).then_some(err),
                                validation_progress: validation.then(|| {
                                    ShellJobValidationProgress {
                                        completed: step_index,
                                        current_step: Some(steps[step_index].name.clone()),
                                        failed_step: None,
                                    }
                                }),
                                activity,
                                ..Default::default()
                            },
                        );
                    }
                    let wait_result = {
                        let mut child = lock_unpoison(&child);
                        child.try_wait()
                    };
                    match wait_result {
                        Ok(Some(status)) => {
                            let stopped = stop_requested.load(Ordering::SeqCst);
                            break (
                                if stopped {
                                    "stopped"
                                } else if status.success() {
                                    "completed"
                                } else {
                                    "failed"
                                }
                                .to_string(),
                                Some(status.code().unwrap_or(-1)),
                                if stopped {
                                    Some("job stopped by request".to_string())
                                } else {
                                    None
                                },
                            );
                        }
                        Ok(None) => {
                            if stop_requested.load(Ordering::SeqCst) {
                                let _ = terminate_managed_tree(&child);
                                break (
                                    "stopped".to_string(),
                                    Some(-1),
                                    Some("job stopped by request".to_string()),
                                );
                            }
                            if start.elapsed() >= Duration::from_secs(timeout_secs) {
                                stop_requested.store(true, Ordering::SeqCst);
                                let _ = terminate_managed_tree(&child);
                                break (
                                    "timeout".to_string(),
                                    Some(-1),
                                    Some(format!("job timed out after {} seconds", timeout_secs)),
                                );
                            }
                        }
                        Err(e) => {
                            // The host lost track of a process it started.
                            // For a validation job that must arrive as a
                            // machine-readable infrastructure code: the step
                            // did not fail, its outcome is simply unknown,
                            // and saying "check failed" would blame the
                            // project for the executor's problem.
                            eprintln!("webcodex-runner failed to wait job {job_id}: {e}");
                            break (
                                "failed".to_string(),
                                None,
                                Some(wait_failure_error(validation, &e)),
                            );
                        }
                    }
                    std::thread::sleep(Duration::from_millis(JOB_UPDATE_INTERVAL_MS));
                };
                // A direct child can exit while a background descendant keeps
                // stdout/stderr open. Give the tree a short window to exit on
                // its own, then force-terminate whatever remains before the
                // bounded reader join, so cleanup cannot wait forever on EOF.
                cleanup_managed_tree(&child);
                let mut out = String::new();
                let mut err = String::new();
                drain_and_join_reader_threads_until(
                    readers,
                    &rx,
                    &mut out,
                    &mut err,
                    Instant::now() + Duration::from_secs(1),
                );
                observe_cargo_test_count_chunks(&mut test_count_accumulator, &out, &err);
                if step_status.0 == "completed" && step_index + 1 < step_count {
                    step_index += 1;
                    if stop_requested.load(Ordering::SeqCst) {
                        break (
                            (
                                "stopped".to_string(),
                                Some(-1),
                                Some("job stopped by request".to_string()),
                            ),
                            out,
                            err,
                            validation.then_some(ShellJobValidationProgress {
                                completed: step_index,
                                current_step: None,
                                failed_step: None,
                            }),
                        );
                    }
                    {
                        let _lifecycle_guard = lock_unpoison(&lifecycle);
                        if shutting_down.load(Ordering::SeqCst)
                            || stop_requested.load(Ordering::SeqCst)
                        {
                            break (
                                (
                                    "stopped".to_string(),
                                    Some(-1),
                                    Some("job stopped by request".to_string()),
                                ),
                                out,
                                err,
                                validation.then_some(ShellJobValidationProgress {
                                    completed: step_index,
                                    current_step: None,
                                    failed_step: None,
                                }),
                            );
                        }
                    }
                    let mut next_command = commands
                        .pop_front()
                        .expect("one command per validation step");
                    let spawn = ManagedChild::spawn(&mut next_command);
                    let mut next = match spawn {
                        Ok(child) => child,
                        Err(_error) => {
                            break (
                                (
                                    "failed".to_string(),
                                    None,
                                    Some(VALIDATION_STEP_SPAWN_FAILED_CODE.to_string()),
                                ),
                                out,
                                err,
                                validation.then_some(ShellJobValidationProgress {
                                    completed: step_index,
                                    current_step: None,
                                    failed_step: None,
                                }),
                            )
                        }
                    };
                    let next_stdout = next.child_mut().stdout.take();
                    let next_stderr = next.child_mut().stderr.take();
                    let next = Arc::new(Mutex::new(next));
                    let reject_for_shutdown = {
                        let _lifecycle_guard = lock_unpoison(&lifecycle);
                        if shutting_down.load(Ordering::SeqCst)
                            || stop_requested.load(Ordering::SeqCst)
                        {
                            true
                        } else if let Some(job) = lock_unpoison(&jobs).get_mut(&job_id) {
                            job.child = Some(Arc::clone(&next));
                            false
                        } else {
                            true
                        }
                    };
                    if reject_for_shutdown {
                        let _ = terminate_managed_tree(&next);
                        break (
                            (
                                "stopped".to_string(),
                                Some(-1),
                                Some("job stopped by request".to_string()),
                            ),
                            out,
                            err,
                            validation.then_some(ShellJobValidationProgress {
                                completed: step_index,
                                current_step: None,
                                failed_step: None,
                            }),
                        );
                    }
                    child = next;
                    manager.update_and_send(
                        &job_id,
                        RunnerJobDelta {
                            status: "running".to_string(),
                            stdout_chunk: (!out.is_empty()).then_some(out),
                            stderr_chunk: (!err.is_empty()).then_some(err),
                            validation_progress: validation.then(|| ShellJobValidationProgress {
                                completed: step_index,
                                current_step: Some(steps[step_index].name.clone()),
                                failed_step: None,
                            }),
                            activity: validation
                                .then(|| validation_step_activity(&steps[step_index])),
                            ..Default::default()
                        },
                    );
                    stdout = next_stdout;
                    stderr = next_stderr;
                    continue;
                }
                let progress = validation.then(|| ShellJobValidationProgress {
                    completed: if step_status.0 == "completed" {
                        steps.len()
                    } else {
                        step_index
                    },
                    current_step: None,
                    // An infrastructure code names no failed step: the
                    // connector reads `failed_step` as "this check rejected
                    // the work", which is exactly what did not happen.
                    failed_step: validation_failed_step(
                        &step_status.0,
                        step_status.2.as_deref(),
                        &steps[step_index].name,
                    ),
                });
                break (step_status, out, err, progress);
            };
            let command_execution_state = (!validation)
                .then(|| raw_shell_job_terminal_lifecycle(&final_status.0, final_status.1));
            let test_count_evidence = finish_cargo_test_count_evidence(test_count_accumulator);
            manager.update_and_send(
                &job_id,
                RunnerJobDelta {
                    status: final_status.0,
                    stdout_chunk: (!out.is_empty()).then_some(out),
                    stderr_chunk: (!err.is_empty()).then_some(err),
                    exit_code: final_status.1,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: final_status.2,
                    command_execution_state,
                    stream_limit_bytes: None,
                    validation_progress: final_progress,
                    test_count_evidence,
                    activity: None,
                    finished: true,
                },
            );
            manager.start_available_queued();
        });
    }

    /// Start one remote SSH command as a normal runner job. Resource/session
    /// validation and local command preparation happen before spawn, so those
    /// failures are unambiguously command-not-started. Once `ssh` is running,
    /// never retry because remote delivery may already have happened.
    fn start_ssh_shell_job(
        &self,
        generation: u64,
        policy: RunnerPolicy,
        ssh: SshConfig,
        operation: RunnerJobOperation,
    ) {
        let request = match &operation {
            RunnerJobOperation::StartShell(request) => request,
            RunnerJobOperation::StartValidation(_) => {
                self.fail_job(
                    &operation,
                    "ssh_resource_unsupported_for_request: SSH resources do not support structured validation jobs; command was not started".to_string(),
                    None,
                );
                return;
            }
            _ => unreachable!("SSH Job starter received non shell/validation operation"),
        };
        let job_id = request.job_id.clone();
        let Some(resource_name) = request.context.ssh_resource.as_deref() else {
            return;
        };
        let Some(session_id) = request.context.workflow_session_id.as_deref() else {
            self.fail_job(
                &operation,
                "ssh_session_required: an SSH resource requires a Workflow Session id; command was not started".to_string(),
                None,
            );
            return;
        };
        let prepared = match self.ssh_pool.prepare_job_command(
            generation,
            &ssh,
            resource_name,
            session_id,
            request.cwd.as_deref(),
            &request.command,
        ) {
            Ok(prepared) => prepared,
            Err(error) => {
                self.fail_job(&operation, error, None);
                return;
            }
        };
        let transport = prepared.transport.clone();
        let program_delivery = prepared.program_delivery;
        let mut command = prepared.command;
        if program_delivery.requires_stdin() {
            command.stdin(Stdio::piped());
        } else {
            command.stdin(Stdio::null());
        }
        command.stdout(Stdio::piped()).stderr(Stdio::piped());

        let stop_requested = {
            let _lifecycle = lock_unpoison(&self.lifecycle);
            if self.shutting_down.load(Ordering::SeqCst) {
                None
            } else {
                let mut jobs = lock_unpoison(&self.jobs);
                let Some(job) = jobs.get_mut(&job_id) else {
                    return;
                };
                job.slot_reserved = true;
                Some(Arc::clone(&job.stop_requested))
            }
        };
        let Some(stop_requested) = stop_requested else {
            self.shutdown_rejection(&operation);
            return;
        };
        let start = Instant::now();
        let spawn = ManagedChild::spawn(&mut command);
        let mut child = match spawn {
            Ok(child) => child,
            Err(error) => {
                self.fail_job(
                    &operation,
                    format!(
                        "ssh_command_spawn_failed: could not start local ssh client: {error}; command was not started"
                    ),
                    None,
                );
                return;
            }
        };
        let mut child_stdin = child.child_mut().stdin.take();
        let mut stdout = child.child_mut().stdout.take();
        let mut stderr = child.child_mut().stderr.take();
        let child = Arc::new(Mutex::new(child));
        let post_spawn_rejection = {
            let _lifecycle = lock_unpoison(&self.lifecycle);
            let mut jobs = lock_unpoison(&self.jobs);
            let mut job = jobs.get_mut(&job_id);
            let rejection = post_spawn_interruption_reason(
                self.shutting_down.load(Ordering::SeqCst),
                stop_requested.load(Ordering::SeqCst),
                job.is_some(),
            );
            if rejection.is_none() {
                if let Some(job) = job.as_mut() {
                    job.child = Some(Arc::clone(&child));
                }
            }
            rejection
        };
        if let Some(error) = post_spawn_rejection {
            let _ = terminate_managed_tree(&child);
            // ManagedChild::spawn has already resumed ssh.exe. A successful
            // local tree termination cannot prove that the remote command was
            // never dispatched, so do not recycle pre-start NotStarted here.
            self.update_and_send(
                &job_id,
                post_spawn_interruption_delta(
                    &operation,
                    start.elapsed().as_millis() as u64,
                    error,
                ),
            );
            self.start_available_queued();
            return;
        }
        self.update_and_send(
            &job_id,
            RunnerJobDelta {
                status: "running".to_string(),
                ..Default::default()
            },
        );
        let manager = self.clone_for_worker();
        let ssh_pool = self.ssh_pool.clone();
        let output_limit_bytes = policy.max_output_bytes;
        let worker_guard = self.workers.enter();
        let timeout_secs = request.timeout_secs.min(policy.max_timeout_secs).max(1);
        std::thread::spawn(move || {
            let _worker_guard = worker_guard;
            const OUTPUT_CHANNEL_CAPACITY: usize = 64;
            let (tx, rx) = mpsc::sync_channel::<OutputChunk>(OUTPUT_CHANNEL_CAPACITY);
            let mut readers = Vec::new();
            if let Some(stdout) = stdout.take() {
                readers.push(spawn_reader(
                    stdout,
                    tx.clone(),
                    true,
                    OutputTextSource::RemoteSsh,
                ));
            }
            if let Some(stderr) = stderr.take() {
                readers.push(spawn_reader(
                    stderr,
                    tx.clone(),
                    false,
                    OutputTextSource::RemoteSsh,
                ));
            }
            drop(tx);
            // Readers must already be draining before program/caller stdin can
            // block. The writer is tracked and polled by this same Job worker.
            let mut writer_start_error = None;
            let mut stdin_writer = match program_delivery.spawn_writer(child_stdin.take(), None) {
                Ok(writer) => writer,
                Err(error) => {
                    writer_start_error = Some(error);
                    let _ = terminate_managed_tree(&child);
                    None
                }
            };
            let mut transport_stderr = String::new();
            let (mut status, mut exit_code, mut error, interrupted_after_dispatch) = loop {
                let mut out = String::new();
                let mut err = String::new();
                while let Ok(chunk) = rx.try_recv() {
                    match chunk {
                        OutputChunk::Stdout(text) => out.push_str(&text),
                        OutputChunk::Stderr(text) => err.push_str(&text),
                    }
                }
                if !err.is_empty() {
                    append_bounded_tail(&mut transport_stderr, &err, 16 * 1024);
                }
                if !out.is_empty() || !err.is_empty() {
                    manager.update_and_send(
                        &job_id,
                        RunnerJobDelta {
                            status: "running".to_string(),
                            stdout_chunk: (!out.is_empty()).then_some(out),
                            stderr_chunk: (!err.is_empty()).then_some(err),
                            stream_limit_bytes: Some(output_limit_bytes),
                            ..Default::default()
                        },
                    );
                }
                if let Some(writer_error) = writer_start_error.take().or_else(|| {
                    stdin_writer
                        .as_mut()
                        .and_then(|writer| writer.poll_failure())
                }) {
                    let _ = terminate_managed_tree(&child);
                    break ("failed".to_string(), None, Some(writer_error), true);
                }
                let wait_result = {
                    let mut child = lock_unpoison(&child);
                    child.try_wait()
                };
                match wait_result {
                    Ok(Some(status)) => {
                        if stop_requested.load(Ordering::SeqCst) {
                            break (
                                "failed".to_string(),
                                None,
                                Some(
                                    "ssh_command_stopped_after_dispatch: local SSH tree was terminated, remote command outcome is unknown; do not blindly retry"
                                        .to_string(),
                                ),
                                true,
                            );
                        }
                        if status.success() {
                            break ("completed".to_string(), Some(0), None, false);
                        }
                        break ("failed".to_string(), status.code(), None, false);
                    }
                    Ok(None) => {
                        if stop_requested.load(Ordering::SeqCst) {
                            let _ = terminate_managed_tree(&child);
                            break (
                                "failed".to_string(),
                                None,
                                Some(
                                    "ssh_command_stopped_after_dispatch: local SSH tree was terminated, remote command outcome is unknown; do not blindly retry"
                                        .to_string(),
                                ),
                                true,
                            );
                        }
                        if start.elapsed() >= Duration::from_secs(timeout_secs) {
                            stop_requested.store(true, Ordering::SeqCst);
                            let _ = terminate_managed_tree(&child);
                            break (
                                "timeout".to_string(),
                                Some(-1),
                                Some(format!("job timed out after {timeout_secs} seconds")),
                                false,
                            );
                        }
                    }
                    Err(wait_error) => {
                        break (
                            "failed".to_string(),
                            None,
                            Some(format!(
                                "ssh_command_wait_failed: command may have started and was not retried: {wait_error}"
                            )),
                            false,
                        );
                    }
                }
                std::thread::sleep(Duration::from_millis(JOB_UPDATE_INTERVAL_MS));
            };
            // The SSH client is the root of a private process tree. Ensure a
            // background child holding either pipe cannot delay the terminal
            // update indefinitely.
            cleanup_managed_tree(&child);
            let tree_cleanup_uncertain = managed_tree_running(&child);
            let writer_finish_error = stdin_writer.as_mut().and_then(|writer| {
                let interrupted =
                    interrupted_after_dispatch || status == "timeout" || tree_cleanup_uncertain;
                let result = if interrupted {
                    writer.finish_after_tree_cleanup()
                } else {
                    writer.finish_bounded()
                };
                result.err()
            });
            let mut final_out = String::new();
            let mut final_err = String::new();
            drain_and_join_reader_threads_until(
                readers,
                &rx,
                &mut final_out,
                &mut final_err,
                Instant::now() + Duration::from_secs(1),
            );
            if !final_err.is_empty() {
                append_bounded_tail(&mut transport_stderr, &final_err, 16 * 1024);
            }
            let mut command_execution_state = if interrupted_after_dispatch {
                ShellCommandExecutionState::OutcomeUnknown
            } else {
                raw_shell_job_terminal_lifecycle(&status, exit_code)
            };
            if let Some(writer_error) = writer_finish_error {
                status = "failed".to_string();
                exit_code = None;
                error = Some(writer_error);
                command_execution_state = ShellCommandExecutionState::OutcomeUnknown;
            }
            if tree_cleanup_uncertain {
                status = "failed".to_string();
                error = Some(
                    "ssh_command_cleanup_failed: local SSH process tree exit could not be proven; command may have started and was not retried"
                        .to_string(),
                );
                command_execution_state = ShellCommandExecutionState::OutcomeUnknown;
            }
            if matches!(status.as_str(), "completed" | "failed")
                && matches!(
                    command_execution_state,
                    ShellCommandExecutionState::Completed
                )
                && is_transport_failure(&transport, exit_code, Some(&transport_stderr))
            {
                ssh_pool.invalidate_after_transport_failure(&transport);
                status = "failed".to_string();
                error = Some(
                    "ssh_transport_failed: command may have started and was not retried"
                        .to_string(),
                );
                command_execution_state = ShellCommandExecutionState::OutcomeUnknown;
                if !final_err.is_empty() && !final_err.ends_with('\n') {
                    final_err.push('\n');
                }
                final_err.push_str(
                    "webcodex: SSH transport ended after dispatch; the command may have started and was not retried\n",
                );
            }
            manager.update_and_send(
                &job_id,
                RunnerJobDelta {
                    status,
                    stdout_chunk: (!final_out.is_empty()).then_some(final_out),
                    stderr_chunk: (!final_err.is_empty()).then_some(final_err),
                    exit_code,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error,
                    command_execution_state: Some(command_execution_state),
                    stream_limit_bytes: Some(output_limit_bytes),
                    finished: true,
                    ..Default::default()
                },
            );
            manager.start_available_queued();
        });
    }

    pub(super) fn stop(&self, job_id: &str) -> Result<(), String> {
        let queued_job = {
            let _lifecycle = lock_unpoison(&self.lifecycle);
            let mut queued = lock_unpoison(&self.queued);
            if let Some(pos) = queued
                .iter()
                .position(|queued_start| queued_start.operation.job_id() == job_id)
            {
                queued.remove(pos)
            } else {
                None
            }
        };
        if let Some(queued_start) = queued_job {
            let operation = queued_start.operation;
            self.update_and_send(
                job_id,
                RunnerJobDelta {
                    status: "stopped".to_string(),
                    stderr_chunk: Some("job stopped before start".to_string()),
                    exit_code: Some(-1),
                    duration_ms: Some(0),
                    error: Some("job stopped before start".to_string()),
                    command_execution_state: job_prestart_lifecycle(&operation),
                    finished: true,
                    ..Default::default()
                },
            );
            self.start_available_queued();
            return Ok(());
        }
        {
            let jobs = lock_unpoison(&self.jobs);
            let Some(job) = jobs.get(job_id) else {
                return Err(format!("unknown local job: {}", job_id));
            };
            if runner_job_is_terminal(&job.snapshot.status) {
                drop(jobs);
                // A stop can race a terminal update that failed in transport.
                // Replay the retained terminal snapshot with its original
                // sequence so the server converges instead of remaining
                // `stop_requested`.
                self.resend_snapshot(job_id);
                return Ok(());
            }
        }
        let detached = {
            let detached_jobs = lock_unpoison(&self.detached_jobs);
            detached_jobs.get(job_id).cloned()
        };
        if let Some(detached) = detached {
            let record = detached
                .store
                .request_stop(job_id, &detached.execution_id)?;
            self.sync_detached_record(job_id, &record)?;
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                let record = detached.store.read(job_id)?;
                let record = detached
                    .store
                    .reconcile_after_runner_restart(record)?
                    .ok_or_else(|| {
                        format!("detached Job {job_id} regressed before ownership acceptance")
                    })?;
                let terminal = self.sync_detached_record(job_id, &record)?;
                if terminal {
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    return Err(format!(
                        "detached Job {job_id} stop was durably requested but terminal state was not observed within the bounded deadline"
                    ));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        let (child, stop_requested) = {
            let jobs = lock_unpoison(&self.jobs);
            let job = jobs
                .get(job_id)
                .ok_or_else(|| format!("unknown local job: {job_id}"))?;
            (job.child.clone(), job.stop_requested.clone())
        };
        stop_requested.store(true, Ordering::SeqCst);
        self.update_and_send(
            job_id,
            RunnerJobDelta {
                status: "stop_requested".to_string(),
                error: Some("stop requested".to_string()),
                ..Default::default()
            },
        );
        if let Some(child) = child {
            let deadline = Instant::now() + Duration::from_secs(1);
            if let Err(e) = terminate_managed_tree(&child) {
                return Err(format!("failed to kill job {}: {}", job_id, e));
            }
            match wait_managed_tree_exit(&child, deadline) {
                Ok(true) => {}
                Ok(false) => {
                    return Err(format!(
                        "failed to kill job {}: job tree did not exit within the bounded stop deadline",
                        job_id
                    ));
                }
                Err(e) => {
                    return Err(format!("failed to wait for job {} tree: {}", job_id, e));
                }
            }
            // Best-effort reap of the direct child so a Unix parent killed by
            // termination is not left as a zombie for the worker to discover
            // late.
            let _ = reap_managed_direct_child(&child, deadline);
            Ok(())
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
#[path = "job_manager_tests.rs"]
pub(crate) mod job_manager_tests;

#[cfg(test)]
mod utf8_truncation_tests {
    use super::*;

    #[test]
    fn runner_stream_truncation_keeps_utf8_boundary() {
        let mut stream = ShellJobStreamSnapshot::default();
        let chunk = format!(
            "█{}",
            "x".repeat(JOB_SNAPSHOT_STREAM_MAX_BYTES - "█".len() + 1)
        );
        assert_eq!(chunk.len(), JOB_SNAPSHOT_STREAM_MAX_BYTES + 1);

        append_runner_stream(&mut stream, Some(&chunk));

        assert!(stream.truncated);
        assert!(stream.tail.len() <= JOB_SNAPSHOT_STREAM_MAX_BYTES);
        assert!(stream.tail.bytes().all(|byte| byte == b'x'));
    }

    #[test]
    fn runner_inventory_trim_keeps_utf8_boundary() {
        let mut stream = ShellJobStreamSnapshot {
            tail: format!("█{}", "x".repeat(64)),
            ..Default::default()
        };
        let max_bytes = stream.tail.len() - 1;

        trim_runner_stream_to(&mut stream, max_bytes);

        assert!(stream.truncated);
        assert!(stream.tail.len() <= max_bytes);
        assert!(stream.tail.bytes().all(|byte| byte == b'x'));
    }
}
