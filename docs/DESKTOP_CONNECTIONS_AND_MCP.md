# Desktop Connections and MCP Providers (0.4.2 development)

Desktop is a control surface for the user's persistent **desired configuration**.
For a persistent Environment, `EnvironmentStore` owns Tunnel profiles; Desktop and
CLI project and mutate that same authority. Legacy/non-persistent Desktop runtimes
continue to use their private local profile file. Processes and `runner.toml` are
runtime materializations, not durable configuration authorities. This development
work does not change the repository release version or publish a release.

## Runtime folders and everyday service controls

Settings → Runtime & services shows service state first. A stopped local service
has Start; a ready local service has Restart. Stop and credential repair are
under Advanced. Remote processes never acquire local service actions.

The bundled Runtime is the default; ordinary users need not select binaries.
Selecting a custom folder validates and applies it in the same action. Desktop
remembers the **directory**, not a permanent allowlist of those exact file bytes.
After recompiling into `target/dogfood`, restart Desktop, or use **Reload selected
folder** to validate and restart the Desktop-owned Runtime without selecting the
folder again. A changed Git commit, dirty build or compatible version is diagnostic
information, not a startup prohibition. An earlier saved fingerprint no longer
blocks a compatible rebuild, including when loading an older Desktop configuration.

Only interruptions with active or unconfirmed Jobs need additional confirmation.
The native switch still checks the current owner, selected candidate/revision,
architecture, protocol compatibility, and files changing *during* validation or
startup. Missing/invalid binaries remain actionable errors; Desktop never silently
falls back to a different directory. Healthy per-binary diagnostics are collapsed;
failed startup checks expand so the broken component remains visible. Installed
system/user-service environments retain their installer-based update path.

## One runtime, multiple connections

```text
Desktop
  ├─ one local Server ── http://127.0.0.1:<server-port>/mcp
  │    └─ recommended: Server-owned Tunnel profiles A, B, ...
  ├─ one local Runner ── projects / instructions / skills / MCP providers
  └─ Connections
       ├─ ChatGPT Personal ── Server-owned or separate Tunnel A ── same /mcp
       ├─ ChatGPT Work     ── Server-owned or separate Tunnel B ── same /mcp
       └─ ...
```

A connection profile has a stable opaque ID, a user-visible name, an independent
Tunnel ID/API Key pair, host ownership, and a revision. Server-owned profiles also
have per-profile startup intent. Separate managed services retain their historical
service-manager lifecycle instead of exposing an autostart switch that the native
owner cannot enforce. A profile is not a second Server or a second Runner. Reusing
a Tunnel ID in another profile is rejected. Names do not identify owners or processes.

In a persistent local Environment, **Run with WebCodex Server (recommended)** writes
an embedded record into `EnvironmentStore`; the Server loads all selected profiles
at startup and supervises them with its own lifecycle. **Separate Tunnel service
(advanced)** retains one independently managed service per profile. Saving a
Server-owned profile never installs a standalone service and never restarts a
working Server. Desktop shows one explicit **Restart Server** action when any saved
runtime revision is not yet applied, so several profiles can be configured before
one restart.

Legacy/non-persistent Desktop runtimes keep the previous process model:
`ProcessKey::RegularTunnel(TunnelProfileId)` gives every Desktop-owned CLI/native
Tunnel process tree its own supervisor entry. Server, Runner and Quick Share retain
their singleton identities. Supervisor generations fence late monitors so an old
child cannot overwrite or kill a replacement. Platform process-group / Windows Job
Object ownership remains in force.

Every Tunnel uses the same native Rust implementation and the same authenticated
local `/mcp` endpoint. Credentials are injected from the selected private binding;
no per-profile MCP Server, health port or database is allocated. Managed starts do
not race to overwrite the clipboard; **Copy ID** is explicit in each card.

### Lifecycle and health

Connections supports Add, Edit, Rename and Delete for every profile. Separate
services also expose profile-scoped Start, Stop and Restart. Server-owned profiles
use only the explicit Server lifecycle action; Desktop never maps a profile action
to an implicit Server restart. Credential changes and Server-owned autostart changes are revision-fenced
and become active after the indicated owner lifecycle action. Project selection
restarts neither connections nor Runner. Deleting a live Server-owned profile
requires a clean Server stop; ambiguous ownership retains the catalog and private
binding for recovery.

Starting a profile publishes `starting` immediately. Its observer independently
waits for a bounded ready event, then consumes health observations. The CLI probes
the native task’s poll state and the authenticated local `/mcp` information
endpoint. No jobs or ChatGPT activity are synthesized by these probes. A local
endpoint outage degrades affected connections; restoration can recover the same
processes without a restart. A dead tunnel process requires an explicit Restart
(or the next enabled/autostart Desktop runtime restoration); there is no unlimited
crash/retry loop. A stalled health observer cannot leave a stale green badge.

