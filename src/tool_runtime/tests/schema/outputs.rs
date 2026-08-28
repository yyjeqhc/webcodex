use super::*;

struct TemporaryDefaultOnlyOutputSchemaGap {
    name: &'static str,
    reason: &'static str,
    exit_condition: &'static str,
}

// TODO(tool-definition): remove entries as these tools gain explicit output
// schema fields, or move the allowlist to a generated definition-backed
// declaration once output_schema is part of ToolDefinition.
const TEMPORARY_MODEL_VISIBLE_TOOLS_WITH_DEFAULT_ONLY_OUTPUT_SCHEMA_GAPS:
    &[TemporaryDefaultOnlyOutputSchemaGap] = &[];

fn structured_execution_output(
    execution_source: &str,
    execution_state: &str,
    command_started: bool,
    command_completed: bool,
    promoted_to_job: bool,
    terminal: bool,
    job_id: Option<&str>,
    job_status: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "success": promoted_to_job,
        "output": {
            "execution_source": execution_source,
            "execution_state": execution_state,
            "command_started": command_started,
            "command_completed": command_completed,
            "promoted_to_job": promoted_to_job,
            "terminal": terminal,
            "job_id": job_id,
            "job_status": job_status,
            "observation_token": if promoted_to_job { Some("observation") } else { None },
            "effective_timeout_secs": 60,
            "sync_wait_secs": 10,
            "async_handoff_available": true
        },
        "error": null
    })
}

#[test]
fn computer_launch_application_output_schema_has_closed_native_platforms() {
    let schema =
        crate::tool_runtime::registry::output_schema_for_tool("computer_launch_application");
    let validate = |value: &Value| {
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(value, &schema)
    };
    let application_id = "application_0123456789abcdef0123456789abcdef";
    for platform in ["windows", "macos"] {
        let output =
            serde_json::to_value(crate::tool_runtime::tool_result::ToolResult::ok(json!({
                "platform": platform,
                "application_id": application_id,
                "success": true,
            })))
            .unwrap();
        validate(&output).unwrap_or_else(|error| panic!("{platform}: {error}"));
    }

    let stale = serde_json::to_value(
        crate::tool_runtime::tool_result::ToolResult::err_with_output(
            "stale application",
            json!({
                "error_kind": "stale_application",
                "message": "application identity is stale",
                "application_id": application_id,
                "state_changed": false,
                "execution_state": "not_started",
                "recovery_kind": "reobserve",
                "recovery_tool": "computer_list_applications"
            }),
        ),
    )
    .unwrap();
    validate(&stale).unwrap();
    let mut invalid_recovery = stale.clone();
    invalid_recovery["output"]["recovery_kind"] = json!("blind_retry");
    assert!(validate(&invalid_recovery).is_err());
    let mut invalid_tool_class = stale.clone();
    invalid_tool_class["output"]["recovery_kind"] = json!("fix_input");
    assert!(validate(&invalid_tool_class).is_err());

    let mut recovery_on_success =
        serde_json::to_value(crate::tool_runtime::tool_result::ToolResult::ok(json!({
            "platform": "macos",
            "application_id": application_id,
            "success": true,
        })))
        .unwrap();
    recovery_on_success["output"]["recovery_kind"] = json!("none");
    assert!(validate(&recovery_on_success).is_err());

    let unsupported =
        serde_json::to_value(crate::tool_runtime::tool_result::ToolResult::ok(json!({
            "platform": "linux",
            "application_id": application_id,
            "success": true,
        })))
        .unwrap();
    assert!(validate(&unsupported).is_err());

    let extra = serde_json::to_value(crate::tool_runtime::tool_result::ToolResult::ok(json!({
        "platform": "macos",
        "application_id": application_id,
        "success": true,
        "bundle_url": "PRIVATE",
    })))
    .unwrap();
    assert!(validate(&extra).is_err());
}

#[test]
fn observe_jobs_failure_item_schema_closes_recovery_metadata() {
    let schema = crate::tool_runtime::registry::output_schema_for_tool("observe_jobs");
    let validate = |value: &Value| {
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(value, &schema)
    };
    let result = json!({
        "success": true,
        "output": {
            "requested_count": 1,
            "returned_count": 1,
            "succeeded_count": 0,
            "failed_count": 1,
            "items": [{
                "index": 0,
                "job_id": "job-missing",
                "success": false,
                "output": null,
                "error_kind": "unknown_job",
                "recovery_kind": "reobserve",
                "recovery_tool": "list_jobs",
                "error": "unknown job"
            }],
            "wake_reason": "item_error",
            "waited_ms": 0,
            "changed_count": 0,
            "terminal_count": 0,
            "output_truncated": false,
            "next_index": null
        },
        "error": null
    });
    validate(&result).unwrap();

    let mut invalid_kind = result.clone();
    invalid_kind["output"]["items"][0]["recovery_kind"] = json!("blind_retry");
    assert!(validate(&invalid_kind).is_err());

    let mut invalid_tool = result;
    invalid_tool["output"]["items"][0]["recovery_tool"] = json!("computer_list_windows");
    assert!(validate(&invalid_tool).is_err());
}

#[test]
fn model_visible_tool_definitions_have_output_schema_coverage_or_allowance() {
    let specs = registered_tool_specs();
    let default_fields = default_output_schema_field_names();
    let default_schema_names = specs
        .iter()
        .filter(|spec| output_schema_field_names(spec) == default_fields)
        .map(|spec| spec.name.as_str())
        .collect::<Vec<_>>();
    let allowed_names = TEMPORARY_MODEL_VISIBLE_TOOLS_WITH_DEFAULT_ONLY_OUTPUT_SCHEMA_GAPS
        .iter()
        .map(|gap| {
            assert!(
                specs.iter().any(|spec| spec.name == gap.name),
                "{} default output schema gap must refer to a public ToolSpec",
                gap.name
            );
            assert!(
                !gap.reason.trim().is_empty(),
                "{} default output schema allowance must explain the drift risk",
                gap.name
            );
            assert!(
                !gap.exit_condition.trim().is_empty(),
                "{} default output schema allowance must explain how to remove it",
                gap.name
            );
            gap.name
        })
        .collect::<Vec<_>>();

    assert_eq!(
        specs.len() - default_schema_names.len(),
        specs.len(),
        "explicit model-visible output schema coverage"
    );
    assert_eq!(
        default_schema_names.len(),
        0,
        "temporary default-only output schema gap count"
    );
    assert_eq!(
        default_schema_names, allowed_names,
        "model-visible tools may use the default output schema only with an explicit allowance"
    );
}

