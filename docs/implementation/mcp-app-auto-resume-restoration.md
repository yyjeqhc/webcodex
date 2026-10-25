# MCP App continuation restoration acceptance

[PR #1028](https://github.com/yyjeqhc/webcodex/pull/1028) restored
`present_agent_continuation` and `present_job_terminal_continuation` as
model-visible Direct tools with Continuation reason. Durable waits remain
Gateway tools. This procedure is the dogfood/publication gate for future changes
to continuation behavior or Host integration. It does not itself establish Host
readiness or authorize deployment.

The Agent View uses `ui://webcodex/agent-continuation/v24`. A valid ref-only
presentation input waits for the initial Server result to resolve the exact
identity; the ref itself never grants identity or authority. This fixes the
previous initialization teardown when the Host delivered ref-only tool input
before the result. Complete tuple inputs still work, and invalid selectors or
conflicting resolved identities still stop coordination. The View also retains
the presentation-reference fence: changing `~ac1` to `~ac2` is a terminal conflict,
even before a Server projection arrives. Tests cover all six
initialization/input/result orders, including an initial `host_binding.bound=false`.
The [MCP Apps input notification](https://apps.extensions.modelcontextprotocol.io/api/interfaces/app.McpUiToolInputNotification.html)
allows `arguments` to be omitted. Such notifications preserve an already
selected identity or keep waiting for one, rather than tearing down the card.
Explicit malformed or conflicting selectors still fail closed.

## Local and Host evidence

Rust contract, MCP routing/admission and continuation regression tests validate
the local boundary. JavaScript tests validate the card state machines, including
duplicate and uncertain dispatch. They cannot prove ChatGPT scheduling.
`ui/message` acceptance is only dispatch evidence. Agent resumption requires a
fresh model turn and exact Wake consume. The existing Job protocol has no
consume operation: require its exact wait/Job delivery evidence and a fresh turn
that actually observes that Job instead. Neither a delivery ACK nor a displayed
message proves model execution.

Use an explicitly selected disposable test connection on the approved server.
Build the reviewed commit using the `dogfood` profile, record the existing
`git_commit`, `git_dirty` and `built_at` identity, and preserve the previous
binary/configuration before any approved replacement. Isolate state, credentials,
ports and Runner Projects from ordinary service data. Never expose a shared-key
test server without normal authentication. Refresh ChatGPT tool registration
and confirm both dedicated presentation descriptors and their App resources.
Do not route their presentation through `call_runtime_tool`.

Keep `WEBCODEX_MCP_APP_RESUME_MODE=unknown` for an unverified deployment.
Do not set `unattended` merely to make a readiness flag true. The existing App
acquire/prepare/dispatch path can be measured while policy is `unknown`; that
mode keeps the readiness declaration false without blocking this explicit
experiment. Run the complete matrix in that mode first. Only after measured
zero-click fresh-turn behavior may the test deployment declare `unattended` and
verify its readiness projection. Restore its previous policy when the experiment
ends. Historical Host observations are in the
[continuation experiments](../agent/mcp-app-continuation-experiments.md).

## Real ChatGPT matrix

Run each case in a new test conversation with fresh identities. For Agent,
create one identity, rotate one Endpoint and present its exact ref/tuple. Arrange
a delayed Conversation message from an independently authorized sender to that
Agent. For Job, start one harmless delayed Job in a disposable Project, create
its exact durable terminal wait and present that still-waiting wait. Handle a
Job that already terminated in the current turn instead of presenting it.

After the presentation tool result, output a normal assistant final response
and ensure the original turn has finished. Do not click, send another user
message or use a current-turn wait to manufacture the later turn. Leave the
conversation visible for foreground cases; switch away for background cases.

| Card | Delay | Visibility | Required observation |
| --- | --- | --- | --- |
| Agent | about 30 seconds | foreground | zero-click fresh turn, exact Wake consume |
| Agent | about 5 minutes | foreground | zero-click fresh turn, exact Wake consume |
| Agent | about 5 minutes | background | zero-click fresh turn, exact Wake consume |
| Job | about 30 seconds | foreground | zero-click fresh turn, exact wait delivery and Job observation |
| Job | about 5 minutes | foreground | zero-click fresh turn, exact wait delivery and Job observation |
| Job | about 5 minutes | background | zero-click fresh turn, exact wait delivery and Job observation |

For each case record UTC timestamps for original final response, trigger,
prepare, Host dispatch outcome, fresh-turn first tool call and exact Agent
consume or Job observation.
Record the source build, protocol, visibility, policy, correlation IDs and
whether any user action occurred. Verify the Agent turn calls
`consume_agent_wake` with the matching identity. For Job, correlate the exact
wait's one-shot delivery with the resumed turn's `observe_jobs` call for that
Job; do not invent a Job consume API or infer execution from `delivered` alone.
Observe a further polling interval and verify there is no second turn or
duplicated consume/delivery effect for the same logical event.

The coordinator can use the [bounded read-only collector](../agent/continuation-evidence-capture.md)
for exact Goal/Agent/Session Server metadata after resources exist. It records
partial capture explicitly and does not dispatch Wakes or replace Host evidence.

Retain only bounded, redacted evidence. Omit credentials, binding/claim fences,
consume tokens, raw automatic envelopes and private Conversation content.
`delivery_unknown` must never cause an automatic resend; use the established
explicit recovery path. Stale generations and foreign-owner calls must remain
rejected by their ordinary authority checks.

## Readiness and cleanup

For changes to continuation behavior or Host integration, the matrix is a manual
dogfood/publication gate. Record actual results against the exact candidate build
and deployment; report failures as Host observations without guessing their cause.
Do not declare such a candidate ready while any required case is missing or failed.
Automatic tests do not waive this gate. Test-only and documentation-only changes
use focused local validation and must not claim a new real-Host acceptance result.
Stop the disposable processes and restore the previous test configuration when
done; leave normal service and release state untouched.

## Goal hidden cadence baseline and candidate comparison

This is the user-assisted measurement gate for
[Issue #1159](https://github.com/yyjeqhc/webcodex/issues/1159), with historical
Host limitations tracked in [#1051](https://github.com/yyjeqhc/webcodex/issues/1051).
Use the same actual Host/browser, account, disposable connection, visibility
pattern and readiness policy for both builds. Record their exact commit and
build identities. Baseline Goal Plan uses a 60s hidden target; candidate uses
30s (resource `ui://webcodex/goal-plan/v10`, App 10.0.0) and Agent resource
`ui://webcodex/agent-continuation/v24` (App 24.0.0). The 300s inactivity threshold,
75s observation lease, 10s RPC timeout and Agent 15s hidden heartbeat are unchanged.
Changing binaries or restarting a deployment needs separate authorization; this
procedure grants none. Keep policy `unknown` until real unattended behavior is
established. Candidate timing records are diagnostic evidence, never authority.

Run three independent 300s inactivity trials on **each** build (six trials total),
with fresh Goal/Session/Wake identities per trial. Alternate baseline/candidate
where practical to reduce Host-time confounding. A trial starts at the Server's
last completed meaningful activity, then lasts at least 300s plus the observed
dispatch/fresh-turn interval. A missing fresh turn is a measured missing result;
record the observation end time rather than substituting a guessed latency.

1. On the authorized disposable connection, explicitly correlate one active Goal,
   its Session and its bound controller Agent. Present both exact cards and end
   the setup turn normally. Verify the loaded resource/build identities; stale
   cached cards cannot establish candidate behavior.
2. Enable each card's local timing ring and existing metadata tracing on the
   authorized test service. Capture the final meaningful completion timestamp.
   Hide the chat/browser in the same way on every trial. Record hide/show times,
   browser/window minimization, device sleep and any user interaction; do not
   interact during the inactivity interval or inject another message to create
   the follow-up turn.
3. After at least 300s, let the existing pipeline run. Record actual Goal poll
   gaps, RPC durations, observation age and remaining lease margin at attention
   reconciliation. Record the first eligible attention candidate, Event/Wake,
   prepare, Host dispatch, exact consume, and first meaningful work timestamps.
   Candidate Agent timing shows local `ui/message` start/completion/error and
   duration; baseline dispatch timing may require the existing server trace and
   manual Host observation. Mark unavailable fields as unavailable.
4. A human observes and records whether the Host displays a Send/confirmation
   modal, visible automatic message, or fresh model turn. Preserve a redacted
   screenshot or screen-recording reference and UTC timestamp. Do not infer
   absence of a modal or execution from API ACKs or server logs. If confirmation
   is needed, record it before deliberately confirming; the result is user
   confirmed, and its latency is separate from unattended latency.
5. Correlate exactly one logical Wake/Attempt and exact `consume_agent_wake`,
   then observe at least one further Goal and Agent polling interval for a
   duplicate. Check fresh-turn recovery of the exact Goal/Session and current
   work. Unknown delivery must never trigger a blind resend. A detached or
   expired observation cannot create attention until eligible observation is
   restored. Exercise visible/hidden transitions, teardown/remount and a
   separately authorized restart as additional labeled lifecycle trials;
   they do not replace the six comparison trials or the Agent/Job matrix above.
6. Save bounded redacted evidence, disable timing to clear the local rings,
   restore authorized test configuration and stop disposable processes. Rings
   retain only 64 events, so capture relevant records before they roll out.
   Never save tokens, raw automatic envelopes, credentials or private messages.

Report the three trial distributions and range for each build, with sample count
and missing outcomes. Separate inactivity-threshold → attention, attention →
prepare, prepare → Host ACK/error, and Host outcome → exact consume/first
meaningful work. Compare timer gaps and lease misses as well as end-to-end
latency; three trials do not establish broad Host reliability. A faster callback
with the same modal behavior is a narrower result than unattended continuation.
Do not claim improvement, zero-click readiness or gate completion from local
fake-clock tests. The required real-Host matrix remains a publication gate.

### Safe single-trial prompt and capture

The operator first verifies the existing test role, loaded resources and an
explicitly authorized controller setup in the real ChatGPT Web conversation.
Health, a connected Tunnel, an attached Endpoint or a readiness flag alone is
not acceptance evidence. If Host safety blocks Endpoint rotation or card
presentation, record that exact blocked step and stop the trial. Do not retry
it, substitute a gateway call, change the policy, or use another connection to
bypass the block. A blocked setup is not one of the six completed trials.

After the operator has supplied an existing test Project and exact authorized
controller identity, use this prompt, filling every bracketed selector explicitly:

```text
Run one harmless Goal-continuation measurement: [baseline/candidate]-[1/2/3].
Use only the selected existing test connection [connection] and disposable
Project [exact project selector]. The operator has verified build [identity],
Goal resource [identity] and Agent resource [identity], with readiness policy
unknown. Use only this exact authorized controller [Agent/Endpoint/generation].

Establish the exact Workflow Session with work_on_project. Prepare one new Goal
explicitly correlated to that Session and controller, with two steps: establish
this read-only observation, then verify a fresh resumed turn and close the Goal.
Completion means recovering this exact Goal/Session after inactivity, checking
one named existing harmless file [path], and recording the observed result.
Present the exact Goal Plan and Agent Continuation cards through their normal
Direct presentation tools, only while the ordinary Host permits those actions.
If safety blocks an action, stop and report the blocked step; do not retry it,
route around it, or change connections/permissions. If the supplied identity,
build or normal tool guidance cannot be verified, stop without guessing.

Complete the first Goal step and leave the second pending. Make one bounded
read of [path] with the exact Session recording correlation, then end this turn
normally with the trial label and exact Goal/Session references. Do not sleep,
wait in this turn, poll from the model, queue a scheduled task, send another
message or simulate resumption. The operator will hide the chat for the trial.

Only if the existing continuation path starts a genuinely new model turn,
follow its exact Wake/bootstrap/consume instructions, recover this Goal and
Session handoff, perform the bounded read, and checkpoint/complete this Goal.
Report that measured result once. Never resend unknown delivery or repeat an
uncertain effect. Do not edit source files, commit, publish, message anyone,
change settings, rotate credentials or operate services during this trial.
```

Before ending the setup turn, enable both local timing rings and the authorized
Server's existing metadata trace. Capture the final meaningful completion from
Server evidence as the quiet-start anchor; the assistant final timestamp is a
separate observation. Agree on one fixed visibility pattern and an observation
end limit for all six trials (for example 420s after quiet start, extending it
only to finish a dispatch already observed). Device sleep and user actions are
confounders and must be attributed. Do not click a confirmation during the
unattended measurement window. If a modal appears, first record its UTC time
and redacted image reference; any later deliberate confirmation starts a
separately labeled user-confirmed segment.

Capture before the 64-record rings roll over. Preserve only bounded projections:
local poll/RPC/Host timing records, metadata eligibility reasons and lease
margins, exact Event/Wake/Attempt linkage, and exact-consume/first meaningful
work timestamps. Keep a private local mapping of exact identities if needed;
public evidence uses consistent redacted labels. Use the existing protected
trace destination and a private evidence directory outside the repository.
Neither complete logs nor chat transcripts should be uploaded. Crop/redact
screenshots to exclude identities, automatic messages and private content;
never copy consume tokens, binding/claim fences or credentials. Correlate local
monotonic clocks through App call ids and Server request timestamps rather than
subtracting unrelated clock origins. Record unavailable fields explicitly.

When resumed, verify at least one further Goal/Agent polling interval without
a second Host dispatch for the same event. If no fresh turn occurs, end at the
agreed limit and record missing consume and work timestamps; do not invent
zero-click latency or count a manually prompted turn as automatic recovery.
The six trials, lifecycle evidence and original Agent/Job matrix remain separate
required gates. A narrower fix may reuse an existing matrix result only when
its exact unchanged runtime source/Host behavior and applicability are recorded;
new cadence or local timing evidence alone cannot waive a missing gate.

Copy this report template into the task's evidence when actual trials are run:

```text
Status: NOT RUN / PARTIAL / COMPLETE (no measured result is implied by this template)
Host/browser/version, device, UTC date:
Disposable connection/environment and authorization reference:
Readiness policy; metadata tracing mode:
Baseline commit/build identity; Goal/Agent resource identities:
Candidate commit/build identity; Goal/Agent resource identities:
Visibility pattern; terminal observation limit; confounding user/device actions:

Build | Trial | Quiet start UTC | Hide/show UTC | Observation end UTC
baseline | 1 | | |
baseline | 2 | | |
baseline | 3 | | |
candidate | 1 | | |
candidate | 2 | | |
candidate | 3 | | |

For EACH row record (unknown/unavailable fields must stay explicit):
- Poll target/actual/max gap; RPC duration; observation age/min lease margin
- Threshold, eligible candidate, attention/Wake, prepare timestamps
- Host dispatch start/end, elapsed, accepted/unknown/error; local/Server clock source
- Modal shown? Human-observed UTC/evidence reference; confirmation click if any
- Fresh-turn first tool; exact consume; first meaningful work timestamps
- Correlated exact Goal/Session and redacted Event/Wake/Attempt references
- Logical Wake/dispatch/consume counts; duplicate check interval and outcome
- Outcome: unattended / user-confirmed / unknown / missing / failed
- Redacted evidence references; unavailable data and reason

Per-build count, latency distributions/ranges, lease misses, modal/duplicate counts:
Lifecycle trials and original Agent/Job matrix results:
Acceptance condition → actual evidence or remaining step:
Measured conclusion and limits (never infer real Host behavior from fake clocks):
Remaining required trials/gates; cleanup performed:
```
