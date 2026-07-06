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

    assert_eq!(specs.len(), 66, "model-visible tools.count");
    assert_eq!(
        specs.len() - default_schema_names.len(),
        66,
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
    for field in ["project", "path", "entries", "truncated"] {
        assert!(
            has_output_field("list_project_files", field),
            "list_project_files missing {field}"
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
        "executor",
        "created_at",
        "started_at",
        "ended_at",
        "exit_code",
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
        "stdout",
        "stderr",
        "next_stdout_line",
        "next_stderr_line",
        "status",
    ] {
        assert!(
            has_output_field("job_tail", field),
            "job_tail missing {field}"
        );
    }
    for field in ["stdout", "stderr"] {
        let description = output_schema_property(&specs, "job_tail", field)["description"]
            .as_str()
            .expect("job_tail stream description")
            .to_lowercase();
        assert!(
            description.contains("bounded") && description.contains("not an unbounded"),
            "job_tail {field} description must describe bounded tail text: {description}"
        );
    }
    for field in ["next_stdout_line", "next_stderr_line"] {
        let description = output_schema_property(&specs, "job_tail", field)["description"]
            .as_str()
            .expect("job_tail offset description")
            .to_lowercase();
        assert!(
            description.contains("offset") && description.contains("bounded tail"),
            "job_tail {field} description must describe bounded tail offset metadata: {description}"
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
fn compatibility_edit_output_schemas_include_metadata_fields() {
    let specs = registered_tool_specs();

    for field in [
        "changed",
        "path",
        "replacements",
        "before_sha256",
        "after_sha256",
        "bytes_written",
        "occurrences",
        "expected",
        "error",
    ] {
        assert!(
            output_schema_properties(&specs, "replace_in_file").contains_key(field),
            "replace_in_file missing {field}"
        );
    }

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
        output_schema_property(&specs, "replace_in_file", "replacements")["type"],
        "integer"
    );
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
        "replace_in_file",
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

    for tool in ["replace_in_file", "write_project_file"] {
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
        "latest_status",
        "historical_failures",
    ] {
        assert!(
            description.contains(phrase),
            "validation output schema should mention {phrase}: {description}"
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
        "does not include file contents",
        "does not change validation.status",
    ] {
        assert!(
            review_description.contains(phrase),
            "finish review_evidence schema should mention {phrase}: {review_description}"
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
        "latest_status",
        "historical_failures",
    ] {
        assert!(
            description.contains(phrase),
            "handoff validation output schema should mention {phrase}: {description}"
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
