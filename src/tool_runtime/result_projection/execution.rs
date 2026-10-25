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
    // A Session or an explicit setting may change execution semantics even
    // when the resolved output happens to look like an ordinary default.
    pub(super) default_execution_context: bool,
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
        ToolCall::ProjectFormat { .. }
        | ToolCall::ProjectBuild { .. }
        | ToolCall::RunProcess { .. }
        | ToolCall::RunDetachedProcess { .. }
        | ToolCall::RunSkillResource { .. }
        | ToolCall::RunScript { .. }
        | ToolCall::RunShell { .. }
        | ToolCall::CargoFmt { .. }
        | ToolCall::CargoCheck { .. }
        | ToolCall::ProjectValidate { .. }
        | ToolCall::GoTest { .. } => ValidationSuccessPolicy::default(),
        _ => return None,
    };
    let default_execution_context = match call {
        ToolCall::RunShell {
            session_id,
            cwd,
            shell,
            login,
            ..
        } => session_id.is_none() && cwd.is_none() && shell.is_none() && !*login,
        ToolCall::RunProcess {
            session_id, cwd, ..
        }
        | ToolCall::RunScript {
            session_id, cwd, ..
        } => session_id.is_none() && cwd.is_none(),
        _ => false,
    };
    Some(Box::new(ExecutionProjection {
        tool_name: call.tool_name(),
        validation_policy,
        default_execution_context,
    }))
}

impl ResultProjection for ExecutionProjection {
    fn project(self: Box<Self>, result: &mut ToolResult) {
        sparsify_structured_validation_success_evidence(
            self.tool_name,
            self.validation_policy,
            result,
        );
        let ordinary_terminal_success = if self.tool_name == "run_shell" {
            sparsify_terminal_shell_success(result)
        } else {
            sparsify_terminal_structured_execution_success(self.tool_name, result)
        };
        if self.default_execution_context && ordinary_terminal_success {
            // Request facts and actual resolved values must both prove the
            // context redundant. Never infer this from cwd/shell values alone.
            if let Some(output) = result.output.as_object_mut() {
                if output.get("cwd").and_then(serde_json::Value::as_str) == Some(".") {
                    output.remove("cwd");
                }
                if self.tool_name == "run_shell"
                    && output.get("shell").and_then(serde_json::Value::as_str) == Some("configured")
                {
                    output.remove("shell");
                }
            }
        }
        sparsify_structured_validation_runtime_metadata(self.tool_name, result);
        super::super::jobs::sparsify_job_handoff_model_result(result);
        if matches!(self.tool_name, "run_shell" | "run_process" | "run_script") {
            bound_execution_lines(result);
        }
    }
}

/// Bound display only, after audit/validation consumed the full retained result.
/// Never suggest replaying a command to recover omitted output.
fn bound_execution_lines(result: &mut ToolResult) {
    const MAX_LINES: usize = 200;
    let mut omitted = false;
    for stream in ["stdout", "stderr"] {
        let key = format!("{stream}_tail");
        let Some(original) = result.output.get(&key).and_then(serde_json::Value::as_str) else {
            continue;
        };
        let lines = original.split_inclusive('\n').collect::<Vec<_>>();
        if lines.len() <= MAX_LINES {
            continue;
        }
        let tail = lines[lines.len() - MAX_LINES..].concat();
        // The canonical failure template may still carry these exact tails.
        if let Some(error) = result.error.as_mut() {
            *error = error.replace(&format!("{key}:\n{original}"), &format!("{key}:\n{tail}"));
        }
        result.output[key] = serde_json::json!(tail);
        result.output[format!("{stream}_truncated")] = serde_json::json!(true);
        omitted = true;
    }
    if omitted {
        result.output["output_hint"] = serde_json::json!("Showing the last 200 lines per stream. For full future output, write a project file and use read_files.");
    }
}
