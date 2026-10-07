# campus-application Native Tool Plugin

`campus-application` is a first-party WebCodex Native Tool Plugin for planning safe resume-form filling with WebCodex Browser Use. It turns bounded Browser semantic snapshots into structured fill plans, supports repeated and dynamically-added resume sections, and can advance explicit multi-step application flows. It never submits an application.

The Plugin is deliberately split from Browser execution:

- Browser observation supplies semantic controls and opaque element identities.
- `campus-application` maps those controls to a structured resume profile and proposes bounded actions.
- the caller executes ordinary Browser actions and takes a fresh snapshot after every structural or page-step mutation.

The normal native-field path is:

1. `observe_browser(action="snapshot", query={fields_only:true})`, optionally
   narrowed by `section` or `group`. Filtering happens before pagination. Forward
   the returned nodes and snapshot generation with the existing target/URL context.
2. `plan_fill` returns `batch`, ready to pass unchanged to `control_browser`,
   plus an opaque `plan_id`. It no longer emits duplicate per-field proposals.
3. Execute that one batch (1..32 operations).
4. Take one fresh query with the same scope and send its nodes/generation plus
   the **whole Browser output receipt** to `reconcile_fill`.

Reconciliation success is just `{"confirmed":20,"needs_attention":[]}` (37 ASCII
bytes). It returns no profile, confirmed fields, or semantic nodes. Values and
field identities stay in at most 32 provider-memory plans, each with at most 256
fields and a 15-minute lifetime. No fill plan is persisted. A plan token is
consumed once; expiry/restart requires fresh planning, never old-batch replay.
Unmapped or unsupported fields remain compact `needs_attention` entries with
existing `mapping_id` identities for teaching or disambiguating repeated labels. Use
`analyze_form` only for additional diagnostic detail; normal planning does not
duplicate those details.

For more than 32 fields, execute the returned batch, then reconcile once;
reconciliation issues the next batch with **fresh** element ids. A 65-field section
is 32/32/1, never 65 single-field calls. Completed stable batches can produce a
retry batch for visible mismatches. Confirmed fields leave subsequent plans;
missing, ambiguous, clipped, or invalid observations remain attention. Native
`set_value` replaces existing input/textarea values using the native setter and
input/change events. Older Runners admitting only `input_text` are used only for
observably empty native text fields; insertion must never append to a mismatch.

Partial, missing, `outcome_unknown`, or unstable receipts produce no automatic
retry batch. Known completed/unstarted counts are retained in exception reasons;
readback confirms current state, not execution certainty. Resolve the stopped
boundary and replan only the fresh exception fields. Navigation/replacement
invalidates old Browser element authority; changed target/URL or non-newer
readbacks are rejected. Expansion and page-step clicks stay separate and require
a fresh snapshot. Final submission always remains a review boundary.

**Call budget:** a 20-field native section uses 5 tool invocations: query, plan,
batch, query, reconcile. The provider SDK cannot invoke Browser. In code
orchestration, pass the readback directly into reconciliation within one outer
call and print only reconciliation: **4 model round trips**, 0 screenshots,
0 pointer operations, 0 per-field tool calls. Compared with query + plan + 20
individual writes + readback (23 calls), direct mode removes 18 calls (78%).
Compared with an already manually assembled batch (4 calls), direct mode adds
one reconciliation call but eliminates manual operation construction and the
full readback from model context when composed. Do not claim 4 primitive calls.

