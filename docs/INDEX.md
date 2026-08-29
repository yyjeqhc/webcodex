# Documentation Index

[English](INDEX.md) | [简体中文](INDEX.zh-CN.md)

Start with the goal that matches what you are trying to do.

## I want to try WebCodex

- [README](../README.md) — what WebCodex does and the one-command first-run path
- [Quick Start](QUICK_START.md) — zero to the first successful AI-assisted development request
- [MCP](MCP.md) — ChatGPT, Claude, authentication choices, and other MCP clients
- [AI-assisted setup](AI_ONBOARDING.md) — have an AI agent help configure WebCodex

## I need Windows or a permanent deployment

- [Deployment](DEPLOYMENT.md) — self-hosting, Linux Server setup, Windows/Runner enrollment, and long-lived operation
- [Windows + OpenAI Secure MCP Tunnel](WINDOWS_OPENAI_TUNNEL.md) — independent Windows Server/Runner, ChatGPT Tunnel Connector, and real troubleshooting notes
- [Runner](RUNNER.md) — operate the component that works beside your repositories
- [CLI](CLI.md) — commands, profiles, credentials, and operator reference

## I need authentication or network options

- [MCP](MCP.md) — Bearer, query-token fallback, OAuth, private tunnel, and MCP protocol reference
- [Authentication](AUTH_MODEL.md) — detailed credential and authority boundaries
- [Deployment](DEPLOYMENT.md) — stable HTTPS origins, self-hosting, and production networking
- [GPT Actions](GPT_ACTIONS.md) — optional OpenAPI-based Custom GPT integration

## I need help

- [Troubleshooting](TROUBLESHOOTING.md) — installation, connection, runtime, and Runner problems
- [Security](../SECURITY.md) — safe operating guidance and security model

## I want to understand or extend WebCodex

- [Architecture](ARCHITECTURE.md) — how the major components fit together
- [Coding Workflow](CODING_WORKFLOW.md) — task bootstrap, guidance, validation, and closeout
- [Computer Use roadmap](COMPUTER_USE.md) — semantic-first desktop automation direction and dogfood priorities

## I want to contribute or release WebCodex

- [AGENTS.md](../AGENTS.md) — repository instructions for coding/AI agents
- [Maintenance](MAINTENANCE.md) — maintenance queue, dependency cadence, PR/CI expectations, and bilingual-doc policy
- [Testing](TESTING.md) — testing strategy
- [Release checklist](RELEASE_CHECKLIST.md) — release readiness
- [Architecture decisions](agent/architecture-decisions.md)
- [Runtime host context](agent/runtime-host-context.md) — Runner-configured planning context and runtime diagnostics
- [Job reliability and Runner concurrency](agent/job-reliability-and-concurrency.md) — restart recovery, observation semantics, shared Job capacity, and tool-description requirements
- [Authority model](agent/permission-model.md)
- [Session model](agent/session-model.md)
- [Manual multi-window collaboration](agent/manual-window-collaboration.md)
- [OpenAPI guidelines](agent/openapi-guidelines.md)
- [Release process](agent/release-process.md)
