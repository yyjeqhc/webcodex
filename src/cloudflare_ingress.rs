//! The Server owns public admission even when a separate service owns cloudflared.
use crate::project_entry::cloudflare_transport::{prepare_cloudflared, CloudflareTransport};
use crate::public_ingress_auth::{ClientIngressGate, PublicIngressRequest};
use salvo::conn::{Listener, TcpListener};
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::watch;
#[cfg(not(windows))]
use webcodex_environment::EnvironmentStore;
use webcodex_environment::{CloudflareTunnelRuntimeProfile, TunnelHostMode, TunnelProvider};
use webcodex_store::{PublicIngressEntry, PublicIngressEntrySpec, PublicIngressMode};

const PROBE_PATH: &str = "/.well-known/webcodex-connection";
const START_TIMEOUT: Duration = Duration::from_secs(60);
const LEASE_TIMEOUT: Duration = Duration::from_secs(8);

struct OwnedTask(tokio::task::JoinHandle<()>);
impl Drop for OwnedTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct CloudflareStatus {
    pub profile_id: String,
    pub server_instance_id: String,
    pub process_generation: i64,
    pub lifecycle: String,
    pub public_origin: Option<String>,
    pub oauth_configured: bool,
    pub observed_authorization: bool,
    pub configured_revision: u64,
    pub applied_revision: Option<u64>,
    pub local_target: String,
    pub reason_code: Option<String>,
}

struct Attempt {
    profile_id: String,
    revision: u64,
    runtime_revision: u64,
    readiness_path: PathBuf,
    network_ready: bool,
    generation: i64,
    probe_nonce: String,
    entry: Option<PublicIngressEntry>,
    last_heartbeat: Instant,
    standalone: bool,
    lifecycle: &'static str,
    reason: Option<&'static str>,
    task: Option<OwnedTask>,
}

pub(crate) struct CloudflareControl {
    root: PathBuf,
    ingress_port: u16,
    db: Arc<crate::Database>,
    registry: Arc<crate::RunnerRegistry>,
    config: Arc<crate::Config>,
    instance_id: String,
    generation: std::sync::atomic::AtomicI64,
    attempt: Mutex<Option<Attempt>>,
}

#[derive(Clone, Serialize, Deserialize)]
struct PreparedIngress {
    profile_id: String,
    server_instance_id: String,
    process_generation: i64,
    local_target: String,
    probe_nonce: String,
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum ControlRequest {
    Status {
        profile_id: String,
    },
    Start {
        profile_id: String,
        expected_revision: u64,
        server_instance_id: String,
    },
    Prepare {
        profile_id: String,
        expected_revision: u64,
        server_instance_id: String,
    },
    Activate {
        profile_id: String,
        expected_revision: u64,
        server_instance_id: String,
        process_generation: i64,
        origin: String,
    },
    Heartbeat {
        profile_id: String,
        server_instance_id: String,
        process_generation: i64,
    },
    Stop {
        profile_id: String,
        server_instance_id: String,
        process_generation: i64,
    },
    ConfigureOauth {
        profile_id: String,
        server_instance_id: String,
        process_generation: i64,
        redirect_uri: String,
        #[serde(default)]
        scopes: Vec<String>,
        #[serde(default)]
        replace: bool,
    },
}

impl CloudflareControl {
    fn profile(&self, id: &str) -> Result<CloudflareTunnelRuntimeProfile, &'static str> {
        #[cfg(not(windows))]
        let profile = {
            let store =
                EnvironmentStore::open(self.root.clone()).map_err(|_| "environment_unavailable")?;
            webcodex_environment::cloudflare_tunnel_profile(&store, id)
                .map_err(|_| "cloudflare_profile_unavailable")?
        };
        #[cfg(windows)]
        let profile = webcodex_environment::load_cloudflare_server_materializations(
            &self.root.join("server"),
        )
        .map_err(|_| "cloudflare_restart_server_required")?
        .into_iter()
        .find(|profile| profile.profile_id == id)
        .ok_or("cloudflare_restart_server_required")?;
        if profile.ingress_port != self.ingress_port {
            return Err("cloudflare_restart_server_required");
        }
        Ok(profile)
    }

    fn current(&self, id: &str, instance: &str, generation: i64) -> bool {
        instance == self.instance_id
            && self.attempt.lock().is_ok_and(|guard| {
                guard.as_ref().is_some_and(|attempt| {
                    attempt.profile_id == id
                        && attempt.generation == generation
                        && matches!(attempt.lifecycle, "starting" | "running")
                })
            })
    }