#[test]
fn key_tool_output_schemas_include_expected_fields() {
    let specs = registered_tool_specs();
    let has_output_field = |name: &str, field: &str| {
        let spec = spec_named(&specs, name);
        spec.output_schema["properties"]["output"]["properties"]
            .as_object()
            .is_some_and(|props| props.contains_key(field))
    };

    for field in [
        "session_id",
        "project",
        "title",
        "execution_context",
        "previous_execution_context",
        "changed",
        "created_at",
        "updated_at",
    ] {
        assert!(
            has_output_field("update_session_context", field),
            "update_session_context missing {field}"
        );
    }

    for field in [
        "duration_ms",
        "exit_code",
        "stdout_tail",
        "stderr_tail",
        "stdout_lines",
        "stderr_lines",
        "stdout_truncated",
        "stderr_truncated",
        "command_started",
        "command_completed",
        "command_ok",
        "failure_kind",
        "tool_failure",
        "purpose",
        "process_summary",
        "cwd",
        "executor",
        "execution_source",
        "execution_state",
        "promoted_to_job",
        "terminal",
        "job_id",
        "job_status",
        "observation_token",
        "effective_timeout_secs",
        "sync_wait_secs",
        "async_handoff_available",
    ] {
        assert!(
            has_output_field("run_process", field),
            "run_process missing {field}"
        );
    }
    assert_eq!(
        output_schema_property(&specs, "run_process", "execution_state")["enum"],
        serde_json::json!([
            "not_started",
            "outcome_unknown",
            "completed",
            "timed_out",
            "queued",
            "running"
        ])
    );
    let run_process_schema = &spec_named(&specs, "run_process").output_schema;
    for (state, command_started, command_completed) in [
        ("not_started", false, false),
        ("outcome_unknown", true, false),
        ("completed", true, true),
        ("timed_out", true, false),
    ] {
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
            &serde_json::json!({
                "success": false,
                "output": {
                    "execution_state": state,
                    "command_started": command_started,
                    "command_completed": command_completed
                },
                "error": null
            }),
            run_process_schema,
        )
        .unwrap_or_else(|error| panic!("run_process state {state} should validate: {error}"));
    }
    for (state, command_started, job_status) in [
        ("queued", false, "agent_queued"),
        ("running", true, "running"),
    ] {
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
            &structured_execution_output(
                "run_process",
                state,
                command_started,
                false,
                true,
                false,
                Some("job-1"),
                Some(job_status),
            ),
            run_process_schema,
        )
        .unwrap_or_else(|error| {
            panic!("run_process handoff state {state} should validate: {error}")
        });
    }
    assert!(
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
            &serde_json::json!({
                "success": false,
                "output": {
                    "execution_state": "started",
                    "command_started": true,
                    "command_completed": false
                },
                "error": null
            }),
            run_process_schema,
        )
        .is_err(),
        "run_process must reject lifecycle states outside the terminal and handoff contract"
    );
    assert!(
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
            &serde_json::json!({
                "success": false,
                "output": {
                    "execution_state": "not_started",
                    "command_started": true,
                    "command_completed": false
                },
                "error": null
            }),
            run_process_schema,
        )
        .is_err(),
        "run_process must reject lifecycle booleans that contradict execution_state"
    );
    assert!(
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
            &serde_json::json!({
                "success": false,
                "output": {
                    "failure_kind": "permission_denied",
                    "tool_failure": true
                },
                "error": "permission denied"
            }),
            run_process_schema,
        )
        .is_err(),
        "an execution-style run_process denial must include the canonical lifecycle tuple"
    );
    for (tool_name, schema) in [
        ("run_process", run_process_schema),
        (
            "run_script",
            &spec_named(&specs, "run_script").output_schema,
        ),
    ] {
        assert!(
            crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
                &serde_json::json!({
                    "success": true,
                    "output": {
                        "execution_state": "completed",
                        "command_started": true,
                        "command_completed": true,
                        "command_ok": true,
                        "exit_code": 0,
                        "observation_token": "orphan-observation"
                    },
                    "error": null
                }),
                schema,
            )
            .is_err(),
            "{tool_name} must reject an observation_token without the full continuation tuple"
        );
    }
    let process_started_description =
        output_schema_property(&specs, "run_process", "command_started")["description"]
            .as_str()
            .expect("run_process command_started description");
    assert!(process_started_description.contains("outcome_unknown"));

    for field in [
        "duration_ms",
        "exit_code",
        "stdout_tail",
        "stderr_tail",
        "stdout_lines",
        "stderr_lines",
        "stdout_truncated",
        "stderr_truncated",
        "command_started",
        "command_completed",
        "command_ok",
        "failure_kind",
        "tool_failure",
        "purpose",
        "script_summary",
        "language",
        "cwd",
        "executor",
        "execution_source",
        "execution_state",
        "promoted_to_job",
        "terminal",
        "job_id",
        "job_status",
        "observation_token",
        "effective_timeout_secs",
        "sync_wait_secs",
        "async_handoff_available",
    ] {
        assert!(
            has_output_field("run_script", field),
            "run_script missing {field}"
        );
    }
    assert_eq!(
        output_schema_property(&specs, "run_script", "language")["enum"],
        serde_json::json!(["sh", "bash", "powershell"])
    );
    assert_eq!(
        output_schema_property(&specs, "run_script", "execution_source")["const"],
        "run_script"
    );
    let run_script_schema = &spec_named(&specs, "run_script").output_schema;
    for (state, command_started, command_completed) in [
        ("not_started", false, false),
        ("outcome_unknown", true, false),
        ("completed", true, true),
        ("timed_out", true, false),
    ] {
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
            &serde_json::json!({
                "success": false,
                "output": {
                    "execution_source": "run_script",
                    "execution_state": state,
                    "command_started": command_started,
                    "command_completed": command_completed
                },
                "error": null
            }),
            run_script_schema,
        )
        .unwrap_or_else(|error| panic!("run_script state {state} should validate: {error}"));
    }
    for (state, command_started, job_status) in [
        ("queued", false, "agent_queued"),
        ("running", true, "running"),
    ] {
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
            &structured_execution_output(
                "run_script",
                state,
                command_started,
                false,
                true,
                false,
                Some("job-1"),
                Some(job_status),
            ),
            run_script_schema,
        )
        .unwrap_or_else(|error| {
            panic!("run_script handoff state {state} should validate: {error}")
        });
    }
    assert!(
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
            &serde_json::json!({
                "success": false,
                "output": {
                    "execution_source": "run_script",
                    "execution_state": "completed",
                    "command_started": true,
                    "command_completed": false
                },
                "error": null
            }),
            run_script_schema,
        )
        .is_err(),
        "run_script must reject lifecycle booleans that contradict execution_state"
    );
    assert!(
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
            &serde_json::json!({
                "success": false,
                "output": {
                    "failure_kind": "permission_denied",
                    "tool_failure": true
                },
                "error": "permission denied"
            }),
            run_script_schema,
        )
        .is_err(),
        "an execution-style run_script denial must include the canonical lifecycle tuple"
    );

    for (tool, execution_source, schema) in [
        ("run_process", "run_process", run_process_schema),
        ("run_script", "run_script", run_script_schema),
    ] {
        let mut promoted_without_job_id = structured_execution_output(
            execution_source,
            "running",
            true,
            false,
            true,
            false,
            Some("job-1"),
            Some("running"),
        );
        promoted_without_job_id["output"]
            .as_object_mut()
            .expect("output object")
            .remove("job_id");
        let impossible = [
            ("promoted execution without job_id", promoted_without_job_id),
            (
                "running execution with command_started=false",
                structured_execution_output(
                    execution_source,
                    "running",
                    false,
                    false,
                    true,
                    false,
                    Some("job-1"),
                    Some("running"),
                ),
            ),
            ("promoted execution with async_handoff_available=false", {
                let mut instance = structured_execution_output(
                    execution_source,
                    "running",
                    true,
                    false,
                    true,
                    false,
                    Some("job-1"),
                    Some("running"),
                );
                instance["output"]["async_handoff_available"] = serde_json::json!(false);
                instance
            }),
            (
                "queued execution with command_completed=true",
                structured_execution_output(
                    execution_source,
                    "queued",
                    false,
                    true,
                    true,
                    false,
                    Some("job-1"),
                    Some("agent_queued"),
                ),
            ),
            (
                "completed execution promoted to a Job",
                structured_execution_output(
                    execution_source,
                    "completed",
                    true,
                    true,
                    true,
                    false,
                    Some("job-1"),
                    Some("completed"),
                ),
            ),
            (
                "not_started execution with command_started=true",
                structured_execution_output(
                    execution_source,
                    "not_started",
                    true,
                    false,
                    false,
                    true,
                    None,
                    None,
                ),
            ),
            (
                "timed_out execution with command_completed=true",
                structured_execution_output(
                    execution_source,
                    "timed_out",
                    true,
                    true,
                    false,
                    true,
                    None,
                    None,
                ),
            ),
        ];
        for (description, instance) in impossible {
            assert!(
                crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
                    &instance, schema,
                )
                .is_err(),
                "{tool} schema must reject {description}"
            );
        }
    }

    for field in [
        "duration_ms",
        "exit_code",
        "stdout_tail",
        "stderr_tail",
        "stdout_lines",
        "stderr_lines",
        "stdout_truncated",
        "stderr_truncated",
        "command_started",
        "command_completed",
        "command_ok",
        "failure_kind",
        "tool_failure",
        "purpose",
        "command_summary",
        "cwd",
        "shell",
        "executor",
        "ssh_resource",
        "execution_state",
    ] {
        assert!(
            has_output_field("run_shell", field),
            "run_shell missing {field}"
        );
    }
    let run_shell_started_description =
        output_schema_property(&specs, "run_shell", "command_started")["description"]
            .as_str()
            .expect("run_shell command_started description");
    assert!(
        run_shell_started_description.contains("outcome_unknown"),
        "run_shell command_started must describe conservative unknown-outcome semantics: {run_shell_started_description}"
    );
    let run_shell_state_description =
        output_schema_property(&specs, "run_shell", "execution_state")["description"]
            .as_str()
            .expect("run_shell execution_state description");
    for state in ["not_started", "outcome_unknown", "completed", "timed_out"] {
        assert!(
            run_shell_state_description.contains(state),
            "run_shell execution_state description missing {state}: {run_shell_state_description}"
        );
    }
    for name in ["cargo_fmt", "cargo_check", "cargo_test", "go_test"] {
        assert!(
            has_output_field(name, "observation_token"),
            "{name} missing promoted Job observation_token"
        );
        assert_eq!(
            output_schema_property(&specs, name, "observation_token")["maxLength"],
            crate::job_observation::MAX_JOB_OBSERVATION_TOKEN_LEN
        );
        assert!(
            has_output_field(name, "failure_kind"),
            "{name} missing failure_kind"
        );
        let description = output_schema_property(&specs, name, "failure_kind")["description"]
            .as_str()
            .expect("cargo failure_kind description");
        assert!(
            description.contains("validation_failed"),
            "{name} failure_kind description should mention validation_failed: {description}"
        );
        assert!(
            description.contains("outcome_unknown"),
            "{name} failure_kind description should mention outcome_unknown: {description}"
        );
        let state_description = output_schema_property(&specs, name, "execution_state")
            ["description"]
            .as_str()
            .expect("cargo execution_state description");
        for state in [
            "not_started",
            "outcome_unknown",
            "completed",
            "timed_out",
            "queued",
            "running",
        ] {
            assert!(
                state_description.contains(state),
                "{name} execution_state description missing {state}: {state_description}"
            );
        }
    }
    for field in ["tests_detected", "tests_run_count", "zero_tests_run"] {
        for name in ["cargo_test", "go_test"] {
            assert!(has_output_field(name, field), "{name} missing {field}");
        }
        assert!(
            !has_output_field("cargo_fmt", field),
            "cargo_fmt should not expose cargo_test zero-tests metadata field {field}"
        );
        assert!(
            !has_output_field("cargo_check", field),
            "cargo_check should not expose cargo_test zero-tests metadata field {field}"
        );
    }
    assert!(has_output_field("cargo_test", "diagnostics"));
    assert!(has_output_field("cargo_check", "diagnostics"));
    assert!(!has_output_field("cargo_fmt", "diagnostics"));
    let diagnostics_schema = output_schema_property(&specs, "cargo_test", "diagnostics");
    assert_eq!(diagnostics_schema["type"], "object");
    let diagnostics_props = diagnostics_schema["properties"]
        .as_object()
        .expect("cargo_test diagnostics schema properties");
    for field in [
        "available",
        "parser",
        "reason",
        "diagnostic_count",
        "diagnostics",
        "returned_diagnostic_count",
        "diagnostics_truncated",
        "invalid_diagnostics_omitted",
        "test_summary",
        "failed_test_details",
        "failed_test_details_truncated",
        "truncated",
    ] {
        assert!(
            diagnostics_props.contains_key(field),
            "cargo_test diagnostics schema missing {field}"
        );
    }
    for removed in [
        "first_diagnostic",
        "failed_tests",
        "first_failed_test",
        "failed_tests_truncated",
    ] {
        assert!(
            !diagnostics_props.contains_key(removed),
            "cargo_test diagnostics schema must not retain removed field {removed}"
        );
    }
    assert_eq!(diagnostics_props["diagnostics"]["maxItems"], 20);
    assert_eq!(diagnostics_props["failed_test_details"]["maxItems"], 20);
    assert_eq!(
        diagnostics_props["parser"]["enum"],
        json!(["structured_validation_parser"])
    );
    assert_eq!(
        diagnostics_props["failed_test_details_truncated"]["type"],
        "boolean"
    );
    assert_eq!(
        diagnostics_schema["additionalProperties"], false,
        "cargo_test diagnostics schema must close undeclared fields"
    );
    assert_eq!(
        diagnostics_props["test_summary"]["additionalProperties"], false,
        "cargo_test diagnostics.test_summary schema must close undeclared fields"
    );
    let summary_props = diagnostics_props["test_summary"]["properties"]
        .as_object()
        .expect("cargo_test diagnostics.test_summary properties");
    for field in ["passed", "failed", "ignored"] {
        assert!(
            summary_props.contains_key(field),
            "cargo_test diagnostics.test_summary missing {field}"
        );
    }
    for field in [
        "text",
        "format",
        "start_line",
        "limit",
        "total_lines",
        "returned_lines",
        "end_line",
        "has_more",
        "next_start_line",
        "sha256",
    ] {
        assert!(
            has_output_field("read_file", field),
            "read_file missing {field}"
        );
    }
    for removed in ["content", "numbered_text"] {
        assert!(
            !has_output_field("read_file", removed),
            "read_file must not duplicate its primary text as {removed}"
        );
    }
    for field in [
        "backend",
        "result_mode",
        "effective_timeout_secs",
        "matches",
        "count",
        "files",
        "returned_file_count",
        "returned_match_count",
        "count_complete",
        "total_matches",
        "truncated",
        "truncation_reason",
        "context_before",
        "context_after",
    ] {
        assert!(
            has_output_field("search_project_text", field),
            "search_project_text missing {field}"
        );
    }
    for field in ["project", "path", "entries", "truncated"] {
        assert!(
            has_output_field("list_project_files", field),
            "list_project_files missing {field}"
        );
    }
    for field in [
        "schema_version",
        "project",
        "path",
        "deterministic",
        "project_types",
        "manifests",
        "key_files",
        "roots",
        "top_level",
        "suggested_next_reads",
        "scan",
        "warnings",
    ] {
        assert!(
            has_output_field("project_overview", field),
            "project_overview missing {field}"
        );
    }
    assert!(
        !output_schema_properties(&specs, "list_project_files").contains_key("count"),
        "list_project_files schema must not invent a count field absent from runtime output"
    );
    let file_entries = output_schema_property(&specs, "list_project_files", "entries");
    let file_entry_props = file_entries["items"]["properties"]
        .as_object()
        .expect("list_project_files entries item properties");
    for field in ["path", "kind"] {
        assert!(
            file_entry_props.contains_key(field),
            "list_project_files entry missing {field}"
        );
    }
    for field in [
        "job_id",
        "kind",
        "status",
        "project",
        "ssh_resource",
        "last_update_seq",
    ] {
        assert!(
            has_output_field("run_job", field),
            "run_job missing {field}"
        );
    }
    for field in [
        "already_finished",
        "already_stop_requested",
        "stop_request_accepted",
        "target_was_active_at_request",
        "terminal",
        "terminal_pending",
        "final_status",
        "stop_effect",
        "job_id",
        "project",
        "status_before",
        "status_after",
        "command_started",
        "ownership_basis",
    ] {
        assert!(
            has_output_field("stop_job", field),
            "stop_job missing {field}"
        );
    }
    for field in [
        "job_id",
        "project",
        "session_id",
        "ssh_resource",
        "status",
        "exit_code",
        "started_at",
        "ended_at",
        "error",
        "command_execution_state",
        "structured_execution",
        "command_preview_included",
        "active",
        "blocking_active",
        "terminal",
        "terminal_pending",
        "command_preview",
        "command_preview_truncated",
        "command_preview_max_chars",
        "command_preview_bounded",
    ] {
        assert!(
            has_output_field("job_status", field),
            "job_status missing {field}"
        );
    }
    for field in [
        "job_id",
        "session_id",
        "ssh_resource",
        "exit_code",
        "command_execution_state",
        "structured_execution",
        "stdout_tail",
        "stderr_tail",
        "stdout_lines",
        "stderr_lines",
        "stdout_truncated",
        "stderr_truncated",
        "log_delta_status",
        "stdout_delta_reset",
        "stderr_delta_reset",
        "cursor",
        "status",
        "executor",
        "cwd",
        "shell",
        "purpose",
        "command_summary",
        "detected_summary",
        "wait_outcome",
        "waited_ms",
        "changed",
        "terminal",
    ] {
        assert!(
            has_output_field("job_log", field),
            "job_log missing {field}"
        );
    }
    for field in ["jobs", "count", "truncated"] {
        assert!(
            has_output_field("list_jobs", field),
            "list_jobs missing {field}"
        );
    }
    let jobs_schema = output_schema_property(&specs, "list_jobs", "jobs");
    let jobs_description = jobs_schema["description"]
        .as_str()
        .expect("list_jobs jobs description")
        .to_lowercase();
    assert!(
        jobs_description.contains("bounded") && jobs_description.contains("never includes stdout"),
        "list_jobs jobs description must describe bounded metadata without stdout/stderr bodies: {jobs_description}"
    );
    let job_summary_props = jobs_schema["items"]["properties"]
        .as_object()
        .expect("list_jobs item properties");
    for field in [
        "job_id",
        "kind",
        "status",
        "project",
        "session_id",
        "ssh_resource",
        "executor",
        "created_at",
        "started_at",
        "ended_at",
        "exit_code",
        "command_execution_state",
        "structured_execution",
    ] {
        assert!(
            job_summary_props.contains_key(field),
            "list_jobs summary missing {field}"
        );
    }
    for forbidden in ["stdout", "stderr"] {
        assert!(
            !job_summary_props.contains_key(forbidden),
            "list_jobs summary schema must not expose {forbidden} bodies"
        );
    }
    for field in [
        "job_id",
        "session_id",
        "ssh_resource",
        "exit_code",
        "command_execution_state",
        "structured_execution",
        "stdout_tail",
        "stderr_tail",
        "stdout_lines",
        "stderr_lines",
        "stdout_truncated",
        "stderr_truncated",
        "log_delta_status",
        "stdout_delta_reset",
        "stderr_delta_reset",
        "cursor",
        "status",
        "executor",
        "cwd",
        "shell",
        "purpose",
        "command_summary",
        "detected_summary",
        "wait_outcome",
        "waited_ms",
        "changed",
        "terminal",
    ] {
        assert!(
            has_output_field("job_log", field),
            "job_log missing {field}"
        );
    }
    for field in ["stdout_tail", "stderr_tail"] {
        let description = output_schema_property(&specs, "job_log", field)["description"]
            .as_str()
            .expect("job_log stream description")
            .to_lowercase();
        assert!(
            description.contains("bounded"),
            "job_log {field} description must describe bounded tail text: {description}"
        );
    }
    assert_eq!(
        output_schema_property(&specs, "job_log", "log_delta_status")["enum"],
        serde_json::json!(["baseline", "delta", "unchanged", "reset"])
    );
    assert_eq!(
        output_schema_property(&specs, "job_log", "observation_token")["maxLength"],
        crate::job_observation::MAX_JOB_OBSERVATION_TOKEN_LEN
    );
    let cursor_description = output_schema_property(&specs, "job_log", "cursor")["description"]
        .as_str()
        .expect("job_log cursor description")
        .to_lowercase();
    assert!(
        cursor_description.contains("cursor") && cursor_description.contains("bounded"),
        "job_log cursor must describe bounded continuation metadata: {cursor_description}"
    );
    for field in [
        "path",
        "exists",
        "missing",
        "bytes",
        "sha256",
        "mime_type",
        "modified_at",
    ] {
        assert!(
            has_output_field("read_project_artifact_metadata", field),
            "read_project_artifact_metadata missing {field}"
        );
    }
    for field in [
        "path",
        "file_bytes",
        "offset",
        "bytes_returned",
        "content_base64",
        "next_offset",
        "truncated",
        "eof",
    ] {
        assert!(
            has_output_field("read_project_artifact", field),
            "read_project_artifact missing {field}"
        );
    }
    let upload_progress_fields = [
        "path",
        "upload_id",
        "received_bytes",
        "next_offset",
        "expected_bytes",
        "expected_sha256",
        "committed",
    ];
    for field in upload_progress_fields {
        assert!(
            has_output_field("artifact_upload_begin", field),
            "artifact_upload_begin missing {field}"
        );
        assert!(
            has_output_field("artifact_upload_chunk", field),
            "artifact_upload_chunk missing {field}"
        );
    }
    for field in [
        "path",
        "upload_id",
        "bytes",
        "received_bytes",
        "expected_bytes",
        "expected_sha256",
        "sha256",
        "committed",
    ] {
        assert!(
            has_output_field("artifact_upload_finish", field),
            "artifact_upload_finish missing {field}"
        );
    }
    for field in [
        "path",
        "upload_id",
        "received_bytes",
        "aborted",
        "temp_file_removed",
        "sidecar_removed",
        "final_file_touched",
        "final_file_exists",
        "changed_path_details",
    ] {
        assert!(
            has_output_field("artifact_upload_abort", field),
            "artifact_upload_abort missing {field}"
        );
    }
    for field in [
        "service",
        "version",
        "build",
        "auth_enabled",
        "configured_public_url",
        "agents",
        "projects",
        "jobs",
        "tools",
        "authority",
        "quic",
    ] {
        assert!(
            has_output_field("runtime_status", field),
            "runtime_status missing {field}"
        );
    }
    for field in ["projects", "count", "recommended_for_smoke"] {
        assert!(
            has_output_field("list_projects", field),
            "list_projects missing {field}"
        );
    }
}

