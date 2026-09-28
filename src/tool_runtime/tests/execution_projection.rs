use super::*;
use serde_json::json;

fn project(name: &'static str, result: &mut ToolResult) {
    ModelFacingProjectionPlan {
        projection: ModelFacingProjection::Execution { tool_name: name },
    }.project(result);
}

#[test]
fn late_execution_projection_preserves_existing_shapes_and_canonical_consumers() {
    for name in ["run_process", "run_script", "run_skill_resource", "project_validate",
        "cargo_fmt", "cargo_check", "cargo_test", "go_test"] {
        let mut canonical = super::structured_execution_sparse_projection_tests::terminal_process_result(name);
        canonical.output["passed"] = json!(true);
        canonical.output["source_state"] = json!({"freshness":"unproven", "observed_mutation_fence":"uncrossed"});
        canonical.output["script_summary"] = json!("script summary");
        canonical.output["stdout_tail"] = json!("PRIVATE_OUTPUT");
        canonical.output["command"] = json!("PRIVATE_COMMAND");
        let audit = super::super::tool_audit::canonical_execution_audit_result_for_tool(name, &canonical.output);
        for key in ["command_started", "command_completed", "command_ok", "passed"].into_iter().filter(|_| name != "run_skill_resource") {
            assert_eq!(audit[key], true, "{name}: {key}");
        }
        assert_eq!(audit["execution_state"], "completed");
        if name != "run_skill_resource" {
            assert_eq!(audit["source_state"], canonical.output["source_state"]);
        }
        assert!(!audit.to_string().contains("PRIVATE"));
        let mut timer = super::super::model_ergonomics_telemetry::ModelErgonomicsTimer::start_with_arguments(name, &json!({})).unwrap();
        timer.capture_canonical_result(&canonical);
        let completion = timer.finish();
        let mut previous = ToolResult { success: canonical.success, output: canonical.output.clone(), error: canonical.error.clone() };
        sparsify_terminal_structured_execution_success(name, &mut previous);
        sparsify_structured_validation_runtime_metadata(name, &mut previous);
        super::super::jobs::sparsify_job_handoff_model_result(&mut previous);
        project(name, &mut canonical);
        assert_eq!(canonical.output, previous.output, "{name}");
        assert!(canonical.output.get("execution_state").is_none());
        let api = completion.record_for_tool_result(&canonical).unwrap();
        let wire = serde_json::to_value(&canonical).unwrap();
        let mcp = completion.record_for_structured_content(&wire).unwrap();
        assert_eq!(api, mcp);
        assert_eq!(api.execution_state.as_deref(), Some("completed"));
        assert_eq!(api.serialized_result_bytes, Some(serde_json::to_vec(&wire).unwrap().len() as u64));
    }
}

#[test]
fn late_execution_projection_preserves_failure_uncertainty_and_pending_shapes() {
    for name in ["run_process", "run_script", "run_skill_resource", "project_validate",
        "cargo_fmt", "cargo_check", "cargo_test", "go_test"] {
        for state in ["not_started", "outcome_unknown", "timed_out", "pending", "completed"] {
            let mut result = super::structured_execution_sparse_projection_tests::terminal_process_result(name);
            result.output["execution_state"] = json!(state);
            result.output["command_ok"] = json!(false);
            result.output["passed"] = json!(false);
            result.success = state == "pending";
            let mut previous = ToolResult { success: result.success, output: result.output.clone(), error: result.error.clone() };
            sparsify_terminal_structured_execution_success(name, &mut previous);
            sparsify_structured_validation_runtime_metadata(name, &mut previous);
            super::super::jobs::sparsify_job_handoff_model_result(&mut previous);
            project(name, &mut result);
            assert_eq!(serde_json::to_value(result).unwrap(), serde_json::to_value(previous).unwrap(), "{name}: {state}");
        }
    }
}
