# Nonsecret settings export

`webcodex environment settings-export [--environment-dir PATH] [--json]`
prints a schema-version-1 JSON document with `kind: "settings_export"`.
Desktop **Settings → Configuration & data → Export nonsecret settings** saves
the same document and adds Desktop's own preferences. This operation observes
existing configuration; it does not configure an Environment or import settings.

The allowlist contains the saved Environment ID, local Server/Runner roles,
saved service scope, up to 16 recorded Project IDs and path references, and the
Runner's optional `display_name`. Desktop separately adds its effective current
language (one of its seven supported locales) and automatic update download
boolean. The language comes from the existing native locale state; the update
preference comes from the existing Desktop configuration owner. CLI never reads
Desktop app-data or invents Desktop preferences.

Each value has an explicit `state`: `known` with a typed `value`, `unknown`, or
`not_applicable`. A known null device name means the optional key is absent in
an otherwise admitted Runner configuration. A missing, unreadable, malformed,
duplicated-key or mismatched Runner configuration does not provide a device
name. The parser and binding checks are the same ones used by the inventory.
An absent local Runner makes its device name inapplicable. Unknown Environment
records, project lists or legacy service scopes remain unknown. Duplicate project
IDs make the project list unknown; no registry or project-directory enumeration
fills missing references. Relative/unsafe/remote paths retain the inventory's
status, without inventing a local effective path.

Only named typed values are projected. Credential fields, API keys, tokens,
Server/Tunnel URLs, service arguments, arbitrary extension fields, raw mixed
configuration, databases, logs, recovery payloads, project contents and unknown
files are excluded by construction. Display names are bounded to 256 UTF-8
bytes, reject control characters and URL/userinfo-like values, and retain the
inventory's existing sensitive-text guard. This guard is not a guarantee about
arbitrary user-entered metadata: names and path references may be private or
contain information the user entered. Review the output before sharing it.

The inventory revision continues to bind its existing serialized location
projection (excluding observation time) and Desktop selection context. The Runner
name is an internal allowlisted observation from the same bounded configuration
read; it is not serialized into inventory or manifest documents and does not
change their revision. Native export reprojects and checks the visible context
before collecting current Desktop preferences, only for explicit settings export.
Settings values are not previewed, so language, name or update preference changes
alone do not stale navigation or metadata export. No separate settings revision is
introduced. The native save chooser selects a new `.json` document; existing
files and managed locations cannot be overwritten. Cancelled or stale context
selections do not write.
Incomplete inventories cannot establish a safe destination and refuse export.
Native output remains capped at 1 MiB and uses mode 0600/no-follow on Unix.

`cannot_restore: true` is deliberate. Exported values do not grant authority,
copy identities or credentials, register Projects, or form a complete recoverable
backup. There is no include-secrets option, import/restore action, automatic
support attachment, network request, process launch or query-state mutation.

Linux focused coverage includes all four role combinations, missing state,
actual value projection, excluded-field/file canaries, malformed/duplicate
Runner configuration, duplicate project references, revision changes, bounded
JSON/schema rejection, all seven native/UI languages, native create-new and
protected-destination behavior, stale exports and picker cancellation. Native
Windows/macOS save/ACL/WebView behavior requires their platform validation;
headless component checks do not establish installed-application behavior.

## Validation on Linux, 2026-10-06

| Focused check | Result |
| --- | --- |
| Shared inventory, including six settings-export regressions | 24 passed |
| CLI Environment tests after main `03c2c313` integration | 13 passed |
| Native Desktop inventory/open/export adapter tests | 13 passed |
| Desktop ConfigurationDataPanel, RuntimeShell, WorkspaceSettings and presentation | 81 passed |
| Desktop TypeScript, production frontend build, form-control CSS contract | Passed; existing large-bundle advisory remains |
| CLI and native Desktop dogfood builds | Passed; existing native dead-code warnings remain |
| Rust formatting and diff whitespace | Passed |
| Actual CLI binary | Missing roots stayed absent; joined Runner exported actual values; excluded canaries stayed absent; the temporary fixture tree stayed byte-for-byte unchanged |
| Isolated Chromium component fixture | At 360 px, all seven languages had no horizontal overflow and keyboard Tab reached the settings-export action |

The temporary browser fixture used the actual component and styles with an
in-memory inventory stub. It did not invoke the native application or any
production service. No full workspace suite, installation, deployment, secret
backup or restore was performed.

After merging main `03c2c313`, shared inventory (24), CLI Environment (13),
Desktop frontend (81), TypeScript/build/CSS and formatting passed again.
The native export adapter suite (13) passed after `545d5caf`; a fixture's
private updater import was corrected to the public shared compatibility type.
The added Windows binding changes do not alter inventory/export behavior.

Main `94c6e24a` was then merged without conflicts. Desktop behavior/presentation
81, native inventory/export 13, TypeScript/build/CSS and formatting passed again.

After merging upstream `8719afd7`, inventory 24, CLI Environment 22 and
Desktop export-panel 24 focused tests passed, with Desktop typecheck and Rust
formatting. Windows CI on `7a13acb6` failed three candidate-relocation fixtures
while checking inherited directory permissions (`state_io`), before export code
ran. The fixture now initializes the intermediate private directory and private
files through existing storage helpers; production permission checks remain
strict. Three relocation regressions passed on Linux. Windows confirmation for
this follow-up remains the CI gate, not a native installation claim. The repair
is appended to this feature PR and does not introduce Runtime dependencies.

## Windows CI investigation, 2026-10-08

At settings-export head `7a322714`, Windows Runner
[job 113218810442](https://github.com/yyjeqhc/webcodex/actions/runs/37749427831/job/113218810442)
failed the initial open in
`runner_windows_local_persistent_shell_preserves_state_and_existing_protocol`:
the result was `poisoned`, rather than `running` (926 passed, one failed,
47 ignored). The test never reached its later command/state assertions. This
was a test failure after compilation, on Windows Server 2025 x64, not an
installation or native acceptance result. At that failing head, the Runner,
process and persistent-shell sources matched the upstream baseline.

The old assertion omitted the actual error, and initialization merged timeout
and lost-control outcomes into the same human-readable message. The diagnostic
follow-up distinguishes those outcomes and reports only whether a control frame
and each stream's synchronization marker arrived. It preserves the existing
error code, terminal state, close reason, shutdown and 30-second startup budget.
It never emits the control token/path, cwd, initialization source or retained
stdout/stderr. The native fixture now includes the error and elapsed open time
in its failure assertion.

Two deterministic fake-transport regressions passed in the dogfood profile;
they verify diagnostic privacy, terminal poisoning/shutdown and rejection of
subsequent execution without another transport write. Windows-target library
and test compilation, Rust formatting and diff checks passed. Cross-compilation
does not execute Windows behavior. The original native cause remains unconfirmed
until the updated Windows CI reproduces or supplies a precise outcome; a
successful later attempt alone must not be described as a repaired startup race.
