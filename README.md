# WebPi

**WebPi is a local-first coding runtime for web AI agents.** It lets ChatGPT / Custom GPT work on real repositories, Git, tests, processes, browser/computer observations, artifacts, and the native Pi extension ecosystem while keeping execution on the machine that owns the project.

WebPi is an independent product and deployment. The supported native programs are `webpi`, `webpi-server`, and `webpi-runner`; on the validated Windows standalone installation, use `webpi.cmd`.

> Internal compatibility names such as some `webcodex-*` Rust crates, wire/resource identifiers, database filenames, credential prefixes, and the published `@yyjeqhc/webcodex-plugin-sdk` may remain intentionally. User-facing commands, configuration, runtime messages, and product documentation use WebPi.

## Why WebPi

- **Local-first development** — repositories and toolchains stay on the machine where they already live.
- **Guarded mutations** — project roots, stale-write fences, exact file edits, scope checks, and reviewable Git changes are first-class.
- **Real development tools** — run tests, compilers, formatters, language services, durable Jobs, and project-specific commands.
- **Durable work** — Workflow Sessions, Jobs, validation evidence, and resumable observations survive beyond one model turn.
- **Read-only desktop/browser observation by default** — optional Computer/Browser scopes can inspect without granting pointer, keyboard, clipboard, or launch authority.
- **High-resolution artifact delivery** — screenshots and other bounded artifacts can be saved as project artifacts and exported through short-lived one-shot HTTPS capabilities instead of oversized base64 Action responses.
- **Pi-native extensions without a second model loop** — WebPi hosts the Pi resource/package/extension ecosystem through the first-party `pi-bridge` while WebPi remains the execution authority.

## Quick start — validated Windows standalone

From the WebPi checkout:

```powershell
.\webpi.cmd status
.\webpi.cmd doctor
.\webpi.cmd run
```

`doctor` is the first troubleshooting command. A healthy clean deployment reports the Server and Runner online, Pi Bridge ready when configured, and source alignment as `aligned`.

For first-time local configuration, use:

```powershell
.\webpi.cmd init
.\webpi.cmd enroll
.\webpi.cmd doctor
```

Private runtime state stays under `.webpi-state/`; portable runtime assets stay under `.webpi-runtime/`. Do not put PATs, bootstrap credentials, tunnel credentials, or private keys in prompts, logs, or Git.

### Public ChatGPT Actions

When an HTTPS origin already routes to the local WebPi Server, register it explicitly:

```powershell
.\webpi.cmd cloudflare-config https://webpi.example.com
.\webpi.cmd doctor
.\webpi.cmd verify --base-url https://webpi.example.com --expect-public-origin https://webpi.example.com --with-action-token
```

Then import:

```text
https://webpi.example.com/openapi.json
```

Configure the WebPi Action PAT as Bearer authentication in the client. Never paste the PAT into chat. The public origin is stored as `WEBPI_PUBLIC_URL`; public reachability does not remove bearer/scope checks from protected execution routes.

## Recommended agent workflow

1. **Establish exact authority** — resolve the Project and Workflow Session with `work_on_project`; never guess ids.
2. **Observe before mutating** — inspect runtime, Git/workspace state, relevant files, and tool/extension schemas first.
3. **Use the narrowest structured tool** — prefer exact file/Git/process/validation tools over shell; prefer direct Actions over generic gateways.
4. **Change one bounded thing** — preserve unrelated work and treat unknown outcomes as “inspect before retry”.
5. **Prove the result** — run the smallest targeted test, then the relevant broader gate; do not equate HTTP 200, process start, or a successful retry with correctness.
6. **Review the diff** — inspect changed paths and evidence before declaring completion.
7. **Report clearly** — what changed, what was validated, remaining risks, and whether the user must do anything.

See [WebPi agent instructions](docs/WEBPI_GPT_INSTRUCTIONS.md) and [WebPi prompt recipes](docs/WEBPI_PROMPTS.md).

## Core capabilities

