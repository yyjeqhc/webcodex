//! Compact MCP selection copy, applied only to owned tools/list projections.
//! Exact manifests and canonical ToolSpecs retain the operational contract.

use serde_json::Value;

use super::tools::RECORDING_SESSION_SELECTOR_SCHEMA_PATTERN;

// MCP discovery targets, independent of GPT Actions' importer limits. Keep
// purpose, the nearest selection boundary, and essential continuation guidance.
pub(super) const TOOL_DESCRIPTION_MAX_CHARS: usize = 420;
pub(super) const INPUT_DESCRIPTION_MAX_CHARS: usize = 180;

pub(super) fn compact_tool(tool: &mut Value) {
    let name = tool["name"].as_str().unwrap_or_default().to_string();
    if let Some(description) = tool["description"].as_str() {
        let selection = match name.as_str() {
            "work_on_project" => "Start ordinary coding/review with project or client_id+path. Omit session_id for a fresh Workflow Session; supply it only for exact resume. Defaults return project instructions, workflow and extension guidance. Use mode=worktree for an isolated Git worktree.",
            "tool_manifest" => "Discover tools by intent/category, or pass tool_name for one exact canonical contract plus route.primary/route.fallback. Discovery never registers a new Host tool. If a direct callable is absent, follow the exact gateway fallback when it is allowed.",
            "call_runtime_tool" => "Call one admitted runtime tool with its exact arguments. Use tool_manifest to discover the contract. Prefer an available direct callable; ordinary direct tools may fall back here when unavailable, but MCP App presentation tools must use their direct callable while Apps are enabled. Target validation and authority checks still apply.",
            "run_process" => "Run one native executable with literal argv. Use run_shell for shell grammar or a short related command chain. Long work continues as the same Runner-owned Job through observe_jobs; retain the returned continuation instead of redispatching.",
            "run_shell" => "Run shell grammar or a short related command chain. Use run_process for one native executable with literal argv. Long work continues as the same Runner-owned Job through observe_jobs; retain the returned continuation instead of redispatching.",
            "run_detached_process" => "Start a native child that intentionally survives Runner restart or replacement as a durable Job. Duration alone does not require detachment. Requires an idempotency_key; retain the same Job and use observe_jobs or stop_job after handoff uncertainty.",
            "observe_jobs" => "Continue known Jobs by job_id; do not list first. Pass observation_token unchanged as after_observation_token. Follow the returned continuation for more output; observation never redispatches work. Use wait_for_job_terminal when blocked only on terminal completion.",
            "list_jobs" => "Recover or inventory caller-visible Job identities. When a job_id or continuation is already known, use observe_jobs directly.",
            "wait_for_job_terminal" => "Arm a bounded one-shot terminal wait for one exact existing Job. Reuse the keyed wait and returned continuation; never redispatch the Job. Continue independent work, or follow the offered Host continuation when only terminal completion blocks progress.",
            "stop_job" => "Stop one existing Job by exact job_id with confirm=true. Preserves Project and Session ownership. Use observe_jobs to inspect output or wait_for_job_terminal to wait without stopping.",
            "present_agent_continuation" => "Present one exact Agent/Endpoint generation as the persistent MCP App continuation card. New window setup: create_agent_identity -> rotate_agent_continuation_endpoint -> present_agent_continuation, then yield/end promptly. Presentation success is not wake readiness; later verify list_agent_identities.production_auto_resume_available.",
            _ => description,
        };
        tool["description"] =
            Value::String(bound_description(selection, TOOL_DESCRIPTION_MAX_CHARS));
    }
    if let Some(schema) = tool.get_mut("inputSchema") {
        compact_input_descriptions(schema);
        compact_control_sidecar(schema);
        if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
            for (field, property) in properties {
                if let (Some(description), Some(Value::String(copy))) = (
                    common_input_description(&name, field),
                    property.get_mut("description"),
                ) {
                    *copy = description.to_string();
                }
            }
        }
        // Only these root properties are protocol wrappers. A business
        // session_id keeps its own canonical-derived copy and requiredness.
        // Nested IDs/resolution have no description; keep their type hints here.
        for (pointer, description) in [
            ("/properties/recording_session_id", "Optional explicit recorder for one exact Workflow Session (wc_sess_* or issued ~sN); never execution/business authority. Omission may still allow authorized same-Window attention without recording."),
            ("/properties/ack_session_message_ids", "ACK-required wc_msg_* IDs retained in model context; Session uses recorder/window affinity; never resolves or authorizes."),
            ("/properties/ack_ref", "Compact exact Session ACK-set evidence returned in session_attention; Session-only, request-scoped, non-authoritative, and never resolves messages."),
            ("/properties/session_message_resolution", "Resolve one handled non-todo recorder message by exact wc_msg_*; ACK separately if required. Independent of call success."),
            ("/properties/context_request", "Post-result sidecar keys; no authority: project.instructions, webcodex.workflow, jobs.attention, skills.catalog, plugins.catalog, memory.bootstrap."),
            ("/properties/context_request/items", "Context key; unsupported keys are nonfatal."),
        ] {
            if let Some(Value::String(copy)) = schema
                .pointer_mut(pointer)
                .and_then(|property| property.get_mut("description"))
            {
                *copy = description.to_string();
            }
        }
        // ACK ref is echoed verbatim from session_attention. Repeating the
        // explanation on every compact tool would dominate the token savings.
        // Full discovery keeps the safety contract; compact keeps field/bound.
        if let Some(property) = schema
            .pointer_mut("/properties/ack_ref")
            .and_then(Value::as_object_mut)
        {
            property.remove("description");
        }
        compact_discovery_validation_annotations(schema);
    }
}

