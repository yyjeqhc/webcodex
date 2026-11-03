# Documentation Index

[English](INDEX.md) | [简体中文](INDEX.zh-CN.md)

Start with the goal that matches what you are trying to do.

## I want to use WebCodex normally

- [README](../README.md) — what WebCodex does and how to choose a setup
- [Unified installation](unified-installation.md) — persistent main-node and multi-computer setup; check [deployment validation](unified-deployment-validation.md) before choosing an installer
- [Runtime installation and CLI join](runtime-installation.md) — headless Linux Runner setup and package update/recovery
- [Desktop installation](desktop-install.md) — release-specific Windows/macOS installation and OpenAI Secure Tunnel details
- [Using Desktop](desktop-guide.md) — projects, device invitations, connections, activity, and service controls
- [Desktop Runtime compatibility](DESKTOP_RUNTIME_COMPATIBILITY.md) — Runtime selection, diagnostics, updates, and rollback
- [Runtime Console](runtime-console.md) — browser activity, projects, Sessions, and Agent communication
- [Full Setup](PERSONAL_SETUP.md) — CLI, existing Server, Linux, and advanced regular Server + Runner setup
- [AI-assisted setup](AI_ONBOARDING.md) — have an AI agent configure WebCodex using ordinary user language
- [MCP](MCP.md) — ChatGPT, Claude, and other MCP clients
- [Session continuity](SESSION_CONTINUITY.md) — save context, continue across accounts/windows, and delegate model API reviews

## I only want to try it for a few minutes

- [Quick Trial](QUICK_START.md) — temporarily try one repository with one `webcodex share` command

## I need production deployment or deep troubleshooting

- [Deployment](DEPLOYMENT.md) — systemd/Docker, multi-user self-hosting, and long-lived operations
- [Windows + OpenAI Secure MCP Tunnel deep dive](WINDOWS_OPENAI_TUNNEL.md) — independent Windows Server/Runner, Tunnel setup, and real troubleshooting notes
- [Runner](RUNNER.md) — Runner operations reference
- [CLI](CLI.md) — full command, configuration, and credential reference

## I need authentication or network options

- [MCP](MCP.md) — Bearer, query-token fallback, OAuth, private tunnel, and MCP protocol reference
- [Authentication](AUTH_MODEL.md) — detailed credential and authority boundaries
- [Deployment](DEPLOYMENT.md) — stable HTTPS origins, self-hosting, and production networking

## I need help

- [Troubleshooting](TROUBLESHOOTING.md) — ChatGPT/MCP Host, installation, connection, runtime, and Runner problems
- [Security](../SECURITY.md) — safe operating guidance and security model

## I want to understand or extend WebCodex

- [Architecture](ARCHITECTURE.md) — how the major components fit together
- [Coding Workflow](CODING_WORKFLOW.md) — task bootstrap, guidance, validation, and closeout
- [Native Tool Plugins](PLUGINS.md) — add Runner-local tools with any executable language and no MCP SDK
- [Computer Use](COMPUTER_USE.md) — current desktop primitives, authority/platform boundaries, and explicitly unimplemented directions
- [Browser/CDP runtime architecture](architecture/browser-cdp-runtime.md) — current Browser ownership, managed sessions, handoff, authority, and stale fencing

## Architecture reading path

Read each row's current contract before its implementation or experiment record.
For Agent Infra or application design study, ask which component owns identity,
authority, state, retries and the next observation; follow the linked source/tests
in the contract instead of copying a second normative checklist.

