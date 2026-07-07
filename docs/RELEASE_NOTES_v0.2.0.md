# WebCodex 0.2.0

[English](RELEASE_NOTES_v0.2.0.md)

WebCodex lets ChatGPT and other MCP/GPT Action clients safely operate your private codebase through a self-hosted, auditable tool runtime.

It turns online AI coding from blind file edits into a permissioned, validated, reviewable engineering workflow.

## Highlights

- ChatGPT, MCP, and GPT Actions can use one self-hosted coding runtime.
- MCP and GPT Actions call the same WebCodex ToolRuntime.
- Projects are registered by agents, with runtime ids shaped like `agent:<client_id>:<project_id>`.
- The recommended coding workflow is structured: start, inspect, edit, validate, review, finish.
- Sessions, handoff summaries, finish verdicts, hygiene checks, and validation summaries provide review evidence.
- Structured edit tools support scoped line edits and checked patch application.
- Runtime health, project discovery, Git status/diff, Cargo validation helpers, bounded shell/job execution, and artifact workflows are available through the runtime.
- Legacy server-side `projects.toml` onboarding, legacy `/api/codex/*` routes, and `run_codex` are removed from the model-facing runtime.

## Who Should Try This

Try WebCodex 0.2.0 if you want an online AI coding client to work on private code while repository execution stays on a machine you control.

Good fit:

- Solo developers and small teams comfortable self-hosting a server and agent.
- Teams evaluating ChatGPT MCP or Custom GPT Actions for private repositories.
- Operators who want auditable coding tasks with explicit validation and review evidence.

Not yet a fit:

- Teams looking for hosted SaaS.
- Teams requiring first-class IDE/LSP semantics in the first setup.
- Operators who cannot manage token, HTTPS, agent, and shell boundaries.

## What Changed

- Added a productized online coding loop around `start_coding_task`, structured inspection, structured edits, validation helpers, review/hygiene tools, and `finish_coding_task`.
- Improved agent-registered project discovery and runtime project id guidance.
- Aligned MCP and GPT Actions around the same ToolRuntime and safety model.
- Added or refined session/handoff/finish summaries for reviewable closeout.
- Added structured line edit and patch workflows for safer source changes.
- Added runtime health and compact discovery patterns for model-facing clients.
- Added OAuth/PAT/shared-key documentation for deployment paths while keeping README and Quick Start focused on first setup.
- Expanded Chinese onboarding docs for README, Quick Start, Concepts, MCP, and GPT Actions.

## Breaking Changes

- Server-side `projects.toml` project onboarding is removed.
- Projects must be registered by agents.
- `PROJECTS_CONFIG` is not the runtime project source.
- Legacy `/api/codex/*` routes are removed.
- `run_codex` is removed from the model-facing runtime surface.
- Operators who need Codex-specific workflows should run them outside WebCodex.

## Security Model

- The model can only call exposed tools.
- Project access is agent-registered.
- The server does not scan the filesystem.
- Shell/job tools are bounded but powerful and should be treated as escape hatches.
- Tokens, shared keys, env files, Authorization headers, and complete agent configs must not be exposed in prompts, logs, examples, or committed files.
- Session and finish outputs provide bounded review evidence, not a replacement for normal code review or infrastructure logging.

See [../SECURITY.md](../SECURITY.md) and [CONCEPTS.md](CONCEPTS.md).

## Known Limitations

- WebCodex is self-hosted infrastructure, not hosted SaaS.
- First setup is still technical and assumes command-line comfort.
- Semantic code intelligence, LSP diagnostics, references, and symbol outline are not first-class in 0.2.0.
- The UI/dashboard is minimal and not the primary workflow.
- Shell/job tools require trust, scoped configuration, and operator discipline.
- Full production hardening still depends on your HTTPS, token, OS-user, reverse-proxy, and agent deployment choices.

## Upgrade Notes

- Binary users should replace `webcodex`, `webcodex-cli`, and `webcodex-agent` with matching 0.2.0 artifacts when the release artifacts are published.
- If using an npm wrapper, confirm which binary version its manifest installs before assuming it is 0.2.0.
- Re-register projects through agents instead of relying on removed server-side project configuration.
- Refresh GPT Actions schemas from `/openapi.json` after upgrading.
- Reconnect or restart agents after changing project registrations, tokens, or shell profiles.

## Validation

Before publishing or deploying 0.2.0 artifacts, run the current procedure in [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md). At minimum, verify formatting, cargo check, markdown links, OpenAPI/MCP smoke discovery, agent project discovery, a read-only coding task, and a small reversible edit task.

This release note is not a substitute for fresh validation output from the final release commit.

## Next

The 0.2.x line focuses on making the online coding loop easier to try, validate, review, and roll back. See [ROADMAP.md](ROADMAP.md) for the short roadmap.
