use super::*;

#[tokio::test]
async fn discovery_materializes_only_the_requested_contract_shape() {
    use crate::tool_runtime::kernel::ToolProtocolCapabilities;
    use webcodex_tool_contracts::take_tool_materialization_counts_for_test as take_counts;
    let runtime = test_runtime();
    take_counts();
    let exact = runtime
        .tool_manifest(
            None,
            Some("run_process".into()),
            None,
            None,
            false,
            false,
            None,
            None,
            ToolProtocolCapabilities::default(),
        )
        .await;
    assert!(exact.success, "{:?}", exact.error);
    assert_eq!(
        take_counts(),
        (1, 0),
        "exact discovery projects one input and no outputs"
    );
    assert_eq!(
        exact.output["contract"]["input_schema"],
        webcodex_tool_contracts::input_schema_for_tool("run_process")
    );
    let category = runtime
        .tool_manifest(
            None,
            None,
            Some("execution".into()),
            None,
            false,
            true,
            None,
            None,
            ToolProtocolCapabilities::default(),
        )
        .await;
    assert!(category.success);
    assert!(category.output["count"].as_u64().unwrap() > 0);
    assert_eq!(take_counts(), (0, 0));
    let mut projected = category;
    crate::tool_runtime::surface::sparsify_tool_manifest_model_result(&mut projected);
    assert_eq!(
        take_counts(),
        (0, 0),
        "description lookup must not rebuild full specs"
    );
    let hidden = runtime
        .tool_manifest(
            None,
            Some("read_memory".into()),
            None,
            None,
            false,
            false,
            None,
            None,
            ToolProtocolCapabilities::default(),
        )
        .await;
    assert!(
        !hidden.success,
        "metadata must not grant extension admission"
    );
    assert_eq!(take_counts(), (0, 0));
    let summary = runtime.list_tools_payload(crate::tool_runtime::tool_inputs::ListToolsOptions {
        category: None,
        features: None,
        limit: Some(1),
        summary_only: true,
    });
    assert_eq!(summary["count"], 1);
    assert_eq!(take_counts(), (0, 0));
    let full = runtime.list_tools_payload(crate::tool_runtime::tool_inputs::ListToolsOptions {
        category: None,
        features: None,
        limit: Some(1),
        summary_only: false,
    });
    assert_eq!(full["count"], 1);
    assert_eq!(
        take_counts(),
        (1, 1),
        "full list must select before materializing"
    );
}

#[cfg(feature = "experimental-code-mode")]
use std::collections::HashMap;
#[cfg(feature = "experimental-code-mode")]
use std::sync::Arc;
#[cfg(feature = "experimental-code-mode")]
use webcodex_code_mode::{
    CodeModeExecuteRequest, CodeModeHost, CodeModeHostError, CodeModeHostFuture,
    CodeModeToolRequest, CodeModeToolResponse,
};

#[tokio::test]
async fn stop_job_manifest_remains_one_gateway_canonical_mutation() {
    let mut result = test_runtime()
        .dispatch(ToolCall::ToolManifest {
            query: None,
            limit: None,
            tool_name: Some("stop_job".into()),
            category: None,
            intent: None,
            include_recommended_flows: false,
            include_risk_summary: false,
        })
        .await;
    assert!(result.success, "{:?}", result.error);
    crate::tool_runtime::surface::sparsify_tool_manifest_model_result(&mut result);
    assert_eq!(result.output["name"], "stop_job");
    assert_eq!(result.output["route"]["primary"]["mode"], "gateway");
    assert_eq!(
        result.output["route"]["primary"]["tool"],
        "call_runtime_tool"
    );
    assert_eq!(result.output["route"]["primary"]["target"], "stop_job");
    assert!(result.output["route"]["fallback"].is_null());
    assert_eq!(result.output["effect"], "mutate");
    assert_eq!(result.output["idempotency"], "desired_state");
    assert_eq!(
        result.output["input_schema"],
        webcodex_tool_contracts::input_schema_for_tool("stop_job")
    );
}

#[cfg(feature = "experimental-code-mode")]
#[tokio::test]
async fn code_mode_job_tools_stay_outside_all_typed_surfaces_and_host_admission() {
    use crate::tool_runtime::code_mode::{code_mode_orchestration_policy, CodeModeCallableStage};
    use crate::tool_runtime::kernel::ToolTransport;
    use crate::tool_runtime::orchestration_host::CanonicalOrchestrationHost;
    let runtime = runtime_with_agent_project("job-admission-fixture");
    register_agent(&runtime, "job-admission-fixture", None, Default::default()).await;
    let project = agent_test_project_id("job-admission-fixture");
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let auth = bootstrap_auth_context();
    for stage in [
        CodeModeCallableStage::ReadOnly,
        CodeModeCallableStage::Validation,
        CodeModeCallableStage::GuardedEdit,
    ] {
        let policy = code_mode_orchestration_policy(stage);
        let result = runtime
            .dispatch(ToolCall::ToolManifest {
                query: None,
                limit: None,
                tool_name: Some(stage.entry_tool().into()),
                category: None,
                intent: None,
                include_recommended_flows: false,
                include_risk_summary: false,
            })
            .await;
        assert!(result.success, "{:?}", result.error);
        let names = result.output["code_mode_callable_contract"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["tool"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(names, policy.admitted_tools);
        let host = CanonicalOrchestrationHost::new(
            runtime.clone(),
            Some(&auth),
            project.clone(),
            session.session_id.clone(),
            ToolTransport::Mcp,
            None,
            policy,
        );
        for (index, name) in [
            "observe_jobs",
            "list_jobs",
            "wait_for_job_terminal",
            "stop_job",
            "present_job_terminal_continuation",
        ]
        .into_iter()
        .enumerate()
        {
            assert!(!names.contains(&name));
            assert!(!policy.is_admitted(name));
            let denied = host
                .invoke_tool(index + 1, name.to_string(), json!({}))
                .await;
            assert_eq!(denied.unwrap_err().failure_kind(), crate::tool_runtime::orchestration_host::OrchestrationHostFailureKind::ToolNotAdmitted, "{} admitted {name}", stage.entry_tool());
        }
        assert!(probe_patch_agent_request(&runtime, "job-admission-fixture")
            .await
            .is_none());
    }
}

#[cfg(feature = "experimental-code-mode")]
struct CallableExampleHost {
    input_schemas: HashMap<String, Value>,
}

#[cfg(feature = "experimental-code-mode")]
impl CodeModeHost for CallableExampleHost {
    fn invoke_tool(
        &self,
        request: CodeModeToolRequest,
    ) -> CodeModeHostFuture<'_, Result<CodeModeToolResponse, CodeModeHostError>> {
        Box::pin(async move {
            let schema = self.input_schemas.get(&request.tool_name).ok_or_else(|| {
                CodeModeHostError::new(format!("unexpected example tool {}", request.tool_name))
            })?;
            crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
                &request.arguments,
                schema,
            )
            .map_err(|error| {
                CodeModeHostError::new(format!(
                    "{} example arguments drifted from callable input: {error}",
                    request.tool_name
                ))
            })?;
            let output = match request.tool_name.as_str() {
                "search_project_texts" => json!({
                    "items": [{
                        "success": true,
                        "output": {
                            "matches": [{
                                "path": "src/tool_runtime/code_mode.rs",
                                "line": 1,
                                "preview": "CanonicalOrchestrationHost",
                                "read_hint": {"start_line": 1, "limit": 80}
                            }]
                        }
                    }]
                }),
                "read_files" => json!({
                    "items": [{
                        "path": "src/tool_runtime/code_mode.rs",
                        "success": true,
                        "output": {
                            "text": "detail",
                            "read_revision": 1,
                            "returned_lines": 1,
                            "has_more": false
                        }
                    }]
                }),
                "get_git_status" => json!({"stdout": "## clean"}),
                "project_validate" => json!({
                    "execution_state": "pending",
                    "continuation": {"follow_up_kind": "fallback_recovery", "tool": "observe_jobs",
                        "arguments": {"items":[{"job_id":"wc_job_example"}]}}
                }),
                // Current sparse success retains changed, not the duplicate
                // state_changed/execution_state presentation fields.
                "edit_project_files" => json!({"changed":true}),
                other => {
                    return Err(CodeModeHostError::new(format!(
                        "example unexpectedly invoked {other}"
                    )))
                }
            };
            Ok(CodeModeToolResponse {
                success: true,
                output,
                error: None,
            })
        })
    }
}

#[cfg(feature = "experimental-code-mode")]
#[tokio::test]
async fn code_mode_callable_projection_examples_execute_against_projected_inputs() {
    let runtime = test_runtime();
    for entry_tool in [
        "execute_code_mode",
        "execute_effectful_code_mode",
        "execute_mutating_code_mode",
    ] {
        let manifest = runtime
            .dispatch(ToolCall::ToolManifest {
                query: None,
                limit: None,
                tool_name: Some(entry_tool.to_string()),
                category: None,
                intent: None,
                include_recommended_flows: false,
                include_risk_summary: false,
            })
            .await;
        assert!(manifest.success, "{entry_tool}: {:?}", manifest.error);
        let projection = &manifest.output["code_mode_callable_contract"];
        let tools = projection["tools"]
            .as_array()
            .expect("projected callable tools");
        let input_schemas = tools
            .iter()
            .map(|tool| {
                (
                    tool["tool"]
                        .as_str()
                        .expect("projected tool name")
                        .to_string(),
                    tool["input"].clone(),
                )
            })
            .collect::<HashMap<_, _>>();
        let allowed_tools = tools
            .iter()
            .map(|tool| tool["tool"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        for example in projection["examples"]
            .as_array()
            .expect("callable usage examples")
        {
            let source = example["source"]
                .as_str()
                .expect("Code Mode example source")
                .to_string();
            let result = webcodex_code_mode::execute(
                Arc::new(CallableExampleHost {
                    input_schemas: input_schemas.clone(),
                }),
                CodeModeExecuteRequest {
                    source,
                    allowed_tools: allowed_tools.clone(),
                    timeout_ms: Some(2_000),
                },
            )
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "{} example {} failed: {error}",
                    projection["stage"], example["name"]
                )
            });
            assert!(
                !result.content.is_empty(),
                "{}: {}",
                projection["stage"],
                example["name"]
            );
        }
    }
}

#[test]
fn discovery_output_schemas_cover_runtime_payload_keys() {
    use crate::tool_runtime::tool_definition::TOOL_CATEGORY_GIT;

    let runtime = test_runtime();
    let specs = registered_tool_specs();

    let list_tools_spec = spec_named(&specs, "list_tools");
    let list_tools_payload = runtime.list_tools_payload(ListToolsOptions {
        category: Some(TOOL_CATEGORY_GIT.to_string()),
        features: Some("read".to_string()),
        summary_only: true,
        limit: Some(3),
    });
    assert_payload_keys_declared(
        "list_tools",
        &list_tools_payload,
        output_schema_properties(list_tools_spec),
    );

    let tool_manifest_spec = spec_named(&specs, "read_tool_manifest");
    let tool_manifest_payload = runtime
        .compact_tool_manifest_payload_bounded(
            Some(vec![TOOL_CATEGORY_GIT.to_string()]),
            None,
            Some(2),
        )
        .unwrap();
    assert_payload_keys_declared(
        "read_tool_manifest",
        &tool_manifest_payload,
        output_schema_properties(tool_manifest_spec),
    );
}

