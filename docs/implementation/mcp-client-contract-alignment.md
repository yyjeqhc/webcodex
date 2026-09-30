# MCP client contract alignment

## Scope

Implemented from special `/root/git/webcodex` at `afcae932461ad190920b73ffd136ec701bc9c84f`.
The observed live Server/Runner ran `c3cffe936bb8`; source tests do not deploy the
new behavior. No Codex checkout, user configuration, service, public tool name,
Direct inventory, authority or execution-timeout policy is changed here.

There are three independent observations in an end-to-end client investigation:

| Path | What must be observed |
|---|---|
| ChatGPT App | The App/Host's loaded callable and actual returned result |
| Codex through Apps | The Apps descriptor plus Codex's loading/Code Mode representation |
| Agent configured with MCP | MCP discovery plus that client's schema conversion and result delivery |

A `tool_manifest` is Server discovery, not Host tool registration. Prefer a
loaded direct callable; use Host-native discovery when that Host has deferred
it; use the declared gateway fallback when the callable is genuinely unavailable.
Never manufacture Host-prefixed names from a canonical name. A generic gateway
can mutate, so it cannot advertise `readOnlyHint=true` merely to obtain parallel
execution. Dedicated read/wait callables retain their existing annotations.
The new tests verify the Server side of this boundary, not private Apps caches
or a particular version of a client's live model context.

## Equivalent numeric fences

A real direct call in this task failed before execution because the adapter
serialized `expected_read_revision=8535043794784493` as
`8535043794784493.0`; the same integer through the generic gateway succeeded.
Read and structured edit/delete/rename inputs now use one deserializer accepting
positive integral JSON numbers up to 2^53-1, including an integral floating
representation. It never rounds a fractional parsed number, clamps an unsafe
integer, converts a string, substitutes a fence, or accepts missing required
edit guards. Null, zero, fractions, negative values and values above the safe
integer domain remain invalid. Serialization returns canonical integer JSON.

This is representation tolerance, not weaker revision semantics. Runtime still
resolves that exact revision for the authorized Project/path/Runner snapshot;
stale, wrong-path and unknown revisions do not authorize edits. Precision already
lost in a client before JSON transmission cannot be recovered by the Server.
The schema's integer domain and safety limits are unchanged. No legacy edit
alias is restored; `edit_project_files` remains the only current model name.

The upper-bound regression initially exposed a second issue: the default
serde_json float reader decoded `9007199254740991.0` as `9007199254740990`.
The contracts crate now enables the existing `float_roundtrip` parsing feature
so downstream Server JSON parsing preserves representable values. No crate
version or new package is added. The max-value regression remains mandatory;
we did not remove the boundary case or accept a nearby revision as equivalent.

## Result consumption contract

Use the representation promised by the actual adapter, without guessing success
from a transport status or parsing the English text acknowledgement:

```js
// A raw MCP CallToolResult, including Codex Code Mode MCP results:
const canonical = mcpResult.structuredContent;
// An adapter that already delivers WebCodex's ToolResult directly:
const canonicalFromAdapter = toolResult;
```

In each case, business success is `canonical.success`, data is `canonical.output`,
and business failure information includes `canonical.error` and structured
recovery fields. Applications using a generic JSON-RPC transport first unwrap
its `result` (or handle its protocol-level `error`). This is not an invitation
to build a heuristic multi-format unwrap around every call.

Standard MCP business failures continue to use `isError=true`. The existing
explicit OpenAI adapter presentation keeps structured failures accessible as
values and `success=false` remains authoritative. Compact `content.text` is not
the data channel. Text-only integrations must explicitly enable the existing
text-JSON compatibility mode instead of requiring all clients to receive the
same object twice.

A successful edit consumes `changed` and returned file revisions; it must not
require a removed duplicate `state_changed`. A dry-run or failure can still
carry `state_changed`/`would_change` because those fields distinguish evidence.
`execution_state=pending` is the same execution with an exact Job continuation,
not execution failure, retry authority, or a new task. Only mechanically_followable
continuations qualify as predetermined next calls; fallback_recovery is retained
until details/recovery or a real dependency requires it. JSON-safe numeric fences
and continuation arguments are copied, not reinterpreted by the model.

Actual HTTP tests exercise successful edit and dry-run receipts through direct
and gateway routes, legacy/modern protocol and ordinary/OpenAI presentation.
Existing failure and single-Job pending/wait/observe tests remain authoritative;
there is no second execution harness or mock success-only decoder.

## Per-request waiting policy evidence

Enabled metadata/full trace now adds one `mcp_request_policy_selected` event under
the existing Server trace identity, after protocol/header validation and Window
resolution, before tool dispatch. It includes:

- `selection.effective`: profile, Host budget, initial handoff, maximum synchronous
  wait, and continuation slice selected by the canonical resolver;
- separate `profile_source` and `budget_source`: `deployment` or `request_header`;
- parsed optional `requested_budget_secs` and `deployment_budget_secs`, so clamping
  is observable without retaining raw headers;
