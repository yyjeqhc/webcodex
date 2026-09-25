use serde_json::{json, Value};

use super::common::{
    array_schema, authority_profile_schema, nullable_schema, open_object_schema, schema_type,
    wrapped_output_schema,
};

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    match name {
        "current_window_activity" => Some(wrapped_output_schema(vec![
            ("status", json!({"type":"string","enum":["available","unavailable"]})),
            ("reason_code", schema_type("string", "Bounded reason when current Window, authenticated principal, runtime:read, or activity storage is unavailable.")),
            ("events", json!({"type":"array","maxItems":50,"description":"Newest first, sanitized current-Window events after principal and current Project visibility filtering. No arguments, outputs, raw payloads, native paths, credentials, or principal identifiers.","items":{"type":"object","additionalProperties":false,"properties":{
                "request_observed_at_ms":{"type":"integer"},
                "response_handed_at_ms":{"type":"integer","description":"WebCodex response constructed and handed to HTTP framework / handler returned. No client, Host, ChatGPT, or model-continuation receipt is implied."},
                "started_at_ms":{"type":"integer"},"ended_at_ms":{"type":"integer"},"duration_ms":{"type":"integer"},
                "service_ms":{"type":"integer"},
                "next_call_gap_ms":{"type":["integer","null"],"description":"Observed only from a later canonical meaningful call in this principal and Window; null means no serial gap was observed, never elapsed time."},
                "cycle_ms":{"type":"integer"},"window_transition_kind":{"type":"string"},"response_streaming":{"type":"boolean"},
                "method":{"type":"string"},"tool_name":{"type":"string"},"activity_presentation":{"type":"string"},"activity_kind":{"type":"string"},
                "project":{"type":"string"},"status":{"type":"string"},"http_status":{"type":"integer"},"meaningful":{"type":"boolean"},
                "server_trace_id":{"type":"string"},"workflow_sessions":{"type":"array","items":{"type":"object","additionalProperties":false,"properties":{"workflow_session_id":{"type":"string"},"project":{"type":"string"},"relation":{"type":"string"}},"required":["workflow_session_id","relation"]}},
                "code_mode_composition":open_object_schema("Validated bounded nested WebCodex Code Mode composition when available.")
            },"required":["started_at_ms","ended_at_ms","duration_ms","method","status","meaningful","workflow_sessions"]}})),
            ("summary", current_window_activity_summary_schema()),
            ("active_requests", json!({"type":"array","maxItems":8,"items":{"type":"object","additionalProperties":false,"properties":{"server_trace_id":{"type":"string"},"tool_name":{"type":["string","null"]},"started_at_ms":{"type":"integer"}},"required":["server_trace_id","tool_name","started_at_ms"]}})),
            ("truncated", schema_type("boolean", "Visible events from the bounded recent scan were omitted by the presentation or serialized byte bound; this is not a lifetime-history completeness claim.")),
        ])),
        "runtime_status" => Some(wrapped_output_schema(vec![
            ("service", schema_type("string", "Runtime service name.")),
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
                                "shared_key_enabled": {"type": "boolean", "description": "Whether direct shared-key quick-start authentication is effective for the running Server."},
                                "anonymous_enabled": {"type": "boolean", "description": "Whether explicit open-anonymous access is effective for the running Server."},
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
                open_object_schema("Project counts from the Runner registry. Prefer projects.effective for model-facing status."),
            ),
            (
                "runners",
                open_object_schema("Runner counts and a single clients collection using runner_instance_id and runner_protocol_generation; summary contains only aggregate counts. Omitted in focused compact/summary mode. Per-client host_context is bounded Runner-configured advisory data, not observed truth or authority. job_concurrency contains the static Runner limit plus caller-visible running and queued counts. Canonical top-level counts are count, online_count, and stale_count in full, compact, and summary_only output."),
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
        "read_tool_trace" => Some(wrapped_output_schema(vec![
            ("trace_mode", schema_type("string", "Trace mode; read_tool_trace requires full.")),
            ("payload_count", schema_type("integer", "Total indexed payloads in this trace.")),
            ("returned_count", schema_type("integer", "Payload metadata entries returned in listing mode.")),
            ("offset", schema_type("integer", "Listing offset.")),
            ("next_offset", nullable_schema("integer", "Next listing offset or null.")),
            ("payloads", array_schema(open_object_schema("Safe payload metadata: payload_index, phase, payload_bytes, compressed_bytes, payload_sha256, and payload_available. Native paths are never returned."), "Bounded payload metadata.")),
            ("payload_index", schema_type("integer", "Selected payload index in payload-read mode.")),
            ("phase", schema_type("string", "Selected payload lifecycle phase.")),
            ("payload_bytes", schema_type("integer", "Uncompressed payload size.")),
            ("payload_sha256", schema_type("string", "SHA-256 of the selected uncompressed payload.")),
            ("payload_available", schema_type("boolean", "Whether the selected payload is within the model read ceiling.")),
            ("max_payload_bytes", schema_type("integer", "Maximum uncompressed payload bytes readable through this tool.")),
            ("reason", schema_type("string", "Bounded reason when a payload is not model-readable.")),
            ("payload", json!({"description": "Selected raw JSON trace payload of any JSON type. May contain sensitive tool data; operator-only."})),
        ])),
        "list_projects" => Some(wrapped_output_schema(vec![
            (
                "projects",
                array_schema(open_object_schema("Project summary including canonical id, Server-issued project_ref when a stable Project root identity is available, and capabilities.git_available/supports_cleanup_verification/recommended_for_smoke."), "Runtime projects."),
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
        "list_runners" => Some(wrapped_output_schema(vec![
            (
                "runners",
                array_schema(open_object_schema("Runner summary including bounded Runner-configured host_context advisory data, never authority or proof of current state, plus job_concurrency limit/running/queued facts."), "Canonical Runner collection; per-Runner identity uses runner_instance_id and runner_protocol_generation."),
            ),
            (
                "summary",
                open_object_schema("Aggregate count, online, offline, and stale counts; Runner entries appear only in runners."),
            ),
            ("count", schema_type("integer", "Runner/client count.")),
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
        "tool_manifest" => {
            let fields = vec![
            (
                "name",
                schema_type(
                    "string",
                    "Exact requested tool identity in the sparse model-facing exact projection.",
                ),
            ),
            (
                "description",
                schema_type(
                    "string",
                    "Canonical ToolSpec description for exact lookup, or a bounded canonical-derived selection summary in filtered tool entries.",
                ),
            ),
            (
                "route",
                tool_manifest_invocation_route_schema(),
            ),
            (
                "routing_note",
                schema_type(
                    "string",
                    "Model-facing routing guidance. tool_manifest discovery never dynamically registers a new Host tool.",
                ),
            ),
            (
                "input_schema",
                open_object_schema(
                    "Exact tool input schema in the sparse model-facing exact projection; output schema remains omitted.",
                ),
            ),
            (
                "effect",
                schema_type(
                    "string",
                    "Canonical business effect: observe, mutate, or execute.",
                ),
            ),
            (
                "risk",
                schema_type(
                    "string",
                    "Canonical risk class when relevant to exact lookup or filtered selection.",
                ),
            ),
            (
                "approval",
                schema_type(
                    "string",
                    "Canonical interactive approval policy in exact lookup.",
                ),
            ),
            (
                "idempotency",
                schema_type(
                    "string",
                    "Canonical retry/idempotency contract in exact lookup.",
                ),
            ),
            (
                "authority",
                open_object_schema(
                    "Canonical required-scope policy in exact lookup. Route discovery never grants these scopes.",
                ),
            ),
            (
                "annotations",
                open_object_schema("Canonical ToolSpec annotations in exact lookup."),
            ),
            (
                "execution",
                execution_selection_schema(),
            ),
            (
                "host_orchestration",
                host_orchestration_schema(),
            ),
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
                "tool_name",
                nullable_schema("string", "Compatibility/full canonical exact requested tool name. The default sparse model projection uses name instead."),
            ),
            (
                "contract",
                json!({
                    "anyOf": [
                        {
                            "type": "object",
                            "description": "Exact one-tool contract containing name, description, canonical effect/risk/approval/idempotency semantics, optional execution selection semantics, input_schema, annotations, and current MCP model-surface invocation routing. output_schema is intentionally omitted.",
                            "additionalProperties": false,
                            "properties": {
                                "name": {"type": "string"},
                                "description": {"type": "string"},
                                "effect": {
                                    "type": "string",
                                    "enum": ["observe", "mutate", "execute"],
                                    "description": "Canonical business effect."
                                },
                                "risk": {
                                    "type": "string",
                                    "enum": ["read_only", "project_write", "skill_manage", "memory_manage", "communication_manage", "session_collaborate", "workflow_manage", "checkpoint_manage", "run_control", "computer_control", "job_run", "account_manage"],
                                    "description": "Canonical risk class; read_only is the retained external label for read/observation risk."
                                },
                                "approval": {
                                    "type": "string",
                                    "enum": ["none", "standard", "inherit_from_start"],
                                    "description": "Canonical interactive approval policy, independent from effect and authority."
                                },
                                "idempotency": {
                                    "type": "string",
                                    "enum": ["pure_read", "desired_state", "keyed", "fenced_replay", "non_idempotent"],
                                    "description": "Canonical retry/idempotency contract."
                                },
                                "execution": execution_selection_schema(),
                                "input_schema": {"type": "object", "additionalProperties": true},
                                "annotations": {"type": "object", "additionalProperties": true},
                                "availability": {
                                    "type": "string",
                                    "enum": ["direct", "gateway", "unavailable"],
                                    "description": "Invocation route on canonical Adaptive Runtime only; authorization, feature gates, and project authority are checked separately."
                                },
                                "gateway_tool": {
                                    "anyOf": [
                                        {"type": "string", "const": "call_runtime_tool"},
                                        {"type": "null"}
                                    ],
                                    "description": "call_runtime_tool only when availability=gateway; otherwise null."
                                }
                            },
                            "required": ["name", "description", "effect", "risk", "approval", "idempotency", "input_schema", "annotations", "availability", "gateway_tool"]
                        },
                        {"type": "null"}
                    ]
                }),
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
                    "Global category inventory in unfiltered discovery. Compatibility/full canonical results may also carry it for filtered/exact calls, but the default sparse model projection omits unrelated inventory.",
                ),
            ),
            (
                "tools",
                array_schema(
                    open_object_schema(
                        "Default filtered model projection: name, bounded canonical-derived description, route, requires_project, effect, optional execution selection metadata, and risk only when non-observe. Compatibility/full canonical results may retain richer metadata."
                    ),
                    "Filtered selection entries without input/output schemas; unfiltered sparse discovery uses the categories inventory instead of duplicating all tool names here.",
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
                    open_object_schema("Recommended tool flow with name, purpose, and tools; filtered partial projections also identify partial=true and omitted_tools for canonical members not selected into that projection."),
                    "Short list of recommended tool flows for common tasks.",
                ),
            ),
        ];
            #[cfg(feature = "experimental-code-mode")]
            let fields = {
                let mut fields = fields;
                fields.push((
                    "code_mode_callable_contract",
                    open_object_schema(
                        "Bounded presentation-only callable contract attached only to exact Code Mode entry-tool discovery. It is derived from canonical ToolSpecs plus the existing Code Mode admission policy and grants no authority.",
                    ),
                ));
                fields
            };
            Some(wrapped_output_schema(fields))
        }
        _ => None,
    }
}

fn current_window_activity_summary_schema() -> Value {
    json!({
        "type": "object",
        "description": "Descriptive factual metrics over the bounded visible event scan. Gap fields are WebCodex-observed serial request timing only; they do not identify Host cells, model turns, thinking time, frontend delay, network delay, or user delay.",
        "additionalProperties": false,
        "properties": {
            "events_scanned": {"type":"integer"},
            "meaningful_call_count": {"type":"integer"},
            "observe_jobs_count": {"type":"integer"},
            "observe_jobs_ratio_denominator": {"type":"integer"},
            "observe_jobs_ratio": {"type":["number","null"]},
            "handler_returned_count": {"type":"integer"},
            "missing_handoff_count": {"type":"integer"},
            "overlapping_call_count": {"type":"integer","description":"Count of persisted window_transition_kind=overlap facts; never inferred from a short gap."},
            "serial_call_count": {"type":"integer"},
            "observed_next_call_gap_count": {"type":"integer"},
            "gaps_lt_1s": {"type":"integer","description":"Cumulative count of observed serial next-call gaps below 1 second."},
            "gaps_lt_2s": {"type":"integer","description":"Cumulative count of observed serial next-call gaps below 2 seconds; includes gaps_lt_1s."},
            "gaps_lt_5s": {"type":"integer","description":"Cumulative count of observed serial next-call gaps below 5 seconds; includes the lower less-than thresholds."},
            "gaps_ge_5s": {"type":"integer","description":"Cumulative count of observed serial next-call gaps at least 5 seconds."},
            "gaps_ge_10s": {"type":"integer","description":"Cumulative count of observed serial next-call gaps at least 10 seconds; a subset of gaps_ge_5s."},
            "gaps_ge_30s": {"type":"integer","description":"Cumulative count of observed serial next-call gaps at least 30 seconds."},
            "gaps_ge_120s": {"type":"integer","description":"Cumulative count of observed serial next-call gaps at least 120 seconds."},
            "total_service_ms": {"type":"integer"},
            "total_positive_observed_next_call_gap_ms": {"type":"integer"},
            "max_service_ms": {"type":["integer","null"]},
            "max_observed_next_call_gap_ms": {"type":["integer","null"]},
            "returned_nested_code_mode_child_count": {"type":"integer"}
        },
        "required": [
            "events_scanned",
            "meaningful_call_count",
            "observe_jobs_count",
            "observe_jobs_ratio_denominator",
            "observe_jobs_ratio",
            "handler_returned_count",
            "missing_handoff_count",
            "overlapping_call_count",
            "serial_call_count",
            "observed_next_call_gap_count",
            "gaps_lt_1s",
            "gaps_lt_2s",
            "gaps_lt_5s",
            "gaps_ge_5s",
            "gaps_ge_10s",
            "gaps_ge_30s",
            "gaps_ge_120s",
            "total_service_ms",
            "total_positive_observed_next_call_gap_ms",
            "max_service_ms",
            "max_observed_next_call_gap_ms",
            "returned_nested_code_mode_child_count"
        ]
    })
}

fn tool_manifest_invocation_route_schema() -> Value {
    json!({
        "type": "object",
        "description": "Parser-ready Adaptive Runtime invocation routing. Discovery never registers a Host tool and never grants authority.",
        "additionalProperties": false,
        "properties": {
            "primary": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "mode": {
                        "type": "string",
                        "enum": ["direct", "gateway", "unavailable"]
                    },
                    "tool": {
                        "anyOf": [
                            {"type": "string"},
                            {"type": "null"}
                        ]
                    },
                    "target": {
                        "type": "string",
                        "description": "Canonical runtime target when primary execution uses the gateway or the tool is unavailable."
                    }
                },
                "required": ["mode", "tool"]
            },
            "fallback": {
                "anyOf": [
                    {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {
                            "mode": {"type": "string", "const": "gateway"},
                            "tool": {"type": "string", "const": "call_runtime_tool"},
                            "target": {"type": "string"},
                            "when": {
                                "type": "string",
                                "const": "direct_callable_unavailable"
                            },
                            "blocked_when_mcp_apps_enabled": {"type": "boolean"}
                        },
                        "required": [
                            "mode",
                            "tool",
                            "target",
                            "when",
                            "blocked_when_mcp_apps_enabled"
                        ]
                    },
                    {"type": "null"}
                ],
                "description": "Gateway fallback for an ordinary direct tool when the Host direct callable is absent; null when no fallback applies."
            },
            "tool_manifest_registers_host_tool": {
                "type": "boolean",
                "const": false
            },
            "discovery_effect": {
                "type": "string",
                "const": "none"
            }
        },
        "required": [
            "primary",
            "fallback",
            "tool_manifest_registers_host_tool",
            "discovery_effect"
        ]
    })
}

fn host_orchestration_schema() -> Value {
    json!({
        "type": "object",
        "description": "Static guidance-only Host-native orchestration hints derived from ToolDefinition. They grant no authority and do not change ToolCompositionPolicy, effects, permissions, retry, idempotency, or runtime scheduling.",
        "additionalProperties": false,
        "properties": {
            "guidance_only": {"type": "boolean", "const": true},
            "concurrency": {
                "type": "string",
                "enum": ["unspecified", "independent_parallel_read", "sequential"]
            },
            "native_batch_field": {
                "anyOf": [
                    {"type": "string", "maxLength": 64},
                    {"type": "null"}
                ]
            },
            "compound_preferred": {"type": "boolean"}
        },
        "required": [
            "guidance_only",
            "concurrency",
            "native_batch_field",
            "compound_preferred"
        ]
    })
}

fn execution_selection_schema() -> Value {
    json!({
        "type": "object",
        "description": "Optional canonical ordinary-execution selection semantics. This is model guidance only and does not grant authority or change runtime lifecycle behavior.",
        "additionalProperties": false,
        "properties": {
            "form": {"type": "string", "enum": ["native_argv", "typed_script", "shell_command", "structured_validation", "persistent_shell_command"]},
            "lifetime": {"type": "string", "enum": ["runner", "supervisor", "session_shell"]},
            "start": {"type": "string", "enum": ["sync_first", "async_immediate", "existing_session"]},
            "continuation": {"type": "string", "enum": ["observe_jobs", "session_shell", "none"]}
        },
        "required": ["form", "lifetime", "start", "continuation"]
    })
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
