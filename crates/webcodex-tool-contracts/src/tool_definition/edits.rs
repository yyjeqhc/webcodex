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
        ).with_discovery_keywords(&["create", "write", "overwrite", "file", "contents", "创建", "写入", "覆盖", "文件", "内容"]),
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
                "Project editor: read_files → edit_project_files. Use review_changes when Git review is useful, then task-appropriate validation. One change per file. For existing-file edits/deletes/renames, set expected_read_revision from read_files.read_revision; create requires content. Exact edits fail closed on ambiguity; replace_range edits 1-based inclusive lines from that snapshot. Batches preflight transactionally; Runner rechecks source before mutation. dry_run plans only. Stale state returns read_files recovery; outcome_unknown requires workspace observation before writing.",
            ).with_discovery_keywords(&["replace", "exact", "snippets", "edit", "existing", "files", "text", "insert", "delete", "rename", "move", "替换", "编辑", "修改", "文字", "文本", "插入", "删除", "重命名", "移动", "文件"]),
            PERMISSION_RISK_WRITE,
        ),
        60,
        super::ToolDirectReason::CoreWorkflow,
    ),
];