- the existing Server-local elapsed milliseconds and known method/tool/Window.

Malformed policy headers still produce the existing request-policy error and no
successful-selection event; raw private header values are never copied. An
explicit header equal to the default is distinguishable from omission. Omission
never inherits a previous request, client brand, credential, Project or Session.
Trace-off captures nothing. This small diagnostic does not enlarge normal tool
results, change authority or infer whether the Host actually supports Code Mode.

A configured HTTP MCP connection may select `X-WebCodex-MCP-Profile` and
`X-WebCodex-MCP-Budget-Secs` where its client supports headers. Apps connections
have a separate configuration boundary; a local MCP setting is not assumed to
reach them. `guidance_profile` selects advice, not request waiting policy.

The 5-second Host Code Mode and existing direct defaults are unchanged. A
45-second readiness request still has its own bound and remaining Host budget;
it is not a five-second polling prescription. To explain a slow outer call,
combine this receipt with existing received/dispatch/Runner/handoff timestamps
and the client's observed duration. Do not subtract unsynchronized remote
clocks or attribute unobserved transport/model delay to Runner execution.

## Shared bootstrap budget

When an instruction material precedes an explicitly requested workflow, its
body projection now reserves the exact canonical workflow bytes plus bounded
omission receipts for later keys. Output order stays the caller's order. The
workflow is generated once; instructions reuse the authorized bootstrap snapshot.
No additional provider I/O, implicit Session, or extra context key is introduced.

The 20 KiB total budget stays unchanged. Large instructions can be shortened more
to preserve both requested materials; source fingerprints, scope, truncation and
project-file continuation remain explicit. Runtime-global files do not gain an
arbitrary filesystem read continuation. This does not promise all instruction
bodies fit, nor that arbitrary earlier catalogs can never exhaust the envelope.
Budget-insufficient or unauthorized source observations remain explicit failures
of that material, not evidence that the model has read complete rules.

## Evaluation boundary and reproduction

`scripts/eval_coding_loop.sh` uses `/api/tools/call`; its calls and counters now
use `edit_project_files`, retaining the no-effect `state_changed=false` assertion
in the failure/recovery case. It is an API mechanics harness, not a live MCP/Apps
or paid-model benchmark. Historical report fixtures can keep historical names;
changing their evidence to current names would falsify history. Current metadata
trace files can supply Runner observations; full raw payloads are unnecessary.

Both baseline/guided now bootstrap through `work_on_project`, since the old
`start_session` tool is retired. Baseline keeps its manual closeout and guided
uses `finish_coding_task`; compare historical startup call counts only with this
fixture change recorded. Sourcing the script exposes its actual helpers without
starting services. Three bounded shell-helper regressions verify bootstrap,
editor counting and API encoding. The complete service-starting evaluation was
not run in this change, so other historical end-to-end assumptions are not claimed
as revalidated.

Focused commands:

```sh
cargo test --locked -p webcodex-tool-contracts --lib read_revision
cargo test --locked -p webcodex --lib client_contract
cargo test --locked -p webcodex --lib request_policy
cargo test --locked -p webcodex --lib bootstrap_budget
cargo test --locked -p webcodex --lib response_tests
bash -n scripts/eval_coding_loop.sh
python3 -m unittest scripts.tests.test_eval_coding_loop_contract scripts.tests.test_agent_loop_report
```

## Validation performed

The final related Rust check chain completed with exit 0:

```sh
cargo test --locked -p webcodex --lib client_contract
cargo test --locked -p webcodex --lib
cargo test --locked -p webcodex-tool-contracts --all-features --lib
```

The full default Server suite was run because `float_roundtrip` affects shared
JSON input parsing, not only the new fence visitor. The final contracts suite
passed all 275 tests. Earlier focused policy/bootstrap tests passed (7 + 1), and
both new numeric parser regressions passed including the maximum safe integer.
The Python evaluation/report suite passed 75 tests; shell syntax, final Rust
formatting, whitespace and the 21-package workspace boundary checks passed.
Selections overlap and are not additive unique coverage counts.

The new adapter dry-run fixture initially contradicted the real receipt contract
by claiming one applied change and a minted revision. It was corrected to zero
applied changes and no new revision; production dry-run validation was unchanged.
The maximum-number test initially exposed the serde_json parsing error described
above; it remains in the passing suite. No failing tests were removed or ignored.
The final implementation stayed fixed during the successful validation chain.
No live Codex/Apps model run, production deployment, native Windows/macOS suite,
full-workspace/all-features runtime test or latency benchmark was performed.

A later live comparison must hold model/prompt/repository/Server/Runner versions
and execution target fixed, record each connection route and effective policy,
and separate cold discovery from warm calls. Test loading, a bounded batch read,
a revision-guarded edit, pending→same-Job observation, failure recovery and bounded
read continuation before drawing performance conclusions. This patch does not
start paid Agents, alter real client settings or claim that a private Apps cache
has refreshed. Native PTY/stdin parity is a separate test scenario, not implied
by durable Jobs.
