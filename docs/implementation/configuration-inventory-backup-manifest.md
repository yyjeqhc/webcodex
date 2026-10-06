# Configuration visibility and backup manifests

## Baseline and sources

Round two began independently from upstream `390b7bbe`. Before publication,
latest upstream `6d249979` was merged without conflicts (including the separate
Core invitation fence). First Run/device UI changes remain a separate
contribution. This work projects existing
configuration; it does not relocate authority or change enrollment, project
registration, credential storage, service ownership, or upgrade transactions.

| Fact | Existing authoritative source |
| --- | --- |
| Environment root | `default_environment_dir` or the CLI's explicit `--environment-dir`, using the existing client configuration-base resolver |
| Identity, local roles, service scope, project references | Environment record or interrupted setup journal |
| Server configuration and working directory | Saved Environment service specification; Desktop-owned legacy Runtime references |
| Server data and trace locations | Known configuration path keys, interpreted with the existing Server parser and saved working directory |
| Runner configuration and project registry | Managed Runner configuration and its saved binding; existing Runner path contracts |
| Environment Tunnel profiles | Existing Tunnel records and their service specifications |
| Desktop app-data | Already resolved Tauri app-local-data root or `WEBCODEX_DESKTOP_DATA_DIR` |
| Desktop settings, Tunnel profiles, cache | Existing Desktop stores under that root; not another Environment authority |
| Persistent service logs | Existing platform adapter: systemd journal, private lifecycle file, or system task diagnostics |
| Desktop Activity and child output | Bounded in-memory queues, not an invented persistent log directory |

`EnvironmentStore::open` can create directories and adjust Windows permissions;
`lock` can create a lock file. Neither is suitable for a read-only inventory.
The existing diagnostic report is an allowlisted support projection. The existing
upgrade snapshot is private, secret-bearing recovery state. Neither is a portable
secret-free Environment backup.

## Delivered interfaces

The shared inventory is a bounded, read-only projection. Desktop adds only its
own resolved locations; the headless CLI describes the selected Environment.
Unknown, inaccessible, missing, remote, unconfigured, and inapplicable locations
remain distinct. Canonical paths are established only after native validation.
Saved configuration is not evidence of a live process's effective environment.

The versioned backup manifest describes safe metadata categories, their
projections, exclusions and role-specific restoration requirements. It contains
no file payloads and is not a complete recoverable backup. Raw mixed configuration,
credentials, database state, logs, project contents, recovery snapshots and
unknown files are excluded. Full secret backup and restore need a separate design.

Location opening and JSON export are explicit user actions bound to an observed
inventory revision. Frontend input cannot choose an arbitrary navigation target.
Inventory and manifest exports contain local paths and are private metadata, not
automatic support-report attachments.


## Use and schema

Desktop **Settings → Configuration and data** shows the Environment and Desktop
roots separately, followed by component locations. Refresh obtains a new
observation. Copy is explicit; Open sends a stable entry ID and revision to native
code, which reprojects and opens only a confirmed local directory. A remote
reference, missing/unsafe path, system journal or unknown location has no local
Open action. Linux journal entries identify the unit and user/system scope;
other platform log entries identify their existing lifecycle-file or system
Task Scheduler/Event Viewer mechanism. Activity and child output are in memory.

The CLI entry points use the same Environment projection:

```sh
webcodex environment paths --environment-dir /absolute/environment --json
webcodex environment backup-manifest --environment-dir /absolute/environment --json
```

Without `--json`, `paths` prints a location/status summary. `backup-manifest`
produces its JSON document. Omit `--environment-dir` to use the existing default
resolver. These commands do not create the root, lock files, services or network
connections. CLI output excludes Desktop-owned locations; Desktop adds those
from its already resolved app-data and selected saved configuration.

`PathInventory` schema version 1 contains `observed_at_ms`, saved environment ID,
local Server/Runner roles and service scope, roots, entries, safe identity/build
observations, issues, and an opaque revision. Each entry has a stable ID,
component, purpose, provenance, configured path, optional canonical path,
status, kind, safety category, optional native directory and optional typed log
source. These describe saved configuration, not a running process probe.
Relative paths resolve only with the saved service working directory; legacy
Desktop launch directories are not recorded, so their relative Server paths stay
unconfirmed. Invalid Unicode/control/unsafe values are not rendered as paths.

Bounds are 1 MiB per existing private metadata read, 64 entries, 16 Tunnel
profiles and 16 saved project references. The compact inventory has a 900 KiB
budget; native pretty-printed exports have a 1 MiB limit. Truncation is explicit
and prevents native export, since an omitted managed directory cannot safely be
excluded as an export destination. Navigation of retained confirmed entries
remains available. Oversized output is rejected rather than exported partially.

`BackupManifest` schema version 1 has `kind: "manifest_only"`,
`does_not_contain_files: true`, `cannot_restore: true`, `privacy_notice`, the same
`inventory`, `projections`, `exclusions` and `required_restore_materials`.
Projections describe allowlisted intent/role/scope/identity, saved project path
references and safe build metadata. UI preferences and connection modes are
acknowledged categories; their raw values, profile bodies and arguments are
excluded. This is a planning manifest, not a settings export or backup payload.

Unknown files are excluded by construction: the implementation reads only fixed
known configuration/state files, projects named fields, and does not enumerate
registry/project directories. Mixed Server/Runner/Tunnel/Desktop configurations,
credentials, databases and sidecars, recovery records, trace/log output, caches,
programs and project source contents are never copied. Only existing verified
build observations are used for other components; no executable is invoked to
fill unknown build information. Components absent from `builds` have unknown
build identity; a cached verified observation is not proof of a running version.

