use super::RunnerCapabilityRequirement::{FileRead, Shell};
use super::ToolVisibility::ModelVisible;
use super::{
    adaptive_runtime_direct, def, model_spec, ToolDefinition, TOOL_CATEGORY_FILE,
    TOOL_CATEGORY_PROJECT,
};
use crate::metadata::{
    ToolPathHint::None as NoPath, ToolRisk::Read, PROJECT_READ, TOOL_PROVIDER_RUNNER,
};

pub(super) const SEARCH_DEFINITIONS: &[ToolDefinition] = &[
    model_spec(
        def(
            "project_overview",
            super::ToolAuditPolicy::TYPED_CANONICAL,
            ModelVisible,
            TOOL_CATEGORY_PROJECT,
            Some(FileRead),
            TOOL_PROVIDER_RUNNER,
            super::ToolSemanticContract {
                effect: super::ToolEffect::Observe,
                risk: Read,
                approval: super::ToolApprovalPolicy::None,
                idempotency: super::ToolIdempotency::PureRead,
            },
            Some(PROJECT_READ),
            true,
            NoPath,
            false,
            false,
            super::ToolSessionEvidencePolicy::NONE.review(super::ToolReviewEvidence::ReadOnlyInspection),
        ).with_composition_policy(super::ToolCompositionPolicy::Parallel),
        "Deterministic, bounded, metadata-only overview of an unfamiliar project: conventional project types, manifests, key files, roots, and direct children. Reads no file contents, uses no LLM, and is not semantic/LSP analysis; use read_files for contents.",
    ),
    model_spec(
        def(
            "list_project_files",
            super::ToolAuditPolicy::TYPED_CANONICAL,
            ModelVisible,
            TOOL_CATEGORY_FILE,
            Some(FileRead),
            TOOL_PROVIDER_RUNNER,
            super::ToolSemanticContract {
                effect: super::ToolEffect::Observe,
                risk: Read,
                approval: super::ToolApprovalPolicy::None,
                idempotency: super::ToolIdempotency::PureRead,
            },
            Some(PROJECT_READ),
            true,
            NoPath,
            false,
            false,
            super::ToolSessionEvidencePolicy::NONE.review(super::ToolReviewEvidence::ReadOnlyInspection),
        ),
        "List one deterministic page of files in a Runner-registered project directory (bounded, read-only). Entries are sorted before offset/limit slicing; use next_offset until null. Successful paging is exposed only when the complete directory source reached the Server—retained-tail truncation fails closed instead of inventing total_entries or a safe continuation. Returns project-relative paths plus a file/dir kind. Routed to the owning registered Runner; the server never reads the Runner project path directly.",
    ),
    model_spec(
        def(
            "list_project_tracked_files",
            super::ToolAuditPolicy::TYPED_CANONICAL,
            ModelVisible,
            TOOL_CATEGORY_FILE,
            // Runs `git ls-files` on the Runner, so the shell capability is what
            // the Runner must actually hold — not FileRead's directory op.
            Some(Shell),
            TOOL_PROVIDER_RUNNER,
            super::ToolSemanticContract {
                effect: super::ToolEffect::Observe,
                risk: Read,
                approval: super::ToolApprovalPolicy::None,
                idempotency: super::ToolIdempotency::PureRead,
            },
            Some(PROJECT_READ),
            true,
            NoPath,
            false,
            false,
            super::ToolSessionEvidencePolicy::NONE,
        ).with_composition_policy(super::ToolCompositionPolicy::Parallel),
        "Default discovery tool: what files does this project contain? Lists Git-tracked paths from a bounded producer source, so ignored directories like .venv and target never appear. Supports globs, a project-relative path scope, rollup, and offset paging when source acquisition is complete. If list_truncated=true, the source itself is incomplete: next_offset is null and offset must not be treated as recovery for the full repository; narrow path and retry. Retained-tail source truncation fails closed rather than exposing a false continuation.",
    ),
    adaptive_runtime_direct(
        model_spec(
            def(
                "search_project_texts",
                super::ToolAuditPolicy::TYPED_CANONICAL
                    .session_input(super::ToolAuditSessionInputPolicy::SearchProjectTexts),
                ModelVisible,
                TOOL_CATEGORY_FILE,
                Some(Shell),
                TOOL_PROVIDER_RUNNER,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Observe,
                    risk: Read,
                    approval: super::ToolApprovalPolicy::None,
                    idempotency: super::ToolIdempotency::PureRead,
                },
                Some(PROJECT_READ),
                true,
                NoPath,
                false,
                false,
                super::ToolSessionEvidencePolicy::NONE.review(super::ToolReviewEvidence::Search).exploration(super::ToolExplorationEvidence::SearchBatch),
            ).with_composition_policy(super::ToolCompositionPolicy::Parallel),
            "Batch-capable project-text search for 1..8 predetermined independent queries with bounded structured results, protected-path policy, and isolated failures. For broad discovery prefer files_with_matches/count or a small low-context match set. A complete zero-result query with include_globs may return zero_match_hint=include_globs_excluded_matches after one bounded path-private diagnostic; broaden only the include filter deliberately. When matched source will be read immediately, prefer search_and_read; for one small known-scope search, native rg is first-class. Queries default to regex; prefer pattern_mode=literal for exact text. Batch only independent queries and keep result-dependent follow-ups sequential. Use the returned suggested_call for whole-query continuation; truncated individual queries must be narrowed.",
        ).with_gpt_action_description("Batch-search 1..8 independent queries. Prefer search_and_read when matched source will be read immediately; use files/count or small low-context matches for broad discovery. Prefer literal mode for exact text. Follow suggested_call for batch continuation; narrow truncated queries."),
        40,
    ),
    adaptive_runtime_direct(
        model_spec(
            def(
                "search_and_read",
                super::ToolAuditPolicy::TYPED_CANONICAL
                    .session_input(super::ToolAuditSessionInputPolicy::SearchAndRead),
                ModelVisible,
                TOOL_CATEGORY_FILE,
                Some(Shell),
                TOOL_PROVIDER_RUNNER,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Observe,
                    risk: Read,
                    approval: super::ToolApprovalPolicy::None,
                    idempotency: super::ToolIdempotency::PureRead,
                },
                Some(PROJECT_READ),
                true,
                NoPath,
                false,
                false,
                super::ToolSessionEvidencePolicy::NONE
                    .review(super::ToolReviewEvidence::ReadOnlyInspection)
                    .exploration(super::ToolExplorationEvidence::SearchCompound),
            ),
            "Compound coding inspection: run one bounded project-text query or 1..8 predetermined independent queries, then read ranges around up to eight matches in one call. Provide query xor queries; max_reads is one global read budget shared fairly across the batch. Runtime forces zero-context match mode and returns coalesced ranges while preserving per-query batch failures and read_files snapshot/recovery semantics. If explicit include_globs yield no matches, zero_match_hint can report that the filter excluded otherwise eligible matches without exposing diagnostic paths. Batch only independent queries; keep result-dependent follow-ups sequential. Follow reads.suggested_call for revision-fenced continuation. Prefer search_project_texts for discovery, count, or files-only tasks.",
        ).with_gpt_action_description("Search one query or batch 1..8 independent queries, then inspect up to eight matched source ranges total. Use query xor queries; max_reads is shared across the batch. Follow reads.suggested_call for snapshot-fenced continuation; use search_project_texts for discovery/count/files-only tasks."),
        52,
    ),
];

