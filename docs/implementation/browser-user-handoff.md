# Browser user handoff

[中文](browser-user-handoff.zh-CN.md)

User handoff protects an exact managed visible persistent Browser while a person
completes login, MFA, consent or another manual step. It does not automate or
bypass those interactions. The Runner must advertise `browser_user_handoff` and
resolve the exact native surface; unsupported platforms/ambiguous windows fail
before creating a hold. Ephemeral, headless and externally attached Browsers are
not supported by this first version.

## Workflow

1. In the existing authorized Workflow Session, post an open `question` or `todo`
   explaining the required manual step. Keep its exact message id.
2. Call `control_browser(action="begin_user_handoff", client_id, browser_id,
   session_id, message_id)`. The Session owner and Browser/Runner permissions are
   checked independently. Retain the returned `handoff_fence`, exact surface and
   remaining expiry. Duplicate begin for the same association neither renews the
   hold nor starts another handoff.
   Begin requires `browser:control`, `computer:read` and `runtime:read`. A current
   principal-scoped Session ref resolves to its exact Session before owner checks.
3. Ask the user to perform that step and reply when ready. The hold protects idle
   lifetime for at most ten minutes, capped by remaining maximum Browser lifetime;
   it never auto-renews. Observations, surface resolution and `close_browser` remain
   possible. Page effects, including `close_page`, are denied. Native Computer
   effects are also paused while any handoff exists because focus, pointer and clipboard input are
   shared across the desktop. The user's native input is unaffected.
4. On the user's explicit reply, call `control_browser(action="finish_user_handoff",
   client_id, browser_id, session_id, handoff_fence)`, then obtain a fresh Browser
   snapshot. Computer input remains paused until this first successful recovery
   snapshot (or explicit Browser close). Each page that existed at finish needs its
   own fresh snapshot before page effects resume. Old Browser elements, Computer
   elements and display snapshot input generations cannot be reused. Observe the
   actual outcome; the user's reply and Session ACK do not prove login success,
   resolve the todo, or grant permission.

## Expiry and recovery

Expiry removes idle protection; it never silently unpauses a surviving Browser.
Ordinary idle/max-lifetime cleanup can close that exact Browser and revoke its
identities, releasing the native input reservation. Close, shutdown and Runner
replacement also revoke the hold. A Server restart cannot create a new hold from
a retained question. After a lost receipt, observe the same Browser's handoff
summary before deciding whether to finish; never blindly replay effects or adopt
a replacement Browser. An unavailable/stale finish needs fresh observation.
After finish, the same Browser/Session/message association cannot begin again.
Each Browser retains at most 64 completed associations; use a new attention message
for a new manual step, or explicitly close and replace the Browser at capacity.

## Owning implementation

The Server's [Browser gateway](../../src/tool_runtime/browser_tools.rs) authorizes
the Session/message association independently from Browser authority. The Runner's
[Browser adapter](../../crates/webcodex-runner/src/webcodex_runner/browser.rs) resolves
the exact native surface and bridges the
[Supervisor handoff state](../../crates/webcodex-browser/src/supervisor/user_handoff.rs)
to the Computer input reservation. The
[handoff tests](../../crates/webcodex-browser/src/supervisor/tests/user_handoff.rs)
cover duplicate begin, expiry, per-page recovery snapshots and native reservation
release; [Server tests](../../src/tool_runtime/tests/browser_user_handoff.rs) cover
the independent authorization boundary.

`close_browser` is the explicit shutdown escape path. A deadline, a Session ACK,
a local Computer resume or a page-close request cannot substitute for exact
handoff finish and recovery observation.

## Acceptance and limitations

In an isolated registered test project on a supported native host, launch a
managed visible Browser, post a question and begin handoff. Confirm that Browser
and Computer writes are rejected while observations work. Perform a harmless
manual page change, explicitly continue, take a fresh snapshot and resume with
new element ids. Also close the Browser during handoff and verify that the old
fence cannot target a replacement.

Controlled-clock/backend and native-input-gate tests run without a live login.
Real macOS/Windows surface behavior and ChatGPT human interaction require separate
manual acceptance; Linux capability denial is not evidence of native UI support.
