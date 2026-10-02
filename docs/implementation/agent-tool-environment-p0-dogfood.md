# Agent Tool Environment Bridge P0: dogfood record

Date: 2026-10-02. Host: `special`, Linux. Source workspace:
`/root/git/webcodex`, branch `feat/817-agent-tool-environment-p0`, based on
`c2cd4f4877273f73d5592524b117e3bc6ebe25d1` (merged v0.5 surface standardization).
Tests were run against the implementation working tree before its local commits;
this was not a production deployment. No push, PR, merge or fleet service restart
was performed.

## Environment and isolation

The actual runtime was Node `22.23.2` with pinned
`@earendil-works/pi-coding-agent@1.0.0` and `@earendil-works/pi-ai@1.0.0`.
Dependencies were installed under `plugins/agent-environment/node_modules`, with an
initial independent API probe under `/tmp/webcodex-agent817`. The new package's
`.gitignore` excludes its installed dependencies; only its lockfile is committed.

Each native integration fixture creates a temporary Project, Pi agent directory,
MCP configuration and bridge state/transcript directory. The loopback E2E also
creates disposable Server users/tokens and a separate websocket Runner. It uses
the repository's existing local test-service harness and `target/dogfood` binaries,
not the operating WebCodex service. User Pi configuration, model credentials and
other hosts are not modified.

Chappie `/root/git/chappie` at `6f380df` (v1.1.0) was read as an architectural
reference. **Chappie itself was not installed or executed in the dogfood.** The
actual tested path is WebCodex's native Plugin adapter plus the installed Pi SDK.
The older local Pi checkout was not used as evidence for Pi 1.0 behavior.

## Executed native acceptance

`npm --prefix plugins/agent-environment run test:pi` passed its parent test and
nine subtests (10 reported tests). The tested path is real Plugin stdio -> actual
Pi local provider -> Pi's native execution -> `turn_end` results.

| Case | Observed result |
| --- | --- |
| Fixed outer catalog | One `projectBound` `agent_environment` tool, not one Plugin definition per native tool. |
| Builtins | Real `write` creates a temporary file, `edit` changes its text, `read` returns that text and `bash` returns actual process output. A persistent native transcript is created. |
| Extension | A native extension tool is discovered and called; `tool_call` and `tool_result` hooks record the native call identity. |
| Permission/schema | Native hook blocks a selected argument value; malformed arguments produce native validation errors, not bypassed execution. |
| MCP | A real local MCP stdio fixture is loaded by Pi; `mcp__fixture__echo` is invoked through the bridge and passes native hooks. |
| Exposure | Hidden/inactive tools stay unavailable; codemode/deferred tools retain native orchestration routes. An active model-only tool works as a native model call and is rejected from native codemode. |
| Batch | Two native calls retain exact IDs/input order and independent success/error states; one intentional failure does not erase the successful result. |
| Dynamic catalog | A new `.js` extension is installed in the isolated Pi directory and native `refresh` loads it with the same native session ID. Old schema bindings fail before dispatch. |
| Isolation | User-style settings and an LLM credential sentinel remain unchanged/unforwarded. Caller `_meta` is rejected, not used to bind the native session. |
| Deadline | An expired deadline does not start a call. A deliberately uncancellable native tool crosses its deadline, returns `outcome_unknown`, performs its delayed effect exactly once, and cannot be replayed through the quarantined bridge. |

Pi 1.0 observations that changed the implementation: automatic extension scanning
accepts `.js`/`.ts` rather than `.mjs`; bare SDK initialization omits CLI builtin
MCP/codemode/tool-search factories; effective model tool declarations live in
transcript system-message changes and are read using public `getCurrentTools`.
The two read-only version-specific compatibility seams are isolated and pinned.

A separate real-Pi cancellation probe also passed: create a managed native Pi
session, describe `write`, then invoke with an already-aborted `AbortSignal`.
The result was `not_started/cancelled_before_dispatch`; no file or native tool
history existed, and the request token was not consumed. Submitting that same
still-unused request without cancellation then wrote the file exactly once.
The probe used only a disposable temporary directory and cleaned it up.

## Full WebCodex transport acceptance

Reproduction:

```sh
cargo build --profile dogfood -p webcodex --bin webcodex-server \
  -p webcodex-runner --bin webcodex-runner
python3 scripts/e2e_agent_environment_ws.py
```

The final full-chain rerun rebuilt the actual Server and Runner binaries from
the final code, exited successfully, and retained local diagnostics under
`/tmp/webcodex-agent-environment-e2e-l9kfon6p`. It verified:

