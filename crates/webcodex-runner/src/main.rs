use reqwest::blocking::Client;
use std::collections::{HashMap, VecDeque};
use std::error::Error as StdError;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex, Weak};
use std::time::{Duration, Instant};
use tracing_subscriber::EnvFilter;
use webcodex_process::{GracefulTermination, ManagedChild};
use webcodex_runner::shutdown::{lock_unpoison, ActivityTracker, BackgroundThreads};

#[cfg(test)]
#[path = "webcodex_runner/job_manager_tests.rs"]
mod job_manager_tests;
mod webcodex_runner;

use webcodex_agent_config as agent_init;
use webcodex_core::{
    apply_edits_shared, artifact_policy, build_info, lsp_bridge, shell_protocol, validation_bridge,
};
use webcodex_sandbox as command_sandbox;
use webcodex_workspace::{project_overview, workspace_checkpoint};

use shell_protocol::{
    validation_infrastructure_failure_code, AgentPolicySummary, ShellAgentJobUpdateRequest,
    ShellAgentPollPayload, ShellAgentPollRequest, ShellAgentPollResponse, ShellAgentProjectSummary,
    ShellAgentShellRequest, ShellClientCapabilities, ShellClientRegisterRequest,
    ShellClientRegisterResponse, ShellCommandExecutionState, ShellJobContext, ShellJobInventory,
    ShellJobLogSnapshot, ShellJobSnapshot, ShellJobStreamSnapshot, ShellJobValidationProgress,
    ShellJobValidationStep, ShellProfileSummaryEntry, ShellProfilesSummary,
    AGENT_PROTOCOL_VERSION_POLLING_V1, JOB_INVENTORY_MAX_ACTIVE_JOBS,
    JOB_INVENTORY_MAX_SERIALIZED_BYTES, JOB_INVENTORY_MAX_TERMINAL_JOBS,
    JOB_SNAPSHOT_STREAM_MAX_BYTES, JOB_TERMINAL_RETENTION_SECS, VALIDATION_STEP_SPAWN_FAILED_CODE,
    VALIDATION_STEP_WAIT_FAILED_CODE, VALIDATION_TOOL_UNAVAILABLE_CODE,
};

#[cfg(test)]
use agent_init::{TRANSPORT_AUTO, TRANSPORT_POLLING, TRANSPORT_QUIC, TRANSPORT_WEBSOCKET};
#[cfg(test)]
use shell_protocol::{
    AgentEnvelope, AGENT_PROTOCOL_VERSION_QUIC_V1, AGENT_PROTOCOL_VERSION_WEBSOCKET_V1,
};
#[cfg(test)]
use std::collections::BTreeMap;
#[cfg(test)]
use std::net::SocketAddr;
use webcodex_runner::contains_any;
use webcodex_runner::output_text::{OutputTextDecoder, OutputTextSource};
#[cfg(test)]
use webcodex_runner::QuicClientConfig;
#[cfg(test)]
use webcodex_runner::{
    agent_project_summary, auto_transport_plan, build_ws_request, default_quic_alpn,
    default_quic_connect_timeout_secs, default_quic_keepalive_interval_secs,
    default_websocket_connect_timeout_secs, effective_transport, handle_project_op,
    load_agent_project_summaries_from_dir, non_empty_token, parse_agent_project_toml,
    quic_client_bind_addr_for, resolve_quic_config, resolve_quic_server_addrs, run_shell,
    run_shell_with_profiles, server_url_to_ws, sha256_hex_bytes, validate_project_path_policy,
    websocket_session, AgentRuntimeState, ShellProfileConfig, CLIENT_PROFILE_ERROR,
    DEFAULT_MAX_CONCURRENT_JOBS, WS_OUTGOING_CAPACITY,
};
use webcodex_runner::{
    client_profile_agent_config, configured_prepared_shell_job_command,
    configured_shell_job_command, configured_validation_job_command, cwd_allowed,
    default_config_path, dispatch_request, err_cmd, handle_apply_text_edits_file_request,
    handle_artifact_file_request, handle_basic_file_request, handle_checkpoint_file_request,
    handle_write_project_file_request, hostname, is_artifact_request_kind,
    is_basic_file_request_kind, is_checkpoint_request_kind, is_project_op,
    is_structured_edit_request_kind, load_config, max_concurrent_jobs, ok_cmd, projects_dir,
    resolve_prepared_shell_profile, resolve_requested_path, run_agent, validate_client_profile,
    validate_structured_edit_agent_path, AgentConfig, AgentPolicy, AgentProjectCache, AgentSink,
    CommandResult, HotAgentConfig, HttpSendConfig, PreparedShellProfile, PreparedShellProfileCache,
    ReloadableAgentConfig, ShellConfig, SubmitResultError,
};
use webcodex_runner::{is_transport_failure, SshConfig, SshConnectionPool};
use webcodex_runner::{
    run_process_with_profiles_in_sandbox_and_execution_state_with_start_hook,
    run_script_with_profiles_in_sandbox_and_execution_state_with_start_hook,
};

const JOB_UPDATE_INTERVAL_MS: u64 = 250;
const AGENT_REGISTER_PATH: &str = "/api/shell/agent/register";
const AGENT_POLL_PATH: &str = "/api/shell/agent/poll";
/// Polling HTTP responses can carry the current largest 15 MiB request
/// payloads plus their JSON envelope, but must never be loaded without a
/// finite bound.
const AGENT_HTTP_RESPONSE_BODY_MAX_BYTES: usize = 32 * 1024 * 1024;

/// At most the validated Job state machine's semantic transitions are retained
/// for live delivery. Output-only updates are coalesced separately below.
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
        let targets = {
            let jobs = lock_unpoison(&jobs);
            jobs.values()
                .filter(|job| runner_job_is_active(&job.snapshot.status))
                .map(|job| (job.child.clone(), Arc::clone(&job.stop_requested)))
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
struct PendingJobStart {
    generation: u64,
    policy: AgentPolicy,
    shell: ShellConfig,
    ssh: SshConfig,
    projects_dir: PathBuf,
    request: ShellAgentShellRequest,
}

#[derive(Debug, Clone)]
struct JobManager {
    max_concurrent: usize,
    jobs: Arc<Mutex<HashMap<String, RunningJob>>>,
    queued: Arc<Mutex<VecDeque<PendingJobStart>>>,
    prepared_profiles: PreparedShellProfileCache,
    ssh_pool: SshConnectionPool,
    lifecycle: Arc<Mutex<()>>,
    shutting_down: Arc<AtomicBool>,
    workers: ActivityTracker,
    current_sink: Arc<Mutex<Option<AgentSink>>>,
    pending_job_updates: Arc<Mutex<HashMap<String, JobUpdateDeliveryQueue>>>,
    delivery_signal: Arc<JobUpdateDeliverySignal>,
    owner_lifetime: Option<Arc<JobManagerOwnerLifetime>>,
}

impl JobManager {
    fn new(max_concurrent: usize) -> Self {
        let jobs = Arc::new(Mutex::new(HashMap::new()));
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
            shutting_down: Arc::downgrade(&shutting_down),
            delivery_signal: Arc::clone(&delivery_signal),
        });
        Self {
            max_concurrent: max_concurrent.max(1),
            jobs,
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
        }
    }

    fn clone_for_worker(&self) -> Self {
        let mut worker = self.clone();
        worker.owner_lifetime = None;
        worker
    }
}

#[derive(Debug, Clone)]
struct RunningJob {
    client_id: String,
    agent_instance_id: String,
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
    finished: bool,
}

