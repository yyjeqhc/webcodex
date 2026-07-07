//! Tool Call tests for tool_runtime.

use super::super::*;
use super::support::*;
use serde_json::{json, Value};

#[test]
fn from_tool_name_parses_unit_tools_without_arguments() {
    for name in [
        "list_tools",
        "list_projects",
        "list_agents",
        "runtime_status",
    ] {
        let call = ToolCall::from_tool_name(name, Value::Null).unwrap_or_else(|e| panic!("{}", e));
        assert!(
            matches!(
                call,
                ToolCall::ListTools { .. }
                    | ToolCall::ListProjects
                    | ToolCall::ListAgents
                    | ToolCall::RuntimeStatus { .. }
            ),
            "unit tool {} should parse",
            name
        );
    }
}

#[test]
fn from_tool_name_parses_unit_tools_with_empty_object() {
    let call = ToolCall::from_tool_name("list_tools", json!({})).unwrap();
    assert!(matches!(call, ToolCall::ListTools { .. }));
}

#[test]
fn from_tool_name_parses_bounded_list_tools_options() {
    let call = ToolCall::from_tool_name(
        "list_tools",
        json!({
            "category": "artifact",
            "features": "artifact_upload",
            "summary_only": true,
            "limit": 4
        }),
    )
    .unwrap();
    match call {
        ToolCall::ListTools {
            category,
            features,
            summary_only,
            limit,
        } => {
            assert_eq!(category.as_deref(), Some("artifact"));
            assert_eq!(features.as_deref(), Some("artifact_upload"));
            assert!(summary_only);
            assert_eq!(limit, Some(4));
        }
        other => panic!("expected ListTools, got {:?}", other),
    }
}

#[test]
fn from_tool_name_strips_testing_metadata_before_parsing() {
    let call = ToolCall::from_tool_name(
        "job_status",
        json!({
            "job_id": "abc",
            "expected_failure": true,
            "expected_failure_kind": "job_not_found",
            "test_expect_failure_kind": "job_not_found",
            "assertion_name": "missing job negative path"
        }),
    )
    .unwrap();
    assert!(matches!(
        call,
        ToolCall::JobStatus {
            ref job_id,
            include_command_preview: false,
        } if job_id == "abc"
    ));
}

#[test]
fn artifact_upload_followup_tools_missing_path_error_is_actionable() {
    for name in [
        "artifact_upload_chunk",
        "artifact_upload_finish",
        "artifact_upload_abort",
    ] {
        let err =
            ToolCall::from_tool_name(name, json!({"upload_id": "wc_upload_test_1"})).unwrap_err();
        assert!(
            err.contains("path is required")
                && err.contains("artifact_upload_begin")
                && err.contains("bind upload_id"),
            "{name}: {err}"
        );
    }
}

#[test]
fn from_tool_name_parses_read_project_artifact_metadata_allow_missing() {
    let call = ToolCall::from_tool_name(
        "read_project_artifact_metadata",
        json!({
            "project": "agent:demo:smoke",
            "path": "artifacts/smoke/missing.artifact",
            "allow_missing": true
        }),
    )
    .unwrap();

    match call {
        ToolCall::ReadProjectArtifactMetadata {
            project,
            path,
            allow_missing,
            ..
        } => {
            assert_eq!(project, "agent:demo:smoke");
            assert_eq!(path, "artifacts/smoke/missing.artifact");
            assert_eq!(allow_missing, Some(true));
        }
        other => panic!("expected ReadProjectArtifactMetadata, got {:?}", other),
    }
}

#[test]
fn from_tool_name_parses_run_shell_with_required_fields() {
    let call = ToolCall::from_tool_name(
        "run_shell",
        json!({"project": "demo", "command": "echo hi"}),
    )
    .unwrap();
    match call {
        ToolCall::RunShell {
            project,
            command,
            timeout_secs,
            cwd,
            ..
        } => {
            assert_eq!(project, "demo");
            assert_eq!(command, "echo hi");
            assert_eq!(timeout_secs, None);
            assert_eq!(cwd, None);
        }
        other => panic!("expected RunShell, got {:?}", other),
    }
}

