use super::*;

struct DefaultOutputSchemaAllowance {
    name: &'static str,
    reason: &'static str,
}

// TODO(tool-definition): remove entries as these tools gain explicit output
// schema fields, or move the allowlist to a generated definition-backed
// declaration once output_schema is part of ToolDefinition.
const MODEL_VISIBLE_TOOLS_ALLOWED_TO_USE_DEFAULT_OUTPUT_SCHEMA: &[DefaultOutputSchemaAllowance] = &[
    DefaultOutputSchemaAllowance {
        name: "register_project",
        reason: "project onboarding response still uses the generic wrapper while schema coverage converges",
    },
    DefaultOutputSchemaAllowance {
        name: "create_project",
        reason: "project onboarding response still uses the generic wrapper while schema coverage converges",
    },
    DefaultOutputSchemaAllowance {
        name: "list_project_files",
        reason: "bounded file-list payload is covered by behavior tests while output schema is pending",
    },
    DefaultOutputSchemaAllowance {
        name: "list_jobs",
        reason: "bounded job summary payload is covered by behavior tests while output schema is pending",
    },
    DefaultOutputSchemaAllowance {
        name: "job_tail",
        reason: "bounded log-tail payload is covered by behavior tests while output schema is pending",
    },
    DefaultOutputSchemaAllowance {
        name: "git_restore_paths",
        reason: "cleanup write result is covered by behavior tests while output schema is pending",
    },
    DefaultOutputSchemaAllowance {
        name: "discard_untracked",
        reason: "cleanup write result is covered by behavior tests while output schema is pending",
    },
    DefaultOutputSchemaAllowance {
        name: "replace_in_file",
        reason: "compatibility edit result is covered by behavior tests while output schema is pending",
    },
    DefaultOutputSchemaAllowance {
        name: "write_project_file",
        reason: "compatibility whole-file write result is covered by behavior tests while output schema is pending",
    },
];

#[test]
fn model_visible_tool_definitions_have_output_schema_coverage_or_allowance() {
    let specs = registered_tool_specs();
    let default_fields = default_output_schema_field_names();
    let default_schema_names = specs
        .iter()
        .filter(|spec| output_schema_field_names(spec) == default_fields)
        .map(|spec| spec.name.as_str())
        .collect::<Vec<_>>();
    let allowed_names = MODEL_VISIBLE_TOOLS_ALLOWED_TO_USE_DEFAULT_OUTPUT_SCHEMA
        .iter()
        .map(|allowance| {
            assert!(
                !allowance.reason.trim().is_empty(),
                "{} default output schema allowance must explain the drift risk",
                allowance.name
            );
            allowance.name
        })
        .collect::<Vec<_>>();

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
        "duration_ms",
        "exit_code",
        "stdout",
        "stderr",
        "command_started",
        "command_completed",
        "command_ok",
        "failure_kind",
        "tool_failure",
    ] {
        assert!(
            has_output_field("run_shell", field),
            "run_shell missing {field}"
        );
    }
    for field in [
        "content",
        "start_line",
        "limit",
        "total_lines",
        "numbered_text",
        "lines",
    ] {
        assert!(
            has_output_field("read_file", field),
            "read_file missing {field}"
        );
    }
    for field in [
        "backend",
        "matches",
        "count",
        "truncated",
        "context_before",
        "context_after",
    ] {
        assert!(
            has_output_field("search_project_text", field),
            "search_project_text missing {field}"
        );
    }
    for field in ["job_id", "kind", "status", "project"] {
        assert!(
            has_output_field("run_job", field),
            "run_job missing {field}"
        );
    }
    for field in [
        "stopped",
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
        "status",
        "exit_code",
        "started_at",
        "ended_at",
        "error",
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
        "stdout",
        "stderr",
        "next_stdout_line",
        "next_stderr_line",
        "status",
    ] {
        assert!(
            has_output_field("job_log", field),
            "job_log missing {field}"
        );
    }
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
        "permissions",
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

fn default_output_schema_field_names() -> BTreeSet<&'static str> {
    BTreeSet::from([
        "session_recorded",
        "session_id",
        "session_event_id",
        "session_hint",
        "permission",
    ])
}

fn output_schema_field_names(spec: &ToolSpec) -> BTreeSet<&str> {
    spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap_or_else(|| panic!("{} output schema properties", spec.name))
        .keys()
        .map(String::as_str)
        .collect()
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
    assert_permission_summary_schema_fields(&output_props["permissions"]);
    assert_job_lifecycle_summary_schema_fields(&output_props["jobs"]);
    let description = schema["properties"]["output"]["properties"]["validation"]["description"]
        .as_str()
        .unwrap();
    let description = description.to_lowercase();
    for phrase in [
        "ledger-based",
        "validation-like tool-call summary",
        "status/reason",
        "does not include stdout/stderr",
        "minimal diagnostics",
        "bounded tails",
        "safe result metadata",
        "never infer root cause",
    ] {
        assert!(
            description.contains(phrase),
            "validation output schema should mention {phrase}: {description}"
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
    assert_permission_summary_schema_fields(&output_props["permissions"]);
    assert_job_lifecycle_summary_schema_fields(&output_props["jobs"]);
    let description = output_props["validation"]["description"]
        .as_str()
        .unwrap()
        .to_lowercase();
    for phrase in [
        "ledger-derived",
        "validation-like tool-call summary",
        "status/reason",
        "does not include stdout/stderr",
        "minimal diagnostics",
        "bounded tails",
        "safe result metadata",
        "never infer root cause",
        "parser.available remains false when session ledger events lack those fields",
    ] {
        assert!(
            description.contains(phrase),
            "handoff validation output schema should mention {phrase}: {description}"
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