impl PendingJobUpdateDelivery {
    fn from_update(update: &ShellAgentJobUpdateRequest) -> Self {
        Self {
            update_seq: update.update_seq.unwrap_or_default(),
            status: update.status.clone(),
            exit_code: update.exit_code,
            duration_ms: update.duration_ms,
            error: update.error.clone(),
            command_execution_state: update.command_execution_state.clone(),
            validation_progress: update.validation_progress.clone(),
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
) -> ShellAgentJobUpdateRequest {
    let mut update =
        job_update_from_snapshot(&job.client_id, &job.agent_instance_id, &job.snapshot);
    update.update_seq = Some(pending.update_seq);
    update.status = pending.status.clone();
    update.exit_code = pending.exit_code;
    update.duration_ms = pending.duration_ms;
    update.error = pending.error.clone();
    update.command_execution_state = pending.command_execution_state.clone();
    update.validation_progress = pending.validation_progress.clone();
    update.finished = pending.finished;
    update
}

fn spawn_job_update_delivery_worker(
    jobs: Weak<Mutex<HashMap<String, RunningJob>>>,
    current_sink: Weak<Mutex<Option<AgentSink>>>,
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

            match sink.try_send_job_update(&update) {
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
        context: shell_protocol::ShellJobContext {
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
    }
}

#[cfg(test)]
fn test_job_context(cwd: &Path, validation_steps: Vec<String>) -> shell_protocol::ShellJobContext {
    shell_protocol::ShellJobContext {
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

#[derive(Debug)]
enum OutputChunk {
    Stdout(String),
    Stderr(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AgentCliAction {
    Run {
        config_path: PathBuf,
        once: bool,
    },
    Exit {
        code: i32,
        stdout: String,
        stderr: String,
    },
}

fn usage() -> &'static str {
    "Usage: webcodex-runner [--config PATH] [--once]\n\n\
     Options:\n\
       -h, --help                 Print help and exit\n\
       -V, --version              Print version and exit\n\
       -c, --config PATH          Agent config path for normal runtime\n\
       --profile NAME             Client config profile for default config path\n\
       --once                     Complete one successful poll, then exit (polling transport)\n\n\
     With --profile, the default config path is derived under\n\
     /etc/webcodex/clients/<profile> for root or\n\
     ~/.config/webcodex/clients/<profile> for non-root users. Explicit\n\
     --config overrides the profile-derived default.\n\n\
     Environment:\n\
       WEBCODEX_AGENT_CONFIG      default config path override\n\
     Example agent.toml:\n\
       server_url = \"https://v4.yyjeqhc.cn\"\n\
       token = \"...\"\n\
       client_id = \"xrh\"\n\
       display_name = \"XRH\"\n\
       owner = \"yyjeqhc\"\n\
       projects_dir = \"/root/.config/webcodex/projects.d\"\n\
       poll_interval_ms = 1000\n\
\n\
       [policy]\n\
       allow_raw_shell = true\n\
       allow_cwd_anywhere = true\n\
       max_timeout_secs = 3600\n\
       max_output_bytes = 262144\n"
}

fn parse_args() -> Result<AgentCliAction, String> {
    parse_agent_args(std::env::args().skip(1))
}

fn parse_agent_args<I, S>(args: I) -> Result<AgentCliAction, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args: Vec<String> = args
        .into_iter()
        .map(|arg| arg.as_ref().to_string())
        .collect();
    if args.len() == 1 {
        match args[0].as_str() {
            "--help" | "-h" => {
                return Ok(AgentCliAction::Exit {
                    code: 0,
                    stdout: usage().to_string(),
                    stderr: String::new(),
                });
            }
            "--version" | "-V" => {
                return Ok(AgentCliAction::Exit {
                    code: 0,
                    stdout: build_info::version_output("webcodex-runner"),
                    stderr: String::new(),
                });
            }
            _ => {}
        }
    }
    let mut config_path = match std::env::var("WEBCODEX_AGENT_CONFIG") {
        Ok(path) => PathBuf::from(path),
        Err(_) => default_config_path()?,
    };
    let mut config_explicit = false;
    let mut profile: Option<String> = None;
    let mut once = false;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                return Ok(AgentCliAction::Exit {
                    code: 0,
                    stdout: usage().to_string(),
                    stderr: String::new(),
                });
            }
            "--version" | "-V" => {
                return Ok(AgentCliAction::Exit {
                    code: 0,
                    stdout: build_info::version_output("webcodex-runner"),
                    stderr: String::new(),
                });
            }
            "--once" => once = true,
            "--config" | "-c" => {
                let Some(path) = args.next() else {
                    return Err("--config requires a path".to_string());
                };
                config_path = PathBuf::from(path);
                config_explicit = true;
            }
            "--profile" => {
                let Some(value) = args.next() else {
                    return Err("--profile requires a value".to_string());
                };
                profile = Some(value);
            }
            _ => return Err(format!("unknown argument: {}\n{}", arg, usage())),
        }
    }
    if let Some(profile) = profile
        .as_deref()
        .map(validate_client_profile)
        .transpose()?
    {
        if !config_explicit {
            config_path = client_profile_agent_config(&profile)?;
        }
    }
    Ok(AgentCliAction::Run { config_path, once })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentHttpErrorKind {
    ServerUnavailable,
    Auth,
    NotFound,
    /// A local URL/TLS configuration failure that retrying cannot repair.
    Config,
    /// 4xx (other than auth/endpoint kinds): the server understood the
    /// exchange and rejected this exact request. Resending the identical
    /// payload cannot succeed.
    ClientRejected,
    Status,
    RequestTimeout,
    Request,
    /// The response was incomplete or was recognizably produced by a
    /// temporary proxy/upstream failure.
    DecodeTransient,
    /// The response was complete enough to prove that it does not implement
    /// the expected server protocol.
    ProtocolDecode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AgentHttpError {
    kind: AgentHttpErrorKind,
    path: String,
    summary: String,
    /// Bounded structured server error, when the response contract supplied
    /// one. Recovery classifiers use this instead of parsing display strings.
    server_error: Option<String>,
}

impl AgentHttpError {
    fn status(path: &str, status: reqwest::StatusCode, body: &str) -> Self {
        let kind = match status.as_u16() {
            401 | 403 => AgentHttpErrorKind::Auth,
            404 => AgentHttpErrorKind::NotFound,
            // Explicitly retryable request-level statuses.
            408 | 429 => AgentHttpErrorKind::Status,
            code if (500..600).contains(&code) => AgentHttpErrorKind::ServerUnavailable,
            code if (400..500).contains(&code) => AgentHttpErrorKind::ClientRejected,
            _ if looks_like_proxy_html_error(body) => AgentHttpErrorKind::ServerUnavailable,
            _ => AgentHttpErrorKind::Status,
        };
        let server_error = structured_body_error(body);
        let mut summary = http_status_summary(status);
        if kind == AgentHttpErrorKind::ClientRejected {
            if let Some(detail) = server_error.as_deref() {
                summary = format!("{}: {}", summary, detail);
            }
        }
        Self {
            kind,
            path: bounded_endpoint_path(path),
            summary,
            server_error,
        }
    }

    fn request(path: &str, error: reqwest::Error) -> Self {
        let chain = error_chain_text(&error);
        let kind = if error.is_builder() || looks_like_fatal_tls_request(&chain) {
            AgentHttpErrorKind::Config
        } else if looks_like_server_down_request(&error, &chain) {
            AgentHttpErrorKind::ServerUnavailable
        } else if error.is_timeout() {
            AgentHttpErrorKind::RequestTimeout
        } else {
            AgentHttpErrorKind::Request
        };
        Self {
            kind,
            path: bounded_endpoint_path(path),
            summary: request_error_summary(error, &chain),
            server_error: None,
        }
    }

    fn decode_transient(path: &str, summary: String) -> Self {
        Self {
            kind: AgentHttpErrorKind::DecodeTransient,
            path: bounded_endpoint_path(path),
            summary,
            server_error: None,
        }
    }

    fn protocol_decode(path: &str, summary: String) -> Self {
        Self {
            kind: AgentHttpErrorKind::ProtocolDecode,
            path: bounded_endpoint_path(path),
            summary,
            server_error: None,
        }
    }
}

impl std::fmt::Display for AgentHttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            AgentHttpErrorKind::ServerUnavailable => {
                write!(f, "server unavailable for {}: {}", self.path, self.summary)
            }
            AgentHttpErrorKind::Auth => write!(
                f,
                "authentication failed for {}: {}; check agent token/config",
                self.path, self.summary
            ),
            AgentHttpErrorKind::NotFound => write!(
                f,
                "endpoint missing or incompatible server for {}: {}",
                self.path, self.summary
            ),
            AgentHttpErrorKind::Config => {
                write!(
                    f,
                    "HTTP/TLS configuration failed for {}: {}",
                    self.path, self.summary
                )
            }
            AgentHttpErrorKind::ClientRejected => {
                write!(f, "server rejected {} request: {}", self.path, self.summary)
            }
            AgentHttpErrorKind::Status
            | AgentHttpErrorKind::RequestTimeout
            | AgentHttpErrorKind::Request => {
                write!(f, "{} request failed: {}", self.path, self.summary)
            }
            AgentHttpErrorKind::DecodeTransient => {
                write!(
                    f,
                    "transient response corruption for {}: {}",
                    self.path, self.summary
                )
            }
            AgentHttpErrorKind::ProtocolDecode => write!(
                f,
                "response from {} incompatible with server protocol: {}",
                self.path, self.summary
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegisterRecoveryAction {
    Retry,
    WaitForLease,
    Fatal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RegisterErrorKind {
    Transient,
    LeaseConflict,
    Auth,
    EndpointMissing,
    Rejected,
    Config,
    Protocol,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RegisterError {
    kind: RegisterErrorKind,
    message: String,
}

impl RegisterError {
    fn from_http(error: AgentHttpError, client_id: &str) -> Self {
        let kind = match error.kind {
            AgentHttpErrorKind::ServerUnavailable
            | AgentHttpErrorKind::Status
            | AgentHttpErrorKind::RequestTimeout
            | AgentHttpErrorKind::Request
            | AgentHttpErrorKind::DecodeTransient => RegisterErrorKind::Transient,
            AgentHttpErrorKind::Auth => RegisterErrorKind::Auth,
            AgentHttpErrorKind::NotFound => RegisterErrorKind::EndpointMissing,
            AgentHttpErrorKind::Config => RegisterErrorKind::Config,
            AgentHttpErrorKind::ProtocolDecode => RegisterErrorKind::Protocol,
            AgentHttpErrorKind::ClientRejected
                if is_active_instance_lease_conflict(client_id, error.server_error.as_deref()) =>
            {
                RegisterErrorKind::LeaseConflict
            }
            AgentHttpErrorKind::ClientRejected => RegisterErrorKind::Rejected,
        };
        let message = if error.kind == AgentHttpErrorKind::ProtocolDecode {
            format!(
                "register response incompatible with server protocol: endpoint={} {}",
                error.path, error.summary
            )
        } else {
            error.to_string()
        };
        Self { kind, message }
    }

    fn from_response_error(client_id: &str, error: Option<String>) -> Self {
        let summary =
            bounded_single_line(error.as_deref().unwrap_or("register failed without error"));
        let kind = if is_active_instance_lease_conflict(client_id, Some(&summary)) {
            RegisterErrorKind::LeaseConflict
        } else if looks_like_auth_failure_message(&summary) {
            RegisterErrorKind::Auth
        } else {
            RegisterErrorKind::Rejected
        };
        Self {
            kind,
            message: format!("register rejected by server: {summary}"),
        }
    }

    fn recovery_action(&self) -> RegisterRecoveryAction {
        match self.kind {
            RegisterErrorKind::Transient => RegisterRecoveryAction::Retry,
            RegisterErrorKind::LeaseConflict => RegisterRecoveryAction::WaitForLease,
            RegisterErrorKind::Auth
            | RegisterErrorKind::EndpointMissing
            | RegisterErrorKind::Rejected
            | RegisterErrorKind::Config
            | RegisterErrorKind::Protocol => RegisterRecoveryAction::Fatal,
        }
    }

    fn into_message(self) -> String {
        self.message
    }
}

impl std::fmt::Display for RegisterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PollingRecoveryAction {
    RetryPoll,
    ReRegister,
    Fatal,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PollErrorKind {
    Transient,
    SessionLost,
    Auth,
    EndpointMissing,
    Rejected,
    Config,
    Protocol,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PollError {
    kind: PollErrorKind,
    message: String,
}

impl PollError {
    fn new(kind: PollErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    fn from_http(error: AgentHttpError, client_id: &str) -> Self {
        match error.kind {
            AgentHttpErrorKind::ServerUnavailable => Self::new(
                PollErrorKind::Transient,
                format!(
                    "server unavailable while polling {}: {}",
                    error.path, error.summary
                ),
            ),
            AgentHttpErrorKind::Auth => Self::new(
                PollErrorKind::Auth,
                format!(
                    "authentication failed while polling {}: {}; check agent token/config",
                    error.path, error.summary
                ),
            ),
            AgentHttpErrorKind::NotFound => Self::new(
                PollErrorKind::EndpointMissing,
                format!(
                    "poll endpoint missing or incompatible server while polling {}: {}",
                    error.path, error.summary
                ),
            ),
            AgentHttpErrorKind::Config => Self::new(
                PollErrorKind::Config,
                format!(
                    "HTTP/TLS configuration failed while polling {}: {}",
                    error.path, error.summary
                ),
            ),
            AgentHttpErrorKind::RequestTimeout => Self::new(
                PollErrorKind::Transient,
                format!(
                    "poll request timed out while polling {}: {}",
                    error.path, error.summary
                ),
            ),
            AgentHttpErrorKind::ClientRejected
                if is_unknown_polling_session(client_id, error.server_error.as_deref()) =>
            {
                Self::new(
                    PollErrorKind::SessionLost,
                    format!(
                        "polling session is not registered for client_id={}",
                        bounded_single_line(client_id)
                    ),
                )
            }
            AgentHttpErrorKind::ClientRejected => Self::new(
                PollErrorKind::Rejected,
                format!(
                    "server permanently rejected polling {}: {}",
                    error.path, error.summary
                ),
            ),
            AgentHttpErrorKind::Status | AgentHttpErrorKind::Request => Self::new(
                PollErrorKind::Transient,
                format!(
                    "poll request failed while polling {}: {}",
                    error.path, error.summary
                ),
            ),
            AgentHttpErrorKind::DecodeTransient => Self::new(
                PollErrorKind::Transient,
                format!(
                    "transient poll response corruption: endpoint={} {}",
                    error.path, error.summary
                ),
            ),
            AgentHttpErrorKind::ProtocolDecode => Self::new(
                PollErrorKind::Protocol,
                format!(
                    "poll response incompatible with server protocol: endpoint={} {}",
                    error.path, error.summary
                ),
            ),
        }
    }

    /// Classify a fatal result submission failure surfaced by
    /// `dispatch_request`. Permanent rejection and exhausted transient retries
    /// are resolved as payload-lifecycle outcomes inside the HTTP sink, so
    /// neither can trigger polling sleep/re-registration recovery here.
    fn from_submit(error: SubmitResultError) -> Self {
        match error {
            SubmitResultError::FatalAuth(message) => Self::new(PollErrorKind::Auth, message),
            SubmitResultError::FatalProtocol(message) => {
                Self::new(PollErrorKind::EndpointMissing, message)
            }
            SubmitResultError::FatalConfig(message) => Self::new(PollErrorKind::Config, message),
            SubmitResultError::TransportClosed(message) => {
                Self::new(PollErrorKind::Rejected, message)
            }
            SubmitResultError::Shutdown(message) => Self::new(PollErrorKind::Shutdown, message),
        }
    }

    fn from_response_error(client_id: &str, error: Option<String>) -> Self {
        let message = error.unwrap_or_else(|| "poll failed without error".to_string());
        let summary = bounded_single_line(&message);
        if looks_like_auth_failure_message(&summary) {
            Self::new(
                PollErrorKind::Auth,
                format!(
                    "authentication failed while polling {}: {}; check agent token/config",
                    AGENT_POLL_PATH, summary
                ),
            )
        } else if is_unknown_polling_session(client_id, Some(&summary)) {
            Self::new(
                PollErrorKind::SessionLost,
                format!(
                    "polling session is not registered for client_id={}",
                    bounded_single_line(client_id)
                ),
            )
        } else {
            Self::new(
                PollErrorKind::Rejected,
                format!("server permanently rejected polling response: {summary}"),
            )
        }
    }

    fn recovery_action(&self) -> PollingRecoveryAction {
        match self.kind {
            PollErrorKind::Transient => PollingRecoveryAction::RetryPoll,
            PollErrorKind::SessionLost => PollingRecoveryAction::ReRegister,
            PollErrorKind::Auth
            | PollErrorKind::EndpointMissing
            | PollErrorKind::Rejected
            | PollErrorKind::Config
            | PollErrorKind::Protocol => PollingRecoveryAction::Fatal,
            PollErrorKind::Shutdown => PollingRecoveryAction::Shutdown,
        }
    }

    #[cfg(test)]
    fn is_terminal(&self) -> bool {
        self.recovery_action() == PollingRecoveryAction::Fatal
    }

    #[cfg(test)]
    fn is_shutdown(&self) -> bool {
        self.kind == PollErrorKind::Shutdown
    }

    fn into_message(self) -> String {
        self.message
    }
}

impl std::fmt::Display for PollError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// Polling needs enough independent dispatch capacity for one long ordinary
/// request plus a later request (notably a Job start/stop or persistent-shell
/// close), but E1 deliberately does not introduce deployment tuning. Two is
/// the smallest bound that removes the one-request starvation failure while
/// keeping the process-local OS-thread surface conservative.
pub(crate) const POLLING_DISPATCH_MAX_IN_FLIGHT: usize = 2;

struct PollingDispatch {
    request_id: String,
    project_cache_invalidation_required: bool,
    sink: AgentSink,
    config: Arc<HotAgentConfig>,
    runtime: Arc<ReloadableAgentConfig>,
    jobs: JobManager,
    persistent_shells: webcodex_runner::PersistentShellManager,
    projects_dir: PathBuf,
    lsp: webcodex_runner::LspSupervisor,
    request: ShellAgentShellRequest,
}

impl PollingDispatch {
    fn run(self) -> Result<bool, SubmitResultError> {
        dispatch_request(
            &self.sink,
            &self.config,
            &self.runtime,
            &self.jobs,
            &self.persistent_shells,
            &self.projects_dir,
            &self.lsp,
            self.request,
        )
    }
}

struct PollingDispatchCompletion {
    request_id: String,
    project_cache_invalidation_required: bool,
    dispatch_result: Result<bool, SubmitResultError>,
}

/// Sends a completion even if a worker unwinds. It is declared before the
/// ActivityGuard in the worker so reverse drop order releases the activity
/// slot only after dispatch and result submission, then publishes completion.
struct PollingDispatchCompletionOnDrop {
    completion_tx: mpsc::SyncSender<PollingDispatchCompletion>,
    request_id: String,
    project_cache_invalidation_required: bool,
    dispatch_result: Option<Result<bool, SubmitResultError>>,
}

impl PollingDispatchCompletionOnDrop {
    fn new(
        completion_tx: mpsc::SyncSender<PollingDispatchCompletion>,
        request_id: String,
        project_cache_invalidation_required: bool,
    ) -> Self {
        Self {
            completion_tx,
            request_id,
            project_cache_invalidation_required,
            dispatch_result: None,
        }
    }

    fn complete(&mut self, result: Result<bool, SubmitResultError>) {
        self.dispatch_result = Some(result);
    }
}

impl Drop for PollingDispatchCompletionOnDrop {
    fn drop(&mut self) {
        let dispatch_result = self.dispatch_result.take().unwrap_or_else(|| {
            Err(SubmitResultError::TransportClosed(
                "polling dispatch worker closed unexpectedly".to_string(),
            ))
        });
        let _ = self.completion_tx.send(PollingDispatchCompletion {
            request_id: std::mem::take(&mut self.request_id),
            project_cache_invalidation_required: self.project_cache_invalidation_required,
            dispatch_result,
        });
    }
}

/// Process-local coordination for normal polling dispatches. The Server queue
/// remains the only pending-work queue: this supervisor admits at most two
/// already-dequeued requests, creates no local holding queue, and returns
/// worker completion/fatal submission outcomes to the polling control loop.
pub(crate) struct PollingDispatchSupervisor {
    completion_tx: mpsc::SyncSender<PollingDispatchCompletion>,
    completion_rx: mpsc::Receiver<PollingDispatchCompletion>,
    in_flight: usize,
    background_threads: Arc<BackgroundThreads>,
    dispatches: ActivityTracker,
}

impl PollingDispatchSupervisor {
    pub(crate) fn new(
        background_threads: Arc<BackgroundThreads>,
        dispatches: ActivityTracker,
    ) -> Self {
        let (completion_tx, completion_rx) = mpsc::sync_channel(POLLING_DISPATCH_MAX_IN_FLIGHT);
        Self {
            completion_tx,
            completion_rx,
            in_flight: 0,
            background_threads,
            dispatches,
        }
    }

    fn has_capacity(&self) -> bool {
        self.in_flight < POLLING_DISPATCH_MAX_IN_FLIGHT
    }

    fn spawn(&mut self, dispatch: PollingDispatch) -> Result<(), PollError> {
        if !self.has_capacity() {
            return Err(PollError::new(
                PollErrorKind::Config,
                "polling dispatch capacity invariant violated",
            ));
        }
        let completion_tx = self.completion_tx.clone();
        let dispatch_guard = self.dispatches.enter();
        let request_id = dispatch.request_id.clone();
        let project_cache_invalidation_required = dispatch.project_cache_invalidation_required;
        let handle = std::thread::Builder::new()
            .name("webcodex-poll-dispatch".to_string())
            .spawn(move || {
                let mut completion = PollingDispatchCompletionOnDrop::new(
                    completion_tx,
                    request_id,
                    project_cache_invalidation_required,
                );
                let _dispatch_guard = dispatch_guard;
                completion.complete(dispatch.run());
            })
            .map_err(|error| {
                PollError::new(
                    PollErrorKind::Config,
                    format!("failed to start polling dispatch worker: {error}"),
                )
            })?;
        self.in_flight += 1;
        self.background_threads.register(handle);
        Ok(())
    }

    fn record_completion(
        &mut self,
        project_cache: &mut AgentProjectCache,
        completion: PollingDispatchCompletion,
    ) -> Result<bool, SubmitResultError> {
        self.in_flight = self.in_flight.checked_sub(1).unwrap_or_else(|| {
            debug_assert!(false, "polling completion without an in-flight dispatch");
            0
        });
        let _request_id = completion.request_id;
        if completion.project_cache_invalidation_required && completion.dispatch_result.is_ok() {
            project_cache.invalidate();
        }
        completion.dispatch_result
    }

    /// Inspect every completion currently available. This is called before
    /// each poll and after each poll/dispatch turn, so a fatal result delivery
    /// failure cannot be silently discarded by background dispatch.
    pub(crate) fn drain_completed(
        &mut self,
        project_cache: &mut AgentProjectCache,
    ) -> Result<(), PollError> {
        loop {
            match self.completion_rx.try_recv() {
                Ok(completion) => {
                    let result = self
                        .record_completion(project_cache, completion)
                        .map(|_| ())
                        .map_err(PollError::from_submit);
                    let _ = self.background_threads.reap_finished();
                    result?;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    let _ = self.background_threads.reap_finished();
                    return Ok(());
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err(PollError::from_submit(SubmitResultError::TransportClosed(
                        "polling dispatch completion channel closed".to_string(),
                    )));
                }
            }
        }
    }

    /// Apply backpressure before another Server dequeue. There is no local
    /// pending queue: when both slots are occupied the control loop waits for
    /// one worker completion (or shutdown) and only then polls again.
    pub(crate) fn wait_for_capacity_or_shutdown(
        &mut self,
        project_cache: &mut AgentProjectCache,
        shutdown: &AtomicBool,
    ) -> Result<(), PollError> {
        while !self.has_capacity() {
            if shutdown.load(Ordering::SeqCst) {
                return Err(PollError::from_submit(SubmitResultError::Shutdown(
                    "process shutdown".to_string(),
                )));
            }
            match self.completion_rx.recv_timeout(Duration::from_millis(25)) {
                Ok(completion) => {
                    let result = self
                        .record_completion(project_cache, completion)
                        .map(|_| ())
                        .map_err(PollError::from_submit);
                    let _ = self.background_threads.reap_finished();
                    result?;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(PollError::from_submit(SubmitResultError::TransportClosed(
                        "polling dispatch completion channel closed".to_string(),
                    )));
                }
            }
        }
        Ok(())
    }

    /// Preserve the former synchronous dispatch's bounded shutdown race:
    /// already-returned fatal auth/protocol/config outcomes win over clean
    /// shutdown. A worker-reported Shutdown is remembered while remaining
    /// completions get a short chance to expose a sibling fatal outcome.
    pub(crate) fn wait_for_shutdown_outcome(
        &mut self,
        project_cache: &mut AgentProjectCache,
        wait: Duration,
    ) -> Result<(), PollError> {
        let deadline = Instant::now() + wait;
        let mut shutdown_error = None;
        while self.in_flight > 0 && Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self
                .completion_rx
                .recv_timeout(remaining.min(Duration::from_millis(25)))
            {
                Ok(completion) => {
                    match self.record_completion(project_cache, completion) {
                        Ok(_) => {}
                        Err(error @ SubmitResultError::Shutdown(_)) => {
                            shutdown_error = Some(error);
                        }
                        Err(error) => {
                            let _ = self.background_threads.reap_finished();
                            return Err(PollError::from_submit(error));
                        }
                    }
                    let _ = self.background_threads.reap_finished();
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(PollError::from_submit(SubmitResultError::TransportClosed(
                        "polling dispatch completion channel closed".to_string(),
                    )));
                }
            }
        }
        let _ = self.background_threads.reap_finished();
        if let Some(error) = shutdown_error {
            Err(PollError::from_submit(error))
        } else {
            Ok(())
        }
    }
}

fn http_status_summary(status: reqwest::StatusCode) -> String {
    match status.canonical_reason() {
        Some(reason) => format!("HTTP {} {}", status.as_u16(), reason),
        None => format!("HTTP {}", status.as_u16()),
    }
}

fn looks_like_proxy_html_error(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    lower.contains("<html")
        && contains_any(
            &lower,
            &[
                "bad gateway",
                "service unavailable",
                "gateway timeout",
                "nginx",
                "upstream",
            ],
        )
}

fn looks_like_server_down_request(error: &reqwest::Error, chain: &str) -> bool {
    if error.is_connect() {
        return true;
    }
    let lower = chain.to_ascii_lowercase();
    contains_any(
        &lower,
        &[
            "connection refused",
            "connection reset",
            "connection aborted",
            "connection closed",
            "early eof",
            "unexpected eof",
            "incomplete message",
            "broken pipe",
        ],
    )
}

fn looks_like_fatal_tls_request(chain: &str) -> bool {
    let lower = chain.to_ascii_lowercase();
    contains_any(
        &lower,
        &[
            "certificate verify failed",
            "invalid peer certificate",
            "unknownissuer",
            "notvalidforname",
            "certificateunknown",
            "invalid certificate",
            "no application protocol",
            "alpn mismatch",
        ],
    )
}

fn looks_like_auth_failure_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    contains_any(
        &lower,
        &[
            "unauthorized",
            "forbidden",
            "invalid token",
            "bad token",
            "auth failed",
            "authentication",
        ],
    )
}

fn is_active_instance_lease_conflict(client_id: &str, error: Option<&str>) -> bool {
    let expected = format!(
        "agent client {} is already online with a different instance",
        client_id
    );
    error == Some(expected.as_str())
}

fn is_unknown_polling_session(client_id: &str, error: Option<&str>) -> bool {
    let expected = format!("unknown shell client: {}", client_id);
    error == Some(expected.as_str())
}

fn error_chain_text(error: &reqwest::Error) -> String {
    let mut parts = vec![error.to_string()];
    let mut source = StdError::source(error);
    while let Some(err) = source {
        parts.push(err.to_string());
        source = err.source();
    }
    parts.join(": ")
}

fn request_error_summary(error: reqwest::Error, chain: &str) -> String {
    let lower = chain.to_ascii_lowercase();
    if lower.contains("connection refused") {
        "connection refused".to_string()
    } else if lower.contains("connection reset") {
        "connection reset".to_string()
    } else if lower.contains("connection aborted") {
        "connection aborted".to_string()
    } else if lower.contains("broken pipe") {
        "broken pipe".to_string()
    } else if contains_any(
        &lower,
        &[
            "connection closed",
            "early eof",
            "unexpected eof",
            "incomplete message",
        ],
    ) {
        "connection closed before response completed".to_string()
    } else if error.is_connect() {
        "connection failed".to_string()
    } else if error.is_timeout() {
        "request timed out".to_string()
    } else {
        bounded_single_line(&error.without_url().to_string())
    }
}

fn bounded_single_line(text: &str) -> String {
    const MAX_CHARS: usize = 160;
    let mut out = String::new();
    let mut last_space = false;
    for ch in text.chars() {
        let ch = if ch.is_whitespace() || ch.is_control() {
            ' '
        } else {
            ch
        };
        if ch == ' ' {
            if last_space {
                continue;
            }
            last_space = true;
        } else {
            last_space = false;
        }
        out.push(ch);
        if out.chars().count() >= MAX_CHARS {
            out.push_str("...");
            break;
        }
    }
    out.trim().to_string()
}

fn bounded_endpoint_path(path: &str) -> String {
    let without_query = path.split_once('?').map_or(path, |(path, _)| path);
    bounded_single_line(without_query)
}

/// Extract the structured `error` field from a JSON error response body, if
/// present. Non-JSON bodies (proxy HTML, truncated payloads) yield `None` so
/// raw response bytes never leak into diagnostics.
fn structured_body_error(body: &str) -> Option<String> {
    const MAX_PARSE_BYTES: usize = 64 * 1024;
    if body.len() > MAX_PARSE_BYTES {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let error = value.get("error")?.as_str()?;
    let error = bounded_single_line(error);
    if error.is_empty() {
        None
    } else {
        Some(error)
    }
}

#[derive(Debug, PartialEq, Eq)]
struct BoundedResponseBody {
    bytes: Vec<u8>,
    exceeded_limit: bool,
}

fn read_bounded_response_body<R: Read>(
    reader: &mut R,
    content_length: Option<u64>,
    max_bytes: usize,
) -> std::io::Result<BoundedResponseBody> {
    let read_limit = (max_bytes as u64).saturating_add(1);
    let initial_capacity = content_length
        .and_then(|length| usize::try_from(length).ok())
        .unwrap_or_default()
        .min(max_bytes.saturating_add(1));
    let mut bytes = Vec::with_capacity(initial_capacity);
    reader.take(read_limit).read_to_end(&mut bytes)?;
    let exceeded_limit = bytes.len() > max_bytes;
    if exceeded_limit {
        bytes.truncate(max_bytes);
    }
    Ok(BoundedResponseBody {
        bytes,
        exceeded_limit,
    })
}

fn bounded_response_content_type(
    value: Option<&reqwest::header::HeaderValue>,
    token: &str,
) -> String {
    match value {
        Some(value) => value
            .to_str()
            .ok()
            .and_then(|value| {
                let media_type = value.split(';').next()?.trim();
                let lower = media_type.to_ascii_lowercase();
                let token = token.trim();
                if media_type.is_empty()
                    || lower.contains("authorization")
                    || lower.contains("bearer")
                    || (!token.is_empty() && media_type.contains(token))
                    || !media_type.chars().all(|ch| {
                        ch.is_ascii_alphanumeric()
                            || matches!(
                                ch,
                                '/' | '!' | '#' | '$' | '&' | '^' | '_' | '.' | '+' | '-'
                            )
                    })
                {
                    None
                } else {
                    Some(bounded_single_line(media_type))
                }
            })
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "<redacted-or-invalid>".to_string()),
        None => "<missing>".to_string(),
    }
}

fn response_decode_summary(
    status: reqwest::StatusCode,
    content_type: &str,
    detail: impl AsRef<str>,
) -> String {
    format!(
        "status={} content_type={} {}",
        http_status_summary(status),
        content_type,
        detail.as_ref()
    )
}

fn looks_like_transient_proxy_response(content_type: &str, body: &[u8]) -> bool {
    const MAX_INSPECT_BYTES: usize = 8 * 1024;
    let inspected = &body[..body.len().min(MAX_INSPECT_BYTES)];
    let text = String::from_utf8_lossy(inspected);
    let lower = text.to_ascii_lowercase();
    let has_temporary_gateway_marker = contains_any(
        &lower,
        &[
            "bad gateway",
            "service unavailable",
            "gateway timeout",
            "upstream connect error",
            "upstream connection error",
            "upstream unavailable",
            "proxy error",
            "temporarily unavailable",
        ],
    );
    let looks_html = content_type
        .split(';')
        .next()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("text/html"))
        || lower.contains("<html")
        || lower.contains("<!doctype html");
    if looks_html && has_temporary_gateway_marker {
        return true;
    }
    let plain = lower.trim();
    body.len() <= MAX_INSPECT_BYTES
        && (matches!(
            plain,
            "bad gateway"
                | "service unavailable"
                | "gateway timeout"
                | "upstream unavailable"
                | "temporarily unavailable"
        ) || plain.starts_with("upstream connect error")
            || plain.starts_with("upstream connection error"))
}

fn serde_json_category_name(error: &serde_json::Error) -> &'static str {
    match error.classify() {
        serde_json::error::Category::Io => "io",
        serde_json::error::Category::Syntax => "syntax",
        serde_json::error::Category::Data => "data",
        serde_json::error::Category::Eof => "eof",
    }
}

fn decode_json_response<R>(
    path: &str,
    status: reqwest::StatusCode,
    content_type: &str,
    body: BoundedResponseBody,
) -> Result<R, AgentHttpError>
where
    R: serde::de::DeserializeOwned,
{
    if body.bytes.iter().all(u8::is_ascii_whitespace) {
        return Err(AgentHttpError::decode_transient(
            path,
            response_decode_summary(status, content_type, "empty response body"),
        ));
    }
    if looks_like_transient_proxy_response(content_type, &body.bytes) {
        return Err(AgentHttpError::decode_transient(
            path,
            response_decode_summary(
                status,
                content_type,
                "recognized temporary proxy/upstream response",
            ),
        ));
    }
    if body.exceeded_limit {
        return Err(AgentHttpError::protocol_decode(
            path,
            response_decode_summary(
                status,
                content_type,
                format!(
                    "response body exceeds limit_bytes={}",
                    AGENT_HTTP_RESPONSE_BODY_MAX_BYTES
                ),
            ),
        ));
    }
    serde_json::from_slice(&body.bytes).map_err(|error| {
        let detail = format!(
            "serde_category={} line={} column={}",
            serde_json_category_name(&error),
            error.line(),
            error.column()
        );
        let summary = response_decode_summary(status, content_type, detail);
        if error.is_eof() {
            AgentHttpError::decode_transient(path, summary)
        } else {
            AgentHttpError::protocol_decode(path, summary)
        }
    })
}

fn post_json<T, R>(
    client: &Client,
    cfg: &AgentConfig,
    path: &str,
    body: &T,
) -> Result<R, AgentHttpError>
where
    T: serde::Serialize + ?Sized,
    R: serde::de::DeserializeOwned,
{
    post_json_with_auth(client, &cfg.server_url, &cfg.token, path, body)
}

fn post_json_with_auth<T, R>(
    client: &Client,
    server_url: &str,
    token: &str,
    path: &str,
    body: &T,
) -> Result<R, AgentHttpError>
where
    T: serde::Serialize + ?Sized,
    R: serde::de::DeserializeOwned,
{
    let url = format!("{}{}", server_url.trim_end_matches('/'), path);
    let mut req = client.post(url);
    if !token.trim().is_empty() {
        req = req.bearer_auth(token.trim());
    }
    let resp = req
        .json(body)
        .send()
        .map_err(|e| AgentHttpError::request(path, e))?;
    let status = resp.status();
    let content_type =
        bounded_response_content_type(resp.headers().get(reqwest::header::CONTENT_TYPE), token);
    let content_length = resp.content_length();
    if content_length.is_some_and(|length| length > AGENT_HTTP_RESPONSE_BODY_MAX_BYTES as u64) {
        if !status.is_success() {
            return Err(AgentHttpError::status(path, status, ""));
        }
        return Err(AgentHttpError::protocol_decode(
            path,
            response_decode_summary(
                status,
                &content_type,
                format!(
                    "declared response body exceeds limit_bytes={}",
                    AGENT_HTTP_RESPONSE_BODY_MAX_BYTES
                ),
            ),
        ));
    }
    let mut resp = resp;
    let body = match read_bounded_response_body(
        &mut resp,
        content_length,
        AGENT_HTTP_RESPONSE_BODY_MAX_BYTES,
    ) {
        Ok(body) => body,
        Err(error) if status.is_success() => {
            return Err(AgentHttpError::decode_transient(
                path,
                response_decode_summary(
                    status,
                    &content_type,
                    format!("response body read interrupted io_kind={:?}", error.kind()),
                ),
            ));
        }
        Err(_) => return Err(AgentHttpError::status(path, status, "")),
    };
    if !status.is_success() {
        let text = String::from_utf8_lossy(&body.bytes);
        return Err(AgentHttpError::status(path, status, &text));
    }
    decode_json_response(path, status, &content_type, body)
}

/// Hidden, test/ops-only knob: parse `WEBCODEX_RUNNER_DISABLE_JOB_STATE_RECONCILIATION`
/// as a boolean. Default false (reconciliation stays on). Inline rather than
/// shared because the runner crate does not depend on the server config helpers.
fn disable_job_state_reconciliation_for_test() -> bool {
    matches!(
        std::env::var("WEBCODEX_RUNNER_DISABLE_JOB_STATE_RECONCILIATION")
            .ok()
            .map(|raw| raw.trim().to_ascii_lowercase())
            .as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

fn agent_register_capabilities(cfg: &AgentConfig) -> ShellClientCapabilities {
    let mut capabilities = cfg.capabilities.clone().unwrap_or_default();
    capabilities.jobs = true;
    capabilities.file_read = true;
    capabilities.file_write = true;
    // This binary implements the narrow internal seek/read export-chunk path.
    // Older binaries omit the field so Control uses the existing slow fallback.
    capabilities.artifact_export_chunk_read = true;
    // This binary implements the complete bounded structured delete contract.
    // Older binaries omit the field and therefore keep using the Server's legacy path.
    capabilities.structured_file_delete = true;
    capabilities.async_jobs = true;
    capabilities.async_shell_jobs = true;
    // SSH support intentionally depends on the local OpenSSH executable.
    // Authentication and Host aliases remain entirely Runner-local.
    capabilities.ssh_shell = SshConnectionPool::is_available();
    // This binary installs the bounded, process-local persistent-shell
    // manager. Older binaries omit this field and therefore fail closed.
    capabilities.persistent_shell = cfg!(unix);
    // SSH persistent shells reuse the same OpenSSH executable as `ssh_shell`.
    // Older binaries omit this field and therefore fail closed; it is never
    // inferred from `ssh_shell` + `persistent_shell`.
    capabilities.ssh_persistent_shell = SshConnectionPool::is_available() && cfg!(unix);
    capabilities.structured_validation_argv = true;
    // This binary accepts both legacy Go validation argv from old Servers and
    // the current machine-readable JSON argv. Do not trust static config or
    // infer this from generic structured validation support.
    capabilities.structured_go_test_json = true;
    // This binary also understands the first-class go_test durable metadata
    // identity. Keep this independent from JSON parsing so an old Runner that
    // supported Connector Go evidence cannot be mistaken for a first-class
    // go_test executor by a newer Server.
    capabilities.structured_go_test_tool = true;
    // Focused first-class go_test packages extend the older fixed `./...`
    // wire shape, so advertise them independently for rolling upgrades.
    capabilities.structured_go_test_packages = true;
    capabilities.structured_process_argv = true;
    capabilities.structured_script_payload = true;
    capabilities.structured_execution_jobs = true;
    capabilities.project_lifecycle = true;
    // This binary implements resolve_or_register_project; do not trust config to
    // advertise a capability that the binary does not implement.
    capabilities.project_path_registration = true;
    // `job_state_reconciliation` is on by default. A hidden, test/ops-only env
    // knob lets an E2E simulate a legacy runner that predates the capability
    // (it then has no job inventory and a disconnect falls straight to `lost`).
    // Default production behavior is unchanged: only the explicit opt-out
    // disables it, and the server already rejects inventory without the
    // capability and vice-versa.
    // Native read-only desktop observation is implemented only on macOS and
    // Windows. Unsupported platforms advertise false and fail closed.
    capabilities.computer_observe = cfg!(any(target_os = "macos", windows));
    // Accessibility inspection is a separate read-only semantic capability.
    // It is currently implemented only by the macOS Runner and never implies
    // future computer-control authority.
    capabilities.computer_accessibility_observe = cfg!(target_os = "macos");
    // Accessibility control is independently fenced and currently implemented
    // only by the macOS Runner; observation authority never implies it.
    capabilities.computer_control = cfg!(target_os = "macos");
    // Bounded Accessibility text input is a separate rolling-upgrade fence;
    // older macOS Runners with computer_control must not be treated as capable.
    capabilities.computer_text_input = cfg!(target_os = "macos");
    capabilities.job_state_reconciliation = !disable_job_state_reconciliation_for_test();

    // New agents always advertise read-only LSP navigation. Older agents omit
    // the field and deserialize as false on the server.
    capabilities.lsp_read_only_navigation = true;
    // Advertise the distinct capability only because this binary installs the
    // bounded typed prepare/incoming/outgoing traversal implementation.
    capabilities.lsp_call_hierarchy = true;
    // Advertise only after a real child-process enforcement probe proves Linux
    // Landlock ABI v3 (including TRUNCATE) works on this host. Every request
    // still applies the policy again in pre_exec and fails closed on error.
    capabilities.sandbox_inspect_commands =
        crate::command_sandbox::inspect_sandbox_available().is_ok();
    capabilities
}

#[cfg(test)]
fn build_register_request(
    cfg: &AgentConfig,
    projects: Vec<ShellAgentProjectSummary>,
    protocol_version: &str,
    agent_instance_id: &str,
    prepared_cache_count: usize,
) -> ShellClientRegisterRequest {
    let runtime = ReloadableAgentConfig::new(cfg.clone(), PathBuf::new());
    build_register_request_with_provider_status(
        cfg,
        &runtime,
        projects,
        protocol_version,
        agent_instance_id,
        prepared_cache_count,
        ShellJobInventory {
            active_complete: true,
            jobs: Vec::new(),
        },
    )
    .0
}

fn build_register_request_with_provider_status(
    cfg: &AgentConfig,
    runtime: &ReloadableAgentConfig,
    projects: Vec<ShellAgentProjectSummary>,
    protocol_version: &str,
    agent_instance_id: &str,
    prepared_cache_count: usize,
    job_inventory: ShellJobInventory,
) -> (
    ShellClientRegisterRequest,
    Arc<webcodex_runner::external_tools::ExternalToolRouter>,
    u64,
) {
    let hot = runtime.snapshot();
    let capabilities = agent_register_capabilities(cfg);
    let (mut tool_providers, revision) = hot.external_tools.registration_status();
    tool_providers.config_reload = hot.reload_status();
    (
        ShellClientRegisterRequest {
            client_id: cfg.client_id.clone(),
            agent_instance_id: agent_instance_id.to_string(),
            display_name: cfg.display_name.clone(),
            owner: cfg.owner.clone(),
            hostname: cfg.hostname.clone().or_else(hostname),
            host_context: cfg.host_context.clone(),
            capabilities: Some(capabilities),
            projects: Some(projects),
            agent_protocol_version: Some(protocol_version.to_string()),
            policy: Some(register_policy_summary(
                &hot,
                prepared_cache_count,
                tool_providers,
            )),
            process_started_at: Some(process_started_at()),
            build: Some(runner_build_info()),
            job_concurrency_limit: Some(max_concurrent_jobs(cfg)),
            // A legacy runner (capability disabled for E2E) must not send a job
            // inventory: the server rejects inventory without the capability and
            // vice-versa, so a true legacy client advertises neither.
            job_inventory: if disable_job_state_reconciliation_for_test() {
                None
            } else {
                Some(job_inventory)
            },
        },
        Arc::clone(&hot.external_tools),
        revision,
    )
}

/// Unix timestamp when this runner process started. Captured on first call;
/// `run_agent` initializes it at startup so registration payloads report the
/// real process start, not the first register time after a reconnect.
fn process_started_at() -> i64 {
    static STARTED_AT: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *STARTED_AT.get_or_init(|| chrono::Utc::now().timestamp())
}

/// Non-secret runner build identity for mixed-version diagnostics.
fn runner_build_info() -> shell_protocol::AgentBuildInfo {
    let info = build_info::current();
    shell_protocol::AgentBuildInfo {
        version: Some(info.version.to_string()),
        git_commit: info.git_commit.map(str::to_string),
        git_dirty: info.git_dirty,
    }
}

/// Shell dialect derived from a program path basename. Only `sh` and `bash`
/// map to portable POSIX dialects; `powershell`/`powershell.exe`/`pwsh` map to
/// the Windows PowerShell dialect; anything else is `custom` and callers that
/// need deterministic syntax must select an explicit `shell=sh|bash` (or a
/// configured dialect on the Runner side).
fn shell_dialect_for_program(program: &str) -> &'static str {
    match std::path::Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(program)
    {
        "sh" => "sh",
        "bash" => "bash",
        "powershell" | "powershell.exe" | "pwsh" => "powershell",
        _ => "custom",
    }
}

/// Build the sanitized shell-profiles summary from the active shell config.
/// Exposes only safe metadata: profile names, whether each has an init_script
/// (boolean, never the body), env key counts (never values), the resolved
/// program, and arg counts. `prepared_cache_count` is the number of snapshots
/// prepared at call time (typically 0 right after start). Never includes env
/// values, init_script bodies, tokens, or the full env snapshot.
fn build_shell_profiles_summary(
    shell: &ShellConfig,
    prepared_cache_count: usize,
) -> ShellProfilesSummary {
    let profiles: Vec<ShellProfileSummaryEntry> = shell
        .profiles
        .iter()
        .map(|(name, profile)| {
            let program = profile
                .program
                .clone()
                .unwrap_or_else(|| shell.program.clone());
            let args = profile.args.clone().unwrap_or_else(|| shell.args.clone());
            let dialect = shell_dialect_for_program(&program);
            ShellProfileSummaryEntry {
                name: name.clone(),
                has_init_script: profile.init_script.is_some(),
                env_keys_count: profile.env.len(),
                program,
                args_count: args.len(),
                dialect: Some(dialect.to_string()),
            }
        })
        .collect();
    // Default execution path when the caller selects no explicit shell:
    // shell.default_profile if set, otherwise the plain shell program.
    // (A project-level shell_profile override is reported per project.)
    let default_program = shell
        .default_profile
        .as_deref()
        .and_then(|name| shell.profiles.get(name))
        .and_then(|profile| profile.program.clone())
        .unwrap_or_else(|| shell.program.clone());
    let default_dialect = shell_dialect_for_program(&default_program).to_string();
    // Explicit shell=sh|bash always resolves on the runner; configured custom
    // profiles add the custom dialect.
    let mut available: Vec<String> = vec!["sh".to_string(), "bash".to_string()];
    for entry in &profiles {
        if let Some(dialect) = entry.dialect.as_deref() {
            if !available.iter().any(|existing| existing == dialect) {
                available.push(dialect.to_string());
            }
        }
    }
    ShellProfilesSummary {
        default_profile: shell.default_profile.clone(),
        configured_count: shell.profiles.len(),
        prepared_cache_count,
        profiles,
        default_dialect: Some(default_dialect),
        available_dialects: Some(available),
    }
}

/// Build the sanitized agent policy summary sent at registration. Mirrors the
/// local `AgentPolicy` but only carries non-secret fields. The shell env
/// values and init_script path are intentionally NOT included. The sanitized
/// shell-profiles summary is attached so observability can show which profile
/// a project resolves to without exposing env values or init_script bodies.
fn register_policy_summary(
    cfg: &HotAgentConfig,
    prepared_cache_count: usize,
    tool_providers: shell_protocol::ToolProvidersStatus,
) -> AgentPolicySummary {
    AgentPolicySummary {
        allow_raw_shell: cfg.policy.allow_raw_shell,
        allow_cwd_anywhere: cfg.policy.allow_cwd_anywhere,
        allowed_roots: cfg.policy.allowed_roots.clone(),
        max_timeout_secs: cfg.policy.max_timeout_secs,
        max_output_bytes: cfg.policy.max_output_bytes,
        shell_profiles: Some(build_shell_profiles_summary(
            &cfg.shell,
            prepared_cache_count,
        )),
        tool_providers: Some(tool_providers),
    }
}

fn register(
    client: &Client,
    cfg: &AgentConfig,
    runtime: &ReloadableAgentConfig,
    project_cache: &mut AgentProjectCache,
    shutdown: Option<&AtomicBool>,
    agent_instance_id: &str,
    prepared_cache_count: usize,
    jobs: &JobManager,
) -> Result<(usize, ShellJobInventory), RegisterError> {
    let projects = project_cache.get_with_shutdown(cfg, shutdown);
    let projects_count = projects.iter().filter(|project| !project.disabled).count();
    let job_inventory = jobs.inventory();
    let (body, provider, provider_revision) = build_register_request_with_provider_status(
        cfg,
        runtime,
        projects,
        AGENT_PROTOCOL_VERSION_POLLING_V1,
        agent_instance_id,
        prepared_cache_count,
        job_inventory.clone(),
    );
    let response: ShellClientRegisterResponse = post_json(client, cfg, AGENT_REGISTER_PATH, &body)
        .map_err(|error| RegisterError::from_http(error, &cfg.client_id))?;
    if response.success {
        provider.mark_status_reported(provider_revision);
        Ok((projects_count, job_inventory))
    } else {
        Err(RegisterError::from_response_error(
            &cfg.client_id,
            response.error,
        ))
    }
}

fn is_file_request_kind(kind: &str) -> bool {
    is_basic_file_request_kind(kind)
        || is_structured_edit_request_kind(kind)
        || is_artifact_request_kind(kind)
        || is_checkpoint_request_kind(kind)
}

fn handle_file_request(policy: &AgentPolicy, request: &ShellAgentShellRequest) -> CommandResult {
    let Some(path) = request.path.as_deref() else {
        return CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(0),
            error: Some("file request missing path".to_string()),
        };
    };
    let start = Instant::now();
    if is_structured_edit_request_kind(&request.kind) {
        if let Err(e) = validate_structured_edit_agent_path(path) {
            return CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(0),
                error: Some(e),
            };
        }
    }
    let resolved = match resolve_requested_path(policy, request.cwd.as_deref(), path) {
        Ok(path) => path,
        Err(e) => {
            return CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(0),
                error: Some(e),
            }
        }
    };
    match request.kind.as_str() {
        "file_write_project_file" => handle_write_project_file_request(request, &resolved, start),
        "file_apply_text_edits" => handle_apply_text_edits_file_request(policy, request, start),
        "file_save_project_artifact"
        | "file_read_project_artifact_metadata"
        | "file_read_project_artifact"
        | "file_read_project_artifact_export_chunk"
        | "file_artifact_upload_begin"
        | "file_artifact_upload_chunk"
        | "file_artifact_upload_finish"
        | "file_artifact_upload_abort" => handle_artifact_file_request(request, &resolved, start),
        "file_checkpoint_create" | "file_checkpoint_restore" => {
            handle_checkpoint_file_request(request, &resolved, start)
        }
        "file_read"
        | "file_write"
        | "file_list"
        | "file_project_overview"
        | "file_delete_project_files" => {
            handle_basic_file_request(policy, request, &resolved, start)
        }
        _ => CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: Some(format!("unknown file request kind: {}", request.kind)),
        },
    }
}

#[derive(Debug, Default)]
struct CreatedProjectPaths {
    project_dir_created: Option<PathBuf>,
    paths: Vec<PathBuf>,
}

impl CreatedProjectPaths {
    fn mark_project_dir_created(&mut self, path: PathBuf) {
        self.project_dir_created = Some(path);
    }

    fn track(&mut self, path: PathBuf) {
        self.paths.push(path);
    }

    fn cleanup(&self) {
        for path in self.paths.iter().rev() {
            if path.is_dir() {
                let _ = std::fs::remove_dir_all(path);
            } else if path.exists() {
                let _ = std::fs::remove_file(path);
            }
        }
        if let Some(dir) = &self.project_dir_created {
            let _ = std::fs::remove_dir(dir);
        }
    }
}

fn write_created_file(
    path: &Path,
    content: &[u8],
    created_paths: &mut CreatedProjectPaths,
) -> Result<(), std::io::Error> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    created_paths.track(path.to_path_buf());
    file.write_all(content)
}

fn spawn_reader<R: Read + Send + 'static>(
    mut reader: R,
    tx: mpsc::SyncSender<OutputChunk>,
    stdout: bool,
    source: OutputTextSource,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        // A bounded channel plus fixed-size reads prevents a fast child (or
        // one enormous line) from retaining unbounded output in the runner
        // while a transport send is slow.
        let mut buf = [0_u8; 8 * 1024];
        let mut decoder = OutputTextDecoder::new(source);
        loop {
            match reader.read(&mut buf) {
                Ok(0) => {
                    let text = decoder.push(&[], true);
                    if !text.is_empty() {
                        let _ = if stdout {
                            tx.send(OutputChunk::Stdout(text))
                        } else {
                            tx.send(OutputChunk::Stderr(text))
                        };
                    }
                    break;
                }
                Ok(read) => {
                    let text = decoder.push(&buf[..read], false);
                    if !text.is_empty() {
                        let _ = if stdout {
                            tx.send(OutputChunk::Stdout(text))
                        } else {
                            tx.send(OutputChunk::Stderr(text))
                        };
                    }
                }
                Err(_) => {
                    let text = decoder.push(&[], true);
                    if !text.is_empty() {
                        let _ = if stdout {
                            tx.send(OutputChunk::Stdout(text))
                        } else {
                            tx.send(OutputChunk::Stderr(text))
                        };
                    }
                    break;
                }
            }
        }
    })
}

