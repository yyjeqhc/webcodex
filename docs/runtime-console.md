# Runtime Console navigation

Open `/runtime` and connect with an existing runtime credential.

The default **Work** page opens **Activity**, with a Window list and a single
workspace. Filter Windows by Project or search by activity, Project, Runner, or
Window identity. The selected Window displays a chronological tool-call stream,
without a Session selector or a separate context sidebar.

Every returned invocation has its own row, including repeated observe and
diagnostic calls. Running and completed calls share start-time order, oldest
first. Each row shows the tool name, status, start time, and duration (elapsed
time for running calls). A Project path tag appears only when the call itself
names a Project whose path is available; Session relations never supply a tag.
There are no per-call technical disclosures or Session links. The Window view
also does not fetch Session details or messages.

The console requests up to 2,000 retained calls. When the response is truncated,
a visible notice explains that earlier calls are not available in this view;
the stream does not imply that pruned history is recoverable.
**Goals** remains available in the Work switch; returning from another destination
preserves the chosen Work surface.

**Projects** groups a primary checkout and its managed worktrees into one project.
On desktop, select a project on the left and inspect its activity, workspaces,
and retained Sessions on the right. Narrow screens stack the list and details;
selecting a project moves focus to its details. Initial loading fetches the
project list, Window inventory, and the selected workspace's Session list.
Git branches are checked only with **Check branch** for the selected workspace.
Opening a Session resolves its authorized Window links and opens the same
Activity workbench used by **Work**, focused on that Session. If multiple Windows
are linked, choose one; if no link is retained or Window observation is unavailable,
the dialog offers **View Session record** explicitly. A matching Project alone is
never used to infer a Window link. A Window omitted from the bounded inventory is
loaded by its exact key, and unavailable targets never silently select another
Window. The selected Window remains visible in the sidebar under **Current Window**
when omitted by the inventory or filters, including a loading/unavailable state
before detail is known. Sidebar and detail show the same short Window identifier;
selection scrolls into view without scrolling the whole page. If the inventory
later includes the target, its entry appears once in the normal list.
Session details load when opened, not to populate counts in the project list.

**Runtime** contains **Overview** and **Agents**. Overview puts Runner availability
and running work first, with disconnected or protocol-mismatched Runners at the
top. Build identity is available under the collapsed **Build diagnostics** section.
**View activity** opens the canonical **Work / Activity** workbench; Runtime no
longer maintains a second Window browser. Agent inventories load when **Agents**
is opened.

**Agents** opens the selected Agent's **Inbox**, showing pending messages and
connections. Connecting this browser explicitly enables inbox reading,
acknowledgement, and sending as that Agent; it does not start a model. An inbox
message can open its conversation, and acknowledgement uses the exact attached
Endpoint generation. **All conversations** is the account-visible shared list,
not a list inferred from the selected Agent. Participants and optional recipients
are selected by name. Sending defaults to the human identity; sending as an Agent
requires its attached browser connection. **Profile** holds editable identity
and specialty fields, with revision, generation and lease evidence under
**Technical details**. Connection counts and pending messages are not execution
or task completion indicators.

Failed Window refreshes identify retained observations as stale. Switching
Windows never displays the previous Window's calls under the newly selected
identity. Conversation detail has one cancellable loading path so a late response
cannot replace the newly selected conversation's messages.

Adapter `_meta["openai/session"]` is hashed into a `ClientWindow`, separate from
explicit `wc_sess_*` Workflow Sessions. This view never exposes the raw host value,
tool arguments or outputs, and cannot observe model reasoning or determine whether
a host UI is frozen. Window/Session links are many-to-many observations: they do
not select a Workflow Session, grant Project authority, or imply ownership.

