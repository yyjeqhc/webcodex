use crate::json_error;
use salvo::prelude::*;

fn public_url() -> String {
    std::env::var("DROP_PUBLIC_URL")
        .ok()
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "http://localhost:8080".to_string())
}

/// GPT Actions OpenAPI endpoint.
///
/// Returns a clean OpenAPI 3.1 spec exposing only the operations needed for
/// GPT Actions integration. Both GPT Actions and (future) MCP over HTTP call
/// the same underlying tool runtime.
#[handler]
pub async fn openapi_json(res: &mut Response) {
    let spec = match serde_json::from_str::<serde_json::Value>(include_str!("../data/openapi.json"))
    {
        Ok(s) => s,
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Invalid OpenAPI schema: {}", e),
            ));
            return;
        }
    };

    // GPT Actions operationIds to keep.
    let keep_ops: &[&str] = &[
        "listProjects",
        "getProjectContext",
        "getProjectContextBatch",
        "applyProjectPatch",
        "applyProjectEdit",
        "runProjectGit",
        "runShell",
        "runShellJob",
        "getShellClientJobStatus",
        "getShellClientJobLog",
        "stopShellClientJob",
        "listShellClientJobs",
        "shellFileOp",
        "runJobOp",
        "healthCheck",
    ];

    let keep_set: std::collections::HashSet<&str> = keep_ops.iter().copied().collect();

    // Filter paths to only kept operations.
    let mut filtered_paths = serde_json::Map::new();
    if let Some(paths) = spec["paths"].as_object() {
        for (path, methods) in paths {
            let mut filtered_methods = serde_json::Map::new();
            let Some(methods_map) = methods.as_object() else {
                continue;
            };
            for (method, op) in methods_map {
                if let Some(op_id) = op["operationId"].as_str() {
                    if keep_set.contains(op_id) {
                        filtered_methods.insert(method.clone(), op.clone());
                    }
                }
            }
            if !filtered_methods.is_empty() {
                filtered_paths.insert(path.clone(), serde_json::Value::Object(filtered_methods));
            }
        }
    }

    // Filter schemas to only those referenced by kept paths.
    let mut used_schemas = std::collections::HashSet::new();
    collect_schema_refs_from_value(&spec["paths"], &mut used_schemas);
    // Also keep the schemas that appear in our filtered paths.
    let filtered_paths_value = serde_json::Value::Object(filtered_paths.clone());
    let mut filtered_refs = std::collections::HashSet::new();
    collect_schema_refs_from_value(&filtered_paths_value, &mut filtered_refs);

    let mut filtered_schemas = serde_json::Map::new();
    if let Some(schemas) = spec["components"]["schemas"].as_object() {
        for (name, schema) in schemas {
            if filtered_refs.contains(name.as_str()) || is_base_schema(name) {
                filtered_schemas.insert(name.clone(), schema.clone());
            }
        }
    }

    let mut result = serde_json::json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Private Drop — GPT Actions API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "GPT Actions integration for Private Drop tool runtime. Exposes project context, file operations, git, shell execution, and job management."
        },
        "servers": [{ "url": public_url(), "description": "Server" }],
        "paths": filtered_paths,
        "components": {
            "schemas": filtered_schemas,
            "securitySchemes": {
                "bearerAuth": {
                    "type": "http",
                    "scheme": "bearer"
                }
            }
        },
        "security": [{ "bearerAuth": [] }]
    });

    // Remove project enum from schemas — projects are runtime-configured.
    apply_project_description(&mut result);

    res.render(Json(result));
}

const PROJECT_DESC: &str =
    "Runtime-validated project name. Configure projects in projects.toml and restart.";

fn apply_project_description(spec: &mut serde_json::Value) {
    let schemas = [
        "ContextRequest",
        "ContextBatchRequest",
        "PatchRequest",
        "EditRequest",
        "GitRequest",
        "CheckRequest",
        "ReportRequest",
        "JobOpRequest",
    ];
    for name in &schemas {
        if let Some(project) = spec["components"]["schemas"][*name]["properties"].get_mut("project")
        {
            if let Some(obj) = project.as_object_mut() {
                obj.remove("enum");
                obj.insert("description".to_string(), serde_json::json!(PROJECT_DESC));
            }
        }
    }
}

fn is_base_schema(name: &str) -> bool {
    matches!(
        name,
        "ContextRequest"
            | "ContextResponse"
            | "ContextBatchItem"
            | "ContextBatchRequest"
            | "ContextBatchResultMetadata"
            | "ContextBatchResponse"
            | "PatchRequest"
            | "PatchResponse"
            | "EditRequest"
            | "EditResponse"
            | "EditPostCheckResult"
            | "GitRequest"
            | "GitResponse"
            | "CheckRequest"
            | "CheckResponse"
            | "ReportRequest"
            | "ReportResponse"
            | "ProjectsResponse"
            | "ProjectCapabilities"
            | "ProjectCapabilityInfo"
            | "InstanceInfo"
            | "JobOpRequest"
            | "JobInfo"
            | "JobOpResponse"
            | "ReplaceTextEdit"
            | "ReplaceRangeEdit"
            | "AppendFileEdit"
            | "CreateFileEdit"
            | "WriteFileEdit"
    )
}

/// Recursively collect schema names referenced via `$ref`.
fn collect_schema_refs_from_value(
    value: &serde_json::Value,
    out: &mut std::collections::HashSet<String>,
) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(reference) = map.get("$ref").and_then(|v| v.as_str()) {
                // Extract schema name from "#/components/schemas/Name"
                if let Some(name) = reference.strip_prefix("#/components/schemas/") {
                    out.insert(name.to_string());
                }
            }
            for v in map.values() {
                collect_schema_refs_from_value(v, out);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                collect_schema_refs_from_value(v, out);
            }
        }
        _ => {}
    }
}
