use salvo::prelude::*;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

use crate::model_surface::{
    gpt_action_gateway_target_route, AdaptiveRuntimeGatewayTargetRoute,
    ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME,
};
use webcodex_tool_contracts::{
    gpt_action_direct_tool_definitions, model_visible_tool_definitions, registered_tool_specs,
    ToolApprovalPolicy, ToolDefinition, ToolSpec, GPT_ACTION_DESCRIPTION_MAX_CHARS,
};

const GPT_ACTION_OPERATION_LIMIT: usize = 30;
#[cfg(test)]
const GPT_ACTION_OPENAPI_IMPORT_BUDGET_BYTES: usize = 800_000;
const GPT_ACTION_PATH_PREFIX: &str = "/api/actions/";

pub(crate) fn public_url() -> String {
    std::env::var("WEBCODEX_PUBLIC_URL")
        .ok()
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "http://localhost:8080".to_string())
}

#[handler]
pub async fn openapi_json(res: &mut Response) {
    res.render(Json(build_openapi_spec()));
}

pub(crate) fn build_openapi_spec() -> Value {
    let specs = registered_tool_specs()
        .into_iter()
        .map(|spec| (spec.name.clone(), spec))
        .collect::<BTreeMap<_, _>>();
    let direct = gpt_action_direct_tool_definitions();
    let operation_count = direct.len() + 1;
    assert!(
        operation_count < GPT_ACTION_OPERATION_LIMIT,
        "GPT Actions operation budget exceeded: {operation_count} >= {GPT_ACTION_OPERATION_LIMIT}; explicitly move a canonical Adaptive Direct tool behind the gateway or mark a protocol-incompatible tool unsupported"
    );

    let mut paths = Map::new();
    for definition in direct {
        let spec = specs.get(definition.name).unwrap_or_else(|| {
            panic!(
                "{} GPT Action direct tool is missing canonical ToolSpec",
                definition.name
            )
        });
        paths.insert(
            format!("{GPT_ACTION_PATH_PREFIX}{}", definition.name),
            json!({"post": direct_operation(definition, spec)}),
        );
    }
    paths.insert(
        format!("{GPT_ACTION_PATH_PREFIX}{ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME}"),
        json!({"post": gateway_operation()}),
    );

    let spec = json!({
        "openapi": "3.1.0",
        "info": {
            "title": "WebCodex GPT Actions",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Custom GPT OpenAPI compatibility surface for the canonical WebCodex Adaptive Runtime. Adaptive direct tools are direct operations; supported long-tail tools use call_runtime_tool. MCP remains the primary ChatGPT integration."
        },
        "servers": [{"url": public_url(), "description": "WebCodex Server"}],
        "paths": Value::Object(paths),
        "components": {
            "securitySchemes": {
                "bearerAuth": {
                    "type": "http",
                    "scheme": "bearer",
                    "description": "WebCodex Bearer credential. Authorization, Project authority, permission gates, Runner capability checks, and destructive policy remain enforced by the canonical ToolRuntime kernel."
                }
            }
        },
        "security": [{"bearerAuth": []}]
    });
    assert_all_descriptions_bounded(&spec);
    spec
}

fn direct_operation(definition: &ToolDefinition, spec: &ToolSpec) -> Value {
    let description = action_operation_description(definition, spec);
    let request_schema = action_request_schema(definition.name, spec.input_schema.clone());
    let response_schema = action_tool_result_schema(json!({}));
    json!({
        "operationId": definition.name,
        "description": description,
        "x-openai-isConsequential": action_is_consequential(definition),
        "requestBody": {
            "required": true,
            "content": {"application/json": {"schema": request_schema}}
        },
        "responses": standard_responses(response_schema)
    })
}

