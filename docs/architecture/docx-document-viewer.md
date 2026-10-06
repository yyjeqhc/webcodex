# Dedicated DOCX presentation

`present_docx(project, path)` is a model-visible, read-only presentation tool for
one authorized project-relative `.docx`, including unchanged or untracked files.
It does not require Git or a Workflow Session, edit the file, or create a Session.
An MCP Apps Host mounts `ui://webcodex/docx/v1`; other clients retain the selected
file metadata. A presentation result selects exact size/SHA-256, not a live path.
The resource URI advances when HTML changes so Hosts invalidate their cached
reader. Retired URIs never alias the current template.

## Authority and transfer

The canonical ToolDefinition requires Project read scope, Runner ownership and
file-read capability. Existing artifact metadata/chunk primitives preserve path,
sensitive-file, mixed-version Runner capability and full-file digest checks.
The View reuses the ModelHidden, format-neutral `read_app_artifact_chunk` bridge
already used by PDF and spreadsheets; no DOCX-specific binary transport or
parallel authority path is added. Generic model dispatch and ordinary Runtime
API calls cannot admit the hidden reader. Each read reauthorizes the Project; a
digest is not authority.

The View fetches 512 KiB Host-facing segments under the generic scaled transfer
deadline (minimum 120 seconds, 60 seconds plus 20 seconds per segment, capped at
15 minutes), validates every identity/continuation, then verifies the complete
SHA-256. MCP removes bytes from both structuredContent and text before returning
them in private `_meta["webcodex/artifactChunk"]`. The App normalizes the Host
wrapper aliases observed by the PDF/spreadsheet readers (`result`,
`result.result`, `toolResult`, `tool_result`, and `_meta`/`meta`).
Changed content fails closed. Retry retains the selected version; a fresh
`present_docx` explicitly selects a new one. No immutable file cache is implied.
Repeated Host delivery of the same result does not start another transfer.
Teardown cancels pending requests, releases verified bytes and disconnects the
viewport observer.

## Renderer and limits

`docx-preview` 0.4.1, JSZip 3.10.1 and DOMPurify 3.4.16 are bundled into the HTML
with their licenses; no CDN, network font or nested document iframe is needed.
Only stable `renderAsync` is used. External relationships are removed before
rendering; embedded HTML/altChunks, embedded fonts, comments and tracked-change
rendering are disabled. Rendered DOM is sanitized, stylesheet selectors are
scoped to the document stage, and external URL/active content is removed.

Preview bounds are 10 MiB compressed, 32 MiB expanded, 8 MiB per ZIP entry,
2048 entries, 8 MiB XML, 60000 XML nodes and 40000 rendered elements. Central and
local ZIP identities are preflighted; streaming decompression also enforces
actual expansion bounds before accumulating oversized entries. ZIP64,
encrypted/multipart ZIPs, legacy `.doc`, unsupported packages, XML DTD/entities
and malformed content fail with a visible message. When the Host advertises
`downloadFile`, `ui/download-file` carries only the
verified original bytes in an embedded resource, never the sanitized render copy.
The download control remains hidden when the Host does not support this capability;
it never bypasses iframe download restrictions.

The reader follows Host viewport changes while fit width is selected, preserves
manual zoom, and declares/checks supported display modes before requesting
fullscreen.
Original DOCX download is optional and mediated by the Host. Common text,
tables, inline images, headers/footers and explicit page
breaks are supported. Automatic Word-equivalent pagination, fields/TOC updates,
complex shapes and exact font fidelity are not promised. The UI states that
layout and pagination may differ from Word. Layout acceptance tests use the
real renderer; a local browser Host smoke verifies bridge transfer and controls.

## Verification

```powershell
npm --prefix frontend run build:docx
npm --prefix frontend run test:docx
node frontend/scripts/build-docx.mjs --check
cargo test --locked -p webcodex --lib docx
# With scripts/ui-smoke dependencies and a local browser installed:
node scripts/ui-smoke/docx.mjs
```

Real ChatGPT host compatibility remains a deployment smoke requirement. The
local Host fixture does not demonstrate production host behavior or deployment.
