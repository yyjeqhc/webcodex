# WebCodex

[English](README.md) | [简体中文](README.zh-CN.md)

WebCodex lets a coding client work on private code through a project-scoped
server and local Agent. Source files, Git operations, edits, and checks remain
on the machine that owns the repository.

## Install

On supported Linux x64 systems:

```bash
npm install -g @yyjeqhc/webcodex
```

Or build every binary from source:

```bash
cargo build --release --bins
export PATH="$PWD/target/release:$PATH"
```

See [docs/BUILD_INSTALL.md](docs/BUILD_INSTALL.md) for installation details.

## One Project, One Entry

Run the following from the Git project you want to expose:

```bash
webcodex setup
webcodex doctor
webcodex agent start
```

`setup` creates minimal private state outside the checkout. It does not modify
Git content, start a background service, open a port, edit shell startup files,
or send project files anywhere. Running it again is safe: valid configuration
is preserved, missing pieces are repaired, and conflicting state fails closed.
The private state contains one exact Project Credential shared by this
project's Connector and Agent. It is never printed and an arbitrary Bearer
token cannot substitute for it.

`doctor` is read-only. Immediately after setup it normally reports that the
local Agent still needs to start and gives the exact next command.

`agent start` is the explicit foreground action that starts the project-bound
loopback runtime and Agent. Leave that terminal open. In another terminal:

```bash
webcodex status
```

The default output uses only product concepts: Project, Connection, Agent,
Capabilities, readiness, and the next action. It does not print credentials,
client IDs, runtime project IDs, executor references, workflow sessions, or
transport details.

The complete walkthrough is in [docs/QUICK_START.md](docs/QUICK_START.md).

## Canonical Coding Path

A configured MCP/OpenAPI Connector exposes exactly nine project-bound
capabilities:

```text
task_start
→ files_read / files_search
→ edits_apply
→ checks_run
→ task_finish
→ task_review
→ task_cancel (when needed)
```

The configured Connector context resolves the project deterministically.
Ordinary coding does not need `list_projects`, `runtime_status`,
`tool_manifest`, `start_session`, `current_session`, Agent listing, or project
registration calls.

Normal writable tasks must run structured checks before `task_finish`. The
result remains isolated until a human reviews and accepts it locally:

```bash
webcodex task list
webcodex task show <task-id>
webcodex task accept <task-id>
```

Task, operation, execution, and result IDs remain visible because they provide
exact retry, progress, review, and acceptance identity. Executor routing and
queue IDs stay internal.

### Project-aware validation

`checks_run` remains one of the nine capabilities. Omit its optional `recipe`
field to resolve the nearest supported manifest from the Task execution
workspace and relative `cwd`; use `recipe: rust|node|python|go` only to resolve
a same-directory ambiguity. Resolution never scans sibling projects, and
absolute, parent-traversing, or symlink-escaping `cwd` values fail closed.

Rust supports `format`, `check`, and `test`; Node selects only fixed
non-mutating script names; Python selects configured Ruff/Black, Ruff/Mypy,
and pytest tools; Go supports `check` and `test` while `format` is deliberately
unavailable. Recipes never install dependencies, generate configuration,
change lockfiles, or access the network. Missing tools are executor failures;
a non-zero result after a tool starts is an assertion failure. The resolved
recipe version, relative root, manifest/lock evidence, and structured
invocation are part of `operation_id` exact-retry identity. See
[docs/QUICK_START.md](docs/QUICK_START.md#project-aware-validation-recipes).

## Readiness

Use `webcodex status` for a quick “can this project work now?” answer. Use
`webcodex doctor` for structured, actionable checks covering local config,
authentication presence, project registration, Git/workspace access, the Agent
runtime, server reachability, Agent registration, required capabilities, and
structured validation.

The Browser console at `/console` is a minimal read-only projection of the same
readiness facts. It is not a second status implementation and is not a browser
IDE.

## Client Access

The canonical setup starts on loopback and does not create public ingress.
Local clients can use the project-bound Connector when they share the approved
local connection configuration and its exact Project Credential. Loopback is a
network boundary, not an authentication exemption: unknown credentials are
rejected before readiness, task state, or Agent dispatch. Hosted ChatGPT
clients require an operator-
managed HTTPS endpoint and authentication; setup deliberately does not create a
tunnel, open a public port, or change production auth. See
[docs/DEPLOYMENT.md](docs/DEPLOYMENT.md), [docs/MCP.md](docs/MCP.md), and
[docs/GPT_ACTIONS.md](docs/GPT_ACTIONS.md).

The legacy ToolRuntime discovery and operations tools remain available for
administration and diagnostics. They are not prerequisites for the project
coding path.

## Safety Boundary

- Setup registers only the resolved Git root; it never guesses from names or
  recent usage.
- Project setup uses its exact credential path, not the ordinary arbitrary-key
  quick-start fallback; Connector and Agent must resolve to the same
  non-secret project grant identity.
- Explicit project binding is principal-scoped and transport-scoped where the
  protocol requires it; ambiguous binding fails closed.
- Read-only tasks deny mutation, shell, and job-like actions.
- Structured edits and validation are preferred over raw shell.
- A validation command that cannot spawn is an executor failure, not a failed
  project assertion.
- Tokens, Authorization headers, hashes, private keys, and secret paths must
  never appear in prompts, logs, examples, or committed configuration.

Read [SECURITY.md](SECURITY.md) and
[docs/CONCEPTS.md](docs/CONCEPTS.md) for the full boundary model.

## Scope

WebCodex is self-hosted infrastructure, not a hosted SaaS or a full browser
IDE. Advanced multi-client enrollment, production OAuth, remote deployment,
QUIC, shell profiles, and operator observability remain available through the
management documentation and `webcodex-cli`; they do not change the ordinary
project entry above.

## Documentation

- Getting started: [docs/QUICK_START.md](docs/QUICK_START.md)
- Build/install: [docs/BUILD_INSTALL.md](docs/BUILD_INSTALL.md)
- Concepts: [docs/CONCEPTS.md](docs/CONCEPTS.md)
- MCP: [docs/MCP.md](docs/MCP.md)
- GPT Actions: [docs/GPT_ACTIONS.md](docs/GPT_ACTIONS.md)
- Deployment: [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md)
- Roadmap: [docs/ROADMAP.zh-CN.md](docs/ROADMAP.zh-CN.md)

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
