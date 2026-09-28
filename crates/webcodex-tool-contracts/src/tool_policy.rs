//! Runtime tool lookup and policy helpers derived from ToolDefinition.

use super::metadata::{
    tool_metadata, ToolApprovalPolicy, ToolEffect, ToolMetadata, ToolPathHint, ToolRisk,
};
use super::tool_definition::{
    tool_definitions, RunnerCapabilityRequirement, ToolActivityInteraction, ToolActivityKind,
    ToolActivityPresentation, ToolActivitySemantics, ToolAuditPolicy, ToolCompositionPolicy,
    ToolDefinition, ToolDiffReviewEvidence, ToolEffectAnnotations, ToolExecutionContract,
    ToolExecutionForm, ToolExplorationEvidence, ToolGptActionExposure, ToolHostOrchestrationHint,
    ToolOperatorExtensionFamily, ToolReviewEvidence, ToolSessionEvidencePolicy,
    ToolValidationIdentityKind, PERMISSION_RISK_ARTIFACT_WRITE, PERMISSION_RISK_DESTRUCTIVE,
    PERMISSION_RISK_PATCH, PERMISSION_RISK_SHELL, PERMISSION_RISK_VALIDATION,
    PERMISSION_RISK_WRITE, TOOL_CATEGORY_JOB,
};

impl ToolDefinition {
    pub fn metadata(self) -> ToolMetadata {
        self.metadata
    }

    pub fn audit_policy(self) -> ToolAuditPolicy {
        self.audit
    }

    pub fn effect_annotations(self) -> ToolEffectAnnotations {
        ToolEffectAnnotations {
            read_only_hint: self.metadata.effect.read_only_hint(),
            destructive_hint: self.metadata.destructive,
            idempotent_hint: self.metadata.idempotency.mcp_hint(),
            open_world_hint: self.metadata.shell_like,
        }
    }

