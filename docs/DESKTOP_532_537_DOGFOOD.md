# Desktop #532 / #537: mini dogfood verification

Verified on macOS arm64, 2026-09-19. This is development work for 0.4.2, not a
release. Package/build versions remain 0.4.1; no tag, release, or npm publication
was performed. Architecture and security contracts are documented in
[Desktop Connections and MCP Providers](DESKTOP_CONNECTIONS_AND_MCP.md).

## Source and deployed identity

The feature branch was created from `cc1943eff0679308852909cad9e3a72918e55ee9`,
the merged #531 baseline, rather than continuing the previously merged #470 branch.

| Component | Actual deployed source | Evidence |
| --- | --- | --- |
| CLI, Server, Runner | `5c53c10b378f9722c04e33f2e081241c92ff4ddf` | All three report `dirty=false`, `built_at=1789778819`; Server and Runner report aligned through `@mini.runtime_status`. |
| Desktop application | `ec451e5d4cf9c0799bce33e50f00c30692b4bcfd` | Includes the two native-UX fixes below; built from a clean checkout. |
| Independent recovery/control Runner | `9e200c51f7bf7c9d766981054d1033fc278eb7d8` | Live PID 26467 and restored disk executable have the original source identity; its config was not changed. |

The runtime uses the `dogfood` Cargo profile and includes experimental Code Mode.
The Desktop reuses the existing debug dogfood packaging/cache. Only Desktop was
incrementally rebuilt for the UI fixes; no full candidate runtime rebuild was
repeated. Server/Runner remain exactly the tested 5c53c10b binary inputs; the later
commits change Desktop frontend/tests and documentation. A CI follow-up adds a
missing error conversion only in the non-Unix MCP credential writer and aligns
a Goal test fixture with updated main; neither changes the deployed macOS logic.

The installed application remains at:

```text
target/desktop-code-mode-tauri/debug/bundle/macos/WebCodex Desktop.app
```

Runtime executables remain in `target/dogfood/`. Local code signing retains
`WebCodex Local Development` and the existing certificate-anchored designated
requirements for `dev.webcodex.desktop` and `dev.webcodex.runner.local`.
The final signed Desktop executable SHA-256 is:

```text
91c597a9819d50fb5a1b5095470c98a9262da972202a01ef20ce8404a22a934c
```

## Control-plane separation and readiness

Every Desktop replacement or complete restart in the recovery phase used
`@0914 -> mini`, whose independent Runner is PID 26467. The controller verified
that it descended from that independent Runner and was not in the exact Desktop
executable's descendant set. It staged and verified the application beside the
fixed target, stopped only the Desktop-owned tree, waited for its exit, then
renamed the application and relaunched the fixed path. PID 26467 was not stopped,
restarted, or included in Desktop cleanup. Runtime and independent Runner/config
hashes were checked around the final Desktop-only replacement.

`GET /health` returns 404 in this source; it was not treated as readiness success.
The canonical authenticated `POST /api/runtime/status` returned 200, followed by
actual `@mini.runtime_status` verification of online/aligned Server and Runner.
Accessibility trust, native Screen Recording snapshots, Code Mode and five visible
projects were verified. No extra Server port or Runner was created per connection.

## Automated evidence (not real-account E2E)

| Validation | Result and scope |
| --- | --- |
| Desktop Rust library | 124 passed, 0 failed, 3 explicitly ignored native/dogfood tests. Covers migration interruption, private projections, process-instance isolation, health observation, shutdown, desired MCP state, TOML identity/comments, re-pair/A-B materialization and cross-feature boundaries. |
| Desktop frontend | Final 69 tests passed in six files; TypeScript and form-control CSS checks passed. Includes failed/reaped-profile Stop/Restart and portal accessibility regressions. |
| Runner MCP credential-source isolation | One focused test passed; private Desktop MCP source variables are not ambient shell job environment. |
| Final source review | No conflict markers or whitespace errors; working tree checked clean before deployment and before push. |

Rust/runtime source did not change after its recorded validation, so those results
were reused. No redundant full workspace build or test run was used to replace
focused evidence.

## Fixture concurrent-tunnel PoC

