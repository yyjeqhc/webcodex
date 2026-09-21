use super::*;
use std::collections::BTreeSet;

fn registered_tool_categories() -> Value {
    Value::Object(
        TOOL_DISCOVERY_GROUPS
            .iter()
            .map(|group| {
                (
                    group.name.to_string(),
                    Value::Array(
                        group
                            .tools
                            .iter()
                            .map(|tool| Value::String((*tool).to_string()))
                            .collect(),
                    ),
                )
            })
            .collect(),
    )
}

fn recommended_flows() -> Vec<&'static str> {
    TOOL_RECOMMENDED_FLOWS
        .iter()
        .map(|flow| flow.summary)
        .collect()
}

#[test]
fn model_tool_contracts_do_not_teach_retired_runner_as_agent_prose() {
    for spec in registered_tool_specs() {
        let rendered = format!(
            "{}\n{}\n{}",
            spec.description, spec.input_schema, spec.output_schema
        )
        .to_ascii_lowercase();
        for retired in [
            "agent-registered",
            "agent registry",
            "owning registered agent",
            "agent project config",
            "an runner",
        ] {
            assert!(
                !rendered.contains(retired),
                "{} still exposes retired Runner-as-Agent prose: {retired}",
                spec.name
            );
        }
    }
}

#[test]
fn list_tools_schema_exposes_bounded_discovery_fields() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "list_tools");
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert_schema_fields!(
        props,
        "list_tools input schema",
        present: ["category", "features", "summary_only", "limit"]
    );
    assert!(spec.input_schema["required"].as_array().unwrap().is_empty());
    let output = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    assert_schema_fields!(
        output,
        "list_tools output schema",
        present: [
            "category", "features", "limit", "returned_count", "total_count",
            "filtered_count", "limit_applied", "requested_limit", "truncation_reason",
            "truncated", "categories", "recommended_flows",
        ]
    );
}

#[test]
fn tool_manifest_schema_exposes_compact_discovery_fields() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "tool_manifest");
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert_schema_fields!(
        props,
        "tool_manifest input schema",
        present: ["category", "intent", "include_recommended_flows", "include_risk_summary"]
    );
    let flow_description = props["include_recommended_flows"]["description"]
        .as_str()
        .expect("include_recommended_flows description");
    assert!(flow_description.contains("exact tool_name"));
    assert!(flow_description.contains("false"));
    assert!(flow_description.contains("category"));
    assert!(flow_description.contains("intent"));
    assert!(flow_description.contains("true"));
    let risk_summary_description = props["include_risk_summary"]["description"]
        .as_str()
        .expect("include_risk_summary description");
    assert!(risk_summary_description.contains("where the selected projection exposes it"));
    assert!(risk_summary_description.contains("Unfiltered/full discovery can return the aggregate"));
    assert!(risk_summary_description.contains("sparse filtered discovery omits it"));
    assert!(risk_summary_description
        .contains("does not change authority, permission, or tool behavior"));
    assert!(!risk_summary_description.contains("Include risk_summary in the output"));
    let output = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    assert_schema_fields!(
        output,
        "tool_manifest output schema",
        present: [
            "schema_version", "count", "tool_count", "filtered_count", "category", "intent",
            "available_intents", "filtered", "categories_requested", "limit", "returned_count",
            "total_count", "limit_applied", "requested_limit", "truncation_reason", "truncated",
            "categories", "tools", "risk_summary", "recommended_flows",
        ]
    );
}

#[test]
fn tool_recommended_flows_reference_visible_defined_tools() {
    let expected_summaries = TOOL_RECOMMENDED_FLOWS
        .iter()
        .map(|flow| {
            assert!(!flow.name.trim().is_empty());
            assert!(!flow.manifest_purpose.trim().is_empty(), "{}", flow.name);
            assert!(flow.summary.chars().count() <= 300, "{}", flow.name);
            assert!(!flow.tools.is_empty(), "{}", flow.name);
            for tool in flow.tools {
                let definition = lookup_tool_definition(tool)
                    .unwrap_or_else(|| panic!("{} references unknown tool {tool}", flow.name));
                assert!(
                    definition.visibility.is_model_visible(),
                    "{}: {tool}",
                    flow.name
                );
                assert!(is_model_visible_tool_name(tool), "{}: {tool}", flow.name);
            }
            for native in ["run_process", "run_script", "run_shell"] {
                if flow.manifest_purpose.contains(native) {
                    assert!(
                        flow.tools.contains(&native),
                        "{} purpose recommends {native} but its machine-readable tools omit it",
                        flow.name
                    );
                }
            }
            flow.summary
        })
        .collect::<Vec<_>>();
    assert_eq!(recommended_flows(), expected_summaries);
}

