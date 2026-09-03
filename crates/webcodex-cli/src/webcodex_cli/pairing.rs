use serde_json::{json, Value};

use crate::PairingCreateOptions;

use super::{post_json_authed, read_pairing_server_env_file_value, shell_command, ApiCall};

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
        let lifetime = if opts.ttl_secs % 60 == 0 {
            let minutes = opts.ttl_secs / 60;
            format!("{minutes} minute{}", if minutes == 1 { "" } else { "s" })
        } else {
            format!("{} seconds", opts.ttl_secs)
        };
        let mut out = String::new();
        out.push_str("One-time login code created\n\n");
        out.push_str("Code:\n");
        out.push_str(&format!(
            "  {}\n",
            value["pairing_code"].as_str().unwrap_or("")
        ));
        out.push_str("\nExpires in:\n");
        out.push_str(&format!("  {lifetime}\n"));
        out.push_str("\nUse this code once on the machine that holds your project.\n");
        out.push_str("\nNext:\n");
        out.push_str(&format!("  {}\n", shell_command(&login_argv)));
        out.push_str("\nThis code is not the Server administrator token. Do not copy the Server env file to the project machine.\n");
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
    if let Some(name) = &opts.runner_token_name {
        // Stable pairing API field retained for Server compatibility.
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
        assert!(output.contains("One-time login code created"), "{output}");
        assert!(output.contains("Code:\n  wc_pair_example"), "{output}");
        assert!(output.contains("Expires in:\n  10 minutes"), "{output}");
        assert!(
            output.contains("Use this code once on the machine that holds your project."),
            "{output}"
        );
        assert!(
            output.contains("webcodex login https://example.test --code wc_pair_example"),
            "{output}"
        );
        assert!(
            output.contains("not the Server administrator token"),
            "{output}"
        );
        assert!(!output.contains("--device"), "{output}");
        assert!(!output.contains("wc_pat_"), "{output}");
        assert!(!output.contains("wc_agent_"), "{output}");
    }

    #[test]
    fn bound_pairing_output_uses_the_same_device() {
        let output =
            render_pairing_create_result(&opts("alice-laptop", false), &response("alice-laptop"))
                .unwrap();
        assert!(
            output.contains(
                "webcodex login https://example.test --code wc_pair_example --device alice-laptop"
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
