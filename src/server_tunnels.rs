//! Server-owned Tunnel tasks. This is lifecycle supervision, not a second wire
//! implementation: all profiles use the same fixed-binding TunnelClient adapter.
use crate::project_entry::{probe_local_mcp, OpenAiTunnel, OpenAiTunnelPrerequisites};
use futures_util::{stream::FuturesUnordered, StreamExt};
use serde::Serialize;
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    sync::watch,
    time::{Instant, MissedTickBehavior},
};
use webcodex_environment::{
    embedded_tunnel_profiles, write_embedded_tunnel_health, EmbeddedTunnelProfile,
};
use webcodex_openai_tunnel::ControlPlaneProxy;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TunnelState {
    Starting,
    Running,
    Draining,
    Stopped,
    Failed,
}

/// Only safe observations; no Tunnel IDs, proxy URLs, tokens, payloads or headers.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct ProfileStatus {
    profile_id: String,
    host_mode: &'static str,
    state: TunnelState,
    tunnel_ready: bool,
    local_mcp_ready: bool,
    shutdown_outcome: Option<&'static str>,
    diagnostic: Option<String>,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct TunnelStatus(Arc<Mutex<Vec<ProfileStatus>>>);
impl TunnelStatus {
    pub(crate) fn snapshot(&self) -> Vec<ProfileStatus> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
    fn update(&self, index: usize, update: impl FnOnce(&mut ProfileStatus)) {
        update(
            &mut self
                .0
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())[index],
        );
    }
}

#[derive(Default)]
pub(crate) struct TunnelSupervisor {
    profiles: Vec<EmbeddedTunnelProfile>,
    mcp_url: String,
    status: TunnelStatus,
}
impl TunnelSupervisor {
    pub(crate) fn from_env(listener: SocketAddr) -> std::io::Result<Self> {
        let Some(root) = std::env::var_os("WEBCODEX_TUNNEL_ENVIRONMENT") else {
            return Ok(Self::default());
        };
        let profiles = embedded_tunnel_profiles(Path::new(&root)).map_err(std::io::Error::other)?;
        let ip = match listener.ip() {
            IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
            ip if ip.is_loopback() => ip,
            _ if profiles.is_empty() => listener.ip(),
            _ => {
                return Err(std::io::Error::other(
                    "Embedded Tunnel requires a reachable loopback Server listener",
                ))
            }
        };
        let mcp_url = format!("http://{}/mcp", SocketAddr::new(ip, listener.port()));
        let status = TunnelStatus(Arc::new(Mutex::new(
            profiles
                .iter()
                .map(|profile| ProfileStatus {
                    profile_id: profile.profile_id.clone(),
                    host_mode: "embedded",
                    state: if profile.autostart {
                        TunnelState::Starting
                    } else {
                        TunnelState::Stopped
                    },
                    tunnel_ready: false,
                    local_mcp_ready: false,
                    shutdown_outcome: None,
                    diagnostic: None,
                })
                .collect(),
        )));
        Ok(Self {
            profiles,
            mcp_url,
            status,
        })
    }
    pub(crate) fn status(&self) -> TunnelStatus {
        self.status.clone()
    }

    /// This future is polled alongside HTTP. Dropping it drops every profile owner,
    /// aborting its owned client and retaining its marker, never detaching a poller.
    pub(crate) async fn run(self, stop: watch::Receiver<bool>) {
        let mut profiles = FuturesUnordered::new();
        for (index, profile) in self.profiles.into_iter().enumerate() {
            if profile.autostart {
                profiles.push(run_profile(
                    index,
                    profile,
                    self.mcp_url.clone(),
                    self.status.clone(),
                    stop.clone(),
                ));
            } else if write_embedded_tunnel_health(
                &profile.readiness_path,
                profile.runtime_revision,
                false,
                false,
            )
            .is_err()
            {
                self.status.update(index, |state| {
                    state.state = TunnelState::Failed;
                    state.diagnostic = Some("tunnel_health_unavailable".into());
                });
            }
        }
        while profiles.next().await.is_some() {}
    }
}

async fn stopped(stop: &mut watch::Receiver<bool>) {
    if *stop.borrow() {
        return;
    }
    loop {
        if stop.changed().await.is_err() || *stop.borrow() {
            return;
        }
    }
}

