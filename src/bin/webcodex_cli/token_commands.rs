use reqwest::header::CONTENT_TYPE;
use serde_json::json;

use crate::{
    admin_cli::{self, build_admin_request, AdminCliCommand},
    AgentTokenCreateLocalOptions, TokenCreateLocalOptions,
};

use super::{
    generate_local_agent_token, generate_local_api_token, hash_local_token, local_token_prefix,
};

pub(crate) fn resolve_account_credential(
    explicit: &Option<String>,
    env_name: &Option<String>,
) -> Result<String, String> {
    if let Some(value) = explicit {
        let value = value.trim().to_string();
        if value.is_empty() {
            return Err("--credential cannot be empty".to_string());
        }
        return Ok(value);
    }
    if let Some(env_name) = env_name {
        let env_name = env_name.trim();
        if env_name.is_empty() {
            return Err("--credential-env cannot be empty".to_string());
        }
        let value = std::env::var(env_name)
            .map_err(|_| format!("credential env var {} is not set", env_name))?
            .trim()
            .to_string();
        if value.is_empty() {
            return Err(format!("{} cannot be empty", env_name));
        }
        return Ok(value);
    }
    let value = std::env::var("WEBCODEX_ACCOUNT_CREDENTIAL")
        .map_err(|_| {
            "--credential, --credential-env, or WEBCODEX_ACCOUNT_CREDENTIAL is required".to_string()
        })?
        .trim()
        .to_string();
    if value.is_empty() {
        return Err("WEBCODEX_ACCOUNT_CREDENTIAL cannot be empty".to_string());
    }
    Ok(value)
}

pub(crate) async fn run_token_create_local(
    opts: TokenCreateLocalOptions,
) -> Result<String, String> {
    let credential = resolve_account_credential(&opts.credential, &opts.credential_env)?;
    let token = generate_local_api_token();
    let hash = hash_local_token(&token);
    let prefix = local_token_prefix(&token);
    let mut body = json!({
        "username": opts.username,
        "token_hash": format!("sha256:{}", hash),
        "token_prefix": prefix,
        "scopes": opts.scopes,
    });
    if let Some(name) = opts.name {
        body["name"] = json!(name);
    }
    let url = format!(
        "{}/api/tokens/register_hash",
        opts.server_url.trim_end_matches('/')
    );
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|e| format!("failed to build HTTP client: {}", e))?;
    let resp = client
        .post(url)
        .bearer_auth(&credential)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("request failed: {}", e).replace(&credential, "[redacted]"))?;
    let status = resp.status();
    let content_type = resp
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();
    let _text = resp
        .text()
        .await
        .map_err(|e| format!("failed to read response: {}", e))?;
    if !status.is_success() {
        return Err(format!(
            "request failed: HTTP {} (content-type: {})",
            status.as_u16(),
            content_type
        ));
    }
    Ok(format!(
        "API token created locally and registered with server.\n\nToken:\n{}\n\nUse this as Bearer token in GPT Action or MCP.\nThis token will not be shown again.\n",
        token
    ))
}

pub(crate) async fn run_agent_token_create_local(
    opts: AgentTokenCreateLocalOptions,
) -> Result<String, String> {
    let token = generate_local_agent_token();
    let hash = hash_local_token(&token);
    let prefix = local_token_prefix(&token);
    let cmd = AdminCliCommand::AgentTokensRegisterHash(
        opts.admin,
        admin_cli::AgentTokenRegisterHashArgs {
            username: opts.username,
            client_id: opts.client_id.clone(),
            name: opts.name,
            token_hash: format!("sha256:{}", hash),
            token_prefix: prefix,
            scopes: opts.scopes,
        },
    );
    let req = build_admin_request(&cmd)?;
    post_json_with_bearer(&req).await?;
    Ok(format!(
        "Agent token created locally and registered with server.\n\nClient ID:\n{}\n\nToken:\n{}\n\nUse this token in webcodex-agent config or WEBCODEX_AGENT_TOKEN.\nThis token will not be shown again.\n",
        opts.client_id, token
    ))
}

async fn post_json_with_bearer(req: &admin_cli::AdminCliRequest) -> Result<(), String> {
    let url = format!("{}{}", req.server_url, req.path);
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|e| format!("failed to build HTTP client: {}", e))?;
    let resp = client
        .post(url)
        .bearer_auth(&req.token)
        .json(&req.body)
        .send()
        .await
        .map_err(|e| format!("request failed: {}", e).replace(&req.token, "[redacted]"))?;
    let status = resp.status();
    let content_type = resp
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();
    let _text = resp
        .text()
        .await
        .map_err(|e| format!("failed to read response: {}", e))?;
    if !status.is_success() {
        return Err(format!(
            "request failed: HTTP {} (content-type: {})",
            status.as_u16(),
            content_type
        )
        .replace(&req.token, "[redacted]"));
    }
    Ok(())
}
