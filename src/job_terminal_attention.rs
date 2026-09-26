use crate::auth::AuthContext;
use crate::Database;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex, RwLock};
use webcodex_runner_registry::{
    JobTerminalEvent, JobTerminalEventSink, JobTerminalRegistrationSnapshot, RunnerAccessGroup,
};
use webcodex_store::{
    JobTerminalDeliveryState, JobTerminalFact, JobTerminalSourceIdentity, JobTerminalWaitPrincipal,
    JobTerminalWaitRecord, JobTerminalWaitStoreError,
};

pub(crate) const JOB_TERMINAL_ATTENTION_METRIC: &str = "job_terminal_attention_total";
const JOB_TERMINAL_APP_BINDING_ID_PREFIX: &str = "wc_host_binding_";
const MAX_RETIRED_APP_BINDINGS_PER_WAIT: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JobTerminalDeliveryEnvelope {
    pub wait_id: String,
    pub job_id: String,
    pub status: String,
    pub outcome: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JobTerminalDispatchOutcome {
    Delivered,
    OutcomeUnknown,
}

/// Process-local presentation seam for one already-durable Job terminal fact.
/// It grants no Job visibility or execution authority and owns no retry policy.
pub(crate) trait JobTerminalContinuationAdapter: std::fmt::Debug + Send + Sync {
    fn adapter_kind(&self) -> &'static str;
    fn production_auto_resume_available(&self) -> bool;
    fn preflight(&self, envelope: &JobTerminalDeliveryEnvelope) -> Result<(), ()>;
    fn dispatch(&self, envelope: JobTerminalDeliveryEnvelope) -> JobTerminalDispatchOutcome;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JobTerminalDeliveryAttempt {
    NoCarrier,
    PreflightFailed,
    Deduplicated,
    Delivered,
    DeliveryUnknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct JobTerminalAppBinding {
    principal: JobTerminalWaitPrincipal,
    client_window_key: String,
    binding_id: String,
    expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RetiredJobTerminalAppBindings {
    expires_at: i64,
    binding_ids: HashSet<String>,
    sealed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JobTerminalAppBindResult {
    pub(crate) wait: JobTerminalWaitRecord,
    pub(crate) state_changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JobTerminalAppState {
    pub(crate) wait: JobTerminalWaitRecord,
    pub(crate) prepared_attempt_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JobTerminalAppPrepared {
    pub(crate) wait: JobTerminalWaitRecord,
    pub(crate) attempt_id: String,
    pub(crate) automatic_message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JobTerminalAppFinishResult {
    pub(crate) wait: JobTerminalWaitRecord,
    pub(crate) state_changed: bool,
}

#[derive(Clone)]
pub(crate) struct JobTerminalContinuationController {
    db: Arc<Database>,
    adapter: Arc<RwLock<Option<Arc<dyn JobTerminalContinuationAdapter>>>>,
    app_bindings: Arc<Mutex<HashMap<String, JobTerminalAppBinding>>>,
    retired_app_bindings: Arc<Mutex<HashMap<String, RetiredJobTerminalAppBindings>>>,
    app_binding_transitions: Arc<Mutex<()>>,
}

impl std::fmt::Debug for JobTerminalContinuationController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobTerminalContinuationController")
            .field(
                "push_adapter_auto_resume_available",
                &self.push_adapter_auto_resume_available(),
            )
            .finish_non_exhaustive()
    }
}

impl JobTerminalContinuationController {
    pub(crate) fn new(db: Arc<Database>) -> Self {
        Self {
            db,
            adapter: Arc::new(RwLock::new(None)),
            app_bindings: Arc::new(Mutex::new(HashMap::new())),
            retired_app_bindings: Arc::new(Mutex::new(HashMap::new())),
            app_binding_transitions: Arc::new(Mutex::new(())),
        }
    }

    fn push_adapter_auto_resume_available(&self) -> bool {
        self.adapter
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .is_some_and(|adapter| adapter.production_auto_resume_available())
    }

    fn prune_expired_app_bindings(&self, now: i64) {
        self.app_bindings
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .retain(|_, binding| binding.expires_at > now);
        self.retired_app_bindings
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .retain(|_, retired| retired.expires_at > now);
    }

    pub(crate) fn automatic_resume_available_for_wait(
        &self,
        principal: &JobTerminalWaitPrincipal,
        wait_id: &str,
        now: i64,
    ) -> bool {
        self.prune_expired_app_bindings(now);
        let Ok(wait) = self.db.read_job_terminal_wait(principal, wait_id, now) else {
            return false;
        };
        if !matches!(
            wait.delivery_state,
            JobTerminalDeliveryState::NotReady | JobTerminalDeliveryState::Pending
        ) {
            return false;
        }
        let app_bound = self
            .app_bindings
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(wait_id)
            .is_some_and(|binding| binding.principal == *principal);
        app_bound || self.push_adapter_auto_resume_available()
    }

    #[cfg(test)]
    pub(crate) fn install_adapter_for_tests(
        &self,
        adapter: Arc<dyn JobTerminalContinuationAdapter>,
    ) {
        *self
            .adapter
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(adapter);
    }

    pub(crate) fn bind_mcp_app(
        &self,
        principal: &JobTerminalWaitPrincipal,
        wait_id: &str,
        binding_id: String,
        client_window_key: Option<&str>,
        now: i64,
    ) -> Result<JobTerminalAppBindResult, JobTerminalWaitStoreError> {
        let _transition = self
            .app_binding_transitions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        validate_app_binding_id(&binding_id)?;
        self.prune_expired_app_bindings(now);
        let client_window_key = client_window_key.ok_or_else(missing_client_window)?;
        let mut wait = self.db.read_job_terminal_wait(principal, wait_id, now)?;
        if self.is_retired_app_binding(wait_id, &binding_id) {
            return Err(stale_app_binding());
        }
        let current = self
            .app_bindings
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(wait_id)
            .cloned();
        if let Some(current) = current {
            if current.principal != *principal || current.client_window_key != client_window_key {
                return Err(stale_app_binding());
            }
            if current.binding_id == binding_id {
                return Ok(JobTerminalAppBindResult {
                    wait,
                    state_changed: false,
                });
            }
            self.ensure_retirement_capacity(wait_id, &current.binding_id)?;
            wait = self.reconcile_prepared_as_unknown(principal, wait, now)?;
            self.retire_app_binding(wait_id, &current.binding_id, wait.expires_at, false);
        } else if wait.delivery_state == JobTerminalDeliveryState::Prepared {
            wait = self.reconcile_prepared_as_unknown(principal, wait, now)?;
        }
        self.app_bindings
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(
                wait_id.to_string(),
                JobTerminalAppBinding {
                    principal: principal.clone(),
                    client_window_key: client_window_key.to_string(),
                    binding_id,
                    expires_at: wait.expires_at,
                },
            );
        Ok(JobTerminalAppBindResult {
            wait,
            state_changed: true,
        })
    }

    pub(crate) fn mcp_app_state(
        &self,
        principal: &JobTerminalWaitPrincipal,
        wait_id: &str,
        binding_id: &str,
        client_window_key: Option<&str>,
        now: i64,
    ) -> Result<JobTerminalAppState, JobTerminalWaitStoreError> {
        let wait =
            self.verify_mcp_app_binding(principal, wait_id, binding_id, client_window_key, now)?;
        Ok(JobTerminalAppState {
            prepared_attempt_id: (wait.delivery_state == JobTerminalDeliveryState::Prepared)
                .then(|| wait.delivery_attempt_id.clone())
                .flatten(),
            wait,
        })
    }

    pub(crate) fn prepare_mcp_app_delivery(
        &self,
        principal: &JobTerminalWaitPrincipal,
        wait_id: &str,
        binding_id: &str,
        client_window_key: Option<&str>,
        now: i64,
    ) -> Result<JobTerminalAppPrepared, JobTerminalWaitStoreError> {
        let _transition = self
            .app_binding_transitions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let wait =
            self.verify_mcp_app_binding(principal, wait_id, binding_id, client_window_key, now)?;
        if wait.delivery_state != JobTerminalDeliveryState::Pending {
            return Err(app_error(
                "job_terminal_delivery_not_pending",
                "Job terminal delivery is not pending and must not be prepared again",
            ));
        }
        let Some(prepared) = self
            .db
            .prepare_job_terminal_delivery(principal, wait_id, now)?
        else {
            return Err(app_error(
                "job_terminal_delivery_not_pending",
                "Job terminal delivery is no longer pending",
            ));
        };
        let delivery = envelope(&prepared.wait)?;
        let automatic_message = automatic_message(&delivery);
        metric("app_delivery_prepared");
        Ok(JobTerminalAppPrepared {
            wait: prepared.wait,
            attempt_id: prepared.attempt_id,
            automatic_message,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn finish_mcp_app_delivery(
        &self,
        principal: &JobTerminalWaitPrincipal,
        wait_id: &str,
        binding_id: &str,
        client_window_key: Option<&str>,
        attempt_id: &str,
        dispatch_accepted: bool,
        now: i64,
    ) -> Result<JobTerminalAppFinishResult, JobTerminalWaitStoreError> {
        let _transition = self
            .app_binding_transitions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let current =
            self.verify_mcp_app_binding(principal, wait_id, binding_id, client_window_key, now)?;
        let replay_state = if dispatch_accepted {
            JobTerminalDeliveryState::Delivered
        } else {
            JobTerminalDeliveryState::DeliveryUnknown
        };
        if current.delivery_attempt_id.as_deref() == Some(attempt_id)
            && current.delivery_state == replay_state
        {
            metric("app_finish_replayed");
            return Ok(JobTerminalAppFinishResult {
                wait: current,
                state_changed: false,
            });
        }
        let wait = self.db.finish_job_terminal_delivery(
            principal,
            wait_id,
            attempt_id,
            dispatch_accepted,
            now,
        )?;
        metric(if dispatch_accepted {
            "app_delivery_accepted"
        } else {
            "app_delivery_unknown"
        });
        Ok(JobTerminalAppFinishResult {
            wait,
            state_changed: true,
        })
    }

    pub(crate) fn unbind_mcp_app(
        &self,
        principal: &JobTerminalWaitPrincipal,
        wait_id: &str,
        binding_id: &str,
        client_window_key: Option<&str>,
        now: i64,
    ) -> Result<JobTerminalAppBindResult, JobTerminalWaitStoreError> {
        let _transition = self
            .app_binding_transitions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let wait =
            self.verify_mcp_app_binding(principal, wait_id, binding_id, client_window_key, now)?;
        let wait = self.reconcile_prepared_as_unknown(principal, wait, now)?;
        self.retire_app_binding(wait_id, binding_id, wait.expires_at, true);
        let removed = self
            .app_bindings
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(wait_id)
            .is_some_and(|binding| binding.binding_id == binding_id);
        Ok(JobTerminalAppBindResult {
            wait,
            state_changed: removed,
        })
    }

    fn is_retired_app_binding(&self, wait_id: &str, binding_id: &str) -> bool {
        self.retired_app_bindings
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(wait_id)
            .is_some_and(|retired| retired.sealed || retired.binding_ids.contains(binding_id))
    }

    fn ensure_retirement_capacity(
        &self,
        wait_id: &str,
        binding_id: &str,
    ) -> Result<(), JobTerminalWaitStoreError> {
        let retired = self
            .retired_app_bindings
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if retired.get(wait_id).is_some_and(|retired| {
            retired.sealed
                || (!retired.binding_ids.contains(binding_id)
                    && retired.binding_ids.len() >= MAX_RETIRED_APP_BINDINGS_PER_WAIT)
        }) {
            return Err(app_error(
                "job_terminal_host_binding_capacity_reached",
                "Job terminal Host binding replacement history is exhausted",
            ));
        }
        Ok(())
    }

    fn retire_app_binding(
        &self,
        wait_id: &str,
        binding_id: &str,
        expires_at: i64,
        seal_if_full: bool,
    ) {
        let mut retired = self
            .retired_app_bindings
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let entry =
            retired
                .entry(wait_id.to_string())
                .or_insert_with(|| RetiredJobTerminalAppBindings {
                    expires_at,
                    binding_ids: HashSet::new(),
                    sealed: false,
                });
        entry.expires_at = entry.expires_at.max(expires_at);
        if entry.sealed {
            return;
        }
        if seal_if_full && entry.binding_ids.len() >= MAX_RETIRED_APP_BINDINGS_PER_WAIT {
            entry.sealed = true;
            entry.binding_ids.clear();
        } else {
            entry.binding_ids.insert(binding_id.to_string());
        }
    }

    fn verify_mcp_app_binding(
        &self,
        principal: &JobTerminalWaitPrincipal,
        wait_id: &str,
        binding_id: &str,
        client_window_key: Option<&str>,
        now: i64,
    ) -> Result<JobTerminalWaitRecord, JobTerminalWaitStoreError> {
        validate_app_binding_id(binding_id)?;
        self.prune_expired_app_bindings(now);
        let client_window_key = client_window_key.ok_or_else(missing_client_window)?;
        let wait = self.db.read_job_terminal_wait(principal, wait_id, now)?;
        let bindings = self
            .app_bindings
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(binding) = bindings.get(wait_id) else {
            return Err(stale_app_binding());
        };
        if binding.principal != *principal
            || binding.binding_id != binding_id
            || binding.client_window_key != client_window_key
        {
            return Err(stale_app_binding());
        }
        Ok(wait)
    }

    fn reconcile_prepared_as_unknown(
        &self,
        principal: &JobTerminalWaitPrincipal,
        wait: JobTerminalWaitRecord,
        now: i64,
    ) -> Result<JobTerminalWaitRecord, JobTerminalWaitStoreError> {
        if wait.delivery_state != JobTerminalDeliveryState::Prepared {
            return Ok(wait);
        }
        let Some(attempt_id) = wait.delivery_attempt_id.as_deref() else {
            return Err(app_error(
                "job_terminal_wait_storage_invariant",
                "prepared Job terminal wait is missing delivery attempt identity",
            ));
        };
        metric("app_prepared_reconciled_unknown");
        self.db
            .finish_job_terminal_delivery(principal, &wait.wait_id, attempt_id, false, now)
    }

    pub(crate) fn attempt_delivery(
        &self,
        principal: &JobTerminalWaitPrincipal,
        wait_id: &str,
        now: i64,
    ) -> Result<JobTerminalDeliveryAttempt, JobTerminalWaitStoreError> {
        let wait = self.db.read_job_terminal_wait(principal, wait_id, now)?;
        if wait.delivery_state != JobTerminalDeliveryState::Pending {
            metric("deduplicated");
            return Ok(JobTerminalDeliveryAttempt::Deduplicated);
        }
        let adapter = self
            .adapter
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let Some(adapter) = adapter else {
            metric("no_carrier");
            return Ok(JobTerminalDeliveryAttempt::NoCarrier);
        };
        tracing::debug!(
            adapter_kind = adapter.adapter_kind(),
            "Job terminal Host carrier preflight"
        );
        let envelope = envelope(&wait)?;
        if adapter.preflight(&envelope).is_err() {
            metric("preflight_failed");
            return Ok(JobTerminalDeliveryAttempt::PreflightFailed);
        }
        let Some(prepared) = self
            .db
            .prepare_job_terminal_delivery(principal, wait_id, now)?
        else {
            metric("deduplicated");
            return Ok(JobTerminalDeliveryAttempt::Deduplicated);
        };
        metric("delivery_attempted");
        let dispatch = catch_unwind(AssertUnwindSafe(|| adapter.dispatch(envelope)))
            .unwrap_or(JobTerminalDispatchOutcome::OutcomeUnknown);
        let delivered = dispatch == JobTerminalDispatchOutcome::Delivered;
        self.db.finish_job_terminal_delivery(
            principal,
            wait_id,
            &prepared.attempt_id,
            delivered,
            now,
        )?;
        if delivered {
            metric("delivery_accepted");
            Ok(JobTerminalDeliveryAttempt::Delivered)
        } else {
            metric("delivery_unknown");
            Ok(JobTerminalDeliveryAttempt::DeliveryUnknown)
        }
    }
}

fn validate_app_binding_id(binding_id: &str) -> Result<(), JobTerminalWaitStoreError> {
    if !binding_id
        .strip_prefix(JOB_TERMINAL_APP_BINDING_ID_PREFIX)
        .is_some_and(|suffix| webcodex_core::compact::decode::<16>(suffix).is_some())
    {
        return Err(app_error(
            "invalid_host_binding_id",
            "binding_id must be wc_host_binding_ followed by canonical base64url of 16 random bytes",
        ));
    }
    Ok(())
}

fn stale_app_binding() -> JobTerminalWaitStoreError {
    app_error(
        "job_terminal_host_binding_stale",
        "Job terminal Host binding is missing, stale, or belongs to another ClientWindow",
    )
}

fn missing_client_window() -> JobTerminalWaitStoreError {
    app_error(
        "job_terminal_client_window_unavailable",
        "Job terminal Host binding requires canonical ClientWindow sideband identity",
    )
}

fn app_error(code: &'static str, message: &str) -> JobTerminalWaitStoreError {
    JobTerminalWaitStoreError {
        code,
        message: message.to_string(),
    }
}

fn automatic_message(envelope: &JobTerminalDeliveryEnvelope) -> String {
    format!(
        "WebPi Job {} reached terminal status {} with outcome {}. Reconcile this completion with the current conversation before continuing: if newer user instructions superseded the waiting task, do not resume the old work. If it is still relevant, continue without rerunning this Job. Call observe_jobs once only if detailed logs or validation evidence are needed.",
        envelope.job_id, envelope.status, envelope.outcome
    )
}

fn envelope(
    wait: &JobTerminalWaitRecord,
) -> Result<JobTerminalDeliveryEnvelope, JobTerminalWaitStoreError> {
    let Some(status) = wait.terminal_status.clone() else {
        return Err(JobTerminalWaitStoreError {
            code: "job_terminal_wait_storage_invariant",
            message: "triggered Job terminal wait is missing terminal status".to_string(),
        });
    };
    let Some(outcome) = wait.terminal_outcome.clone() else {
        return Err(JobTerminalWaitStoreError {
            code: "job_terminal_wait_storage_invariant",
            message: "triggered Job terminal wait is missing terminal outcome".to_string(),
        });
    };
    Ok(JobTerminalDeliveryEnvelope {
        wait_id: wait.wait_id.clone(),
        job_id: wait.source.job_id.clone(),
        status,
        outcome,
    })
}

pub(crate) struct SqliteJobTerminalEventSink {
    db: Arc<Database>,
    controller: JobTerminalContinuationController,
}

impl std::fmt::Debug for SqliteJobTerminalEventSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SqliteJobTerminalEventSink")
    }
}

impl SqliteJobTerminalEventSink {
    pub(crate) fn new(db: Arc<Database>, controller: JobTerminalContinuationController) -> Self {
        Self { db, controller }
    }
}

impl JobTerminalEventSink for SqliteJobTerminalEventSink {
    fn record_terminal_event(&self, event: &JobTerminalEvent) -> Result<(), String> {
        let now = chrono::Utc::now().timestamp();
        let fact = fact_from_event(event);
        let matched = self
            .db
            .match_job_terminal_fact(&fact, now)
            .map_err(|error| error.code.to_string())?;
        if matched.matched_count > 0 {
            metric("async_match");
        }
        let mut first_error = None;
        for (principal, wait_id) in matched.delivery_candidates {
            if let Err(error) = self.controller.attempt_delivery(&principal, &wait_id, now) {
                first_error.get_or_insert_with(|| error.code.to_string());
            }
        }
        if let Some(error) = first_error {
            Err(error)
        } else {
            Ok(())
        }
    }
}

pub(crate) fn principal_for_auth(auth: Option<&AuthContext>) -> JobTerminalWaitPrincipal {
    let access = crate::runner_http::runner_access_from_auth(auth);
    let (kind, subject) = match access.as_ref() {
        None => ("internal", "internal".to_string()),
        Some(access) if access.owner_bypass => ("bootstrap", "bootstrap".to_string()),
        Some(access) if access.global_visibility => {
            let subject = auth
                .and_then(|auth| auth.user_id.as_deref())
                .or_else(|| auth.and_then(|auth| auth.username.as_deref()))
                .or_else(|| auth.and_then(|auth| auth.api_key_id.as_deref()))
                .unwrap_or("global");
            ("global_user", subject.to_string())
        }
        Some(access) => match access.group.as_ref() {
            Some(RunnerAccessGroup::SharedKey(value)) => ("shared_key", value.clone()),
            Some(RunnerAccessGroup::ProjectGrant(value)) => ("project_grant", value.clone()),
            Some(RunnerAccessGroup::OpenAnonymous) => {
                ("open_anonymous", "open_anonymous".to_string())
            }
            None => (
                "managed_owner",
                access
                    .username
                    .clone()
                    .unwrap_or_else(|| "managed_unowned".to_string()),
            ),
        },
    };
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex.job-terminal-wait.principal.v1\0");
    hash_field(&mut hasher, kind);
    hash_field(&mut hasher, &subject);
    JobTerminalWaitPrincipal {
        kind: kind.to_string(),
        digest: format!("{:x}", hasher.finalize()),
    }
}

fn hash_field(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value.as_bytes());
}

pub(crate) fn source_from_snapshot(
    snapshot: &JobTerminalRegistrationSnapshot,
) -> JobTerminalSourceIdentity {
    source(
        &snapshot.job_id,
        &snapshot.client_id,
        snapshot.auth_group.as_ref(),
        snapshot.owner_at_admission.as_deref(),
    )
}

pub(crate) fn fact_from_event(event: &JobTerminalEvent) -> JobTerminalFact {
    JobTerminalFact {
        source: source(
            &event.job_id,
            &event.client_id,
            event.auth_group.as_ref(),
            event.owner_at_admission.as_deref(),
        ),
        status: event.status.clone(),
        outcome: event.outcome.clone(),
        terminal_observed_at: event.terminal_observed_at,
        expires_at: event.expires_at,
    }
}

fn source(
    job_id: &str,
    client_id: &str,
    auth_group: Option<&RunnerAccessGroup>,
    owner_at_admission: Option<&str>,
) -> JobTerminalSourceIdentity {
    let (auth_kind, auth_value) = match auth_group {
        Some(RunnerAccessGroup::SharedKey(value)) => ("shared_key", Some(value.clone())),
        Some(RunnerAccessGroup::ProjectGrant(value)) => ("project_grant", Some(value.clone())),
        Some(RunnerAccessGroup::OpenAnonymous) => ("open_anonymous", None),
        None => match owner_at_admission {
            Some(owner) => ("managed_owner", Some(owner.to_string())),
            None => ("managed_unowned", None),
        },
    };
    JobTerminalSourceIdentity {
        job_id: job_id.to_string(),
        client_id: client_id.to_string(),
        auth_kind: auth_kind.to_string(),
        auth_value,
    }
}

pub(crate) fn metric(outcome: &'static str) {
    tracing::info!(
        target: "webcodex::job_terminal_attention",
        metric = JOB_TERMINAL_ATTENTION_METRIC,
        outcome,
        value = 1_u64,
        "Job terminal attention observation"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::tempdir;
    use webcodex_store::{JobTerminalWaitState, NewJobTerminalWait};

    #[derive(Debug)]
    struct TestAdapter {
        preflight_ok: bool,
        outcome: JobTerminalDispatchOutcome,
        preflights: AtomicUsize,
        dispatches: AtomicUsize,
    }

    impl TestAdapter {
        fn new(preflight_ok: bool, outcome: JobTerminalDispatchOutcome) -> Self {
            Self {
                preflight_ok,
                outcome,
                preflights: AtomicUsize::new(0),
                dispatches: AtomicUsize::new(0),
            }
        }
    }

    impl JobTerminalContinuationAdapter for TestAdapter {
        fn adapter_kind(&self) -> &'static str {
            "test"
        }
        fn production_auto_resume_available(&self) -> bool {
            true
        }
        fn preflight(&self, _envelope: &JobTerminalDeliveryEnvelope) -> Result<(), ()> {
            self.preflights.fetch_add(1, Ordering::SeqCst);
            if self.preflight_ok {
                Ok(())
            } else {
                Err(())
            }
        }
        fn dispatch(&self, _envelope: JobTerminalDeliveryEnvelope) -> JobTerminalDispatchOutcome {
            self.dispatches.fetch_add(1, Ordering::SeqCst);
            self.outcome
        }
    }

    fn principal() -> JobTerminalWaitPrincipal {
        JobTerminalWaitPrincipal {
            kind: "test".to_string(),
            digest: "a".repeat(64),
        }
    }

    fn source(job_id: &str) -> JobTerminalSourceIdentity {
        JobTerminalSourceIdentity {
            job_id: job_id.to_string(),
            client_id: "runner".to_string(),
            auth_kind: "managed_owner".to_string(),
            auth_value: Some("alice".to_string()),
        }
    }

    #[test]
    fn terminal_wait_principal_tracks_job_visibility_not_rotating_credentials() {
        let mut first = AuthContext::new(crate::auth::AuthKind::ApiToken);
        first.user_id = Some("user-alice".to_string());
        first.username = Some("alice".to_string());
        first.api_key_id = Some("key-a".to_string());
        let mut rotated = first.clone();
        rotated.api_key_id = Some("key-b".to_string());
        rotated.allowed_client_id = Some("unrelated-transport-hint".to_string());
        assert_eq!(
            principal_for_auth(Some(&first)),
            principal_for_auth(Some(&rotated))
        );

        let mut bob = rotated;
        bob.user_id = Some("user-bob".to_string());
        bob.username = Some("bob".to_string());
        assert_ne!(
            principal_for_auth(Some(&first)),
            principal_for_auth(Some(&bob))
        );
    }

    #[test]
    fn logical_job_source_survives_runner_instance_transfer() {
        let snapshot = JobTerminalRegistrationSnapshot {
            job_id: "job-detached".to_string(),
            client_id: "runner".to_string(),
            runner_instance_id: "instance-old".to_string(),
            auth_group: None,
            owner_at_admission: Some("alice".to_string()),
            terminal_event: None,
            wait_expires_at: 1_900,
        };
        let event = JobTerminalEvent {
            job_id: "job-detached".to_string(),
            client_id: "runner".to_string(),
            runner_instance_id: "instance-new".to_string(),
            auth_group: None,
            owner_at_admission: Some("alice".to_string()),
            status: "completed".to_string(),
            outcome: "succeeded".to_string(),
            terminal_observed_at: 1_000,
            expires_at: 1_900,
        };
        assert_eq!(
            source_from_snapshot(&snapshot),
            fact_from_event(&event).source
        );
    }

    fn create_triggered(db: &Database, job_id: &str, key: &str, now: i64) -> String {
        let terminal = JobTerminalFact {
            source: source(job_id),
            status: "completed".to_string(),
            outcome: "succeeded".to_string(),
            terminal_observed_at: now,
            expires_at: now + 900,
        };
        db.create_job_terminal_wait(
            &principal(),
            NewJobTerminalWait {
                source: source(job_id),
                idempotency_key: key.to_string(),
                expires_at: now + 900,
                already_terminal: Some(terminal),
            },
            now,
        )
        .unwrap()
        .wait
        .wait_id
    }

    fn create_waiting(db: &Database, job_id: &str, key: &str, now: i64) -> String {
        db.create_job_terminal_wait(
            &principal(),
            NewJobTerminalWait {
                source: source(job_id),
                idempotency_key: key.to_string(),
                expires_at: now + 900,
                already_terminal: None,
            },
            now,
        )
        .unwrap()
        .wait
        .wait_id
    }

    fn binding(byte: u8) -> String {
        format!(
            "{JOB_TERMINAL_APP_BINDING_ID_PREFIX}{}",
            webcodex_core::compact::encode([byte; 16])
        )
    }

    fn foreign_principal() -> JobTerminalWaitPrincipal {
        JobTerminalWaitPrincipal {
            kind: "test".to_string(),
            digest: "b".repeat(64),
        }
    }

    #[test]
    fn no_carrier_and_preflight_failure_leave_triggered_event_pending() {
        let temp = tempdir().unwrap();
        let db = Arc::new(Database::open(&temp.path().join("host-pending.db")).unwrap());
        let controller = JobTerminalContinuationController::new(db.clone());
        let wait_id = create_triggered(&db, "job-no-carrier", "key-no-carrier", 1_000);
        assert_eq!(
            controller
                .attempt_delivery(&principal(), &wait_id, 1_001)
                .unwrap(),
            JobTerminalDeliveryAttempt::NoCarrier
        );
        assert_eq!(
            db.read_job_terminal_wait(&principal(), &wait_id, 1_001)
                .unwrap()
                .delivery_state,
            JobTerminalDeliveryState::Pending
        );

        let adapter = Arc::new(TestAdapter::new(
            false,
            JobTerminalDispatchOutcome::Delivered,
        ));
        controller.install_adapter_for_tests(adapter.clone());
        assert!(controller.automatic_resume_available_for_wait(&principal(), &wait_id, 1_001));
        assert_eq!(
            controller
                .attempt_delivery(&principal(), &wait_id, 1_002)
                .unwrap(),
            JobTerminalDeliveryAttempt::PreflightFailed
        );
        assert_eq!(adapter.preflights.load(Ordering::SeqCst), 1);
        assert_eq!(adapter.dispatches.load(Ordering::SeqCst), 0);
        let wait = db
            .read_job_terminal_wait(&principal(), &wait_id, 1_002)
            .unwrap();
        assert_eq!(wait.state, JobTerminalWaitState::Triggered);
        assert_eq!(wait.delivery_state, JobTerminalDeliveryState::Pending);
    }

    #[test]
    fn exact_wait_app_capability_and_binding_fences_are_local_and_authorized() {
        let temp = tempdir().unwrap();
        let db = Arc::new(Database::open(&temp.path().join("app-binding.db")).unwrap());
        let controller = JobTerminalContinuationController::new(db.clone());
        let wait_id = create_waiting(&db, "job-app-bound", "app-bound", 1_100);
        let unrelated = create_waiting(&db, "job-app-unrelated", "app-unrelated", 1_100);
        let first = binding(1);
        let replacement = binding(2);

        assert!(!controller.automatic_resume_available_for_wait(&principal(), &wait_id, 1_101));
        let bound = controller
            .bind_mcp_app(
                &principal(),
                &wait_id,
                first.clone(),
                Some("window-a"),
                1_101,
            )
            .unwrap();
        assert!(bound.state_changed);
        assert!(controller.automatic_resume_available_for_wait(&principal(), &wait_id, 1_101));
        assert!(!controller.automatic_resume_available_for_wait(&principal(), &unrelated, 1_101));

        let replay = controller
            .bind_mcp_app(
                &principal(),
                &wait_id,
                first.clone(),
                Some("window-a"),
                1_102,
            )
            .unwrap();
        assert!(!replay.state_changed);
        let stolen = controller.bind_mcp_app(
            &principal(),
            &wait_id,
            replacement.clone(),
            Some("window-b"),
            1_102,
        );
        assert_eq!(stolen.unwrap_err().code, "job_terminal_host_binding_stale");

        let refreshed = controller
            .bind_mcp_app(
                &principal(),
                &wait_id,
                replacement.clone(),
                Some("window-a"),
                1_103,
            )
            .unwrap();
        assert!(refreshed.state_changed);
        assert_eq!(
            controller
                .mcp_app_state(&principal(), &wait_id, &first, Some("window-a"), 1_103,)
                .unwrap_err()
                .code,
            "job_terminal_host_binding_stale"
        );
        assert_eq!(
            controller
                .bind_mcp_app(
                    &principal(),
                    &wait_id,
                    first.clone(),
                    Some("window-a"),
                    1_103,
                )
                .unwrap_err()
                .code,
            "job_terminal_host_binding_stale"
        );
        assert_eq!(
            controller
                .mcp_app_state(&principal(), &wait_id, &replacement, None, 1_103,)
                .unwrap_err()
                .code,
            "job_terminal_client_window_unavailable"
        );
        assert!(controller
            .bind_mcp_app(
                &foreign_principal(),
                &wait_id,
                binding(3),
                Some("window-a"),
                1_103,
            )
            .is_err());
        assert!(controller
            .mcp_app_state(
                &foreign_principal(),
                &wait_id,
                &replacement,
                Some("window-a"),
                1_103,
            )
            .is_err());
        assert!(controller
            .prepare_mcp_app_delivery(
                &foreign_principal(),
                &wait_id,
                &replacement,
                Some("window-a"),
                1_103,
            )
            .is_err());
        assert!(controller
            .finish_mcp_app_delivery(
                &foreign_principal(),
                &wait_id,
                &replacement,
                Some("window-a"),
                "wc_job_delivery_ZmZmZmZmZmZmZmZm",
                true,
                1_103,
            )
            .is_err());

        let unbound = controller
            .unbind_mcp_app(
                &principal(),
                &wait_id,
                &replacement,
                Some("window-a"),
                1_104,
            )
            .unwrap();
        assert!(unbound.state_changed);
        assert!(!controller.automatic_resume_available_for_wait(&principal(), &wait_id, 1_104));
        assert_eq!(
            controller
                .bind_mcp_app(&principal(), &wait_id, replacement, Some("window-a"), 1_105,)
                .unwrap_err()
                .code,
            "job_terminal_host_binding_stale"
        );
    }

    #[test]
    fn app_binding_replacement_history_is_bounded_and_expired_state_is_pruned() {
        let temp = tempdir().unwrap();
        let db = Arc::new(Database::open(&temp.path().join("app-binding-bounds.db")).unwrap());
        let controller = JobTerminalContinuationController::new(db.clone());
        let wait_id = create_waiting(&db, "job-app-bounds", "app-bounds", 4_000);
        let window = Some("window-bounds");

        controller
            .bind_mcp_app(&principal(), &wait_id, binding(1), window, 4_001)
            .unwrap();
        for byte in 2..=(MAX_RETIRED_APP_BINDINGS_PER_WAIT as u8 + 1) {
            controller
                .bind_mcp_app(&principal(), &wait_id, binding(byte), window, 4_001)
                .unwrap();
        }
        let current = binding(MAX_RETIRED_APP_BINDINGS_PER_WAIT as u8 + 1);
        assert_eq!(
            controller
                .bind_mcp_app(
                    &principal(),
                    &wait_id,
                    binding(MAX_RETIRED_APP_BINDINGS_PER_WAIT as u8 + 2),
                    window,
                    4_001,
                )
                .unwrap_err()
                .code,
            "job_terminal_host_binding_capacity_reached"
        );
        assert!(controller
            .mcp_app_state(&principal(), &wait_id, &current, window, 4_001)
            .is_ok());
        assert_eq!(
            controller
                .bind_mcp_app(&principal(), &wait_id, binding(1), window, 4_001)
                .unwrap_err()
                .code,
            "job_terminal_host_binding_stale"
        );
        assert_eq!(
            controller
                .retired_app_bindings
                .lock()
                .unwrap()
                .get(&wait_id)
                .unwrap()
                .binding_ids
                .len(),
            MAX_RETIRED_APP_BINDINGS_PER_WAIT
        );

        let unbound = controller
            .unbind_mcp_app(&principal(), &wait_id, &current, window, 4_002)
            .unwrap();
        assert!(unbound.state_changed);
        let retired = controller.retired_app_bindings.lock().unwrap();
        let sealed = retired.get(&wait_id).unwrap();
        assert!(sealed.sealed);
        assert!(sealed.binding_ids.is_empty());
        drop(retired);
        assert_eq!(
            controller
                .bind_mcp_app(&principal(), &wait_id, binding(90), window, 4_003)
                .unwrap_err()
                .code,
            "job_terminal_host_binding_stale"
        );

        assert!(!controller.automatic_resume_available_for_wait(&principal(), &wait_id, 4_901));
        assert!(controller.app_bindings.lock().unwrap().is_empty());
        assert!(controller.retired_app_bindings.lock().unwrap().is_empty());
    }

    #[test]
    fn job_app_deterministic_terminal_flow_prepares_and_finishes_exactly_once() {
        let temp = tempdir().unwrap();
        let db = Arc::new(Database::open(&temp.path().join("app-flow.db")).unwrap());
        let controller = JobTerminalContinuationController::new(db.clone());
        let now = chrono::Utc::now().timestamp();
        let wait_id = create_waiting(&db, "job-app-flow", "app-flow", now - 1);
        let binding_id = binding(4);
        controller
            .bind_mcp_app(
                &principal(),
                &wait_id,
                binding_id.clone(),
                Some("window-flow"),
                now,
            )
            .unwrap();

        let sink = SqliteJobTerminalEventSink::new(db.clone(), controller.clone());
        sink.record_terminal_event(&JobTerminalEvent {
            job_id: "job-app-flow".to_string(),
            client_id: "runner".to_string(),
            runner_instance_id: "runner-instance".to_string(),
            auth_group: None,
            owner_at_admission: Some("alice".to_string()),
            status: "completed".to_string(),
            outcome: "succeeded".to_string(),
            terminal_observed_at: now - 1,
            expires_at: now + 900,
        })
        .unwrap();

        let state = controller
            .mcp_app_state(
                &principal(),
                &wait_id,
                &binding_id,
                Some("window-flow"),
                now,
            )
            .unwrap();
        assert_eq!(state.wait.state, JobTerminalWaitState::Triggered);
        assert_eq!(state.wait.delivery_state, JobTerminalDeliveryState::Pending);
        assert!(state.prepared_attempt_id.is_none());

        let prepared = controller
            .prepare_mcp_app_delivery(
                &principal(),
                &wait_id,
                &binding_id,
                Some("window-flow"),
                now,
            )
            .unwrap();
        assert_eq!(
            prepared.wait.delivery_state,
            JobTerminalDeliveryState::Prepared
        );
        assert!(prepared.automatic_message.contains("job-app-flow"));
        assert!(prepared.automatic_message.contains("completed"));
        assert!(prepared.automatic_message.contains("succeeded"));
        assert!(prepared
            .automatic_message
            .contains("if newer user instructions superseded the waiting task"));
        for forbidden in [
            "stdout",
            "stderr",
            "command",
            "environment",
            "/tmp/",
            "Bearer ",
        ] {
            assert!(!prepared.automatic_message.contains(forbidden));
        }
        assert_eq!(
            controller
                .prepare_mcp_app_delivery(
                    &principal(),
                    &wait_id,
                    &binding_id,
                    Some("window-flow"),
                    now,
                )
                .unwrap_err()
                .code,
            "job_terminal_delivery_not_pending"
        );

        let delivered = controller
            .finish_mcp_app_delivery(
                &principal(),
                &wait_id,
                &binding_id,
                Some("window-flow"),
                &prepared.attempt_id,
                true,
                now,
            )
            .unwrap();
        assert_eq!(
            delivered.wait.delivery_state,
            JobTerminalDeliveryState::Delivered
        );
        assert!(delivered.state_changed);
        assert!(!controller.automatic_resume_available_for_wait(&principal(), &wait_id, now));
        let finish_replay = controller
            .finish_mcp_app_delivery(
                &principal(),
                &wait_id,
                &binding_id,
                Some("window-flow"),
                &prepared.attempt_id,
                true,
                now,
            )
            .unwrap();
        assert_eq!(
            finish_replay.wait.delivery_state,
            JobTerminalDeliveryState::Delivered
        );
        assert!(!finish_replay.state_changed);
        assert!(controller
            .finish_mcp_app_delivery(
                &principal(),
                &wait_id,
                &binding_id,
                Some("window-flow"),
                &prepared.attempt_id,
                false,
                now,
            )
            .is_err());

        sink.record_terminal_event(&JobTerminalEvent {
            job_id: "job-app-flow".to_string(),
            client_id: "runner".to_string(),
            runner_instance_id: "runner-instance-2".to_string(),
            auth_group: None,
            owner_at_admission: Some("alice".to_string()),
            status: "completed".to_string(),
            outcome: "succeeded".to_string(),
            terminal_observed_at: now - 1,
            expires_at: now + 900,
        })
        .unwrap();
        assert_eq!(
            db.read_job_terminal_wait(&principal(), &wait_id, now)
                .unwrap()
                .delivery_state,
            JobTerminalDeliveryState::Delivered
        );
    }

    #[test]
    fn app_teardown_replacement_and_restart_never_blindly_redispatch_prepared_delivery() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("app-recovery.db");
        let db = Arc::new(Database::open(&path).unwrap());
        let controller = JobTerminalContinuationController::new(db.clone());

        let pending_wait = create_triggered(&db, "job-pending-rebind", "pending-rebind", 3_100);
        let first = binding(5);
        controller
            .bind_mcp_app(
                &principal(),
                &pending_wait,
                first.clone(),
                Some("window-pending"),
                3_101,
            )
            .unwrap();
        controller
            .unbind_mcp_app(
                &principal(),
                &pending_wait,
                &first,
                Some("window-pending"),
                3_102,
            )
            .unwrap();
        assert_eq!(
            db.read_job_terminal_wait(&principal(), &pending_wait, 3_102)
                .unwrap()
                .delivery_state,
            JobTerminalDeliveryState::Pending
        );
        let rebound = JobTerminalContinuationController::new(db.clone());
        assert!(!rebound.automatic_resume_available_for_wait(&principal(), &pending_wait, 3_103));
        rebound
            .bind_mcp_app(
                &principal(),
                &pending_wait,
                first.clone(),
                Some("window-after-restart"),
                3_103,
            )
            .unwrap();
        assert!(rebound.automatic_resume_available_for_wait(&principal(), &pending_wait, 3_103));

        let prepared_wait =
            create_triggered(&db, "job-prepared-replace", "prepared-replace", 3_200);
        let old_binding = binding(7);
        rebound
            .bind_mcp_app(
                &principal(),
                &prepared_wait,
                old_binding.clone(),
                Some("window-replace"),
                3_201,
            )
            .unwrap();
        let prepared = rebound
            .prepare_mcp_app_delivery(
                &principal(),
                &prepared_wait,
                &old_binding,
                Some("window-replace"),
                3_202,
            )
            .unwrap();
        let replacement = binding(8);
        let replaced = rebound
            .bind_mcp_app(
                &principal(),
                &prepared_wait,
                replacement.clone(),
                Some("window-replace"),
                3_203,
            )
            .unwrap();
        assert_eq!(
            replaced.wait.delivery_state,
            JobTerminalDeliveryState::DeliveryUnknown
        );
        assert!(rebound
            .finish_mcp_app_delivery(
                &principal(),
                &prepared_wait,
                &old_binding,
                Some("window-replace"),
                &prepared.attempt_id,
                true,
                3_204,
            )
            .is_err());
        assert!(rebound
            .prepare_mcp_app_delivery(
                &principal(),
                &prepared_wait,
                &replacement,
                Some("window-replace"),
                3_204,
            )
            .is_err());

        let restart_wait = create_triggered(&db, "job-prepared-restart", "prepared-restart", 3_300);
        let restart_binding = binding(9);
        rebound
            .bind_mcp_app(
                &principal(),
                &restart_wait,
                restart_binding.clone(),
                Some("window-before-restart"),
                3_301,
            )
            .unwrap();
        rebound
            .prepare_mcp_app_delivery(
                &principal(),
                &restart_wait,
                &restart_binding,
                Some("window-before-restart"),
                3_302,
            )
            .unwrap();
        db.recover_job_terminal_deliveries_after_restart(3_303)
            .unwrap();
        let after_restart = JobTerminalContinuationController::new(db.clone());
        let recovered = db
            .read_job_terminal_wait(&principal(), &restart_wait, 3_303)
            .unwrap();
        assert_eq!(
            recovered.delivery_state,
            JobTerminalDeliveryState::DeliveryUnknown
        );
        assert!(!after_restart.automatic_resume_available_for_wait(
            &principal(),
            &restart_wait,
            3_303
        ));
        let rebound_unknown = after_restart
            .bind_mcp_app(
                &principal(),
                &restart_wait,
                binding(10),
                Some("window-after-restart"),
                3_304,
            )
            .unwrap();
        assert_eq!(
            rebound_unknown.wait.delivery_state,
            JobTerminalDeliveryState::DeliveryUnknown
        );
        assert!(after_restart
            .prepare_mcp_app_delivery(
                &principal(),
                &restart_wait,
                &binding(10),
                Some("window-after-restart"),
                3_305,
            )
            .is_err());
    }

    #[test]
    fn host_rejection_finishes_existing_attempt_unknown_without_retry() {
        let temp = tempdir().unwrap();
        let db = Arc::new(Database::open(&temp.path().join("app-unknown.db")).unwrap());
        let controller = JobTerminalContinuationController::new(db.clone());
        let wait_id = create_triggered(&db, "job-app-unknown", "app-unknown", 4_100);
        let binding_id = binding(11);
        controller
            .bind_mcp_app(
                &principal(),
                &wait_id,
                binding_id.clone(),
                Some("window-unknown"),
                4_101,
            )
            .unwrap();
        let prepared = controller
            .prepare_mcp_app_delivery(
                &principal(),
                &wait_id,
                &binding_id,
                Some("window-unknown"),
                4_102,
            )
            .unwrap();
        let unknown = controller
            .finish_mcp_app_delivery(
                &principal(),
                &wait_id,
                &binding_id,
                Some("window-unknown"),
                &prepared.attempt_id,
                false,
                4_103,
            )
            .unwrap();
        assert_eq!(
            unknown.wait.delivery_state,
            JobTerminalDeliveryState::DeliveryUnknown
        );
        assert!(unknown.state_changed);
        assert!(controller
            .prepare_mcp_app_delivery(
                &principal(),
                &wait_id,
                &binding_id,
                Some("window-unknown"),
                4_104,
            )
            .is_err());
    }

    #[test]
    fn accepted_dispatch_delivers_once_and_duplicate_scheduling_is_deduplicated() {
        let temp = tempdir().unwrap();
        let db = Arc::new(Database::open(&temp.path().join("host-delivered.db")).unwrap());
        let controller = JobTerminalContinuationController::new(db.clone());
        let adapter = Arc::new(TestAdapter::new(
            true,
            JobTerminalDispatchOutcome::Delivered,
        ));
        controller.install_adapter_for_tests(adapter.clone());
        let wait_id = create_triggered(&db, "job-delivered", "key-delivered", 2_000);
        assert_eq!(
            controller
                .attempt_delivery(&principal(), &wait_id, 2_001)
                .unwrap(),
            JobTerminalDeliveryAttempt::Delivered
        );
        assert_eq!(
            controller
                .attempt_delivery(&principal(), &wait_id, 2_002)
                .unwrap(),
            JobTerminalDeliveryAttempt::Deduplicated
        );
        assert_eq!(adapter.dispatches.load(Ordering::SeqCst), 1);
        assert_eq!(
            db.read_job_terminal_wait(&principal(), &wait_id, 2_002)
                .unwrap()
                .delivery_state,
            JobTerminalDeliveryState::Delivered
        );
    }

    #[test]
    fn unknown_dispatch_outcome_is_fenced_and_never_silently_redispatched() {
        let temp = tempdir().unwrap();
        let db = Arc::new(Database::open(&temp.path().join("host-unknown.db")).unwrap());
        let controller = JobTerminalContinuationController::new(db.clone());
        let adapter = Arc::new(TestAdapter::new(
            true,
            JobTerminalDispatchOutcome::OutcomeUnknown,
        ));
        controller.install_adapter_for_tests(adapter.clone());
        let wait_id = create_triggered(&db, "job-unknown", "key-unknown", 3_000);
        assert_eq!(
            controller
                .attempt_delivery(&principal(), &wait_id, 3_001)
                .unwrap(),
            JobTerminalDeliveryAttempt::DeliveryUnknown
        );
        assert_eq!(
            controller
                .attempt_delivery(&principal(), &wait_id, 3_002)
                .unwrap(),
            JobTerminalDeliveryAttempt::Deduplicated
        );
        assert_eq!(adapter.dispatches.load(Ordering::SeqCst), 1);
        assert_eq!(
            db.read_job_terminal_wait(&principal(), &wait_id, 3_002)
                .unwrap()
                .delivery_state,
            JobTerminalDeliveryState::DeliveryUnknown
        );
    }
}
