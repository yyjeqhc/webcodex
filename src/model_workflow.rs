//! Deployment guidance and Host interaction assumptions, not tool admission.
//!
//! Keep this policy out of ToolDefinition, schemas and Direct/gateway routing.
//! A new snapshot is consumed as ordinary context data, never as a tools/list
//! change. It cannot select identities, acknowledge messages or replay work.

use serde_json::{json, Value};

pub(crate) const GOAL_WORKFLOW_CONTEXT_KEY: &str = "webcodex.goal_workflow";
const GOAL_WORKFLOW_ENV: &str = "WEBCODEX_GOAL_WORKFLOW";
const MCP_APP_RESUME_ENV: &str = "WEBCODEX_MCP_APP_RESUME_MODE";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum GoalWorkflowPreference {
    #[default]
    OnDemand,
    Preferred,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum McpAppResumeMode {
    #[default]
    Unknown,
    UserConfirmed,
    /// Explicit operator declaration for a separately verified deployment.
    /// Never inferred from ui/message support, acceptance, or a consumed Wake.
    Unattended,
}

impl McpAppResumeMode {
    pub(crate) fn allows_unattended_claim(self) -> bool {
        self == Self::Unattended
    }

    pub(crate) fn guidance(self) -> &'static str {
        match self {
            Self::Unknown => "Host confirmation behavior is unknown. Do not promise unattended continuation. A message channel or dispatch acceptance does not prove a fresh turn; do not create Goal/Agent/Endpoint state just to test it. Preserve pending execution and use exact recovery when the user resumes.",
            Self::UserConfirmed => "Host follow-up requires user confirmation. Do not promise unattended continuation or bypass confirmation. Message dispatch acceptance is not a fresh turn. Preserve the existing Job/Wake and its exact recovery path rather than re-dispatching work or repeatedly sending a follow-up.",
            Self::Unattended => "Unattended MCP App follow-up is operator-declared for this deployment, not dynamically proven or guaranteed. Automatic continuation still needs exact authorized controller/Endpoint binding and current readiness; dispatch acceptance is not a fresh turn. Never bypass Host confirmation or retry an uncertain prior effect.",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ModelWorkflowPolicy {
    pub(crate) goal: GoalWorkflowPreference,
    pub(crate) mcp_app_resume: McpAppResumeMode,
}

impl ModelWorkflowPolicy {
    /// Read once at Server construction. No per-request environment mutation or
    /// hidden hot reload: deploying another snapshot does not refresh Host schemas
    /// or replace instructions already retained in an active model conversation.
    pub(crate) fn from_env() -> Result<Self, String> {
        fn read(key: &str) -> Result<Option<String>, String> {
            match std::env::var(key) {
                Ok(value) => Ok(Some(value)),
                Err(std::env::VarError::NotPresent) => Ok(None),
                Err(_) => Err(format!("{key} must be valid UTF-8")),
            }
        }
        Self::from_values(
            read(GOAL_WORKFLOW_ENV)?.as_deref(),
            read(MCP_APP_RESUME_ENV)?.as_deref(),
        )
    }

    pub(crate) fn from_values(goal: Option<&str>, resume: Option<&str>) -> Result<Self, String> {
        let goal = match goal.map(str::trim).unwrap_or("on_demand") {
            "on_demand" => GoalWorkflowPreference::OnDemand,
            "preferred" => GoalWorkflowPreference::Preferred,
            _ => {
                return Err(format!(
                    "{GOAL_WORKFLOW_ENV} must be on_demand or preferred"
                ))
            }
        };
        let mcp_app_resume = match resume.map(str::trim).unwrap_or("unknown") {
            "unknown" => McpAppResumeMode::Unknown,
            "user_confirmed" => McpAppResumeMode::UserConfirmed,
            "unattended" => McpAppResumeMode::Unattended,
            _ => {
                return Err(format!(
                    "{MCP_APP_RESUME_ENV} must be unknown, user_confirmed or unattended"
                ))
            }
        };
        Ok(Self {
            goal,
            mcp_app_resume,
        })
    }

    pub(crate) fn goal_selection_guidance(self) -> &'static str {
        match self.goal {
            GoalWorkflowPreference::OnDemand => "Goal workflow is on-demand. Ordinary implementation, review and multi-step work do not require a Goal. Use one for an explicit user-requested durable plan/handoff or an established exact Goal. On exact Session re-entry honor work_on_project.goal_context; choose explicitly among multiple candidates, never infer from Project/Window/title/recency, and available=false is inconclusive. For selected Goal work request _wc.context=[\"webcodex.goal_workflow\"]. Do not create a Goal, controller or card merely because its tool is available.",
            GoalWorkflowPreference::Preferred => "Operator preference: prefer a durable Goal for substantial multi-step/cross-turn work when useful; user intent wins and tiny work needs none. Reuse exact work_on_project.goal_context before creating another Goal; choose explicitly among multiple candidates, never infer from Project/Window/title/recency, and available=false is inconclusive. For selected Goal work request _wc.context=[\"webcodex.goal_workflow\"]. This preference neither proves unattended Host continuation nor authorizes controller/Endpoint setup.",
        }
    }

    pub(crate) fn goal_checkpoint_guidance(self) -> &'static str {
        "Only maintain an explicitly established Goal. No Goal means no checkpoint or Goal closeout. Load webcodex.goal_workflow for exact revision/key transitions; record facts already true, never pre-complete tests. An existing Goal remains recoverable regardless of the current default preference."
    }

    pub(crate) fn goal_workflow_projection(self) -> Value {
        json!({
            "authority": "model_guidance_only",
            "selection": self.goal_selection_guidance(),
            "host_interaction": self.mcp_app_resume.guidance(),
            "workflow": "For selected Goal work, reuse the exact active Goal in work_on_project.goal_context with get_goal/present_goal_plan; explicitly choose among multiple candidates. Never infer from Project/Window/title/recency; available=false never proves no Goal. Without a reusable Goal, call prepare_goal_workflow with the exact current Workflow Session, bounded completion_conditions/steps and optional explicit controller Agent, then present_goal_plan when a plan card is requested/useful. Low-level create_goal and associate_goal_workflow_session remain available. Host continuation setup/readiness remains separate.",
            "continuation": "Automatic continuation needs an exact explicit durable controller Agent and a separately verified Host carrier. Reuse the same Agent from explicit setup or exact Wake context; never infer Agent identity from a Window or create a second Goal-only identity. An Agent may be both Task assignee and Goal controller; Tasks/Attempts own execution. Goal Plan detects; the separate Agent Continuation card requests a turn. Stalled is not offline; dispatch acceptance is not resume. An exact stall Wake requires bootstrap, immediate consume, get_goal and exact Session handoff recovery; never retry an uncertain prior effect.",
            "checkpoint": "After a plan phase completes, prefer _wc.control.before.goal_progress on the next already-needed ordinary call with exact revision/key. Record facts already true; never pre-complete tests or defer all checkpoints to closeout. checkpoint_goal remains valid standalone. Complete all steps and verify/review before explicit update_goal or finish_coding_task + goal_completion; the Server cannot judge natural-language conditions.",
            "closeout": "Honor goal_follow_up only for explicitly correlated active Goals. Changing the default preference never completes/cancels a Goal, acknowledges a message, rotates an Endpoint, resends a Wake or retries a Job. Read the exact current state and preserve unresolved evidence before an explicit terminal transition.",
        })
    }
}
