use super::RunnerCapabilityRequirement::{FileRead, Shell};
use super::{
    adaptive_runtime_direct, model_spec, runner_project_read_def, ToolDefinition,
    TOOL_CATEGORY_FILE, TOOL_CATEGORY_PROJECT,
};
use crate::metadata::ToolPathHint::None as NoPath;

pub(super) const SEARCH_DEFINITIONS: &[ToolDefinition] = &[
    model_spec(
        runner_project_read_def(
            "read_project_overview",
            super::ToolAuditPolicy::TYPED_CANONICAL,
            TOOL_CATEGORY_PROJECT,
            FileRead,
            NoPath,
            super::ToolSessionEvidencePolicy::NONE.review(super::ToolReviewEvidence::ReadOnlyInspection),
        ).with_composition_policy(super::ToolCompositionPolicy::Parallel),
        "Deterministic, bounded, metadata-only project overview. With the root/default scope it performs conservative project discovery from tracked state; with an explicit eligible path such as cache or build it can inspect ignored/generated directories through the bounded filesystem walk, while high-volume target/node_modules scopes remain excluded. Returns conventional project types, manifests, key files, roots, and direct children. Reads no file contents, uses no LLM, and is not semantic/LSP analysis; use read_files for exact file contents.",
    ).with_discovery_keywords(&["project", "overview", "structure", "manifests", "language", "framework", "项目", "概览", "结构", "清单", "框架"]),
    model_spec(
        runner_project_read_def(
            "list_project_files",
            super::ToolAuditPolicy::TYPED_CANONICAL,
            TOOL_CATEGORY_FILE,
            FileRead,
            NoPath,
            super::ToolSessionEvidencePolicy::NONE.review(super::ToolReviewEvidence::ReadOnlyInspection),
        ),
        "List one live Project directory, including named generated/non-Git directories. Returns sorted project-relative paths and file/dir kinds. Follow next_call for another page; offsets are not snapshots, so concurrent directory changes can shift entries. Current Runners bound pages before transport; older Runners require a complete source or fail closed. Use list_project_tracked_files with query/globs/depth for repository discovery, search_project_texts for content, and read_files for a known file. Do not enumerate directories merely to confirm a known path.",
    ).with_discovery_keywords(&["list", "directory", "folder", "entries", "files", "列出", "目录", "文件夹", "文件"]),
    model_spec(
        runner_project_read_def(
            "list_project_tracked_files",
            super::ToolAuditPolicy::TYPED_CANONICAL,
            TOOL_CATEGORY_FILE,
            // Runs `git ls-files` on the Runner, so the shell capability is what
            // the Runner must actually hold — not FileRead's directory op.
            Shell,
            NoPath,
            super::ToolSessionEvidencePolicy::NONE,
        ).with_composition_policy(super::ToolCompositionPolicy::Parallel),
        "Default discovery tool: what files does this project contain? Lists Git-tracked paths from a bounded producer source, so ignored directories like .venv and target never appear. Supports globs, a project-relative path scope, rollup, and offset paging when source acquisition is complete. If list_truncated=true, the source itself is incomplete: next_offset is null and offset must not be treated as recovery for the full repository; narrow path and retry. Retained-tail source truncation fails closed rather than exposing a false continuation.",
    ).with_discovery_keywords(&["find", "filename", "glob", "tracked", "repository", "files", "文件名", "通配符", "跟踪", "仓库", "文件"]),
    adaptive_runtime_direct(
        model_spec(
            runner_project_read_def(
                "search_project_texts",
                super::ToolAuditPolicy::TYPED_CANONICAL
                    .session_input(super::ToolAuditSessionInputPolicy::SearchProjectTexts),
                TOOL_CATEGORY_FILE,
                Shell,
                NoPath,
                super::ToolSessionEvidencePolicy::NONE.review(super::ToolReviewEvidence::Search).exploration(super::ToolExplorationEvidence::SearchBatch),
            )
            .with_composition_policy(super::ToolCompositionPolicy::Parallel)
            .with_host_orchestration_hint(
                super::ToolHostOrchestrationHint::independent_parallel_read()
                    .with_native_batch_field("queries"),
            ),
            "Batch-capable project-text search for 1..8 predetermined independent queries with bounded structured results, protected-path policy, and isolated failures. One absolute batch deadline covers queueing/retries; timeout_secs is an item ceiling capped by remaining batch budget and retries do not reset it. For broad discovery prefer files_with_matches/count or a small low-context match set. A complete zero-result query with include_globs may return zero_match_hint=include_globs_excluded_matches after one bounded path-private diagnostic; broaden only the include filter deliberately. When matched source will be read immediately, prefer search_file_context; for one small known-scope search, native rg is first-class. Queries default to regex; prefer pattern_mode=literal for exact text. Batch only independent queries and keep result-dependent follow-ups sequential. Use the returned suggested_call for whole-query continuation; truncated individual queries must be narrowed.",
        ).with_discovery_keywords(&["find", "search", "occurrences", "symbol", "function", "text", "source", "files", "grep", "regex", "搜索", "检索", "查找", "函数名", "符号", "文本", "内容", "文件"]),
        40,
        super::ToolDirectReason::CoreWorkflow,
    ),
    adaptive_runtime_direct(
        model_spec(
            runner_project_read_def(
                "search_file_context",
                super::ToolAuditPolicy::TYPED_CANONICAL
                    .session_input(super::ToolAuditSessionInputPolicy::SearchAndRead),
                TOOL_CATEGORY_FILE,
                Shell,
                NoPath,
                super::ToolSessionEvidencePolicy::NONE
                    .review(super::ToolReviewEvidence::ReadOnlyInspection)
                    .exploration(super::ToolExplorationEvidence::SearchCompound),
            )
            .with_host_orchestration_hint(
                super::ToolHostOrchestrationHint::independent_parallel_read()
                    .with_native_batch_field("queries")
                    .with_compound_preferred(),
            ),
            "Compound coding inspection: run one bounded project-text query or 1..8 predetermined independent queries, then read ranges around up to eight matches in one call. Provide query xor queries; max_reads is one global read budget shared fairly across the batch. Runtime forces zero-context match mode and returns coalesced ranges while preserving per-query batch failures and read_files snapshot/recovery semantics. Matches whose source text was successfully materialized keep path/line identity but omit duplicate preview content; unmaterialized matches keep the existing compound-search evidence. If explicit include_globs yield no matches, zero_match_hint can report that the filter excluded otherwise eligible matches without exposing diagnostic paths. Batch only independent queries; keep result-dependent follow-ups sequential. Follow reads.suggested_call for revision-fenced continuation. Prefer search_project_texts for discovery, count, or files-only tasks.",
        ).with_discovery_keywords(&["search", "find", "source", "context", "surrounding", "matches", "搜索", "查找", "上下文", "匹配", "源码"]),
        52,
        super::ToolDirectReason::CoreWorkflow,
    ),
];

