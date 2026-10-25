use super::*;
use serde_json::json;

fn project(name: &'static str, result: &mut ToolResult) {
    registry::ResultProjection::project(
        Box::new(execution::ExecutionProjection {
            tool_name: name,
            validation_policy: ValidationSuccessPolicy::default(),
        }),
        result,
    );
}

#[test]
fn late_execution_projection_preserves_existing_shapes_and_canonical_consumers() {
    for name in [
        "run_process",
        "run_script",
        "run_skill_resource",
        "project_validate",
        "cargo_fmt",
        "cargo_check",
        "cargo_test",
        "go_test",
    ] {
        let mut canonical =
            super::structured_execution_sparse_projection_tests::terminal_process_result(name);
        canonical.output["passed"] = json!(true);
        canonical.output["source_state"] =
            json!({"freshness":"unproven", "observed_mutation_fence":"uncrossed"});
        canonical.output["script_summary"] = json!("script summary");
        canonical.output["stdout_tail"] = json!("PRIVATE_OUTPUT");
        canonical.output["command"] = json!("PRIVATE_COMMAND");
        let audit = super::super::tool_audit::canonical_execution_audit_result_for_tool(
            name,
            &canonical.output,
        );
        for key in [
            "command_started",
            "command_completed",
            "command_ok",
            "passed",
        ]
        .into_iter()
        .filter(|_| name != "run_skill_resource")
        {
            assert_eq!(audit[key], true, "{name}: {key}");
        }
        assert_eq!(audit["execution_state"], "completed");
        if name != "run_skill_resource" {
            assert_eq!(audit["source_state"], canonical.output["source_state"]);
        }
        assert!(!audit.to_string().contains("PRIVATE"));
        let mut timer =
            super::super::model_ergonomics_telemetry::ModelErgonomicsTimer::start_with_arguments(
                name,
                &json!({}),
            )
            .unwrap();
        timer.capture_canonical_result(&canonical);
        let completion = timer.finish();
        let mut previous = ToolResult {
            success: canonical.success,
            output: canonical.output.clone(),
            error: canonical.error.clone(),
        };
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
        assert_eq!(
            api.serialized_result_bytes,
            Some(serde_json::to_vec(&wire).unwrap().len() as u64)
        );
    }
}

#[test]
fn late_execution_projection_preserves_failure_uncertainty_and_pending_shapes() {
    for name in [
        "run_process",
        "run_script",
        "run_skill_resource",
        "project_validate",
        "cargo_fmt",
        "cargo_check",
        "cargo_test",
        "go_test",
    ] {
        for state in [
            "not_started",
            "outcome_unknown",
            "timed_out",
            "pending",
            "completed",
        ] {
            let mut result =
                super::structured_execution_sparse_projection_tests::terminal_process_result(name);
            result.output["execution_state"] = json!(state);
            result.output["command_ok"] = json!(false);
            result.output["passed"] = json!(false);
            result.success = state == "pending";
            let mut previous = ToolResult {
                success: result.success,
                output: result.output.clone(),
                error: result.error.clone(),
            };
            sparsify_terminal_structured_execution_success(name, &mut previous);
            sparsify_structured_validation_runtime_metadata(name, &mut previous);
            super::super::jobs::sparsify_job_handoff_model_result(&mut previous);
            project(name, &mut result);
            assert_eq!(
                serde_json::to_value(result).unwrap(),
                serde_json::to_value(previous).unwrap(),
                "{name}: {state}"
            );
        }
    }
}

fn terminal_shell_result() -> ToolResult {
    let mut result =
        super::structured_execution_sparse_projection_tests::terminal_process_result("run_shell");
    result
        .output
        .as_object_mut()
        .unwrap()
        .remove("process_summary");
    result.output["command_summary"] = json!("printf witness");
    result.output["shell"] = json!("bash");
    result
}

fn assert_shell_schema(result: &ToolResult) {
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
        &serde_json::to_value(result).unwrap(),
        &crate::tool_runtime::registry::output_schema_for_tool("run_shell"),
    )
    .unwrap();
}