#[test]
fn tool_manifest_and_list_tools_limit_truncation_reports_limit_reason() {
    use crate::tool_runtime::tool_definition::TOOL_CATEGORY_SESSION;

    let runtime = test_runtime();
    let list_tools = runtime.list_tools_payload(ListToolsOptions {
        category: None,
        features: None,
        summary_only: true,
        limit: Some(2),
    });
    assert_eq!(list_tools["truncated"], true);
    assert_eq!(list_tools["truncation_reason"], "limit");
    assert_eq!(list_tools["limit_applied"], true);
    assert_eq!(list_tools["requested_limit"], 2);
    assert_eq!(list_tools["count"], 2);
    assert_eq!(list_tools["returned_count"], 2);
    assert_eq!(list_tools["filtered_count"], list_tools["total_count"]);
    assert!(list_tools["total_count"].as_u64().unwrap() > 2);
    assert!(!serde_json::to_string(&list_tools)
        .unwrap()
        .contains("ResponseTooLarge"));

    let manifest = runtime
        .compact_tool_manifest_payload_bounded(
            Some(vec![TOOL_CATEGORY_SESSION.to_string()]),
            None,
            Some(2),
        )
        .unwrap();
    assert_eq!(manifest["truncated"], true);
    assert_eq!(manifest["truncation_reason"], "limit");
    assert_eq!(manifest["limit_applied"], true);
    assert_eq!(manifest["requested_limit"], 2);
    assert_eq!(manifest["count"], 2);
    assert_eq!(manifest["returned_count"], 2);
    assert!(manifest["filtered_count"].as_u64().unwrap() > 2);
    assert!(
        manifest["total_count"].as_u64().unwrap() >= manifest["filtered_count"].as_u64().unwrap()
    );
    assert!(!serde_json::to_string(&manifest)
        .unwrap()
        .contains("ResponseTooLarge"));
}

#[test]
fn tool_manifest_sparse_filtered_projection_only_keeps_truncation_metadata_when_needed() {
    let runtime = test_runtime();
    let canonical = runtime
        .compact_tool_manifest_payload_bounded(None, Some("coding".to_string()), Some(3))
        .expect("limited coding manifest");
    let mut result = crate::tool_runtime::ToolResult::ok(canonical);
    crate::tool_runtime::surface::sparsify_tool_manifest_model_result(&mut result);

    assert_eq!(result.output["truncated"], true);
    assert_eq!(result.output["truncation_reason"], "limit");
    assert_eq!(result.output["returned_count"], 3);
    assert!(result.output["filtered_count"].as_u64().unwrap() > 3);
    assert_eq!(result.output["limit"], 3);
    assert!(result.output.get("categories").is_none());
    assert!(result.output.get("tool_count").is_none());
    assert!(result.output.get("count").is_none());
    assert!(result.output.get("total_count").is_none());
}

fn output_schema_properties(spec: &ToolSpec) -> &serde_json::Map<String, Value> {
    spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap_or_else(|| panic!("{} output schema properties", spec.name))
}

fn assert_payload_keys_declared(
    tool_name: &str,
    payload: &Value,
    output_schema_properties: &serde_json::Map<String, Value>,
) {
    let payload = payload
        .as_object()
        .unwrap_or_else(|| panic!("{tool_name} payload object"));
    for key in payload.keys() {
        assert!(
            output_schema_properties.contains_key(key),
            "{tool_name} runtime output key {key} is missing from output_schema properties"
        );
    }
}

fn string_array(value: &Value, context: &str) -> Vec<String> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("{context} must be an array"))
        .iter()
        .map(|member| {
            member
                .as_str()
                .unwrap_or_else(|| panic!("{context} member must be a string: {member:?}"))
                .to_string()
        })
        .collect()
}

fn string_set(value: &Value, context: &str) -> BTreeSet<String> {
    string_array(value, context).into_iter().collect()
}

fn category_member_sets(
    categories: &Value,
    context: &str,
) -> std::collections::BTreeMap<String, BTreeSet<String>> {
    categories
        .as_object()
        .unwrap_or_else(|| panic!("{context} must be an object"))
        .iter()
        .map(|(category, members)| {
            (
                category.clone(),
                string_set(members, &format!("{context}.{category}")),
            )
        })
        .collect()
}

fn definition_category_member_sets() -> std::collections::BTreeMap<String, BTreeSet<String>> {
    use crate::tool_runtime::tool_definition::model_visible_tool_definitions;

    let mut categories = std::collections::BTreeMap::new();
    for definition in model_visible_tool_definitions() {
        categories
            .entry(definition.category.to_string())
            .or_insert_with(BTreeSet::new)
            .insert(definition.name.to_string());
    }
    categories
}

fn tool_entry_names(tools: &Value, context: &str) -> BTreeSet<String> {
    tools
        .as_array()
        .unwrap_or_else(|| panic!("{context} must be an array"))
        .iter()
        .map(|tool| {
            tool["name"]
                .as_str()
                .unwrap_or_else(|| panic!("{context} entry missing name: {tool:?}"))
                .to_string()
        })
        .collect()
}

fn assert_categories_hide_runtime_only_tools(
    categories: &std::collections::BTreeMap<String, BTreeSet<String>>,
    context: &str,
) {
    for forbidden in ["delete_files", "run_codex"] {
        assert!(
            categories
                .values()
                .all(|members| !members.contains(forbidden)),
            "{context} categories must not expose {forbidden}: {categories:?}"
        );
    }
}

fn assert_no_response_too_large(surface: &str, payload: &Value) {
    assert!(
        !serde_json::to_string(payload)
            .unwrap()
            .contains("ResponseTooLarge"),
        "{surface} bounded discovery must not surface ResponseTooLarge: {payload:?}"
    );
}

#[test]
fn canonical_categories_match_list_and_manifest_without_overlapping_groups() {
    let runtime = test_runtime();
    let categories = registered_tool_categories();
    let manifest = runtime.compact_tool_manifest_payload();
    assert_eq!(categories, manifest["categories"]);
    let mut seen = BTreeSet::new();
    for (category, members) in categories.as_object().unwrap() {
        let members = members.as_array().unwrap();
        let names = members
            .iter()
            .map(|name| name.as_str().unwrap())
            .collect::<Vec<_>>();
        assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
        for name in names {
            assert!(seen.insert(name), "duplicate category membership: {name}");
            assert_eq!(
                crate::tool_runtime::tool_definition::runtime_tool_category(name),
                category
            );
        }
        let filtered = runtime.list_tools_payload(ListToolsOptions {
            category: Some(category.clone()),
            features: None,
            summary_only: true,
            limit: None,
        });
        assert_eq!(
            tool_entry_names(&filtered["tools"], category),
            members
                .iter()
                .map(|name| name.as_str().unwrap().to_string())
                .collect(),
            "list_tools category selection must match its advertised category"
        );
    }
    let specs = registered_tool_specs();
    assert_eq!(seen, specs.iter().map(|spec| spec.name.as_str()).collect());
    for obsolete_group in [
        "inspect",
        "shell",
        "jobs",
        "projects",
        "review",
        "file_transfer",
    ] {
        assert!(categories.get(obsolete_group).is_none(), "{obsolete_group}");
    }
    assert!(
        categories.get("memory").is_none(),
        "hidden Memory is not admitted"
    );
}

#[test]
fn tool_manifest_categories_cover_every_model_visible_definition() {
    use crate::tool_runtime::tool_definition::model_visible_tool_definitions;

    let runtime = test_runtime();
    let manifest = runtime.compact_tool_manifest_payload();
    assert_eq!(
        manifest["tool_count"],
        registered_tool_specs().len() as i64,
        "read_tool_manifest tool_count must mirror model-facing ToolSpec count"
    );
    let categories = manifest["categories"]
        .as_object()
        .expect("read_tool_manifest categories");

    for definition in model_visible_tool_definitions() {
        let members = categories
            .get(definition.category)
            .and_then(Value::as_array)
            .unwrap_or_else(|| {
                panic!(
                    "missing read_tool_manifest category {}",
                    definition.category
                )
            });
        assert!(
            members.iter().any(|member| member == definition.name),
            "{} ToolDefinition category {} must include the tool in read_tool_manifest",
            definition.name,
            definition.category
        );
    }
}

#[test]
fn tool_manifest_compact_categories_match_single_tool_definition_category() {
    use crate::tool_runtime::tool_definition::{
        lookup_tool_definition, model_visible_tool_definitions,
    };
    use std::collections::BTreeMap;

    let runtime = test_runtime();
    let manifest = runtime.compact_tool_manifest_payload();
    let categories = manifest["categories"]
        .as_object()
        .expect("read_tool_manifest categories");
    let visible_names = model_visible_tool_definitions()
        .map(|definition| definition.name)
        .collect::<BTreeSet<_>>();
    let mut memberships: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for (category, members) in categories {
        for member in members
            .as_array()
            .unwrap_or_else(|| panic!("{category} members must be an array"))
        {
            let name = member
                .as_str()
                .unwrap_or_else(|| panic!("{category} member must be a string"));
            let definition = lookup_tool_definition(name)
                .unwrap_or_else(|| panic!("{category} member {name} missing ToolDefinition"));
            assert!(
                definition.visibility.is_model_visible(),
                "{category} member {name} must be model-visible"
            );
            assert_eq!(
                definition.category, category,
                "{name} compact manifest category must match ToolDefinition category"
            );
            memberships
                .entry(name.to_string())
                .or_default()
                .push(category.clone());
        }
    }

    assert_eq!(
        memberships.len(),
        visible_names.len(),
        "compact read_tool_manifest categories must cover every model-visible tool exactly once"
    );
    for definition in model_visible_tool_definitions() {
        let member_categories = memberships
            .get(definition.name)
            .unwrap_or_else(|| panic!("{} missing compact manifest category", definition.name));
        assert_eq!(
            member_categories,
            &vec![definition.category.to_string()],
            "{} must have exactly one compact manifest category",
            definition.name
        );
    }
    let tools = manifest["tools"]
        .as_array()
        .expect("read_tool_manifest tools");
    assert_eq!(
        tools.len(),
        visible_names.len(),
        "unfiltered compact read_tool_manifest must list every model-visible tool"
    );
    for tool in tools {
        let name = tool["name"]
            .as_str()
            .expect("read_tool_manifest tool name must be a string");
        let definition = lookup_tool_definition(name)
            .unwrap_or_else(|| panic!("{name} compact manifest entry missing ToolDefinition"));
        assert!(
            visible_names.contains(name),
            "{name} compact manifest entry must be model-visible"
        );
        assert_eq!(
            tool["category"].as_str(),
            Some(definition.category),
            "{name} compact manifest entry category must match ToolDefinition"
        );
    }
}

