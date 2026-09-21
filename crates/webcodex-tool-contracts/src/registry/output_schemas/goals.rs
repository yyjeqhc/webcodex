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

pub(super) fn goal_step_schema() -> Value {
    json!({
        "type": "object", "additionalProperties": false,
        "properties": {
            "id": {"type": "string", "pattern": "^[A-Za-z0-9_-]{1,32}$"},
            "title": {"type": "string", "minLength": 1, "maxLength": 120},
            "status": {"type": "string", "enum": ["pending", "in_progress", "completed"]},
            "updated_at_unix_ms": {"type": "integer"}
        },
        "required": ["id", "title", "status", "updated_at_unix_ms"]
    })
}

fn progress_summary_schema() -> Value {
    json!({"anyOf": [{"type": "string", "minLength": 1, "maxLength": 2048}, {"type": "null"}]})
}

fn mechanical_plan_schema() -> Value {
    json!({
        "type": "object", "additionalProperties": false,
        "properties": {
            "completion_conditions": {"type": "array", "maxItems": 8, "items": {"type": "string", "minLength": 1, "maxLength": 512}, "description": "Fixed durable completion intent, not Server-evaluated predicates."},
            "steps": {"type": "array", "maxItems": 32, "items": goal_step_schema()},
            "progress_summary": progress_summary_schema(),
            "checkpoint_at_unix_ms": nullable_integer("Last committed recovery checkpoint, or null before the first checkpoint.")
        },
        "required": ["completion_conditions", "steps", "progress_summary", "checkpoint_at_unix_ms"]
    })
}

fn goal_detail_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "summary": goal_summary_schema(),
            "plan": mechanical_plan_schema(),
            "objective": {"type": "string", "minLength": 1, "maxLength": 8192, "description": "Bounded authoritative high-level objective/instruction; the Store enforces the same 8192-byte UTF-8 ceiling."},
            "controller_agent_id": nullable_controller_agent_id("Exact durable Goal attention-routing Agent, or null when no workflow-stall controller was specified. Identity only; it grants no Goal, Task, Project, Session, Runner, Endpoint, or execution authority."),
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
        "required": ["summary", "plan", "objective", "controller_agent_id", "terminal_reason", "correlations"]
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
            "observation_lease_ms": {"type": "integer", "const": 75000, "description": "Server-owned Goal Plan card observation lease. Successful App sync calls renew liveness evidence; browser timestamps are never accepted."},
            "last_seen_at_ms": nullable_integer("Latest caller-visible WebCodex Window activity, including non-meaningful Host/App control traffic."),
            "last_meaningful_activity_at_ms": nullable_integer("Latest caller-visible meaningful WebCodex business activity across correlated Windows."),
            "quiet_for_ms": nullable_integer("Milliseconds since latest visible meaningful activity at projection time, or null when unobserved."),
            "linked_window_count": {"anyOf": [{"type": "integer", "minimum": 0, "maximum": 16}, {"type": "null"}], "description": "Bounded count of legally observable correlated Window candidates, or null when runtime observation is unavailable/not applicable."},
            "active_meaningful_request_count": {"anyOf": [{"type": "integer", "minimum": 0, "maximum": 64}, {"type": "null"}], "description": "Visible in-flight meaningful WebCodex requests across candidate Windows, or null when unavailable/not applicable."},
            "coverage_partial": {"type": "boolean", "description": "True when bounded scans or active-request retention may omit evidence. Partial coverage never produces attention_needed."}
        },
        "required": [
            "available", "state", "idle_threshold_ms", "observation_lease_ms", "last_seen_at_ms",
            "last_meaningful_activity_at_ms", "quiet_for_ms", "linked_window_count",
            "active_meaningful_request_count", "coverage_partial"
        ],
        "description": "Payload-free ClientWindow liveness evidence derived only after exact Goal/Session/Project re-authorization. It grants no authority and never exposes Window keys or raw Host metadata."
    })
}