#[test]
fn from_tool_name_parses_run_shell_with_optional_fields() {
    let call = ToolCall::from_tool_name(
        "run_shell",
        json!({"project": "demo", "command": "ls", "timeout_secs": 5, "cwd": "sub"}),
    )
    .unwrap();
    match call {
        ToolCall::RunShell {
            project,
            command,
            timeout_secs,
            cwd,
            ..
        } => {
            assert_eq!(project, "demo");
            assert_eq!(command, "ls");
            assert_eq!(timeout_secs, Some(5));
            assert_eq!(cwd, Some("sub".to_string()));
        }
        other => panic!("expected RunShell, got {:?}", other),
    }
}

#[test]
fn from_tool_name_rejects_removed_run_codex() {
    let err = ToolCall::from_tool_name(
        "run_codex",
        json!({
            "project": "demo",
            "prompt": "fix tests",
            "approval_mode": "suggest",
            "timeout_secs": 120,
            "cwd": "src",
            "extra_args": ["--verbose"]
        }),
    )
    .unwrap_err();
    assert!(err.contains("unknown tool"), "{err}");
}

#[test]
fn from_tool_name_parses_job_status_and_job_log() {
    let call = ToolCall::from_tool_name("job_status", json!({"job_id": "abc"})).unwrap();
    assert!(matches!(
        call,
        ToolCall::JobStatus {
            ref job_id,
            include_command_preview: false,
        } if job_id == "abc"
    ));

    let call = ToolCall::from_tool_name(
        "job_status",
        json!({"job_id": "abc", "include_command_preview": true}),
    )
    .unwrap();
    assert!(matches!(
        call,
        ToolCall::JobStatus {
            ref job_id,
            include_command_preview: true,
        } if job_id == "abc"
    ));

    let call = ToolCall::from_tool_name("job_log", json!({"job_id": "abc", "offset": 10})).unwrap();
    match call {
        ToolCall::JobLog {
            job_id,
            offset,
            tail_lines,
        } => {
            assert_eq!(job_id, "abc");
            assert_eq!(offset, Some(10));
            assert_eq!(tail_lines, None);
        }
        other => panic!("expected JobLog, got {:?}", other),
    }
}

#[test]
fn from_tool_name_parses_stop_job_with_default_confirmation_false() {
    let call =
        ToolCall::from_tool_name("stop_job", json!({"project": "demo", "job_id": "abc"})).unwrap();
    match call {
        ToolCall::StopJob {
            project,
            job_id,
            confirm,
            session_id,
        } => {
            assert_eq!(project, "demo");
            assert_eq!(job_id, "abc");
            assert!(!confirm);
            assert!(session_id.is_none());
        }
        other => panic!("expected StopJob, got {:?}", other),
    }

    let call = ToolCall::from_tool_name(
        "stop_job",
        json!({"project": "demo", "job_id": "abc", "session_id": "wc_sess_x", "confirm": true}),
    )
    .unwrap();
    assert!(matches!(
        call,
        ToolCall::StopJob {
            ref project,
            ref job_id,
            ref session_id,
            confirm: true,
        } if project == "demo" && job_id == "abc" && session_id.as_deref() == Some("wc_sess_x")
    ));
}

