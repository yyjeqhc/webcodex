# Read-only spreadsheet reader

`present_spreadsheet(project, path)` is the only added model-visible tool. It is a
Direct Presentation tool with `project:read` and the owning Runner's FileRead
capability. It opens `ui://webcodex/spreadsheet/v3`, an independent MCP App with no
Activity, Results or Collaboration tabs. Viewing a spreadsheet alone requires
neither a Workflow Session nor `present_work_result`.

The first version accepts UTF-8 CSV/TSV and XLSX up to 5 MiB. It retains worksheet
names, original cell coordinates, stored numbers, formatted date/currency strings,
formula text and cached formula results. Missing formula caches are labelled;
the reader never recalculates, edits or saves, executes VBA, loads links, or
interprets cell content as HTML. Excel styling, merged-cell layout, charts,
pivots, legacy XLS and encrypted files are outside this data reader.

## Source and authority

The canonical runtime observes bounded export metadata through the existing
Runner file operation and returns a caller-bound artifact export ResourceLink
for download. Project-relative paths are limited to 512 bytes, matching the App
artifact transport. The MCP adapter also places the canonical result in private
`_meta["webcodex/spreadsheetSource"]`. The reader normalizes common Host wrappers,
`structuredContent` / `structured_content`, and `_meta` / `meta`.

The View uses the existing MCP App-only `read_app_artifact_chunk` tool from the
PDF reader, with exact Project, path, bytes, SHA-256 and byte offset. Each call
reads at most 512 KiB and reauthorizes Project access, FileRead capability and the
unchanged file incarnation through the canonical Runtime. Binary data remains
in private `_meta["webcodex/artifactChunk"]`; public structured/text fallback
contains only chunk identity and continuation metadata. No spreadsheet-specific
transport tool, model-visible byte payload or Workflow Session is added.

The App validates the segment's complete identity, encoded/decoded sizes and
continuation, then verifies the complete file SHA-256 before parsing. Transfer
uses one absolute deadline matching the generic transport: at least 120 seconds,
60 seconds plus 20 seconds per 512 KiB segment, capped at 15 minutes. The 5 MiB
reader limit therefore permits up to 260 seconds. Each call receives only the
remaining total budget; progress never resets it. Replacement and teardown
cancel pending requests, fence late replies and stop further segment requests.
The ten-second disposable Worker parsing deadline is a separate resource bound.
After loading, worksheet switching and navigation are local.

The downloadable export expires after five minutes. App reads use the explicit
version identity, rather than the expiring export handle; SHA-256 is never
permission to read another caller's Project. Changed content requires an explicit
new presentation. The adapter supplies private presentation metadata according
to the frozen Server Apps setting, since direct calls may omit discovery's UI
capabilities; no retained transport or Session authority is inferred.

## Rendering choice

Reuse SheetJS CE for XLSX parsing and formatted text, and own the small read-only
DOM grid. CSV/TSV use a bounded text-only decoder; their content must never be
autodetected as HTML, XML or SYLK. Existing MCP Apps use standalone scripts; introducing an entire spreadsheet
editor or a React grid would add a separate UI stack and editable state that this
reader does not need. Both axes render only the visible window plus one-item
overscan. The viewer supports cell selection and arrow-key navigation. When the
selected cell scrolls outside the rendered window, the grid container retains
the keyboard entry and focus without changing the selection.
Each worksheet retains its selection and both scroll offsets while the current
workbook is open. Returning to a worksheet restores its viewport without forcing
an off-screen selection into view; clicking the active tab leaves it untouched.
Selection content is updated before measuring the viewport for scroll restoration,
since a taller value/formula area can add a document scrollbar and narrow the grid.
Positions are keyed by worksheet index and cleared when the source changes,
becomes invalid or fails, or the reader closes. They are not persisted across
presentations of different file versions.

Parsing runs in a disposable Blob Worker with a ten-second deadline. XLSX ZIP
directories are checked before inflation: no encryption, ZIP64 or multi-volume
archives, at most 4,096 entries and 32 MiB declared expanded content. Compressed entries
are streamed through native `DecompressionStream` and discarded to verify their
actual expanded size and CRC before SheetJS parses them; stored entries also
verify CRC. Dishonest size fields cannot bypass the 32 MiB bound. Sparse parsing
visits actual worksheet cell keys rather than the full used-range rectangle,
preserves high absolute row coordinates, and retains readable Excel error values; the row/column limits bound each used
range's span, rather than truncating absolute row numbers. Workbook
bounds are 32 sheets, 50,000 rows and 256 columns per sheet, 200,000 populated
cells total, 32,768 characters per cell field, and bounded extracted text. These
checks do not replace the parser's correctness or the Host's Worker isolation.
Only this App declares `ui.csp.resourceDomains: ["blob:"]` in both resource listing
and reads, so Hosts can permit its embedded Blob Worker. `connectDomains` stays
empty, and other Apps keep their existing CSP. Hosts may still deny Workers;
parsing never falls back to the main UI thread. Hosts must support MCP Apps,
private tool-result metadata, App tool calls, Blob Workers and native raw-deflate decompression for compressed
XLSX files. Unsupported
Hosts receive a clear reader error; the original export remains downloadable.
Actual ChatGPT placement/CSP compatibility requires a Host smoke test.