fn gateway_operation() -> Value {
    let mut targets = model_visible_tool_definitions()
        .filter(|definition| definition.supports_gpt_actions())
        .filter(|definition| {
            gpt_action_gateway_target_route(definition.name)
                == AdaptiveRuntimeGatewayTargetRoute::Gateway
        })
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    targets.sort_unstable();
    targets.dedup();

    let request_schema = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "tool": {
                "type": "string",
                "enum": targets,
                "description": "Exact supported long-tail runtime tool name. Direct GPT Action tools must use their direct operation instead."
            },
            "arguments": {
                "type": "object",
                "additionalProperties": true,
                "description": "Canonical arguments for the selected runtime tool. Discover the tool contract first when it is not already known."
            }
        },
        "required": ["tool", "arguments"]
    });
    let response_schema = action_tool_result_schema(json!({}));
    json!({
        "operationId": ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME,
        "description": "Call one GPT-Action-supported long-tail tool from the canonical Adaptive Runtime surface. Use direct Action operations for direct tools. This gateway grants no authority and cannot target model-hidden or protocol-unsupported tools.",
        "x-openai-isConsequential": true,
        "requestBody": {
            "required": true,
            "content": {"application/json": {"schema": request_schema}}
        },
        "responses": standard_responses(response_schema)
    })
}

fn action_tool_result_schema(output_schema: Value) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "success": {"type": "boolean"},
            "output": project_schema_descriptions(output_schema),
            "error": {"type": "string"}
        },
        "required": ["success", "output"]
    })
}

fn action_tool_result_failure_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "success": {"type": "boolean", "const": false},
            "output": {},
            "error": {"type": "string"}
        },
        "required": ["success", "output"]
    })
}

fn json_error_response_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "status": {"type": "integer"},
            "error": {"type": "string"}
        },
        "required": ["status", "error"]
    })
}

fn standard_responses(success_schema: Value) -> Value {
    json!({
        "200": {
            "description": "Canonical ToolResult returned by ToolRuntime.",
            "content": {"application/json": {"schema": success_schema}}
        },
        "400": {
            "description": "Invalid Action request or canonical tool failure.",
            "content": {"application/json": {"schema": {
                "oneOf": [action_tool_result_failure_schema(), json_error_response_schema()]
            }}}
        },
        "403": {"description": "Canonical scope, authority, or permission admission denied the request."}
    })
}

fn action_operation_description(definition: &ToolDefinition, spec: &ToolSpec) -> String {
    let model = definition
        .model_spec
        .expect("GPT Action tool must have canonical model spec");
    let description = model
        .gpt_action_description
        .unwrap_or(spec.description.as_str());
    let chars = description.chars().count();
    assert!(
        chars <= GPT_ACTION_DESCRIPTION_MAX_CHARS,
        "{} GPT Action operation description is {chars} chars; add an explicit with_gpt_action_description override instead of truncating canonical model copy",
        definition.name
    );
    description.to_string()
}

fn action_is_consequential(definition: &ToolDefinition) -> bool {
    !matches!(definition.approval_policy(), ToolApprovalPolicy::None)
}

fn action_request_schema(tool_name: &str, mut schema: Value) -> Value {
    if tool_name == "import_conversation_files_to_project" {
        project_gpt_action_file_params(&mut schema);
    }
    if tool_name == "project_artifact" {
        project_gpt_action_artifact_read_modes(&mut schema);
    }
    project_schema_descriptions(schema)
}

/// GPT Actions and MCP receive host-file references in different host-owned
/// wire shapes, and GPT Actions cannot carry MCP-native image or ResourceLink
/// delivery. These are presentation overlays only; canonical ToolRuntime input
/// validation and authority remain unchanged.
fn project_gpt_action_artifact_read_modes(schema: &mut Value) {
    if let Some(actions) = schema
        .pointer_mut("/properties/action/enum")
        .and_then(Value::as_array_mut)
    {
        actions.retain(|value| matches!(value.as_str(), Some("metadata" | "inspect")));
    }
    if let Some(variants) = schema.get_mut("oneOf").and_then(Value::as_array_mut) {
        variants.retain(|variant| {
            matches!(
                variant
                    .pointer("/properties/action/const")
                    .and_then(Value::as_str),
                Some("metadata" | "inspect")
            )
        });
    }
}

