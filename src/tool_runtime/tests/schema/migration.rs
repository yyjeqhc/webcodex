use super::*;

use crate::tool_runtime::metadata::{
    ToolPathHint, ToolRisk, PROJECT_WRITE, TOOL_PROVIDER_AGENT, TOOL_PROVIDER_UNKNOWN,
};
use crate::tool_runtime::tool_definition::AgentCapability;

struct LegacyMetadataFallback {
    name: &'static str,
    reason: &'static str,
}

// TODO(tool-definition): delete this allowlist when the legacy dedicated
// delete-files HTTP route is removed or is represented outside the runtime tool
// metadata facade.
const KNOWN_LEGACY_METADATA_FALLBACKS: &[LegacyMetadataFallback] = &[LegacyMetadataFallback {
    name: "delete_files",
    reason: "legacy dedicated HTTP route metadata; not accepted by ToolCall and not a runtime tool",
}];

#[derive(Clone, Copy)]
struct ExpectedToolPolicy {
    name: &'static str,
    category: &'static str,
    risk_class: &'static str,
    read_like: bool,
    write_like: bool,
    shell_like: bool,
    git_like: bool,
    session_policy: &'static str,
    requires_permission: bool,
    agent_capability: Option<AgentCapability>,
}

#[test]
fn tool_definition_runtime_tool_policy_inventory_is_stable() {
    use crate::tool_runtime::tool_definition::{
        lookup_tool_definition, runtime_tool_allows_current_session_fallback,
        runtime_tool_creates_or_binds_session, runtime_tool_is_current_session_control,
        runtime_tool_is_git_like, runtime_tool_is_read_like, runtime_tool_is_shell_like,
        runtime_tool_is_write_like, runtime_tool_requires_explicit_business_session,
        runtime_tool_requires_permission, runtime_tool_session_risk_class, tool_definitions,
    };
    use AgentCapability::{AsyncJobs, FileRead, FileWrite, GitOrShell, OwnerOnly, Shell};

    let expected = [
        ExpectedToolPolicy::new(
            "list_tools",
            "runtime",
            "read_only",
            true,
            false,
            false,
            false,
            "none",
            false,
            None,
        ),
        ExpectedToolPolicy::new(
            "start_session",
            "session",
            "read_only",
            true,
            false,
            false,
            false,
            "creates_or_binds",
            false,
            None,
        ),
        ExpectedToolPolicy::new(
            "start_coding_task",
            "workflow",
            "read_only",
            true,
            false,
            false,
            false,
            "creates_or_binds",
            false,
            Some(GitOrShell),
        ),
        ExpectedToolPolicy::new(
            "finish_coding_task",
            "workflow",
            "read_only",
            true,
            false,
            false,
            false,
            "explicit_business_session",
            false,
            Some(GitOrShell),
        ),
        ExpectedToolPolicy::new(
            "session_summary",
            "session",
            "read_only",
            true,
            false,
            false,
            false,
            "explicit_business_session",
            false,
            None,
        ),
        ExpectedToolPolicy::new(
            "post_session_message",
            "session",
            "read_only",
            true,
            false,
            false,
            false,
            "explicit_business_session",
            false,
            None,
        ),
        ExpectedToolPolicy::new(
            "list_session_messages",
            "session",
            "read_only",
            true,
            false,
            false,
            false,
            "explicit_business_session",
            false,
            None,
        ),
        ExpectedToolPolicy::new(
            "resolve_session_message",
            "session",
            "read_only",
            true,
            false,
            false,
            false,
            "explicit_business_session",
            false,
            None,
        ),
        ExpectedToolPolicy::new(
            "session_discussion_summary",
            "session",
            "read_only",
            true,
            false,
            false,
            false,
            "explicit_business_session",
            false,
            None,
        ),
        ExpectedToolPolicy::new(
            "session_handoff_summary",
            "session",
            "read_only",
            true,
            false,
            false,
            false,
            "explicit_business_session",
            false,
            None,
        ),
        ExpectedToolPolicy::new(
            "bind_current_session",
            "session",
            "read_only",
            true,
            false,
            false,
            false,
            "creates_or_binds+current_session_control",
            false,
            None,
        ),
        ExpectedToolPolicy::new(
            "current_session",
            "session",
            "read_only",
            true,
            false,
            false,
            false,
            "current_session_control",
            false,
            None,
        ),
        ExpectedToolPolicy::new(
            "unbind_current_session",
            "session",
            "read_only",
            true,
            false,
            false,
            false,
            "current_session_control",
            false,
            None,
        ),
        ExpectedToolPolicy::new(
            "workspace_checkpoint_create",
            "checkpoint",
            "read_only",
            true,
            false,
            false,
            true,
            "current_session_fallback",
            false,
            Some(FileRead),
        ),
        ExpectedToolPolicy::new(
            "workspace_checkpoint_list",
            "checkpoint",
            "read_only",
            true,
            false,
            false,
            false,
            "current_session_fallback",
            false,
            Some(OwnerOnly),
        ),
        ExpectedToolPolicy::new(
            "workspace_checkpoint_show",
            "checkpoint",
            "read_only",
            true,
            false,
            false,
            false,
            "current_session_fallback",
            false,
            Some(OwnerOnly),
        ),
        ExpectedToolPolicy::new(
            "workspace_checkpoint_restore",
            "checkpoint",
            "project_write",
            false,
            true,
            false,
            true,
            "current_session_fallback",
            true,
            Some(FileWrite),
        ),
        ExpectedToolPolicy::new(
            "workspace_checkpoint_delete",
            "checkpoint",
            "project_write",
            false,
            true,
            false,
            false,
            "current_session_fallback",
            true,
            Some(OwnerOnly),
        ),
        ExpectedToolPolicy::new(
            "run_shell",
            "job",
            "job_run",
            false,
            false,
            true,
            false,
            "current_session_fallback",
            true,
            Some(Shell),
        ),
        ExpectedToolPolicy::new(
            "apply_patch",
            "patch",
            "project_write",
            false,
            true,
            false,
            false,
            "current_session_fallback",
            true,
            Some(Shell),
        ),
        ExpectedToolPolicy::new(
            "apply_patch_checked",
            "patch",
            "project_write",
            false,
            true,
            false,
            false,
            "current_session_fallback",
            true,
            Some(Shell),
        ),
        ExpectedToolPolicy::new(
            "delete_project_files",
            "cleanup",
            "project_write",
            false,
            true,
            false,
            false,
            "current_session_fallback",
            true,
            Some(Shell),
        ),
        ExpectedToolPolicy::new(
            "git_restore_paths",
            "cleanup",
            "project_write",
            false,
            true,
            false,
            true,
            "current_session_fallback",
            true,
            Some(Shell),
        ),
        ExpectedToolPolicy::new(
            "discard_untracked",
            "cleanup",
            "project_write",
            false,
            true,
            false,
            true,
            "current_session_fallback",
            true,
            Some(Shell),
        ),
        ExpectedToolPolicy::new(
            "validate_patch",
            "patch",
            "read_only",
            true,
            false,
            false,
            false,
            "current_session_fallback",
            false,
            Some(Shell),
        ),
        ExpectedToolPolicy::new(
            "git_status",
            "git",
            "read_only",
            true,
            false,
            false,
            true,
            "current_session_fallback",
            false,
            Some(GitOrShell),
        ),
        ExpectedToolPolicy::new(
            "git_diff",
            "git",
            "read_only",
            true,
            false,
            false,
            true,
            "current_session_fallback",
            false,
            Some(GitOrShell),
        ),
        ExpectedToolPolicy::new(
            "git_diff_hunks",
            "git",
            "read_only",
            true,
            false,
            false,
            true,
            "current_session_fallback",
            false,
            Some(GitOrShell),
        ),
        ExpectedToolPolicy::new(
            "git_log",
            "git",
            "read_only",
            true,
            false,
            false,
            true,
            "current_session_fallback",
            false,
            Some(GitOrShell),
        ),
        ExpectedToolPolicy::new(
            "cargo_fmt",
            "validation",
            "job_run",
            false,
            false,
            true,
            false,
            "current_session_fallback",
            true,
            Some(Shell),
        ),
        ExpectedToolPolicy::new(
            "cargo_check",
            "validation",
            "job_run",
            false,
            false,
            true,
            false,
            "current_session_fallback",
            true,
            Some(Shell),
        ),
        ExpectedToolPolicy::new(
            "cargo_test",
            "validation",
            "job_run",
            false,
            false,
            true,
            false,
            "current_session_fallback",
            true,
            Some(Shell),
        ),
        ExpectedToolPolicy::new(
            "read_file",
            "file",
            "read_only",
            true,
            false,
            false,
            false,
            "current_session_fallback",
            false,
            Some(FileRead),
        ),
        ExpectedToolPolicy::new(
            "run_job",
            "job",
            "job_run",
            false,
            false,
            true,
            false,
            "current_session_fallback",
            true,
            Some(AsyncJobs),
        ),
        ExpectedToolPolicy::new(
            "stop_job",
            "job",
            "job_run",
            false,
            false,
            true,
            false,
            "current_session_fallback",
            true,
            None,
        ),
        ExpectedToolPolicy::new(
            "run_codex",
            "codex",
            "job_run",
            false,
            false,
            true,
            false,
            "current_session_fallback",
            true,
            Some(AsyncJobs),
        ),
        ExpectedToolPolicy::new(
            "job_status",
            "job",
            "read_only",
            true,
            false,
            false,
            false,
            "none",
            false,
            None,
        ),
        ExpectedToolPolicy::new(
            "job_log",
            "job",
            "read_only",
            true,
            false,
            false,
            false,
            "none",
            false,
            None,
        ),
        ExpectedToolPolicy::new(
            "list_project_files",
            "file",
            "read_only",
            true,
            false,
            false,
            false,
            "current_session_fallback",
            false,
            Some(FileRead),
        ),
        ExpectedToolPolicy::new(
            "search_project_text",
            "file",
            "read_only",
            true,
            false,
            false,
            false,
            "current_session_fallback",
            false,
            Some(Shell),
        ),
        ExpectedToolPolicy::new(
            "git_diff_summary",
            "git",
            "read_only",
            true,
            false,
            false,
            true,
            "current_session_fallback",
            false,
            Some(GitOrShell),
        ),
        ExpectedToolPolicy::new(
            "show_changes",
            "git",
            "read_only",
            true,
            false,
            false,
            true,
            "current_session_fallback",
            false,
            Some(GitOrShell),
        ),
        ExpectedToolPolicy::new(
            "workspace_hygiene_check",
            "cleanup",
            "read_only",
            true,
            false,
            false,
            false,
            "current_session_fallback",
            false,
            Some(GitOrShell),
        ),
        ExpectedToolPolicy::new(
            "list_jobs",
            "job",
            "read_only",
            true,
            false,
            false,
            false,
            "none",
            false,
            None,
        ),
        ExpectedToolPolicy::new(
            "job_tail",
            "job",
            "read_only",
            true,
            false,
            false,
            false,
            "none",
            false,
            None,
        ),
        ExpectedToolPolicy::new(
            "replace_in_file",
            "edit",
            "project_write",
            false,
            true,
            false,
            false,
            "current_session_fallback",
            true,
            Some(FileWrite),
        ),
        ExpectedToolPolicy::new(
            "replace_exact_block",
            "edit",
            "project_write",
            false,
            true,
            false,
            false,
            "current_session_fallback",
            true,
            Some(FileWrite),
        ),
        ExpectedToolPolicy::new(
            "insert_before_pattern",
            "edit",
            "project_write",
            false,
            true,
            false,
            false,
            "current_session_fallback",
            true,
            Some(FileWrite),
        ),
        ExpectedToolPolicy::new(
            "insert_after_pattern",
            "edit",
            "project_write",
            false,
            true,
            false,
            false,
            "current_session_fallback",
            true,
            Some(FileWrite),
        ),
        ExpectedToolPolicy::new(
            "write_project_file",
            "edit",
            "project_write",
            false,
            true,
            false,
            false,
            "current_session_fallback",
            true,
            Some(FileWrite),
        ),
        ExpectedToolPolicy::new(
            "save_project_artifact",
            "artifact",
            "project_write",
            false,
            true,
            false,
            false,
            "current_session_fallback",
            true,
            Some(FileWrite),
        ),
        ExpectedToolPolicy::new(
            "read_project_artifact_metadata",
            "artifact",
            "read_only",
            true,
            false,
            false,
            false,
            "current_session_fallback",
            false,
            Some(FileRead),
        ),
        ExpectedToolPolicy::new(
            "read_project_artifact",
            "artifact",
            "read_only",
            true,
            false,
            false,
            false,
            "current_session_fallback",
            false,
            Some(FileRead),
        ),
        ExpectedToolPolicy::new(
            "artifact_upload_begin",
            "artifact",
            "project_write",
            false,
            true,
            false,
            false,
            "current_session_fallback",
            true,
            Some(FileWrite),
        ),
        ExpectedToolPolicy::new(
            "artifact_upload_chunk",
            "artifact",
            "project_write",
            false,
            true,
            false,
            false,
            "current_session_fallback",
            true,
            Some(FileWrite),
        ),
        ExpectedToolPolicy::new(
            "artifact_upload_finish",
            "artifact",
            "project_write",
            false,
            true,
            false,
            false,
            "current_session_fallback",
            true,
            Some(FileWrite),
        ),
        ExpectedToolPolicy::new(
            "artifact_upload_abort",
            "artifact",
            "project_write",
            false,
            true,
            false,
            false,
            "current_session_fallback",
            true,
            Some(FileWrite),
        ),
        ExpectedToolPolicy::new(
            "replace_line_range",
            "edit",
            "project_write",
            false,
            true,
            false,
            false,
            "current_session_fallback",
            true,
            Some(FileWrite),
        ),
        ExpectedToolPolicy::new(
            "insert_at_line",
            "edit",
            "project_write",
            false,
            true,
            false,
            false,
            "current_session_fallback",
            true,
            Some(FileWrite),
        ),
        ExpectedToolPolicy::new(
            "delete_line_range",
            "edit",
            "project_write",
            false,
            true,
            false,
            false,
            "current_session_fallback",
            true,
            Some(FileWrite),
        ),
        ExpectedToolPolicy::new(
            "apply_text_edits",
            "edit",
            "project_write",
            false,
            true,
            false,
            false,
            "current_session_fallback",
            true,
            Some(FileWrite),
        ),
        ExpectedToolPolicy::new(
            "list_projects",
            "project",
            "read_only",
            true,
            false,
            false,
            false,
            "none",
            false,
            None,
        ),
        ExpectedToolPolicy::new(
            "register_project",
            "project",
            "project_write",
            false,
            true,
            false,
            false,
            "none",
            true,
            None,
        ),
        ExpectedToolPolicy::new(
            "create_project",
            "project",
            "project_write",
            false,
            true,
            false,
            false,
            "none",
            true,
            None,
        ),
        ExpectedToolPolicy::new(
            "list_agents",
            "runtime",
            "read_only",
            true,
            false,
            false,
            false,
            "none",
            false,
            None,
        ),
        ExpectedToolPolicy::new(
            "runtime_status",
            "runtime",
            "read_only",
            true,
            false,
            false,
            false,
            "none",
            false,
            None,
        ),
        ExpectedToolPolicy::new(
            "tool_manifest",
            "runtime",
            "read_only",
            true,
            false,
            false,
            false,
            "none",
            false,
            None,
        ),
    ];

    let expected_names = expected
        .iter()
        .map(|entry| entry.name)
        .collect::<BTreeSet<_>>();
    let definition_names = tool_definitions()
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    let definition_name_set = definition_names.iter().copied().collect::<BTreeSet<_>>();
    assert_eq!(definition_name_set, expected_names);
    assert_eq!(definition_names, known_tool_names().collect::<Vec<_>>());
    assert_eq!(definition_names.len(), 67, "runtime ToolDefinition count");

    for entry in expected {
        let definition = lookup_tool_definition(entry.name)
            .unwrap_or_else(|| panic!("{} missing ToolDefinition", entry.name));
        assert_eq!(
            definition.category, entry.category,
            "{} category",
            entry.name
        );
        assert_eq!(
            runtime_tool_session_risk_class(entry.name),
            entry.risk_class,
            "{} risk class",
            entry.name
        );
        assert_eq!(
            runtime_tool_is_read_like(entry.name),
            entry.read_like,
            "{} read-like classification",
            entry.name
        );
        assert_eq!(
            runtime_tool_is_write_like(entry.name),
            entry.write_like,
            "{} write-like classification",
            entry.name
        );
        assert_eq!(
            runtime_tool_is_shell_like(entry.name),
            entry.shell_like,
            "{} shell-like classification",
            entry.name
        );
        assert_eq!(
            runtime_tool_is_git_like(entry.name),
            entry.git_like,
            "{} git-like classification",
            entry.name
        );
        assert_eq!(
            session_policy_label(
                runtime_tool_creates_or_binds_session(entry.name),
                runtime_tool_is_current_session_control(entry.name),
                runtime_tool_requires_explicit_business_session(entry.name),
                runtime_tool_allows_current_session_fallback(entry.name),
            ),
            entry.session_policy,
            "{} session policy",
            entry.name
        );
        assert_eq!(
            runtime_tool_requires_permission(entry.name),
            entry.requires_permission,
            "{} permission requirement",
            entry.name
        );
        assert_eq!(
            definition.agent_capability, entry.agent_capability,
            "{} agent capability",
            entry.name
        );
    }
}