## Dependency provenance

The SheetJS CE 0.20.3 Mini build is vendored without source modifications, with its complete
Apache-2.0 license retained and embedded in the generated worker. It is obtained
from the official fixed-version distribution (the Tsinghua mirror returned 404):

- Package: https://cdn.sheetjs.com/xlsx-0.20.3/xlsx-0.20.3.tgz
- Package SHA-256: `8dc73fc3b00203e72d176e85b50938627c7b086e607c682e8d3c22c02bb99fe8`
- Mini script SHA-256: `0cb353f830d7288385492c83d277b058ddeac664ca51cf1393aa1fd3e2b70939`
- Mini build: https://docs.sheetjs.com/docs/getting-started/installation/standalone/
- Parsing: https://docs.sheetjs.com/docs/api/parse-options/
- License: https://docs.sheetjs.com/docs/miscellany/license/

`npm --prefix frontend run build:spreadsheet` rebuilds the embedded App without
network dependencies; `node frontend/scripts/build-spreadsheet.mjs --check`
checks generated content. Git attributes disable text conversion for the vendored
SheetJS script, preserving its exact upstream SHA-256, and pin the spreadsheet
bundle inputs and generated HTML to LF. Windows `core.autocrlf=true` must not
change the vendor bytes or invalidate the generated-content check. The existing
working files use these same canonical line endings. Advance the App resource
URI when shipping an updated template or incompatible contract.


## Focused verification

Run `npm --prefix frontend run test:spreadsheet` for parser, deterministic
Host/Worker lifecycle and isolated Git recheckout regressions. These require Node
and Git, with no browser or network dependencies. Recheckout tests cover both
`core.autocrlf=true` and `false`, bundle freshness, and rejection of vendor bytes
changed by line-ending conversion. Use `cargo test --locked -p webcodex --lib present_spreadsheet`
for download-resource authority/expiry, private generic chunks, Project authority,
version fences, capability-scoped App metadata and path rejection,
and the `app_registry` filter for bundled resource invariants. The tool definition
test lives in `webcodex-tool-contracts` under `spreadsheet_presentation`.

Local Chrome verification used a sandboxed iframe and a 20,001-row workbook,
including formulas, Unicode, multiple sheets and HTML-looking cell text. It
confirmed virtual scrolling, keyboard selection, no View resource reads or extra tool calls for local
navigation, failed source replacement, and no page overflow at
736 px and 320 px in light/dark themes. Bug regressions also verify high absolute
rows through 1,048,576 (including formulas), CRC corruption in both stored and
compressed entries, and retained keyboard focus/Tab entry after scrolling the
selection outside either rendered axis. Chrome confirms Blob Workers run with
explicit `worker-src blob:` or the declared `script-src blob:` fallback, while
`worker-src 'none'` fails visibly. This fixture does not establish actual ChatGPT
Host compatibility.

Earlier ChatGPT sidebar verification on 2026-10-05 used the v7 template with
matching dogfood Server and Runner builds. A 3,110,691-byte, 20,001-row XLSX
loaded completely through App-only paged reads. Both worksheet tabs worked,
formatted values and formula text were retained, and the visible grid stayed
bounded at 64–68 cells. After Control+End scrolled the selected cell out of the
rendered window, the grid retained focus and a Tab entry; ArrowLeft selected C2
and brought it back into view. A separate workbook preserved row 1,048,576 and
its formula. UTF-8 CSV retained leading zeros and rendered formula/HTML-looking
strings as plain text. The intentionally corrupted XLSX failed with a CRC error
and no cells. The independent sidebar contains the reader alone, without the
work-result Activity, Results or Collaboration panels.

Earlier direct View resource reads were rejected by ChatGPT's widget scope. A
single large private result was also truncated in this sample, and splitting
that same result into an array did not deliver it completely. The v7 adapter
therefore used a caller-bound export URI and spreadsheet-specific App-only reads.
A later dogfood adapter instead used the mainline generic private App transport,
with wrapper normalization and a deadline scaled by Host round trips. The observed truncation is evidence for
this Host test, not a documented universal payload limit. Other Hosts still
require their own compatibility smoke test. Screenshots and synthetic fixtures
are kept outside the tracked feature changes.

The final transport convergence has deterministic coverage for 5 MiB transfers with
15-second Host round trips, common Host wrappers, private metadata normalization,
identity/continuation corruption, deadline expiry, replacement and teardown.
Parser regressions cover a two-cell A1:IV50000 worksheet without scanning empty
coordinates, and both plain and formula-bearing Excel error cells. The v7/v8
identities below are historical dogfood iterations; the unmerged production
resource starts at v1. Actual production-resource ChatGPT smoke must be repeated
before claiming acceptance of this transport change. Disposable recheckout
fixtures are removed after a verified temporary-directory boundary.
