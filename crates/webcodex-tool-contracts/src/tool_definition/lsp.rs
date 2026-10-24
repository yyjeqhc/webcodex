use super::RunnerCapabilityRequirement::{LspCallHierarchy, LspReadOnlyNavigation};
use super::{model_spec, runner_project_read_def, ToolDefinition, TOOL_CATEGORY_LSP};
use crate::metadata::ToolPathHint::{None as NoPath, SinglePath};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    model_spec(
        runner_project_read_def(
            "get_lsp_status",
            super::ToolAuditPolicy::TYPED_CANONICAL,
            TOOL_CATEGORY_LSP,
            LspReadOnlyNavigation,
            NoPath,
            super::ToolSessionEvidencePolicy::NONE,
        )
        .with_activity_kind(super::ToolActivityKind::Navigate),
        "Read-only probe of configured Runner-side language-server availability for a Project. Does not start a language server, run checks, or execute Project code. Returns detected languages and availability/running status and independent index_readiness (unknown/indexing/ready/unhealthy/unsupported) without absolute executable paths.",
    ),
    model_spec(
        runner_project_read_def(
            "list_document_symbols",
            super::ToolAuditPolicy::TYPED_CANONICAL,
            TOOL_CATEGORY_LSP,
            LspReadOnlyNavigation,
            SinglePath,
            super::ToolSessionEvidencePolicy::NONE.exploration(super::ToolExplorationEvidence::Navigation(super::ToolNavigationEvidenceKind::DocumentSymbols)),
        ),
        "Read-only hierarchical document symbols for a Project-relative supported source file via its configured Runner-side language server. Returns Project-relative paths, 1-based Unicode scalar columns, and bounded pre-order results. External or invalid ranges are omitted.",
    ),
    model_spec(
        runner_project_read_def(
            "read_document_diagnostics",
            super::ToolAuditPolicy::TYPED_CANONICAL,
            TOOL_CATEGORY_LSP,
            LspReadOnlyNavigation,
            SinglePath,
            super::ToolSessionEvidencePolicy::NONE.exploration(super::ToolExplorationEvidence::Navigation(super::ToolNavigationEvidenceKind::DocumentDiagnostics)),
        ),
        "Read-only bounded diagnostics for a Project-relative supported source file via Runner-side publishDiagnostics. Returns normalized 1-based Unicode scalar ranges and explicit freshness/timeout state; it does not run a Project check.",
    ),
    model_spec(
        runner_project_read_def(
            "read_symbol_hover",
            super::ToolAuditPolicy::TYPED_CANONICAL,
            TOOL_CATEGORY_LSP,
            LspReadOnlyNavigation,
            SinglePath,
            super::ToolSessionEvidencePolicy::NONE.exploration(super::ToolExplorationEvidence::Navigation(super::ToolNavigationEvidenceKind::Hover)),
        ),
        "Read a symbol's type and documentation at a 1-based Unicode scalar position through the Project's language server; no source edit. MarkupContent and MarkedString forms are normalized to bounded markdown/plaintext; invalid optional ranges are omitted. Rust index readiness is required within the shared request deadline; an unready index returns an error, not an empty result. Retry this read-only observation later.",
    ).with_discovery_keywords(&["symbol", "documentation", "type", "signature", "hover", "符号", "文档", "类型", "签名", "悬浮"]),
    model_spec(
        runner_project_read_def(
            "list_workspace_symbols",
            super::ToolAuditPolicy::TYPED_CANONICAL.session_input(
                super::ToolAuditSessionInputPolicy::OmitTopLevel(&["query"]),
            ),
            TOOL_CATEGORY_LSP,
            LspReadOnlyNavigation,
            NoPath,
            super::ToolSessionEvidencePolicy::NONE.exploration(super::ToolExplorationEvidence::Navigation(super::ToolNavigationEvidenceKind::WorkspaceSymbols)),
        ),
        "Read-only bounded workspace/symbol query via configured Runner-side language servers. Requires a non-empty 1..200 character query; results are workspace-filtered, sorted, deduplicated, and use Project-relative paths only. Rust index readiness is required within the shared request deadline; an unready index returns an error, not an empty result. Retry this read-only observation later.",
    ),
    model_spec(
        runner_project_read_def(
            "find_definition",
            super::ToolAuditPolicy::TYPED_CANONICAL,
            TOOL_CATEGORY_LSP,
            LspReadOnlyNavigation,
            SinglePath,
            super::ToolSessionEvidencePolicy::NONE.exploration(super::ToolExplorationEvidence::Navigation(super::ToolNavigationEvidenceKind::Locations)),
        ),
        "Read-only goto-definition for a Project-relative supported source file at a 1-based Unicode scalar position via its configured Runner-side language server. Supports Location, Location[], and LocationLink[]; external dependency results are omitted. Rust index readiness is required within the shared request deadline; an unready index returns an error, not an empty result. Retry this read-only observation later.",
    ),
    model_spec(
        runner_project_read_def(
            "find_references",
            super::ToolAuditPolicy::TYPED_CANONICAL,
            TOOL_CATEGORY_LSP,
            LspReadOnlyNavigation,
            SinglePath,
            super::ToolSessionEvidencePolicy::NONE.exploration(super::ToolExplorationEvidence::Navigation(super::ToolNavigationEvidenceKind::Locations)),
        ),
        "Read-only find-references for a Project-relative supported source file at a 1-based Unicode scalar position via its configured Runner-side language server. Results are deduplicated and truncated on the Runner; external/invalid locations are counted separately. Rust index readiness is required within the shared request deadline; an unready index returns an error, not an empty result. Retry this read-only observation later.",
    ),
    model_spec(
        runner_project_read_def(
            "read_call_hierarchy",
            super::ToolAuditPolicy::TYPED_CANONICAL,
            TOOL_CATEGORY_LSP,
            LspCallHierarchy,
            SinglePath,
            super::ToolSessionEvidencePolicy::NONE.exploration(super::ToolExplorationEvidence::Navigation(super::ToolNavigationEvidenceKind::CallHierarchy)),
        ),
        "Read-only bounded call hierarchy for a project-relative supported source position. The Runner performs prepare plus incoming/outgoing breadth-first traversal to depth 1 or 2, returning only normalized project-local symbols and globally bounded edges.",
    ),
];
