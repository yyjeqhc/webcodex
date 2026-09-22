# Desktop Runner capabilities: ACP discovery and managed SSH

This increment addresses #616 and #603. It adds product surfaces to existing
Runner capabilities; it does not introduce another ACP runtime, an SSH registry,
provider auto-download, ACP hot reload, SSH configuration import, or credential
management.

## User workflow

The existing **Extensions** route now starts with first-class tabs for **Coding
Agents**, **SSH Resources**, **MCP Providers**, **Skills**, and **Instructions**.
Native Tool Plugins remain under Advanced. Project selection and instruction/
Skill path management stay on their project-specific tabs.

### Coding Agents

1. Install an ACP-compatible executable through an operator-controlled process.
2. Choose **Add Coding Agent**, then supply its name, logical Provider ID,
   absolute executable path, and JSON array of literal arguments.
3. Advanced settings optionally map child environment variable names to Runner
   environment variable names, allow specific ACP configuration options, and
   manage global concurrency/permission timeout settings.
4. **Save** commits Desktop desired state. It neither writes the active Runner
   configuration nor restarts Runner, Server, or Tunnel.
5. **Saved · Restart Runner to apply** remains visible until the explicit
   **Restart Runner** action applies the desired state. Availability is then
   re-observed from that exact Runner, not inferred from the saved form.

Environment mapping is names-to-names only, for example
`OPENAI_API_KEY ← SUB2API_API_KEY`. No environment values are resolved by Desktop.
Credentials belong in operator-controlled environment/private agent configuration,
not in executable arguments or the form. No provider is installed at run start.

Configured and Active are independent facts. A disabled/removed/reconfigured
provider can still be advertised by the old running process until restart.
Advertised providers that are not in Desktop desired state are shown read-only.
The displayed name and logical ID must match the live advertisement before a
Desktop-managed row is shown as Active.

### SSH Resources

The inventory always comes from the exact Desktop Runner's canonical
`ssh_resource` gateway. It shows only name, Managed/Static source, active state,
and pending-restart state. Static resources have no Remove action.

**Add SSH Resource** accepts a name, a target, and optional default working
directory. A target can be `user@host`, a hostname, or an existing OpenSSH alias
such as `special`. OpenSSH resolves aliases. Desktop does not parse or import
`~/.ssh/config`, identity files, jump hosts, passwords, or keys.

After submission, target/cwd are discarded from the transient form. They are not
returned in the inventory and there is no local history or parallel SSH database.
To change a target, remove the managed resource and add it again. Removing a
resource removes the managed reference, not files on the remote machine.

Every mutation consumes a fresh observation and immediately re-lists. An uncertain
result remains explicitly uncertain even when the subsequent list succeeds.
**Operation status uncertain. Refresh resources before retrying** is not an
invitation for an automatic mutation retry: register/remove are each sent at most
once per explicit submission. Runner replacement and stale-registry errors also
discard old observations.

## ACP discovery contract

`runtime_status` (full, compact, and exact-Runner focus), `list_runners` (full and
summary), the Desktop Runner overview, and Project startup use a dedicated safe
summary type:

```json
{
  "coding_agent_providers": [
    { "provider_id": "pi", "name": "Pi Agent" }
  ]
}
```

The summary is bounded to the canonical provider limit and contains only logical
ID and display name. It never serializes `provider_instance_id`, executable,
arguments, cwd, environment, source environment names, configuration paths, or
ACP session identifiers. Runner visibility continues to be authorized before
projection. Empty, null, or absent old inventories yield an empty safe list;
an old Runner serializing an empty list does not acquire execution capability.

Project startup selects only the Project's online owning Runner from the already
authorized runtime observation. It does not merge fleet inventories or establish
a default provider. The compact `work_on_project` projection retains this field,
so normal project entry naturally exposes provider choices. Instruction content
can still specify a preferred provider; Server does not choose one implicitly.

`coding_agent_provider_unavailable` remains a not-started error. It includes the
exact Runner's safe available providers and a mechanical `runtime_status` call
for re-observation. It does not search PATH, substitute a provider, or retry a
different logical ID. Existing internal Runner/provider replacement fences remain
unchanged.

## Desktop ACP ownership and persistence

`coding-agents.json` is Desktop desired state, with a schema version, monotonic
revision, private stable owner UUID, provider profiles, optional global settings,
and retained managed-ID tombstones. It is bounded, rejects malformed state, and
uses compare-and-swap atomic persistence. Invalid state is not treated as an
empty writable configuration.

Reconciliation writes a `desktop_owner` marker on each owned `[[acp.agents]]`
entry. It is configuration ownership metadata, not model-facing provider identity.
Existing operator entries are never adopted solely because their IDs match:
matching IDs without the exact ownership marker are conflicts. Removal and rename
retain old IDs as tombstones. The same desired state is reconciled when restarting
Runner or generating/selecting enrollment slots, so an older A/B slot cannot
resurrect an entry the Desktop removed.

Both table-array and inline-array TOML are supported. Unrelated entries, comments,
server URL, client ID, token, policy, and other capability settings are preserved.
The combined operator/Desktop enabled-provider count must fit Runner's limit.
Global settings remain untouched unless explicitly managed through Advanced.