    pub fn session_risk_class(self) -> &'static str {
        self.metadata.risk.session_risk_class()
    }

    pub fn is_read_like(self) -> bool {
        self.metadata.effect == ToolEffect::Observe
    }

    pub fn is_write_like(self) -> bool {
        matches!(
            self.metadata.risk,
            ToolRisk::ProjectWrite
                | ToolRisk::SkillManage
                | ToolRisk::MemoryManage
                | ToolRisk::CommunicationManage
                | ToolRisk::ComputerControl
        )
    }

    pub fn is_shell_like(self) -> bool {
        self.metadata.shell_like || self.metadata.risk == ToolRisk::JobRun
    }

    pub fn is_git_like(self) -> bool {
        self.policy.git_like
    }

    pub fn is_change_summary_like(self) -> bool {
        self.policy.change_summary_like
    }

    pub fn captures_validation_output(self) -> bool {
        self.policy.captures_validation_output
    }

    pub fn adaptive_runtime_direct_rank(self) -> Option<u16> {
        self.adaptive_runtime_direct_rank
    }

    pub fn gpt_action_exposure(self) -> ToolGptActionExposure {
        self.gpt_action_exposure
    }

    pub fn supports_gpt_actions(self) -> bool {
        self.visibility.is_model_visible()
            && self.gpt_action_exposure() != ToolGptActionExposure::Unsupported
    }

    pub fn gpt_action_description(self) -> Option<&'static str> {
        self.model_spec
            .map(|spec| spec.gpt_action_description.unwrap_or(spec.description))
    }

    pub fn session_evidence_policy(self) -> ToolSessionEvidencePolicy {
        self.session_evidence
    }

    pub fn activity_semantics(self) -> ToolActivitySemantics {
        let kind = self.activity.kind_override.unwrap_or_else(|| {
            if self.activity.presentation != ToolActivityPresentation::Work {
                return ToolActivityKind::None;
            }
            match self.session_evidence.exploration {
                ToolExplorationEvidence::Read | ToolExplorationEvidence::ReadBatch => {
                    return ToolActivityKind::Read;
                }
                ToolExplorationEvidence::Search
                | ToolExplorationEvidence::SearchBatch
                | ToolExplorationEvidence::SearchCompound => {
                    return ToolActivityKind::Search;
                }
                ToolExplorationEvidence::Navigation(_) => return ToolActivityKind::Navigate,
                ToolExplorationEvidence::None => {}
            }
            if self.session_evidence.validation_identity != ToolValidationIdentityKind::None
                || self.policy.captures_validation_output
                || self.execution.is_some_and(|execution| {
                    execution.form == ToolExecutionForm::StructuredValidation
                })
            {
                return ToolActivityKind::Test;
            }
            if self.metadata.effect == ToolEffect::Mutate
                && self.metadata.risk == ToolRisk::ProjectWrite
            {
                return ToolActivityKind::Edit;
            }
            if self.execution.is_some()
                || (self.category == TOOL_CATEGORY_JOB
                    && self.session_evidence.persistent_shell.is_some())
                || self.metadata.effect == ToolEffect::Execute
                || self.metadata.shell_like
                || self.metadata.risk == ToolRisk::JobRun
            {
                return ToolActivityKind::Run;
            }
            if self.session_evidence.review != ToolReviewEvidence::None
                || self.session_evidence.diff_review != ToolDiffReviewEvidence::None
                || self.policy.git_like
                || self.policy.change_summary_like
            {
                return ToolActivityKind::Review;
            }
            if self.metadata.effect == ToolEffect::Observe {
                return ToolActivityKind::Read;
            }
            ToolActivityKind::None
        });
        ToolActivitySemantics {
            presentation: self.activity.presentation,
            interaction: self.activity.interaction,
            kind,
        }
    }

    #[cfg(any(test, feature = "root-test-support"))]
    pub fn requires_explicit_business_session(self) -> bool {
        self.policy.requires_explicit_business_session
    }

    pub fn uses_unit_arguments(self) -> bool {
        self.policy.unit_arguments
    }

    pub fn requires_artifact_upload_path_binding(self) -> bool {
        self.policy.requires_artifact_upload_path_binding
    }

    pub fn approval_policy(self) -> ToolApprovalPolicy {
        self.metadata.approval
    }

    pub fn requires_permission(self) -> bool {
        self.approval_policy().requires_permission()
    }

    pub fn permission_risk(self) -> &'static str {
        if self.captures_validation_output() {
            return PERMISSION_RISK_VALIDATION;
        }
        if let Some(permission_risk) = self.policy.permission_risk {
            return permission_risk;
        }
        permission_risk_from_metadata(self.metadata)
    }
}

fn permission_risk_from_metadata(metadata: ToolMetadata) -> &'static str {
    if metadata.shell_like {
        return PERMISSION_RISK_SHELL;
    }
    if metadata.destructive {
        return PERMISSION_RISK_DESTRUCTIVE;
    }
    if metadata.path_hint == ToolPathHint::Artifact {
        return PERMISSION_RISK_ARTIFACT_WRITE;
    }
    if metadata.path_hint == ToolPathHint::Patch {
        return PERMISSION_RISK_PATCH;
    }
    if matches!(
        metadata.risk,
        ToolRisk::ProjectWrite
            | ToolRisk::SkillManage
            | ToolRisk::MemoryManage
            | ToolRisk::ComputerControl
            | ToolRisk::CommunicationManage
    ) {
        return PERMISSION_RISK_WRITE;
    }
    PERMISSION_RISK_WRITE
}

fn fallback_permission_risk(name: &str, metadata: ToolMetadata) -> &'static str {
    if name.contains("patch") && metadata.path_hint != ToolPathHint::Patch {
        return PERMISSION_RISK_PATCH;
    }
    permission_risk_from_metadata(metadata)
}

pub fn lookup_tool_definition(name: &str) -> Option<&'static ToolDefinition> {
    tool_definitions().find(|definition| definition.name == name)
}