#[test]
fn compact_tool_manifest_categories_match_bounded_list_tools_categories() {
    let runtime = test_runtime();
    let expected_categories = definition_category_member_sets();
    let expected_count: usize = expected_categories
        .values()
        .map(|members| members.len())
        .sum();

    let manifest = runtime.compact_tool_manifest_payload();
    let manifest_categories = category_member_sets(&manifest["categories"], "read_tool_manifest");
    assert_eq!(
        manifest_categories, expected_categories,
        "compact read_tool_manifest categories must be grouped by ToolDefinition category"
    );
    assert_categories_hide_runtime_only_tools(&manifest_categories, "read_tool_manifest");

    let list_tools = runtime.list_tools_payload(ListToolsOptions {
        category: None,
        features: None,
        summary_only: true,
        limit: None,
    });
    let list_categories = category_member_sets(&list_tools["categories"], "list_tools");
    assert_eq!(
        list_categories, manifest_categories,
        "bounded list_tools categories must match compact read_tool_manifest categories"
    );
    assert_categories_hide_runtime_only_tools(&list_categories, "list_tools");
    assert_eq!(manifest["tool_count"].as_u64(), Some(expected_count as u64));
    assert_eq!(
        manifest["returned_count"].as_u64(),
        Some(expected_count as u64)
    );
    assert_eq!(
        list_tools["total_count"].as_u64(),
        Some(expected_count as u64)
    );
    assert_eq!(
        list_tools["returned_count"].as_u64(),
        Some(expected_count as u64)
    );
    assert_eq!(list_tools["truncated"], false);
}

#[test]
fn tool_manifest_category_filter_matches_tool_definition_categories() {
    let runtime = test_runtime();
    let expected_categories = definition_category_member_sets();
    let all_manifest_categories = category_member_sets(
        &runtime.compact_tool_manifest_payload()["categories"],
        "unfiltered read_tool_manifest",
    );

    for (category, expected_tools) in expected_categories {
        let manifest = runtime
            .compact_tool_manifest_payload_bounded(Some(vec![category.clone()]), None, None)
            .unwrap();
        assert_eq!(manifest["filtered"], true);
        assert_eq!(manifest["category"].as_str(), Some(category.as_str()));
        assert_eq!(
            string_array(&manifest["categories_requested"], "categories_requested"),
            vec![category.clone()]
        );
        assert_eq!(
            manifest["filtered_count"].as_u64(),
            Some(expected_tools.len() as u64)
        );
        assert_eq!(
            manifest["returned_count"].as_u64(),
            Some(expected_tools.len() as u64)
        );
        assert_eq!(
            manifest["count"].as_u64(),
            Some(expected_tools.len() as u64)
        );
        assert_eq!(manifest["truncated"], false);
        assert_eq!(manifest["limit_applied"], false);
        assert!(manifest["total_count"].as_u64().unwrap() >= expected_tools.len() as u64);
        assert_no_response_too_large("read_tool_manifest", &manifest);

        let filtered_categories = category_member_sets(
            &manifest["categories"],
            &format!("read_tool_manifest filtered {category} categories"),
        );
        assert_eq!(
            filtered_categories, all_manifest_categories,
            "filtered compact read_tool_manifest currently preserves the full categories map"
        );
        assert_categories_hide_runtime_only_tools(
            &filtered_categories,
            "filtered read_tool_manifest",
        );

        let returned_tools = tool_entry_names(
            &manifest["tools"],
            &format!("read_tool_manifest filtered {category} tools"),
        );
        assert_eq!(
            returned_tools, expected_tools,
            "read_tool_manifest category filter must return exactly the ToolDefinition category members"
        );
        for tool in manifest["tools"]
            .as_array()
            .expect("read_tool_manifest tools")
        {
            assert_eq!(
                tool["category"].as_str(),
                Some(category.as_str()),
                "filtered read_tool_manifest must not mix categories: {tool:?}"
            );
        }
    }
}

#[test]
fn list_tools_category_filter_matches_tool_definition_categories() {
    let runtime = test_runtime();
    let expected_categories = definition_category_member_sets();
    let all_list_categories = category_member_sets(
        &runtime.list_tools_payload(ListToolsOptions {
            category: None,
            features: None,
            summary_only: true,
            limit: None,
        })["categories"],
        "unfiltered list_tools",
    );

    for (category, expected_tools) in expected_categories {
        let list_tools = runtime.list_tools_payload(ListToolsOptions {
            category: Some(category.clone()),
            features: None,
            summary_only: true,
            limit: None,
        });
        assert_eq!(list_tools["category"].as_str(), Some(category.as_str()));
        assert_eq!(list_tools["features"], Value::Null);
        assert_eq!(
            list_tools["filtered_count"].as_u64(),
            Some(expected_tools.len() as u64)
        );
        assert_eq!(
            list_tools["returned_count"].as_u64(),
            Some(expected_tools.len() as u64)
        );
        assert_eq!(
            list_tools["count"].as_u64(),
            Some(expected_tools.len() as u64)
        );
        assert_eq!(list_tools["truncated"], false);
        assert_eq!(list_tools["limit_applied"], false);
        assert!(list_tools["total_count"].as_u64().unwrap() >= expected_tools.len() as u64);
        assert_no_response_too_large("list_tools", &list_tools);

        let filtered_categories = category_member_sets(
            &list_tools["categories"],
            &format!("list_tools filtered {category} categories"),
        );
        assert_eq!(
            filtered_categories, all_list_categories,
            "filtered list_tools currently preserves the full ToolDefinition category map"
        );
        assert_categories_hide_runtime_only_tools(&filtered_categories, "filtered list_tools");

        let names = string_set(
            &list_tools["names"],
            &format!("list_tools {category} names"),
        );
        assert_eq!(
            names, expected_tools,
            "list_tools category filter names must match ToolDefinition category members"
        );
        let returned_tools = tool_entry_names(
            &list_tools["tools"],
            &format!("list_tools filtered {category} tools"),
        );
        assert_eq!(
            returned_tools, expected_tools,
            "list_tools category filter tools must match ToolDefinition category members"
        );
        for tool in list_tools["tools"].as_array().expect("list_tools tools") {
            assert_eq!(
                tool["category"].as_str(),
                Some(category.as_str()),
                "filtered list_tools must not mix categories: {tool:?}"
            );
        }
    }
}

#[test]
fn tool_manifest_recommended_flows_reference_visible_defined_tools() {
    use crate::tool_runtime::tool_definition::{
        is_model_visible_tool_name, lookup_tool_definition, model_visible_recommended_flows,
    };

    let runtime = test_runtime();
    let manifest = runtime.compact_tool_manifest_payload();
    let manifest_categories = category_member_sets(&manifest["categories"], "read_tool_manifest");
    let flows = manifest["recommended_flows"]
        .as_array()
        .expect("read_tool_manifest recommended_flows");
    assert_eq!(flows.len(), model_visible_recommended_flows().count());
    for inactive in ["agent_continuation_setup", "goal_agent_wait_orchestration"] {
        assert!(!flows.iter().any(|flow| flow["name"] == inactive));
    }

    for (actual, expected) in flows.iter().zip(model_visible_recommended_flows()) {
        assert_eq!(actual["name"], expected.name);
        assert_eq!(actual["purpose"], expected.manifest_purpose);
        let tools = actual["tools"]
            .as_array()
            .unwrap_or_else(|| panic!("{} recommended flow tools", expected.name));
        assert_eq!(tools.len(), expected.tools.len());
        for (actual_tool, expected_tool) in tools.iter().zip(expected.tools) {
            assert_eq!(actual_tool, expected_tool);
            let definition = lookup_tool_definition(expected_tool).unwrap_or_else(|| {
                panic!(
                    "{} recommended flow references unknown tool {expected_tool}",
                    expected.name
                )
            });
            assert!(
                definition.visibility.is_model_visible(),
                "{} recommended flow references hidden tool {expected_tool}",
                expected.name
            );
            assert!(
                is_model_visible_tool_name(expected_tool),
                "{} recommended flow references non-visible tool {expected_tool}",
                expected.name
            );
            assert!(
                manifest_categories
                    .values()
                    .any(|members| members.contains(*expected_tool)),
                "{} recommended flow references {expected_tool}, which is missing from compact manifest categories",
                expected.name
            );
        }
    }
}

#[tokio::test]
async fn tool_manifest_omits_recommended_flows_when_disabled() {
    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::ToolManifest {
            query: None,
            limit: None,
            tool_name: None,
            category: None,
            intent: None,
            include_recommended_flows: false,
            include_risk_summary: true,
        })
        .await;
    assert!(result.success, "{:?}", result.error);
    assert!(
        result.output.get("recommended_flows").is_none(),
        "include_recommended_flows=false currently omits recommended_flows: {:?}",
        result.output
    );
}

#[tokio::test]
async fn tool_manifest_without_intent_keeps_compat_shape_and_lists_available_intents() {
    let runtime = test_runtime();
    let call = ToolCall::from_tool_name("read_tool_manifest", json!({})).unwrap();
    let result = runtime.dispatch(call).await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["schema_version"], 1);
    assert_eq!(result.output["intent"], Value::Null);
    assert_eq!(result.output["filtered"], false);
    assert!(
        result.output["count"].as_u64().unwrap() > 20,
        "unfiltered read_tool_manifest should still return the broad compact tool set"
    );
    let available = string_array(&result.output["available_intents"], "available_intents");
    assert_eq!(
        available,
        vec![
            "maintenance".to_string(),
            "resources".to_string(),
            "coding".to_string(),
            "audit".to_string(),
            "exploration".to_string(),
            "file_transfer".to_string(),
            "release".to_string(),
            "discovery".to_string(),
        ]
    );
    assert_payload_keys_declared(
        "read_tool_manifest",
        &result.output,
        output_schema_properties(spec_named(&registered_tool_specs(), "read_tool_manifest")),
    );
}

#[tokio::test]
async fn tool_manifest_all_available_intents_parse_and_filter_through_tool_call() {
    let runtime = test_runtime();
    for intent in [
        "coding",
        "audit",
        "exploration",
        "file_transfer",
        "release",
        "discovery",
    ] {
        let call = ToolCall::from_tool_name(
            "read_tool_manifest",
            json!({
                "intent": intent,
                "include_recommended_flows": false,
                "include_risk_summary": false,
            }),
        )
        .unwrap_or_else(|error| panic!("{intent} must parse: {error}"));
        let result = runtime.dispatch(call).await;

        assert!(result.success, "{intent}: {:?}", result.error);
        assert_eq!(result.output["intent"], intent);
        assert_eq!(result.output["filtered"], true);
        assert!(
            result.output["returned_count"].as_u64().unwrap()
                < result.output["total_count"].as_u64().unwrap(),
            "{intent} must return a bounded manifest: {:?}",
            result.output
        );
    }
}

