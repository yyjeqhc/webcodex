use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use super::http::{fetch_runtime_status, http_post_json_status, HttpStatusSummary};
use super::{
    control_service_for_scope, encode_exec_argument, encode_exec_path_argument,
    encode_exec_program, encode_unit_path_value, ensure_service_file_parent,
    install_unit_for_scope, local_runner_profile_marker, local_runner_state_summary,
    query_systemd_service_status_for_scope, read_optional_token, read_optional_user_api_token,
    run_local_runner_logs, run_local_runner_service, run_logs_for_scope, service_unit_name,
    uninstall_unit_for_scope, validate_systemd_identity, LocalRunnerServiceAction,
    AGENT_SERVICE_UNIT,
};
use crate::{
    AgentInstallServiceOptions, AgentStatusOptions, ServiceActionKind, ServiceActionOptions,
    ServiceScope,
};

pub(crate) fn render_agent_systemd_unit(
    opts: &AgentInstallServiceOptions,
) -> Result<String, String> {
    match opts.scope {
        ServiceScope::User if opts.user.is_some() || opts.group.is_some() => {
            return Err(
                "user service units cannot contain User= or Group=; they run as the current user"
                    .to_string(),
            );
        }
        _ if opts.root_runner && !opts.allow_root_runner => {
            return Err(
                "refusing to render a Runner that would run as root without --allow-root-runner"
                    .to_string(),
            );
        }
        _ => {}
    }
    let binary = encode_exec_program("ExecStart", &opts.bin)?;
    let config_flag = encode_exec_argument("ExecStart option", "--config")?;
    let config = encode_exec_path_argument("ExecStart --config", &opts.config)?;
    let exec_start = format!("{binary} {config_flag} {config}");
    let working_directory = encode_unit_path_value("WorkingDirectory", &opts.working_directory)?;
    if let Some(user) = &opts.user {
        validate_systemd_identity("User", user)?;
    }
    if let Some(group) = &opts.group {
        validate_systemd_identity("Group", group)?;
    }

    let mut unit = String::new();
    if opts.root_runner {
        unit.push_str(
            "# WARNING: --allow-root-runner was explicitly accepted; project commands run as root.\n",
        );
    }
    unit.push_str("[Unit]\n");
    unit.push_str("Description=WebCodex Runner\n");
    if opts.scope == ServiceScope::System {
        unit.push_str("After=network-online.target\n");
        unit.push_str("Wants=network-online.target\n");
    }
    unit.push_str("\n[Service]\n");
    unit.push_str("Type=simple\n");
    unit.push_str(&format!("ExecStart={exec_start}\n"));
    unit.push_str("ExecReload=/bin/kill -HUP $MAINPID\n");
    unit.push_str("Restart=always\n");
    unit.push_str("RestartSec=5s\n");
    unit.push_str("StandardOutput=journal\n");
    unit.push_str("StandardError=journal\n");
    unit.push_str("Environment=RUST_LOG=info\n");
    unit.push_str(&format!("WorkingDirectory={working_directory}\n"));
    if opts.scope == ServiceScope::System {
        if let Some(user) = &opts.user {
            unit.push_str(&format!("User={user}\n"));
        }
        if let Some(group) = &opts.group {
            unit.push_str(&format!("Group={group}\n"));
        }
    }
    unit.push_str("\n[Install]\n");
    unit.push_str(match opts.scope {
        ServiceScope::User => "WantedBy=default.target\n",
        ServiceScope::System => "WantedBy=multi-user.target\n",
    });
    Ok(unit)
}

pub(crate) fn run_agent_install_service(
    opts: AgentInstallServiceOptions,
) -> Result<String, String> {
    let rendered = render_agent_systemd_unit(&opts)?;
    let unit = service_unit_name(&opts.service_file, AGENT_SERVICE_UNIT);
    if opts.output_stdout || opts.dry_run {
        if opts.json {
            return serde_json::to_string_pretty(&json!({
                "service_file": opts.service_file.to_string_lossy(),
                "config": opts.config.to_string_lossy(),
                "bin": opts.bin.to_string_lossy(),
                "unit_name": unit,
                "scope": opts.scope.as_str(),
                "root_runner": opts.root_runner,
                "dry_run": true,
                "systemd_called": false,
                "unit": rendered,
            }))
            .map_err(|e| e.to_string());
        }
        return Ok(rendered);
    }
    if opts.scope == ServiceScope::User {
        ensure_service_file_parent(&opts.service_file)?;
    }
    let result = install_unit_for_scope(
        opts.scope,
        &opts.service_file,
        &unit,
        &rendered,
        opts.overwrite,
        opts.no_start,
    )?;
    if opts.json {
        return serde_json::to_string_pretty(&json!({
            "service_file": opts.service_file.to_string_lossy(),
            "config": opts.config.to_string_lossy(),
            "bin": opts.bin.to_string_lossy(),
            "unit": result.unit,
            "scope": opts.scope.as_str(),
            "root_runner": opts.root_runner,
            "enabled": true,
            "started": result.started,
        }))
        .map_err(|e| e.to_string());
    }
    let warning = if opts.root_runner {
        "\nWARNING: explicit --allow-root-runner accepted; this Runner executes project commands as root.\n"
    } else {
        ""
    };
    Ok(format!(
        "Agent service installed.\n\n  scope:        {}\n  service file: {}\n  unit:         {}\n  config:       {}\n  binary:       {}\n  enabled:      yes\n  started:      {}\n{}",
        opts.scope.as_str(),
        opts.service_file.display(),
        result.unit,
        opts.config.display(),
        opts.bin.display(),
        if result.started { "yes" } else { "no (--no-start)" },
        warning,
    ))
}

