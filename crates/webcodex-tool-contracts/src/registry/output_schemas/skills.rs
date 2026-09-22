use serde_json::{json, Value};

use super::common::{
    array_schema, nullable_schema, schema_type, suggested_tool_call_schema, wrapped_output_schema,
};
use webcodex_core::skill_metadata::{MAX_SKILL_DESCRIPTION_CHARS, MAX_SKILL_NAME_CHARS};
use webcodex_core::skill_store::MAX_OPERATOR_SKILL_KEY_CHARS;

fn descriptor_schema() -> Value {
    // Skill descriptors intentionally never expose native Runner Skill root paths.
    json!({
        "type": "object",
        "properties": {
            "skill_id": {"type": "string", "pattern": "^wc_skill_[A-Za-z0-9_-]{21}[AQgw]$"},
            "name": {"type": "string", "maxLength": MAX_SKILL_NAME_CHARS},
            "description": {"type": "string", "maxLength": MAX_SKILL_DESCRIPTION_CHARS},
            "definition_revision": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "source_scope": {"type": "string", "enum": ["project", "runner"]},
            "trust": {"type": "string", "enum": ["project_content", "operator_configured_guidance", "operator_installed_guidance"]},
            "package_revision": {"anyOf": [{"type":"string","pattern":"^wc_skillpkg_[A-Za-z0-9_-]{43}$"},{"type":"null"}]},
            "name_conflict": {"type": "boolean"}
        },
        "required": ["skill_id", "name", "description", "definition_revision", "source_scope", "trust", "package_revision", "name_conflict"],
        "additionalProperties": false
    })
}

fn skill_source_summary_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "kind": {
                "type": "string",
                "enum": ["project", "configured_runner_roots", "managed_runner_store"]
            },
            "status": {
                "type": "string",
                "enum": ["available", "unavailable"]
            },
            "root_hint": {
                "type": "string",
                "const": ".agents/skills",
                "description": "Logical Project-relative root hint. Native Runner Skill root paths are never exposed."
            },
            "skill_count": {"type": "integer", "minimum": 0},
            "invalid_count": {"type": "integer", "minimum": 0},
            "discovery_truncated": {"type": "boolean"},
            "reason_code": {
                "type": "string",
                "enum": ["runner_skill_sources_unavailable"]
            }
        },
        "required": ["kind", "status", "skill_count", "invalid_count", "discovery_truncated"],
        "additionalProperties": false
    })
}

fn skill_load_candidate_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "skill_id": {"type": "string", "pattern": "^wc_skill_[A-Za-z0-9_-]{21}[AQgw]$"},
            "name": {"type": "string", "maxLength": MAX_SKILL_NAME_CHARS},
            "source_scope": {"type": "string", "enum": ["project", "runner"]},
            "trust": {"type": "string", "enum": ["project_content", "operator_configured_guidance", "operator_installed_guidance"]},
            "package_revision": {"anyOf": [{"type":"string","pattern":"^wc_skillpkg_[A-Za-z0-9_-]{43}$"},{"type":"null"}]},
            "definition_revision": {"type": "string", "pattern": "^[0-9a-f]{64}$"}
        },
        "required": ["skill_id", "name", "source_scope", "trust", "package_revision", "definition_revision"],
        "additionalProperties": false
    })
}

fn skill_versions_recovery_call_schema() -> Value {
    suggested_tool_call_schema(
        "skill_versions",
        json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "project": {"type": "string", "minLength": 1},
                "skill_key": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": MAX_OPERATOR_SKILL_KEY_CHARS,
                    "pattern": "^[A-Za-z0-9._-]+$"
                }
            },
            "required": ["project", "skill_key"]
        }),
        "Parser-ready advisory skill_versions reconciliation using only the exact Project and logical Skill key owned by the failed mutation. On Adaptive Runtime this target remains a discovered call_runtime_tool gateway action; the call grants no authority and is not mutation retry permission.",
    )
}