The former `scripts/tests/desktop_multi_tunnel_poc.py` (retired when the native Rust client replaced the external Go adapter) exercised two actual production CLI
`webcodex server tunnel` processes with fixture tunnel clients, targeting one
authenticated loopback MCP fixture. It verified concurrent operation, unique real
health ports and runtime/log/auth paths, independent stop/restart/failure,
upstream outage/recovery without restarting the tunnel, authorization cleanup and
12 authenticated MCP probes. It created zero database files; that fixture is not
an independent real-Server database-load benchmark or a real dual-account test.

**Real dual-account E2E not validated.** Only one usable real Tunnel credential
pair was available for native dogfood verification. The second profile used a
separate syntactically valid but deliberately unusable fixture credential. No
real Tunnel ID was duplicated to pretend that two accounts had connected.

## Real Computer Use: Connections

The existing real connection was renamed through the native UI to **ChatGPT
Personal**, retaining its write-only saved credential. Its Save & Apply, Stop,
Start and Restart controls were operated. The user entered the fixture API Key
into the protected native password field; automation did not bypass that field's
security restriction. The Add Connection form persisted **ChatGPT Work Fixture**.

Native observations then showed Personal **Running**, Work Fixture **Needs
attention**, Server **Running**, Runner **Running**, and **1 / 2 connections**.
Fixture Restart, Stop, Start and confirmed Delete were all performed through
native semantic controls, after the recovery-controls fix. Stop persisted
`enabled=false`, and Start re-enabled only the fixture profile.

Throughout that sequence the following PIDs remained unchanged:

| Owned component | PID |
| --- | ---: |
| Desktop | 8286 |
| Shared Server | 8305 |
| Shared Runner | 8309 |
| Personal tunnel wrapper | 8380 |
| Personal tunnel client | 8513 |

The exact Personal desired configuration and MCP manifest digests were unchanged.
Delete removed only the fixture profile/credential from the authoritative store;
no fixture wrapper remained. The real Personal connection remained available.
The invalid test connection was not left configured or autostarting after testing.

## Real Computer Use: persistent MCP Providers

Using **Extensions -> MCP Providers**, native controls added **Desktop MCP
Fixture**, compiled directly from the repository's standalone
`crates/webcodex-runner/src/webcodex_runner/fake_mcp_gateway.rs` (not a newly
implemented provider). Name/Command were entered through Computer Use; `[]`
arguments were retained. Save displayed the explicit Restart Runner apply state.

After the native Restart Runner action, Runner PID changed from 98177 to 98818,
while Server 98173, tunnel wrapper 98250 and tunnel client 98376 were unchanged.
The Desktop desired manifest remained present, the active Runner TOML contained
the materialized provider, and identity/token/policy were preserved. The gateway
listed and invoked the fixture `echo` tool. After a complete Desktop exit/relaunch
through the independent control plane, the same provider remained visible in the
native UI and the desired manifest digest was unchanged. It also remained
registered after the final Desktop-only UI deployment.

Re-pair and A/B slot regeneration retain automated regression evidence; the live
pairing was not deliberately destroyed for an additional manual test. The benign
MCP fixture remains configured for dogfood verification.

## Real Browser Use

Live `http://127.0.0.1:62645/runtime` was authenticated using the canonical Browser
Use input path without projecting the credential. While the native Work Fixture
profile was in Error, Browser Use verified the Ready homepage, Server Running,
one online Runner, five projects, Projects and Windows views, and opened the real
Workflow Session associated with the active Window. A failed tunnel did not turn
the shared Runtime Workspace into a failed runtime.

## Native UX fixes and observation recovery

1. `faa0ef58`: redundant nested accessibility landmarks hid MCP controls beyond
   bounded native tree depth. Removing the redundant wrappers and portalling
   dialogs to `document.body` exposed the named controls while preserving focus
   and busy-dismissal semantics. Two dedicated regressions were added.
2. `ec451e5d`: an enabled profile whose failed child had been reaped showed only
   Start, hiding Restart and the ability to persist Stop intent. Failed enabled
   profiles now retain both controls; stopped profiles still expose Start.