| Area | WebPi capability |
| --- | --- |
| Project files | bounded read/search, exact guarded edits, sensitive-path policy |
| Git | status, diff/review, references, commits when explicitly authorized |
| Processes | structured commands, shell only when needed, durable Jobs and terminal attention |
| Code intelligence | symbols, definitions, references, diagnostics, validation evidence |
| Sessions | Workflow Session provenance, messages, goals/tasks, resumable handoff |
| Computer | display/window discovery, read-only snapshots, artifact-backed high-resolution screenshots |
| Browser | target/DOM observation with separate launch/control authority |
| Artifacts | metadata/inspect plus bounded project artifacts and one-shot HTTPS downloads |
| Plugins | inspect/describe/invoke with scope separation; management is a distinct authority |
| Pi ecosystem | resources, skills, prompts, packages, exact extension candidate fingerprints, approval/reload/revoke |

## Safe defaults

WebPi deliberately separates observation from control and discovery from execution.

- Computer display read does **not** grant pointer/keyboard/launch or clipboard access.
- Browser read does **not** grant browser control or launch.
- Plugin inspect/invoke does **not** grant plugin management.
- Pi extension discovery does not import unapproved executable code.
- Package install/update/remove is consequential and may execute lifecycle scripts; explicit confirmation/trust is required.
- An `outcome_unknown` mutation is never blindly retried: inspect the real state first.
- Destructive cleanup, credential rotation, release/push, trust elevation, and broad overwrites remain explicit user decisions.

See [credential and authority boundaries](docs/WEBPI_AUTH_BOUNDARIES.md) and [Pi parity](docs/WEBPI_PI_PARITY.md).

## Pi and Native Tool Plugins

WebPi's first-party `pi-bridge` exposes Pi resources without starting a second reasoning model. The normal extension flow is:

```text
discover candidate
→ inspect source/dependencies/permissions
→ verify exact candidate id + SHA-256
→ approve that fingerprint
→ reload resources
→ list/describe the exact tool
→ invoke
→ validate
```

Changing extension content changes its fingerprint and returns it to pending approval.

For authoring a Native Tool Plugin:

```text
webpi plugin init ...
webpi plugin check ...
webpi plugin reload ...
webpi plugin list ...
webpi plugin describe ...
```

The published SDK package currently retains its compatibility name: `@yyjeqhc/webcodex-plugin-sdk`.

## Documentation

- [WebPi operating guide](docs/WEBPI.md)
- [Identity, configuration isolation, and migration](docs/WEBPI_IDENTITY.md)
- [Authentication and credential boundaries](docs/WEBPI_AUTH_BOUNDARIES.md)
- [WebPi ↔ Pi capability contract](docs/WEBPI_PI_PARITY.md)
- [WebPi + Pi ecosystem strategy and package watchlist](docs/WEBPI_PI_ECOSYSTEM.md)
- [WebPi agent/system instructions](docs/WEBPI_GPT_INSTRUCTIONS.md)
- [Prompt recipes for research, features, bugs, review, and release](docs/WEBPI_PROMPTS.md)
- [Upstream/compatibility policy](docs/WEBPI_UPSTREAM.md)

Historical audit and cutover records under `docs/` are evidence for specific past deployments; they are not the current installation instructions.

## Development

Run focused tests while editing, then the relevant broader gates. Typical Rust checks are:

```bash
cargo fmt --all -- --check
cargo test -p webcodex --lib
cargo test -p webcodex-tool-contracts --lib
cargo test -p webcodex-cli
```

For the Windows standalone layer:

```powershell
C:\Python314\python.exe -m unittest discover -s scripts\webpi\tests -p "test_*.py" -v
.\webpi.cmd doctor
```

Production builds should come from a clean Git revision so Server, Runner, and source-alignment evidence agree.

## Upstream and compatibility

WebPi reuses hardened implementation assets from WebCodex but has its own product identity, runtime state, Server/Runner deployment, public schema, and Pi integration. Do not mechanically rename internal compatibility identifiers; see [WEBPI_UPSTREAM.md](docs/WEBPI_UPSTREAM.md) and [WEBPI_IDENTITY.md](docs/WEBPI_IDENTITY.md) for the boundary.

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