#[test]
fn agent_continuation_setup_flow_is_focused_and_keeps_resume_tools_separate() {
    let flow = TOOL_RECOMMENDED_FLOWS
        .iter()
        .find(|flow| flow.name == "agent_continuation_setup")
        .expect("agent_continuation_setup recommended flow");
    assert_eq!(
        flow.tools,
        &[
            "create_agent_identity",
            "rotate_agent_continuation_endpoint",
            "present_agent_continuation",
            "list_agent_identities",
        ]
    );
    let guidance = format!("{}\n{}", flow.summary, flow.manifest_purpose).to_lowercase();
    for phrase in [
        "new durable agent window setup",
        "yield/end",
        "production_auto_resume_available",
        "presentation success is not host readiness",
    ] {
        assert!(
            guidance.contains(phrase),
            "setup flow should mention {phrase}: {guidance}"
        );
    }
    for resume_tool in ["bootstrap_agent_conversation", "consume_agent_wake"] {
        assert!(!flow.tools.contains(&resume_tool));
    }
}

#[test]
fn single_window_goal_workflow_prefers_atomic_admission_and_keeps_host_setup_separate() {
    let flow = TOOL_RECOMMENDED_FLOWS
        .iter()
        .find(|flow| flow.name == "single_window_goal_workflow")
        .expect("single_window_goal_workflow recommended flow");
    assert_eq!(
        flow.tools,
        &[
            "work_on_project",
            "get_goal",
            "prepare_goal_workflow",
            "present_goal_plan",
            "checkpoint_goal",
            "finish_coding_task",
            "update_goal",
        ]
    );
    for host_setup in [
        "create_agent_identity",
        "rotate_agent_continuation_endpoint",
        "present_agent_continuation",
        "list_agent_identities",
    ] {
        assert!(
            !flow.tools.contains(&host_setup),
            "ordinary Goal flow duplicated Host continuation setup: {host_setup}"
        );
    }
    let guidance = format!("{}\n{}", flow.summary, flow.manifest_purpose).to_lowercase();
    for phrase in [
        "goal_context",
        "reuse one exact candidate",
        "with multiple candidates, read candidate details through exact get_goal calls",
        "explicitly choose one before present_goal_plan",
        "get_goal",
        "prepare_goal_workflow",
        "durable admission only",
        "host carrier setup/readiness remains separate",
        "agent_continuation_setup",
        "low-level create_goal and associate_goal_workflow_session remain available",
    ] {
        assert!(
            guidance.contains(phrase),
            "single-window Goal flow should mention {phrase}: {guidance}"
        );
    }

    let categories = registered_tool_categories();
    let goal_tools = categories["goal"].as_array().unwrap();
    for low_level_or_composed in [
        "prepare_goal_workflow",
        "create_goal",
        "associate_goal_workflow_session",
    ] {
        assert!(
            goal_tools
                .iter()
                .any(|tool| tool.as_str() == Some(low_level_or_composed)),
            "Goal discovery lost {low_level_or_composed}"
        );
    }
}