pub(crate) fn run_agent_service(opts: ServiceActionOptions) -> Result<String, String> {
    if let Some(local) = &opts.local_profile {
        if local_runner_profile_marker(&local.state_dir).is_file() {
            return match &opts.kind {
                ServiceActionKind::Control(control) => run_local_runner_service(
                    match control {
                        super::ServiceControl::Start => LocalRunnerServiceAction::Start,
                        super::ServiceControl::Stop => LocalRunnerServiceAction::Stop,
                        super::ServiceControl::Restart => LocalRunnerServiceAction::Restart,
                    },
                    &local.config,
                    &local.state_dir,
                    None,
                ),
                ServiceActionKind::Logs {
                    lines,
                    since,
                    follow,
                } => run_local_runner_logs(
                    &local.state_dir,
                    *lines,
                    since.as_deref(),
                    *follow,
                ),
                ServiceActionKind::Uninstall { .. } => Err(
                    "hosted connect profiles are not system services; use `webcodex agent stop --profile <name>` and remove local profile files explicitly if desired"
                        .to_string(),
                ),
            };
        }
    }
    match opts.kind {
        ServiceActionKind::Control(control) => {
            control_service_for_scope(opts.scope, &opts.unit, control)?;
            Ok(format!(
                "Agent service {} completed for {}.\n",
                control.as_str(),
                opts.unit
            ))
        }
        ServiceActionKind::Logs {
            lines,
            since,
            follow,
        } => run_logs_for_scope(opts.scope, &opts.unit, lines, since.as_deref(), follow),
        ServiceActionKind::Uninstall { confirm } => {
            if !confirm {
                return Err("agent uninstall requires --confirm; no changes were made".to_string());
            }
            let result = uninstall_unit_for_scope(opts.scope, &opts.service_file, &opts.unit)?;
            Ok(format!(
                "Agent service {}. Agent config, tokens, profile data and binaries were not deleted.\n",
                if result.removed { "uninstalled" } else { "was already absent" }
            ))
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct AgentStatusConfig {
    #[serde(default)]
    server_url: String,
    #[serde(default)]
    token: String,
    #[serde(default)]
    client_id: String,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    transport: Option<String>,
    #[serde(default)]
    projects_dir: Option<PathBuf>,
    #[serde(default)]
    policy: AgentStatusPolicy,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct AgentStatusPolicy {
    #[serde(default)]
    allowed_roots: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
struct AgentConfigMetadata {
    path: PathBuf,
    client_id: String,
    owner: Option<String>,
    display_name: Option<String>,
    transport: Option<String>,
    projects_dir: Option<PathBuf>,
    allowed_roots: Vec<PathBuf>,
    server_url: String,
    token: String,
}

fn read_agent_config_metadata(path: &Path) -> Result<AgentConfigMetadata, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read agent config {}: {}", path.display(), e))?;
    let cfg: AgentStatusConfig = toml::from_str(&content)
        .map_err(|e| format!("failed to parse agent config {}: {}", path.display(), e))?;
    Ok(AgentConfigMetadata {
        path: path.to_path_buf(),
        client_id: cfg.client_id,
        owner: cfg.owner,
        display_name: cfg.display_name,
        transport: cfg.transport,
        projects_dir: cfg.projects_dir,
        allowed_roots: cfg.policy.allowed_roots,
        server_url: cfg.server_url,
        token: cfg.token,
    })
}

fn allowed_roots_summary(roots: &[PathBuf]) -> String {
    if roots.is_empty() {
        "0 configured; agent runtime defaults to $HOME when allowed_roots is omitted".to_string()
    } else {
        format!(
            "{} configured: {}",
            roots.len(),
            roots
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

fn runtime_client_entry<'a>(output: &'a Value, client_id: &str) -> Option<&'a Value> {
    output
        .pointer("/agents/clients")
        .and_then(Value::as_array)
        .and_then(|clients| {
            clients
                .iter()
                .find(|client| client.get("client_id").and_then(Value::as_str) == Some(client_id))
        })
}

fn runtime_client_online(output: &Value, client_id: &str) -> Option<bool> {
    let entry = runtime_client_entry(output, client_id)?;
    entry.get("connected").and_then(Value::as_bool).or_else(|| {
        entry
            .get("status")
            .and_then(Value::as_str)
            .map(|s| s == "online")
    })
}

pub(crate) async fn run_agent_status(opts: AgentStatusOptions) -> Result<String, String> {
    let service_unit = service_unit_name(&opts.service_file, AGENT_SERVICE_UNIT);
    let local = opts
        .local_state_dir
        .as_ref()
        .filter(|dir| local_runner_profile_marker(dir).is_file())
        .map(|dir| local_runner_state_summary(dir))
        .transpose()?;
    let systemd = local
        .is_none()
        .then(|| query_systemd_service_status_for_scope(opts.scope, &service_unit));
    let metadata = read_agent_config_metadata(&opts.config)?;
    let effective_server_url = opts.server_url.clone().or_else(|| {
        let url = metadata.server_url.trim().to_string();
        if url.is_empty() {
            None
        } else {
            Some(url)
        }
    });
    let user_token = if local.is_some() {
        let key = metadata.token.trim();
        if key.is_empty() {
            return Err("hosted Runner config has an empty shared key".to_string());
        }
        Some(key.to_string())
    } else {
        read_optional_user_api_token(&opts.user_token_file, "--user-token-file")?
    };
    let agent_token = if local.is_some() {
        None
    } else {
        read_optional_token(&opts.agent_token_file, "--agent-token-file")?
    };

    let mut runtime_http: Option<HttpStatusSummary> = None;
    let mut client_online: Option<bool> = None;
    if let (Some(server_url), Some(token)) =
        (effective_server_url.as_deref(), user_token.as_deref())
    {
        let http = fetch_runtime_status(server_url, &opts.server_http, Some(token)).await?;
        if let Some(output) = http.output.as_ref() {
            if !metadata.client_id.trim().is_empty() {
                client_online = runtime_client_online(output, &metadata.client_id);
            }
        }
        runtime_http = Some(http);
    }

    let mut agent_boundary_status: Option<&'static str> = None;
    let mut agent_boundary_detail: Option<String> = None;
    if let (Some(server_url), Some(token)) =
        (effective_server_url.as_deref(), agent_token.as_deref())
    {
        match http_post_json_status(
            server_url,
            &opts.server_http,
            "/api/runtime/status",
            Some(token),
            json!({}),
        )
        .await
        {
            Ok((status, content_type, _)) if status == 401 || status == 403 => {
                agent_boundary_status = Some("PASS");
                agent_boundary_detail =
                    Some("agent token cannot call /api/runtime/status".to_string());
                let _ = content_type;
            }
            Ok((status, content_type, _)) => {
                agent_boundary_status = Some("FAIL");
                agent_boundary_detail = Some(format!(
                    "unexpected HTTP {} content-type {}",
                    status, content_type
                ));
            }
            Err(e) => {
                agent_boundary_status = Some("FAIL");
                agent_boundary_detail = Some(e);
            }
        }
    }

    if opts.json {
        let summary = json!({
            "service": {
                "mode": if local.is_some() { "hosted_local" } else { "systemd" },
                "scope": if local.is_some() { Value::Null } else { json!(opts.scope.as_str()) },
                "unit": service_unit,
                "active": local.as_ref().map(|state| json!(state.running)).unwrap_or_else(|| json!(systemd.as_ref().expect("systemd status exists outside hosted mode").active)),
                "enabled": local.as_ref().map(|state| json!(state.managed)).unwrap_or_else(|| json!(systemd.as_ref().expect("systemd status exists outside hosted mode").enabled)),
                "pid": local.as_ref().and_then(|state| state.pid),
                "logs": local.as_ref().map(|state| state.log_path.to_string_lossy().to_string()),
            },
            "config": {
                "path": metadata.path.to_string_lossy(),
                "client_id": metadata.client_id,
                "owner": metadata.owner,
                "display_name": metadata.display_name,
                "transport": metadata.transport,
                "projects_dir": metadata.projects_dir.map(|p| p.to_string_lossy().to_string()),
                "allowed_roots": {
                    "count": metadata.allowed_roots.len(),
                    "summary": allowed_roots_summary(&metadata.allowed_roots),
                    "paths": metadata.allowed_roots.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>(),
                },
                "server_url": metadata.server_url,
            },
            "runtime": runtime_http.as_ref().map(|http| json!({
                "checked": true,
                "reachable": http.reachable,
                "status_code": http.status_code,
                "content_type": http.content_type,
                "error": http.error,
                "client_online": client_online,
            })).unwrap_or_else(|| json!({
                "checked": false,
                "reason": "requires server URL and --user-token-file",
            })),
            "agent_token_boundary": agent_boundary_status.map(|status| json!({
                "checked": true,
                "status": status,
                "detail": agent_boundary_detail,
            })).unwrap_or_else(|| json!({
                "checked": false,
                "reason": "requires server URL and --agent-token-file",
            })),
        });
        return serde_json::to_string_pretty(&summary).map_err(|e| e.to_string());
    }

    let mut out = String::new();
    out.push_str("Agent status:\n\n");
    if let Some(local) = &local {
        out.push_str("  runner mode:          hosted local process\n");
        out.push_str(&format!("  runner active:        {}\n", local.running));
        out.push_str(&format!(
            "  runner pid:           {}\n",
            local
                .pid
                .map(|pid| pid.to_string())
                .unwrap_or_else(|| "none".to_string())
        ));
        out.push_str(&format!(
            "  runner logs:          {}\n",
            local.log_path.display()
        ));
    } else {
        let systemd = systemd
            .as_ref()
            .expect("systemd status exists outside hosted mode");
        out.push_str(&format!(
            "  service scope:        {}\n",
            opts.scope.as_str()
        ));
        out.push_str(&format!("  service unit:         {service_unit}\n"));
        out.push_str(&format!("  service active:       {}\n", systemd.active));
        out.push_str(&format!("  service enabled:      {}\n", systemd.enabled));
    }
    out.push_str(&format!(
        "  config:               {}\n",
        metadata.path.display()
    ));
    out.push_str(&format!(
        "  client_id:            {}\n",
        if metadata.client_id.trim().is_empty() {
            "unknown"
        } else {
            metadata.client_id.as_str()
        }
    ));
    out.push_str(&format!(
        "  owner:                {}\n",
        metadata.owner.as_deref().unwrap_or("unknown")
    ));
    out.push_str(&format!(
        "  display_name:         {}\n",
        metadata.display_name.as_deref().unwrap_or("unknown")
    ));
    out.push_str(&format!(
        "  transport:            {}\n",
        metadata.transport.as_deref().unwrap_or("unknown")
    ));
    out.push_str(&format!(
        "  projects_dir:         {}\n",
        metadata
            .projects_dir
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "runtime default".to_string())
    ));
    out.push_str(&format!(
        "  allowed_roots:        {}\n",
        allowed_roots_summary(&metadata.allowed_roots)
    ));
    out.push_str(&format!(
        "  server_url:           {}\n",
        if metadata.server_url.trim().is_empty() {
            "unknown"
        } else {
            metadata.server_url.as_str()
        }
    ));
    match runtime_http {
        Some(http) => {
            out.push_str(&format!(
                "  runtime reachable:    {}\n",
                if http.reachable { "yes" } else { "no" }
            ));
            if let Some(code) = http.status_code {
                out.push_str(&format!("  runtime status:       {}\n", code));
            }
            if let Some(content_type) = &http.content_type {
                out.push_str(&format!("  runtime content-type: {}\n", content_type));
            }
            if let Some(error) = &http.error {
                out.push_str(&format!("  runtime error:        {}\n", error));
            }
            out.push_str(&format!(
                "  client online:        {}\n",
                client_online
                    .map(|online| if online { "yes" } else { "no" })
                    .unwrap_or("unknown")
            ));
        }
        None => out.push_str(
            "  runtime check:        skipped (requires server URL and --user-token-file)\n",
        ),
    }
    match agent_boundary_status {
        Some(status) => out.push_str(&format!(
            "  agent token boundary: {} ({})\n",
            status,
            agent_boundary_detail.unwrap_or_else(|| "unknown".to_string())
        )),
        None => out.push_str(
            "  agent token boundary: skipped (requires server URL and --agent-token-file)\n",
        ),
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_unit_name_tracks_the_selected_profile_unit() {
        assert_eq!(
            service_unit_name(
                Path::new("/etc/systemd/system/webcodex-runner-workstation.service"),
                AGENT_SERVICE_UNIT,
            ),
            "webcodex-runner-workstation.service"
        );
        assert_eq!(
            service_unit_name(
                Path::new("/etc/systemd/system/webcodex-runner.service"),
                AGENT_SERVICE_UNIT,
            ),
            AGENT_SERVICE_UNIT
        );
    }
}
