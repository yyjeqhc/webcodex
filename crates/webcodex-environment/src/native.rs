use crate::service::{
    Component, LinuxSocketSpec, Ownership, ServiceAccount, ServiceCredential, ServiceManager,
    ServiceSpec,
};
use crate::storage::{atomic_private_write, ensure_private_directory, read_private};
use crate::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

const HTTP_LIMIT: usize = 1024 * 1024;
const USER_SCOPES: &[&str] = &[
    "runtime:read",
    "runner:manage",
    "session:collaborate",
    "project:read",
    "project:write",
    "job:run",
];
const RUNNER_SCOPES: &[&str] = &[
    "agent:register",
    "agent:poll",
    "agent:result",
    "agent:job_update",
];

/// The production adapter. Neither frontend supplies its own deployment backend.
pub struct NativeEnvironment {
    client: reqwest::Client,
    pub readiness_timeout: Duration,
    preserve_legacy_listen: bool,
}

impl NativeEnvironment {
    /// Only the Core migration coordinator may preserve a listen address
    /// whose old service was inspected by its frozen ownership adapter.
    pub(crate) fn preserving_legacy_listen(mut self) -> Self {
        self.preserve_legacy_listen = true;
        self
    }

    pub async fn control_service(
        &self,
        store: &EnvironmentStore,
        component: Component,
        operation: ServiceOperation,
    ) -> SetupResultValue<service::ServiceStatus> {
        self.control_service_for_environment(store, None, component, operation)
            .await
    }

    /// A Desktop action is bound to the environment the user actually saw.
    /// Check its identity after acquiring the same lock used by setup/installers.
    pub async fn control_service_for_environment(
        &self,
        store: &EnvironmentStore,
        expected_environment: Option<&str>,
        component: Component,
        operation: ServiceOperation,
    ) -> SetupResultValue<service::ServiceStatus> {
        let _lock = store.lock()?;
        crate::ensure_upgrade_idle_under_lock(store)?;
        let record = store
            .load_environment()?
            .ok_or_else(|| diagnostic("not_configured", "No saved environment is available"))?;
        if expected_environment.is_some_and(|expected| expected != record.environment_id) {
            return Err(diagnostic(
                "environment_changed",
                "The saved environment changed; refresh before controlling its services",
            ));
        }
        if component == Component::Server && !record.request.local_server()
            || component == Component::Runner && !record.request.local_runner()
        {
            return Err(diagnostic(
                "component_not_configured",
                "This component is not configured on this machine",
            ));
        }
        crate::privilege::service_operation(store, &record, component, operation, None).await
    }

    /// Replace only the saved user credential after the saved Server attests
    /// that the replacement belongs to the same frozen user identity.
    pub async fn repair_user_credential(
        &self,
        store: &EnvironmentStore,
        expected_environment: &str,
        token: &Secret,
    ) -> SetupResultValue<()> {
        let _lock = store.lock()?;
        crate::ensure_upgrade_idle_under_lock(store)?;
        let record = store
            .load_environment()?
            .ok_or_else(|| diagnostic("not_configured", "No saved environment is available"))?;
        if expected_environment.is_empty() || expected_environment != record.environment_id {
            return Err(diagnostic(
                "environment_changed",
                "The saved environment changed; refresh before restoring its user credential",
            ));
        }
        if current_account()?.identity != record.request.account.identity {
            return Err(diagnostic(
                "account_changed",
                "The saved environment belongs to another local account",
            ));
        }
        let saved_username = record
            .username
            .as_deref()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| {
                diagnostic(
                    "server_identity_contract",
                    "The saved environment has no verified Server user identity",
                )
            })?;
        if token.expose().is_empty()
            || token.expose().len() > 8192
            || token.expose() != token.expose().trim()
        {
            return Err(diagnostic(
                "user_credential_required",
                "Provide a bounded user credential without surrounding whitespace",
            ));
        }

        // Check recovery material before any write: a stale or foreign
        // enrollment must never replay a credential over this repair.
        let mut enrollment = read_enrollment(store, &record)?;
        let overview = self.authenticated(&record, token.expose()).await?;
        if authenticated_username(&overview)? != saved_username {
            return Err(diagnostic(
                "user_identity_conflict",
                "The credential belongs to a different Server user than the saved environment",
            ));
        }

