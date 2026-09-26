use super::*;

#[test]
fn tool_specs_describe_default_coding_loop_preferences() {
    let specs = registered_tool_specs();

    let desc = |name: &str| spec_named(&specs, name).description.to_lowercase();

    let work_on_project_desc = desc("work_on_project");
    for phrase in [
        "canonical bootstrap",
        "ordinary coding/review",
        "project_ref",
        "omit session_id",
        "fresh workflow session",
        "does not imply a fresh model context",
        "fresh or uncertain model context",
        "re-observes instruction files",
        "exact resume",
        "active accessible session",
        "never guesses prior session",
        "context_request",
        "project.instructions",
        "webcodex.workflow",
        "guidance_profile",
        "no authority",
        "principal-scoped",
        "reauthorizes",
        "include_extension_catalog",
        "skills/plugins",
        "mode=worktree",
        "exact git base",
        "project authority",
    ] {
        assert!(
            work_on_project_desc.contains(phrase),
            "work_on_project description should mention {phrase}: {work_on_project_desc}"
        );
    }

    let read_files_desc = desc("read_files");
    for phrase in [
        "batch/snapshot-aware project inspect",
        "read_revision",
        "snapshot-bound continuation",
        "protected-path policy",
        "range normalization",
        "target symbol/test/implementation region is known",
        "bounded targeted ranges",
        "batch related ranges already known to be needed",
        "do not read an entire large file merely because the budget permits it",
        "small known one-off observation",
        "without downstream snapshot dependency",
        "native file commands",
        "no fake continuation",
        "512 kib",
        "exact resolved project",
        "business session_id",
        "continued ranges are fenced",
        "runtime rejects a continuation",
    ] {
        assert!(
            read_files_desc.contains(phrase),
            "read_files description should mention {phrase}: {read_files_desc}"
        );
    }

    let batch_search_desc = desc("search_project_texts");
    for obsolete in [
        "adaptive runtime preferred batch-capable inspect tool",
        "including when only one known range is needed",
    ] {
        assert!(
            !read_files_desc.contains(obsolete),
            "obsolete read ritual: {read_files_desc}"
        );
    }

    for phrase in [
        "batch-capable project-text search",
        "bounded structured results",
        "protected-path policy",
        "isolated failures",
        "broad discovery",
        "files_with_matches/count",
        "matched source will be read immediately",
        "search_and_read",
        "small known-scope search",
        "native rg is first-class",
        "batch only independent queries",
        "result-dependent follow-ups sequential",
        "pattern_mode=literal",
        "returned suggested_call",
        "whole-query continuation",
        "truncated individual queries must be narrowed",
    ] {
        assert!(
            batch_search_desc.contains(phrase),
            "search_project_texts description should mention {phrase}: {batch_search_desc}"
        );
    }
    for obsolete in [
        "adaptive runtime preferred batch-capable project-text search",
        "including when only one query is needed",
    ] {
        assert!(
            !batch_search_desc.contains(obsolete),
            "obsolete search ritual returned: {batch_search_desc}"
        );
    }

    let search_and_read_desc = desc("search_and_read");
    for phrase in [
        "one bounded project-text query or 1..8 predetermined independent queries",
        "query xor queries",
        "max_reads is one global read budget",
        "shared fairly across the batch",
        "preserving per-query batch failures",
        "batch only independent queries",
        "result-dependent follow-ups sequential",
    ] {
        assert!(
            search_and_read_desc.contains(phrase),
            "search_and_read description should mention {phrase}: {search_and_read_desc}"
        );
    }
    assert!(
        !search_and_read_desc.contains("run one bounded project-text search"),
        "obsolete single-query search_and_read description returned: {search_and_read_desc}"
    );

    let save_artifact_desc = desc("save_project_artifact");
    for phrase in [
        "already holds the bounded binary/base64 content",
        "do not read a current chatgpt/host attachment",
        "import_conversation_files_to_project",
    ] {
        assert!(
            save_artifact_desc.contains(phrase),
            "save_project_artifact: {phrase}"
        );
    }
    let import_artifact_desc = desc("import_conversation_files_to_project");
    for phrase in [
        "preferred host-native attachment-to-project transfer path",
        "do not base64-transfer files",
        "active authenticated oauth client",
        "openai file hosts",
        "arbitrary public https",
        "up to 256 mib per file",
        "batch is not atomic",
        "partial_success=true",
    ] {
        assert!(
            import_artifact_desc.contains(phrase),
            "import_conversation_files_to_project: {phrase}"
        );
    }
    let project_artifact_desc = desc("project_artifact");
    let transfer_artifact_desc = desc("transfer_project_artifact");
    for phrase in [
        "source project:read",
        "destination project:write",
        "independently resolved and authorized",
        "exact source bytes/sha-256/mime snapshot",
        "do not pass through host attachments or model text",
    ] {
        assert!(
            transfer_artifact_desc.contains(phrase),
            "transfer_project_artifact: {phrase}"
        );
    }
    for phrase in [
        "metadata=facts",
        "inspect=fenced segment",
        "image=mcp image",
        "export=mcp resourcelink",
        "not repeated inspect calls",
        "import_conversation_files_to_project",
    ] {
        assert!(
            project_artifact_desc.contains(phrase),
            "project_artifact: {phrase}"
        );
    }
    let read_artifact_desc = desc("read_project_artifact");
    for phrase in [
        "bounded chunk inspection api",
        "parser-ready suggested_call",
        "expected_sha256",
        "snapshot_changed",
        "do not manually translate",
        "do not loop over base64 chunks",
        "project_artifact(action=export)",
    ] {
        assert!(
            read_artifact_desc.contains(phrase),
            "read_project_artifact: {phrase}"
        );
    }
    let upload_begin_desc = desc("artifact_upload_begin");
    for phrase in [
        "low-level chunked binary artifact upload",
        "not the preferred path for a current chatgpt/host attachment",
        "import_conversation_files_to_project",
    ] {
        assert!(
            upload_begin_desc.contains(phrase),
            "artifact_upload_begin: {phrase}"
        );
    }

    let git_log_desc = desc("git_log");
    for phrase in ["next_skip", "parser-ready", "10000 skip bound"] {
        assert!(git_log_desc.contains(phrase), "git_log: {phrase}");
    }
    let list_files_desc = desc("list_project_files");
    for phrase in [
        "deterministic page",
        "next_offset",
        "complete directory source",
        "retained-tail truncation fails closed",
    ] {
        assert!(
            list_files_desc.contains(phrase),
            "list_project_files: {phrase}"
        );
    }
    let tracked_files_desc = desc("list_project_tracked_files");
    for phrase in [
        "bounded producer source",
        "source acquisition is complete",
        "list_truncated=true",
        "next_offset is null",
        "narrow path",
        "retained-tail source truncation fails closed",
    ] {
        assert!(
            tracked_files_desc.contains(phrase),
            "list_project_tracked_files: {phrase}"
        );
    }

    let show_changes_desc = desc("show_changes");
    for phrase in [
        "canonical bounded workspace-wide review",
        "worktree overview",
        "compact session signals",
        "structured closeout evidence",
        "tiny targeted git observations need not call it first",
        "diff_review_handoff",
        "git_diff_hunks",
    ] {
        assert!(
            show_changes_desc.contains(phrase),
            "show_changes description should mention {phrase}: {show_changes_desc}"
        );
    }

    let git_diff_hunks = spec_named(&specs, "git_diff_hunks");
    assert!(!show_changes_desc.contains("default inspect/review tool before final response"));

    let git_review_summary_desc = desc("git_review_summary");
    for phrase in [
        "broad or unknown ranges",
        "file/change map",
        "targeted git_diff_hunks/read_files",
        "small bounded understood committed diffs may use native git directly",
        "never mutates",
    ] {
        assert!(
            git_review_summary_desc.contains(phrase),
            "git_review_summary description should mention {phrase}: {git_review_summary_desc}"
        );
    }

    let git_diff_hunks_desc = git_diff_hunks.description.to_lowercase();
    for phrase in [
        "targeted/paged",
        "scope/fence-bound opaque continuation",
        "safe bounded traversal",
        "max_page_bytes",
        "raw producer page",
        "shared safe producer maximum",
        "512 kib",
        "final model-facing",
        "parser-ready next_call",
        "recovery.later_hunks.next_call",
        "next logical diff record",
        "never an intra-hunk cursor",
        "recovery.current_hunk.next_call",
        "bounded refinement",
        "exact hunk-fragment token",
        "next complete diff line",
        "line-budget and page-byte-budget truncation",
        "positive complete-line progress",
        "safe forward progress is not proven",
    ] {
        assert!(
            git_diff_hunks_desc.contains(phrase),
            "git_diff_hunks description should mention {phrase}: {git_diff_hunks_desc}"
        );
    }
    let default_page_bytes = git_diff_hunks.input_schema["properties"]["max_page_bytes"]["default"]
        .as_u64()
        .expect("git_diff_hunks max_page_bytes default");
    assert_eq!(
        default_page_bytes as usize,
        webcodex_core::runtime_contract::DEFAULT_GIT_DIFF_HUNKS_PAGE_BYTES
    );
    assert_eq!(
        webcodex_core::runtime_contract::DEFAULT_GIT_DIFF_HUNKS_PAGE_BYTES,
        webcodex_core::runtime_contract::MAX_GIT_DIFF_HUNKS_PAGE_BYTES
    );
    let continuation_desc = git_diff_hunks.input_schema["properties"]["continuation"]
        ["description"]
        .as_str()
        .expect("git_diff_hunks continuation description")
        .to_lowercase();
    for phrase in [
        "later-record page cursor",
        "next complete-line fragment",
        "scope/fence-bound",
        "parser-ready suggested_call",
        "exact original effective scope/paging inputs",
        "base_commit/head_commit",
        "cached/worktree mode",
        "paths",
        "max_hunks",
        "max_hunk_lines",
        "max_page_bytes",
        "later-record and hunk-fragment continuations remain distinct identities",
    ] {
        assert!(
            continuation_desc.contains(phrase),
            "git_diff_hunks continuation description should mention {phrase}: {continuation_desc}"
        );
    }

    // Patch input remains available without becoming the default recovery path.
    let apply_patch_desc = desc("apply_patch");
    for phrase in [
        "naturally patch-shaped",
        "genuinely the clearest reliable representation",
        "not the default recovery",
        "stable unique context",
        "function/impl/type/test/module",
        "transactional",
        "rollback",
        "dry_run",
        "matching_mode=unique",
        "matching_mode_rejected",
        "weakening the guard",
        "first_match",
        "preserve unique/exact_unique",
        "outcome_unknown",
    ] {
        assert!(
            apply_patch_desc.contains(phrase),
            "apply_patch description should mention {phrase}: {apply_patch_desc}"
        );
    }
    assert!(!apply_patch_desc.contains("prefer apply_patch"));

    let apply_text_edits_desc = desc("apply_text_edits");
    for phrase in [
        "transactional structured option",
        "small/local exact edits",
        "globally unique",
        "may omit expected_read_revision",
        "occurrence or line_scope",
        "requires expected_read_revision",
        "revisions fence whole-file snapshots",
        "model input never needs a digest",
        "preflighted transactionally",
        "conflicts fail closed",
        "rechecks source before mutation",
        "one parser-ready read_files recovery call",
        "inspect the resulting diff",
        "validate the final source",
    ] {
        assert!(
            apply_text_edits_desc.contains(phrase),
            "apply_text_edits description should mention {phrase}: {apply_text_edits_desc}"
        );
    }
    assert!(!apply_text_edits_desc.contains("canonical default guarded edit path"));
    assert!(!apply_text_edits_desc.contains("expected_sha256"));
    assert!(!apply_text_edits_desc.contains("prefer apply_patch"));

    let unified_diff_desc = desc("apply_unified_diff");
    for phrase in [
        "external raw unified-diff mutation path",
        "input is already a standard unified diff",
        "clearest reliable mutation",
        "bounded preflight",
        "never needs a separate validation call",
        "rather than converting model-generated work into unified diff by default",
    ] {
        assert!(
            unified_diff_desc.contains(phrase),
            "apply_unified_diff description should mention {phrase}: {unified_diff_desc}"
        );
    }

    let write_file_desc = desc("write_project_file");
    for phrase in [
        "create a new file",
        "intentional whole-file replacement",
        "expected_read_revision",
        "model-facing snapshot handle",
        "model does not copy a digest",
        "minimal error facts",
        "one parser-ready read_files recovery call",
        "clearest reliable mutation",
        "inspect the resulting diff",
        "validate the final source",
    ] {
        assert!(
            write_file_desc.contains(phrase),
            "write_project_file description should mention {phrase}: {write_file_desc}"
        );
    }
    assert!(!write_file_desc.contains("expected_sha256"));
    assert!(!write_file_desc.contains("prefer apply_text_edits"));

    // The legacy single-purpose edit tools (replace_line_range, insert_at_line,
    // delete_line_range, replace_in_file, replace_exact_block,
    // insert_before_pattern, insert_after_pattern) were removed; they no longer
    // carry a model-facing ToolSpec/description. Their absence from the known
    // tool set is asserted by
    // `removed_legacy_edit_tools_are_not_known_tools` and the parser-name gate
    // in `tool_call_parser_name_gate_matches_tool_definitions`.

    for name in ["cargo_check", "cargo_test"] {
        let validation_desc = desc(name);
        assert!(
            validation_desc.contains("structured") && validation_desc.contains("common supported"),
            "{name} should explain the structured option's supported semantics: {validation_desc}"
        );
        for phrase in ["validation identity", "same execution job handoff"] {
            assert!(
                validation_desc.contains(phrase),
                "{name} should explain structured evidence semantics {phrase}: {validation_desc}"
            );
        }
        assert!(
            !validation_desc.contains("preferred structured"),
            "{name} must not encode structure as an unconditional preference: {validation_desc}"
        );
        assert!(
            !validation_desc.contains("run_shell"),
            "{name} should stay focused on its own differentiating semantics: {validation_desc}"
        );
    }
    let cargo_test_desc = desc("cargo_test");
    for phrase in [
        "executed-test evidence",
        "min_tests/require_tests",
        "bounded output",
    ] {
        assert!(cargo_test_desc.contains(phrase), "cargo_test: {phrase}");
    }
    let go_test_desc = desc("go_test");
    for phrase in [
        "structured option for common supported",
        "go json test-count evidence",
    ] {
        assert!(go_test_desc.contains(phrase), "go_test: {phrase}");
    }
    assert!(!go_test_desc.contains("preferred structured"));
    let cargo_fmt_desc = desc("cargo_fmt");
    for phrase in [
        "final formatting after relevant rust source stabilizes",
        "do not use cargo_fmt as a per-edit ritual",
        "read-only formatting validation",
        "precheck",
        "changed/state_changed",
    ] {
        assert!(cargo_fmt_desc.contains(phrase), "cargo_fmt: {phrase}");
    }

    let workspace_hygiene_desc = desc("workspace_hygiene_check");
    for phrase in ["pre-final", "workspace hygiene", "read-only"] {
        assert!(
            workspace_hygiene_desc.contains(phrase),
            "workspace_hygiene_check description should mention {phrase}: {workspace_hygiene_desc}"
        );
    }

    let handoff_desc = desc("session_handoff_summary");
    for phrase in ["handoff", "missing task context", "read-only"] {
        assert!(
            handoff_desc.contains(phrase),
            "session_handoff_summary description should mention {phrase}: {handoff_desc}"
        );
    }

    let run_shell_desc = desc("run_shell");
    for phrase in [
        "bounded shell grammar or a short related command chain",
        "prefer run_process for literal argv",
        "run_script for program-like scripts",
        "predetermined related observations may share one command",
        "result-dependent follow-ups stay sequential",
        "project-source mutation should normally use canonical structured editors",
        "runner-owned execution",
        "timeout_secs is total lifetime",
        "server timing policy controls job-handoff grace",
        "execution_state=pending",
        "continue independent work",
        "sparse terminal job attention",
        "observe_jobs only for logs/details/recovery",
        "wait_for_job_terminal only when terminal outcome is a true dependency",
        "duration alone does not select a detached primitive",
    ] {
        assert!(
            run_shell_desc.contains(phrase),
            "run_shell description should mention {phrase}: {run_shell_desc}"
        );
    }

    let run_process_desc = desc("run_process");
    for phrase in [
        "native executable with structured literal argv",
        "prefer this over run_shell unless shell grammar or a short related command chain is required",
        "windows batch shims",
        "bounded runner-owned quoting contract",
        "persistent shell is only for retained same-process or named-ssh state, not command count",
        "same execution and remains runner-owned",
        "execution_state=pending",
        "continue independent work",
        "sparse terminal job attention",
        "observe_jobs only for logs/details/recovery",
        "wait_for_job_terminal only when terminal outcome is a true dependency",
        "run_detached_process",
        "survive runner restart/upgrade/stop/replacement",
        "duration alone is not a reason to detach",
    ] {
        assert!(
            run_process_desc.contains(phrase),
            "run_process description should mention {phrase}: {run_process_desc}"
        );
    }

    // Interpreter/runtime details stay locked by run_script input-schema tests;
    // this model-facing description test keeps selection and lifecycle decisions dense.
    let run_script_desc = desc("run_script");
    for phrase in [
        "sh, bash, powershell, python, javascript, or typescript",
        "run_process for native argv",
        "computation/inspection/generation/non-source transforms",
        "run_shell for shell grammar",
        "project-source mutation should normally use canonical structured editors",
        "same execution and remains runner-owned",
        "execution_state=pending",
        "continue independent work",
        "sparse terminal job attention",
        "observe_jobs only for logs/details/recovery",
        "wait_for_job_terminal only when terminal outcome is a true dependency",
        "script bodies never become shell command text",
        "survive runner restart/upgrade/stop/replacement",
    ] {
        assert!(
            run_script_desc.contains(phrase),
            "run_script description should mention {phrase}: {run_script_desc}"
        );
    }

    let run_job_desc = desc("run_job");
    for phrase in [
        "runner-owned",
        "server disconnect/restart",
        "replacement runner does not inherit",
        "outlive the current runner process",
        "run_detached_process",
    ] {
        assert!(
            run_job_desc.contains(phrase),
            "run_job description should mention {phrase}: {run_job_desc}"
        );
    }

    let detached_desc = desc("run_detached_process");
    for phrase in [
        "supervisor-owned",
        "outlive the initiating runner process",
        "runner exit",
        "restart",
        "upgrade",
        "replacement",
        "ownership is handed off before payload start",
        "expired keys are not retry tokens",
        "workflow will restart, upgrade, stop, or replace this runner",
        "duration alone is not a reason to detach",
    ] {
        assert!(
            detached_desc.contains(phrase),
            "run_detached_process description should mention {phrase}: {detached_desc}"
        );
    }

    let open_shell_desc = desc("open_session_shell");
    for phrase in [
        "primary use",
        "repeated commands",
        "active named ssh resource",
        "execution_context.resource",
        "remote cwd/env/exports/functions/umask",
        "local sh/bash or windows powershell remains supported",
        "same local shell-process state",
        "not merely for several commands",
        "update_session_context",
        "no per-shell host/resource parameter",
        "does not need webcodex runner",
        "ssh_resource",
        "runner restart",
    ] {
        assert!(
            open_shell_desc.contains(phrase),
            "open_session_shell description should mention {phrase}: {open_shell_desc}"
        );
    }

    let update_context_desc = desc("update_session_context");
    for phrase in [
        "active runner-local named ssh resource",
        "open_session_shell",
        "ssh_resource",
        "restart the runner",
    ] {
        assert!(
            update_context_desc.contains(phrase),
            "update_session_context description should mention {phrase}: {update_context_desc}"
        );
    }

    let session_shell_exec_desc = desc("session_shell_exec");
    for phrase in [
        "primary route",
        "same named ssh resource",
        "remote cwd/env/exports/functions/umask",
        "local persistent execution remains supported",
        "same local shell process must retain state",
        "ordinary one-shot work",
        "run_shell for shell semantics or short tightly related chains",
        "run_script for program-like shell content",
        "several commands alone are not a reason to open persistent shell",
    ] {
        assert!(
            session_shell_exec_desc.contains(phrase),
            "session_shell_exec description should mention {phrase}: {session_shell_exec_desc}"
        );
    }

    let persistent_exec = spec_named(&specs, "session_shell_exec");
    assert_eq!(
        persistent_exec.input_schema["properties"]["result_expectation"]["enum"],
        json!(["success", "failure", "observe"])
    );
    assert!(persistent_exec.input_schema["properties"]
        .get("accepted_exit_codes")
        .is_none());

    let resource_desc = spec_named(&specs, "update_session_context").input_schema["properties"]
        ["execution_context"]["properties"]["resource"]["description"]
        .as_str()
        .expect("update_session_context resource description")
        .to_lowercase();
    for phrase in [
        "logical name",
        "runner-owned resource",
        "open_session_shell",
    ] {
        assert!(
            resource_desc.contains(phrase),
            "execution_context.resource should mention {phrase}: {resource_desc}"
        );
    }
}