#[test]
fn tool_definition_explains_all_tool_call_runtime_names() {
    use crate::tool_runtime::tool_definition::{lookup_tool_definition, tool_definitions};

    let definition_names = tool_definitions()
        .map(|definition| definition.name)
        .collect::<BTreeSet<_>>();
    let known_names = known_tool_names().collect::<BTreeSet<_>>();
    assert_eq!(
        definition_names, known_names,
        "Every ToolCall-reachable runtime name must be explained by ToolDefinition"
    );

    for name in known_tool_names() {
        let args = if name == "run_codex" {
            json!({"project": SAMPLE_PROJECT, "prompt": "summarize"})
        } else {
            sample_tool_args(name)
        };
        let call = ToolCall::from_tool_name(name, args)
            .unwrap_or_else(|err| panic!("{name} should parse through ToolDefinition: {err}"));
        assert_eq!(call.tool_name(), name);
        assert!(
            lookup_tool_definition(call.tool_name()).is_some(),
            "{} ToolCall::tool_name must resolve to ToolDefinition",
            call.tool_name()
        );
    }

    for fallback in KNOWN_LEGACY_METADATA_FALLBACKS {
        assert!(
            ToolCall::from_tool_name(fallback.name, json!({})).is_err(),
            "{} is a legacy metadata fallback only: {}",
            fallback.name,
            fallback.reason
        );
    }
}