Complete recovery still requires original authoritative/private configuration,
compatible programs and data formats, and ownership/permissions. A local Runner
requires its exact existing identity, credentials and project registration;
a local Server additionally requires secret Server/Tunnel configuration and a
consistent database backup. A joined Runner does not require the central
Server's secrets. This round has no secret-backup switch, credential import,
identity cloning, key management or restore command.

Native JSON exports use an explicit save dialog, refuse existing files and
managed locations, and are revision-bound. Unix output files are created mode
0600 with no-follow; Windows output follows the selected destination's normal
ACLs. Choose a private directory you control. A failed write is unconfirmed and
is not silently overwritten on retry. Local paths and saved account/project
identities are private metadata: review before sharing. They do not enter
ordinary Activity, diagnostics, telemetry or automatic support bundles.

## Validation

Validation on Linux x86_64 (kernel 7.0.0-34-generic, Rust 1.98.0,
Node 24.20.0), using locked dependencies and the `dogfood` Cargo profile:

| Check | Actual result |
| --- | --- |
| Shared inventory tests | 18 passed: roles, setup journal, defaults/overrides, binding, missing/unreadable/invalid paths, no writes, link rejection, log mechanisms, canaries, JSON bounds and revisions |
| CLI Environment tests | 10 passed; actual CLI binary JSON smoke also passed for both commands against an absent explicit root, which remained absent |
| Shared pure Server parser | 1 passed; preserves literal-value grammar and guards a single-quote value against slicing panic |
| Server configuration regression module | 22 selected tests passed, including startup-file precedence and atomic invalid-file handling |
| Existing private Store link guard | 1 passed |
| Native inventory/open/export adaptation | 11 passed: saved and legacy modes, unknown working directory, revision fences, no secret projection, bounded create-new exports, protected roots, incomplete projection refusal and links |
| Existing native diagnostics | 7 passed: allowlisted support data, console navigation, trace editor and create-new support export |
| Desktop behavior/language/error rendering | 79 passed across ConfigurationDataPanel, RuntimeShell, WorkspaceSettings and presentation tests; seven languages included |
| TypeScript / frontend production build / CSS contract | Passed; existing Vite >500 kB bundle advisory remains |
| Native Desktop and CLI dogfood builds | Passed again after merging upstream `6d249979`; existing Desktop dead-code warnings remain |
| Rust formatting / diff whitespace | Passed |
| Browser component fixture | Chromium 153, 360 px fixed scroll container, long paths and all seven languages: no horizontal overflow; keyboard Tab reaches buttons. Isolated frontend fixture, not native WebView or installed application verification |

A broader Environment library run during the private-directory helper refactor
found one unchanged service-scope fixture failing under inherited `umask 0002`:
its temporary ancestor was shared-writable. The same isolated test passed with
only child-shell `umask 0022`; no service-scope code, assertion or ownership check
was weakened. The focused checks above passed before upstream integration;
inventory, CLI, native inventory and both dogfood builds were repeated after merging `6d249979`.

The initial native export test exposed a missing-root destination being classified
as unconfirmed before protected-root rejection. The implementation now rejects
known managed destinations before parent inspection; the regression passes.
Review also corrected the Runner's omitted-key registry default, refused guessed
legacy Server working directories, and blocked export of truncated inventories.

No native installation, logout/reboot, upgrade/rollback or production service
action was performed. Windows/macOS path and log branches have source/test
coverage where platform-independent; their OS-specific ACL, reparse-point,
opener and native installation behavior still require real platform validation.

## Native CI fixture correction — 2026-10-05

The first Windows Desktop run for PR #918 failed
[the legacy relative Server path test](https://github.com/yyjeqhc/webcodex/actions/runs/37324556898/job/111812094604):
its env file was created with ordinary inherited Windows temporary-directory
permissions. The private reader correctly refused that file, leaving the
configured path unconfirmed; the fixture then unwrapped an absent path.
The test now provisions its file through the existing EnvironmentStore private
writer, closes the lock handle, and renames the fixture before writing the env
payload. Windows ACL validation remains enforced. The same 11 inventory adapter
tests passed locally after this correction; native Windows CI is the remaining
platform check.

Latest main includes First Run/Add device from #917. Its Windows invitation
fixture also needs the separately reviewed private-file provisioning correction;
that follow-up is carried into this branch so the Desktop lane can test both
features together. Neither correction changes runtime permissions, identity,
configuration or service behavior. The macOS invitation effect synchronization
fix already merged upstream is retained. Linux-to-MSVC checking was attempted
but stopped in the `ring` dependency because the host lacks `lib.exe`; this is
not evidence of native Windows compilation or execution.

The first corrected Windows run passed all 10 platform-applicable inventory
adapter tests and all 476 frontend tests. It then exposed a previously masked
invitation test failure: reading `setup.lock` while the HTTP-gated invitation
holds the Core lock fails with Windows error 33. The follow-up in #919 captures
its complete file baseline before starting that operation and updates only the
expected, intentionally changed record or published identity. The final
comparison still includes the lock file, credentials, database and configuration
bytes after the operation has completed. No production lock or assertion is
removed. Both corrections are carried into this branch; final native CI evidence
is recorded in the PR description.