#[test]
fn project_onboarding_output_schemas_include_result_metadata_fields() {
    let specs = registered_tool_specs();

    for field in [
        "id",
        "agent_project_id",
        "client_id",
        "name",
        "path",
        "description",
        "projects_config_path",
        "created_config",
        "overwritten",
        "allow_patch",
    ] {
        assert!(
            output_schema_properties(&specs, "register_project").contains_key(field),
            "register_project missing {field}"
        );
    }

    for field in [
        "id",
        "agent_project_id",
        "client_id",
        "name",
        "path",
        "description",
        "projects_config_path",
        "created_directory",
        "created_config",
        "overwritten",
        "allow_patch",
        "template",
        "git_initialized",
    ] {
        assert!(
            output_schema_properties(&specs, "create_project").contains_key(field),
            "create_project missing {field}"
        );
    }

    for tool in ["register_project", "create_project"] {
        let props = output_schema_properties(&specs, tool);
        for forbidden in [
            "token",
            "secret",
            "env",
            "stdout",
            "stderr",
            "command",
            "file_content",
            "content",
        ] {
            assert!(
                !props.contains_key(forbidden),
                "{tool} output schema must not advertise {forbidden}"
            );
        }

        let descriptions = output_schema_description_text(props);
        for phrase in [
            "result metadata",
            "does not include file content",
            "does not expose environment, token, or secret values",
            "does not bypass authorization, permission, allowed-root, or agent path policy",
        ] {
            assert!(
                descriptions.contains(phrase),
                "{tool} output schema descriptions should mention {phrase}: {descriptions}"
            );
        }

        for field in ["path", "projects_config_path"] {
            let description = output_schema_property(&specs, tool, field)["description"]
                .as_str()
                .expect("path-like field description")
                .to_lowercase();
            assert!(
                description.contains("result metadata path")
                    && description.contains("not file content"),
                "{tool} {field} description must describe metadata path only: {description}"
            );
        }

        for field in ["created_config", "overwritten"] {
            let description = output_schema_property(&specs, tool, field)["description"]
                .as_str()
                .expect("outcome field description")
                .to_lowercase();
            assert!(
                description.contains("result outcome metadata"),
                "{tool} {field} description must describe outcome metadata: {description}"
            );
        }
    }

    let created_directory_description =
        output_schema_property(&specs, "create_project", "created_directory")["description"]
            .as_str()
            .expect("created_directory description")
            .to_lowercase();
    assert!(
        created_directory_description.contains("result outcome metadata"),
        "create_project created_directory description must describe outcome metadata: {created_directory_description}"
    );

    let template_description = output_schema_property(&specs, "create_project", "template")
        ["description"]
        .as_str()
        .expect("template description")
        .to_lowercase();
    assert!(
        template_description.contains("does not change")
            && template_description.contains("template behavior"),
        "create_project template description must not imply behavior changes: {template_description}"
    );

    let git_description = output_schema_property(&specs, "create_project", "git_initialized")
        ["description"]
        .as_str()
        .expect("git_initialized description")
        .to_lowercase();
    assert!(
        git_description.contains("does not change") && git_description.contains("git-init"),
        "create_project git_initialized description must not imply behavior changes: {git_description}"
    );
}