async fn run_profile(
    index: usize,
    profile: EmbeddedTunnelProfile,
    mcp_url: String,
    status: TunnelStatus,
    mut stop: watch::Receiver<bool>,
) {
    if write_embedded_tunnel_health(
        &profile.readiness_path,
        profile.runtime_revision,
        false,
        false,
    )
    .is_err()
    {
        status.update(index, |state| {
            state.state = TunnelState::Failed;
            state.diagnostic = Some("tunnel_health_unavailable".into());
        });
        return;
    }
    let failed = |code: &str| {
        status.update(index, |state| {
            state.state = TunnelState::Failed;
            state.tunnel_ready = false;
            state.diagnostic = Some(code.to_string());
        });
        let _ = write_embedded_tunnel_health(
            &profile.readiness_path,
            profile.runtime_revision,
            false,
            false,
        );
    };
    let proxy = match profile.proxy.as_ref() {
        Some(proxy) => ControlPlaneProxy::Explicit(proxy.expose().to_string()),
        None => ControlPlaneProxy::Direct,
    };
    let binding = match OpenAiTunnelPrerequisites::bound(
        profile.credentials.tunnel_id.expose().to_string(),
        profile.credentials.api_key.expose().to_string(),
        proxy,
    ) {
        Ok(binding) => binding,
        Err(_) => {
            failed("tunnel_profile_configuration");
            return;
        }
    };
    let probe = match reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(probe) => probe,
        Err(_) => {
            failed("tunnel_probe_configuration");
            return;
        }
    };
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        tokio::select! {
            biased;
            _ = stopped(&mut stop) => {
                status.update(index, |state| { state.state = TunnelState::Stopped; state.shutdown_outcome = Some("clean"); });
                let _ = write_embedded_tunnel_health(
                    &profile.readiness_path,
                    profile.runtime_revision,
                    false,
                    false,
                );
                return;
            }
            ready = probe_local_mcp(&probe, &mcp_url, profile.local_token.expose()) => if ready { break; },
            _ = tokio::time::sleep_until(deadline) => { failed("local_mcp_unavailable"); return; }
        }
        tokio::select! {
            _ = stopped(&mut stop) => continue,
            _ = tokio::time::sleep(Duration::from_millis(100)) => {},
        }
        if Instant::now() >= deadline {
            failed("local_mcp_unavailable");
            return;
        }
    }
    let mut tunnel = match OpenAiTunnel::launch(&binding, &mcp_url, profile.local_token.expose()) {
        Ok(tunnel) => tunnel,
        Err(error) => {
            failed(&error.code);
            return;
        }
    };
    let health = tunnel.health();
    let mut tick = tokio::time::interval(Duration::from_secs(2));
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut reached_ready = false;
    let outcome = loop {
        tokio::select! {
            biased;
            _ = stopped(&mut stop) => break None,
            result = tunnel.wait_for_exit() => break Some(result.err().map(|error| error.code).unwrap_or_else(|| "tunnel_unexpected_exit".into())),
            _ = tokio::time::sleep_until(deadline), if !reached_ready => break Some("tunnel_startup_timeout".into()),
            _ = tick.tick() => {
                let local = tokio::select! {
                    biased;
                    _ = stopped(&mut stop) => break None,
                    ready = probe_local_mcp(&probe, &mcp_url, profile.local_token.expose()) => ready,
                };
                let ready = health.is_ready();
                reached_ready |= ready && local;
                status.update(index, |state| {
                    state.state = if reached_ready { TunnelState::Running } else { TunnelState::Starting };
                    state.tunnel_ready = ready; state.local_mcp_ready = local;
                });
                if write_embedded_tunnel_health(
                    &profile.readiness_path,
                    profile.runtime_revision,
                    ready,
                    local,
                )
                .is_err()
                {
                    break Some("tunnel_health_unavailable".into());
                }
            }
        }
    };
    status.update(index, |state| {
        state.state = TunnelState::Draining;
        state.tunnel_ready = false;
    });
    let _ = write_embedded_tunnel_health(
        &profile.readiness_path,
        profile.runtime_revision,
        false,
        false,
    );
    let clean = tunnel.stop_with_outcome().await.is_ok();
    status.update(index, |state| {
        state.state = if clean && outcome.is_none() {
            TunnelState::Stopped
        } else {
            TunnelState::Failed
        };
        state.tunnel_ready = false;
        state.local_mcp_ready = false;
        state.shutdown_outcome = Some(if clean { "clean" } else { "uncertain" });
        state.diagnostic = outcome.or_else(|| (!clean).then(|| "tunnel_restart_uncertain".into()));
    });
    tracing::info!(profile = %profile.profile_id, clean_shutdown = clean, "Server-owned Tunnel stopped");
}
