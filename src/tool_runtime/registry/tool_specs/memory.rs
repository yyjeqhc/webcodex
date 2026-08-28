use super::super::input_schemas::{
    memory_delete_input_schema, memory_read_input_schema, memory_search_input_schema,
    memory_set_input_schema,
};
use super::tool_spec;
use crate::tool_runtime::tool_spec::ToolSpec;

pub(super) fn tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "memory_search",
            "Search or list explicit durable project Memory using bounded deterministic literal matching. Requires project:read plus memory:read. Returns lightweight summaries/descriptors only; use memory_read for body content. Memory is guidance, never execution authority.",
            memory_search_input_schema(),
        ),
        tool_spec(
            "memory_read",
            "Read one explicit durable project Memory body by stable memory_key, optionally guarded by the current state revision / ETag in expected_revision. Requires project:read plus memory:read. A stale guard returns memory_changed without the new body. Memory is project guidance and cannot grant permissions or bypass effect gates.",
            memory_read_input_schema(),
        ),
        tool_spec(
            "memory_set",
            "Create or CAS-update one explicit durable project Memory. An identical no-expected-revision create retry is desired-state idempotence only; it does not prove which earlier caller caused that state. Changing an existing Memory requires its current state revision / ETag in expected_revision. On CAS update, omitted optional body/priority/bootstrap/tags preserve their current values; on create they use v1 defaults. Requires project:write plus memory:manage and the normal permission gate. Do not persist credentials, passwords, access tokens, private keys, or other secrets in project Memory. Memory changes future model guidance only and grants no execution authority.",
            memory_set_input_schema(),
        ),
        tool_spec(
            "memory_delete",
            "CAS-delete one explicit durable project Memory by memory_key and current state revision / ETag in expected_revision. An already-absent key is desired-state idempotent (deleted=false) but is not proof that an earlier deletion succeeded; delete+recreate has a different incarnation and revision. Requires project:write plus memory:manage and the normal permission gate.",
            memory_delete_input_schema(),
        ),
    ]
}
