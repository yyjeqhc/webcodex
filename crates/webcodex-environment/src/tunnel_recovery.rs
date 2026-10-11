//! Operator-only exact-profile recovery. This uses the existing service manager;
//! it never stops a live owner or replays an unconfirmed operation.
use crate::{
    native::service_error,
    service::{Component, Ownership, ServiceManager, ServiceScope},
    *,
};
use serde::Serialize;
use webcodex_openai_tunnel::run_fence;
const ORIGIN: &str = "https://api.openai.com";

#[derive(Debug, Default)]
pub struct TunnelRecoveryRequest {
    pub apply: bool,
    pub accept_uncertain_effects: bool,
    pub expected_revision: Option<u64>,
    pub expected_environment_id: Option<String>,
    pub expected_run_id: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct TunnelRecoveryObservation {
    pub profile_id: String,
    pub environment_id: Option<String>,
    pub revision: u64,
    pub run_id: Option<String>,
    pub owner_pid: Option<u32>,
    pub status: String,
    pub owner_service: Option<service::ServiceStatus>,
    pub standalone_service: Option<service::ServiceStatus>,
    pub next_action: String,
}
fn error(code: &str) -> SetupDiagnostic {
    SetupDiagnostic::new(code, "Exact-profile Tunnel recovery did not complete", match code {
        "tunnel_legacy_owner_unverifiable" => "The v1 fence has no owner identity. Preserve it, reconcile prior effects and establish the old owner's termination separately; automatic archival is unavailable",
        "tunnel_owner_active" => "Stop the exact owning service through its existing lifecycle and verify active work has ended; recovery never stops a live owner",
        "tunnel_recovery_confirmation_required" => "Diagnose first; apply requires the exact Environment ID, profile revision and run ID plus explicit acceptance of uncertain prior effects",
        "not_configured" => "Catalog-only configurations support read-only diagnosis; no managed owner may be invented for recovery",
        _ => "Re-run read-only recovery diagnosis for this Environment and Profile; retain the fence/archive and do not replay uncertain writes",
    })
}
fn owner_absent(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let Ok(pid) = i32::try_from(pid) else {
            return false;
        };
        // EPERM, a reused PID, or any unknown result must remain occupied.
        unsafe {
            libc::kill(pid, 0) == -1
                && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}
fn stopped_owned(status: &service::ServiceStatus) -> bool {
    status.ownership == Ownership::Owned && status.running == Some(false)
}
trait RecoveryServices {
    fn inspect(
        &self,
        spec: &service::ServiceSpec,
    ) -> Result<service::ServiceStatus, service::ServiceError>;
    fn start(
        &self,
        spec: &service::ServiceSpec,
    ) -> Result<service::ServiceStatus, service::ServiceError>;
    fn running_pid(
        &self,
        spec: &service::ServiceSpec,
    ) -> Result<Option<u32>, service::ServiceError>;
    fn owner_absent(&self, pid: u32) -> bool;
}
struct NativeRecoveryServices;
impl RecoveryServices for NativeRecoveryServices {
    fn inspect(
        &self,
        spec: &service::ServiceSpec,
    ) -> Result<service::ServiceStatus, service::ServiceError> {
        ServiceManager::inspect(spec)
    }
    fn start(
        &self,
        spec: &service::ServiceSpec,
    ) -> Result<service::ServiceStatus, service::ServiceError> {
        ServiceManager::start(spec)
    }
    fn running_pid(
        &self,
        spec: &service::ServiceSpec,
    ) -> Result<Option<u32>, service::ServiceError> {
        ServiceManager::running_pid(spec)
    }
    fn owner_absent(&self, pid: u32) -> bool {
        owner_absent(pid)
    }
}

impl NativeEnvironment {
    pub async fn recover_tunnel(
        &self,
        store: &EnvironmentStore,
        profile_id: &str,
        request: TunnelRecoveryRequest,
    ) -> SetupResultValue<TunnelRecoveryObservation> {
        let root = run_fence::default_root().map_err(error)?;
        recover(store, profile_id, request, &root, &NativeRecoveryServices).await
    }
}
async fn recover(
    store: &EnvironmentStore,
    profile_id: &str,
    request: TunnelRecoveryRequest,
    root: &std::path::Path,
    services: &impl RecoveryServices,
) -> SetupResultValue<TunnelRecoveryObservation> {
    crate::tunnel::validate_id(profile_id)?;
    let _lock = store.lock()?;
    ensure_upgrade_idle_under_lock(store)?;
    let profiles = tunnel_profiles(store)?;
    crate::tunnel::validate_catalog(&profiles)?;
    let _ = tunnel_profile_snapshots(store)?;
    let profile = profiles
        .iter()
        .find(|p| p.profile_id == profile_id)
        .ok_or_else(|| error("tunnel_profile"))?;
    if profile.provider != TunnelProvider::Openai {
        return Err(error("tunnel_recovery_provider"));
    }
    let binding = crate::tunnel::tunnel_profile_binding(store, profile_id)?;
    let environment = store.load_environment()?;
    let fence = run_fence::observe(root, ORIGIN, binding.tunnel_id.expose());
    let mut observation = TunnelRecoveryObservation {
            profile_id: profile_id.to_owned(), environment_id: environment.as_ref().map(|e| e.environment_id.clone()),
            revision: profile.revision, run_id: None, owner_pid: None,
            status: "tunnel_fence_missing".into(),
            owner_service: None, standalone_service: None,
            next_action: "No recovery was performed; inspect Tunnel diagnostics before starting its existing owner".into(),
        };
    match &fence {
        Ok(Some(identity)) => {
            observation.run_id = Some(identity.run_id.clone());
            observation.owner_pid = Some(identity.owner_pid);
            observation.status = if services.owner_absent(identity.owner_pid) {
                "tunnel_restart_uncertain"
            } else {
                "tunnel_owner_active"
            }
            .into();
            observation.next_action = "Reconcile prior effects, stop the exact owner using the existing service lifecycle, then apply with the observed Environment ID, revision, run ID and --accept-uncertain-effects".into();
        }
        Err(code) => {
            observation.status = (*code).into();
            observation.next_action = error(code).recovery;
        }
        Ok(None) => {}
    }
    if let Some(environment) = environment.as_ref() {
        let standalone = tunnel_service_spec(store, environment, profile_id)?;
        observation.standalone_service =
            Some(services.inspect(&standalone).map_err(service_error)?);
        let owner = if profile.host_mode == TunnelHostMode::Embedded {
            crate::service_spec(store, environment, Component::Server)?
        } else {
            standalone
        };
        observation.owner_service = Some(services.inspect(&owner).map_err(service_error)?);
    } else {
        observation.next_action = error("not_configured").recovery;
    }
    if !request.apply {
        return Ok(observation);
    }
    let environment = environment.ok_or_else(|| error("not_configured"))?;
    let identity = fence
        .map_err(error)?
        .ok_or_else(|| error("tunnel_fence_missing"))?;
    if !request.accept_uncertain_effects
        || request.expected_revision != Some(profile.revision)
        || request.expected_environment_id.as_deref() != Some(&environment.environment_id)
        || request.expected_run_id.as_deref() != observation.run_id.as_deref()
        || request.expected_run_id.is_none()
    {
        return Err(error("tunnel_recovery_confirmation_required"));
    }
    if !cfg!(unix)
        || environment.request.service_scope != ServiceScope::User
        || !environment.request.local_server()
    {
        return Err(error("tunnel_recovery_owner_unsupported"));
    }
    let standalone = tunnel_service_spec(store, &environment, profile_id)?;
    let owner = if profile.host_mode == TunnelHostMode::Embedded {
        if !profile.autostart {
            return Err(error("tunnel_recovery_autostart_disabled"));
        }
        if services
            .inspect(&standalone)
            .map_err(service_error)?
            .ownership
            != Ownership::Absent
        {
            return Err(error("tunnel_recovery_other_owner"));
        }
        crate::service_spec(store, &environment, Component::Server)?
    } else {
        standalone.clone()
    };
    if !stopped_owned(&services.inspect(&owner).map_err(service_error)?) {
        return Err(error("tunnel_owner_active"));
    }
    // The identity lock excludes other recoveries and live Tunnel tasks. The
    // Environment lock fences profile edits throughout archive/start/verify.
    run_fence::archive(
        root,
        ORIGIN,
        binding.tunnel_id.expose(),
        &identity.run_id,
        |pid| {
            services.owner_absent(pid)
                && services.inspect(&owner).is_ok_and(|s| stopped_owned(&s))
                && (profile.host_mode != TunnelHostMode::Embedded
                    || services
                        .inspect(&standalone)
                        .is_ok_and(|s| s.ownership == Ownership::Absent))
        },
    )
    .map_err(error)?;
    if !stopped_owned(&services.inspect(&owner).map_err(service_error)?) {
        return Err(error("tunnel_recovery_owner_changed"));
    }
    services.start(&owner).map_err(service_error)?;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(65);
    let mut started_pid = None;
    loop {
        let pid = services.running_pid(&owner).map_err(service_error)?;
        if started_pid.is_some() && pid != started_pid {
            return Err(error("tunnel_recovery_owner_changed"));
        }
        if pid.is_some() {
            started_pid = pid;
        }
        let health =
            crate::tunnel::read_tunnel_health(&standalone.working_directory.join("readiness.json"))
                .ok();
        let revision = (profile.host_mode == TunnelHostMode::Embedded)
            .then(|| profile.effective_runtime_revision());
        if crate::tunnel::health_status(health.as_ref(), revision, pid) == "current"
            && health.is_some_and(|h| h.tunnel_ready && h.local_mcp_ready)
        {
            if services.running_pid(&owner).map_err(service_error)? != pid {
                return Err(error("tunnel_recovery_owner_changed"));
            }
            observation.owner_service = Some(services.inspect(&owner).map_err(service_error)?);
            observation.status = "ready".into();
            observation.next_action = "Recovery completed; the original run marker is archived. No prior operation was replayed".into();
            return Ok(observation);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(error("tunnel_recovery_not_ready"));
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}

#[cfg(test)]
#[path = "tunnel_recovery/tests.rs"]
mod tests;
