# MCP collaboration on CLI and Adaptive surfaces

## Problem and change

The CLI renderer discarded the Runtime's bounded `peer_awareness` field. CLI
and legacy Adaptive MCP also rejected every `_wc` envelope, so a client could
receive an ACK-required Operator or peer message without the advertised
invocation path needed to acknowledge it.

The renderer now retains peer discovery together with Operator messages, peer
messages and Session attention. A surface-aware projection of the existing
canonical invocation field policy admits only `ack` and `reply` on CLI and
legacy Adaptive. The stateless Adaptive invocation policy remains unchanged.
The same policy supplies discovery, exact CLI/legacy manifests and parsing;
business arguments reach the original Runtime parsers after the envelope is
removed.

`call_runtime_tool` takes `_wc` beside `tool` and `arguments`. The target
tool's own argument object does not carry transport metadata. Apps stay
unavailable on CLI, and control, recorder and context fields stay unavailable on
CLI and legacy Adaptive. Individually reviewed operations keep their original
direct-call exposure and authority policy.

## Delivery and authority

- Peer discovery is recent meaningful activity under the existing observation
  principal and an authorized shared Project. It does not assert online presence.
- The existing producer bounds, persisted discovery routes, deduplication and
  result-size rollback remain the owners of delivery. The CLI does not add a
  second cache, heartbeat or unbounded message channel.
- An explicit ACK echoes a message already retained by the model. Projection
  itself never updates `first_ack_observed_at_ms`. ACK is neither human
  acceptance nor evidence that work was executed.
- Operator replies require the exact current Window, principal and a previously
  projected message. A repeated identical reply reuses the canonical delivery
  key; another Window cannot reply to that message.
- Session ACK continues to use the authorized existing Session selection/affinity
  rules. These communication fields do not create a Session, infer a business
  Project or expand execution authority.
- Stable Window identity and peer authorization are separate. In particular,
  this change does not broaden the existing OAuth observation-principal domain
  across access tokens.

## Verification on MSI

The real HTTP adapter test
`src/mcp_tests/cli/collaboration.rs` exercises all four combinations below with
synthetic credentials and temporary databases. It verifies delivery, no implicit
ACK on display, explicit ACK, persisted reply, idempotent replay and rejection
from a different Window.

| Protocol | Surface | Result |
|---|---|---|
| Legacy MCP with transport Window | CLI text | Pass |
| Legacy MCP with transport Window | Adaptive structured | Pass |
| Stateless MCP with OpenAI Window metadata | CLI text | Pass |
| Stateless MCP with OpenAI Window metadata | Adaptive structured | Pass |


The Adaptive fixture requires a real object output schema from discovery and
validates the actual structured reply against it. A different Window attempts
ACK and reply in separate HTTP requests before the valid recipient's ACK; both
must leave the original message unacknowledged and create no reply.

A second HTTP fixture, `src/mcp_tests/cli/peer_clients.rs`, composes the
[verified Window resolver](../agent/session-model.md) with discovery, delivery
and explicit ACK:

| Synthetic client shape | Protocol and response surface | Window source |
|---|---|---|
| ChatGPT | Stateless MCP, Adaptive structured | `_meta["openai/session"]` |
| Codex | Stateless MCP, CLI text | `_meta.threadId` |
| Claude | Legacy MCP, CLI text | `X-WebCodex-Client-Window` |

All three use the same verified API-token principal and one authorized Project.
The test obtains their Window/source/principal from real HTTP audit, then seeds
meaningful Project observations through the canonical audit writer. Discovery
and deduplication use authorized `list_sessions` calls; message delivery and ACK
use ordinary `list_jobs` calls. Messages travel ChatGPT → Codex → Claude →
ChatGPT, and a foreign principal cannot send into those retained routes.

No-ACK discovery and message delivery may be omitted by the existing 40 ms
optional budget. The fixture allows up to 12 ordinary HTTP observations, with
a three-second deadline checked between attempts, and fails immediately on
business errors. It accumulates
the expected peers and rejects repeated discovery, while checking that message
display never writes an ACK timestamp. Posting and explicit ACK each happen
once; the ACK must be durable immediately. The fixture neither expands the
production budget nor inserts discovery routes directly.

A separate projection matrix retains job identity, output, continuation,
Operator/peer/Session messages and discovery across completed, pending, failed,
timeout and outcome-unknown results for `run_job`, `run_process`,
`run_shell`, `run_script` and `observe_jobs`.

Validation on the verified ClientWindow branch plus this change:

- `cargo test --locked -p webcodex --lib mcp::tests:: -- --test-threads=8`:
  **474 passed, 2 existing ignored**, 21.15 seconds of test execution.
- Compiled test binary filter `peer_collaboration`: **14 passed**, including
  same-Project discovery, principal isolation, retained-route requirements,
  one-shot delivery, ACK repetition, budget expiry and oversized-result rollback.
- CLI discovery at the same 17-tool fixture: **36,294 → 39,112 bytes**. The extra
  2,818 bytes advertise the communication contract. The existing 40 KiB default
  CLI discovery budget remains unchanged and passes across exposure settings.
  Repeated envelope prose is conveyed once in initialize instructions instead
  of on every CLI descriptor.
- Canonical business-schema checks still compare every business constraint after
  separately validating the optional closed communication envelope. Bounds and
  authority assertions were not relaxed.

The pre-change discovery measurement used the preserved executable built from
`93d5911587afbb75b6ecf61051e79583ff59597e`; the after measurement used the
new test executable on the same MSI host. These are transport and Runtime
fixtures, not an assertion that a real Claude/Codex/ChatGPT model will choose to
echo an ACK. A UI may correctly continue to show awaiting ACK until the client
actually sends one.

## Reproduce

Run the focused CLI, invocation and permission regressions with:

```text
cargo test --lib mcp::tests::cli
cargo test --lib mcp::tests::invocation_envelope
cargo test --lib peer_collaboration
cargo test --lib mcp::tests:: -- --test-threads=8
```

No Server or Runner deployment/restart is required to run these tests.
