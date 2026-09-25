# Desktop Shell and Runtime compatibility

WebCodex compatibility is protocol- and capability-based. Build revisions are diagnostic identity. Normal compatible WebCodex builds may differ in revision; modified builds remain the operator's responsibility.

## Desktop and Runtime are separate

Desktop is the persistent native shell: project selection, saved credentials, connections, settings, diagnostics, and ownership of the processes it launches. A Runtime folder supplies three native executables: `webcodex`, `webcodex-server`, and `webcodex-runner` (`.exe` on Windows). A release Desktop initially selects its bundled Runtime, but also supports an explicitly selected external Runtime folder without editing the application bundle or installation directory.

Desktop, CLI, Server, and Runner do **not** need the same Git commit or package version. The Server and Runner do not need the same commit either. Version, commit, target, architecture and dirty state remain visible for diagnosis; they are not execution permission or compatibility evidence by themselves.

## Compatibility policy

1. Git commit equality is not compatibility.
2. Package version equality is not compatibility.
3. Desktop management contracts use an explicit supported generation range; Server/Runner use the existing Runner protocol generation and negotiated capabilities.
4. Additive fields and optional features remain backward-compatible. Readers ignore unknown fields in a known schema.
5. A breaking management or wire contract change increments its respective generation. It is not hidden behind a new package version.
6. A missing feature capability disables that operation only. For example, Project Lifecycle still requires `RunnerFeature::ProjectLifecycle`; it must not disable otherwise-supported read, shell or Git operations.
7. Custom source modifications are the operator's responsibility. Inspecting declared metadata is not code auditing, signature trust evaluation, or a filesystem sandbox.

