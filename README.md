# WebCodex

[English](README.md) | [简体中文](README.zh-CN.md)

WebCodex connects online AI clients such as ChatGPT or Claude to repositories
and development tools running on your own machines. Through a WebCodex Server
and a Runner on the machine that owns the code, an AI assistant can inspect and
edit files, run commands and tests, work with Git, and use the local toolchain —
from the chat window you already use.

See the [latest release](https://github.com/yyjeqhc/webcodex/releases/latest)
and the [documentation index](docs/INDEX.md).

## What it does

- **Files and source inspection** — read, search, and list files inside
  registered projects.
- **Guarded edits** — structured file edits and checked patches, applied within
  project boundaries.
- **Git** — status, diffs, and focused commit preparation.
- **Commands and tests** — bounded shell commands and structured validation
  (Rust, Node, Python, Go).
- **Long-running Jobs** — work that outlives a single chat turn continues as an
  observable, queryable Job.
- **Multiple Runner machines and projects** — one Server can route work to many
  machines that own repositories.
- **MCP** — connect from ChatGPT, Claude, or any MCP client.
- **Optional GPT Actions** — an OpenAPI-based integration for Custom GPTs.

The exact tools available depend on the Server surface, the Runner
capabilities, and the permissions you configure.

## How it works

```text
AI client
   |
   | MCP / HTTPS (or GPT Actions)
   v
WebCodex Server
   |
   | authenticated Runner connection
   v
webcodex-runner
   |
   repository / Git / toolchains
```

The Server authenticates callers and routes tool requests. The Runner does the
actual work on the machine that owns the repository. Repositories and local
toolchains stay on the Runner host; only the requested tool inputs and results
travel across the connection.

## Quick start

Three common paths:

**1. Connect a repository to an existing Server**

```bash
npm install -g @yyjeqhc/webcodex
cd /path/to/your/repository
webcodex connect https://your-server.example
```

`connect` uses the current directory as the project, creates a local profile,
starts a detached Runner, and prints the MCP URL and a generated key. Paste the
URL and key into your MCP client, then ask for a real task.

**2. Temporarily share one local project**

```bash
webcodex share
```

`share` starts a local Server + Runner and a Cloudflare Quick Tunnel, then
prints a temporary HTTPS `/mcp` URL and Bearer credential for that session. It
is for development and testing, not production.

**3. Self-host a Server**

Deploy the server-only Docker/Compose stack on an always-on host, put a stable
HTTPS domain in front of it, then enroll each repository machine as a Runner
with `webcodex login`.

The detailed steps for each path are in
[Quick Start](docs/QUICK_START.md), and full production setup is in
[Deployment](docs/DEPLOYMENT.md).

## CLI

The `webcodex` CLI covers project setup, Server and Runner lifecycle, device
enrollment, and operator checks. Representative commands:

```bash
webcodex connect https://your-server.example   # connect a repo to a hosted Server
webcodex share                                 # temporarily share the local project
webcodex login https://your-server.example --code <wc_pair_...>
webcodex setup                                 # project-first local setup
webcodex doctor                                # read-only readiness checks
webcodex run                                   # start the project-bound runtime
webcodex agent status --profile <profile>      # inspect the Runner
webcodex ops status                            # operator read-only checks
webcodex task list                             # review tasks and decide locally
```

See the [CLI reference](docs/CLI.md) for the full command map, terminology, and
credentials.

## Let an AI agent set it up

Prefer having a coding agent configure WebCodex for you? Give the agent this
repository and ask it to read `docs/AI_ONBOARDING.md` first. A copyable prompt:

```text
Read docs/AI_ONBOARDING.md and help me connect or deploy WebCodex.
Verify the current machine and existing configuration first.
Do not print or copy secrets; tell me when I need to enter a credential.
```

Two different guides serve two different purposes:

- `docs/AI_ONBOARDING.md` is for an AI agent helping a **user** install,
  connect, or deploy WebCodex.
- `AGENTS.md` is for an AI coding agent **developing WebCodex itself**.

## Documentation

- [Quick Start](docs/QUICK_START.md) — shortest working setup
- [AI-assisted setup](docs/AI_ONBOARDING.md) — for an AI agent helping you
- [CLI](docs/CLI.md) — command map, terminology, credentials
- [Deployment](docs/DEPLOYMENT.md) — self-hosting and production operations
- [Authentication](docs/AUTH_MODEL.md) — credentials and tokens
- [Runner](docs/RUNNER.md) — what the Runner/agent is and how to operate it
- [MCP](docs/MCP.md) — connecting MCP clients
- [Troubleshooting](docs/TROUBLESHOOTING.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Security](SECURITY.md)

## Security

WebCodex can modify files and execute commands, so treat a connected client as
a development assistant with real access to the configured machines. Register
only the project roots the assistant should use, keep tokens out of prompts,
logs, and Git, and prefer an ordinary OS user for the Runner. Read
[SECURITY.md](SECURITY.md) for the complete model.

## Build from source

```bash
cargo build --release --workspace --bins
export PATH="$PWD/target/release:$PATH"
```

## Acknowledgements

Thanks to the [LINUX DO](https://linux.do/) community for its welcoming space
for technical discussion and support for open-source sharing.

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