        if let Some(recovery) = enrollment.as_mut() {
            // Commit recovery first. If the active-token write then fails,
            // resume can still use the newly verified credential; the reverse
            // order could resurrect the expired credential.
            unsafe { recovery.user_token.as_bytes_mut().fill(0) };
            recovery.user_token = token.expose().to_owned();
            store.write_json("enrollment-recovery.json", recovery)?;
        }
        atomic_private_write(
            &store.root().join("webcodex-user-token"),
            token.expose().as_bytes(),
        )
    }

    pub async fn add_project(
        &mut self,
        store: &EnvironmentStore,
        path: &Path,
    ) -> SetupResultValue<SetupResult> {
        let _lock = store.lock()?;
        crate::ensure_upgrade_idle_under_lock(store)?;
        let mut record = store
            .load_environment()?
            .ok_or_else(|| diagnostic("not_configured", "Configure an environment first"))?;
        let client_id = record.runner_client_id.clone().ok_or_else(|| {
            diagnostic(
                "runner_enrollment_required",
                "This viewing machine must join as a project machine before adding a project",
            )
        })?;
        let path = path.canonicalize().map_err(|_| {
            diagnostic(
                "project_path",
                "The selected project directory is unavailable",
            )
        })?;
        let mut request = record.request.clone();
        request.project = Some(path.clone());
        validate_existing_request(store, &request)?;
        let token = read_secret(&store.root().join("webcodex-user-token"))?;
        let mut pending: ProjectAddition =
            store
                .read_json("add-project.json")?
                .unwrap_or(ProjectAddition {
                    path: path.clone(),
                    dispatched: false,
                });
        if pending.path != path {
            return Err(diagnostic(
                "project_operation_conflict",
                "Another project addition is unfinished",
            ));
        }
        store.write_json("add-project.json", &pending)?;
        let lookup = |value: &Value| -> Option<ProjectRecord> {
            value
                .get("projects")?
                .as_array()?
                .iter()
                .find_map(|project| {
                    if project.get("path")?.as_str()? != path.to_str()? {
                        return None;
                    }
                    let id = project
                        .get("id")?
                        .as_str()?
                        .strip_prefix(&format!("agent:{client_id}:"))?;
                    Some(ProjectRecord {
                        id: id.into(),
                        path: path.clone(),
                    })
                })
        };
        let inventory = self
            .post(
                &record.request.server_url,
                "/api/runtime-console/projects",
                Some(token.expose()),
                json!({"client_id": client_id, "limit": 200}),
            )
            .await?;
        let mut visible = lookup(&inventory);
        if visible.is_none() && pending.dispatched {
            return Err(diagnostic(
                "project_reconcile_required",
                "Project registration may have occurred but is not visible in Server inventory",
            ));
        }
        if visible.is_none() {
            let config_path = store.root().join("runner.toml");
            let before = read_secret(&config_path)?;
            let mut config: toml::Value = toml::from_str(before.expose()).map_err(|_| {
                diagnostic("runner_configuration", "Runner configuration is invalid")
            })?;
            if config.get("client_id").and_then(toml::Value::as_str) != Some(&client_id)
                || config.get("server_url").and_then(toml::Value::as_str)
                    != Some(record.request.server_url.as_str())
            {
                return Err(diagnostic(
                    "runner_binding_conflict",
                    "The local Runner binding has changed",
                ));
            }
            let roots = config
                .get_mut("policy")
                .and_then(|v| v.get_mut("allowed_roots"))
                .and_then(toml::Value::as_array_mut)
                .ok_or_else(|| diagnostic("runner_policy", "Runner allowed roots are missing"))?;
            if !roots.iter().any(|root| root.as_str() == path.to_str()) {
                roots.push(toml::Value::String(path.to_string_lossy().into_owned()));
                if read_secret(&config_path)?.expose() != before.expose() {
                    return Err(diagnostic(
                        "config_concurrent_change",
                        "Runner configuration changed while preparing the project",
                    ));
                }
                atomic_private_write(
                    &config_path,
                    toml::to_string(&config)
                        .map_err(|_| SetupDiagnostic::io())?
                        .as_bytes(),
                )?;
            }
            let candidate = read_secret(&config_path)?;
            let checked = self
                .post(
                    &record.request.server_url,
                    "/api/tools/call",
                    Some(token.expose()),
                    json!({"tool":"runner_config_check","params":{"client_id":client_id}}),
                )
                .await?;
            let checked = tool_output(&checked)?;
            if checked.get("valid").and_then(Value::as_bool) != Some(true)
                || checked.get("restart_required").and_then(Value::as_bool) == Some(true)
            {
                return Err(diagnostic(
                    "config_requires_attention",
                    "Runner configuration cannot be hot reloaded",
                ));
            }
            let generation = checked
                .get("current_generation")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    diagnostic(
                        "config_generation",
                        "Runner did not report its configuration generation",
                    )
                })?;
            if candidate.expose() != read_secret(&config_path)?.expose() {
                return Err(diagnostic(
                    "config_concurrent_change",
                    "Runner configuration changed before reload",
                ));
            }
            let reload = self.post(&record.request.server_url, "/api/tools/call", Some(token.expose()), json!({"tool":"runner_config_reload","params":{"client_id":client_id,"expected_generation":generation}})).await?;
            tool_output(&reload)?;
            pending.dispatched = true;
            store.write_json("add-project.json", &pending)?;
            // Lost responses are reconciled by exact path on the next invocation.
            let registration = self
                .post(
                    &record.request.server_url,
                    "/api/projects/resolve-or-register",
                    Some(token.expose()),
                    json!({"client_id":client_id,"path":path}),
                )
                .await;
            match registration {
                Ok(ref value) if value.get("success").and_then(Value::as_bool) != Some(true) => {
                    if value
                        .pointer("/output/execution_state")
                        .and_then(Value::as_str)
                        == Some("not_started")
                    {
                        pending.dispatched = false;
                        store.write_json("add-project.json", &pending)?;
                    }
                    tool_output(value)?;
                }
                Err(ref error)
                    if matches!(
                        error.code.as_str(),
                        "authentication_required" | "permission_denied"
                    ) =>
                {
                    pending.dispatched = false;
                    store.write_json("add-project.json", &pending)?;
                    return Err(error.clone());
                }
                _ => {}
            }
            let inventory = self
                .post(
                    &record.request.server_url,
                    "/api/runtime-console/projects",
                    Some(token.expose()),
                    json!({"client_id":client_id,"limit":200}),
                )
                .await?;
            visible = lookup(&inventory);
        }
        let project = visible.ok_or_else(|| {
            diagnostic(
                "project_reconcile_required",
                "The new project has not been verified in Server inventory",
            )
        })?;
        if !record
            .projects
            .iter()
            .any(|existing| existing.path == project.path)
        {
            record.projects.push(project);
        }
        if let Some(mut journal) = store.load_journal()? {
            journal.environment = record.clone();
            store.save_journal(&journal)?;
        }
        store.save_environment(&record)?;
        std::fs::remove_file(store.root().join("add-project.json"))
            .map_err(|_| SetupDiagnostic::io())?;
        let observation = self.observe(store, &record).await?;
        Ok(SetupResult {
            environment: record,
            observation,
        })
    }
    pub async fn remove_project(
        &mut self,
        store: &EnvironmentStore,
        project_id: &str,
        expected_revision: Option<&str>,
    ) -> SetupResultValue<SetupResult> {
        let _lock = store.lock()?;
        crate::ensure_upgrade_idle_under_lock(store)?;
        let mut record = store
            .load_environment()?
            .ok_or_else(|| diagnostic("not_configured", "Configure an environment first"))?;
        let client_id = record.runner_client_id.as_deref().ok_or_else(|| {
            diagnostic("local_runner_required", "This machine has no local Runner")
        })?;
        let prefix = format!("agent:{client_id}:");
        let id = project_id.strip_prefix(&prefix).unwrap_or(project_id);
        let mut pending: ProjectRemoval =
            if let Some(pending) = store.read_json("remove-project.json")? {
                pending
            } else {
                let project = record
                    .projects
                    .iter()
                    .find(|project| project.id == id)
                    .cloned()
                    .ok_or_else(|| {
                        diagnostic(
                            "local_project_required",
                            "Only a registered project of this local Runner can be removed",
                        )
                    })?;
                ProjectRemoval {
                    project,
                    revision: None,
                    dispatched: false,
                    confirmed: false,
                }
            };
        if pending.project.id != id {
            return Err(diagnostic(
                "project_operation_conflict",
                "Another project removal is unfinished",
            ));
        }
        if id.contains(['/', '\\']) || matches!(id, "." | "..") {
            return Err(diagnostic(
                "project_identity",
                "The saved project identity is invalid",
            ));
        }
        store.write_json("remove-project.json", &pending)?;
        let token = read_secret(&store.root().join("webcodex-user-token"))?;
        let selector = format!("{prefix}{id}");
        let inventory = self
            .post(
                &record.request.server_url,
                "/api/tools/call",
                Some(token.expose()),
                json!({"tool":"list_projects","params":{}}),
            )
            .await?;
        let projects = tool_output(&inventory)?
            .get("projects")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                diagnostic(
                    "project_inventory",
                    "Server did not return a valid project inventory",
                )
            })?;
        let visible = projects
            .iter()
            .find(|project| project.get("id").and_then(Value::as_str) == Some(selector.as_str()));
        if !pending.confirmed && pending.dispatched {
            let observation = self.observe(store, &record).await?;
            if visible.is_none()
                && observation.runner_online == Some(true)
                && !store
                    .root()
                    .join("project-registry")
                    .join(format!("{id}.toml"))
                    .exists()
            {
                pending.confirmed = true;
            } else {
                return Err(diagnostic("project_reconcile_required", "The previous unregister result is not yet confirmed; it will not be automatically repeated"));
            }
        }
        if !pending.confirmed {
            let revision = visible
                .and_then(|value| value.get("revision"))
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    diagnostic(
                        "project_revision",
                        "An authorized current project revision is required before unregistering",
                    )
                })?;
            if expected_revision.is_some_and(|expected| expected != revision) {
                return Err(diagnostic(
                    "revision_conflict",
                    "The project changed after confirmation; inspect it again before removing it",
                ));
            }
            pending.revision = Some(revision.into());
            pending.dispatched = true;
            store.write_json("remove-project.json", &pending)?;
            let response = self.post(&record.request.server_url, "/api/tools/call", Some(token.expose()), json!({"tool":"unregister_project","params":{"project":selector,"expected_revision":revision}})).await?;
            if response.get("success").and_then(Value::as_bool) != Some(true) {
                let code = response
                    .pointer("/output/error/code")
                    .or_else(|| response.pointer("/output/error_code"))
                    .or_else(|| response.pointer("/output/error_kind"))
                    .and_then(Value::as_str);
                if matches!(
                    code,
                    Some(
                        "active_jobs_conflict"
                            | "agent_unavailable"
                            | "unsupported_runner_version"
                            | "revision_conflict"
                            | "project_not_found"
                            | "invalid_request"
                    )
                ) {
                    pending.dispatched = false;
                    store.write_json("remove-project.json", &pending)?;
                }
                return Err(diagnostic("project_remove_rejected", "Server did not confirm removal; inspect active tasks and the project revision before retrying"));
            }
            let output = tool_output(&response)?;
            if output.get("project").and_then(Value::as_str) != Some(selector.as_str())
                || !matches!(
                    output.get("outcome").and_then(Value::as_str),
                    Some("unregistered" | "already_unregistered")
                )
            {
                return Err(diagnostic(
                    "project_reconcile_required",
                    "Server did not confirm a terminal result for this project",
                ));
            }
            pending.confirmed = true;
            store.write_json("remove-project.json", &pending)?;
        }
        let mut removed: Vec<ProjectRecord> = store
            .read_json("removed-projects.json")?
            .unwrap_or_default();
        if !removed.contains(&pending.project) {
            removed.push(pending.project.clone());
        }
        store.write_json("removed-projects.json", &removed)?;
        record.projects.retain(|project| project.id != id);
        if let Some(mut journal) = store.load_journal()? {
            journal.environment = record.clone();
            store.save_journal(&journal)?;
        }
        store.save_environment(&record)?;
        std::fs::remove_file(store.root().join("remove-project.json"))
            .map_err(|_| SetupDiagnostic::io())?;
        let observation = self.observe(store, &record).await?;
        Ok(SetupResult {
            environment: record,
            observation,
        })
    }
    pub fn new() -> SetupResultValue<Self> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|_| diagnostic("http_client", "Could not initialize the Server connection"))?;
        Ok(Self {
            client,
            readiness_timeout: Duration::from_secs(45),
            preserve_legacy_listen: false,
        })
    }

    pub async fn status(&mut self, store: &EnvironmentStore) -> SetupResultValue<SetupResult> {
        let record = store
            .load_environment()?
            .or_else(|| {
                store
                    .load_journal()
                    .ok()
                    .flatten()
                    .map(|journal| journal.environment)
            })
            .ok_or_else(|| diagnostic("not_configured", "This machine has no saved environment"))?;
        let observation = self.observe(store, &record).await?;
        Ok(SetupResult {
            environment: record,
            observation,
        })
    }

    /// Read-only diagnosis for the CLI and Desktop. Each local service is
    /// inspected independently; a viewer-only environment has none to inspect.
    pub async fn doctor(&mut self, store: &EnvironmentStore) -> SetupResultValue<SetupResult> {
        let mut result = self.status(store).await?;
        let record = &result.environment;
        for component in [
            record.request.local_server().then_some(Component::Server),
            record.request.local_runner().then_some(Component::Runner),
        ]
        .into_iter()
        .flatten()
        {
            let name = match component {
                Component::Server => "server",
                Component::Runner => "runner",
                Component::Tunnel => unreachable!(),
            };
            match service_spec(store, record, component) {
                Ok(spec) => {
                    match ServiceManager::inspect(&spec).map_err(service_error) {
                        Ok(status) => result
                            .observation
                            .diagnostics
                            .push(service_diagnostic(name, &status)),
                        Err(error) => result.observation.diagnostics.push(error),
                    }
                    #[cfg(any(windows, target_os = "macos"))]
                    result
                        .observation
                        .diagnostics
                        .push(service_log_diagnostic(name, &spec));
                }
                Err(error) => result.observation.diagnostics.push(error),
            }
            #[cfg(any(windows, target_os = "macos"))]
            if component == Component::Runner {
                match service_spec(store, record, component).and_then(|spec| {
                    crate::session_service::inspect_session_helper(&spec, &store.root().join("cu"))
                        .map_err(service_error)
                }) {
                    Ok(status) => result
                        .observation
                        .diagnostics
                        .push(service_diagnostic("computer_helper", &status)),
                    Err(error) => result.observation.diagnostics.push(error),
                }
            }
        }
        if record.request.local_server() {
            match crate::tunnel::tunnel_profiles(store) {
                Ok(profiles) => {
                    for profile in profiles {
                        match self.tunnel_status(store, &profile.profile_id) {
                            Ok(tunnel) => {
                                result
                                    .observation
                                    .diagnostics
                                    .push(service_diagnostic("tunnel", &tunnel.service_status));
                                result.observation.diagnostics.push(SetupDiagnostic::new(
                                if tunnel.ready { "tunnel_ready" } else { "tunnel_not_ready" },
                                &format!("Tunnel profile {}: tunnel_ready={}, local_mcp_ready={}", profile.profile_id, tunnel.tunnel_ready, tunnel.local_mcp_ready),
                                "Check the private Tunnel readiness marker and managed service before retrying",
                            ));
                                #[cfg(any(windows, target_os = "macos"))]
                                if let Ok(spec) = crate::tunnel::tunnel_service_spec(
                                    store,
                                    record,
                                    &profile.profile_id,
                                ) {
                                    result
                                        .observation
                                        .diagnostics
                                        .push(service_log_diagnostic("tunnel", &spec));
                                }
                            }
                            Err(error) => result.observation.diagnostics.push(error),
                        }
                    }
                }
                Err(error) => result.observation.diagnostics.push(error),
            }
        }
        if result.observation.authenticated {
            match read_secret(&store.root().join("webcodex-user-token")) {
                Ok(token) => {
                    match self.authenticated(record, token.expose()).await {
                        Ok(overview) => {
                            let version = overview
                                .get("version")
                                .and_then(Value::as_str)
                                .filter(|value| {
                                    !value.is_empty()
                                        && value.len() <= 80
                                        && !value.chars().any(char::is_control)
                                })
                                .unwrap_or("unknown");
                            result.observation.diagnostics.push(SetupDiagnostic::new(
                                "server_protocol",
                                &format!("WebCodex Server version: {version}"),
                                "Check Server and Runner release compatibility before upgrading",
                            ));
                            let identity = overview
                                .get("authenticated_user")
                                .and_then(Value::as_str)
                                .filter(|value| !value.is_empty());
                            result.observation.diagnostics.push(SetupDiagnostic::new(if identity.is_some() { "server_user_verified" } else { "server_user_unreported" }, if identity.is_some() { "Server reported the authenticated user" } else { "Server does not report the authenticated user; viewing remains available, but attaching a Runner requires a Server upgrade" }, "Keep the current user credential; do not infer ownership from the local account name"));
                        }
                        Err(error) => result.observation.diagnostics.push(error),
                    }
                    if let Some(client_id) = &record.runner_client_id {
                        match self
                            .post(
                                &record.request.server_url,
                                "/api/runtime-console/runner",
                                Some(token.expose()),
                                json!({"client_id": client_id, "project_limit": 200}),
                            )
                            .await
                        {
                            Ok(runner) => {
                                let compatibility = runner
                                    .get("protocol_compatibility")
                                    .and_then(Value::as_str)
                                    .filter(|value| {
                                        value.len() <= 40
                                            && value.bytes().all(|byte| {
                                                byte.is_ascii_alphanumeric()
                                                    || byte == b'_'
                                                    || byte == b'-'
                                            })
                                    })
                                    .unwrap_or("unknown");
                                result.observation.diagnostics.push(SetupDiagnostic::new("runner_protocol", &format!("Runner protocol compatibility: {compatibility}"), "Use a Server and Runner release with matching protocol support"));
                            }
                            Err(error) => result.observation.diagnostics.push(error),
                        }
                    }
                }
                Err(error) => result.observation.diagnostics.push(error),
            }
        } else {
            result.observation.diagnostics.push(diagnostic(
                "server_auth_unverified",
                "The saved user credential has not been verified by the Server",
            ));
        }
        if record.runner_client_id.is_some() {
            if result.observation.runner_online != Some(true) {
                result.observation.diagnostics.push(diagnostic(
                    "runner_offline",
                    "The local Runner is not confirmed online by the Server",
                ));
            }
            for project in &record.projects {
                if !result.observation.projects_visible.contains(&project.id) {
                    result.observation.diagnostics.push(diagnostic(
                        "project_not_visible",
                        &format!(
                            "Saved project {} is not visible to the authenticated user",
                            project.id
                        ),
                    ));
                }
            }
        }
        Ok(result)
    }

    pub(crate) async fn post(
        &self,
        server: &str,
        route: &str,
        token: Option<&str>,
        body: Value,
    ) -> SetupResultValue<Value> {
        let mut request = self.client.post(format!("{server}{route}")).json(&body);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        let mut response = request.send().await.map_err(|_| {
            diagnostic(
                "server_unreachable",
                "The Server request did not produce a verified response",
            )
        })?;
        let status = response.status();
        if !status.is_success() {
            return Err(match status.as_u16() {
                401 => diagnostic(
                    "authentication_required",
                    "The user credential was not accepted",
                ),
                403 => diagnostic(
                    "permission_denied",
                    "The authenticated user is not authorized for this operation",
                ),
                409 => diagnostic(
                    "server_conflict",
                    "The Server reports an existing or conflicting resource",
                ),
                _ => diagnostic("server_request_failed", "The Server rejected the operation"),
            });
        }
        if response
            .content_length()
            .is_some_and(|n| n > HTTP_LIMIT as u64)
        {
            return Err(diagnostic(
                "response_limit",
                "The Server response exceeded its bound",
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| diagnostic("response_uncertain", "The Server response was interrupted"))?
        {
            if bytes.len().saturating_add(chunk.len()) > HTTP_LIMIT {
                return Err(diagnostic(
                    "response_limit",
                    "The Server response exceeded its bound",
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes)
            .map_err(|_| diagnostic("response_invalid", "The Server response was invalid"))
    }

    async fn authenticated(
        &self,
        record: &EnvironmentRecord,
        token: &str,
    ) -> SetupResultValue<Value> {
        if token.starts_with("wc_agent_")
            || token.starts_with("wc_boot_")
            || token.starts_with("wc_acct_")
        {
            return Err(diagnostic(
                "user_credential_required",
                "Use a user API credential for the Runtime Console",
            ));
        }
        let overview = self
            .post(
                &record.request.server_url,
                "/api/runtime-console/overview",
                Some(token),
                json!({}),
            )
            .await?;
        if overview.get("service").and_then(Value::as_str) != Some("webcodex") {
            return Err(diagnostic(
                "server_identity_contract",
                "The address did not return a WebCodex Runtime Console",
            ));
        }
        Ok(overview)
    }

    async fn reachable(&self, record: &EnvironmentRecord) -> SetupResultValue<()> {
        let response = self
            .client
            .get(format!("{}/runtime", record.request.server_url))
            .send()
            .await
            .map_err(|_| diagnostic("server_unreachable", "The Server address is not reachable"))?;
        if response.status().is_success()
            && response
                .headers()
                .get("x-webcodex-console-assets")
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| matches!(value, "embedded" | "filesystem"))
        {
            Ok(())
        } else {
            Err(diagnostic(
                "server_unreachable",
                "The address did not return a healthy WebCodex Server",
            ))
        }
    }

    async fn wait_reachable(&self, record: &EnvironmentRecord) -> SetupResultValue<()> {
        let deadline = tokio::time::Instant::now() + self.readiness_timeout;
        loop {
            if matches!(
                tokio::time::timeout_at(deadline, self.reachable(record)).await,
                Ok(Ok(()))
            ) {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(diagnostic(
                    "server_not_ready",
                    "Server startup did not become healthy before its deadline",
                ));
            }
            tokio::time::sleep_until(
                (tokio::time::Instant::now() + Duration::from_millis(250)).min(deadline),
            )
            .await;
        }
    }

    async fn create_user(
        &self,
        store: &EnvironmentStore,
        record: &mut EnvironmentRecord,
    ) -> SetupResultValue<()> {
        let bootstrap = bootstrap_token(store)?;
        let username = record
            .username
            .get_or_insert_with(|| account_username(&record.request.account.name))
            .clone();
        let path = store.root().join("webcodex-user-token");
        if path.exists() {
            let token = read_secret(&path)?;
            match self.authenticated(record, token.expose()).await {
                Ok(identity) => {
                    ensure_username(record, &identity)?;
                    return Ok(());
                }
                Err(error) if error.code == "authentication_required" => {}
                Err(error) => return Err(error),
            }
        }
        match self
            .post(
                &record.request.server_url,
                "/api/users/create",
                Some(bootstrap.expose()),
                json!({"username": username}),
            )
            .await
        {
            Ok(created)
                if created.get("success").and_then(Value::as_bool) == Some(true)
                    && created.pointer("/user/username").and_then(Value::as_str)
                        == Some(username.as_str()) => {}
            Ok(_) => {
                return Err(diagnostic(
                    "response_invalid",
                    "Server did not confirm the created user identity",
                ))
            }
            Err(error) if error.code == "server_conflict" => {}
            Err(error) => return Err(error),
        }
        if !path.exists() {
            atomic_private_write(&path, generate_token("wc_pat_").as_bytes())?;
        }
        let token = read_secret(&path)?;
        match self.authenticated(record, token.expose()).await {
            Ok(identity) => {
                ensure_username(record, &identity)?;
                return Ok(());
            }
            Err(error) if error.code == "authentication_required" => {}
            Err(error) => return Err(error),
        }
        // Only a definite authentication rejection permits registration. A
        // timeout or malformed response leaves the saved token intact for
        // reconciliation instead of issuing another external mutation.
        match self
            .register_hash(record, bootstrap.expose(), token.expose(), false)
            .await
        {
            Ok(()) => {}
            Err(error) if error.code == "server_conflict" => {}
            Err(error) => return Err(error),
        }
        let identity = self.authenticated(record, token.expose()).await?;
        ensure_username(record, &identity)?;
        Ok(())
    }

    async fn register_hash(
        &self,
        record: &EnvironmentRecord,
        bootstrap: &str,
        token: &str,
        runner: bool,
    ) -> SetupResultValue<()> {
        let mut body = json!({"username": record.username, "name": "WebCodex environment", "token_hash": format!("sha256:{:x}", Sha256::digest(token.as_bytes())), "token_prefix": &token[..token.len().min(16)], "scopes": if runner { RUNNER_SCOPES } else { USER_SCOPES }});
        if runner {
            body["client_id"] = json!(record.runner_client_id);
        }
        let route = if runner {
            "/api/agent-tokens/register_hash"
        } else {
            "/api/tokens/register_hash"
        };
        let result = self
            .post(&record.request.server_url, route, Some(bootstrap), body)
            .await?;
        if result.get("success").and_then(Value::as_bool) != Some(true)
            || result
                .pointer("/token/token_prefix")
                .and_then(Value::as_str)
                != Some(&token[..token.len().min(16)])
            || runner
                && result
                    .pointer("/token/allowed_client_id")
                    .and_then(Value::as_str)
                    != record.runner_client_id.as_deref()
        {
            return Err(diagnostic(
                "response_invalid",
                "Server did not confirm the credential binding",
            ));
        }
        Ok(())
    }

    async fn enroll(
        &self,
        store: &EnvironmentStore,
        record: &mut EnvironmentRecord,
        secrets: &SetupSecrets,
    ) -> SetupResultValue<()> {
        let code = secrets.pairing_code.as_ref().ok_or_else(|| {
            diagnostic(
                "pairing_code_required",
                "A short-lived Runner pairing code is required",
            )
        })?;
        let value = self.post(&record.request.server_url, "/api/pairing/enroll", None, json!({"pairing_code": code.expose(), "client_id": record.runner_client_id, "transport": "auto"})).await?;
        if value.get("success").and_then(Value::as_bool) != Some(true)
            || value.get("client_id").and_then(Value::as_str) != record.runner_client_id.as_deref()
        {
            return Err(diagnostic(
                "enrollment_binding_conflict",
                "The Server did not confirm the requested Runner identity",
            ));
        }
        let mut enrollment = Enrollment {
            server_url: record.request.server_url.clone(),
            client_id: record
                .runner_client_id
                .clone()
                .ok_or_else(|| diagnostic("runner_identity", "Runner identity was not prepared"))?,
            username: response_string(&value, "username")?,
            user_token: response_string(&value, "user_token")?,
            runner_token: response_string(&value, "agent_token")?,
        };
        if !enrollment.user_token.starts_with("wc_pat_")
            || !enrollment.runner_token.starts_with("wc_agent_")
        {
            return Err(diagnostic(
                "response_invalid",
                "The Server returned unexpected enrollment credential types",
            ));
        }
        // This file is recovery material, never a journal or diagnostic. Keep it
        // until runner.toml and the user token are both durable and verified.
        store.write_json("enrollment-recovery.json", &enrollment)?;
        if record
            .username
            .as_ref()
            .is_some_and(|user| user != &enrollment.username)
        {
            return Err(diagnostic("pairing_user_conflict", "The paired Runner belongs to another user; credentials were retained for explicit recovery and the existing viewing identity was preserved"));
        }
        record.username = Some(enrollment.username.clone());
        atomic_private_write(
            &store.root().join("webcodex-user-token"),
            enrollment.user_token.as_bytes(),
        )?;
        enrollment.clear();
        Ok(())
    }

    async fn configure_runner(
        &self,
        store: &EnvironmentStore,
        record: &mut EnvironmentRecord,
    ) -> SetupResultValue<()> {
        let mut enrollment = match read_enrollment(store, record)? {
            Some(value) => value,
            None if record.request.local_server() => {
                let value = Enrollment {
                    server_url: record.request.server_url.clone(),
                    client_id: record.runner_client_id.clone().ok_or_else(|| {
                        diagnostic("runner_identity", "Runner identity is missing")
                    })?,
                    username: record.username.clone().ok_or_else(|| {
                        diagnostic("user_identity", "User authentication is incomplete")
                    })?,
                    user_token: read_secret(&store.root().join("webcodex-user-token"))?
                        .expose()
                        .to_string(),
                    runner_token: generate_token("wc_agent_"),
                };
                store.write_json("enrollment-recovery.json", &value)?;
                value
            }
            None => {
                return Err(diagnostic(
                    "pairing_recovery_required",
                    "Runner enrollment credentials could not be recovered",
                ))
            }
        };
        if record.request.local_server() {
            // Locally generated tokens are durable before hash registration;
            // retry registers the same identity, never mints another token.
            let result = self
                .register_hash(
                    record,
                    bootstrap_token(store)?.expose(),
                    &enrollment.runner_token,
                    true,
                )
                .await;
            if let Err(error) = result {
                if error.code != "server_conflict" {
                    return Err(error);
                }
            }
        }
        let registry = store.root().join("project-registry");
        ensure_private_directory(&registry)?;
        let config = webcodex_runner_config::generated_runner_config_toml(
            &webcodex_runner_config::RunnerInitOptions {
                server_url: record.request.server_url.clone(),
                token: Some(enrollment.runner_token.clone()),
                token_file: None,
                client_id: enrollment.client_id.clone(),
                owner: enrollment.username.clone(),
                display_name: None,
                transport: "auto".into(),
                poll_interval_ms: 1000,
                project_registry_dir: registry,
                output: store.root().join("runner.toml"),
                allowed_roots: record.request.project.iter().cloned().collect(),
                allow_cwd_anywhere: false,
                overwrite: false,
            },
        )
        .map_err(|_| {
            diagnostic(
                "runner_configuration",
                "Runner configuration did not pass validation",
            )
        })?;
        atomic_private_write(&store.root().join("runner.toml"), config.as_bytes())?;
        atomic_private_write(
            &store.root().join("webcodex-user-token"),
            enrollment.user_token.as_bytes(),
        )?;
        record.username = Some(enrollment.username.clone());
        enrollment.clear();
        Ok(())
    }

    pub async fn invite(&self, store: &EnvironmentStore) -> SetupResultValue<Secret> {
        let _lock = store.lock()?;
        crate::ensure_upgrade_idle_under_lock(store)?;
        let record = store.load_environment()?.ok_or_else(|| {
            diagnostic("not_configured", "Configure the Server environment first")
        })?;
        if !record.request.local_server() {
            return Err(diagnostic(
                "server_admin_required",
                "Create pairing codes on the Server machine",
            ));
        }
        let response = self
            .post(
                &record.request.server_url,
                "/api/pairing/create",
                Some(bootstrap_token(store)?.expose()),
                json!({"username": record.username, "ttl_secs": 600}),
            )
            .await?;
        Ok(Secret::new(response_string(&response, "pairing_code")?))
    }
}

impl EnvironmentBackend for NativeEnvironment {
    async fn reconcile(
        &mut self,
        store: &EnvironmentStore,
        record: &mut EnvironmentRecord,
        step: SetupStep,
    ) -> SetupResultValue<Reconciliation> {
        use Reconciliation::*;
        use SetupStep::*;
        match step {
            Preflight | ServerReachability | Readiness => Ok(Missing),
            ServerConfiguration => {
                let path = store.root().join("server/webcodex.env");
                if !path.exists() {
                    return Ok(Missing);
                }
                let content = read_secret(&path)?;
                let expected = match &record.request.mode {
                    EnvironmentMode::Create { listen } => listen.as_str(),
                    _ => return Ok(Unknown),
                };
                if env_value(content.expose(), "WEBCODEX_ADDR").as_deref() != Some(expected) {
                    return Err(diagnostic(
                        "server_binding_conflict",
                        "The existing Server configuration has another listen address",
                    ));
                }
                bootstrap_token(store)?;
                Ok(Complete)
            }
            UserAuthentication => {
                let path = store.root().join("webcodex-user-token");
                if !path.exists() {
                    return Ok(Missing);
                }
                let token = read_secret(&path)?;
                if record.request.local_server() && record.username.is_none() {
                    record.username = Some(account_username(&record.request.account.name));
                }
                match self.authenticated(record, token.expose()).await {
                    Ok(identity) => {
                        ensure_username(record, &identity)?;
                        Ok(Complete)
                    }
                    Err(error)
                        if record.request.local_server()
                            && error.code == "authentication_required" =>
                    {
                        Ok(Missing)
                    }
                    Err(error) => Err(error),
                }
            }
            RunnerEnrollment => {
                if let Some(enrollment) = read_enrollment(store, record)? {
                    record.username = Some(enrollment.username.clone());
                    atomic_private_write(
                        &store.root().join("webcodex-user-token"),
                        enrollment.user_token.as_bytes(),
                    )?;
                    return Ok(Complete);
                }
                if runner_config_matches(store, record)?
                    && store.root().join("webcodex-user-token").is_file()
                {
                    Ok(Complete)
                } else {
                    Ok(Missing)
                }
            }
            RunnerConfiguration => Ok(if runner_config_matches(store, record)? {
                Complete
            } else {
                Missing
            }),
            ProjectRegistration => {
                let project = record.request.project.as_ref().ok_or_else(|| {
                    diagnostic("project_required", "No local project was selected")
                })?;
                let removed: Vec<ProjectRecord> = store
                    .read_json("removed-projects.json")?
                    .unwrap_or_default();
                if removed.iter().any(|entry| &entry.path == project) {
                    return Ok(Complete);
                }
                let expected = record
                    .projects
                    .iter()
                    .find(|entry| &entry.path == project)
                    .cloned()
                    .unwrap_or_else(|| project_record(project));
                let path = store
                    .root()
                    .join("project-registry")
                    .join(format!("{}.toml", expected.id));
                if path.exists() {
                    let value: toml::Value =
                        toml::from_str(read_secret(&path)?.expose()).map_err(|_| {
                            diagnostic(
                                "project_record_invalid",
                                "Saved project registration is invalid",
                            )
                        })?;
                    if value.get("path").and_then(toml::Value::as_str) != project.to_str() {
                        return Err(diagnostic(
                            "project_conflict",
                            "The project identifier is registered to another directory",
                        ));
                    }
                    if !record.projects.contains(&expected) {
                        record.projects.push(expected);
                    }
                    Ok(Complete)
                } else {
                    Ok(Missing)
                }
            }
            ServerServiceInstall | RunnerServiceInstall | ServerServiceStart
            | RunnerServiceStart => {
                let component = if matches!(step, ServerServiceInstall | ServerServiceStart) {
                    Component::Server
                } else {
                    Component::Runner
                };
                let status = ServiceManager::inspect(&service_spec(store, record, component)?)
                    .map_err(service_error)?;
                match status.ownership {
                    Ownership::Foreign => Err(diagnostic(
                        "service_owner_conflict",
                        "An existing service belongs to another configuration",
                    )),
                    Ownership::Unknown => Ok(Unknown),
                    Ownership::Absent => Ok(Missing),
                    Ownership::Owned
                        if matches!(step, ServerServiceInstall | RunnerServiceInstall) =>
                    {
                        #[cfg(any(windows, target_os = "macos"))]
                        if component == Component::Runner {
                            let helper = crate::session_service::inspect_session_helper(
                                &service_spec(store, record, component)?,
                                &store.root().join("cu"),
                            )
                            .map_err(service_error)?;
                            match helper.ownership {
                                Ownership::Absent => return Ok(Missing),
                                Ownership::Owned => {}
                                Ownership::Foreign => {
                                    return Err(diagnostic(
                                        "helper_owner_conflict",
                                        "A different Computer session helper owns this login task",
                                    ))
                                }
                                Ownership::Unknown => return Ok(Unknown),
                            }
                        }
                        Ok(if status.enabled == Some(true) {
                            Complete
                        } else {
                            Missing
                        })
                    }
                    Ownership::Owned => Ok(match status.running {
                        Some(true) => Complete,
                        Some(false) => Missing,
                        None => Unknown,
                    }),
                }
            }
        }
    }

    async fn apply(
        &mut self,
        store: &EnvironmentStore,
        record: &mut EnvironmentRecord,
        step: SetupStep,
        secrets: &SetupSecrets,
    ) -> SetupResultValue<()> {
        use SetupStep::*;
        match step {
            Preflight => {
                if self.preserve_legacy_listen {
                    validate_request_with_preserved_listen(&record.request, true)?;
                } else {
                    validate_existing_request(store, &record.request)?;
                }
                if !record.request.local_server()
                    && record.request.local_runner()
                    && !store.root().join("enrollment-recovery.json").exists()
                    && !store.root().join("runner.toml").exists()
                    && secrets.pairing_code.is_none()
                {
                    return Err(diagnostic(
                        "pairing_code_required",
                        "Joining this project machine requires a one-time pairing code",
                    ));
                }
                if record.request.local_runner()
                    && !record.request.local_server()
                    && record.runner_client_id.is_none()
                    && store.root().join("webcodex-user-token").exists()
                {
                    let token = read_secret(&store.root().join("webcodex-user-token"))?;
                    let identity = self.authenticated(record, token.expose()).await?;
                    let username = authenticated_username(&identity)?;
                    if record
                        .username
                        .as_deref()
                        .is_some_and(|existing| existing != username)
                    {
                        return Err(diagnostic(
                            "user_identity_conflict",
                            "The saved viewing identity differs from the Server user",
                        ));
                    }
                    record.username = Some(username.to_owned());
                }
                if record.request.local_runner() && record.runner_client_id.is_none() {
                    record.runner_client_id = Some(format!(
                        "{}-{}",
                        account_username(&record.request.account.name),
                        &record.environment_id.replace('-', "")[..16]
                    ));
                }
                if current_account()?.identity != record.request.account.identity {
                    return Err(diagnostic("project_user_required", "Run setup as the actual project user; service installation requests administrator authorization separately"));
                }
                if record.request.local_runner() {
                    ensure_private_directory(&store.root().join("cu"))?;
                }
                for component in [
                    record.request.local_server().then_some(Component::Server),
                    record.request.local_runner().then_some(Component::Runner),
                ]
                .into_iter()
                .flatten()
                {
                    ServiceManager::preflight(&service_spec(store, record, component)?)
                        .map_err(service_error)?;
                }
                #[cfg(windows)]
                if record.request.local_runner() {
                    // SCM holds the only retained account password. Reserve a
                    // disabled definition before spending a one-time code.
                    let credential = secrets
                        .service_password
                        .as_ref()
                        .map(|secret| ServiceCredential::from_password(secret.expose()));
                    crate::privilege::service_operation(
                        store,
                        record,
                        Component::Runner,
                        ServiceOperation::PrepareRunner,
                        credential.as_ref(),
                    )
                    .await?;
                }
                Ok(())
            }
            ServerConfiguration => {
                let directory = store.root().join("server");
                ensure_private_directory(&directory)?;
                ensure_private_directory(&directory.join("data"))?;
                let listen = match &record.request.mode {
                    EnvironmentMode::Create { listen } => listen,
                    _ => {
                        return Err(diagnostic(
                            "mode_conflict",
                            "A remote environment cannot initialize a Server",
                        ))
                    }
                };
                let content = format!("WEBCODEX_ADDR={listen}\nWEBCODEX_DATA={}\nWEBCODEX_TOKEN={}\nWEBCODEX_SHARED_KEY_ENABLED=true\n", directory.join("data").display(), generate_token("wc_boot_"));
                atomic_private_write(&directory.join("webcodex.env"), content.as_bytes())
            }
            ServerReachability => self.wait_reachable(record).await,
            UserAuthentication if record.request.local_server() => {
                self.create_user(store, record).await
            }
            UserAuthentication => {
                let token = secrets.user_token.as_ref().ok_or_else(|| diagnostic("user_credential_required", "Viewing an existing environment requires user authentication, not a Runner pairing code"))?;
                let identity = self.authenticated(record, token.expose()).await?;
                ensure_username(record, &identity)?;
                atomic_private_write(
                    &store.root().join("webcodex-user-token"),
                    token.expose().as_bytes(),
                )
            }
            RunnerEnrollment => self.enroll(store, record, secrets).await,
            RunnerConfiguration => self.configure_runner(store, record).await,
            ProjectRegistration => {
                let project =
                    project_record(record.request.project.as_ref().ok_or_else(|| {
                        diagnostic("project_required", "No project was selected")
                    })?);
                let mut value = toml::Table::new();
                value.insert("id".into(), project.id.clone().into());
                value.insert(
                    "path".into(),
                    project.path.to_string_lossy().to_string().into(),
                );
                value.insert("allow_patch".into(), true.into());
                value.insert("disabled".into(), false.into());
                atomic_private_write(
                    &store
                        .root()
                        .join("project-registry")
                        .join(format!("{}.toml", project.id)),
                    toml::to_string(&value)
                        .map_err(|_| SetupDiagnostic::io())?
                        .as_bytes(),
                )?;
                if !record.projects.contains(&project) {
                    record.projects.push(project);
                }
                Ok(())
            }
            ServerServiceInstall | RunnerServiceInstall => {
                let component = if step == ServerServiceInstall {
                    Component::Server
                } else {
                    Component::Runner
                };
                let credential = secrets
                    .service_password
                    .as_ref()
                    .map(|secret| ServiceCredential::from_password(secret.expose()));
                crate::privilege::service_operation(
                    store,
                    record,
                    component,
                    ServiceOperation::Install,
                    credential.as_ref(),
                )
                .await?;
                Ok(())
            }
            ServerServiceStart | RunnerServiceStart => {
                let component = if step == ServerServiceStart {
                    Component::Server
                } else {
                    Component::Runner
                };
                crate::privilege::service_operation(
                    store,
                    record,
                    component,
                    ServiceOperation::Start,
                    None,
                )
                .await?;
                Ok(())
            }
            Readiness => {
                let deadline = tokio::time::Instant::now() + self.readiness_timeout;
                loop {
                    let observed =
                        tokio::time::timeout_at(deadline, self.observe(store, record)).await;
                    if let Ok(Ok(observation)) = observed {
                        if observation.authenticated
                            && observation.runner_online != Some(false)
                            && record
                                .projects
                                .iter()
                                .all(|project| observation.projects_visible.contains(&project.id))
                        {
                            if store.root().join("enrollment-recovery.json").exists()
                                && runner_config_matches(store, record)?
                            {
                                std::fs::remove_file(store.root().join("enrollment-recovery.json"))
                                    .map_err(|_| SetupDiagnostic::io())?;
                            }
                            return Ok(());
                        }
                    }
                    if tokio::time::Instant::now() >= deadline {
                        return Err(diagnostic("runtime_not_ready", "Configuration is saved, but Runner or project visibility is not verified"));
                    }
                    tokio::time::sleep_until(
                        (tokio::time::Instant::now() + Duration::from_millis(300)).min(deadline),
                    )
                    .await;
                }
            }
        }
    }

    async fn observe(
        &mut self,
        store: &EnvironmentStore,
        record: &EnvironmentRecord,
    ) -> SetupResultValue<RuntimeObservation> {
        let mut observation = RuntimeObservation {
            runner_online: record.request.local_runner().then_some(false),
            ..Default::default()
        };
        let token = match read_secret(&store.root().join("webcodex-user-token")) {
            Ok(token) => token,
            Err(error) => {
                observation.diagnostics.push(error);
                return Ok(observation);
            }
        };
        match self.authenticated(record, token.expose()).await {
            Ok(overview) => observation.fleet = fleet_from_overview(&overview),
            Err(error) => {
                observation.server_reachable = matches!(
                    error.code.as_str(),
                    "authentication_required" | "permission_denied"
                );
                observation.diagnostics.push(error);
                return Ok(observation);
            }
        }
        observation.server_reachable = true;
        observation.authenticated = true;
        if let Some(client_id) = &record.runner_client_id {
            match self
                .post(
                    &record.request.server_url,
                    "/api/runtime-console/runner",
                    Some(token.expose()),
                    json!({"client_id": client_id, "project_limit": 200}),
                )
                .await
            {
                Ok(value) => {
                    observation.runner_online = Some(
                        value
                            .get("connected")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    );
                    observation.projects_visible = value
                        .get("projects")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|project| {
                            let id = project.get("id")?.as_str()?;
                            id.strip_prefix(&format!("agent:{client_id}:"))
                                .map(str::to_string)
                        })
                        .collect();
                }
                Err(error) => observation.diagnostics.push(error),
            }
        }
        Ok(observation)
    }
}

#[derive(Serialize, Deserialize)]
struct Enrollment {
    server_url: String,
    client_id: String,
    username: String,
    user_token: String,
    runner_token: String,
}
#[derive(Serialize, Deserialize)]
struct ProjectAddition {
    path: std::path::PathBuf,
    dispatched: bool,
}
#[derive(Serialize, Deserialize)]
struct ProjectRemoval {
    project: ProjectRecord,
    revision: Option<String>,
    dispatched: bool,
    confirmed: bool,
}
fn tool_output(value: &Value) -> SetupResultValue<&Value> {
    if value.get("success").and_then(Value::as_bool) != Some(true) {
        return Err(diagnostic(
            "runner_operation_rejected",
            "The Runner did not confirm the requested operation",
        ));
    }
    value
        .get("output")
        .ok_or_else(|| diagnostic("response_invalid", "Runner output is missing"))
}
fn service_diagnostic(name: &str, status: &service::ServiceStatus) -> SetupDiagnostic {
    let ownership = format!("{:?}", status.ownership).to_ascii_lowercase();
    let running = match status.running {
        Some(true) => "running",
        Some(false) => "stopped",
        None => "unknown",
    };
    let enabled = match status.enabled {
        Some(true) => "enabled",
        Some(false) => "disabled",
        None => "unknown",
    };
    let code = format!("service_{name}_{ownership}_{running}");
    let message = format!(
        "Local {name} service ({}): owner={ownership}, boot={enabled}, process={running}",
        status.id
    );
    // Only fixed lifecycle events go into the bounded private service log.
    // Runtime stdout/stderr and tracing fields can contain secrets and are
    // intentionally not copied into it.
    #[cfg(windows)]
    let recovery = "Inspect the private service-events.log, named SCM service with sc.exe queryex, and Windows System Event Log; arbitrary runtime stdout is not retained";
    #[cfg(target_os = "macos")]
    let recovery = "Inspect the private service-events.log and named LaunchDaemon with launchctl print system/org.webcodex.<id>; arbitrary runtime stdout is not retained";
    #[cfg(not(any(windows, target_os = "macos")))]
    let recovery = "Inspect the named systemd unit with systemctl status and journalctl -u; check Runtime Console for authenticated Runner/project state";
    SetupDiagnostic::new(&code, &message, recovery)
}

#[cfg(any(windows, target_os = "macos"))]
fn service_log_diagnostic(name: &str, spec: &ServiceSpec) -> SetupDiagnostic {
    let path = spec.working_directory.join(service::SERVICE_LOG_NAME);
    let code = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => {
            let mut safe = metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len() <= 256 * 1024;
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                safe &= metadata.file_attributes() & 0x400 == 0;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                safe &=
                    metadata.mode() & 0o077 == 0 && metadata.uid() == unsafe { libc::geteuid() };
            }
            if safe {
                "service_log_present"
            } else {
                "service_log_unsafe"
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => "service_log_missing",
        Err(_) => "service_log_unavailable",
    };
    SetupDiagnostic::new(code, &format!("Local {name} lifecycle log: {}", path.display()), "Read only fixed lifecycle events; use authenticated Runtime Console for readiness and project diagnostics")
}
fn authenticated_username(value: &Value) -> SetupResultValue<&str> {
    value
        .get("authenticated_user")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            diagnostic(
                "server_identity_contract",
                "The Server did not provide the authenticated user identity",
            )
        })
}
fn ensure_username(record: &mut EnvironmentRecord, identity: &Value) -> SetupResultValue<()> {
    // Older Servers omit this additive overview field. A viewer may still
    // use their existing user token, but attaching a Runner requires the
    // exact Server user and is checked separately during Preflight.
    let Some(username) = identity
        .get("authenticated_user")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
    else {
        return Ok(());
    };
    if record
        .username
        .as_deref()
        .is_some_and(|existing| existing != username)
    {
        return Err(diagnostic(
            "user_identity_conflict",
            "The Server credential belongs to a different user than the saved environment",
        ));
    }
    record.username = Some(username.to_owned());
    Ok(())
}
impl Enrollment {
    fn clear(&mut self) {
        unsafe {
            self.user_token.as_bytes_mut().fill(0);
            self.runner_token.as_bytes_mut().fill(0);
        }
    }
}
impl Drop for Enrollment {
    fn drop(&mut self) {
        self.clear();
    }
}

