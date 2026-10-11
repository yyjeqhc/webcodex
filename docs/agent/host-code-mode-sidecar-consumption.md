# Host-native Code Mode: preserving WebCodex Sidecars

This document covers **ChatGPT Host Code Mode** (the JavaScript that invokes
WebCodex MCP tools and chooses which results to print). It does not change or
describe the separate, Server-hosted WebCodex nested `execute_code_mode` VM.

## Why this matters

A model-visible WebCodex tool can return a normal business result together with
optional post-result attention. The Server projects the attention into
`structuredContent.output`, but a Host Code Mode program can silently discard
it by printing only `stdout_tail`, `service`, or another business field.

Successful Server projection is **not** proof that the Host printed the message,
that the model retained it, or that the model acted on it. WebCodex cannot force
arbitrary Host JavaScript to preserve fields. The host-specific built-in
`webcodex.workflow.tool_strategy.guidance` therefore asks the caller to forward
nonempty attention before compact business output.

The four principal attention channels are:

| Result key | Evidence | Consumer action |
| --- | --- | --- |
| `session_attention` | Session message bodies, `ack_ref`, omission counts | Explicit `_wc.ack` or Session-only `_wc.ack_ref` on a later ordinary call; `_wc.resolve` to finish a non-todo |
| `operator_messages` | Window Operator messages and ACK receipts | `_wc.ack` (persistent Operator delivery suppression), optional `_wc.reply` |
| `peer_messages` | Window Peer messages and ACK receipts | `_wc.ack`, independent of Session `ack_ref` |
| `job_attention` | Opportunistic terminal/recovery changes | Interpret/reconcile exact Job; use `observe_jobs` only for additional evidence, not as an automatic heartbeat |

Other result sections may matter, especially requested `context_projection`,
`session_hint`, `workflow_recording_attention`, `window_reply`, `control`,
and `peer_awareness`. Do not turn success compaction into loss of instructions,
failures, uncertainty, permissions, or pending continuations.

## Host Code Mode projection recipe

For a result that may contain Sidecars, keep the raw result inside the cell and
print **nonempty attention before large business logs**. This example uses a
normal `get_runtime_status` call only as a small reproducible carrier; use the
same pattern for other ordinary model-visible tools.

```javascript
const raw = await tools.mcp__webcodex__get_runtime_status({
  compact: true,
  _wc: {record: "<existing-authorized-session-id>"}
});
const envelope = raw.structuredContent ?? raw;
const output = envelope.output ?? {};

function hasAttention(key, value) {
  if (!value || typeof value !== "object") return false;
  if (["context_projection", "window_reply", "control",
       "session_hint", "workflow_recording_attention"].includes(key)) {
    return true;
  }
  return Boolean(
    value.messages?.length || value.items?.length ||
    value.new_peers?.length || value.self_peer_id ||
    value.omitted_count || value.truncated ||
    value.ack?.accepted_count || value.ack?.ignored_count ||
    value.ack?.accepted_ids?.length
  );
}

// Prioritize messages and explicit state transitions, not a raw-result dump.
for (const key of [
  "session_attention", "operator_messages", "peer_messages", "job_attention",
  "context_projection", "window_reply", "control",
  "session_hint", "workflow_recording_attention", "peer_awareness"
]) {
  if (hasAttention(key, output[key])) {
    text(JSON.stringify({kind: "webcodex_attention", channel: key, value: output[key]}));
  }
}

text(JSON.stringify({
  kind: "business",
  success: envelope.success,
  error: envelope.error ?? null,
  service: output.service,
  version: output.version,
  connection: output.connection
}));
```

The `text(...)` output is what the model sees. Merely reading a Sidecar in
JavaScript, or incrementing the Server's projection count, does **not** mean it
was forwarded. Do not automatically generate `_wc.ack` inside the cell before
the model has retained the message. A later model-visible call should carry
the exact ACK IDs or the current Session-only `ack_ref` when applicable.

