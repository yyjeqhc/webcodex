# Desktop Browser runtime status — #1055 implementation gate

Status: **read-only first stage implemented**. The Desktop dashboard shows a bounded, Runner-scoped count using the existing BrowserObserve authorization gateway. **Close all remains unimplemented and unauthorized by observation alone.**

## Verified existing boundaries

- `observe_browser(action=browsers, client_id=...)` goes through `src/tool_runtime/browser_tools.rs` to the exact Runner `browser_list_browsers` operation, with `BrowserObserve` admission.
- `control_browser(action=close_browser, ...)` goes through the Browser control path and its separate `BrowserControl` authorization. Listing is not shutdown authority.
- `RunnerBrowserOperationKind` in `crates/webcodex-core/src/runner_operation.rs` distinguishes `ListBrowsers` from `CloseBrowser`.
- The Desktop dashboard now queries `/api/runtime-console/browser-status` on mount/manual refresh for its selected Runner. The Tauri adapter selects the authenticated Server and configured Runner; the Server returns only a validated count, not Browser identities. Browser state belongs to a Runner, not a global Desktop singleton.
- #966 already addressed idle-runtime cleanup; this issue must not alter those timers or adopt external Chrome processes.

## Implemented first stage and remaining proposal

1. **Implemented for the selected Runner:** an authenticated, read-only snapshot adapter reuses the canonical BrowserObserve visibility, capability and dispatch gates. Desktop queries on mount/manual refresh, not by permanent polling; the Server returns a validated 0–4 count with `fresh`/`unavailable` and the Runner identity. Offline, unauthorized and malformed results never become zero. Multi-Runner partial aggregation and timestamps are not implemented.
2. **Implemented for one Runner:** a lightweight, localized indicator (`0`, `N active`, `Unavailable`) with an accessible label. Page addresses, profiles, tab titles, cookies, logs and CDP endpoints are excluded. Partial multi-Runner status remains future work.
3. Add **Close all** only for exact, currently known WebCodex-owned Browser identities. Require confirmation showing selected Runner, count and possible interruption of active automation. Route **each** close through the existing BrowserControl authorization and Runner ownership checks; never expose direct process termination or skip the canonical specialized governance. Exclude extension-shared, user-owned or ambiguous instances, even if they appear in observation. Do not interpret `close_browser` as permission to close arbitrary browsers.
4. On confirmation, use a bounded per-Runner worklist and stop/mark uncertain effects on connection loss or unclear completion. Report known closed / known failed / unknown outcome separately. Refresh after the operation rather than optimistically setting the active count to zero. No implicit retry on uncertain outcomes.

## Required focused tests before enabling the button

- Multiple visible Runners: a failed/offline Runner produces partial/unavailable state, never a misleading zero.
- Missing observe permission, missing control permission, and stale client selection all fail closed.
- List responses with truncated IDs, ambiguous ownership or unsupported Runner capability cannot authorize Close all.
- User cancellation causes no Browser effect; user-owned and extension-shared browsers are never passed to the close operation.
- One successful close followed by a failure or unknown outcome reports accurate partial results without retrying the uncertain close.
- A Browser closing independently during confirmation does not cause a different Browser to be targeted.
- No production default, idle reaper or unattended Desktop background poll changes.

## Authority boundary

The read-only adapter is now established: Tauri `WorkspaceRequest::BrowserStatus` posts to the authenticated Server endpoint, which invokes the canonical `BrowserObserve` gateway and returns only the count. It does **not** grant a `BrowserControl` capability or establish Browser ownership for closing. Any future Close all action requires separate design, explicit consent and the focused tests above; a direct Tauri-to-Runner kill command remains out of scope.