#[tokio::test]
async fn tool_manifest_intent_coding_returns_ranked_compact_tools() {
    use crate::tool_runtime::tool_definition::TOOL_MANIFEST_INTENTS;

    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::ToolManifest {
            query: None,
            limit: None,
            tool_name: None,
            category: None,
            intent: Some("coding".to_string()),
            include_recommended_flows: false,
            include_risk_summary: true,
        })
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["intent"], "coding");
    assert_eq!(result.output["filtered"], true);
    assert_eq!(result.output["schema_version"], 1);

    let expected: Vec<&str> = TOOL_MANIFEST_INTENTS
        .iter()
        .find(|intent| intent.name == "coding")
        .unwrap()
        .tools
        .to_vec();
    let names: Vec<&str> = result.output["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names, expected,
        "coding intent tools must be ranked in table order"
    );
    assert_eq!(result.output["count"], expected.len() as u64);
    assert_eq!(result.output["filtered_count"], expected.len() as u64);
    assert!(
        result.output["total_count"].as_u64().unwrap() > expected.len() as u64,
        "total_count should remain the full runtime tool count"
    );
    assert!(result.output.get("recommended_flows").is_none());
    assert!(result.output["risk_summary"].is_object());
    assert_eq!(
        names,
        crate::tool_runtime::tool_definition::CODING_INTENT_TOOL_NAMES,
        "coding manifest must use its independent ordered selection surface"
    );
    for compatibility_or_overlap in [
        "read_file",
        "search_project_text",
        "git_diff",
        "git_diff_summary",
        "job_status",
        "job_log",
        "run_job",
        "apply_unified_diff",
    ] {
        assert!(
            !names.contains(&compatibility_or_overlap),
            "coding intent should not recommend {compatibility_or_overlap}: {names:?}"
        );
    }
    for gateway_specialist in ["cargo_fmt", "check_workspace_hygiene", "finish_coding_task"] {
        let tool = result.output["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == gateway_specialist)
            .unwrap_or_else(|| panic!("missing coding specialist {gateway_specialist}"));
        assert_eq!(tool["availability"], "gateway", "{gateway_specialist}");
        assert_eq!(
            tool["gateway_tool"],
            crate::model_surface::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME,
            "{gateway_specialist}"
        );
    }
    for direct in [
        "work_on_project",
        "search_project_texts",
        "search_file_context",
        "read_files",
        "edit_project_files",
        "run_process",
        "run_script",
        "run_shell",
        "observe_jobs",
        "project_validate",
        "review_changes",
    ] {
        let tool = result.output["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == direct)
            .unwrap_or_else(|| panic!("missing canonical coding tool {direct}"));
        assert_eq!(tool["availability"], "direct", "{direct}");
        assert!(tool["gateway_tool"].is_null(), "{direct}");
    }
    for (advanced, expected_availability) in [
        ("cargo_check", "gateway"),
        ("cargo_test", "gateway"),
        ("go_test", "gateway"),
    ] {
        assert!(
            !names.contains(&advanced),
            "ordinary coding intent must not recommend advanced validator {advanced}"
        );
        let exact = runtime
            .dispatch(ToolCall::ToolManifest {
                query: None,
                limit: None,
                tool_name: Some(advanced.to_string()),
                category: None,
                intent: None,
                include_recommended_flows: false,
                include_risk_summary: false,
            })
            .await;
        assert!(exact.success, "{advanced}: {:?}", exact.error);
        assert_eq!(
            exact.output["contract"]["availability"],
            expected_availability
        );
        if expected_availability == "direct" {
            assert!(
                exact.output["contract"]["gateway_tool"].is_null(),
                "{advanced}"
            );
        } else {
            assert_eq!(
                exact.output["contract"]["gateway_tool"],
                crate::model_surface::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME,
                "{advanced}"
            );
        }
    }
}

#[tokio::test]
async fn tool_manifest_accepts_hyphenated_intent_alias() {
    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::ToolManifest {
            query: None,
            limit: None,
            tool_name: None,
            category: None,
            intent: Some("Discovery".to_string()),
            include_recommended_flows: false,
            include_risk_summary: false,
        })
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["intent"], "discovery");
    let names: Vec<&str> = result.output["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        vec![
            "read_tool_manifest",
            "list_tools",
            "resolve_workspace",
            "open_webcodex_workbench",
            "search_webcodex_resources",
            "get_runtime_status",
            "list_runners",
            "list_projects",
            "read_project_overview",
        ]
    );
}

#[tokio::test]
async fn tool_manifest_unknown_intent_returns_structured_error() {
    let runtime = test_runtime();
    let call =
        ToolCall::from_tool_name("read_tool_manifest", json!({"intent": "not_a_real_intent"}))
            .unwrap();
    let result = runtime.dispatch(call).await;
    assert!(!result.success, "unknown intent must fail");
    assert!(
        result
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("unknown read_tool_manifest intent"),
        "{:?}",
        result.error
    );
    assert_eq!(result.output["code"], "unknown_tool_manifest_intent");
    assert_eq!(result.output["intent"], "not_a_real_intent");
    let available = string_array(&result.output["available_intents"], "available_intents");
    assert!(available.contains(&"coding".to_string()));
    assert!(
        result.output.get("tools").is_none(),
        "unknown intent must not return a silent empty tool list: {:?}",
        result.output
    );
}

#[tokio::test]
async fn tool_manifest_intent_can_combine_with_category_filter() {
    // category is a strict ToolDefinition.category filter, not a flow/group filter.
    // apply_unified_diff is a patch-category mutation and is intentionally not a
    // validation-category tool merely because it performs an internal preflight.
    // Intent only ranks/filters discovery output and does not change tool behavior.
    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::ToolManifest {
            query: None,
            limit: None,
            tool_name: None,
            category: Some("validation".to_string()),
            intent: Some("coding".to_string()),
            include_recommended_flows: false,
            include_risk_summary: false,
        })
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["intent"], "coding");
    assert_eq!(result.output["category"], "validation");
    let names: Vec<&str> = result.output["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    // Coding intent keeps the portable project gateway plus explicit formatting;
    // advanced ecosystem validators remain available through exact/category discovery.
    assert_eq!(names, vec!["project_validate", "cargo_fmt"]);
}

#[tokio::test]
async fn audit_and_exploration_intents_exclude_shell_and_jobs() {
    for intent in ["audit", "exploration"] {
        let runtime = test_runtime();
        let result = runtime
            .dispatch(ToolCall::ToolManifest {
                query: None,
                limit: None,
                tool_name: None,
                category: None,
                intent: Some(intent.to_string()),
                include_recommended_flows: false,
                include_risk_summary: false,
            })
            .await;
        assert!(result.success, "{intent}: {:?}", result.error);
        assert_eq!(result.output["intent"], intent);
        assert_eq!(result.output["filtered"], true);
        assert!(
            result.output["returned_count"].as_u64().unwrap()
                < result.output["total_count"].as_u64().unwrap(),
            "{intent} must not return the full manifest"
        );
        let names: Vec<&str> = result.output["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        for forbidden in ["run_shell", "run_job"] {
            assert!(
                !names.contains(&forbidden),
                "{intent} must not include {forbidden}: {names:?}"
            );
        }
        assert!(
            !names.contains(&"start_coding_task"),
            "{intent} must not expose retired start_coding_task: {names:?}"
        );
        if intent == "audit" {
            assert!(
                !names.contains(&"work_on_project"),
                "read-only audit must not implicitly create a Session"
            );
            for required in [
                "resolve_workspace",
                "read_project_overview",
                "read_files",
                "search_project_texts",
                "get_git_status",
                "read_git_log",
                "review_changes",
                "check_workspace_hygiene",
                "read_session_handoff",
                "read_tool_manifest",
            ] {
                assert!(
                    names.contains(&required),
                    "audit intent must include {required}: {names:?}"
                );
            }
            for demoted in webcodex_tool_contracts::ORDINARY_DISCOVERY_DEMOTED_REVIEW_TOOL_NAMES {
                assert!(
                    !names.contains(demoted),
                    "audit intent should keep {demoted} out of ordinary review selection: {names:?}"
                );
            }
            for compatibility_primitive in [
                "read_file",
                "search_project_text",
                "git_diff",
                "git_diff_summary",
            ] {
                assert!(
                    !names.contains(&compatibility_primitive),
                    "audit intent should keep {compatibility_primitive} exact-discovery-only: {names:?}"
                );
            }
            for tool in result.output["tools"].as_array().unwrap() {
                let name = tool["name"].as_str().unwrap();
                assert_ne!(
                    tool["risk"], "project_write",
                    "audit intent must exclude Project mutation: {tool:?}"
                );
                assert_ne!(
                    tool["risk"], "job_run",
                    "audit intent must exclude command/Job execution: {tool:?}"
                );
                assert_eq!(
                    tool["approval"], "none",
                    "audit intent must not introduce standard interactive approval: {tool:?}"
                );
                assert_eq!(
                    tool["shell_like"], false,
                    "audit intent must exclude shell-like tools: {tool:?}"
                );
                if name == "work_on_project" {
                    assert_eq!(tool["effect"], "mutate");
                    assert_eq!(tool["risk"], "workflow_manage");
                } else {
                    assert_eq!(
                        tool["effect"], "observe",
                        "only work_on_project may mutate bounded Workflow state in audit intent: {tool:?}"
                    );
                    assert_eq!(tool["read_only"], true, "{name}");
                }
            }
        }
    }
}

#[tokio::test]
async fn release_intent_includes_list_jobs_but_not_run_shell_or_run_job() {
    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::ToolManifest {
            query: None,
            limit: None,
            tool_name: None,
            category: None,
            intent: Some("release".to_string()),
            include_recommended_flows: false,
            include_risk_summary: false,
        })
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["intent"], "release");
    let names: Vec<&str> = result.output["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert!(
        !names.contains(&"read_project_overview"),
        "release intent must not include read_project_overview: {names:?}"
    );
    assert!(
        names.contains(&"list_jobs"),
        "release intent should keep read-only list_jobs: {names:?}"
    );
    for forbidden in ["run_shell", "run_job"] {
        assert!(
            !names.contains(&forbidden),
            "release intent must not include {forbidden}: {names:?}"
        );
    }
}

