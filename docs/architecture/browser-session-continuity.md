# Browser session continuity and exact Computer handoff

This extends the existing `webcodex-browser` domain. It does not add raw CDP,
selectors, scripts, debugger ports, native IDs or filesystem profile paths to tools.

## Ownership, launch and cleanup

| Ownership | Launch/attachment | Cleanup | Persistent data |
|---|---|---|---|
| `owned_ephemeral` | Existing default headless Chrome, temporary profile | Close/reap only the owned process tree | Temporary directory removed |
| `owned_managed_persistent` | Explicit `mode=managed`, private named profile, visible Chrome | Close/reap only the owned process tree | Retained for the next launch |
| `attached_external` | Exact live offer shared by the user in the extension | Debugger detach only; never Browser.close, window close or process kill | Existing Chrome profile untouched |

Existing `{action:launch, client_id:...}` inputs retain their shape and behavior.
Managed launch adds `mode:managed` and `profile:login` (a 1..48 lowercase name, not
a path). The private layout is `<Runner default client state>/browser/profiles/
<name>/data`, with an exclusive process lease beside it. Profile roots reject
symlinks/reparse points, other owners and non-private permissions. Unix also refuses
a live/unverified Chrome SingletonLock, rather than adopting an orphan process.
No personal Chrome profile is copied or opened as managed storage.

Closing a managed Browser releases its process lease but retains cookies/storage.
Relaunching the same profile creates new opaque Browser/page/element identities.
Opaque identities are never durable bookmarks or restart authority. Login challenges
remain human actions; there is no CAPTCHA or login-security bypass.

## Existing Chrome bridge

`extensions/browser-bridge` is a Manifest V3 development extension, minimum Chrome
118. It requests only `activeTab`, `debugger` and `nativeMessaging`. There are no
content scripts, remote origins, externally-connectable APIs or broad host grants.
The user explicitly chooses **Share current tab** in its popup. The extension sends
an offer; it does not attach the debugger until a subsequent authorized Browser
attach operation consumes that exact offer. **Revoke current tab** detaches it.

The Runner owns a loopback-only TCP listener and a private, user-owned rendezvous
record under `browser/extension`. A fresh 256-bit token and instance ID authenticate
the native host; these never enter the extension, model output, page content or
logs. The host checks the exact installed extension origin, derives its Chrome
parent process from the OS, and verifies the parent lifetime. Reconnection creates
a new peer and invalidates old offers/leases/routes. The host exits on EOF or Runner
loss; the extension's port-disconnect handler detaches all debuggees and clears
consent. No Chrome process is adopted or killed.

The internal protocol is version 1, length-prefixed bounded JSON: little-endian
32-bit lengths for local TCP and native-endian lengths for Chrome stdio. Limits
are checked before allocating bodies: 256 KiB commands, 4 MiB responses, 64 KiB
events, 4 peers including unverified handshakes, 16 ten-minute offers, 64 channels,
32 queued commands per peer, and 16 MiB total queued response/event data. Queue
exhaustion closes the peer and invalidates leases, never retries an effect. The
native host handshake is bounded at two seconds. Socket timeouts retain partial
frame state instead of interpreting a truncated body as a new header.

The closed bridge controls are attach, detach, targets, new_page and close_page.
The CDP channel admits only the domains needed by the shared Browser backend.
`Browser.close` is never forwarded. An explicit `close_page` tool may close its
exact leased tab; Browser expiry/shutdown/detach never closes tabs. Lost effect
acknowledgements preserve `outcome_unknown`; only observation reconciles them.

`CdpOwner` represents ownership; `CdpSocket` represents transport. The local
WebSocket and authenticated bridge socket feed the same CDP response handling,
Supervisor, opaque identities, page reconciliation, document/iframe/snapshot
fences, action admission, disabled/read-only checks, upload authorization,
deadlines, diagnostics and result/image bounds. The extension is not a second
unfenced Browser tool API. Out-of-process iframe support remains bounded by the
existing same-origin/frame policy; extension attachment does not grant privileged
Chrome pages or bypass debugger restrictions.

## Tool sequence and capability admission

1. `observe_browser(action=targets)` reports independent capability bits.
2. Managed: `control_browser(action=launch, mode=managed, profile=login)`.
   Existing Chrome: user shares a tab, then `observe_browser(action=discover)` and
   `control_browser(action=attach, attachment_id=<exact offer>)`.
3. Use Browser pages/snapshot normally. Request `observe_browser(action=surface,
   browser_id=<exact Browser>)` for Computer handoff.
4. Use the returned opaque `surface_id` with normal Computer observation/control.
   Then take a new Browser snapshot after human/Computer changes.
5. `close_browser` follows the ownership table above.

New capabilities are `browser_managed_profile`, `browser_extension_bridge` and
`browser_surface_handoff`, all absent/false on old Runners. The bridge capability
means this Runner owns its listener, not that a tab is shared or Chrome has already
loaded the extension. Launch remains `browser:launch`; attach and other effects
remain `browser:control`. Discover is `browser:read`; surface resolution requires
both `browser:read` and `computer:read` plus the exact handoff/Computer capabilities.
No legacy capability implies new support.

The Runner holds the Browser operation lease while checking the exact live Browser
process and a fresh native Computer window inventory. It matches the Browser/main
process, not renderers or titles. Multiple matching windows, incomplete inventory,
missing windows, or isolated Computer sessions fail closed. The returned Surface
keeps existing native identity and snapshot fencing. macOS/Windows support this
mapping; Linux native handoff is not advertised. External multi-window Chrome may
therefore need the user to select a less ambiguous instance; no guessed mapping is
presented as exact.

## Development installation

Build the regular Runner and the same-revision native binary:

```
cargo build --locked --profile dogfood -p webcodex-runner -p webcodex-browser
python3 scripts/install_browser_bridge.py --binary /absolute/path/webcodex-browser-bridge
```

The installer writes only the current-user Chrome Native Messaging manifest (and
HKCU registration on Windows), with a single exact `allowed_origins` entry. It
backs up an existing changed manifest and refuses to overwrite that backup. It
does not modify Chrome preferences, extensions, profile contents or running Chrome.
Use Chrome's extensions page, enable developer mode, and load unpacked
`extensions/browser-bridge`. Pin the extension and share the selected HTTP(S) tab.
The Native Messaging host and Runner must use the same user and default client
state root (including the same XDG state override on Unix). Other Chrome-family
brands need their own native manifest registration; they are not installed by the
Chrome installer. Signed store packaging and auto-updates are not part of this
initial development distribution.

## Validation

```
cargo test --locked -p webcodex-browser
cargo test --locked -p webcodex-computer browser_handoff
cargo test --locked -p webcodex-tool-contracts browser
cargo test --locked -p webcodex --lib browser
node --test extensions/browser-bridge/worker.test.cjs
```

Native loopback fixtures test exact authentication, stale instances/leases, bounded
framing/queues, shared CDP, detach-only teardown and uncertain delivery. Extension
VM fixtures test consent, unsupported process commands, exact tabs, attach races
and disconnect cleanup. Neither fixture suite is a claim of real Chrome dogfood;
real deployment, profile persistence, screenshots and live external attach are
recorded separately with their observed limitations.
