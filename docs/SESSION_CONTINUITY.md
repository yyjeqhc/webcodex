# Continue across accounts and delegate model reviews

[English](SESSION_CONTINUITY.md) | [简体中文](SESSION_CONTINUITY.zh-CN.md)

WebCodex keeps project work independently of the ChatGPT account carrying the
conversation. A new conversation can discover an authorized Workflow Session,
read its saved task, decisions, progress and validation evidence, and explicitly
continue it. Configured model API providers can review that same saved context.

## Save context while working

Ask the active model to record important decisions and recovery-worthy progress
in the current WebCodex Session. For example:

> Keep the migration goal, agreed constraints, decisions and remaining work in
> WebCodex as we work. Before switching accounts, save a concise handoff.

The model uses `post_session_message` with `kind=decision` for agreed decisions,
`kind=progress` for implementation state and remaining work, and `kind=todo`,
`risk`, `question` or `guidance` when appropriate. A stable `delivery_key` makes an
exact retry restart-safe while its message/replay metadata is retained. An
uncertain keyed delivery must be reconciled with the same key and payload.

For a task already using a durable Goal, use `checkpoint_goal` at meaningful
boundaries. Keep the Goal explicitly associated with the exact Session. Execution
and validation tools keep their own evidence when an authorized recorder is
explicitly supplied; a message claiming “tests passed” remains a report.

WebCodex receives tool arguments and results, not every message in the host's
conversation. Unsaved discussion and private model reasoning cannot be restored.
Do not put credentials in notes. Retention and truncation remain explicit; the
saved Session is bounded project history, not an unlimited conversation archive.

## Continue from a new conversation

Connect the new conversation to the same WebCodex Server and authorized Project.
Then say:

> Continue the migration work in this project. Find my saved WebCodex Sessions,
> show the candidates if there is a choice, load the selected handoff, and check
> the current files and tests before continuing.

The recovery sequence is:

1. Discover the authorized Project with `list_projects` when its identity is
   missing. Call `list_sessions(project)`; optionally filter `lifecycle` or page
   with `offset` and `limit`. Only Sessions owned by the caller's authority group
   in that exact Project are counted and returned.
2. Select one exact `session_id` or returned `session_ref`. Ordering is only for
   display; a title, newest row or the word “continue” does not resolve an
   ambiguous task choice.
3. Read `session_handoff_summary(session_id)`; an explicit matching `project` is optional. The compact
   `handoff_brief.task.decisions` and `recent_progress` include the newest
   recorded notes, their status, supersession, and coverage/truncation. The
   default brief stays below 8 KiB. Use the diagnostic handoff or authorized
   `list_session_messages` for additional retained todos, risks or note detail.
   Recheck current Git/files and any stale test claims before acting.
4. For an Active Session, explicitly resume with
   `work_on_project(session_id, instruction, ...)` in default `checkout` mode. The
   authorized Session supplies its bound Project; an explicit Project must still match.
   Reuse the returned `project_ref` for ordinary Project tools, requesting
   `_wc.context=["project.instructions", "webcodex.workflow"]` when this
   model context needs them. A Closed Session can be read for recovery but cannot
   be reopened; start fresh work and explicitly carry forward the selected
   context instead.

When the original Window is available, `_wc.context=["workflow.resume"]` can discover
its authorized candidates with `session_ref` and an optional current `project_ref`.
It does not select a candidate, resume work, or establish implicit recorder state.
Fresh work and `mode="worktree"` still require an explicit Project source.

Discovery and handoff are reads. They neither select a task automatically nor
create, resume or close one. Omitting `work_on_project.session_id` starts fresh
work. Goal recovery is separately authorized; explicitly select among multiple
Goal candidates rather than guessing.

Changing the ChatGPT account can preserve this workflow when both connections
use the same canonical WebCodex authority and Project permissions. A different
WebCodex user, shared-key authority or project grant cannot read another owner's
Session merely because it reaches the same Server. If the operator intentionally
uses separate identities, transfer a user-reviewed summary through an authorized
channel; this feature does not widen grants or import private host transcripts.

## Delegate to a configured API model

The operator installs the [model API ACP adapter](../integrations/model_gateway/README.md)
and configures one Runner provider per fixed API/model/credential audience.
Python 3.10+ is required. Multiple providers support OpenAI Responses and OpenAI
compatible chat completions. Tokens stay in the Runner's explicitly mapped
environment. Provider configuration and credentials are not model-visible.

The MCP client must already have `coding_agent:run` and `project:write` authority
to start a Run; saved-context access additionally needs `runtime:read` and the
source Session's Project/owner permissions. Existing OAuth clients are not
widened on upgrade. Provision the intended client through the operator's normal
authorization/consent path before using this flow.

Ask the active model to discover the exact Project's `coding_agent_providers`
through `work_on_project` or `list_runners`, then explicitly choose a configured
`provider_id`. A tool call can supply the saved context directly:

```json
{
  "project": "<authorized project selector>",
  "provider_id": "model-review",
  "idempotency_key": "migration-review-1",
  "instruction": "Review the migration design and remaining validation. Return concrete findings.",
  "context_session_id": "<selected session_id or session_ref>",
  "timeout_secs": 120
}
```

These are `coding_agent_start` arguments. `context_session_id` requires
independent Session and `runtime:read` authorization and must match the exact
delegated Project. It quotes the bounded handoff and separately authorized Goal
context into the prompt; it neither resumes that Session nor chooses a recorder.
This snapshot does not fetch current files or Git. Include explicitly selected
source excerpts in `instruction` when the review needs them; the combined prompt
must fit the existing 64 KiB Run input bound.

Observe the returned Run with `coding_agent_observe`; omit the token on the first
read to include retained output, then use that observation's token for only-new
follow-ups. Use `coding_agent_cancel` to request cancellation. After an
uncertain start, observe the same Run rather than dispatching a replacement.
Context belongs to the initiation fingerprint: changing the saved snapshot while
reusing a key fails with a conflict. There is no automatic quota fallback or model
switching.

The adapter sends the supplied text to the operator-selected endpoint, streams
bounded model text, and accepts success only after a normal completed response.
It performs no file access, code changes, model tool execution or conversation
storage. Credentials must be valid for the configured API; account login alone
does not establish that entitlement. After reviewing the result, explicitly save
the useful findings as a Session decision/progress note if they should survive
another account or model switch. A delegated model's claim is not native
validation or approval evidence.

The existing [ACP Run contract](agent/acp-coding-agent-run.md) owns authorization,
durable dispatch, observation and cancellation. MCP exposes this flow through the
canonical gateway.