#[test]
fn tool_policy_helpers_match_tool_definitions_for_known_runtime_names() {
    use crate::tool_runtime::tool_definition::{
        runtime_tool_is_read_like, runtime_tool_is_shell_like, runtime_tool_is_write_like,
        runtime_tool_metadata, runtime_tool_permission_risk, runtime_tool_requires_permission,
        runtime_tool_requires_session_project_escape, runtime_tool_session_risk_class,
        tool_definitions,
    };

    for definition in tool_definitions() {
        assert_eq!(
            runtime_tool_metadata(definition.name),
            definition.metadata(),
            "{} metadata policy helper must read the ToolDefinition metadata",
            definition.name
        );
        assert_eq!(
            runtime_tool_session_risk_class(definition.name),
            definition.session_risk_class(),
            "{} session risk helper must match ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_is_read_like(definition.name),
            definition.is_read_like(),
            "{} read-like helper must match ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_is_write_like(definition.name),
            definition.is_write_like(),
            "{} write-like helper must match ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_is_shell_like(definition.name),
            definition.is_shell_like(),
            "{} shell-like helper must match ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_requires_permission(definition.name),
            definition.requires_permission(),
            "{} permission helper must match ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_requires_session_project_escape(definition.name),
            definition.requires_session_project_escape(),
            "{} session-project escape helper must match ToolDefinition",
            definition.name
        );
        assert_eq!(
            runtime_tool_permission_risk(definition.name),
            definition.permission_risk(),
            "{} permission risk helper must match ToolDefinition",
            definition.name
        );
    }
}

