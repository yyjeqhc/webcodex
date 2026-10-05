# PDF document viewer MCP App dogfood

Date: 2026-10-05.

This report records the live ChatGPT MCP App experiments that led from the
initial dedicated PDF reader to the production transport/reader design. It is
evidence for Host compatibility decisions, not a claim about every future MCP
App Host.

## Scope and environment

The test surface used the MSI WebCodex dogfood Runner and the same configured
MCP/tunnel connection exposed through the `w10` / `windows` app identities.
Full WebCodex tool-request tracing was enabled with
`WEBCODEX_TOOL_REQUEST_TRACE=full`.

Two project PDFs were used repeatedly:

| PDF | Bytes | Purpose |
| --- | ---: | --- |
| `P07_2023_TITS_Spatiotemporal-Anomaly-Detection.pdf` | 1,360,401 | Small transport probe |
| `P25_2023_TIFS_Federated-Graph-Network.pdf` | 9,263,389 | Realistic multi-chunk paper |

Server-side `present_pdf` and chunk handling were consistently on the order of
milliseconds once a request reached WebCodex. The experiment therefore treats
long gaps before the next App call as Host/App delivery behavior, not Runner
file-read time. A contemporaneous Statsig request failure was not sufficient
evidence of a PDF-network root cause, and MSI↔sf latency was not the direct
artifact-read path.

## Experiments

### App-originated `resources/read`

An early transport attempted to export the exact artifact as a normal MCP
Resource and let the App issue `resources/read`. The current ChatGPT MCP App
bridge rejected that App-originated request method. The generic server Resource
transport itself remained valid; the incompatibility was at the App→Host
method boundary.

Conclusion: presentation Apps should use an admitted App-only `tools/call`
bridge for private binary reads until the Host exposes a supported resource
streaming path.

### Generic artifact transport

The PDF-specific 128 KiB loop was replaced by
`read_app_artifact_chunk`, a format-neutral, version-fenced Project artifact
reader. It reuses current Project authorization and the canonical internal
artifact-export path. Binary bytes remain private MCP metadata.

This is intentionally not named after PDF so future document/spreadsheet/slide
renderers can reuse the same authority and transfer contract.

### Full PDF.js viewer application

A reader variant bundled `pdfjs-dist/web/pdf_viewer.mjs` for its complete
viewer infrastructure. In live ChatGPT dogfood the App failed before it issued
a chunk call. Local browser parsing alone did not reproduce the Host failure,
and the experiment did not prove which exact construct in the larger viewer
bundle was rejected.

Conclusion: keep PDF.js as the parsing/rendering engine, but use a thin
Host-compatible continuous reader shell instead of importing the complete
viewer application until that Host boundary is understood.

### Chunk-size hypothesis

With the thin continuous shell, a 1 MiB chunk request reached WebCodex and the
Server returned the response in milliseconds, but the App did not issue the
next request. A diagnostic revision reduced the Host-facing segment to 512 KiB;
that first attempt still stopped because private metadata wrapper handling was
also incorrect.

After fixing private metadata normalization, the 512 KiB path completed full
multi-chunk documents. A final 2026-10-05 probe then restored 1 MiB and reopened
the 9,263,389-byte P25 paper: `present_pdf` succeeded, the first 1 MiB chunk
completed server-side in 27 ms, and the serialized MCP response was 1,399,036
bytes, but no second App chunk call arrived. This isolates a second Host-facing
constraint in addition to the metadata wrapper bug.

Conclusion: keep the canonical internal artifact transport, but bound the
presentation App's Host-facing segment to 512 KiB.
### Private metadata wrapper normalization

Structured tool-result parsing already accepted several Host wrapper shapes,
but binary metadata was read only from top-level `result._meta`. The App was
updated to normalize private metadata across:

- `result._meta` / `result.meta`;
- `result.result._meta` / `result.result.meta`;
- `result.toolResult._meta`;
- `result.tool_result.meta`.

After this change, the same live Host completed every requested chunk.

Observed 512 KiB diagnostic run:

| PDF | Expected calls | Observed offsets | Result |
| --- | ---: | --- | --- |
| P07, 1,360,401 bytes | 3 | 0; 524,288; 1,048,576 | All 3 completed |
| P25, 9,263,389 bytes | 18 | 0 through 8,912,896 | All 18 completed |

