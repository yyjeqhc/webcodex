//! Surface-shaping helpers for model-facing runtime discovery.
//!
//! These functions keep MCP-compatible tool specs, GPT Action compact
//! manifests, and bounded `list_tools` filtering close together while leaving
//! dispatch and authorization flow in `mod.rs`.

use super::metadata::ToolPathHint;
use super::registry::accepted_flattened_args_for_spec;
use super::runtime::ToolRuntime;
use super::tool_definition::{
    is_model_visible_tool_name, runtime_tool_category, runtime_tool_metadata,
    TOOL_DISCOVERY_GROUPS, TOOL_RECOMMENDED_FLOWS,
};
use super::tool_inputs::ListToolsOptions;
use super::tool_result::ToolResult;
use super::tool_spec::ToolSpec;
use serde_json::{json, Value};
use std::collections::BTreeMap;

impl ToolRuntime {
    pub(crate) const LIST_TOOLS_MAX_LIMIT: usize = 100;

    /// Group every accepted tool name into coarse categories so a custom GPT
    /// can pick the right tool family at a glance. A tool may appear in more
    /// than one category. Returned as a JSON object keyed by category.
    pub fn tool_categories(&self) -> Value {
        let mut categories = serde_json::Map::new();
        for group in TOOL_DISCOVERY_GROUPS {
            let tools = group
                .tools
                .iter()
                .filter(|name| is_model_visible_tool_name(name))
                .map(|name| Value::String((*name).to_string()))
                .collect::<Vec<_>>();
            categories.insert(group.name.to_string(), Value::Array(tools));
        }
        Value::Object(categories)
    }

