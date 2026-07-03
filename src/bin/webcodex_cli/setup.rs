use serde_json::{json, Value};

use crate::{
    admin_cli::{self, build_admin_request, AdminCliCommand, AdminOptions},
    write_secret_file, SetupSingleUserOptions, SETUP_AGENT_SCOPES, SETUP_GPT_SCOPES,
};

use super::{post_json_authed, resolve_token, token_prefix, ApiCall};

/// Run `setup single-user`:
/// 1. Create the user (tolerate an "already exists" JSON error and continue).
/// 2. Create a personal API token for GPT Actions with the GPT scopes.
/// 3. Create an agent token bound to `--client-id` with the agent scopes.
/// 4. Save the returned plaintext tokens to 0600 files under `--output-dir`.
/// 5. Print a concise summary (token prefixes only) or machine JSON.
pub(crate) async fn run_setup_single_user(opts: SetupSingleUserOptions) -> Result<String, String> {
    let admin_opts = AdminOptions {
        server_url: opts.server_url.clone(),
        token: opts.token.clone(),
        token_env: None,
        credential: None,
        credential_env: None,
        token_file: opts.token_file.clone(),
        json: opts.json,
    };
    let bootstrap = resolve_token(&admin_opts, "WEBCODEX_TOKEN")?;
    let server_url = admin_opts.server_url.trim_end_matches('/').to_string();

    // AdminOptions carrying the bootstrap token. `build_admin_request` resolves
    // the token from these options; setup then issues the call with the same
    // bootstrap token (never printed).
    let call_opts = AdminOptions {
        server_url: opts.server_url.clone(),
        token: Some(bootstrap.clone()),
        token_env: None,
        credential: None,
        credential_env: None,
        token_file: None,
        json: opts.json,
    };

    // 1. Create user (tolerate already-exists).
    let mut user_body = json!({
        "username": opts.username,
        "role": opts.role,
    });
    if let Some(display_name) = &opts.display_name {
        user_body["display_name"] = json!(display_name);
    }
    let user_result = post_json_authed(ApiCall {
        server_url: &server_url,
        token: &bootstrap,
        path: "/api/users/create",
        body: user_body,
    })
    .await;
    let user_already_existed = match &user_result {
        Ok(_) => false,
        Err(e) if e.contains("already exists") => true,
        Err(e) => return Err(e.clone()),
    };

    // 2. Create GPT Actions personal API token.
    let gpt_create = AdminCliCommand::TokensCreate(
        call_opts.clone(),
        admin_cli::TokenCreateArgs {
            username: opts.username.clone(),
            name: Some(opts.gpt_token_name.clone()),
            scopes: SETUP_GPT_SCOPES.iter().map(|s| s.to_string()).collect(),
        },
    );
    let gpt_req = build_admin_request(&gpt_create)
        .map_err(|e| format!("internal: failed to build tokens create: {}", e))?;
    let gpt_resp = post_json_authed(ApiCall {
        server_url: &server_url,
        token: &bootstrap,
        path: gpt_req.path,
        body: gpt_req.body,
    })
    .await?;
    let user_token = gpt_resp
        .get("token")
        .and_then(Value::as_str)
        .ok_or_else(|| "tokens create response missing plaintext token".to_string())?
        .to_string();

    // 3. Create agent token bound to client_id.
    let agent_create = AdminCliCommand::AgentTokensCreate(
        call_opts.clone(),
        admin_cli::AgentTokenCreateArgs {
            username: opts.username.clone(),
            client_id: opts.client_id.clone(),
            name: Some(opts.agent_token_name.clone()),
            scopes: SETUP_AGENT_SCOPES.iter().map(|s| s.to_string()).collect(),
        },
    );
    let agent_req = build_admin_request(&agent_create)
        .map_err(|e| format!("internal: failed to build agent-tokens create: {}", e))?;
    let agent_resp = post_json_authed(ApiCall {
        server_url: &server_url,
        token: &bootstrap,
        path: agent_req.path,
        body: agent_req.body,
    })
    .await?;
    let agent_token = agent_resp
        .get("token")
        .and_then(Value::as_str)
        .ok_or_else(|| "agent-tokens create response missing plaintext token".to_string())?
        .to_string();

    // 4. Persist tokens to 0600 files.
    let user_token_path = opts.output_dir.join("webcodex-user-token");
    let agent_token_path = opts.output_dir.join("webcodex-agent-token");
    write_secret_file(&user_token_path, &format!("{}\n", user_token))?;
    write_secret_file(&agent_token_path, &format!("{}\n", agent_token))?;

    // 5. Summary. Never print full tokens or the bootstrap token.
    if opts.json {
        let summary = json!({
            "username": opts.username,
            "client_id": opts.client_id,
            "role": opts.role,
            "user_already_existed": user_already_existed,
            "gpt_token_name": opts.gpt_token_name,
            "agent_token_name": opts.agent_token_name,
            "user_token_prefix": token_prefix(&user_token),
            "agent_token_prefix": token_prefix(&agent_token),
            "user_token_path": user_token_path.to_string_lossy(),
            "agent_token_path": agent_token_path.to_string_lossy(),
        });
        serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())
    } else {
        let mut out = String::new();
        out.push_str("Setup complete.\n\n");
        out.push_str(&format!("  username:           {}\n", opts.username));
        out.push_str(&format!("  client_id:          {}\n", opts.client_id));
        if user_already_existed {
            out.push_str("  user:               already existed (reused)\n");
        }
        out.push_str(&format!(
            "  gpt token prefix:   {}\n",
            token_prefix(&user_token)
        ));
        out.push_str(&format!(
            "  agent token prefix: {}\n",
            token_prefix(&agent_token)
        ));
        out.push_str(&format!(
            "  user token file:    {}\n",
            user_token_path.display()
        ));
        out.push_str(&format!(
            "  agent token file:   {}\n",
            agent_token_path.display()
        ));
        out.push_str("\nNext steps:\n");
        out.push_str(&format!(
            "  - GPT Actions: use the personal API token in {} as the Bearer token.\n",
            user_token_path.display()
        ));
        out.push_str(&format!(
            "  - Agent: run `webcodex-cli agent init --server-url {} --token <wc_agent_token> --client-id {} --owner {} ...` using the agent token in {}.\n",
            server_url, opts.client_id, opts.username, agent_token_path.display()
        ));
        Ok(out)
    }
}
