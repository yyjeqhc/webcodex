use serde_json::{json, Value};

use super::common::{
    array_schema, authority_profile_schema, nullable_schema, open_object_schema, schema_type,
    wrapped_output_schema,
};

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    match name {
        "runtime_status" => Some(wrapped_output_schema(vec![
            ("service", schema_type("string", "Runtime service name.")),
            (
                "model_surface",
                schema_type(
                    "string",
                    "Configured MCP model surface: canonical_connector, local_coding, adaptive_runtime, or full_operator_runtime.",
                ),
            ),
            (
                "mcp_compact_schemas",
                schema_type(
                    "boolean",
                    "Whether MCP tools/list omits outputSchema while retaining tool names, descriptions, inputSchema, and annotations. This is MCP discovery schema compaction, not runtime_status compact=true response shaping or GPT Action response compaction.",
                ),
            ),
            (
                "effective_config",
                json!({
                    "type": "object",
                    "description": "Safe allowlisted effective configuration of the running Server. This is distinct from runtime_status compact=true response shaping and from health or transport state.",
                    "additionalProperties": false,
                    "properties": {
                        "auth": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {
                                "shared_key_enabled": {"type": "boolean", "description": "Whether direct shared-key quick-start authentication is effective for the running Server; false on the project-bound canonical Connector surface."},
                                "anonymous_enabled": {"type": "boolean", "description": "Whether explicit open-anonymous access is effective for the running Server; false on the project-bound canonical Connector surface."},
                                "oauth2_enabled": {"type": "boolean", "description": "Whether OAuth2 support was enabled in the running Server configuration."},
                                "oauth2_shared_key_bridge_enabled": {"type": "boolean", "description": "Whether the OAuth2 shared-key bridge is enabled in the running OAuth2 configuration; false whenever OAuth2 itself is disabled. This public OAuth flow is distinct from direct Bearer shared-key authentication."}
                            },
                            "required": ["shared_key_enabled", "anonymous_enabled", "oauth2_enabled", "oauth2_shared_key_bridge_enabled"]
                        },
                        "tool_request_trace_mode": {
                            "type": "string",
                            "enum": ["off", "metadata", "full"],
                            "description": "Effective bounded tool-request trace mode; no trace paths, request bodies, headers, or environment values are exposed."
                        }
                    },
                    "required": ["auth", "tool_request_trace_mode"]
                }),
            ),
            ("version", schema_type("string", "Runtime version.")),
            (
                "focus",
                open_object_schema("Exact focused Runner status when client_id is supplied."),
            ),
            (
                "server",
                open_object_schema("Server build/version identity in focused mode."),
            ),
            (
                "fleet_summary",
                open_object_schema("Secondary aggregate fleet mismatch counts in focused mode; never overrides focus truth."),
            ),
            (
                "build",
                open_object_schema("Build revision metadata for the running binary."),
            ),
            ("server_time", schema_type("integer", "Server timestamp.")),
            ("pid", schema_type("integer", "Server process id.")),
            (
                "auth_enabled",
                schema_type("boolean", "Whether bearer auth is enabled."),
            ),
            (
                "configured_public_url",
                nullable_schema("string", "Configured public URL, when set."),
            ),
            (
                "projects",
                open_object_schema("Project counts from the agent registry. Prefer projects.effective for model-facing status."),
            ),
            (
                "agents",
                open_object_schema("Agent counts and client summaries. Per-client host_context is bounded Runner-configured advisory data, not observed truth or authority. job_concurrency contains the static Runner limit plus caller-visible running and queued counts. Canonical top-level counts are count, online_count, and stale_count in full, compact, and summary_only output."),
            ),
            (
                "jobs",
                open_object_schema("Bounded runtime job counts, including active_count, running_count, queued_count, recovering_count, reconciled_count, and lost_after_reconcile_count."),
            ),
            ("tools", open_object_schema("Runtime tool counts and names.")),
            (
                "authority",
                authority_profile_schema("Canonical authority profile. trusted_agent is the self-hosted single-operator default and does not bypass hard safety checks (scopes, project boundary, read-only sessions, path policy)."),
            ),
            (
                "quic",
                open_object_schema("QUIC transport status, when enabled."),
            ),
        ])),
        "list_projects" => Some(wrapped_output_schema(vec![
            (
                "projects",
                array_schema(open_object_schema("Project summary including capabilities.git_available, supports_cleanup_verification, and recommended_for_smoke."), "Runtime projects."),
            ),
            ("count", schema_type("integer", "Project count.")),
            (
                "matched_count",
                schema_type("integer", "Caller-visible Project count matching filters before limit."),
            ),
            (
                "truncated",
                schema_type("boolean", "Whether limit truncated matching Projects."),
            ),
            (
                "recommended_for_smoke",
                array_schema(
                    schema_type("string", "Runtime project id recommended for smoke tests."),
                    "Runtime project ids whose capabilities.recommended_for_smoke is true.",
                ),
            ),
        ])),
        "list_agents" => Some(wrapped_output_schema(vec![
            (
                "agents",
                array_schema(open_object_schema("Agent summary including bounded Runner-configured host_context advisory data, never authority or proof of current state, plus job_concurrency limit/running/queued facts."), "Agent summaries."),
            ),
            (
                "clients",
                array_schema(open_object_schema("Client summary including job_concurrency limit/running/queued facts."), "Client summaries."),
            ),
            ("count", schema_type("integer", "Agent/client count.")),
        ])),
        "list_tools" => Some(wrapped_output_schema(vec![
            (
                "tools",
                array_schema(
                    open_object_schema("Tool metadata or compact summary."),
                    "Runtime tool specs, or compact summaries when summary_only is true.",
                ),
            ),
            (
                "names",
                array_schema(schema_type("string", "Tool name."), "Returned tool names."),
            ),
            ("count", schema_type("integer", "Tool count.")),
            (
                "returned_count",
                schema_type("integer", "Returned tool count after filters and limits."),
            ),
            (
                "total_count",
                schema_type("integer", "Total number of visible runtime tools."),
            ),
            (
                "filtered_count",
                schema_type("integer", "Number of tools matching category/features before limit."),
            ),
            (
                "truncated",
                schema_type("boolean", "Whether limit truncated the matching tools."),
            ),
            (
                "truncation_reason",
                nullable_schema("string", "Reason for truncation, such as limit, or null."),
            ),
            (
                "limit_applied",
                schema_type("boolean", "Whether a caller-supplied limit was applied."),
            ),
            (
                "requested_limit",
                nullable_schema("integer", "Caller-supplied limit before effective cap, or null."),
            ),
            (
                "category",
                nullable_schema("string", "Requested category filter, or null."),
            ),
            (
                "features",
                nullable_schema("string", "Requested loose feature filter, or null."),
            ),
            (
                "limit",
                nullable_schema("integer", "Effective focused discovery limit, or null."),
            ),
            (
                "categories",
                open_object_schema("Map of discovery category name to visible tool names."),
            ),
            (
                "recommended_flows",
                array_schema(
                    schema_type("string", "Short recommended tool flow summary."),
                    "Short GPT-facing recommended flow summaries.",
                ),
            ),
            ("hint", schema_type("string", "Focused discovery guidance.")),
            (
                "recommended_next",
                schema_type("string", "Recommended next discovery action."),
            ),
        ])),
        "tool_manifest" => Some(wrapped_output_schema(vec![
            (
                "schema_version",
                schema_type("integer", "Manifest schema version."),
            ),
            (
                "tool_count",
                schema_type("integer", "Total number of tools in the runtime."),
            ),
            (
                "count",
                schema_type("integer", "Returned compact tool count after filtering."),
            ),
            (
                "returned_count",
                schema_type("integer", "Returned compact tool count after filtering and limit."),
            ),
            (
                "total_count",
                schema_type("integer", "Total number of runtime tools before filtering."),
            ),
            (
                "filtered_count",
                schema_type(
                    "integer",
                    "Number of tools after applying the optional category filter.",
                ),
            ),
            (
                "category",
                nullable_schema(
                    "string",
                    "Requested category filter, or null when no filter was applied.",
                ),
            ),
            (
                "intent",
                nullable_schema(
                    "string",
                    "Resolved task-intent view such as coding, audit, exploration, release, or discovery; null when no intent was requested. Intent views only filter and rank discovery output; they do not change tool behavior, policy, permissions, execution, or finish verdict semantics.",
                ),
            ),
            (
                "available_intents",
                array_schema(
                    schema_type("string", "Supported tool_manifest intent name."),
                    "Stable list of supported tool_manifest intent names used for discovery filtering only.",
                ),
            ),
            (
                "filtered",
                schema_type(
                    "boolean",
                    "True when intent, category filtering, or limit was applied.",
                ),
            ),
            (
                "categories_requested",
                nullable_string_array_schema(
                    "Normalized requested category filters, or null when unfiltered.",
                ),
            ),
            (
                "limit",
                nullable_schema("integer", "Effective manifest limit, or null."),
            ),
            (
                "truncated",
                schema_type("boolean", "Whether the limit truncated matching tools."),
            ),
            (
                "truncation_reason",
                nullable_schema("string", "Reason for truncation, such as limit, or null."),
            ),
            (
                "limit_applied",
                schema_type("boolean", "Whether a caller-supplied limit was applied."),
            ),
            (
                "requested_limit",
                nullable_schema("integer", "Caller-supplied limit after runtime clamping, or null."),
            ),
            (
                "categories",
                open_object_schema(
                    "Map of category name to the list of tool names in that category.",
                ),
            ),
            (
                "tools",
                array_schema(
                    open_object_schema(
                        "Compact tool entry: name, category, accepted_flattened_args, deprecated_or_unsupported_args, provider, risk, read_only, requires_project, path_hint, destructive, shell_like, oauth_scope.",
                    ),
                    "Compact tool entries without input/output schemas.",
                ),
            ),
            (
                "risk_summary",
                open_object_schema(
                    "Counts of tools grouped by risk class (read_only, project_write, job_run, etc.).",
                ),
            ),
            (
                "recommended_flows",
                array_schema(
                    open_object_schema("Recommended tool flow with name, purpose, and tools."),
                    "Short list of recommended tool flows for common tasks.",
                ),
            ),
        ])),
        _ => None,
    }
}

fn nullable_string_array_schema(description: &str) -> Value {
    json!({
        "anyOf": [
            {
                "type": "array",
                "items": {
                    "type": "string"
                }
            },
            {
                "type": "null"
            }
        ],
        "description": description,
    })
}