fn read_enrollment(
    store: &EnvironmentStore,
    record: &EnvironmentRecord,
) -> SetupResultValue<Option<Enrollment>> {
    let result: Option<Enrollment> = store.read_json("enrollment-recovery.json")?;
    if let Some(value) = result.as_ref() {
        if record
            .username
            .as_ref()
            .is_some_and(|user| user != &value.username)
        {
            return Err(diagnostic(
                "pairing_user_conflict",
                "Recovered Runner credentials belong to another user",
            ));
        }
        if value.server_url != record.request.server_url
            || Some(&value.client_id) != record.runner_client_id.as_ref()
        {
            return Err(diagnostic(
                "enrollment_binding_conflict",
                "Saved enrollment belongs to another Server or Runner",
            ));
        }
    }
    Ok(result)
}

fn runner_config_matches(
    store: &EnvironmentStore,
    record: &mut EnvironmentRecord,
) -> SetupResultValue<bool> {
    let path = store.root().join("runner.toml");
    if !path.exists() {
        return Ok(false);
    }
    let value: toml::Value = toml::from_str(read_secret(&path)?.expose()).map_err(|_| {
        diagnostic(
            "runner_configuration",
            "Existing Runner configuration is invalid",
        )
    })?;
    if value.get("server_url").and_then(toml::Value::as_str)
        != Some(record.request.server_url.as_str())
        || value.get("client_id").and_then(toml::Value::as_str)
            != record.runner_client_id.as_deref()
    {
        return Err(diagnostic(
            "runner_binding_conflict",
            "Existing Runner configuration belongs to another Server or Runner",
        ));
    }
    let owner = value
        .get("owner")
        .and_then(toml::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            diagnostic(
                "runner_configuration",
                "Existing Runner configuration has no user owner",
            )
        })?;
    if record
        .username
        .as_deref()
        .is_some_and(|username| username != owner)
    {
        return Err(diagnostic(
            "runner_owner_conflict",
            "Existing Runner configuration belongs to a different Server user",
        ));
    }
    record.username = Some(owner.to_owned());
    Ok(value
        .get("token")
        .and_then(toml::Value::as_str)
        .is_some_and(|token| !token.is_empty()))
}

