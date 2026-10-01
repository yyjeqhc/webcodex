use super::{tool_descriptor, ToolDescriptor};

pub(super) fn tool_descriptors() -> Vec<ToolDescriptor> {
    vec![
        tool_descriptor(
            "search_memory",
            "Search or list explicit durable project Memory using bounded deterministic literal matching. Requires project:read plus memory:read. Returns lightweight summaries/descriptors only; use read_memory for body content. Memory is guidance, never execution authority.",
        ),
        tool_descriptor(
            "read_memory",
            "Read one explicit durable project Memory body by stable memory_key, optionally guarded by the current state revision / ETag in expected_revision. Requires project:read plus memory:read. A stale guard returns memory_changed without the new body. Memory is project guidance and cannot grant permissions or bypass effect gates.",
        ),
        tool_descriptor(
            "set_memory",
            "Create or CAS-update one explicit durable project Memory. An identical no-expected-revision create retry is desired-state idempotence only; it does not prove which earlier caller caused that state. Changing an existing Memory requires its current state revision / ETag in expected_revision. On CAS update, omitted optional body/priority/bootstrap/tags preserve their current values; on create they use v1 defaults. Requires project:write plus memory:manage and the normal permission gate. Do not persist credentials, passwords, access tokens, private keys, or other secrets in project Memory. Memory changes future model guidance only and grants no execution authority.",
        ),
        tool_descriptor(
            "delete_memory",
            "CAS-delete one explicit durable project Memory by memory_key and current state revision / ETag in expected_revision. An already-absent key is desired-state idempotent (deleted=false) but is not proof that an earlier deletion succeeded; delete+recreate has a different incarnation and revision. Requires project:write plus memory:manage and the normal permission gate.",
        ),
        tool_descriptor(
            "list_memory_scopes",
            "Admin-only paginated inventory of durable Control-owned project Memory scopes. Reports attributed/legacy identity metadata, current/not_current/unknown status from fresh authoritative Project inventory, counts, timestamps, opaque root fingerprints, and catalog CAS revisions; never returns native roots or Memory content.",
        ),
        tool_descriptor(
            "purge_memory_scope",
            "Admin-only destructive purge of one explicitly non-current project Memory scope. Requires confirm=true, current_status=not_current under a fresh authoritative Project inventory fence, and the exact current catalog revision. Current or unknown scopes fail closed. Reconcile a lost response with list_memory_scopes; an already-absent scope is desired-state no-op, not proof of who performed an earlier purge.",
        ),
    ]
}