#[test]
fn removed_legacy_edit_tools_are_not_known_tools() {
    let specs = registered_tool_specs();
    let spec_names: std::collections::BTreeSet<&str> =
        specs.iter().map(|s| s.name.as_str()).collect();
    for removed in [
        "replace_exact_block",
        "insert_before_pattern",
        "insert_after_pattern",
        "replace_in_file",
        "replace_line_range",
        "insert_at_line",
        "delete_line_range",
        "apply_patch_checked",
        "validate_patch",
    ] {
        assert!(
            !is_known_tool_name(removed),
            "{removed} must no longer be a known tool definition"
        );
        assert!(
            !is_model_hidden_tool_name(removed),
            "{removed} must not be a hidden ToolDefinition"
        );
        assert!(
            !spec_names.contains(removed),
            "{removed} must not keep a model-facing ToolSpec"
        );
    }
}

#[test]
fn model_preference_upper_bounds_are_clamped_by_runtime_not_rejected_by_schema() {
    let specs = registered_tool_specs();
    let cases: &[(&str, &[&str])] = &[
        ("run_process", &["timeout_secs"]),
        ("run_detached_process", &["timeout_secs"]),
        ("run_script", &["timeout_secs"]),
        ("run_shell", &["timeout_secs"]),
        ("session_shell_exec", &["timeout_secs"]),
        ("observe_jobs", &["tail_lines", "wait_secs"]),
        ("list_jobs", &["limit"]),
        ("cargo_fmt", &["timeout_secs"]),
        ("cargo_check", &["timeout_secs"]),
        ("cargo_test", &["timeout_secs"]),
        ("go_test", &["timeout_secs"]),
        ("session_discussion_summary", &["limit"]),
        ("workspace_hygiene_check", &["max_findings"]),
        ("list_projects", &["limit"]),
        ("list_session_messages", &["limit"]),
        ("observe_session_messages", &["wait_secs", "limit"]),
        ("validation_summary", &["limit"]),
        ("session_handoff_summary", &["limit"]),
        ("document_symbols", &["limit"]),
        ("document_diagnostics", &["limit"]),
        ("workspace_symbols", &["limit"]),
        ("goto_definition", &["limit"]),
        ("find_references", &["limit"]),
        ("call_hierarchy", &["limit"]),
        ("coding_agent_observe", &["wait_secs"]),
        ("list_agent_tasks", &["limit"]),
        ("list_agent_identities", &["limit"]),
        ("list_conversations", &["limit"]),
        ("read_conversation", &["limit"]),
        ("list_agent_inbox", &["limit"]),
    ];

    for (tool_name, fields) in cases {
        let spec = spec_named(&specs, tool_name);
        for field in *fields {
            let property = &spec.input_schema["properties"][*field];
            assert!(
                property.get("maximum").is_none(),
                "{tool_name}.{field} must let the runtime clamp oversized preferences: {property}"
            );
            let description = property["description"]
                .as_str()
                .unwrap_or_default()
                .to_ascii_lowercase();
            assert!(
                description.contains("clamp"),
                "{tool_name}.{field} should document runtime clamping: {description}"
            );
        }
    }
}

