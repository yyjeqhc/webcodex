# Conversation attachment import

Use `import_host_files` with current host-populated file
references. Do not construct attachment URLs, infer host file IDs, or copy
signed download URLs into logs. Import runs on Control and commits through the
bounded Runner artifact-upload protocol; it does not edit documents or create
Markdown links.

## Create references after confirmed import

1. Await the terminal import result. Inspect `output.imported` (also exposed as
   `output.succeeded`). Each completed item contains `project`, `path`,
   `bytes_written`, `sha256`, `mime_type`, and `source_name`.
2. Verify the completed destination with `inspect_project_artifact(action=metadata)`;
   its existence, byte count and SHA-256 must match the completed item. Use the
   returned path, including any normalized file name, rather than predicting it.
   Metadata verification requires its own Project read authority; if it is
   unavailable, report verification as blocked rather than assuming existence.
3. Only then write the document reference using the document's relative-path
   conventions. Document edits have their own write authority and revision
   fences; import does not authorize a separate document edit.

A batch is non-atomic. If a later item fails, the overall result can fail while
`imported`/`succeeded` retain prior committed items. `partial_success=true`,
`failed_item`, `failure_reason`, and `failure` describe that outcome. Preserve
the verified successes; never turn the failed item into a link target. An upload
whose result is uncertain is not a confirmed success: reconcile its existing
upload state before retrying, and avoid blindly replaying the entire batch.
Do not equate a document link, a host download response, or `upload_begin`
success with a committed destination.

## Diagnose failures without exposing attachments

At WARN level, `mcp_host_file_import_failed` reports fixed `stage` and
`failure_class` fields. Stages are `dns`, `download_request`, `download_body`,
`upload_begin`, `upload_chunk`, and `upload_finish`. Classes distinguish
`timeout`, `connection_failed`, `failed`, `http_status`, `size_limit`, and
`protocol_mismatch`. An HTTP rejection includes only the numeric `http_status`.
When a current request trace is available and parses as a UUID, it is emitted
as the canonical `server_trace_id`; absent or invalid trace IDs are omitted.

These events contain no URL, path, query, file name, raw error text, upload ID,
credentials, or attachment contents. They diagnose a failure location, not a
root cause. Upload failures retain the existing definite-versus-uncertain
cleanup policy; a diagnostic event grants no authority to retry or abort.

`mcp_host_file_download_dns_rejected` separately reports a bounded DNS host,
address classes, and a safe request UUID. No raw resolved addresses or signed
URL are emitted. Empty results, mixed public/non-public answers, loopback,
link-local and other non-public answers remain rejected. Trust rejection has
its own `mcp_host_file_import_trust_decision` event. Correlate the request and
the failing stage before changing configuration.

## Fake-IP DNS troubleshooting

Some DNS/proxy tools return synthetic addresses in the benchmarking range
`198.18.0.0/15`. These are non-public addresses. A `benchmarking` class can
indicate Fake-IP DNS but does not prove which resolver or rule produced it.
Keep WebCodex's SSRF rejection intact; do not permit this range, disable DNS
checks, remove address pinning, enable redirects, or widen credential trust to
work around DNS results.

An operator may inspect DNS answers for the exact host in the rejection event
through the configured system resolver. Check A and AAAA answers and every
resolver layer actually used by Control. Use only the host name; do not paste a
signed URL into shell history or DNS tools. A host such as
`oaisdmntpr<region>.blob.core.windows.net` illustrates the narrow Sediment
account pattern; it is not a download URL or a rule to apply literally.

If the resolver supports a documented Fake-IP exclusion, propose an exact-host
exception for the observed attachment host. Apply it only with the operator's
authorization, preserve a rollback copy, validate configuration using the
resolver's native checker, and follow its documented reload procedure. Do not
exclude all Azure Blob storage, all OpenAI traffic, or all HTTPS hosts. Changes
to shared DNS affect other clients and require the appropriate operator scope.

After an authorized resolver change, confirm that all A/AAAA answers used by
Control are public, retry the same authorized import with a fresh host reference
if necessary, and verify the committed destination metadata before linking it.
A public answer only passes DNS admission; HTTP expiry, upload authority and
write failures remain possible. Preserve HTTPS, port 443, no userinfo, no
redirects, public-address checks, pinning, normal PAT/OAuth trust gates, and
bounded streaming throughout.
