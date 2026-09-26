use crate::state::{RunnerRegistryInner, ShellJobRecord, ShellJobVisibility};
use crate::{now_ts, RunnerRegistry};
use std::collections::{HashMap, HashSet};
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};
use tokio::sync::{Mutex as AsyncMutex, MutexGuard};
pub use webcodex_core::runner_job_receipt::RetainedJobReceipt;
use webcodex_core::runner_protocol::{
    ShellJobContext, ShellJobSnapshot, ShellJobStreamSnapshot, JOB_TERMINAL_RETENTION_SECS,
};

/// Synchronous, best-effort historical persistence, invoked only after unlocking
/// registry state. Implementations must keep writes bounded and first-write-wins;
/// errors must never affect the accepted execution verdict. No execution methods.
pub trait JobReceiptStore: std::fmt::Debug + Send + Sync {
    fn upsert(&self, receipt: &RetainedJobReceipt) -> Result<(), String>;
    fn load(&self, now: i64) -> Result<Vec<RetainedJobReceipt>, String>;
    fn prune(&self, now: i64) -> Result<(), String>;
}

/// Sparse authoritative terminal fact emitted only after the canonical registry
/// has accepted a terminal Job lifecycle. It intentionally excludes commands,
/// streams, paths, validation bodies, credentials, and Session provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobTerminalEvent {
    pub job_id: String,
    pub client_id: String,
    pub runner_instance_id: String,
    pub auth_group: Option<crate::RunnerAccessGroup>,
    pub owner_at_admission: Option<String>,
    pub status: String,
    pub outcome: String,
    pub terminal_observed_at: i64,
    pub expires_at: i64,
}

/// Caller-authorized registration snapshot for one exact public Job. The
/// process-scoped Runner instance remains lifecycle evidence here, but durable
/// terminal-attention identity is the logical Job/client plus auth partition so
/// detached instance transfer cannot orphan an armed wait. Observation cursors
/// and Workflow Session provenance are intentionally absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobTerminalRegistrationSnapshot {
    pub job_id: String,
    pub client_id: String,
    pub runner_instance_id: String,
    pub auth_group: Option<crate::RunnerAccessGroup>,
    pub owner_at_admission: Option<String>,
    pub terminal_event: Option<JobTerminalEvent>,
    pub wait_expires_at: i64,
}

/// Best-effort post-lock sink for durable terminal-attention matching. The sink
/// owns no Job state and its failure can never change an accepted Job verdict.
pub trait JobTerminalEventSink: std::fmt::Debug + Send + Sync {
    fn record_terminal_event(&self, event: &JobTerminalEvent) -> Result<(), String>;
}

pub(crate) type ReceiptCandidates = Arc<Mutex<HashSet<String>>>;
pub(crate) type TerminalEventCandidates = Arc<Mutex<HashSet<String>>>;
pub(crate) type TerminalEventRetrySchedule = Arc<Mutex<HashMap<String, i64>>>;
const TERMINAL_EVENT_RETRY_COOLDOWN_SECS: i64 = 5;

/// One unlock boundary for every registry path, including early returns and
/// read-triggered lost transitions. Notifications mark only changed terminal
/// ids; this does not scan all Jobs or perform storage I/O under the async mutex.
#[derive(Debug)]
pub(crate) struct ReceiptRegistryState {
    state: AsyncMutex<RunnerRegistryInner>,
    pub(crate) candidates: ReceiptCandidates,
    pub(crate) terminal_event_candidates: TerminalEventCandidates,
    terminal_event_retry_not_before: TerminalEventRetrySchedule,
    store: Option<Arc<dyn JobReceiptStore>>,
    terminal_event_sink: Option<Arc<dyn JobTerminalEventSink>>,
}

impl ReceiptRegistryState {
    pub(crate) fn new(store: Option<Arc<dyn JobReceiptStore>>) -> Self {
        Self::with_sinks(store, None)
    }

    pub(crate) fn with_sinks(
        store: Option<Arc<dyn JobReceiptStore>>,
        terminal_event_sink: Option<Arc<dyn JobTerminalEventSink>>,
    ) -> Self {
        Self {
            state: AsyncMutex::new(RunnerRegistryInner::default()),
            candidates: Arc::default(),
            terminal_event_candidates: Arc::default(),
            terminal_event_retry_not_before: Arc::default(),
            store,
            terminal_event_sink,
        }
    }

    pub(crate) fn capture_candidates(&self) -> Option<ReceiptCandidates> {
        self.store.as_ref().map(|_| self.candidates.clone())
    }

    pub(crate) fn capture_terminal_event_candidates(&self) -> Option<TerminalEventCandidates> {
        self.terminal_event_sink
            .as_ref()
            .map(|_| self.terminal_event_candidates.clone())
    }