#[test]
fn goal_agent_wait_orchestration_flow_registers_before_worker_execution_without_discovery() {
    let flow = TOOL_RECOMMENDED_FLOWS
        .iter()
        .find(|flow| flow.name == "goal_agent_wait_orchestration")
        .expect("goal_agent_wait_orchestration recommended flow");
    let associate = flow
        .tools
        .iter()
        .position(|tool| *tool == "associate_goal_agent_task")
        .unwrap();
    let wait = flow
        .tools
        .iter()
        .position(|tool| *tool == "wait_for_agent_events")
        .unwrap();
    let start = flow
        .tools
        .iter()
        .position(|tool| *tool == "start_agent_task_attempt")
        .unwrap();
    let dispatch = flow
        .tools
        .iter()
        .position(|tool| *tool == "start_agent_task_endpoint_continuation")
        .unwrap();
    let bootstrap = flow
        .tools
        .iter()
        .position(|tool| *tool == "bootstrap_agent_conversation")
        .unwrap();
    let consume = flow
        .tools
        .iter()
        .position(|tool| *tool == "consume_agent_wake")
        .unwrap();
    assert!(associate < wait && wait < start && start < dispatch);
    assert!(dispatch < bootstrap && bootstrap < consume);
    for required in [
        "start_agent_task_endpoint_continuation",
        "bootstrap_agent_conversation",
        "consume_agent_wake",
        "read_agent_wait",
        "get_goal",
        "read_agent_task",
        "update_goal",
    ] {
        assert!(flow.tools.contains(&required), "missing {required}");
    }
    let guidance = format!("{}\n{}", flow.summary, flow.manifest_purpose).to_lowercase();
    for phrase in [
        "before any selected worker can terminalize",
        "explicit 1..8 task selector list",
        "any for first-result continuation",
        "all for fan-in",
        "only after registration start each worker with start_agent_task_attempt followed by start_agent_task_endpoint_continuation",
        "fresh resumed coordinator turn bootstrap the exact wake",
        "consume it immediately",
        "never derive the wait source list from goal correlations",
        "not treat this flow as a scheduler",
        "explicitly decide/update goal state",
    ] {
        assert!(
            guidance.contains(phrase),
            "Goal AgentWait flow should mention {phrase}: {guidance}"
        );
    }
}

#[test]
fn edit_recommended_flow_selects_mutation_by_shape_without_weakening_guards() {
    let flow = TOOL_RECOMMENDED_FLOWS
        .iter()
        .find(|flow| flow.name == "edit")
        .expect("edit recommended flow");
    assert_eq!(flow.tools.first().copied(), Some("read_files"));
    assert_eq!(flow.tools.get(1).copied(), Some("apply_text_edits"));
    assert_eq!(flow.tools.get(2).copied(), Some("apply_patch"));
    let guidance = format!("{}\n{}", flow.summary, flow.manifest_purpose).to_lowercase();
    for phrase in [
        "edit by mutation shape",
        "apply_text_edits for small/local exact edits",
        "intentional whole-file replacement",
        "bounded deterministic programmatic transforms",
        "repetitive mechanical",
        "do not add a ritual read",
        "read_revision",
        "naturally contextual",
        "stable unique containing function/impl/type/test/module context",
        "matching_mode_rejected",
        "never weaken the guard or switch to first_match",
        "preserve unique/exact_unique",
        "context_mismatch requires bounded reread",
        "never blind retry",
    ] {
        assert!(
            guidance.contains(phrase),
            "edit flow should mention {phrase}: {guidance}"
        );
    }
    for obsolete in [
        "canonical default even when many lines change",
        "after read_files, apply_text_edits with current sha is the default",
    ] {
        assert!(
            !guidance.contains(obsolete),
            "obsolete edit ritual returned: {guidance}"
        );
    }
}

#[test]
fn execution_lifetime_flow_routes_runner_owned_and_supervisor_owned_work() {
    let flow = TOOL_RECOMMENDED_FLOWS
        .iter()
        .find(|flow| flow.name == "execution_lifetime")
        .expect("execution_lifetime recommended flow");
    assert_eq!(
        flow.tools,
        &[
            "run_process",
            "run_script",
            "run_shell",
            "run_job",
            "run_detached_process",
            "session_shell_exec",
            "observe_jobs",
            "stop_job",
        ]
    );
    let text = format!("{}\n{}", flow.summary, flow.manifest_purpose).to_ascii_lowercase();
    for phrase in [
        "runner-owned sync-first",
        "run_script",
        "supervisor-owned immediate async",
        "session_shell_exec",
        "duration alone is not a reason to detach",
    ] {
        assert!(
            text.contains(phrase),
            "execution_lifetime flow should mention {phrase}: {text}"
        );
    }
}

