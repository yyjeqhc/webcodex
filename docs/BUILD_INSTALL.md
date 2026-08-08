# Build and Install Quick Reference

[English](BUILD_INSTALL.md) | [简体中文](BUILD_INSTALL.zh-CN.md)

This is the short install path. See [DEPLOYMENT.md](DEPLOYMENT.md) for production details.

## Fastest hosted install

For the official hosted shared-key path, install the CLI/Runner package on the
machine that owns the project and run one command:

```bash
npm install -g @yyjeqhc/webcodex
cd /path/to/your/repository
webcodex connect https://sg4.yyjeqhc.cn
```

The current directory is the default project. `connect` generates a strong key,
prints it once, writes an owner-only profile, starts a detached Runner, and
waits until the Server can see both the Runner and project. Use the printed
`https://sg4.yyjeqhc.cn/mcp` URL and key in the MCP client. This path needs no
local Server, database, reverse proxy, systemd unit, or sudo. The output names
the profile, configuration path, and log path. After a machine reboot, rerun
the same `connect` command or use `webcodex agent start --profile <profile>`.

## Build binaries

Build the three current binaries for your host:

```text
webcodex
webcodex-server
webcodex-runner
```

`webcodex-runner` runs shell commands the server sends rather than an agent
loop. The binary, npm command, systemd unit, and QUIC ALPN
(`webcodex-runner/1`) use that name without old-name aliases.

Do not run unauthenticated production deployments.

## Help-verified command shape

The examples in this guide were checked against the current help output from `webcodex -h`, `webcodex server -h`, and `webcodex agent -h`. Keep these flag differences in mind:

| Task | Preferred command shape |
| --- | --- |
| Hosted project connection | `webcodex connect <server-url> [--key ...] --project ...` |
| Ordinary project onboarding | `webcodex setup` |
| Project diagnostics/readiness | `webcodex doctor` / `webcodex status` |
| Server env bootstrap | `webcodex server init --listen ... --data-dir ... --env-file ...` |
| Server systemd unit | `webcodex server install --env-file ... --bin ...` |
| Server status | `webcodex server status --env-file ...` |
| Admin-created account credential | `webcodex users create --server-url ... --token ... --username ... --issue-credential` |
| User-created PAT | `webcodex token create-local --server ... --user ... --credential ... --scopes ...` |
| User-created agent token | `webcodex agent-token create-local --server ... --user ... --credential ... --client-id ...` |
| Pairing code | `webcodex pairing create --server-url ... --username ... [--client-id ...]` |
| Client enrollment (primary) | `webcodex login <server-url> --code <pairing-code>` |
| Client enrollment (advanced) | `webcodex client enroll --server-url ... --pairing-code ... --client-id ...` |
| Agent foreground run | `webcodex-runner --profile ...` |
| Runner user service | `webcodex agent install --scope user --profile ... --bin ...` |
| Runner system service | `sudo webcodex agent install --scope system --user ... --working-directory ...` |

The account-management command uses `users create` and `--server-url`; local token creation commands use `--server`. That difference comes from the current CLI surface and is intentionally reflected in the examples.

## Install packages

The documented distribution path uses the npm thin installer/wrapper:

```bash
npm install -g @yyjeqhc/webcodex
```
The npm wrapper currently supports `linux-x64`, `linux-arm64`, `darwin-arm64`, and `win32-x64`. `darwin-x64`, Windows ARM64, and other targets are not currently published. Release checksums are generated on OE from the exact native artifacts after tagging; the publish-ready `manifest.json` is release metadata and is no longer committed to Git.

### Windows x64 support scope

Windows x64 is the supported **client + Runner** target: the `webcodex` CLI, `webcodex-runner` as a hosted/local-profile Runner, Windows local repository work, and connecting to a remote Linux WebCodex Server (`webcodex connect <server>`). The Runner is started and stopped with `webcodex agent status|start|stop|restart|logs --profile <name>`, and after a machine restart it resumes with an explicit `webcodex connect ...` or `webcodex agent start --profile <name>` — automatic startup at logon/reboot is not implemented yet.

Not supported on Windows: a long-running local WebCodex Server (`webcodex server ...`, `webcodex share`), `webcodex agent install` (systemd service install), persistent shells, SSH resources, config hot reload, AppContainer sandboxing, ARM64, and UNC project roots. `webcodex-server.exe` is packaged in Windows artifacts only to keep the three-binary npm contract; it does not imply a Windows Server runtime.

