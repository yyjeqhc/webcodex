# Issue #994 stage three: the authorized Server Diagnose entry and its CLI consumer

Status: **implemented Server/CLI contract**. The earlier Runner-only phases remain
in the [historical diagnostics report](../contribution-reports/issue-994-acp-provider-diagnostics-design.md).
This entry does not establish native Desktop or complete Issue #994 acceptance.

Follow-up to the merged Runner-side primitive (#1129). That PR shipped the
typed Runner fence (`CodingAgentRequest::Diagnose`), the presence-only
`env_from_env` projection, and the bounded ephemeral live ACP probe. This
round adds the missing middle: one canonical Server/ToolRuntime entry that
independently authorizes the exact Runner, provider generation, and
authorized Project before forwarding the typed request, plus the first
presentation surface (a CLI operator command). Desktop can reuse the same
entry later without new authority semantics.

## The entry

`diagnose_coding_agent` is a model-visible runtime tool next to
`observe_coding_agent`, with the same static authority shape:

- static OAuth policy `Require(coding_agent:run)` for presence-only observation;
  the optional live probe adds the runtime start gate below;
- effect `Observe`, approval `None`, idempotency `PureRead`;
- Runner capability `CodingAgentRuns` required.

The resolution order inside the entry is fail-closed and ordered so nothing
is observed or launched before its authority is established:

1. provider id validation and principal binding;
2. for `include_live_acp=true`, `check_runtime_tool_scope` against the exact
   `start_coding_agent` scope policy, before Project or Runner observation;
3. Project resolution for the caller (`resolve_project_input_for_auth`);
4. exact Runner semantic view and `CodingAgentRuns` capability for the caller
   (offline or unauthorized → bounded `coding_agent_runner_unavailable`);
5. provider generation from the Runner's advertised inventory
   (`provider_instance_id` is looked up, never supplied);
6. for the live probe, a fresh
   `PermissionEvaluator::evaluate_resolved_required("start_coding_agent", …)`
   decision for that resolved Project, before dispatch. The decision (allow or
   deny) is attached to the result like any permission-bearing execution;
7. typed dispatch through the existing `enqueue_coding_agent` fence, which
   independently re-verifies Runner identity, provider generation, online
   window, and access;
8. bounded wait (45 s > the Runner's 30 s probe budget), response identity
   verification, and server-side bound re-verification of the payload.

Presence-only diagnostics do not require `project:write`. The live probe reuses
`start_coding_agent` admission, whose current OAuth policy requires both
`coding_agent:run` and `project:write`, plus the fresh per-Project permission decision.
That extra gate authorizes the ephemeral provider launch; static `Observe`/`PureRead`
metadata alone cannot authorize it. The probe sends no prompt or provider tool call.
The scope regression is in [auth/scopes.rs](../../src/auth/scopes.rs), and
`diagnose_live_probe_requires_start_authority_before_any_resolution` verifies that
`coding_agent:run` alone cannot launch it.

## Bound re-verification at the Server

The Runner already projects inside fixed budgets. The Server re-verifies the
response before projecting it, so a malformed or oversized Runner answer can
never reach a caller as a diagnostic: option/choice counts, identity byte
bounds, the stable `select|boolean` kind vocabulary, the
"options only on success" invariant, and the environment-presence map bound
(`CODING_AGENT_MAX_ENV_MAPPINGS`, now shared with the Runner's ACP config
validation so the schema can never disagree with what was admitted). All
violations collapse into `invalid_runner_response`.

## Projection

The success payload is `project`, `provider_id`, `env_presence`
(destination name → boolean), and `live_acp` (the bounded
`CodingAgentLiveAcpDiagnostic`, `null` unless the caller authorized the live
probe). The provider instance id never crosses to the caller — it is the
Server-side identity anchor only, matching `safe_provider_inventory`.

## CLI consumer

`webcodex ops diagnose-provider --project <id> --provider <id>
[--include-live-acp]` calls the same authorized entry through the existing
`/api/tools/call` bridge with the operator credential. It renders presence
lines (`present` / `ABSENT`), the live option identities when requested, and
an ops verdict (`pass` / `warn` for absent env sources, `fail` for bounded
failures). Values, provider stderr, instance ids, and credentials never
reach the output.

## Deliberately unchanged

- The Runner fence, probe lifecycle, cleanup, and busy/timeout contracts
  (#1129) are reused verbatim; no second ACP client exists.
- `env_from_env` presence stays spawn-free; nothing about
  `include_live_acp=false` touches a process.
- No Desktop surface, no automatic retry, no configuration writes.

## Source and regression evidence

- [Canonical definition](../../crates/webcodex-tool-contracts/src/tool_definition/coding_agents.rs)
  owns observation metadata and the explicit additional live-probe authority.
- [Runtime](../../src/tool_runtime/coding_agent.rs) owns ordered admission,
  exact-generation dispatch and bounded response verification.
- [CLI consumer](../../crates/webcodex-cli/src/webcodex_cli/ops.rs) renders the same
  authorized tool result through `ops diagnose-provider`.
- [Runtime tests](../../src/tool_runtime/tests/coding_agent_diagnose.rs) cover scope
  denial before resolution, per-Project live-probe permission, full dispatch,
  stale provider identity, bounded disclosure and lost responses that may have
  launched a probe. The focused filter is `coding_agent_diagnose`; it is a
  reproducible regression scope, not a new live-provider acceptance claim.
