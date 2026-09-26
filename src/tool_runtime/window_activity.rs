use crate::auth::AuthContext;
use crate::client_window::ClientWindow;
use crate::tool_request_trace::RequestCompletionTiming;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

pub(crate) const MAX_ACTIVE_WINDOW_REQUESTS: usize = 64;
pub(crate) const MAX_ACTIVE_REQUESTS_PER_WINDOW: usize = 8;
pub(crate) const MAX_WINDOW_LOOP_CONTINUITIES: usize = 256;
const MAX_UNTRACKED_ACTIVE_PRINCIPALS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowLoopTransition {
    Unavailable,
    Serial { gap_ms: u64 },
    Overlap,
}

impl WindowLoopTransition {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::Serial { .. } => "serial",
            Self::Overlap => "overlap",
        }
    }

    #[cfg(test)]
    pub(crate) fn gap_ms(self) -> Option<u64> {
        match self {
            Self::Serial { gap_ms } => Some(gap_ms),
            Self::Unavailable | Self::Overlap => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkflowSessionCorrelationRelation {
    Recording,
    WorkOnProject,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkflowSessionCorrelation {
    pub(crate) session_id: String,
    pub(crate) project: Option<String>,
    pub(crate) relation: WorkflowSessionCorrelationRelation,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ToolCallCorrelation {
    pub(crate) resolved_project: Option<String>,
    /// Exact business Workflow Session selected by the typed ToolCall after
    /// canonical dispatch succeeds. Audit evidence only; never recorder or execution authority.
    pub(crate) business_session_id: Option<String>,
    pub(crate) workflow_sessions: Vec<WorkflowSessionCorrelation>,
    pub(crate) recorder_gap_session_id: Option<String>,
    #[cfg(feature = "experimental-code-mode")]
    pub(crate) code_mode_composition: Option<super::code_mode::CodeModeCompositionSummary>,
}

impl ToolCallCorrelation {
    pub(crate) fn add_workflow_session(&mut self, link: WorkflowSessionCorrelation) {
        if self
            .workflow_sessions
            .iter()
            .any(|existing| existing == &link)
        {
            return;
        }
        self.workflow_sessions.push(link);
    }

    /// Bounded diagnostic-only composition projection for outer ActionAudit.
    /// The feature-off build deliberately exposes no new durable shape.
    pub(crate) fn code_mode_composition_audit_summary(&self) -> Option<serde_json::Value> {
        #[cfg(feature = "experimental-code-mode")]
        {
            return self
                .code_mode_composition
                .as_ref()
                .and_then(|summary| serde_json::to_value(summary).ok());
        }
        #[cfg(not(feature = "experimental-code-mode"))]
        {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct ActiveWindowRequest {
    pub(crate) client_window_key: String,
    pub(crate) client_window_source: String,
    pub(crate) server_trace_id: String,
    pub(crate) method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) project: Option<String>,
    #[serde(skip)]
    principal_correlation_kind: Option<String>,
    #[serde(skip)]
    principal_correlation_id: Option<String>,
    #[serde(skip)]
    meaningful: bool,
    #[serde(skip)]
    overlapped: bool,
    pub(crate) started_at_ms: i64,
}

impl ActiveWindowRequest {
    pub(crate) fn is_meaningful(&self) -> bool {
        self.meaningful
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ActiveWindowSummary {
    pub(crate) client_window_key: String,
    pub(crate) client_window_source: String,
    pub(crate) active_count: usize,
    pub(crate) last_started_at_ms: i64,
}

#[derive(Debug, Default)]
struct WindowActivityRegistryInner {
    by_trace: BTreeMap<String, ActiveWindowRequest>,
    // A process-local synchronization fence, not an activity clock or authority.
    // No asynchronous visibility check may commit stall attention across a change.
    meaningful_revision: u64,
    meaningful_revision_exhausted: bool,
    // Missing completion evidence extends the existing bounded coverage model.
    // A later fully recorded meaningful call in the same principal+Window clears it.
    completion_coverage_gaps: BTreeMap<WindowContinuityKey, i64>,
    completion_coverage_overflow: bool,
    previous_meaningful: BTreeMap<WindowContinuityKey, CompletedMeaningfulCall>,
    // Active requests evicted by the global bounded registry are still owned by
    // their RAII guards. Keep a bounded principal-scoped count so one caller's
    // overflow does not normally degrade another caller's liveness projection.
    untracked_active_by_principal: BTreeMap<PrincipalCoverageKey, usize>,
    untracked_unscoped_count: usize,
    untracked_principal_overflow_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PrincipalCoverageKey {
    principal_kind: String,
    principal_id: String,
}

impl PrincipalCoverageKey {
    fn from_request(request: &ActiveWindowRequest) -> Option<Self> {
        Some(Self {
            principal_kind: request.principal_correlation_kind.clone()?,
            principal_id: request.principal_correlation_id.clone()?,
        })
    }

    fn from_continuity(key: &WindowContinuityKey) -> Self {
        Self {
            principal_kind: key.principal_kind.clone(),
            principal_id: key.principal_id.clone(),
        }
    }

    fn matches(&self, principal: (&str, &str)) -> bool {
        self.principal_kind == principal.0 && self.principal_id == principal.1
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct WindowContinuityKey {
    client_window_key: String,
    principal_kind: String,
    principal_id: String,
}

#[derive(Debug, Clone)]
struct CompletedMeaningfulCall {
    server_trace_id: String,
    request_observed_at_ms: i64,
    response_handed_at_ms: i64,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct WindowActivityRegistry {
    inner: Arc<Mutex<WindowActivityRegistryInner>>,
}

impl WindowActivityRegistryInner {
    fn advance_meaningful_revision(&mut self) {
        match self.meaningful_revision.checked_add(1) {
            Some(revision) => self.meaningful_revision = revision,
            None => self.meaningful_revision_exhausted = true,
        }
    }

    fn coverage_partial_for(&self, principal: Option<(&str, &str)>) -> bool {
        if self.meaningful_revision_exhausted || self.completion_coverage_overflow {
            return true;
        }
        match principal {
            None => {
                self.untracked_unscoped_count > 0
                    || self.untracked_principal_overflow_count > 0
                    || !self.untracked_active_by_principal.is_empty()
                    || !self.completion_coverage_gaps.is_empty()
            }
            Some(principal) => {
                self.untracked_principal_overflow_count > 0
                    || self
                        .untracked_active_by_principal
                        .keys()
                        .any(|key| key.matches(principal))
                    || self.completion_coverage_gaps.keys().any(|key| {
                        key.principal_kind == principal.0 && key.principal_id == principal.1
                    })
            }
        }
    }
}

impl WindowActivityRegistry {
    #[cfg(test)]
    pub(crate) fn start(
        &self,
        window: &ClientWindow,
        server_trace_id: &str,
        method: &str,
        principal: Option<(&str, &str)>,
    ) -> WindowActivityGuard {
        self.start_observed(
            window,
            server_trace_id,
            method,
            None,
            principal,
            chrono::Utc::now().timestamp_millis(),
        )
    }

    pub(crate) fn start_observed(
        &self,
        window: &ClientWindow,
        server_trace_id: &str,
        method: &str,
        tool_name: Option<&str>,
        principal: Option<(&str, &str)>,
        request_observed_at_ms: i64,
    ) -> WindowActivityGuard {
        let meaningful = method == "tools/call"
            && tool_name.is_some_and(|tool| {
                webcodex_tool_contracts::runtime_tool_activity_interaction(tool).is_meaningful()
            });
        let mut inner = self.inner.lock().expect("Window activity mutex poisoned");
        let continuity_key = principal.map(|(kind, id)| WindowContinuityKey {
            client_window_key: window.key().to_string(),
            principal_kind: kind.to_string(),
            principal_id: id.to_string(),
        });
        if meaningful {
            inner.advance_meaningful_revision();
        }
        let (transition, overlapped, previous_meaningful_call) = if meaningful {
            continuity_key
                .as_ref()
                .map(|key| {
                    classify_transition_and_mark_overlap(&mut inner, key, request_observed_at_ms)
                })
                .unwrap_or((WindowLoopTransition::Unavailable, false, None))
        } else {
            (WindowLoopTransition::Unavailable, false, None)
        };
        let record = ActiveWindowRequest {
            client_window_key: window.key().to_string(),
            client_window_source: window.source().to_string(),
            server_trace_id: server_trace_id.to_string(),
            method: method.to_string(),
            tool_name: tool_name.map(str::to_string),
            project: None,
            principal_correlation_kind: principal.map(|(kind, _)| kind.to_string()),
            principal_correlation_id: principal.map(|(_, id)| id.to_string()),
            meaningful,
            overlapped,
            started_at_ms: request_observed_at_ms,
        };
        if inner.by_trace.len() >= MAX_ACTIVE_WINDOW_REQUESTS {
            if let Some(oldest) = inner
                .by_trace
                .values()
                .min_by_key(|request| request.started_at_ms)
                .map(|request| request.server_trace_id.clone())
            {
                if let Some(evicted) = inner.by_trace.remove(&oldest) {
                    if let Some(key) = PrincipalCoverageKey::from_request(&evicted) {
                        if let Some(count) = inner.untracked_active_by_principal.get_mut(&key) {
                            *count = count.saturating_add(1);
                        } else if inner.untracked_principal_overflow_count == 0
                            && inner.untracked_active_by_principal.len()
                                < MAX_UNTRACKED_ACTIVE_PRINCIPALS
                        {
                            inner.untracked_active_by_principal.insert(key, 1);
                        } else {
                            inner.untracked_principal_overflow_count =
                                inner.untracked_principal_overflow_count.saturating_add(1);
                        }
                    } else {
                        inner.untracked_unscoped_count =
                            inner.untracked_unscoped_count.saturating_add(1);
                    }
                }
            }
        }
        inner.by_trace.insert(server_trace_id.to_string(), record);
        WindowActivityGuard {
            registry: self.clone(),
            server_trace_id: server_trace_id.to_string(),
            continuity_key,
            meaningful,
            request_observed_at_ms,
            transition,
            previous_meaningful_call,
            active: true,
        }
    }

    pub(crate) fn update(
        &self,
        server_trace_id: &str,
        tool_name: Option<&str>,
        project: Option<&str>,
    ) {
        let mut inner = self.inner.lock().expect("Window activity mutex poisoned");
        let Some(request) = inner.by_trace.get_mut(server_trace_id) else {
            return;
        };
        if let Some(tool_name) = tool_name {
            request.tool_name = Some(tool_name.to_string());
        }
        if let Some(project) = project {
            request.project = Some(project.to_string());
        }
    }

    pub(crate) fn list_for_window(
        &self,
        window_key: &str,
        principal: Option<(&str, &str)>,
    ) -> Vec<ActiveWindowRequest> {
        let inner = self.inner.lock().expect("Window activity mutex poisoned");
        let mut records = inner
            .by_trace
            .values()
            .filter(|request| {
                request.client_window_key == window_key && principal_visible(request, principal)
            })
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by(|a, b| {
            b.started_at_ms
                .cmp(&a.started_at_ms)
                .then_with(|| a.server_trace_id.cmp(&b.server_trace_id))
        });
        records
    }

    pub(crate) fn coverage_partial_for(&self, principal: Option<(&str, &str)>) -> bool {
        self.inner
            .lock()
            .map(|inner| inner.coverage_partial_for(principal))
            .unwrap_or(true)
    }

    pub(crate) fn meaningful_revision(&self) -> Option<u64> {
        self.inner.lock().ok().and_then(|inner| {
            (!inner.meaningful_revision_exhausted).then_some(inner.meaningful_revision)
        })
    }

    /// Linearize a narrow Goal attention commit against meaningful request start
    /// and finish. The callback is synchronous and must never reenter this registry.
    /// Principal-local active requests and bounded observation gaps fail closed.
    pub(crate) fn with_goal_stall_fence<T>(
        &self,
        expected_revision: u64,
        principal: (&str, &str),
        commit: impl FnOnce() -> T,
    ) -> Option<T> {
        let inner = self.inner.lock().ok()?;
        if inner.meaningful_revision != expected_revision
            || inner.coverage_partial_for(Some(principal))
            || inner.by_trace.values().any(|request| {
                principal_visible(request, Some(principal))
                    && (request.is_meaningful()
                        || (request.method == "tools/call" && request.tool_name.is_none()))
            })
        {
            return None;
        }
        Some(commit())
    }

    pub(crate) fn active_windows(
        &self,
        principal: Option<(&str, &str)>,
    ) -> Vec<ActiveWindowSummary> {
        let inner = self.inner.lock().expect("Window activity mutex poisoned");
        let mut summaries = BTreeMap::<String, ActiveWindowSummary>::new();
        for request in inner
            .by_trace
            .values()
            .filter(|request| principal_visible(request, principal))
        {
            summaries
                .entry(request.client_window_key.clone())
                .and_modify(|summary| {
                    summary.active_count = summary.active_count.saturating_add(1);
                    summary.last_started_at_ms =
                        summary.last_started_at_ms.max(request.started_at_ms);
                })
                .or_insert_with(|| ActiveWindowSummary {
                    client_window_key: request.client_window_key.clone(),
                    client_window_source: request.client_window_source.clone(),
                    active_count: 1,
                    last_started_at_ms: request.started_at_ms,
                });
        }
        let mut values = summaries.into_values().collect::<Vec<_>>();
        values.sort_by(|a, b| {
            b.last_started_at_ms
                .cmp(&a.last_started_at_ms)
                .then_with(|| a.client_window_key.cmp(&b.client_window_key))
        });
        values
    }

    #[cfg(test)]
    pub(crate) fn counts_by_window(
        &self,
        principal: Option<(&str, &str)>,
    ) -> BTreeMap<String, usize> {
        let inner = self.inner.lock().expect("Window activity mutex poisoned");
        let mut counts = BTreeMap::new();
        for request in inner
            .by_trace
            .values()
            .filter(|request| principal_visible(request, principal))
        {
            *counts.entry(request.client_window_key.clone()).or_insert(0) += 1;
        }
        counts
    }

    fn finish(
        &self,
        server_trace_id: &str,
        continuity_key: Option<&WindowContinuityKey>,
        meaningful: bool,
        request_observed_at_ms: i64,
        completion: Option<RequestCompletionTiming>,
        continuity_eligible: bool,
        evidence_recorded: bool,
    ) {
        let Ok(mut inner) = self.inner.lock() else {
            tracing::warn!(
                event = "window_activity_lock_poisoned",
                "window_activity_lock_poisoned"
            );
            return;
        };
        if meaningful {
            inner.advance_meaningful_revision();
            if let Some(key) = continuity_key {
                if evidence_recorded && completion.is_some() {
                    if inner
                        .completion_coverage_gaps
                        .get(key)
                        .is_some_and(|gap| request_observed_at_ms >= *gap)
                    {
                        inner.completion_coverage_gaps.remove(key);
                    }
                } else {
                    let missing_at = completion
                        .map(|timing| timing.response_handed_at_ms)
                        .unwrap_or(request_observed_at_ms);
                    if let Some(gap) = inner.completion_coverage_gaps.get_mut(key) {
                        *gap = (*gap).max(missing_at);
                    } else if inner.completion_coverage_gaps.len() < MAX_WINDOW_LOOP_CONTINUITIES {
                        inner
                            .completion_coverage_gaps
                            .insert(key.clone(), missing_at);
                    } else {
                        inner.completion_coverage_overflow = true;
                    }
                }
            } else if !evidence_recorded {
                inner.completion_coverage_overflow = true;
            }
        }
        let finished_request = inner.by_trace.remove(server_trace_id);
        if finished_request.is_none() {
            if let Some(key) = continuity_key.map(PrincipalCoverageKey::from_continuity) {
                let mut remove_key = false;
                if let Some(count) = inner.untracked_active_by_principal.get_mut(&key) {
                    *count = count.saturating_sub(1);
                    remove_key = *count == 0;
                } else if inner.untracked_principal_overflow_count > 0 {
                    inner.untracked_principal_overflow_count -= 1;
                }
                if remove_key {
                    inner.untracked_active_by_principal.remove(&key);
                }
            } else if inner.untracked_unscoped_count > 0 {
                inner.untracked_unscoped_count -= 1;
            }
        }
        let overlapped = finished_request
            .as_ref()
            .is_some_and(|request| request.overlapped);
        if meaningful && continuity_eligible && finished_request.is_some() && !overlapped {
            if let (Some(key), Some(completion)) = (continuity_key, completion) {
                let should_replace = inner.previous_meaningful.get(key).is_none_or(|previous| {
                    request_observed_at_ms >= previous.request_observed_at_ms
                });
                if should_replace {
                    inner.previous_meaningful.insert(
                        key.clone(),
                        CompletedMeaningfulCall {
                            server_trace_id: server_trace_id.to_string(),
                            request_observed_at_ms,
                            response_handed_at_ms: completion.response_handed_at_ms,
                        },
                    );
                }
                while inner.previous_meaningful.len() > MAX_WINDOW_LOOP_CONTINUITIES {
                    let Some(oldest) = inner
                        .previous_meaningful
                        .iter()
                        .min_by_key(|(_, previous)| previous.response_handed_at_ms)
                        .map(|(key, _)| key.clone())
                    else {
                        break;
                    };
                    inner.previous_meaningful.remove(&oldest);
                }
            }
        }
    }
}

fn classify_transition_and_mark_overlap(
    inner: &mut WindowActivityRegistryInner,
    key: &WindowContinuityKey,
    request_observed_at_ms: i64,
) -> (WindowLoopTransition, bool, Option<String>) {
    let active_overlap = inner.by_trace.values().any(|request| {
        request.meaningful
            && request.client_window_key == key.client_window_key
            && request.principal_correlation_kind.as_deref() == Some(key.principal_kind.as_str())
            && request.principal_correlation_id.as_deref() == Some(key.principal_id.as_str())
    });
    let previous_overlap = inner
        .previous_meaningful
        .get(key)
        .is_some_and(|previous| request_observed_at_ms < previous.response_handed_at_ms);
    if active_overlap || previous_overlap {
        // Once a meaningful sequence overlaps, there is no unambiguous adjacent
        // serial predecessor. Clear the prior anchor and mark every in-flight
        // member of this Window+principal overlap group so none can later
        // manufacture an outside-WebCodex gap. A later clean completion will
        // establish a fresh anchor for the following call.
        inner.previous_meaningful.remove(key);
        for request in inner.by_trace.values_mut() {
            if request.meaningful
                && request.client_window_key == key.client_window_key
                && request.principal_correlation_kind.as_deref()
                    == Some(key.principal_kind.as_str())
                && request.principal_correlation_id.as_deref() == Some(key.principal_id.as_str())
            {
                request.overlapped = true;
            }
        }
        return (WindowLoopTransition::Overlap, true, None);
    }
    // Consume the predecessor at arrival. Only this request's eligible
    // completion may establish the next anchor; cancellation, streaming,
    // timeout, or active-record eviction must not leave an older call behind.
    let Some(previous) = inner.previous_meaningful.remove(key) else {
        return (WindowLoopTransition::Unavailable, false, None);
    };
    (
        WindowLoopTransition::Serial {
            gap_ms: u64::try_from(request_observed_at_ms - previous.response_handed_at_ms)
                .unwrap_or(u64::MAX),
        },
        false,
        Some(previous.server_trace_id),
    )
}

fn principal_visible(request: &ActiveWindowRequest, principal: Option<(&str, &str)>) -> bool {
    match principal {
        None => true,
        Some((kind, id)) => {
            request.principal_correlation_kind.as_deref() == Some(kind)
                && request.principal_correlation_id.as_deref() == Some(id)
        }
    }
}

pub(crate) fn active_window_request_matches_principal(
    request: &ActiveWindowRequest,
    principal: (&str, &str),
) -> bool {
    principal_visible(request, Some(principal))
}

pub(crate) async fn window_project_visible_cached(
    runtime: &super::ToolRuntime,
    auth: &AuthContext,
    cache: &mut HashMap<String, bool>,
    project: Option<&str>,
) -> bool {
    let Some(project) = project else {
        return true;
    };
    if let Some(visible) = cache.get(project) {
        return *visible;
    }
    let visible = runtime.exact_project_visible_to_auth(auth, project).await;
    cache.insert(project.to_string(), visible);
    visible
}

pub(crate) async fn window_event_visible_cached(
    runtime: &super::ToolRuntime,
    auth: &AuthContext,
    cache: &mut HashMap<String, bool>,
    event: &webcodex_store::models::WindowActivityEventRecord,
) -> bool {
    if event.project.is_some() {
        return window_project_visible_cached(runtime, auth, cache, event.project.as_deref()).await;
    }
    if event.workflow_links.is_empty() {
        return true;
    }
    for link in &event.workflow_links {
        match link.project.as_deref() {
            None => return true,
            Some(project)
                if window_project_visible_cached(runtime, auth, cache, Some(project)).await =>
            {
                return true;
            }
            Some(_) => {}
        }
    }
    false
}

pub(crate) async fn active_window_request_visible_cached(
    runtime: &super::ToolRuntime,
    auth: &AuthContext,
    cache: &mut HashMap<String, bool>,
    request: &ActiveWindowRequest,
) -> bool {
    if request.project.is_some() {
        return window_project_visible_cached(runtime, auth, cache, request.project.as_deref())
            .await;
    }
    if auth.is_admin_caller() || request.method == "tools/list" {
        return true;
    }
    request.tool_name.as_deref().is_some_and(|tool| {
        !webcodex_tool_contracts::runtime_tool_activity_interaction(tool).is_meaningful()
    })
}

pub(crate) struct WindowActivityGuard {
    registry: WindowActivityRegistry,
    server_trace_id: String,
    continuity_key: Option<WindowContinuityKey>,
    meaningful: bool,
    request_observed_at_ms: i64,
    transition: WindowLoopTransition,
    previous_meaningful_call: Option<String>,
    active: bool,
}

impl WindowActivityGuard {
    /// Exact serial predecessor established by the existing principal/Window
    /// continuity registry. Observation only; absent after gaps or overlaps.
    pub(crate) fn previous_meaningful_call(&self) -> Option<&str> {
        // A later arrival can mark this call overlapped after its initial
        // transition was captured. Do not persist a serial chain through it.
        let inner = self.registry.inner.lock().ok()?;
        let current = inner.by_trace.get(&self.server_trace_id)?;
        if current.overlapped {
            None
        } else {
            self.previous_meaningful_call.as_deref()
        }
    }

    pub(crate) fn update(&self, tool_name: Option<&str>, project: Option<&str>) {
        self.registry
            .update(&self.server_trace_id, tool_name, project);
    }

    pub(crate) fn transition(&self) -> WindowLoopTransition {
        self.transition
    }

    pub(crate) fn complete(
        mut self,
        timing: RequestCompletionTiming,
        continuity_eligible: bool,
        evidence_recorded: bool,
    ) {
        if self.active {
            self.registry.finish(
                &self.server_trace_id,
                self.continuity_key.as_ref(),
                self.meaningful,
                self.request_observed_at_ms,
                Some(timing),
                continuity_eligible,
                evidence_recorded,
            );
            self.active = false;
        }
    }
}

impl Drop for WindowActivityGuard {
    fn drop(&mut self) {
        if self.active {
            self.registry.finish(
                &self.server_trace_id,
                self.continuity_key.as_ref(),
                self.meaningful,
                self.request_observed_at_ms,
                None,
                false,
                false,
            );
            self.active = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(key: &str) -> ClientWindow {
        ClientWindow::for_test(key)
    }

    fn window_key(value: &str) -> String {
        window(value).key().to_string()
    }

    #[test]
    fn live_window_registry_tracks_concurrent_windows_and_raii_cleanup() {
        let registry = WindowActivityRegistry::default();
        let first = registry.start(
            &window("w1"),
            "trace-1",
            "tools/call",
            Some(("username", "alice")),
        );
        first.update(Some("read_files"), Some("agent:r:p"));
        let second = registry.start(
            &window("w2"),
            "trace-2",
            "tools/call",
            Some(("username", "bob")),
        );
        assert_eq!(
            registry
                .list_for_window(&window_key("w1"), Some(("username", "alice")))
                .len(),
            1
        );
        assert_eq!(
            registry
                .list_for_window(&window_key("w2"), Some(("username", "bob")))
                .len(),
            1
        );
        assert_eq!(
            registry
                .counts_by_window(Some(("username", "alice")))
                .get(window_key("w1").as_str()),
            Some(&1)
        );
        drop(first);
        assert!(registry
            .list_for_window(&window_key("w1"), Some(("username", "alice")))
            .is_empty());
        assert_eq!(
            registry
                .list_for_window(&window_key("w2"), Some(("username", "bob")))
                .len(),
            1
        );
        drop(second);
        assert!(registry.counts_by_window(None).is_empty());
    }

    #[test]
    fn live_window_registry_filters_principals_before_key_lookup() {
        let registry = WindowActivityRegistry::default();
        let _alice = registry.start(
            &window("wa"),
            "trace-alice",
            "tools/call",
            Some(("username", "alice")),
        );
        let _bob = registry.start(
            &window("wb"),
            "trace-bob",
            "tools/call",
            Some(("username", "bob")),
        );
        assert_eq!(
            registry
                .list_for_window(&window_key("wa"), Some(("username", "alice")))
                .len(),
            1
        );
        assert!(registry
            .list_for_window(&window_key("wb"), Some(("username", "alice")))
            .is_empty());
        assert_eq!(
            registry
                .counts_by_window(Some(("username", "alice")))
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec![window_key("wa")]
        );
    }

    #[test]
    fn sequential_requests_do_not_leave_phantom_active_state() {
        let registry = WindowActivityRegistry::default();
        {
            let _first = registry.start(
                &window("w"),
                "trace-a",
                "tools/list",
                Some(("username", "alice")),
            );
            assert_eq!(
                registry
                    .list_for_window(&window_key("w"), Some(("username", "alice")))
                    .len(),
                1
            );
        }
        assert!(registry
            .list_for_window(&window_key("w"), Some(("username", "alice")))
            .is_empty());
        {
            let _second = registry.start(
                &window("w"),
                "trace-b",
                "tools/call",
                Some(("username", "alice")),
            );
            assert_eq!(
                registry
                    .list_for_window(&window_key("w"), Some(("username", "alice")))
                    .len(),
                1
            );
        }
        assert!(registry
            .list_for_window(&window_key("w"), Some(("username", "alice")))
            .is_empty());
    }

    fn completion(started_at_ms: i64, response_handed_at_ms: i64) -> RequestCompletionTiming {
        RequestCompletionTiming {
            request_observed_at_ms: started_at_ms,
            response_handed_at_ms,
            elapsed_ms: u64::try_from(response_handed_at_ms - started_at_ms).unwrap(),
        }
    }

    fn meaningful_start(
        registry: &WindowActivityRegistry,
        window: &ClientWindow,
        trace: &str,
        principal: (&str, &str),
        at_ms: i64,
    ) -> WindowActivityGuard {
        registry.start_observed(
            window,
            trace,
            "tools/call",
            Some("read_files"),
            Some(principal),
            at_ms,
        )
    }

    #[test]
    fn meaningful_sequential_gap_uses_previous_response_handoff() {
        let registry = WindowActivityRegistry::default();
        let window = window("sequential");
        let first = meaningful_start(
            &registry,
            &window,
            "trace-first",
            ("username", "alice"),
            1_000,
        );
        assert_eq!(first.transition(), WindowLoopTransition::Unavailable);
        first.complete(completion(1_000, 1_125), true, true);

        let second = meaningful_start(
            &registry,
            &window,
            "trace-second",
            ("username", "alice"),
            1_500,
        );
        assert_eq!(second.transition().gap_ms(), Some(375));
        assert_eq!(second.previous_meaningful_call(), Some("trace-first"));
    }

    #[test]
    fn meaningful_continuity_requires_same_window_and_principal() {
        let registry = WindowActivityRegistry::default();
        let first_window = window("identity-a");
        let second_window = window("identity-b");
        meaningful_start(
            &registry,
            &first_window,
            "trace-first",
            ("username", "alice"),
            1_000,
        )
        .complete(completion(1_000, 1_050), true, true);

        let different_principal = meaningful_start(
            &registry,
            &first_window,
            "trace-bob",
            ("username", "bob"),
            1_200,
        );
        assert_eq!(
            different_principal.transition(),
            WindowLoopTransition::Unavailable
        );
        drop(different_principal);

        let different_window = meaningful_start(
            &registry,
            &second_window,
            "trace-other-window",
            ("username", "alice"),
            1_300,
        );
        assert_eq!(
            different_window.transition(),
            WindowLoopTransition::Unavailable
        );
    }

    #[test]
    fn discovery_call_does_not_break_meaningful_cadence() {
        let registry = WindowActivityRegistry::default();
        let window = window("meaningful-cadence");
        meaningful_start(
            &registry,
            &window,
            "trace-first",
            ("username", "alice"),
            1_000,
        )
        .complete(completion(1_000, 1_100), true, true);

        let discovery = registry.start_observed(
            &window,
            "trace-status",
            "tools/call",
            Some("runtime_status"),
            Some(("username", "alice")),
            1_200,
        );
        assert_eq!(discovery.transition(), WindowLoopTransition::Unavailable);
        discovery.complete(completion(1_200, 1_225), true, true);

        let second = meaningful_start(
            &registry,
            &window,
            "trace-second",
            ("username", "alice"),
            1_500,
        );
        assert_eq!(second.transition().gap_ms(), Some(400));
    }

    #[test]
    fn observe_jobs_transport_remains_meaningful_for_window_cadence() {
        let registry = WindowActivityRegistry::default();
        let window = window("observe-jobs-cadence");
        meaningful_start(
            &registry,
            &window,
            "trace-first",
            ("username", "alice"),
            1_000,
        )
        .complete(completion(1_000, 1_100), true, true);

        let observation = registry.start_observed(
            &window,
            "trace-observe-jobs",
            "tools/call",
            Some("observe_jobs"),
            Some(("username", "alice")),
            1_200,
        );
        assert_eq!(observation.transition().gap_ms(), Some(100));
        let requests = registry.list_for_window(
            &window_key("observe-jobs-cadence"),
            Some(("username", "alice")),
        );
        assert!(requests
            .iter()
            .find(|request| request.server_trace_id == "trace-observe-jobs")
            .expect("observe_jobs request")
            .is_meaningful());
        observation.complete(completion(1_200, 1_225), true, true);

        let followup = meaningful_start(
            &registry,
            &window,
            "trace-followup",
            ("username", "alice"),
            1_500,
        );
        assert_eq!(followup.transition().gap_ms(), Some(275));
    }

    #[test]
    fn later_overlap_invalidates_job_telemetry_predecessor_before_completion() {
        let registry = WindowActivityRegistry::default();
        let window = window("late-overlap");
        meaningful_start(&registry, &window, "pending", ("username", "alice"), 1000).complete(
            completion(1000, 1100),
            true,
            true,
        );
        let first = meaningful_start(&registry, &window, "passive", ("username", "alice"), 1200);
        assert_eq!(first.previous_meaningful_call(), Some("pending"));
        let overlapping =
            meaningful_start(&registry, &window, "observe", ("username", "alice"), 1250);
        assert_eq!(first.previous_meaningful_call(), None);
        assert_eq!(overlapping.previous_meaningful_call(), None);
    }

    #[test]
    fn overlapping_meaningful_calls_never_emit_negative_serial_gap() {
        let registry = WindowActivityRegistry::default();
        let window = window("overlap");
        let first = meaningful_start(
            &registry,
            &window,
            "trace-first",
            ("username", "alice"),
            1_000,
        );
        let second = meaningful_start(
            &registry,
            &window,
            "trace-second",
            ("username", "alice"),
            1_050,
        );
        assert_eq!(second.transition(), WindowLoopTransition::Overlap);
        assert_eq!(second.previous_meaningful_call(), None);
        assert_eq!(second.transition().gap_ms(), None);
        first.complete(completion(1_000, 1_200), true, true);
        second.complete(completion(1_050, 1_250), true, true);

        let after_overlap = meaningful_start(
            &registry,
            &window,
            "trace-after-overlap",
            ("username", "alice"),
            1_500,
        );
        assert_eq!(
            after_overlap.transition(),
            WindowLoopTransition::Unavailable,
            "an overlap group must invalidate the serial anchor instead of leaking WebCodex overlap time into an outside gap"
        );
        after_overlap.complete(completion(1_500, 1_550), true, true);

        let clean_followup = meaningful_start(
            &registry,
            &window,
            "trace-clean-followup",
            ("username", "alice"),
            1_700,
        );
        assert_eq!(clean_followup.transition().gap_ms(), Some(150));
    }

    #[test]
    fn interrupted_meaningful_call_consumes_existing_anchor() {
        for complete_ineligible in [false, true] {
            let registry = WindowActivityRegistry::default();
            let window = window("interrupted");
            let principal = ("username", "alice");
            meaningful_start(&registry, &window, "first", principal, 1_000).complete(
                completion(1_000, 1_100),
                true,
                true,
            );
            let interrupted = meaningful_start(&registry, &window, "interrupted", principal, 1_200);
            assert_eq!(interrupted.transition().gap_ms(), Some(100));
            if complete_ineligible {
                interrupted.complete(completion(1_200, 1_400), false, true);
            } else {
                drop(interrupted);
            }
            let next = meaningful_start(&registry, &window, "next", principal, 1_500);
            assert_eq!(next.transition(), WindowLoopTransition::Unavailable);
            next.complete(completion(1_500, 1_600), true, true);
            let recovered = meaningful_start(&registry, &window, "recovered", principal, 1_700);
            assert_eq!(recovered.transition().gap_ms(), Some(100));
        }
    }

    #[test]
    fn app_control_requests_remain_seen_but_are_not_meaningful() {
        let registry = WindowActivityRegistry::default();
        let window = window("app-control-classification");
        let app_control = registry.start_observed(
            &window,
            "trace-goal-plan-state",
            "tools/call",
            Some("goal_plan_sync"),
            Some(("username", "alice")),
            1_000,
        );
        let business = registry.start_observed(
            &window,
            "trace-read-files",
            "tools/call",
            Some("read_files"),
            Some(("username", "alice")),
            1_001,
        );
        let requests = registry.list_for_window(
            &window_key("app-control-classification"),
            Some(("username", "alice")),
        );
        assert_eq!(requests.len(), 2);
        assert!(!requests
            .iter()
            .find(|request| request.server_trace_id == "trace-goal-plan-state")
            .unwrap()
            .is_meaningful());
        assert!(requests
            .iter()
            .find(|request| request.server_trace_id == "trace-read-files")
            .unwrap()
            .is_meaningful());
        drop((app_control, business));
    }

    #[test]
    fn active_request_eviction_marks_observation_partial_until_evicted_guard_finishes() {
        let registry = WindowActivityRegistry::default();
        let mut guards = Vec::new();
        for index in 0..=MAX_ACTIVE_WINDOW_REQUESTS {
            guards.push(registry.start_observed(
                &window(&format!("bounded-{index}")),
                &format!("trace-bounded-{index}"),
                "tools/call",
                Some("run_process"),
                Some(("username", "alice")),
                1_000 + index as i64,
            ));
        }
        assert!(registry.coverage_partial_for(Some(("username", "alice"))));
        assert!(!registry.coverage_partial_for(Some(("username", "bob"))));
        assert_eq!(
            registry.active_windows(None).len(),
            MAX_ACTIVE_WINDOW_REQUESTS
        );
        guards
            .remove(0)
            .complete(completion(1_000, 10_000), true, true);
        assert!(!registry.coverage_partial_for(Some(("username", "alice"))));
        drop(guards);
    }

    #[test]
    fn restart_or_ineligible_completion_does_not_invent_continuity() {
        let window = window("restart");
        let registry = WindowActivityRegistry::default();
        meaningful_start(
            &registry,
            &window,
            "trace-stream",
            ("username", "alice"),
            1_000,
        )
        .complete(completion(1_000, 1_100), false, true);
        let after_stream = meaningful_start(
            &registry,
            &window,
            "trace-after-stream",
            ("username", "alice"),
            1_300,
        );
        assert_eq!(after_stream.transition(), WindowLoopTransition::Unavailable);
        after_stream.complete(completion(1_300, 1_350), true, true);

        let restarted_registry = WindowActivityRegistry::default();
        let after_restart = meaningful_start(
            &restarted_registry,
            &window,
            "trace-after-restart",
            ("username", "alice"),
            1_600,
        );
        assert_eq!(
            after_restart.transition(),
            WindowLoopTransition::Unavailable
        );
    }

    #[test]
    fn goal_stall_fence_rejects_new_work_across_snapshot_and_unrecorded_completion() {
        let registry = WindowActivityRegistry::default();
        let window = window("goal-stall-fence");
        let principal = ("username", "alice");
        let before = registry.meaningful_revision().unwrap();
        let request = meaningful_start(&registry, &window, "new-work", principal, 100);
        assert!(registry
            .with_goal_stall_fence(before, principal, || panic!("stale snapshot committed"))
            .is_none());
        let during = registry.meaningful_revision().unwrap();
        assert!(registry
            .with_goal_stall_fence(during, principal, || panic!("active request committed"))
            .is_none());
        request.complete(completion(100, 200), true, true);
        assert!(registry
            .with_goal_stall_fence(before, principal, || panic!("completed new work was lost"))
            .is_none());
        let current = registry.meaningful_revision().unwrap();
        assert_eq!(
            registry.with_goal_stall_fence(current, principal, || 1),
            Some(1)
        );
        let missing = meaningful_start(&registry, &window, "missing-audit", principal, 300);
        missing.complete(completion(300, 400), true, false);
        assert!(registry.coverage_partial_for(Some(principal)));
        assert!(!registry.coverage_partial_for(Some(("username", "bob"))));
        assert!(registry
            .with_goal_stall_fence(
                registry.meaningful_revision().unwrap(),
                principal,
                || panic!("missing audit treated as silence")
            )
            .is_none());
        meaningful_start(&registry, &window, "fresh-work", principal, 500).complete(
            completion(500, 600),
            true,
            true,
        );
        assert_eq!(
            registry.with_goal_stall_fence(
                registry.meaningful_revision().unwrap(),
                principal,
                || 2
            ),
            Some(2)
        );
    }

    #[test]
    fn goal_stall_commit_and_new_request_start_have_a_single_linearization_order() {
        let registry = WindowActivityRegistry::default();
        let window = window("goal-stall-linearization");
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (go_tx, go_rx) = std::sync::mpsc::channel();
        let registry2 = registry.clone();
        let handle = std::thread::spawn(move || {
            go_rx.recv().unwrap();
            ready_tx.send(()).unwrap();
            let request = meaningful_start(
                &registry2,
                &window,
                "concurrent-work",
                ("username", "alice"),
                100,
            );
            request.complete(completion(100, 200), true, true);
        });
        let revision = registry.meaningful_revision().unwrap();
        assert_eq!(
            registry.with_goal_stall_fence(revision, ("username", "alice"), || {
                go_tx.send(()).unwrap();
                ready_rx.recv().unwrap();
                // The request start must wait for this commit boundary to release.
                3
            }),
            Some(3)
        );
        handle.join().unwrap();
        assert!(registry
            .with_goal_stall_fence(revision, ("username", "alice"), || panic!(
                "old snapshot committed after request"
            ))
            .is_none());
    }
}
