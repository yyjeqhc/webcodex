# Optional Desktop lightweight mode (#947)

The native tray has an **Enter Lightweight Mode (unload UI)** action. It is an
explicit, process-local UI choice, not a saved preference or a Runtime setting.
Ordinary window X still hides the existing WebView for fast reopening. Only the
lightweight action destroys it. Quit still exits Desktop using existing shutdown
and service-ownership rules, including when no window exists.

## Lifecycle and ownership

`DesktopShellState` serializes Loaded, Destroying, Lightweight, Recreating and
ExitRequested. A successful destroy request does not claim the window is gone:
the native Destroyed event confirms removal before the same label may be rebuilt.
Only intentional no-window transitions suppress implicit `ExitRequested(None)`;
explicit application exit codes and Quit are not swallowed. Duplicate enter is a
no-op, duplicate Open coalesces onto one generation, and late creation after Quit
is destroyed without showing. Failure rolls back to Loaded or retryable
Lightweight without restarting any backend component.

All UI opening entry points use the same ensure/show path: Open, Activity,
Settings, Connections, normal second-instance launch, and macOS Reopen. Recreation
uses the existing main WindowConfig. A separate worker builds the WebView, then
its completion is serialized on the event loop. No synchronous event/tray callback
builds a WebView, avoiding the Windows/WebView2 builder deadlock documented by
Tauri. The app does not retain a second hidden WebView.

AppState, tray and background Runtime ownership remain in the native process.
Recreated React mounts read the current state but do not replay saved Runtime or
Connection autostart. While UI is unloaded, one native observer refreshes existing
persistent service status and tray snapshots every three seconds, skipping busy
operations and never calling start/stop/resume. Loaded UI keeps its existing
observation path. No credentials, registration, enrollment or Environment are
recreated by the lightweight transition.

## Navigation handoff

Native shell stores one bounded `{sequence, target}` intent. Latest explicit page
wins; plain Open does not overwrite a page intent. The renderer subscribes to
`desktop:navigate` before reading the slot, applies the intent, then acknowledges
its exact sequence. Native events are wake-ups, not the only delivery channel.
An old ACK cannot clear a newer request, and a disposed React/StrictMode mount does
not ACK an unapplied response. There are no timer guesses or repeated navigation
emits. Invalid targets are ignored; the slot is memory-only and grants no execution
authority. Reopen rehydrates backend snapshots, not serialized React state.

## Validation

Ordinary checks (native smoke is deliberately not in this suite):

```powershell
cargo fmt --all -- --check
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check
cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml
npm --prefix apps/desktop run typecheck
npm --prefix apps/desktop test
npm --prefix apps/desktop run build
git diff --check
```

For the explicit Windows/WebView2 smoke, start a dedicated Desktop dev server at
the configured localhost URL in one terminal (do not replace an existing server):

```powershell
npm --prefix apps/desktop run dev -- --host 127.0.0.1 --strictPort
```

Then, from the repository root:

```powershell
pwsh -File scripts/desktop_lightweight_windows_smoke.ps1
```

The script enables `desktop_native_smoke` only on a dedicated library test host.
Cargo unit-test hosts do not inherit Tauri's executable manifest, so it links the
smoke-only Common Controls v6 manifest onto that target. Production resources and
normal unit-test linking are unchanged. The test launches a child with temporary
Desktop/AppData/Environment/WebView storage and a unique application identity;
it never adopts the user's Environment or installs/replaces Desktop. It exercises
the actual builder, event callback and tray handlers with real WebView2, checking
X/hide, destruction (`main == None`), tray/AppState survival, repeated single-window
recreation, real React page navigation/ACK, second-instance handler and Quit from
lightweight. It has bounded observation deadlines and a child-only watchdog.

The smoke uses native handler dispatch and renderer DOM assertions, not physical
mouse clicks. Its isolated backend has no live Server/Runner/Tunnel; preserving an
already-running external Runtime is a separate process-identity observation. It
does not assert a fixed RSS reduction or that every shared WebView2 subprocess
must exit. macOS Reopen is wired through the same path but needs macOS-native
validation. A no-window `--background` startup optimization and persistent
lightweight preferences remain out of scope.
