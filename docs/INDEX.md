# Documentation Index

[English](INDEX.md) | [简体中文](INDEX.zh-CN.md)

## Getting started

- [README](../README.md) — what WebCodex is and the `webcodex share` first-run path
- [Quick Start](QUICK_START.md) — shortest path from a local repository to ChatGPT MCP
- [MCP](MCP.md) — ChatGPT Developer Mode and other MCP client setup
- [AI-assisted setup](AI_ONBOARDING.md) — have an AI agent configure WebCodex for you

## Common operation

- [CLI](CLI.md) — command map, compatibility terminology, and credentials
- [Coding Workflow](CODING_WORKFLOW.md) — canonical task bootstrap, behavioral guidance, validation, and closeout
- [Runner](RUNNER.md) — operating the Runner on repository machines

## Advanced deployment and integration

- [Deployment](DEPLOYMENT.md) — self-hosting, Server bootstrap, and long-lived Runner enrollment
- [Authentication](AUTH_MODEL.md) — detailed credential and token boundaries
- [GPT Actions](GPT_ACTIONS.md) — optional OpenAPI-based Custom GPT integration

## Understanding WebCodex

- [Architecture](ARCHITECTURE.md) — how the pieces fit together
- [Computer Use roadmap](COMPUTER_USE.md) — semantic-first desktop automation direction and dogfood priorities
- [Security](../SECURITY.md) — security model and policy

## Help

- [Troubleshooting](TROUBLESHOOTING.md)

## Contributing

- [AGENTS.md](../AGENTS.md) — repository instructions for coding/AI agents
- [Maintenance](MAINTENANCE.md) — canonical maintenance queue, dependency cadence, PR/CI expectations, and bilingual-doc policy
- [Testing](TESTING.md) — testing strategy
- [Release checklist](RELEASE_CHECKLIST.md) — release readiness
- [Architecture decisions](agent/architecture-decisions.md)
- [Runtime host context](agent/runtime-host-context.md) — bounded Runner-configured planning context and exact-source runtime diagnostics
- [Job reliability and Runner concurrency](agent/job-reliability-and-concurrency.md) — Control restart recovery, observation semantics, shared Job capacity, and tool-description requirements
- [Authority model](agent/permission-model.md)
- [Session model](agent/session-model.md)
- [Manual multi-window collaboration](agent/manual-window-collaboration.md) — coordinator/worker handoff through existing Workflow Session message primitives
- [OpenAPI guidelines](agent/openapi-guidelines.md)
- [Release process](agent/release-process.md)
