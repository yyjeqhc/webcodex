# Runtime installation and CLI join

[简体中文](runtime-installation.zh-CN.md)

The Linux **Runtime** package contains the CLI, Server and Runner. **Full** adds Desktop. Package contents and the machine's role are separate: installing Runtime does not create an environment or start a Server, and joining a main node starts only the local Runner. Runtime packages and this update path are under development; the source and focused tests described here do not establish native package installation, reboot or rollback acceptance. Published platform/architecture assets must be available before using the package workflow. See [Unified installation](unified-installation.md) and [Deployment validation](unified-deployment-validation.md).

## Join an existing main node

Install the official Runtime package for the machine's Linux architecture and package manager. Run the CLI as the original project-owning user, including over SSH; do not prefix setup with `sudo`. The system-service option requests its separate OS authorization while preserving that user. The central node owner supplies a direct Server URL reachable from this machine and creates a short-lived invitation with Desktop's **Add device** or `webcodex environment invite`. Keep the central Server's bootstrap credentials and Tunnel/API keys on their intended machine.

In a terminal, join with a projectless Runner:

```text
webcodex environment configure --join https://server.example --runner --no-project --scope user
```

Enter the one-time code at the hidden prompt. Do not put it in command-line arguments, shell history, logs or a command substitution. For a protected non-interactive input stream, add `--code-stdin`; the CLI consumes the code once through stdin. A protected secret source can supply that stream without including the value in argv. `--no-project` does not disable the Runner when `--runner` is present. Without `--runner`, `--no-project` selects the existing viewer role instead.

Optionally add `--runner-name "SSH worker"` during this explicit setup. It uses the existing Runner display label (at most 200 characters, no NUL); it does not change the Runner or project identity. A saved setup/resume retains its original name.

User services follow the existing platform manager contract. Linux logout survival depends on the user's systemd manager and linger configuration; setup does not silently fall back to system services. Choose `--scope system` explicitly when a boot service is intended and obtain its OS authorization. Run subsequent commands as the same original user; saved environments retain their service scope and identity.

To register a folder during Join, replace `--no-project` with `--project /home/alice/src/repo`. To add one later:

```text
webcodex environment add-project /home/alice/src/repo
webcodex environment status --json
webcodex environment doctor --json
```

The same Runner identity is reused. Join does not create, install or start the central Server; the presence of `webcodex-server` in Runtime is not permission to do so. The direct URL is a separate path from ChatGPT's Tunnel/MCP connection: use the main node's Server address, not an OpenAI or loopback address. The CLI does not change listener, firewall or DNS settings. The central owner manages ChatGPT's connection.

For interrupted setup, use `webcodex environment resume` as the same user. Supply `--code-stdin` when a code is required. Use `--new-pairing-code` only for the explicit replacement-code recovery path; do not reconfigure another environment, change the saved Server address, or assume a consumed one-time code can be replayed. There is no new Runtime-specific credential or identity store.

## Inspect and update the installed package

Use the CLI installed by the package, as the same original user who configured its Environment:

```text
webcodex environment update status --json
webcodex environment update check --json
webcodex environment update download --version VERSION --json
```

Replace `VERSION` with a published stable version. `status` reads local state; `check` queries release metadata; `download` verifies published hashes/provenance and saves an installer candidate. These commands do not replace programs or restart services, and do not require an interactive terminal. A downloaded candidate is not an installed update.

The bounded update JSON preserves the existing schema and includes the canonical `view.installed_target` only when the installed package is verified and eligible. Runtime has `flavor: "runtime"`; Full keeps the legacy target shape with the flavor field omitted. An absent installed target means unverified, not Runtime inferred from Desktop's absence. Verified Runtime reports CLI, Server and Runner component rows; Full also reports Desktop. Candidate identity and component rows describe the downloaded package independently. Local Server/Runner roles describe the saved environment, not which programs a package happens to contain or whether a remote machine is online.

Managed updates preserve the installed flavor, architecture and package format. Runtime downloads select the verified Runtime target; Full selects Full. A mismatched candidate is rejected before task probes, sudo validation or service preparation. Neither a manual Full download nor a missing Desktop authorizes converting an installation to Runtime. npm, source, bare Runner, custom and unsupported installations use the [manual deployment/release path](DEPLOYMENT.md), with their original configuration and ownership preserved. Use `status` to see limitations; release availability does not make a manual installation eligible for replacement.

On Linux, an eligible installed package can be applied from a real terminal, for example SSH with an allocated TTY:

```text
webcodex environment update apply --version VERSION --yes
```

The original user stays unprivileged; the existing trusted installer helper obtains system sudo authorization. `--yes` confirms this exact application but does not remove the TTY or OS authorization requirement. Non-interactive `apply`, `resume` and `rollback` fail before opening update stores or preparing services. macOS and Windows application is not supported by this headless entry point. Runtime currently targets Linux; use the established native or manual path on other platforms.

Running work, unavailable task observations, missing package tools, an unsupported installation, an unverified candidate or an unfinished operation can block application. A helper acknowledgement is not installation success. Inspect the authoritative result:

```text
webcodex environment update status --json
webcodex environment update resume --operation-id OPERATION_ID --yes
webcodex environment update rollback --operation-id OPERATION_ID --yes
```

Use the exact saved UUID reported in `view.upgrade.operation_id` for recovery, from a Linux TTY and the original user's environment. These commands delegate to existing Environment finish/rollback guards; they do not launch a second installer or infer that uncertain replacement is safe to retry. Keep journals and private backups when recovery is required. An update affects only this machine's installed package and owned environment; it does not upgrade a remote central Server or other Runners through the connection. Native installation and recovery acceptance still need the platform-specific checks linked above.
