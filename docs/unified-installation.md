# Unified installation

[简体中文](unified-installation.zh-CN.md)

This guide describes the unified installer workflow being developed on this branch. It is not a claim that a new release is available. Windows NSIS, macOS package, and Debian 12 / Ubuntu 22.04+ `.deb` builds are intended to include Desktop, CLI, Server, and Runner for x64 and arm64. Linux unified installers build and probe in native-architecture Ubuntu 22.04 containers, with GLIBC symbol requirements capped at 2.35 and runtime dependencies checked there. Real-machine installation, reboot persistence, GUI-session behavior, and upgrade have not been accepted across all three platforms. Track the concrete checks in [Deployment validation](unified-deployment-validation.md).

Get published files from [GitHub Releases](https://github.com/yyjeqhc/webcodex/releases). The tracked [`download/`](../download/README.md) directory contains only the static page source; its generated `manifest.json` is intentionally not committed. The [download-page workflow](https://github.com/yyjeqhc/webcodex/actions/workflows/download-page.yml) builds a manifest-based GitHub Actions artifact after a release, but does not host or deploy it. To preview a source revision before installer release, follow [Linux source preview](DESKTOP_DEVELOPMENT.md#linux-source-preview-against-an-existing-server).

## One computer

The following workflow applies when a validated installer for your platform is available; source previews are documented separately above.

1. Install the platform package and open WebCodex Desktop.
2. Choose **Create main node** and confirm setup. This creates independent Server and local Runner services even if you skip the optional initial Project. **Advanced** retains intentional Server-only setup; existing environments keep their saved role.
3. Confirm Server, Runner, and project status in Desktop. Desktop, CLI, and the web runtime use the same Server-authorized view. Open `SERVER_URL/runtime` in a browser and use an existing user credential to view authorized Runners, projects, and status.
4. Configure the existing ChatGPT MCP/Tunnel connection through the Server. ChatGPT remains connected to the central Server. A remote project path belongs to its Runner machine; it is not a local path on the Server computer.

Desktop closing does not stop persistent services. The GUI helper runs only in the same user's logged-in, unlocked session. Linux does not add a GUI helper backend.

## Several computers

Create the same complete main node as the single-computer flow. Other computers join it as additional Runners; the main node keeps its own local Runner and Projects. Install the appropriate OS/architecture package on each machine. Advanced Server-only and viewer-only capabilities remain available.

On the main node, use **Projects → Add device**, supply a Server URL reachable from the additional machine, and explicitly click **Create invitation**. Install WebCodex there and choose **Join main node**; enter that direct URL and the one-time code in the protected input. The initial Project is optional and the Runner stays enabled. The CLI alternative remains `webcodex environment invite`. Codes are secrets; never put them in command-line arguments or logs.

ChatGPT reaches the central Server through OpenAI Tunnel; additional Runners use the separate direct Server URL. A loopback or OpenAI URL is not a remote Runner endpoint. Check listener, firewall, DNS or private networking if the address is unreachable. The dialog does not change these settings. Invitation creation does not prove device connection; refresh the authorized devices and Projects view afterwards. See [Desktop device instructions](desktop-guide.md#add-another-device).

On the central Server machine, create an environment:

```text
webcodex environment configure --create --project PATH
```

For a projectless main node in the CLI, use `webcodex environment configure --create --runner`; `--no-project` without `--runner` intentionally keeps the advanced Server-only role. A remote machine can likewise use `webcodex environment configure --join https://server.example --runner --code-stdin`. Existing environments retain their saved role; use Desktop's explicit local-work option or the CLI's `add-project` transition to enable a viewer's first Runner. On each repository machine, join the Server:

```text
webcodex environment configure --join https://server.example --project PATH
```

Use `--no-project` to join as a viewer without configuring a local Server or Runner. Ordinary Desktop Create/Join enables local work; Advanced keeps explicit role alternatives. In the CLI, `--runner` enables a Runner without an initial project, while legacy `--project PATH` still enables a Runner and registers that explicit project. The selected flow requests the Server address, authentication, and any required system authorization. Joining as a viewer uses the user's personal access token, entered through secure hidden input or a protected `--token-file`; it does not use a pairing code. For example, import an existing user API credential from a protected file:

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

## Upgrade from v0.4.6

v0.4.6 already includes Environment. Keep its existing Environment directory,
service owner/scope, Runtime selection, private credentials and recovery records;
ordinary upgrades do not require deleting configuration or re-pairing Runners.
After moving to v0.5, refresh the MCP connection's tool schema and reopen old App
readers so they use the new canonical tools. See the
[compatibility policy](compatibility-policy.md) for the published-data boundaries,
older installation paths, and explicit reconciliation of a Desktop Tunnel profile
that was saved but never configured in Environment.

## Upgrade installations older than v0.4.6

v0.5 uses the published v0.4.6 Environment release as its minimum direct upgrade source. Pre-Environment Linux installations and the official Windows v0.4.3 package are no longer migrated directly by v0.5. Upgrade or migrate those installations to v0.4.6 first, verify that the Environment owns the intended Server/Runner identity and services, and then upgrade that Environment to v0.5.

This keeps one tested bridge for historical installation shapes instead of carrying their systemd handoff and v0.4.3 package classifier into every later release. Do not delete or recreate credentials when crossing the bridge; v0.4.6 is responsible for importing the older identity, while v0.5 preserves the resulting Environment data.

## Tunnel profiles

For a persistent local Server, create the recommended Server-owned profile directly with `webcodex environment configure-tunnel work --host embedded --credentials-file /secure/work.json`. The protected JSON contains only `tunnel_id` and `api_key`; hidden terminal input is also supported. This saves the exact profile and private local-MCP binding in `EnvironmentStore`, never installs or starts a standalone Tunnel service, and never hot-reloads or restarts a running Server. JSON output reports `server_restart_required` and `next_action`; configure several profiles first, then run one explicit `webcodex environment restart server` when requested. Desktop exposes the same choice as **Run with WebCodex Server (recommended)** and shows the same explicit restart action.

Use `--host standalone` when a separate per-profile service is intentionally required. Its existing `start|stop|restart tunnel --profile PROFILE` lifecycle remains unchanged. Changing the owner of an existing profile still requires the explicit `tunnel-host PROFILE --host ...` flow after a clean stop and standalone uninstall; configuration never auto-adopts a foreign or legacy service. Inspect or remove profiles with `tunnel-status PROFILE` and `remove-tunnel PROFILE`.

In persistent mode, `EnvironmentStore` is the canonical profile catalog for both CLI and Desktop. The historical Desktop `secrets/tunnel-config.json` remains only for legacy/non-persistent runtimes; if it coexists with a persistent Environment it is a fail-closed profile/Tunnel identity fence, not a second writable catalog. Its stale API-key bytes never override or block a revision-fenced EnvironmentStore key rotation. Keep credential files private and remove them after use.

## Services and credentials

The intended persistent service managers are systemd on Linux, LaunchDaemon on macOS, and Windows Service Control Manager (SCM) on Windows. Manage a component with `webcodex environment start server`, `webcodex environment stop runner`, or `webcodex environment restart tunnel` (replace the action and component as needed). A persistent service is independent of the Desktop window.

If a Windows Runner service loses its logon credential, use `webcodex environment repair-credential runner`. It requires the native Windows account credential through hidden console input; a Windows Hello PIN is not the account password. The service runs under the real user's SID. SCM stores the service logon password; WebCodex does not retain a duplicate password copy.

Desktop Diagnostics provides separate Start, Stop, and Restart controls only for local components owned by the saved environment. A Server-only environment has only Server controls; a viewer has neither. Connecting as a viewer to a Server on the same machine does not take over independently managed services. On Windows it also offers native Runner service credential repair. When Desktop opens an environment with persistent services, it reads their status and periodically refreshes it; it does not automatically restart a stopped service. To restore the saved Server user credential, use **Restore Server user credential** in Diagnostics and enter the replacement through its protected input, or run `webcodex environment repair-user-credential [--token-file PATH]`. The operation verifies the saved Server and username, then atomically saves the replacement credential. It does not pair a Runner or change service state.

ChatGPT uses the central Server's existing MCP/Tunnel integration. A Runner on another machine needs a separately reachable URL to that Server. The OpenAI Tunnel only carries ChatGPT-to-MCP traffic; its address is not a Runner enrollment endpoint. Setup does not open firewall ports or change the Server's listening address. Tailscale is an optional way to provide private reachability; it is not required by WebCodex.

On macOS, boot recovery requires the system and project volumes to be unlocked. FileVault unlocking remains an OS responsibility; WebCodex does not disable encryption or retain disk-unlock passwords. After unlocking, project services do not depend on an open Desktop window. GUI operations still require the owner’s active, unlocked session. See [Apple’s FileVault documentation](https://support.apple.com/guide/deployment/intro-to-filevault-dep82064ec40/web).

## Upgrade

Normal upgrade validates the candidate against the published HTTPS source manifest, including source SHA, version, and provenance hash. Those identities are compared as declared by the manifest; they need not share a CI job or timestamp. Development and CI builds must opt into `--development-build` and report `provenance_verified: false`; they are not official release verification and must not be presented as such.

For an existing Linux or macOS installation, the original user first runs Core `upgrade-prepare` against the candidate and their EnvironmentStore. An administrator authorizes that exact receipt with `installer-authorize`; the package hook checks it against the candidate and stable runtime target without opening root's default EnvironmentStore. During package completion, root's installed CLI uses the narrow owner-context broker to run `installer-finish`. It reports success only when Core commits the owner transaction; a failed finish retains the authorization receipt for recovery. Fresh installs use an isolated installer transaction store and do not start services. Windows uses the outer installer in the current user's context, prepares and verifies that user's store before invoking the inner Tauri installer, and then runs `upgrade-finish`.

On Windows, the manifest-bound candidate CLI classifies the installation separately from Environment ownership. Official x64 v0.4.3 is explicitly recognized by its old build identity and absence of Environment data format support; its CLI is never sent Environment commands. A complete unconfigured newer package has the same package-only transaction. Quit the old Desktop and stop its Server/Runner first: unavailable or busy programs, incomplete installs, foreign owners and unmatched Windows services block replacement. The candidate prepares private backups and a `windows-package-prepared.json` receipt bound to the original Windows SID, exact four targets, operation and published manifest. Verification checks the receipt and original bytes before the inner installer runs; finish checks all installed hashes and build-info launches. Rollback restores all four original programs from verified backups. This does not create an Environment, enroll a Runner, modify provider configuration, or migrate Server data. An existing Environment continues to use its installed CLI and existing owner transaction.

If package rollback fails, retain the private `windows-package-upgrade.json` journal and `upgrade-backups/` in the selected Environment directory. Stop the affected programs, then use a verified current CLI to run `environment package-upgrade-rollback --expected-runtime-dir <original-installation>/webcodex-runtime --environment-dir <original-environment-directory> --json`. Do not delete recovery files or retarget the transaction. The native Windows suite includes a v0.4.3-compatible PE fixture which rejects Environment commands with exit 2; real official-installer acceptance remains pending.

Reinstalling the exact same validated package is a read-only, idempotent verification path, including when no Environment is configured. A different package version without an Environment is still rejected on Unix and needs an owner recovery record; do not treat the same-package check as permission to replace changed files. For a configured Environment, a different candidate requires the owner-receipt flow and published source-manifest verification.

Rollback coverage follows exact package ownership. Linux restores its managed Desktop executable; macOS restores the complete `.app` bundle; Windows snapshots and restores only four exact managed executables: `WebCodex.exe` and the CLI, Server, and Runner under `webcodex-runtime/`. Operating-system menu entries, `.desktop` files, and uninstall metadata remain owned by the package manager and are outside the application rollback snapshot. Core also snapshots runtime executables and Environment data. Broken-installed-CLI recovery and the complete outer-installer rollback still require native validation. Legacy CLI service migration is a separate explicit Linux flow above, not automatic during install. These upgrade and migration paths have not passed native Windows NSIS, macOS package, or Debian package acceptance.

For platform-specific acceptance steps and items that still require native machines, see [Deployment validation](unified-deployment-validation.md). Advanced npm/runtime archive and Docker instructions remain in [Deployment](DEPLOYMENT.md).