#[test]
fn cleanup_tool_output_schemas_include_metadata_fields() {
    let specs = registered_tool_specs();

    for field in ["restored_paths", "command_result"] {
        assert!(
            output_schema_properties(&specs, "git_restore_paths").contains_key(field),
            "git_restore_paths missing {field}"
        );
    }
    for field in ["discarded_untracked_paths", "command_result"] {
        assert!(
            output_schema_properties(&specs, "discard_untracked").contains_key(field),
            "discard_untracked missing {field}"
        );
    }

    let restored = output_schema_property(&specs, "git_restore_paths", "restored_paths");
    assert_eq!(restored["type"], "array");
    assert_eq!(restored["items"]["type"], "string");

    let discarded =
        output_schema_property(&specs, "discard_untracked", "discarded_untracked_paths");
    assert_eq!(discarded["type"], "array");
    assert_eq!(discarded["items"]["type"], "string");
}

#[test]
fn cleanup_output_schemas_describe_result_metadata_only() {
    let specs = registered_tool_specs();

    for tool in ["git_restore_paths", "discard_untracked"] {
        let props = output_schema_properties(&specs, tool);
        for forbidden in [
            "content",
            "file_content",
            "stdout",
            "stderr",
            "stdin",
            "env",
            "token",
            "secret",
            "command",
            "shell_command",
        ] {
            assert!(
                !props.contains_key(forbidden),
                "{tool} output schema must not advertise {forbidden}"
            );
        }

        let description = output_schema_property(&specs, tool, "command_result")["description"]
            .as_str()
            .unwrap_or("")
            .to_lowercase();
        for phrase in [
            "fixed git cleanup",
            "result metadata",
            "not a general shell-execution interface",
        ] {
            assert!(
                description.contains(phrase),
                "{tool} command_result description should mention {phrase}: {description}"
            );
        }
    }
}