- Real MCP -> WebCodex Server -> websocket Runner -> native Plugin -> Pi builtins,
  extension and MCP tool, native permission hooks, partial batch and native
  codemode/deferred calls.
- Exact Project/Runner targeting; foreign user, missing Project scope and wrong
  Project rejection before Pi startup; model-visible tool names stay unchanged.
- Direct MCP and generic Runtime provenance, with no outer `openai/session`
  forwarding or retargeting.
- Same-session native extension refresh and retained provider identity on an
  unchanged **general Runner config reload**.
- Native pre-dispatch deadline and post-dispatch uncertain effect without replay.
- Explicit Plugin/provider recreation invalidates outer/native session bindings.
  With an actual Runner five-second deadline, native Pi `bash` writes a start
  marker before sleeping; the Runner reports `outcome_unknown`, retires the
  provider and does not execute the marker write a second time.

The final run also verified current `allow_patch` revocation at the same Project
path before Pi startup, plus live provider add/remove using generation-fenced
Runner configuration. A removed provider's old binding was rejected as
`not_started`; adding/removing providers did not change global MCP tool names.
All six reported E2E acceptance groups passed, including these added regressions.

`--keep` retains **disposable local test credentials** in its diagnostic directory.
Do not commit or share those directories. The default command removes them, and
the harness closes its disposable Server/Runner processes in either mode.

## Automated regression evidence

| Suite | Result on the affected implementation |
| --- | --- |
| `webcodex` all targets | 3,214 passed; 3 existing ignored tests not executed. |
| `webcodex-runner` all targets, `runner-real-process-tests` enabled | 989 passed; 64 ignored tests not executed. Includes current write revocation, stale target/schema and busy-provider tests. |
| `webcodex-core` all targets | 321 unit tests and one integration test passed. |
| `webcodex-runner-registry` all targets | 317 passed. |
| `webcodex-tool-contracts` all targets | 278 passed. |
| Plugin SDK | Typecheck, build and complete test suite passed. |
| Agent environment bridge | 19 unit tests passed, including a 2,048-tool catalog, paging, exact schemas/IDs, hidden tools, partial failure, cancellation, quarantine and bounded output. |
| Real Pi native Plugin integration | 10 reported tests passed. |
| CI path-risk tests | 39 passed. |

The root run includes `mcp_tools_list`, `model_surface`, tool-definition/surface,
Plugin/MCP gateway and specialized permission regressions. Initial failures in
case-sensitive catalog descriptions and an exceeded serialized tool budget were
fixed by restoring the required wording and reducing description text: existing
test assertions and byte budgets were not weakened or increased.

Logs kept locally, without adding them to the repository:
`/tmp/webcodex-agent817-rust-complete.log` (root/Runner/contracts),
`/tmp/webcodex-agent817-rust-final.log` (includes successful core/registry suites),
`/tmp/webcodex-agent817-node-final.log` (bridge/Pi/path-risk).
The final successful build and transport logs are
`/tmp/webcodex-agent817-build-complete.log` and
`/tmp/webcodex-agent817-e2e-complete.log`.

Documentation-test commands for `webcodex`, `webcodex-core`,
`webcodex-tool-contracts` and `webcodex-runner-registry` also completed
successfully; each package reported zero doctest examples. This is not counted
as additional executed test cases. The local log is
`/tmp/webcodex-agent817-doctests.log`.

Final `cargo fmt --all -- --check`, explicit rustfmt checking of the included
contract test file, and `git diff --check` passed. The final process check found
no remaining Pi adapter or MCP fixture processes from these tests.

## Review and remaining scope

A separate post-implementation diff pass checked cross-layer authority, native
execution, boundedness, credential separation and replay. It found and corrected
write revocation at an unchanged Project path, busy-call rather than queue
semantics, initialization/reload quarantine, malformed-catalog repair/history,
native name preservation, long help-text bounds and new-package ignore rules.
This was a separate review pass by the implementing assistant, not an independent
human or external coding-agent approval.

P0 remains Pi 1.0.0/headless and Linux-dogfooded. It does not attach to an existing
Pi TUI, relay interactive confirmation widgets, implement another native agent,
or provide a WebCodex filesystem sandbox around Pi. Native resource output is
bounded rather than a complete renderer. Existing config changes replace the
whole provider set; individual unchanged members of a changed set are not retained.
Native refresh and general Runner config reload are distinct from explicit Plugin
reload, which intentionally restarts providers. Uncertain effects are never
replayed automatically.