/// Join the output reader threads until `deadline`. Returns the number of
/// readers that had not finished by the deadline and were detached (their
/// `JoinHandle`s dropped without joining).
fn join_reader_threads_until(
    mut readers: Vec<std::thread::JoinHandle<()>>,
    deadline: Instant,
) -> usize {
    loop {
        let mut index = 0;
        while index < readers.len() {
            if readers[index].is_finished() {
                let reader = readers.swap_remove(index);
                let _ = reader.join();
            } else {
                index += 1;
            }
        }
        if readers.is_empty() {
            return 0;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            // Dropping a JoinHandle detaches it. The output channel is bounded,
            // so an abnormal pipe holder cannot retain unbounded runner memory
            // or block process shutdown.
            return readers.len();
        }
        std::thread::sleep(Duration::from_millis(10).min(remaining));
    }
}

fn wait_failure_error(validation: bool, error: &std::io::Error) -> String {
    if validation {
        VALIDATION_STEP_WAIT_FAILED_CODE.to_string()
    } else {
        format!("failed to wait job: {error}")
    }
}

fn validation_failed_step(status: &str, error: Option<&str>, step_name: &str) -> Option<String> {
    (status == "failed"
        && error
            .and_then(validation_infrastructure_failure_code)
            .is_none())
    .then(|| step_name.to_string())
}