fn assert_recommended_flows_subset_of_manifest_tools(manifest: &Value, context: &str) {
    use crate::tool_runtime::tool_definition::TOOL_RECOMMENDED_FLOWS;

    let tool_names: std::collections::BTreeSet<&str> = manifest["tools"]
        .as_array()
        .expect("manifest tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    let flows = manifest["recommended_flows"]
        .as_array()
        .expect("recommended_flows");
    for flow in flows {
        let flow_name = flow["name"].as_str().unwrap_or("<unnamed>");
        let tools = flow["tools"]
            .as_array()
            .unwrap_or_else(|| panic!("{context}: flow {flow_name} tools"));
        assert!(
            !tools.is_empty(),
            "{context}: projected flow {flow_name} must not be empty"
        );
        for tool in tools {
            let tool = tool.as_str().expect("flow tool name");
            assert!(
                tool_names.contains(tool),
                "{context}: recommended_flows[{flow_name}] references invisible tool {tool}; visible={tool_names:?}"
            );
        }

        let canonical = TOOL_RECOMMENDED_FLOWS
            .iter()
            .find(|candidate| candidate.name == flow_name)
            .unwrap_or_else(|| panic!("{context}: unknown canonical flow {flow_name}"));
        let omitted = canonical
            .tools
            .iter()
            .copied()
            .filter(|tool| !tool_names.contains(tool))
            .collect::<Vec<_>>();
        if omitted.is_empty() {
            assert_ne!(
                flow["partial"], true,
                "{context}: complete flow {flow_name}"
            );
            assert!(
                flow.get("omitted_tools").is_none(),
                "{context}: complete flow {flow_name}"
            );
            assert_eq!(flow["purpose"], canonical.manifest_purpose);
        } else {
            assert_eq!(flow["partial"], true, "{context}: partial flow {flow_name}");
            assert_eq!(
                flow["omitted_tools"],
                json!(omitted),
                "{context}: {flow_name}"
            );
            let purpose = flow["purpose"]
                .as_str()
                .expect("partial recommended flow purpose");
            assert!(
                purpose.starts_with("Partial projection of the canonical flow"),
                "{context}: partial flow purpose must identify projection: {flow}"
            );
            assert!(
                purpose.contains("not selected into this projection"),
                "{context}: partial flow purpose must distinguish sparse selection from availability: {flow}"
            );
            assert!(
                !purpose.contains("unavailable"),
                "{context}: partial flow purpose must not misclassify omitted tools as unavailable: {flow}"
            );
        }
    }
}

#[tokio::test]
async fn tool_manifest_default_flows_follow_exact_vs_discovery_shape_end_to_end() {
    let runtime = test_runtime();

    let exact = runtime
        .dispatch(
            ToolCall::from_tool_name("read_tool_manifest", json!({"tool_name": "cargo_test"}))
                .unwrap(),
        )
        .await;
    assert!(exact.success, "{:?}", exact.error);
    assert!(exact.output.get("recommended_flows").is_none());

    for specialist in ["cargo_check", "cargo_test"] {
        let specialist_true = runtime
            .dispatch(
                ToolCall::from_tool_name(
                    "read_tool_manifest",
                    json!({
                        "tool_name": specialist,
                        "include_recommended_flows": true
                    }),
                )
                .unwrap(),
            )
            .await;
        assert!(
            specialist_true.success,
            "{specialist}: {:?}",
            specialist_true.error
        );
        assert!(
            specialist_true.output["recommended_flows"]
                .as_array()
                .is_some_and(Vec::is_empty),
            "{specialist}"
        );
    }

    let ordinary_true = runtime
        .dispatch(
            ToolCall::from_tool_name(
                "read_tool_manifest",
                json!({
                    "tool_name": "project_validate",
                    "include_recommended_flows": true
                }),
            )
            .unwrap(),
        )
        .await;
    assert!(ordinary_true.success, "{:?}", ordinary_true.error);
    assert!(ordinary_true.output["recommended_flows"]
        .as_array()
        .is_some_and(|flows| !flows.is_empty()));

    let exact_false = runtime
        .dispatch(
            ToolCall::from_tool_name(
                "read_tool_manifest",
                json!({
                    "tool_name": "cargo_test",
                    "include_recommended_flows": false
                }),
            )
            .unwrap(),
        )
        .await;
    assert!(exact_false.success, "{:?}", exact_false.error);
    assert!(exact_false.output.get("recommended_flows").is_none());

    for arguments in [
        json!({}),
        json!({"category": "validation"}),
        json!({"intent": "coding"}),
    ] {
        let result = runtime
            .dispatch(ToolCall::from_tool_name("read_tool_manifest", arguments).unwrap())
            .await;
        assert!(result.success, "{:?}", result.error);
        assert!(result.output["recommended_flows"]
            .as_array()
            .is_some_and(|flows| !flows.is_empty()));
    }
}

#[tokio::test]
async fn filtered_tool_manifest_recommended_flows_only_reference_returned_tools() {
    let runtime = test_runtime();

    // intent=coding
    let coding = runtime
        .dispatch(ToolCall::ToolManifest {
            query: None,
            limit: None,
            tool_name: None,
            category: None,
            intent: Some("coding".to_string()),
            include_recommended_flows: true,
            include_risk_summary: false,
        })
        .await;
    assert!(coding.success, "{:?}", coding.error);
    assert_eq!(coding.output["filtered"], true);
    assert_recommended_flows_subset_of_manifest_tools(&coding.output, "intent=coding");
    let coding_names: Vec<&str> = coding.output["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(
        coding_names.contains(&"edit_project_files"),
        "coding intent tools should expose canonical precise edits: {coding_names:?}"
    );
    assert!(
        !coding_names.contains(&"apply_patch"),
        "exact-manifest patch specialist should stay outside ordinary coding intent: {coding_names:?}"
    );
    assert!(
        !coding_names.contains(&"apply_unified_diff"),
        "external raw unified-diff compatibility path should stay outside ordinary coding intent: {coding_names:?}"
    );
    assert!(
        !coding_names.contains(&"replace_line_range"),
        "coding intent should not rank line compatibility tools: {coding_names:?}"
    );

    // single category filter
    let file_only = runtime
        .dispatch(ToolCall::ToolManifest {
            query: None,
            limit: None,
            tool_name: None,
            category: Some("file".to_string()),
            intent: None,
            include_recommended_flows: true,
            include_risk_summary: false,
        })
        .await;
    assert!(file_only.success, "{:?}", file_only.error);
    assert_eq!(file_only.output["filtered"], true);
    assert_recommended_flows_subset_of_manifest_tools(&file_only.output, "category=file");

    // multi-category startup-style filter without patch
    let no_patch = runtime
        .compact_tool_manifest_payload_bounded(
            Some(vec![
                "workflow".to_string(),
                "file".to_string(),
                "edit".to_string(),
                "validation".to_string(),
                "git".to_string(),
                "cleanup".to_string(),
            ]),
            Some("coding".to_string()),
            None,
        )
        .expect("startup-style no-patch manifest");
    assert_eq!(no_patch["filtered"], true);
    assert_recommended_flows_subset_of_manifest_tools(&no_patch, "startup no-patch");
    let no_patch_tools = serde_json::to_string(&no_patch["tools"]).unwrap();
    assert!(
        !no_patch_tools.contains("apply_patch"),
        "without patch category, tools must not include apply_patch"
    );
    assert!(
        !no_patch_tools.contains("apply_unified_diff"),
        "without patch category, tools must not include apply_unified_diff"
    );
    for forbidden in ["apply_patch", "apply_unified_diff"] {
        assert!(
            no_patch["recommended_flows"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|flow| flow["tools"].as_array().into_iter().flatten())
                .all(|tool| tool.as_str() != Some(forbidden)),
            "without patch category, recommended flow tool references must not include {forbidden}"
        );
    }

    // same filter with patch
    let with_patch = runtime
        .compact_tool_manifest_payload_bounded(
            Some(vec![
                "workflow".to_string(),
                "file".to_string(),
                "edit".to_string(),
                "patch".to_string(),
                "validation".to_string(),
                "git".to_string(),
                "cleanup".to_string(),
            ]),
            Some("coding".to_string()),
            None,
        )
        .expect("startup-style with-patch manifest");
    assert_recommended_flows_subset_of_manifest_tools(&with_patch, "startup with-patch");
    let with_patch_tools: Vec<&str> = with_patch["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(
        !with_patch_tools.contains(&"apply_patch"),
        "coding intent must not reintroduce exact-manifest apply_patch even when patch category is requested: {with_patch_tools:?}"
    );
    assert!(
        !with_patch_tools.contains(&"apply_unified_diff"),
        "coding intent should keep external raw unified-diff mutation exact/category-only: {with_patch_tools:?}"
    );
    let edit_flow = with_patch["recommended_flows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|flow| flow["name"] == "edit")
        .expect("edit flow");
    assert!(
        edit_flow["tools"]
            .as_array()
            .unwrap()
            .iter()
            .all(|tool| tool != "apply_patch"),
        "filtered coding edit flow must not reintroduce exact-manifest apply_patch: {edit_flow}"
    );
    assert!(
        edit_flow["tools"]
            .as_array()
            .unwrap()
            .iter()
            .all(|tool| tool != "apply_unified_diff"),
        "filtered coding edit flow should not reintroduce apply_unified_diff: {edit_flow}"
    );

    // limit truncation after intent ordering
    let limited = runtime
        .compact_tool_manifest_payload_bounded(None, Some("coding".to_string()), Some(3))
        .expect("limit-truncated coding manifest");
    assert_eq!(limited["returned_count"], 3);
    assert_eq!(limited["truncated"], true);
    assert_recommended_flows_subset_of_manifest_tools(&limited, "intent=coding limit=3");
}

#[cfg(feature = "experimental-code-mode")]
#[tokio::test]
async fn code_mode_exact_manifest_projects_canonical_stage_callable_contracts() {
    use crate::tool_runtime::code_mode::{
        code_mode_callable_stage_for_entry_tool, code_mode_orchestration_policy,
    };
    use crate::tool_runtime::orchestration_host::is_server_owned_orchestration_argument;

    let runtime = test_runtime();
    let specs = registered_tool_specs();
    let cases = [
        ("execute_code_mode", "read_only"),
        ("execute_effectful_code_mode", "validation"),
        ("execute_mutating_code_mode", "guarded_edit"),
    ];

    for (entry_tool, expected_stage) in cases {
        let result = runtime
            .dispatch(ToolCall::ToolManifest {
                query: None,
                limit: None,
                tool_name: Some(entry_tool.to_string()),
                category: None,
                intent: None,
                include_recommended_flows: false,
                include_risk_summary: false,
            })
            .await;
        assert!(result.success, "{entry_tool}: {:?}", result.error);
        let projection = &result.output["code_mode_callable_contract"];
        assert_eq!(projection["version"], 2);
        assert_eq!(
            projection["result_envelope"],
            json!(["success", "output", "error"])
        );
        assert_eq!(projection["output_fields_scope"], "output");
        assert_eq!(projection["stage"], expected_stage, "{entry_tool}");
        assert_eq!(projection["entry_tool"], entry_tool, "{entry_tool}");
        assert_eq!(projection["authority"], "presentation_only", "{entry_tool}");
        let mut sparse = crate::tool_runtime::ToolResult::ok(result.output.clone());
        crate::tool_runtime::surface::sparsify_tool_manifest_model_result(&mut sparse);
        assert_eq!(
            sparse.output["code_mode_callable_contract"], *projection,
            "{entry_tool} sparse model projection must preserve the bounded callable contract"
        );

        let stage =
            code_mode_callable_stage_for_entry_tool(entry_tool).expect("Code Mode entry stage");
        let policy = code_mode_orchestration_policy(stage);
        assert_eq!(
            projection["constraints"]["max_mutation_calls"],
            json!(policy.max_mutation_calls)
        );
        assert_eq!(
            projection["constraints"]["validation_after_successful_known_mutation"],
            policy.validation_after_mutation
        );
        assert!(
            projection["constraints"]
                .get("nested_sync_wait_max_secs")
                .is_none(),
            "{entry_tool} must not expose internal return timing"
        );
        assert!(
            !projection["examples"]
                .to_string()
                .contains("sync_wait_secs"),
            "{entry_tool} examples must not teach hidden compatibility timing"
        );
        let projected_names = projection["tools"]
            .as_array()
            .expect("projected callable tools")
            .iter()
            .map(|tool| tool["tool"].as_str().expect("projected tool name"))
            .collect::<Vec<_>>();
        assert_eq!(projected_names, policy.admitted_tools, "{entry_tool}");
        assert!(!projected_names
            .iter()
            .any(|name| name.starts_with("execute_code_mode")));

        for tool in projection["tools"].as_array().unwrap() {
            let tool_name = tool["tool"].as_str().unwrap();
            let canonical = spec_named(&specs, tool_name);
            let input = &tool["input"];
            let input_properties = input["properties"]
                .as_object()
                .unwrap_or_else(|| panic!("{tool_name} projected input properties"));
            assert!(
                !input_properties.contains_key("sync_wait_secs"),
                "{tool_name} callable projection must keep legacy sync_wait_secs hidden"
            );
            assert!(
                input_properties
                    .keys()
                    .all(|field| !is_server_owned_orchestration_argument(field)),
                "{tool_name} exposed a server-owned input: {input_properties:?}"
            );
            if let Some(required) = input.get("required").and_then(Value::as_array) {
                assert!(required.iter().all(|field| {
                    field
                        .as_str()
                        .is_none_or(|field| !is_server_owned_orchestration_argument(field))
                }));
            }
            let output_fields = tool["output_fields"]
                .as_array()
                .unwrap_or_else(|| panic!("{tool_name} output_fields"));
            assert!(
                !output_fields.is_empty(),
                "{tool_name} must expose bounded useful output fields"
            );
            assert!(
                output_fields.len()
                    <= crate::tool_runtime::surface::CODE_MODE_OUTPUT_FIELDS_PER_TOOL_MAX,
                "{tool_name} output projection exceeded its per-tool field bound: {}",
                output_fields.len()
            );
            assert!(
                output_fields.iter().all(|field| field
                    .as_str()
                    .is_none_or(|field| !field.contains("job_attention"))),
                "{tool_name} nested callable projection must not advertise outer-only job_attention"
            );
            assert_eq!(
                input["additionalProperties"], canonical.input_schema["additionalProperties"],
                "{tool_name} closed-object semantics drifted"
            );
        }

        assert_eq!(
            projection["bounds"]["output_fields_per_tool_max"],
            crate::tool_runtime::surface::CODE_MODE_OUTPUT_FIELDS_PER_TOOL_MAX
        );
        let bytes = serde_json::to_vec(projection).unwrap().len();
        println!("code_mode_callable_projection stage={expected_stage} bytes={bytes}");
        let soft_max_bytes = match expected_stage {
            "read_only" => 10 * 1024,
            "validation" => 13 * 1024,
            // E2c adds both canonical validators to the guarded-edit projection;
            // the shared 16 KiB hard transport bound remains unchanged.
            "guarded_edit" => 15 * 1024,
            _ => unreachable!(),
        };
        assert!(
            bytes <= soft_max_bytes,
            "{expected_stage} projection is {bytes} bytes; soft cap is {soft_max_bytes}"
        );
        assert!(
            bytes <= crate::tool_runtime::surface::CODE_MODE_CALLABLE_CONTRACT_HARD_MAX_BYTES,
            "{expected_stage} projection is {bytes} bytes"
        );
    }
}

#[cfg(feature = "experimental-code-mode")]
#[tokio::test]
async fn code_mode_callable_projection_preserves_key_input_constraints_and_output_handoffs() {
    fn qualified_output_fields(tool: &Value) -> Vec<Value> {
        // Reconstruct the same path assertions from the shared envelope and
        // relative paths; compact presentation must not lose these semantics.
        let mut fields = vec![json!("success"), json!("error")];
        fields.extend(
            tool["output_fields"]
                .as_array()
                .unwrap()
                .iter()
                .map(|field| json!(format!("output.{}", field.as_str().unwrap()))),
        );
        fields
    }
    let runtime = test_runtime();

    let read_only = runtime
        .dispatch(ToolCall::ToolManifest {
            query: None,
            limit: None,
            tool_name: Some("execute_code_mode".to_string()),
            category: None,
            intent: None,
            include_recommended_flows: false,
            include_risk_summary: false,
        })
        .await;
    assert!(read_only.success, "{:?}", read_only.error);
    let read_tools = read_only.output["code_mode_callable_contract"]["tools"]
        .as_array()
        .unwrap();
    let read_files = read_tools
        .iter()
        .find(|tool| tool["tool"] == "read_files")
        .expect("read_files projection");
    assert_eq!(read_files["input"]["required"], json!(["items"]));
    assert_eq!(read_files["input"]["properties"]["items"]["maxItems"], 8);
    assert_eq!(
        read_files["input"]["properties"]["items"]["items"]["properties"]["start_line"]["minimum"],
        0
    );
    assert!(read_files["input"]["properties"].get("project").is_none());
    assert!(read_files["input"]["properties"]
        .get("session_id")
        .is_none());
    let read_outputs = qualified_output_fields(read_files);
    for field in [
        "output.items[].path",
        "output.items[].output.text",
        "output.items[].output.read_revision",
        "output.items[].output.returned_lines",
        "output.items[].output.has_more",
    ] {
        assert!(read_outputs.contains(&json!(field)), "missing {field}");
    }
    let search = read_tools
        .iter()
        .find(|tool| tool["tool"] == "search_project_texts")
        .expect("search_project_texts projection");
    let search_outputs = qualified_output_fields(search);
    for field in [
        "output.items[].output.matches",
        "output.items[].output.matches[].path",
        "output.items[].output.matches[].line",
        "output.items[].output.matches[].preview",
        "output.items[].output.matches[].read_hint",
    ] {
        assert!(search_outputs.contains(&json!(field)), "missing {field}");
    }

    let validation = runtime
        .dispatch(ToolCall::ToolManifest {
            query: None,
            limit: None,
            tool_name: Some("execute_effectful_code_mode".to_string()),
            category: None,
            intent: None,
            include_recommended_flows: false,
            include_risk_summary: false,
        })
        .await;
    assert!(validation.success, "{:?}", validation.error);
    let validation_tools = validation.output["code_mode_callable_contract"]["tools"]
        .as_array()
        .unwrap();
    let cargo_test = validation_tools
        .iter()
        .find(|tool| tool["tool"] == "cargo_test")
        .expect("cargo_test projection");
    assert_eq!(cargo_test["input"]["properties"]["min_tests"]["minimum"], 1);
    assert!(cargo_test["input"]["properties"]
        .get("result_expectation")
        .is_none());
    let cargo_test_outputs = qualified_output_fields(cargo_test);
    for field in [
        "success",
        "output.execution_state",
        "output.terminal",
        "output.passed",
        "output.source_state",
        "output.source_state.freshness",
        "output.source_state.observed_mutation_fence",
        "output.failure_kind",
        "output.job_id",
        "output.continuation",
        "output.diagnostics",
    ] {
        assert!(
            cargo_test_outputs.contains(&json!(field)),
            "missing {field}"
        );
    }

    let guarded = runtime
        .dispatch(ToolCall::ToolManifest {
            query: None,
            limit: None,
            tool_name: Some("execute_mutating_code_mode".to_string()),
            category: None,
            intent: None,
            include_recommended_flows: false,
            include_risk_summary: false,
        })
        .await;
    assert!(guarded.success, "{:?}", guarded.error);
    assert_eq!(
        guarded.output["code_mode_callable_contract"]["constraints"]
            ["validation_after_successful_known_mutation"],
        true
    );
    assert_eq!(
        guarded.output["code_mode_callable_contract"]["constraints"]["max_mutation_calls"],
        1
    );
    let guarded_tools = guarded.output["code_mode_callable_contract"]["tools"]
        .as_array()
        .unwrap();
    let edit = guarded_tools
        .iter()
        .find(|tool| tool["tool"] == "edit_project_files")
        .expect("apply_text_edits projection");
    assert_eq!(edit["input"]["properties"]["changes"]["maxItems"], 16);
    assert!(edit["input"]["properties"].get("project").is_none());
    let edit_outputs = qualified_output_fields(edit);
    for field in [
        "success",
        "output.state_changed",
        "output.execution_state",
        "output.error_kind",
        "output.change_index",
        "output.edit_index",
        "output.conflicting_edit_indices",
        "output.conflicting_edit_ranges",
        "output.recovery",
    ] {
        assert!(edit_outputs.contains(&json!(field)), "missing {field}");
    }
}

#[cfg(feature = "experimental-code-mode")]
#[test]
fn experimental_tool_manifest_schema_declares_callable_contract_sidecar() {
    let specs = registered_tool_specs();
    let manifest = spec_named(&specs, "read_tool_manifest");
    assert!(output_schema_properties(manifest).contains_key("code_mode_callable_contract"));
}

#[cfg(not(feature = "experimental-code-mode"))]
#[test]
fn default_tool_manifest_schema_omits_callable_contract_sidecar() {
    let specs = registered_tool_specs();
    let manifest = spec_named(&specs, "read_tool_manifest");
    assert!(!output_schema_properties(manifest).contains_key("code_mode_callable_contract"));
}

#[tokio::test]
async fn tool_manifest_exact_tool_returns_input_contract_without_output_schema() {
    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::ToolManifest {
            query: None,
            limit: None,
            tool_name: Some("cargo_test".to_string()),
            category: None,
            intent: None,
            include_recommended_flows: false,
            include_risk_summary: false,
        })
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["tool_name"], "cargo_test");
    assert_eq!(result.output["count"], 1);
    assert_eq!(result.output["returned_count"], 1);
    let categories = result.output["categories"].as_object().unwrap();
    assert_eq!(categories.len(), 1);
    assert_eq!(categories["validation"], json!(["cargo_test"]));
    let contract = &result.output["contract"];
    assert_eq!(contract["name"], "cargo_test");
    assert!(contract["description"].as_str().is_some());
    assert_eq!(contract["input_schema"]["type"], "object");
    assert!(contract["input_schema"]["properties"]["package"].is_object());
    assert!(
        contract["input_schema"]["properties"]
            .get("sync_wait_secs")
            .is_none(),
        "read_tool_manifest must not re-expose legacy sync_wait_secs tuning"
    );
    assert!(contract["annotations"].is_object());
    let specs = registered_tool_specs();
    let manifest_spec = spec_named(&specs, "read_tool_manifest");
    let contract_schema =
        &manifest_spec.output_schema["properties"]["output"]["properties"]["contract"]["anyOf"][0];
    let contract_schema_properties = contract_schema["properties"].as_object().unwrap();
    for key in contract.as_object().unwrap().keys() {
        assert!(
            contract_schema_properties.contains_key(key),
            "read_tool_manifest exact contract runtime key {key} is missing from output_schema"
        );
    }
    for key in ["effect", "risk", "approval", "idempotency"] {
        assert!(
            contract_schema["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|required| required == key),
            "read_tool_manifest exact contract output_schema must require {key}"
        );
    }
    assert_eq!(contract["availability"], "gateway");
    assert_eq!(
        contract["gateway_tool"],
        crate::model_surface::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME
    );
    assert!(contract.get("output_schema").is_none());
    assert_eq!(result.output["tools"][0]["name"], "cargo_test");
    assert_eq!(result.output["tools"][0]["availability"], "gateway");
    assert_eq!(
        result.output["tools"][0]["gateway_tool"],
        crate::model_surface::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME
    );
    assert!(result.output["tools"][0].get("input_schema").is_none());
}

#[tokio::test]
async fn exact_tool_manifest_projects_bounded_host_orchestration_from_tool_definition_only() {
    let runtime = test_runtime();
    let cases = [
        (
            "read_files",
            json!({
                "guidance_only": true,
                "concurrency": "independent_parallel_read",
                "native_batch_field": "items",
                "compound_preferred": false,
            }),
        ),
        (
            "search_file_context",
            json!({
                "guidance_only": true,
                "concurrency": "independent_parallel_read",
                "native_batch_field": "queries",
                "compound_preferred": true,
            }),
        ),
        (
            "cargo_check",
            json!({
                "guidance_only": true,
                "concurrency": "sequential",
                "native_batch_field": "packages",
                "compound_preferred": false,
            }),
        ),
    ];

    for (tool_name, expected) in cases {
        let result = runtime
            .dispatch(ToolCall::ToolManifest {
                query: None,
                limit: None,
                tool_name: Some(tool_name.to_string()),
                category: None,
                intent: None,
                include_recommended_flows: false,
                include_risk_summary: false,
            })
            .await;
        assert!(result.success, "{tool_name}: {:?}", result.error);
        assert_eq!(result.output["host_orchestration"], expected, "{tool_name}");
        assert_no_response_too_large(tool_name, &result.output);

        let definition =
            crate::tool_runtime::tool_definition::lookup_tool_definition(tool_name).unwrap();
        assert_eq!(
            result.output["host_orchestration"]["concurrency"],
            definition.host_orchestration.concurrency.as_str(),
            "{tool_name}"
        );
    }

    let no_hint = runtime
        .dispatch(ToolCall::ToolManifest {
            query: None,
            limit: None,
            tool_name: Some("run_shell".to_string()),
            category: None,
            intent: None,
            include_recommended_flows: false,
            include_risk_summary: false,
        })
        .await;
    assert!(no_hint.success, "{:?}", no_hint.error);
    assert!(no_hint.output.get("host_orchestration").is_none());

    let specs = registered_tool_specs();
    let manifest_spec = spec_named(&specs, "read_tool_manifest");
    let output_properties = output_schema_properties(manifest_spec);
    let schema = &output_properties["host_orchestration"];
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(
        schema["properties"]["concurrency"]["enum"],
        json!(["unspecified", "independent_parallel_read", "sequential"])
    );

    let specs = registered_tool_specs();
    for tool_name in [
        "read_files",
        "search_project_texts",
        "search_file_context",
        "cargo_check",
        "edit_project_files",
    ] {
        let serialized = serde_json::to_string(spec_named(&specs, tool_name)).unwrap();
        assert!(
            !serialized.contains("host_orchestration"),
            "{tool_name} default ToolSpec must not carry Host orchestration metadata"
        );
    }
}

#[tokio::test]
async fn tool_manifest_projects_canonical_execution_selection_for_exact_and_filtered_views() {
    let runtime = test_runtime();
    let expected = json!({
        "form": "shell_command",
        "lifetime": "runner",
        "start": "sync_first",
        "continuation": "observe_jobs",
    });

    let exact = runtime
        .dispatch(ToolCall::ToolManifest {
            query: None,
            limit: None,
            tool_name: Some("run_shell".to_string()),
            category: None,
            intent: None,
            include_recommended_flows: false,
            include_risk_summary: false,
        })
        .await;
    assert!(exact.success, "{:?}", exact.error);
    assert_eq!(exact.output["contract"]["execution"], expected);
    assert_eq!(exact.output["tools"][0]["execution"], expected);
    assert_eq!(exact.output["contract"]["availability"], "direct");

    let specs = registered_tool_specs();
    let manifest_spec = spec_named(&specs, "read_tool_manifest");
    let output_properties = output_schema_properties(manifest_spec);
    let execution_schema = &output_properties["execution"];
    assert_eq!(
        execution_schema["properties"]["form"]["enum"],
        json!([
            "native_argv",
            "typed_script",
            "shell_command",
            "structured_validation",
            "persistent_shell_command"
        ])
    );
    assert_eq!(
        execution_schema["properties"]["lifetime"]["enum"],
        json!(["runner", "supervisor", "session_shell"])
    );
    assert_eq!(
        execution_schema["properties"]["start"]["enum"],
        json!(["sync_first", "async_immediate", "existing_session"])
    );
    assert_eq!(
        execution_schema["properties"]["continuation"]["enum"],
        json!(["observe_jobs", "session_shell", "none"])
    );

    let mut sparse_exact = crate::tool_runtime::ToolResult::ok(exact.output.clone());
    crate::tool_runtime::surface::sparsify_tool_manifest_model_result(&mut sparse_exact);
    assert_eq!(sparse_exact.output["execution"], expected);
    assert_payload_keys_declared(
        "read_tool_manifest sparse exact execution",
        &sparse_exact.output,
        output_properties,
    );

    let filtered = runtime
        .dispatch(ToolCall::ToolManifest {
            query: None,
            limit: None,
            tool_name: None,
            category: Some("execution".to_string()),
            intent: Some("coding".to_string()),
            include_recommended_flows: false,
            include_risk_summary: false,
        })
        .await;
    assert!(filtered.success, "{:?}", filtered.error);
    let run_shell = filtered.output["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "run_shell")
        .expect("filtered run_shell");
    assert_eq!(run_shell["execution"], expected);

    let read_files = runtime
        .dispatch(ToolCall::ToolManifest {
            query: None,
            limit: None,
            tool_name: Some("read_files".to_string()),
            category: None,
            intent: None,
            include_recommended_flows: false,
            include_risk_summary: false,
        })
        .await;
    assert!(read_files.success, "{:?}", read_files.error);
    assert!(read_files.output["contract"].get("execution").is_none());
    assert!(read_files.output["tools"][0].get("execution").is_none());
}

#[tokio::test]
async fn tool_manifest_exact_persistent_shell_tool_surfaces_its_reuse_flow() {
    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::ToolManifest {
            query: None,
            limit: None,
            tool_name: Some("open_session_shell".to_string()),
            category: None,
            intent: None,
            include_recommended_flows: true,
            include_risk_summary: false,
        })
        .await;
    assert!(result.success, "{:?}", result.error);
    let flows = result.output["recommended_flows"]
        .as_array()
        .expect("exact persistent-shell recommended flows");
    let flow = flows
        .iter()
        .find(|flow| flow["name"] == "persistent_shell")
        .expect("persistent_shell flow");
    let purpose = flow["purpose"].as_str().expect("persistent_shell purpose");
    assert!(purpose.contains("Runner-local named SSH resource"));
    assert!(purpose.contains("not an arbitrary host"));
    assert!(purpose.contains("does not run WebCodex Runner"));
    for tool in [
        "update_session_context",
        "open_session_shell",
        "execute_session_shell",
        "get_session_shell_status",
        "close_session_shell",
        "run_process",
    ] {
        assert!(purpose.contains(tool), "persistent_shell purpose: {tool}");
    }
    // Exact manifests keep their returned tool set bounded to the requested tool,
    // while the flow purpose names the sibling tools needed to complete the route.
    assert_eq!(flow["tools"], json!(["open_session_shell"]));
}

#[tokio::test]
async fn tool_manifest_exact_fleet_tool_surfaces_exact_runner_targeting_route() {
    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::ToolManifest {
            query: None,
            limit: None,
            tool_name: Some("list_runners".to_string()),
            category: None,
            intent: None,
            include_recommended_flows: true,
            include_risk_summary: false,
        })
        .await;
    assert!(result.success, "{:?}", result.error);
    let flow = result.output["recommended_flows"]
        .as_array()
        .expect("exact fleet recommended flows")
        .iter()
        .find(|flow| flow["name"] == "discovery")
        .expect("discovery flow");
    let purpose = flow["purpose"].as_str().expect("discovery purpose");
    assert!(purpose.contains("get_runtime_status(client_id=...)"));
    assert!(purpose.contains("list_projects(client_id=...)"));
    assert!(purpose.contains("list_runners"));
    assert!(!purpose.contains("list_agents"));
    assert_eq!(flow["tools"], json!(["list_runners"]));
}