fn project_gpt_action_file_params(schema: &mut Value) {
    let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) else {
        return;
    };
    properties.insert(
        "openaiFileIdRefs".to_string(),
        json!({
            "type": "array",
            "minItems": 1,
            "maxItems": 10,
            "description": "ChatGPT Action host-populated conversation attachment references. Do not construct file ids or temporary download links manually.",
            "items": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "name": {"type": "string", "description": "Host-supplied attachment filename when available."},
                    "id": {"type": "string", "description": "Host-supplied file id when available."},
                    "mime_type": {"type": "string", "description": "Host-supplied MIME type when available."},
                    "download_link": {"type": "string", "description": "Temporary OpenAI-hosted download URL supplied by ChatGPT Actions."}
                },
                "required": ["download_link"]
            }
        }),
    );
}

fn project_schema_descriptions(mut value: Value) -> Value {
    project_schema_descriptions_in_place(&mut value);
    value
}

fn project_schema_descriptions_in_place(value: &mut Value) {
    match value {
        Value::Object(object) => {
            if let Some(Value::String(description)) = object.get_mut("description") {
                if description.chars().count() > GPT_ACTION_DESCRIPTION_MAX_CHARS {
                    *description = bound_schema_description(description);
                }
            }
            for nested in object.values_mut() {
                project_schema_descriptions_in_place(nested);
            }
        }
        Value::Array(items) => {
            for item in items {
                project_schema_descriptions_in_place(item);
            }
        }
        _ => {}
    }
}

fn bound_schema_description(description: &str) -> String {
    if description.chars().count() <= GPT_ACTION_DESCRIPTION_MAX_CHARS {
        return description.to_string();
    }
    let hard_prefix: String = description
        .chars()
        .take(GPT_ACTION_DESCRIPTION_MAX_CHARS - 3)
        .collect();
    let sentence_end = hard_prefix
        .rfind(". ")
        .map(|index| index + 1)
        .filter(|index| *index >= 80);
    let boundary = sentence_end.or_else(|| hard_prefix.rfind(char::is_whitespace));
    let base = boundary
        .map(|index| hard_prefix[..index].trim_end())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| hard_prefix.trim_end());
    format!("{base}...")
}