#[test]
fn write_project_file_output_schema_include_metadata_fields() {
    let specs = registered_tool_specs();

    // The removed legacy edit tools (`replace_in_file` and friends) are no
    // longer known tools, so they have no public ToolSpec/output schema.
    // Only the visible whole-file write tool's metadata schema is asserted
    // here.

    for field in [
        "path",
        "created",
        "overwritten",
        "bytes_written",
        "sha256",
        "warning",
        "error",
    ] {
        assert!(
            output_schema_properties(&specs, "write_project_file").contains_key(field),
            "write_project_file missing {field}"
        );
    }

    assert_eq!(
        output_schema_property(&specs, "write_project_file", "bytes_written")["type"],
        "integer"
    );
}

#[test]
fn cleanup_and_compatibility_write_output_schemas_do_not_advertise_broad_exfiltration() {
    let specs = registered_tool_specs();

    for tool in [
        "git_restore_paths",
        "discard_untracked",
        "write_project_file",
    ] {
        let props = output_schema_properties(&specs, tool);
        for forbidden in [
            "content",
            "file_content",
            "stdout",
            "stderr",
            "stdin",
            "env",
            "environment",
            "token",
            "secret",
            "old",
            "new",
            "command",
            "shell_command",
        ] {
            assert!(
                !props.contains_key(forbidden),
                "{tool} output schema must not advertise {forbidden}"
            );
        }
    }

    for tool in ["write_project_file"] {
        let descriptions = output_schema_description_text(output_schema_properties(&specs, tool));
        for phrase in [
            "result metadata",
            "does not include file content",
            "not a shell-execution interface",
            "does not expose environment, token, or secret values",
        ] {
            assert!(
                descriptions.contains(phrase),
                "{tool} output schema descriptions should mention {phrase}: {descriptions}"
            );
        }
    }
}

