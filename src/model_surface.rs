//! Canonical model-facing routing for Adaptive Runtime.
//!
//! WebCodex has one model-facing runtime contract. Canonical ToolDefinitions
//! decide which model-visible tools are directly exposed; every other
//! model-visible runtime tool is reached through `call_runtime_tool`. Hidden
//! protocol extensions are reachable only after the adapter independently
//! admits their protocol capability.

use crate::tool_runtime::tool_definition::{
    adaptive_runtime_direct_tool_definitions, is_adaptive_runtime_direct_tool,
    is_model_visible_tool_name,
};
use crate::tool_runtime::{registered_tool_specs, ToolResult, ToolSpec};
use serde_json::{json, Value};
use std::collections::HashSet;

pub(crate) const ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME: &str = "call_runtime_tool";
pub(crate) const TOOL_SURFACE_AVAILABILITY_DIRECT: &str = "direct";
pub(crate) const TOOL_SURFACE_AVAILABILITY_GATEWAY: &str = "gateway";
pub(crate) const TOOL_SURFACE_AVAILABILITY_UNAVAILABLE: &str = "unavailable";

pub(crate) fn tool_requires_direct_app_presentation(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "present_work_result"
            | "present_pdf"
            | "present_goal_plan"
            | "present_agent_continuation"
            | "present_job_terminal_continuation"
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdaptiveRuntimeGatewayTargetRoute {
    Gateway,
    Direct,
    Recursive,
    Unknown,
}

/// Canonical Adaptive Runtime routing for one ordinary registered tool.
/// Routing changes model exposure only; canonical authority checks are unchanged.
pub(crate) fn adaptive_runtime_tool_invocation_route(
    tool_name: &str,
) -> (&'static str, Option<&'static str>) {
    adaptive_runtime_tool_invocation_route_with_operator_extension(tool_name, false)
}

/// Route a tool after a protocol adapter has independently admitted a hidden
/// extension. This flag is server-owned request context, not caller authority.
pub(crate) fn adaptive_runtime_tool_invocation_route_with_operator_extension(
    tool_name: &str,
    operator_extension_admitted: bool,
) -> (&'static str, Option<&'static str>) {
    if operator_extension_admitted
        || webcodex_tool_contracts::EXACT_MANIFEST_SPECIALIST_TOOL_NAMES.contains(&tool_name)
    {
        return (
            TOOL_SURFACE_AVAILABILITY_GATEWAY,
            Some(ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME),
        );
    }
    if !is_model_visible_tool_name(tool_name) {
        return (TOOL_SURFACE_AVAILABILITY_UNAVAILABLE, None);
    }
    if is_adaptive_runtime_direct_tool(tool_name) {
        (TOOL_SURFACE_AVAILABILITY_DIRECT, None)
    } else {
        (
            TOOL_SURFACE_AVAILABILITY_GATEWAY,
            Some(ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME),
        )
    }
}

pub(crate) fn adaptive_runtime_gateway_target_route(
    target: &str,
) -> AdaptiveRuntimeGatewayTargetRoute {
    if target == ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME {
        return AdaptiveRuntimeGatewayTargetRoute::Recursive;
    }
    match adaptive_runtime_tool_invocation_route(target) {
        (TOOL_SURFACE_AVAILABILITY_DIRECT, None) => AdaptiveRuntimeGatewayTargetRoute::Direct,
        (TOOL_SURFACE_AVAILABILITY_GATEWAY, Some(ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME)) => {
            AdaptiveRuntimeGatewayTargetRoute::Gateway
        }
        _ => AdaptiveRuntimeGatewayTargetRoute::Unknown,
    }
}

/// Presentation route for one canonical SuggestedToolCall target. This is not
/// authority: adapters resolve the route from their already-admitted model
/// surface and the canonical target still runs through ordinary ToolRuntime
/// validation, authorization, permission, capability, and effect checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SuggestedToolCallRoute {
    Direct,
    Gateway(&'static str),
    Unavailable,
}

pub(crate) fn suggested_tool_call_route(
    target: &str,
    operator_extension_admitted: bool,
) -> SuggestedToolCallRoute {
    match adaptive_runtime_tool_invocation_route_with_operator_extension(
        target,
        operator_extension_admitted,
    ) {
        (TOOL_SURFACE_AVAILABILITY_DIRECT, None) => SuggestedToolCallRoute::Direct,
        (TOOL_SURFACE_AVAILABILITY_GATEWAY, Some(gateway)) => {
            SuggestedToolCallRoute::Gateway(gateway)
        }
        _ => SuggestedToolCallRoute::Unavailable,
    }
}

/// Project formally declared SuggestedToolCall schemas to the callable shape of
/// one model surface. Domain ToolSpecs remain canonical; adapters apply this to
/// response-schema copies only.
pub(crate) fn project_suggested_tool_call_schema<F>(schema: &mut Value, route_for: &F)
where
    F: Fn(&str) -> SuggestedToolCallRoute,
{
    if project_suggested_tool_call_schema_node(schema, route_for) {
        *schema = json!({"not": {}});
    }
}

fn project_suggested_tool_call_schema_node<F>(schema: &mut Value, route_for: &F) -> bool
where
    F: Fn(&str) -> SuggestedToolCallRoute,
{
    if let Some(target) =
        webcodex_tool_contracts::suggested_tool_call_schema_target(schema).map(str::to_string)
    {
        return match route_for(&target) {
            SuggestedToolCallRoute::Direct => false,
            SuggestedToolCallRoute::Gateway(gateway) => {
                let mut canonical = std::mem::take(schema);
                let description = canonical.get("description").cloned();
                let properties = canonical
                    .get_mut("properties")
                    .and_then(Value::as_object_mut)
                    .expect("recognized SuggestedToolCall schema has properties");
                let follow_up_kind = properties
                    .remove("follow_up_kind")
                    .expect("recognized SuggestedToolCall schema has follow_up_kind");
                let canonical_tool = properties
                    .remove("tool")
                    .expect("recognized SuggestedToolCall schema has tool");
                let canonical_arguments = properties
                    .remove("arguments")
                    .expect("recognized SuggestedToolCall schema has arguments");
                *schema = json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "follow_up_kind": follow_up_kind,
                        "tool": {"type": "string", "const": gateway},
                        "arguments": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {
                                "tool": canonical_tool,
                                "arguments": canonical_arguments
                            },
                            "required": ["tool", "arguments"]
                        }
                    },
                    "required": ["follow_up_kind", "tool", "arguments"]
                });
                if let Some(description) = description {
                    schema["description"] = description;
                }
                false
            }
            SuggestedToolCallRoute::Unavailable => true,
        };
    }
    let mut removed_properties = Vec::new();
    if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        let names = properties.keys().cloned().collect::<Vec<_>>();
        for name in names {
            if properties
                .get_mut(&name)
                .is_some_and(|child| project_suggested_tool_call_schema_node(child, route_for))
            {
                removed_properties.push(name);
            }
        }
        for name in &removed_properties {
            properties.remove(name);
        }
    }
    if !removed_properties.is_empty() {
        if let Some(required) = schema.get_mut("required").and_then(Value::as_array_mut) {
            required.retain(|field| {
                field
                    .as_str()
                    .is_none_or(|field| !removed_properties.iter().any(|name| name == field))
            });
        }
    }

    let item_became_unavailable = schema
        .get_mut("items")
        .is_some_and(|items| project_suggested_tool_call_schema_node(items, route_for));
    if item_became_unavailable {
        if schema.get("minItems").and_then(Value::as_u64).unwrap_or(0) > 0 {
            return true;
        }
        schema["items"] = json!({"not": {}});
    }

    for keyword in ["anyOf", "oneOf"] {
        let Some(branches) = schema.get_mut(keyword).and_then(Value::as_array_mut) else {
            continue;
        };
        let mut index = 0;
        while index < branches.len() {
            if project_suggested_tool_call_schema_node(&mut branches[index], route_for) {
                branches.remove(index);
            } else {
                index += 1;
            }
        }
        if branches.is_empty() {
            return true;
        }
    }
    if let Some(branches) = schema.get_mut("allOf").and_then(Value::as_array_mut) {
        for branch in branches {
            if project_suggested_tool_call_schema_node(branch, route_for) {
                return true;
            }
        }
    }
    for keyword in ["then", "else"] {
        if let Some(branch) = schema.get_mut(keyword) {
            if project_suggested_tool_call_schema_node(branch, route_for) {
                *branch = json!({"not": {}});
            }
        }
    }
    false
}

