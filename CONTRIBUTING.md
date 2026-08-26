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

A useful bug report should include, when relevant:

- the WebCodex version or commit;
- Server and Runner operating systems;
- the client in use, such as ChatGPT, Claude, Gemini, Grok, or another MCP
  client;
- the authentication mode, such as Bearer or OAuth;
- clear reproduction steps and expected versus actual behavior;
- redacted logs or error messages that help identify the failing stage.

Never include tokens, authorization headers, private keys, cookies, passwords,
or other secrets.

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

## Pull requests

A pull request should explain what changed, why the change is needed, and what
validation was performed. Link the relevant issue when one exists.

Please keep each pull request reviewable and limited to one coherent purpose. If
a change affects authentication, authorization, process lifecycle, persistence,
public protocol behavior, or another trust boundary, include focused regression
evidence for that boundary.

By contributing, you agree that your contribution is licensed under the
repository's [Apache License 2.0](LICENSE).
