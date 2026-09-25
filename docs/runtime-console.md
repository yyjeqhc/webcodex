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

**Projects** browses authorized workspaces and their retained Sessions.
**Runtime** contains **Overview**, **Window Activity**, and **Agents**.

In **Window Activity**, the left list puts Windows with active requests first,
then sorts by recent activity. Each row shows the hashed Window identity,
last observed Project, request count, and recency. Hovering
on a timestamp reveals the absolute time. The last Project is a historical
observation, not the attribution for every call in the Window.

The selected Window uses the same individual tool-call stream as **Work**.
Failed refreshes identify retained observations as stale. Switching Windows
never displays the previous Window's calls under the newly selected identity.

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
Model-reported progress remains informational.

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

The Session board is not a mirrored host chat transcript. An observed ACK records
an explicit model-context acknowledgement, not a reply, read receipt, or completed
work. Model replies appear only when explicitly posted to the Session; reported
progress is separately labeled as informational. An empty latest-message card
means no Agent message exists in the currently retained window.