The operator also corrected preflight harness assumptions before stopping any
process (the recorded Server env path and codesign output stream), and re-observed
Browser nodes after asynchronous navigation/stale-node rejection. These were not
silently counted as successful attempts. Successful final deployment and native
observations are separately retained below.

## Independent Runner source restoration

An earlier signing invocation had accidentally written the independent Runner's
default destination. Recovery used a temporary clean worktree at the exact
9e200c51 source, built only `webcodex-runner`, and passed both SOURCE and an explicit
staging DEST to `scripts/macos_sign_local_runner.sh`. Staging source, signature,
identifier and existing certificate requirement were verified before an atomic
same-directory replacement of the independent executable. PID 26467 was never
restarted; the independent config digest remained unchanged.

**Source identity restored; original byte-for-byte SHA not recoverable locally.**
The rebuilt/re-signed file is not claimed to be the original artifact:

```text
Original SHA-256: 4adec06b99fac1e9b6544703b677248f2a30ed8896f8f706e948f7bd762e29e8
Restored SHA-256: 191278303cee8f395648be9757e74245c8f005342d02a75b011c80b1234b439d
```

Only identified Cargo incremental directories were removed to recover disk space;
deployed executables, candidates, rollback copies and evidence were retained.
The temporary recovery worktree was removed after restoration; the signed staging
artifact remains in the evidence directory.

## Windows considerations and remaining boundary

No native Windows deployment/E2E is claimed in this macOS run. Windows retains
existing Job Object/process-tree ownership, atomic-file helpers and bounded
command-wrapper support. CI/native Windows testing supplies that platform's
execution evidence. Private storage relies on the user app-data ACL on Windows;
this work does not claim Keychain/DPAPI encryption.

Multiple ChatGPT connections share one runtime security boundary and bootstrap
principal in this MVP. Concurrent writes to one Git working tree can conflict;
stateful MCP Providers are Runner-scoped and may share state. Per-account
principal attribution and per-window/session provider processes remain separate
future work, not hidden guarantees of these profiles.

## Post-push CI integration corrections

The first PR #544 CI run tested merge commit
`278a647b2763d9e7122c4fdc17ec9b704675711b`, combining this branch with newer main
`3ec0c709adc0753448fc4ea49eef84c13c5998c1`. Its Goal Plan App fixture omitted
`controller_agent_id`, which updated main now requires to be either null or a
valid Agent ID. The same 27 failures were reproduced against both main alone
and the CI merge, while this branch's original Goal tests passed 43/43.
Adding explicit `controller_agent_id: null` to the fixture restored 43/43 on the
CI merge's unchanged production HTML. The current branch's complete Node MCP App
suite passed 253/253. No production validation was weakened or replaced.

The Windows Desktop compile gate separately found a missing `std::io::Error`
to `DesktopError` conversion in the `#[cfg(not(unix))]` private-credential writer.
It now uses the same `map_err(|_| invalid())` redaction boundary as the surrounding
storage operations. This is a Windows/non-Unix compile correction; the deployed
macOS code path and candidate runtime bytes are unchanged. Updated CI results
belong to the PR check suite; local macOS checks do not claim Windows execution.

## Rollback and local evidence

Original pre-migration Desktop/runtime/config rollback:

```text
target/dogfood-rollbacks/issue532-537-20260919T004803Z
```

The final frontend-only replacement also retained:

```text
target/desktop-code-mode-tauri/debug/bundle/macos/WebCodex Desktop.recovery-controls.previous.app
```

Any rollback that stops Desktop must use the independent control plane. A rollback
to the pre-profile version must restore its corresponding private configuration
backup as well as app/runtime binaries. The independent Runner is not part of
that Desktop rollback target.

Operator evidence is retained locally under `target/desktop-532-537-runtime/`:
`continue-deployment.json`, `desktop-ax-deployment.json`,
`desktop-recovery-controls-deployment.json`, `two-profile-native-isolation.json`,
`mcp-ui-persistence.json`, `browser-fixture-error-smoke.json`,
`independent-runner-restoration.json`, `completion-gates.json`,
`desktop-rust-final.log` and `ui-failed-profile-controls.log`. These records separate
automated tests, fixture topology, real native/browser observations and the
unvalidated real dual-account E2E boundary. Private stores are not PR artifacts.
