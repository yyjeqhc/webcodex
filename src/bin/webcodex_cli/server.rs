use serde_json::json;

use crate::{
    is_systemd_platform, write_text_file, ServerInitOptions, ServerInstallServiceOptions,
    ServerUpOptions,
};

use super::{
    default_server_paths, generate_bootstrap_token, read_env_file_value, render_server_env,
    token_prefix,
};

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
