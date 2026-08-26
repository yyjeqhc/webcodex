# WebCodex

[English](README.md) | [简体中文](README.zh-CN.md)

WebCodex lets ChatGPT, Claude, and other MCP clients work with repositories and
development tools on your own machines. The Runner executes file, Git, command,
and test operations where the repository lives; WebCodex exposes those
capabilities to the chat client without requiring the repository itself to move.

## Try it with one repository

Platform note: `webcodex share` starts a local WebCodex Server and is supported on
Linux and macOS. Windows builds support the CLI + Runner against a remote Linux
Server; on Windows use `webcodex connect <server-url>`. If you do not already
have a Server, deploy one on Linux first.

For the fastest Linux/macOS trial, no global install is required:

```bash
cd /path/to/your/repository
npx --yes @yyjeqhc/webcodex
```

The npm wrapper lazily bootstraps the verified native binary set if lifecycle
installation did not leave it behind. If you prefer a persistent CLI, install it
once and then use bare `webcodex` inside a Git repository:

```bash
npm install -g @yyjeqhc/webcodex
cd /path/to/your/repository
webcodex
```

In an interactive Linux/macOS Git repository, bare `webcodex` is a convenience
alias for the normal `webcodex share` first-run path. Scripts, non-interactive
calls, Windows, and directories outside a Git checkout do not auto-start a
runtime; use explicit `webcodex share` when deterministic dispatch matters.

For the default temporary public share, WebCodex reuses `cloudflared` from
`WEBCODEX_CLOUDFLARED_BIN` or `PATH` when available. Otherwise it downloads and
verifies a WebCodex-managed copy automatically. When launched through the npm
wrapper, that managed download also reuses npm's proxy, `noproxy`, CA, and
`strict-ssl` settings; otherwise standard proxy/system trust behavior remains
available. `share` is self-contained: it prepares the tunnel dependency when
needed, configures the current Git project,
starts a local WebCodex Server + Runner,
creates a temporary Connector credential, and opens a Cloudflare Quick Tunnel.
You do **not** need to run `setup`, `doctor`, or `run` first.

When the command reports **WebCodex ready**, keep that terminal open. For a
public share, WebCodex best-effort copies the **MCP URL** to the clipboard; the
credential is never copied automatically. In an interactive terminal, press
Enter to open ChatGPT App settings, then:

1. In ChatGPT, enable **Developer Mode** and go to **Settings -> Apps -> Create**.
2. Paste the copied **MCP URL** (or copy the printed fallback URL).
3. Choose **Access token / API key** or the equivalent Bearer-token option.
4. Paste the printed temporary **Credential**.
5. Run **Scan Tools**.
6. Start with a read-only prompt such as:

```text
Inspect this repository and summarize its structure. Do not make changes.
```

ChatGPT UI labels can vary by workspace and rollout. Developer Mode, custom MCP
apps, and write/modify actions are controlled by the ChatGPT plan, workspace, and
admin settings; WebCodex cannot widen client-side app permissions. The CLI output
is the source of truth for the WebCodex URL, authentication type, and credential
for that run.
Use `webcodex share --no-copy-url` when clipboard access is undesirable.

A default `share` URL and credential are temporary and stop working when the
command exits. `webcodex share --tunnel none` is available for local-only MCP
debugging and does not require `cloudflared`.

### Optional: OpenAI Secure MCP Tunnel

If the repository should be reachable only from a supported OpenAI product, use
`webcodex share --tunnel openai`. Create/select a Secure MCP Tunnel in the OpenAI
Platform first, then export `CONTROL_PLANE_TUNNEL_ID` and a Restricted
`CONTROL_PLANE_API_KEY` with **Tunnels Read + Use**. WebCodex reuses a matching
`tunnel-client` from `WEBCODEX_TUNNEL_CLIENT_BIN` or `PATH`, or downloads and
verifies pinned OpenAI `tunnel-client` v0.0.12 for Linux/macOS amd64/arm64.