#[test]
fn from_tool_name_parses_read_file_and_git_tools() {
    let call =
        ToolCall::from_tool_name("read_file", json!({"project": "demo", "path": "README.md"}))
            .unwrap();
    assert!(matches!(call, ToolCall::ReadFile { .. }));

    let call = ToolCall::from_tool_name(
        "read_file",
        json!({
            "project": "demo",
            "path": "src/main.rs",
            "start_line": 10,
            "limit": 3,
            "with_line_numbers": true
        }),
    )
    .unwrap();
    match call {
        ToolCall::ReadFile {
            project,
            path,
            start_line,
            limit,
            with_line_numbers,
            ..
        } => {
            assert_eq!(project, "demo");
            assert_eq!(path, "src/main.rs");
            assert_eq!(start_line, Some(10));
            assert_eq!(limit, Some(3));
            assert_eq!(with_line_numbers, Some(true));
        }
        other => panic!("expected ReadFile, got {:?}", other),
    }

    let call = ToolCall::from_tool_name("git_status", json!({"project": "demo"})).unwrap();
    assert!(matches!(call, ToolCall::GitStatus { .. }));

    let call = ToolCall::from_tool_name("git_diff", json!({"project": "demo", "args": ["--stat"]}))
        .unwrap();
    assert!(matches!(call, ToolCall::GitDiff { .. }));

    let call = ToolCall::from_tool_name("apply_patch", json!({"project": "demo", "patch": "diff"}))
        .unwrap();
    assert!(matches!(call, ToolCall::ApplyPatch { .. }));

    let call =
        ToolCall::from_tool_name("run_job", json!({"project": "demo", "command": "make"})).unwrap();
    assert!(matches!(call, ToolCall::RunJob { .. }));
}

#[test]
fn from_tool_name_rejects_unknown_tool_name() {
    let err = ToolCall::from_tool_name("not_a_tool", Value::Null).unwrap_err();
    assert!(err.contains("not_a_tool"));
}

#[test]
fn from_tool_name_rejects_missing_required_field() {
    let err = ToolCall::from_tool_name("run_shell", json!({"command": "echo"})).unwrap_err();
    assert!(
        err.contains("project"),
        "error should mention missing field: {}",
        err
    );

    let err = ToolCall::from_tool_name("job_status", json!({})).unwrap_err();
    assert!(err.contains("job_id"));
}

#[test]
fn from_tool_name_rejects_wrong_field_type() {
    let err = ToolCall::from_tool_name("run_shell", json!({"project": 123, "command": "echo"}))
        .unwrap_err();
    assert!(!err.is_empty());
}

#[test]
fn from_tool_name_error_includes_tool_name() {
    let err = ToolCall::from_tool_name("run_shell", json!({})).unwrap_err();
    assert!(err.contains("run_shell"));
}

#[test]
fn tool_call_project_accessor_covers_project_tool_specs() {
    for spec in registered_tool_specs() {
        let call = ToolCall::from_tool_name(&spec.name, sample_tool_args(&spec.name))
            .unwrap_or_else(|e| panic!("{} should deserialize: {}", spec.name, e));
        let schema_has_project = spec.input_schema["properties"].get("project").is_some();
        // `start_session` has an optional project for task association that is
        // intentionally not exposed by `project()`. `session_handoff_summary`
        // has an optional project for workspace/checkpoint enrichment; sample
        // args omit it (it is not required), so `project()` returns `None`.
        let expected_project = if schema_has_project
            && spec.name != "start_session"
            && spec.name != "session_handoff_summary"
        {
            Some("agent:oe:private-drop")
        } else {
            None
        };
        assert_eq!(
            call.project(),
            expected_project,
            "{} ToolCall::project() mismatch",
            spec.name
        );
    }

    // start_session's optional project is task association metadata, not an
    // execution target used for authorization or kernel project reporting.
    let start_session =
        ToolCall::from_tool_name("start_session", json!({"project": "agent:oe:private-drop"}))
            .unwrap();
    assert_eq!(start_session.project(), None);

    // session_handoff_summary's optional project IS exposed by project()
    // when provided, so the kernel can report it and authorize the workspace
    // git inspection path.
    let handoff = ToolCall::from_tool_name(
        "session_handoff_summary",
        json!({"session_id": "wc_sess_x", "project": "agent:oe:private-drop"}),
    )
    .unwrap();
    assert_eq!(handoff.project(), Some("agent:oe:private-drop"));
}