fn goal_continuity_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "available": {"type": "boolean", "description": "Whether the bounded continuity join is available. False is fail-closed and never implies absence of durable continuation state."},
            "state": {"type": "string", "enum": ["ready", "stalled", "wake_queued", "dispatching", "host_accepted", "host_unknown", "resume_confirmed", "not_configured", "not_applicable", "unavailable"], "description": "Current exact-Goal continuity summary. Host acceptance/delivery is distinct from exact Wake-consume resume confirmation."},
            "production_auto_resume_available": {"type": "boolean", "description": "Current exact controller Agent/generation production Host-carrier readiness only. It grants no execution authority and is not proof that a Wake exists or a fresh turn ran."},
            "wake_state": {"anyOf": [{"type": "string", "enum": ["pending", "claimed", "prepared", "delivered", "delivery_unknown", "consumed", "retired"]}, {"type": "null"}], "description": "Bounded durable lifecycle of the exact current Goal-stall Wake, or null when no current-epoch Wake is correlated."},
            "host_delivery": {"type": "string", "enum": ["not_started", "dispatching", "accepted", "unknown", "not_confirmed", "not_applicable"], "description": "Bounded Host-delivery observation for the current Goal-stall Wake, or for the retained most recent confirmed resume when no current-epoch Wake exists. accepted never means the fresh model turn ran; unknown remains uncertain."},
            "fresh_turn": {"type": "string", "enum": ["not_confirmed", "confirmed", "not_applicable"], "description": "Fresh-turn proof for the current Goal-stall Wake, or for the retained most recent confirmed resume when no current-epoch Wake exists. confirmed requires exact corresponding durable Wake consume."},
            "attention_candidate_at_unix_ms": nullable_integer("Server-derived inactivity threshold instant for the current stall epoch, or for the most recent confirmed Goal-stall continuation after newer meaningful work begins; null when no bounded timeline exists."),
            "attention_created_at_unix_ms": nullable_integer("Durable goal_workflow_stalled Attention creation time for the current stall epoch or most recent confirmed continuation timeline, or null before any commit."),
            "wake_created_at_unix_ms": nullable_integer("Durable Goal-stall Wake creation time for the current stall epoch or most recent confirmed continuation timeline, or null before any commit."),
            "host_dispatch_prepared_at_unix_ms": nullable_integer("Existing Wake Attempt preparation time, or null before the Host dispatch fence."),
            "host_dispatch_accepted_at_unix_ms": nullable_integer("Host dispatch acceptance time, or null when not accepted. Acceptance never proves a fresh turn."),
            "host_dispatch_unknown_at_unix_ms": nullable_integer("Conservative Host delivery-unknown time, or null when delivery is not uncertain."),
            "wake_consumed_at_unix_ms": nullable_integer("Exact Goal-stall Wake consume time proving the current or most recent confirmed fresh turn, or null before any consume."),
            "first_post_resume_meaningful_at_unix_ms": nullable_integer("First durable meaningful Window activity correlated to the exact resumed Workflow Session after Wake consume, or null."),
            "last_post_resume_meaningful_at_unix_ms": nullable_integer("Latest durable meaningful Window activity correlated to the exact resumed Workflow Session after Wake consume, or null."),
            "last_resume_at_unix_ms": nullable_integer("Most recent exact Goal-stall Wake consume time for bounded historical context, or null. Historical resume never determines the current continuity state.")
        },
        "required": [
            "available", "state", "production_auto_resume_available", "wake_state",
            "host_delivery", "fresh_turn", "attention_candidate_at_unix_ms",
            "attention_created_at_unix_ms", "wake_created_at_unix_ms",
            "host_dispatch_prepared_at_unix_ms", "host_dispatch_accepted_at_unix_ms",
            "host_dispatch_unknown_at_unix_ms", "wake_consumed_at_unix_ms",
            "first_post_resume_meaningful_at_unix_ms", "last_post_resume_meaningful_at_unix_ms",
            "last_resume_at_unix_ms"
        ],
        "description": "Read-only exact-Goal continuity observability. It exposes no Wake/Attempt/Endpoint/Host-binding ids, consume tokens, claim fences, principal identity, Project path, Session ledger, stdout, or stderr."
    })
}