pub fn canonical_server_url(raw: &str) -> SetupResultValue<String> {
    let mut url = url::Url::parse(raw.trim()).map_err(|_| {
        diagnostic(
            "server_url",
            "Server address must be a plain HTTP(S) origin",
        )
    })?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
    {
        return Err(diagnostic(
            "server_url",
            "Server address must be a plain HTTP(S) origin without credentials or a path",
        ));
    }
    url.set_path("");
    Ok(url.as_str().trim_end_matches('/').to_string())
}

pub fn current_account() -> SetupResultValue<LocalAccount> {
    let account = service::current_account().map_err(service_error)?;
    Ok(LocalAccount {
        name: account.name,
        identity: account.identity,
        home: account.home,
    })
}

pub fn validate_request(request: &SetupRequest) -> SetupResultValue<()> {
    validate_request_with_preserved_listen(request, false)
}

/// A saved native owner may keep its already deployed listen address. New
/// environments and requests that change this binding receive ordinary checks.
pub(crate) fn validate_existing_request(
    store: &EnvironmentStore,
    request: &SetupRequest,
) -> SetupResultValue<()> {
    let saved = store.load_environment()?;
    let preserve = if let Some(saved) = saved.filter(|saved| {
        saved.configured
            && saved.request.local_server()
            && saved.request.mode == request.mode
            && saved.request.server_url == request.server_url
            && saved.request.account == request.account
            && saved.request.binaries == request.binaries
    }) {
        let spec = service_spec(store, &saved, Component::Server)?;
        ServiceManager::inspect(&spec)
            .map_err(service_error)?
            .ownership
            == Ownership::Owned
    } else {
        false
    };
    validate_request_with_preserved_listen(request, preserve)
}