    fn status(&self, id: &str) -> Result<CloudflareStatus, &'static str> {
        let profile = self.profile(id)?;
        let entry = self
            .db
            .get_public_ingress_entry_for_profile(id)
            .map_err(|_| "ingress_store_unavailable")?;
        let oauth_configured = entry.as_ref().is_some_and(|entry| {
            self.db.list_oauth_clients().is_ok_and(|clients| {
                clients
                    .iter()
                    .filter(|client| client.revoked_at.is_none())
                    .any(|client| {
                        self.db
                            .oauth_client_public_ingress_entry_id(&client.client_id)
                            .ok()
                            .flatten()
                            .as_deref()
                            == Some(&entry.entry_id)
                    })
            })
        });
        let observed_authorization = entry.as_ref().is_some_and(|entry| {
            self.db
                .public_ingress_has_observed_authorization(&entry.fence())
                .unwrap_or(false)
        });
        let guard = self
            .attempt
            .lock()
            .map_err(|_| "ingress_state_unavailable")?;
        let attempt = guard.as_ref().filter(|attempt| attempt.profile_id == id);
        Ok(CloudflareStatus {
            profile_id: id.to_owned(),
            server_instance_id: self.instance_id.clone(),
            process_generation: attempt.map_or(0, |attempt| attempt.generation),
            lifecycle: attempt
                .map_or("stopped", |attempt| {
                    if attempt.lifecycle == "running" && !attempt.network_ready {
                        "disconnected"
                    } else {
                        attempt.lifecycle
                    }
                })
                .to_owned(),
            public_origin: attempt
                .and_then(|attempt| attempt.entry.as_ref())
                .filter(|entry| entry.admission_open)
                .map(|entry| entry.origin.clone()),
            oauth_configured,
            observed_authorization,
            configured_revision: profile.revision,
            applied_revision: attempt
                .filter(|attempt| attempt.lifecycle == "running")
                .map(|attempt| attempt.revision),
            local_target: profile.local_target,
            reason_code: attempt
                .and_then(|attempt| {
                    attempt.reason.or_else(|| {
                        (attempt.lifecycle == "running" && !attempt.network_ready)
                            .then_some("network_disconnected")
                    })
                })
                .map(str::to_owned),
        })
    }

    fn prepare(
        &self,
        id: &str,
        revision: u64,
        standalone: bool,
    ) -> Result<PreparedIngress, &'static str> {
        let profile = self.profile(id)?;
        if profile.revision != revision {
            return Err("cloudflare_revision_conflict");
        }
        if standalone != (profile.host_mode == TunnelHostMode::Standalone) {
            return Err("cloudflare_owner_conflict");
        }
        let user = self
            .db
            .get_user_by_username(&profile.owner_username)
            .map_err(|_| "cloudflare_identity_unavailable")?
            .filter(|user| !user.is_disabled())
            .ok_or("cloudflare_identity_repair_required")?;
        let generation = self
            .generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            .checked_add(1)
            .ok_or("cloudflare_generation_exhausted")?;
        let mut guard = self
            .attempt
            .lock()
            .map_err(|_| "ingress_state_unavailable")?;
        if guard
            .as_ref()
            .is_some_and(|attempt| matches!(attempt.lifecycle, "starting" | "running"))
        {
            return Err("cloudflare_already_active");
        }
        let origin = match &profile.provider {
            TunnelProvider::CloudflareNamed { public_origin, .. } => public_origin.clone(),
            _ => "https://pending.invalid".to_owned(),
        };
        self.db
            .reconcile_public_ingress_configuration(id, &profile.configuration_id)
            .map_err(|_| "ingress_store_unavailable")?;
        let prior = self
            .db
            .get_public_ingress_entry_for_profile(id)
            .map_err(|_| "ingress_store_unavailable")?;
        let spec = PublicIngressEntrySpec {
            entry_id: prior.as_ref().map_or_else(
                || uuid::Uuid::new_v4().to_string(),
                |entry| entry.entry_id.clone(),
            ),
            profile_id: id.to_owned(),
            mode: if matches!(profile.provider, TunnelProvider::CloudflareNamed { .. }) {
                PublicIngressMode::Named
            } else {
                PublicIngressMode::Quick
            },
            origin,
            owner_user_id: user.id,
            process_generation: generation,
            server_instance_id: self.instance_id.clone(),
        };
        let entry = self
            .db
            .prepare_public_ingress_entry(&spec, prior.as_ref().map(|entry| entry.runtime_revision))
            .map_err(|_| "ingress_store_unavailable")?
            .ok_or("cloudflare_revision_conflict")?;
        let probe_nonce = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        *guard = Some(Attempt {
            profile_id: id.to_owned(),
            revision,
            runtime_revision: profile.runtime_revision,
            readiness_path: profile.readiness_path,
            network_ready: false,
            generation,
            probe_nonce: probe_nonce.clone(),
            entry: Some(entry),
            last_heartbeat: Instant::now(),
            standalone,
            lifecycle: "starting",
            reason: None,
            task: None,
        });
        Ok(PreparedIngress {
            profile_id: id.to_owned(),
            server_instance_id: self.instance_id.clone(),
            process_generation: generation,
            local_target: profile.local_target,
            probe_nonce,
        })
    }

    async fn activate(
        &self,
        prepared: &PreparedIngress,
        revision: u64,
        origin: &str,
    ) -> Result<PublicIngressEntry, &'static str> {
        if !self.current(
            &prepared.profile_id,
            &prepared.server_instance_id,
            prepared.process_generation,
        ) {
            return Err("cloudflare_stale_attempt");
        }
        let profile = self.profile(&prepared.profile_id)?;
        if profile.revision != revision {
            return Err("cloudflare_revision_conflict");
        }
        match &profile.provider {
            TunnelProvider::CloudflareNamed { public_origin, .. } if origin == public_origin => {}
            TunnelProvider::CloudflareQuick
                if crate::project_entry::cloudflare_transport::valid_quick_origin(origin) => {}
            _ => return Err("cloudflare_origin_mismatch"),
        }
        self.verify_owner(&profile).await?;
        verify_forwarding(origin, prepared).await?;
        let current_profile = self.profile(&prepared.profile_id)?;
        if current_profile.revision != revision
            || current_profile.configuration_id != profile.configuration_id
            || current_profile.owner_username != profile.owner_username
        {
            return Err("cloudflare_revision_conflict");
        }
        self.verify_owner(&current_profile).await?;
        // No asynchronous gap may separate the final catalog check and admission.
        #[cfg(not(windows))]
        let _configuration_lock = EnvironmentStore::open(self.root.clone())
            .map_err(|_| "environment_unavailable")?
            .lock()
            .map_err(|_| "cloudflare_configuration_busy")?;
        let final_profile = self.profile(&prepared.profile_id)?;
        if final_profile.revision != revision
            || final_profile.configuration_id != profile.configuration_id
            || final_profile.owner_username != profile.owner_username
        {
            return Err("cloudflare_revision_conflict");
        }
        let mut guard = self
            .attempt
            .lock()
            .map_err(|_| "ingress_state_unavailable")?;
        let attempt = guard
            .as_mut()
            .filter(|attempt| {
                attempt.profile_id == prepared.profile_id
                    && attempt.generation == prepared.process_generation
                    && attempt.lifecycle == "starting"
            })
            .ok_or("cloudflare_stale_attempt")?;
        let mut entry = attempt.entry.clone().ok_or("cloudflare_stale_attempt")?;
        if entry.origin != origin {
            entry = self
                .db
                .finalize_public_ingress_origin(&entry.fence(), origin)
                .map_err(|_| "ingress_store_unavailable")?
                .ok_or("cloudflare_stale_attempt")?;
        }
        webcodex_environment::write_embedded_tunnel_health(
            &attempt.readiness_path,
            attempt.runtime_revision,
            true,
            true,
        )
        .map_err(|_| "cloudflare_health_unavailable")?;
        if !self
            .db
            .set_public_ingress_admission(&entry.fence(), true)
            .map_err(|_| "ingress_store_unavailable")?
        {
            return Err("cloudflare_stale_attempt");
        }
        entry.active = true;
        entry.admission_open = true;
        attempt.entry = Some(entry.clone());
        attempt.lifecycle = "running";
        attempt.network_ready = true;
        attempt.last_heartbeat = Instant::now();
        Ok(entry)
    }

    async fn verify_owner(
        &self,
        profile: &CloudflareTunnelRuntimeProfile,
    ) -> Result<(), &'static str> {
        let client_id = profile
            .runner_client_id
            .as_deref()
            .ok_or("cloudflare_identity_repair_required")?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let runners = self.registry.list_runners_for_auth(None).await;
            if let Some(runner) = runners.iter().find(|runner| runner.client_id == client_id) {
                if runner.owner.as_deref() != Some(profile.owner_username.as_str()) {
                    return Err("cloudflare_owner_repair_required");
                }
                if runner.connected {
                    break;
                }
            }
            if Instant::now() >= deadline {
                return Err("cloudflare_runner_unavailable");
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Ok(())
    }

    fn stop(
        &self,
        id: &str,
        instance: &str,
        generation: i64,
        reason: Option<&'static str>,
    ) -> Result<(), &'static str> {
        if instance != self.instance_id {
            return Err("cloudflare_stale_attempt");
        }
        let mut guard = self
            .attempt
            .lock()
            .map_err(|_| "ingress_state_unavailable")?;
        let attempt = guard
            .as_mut()
            .filter(|attempt| attempt.profile_id == id && attempt.generation == generation)
            .ok_or("cloudflare_stale_attempt")?;
        if let Some(entry) = &attempt.entry {
            self.db
                .set_public_ingress_admission(&entry.fence(), false)
                .map_err(|_| "ingress_store_unavailable")?;
        }
        attempt.lifecycle = if reason.is_some() { "error" } else { "stopped" };
        attempt.reason = reason;
        attempt.entry = None;
        attempt.network_ready = false;
        attempt.task.take();
        let _ = webcodex_environment::write_embedded_tunnel_health(
            &attempt.readiness_path,
            attempt.runtime_revision,
            false,
            false,
        );
        Ok(())
    }

    fn start(self: &Arc<Self>, id: &str, revision: u64) -> Result<(), &'static str> {
        let prepared = self.prepare(id, revision, false)?;
        let profile = match self.profile(id) {
            Ok(profile) => profile,
            Err(reason) => {
                let _ = self.stop(
                    id,
                    &prepared.server_instance_id,
                    prepared.process_generation,
                    Some(reason),
                );
                return Err(reason);
            }
        };
        let weak = Arc::downgrade(self);
        let generation = prepared.process_generation;
        let task = tokio::spawn(async move {
            let result: Result<(), &'static str> = async {
                let control = weak.upgrade().ok_or("cloudflare_stopped")?;
                control.verify_owner(&profile).await?;
                drop(control);
                let binary = prepare_cloudflared()
                    .await
                    .map_err(|_| "cloudflare_binary_unavailable")?;
                let deadline = Instant::now() + START_TIMEOUT;
                let (origin, mut transport) = match &profile.provider {
                    TunnelProvider::CloudflareNamed { public_origin, .. } => (
                        public_origin.clone(),
                        CloudflareTransport::start_named(
                            &binary,
                            public_origin,
                            profile
                                .token_file
                                .as_deref()
                                .ok_or("cloudflare_token_unavailable")?,
                            deadline,
                        )
                        .await
                        .map_err(|_| "cloudflare_start_failed")?,
                    ),
                    TunnelProvider::CloudflareQuick => {
                        CloudflareTransport::start_quick(&binary, &profile.local_target, deadline)
                            .await
                            .map_err(|_| "cloudflare_start_failed")?
                    }
                    _ => return Err("cloudflare_provider_invalid"),
                };
                let control = weak.upgrade().ok_or("cloudflare_stopped")?;
                control.activate(&prepared, revision, &origin).await?;
                drop(control);
                let _ = transport.wait_for_exit().await;
                Err("cloudflare_process_exited")
            }
            .await;
            if let (Some(control), Err(reason)) = (weak.upgrade(), result) {
                let _ = control.stop(
                    &prepared.profile_id,
                    &prepared.server_instance_id,
                    prepared.process_generation,
                    Some(reason),
                );
            }
        });
        if let Ok(mut guard) = self.attempt.lock() {
            if let Some(attempt) = guard
                .as_mut()
                .filter(|attempt| attempt.profile_id == id && attempt.generation == generation)
            {
                attempt.task = Some(OwnedTask(task));
                return Ok(());
            }
        }
        task.abort();
        Err("cloudflare_stale_attempt")
    }
}