    /// Short, GPT-facing flow hints. Each entry is well under the 300-char
    /// ToolSpec/operation description budget.
    pub fn recommended_flows() -> Vec<&'static str> {
        TOOL_RECOMMENDED_FLOWS
            .iter()
            .map(|flow| flow.summary)
            .collect()
    }

    pub(crate) fn list_tools_payload(&self, options: ListToolsOptions) -> Value {
        let specs = self.tool_specs();
        let total_count = specs.len();
        let filtered_indexes = list_tools_filtered_indexes(&specs, &options);
        let filtered_count = filtered_indexes.len();
        let bounded_request = options.summary_only
            || options.category.is_some()
            || options.features.is_some()
            || options.limit.is_some();
        let effective_limit = options
            .limit
            .map(|limit| limit.clamp(1, Self::LIST_TOOLS_MAX_LIMIT))
            .unwrap_or(Self::LIST_TOOLS_MAX_LIMIT);
        let returned_indexes: Vec<usize> = if bounded_request {
            filtered_indexes
                .iter()
                .copied()
                .take(effective_limit)
                .collect()
        } else {
            filtered_indexes
        };
        let truncated = filtered_count > returned_indexes.len();
        let names: Vec<String> = returned_indexes
            .iter()
            .map(|index| specs[*index].name.clone())
            .collect();
        let all_summary_tools = build_list_tools_summary_entries(&specs);
        let tools = if options.summary_only {
            returned_indexes
                .iter()
                .map(|index| all_summary_tools[*index].clone())
                .collect()
        } else {
            returned_indexes
                .iter()
                .map(|index| serde_json::to_value(&specs[*index]).unwrap_or(Value::Null))
                .collect()
        };

        let mut output = json!({
            "tools": Value::Array(tools),
            "names": names,
            "count": returned_indexes.len(),
            "total_count": total_count,
            "filtered_count": filtered_count,
            "truncated": truncated,
            "category": options.category,
            "features": options.features,
            "limit": if bounded_request { Some(effective_limit) } else { None },
            "categories": if bounded_request {
                build_manifest_categories(&all_summary_tools)
            } else {
                self.tool_categories()
            },
            "recommended_flows": ToolRuntime::recommended_flows(),
            "recommended_next": "For daily GPT Action discovery, call callRuntimeTool with tool=tool_manifest. Use full listRuntimeTools only when debugging schemas.",
            "hint": "Full listRuntimeTools responses include schemas and may be large. Use summary_only=true with category, features, or limit for focused discovery.",
        });
        if !bounded_request {
            output["filtered_count"] = json!(total_count);
            output["total_count"] = json!(total_count);
            output["truncated"] = json!(false);
            output["category"] = Value::Null;
            output["features"] = Value::Null;
            output["limit"] = Value::Null;
        }
        output
    }

    /// Return a compact, bounded tool manifest with categories, risk summary,
    /// and recommended flows. Read-only runtime introspection; never exposes
    /// full input/output schemas, tokens, secrets, or internal paths.
    /// Intended as a lightweight alternative to `list_tools` for long-running
    /// tasks where the full schemas cause ResponseTooLargeError.
    pub(super) async fn tool_manifest(
        &self,
        category: Option<String>,
        include_recommended_flows: bool,
        include_risk_summary: bool,
    ) -> ToolResult {
        ToolResult::ok(self.tool_manifest_payload(
            category,
            include_recommended_flows,
            include_risk_summary,
        ))
    }

    pub(crate) fn compact_tool_manifest_payload(&self) -> Value {
        self.tool_manifest_payload(None, true, true)
    }

    pub(crate) fn compact_tool_manifest_payload_bounded(
        &self,
        categories: Option<Vec<String>>,
        limit: Option<usize>,
    ) -> Value {
        if categories.is_none() && limit.is_none() {
            return self.compact_tool_manifest_payload();
        }
        self.tool_manifest_payload_for_categories(categories, limit, true, true)
    }

    fn tool_manifest_payload(
        &self,
        category: Option<String>,
        include_recommended_flows: bool,
        include_risk_summary: bool,
    ) -> Value {
        self.tool_manifest_payload_for_categories(
            category.map(|category| vec![category]),
            None,
            include_recommended_flows,
            include_risk_summary,
        )
    }

    fn tool_manifest_payload_for_categories(
        &self,
        categories: Option<Vec<String>>,
        limit: Option<usize>,
        include_recommended_flows: bool,
        include_risk_summary: bool,
    ) -> Value {
        let specs = self.tool_specs();
        let tool_count = specs.len();
        let categories_requested = normalize_tool_manifest_categories(categories);
        let category = categories_requested
            .as_ref()
            .and_then(|categories| (categories.len() == 1).then(|| categories[0].clone()));

        // Build compact tool entries from metadata without long schemas or
        // descriptions. This keeps GPT Action discovery payloads bounded.
        let all_tools: Vec<Value> = specs
            .iter()
            .map(|spec| {
                let name = spec.name.as_str();
                let m = runtime_tool_metadata(name);
                json!({
                    "name": name,
                    "category": runtime_tool_category(name),
                    "accepted_flattened_args": accepted_flattened_args_for_spec(spec),
                    "deprecated_or_unsupported_args": [],
                    "provider": m.provider_id,
                    "risk": m.risk.session_risk_class(),
                    "read_only": m.read_only,
                    "requires_project": m.requires_project,
                    "path_hint": path_hint_str(m.path_hint),
                    "destructive": m.destructive,
                    "shell_like": m.shell_like,
                    "oauth_scope": m.oauth_scope,
                })
            })
            .collect();

        // Build the categories map from the full tool set so the caller can
        // always see valid categories even when filtering.
        let categories = build_manifest_categories(&all_tools);

        // Apply the optional category filter and startup limit.
        let filtered_tools: Vec<Value> = match &categories_requested {
            Some(requested) => all_tools
                .iter()
                .filter(|t| {
                    t["category"].as_str().is_some_and(|category| {
                        requested.iter().any(|requested| requested == category)
                    })
                })
                .cloned()
                .collect(),
            None => all_tools,
        };
        let filtered_count = filtered_tools.len();
        let limit = limit.map(|limit| limit.clamp(1, 100));
        let truncated = limit.is_some_and(|limit| filtered_count > limit);
        let tools: Vec<Value> = match limit {
            Some(limit) => filtered_tools.into_iter().take(limit).collect(),
            None => filtered_tools,
        };

        let mut output = json!({
            "schema_version": 1,
            "tool_count": tool_count,
            "count": tools.len(),
            "filtered_count": filtered_count,
            "category": category,
            "filtered": categories_requested.is_some() || limit.is_some(),
            "categories_requested": categories_requested,
            "limit": limit,
            "truncated": truncated,
            "categories": categories,
            "tools": tools,
        });

        if include_risk_summary {
            output["risk_summary"] =
                build_risk_summary(output["tools"].as_array().unwrap_or(&Vec::new()));
        }

        if include_recommended_flows {
            output["recommended_flows"] = Value::Array(tool_manifest_recommended_flows());
        }

        output
    }
}