#[test]
fn read_file_output_schema_matches_real_results_and_strict_tool_payloads() {
    let schema = crate::tool_runtime::registry::output_schema_for_tool("read_file");
    let validate = |value: &Value| {
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(value, &schema)
    };
    let success = serde_json::to_value(crate::tool_runtime::tool_result::ToolResult::ok(json!({
        "text": "hello",
        "format": "plain",
        "path": "src/lib.rs",
        "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "start_line": 1,
        "limit": 2,
        "total_lines": 3,
        "returned_lines": 1,
        "end_line": 1,
        "has_more": true,
        "next_start_line": 2
    })))
    .unwrap();
    assert!(
        success.get("error").is_none(),
        "successful ToolResult omits error"
    );
    validate(&success).unwrap();
    assert!(
        success["output"]["returned_lines"].as_u64().unwrap()
            <= success["output"]["limit"].as_u64().unwrap()
    );

    let sparse = serde_json::to_value(crate::tool_runtime::tool_result::ToolResult::ok(json!({
        "text": "hello",
        "path": "src/lib.rs",
        "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "total_lines": 1
    })))
    .unwrap();
    validate(&sparse).unwrap();
    let mut sparse_numbered = sparse.clone();
    sparse_numbered["output"]["format"] = json!("numbered");
    validate(&sparse_numbered).unwrap();

    let mut sparse_plain = sparse.clone();
    sparse_plain["output"]["format"] = json!("plain");
    assert!(
        validate(&sparse_plain).is_err(),
        "complete sparse plain output must omit redundant format"
    );
    let default_limit = webcodex_workspace::file_read_range::EffectiveRange::new(None, None).limit;
    let mut sparse_over_default_limit = sparse.clone();
    sparse_over_default_limit["output"]["total_lines"] = json!(default_limit + 1);
    assert!(
        validate(&sparse_over_default_limit).is_err(),
        "complete sparse read_file output cannot claim more lines than the default range can return"
    );

    let read_files_schema = crate::tool_runtime::registry::output_schema_for_tool("read_files");
    let sparse_batch_over_default_limit = json!({
        "success": true,
        "output": {
            "items": [{
                "index": 0,
                "path": "src/lib.rs",
                "success": true,
                "output": {
                    "text": "hello",
                    "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "total_lines": default_limit + 1
                },
                "error": null
            }]
        },
        "error": null
    });
    assert!(
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
            &sparse_batch_over_default_limit,
            &read_files_schema,
        )
        .is_err(),
        "complete sparse read_files item cannot claim more lines than the default range can return"
    );
    let mut sparse_missing_sha = sparse.clone();
    sparse_missing_sha["output"]
        .as_object_mut()
        .unwrap()
        .remove("sha256");
    assert!(validate(&sparse_missing_sha).is_err());
    let mut sparse_fake_continuation = sparse.clone();
    sparse_fake_continuation["output"]["has_more"] = json!(true);
    assert!(
        validate(&sparse_fake_continuation).is_err(),
        "sparse complete form must not admit partial-read continuation fields"
    );

    let failure = serde_json::to_value(
        crate::tool_runtime::tool_result::ToolResult::err_with_output(
            "read_file failed: not_found",
            json!({
                "error_kind": "read_file_failed",
                "reason_code": "not_found",
                "path": "missing.rs",
                "state_changed": false
            }),
        ),
    )
    .unwrap();
    validate(&failure).unwrap();

    // Failures produced before the read implementation runs retain the runtime's
    // generic null/object payloads and must still match the advertised envelope.
    let generic_null = serde_json::to_value(crate::tool_runtime::tool_result::ToolResult::err(
        "unknown project",
    ))
    .unwrap();
    validate(&generic_null).unwrap();
    let generic_object = serde_json::to_value(
        crate::tool_runtime::tool_result::ToolResult::err_with_output(
            "session guard denied",
            json!({"error_kind": "session_guard_denied", "state_changed": false}),
        ),
    )
    .unwrap();
    validate(&generic_object).unwrap();

    let mut telemetry = success.clone();
    telemetry["output"]["session_recorded"] = json!(true);
    telemetry["output"]["session_id"] = json!("wc_sess_test");
    telemetry["output"]["session_event_id"] = json!("evt_test");
    validate(&telemetry).unwrap();

    for missing in ["next_start_line", "sha256"] {
        let mut value = success.clone();
        value["output"].as_object_mut().unwrap().remove(missing);
        assert!(validate(&value).is_err(), "missing {missing} was accepted");
    }

    let mut bad_sha = success.clone();
    bad_sha["output"]["sha256"] = json!("ABC");
    assert!(validate(&bad_sha).is_err());

    let mut zero_limit = success.clone();
    zero_limit["output"]["limit"] = json!(0);
    assert!(validate(&zero_limit).is_err());

    let mut impossible_count = success.clone();
    impossible_count["output"]["returned_lines"] = json!(2001);
    assert!(validate(&impossible_count).is_err());

    let mut missing_reason = failure.clone();
    missing_reason["output"]
        .as_object_mut()
        .unwrap()
        .remove("reason_code");
    assert!(validate(&missing_reason).is_err());

    let mut unknown_failure = failure.clone();
    unknown_failure["output"]["runner_secret"] = json!("hidden");
    assert!(validate(&unknown_failure).is_err());

    let mut unknown_output = success.clone();
    unknown_output["output"]["runner_secret"] = json!("hidden");
    assert!(validate(&unknown_output).is_err());

    let mut unknown_top = success;
    unknown_top["runner_secret"] = json!("hidden");
    assert!(validate(&unknown_top).is_err());
}

