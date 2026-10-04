# Tool Request Tracing — Maintainer Forensics

This page is the maintainer-level contract for WebCodex tool-request tracing.
Ordinary operators should start with
[Troubleshooting](../TROUBLESHOOTING.md#capture-one-failing-tool-call), reproduce
the problem once, and only use the details below when field-by-field correlation
is actually required.

## Enable a bounded capture

Tracing is disabled by default. `metadata` now persists lifecycle events and
bounded operator diagnostic details for selected development tools; it is no
longer journal-only. It retains actual paths, selection parameters and bounded
command/argv text, while omitting large bodies and credential-labelled fields.
`full` additionally captures request/response payloads. This is operator data,
not the safe business-audit projection; arbitrary inline secrets in commands or
arguments are not guaranteed to be redacted.

```text
WEBCODEX_TOOL_REQUEST_TRACE=metadata
WEBCODEX_TOOL_REQUEST_TRACE_DIR=/var/lib/webcodex/tool-request-traces
WEBCODEX_TOOL_REQUEST_TRACE_RETENTION_HOURS=168
WEBCODEX_TOOL_REQUEST_TRACE_MAX_TOTAL_BYTES=2147483648
```

Apply the Server environment change using the normal deployment lifecycle.
Use `full` only when the omitted body is necessary to reproduce a specific
problem, and disable it again after capture. Changing this switch does not
reconstruct earlier omitted data. Already-retained metadata and payloads remain
readable by an administrator when capture is off.

In WebUI, open Work / Windows and use **Find calls by time or Project**. A Window
hash is optional. Calls in the existing Window activity view also have an explicit
**Inspect call diagnostics** action. Listing and expansion never automatically
fetch full payloads or introduce a refresh timer. The shared MCP reader remains
the admin-only `read_tool_trace` operator extension, behind its existing protocol
and authority checks; no new Direct tool is added.

Examples of `read_tool_trace` arguments:

```json
{"query":{"project":"agent:xa:webcodex","tool_name":"work_on_project"},"limit":20}
```

```json
{"trace_ref":"<exact-server-trace-uuid>","limit":20}
```

```json
{"trace_ref":"<exact-server-trace-uuid>","payload_index":0}
```

Use either query filters or an exact trace, not both. Default query range is the
last 24 hours; explicit Unix-millisecond ranges may span at most 31 days. Copy
the returned effective `query` and `next_offset` when paging. The retained
ActionAudit index is live: late inserts and retention can change later pages;
a fixed time range is not a transaction held across requests. Window summaries
cover the returned page only. Missing Host Window identity stays missing.

The default trace listing returns lifecycle/diagnostic **events**, including
payload-index entries when full data exists. A full payload is fetched only by
an explicit `payload_index` read; its existing size/digest/private-file checks
remain enforced. Unknown or malformed selectors never authorize another read.

## Execution outcome and declared expectations

Window activity in WebUI and the MCP Work Result card keeps the raw action
`status` separate from the optional `failure_expectation_result`. Dispatch uses
the Workflow Session ledger's canonical matcher and retains its classification
in the ActionAudit summary, even without a recording Session. Single-result
MCP App presentation metadata carries the same classification.

- `matched_expected_failure` and `matched_expected_result` display **Expected result**
  alongside the raw outcome, for example **Failed · Expected result**.
- `expectation_mismatch`, `unexpected_success`, and `unexpected_failure` display
  **Expectation not met**. Unexpected success does not get a green success badge.
- Calls without declared expectations, pending Job handoffs, and old records
  without classification retain their ordinary status. Missing evidence is never
  reconstructed from error text or a later call's expectation.

This display projection does not change `ToolResult.success`, MCP Host
compatibility behavior, retry authority, validation evidence, or Job lifecycle.
A Job's eventual result remains separate from its initiating call's handoff.

## Server trace layout

Each captured request has a Server-generated `server_trace_id`. Both enabled
modes use the existing private, bounded trace directory and background writer.
Metadata writes only `events.jsonl`; full can additionally write compressed JSON
payloads. The following raw file commands are operator-only, for example:

```bash
TRACE_ROOT=/var/lib/webcodex/tool-request-traces
find "$TRACE_ROOT/<server_trace_id>" -maxdepth 2 -type f -print
cat "$TRACE_ROOT/<server_trace_id>/events.jsonl"
zstd -dc "$TRACE_ROOT/<server_trace_id>/payloads/<payload>.json.zst" | jq .
```

For a model tool call, the useful payload layers are:

- `raw_request_body` for the HTTP API, or `raw_arguments` for MCP: what reached
  the Server before wrapper/session normalization;
- `effective_arguments`: the semantic argument object passed into the
  runtime/Connector dispatch;
- `runner_request`: the exact typed request enqueued to the selected Runner when
  dispatch reached that boundary;
- `final_response`: the bounded JSON response generated by the Server when one
  exists.

These layers let a maintainer determine where a field disappeared or changed.
For example, a caller-visible wrapper field such as
`context_request` can be compared between the raw and effective
argument layers without making that field part of ordinary user troubleshooting.

## Metadata evidence stages

- `mcp_request_policy_selected`: the MCP adapter's validated effective profile,
  Host budget and wait slices, with separate `deployment`/`request_header` sources
  and a parsed requested-budget/cap comparison. No raw headers or brand inference.
  This is Server selection evidence, not observed Host execution or delivery.
- `supplied_arguments`: selected bounded arguments and `_wc` before MCP envelope
  parsing; gateway entry and selected target remain distinct. `_wc` is captured
  even on other ordinary tools, without copying their business payload. Suppressed
  trace-reader/continuation tools keep their existing sensitive-data exclusions.
- `kernel_arguments`: the business arguments entering the canonical parser after
  adapter envelope processing. This is not a claim that parsing, authorization,
  execution or all later normalization succeeded.
- `execution_evidence`: producer-reported normalization, effect, execution state,
  Job and effective timing fields before model compaction, when available.
- `response_summary`: bounded final returned selectors/outcomes and context
  receipts. Instructions record source path/scope/fingerprint, content inclusion,
  returned byte count, truncation and continuation, not instruction content.
  Missing materials keep their actual reason (including budget omission).

Each diagnostic is at most 8 KiB, with bounded nodes, depth, fields, items and
UTF-8 text previews. Large script/stdin/file/patch/text/log bodies are omitted
without serializing them merely to find their size. An explicit truncation or
omission marker is not an empty value. There is no guarantee of complete args
when the bound is reached. A fingerprint is identity evidence, not recoverable
content. Returned instruction material does not prove model reading/compliance.

The current implementation does not add per-Window full-capture arming, automatic
failure-triggered capture, or retroactive payload reconstruction. Those require
an independently designed operator control boundary; ordinary diagnostics stay
read-only. Details: [bounded metadata diagnostics](../implementation/bounded-meta-trace-diagnostics.md).

## Runner correlation

When a request reaches a Runner, `events.jsonl` records the existing
`runner_request_id` together with bounded Runner/transport/build metadata. If
Runner-side tool tracing is enabled, search the Runner journal for that same
request id. Later Runner results and Job updates are correlated back to the
originating Server trace through the bounded in-memory correlation index.

The correlation identifiers are diagnostics only. They are not authority,
execution identity, Session continuity, or retry permission.

## Missing or omitted payloads

Absence of a payload file does not prove that the payload was empty or truncated.
Check the Server journal for trace-capture diagnostics. In particular:

- `tool_trace_capture_omitted` + `trace_writer_queue_full` means the bounded
  background writer queue was saturated;
- `tool_trace_capture_omitted` + `trace_disk_budget_exceeded` means the configured
  retained-byte budget could not admit the capture after pruning;
- `tool_trace_capture_failed` means trace persistence/correlation failed for a
  diagnostic reason.

Trace failures are fail-open for the underlying tool call: they reduce forensic
evidence but do not redefine the tool result.

Diagnostic reads return `capture_health` counters for queue/budget drops and write
failures since this Server process started. These aggregate counters neither
attribute a loss to one trace nor survive process replacement. A missing capture
returns `trace_not_retained` with possible causes instead of claiming execution
never happened. Missing handoff events are likewise inconclusive. Legacy
metadata captured only in journals is not backfilled into the file index.

## Delivery boundary

`tool_handler_returned` proves only that WebCodex handed a response to the HTTP
framework. It is not proof that the remote client received it. When the Server
trace ends cleanly but the client reports no response, correlate the request time
with the reverse-proxy/access logs before treating the call as a runtime failure.

## Sensitive data

Full tracing is raw forensic capture. The trace path does not read the WebCodex
ingress HTTP `Authorization` header, but a token, key, source fragment, patch,
script/stdin value, command output, user message, or other secret that is itself
present inside a captured tool/Runner payload can appear in the trace.

Treat the trace directory as sensitive diagnostic data. Do not attach raw traces
to public issues or chat without reviewing/redacting them first. Keep retention
and total-byte budgets bounded.

## Documentation boundary

Public troubleshooting should document only the operator action: enable tracing,
reproduce once, locate the capture, protect it, and escalate when needed. Exact
payload names, request ids, correlation rules, queue/disk omission codes, and
wire-normalization comparisons belong on this maintainer page, in tests, or in
the implementation that validates them.