#[test]
fn tool_call_session_id_accessor_covers_session_tool_specs() {
    for spec in registered_tool_specs() {
        if spec.input_schema["properties"].get("session_id").is_none() {
            continue;
        }
        if spec.input_schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|field| field == "session_id")
        {
            continue;
        }
        let call = ToolCall::from_tool_name(&spec.name, sample_tool_args_with_session(&spec.name))
            .unwrap_or_else(|e| panic!("{} should deserialize: {}", spec.name, e));
        assert_eq!(
            call.session_id(),
            Some("wc_sess_accessor"),
            "{} ToolCall::session_id() mismatch",
            spec.name
        );
    }
}

#[test]
fn from_tool_name_unknown_tool_lists_available_tools_and_hint() {
    let err = ToolCall::from_tool_name("definitely_not_a_tool", Value::Null).unwrap_err();
    assert!(err.contains("definitely_not_a_tool"));
    assert!(
        err.contains("listRuntimeTools") || err.contains("list_tools"),
        "unknown-tool error should hint at discovery: {}",
        err
    );
    // Should list at least a couple of known tool names.
    assert!(err.contains("git_diff_summary"));
    assert!(err.contains("apply_patch_checked"));
    // Must not leak secret/config artifacts.
    let lower = err.to_lowercase();
    for forbidden in [
        "token",
        "authorization",
        "agent.toml",
        "webcodex.env",
        "secret",
    ] {
        assert!(
            !lower.contains(&forbidden),
            "unknown-tool error must not leak '{}': {}",
            forbidden,
            err
        );
    }
}

#[test]
fn known_tool_names_matches_spec_count() {
    let specs = registered_tool_specs();
    for spec in &specs {
        assert!(
            is_known_tool_name(&spec.name),
            "{} spec must be known to ToolCall",
            spec.name
        );
        assert!(
            !is_model_hidden_tool_name(&spec.name),
            "{} must not be model-hidden when exposed in registered tool specs",
            spec.name
        );
    }
    assert_eq!(
        specs.len(),
        known_tool_names().count(),
        "registered tool specs should cover every known runtime tool after hidden tools are removed"
    );
    // Every known name must be recognized (i.e. must NOT yield the
    // "unknown tool" error). Unit tools parse with null args; non-unit
    // tools fail with a missing-field error, which is still a recognition
    // success (the variant matched).
    for name in known_tool_names() {
        assert!(
            is_known_tool_name(name),
            "known name '{}' not recognized by is_known_tool_name",
            name
        );
        let result = ToolCall::from_tool_name(name, Value::Null);
        match result {
            Ok(_) => {}
            Err(e) => {
                assert!(
                    !e.contains("unknown tool"),
                    "known tool '{}' was treated as unknown: {}",
                    name,
                    e
                );
            }
        }
    }
    // An unknown name must still produce the unknown-tool error.
    let err = ToolCall::from_tool_name("not_a_real_tool", Value::Null).unwrap_err();
    assert!(err.contains("unknown tool"));
    assert!(
        !err.contains("run_codex"),
        "unknown-tool guidance must not advertise hidden tools: {}",
        err
    );
}

#[test]
fn from_tool_name_parses_runtime_status() {
    let call = ToolCall::from_tool_name("runtime_status", Value::Null).unwrap();
    assert!(matches!(
        call,
        ToolCall::RuntimeStatus {
            compact: false,
            summary_only: false
        }
    ));
    // Also accepts an empty object.
    let call = ToolCall::from_tool_name("runtime_status", json!({})).unwrap();
    assert!(matches!(
        call,
        ToolCall::RuntimeStatus {
            compact: false,
            summary_only: false
        }
    ));
    let call = ToolCall::from_tool_name(
        "runtime_status",
        json!({"compact": true, "summary_only": true}),
    )
    .unwrap();
    assert!(matches!(
        call,
        ToolCall::RuntimeStatus {
            compact: true,
            summary_only: true
        }
    ));
}