#[tokio::test]
async fn tool_manifest_projects_canonical_semantic_contracts() {
    let runtime = test_runtime();
    for (tool_name, effect, risk, approval, idempotency, read_only) in [
        (
            "read_files",
            "observe",
            "read_only",
            "none",
            "pure_read",
            true,
        ),
        (
            "close_session",
            "mutate",
            "session_collaborate",
            "none",
            "desired_state",
            false,
        ),
        (
            "start_coding_agent",
            "execute",
            "job_run",
            "standard",
            "keyed",
            false,
        ),
        (
            "cancel_coding_agent",
            "mutate",
            "run_control",
            "inherit_from_start",
            "desired_state",
            false,
        ),
    ] {
        let result = runtime
            .dispatch(ToolCall::ToolManifest {
                query: None,
                limit: None,
                tool_name: Some(tool_name.to_string()),
                category: None,
                intent: None,
                include_recommended_flows: false,
                include_risk_summary: false,
            })
            .await;
        assert!(result.success, "{tool_name}: {:?}", result.error);
        let contract = &result.output["contract"];
        assert_eq!(contract["effect"], effect, "{tool_name}");
        assert_eq!(contract["risk"], risk, "{tool_name}");
        assert_eq!(contract["approval"], approval, "{tool_name}");
        assert_eq!(contract["idempotency"], idempotency, "{tool_name}");
        let compact = &result.output["tools"][0];
        assert_eq!(compact["effect"], effect, "{tool_name}");
        assert_eq!(compact["risk"], risk, "{tool_name}");
        assert_eq!(compact["approval"], approval, "{tool_name}");
        assert_eq!(compact["idempotency"], idempotency, "{tool_name}");
        assert_eq!(compact["read_only"], read_only, "{tool_name}");
    }
}

