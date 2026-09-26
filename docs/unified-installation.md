# Unified installation

[简体中文](unified-installation.zh-CN.md)

This guide describes the unified installer workflow being developed on this branch. It is not a claim that a new release is available. Windows NSIS, macOS package, and Debian 12 / Ubuntu 22.04+ `.deb` builds are intended to include Desktop, CLI, Server, and Runner for x64 and arm64. Linux unified installers build and probe in native-architecture Ubuntu 22.04 containers, with GLIBC symbol requirements capped at 2.35 and runtime dependencies checked there. Real-machine installation, reboot persistence, GUI-session behavior, and upgrade have not been accepted across all three platforms. Track the concrete checks in [Deployment validation](unified-deployment-validation.md).

Get published files from [GitHub Releases](https://github.com/yyjeqhc/webcodex/releases). The tracked [`download/`](../download/README.md) directory contains only the static page source; its generated `manifest.json` is intentionally not committed. The [download-page workflow](https://github.com/yyjeqhc/webcodex/actions/workflows/download-page.yml) builds a manifest-based GitHub Actions artifact after a release, but does not host or deploy it. To preview a source revision before installer release, follow [Linux source preview](DESKTOP_DEVELOPMENT.md#linux-source-preview-against-an-existing-server).

## One computer

The following workflow applies when a validated installer for your platform is available; source previews are documented separately above.

1. Install the platform package and open WebCodex Desktop.
2. Choose **Create** and select the project directory, or choose no project yet.
3. Confirm Server, Runner, and project status in Desktop. Desktop, CLI, and the web runtime use the same Server-authorized view. Open `SERVER_URL/runtime` in a browser and use an existing user credential to view authorized Runners, projects, and status.
4. Configure the existing ChatGPT MCP/Tunnel connection through the Server. ChatGPT remains connected to the central Server. A remote project path belongs to its Runner machine; it is not a local path on the Server computer.

Desktop closing does not stop persistent services. The GUI helper runs only in the same user's logged-in, unlocked session. Linux does not add a GUI helper backend.

## Several computers

Install the same package on each computer. Decide which machine hosts the central Server, and which machines own the repositories. A Server-only central machine can show projects on remote machines B and C when their Runners connect.

On the central Server, create short-lived Runner invitation codes with `webcodex environment invite`. Displayed codes are secrets: deliver one only to the intended machine, and do not place it in command-line arguments or logs.

On the central Server machine, create an environment:

```text
webcodex environment configure --create --project PATH
```

Use `--no-project` if that machine has no repository. On each repository machine, join the Server:

```text
webcodex environment configure --join https://server.example --project PATH
```

Use `--no-project` to join as a viewer without configuring a local Server or Runner. The two setup choices are create/join and project/skip; the selected flow then requests the Server address, authentication, and any required system authorization. Joining as a viewer uses the user's personal access token, entered through secure hidden input or a protected `--token-file`; it does not use a pairing code. For example, import an existing user API credential from a protected file:

```text
webcodex environment configure --join https://server.example --no-project --token-file PATH
```

Viewer setup does not redeem Runner pairing codes or create a Runner token, `client_id`, or `runner.toml`. Desktop reports **Local Runner · Not configured**, while its project list can show authorized projects on other Runners.

A Runner join uses a one-time pairing code supplied once through stdin:

```text
webcodex environment configure --join https://server.example --project PATH --code-stdin
```

The pairing code is read from stdin and must not be placed in command-line arguments. The Server operator creates short-lived codes on the central Server. Keep user tokens and server bootstrap credentials on their intended machines.

Add another repository on an already-configured Runner machine with `webcodex environment add-project PATH`. It reuses that machine's identity. On a viewer machine, the first project addition prompts to convert the machine to a Runner and requires a one-time pairing code.

Inspect setup state with `webcodex environment status --json` or `webcodex environment doctor --json`. Resume interrupted setup with `webcodex environment resume`; `--new-pairing-code` requests a replacement only when recovery cannot determine whether the prior one-time code was consumed. Do not routinely generate a new code.

Passing `--token-file` to ordinary `resume` supplies the credential needed to continue setup; it does not rotate a saved user credential. To replace a lost or invalid saved Server user credential, use `webcodex environment repair-user-credential [--token-file PATH]`. Without `--token-file`, the CLI requests it through hidden terminal input.

## Migrate an existing Linux CLI service

Linux provides explicit migration commands for the supported legacy CLI templates. They preserve the existing identity and do not issue a new pairing code. Source review is complete, but these commands have not passed native package/service acceptance. Do not treat M2f migration as natively accepted for these paths.

For a legacy user-owned Runner, run as its original owner and provide that owner's private user credential file:

```bash
webcodex environment migrate-legacy-runner \
  --join https://server.example \
  --project /home/alice/src/repo \
  --token-file /home/alice/.config/webcodex/webcodex-user-token
```

Add `--profile NAME` only for a named profile. The migration accepts a known owner-bearing Runner configuration and its generated user unit, reuses its Runner identity, project IDs, and user credential, and does not re-pair. It will not infer ownerless shared-key configurations or custom units.

For the fixed root-owned system Server unit/socket pair, run the explicit migration with the original Server username, that user's private API credential file, and the exact old listen address:

```bash
webcodex environment migrate-legacy-server \
  --user alice \
  --token-file /home/alice/.config/webcodex/webcodex-user-token \
  --listen 0.0.0.0:8080 \
  --server-url http://127.0.0.1:8080
```

`--server-url` is optional; when omitted, the CLI derives a local URL from the original listener. This command targets only the known systemd unit/socket and fixed environment/data locations. It transfers the Server only. An independently configured legacy Tunnel remains with its existing owner/profile and is not imported into Core by this migration. The command does not guess custom units, paths, or owners. Keep the credential file private and do not use the Server bootstrap token in place of the original user's API credential.

## Tunnel profiles

Configure a named Tunnel profile with `webcodex environment configure-tunnel PROFILE --credentials-file PATH`. The protected JSON file contains `tunnel_id` and `api_key`; alternatively, the CLI accepts the credentials through hidden terminal input. Inspect or remove a profile with `webcodex environment tunnel-status PROFILE` and `webcodex environment remove-tunnel PROFILE`. Control one profile with `webcodex environment start tunnel --profile PROFILE`, `stop tunnel --profile PROFILE`, or `restart tunnel --profile PROFILE`. Keep the credential file private and remove it after use.

## Services and credentials

The intended persistent service managers are systemd on Linux, LaunchDaemon on macOS, and Windows Service Control Manager (SCM) on Windows. Manage a component with `webcodex environment start server`, `webcodex environment stop runner`, or `webcodex environment restart tunnel` (replace the action and component as needed). A persistent service is independent of the Desktop window.

If a Windows Runner service loses its logon credential, use `webcodex environment repair-credential runner`. It requires the native Windows account credential through hidden console input; a Windows Hello PIN is not the account password. The service runs under the real user's SID. SCM stores the service logon password; WebCodex does not retain a duplicate password copy.

Desktop Diagnostics provides separate Start, Stop, and Restart controls only for local components owned by the saved environment. A Server-only environment has only Server controls; a viewer has neither. Connecting as a viewer to a Server on the same machine does not take over independently managed services. On Windows it also offers native Runner service credential repair. When Desktop opens an environment with persistent services, it reads their status and periodically refreshes it; it does not automatically restart a stopped service. To restore the saved Server user credential, use **Restore Server user credential** in Diagnostics and enter the replacement through its protected input, or run `webcodex environment repair-user-credential [--token-file PATH]`. The operation verifies the saved Server and username, then atomically saves the replacement credential. It does not pair a Runner or change service state.

ChatGPT uses the central Server's existing MCP/Tunnel integration. A Runner on another machine needs a separately reachable URL to that Server. The OpenAI Tunnel only carries ChatGPT-to-MCP traffic; its address is not a Runner enrollment endpoint. Setup does not open firewall ports or change the Server's listening address. Tailscale is an optional way to provide private reachability; it is not required by WebCodex.

On macOS, boot recovery requires the system and project volumes to be unlocked. FileVault unlocking remains an OS responsibility; WebCodex does not disable encryption or retain disk-unlock passwords. After unlocking, project services do not depend on an open Desktop window. GUI operations still require the owner’s active, unlocked session. See [Apple’s FileVault documentation](https://support.apple.com/guide/deployment/intro-to-filevault-dep82064ec40/web).

## Upgrade

Normal upgrade validates the candidate against the published HTTPS source manifest, including source SHA, version, and provenance hash. Those identities are compared as declared by the manifest; they need not share a CI job or timestamp. Development and CI builds must opt into `--development-build` and report `provenance_verified: false`; they are not official release verification and must not be presented as such.

For an existing Linux or macOS installation, the original user first runs Core `upgrade-prepare` against the candidate and their EnvironmentStore. An administrator authorizes that exact receipt with `installer-authorize`; the package hook checks it against the candidate and stable runtime target without opening root's default EnvironmentStore. During package completion, root's installed CLI uses the narrow owner-context broker to run `installer-finish`. It reports success only when Core commits the owner transaction; a failed finish retains the authorization receipt for recovery. Fresh installs use an isolated installer transaction store and do not start services. Windows uses the outer installer in the current user's context, prepares and verifies that user's store before invoking the inner Tauri installer, and then runs `upgrade-finish`.

Reinstalling the exact same validated package is a read-only, idempotent verification path, including when no Environment is configured. A different package version without an Environment is still rejected on Unix and needs an owner recovery record; do not treat the same-package check as permission to replace changed files. For a configured Environment, a different candidate requires the owner-receipt flow and published source-manifest verification.

Rollback coverage follows exact package ownership. Linux restores its managed Desktop executable; macOS restores the complete `.app` bundle; Windows snapshots and restores only four exact managed executables: `WebCodex.exe` and the CLI, Server, and Runner under `webcodex-runtime/`. Operating-system menu entries, `.desktop` files, and uninstall metadata remain owned by the package manager and are outside the application rollback snapshot. Core also snapshots runtime executables and Environment data. Broken-installed-CLI recovery and the complete outer-installer rollback still require native validation. Legacy CLI service migration is a separate explicit Linux flow above, not automatic during install. These upgrade and migration paths have not passed native Windows NSIS, macOS package, or Debian package acceptance.

For platform-specific acceptance steps and items that still require native machines, see [Deployment validation](unified-deployment-validation.md). Advanced npm/runtime archive and Docker instructions remain in [Deployment](DEPLOYMENT.md).