#[test]
fn tool_definition_strict_agent_capability_lookup_has_no_metadata_fallback() {
    use crate::tool_runtime::tool_definition::runtime_tool_agent_capability;

    for name in known_tool_names() {
        let _ = runtime_tool_agent_capability(name);
    }
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    for fallback in KNOWN_LEGACY_METADATA_FALLBACKS {
        let result = std::panic::catch_unwind(|| runtime_tool_agent_capability(fallback.name));
        assert!(
            result.is_err(),
            "{} must not resolve agent capability through legacy metadata fallback: {}",
            fallback.name,
            fallback.reason
        );
    }
    std::panic::set_hook(previous_hook);
}

#[test]
fn tool_definition_metadata_fallback_facade_is_legacy_or_unknown_only() {
    use crate::tool_runtime::metadata::{lookup_tool_metadata, tool_metadata};
    use crate::tool_runtime::tool_definition::{
        lookup_tool_definition, runtime_tool_is_read_like, runtime_tool_is_shell_like,
        runtime_tool_is_write_like, runtime_tool_metadata, runtime_tool_permission_risk,
        runtime_tool_requires_permission, runtime_tool_requires_session_project_escape,
        runtime_tool_session_risk_class, PERMISSION_RISK_DESTRUCTIVE, PERMISSION_RISK_WRITE,
    };

    let delete_files = lookup_tool_metadata("delete_files")
        .copied()
        .expect("delete_files legacy route metadata");
    assert_eq!(delete_files.name, "delete_files");
    assert_eq!(delete_files.provider_id, TOOL_PROVIDER_AGENT);
    assert_eq!(delete_files.risk, ToolRisk::ProjectWrite);
    assert_eq!(delete_files.oauth_scope, Some(PROJECT_WRITE));
    assert!(delete_files.requires_project);
    assert_eq!(delete_files.path_hint, ToolPathHint::PathList);
    assert!(!delete_files.read_only);
    assert!(delete_files.destructive);
    assert!(!delete_files.shell_like);
    assert!(
        lookup_tool_definition("delete_files").is_none(),
        "delete_files must remain metadata-only legacy route metadata"
    );
    assert!(!is_known_tool_name("delete_files"));
    assert!(
        ToolCall::from_tool_name(
            "delete_files",
            json!({"project": SAMPLE_PROJECT, "paths": ["old.txt"]})
        )
        .is_err(),
        "delete_files metadata fallback must not make it ToolCall-parseable"
    );
    assert_eq!(runtime_tool_metadata("delete_files"), delete_files);
    assert_eq!(
        runtime_tool_session_risk_class("delete_files"),
        ToolRisk::ProjectWrite.session_risk_class()
    );
    assert!(!runtime_tool_is_read_like("delete_files"));
    assert!(runtime_tool_is_write_like("delete_files"));
    assert!(!runtime_tool_is_shell_like("delete_files"));
    assert!(runtime_tool_requires_permission("delete_files"));
    assert!(runtime_tool_requires_session_project_escape("delete_files"));
    assert_eq!(
        runtime_tool_permission_risk("delete_files"),
        PERMISSION_RISK_DESTRUCTIVE
    );

    let unknown = tool_metadata("__unknown_non_runtime__");
    assert!(lookup_tool_metadata("__unknown_non_runtime__").is_none());
    assert_eq!(unknown.name, "<unknown>");
    assert_eq!(unknown.provider_id, TOOL_PROVIDER_UNKNOWN);
    assert_eq!(unknown.risk, ToolRisk::Unknown);
    assert_eq!(unknown.oauth_scope, None);
    assert!(!unknown.requires_project);
    assert_eq!(unknown.path_hint, ToolPathHint::None);
    assert!(!unknown.read_only);
    assert!(!unknown.destructive);
    assert!(!unknown.shell_like);
    assert_eq!(runtime_tool_metadata("__unknown_non_runtime__"), unknown);
    assert_eq!(
        runtime_tool_session_risk_class("__unknown_non_runtime__"),
        ToolRisk::Unknown.session_risk_class()
    );
    assert!(!runtime_tool_is_read_like("__unknown_non_runtime__"));
    assert!(!runtime_tool_is_write_like("__unknown_non_runtime__"));
    assert!(!runtime_tool_is_shell_like("__unknown_non_runtime__"));
    assert!(runtime_tool_requires_permission("__unknown_non_runtime__"));
    assert!(runtime_tool_requires_session_project_escape(
        "__unknown_non_runtime__"
    ));
    assert_eq!(
        runtime_tool_permission_risk("__unknown_non_runtime__"),
        PERMISSION_RISK_WRITE
    );
    assert!(!is_known_tool_name("__unknown_non_runtime__"));
    assert!(ToolCall::from_tool_name("__unknown_non_runtime__", json!({})).is_err());
}

