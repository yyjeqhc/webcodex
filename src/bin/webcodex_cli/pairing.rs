use reqwest::header::CONTENT_TYPE;
use serde_json::{json, Value};

use crate::{
    agent_init::{run_agent_init, AgentInitOptions, DEFAULT_POLL_INTERVAL_MS},
    format_error_body, post_json_authed, write_text_file, ApiCall, ClientEnrollOptions,
    PairingCreateOptions,
};

use super::{read_pairing_server_env_file_value, token_prefix};

pub(crate) fn resolve_pairing_create_token(opts: &PairingCreateOptions) -> Result<String, String> {
    if let Some(token) = &opts.token {
        let token = token.trim().to_string();
        if token.is_empty() {
            return Err("--token cannot be empty".to_string());
        }
        return Ok(token);
    }
    if let Some(path) = &opts.token_file {
        let token = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read token file {}: {}", path.display(), e))?
            .trim()
            .to_string();
        if token.is_empty() {
            return Err("--token-file cannot be empty".to_string());
        }
        return Ok(token);
    }
    if let Some(path) = &opts.env_file {
        let token = read_pairing_server_env_file_value(path, "WEBCODEX_TOKEN")?
            .unwrap_or_default()
            .trim()
            .to_string();
        if token.is_empty() {
            return Err(format!(
                "env file {} does not contain WEBCODEX_TOKEN",
                path.display()
            ));
        }
        return Ok(token);
    }
    let token = std::env::var("WEBCODEX_TOKEN")
        .map_err(|_| {
            "--env-file, --token-file, --token, or WEBCODEX_TOKEN is required".to_string()
        })?
        .trim()
        .to_string();
    if token.is_empty() {
        return Err("WEBCODEX_TOKEN cannot be empty".to_string());
    }
    Ok(token)
}

pub(crate) async fn run_pairing_create(opts: PairingCreateOptions) -> Result<String, String> {
    let token = resolve_pairing_create_token(&opts)?;
    let mut body = json!({
        "username": opts.username,
        "client_id": opts.client_id,
        "ttl_secs": opts.ttl_secs,
    });
    if let Some(display_name) = &opts.display_name {
        body["display_name"] = json!(display_name);
    }
    if let Some(name) = &opts.user_token_name {
        body["user_token_name"] = json!(name);
    }
    if let Some(name) = &opts.agent_token_name {
        body["agent_token_name"] = json!(name);
    }
    let value = post_json_authed(ApiCall {
        server_url: &opts.server_url,
        token: &token,
        path: "/api/pairing/create",
        body,
    })
    .await
    .map_err(|e| e.replace(&token, "[redacted]"))?;
    if opts.json {
        let summary = json!({
            "pairing_code": value["pairing_code"],
            "expires_at": value["expires_at"],
            "username": value["username"],
            "client_id": value["client_id"],
        });
        serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())
    } else {
        let mut out = String::new();
        out.push_str("Pairing code created.\n\n");
        out.push_str(&format!(
            "  username:     {}\n",
            value["username"].as_str().unwrap_or("unknown")
        ));
        out.push_str(&format!(
            "  client_id:    {}\n",
            value["client_id"].as_str().unwrap_or("unknown")
        ));
        out.push_str(&format!(
            "  expires_at:   {}\n",
            value["expires_at"]
                .as_i64()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        ));
        out.push_str(&format!(
            "  pairing code: {}\n",
            value["pairing_code"].as_str().unwrap_or("")
        ));
        out.push_str(
            "\nCopy the pairing code to the client and run `webcodex-cli client enroll`.\n",
        );
        out.push_str("No wc_pat_* or wc_agent_* token files were created on the server.\n");
        Ok(out)
    }
}

pub(crate) fn ensure_enroll_outputs_available(opts: &ClientEnrollOptions) -> Result<(), String> {
    if opts.overwrite {
        return Ok(());
    }
    for path in [
        opts.output_dir.join("webcodex-user-token"),
        opts.output_dir.join("webcodex-agent-token"),
        opts.agent_config.clone(),
    ] {
        if path.exists() {
            return Err(format!(
                "{} already exists; pass --overwrite to replace it",
                path.display()
            ));
        }
    }
    Ok(())
}