fn probe_client() -> Result<reqwest::Client, &'static str> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|_| "cloudflare_probe_unavailable")
}

async fn forwarding_matches(
    client: &reqwest::Client,
    origin: &str,
    prepared: &PreparedIngress,
) -> Result<bool, &'static str> {
    let mut response = client
        .get(format!("{origin}{PROBE_PATH}"))
        .send()
        .await
        .map_err(|_| "cloudflare_probe_unavailable")?;
    if !response.status().is_success() {
        return Ok(false);
    }
    if response
        .content_length()
        .is_some_and(|length| length > 4096)
    {
        return Err("cloudflare_forwarding_mismatch");
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "cloudflare_probe_unavailable")?
    {
        if bytes.len() + chunk.len() > 4096 {
            return Err("cloudflare_forwarding_mismatch");
        }
        bytes.extend_from_slice(&chunk);
    }
    let observed: PreparedIngress =
        serde_json::from_slice(&bytes).map_err(|_| "cloudflare_forwarding_mismatch")?;
    if observed.profile_id != prepared.profile_id
        || observed.server_instance_id != prepared.server_instance_id
        || observed.process_generation != prepared.process_generation
        || !crate::config::constant_time_eq(
            observed.probe_nonce.as_bytes(),
            prepared.probe_nonce.as_bytes(),
        )
    {
        return Err("cloudflare_forwarding_mismatch");
    }
    Ok(true)
}

