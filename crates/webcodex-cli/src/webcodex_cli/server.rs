use serde_json::{json, Value};
use std::path::PathBuf;
use webcodex_admin::ServerHttpOptions;

use crate::{
    ServerInitOptions, ServerInstallServiceOptions, ServiceActionKind, ServiceActionOptions,
};

use super::{
    compare_build_commits, control_service, encode_exec_program, encode_unit_path_value,
    fetch_runtime_status, generate_bootstrap_token, install_unit, local_cli_build_metadata,
    query_systemd_status, read_env_file_value, render_build_metadata_block, render_server_env,
    run_logs, runtime_build_metadata, server_status_revision_check, service_unit_name,
    token_prefix, uninstall_unit, validate_systemd_identity, SERVER_SERVICE_UNIT,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ServerStatusOptions {
    pub(crate) url: String,
    pub(crate) server_http: ServerHttpOptions,
    pub(crate) env_file: Option<PathBuf>,
    pub(crate) env_file_explicit: bool,
    pub(crate) token_file: Option<PathBuf>,
    pub(crate) json: bool,
}

pub(crate) fn run_server_init(opts: ServerInitOptions) -> Result<String, String> {
    if opts.env_file.exists() && !opts.overwrite {
        return Err(format!(
            "{} already exists; pass --overwrite to update it",
            opts.env_file.display()
        ));
    }
    let existing_token = if opts.env_file.exists() {
        read_env_file_value(&opts.env_file, "WEBCODEX_TOKEN")?
            .filter(|token| !token.trim().is_empty())
    } else {
        None
    };
    let token_generated = existing_token.is_none();
    let token = existing_token.unwrap_or_else(generate_bootstrap_token);
    let env_content = render_server_env(&opts, &token);
    super::write_text_file(&opts.env_file, &env_content, opts.overwrite, true)?;
    if opts.json {
        let summary = json!({
            "env_file": opts.env_file.to_string_lossy(),
            "listen": opts.listen,
            "data_dir": opts.data_dir.to_string_lossy(),
            "public_url": opts.public_url,
            "open": opts.open,
            "shared_key_enabled": true,
            "token_generated": token_generated,
            "token_prefix": token_prefix(&token),
            "wrote_env_file": true,
            "next_steps": [
                "webcodex server install",
                "webcodex server status",
                "configure HTTPS/public URL separately if using GPT Actions"
            ],
        });
        return serde_json::to_string_pretty(&summary).map_err(|e| e.to_string());
    }
    let mut out = String::new();
    out.push_str("Server configuration initialized.\n\n");
    out.push_str(&format!("  env file:     {}\n", opts.env_file.display()));
    out.push_str(&format!("  listen:       {}\n", opts.listen));
    out.push_str(&format!("  data dir:     {}\n", opts.data_dir.display()));
    out.push_str(&format!("  token prefix: {}\n", token_prefix(&token)));
    out.push_str(&format!(
        "  open mode:    {}\n",
        if opts.open { "enabled" } else { "disabled" }
    ));
    out.push_str("  shared key:   enabled\n");
    out.push_str("\nNext steps:\n");
    out.push_str("  - Install and start: `webcodex server install`\n");
    out.push_str("  - Or run in foreground: `webcodex server run`\n");
    out.push_str("  - Check it: `webcodex server status`\n");
    out.push_str("\nNo full token was printed. No user or Agent tokens were created.\n");
    Ok(out)
}

pub(crate) fn run_server_install_service(
    opts: ServerInstallServiceOptions,
) -> Result<String, String> {
    let rendered = render_systemd_unit(&opts)?;
    let unit = service_unit_name(&opts.service_file, SERVER_SERVICE_UNIT);
    if opts.output_stdout || opts.dry_run {
        if opts.json {
            return serde_json::to_string_pretty(&json!({
                "service_file": opts.service_file.to_string_lossy(),
                "env_file": opts.env_file.to_string_lossy(),
                "bin": opts.bin.to_string_lossy(),
                "unit_name": unit,
                "dry_run": true,
                "systemd_called": false,
                "unit": rendered,
            }))
            .map_err(|e| e.to_string());
        }
        return Ok(rendered);
    }
    let result = install_unit(
        &opts.service_file,
        &unit,
        &rendered,
        opts.overwrite,
        opts.no_start,
    )?;
    if opts.json {
        return serde_json::to_string_pretty(&json!({
            "service_file": opts.service_file.to_string_lossy(),
            "env_file": opts.env_file.to_string_lossy(),
            "bin": opts.bin.to_string_lossy(),
            "unit": result.unit,
            "enabled": true,
            "started": result.started,
        }))
        .map_err(|e| e.to_string());
    }
    Ok(format!(
        "Server service installed.\n\n  service file: {}\n  unit:         {}\n  binary:       {}\n  enabled:      yes\n  started:      {}\n",
        opts.service_file.display(),
        result.unit,
        opts.bin.display(),
        if result.started { "yes" } else { "no (--no-start)" }
    ))
}

fn render_systemd_unit(opts: &ServerInstallServiceOptions) -> Result<String, String> {
    let environment_file = encode_unit_path_value("EnvironmentFile", &opts.env_file)?;
    let exec_start = encode_exec_program("ExecStart", &opts.bin)?;
    let working_directory = encode_unit_path_value("WorkingDirectory", &opts.working_directory)?;
    if let Some(user) = &opts.user {
        validate_systemd_identity("User", user)?;
    }
    if let Some(group) = &opts.group {
        validate_systemd_identity("Group", group)?;
    }

    let mut unit = String::new();
    unit.push_str("[Unit]\n");
    unit.push_str("Description=WebCodex Runtime\n");
    unit.push_str("After=network-online.target\n");
    unit.push_str("Wants=network-online.target\n\n");
    unit.push_str("[Service]\n");
    unit.push_str("Type=simple\n");
    unit.push_str(&format!("EnvironmentFile={environment_file}\n"));
    unit.push_str(&format!("ExecStart={exec_start}\n"));
    unit.push_str("Restart=on-failure\n");
    unit.push_str("RestartSec=3\n");
    unit.push_str(&format!("WorkingDirectory={working_directory}\n"));
    if let Some(user) = &opts.user {
        unit.push_str(&format!("User={user}\n"));
    }
    if let Some(group) = &opts.group {
        unit.push_str(&format!("Group={group}\n"));
    }
    unit.push_str("\n[Install]\n");
    unit.push_str("WantedBy=multi-user.target\n");
    Ok(unit)
}

pub(crate) fn run_server_service(opts: ServiceActionOptions) -> Result<String, String> {
    match opts.kind {
        ServiceActionKind::Control(control) => {
            control_service(&opts.unit, control)?;
            Ok(format!(
                "Server service {} completed for {}.\n",
                control.as_str(),
                opts.unit
            ))
        }
        ServiceActionKind::Logs {
            lines,
            since,
            follow,
        } => run_logs(&opts.unit, lines, since.as_deref(), follow),
        ServiceActionKind::Uninstall { confirm } => {
            if !confirm {
                return Err("server uninstall requires --confirm; no changes were made".to_string());
            }
            let result = uninstall_unit(&opts.service_file, &opts.unit)?;
            Ok(format!(
                "Server service {}. Configuration and data were not deleted.\n",
                if result.removed {
                    "uninstalled"
                } else {
                    "was already absent"
                }
            ))
        }
    }
}

fn resolve_status_token(opts: &ServerStatusOptions) -> Result<Option<String>, String> {
    if let Some(path) = &opts.token_file {
        let token = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read token file {}: {}", path.display(), e))?
            .trim()
            .to_string();
        if token.is_empty() {
            return Err("--token-file cannot be empty".to_string());
        }
        return Ok(Some(token));
    }
    if let Some(path) = &opts.env_file {
        if !path.exists() {
            if opts.env_file_explicit {
                return Err(format!("env file {} does not exist", path.display()));
            }
        } else if let Some(token) = read_env_file_value(path, "WEBCODEX_TOKEN")? {
            let token = token.trim().to_string();
            if !token.is_empty() {
                return Ok(Some(token));
            }
        }
    }
    if let Ok(token) = std::env::var("WEBCODEX_TOKEN") {
        let token = token.trim().to_string();
        if !token.is_empty() {
            return Ok(Some(token));
        }
    }
    Ok(None)
}

pub(crate) async fn run_server_status(opts: ServerStatusOptions) -> Result<String, String> {
    let systemd = query_systemd_status();
    let token = resolve_status_token(&opts)?;
    let http = fetch_runtime_status(&opts.url, &opts.server_http, token.as_deref()).await?;
    let output = http.output.as_ref();
    let auth_enabled = output.and_then(|v| v.get("auth_enabled")).cloned();
    let configured_public_url = output
        .and_then(|v| v.get("configured_public_url"))
        .cloned()
        .unwrap_or(Value::Null);
    let tools_count = output
        .and_then(|v| v.pointer("/tools/count"))
        .and_then(Value::as_u64);
    let agents_online_count = output
        .and_then(|v| v.pointer("/agents/online_count"))
        .and_then(Value::as_u64);
    let server_build = runtime_build_metadata(output);
    let local_build = local_cli_build_metadata();
    let revision_comparison = compare_build_commits(
        local_build.git_commit.as_deref(),
        server_build.git_commit.as_deref(),
    );
    if opts.json {
        let summary = json!({
            "http_reachable": http.reachable,
            "http_status_code": http.status_code,
            "http_content_type": http.content_type,
            "http_error": http.error,
            "service": {
                "active": systemd.active,
                "enabled": systemd.enabled,
            },
            "auth_enabled": auth_enabled.unwrap_or(Value::Null),
            "configured_public_url": configured_public_url,
            "tools": {
                "count": tools_count,
            },
            "agents": {
                "online_count": agents_online_count,
            },
            "server_build": {
                "version": server_build.version,
                "git_commit": server_build.git_commit,
                "git_dirty": server_build.git_dirty,
                "built_at": server_build.built_at,
            },
            "local_cli_build": {
                "version": local_build.version,
                "git_commit": local_build.git_commit,
                "git_dirty": local_build.git_dirty,
                "built_at": local_build.built_at,
            },
            "revision_check": server_status_revision_check(&revision_comparison),
        });
        return serde_json::to_string_pretty(&summary).map_err(|e| e.to_string());
    }
    let mut out = String::new();
    out.push_str("Server status:\n\n");
    out.push_str(&format!(
        "  HTTP reachable:        {}\n",
        if http.reachable { "yes" } else { "no" }
    ));
    if !http.reachable {
        if let Some(code) = http.status_code {
            out.push_str(&format!("  HTTP status:           {}\n", code));
        }
        if let Some(content_type) = &http.content_type {
            out.push_str(&format!("  HTTP content-type:     {}\n", content_type));
        }
        if let Some(error) = &http.error {
            out.push_str(&format!("  HTTP error:            {}\n", error));
        }
    }
    out.push_str(&format!("  service active:        {}\n", systemd.active));
    out.push_str(&format!("  service enabled:       {}\n", systemd.enabled));
    out.push_str(&format!(
        "  auth_enabled:          {}\n",
        auth_enabled
            .as_ref()
            .map(Value::to_string)
            .unwrap_or_else(|| "unknown".to_string())
    ));
    out.push_str(&format!(
        "  configured_public_url: {}\n",
        if configured_public_url.is_null() {
            "null".to_string()
        } else {
            configured_public_url
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| configured_public_url.to_string())
        }
    ));
    out.push_str(&format!(
        "  tools.count:           {}\n",
        tools_count
            .map(|v| v.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    ));
    out.push_str(&format!(
        "  agents.online_count:   {}\n",
        agents_online_count
            .map(|v| v.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    ));
    out.push('\n');
    out.push_str(&render_build_metadata_block("Server build", &server_build));
    out.push('\n');
    out.push_str(&render_build_metadata_block(
        "Local CLI build",
        &local_build,
    ));
    out.push('\n');
    out.push_str("Revision check:\n");
    out.push_str(&format!(
        "  {}\n",
        server_status_revision_check(&revision_comparison)
    ));
    Ok(out)
}