This provider keeps the temporary WebCodex Bearer credential in private local
share state and gives `tunnel-client` a file-backed `Authorization` header for
the loopback MCP hop. In ChatGPT choose **Connection: Tunnel**, select/paste the
Tunnel ID, and choose **No authentication**; do not paste the local WebCodex
credential into ChatGPT. `--tunnel openai` currently supports the default
`--auth bearer` path only. Ctrl-C stops the local runtime and `tunnel-client` and
removes the temporary WebCodex credential; the Platform Tunnel identity remains
operator-managed for later reuse.

## What happens after the first connection

WebCodex can read/search files, prepare guarded edits, run commands and focused
validation, inspect Git, and keep long-running Jobs observable. Coding results
remain subject to the product's existing authority boundaries. Open `/console`
to inspect project readiness and the work queue; the Console can guide, cancel,
Accept, or Reject work where those actions are available. It deliberately does
not reveal credentials.

## Existing Server and long-lived deployments

For a hosted Server whose operator intentionally gave you a shared key, connect
the current repository with that shared-key identity:

```bash
cd /path/to/your/repository
webcodex connect https://webcodex.example --key-file /private/path/shared-key
```

`connect` creates a reusable local profile, starts the Runner, waits for the
project to become visible through that Server, and prints the MCP setup values.
This is the hosted shared-key path; it is not the enrollment path for a freshly
self-hosted Docker Server.

For a fresh self-hosted Server, keep its bootstrap administrator token on the
Server. Create a short-lived pairing code there, then use `webcodex login` with
that `wc_pair_...` code on the repository machine and explicitly install the
reported Runner config as a user service. Managed OAuth remains an advanced
identity option. See [Deployment](docs/DEPLOYMENT.md) for the complete flow.

## How it works

```text
ChatGPT / Claude / MCP client
            |
            | MCP / HTTPS
            v
      WebCodex Server
            |
            | authenticated Runner connection
            v
     webcodex-runner
            |
      repository / Git / toolchains
```

The Server authenticates callers and routes requests. The Runner performs the
actual work on the machine that owns the repository. Only requested tool inputs
and results cross the connection.

## CLI

The common commands are intentionally small:

```bash
webcodex share                     # fastest temporary ChatGPT/MCP connection
webcodex connect <server-url>      # connect to an existing Server
webcodex status                    # concise project readiness
webcodex doctor                    # deeper local diagnostics
webcodex setup                     # manual/local project setup
webcodex run                       # manual local Server + Runner runtime
webcodex task list                 # review local tasks
```

The Runner is the execution component. For compatibility, Runner service
management still uses the historical `webcodex agent ...` CLI namespace. See
[CLI](docs/CLI.md) for operator commands and credential reference.

## Documentation

- [Quick Start](docs/QUICK_START.md) — first ChatGPT/MCP connection
- [MCP](docs/MCP.md) — ChatGPT, Claude, authentication, then protocol reference
- [AI-assisted setup](docs/AI_ONBOARDING.md) — instructions for an AI helping a user configure WebCodex
- [CLI](docs/CLI.md) — commands, compatibility notes, and credentials
- [Deployment](docs/DEPLOYMENT.md) — self-hosting and production operations
- [Authentication](docs/AUTH_MODEL.md) — credential and authority model
- [Runner](docs/RUNNER.md) — Runner operation
- [Coding Workflow](docs/CODING_WORKFLOW.md) — task workflow, validation, and closeout
- [Troubleshooting](docs/TROUBLESHOOTING.md)
- [Documentation index](docs/INDEX.md)
- [Security](SECURITY.md)

## Security

WebCodex can modify files and execute commands. Register only project roots the
assistant should access, keep credentials out of prompts/logs/Git, and prefer an
ordinary OS user for the Runner. The simplified onboarding does not collapse
the underlying credential or authority boundaries. Read [SECURITY.md](SECURITY.md)
for the complete model.

## Build from source

```bash
cargo build --release --workspace --bins
export PATH="$PWD/target/release:$PATH"
```

## Contributing

Contributions are welcome, including contributions created with WebCodex itself
or other coding agents. For bug reports, development workflow, and pull request
guidance, see [CONTRIBUTING.md](CONTRIBUTING.md).

## Acknowledgements

Thanks to the [LINUX DO](https://linux.do/) community for its welcoming space
for technical discussion and support for open-source sharing.

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
