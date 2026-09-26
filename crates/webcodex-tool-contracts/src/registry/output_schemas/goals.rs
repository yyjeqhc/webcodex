use super::common::{schema_type, wrapped_output_schema};
use serde_json::{json, Value};

fn nullable_integer(description: &str) -> Value {
    json!({
        "anyOf": [{"type": "integer"}, {"type": "null"}],
        "description": description
    })
}

fn nullable_controller_agent_id(description: &str) -> Value {
    json!({
        "anyOf": [
            {"type": "string", "pattern": "^wc_dagent_[A-Za-z0-9_-]{16}$"},
            {"type": "null"}
        ],
        "description": description
    })
}

fn lifecycle_schema() -> Value {
    json!({
        "type": "string",
        "enum": ["active", "completed", "cancelled"],
        "description": "Authoritative durable Goal lifecycle."
    })
}

fn goal_summary_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "goal_id": {"type": "string", "pattern": "^wc_goal_[A-Za-z0-9_-]{16}$", "description": "Canonical durable Goal id; identity only, never authority."},
            "title": {"type": "string", "minLength": 1, "maxLength": 200, "description": "Bounded Goal title."},
            "lifecycle": lifecycle_schema(),
            "revision": {"type": "integer", "minimum": 1, "description": "Monotonic authoritative Goal revision."},
            "created_at_unix_ms": schema_type("integer", "Goal creation time."),
            "updated_at_unix_ms": schema_type("integer", "Latest durable Goal mutation time."),
            "terminal_at_unix_ms": nullable_integer("Terminal transition time, or null while active."),
            "agent_task_count": {"type": "integer", "minimum": 0, "maximum": 64, "description": "Count of explicit AgentTask correlations; correlation grants no Task or execution authority."},
            "workflow_session_count": {"type": "integer", "minimum": 0, "maximum": 64, "description": "Count of explicit Workflow Session correlations; correlation grants no Session or Project authority."}
        },
        "required": [
            "goal_id", "title", "lifecycle", "revision", "created_at_unix_ms",
            "updated_at_unix_ms", "terminal_at_unix_ms", "agent_task_count",
            "workflow_session_count"
        ]
    })
}

fn correlation_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "kind": {"type": "string", "enum": ["agent_task", "workflow_session"]},
            "reference_id": {"type": "string", "pattern": "^(wc_agent_task_[A-Za-z0-9_-]{16}|wc_sess_([A-Za-z0-9_-]{16}|[0-9a-f]{32}))$", "maxLength": 46, "description": "Exact correlated durable identity. It is not a credential and cannot be dereferenced without that domain's normal authorization."},
            "created_at_unix_ms": schema_type("integer", "Correlation creation time.")
        },
        "required": ["kind", "reference_id", "created_at_unix_ms"]
    })
}

fn goal_detail_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "summary": goal_summary_schema(),
            "objective": {"type": "string", "minLength": 1, "maxLength": 8192, "description": "Bounded authoritative high-level objective/instruction; the Store enforces the same 8192-byte UTF-8 ceiling."},
            "controller_agent_id": nullable_controller_agent_id("Exact durable Goal attention-routing Agent, or null for the legacy worker fallback. Identity only; it grants no Goal, Task, Project, Session, Runner, Endpoint, or execution authority."),
            "terminal_reason": {
                "anyOf": [
                    {"type": "string", "minLength": 1, "maxLength": 4096},
                    {"type": "null"}
                ],
                "description": "Bounded terminal reason, if one was explicitly supplied."
            },
            "correlations": {
                "type": "array",
                "maxItems": 64,
                "items": correlation_schema(),
                "description": "Bounded explicit AgentTask and Workflow Session correlations only. No target-domain authority or private target state is projected."
            }
        },
        "required": ["summary", "objective", "controller_agent_id", "terminal_reason", "correlations"]
    })
}