fn compact_control_sidecar(schema: &mut Value) {
    let Some(control) = schema
        .pointer_mut("/properties/_control")
        .filter(|value| value.is_object())
    else {
        return;
    };
    // Full MCP discovery retains the exact closed per-kind canonical schemas.
    // Compact discovery is only a model-selection copy, so do not repeat those
    // large canonical payload schemas on every ordinary tool. Runtime stripping,
    // closed enum parsing, and canonical ToolCall parsing remain unchanged.
    *control = serde_json::json!({
        "type": "object",
        "description": "Optional explicit control piggyback; exact payloads use the full MCP schema and canonical standalone-tool contracts."
    });
}

fn compact_discovery_validation_annotations(schema: &mut Value) {
    // Only copied opaque IDs in protocol wrappers, at these exact schema
    // positions and with these exact patterns. Business IDs, fences, resource
    // paths and all bounds stay intact. Full discovery and runtime validation
    // use their original schemas/parsers, never this owned presentation copy.
    for (pointer, pattern) in [
        (
            "/properties/recording_session_id",
            RECORDING_SESSION_SELECTOR_SCHEMA_PATTERN,
        ),
        (
            "/properties/ack_session_message_ids/items",
            "^wc_msg_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$",
        ),
        (
            "/properties/session_message_resolution/properties/message_id",
            "^wc_msg_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$",
        ),
    ] {
        if let Some(property) = schema.pointer_mut(pointer).and_then(Value::as_object_mut) {
            if property.get("type").and_then(Value::as_str) == Some("string")
                && property.get("pattern").and_then(Value::as_str) == Some(pattern)
            {
                property.remove("pattern");
            }
        }
    }
}

fn common_input_description(tool: &str, field: &str) -> Option<&'static str> {
    // Root arguments only, in audited groups with the same semantics. In
    // particular run_shell cwd/timeouts can refer to a named SSH resource;
    // project, client_id and idempotency_key also differ between direct tools.
    Some(match (tool, field) {
        ("run_process" | "run_detached_process", "cwd") =>
            "Project-relative cwd; omit, empty or '.' for root. No named Session SSH resources.",
        ("run_skill_resource", "cwd") =>
            "Project-relative cwd; omit, empty or '.' for root. Skill resolution does not change cwd.",
        ("run_process" | "run_detached_process", "timeout_secs") =>
            "Total runtime seconds; default 60, clamped to 604800 (7 days).",
        ("run_skill_resource", "timeout_secs") =>
            "Total runtime seconds; default 60, clamped to 3600.",
        ("cargo_check" | "cargo_test", "timeout_secs") =>
            "Total validation runtime seconds, clamped to 3600. Defaults vary per tool.",
        ("run_process" | "run_script" | "run_skill_resource" | "cargo_check" | "cargo_test" | "go_test", "sync_wait_secs") =>
            "Same-execution Job handoff grace; default 10s, clamped to 55s and timeout. Never extends runtime or retries.",
        ("run_shell", "sync_wait_secs") =>
            "Same-execution Job handoff grace; default 10s, clamped to 55s and timeout; controls return only. Named Session SSH unsupported.",
        ("run_process" | "run_shell", "assertion_name") =>
            "Validation label; reuse after a fix to correlate evidence. Inert unless execution is validation-like.",
        _ => return None,
    })
}

fn compact_input_descriptions(schema: &mut Value) {
    // Traverse schema positions only: const/default/enum/examples may contain
    // business data named "description" that must never be rewritten.
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    if let Some(Value::String(description)) = object.get_mut("description") {
        *description = bound_description(description, INPUT_DESCRIPTION_MAX_CHARS);
    }
    for keyword in [
        "properties",
        "patternProperties",
        "$defs",
        "definitions",
        "dependentSchemas",
        "dependencies",
    ] {
        if let Some(children) = object.get_mut(keyword).and_then(Value::as_object_mut) {
            for child in children.values_mut() {
                compact_input_descriptions(child);
            }
        }
    }
    for keyword in [
        "items",
        "prefixItems",
        "allOf",
        "anyOf",
        "oneOf",
        "additionalItems",
        "additionalProperties",
        "unevaluatedItems",
        "unevaluatedProperties",
        "propertyNames",
        "contains",
        "not",
        "if",
        "then",
        "else",
    ] {
        if let Some(child) = object.get_mut(keyword) {
            if let Some(children) = child.as_array_mut() {
                for child in children {
                    compact_input_descriptions(child);
                }
            } else {
                compact_input_descriptions(child);
            }
        }
    }
}

pub(super) fn bound_description(description: &str, max_chars: usize) -> String {
    let description = description.trim();
    if description.chars().count() <= max_chars {
        return description.to_string();
    }
    // Prefer complete sentences; periods in foo.rs, v0.4.0, and context keys
    // are not sentence boundaries. Fall back to a Unicode-safe word prefix.
    let prefix: String = description.chars().take(max_chars - 1).collect();
    if let Some(end) = prefix
        .char_indices()
        .filter_map(|(index, ch)| {
            let end = index + ch.len_utf8();
            (matches!(ch, '.' | '!' | '?')
                && description[end..]
                    .chars()
                    .next()
                    .is_none_or(char::is_whitespace))
            .then_some(end)
        })
        .last()
    {
        return prefix[..end].to_string();
    }
    let end = prefix
        .rfind(char::is_whitespace)
        .filter(|index| *index > 0)
        .unwrap_or(prefix.len());
    format!("{}…", prefix[..end].trim_end())
}