Server/Runner readiness is separate from connection health: one failed account does
not mark the runtime failed. The Desktop displays an aggregate such as `2 / 3
Running`, plus per-profile state, safe errors, and bounded event history. This does
not claim that ChatGPT has actually opened a session. Existing Window/ChatGPT
observations remain a separate signal. The WebUI continues to observe its own
shared Server/Runner workspace; Desktop connection failures do not alter Server
runtime readiness or expose configuration mutation through the WebUI.

Autostart is applied by the owning backend, not by frontend polling. The Server
selects embedded profiles once at startup; separate services keep their own saved
lifecycle. Legacy Desktop-owned profiles retain the previous enabled/autostart
reconciliation. First-run records the selected ownership and autostart choice;
deleting the default does not resurrect environment fallback.

### Tunnel startup and proxy recovery

A start action being accepted is not proof that the tunnel is ready. Later
readiness failures appear on the affected connection card, including after an
automatic startup. Authorization, capacity, protocol and uncertain-restart
failures have distinct safe reason codes. An unresolved previous run must not be
restarted automatically; resolve prior effects and pending work first.

When **Auto** selected a detected proxy and the tunnel reports a network/readiness
failure, the card offers an explicit diagnostic option: try **Direct** under
**Settings → Network**, then restart the affected connection. This covers the
reported Clash TUN scenario in #720 without silently bypassing a user's proxy.
Direct removes WebCodex's application-level proxy configuration; it does not
turn off the operating system's TUN or guarantee a particular network route.
Direct succeeding narrows the problem but does not establish the root cause.
Custom/Direct choices, authorization errors and uncertain-restart errors do not trigger
this Auto-proxy advice. Proxy settings are shared by Desktop connections; changes
are applied on subsequent starts, not by silently restarting healthy peers.

For support, retain the selected proxy mode, effective source, and the connection's
safe `failure_stage` / `reason_code` shown under Advanced. Do not post API keys,
authorization files or credential-bearing proxy URLs. Retrying a separate Tunnel
does not require restarting Server or Runner; applying changed Server-owned profile
runtime state requires the one explicit Server restart shown by Desktop.

### Credential migration and public state

For a persistent Environment, `tunnel.json` plus
`server/tunnels/<profile>/webcodex.env` is the canonical catalog and private binding.
CLI-created profiles are immediately visible to Desktop, and Desktop writes use the
same authority. A historical `secrets/tunnel-config.json` is never treated as a
second writable catalog in this mode. It is read only as a reconciliation fence:
exact matching credential claims are accepted, extra Environment profiles are
allowed, and missing/different claims fail closed. Desktop does not silently import,
overwrite, or delete either side.

For legacy/non-persistent Desktop runtimes, `secrets/tunnel-config.json` retains its
versioned profile collection and in-place singleton migration. A valid old pair
becomes profile `default`, named `ChatGPT`; an interrupted guarded write keeps the
previous valid file. `CONTROL_PLANE_TUNNEL_ID` / `CONTROL_PLANE_API_KEY` remain only
a legacy/default fallback, not a persistent Environment multi-profile format.

API keys never appear in public snapshots, activity, safe connection events, command
arguments, logs, errors, or UI form prefills. Private stores are not Debug-printable.
Credential inputs are write-only; a blank key retains only the selected profile's
saved value. Profile revisions fence stale editors and credential rotation; runtime
revisions separately report whether the owning Server has applied the saved state.

## Persistent MCP Providers

Open **Extensions → MCP Providers**. Configure an arbitrary Runner-compatible stdio
provider, not just Playwright. For example:

```text
Provider Name: Playwright
Command: npx
Arguments: ["-y", "@playwright/mcp"]
```

Install the required executable/package on the host first where appropriate. Bare
command names are resolved against the Desktop host's PATH and standard local
installation directories; an absolute executable path is also accepted. Resolution
does not execute a command. Arguments are a JSON string array, not a shell command.
A selected working directory must be absolute. Credential values belong in **Private
environment variables**, not command arguments or names. Arguments and command
metadata are ordinary configuration and are not secret storage.

Save stages desired state and shows **Saved · Restart Runner to apply**. **Restart
Runner** uses a fresh identity-checked target and restarts only the Desktop-owned
Runner. It does not restart Server or any connection. There is deliberately no
partial hot reload of new private environment references into an old process that
cannot possess those values.

### Storage and reconciliation

- `mcp-providers.json`: schema/revision, provider IDs/names, command/arguments,
  enabled state, `scope: runner`, environment key metadata and opaque private refs.
- `secrets/mcp-providers/<UUID>.json`: private environment values. Files are owner-only
  on Unix and their containing directory is private. Values are never projected to
  UI/activity/errors or ordinary Desktop state.