async fn verify_forwarding(origin: &str, prepared: &PreparedIngress) -> Result<(), &'static str> {
    let client = probe_client()?;
    let deadline = Instant::now() + START_TIMEOUT;
    loop {
        match forwarding_matches(&client, origin, prepared).await {
            Ok(true) => return Ok(()),
            Err("cloudflare_forwarding_mismatch") => return Err("cloudflare_forwarding_mismatch"),
            _ => {}
        }
        if Instant::now() >= deadline {
            return Err("cloudflare_forwarding_timeout");
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

impl CloudflareControl {
    async fn observe_network(&self) {
        let target = self.attempt.lock().ok().and_then(|guard| {
            guard
                .as_ref()
                .filter(|attempt| attempt.lifecycle == "running")
                .and_then(|attempt| {
                    attempt.entry.as_ref().map(|entry| {
                        (
                            entry.origin.clone(),
                            PreparedIngress {
                                profile_id: attempt.profile_id.clone(),
                                server_instance_id: self.instance_id.clone(),
                                process_generation: attempt.generation,
                                local_target: String::new(),
                                probe_nonce: attempt.probe_nonce.clone(),
                            },
                        )
                    })
                })
        });
        let Some((origin, prepared)) = target else {
            return;
        };
        let ready = match probe_client() {
            Ok(client) => forwarding_matches(&client, &origin, &prepared)
                .await
                .unwrap_or(false),
            Err(_) => false,
        };
        if let Ok(mut guard) = self.attempt.lock() {
            if let Some(attempt) = guard.as_mut().filter(|attempt| {
                attempt.profile_id == prepared.profile_id
                    && attempt.generation == prepared.process_generation
                    && attempt.lifecycle == "running"
            }) {
                attempt.network_ready = ready;
                let _ = webcodex_environment::write_embedded_tunnel_health(
                    &attempt.readiness_path,
                    attempt.runtime_revision,
                    ready,
                    ready,
                );
            }
        }
    }
}

#[handler]
async fn forwarding_probe(depot: &mut Depot, res: &mut Response) {
    let Ok(control) = depot.obtain::<Arc<CloudflareControl>>() else {
        res.status_code(StatusCode::SERVICE_UNAVAILABLE);
        return;
    };
    if let Ok(guard) = control.attempt.lock() {
        if let Some(attempt) = guard
            .as_ref()
            .filter(|attempt| matches!(attempt.lifecycle, "starting" | "running"))
        {
            res.headers_mut().insert(
                "cache-control",
                salvo::http::HeaderValue::from_static("no-store"),
            );
            res.render(Json(PreparedIngress {
                profile_id: attempt.profile_id.clone(),
                server_instance_id: control.instance_id.clone(),
                process_generation: attempt.generation,
                local_target: String::new(),
                probe_nonce: attempt.probe_nonce.clone(),
            }));
            return;
        }
    }
    res.status_code(StatusCode::SERVICE_UNAVAILABLE);
}

impl ControlRequest {
    fn profile_id(&self) -> &str {
        match self {
            Self::Status { profile_id }
            | Self::Start { profile_id, .. }
            | Self::Prepare { profile_id, .. }
            | Self::Activate { profile_id, .. }
            | Self::Heartbeat { profile_id, .. }
            | Self::Stop { profile_id, .. }
            | Self::ConfigureOauth { profile_id, .. } => profile_id,
        }
    }
}

#[handler]
pub(crate) async fn cloudflare_control_handler(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) {
    res.headers_mut().insert(
        "cache-control",
        salvo::http::HeaderValue::from_static("no-store"),
    );
    let Some(control) = depot
        .obtain::<Option<Arc<CloudflareControl>>>()
        .ok()
        .cloned()
        .flatten()
    else {
        res.status_code(StatusCode::SERVICE_UNAVAILABLE);
        res.render(Json(serde_json::json!({"error":"cloudflare_ingress_not_applied","next_action":"restart_server"})));
        return;
    };
    if let Err((status, _, message)) = crate::auth::require_json_same_origin(req) {
        res.status_code(StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_REQUEST));
        res.render(Json(serde_json::json!({"error":message})));
        return;
    }
    let request = match req.payload().await {
        Ok(bytes) if bytes.len() <= 16 * 1024 => {
            serde_json::from_slice::<ControlRequest>(bytes).ok()
        }
        _ => None,
    };
    let Some(request) = request else {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(
            serde_json::json!({"error":"invalid_cloudflare_request"}),
        ));
        return;
    };
    let auth = depot.obtain::<crate::auth::AuthContext>().ok();
    let profile = match control.profile(request.profile_id()) {
        Ok(profile) => profile,
        Err(reason) => {
            res.status_code(StatusCode::CONFLICT);
            res.render(Json(serde_json::json!({"error":reason})));
            return;
        }
    };
    let admitted = auth.is_some_and(|auth| {
        auth.is_bootstrap()
            || (auth.kind == crate::auth::AuthKind::ApiToken
                && auth.username.as_deref() == Some(&profile.owner_username)
                && auth.has_scope(webcodex_core::authority::SCOPE_RUNNER_MANAGE))
    });
    if !admitted {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(
            serde_json::json!({"error":"cloudflare_owner_required"}),
        ));
        return;
    }
    let result: Result<serde_json::Value, &'static str> = async {
        let id = request.profile_id().to_owned();
        match request {
            ControlRequest::Status { .. } => {}
            ControlRequest::Start {
                expected_revision,
                server_instance_id,
                ..
            } => {
                if server_instance_id != control.instance_id {
                    return Err("cloudflare_stale_attempt");
                }
                control.start(&id, expected_revision)?;
            }
            ControlRequest::Prepare {
                expected_revision,
                server_instance_id,
                ..
            } => {
                if server_instance_id != control.instance_id || !profile.autostart {
                    return Err("cloudflare_stale_attempt");
                }
                control.verify_owner(&profile).await?;
                return serde_json::to_value(control.prepare(&id, expected_revision, true)?)
                    .map_err(|_| "cloudflare_state_unavailable");
            }
            ControlRequest::Activate {
                expected_revision,
                server_instance_id,
                process_generation,
                origin,
                ..
            } => {
                let prepared = {
                    let guard = control
                        .attempt
                        .lock()
                        .map_err(|_| "ingress_state_unavailable")?;
                    let attempt = guard
                        .as_ref()
                        .filter(|attempt| {
                            attempt.profile_id == id && attempt.generation == process_generation
                        })
                        .ok_or("cloudflare_stale_attempt")?;
                    PreparedIngress {
                        profile_id: id.clone(),
                        server_instance_id,
                        process_generation,
                        local_target: profile.local_target.clone(),
                        probe_nonce: attempt.probe_nonce.clone(),
                    }
                };
                control
                    .activate(&prepared, expected_revision, &origin)
                    .await?;
            }
            ControlRequest::Heartbeat {
                server_instance_id,
                process_generation,
                ..
            } => {
                if server_instance_id != control.instance_id {
                    return Err("cloudflare_stale_attempt");
                }
                let mut guard = control
                    .attempt
                    .lock()
                    .map_err(|_| "ingress_state_unavailable")?;
                let attempt = guard
                    .as_mut()
                    .filter(|attempt| {
                        attempt.profile_id == id
                            && attempt.generation == process_generation
                            && matches!(attempt.lifecycle, "starting" | "running")
                    })
                    .ok_or("cloudflare_stale_attempt")?;
                attempt.last_heartbeat = Instant::now();
            }
            ControlRequest::Stop {
                server_instance_id,
                process_generation,
                ..
            } => control.stop(&id, &server_instance_id, process_generation, None)?,
            ControlRequest::ConfigureOauth {
                server_instance_id,
                process_generation,
                redirect_uri,
                scopes,
                replace,
                ..
            } => {
                if server_instance_id != control.instance_id {
                    return Err("cloudflare_stale_attempt");
                }
                // Provisioning is a protected owner operation. A normal coding PAT
                // is never silently upgraded to account administration.
                if !auth.is_some_and(|auth| {
                    auth.is_bootstrap() || auth.has_scope(crate::auth::SCOPE_ADMIN)
                }) {
                    return Err("cloudflare_owner_administration_required");
                }
                return control.provision_oauth(
                    &id,
                    process_generation,
                    &redirect_uri,
                    scopes,
                    replace,
                );
            }
        }
        serde_json::to_value(control.status(&id)?).map_err(|_| "cloudflare_state_unavailable")
    }
    .await;
    match result {
        Ok(value) => {
            res.render(Json(value));
        }
        Err(reason) => {
            res.status_code(StatusCode::CONFLICT);
            res.render(Json(serde_json::json!({"error":reason})));
        }
    }
}