pub(crate) async fn run_client_enroll(opts: ClientEnrollOptions) -> Result<String, String> {
    ensure_enroll_outputs_available(&opts)?;
    let mut body = json!({
        "pairing_code": opts.pairing_code,
        "client_id": opts.client_id,
        "transport": opts.transport,
        "projects_dir": opts.projects_dir.to_string_lossy(),
        "allow_cwd_anywhere": opts.allow_cwd_anywhere,
    });
    if let Some(display_name) = &opts.display_name {
        body["display_name"] = json!(display_name);
    }
    if !opts.allowed_roots.is_empty() {
        body["allowed_roots"] = json!(opts
            .allowed_roots
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect::<Vec<_>>());
    }
    let value = post_json_unauthed(&opts.server_url, "/api/pairing/enroll", body).await?;
    let user_token = value
        .get("user_token")
        .and_then(Value::as_str)
        .ok_or_else(|| "enroll response missing user_token".to_string())?
        .to_string();
    let agent_token = value
        .get("agent_token")
        .and_then(Value::as_str)
        .ok_or_else(|| "enroll response missing agent_token".to_string())?
        .to_string();
    let username = value
        .get("username")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let user_prefix = value
        .get("user_token_prefix")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| token_prefix(&user_token));
    let agent_prefix = value
        .get("agent_token_prefix")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| token_prefix(&agent_token));

    let user_token_path = opts.output_dir.join("webcodex-user-token");
    let agent_token_path = opts.output_dir.join("webcodex-agent-token");
    write_text_file(
        &user_token_path,
        &format!("{}\n", user_token),
        opts.overwrite,
        true,
    )?;
    write_text_file(
        &agent_token_path,
        &format!("{}\n", agent_token),
        opts.overwrite,
        true,
    )?;
    let agent_opts = AgentInitOptions {
        server_url: opts.server_url.clone(),
        token: Some(agent_token.clone()),
        token_file: None,
        client_id: opts.client_id.clone(),
        owner: username.clone(),
        display_name: opts.display_name.clone(),
        transport: opts.transport.clone(),
        poll_interval_ms: DEFAULT_POLL_INTERVAL_MS,
        projects_dir: opts.projects_dir.clone(),
        output: opts.agent_config.clone(),
        allowed_roots: opts.allowed_roots.clone(),
        allow_cwd_anywhere: opts.allow_cwd_anywhere,
        overwrite: opts.overwrite,
    };
    run_agent_init(agent_opts)?;

    if opts.json {
        let summary = json!({
            "username": username,
            "client_id": opts.client_id,
            "user_token_prefix": user_prefix,
            "agent_token_prefix": agent_prefix,
            "user_token_file": user_token_path.to_string_lossy(),
            "agent_token_file": agent_token_path.to_string_lossy(),
            "agent_config": opts.agent_config.to_string_lossy(),
            "projects_dir": opts.projects_dir.to_string_lossy(),
            "next_steps": [
                "start webcodex-agent with the generated agent.toml",
                "configure GPT Actions with the user-token file"
            ],
        });
        serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())
    } else {
        let mut out = String::new();
        out.push_str("Client enrollment complete.\n\n");
        out.push_str(&format!("  username:          {}\n", username));
        out.push_str(&format!("  client_id:         {}\n", opts.client_id));
        out.push_str(&format!("  user token prefix: {}\n", user_prefix));
        out.push_str(&format!("  agent token prefix:{}\n", agent_prefix));
        out.push_str(&format!(
            "  user token file:   {}\n",
            user_token_path.display()
        ));
        out.push_str(&format!(
            "  agent token file:  {}\n",
            agent_token_path.display()
        ));
        out.push_str(&format!(
            "  agent config:      {}\n",
            opts.agent_config.display()
        ));
        out.push_str("\nNext steps:\n");
        out.push_str(&format!(
            "  - Start the agent: `webcodex-agent --config {}`\n",
            opts.agent_config.display()
        ));
        out.push_str(&format!(
            "  - GPT Actions should use the user-token file: {}\n",
            user_token_path.display()
        ));
        Ok(out)
    }
}

async fn post_json_unauthed(server_url: &str, path: &str, body: Value) -> Result<Value, String> {
    let url = format!("{}{}", server_url.trim_end_matches('/'), path);
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|e| format!("failed to build HTTP client: {}", e))?;
    let resp = client
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("request failed: {}", e))?;
    let status = resp.status();
    let content_type = resp
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("failed to read response: {}", e))?;
    if !status.is_success() {
        return Err(format_error_body(status.as_u16(), &content_type, &text));
    }
    serde_json::from_str(&text).map_err(|e| {
        format!(
            "failed to parse JSON response: {} (content-type: {})",
            e, content_type
        )
    })
}
