# Projects & Resources workbench

WebCodex provides readonly discovery and latest-content references for Projects,
tracked project files, owned Goals, and recorded task outputs. The Rust resource
service is shared by ordinary tools, the Workbench App, and MCP `resources/read`.
It adds no file editor, task execution, sharing grant, or host file-save operation.

## Ordinary chat and MCP clients

These tools work independently of the MCP Apps configuration switch:

```json
{"tool":"open_webcodex_workbench","params":{}}
{"tool":"search_webcodex_resources","params":{"kind":"project","query":"webcodex"}}
{"tool":"search_webcodex_resources","params":{"kind":"file","project":"~p1","query":"README"}}
{"tool":"search_webcodex_resources","params":{"kind":"goal","query":"release"}}
{"tool":"search_webcodex_resources","params":{"kind":"artifact","project":"~p1","session_id":"wc_sess_EXPLICIT"}}
{"tool":"read_webcodex_resource","params":{"uri":"<uri returned by search>"}}
```

Select a Project from the returned list. For outputs, call `list_sessions` on that
Project and explicitly select a retained Session. A missing Session or output
manifest is not proof that the project never produced any artifacts. Goals are
owned by the caller and searchable across their Projects; selecting a Project
does not imply an association with every Goal in the list.

Queries are literal substrings with a 200-character ceiling. Project search uses
the existing case-insensitive discovery; Goal search folds ASCII case; file paths
are case-sensitive. File filtering precedes pagination and query mode returns
exact tracked paths without directory rollup. Untracked/generated outputs appear
through the explicit Session manifest, not tracked-file discovery.

Pages include `offset`, effective `limit` (1–100), `total`, `next_offset`, and
`list_truncated`. A truncated producer source does not advertise a false offset
continuation. Project acquisition is capped at 2,000 entries; narrow the query
when incomplete. Bodies and pages have byte budgets. Text reads retain the
canonical file-read version and partial-read flags; `truncated=true` also covers
remaining lines and output-budget clipping. Binary reads return current metadata;
use the existing `project_artifact` image/export operations for image presentation
or download, with their ordinary policy and version fences. The Workbench does not
save files into the Host.

## Resource identity and authorization

Resource URIs use `webcodex-resource://` with canonical base64url (without padding)
components:

| Resource | URI components |
| --- | --- |
| Project | `project/<canonical-project-id>/<root-fingerprint>` |
| File | `file/<canonical-project-id>/<root-fingerprint>/<relative-path>` |
| Goal | `goal/<canonical-goal-id>` |
| Task output | The same File URI, with Session observation provenance on its search link |

These components are encoded identifiers, not secrets or bearer capabilities.
Principal-local `~pN`/`~sN` selectors and expiring export links are not the durable
identity. Reads re-authorize the original domain and pin the current Project root
through the existing durable-reference resolver. Replacing the root, deleting or
renaming the file, or losing visibility makes the old locator fail. A recreated
file at the same path in the same root is the latest content of that path resource.
Project summaries omit absolute local paths.

Project reads require the existing Project authority; Goal reads retain owner and
communication scope checks; output discovery requires Project and exact Session
authority. One domain's missing scope does not prevent use of an authorized domain.
Output provenance retains the historical observed time, bytes, SHA and status. It
never certifies the current file. A later finish event invalidates an older manifest,
including when the later event omits outputs. No new artifact index is introduced.

Both legacy and Stateless MCP accept these resource URIs in `resources/read`.
The JSON in `contents[0].text` is the same output as `read_webcodex_resource`.
Resource reads use domain authority rather than adding an unrelated runtime-read
scope requirement. Discovery still retains the existing discovery scope checks.

## App and OpenAI adapter