pub(crate) fn validate_request_with_preserved_listen(
    request: &SetupRequest,
    preserve: bool,
) -> SetupResultValue<()> {
    if canonical_server_url(&request.server_url)? != request.server_url {
        return Err(diagnostic("server_url", "Use the canonical Server address"));
    }
    if let Some(project) = &request.project {
        webcodex_runner_config::paths::validate_project_path_ingress(project)
            .map_err(|_| diagnostic("project_path", "This project path is not supported"))?;
        let canonical = project.canonicalize().map_err(|_| {
            diagnostic(
                "project_path",
                "The selected project directory is unavailable",
            )
        })?;
        if canonical != *project || !canonical.is_dir() {
            return Err(diagnostic(
                "project_path",
                "Select the canonical existing project directory",
            ));
        }
        webcodex_runner_config::paths::validate_project_path_policy(
            project,
            &[project.clone()],
            false,
        )
        .map_err(|_| {
            diagnostic(
                "project_policy",
                "The selected directory is not an allowed project root",
            )
        })?;
        if request.account.identity == "0" {
            return Err(diagnostic(
                "project_user_required",
                "Persistent Runner must run as the user who owns the projects",
            ));
        }
    }
    if let EnvironmentMode::Create { listen } = &request.mode {
        let socket: std::net::SocketAddr = listen
            .parse()
            .map_err(|_| diagnostic("listen_address", "The Server listen address is invalid"))?;
        if socket.port() == 0 || (!preserve && !socket.ip().is_loopback()) {
            return Err(diagnostic("listen_address", "Initial setup uses a fixed loopback address; configure network exposure explicitly"));
        }
        if !preserve && canonical_server_url(&format!("http://{socket}"))? != request.server_url {
            return Err(diagnostic(
                "server_binding_conflict",
                "The local Server URL and listen address disagree",
            ));
        }
    }
    Ok(())
}