    #[cfg(test)]
    pub(crate) fn is_unlocked_for_test(&self) -> bool {
        self.state.try_lock().is_ok()
    }

    pub(crate) async fn lock(&self) -> ReceiptRegistryGuard<'_> {
        ReceiptRegistryGuard {
            guard: Some(self.state.lock().await),
            state: self,
        }
    }
}

pub(crate) struct ReceiptRegistryGuard<'a> {
    guard: Option<MutexGuard<'a, RunnerRegistryInner>>,
    state: &'a ReceiptRegistryState,
}

impl Deref for ReceiptRegistryGuard<'_> {
    type Target = RunnerRegistryInner;
    fn deref(&self) -> &Self::Target {
        self.guard.as_ref().unwrap()
    }
}
impl DerefMut for ReceiptRegistryGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.guard.as_mut().unwrap()
    }
}
pub(crate) fn terminal_persistence_error_code(error: &str) -> &str {
    let code = error.trim();
    if !code.is_empty()
        && code.len() <= 96
        && code
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        code
    } else {
        "unknown"
    }
}

impl Drop for ReceiptRegistryGuard<'_> {
    fn drop(&mut self) {
        let now = now_ts();
        let ids = std::mem::take(&mut *self.state.candidates.lock().unwrap());
        let terminal_ids = {
            let mut candidates = self.state.terminal_event_candidates.lock().unwrap();
            let retry_not_before = self.state.terminal_event_retry_not_before.lock().unwrap();
            let due = candidates
                .iter()
                .filter(|id| {
                    retry_not_before
                        .get(*id)
                        .is_none_or(|deadline| *deadline <= now)
                })
                .cloned()
                .collect::<HashSet<_>>();
            for id in &due {
                candidates.remove(id);
            }
            due
        };
        let receipts: Vec<_> = ids
            .iter()
            .filter_map(|id| self.jobs_by_id.get(id).and_then(capture))
            .collect();
        let mut terminal_events = Vec::new();
        let mut terminal_ids_without_live_event = Vec::new();
        for id in &terminal_ids {
            match self.jobs_by_id.get(id).and_then(capture_terminal_event) {
                Some(event) if event.expires_at > now => terminal_events.push(event),
                _ => terminal_ids_without_live_event.push(id.clone()),
            }
        }
        if !terminal_ids_without_live_event.is_empty() {
            let mut retry_not_before = self.state.terminal_event_retry_not_before.lock().unwrap();
            for id in terminal_ids_without_live_event {
                retry_not_before.remove(&id);
            }
        }
        // Release authority before any storage calls, even if an adapter fails.
        drop(self.guard.take());
        if let Some(store) = &self.state.store {
            let mut failed = 0;
            for receipt in receipts {
                if store.upsert(&receipt).is_err() {
                    failed += 1;
                }
            }
            if failed > 0 {
                // Never log adapter errors or payloads: they may contain SQL data.
                tracing::warn!(count = failed, "terminal Job receipt persistence degraded");
            }
        }
        if let Some(sink) = &self.state.terminal_event_sink {
            let mut failed = 0;
            let mut first_error_code = None;
            let mut retry_ids = Vec::new();
            let mut succeeded_ids = Vec::new();
            for event in terminal_events {
                let job_id = event.job_id.clone();
                if let Err(error) = sink.record_terminal_event(&event) {
                    failed += 1;
                    first_error_code
                        .get_or_insert_with(|| terminal_persistence_error_code(&error).to_string());
                    retry_ids.push(job_id);
                } else {
                    succeeded_ids.push(job_id);
                }
            }
            if !succeeded_ids.is_empty() {
                let mut retry_not_before =
                    self.state.terminal_event_retry_not_before.lock().unwrap();
                for id in succeeded_ids {
                    if retry_not_before.remove(&id).is_some() {
                        webcodex_core::runtime_diagnostics::record(
                            webcodex_core::runtime_diagnostics::DiagnosticSeverity::Info,
                            "job_terminal_attention",
                            "persistence_recovered",
                            Some(&id),
                        );
                    }
                }
            }
            if !retry_ids.is_empty() {
                // Terminal attention is durable caller state rather than optional
                // historical telemetry. Preserve failed post-lock candidates, but
                // do not hot-loop every registry unlock. A bounded cooldown keeps
                // persistence outages from turning into CPU/log storms while any
                // later unlock after the deadline still retries the exact event.
                let retry_at = now_ts().saturating_add(TERMINAL_EVENT_RETRY_COOLDOWN_SECS);
                {
                    let mut candidates = self.state.terminal_event_candidates.lock().unwrap();
                    candidates.extend(retry_ids.iter().cloned());
                }
                let mut retry_not_before =
                    self.state.terminal_event_retry_not_before.lock().unwrap();
                let mut first_degradation = false;
                for id in retry_ids {
                    if retry_not_before.insert(id, retry_at).is_none() {
                        first_degradation = true;
                    }
                }
                if first_degradation {
                    webcodex_core::runtime_diagnostics::record(
                        webcodex_core::runtime_diagnostics::DiagnosticSeverity::Warn,
                        "job_terminal_attention",
                        "persistence_degraded",
                        None,
                    );
                }
            }
            if failed > 0 {
                tracing::warn!(
                    count = failed,
                    retry_after_secs = TERMINAL_EVENT_RETRY_COOLDOWN_SECS,
                    error_code = first_error_code.as_deref().unwrap_or("unknown"),
                    "terminal Job attention persistence degraded"
                );
            }
        }
    }
}

