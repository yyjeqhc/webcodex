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
    run_process_with_profiles_and_execution_state_with_internal_env_and_start_hook,
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

const GO_PROJECT_SINGLE_MODULE_ENV: [(&str, &str); 2] = [("GO111MODULE", "on"), ("GOWORK", "off")];

const JOB_UPDATE_INTERVAL_MS: u64 = 250;
/// Runner-owned liveness cadence for active Jobs. The cadence is independent
/// of stdout/stderr and other Job traffic so one noisy Job cannot starve
/// liveness evidence for another silent Job.
const JOB_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);

/// At most the validated Job state machine's required semantic transitions are
/// retained for live delivery. Output and advisory current-activity-only
/// updates are coalesced separately below.
const JOB_UPDATE_REQUIRED_PENDING_MAX: usize = 8;
const JOB_UPDATE_DELIVERY_RETRY: Duration = Duration::from_millis(JOB_UPDATE_INTERVAL_MS);

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
    /// Orders the gap between assigning/observing an update sequence and making
    /// that update visible to the delivery worker. The worker holds this only
    /// while selecting a candidate; transport I/O always happens after release.
    job_update_delivery_order: Arc<Mutex<()>>,
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
        let job_update_delivery_order = Arc::new(Mutex::new(()));
        let delivery_signal = Arc::new(JobUpdateDeliverySignal::default());
        spawn_job_update_delivery_worker(
            Arc::downgrade(&jobs),
            Arc::downgrade(&current_sink),
            Arc::downgrade(&pending_job_updates),
            Arc::downgrade(&job_update_delivery_order),
            Arc::clone(&delivery_signal),
        );
        spawn_job_heartbeat_worker(
            Arc::downgrade(&jobs),
            Arc::downgrade(&pending_job_updates),
            Arc::downgrade(&job_update_delivery_order),
            Arc::downgrade(&shutting_down),
            Arc::downgrade(&delivery_signal),
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
            job_update_delivery_order,
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
    input: Option<Arc<input::InputChannel>>,
    stop_requested: Arc<AtomicBool>,
    slot_reserved: bool,
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
#[cfg(test)]
use admission::{validate_runner_job_context, validate_runner_structured_common};
use delivery::{
    spawn_job_heartbeat_worker, spawn_job_update_delivery_worker, JobUpdateDeliveryQueue,
    JobUpdateDeliverySignal,
};
pub(crate) use lifecycle::decode_failure_prestart_lifecycle;
use shutdown::JobManagerOwnerLifetime;
mod admission;
#[cfg(test)]
use delivery::{
    job_update_from_delivery, queue_job_heartbeat_batch, spawn_job_heartbeat_worker_with_interval,
    PendingJobUpdateDelivery,
};
mod delivery;
mod detached;
#[cfg(test)]
use lifecycle::{
    cargo_activity_from_stderr, job_prestart_lifecycle, post_spawn_interruption_lifecycle,
    process_running_activity, raw_shell_job_terminal_lifecycle, runner_job_is_active,
    runner_job_is_terminal, validation_step_activity,
};
#[cfg(all(test, windows))]
use lifecycle::{post_spawn_interruption_delta, post_spawn_interruption_reason};
mod input;
mod interactive;
mod lifecycle;
mod local_shell;
mod process;
#[cfg(test)]
use retained::{append_runner_stream, trim_runner_stream_to};
mod retained;
mod shell_worker;
mod shutdown;
mod ssh;

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
