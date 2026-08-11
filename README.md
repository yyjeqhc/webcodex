# WebCodex

[English](README.md) | [简体中文](README.zh-CN.md)

[![CI](https://github.com/yyjeqhc/webcodex/actions/workflows/ci.yml/badge.svg)](https://github.com/yyjeqhc/webcodex/actions/workflows/ci.yml)
[![npm](https://img.shields.io/npm/v/%40yyjeqhc%2Fwebcodex)](https://www.npmjs.com/package/@yyjeqhc/webcodex)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE)

[Latest release](https://github.com/yyjeqhc/webcodex/releases/latest) ·
[0.3.5 release notes](docs/RELEASE_NOTES_v0.3.5.md) ·
[Documentation](docs/INDEX.md)

**Turn your online AI chat into a personal assistant connected to your own machines.**

WebCodex connects ChatGPT, Claude, and other MCP clients to local repositories,
workstations, and servers. From the chat window you already use, the assistant
can inspect and edit files, run commands and tests, work with Git, and use the
toolchains installed on the machine that owns the project.

| Work from ChatGPT over MCP | Inspect changes and runtime state |
| --- | --- |
| ![MCP session](docs/assets/mcp-1.png) | ![WebCodex console](docs/assets/gpt-action-1.png) |

## Quick start

Supported package platforms are Linux x64, Linux arm64, macOS arm64, and Windows x64. Starting with v0.3.5, the native Linux x64 release artifact targets glibc 2.17 or newer; Linux arm64 does not yet carry the same glibc compatibility guarantee. The npm installer requires Node.js 18 or newer, so npm-based installation also depends on a Node.js build that supports the host.

Pick the path that matches how you want to run WebCodex:

| Goal | Start here |
| --- | --- |
| Use an existing hosted Server | Install WebCodex, then run `webcodex connect <server>` in the repository. |
| Temporarily expose one local project | Run `webcodex share`; it starts a local Server + Runner and a Cloudflare Quick Tunnel. |
| Run your own long-lived Server | Deploy the server-only Docker/Compose stack, put a stable HTTPS domain or named tunnel in front of it, then enroll each repository machine as a Runner. |

### Hosted Server: connect one project

```bash
npm install -g @yyjeqhc/webcodex
cd /path/to/your/repository
webcodex connect https://sg4.yyjeqhc.cn
```

`connect` uses the current directory as the project, creates a local profile,
starts a detached Runner, and prints the MCP URL and generated key. Add those
values to ChatGPT or Claude, then ask for a real task, for example:

```text
Find the first failing test, fix the cause, and run the relevant test suite again.
```

The generated key is printed in full only when it is first created. Keep it and
the generated `agent.toml` out of Git. Closing the terminal does not stop the
Runner; after a machine reboot, rerun the same `connect` command or use the
profile shown by the first run:

```bash
webcodex agent start --profile <profile>
```

### Temporary local sharing

If you do not have a hosted Server and only need temporary access to the current
computer, use `webcodex share`. The default path uses a Cloudflare Quick Tunnel,
which does not require a Cloudflare account.

Install `cloudflared` first if needed. These commands follow Cloudflare's
[official installation instructions](https://developers.cloudflare.com/tunnel/downloads/):

```bash
# macOS
brew install cloudflared

# Debian / Ubuntu: one copy-paste command using Cloudflare's official APT repository
sudo mkdir -p --mode=0755 /usr/share/keyrings && curl -fsSL https://pkg.cloudflare.com/cloudflare-main.gpg | sudo tee /usr/share/keyrings/cloudflare-main.gpg >/dev/null && echo "deb [signed-by=/usr/share/keyrings/cloudflare-main.gpg] https://pkg.cloudflare.com/cloudflared any main" | sudo tee /etc/apt/sources.list.d/cloudflared.list >/dev/null && sudo apt-get update && sudo apt-get install -y cloudflared
```

Then share the current project:

```bash
cd /path/to/your/repository
webcodex share
```

`share` performs the idempotent project setup when needed, starts the local
Server and Agent, creates a separate temporary Project Connector credential,
and exposes `/mcp` through the Quick Tunnel. WebCodex does not silently install
system packages. The printed HTTPS URL and Bearer credential are valid only for
that share session; Ctrl-C stops the runtime and tunnel and removes the temporary
credential.

In ChatGPT Developer Mode, create a custom app with the printed `/mcp` URL. If
your workspace offers **Access token/API key** authentication, choose it, paste
the temporary Bearer credential, and run **Scan Tools**. ChatGPT UI labels and
availability can vary by workspace and rollout. Treat this credential as live
coding access: anyone who has it can modify this project and run commands allowed
by the share runtime while the session is active. Keep it private and stop
sharing when finished.

Use `webcodex share --tunnel none` for local-only debugging. Quick Tunnels are
for development and testing, not stable production deployment. If Quick Tunnel
startup fails and `~/.cloudflared/config.yaml` already exists, Cloudflare does
not currently support Quick Tunnels with that default config present; use a
separate Quick Tunnel environment or temporarily move the file out of the way.

See [AI Onboarding](docs/AI_ONBOARDING.md) for managed accounts, custom servers,
and other connection paths.

## What WebCodex can do

- Read, search, create, and edit files inside registered projects.
- Run builds, tests, package scripts, and bounded shell commands.
- Inspect Git status and diffs, and help prepare focused commits.
- Keep long-running jobs and coding context available across chat turns.
- Connect multiple projects or machines to the same online assistant.
- Serve both MCP clients and OpenAPI-based GPT Actions.

The exact tools available depend on the Server surface, Runner capabilities,
and permissions you configure. WebCodex supports direct project operations as
well as optional task and review workflows; it is not limited to a single
approval model.

## How it works

```text
ChatGPT / Claude / another MCP client
                 │ HTTPS: MCP or GPT Actions
                 ▼
          WebCodex Server
                 │ authenticated Runner transport
                 ▼
     Runner on your workstation or server
                 │
                 └── files · Git · commands · tests · local toolchains
```

The Server coordinates clients, credentials, and tool requests. The Runner does
the actual work on the machine that owns the repository. Repositories and local
toolchains stay on that Runner host; only the requested tool inputs and results
travel through the connection.

## Long-lived self-hosting

A durable personal deployment keeps the Server separate from the machines that own your
repositories:

1. Deploy the server-only Docker/Compose stack on an always-on Linux host.
2. Publish it through a stable HTTPS hostname. Nginx is supported; a named Cloudflare Tunnel
   or another durable reverse tunnel can also route the hostname to the loopback Server.
   Cloudflare Quick Tunnel (`trycloudflare.com`) is only for temporary development/testing.
3. Create a short-lived pairing code on the Server.
4. On each workstation/server that owns repositories, run `webcodex login`, then install its
   generated Runner profile as a user service.

This gives ChatGPT/MCP one stable `/mcp` endpoint while files, Git state, compilers, and other
local toolchains stay on the Runner machines.

## Self-host the Server with Docker

The repository includes a server-only Dockerfile and Compose deployment. It
runs `webcodex-server` plus the administrative `webcodex` CLI; it intentionally
does not contain the Runner, project repositories, or language toolchains.

```bash
git clone https://github.com/yyjeqhc/webcodex.git
cd webcodex
./deploy/docker/bootstrap.sh https://webcodex.example.com
docker compose ps
```

The default port binding is `127.0.0.1:8080`. Put an HTTPS reverse proxy in
front of it, then create a short-lived pairing code and enroll the machines
that hold your repositories.

The current Compose file builds the image from the checked-out source. A
registry image can be published separately without changing the server-only
architecture. See [Docker deployment](docs/DOCKER_DEPLOYMENT.md) for the full
setup and [Deployment](docs/DEPLOYMENT.md) for systemd, OAuth, and production
operations.

## Run the Runner as a user service

On a managed or self-hosted Server, run `webcodex login` as the ordinary user
who owns the repositories. The login output reports the generated Agent config.
Install it as a user service without `sudo`:

```bash
webcodex agent install --scope user \
  --config /path/reported/by/login/agent.toml
webcodex agent status --scope user \
  --config /path/reported/by/login/agent.toml
```

See [Build and Install](docs/BUILD_INSTALL.md#runner-service-scopes) for system
services and advanced overrides.

WebCodex CLI requests to a WebCodex Server follow reqwest's standard proxy environment by
default, including `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`, and `NO_PROXY` (plus the
corresponding forms reqwest supports). Use `--proxy http://HOST:PORT` to override that
selection for the current CLI invocation, or `--no-system-proxy` to ignore proxy environment
and connect directly. These flags affect CLI HTTP requests only. In particular,
`webcodex connect` does not persist or inject them into the Runner configuration.

If a Runner host needs an outbound HTTP proxy, make the proxy variables available to the
Runner process/service. WebSocket honors `HTTPS_PROXY` / `HTTP_PROXY`, `ALL_PROXY`, and
`NO_PROXY` (including lowercase forms); the supported proxy transport is HTTP `CONNECT`.
See [Agent transports](docs/AGENT_TRANSPORTS.md) for precedence, limitations, and fallback
behavior.

## Client access

- **ChatGPT over MCP:** create a Developer Mode custom app that points to the
  Server `/mcp` endpoint. For `webcodex share`, choose **Access token/API key**
  when that option is available and paste the temporary Bearer credential.
  Managed and self-hosted HTTPS Servers can use OAuth; the setup below shows
  that long-lived ChatGPT flow.
- **Other MCP clients:** use the Server `/mcp` endpoint with the credential
  produced by the selected setup flow. See [MCP](docs/MCP.md).
- **GPT Actions:** OpenAPI-based GPT Actions remain available as an alternative
  integration path. See [GPT Actions](docs/GPT_ACTIONS.md).
- **Browser console:** open `/console` for connection details, runtime status,
  and available review or operations controls.

### ChatGPT OAuth (Developer Mode)

For a managed or self-hosted WebCodex Server with public HTTPS and OAuth enabled:

1. In ChatGPT, open **Settings → Apps → Create**, set the Server URL to
   `https://your-domain.example/mcp`, choose **OAuth**, and select the
   user-defined/custom OAuth client option. Copy the callback URL shown by
   ChatGPT; it is specific to that app configuration.
2. Register that exact callback URL as a WebCodex OAuth client. Keep the returned
   `client_secret`: it is shown only once.

   ```bash
   curl -fsS -X POST https://your-domain.example/api/oauth/clients/create \
     -H "Authorization: Bearer $WEBCODEX_PAT" \
     -H "Content-Type: application/json" \
     -d '{
       "name":"ChatGPT MCP",
       "redirect_uris":["https://chatgpt.com/connector/oauth/<callback-id>"],
       "allowed_scopes":["runtime:read","project:read","project:write","job:run"]
     }'
   ```
3. Enter the returned client ID and secret in ChatGPT and choose
   `client_secret_post` for token-endpoint authentication. Select only the
   WebCodex permissions the app needs. For the ordinary coding setup that is
   `runtime:read`, `project:read`, `project:write`, and `job:run`. Keep
   `account:manage` disabled unless the app really needs account administration.
   Leave `offline_access` enabled when ChatGPT offers it: WebCodex advertises it
   as a protocol-level refresh-token scope, and it does **not** grant an extra
   WebCodex permission or belong in the client's `allowed_scopes` list.
4. Run **Scan Tools**. On the WebCodex authorization page, sign in with a normal
   WebCodex PAT, review the requested scopes, choose **Allow**, and wait for the
   tool scan to finish.

![ChatGPT OAuth client setup](docs/assets/chatgpt-oauth-create.webp)

After **Scan Tools** succeeds, ChatGPT should show the discovered WebCodex operations on the app page.

ChatGPT UI labels can change. If OAuth discovery metadata changes, recreate the
ChatGPT app so it fetches the new metadata. See [MCP](docs/MCP.md),
[Deployment](docs/DEPLOYMENT.md), and the
[OAuth2 smoke test](docs/OAUTH2_SMOKE_TEST.md) for the server-side details.

## Security boundary

WebCodex can modify files and execute commands, so treat a connected client as
a development assistant with real access to the configured machines.

- Register only project roots the assistant should use.
- Keep shared keys, user tokens, Agent tokens, and generated config files out of
  prompts, logs, and Git.
- Use version control and recoverable backups before enabling write or command
  capabilities.
- Prefer an ordinary OS user for the Runner; root execution requires explicit
  configuration.

Read [SECURITY.md](SECURITY.md) for the complete model.

## Documentation

- [Quick Start](docs/QUICK_START.md)
- [AI Onboarding](docs/AI_ONBOARDING.md)
- [Build and Install](docs/BUILD_INSTALL.md)
- [Docker deployment](docs/DOCKER_DEPLOYMENT.md)
- [MCP](docs/MCP.md)
- [GPT Actions](docs/GPT_ACTIONS.md)
- [Operations and deployment](docs/DEPLOYMENT.md)
- [Full documentation index](docs/INDEX.md)

## Build from source

```bash
cargo build --release --workspace --bins
export PATH="$PWD/target/release:$PATH"
```

## Disclaimer

WebCodex is provided for research and learning. It can read and modify files
and execute commands within configured project boundaries. Use it only on
systems and repositories you are prepared to recover. The author is not
responsible for filesystem damage, data loss, or other consequences arising
from use of the software.

## Acknowledgements

Thanks to the [LINUX DO](https://linux.do/) community for its welcoming space
for technical discussion and support for open-source sharing.

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