pub(crate) fn terminal_outcome(status: &str) -> &'static str {
    match status {
        "completed" => "succeeded",
        "timeout" | "timed_out" => "timed_out",
        "stopped" | "cancelled" => "cancelled",
        "lost" => "outcome_unknown",
        _ => "failed",
    }
}

fn canonical_terminal_attention_status(status: &str) -> &str {
    if status == "timeout" {
        "timed_out"
    } else {
        status
    }
}

pub(crate) fn capture_terminal_event(job: &ShellJobRecord) -> Option<JobTerminalEvent> {
    if job.visibility != ShellJobVisibility::Public || !job.lifecycle.is_terminal() {
        return None;
    }
    let terminal_observed_at = job.observation.terminal_observed_at?;
    let wire_status = job.lifecycle.as_wire();
    let status = canonical_terminal_attention_status(wire_status).to_string();
    Some(JobTerminalEvent {
        job_id: job.job_id.clone(),
        client_id: job.client_id.clone(),
        runner_instance_id: job.runner_instance_id.clone(),
        auth_group: job.auth_group.clone(),
        owner_at_admission: job.owner_at_admission.clone(),
        outcome: terminal_outcome(&status).to_string(),
        status,
        terminal_observed_at,
        expires_at: terminal_observed_at.saturating_add(JOB_TERMINAL_RETENTION_SECS),
    })
}

pub(crate) fn registration_snapshot(
    job: &ShellJobRecord,
    now: i64,
) -> JobTerminalRegistrationSnapshot {
    let terminal_event = capture_terminal_event(job);
    let active_retention =
        (webcodex_core::runner_protocol::job_execution_timeout_max_secs(&job.kind) as i64)
            .saturating_add(crate::JOB_RECOVERY_GRACE_MAX_SECS)
            .saturating_add(JOB_TERMINAL_RETENTION_SECS);
    let wait_expires_at = terminal_event
        .as_ref()
        .map(|event| event.expires_at)
        .unwrap_or_else(|| now.saturating_add(active_retention));
    JobTerminalRegistrationSnapshot {
        job_id: job.job_id.clone(),
        client_id: job.client_id.clone(),
        runner_instance_id: job.runner_instance_id.clone(),
        auth_group: job.auth_group.clone(),
        owner_at_admission: job.owner_at_admission.clone(),
        terminal_event,
        wait_expires_at,
    }
}

fn capture(job: &ShellJobRecord) -> Option<RetainedJobReceipt> {
    if job.visibility != ShellJobVisibility::Public
        || !job.lifecycle.is_terminal()
        || job.kind == "run_detached_process"
    {
        return None;
    }
    let terminal_observed_at = job.observation.terminal_observed_at?;
    let stream = |log: &crate::state::ShellJobLogState| ShellJobStreamSnapshot {
        tail: log.tail.clone(),
        first_retained_line: log.first_retained_line,
        next_line: log.next_line,
        truncated: log.truncated,
    };
    let receipt = RetainedJobReceipt {
        client_id: job.client_id.clone(),
        runner_instance_id: job.runner_instance_id.clone(),
        auth_group: job.auth_group.clone(),
        owner_at_admission: job.owner_at_admission.clone(),
        kind: job.kind.clone(),
        terminal_observed_at,
        expires_at: terminal_observed_at.saturating_add(JOB_TERMINAL_RETENTION_SECS),
        snapshot: ShellJobSnapshot {
            job_id: job.job_id.clone(),
            request_id: job.request_id.clone()?,
            status: job.lifecycle.as_wire().to_string(),
            update_seq: job.last_update_seq,
            created_at: job.created_at,
            started_at: job.started_at,
            ended_at: job.ended_at,
            exit_code: job.exit_code,
            duration_ms: job.duration_ms,
            error: job.error.clone(),
            command_execution_state: job.command_execution_state,
            context: ShellJobContext {
                runtime_project_id: job.project_id.clone(),
                workflow_session_id: job.session_id.clone(),
                ssh_resource: job.ssh_resource.clone(),
                project_cwd: job.project_cwd.clone(),
                cwd: job.cwd.clone(),
                purpose: job.purpose.clone(),
                shell: job.shell.clone(),
                command_preview: job.command_preview.clone(),
                validation_steps: job.validation_steps.clone(),
                validation: None,
                structured_execution: job.structured_execution.clone(),
            },
            stdout: stream(&job.stdout),
            stderr: stream(&job.stderr),
            validation_progress: job.validation_progress.clone(),
            // Receipts deliberately omit exact structured-validation identity.
            // A test count without that provenance is not reusable correctness
            // evidence, so keep it only in Runner-owned snapshots/inventory.
            test_count_evidence: None,
            activity: None,
        },
    };
    if receipt.validate(now_ts()).is_err() {
        tracing::warn!("skipping invalid terminal Job receipt");
        return None;
    }
    Some(receipt)
}

