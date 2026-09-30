use super::*;
use webcodex_environment::{
    current_account,
    service::{Component, Ownership, ServiceManager, ServiceSpec},
    service_spec, EnvironmentLock, EnvironmentStore,
};

pub(super) struct ManagedTraceTarget {
    pub path: PathBuf,
    pub can_restart: bool,
    environment_id: Option<String>,
    // Keep the installer/setup fence until the atomic file update is complete.
    _environment_lock: Option<EnvironmentLock>,
}

impl ManagedTraceTarget {
    fn bind_revision(&self, revision: &str) -> String {
        let identity = serde_json::json!([self.path, self.environment_id, revision]);
        format!("{:x}", Sha256::digest(identity.to_string().as_bytes()))
    }

    pub(super) fn inspect(&self) -> DesktopResult<TraceSettings> {
        let mut trace = diagnostics::inspect_trace(&self.path, self.can_restart)?;
        trace.revision = self.bind_revision(&trace.revision);
        Ok(trace)
    }

    pub(super) fn update(
        &self,
        mode: TraceMode,
        expected_revision: &str,
        confirm_full: bool,
    ) -> DesktopResult<TraceSettings> {
        let current = diagnostics::inspect_trace(&self.path, self.can_restart)?;
        if self.bind_revision(&current.revision) != expected_revision {
            return Err(diagnostics::diagnostic_error("server_environment_changed"));
        }
        let mut trace =
            diagnostics::update_trace(&self.path, mode, &current.revision, confirm_full)?;
        trace.revision = self.bind_revision(&trace.revision);
        Ok(trace)
    }
}

pub(super) fn effective_trace_mode(overview: &Value) -> Option<TraceMode> {
    overview
        .pointer("/effective_config/tool_request_trace_mode")
        .and_then(Value::as_str)
        .and_then(TraceMode::parse)
}

fn persistent_trace_target(
    store: EnvironmentStore,
    environment_id: &str,
    runtime: &StoredRuntime,
    inspect: impl FnOnce(&ServiceSpec) -> DesktopResult<bool>,
) -> DesktopResult<ManagedTraceTarget> {
    let lock = store.lock().map_err(environment::desktop_error)?;
    webcodex_environment::ensure_upgrade_idle_under_lock(&store)
        .map_err(environment::desktop_error)?;
    let record = store
        .load_environment()
        .map_err(environment::desktop_error)?
        .ok_or_else(|| diagnostics::diagnostic_error("server_environment_unavailable"))?;
    if record.environment_id != environment_id
        || !record.configured
        || record.request.account.identity
            != current_account()
                .map_err(environment::desktop_error)?
                .identity
        || record.request.server_url != runtime.server_url
    {
        return Err(diagnostics::diagnostic_error("diagnostic_identity_changed"));
    }
    if !record.request.local_server() {
        return Err(diagnostics::diagnostic_error("server_not_owned"));
    }
    if record.request.service_scope.is_system() {
        // System configuration stays with the administrator/service-account
        // broker; a current-user edit must not bypass that authority.
        return Err(diagnostics::diagnostic_error(
            "trace_system_service_read_only",
        ));
    }
    let path = store.root().join("server/webcodex.env");
    if runtime.server_env_file.as_ref() != Some(&path) {
        return Err(diagnostics::diagnostic_error(
            "server_environment_not_managed",
        ));
    }
    let spec =
        service_spec(&store, &record, Component::Server).map_err(environment::desktop_error)?;
    if spec.env_file.as_ref() != Some(&path) || !inspect(&spec)? {
        return Err(diagnostics::diagnostic_error("server_not_owned"));
    }
    let parent = path.parent().expect("fixed Server directory");
    if !parent.is_dir() {
        return Err(diagnostics::diagnostic_error(
            "server_environment_unavailable",
        ));
    }
    // Reject linked ancestors and verify the private directory from which a
    // replacement file will inherit permissions; never create a new target.
    EnvironmentStore::open(parent.to_path_buf()).map_err(environment::desktop_error)?;
    // Verify private ownership/permissions before reading or replacing secrets.
    webcodex_environment::read_secret(&path).map_err(environment::desktop_error)?;
    Ok(ManagedTraceTarget {
        path,
        can_restart: true,
        environment_id: Some(environment_id.into()),
        _environment_lock: Some(lock),
    })
}

// Navigation only: do not read environment contents, acquire edit authority, or
// change ACLs merely to reveal a known local configuration directory.
fn configuration_location(
    local: bool,
    runtime: Option<&StoredRuntime>,
    expected: &std::path::Path,
) -> DesktopResult<PathBuf> {
    if !local {
        return Err(diagnostics::diagnostic_error("server_not_owned"));
    }
    let path = runtime.and_then(|runtime| runtime.server_env_file.as_deref());
    if path != Some(expected) {
        return Err(diagnostics::diagnostic_error(
            "server_environment_not_managed",
        ));
    }
    expected
        .parent()
        .filter(|parent| parent.is_dir())
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| diagnostics::diagnostic_error("server_environment_unavailable"))
}

impl DesktopCore {
    pub(super) fn server_configuration_location(&self) -> DesktopResult<PathBuf> {
        let expected = if self.config.persistent_environment.is_some() {
            webcodex_environment::default_environment_dir()
                .map_err(environment::desktop_error)?
                .join("server/webcodex.env")
        } else {
            self.data_dir.join("runtime/local/webcodex.env")
        };
        configuration_location(
            matches!(
                self.config.topology.as_ref().map(|t| &t.server),
                Some(ServerTopology::Local)
            ),
            self.config.runtime.as_ref(),
            &expected,
        )
    }

    pub(super) async fn managed_trace_target(&self) -> DesktopResult<ManagedTraceTarget> {
        if self.configuration_issue.is_some() {
            return Err(diagnostics::diagnostic_error(
                "configuration_migration_failed",
            ));
        }
        if let Some(environment_id) = self.config.persistent_environment.clone() {
            let runtime =
                self.config.runtime.clone().ok_or_else(|| {
                    diagnostics::diagnostic_error("server_environment_unavailable")
                })?;
            let store = environment::store()?;
            return tokio::task::spawn_blocking(move || {
                persistent_trace_target(store, &environment_id, &runtime, |spec| {
                    ServiceManager::inspect(spec)
                        .map(|status| status.ownership == Ownership::Owned)
                        .map_err(|_| diagnostics::diagnostic_error("server_not_owned"))
                })
            })
            .await
            .map_err(|_| diagnostics::diagnostic_error("server_environment_unavailable"))?;
        }
        self.local_trace_target(self.process_snapshot(ProcessKey::LocalServer).await)
    }

    fn local_trace_target(
        &self,
        server: Option<crate::process::ProcessSnapshot>,
    ) -> DesktopResult<ManagedTraceTarget> {
        // Editing this file does not switch binaries or control the Runner.
        // A stopped Server does not remove authority over its managed config.
        if server.as_ref().is_some_and(|p| !p.owned_by_desktop) {
            return Err(diagnostics::diagnostic_error("server_not_owned"));
        }
        let path = self.managed_server_environment()?;
        let parent = path.parent().expect("fixed Server directory");
        EnvironmentStore::open(parent.to_path_buf()).map_err(environment::desktop_error)?;
        webcodex_environment::read_secret(&path).map_err(environment::desktop_error)?;
        Ok(ManagedTraceTarget {
            path,
            can_restart: server.is_some_and(|p| p.owned_by_desktop),
            environment_id: None,
            _environment_lock: None,
        })
    }
}

#[cfg(test)]
mod tests;
