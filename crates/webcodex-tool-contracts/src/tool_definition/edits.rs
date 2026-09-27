use super::RunnerCapabilityRequirement::FileWrite;
use super::ToolVisibility::{ModelHidden, ModelVisible};
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
            ModelHidden,
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
                "edit_project_files",
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
                "Primary project editor: read_files → edit_project_files → show_changes → structured validation. Use ONE change per file. edit/delete/rename require expected_read_revision; create requires content. Exact edits fail closed on ambiguity; replace_range edits 1-based inclusive lines from the same original snapshot. Batches are preflighted transactionally and the Runner rechecks source before mutation. dry_run plans only. Stale state returns read_files recovery; outcome_unknown requires workspace observation before another write.",
            ).with_gpt_action_description("Read files, then edit/create/delete/rename transactionally. Existing sources require expected_read_revision. Exact edits fail closed on ambiguity; stale source requires reread. Optional dry_run. Review changes and validate. Unknown outcomes require workspace observation before another write."),
            PERMISSION_RISK_WRITE,
        ),
        60,
    ),
];
