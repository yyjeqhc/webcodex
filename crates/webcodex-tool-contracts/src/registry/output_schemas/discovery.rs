use serde_json::{json, Value};

use super::common::{
    array_schema, authority_profile_schema, nullable_schema, open_object_schema,
    operation_phase_schema, schema_type, wrapped_output_schema,
};

fn deployment_receipt_schema(description: &str) -> Value {
    json!({
        "type": "object",
        "description": description,
        "additionalProperties": false,
        "properties": {
            "receipt_id": schema_type("string", "Stable durable deployment receipt id."),
            "owner_kind": schema_type("string", "Non-secret authenticated principal kind that owns the receipt."),
            "operation": {"type": "string", "enum": ["deploy", "restart", "rollback"]},
            "client_id": schema_type("string", "Exact target Runner client_id bound to the receipt."),
            "request_hash": schema_type("string", "SHA-256 binding the exact idempotent deployment request."),
            "target_manifest": open_object_schema("Bounded non-secret target release manifest."),
            "state": {"type": "string", "enum": ["planned", "draining", "ready", "switching", "verifying", "succeeded", "failed", "rolled_back", "outcome_unknown"]},
            "operation_phase": operation_phase_schema("Canonical cross-domain operation phase derived from the durable deployment state. This does not replace state."),
            "revision": schema_type("integer", "Optimistic durable deployment receipt revision."),
            "created_at": schema_type("integer", "Receipt creation timestamp."),
            "updated_at": schema_type("integer", "Latest receipt transition timestamp."),
            "terminal_at": nullable_schema("integer", "Terminal transition timestamp, when terminal."),
            "last_error_code": nullable_schema("string", "Bounded stable terminal diagnostic code, when any."),
            "backup_id": nullable_schema("string", "Validated rollback backup identity, when one has been created."),
        },
        "required": [
            "receipt_id", "owner_kind", "operation", "client_id", "request_hash",
            "target_manifest", "state", "operation_phase", "revision", "created_at",
            "updated_at", "terminal_at", "last_error_code", "backup_id"
        ]
    })
}