#[cfg(test)]
mod terminal_outcome_tests {
    use super::terminal_outcome;

    #[test]
    fn lost_terminal_job_preserves_unknown_outcome_semantics() {
        assert_eq!(terminal_outcome("lost"), "outcome_unknown");
        assert_eq!(terminal_outcome("failed"), "failed");
        assert_eq!(terminal_outcome("completed"), "succeeded");
    }
}

impl RunnerRegistry {
    /// Startup-only construction. Hydrates before the registry can be shared
    /// with transports/tools. A failed load degrades history, never execution.
    pub async fn with_job_receipt_store(
        telemetry: Arc<dyn crate::RunnerRegistryTelemetry>,
        store: Arc<dyn JobReceiptStore>,
    ) -> Self {
        Self::with_job_stores(telemetry, store, None).await
    }

    pub async fn with_job_receipt_and_terminal_event_sink(
        telemetry: Arc<dyn crate::RunnerRegistryTelemetry>,
        store: Arc<dyn JobReceiptStore>,
        terminal_event_sink: Arc<dyn JobTerminalEventSink>,
    ) -> Self {
        Self::with_job_stores(telemetry, store, Some(terminal_event_sink)).await
    }

    async fn with_job_stores(
        telemetry: Arc<dyn crate::RunnerRegistryTelemetry>,
        store: Arc<dyn JobReceiptStore>,
        terminal_event_sink: Option<Arc<dyn JobTerminalEventSink>>,
    ) -> Self {
        let now = now_ts();
        let receipts = match store.load(now) {
            Ok(receipts) => receipts,
            Err(_) => {
                tracing::warn!("terminal Job receipt recovery degraded");
                Vec::new()
            }
        };
        let registry = Self {
            inner: Arc::new(ReceiptRegistryState::with_sinks(
                Some(store),
                terminal_event_sink,
            )),
            ..Self::with_telemetry(telemetry)
        };
        {
            let mut inner = registry.inner.lock().await;
            let receipt_candidates = registry.inner.capture_candidates();
            let terminal_event_candidates = registry.inner.capture_terminal_event_candidates();
            for receipt in receipts {
                if receipt.validate(now).is_err() {
                    tracing::warn!("skipping invalid terminal Job receipt");
                    continue;
                }
                let mut job = crate::reconciliation::record_from_snapshot(
                    &receipt.client_id,
                    &receipt.runner_instance_id,
                    receipt.auth_group,
                    registry.observation_epoch.clone(),
                    receipt_candidates.clone(),
                    terminal_event_candidates.clone(),
                    &receipt.snapshot,
                    now,
                );
                job.owner_at_admission = receipt.owner_at_admission;
                job.kind = receipt.kind;
                job.observation.terminal_observed_at = Some(receipt.terminal_observed_at);
                job.observation.receipt_expires_at = Some(receipt.expires_at);
                crate::jobs::replace_log_from_snapshot(&mut job.stdout, &receipt.snapshot.stdout);
                crate::jobs::replace_log_from_snapshot(&mut job.stderr, &receipt.snapshot.stderr);
                // Only a terminal record; no mapping, waiter, queue, intent or lease.
                inner.jobs_by_id.entry(job.job_id.clone()).or_insert(job);
            }
        }
        registry
    }

    pub(crate) fn prune_job_receipts(&self, now: i64) {
        if self
            .inner
            .store
            .as_ref()
            .is_some_and(|store| store.prune(now).is_err())
        {
            tracing::warn!("terminal Job receipt cleanup degraded");
        }
    }
}