#[test]
fn from_tool_name_parses_finish_coding_task_include_workspace_compatibility() {
    let call = ToolCall::from_tool_name(
        "finish_coding_task",
        json!({
            "project": "agent:client:demo",
            "session_id": "wc_sess_demo",
            "summary_only": true,
            "include_workspace": true,
            "include_validation_summary": true,
            "include_hygiene": true,
            "include_handoff": true,
            "include_diff": false
        }),
    )
    .unwrap();

    match call {
        ToolCall::FinishCodingTask {
            project,
            session_id,
            summary_only,
            include_diff,
            include_workspace,
            include_hygiene,
            include_handoff,
            include_validation_summary,
        } => {
            assert_eq!(project, "agent:client:demo");
            assert_eq!(session_id, "wc_sess_demo");
            assert!(summary_only);
            assert_eq!(include_diff, Some(false));
            assert_eq!(include_workspace, Some(true));
            assert_eq!(include_hygiene, Some(true));
            assert_eq!(include_handoff, Some(true));
            assert_eq!(include_validation_summary, Some(true));
        }
        other => panic!("expected finish_coding_task, got {other:?}"),
    }
}

#[test]
fn from_tool_name_parses_phase_a_tools() {
    let call = ToolCall::from_tool_name("list_project_files", json!({"project": "demo"})).unwrap();
    match call {
        ToolCall::ListProjectFiles {
            project,
            path,
            limit,
            ..
        } => {
            assert_eq!(project, "demo");
            assert_eq!(path, None);
            assert_eq!(limit, None);
        }
        other => panic!("expected ListProjectFiles, got {:?}", other),
    }

    let call = ToolCall::from_tool_name(
        "search_project_text",
        json!({
            "project": "demo",
            "pattern": "fn main",
            "limit": 5,
            "context_before": 3,
            "context_after": 8
        }),
    )
    .unwrap();
    match call {
        ToolCall::SearchProjectText {
            project,
            pattern,
            path,
            limit,
            context_before,
            context_after,
            ..
        } => {
            assert_eq!(project, "demo");
            assert_eq!(pattern, "fn main");
            assert_eq!(path, None);
            assert_eq!(limit, Some(5));
            assert_eq!(context_before, Some(3));
            assert_eq!(context_after, Some(8));
        }
        other => panic!("expected SearchProjectText, got {:?}", other),
    }

    let call = ToolCall::from_tool_name("git_diff_summary", json!({"project": "demo"})).unwrap();
    assert!(matches!(call, ToolCall::GitDiffSummary { project, .. } if project == "demo"));

    // list_jobs has only optional fields; null arguments must still parse.
    let call = ToolCall::from_tool_name("list_jobs", Value::Null).unwrap();
    assert!(matches!(
        call,
        ToolCall::ListJobs {
            limit: None,
            status: None
        }
    ));
    let call =
        ToolCall::from_tool_name("list_jobs", json!({"limit": 3, "status": "running"})).unwrap();
    match call {
        ToolCall::ListJobs { limit, status } => {
            assert_eq!(limit, Some(3));
            assert_eq!(status.as_deref(), Some("running"));
        }
        other => panic!("expected ListJobs, got {:?}", other),
    }

    let call =
        ToolCall::from_tool_name("job_tail", json!({"job_id": "abc", "tail_lines": 10})).unwrap();
    match call {
        ToolCall::JobTail { job_id, tail_lines } => {
            assert_eq!(job_id, "abc");
            assert_eq!(tail_lines, Some(10));
        }
        other => panic!("expected JobTail, got {:?}", other),
    }
}