| Step and question | Read next |
| --- | --- |
| 1. Where does a request execute, and what survives device replacement? | [Architecture and request trace](ARCHITECTURE.md#module-map-and-one-request), [Runner](RUNNER.md), [Server-owned tunnels](architecture/server-owned-tunnels.md) |
| 2. How do tools become discoverable without widening authority? | [MCP](MCP.md), [tool contracts](agent/tool-contract-guidelines.md), [Context/guidance](architecture/builtin-context-guidance.md), [Control sidecars](agent/control-sidecars.md) |
| 3. What is evidence, what is execution, and what can resume? | [Session model](agent/session-model.md), [SQLite and Hot/Cold residency](implementation/session-active-residency.md), [Job recovery](agent/job-reliability-and-concurrency.md), [manual collaboration](agent/manual-window-collaboration.md) |
| 4. How do Goal, Agent Task/TaskAttempt, Conversation and Wake differ? | [Durable Agent architecture](architecture/durable-agent-runtime.md), [current communication contract](architecture/durable-agent-conversation.md) |
| 5. Why do UI actions need exact targets and fresh observations? | [Browser/CDP](architecture/browser-cdp-runtime.md), [browser continuity](architecture/browser-session-continuity.md), [Computer Use](COMPUTER_USE.md) |
| 6. What context and extension resources actually exist? | [Native Tool Plugins](PLUGINS.md), [resource ownership review](architecture/resource-model-and-quality-review.md), [Project Knowledge plugin](../plugins/project-knowledge/README.md). Core Memory, lexical tool discovery and this optional FTS5 plugin are distinct; none is a built-in vector RAG pipeline. |
| 7. Which orchestration claims are experimental? | [Host versus V8 Code Mode](experiments/code-mode.md), [measured Code Mode conditions](experiments/code-mode-performance.md), [composition research](architecture/tool-composition-research.md) |

### Document status and ownership

| Status | How to use it |
| --- | --- |
| Current contract | Follow the owning schema/source and regression references. Current `main` behavior does not imply availability in every published release or Host. |
| Historical experiment or implementation record | Preserve the source revision, environment, raw measurements and validation limits. Reported pass counts describe that run; they are not current acceptance evidence. |
| Unimplemented design direction | Treat it as a proposal with explicit conditions, not a supported tool or an execution requirement. |
| Retired or replaced design | Follow the named replacement. Merge useful evidence into its owner and update references before removing the redundant page. |

`docs/agent/` holds maintainer contracts and some explicitly marked historical audits;
`docs/architecture/` holds designs and source navigation;
`docs/implementation/` mixes current implementation contracts with dated acceptance;
`docs/experiments/`, `docs/design/` and `docs/contribution-reports/` retain scoped
research or staged work. Integrations and Plugins keep their consumer-specific
guides beside their code. A directory, date, title or TODO count alone does not
establish a document's status; mixed pages must identify the status of each part.

## I want to contribute or release WebCodex

- [Contributing](../CONTRIBUTING.md) — issue evidence, focused fixes, validation, and pull requests
- [Desktop development](DESKTOP_DEVELOPMENT.md) — run Desktop from source and build/test local platform packages
- [Sponsorship and project support](SPONSORSHIP.md) — community sponsorship, infrastructure support, partnerships, and recognition principles

The references below cover repository policy and maintainer/internal contracts.
The contract pages intentionally contain protocol fields, compatibility names,
and implementation invariants that ordinary users should not need to learn.

- [AGENTS.md](../AGENTS.md) — repository instructions for coding/AI agents
- [Maintenance](MAINTENANCE.md) — maintenance queue, dependency cadence, PR/CI expectations, and bilingual-doc policy
- [Testing](TESTING.md) — testing strategy
- [Release checklist](RELEASE_CHECKLIST.md) — release readiness
- [Compatibility policy](compatibility-policy.md) — supported published formats, upgrade bridges, and named protocol consumers
- [Architecture decisions](agent/architecture-decisions.md)
- [Tool contract guidelines](agent/tool-contract-guidelines.md) — model-facing selection, authority, effects, continuation, and recovery
- [Durable Agent runtime](architecture/durable-agent-runtime.md) — durable identity and asynchronous work architecture
- [Durable Agent/Conversation/Wake contract](architecture/durable-agent-conversation.md) — current communication implementation
- [Runtime host context](agent/runtime-host-context.md) — Runner-configured planning context and runtime diagnostics
- [Job reliability and Runner concurrency](agent/job-reliability-and-concurrency.md) — restart recovery, observation semantics, shared Job capacity, and tool-description requirements
- [Tool request tracing](agent/tool-request-tracing.md) — maintainer forensic payload/correlation contract
- [Authority model](agent/permission-model.md)
- [Session model](agent/session-model.md)
- [Manual multi-window collaboration](agent/manual-window-collaboration.md)
- [Runtime API guidelines](agent/runtime-api-guidelines.md)
- [Release process](agent/release-process.md)