fn assert_all_descriptions_bounded(value: &Value) {
    fn visit(value: &Value, path: &str) {
        match value {
            Value::Object(object) => {
                if let Some(description) = object.get("description").and_then(Value::as_str) {
                    let chars = description.chars().count();
                    assert!(
                        chars <= GPT_ACTION_DESCRIPTION_MAX_CHARS,
                        "GPT Action OpenAPI description at {path} is {chars} chars"
                    );
                }
                for (key, nested) in object {
                    visit(nested, &format!("{path}/{key}"));
                }
            }
            Value::Array(items) => {
                for (index, nested) in items.iter().enumerate() {
                    visit(nested, &format!("{path}/{index}"));
                }
            }
            _ => {}
        }
    }
    visit(value, "$");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn operation_ids(spec: &Value) -> BTreeSet<String> {
        spec["paths"]
            .as_object()
            .unwrap()
            .values()
            .filter_map(|path| path["post"]["operationId"].as_str())
            .map(str::to_string)
            .collect()
    }

    fn strip_descriptions(value: &mut Value) {
        match value {
            Value::Object(object) => {
                object.remove("description");
                for nested in object.values_mut() {
                    strip_descriptions(nested);
                }
            }
            Value::Array(items) => {
                for item in items {
                    strip_descriptions(item);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn gpt_action_direct_surface_follows_adaptive_direct_with_definition_owned_exceptions() {
        let adaptive = webcodex_tool_contracts::adaptive_runtime_direct_tool_definitions();
        let expected = adaptive
            .iter()
            .copied()
            .filter(|definition| {
                definition.supports_gpt_actions()
                    && definition.gpt_action_exposure()
                        != webcodex_tool_contracts::ToolGptActionExposure::GatewayOnly
            })
            .map(|definition| definition.name)
            .collect::<Vec<_>>();
        let actual = gpt_action_direct_tool_definitions()
            .into_iter()
            .map(|definition| definition.name)
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
        let ranks = actual
            .iter()
            .map(|name| webcodex_tool_contracts::runtime_tool_adaptive_direct_rank(name).unwrap())
            .collect::<Vec<_>>();
        assert!(ranks.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(actual.contains(&"apply_text_edits"));
        assert!(!actual.contains(&"apply_patch"));
        #[cfg(feature = "experimental-code-mode")]
        for name in [
            "code_mode_exec",
            "code_mode_exec_effectful",
            "code_mode_exec_mutating",
        ] {
            assert!(webcodex_tool_contracts::gpt_action_tool_supported(name));
            assert!(!actual.contains(&name));
        }
    }

    #[test]
    fn stop_job_remains_gateway_only_without_operation_growth() {
        let definition = webcodex_tool_contracts::lookup_tool_definition("stop_job").unwrap();
        assert!(definition.adaptive_runtime_direct_rank().is_none());
        assert_eq!(
            definition.gpt_action_exposure(),
            webcodex_tool_contracts::ToolGptActionExposure::GatewayOnly
        );
        assert!(webcodex_tool_contracts::gpt_action_tool_supported(
            "stop_job"
        ));
        let gateway = gateway_operation();
        let targets = gateway["requestBody"]["content"]["application/json"]["schema"]["properties"]
            ["tool"]["enum"]
            .as_array()
            .unwrap();
        assert!(
            targets.contains(&json!("stop_job")),
            "GatewayOnly tools must be parser-ready gateway targets"
        );
        let ids = operation_ids(&build_openapi_spec());
        assert!(!ids.contains("stop_job"));
        assert!(!ids.contains("cancel_job"));
        assert!(ids.contains(ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME));
        assert!(ids.len() < GPT_ACTION_OPERATION_LIMIT);
        assert_eq!(ids.len(), gpt_action_direct_tool_definitions().len() + 1);
    }

    #[test]
    fn generic_openapi_is_canonical_adaptive_actions_surface() {
        let spec = build_openapi_spec();
        let ids = operation_ids(&spec);
        let expected_direct = gpt_action_direct_tool_definitions()
            .into_iter()
            .map(|definition| definition.name.to_string())
            .collect::<BTreeSet<_>>();
        let direct_ids = ids
            .iter()
            .filter(|name| name.as_str() != ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME)
            .cloned()
            .collect::<BTreeSet<_>>();
        assert_eq!(direct_ids, expected_direct);
        assert!(ids.contains(ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME));
        assert!(ids.len() < GPT_ACTION_OPERATION_LIMIT);
        assert!(ids.iter().all(|name| !name.chars().any(char::is_uppercase)));
        for legacy in ["listRuntimeTools", "getRuntimeStatus", "callRuntimeTool"] {
            assert!(!ids.contains(legacy));
        }
        for path in spec["paths"].as_object().unwrap().keys() {
            assert!(path.starts_with(GPT_ACTION_PATH_PREFIX));
        }
        assert!(spec.get("components").unwrap().get("schemas").is_none());
        assert_all_descriptions_bounded(&spec);
    }

    #[test]
    fn protocol_only_tools_are_absent_from_direct_and_gateway() {
        let spec = build_openapi_spec();
        let serialized = serde_json::to_string(&spec).unwrap();
        for tool in [
            "present_goal_plan",
            "present_agent_continuation",
            "rotate_agent_continuation_endpoint",
            "present_work_result",
        ] {
            assert!(!webcodex_tool_contracts::gpt_action_tool_supported(tool));
            assert!(!serialized.contains(&format!("\"{tool}\"")));
        }
    }

    #[test]
    fn direct_request_schemas_are_canonical_except_description_and_host_file_overlay() {
        let generated = build_openapi_spec();
        let specs = registered_tool_specs()
            .into_iter()
            .map(|spec| (spec.name.clone(), spec))
            .collect::<BTreeMap<_, _>>();
        for definition in gpt_action_direct_tool_definitions() {
            if matches!(
                definition.name,
                "import_conversation_files_to_project" | "project_artifact"
            ) {
                continue;
            }
            let mut canonical = specs[definition.name].input_schema.clone();
            let mut action = generated["paths"]
                [format!("{GPT_ACTION_PATH_PREFIX}{}", definition.name)]["post"]["requestBody"]
                ["content"]["application/json"]["schema"]
                .clone();
            strip_descriptions(&mut canonical);
            strip_descriptions(&mut action);
            assert_eq!(action, canonical, "{}", definition.name);
        }
    }

    #[test]
    fn project_artifact_gpt_action_schema_excludes_mcp_only_delivery_modes() {
        let generated = build_openapi_spec();
        let schema = &generated["paths"][format!("{GPT_ACTION_PATH_PREFIX}project_artifact")]
            ["post"]["requestBody"]["content"]["application/json"]["schema"];
        assert_eq!(
            schema["properties"]["action"]["enum"],
            json!(["metadata", "inspect"])
        );
        assert!(schema.get("allOf").is_none());
        let serialized = serde_json::to_string(schema).unwrap();
        assert!(!serialized.contains("\"image\""));
        assert!(!serialized.contains("\"export\""));
    }

    #[test]
    fn direct_response_schemas_use_compact_tool_result_envelope() {
        let generated = build_openapi_spec();
        for definition in gpt_action_direct_tool_definitions() {
            let schema = &generated["paths"]
                [format!("{GPT_ACTION_PATH_PREFIX}{}", definition.name)]["post"]["responses"]
                ["200"]["content"]["application/json"]["schema"];
            assert_eq!(schema["type"], "object", "{}", definition.name);
            assert_eq!(schema["additionalProperties"], false, "{}", definition.name);
            assert_eq!(
                schema["required"],
                json!(["success", "output"]),
                "{}",
                definition.name
            );
            assert_eq!(
                schema["properties"]["success"]["type"], "boolean",
                "{}",
                definition.name
            );
            assert_eq!(
                schema["properties"]["output"],
                json!({}),
                "{} Action response output stays intentionally generic so large canonical output schemas do not inflate the host OpenAPI document",
                definition.name
            );
            assert_eq!(
                schema["properties"]["error"]["type"], "string",
                "{}",
                definition.name
            );
        }
    }

    #[test]
    fn gateway_response_schema_uses_the_same_tool_result_envelope() {
        let generated = build_openapi_spec();
        let schema = &generated["paths"]
            [format!("{GPT_ACTION_PATH_PREFIX}{ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME}")]["post"]
            ["responses"]["200"]["content"]["application/json"]["schema"];
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["required"], json!(["success", "output"]));
        assert_eq!(schema["properties"]["success"]["type"], "boolean");
        assert_eq!(schema["properties"]["output"], json!({}));
        assert_eq!(schema["properties"]["error"]["type"], "string");
    }

    #[test]
    fn action_response_schemas_match_tool_result_serde_shape() {
        let generated = build_openapi_spec();
        let direct_schema = &generated["paths"][format!("{GPT_ACTION_PATH_PREFIX}runtime_status")]
            ["post"]["responses"]["200"]["content"]["application/json"]["schema"];
        let success = serde_json::to_value(crate::tool_runtime::ToolResult::ok(json!({
            "status": "ok"
        })))
        .unwrap();
        assert!(success.get("success").is_some());
        assert!(success.get("output").is_some());
        assert!(success.get("error").is_none());
        assert_eq!(direct_schema["required"], json!(["success", "output"]));
        assert!(!direct_schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|field| field == "error"));

        let failure = serde_json::to_value(crate::tool_runtime::ToolResult::err("failed")).unwrap();
        assert_eq!(failure["success"], false);
        assert!(failure.get("output").is_some());
        assert_eq!(failure["error"], "failed");
        let failure_variants = generated["paths"]
            [format!("{GPT_ACTION_PATH_PREFIX}runtime_status")]["post"]["responses"]["400"]
            ["content"]["application/json"]["schema"]["oneOf"]
            .as_array()
            .unwrap();
        assert_eq!(failure_variants.len(), 2);
        assert_eq!(
            failure_variants[0]["required"],
            json!(["success", "output"])
        );
        assert_eq!(failure_variants[0]["properties"]["error"]["type"], "string");
        assert_eq!(failure_variants[1]["required"], json!(["status", "error"]));
    }

    #[test]
    fn generic_openapi_stays_below_import_size_budget() {
        let spec = build_openapi_spec();
        let compact = serde_json::to_vec(&spec).unwrap();
        let pretty = serde_json::to_vec_pretty(&spec).unwrap();
        for (format, bytes) in [("compact", compact.len()), ("pretty", pretty.len())] {
            assert!(
                bytes < GPT_ACTION_OPENAPI_IMPORT_BUDGET_BYTES,
                "GPT Action OpenAPI {format} JSON is {bytes} bytes; keep it below the internal {}-byte budget so the host has headroom under its 1 MB importer limit",
                GPT_ACTION_OPENAPI_IMPORT_BUDGET_BYTES
            );
        }
    }

    #[test]
    fn long_canonical_descriptions_require_explicit_action_copy() {
        let missing = gpt_action_direct_tool_definitions()
            .into_iter()
            .filter_map(|definition| {
                let model = definition.model_spec.unwrap();
                (model.description.chars().count() > GPT_ACTION_DESCRIPTION_MAX_CHARS
                    && model.gpt_action_description.is_none())
                .then_some((definition.name, model.description.chars().count()))
            })
            .collect::<Vec<_>>();
        assert!(
            missing.is_empty(),
            "long canonical GPT Action descriptions need explicit presentation copy: {missing:?}"
        );
    }

    #[test]
    fn action_operation_descriptions_inherit_when_canonical_copy_already_fits() {
        let specs = registered_tool_specs()
            .into_iter()
            .map(|spec| (spec.name.clone(), spec))
            .collect::<BTreeMap<_, _>>();
        for definition in gpt_action_direct_tool_definitions() {
            let model = definition.model_spec.unwrap();
            if model.gpt_action_description.is_none()
                && model.description.chars().count() <= GPT_ACTION_DESCRIPTION_MAX_CHARS
            {
                assert_eq!(
                    action_operation_description(definition, &specs[definition.name]),
                    model.description
                );
            }
        }
    }

    #[test]
    fn gateway_schema_is_exact_tool_arguments_without_flattened_business_fields() {
        let spec = build_openapi_spec();
        let schema = &spec["paths"]
            [format!("{GPT_ACTION_PATH_PREFIX}{ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME}")]["post"]
            ["requestBody"]["content"]["application/json"]["schema"];
        assert_eq!(schema["required"], json!(["tool", "arguments"]));
        assert_eq!(schema["additionalProperties"], false);
        let properties = schema["properties"].as_object().unwrap();
        assert_eq!(properties.len(), 2);
        assert!(properties.contains_key("tool"));
        assert!(properties.contains_key("arguments"));
        for retired in [
            "params", "project", "path", "query", "line", "column", "changes",
        ] {
            assert!(!properties.contains_key(retired));
        }
    }

    #[test]
    fn schema_description_projection_changes_only_descriptions() {
        let canonical = json!({
            "type": "object",
            "required": ["x"],
            "additionalProperties": false,
            "properties": {
                "x": {"type": "string", "minLength": 2, "description": "sentence. ".repeat(80)}
            }
        });
        let mut projected = project_schema_descriptions(canonical.clone());
        assert!(
            projected["properties"]["x"]["description"]
                .as_str()
                .unwrap()
                .chars()
                .count()
                <= GPT_ACTION_DESCRIPTION_MAX_CHARS
        );
        let mut stripped_canonical = canonical;
        strip_descriptions(&mut stripped_canonical);
        strip_descriptions(&mut projected);
        assert_eq!(projected, stripped_canonical);
    }
}