#[test]
fn from_tool_name_list_jobs_with_null_arguments_parses() {
    // Regression: a non-unit tool with all-optional fields must deserialize
    // when a caller passes `null` arguments (normalized to an empty object).
    let call = ToolCall::from_tool_name("list_jobs", Value::Null)
        .unwrap_or_else(|e| panic!("list_jobs with null args should parse: {}", e));
    assert!(matches!(call, ToolCall::ListJobs { .. }));
}

#[test]
fn from_tool_name_parses_checked_and_cleanup_tools() {
    let checked = ToolCall::from_tool_name(
        "apply_patch_checked",
        json!({"project":"agent:c:p","patch":"diff","deny_sensitive_paths":true}),
    )
    .unwrap();
    assert!(matches!(
        checked,
        ToolCall::ApplyPatchChecked { project, patch, deny_sensitive_paths, .. }
            if project == "agent:c:p" && patch == "diff" && deny_sensitive_paths == Some(true)
    ));

    let delete = ToolCall::from_tool_name(
        "delete_project_files",
        json!({"project":"agent:c:p","paths":["tmp.txt"]}),
    )
    .unwrap();
    assert!(
        matches!(delete, ToolCall::DeleteProjectFiles { project, paths, .. } if project == "agent:c:p" && paths == vec!["tmp.txt"])
    );

    let restore = ToolCall::from_tool_name(
        "git_restore_paths",
        json!({"project":"agent:c:p","paths":["README.md"]}),
    )
    .unwrap();
    assert!(
        matches!(restore, ToolCall::GitRestorePaths { project, paths, .. } if project == "agent:c:p" && paths == vec!["README.md"])
    );

    let discard = ToolCall::from_tool_name(
        "discard_untracked",
        json!({"project":"agent:c:p","paths":["tmp.txt"]}),
    )
    .unwrap();
    assert!(
        matches!(discard, ToolCall::DiscardUntracked { project, paths, .. } if project == "agent:c:p" && paths == vec!["tmp.txt"])
    );
}

#[test]
fn from_tool_name_parses_validate_patch() {
    let call = ToolCall::from_tool_name(
        "validate_patch",
        json!({"project": "agent:c:p", "patch": "diff"}),
    )
    .unwrap();
    assert!(
        matches!(call, ToolCall::ValidatePatch { project, patch, .. } if project == "agent:c:p" && patch == "diff")
    );
}

#[test]
fn from_tool_name_parses_phase4_edit_tools() {
    let replace = ToolCall::from_tool_name(
        "replace_in_file",
        json!({
            "project": "agent:c:p",
            "path": "src/main.rs",
            "old": "foo",
            "new": "bar",
            "expected_replacements": 3,
            "allow_multiple": true
        }),
    )
    .unwrap();
    assert!(matches!(
        replace,
        ToolCall::ReplaceInFile { project, path, old, new, expected_replacements, allow_multiple, .. }
            if project == "agent:c:p"
            && path == "src/main.rs"
            && old == "foo"
            && new == "bar"
            && expected_replacements == Some(3)
            && allow_multiple == Some(true)
    ));

    let write = ToolCall::from_tool_name(
        "write_project_file",
        json!({
            "project": "agent:c:p",
            "path": "new.txt",
            "content": "hello"
        }),
    )
    .unwrap();
    assert!(matches!(
        write,
        ToolCall::WriteProjectFile { project, path, content, overwrite, expected_sha256, expected_content_prefix, .. }
            if project == "agent:c:p"
            && path == "new.txt"
            && content == "hello"
            && overwrite.is_none()
            && expected_sha256.is_none()
            && expected_content_prefix.is_none()
    ));

    let replace_lines = ToolCall::from_tool_name(
        "replace_line_range",
        json!({
            "project": "agent:c:p",
            "path": "src/main.rs",
            "start_line": 2,
            "end_line": 4,
            "new_text": "replacement",
            "expected_old_prefix": "old"
        }),
    )
    .unwrap();
    assert!(matches!(
        replace_lines,
        ToolCall::ReplaceLineRange { project, path, start_line, end_line, new_text, expected_old_prefix, .. }
            if project == "agent:c:p"
            && path == "src/main.rs"
            && start_line == 2
            && end_line == 4
            && new_text == "replacement"
            && expected_old_prefix.as_deref() == Some("old")
    ));

    let insert = ToolCall::from_tool_name(
        "insert_at_line",
        json!({"project": "agent:c:p", "path": "src/main.rs", "line": 1, "text": "use x;"}),
    )
    .unwrap();
    assert!(matches!(insert, ToolCall::InsertAtLine { line: 1, .. }));

    let delete = ToolCall::from_tool_name(
        "delete_line_range",
        json!({"project": "agent:c:p", "path": "src/main.rs", "start_line": 8, "end_line": 9}),
    )
    .unwrap();
    assert!(matches!(
        delete,
        ToolCall::DeleteLineRange {
            start_line: 8,
            end_line: 9,
            ..
        }
    ));
}