Windows x64 is a published client + Runner platform starting with v0.3.3. Each future Windows release artifact is still built natively from the exact immutable tag with `scripts/package_release_artifact.ps1`; the default mode requires a concrete commit, `dirty=false`, a clean tag worktree, and matching binary provenance. `-AllowDevelopmentBuild` remains local/CI smoke only and must never produce an uploaded artifact. Native Windows validation includes `npm --prefix npm/webcodex test` and `scripts/npm_install_windows_smoke.ps1`.

The npm package is a thin wrapper around native release artifacts. During install it downloads the matching GitHub Release artifact and verifies the SHA-256 checksum from the generated release manifest. Release publication is staged on OE after all native archives exist:

```bash
python3 scripts/prepare_release_metadata.py --version <VERSION> --artifact-dir <ARTIFACT_DIR> --output-dir <METADATA_DIR>
scripts/stage_npm_release.sh --manifest <METADATA_DIR>/manifest.json --output-dir <STAGE_DIR>
WEBCODEX_NPM_PACKAGE_DIR=<STAGE_DIR>/npm-package bash scripts/npm_package_smoke.sh
```

The staging script is release-safe by default and requires the source worktree to be clean at the exact `v<VERSION>` tag. The resulting staging tree, not the source tree, is the npm publication input.

## Example files

The `deploy/` directory contains short examples you can adapt:

- `deploy/webcodex.env.example`
- `deploy/webcodex.service.example`
- `deploy/webcodex-runner.toml.example`
- `deploy/webcodex-runner.service.example`
- `deploy/nginx.webcodex.example.conf`

The nginx file is only an example. WebCodex CLI does not automate reverse proxy setup.

## Binary deployment flow

The remaining flow in this section is for full self-hosting. It is not
required when using the official hosted `connect` path.

Server:

1. Install the public `webcodex` CLI and the `webcodex-server` binary.
2. Initialize the server env file:

```bash
sudo webcodex server init \
  --listen 127.0.0.1:8080 \
  --data-dir /var/lib/webcodex \
  --env-file /etc/webcodex/webcodex.env
```

This creates only the server bootstrap/admin `WEBCODEX_TOKEN` in `/etc/webcodex/webcodex.env`. That file is server-side only; it does not create user API tokens or agent tokens.

3. Install the server service. Use `--overwrite` only when replacing an old unit.

```bash
sudo webcodex server install \
  --env-file /etc/webcodex/webcodex.env \
  --bin /usr/local/bin/webcodex-server
```

4. Reload systemd, start the service, and check status:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now webcodex
webcodex server status --env-file /etc/webcodex/webcodex.env
```

Server/admin:

5. Create a temporary one-time pairing code:

```bash
webcodex pairing create \
  --server-url https://your-domain.example \
  --env-file /etc/webcodex/webcodex.env \
  --username friendname \
  --display-name "Friend Name" \
  --ttl-secs 600
```

`pairing create` is a server/admin-side command. It needs server bootstrap/admin auth. This ordinary flow leaves the code unbound so the device running `login` can claim its automatically generated id. Copy only the short-lived `wc_pair_*` code to the client; do not copy `WEBCODEX_TOKEN`, `wc_pat_*`, `wc_agent_*`, complete env files, or complete `agent.toml` files. Each friend should use a unique `username`.

Client:

6. Install the public `webcodex` CLI and the `webcodex-runner` binary.
7. Exchange the pairing code over HTTPS and write client-side credentials/config:

```bash
webcodex login https://your-domain.example --code <wc_pair_...> \
  --allowed-root "$HOME/git"
```

Login derives a unique device name automatically (hostname + local suffix), redeems the pairing code, writes the `wc_pat_*` user token to `webcodex-user-token`, and stores the `wc_agent_*` agent token inside the generated `agent.toml`; both files use `0600` permissions on Unix. `/etc/webcodex/webcodex.env` is server-side only. For an explicit client id or a custom output directory, the advanced `webcodex client enroll` flow still works with its existing flags.

8. As that same ordinary user, install and start the Runner user service, then
   validate. Login prints the exact config path and a `--scope user` install
   command. `agent install` performs the user-manager daemon reload and
   enable/start operation itself:

```bash
webcodex agent install --scope user \
  --config /path/to/login/wrote/agent.toml
webcodex agent status --scope user \
  --config /path/to/login/wrote/agent.toml \
  --server-url https://your-domain.example
webcodex ops status \
  --server-url https://your-domain.example \
  --token-file /path/to/login/wrote/webcodex-user-token
