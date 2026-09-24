<p align="right"><a href="README.md">English</a> · <a href="README.zh-CN.md">简体中文</a></p>

<p align="center"><img src="docs/assets/brand/webcodex-app-icon.png" alt="WebCodex" width="96" height="96"></p>

<h1 align="center">WebCodex</h1>

<p align="center"><strong>Give cloud AI agents a real development environment on your own machines.</strong></p>
<p align="center">Connect ChatGPT, Claude, and other MCP clients to the repositories, Git checkout, and tools you already use.</p>
<p align="center"><a href="#just-trying-it-for-a-few-minutes-temporary-share">Quick Trial</a> · <a href="#download-a-release">Downloads</a> · <a href="docs/PERSONAL_SETUP.md">Full Setup</a> · <a href="#documentation">Documentation</a> · <a href="https://github.com/yyjeqhc/webcodex/issues">Issues</a> · <a href="CONTRIBUTING.md">Contribute</a> · <a href="SECURITY.md">Security</a></p>

<p align="center">
  <a href="docs/MCP.md"><img src="https://img.shields.io/badge/protocol-MCP-2563EB?labelColor=1E40AF&amp;style=flat-square" alt="MCP protocol"></a>
  <a href="docs/QUICK_START.md#prerequisites"><img src="https://img.shields.io/badge/Node.js-18%2B-0D9488?labelColor=0F766E&amp;style=flat-square" alt="Node.js 18 or newer"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-2563EB?labelColor=1E40AF&amp;style=flat-square" alt="Apache 2.0 license"></a>
</p>

Ask your assistant to inspect a repository, modify code, run tests, use Git, or investigate a failure. Your repository stays on the machine where it already lives; you do not need to move the project into a hosted workspace just to use an AI coding agent.

<p align="center">
  <a href="https://trendshift.io/repositories/121867">
    <img src="https://trendshift.io/api/badge/repositories/121867" alt="WebCodex | GitHub Trending" width="250" height="55" />
  </a>
</p>

## Start using WebCodex

### Just trying it for a few minutes: temporary share

To quickly see whether WebCodex fits your workflow, run this inside one repository:

```bash
cd /path/to/your/repository
npx --yes @yyjeqhc/webcodex share
```

`share` starts a temporary, single-project instance of the ordinary WebCodex Adaptive Runtime and prints the ChatGPT connection values. Its temporary Project Credential limits access to that ProjectGrant; the endpoint and credential stop working when the command exits. It is intended for trials and short-lived sharing, not as the default full daily setup. See the [Quick Trial](docs/QUICK_START.md) for the exact steps.

### Everyday development: full WebCodex (recommended)

If you want ChatGPT to keep using your real development environment, start with a **regular Server + Runner**. This is the full development experience: durable access to multiple projects plus project exploration, editing, Git, commands, tests, long-running work, and code navigation. Public HTTPS, Cloudflare Tunnel, and OpenAI Secure MCP Tunnel are only ways for ChatGPT to reach the Server; they do not switch you into a different restricted experience.

For Windows or macOS, the recommended first path is **WebCodex Desktop + the official OpenAI Secure Tunnel**. Follow the [Desktop installation guide](docs/desktop-install.md). For CLI, an existing Server, self-hosting, or advanced setup, use the [Full Setup guide](docs/PERSONAL_SETUP.md).

### Download a release

