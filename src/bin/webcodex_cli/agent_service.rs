use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use super::http::{fetch_runtime_status, http_post_json_status, HttpStatusSummary};
use crate::{
    is_systemd_platform, query_systemd_service_status, read_optional_token, write_text_file,
    AgentInstallServiceOptions, AgentStatusOptions,
};

pub(crate) fn render_agent_systemd_unit(opts: &AgentInstallServiceOptions) -> String {
    let mut unit = String::new();
    unit.push_str("[Unit]\n");
    unit.push_str("Description=WebCodex Agent\n");
    unit.push_str("After=network-online.target\n");
    unit.push_str("Wants=network-online.target\n\n");
    unit.push_str("[Service]\n");
    unit.push_str("Type=simple\n");
    unit.push_str(&format!(
        "ExecStart={} --config {}\n",
        opts.bin.display(),
        opts.config.display()
    ));
    unit.push_str("Restart=on-failure\n");
    unit.push_str("RestartSec=3\n");
    unit.push_str(&format!(
        "WorkingDirectory={}\n",
        opts.working_directory.display()
    ));
    if let Some(user) = &opts.user {
        unit.push_str(&format!("User={}\n", user));
    }
    if let Some(group) = &opts.group {
        unit.push_str(&format!("Group={}\n", group));
    }
    unit.push_str("\n[Install]\n");
    unit.push_str("WantedBy=multi-user.target\n");
    unit
}

pub(crate) fn run_agent_install_service(
    opts: AgentInstallServiceOptions,
) -> Result<String, String> {
    let unit = render_agent_systemd_unit(&opts);
    let writes_file = !opts.dry_run && !opts.output_stdout;
    if writes_file {
        if opts.service_file.exists() && !opts.overwrite {
            return Err(format!(
                "{} already exists; pass --overwrite to replace it",
                opts.service_file.display()
            ));
        }
        if !is_systemd_platform() {
            return Err(
                "systemd was not detected; use --dry-run or --output - to render the unit"
                    .to_string(),
            );
        }
        write_text_file(&opts.service_file, &unit, opts.overwrite, false)?;
    }
    if opts.output_stdout || opts.dry_run {
        if opts.json {
            let summary = json!({
                "service_file": opts.service_file.to_string_lossy(),
                "config": opts.config.to_string_lossy(),
                "bin": opts.bin.to_string_lossy(),
                "dry_run": true,
                "unit": unit,
            });
            return serde_json::to_string_pretty(&summary).map_err(|e| e.to_string());
        }
        return Ok(unit);
    }
    if opts.json {
        let summary = json!({
            "service_file": opts.service_file.to_string_lossy(),
            "config": opts.config.to_string_lossy(),
            "bin": opts.bin.to_string_lossy(),
            "wrote_service_file": true,
            "next_steps": [
                "sudo systemctl daemon-reload",
                "sudo systemctl enable --now webcodex-agent",
                "sudo systemctl status webcodex-agent"
            ],
        });
        return serde_json::to_string_pretty(&summary).map_err(|e| e.to_string());
    }
    let mut out = String::new();
    out.push_str("Agent service unit installed.\n\n");
    out.push_str(&format!(
        "  service file: {}\n",
        opts.service_file.display()
    ));
    out.push_str(&format!("  config:       {}\n", opts.config.display()));
    out.push_str(&format!("  binary:       {}\n", opts.bin.display()));
    out.push_str("\nNext steps:\n");
    out.push_str("  - sudo systemctl daemon-reload\n");
    out.push_str("  - sudo systemctl enable --now webcodex-agent\n");
    out.push_str("  - sudo systemctl status webcodex-agent\n");
    Ok(out)
}

#[derive(Debug, Clone, Deserialize)]
struct AgentStatusConfig {
    #[serde(default)]
    server_url: String,
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
    let systemd = query_systemd_service_status("webcodex-agent.service");
    let metadata = read_agent_config_metadata(&opts.config)?;
    let effective_server_url = opts.server_url.clone().or_else(|| {
        let url = metadata.server_url.trim().to_string();
        if url.is_empty() {
            None
        } else {
            Some(url)
        }
    });
    let user_token = read_optional_token(&opts.user_token_file, "--user-token-file")?;
    let agent_token = read_optional_token(&opts.agent_token_file, "--agent-token-file")?;

    let mut runtime_http: Option<HttpStatusSummary> = None;
    let mut client_online: Option<bool> = None;
    if let (Some(server_url), Some(token)) =
        (effective_server_url.as_deref(), user_token.as_deref())
    {
        let http = fetch_runtime_status(server_url, Some(token)).await?;
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
        match http_post_json_status(server_url, "/api/runtime/status", Some(token), json!({})).await
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
                "active": systemd.active,
                "enabled": systemd.enabled,
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
    out.push_str(&format!("  service active:       {}\n", systemd.active));
    out.push_str(&format!("  service enabled:      {}\n", systemd.enabled));
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