pub fn service_spec(
    store: &EnvironmentStore,
    record: &EnvironmentRecord,
    component: Component,
) -> SetupResultValue<ServiceSpec> {
    if component == Component::Tunnel {
        return crate::tunnel_service_spec(store, record, "default");
    }
    let account = &record.request.account;
    let server = component == Component::Server;
    let id = if cfg!(windows) {
        match component {
            Component::Server => "WebCodexServer".into(),
            Component::Runner => format!(
                "WebCodexRunner-{}",
                &format!("{:x}", Sha256::digest(account.identity.as_bytes()))[..12]
            ),
            Component::Tunnel => "WebCodexTunnel".into(),
        }
    } else {
        match component {
            Component::Server => "webcodex".into(),
            Component::Runner => format!("webcodex-runner-{}", account.identity),
            Component::Tunnel => "webcodex-tunnel".into(),
        }
    };
    let account = if cfg!(windows) && component != Component::Runner {
        ServiceAccount::WindowsVirtual {
            name: format!("NT SERVICE\\{id}"),
        }
    } else {
        ServiceAccount::SystemUser {
            name: account.name.clone(),
            group: None,
            expected_identity: account.identity.clone(),
            home: Some(account.home.clone()),
        }
    };
    let env_file =
        (component != Component::Runner).then(|| store.root().join("server/webcodex.env"));
    let (program, mut args) = match component {
        Component::Runner => (
            record.request.binaries.runner.clone(),
            vec![
                "--config".into(),
                store
                    .root()
                    .join("runner.toml")
                    .to_string_lossy()
                    .into_owned(),
                "--computer-session-dir".into(),
                store.root().join("cu").to_string_lossy().into_owned(),
            ],
        ),
        Component::Server if cfg!(windows) => (
            record.request.binaries.server.clone(),
            vec![
                "--env-file".into(),
                env_file.as_ref().unwrap().to_string_lossy().into_owned(),
            ],
        ),
        Component::Server | Component::Tunnel => (
            record.request.binaries.cli.clone(),
            vec![
                "server".into(),
                if server { "run" } else { "tunnel" }.into(),
                "--env-file".into(),
                env_file.as_ref().unwrap().to_string_lossy().into_owned(),
            ],
        ),
    };
    if component == Component::Tunnel {
        args.extend(["--provider".into(), "openai".into(), "--json".into()]);
    }
    if cfg!(windows) {
        args.splice(0..0, ["--windows-service".into(), id.clone()]);
    }
    let working_directory = if component != Component::Runner {
        store.root().join("server")
    } else {
        store.root().to_path_buf()
    };
    let environment = if cfg!(target_os = "macos") {
        BTreeMap::from([(
            "WEBCODEX_SERVICE_LOG_DIR".into(),
            working_directory.to_string_lossy().into_owned(),
        )])
    } else {
        BTreeMap::new()
    };
    Ok(ServiceSpec {
        id,
        component,
        program,
        args,
        working_directory,
        account,
        config_identity: record.environment_id.clone(),
        env_file,
        environment,
        linux_socket: if cfg!(target_os = "linux") && server {
            match &record.request.mode {
                EnvironmentMode::Create { listen } => Some(LinuxSocketSpec {
                    listen: listen.clone(),
                }),
                _ => None,
            }
        } else {
            None
        },
    })
}

