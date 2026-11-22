//! Interactive Desktop intent. Core owns the catalog and Server owns the process,
//! forwarding and OAuth discovery. Never infer success from saving the profile.
use super::*;
use webcodex_environment::{
    CloudflareProfileTarget, CloudflareTunnelProfileRequest, EnvironmentStore, NativeEnvironment,
    TunnelHostMode, TunnelProfileSnapshot,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeminiConnectionRequest {
    environment_id: String,
    profile: Option<SelectedProfile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectedProfile {
    id: String,
    configuration_id: String,
    revision: u64,
}

impl AppState {
    pub async fn connect_gemini(
        &self,
        request: GeminiConnectionRequest,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancel, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::TunnelConfigUpdate, false)
            .await?;
        let result = async {
            cancel.check()?;
            if core.config.persistent_environment.as_deref() != Some(&request.environment_id)
                || !core.config.topology.as_ref().is_some_and(|t| {
                    t.experience == Experience::Full
                        && matches!(t.server, ServerTopology::Local)
                        && matches!(t.runner, RunnerTopology::Local)
                })
            {
                return Err(requires_environment());
            }
            let store = environment::store()?;
            core.tunnel_config
                .ensure_persistent_catalog_compatible(&store)?;
            if !core.snapshot.readiness.runtime_ready {
                core.resume_environment(&cancel).await?;
            }
            cancel.check()?;
            let profile = prepare_profile(&store, &request).await?;
            core.publish_snapshot();
            let id = &profile.profile_id;
            let observed = self
                .cloudflare_connection(CloudflareConnectionRequest::Status {
                    profile_id: id.clone(),
                })
                .await?;
            let CloudflareConnectionResponse::Status(status) = observed else {
                return Err(unavailable());
            };
            if status.configured_revision != profile.revision
                || profile.configuration_id.as_deref() != Some(status.configuration_id.as_str())
            {
                return Err(stale_revision("cloudflare_start", id));
            }
            if matches!(status.lifecycle.as_str(), "stopped" | "error") {
                cancel.check()?;
                // A user-scoped standalone host has the same per-user cache.
                // Boot and system accounts continue to use installed tools only.
                if profile.host_mode == TunnelHostMode::Standalone && profile.autostart {
                    let record = store
                        .load_environment()
                        .map_err(environment::desktop_error)?
                        .ok_or_else(requires_environment)?;
                    if record.environment_id != request.environment_id {
                        return Err(requires_environment());
                    }
                    if record.request.service_scope
                        == webcodex_environment::service::ServiceScope::User
                    {
                        if webcodex_environment::current_account()
                            .map_err(environment::desktop_error)?
                            .identity
                            != record.request.account.identity
                        {
                            return Err(requires_environment());
                        }
                        prepare_standalone_component(&core.adapter.binaries()?.webcodex, &cancel)
                            .await?;
                    }
                }
                // One mutation from a current observation; response loss never
                // causes a retry. The card subsequently polls only Status.
                self.cloudflare_connection(CloudflareConnectionRequest::Start {
                    profile_id: id.clone(),
                    server_instance_id: status.server_instance_id,
                    expected_revision: profile.revision,
                    expected_generation: status.process_generation,
                    expected_configuration_id: profile
                        .configuration_id
                        .clone()
                        .ok_or_else(requires_environment)?,
                })
                .await?;
            }
            core.get_state().await
        }
        .await;
        self.finish_operation(operation, cancel, core, baseline, result)
            .await
    }
}

async fn prepare_profile(
    store: &EnvironmentStore,
    request: &GeminiConnectionRequest,
) -> DesktopResult<TunnelProfileSnapshot> {
    let record = store
        .load_environment()
        .map_err(environment::desktop_error)?
        .ok_or_else(requires_environment)?;
    if record.environment_id != request.environment_id
        || !record.request.local_server()
        || !record.request.local_runner()
    {
        return Err(requires_environment());
    }
    let catalog = webcodex_environment::tunnel_profile_snapshots(store)
        .map_err(environment::desktop_error)?;
    let existing = match &request.profile {
        Some(target) => Some(
            catalog
                .iter()
                .find(|p| {
                    p.profile_id == target.id
                        && p.provider != TunnelProvider::Openai
                        && p.configuration_id.as_deref() == Some(&target.configuration_id)
                        && p.revision == target.revision
                })
                .ok_or_else(|| stale_revision("cloudflare_start", &target.id))?,
        ),
        None => None,
    };
    if let Some(profile) = existing {
        if profile.oauth_dynamic_registration_enabled {
            return Ok(profile.clone());
        }
    }
    let target = CloudflareProfileTarget {
        environment_id: request.environment_id.clone(),
        configuration_id: existing.and_then(|p| p.configuration_id.clone()),
    };
    // Create-only identity for the wizard default. Two concurrent first-time
    // connects cannot create two Quick profiles or adopt an intervening record.
    let id = existing.map_or("gemini-quick", |p| p.profile_id.as_str());
    let native = NativeEnvironment::new().map_err(environment::desktop_error)?;
    let result = native
        .configure_cloudflare_tunnel_profile(
            store,
            &CloudflareTunnelProfileRequest {
                profile_id: id,
                expected_target: Some(&target),
                expected_revision: existing.map(|p| p.revision),
                name: Some(existing.map_or("Gemini", |p| p.name.as_str())),
                provider: existing.map_or(TunnelProvider::CloudflareQuick, |p| p.provider.clone()),
                host_mode: existing.map_or(TunnelHostMode::Embedded, |p| p.host_mode),
                autostart: existing.map_or_else(
                    || {
                        !catalog
                            .iter()
                            .any(|p| p.provider != TunnelProvider::Openai && p.autostart)
                    },
                    |p| p.autostart,
                ),
                oauth_dynamic_registration_enabled: Some(true),
                token: None,
                ingress_port: None,
            },
        )
        .await
        .map_err(environment::desktop_error)?;
    Ok(result.profile)
}

async fn prepare_standalone_component(
    cli: &std::path::Path,
    cancel: &CancellationContext,
) -> DesktopResult<()> {
    use crate::webcodex::cli::{run_json_until, CliCommandContext};
    let prepared: bool = run_json_until(
        cli,
        &[
            "environment".into(),
            "prepare-cloudflared".into(),
            "--json".into(),
        ],
        None,
        false,
        CliCommandContext::new("cloudflare_start", "prepare-cloudflared"),
        cancel,
        Deadline::after(std::time::Duration::from_secs(170)),
    )
    .await
    .map_err(|error| {
        let reason = error
            .diagnostic_ref()
            .and_then(|d| d.reason_code.as_deref());
        if let Some(
            code @ ("cloudflare_network_unavailable"
            | "cloudflare_download_failed"
            | "cloudflare_proxy_invalid"
            | "cloudflare_storage_unavailable"
            | "cloudflare_version_unsupported"
            | "cloudflare_platform_unsupported"
            | "cloudflare_verification_failed"
            | "cloudflare_extraction_failed"
            | "cloudflare_binary_unavailable"),
        ) = reason
        {
            DesktopError::new(
                code,
                "The Cloudflare component could not be prepared",
                "Review the component diagnostic and reconnect.",
            )
            .with_stage("cloudflare_start")
        } else {
            error
        }
    })?;
    if !prepared {
        return Err(unavailable());
    }
    Ok(())
}
