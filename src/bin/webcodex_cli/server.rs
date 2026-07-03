use serde_json::{json, Value};
use std::path::PathBuf;

use crate::{
    is_systemd_platform, write_text_file, ServerInitOptions, ServerInstallServiceOptions,
    ServerUpOptions,
};

use super::{
    compare_build_commits, default_server_paths, fetch_runtime_status, generate_bootstrap_token,
    local_cli_build_metadata, query_systemd_status, read_env_file_value,
    render_build_metadata_block, render_server_env, runtime_build_metadata,
    server_status_revision_check, token_prefix,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ServerStatusOptions {
    pub(crate) url: String,
    pub(crate) env_file: Option<PathBuf>,
    pub(crate) env_file_explicit: bool,
    pub(crate) token_file: Option<PathBuf>,
    pub(crate) json: bool,
}

pub(crate) fn run_server_up(opts: ServerUpOptions) -> Result<String, String> {
    let defaults = default_server_paths();
    let env_file = opts.env_file.unwrap_or(defaults.env_file);
    let data_dir = opts.data_dir.unwrap_or(defaults.data_dir);
    let listen = opts
        .listen
        .clone()
        .unwrap_or_else(|| "0.0.0.0:8080".to_string());

    // If the env file already exists and contains a WEBCODEX_TOKEN, reuse it.
    // Otherwise generate a fresh bootstrap/admin key.
    let existing_token = if env_file.exists() {
        read_env_file_value(&env_file, "WEBCODEX_TOKEN")
            .ok()
            .flatten()
    } else {
        None
    };
    let token_was_generated = existing_token.is_none();
    let token = existing_token.unwrap_or_else(generate_bootstrap_token);

    // Render the env file content.
    let mut content = String::new();
    content.push_str(&format!("WEBCODEX_ADDR={}\n", listen.trim()));
    content.push_str(&format!("WEBCODEX_DATA={}\n", data_dir.display()));
    content.push_str(&format!("WEBCODEX_TOKEN={}\n", token));
    if let Some(public_url) = &opts.public_url {
        content.push_str(&format!(
            "WEBCODEX_PUBLIC_URL={}\n",
            public_url.trim().trim_end_matches('/')
        ));
    }
    if opts.open {
        content.push_str("WEBCODEX_ALLOW_ANONYMOUS=true\n");
    }
    // Enable shared-key quick-start mode so clients using --key <KEY> are
    // accepted without managed token enrollment.
    content.push_str("WEBCODEX_SHARED_KEY_ENABLED=true\n");
    // Write the env file (overwrite when the token was freshly generated,
    // otherwise preserve and merge - simplest is to overwrite with the full
    // content since we read the token first).
    write_text_file(&env_file, &content, true, true)?;

    if opts.json {
        let summary = json!({
            "env_file": env_file.to_string_lossy(),
            "listen": listen,
            "data_dir": data_dir.to_string_lossy(),
            "open": opts.open,
            "token_generated": token_was_generated,
            "token_prefix": token_prefix(&token),
        });
        return serde_json::to_string_pretty(&summary).map_err(|e| e.to_string());
    }

    let mut out = String::new();
    out.push_str("Server bootstrap ready.\n\n");
    out.push_str(&format!("  env file:     {}\n", env_file.display()));
    out.push_str(&format!("  listen:       {}\n", listen));
    out.push_str(&format!("  data dir:     {}\n", data_dir.display()));
    if let Some(public_url) = &opts.public_url {
        out.push_str(&format!("  public URL:   {}\n", public_url.trim()));
    } else {
        out.push_str("  public URL:   not configured\n");
    }
    if token_was_generated {
        out.push_str(&format!(
            "  admin key:    generated and saved to {}\n",
            env_file.display()
        ));
    } else {
        out.push_str(&format!(
            "  admin key:    (reused from {})\n",
            env_file.display()
        ));
    }
    out.push_str(&format!("  token prefix: {}\n", token_prefix(&token)));
    out.push_str(&format!(
        "  open mode:    {}\n",
        if opts.open {
            "enabled (anonymous allowed)"
        } else {
            "disabled (anonymous denied)"
        }
    ));
    if opts.open {
        out.push_str("\n  WARNING: --open allows anonymous GPT/MCP and client access.\n");
        out.push_str("  Only use --open on localhost, trusted LAN, or temporary demos.\n");
        out.push_str("  Do NOT use --open on untrusted public networks.\n");
    }
    out.push_str("\nAuth modes:\n");
    out.push_str("  - Shared-key clients: allowed (use --key <KEY> on the client)\n");
    out.push_str("  - Managed tokens (wc_pat_*/wc_agent_*): allowed\n");
    if opts.open {
        out.push_str("  - Anonymous (no token): allowed under open group\n");
    } else {
        out.push_str("  - Anonymous (no token): denied\n");
    }
    out.push_str("\nNext steps:\n");
    out.push_str(&format!(
        "  1. Load the env:  set -a && . {} && set +a\n",
        env_file.display()
    ));
    out.push_str("  2. Start server:  webcodex\n");
    out.push_str("  3. Connect client: webcodex-cli connect <URL> --key <KEY> --root <PROJECT>\n");
    out.push_str("  4. GPT/MCP:       use the same --key value as a Bearer token\n");
    Ok(out)
}

pub(crate) fn run_server_init(opts: ServerInitOptions) -> Result<String, String> {
    let token = generate_bootstrap_token();
    let env_content = render_server_env(&opts, &token);
    write_text_file(&opts.env_file, &env_content, opts.overwrite, true)?;
    if opts.output_stdout {
        return Ok(env_content);
    }
    if opts.json {
        let summary = json!({
            "env_file": opts.env_file.to_string_lossy(),
            "listen": opts.listen,
            "data_dir": opts.data_dir.to_string_lossy(),
            "public_url": opts.public_url,
            "token_prefix": token_prefix(&token),
            "wrote_env_file": true,
            "next_steps": [
                "install service",
                "start service",
                "run server status",
                "configure HTTPS/public URL separately if using GPT Actions"
            ],
        });
        return serde_json::to_string_pretty(&summary).map_err(|e| e.to_string());
    }
    let mut out = String::new();
    out.push_str("Server bootstrap initialized.\n\n");
    out.push_str(&format!("  env file:     {}\n", opts.env_file.display()));
    out.push_str(&format!("  listen:       {}\n", opts.listen));
    out.push_str(&format!("  data dir:     {}\n", opts.data_dir.display()));
    if let Some(public_url) = &opts.public_url {
        out.push_str(&format!("  public URL:   {}\n", public_url.trim()));
    } else {
        out.push_str("  public URL:   not configured\n");
    }
    out.push_str(&format!("  token prefix: {}\n", token_prefix(&token)));
    out.push_str("\nNext steps:\n");
    out.push_str("  - Install the service: `webcodex-cli server install-service ...`\n");
    out.push_str(
        "  - Start it: `sudo systemctl daemon-reload && sudo systemctl enable --now webcodex`\n",
    );
    out.push_str("  - Check it: `webcodex-cli server status ...`\n");
    out.push_str("  - For GPT Actions, configure a public HTTPS URL separately.\n");
    out.push_str("\nNo user API tokens or agent tokens were created.\n");
    Ok(out)
}

pub(crate) fn run_server_install_service(
    opts: ServerInstallServiceOptions,
) -> Result<String, String> {
    let unit = render_systemd_unit(&opts);
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
                "env_file": opts.env_file.to_string_lossy(),
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
            "env_file": opts.env_file.to_string_lossy(),
            "bin": opts.bin.to_string_lossy(),
            "wrote_service_file": true,
            "next_steps": [
                "sudo systemctl daemon-reload",
                "sudo systemctl enable --now webcodex",
                "sudo systemctl status webcodex"
            ],
        });
        return serde_json::to_string_pretty(&summary).map_err(|e| e.to_string());
    }
    let mut out = String::new();
    out.push_str("Service unit installed.\n\n");
    out.push_str(&format!(
        "  service file: {}\n",
        opts.service_file.display()
    ));
    out.push_str(&format!("  env file:     {}\n", opts.env_file.display()));
    out.push_str(&format!("  binary:       {}\n", opts.bin.display()));
    out.push_str("\nNext steps:\n");
    out.push_str("  - sudo systemctl daemon-reload\n");
    out.push_str("  - sudo systemctl enable --now webcodex\n");
    out.push_str("  - sudo systemctl status webcodex\n");
    Ok(out)
}

fn render_systemd_unit(opts: &ServerInstallServiceOptions) -> String {
    let mut unit = String::new();
    unit.push_str("[Unit]\n");
    unit.push_str("Description=WebCodex Runtime\n");
    unit.push_str("After=network-online.target\n");
    unit.push_str("Wants=network-online.target\n\n");
    unit.push_str("[Service]\n");
    unit.push_str("Type=simple\n");
    unit.push_str(&format!("EnvironmentFile={}\n", opts.env_file.display()));
    unit.push_str(&format!("ExecStart={}\n", opts.bin.display()));
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
    let http = fetch_runtime_status(&opts.url, token.as_deref()).await?;
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