#[test]
fn tool_definition_legacy_metadata_fallbacks_are_explicit_and_reasoned() {
    let metadata_only_names = crate::tool_runtime::metadata::iter_tool_metadata()
        .filter(|metadata| !is_known_tool_name(metadata.name))
        .map(|metadata| metadata.name)
        .collect::<Vec<_>>();
    let expected_names = KNOWN_LEGACY_METADATA_FALLBACKS
        .iter()
        .map(|fallback| fallback.name)
        .collect::<Vec<_>>();
    let fallback_reasons = KNOWN_LEGACY_METADATA_FALLBACKS
        .iter()
        .map(|fallback| format!("{}: {}", fallback.name, fallback.reason))
        .collect::<Vec<_>>();

    assert_eq!(
        metadata_only_names, expected_names,
        "remaining metadata fallbacks must stay explicit and reasoned: {fallback_reasons:?}"
    );
    for fallback in KNOWN_LEGACY_METADATA_FALLBACKS {
        eprintln!(
            "legacy metadata fallback retained: {} - {}",
            fallback.name, fallback.reason
        );
        assert!(
            !fallback.reason.trim().is_empty(),
            "{} fallback must explain why it remains",
            fallback.name
        );
    }

    let unknown = crate::tool_runtime::tool_definition::runtime_tool_metadata("__unknown__");
    eprintln!(
        "unknown metadata fallback retained: non-runtime unknown names return provider={} risk={:?}",
        unknown.provider_id, unknown.risk
    );
    assert_eq!(unknown.provider_id, TOOL_PROVIDER_UNKNOWN);
    assert_eq!(unknown.risk, ToolRisk::Unknown);
    assert!(!is_known_tool_name("__unknown__"));
    assert!(ToolCall::from_tool_name("__unknown__", json!({})).is_err());
}