impl CloudflareControl {
    fn provision_oauth(
        &self,
        id: &str,
        process_generation: i64,
        redirect_uri: &str,
        scopes: Vec<String>,
        replace: bool,
    ) -> Result<serde_json::Value, &'static str> {
        // A delayed configuration request must retain the process target the
        // caller observed, including when a Quick profile has since restarted.
        if !self.current(id, &self.instance_id, process_generation) {
            return Err("cloudflare_stale_attempt");
        }
        crate::oauth_http::validate_redirect_uri(redirect_uri)
            .map_err(|_| "cloudflare_redirect_invalid")?;
        let entry = self
            .db
            .get_public_ingress_entry_for_profile(id)
            .map_err(|_| "ingress_store_unavailable")?
            .ok_or("cloudflare_start_required")?;
        if entry.server_instance_id != self.instance_id
            || entry.process_generation != process_generation
        {
            return Err("cloudflare_stale_attempt");
        }
        let profile = self.profile(id)?;
        // A standalone lease may outlive deletion and same-name recreation.
        // Retained monotonic profile revisions also fence that replacement.
        if !self
            .attempt
            .lock()
            .map_err(|_| "ingress_state_unavailable")?
            .as_ref()
            .is_some_and(|attempt| {
                attempt.profile_id == id
                    && attempt.generation == process_generation
                    && attempt.revision == profile.revision
            })
        {
            return Err("cloudflare_stale_attempt");
        }
        let user = self
            .db
            .get_user_by_username(&profile.owner_username)
            .map_err(|_| "cloudflare_identity_unavailable")?
            .ok_or("cloudflare_identity_repair_required")?;
        if user.id != entry.owner_user_id || user.is_disabled() {
            return Err("cloudflare_owner_repair_required");
        }
        let scopes = if scopes.is_empty() {
            webcodex_core::authority::profiles::LOCAL_USER
                .iter()
                .map(|scope| (*scope).to_owned())
                .collect()
        } else {
            scopes
        };
        if scopes.len() > 64
            || scopes
                .iter()
                .any(|scope| !crate::oauth_http::oauth_scopes_supported().contains(&scope.as_str()))
            || scopes
                .iter()
                .any(|scope| matches!(scope.as_str(), "admin" | "account:manage"))
        {
            return Err("cloudflare_scope_invalid");
        }
        let clients = self
            .db
            .list_oauth_clients()
            .map_err(|_| "ingress_store_unavailable")?;
        for client in clients
            .iter()
            .filter(|client| client.revoked_at.is_none())
            .filter(|client| {
                self.db
                    .oauth_client_public_ingress_entry_id(&client.client_id)
                    .ok()
                    .flatten()
                    .as_deref()
                    == Some(&entry.entry_id)
            })
        {
            if !replace {
                if client.redirect_uris != redirect_uri || client.allowed_scopes != scopes.join(" ")
                {
                    return Err("cloudflare_oauth_replace_required");
                }
                if !self
                    .db
                    .oauth_client_matches_public_ingress(&client.client_id, &entry.fence())
                    .map_err(|_| "ingress_store_unavailable")?
                {
                    return Err("cloudflare_stale_attempt");
                }
                return Ok(
                    serde_json::json!({"client_id":client.client_id,"client_secret":null,"already_configured":true}),
                );
            }
        }
        let secret = crate::auth::generate_oauth_client_secret();
        let client = crate::models::OAuthClientRecord {
            id: uuid::Uuid::new_v4().to_string(),
            client_id: crate::auth::generate_oauth_client_id(),
            client_secret_hash: crate::auth::hash_token(&secret),
            name: profile.profile_id,
            owner_user_id: Some(user.id),
            owner_project_grant_id: None,
            owner_shared_key_hash: None,
            redirect_uris: redirect_uri.to_owned(),
            allowed_scopes: scopes.join(" "),
            created_at: chrono::Utc::now().timestamp(),
            revoked_at: None,
        };
        if !self
            .db
            .insert_public_ingress_oauth_client(&client, &entry.fence(), replace)
            .map_err(|_| "ingress_store_unavailable")?
        {
            return Err("cloudflare_stale_attempt");
        }
        Ok(
            serde_json::json!({"client_id":client.client_id,"client_secret":secret,"already_configured":false}),
        )
    }
}