```

GPT Actions, MCP, and ordinary REST/project APIs use the generated client-side
`webcodex-user-token`. The Agent token stored in `agent.toml` is Runner
transport-only. GPT Actions require a public HTTPS URL; WebCodex CLI does not
automate reverse proxies or tunnels.

## Runner service scopes

The npm package can be installed in an ordinary user's npm environment. Its
install location does not select the systemd service scope.

`--scope user` uses `systemctl --user`, writes the unit to
`$XDG_CONFIG_HOME/systemd/user` (or `$HOME/.config/systemd/user`), stores
default WebCodex config under `$XDG_CONFIG_HOME/webcodex` (or
`$HOME/.config/webcodex`), uses `default.target`, and runs without `User=` or
`Group=`. It requires no `sudo`. Use the same scope for every lifecycle
command:

```bash
webcodex agent install --scope user --profile workstation
webcodex agent status --scope user --profile workstation
webcodex agent restart --scope user --profile workstation
webcodex agent logs --scope user --profile workstation --lines 100
webcodex agent uninstall --scope user --profile workstation --confirm
```

An enabled user unit starts when that account's user manager starts. It does not
by itself guarantee that the Runner starts at boot before the first login or
remains after the last logout. If unattended boot persistence is required, an
administrator may explicitly run `sudo loginctl enable-linger <runner-user>`
after reviewing the long-lived service authority granted to that account.
WebCodex does not change lingering automatically.

Non-root callers default to user scope. Root callers default to system scope,
but a system Runner may not run as root without explicit opt-in. The normal
administrator-managed installation names a non-root account and a matching
working directory; `--group` is optional:

```bash
sudo webcodex agent install \
  --scope system \
  --profile workstation \
  --user <runner-user> \
  --working-directory /home/<runner-user> \
  --config /etc/webcodex/clients/workstation/agent.toml
sudo webcodex agent status --scope system --profile workstation
```

System scope uses `/etc/systemd/system`, `systemctl`, and
`multi-user.target`. It does not create users, change sudoers, migrate files,
or overwrite an existing unit unless `--overwrite` is explicit. Running
project commands as root is strongly discouraged and is accepted only with
`--allow-root-runner`, which prints and embeds a prominent warning. Explicit
`--config` and `--working-directory` values override the defaults and must be
readable or usable by the selected service account. An explicit
`--service-file` must belong to the selected manager scope; reuse the same
`--scope` and `--service-file` for status, control, logs, and uninstall.

Compatibility commands still work, but should not be the first choice in new docs:

```bash
webcodex users ...
webcodex tokens ...
webcodex agent-tokens ...
webcodex setup single-user
```

## Agent config

Client enroll writes `agent.toml`. For the normal user service, use
`webcodex agent install --scope user`; for a foreground test, run:

```bash
webcodex-runner --profile workstation
```

For advanced manual generation, use the single low-level entry
`webcodex agent init`. The `webcodex-runner init` alias was removed.

## Project readiness

For an ordinary Git project, use the canonical read-only diagnostics:

```bash
webcodex setup
webcodex doctor
webcodex agent start
webcodex status
```

`doctor` checks the current project configuration, registration, Git
workspace, Agent runtime, connection, Agent registration, required coding
capabilities, and structured validation without modifying state.

For an advanced multi-client deployment, keep project readiness separate from
operator fleet diagnostics:

```bash
webcodex agent status \
  --profile workstation \
  --server-url https://your-domain.example
webcodex ops status \
  --server-url https://your-domain.example \
  --token-file /etc/webcodex/clients/workstation/webcodex-user-token
```

These commands never make transport/fleet discovery a prerequisite for the
ordinary Connector coding path. See [SHELL_PROFILES.md](SHELL_PROFILES.md) for
advanced profile config and troubleshooting.

Agent policy defaults:

- Missing or empty `allowed_roots` defaults to `$HOME`.
- Explicit `allowed_roots` replaces the `$HOME` default.
- To narrow an agent, set an explicit workspace root such as:

```toml
[policy]
allowed_roots = ["/root/git"]
```

The example above is a narrowing example, not the default.

## Auth reminders

Use a user token such as the generated `webcodex-user-token` (`wc_pat_*`):

```text
Authorization: Bearer <token>
```

for REST, polling, MCP, and GPT Actions.

Use `webcodex-runner-token` (`wc_agent_*`) only through the Runner config for
Agent transport. It is intentionally rejected by project/runtime APIs.

`?token=` is allowed only for `/api/agents/ws` WebSocket handshake compatibility.

## systemd PATH reminder

systemd services do not read interactive shell startup files such as `~/.bashrc`. If commands need Rust/Cargo, Node, or Codex CLI, expose them through configured agent shell profiles or through the service manager's environment.

WebCodex no longer exposes `run_codex` or legacy `/api/codex/*` routes. Run Codex outside WebCodex for Codex-specific workflows.
