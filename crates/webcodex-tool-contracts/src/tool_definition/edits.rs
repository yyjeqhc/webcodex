use super::RunnerCapabilityRequirement::FileWrite;
use super::ToolVisibility::ModelVisible;
use super::{
    adaptive_runtime_direct, def, model_spec, permission_risk, ToolDefinition,
    PERMISSION_RISK_WRITE, TOOL_CATEGORY_EDIT,
};
use crate::metadata::{
    ToolPathHint::{PathList, SinglePath},
    ToolRisk::ProjectWrite,
    PROJECT_WRITE, TOOL_PROVIDER_RUNNER,
};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    permission_risk(
        model_spec(
            def(
            "write_project_file",
            super::ToolAuditPolicy::TYPED_CANONICAL,
            ModelVisible,
            TOOL_CATEGORY_EDIT,
            Some(FileWrite),
            TOOL_PROVIDER_RUNNER,
            super::ToolSemanticContract {
                effect: super::ToolEffect::Mutate,
                risk: ProjectWrite,
                approval: super::ToolApprovalPolicy::Standard,
                idempotency: super::ToolIdempotency::NonIdempotent,
            },
            Some(PROJECT_WRITE),
            true,
            SinglePath,
            true,
            false,
            super::ToolSessionEvidencePolicy::NONE,
            ),
            "Create a new file or perform an intentional whole-file replacement. Existing-file replacement requires expected_read_revision from read_files; ToolRuntime resolves that model-facing snapshot handle to the Runner guard, so the model does not copy a digest. Failures expose minimal error facts and at most one parser-ready read_files recovery call when a fresh read is required. Choose this path when whole-file replacement is genuinely the clearest reliable mutation, then inspect the resulting diff and validate the final source.",
        ),
        PERMISSION_RISK_WRITE,
    ),
    adaptive_runtime_direct(
        permission_risk(
            model_spec(
                def(
                "apply_text_edits",
                super::ToolAuditPolicy::TYPED_CANONICAL,
                ModelVisible,
                TOOL_CATEGORY_EDIT,
                Some(FileWrite),
                TOOL_PROVIDER_RUNNER,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Mutate,
                    risk: ProjectWrite,
                    approval: super::ToolApprovalPolicy::Standard,
                    idempotency: super::ToolIdempotency::NonIdempotent,
                },
                Some(PROJECT_WRITE),
                true,
                PathList,
                true,
                false,
                super::ToolSessionEvidencePolicy::NONE,
                )
                .with_composition_policy(super::ToolCompositionPolicy::Sequential)
                .with_host_orchestration_hint(
                    super::ToolHostOrchestrationHint::sequential()
                        .with_native_batch_field("changes"),
                ),
                "Small/local exact edits use a transactional structured option: use ONE change per file with edits. Globally unique edits may omit expected_read_revision; occurrence or line_scope requires expected_read_revision; occurrence selects one match in global source order; revisions fence whole-file snapshots; model input never needs a digest. For same old_text in an explicit bounded file when exact cardinality is known, use replace_exact expected_match_count=N (1..=1024), scoped if needed; never occurrence. If uncertain, optional dry_run; if obvious, apply directly—dry_run is not ritual. Batches are preflighted transactionally; conflicts fail closed; Runner rechecks source before mutation. change_summary/changed_files/resolved_matches confirm mechanical scope; do not diff only to recount. Summary is not semantic review: use show_changes, git_diff_hunks, or git_review_summary. On stale state use one parser-ready read_files recovery call; inspect the resulting diff if review is needed; validate the final source.",
            ).with_gpt_action_description("Transactional exact edits. Known same old_text + bounded exact cardinality: expected_match_count=N; uncertain count/range: optional dry_run; obvious count: apply directly. change_summary is mechanical scope only; use Git review tools for semantic review. Canonical guards and rollback remain."),
            PERMISSION_RISK_WRITE,
        ),
        60,
    ),
];