fn default_output_schema_field_names() -> BTreeSet<&'static str> {
    BTreeSet::from([
        "session_recorded",
        "session_id",
        "session_event_id",
        "session_hint",
        "permission",
        "recovery_kind",
        "recovery_tool",
    ])
}

fn output_schema_field_names(spec: &ToolSpec) -> BTreeSet<&str> {
    let mut fields = BTreeSet::new();
    collect_envelope_output_fields(&spec.output_schema, &mut fields);
    assert!(
        !fields.is_empty(),
        "{} output schema properties or variants",
        spec.name
    );
    fields
}

fn collect_envelope_output_fields<'a>(schema: &'a Value, fields: &mut BTreeSet<&'a str>) {
    if let Some(output) = schema
        .get("properties")
        .and_then(Value::as_object)
        .and_then(|properties| properties.get("output"))
    {
        collect_output_variant_fields(output, fields);
    }
    for keyword in ["oneOf", "anyOf", "allOf"] {
        if let Some(variants) = schema.get(keyword).and_then(Value::as_array) {
            for variant in variants {
                collect_envelope_output_fields(variant, fields);
            }
        }
    }
}

fn collect_output_variant_fields<'a>(schema: &'a Value, fields: &mut BTreeSet<&'a str>) {
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        fields.extend(properties.keys().map(String::as_str));
    }
    for keyword in ["oneOf", "anyOf", "allOf"] {
        if let Some(variants) = schema.get(keyword).and_then(Value::as_array) {
            for variant in variants {
                collect_output_variant_fields(variant, fields);
            }
        }
    }
    for keyword in ["then", "else"] {
        if let Some(branch) = schema.get(keyword) {
            collect_output_variant_fields(branch, fields);
        }
    }
}

fn output_schema_properties<'a>(
    specs: &'a [ToolSpec],
    name: &str,
) -> &'a serde_json::Map<String, Value> {
    let spec = spec_named(specs, name);
    spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap_or_else(|| panic!("{} output schema properties", spec.name))
}

fn output_schema_property<'a>(specs: &'a [ToolSpec], name: &str, field: &str) -> &'a Value {
    output_schema_properties(specs, name)
        .get(field)
        .unwrap_or_else(|| panic!("{name} missing output field {field}"))
}

