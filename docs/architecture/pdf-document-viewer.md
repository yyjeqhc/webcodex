# Dedicated PDF document viewer

`present_pdf(project, path)` binds a direct model tool to the self-contained
`src/mcp_pdf_app.html` resource, `ui://webcodex/pdf/v1`. It selects one authorized
project-relative `.pdf`, independent of Git changes, a Work Result card, or a
Workflow Session. The MCP App registry supplies the resource binding and prefers
the Host's fullscreen/sidebar mode. The App's UI contains only the filename,
PDF controls and reading area; the Host still owns its external chrome.

## Authority and version selection

Canonical ToolDefinition owns the model-visible presentation tool and model-hidden
`read_pdf_chunk`. Both are project-read observations. App reads additionally
require the adapter-owned PDF App protocol capability; the ordinary gateway does
not expose the read tool. The capability enables transport, never Project access.
The presentation tool must use its direct callable while MCP Apps are enabled.

Metadata and byte reads reuse the existing authenticated generation-2 artifact
export transport, including Runner ownership, native artifact path policy and
streaming capability checks. Symlinks and sensitive/outside paths follow that
transport's existing fail-closed policy. The selected size and SHA-256 are a
version fence, not a capability or bearer credential. Each chunk re-resolves
current authority. No preview handle, extra credential audience, or Session is
created. The first five bytes must be `%PDF-`; files are bounded at 20 MiB.

The App supplies the exact Project/path/size/digest plus byte offset. Each read
returns at most 128 KiB. Runtime `pdf_chunk.content_base64` is removed by the MCP
adapter before text/structured framing and placed in
`_meta["webcodex/pdfChunk"].content_base64`. The App validates identity, bounds,
continuation and the final assembled SHA-256 under one 120-second transfer
deadline. A changed file fails with `snapshot_changed`; the current hash is not
returned as permission to retarget. Retry retains the selected version. Another
explicit `present_pdf` call selects a replacement.

Initial display metadata also uses `_meta["webcodex/pdfDocument"]` for Hosts that
omit structuredContent. App-only reads retain the existing bounded text-envelope
fallback, without binary bytes. Repeated initial tool-result delivery does not
restart a live reader. Replacement and resource teardown fence late replies and
destroy the old PDF Worker.

## Rendering and build

The dedicated reader reuses PDF.js 6.4.299 and the Work Result renderer, including
offline fonts/CMaps, the 8-million-pixel canvas cap, disabled scripting/eval/XFA/
Wasm/external fetches, page-local search and bounded Worker cleanup. The standalone
version-fenced reader is separate from Work Result's immutable Git blob reader.
Embedded images remain capped at 16 million pixels. PDF.js parsing errors,
including that limit, fail the preview visibly instead of silently removing
content; the canvas is cleared and retry retains the selected version.
Worker stream failures are also checked before reporting render success because
PDF.js can resolve its render promise after an operator-list stream fails.
The common asset builder creates both closed HTML bundles. Work Result's URI is
advanced to v27 because its embedded renderer changes, preserving the pre-existing
v26 layout fix.

Run `npm --prefix frontend run build:work-result` to generate both resources.
`build-pdf-document.mjs --check` is included in `check:dist`. Focused reader tests
are in `frontend/test/pdf-document-reader.test.mjs`; runtime and MCP tests are
in the existing `pdf_document` domain modules.

Run the maintained reader and authority/framing tests from the repository root:

```sh
node --test frontend/test/pdf-document-reader.test.mjs
cargo test --locked -p webcodex --lib pdf_document
```
