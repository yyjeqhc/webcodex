use serde_json::{json, Value};

use super::common::{array_schema, nullable_schema, schema_type, wrapped_output_schema};
use webcodex_core::memory_contract::{
    MAX_MEMORY_SUMMARY_CHARS, MAX_MEMORY_TAGS, MAX_MEMORY_TAG_CHARS,
};

fn descriptor_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "memory_id": {"type":"string","pattern":"^wc_mem_[0-9a-f]{32}$"},
            "memory_key": {"type":"string"},
            "summary": {"type":"string","maxLength":MAX_MEMORY_SUMMARY_CHARS},
            "priority": {"type":"string","enum":["high","normal","low"]},
            "bootstrap": {"type":"boolean"},
            "tags": {"type":"array","maxItems":MAX_MEMORY_TAGS,"items":{"type":"string","maxLength":MAX_MEMORY_TAG_CHARS}},
            "revision": {"type":"string","pattern":"^wc_memrev_[0-9a-f]{64}$"},
            "matched_fields": {"type":"array","items":{"type":"string","enum":["memory_key","summary","body","tags"]}}
        },
        "required": ["memory_id","memory_key","summary","priority","bootstrap","tags","revision"],
        "additionalProperties": false
    })
}

fn provenance_schema() -> Value {
    json!({
        "type":"object",
        "properties": {
            "created_by_kind": {"type":"string"},
            "updated_by_kind": {"type":"string"}
        },
        "required":["created_by_kind","updated_by_kind"],
        "additionalProperties":false,
        "description":"Coarse durable attribution only. Principal digests and raw principal identities are never exposed here."
    })
}