#[test]
fn from_tool_name_parses_replace_exact_block() {
    let call = ToolCall::from_tool_name(
            "replace_exact_block",
            json!({
                "project": "agent:c:p",
                "path": "src/main.rs",
                "old_text": "old",
                "new_text": "new",
                "expected_old_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
            }),
        )
        .unwrap();
    assert!(matches!(
        call,
        ToolCall::ReplaceExactBlock { project, path, old_text, new_text, expected_old_sha256, .. }
            if project == "agent:c:p"
            && path == "src/main.rs"
            && old_text == "old"
            && new_text == "new"
            && expected_old_sha256.is_some()
    ));
}

#[test]
fn from_tool_name_parses_insert_before_pattern() {
    let call = ToolCall::from_tool_name(
            "insert_before_pattern",
            json!({"project": "agent:c:p", "path": "src/main.rs", "pattern": "fn main", "text": "// before\n"}),
        )
        .unwrap();
    assert!(matches!(
        call,
        ToolCall::InsertBeforePattern { project, path, pattern, text, .. }
            if project == "agent:c:p" && path == "src/main.rs" && pattern == "fn main" && text == "// before\n"
    ));
}

#[test]
fn from_tool_name_parses_insert_after_pattern() {
    let call = ToolCall::from_tool_name(
            "insert_after_pattern",
            json!({"project": "agent:c:p", "path": "src/main.rs", "pattern": "fn main", "text": " // after"}),
        )
        .unwrap();
    assert!(matches!(
        call,
        ToolCall::InsertAfterPattern { project, path, pattern, text, .. }
            if project == "agent:c:p" && path == "src/main.rs" && pattern == "fn main" && text == " // after"
    ));
}

#[test]
fn from_tool_name_parses_project_management_tools() {
    let register = ToolCall::from_tool_name(
        "register_project",
        json!({
            "client_id":"oe",
            "id":"my-project",
            "name":"My Project",
            "path":"/root/git/my-project"
        }),
    )
    .unwrap();
    assert!(matches!(
        register,
        ToolCall::RegisterProject { ref client_id, ref id, ref name, ref path, .. }
            if client_id == "oe" && id == "my-project" && name == "My Project"
            && path == "/root/git/my-project"
    ));

    let create = ToolCall::from_tool_name(
        "create_project",
        json!({
            "client_id":"oe",
            "id":"hello",
            "name":"Hello",
            "path":"/root/git/hello",
            "template":"basic",
            "git_init":true
        }),
    )
    .unwrap();
    assert!(matches!(
        create,
        ToolCall::CreateProject { ref client_id, ref id, ref name, ref path, ref template, git_init, .. }
            if client_id == "oe" && id == "hello" && name == "Hello"
            && path == "/root/git/hello" && template.as_deref() == Some("basic")
            && git_init
    ));
}
