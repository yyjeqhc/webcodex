# Stable tool surface, optional workflow guidance

The model tool inventory is an integration contract, not a deployment preference.
Some Hosts cache tool descriptors and require an explicit schema refresh. Restarting
WebCodex or changing configuration does not update those cached descriptors.

## Deployment settings

Two optional Server environment settings are read once after the normal startup
environment-file loader. They can be set in the existing Server environment file:

```dotenv
WEBCODEX_GOAL_WORKFLOW=on_demand
WEBCODEX_MCP_APP_RESUME_MODE=user_confirmed
```

`WEBCODEX_GOAL_WORKFLOW` accepts `on_demand` (default) or `preferred`.
On-demand ordinary coding, review and multi-step work creates no Goal just because
Goal tools are available. Explicit user-requested durable plans, handoffs and
already-established exact Goals remain supported. Preferred recommends a durable
Goal when substantial work benefits from one; user intent and scope still win.

`WEBCODEX_MCP_APP_RESUME_MODE` accepts `unknown` (default), `user_confirmed` or
`unattended`. This is an operator declaration for this deployment, not protocol
capability detection. Unknown and user-confirmed never claim confirmation-free
continuation. Unattended must be selected only after separate deployment-level
verification; it neither bypasses Host confirmation nor guarantees scheduling.
Invalid settings fail startup with allowed values, without echoing supplied data.
These settings are independent from direct/host_code_mode orchestration timing.

No live configuration API, polling, automatic Host detection, or hot reload is
introduced. Change settings and restart the named Server through the ordinary
operator deployment procedure. **No Host tool-schema refresh is needed for these
policy changes**: names, ranks, Direct/gateway admission, tool descriptions,
input/output schemas and App resource associations do not depend on the settings.
An actual future tool-contract change still requires the Host's refresh process.
The preceding taxonomy/Direct-reduction commit is such a separate contract change;
this policy layer does not remove its one-time refresh requirement.

## Two distinct refreshes

1. Host schema refresh registers tool contracts. Policy selection never requests it.
2. Guidance refresh obtains current instructions as ordinary tool-result data.
   An active model conversation may retain old guidance even after a Server restart.
   On announced policy change, missing/stale instructions, or context recovery,
   request `_wc.context=["webcodex.workflow"]` on an already-needed ordinary call.
   Do not poll for policy changes or assume a restart changes retained model context.

The existing `webcodex.workflow` entrypoint returns the current runtime policy.
Its payload uses the existing workflow schema and does not change permissions.
The canonical internal startup diagnostic continues to use the deterministic
on-demand baseline so source validation does not depend on deployment settings;
public model policy is delivered by the explicit context channel, not inferred
from a Session, project instruction fingerprint or a Host tool description.

Selected Goal work can then request `_wc.context=["webcodex.goal_workflow"]`.
This supplemental chapter is discovered in the workflow result, not appended to
every cached descriptor's example key list. The already-open, bounded context-key
string contract accepts it without a new tool, enum member or schema field.
The same context capability gate, deduplication and 20 KiB budget apply. Loading
it performs no Goal creation, Session selection, binding, acknowledgement or job
execution. Unknown keys on older Servers remain nonfatal/unsupported.

The base workflow contains only selection and existing-obligation reminders.
Creation/checkpoint/controller/Wake recipes live in the optional chapter. Static
recommended flows describe how a selected workflow works, not when every task
must create one; current context owns the recommendation.

## Continuation truth and durable boundaries

An MCP App binding proves a message channel, not that confirmation is unnecessary.
The existing Agent `production_auto_resume_available` and Job terminal
`automatic_resume_available` booleans are conservative projections: an MCP App
needs the explicit unattended declaration **and** its existing exact live,
authorized binding. Push adapters keep their own independently declared capability.

Unknown/user-confirmed does not remove the App, withdraw bindings, block an
explicit manual follow-up, or rewrite a pending delivery. Dispatch acceptance,
Wake consume and fresh-turn observations retain their existing distinct meaning;
a consumed Wake does not prove that the user did not click a confirmation.

Changing preference never completes/cancels a Goal, rotates an Endpoint, changes
its generation, resends a Wake, acknowledges a message or retries a Job. Existing
Goal/session correlations and closeout obligations remain queryable. No database
migration, new scope, permission shortcut, schedule, or inferred identity is added.

## Regression boundaries

Tests cover both Goal preferences and all three interaction declarations. They
compare cached MCP inventories and full schemas byte-for-byte, exact manifest
routes and App bindings, unchanged canonical outputs outside requested context,
optional chapter capability/budget handling, explicit Goal lifecycle recovery,
and conservative Agent/Job readiness without modifying durable records.

Run focused workflow, context, Agent continuation, Goal and Job terminal tests;
keep the existing tool-list size/contract checks. No end-to-end claim about the
Host's current confirmation UI can be made by these local regression tests.
