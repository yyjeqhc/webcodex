# Contributing to WebCodex

[简体中文](CONTRIBUTING.zh-CN.md)

Contributions are welcome, including bug reports, documentation improvements,
focused fixes, and new capabilities that fit WebCodex's current product
direction.

Contributions created with WebCodex itself or with other coding agents are also
welcome. The contributor remains responsible for reviewing the resulting diff,
validating the change, and making sure no credentials or machine-private data
are included.

## Before you start

- Check the latest `main` and existing issues before starting substantial work.
- Small documentation fixes and focused corrections can be submitted directly.
  For larger features or architectural changes, opening an issue first helps
  avoid duplicated or misaligned work.
- For bugs and client interoperability problems, opening an issue first is
  usually the best way to confirm the problem and share reproduction details.
- For security-sensitive reports, follow [SECURITY.md](SECURITY.md) instead of
  opening a public issue.
- Keep changes focused. Avoid unrelated refactors or generated changes in the
  same pull request.

## Reporting issues

A useful issue should let a maintainer understand the observed problem without
first asking for basic context. Include what is relevant to the problem; there
is no need to fill unrelated fields with `N/A`.

For bugs and interoperability problems, include when available:

- **Observed behavior and impact:** what action, tool, UI, or workflow failed,
  what actually happened, and whether it blocks or degrades normal work.
- **Environment:** the WebCodex version or commit; Server and Runner operating
  systems; whether Server and Runner builds are aligned if known; the client or
  Host in use, such as ChatGPT, Codex, Claude, Gemini, Grok, CLI, Desktop, or
  WebUI; and the authentication mode. Add browser, proxy, SSH, sandbox, or other
  environment details only when they are relevant.
- **Minimal reproduction:** the shortest steps that reproduce the problem and
  whether it is deterministic, intermittent, or has only been observed once.
- **Expected versus actual behavior:** state them separately.
- **Exact evidence:** preserve the exact error text, `error_kind`, exit code,
  failed stage, or a short redacted log tail when available. Do not replace the
  only concrete error with a paraphrase.
- **Diagnostics already tried:** for example, whether the problem also occurs on
  current `main`, another Runner, a fresh Session, another client, or a known
  previous version. If this appears to be a regression, include the last known
  good and first known bad version or commit when known.
- **Facts versus assessment:** distinguish directly observed facts from a
  suspected root cause or interpretation.

### Issues written by coding agents

Coding agents are welcome to create issues, but they should make inexpensive
use of the diagnostics already available to them before filing. An agent that
can inspect runtime status, build alignment, the exact tool error, relevant
source, or focused test evidence should normally include that information
instead of asking a maintainer or reporter to provide it later.

An agent-authored issue should also:

- state whether the problem was **reproduced directly**, **inferred from source
  or test evidence**, or **reported by a user and not independently reproduced**;
- never invent missing environment details, reproduction results, or a root
  cause merely to make the report look complete;
- identify the inspected WebCodex commit and relevant files when source review
  materially supports the report;
- label a root cause as suspected unless the evidence actually establishes it;
- stop after reasonable, focused diagnostics instead of delaying a useful issue
  indefinitely for exhaustive investigation.

Never include tokens, authorization headers, private keys, cookies, passwords,
private file contents, or other secrets. For security-sensitive reports, use
[SECURITY.md](SECURITY.md) instead of a public issue.

## Development workflow

1. Start from the current `main` branch and create a focused branch for the
   change.
2. Follow the repository guidance in [AGENTS.md](AGENTS.md) and any more
   specific guidance referenced from it.
3. Match the existing architecture and naming. Prefer the smallest coherent
   change that solves the current problem.
4. Add or update focused tests when behavior changes, and update documentation
   when public behavior or operations change.
5. Run the smallest relevant validation for the files you changed.
   Documentation-only changes do not require a Cargo build.
6. Review the final diff and worktree state before committing.

For repository testing guidance, see [docs/TESTING.md](docs/TESTING.md). For the
coding workflow and closeout conventions, see
[docs/CODING_WORKFLOW.md](docs/CODING_WORKFLOW.md).

On developer machines, ordinary `dev` and `test` Cargo profiles intentionally omit
source-line debuginfo to keep large test binaries and links smaller. Linux developers
may optionally run Cargo through `bash scripts/cargo_fast.sh <cargo-args>`; the wrapper
runs Cargo under `mold -run` only when mold is available. Missing mold falls back to
normal Cargo, and the wrapper never changes the macOS or Windows linker.

## Pull requests

A pull request should explain what changed, why the change is needed, and what
validation was performed. Link the relevant issue when one exists.

Please keep each pull request reviewable and limited to one coherent purpose. If
a change affects authentication, authorization, process lifecycle, persistence,
public protocol behavior, or another trust boundary, include focused regression
evidence for that boundary.

By contributing, you agree that your contribution is licensed under the
repository's [Apache License 2.0](LICENSE).