#[test]
fn call_hierarchy_schema_keeps_traversal_strict_and_result_budget_clamped() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "call_hierarchy");
    let properties = spec.input_schema["properties"].as_object().unwrap();

    assert_eq!(properties["depth"]["minimum"], 1);
    assert_eq!(properties["depth"]["maximum"], 2);
    assert_eq!(properties["depth"]["default"], 1);

    assert_eq!(properties["limit"]["minimum"], 1);
    assert!(properties["limit"].get("maximum").is_none());
    assert_eq!(properties["limit"]["default"], 50);
    let description = properties["limit"]["description"]
        .as_str()
        .unwrap_or_default();
    assert!(description.contains("above 100"), "{description}");
    assert!(description.contains("clamped to 100"), "{description}");
}

#[test]
fn edit_tool_surface_keeps_mutation_options_visible_and_schemas_stable() {
    let specs = registered_tool_specs();
    let names: std::collections::BTreeSet<&str> =
        specs.iter().map(|spec| spec.name.as_str()).collect();

    for required in [
        "apply_text_edits",
        "apply_patch",
        "apply_unified_diff",
        "write_project_file",
    ] {
        assert!(
            names.contains(required),
            "edit surface must keep {required} model-visible"
        );
        let spec = spec_named(&specs, required);
        assert!(
            spec.input_schema.is_object(),
            "{required} must keep an object input schema"
        );
        assert!(
            !spec.input_schema.as_object().unwrap().is_empty(),
            "{required} input schema must not be empty"
        );
    }

    // Parameter surface smoke checks (names only; not full-schema snapshots).
    let text_edits = &spec_named(&specs, "apply_text_edits").input_schema["properties"];
    for field in ["project", "changes", "dry_run"] {
        assert!(
            text_edits.get(field).is_some(),
            "apply_text_edits must keep field {field}"
        );
    }
    let text_edit_output =
        &spec_named(&specs, "apply_text_edits").output_schema["properties"]["output"]["properties"];
    let text_edit_file_properties = text_edit_output["files"]["items"]["properties"]
        .as_object()
        .expect("apply_text_edits file summary properties");
    assert!(text_edit_file_properties.contains_key("read_revision"));
    assert!(!text_edit_file_properties.contains_key("old_sha256"));
    assert!(!text_edit_file_properties.contains_key("new_sha256"));
    let codex_patch = &spec_named(&specs, "apply_patch").input_schema["properties"];
    for field in ["project", "patch", "dry_run", "matching_mode"] {
        assert!(
            codex_patch.get(field).is_some(),
            "apply_patch must keep field {field}"
        );
    }
    assert_eq!(codex_patch["matching_mode"]["default"], "unique");
    assert_eq!(
        codex_patch["matching_mode"]["enum"],
        json!(["first_match", "unique", "exact_unique"])
    );
    let matching_mode_desc = codex_patch["matching_mode"]["description"]
        .as_str()
        .expect("matching_mode description")
        .to_lowercase();
    assert!(matching_mode_desc.contains("unique (default)"));
    assert!(matching_mode_desc.contains("exact_unique"));
    assert!(matching_mode_desc.contains("stale-context/concurrency fence"));
    assert!(matching_mode_desc
        .contains("first_match is only for explicitly requested permissive compatibility"));
    assert!(
        codex_patch.get("strict_matching").is_none(),
        "legacy strict_matching must not remain model-facing"
    );
    let patch_spec = spec_named(&specs, "apply_patch");
    assert!(patch_spec.description.contains("naturally patch-shaped"));
    assert!(patch_spec
        .description
        .contains("preserve unique/exact_unique"));
    assert!(patch_spec.description.contains("not the default recovery"));
    let patch_output = &patch_spec.output_schema["properties"]["output"]["properties"];
    assert!(
        patch_output.get("match_diagnostic").is_some(),
        "apply_patch failures must expose body-free match diagnostics"
    );
    let match_rejection = patch_output
        .get("match_rejection_diagnostic")
        .expect("apply_patch matching failures must expose validated body-free diagnostics");
    assert_eq!(match_rejection["additionalProperties"], false);
    assert_eq!(
        match_rejection["properties"]["classification"]["enum"],
        json!(["unique_fuzzy_candidate", "ambiguous_candidate"])
    );
    assert_eq!(
        match_rejection["properties"]["matched_start_line"]["anyOf"][1]["type"],
        "null"
    );
    assert_eq!(
        match_rejection["properties"]["candidate_start_lines"]["maxItems"],
        webcodex_core::apply_patch_shared::MAX_CODEX_PATCH_CANDIDATE_POSITIONS
    );
    let recovery = patch_output
        .get("recovery")
        .expect("apply_patch must publish bounded reread recovery");
    assert_eq!(recovery["additionalProperties"], false);
    assert_eq!(
        recovery["properties"]["action"]["enum"],
        json!(["read_files"])
    );
    assert_eq!(
        recovery["properties"]["reason"]["enum"],
        json!([
            "context_mismatch",
            "matching_mode_rejected_unique_fuzzy",
            "matching_mode_rejected_ambiguous"
        ])
    );
    assert_eq!(
        recovery["properties"]["items"]["maxItems"],
        webcodex_core::apply_patch_shared::MAX_CODEX_PATCH_CANDIDATE_POSITIONS
    );
    assert_eq!(
        recovery["properties"]["items"]["items"]["properties"]["limit"]["maximum"],
        webcodex_core::apply_patch_shared::MAX_CODEX_PATCH_RECOVERY_READ_LINES
    );
    let patch_files = &patch_output["files"];
    assert_eq!(patch_files["type"], "array");
    let file_properties = patch_files["items"]["properties"]
        .as_object()
        .expect("apply_patch file summary properties");
    for field in [
        "index",
        "kind",
        "path",
        "to_path",
        "old_sha256",
        "new_sha256",
        "changed",
        "would_change",
        "edits",
    ] {
        assert!(
            file_properties.contains_key(field),
            "apply_patch file summary must expose {field}"
        );
    }
    let edit_properties = file_properties["edits"]["items"]["properties"]
        .as_object()
        .expect("apply_patch edit summary properties");
    for field in [
        "chunk_index",
        "change_context_present",
        "old_line_count",
        "new_line_count",
        "end_of_file",
        "match_mode",
        "match_source",
        "matched_start_line",
        "candidate_count",
        "unique_match",
        "strict_match",
    ] {
        assert!(
            edit_properties.contains_key(field),
            "apply_patch edit summary must expose {field}"
        );
    }
    assert!(patch_spec.description.contains("Transactional"));
    assert!(patch_spec.description.contains("matching_mode_rejected"));
    assert!(patch_spec.description.contains("weakening the guard"));
    let unified_diff = &spec_named(&specs, "apply_unified_diff").input_schema["properties"];
    for field in ["project", "diff", "deny_sensitive_paths"] {
        assert!(
            unified_diff.get(field).is_some(),
            "apply_unified_diff must keep field {field}"
        );
    }
    assert!(unified_diff.get("patch").is_none());
    let write_file = &spec_named(&specs, "write_project_file").input_schema["properties"];
    for field in ["project", "path", "content"] {
        assert!(
            write_file.get(field).is_some(),
            "write_project_file must keep field {field}"
        );
    }
}