/// Project only values proven by the canonical output schema to be
/// SuggestedToolCall edges. The visited JSON-pointer set prevents one runtime
/// value from being rewritten twice when `oneOf`/`allOf` branches describe the
/// same output location.
pub(crate) fn project_suggested_tool_calls_in_value<F>(
    value: &mut Value,
    schema: &Value,
    route_for: &F,
) where
    F: Fn(&str) -> SuggestedToolCallRoute,
{
    let mut visited = HashSet::new();
    if project_suggested_tool_calls_in_value_node(value, schema, route_for, "", &mut visited) {
        *value = Value::Null;
    }
}

fn project_suggested_tool_calls_in_value_node<F>(
    value: &mut Value,
    schema: &Value,
    route_for: &F,
    path: &str,
    visited: &mut HashSet<String>,
) -> bool
where
    F: Fn(&str) -> SuggestedToolCallRoute,
{
    if let Some(target) = webcodex_tool_contracts::suggested_tool_call_schema_target(schema) {
        if !visited.insert(path.to_string()) {
            return false;
        }
        let expected_follow_up_kind = schema
            .pointer("/properties/follow_up_kind/const")
            .and_then(Value::as_str)
            .expect("recognized SuggestedToolCall schema has a constant follow_up_kind");
        if value.get("tool").and_then(Value::as_str) != Some(target)
            || value.get("arguments").is_none()
            || value.get("follow_up_kind").and_then(Value::as_str) != Some(expected_follow_up_kind)
        {
            // This location is formally declared as a server-generated call. A
            // runtime target/posture mismatch must not remain machine-actionable:
            // remove the edge rather than letting a Host infer authority from a
            // value that contradicts the advertised output contract.
            return true;
        }
        return match route_for(target) {
            SuggestedToolCallRoute::Direct => false,
            SuggestedToolCallRoute::Gateway(gateway) => {
                let follow_up_kind = value
                    .get_mut("follow_up_kind")
                    .map(Value::take)
                    .unwrap_or(Value::Null);
                let arguments = value
                    .get_mut("arguments")
                    .map(Value::take)
                    .unwrap_or(Value::Null);
                *value = json!({
                    "follow_up_kind": follow_up_kind,
                    "tool": gateway,
                    "arguments": {
                        "tool": target,
                        "arguments": arguments
                    }
                });
                false
            }
            SuggestedToolCallRoute::Unavailable => true,
        };
    }

    if let (Some(properties), Some(object)) = (
        schema.get("properties").and_then(Value::as_object),
        value.as_object_mut(),
    ) {
        let names = properties.keys().cloned().collect::<Vec<_>>();
        let mut remove = Vec::new();
        for name in names {
            let Some(child_value) = object.get_mut(&name) else {
                continue;
            };
            let Some(child_schema) = properties.get(&name) else {
                continue;
            };
            let child_path = format!("{}/{}", path, json_pointer_segment(&name));
            if project_suggested_tool_calls_in_value_node(
                child_value,
                child_schema,
                route_for,
                &child_path,
                visited,
            ) {
                remove.push(name);
            }
        }
        for name in remove {
            object.remove(&name);
        }
    }

    if let (Some(item_schema), Some(items)) = (schema.get("items"), value.as_array_mut()) {
        let mut remove = Vec::new();
        for (index, item) in items.iter_mut().enumerate() {
            let child_path = format!("{path}/{index}");
            if project_suggested_tool_calls_in_value_node(
                item,
                item_schema,
                route_for,
                &child_path,
                visited,
            ) {
                remove.push(index);
            }
        }
        for index in remove.into_iter().rev() {
            items.remove(index);
        }
    }

    for keyword in ["anyOf", "oneOf", "allOf"] {
        if let Some(branches) = schema.get(keyword).and_then(Value::as_array) {
            let has_matching_required_shape = keyword != "allOf"
                && branches.iter().any(|branch| {
                    branch
                        .get("required")
                        .and_then(Value::as_array)
                        .is_some_and(|required| {
                            !required.is_empty()
                                && required
                                    .iter()
                                    .filter_map(Value::as_str)
                                    .all(|key| value.get(key).is_some())
                        })
                });
            for branch in branches {
                // Alternative object representations may declare different calls at
                // the same path. Do not let an inapplicable representation erase an
                // edge before its matching representation can project it. This is
                // structural selection only; target/posture checks below remain
                // fail closed, and conjunctive schemas still all apply.
                if has_matching_required_shape
                    && branch
                        .get("required")
                        .and_then(Value::as_array)
                        .is_some_and(|required| {
                            required
                                .iter()
                                .filter_map(Value::as_str)
                                .any(|key| value.get(key).is_none())
                        })
                {
                    continue;
                }
                if project_suggested_tool_calls_in_value_node(
                    value, branch, route_for, path, visited,
                ) {
                    return true;
                }
            }
        }
    }
    for keyword in ["then", "else"] {
        if let Some(branch) = schema.get(keyword) {
            if project_suggested_tool_calls_in_value_node(value, branch, route_for, path, visited) {
                return true;
            }
        }
    }
    false
}

