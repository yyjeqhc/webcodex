//! Admin dashboard snapshot over canonical typed runtime state.
//!
//! This query deliberately bypasses model-facing ToolResult projections. The
//! HTTP adapter receives one typed DTO and never reinterprets runtime JSON as
//! Runner, Project, Job, or compatibility business semantics.

use super::projects::{project_git_available, resolve_project_shell_profile};
use super::runtime_compatibility::{
    build_alignment_name, current_compatibility_facts, protocol_compatibility_name,
    RunnerCompatibilityFacts,
};
use super::runtime_info::{connection_layers, version_compatibility};
use super::{runner_project_runtime_id, ToolRuntime};
use crate::auth::AuthContext;
use serde_json::Value;
use std::collections::HashMap;
use webcodex_core::desktop_runtime_contract::{DesktopRuntimeContract, DESKTOP_RUNTIME_CONTRACT};
use webcodex_core::runner_protocol::RunnerCapabilityId;

/// Section results retain the existing fail-closed HTTP envelope even though
/// the current in-memory registry queries are infallible.
#[derive(Debug, Clone)]
pub(crate) struct AdminDashboardSnapshot {
    pub(crate) overview: Result<AdminDashboardOverview, ()>,
    pub(crate) devices: Result<Vec<AdminDashboardDevice>, ()>,
    pub(crate) projects: Result<Vec<AdminDashboardProject>, ()>,
}

#[derive(Debug, Clone)]
pub(crate) struct AdminDashboardOverview {
    pub(crate) version: String,
    pub(crate) build_commit: Option<String>,
    pub(crate) authority_mode: String,
    pub(crate) runners_total: usize,
    pub(crate) runners_online: usize,
    pub(crate) projects_total: usize,
    pub(crate) projects_online: usize,
    pub(crate) active_jobs: usize,
    pub(crate) version_compatibility: String,
    pub(crate) protocol_compatibility: String,
    pub(crate) build_alignment: String,
    pub(crate) desktop_runtime_contract: DesktopRuntimeContract,
    pub(crate) diagnostics: AdminDashboardDiagnostics,
}

#[derive(Debug, Clone)]
pub(crate) struct AdminDashboardDiagnostics {
    pub(crate) runner_process: Value,
    pub(crate) server_transport: Value,
    pub(crate) server_registration: Value,
    pub(crate) project_registry: Value,
    pub(crate) version_compatibility: Value,
}

#[derive(Debug, Clone)]
pub(crate) struct AdminDashboardDevice {
    pub(crate) display_name: Option<String>,
    pub(crate) client_id: String,
    pub(crate) status: String,
    pub(crate) transport: String,
    pub(crate) hostname: Option<String>,
    pub(crate) last_seen: i64,
    pub(crate) capabilities: Vec<String>,
    pub(crate) project_count: usize,
    pub(crate) active_jobs: usize,
    pub(crate) runner_protocol_generation: u16,
    pub(crate) compatibility: String,
    pub(crate) protocol_compatibility: String,
    pub(crate) build_alignment: String,
}

#[derive(Debug, Clone)]
pub(crate) struct AdminDashboardProject {
    pub(crate) id: String,
    pub(crate) name: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) client_id: String,
    pub(crate) path: String,
    pub(crate) connected: bool,
    pub(crate) git_available: Option<bool>,
    pub(crate) allow_patch: bool,
    pub(crate) enabled: bool,
    pub(crate) revision: Option<String>,
    pub(crate) active_jobs: usize,
    pub(crate) shell_profile_status: String,
    pub(crate) compatibility: String,
    pub(crate) protocol_compatibility: String,
    pub(crate) build_alignment: String,
}

fn enabled_capabilities(client: &crate::runner_protocol::RunnerView) -> Vec<String> {
    let mut names = RunnerCapabilityId::all()
        .iter()
        .copied()
        .filter(|capability| client.capabilities.supports(*capability))
        .map(|capability| capability.as_wire_name().to_string())
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn compatibility_by_client<'a>(
    facts: &'a [RunnerCompatibilityFacts],
) -> HashMap<&'a str, &'a RunnerCompatibilityFacts> {
    facts
        .iter()
        .map(|facts| (facts.client_id.as_str(), facts))
        .collect()
}