#[test]
fn tool_definition_surface_counts_stay_fixed_during_fallback_migration() {
    use crate::tool_runtime::tool_definition::{
        lookup_tool_definition, model_hidden_tool_names, tool_definitions,
    };

    let openapi = crate::openapi::build_openapi_spec();
    let openapi_operation_count: usize = openapi["paths"]
        .as_object()
        .unwrap()
        .values()
        .map(|methods| methods.as_object().unwrap().len())
        .sum();
    assert_eq!(openapi_operation_count, 25, "OpenAPI operation count");

    let operation_ids = openapi["paths"]
        .as_object()
        .unwrap()
        .values()
        .flat_map(|methods| methods.as_object().unwrap().values())
        .map(|operation| operation["operationId"].as_str().unwrap())
        .collect::<Vec<_>>();
    for forbidden in [
        "runCodex",
        "RunCodex",
        "sessionHandoffSummary",
        "SessionHandoff",
        "applyTextEdits",
        "ApplyTextEdits",
        "artifactUpload",
        "ArtifactUpload",
    ] {
        assert!(
            !operation_ids
                .iter()
                .any(|operation_id| operation_id.contains(forbidden)),
            "{forbidden} must remain hidden/runtime-only and not become a dedicated GPT Action: {operation_ids:?}"
        );
    }

    let tool_call_properties = openapi["components"]["schemas"]["ToolCallRequest"]["properties"]
        .as_object()
        .expect("ToolCallRequest properties");
    for field in [
        "expected_failure",
        "expected_failure_kind",
        "test_expect_failure_kind",
        "assertion_name",
        "summary_only",
        "include_command_preview",
        "compact_startup",
    ] {
        assert!(
            tool_call_properties.contains_key(field),
            "callRuntimeTool must keep flattened GPT Action field {field}"
        );
    }
    let tool_description = tool_call_properties["tool"]["description"]
        .as_str()
        .unwrap();
    assert!(
        !tool_description.contains("run_codex"),
        "callRuntimeTool model-facing accepted-name description must not advertise run_codex"
    );

    let model_facing_names = registered_tool_names();
    let definition_names = tool_definitions()
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    assert_eq!(definition_names.len(), 67, "ToolDefinition count");
    assert!(
        lookup_tool_definition("run_codex").is_some(),
        "hidden run_codex must keep an explicit ToolDefinition"
    );
    assert_eq!(
        model_hidden_tool_names().collect::<Vec<_>>(),
        vec!["run_codex"],
        "run_codex must remain the only hidden ToolDefinition"
    );
    assert_eq!(
        model_facing_names.len(),
        66,
        "tools.count / model-facing tool count"
    );
    assert!(
        !model_facing_names.iter().any(|name| name == "run_codex"),
        "run_codex must remain hidden from model-facing tools: {model_facing_names:?}"
    );
    assert_eq!(
        known_tool_names().count(),
        model_facing_names.len() + 1,
        "ToolDefinition includes only one hidden runtime tool"
    );
}