- Current `runner.toml`: safely materialized `[mcp]` provider entries. It contains
  `env_from_env` references, never these private values. Desktop injects the referenced
  values only into the owned Runner's environment. Ordinary job child inheritance
  filters the reserved `WEBCODEX_DESKTOP_MCP_` prefix; only explicit MCP mappings use it.

A new immutable private blob is flushed before the manifest points to it. Replacing
or deleting a provider retires only that transaction's old private reference after
manifest commit. An interrupted manifest commit retains the old configuration.
An unpublished private candidate may remain after interruption; it is not an active
provider and is never treated as ordinary configuration. Concurrent stale writers
cannot overwrite a newer manifest.

Every Desktop-managed Runner launch (initial setup, resume, explicit restart,
re-pairing, A/B enrollment slot change, legacy enrollment fallback) replays desired
MCP state before spawning. Removed IDs remain as bounded ownership tombstones so
returning to an older slot removes the old materialization. Operator-owned MCP
entries, native Tool Plugins and unrelated Runner settings remain untouched.

The existing identity-checked `toml_edit` mutation path preserves client identity,
Server URL, token, policy, allowed roots, unrelated options, and comments, including
array-of-tables and inline provider arrays. Duplicate IDs, malformed TOML, wrong
identity, missing executable and capacity violations fail before replacing the
Runner file. Existing advanced provider options are preserved for managed entries
unless they are the fields owned by this UI.

The Runner currently permits eight enabled MCP providers in total (including
operator-owned providers), and at most 64 environment mappings per provider,
including explicit OS bootstrap variables. Disabled desired profiles are retained
without being spawned. The Desktop model retains scope metadata for future
per-window/session lifecycles, without pretending those lifecycles exist today.

## Security boundary and concurrency

Multiple ChatGPT accounts share **one WebCodex runtime security boundary** in this
MVP. The local bootstrap principal forwarded to the Server is shared. A Desktop
profile ID is lifecycle/observability attribution, not yet an independently minted
Server principal or trustworthy per-account Window audit identity. Nothing here
creates account-level sandbox separation.

Windows, Workflow Sessions, Jobs and the project registry support concurrent
activity. Two accounts editing the **same Git working tree can still conflict**:
that is shared-workspace concurrency, not tunnel collision. Use separate worktrees
or otherwise coordinate conflicting edits.

MCP Providers are **Runner-scoped shared providers**. Provider state sharing across
Workflow Sessions depends on the provider and the Runner gateway lifecycle. This
version does not create per-window or per-session provider processes. A stateful
browser/database MCP may therefore share state across callers; do not assume
session isolation.

## Verification and platform notes

For the actual macOS deployment identities, native/browser observations, test
matrix, independent Runner restoration and rollback evidence, see
[mini dogfood verification](DESKTOP_532_537_DOGFOOD.md).

Desktop Rust tests cover multi-instance cleanup/generation fences, migration and
interruption, private projections, independent observers, TOML identity/comment
preservation, A/B re-materialization, updates/removals and cross-feature PID stability.
Frontend tests cover named semantic controls, exact profile routing, deletion
confirmation, write-only credentials, explicit Runner apply, and all six locales.

The old fake-Go-binary CLI PoC was retired with the external client adapter.
The native crate's loopback control-plane/MCP tests now cover command correlation,
no replay, credential isolation, deadlines, bounded ingress and shutdown. Desktop
connection tests cover per-profile machine events and generation fencing.
These do not prove real dual-account ChatGPT acceptance; that requires two
independently issued Tunnel identities and usable account access.

Windows retains Job Object tree ownership and platform atomic-file helpers. Windows
executable resolution recognizes native executable and command-wrapper extensions;
environment destination names are compared case-insensitively. This macOS dogfood
run does not claim native Windows execution. Windows CI/real-host tests remain the
source of platform evidence. Private storage inherits the user's application-data
ACL on Windows; there is no claim of Keychain/DPAPI encryption.

Dogfood deployment must preserve the existing `WebCodex Local Development`
certificate and designated requirements (`dev.webcodex.desktop`,
`dev.webcodex.runner.local`), create a rollback before replacement, and record actual
source/build/signature/Computer Use evidence in the PR. A rollback to a pre-profile
Desktop also requires restoring its private configuration backup, not merely the
old App. The independent `~/.local/lib/webcodex-dev/webcodex-runner` is not a Desktop
runtime deployment target.

Native Tunnel ready events use `schema_version: 2`: runtime metadata contains only
`directory` and `local_mcp_url`. The native task has no child PID, health port or
per-child log file. Health and failure events retain their unchanged version-1
shapes. Desktop and CLI must be upgraded together for this ready-event contract.