fn memory_scope_descriptor_schema() -> Value {
    json!({
        "type":"object",
        "properties": {
            "memory_scope_id":{"type":"string","pattern":"^wc_memscope_[0-9a-f]{64}$"},
            "identity_state":{"type":"string","enum":["attributed"]},
            "current_status":{"type":"string","enum":["current","not_current","unknown"]},
            "project_runtime_id":{"type":["string","null"]},
            "runner_client_id":{"type":["string","null"]},
            "root_fingerprint":{"type":["string","null"],"pattern":"^wc_memroot_[0-9a-f]{64}$"},
            "current_project_runtime_id":{"type":["string","null"]},
            "memory_count":{"type":"integer","minimum":1},
            "bootstrap_count":{"type":"integer","minimum":0},
            "catalog_revision":{"type":"string","pattern":"^wc_memcat_[0-9a-f]{64}$"},
            "oldest_memory_created_at_unix_ms":{"type":"integer"},
            "latest_memory_updated_at_unix_ms":{"type":"integer"},
            "scope_created_at_unix_ms":{"type":"integer"},
            "scope_last_mutated_at_unix_ms":{"type":"integer"}
        },
        "required":["memory_scope_id","identity_state","current_status","project_runtime_id","runner_client_id","root_fingerprint","current_project_runtime_id","memory_count","bootstrap_count","catalog_revision","oldest_memory_created_at_unix_ms","latest_memory_updated_at_unix_ms","scope_created_at_unix_ms","scope_last_mutated_at_unix_ms"],
        "additionalProperties":false
    })
}

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    match name {
        "memory_search" => Some(wrapped_output_schema(vec![
            ("project", schema_type("string", "Resolved Project id.")),
            (
                "catalog_revision",
                schema_type(
                    "string",
                    "Digest of sorted current project Memory key/revision pairs.",
                ),
            ),
            (
                "total_count",
                schema_type("integer", "Matching Memory count."),
            ),
            (
                "returned_count",
                schema_type("integer", "Descriptors returned in this bounded page."),
            ),
            ("offset", schema_type("integer", "Effective page offset.")),
            (
                "next_offset",
                nullable_schema("integer", "Next offset when more results remain."),
            ),
            (
                "truncated",
                schema_type("boolean", "Whether more matching descriptors remain."),
            ),
            (
                "memories",
                array_schema(
                    descriptor_schema(),
                    "Lightweight Memory descriptors; never body content.",
                ),
            ),
            (
                "error_kind",
                schema_type("string", "Stable error/guard code."),
            ),
            (
                "current_revision",
                schema_type(
                    "string",
                    "Current Memory revision when a CAS guard is stale.",
                ),
            ),
            (
                "state_changed",
                schema_type("boolean", "Always false for search failures."),
            ),
        ])),
        "memory_read" => Some(wrapped_output_schema(vec![
            ("project", schema_type("string", "Resolved Project id.")),
            (
                "memory_id",
                schema_type("string", "Opaque identity of the current Memory incarnation. Delete plus recreate produces a new memory_id."),
            ),
            (
                "memory_key",
                schema_type("string", "Stable project-scoped semantic key."),
            ),
            (
                "summary",
                schema_type("string", "Lightweight guidance summary."),
            ),
            (
                "body",
                schema_type(
                    "string",
                    "Bounded durable Memory body; guidance only, never execution authority.",
                ),
            ),
            ("priority", schema_type("string", "high, normal, or low; used only for ordering Memory entries within bootstrap, never trust or authority.")),
            (
                "bootstrap",
                schema_type(
                    "boolean",
                    "Eligibility for explicit memory.bootstrap projection.",
                ),
            ),
            (
                "tags",
                array_schema(schema_type("string", "Memory tag."), "Bounded tags."),
            ),
            (
                "revision",
                schema_type(
                    "string",
                    "Current Memory state revision / ETag used for CAS. It changes for each real incarnation generation even if content later returns to an earlier definition.",
                ),
            ),
            (
                "created_at_unix_ms",
                schema_type("integer", "Durable creation timestamp."),
            ),
            (
                "updated_at_unix_ms",
                schema_type("integer", "Last changed timestamp."),
            ),
            (
                "provenance",
                provenance_schema(),
            ),
            (
                "error_kind",
                schema_type("string", "Stable error/guard code."),
            ),
            (
                "current_revision",
                schema_type("string", "Current revision on stale expected_revision."),
            ),
            (
                "state_changed",
                schema_type("boolean", "Always false for read failures."),
            ),
        ])),
        "memory_set" => Some(wrapped_output_schema(vec![
            ("project", schema_type("string", "Resolved Project id.")),
            (
                "memory_id",
                schema_type("string", "Opaque identity of the current Memory incarnation."),
            ),
            (
                "memory_key",
                schema_type("string", "Stable project-scoped semantic key."),
            ),
            (
                "old_revision",
                nullable_schema("string", "Previous revision when content changed."),
            ),
            (
                "revision",
                schema_type("string", "Current Memory state revision / ETag for CAS."),
            ),
            (
                "created",
                schema_type("boolean", "Whether a new durable Memory row was created."),
            ),
            (
                "state_changed",
                schema_type(
                    "boolean",
                    "Whether durable model-relevant Memory state changed.",
                ),
            ),
            (
                "error_kind",
                schema_type("string", "Stable error/CAS/capacity code."),
            ),
            (
                "current_revision",
                schema_type(
                    "string",
                    "Current revision when an update guard is missing or stale.",
                ),
            ),
        ])),
        "memory_delete" => Some(wrapped_output_schema(vec![
            ("project", schema_type("string", "Resolved Project id.")),
            (
                "memory_id",
                nullable_schema(
                    "string",
                    "Deleted Memory identity, or null when already absent.",
                ),
            ),
            ("memory_key", schema_type("string", "Requested Memory key.")),
            (
                "revision",
                nullable_schema("string", "Deleted revision, or null when already absent."),
            ),
            (
                "deleted",
                schema_type("boolean", "Whether this call deleted the current Memory."),
            ),
            (
                "state_changed",
                schema_type("boolean", "Whether durable Memory state changed."),
            ),
            (
                "error_kind",
                schema_type("string", "Stable error/CAS code."),
            ),
            (
                "current_revision",
                schema_type("string", "Current revision on stale delete CAS."),
            ),
        ])),
        "memory_scope_list" => Some(wrapped_output_schema(vec![
            ("total_count", schema_type("integer", "Total durable Memory scope count.")),
            ("returned_count", schema_type("integer", "Scope descriptors returned in this page.")),
            ("offset", schema_type("integer", "Effective page offset.")),
            ("next_offset", nullable_schema("integer", "Next offset when more scopes remain.")),
            ("truncated", schema_type("boolean", "Whether more scopes remain.")),
            ("scopes", array_schema(memory_scope_descriptor_schema(), "Bounded operator scope metadata; never native roots or Memory content.")),
            ("error_kind", schema_type("string", "Stable lifecycle/store error code.")),
            ("state_changed", schema_type("boolean", "Always false for list failures.")),
        ])),
        "memory_scope_purge" => Some(wrapped_output_schema(vec![
            ("memory_scope_id", schema_type("string", "Opaque Memory scope identity.")),
            ("catalog_revision", nullable_schema("string", "Purged catalog revision, or null when already absent.")),
            ("current_catalog_revision", schema_type("string", "Current catalog revision when the CAS fence is stale.")),
            ("purged_count", schema_type("integer", "Number of Memory rows atomically deleted.")),
            ("purged", schema_type("boolean", "Whether this call purged an existing scope.")),
            ("state_changed", schema_type("boolean", "Whether durable Memory state changed.")),
            ("error_kind", schema_type("string", "Stable lifecycle/CAS/safety error code.")),
        ])),
        _ => None,
    }
}