The #579 Runner observation migration keeps the management contract at `[1, 1]`.
It changes raw `runtime_status` / `list_runners` projections and their first-party
consumers, while preserving the management command fields Desktop deserializes.
It does not require raw observation compatibility with older `/agents` clients.
See [migration scope](agent/runner-observability.md#upgrade-status).

### Machine-readable build information

All three binaries support the side-effect-free standalone flag:

```sh
webcodex --build-info-json
webcodex-server --build-info-json
webcodex-runner --build-info-json
```

This exit path runs before loading Runtime configuration or starting services. Schema version 1 reports `binary`, `version`, `git_commit`, `git_dirty`, `built_at`, `target`, `architecture`, `desktop_runtime_contract`, and, for Server/Runner, `agent_protocol_generation`. Optional unknown fields may be added without changing the schema or breaking generation. Malformed or unsupported required metadata is rejected rather than treated as a compatible legacy binary.

The shared `webcodex-core::desktop_runtime_contract::DESKTOP_RUNTIME_CONTRACT` currently advertises `[1, 1]`. Desktop intersects its range with each selected binary's range and requires a nonempty common range for the three-binary management set. `[1,1]` and `[1,2]` overlap; `[1,1]` and `[2,2]` do not. The independent Runner wire generation is not replaced by this management contract.

A Server reachable at the configured address must also report an overlapping management contract. CLI `server status` exposes the actual responding Server's contract/build, not merely its own local build. Source revision warnings are advisory, not `binary_version_mismatch` failures. Unsupported operation contracts, auth/scope failures, stale identity fences and uncertain side-effect outcomes remain fail-closed.

`runtime_status`, Admin, and Runtime Console distinguish `protocol_compatibility` (`compatible`, `incompatible`, `unknown`) from `build_alignment` (`exact`, `different_version`, `different_commit`, `dirty`, `unknown`). Existing `source_alignment`, `version_matches_server` and `git_commit_matches_server` are retained as diagnostics. A connected Runner missing an optional capability is not made globally incompatible.

## Select a Runtime folder

Open **Settings → Runtime**. The current source and the individual CLI/Server/Runner build identities are shown there. Choose **Select Runtime folder…**, then select an extracted official release archive or your own native build output directory.

Candidate inspection executes the selected binaries' bounded `--build-info-json` commands with a minimal environment. Choose only executables you intend to run. Probing verifies required files, executable status, declared native target/architecture, structured metadata, supported contract ranges and unchanged file hashes; it does not establish that modified code implements its declarations honestly.

The candidate preview does not change active processes or the persisted selection. Different versions, different revisions, and dirty builds are accepted with advisories when the contracts overlap. Missing/non-executable files, wrong observable architecture, unsupported/malformed metadata or disjoint contracts block activation.

Choose **Use this Runtime** only after reviewing the preview. **Use bundled Runtime** stages the bundled candidate for the same explicit confirmation flow. **Recheck Runtime** observes the currently selected files; it does not silently restart services or approve changed bytes.

### Switching, Jobs, and rollback

The switch rechecks the exact candidate, current connection/project identity, selection revision and executable hashes. It warns about running Jobs; an unknown count also requires explicit interruption confirmation. Only Desktop-owned relevant processes may be stopped. A user-managed Server/Runner is never killed or adopted merely because it listens at the expected address.

Desktop stops its owned Runner and, for a local setup, its owned Server, starts the selected binaries with the existing identity, waits for bounded readiness, verifies responding/executed identities, and only then commits the new source. It does not re-pair, generate new credentials, change ports, reactivate/register projects, or rewrite connection/Tunnel configuration as a shortcut.

Failed activation attempts one bounded restoration of the previous known-good source and verified files. The result distinguishes **previous Runtime restored** from **recovery required**. A failed/unconfirmed stop is not permission to spawn a replacement. If previous files were removed or changed, Desktop reports that recovery cannot be confirmed; it does not fall back secretly to bundled files.

The Custom selection survives Quit/reopen. If its folder later disappears or becomes invalid, Desktop presents **Selected Runtime is unavailable**, with explicit bundled/other-folder/diagnostic recovery actions. Keep previously selected Runtime files available until a successful upgrade is confirmed.

## Build your own Runtime

From a normal WebCodex checkout, the Cargo packages and binary targets are:

| Package | Native executable |
| --- | --- |
| `webcodex-cli` | `webcodex` |
| `webcodex` | `webcodex-server` |
| `webcodex-runner` | `webcodex-runner` |

Build all three from the workspace root:

```sh
cargo build --locked --release -p webcodex-cli -p webcodex -p webcodex-runner --bins
```

The output is `target/release/webcodex`, `target/release/webcodex-server`, and `target/release/webcodex-runner`, with `.exe` suffixes on Windows. On the native host, select the whole `target/release` directory. An explicit `--target` build instead writes under `target/<target-triple>/release`; select that directory and use the host's supported native architecture.

For development dogfood using the existing optimized, non-LTO workspace profile:

```sh
cargo build --locked --profile dogfood -p webcodex-cli -p webcodex -p webcodex-runner --bins
```

Select `target/dogfood`. No special source revision is required by Desktop. Runtime-specific experimental features remain explicit Cargo features and still require their operation-level capabilities; a missing optional feature is not repaired by pretending another operation succeeded.

On macOS, OS code signing, Gatekeeper and Computer Use permissions are separate from protocol compatibility. Rebuilding a binary may change its OS identity; use the repository's documented local signing flow where appropriate. Never overwrite an independent control Runner while dogfooding the candidate it is meant to recover.

## Diagnostics and tracing

Open **Settings → Troubleshooting** for the Diagnostics Center. Metadata tracing records request lifecycle/correlation information without full tool argument/result bodies. Full tracing can contain sensitive inputs/results and requires explicit confirmation; enable it only temporarily.

Saving modifies only `WEBCODEX_TOOL_REQUEST_TRACE` in the managed Server environment. Other entries, including unknown settings and secrets, remain intact, duplicate target keys are removed, and an exact file-revision check prevents writing over a changed file. The entire environment is never sent to the WebView.

**Save & Restart Runtime** restarts the Desktop-owned local Server and waits for the existing Runner to reconnect. It does not broad-kill a healthy Runner or user-managed service. Restart interruption is confirmed explicitly. If the effective mode differs because another setting overrides it or readiness is unconfirmed, Diagnostics keeps **Restart required** and a precise safe reason rather than claiming success.

**Open Runtime Console** opens the current local Server's `/runtime` URL without a credential in the query or fragment. **Copy Runtime Console credential** is a separate sensitive action: after confirmation, native code copies the current managed user credential to the clipboard. It never returns it to the WebView, Activity or reports and rejects Runner `wc_agent_*` credentials. Clipboard managers may retain sensitive clipboard data; do not share it.

**Copy Diagnostic Report** and **Export Support Bundle** use fixed allowlisted projections. They include build/contract identities, readiness, tracing mode, safe event kinds and observed capabilities. They exclude credentials, environment/Runner-config contents, project files, arbitrary process output, clipboard contents and full request/result payloads. The ZIP contains only `diagnostic-report.json`, `diagnostic-report.md` and `activity-safe.json`, never a recursive copy of app data. Export requires a fresh `.zip` filename and does not overwrite existing files.

### Interpreting continuation evidence

Activity's **ChatGPT calls** tab shows actual observed work/calls; **Workflow Sessions** shows durable work progress/Jobs/validation; **System events** shows Desktop-owned service events. IDs are correlation details, not the primary user-facing description.

A completed call without `response_handed_at_ms` is **Execution completed; response handoff not confirmed**. A confirmed handoff means the response was handed to the HTTP framework, not proof that the MCP client received it or that a model processed it. Streaming start is shown separately from completed handoff.

The canonical `next_call_gap_ms` is attached to the arriving request and measures its gap from a previous non-streaming response. It is **not** evidence that another meaningful call followed the current event. A following meaningful call is only claimed when later canonical activity proves it. The elapsed gap can include networking, Host scheduling, inference, user input, or other time WebCodex cannot observe. The UI does not diagnose “ChatGPT stuck” or model failure from silence.

## Update notifications

After the UI is ready, Desktop performs a bounded public GitHub stable-release check without authentication. Startup/network/rate-limit failures are silent and do not change readiness. The existing Desktop state caches attempts for 24 hours; a manual check can retry immediately and reports an ordinary local message on failure.

A valid official `webcodex-release-manifest.json` provides the release/runtime versions and management contract range. Overlap means the current Desktop can continue with the newer Runtime; a disjoint range requires updating Desktop. Missing, malformed, mismatched or unknown manifest data yields only a generic new-release link. Drafts/prereleases are not offered as the stable update. **Nothing is downloaded or installed automatically.** Remind later snoozes the banner for a day.

## Configuration recovery

The existing atomic Desktop-state write and previous-known-good backup are reused; no parallel state store is introduced. Older known configuration fields migrate additively. An unsupported future schema is not silently replaced with an older backup. The shell remains available with **Configuration could not be migrated**, Diagnostics, and an explicitly confirmed restore action when a valid backup exists. Restore does not delete project directories, registries or credentials and does not automatically start Runtime.
