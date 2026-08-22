# Deterministic committed-range review summary

`git_review_summary` is a read-only review primitive for committed Git ranges. It produces a bounded deterministic map that helps a reviewer choose the next `git_diff_hunks` or `read_file` targets. It does not review code with an LLM, judge correctness, approve a change, or mutate/fetch the repository.

## Exact range

Inputs are `project`, `base_commit`, and `head_commit`. V1 intentionally accepts only exact 40-hex SHA-1 commit object IDs already present in the registered repository; branch names, revision expressions, command-line options, and remote fetches are rejected or unavailable by construction.

The tool resolves both objects as commits, computes the best merge-base set, reports whether the requested base is an ancestor of the head, and reviews the single exact `merge_base..head_commit` range. A missing object, missing merge-base, or criss-cross history with multiple best merge-bases is a structured failure rather than a guessed range.

Git diff observations run through the existing agent internal POSIX Git path with external diff drivers and textconv disabled. The commands are read-only and do not update the index or worktree.

## Output contract

The model-facing result contains bounded metadata only:

- exact requested base/head, merge-base, ancestry, commit count, and effective diff range;
- aggregate file/line/binary statistics;
- deterministic path-based file classes and subsystem buckets;
- reviewer-attention signals such as auth/scope, protocol/wire schema, tool contract, runtime config, persistence/migration, execution lifecycle, and CLI/HTTP surface touches;
- bounded changed-file metadata and best-effort Git hunk/function-context symbol hints;
- conservative production/test/docs coverage observations;
- explicit bounds, truncation/partiality metadata, and warnings;
- `deterministic=true` and `llm_summary=false`.

Signals mean only that a surface deserves targeted inspection. They never claim a security bug, breaking change, safety, or correctness.

The first implementation is path- and Git-context-based. It does not invoke an LSP, execute project code, build an AST/dependency graph, infer affected tests, or fetch GitHub/PR metadata.

## Bounds and partial results

The initial fixed bounds are:

- 80 returned files;
- 512 bytes per returned path;
- 12 subsystem buckets;
- 12 review signals;
- 8 paths per signal;
- 6 symbol hints per file and 80 total;
- 120 bytes per symbol hint;
- 64 KiB of raw diff context retained internally for symbol extraction.

Aggregate Git stats are computed independently of the returned-file bound. If changed-file or symbol evidence exceeds a bound, the successful result sets `truncated=true` and reports the relevant `*_partial` / `*_truncated` fields. When file classification is partial, an unobserved production/test/docs category is returned as `null`, not falsely asserted as `false`.

Binary files and Gitlinks are metadata-only for symbol inspection. Secret-like and bulk-excluded paths reuse the repository sensitive-path policy and are not opened for hunk-context symbol extraction.

## Review workflow

For a branch or PR whose exact commits are already local:

1. Call `git_review_summary` with exact base/head object IDs.
2. Use its subsystem/signal/file map to select a small set of `git_diff_hunks` and `read_file` targets.
3. Perform the actual correctness/security review from those source-level observations.

Use `show_changes` instead for the current dirty worktree. `git_review_summary` is deliberately not injected into normal `finish_coding_task` output.