fn validation_module_available(
    shell: &ShellConfig,
    profile: Option<&PreparedShellProfile>,
    cwd: &Path,
    step: &ShellJobValidationStep,
    inspect_scratch: Option<&crate::command_sandbox::InspectScratch>,
    shutdown: Option<&AtomicBool>,
) -> bool {
    if step.program != "python" {
        return true;
    }
    let Some(module) = step
        .args
        .windows(2)
        .find(|p| p[0] == "-m")
        .map(|p| p[1].as_str())
    else {
        return false;
    };
    const PROBE: &str =
        "import importlib.util,sys;sys.exit(0 if importlib.util.find_spec(sys.argv[1]) else 42)";
    let args = ["-I", "-c", PROBE, module].map(str::to_string);
    let Ok(mut command) = configured_validation_job_command(shell, profile, &step.program, &args)
    else {
        return false;
    };
    command
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(scratch) = inspect_scratch {
        if crate::command_sandbox::sandbox_command_inspect(&mut command, scratch).is_err() {
            return false;
        }
    }
    let Ok(child) = ManagedChild::spawn(&mut command) else {
        return false;
    };
    let child = Arc::new(Mutex::new(child));
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let wait_result = {
            let mut child = lock_unpoison(&child);
            child.try_wait()
        };
        match wait_result {
            Ok(Some(status)) => {
                let success = status.success();
                let _ = terminate_managed_tree(&child);
                return success;
            }
            Ok(None) => {
                if shutdown.is_some_and(|flag| flag.load(Ordering::SeqCst))
                    || Instant::now() >= deadline
                {
                    let _ = terminate_managed_tree(&child);
                    return false;
                }
            }
            Err(_) => {
                let _ = terminate_managed_tree(&child);
                return false;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Forcefully terminate the entire process tree owned by a job.
///
/// The platform detail (SIGKILL to a private process group on Unix,
/// `TerminateJobObject` on Windows) stays inside `webcodex-process`.
fn terminate_managed_tree(child: &Arc<Mutex<ManagedChild>>) -> Result<(), String> {
    lock_unpoison(child)
        .terminate_tree()
        .map_err(|error| error.to_string())
}

/// Request graceful tree termination, escalating to force termination where the
/// platform cannot deliver a graceful signal to the whole tree (Windows Job
/// Objects). An already-exited tree is idempotent success.
fn request_terminate_managed_tree(child: &Arc<Mutex<ManagedChild>>) -> Result<(), String> {
    // Bind the result first so the temporary `MutexGuard` is released before
    // the match arms: the Unsupported arm re-locks the same mutex, and a guard
    // still alive across the match would deadlock that re-lock.
    let outcome = lock_unpoison(child).request_terminate_tree();
    match outcome {
        Ok(GracefulTermination::Requested | GracefulTermination::AlreadyExited) => Ok(()),
        Ok(GracefulTermination::Unsupported) => terminate_managed_tree(child),
        Err(error) => Err(error.to_string()),
    }
}

/// Wait, bounded by `deadline`, until the managed process tree is empty.
///
/// Returns `Ok(true)` when the tree exited within the budget, `Ok(false)` when
/// the deadline elapsed first, and `Err` on a platform failure. The lock is
/// held while polling, so a concurrent `terminate_tree` blocks only until this
/// bounded wait returns; nothing here waits on another thread's progress.
fn wait_managed_tree_exit(
    child: &Arc<Mutex<ManagedChild>>,
    deadline: Instant,
) -> Result<bool, String> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Ok(false);
    }
    lock_unpoison(child)
        .wait_tree_exit(remaining)
        .map_err(|error| error.to_string())
}

/// Non-blocking probe: is the managed process tree still running?
///
/// Opportunistically reaps the direct child so a Unix zombie is not mistaken
/// for a live tree member. A busy lock or a platform probe failure is treated
/// conservatively as "still running".
fn managed_tree_running(child: &Arc<Mutex<ManagedChild>>) -> bool {
    let mut guard = match child.try_lock() {
        Ok(guard) => guard,
        Err(std::sync::TryLockError::WouldBlock) => return true,
        Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
    };
    if guard.try_wait().is_err() {
        return true;
    }
    match guard.try_tree_exit() {
        Ok(true) => false,
        Ok(false) => true,
        Err(_) => true,
    }
}

/// Complete a job step's tree lifecycle after the direct child's status has
/// been decided: give the tree a short bounded window to exit on its own,
/// force-terminate whatever remains, and confirm the tree emptied, so
/// pipe-holding descendants cannot stall the output readers forever.
fn cleanup_managed_tree(child: &Arc<Mutex<ManagedChild>>) {
    const NATURAL_EXIT_GRACE: Duration = Duration::from_millis(500);
    const FORCE_EXIT_GRACE: Duration = Duration::from_millis(500);
    let natural_deadline = Instant::now() + NATURAL_EXIT_GRACE;
    if !wait_managed_tree_exit(child, natural_deadline).unwrap_or(false) {
        let _ = terminate_managed_tree(child);
        let force_deadline = Instant::now() + FORCE_EXIT_GRACE;
        let _ = wait_managed_tree_exit(child, force_deadline);
    }
}

/// Best-effort bounded reap of the direct child. Returns `Ok(true)` once the
/// direct child has been reaped, `Ok(false)` if the deadline elapsed first
/// (including when another thread reaped it concurrently), and `Err` on a wait
/// failure. The lock is only ever taken briefly with `try_lock`.
fn reap_managed_direct_child(
    child: &Arc<Mutex<ManagedChild>>,
    deadline: Instant,
) -> Result<bool, String> {
    loop {
        let mut guard = match child.try_lock() {
            Ok(guard) => guard,
            Err(std::sync::TryLockError::WouldBlock) => return Ok(false),
            Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        };
        match guard.try_wait() {
            Ok(Some(_)) => return Ok(true),
            Ok(None) => {}
            Err(error) => return Err(error.to_string()),
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(10).min(remaining));
    }
}

#[derive(Clone)]
struct JobShutdownTarget {
    child: Arc<Mutex<ManagedChild>>,
}

struct JobShutdownBatch {
    targets: Vec<JobShutdownTarget>,
    running: usize,
    failures: usize,
}

#[derive(Debug, Clone, Copy)]
struct JobShutdownOutcome {
    resources: usize,
    timed_out: usize,
    failures: usize,
}

fn shutdown_target_running(target: &mut JobShutdownTarget) -> bool {
    managed_tree_running(&target.child)
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
    validation_progress: Option<ShellJobValidationProgress>,
    finished: bool,
}

fn runner_job_is_terminal(status: &str) -> bool {
    matches!(
        status,
        "completed" | "failed" | "stopped" | "timeout" | "timed_out" | "lost" | "cancelled"
    )
}

fn runner_job_is_active(status: &str) -> bool {
    matches!(status, "agent_queued" | "running" | "stop_requested")
}

fn structured_prestart_lifecycle(
    request: &ShellAgentShellRequest,
) -> Option<ShellCommandExecutionState> {
    matches!(
        request.kind.as_str(),
        "start_process_job" | "start_script_job"
    )
    .then_some(ShellCommandExecutionState::NotStarted)
}

fn job_update_from_snapshot(
    client_id: &str,
    agent_instance_id: &str,
    snapshot: &ShellJobSnapshot,
) -> ShellAgentJobUpdateRequest {
    ShellAgentJobUpdateRequest {
        client_id: client_id.to_string(),
        agent_instance_id: agent_instance_id.to_string(),
        job_id: snapshot.job_id.clone(),
        request_id: Some(snapshot.request_id.clone()),
        update_seq: Some(snapshot.update_seq),
        status: snapshot.status.clone(),
        stdout_chunk: None,
        stderr_chunk: None,
        stdout_tail: None,
        stderr_tail: None,
        log_snapshot: Some(ShellJobLogSnapshot {
            stdout: snapshot.stdout.clone(),
            stderr: snapshot.stderr.clone(),
        }),
        exit_code: snapshot.exit_code,
        duration_ms: snapshot.duration_ms,
        error: snapshot.error.clone(),
        command_execution_state: snapshot.command_execution_state,
        validation_progress: snapshot.validation_progress.clone(),
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

fn validate_runner_job_context(
    context: &ShellJobContext,
    request: &ShellAgentShellRequest,
    client_id: &str,
) -> Result<(), String> {
    const MAX_CONTEXT_FIELD_CHARS: usize = 1_024;
    const MAX_COMMAND_PREVIEW_CHARS: usize = 121;
    let bounded =
        |value: &str, max_chars: usize| !value.contains('\0') && value.chars().count() <= max_chars;
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
    if context.cwd != request.cwd {
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
            "sh" | "bash" | "powershell" | "configured" | "custom" | "remote" | "direct_argv"
        )
    }) {
        return Err("job recovery context shell is invalid".to_string());
    }
    let validation_context = request.kind == "start_validation_job";
    if (validation_context && !(1..=3).contains(&context.validation_steps.len()))
        || (!validation_context && !context.validation_steps.is_empty())
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
    let expected_structured = match request.kind.as_str() {
        "start_process_job" => {
            if !request.command.is_empty()
                || request.script.is_some()
                || request.process.is_none()
                || context.ssh_resource.is_some()
            {
                return Err("typed process Job request shape is invalid".to_string());
            }
            let process = request.process.as_ref().expect("checked process payload");
            shell_protocol::validate_process_argv(process)?;
            validate_runner_structured_common(request)?;
            Some(shell_protocol::ShellJobStructuredExecutionMetadata {
                execution_source: "run_process".to_string(),
                language: None,
                script_bytes: None,
                arg_count: process.args.len(),
                stdin_present: request.stdin.is_some(),
            })
        }
        "start_script_job" => {
            if !request.command.is_empty()
                || request.process.is_some()
                || request.script.is_none()
                || context.ssh_resource.is_some()
            {
                return Err("typed script Job request shape is invalid".to_string());
            }
            let script = request.script.as_ref().expect("checked script payload");
            shell_protocol::validate_script_request(
                script,
                request.stdin.as_deref(),
                request.cwd.as_deref(),
                request.timeout_secs,
            )?;
            Some(shell_protocol::ShellJobStructuredExecutionMetadata {
                execution_source: "run_script".to_string(),
                language: Some(script.language),
                script_bytes: Some(script.script.len()),
                arg_count: script.args.len(),
                stdin_present: request.stdin.is_some(),
            })
        }
        _ => None,
    };
    if context.structured_execution != expected_structured {
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
            || session_id.len() > 128
            || !session_id.starts_with("wc_sess_")
            || !session_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err("job recovery context workflow_session_id is invalid".to_string());
        }
    }
    Ok(())
}