fn output_schema_description_text(props: &serde_json::Map<String, Value>) -> String {
    props
        .values()
        .filter_map(|schema| schema["description"].as_str())
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[test]
fn validation_summary_schema_exposes_optional_recoverable_assertion_label_only() {
    let schema = crate::tool_runtime::registry::output_schema_for_tool("validation_summary");
    let event = &schema["properties"]["output"]["properties"]["validation"]["properties"]["events"]
        ["items"];
    let properties = event["properties"].as_object().unwrap();
    let assertion = &properties["assertion_name"];
    assert_eq!(assertion["type"], "string");
    assert_eq!(assertion["minLength"], 1);
    assert_eq!(
        assertion["maxLength"],
        crate::tool_runtime::sessions::MAX_MODEL_VALIDATION_ASSERTION_NAME_CHARS
    );
    let required = event["required"].as_array().unwrap();
    assert!(!required.iter().any(|field| field == "assertion_name"));
    for hidden in ["expected_failure", "expected_failure_kind"] {
        assert!(
            !properties.contains_key(hidden),
            "validation event schema must not expose internal expectation field {hidden}"
        );
    }
}

#[test]
fn finish_coding_task_output_schema_describes_ledger_validation_summary() {
    let schema = crate::tool_runtime::registry::output_schema_for_tool("finish_coding_task");
    let output_props = schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    assert!(
        output_props.contains_key("permissions"),
        "finish_coding_task output schema should include permissions"
    );
    assert!(
        output_props.contains_key("tool_failures"),
        "finish_coding_task output schema should include classified tool failures"
    );
    assert!(
        output_props.contains_key("summary_only"),
        "finish_coding_task output schema should include summary_only for compact output"
    );
    assert!(
        output_props.contains_key("review_evidence"),
        "finish_coding_task output schema should include review_evidence"
    );
    assert!(
        !output_props.contains_key("verdict"),
        "finish_coding_task output schema should omit legacy verdict"
    );
    assert!(
        !output_props.contains_key("finish_verdict"),
        "finish_coding_task output schema should omit finish_verdict alias"
    );
    for field in [
        "task_outcome",
        "evidence_history",
        "evidence_integrity",
        "informational_notes",
    ] {
        assert!(
            output_props.contains_key(field),
            "finish_coding_task output schema should include {field}"
        );
    }
    assert_outcome_model_schema_fields(output_props);
    assert!(
        output_props.contains_key("suggested_next_actions"),
        "finish_coding_task output schema should include top-level suggested_next_actions"
    );
    assert_permission_summary_schema_fields(&output_props["permissions"]);
    assert_job_lifecycle_summary_schema_fields(&output_props["jobs"]);
    assert_review_evidence_schema_fields(&output_props["review_evidence"]);
    let description = schema["properties"]["output"]["properties"]["validation"]["description"]
        .as_str()
        .unwrap();
    let description = description.to_lowercase();
    for phrase in [
        "validation closeout evidence",
        "full closeout",
        "historical",
        "resolved",
        "unresolved",
        "stable identity",
        "summary_only",
        "final status/reason",
        "success/failure counts",
        "zero-test integrity flag",
    ] {
        assert!(
            description.contains(phrase),
            "validation output schema should mention {phrase}: {description}"
        );
    }
    for forbidden in [
        "backward-compatible",
        "first_diagnostic",
        "first_failed_test",
        "failed_tests,",
    ] {
        assert!(
            !description.contains(forbidden),
            "validation output schema must not mention removed compatibility phrase {forbidden}: {description}"
        );
    }
    let review_description = output_props["review_evidence"]["description"]
        .as_str()
        .unwrap()
        .to_lowercase();
    for phrase in [
        "full-closeout",
        "ledger-derived",
        "non-cargo review evidence",
        "omitted from summary_only",
        "internally in canonical task_outcome",
        "does not include file contents",
    ] {
        assert!(
            review_description.contains(phrase),
            "finish review_evidence schema should mention {phrase}: {review_description}"
        );
    }
    let suggested_description = output_props["suggested_next_actions"]["description"]
        .as_str()
        .unwrap()
        .to_lowercase();
    for phrase in [
        "top-level",
        "final closeout actions",
        "summary_only",
        "task outcome",
        "evidence integrity",
    ] {
        assert!(
            suggested_description.contains(phrase),
            "finish suggested_next_actions schema should mention {phrase}: {suggested_description}"
        );
    }
}

#[test]
fn session_handoff_summary_schema_exposes_ledger_validation_summary() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "session_handoff_summary");
    let input_props = spec.input_schema["properties"].as_object().unwrap();
    assert!(
        input_props.contains_key("include_validation"),
        "session_handoff_summary input schema should include include_validation"
    );
    assert!(
        input_props.contains_key("summary_only"),
        "session_handoff_summary input schema should include summary_only"
    );

    let schema = crate::tool_runtime::registry::output_schema_for_tool("session_handoff_summary");
    let output_props = schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    assert!(
        output_props.contains_key("validation"),
        "session_handoff_summary output schema should include validation"
    );
    assert!(
        output_props.contains_key("review_evidence"),
        "session_handoff_summary output schema should include review_evidence"
    );
    assert!(
        output_props.contains_key("permissions"),
        "session_handoff_summary output schema should include permissions"
    );
    assert!(
        output_props.contains_key("tool_failures"),
        "session_handoff_summary output schema should include classified tool failures"
    );
    assert!(
        output_props.contains_key("expected_failed_tool_calls"),
        "session_handoff_summary output schema should include expected failed tool calls"
    );
    assert!(
        output_props.contains_key("unexpected_failed_tool_calls"),
        "session_handoff_summary output schema should include unexpected failed tool calls"
    );
    assert!(
        output_props.contains_key("expectation_mismatches"),
        "session_handoff_summary output schema should include expectation mismatches"
    );
    for field in [
        "task_outcome",
        "evidence_history",
        "evidence_integrity",
        "informational_notes",
    ] {
        assert!(
            output_props.contains_key(field),
            "session_handoff_summary output schema should include {field}"
        );
    }
    assert_outcome_model_schema_fields(output_props);
    assert_permission_summary_schema_fields(&output_props["permissions"]);
    assert_job_lifecycle_summary_schema_fields(&output_props["jobs"]);
    assert_review_evidence_schema_fields(&output_props["review_evidence"]);
    let description = output_props["validation"]["description"]
        .as_str()
        .unwrap()
        .to_lowercase();
    for phrase in [
        "ledger-derived",
        "validation-like tool-call summary",
        "status/reason",
        "does not include stdout/stderr",
        "structured diagnostics",
        "bounded validation metadata",
        "parser version 3",
        "canonical diagnostics",
        "failed_test_details",
        "no root-cause inference",
        "parser.available remains false when session ledger events lack those fields",
        "latest_status",
        "historical_failures",
    ] {
        assert!(
            description.contains(phrase),
            "handoff validation output schema should mention {phrase}: {description}"
        );
    }
    for forbidden in [
        "backward-compatible",
        "first_diagnostic",
        "first_failed_test",
        "failed_tests,",
    ] {
        assert!(
            !description.contains(forbidden),
            "handoff validation output schema must not mention removed compatibility phrase {forbidden}: {description}"
        );
    }
    let review_description = output_props["review_evidence"]["description"]
        .as_str()
        .unwrap()
        .to_lowercase();
    for phrase in [
        "ledger-derived",
        "non-cargo review evidence",
        "summary_only",
        "read/search/diff/workspace/hygiene",
        "bounded tools",
        "does not include file contents",
        "does not change validation.status",
    ] {
        assert!(
            review_description.contains(phrase),
            "handoff review_evidence schema should mention {phrase}: {review_description}"
        );
    }
}

fn assert_permission_summary_schema_fields(schema: &Value) {
    let props = schema["properties"].as_object().unwrap();
    for field in [
        "approved_count",
        "manual_approved_count",
        "auto_approved_count",
        "total_approved_count",
    ] {
        assert!(props.contains_key(field), "permissions missing {field}");
    }
}

fn assert_job_lifecycle_summary_schema_fields(schema: &Value) {
    let props = schema["properties"].as_object().unwrap();
    for field in [
        "active_count",
        "running_count",
        "stop_requested_count",
        "terminal_pending_count",
        "blocking_active_count",
        "nonblocking_active_count",
        "warnings",
    ] {
        assert!(props.contains_key(field), "jobs summary missing {field}");
    }
}

fn assert_review_evidence_schema_fields(schema: &Value) {
    let props = schema["properties"].as_object().unwrap();
    for field in [
        "available",
        "total",
        "read_only_inspection_count",
        "search_count",
        "diff_review_count",
        "workspace_review_count",
        "hygiene_review_count",
        "tools",
    ] {
        assert!(props.contains_key(field), "review_evidence missing {field}");
    }
    assert_eq!(props["tools"]["type"], "array");
    assert_eq!(props["tools"]["items"]["type"], "string");
}

fn assert_outcome_model_schema_fields(output_props: &serde_json::Map<String, Value>) {
    assert_eq!(
        output_props["task_outcome"]["properties"]["status"]["enum"],
        json!(["pass", "warn", "fail"])
    );
    assert_eq!(
        output_props["evidence_history"]["properties"]["status"]["enum"],
        json!(["clean", "mixed_resolved", "mixed_unresolved", "failed"])
    );
    assert_eq!(
        output_props["evidence_integrity"]["properties"]["status"]["enum"],
        json!(["clean", "warning", "error"])
    );
    assert_eq!(output_props["informational_notes"]["type"], "array");
}