struct PublicSnapshot;
#[async_trait]
impl Handler for PublicSnapshot {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let Some(control) = depot.obtain::<Arc<CloudflareControl>>().ok().cloned() else {
            res.status_code(StatusCode::SERVICE_UNAVAILABLE);
            ctrl.skip_rest();
            return;
        };
        let entry = control
            .db
            .get_active_public_ingress_entry()
            .ok()
            .flatten()
            .filter(|entry| {
                entry.server_instance_id == control.instance_id && entry.admission_open
            });
        let Some(entry) = entry else {
            res.status_code(StatusCode::SERVICE_UNAVAILABLE);
            ctrl.skip_rest();
            return;
        };
        let mut config = (*control.config).clone();
        config.oauth2.enabled = true;
        config.oauth2.require_pkce = true;
        config.oauth2.issuer = Some(entry.origin.clone());
        config.oauth2.shared_key_bridge_enabled = false;
        config.oauth2.project_share_grant_id = None;
        config.oauth2.project_share_session_id = None;
        depot.inject(Arc::new(config));
        depot.inject(PublicIngressRequest(entry));
        if let Err((status, _, message)) =
            crate::public_ingress_auth::require_authority(req, depot, &control.config)
        {
            res.status_code(StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_REQUEST));
            res.render(Json(serde_json::json!({"error":message})));
            ctrl.skip_rest();
            return;
        }
        ctrl.call_next(req, depot, res).await;
    }
}

