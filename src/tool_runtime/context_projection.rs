use super::project_resolution::ResolvedProject;
use super::startup_brief::{
    builtin_coding_workflow_projection, project_instructions_context_projection,
};
use super::{ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use serde_json::{json, Value};
use std::collections::HashSet;

pub(crate) const TOOL_CALL_CONTEXT_REQUEST_FIELD: &str = "context_request";
pub(crate) const TOOL_CALL_CONTEXT_REQUEST_INTERNAL_FIELD: &str =
    "__webcodex_stateless_context_request";
pub(crate) const MAX_CONTEXT_REQUEST_ITEMS: usize = 8;
pub(crate) const MAX_CONTEXT_REQUEST_KEY_CHARS: usize = 64;
pub(crate) const MAX_CONTEXT_PROJECTION_BYTES: usize = 20 * 1024;

pub(crate) fn context_request_from_arguments(arguments: &Value) -> Vec<String> {
    let Some(values) = arguments
        .as_object()
        .and_then(|object| object.get(TOOL_CALL_CONTEXT_REQUEST_INTERNAL_FIELD))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    values
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .filter(|key| seen.insert((*key).to_string()))
        .take(MAX_CONTEXT_REQUEST_ITEMS)
        .map(str::to_string)
        .collect()
}

fn projection_envelope(materials: Vec<Value>, truncated: bool) -> Value {
    json!({
        "timing": "post_tool",
        "applies_to_current_effect": false,
        "materials": materials,
        "truncated": truncated,
    })
}

fn fits_projection_budget(materials: &[Value], truncated: bool) -> bool {
    serde_json::to_vec(&projection_envelope(materials.to_vec(), truncated))
        .map(|bytes| bytes.len() <= MAX_CONTEXT_PROJECTION_BYTES)
        .unwrap_or(false)
}

fn unavailable(key: &str, reason_code: &str) -> Value {
    json!({
        "key": key,
        "status": "unavailable",
        "reason_code": reason_code,
    })
}

impl ToolRuntime {
    pub(crate) async fn add_requested_context_projection(
        &self,
        result: &mut ToolResult,
        requested: &[String],
        resolved_project: Option<&ResolvedProject>,
        auth: Option<&AuthContext>,
    ) {
        if requested.is_empty() {
            return;
        }

        let mut seen = HashSet::new();
        let mut materials = Vec::new();
        let mut truncated = false;
        for key in requested
            .iter()
            .map(|key| key.trim())
            .filter(|key| !key.is_empty())
            .filter(|key| seen.insert((*key).to_string()))
            .take(MAX_CONTEXT_REQUEST_ITEMS)
        {
            let material = match key {
                "project.instructions" => match resolved_project {
                    Some(project) => {
                        let snapshot = self.load_coding_project_instructions(&project.config).await;
                        let projection = project_instructions_context_projection(&snapshot);
                        if snapshot.scan_complete {
                            json!({
                                "key": key,
                                "status": "available",
                                "projection": projection,
                            })
                        } else {
                            json!({
                                "key": key,
                                "status": "unavailable",
                                "reason_code": "project_instructions_observation_incomplete",
                                "projection": projection,
                            })
                        }
                    }
                    None => unavailable(key, "project_target_unavailable"),
                },
                "skills.catalog" => match resolved_project {
                    Some(project) => {
                        match self.skills_catalog_context_projection(project, auth).await {
                            Ok(projection) => json!({
                                "key": key,
                                "status": "available",
                                "projection": projection,
                            }),
                            Err(reason_code) => unavailable(key, reason_code),
                        }
                    }
                    None => unavailable(key, "project_target_unavailable"),
                },
                "webcodex.workflow" => json!({
                    "key": key,
                    "status": "available",
                    "projection": builtin_coding_workflow_projection(),
                }),
                _ => json!({
                    "key": key,
                    "status": "unsupported",
                    "reason_code": "unsupported_context_material",
                }),
            };

            let mut candidate = materials.clone();
            candidate.push(material.clone());
            if fits_projection_budget(&candidate, truncated) {
                materials.push(material);
                continue;
            }

            truncated = true;
            let bounded = unavailable(key, "context_projection_budget_exceeded");
            let mut bounded_candidate = materials.clone();
            bounded_candidate.push(bounded.clone());
            if fits_projection_budget(&bounded_candidate, true) {
                materials.push(bounded);
            }
        }

        let projection = projection_envelope(materials, truncated);
        debug_assert!(
            serde_json::to_vec(&projection)
                .map(|bytes| bytes.len() <= MAX_CONTEXT_PROJECTION_BYTES)
                .unwrap_or(false),
            "context projection must stay inside its independent budget"
        );
        let mut output = match std::mem::take(&mut result.output) {
            Value::Object(output) => output,
            other => {
                let mut output = serde_json::Map::new();
                output.insert("value".to_string(), other);
                output
            }
        };
        output.insert("context_projection".to_string(), projection);
        result.output = Value::Object(output);
    }
}
