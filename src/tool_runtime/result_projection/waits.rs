//! Wait projections only format already-produced outcomes; they never wait,
//! resume an Agent, consume events, or change Job lifecycle state.

use super::registry::{CapturedProjection, ResultProjection};
use super::{ToolCall, ToolResult};

struct AgentWaitProjection;
struct JobReadinessProjection;

pub(super) fn capture_agent_wait(call: &ToolCall) -> Option<CapturedProjection> {
    if matches!(
        call,
        ToolCall::WaitForAgentEvents { .. }
            | ToolCall::ReadAgentWait { .. }
            | ToolCall::CancelAgentWait { .. }
    ) {
        Some(Box::new(AgentWaitProjection))
    } else {
        None
    }
}

pub(super) fn capture_job_readiness(call: &ToolCall) -> Option<CapturedProjection> {
    if matches!(call, ToolCall::WaitForJobReadiness { .. }) {
        Some(Box::new(JobReadinessProjection))
    } else {
        None
    }
}

impl ResultProjection for AgentWaitProjection {
    fn project(self: Box<Self>, result: &mut ToolResult) {
        super::super::agent_wait::agent_wait_model_projection(result);
    }
}

impl ResultProjection for JobReadinessProjection {
    fn project(self: Box<Self>, result: &mut ToolResult) {
        super::super::observe_jobs::sparsify_job_readiness_model_result(result);
    }
}