/// Returns the canonical static Stateless Operator extension family for a runtime
/// tool. Unknown and ordinary tools return `None`, so protocol admission fails
/// closed unless a ToolDefinition explicitly declares a family.
pub fn runtime_tool_operator_extension_family(name: &str) -> Option<ToolOperatorExtensionFamily> {
    lookup_tool_definition(name).and_then(|definition| definition.operator_extension_family)
}

/// Returns canonical model-selection semantics for ordinary execution tools.
/// Unknown and non-execution tools return `None`; callers must not infer the
/// contract from names, descriptions, effects, or Runner capabilities.
pub fn runtime_tool_execution_contract(name: &str) -> Option<ToolExecutionContract> {
    lookup_tool_definition(name).and_then(|definition| definition.execution)
}

/// Canonical nested-orchestration scheduling policy. Unknown/non-runtime names
/// fail closed and are never composable by implication from effect metadata.
pub fn runtime_tool_composition_policy(name: &str) -> ToolCompositionPolicy {
    lookup_tool_definition(name)
        .map(|definition| definition.composition)
        .unwrap_or(ToolCompositionPolicy::Denied)
}

pub fn runtime_tool_host_orchestration_hint(name: &str) -> ToolHostOrchestrationHint {
    lookup_tool_definition(name)
        .map(|definition| definition.host_orchestration)
        .unwrap_or(ToolHostOrchestrationHint::UNSPECIFIED)
}

pub fn runtime_tool_session_evidence_policy(name: &str) -> ToolSessionEvidencePolicy {
    lookup_tool_definition(name)
        .map(|definition| definition.session_evidence_policy())
        .unwrap_or(ToolSessionEvidencePolicy::NONE)
}

/// Canonical Activity semantics. Unknown/non-runtime names preserve the legacy
/// conservative interaction default (Meaningful) without inventing a kind.
pub fn runtime_tool_activity_semantics(name: &str) -> ToolActivitySemantics {
    lookup_tool_definition(name)
        .map(|definition| definition.activity_semantics())
        .unwrap_or(ToolActivitySemantics {
            presentation: ToolActivityPresentation::Work,
            interaction: ToolActivityInteraction::Meaningful,
            kind: ToolActivityKind::None,
        })
}

pub fn runtime_tool_activity_interaction(name: &str) -> ToolActivityInteraction {
    runtime_tool_activity_semantics(name).interaction
}

pub fn exploration_tool_names() -> impl Iterator<Item = &'static str> {
    tool_definitions()
        .filter(|definition| definition.session_evidence.exploration.is_exploration())
        .map(|definition| definition.name)
}

/// Audit policy lookup is intentionally optional. Unknown/non-runtime names
/// have no audit contract and callers must fail closed rather than infer one.
pub fn runtime_tool_audit_policy(name: &str) -> Option<ToolAuditPolicy> {
    lookup_tool_definition(name).map(|definition| definition.audit_policy())
}

fn definition_or_metadata_facade(name: &str) -> Result<&'static ToolDefinition, ToolMetadata> {
    lookup_tool_definition(name).ok_or_else(|| fallback_metadata_for_non_runtime_name(name))
}

fn fallback_metadata_for_non_runtime_name(name: &str) -> ToolMetadata {
    // Known runtime names must resolve through ToolDefinition. Non-runtime names
    // receive safe Unknown metadata; ToolCall still rejects them.
    tool_metadata(name)
}

/// Returns `true` if `name` is a recognized runtime tool name.
#[cfg(any(test, feature = "root-test-support"))]
pub fn is_known_tool_name(name: &str) -> bool {
    lookup_tool_definition(name).is_some()
}

#[cfg(any(test, feature = "root-test-support"))]
pub fn known_tool_names() -> impl Iterator<Item = &'static str> {
    tool_definitions().map(|definition| definition.name)
}

pub fn runtime_tool_metadata(name: &str) -> ToolMetadata {
    match definition_or_metadata_facade(name) {
        Ok(definition) => definition.metadata(),
        Err(metadata) => metadata,
    }
}

