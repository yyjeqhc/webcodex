use super::{RecoveryKind, ToolResult, ToolRuntime};
use crate::auth::scopes::{
    SCOPE_COMMUNICATION_MANAGE, SCOPE_COMMUNICATION_READ, SCOPE_PROJECT_READ,
    SCOPE_SESSION_COLLABORATE,
};
use crate::auth::{AuthContext, SCOPE_RUNTIME_READ};
use crate::db::{
    AgentWakeState, GoalCheckpoint, GoalCorrelationKind, GoalDetail, GoalLifecycle, GoalPatch,
    GoalStallHostDeliveryObservation, GoalStep, GoalStoreError, NewGoal, MAX_GOAL_LIST_LIMIT,
};
use serde::Serialize;
use serde_json::{json, to_value};
use std::collections::{BTreeSet, HashMap};

const DEFAULT_GOAL_LIST_LIMIT: usize = 50;
pub(crate) const GOAL_ACTIVITY_ATTENTION_AFTER_MS: i64 =
    crate::db::GOAL_ACTIVITY_ATTENTION_AFTER_MS;
const GOAL_ACTIVITY_SESSION_SCAN_LIMIT: usize = 16;
const GOAL_ACTIVITY_WINDOW_SCAN_LIMIT: usize = 16;
const GOAL_ACTIVITY_EVENT_SCAN_LIMIT: usize = 64;
const GOAL_ACTIVITY_PROJECT_VISIBILITY_LIMIT: usize = 32;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GoalActivityState {
    Active,
    AttentionNeeded,
    Unobserved,
    NotApplicable,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct GoalActivityObservation {
    pub available: bool,
    pub state: GoalActivityState,
    pub idle_threshold_ms: i64,
    pub observation_lease_ms: i64,
    pub last_seen_at_ms: Option<i64>,
    pub last_meaningful_activity_at_ms: Option<i64>,
    pub quiet_for_ms: Option<i64>,
    pub linked_window_count: Option<usize>,
    pub active_meaningful_request_count: Option<usize>,
    pub coverage_partial: bool,
}

impl GoalActivityObservation {
    fn unavailable(state: GoalActivityState) -> Self {
        Self {
            available: false,
            state,
            idle_threshold_ms: GOAL_ACTIVITY_ATTENTION_AFTER_MS,
            observation_lease_ms: crate::db::GOAL_CARD_OBSERVATION_LEASE_MS,
            last_seen_at_ms: None,
            last_meaningful_activity_at_ms: None,
            quiet_for_ms: None,
            linked_window_count: None,
            active_meaningful_request_count: None,
            coverage_partial: false,
        }
    }

    fn not_applicable(available: bool) -> Self {
        if !available {
            return Self::unavailable(GoalActivityState::NotApplicable);
        }
        Self {
            available: true,
            state: GoalActivityState::NotApplicable,
            idle_threshold_ms: GOAL_ACTIVITY_ATTENTION_AFTER_MS,
            observation_lease_ms: crate::db::GOAL_CARD_OBSERVATION_LEASE_MS,
            last_seen_at_ms: None,
            last_meaningful_activity_at_ms: None,
            quiet_for_ms: None,
            linked_window_count: None,
            active_meaningful_request_count: None,
            coverage_partial: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GoalContinuityState {
    Ready,
    Stalled,
    WakeQueued,
    Dispatching,
    HostAccepted,
    HostUnknown,
    ResumeConfirmed,
    NotConfigured,
    NotApplicable,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GoalHostDeliveryState {
    NotStarted,
    Dispatching,
    Accepted,
    Unknown,
    NotConfirmed,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GoalFreshTurnState {
    NotConfirmed,
    Confirmed,
    NotApplicable,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct GoalContinuityObservation {
    pub available: bool,
    pub state: GoalContinuityState,
    pub production_auto_resume_available: bool,
    pub wake_state: Option<AgentWakeState>,
    pub host_delivery: GoalHostDeliveryState,
    pub fresh_turn: GoalFreshTurnState,
    pub attention_candidate_at_unix_ms: Option<i64>,
    pub attention_created_at_unix_ms: Option<i64>,
    pub wake_created_at_unix_ms: Option<i64>,
    pub host_dispatch_prepared_at_unix_ms: Option<i64>,
    pub host_dispatch_accepted_at_unix_ms: Option<i64>,
    pub host_dispatch_unknown_at_unix_ms: Option<i64>,
    pub wake_consumed_at_unix_ms: Option<i64>,
    pub first_post_resume_meaningful_at_unix_ms: Option<i64>,
    pub last_post_resume_meaningful_at_unix_ms: Option<i64>,
    pub last_resume_at_unix_ms: Option<i64>,
}

impl GoalContinuityObservation {
    fn unavailable() -> Self {
        Self {
            available: false,
            state: GoalContinuityState::Unavailable,
            production_auto_resume_available: false,
            wake_state: None,
            host_delivery: GoalHostDeliveryState::NotApplicable,
            fresh_turn: GoalFreshTurnState::NotApplicable,
            attention_candidate_at_unix_ms: None,
            attention_created_at_unix_ms: None,
            wake_created_at_unix_ms: None,
            host_dispatch_prepared_at_unix_ms: None,
            host_dispatch_accepted_at_unix_ms: None,
            host_dispatch_unknown_at_unix_ms: None,
            wake_consumed_at_unix_ms: None,
            first_post_resume_meaningful_at_unix_ms: None,
            last_post_resume_meaningful_at_unix_ms: None,
            last_resume_at_unix_ms: None,
        }
    }

    fn not_configured() -> Self {
        Self {
            available: true,
            state: GoalContinuityState::NotConfigured,
            production_auto_resume_available: false,
            wake_state: None,
            host_delivery: GoalHostDeliveryState::NotApplicable,
            fresh_turn: GoalFreshTurnState::NotApplicable,
            attention_candidate_at_unix_ms: None,
            attention_created_at_unix_ms: None,
            wake_created_at_unix_ms: None,
            host_dispatch_prepared_at_unix_ms: None,
            host_dispatch_accepted_at_unix_ms: None,
            host_dispatch_unknown_at_unix_ms: None,
            wake_consumed_at_unix_ms: None,
            first_post_resume_meaningful_at_unix_ms: None,
            last_post_resume_meaningful_at_unix_ms: None,
            last_resume_at_unix_ms: None,
        }
    }

    fn not_applicable() -> Self {
        Self {
            available: true,
            state: GoalContinuityState::NotApplicable,
            production_auto_resume_available: false,
            wake_state: None,
            host_delivery: GoalHostDeliveryState::NotApplicable,
            fresh_turn: GoalFreshTurnState::NotApplicable,
            attention_candidate_at_unix_ms: None,
            attention_created_at_unix_ms: None,
            wake_created_at_unix_ms: None,
            host_dispatch_prepared_at_unix_ms: None,
            host_dispatch_accepted_at_unix_ms: None,
            host_dispatch_unknown_at_unix_ms: None,
            wake_consumed_at_unix_ms: None,
            first_post_resume_meaningful_at_unix_ms: None,
            last_post_resume_meaningful_at_unix_ms: None,
            last_resume_at_unix_ms: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct GoalPlanProjection {
    pub version: u8,
    pub goal_id: String,
    pub title: String,
    pub total_step_count: usize,
    pub completed_step_count: usize,
    pub current_step_id: Option<String>,
    pub steps: Vec<GoalStep>,
    pub progress_summary: Option<String>,
    pub checkpoint_at_unix_ms: Option<i64>,
    pub controller_agent_id: Option<String>,
    pub lifecycle: GoalLifecycle,
    pub revision: i64,
    pub updated_at_unix_ms: i64,
    pub terminal_at_unix_ms: Option<i64>,
    pub agent_task_count: i64,
    pub workflow_session_count: i64,
    pub activity: GoalActivityObservation,
    pub continuity: GoalContinuityObservation,
}

fn goal_plan_projection(
    goal: GoalDetail,
    activity: GoalActivityObservation,
    continuity: GoalContinuityObservation,
) -> GoalPlanProjection {
    GoalPlanProjection {
        version: 3,
        goal_id: goal.summary.goal_id,
        title: goal.summary.title,
        total_step_count: goal.plan.steps.len(),
        completed_step_count: goal.plan.completed_count(),
        current_step_id: goal.plan.current_step().map(|step| step.id.clone()),
        steps: goal.plan.steps,
        progress_summary: goal.plan.progress_summary,
        checkpoint_at_unix_ms: goal.plan.checkpoint_at_unix_ms,
        controller_agent_id: goal.controller_agent_id,
        lifecycle: goal.summary.lifecycle,
        revision: goal.summary.revision,
        updated_at_unix_ms: goal.summary.updated_at_unix_ms,
        terminal_at_unix_ms: goal.summary.terminal_at_unix_ms,
        agent_task_count: goal.summary.agent_task_count,
        workflow_session_count: goal.summary.workflow_session_count,
        activity,
        continuity,
    }
}

fn goal_principal(
    auth: Option<&AuthContext>,
) -> Result<crate::db::CommunicationPrincipal, ToolResult> {
    super::communication::communication_principal(auth)
}

fn goal_continuity_observation(
    runtime: &ToolRuntime,
    principal: &crate::db::CommunicationPrincipal,
    goal: &GoalDetail,
    activity: &GoalActivityObservation,
) -> GoalContinuityObservation {
    if goal.summary.lifecycle.terminal() {
        return GoalContinuityObservation::not_applicable();
    }
    let Some(controller_agent_id) = goal.controller_agent_id.as_deref() else {
        return GoalContinuityObservation::not_configured();
    };
    let Some(db) = runtime.communication_db.as_ref() else {
        return GoalContinuityObservation::unavailable();
    };
    let agents = match db.list_agent_identities(principal, Some(controller_agent_id), 0, 1) {
        Ok(page) => page.agents,
        Err(_) => return GoalContinuityObservation::unavailable(),
    };
    let Some(agent) = agents.into_iter().next() else {
        return GoalContinuityObservation::unavailable();
    };
    if agent.agent_id != controller_agent_id {
        return GoalContinuityObservation::unavailable();
    }
    let production_auto_resume_available = agent.active_endpoint_count > 0
        && runtime
            .agent_continuations
            .as_ref()
            .is_some_and(|continuations| {
                continuations.production_auto_resume_available(
                    principal,
                    controller_agent_id,
                    agent.current_controller_generation,
                )
            });
    if !activity.available || activity.coverage_partial {
        return GoalContinuityObservation {
            production_auto_resume_available,
            ..GoalContinuityObservation::unavailable()
        };
    }
    let attention_candidate_at_unix_ms = activity
        .last_meaningful_activity_at_ms
        .map(|last| last.saturating_add(GOAL_ACTIVITY_ATTENTION_AFTER_MS));
    let durable = match db.read_goal_stall_continuity(
        principal,
        &goal.summary.goal_id,
        activity.last_meaningful_activity_at_ms,
        controller_agent_id,
    ) {
        Ok(observation) => observation,
        Err(_) => return GoalContinuityObservation::unavailable(),
    };
    let Some(wake) = durable.current_wake else {
        let resume = durable.last_resume.as_ref();
        let (host_delivery, fresh_turn) = if let Some(resume) = resume {
            let host_delivery = if resume.host_dispatch_accepted_at_unix_ms.is_some() {
                GoalHostDeliveryState::Accepted
            } else if resume.host_dispatch_unknown_at_unix_ms.is_some() {
                GoalHostDeliveryState::Unknown
            } else {
                GoalHostDeliveryState::NotConfirmed
            };
            (host_delivery, GoalFreshTurnState::Confirmed)
        } else {
            (
                GoalHostDeliveryState::NotStarted,
                GoalFreshTurnState::NotConfirmed,
            )
        };
        return GoalContinuityObservation {
            available: true,
            state: if activity.state == GoalActivityState::AttentionNeeded {
                GoalContinuityState::Stalled
            } else {
                GoalContinuityState::Ready
            },
            production_auto_resume_available,
            wake_state: None,
            host_delivery,
            fresh_turn,
            attention_candidate_at_unix_ms: resume
                .map(|value| value.attention_candidate_at_unix_ms)
                .or(attention_candidate_at_unix_ms),
            attention_created_at_unix_ms: resume.map(|value| value.attention_created_at_unix_ms),
            wake_created_at_unix_ms: resume.map(|value| value.wake_created_at_unix_ms),
            host_dispatch_prepared_at_unix_ms: resume
                .and_then(|value| value.dispatch_prepared_at_unix_ms),
            host_dispatch_accepted_at_unix_ms: resume
                .and_then(|value| value.host_dispatch_accepted_at_unix_ms),
            host_dispatch_unknown_at_unix_ms: resume
                .and_then(|value| value.host_dispatch_unknown_at_unix_ms),
            wake_consumed_at_unix_ms: resume.map(|value| value.consumed_at_unix_ms),
            first_post_resume_meaningful_at_unix_ms: resume
                .and_then(|value| value.first_post_resume_meaningful_at_unix_ms),
            last_post_resume_meaningful_at_unix_ms: resume
                .and_then(|value| value.last_post_resume_meaningful_at_unix_ms),
            last_resume_at_unix_ms: resume.map(|value| value.consumed_at_unix_ms),
        };
    };
    let state = match wake.state {
        AgentWakeState::Pending | AgentWakeState::Claimed => GoalContinuityState::WakeQueued,
        AgentWakeState::Prepared => GoalContinuityState::Dispatching,
        AgentWakeState::Delivered => GoalContinuityState::HostAccepted,
        AgentWakeState::DeliveryUnknown => GoalContinuityState::HostUnknown,
        AgentWakeState::Consumed => GoalContinuityState::ResumeConfirmed,
        AgentWakeState::Retired => GoalContinuityState::Stalled,
    };
    let host_delivery = match wake.state {
        AgentWakeState::Pending | AgentWakeState::Claimed | AgentWakeState::Retired => {
            GoalHostDeliveryState::NotStarted
        }
        AgentWakeState::Prepared => GoalHostDeliveryState::Dispatching,
        AgentWakeState::Delivered => GoalHostDeliveryState::Accepted,
        AgentWakeState::DeliveryUnknown => GoalHostDeliveryState::Unknown,
        AgentWakeState::Consumed => match wake.host_delivery {
            GoalStallHostDeliveryObservation::Accepted => GoalHostDeliveryState::Accepted,
            GoalStallHostDeliveryObservation::Unknown => GoalHostDeliveryState::Unknown,
            GoalStallHostDeliveryObservation::NotObserved => GoalHostDeliveryState::NotConfirmed,
        },
    };
    GoalContinuityObservation {
        available: true,
        state,
        production_auto_resume_available,
        wake_state: Some(wake.state),
        host_delivery,
        fresh_turn: if wake.state == AgentWakeState::Consumed {
            GoalFreshTurnState::Confirmed
        } else {
            GoalFreshTurnState::NotConfirmed
        },
        attention_candidate_at_unix_ms,
        attention_created_at_unix_ms: Some(wake.attention_created_at_unix_ms),
        wake_created_at_unix_ms: Some(wake.wake_created_at_unix_ms),
        host_dispatch_prepared_at_unix_ms: wake.dispatch_prepared_at_unix_ms,
        host_dispatch_accepted_at_unix_ms: wake.host_dispatch_accepted_at_unix_ms,
        host_dispatch_unknown_at_unix_ms: wake.host_dispatch_unknown_at_unix_ms,
        wake_consumed_at_unix_ms: wake.consumed_at_unix_ms,
        first_post_resume_meaningful_at_unix_ms: wake.first_post_resume_meaningful_at_unix_ms,
        last_post_resume_meaningful_at_unix_ms: wake.last_post_resume_meaningful_at_unix_ms,
        last_resume_at_unix_ms: durable
            .last_resume
            .as_ref()
            .map(|value| value.consumed_at_unix_ms),
    }
}

fn goal_store_unavailable() -> ToolResult {
    ToolResult::err_with_output(
        "Durable Goal storage is unavailable in this runtime",
        json!({
            "error_kind": "goal_store_unavailable",
            "state_changed": false,
        }),
    )
    .with_recovery(RecoveryKind::UserAction)
}

fn goal_error(error: GoalStoreError, store_failure_recovery: RecoveryKind) -> ToolResult {
    let recovery = match error.code() {
        "goal_store_unavailable" => store_failure_recovery,
        "goal_not_found" | "agent_not_found" | "goal_revision_changed" | "goal_terminal" => {
            RecoveryKind::Reobserve
        }
        "goal_idempotency_conflict" => RecoveryKind::Reobserve,
        _ => RecoveryKind::FixInput,
    };
    ToolResult::err_with_output(
        error.message(),
        json!({
            "error_kind": error.code(),
            "message": error.message(),
            "current_revision": error.current_revision(),
            "state_changed": false,
        }),
    )
    .with_recovery(recovery)
}

fn target_authorization_error(error: crate::db::CommunicationStoreError) -> ToolResult {
    ToolResult::err_with_output(
        error.message(),
        json!({
            "error_kind": error.code(),
            "message": error.message(),
            "state_changed": false,
        }),
    )
    .with_recovery(RecoveryKind::Reobserve)
}

fn serialized_goal_success<T: Serialize>(value: T) -> ToolResult {
    match to_value(value) {
        Ok(value) => ToolResult::ok(value),
        Err(error) => ToolResult::err_with_output(
            format!("Failed to serialize durable Goal result: {error}"),
            json!({
                "error_kind": "goal_result_serialization_failed",
                "state_changed": false,
            }),
        )
        .with_recovery(RecoveryKind::NoAction),
    }
}

fn event_visibility_budget_available(
    event: &webcodex_store::models::WindowActivityEventRecord,
    cache: &HashMap<String, bool>,
) -> bool {
    let mut unknown = BTreeSet::new();
    if let Some(project) = event.project.as_deref() {
        if cache.contains_key(project) {
            return true;
        }
        unknown.insert(project);
    } else {
        if event.workflow_links.is_empty()
            || event
                .workflow_links
                .iter()
                .any(|link| link.project.is_none())
            || event.workflow_links.iter().any(|link| {
                link.project
                    .as_deref()
                    .and_then(|project| cache.get(project))
                    == Some(&true)
            })
        {
            return true;
        }
        for project in event
            .workflow_links
            .iter()
            .filter_map(|link| link.project.as_deref())
        {
            if !cache.contains_key(project) {
                unknown.insert(project);
            }
        }
    }
    cache.len().saturating_add(unknown.len()) <= GOAL_ACTIVITY_PROJECT_VISIBILITY_LIMIT
}

fn request_visibility_budget_available(
    request: &super::ActiveWindowRequest,
    cache: &HashMap<String, bool>,
) -> bool {
    request.project.as_deref().is_none_or(|project| {
        cache.contains_key(project) || cache.len() < GOAL_ACTIVITY_PROJECT_VISIBILITY_LIMIT
    })
}

impl ToolRuntime {
    pub(crate) async fn prepare_goal_workflow(
        &self,
        auth: Option<&AuthContext>,
        session_id: String,
        input: NewGoal,
    ) -> ToolResult {
        // Exact Workflow Session authority is a Runtime concern. Re-authorize its
        // immutable creation fingerprint and any bound Project before touching Goal
        // durable state; the Store receives only the validated canonical identity.
        if let Err(result) = self
            .authorize_session_target(&session_id, "prepare_goal_workflow", auth)
            .await
        {
            return result;
        }
        let principal = match goal_principal(auth) {
            Ok(principal) => principal,
            Err(result) => return result,
        };
        let Some(db) = self.communication_db.as_ref() else {
            return goal_store_unavailable();
        };
        match db.prepare_goal_workflow(&principal, &session_id, input) {
            Ok(result) => serialized_goal_success(result),
            Err(error) => goal_error(error, RecoveryKind::RetrySame),
        }
    }

    #[cfg(test)]
    pub(crate) fn create_goal(
        &self,
        auth: Option<&AuthContext>,
        title: String,
        objective: String,
        idempotency_key: String,
    ) -> ToolResult {
        self.create_goal_with_controller(auth, title, objective, None, idempotency_key)
    }

    #[cfg(test)]
    pub(crate) fn create_goal_with_controller(
        &self,
        auth: Option<&AuthContext>,
        title: String,
        objective: String,
        controller_agent_id: Option<String>,
        idempotency_key: String,
    ) -> ToolResult {
        self.create_goal_with_plan(
            auth,
            NewGoal {
                title,
                objective,
                controller_agent_id,
                idempotency_key,
                completion_conditions: Vec::new(),
                steps: Vec::new(),
            },
        )
    }

    pub(crate) fn create_goal_with_plan(
        &self,
        auth: Option<&AuthContext>,
        input: NewGoal,
    ) -> ToolResult {
        let principal = match goal_principal(auth) {
            Ok(principal) => principal,
            Err(result) => return result,
        };
        let Some(db) = self.communication_db.as_ref() else {
            return goal_store_unavailable();
        };
        match db.create_goal(&principal, input) {
            Ok(result) => serialized_goal_success(result),
            Err(error) => goal_error(error, RecoveryKind::RetrySame),
        }
    }

    pub(crate) fn checkpoint_goal(
        &self,
        auth: Option<&AuthContext>,
        goal_id: String,
        expected_revision: i64,
        checkpoint: GoalCheckpoint,
        idempotency_key: String,
    ) -> ToolResult {
        let principal = match goal_principal(auth) {
            Ok(principal) => principal,
            Err(result) => return result,
        };
        let Some(db) = self.communication_db.as_ref() else {
            return goal_store_unavailable();
        };
        match db.checkpoint_goal(
            &principal,
            &goal_id,
            expected_revision,
            checkpoint,
            &idempotency_key,
        ) {
            Ok(result) => serialized_goal_success(result),
            Err(error) => goal_error(error, RecoveryKind::RetrySame),
        }
    }

    pub(crate) fn get_goal(&self, auth: Option<&AuthContext>, goal_id: String) -> ToolResult {
        let principal = match goal_principal(auth) {
            Ok(principal) => principal,
            Err(result) => return result,
        };
        let Some(db) = self.communication_db.as_ref() else {
            return goal_store_unavailable();
        };
        match db.read_goal(&principal, &goal_id) {
            Ok(goal) => serialized_goal_success(json!({"goal": goal})),
            Err(error) => goal_error(error, RecoveryKind::Reobserve),
        }
    }

    async fn goal_activity_observation_at(
        &self,
        auth: Option<&AuthContext>,
        goal: &GoalDetail,
        now_ms: i64,
    ) -> GoalActivityObservation {
        let runtime_observation_available = auth
            .is_some_and(|auth| auth.has_scope(SCOPE_RUNTIME_READ))
            && self.window_activity_db.is_some();
        if goal.summary.lifecycle != GoalLifecycle::Active {
            return GoalActivityObservation::not_applicable(runtime_observation_available);
        }
        if !runtime_observation_available {
            return GoalActivityObservation::unavailable(GoalActivityState::Unobserved);
        }
        let Some(auth) = auth else {
            return GoalActivityObservation::unavailable(GoalActivityState::Unobserved);
        };
        let Ok((principal_kind, principal_id)) = super::runtime_observation_principal(Some(auth))
        else {
            return GoalActivityObservation::unavailable(GoalActivityState::Unobserved);
        };
        let principal = Some((principal_kind.as_str(), principal_id.as_str()));
        let Some(db) = self.window_activity_db.as_ref() else {
            return GoalActivityObservation::unavailable(GoalActivityState::Unobserved);
        };

        let mut correlations = goal
            .correlations
            .iter()
            .filter(|correlation| correlation.kind == GoalCorrelationKind::WorkflowSession)
            .collect::<Vec<_>>();
        correlations.sort_by(|a, b| {
            b.created_at_unix_ms
                .cmp(&a.created_at_unix_ms)
                .then_with(|| a.reference_id.cmp(&b.reference_id))
        });
        let mut coverage_partial = correlations.len() > GOAL_ACTIVITY_SESSION_SCAN_LIMIT;
        correlations.truncate(GOAL_ACTIVITY_SESSION_SCAN_LIMIT);

        let mut visibility_cache = HashMap::new();
        let mut candidate_windows = BTreeSet::new();
        for correlation in correlations {
            let Ok(Some(resolved_project)) = self
                .authorize_session_target(
                    &correlation.reference_id,
                    "goal_activity_observation",
                    Some(auth),
                )
                .await
            else {
                // Missing or revoked correlated Sessions may hide Window work.
                // Their older timestamps cannot make an incomplete scan complete.
                coverage_partial = true;
                continue;
            };
            if !visibility_cache.contains_key(&resolved_project.resolved_id)
                && visibility_cache.len() >= GOAL_ACTIVITY_PROJECT_VISIBILITY_LIMIT
            {
                coverage_partial = true;
                continue;
            }
            if !super::window_activity::window_project_visible_cached(
                self,
                auth,
                &mut visibility_cache,
                Some(&resolved_project.resolved_id),
            )
            .await
            {
                coverage_partial = true;
                continue;
            }
            let mut linked = match db.list_session_linked_windows(
                &correlation.reference_id,
                principal,
                GOAL_ACTIVITY_WINDOW_SCAN_LIMIT + 1,
            ) {
                Ok(linked) => linked,
                Err(_) => {
                    coverage_partial = true;
                    continue;
                }
            };
            if linked.len() > GOAL_ACTIVITY_WINDOW_SCAN_LIMIT {
                coverage_partial = true;
                linked.truncate(GOAL_ACTIVITY_WINDOW_SCAN_LIMIT);
            }
            for window in linked {
                if candidate_windows.len() >= GOAL_ACTIVITY_WINDOW_SCAN_LIMIT
                    && !candidate_windows.contains(&window.client_window_key)
                {
                    coverage_partial = true;
                    continue;
                }
                candidate_windows.insert(window.client_window_key);
            }
        }

        let linked_window_count = candidate_windows.len();
        let mut last_seen_at_ms = None;
        let mut last_meaningful_activity_at_ms = None;
        let mut active_meaningful_request_count = 0usize;
        coverage_partial |= self.window_activity.coverage_partial_for(principal);

        for window_key in candidate_windows {
            let events = match db.list_goal_window_activity_events(
                &window_key,
                principal,
                GOAL_ACTIVITY_EVENT_SCAN_LIMIT,
            ) {
                Ok(events) => events,
                Err(_) => {
                    coverage_partial = true;
                    continue;
                }
            };
            let meaningful_events: Vec<_> =
                events.iter().filter(|event| event.meaningful).collect();
            if meaningful_events.len() == GOAL_ACTIVITY_EVENT_SCAN_LIMIT
                && meaningful_events.last().is_some_and(|oldest_scanned| {
                    now_ms.saturating_sub(oldest_scanned.ended_at_ms)
                        <= GOAL_ACTIVITY_ATTENTION_AFTER_MS
                })
            {
                // Meaningful events are newest-completed-first. Transport polls
                // cannot evict the work anchor or turn complete evidence partial.
                coverage_partial = true;
            }
            for event in events {
                if !event_visibility_budget_available(&event, &visibility_cache) {
                    coverage_partial = true;
                    continue;
                }
                if !super::window_activity::window_event_visible_cached(
                    self,
                    auth,
                    &mut visibility_cache,
                    &event,
                )
                .await
                {
                    coverage_partial = true;
                    continue;
                }
                last_seen_at_ms = Some(last_seen_at_ms.unwrap_or(i64::MIN).max(event.ended_at_ms));
                if event.meaningful {
                    last_meaningful_activity_at_ms = Some(
                        last_meaningful_activity_at_ms
                            .unwrap_or(i64::MIN)
                            .max(event.ended_at_ms),
                    );
                }
            }
            for request in self.window_activity.list_for_window(&window_key, principal) {
                if !request_visibility_budget_available(&request, &visibility_cache) {
                    coverage_partial = true;
                    continue;
                }
                if !super::window_activity::active_window_request_visible_cached(
                    self,
                    auth,
                    &mut visibility_cache,
                    &request,
                )
                .await
                {
                    coverage_partial = true;
                    continue;
                }
                last_seen_at_ms = Some(
                    last_seen_at_ms
                        .unwrap_or(i64::MIN)
                        .max(request.started_at_ms),
                );
                if request.is_meaningful() {
                    active_meaningful_request_count =
                        active_meaningful_request_count.saturating_add(1);
                }
            }
        }

        let quiet_for_ms =
            last_meaningful_activity_at_ms.map(|last| now_ms.saturating_sub(last).max(0));
        let state = if active_meaningful_request_count > 0
            || quiet_for_ms.is_some_and(|quiet| quiet < GOAL_ACTIVITY_ATTENTION_AFTER_MS)
        {
            GoalActivityState::Active
        } else if coverage_partial {
            // Missing bounded evidence might contain a recent meaningful call or
            // a still-running request, so never manufacture an inactivity alert.
            GoalActivityState::Unobserved
        } else if quiet_for_ms.is_some() {
            GoalActivityState::AttentionNeeded
        } else {
            GoalActivityState::Unobserved
        };
        GoalActivityObservation {
            available: true,
            state,
            idle_threshold_ms: GOAL_ACTIVITY_ATTENTION_AFTER_MS,
            observation_lease_ms: crate::db::GOAL_CARD_OBSERVATION_LEASE_MS,
            last_seen_at_ms,
            last_meaningful_activity_at_ms,
            quiet_for_ms,
            linked_window_count: Some(linked_window_count),
            active_meaningful_request_count: Some(active_meaningful_request_count),
            coverage_partial,
        }
    }

    /// Sparse Goal follow-up for an independently authorized exact Workflow
    /// Session. This does not complete Goals or reuse Session authority as Goal
    /// authority. Inaccessible Goal metadata is never exposed through closeout.
    pub(crate) fn goal_follow_up_for_session(
        &self,
        auth: Option<&AuthContext>,
        session_id: &str,
    ) -> Option<serde_json::Value> {
        if !auth.is_some_and(|auth| auth.has_scope(SCOPE_COMMUNICATION_READ)) {
            return None;
        }
        let principal = goal_principal(auth).ok()?;
        let db = self.communication_db.as_ref()?;
        match db.active_goals_for_workflow_session(&principal, session_id, 8) {
            Ok((goals, truncated)) if !goals.is_empty() => Some(json!({
                "available": true,
                "truncated": truncated,
                "goals": goals.into_iter().map(|goal| {
                    let incomplete = goal.plan.steps.len() - goal.plan.completed_count();
                    json!({
                        "goal_id": goal.summary.goal_id,
                        "revision": goal.summary.revision,
                        "incomplete_step_count": incomplete,
                        "current_step": goal.plan.current_step().map(|step| json!({"id": step.id, "title": step.title})),
                        "next_action": if incomplete > 0 { "checkpoint_goal" } else { "update_goal" },
                    })
                }).collect::<Vec<_>>(),
            })),
            Ok(_) => None,
            Err(_) => Some(json!({"available": false, "truncated": false, "goals": []})),
        }
    }

    #[cfg(test)]
    pub(crate) async fn goal_plan_recheck_attention_at(
        &self,
        auth: Option<&AuthContext>,
        window: Option<&crate::client_window::ClientWindow>,
        goal_id: String,
        now: i64,
    ) -> ToolResult {
        self.goal_plan_recheck_attention_with_clock(auth, window, goal_id, || now)
            .await
    }

    async fn goal_plan_recheck_attention_with_clock(
        &self,
        auth: Option<&AuthContext>,
        window: Option<&crate::client_window::ClientWindow>,
        goal_id: String,
        clock: impl Fn() -> i64 + Send + Sync,
    ) -> ToolResult {
        let principal = match goal_principal(auth) {
            Ok(principal) => principal,
            Err(result) => return result,
        };
        let Some(db) = self.communication_db.as_ref() else {
            return goal_store_unavailable();
        };
        // Read through the normal owned Goal path before projecting any mapping.
        let goal = match db.read_goal(&principal, &goal_id) {
            Ok(goal) => goal,
            Err(error) => return goal_error(error, RecoveryKind::Reobserve),
        };
        let ineligible = || ToolResult::ok(json!({"attention": null, "state_changed": false}));
        if !auth.is_some_and(|auth| {
            [
                SCOPE_COMMUNICATION_READ,
                SCOPE_COMMUNICATION_MANAGE,
                SCOPE_RUNTIME_READ,
                SCOPE_SESSION_COLLABORATE,
                SCOPE_PROJECT_READ,
            ]
            .iter()
            .all(|scope| auth.has_scope(scope))
        }) || goal.summary.lifecycle != GoalLifecycle::Active
        {
            return ineligible();
        }
        let (Some(window), Some(controller_agent_id), Some(activity_db)) = (
            window,
            goal.controller_agent_id.as_ref(),
            self.window_activity_db.as_ref(),
        ) else {
            return ineligible();
        };
        // Atomic attention revalidation requires the existing shared action/Goal
        // database, not a second liveness store or a cross-database best effort.
        if !std::sync::Arc::ptr_eq(db, activity_db) {
            return ineligible();
        }
        let Some(activity_fence) = self.window_activity.meaningful_revision() else {
            return ineligible();
        };
        let activity = self
            .goal_activity_observation_at(auth, &goal, clock())
            .await;
        if !activity.available
            || activity.coverage_partial
            || activity.state != GoalActivityState::AttentionNeeded
            || activity.active_meaningful_request_count != Some(0)
        {
            return ineligible();
        }
        let Some(last_work) = activity.last_meaningful_activity_at_ms else {
            return ineligible();
        };
        let (observation_kind, observation_id) = match super::runtime_observation_principal(auth) {
            Ok(principal) => principal,
            Err(_) => return ineligible(),
        };
        let observation_principal = Some((observation_kind.as_str(), observation_id.as_str()));
        let relations =
            match activity_db.list_window_workflow_sessions(window.key(), observation_principal, 2)
            {
                Ok(relations) => relations,
                Err(_) => return ineligible(),
            };
        let Some(relation) = relations.first() else {
            return ineligible();
        };
        if relations
            .get(1)
            .is_some_and(|other| other.last_linked_at_ms >= relation.last_linked_at_ms)
            || !goal.correlations.iter().any(|correlation| {
                correlation.kind == GoalCorrelationKind::WorkflowSession
                    && correlation.reference_id == relation.workflow_session_id
            })
        {
            return ineligible();
        }
        let session_id = &relation.workflow_session_id;
        let Some((Some(project), owner_fingerprint)) =
            self.sessions.session_target_authority(session_id)
        else {
            return ineligible();
        };
        let resolved = match self
            .authorize_session_target(session_id, "goal_plan_sync", auth)
            .await
        {
            Ok(Some(resolved)) => resolved,
            _ => return ineligible(),
        };
        if resolved.resolved_id != project || relation.project.as_deref() != Some(project.as_str())
        {
            return ineligible();
        }
        let candidate = crate::db::GoalStallCandidate {
            goal_id,
            expected_revision: goal.summary.revision,
            controller_agent_id: controller_agent_id.clone(),
            workflow_session_id: session_id.clone(),
            project_id: project.clone(),
            observed_window_key: window.key().to_string(),
            observation_principal_kind: observation_kind.clone(),
            observation_principal_id: observation_id.clone(),
            last_meaningful_activity_at_ms: last_work,
        };
        // No awaits or transport dispatch under these fences. A meaningful call
        // that starts/finishes during visibility checks, or a closed/evicted
        // Session, invalidates the candidate before its durable transaction.
        let committed = self
            .window_activity
            .with_goal_stall_fence(
                activity_fence,
                (observation_kind.as_str(), observation_id.as_str()),
                || {
                    self.sessions.with_active_session_authority_fence(
                        session_id,
                        &project,
                        &owner_fingerprint,
                        || db.record_goal_workflow_stalled(&principal, &candidate, &clock),
                    )
                },
            )
            .flatten();
        match committed {
            Some(Ok(Some(attention))) => {
                if attention.created {
                    if let Some(controller) = self.agent_continuations.as_ref() {
                        controller.schedule_agent(controller_agent_id);
                    }
                }
                // A durable Wake is not proof of Host delivery or a resumed turn.
                serialized_goal_success(
                    json!({"state_changed": attention.created, "attention": attention}),
                )
            }
            Some(Err(error)) => target_authorization_error(error),
            Some(Ok(None)) | None => ineligible(),
        }
    }

    async fn exact_goal_plan_at(
        &self,
        auth: Option<&AuthContext>,
        goal_id: String,
        now_ms: i64,
    ) -> ToolResult {
        let principal = match goal_principal(auth) {
            Ok(principal) => principal,
            Err(result) => return result,
        };
        let Some(db) = self.communication_db.as_ref() else {
            return goal_store_unavailable();
        };
        match db.read_goal(&principal, &goal_id) {
            Ok(goal) => {
                let activity = self.goal_activity_observation_at(auth, &goal, now_ms).await;
                let continuity = goal_continuity_observation(self, &principal, &goal, &activity);
                serialized_goal_success(json!({
                    "goal_plan": goal_plan_projection(goal, activity, continuity),
                }))
            }
            Err(error) => goal_error(error, RecoveryKind::Reobserve),
        }
    }

    async fn exact_goal_plan(&self, auth: Option<&AuthContext>, goal_id: String) -> ToolResult {
        self.exact_goal_plan_at(auth, goal_id, chrono::Utc::now().timestamp_millis())
            .await
    }

    #[cfg(test)]
    pub(crate) async fn goal_plan_sync_at(
        &self,
        auth: Option<&AuthContext>,
        goal_id: String,
        now_ms: i64,
    ) -> ToolResult {
        self.exact_goal_plan_at(auth, goal_id, now_ms).await
    }

    pub(crate) async fn present_goal_plan(
        &self,
        auth: Option<&AuthContext>,
        goal_id: String,
    ) -> ToolResult {
        self.exact_goal_plan(auth, goal_id).await
    }

    pub(crate) async fn goal_plan_sync_for_window(
        &self,
        auth: Option<&AuthContext>,
        window: Option<&crate::client_window::ClientWindow>,
        goal_id: String,
    ) -> ToolResult {
        self.goal_plan_sync_for_window_at_inner(
            auth,
            window,
            goal_id,
            chrono::Utc::now().timestamp_millis(),
        )
        .await
    }

    #[cfg(test)]
    pub(crate) async fn goal_plan_sync_for_window_at(
        &self,
        auth: Option<&AuthContext>,
        window: Option<&crate::client_window::ClientWindow>,
        goal_id: String,
        now_ms: i64,
    ) -> ToolResult {
        self.goal_plan_sync_for_window_at_inner(auth, window, goal_id, now_ms)
            .await
    }

    async fn goal_plan_sync_for_window_at_inner(
        &self,
        auth: Option<&AuthContext>,
        window: Option<&crate::client_window::ClientWindow>,
        goal_id: String,
        now_ms: i64,
    ) -> ToolResult {
        let attention = self
            .goal_plan_recheck_attention_with_clock(auth, window, goal_id.clone(), || now_ms)
            .await;
        if !attention.success {
            return attention;
        }
        // One App RPC owns both authoritative stall reconciliation and the
        // post-reconciliation projection. The browser never supplies timing,
        // Session, Window, controller, Project, or revision authority.
        self.exact_goal_plan_at(auth, goal_id, now_ms).await
    }

    pub(crate) fn list_goals(
        &self,
        auth: Option<&AuthContext>,
        lifecycle: Option<String>,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> ToolResult {
        let principal = match goal_principal(auth) {
            Ok(principal) => principal,
            Err(result) => return result,
        };
        let lifecycle = match lifecycle
            .as_deref()
            .map(GoalLifecycle::from_input)
            .transpose()
        {
            Ok(lifecycle) => lifecycle,
            Err(error) => return goal_error(error, RecoveryKind::FixInput),
        };
        let limit = limit.unwrap_or(DEFAULT_GOAL_LIST_LIMIT);
        if limit == 0 || limit > MAX_GOAL_LIST_LIMIT {
            return goal_error(
                GoalStoreError::new(
                    "invalid_goal_list_limit",
                    format!("limit must be within 1..={MAX_GOAL_LIST_LIMIT}"),
                ),
                RecoveryKind::FixInput,
            );
        }
        let Some(db) = self.communication_db.as_ref() else {
            return goal_store_unavailable();
        };
        match db.list_goals(&principal, lifecycle, offset.unwrap_or(0), limit) {
            Ok(page) => serialized_goal_success(page),
            Err(error) => goal_error(error, RecoveryKind::Reobserve),
        }
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn update_goal(
        &self,
        auth: Option<&AuthContext>,
        goal_id: String,
        expected_revision: i64,
        title: Option<String>,
        objective: Option<String>,
        lifecycle: Option<String>,
        terminal_reason: Option<String>,
        idempotency_key: String,
    ) -> ToolResult {
        self.update_goal_with_controller(
            auth,
            goal_id,
            expected_revision,
            title,
            objective,
            None,
            lifecycle,
            terminal_reason,
            idempotency_key,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn update_goal_with_controller(
        &self,
        auth: Option<&AuthContext>,
        goal_id: String,
        expected_revision: i64,
        title: Option<String>,
        objective: Option<String>,
        controller_agent_id: Option<String>,
        lifecycle: Option<String>,
        terminal_reason: Option<String>,
        idempotency_key: String,
    ) -> ToolResult {
        let principal = match goal_principal(auth) {
            Ok(principal) => principal,
            Err(result) => return result,
        };
        let lifecycle = match lifecycle
            .as_deref()
            .map(GoalLifecycle::from_input)
            .transpose()
        {
            Ok(lifecycle) => lifecycle,
            Err(error) => return goal_error(error, RecoveryKind::FixInput),
        };
        let Some(db) = self.communication_db.as_ref() else {
            return goal_store_unavailable();
        };
        match db.update_goal(
            &principal,
            &goal_id,
            expected_revision,
            GoalPatch {
                title,
                objective,
                controller_agent_id,
                lifecycle,
                terminal_reason,
            },
            &idempotency_key,
        ) {
            Ok(result) => serialized_goal_success(result),
            Err(error) => goal_error(error, RecoveryKind::RetrySame),
        }
    }

    pub(crate) fn associate_goal_agent_task(
        &self,
        auth: Option<&AuthContext>,
        goal_id: String,
        task_id: String,
        idempotency_key: String,
    ) -> ToolResult {
        let principal = match goal_principal(auth) {
            Ok(principal) => principal,
            Err(result) => return result,
        };
        let Some(db) = self.communication_db.as_ref() else {
            return goal_store_unavailable();
        };
        // Authorize the Goal first so a foreign Goal cannot be used to probe target ids.
        if let Err(error) = db.read_goal(&principal, &goal_id) {
            return goal_error(error, RecoveryKind::Reobserve);
        }
        // Re-authorize the AgentTask in its own domain. The subsequent Goal record stores
        // only the exact identity; this check is never converted into inherited authority.
        if let Err(error) = db.read_agent_task(&principal, &task_id) {
            return target_authorization_error(error);
        }
        match db.associate_goal_agent_task(&principal, &goal_id, &task_id, &idempotency_key) {
            Ok(result) => serialized_goal_success(result),
            Err(error) => goal_error(error, RecoveryKind::RetrySame),
        }
    }

    pub(crate) async fn associate_goal_workflow_session(
        &self,
        auth: Option<&AuthContext>,
        goal_id: String,
        session_id: String,
        idempotency_key: String,
    ) -> ToolResult {
        let principal = match goal_principal(auth) {
            Ok(principal) => principal,
            Err(result) => return result,
        };
        let Some(db) = self.communication_db.as_ref() else {
            return goal_store_unavailable();
        };
        // Check Goal ownership before looking at the target Session to preserve existence hiding.
        if let Err(error) = db.read_goal(&principal, &goal_id) {
            return goal_error(error, RecoveryKind::Reobserve);
        }
        // This is the existing authoritative Session fence: it verifies the immutable
        // creation-time authority fingerprint and independently re-authorizes a bound Project.
        if let Err(result) = self
            .authorize_session_target(&session_id, "associate_goal_workflow_session", auth)
            .await
        {
            return result;
        }
        match db.associate_goal_workflow_session(
            &principal,
            &goal_id,
            &session_id,
            &idempotency_key,
        ) {
            Ok(result) => serialized_goal_success(result),
            Err(error) => goal_error(error, RecoveryKind::RetrySame),
        }
    }
}