#[test]
fn session_tool_specs_describe_explicit_targeting() {
    let specs = registered_tool_specs();

    let desc = |name: &str| spec_named(&specs, name).description.to_lowercase();

    let summary_desc = desc("session_summary");
    for phrase in ["session ledger", "explicit session_id"] {
        assert!(
            summary_desc.contains(phrase),
            "session_summary description should mention {phrase}: {summary_desc}"
        );
    }

    let update = spec_named(&specs, "update_session_context");
    assert_eq!(
        update.input_schema["required"],
        json!(["project", "session_id", "execution_context"])
    );
    assert_eq!(update.input_schema["additionalProperties"], false);
    assert_eq!(
        update.input_schema["properties"]["execution_context"]["additionalProperties"],
        false
    );
    assert!(
        update.input_schema["properties"]["execution_context"]["properties"]
            .get("resource")
            .is_some(),
        "update_session_context must expose the named SSH resource field"
    );

    let work = spec_named(&specs, "work_on_project");
    let session_id_description = work.input_schema["properties"]["session_id"]["description"]
        .as_str()
        .expect("work_on_project session_id description")
        .to_lowercase();
    for phrase in ["does not prove", "fresh model context", "context_request"] {
        assert!(
            session_id_description.contains(phrase),
            "work_on_project session_id description should mention {phrase}: {session_id_description}"
        );
    }
    let update_desc = update.description.to_lowercase();
    for phrase in [
        "authorized project",
        "exact session project",
        "cross-project escape is not supported",
        "store lock",
        "background writer",
        "success does not mean",
        "never falls back",
        "never creates",
    ] {
        assert!(
            update_desc.contains(phrase),
            "update_session_context description should mention {phrase}: {update_desc}"
        );
    }

    let handoff_desc = desc("session_handoff_summary");
    for phrase in [
        "exact session_id",
        "handoff_brief",
        "8 kib",
        "diagnostic=true",
        "basis incomplete",
    ] {
        assert!(
            handoff_desc.contains(phrase),
            "session_handoff_summary description should mention {phrase}: {handoff_desc}"
        );
    }

    let names: Vec<&str> = specs.iter().map(|spec| spec.name.as_str()).collect();
    for removed in [
        "bind_current_session",
        "current_session",
        "unbind_current_session",
    ] {
        assert!(
            !names.contains(&removed),
            "removed Session tool leaked into specs: {removed}"
        );
    }
}