#[test]
fn shell_success_preserves_selected_context_output_loss_and_sidecars() {
    let mut result = terminal_shell_result();
    result.output["ssh_resource"] = json!("build-host");
    result.output["cwd"] = json!("/srv/build");
    result.output["shell"] = json!("ssh");
    result.output["stdout_tail"] = json!("output\n");
    result.output["stderr_tail"] = json!("warning\n");
    result.output["stdout_lines"] = json!(7);
    result.output["stderr_lines"] = json!(1);
    result.output["stdout_truncated"] = json!(true);
    result.output["async_handoff_available"] = json!(false);
    let before = result.output.clone();
    project("run_shell", &mut result);
    assert_shell_schema(&result);
    for key in [
        "cwd",
        "shell",
        "manage_ssh_resource",
        "stdout_tail",
        "stderr_tail",
        "stdout_lines",
        "stderr_lines",
        "stdout_truncated",
    ] {
        assert_eq!(result.output[key], before[key], "{key}");
    }
    assert!(result.output.get("execution_state").is_none());
    assert!(result.output.get("async_handoff_available").is_none());

    let mut result = terminal_shell_result();
    for key in [
        "expectation_satisfied",
        "input_normalization",
        "context_projection",
        "control",
        "session_attention",
        "workflow_recording_attention",
        "loss_evidence",
    ] {
        result.output[key] = json!({"witness":key});
    }
    let before = result.output.clone();
    project("run_shell", &mut result);
    for key in [
        "expectation_satisfied",
        "input_normalization",
        "context_projection",
        "control",
        "session_attention",
        "workflow_recording_attention",
        "loss_evidence",
    ] {
        assert_eq!(result.output[key], before[key], "{key}");
    }
}

#[test]
fn shell_compaction_requires_every_terminal_proof_and_keeps_exceptions_rich() {
    let changes = [
        ("execution_state", json!("outcome_unknown")),
        ("execution_state", json!("timed_out")),
        ("execution_state", json!("not_started")),
        ("execution_state", json!("pending")),
        ("execution_state", json!("queued")),
        ("execution_state", json!("running")),
        ("command_started", json!(false)),
        ("command_completed", json!(false)),
        ("command_ok", json!(false)),
        ("terminal", json!(false)),
        ("promoted_to_job", json!(true)),
        ("exit_code", json!(1)),
        ("tool_failure", json!(true)),
        ("failure_kind", json!("transport_failure")),
        ("requested_surface", json!("run_process")),
        ("job_id", json!("job-recovery")),
        ("job_status", json!("recovering")),
        ("observation_token", json!("token")),
        ("recovery_state", json!("reconciled")),
        ("recovery_reason_code", json!("ssh_connection_lost")),
        ("observation_error", json!("uncertain")),
        ("reconciled_at", json!(1)),
    ];
    for (key, value) in changes {
        let mut result = terminal_shell_result();
        result.output[key] = value;
        let before = result.output.clone();
        sparsify_terminal_shell_success(&mut result);
        assert_eq!(result.output, before, "{key}");
    }
    for key in [
        "execution_state",
        "command_started",
        "command_completed",
        "command_ok",
        "terminal",
        "promoted_to_job",
        "exit_code",
    ] {
        let mut result = terminal_shell_result();
        result.output.as_object_mut().unwrap().remove(key);
        let before = result.output.clone();
        project("run_shell", &mut result);
        assert_eq!(result.output, before, "missing {key}");
    }
    let mut result = terminal_shell_result();
    result.success = false;
    let before = result.output.clone();
    project("run_shell", &mut result);
    assert_eq!(result.output, before);
}

#[test]
fn shell_sparse_schema_rejects_lifecycle_fragments() {
    let mut result = terminal_shell_result();
    project("run_shell", &mut result);
    assert_shell_schema(&result);
    let schema = crate::tool_runtime::registry::output_schema_for_tool("run_shell");
    assert_eq!(result.output["shell"], "bash");
    assert!(
        result.output.get("cwd").is_some(),
        "retain actual resolved cwd"
    );
    assert!(result.output.get("command_summary").is_none());
    for (key, value) in [
        ("exit_code", json!(1)),
        ("duration_ms", json!(1)),
        ("executor", json!("agent")),
        ("command_started", json!(true)),
        ("command_completed", json!(true)),
        ("terminal", json!(true)),
        ("job_id", json!("job-impossible")),
        (
            "continuation",
            super::super::jobs::observe_job_continuation("job-impossible", None),
        ),
        ("recovery_state", json!("reconciled")),
        ("recovery_reason_code", json!("server_epoch_changed")),
        ("recovered_after_server_restart", json!(true)),
        ("reconciled_at", json!(1)),
    ] {
        let mut wire = serde_json::to_value(&result).unwrap();
        wire["output"][key] = value;
        assert!(
            crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&wire, &schema)
                .is_err(),
            "{key}"
        );
    }
}

