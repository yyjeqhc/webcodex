//! Execution-family capture and projection stay together. The captured Cargo
//! postconditions must survive argument normalization and canonical dispatch.

use super::registry::{CapturedProjection, ResultProjection};
use super::{
    sparsify_structured_validation_runtime_metadata,
    sparsify_structured_validation_success_evidence, sparsify_terminal_shell_success,
    sparsify_terminal_structured_execution_success, ToolCall, ToolResult, ValidationSuccessPolicy,
};

pub(super) struct ExecutionProjection {
    pub(super) tool_name: &'static str,
    pub(super) validation_policy: ValidationSuccessPolicy,
}

pub(super) fn capture(call: &ToolCall) -> Option<CapturedProjection> {
    let validation_policy = match call {
        ToolCall::CargoTest {
            require_tests,
            no_run,
            min_tests,
            ..
        } => ValidationSuccessPolicy {
            require_tests: *require_tests,
            no_run: *no_run,
            min_tests: *min_tests,
        },
        ToolCall::ProjectBuild { .. }
        | ToolCall::RunProcess { .. }
        | ToolCall::RunSkillResource { .. }
        | ToolCall::RunScript { .. }
        | ToolCall::RunShell { .. }
        | ToolCall::CargoFmt { .. }
        | ToolCall::CargoCheck { .. }
        | ToolCall::ProjectValidate { .. }
        | ToolCall::GoTest { .. } => ValidationSuccessPolicy::default(),
        _ => return None,
    };
    Some(Box::new(ExecutionProjection {
        tool_name: call.tool_name(),
        validation_policy,
    }))
}

impl ResultProjection for ExecutionProjection {
    fn project(self: Box<Self>, result: &mut ToolResult) {
        sparsify_structured_validation_success_evidence(
            self.tool_name,
            self.validation_policy,
            result,
        );
        if self.tool_name == "run_shell" {
            sparsify_terminal_shell_success(result);
        }
        sparsify_terminal_structured_execution_success(self.tool_name, result);
        sparsify_structured_validation_runtime_metadata(self.tool_name, result);
        super::super::jobs::sparsify_job_handoff_model_result(result);
    }
}