For ordinary non-streaming `tools/call`, Window activity can project three timing
facts from canonical adapter timestamps: `service_ms` is request-observed to
response-handoff time, `next_call_gap_ms` is response handoff to the next
meaningful same-Window/same-principal request, and `cycle_ms` is request start to
the next meaningful request start. The gap is explicitly outside-WebCodex time;
it may include network, host scheduling, model inference, user interaction, or
other unobservable work and is never presented as model think/reasoning time.
Status/discovery calls and MCP App controller/polling calls (including Goal Plan state and Agent continuation bind/state/acquire/prepare/finish/recovery/unbind traffic) remain ordinary Window-seen evidence but do not count as meaningful business activity or break the meaningful sequence. Model visibility is not the classifier: real read/search/edit/Git/process/validation/work calls remain meaningful. Overlapping calls
are classified as overlap rather than producing a negative serial gap. Streaming
handoff is not stream completion, and a Server restart loses process-local prior
completion state, so both cases leave ordinary next-call timing unavailable.
A meaningful request consumes its predecessor at arrival; cancellation and hard
timeout also break continuity until a later eligible response establishes a new
anchor.
Legacy ActionAudit `window_ended_at_ms` remains the audit-record boundary and is
not used as the HTTP response-handoff performance timestamp.

When the experimental Code Mode feature is enabled, one outer `code_mode_exec` Window activity may also show a bounded composition projection: nested call/success/failure counts, maximum nested in-flight concurrency, Code Mode duration, input/returned bytes, nested raw-result bytes, and counts for the explicit admitted nested tool names. These fields come from the same outer ActionAudit row. Nested canonical calls do **not** create synthetic Window activities, and the projection never includes JavaScript source, nested arguments/results, paths, queries, commands, credentials, arbitrary error strings, or raw Window identity.

These are presentation changes. Workflow Session and durable Agent identities,
credential scopes, and mutation handling keep their existing contracts.
Model-reported progress remains informational. Collaboration message status
distinguishes a saved message from one included in a tool result; projection
alone does not establish that the recipient received or read it.

The Windows list and detail routes require `runtime:read`. Non-admin callers are
first principal-filtered and then re-projected through current canonical Project
authority, so revoked Project access cannot leave a Window timestamp, Session
count, gap count, timing projection, or direct-key existence oracle. Session detail itself keeps its
existing Project-read contract; without `runtime:read` it reports Window activity
as unavailable instead of elevating the whole Session read to a runtime-wide
permission requirement. Durable terminal history is backed by ActionAudit, while
currently-running requests are process-local and intentionally disappear on
Server restart. The dedicated Windows refresh is three seconds only while that
view is selected and the page is foregrounded; the normal Runtime Console refresh
cadence is unchanged. List and detail polls run independently and skip resources
with a request already in flight, so a response slower than the polling interval
is not repeatedly canceled. Returning to a visible/focused page refreshes idle
requests immediately. Active Session refreshes likewise preserve slow requests.
Window relations use the returned evidence directly instead of fetching each
linked Session solely to enrich a Window count.

These are bounded polling views, not lossless real-time event subscriptions. The
WebUI reads the latest 80 calls every 3 seconds in the foreground (15 seconds in
the background), and hydrates up to 2,000 retained calls at entry and every
30 seconds. If a new primary page no longer overlaps the previous page, history
catch-up starts immediately; an in-flight history request is allowed to finish
before the next catch-up. Truncated history and omitted running calls are marked.
A Session filter deliberately shows only related calls; choose **All calls** for
the whole Window.

The MCP Work Result card displays up to 200 completed calls and 8 active calls,
individually, in start-time order. Exact tool names identify calls; trace IDs only
reconcile a running call with its completed record. Foreground polling is normally
2.5 seconds and background polling 12 seconds. After one minute without changes,
idle polling backs off to 10/30 seconds; after 30 idle minutes it pauses until
**Refresh**. Observed active calls keep polling, including long-running calls.
Failed polling visibly marks the snapshot as stale. Host suspension, network
latency, retention/response bounds, and bursts larger than a retained page can
still prevent every invocation from being displayed. App-internal refresh calls
are intentionally excluded, and Code Mode nested calls remain composition
summaries rather than separate Window calls.

The Session board is not a mirrored host chat transcript. An observed ACK records
an explicit model-context acknowledgement, not a reply, read receipt, or completed
work. Model replies appear only when explicitly posted to the Session; reported
progress is separately labeled as informational. An empty latest-message card
means no Agent message exists in the currently retained window.