fn goal_activity_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "available": {"type": "boolean", "description": "Whether runtime liveness evidence is authorized and available. False never reveals Window existence, counts, or timestamps."},
            "state": {"type": "string", "enum": ["active", "attention_needed", "unobserved", "not_applicable"], "description": "Derived soft liveness observation only; never authoritative Goal or Task state."},
            "idle_threshold_ms": {"type": "integer", "const": 300000, "description": "Internal five-minute soft-attention heuristic, not an execution timeout."},
            "last_seen_at_ms": nullable_integer("Latest caller-visible WebPi Window activity, including non-meaningful Host/App control traffic."),
            "last_meaningful_activity_at_ms": nullable_integer("Latest caller-visible meaningful WebPi business activity across correlated Windows."),
            "quiet_for_ms": nullable_integer("Milliseconds since latest visible meaningful activity at projection time, or null when unobserved."),
            "linked_window_count": {"anyOf": [{"type": "integer", "minimum": 0, "maximum": 16}, {"type": "null"}], "description": "Bounded count of legally observable correlated Window candidates, or null when runtime observation is unavailable/not applicable."},
            "active_meaningful_request_count": {"anyOf": [{"type": "integer", "minimum": 0, "maximum": 64}, {"type": "null"}], "description": "Visible in-flight meaningful WebPi requests across candidate Windows, or null when unavailable/not applicable."},
            "coverage_partial": {"type": "boolean", "description": "True when bounded scans or active-request retention may omit evidence. Partial coverage never produces attention_needed."}
        },
        "required": [
            "available", "state", "idle_threshold_ms", "last_seen_at_ms",
            "last_meaningful_activity_at_ms", "quiet_for_ms", "linked_window_count",
            "active_meaningful_request_count", "coverage_partial"
        ],
        "description": "Payload-free ClientWindow liveness evidence derived only after exact Goal/Session/Project re-authorization. It grants no authority and never exposes Window keys or raw Host metadata."
    })
}

fn goal_plan_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "version": {"type": "integer", "const": 1, "description": "Backward-compatible Goal Plan presentation projection version; live activity is an additive observation field."},
            "goal_id": {"type": "string", "pattern": "^wc_goal_[A-Za-z0-9_-]{16}$", "description": "Exact durable Goal identity used for refresh/rehydration and app-only polling. Identity is never authority."},
            "title": {"type": "string", "minLength": 1, "maxLength": 200, "description": "Bounded Goal title."},
            "objective": {"type": "string", "minLength": 1, "maxLength": 8192, "description": "Bounded authoritative Goal objective; the Store enforces the same 8192-byte UTF-8 ceiling."},
            "controller_agent_id": nullable_controller_agent_id("Exact durable Goal attention-routing Agent, or null when terminal attention uses the backward-compatible Task worker fallback. Endpoint/window bindings are never projected."),
            "lifecycle": lifecycle_schema(),
            "revision": {"type": "integer", "minimum": 1, "description": "Monotonic authoritative Goal revision."},
            "updated_at_unix_ms": schema_type("integer", "Latest authoritative Goal mutation time."),
            "terminal_at_unix_ms": nullable_integer("Terminal transition time, or null while active."),
            "agent_task_count": {"type": "integer", "minimum": 0, "maximum": 64, "description": "Count of explicit AgentTask correlations; no target-domain state or authority is projected."},
            "workflow_session_count": {"type": "integer", "minimum": 0, "maximum": 64, "description": "Count of explicit Workflow Session correlations; no Session ledger, Project state, or authority is projected."},
            "activity": goal_activity_schema()
        },
        "required": [
            "version", "goal_id", "title", "objective", "controller_agent_id", "lifecycle", "revision",
            "updated_at_unix_ms", "terminal_at_unix_ms", "agent_task_count",
            "workflow_session_count", "activity"
        ],
        "description": "Read-only bounded Goal Plan presentation projection. It contains no execution authority, fences, tokens, credentials, Session ledger, Job logs, stdout, or stderr."
    })
}

fn goal_mutation_schema() -> Value {
    wrapped_output_schema(vec![
        ("goal", goal_detail_schema()),
        (
            "created",
            schema_type("boolean", "True only for first Goal creation."),
        ),
        (
            "replayed",
            schema_type("boolean", "True only for exact keyed replay."),
        ),
        (
            "state_changed",
            schema_type("boolean", "Whether authoritative Goal state changed."),
        ),
    ])
}

pub fn output_schema_for_tool(name: &str) -> Option<Value> {
    let schema = match name {
        "create_goal"
        | "update_goal"
        | "associate_goal_agent_task"
        | "associate_goal_workflow_session" => goal_mutation_schema(),
        "get_goal" => wrapped_output_schema(vec![("goal", goal_detail_schema())]),
        "present_goal_plan" | "goal_plan_state" => {
            wrapped_output_schema(vec![("goal_plan", goal_plan_schema())])
        }
        "list_goals" => wrapped_output_schema(vec![
            (
                "total_count",
                schema_type(
                    "integer",
                    "Total Goals visible to the current owner principal.",
                ),
            ),
            ("offset", schema_type("integer", "Returned page offset.")),
            (
                "next_offset",
                nullable_integer("Next page offset when truncated."),
            ),
            (
                "truncated",
                schema_type("boolean", "True when more caller-visible Goals remain."),
            ),
            (
                "goals",
                json!({
                    "type": "array",
                    "maxItems": 100,
                    "items": goal_summary_schema(),
                    "description": "Bounded Goal summaries. Objective, terminal reason, and correlation identities are omitted from list projection."
                }),
            ),
        ]),
        _ => return None,
    };
    Some(schema)
}
