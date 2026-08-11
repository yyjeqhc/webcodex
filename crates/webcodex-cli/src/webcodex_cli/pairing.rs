use serde_json::{json, Value};

use crate::{
    agent_init::{run_agent_init, AgentInitOptions, DEFAULT_POLL_INTERVAL_MS},
    write_text_file, ClientEnrollOptions, PairingCreateOptions,
};

use super::{
    post_json_authed, post_json_unauthed, read_pairing_server_env_file_value, shell_command,
    token_prefix, ApiCall,
};

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

fn pairing_login_argv(opts: &PairingCreateOptions, value: &Value) -> Vec<String> {
    let mut argv = vec![
        "webcodex".to_string(),
        "login".to_string(),
        opts.server_url.clone(),
        "--code".to_string(),
        value["pairing_code"].as_str().unwrap_or("").to_string(),
    ];
    if let Some(client_id) = value
        .get("client_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|client_id| !client_id.is_empty())
    {
        argv.push("--device".to_string());
        argv.push(client_id.to_string());
    }
    argv
}

fn render_pairing_create_result(
    opts: &PairingCreateOptions,
    value: &Value,
) -> Result<String, String> {
    let login_argv = pairing_login_argv(opts, value);
    if opts.json {
        let summary = json!({
            "pairing_code": value["pairing_code"],
            "expires_at": value["expires_at"],
            "username": value["username"],
            "client_id": value["client_id"],
            "login_argv": &login_argv,
        });
        serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())
    } else {
        let client_id = value["client_id"]
            .as_str()
            .filter(|client_id| !client_id.is_empty())
            .unwrap_or("(unbound; claimed by the login device)");
        let mut out = String::new();
        out.push_str("Pairing code created.\n\n");
        out.push_str(&format!(
            "  username:     {}\n",
            value["username"].as_str().unwrap_or("unknown")
        ));
        out.push_str(&format!("  client_id:    {client_id}\n"));
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
        out.push_str(&format!(
            "\nOn the client, run: {}\n",
            shell_command(&login_argv)
        ));
        out.push_str("No wc_pat_* or wc_agent_* token files were created on the server.\n");
        Ok(out)
    }
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
        server_http: &opts.server_http,
        token: &token,
        path: "/api/pairing/create",
        body,
    })
    .await
    .map_err(|e| e.replace(&token, "[redacted]"))?;
    render_pairing_create_result(&opts, &value)
}

pub(crate) fn ensure_enroll_outputs_available(opts: &ClientEnrollOptions) -> Result<(), String> {
    if opts.overwrite {
        return Ok(());
    }
    for path in [
        opts.output_dir.join("webcodex-user-token"),
        opts.output_dir.join("webcodex-runner-token"),
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
    let value = post_json_unauthed(
        &opts.server_url,
        &opts.server_http,
        "/api/pairing/enroll",
        body,
    )
    .await?;
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
    let agent_token_path = opts.output_dir.join("webcodex-runner-token");
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
                "start webcodex-runner with the generated agent.toml",
                "configure GPT Actions with the user-token file"
            ],
            "credential_usage": {
                "webcodex-user-token": "GPT Actions, MCP, and REST/project APIs",
                "webcodex-runner-token": "Runner/Agent transport only"
            }
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
        out.push_str("\nCredential usage:\n");
        out.push_str("  - webcodex-user-token: GPT Actions, MCP, and REST/project APIs\n");
        out.push_str("  - webcodex-runner-token: Runner/Agent transport only\n");
        out.push_str("\nNext steps:\n");
        let foreground_command = shell_command(&[
            "webcodex-runner".to_string(),
            "--config".to_string(),
            opts.agent_config.to_string_lossy().into_owned(),
        ]);
        out.push_str(&format!("  - Start the agent: `{foreground_command}`\n"));
        out.push_str(&format!(
            "  - GPT Actions should use the user-token file: {}\n",
            user_token_path.display()
        ));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(client_id: &str, json: bool) -> PairingCreateOptions {
        PairingCreateOptions {
            server_url: "https://example.test".to_string(),
            username: "alice".to_string(),
            client_id: client_id.to_string(),
            ttl_secs: 600,
            json,
            ..PairingCreateOptions::default()
        }
    }

    fn response(client_id: &str) -> Value {
        serde_json::json!({
            "pairing_code": "wc_pair_example",
            "expires_at": 1234,
            "username": "alice",
            "client_id": client_id,
        })
    }

    #[test]
    fn unbound_pairing_output_uses_login_without_device() {
        let output = render_pairing_create_result(&opts("", false), &response("")).unwrap();
        assert!(
            output.contains(
                "On the client, run: webcodex login https://example.test --code wc_pair_example"
            ),
            "{output}"
        );
        assert!(!output.contains("--device"), "{output}");
    }

    #[test]
    fn bound_pairing_output_uses_the_same_device() {
        let output =
            render_pairing_create_result(&opts("alice-laptop", false), &response("alice-laptop"))
                .unwrap();
        assert!(
            output.contains(
                "On the client, run: webcodex login https://example.test --code wc_pair_example --device alice-laptop"
            ),
            "{output}"
        );
    }

    #[test]
    fn pairing_login_command_quotes_dynamic_arguments() {
        let mut opts = opts("alice-laptop", false);
        opts.server_url = "https://example.test/path with space;$value".to_string();
        let value = serde_json::json!({
            "pairing_code": "wc_pair_a'b`c;d",
            "expires_at": 1234,
            "username": "alice",
            "client_id": "alice-laptop",
        });
        let argv = pairing_login_argv(&opts, &value);
        let output = render_pairing_create_result(&opts, &value).unwrap();
        assert!(output.contains(&shell_command(&argv)), "{output}");
        assert!(output.contains("'https://example.test/path with space;$value'"));
        assert!(output.contains("'wc_pair_a'\\''b`c;d'"));
    }

    #[test]
    fn pairing_json_exposes_structured_login_argv() {
        let output =
            render_pairing_create_result(&opts("alice-laptop", true), &response("alice-laptop"))
                .unwrap();
        let value: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(
            value["login_argv"],
            serde_json::json!([
                "webcodex",
                "login",
                "https://example.test",
                "--code",
                "wc_pair_example",
                "--device",
                "alice-laptop"
            ])
        );
    }
}