`ui://webcodex/workbench/v1` is a separate App from the existing Project-bound
Work Result card. With `{}`, the Workbench presents a Project chooser even when
only one Project is visible. Named launch selectors are authorized and returned
canonically. Overview reuses Work Result; files and outputs require explicit
Project/Session selection. Goals remain usable without a selected Project.
Switches clear stale results and fence delayed replies. Hidden views pause refresh;
teardown cancels pending view work. Repeated previews re-read current content.

The primary reference action calls `ui/update-model-context`, replacing the View's
selected references without sending a message or starting a task. Standard
`hostCapabilities.updateModelContext` determines whether resource links, embedded
resources, or bounded text are accepted. A Host without these modalities gets
copyable bounded text. Only an advertised `experimental["openai/modelContext"]`
activates OpenAI remount/removal synchronization through
`hostContext["openai/modelContext"]`, context-change notifications, and the
response `_meta["openai/modelContext"].updateId`.

The launcher advertises `_meta["openai/ui"].entrypoints` for `global` and `thread`,
with tool title **Projects & Resources**. The host-only `search_mentions({query})`
descriptor uses `_meta["openai/extensions"]["mentions/search"]` and
`_meta.ui.visibility=["app"]`. Its result is exactly `structuredContent.items`
containing standard resource links. It searches Projects and Goals only, skips
unauthorized domains, and never scans all Runners for files. Incomplete authorized
sources are marked in result metadata; an empty incomplete search is an error.
This adapter tool is unavailable through the generic runtime gateway.

Apps require the existing server switch, Stateless protocol, and advertised MCP
Apps MIME capability. App-originated calls retain configuration/protocol admission
and domain authorization even when subsequent view calls omit capability metadata.
Disabling `WEBCODEX_MCP_APPS_ENABLED` removes UI metadata/resources and native
mentions; ordinary resource tools and reads remain available. View reads do not
record in a Workflow Session or consume Window message attention.

The OpenAI extensions documentation describes native support for Work and excludes
classic ChatGPT from its web support table; mention search is desktop-only. Use
capabilities rather than product-name detection. Ordinary ChatGPT can use the
tool workflow even when it cannot mount the Workbench.
See the [extension specification](https://github.com/openai/mcp-extensions/blob/main/docs/spec.md),
[server UI metadata](https://github.com/openai/mcp-extensions/blob/main/typescript/src/server/ui.ts),
and [mention search contract](https://github.com/openai/mcp-extensions/blob/main/typescript/src/server/mentions.ts).

## Validation and host acceptance

Run the focused Rust resource, Goal, file-listing, permission and MCP suites plus:

```sh
cargo check -p webcodex --profile dogfood
cargo test -p webcodex-store goal_tests --lib
cargo test -p webcodex-core project_listing --lib
cargo test -p webcodex-tool-contracts --lib
cargo test -p webcodex --profile dogfood --lib resource_references
cargo test -p webcodex --profile dogfood --lib workbench
cargo test -p webcodex --profile dogfood --lib mcp::tests
node --test src/mcp_tests/*.test.mjs
```

The dependency-free DOM tests cover initialization order, absent structured content,
empty launch, selection races, independent Goal access, stale replies, lazy latest
reads, modality fallback, removal/remount synchronization, visibility and teardown.
The Rust fixtures include foreign owners/scopes, stale roots, latest text/deletion,
binary metadata, incomplete ranges and retained output manifest invalidation.

Live-host acceptance requires an explicitly named test deployment. No service
restart, deployment, connector refresh or plugin publication is implied by local
implementation. Record evidence separately:

| Host | Protocol / capabilities | Handshake and rendering | References / removal | Status |
| --- | --- | --- | --- | --- |
| Ordinary ChatGPT web | Capture actual request metadata | Check Workbench mounting or tool fallback | Verify supported modalities | Not run; test deployment not specified |
| Desktop Codex | Capture Stateless/MCP Apps/experimental capabilities | Check global/thread entry and resource fetch | Check `@`, add/remove and remount | Not run; test deployment not specified |

Passing source tests does not certify either live Host.
