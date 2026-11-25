use super::RunnerCapabilityRequirement::DetachedProcess;
use super::ToolVisibility::ModelHidden;
use super::*;
use crate::metadata::{
    ToolPathHint::None as NoPath,
    ToolRisk::{JobRun, Read},
    JOB_RUN, PROJECT_READ, TOOL_PROVIDER_RUNNER,
};
use webcodex_core::authority::SCOPE_JOB_DETACH;

const fn resource_tool(
    name: &'static str,
    description: &'static str,
    rank: u16,
    reason: ToolDirectReason,
) -> ToolDefinition {
    adaptive_runtime_direct(
        model_spec(
            require_any_scopes(
                control_def(
                    name,
                    ToolAuditPolicy::TYPED_CANONICAL.session_input(
                        ToolAuditSessionInputPolicy::OmitTopLevel(&["uri", "query"]),
                    ),
                    ToolVisibility::ModelVisible,
                    TOOL_CATEGORY_PROJECT,
                    ToolSemanticContract {
                        effect: ToolEffect::Observe,
                        risk: Read,
                        approval: ToolApprovalPolicy::None,
                        idempotency: ToolIdempotency::PureRead,
                    },
                    None,
                    false,
                ),
                &[
                    crate::metadata::PROJECT_READ,
                    crate::metadata::COMMUNICATION_READ,
                ],
            )
            .with_activity(
                ToolActivityPresentation::Support,
                ToolActivityInteraction::NonMeaningful,
            ),
            description,
        ),
        rank,
        reason,
    )
}

const PREVIEW_AUDIT_FIELDS: &[ToolAuditResultField] = &[
    ToolAuditResultField::value("job_id"),
    ToolAuditResultField::value("status"),
    ToolAuditResultField::value("port"),
    ToolAuditResultField::value("project"),
    ToolAuditResultField::value("expires_at"),
    ToolAuditResultField::value("failure_kind"),
];

const fn start_web_preview_tool() -> ToolDefinition {
    require_all_scopes(
        model_spec(
            ToolDeclaration {
                name: "start_web_preview",
                audit: ToolAuditPolicy::typed_fields(PREVIEW_AUDIT_FIELDS)
                    .session_input(ToolAuditSessionInputPolicy::OmitTopLevel(&["idempotency_key"])),
                visibility: ModelHidden,
                category: TOOL_CATEGORY_EXECUTION,
                runner_capability: Some(DetachedProcess),
                provider_id: TOOL_PROVIDER_RUNNER,
                semantic: ToolSemanticContract {
                    effect: ToolEffect::Execute,
                    risk: JobRun,
                    approval: ToolApprovalPolicy::Standard,
                    idempotency: ToolIdempotency::Keyed,
                },
                required_scope: Some(JOB_RUN),
                requires_project: true,
                path_hint: NoPath,
                destructive: false,
                shell_like: false,
                session_evidence: ToolSessionEvidencePolicy::NONE,
            }
            .define(),
            "Start an interactive HTTPS preview for one explicitly selected Runner Project and local HTTP port. Requires project:read, job:run, detached-process authority, and a Runner with detached_process_jobs. The Runner must have Node.js 22+ and an HTTP server already listening on 127.0.0.1:<port>. The Server creates a 30-minute gateway session and starts one supervisor-owned preview agent; the agent token is delivered through stdin and never returned. Returns only after the agent connects. Reuse the same idempotency_key for an uncertain retry; a different key starts a different preview. The preview URL is a temporary bearer capability. No public inbound Runner port is opened.",
        )
        .with_execution(ToolExecutionContract::new(
            ToolExecutionForm::NativeArgv,
            ToolExecutionLifetime::Supervisor,
            ToolExecutionStart::AsyncImmediate,
            ToolExecutionContinuation::ObserveJobs,
        )),
        &[PROJECT_READ, JOB_RUN, SCOPE_JOB_DETACH],
    )
}

// Authorization is selected by resource kind inside the canonical service. A common
// project/communication/session scope here would incorrectly disable other domains.
pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    resource_tool("open_webcodex_workbench", "Open the WebCodex Workbench, optionally selecting Runner/Project/Session or showing one temporary HTTPS preview_url from the configured preview domain. UI context remains reauthorized; no Session creation or execution.", 152, ToolDirectReason::Presentation),
    start_web_preview_tool(),
    resource_tool("search_webcodex_resources", "Find authorized links: client_id filters Projects; files need project, artifacts need project+session_id. Literal query precedes paging; incomplete sources stay explicit. Links grant no authority.", 154, ToolDirectReason::CoreWorkflow),
    resource_tool("read_webcodex_resource", "Read the latest bounded content or metadata of one exact webcodex-resource:// reference. Re-authorizes the original domain; pinned project roots, deleted paths and inaccessible Goals fail closed. Artifacts reuse file identity; historical observations are not current facts.", 153, ToolDirectReason::CoreWorkflow),
];
