fn tunnel_apply_error(cause: DesktopError) -> DesktopError {
    DesktopError::new("tunnel_config_apply_failed", "Configuration saved; the secure tunnel needs recovery",
        "The new credentials are saved. Retry starting the secure tunnel; Server and Runner were not restarted.")
        .with_details(serde_json::json!({"configuration_saved": true, "cause_code": cause.code}))
}

fn exposure_readiness(topology: Option<&RuntimeTopology>) -> ExposureReadiness {
    match topology.map(|topology| &topology.exposure) {
        Some(Exposure::None) => ExposureReadiness::LocalReady,
        // An HTTPS origin is a configured route, not evidence that the MCP
        // endpoint plus ChatGPT authentication/handoff is externally usable.
        Some(Exposure::ExistingHttps { .. }) => ExposureReadiness::Unknown,
        Some(Exposure::Cloudflare | Exposure::OpenAiTunnel) => ExposureReadiness::Unknown,
        None => ExposureReadiness::Unknown,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EffectiveTunnelProxy {
    url: Option<String>,
    source: &'static str,
    system_proxy_detected: bool,
}

fn validate_tunnel_proxy_url(value: &str) -> DesktopResult<String> {
    crate::platform::normalize_proxy_server(value).ok_or_else(|| {
        DesktopError::new(
            "tunnel_proxy_invalid",
            "The Tunnel proxy must be an HTTP or HTTPS proxy URL without embedded credentials",
            "Use a value such as http://127.0.0.1:7890, or choose automatic proxy detection.",
        )
    })
}

fn environment_tunnel_proxy() -> Option<String> {
    ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"]
        .iter()
        .find_map(|name| {
            std::env::var(name)
                .ok()
                .and_then(|value| crate::platform::normalize_proxy_server(&value))
        })
}

fn effective_tunnel_proxy(config: &TunnelProxyConfig) -> DesktopResult<EffectiveTunnelProxy> {
    resolve_tunnel_proxy(
        config,
        environment_tunnel_proxy(),
        crate::platform::system_http_proxy_candidate(),
    )
}

fn resolve_tunnel_proxy(
    config: &TunnelProxyConfig,
    environment: Option<String>,
    system: Option<crate::platform::SystemProxyCandidate>,
) -> DesktopResult<EffectiveTunnelProxy> {
    let system_proxy_detected = system.is_some();
    match config.mode {
        TunnelProxyMode::Direct => Ok(EffectiveTunnelProxy {
            url: None,
            source: "direct",
            system_proxy_detected,
        }),
        TunnelProxyMode::Custom => Ok(EffectiveTunnelProxy {
            url: Some(validate_tunnel_proxy_url(
                config.custom_url.as_deref().unwrap_or(""),
            )?),
            source: "custom",
            system_proxy_detected,
        }),
        TunnelProxyMode::Auto => {
            if let Some(url) = environment {
                return Ok(EffectiveTunnelProxy {
                    url: Some(url),
                    source: "environment",
                    system_proxy_detected,
                });
            }
            if let Some(candidate) = system {
                return Ok(EffectiveTunnelProxy {
                    url: Some(candidate.url),
                    source: "system",
                    system_proxy_detected,
                });
            }
            Ok(EffectiveTunnelProxy {
                url: None,
                source: "direct",
                system_proxy_detected,
            })
        }
    }
}

fn runtime_autostart(config: &StoredDesktopConfig) -> bool {
    config.runtime_autostart.unwrap_or_else(|| {
        config.runtime.is_some()
            && config
                .topology
                .as_ref()
                .is_some_and(|topology| topology.experience == Experience::Full)
    })
}

fn preferred_connection(config: &StoredDesktopConfig) -> RegularConnectionPreference {
    config.preferred_connection.unwrap_or_default()
}

fn apply_config_projection(snapshot: &mut DesktopStateSnapshot, config: &StoredDesktopConfig) {
    snapshot.persistent_environment = config.persistent_environment.clone();
    snapshot.can_repair_runner_credential = cfg!(windows)
        && config
            .persistent_environment
            .as_deref()
            .is_some_and(environment::runner_uses_system_service)
        && config
            .topology
            .as_ref()
            .is_some_and(|topology| topology.runner == RunnerTopology::Local);
    snapshot.workspace_runner = config
        .runtime
        .as_ref()
        .and_then(|runtime| crate::webcodex::settings::target(runtime).ok());
    snapshot.saved_projects = config
        .saved_projects
        .iter()
        .filter(|entry| {
            config
                .runtime
                .as_ref()
                .and_then(|r| r.runner_config.as_ref())
                == Some(&entry.runner_config)
        })
        .map(|entry| entry.project.clone())
        .collect();
    snapshot.runtime_autostart = runtime_autostart(config);
    snapshot.preferred_connection = preferred_connection(config);
    snapshot.tunnel_proxy = match effective_tunnel_proxy(&config.tunnel_proxy) {
        Ok(proxy) => TunnelProxySnapshot {
            mode: config.tunnel_proxy.mode,
            custom_url: config.tunnel_proxy.custom_url.clone(),
            effective_source: proxy.source.to_string(),
            effective_proxy_present: proxy.url.is_some(),
            system_proxy_detected: proxy.system_proxy_detected,
        },
        Err(_) => {
            let system = crate::platform::system_http_proxy_candidate();
            TunnelProxySnapshot {
                mode: config.tunnel_proxy.mode,
                custom_url: config.tunnel_proxy.custom_url.clone(),
                effective_source: "invalid_custom".to_string(),
                effective_proxy_present: false,
                system_proxy_detected: system.is_some(),
            }
        }
    };
}

fn apply_openai_tunnel_configuration(
    snapshot: &mut DesktopStateSnapshot,
    config: &TunnelConfig,
    persistent_environment: bool,
) {
    let configuration = if persistent_environment {
        persistent_default_tunnel_snapshot(config)
    } else {
        config.snapshot()
    };
    snapshot.openai_tunnel_configured = configuration.is_configured();
    snapshot.openai_tunnel_config = configuration;
}

fn persistent_default_tunnel_snapshot(
    legacy: &TunnelConfig,
) -> crate::models::OpenAiTunnelConfigSnapshot {
    let result = (|| -> DesktopResult<crate::models::OpenAiTunnelConfigSnapshot> {
        let store = environment::store()?;
        legacy.ensure_persistent_catalog_compatible(&store)?;
        let profile = webcodex_environment::tunnel_profile_snapshots(&store)
            .map_err(environment::desktop_error)?
            .into_iter()
            .find(|profile| profile.profile_id == "default");
        Ok(match profile {
            Some(profile) => crate::models::OpenAiTunnelConfigSnapshot {
                tunnel_id_present: true,
                api_key_present: profile.credential_present,
                source: crate::models::TunnelConfigSource::Environment,
                saved_tunnel_id: Some(profile.tunnel_id.clone()),
                effective_tunnel_id: Some(profile.tunnel_id),
            },
            None => crate::models::OpenAiTunnelConfigSnapshot {
                source: crate::models::TunnelConfigSource::Environment,
                ..Default::default()
            },
        })
    })();
    result.unwrap_or_else(|_| crate::models::OpenAiTunnelConfigSnapshot {
        source: crate::models::TunnelConfigSource::Invalid,
        ..Default::default()
    })
}

fn same_server(left: &str, right: &str) -> bool {
    left.trim_end_matches('/')
        .eq_ignore_ascii_case(right.trim_end_matches('/'))
}

#[cfg(test)]
fn same_project(left: &str, right: &str) -> bool {
    if cfg!(windows) {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
    }
}