The links below are for the **official upstream v0.4.1 release**. Choose a Desktop installer for ordinary Windows/macOS use, or a native CLI archive for Server/Runner workflows. [View the release and checksums](https://github.com/yyjeqhc/webcodex/releases/tag/v0.4.1).

| Platform | Desktop | CLI / Server / Runner |
| --- | --- | --- |
| Windows x64 | [Installer](https://github.com/yyjeqhc/webcodex/releases/download/v0.4.1/webcodex-desktop-v0.4.1-win32-x64-setup.exe) | [Archive](https://github.com/yyjeqhc/webcodex/releases/download/v0.4.1/webcodex-v0.4.1-win32-x64.tar.gz) |
| Windows arm64 | — | [Archive](https://github.com/yyjeqhc/webcodex/releases/download/v0.4.1/webcodex-v0.4.1-win32-arm64.tar.gz) |
| macOS Apple Silicon | [DMG](https://github.com/yyjeqhc/webcodex/releases/download/v0.4.1/webcodex-desktop-v0.4.1-darwin-arm64.dmg) | [Archive](https://github.com/yyjeqhc/webcodex/releases/download/v0.4.1/webcodex-v0.4.1-darwin-arm64.tar.gz) |
| macOS Intel | [DMG](https://github.com/yyjeqhc/webcodex/releases/download/v0.4.1/webcodex-desktop-v0.4.1-darwin-x64.dmg) | [Archive](https://github.com/yyjeqhc/webcodex/releases/download/v0.4.1/webcodex-v0.4.1-darwin-x64.tar.gz) |
| Linux x64 | — | [Archive](https://github.com/yyjeqhc/webcodex/releases/download/v0.4.1/webcodex-v0.4.1-linux-x64.tar.gz) |
| Linux arm64 | — | [Archive](https://github.com/yyjeqhc/webcodex/releases/download/v0.4.1/webcodex-v0.4.1-linux-arm64.tar.gz) |

The npm installation path remains `npm install -g @yyjeqhc/webcodex` (Node.js 18+).

## What can it do?

- **Understand and edit code** — read, search, inspect, and make guarded changes inside configured projects.
- **Use the real toolchain** — run commands, tests, formatters, compilers, and project-specific tooling on the machine that owns the repository.
- **Work with Git** — inspect status and diffs while keeping repository operations visible and reviewable.
- **Handle long-running work** — keep jobs observable instead of requiring one model turn to stay open indefinitely.
- **Support human review** — use the [Runtime Console](docs/runtime-console.md), Workflow Session evidence, Jobs, and Git/diff review without a separate task/result acceptance subsystem.

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

## Star History

This chart tracks the upstream [yyjeqhc/webcodex](https://github.com/yyjeqhc/webcodex) repository.

<a href="https://www.star-history.com/yyjeqhc/webcodex">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=yyjeqhc/webcodex&amp;type=Date&amp;theme=dark">
    <img alt="Star history of yyjeqhc/webcodex" src="https://api.star-history.com/svg?repos=yyjeqhc/webcodex&amp;type=Date">
  </picture>
</a>

## Platforms

The platform capabilities below describe the official upstream release linked above.

- **Linux x64/arm64** — local `share`, Server, and Runner workflows.
- **macOS x64/arm64** — Desktop local Server + Runner, OpenAI Secure Tunnel, local `share`, and standalone Runner workflows.
- **Windows x64** — Desktop local Server + Runner with the official OpenAI Secure Tunnel, plus CLI + Runner, local foreground Server, and explicit `webcodex share --tunnel cloudflare|openai|none`.
- **Windows arm64** — Desktop local Server + Runner and the official OpenAI Secure Tunnel are supported by the native ARM64 build path; CLI + Runner, local foreground Server, and `share` are also supported. Release builds include the Windows ARM64 Desktop installer from v0.4.2+. The pinned Cloudflare release has no official Windows ARM64 artifact, so Cloudflare still requires a trusted explicit/PATH `cloudflared`. WebCodex-managed Windows Server services remain unsupported outside Desktop's owned foreground runtime.

Windows and long-lived deployments are covered in [Deployment](docs/DEPLOYMENT.md) and [MCP](docs/MCP.md).

## Existing Servers and advanced setup

If someone already provides the WebCodex Server and connection credential, use that existing Server and follow the [Full Setup guide](docs/PERSONAL_SETUP.md). For a normal Windows/macOS personal installation, use the [Desktop guide](docs/desktop-install.md). Use [Deployment](docs/DEPLOYMENT.md) only for production hosting, multiple users, systemd/Docker, OAuth, proxies, and private CAs.

Those are follow-up operating concerns, not concepts a first-time user should have to learn before WebCodex works.

## Documentation

- [Desktop installation](docs/desktop-install.md) — recommended Windows/macOS path: Desktop + official OpenAI Secure Tunnel
- [Using Desktop](docs/desktop-guide.md) — projects, connections, activity, and background operation
- [Desktop development](docs/DESKTOP_DEVELOPMENT.md) — run from source and build Windows/macOS installers locally
- [Full Setup](docs/PERSONAL_SETUP.md) — CLI, existing Server, Linux, and advanced regular Server + Runner setup
- [Quick Trial](docs/QUICK_START.md) — temporarily try one repository with `share`
- [MCP](docs/MCP.md) — ChatGPT, Claude, authentication choices, and MCP reference
- [Deployment](docs/DEPLOYMENT.md) — production, self-hosting, and advanced operations
- [Troubleshooting](docs/TROUBLESHOOTING.md) — ChatGPT/MCP Host, connection, and runtime problems
- [CLI](docs/CLI.md) — command and credential reference
- [AI-assisted setup](docs/AI_ONBOARDING.md) — have an AI agent help configure WebCodex
- [Security](SECURITY.md) — security model and operational guidance
- [Documentation index](docs/INDEX.md) — all user and contributor documentation

## Security

WebCodex can read and modify files and execute commands inside configured project boundaries. Use version control, keep credentials out of prompts/logs/Git, and register only project roots the assistant should access. Tool results, including requested file excerpts, may be returned to the AI client. Read [SECURITY.md](SECURITY.md) for the complete model.

## Build from source

If a release has a problem, you do not need to wait for a maintainer to produce a
new build. Reproducing the issue on current `main`, testing a focused fix locally,
and submitting the result as a pull request are all welcome.

For CLI / Server / Runner development, install [Git](https://git-scm.com/) and the
stable Rust toolchain through [rustup](https://rustup.rs/), then build the normal
dogfood profile:

```bash
cargo build --locked --profile dogfood --workspace --bins
```

The binaries are written under `target/dogfood/`. For Desktop/frontend work, also
install Node.js 22 + npm and the native platform toolchain. Windows Desktop
development needs an MSVC/Windows SDK environment; macOS development needs Xcode
Command Line Tools. The complete prerequisites and source-build workflow are in the
[Desktop development guide](docs/DESKTOP_DEVELOPMENT.md).

For local Desktop installer testing, use the repository helpers rather than a raw
Tauri bundle command:

```powershell
# Windows: clean committed source
.\scripts\build_desktop_windows_local.ps1

# Windows: explicitly package uncommitted local work as dirty dogfood
.\scripts\build_desktop_windows_local.ps1 -AllowDirty
```

```bash
# macOS
bash scripts/build_desktop_macos_local.sh
```

These are development/dogfood builds, not public release artifacts.

## Contributing

Issues and focused pull requests are welcome. Maintainer response time may vary, so
users who can reproduce a problem on current `main` are especially encouraged to
investigate it, build and test a fix locally, and submit a PR rather than waiting for
the fix to be implemented upstream. WebCodex itself, Codex, ChatGPT, Claude, or other
coding agents may be used to help inspect, edit, and validate the repository.

For what to include in a bug report, the self-service fix workflow, validation
expectations, and pull request guidance, see [CONTRIBUTING.md](CONTRIBUTING.md).

## Acknowledgements

Thanks to the [LINUX DO](https://linux.do/) community for its welcoming space for technical discussion and support for open-source sharing.

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).

## Desktop Shell and Runtime upgrades

Desktop can keep its existing shell while using a separately selected compatible Runtime folder. Build revisions and package versions are diagnostic identity, not compatibility gates. See [Desktop Runtime compatibility](docs/DESKTOP_RUNTIME_COMPATIBILITY.md) for custom builds, safe switching/rollback, diagnostics, tracing and update notifications.