pub fn runtime_tool_effect_annotations(name: &str) -> ToolEffectAnnotations {
    match definition_or_metadata_facade(name) {
        Ok(definition) => definition.effect_annotations(),
        Err(metadata) => ToolEffectAnnotations {
            read_only_hint: metadata.effect.read_only_hint(),
            destructive_hint: metadata.destructive,
            idempotent_hint: metadata.idempotency.mcp_hint(),
            open_world_hint: metadata.shell_like,
        },
    }
}

pub fn runtime_tool_runner_capability(name: &str) -> Option<RunnerCapabilityRequirement> {
    lookup_tool_definition(name)
        .unwrap_or_else(|| panic!("missing ToolDefinition for {name}"))
        .runner_capability
}

pub fn runtime_tool_category(name: &str) -> &'static str {
    lookup_tool_definition(name)
        .map(|definition| definition.category)
        .unwrap_or("other")
}

pub fn runtime_tool_session_risk_class(name: &str) -> &'static str {
    match definition_or_metadata_facade(name) {
        Ok(definition) => definition.session_risk_class(),
        Err(metadata) => metadata.risk.session_risk_class(),
    }
}

pub fn runtime_tool_is_read_like(name: &str) -> bool {
    match definition_or_metadata_facade(name) {
        Ok(definition) => definition.is_read_like(),
        Err(metadata) => metadata.effect == ToolEffect::Observe,
    }
}

pub fn runtime_tool_is_write_like(name: &str) -> bool {
    match definition_or_metadata_facade(name) {
        Ok(definition) => definition.is_write_like(),
        Err(metadata) => matches!(
            metadata.risk,
            ToolRisk::ProjectWrite
                | ToolRisk::SkillManage
                | ToolRisk::MemoryManage
                | ToolRisk::CommunicationManage
                | ToolRisk::ComputerControl
        ),
    }
}

pub fn runtime_tool_is_shell_like(name: &str) -> bool {
    match definition_or_metadata_facade(name) {
        Ok(definition) => definition.is_shell_like(),
        Err(metadata) => metadata.shell_like || metadata.risk == ToolRisk::JobRun,
    }
}

pub fn runtime_tool_is_git_like(name: &str) -> bool {
    lookup_tool_definition(name).is_some_and(|definition| definition.is_git_like())
}

pub fn runtime_tool_is_change_summary_like(name: &str) -> bool {
    lookup_tool_definition(name).is_some_and(|definition| definition.is_change_summary_like())
}

pub fn runtime_tool_captures_validation_output(name: &str) -> bool {
    lookup_tool_definition(name).is_some_and(|definition| definition.captures_validation_output())
}

#[cfg(any(test, feature = "root-test-support"))]
pub fn runtime_tool_requires_explicit_business_session(name: &str) -> bool {
    lookup_tool_definition(name)
        .is_some_and(|definition| definition.requires_explicit_business_session())
}

pub fn runtime_tool_approval_policy(name: &str) -> ToolApprovalPolicy {
    match definition_or_metadata_facade(name) {
        Ok(definition) => definition.approval_policy(),
        Err(metadata) => metadata.approval,
    }
}

pub fn runtime_tool_requires_permission(name: &str) -> bool {
    match definition_or_metadata_facade(name) {
        Ok(definition) => definition.requires_permission(),
        Err(metadata) => metadata.approval.requires_permission(),
    }
}

pub fn runtime_tool_permission_risk(name: &str) -> &'static str {
    match definition_or_metadata_facade(name) {
        Ok(definition) => definition.permission_risk(),
        Err(metadata) => fallback_permission_risk(name, metadata),
    }
}

pub fn is_model_visible_tool_name(name: &str) -> bool {
    lookup_tool_definition(name).is_some_and(|definition| definition.visibility.is_model_visible())
}

/// Whether an ordinary model-facing result may carry the passive Job-attention
/// sidecar. Explicit Job lifecycle/control surfaces and the Window diagnostic
/// remain self-describing and must not recursively consume the sidecar.
pub fn runtime_tool_supports_passive_job_attention(name: &str) -> bool {
    is_model_visible_tool_name(name)
        && !matches!(
            name,
            "current_window_activity"
                | "plugin_tool"
                | "run_job"
                | "run_detached_process"
                | "observe_jobs"
                | "list_jobs"
                | "stop_job"
                | "wait_for_job_readiness"
                | "wait_for_job_terminal"
                | "present_job_terminal_continuation"
        )
}

