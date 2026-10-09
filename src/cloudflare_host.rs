//! Standalone transport owner. Admission and grants remain in the Server.
use crate::project_entry::cloudflare_transport::{prepare_cloudflared, CloudflareTransport};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use webcodex_environment::{CloudflareTunnelRuntimeProfile, TunnelHostMode, TunnelProvider};

/// Run a protected, revision-bound Cloudflare service until its owner stops.
pub async fn run_cloudflare_tunnel_with_stop(
    runtime_binding: PathBuf,
    stop_on_stdin_eof: bool,
    stop: impl std::future::Future<Output = ()>,
) -> Result<(), String> {
    let profile = webcodex_environment::load_cloudflare_tunnel_materialization(&runtime_binding)
        .map_err(|_| "cloudflare_runtime_binding_invalid".to_owned())?;
    if profile.host_mode != TunnelHostMode::Standalone || !profile.autostart {
        return Err("cloudflare_owner_configuration_invalid".into());
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(130))
        .build()
        .map_err(|_| "cloudflare_control_unavailable".to_owned())?;
    let shutdown = async {
        tokio::select! {
            _ = stop => {},
            _ = crate::project_entry::wait_for_regular_tunnel_stop_signal(stop_on_stdin_eof) => {},
        }
    };
    tokio::pin!(shutdown);
    let preparation = async {
        // Only connection establishment is retried, before an instance is acquired.
        let deadline = Instant::now() + Duration::from_secs(60);
        let status = loop {
            match control_request(
                &client,
                &profile,
                json!({"action":"status","profile_id":profile.profile_id}),
            )
            .await
            {
                Ok(status) => break status,
                Err(_) if Instant::now() < deadline => {
                    tokio::time::sleep(Duration::from_millis(250)).await
                }
                Err(reason) => return Err(reason),
            }
        };
        if status.get("configured_revision").and_then(Value::as_u64) != Some(profile.revision) {
            return Err("cloudflare_revision_conflict".to_owned());
        }
        let instance = status
            .get("server_instance_id")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or("cloudflare_control_invalid")?;
        control_request(
            &client,
            &profile,
            json!({"action":"prepare","profile_id":profile.profile_id,
            "expected_revision":profile.revision,"server_instance_id":instance}),
        )
        .await
    };
    let prepared = tokio::select! {
        result = preparation => result?,
        _ = &mut shutdown => return Ok(()),
    };
    let instance = prepared
        .get("server_instance_id")
        .and_then(Value::as_str)
        .ok_or("cloudflare_control_invalid")?
        .to_owned();
    let generation = prepared
        .get("process_generation")
        .and_then(Value::as_i64)
        .ok_or("cloudflare_control_invalid")?;
    if prepared.get("profile_id").and_then(Value::as_str) != Some(&profile.profile_id)
        || prepared.get("local_target").and_then(Value::as_str) != Some(&profile.local_target)
    {
        return Err("cloudflare_control_binding_mismatch".into());
    }
    let lifecycle = async {
        let binary = prepare_cloudflared()
            .await
            .map_err(|_| "cloudflare_binary_unavailable".to_owned())?;
        let deadline = Instant::now() + Duration::from_secs(60);
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
                .map_err(|_| "cloudflare_start_failed".to_owned())?,
            ),
            TunnelProvider::CloudflareQuick => {
                CloudflareTransport::start_quick(&binary, &profile.local_target, deadline)
                    .await
                    .map_err(|_| "cloudflare_start_failed".to_owned())?
            }
            _ => return Err("cloudflare_provider_invalid".to_owned()),
        };
        control_request(
            &client,
            &profile,
            json!({"action":"activate","profile_id":profile.profile_id,
            "expected_revision":profile.revision,"server_instance_id":instance,
            "process_generation":generation,"origin":origin}),
        )
        .await?;
        transport
            .wait_for_exit()
            .await
            .map_err(|_| "cloudflare_process_exited".to_owned())
    };
    let heartbeat = async {
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            tokio::time::timeout(
                Duration::from_secs(4),
                control_request(
                    &client,
                    &profile,
                    json!({"action":"heartbeat","profile_id":profile.profile_id,
                    "server_instance_id":instance,"process_generation":generation}),
                ),
            )
            .await
            .map_err(|_| "cloudflare_owner_lease_lost".to_owned())??;
        }
        #[allow(unreachable_code)]
        Ok::<(), String>(())
    };
    let result = tokio::select! {
        result = lifecycle => result,
        result = heartbeat => result,
        _ = &mut shutdown => Ok(()),
    };
    // No automatic new prepare after a lost lease or replacement Server.
    let _ = tokio::time::timeout(
        Duration::from_secs(4),
        control_request(
            &client,
            &profile,
            json!({"action":"stop","profile_id":profile.profile_id,"server_instance_id":instance,
            "process_generation":generation}),
        ),
    )
    .await;
    result
}

async fn control_request(
    client: &reqwest::Client,
    profile: &CloudflareTunnelRuntimeProfile,
    request: Value,
) -> Result<Value, String> {
    let mut response = client
        .post(format!(
            "{}/api/connections/cloudflare",
            profile.local_server_url.trim_end_matches('/')
        ))
        .bearer_auth(profile.bootstrap_token.expose())
        .json(&request)
        .send()
        .await
        .map_err(|_| "cloudflare_control_unavailable".to_owned())?;
    if !response.status().is_success() {
        return Err("cloudflare_control_rejected".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "cloudflare_control_unavailable".to_owned())?
    {
        if bytes.len() + chunk.len() > 65536 {
            return Err("cloudflare_control_invalid".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| "cloudflare_control_invalid".into())
}