impl ToolRuntime {
    pub(crate) async fn admin_dashboard_snapshot(
        &self,
        auth: &AuthContext,
    ) -> AdminDashboardSnapshot {
        let access = crate::runner_http::runner_access_from_auth(Some(auth));
        let clients = self
            .runner_registry
            .list_runners_for_auth(access.as_ref())
            .await;
        let project_ids = clients
            .iter()
            .flat_map(|client| {
                client.projects.iter().map(|project| {
                    runner_project_runtime_id(&client.client_id, project.id.as_str())
                })
            })
            .collect::<Vec<_>>();
        let project_id_refs = project_ids.iter().map(String::as_str).collect::<Vec<_>>();
        let (active_by_runner, active_by_project) = tokio::join!(
            self.runner_registry
                .active_job_summary_by_runner_for_auth(access.as_ref()),
            self.runner_registry
                .count_active_jobs_for_projects(access.as_ref(), &project_id_refs),
        );

        let compatibility = current_compatibility_facts(&clients);
        let compatibility_by_client = compatibility_by_client(&compatibility.runners);
        let projects_total = clients
            .iter()
            .flat_map(|client| &client.projects)
            .filter(|project| !project.disabled)
            .count();
        let projects_online = clients
            .iter()
            .filter(|client| client.connected)
            .flat_map(|client| &client.projects)
            .filter(|project| !project.disabled)
            .count();
        let runners_online = clients.iter().filter(|client| client.connected).count();
        let active_jobs = active_by_runner.values().map(|jobs| jobs.active).sum();
        let now = chrono::Utc::now().timestamp();
        let layers = connection_layers(
            &clients,
            projects_total,
            projects_online,
            self.observations.as_ref(),
            Some(auth),
            now,
        );
        let build = crate::build_info::runtime_build_info();
        let overview = AdminDashboardOverview {
            version: env!("CARGO_PKG_VERSION").to_string(),
            build_commit: build.git_commit.map(str::to_string),
            authority_mode: self.permission_evaluator.mode_name().to_string(),
            runners_total: clients.len(),
            runners_online,
            projects_total,
            projects_online,
            active_jobs,
            version_compatibility: compatibility.status.status_name().to_string(),
            protocol_compatibility: compatibility.status.protocol_name().to_string(),
            build_alignment: build_alignment_name(compatibility.build_alignment).to_string(),
            desktop_runtime_contract: DESKTOP_RUNTIME_CONTRACT,
            diagnostics: AdminDashboardDiagnostics {
                runner_process: layers.runner_process,
                server_transport: layers.server_transport,
                server_registration: layers.server_registration,
                project_registry: layers.project_registry,
                version_compatibility: version_compatibility(&clients),
            },
        };

        let mut devices = Vec::with_capacity(clients.len());
        let mut projects = Vec::with_capacity(project_ids.len());
        for client in &clients {
            let runner_compatibility = compatibility_by_client.get(client.client_id.as_str());
            let compatibility_name = runner_compatibility
                .map(|facts| facts.status_name())
                .unwrap_or("unknown");
            let protocol_name = runner_compatibility
                .map(|facts| protocol_compatibility_name(facts.protocol_compatibility))
                .unwrap_or("unknown");
            let alignment_name = runner_compatibility
                .map(|facts| build_alignment_name(facts.build_alignment))
                .unwrap_or("unknown");
            devices.push(AdminDashboardDevice {
                display_name: client.display_name.clone(),
                client_id: client.client_id.clone(),
                status: if client.status.is_empty() {
                    if client.connected {
                        "online".to_string()
                    } else {
                        "offline".to_string()
                    }
                } else {
                    client.status.clone()
                },
                transport: client.transport.clone(),
                hostname: client.hostname.clone(),
                last_seen: client.last_seen,
                capabilities: enabled_capabilities(client),
                project_count: client
                    .projects
                    .iter()
                    .filter(|project| !project.disabled)
                    .count(),
                active_jobs: active_by_runner
                    .get(&client.client_id)
                    .map_or(0, |jobs| jobs.active),
                runner_protocol_generation: client.runner_protocol_generation.get(),
                compatibility: compatibility_name.to_string(),
                protocol_compatibility: protocol_name.to_string(),
                build_alignment: alignment_name.to_string(),
            });

            let shell_profiles = client
                .policy
                .as_ref()
                .and_then(|policy| policy.shell_profiles.as_ref());
            for project in &client.projects {
                let id = runner_project_runtime_id(&client.client_id, project.id.as_str());
                let (_, shell_profile_status) =
                    resolve_project_shell_profile(project.shell_profile.as_deref(), shell_profiles);
                projects.push(AdminDashboardProject {
                    active_jobs: active_by_project.get(&id).copied().unwrap_or(0),
                    id,
                    name: project.name.clone(),
                    description: project.description.clone(),
                    client_id: client.client_id.clone(),
                    path: project.path.clone(),
                    connected: client.connected,
                    git_available: project_git_available(project),
                    allow_patch: project.allow_patch,
                    enabled: !project.disabled,
                    revision: project.revision.clone(),
                    shell_profile_status: shell_profile_status.to_string(),
                    compatibility: compatibility_name.to_string(),
                    protocol_compatibility: protocol_name.to_string(),
                    build_alignment: alignment_name.to_string(),
                });
            }
        }
        devices.sort_by(|left, right| left.client_id.cmp(&right.client_id));
        projects.sort_by(|left, right| left.id.cmp(&right.id));

        AdminDashboardSnapshot {
            overview: Ok(overview),
            devices: Ok(devices),
            projects: Ok(projects),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthKind;
    use crate::runner_protocol::{
        RunnerBuildInfo, RunnerCapabilities, RunnerProjectSummary, RunnerRegisterRequest,
    };
    use std::sync::Arc;

    fn project(id: &str, disabled: bool) -> RunnerProjectSummary {
        RunnerProjectSummary {
            id: id.to_string(),
            name: Some(id.to_string()),
            path: format!("/tmp/{id}"),
            allow_patch: true,
            kind: Some("repo".to_string()),
            registration_source: None,
            description: Some(format!("{id} description")),
            hooks: Vec::new(),
            disabled,
            revision: Some(format!(
                "sha256:{}",
                if disabled {
                    "b".repeat(64)
                } else {
                    "a".repeat(64)
                }
            )),
            root_fingerprint: None,
            lineage: None,
            git_branch: Some("main".to_string()),
            git_head: Some("0123456789abcdef0123456789abcdef01234567".to_string()),
            git_dirty: Some(false),
            updated_at: 123,
            shell_profile: None,
        }
    }

    #[tokio::test]
    async fn snapshot_reads_nonempty_typed_registry_state() {
        let registry = Arc::new(crate::RunnerRegistry::default());
        let runtime = ToolRuntime::new(
            registry.clone(),
            Arc::new(crate::tool_runtime::RuntimeInfo::default()),
        );
        let build = crate::build_info::runtime_build_info();
        registry
            .register(crate::test_support::current_runner_registration(
                RunnerRegisterRequest {
                    client_id: "admin-runner".to_string(),
                    runner_instance_id: "admin-instance".to_string(),
                    runner_protocol_generation:
                        crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
                    display_name: Some("Admin Runner".to_string()),
                    owner: None,
                    hostname: Some("admin-host".to_string()),
                    capabilities: RunnerCapabilities {
                        shell: true,
                        git: true,
                        jobs: true,
                        ..Default::default()
                    },
                    computer_session_availability: None,
                    host_context: None,
                    policy: None,
                    process_started_at: Some(100),
                    build: Some(RunnerBuildInfo {
                        version: Some(env!("CARGO_PKG_VERSION").to_string()),
                        git_commit: build.git_commit.map(str::to_string),
                        git_dirty: build.git_dirty,
                        built_at: build.built_at.map(str::to_string),
                        target: build.target.map(str::to_string),
                        architecture: build.architecture.map(str::to_string),
                    }),
                    job_concurrency_limit: Some(2),
                    job_inventory: None,
                    coding_agent_providers: None,
                    coding_agent_inventory: None,
                },
            ))
            .await
            .unwrap();
        crate::test_support::apply_project_inventory_snapshot(
            &registry,
            "admin-runner",
            "admin-instance",
            vec![project("enabled", false), project("disabled", true)],
        )
        .await;

        let mut auth = AuthContext::new(AuthKind::Bootstrap);
        auth.is_bootstrap = true;
        let snapshot = runtime.admin_dashboard_snapshot(&auth).await;
        let overview = snapshot.overview.as_ref().unwrap();
        assert_eq!(overview.runners_total, 1);
        assert_eq!(overview.runners_online, 1);
        assert_eq!(overview.projects_total, 1);
        assert_eq!(overview.projects_online, 1);
        assert_eq!(overview.active_jobs, 0);
        assert_eq!(overview.protocol_compatibility, "compatible");

        let devices = snapshot.devices.as_ref().unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].client_id, "admin-runner");
        assert_eq!(devices[0].project_count, 1);
        assert!(devices[0].capabilities.iter().any(|name| name == "shell"));
        assert!(devices[0].capabilities.iter().any(|name| name == "git"));

        let projects = snapshot.projects.as_ref().unwrap();
        assert_eq!(projects.len(), 2);
        let disabled = projects
            .iter()
            .find(|project| project.id == "agent:admin-runner:disabled")
            .unwrap();
        assert!(!disabled.enabled);
        let enabled = projects
            .iter()
            .find(|project| project.id == "agent:admin-runner:enabled")
            .unwrap();
        assert!(enabled.enabled);
        assert_eq!(enabled.git_available, Some(true));
    }
}