#[test]
fn shell_pending_and_terminal_failures_keep_their_existing_model_contracts() {
    let mut pending = terminal_shell_result();
    pending.output["execution_state"] = json!("running");
    pending.output["command_completed"] = json!(false);
    pending.output["command_ok"] = json!(false);
    pending.output["promoted_to_job"] = json!(true);
    pending.output["terminal"] = json!(false);
    pending.output["job_id"] = json!("job-1");
    pending.output["job_status"] = json!("running");
    pending.output["observation_token"] = json!("token-1");
    let continuation = super::super::jobs::observe_job_continuation("job-1", Some("token-1"));
    pending.output["continuation"] = continuation.clone();
    project("run_shell", &mut pending);
    assert_eq!(
        pending.output,
        json!({"execution_state":"pending", "continuation":continuation})
    );
    assert_shell_schema(&pending);

    for (state, failure, exit_code) in [
        ("completed", "command_exit_nonzero", json!(1)),
        ("timed_out", "timeout", json!(null)),
        ("outcome_unknown", "outcome_unknown", json!(null)),
        ("not_started", "spawn_failed", json!(null)),
    ] {
        let mut result = terminal_shell_result();
        result.success = false;
        result.output["execution_state"] = json!(state);
        result.output["command_started"] = json!(state != "not_started");
        result.output["command_completed"] = json!(state == "completed");
        result.output["command_ok"] = json!(false);
        result.output["terminal"] = json!(state != "outcome_unknown");
        result.output["failure_kind"] = json!(failure);
        result.output["exit_code"] = exit_code;
        let before = serde_json::to_value(&result).unwrap();
        project("run_shell", &mut result);
        assert_eq!(serde_json::to_value(&result).unwrap(), before);
        assert_shell_schema(&result);
    }
}

#[test]
fn execution_tail_bound_keeps_failure_and_loss_evidence_without_duplicate_full_logs() {
    let stdout = (0..210)
        .map(|i| format!("stdout-{i:03}\n"))
        .collect::<String>();
    let stderr = (0..205)
        .map(|i| format!("stderr-{i:03}\n"))
        .collect::<String>();
    for name in ["run_shell", "run_process", "run_script"] {
        let mut result = ToolResult::err_with_output(
            crate::tool_runtime::helpers::command_failed_message(Some(9), &stdout, &stderr),
            json!({"execution_state":"completed","command_started":true,"command_completed":true,
                "failure_kind":"command_exit_nonzero","exit_code":9,"stdout_tail":stdout,"stderr_tail":stderr}),
        );
        project(name, &mut result);
        for stream in ["stdout", "stderr"] {
            assert_eq!(
                result.output[format!("{stream}_tail")]
                    .as_str()
                    .unwrap()
                    .lines()
                    .count(),
                200
            );
            assert_eq!(result.output[format!("{stream}_truncated")], true);
            assert!(!result
                .error
                .as_ref()
                .unwrap()
                .contains(&format!("{stream}-000")));
        }
        assert_eq!(result.output["exit_code"], 9);
        assert!(result
            .error
            .as_ref()
            .unwrap()
            .contains("Command exited with status 9"));
    }
}

#[test]
fn ordinary_shell_failure_omits_timing_but_keeps_outcome_and_schema() {
    let mut result = terminal_shell_result();
    result.success = false;
    result.error = Some("Command exited with status 9.".into());
    result.output["exit_code"] = json!(9);
    result.output["command_ok"] = json!(false);
    result.output["failure_kind"] = json!("command_exit_nonzero");
    let canonical = result.output.clone();
    sparsify_failure_model_result_metadata("run_shell", &mut result);
    for omitted in [
        "async_handoff_available",
        "effective_timeout_secs",
        "sync_wait_secs",
        "command_summary",
        "terminal",
    ] {
        assert!(result.output.get(omitted).is_none(), "{omitted}");
    }
    for retained in [
        "execution_state",
        "command_started",
        "command_completed",
        "exit_code",
        "failure_kind",
    ] {
        assert_eq!(result.output[retained], canonical[retained], "{retained}");
    }
    assert_shell_schema(&result);
}
