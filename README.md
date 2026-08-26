# WebCodex

[English](README.md) | [简体中文](README.zh-CN.md)

**WebCodex lets ChatGPT, Claude, and other AI agents work directly with code and developer tools on your own machines.**

Ask your assistant to inspect a repository, modify code, run tests, use Git, or investigate a failure. Your repository stays on the machine where it already lives; you do not need to move the project into a hosted workspace just to use an AI coding agent.

## Quick start

On Linux or macOS, with Node.js 18+ and Git installed:

```bash
cd /path/to/your/repository
npx --yes @yyjeqhc/webcodex
```

When WebCodex reports **WebCodex ready**, keep the terminal open. The **MCP URL** is normally copied to your clipboard; press **Enter** in the terminal to open ChatGPT App settings, or open **Settings -> Apps -> Create** manually. Then:

1. Enable **Developer Mode** if needed and choose **Create**.
2. Paste the copied **MCP URL** (or use the URL printed by WebCodex).
3. Choose **Access token / API key** (or the equivalent Bearer-token option) and paste the printed **Credential**.
4. Run **Scan Tools**.

Developer Mode, custom MCP Apps, and write/modify actions depend on your ChatGPT plan, workspace, and administrator policy. WebCodex cannot enable capabilities the client does not grant.

Try a read-only first request:

```text
Inspect this repository and summarize its structure. Do not make changes.
```

If ChatGPT does not show a Bearer/access-token option, or tries OAuth discovery and reports **does not implement OAuth**, use:

```bash
npx --yes @yyjeqhc/webcodex share --auth query-token
```

Paste the complete `/mcp?token=...` URL and choose **No authentication**. This fallback requires WebCodex 0.3.9 or later. That URL contains a temporary secret, so do not publish or log it. If WebCodex is installed globally, the equivalent command is `webcodex share --auth query-token`. See the [Quick Start](docs/QUICK_START.md) and [MCP setup guide](docs/MCP.md) for details and other clients.

The default one-command flow creates a temporary public HTTPS MCP endpoint protected by that run's temporary credential; both the endpoint and credential stop working when the command exits. Advanced networking, OAuth, private tunnels, and self-hosting options are documented separately.

## What can it do?

- **Understand and edit code** — read, search, inspect, and make guarded changes inside configured projects.
- **Use the real toolchain** — run commands, tests, formatters, compilers, and project-specific tooling on the machine that owns the repository.
- **Work with Git** — inspect status and diffs while keeping repository operations visible and reviewable.
- **Handle long-running work** — keep jobs observable instead of requiring one model turn to stay open indefinitely.
- **Support human review** — use the Runtime Console and task workflow to guide, cancel, accept, or reject work where those actions are available.

## Why WebCodex?

- **Your code stays on your machine.** The repository does not need to be copied into the chat service.
- **The agent gets a real development environment.** It can use the same files, Git checkout, compilers, tests, and tools you already use.
- **Work survives beyond a single request.** Long-running execution and evidence remain observable through WebCodex.
- **Start temporary or run it long-term.** Use one-command sharing for a quick session, or connect machines to a self-hosted Server for a durable setup.

## How it works

```text
AI client
   |
   | MCP / HTTPS
   v
WebCodex
   |
   v
your machine
   |
   +-- repository
   +-- Git
   +-- compilers / tests / developer tools
```

For the internal Server/Runner architecture, protocol surfaces, and authority boundaries, see [Architecture](docs/ARCHITECTURE.md), [MCP](docs/MCP.md), and [Authentication](docs/AUTH_MODEL.md).

## Platforms

- **Linux x64/arm64** — local `share`, Server, and Runner workflows.
- **macOS arm64** — local `share` and Runner workflows.
- **Windows x64/arm64** — CLI + Runner against a remote Linux Server. Local `webcodex share` is not supported on Windows in this release.

Windows and long-lived deployments are covered in [Deployment](docs/DEPLOYMENT.md) and [MCP](docs/MCP.md).

## Long-lived and advanced setup

If you want to keep the CLI installed:

```bash
npm install -g @yyjeqhc/webcodex
```

If an operator has provided a shared key for an existing hosted WebCodex Server, use `webcodex connect <server-url>`. For a newly deployed self-hosted Server, keep the bootstrap administrator token on the Server and use the [Deployment](docs/DEPLOYMENT.md) guide's short-lived pairing code + `webcodex login` enrollment flow.

For production/self-hosted deployment, Windows enrollment, OAuth, private tunnels, proxy/CA configuration, and other operator workflows, use the documentation below rather than the first-run path.

## Documentation

- [Quick Start](docs/QUICK_START.md) — from zero to the first successful AI connection
- [MCP](docs/MCP.md) — ChatGPT, Claude, authentication choices, and MCP reference
- [Deployment](docs/DEPLOYMENT.md) — self-hosting, permanent Servers, and machine enrollment
- [Troubleshooting](docs/TROUBLESHOOTING.md) — connection and runtime problems
- [CLI](docs/CLI.md) — command and credential reference
- [AI-assisted setup](docs/AI_ONBOARDING.md) — have an AI agent help configure WebCodex
- [Security](SECURITY.md) — security model and operational guidance
- [Documentation index](docs/INDEX.md) — all user and contributor documentation

## Security

WebCodex can read and modify files and execute commands inside configured project boundaries. Use version control, keep credentials out of prompts/logs/Git, and register only project roots the assistant should access. Read [SECURITY.md](SECURITY.md) for the complete model.

## Build from source

```bash
cargo build --release --workspace --bins
export PATH="$PWD/target/release:$PATH"
```

## Contributing

Contributions are welcome, including contributions created with WebCodex itself or other coding agents. For bug reports, development workflow, and pull request guidance, see [CONTRIBUTING.md](CONTRIBUTING.md).

## Acknowledgements

Thanks to the [LINUX DO](https://linux.do/) community for its welcoming space for technical discussion and support for open-source sharing.

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