fn diagnostic(code: &str, message: &str) -> SetupDiagnostic {
    SetupDiagnostic::new(code, message, "Inspect environment status and resume the saved setup after correcting the reported condition")
}
pub(crate) fn service_error(error: service::ServiceError) -> SetupDiagnostic {
    SetupDiagnostic::new(
        &format!("service_{:?}", error.code).to_ascii_lowercase(),
        &error.message,
        error.recovery_hint,
    )
}
fn response_string(value: &Value, key: &str) -> SetupResultValue<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty() && v.len() <= 8192)
        .map(str::to_owned)
        .ok_or_else(|| {
            diagnostic(
                "response_invalid",
                "The Server response omitted a required identity field",
            )
        })
}
pub fn read_secret(path: &Path) -> SetupResultValue<Secret> {
    String::from_utf8(read_private(path)?)
        .map(|value| Secret::new(value.trim().to_string()))
        .map_err(|_| SetupDiagnostic::io())
}
fn generate_token(prefix: &str) -> String {
    format!(
        "{prefix}{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}
fn account_username(name: &str) -> String {
    let value: String = name
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or("user")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        .take(48)
        .collect();
    if value.is_empty() {
        "user".into()
    } else {
        value
    }
}
fn project_record(path: &Path) -> ProjectRecord {
    ProjectRecord {
        id: format!(
            "project-{}",
            &format!("{:x}", Sha256::digest(path.to_string_lossy().as_bytes()))[..16]
        ),
        path: path.to_path_buf(),
    }
}
pub(crate) fn bootstrap_token(store: &EnvironmentStore) -> SetupResultValue<Secret> {
    env_value(
        read_secret(&store.root().join("server/webcodex.env"))?.expose(),
        "WEBCODEX_TOKEN",
    )
    .map(Secret::new)
    .ok_or_else(|| {
        diagnostic(
            "server_credentials",
            "The saved Server bootstrap credential is missing",
        )
    })
}
pub(crate) fn env_value(content: &str, key: &str) -> Option<String> {
    content
        .lines()
        .filter_map(|line| line.split_once('='))
        .find(|(name, _)| name.trim() == key)
        .map(|(_, value)| {
            value
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string()
        })
}

#[cfg(test)]
#[path = "native/tests.rs"]
mod tests;

/// Project only the established Runtime Console fields. Do not forward opaque
/// server JSON (or interpret its project paths as paths on the setup machine).
fn fleet_from_overview(value: &Value) -> Option<RuntimeFleet> {
    let mut runners = value
        .get("runners")?
        .as_array()?
        .iter()
        .map(|entry| serde_json::from_value::<FleetRunner>(entry.clone()))
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    for runner in &mut runners {
        if !runner.connected || runner.status.as_deref() == Some("stale") {
            runner.computer_session_availability =
                runner.computer_session_availability.map(|_| false);
        }
    }
    let projects = value
        .get("projects")?
        .as_array()?
        .iter()
        .map(|entry| serde_json::from_value::<FleetProject>(entry.clone()))
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    Some(RuntimeFleet {
        observed_at_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX),
        projects_available: value
            .get("projects_available")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        projects_truncated: value
            .get("projects_truncated")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        runners,
        projects,
    })
}
