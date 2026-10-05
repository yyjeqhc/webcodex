# Dedicated PDF document viewer

`present_pdf(project, path)` binds a direct model tool to the self-contained
`src/mcp_pdf_app.html` resource, `ui://webcodex/pdf/v10`. It selects one
authorized project-relative `.pdf`, independent of Git changes, a Work Result
card, or a Workflow Session. The App contains only document controls and the
reading area; the Host owns the surrounding chrome and display mode.

## Authority and version selection

`present_pdf` is a read-only `project:read` observation. It validates the
project-relative path, bounds PDFs to 20 MiB, reads the first five bytes, and
pins the selected file by exact size and SHA-256. The size/digest pair is a
version fence, never a bearer credential or replacement for current Project
authority.

The dedicated reader uses the model-hidden, App-only
`read_app_artifact_chunk` transport. The tool is format-neutral so later
DOCX/PPTX/XLSX renderers can share the same transport. Every read:

- re-resolves and reauthorizes the Project;
- revalidates the project-relative artifact path and exact size/SHA-256;
- reuses the authenticated Runner artifact-export path and its capability
  checks;
- returns at most the canonical 1 MiB internal artifact chunk;
- fails closed with `snapshot_changed` if the selected bytes change.

The adapter removes `content_base64` from model-visible structured/text
framing and places bytes only in private
`_meta["webcodex/artifactChunk"].content_base64`. The App accepts the Host
wrapper aliases observed for both structured results and private metadata
(`result`, `result.result`, `result.toolResult`, and
`result.tool_result`) before validating the chunk identity and continuation.

The older PDF-specific `read_pdf_chunk` remains temporarily available for
already-mounted compatibility clients, but new dedicated readers use only the
generic artifact transport.

## Reader behavior

The reader reconstructs the selected byte stream under one 120-second transfer
deadline, validates every continuation, recomputes SHA-256, and checks the
`%PDF-` header before PDF.js receives the document.

Rendering uses PDF.js 6.4.299 core parsing, page rendering, and `TextLayer`
rather than the full `pdf_viewer.mjs` application layer. A small App shell
provides:

- continuous vertical page scrolling;
- current-page tracking with `IntersectionObserver`;
- nearby-page lazy rendering instead of eagerly painting the full document;
- Previous/Next scroll navigation;
- fit-width and bounded zoom;
- page-local text search and selection.

Offline fonts/CMaps, the 8-million-pixel canvas cap, disabled scripting/eval,
XFA/Wasm/external fetches, the 16-million-pixel embedded-image limit, and
bounded Worker cleanup remain in place. Work Result keeps its compact
single-page PDF preview; the dedicated reader is a separate presentation
surface.

The current transport assembles the complete file before PDF.js starts
rendering. Progressive first-page display therefore requires a separate
range-transport design; continuous rendering improves navigation and render
boundedness but does not change initial byte-transfer latency.

## Host compatibility

The tested ChatGPT MCP App bridge supports App-originated `tools/call` for the
private reader. It did not accept the earlier App-originated
`resources/read` experiment. The production design therefore keeps binary
reads on the admitted App-only tool while leaving generic MCP artifact export
unchanged.

Live dogfood also showed that Host tool-result wrapper shape is not stable
enough to assume top-level `_meta`. Private metadata is normalized with the
same wrapper policy as structured results. The evidence and rejected
alternatives are recorded in
[PDF document viewer dogfood](../experiments/pdf-document-viewer-dogfood.md).

## Build and validation

The common asset builder creates the closed PDF HTML bundle and normalizes
gzip's OS header so native platform metadata cannot change checked-in bytes.
Git attributes retain LF line endings for generated frontend assets. Resource
identity is version-fenced so incompatible App changes receive a fresh Host
cache identity.

Run:

```sh
npm --prefix frontend run build:work-result
node --test frontend/test/pdf-document-reader.test.mjs
cargo test --locked -p webcodex --lib pdf_document
cargo test --locked -p webcodex --lib pdf_document_app
```

`frontend/scripts/build-pdf-document.mjs --check` is part of distribution
freshness validation. Runtime and MCP authority/framing tests remain in the
existing PDF document test modules.