pub(crate) struct CloudflareOwner {
    control: Arc<CloudflareControl>,
    listener: OwnedTask,
    handle: salvo::server::ServerHandle,
}

impl CloudflareOwner {
    pub(crate) async fn from_env(
        config: Arc<crate::Config>,
        db: Arc<crate::Database>,
        registry: Arc<crate::RunnerRegistry>,
        runtime: Arc<crate::tool_runtime::ToolRuntime>,
        sessions: Arc<crate::oauth_http::AuthorizeSessionStore>,
        shutdown: Arc<crate::server_shutdown::ShutdownCoordinator>,
    ) -> std::io::Result<Option<Self>> {
        let Some(root) = std::env::var_os("WEBCODEX_TUNNEL_ENVIRONMENT") else {
            return Ok(None);
        };
        Self::from_root(
            PathBuf::from(root),
            config,
            db,
            registry,
            runtime,
            sessions,
            shutdown,
        )
        .await
    }

    async fn from_root(
        root: PathBuf,
        config: Arc<crate::Config>,
        db: Arc<crate::Database>,
        registry: Arc<crate::RunnerRegistry>,
        runtime: Arc<crate::tool_runtime::ToolRuntime>,
        sessions: Arc<crate::oauth_http::AuthorizeSessionStore>,
        shutdown: Arc<crate::server_shutdown::ShutdownCoordinator>,
    ) -> std::io::Result<Option<Self>> {
        #[cfg(not(windows))]
        let port = {
            let store =
                EnvironmentStore::open(PathBuf::from(&root)).map_err(std::io::Error::other)?;
            webcodex_environment::cloudflare_ingress_port(&store).map_err(std::io::Error::other)?
        };
        #[cfg(windows)]
        let port = webcodex_environment::load_cloudflare_server_ingress_port(
            &PathBuf::from(&root).join("server"),
        )
        .map_err(std::io::Error::other)?;
        let Some(port) = port else {
            return Ok(None);
        };
        if !config.is_auth_enabled() {
            return Err(std::io::Error::other(
                "Cloudflare requires private Server authentication",
            ));
        }
        let control = Arc::new(CloudflareControl {
            root: PathBuf::from(root),
            ingress_port: port,
            db: db.clone(),
            registry,
            config: config.clone(),
            instance_id: uuid::Uuid::new_v4().to_string(),
            generation: std::sync::atomic::AtomicI64::new(0),
            attempt: Mutex::new(None),
        });
        // Acquire the listener before retiring persisted admission. A failed
        // bind must leave the current owner's readiness and grants untouched.
        let acceptor = TcpListener::new(format!("127.0.0.1:{port}"))
            .try_bind()
            .await
            .map_err(std::io::Error::other)?;
        // Persisted readiness never survives takeover. Named grant epochs do.
        if let Some(entry) = db
            .get_active_public_ingress_entry()
            .map_err(std::io::Error::other)?
        {
            db.set_public_ingress_admission(&entry.fence(), false)
                .map_err(std::io::Error::other)?;
        }
        let router = Router::new()
            .hoop(crate::server_shutdown::DrainAdmission::new(shutdown))
            .hoop(affix_state::inject(control.clone()))
            .hoop(affix_state::inject(db))
            .hoop(affix_state::inject(runtime))
            .hoop(affix_state::inject(sessions))
            .hoop(salvo::timeout::Timeout::new(Duration::from_secs(180)))
            .push(
                Router::with_path(crate::route_metadata::root_path(
                    crate::route_metadata::RouteId::CloudflareForwardingProbe,
                ))
                .get(forwarding_probe),
            )
            .push(
                Router::new()
                    .hoop(PublicSnapshot)
                    .push(
                        Router::with_path(crate::route_metadata::root_path(
                            crate::route_metadata::RouteId::McpPost,
                        ))
                        .hoop(crate::AuthMiddleware)
                        .post(crate::mcp::mcp_post),
                    )
                    .push(
                        Router::with_path(crate::route_metadata::root_path(
                            crate::route_metadata::RouteId::McpGet,
                        ))
                        .hoop(crate::AuthMiddleware)
                        .get(crate::mcp::mcp_info),
                    )
                    .push(
                        Router::with_path(crate::route_metadata::root_path(
                            crate::route_metadata::RouteId::WellKnownProtectedResource,
                        ))
                        .get(crate::oauth_http::oauth_metadata),
                    )
                    .push(
                        Router::with_path(crate::route_metadata::root_path(
                            crate::route_metadata::RouteId::WellKnownAuthorizationServer,
                        ))
                        .get(crate::oauth_http::oauth_authorization_server_metadata),
                    )
                    .push(
                        Router::with_path(crate::route_metadata::root_path(
                            crate::route_metadata::RouteId::OAuthAuthorize,
                        ))
                        .hoop(ClientIngressGate::ClientId)
                        .get(crate::oauth_http::oauth_authorize),
                    )
                    .push(
                        Router::with_path(crate::route_metadata::root_path(
                            crate::route_metadata::RouteId::OAuthAuthorizeLogin,
                        ))
                        .hoop(ClientIngressGate::LoginReturnTo)
                        .post(crate::oauth_http::oauth_authorize_login),
                    )
                    .push(
                        Router::with_path(crate::route_metadata::root_path(
                            crate::route_metadata::RouteId::OAuthAuthorizeConsent,
                        ))
                        .hoop(ClientIngressGate::ClientId)
                        .post(crate::oauth_http::oauth_authorize_consent),
                    )
                    .push(
                        Router::with_path(crate::route_metadata::root_path(
                            crate::route_metadata::RouteId::OAuthToken,
                        ))
                        .hoop(ClientIngressGate::ClientId)
                        .post(crate::oauth_http::oauth_token),
                    )
                    .push(
                        Router::with_path(crate::route_metadata::root_path(
                            crate::route_metadata::RouteId::OAuthRevoke,
                        ))
                        .hoop(ClientIngressGate::ClientId)
                        .post(crate::oauth_http::oauth_revoke),
                    ),
            );
        let server = Server::new(acceptor);
        let handle = server.handle();
        let listener_control = Arc::downgrade(&control);
        let listener = OwnedTask(tokio::spawn(async move {
            if server.try_serve(router).await.is_err() {
                if let Some(control) = listener_control.upgrade() {
                    let target = control.attempt.lock().ok().and_then(|guard| {
                        guard
                            .as_ref()
                            .map(|attempt| (attempt.profile_id.clone(), attempt.generation))
                    });
                    if let Some((id, generation)) = target {
                        let _ = control.stop(
                            &id,
                            &control.instance_id,
                            generation,
                            Some("cloudflare_listener_failed"),
                        );
                    }
                }
            }
        }));
        Ok(Some(Self {
            control,
            listener,
            handle,
        }))
    }

