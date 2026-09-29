# Personal environment lifecycle and shared authorization defaults

## Product boundary

Desktop and `webcodex environment` use the existing setup journal and native
backend. This change does not introduce another installer, identity provider,
credential kind, account/Project model or onboarding state machine.

For a new personal environment, run as the actual desktop owner:

```sh
webcodex environment configure --create --runner --scope user
webcodex environment configure-tunnel default
webcodex environment status
```

`--scope user` is the new-configuration default; `--scope system` explicitly
selects machine startup. Desktop offers the same choice before first setup.
The saved scope wins on resume, control, status, uninstall and upgrade plans.
A legacy record without the field is **System**, not an invitation to migrate it.
New User requests also encode explicit `user_create` / `user_join` mode tags.
Older binaries have a closed mode decoder and therefore reject these records;
they cannot silently ignore a new scope field and select the system manager.
Current decoding rejects conflicting scope/tag pairs. This is wire-version
recognition, not another business mode or identity. Old System encoding is unchanged.
An explicit mismatched choice or conflicting saved journal is rejected. A failed
user-manager operation never falls back to sudo, UAC or a system service.

| Scope | Linux | macOS | Windows |
|---|---|---|---|
| User | owner systemd manager, `~/.config/systemd/user`, `default.target` | `~/Library/LaunchAgents`, exact `gui/<uid>` domain | Task Scheduler, owner SID, interactive token, least-privilege run level, logon trigger |
| System | existing `/etc/systemd/system` adapter and Server socket activation | existing LaunchDaemon/system domain | existing SCM adapter and service-account credential handling |

These native lifecycles are intentionally not described as equivalent boot
services. Windows/macOS User scope requires the owner to be signed in; a macOS
GUI domain must exist. Linux post-logout/startup persistence depends on linger.
Status observes linger but does not enable it. System scope is the explicit
choice for machine startup without a user session. No account password is
requested or stored for User scope. Existing application Launch-at-Login controls
are separate from runtime service ownership.

Unix user units/plists are owner-controlled, reject linked/shared-writable service
directories, omit User/Group impersonation and address only the saved user manager.
The actual OS account is checked before user operations. Windows tasks are named
from the saved account and environment identity. A fixed PowerShell/Task Scheduler
adapter consumes data, never caller-supplied script; program/argv are literal task
properties. The task definition is compared before control and created without
replace/update flags. An unfamiliar/changed task is not adopted. Native script
parsing is tested on Windows without registering tasks. No system-service
`--windows-service` or GUI-helper relay is added to a User process.

Task Scheduler observation/control is bounded; an incomplete native command yields
unknown outcome rather than blind retry. Tasks use IgnoreNew, no execution-time
limit, and three one-minute restart attempts; this is not an unlimited retry policy.
Actual platform install/login/logout and descendant cleanup should be included in
release acceptance, not inferred solely from rendering or compilation tests.

## Reuse local credential issuance, keep the intended Tunnel trust

Local Create already durably prepares user and Runner credentials before hash
registration and reuses them when reconciling. This implementation reuses that
path; it does not invent a same-machine pairing code just to immediately redeem
it. Remote Runner Join still uses one-time pairing. Adding a Project does not
create another user, and replacing a service manager is not credential recovery.

The Desktop-managed local Tunnel **deliberately retains the local bootstrap
binding**. Its protected local authorization file remains internal to the owned
Tunnel process; ChatGPT is not configured with that secret. This represents the
same self-hosting owner's authority, not a limited delegated/multi-user grant.
No additional Tunnel PAT, consent screen, signature or credential exchange is
introduced. Separate Runner transport credentials, shared-hosted keys, delegated
OAuth and explicit Project grants keep their existing boundaries.

## Scope profile ownership

`webcodex_core::authority::profiles` now owns the existing six-scope local user
profile, the Server's twelve-scope shared-key baseline, its five optional Computer
capabilities, and the combined seventeen-scope ceiling. Local environment issuance,
pairing, Server bridge and CLI consume these definitions instead of independently
maintaining nearly identical lists and validators.

This corrects the CLI's fresh shared-key profile, which previously used a narrower
seven-/twelve-scope list than the Server. Existing stored clients/grants are not
rewritten, upgraded or revoked by this change. A nonempty valid historical baseline
subset stays that subset; explicitly enabling the Computer class adds only those
five scopes, never fills unrelated missing baseline scopes. Duplicates, empty and
out-of-ceiling sets still fail. Admin, account management, transport scopes,
detached execution and plugin management do not enter these ordinary defaults.