fn apply_skill_recovery_contract(name: &str, schema: &mut Value) {
    let mutation = matches!(
        name,
        "skill_install" | "skill_activate" | "skill_remove_revision"
    );
    {
        let properties = schema["properties"]["output"]["properties"]
            .as_object_mut()
            .expect("wrapped Skill output properties");
        if mutation {
            properties.insert(
                "suggested_call".to_string(),
                skill_versions_recovery_call_schema(),
            );
            properties.insert(
                "reconcile_with".to_string(),
                json!({
                    "type": "string",
                    "const": "skill_versions",
                    "description": "Non-actionable reconciliation-family hint used only when a complete safe skill_versions invocation cannot be proven. It grants no authority."
                }),
            );
        }
    }
    let output_all_of = schema["properties"]["output"]
        .as_object_mut()
        .expect("wrapped Skill output schema")
        .entry("allOf".to_string())
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .expect("Skill output allOf");
    output_all_of.push(json!({"not": {"required": ["recovery_tool"]}}));
    if mutation {
        output_all_of.extend([
            json!({
                "if": {"required": ["suggested_call"]},
                "then": {
                    "not": {"anyOf": [
                        {"required": ["reconcile_with"]},
                        {"required": ["recovery_kind"]}
                    ]}
                }
            }),
            json!({
                "if": {"required": ["reconcile_with"]},
                "then": {
                    "required": ["recovery_kind"],
                    "not": {"required": ["suggested_call"]},
                    "properties": {"recovery_kind": {"const": "reconcile"}}
                }
            }),
        ]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_trust_distinguishes_configured_and_managed_runner_guidance() {
        let schema = descriptor_schema();
        let values = schema["properties"]["trust"]["enum"]
            .as_array()
            .expect("trust enum");
        assert!(values.iter().any(|value| value == "project_content"));
        assert!(values
            .iter()
            .any(|value| value == "operator_configured_guidance"));
        assert!(values
            .iter()
            .any(|value| value == "operator_installed_guidance"));
    }

    #[test]
    fn skill_recovery_schemas_use_exact_call_or_family_only_without_legacy_alias() {
        for tool in [
            "skill_list",
            "skill_read_file",
            "skill_versions",
            "skill_install",
            "skill_activate",
            "skill_remove_revision",
        ] {
            let schema = output_schema_for_tool(tool).expect("Skill output schema");
            let properties = schema["properties"]["output"]["properties"]
                .as_object()
                .expect("Skill output properties");
            assert!(
                !properties.contains_key("recovery_tool"),
                "{tool} still publishes recovery_tool"
            );
            let all_of = schema["properties"]["output"]["allOf"]
                .as_array()
                .expect("Skill recovery constraints");
            assert!(all_of
                .iter()
                .any(|constraint| { constraint["not"]["required"] == json!(["recovery_tool"]) }));
        }

        for tool in ["skill_install", "skill_activate", "skill_remove_revision"] {
            let schema = output_schema_for_tool(tool).expect("Skill mutation output schema");
            let properties = schema["properties"]["output"]["properties"]
                .as_object()
                .expect("Skill mutation output properties");
            let suggested = &properties["suggested_call"];
            assert_eq!(suggested["properties"]["tool"]["const"], "skill_versions");
            assert_eq!(
                suggested["properties"]["arguments"]["required"],
                json!(["project", "skill_key"])
            );
            assert_eq!(
                suggested["properties"]["arguments"]["additionalProperties"],
                false
            );
            assert_eq!(properties["reconcile_with"]["const"], "skill_versions");
        }
    }
}

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    let mut schema = match name {
        "skill_load" => Some(wrapped_output_schema(vec![
            ("project", schema_type("string", "Resolved Project id.")),
            (
                "catalog_revision",
                schema_type("string", "Digest of the freshly observed bounded Skill catalog used for exact-name selection."),
            ),
            ("skill_id", schema_type("string", "Opaque project-scoped Skill identity.")),
            ("name", schema_type("string", "Selected Skill metadata name.")),
            ("source_scope", schema_type("string", "Selected Skill source: project or runner.")),
            ("trust", schema_type("string", "Guidance trust label for the selected source; never execution authority.")),
            (
                "package_revision",
                nullable_schema("string", "Active immutable whole-package revision for runner-installed Skills; null for project/configured Skills."),
            ),
            ("definition_revision", schema_type("string", "Current SKILL.md content digest.")),
            ("path", schema_type("string", "Package-relative SKILL.md path.")),
            ("sha256", schema_type("string", "Full current SKILL.md SHA-256.")),
            ("text", schema_type("string", "Bounded UTF-8 SKILL.md text.")),
            ("start_line", schema_type("integer", "Effective 1-based selected start line.")),
            ("end_line", nullable_schema("integer", "Last returned line, or null when none.")),
            ("returned_lines", schema_type("integer", "Returned SKILL.md source lines.")),
            ("has_more", schema_type("boolean", "Whether SKILL.md lines remain.")),
            ("next_start_line", nullable_schema("integer", "Continuation line when has_more.")),
            ("descriptor", descriptor_schema()),
            ("candidate_count", json!({"type":"integer","minimum":2,"description":"Total case-fold-equivalent exact-name candidates when selection is ambiguous."})),
            (
                "candidates",
                {
                    let mut schema = array_schema(
                        skill_load_candidate_schema(),
                        "At most eight bounded exact-name ambiguity candidates.",
                    );
                    schema["maxItems"] = json!(8);
                    schema
                },
            ),
            ("candidates_truncated", schema_type("boolean", "Whether more than eight ambiguity candidates exist.")),
            ("discovery_truncated", schema_type("boolean", "True when bounded catalog discovery was incomplete and exact-name uniqueness could not be proven.")),
            ("error_kind", schema_type("string", "Stable guard/error code on failure.")),
            ("state_changed", schema_type("boolean", "Always false for Skill loading failures.")),
        ])),
        "skill_list" => Some(wrapped_output_schema(vec![
            ("project", schema_type("string", "Resolved Project id.")),
            (
                "catalog_revision",
                schema_type(
                    "string",
                    "Digest of the freshly observed deterministic Skill catalog state.",
                ),
            ),
            (
                "total_count",
                schema_type("integer", "Valid Skills matching the optional query."),
            ),
            (
                "returned_count",
                schema_type("integer", "Descriptors returned in this page."),
            ),
            (
                "offset",
                schema_type(
                    "integer",
                    "Page offset within the filtered deterministic catalog.",
                ),
            ),
            (
                "next_offset",
                nullable_schema(
                    "integer",
                    "Next offset, or null when this page is complete.",
                ),
            ),
            (
                "truncated",
                schema_type("boolean", "Whether more filtered descriptors remain."),
            ),
            (
                "skills",
                array_schema(
                    descriptor_schema(),
                    "Lightweight Skill descriptors; never SKILL.md bodies.",
                ),
            ),
            (
                "sources",
                {
                    let mut schema = array_schema(
                        skill_source_summary_schema(),
                        "Fixed three-source discovery participation summary: Project .agents/skills, configured Runner roots, and the managed Runner store. Empty available sources mean discovery succeeded and found zero Skills; unavailable Runner sources carry only a stable reason code. Native Runner paths are never exposed.",
                    );
                    schema["minItems"] = json!(3);
                    schema["maxItems"] = json!(3);
                    schema
                },
            ),
            (
                "invalid_count",
                schema_type(
                    "integer",
                    "Malformed/invalid packages isolated from the valid catalog.",
                ),
            ),
            (
                "diagnostics",
                array_schema(
                    json!({"type":"object","additionalProperties":true}),
                    "Bounded reason-code-only invalid package diagnostics.",
                ),
            ),
            (
                "discovery_truncated",
                schema_type(
                    "boolean",
                    "Whether the hard discovery package ceiling was reached.",
                ),
            ),
            (
                "error_kind",
                schema_type("string", "Stable guard/error code on failure."),
            ),
            (
                "state_changed",
                schema_type("boolean", "Always false for Skill runtime failures."),
            ),
        ])),
        "skill_read_file" => Some(wrapped_output_schema(vec![
            ("project", schema_type("string", "Resolved Project id.")),
            (
                "skill_id",
                schema_type("string", "Opaque project-scoped Skill identity."),
            ),
            ("name", schema_type("string", "Skill metadata name.")),
            ("source_scope", schema_type("string", "Selected Skill source: project or runner.")),
            ("trust", schema_type("string", "Guidance trust label for the selected source; never execution authority.")),
            (
                "package_revision",
                nullable_schema(
                    "string",
                    "Active immutable whole-package revision for runner-installed Skills; null for project Skills.",
                ),
            ),
            (
                "definition_revision",
                schema_type(
                    "string",
                    "Current SKILL.md content digest only; not a package-tree revision.",
                ),
            ),
            (
                "path",
                schema_type("string", "Package-relative resource path."),
            ),
            (
                "sha256",
                schema_type("string", "Full current resource-file SHA-256."),
            ),
            (
                "text",
                schema_type("string", "Bounded UTF-8 selected text range."),
            ),
            (
                "start_line",
                schema_type("integer", "Effective 1-based selected start line."),
            ),
            (
                "end_line",
                nullable_schema("integer", "Last returned line, or null when none."),
            ),
            (
                "returned_lines",
                schema_type("integer", "Returned source lines."),
            ),
            (
                "has_more",
                schema_type("boolean", "Whether resource lines remain."),
            ),
            (
                "next_start_line",
                nullable_schema("integer", "Continuation line when has_more."),
            ),
            (
                "error_kind",
                schema_type("string", "Stable guard/error code on failure."),
            ),
            (
                "state_changed",
                schema_type("boolean", "Always false for Skill runtime failures."),
            ),
        ])),
        "skill_versions" => Some(wrapped_output_schema(vec![
            ("project", schema_type("string", "Resolved Project id.")),
            ("skill_id", schema_type("string", "Runner-scoped opaque Skill identity.")),
            ("skill_key", schema_type("string", "Stable logical operator Skill key.")),
            ("state_revision", schema_type("string", "CAS revision for installed/active state.")),
            ("active_package_revision", nullable_schema("string", "Currently active immutable package revision.")),
            ("total_count", schema_type("integer", "Installed immutable revision count.")),
            ("offset", schema_type("integer", "Returned page offset.")),
            ("next_offset", nullable_schema("integer", "Continuation offset.")),
            ("versions", array_schema(json!({
                "type":"object",
                "properties": {
                    "package_revision":{"type":"string"},
                    "definition_revision":{"type":"string"},
                    "name":{"type":"string"},
                    "description":{"type":"string"},
                    "file_count":{"type":"integer"},
                    "total_bytes":{"type":"integer"},
                    "installed_at_unix_ms":{"type":"integer"}
                },
                "additionalProperties": false
            }), "Bounded immutable revision metadata; never package bodies or native paths.")),
            ("error_kind", schema_type("string", "Stable error code on failure.")),
            ("state_changed", schema_type("boolean", "Always false for version observation.")),
        ])),

        "skill_install" => Some(wrapped_output_schema(vec![
            ("project", schema_type("string", "Authorized source Project id.")),
            ("skill_id", schema_type("string", "Runner-scoped opaque Skill identity.")),
            ("skill_key", schema_type("string", "Logical operator Skill key.")),
            ("package_revision", schema_type("string", "Immutable whole-package content revision.")),
            ("definition_revision", schema_type("string", "SKILL.md SHA-256 only.")),
            ("artifact_sha256", schema_type("string", "Verified source ZIP SHA-256.")),
            ("file_count", schema_type("integer", "Validated package file count.")),
            ("total_bytes", schema_type("integer", "Validated decompressed package bytes.")),
            ("installed", schema_type("boolean", "Whether this call committed a new immutable revision.")),
            ("activated", schema_type("boolean", "Whether this call changed active state.")),
            ("replayed", schema_type("boolean", "Whether the result reconciled the same idempotent intent.")),
            ("state_revision", schema_type("string", "Current CAS state revision.")),
            ("active_package_revision", nullable_schema("string", "Current active package revision.")),
            ("outcome_unknown", schema_type("boolean", "True only when dispatch may have executed but result cannot be reconciled yet.")),
            ("recovery_kind", schema_type("string", "reconcile when uncertain state requires observation.")),
            ("retry_same_idempotency_key", schema_type("boolean", "When true, any retry must reuse the same logical idempotency key; never invent a new key.")),
            ("error_kind", schema_type("string", "Stable error code on failure.")),
            ("state_changed", nullable_schema("boolean", "Observed mutation flag when outcome is known; null when the mutation outcome is unknown.")),
        ])),
        "skill_activate" => Some(wrapped_output_schema(vec![
            ("project", schema_type("string", "Resolved Project id.")),
            ("skill_id", schema_type("string", "Runner-scoped opaque Skill identity.")),
            ("skill_key", schema_type("string", "Logical operator Skill key.")),
            ("previous_active_package_revision", nullable_schema("string", "Previous active package revision.")),
            ("active_package_revision", schema_type("string", "Current active package revision.")),
            ("state_revision", schema_type("string", "Current CAS state revision.")),
            ("changed", schema_type("boolean", "Whether active state changed.")),
            ("replayed", schema_type("boolean", "Whether same-key reconciliation supplied this result.")),
            ("outcome_unknown", schema_type("boolean", "Whether dispatch outcome must be reconciled with the same key.")),
            ("recovery_kind", schema_type("string", "reconcile when uncertain state requires observation.")),
            ("retry_same_idempotency_key", schema_type("boolean", "When true, any retry must reuse the same logical idempotency key.")),
            ("error_kind", schema_type("string", "Stable error code on failure.")),
            ("state_changed", nullable_schema("boolean", "Observed mutation flag when outcome is known; null when the mutation outcome is unknown.")),
        ])),
        "skill_remove_revision" => Some(wrapped_output_schema(vec![
            ("project", schema_type("string", "Resolved Project id.")),
            ("skill_id", schema_type("string", "Runner-scoped opaque Skill identity.")),
            ("skill_key", schema_type("string", "Logical operator Skill key.")),
            ("package_revision", schema_type("string", "Target immutable package revision.")),
            ("state_revision", schema_type("string", "Current CAS state revision.")),
            ("removed", schema_type("boolean", "Whether the inactive revision was removed.")),
            ("replayed", schema_type("boolean", "Whether same-key reconciliation supplied this result.")),
            ("outcome_unknown", schema_type("boolean", "Whether dispatch outcome must be reconciled with the same key.")),
            ("recovery_kind", schema_type("string", "reconcile when uncertain state requires observation.")),
            ("retry_same_idempotency_key", schema_type("boolean", "When true, any retry must reuse the same logical idempotency key.")),
            ("error_kind", schema_type("string", "Stable error code on failure.")),
            ("state_changed", nullable_schema("boolean", "Observed mutation flag when outcome is known; null when the mutation outcome is unknown.")),
        ])),
        _ => None,
    }?;
    apply_skill_recovery_contract(name, &mut schema);
    Some(schema)
}