    pub(crate) fn control(&self) -> Arc<CloudflareControl> {
        self.control.clone()
    }

    pub(crate) async fn run(self, mut stop: watch::Receiver<bool>) {
        #[cfg(not(windows))]
        let profiles = webcodex_environment::cloudflare_tunnel_profiles(&self.control.root);
        #[cfg(windows)]
        let profiles = webcodex_environment::load_cloudflare_server_materializations(
            &self.control.root.join("server"),
        );
        if let Ok(profiles) = profiles {
            for profile in profiles.into_iter().filter(|profile| {
                profile.autostart && profile.host_mode == TunnelHostMode::Embedded
            }) {
                let _ = self.control.start(&profile.profile_id, profile.revision);
            }
        }
        loop {
            if *stop.borrow() {
                break;
            }
            tokio::select! { changed = stop.changed() => { if changed.is_err() || *stop.borrow() { break; } }, _ = tokio::time::sleep(Duration::from_secs(2)) => {
                let stale = self.control.attempt.lock().ok().and_then(|guard| guard.as_ref().filter(|attempt| attempt.standalone && matches!(attempt.lifecycle, "starting" | "running") && attempt.last_heartbeat.elapsed() > LEASE_TIMEOUT).map(|attempt| (attempt.profile_id.clone(), attempt.generation)));
                if let Some((id, generation)) = stale { let _ = self.control.stop(&id, &self.control.instance_id, generation, Some("cloudflare_owner_lost")); }
                if self.listener.0.is_finished() {
                    let target = self.control.attempt.lock().ok().and_then(|guard| guard.as_ref().map(|attempt| (attempt.profile_id.clone(), attempt.generation)));
                    if let Some((id, generation)) = target { let _ = self.control.stop(&id, &self.control.instance_id, generation, Some("cloudflare_listener_failed")); }
                    break;
                }
                self.control.observe_network().await;
            } }
        }
        let target = self.control.attempt.lock().ok().and_then(|guard| {
            guard
                .as_ref()
                .map(|attempt| (attempt.profile_id.clone(), attempt.generation))
        });
        if let Some((id, generation)) = target {
            let _ = self
                .control
                .stop(&id, &self.control.instance_id, generation, None);
        }
        self.handle.stop_graceful(Some(Duration::from_secs(10)));
        let _ = tokio::time::timeout(Duration::from_secs(12), async {
            while !self.listener.0.is_finished() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await;
    }
}

impl Drop for CloudflareOwner {
    fn drop(&mut self) {
        self.handle.stop_forcible();
        let target = self.control.attempt.lock().ok().and_then(|guard| {
            guard
                .as_ref()
                .map(|attempt| (attempt.profile_id.clone(), attempt.generation))
        });
        if let Some((id, generation)) = target {
            let _ = self
                .control
                .stop(&id, &self.control.instance_id, generation, None);
        }
    }
}

#[cfg(all(test, unix))]
mod tests;