fn goal_plan_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "version": {"type": "integer", "const": 3, "description": "Current bounded Goal workflow plan projection."},
            "goal_id": {"type": "string", "pattern": "^wc_goal_[A-Za-z0-9_-]{16}$", "description": "Exact durable Goal identity used for refresh/rehydration and app-only polling. Identity is never authority."},
            "title": {"type": "string", "minLength": 1, "maxLength": 200, "description": "Bounded Goal title."},
            "total_step_count": {"type": "integer", "minimum": 0, "maximum": 32},
            "completed_step_count": {"type": "integer", "minimum": 0, "maximum": 32},
            "current_step_id": {"anyOf": [{"type": "string", "pattern": "^[A-Za-z0-9_-]{1,32}$"}, {"type": "null"}]},
            "steps": {"type": "array", "maxItems": 32, "items": goal_step_schema()},
            "progress_summary": progress_summary_schema(),
            "checkpoint_at_unix_ms": nullable_integer("Last committed Goal recovery checkpoint."),
            "controller_agent_id": nullable_controller_agent_id("Exact durable Goal attention-routing Agent, or null when workflow-stall continuation is unavailable. Endpoint/window bindings are never projected."),
            "lifecycle": lifecycle_schema(),
            "revision": {"type": "integer", "minimum": 1, "description": "Monotonic authoritative Goal revision."},
            "updated_at_unix_ms": schema_type("integer", "Latest authoritative Goal mutation time."),
            "terminal_at_unix_ms": nullable_integer("Terminal transition time, or null while active."),
            "agent_task_count": {"type": "integer", "minimum": 0, "maximum": 64, "description": "Count of explicit AgentTask correlations; no target-domain state or authority is projected."},
            "workflow_session_count": {"type": "integer", "minimum": 0, "maximum": 64, "description": "Count of explicit Workflow Session correlations; no Session ledger, Project state, or authority is projected."},
            "activity": goal_activity_schema(),
            "continuity": goal_continuity_schema()
        },
        "required": [
            "version", "goal_id", "title", "total_step_count", "completed_step_count", "current_step_id", "steps", "progress_summary", "checkpoint_at_unix_ms", "controller_agent_id", "lifecycle", "revision",
            "updated_at_unix_ms", "terminal_at_unix_ms", "agent_task_count",
            "workflow_session_count", "activity", "continuity"
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

pub(super) fn goal_follow_up_schema() -> Value {
    json!({
        "type": "object", "additionalProperties": false,
        "description": "Owned active Goals explicitly correlated to the authorized Workflow Session. Progress follow-up only; no automatic completion. When incomplete, checkpoint_goal with this exact revision; otherwise freshly verify/review completion intent and explicitly update_goal to completed. Missing authority omits this field; unavailable evidence never implies no active Goal.",
        "properties": {
            "available": {"type": "boolean"},
            "truncated": {"type": "boolean"},
            "goals": {"type": "array", "maxItems": 8, "items": {
                "type": "object", "additionalProperties": false,
                "properties": {
                    "goal_id": {"type": "string", "pattern": "^wc_goal_[A-Za-z0-9_-]{16}$"},
                    "revision": {"type": "integer", "minimum": 1},
                    "incomplete_step_count": {"type": "integer", "minimum": 0, "maximum": 32},
                    "current_step": {"anyOf": [{"type": "null"}, {
                        "type": "object", "additionalProperties": false,
                        "properties": {"id": {"type": "string", "pattern": "^[A-Za-z0-9_-]{1,32}$"}, "title": {"type": "string", "minLength": 1, "maxLength": 120}},
                        "required": ["id", "title"]
                    }]},
                    "next_action": {"type": "string", "enum": ["checkpoint_goal", "update_goal"]}
                },
                "required": ["goal_id", "revision", "incomplete_step_count", "current_step", "next_action"]
            }}
        },
        "required": ["available", "truncated", "goals"]
    })
}

pub fn output_schema_for_tool(name: &str) -> Option<Value> {
    let schema = match name {
        "prepare_goal_workflow"
        | "create_goal"
        | "update_goal"
        | "checkpoint_goal"
        | "associate_goal_agent_task"
        | "associate_goal_workflow_session" => goal_mutation_schema(),
        "get_goal" => wrapped_output_schema(vec![("goal", goal_detail_schema())]),
        "present_goal_plan" | "goal_plan_sync" => {
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