fn validate_runner_structured_common(request: &ShellAgentShellRequest) -> Result<(), String> {
    if let Some(stdin) = request.stdin.as_deref() {
        if stdin.len() > shell_protocol::PROCESS_STDIN_MAX_BYTES {
            return Err(format!(
                "stdin is too large; maximum is {} bytes",
                shell_protocol::PROCESS_STDIN_MAX_BYTES
            ));
        }
        if stdin.contains('\0') {
            return Err("stdin cannot contain NUL bytes".to_string());
        }
    }
    if let Some(cwd) = request.cwd.as_deref() {
        if cwd.len() > shell_protocol::PROCESS_CWD_MAX_BYTES {
            return Err(format!(
                "cwd is too long; maximum is {} bytes",
                shell_protocol::PROCESS_CWD_MAX_BYTES
            ));
        }
        if cwd.contains('\0') {
            return Err("cwd cannot contain NUL bytes".to_string());
        }
    }
    if !(shell_protocol::STRUCTURED_EXECUTION_TIMEOUT_MIN_SECS
        ..=shell_protocol::STRUCTURED_EXECUTION_TIMEOUT_MAX_SECS)
        .contains(&request.timeout_secs)
    {
        return Err(format!(
            "timeout_secs must be between {} and {}",
            shell_protocol::STRUCTURED_EXECUTION_TIMEOUT_MIN_SECS,
            shell_protocol::STRUCTURED_EXECUTION_TIMEOUT_MAX_SECS
        ));
    }
    Ok(())
}