pub(super) const READ_DEFINITIONS: &[ToolDefinition] = &[
    adaptive_runtime_direct(
        model_spec(
            def(
                "read_files",
                super::ToolAuditPolicy::TYPED_CANONICAL,
                ModelVisible,
                TOOL_CATEGORY_FILE,
                Some(FileRead),
                TOOL_PROVIDER_RUNNER,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Observe,
                    risk: Read,
                    approval: super::ToolApprovalPolicy::None,
                    idempotency: super::ToolIdempotency::PureRead,
                },
                Some(PROJECT_READ),
                true,
                NoPath,
                false,
                false,
                super::ToolSessionEvidencePolicy::NONE.review(super::ToolReviewEvidence::ReadOnlyInspection).exploration(super::ToolExplorationEvidence::ReadBatch),
            ).with_composition_policy(super::ToolCompositionPolicy::Parallel),
            "Batch/snapshot-aware project inspect with read_revision, snapshot-bound continuation, batching, protected-path policy, range normalization, and bounded recovery. When the target symbol/test/implementation region is known, prefer bounded targeted ranges and batch related ranges already known to be needed; do not read an entire large file merely because the budget permits it. A small known one-off observation without downstream snapshot dependency may use native file commands. Items expose read_revision for the full-file snapshot. Partial reads return suggested_call; continued ranges are fenced to that read_revision and Runtime rejects a continuation if the snapshot changed. The call binds the exact resolved Project and business session_id. Zero progress may suggest larger max_result_bytes; the 512 KiB hard cap exposes no fake continuation.",
        ).with_gpt_action_description("Batch-read 1..8 UTF-8 ranges. Prefer bounded targeted ranges when locations are known; batch related predetermined ranges instead of broad whole-file reads. Follow suggested_call: continuation is read_revision-fenced, changed snapshots fail closed, zero progress may suggest a larger budget."),
        50,
    ),
];