#[tokio::test]
async fn tool_manifest_routing_metadata_uses_canonical_adaptive_routes() {
    let runtime = test_runtime();
    for (tool_name, availability, gateway_tool) in [
        ("run_process", "direct", None),
        ("run_shell", "direct", None),
        ("import_host_files", "direct", None),
        ("inspect_project_artifact", "direct", None),
        ("read_session_discussion_summary", "direct", None),
        ("list_jobs", "gateway", Some("call_runtime_tool")),
        ("review_changes", "direct", None),
        (
            "read_workspace_changes",
            "gateway",
            Some("call_runtime_tool"),
        ),
        ("read_git_diff_hunks", "gateway", Some("call_runtime_tool")),
        (
            "read_git_review_summary",
            "gateway",
            Some("call_runtime_tool"),
        ),
        ("run_script", "direct", None),
        (
            "check_workspace_hygiene",
            "gateway",
            Some("call_runtime_tool"),
        ),
        ("finish_coding_task", "gateway", Some("call_runtime_tool")),
        (
            "save_project_artifact",
            "gateway",
            Some("call_runtime_tool"),
        ),
        (
            "read_project_artifact_chunk",
            "gateway",
            Some("call_runtime_tool"),
        ),
        (
            "begin_artifact_upload",
            "gateway",
            Some("call_runtime_tool"),
        ),
    ] {
        let result = runtime
            .dispatch(ToolCall::ToolManifest {
                query: None,
                limit: None,
                tool_name: Some(tool_name.to_string()),
                category: None,
                intent: None,
                include_recommended_flows: false,
                include_risk_summary: false,
            })
            .await;
        assert!(result.success, "{tool_name}: {:?}", result.error);
        assert_eq!(result.output["contract"]["availability"], availability);
        assert_eq!(
            result.output["contract"]["gateway_tool"],
            gateway_tool.map_or(Value::Null, |name| json!(name))
        );
        assert_eq!(result.output["tools"][0]["availability"], availability);
        assert_eq!(
            result.output["tools"][0]["gateway_tool"],
            gateway_tool.map_or(Value::Null, |name| json!(name))
        );
        if tool_name == "run_script" {
            let expected = json!({
                "form": "typed_script",
                "lifetime": "runner",
                "start": "sync_first",
                "continuation": "observe_jobs",
            });
            assert_eq!(result.output["contract"]["execution"], expected);
            assert_eq!(result.output["tools"][0]["execution"], expected);
        }
    }
}

