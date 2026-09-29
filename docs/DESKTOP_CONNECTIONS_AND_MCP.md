# Desktop Connections and MCP Providers (0.4.2 development)

Desktop owns the user's persistent **desired configuration**. Processes and
`runner.toml` are runtime materializations, not the durable source of those settings.
This development work does not change the repository release version or publish a
release.

## One runtime, multiple connections

```text
Desktop
  ├─ one local Server ── http://127.0.0.1:<server-port>/mcp
  ├─ one local Runner ── projects / instructions / skills / MCP providers
  └─ Connections
       ├─ ChatGPT Personal ── owned tunnel-client A ── same /mcp
       ├─ ChatGPT Work     ── owned tunnel-client B ── same /mcp
       └─ ...
```

A connection profile has a stable opaque ID, a user-visible name, an independent
Tunnel ID/API Key pair, enabled/autostart intent, and a revision. It is not a second
Server or a second Runner. Reusing a Tunnel ID in another profile is rejected.
Names do not identify processes: renaming an active profile leaves its PID intact.

`ProcessKey::RegularTunnel(TunnelProfileId)` gives every owned CLI/tunnel-client
process tree its own supervisor entry. Server, Runner and Quick Share retain their
singleton identities. Supervisor generations fence late monitors so an old child
cannot overwrite or kill a replacement. Shutdown visits every owned exposure
before stopping the shared Runner and Server; graceful stdin EOF remains the first
stop mechanism. Platform process-group / Windows Job Object ownership remains in
force.

Every tunnel launch uses the existing UUID runtime directory, private authorization
file, health URL file, individual log file, and `127.0.0.1:0` health listener. No
per-profile WebCodex Server port or database is allocated. Managed starts do not
race to overwrite the clipboard; **Copy ID** is explicit in each card.

### Lifecycle and health

Connections supports Add, Edit, Rename, Start, Stop, Restart and Delete. API actions
identify exactly one profile. Credential edits replace only that active connection;
MCP edits never restart a connection. Project selection restarts neither connections
nor Runner. Delete stops the owned process before deleting its credential; a failed
stop retains the profile for recovery.

Starting a profile publishes `starting` immediately. Its observer independently
waits for a bounded ready event, then consumes health observations. The CLI probes
its loopback tunnel health listener and the authenticated local `/mcp` information
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

Autostart is replayed by backend reconciliation, not by frontend polling. Enabled
profiles with **Start automatically** selected are started after runtime restoration.
Stop persists disabled intent. First-run's explicit default connection also records
its autostart choice; deleting the default does not resurrect environment fallback.

### Tunnel startup and proxy recovery

A start action being accepted is not proof that the tunnel is ready. Later
readiness failures appear on the affected connection card, including after an
automatic startup. Download, local installation and integrity verification
failures have distinct recovery messages; changing credentials does not repair a
full disk or an invalid downloaded binary. Integrity checks must never be bypassed.

When **Auto** selected a detected proxy and the tunnel reports a network/readiness
failure, the card offers an explicit diagnostic option: try **Direct** under
**Settings → Network**, then restart the affected connection. This covers the
reported Clash TUN scenario in #720 without silently bypassing a user's proxy.
Direct removes WebCodex's application-level proxy configuration; it does not
turn off the operating system's TUN or guarantee a particular network route.
Direct succeeding narrows the problem but does not establish the root cause.
Custom/Direct choices, installation errors and verification errors do not trigger
this Auto-proxy advice. Proxy settings are shared by Desktop connections; changes
are applied on subsequent starts, not by silently restarting healthy peers.

For support, retain the selected proxy mode, effective source, and the connection's
safe `failure_stage` / `reason_code` shown under Advanced. Do not post API keys,
authorization files or credential-bearing proxy URLs. Server and Runner need not
be restarted merely to retry one tunnel.

### Credential migration and public state

The existing private `secrets/tunnel-config.json` is migrated in place to a versioned
profile collection. A valid old pair becomes profile `default`, named `ChatGPT`;
the old connection preference supplies initial autostart intent. A write is staged,
flushed and atomically replaced with an original-content check. Interrupted migration
keeps the previous valid file and fails closed; no environment identity is silently
substituted. Invalid/symlink/oversized files are not implicitly overwritten.

`CONTROL_PLANE_TUNNEL_ID` / `CONTROL_PLANE_API_KEY` remain legacy/default fallback,
not a multi-profile format. An explicitly empty collection is authoritative.

API keys never appear in public snapshots, activity, safe connection events, command
arguments, or UI form prefills. The private store is not Debug-printable. Credential
inputs are write-only; a blank key retains only the selected profile's saved value.
Per-profile `tunnel_profile_id` is attached by the supervisor, not trusted from child
output. Revision checks and guarded writes reject stale editors.

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

The production CLI fixture PoC is executable with:

```sh
python3 scripts/tests/desktop_multi_tunnel_poc.py --webcodex <built-webcodex>
```

It replaces only tunnel-client/the remote control plane, while running two actual
production `webcodex server tunnel` processes against one authenticated local MCP
fixture. It checks unique real health ports, runtime/log/auth paths, independent
stop/restart/failure, upstream outage/recovery, secret non-output and EOF cleanup.
It does **not** prove two real ChatGPT accounts. Real dual-account E2E requires two
independently issued, usable Tunnel ID/API Key pairs and the corresponding account
access; never clone a live ID and label it a second account.

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
