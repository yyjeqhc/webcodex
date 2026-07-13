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
    for name in ["cargo_fmt", "cargo_check", "cargo_test"] {
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
    }
    for field in [
        "tests_detected",
        "tests_run_count",
        "zero_tests_run",
        "diagnostics",
    ] {
        assert!(
            has_output_field("cargo_test", field),
            "cargo_test missing {field}"
        );
        assert!(
            !has_output_field("cargo_fmt", field),
            "cargo_fmt should not expose cargo_test zero-tests metadata field {field}"
        );
        assert!(
            !has_output_field("cargo_check", field),
            "cargo_check should not expose cargo_test zero-tests metadata field {field}"
        );
    }
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
        "ledger-based",
        "validation-like tool-call summary",
        "status/reason",
        "does not include stdout/stderr",
        "structured diagnostics",
        "bounded validation metadata",
        "parser version 3",
        "canonical diagnostics",
        "failed_test_details",
        "no root-cause inference",
        "latest_status",
        "historical_failures",
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
