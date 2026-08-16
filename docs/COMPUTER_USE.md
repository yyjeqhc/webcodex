# Computer Use roadmap

This page tracks the implementation order for WebCodex Computer Use. It is a roadmap, not a duplicate semantic or authorization specification. Current authority, scope, retry, and privacy contracts remain defined by [Authentication](AUTH_MODEL.md#computer-observation-and-control-authorization), the model-facing tool definitions, and the Runner implementation.

## Current substrate

Computer Use is semantic-first rather than coordinate-first. The current model-facing path is:

```text
computer_list_targets
  -> computer_list_windows
  -> computer_accessibility_tree / computer_snapshot
  -> computer_control(press|focus)
  -> computer_input_text
```

The current contract deliberately keeps read observation separate from effects. Effects name exact opaque Runner/window/element identities, revalidate native state before acting, and never gain AppleScript, shell, paste, coordinate, or synthetic-input fallbacks. A response lost after possible effect dispatch is `outcome_unknown`; callers observe current UI state and reconcile before deciding whether another effect is safe.

The roadmap preserves these invariants while reducing unnecessary model round-trips. The primary optimization target is redundant observation, not collapsing distinct effects into opaque workflows.

## Near-term slices

### CU-4 — semantic element finder

Add a bounded read-only `computer_find_elements` model-facing tool over the existing Accessibility observation path.

First implementation should be a Control-side focused adapter over the canonical bounded Accessibility tree rather than a new Runner query protocol. Reuse `computer:read`, exact `client_id`/`surface_id`, and the existing `computer_accessibility_observe` capability. Return only deterministic bounded matches and fresh ephemeral `element_id` values already produced by the underlying observation.

Initial filters should remain closed and simple: role/subrole, literal semantic label text, focused, enabled, and limit. Do not add regex, fuzzy scoring, a query DSL, arbitrary AXValue search, or effect behavior. If dogfood later shows material transport/latency cost from fetching the bounded tree internally, the same semantics may be moved down to the Runner behind an additive capability.

Success criteria:

- Common known-element tasks no longer require the model to read and parse a full AX tree.
- Finder results preserve existing protected/secure-content behavior and tree bounds.
- Traversal order and truncation are deterministic and explicit.
- A stale result remains an ephemeral handle, not a durable bearer reference.

### CU-5 — exact window activation

Add one narrow effect that activates/raises an already observed exact `surface_id`.

The operation must revalidate the exact native surface and may not accept an application name, PID guess, executable path, bundle path, shell command, AppleScript, or fallback to another window. Lost-response behavior follows the existing Computer effect contract.

This is intentionally separate from application launch. It solves the observed failure mode where an exact element is known but its window is not frontmost without widening Computer into a process launcher.

### CU-6 — normalized element state and observation generation

Expose low-cost read-only state needed to choose the next semantic action without returning sensitive values. Prefer normalized WebCodex affordances over platform-specific AX vocabulary, for example:

```text
role / subrole
enabled
focused
value_empty
protected
can_press
can_focus
can_input_text
```

The same model-facing vocabulary should later be derivable from macOS AX and Windows UI Automation patterns.

Record an observation generation (and observation time where useful) with element registrations. Keep lineage/fingerprint re-resolution as the authoritative semantic stale check. Do not impose one aggressively short wall-clock TTL on all current AX effects merely to add a timer; future geometry/focus-sensitive effects may require a fresher generation than semantic press/text effects.

The first implementation uses a positive process-local generation per observed surface and does not add a wall-clock TTL or observation timestamp. `computer_accessibility_tree` and the Control-side finder expose the generation attached to their fresh element handles. `computer_element_state(surface_id, element_id)` re-resolves an existing handle and reports that same generation; it neither creates a new handle nor advances generation. `value_empty` is actionable only for supported unprotected text input and is `null` for protected/secure content or when unavailable.

Stale recovery should remain re-observation, normally:

```text
stale_element -> computer_find_elements(...) -> new element_id
stale_surface -> computer_list_windows -> new surface_id
```

Do not add a `refresh_element` operation that upgrades an old handle into new authority.

### CU-7 — region snapshot

Extend window snapshot observation with a bounded window-relative region and optional output dimension bounds. The Runner must first revalidate the exact `surface_id`, then capture/crop within that surface; caller coordinates must never be treated as global desktop coordinates.

The first version does not need caller-controlled image quality or a general screenshot backend. Prefer bounded system-selected encoding for model observation. Return enough metadata to correlate the image with what was captured, including source/output dimensions, captured region, MIME type, byte count, digest, and capture time when available.

The implementation keeps `computer_snapshot` as the single model-facing operation. Calls with only `client_id` + `surface_id` retain the existing `computer_snapshot` wire and `computer_observe` capability for rolling compatibility. Supplying `region`, `max_width`, or `max_height` switches internally to the additive `computer_snapshot_region` Runner wire and requires both the baseline `computer_observe` capability and the additive `computer_snapshot_region` capability, so an older Runner can still serve whole-window snapshots but fails closed for the new transform. Region coordinates remain in the revalidated surface coordinate space; the Runner maps them into captured pixels, crops inside the bounded image, applies aspect-preserving downscale only when requested, and keeps JPEG encoding/quality system-selected.

### CU-8 — snapshot to project artifact

Use a separate `computer_save_snapshot(project, path, client_id, surface_id, region?, max_width?, max_height?, session_id?)` operation rather than a `save=true` flag on observation. The Control reuses the exact CU-7 capture path and then sends the validated bounded image directly to the target project's existing artifact write path; the model never receives or resubmits the Base64 image body. Whole-window capture retains the existing rolling-compatible `computer_observe` path, while region/downscale capture still additionally requires `computer_snapshot_region`; CU-8 adds no new Runner Computer protocol capability.

The artifact write is create-only: callers cannot request overwrite, encoding format, quality, or arbitrary content. The target path is validated by the existing project-artifact policy, and the target Runner must still advertise `file_write` when the request is admitted under the registry lock. The source Computer Runner and target project Runner may be different devices.

This operation crosses Computer observation and project artifact authority and therefore requires both `computer:read` and `project:write`. Capture remains observational, but artifact persistence is an effect. A definite pre-dispatch rejection reports `not_started`; if a write may have been dispatched and its result is lost or inconsistent, the result is `outcome_unknown` and includes the exact project/path plus expected digest, byte count, and MIME. Reconcile with `read_project_artifact_metadata` before deciding whether another create-only attempt is safe; never blind-retry by overwriting the path.

### CU-9 — bounded scroll

Add scrolling as a distinct capability/effect after semantic observation and generation handling are established.

Prefer semantic scroll-to-element when the native accessibility backend supports it. A lower-level wheel fallback, if needed, must be bounded and tied to an exactly revalidated active/focused surface. Do not fold scrolling into generic `computer_control`.

## Next capability sequence

After the near-term slices are dogfooded, the expected order is:

```text
closed key input
-> Windows UIA parity
-> application discovery / bounded launch
-> full-display discovery and snapshot
-> coordinate pointer
-> clipboard
```

Key input remains a closed navigation/action vocabulary (for example Enter, Escape, Tab, arrows, paging/home/end and bounded modifiers); ordinary text continues through `computer_input_text`.

Windows parity should map UI Automation patterns into the same WebCodex semantic action/affordance vocabulary instead of exposing a second OS-specific model API.

Full-display observation receives separate authority from single-window observation because it widens the privacy surface. Pointer actions remain late because they depend on fresh snapshot generation, correct surface-relative geometry, DPI/display handling, and post-effect reconciliation. Clipboard read/write remains separately scoped and bounded.

## Explicitly deferred

Do not introduce these as shortcuts while implementing earlier slices:

- a natural-language `computer_do_everything` or generic automation command;
- implicit focus/send/paste/retry flags on existing effects;
- blind retry after `outcome_unknown`;
- arbitrary application/executable paths or shell/AppleScript fallback;
- global desktop coordinates for pointer or region input;
- a general plugin/callback framework for Computer backends;
- browser tasks forced through desktop coordinates when a future DOM/ARIA-native browser surface is the better substrate.

Browser-native automation may become a separate branch of Computer Use later, sharing high-level safety principles without forcing DOM/ARIA semantics into the desktop AX/UIA backend.

## Dogfood loop

Keep a small set of representative traces and use them to reorder work when repeated friction is demonstrated:

1. Edge: find an empty search/text field, focus it, input text, verify semantically.
2. Messaging app: activate an observed window, find a contact/search target, navigate to a conversation, enter a message, explicitly send, then verify.
3. Unfamiliar UI: start with semantic observation and use snapshot only when semantics are insufficient.
4. Stale element: recover by semantic re-observation without reviving the old handle.
5. Unknown effect outcome: observe and reconcile without blind retry.

Track interaction quality by observation economy rather than an absolute call-count target. A healthy happy path should not repeatedly fetch full trees or screenshots, while distinct effects such as focus, text input, press, activation, and later scroll/key input remain explicit when they have different outcome semantics.

For every new Computer feature:

1. define observation/effect semantics, stale behavior, timeout/lost-response handling, bounds, and explicit non-goals;
2. add only the capability/scope distinction required by the new authority surface, with missing old-Runner fields failing closed;
3. close the full Server/protocol/Runner/model projection before local hardening;
4. revalidate exact native surface/element identity at the Runner immediately before an effect;
5. keep sensitive screenshot/text bodies out of audit metadata;
6. test stale handles, replacement/races, permission revocation, protected/secure content, response loss, process restart, and platform geometry where relevant;
7. dogfood on macOS first for AX effects and on Windows as UIA/snapshot support becomes available.
