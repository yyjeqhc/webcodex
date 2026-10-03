//! Explicit, independently bounded context sidecars. Material registration owns
//! discovery and providers; this layer owns request order and the shared budget.

mod providers;
mod registry;

use super::project_resolution::ResolvedProject;
use super::startup_brief::builtin_coding_workflow_projection_with_policy;
use super::tool_inputs::CodingGuidanceProfile;
use super::{SuggestedToolCall, ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use crate::json_measurement::serialized_json_len;
use registry::{ContextMaterialContext, ContextProjectionBudget, BUILTIN_CONTEXT_MATERIALS};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashSet;

pub(crate) const TOOL_CALL_CONTEXT_REQUEST_FIELD: &str = "context_request";
pub(crate) const MAX_CONTEXT_REQUEST_ITEMS: usize = 8;
pub(crate) const MAX_CONTEXT_REQUEST_KEY_CHARS: usize = 64;
pub(crate) const MAX_CONTEXT_PROJECTION_BYTES: usize = 20 * 1024;
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ContextMaterialCapabilities {
    pub(crate) skill_runtime: bool,
    pub(crate) memory_surface: bool,
}

pub(crate) fn context_material_keys_csv() -> String {
    BUILTIN_CONTEXT_MATERIALS
        .advertised_keys()
        .collect::<Vec<_>>()
        .join(", ")
}

fn projection_envelope(materials: Vec<Value>, truncated: bool) -> Value {
    json!({
        "materials": materials,
        "truncated": truncated,
    })
}

#[derive(Serialize)]
struct ContextProjectionMeasure<'a> {
    materials: &'a [Value],
    truncated: bool,
}