For P07, the three Server call durations were 9 ms, 19 ms, and 6 ms in the
captured run. For P25, the 18-call sequence completed end-to-end; the last
request used offset 8,912,896. The user then verified the rendered document was
continuous, mouse scrolling crossed page boundaries, and zoom interaction
worked.

Conclusion: private metadata normalization was necessary but not sufficient for
1 MiB Host-facing responses. The demonstrated stable combination is normalized
private metadata plus 512 KiB App segments; PDF parsing, Runner reads, and the
continuous-page renderer were not the bottleneck in the successful runs.

### Whole-document deadline under Host delay

A final v4 dogfood run kept the stable 512 KiB segment and reopened the same
9,263,389-byte P25 paper after a fresh schema refresh. Seven sequential chunks
reached WebCodex at offsets 0 through 3,145,728. Server handling stayed between
roughly 12 and 24 ms, but later Host handoff gaps were about 15–19 seconds. The
reader's fixed 120-second whole-document deadline was therefore exhausted before
the remaining chunks could be requested, and the App surfaced a Host request
timeout/failure.

Conclusion: the fixed 120-second document budget was inconsistent with the
bounded 512 KiB transport under a slow Host. The reader now keeps one absolute
budget but scales it by expected Host round trips: 60 seconds plus 20 seconds per
512 KiB chunk, with a 120-second floor and 15-minute cap. This changes only the
client wait bound; Project authority, version fencing, per-call Host timeout,
and full-document SHA verification are unchanged.

A follow-up v5 dogfood run exposed one remaining mismatch: the whole-document
budget for P25 was correctly raised to 420 seconds, but each App `tools/call`
still had an independent 65-second timer. Two concurrently mounted readers were
visible in the trace; each advanced through offsets 0, 524,288, 1,048,576, and
1,572,864 with 12–24 ms Server handling, then no later request reached WebCodex.
Regardless of the exact Host-side delay, the local 65-second timer could still
abort a chunk before the 420-second document budget and therefore violated the
intended single-deadline contract.

Conclusion: v6 removes the independent 65-second App-call cap. Each chunk call
inherits the remaining absolute document budget; the Host may still fail a call
earlier on its own, while WebCodex no longer introduces a contradictory shorter
client timeout.

A final v6 retest reopened the same 9,263,389-byte P25 paper after deployment and
schema refresh. The App completed all 18 expected 512 KiB chunk calls, with
byte offsets 0 through 8,912,896. The first chunk arrived about 3.2 seconds after
`present_pdf`; the final chunk arrived about 66.0 seconds after `present_pdf`.
Server handling stayed between 12 and 23 ms per observed chunk. This validates
the combined production contract: normalized private metadata, 512 KiB
Host-facing segments, one size-aware absolute document deadline, and no shorter
per-call client timeout.

## Production conclusions

1. Keep `present_pdf` as a direct read-only App presentation tool.
2. Use one generic App artifact transport with exact Project/path/size/SHA-256
   fencing and per-call reauthorization.
3. Keep document bytes in private MCP metadata; do not put binary payloads in
   structured/text model framing.
4. Normalize both structured results and private metadata across observed Host
   wrapper aliases.
5. Bound Host-facing App segments to 512 KiB. The internal artifact transport may
   still use larger chunks elsewhere; the PDF App bound reflects observed ChatGPT
   delivery behavior after metadata normalization.
6. Use PDF.js core plus a thin continuous-scroll shell. Avoid the full viewer
   application bundle on this Host until compatibility is demonstrated.
7. Keep Work Result's compact single-page preview separate from the dedicated
   document reader.
8. Scale the whole-document deadline with the bounded Host round-trip count rather
   than assuming every Host can deliver a multi-chunk file inside 120 seconds.
9. Treat progressive/range loading as a separate optimization. The current
   reader verifies the complete selected file before PDF.js rendering starts.

## Follow-up opportunities

The transport is deliberately format-neutral. A future document-viewer family
can share the same App artifact bridge while keeping separate Host resource
identities and renderer bundles for PDF, DOCX, PPTX, XLSX, and related formats.

For diagnostics, an explicit App call correlation id would make
iframe→Host→Server trace alignment easier. Progressive PDF range loading is also
worth evaluating once the current whole-file path is stable; it must preserve
the same Project authority and snapshot-change fail-closed behavior.