#[test]
fn tool_categories_and_recommended_flows_are_well_formed() {
    let categories = registered_tool_categories();
    let names = registered_tool_names();
    for (cat, members) in categories.as_object().unwrap() {
        let arr = members.as_array().unwrap();
        assert!(!arr.is_empty(), "category '{cat}' must not be empty");
        for member in arr {
            let name = member.as_str().unwrap();
            assert!(
                names.iter().any(|candidate| candidate == name),
                "{cat}: {name}"
            );
        }
    }
    for cat in [
        TOOL_DISCOVERY_GROUP_INSPECT,
        TOOL_DISCOVERY_GROUP_FILE_TRANSFER,
        TOOL_DISCOVERY_GROUP_GIT,
        TOOL_DISCOVERY_GROUP_REVIEW,
        TOOL_DISCOVERY_GROUP_VALIDATION,
        TOOL_DISCOVERY_GROUP_PATCH,
        TOOL_DISCOVERY_GROUP_SHELL,
        TOOL_DISCOVERY_GROUP_JOBS,
        TOOL_DISCOVERY_GROUP_RUNTIME,
        TOOL_DISCOVERY_GROUP_CLEANUP,
        #[cfg(feature = "workspace-checkpoints")]
        TOOL_DISCOVERY_GROUP_CHECKPOINT,
    ] {
        assert!(
            categories.as_object().unwrap().contains_key(cat),
            "missing category {cat}"
        );
    }
    let validation = categories[TOOL_DISCOVERY_GROUP_VALIDATION]
        .as_array()
        .unwrap();
    for name in ["cargo_fmt", "cargo_check", "cargo_test"] {
        assert!(validation.iter().any(|value| value == name));
    }
    let review = categories[TOOL_DISCOVERY_GROUP_REVIEW].as_array().unwrap();
    assert!(review.iter().any(|value| value == "git_diff_hunks"));
    assert!(review
        .iter()
        .any(|value| value == "workspace_hygiene_check"));
    assert!(review.iter().any(|value| value == "git_log"));
    let inspect = categories[TOOL_DISCOVERY_GROUP_INSPECT].as_array().unwrap();
    for name in [
        "read_files",
        "run_shell",
        "search_project_texts",
        "show_changes",
    ] {
        assert!(
            inspect.iter().any(|value| value == name),
            "inspect category: {name}"
        );
    }
    for compatibility_primitive in ["git_diff", "git_diff_summary"] {
        assert!(
            !inspect.iter().any(|value| value == compatibility_primitive),
            "inspect category should prefer canonical tools over {compatibility_primitive}"
        );
    }
    let git = categories[TOOL_DISCOVERY_GROUP_GIT].as_array().unwrap();
    for compatibility_primitive in ["git_diff", "git_diff_summary"] {
        assert!(
            !git.iter().any(|value| value == compatibility_primitive),
            "git category should not recommend {compatibility_primitive}"
        );
    }
    assert!(!review.iter().any(|value| value == "git_diff"));
    assert!(!review.iter().any(|value| value == "git_diff_summary"));
    let edit = categories[TOOL_DISCOVERY_GROUP_EDIT].as_array().unwrap();
    let edit_prefix = edit
        .iter()
        .take(5)
        .map(|value| value.as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        edit_prefix,
        vec![
            "apply_text_edits",
            "apply_patch",
            "apply_unified_diff",
            "write_project_file",
            "save_project_artifact"
        ]
    );
    let file_transfer = categories[TOOL_DISCOVERY_GROUP_FILE_TRANSFER]
        .as_array()
        .expect("file_transfer category present");
    for name in [
        "import_conversation_files_to_project",
        "transfer_project_artifact",
        "project_artifact",
        "save_project_artifact",
        "read_project_artifact",
        "artifact_upload_begin",
        "artifact_upload_chunk",
        "artifact_upload_finish",
        "artifact_upload_abort",
    ] {
        assert!(
            file_transfer.iter().any(|value| value == name),
            "file_transfer: {name}"
        );
    }
    assert!(edit
        .iter()
        .any(|value| value == "import_conversation_files_to_project"));
    let flows = recommended_flows();
    assert!(!flows.is_empty());
    for flow in &flows {
        assert!(flow.chars().count() <= 300, "flow too long: {flow}");
    }
    let joined_flows = flows.join("\n").to_lowercase();
    for phrase in [
        "if the user gives an exact runner client_id",
        "runtime_status/list_projects for that runner",
        "persistent shell: primarily reuse one shell for repeated commands on an active named ssh resource",
        "keep remote shell state",
        "ssh_resource list/register -> restart -> list -> bind -> open/reuse",
        "local persistent shell is only for true same-process state",
        "one-shot ssh uses run_process",
        "execution selection: run_process/run_script/run_shell and structured validation are runner-owned sync-first",
        "run_job is runner-owned immediate async",
        "run_detached_process is supervisor-owned immediate async",
        "session_shell_exec continues an existing session shell",
        "inspect: choose the simplest sufficient primitive",
        "native commands are first-class for small bounded observations",
        "search_project_texts/read_files when batching",
        "edit by mutation shape",
        "bounded deterministic transforms",
        "validate: use structured validators when their canonical diagnostics",
        "native execution is first-class when the command is outside or awkward",
        "file transfer: host -> import_conversation_files_to_project -> project",
        "project -> project_artifact -> host/model",
        "project a -> transfer_project_artifact -> project b",
        "inspect for one bounded segment",
        "export for complete resourcelink delivery",
        "copy show_changes.head.commit",
        "review: small bounded git observations may use native git",
        "git_review_summary to map broad or unknown committed ranges",
        "git_diff_hunks for fenced, paged, or continued review",
        "handoff/recovery only",
        "session_handoff_summary only for missing task context",
        "never routine progress polling",
    ] {
        assert!(
            joined_flows.contains(phrase),
            "recommended flows should mention {phrase}"
        );
    }
}