Both `plan_fill` and `reconcile_fill` take `client_id`, `browser_id`, `page_id`,
`url`, `snapshot_generation`, and `nodes`. `plan_fill` also takes `title` and
optional `mapping_hints`; reconciliation takes `plan_id` and `receipt` (the
Browser tool's output object, not its transport envelope). The receipt embedding
accepts Browser-owned extra diagnostics without reproducing that entire schema;
missing certainty/count fields never authorize continuation. Nodes without
`name` are accepted directly from Browser and normalized locally.

A code-mode readback composition can keep every successful node out of model
context (wrappers below denote the already admitted Browser/Plugin calls):

```js
const fresh = await observeBrowser({
  action: "snapshot", client_id, browser_id, page_id,
  query: { fields_only: true }, // reuse the same section/group filter if present
});
const delta = await campusReconcile({
  client_id, browser_id, page_id, url, plan_id,
  snapshot_generation: fresh.snapshot_generation,
  nodes: fresh.nodes,
  receipt, // previous control_browser output, forwarded without reinterpretation
});
emit(delta); // do not emit fresh.nodes
```

v0.4.6 was checked: its profile installation/schema-1 format remains supported.
These model-facing tool schemas are refreshed together; the old proposal output
is not retained as a second execution representation. Local mapping-memory format
is unchanged. Optional `personal.height_cm` and `personal.weight_kg` support metric
height/weight fields without inventing missing facts or converting other units.

Local validation includes a real isolated Chromium test:

```bash
cargo test --locked -p webcodex-browser chromium_batch_replaces_32 -- --ignored --test-threads=1
```

It proves one fields-only query, a 32-operation batch replacing prefilled native
inputs/textareas (including framework-style value tracking), and one semantic
readback; it also proves old ids cannot replay after readback. On 2026-10-07 this
passed locally. The connected `mini` Runner did not advertise Browser Bridge, so
no live recruiting-page Share was available through that connection. No real
application was submitted and no private data was used in fixtures.

## Native controls, widgets, locations, and dates

The planner uses only actions admitted by the current Browser snapshot:

| Field/control | Planned operation | Required readback |
| --- | --- | --- |
| Native text, textarea, date, month and structured inputs | `set_value`; legacy `input_text` only for observably empty text inputs | Exact semantic value (date/month writes use canonical ISO values) |
| Native select | `select_option` | Snapshot value exactly matches the planned option |
| Radio / radio group | `click` on one uniquely mapped target | Target `checked=true` or `selected=true` |
| Checkbox / switch / Boolean choice group | `click` only when an observed state differs | Explicit checked/selected state, including false |
| Resume attachment | `upload_file` | Selected filename |
| Custom combobox / select | `select_choice` with one `choice_path` step | Committed semantic value or selection |
| Cascader / location path | `select_choice` with 1..4 steps | Path with preserved segment boundaries; leaf-only evidence is unresolved |
| Custom date / month picker | `set_date` | Canonical `YYYY-MM-DD` or `YYYY-MM` |

Already-satisfied controls produce no duplicate effect. A radio/checkbox group
needs a unique target; ambiguous alternatives, missing checked state or disabled
targets remain `needs_attention`. Readback uses control state rather than nearby
text that happens to contain the requested word.

Native operations run first, including resume upload, in batches of at most 32.
Widgets run after that readback with fresh identities. `plan_fill` accepts
`widget_batch_limit` from 1 to 8, defaulting to 1. For a form whose sibling
identities have been demonstrated to survive widget changes, set this to 5 to
group three comboboxes, one cascader and one date picker. This is a bounded
caller choice, not automatic permission to retain stale ids: Browser still
checks every operation and stops on replacement, uncertainty or stale identity.
Each successful readback/reconciliation can return the next ready-to-execute
batch. Completed widget operations with insufficient semantic readback are
reported for attention and are never automatically replayed. The same rule
applies to completed native selects: Chromium may expose an option's display
label while the requested option was its underlying code. A code/label mismatch
is unresolved; the planner neither guesses that mapping nor repeats selection.

`personal.native_place`, `personal.household_registration` and
`personal.student_origin` represent different facts. Their respective `_province`, `_city`, and `_district`
fields can build explicit hierarchical paths. `contact.province`, `contact.city`,
and `contact.district` describe the contact location; generic labels such as
“省份 / 城市 / 区县” need their surrounding group to determine which fact they
belong to. An unambiguous separated location string can also supply a path.
Multiple preferred locations are alternatives, not hierarchy: choose an
explicit indexed resume path before treating one as the requested location.

Dates come from the profile, including birth, education start/end, expected
graduation, and experience/project periods. Recognized Chinese and slash/dot
forms normalize to ISO values. Month-only data stays month-only; a control
requiring a complete date receives attention when the day is missing.
A native `date`/`month` control keeps `set_value`; a custom picker uses
`set_date`. Invalid dates, including impossible leap days, are not proposed.

### Resume upload context

Pass `upload_source: { project, project_root? }` to `plan_fill` when the
profile supplies `attachments.resume_path`. A relative path needs the explicit
authorized Project. An absolute or `~/` path additionally needs its actual
Project root; the planner derives a relative path only when it stays inside
that root. The provider's profile directory never implies an upload Project.
Out-of-root paths, traversal and missing context produce attention. Browser
independently authorizes the Project and canonical file, including symlink
checks, before any batch effect.

### Mixed-form call budget

For 20 native fields, three custom comboboxes, one cascader, one custom date,
and one upload (26 fields total), `widget_batch_limit=5` produces:

1. Browser query, then Plugin `plan_fill`.
2. One 21-operation native/upload batch.
3. Fresh Browser query, then Plugin `reconcile_fill`, returning five widget operations.
4. One five-operation widget batch.
5. Fresh Browser query, then Plugin `reconcile_fill`.

This is **5 Browser calls + 3 Plugin calls**, with **26 field operations**.
Composing each readback directly with reconciliation gives six outer code-mode
invocations; that count describes this explicit composition, not a measured
language-model benchmark. A host can chain further deterministic dependencies
inside one outer invocation while retaining the same eight native tool calls.
No per-option model round trips, screenshots or coordinate-pointer operations
are needed.

With the default one-widget boundary, the same otherwise stable form uses six
batches: 13 Browser calls and 7 Plugin calls. Widening to five widgets is explicit
and was validated on the owned fixture; a live page that replaces sibling
controls still stops for fresh observation.

The Browser-only comparison fixture also measures the lower-level API benefit.
Its legacy path already batches the native fields and upload, then handles
custom options/calendar steps through successive observations and primitives.
The new path runs one query, one 26-operation batch and one readback. It is
deliberately separate from the Campus orchestration counts above. Timing
excludes Chromium startup and initial navigation; report actual measurements
from the test output, not an implied production-site latency guarantee.

On 2026-10-07, the actual Node-provider/Chromium fixture confirmed all 26 fields
with 0 attention in 2,926 ms, counting provider startup and the first query through
final reconciliation, excluding Chromium bootstrap. The Browser-only same-form
comparison used a two-level province/city choice and measured 27 calls / 13 snapshots / 34 primitives / 9,959 ms for the
legacy path and 3 calls / 2 snapshots / 26 primitives / 1,812 ms for the typed
batch path. Both used 0 screenshots and 0 coordinate-pointer operations. These
are local fixture observations; outer model latency was not measured.

Reproduce the actual provider/Browser integration after `npm run build`:

```bash
cargo test --locked -p webcodex-browser --lib chromium_campus_e2e -- --ignored --test-threads=1 --nocapture
```

After deploying a supporting Server and Runner, exercise the real typed API,
including individual actions, a mixed upload/widget batch, stale-id rejection
and no-submit/default-reset checks:

```bash
node plugins/campus-application/fixtures/browser-api-smoke.mjs \
  http://127.0.0.1:18081 mini-dogfood agent:mini-dogfood:webcodex /path/to/browser-scoped-token-file
```

The API fixture uses fictional data, starts its own loopback HTTP server and
owned temporary Browser, and removes its temporary upload file during cleanup.

## Install, build, and test

```bash
npm ci
npm run typecheck
npm test
```

Node.js 18 or newer is required. Repository CI uses the local `../../npm/plugin-sdk` checkout so this Plugin continuously tests the current WebCodex SDK.

## Configure a resume profile

The repository contains only fictional example data. Copy `profile.example.json` to a private Runner-local directory as `profile.json` and edit that copy, for example:

```text
~/.config/webcodex/campus-application/profile.json
```

`profile.json` is intentionally ignored by Git.

By default the Plugin reads `profile.json` from the provider's configured `cwd`. An explicit `WEBCODEX_CAMPUS_APPLICATION_PROFILE` environment variable may instead point to another profile file.

The structured profile covers identity/contact data, common Chinese campus-recruiting personal fields (gender, birth date, identification, ethnicity, political status, health status, native place, household registration, student origin, fresh-graduate status, and marital status), split province/city location fields, links, repeated education/experience/projects/campus experience, skills/languages, job preferences, application text, and attachments. Native place and student-origin/Gaokao-origin are intentionally distinct fields.

`attachments.resume_path` becomes a Browser `upload_file` batch operation using the explicit `upload_source` context above. It is not resolved relative to the Plugin profile directory.

## Configure one Runner

Build the Plugin, create a private data directory containing `profile.json`, then add a provider entry to the Runner's startup-bound `runner.toml`:

```toml
[[plugins.providers]]
id = "campus-application"
name = "Campus Application"
command = "node"
args = ["/absolute/path/to/webcodex/plugins/campus-application/dist/plugin.js"]
cwd = "/absolute/path/to/private/campus-application-data"
timeout_secs = 30
```

After restarting or reloading the Runner:

```text
webcodex plugin check --runner <runner> --plugin campus-application
webcodex plugin reload --runner <runner>
webcodex plugin describe --runner <runner> --plugin campus-application --tool plan_fill
```

Invocation remains on the canonical `plugin_tool describe -> call` path.

For real ATS controls whose Browser semantic snapshot has no usable label, `analyze_form` and `plan_fill` return a stable `mapping_id`. Callers can teach the structure with `mapping_hints` using a label or explicit canonical field/resume path; radio/checkbox hints may also carry `choice_value` so unlabeled choices can be taught once as values such as male/female or yes/no. Explicit teaching is kept in a site-scoped in-process cache and also persisted in the provider-private `mapping-memory.json` (override with `WEBCODEX_CAMPUS_APPLICATION_MAPPING_MEMORY`), keyed by site identity plus structure signature, so it survives Plugin reloads.

Newer WebCodex Browser snapshots may also attach bounded `form_context` to form controls: a stable field signature, DOM tag/type/name, placeholder/autocomplete, section heading, component hint, `aria-invalid`/validation text, and native-select option count. The Plugin treats this as **additional evidence** for otherwise unlabeled fields and repeated sections. It deliberately does not feed `form_context` into the existing `mapping_id`/structure signature, so learned mappings remain compatible; older Runners without the field continue to work.

## Tools

`profile_get` returns the configured structured resume resource plus the bounded canonical fill view.

`analyze_form` consumes a bounded Browser semantic snapshot and reports mapped controls, indexed resume paths, blockers, site classification, and the stable form-structure signature.

The diagnostic `analyze_form` output also includes `missing_profile_fields`, a deduplicated list of known mappings whose local profile value is missing or empty, and `unmapped_candidates`, actionable controls that still need teaching. This gives the caller a compact fallback path instead of requiring it to reinterpret a large blocker list.

`plan_fill` returns the next safe planning phase:

- `expand_sections`: exactly one add-section click, then a mandatory fresh snapshot;
- `fill_fields`: the next executable `batch`, native fields first and then bounded custom widgets, with unsupported controls in `needs_attention`;
- `advance_step`: exactly one next/continue click when explicit step/progress context is visible, then a mandatory fresh snapshot;
- `ready_for_review`: a final submission control is present; no submit click is returned.

Repeated controls map to exact source paths such as `education[1].school`, `experience[1].start_date`, or `projects[1].technologies`.

The Plugin keeps a bounded 64-entry, site-scoped in-process mapping cache and up to 256 persistent mapping-memory entries. The 24-hex-character structure signature excludes volatile element/group identities, current values, checked state, passive AX text, submit buttons, and native picker affordances. Explicitly taught mappings are restored from the private `mapping-memory.json` after Plugin reload.

## Local fixtures

The repository includes deterministic local-only forms covering Greenhouse-like and Lever-like single-page forms, a Chinese campus form, repeated sections, dynamically-added sections, and a four-step wizard.

Run them with:

```bash
npm run fixtures
```

Then open `http://127.0.0.1:32124/`. Fixture submit events are intercepted locally; no real application is sent.

## Safety boundary

This Plugin is a planner, not an autonomous submission service. It does not call Browser tools itself, access external recruiting APIs, or submit forms. Browser authority stays with WebCodex and the caller, and final submission remains a review boundary.

### Browser semantic query workflow

For a large form, use `observe_browser(action=snapshot, query={fields_only:true})`
(or narrow `text`, `group`, `section`) before passing the returned nodes to
`plan_fill`. This finds fields beyond the first ordinary snapshot page while
keeping all returned element ids in one fresh generation. Queries still return
at most 256 nodes / 64 KiB and search at most 4,352 source nodes; a truncated
no-match result is inconclusive. Each further snapshot invalidates previous ids.
Use only the actions admitted by the current snapshot, then execute the returned
bounded batch and read a fresh snapshot. `reconcile_fill` checks the whole Browser
receipt plus semantic values/states and prepares any safe deferred batch with
fresh ids. Inspect partial completion and certainty before deciding any further
effect; uncertainty never permits replay. The provider constructs and reconciles
plans while Browser owns execution. Final submission remains manual review.
