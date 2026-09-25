use super::project_resolution::ResolvedProject;
use super::startup_brief::{
    builtin_coding_workflow_projection, project_instructions_context_projection,
};
use super::tool_inputs::CodingGuidanceProfile;
use super::{ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use crate::json_measurement::serialized_json_len;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashSet;

pub(crate) const TOOL_CALL_CONTEXT_REQUEST_FIELD: &str = "context_request";
pub(crate) const MAX_CONTEXT_REQUEST_ITEMS: usize = 8;
pub(crate) const MAX_CONTEXT_REQUEST_KEY_CHARS: usize = 64;
pub(crate) const MAX_CONTEXT_PROJECTION_BYTES: usize = 20 * 1024;
const PLUGIN_CATALOG_SCOPES: &[&str] = &[
    crate::auth::SCOPE_PROJECT_READ,
    crate::auth::SCOPE_PLUGIN_INSPECT,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContextMaterialScopePolicy {
    Public,
    Require(&'static str),
    RequireAll(&'static [&'static str]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContextMaterialSurface {
    AnySidecar,
    SkillRuntime,
    MemorySurface,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ContextMaterialSpec {
    pub(crate) key: &'static str,
    pub(crate) project_required: bool,
    pub(crate) scope_policy: ContextMaterialScopePolicy,
    pub(crate) surface: ContextMaterialSurface,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ContextMaterialCapabilities {
    pub(crate) skill_runtime: bool,
    pub(crate) memory_surface: bool,
}

pub(crate) const CONTEXT_MATERIAL_SPECS: &[ContextMaterialSpec] = &[
    ContextMaterialSpec {
        key: "project.instructions",
        project_required: true,
        scope_policy: ContextMaterialScopePolicy::Require(crate::auth::SCOPE_PROJECT_READ),
        surface: ContextMaterialSurface::AnySidecar,
    },
    ContextMaterialSpec {
        key: "webcodex.workflow",
        project_required: false,
        scope_policy: ContextMaterialScopePolicy::Public,
        surface: ContextMaterialSurface::AnySidecar,
    },
    ContextMaterialSpec {
        key: "workflow.resume",
        project_required: false,
        scope_policy: ContextMaterialScopePolicy::Public,
        surface: ContextMaterialSurface::AnySidecar,
    },
    ContextMaterialSpec {
        key: "jobs.attention",
        project_required: true,
        scope_policy: ContextMaterialScopePolicy::Require(crate::auth::SCOPE_RUNTIME_READ),
        surface: ContextMaterialSurface::AnySidecar,
    },
    ContextMaterialSpec {
        key: "skills.catalog",
        project_required: true,
        scope_policy: ContextMaterialScopePolicy::Require(crate::auth::SCOPE_PROJECT_READ),
        surface: ContextMaterialSurface::SkillRuntime,
    },
    ContextMaterialSpec {
        key: "plugins.catalog",
        project_required: true,
        scope_policy: ContextMaterialScopePolicy::RequireAll(PLUGIN_CATALOG_SCOPES),
        surface: ContextMaterialSurface::AnySidecar,
    },
    ContextMaterialSpec {
        key: "memory.bootstrap",
        project_required: true,
        scope_policy: ContextMaterialScopePolicy::RequireAll(
            webcodex_core::authority::MEMORY_READ_SCOPES,
        ),
        surface: ContextMaterialSurface::MemorySurface,
    },
];

pub(crate) fn context_material_keys_csv() -> String {
    CONTEXT_MATERIAL_SPECS
        .iter()
        .map(|spec| spec.key)
        .collect::<Vec<_>>()
        .join(", ")
}

fn context_material_spec(key: &str) -> Option<&'static ContextMaterialSpec> {
    CONTEXT_MATERIAL_SPECS.iter().find(|spec| spec.key == key)
}

fn context_material_surface_available(
    surface: ContextMaterialSurface,
    capabilities: ContextMaterialCapabilities,
) -> bool {
    match surface {
        ContextMaterialSurface::AnySidecar => true,
        ContextMaterialSurface::SkillRuntime => capabilities.skill_runtime,
        ContextMaterialSurface::MemorySurface => capabilities.memory_surface,
    }
}

fn context_material_scope_available(
    policy: ContextMaterialScopePolicy,
    auth: Option<&AuthContext>,
) -> bool {
    match policy {
        ContextMaterialScopePolicy::Public => true,
        ContextMaterialScopePolicy::Require(scope) => {
            auth.is_some_and(|auth| auth.has_scope(scope))
        }
        ContextMaterialScopePolicy::RequireAll(scopes) => {
            auth.is_some_and(|auth| scopes.iter().all(|scope| auth.has_scope(scope)))
        }
    }
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

fn scope_unavailable_reason(key: &str) -> &'static str {
    if key == "plugins.catalog" {
        "plugin_inspect_scope_unavailable"
    } else {
        "context_material_scope_unavailable"
    }
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
            let material = if let Some(spec) = context_material_spec(key) {
                if !context_material_surface_available(spec.surface, capabilities) {
                    unavailable(key, "context_material_surface_unavailable")
                } else if spec.project_required && resolved_project.is_none() {
                    unavailable(key, "project_target_unavailable")
                } else if !context_material_scope_available(spec.scope_policy, auth) {
                    unavailable(key, scope_unavailable_reason(key))
                } else {
                    match key {
                        "project.instructions" => {
                            let project =
                                resolved_project.expect("registry requires project target");
                            let snapshot =
                                self.load_effective_coding_instructions(project, auth).await;
                            let mut material = if snapshot.scan_complete {
                                json!({"key": key, "status": "available", "projection": null})
                            } else {
                                json!({
                                    "key": key,
                                    "status": "unavailable",
                                    "reason_code": "project_instructions_observation_incomplete",
                                    "projection": null,
                                })
                            };
                            // Measure the complete prospective envelope, including
                            // earlier materials and the unavailable-reason overhead.
                            materials.push(material.clone());
                            let reserved = serialized_json_len(&ContextProjectionMeasure {
                                materials: &materials,
                                truncated,
                            })
                            .unwrap_or(usize::MAX)
                            .saturating_sub(4); // Replace the literal JSON null.
                            materials.pop();
                            material["projection"] = project_instructions_context_projection(
                                &snapshot,
                                MAX_CONTEXT_PROJECTION_BYTES.saturating_sub(reserved),
                            );
                            material
                        }
                        "jobs.attention" => {
                            let project =
                                resolved_project.expect("registry requires project target");
                            // Project-level attention only. Recorder/ambient Sessions never
                            // select a business Session or grant Job inventory authority.
                            let projection = Box::pin(self.active_jobs_summary(
                                Some(&project.resolved_id),
                                None,
                                auth,
                                8,
                            ))
                            .await;
                            json!({
                                "key": key,
                                "status": "available",
                                "projection": projection,
                            })
                        }
                        "workflow.resume" => {
                            match self.workflow_resume_context_projection(window, auth).await {
                                Ok(projection) => json!({
                                    "key": key,
                                    "status": "available",
                                    "projection": projection,
                                }),
                                Err(reason_code) => unavailable(key, reason_code),
                            }
                        }
                        "skills.catalog" => {
                            let project =
                                resolved_project.expect("registry requires project target");
                            match self.skills_catalog_context_projection(project, auth).await {
                                Ok(projection) => json!({
                                    "key": key,
                                    "status": "available",
                                    "projection": projection,
                                }),
                                Err(reason_code) => unavailable(key, reason_code),
                            }
                        }
                        "plugins.catalog" => {
                            let project =
                                resolved_project.expect("registry requires project target");
                            match self
                                .plugin_project_catalog_context_projection(project, auth)
                                .await
                            {
                                Ok(projection) => json!({
                                    "key": key,
                                    "status": "available",
                                    "projection": projection,
                                }),
                                Err(reason_code) => unavailable(key, reason_code),
                            }
                        }
                        "memory.bootstrap" => {
                            let project =
                                resolved_project.expect("registry requires project target");
                            match self.memory_bootstrap_context_projection(project) {
                                Ok(projection) => json!({
                                    "key": key,
                                    "status": "available",
                                    "projection": projection,
                                }),
                                Err(reason_code) => unavailable(key, reason_code),
                            }
                        }
                        "webcodex.workflow" => json!({
                            "key": key,
                            "status": "available",
                            "projection": builtin_coding_workflow_projection(guidance_profile),
                        }),
                        _ => unreachable!("context material registry/provider match drifted"),
                    }
                }
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
                    .authorize_session_target(&session_id, "session_handoff_summary", auth)
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
            candidates.push(json!({
                "session_id": session_id,
                "project": project,
                "lifecycle": "active",
                "title": title,
                "relations": link.relations,
                "last_linked_at_ms": link.last_linked_at_ms,
            }));
        }

        let mut projection = json!({
            "candidates": candidates,
            "count": candidates.len(),
            "truncated": candidates_truncated,
            "selection": "caller_must_choose_exact_session",
        });
        if candidates.len() == 1 {
            projection["suggested_call"] = json!({
                "tool": "session_handoff_summary",
                "arguments": {"session_id": candidates[0]["session_id"]},
            });
        }
        Ok(projection)
    }
}
