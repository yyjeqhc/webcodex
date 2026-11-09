use super::*;

fn assert_description_phrases(
    context: &str,
    description: &str,
    required: &[&str],
    forbidden: &[&str],
) {
    let description = description.to_lowercase();
    for phrase in required {
        assert!(
            description.contains(phrase),
            "{context}: missing required phrase {phrase:?} in {description:?}"
        );
    }
    for phrase in forbidden {
        assert!(
            !description.contains(phrase),
            "{context}: forbidden phrase {phrase:?} in {description:?}"
        );
    }
}

fn assert_tool_descriptions(specs: &[ToolSpec], cases: &[(&[&str], &[&str], &[&str])]) {
    for &(names, required, forbidden) in cases {
        for name in names {
            assert_description_phrases(
                name,
                &spec_named(specs, name).description,
                required,
                forbidden,
            );
        }
    }
}

#[test]
fn tool_specs_describe_default_coding_loop_preferences() {
    let specs = registered_tool_specs();
    let specialists = exact_manifest_specialist_tool_specs();

    // Keep safety-copy expectations at the authority source. Surface tests
    // verify they reuse this contract instead of freezing another prose version.
    // Rows contain tool names, required phrases, and forbidden phrases.
    assert_tool_descriptions(
        &specs,
        &[
            (
                &["work_on_project"],
                &[
                    "session_id",
                    "reauthorizes",
                    "must match",
                    "project authority",
                    "uncertainty",
                    "never replay effects",
                ],
                &[],
            ),
            (
                &["read_files"],
                &[
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
                ],
                &[
                    "adaptive runtime preferred batch-capable inspect tool",
                    "including when only one known range is needed",
                ],
            ),
            (
                &["search_project_texts"],
                &[
                    "batch-capable project-text search",
                    "bounded structured results",
                    "protected-path policy",
                    "isolated failures",
                    "broad discovery",
                    "files_with_matches/count",
                    "matched source will be read immediately",
                    "search_file_context",
                    "small known-scope search",
                    "native rg is first-class",
                    "batch only independent queries",
                    "result-dependent follow-ups sequential",
                    "pattern_mode=literal",
                    "returned suggested_call",
                    "whole-query continuation",
                    "truncated individual queries must be narrowed",
                ],
                &[
                    "adaptive runtime preferred batch-capable project-text search",
                    "including when only one query is needed",
                ],
            ),
            (
                &["search_file_context"],
                &[
                    "1..8 predetermined independent project-text queries",
                    "provide queries",
                    "max_reads is one global read budget",
                    "shared fairly across the batch",
                    "preserving per-query batch failures",
                    "batch only independent queries",
                    "result-dependent follow-ups sequential",
                ],
                &["run one bounded project-text search"],
            ),
            (
                &["save_project_artifact"],
                &[
                    "already holds the bounded binary/base64 content",
                    "do not read a current chatgpt/host attachment",
                    "import_host_files",
                ],
                &[],
            ),
            (
                &["import_host_files"],
                &[
                    "preferred host-native attachment-to-project transfer path",
                    "do not base64-transfer files",
                    "active authenticated oauth client",
                    "openai file hosts",
                    "arbitrary public https",
                    "up to 256 mib per file",
                    "batch is not atomic",
                    "partial_success=true",
                ],
                &[],
            ),
            (
                &["transfer_project_artifact"],
                &[
                    "source project:read",
                    "destination project:write",
                    "independently resolved and authorized",
                    "exact source bytes/sha-256/mime snapshot",
                    "do not pass through host attachments or model text",
                ],
                &[],
            ),
            (
                &["inspect_project_artifact"],
                &[
                    "metadata=facts",
                    "inspect=fenced segment",
                    "image=mcp image",
                    "export=mcp resourcelink",
                    "not repeated inspect calls",
                    "import_host_files",
                ],
                &[],
            ),
            (
                &["read_project_artifact_chunk"],
                &[
                    "bounded chunk inspection api",
                    "parser-ready suggested_call",
                    "expected_sha256",
                    "snapshot_changed",
                    "do not manually translate",
                    "do not loop over base64 chunks",
                    "inspect_project_artifact(action=export)",
                ],
                &[],
            ),
            (
                &["begin_artifact_upload"],
                &[
                    "low-level chunked binary artifact upload",
                    "not the preferred path for a current chatgpt/host attachment",
                    "import_host_files",
                ],
                &[],
            ),
            (
                &["read_git_log"],
                &["next_skip", "parser-ready", "10000 skip bound"],
                &[],
            ),
            (
                &["list_project_files"],
                &[
                    "live project directory",
                    "next_call",
                    "offsets are not snapshots",
                    "older runners",
                    "fail closed",
                ],
                &[],
            ),
            (
                &["list_project_tracked_files"],
                &[
                    "bounded producer source",
                    "source acquisition is complete",
                    "list_truncated=true",
                    "next_offset is null",
                    "narrow path",
                    "retained-tail source truncation fails closed",
                ],
                &[],
            ),
            (
                &["read_workspace_changes"],
                &[
                    "workspace summary",
                    "staged/unstaged/untracked",
                    "without a patch by default",
                    "not a complete staged patch",
                    "ordinary code review uses review_changes",
                ],
                &["default inspect/review tool before final response"],
            ),
            (
                &["read_git_review_summary"],
                &[
                    "exact committed git range",
                    "without returning patch bodies",
                    "incomplete coverage",
                    "ordinary review uses review_changes",
                    "read-only",
                ],
                &[],
            ),
            (
                &["read_git_diff_hunks"],
                &[
                    "bounded git diff hunks",
                    "cached=true selects staged changes",
                    "omission selects unstaged changes",
                    "base_commit+head_commit",
                    "use review_changes",
                    "source fences",
                    "bounded page/hunk continuation",
                    "safe recovery",
                ],
                &[],
            ),
            (
                &["edit_project_files"],
                &[
                    "project editor",
                    "read_files",
                    "expected_read_revision",
                    "exact edits fail closed on ambiguity",
                    "replace_range",
                    "from that snapshot",
                    "preflight transactionally",
                    "runner rechecks source",
                    "stale state returns read_files recovery",
                    "outcome_unknown",
                ],
                &[
                    "canonical default guarded edit path",
                    "expected_sha256",
                    "prefer apply_patch",
                ],
            ),
            (
                &["cargo_check", "cargo_test"],
                &[
                    "structured",
                    "common supported",
                    "validation identity",
                    "same execution job handoff",
                ],
                &["preferred structured", "run_shell"],
            ),
            (
                &["cargo_test"],
                &[
                    "executed-test evidence",
                    "min_tests/require_tests",
                    "bounded output",
                ],
                &[],
            ),
            (
                &["go_test"],
                &[
                    "structured option for common supported",
                    "go json test-count evidence",
                ],
                &["preferred structured"],
            ),
            (
                &["cargo_fmt"],
                &[
                    "final formatting after relevant rust source stabilizes",
                    "do not use cargo_fmt as a per-edit ritual",
                    "read-only formatting validation",
                    "precheck",
                    "changed/state_changed",
                ],
                &[],
            ),
            (
                &["check_workspace_hygiene"],
                &["pre-final", "workspace hygiene", "read-only"],
                &[],
            ),
            (
                &["read_session_handoff"],
                &["handoff", "missing task context", "read-only"],
                &[],
            ),
            (
                &["run_shell"],
                &[
                    "shell grammar or a short related command chain",
                    "run_process takes literal argv",
                    "run_script program text",
                    "local jobs are runner-owned",
                    "named session ssh has no durable job handoff",
                    "observe_jobs for job recovery",
                    "never redispatch pending or outcome_unknown",
                    "inspect existing execution",
                    "structured source editors",
                    "runner replacement cannot inherit local jobs",
                ],
                &[],
            ),
            // Cover the selection and lifecycle choices, not the retired full prose.
            // Interactive mode shares the tool but has a deliberately explicit contract.
            (
                &["run_process"],
                &[
                    "native executable literal argv",
                    "run_shell takes shell grammar",
                    "runner ownership",
                    "interactive=true",
                    "stdin pipe",
                    "keyed write_job_input",
                    "no pty/ssh/initial stdin/structured validation evidence",
                    "retain returned job/continuation",
                    "never redispatch pending or outcome_unknown",
                    "inspect existing execution",
                    "runner replacement cannot inherit",
                ],
                &[],
            ),
            // Interpreter/runtime details stay locked by run_script input-schema tests;
            // this description test keeps selection and lifecycle decisions dense.
            (
                &["run_script"],
                &[
                    "sh/bash/powershell/python/javascript/typescript script data",
                    "run_process takes literal argv",
                    "run_shell takes shell grammar",
                    "structured source editors",
                    "never interpolated into shell command text",
                    "retain returned job/continuation",
                    "never redispatch pending or outcome_unknown",
                    "runner-owned; replacement cannot inherit",
                ],
                &[],
            ),
            (
                &["run_job"],
                &[
                    "runner-owned",
                    "server reconnects reconcile",
                    "runner replacement does not inherit",
                    "outlive the runner",
                    "run_detached_process",
                ],
                &[],
            ),
            (
                &["run_detached_process"],
                &[
                    "supervisor-owned",
                    "survive runner replacement",
                    "ownership transfers before payload start",
                    "supervisor identity and lifetime fence",
                    "expired keys never authorize retry",
                    "duration alone is insufficient",
                    "outcome_unknown never authorizes redispatch",
                ],
                &[],
            ),
            (
                &["open_session_shell"],
                &[
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
                    "manage_ssh_resource",
                    "runner restart",
                ],
                &[],
            ),
            (
                &["update_session_context"],
                &[
                    "active runner-local named ssh resource",
                    "open_session_shell",
                    "manage_ssh_resource",
                    "restart the runner",
                ],
                &[],
            ),
            (
                &["execute_session_shell"],
                &[
                    "primary route",
                    "same named ssh resource",
                    "remote cwd/env/exports/functions/umask",
                    "local persistent execution remains supported",
                    "same local shell process must retain state",
                    "ordinary one-shot work",
                    "run_shell for shell semantics or short tightly related chains",
                    "run_script for program-like shell content",
                    "several commands alone are not a reason to open persistent shell",
                ],
                &[],
            ),
        ],
    );

    // Specialist descriptions come from their exact-manifest surface.
    // Patch input remains available without becoming the default recovery path.
    assert_tool_descriptions(
        &specialists,
        &[
            (
                &["apply_patch"],
                &[
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
                ],
                &["prefer apply_patch"],
            ),
            (
                &["apply_unified_diff"],
                &[
                    "external raw unified-diff mutation path",
                    "input is already a standard unified diff",
                    "clearest reliable mutation",
                    "bounded preflight",
                    "never needs a separate validation call",
                    "rather than converting model-generated work into unified diff by default",
                ],
                &[],
            ),
            (
                &["write_project_file"],
                &[
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
                ],
                &["expected_sha256", "prefer apply_text_edits"],
            ),
        ],
    );
}