#[test]
fn tool_definition_dead_code_residue_is_narrow_and_documented() {
    let source = include_str!("../../tool_definition.rs");
    assert!(
        !source.contains("#![allow(dead_code)]"),
        "tool_definition.rs must not use a module-wide dead_code allowance"
    );

    let docs = include_str!("../../../../docs/TOOL_DEFINITION_REGISTRY.md");
    for phrase in [
        "module-wide `#![allow(dead_code)]`",
        "removed",
        "#[cfg(test)]",
        "item-scoped",
    ] {
        assert!(
            docs.contains(phrase),
            "ToolDefinition migration docs should explain dead_code residue: missing {phrase}"
        );
    }
}

impl ExpectedToolPolicy {
    const fn new(
        name: &'static str,
        category: &'static str,
        risk_class: &'static str,
        read_like: bool,
        write_like: bool,
        shell_like: bool,
        git_like: bool,
        session_policy: &'static str,
        requires_permission: bool,
        agent_capability: Option<AgentCapability>,
    ) -> Self {
        Self {
            name,
            category,
            risk_class,
            read_like,
            write_like,
            shell_like,
            git_like,
            session_policy,
            requires_permission,
            agent_capability,
        }
    }
}

fn session_policy_label(
    creates_or_binds_session: bool,
    current_session_control: bool,
    requires_explicit_business_session: bool,
    allows_current_session_fallback: bool,
) -> String {
    let mut labels = Vec::new();
    if creates_or_binds_session {
        labels.push("creates_or_binds");
    }
    if current_session_control {
        labels.push("current_session_control");
    }
    if requires_explicit_business_session {
        labels.push("explicit_business_session");
    }
    if allows_current_session_fallback {
        labels.push("current_session_fallback");
    }
    if labels.is_empty() {
        "none".to_string()
    } else {
        labels.join("+")
    }
}