fn fits_projection_budget(materials: &[Value], truncated: bool) -> bool {
    serialized_json_len(&ContextProjectionMeasure {
        materials,
        truncated,
    })
    .map(|bytes| bytes <= MAX_CONTEXT_PROJECTION_BYTES)
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
    #[cfg(test)]
    pub(crate) async fn add_requested_context_projection(
        &self,
        result: &mut ToolResult,
        requested: &[String],
        resolved_project: Option<&ResolvedProject>,
        auth: Option<&AuthContext>,
        capabilities: ContextMaterialCapabilities,
    ) {
        self.add_requested_context_projection_with_guidance(
            result,
            requested,
            resolved_project,
            auth,
            capabilities,
            CodingGuidanceProfile::default(),
            None,
            None,
        )
        .await;
    }

    pub(crate) async fn add_requested_context_projection_with_guidance(
        &self,
        result: &mut ToolResult,
        requested: &[String],
        resolved_project: Option<&ResolvedProject>,
        auth: Option<&AuthContext>,
        capabilities: ContextMaterialCapabilities,
        guidance_profile: CodingGuidanceProfile,
        window: Option<&crate::client_window::ClientWindow>,
        instructions: Option<&super::project_instructions::ProjectInstructionsSnapshot>,
    ) {
        if requested.is_empty() {
            return;
        }

        let mut seen = HashSet::new();
        let mut materials = Vec::new();
        let mut truncated = false;
        let requested: Vec<_> = requested
            .iter()
            .map(|key| key.trim())
            .filter(|key| !key.is_empty())
            .filter(|key| seen.insert((*key).to_string()))
            .take(MAX_CONTEXT_REQUEST_ITEMS)
            .collect();
        // Reserve the canonical public workflow before shortening an earlier
        // instruction body; never reorder, reload, or silently discard rules.
        let workflow = requested.contains(&providers::WORKFLOW.key).then(|| json!({
            "key":providers::WORKFLOW.key, "status":"available",
            "projection":builtin_coding_workflow_projection_with_policy(guidance_profile, self.model_workflow_policy),
        }));
        for (index, key) in requested.iter().copied().enumerate() {
            let material = if let Some(provider) = BUILTIN_CONTEXT_MATERIALS.get(key) {
                provider
                    .project(
                        ContextMaterialContext {
                            runtime: self,
                            key: provider.key,
                            project: resolved_project,
                            auth,
                            window,
                            instructions,
                            budget: ContextProjectionBudget {
                                preceding: &materials,
                                remaining: &requested[index + 1..],
                                workflow: workflow.as_ref(),
                                truncated,
                            },
                        },
                        capabilities,
                    )
                    .await
            } else {
                json!({
                    "key": key,
                    "status": "unsupported",
                    "reason_code": "unsupported_context_material",
                })
            };

            materials.push(material);
            if fits_projection_budget(&materials, truncated) {
                continue;
            }
            materials.pop();

            truncated = true;
            let bounded = unavailable(key, "context_projection_budget_exceeded");
            materials.push(bounded);
            if !fits_projection_budget(&materials, true) {
                materials.pop();
            }
        }

        let projection = projection_envelope(materials, truncated);
        debug_assert!(
            serialized_json_len(&projection)
                .map(|bytes| bytes <= MAX_CONTEXT_PROJECTION_BYTES)
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

    #[cfg(test)]
    pub(crate) async fn workflow_resume_context_projection_for_test(
        &self,
        window: Option<&crate::client_window::ClientWindow>,
        auth: Option<&AuthContext>,
    ) -> Result<Value, &'static str> {
        self.workflow_resume_context_projection(window, auth).await
    }

    async fn workflow_resume_context_projection(
        &self,
        window: Option<&crate::client_window::ClientWindow>,
        auth: Option<&AuthContext>,
    ) -> Result<Value, &'static str> {
        const MAX_CANDIDATES: usize = 8;
        const SCAN_LIMIT: usize = 100;
        const MAX_TITLE_CHARS: usize = 240;

        let window = window.ok_or("client_window_unavailable")?;
        let db = self
            .window_activity_db
            .as_ref()
            .ok_or("window_activity_unavailable")?;
        let (principal_kind, principal_id) =
            super::session_context::runtime_observation_principal(auth)
                .map_err(|_| "principal_unavailable")?;
        let linked = db
            .list_window_workflow_sessions(
                window.key(),
                Some((&principal_kind, &principal_id)),
                SCAN_LIMIT,
            )
            .map_err(|_| "window_activity_unavailable")?;

        let scan_truncated = linked.len() >= SCAN_LIMIT;
        let mut candidates = Vec::new();
        let mut candidates_truncated = scan_truncated;
        for link in linked {
            let session_id = link.workflow_session_id;
            if self.sessions.lifecycle_state(&session_id)
                != Some(super::sessions::SessionLifecycle::Active)
            {
                continue;
            }
            let Some(project) = self.sessions.session_project(&session_id).flatten() else {
                continue;
            };
            if link.project.as_deref() != Some(project.as_str()) {
                continue;
            }
            let Ok(resolved) = self.resolve_project_input_for_auth(&project, auth).await else {
                continue;
            };
            if resolved.resolved_id != project
                || self
                    .authorize_session_target(&session_id, "read_session_handoff", auth)
                    .await
                    .is_err()
            {
                continue;
            }
            let Some(summary) = self.sessions.summary(&session_id, Some(1)) else {
                continue;
            };
            let title = summary
                .title
                .map(|title| title.chars().take(MAX_TITLE_CHARS).collect::<String>());
            if candidates.len() >= MAX_CANDIDATES {
                candidates_truncated = true;
                break;
            }
            let session_ref = self.session_reference_for_id(&session_id, auth);
            let mut candidate = json!({
                "session_id": session_id,
                "project": project,
                "lifecycle": "active",
                "title": title,
                "relations": link.relations,
                "last_linked_at_ms": link.last_linked_at_ms,
            });
            if let Some(session_ref) = session_ref {
                candidate["session_ref"] = json!(session_ref);
            }
            if let Some(project_ref) = self.project_reference_for_resolved(&resolved, auth) {
                candidate["project_ref"] = json!(project_ref);
            }
            candidates.push(candidate);
        }

        let mut projection = json!({
            "candidates": candidates,
            "count": candidates.len(),
            "truncated": candidates_truncated,
            "selection": "caller_must_choose_exact_session",
        });
        if candidates.len() == 1 {
            let session_selector = candidates[0]
                .get("session_ref")
                .unwrap_or(&candidates[0]["session_id"]);
            projection["suggested_call"] = SuggestedToolCall::fallback_recovery(
                "read_session_handoff",
                json!({"session_id": session_selector}),
            )
            .to_value();
        }
        Ok(projection)
    }
}