pub(super) const READ_DEFINITIONS: &[ToolDefinition] = &[
    adaptive_runtime_direct(
        model_spec(
            runner_project_read_def(
                "read_files",
                super::ToolAuditPolicy::TYPED_CANONICAL,
                TOOL_CATEGORY_FILE,
                FileRead,
                NoPath,
                super::ToolSessionEvidencePolicy::NONE.review(super::ToolReviewEvidence::ReadOnlyInspection).exploration(super::ToolExplorationEvidence::ReadBatch),
            )
            .with_composition_policy(super::ToolCompositionPolicy::Parallel)
            .with_host_orchestration_hint(
                super::ToolHostOrchestrationHint::independent_parallel_read()
                    .with_native_batch_field("items"),
            ),
            "Batch/snapshot-aware project inspect with read_revision, snapshot-bound continuation, batching, protected-path policy, range normalization, and bounded recovery. When the target symbol/test/implementation region is known, prefer bounded targeted ranges and batch related ranges already known to be needed; do not read an entire large file merely because the budget permits it. A small known one-off observation without downstream snapshot dependency may use native file commands. Items expose read_revision for the full-file snapshot. Partial reads return suggested_call; continued ranges are fenced to that read_revision and Runtime rejects a continuation if the snapshot changed. The call binds the exact resolved Project and business session_id. Zero progress may suggest larger max_result_bytes; the 512 KiB hard cap exposes no fake continuation.",
        ).with_discovery_keywords(&["read", "source", "file", "contents", "inspect", "snapshot", "batch", "range", "读取", "阅读", "源码", "文件", "内容", "批量", "行范围"]),
        50,
        super::ToolDirectReason::CoreWorkflow,
    ),
];
