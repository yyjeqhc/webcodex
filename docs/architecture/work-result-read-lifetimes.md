# Work Result read lifetimes

This change targets an observed defect, not a generic UI rewrite. Before the
change three deterministic App tests failed: refreshing Window activity while a
detail was in flight left the replacement expanded row empty; success still wrote
to a collapsed row; and failure feedback went to the detached row instead of the
replacement. The old Set-based pending marker represented an operation, but gave
later readers no way to receive its result.

## Actual owners

`activity-details.mjs` owns detail reads by exact Project, Window and trace. Its
read callback can only invoke the existing detail tool; validation remains at the
App boundary. Each current expanded DOM view can join the same promise and receives
success or failure only while still connected, open and in the same scope.

`read-cache.mjs` is shared by three existing readers: activity details, frozen file
diffs and working-tree file diffs. It owns cached values and pending promises,
deduplicates an exact key, and fences cache writes and pending cleanup by generation.
A replaced read cannot erase a newer same-key request. Failure is not cached and
never initiates a retry; another explicit UI action may retry.

Frozen diff keys include Project/Session/snapshot/path; working-tree keys include
Project/view generation/path, and the request still waits for an exact shared
snapshot before asking for a diff. Existing identity checks, byte/line limits,
canonical error presentation and fail-closed malformed frozen responses remain.
Each file row separately fences DOM updates by display mode, local epoch,
connectedness and folding state. Successful hidden reads may populate the same
scope's cache, so reopening does not issue another request.

This is logical invalidation, not a claim to abort transport reads on collapse.
The mounted App retains its existing Host request deadlines and teardown rejection
of pending transport promises. Teardown disposes these read owners and releases
cached content; they cannot start new reads or accept late results afterward.
No new worker, timer, event bus, dynamic plugin or general service registry is added.

## Deliberate limits

Full-text previews already own bounded page accumulation, four-file retention,
local display epochs, source fences, and explicit same-page retries. They are not
forced into the immutable-result cache. Collaboration/reference UI retains its
working draft, delivery-key and Host-acknowledgement logic. Neither area is rewritten
merely to finish a checklist. Synchronous section renderers still receive no RPC.

## Delivery

The source modules are embedded by the existing deterministic sections build;
no browser dynamic imports or additional resource requests are introduced.
Work Result HTML advances from v23 to v24. Other App URIs, canonical tool contracts,
permissions, Session/Job truth and Rust execution remain unchanged. This PR is
independent of Job/Git display ownership and startup/context dependency changes.

Focused tests cover joining, failure, reset/replacement cleanup, disposal, the
three reproduced activity races, and both frozen/live diff folding and teardown.
The existing shipped-App DOM/message-order suite is run without weakening its
assertions. A successful suite is not a live Host/desktop deployment guarantee.