#[test]
fn recommended_flows_encode_simplest_sufficient_selection_without_old_rituals() {
    let flow = |name: &str| {
        TOOL_RECOMMENDED_FLOWS
            .iter()
            .find(|flow| flow.name == name)
            .unwrap_or_else(|| panic!("missing recommended flow {name}"))
    };

    let inspect = format!(
        "{}\n{}",
        flow("inspect").summary,
        flow("inspect").manifest_purpose
    )
    .to_lowercase();
    for phrase in [
        "simplest sufficient primitive",
        "native commands are first-class for small bounded observations",
        "batching",
        "read_revision",
        "snapshot continuation",
    ] {
        assert!(inspect.contains(phrase), "inspect selection: {phrase}");
    }

    let validate = format!(
        "{}\n{}",
        flow("validate").summary,
        flow("validate").manifest_purpose
    )
    .to_lowercase();
    for phrase in [
        "canonical diagnostics",
        "test-count",
        "validation identity",
        "native validation is first-class",
        "run_process for one literal-argv executable",
        "run_shell when shell grammar/output shaping is required",
    ] {
        assert!(validate.contains(phrase), "validate selection: {phrase}");
    }

    let review = format!(
        "{}\n{}",
        flow("review").summary,
        flow("review").manifest_purpose
    )
    .to_lowercase();
    for phrase in [
        "small bounded git observations may use native git",
        "workspace-wide review",
        "broad/unknown committed-range mapping",
        "scope/fence-bound paging",
    ] {
        assert!(review.contains(phrase), "review selection: {phrase}");
    }

    let all = TOOL_RECOMMENDED_FLOWS
        .iter()
        .flat_map(|flow| [flow.summary, flow.manifest_purpose])
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();
    for obsolete in [
        "prefer search_project_texts/read_files even for one query/range",
        "use search_project_texts/read_files for inspection even with one query or range",
        "canonical default even when many lines change",
        "use structured rust or go validation",
        "use run_shell only for shell-specific validation",
    ] {
        assert!(
            !all.contains(obsolete),
            "obsolete selection ritual returned: {obsolete}"
        );
    }
}

