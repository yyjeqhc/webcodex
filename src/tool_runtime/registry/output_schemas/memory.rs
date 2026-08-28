use serde_json::{json, Value};

use super::common::{array_schema, nullable_schema, schema_type, wrapped_output_schema};
use crate::db::{MAX_MEMORY_SUMMARY_CHARS, MAX_MEMORY_TAGS, MAX_MEMORY_TAG_CHARS};

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
        _ => None,
    }
}