#[cfg(any(test, feature = "root-test-support"))]
pub fn is_model_hidden_tool_name(name: &str) -> bool {
    lookup_tool_definition(name).is_some_and(|definition| definition.visibility.is_model_hidden())
}

#[cfg(any(test, feature = "root-test-support"))]
pub fn model_hidden_tool_names() -> impl Iterator<Item = &'static str> {
    tool_definitions()
        .filter(|definition| definition.visibility.is_model_hidden())
        .map(|definition| definition.name)
}

pub fn model_visible_tool_definitions() -> impl Iterator<Item = &'static ToolDefinition> {
    tool_definitions().filter(|definition| definition.visibility.is_model_visible())
}

pub fn runtime_tool_adaptive_direct_rank(name: &str) -> Option<u16> {
    lookup_tool_definition(name)
        .filter(|definition| definition.visibility.is_model_visible())
        .and_then(|definition| definition.adaptive_runtime_direct_rank())
}

pub fn is_adaptive_runtime_direct_tool(name: &str) -> bool {
    runtime_tool_adaptive_direct_rank(name).is_some()
}

pub fn adaptive_runtime_direct_tool_definitions() -> Vec<&'static ToolDefinition> {
    let mut definitions = model_visible_tool_definitions()
        .filter(|definition| definition.adaptive_runtime_direct_rank().is_some())
        .collect::<Vec<_>>();
    definitions.sort_by_key(|definition| {
        (
            definition
                .adaptive_runtime_direct_rank()
                .expect("adaptive direct definition rank"),
            definition.name,
        )
    });
    definitions
}

/// Frozen GPT Actions direct operations retained only for the legacy adapter.
///
/// This snapshot is intentionally independent from Adaptive Runtime direct ranking:
/// adding or re-ranking canonical tools must not grow the retiring GPT Action surface.
pub const LEGACY_GPT_ACTION_DIRECT_TOOL_NAMES: &[&str] = &[
    "work_on_project",
    "session_discussion_summary",
    "session_handoff_summary",
    "runtime_status",
    "wait_for_agent_events",
    "plugin_tool",
    "skill_load",
    "tool_manifest",
    "search_project_texts",
    "read_files",
    "search_and_read",
    "import_conversation_files_to_project",
    "project_artifact",
    "edit_project_files",
    "run_process",
    "run_skill_resource",
    "run_script",
    "run_shell",
    "wait_for_job_terminal",
    "observe_jobs",
    "cargo_check",
    "cargo_test",
    "review_changes",
    "show_changes",
];