#[test]
fn discovery_and_persistent_shell_flows_route_high_value_adaptive_tools() {
    let discovery = TOOL_RECOMMENDED_FLOWS
        .iter()
        .find(|flow| flow.name == "discovery")
        .expect("discovery recommended flow");
    for tool in ["runtime_status", "list_runners", "list_projects"] {
        assert!(discovery.tools.contains(&tool), "discovery: {tool}");
    }
    assert!(discovery.summary.contains("exact Runner client_id"));
    assert!(discovery.summary.contains("before treating it as absent"));
    for phrase in [
        "runtime_status(client_id=...)",
        "list_projects(client_id=...)",
        "list_runners",
    ] {
        assert!(
            discovery.manifest_purpose.contains(phrase),
            "discovery: {phrase}"
        );
    }
    assert!(!discovery.tools.contains(&"list_agents"));
    assert!(!discovery.manifest_purpose.contains("list_agents"));

    let persistent = TOOL_RECOMMENDED_FLOWS
        .iter()
        .find(|flow| flow.name == "persistent_shell")
        .expect("persistent shell recommended flow");
    assert_eq!(persistent.tools.first().copied(), Some("ssh_resource"));
    assert_eq!(
        persistent.tools.get(1).copied(),
        Some("update_session_context")
    );
    assert_eq!(persistent.tools.get(2).copied(), Some("open_session_shell"));
    assert_eq!(persistent.tools.get(3).copied(), Some("session_shell_exec"));
    assert!(persistent.tools.contains(&"session_shell_status"));
    assert!(persistent.tools.contains(&"close_session_shell"));
    assert!(persistent.tools.contains(&"run_process"));
    assert!(persistent.summary.contains("primarily reuse one shell"));
    assert!(persistent.summary.contains("active named SSH resource"));
    assert!(persistent.summary.contains("remote shell state"));
    assert!(persistent.summary.contains("ssh_resource list/register"));
    assert!(persistent
        .summary
        .contains("restart -> list -> bind -> open/reuse"));
    assert!(persistent
        .summary
        .contains("Local persistent shell is only for true same-process state"));
    assert!(persistent.summary.contains("one-shot SSH uses run_process"));
    assert!(persistent
        .manifest_purpose
        .contains("SSH target does not run WebCodex Runner"));
    assert!(persistent.manifest_purpose.contains("ssh_resource list"));
    assert!(persistent
        .manifest_purpose
        .contains("ssh_resource register"));
    for phrase in [
        "ssh-resource-primary",
        "session_shell_exec repeatedly preserves remote cwd/env/exports/functions/umask",
        "local persistent shell remains supported only when same local-process state is required",
        "several ordinary local commands are not enough",
        "explicit one-shot/no-persistence ssh",
    ] {
        assert!(
            persistent
                .manifest_purpose
                .to_ascii_lowercase()
                .contains(phrase),
            "persistent_shell should mention {phrase}: {}",
            persistent.manifest_purpose
        );
    }
    for tool in [
        "ssh_resource",
        "update_session_context",
        "open_session_shell",
        "session_shell_exec",
        "session_shell_status",
        "close_session_shell",
        "run_process",
    ] {
        assert!(
            persistent.manifest_purpose.contains(tool),
            "persistent_shell: {tool}"
        );
    }
}

#[test]
fn tool_categories_include_edit_group() {
    let categories = registered_tool_categories();
    let edit = categories[TOOL_DISCOVERY_GROUP_EDIT]
        .as_array()
        .expect("edit category present");
    for present in [
        "apply_text_edits",
        "apply_patch",
        "write_project_file",
        "apply_unified_diff",
    ] {
        assert!(edit.iter().any(|value| value == present));
    }
    for removed in ["replace_in_file", "replace_line_range", "insert_at_line"] {
        assert!(!edit.iter().any(|value| value == removed));
    }
}

#[test]
fn tool_categories_include_projects_with_management_tools() {
    let categories = registered_tool_categories();
    let projects = categories[TOOL_DISCOVERY_GROUP_PROJECTS]
        .as_array()
        .expect("projects category present");
    assert!(projects.iter().any(|value| value == "register_project"));
    assert!(projects.iter().any(|value| value == "create_project"));
}

#[test]
fn tool_manifest_intents_reference_only_known_model_visible_tools() {
    // High-level intent views rank canonical choices; exact compatibility
    // primitives remain discoverable by tool_name without becoming peer choices.

    let expected = [
        "coding",
        "audit",
        "exploration",
        "file_transfer",
        "release",
        "discovery",
    ];
    let names = TOOL_MANIFEST_INTENTS
        .iter()
        .map(|intent| intent.name)
        .collect::<Vec<_>>();
    assert_eq!(names, expected);
    assert_eq!(available_tool_manifest_intent_names(), names);
    for name in &names {
        let resolved = resolve_tool_manifest_intent(name)
            .unwrap_or_else(|unknown| panic!("available intent {unknown} must resolve"))
            .unwrap_or_else(|| panic!("available intent {name} must not resolve as empty"));
        assert_eq!(resolved.name, *name);
    }

    let mut seen = BTreeSet::new();
    for intent in TOOL_MANIFEST_INTENTS {
        assert!(!intent.tools.is_empty(), "{}", intent.name);
        assert!(seen.insert(intent.name), "duplicate intent {}", intent.name);
        for tool in intent.tools {
            assert!(is_known_tool_name(tool), "{}: {tool}", intent.name);
            assert!(is_model_visible_tool_name(tool), "{}: {tool}", intent.name);
            if matches!(intent.name, "audit" | "exploration" | "release") {
                assert_ne!(*tool, "run_shell", "{}", intent.name);
                assert_ne!(*tool, "run_job", "{}", intent.name);
            }
        }
    }
}