fn json_pointer_segment(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

pub(crate) fn project_tool_result_suggested_calls<F>(
    tool_name: &str,
    result: &mut ToolResult,
    route_for: &F,
) where
    F: Fn(&str) -> SuggestedToolCallRoute,
{
    let output = result.output.take();
    let mut envelope = json!({"success": result.success, "output": output});
    if let Some(error) = result.error.as_ref() {
        envelope["error"] = Value::String(error.clone());
    }
    let schema = webcodex_tool_contracts::output_schema_for_tool(tool_name);
    project_suggested_tool_calls_in_value(&mut envelope, &schema, route_for);
    result.output = envelope
        .as_object_mut()
        .and_then(|object| object.remove("output"))
        .unwrap_or(Value::Null);
}

/// Compact MCP discovery is the Adaptive Runtime default. The explicit
/// operator override changes schema projection only, never tool behavior.
pub(crate) fn effective_mcp_compact_schemas(configured_override: Option<bool>) -> bool {
    configured_override.unwrap_or(true)
}

/// Direct ToolSpecs ordered by rank declared on canonical ToolDefinitions.
pub(crate) fn adaptive_runtime_direct_tool_specs() -> Vec<ToolSpec> {
    let mut by_name: std::collections::HashMap<String, ToolSpec> = registered_tool_specs()
        .into_iter()
        .map(|spec| (spec.name.clone(), spec))
        .collect();
    adaptive_runtime_direct_tool_definitions()
        .into_iter()
        .map(|definition| {
            by_name.remove(definition.name).unwrap_or_else(|| {
                panic!(
                    "{} adaptive_runtime direct tool is missing a registered ToolSpec",
                    definition.name
                )
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect_suggested_call_targets(
        schema: &Value,
        targets: &mut std::collections::BTreeSet<String>,
    ) {
        if let Some(target) = webcodex_tool_contracts::suggested_tool_call_schema_target(schema) {
            targets.insert(target.to_string());
            return;
        }
        match schema {
            Value::Object(object) => {
                for child in object.values() {
                    collect_suggested_call_targets(child, targets);
                }
            }
            Value::Array(items) => {
                for child in items {
                    collect_suggested_call_targets(child, targets);
                }
            }
            _ => {}
        }
    }

    fn test_suggested_call_route(target: &str) -> SuggestedToolCallRoute {
        let operator_extension_admitted =
            crate::tool_runtime::stateless_operator_extension_tool_specs()
                .iter()
                .any(|spec| spec.name == target);
        suggested_tool_call_route(target, operator_extension_admitted)
    }

    #[test]
    fn direct_app_presentation_requirement_is_closed_and_explicit() {
        for tool in [
            "present_work_result",
            "present_pdf",
            "present_goal_plan",
            "present_agent_continuation",
            "present_job_terminal_continuation",
        ] {
            assert!(tool_requires_direct_app_presentation(tool), "{tool}");
        }
        for ordinary in ["run_shell", "run_script", "read_workspace_changes"] {
            assert!(
                !tool_requires_direct_app_presentation(ordinary),
                "{ordinary}"
            );
        }
    }

    #[test]
    fn direct_specs_are_definition_derived_and_model_visible() {
        let specs = adaptive_runtime_direct_tool_specs();
        let expected = adaptive_runtime_direct_tool_definitions()
            .into_iter()
            .map(|definition| definition.name)
            .collect::<Vec<_>>();
        let actual = specs
            .iter()
            .map(|spec| spec.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
        for spec in specs {
            assert!(is_model_visible_tool_name(&spec.name), "{}", spec.name);
            assert!(is_adaptive_runtime_direct_tool(&spec.name), "{}", spec.name);
        }
    }

    #[test]
    fn coding_intent_tools_are_reachable() {
        for tool_name in crate::tool_runtime::tool_definition::CODING_INTENT_TOOL_NAMES {
            let (availability, gateway) = adaptive_runtime_tool_invocation_route(tool_name);
            assert_ne!(
                availability, TOOL_SURFACE_AVAILABILITY_UNAVAILABLE,
                "{tool_name}"
            );
            if availability == TOOL_SURFACE_AVAILABILITY_DIRECT {
                assert_eq!(gateway, None, "{tool_name}");
            } else {
                assert_eq!(
                    gateway,
                    Some(ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME),
                    "{tool_name}"
                );
            }
        }
    }

    #[test]
    fn long_tail_and_direct_fallback_routes_are_canonical() {
        assert_eq!(
            adaptive_runtime_gateway_target_route("apply_patch"),
            AdaptiveRuntimeGatewayTargetRoute::Gateway
        );
        for tool_name in ["read_files", "run_script"] {
            assert_eq!(
                adaptive_runtime_gateway_target_route(tool_name),
                AdaptiveRuntimeGatewayTargetRoute::Direct
            );
        }
    }

    #[test]
    fn hidden_tools_fail_closed_without_protocol_admission() {
        assert!(!is_model_visible_tool_name("list_skills"));
        assert_eq!(
            adaptive_runtime_gateway_target_route("list_skills"),
            AdaptiveRuntimeGatewayTargetRoute::Unknown
        );
        assert_eq!(
            adaptive_runtime_tool_invocation_route_with_operator_extension("list_skills", true),
            (
                TOOL_SURFACE_AVAILABILITY_GATEWAY,
                Some(ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME)
            )
        );
    }

    #[test]
    fn gateway_is_not_recursive() {
        assert_eq!(
            adaptive_runtime_gateway_target_route(ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME),
            AdaptiveRuntimeGatewayTargetRoute::Recursive
        );
    }

    #[test]
    fn browser_tools_stay_gateway_only_on_adaptive_runtime() {
        for tool_name in ["observe_browser", "control_browser"] {
            assert!(adaptive_runtime_direct_tool_definitions()
                .iter()
                .all(|definition| definition.name != tool_name));
            assert_eq!(
                adaptive_runtime_tool_invocation_route(tool_name),
                (
                    TOOL_SURFACE_AVAILABILITY_GATEWAY,
                    Some(ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME)
                )
            );
        }
    }

    #[test]
    fn structured_suggested_call_targets_project_to_actionable_adaptive_routes() {
        let mut targets = std::collections::BTreeSet::new();
        for spec in registered_tool_specs()
            .into_iter()
            .chain(crate::tool_runtime::stateless_operator_extension_tool_specs())
        {
            collect_suggested_call_targets(&spec.output_schema, &mut targets);
        }
        for expected in [
            "list_runners",
            "read_git_log",
            "read_project_artifact_chunk",
            "list_skill_versions",
        ] {
            assert!(
                targets.contains(expected),
                "formal SuggestedToolCall discovery missed representative target {expected}: {targets:?}"
            );
        }
        assert!(!targets.is_empty());
        for target in targets {
            assert_ne!(
                test_suggested_call_route(&target),
                SuggestedToolCallRoute::Unavailable,
                "Adaptive projection must make formal SuggestedToolCall target {target} immediately executable"
            );
        }

        assert_eq!(
            adaptive_runtime_tool_invocation_route("read_session_discussion_summary"),
            (TOOL_SURFACE_AVAILABILITY_DIRECT, None),
            "session_hint.suggested_next_tool remains a non-parser-ready direct-only hint"
        );
        assert_eq!(
            adaptive_runtime_tool_invocation_route("apply_patch"),
            (
                TOOL_SURFACE_AVAILABILITY_GATEWAY,
                Some(ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME)
            ),
            "specialized patching should remain behind the Adaptive gateway"
        );
    }

    #[test]
    fn suggested_call_value_and_schema_projection_cover_representative_gateway_edges() {
        for (source_tool, target_tool, arguments) in [
            (
                "work_on_project",
                "list_runners",
                json!({"include_projects": false, "summary_only": true}),
            ),
            (
                "read_git_log",
                "read_git_log",
                json!({"project": "demo", "head_commit": "0123456789012345678901234567890123456789", "limit": 20, "skip": 20}),
            ),
            (
                "read_project_artifact_chunk",
                "read_project_artifact_chunk",
                json!({"project": "demo", "path": "out.bin", "encoding": "base64", "offset": 65536, "length": 65536, "expected_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}),
            ),
            (
                "install_skill",
                "list_skill_versions",
                json!({"project": "demo", "skill_key": "trusted-skill"}),
            ),
        ] {
            assert!(
                matches!(
                    test_suggested_call_route(target_tool),
                    SuggestedToolCallRoute::Gateway(ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME)
                ),
                "representative edge {source_tool}->{target_tool} should remain Adaptive gateway-routed"
            );
            let canonical_schema = webcodex_tool_contracts::output_schema_for_tool(source_tool);
            let follow_up_kind = if matches!(source_tool, "work_on_project" | "install_skill") {
                "fallback_recovery"
            } else {
                "mechanically_followable"
            };
            let canonical_call = json!({
                "follow_up_kind": follow_up_kind,
                "tool": target_tool,
                "arguments": arguments
            });
            webcodex_tool_contracts::test_support::validate_generated_tool_call_against_registered_input_schema(
                &canonical_call,
            )
            .unwrap_or_else(|error| {
                panic!(
                    "canonical SuggestedToolCall {source_tool}->{target_tool} must pass the target registered inputSchema unchanged: {error}"
                )
            });
            let mut projected_value = json!({
                "success": false,
                "output": {"suggested_call": canonical_call.clone()},
                "error": "recovery"
            });
            project_suggested_tool_calls_in_value(
                &mut projected_value,
                &canonical_schema,
                &test_suggested_call_route,
            );
            let projected_call = &projected_value["output"]["suggested_call"];
            assert_eq!(projected_call["tool"], ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME);
            assert_eq!(projected_call["follow_up_kind"], follow_up_kind);
            assert_eq!(projected_call["arguments"]["tool"], target_tool);
            assert_eq!(
                projected_call["arguments"]["arguments"],
                canonical_call["arguments"]
            );
            let gateway_input_schema = crate::mcp::adaptive_runtime_gateway_input_schema_for_test();
            crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
                &projected_call["arguments"],
                &gateway_input_schema,
            )
            .unwrap_or_else(|error| {
                panic!(
                    "Adaptive projected SuggestedToolCall {source_tool}->{target_tool} must pass the actual registered call_runtime_tool inputSchema unchanged: {error}"
                )
            });

            let mut projected_schema = canonical_schema;
            project_suggested_tool_call_schema(&mut projected_schema, &test_suggested_call_route);
            let suggested_schema =
                &projected_schema["properties"]["output"]["properties"]["suggested_call"];
            assert_eq!(
                suggested_schema["properties"]["tool"]["const"],
                ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME
            );
            assert_eq!(
                suggested_schema["properties"]["follow_up_kind"]["const"],
                follow_up_kind
            );
            assert_eq!(
                suggested_schema["properties"]["arguments"]["properties"]["tool"]["const"],
                target_tool
            );
            crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
                projected_call,
                suggested_schema,
            )
            .unwrap_or_else(|error| {
                panic!(
                    "Adaptive projected SuggestedToolCall {source_tool}->{target_tool} must match its projected schema: {error}"
                )
            });
        }
    }

    #[test]
    fn mismatched_generated_follow_up_posture_is_removed_fail_closed() {
        let mut value = json!({
            "type": "result",
            "next": {
                "follow_up_kind": "mechanically_followable",
                "tool": "list_runners",
                "arguments": {"include_projects": false, "summary_only": true}
            }
        });
        let schema = json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "type": {"type": "string"},
                "next": webcodex_tool_contracts::suggested_tool_call_schema(
                    webcodex_core::runtime_contract::GeneratedFollowUpKind::FallbackRecovery,
                    "list_runners",
                    webcodex_tool_contracts::input_schema_for_tool("list_runners"),
                    "recovery-only edge"
                )
            },
            "required": ["type"]
        });

        project_suggested_tool_calls_in_value(&mut value, &schema, &|_| {
            SuggestedToolCallRoute::Direct
        });

        assert!(value.get("next").is_none());
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&value, &schema)
            .expect("removing a mismatched optional generated edge must stay schema-valid");
    }

    #[test]
    fn continuation_follow_ups_use_current_routes_in_both_schema_and_value() {
        use webcodex_core::runtime_contract::GeneratedFollowUpKind::FallbackRecovery;
        for (target, arguments, available) in [
            (
                "wait_for_job_terminal",
                json!({"job_id":"wc_job_exact", "idempotency_key":"wait-once"}),
                true,
            ),
            (
                "wait_for_agent_events",
                json!({
                    "agent_id":"wc_dagent_qqqqqqqqqqqqqqqq", "endpoint_id":"wc_endpoint_qqqqqqqqqqqqqqqq",
                    "expected_controller_generation":1, "events":[{"kind":"agent_task_terminal", "task_id":"wc_agent_task_qqqqqqqqqqqqqqqq"}],
                    "idempotency_key":"wait-once"
                }),
                true,
            ),
            (
                "present_job_terminal_continuation",
                json!({"wait_id":"wc_job_wait_qqqqqqqqqqqqqqqq"}),
                false,
            ),
            (
                "present_agent_continuation",
                json!({
                    "agent_id":"wc_dagent_qqqqqqqqqqqqqqqq", "endpoint_id":"wc_endpoint_qqqqqqqqqqqqqqqq", "expected_controller_generation":1
                }),
                false,
            ),
        ] {
            let input = webcodex_tool_contracts::input_schema_for_tool(target);
            crate::tool_runtime::ToolCall::from_tool_name(target, arguments.clone()).unwrap();
            let mut schema = json!({
                "type":"object", "additionalProperties":false,
                "properties":{"next":webcodex_tool_contracts::suggested_tool_call_schema(FallbackRecovery, target, input, "continuation edge")},
                "required":["next"]
            });
            let mut value = json!({"next":{"follow_up_kind":"fallback_recovery", "tool":target, "arguments":arguments}});
            project_suggested_tool_calls_in_value(&mut value, &schema, &|name| {
                suggested_tool_call_route(name, false)
            });
            project_suggested_tool_call_schema(&mut schema, &|name| {
                suggested_tool_call_route(name, false)
            });
            if available {
                assert_eq!(value["next"]["tool"], ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME);
                assert_eq!(value["next"]["arguments"]["tool"], target);
                assert_eq!(value["next"]["arguments"]["arguments"], arguments);
                assert_eq!(
                    schema["properties"]["next"]["properties"]["tool"]["const"],
                    ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME
                );
            } else {
                assert!(value.get("next").is_none(), "{target}");
                assert!(schema["properties"].get("next").is_none(), "{target}");
                assert_eq!(schema["required"], json!([]));
            }
            crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&value, &schema)
                .unwrap();
        }
    }

    #[test]
    fn unavailable_suggested_call_is_removed_from_value_and_schema() {
        let mut value = json!({
            "type": "result",
            "next": {
                "follow_up_kind": "mechanically_followable",
                "tool": "not_available_here",
                "arguments": {"x": 1}
            }
        });
        let mut schema = json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "type": {"type": "string"},
                "next": webcodex_tool_contracts::suggested_tool_call_schema(
                    webcodex_core::runtime_contract::GeneratedFollowUpKind::MechanicallyFollowable,
                    "not_available_here",
                    json!({
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {"x": {"type": "integer"}},
                        "required": ["x"]
                    }),
                    "unavailable edge"
                )
            },
            "required": ["type", "next"]
        });
        let canonical_schema = schema.clone();
        project_suggested_tool_calls_in_value(&mut value, &canonical_schema, &|_| {
            SuggestedToolCallRoute::Unavailable
        });
        project_suggested_tool_call_schema(&mut schema, &|_| SuggestedToolCallRoute::Unavailable);
        assert!(value.get("next").is_none());
        assert!(schema["properties"].get("next").is_none());
        assert_eq!(schema["required"], json!(["type"]));
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&value, &schema)
            .unwrap_or_else(|error| {
                panic!("unavailable edge removal must stay schema-valid: {error}")
            });
    }

    #[test]
    fn finish_coding_task_nested_show_changes_recovery_projects_with_route() {
        let canonical_schema =
            webcodex_tool_contracts::output_schema_for_tool("finish_coding_task");
        let canonical_call = json!({
            "follow_up_kind": "mechanically_followable",
            "tool": "read_git_diff_hunks",
            "arguments": {
                "project": "demo",
                "cached": false,
                "paths": [],
                "max_hunks": 30,
                "max_hunk_lines": 400,
                "max_page_bytes": webcodex_core::runtime_contract::DEFAULT_GIT_DIFF_HUNKS_PAGE_BYTES
            }
        });
        let canonical_value = json!({
            "success": true,
            "output": {
                "changes": {
                    "show_changes": {
                        "diff_review_handoff": {"next_call": canonical_call.clone()}
                    },
                    "hunks_truncated": true
                }
            }
        });

        let mut current_adaptive = canonical_value.clone();
        project_suggested_tool_calls_in_value(
            &mut current_adaptive,
            &canonical_schema,
            &|target| suggested_tool_call_route(target, false),
        );
        let current_call = &current_adaptive["output"]["changes"]["show_changes"]
            ["diff_review_handoff"]["next_call"];
        assert_eq!(current_call["tool"], ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME);
        assert_eq!(current_call["follow_up_kind"], "mechanically_followable");
        assert_eq!(current_call["arguments"]["tool"], "read_git_diff_hunks");
        assert_eq!(
            current_call["arguments"]["arguments"], canonical_call["arguments"],
            "current Adaptive routing must gateway-wrap the exact specialist recovery call"
        );
        let synthetic_gateway_route = |target: &str| {
            if target == "read_git_diff_hunks" {
                SuggestedToolCallRoute::Gateway(ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME)
            } else {
                SuggestedToolCallRoute::Direct
            }
        };
        let mut projected_value = canonical_value;
        project_suggested_tool_calls_in_value(
            &mut projected_value,
            &canonical_schema,
            &synthetic_gateway_route,
        );
        let projected_call = &projected_value["output"]["changes"]["show_changes"]
            ["diff_review_handoff"]["next_call"];
        assert_eq!(projected_call["tool"], ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME);
        assert_eq!(projected_call["arguments"]["tool"], "read_git_diff_hunks");
        assert_eq!(
            projected_call["arguments"]["arguments"],
            canonical_call["arguments"]
        );

        let mut projected_schema = canonical_schema;
        project_suggested_tool_call_schema(&mut projected_schema, &synthetic_gateway_route);
        let projected_call_schema = &projected_schema["properties"]["output"]["properties"]
            ["changes"]["properties"]["show_changes"]["properties"]["diff_review_handoff"]
            ["properties"]["next_call"];
        assert_eq!(
            projected_call_schema["properties"]["tool"]["const"],
            ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME
        );
        assert_eq!(
            projected_call_schema["properties"]["arguments"]["properties"]["tool"]["const"],
            "read_git_diff_hunks"
        );
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
            &projected_value,
            &projected_schema,
        )
        .unwrap_or_else(|error| {
            panic!("projected finish_coding_task nested recovery must match schema: {error}")
        });
    }

    #[test]
    fn compact_schema_policy_defaults_true_and_respects_override() {
        assert!(effective_mcp_compact_schemas(None));
        assert!(effective_mcp_compact_schemas(Some(true)));
        assert!(!effective_mcp_compact_schemas(Some(false)));
    }
}