The example is deliberately small. For execution/edit/validation calls, also
project any `execution_state=pending`, exact `continuation`, `job_id`,
`observation_ref`, `read_revision`, errors, truncation, and outcome-unknown
information. Do not claim an incomplete result was successful.

If the Host output budget cannot hold required attention, **make the omission
explicit**, avoid pretending it was consumed, and use a supported exact recovery
path where available. A one-shot Peer notification without required ACK may
not be recoverable from the next tool result; a fallback cannot be invented.
Splitting large business logs into smaller projections is preferable to
discarding attention. There is no Server-enforced Host output guarantee.

## Reproducible A/B/C experiment

Use an isolated, already-authorized test Workflow Session and a unique marker
per message. No real work should be requested by these messages.

1. **Direct:** post a `post_session_message(kind="note", requires_ack=true)`
   with marker A; call an ordinary model-visible tool with `_wc.record` on
   that Session; observe `session_attention.messages`, then ACK the exact
   returned `message_id` on a *later* ordinary call.
2. **Host filtered:** post marker B; call an ordinary tool in Host Code Mode
   but `text(...)` only its business fields. The Host program can see B in
   the raw result while the model-visible output omits B. **Do not ACK B**
   merely because Server projection happened.
3. **Host forwarded:** on a later call with the same explicit Session, forward
   the attention fields as above. Verify B appears in model-visible output;
   only then send an ACK on a later call.
4. Resolve the exact diagnostic messages with `resolve_session_message`,
   then verify they are no longer open/projected. Keep separate message IDs
   and `delivery_key` values. Never use an unrelated person's Window.

Session ACK is **request-scoped**: if a subsequent call does not carry the
message ID or the valid Session-only `ack_ref`, an unresolved ACK-required
message may be projected again. ACK does not imply Resolve. Operator ACK has
different, persistent suppression semantics. Avoid treating retries, context
compaction, or missing ACK as proof the Host dropped a message.

### Evidence and interpretation

| Observation | What it proves | What it does not prove |
| --- | --- | --- |
| Server `first_projected_at` / projection count | Server attempted model-result inclusion | Host rendered it |
| Host `text(...)` contains exact marker | Marker entered the model-visible cell output | Model acted correctly |
| Later accepted `_wc.ack` | Model/Host returned retained-message evidence | Work complete |
| `_wc.reply` / durable Resolve / completed todo | Explicit follow-up action | Every prior message was understood |
| `job_convergence.passive_terminal_delivery_count` | Server projected passive terminal truth | Host rendered it or model used it |

For sf offline analysis, `window_operator_messages` and
`window_model_replies` provide message projection, ACK, and exact reply
correlation; `action_events.summary_json` contains
`model_ergonomics.invocation` and `model_ergonomics.job_convergence`.
Count **distinct message IDs and valid recipient/Session relations**, not raw
projection attempts as unique messages. Distinguish ineligible/no-subsequent-
activity windows from eligible opportunities. Host output filtering is not
observable from ActionAudit alone, and Server logs must not be described as a
model-context read receipt.

## Runnable fixture regression

Run `node --test scripts/tests/host_sidecar_projection.test.mjs`. It executes
this document's JavaScript example with fake structured tool results and checks
nonempty message forwarding, ACK-only and omitted-message receipts, attention
on business failure, and the empty-sidecar case. It is not a live ChatGPT Host
consumer test or evidence of automatic model ACK.

## Scope of this change

Only model guidance, tests, and documentation change. It does not add a
new MCP field, a runtime queue, ACK persistence, ToolRuntime authority, or
Runner behavior. A Server rebuild/deployment would be needed to serve the
updated built-in guidance to future clients; there is no Runner deployment
or tool Schema refresh for this prose-only change. Live Host compliance is
best-effort unless the Host itself implements an enforced forwarding boundary.