#[test]
fn audit_and_exploration_intents_prefer_canonical_batch_and_review_tools() {
    for intent_name in ["audit", "exploration"] {
        let intent = TOOL_MANIFEST_INTENTS
            .iter()
            .find(|intent| intent.name == intent_name)
            .unwrap();
        assert!(intent.tools.contains(&"read_files"), "{intent_name}");
        assert!(
            intent.tools.contains(&"search_project_texts"),
            "{intent_name}"
        );
        assert!(!intent.tools.contains(&"read_file"), "{intent_name}");
        assert!(
            !intent.tools.contains(&"search_project_text"),
            "{intent_name}"
        );
    }
    let audit = TOOL_MANIFEST_INTENTS
        .iter()
        .find(|intent| intent.name == "audit")
        .unwrap();
    assert!(audit.tools.contains(&"show_changes"));
    assert!(audit.tools.contains(&"git_diff_hunks"));
    assert!(!audit.tools.contains(&"git_diff_summary"));

    let release = TOOL_MANIFEST_INTENTS
        .iter()
        .find(|intent| intent.name == "release")
        .unwrap();
    assert!(release.tools.contains(&"show_changes"));
    assert!(!release.tools.contains(&"git_diff_summary"));
}

#[test]
fn validate_flow_uses_observe_jobs_without_recommending_job_status() {
    let validate = TOOL_RECOMMENDED_FLOWS
        .iter()
        .find(|flow| flow.name == "validate")
        .unwrap();
    assert!(validate.tools.contains(&"observe_jobs"));
    assert!(!validate.tools.contains(&"job_status"));
}

#[test]
fn project_overview_manifest_profiles_match_intended_workflows() {
    for intent in ["coding", "audit", "exploration", "discovery"] {
        let profile = TOOL_MANIFEST_INTENTS
            .iter()
            .find(|profile| profile.name == intent)
            .unwrap_or_else(|| panic!("missing {intent} intent"));
        assert!(profile.tools.contains(&"project_overview"), "{intent}");
    }
    let release = TOOL_MANIFEST_INTENTS
        .iter()
        .find(|profile| profile.name == "release")
        .expect("release intent");
    assert!(!release.tools.contains(&"project_overview"));
}

#[test]
fn coding_intent_has_independent_ordered_canonical_selection_surface() {
    let coding = TOOL_MANIFEST_INTENTS
        .iter()
        .find(|intent| intent.name == "coding")
        .expect("coding intent");
    assert_eq!(coding.tools, CODING_INTENT_TOOL_NAMES);
    assert_eq!(coding.tools.first().copied(), Some("work_on_project"));
    assert_eq!(coding.tools.last().copied(), Some("finish_coding_task"));

    let mut seen = BTreeSet::new();
    for tool in CODING_INTENT_TOOL_NAMES {
        assert!(seen.insert(*tool), "duplicate coding intent tool {tool}");
    }
    for required in [
        "work_on_project",
        "search_project_texts",
        "read_files",
        "apply_text_edits",
        "run_process",
        "run_shell",
        "observe_jobs",
        "cargo_check",
        "cargo_test",
        "show_changes",
        "git_diff_hunks",
        "workspace_hygiene_check",
        "finish_coding_task",
        "apply_patch",
        "run_script",
        "cargo_fmt",
        "go_test",
        "goto_definition",
        "find_references",
    ] {
        assert!(coding.tools.contains(&required), "missing {required}");
    }
    for compatibility_or_overlap in [
        "read_file",
        "search_project_text",
        "git_diff",
        "git_diff_summary",
        "job_status",
        "job_log",
        "run_job",
        "apply_unified_diff",
        "coding_agent_start",
        "coding_agent_observe",
        "coding_agent_cancel",
        "get_session_assignment",
        "complete_session_message",
    ] {
        assert!(
            !coding.tools.contains(&compatibility_or_overlap),
            "coding intent should not recommend {compatibility_or_overlap}"
        );
    }
    let apply_text_edits_position = coding
        .tools
        .iter()
        .position(|tool| *tool == "apply_text_edits")
        .unwrap();
    let apply_patch_position = coding
        .tools
        .iter()
        .position(|tool| *tool == "apply_patch")
        .unwrap();
    assert!(apply_text_edits_position < apply_patch_position);
}