pub(super) fn list_tools_filtered_indexes(
    specs: &[ToolSpec],
    options: &ListToolsOptions,
) -> Vec<usize> {
    specs
        .iter()
        .enumerate()
        .filter(|(_, spec)| {
            let name = spec.name.as_str();
            options
                .category
                .as_deref()
                .map(|category| runtime_tool_category(name) == category)
                .unwrap_or(true)
                && options
                    .features
                    .as_deref()
                    .map(|features| list_tool_matches_features(name, features))
                    .unwrap_or(true)
        })
        .map(|(index, _)| index)
        .collect()
}

pub(super) fn normalize_tool_manifest_categories(
    categories: Option<Vec<String>>,
) -> Option<Vec<String>> {
    let mut out = Vec::new();
    for category in categories.unwrap_or_default() {
        let category = category.trim();
        if category.is_empty() || out.iter().any(|existing| existing == category) {
            continue;
        }
        out.push(category.to_string());
    }
    (!out.is_empty()).then_some(out)
}

pub(super) fn build_list_tools_summary_entries(specs: &[ToolSpec]) -> Vec<Value> {
    specs
        .iter()
        .map(|spec| {
            let name = spec.name.as_str();
            let m = runtime_tool_metadata(name);
            json!({
                "name": name,
                "description": spec.description,
                "category": runtime_tool_category(name),
                "risk": m.risk.session_risk_class(),
                "read_only": m.read_only,
                "requires_project": m.requires_project,
                "annotations": spec.annotations,
            })
        })
        .collect()
}

fn list_tool_matches_features(name: &str, features: &str) -> bool {
    features
        .split(|c: char| c == ',' || c.is_ascii_whitespace())
        .filter_map(normalize_feature)
        .any(|feature| list_tool_matches_feature(name, feature.as_str()))
}

fn normalize_feature(feature: &str) -> Option<String> {
    let normalized = feature.trim().to_ascii_lowercase().replace('-', "_");
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn list_tool_matches_feature(name: &str, feature: &str) -> bool {
    let category = runtime_tool_category(name);
    if category == feature {
        return true;
    }
    match feature {
        "artifact" => category == "artifact",
        "artifact_upload" | "upload" => name.starts_with("artifact_upload_"),
        "read" => {
            runtime_tool_metadata(name).read_only
                || name.starts_with("read_")
                || name.contains("_read_")
        }
        "edit" => matches!(category, "edit" | "patch"),
        "session" => category == "session",
        "git" => category == "git",
        "validation" => category == "validation",
        "runtime" => category == "runtime",
        other => name.contains(other),
    }
}

/// String representation of a `ToolPathHint` for the compact manifest.
pub(super) fn path_hint_str(hint: ToolPathHint) -> &'static str {
    match hint {
        ToolPathHint::None => "none",
        ToolPathHint::SinglePath => "single_path",
        ToolPathHint::PathList => "path_list",
        ToolPathHint::Patch => "patch",
        ToolPathHint::Artifact => "artifact",
    }
}

/// Build the categories map from the compact tool entries. Each category
/// maps to a sorted list of tool names.
pub(super) fn build_manifest_categories(tools: &[Value]) -> Value {
    let mut map: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for tool in tools {
        let name = tool["name"].as_str().unwrap_or("");
        let category = tool["category"].as_str().unwrap_or("other");
        map.entry(category).or_default().push(name.to_string());
    }
    let result: serde_json::Map<String, Value> = map
        .into_iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                Value::Array(v.into_iter().map(Value::String).collect()),
            )
        })
        .collect();
    Value::Object(result)
}

/// Build the risk summary map from the compact tool entries.
pub(super) fn build_risk_summary(tools: &[Value]) -> Value {
    let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
    for tool in tools {
        let risk = tool["risk"].as_str().unwrap_or("unknown");
        *counts.entry(risk).or_insert(0) += 1;
    }
    let result: serde_json::Map<String, Value> = counts
        .into_iter()
        .map(|(k, v)| (k.to_string(), Value::from(v)))
        .collect();
    Value::Object(result)
}

/// Short, bounded list of recommended tool flows for common tasks. Each
/// entry references only known tool names. Kept under 10 entries.
pub(super) fn tool_manifest_recommended_flows() -> Vec<Value> {
    TOOL_RECOMMENDED_FLOWS
        .iter()
        .map(|flow| {
            json!({
                "name": flow.name,
                "purpose": flow.manifest_purpose,
                "tools": flow.tools,
            })
        })
        .collect()
}