impl JobManager {
    fn install_sink(&self, sink: AgentSink) {
        *lock_unpoison(&self.current_sink) = Some(sink);
        self.delivery_signal.notify();
    }

    fn current_sink(&self) -> Option<AgentSink> {
        lock_unpoison(&self.current_sink).clone()
    }

    fn queue_recorded_update(&self, update: ShellAgentJobUpdateRequest, semantic: bool) {
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
    ) -> Option<(ShellAgentJobUpdateRequest, bool)> {
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
            job.snapshot.update_seq = job.snapshot.update_seq.saturating_add(1);
            if !delta.status.trim().is_empty() {
                let incoming_status = delta.status.trim();
                let would_regress_stop = job.snapshot.status == "stop_requested"
                    && matches!(incoming_status, "agent_queued" | "running");
                let would_regress_running =
                    job.snapshot.status == "running" && incoming_status == "agent_queued";
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
                && matches!(
                    job.snapshot.status.as_str(),
                    "running"
                        | "completed"
                        | "failed"
                        | "stopped"
                        | "timeout"
                        | "timed_out"
                        | "cancelled"
                )
            {
                job.snapshot.started_at = Some(now);
            }
            if delta.validation_progress.is_some() {
                job.snapshot.validation_progress = delta.validation_progress.clone();
            }
            if runner_job_is_terminal(&job.snapshot.status) || delta.finished {
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
            // tails. Delivery may coalesce output-only attempts, but semantic
            // markers preserve their sequence while using the latest retained
            // authoritative log snapshot at send time.
            (
                job_update_from_snapshot(&job.client_id, &job.agent_instance_id, &job.snapshot),
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

    fn replay_snapshots_since(&self, registered: &ShellJobInventory) {
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
            job_update_from_snapshot(&job.client_id, &job.agent_instance_id, &job.snapshot)
        });
        if let Some(update) = update {
            self.queue_recorded_update(update, true);
        }
    }

    fn fail_job(
        &self,
        request: &ShellAgentShellRequest,
        error: String,
        validation_progress: Option<ShellJobValidationProgress>,
    ) {
        let Some(job_id) = request.job_id.as_deref() else {
            return;
        };
        self.update_and_send(
            job_id,
            RunnerJobDelta {
                status: "failed".to_string(),
                duration_ms: Some(0),
                error: Some(error),
                command_execution_state: structured_prestart_lifecycle(request),
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
            for job_id in removed {
                pending.remove(&job_id);
            }
        }
    }

    fn inventory(&self) -> ShellJobInventory {
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

    fn has_work(&self) -> bool {
        lock_unpoison(&self.jobs)
            .values()
            .any(|job| runner_job_is_active(&job.snapshot.status))
            || !lock_unpoison(&self.queued).is_empty()
    }

    fn stop_accepting_work(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
    }

    fn cancel_queued_for_shutdown(&self) -> usize {
        let _lifecycle = lock_unpoison(&self.lifecycle);
        self.shutting_down.store(true, Ordering::SeqCst);
        let mut queued = lock_unpoison(&self.queued);
        let cancelled = queued.len();
        queued.clear();
        cancelled
    }

    fn signal_all_for_shutdown(&self) -> JobShutdownBatch {
        let running = {
            let jobs = lock_unpoison(&self.jobs);
            jobs.iter()
                .filter(|(_, job)| runner_job_is_active(&job.snapshot.status))
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

    fn drain_shutdown(&self, mut batch: JobShutdownBatch, deadline: Instant) -> JobShutdownOutcome {
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

    fn wait_for_workers(&self, deadline: Instant) -> bool {
        self.workers.wait_until(deadline)
    }

    fn worker_count(&self) -> usize {
        self.workers.active()
    }

    fn shutdown_rejection(&self, request: &ShellAgentShellRequest) {
        self.fail_job(request, "runner is shutting down".to_string(), None);
    }

    fn enqueue(&self, sink: AgentSink, start: PendingJobStart) {
        let Some(job_id) = start.request.job_id.clone() else {
            return;
        };
        let Some(context) = start.request.job_context.clone() else {
            let command_execution_state = structured_prestart_lifecycle(&start.request);
            let _ = sink.send_job_update(&ShellAgentJobUpdateRequest {
                client_id: sink.client_id().to_string(),
                agent_instance_id: sink.agent_instance_id().to_string(),
                job_id,
                request_id: Some(start.request.request_id),
                update_seq: Some(1),
                status: "failed".to_string(),
                stdout_chunk: None,
                stderr_chunk: None,
                stdout_tail: None,
                stderr_tail: None,
                log_snapshot: None,
                exit_code: None,
                duration_ms: Some(0),
                error: Some("job start request is missing recovery context".to_string()),
                command_execution_state,
                validation_progress: None,
                finished: true,
            });
            return;
        };
        if let Err(error) = validate_runner_job_context(&context, &start.request, sink.client_id())
        {
            let command_execution_state = structured_prestart_lifecycle(&start.request);
            let _ = sink.send_job_update(&ShellAgentJobUpdateRequest {
                client_id: sink.client_id().to_string(),
                agent_instance_id: sink.agent_instance_id().to_string(),
                job_id,
                request_id: Some(start.request.request_id),
                update_seq: Some(1),
                status: "failed".to_string(),
                stdout_chunk: None,
                stderr_chunk: None,
                stdout_tail: None,
                stderr_tail: None,
                log_snapshot: None,
                exit_code: None,
                duration_ms: Some(0),
                error: Some(error),
                command_execution_state,
                validation_progress: None,
                finished: true,
            });
            return;
        }
        self.install_sink(sink.clone());
        let client_id = sink.client_id().to_string();
        let agent_instance_id = sink.agent_instance_id().to_string();
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
                    agent_instance_id,
                    snapshot: ShellJobSnapshot {
                        job_id: job_id.clone(),
                        request_id: start.request.request_id.clone(),
                        status: if terminal {
                            "failed".to_string()
                        } else {
                            "agent_queued".to_string()
                        },
                        update_seq: u64::from(terminal),
                        created_at: start.request.created_at,
                        started_at: None,
                        ended_at: terminal.then_some(now),
                        exit_code: None,
                        duration_ms: terminal.then_some(0),
                        error: immediate_failure.clone(),
                        command_execution_state: terminal
                            .then(|| structured_prestart_lifecycle(&start.request))
                            .flatten(),
                        context,
                        stdout: ShellJobStreamSnapshot::default(),
                        stderr: ShellJobStreamSnapshot::default(),
                        validation_progress: None,
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
            self.shutdown_rejection(&start.request);
            return;
        }
        if matches!(
            start.request.kind.as_str(),
            "start_process_job" | "start_script_job"
        ) {
            let PendingJobStart {
                generation,
                policy,
                shell,
                projects_dir,
                request,
                ..
            } = start;
            self.start_structured_job(generation, policy, shell, projects_dir, request);
        } else {
            self.start_shell_job(start);
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
                            job.client_id == queued_start.request.client_id
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
                    if let Some(job_id) = queued[idx].request.job_id.as_deref() {
                        if let Some(job) = jobs.get_mut(job_id) {
                            job.slot_reserved = true;
                        }
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

    fn start_structured_job(
        &self,
        generation: u64,
        policy: AgentPolicy,
        shell: ShellConfig,
        projects_dir: PathBuf,
        request: ShellAgentShellRequest,
    ) {
        let Some(job_id) = request.job_id.clone() else {
            return;
        };
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
            self.shutdown_rejection(&request);
            return;
        };
        if (request.kind == "start_process_job" && request.process.is_none())
            || (request.kind == "start_script_job" && request.script.is_none())
        {
            self.fail_job(
                &request,
                "typed structured Job request is missing its payload".to_string(),
                None,
            );
            return;
        }
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
                        ..Default::default()
                    },
                );
            };
            let result = match request.kind.as_str() {
                "start_process_job" => {
                    let process = request.process.as_ref().expect("validated process payload");
                    run_process_with_profiles_in_sandbox_and_execution_state_with_start_hook(
                        generation,
                        &policy,
                        &shell,
                        &projects_dir,
                        &manager.prepared_profiles,
                        request.cwd.as_deref(),
                        &process.executable,
                        &process.args,
                        request.stdin.as_deref(),
                        request.timeout_secs,
                        Some(stop_requested.as_ref()),
                        request.sandbox.as_deref(),
                        Some(&on_started),
                    )
                }
                "start_script_job" => {
                    let script = request.script.as_ref().expect("validated script payload");
                    run_script_with_profiles_in_sandbox_and_execution_state_with_start_hook(
                        generation,
                        &policy,
                        &shell,
                        &projects_dir,
                        &manager.prepared_profiles,
                        request.cwd.as_deref(),
                        script,
                        request.stdin.as_deref(),
                        request.timeout_secs,
                        Some(stop_requested.as_ref()),
                        request.sandbox.as_deref(),
                        Some(&on_started),
                    )
                }
                _ => unreachable!("structured Job dispatcher received legacy request"),
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
            projects_dir,
            request,
        } = start;
        let Some(job_id) = request.job_id.clone() else {
            return;
        };
        if !policy.allow_raw_shell {
            self.fail_job(
                &request,
                "raw shell is disabled by local agent policy".to_string(),
                None,
            );
            return;
        }
        if request
            .job_context
            .as_ref()
            .is_some_and(|context| context.ssh_resource.is_some())
        {
            self.start_ssh_shell_job(generation, policy, ssh, request);
            return;
        }
        let cwd_path = request
            .cwd
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
        if let Err(e) = cwd_allowed(&policy, &cwd_path) {
            self.fail_job(&request, e, None);
            return;
        }
        let validation = request.kind == "start_validation_job";
        let steps = if validation {
            match serde_json::from_str::<Vec<ShellJobValidationStep>>(&request.command) {
                Ok(steps)
                    if (1..=3).contains(&steps.len())
                        && steps.iter().all(ShellJobValidationStep::is_canonical)
                        && steps.iter().enumerate().all(|(index, step)| {
                            !steps[..index]
                                .iter()
                                .any(|earlier| earlier.name == step.name)
                        }) =>
                {
                    steps
                }
                _ => {
                    self.fail_job(
                        &request,
                        "invalid structured validation plan".to_string(),
                        None,
                    );
                    return;
                }
            }
        } else {
            Vec::new()
        };
        if validation
            && request.job_context.as_ref().is_none_or(|context| {
                context.validation_steps
                    != steps
                        .iter()
                        .map(|step| step.name.clone())
                        .collect::<Vec<_>>()
            })
        {
            self.fail_job(
                &request,
                "structured validation plan does not match recovery context".to_string(),
                Some(ShellJobValidationProgress {
                    completed: 0,
                    current_step: None,
                    failed_step: None,
                }),
            );
            return;
        }
        let sandbox_mode = request.sandbox.clone();
        let inspect_scratch = match sandbox_mode.as_deref() {
            None => None,
            Some(crate::command_sandbox::INSPECT_SANDBOX_MODE) => {
                match crate::command_sandbox::InspectScratch::create() {
                    Ok(scratch) => Some(scratch),
                    Err(error) => {
                        self.fail_job(
                            &request,
                            format!("inspect sandbox unavailable: {error}"),
                            None,
                        );
                        return;
                    }
                }
            }
            Some(other) => {
                self.fail_job(&request, format!("unknown sandbox mode '{other}'"), None);
                return;
            }
        };
        // Profile preparation runs an init script. Inspect execution bypasses
        // that unsandboxed preparation and uses the base shell environment;
        // the actual command (and global init script, if any) is sandboxed.
        let prepared_profile = match inspect_scratch.as_ref() {
            Some(_) => None,
            None => match resolve_prepared_shell_profile(
                generation,
                &shell,
                &projects_dir,
                &cwd_path,
                request.cwd.is_some(),
                &self.prepared_profiles,
                Some(self.shutting_down.as_ref()),
            ) {
                Ok(profile) => profile,
                Err(e) => {
                    self.fail_job(&request, e, None);
                    return;
                }
            },
        };
        if validation
            && steps.iter().any(|step| {
                !validation_module_available(
                    &shell,
                    prepared_profile.as_deref(),
                    &cwd_path,
                    step,
                    inspect_scratch.as_ref(),
                    Some(self.shutting_down.as_ref()),
                )
            })
        {
            self.fail_job(
                &request,
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
                )
            } else {
                match prepared_profile.as_deref() {
                    Some(profile) => {
                        configured_prepared_shell_job_command(profile, &request.command)
                    }
                    None => configured_shell_job_command(&shell, &request.command),
                }
            };
            let mut command = match configured {
                Ok(command) => command,
                Err(error) => {
                    self.fail_job(&request, error, None);
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
            if let Some(scratch) = inspect_scratch.as_ref() {
                if let Err(error) =
                    crate::command_sandbox::sandbox_command_inspect(&mut command, scratch)
                {
                    self.fail_job(
                        &request,
                        format!("inspect sandbox unavailable: {error}"),
                        None,
                    );
                    return;
                }
            }
            command
                .current_dir(&cwd_path)
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
            self.shutdown_rejection(&request);
            return;
        };
        let start = Instant::now();
        let mut command = commands.pop_front().expect("validated non-empty plan");
        let spawn = ManagedChild::spawn(&mut command);
        let mut child = match spawn {
            Ok(child) => child,
            Err(e) => {
                if validation {
                    self.fail_job(
                        &request,
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
                    self.fail_job(&request, error, None);
                }
                return;
            }
        };
        let mut stdout = child.child_mut().stdout.take();
        let mut stderr = child.child_mut().stderr.take();
        let mut child = Arc::new(Mutex::new(child));
        let reject_for_shutdown = {
            let _lifecycle = lock_unpoison(&self.lifecycle);
            if self.shutting_down.load(Ordering::SeqCst) || stop_requested.load(Ordering::SeqCst) {
                true
            } else if let Some(job) = lock_unpoison(&self.jobs).get_mut(&job_id) {
                job.child = Some(child.clone());
                false
            } else {
                true
            }
        };
        if reject_for_shutdown {
            let _ = terminate_managed_tree(&child);
            self.shutdown_rejection(&request);
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
                ..Default::default()
            },
        );
        let jobs = self.jobs.clone();
        let lifecycle = Arc::clone(&self.lifecycle);
        let shutting_down = Arc::clone(&self.shutting_down);
        let manager = self.clone_for_worker();
        let inspect_scratch_guard = inspect_scratch;
        let worker_guard = self.workers.enter();
        std::thread::spawn(move || {
            let _worker_guard = worker_guard;
            // Keep the private writable directory alive for every process in
            // the job, then clean it when the terminal update has been sent.
            let _inspect_scratch_guard = inspect_scratch_guard;
            let timeout_secs = request.timeout_secs.min(policy.max_timeout_secs).max(1);
            let mut step_index = 0;
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
                join_reader_threads_until(readers, Instant::now() + Duration::from_secs(1));
                let mut out = String::new();
                let mut err = String::new();
                while let Ok(chunk) = rx.try_recv() {
                    match chunk {
                        OutputChunk::Stdout(text) => out.push_str(&text),
                        OutputChunk::Stderr(text) => err.push_str(&text),
                    }
                }
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
            manager.update_and_send(
                &job_id,
                RunnerJobDelta {
                    status: final_status.0,
                    stdout_chunk: (!out.is_empty()).then_some(out),
                    stderr_chunk: (!err.is_empty()).then_some(err),
                    exit_code: final_status.1,
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    error: final_status.2,
                    validation_progress: final_progress,
                    finished: true,
                    ..Default::default()
                },
            );
            manager.start_available_queued();
        });
    }

    /// Start one remote SSH command as a normal runner job. The transport is
    /// established before this child is spawned, so a preparation failure is
    /// unambiguously a command-not-started error. Once `ssh` is running, we
    /// never retry because remote delivery may already have happened.
    fn start_ssh_shell_job(
        &self,
        generation: u64,
        policy: AgentPolicy,
        ssh: SshConfig,
        request: ShellAgentShellRequest,
    ) {
        let Some(job_id) = request.job_id.clone() else {
            return;
        };
        let Some(resource_name) = request
            .job_context
            .as_ref()
            .and_then(|context| context.ssh_resource.as_deref())
        else {
            return;
        };
        let Some(session_id) = request
            .job_context
            .as_ref()
            .and_then(|context| context.workflow_session_id.as_deref())
        else {
            self.fail_job(
                &request,
                "ssh_session_required: an SSH resource requires a Workflow Session id; command was not started".to_string(),
                None,
            );
            return;
        };
        if request.kind != "start_job" {
            self.fail_job(
                &request,
                "ssh_resource_unsupported_for_request: SSH resources do not support structured validation jobs; command was not started".to_string(),
                None,
            );
            return;
        }
        if request.sandbox.is_some() {
            self.fail_job(
                &request,
                "ssh_sandbox_unavailable: SSH resources cannot run in the local inspect sandbox; command was not started".to_string(),
                None,
            );
            return;
        }
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
                self.fail_job(&request, error, None);
                return;
            }
        };
        let connection_key = prepared.key.clone();
        let mut command = prepared.command;
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

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
            self.shutdown_rejection(&request);
            return;
        };
        let start = Instant::now();
        let spawn = ManagedChild::spawn(&mut command);
        let mut child = match spawn {
            Ok(child) => child,
            Err(error) => {
                self.fail_job(
                    &request,
                    format!(
                        "ssh_command_spawn_failed: could not start local ssh client: {error}; command was not started"
                    ),
                    None,
                );
                return;
            }
        };
        let mut stdout = child.child_mut().stdout.take();
        let mut stderr = child.child_mut().stderr.take();
        let child = Arc::new(Mutex::new(child));
        let reject_for_shutdown = {
            let _lifecycle = lock_unpoison(&self.lifecycle);
            if self.shutting_down.load(Ordering::SeqCst) || stop_requested.load(Ordering::SeqCst) {
                true
            } else if let Some(job) = lock_unpoison(&self.jobs).get_mut(&job_id) {
                job.child = Some(Arc::clone(&child));
                false
            } else {
                true
            }
        };
        if reject_for_shutdown {
            let _ = terminate_managed_tree(&child);
            self.shutdown_rejection(&request);
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
        let worker_guard = self.workers.enter();
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
            let timeout_secs = request.timeout_secs.min(policy.max_timeout_secs).max(1);
            let mut transport_stderr = String::new();
            let (mut status, exit_code, mut error) = loop {
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
                        if stop_requested.load(Ordering::SeqCst) {
                            break (
                                "stopped".to_string(),
                                Some(-1),
                                Some("job stopped by request".to_string()),
                            );
                        }
                        if status.success() {
                            break ("completed".to_string(), Some(0), None);
                        }
                        break ("failed".to_string(), status.code(), None);
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
                                Some(format!("job timed out after {timeout_secs} seconds")),
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
                        );
                    }
                }
                std::thread::sleep(Duration::from_millis(JOB_UPDATE_INTERVAL_MS));
            };
            // The SSH client is the root of a private process tree. Ensure a
            // background child holding either pipe cannot delay the terminal
            // update indefinitely.
            cleanup_managed_tree(&child);
            join_reader_threads_until(readers, Instant::now() + Duration::from_secs(1));
            let mut final_out = String::new();
            let mut final_err = String::new();
            while let Ok(chunk) = rx.try_recv() {
                match chunk {
                    OutputChunk::Stdout(text) => final_out.push_str(&text),
                    OutputChunk::Stderr(text) => final_err.push_str(&text),
                }
            }
            if !final_err.is_empty() {
                append_bounded_tail(&mut transport_stderr, &final_err, 16 * 1024);
            }
            if is_transport_failure(exit_code, Some(&transport_stderr)) {
                ssh_pool.invalidate_after_transport_failure(&connection_key);
                status = "failed".to_string();
                error = Some(
                    "ssh_transport_failed: command may have started and was not retried"
                        .to_string(),
                );
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
                    finished: true,
                    ..Default::default()
                },
            );
            manager.start_available_queued();
        });
    }

    fn stop(&self, job_id: &str) -> Result<(), String> {
        let queued_job = {
            let _lifecycle = lock_unpoison(&self.lifecycle);
            let mut queued = lock_unpoison(&self.queued);
            if let Some(pos) = queued
                .iter()
                .position(|queued_start| queued_start.request.job_id.as_deref() == Some(job_id))
            {
                queued.remove(pos)
            } else {
                None
            }
        };
        if let Some(queued_start) = queued_job {
            let PendingJobStart { request, .. } = queued_start;
            self.update_and_send(
                job_id,
                RunnerJobDelta {
                    status: "stopped".to_string(),
                    stderr_chunk: Some("job stopped before start".to_string()),
                    exit_code: Some(-1),
                    duration_ms: Some(0),
                    error: Some("job stopped before start".to_string()),
                    command_execution_state: structured_prestart_lifecycle(&request),
                    finished: true,
                    ..Default::default()
                },
            );
            self.start_available_queued();
            return Ok(());
        }
        let (child, stop_requested) = {
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
fn handle_one_poll(
    client: &Client,
    cfg: &AgentConfig,
    runtime: &Arc<ReloadableAgentConfig>,
    jobs: &JobManager,
    persistent_shells: &webcodex_runner::PersistentShellManager,
    project_cache: &mut AgentProjectCache,
    poll_projects: Option<Vec<ShellAgentProjectSummary>>,
    agent_instance_id: &str,
    lsp: &webcodex_runner::LspSupervisor,
    shutdown: &Arc<AtomicBool>,
    dispatches: &ActivityTracker,
    polling_dispatches: &mut PollingDispatchSupervisor,
    once: bool,
) -> Result<bool, PollError> {
    let metadata_config = runtime.snapshot();
    let provider_update =
        metadata_config
            .external_tools
            .claim_status_update()
            .map(|(mut status, revision)| {
                status.config_reload = metadata_config.reload_status();
                (
                    status,
                    Arc::clone(&metadata_config.external_tools),
                    revision,
                )
            });
    let poll = ShellAgentPollPayload {
        request: ShellAgentPollRequest {
            client_id: cfg.client_id.clone(),
            agent_instance_id: agent_instance_id.to_string(),
            projects: poll_projects,
        },
        tool_providers: provider_update
            .as_ref()
            .map(|(status, _, _)| status.clone()),
    };
    let response: ShellAgentPollResponse = match post_json(client, cfg, AGENT_POLL_PATH, &poll) {
        Ok(response) => response,
        Err(error) => {
            if let Some((_, provider, revision)) = provider_update {
                provider.release_status_update(revision);
            }
            return Err(PollError::from_http(error, &cfg.client_id));
        }
    };
    if !response.success {
        if let Some((_, provider, revision)) = provider_update {
            provider.release_status_update(revision);
        }
        return Err(PollError::from_response_error(
            &cfg.client_id,
            response.error,
        ));
    }
    if let Some((_, provider, revision)) = provider_update {
        provider.mark_status_reported(revision);
    }
    let sink = AgentSink::Http(HttpSendConfig {
        client: client.clone(),
        server_url: cfg.server_url.clone(),
        token: cfg.token.clone(),
        client_id: cfg.client_id.clone(),
        agent_instance_id: agent_instance_id.to_string(),
        shutdown: Arc::clone(shutdown),
    });
    jobs.install_sink(sink.clone());
    let Some(request) = response.request else {
        return Ok(false);
    };
    let project_op = is_project_op(&request.kind);
    let hot = runtime.snapshot();
    let runtime = Arc::clone(runtime);
    let jobs = jobs.clone();
    let persistent_shells = persistent_shells.clone();
    let projects_dir = match projects_dir(cfg) {
        Ok(dir) => dir,
        Err(error) => return Err(PollError::new(PollErrorKind::Config, error)),
    };
    let lsp = lsp.clone();
    let dispatch = PollingDispatch {
        request_id: request.request_id.clone(),
        project_cache_invalidation_required: project_op,
        sink,
        config: hot,
        runtime,
        jobs,
        persistent_shells,
        projects_dir,
        lsp,
        request,
    };
    if !once {
        polling_dispatches.spawn(dispatch)?;
        return Ok(true);
    }

    // `--once` deliberately retains its existing synchronous contract: the
    // one delivered ordinary request stays tracked until dispatch and result
    // submission finish, and the caller still drains any Job work afterward.
    let dispatch_guard = dispatches.enter();
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let handle = std::thread::Builder::new()
        .name("webcodex-poll-dispatch-once".to_string())
        .spawn(move || {
            let _dispatch_guard = dispatch_guard;
            let _ = result_tx.send(dispatch.run());
        })
        .map_err(|error| {
            PollError::new(
                PollErrorKind::Config,
                format!("failed to start polling dispatch worker: {error}"),
            )
        })?;
    polling_dispatches.background_threads.register(handle);
    let result = loop {
        match result_rx.recv_timeout(Duration::from_millis(25)) {
            Ok(result) => break result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if shutdown.load(Ordering::SeqCst) {
                    // A dispatch is already in flight: its result may itself be
                    // a terminal auth/protocol error that must surface rather
                    // than be masked as a clean shutdown. Give the in-flight
                    // submission a bounded window to deliver its result before
                    // falling back to the shutdown outcome. This avoids losing
                    // a fatal submit (e.g. a 401/403/404 the server already
                    // returned) when the shutdown flag flips during the HTTP
                    // round-trip.
                    if let Ok(result) = result_rx.recv_timeout(Duration::from_millis(500)) {
                        break result;
                    }
                    return Err(PollError::from_submit(SubmitResultError::Shutdown(
                        "process shutdown".to_string(),
                    )));
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(PollError::from_submit(SubmitResultError::TransportClosed(
                    "polling dispatch worker closed".to_string(),
                )));
            }
        }
    };
    if project_op && result.is_ok() {
        project_cache.invalidate();
    }
    let _ = polling_dispatches.background_threads.reap_finished();
    result.map_err(PollError::from_submit)
}

fn main() {
    // Pin the process start timestamp before any transport work so register
    // payloads report real process identity even after reconnect loops.
    let _ = process_started_at();
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .try_init();

    let action = match parse_args() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(2);
        }
    };
    let (config_path, once) = match action {
        AgentCliAction::Run { config_path, once } => (config_path, once),
        AgentCliAction::Exit {
            code,
            stdout,
            stderr,
        } => {
            if !stdout.is_empty() {
                print!("{}", stdout);
            }
            if !stderr.is_empty() {
                eprint!("{}", stderr);
            }
            std::process::exit(code);
        }
    };
    let cfg = match load_config(&config_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(2);
        }
    };
    if cfg.token.trim().is_empty() {
        eprintln!(
            "webcodex-runner warning: agent token is empty; connecting without Authorization; the server must be started with --open"
        );
    }
    if let Err(e) = run_agent(cfg, config_path, once) {
        eprintln!("webcodex-runner failed: {}", e);
        std::process::exit(1);
    }
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;

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