Save stages and validates a candidate against the exact current configuration
before committing desired state. Runner startup preflights ACP ownership before
capability reconciliation. ACP is still in the restart-required configuration
domain; there is no hot replacement or fabricated Active result.

## Closed SSH native boundary

The WebView has dedicated `ssh_resource_list`, `ssh_resource_register`, and
`ssh_resource_remove` commands. It cannot choose an arbitrary runtime tool,
endpoint, Runner, header, or credential. Native code sends the fixed canonical
`ssh_resource` request through `/api/tools/call`, using the current connection's
private user token. The Server's normal tool kernel and SSH gateway retain final
scope, ownership, exact-instance, and revision authorization.

Only an opaque, single-use Desktop observation ID is projected to the WebView.
The canonical Server binding stays native. Its native observation is tied to the
Server/Runner/configuration/credential-file identity. A local single-flight gate
serializes mutations; the Server independently enforces its binding/revision.
The Desktop state lock is not held across network I/O. Late reads are discarded
when the connection changes or shutdown begins.

HTTP requests do not follow redirects. Request/response sizes and credential reads
are bounded. HTTP transport uncertainty, malformed mutation responses, or lost
native IPC responses never trigger another mutation. Error presentation uses a
closed vocabulary rather than raw response/parser text.

## Explicit enrollment and legacy local authorization

The SSH gateway still requires `ssh:local`; calling it with an old under-scoped
user token does not bypass that check.

New administrator-issued pairing codes can explicitly opt into Runner capability
access with `runner_capabilities: true`, or CLI `pairing create
--runner-capabilities`. The issuance record, not the enrolling client's body,
determines the grant. Local Desktop setup requests this opt-in using its existing
operator authority. Old persisted codes migrate with the flag false, and old
transport tokens do not gain SSH or coding-agent permissions.

An existing Desktop-owned local connection can choose **Authorize Runner
Capabilities** from either **Coding Agents** or **SSH Resources**, review the
same scope description, and explicitly confirm. The shared flow observes current
user-token scopes through a closed native `runner_capability_authorization` call
to `/api/pairing/runner-capabilities/status`. This read needs `runtime:read`, not
SSH access, a Project, provider inventory, or an online SSH registry. Coding-only
legacy connections therefore discover the upgrade without ever opening SSH.
Connection changes discard the previous confirmation; unavailable observations
are not interpreted as permission. Grant results are re-observed with the user
token independently of SSH inventory; uncertain grants are never retried automatically. This uses
a narrow native-only call to `/api/pairing/runner-capabilities` with the local
Server operator credential and a hash identifying the existing user token. The
endpoint requires account-management plus administrator/bootstrap authority,
an online exact Runner whose owner matches that user, and a live unbound user
key. It adds only `ssh:local` and `coding_agent:run` using a fenced database update.
It cannot accept arbitrary scope names. Token bytes, owner, expiration, revocation,
and Runner transport credentials are unchanged.

The local grant reads the bound address and operator credential from one bounded
private environment-file snapshot and sends neither value nor token hash to the
WebView. Only literal loopback HTTP endpoints with an explicit port are eligible.
Remote connections require an operator-approved pairing code instead. Grants are
never attempted automatically, including after a failure. No Server, Runner, or
Tunnel restart is required for the scope update.

## Validation and manual acceptance

Automated coverage includes safe full/compact/focused discovery, Project ownership,
old inventory compatibility, unavailable-provider recovery, desired-state CAS and
restarts, A/B tombstones, operator collisions/comments, names-only environment
mapping, closed SSH requests, single-use observations, static read-only behavior,
stale/replaced/conflict/uncertain outcomes, bounded HTTP transport, explicit grants,
old-code schema migration, and accessible UI workflows. Native HTTP fixture tests
drop a mutation response after receiving it and assert `list → register → list`,
never `register → register`.

These tests are not a substitute for deployment, native Computer Use, or an actual
model-backed Pi run. Acceptance must record these independently:

| Layer | Required evidence |
| --- | --- |
| Fixture ACP E2E | Desktop Add/Save/Restart, actual advertisement, fresh Project bootstrap, start/observe to terminal against the repository fixture |
| Real Pi ACP E2E | Pinned installed adapter/platform versions, real executable, ACP session and terminal output on a safe read-only Project |
| sub2api + Grok | Actual authenticated local model inventory, selected returned Grok ID, evidence of a request and returned output, no provider/model/API fallback |
| SSH/Computer Use | Accessible Add/Refresh/Active/Remove on the native Desktop, a controlled read-only SSH command, and removal of only the temporary managed resource |
| Deployment safety | Independent control-plane access, current rollback/build identities, protected independent Runner hash unchanged, and unchanged Server/Tunnel identity across Runner-only actions |

On the implementation host, the independent mini control Runner was offline and
the existing local sub2api model endpoint returned HTTP 401. Those are acceptance
blockers, not successful dogfood evidence. Do not install a second API service,
guess a Grok model ID, expose credentials, silently switch providers, or deploy by
stopping the Desktop from its own managed Runner to work around them.
