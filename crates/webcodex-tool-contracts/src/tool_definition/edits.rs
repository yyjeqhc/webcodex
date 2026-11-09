use super::RunnerCapabilityRequirement::FileWrite;
use super::ToolVisibility::{ModelHidden, ModelVisible};
use super::{
    adaptive_runtime_direct, model_spec, permission_risk, ToolDeclaration, ToolDefinition,
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
            ToolDeclaration {
                name: "write_project_file",
                audit: super::ToolAuditPolicy::TYPED_CANONICAL,
                visibility: ModelHidden,
                category: TOOL_CATEGORY_EDIT,
                runner_capability: Some(FileWrite),
                provider_id: TOOL_PROVIDER_RUNNER,
                semantic: super::ToolSemanticContract {
                    effect: super::ToolEffect::Mutate,
                    risk: ProjectWrite,
                    approval: super::ToolApprovalPolicy::Standard,
                    idempotency: super::ToolIdempotency::NonIdempotent,
                },
                required_scope: Some(PROJECT_WRITE),
                requires_project: true,
                path_hint: SinglePath,
                destructive: true,
                shell_like: false,
                session_evidence: super::ToolSessionEvidencePolicy::NONE,
            }.define(),
            "Create a new file or perform an intentional whole-file replacement. Existing-file replacement requires expected_read_revision from read_files; ToolRuntime resolves that model-facing snapshot handle to the Runner guard, so the model does not copy a digest. Failures expose minimal error facts and at most one parser-ready read_files recovery call when a fresh read is required. Choose this path when whole-file replacement is genuinely the clearest reliable mutation, then inspect the resulting diff and validate the final source.",
        ).with_discovery_keywords(&["create", "write", "overwrite", "file", "contents", "创建", "写入", "覆盖", "文件", "内容"]),
        PERMISSION_RISK_WRITE,
    ),
    adaptive_runtime_direct(
        permission_risk(
            model_spec(
                ToolDeclaration {
                    name: "edit_project_files",
                    audit: super::ToolAuditPolicy::TYPED_CANONICAL,
                    visibility: ModelVisible,
                    category: TOOL_CATEGORY_EDIT,
                    runner_capability: Some(FileWrite),
                    provider_id: TOOL_PROVIDER_RUNNER,
                    semantic: super::ToolSemanticContract {
                        effect: super::ToolEffect::Mutate,
                        risk: ProjectWrite,
                        approval: super::ToolApprovalPolicy::Standard,
                        idempotency: super::ToolIdempotency::NonIdempotent,
                    },
                    required_scope: Some(PROJECT_WRITE),
                    requires_project: true,
                    path_hint: PathList,
                    destructive: true,
                    shell_like: false,
                    session_evidence: super::ToolSessionEvidencePolicy::NONE,
                }.define()
                .with_composition_policy(super::ToolCompositionPolicy::Sequential)
                .with_host_orchestration_hint(
                    super::ToolHostOrchestrationHint::sequential()
                        .with_native_batch_field("changes"),
                ),
                "Project editor: one change per file; kind defaults to edit and edit kind to replace_exact. Paths may be Project-relative or absolute inside its root. create needs content; write replaces an existing file with content. write/delete/rename, insertions and positional or multiple-match edits require expected_read_revision from read_files. A globally unique replace_exact/delete_exact may omit the revision; provided revisions always reject stale content. Exact edits fail closed on ambiguity; replace_range uses 1-based inclusive lines from that snapshot. Batches preflight transactionally; Runner rechecks source. dry_run plans only. Stale state returns read_files recovery; outcome_unknown: observe before writing. Use review_changes when Git review is useful, then task-appropriate validation.",
            ).with_discovery_keywords(&["replace", "exact", "snippets", "edit", "existing", "files", "text", "insert", "delete", "rename", "move", "替换", "编辑", "修改", "文字", "文本", "插入", "删除", "重命名", "移动", "文件"]),
            PERMISSION_RISK_WRITE,
        ),
        60,
        super::ToolDirectReason::CoreWorkflow,
    ),
];
