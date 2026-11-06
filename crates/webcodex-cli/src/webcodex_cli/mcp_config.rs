//! Render client configuration without loading credentials or changing files.
use serde_json::json;

pub(crate) const USAGE: &str = "Usage: webcodex mcp-config --client codex|claude --url URL/mcp --project PROJECT [--token-env NAME] [--approve-execution]\n\nPrint a Codex TOML fragment or Claude MCP JSON. Set the named token environment\nvariable separately. No token is read, no config is installed, and no network\nrequest is made. Effectful tools require client approval unless explicitly\nselected with --approve-execution (Codex only).\n";

pub(crate) fn render(args: &[String]) -> Result<String, String> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        return Ok(USAGE.into());
    }
    let mut client = None;
    let mut url = None;
    let mut project = None;
    let mut token_env = "WEBCODEX_TOKEN".to_string();
    let mut approve = false;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == "--approve-execution" {
            approve = true;
            continue;
        }
        if !matches!(
            arg.as_str(),
            "--client" | "--url" | "--project" | "--token-env"
        ) {
            return Err("Unknown mcp-config option; see --help".into());
        }
        let value = args
            .next()
            .ok_or_else(|| format!("Missing value for {arg}"))?
            .clone();
        match arg.as_str() {
            "--client" => client = Some(value),
            "--url" => url = Some(value),
            "--project" => project = Some(value),
            "--token-env" => token_env = value,
            _ => unreachable!(),
        }
    }
    let client = client.ok_or("--client is required")?;
    let url = url.ok_or("--url is required")?;
    let parsed = reqwest::Url::parse(&url).map_err(|_| "Invalid MCP URL")?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err("MCP URL must be HTTP(S), without credentials, query or fragment".into());
    }
    let project = project
        .filter(|p| !p.is_empty() && p.len() <= 256 && !p.chars().any(char::is_control))
        .ok_or("--project must be a non-empty Project selector without control characters")?;
    if token_env.is_empty()
        || token_env.len() > 128
        || !token_env
            .bytes()
            .enumerate()
            .all(|(i, c)| c == b'_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit()))
    {
        return Err("--token-env must be an environment variable name".into());
    }
    let headers = json!({"X-WebCodex-MCP-Surface":"cli", "X-WebCodex-MCP-Profile":"direct", "X-WebCodex-MCP-Project":project});
    match client.as_str() {
        "claude" if !approve => {
            let mut headers = headers;
            headers["Authorization"] = json!(format!("Bearer ${{{token_env}}}"));
            Ok(format!(
                "{}\n",
                serde_json::to_string_pretty(&json!({"mcpServers":{"webcodex":{
                    "type":"http", "url":url, "alwaysLoad":true, "headers":headers
                }}}))
                .map_err(|_| "Could not render configuration")?
            ))
        }
        "claude" => Err(
            "Claude approvals remain in its permission settings; omit --approve-execution".into(),
        ),
        "codex" => {
            let mut output = format!("[mcp_servers.webcodex]\nurl = {}\nbearer_token_env_var = {}\nrequired = true\ntool_timeout_sec = 65\n\n[mcp_servers.webcodex.http_headers]\n", json!(url), json!(token_env));
            for (key, value) in headers.as_object().unwrap() {
                output.push_str(&format!("{} = {}\n", json!(key), value));
            }
            for name in [
                "edit_project_files",
                "run_shell",
                "stop_job",
                "call_runtime_tool",
            ] {
                output.push_str(&format!(
                    "\n[mcp_servers.webcodex.tools.{name}]\napproval_mode = \"{}\"\n",
                    if approve { "approve" } else { "prompt" }
                ));
            }
            Ok(output)
        }
        _ => Err("--client must be codex or claude".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(client: &str) -> Vec<String> {
        [
            "--client",
            client,
            "--url",
            "http://127.0.0.1:8080/mcp",
            "--project",
            "agent:r:repo",
        ]
        .map(str::to_owned)
        .to_vec()
    }
    #[test]
    fn client_configuration_keeps_credentials_indirect_and_approval_explicit() {
        let claude: serde_json::Value =
            serde_json::from_str(&render(&args("claude")).unwrap()).unwrap();
        assert_eq!(
            claude["mcpServers"]["webcodex"]["headers"]["Authorization"],
            "Bearer ${WEBCODEX_TOKEN}"
        );
        assert_eq!(claude["mcpServers"]["webcodex"]["alwaysLoad"], true);
        let codex: toml::Value = toml::from_str(&render(&args("codex")).unwrap()).unwrap();
        assert_eq!(
            codex["mcp_servers"]["webcodex"]["tools"]["run_shell"]["approval_mode"].as_str(),
            Some("prompt")
        );
        let mut unsafe_url = args("codex");
        unsafe_url[3] = "https://secret@example.test/mcp".into();
        assert!(!render(&unsafe_url).unwrap_err().contains("secret"));
        let mut invalid = args("claude");
        invalid.extend(["--token-env".into(), "bad\nname".into()]);
        assert!(render(&invalid).is_err());
    }
}