#[test]
fn observe_jobs_wake_policy_schema_is_closed_and_compatible() {
    let specs = registered_tool_specs();
    let spec = specs
        .iter()
        .find(|spec| spec.name == "observe_jobs")
        .unwrap();
    let wake = &spec.input_schema["properties"]["wake_on"];
    assert_eq!(
        webcodex_core::runtime_contract::MAX_JOB_OBSERVATION_WAIT_SECS,
        100
    );
    assert_eq!(
        wake["enum"],
        serde_json::json!(["change", "terminal", "all_terminal"])
    );
    assert_eq!(wake["default"], "change");
    assert!(!spec.input_schema["required"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("wake_on")));
    for phrase in [
        "No token",
        "no wait_secs",
        "wake_on=change",
        "wake_on=terminal",
        "useful progress is blocked on terminal outcome",
        "independent work remains",
        "do not poll for visibility",
        "changed=true",
    ] {
        assert!(spec.description.contains(phrase), "missing {phrase}");
    }
    let wait_description = spec.input_schema["properties"]["wait_secs"]["description"]
        .as_str()
        .unwrap();
    assert!(wait_description.contains("above 100 seconds"));
    assert!(wait_description.contains("clamped to 100"));
    assert!(wait_description.contains("MCP transport may clamp"));
    assert!(wait_description.contains("further useful progress depends on terminal outcome"));
    assert!(wait_description.contains("independent work continues"));
    let wake_description = wake["description"].as_str().unwrap();
    assert!(wake_description.contains("any terminal result unblocks progress"));
    assert!(wake_description.contains("predetermined set"));
    for policy in ["change", "terminal", "all_terminal"] {
        test_support::validate_schema_instance(
            &json!({"items": [{"job_id": "job"}], "wake_on": policy}),
            &spec.input_schema,
        )
        .unwrap();
    }
}
