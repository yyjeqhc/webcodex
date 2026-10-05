# Work Result PDF preview

Work Result includes PDF.js 6.4.299 in the existing changed-file reader, in both
the card and native conversation panel. Expanding a `.pdf` row selects PDF by
default. Other file formats keep the existing diff/text/Markdown behavior.
PDF.js, the module Worker, CMaps, standard fonts and their licenses are embedded
in the shipped HTML. Rendering needs no CDN, external document reader, Node
service, new tool registration, or additional tunnel in production.

## Read boundary

The existing App-only `get_work_result_state` accepts
`files: { snapshot_id, path, view: "pdf", byte_offset }`. Every call uses the same
exact Project, optional Session, principal-bound snapshot and advertised-path
checks as diff/text inspection. Reads resolve a regular blob in the immutable
final Git tree; they never follow a live path, symbolic link, submodule or Git
filter. Deleted/non-PDF/oversize files return a closed `unavailable_reason`.

PDF files must begin with `%PDF-` and be at most 20 MiB. Pages contain at most
128 KiB of raw bytes, fitting the Runner's bounded text capture after Base64.
Runtime `content_base64` is moved by the MCP resource adapter to
`_meta["webcodex/pdfChunk"].content_base64` before structured/text framing. Only
identity, size and continuation metadata enter those public result channels.
The read tool remains model-hidden and receives no renderer/resource binding.

The HTML controller supplies the pinned identity and RPC. The PDF module checks
every response's Project/Session/snapshot/path/offset/size/continuation, assembles
one document with a single 120-second transfer deadline, and stops on close or
source replacement. A retry starts at byte zero of that exact source. The UTF-8
reader keeps its independent 32 KiB page and 256 KiB file limits.

## Renderer lifecycle

Only one active preview owns a document. Switching mode/file, collapsing the
row, refreshing the file snapshot, failure or teardown cancels rendering and
clears the text layer/canvas. PDF.js receives a brief bounded Terminate handshake
before the Worker and Blob URL are released (at most one second). Pending reads
cannot recreate a closed renderer. No PDF byte cache survives close.
Path filtering only hides a retained row: loading continues against the same
pinned snapshot, and any error remains available when the row is shown again.

One visible page has an 8-million-pixel backing limit, DPR at most 2, zoom from
15% to 300%, fit-width, paging, selectable text and current-page search. The
viewer does not instantiate scripting or annotation/link handling. XFA, eval,
Wasm and external Worker fetches are disabled. Worker denial produces an error
and explicit retry; there is no main-thread parser fallback.
Embedded images above 16 million pixels and other PDF.js parsing errors fail
the preview visibly rather than producing a successful but incomplete page.

Toolbar groups wrap in a narrow panel. Explicit opening focuses the row again
after load only if the reader has not scrolled during loading. Width changes
trigger fit-width rendering; height changes do not.

## Build and verification

`npm --prefix frontend run build:work-result` generates the closed PDF bundle
alongside Markdown/sections. `build-work-result-pdf.mjs --check` verifies it;
`check:dist` includes that gate. Updating bundled HTML advances the App URI.

Focused coverage includes `frontend/test/work-result-pdf.test.mjs`, Work Result
controller tests, immutable PDF blob/authority tests, and private MCP framing.
Run these maintained checks from the repository root:

```sh
node --test frontend/test/work-result-pdf.test.mjs src/mcp_tests/work_result_app.test.mjs
cargo test --locked -p webcodex --lib work_result_pdf
cargo test --locked -p webcodex --lib work_result_content_pages
cargo test --locked -p webcodex --lib mcp::tests::work_result_app
```