#[test]
fn git_diff_hunks_paging_schema_preserves_budget_and_cursor_contract() {
    let specs = registered_tool_specs();
    let git_diff_hunks = spec_named(&specs, "read_git_diff_hunks");
    let default_page_bytes = git_diff_hunks.input_schema["properties"]["max_page_bytes"]["default"]
        .as_u64()
        .expect("read_git_diff_hunks max_page_bytes default");
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
        .expect("read_git_diff_hunks continuation description");
    assert_description_phrases(
        "read_git_diff_hunks input_schema/properties/continuation/description",
        continuation_desc,
        &[
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
        ],
        &[],
    );
}

#[test]
fn session_shell_schema_preserves_result_expectations_and_resource_selection() {
    let specs = registered_tool_specs();
    let persistent_exec = spec_named(&specs, "execute_session_shell");
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
        .expect("update_session_context resource description");
    assert_description_phrases(
        "update_session_context input_schema/properties/execution_context/properties/resource/description",
        resource_desc,
        &["logical name", "runner-owned resource", "open_session_shell"],
        &[],
    );
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
        ("execute_session_shell", &["timeout_secs"]),
        ("observe_jobs", &["tail_lines", "wait_secs"]),
        ("list_jobs", &["limit"]),
        ("cargo_fmt", &["timeout_secs"]),
        ("cargo_check", &["timeout_secs"]),
        ("cargo_test", &["timeout_secs"]),
        ("go_test", &["timeout_secs"]),
        ("read_session_discussion_summary", &["limit"]),
        ("check_workspace_hygiene", &["max_findings"]),
        ("list_projects", &["limit"]),
        ("list_session_messages", &["limit"]),
        ("observe_session_messages", &["wait_secs", "limit"]),
        ("read_validation_summary", &["limit"]),
        ("read_session_handoff", &["limit"]),
        ("list_document_symbols", &["limit"]),
        ("read_document_diagnostics", &["limit"]),
        ("list_workspace_symbols", &["limit"]),
        ("find_definition", &["limit"]),
        ("find_references", &["limit"]),
        ("read_call_hierarchy", &["limit"]),
        ("observe_coding_agent", &["wait_secs"]),
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
    let spec = spec_named(&specs, "read_call_hierarchy");
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
fn edit_tool_surface_keeps_one_primary_editor_and_exact_specialists_stable() {
    let specs = registered_tool_specs();
    let specialist_specs = exact_manifest_specialist_tool_specs();
    let names: std::collections::BTreeSet<&str> =
        specs.iter().map(|spec| spec.name.as_str()).collect();

    assert!(names.contains("edit_project_files"));
    for hidden in EXACT_MANIFEST_SPECIALIST_TOOL_NAMES {
        assert!(!names.contains(hidden), "{hidden} must stay model-hidden");
    }
    for required in EXACT_MANIFEST_SPECIALIST_TOOL_NAMES {
        assert!(
            specialist_specs.iter().any(|spec| spec.name == *required),
            "exact-manifest surface must keep {required}"
        );
        let spec = spec_named(&specialist_specs, required);
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
    let text_edits = &spec_named(&specs, "edit_project_files").input_schema["properties"];
    for field in ["project", "changes", "dry_run"] {
        assert!(
            text_edits.get(field).is_some(),
            "apply_text_edits must keep field {field}"
        );
    }
    let text_edit_output = &spec_named(&specs, "edit_project_files").output_schema["properties"]
        ["output"]["properties"];
    let text_edit_file_properties = text_edit_output["files"]["items"]["properties"]
        .as_object()
        .expect("apply_text_edits file summary properties");
    assert!(text_edit_file_properties.contains_key("read_revision"));
    assert!(!text_edit_file_properties.contains_key("old_sha256"));
    assert!(!text_edit_file_properties.contains_key("new_sha256"));
    let codex_patch = &spec_named(&specialist_specs, "apply_patch").input_schema["properties"];
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
    let patch_spec = spec_named(&specialist_specs, "apply_patch");
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
    let unified_diff =
        &spec_named(&specialist_specs, "apply_unified_diff").input_schema["properties"];
    for field in ["project", "diff", "deny_sensitive_paths"] {
        assert!(
            unified_diff.get(field).is_some(),
            "apply_unified_diff must keep field {field}"
        );
    }
    assert!(unified_diff.get("patch").is_none());
    let write_file =
        &spec_named(&specialist_specs, "write_project_file").input_schema["properties"];
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

    let summary_desc = desc("read_session_summary");
    for phrase in ["session ledger", "explicit session_id"] {
        assert!(
            summary_desc.contains(phrase),
            "read_session_summary description should mention {phrase}: {summary_desc}"
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
    assert!(session_id_description.contains("does not prove"));
    assert!(session_id_description.contains("does not select a recorder"));
    let work_schema_text = serde_json::to_string(&work.input_schema).unwrap();
    for legacy in ["context_request", "recording_session_id"] {
        assert!(
            !work_schema_text.contains(legacy),
            "work_on_project model-facing schema prose leaked legacy wrapper {legacy}: {work_schema_text}"
        );
    }
    assert!(!work.description.contains("context_request"));
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

    let handoff_desc = desc("read_session_handoff");
    for phrase in [
        "exact session_id",
        "handoff_brief",
        "8 kib",
        "diagnostic=true",
        "basis incomplete",
    ] {
        assert!(
            handoff_desc.contains(phrase),
            "read_session_handoff description should mention {phrase}: {handoff_desc}"
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
        serde_json::json!(["change", "meaningful_change", "terminal", "all_terminal"])
    );
    assert_eq!(wake["default"], "terminal");
    assert!(!spec.input_schema["required"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("wake_on")));
    for phrase in [
        "without a token",
        "wait_secs defaults to 0",
        "change/meaningful_change need after_observation_token",
        "Terminal is not success",
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
    for boundary in [
        "terminal (default)",
        "meaningful_change",
        "sequence-only heartbeat",
        "terminal waits for any Job",
        "all_terminal waits for every",
        "changed=true",
        "No token is required for terminal/all_terminal",
        "No wait_secs means immediate observation",
    ] {
        assert!(wake_description.contains(boundary), "missing {boundary}");
    }
    for policy in ["change", "meaningful_change", "terminal", "all_terminal"] {
        test_support::validate_schema_instance(
            &json!({"items": [{"job_id": "job"}], "wake_on": policy}),
            &spec.input_schema,
        )
        .unwrap();
    }
}

#[test]
fn extension_descriptions_distinguish_catalog_body_and_binding_reuse() {
    let specs = registered_tool_specs();
    let skill = &spec_named(&specs, "load_skill").description;
    for phrase in [
        "same Project",
        "new task/Session alone",
        "changed revisions",
        "missing/truncated body",
        "context loss/uncertainty",
        "Catalog metadata is not the Skill body",
        "Guidance grants no authority",
    ] {
        assert!(skill.contains(phrase), "load_skill missing {phrase}");
    }
    let plugin = &spec_named(&specs, "plugin_tool").description;
    for phrase in [
        "selection metadata",
        "not an input schema or binding",
        "reuse both while retained",
        "exact Runner/provider/tool and bound Project",
        "another call/task/Session",
        "Never reuse a projectBound binding for another Project",
        "do not just drop a call-time project",
        "not permission to replay an effect",
        "Unknown outcomes still require reconciliation",
    ] {
        assert!(plugin.contains(phrase), "plugin_tool missing {phrase}");
    }
}
