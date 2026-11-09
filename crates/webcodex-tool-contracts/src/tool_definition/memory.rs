use super::ToolVisibility::ModelHidden;
use super::{
    control_def, require_all_scopes, ToolDeclaration, ToolDefinition, ToolOperatorExtensionFamily,
    TOOL_CATEGORY_MEMORY,
};
use crate::metadata::{
    ToolPathHint::None as NoPath,
    ToolRisk::{MemoryManage, Read},
    ADMIN, PROJECT_READ, PROJECT_WRITE, TOOL_PROVIDER_CONTROL,
};
use webcodex_core::authority::{MEMORY_MANAGE_SCOPES, MEMORY_READ_SCOPES};

/// Fixed Control-owned project Memory runtime contract. These tools remain
/// globally hidden and are projected only by the capable Stateless MCP Full
/// Operator surface; the kernel capability gate is authoritative.
pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    require_all_scopes(
        ToolDeclaration {
            name: "search_memory",
            audit: super::ToolAuditPolicy::typed_fields(&[
                super::ToolAuditResultField::value("project"),
                super::ToolAuditResultField::value("catalog_revision"),
                super::ToolAuditResultField::value("total_count"),
                super::ToolAuditResultField::value("returned_count"),
                super::ToolAuditResultField::value("truncated"),
                super::ToolAuditResultField::value("error_kind"),
                super::ToolAuditResultField::value("state_changed"),
            ])
            .context(super::ToolAuditContextPolicy::Fields(&[
                super::ToolAuditResultField::value("catalog_revision"),
                super::ToolAuditResultField::value("total_count"),
                super::ToolAuditResultField::value("returned_count"),
                super::ToolAuditResultField::value("offset"),
                super::ToolAuditResultField::value("next_offset"),
                super::ToolAuditResultField::value("truncated"),
                super::ToolAuditResultField::value("error_kind"),
                super::ToolAuditResultField::value("state_changed"),
            ])),
            visibility: ModelHidden,
            category: TOOL_CATEGORY_MEMORY,
            runner_capability: None,
            provider_id: TOOL_PROVIDER_CONTROL,
            semantic: super::ToolSemanticContract {
                effect: super::ToolEffect::Observe,
                risk: Read,
                approval: super::ToolApprovalPolicy::None,
                idempotency: super::ToolIdempotency::PureRead,
            },
            required_scope: Some(PROJECT_READ),
            requires_project: true,
            path_hint: NoPath,
            destructive: false,
            shell_like: false,
            session_evidence: super::ToolSessionEvidencePolicy::NONE,
        }
        .define(),
        MEMORY_READ_SCOPES,
    )
    .with_operator_extension_family(ToolOperatorExtensionFamily::MemoryRuntime),
    require_all_scopes(
        ToolDeclaration {
            name: "read_memory",
            audit: super::ToolAuditPolicy::typed_fields(&[
                super::ToolAuditResultField::value("project"),
                super::ToolAuditResultField::value("memory_id"),
                super::ToolAuditResultField::value("memory_key"),
                super::ToolAuditResultField::value("revision"),
                super::ToolAuditResultField::value("bootstrap"),
                super::ToolAuditResultField::value("priority"),
                super::ToolAuditResultField::string_bytes("returned_body_bytes", "body"),
                super::ToolAuditResultField::value("error_kind"),
                super::ToolAuditResultField::value("state_changed"),
            ])
            .context(super::ToolAuditContextPolicy::Fields(&[
                super::ToolAuditResultField::value("memory_id"),
                super::ToolAuditResultField::value("memory_key"),
                super::ToolAuditResultField::value("revision"),
                super::ToolAuditResultField::value("bootstrap"),
                super::ToolAuditResultField::value("priority"),
                super::ToolAuditResultField::string_bytes("returned_body_bytes", "body"),
                super::ToolAuditResultField::value("error_kind"),
                super::ToolAuditResultField::value("state_changed"),
            ])),
            visibility: ModelHidden,
            category: TOOL_CATEGORY_MEMORY,
            runner_capability: None,
            provider_id: TOOL_PROVIDER_CONTROL,
            semantic: super::ToolSemanticContract {
                effect: super::ToolEffect::Observe,
                risk: Read,
                approval: super::ToolApprovalPolicy::None,
                idempotency: super::ToolIdempotency::PureRead,
            },
            required_scope: Some(PROJECT_READ),
            requires_project: true,
            path_hint: NoPath,
            destructive: false,
            shell_like: false,
            session_evidence: super::ToolSessionEvidencePolicy::NONE,
        }
        .define(),
        MEMORY_READ_SCOPES,
    )
    .with_operator_extension_family(ToolOperatorExtensionFamily::MemoryRuntime),
    require_all_scopes(
        ToolDeclaration {
            name: "set_memory",
            audit: super::ToolAuditPolicy::typed_fields(&[
                super::ToolAuditResultField::value("project"),
                super::ToolAuditResultField::value("memory_id"),
                super::ToolAuditResultField::value("memory_key"),
                super::ToolAuditResultField::value("old_revision"),
                super::ToolAuditResultField::value("revision"),
                super::ToolAuditResultField::value("created"),
                super::ToolAuditResultField::value("error_kind"),
                super::ToolAuditResultField::value("state_changed"),
            ])
            .context(super::ToolAuditContextPolicy::Fields(&[
                super::ToolAuditResultField::value("memory_id"),
                super::ToolAuditResultField::value("memory_key"),
                super::ToolAuditResultField::value("old_revision"),
                super::ToolAuditResultField::value("revision"),
                super::ToolAuditResultField::value("created"),
                super::ToolAuditResultField::value("error_kind"),
                super::ToolAuditResultField::value("state_changed"),
            ])),
            visibility: ModelHidden,
            category: TOOL_CATEGORY_MEMORY,
            runner_capability: None,
            provider_id: TOOL_PROVIDER_CONTROL,
            semantic: super::ToolSemanticContract {
                effect: super::ToolEffect::Mutate,
                risk: MemoryManage,
                approval: super::ToolApprovalPolicy::Standard,
                idempotency: super::ToolIdempotency::NonIdempotent,
            },
            required_scope: Some(PROJECT_WRITE),
            requires_project: true,
            path_hint: NoPath,
            destructive: false,
            shell_like: false,
            session_evidence: super::ToolSessionEvidencePolicy::NONE,
        }
        .define(),
        MEMORY_MANAGE_SCOPES,
    )
    .with_operator_extension_family(ToolOperatorExtensionFamily::MemoryManagement),
    require_all_scopes(
        ToolDeclaration {
            name: "delete_memory",
            audit: super::ToolAuditPolicy::typed_fields(&[
                super::ToolAuditResultField::value("project"),
                super::ToolAuditResultField::value("memory_id"),
                super::ToolAuditResultField::value("memory_key"),
                super::ToolAuditResultField::value("revision"),
                super::ToolAuditResultField::value("deleted"),
                super::ToolAuditResultField::value("error_kind"),
                super::ToolAuditResultField::value("state_changed"),
            ])
            .context(super::ToolAuditContextPolicy::Fields(&[
                super::ToolAuditResultField::value("memory_id"),
                super::ToolAuditResultField::value("memory_key"),
                super::ToolAuditResultField::value("revision"),
                super::ToolAuditResultField::value("deleted"),
                super::ToolAuditResultField::value("error_kind"),
                super::ToolAuditResultField::value("state_changed"),
            ])),
            visibility: ModelHidden,
            category: TOOL_CATEGORY_MEMORY,
            runner_capability: None,
            provider_id: TOOL_PROVIDER_CONTROL,
            semantic: super::ToolSemanticContract {
                effect: super::ToolEffect::Mutate,
                risk: MemoryManage,
                approval: super::ToolApprovalPolicy::Standard,
                idempotency: super::ToolIdempotency::NonIdempotent,
            },
            required_scope: Some(PROJECT_WRITE),
            requires_project: true,
            path_hint: NoPath,
            destructive: true,
            shell_like: false,
            session_evidence: super::ToolSessionEvidencePolicy::NONE,
        }
        .define(),
        MEMORY_MANAGE_SCOPES,
    )
    .with_operator_extension_family(ToolOperatorExtensionFamily::MemoryManagement),
    control_def(
        "list_memory_scopes",
        super::ToolAuditPolicy::typed_fields(&[
            super::ToolAuditResultField::value("total_count"),
            super::ToolAuditResultField::value("returned_count"),
            super::ToolAuditResultField::value("truncated"),
            super::ToolAuditResultField::value("error_kind"),
            super::ToolAuditResultField::value("state_changed"),
        ])
        .context_from_result(),
        ModelHidden,
        TOOL_CATEGORY_MEMORY,
        super::ToolSemanticContract {
            effect: super::ToolEffect::Observe,
            risk: Read,
            approval: super::ToolApprovalPolicy::None,
            idempotency: super::ToolIdempotency::PureRead,
        },
        Some(ADMIN),
        false,
    )
    .with_operator_extension_family(ToolOperatorExtensionFamily::MemoryManagement),
    control_def(
        "purge_memory_scope",
        super::ToolAuditPolicy::typed_fields(&[
            super::ToolAuditResultField::value("memory_scope_id"),
            super::ToolAuditResultField::value("catalog_revision"),
            super::ToolAuditResultField::value("current_catalog_revision"),
            super::ToolAuditResultField::value("purged_count"),
            super::ToolAuditResultField::value("error_kind"),
            super::ToolAuditResultField::value("state_changed"),
        ])
        .context_from_result(),
        ModelHidden,
        TOOL_CATEGORY_MEMORY,
        super::ToolSemanticContract {
            effect: super::ToolEffect::Mutate,
            risk: MemoryManage,
            approval: super::ToolApprovalPolicy::Standard,
            idempotency: super::ToolIdempotency::NonIdempotent,
        },
        Some(ADMIN),
        true,
    )
    .with_operator_extension_family(ToolOperatorExtensionFamily::MemoryManagement),
];
