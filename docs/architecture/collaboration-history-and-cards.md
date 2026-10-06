# Durable collaboration history and Work Result presentation

## Observed failure mechanisms

The former Operator and model-reply write paths deleted all but 512 messages per
principal. Traffic from another Window could therefore delete this Window's old
conversation. Peer transport used the same cap. SQLite persistence alone did not
make these transcripts permanent. The WebUI separately read only the newest 50
messages and replaced its visible transcript on every refresh. The Work Result
card showed only six messages, without access to earlier history.

These are implementation findings and deterministic regression fixtures, not a
claim that a particular production record was recovered. No production database
was rewritten. Old records deleted by a previously deployed build require an
existing backup; this migration cannot reconstruct them. ChatGPT's own transcript
is not automatically imported, and legacy Workflow Session messages remain in
their separate Session surface.

## Ownership and bounds

`webcodex-store::window_history` owns the read path. Operator messages and model
replies remain in their existing durable tables. Peer messages move to an archive
in the same transaction as delivery-queue pruning. The archive cannot resolve a
live peer route or project model attention. The existing latest-512 Operator
attention bound remains independent of permanent history.

Six indexed source pages are merged under one SQLite read snapshot. Each is
bounded to `limit + 1`, with `limit` at most 100. Returned message JSON is additionally
bounded to 128 KiB; oversized pages stop at whole-message boundaries, never cut
message bodies. Pagination uses the exact
`(created_at_ms, message_id)` order and a message cursor verified inside the exact
principal/Window namespace. It does not use shifting offsets. New messages and
restart do not move the older-page boundary. Bodies, replies, archive rows, cursors
and receipt data remain subject to current authorization; history never grants a
new recipient, credential audience, Project or execution authority. Inbound peer
history continues to omit the sender's Project/Session context.

The WebUI and App keep at most 200 DOM messages. Loading history is explicit;
newest-slice polls do not erase a reader's older page. A new history namespace is
never merged into an old one, and stale asynchronous replies are discarded.
Persisted history consumes disk proportional to messages: there is no silent
age/count deletion. An eventual export/delete/retention feature must be an
explicit operator action, not attention-queue housekeeping.

## Collaboration presentation

The card distinguishes You, this Window, and peer directions; shows intent and
priority; quotes exact reply targets; and separates delivery observations from
actual replies. ACK is only model-context acknowledgement, not acceptance or
completion. The composer exposes Guidance, Question, Note and Request, retains
the full payload/key after uncertain sends, and releases after a valid receipt
without waiting for a history read. IME composition does not submit on Enter.
Unchanged message/body nodes survive refreshes and receipt changes.

## Why another card may appear

`present_work_result` is the ordinary model-facing presenter. The MCP adapter also
advertises `work_result_thread_panel`: a Host thread entrypoint with an empty input
that resolves a previous successful Work Result binding in this exact authenticated
Window. It is not a new Task, Workflow Session, Goal or Job. Its descriptor now
explicitly directs models to `present_work_result`; both titles identify Work
Result. The initial `Task` subtitle and unavailable-state Task-card wording have
been removed from the Work Result template.

`present_goal_plan`, `present_agent_continuation` / `wait_for_agent_events`, and
`present_job_terminal_continuation` intentionally have separate resources. Ordinary
`work_on_project`, edits, checks, Job observations and closeout do not attach a
Work Result or Task template. Descriptor tests cover this boundary. The protocol
has no generic `tasks/get`, `tasks/update` or `tasks/cancel` implementation.

Without the exact displayed card or a successful trace, these entrypoints and the
old `Task` placeholder explain possible sources, not the identity of every card a
Host might render. This investigation did not retry the reported reconnecting
`read_tool_trace` path or claim that Host behavior was repaired.