#[tokio::test]
async fn tool_manifest_operator_extensions_require_explicit_family_capabilities() {
    use crate::tool_runtime::kernel::ToolProtocolCapabilities;

    let runtime = test_runtime();
    let admin = crate::auth::AuthContext {
        is_bootstrap: true,
        ..crate::auth::AuthContext::new(crate::auth::AuthKind::Bootstrap)
    };
    let manifest = |tool_name: &'static str, capabilities: ToolProtocolCapabilities| {
        runtime.tool_manifest(
            Some(&admin),
            Some(tool_name.to_string()),
            None,
            None,
            false,
            false,
            None,
            None,
            capabilities,
        )
    };

    let no_capability = manifest("list_skills", ToolProtocolCapabilities::default()).await;
    assert!(!no_capability.success);
    assert_eq!(no_capability.output["code"], "unknown_tool_manifest_tool");

    let skill_only = ToolProtocolCapabilities {
        skill_runtime: true,
        ..Default::default()
    };
    for name in ["list_skills", "read_skill_file"] {
        let skill = manifest(name, skill_only).await;
        assert!(skill.success, "{:?}", skill.error);
        assert_eq!(skill.output["contract"]["availability"], "gateway");
        assert_eq!(
            skill.output["contract"]["gateway_tool"],
            "call_runtime_tool"
        );
        assert!(skill.output["contract"]["input_schema"].is_object());
        assert!(
            !manifest(name, ToolProtocolCapabilities::default())
                .await
                .success
        );
    }
    for hidden_without_skill_cap in ["install_skill", "search_memory", "read_tool_trace"] {
        let hidden = manifest(hidden_without_skill_cap, skill_only).await;
        assert!(
            !hidden.success,
            "{hidden_without_skill_cap} leaked via skill runtime capability"
        );
        assert_eq!(hidden.output["code"], "unknown_tool_manifest_tool");
    }

    let memory_only = ToolProtocolCapabilities {
        memory_surface: true,
        ..Default::default()
    };
    let memory = manifest("search_memory", memory_only).await;
    assert!(memory.success, "{:?}", memory.error);
    assert_eq!(memory.output["contract"]["availability"], "gateway");
    assert_eq!(
        memory.output["contract"]["gateway_tool"],
        "call_runtime_tool"
    );
    let canonical = crate::tool_runtime::memory_runtime_tool_specs().remove(0);
    assert_eq!(
        memory.output["contract"]["description"],
        canonical.description
    );
    assert_eq!(
        memory.output["contract"]["input_schema"],
        canonical.input_schema
    );
    assert!(!manifest("list_skills", memory_only).await.success);
    assert!(!manifest("read_tool_trace", memory_only).await.success);

    let diagnostic_only = ToolProtocolCapabilities {
        trace_diagnostics: true,
        ..Default::default()
    };
    assert!(manifest("read_tool_trace", diagnostic_only).await.success);
    assert!(!manifest("search_memory", diagnostic_only).await.success);

    let management_only = ToolProtocolCapabilities {
        skill_management: true,
        ..Default::default()
    };
    assert!(manifest("install_skill", management_only).await.success);
    assert!(!manifest("list_skills", management_only).await.success);
    let versions = manifest("list_skill_versions", management_only).await;
    assert!(versions.success, "{:?}", versions.error);
    assert_eq!(versions.output["contract"]["availability"], "gateway");
    assert_eq!(
        versions.output["contract"]["gateway_tool"],
        "call_runtime_tool"
    );
}

#[tokio::test]
async fn tool_manifest_exact_tool_fails_closed_for_unknown_or_mixed_filters() {
    let runtime = test_runtime();
    let unknown = runtime
        .dispatch(ToolCall::ToolManifest {
            query: None,
            limit: None,
            tool_name: Some("not_a_real_webcodex_tool".to_string()),
            category: None,
            intent: None,
            include_recommended_flows: false,
            include_risk_summary: false,
        })
        .await;
    assert!(!unknown.success);
    assert_eq!(unknown.output["code"], "unknown_tool_manifest_tool");

    let mixed = runtime
        .dispatch(ToolCall::ToolManifest {
            query: None,
            limit: None,
            tool_name: Some("cargo_test".to_string()),
            category: Some("validation".to_string()),
            intent: None,
            include_recommended_flows: false,
            include_risk_summary: false,
        })
        .await;
    assert!(!mixed.success);
    assert_eq!(mixed.output["code"], "tool_manifest_exact_filter_conflict");
}

#[tokio::test]
async fn unfiltered_tool_manifest_keeps_full_recommended_flows() {
    use crate::tool_runtime::tool_definition::model_visible_recommended_flows;

    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::ToolManifest {
            query: None,
            limit: None,
            tool_name: None,
            category: None,
            intent: None,
            include_recommended_flows: true,
            include_risk_summary: true,
        })
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["filtered"], false);
    let flows = result.output["recommended_flows"]
        .as_array()
        .expect("unfiltered recommended_flows");
    assert_eq!(
        flows.len(),
        model_visible_recommended_flows().count(),
        "unfiltered recommended_flows must keep the full currently callable set"
    );
    let serialized = result.output["recommended_flows"]
        .to_string()
        .to_lowercase();
    assert!(
        serialized.contains("runner-owned sync-first")
            && serialized.contains("run_job is runner-owned immediate async")
            && serialized.contains("run_detached_process is supervisor-owned immediate async")
            && serialized
                .contains("execute_session_shell continues an existing persistent session shell"),
        "unfiltered flows must keep the canonical execution selection vocabulary: {serialized}"
    );
}

#[cfg(not(feature = "workspace-checkpoints"))]
#[tokio::test]
async fn workspace_checkpoints_disabled_manifest_and_parser() {
    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::from_tool_name("read_tool_manifest", json!({})).unwrap())
        .await;
    assert!(result.success, "{result:?}");
    assert!(!result.output.to_string().contains("workspace_checkpoint_"));
    for suffix in ["create", "list", "show", "restore", "delete"] {
        let name = format!("workspace_checkpoint_{suffix}");
        assert!(ToolCall::from_tool_name(&name, json!({"project": "demo"})).is_err());
    }
}

#[tokio::test]
async fn keyword_discovery_is_bounded_schema_free_and_new_maintenance_tools_stay_gateway() {
    use crate::tool_runtime::kernel::ToolProtocolCapabilities;
    use webcodex_tool_contracts::take_tool_materialization_counts_for_test as take_counts;
    let runtime = test_runtime();
    take_counts();
    let found = runtime
        .tool_manifest(
            None,
            None,
            None,
            None,
            false,
            false,
            Some("batch unregister".into()),
            Some(3),
            ToolProtocolCapabilities::default(),
        )
        .await;
    assert!(found.success, "{found:?}");
    assert!(found.output["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == "unregister_projects"));
    assert_eq!(
        take_counts(),
        (0, 0),
        "keyword selection must not expand all schemas"
    );
    for name in ["resolve_workspace", "unregister_projects"] {
        let exact = runtime
            .tool_manifest(
                None,
                Some(name.into()),
                None,
                None,
                false,
                false,
                None,
                None,
                ToolProtocolCapabilities::default(),
            )
            .await;
        assert!(exact.success, "{exact:?}");
        assert_eq!(exact.output["route"]["primary"]["mode"], "gateway");
    }
    let list = runtime
        .tool_manifest(
            None,
            None,
            None,
            Some("maintenance".into()),
            false,
            false,
            None,
            None,
            ToolProtocolCapabilities::default(),
        )
        .await;
    assert!(list.success);
    assert!(list.output["tools"]
        .as_array()
        .unwrap()
        .iter()
        .all(|tool| tool["name"] != "work_on_project"));
    let hidden = runtime
        .tool_manifest(
            None,
            None,
            None,
            None,
            false,
            false,
            Some("read_memory".into()),
            None,
            ToolProtocolCapabilities::default(),
        )
        .await;
    assert!(hidden.success);
    assert_eq!(hidden.output["count"], 0);
}

#[tokio::test]
async fn keyword_discovery_finds_git_specialists_by_user_task_not_internal_taxonomy() {
    let runtime = test_runtime();
    for (query, expected) in [
        ("staged diff", "read_git_diff_hunks"),
        ("workspace summary", "read_workspace_changes"),
        ("committed statistics", "read_git_review_summary"),
        ("symbol documentation", "read_symbol_hover"),
        ("remove registrations", "unregister_projects"),
    ] {
        let result = runtime
            .dispatch(
                ToolCall::from_tool_name("read_tool_manifest", json!({"query":query,"limit":10}))
                    .unwrap(),
            )
            .await;
        assert!(result.success, "{result:?}");
        assert!(
            result.output["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|tool| tool["name"] == expected),
            "{query}: {result:?}"
        );
        assert!(
            result.output["contract"].is_null(),
            "keyword search is not schema expansion"
        );
    }
}

#[tokio::test]
async fn exact_manifest_ignores_irrelevant_presentation_limit() {
    let runtime = test_runtime();
    for limit in [0, 1, 10_000] {
        let result = runtime
            .dispatch(
                ToolCall::from_tool_name(
                    "read_tool_manifest",
                    json!({"tool_name":"resolve_workspace","limit":limit}),
                )
                .unwrap(),
            )
            .await;
        assert!(result.success, "{result:?}");
        assert_eq!(result.output["contract"]["name"], "resolve_workspace");
        assert_eq!(result.output["route"]["primary"]["mode"], "gateway");
    }
}