Supported scopes are not default grants. Managed OAuth client defaults, tokens,
refresh/revocation rules, browser PAT login and the historical admin-role-versus-
admin-scope distinction are not redesigned here. Changing these live authorization
semantics deserves a separate migration/consumer review rather than a side effect
of service setup cleanup.

## One status entry, with explicit evidence limits

`environment status` and `doctor` enrich API observations with the saved native
scope/lifecycle, owned component states, optional Linux linger, credential-file
availability, and retained Tunnel readiness. Native reads run on a blocking worker,
not on the async HTTP executor, and are not added to every readiness poll. At most
16 Tunnel profiles are listed; truncation is explicit. Collection is read-only;
knowing a service name never causes adoption, repair, restart or credential issuance.

The user credential file state is only `configured`, `missing`, or
`unreadable_or_empty`; the separate API authentication result is the validation
signal. Neither secrets nor complete config/unit content are included.

Local connection readiness requires authenticated Server observation, an online
Runner when requested, owned/running local components, and an owned/running Tunnel
with fresh control-plane and local MCP health. It does **not** prove that ChatGPT
scanned tools or used the connection. Joined remote Servers report their external
exposure as unobserved, not falsely unavailable because no local Tunnel exists.
Desktop continues to use its established separate observed-ChatGPT-use indicator.

## Health sampling is not journal verbosity

The regular Tunnel wrapper continues its two-second health probes and existing
private readiness updates, including the seven-second freshness bound. Persistent
(non-stdin-owned) processes now print a health event on state transitions and once
per sixty seconds while unchanged. A parent-owned Desktop stdout stream retains
the two-second event heartbeat, since its existing consumer treats silence as stale.
The sampling/readiness cadence and credential/IO behavior are not silently changed.

A ready event no longer depends on copying the Tunnel ID to the clipboard. Clipboard
handoff remains a separate field, and client use is explicitly not observed. No
per-process deployment, live monitoring policy or tunnel-client version is changed
by applying this source patch.

## Validation and operational limits

Unit/contract tests cover legacy-scope serialization, conflicting setup identities,
no local pairing consumption, scope-specific native plans, Unix directory safety,
Windows exact task data/quoting and script syntax, explicit Desktop selection,
conservative readiness, and narrow historical OAuth ceilings. Sampling tests use
fixed virtual instants rather than sleeps.

Final Linux verification on the frozen source:

| Check | Result |
|---|---:|
| Environment library | 102 passed |
| Core authorization-profile invariants | 2 passed |
| CLI environment input/status selection | 9 passed |
| CLI shared-key OAuth compatibility | 6 passed |
| Full default Server library | 3,138 passed, 3 existing ignored |
| Full Linux Desktop library | 248 passed, 4 existing ignored |
| Desktop frontend suite | 190 passed across 13 files |

Desktop TypeScript, form-control CSS contract and frontend build passed. Rust
formatting, whitespace and the 21-package dependency boundary checks passed.
Earlier Server pairing/bridge/Tunnel focused selections also passed; they overlap
the final full Server run and are not additional unique test counts. The first
root test compile found imports used only by pairing fixtures after profile
extraction; these were restored explicitly in the tests without widening production
scopes. Existing Desktop warnings and the frontend chunk-size warning remain.

Final review additionally rejected unknown Task Scheduler states before start,
waits under one absolute deadline for a stopped task before restart, and excludes
system-account SIDs from the User path. Windows user-session Runners no longer
advertise the SCM password-repair action. Doctor consumes the same collected
native snapshot as status instead of repeating manager calls; absent required
Server/Runner rows cannot vacuously satisfy local connection readiness.

No native Windows/macOS compilation, real service installation, reboot/login/logout,
process-descendant cleanup acceptance, full workspace/all-features suite or
cross-platform packaged deployment was run locally. The native CI lanes are
required review evidence; pure task/plist rendering and script parsing are not a
claim that OS service installation has been exercised. This remains a code change,
not a migration or deployment of oe's hand-configured services.

This work was developed in oe `/root/git/webcodex` from `5c57adeb`. No actual
user/system units, tasks, LaunchAgents, production credentials, linger setting or
Server/Runner/Tunnel processes were changed. In particular the manually deployed
oe user stack and root control Runner are not imported or replaced by this patch.
Existing unregistered manual services require their own explicit migration plan;
configure is not an adoption or takeover mechanism.