fn deployment_preflight_evidence_schema(
    description: &str,
    include_candidate_evidence: bool,
) -> Value {
    let mut properties = serde_json::Map::new();
    properties.insert(
        "readiness".to_string(),
        schema_type(
            "string",
            "Deployment readiness: ready, drain_required, or blocked.",
        ),
    );
    properties.insert(
        "operation".to_string(),
        schema_type(
            "string",
            "Requested lifecycle operation: restart, deploy, or rollback.",
        ),
    );
    properties.insert(
        "ready_to_begin".to_string(),
        schema_type(
            "boolean",
            "Whether no hard blocker prevents beginning the controlled lifecycle workflow.",
        ),
    );
    properties.insert(
        "ready_for_cutover".to_string(),
        schema_type(
            "boolean",
            "Whether drain is active, no active Jobs remain, and no hard blocker prevents cutover.",
        ),
    );
    properties.insert(
        "drain_required".to_string(),
        schema_type(
            "boolean",
            "Whether drain must still be entered or active Jobs must finish before cutover.",
        ),
    );
    properties.insert(
        "service_lifecycle".to_string(),
        open_object_schema(
            "Current process-local drain state, optimistic generation, and last-change timestamp.",
        ),
    );
    properties.insert(
        "state_changed".to_string(),
        schema_type("boolean", "Always false for preflight evidence."),
    );
    properties.insert(
        "target".to_string(),
        open_object_schema("Exact target Runner connectivity and source-alignment status."),
    );
    properties.insert(
        "jobs".to_string(),
        open_object_schema(
            "Current active/running/queued/recovering Job counts used by readiness evaluation.",
        ),
    );
    properties.insert(
        "authority".to_string(),
        open_object_schema(
            "Server service-control policy plus credential-scoped caller service authority.",
        ),
    );
    properties.insert(
        "blockers".to_string(),
        array_schema(
            open_object_schema("Hard readiness blocker with stable code and bounded message."),
            "Hard deployment blockers.",
        ),
    );
    properties.insert(
        "warnings".to_string(),
        array_schema(
            open_object_schema(
                "Non-blocking deployment warning with stable code and bounded message.",
            ),
            "Deployment warnings.",
        ),
    );
    if include_candidate_evidence {
        properties.insert(
            "candidate_preflight".to_string(),
            open_object_schema("Verified deploy candidate evidence from the supervisor-controlled candidate root: exact candidate id, all required WebPi artifacts, expected size, SHA-256 verification, installed runtime presence, and staging/backup parent readiness. Caller-provided arbitrary filesystem paths are not accepted."),
        );
        properties.insert(
            "receipt_store_writable".to_string(),
            schema_type("boolean", "Whether the durable deployment receipt SQLite store successfully acquired an immediate write transaction and rolled it back before cutover."),
        );
    }
    let required = properties
        .keys()
        .cloned()
        .map(Value::String)
        .collect::<Vec<_>>();
    json!({
        "type": "object",
        "description": description,
        "additionalProperties": false,
        "properties": properties,
        "required": required,
    })
}

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    match name {
        "deployment_preflight" => Some(wrapped_output_schema(vec![
            ("readiness", schema_type("string", "Deployment readiness: ready, drain_required, or blocked.")),
            ("operation", schema_type("string", "Requested lifecycle operation: restart, deploy, or rollback; determines the narrow service scopes required by preflight.")),
            ("ready_to_begin", schema_type("boolean", "Whether no hard blocker prevents beginning a controlled deployment workflow.")),
            ("ready_for_cutover", schema_type("boolean", "Whether service drain mode is active, no active Jobs remain, and no hard blocker prevents cutover.")),
            ("drain_required", schema_type("boolean", "Whether service drain mode must still be entered or active Jobs must finish before cutover.")),
            ("service_lifecycle", open_object_schema("Current process-local drain state, optimistic generation, and last-change timestamp.")),
            ("state_changed", schema_type("boolean", "Always false; deployment_preflight is read-only.")),
            ("target", open_object_schema("Exact target Runner connectivity and source-alignment status.")),
            ("jobs", open_object_schema("Current active/running/queued/recovering Job counts used by readiness evaluation.")),
            ("authority", open_object_schema("Server service-control policy plus credential-scoped caller service authority. Never grants authority.")),
            ("blockers", array_schema(open_object_schema("Hard readiness blocker with stable code and bounded message."), "Hard deployment blockers.")),
            ("warnings", array_schema(open_object_schema("Non-blocking deployment warning with stable code and bounded message."), "Deployment warnings.")),
        ])),
        "prepare_service_deployment" => Some(wrapped_output_schema(vec![
            ("deployment_receipt", deployment_receipt_schema("Durable deployment receipt identity, bounded target manifest, domain state, canonical operation phase, revision, and timestamps.")),
            ("replayed", schema_type("boolean", "Whether exact idempotency replay returned the existing durable receipt.")),
            ("state_changed", schema_type("boolean", "True only when this call created the durable receipt.")),
            ("preflight", deployment_preflight_evidence_schema("Read-only deployment preflight snapshot captured before receipt admission.", false)),
            ("error_kind", schema_type("string", "Stable preparation failure kind.")),
        ])),
        "read_deployment_receipt" => Some(wrapped_output_schema(vec![
            ("deployment_receipt", deployment_receipt_schema("Exact durable deployment receipt owned by the current principal.")),
            ("state_changed", schema_type("boolean", "Always false; reading a deployment receipt is read-only.")),
            ("error_kind", schema_type("string", "Stable read failure kind.")),
        ])),
        "service_rollback" => Some(wrapped_output_schema(vec![
            ("deployment_receipt", deployment_receipt_schema("Durable rollback receipt including canonical operation phase, target backup identity, state/revision, terminal diagnostics, and reversible safety backup identity when known.")),
            ("scheduled", schema_type("boolean", "Whether the exact rollback request is currently scheduled/in-flight under the standalone supervisor.")),
            ("state_changed", schema_type("boolean", "Whether this call advanced or reconciled durable rollback state.")),
            ("preflight", deployment_preflight_evidence_schema("Deployment preflight snapshot proving drain, target alignment, authority and zero-active-Job readiness before first rollback cutover.", false)),
            ("error_kind", schema_type("string", "Stable rollback scheduling/reconciliation failure kind.")),
        ])),
        "service_deploy" => Some(wrapped_output_schema(vec![
            ("deployment_receipt", deployment_receipt_schema("Durable deploy receipt including canonical operation phase, immutable target manifest, state/revision, terminal diagnostics, and rollback backup identity when known.")),
            ("scheduled", schema_type("boolean", "Whether the exact deploy request is currently scheduled/in-flight under the standalone supervisor.")),
            ("state_changed", schema_type("boolean", "Whether this call advanced or reconciled durable deploy state.")),
            ("preflight", deployment_preflight_evidence_schema("Deployment preflight snapshot proving drain, target alignment, authority, zero-active-Job readiness, candidate artifact SHA-256/size/path validation, staging/backup filesystem readiness, and durable receipt-store writability before first cutover.", true)),
            ("error_kind", schema_type("string", "Stable deploy scheduling/reconciliation failure kind.")),
        ])),
        "service_restart" => Some(wrapped_output_schema(vec![
            ("deployment_receipt", deployment_receipt_schema("Durable restart receipt with canonical operation phase, bound to exact caller, target, lifecycle generation and current build identity.")),
            ("scheduled", schema_type("boolean", "Whether the standalone supervisor currently has this restart scheduled or in-flight.")),
            ("replayed", schema_type("boolean", "Whether the idempotency key resolved to an existing exact restart receipt.")),
            ("state_changed", schema_type("boolean", "Whether this call advanced or reconciled durable restart state.")),
            ("preflight", deployment_preflight_evidence_schema("Deployment preflight snapshot proving drain, supervisor authority/target binding and zero-active-Job readiness before restart scheduling.", false)),
            ("error_kind", schema_type("string", "Stable restart scheduling/reconciliation failure kind.")),
        ])),
        "service_drain" => Some(wrapped_output_schema(vec![
            ("state_changed", schema_type("boolean", "Whether this call changed the process-local drain state.")),
            ("service_lifecycle", open_object_schema("Current process-local drain state, optimistic generation, and last-change timestamp.")),
            ("error_kind", schema_type("string", "Stable error kind on a failed lifecycle generation fence.")),
            ("expected_generation", schema_type("integer", "Caller-provided optimistic generation on a conflict.")),
        ])),
        "runtime_diagnostics" => Some(wrapped_output_schema(vec![
            ("capacity", schema_type("integer", "Maximum in-memory diagnostic events retained.")),
            ("buffered_count", schema_type("integer", "Current number of retained diagnostic events.")),
            ("dropped_count", schema_type("integer", "Number of older diagnostic events evicted since this Server process started.")),
            ("oldest_sequence", nullable_schema("integer", "Oldest diagnostic sequence still retained, or null when empty.")),
            ("newest_sequence", nullable_schema("integer", "Newest diagnostic sequence retained, or null when empty.")),
            ("info_count", schema_type("integer", "Number of retained info diagnostic events.")),
            ("warn_count", schema_type("integer", "Number of retained warning diagnostic events.")),
            ("error_count", schema_type("integer", "Number of retained error diagnostic events.")),
            ("newest_warn_sequence", nullable_schema("integer", "Newest retained warning sequence, or null when none.")),
            ("newest_error_sequence", nullable_schema("integer", "Newest retained error sequence, or null when none.")),
            ("returned_count", schema_type("integer", "Number of events returned by this bounded query.")),
            ("filters", open_object_schema("Effective bounded diagnostic filters.")),
            ("events", array_schema(open_object_schema("Secrets-safe diagnostic event with sequence, timestamp, severity, code-owned component/code and optional token-shaped correlation id."), "Bounded diagnostic events in chronological order.")),
            ("state_changed", schema_type("boolean", "Always false; diagnostics reads are observational.")),
            ("error_kind", schema_type("string", "Stable validation error kind when filters are invalid.")),
        ])),
        "public_tunnel_probe" => Some(wrapped_output_schema(vec![
            ("status", schema_type("string", "Probe state: verified, degraded, or not_configured.")),
            ("verified", schema_type("boolean", "Whether the configured public origin passed OpenAPI identity and unauthenticated protected-route checks.")),
            ("reason_code", schema_type("string", "Stable bounded probe result code; never contains response text or URLs.")),
            ("observed_at", schema_type("integer", "Unix timestamp for this probe evidence.")),
            ("age_secs", schema_type("integer", "Age in seconds when returned.")),
            ("stale_after_secs", schema_type("integer", "Age after which cached public-origin evidence is stale.")),
            ("stale", schema_type("boolean", "Whether cached evidence exceeded the stale threshold.")),
            ("latency_ms", schema_type("integer", "Bounded end-to-end probe latency in milliseconds.")),
            ("openapi_status", nullable_schema("integer", "Observed /openapi.json HTTP status when available.")),
            ("protected_status", nullable_schema("integer", "Observed unauthenticated protected-action HTTP status when available.")),
            ("tls_verified", schema_type("boolean", "For HTTPS, whether a successful request completed normal TLS certificate validation.")),
            ("cached", schema_type("boolean", "Whether a fresh recent probe was reused instead of issuing new network requests.")),
            ("state_changed", schema_type("boolean", "Always false; cached observability evidence is not business state.")),
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
            ("diagnostics", open_object_schema("Secrets-safe runtime diagnostic aggregate counts and sequence bounds. Event details still require diagnostics:read.")),
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
                "agents",
                open_object_schema("Runner counts and client summaries. Per-client host_context is bounded Runner-configured advisory data, not observed truth or authority. job_concurrency contains the static Runner limit plus caller-visible running and queued counts. Canonical top-level counts are count, online_count, and stale_count in full, compact, and summary_only output."),
            ),
            (
                "jobs",
                open_object_schema("Bounded runtime job counts, including active_count, running_count, queued_count, recovering_count, reconciled_count, and lost_after_reconcile_count."),
            ),
            (
                "health",
                open_object_schema("Structured liveness/readiness, stable degraded reason codes, and component states for deployment store, Runner registry, recovery, version/source alignment, service lifecycle/supervisor, and configured-but-unverified public tunnel exposure."),
            ),
            ("tools", open_object_schema("Runtime tool counts and names.")),
            (
                "authority",
                authority_profile_schema("Canonical authority profile. trusted_agent is the self-hosted single-operator default and does not bypass hard safety checks (scopes, project boundary, read-only sessions, path policy)."),
            ),
            (
                "caller_authority",
                open_object_schema("Effective credential-scoped caller capabilities and exact missing-scope requirements for sensitive operations such as run_detached_process. This never widens authority; it only reports the intersection of server policy and the current authenticated principal."),
            ),
            ("service_lifecycle", open_object_schema("Current process-local service drain state and optimistic generation. Restart resets to a fresh non-draining epoch.")),
            ("service_supervisor", open_object_schema("Whether the running Server is attached to the narrow authenticated standalone lifecycle supervisor required for self-restart.")),
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
        "list_runners" => Some(wrapped_output_schema(vec![
            (
                "agents",
                array_schema(open_object_schema("Runner summary including bounded Runner-configured host_context advisory data, never authority or proof of current state, plus job_concurrency limit/running/queued facts."), "Legacy compatibility key containing Runner summaries."),
            ),
            (
                "clients",
                array_schema(open_object_schema("Runner client summary including job_concurrency limit/running/queued facts."), "Runner client summaries."),
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
                json!({
                    "type": "object",
                    "description": "Canonical Adaptive Runtime invocation route only. This never grants scope, project authority, feature availability, or permission.",
                    "additionalProperties": false,
                    "properties": {
                        "mode": {
                            "type": "string",
                            "enum": ["direct", "gateway", "unavailable"]
                        },
                        "via": {
                            "type": "string",
                            "const": "call_runtime_tool",
                            "description": "Gateway entry point, present only when mode=gateway."
                        }
                    },
                    "required": ["mode"]
                }),
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
