# MCP display projection families

`src/mcp/presentation/registry.rs` is the single immutable inventory for the
existing eight tool names with bounded `webcodex/presentation` metadata. It
replaces a support-name predicate followed by a second string dispatch. Entries
pair exact tool names and pure `fn(&str, &Value) -> Option<Value>` projectors.
There is no wildcard, user registration, allocation of a registry, lock or RPC.

## Separate layers, separate responsibilities

This is not the ToolRuntime `ResultProjectionRegistry`, which shapes the final
model-facing result after canonical recording. It is not the MCP App registry,
which binds self-contained HTML resources to already-admitted descriptors. This
layer only derives bounded optional MCP display metadata from structured output.
Registration does not expose tools or create new cards.

The validation family now owns its run/summary registrations and all diagnostic,
failed-test, historical/current-evidence and recent-event formatting in
`presentation/validation.rs`. The existing Job and Git formatters remain in the
parent during this stage; their static registry entries invoke those same helpers.
They can be extracted independently without blocking deployment of this change.

## Preserved behavior

The previous eight names and six projector families are unchanged. Unsupported
names have no generic fallback. Missing or malformed output leaves the response
unchanged. Projection does not filter away failed tool results; uncertainty and
exact Job identity remain visible according to the existing field allowlist.

The renderer borrows output read-only. Attachment writes only the existing
`_meta["webcodex/presentation"]` key, preserving structuredContent, content,
isError and unrelated metadata. It does not gain access to ToolRuntime, Session,
credentials, execution, retry control or canonical audit. The 8-item diagnostic
budget is shared with failed tests; text remains Unicode-safe and bounded. Summary
events keep the latest eight entries in their original chronological order.
Current stale/unknown evidence remains distinct from historical success.

This extraction preserves existing formatter bodies. It does not introduce a
public ResultModifier API, change JSON schemas or HTML templates, advance App
URIs, remove cached Changes support, or alter Native Tool Plugin semantics.

## Validation

Registry tests check exact/unique ownership, unsupported/malformed no-op behavior,
canonical result and unrelated metadata preservation, diagnostic/text bounds,
and current evidence plus chronological summary truncation. Existing MCP Result
App integration tests remain the source for real framing, App policy, Jobs,
validation and Git display contracts. The independent Work Result section PR
can land before or after this one; their changed paths do not overlap.