/// Frozen legacy GPT Actions admission snapshot.
///
/// New canonical/model-visible tools are deliberately absent until this adapter is
/// retired; they remain available through the maintained MCP/Host surfaces.
pub const LEGACY_GPT_ACTION_SUPPORTED_TOOL_NAMES: &[&str] = &[
    "list_tools",
    "list_external_observations",
    "work_on_project",
    "finish_coding_task",
    "session_summary",
    "update_session_context",
    "close_session",
    "validation_summary",
    "post_session_message",
    "post_peer_message",
    "list_session_messages",
    "get_session_assignment",
    "observe_session_messages",
    "resolve_session_message",
    "complete_session_message",
    "session_discussion_summary",
    "session_handoff_summary",
    "create_agent_identity",
    "list_agent_identities",
    "update_agent_identity",
    "attach_agent_endpoint",
    "bootstrap_agent_conversation",
    "detach_agent_endpoint",
    "create_conversation",
    "list_conversations",
    "read_conversation",
    "post_conversation_message",
    "list_agent_inbox",
    "consume_agent_deliveries",
    "consume_agent_wake",
    "prepare_goal_workflow",
    "create_goal",
    "get_goal",
    "list_goals",
    "checkpoint_goal",
    "update_goal",
    "associate_goal_agent_task",
    "associate_goal_workflow_session",
    "create_agent_task",
    "list_agent_tasks",
    "read_agent_task",
    "assign_agent_task",
    "start_agent_task_attempt",
    "start_agent_task_endpoint_continuation",
    "start_agent_task_coding_run",
    "reconcile_agent_task_coding_run",
    "heartbeat_agent_task_attempt",
    "complete_agent_task_attempt",
    "wait_for_agent_events",
    "read_agent_wait",
    "cancel_agent_wait",
    "skill_load",
    "run_skill_resource",
    "workspace_hygiene_check",
    "coding_agent_start",
    "coding_agent_observe",
    "coding_agent_cancel",
    "browser_observe",
    "browser_act",
    "computer_observe",
    "computer_control",
    "computer_save_snapshot",
    "list_projects",
    "register_project",
    "unregister_project",
    "create_project",
    "list_runners",
    "runtime_status",
    "current_window_activity",
    "tool_manifest",
    "runner_config_check",
    "runner_config_reload",
    "ssh_resource",
    "plugin_tool",
    "run_process",
    "run_detached_process",
    "run_script",
    "run_shell",
    "open_session_shell",
    "session_shell_exec",
    "session_shell_status",
    "close_session_shell",
    "run_job",
    "stop_job",
    "observe_jobs",
    "wait_for_job_terminal",
    "project_overview",
    "list_project_files",
    "list_project_tracked_files",
    "search_project_texts",
    "search_and_read",
    "git_review_summary",
    "review_changes",
    "show_changes",
    "list_jobs",
    "read_files",
    "lsp_status",
    "document_symbols",
    "document_diagnostics",
    "hover",
    "workspace_symbols",
    "goto_definition",
    "find_references",
    "call_hierarchy",
    "git_commit_paths",
    "git_status",
    "git_diff_hunks",
    "git_log",
    "cargo_fmt",
    "cargo_check",
    "cargo_test",
    "go_test",
    "delete_project_files",
    "git_restore_paths",
    "discard_untracked",
    "save_project_artifact",
    "import_conversation_files_to_project",
    "transfer_project_artifact",
    "project_artifact",
    "read_project_artifact_metadata",
    "read_project_artifact",
    "artifact_upload_begin",
    "artifact_upload_chunk",
    "artifact_upload_finish",
    "artifact_upload_abort",
    "edit_project_files",
    "apply_patch",
    "apply_unified_diff",
    "write_project_file",
];

pub fn gpt_action_direct_tool_definitions() -> Vec<&'static ToolDefinition> {
    LEGACY_GPT_ACTION_DIRECT_TOOL_NAMES
        .iter()
        .filter_map(|name| lookup_tool_definition(name))
        .filter(|definition| definition.supports_gpt_actions())
        .collect()
}

pub fn gpt_action_tool_is_direct(tool_name: &str) -> bool {
    LEGACY_GPT_ACTION_DIRECT_TOOL_NAMES.contains(&tool_name) && gpt_action_tool_supported(tool_name)
}

/// Admission predicate shared by the legacy GPT Action direct/gateway adapters.
/// Membership is frozen; new canonical tools never enter this surface implicitly.
pub fn gpt_action_tool_supported(tool_name: &str) -> bool {
    LEGACY_GPT_ACTION_SUPPORTED_TOOL_NAMES.contains(&tool_name)
        && lookup_tool_definition(tool_name).is_some_and(|definition| {
            definition.gpt_action_exposure() != ToolGptActionExposure::Unsupported
        })
}

pub fn gpt_action_supported_tool_names() -> &'static [&'static str] {
    LEGACY_GPT_ACTION_SUPPORTED_TOOL_NAMES
}
pub fn model_visible_tool_names_csv() -> String {
    model_visible_tool_definitions()
        .map(|definition| definition.name)
        .collect::<Vec<_>>()
        .join(", ")
}
