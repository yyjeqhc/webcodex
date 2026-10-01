#[derive(Debug, Clone)]
pub struct NewAgentTask {
    pub title: String,
    pub instruction: String,
    pub assignee_agent_id: Option<String>,
    pub source_conversation_id: Option<String>,
    pub source_message_id: Option<String>,
    pub referenced_project_id: Option<String>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentTaskState {
    Ready,
    Active,
    Succeeded,
    Failed,
}

impl AgentTaskState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Active => "active",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }

    pub(crate) fn from_db(value: &str, index: usize) -> rusqlite::Result<Self> {
        match value {
            "ready" => Ok(Self::Ready),
            "active" => Ok(Self::Active),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                index,
                Type::Text,
                format!("unsupported AgentTask state: {other}").into(),
            )),
        }
    }

    pub(crate) const fn terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed)
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentTaskAttemptState {
    Active,
    Expired,
    Succeeded,
    Failed,
}

impl AgentTaskAttemptState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Expired => "expired",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }

    fn from_db(value: &str, index: usize) -> rusqlite::Result<Self> {
        match value {
            "active" => Ok(Self::Active),
            "expired" => Ok(Self::Expired),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                index,
                Type::Text,
                format!("unsupported AgentTaskAttempt state: {other}").into(),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentTaskCodingRunDispatchState {
    Prepared,
    NotStarted,
    OutcomeUnknown,
    Bound,
    Terminal,
}

impl AgentTaskCodingRunDispatchState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::NotStarted => "not_started",
            Self::OutcomeUnknown => "outcome_unknown",
            Self::Bound => "bound",
            Self::Terminal => "terminal",
        }
    }

    fn from_db(value: &str, index: usize) -> rusqlite::Result<Self> {
        match value {
            "prepared" => Ok(Self::Prepared),
            "not_started" => Ok(Self::NotStarted),
            "outcome_unknown" => Ok(Self::OutcomeUnknown),
            "bound" => Ok(Self::Bound),
            "terminal" => Ok(Self::Terminal),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                index,
                Type::Text,
                format!("unsupported AgentTask CodingAgent dispatch state: {other}").into(),
            )),
        }
    }

    const fn blocks_replacement(self) -> bool {
        matches!(self, Self::OutcomeUnknown | Self::Bound)
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentTaskExecutionKind {
    CodingAgentRun,
    AgentEndpoint,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentTaskExecutionStatus {
    NotStarted,
    Active,
    WaitingPermission,
    OutcomeUnknown,
    Terminal,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentTaskExecutionRecoveryKind {
    None,
    Observe,
    Reconcile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTaskCodingRunBindingIntent {
    pub run_id: String,
    pub runtime_project_id: String,
    pub provider_id: String,
    pub provider_instance_id: String,
    pub authority_fingerprint: String,
    pub coding_agent_intent_fingerprint: String,
    pub binding_intent_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTaskCodingRunObservation {
    pub run_id: String,
    pub runtime_project_id: String,
    pub provider_id: String,
    pub provider_instance_id: String,
    pub authority_fingerprint: String,
    pub coding_agent_intent_fingerprint: String,
    pub run_state: CodingAgentRunState,
    pub execution_state: CodingAgentExecutionState,
    pub observation_revision: i64,
    pub terminal_stop_reason: Option<String>,
    pub terminal_error_code: Option<String>,
    pub terminal_message: Option<String>,
    pub completed_at_unix: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTaskCodingRunBindingRecord {
    pub task_id: String,
    pub attempt_id: String,
    pub run_id: String,
    pub runtime_project_id: String,
    pub provider_id: String,
    pub provider_instance_id: String,
    pub authority_fingerprint: String,
    pub coding_agent_intent_fingerprint: String,
    pub binding_intent_fingerprint: String,
    pub dispatch_state: AgentTaskCodingRunDispatchState,
    pub last_observed_run_state: Option<CodingAgentRunState>,
    pub last_observed_execution_state: Option<CodingAgentExecutionState>,
    pub last_observation_revision: Option<i64>,
    pub terminal_stop_reason: Option<String>,
    pub terminal_error_code: Option<String>,
    pub terminal_message: Option<String>,
    pub completed_at_unix: Option<i64>,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
    pub terminal_at_unix_ms: Option<i64>,
}

impl AgentTaskCodingRunBindingRecord {
    fn execution_status(&self) -> AgentTaskExecutionStatus {
        match self.dispatch_state {
            AgentTaskCodingRunDispatchState::Prepared
            | AgentTaskCodingRunDispatchState::NotStarted => AgentTaskExecutionStatus::NotStarted,
            AgentTaskCodingRunDispatchState::OutcomeUnknown => {
                AgentTaskExecutionStatus::OutcomeUnknown
            }
            AgentTaskCodingRunDispatchState::Terminal => AgentTaskExecutionStatus::Terminal,
            AgentTaskCodingRunDispatchState::Bound => match self.last_observed_run_state.as_ref() {
                Some(CodingAgentRunState::WaitingPermission) => {
                    AgentTaskExecutionStatus::WaitingPermission
                }
                Some(CodingAgentRunState::Lost) => AgentTaskExecutionStatus::OutcomeUnknown,
                Some(
                    CodingAgentRunState::Completed
                    | CodingAgentRunState::Failed
                    | CodingAgentRunState::Cancelled,
                ) => AgentTaskExecutionStatus::Terminal,
                _ => AgentTaskExecutionStatus::Active,
            },
        }
    }

    fn recovery_kind(&self) -> AgentTaskExecutionRecoveryKind {
        match self.dispatch_state {
            AgentTaskCodingRunDispatchState::Prepared
            | AgentTaskCodingRunDispatchState::NotStarted
            | AgentTaskCodingRunDispatchState::Terminal => AgentTaskExecutionRecoveryKind::None,
            AgentTaskCodingRunDispatchState::OutcomeUnknown => {
                AgentTaskExecutionRecoveryKind::Reconcile
            }
            AgentTaskCodingRunDispatchState::Bound => match self.execution_status() {
                AgentTaskExecutionStatus::Active | AgentTaskExecutionStatus::WaitingPermission => {
                    AgentTaskExecutionRecoveryKind::Observe
                }
                AgentTaskExecutionStatus::OutcomeUnknown | AgentTaskExecutionStatus::Terminal => {
                    AgentTaskExecutionRecoveryKind::Reconcile
                }
                AgentTaskExecutionStatus::NotStarted => AgentTaskExecutionRecoveryKind::None,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTaskCodingRunStartContext {
    pub task: AgentTaskDetail,
    pub attempt: AgentTaskAttemptRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTaskCodingRunPrepared {
    pub binding: AgentTaskCodingRunBindingRecord,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTaskCodingRunDispatchClaim {
    pub binding: AgentTaskCodingRunBindingRecord,
    pub may_dispatch: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentTaskEndpointExecutionRecord {
    pub task_id: String,
    pub attempt_id: String,
    pub wake_id: String,
    pub wake_state: AgentWakeState,
    pub endpoint_id: Option<String>,
    pub endpoint_controller_generation: Option<i64>,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentTaskEndpointExecutionMutation {
    pub execution: AgentTaskEndpointExecutionRecord,
    pub replayed: bool,
    pub state_changed: bool,
}

impl AgentTaskEndpointExecutionRecord {
    fn execution_status(&self) -> AgentTaskExecutionStatus {
        match self.wake_state {
            AgentWakeState::Pending | AgentWakeState::Claimed => {
                AgentTaskExecutionStatus::NotStarted
            }
            AgentWakeState::Prepared | AgentWakeState::Delivered | AgentWakeState::Consumed => {
                AgentTaskExecutionStatus::Active
            }
            AgentWakeState::DeliveryUnknown => AgentTaskExecutionStatus::OutcomeUnknown,
            AgentWakeState::Retired => AgentTaskExecutionStatus::Terminal,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTaskCodingRunReconcileMutation {
    pub task: AgentTaskSummary,
    pub attempt: AgentTaskAttemptRecord,
    pub binding: AgentTaskCodingRunBindingRecord,
    pub state_changed: bool,
    pub attention_event_count: usize,
    pub attention_target_agent_ids: Vec<String>,
    pub wait_target_agent_ids: Vec<String>,
}

/// Internal pin for issuing `attempt_ref`. The fence never leaves the store API
/// through a model-facing Task summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveAgentTaskAttemptPin {
    pub attempt_id: String,
    pub assignee_agent_id: String,
    pub attempt_fence: String,
    pub attempt_controller_generation: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentTaskAttemptRecord {
    pub attempt_id: String,
    pub task_id: String,
    pub attempt_number: i64,
    pub assignee_agent_id: String,
    pub state: AgentTaskAttemptState,
    pub lease_expires_at_unix_ms: i64,
    pub lease_active: bool,
    pub attempt_controller_generation: i64,
    pub created_at_unix_ms: i64,
    pub started_at_unix_ms: i64,
    pub terminal_at_unix_ms: Option<i64>,
    pub terminal_result: Option<String>,
    pub terminal_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentTaskSummary {
    pub task_id: String,
    pub assignee_agent_id: Option<String>,
    pub title: String,
    pub source_conversation_id: Option<String>,
    pub source_message_id: Option<String>,
    pub referenced_project_id: Option<String>,
    pub state: AgentTaskState,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
    pub terminal_at_unix_ms: Option<i64>,
    pub latest_attempt: Option<AgentTaskAttemptRecord>,
    pub execution_bound: bool,
    pub execution_kind: Option<AgentTaskExecutionKind>,
    pub execution_status: Option<AgentTaskExecutionStatus>,
    pub recovery_kind: AgentTaskExecutionRecoveryKind,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentTaskDetail {
    pub summary: AgentTaskSummary,
    pub instruction: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentTaskMutation {
    pub task: AgentTaskDetail,
    pub created: bool,
    pub replayed: bool,
    pub state_changed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentTaskPage {
    pub total_count: i64,
    pub offset: usize,
    pub next_offset: Option<usize>,
    pub truncated: bool,
    pub tasks: Vec<AgentTaskSummary>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentTaskAttemptStartMutation {
    pub task: AgentTaskSummary,
    pub attempt: AgentTaskAttemptRecord,
    pub attempt_fence: String,
    pub replayed: bool,
    pub state_changed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentTaskAttemptHeartbeatMutation {
    pub task: AgentTaskSummary,
    pub attempt: AgentTaskAttemptRecord,
    pub state_changed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentTaskAttemptCompletionMutation {
    pub task: AgentTaskSummary,
    pub attempt: AgentTaskAttemptRecord,
    pub replayed: bool,
    pub state_changed: bool,
    #[serde(skip_serializing)]
    pub attention_event_count: usize,
    #[serde(skip_serializing)]
    pub attention_target_agent_ids: Vec<String>,
    #[serde(skip_serializing)]
    pub wait_target_agent_ids: Vec<String>,
}

#[derive(Debug, Clone)]
struct StoredAttempt {
    attempt_id: String,
    task_id: String,
    attempt_number: i64,
    assignee_agent_id: String,
    stored_state: AgentTaskAttemptState,
    lease_expires_at_unix_ms: i64,
    attempt_fence: String,
    attempt_controller_generation: i64,
    created_at_unix_ms: i64,
    started_at_unix_ms: i64,
    terminal_at_unix_ms: Option<i64>,
    terminal_result: Option<String>,
    terminal_reason: Option<String>,
}

impl StoredAttempt {
    fn effective_state(&self, now: i64) -> AgentTaskAttemptState {
        if self.stored_state == AgentTaskAttemptState::Active
            && self.lease_expires_at_unix_ms <= now
        {
            AgentTaskAttemptState::Expired
        } else {
            self.stored_state
        }
    }

    fn record(&self, now: i64) -> AgentTaskAttemptRecord {
        let state = self.effective_state(now);
        AgentTaskAttemptRecord {
            attempt_id: self.attempt_id.clone(),
            task_id: self.task_id.clone(),
            attempt_number: self.attempt_number,
            assignee_agent_id: self.assignee_agent_id.clone(),
            state,
            lease_expires_at_unix_ms: self.lease_expires_at_unix_ms,
            lease_active: state == AgentTaskAttemptState::Active,
            attempt_controller_generation: self.attempt_controller_generation,
            created_at_unix_ms: self.created_at_unix_ms,
            started_at_unix_ms: self.started_at_unix_ms,
            terminal_at_unix_ms: self.terminal_at_unix_ms,
            terminal_result: self.terminal_result.clone(),
            terminal_reason: self.terminal_reason.clone(),
        }
    }
}

#[derive(Debug, Clone)]
struct StoredTask {
    task_id: String,
    assignee_agent_id: Option<String>,
    title: String,
    instruction: String,
    source_conversation_id: Option<String>,
    source_message_id: Option<String>,
    referenced_project_id: Option<String>,
    stored_state: AgentTaskState,
    created_at_unix_ms: i64,
    updated_at_unix_ms: i64,
    terminal_at_unix_ms: Option<i64>,
    latest_attempt: Option<StoredAttempt>,
    latest_coding_run: Option<AgentTaskCodingRunBindingRecord>,
    latest_endpoint_execution: Option<AgentTaskEndpointExecutionRecord>,
}

impl StoredTask {
    fn effective_state(&self, now: i64) -> AgentTaskState {
        if self.stored_state == AgentTaskState::Active
            && self.latest_attempt.as_ref().is_some_and(|attempt| {
                attempt.effective_state(now) == AgentTaskAttemptState::Expired
            })
            && !self
                .latest_coding_run
                .as_ref()
                .is_some_and(|binding| binding.dispatch_state.blocks_replacement())
        {
            AgentTaskState::Ready
        } else {
            self.stored_state
        }
    }

    fn summary(&self, now: i64) -> AgentTaskSummary {
        let endpoint_execution_status = self.latest_endpoint_execution.as_ref().map(|execution| {
            let attempt_terminal = self.latest_attempt.as_ref().is_some_and(|attempt| {
                matches!(
                    attempt.effective_state(now),
                    AgentTaskAttemptState::Succeeded
                        | AgentTaskAttemptState::Failed
                        | AgentTaskAttemptState::Expired
                )
            });
            if attempt_terminal {
                AgentTaskExecutionStatus::Terminal
            } else {
                execution.execution_status()
            }
        });
        AgentTaskSummary {
            task_id: self.task_id.clone(),
            assignee_agent_id: self.assignee_agent_id.clone(),
            title: self.title.clone(),
            source_conversation_id: self.source_conversation_id.clone(),
            source_message_id: self.source_message_id.clone(),
            referenced_project_id: self.referenced_project_id.clone(),
            state: self.effective_state(now),
            created_at_unix_ms: self.created_at_unix_ms,
            updated_at_unix_ms: self.updated_at_unix_ms,
            terminal_at_unix_ms: self.terminal_at_unix_ms,
            latest_attempt: self
                .latest_attempt
                .as_ref()
                .map(|attempt| attempt.record(now)),
            execution_bound: self.latest_coding_run.is_some()
                || self.latest_endpoint_execution.is_some(),
            execution_kind: if self.latest_coding_run.is_some() {
                Some(AgentTaskExecutionKind::CodingAgentRun)
            } else if self.latest_endpoint_execution.is_some() {
                Some(AgentTaskExecutionKind::AgentEndpoint)
            } else {
                None
            },
            execution_status: self
                .latest_coding_run
                .as_ref()
                .map(AgentTaskCodingRunBindingRecord::execution_status)
                .or(endpoint_execution_status),
            recovery_kind: self
                .latest_coding_run
                .as_ref()
                .map(AgentTaskCodingRunBindingRecord::recovery_kind)
                .unwrap_or(AgentTaskExecutionRecoveryKind::None),
        }
    }

    fn detail(&self, now: i64) -> AgentTaskDetail {
        AgentTaskDetail {
            summary: self.summary(now),
            instruction: self.instruction.clone(),
        }
    }
}

