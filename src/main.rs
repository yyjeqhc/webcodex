#![recursion_limit = "512"]

use salvo::cors::Cors;
use salvo::prelude::*;
#[cfg(test)]
use std::path::PathBuf;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;
#[cfg(test)]
use uuid::Uuid;

mod action_audit;
mod action_audit_sessions;
mod admin_cli;
mod agent_quic;
mod agent_tokens_http;
mod agent_ws;
mod artifact_policy;
mod audit_http;
mod auth;
mod build_info;
mod config;
mod connector_runtime;
mod console_web;
mod db;
mod hosted_connect;
mod mcp;
mod models;
mod oauth_http;
mod openapi;
mod pairing_http;
// The server uses only normalization/bounds helpers; the same module's
// filesystem scanner is compiled for and invoked by webcodex-agent.
mod lsp_bridge;
#[allow(dead_code)]
mod project_overview;
mod projects;
mod runtime_http;
mod shell_client;
mod shell_protocol;
mod startup;
mod tool_request_trace;
mod tool_runtime;
mod users_http;
#[allow(dead_code)]
mod validation_bridge;
mod workspace_checkpoint;

pub(crate) use auth::{get_db, json_error, AuthMiddleware};
pub(crate) use config::load_startup_env_files;
#[cfg(test)]
pub(crate) use config::parse_env_file_line;
pub use config::CodexConfig;
pub use config::Config;
pub use config::OAuth2Config;
pub use db::{Database, RotateResult};
pub use models::{ActionEventRecord, ActionSessionRecord};
pub(crate) use openapi::openapi_json;
pub(crate) use shell_client::{
    shell_agent_job_update, shell_agent_poll, shell_agent_register, shell_agent_result,
    shell_file_op, shell_job, shell_job_log, shell_job_status, shell_job_stop, shell_jobs_list,
    shell_run, ShellClientRegistry,
};
use startup::{server_cli_action, ServerCliAction};

// ============================================================================
// Main
// ============================================================================

/// Whole-service HTTP request timeout (defense in depth). Must stay above the
/// MCP dispatch hard bound (150s) plus response-write margin so the inner,
/// better-reported timeouts always fire first.
const REQUEST_HARD_TIMEOUT_SECS: u64 = 300;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    match server_cli_action(std::env::args().skip(1)) {
        ServerCliAction::Run => {}
        ServerCliAction::Connect(opts) => {
            if let Err(stderr) = hosted_connect::run(opts).await {
                eprintln!("{}", stderr);
                std::process::exit(1);
            }
            return Ok(());
        }
        ServerCliAction::Admin(cmd) => match admin_cli::run_admin_command(cmd).await {
            Ok(stdout) => {
                println!("{}", stdout);
                std::process::exit(0);
            }
            Err(stderr) => {
                eprintln!("{}", stderr);
                std::process::exit(1);
            }
        },
        ServerCliAction::Exit {
            code,
            stdout,
            stderr,
        } => {
            if !stdout.is_empty() {
                print!("{}", stdout);
            }
            if !stderr.is_empty() {
                eprint!("{}", stderr);
            }
            std::process::exit(code);
        }
    }
    let env_loads = load_startup_env_files().map_err(std::io::Error::other)?;
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    for load in &env_loads {
        tracing::info!(
            "Loaded env file {} ({} variables set{})",
            load.path.display(),
            load.loaded_count,
            if load.legacy {
                ", legacy deprecated path"
            } else {
                ""
            }
        );
    }
    let config = Config::from_env();
    if !config.is_auth_enabled() {
        tracing::warn!(
            "WEBCODEX_TOKEN is not set! Running in development mode without authentication. \
Use `webcodex-cli server up` to generate a bootstrap/admin key, or set WEBCODEX_ALLOW_ANONYMOUS=true \
only for local/trusted-network demos."
        );
        tracing::warn!("Anonymous API access is rejected by default in production mode.");
    }
    let build_info = build_info::current();
    tracing::info!(
        "Starting WebCodex v{} (commit {})",
        build_info.version,
        build_info.git_commit.unwrap_or("unknown")
    );
    tracing::info!("Data directory: {:?}", config.data_dir);
    let addr = config.addr.clone();
    tracing::info!("Listening on: {}", addr);
    std::fs::create_dir_all(config.uploads_dir())?;
    let db = Database::open(&config.db_path())?;
    tracing::info!("Database initialized at {:?}", config.db_path());

    // Set max payload size to 2MB for text messages
    salvo::http::request::set_global_secure_max_size(config.max_text_size);

    let cors = Cors::permissive();
    let config = Arc::new(config);
    let db = Arc::new(db);
    // First-party authorize browser session store (in-memory, short-lived).
    // Holds the opaque session id -> user mapping bridging the authorize
    // login form to the consent decision. PAT/bootstrap plaintext is never
    // stored here — only the resolved user identity.
    let authorize_session_store = Arc::new(oauth_http::AuthorizeSessionStore::new());
    let shell_registry = Arc::new(ShellClientRegistry::default());
    let quic_cfg = config::QuicServerConfig::from_env();
    let runtime_info = Arc::new(tool_runtime::RuntimeInfo::from_env_with_quic_config(
        &quic_cfg,
    ));
    let tool_runtime = Arc::new(
        tool_runtime::ToolRuntime::new(
            shell_registry.clone(),
            Arc::new(config.codex.clone()),
            runtime_info.clone(),
        )
        .with_checkpoint_state_dir(config.runtime_state_dir())
        .with_session_ledger(config.session_ledger_path()),
    );
    let connector_runtime =
        connector_runtime::ConnectorRuntime::from_env(tool_runtime.clone(), db.clone())
            .map_err(std::io::Error::other)?;
    if let Some(runtime) = connector_runtime.0.as_ref() {
        tracing::info!(
            project_id = %runtime.context().project_id,
            profile = %runtime.context().profile,
            capabilities = connector_runtime::surface::CAPABILITY_NAMES.len(),
            "Project-bound connector surface enabled"
        );
    }

    // Custom QUIC agent transport. Default disabled;
    // only starts when WEBCODEX_QUIC_ENABLED=true. Runs a separate quinn UDP
    // listener in parallel with the HTTP server. HTTP/WebSocket/polling and
    // the GPT Actions / Nginx path are completely unaffected. This is NOT
    // HTTP/3 and Nginx does not terminate QUIC.
    if quic_cfg.enabled {
        if let Err(e) = quic_cfg.validate() {
            if let Some(status) = runtime_info.quic.as_ref() {
                status
                    .lock()
                    .expect("quic runtime status mutex poisoned")
                    .mark_error(&e);
            }
            tracing::error!(
                "QUIC listener disabled due to config error: {}; check WEBCODEX_QUIC_LISTEN/CERT/KEY/ALPN",
                e
            );
        } else {
            let quic_config = config.clone();
            let quic_db = db.clone();
            let quic_registry = shell_registry.clone();
            let quic_cfg_task = quic_cfg.clone();
            let quic_status = runtime_info.quic.clone();
            tokio::spawn(async move {
                if let Err(e) = agent_quic::run_quic_agent_listener(
                    quic_config,
                    Some(quic_db),
                    quic_registry,
                    quic_cfg_task,
                    quic_status,
                )
                .await
                {
                    tracing::error!(
                        "QUIC agent listener exited with error: {}; check bind address, UDP port availability, certificate/key readability, and ALPN",
                        e
                    );
                }
            });
            tracing::info!(
                "Agent QUIC configured on UDP {} ALPN {}",
                quic_cfg.listen,
                quic_cfg.alpn
            );
        }
    }

    let authed_api_router = Router::new()
        .hoop(AuthMiddleware)
        .push(connector_runtime::http::routes())
        .push(Router::with_path("tools/list").post(runtime_http::tools_list))
        .push(Router::with_path("tools/call").post(runtime_http::tools_call))
        .push(
            Router::with_path("artifacts/import")
                .post(runtime_http::import_conversation_files_to_project),
        )
        .push(Router::with_path("jobs/status").post(runtime_http::job_status))
        .push(Router::with_path("jobs/log").post(runtime_http::job_log))
        .push(Router::with_path("jobs/stop").post(runtime_http::job_stop))
        .push(Router::with_path("jobs/list").post(runtime_http::jobs_list))
        .push(Router::with_path("jobs/tail").post(runtime_http::job_tail))
        .push(Router::with_path("projects/list").post(runtime_http::projects_list))
        .push(Router::with_path("projects/register").post(runtime_http::projects_register))
        .push(Router::with_path("projects/create").post(runtime_http::projects_create))
        .push(Router::with_path("projects/read_file").post(runtime_http::projects_read_file))
        .push(Router::with_path("projects/git_status").post(runtime_http::projects_git_status))
        .push(Router::with_path("projects/git_diff").post(runtime_http::projects_git_diff))
        .push(
            Router::with_path("projects/git_diff_summary")
                .post(runtime_http::projects_git_diff_summary),
        )
        .push(Router::with_path("projects/list_files").post(runtime_http::projects_list_files))
        .push(Router::with_path("projects/search_text").post(runtime_http::projects_search_text))
        .push(Router::with_path("projects/apply_patch").post(runtime_http::projects_apply_patch))
        .push(
            Router::with_path("projects/validate_patch")
                .post(runtime_http::projects_validate_patch),
        )
        .push(Router::with_path("projects/run_shell").post(runtime_http::projects_run_shell))
        .push(
            Router::with_path("projects/apply_patch_checked")
                .post(runtime_http::projects_apply_patch_checked),
        )
        .push(Router::with_path("projects/delete_files").post(runtime_http::projects_delete_files))
        .push(
            Router::with_path("projects/git_restore_paths")
                .post(runtime_http::projects_git_restore_paths),
        )
        .push(
            Router::with_path("projects/discard_untracked")
                .post(runtime_http::projects_discard_untracked),
        )
        .push(
            Router::with_path("projects/replace_in_file")
                .post(runtime_http::projects_replace_in_file),
        )
        .push(Router::with_path("projects/write_file").post(runtime_http::projects_write_file))
        .push(Router::with_path("projects/run_job").post(runtime_http::projects_run_job))
        .push(Router::with_path("runtime/status").post(runtime_http::runtime_status))
        // Phase 2e-3: first-party OAuth client management API. Behind
        // AuthMiddleware; route policy is FirstPartyOnly so OAuth2 access
        // tokens are rejected even with account:manage.
        .push(Router::with_path("oauth/clients/create").post(oauth_http::oauth_clients_create))
        .push(Router::with_path("oauth/clients/list").post(oauth_http::oauth_clients_list))
        .push(Router::with_path("oauth/clients/revoke").post(oauth_http::oauth_clients_revoke))
        // Phase 2 multi-user auth: user + personal API token management.
        // REST-only admin/self-management surface; intentionally NOT
        // exposed in /openapi.json (GPT Actions) because token creation is
        // sensitive. All behind the shared AuthMiddleware Bearer auth.
        .push(Router::with_path("users/create").post(users_http::users_create))
        .push(Router::with_path("users/list").post(users_http::users_list))
        .push(Router::with_path("users/me").post(users_http::users_me))
        .push(Router::with_path("tokens/create").post(users_http::tokens_create))
        .push(Router::with_path("tokens/register_hash").post(users_http::tokens_register_hash))
        .push(Router::with_path("tokens/list").post(users_http::tokens_list))
        .push(Router::with_path("tokens/revoke").post(users_http::tokens_revoke))
        // Phase 3 agent token management: REST-only admin/self-management
        // surface for agent tokens bound to an owner + allowed_client_id.
        // Intentionally NOT exposed in /openapi.json (GPT Actions) because
        // token creation is sensitive. All behind the shared AuthMiddleware
        // Bearer auth. Agent tokens themselves are rejected from these
        // endpoints so a leaked agent token cannot mint more tokens.
        .push(Router::with_path("agent-tokens/create").post(agent_tokens_http::agent_tokens_create))
        .push(
            Router::with_path("agent-tokens/register_hash")
                .post(agent_tokens_http::agent_tokens_register_hash),
        )
        .push(Router::with_path("agent-tokens/list").post(agent_tokens_http::agent_tokens_list))
        .push(Router::with_path("agent-tokens/revoke").post(agent_tokens_http::agent_tokens_revoke))
        .push(Router::with_path("shell/run").post(shell_run))
        .push(Router::with_path("shell/file").post(shell_file_op))
        .push(Router::with_path("shell/job").post(shell_job))
        .push(Router::with_path("shell/jobs/status").post(shell_job_status))
        .push(Router::with_path("shell/jobs/log").post(shell_job_log))
        .push(Router::with_path("shell/jobs/stop").post(shell_job_stop))
        .push(Router::with_path("shell/jobs/list").post(shell_jobs_list))
        .push(Router::with_path("shell/agent/register").post(shell_agent_register))
        .push(Router::with_path("shell/agent/poll").post(shell_agent_poll))
        .push(Router::with_path("shell/agent/result").post(shell_agent_result))
        .push(Router::with_path("shell/agent/job_update").post(shell_agent_job_update))
        // WebSocket agent transport (preferred long-lived connection).
        // Polling endpoints above remain as fallback. Bearer auth is
        // enforced by the shared AuthMiddleware hoop.
        .push(Router::with_path("agents/ws").get(agent_ws::agent_ws));

    let api_router = Router::with_path("api")
        .push(Router::with_path("pairing/enroll").post(pairing_http::pairing_enroll))
        .push(
            authed_api_router
                .push(Router::with_path("pairing/create").post(pairing_http::pairing_create)),
        );

    let openapi_router = Router::with_path("openapi.json").get(openapi_json);

    // Read-only MCP App console (Phase B). Public static entry — the HTML/JS/CSS
    // bundle carries no secrets; all runtime data is fetched by the browser
    // from the protected `POST /api/runtime/status` endpoint. Mirrors
    // `/openapi.json` being public. NOT part of the GPT Actions schema.
    let console_router = Router::with_path("console")
        .get(console_web::console_html)
        .push(Router::with_path("app.js").get(console_web::console_app_js))
        .push(Router::with_path("styles.css").get(console_web::console_styles_css));

    let mut router = Router::new()
        // Whole-service backstop: no handler may hold an HTTP request open
        // forever. Sized well above every legitimate request — sync agent
        // waits are <= ~122s and MCP dispatch is hard-bounded at 150s — so it
        // only fires on a genuinely unbounded hang, converting a permanently
        // silent request into an explicit 503. Long-lived work is unaffected:
        // agent polling replies immediately and WebSocket connections live in
        // a task spawned after the (fast) upgrade handshake completes.
        .hoop(salvo::timeout::Timeout::new(
            std::time::Duration::from_secs(REQUEST_HARD_TIMEOUT_SECS),
        ))
        .hoop(affix_state::inject(config.clone()))
        .hoop(affix_state::inject(db.clone()))
        .hoop(affix_state::inject(authorize_session_store.clone()))
        .hoop(affix_state::inject(shell_registry.clone()))
        .hoop(affix_state::inject(tool_runtime.clone()))
        .hoop(affix_state::inject(connector_runtime.clone()))
        .hoop(cors.into_handler())
        .push(api_router)
        .push(openapi_router)
        .push(console_router)
        // OAuth2 token, revocation, and discovery endpoints — public, no
        // AuthMiddleware. Token/revoke clients authenticate via
        // client_id + client_secret in the form body.
        .push(Router::with_path("oauth/token").post(oauth_http::oauth_token))
        .push(Router::with_path("oauth/revoke").post(oauth_http::oauth_revoke))
        // /oauth/authorize is NOT behind AuthMiddleware: the handler accepts
        // either a first-party Bearer token (Bootstrap / PAT, backward
        // compatible direct code issuance) or a short-lived authorize
        // session cookie set by the login form. login/consent do their own
        // token/session validation.
        .push(
            Router::with_path("oauth/authorize")
                .get(oauth_http::oauth_authorize)
                .push(Router::with_path("login").post(oauth_http::oauth_authorize_login))
                .push(Router::with_path("consent").post(oauth_http::oauth_authorize_consent))
                .push(Router::with_path("bridge").post(oauth_http::oauth_authorize_bridge)),
        )
        .push(
            Router::with_path(".well-known/oauth-protected-resource")
                .get(oauth_http::oauth_metadata),
        )
        .push(
            Router::with_path(".well-known/oauth-authorization-server")
                .get(oauth_http::oauth_authorization_server_metadata),
        )
        .push(
            Router::with_path("mcp")
                .hoop(AuthMiddleware)
                .get(mcp::mcp_info)
                .post(mcp::mcp_post),
        );

    // Read-only audit query API. Admin/debug surface only: NOT part of the
    // GPT Actions OpenAPI schema. All endpoints are POST + Bearer auth.
    router = router.push(
        Router::with_path("api/audit")
            .hoop(AuthMiddleware)
            .push(Router::with_path("sessions").post(audit_http::audit_sessions))
            .push(Router::with_path("session").post(audit_http::audit_session))
            .push(Router::with_path("stats").post(audit_http::audit_stats)),
    );
    let acceptor = TcpListener::new(addr.clone()).bind().await;
    tracing::info!("Server started successfully!");
    let port = addr.split(':').last().unwrap_or("8080");
    let base = format!("http://localhost:{}", port);
    tracing::info!("Runtime base: {}", base);
    tracing::info!("MCP endpoint: {}/mcp", base);
    tracing::info!(
        tool_request_trace = crate::config::tool_request_trace_enabled(),
        "tool_request_trace"
    );
    tracing::info!(
        mcp_compact_schemas = crate::config::mcp_compact_schemas_enabled(),
        "mcp_compact_schemas"
    );
    tracing::info!(
        action_compact_responses = crate::config::action_compact_responses_enabled(),
        "action_compact_responses"
    );
    tracing::info!("OpenAPI (GPT Actions): {}/openapi.json", base);
    tracing::info!("MCP App console: {}/console", base);
    tracing::info!("Runtime status: {}/api/runtime/status", base);
    tracing::info!("Agent WebSocket: {}/api/agents/ws", base);
    tracing::info!("Agent polling (fallback): {}/api/shell/agent/poll", base);
    tracing::info!("Audit API (read-only): {}/api/audit/sessions", base);
    Server::new(acceptor).serve(router).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_env_file_line_basic() {
        let parsed = parse_env_file_line("WEBCODEX_ADDR=127.0.0.1:8080")
            .unwrap()
            .unwrap();
        assert_eq!(parsed.0, "WEBCODEX_ADDR");
        assert_eq!(parsed.1, "127.0.0.1:8080");
    }

    #[test]
    fn test_parse_env_file_line_quotes_and_export() {
        let parsed = parse_env_file_line("export RUST_LOG='info,codex.metrics=info'")
            .unwrap()
            .unwrap();
        assert_eq!(parsed.0, "RUST_LOG");
        assert_eq!(parsed.1, "info,codex.metrics=info");
    }

    #[test]
    fn test_parse_env_file_line_ignores_empty_and_comments() {
        assert!(parse_env_file_line("").is_none());
        assert!(parse_env_file_line("  # comment").is_none());
    }

    #[test]
    fn test_parse_env_file_line_rejects_invalid_key() {
        assert!(parse_env_file_line("webcodex_token=x").unwrap().is_err());
        assert!(parse_env_file_line("DROP TOKEN=x").unwrap().is_err());
    }

    #[test]
    fn test_uuid_generation_not_empty() {
        let id = Uuid::new_v4().to_string();
        assert!(!id.is_empty());
        assert_eq!(id.len(), 36); // UUID v4 with hyphens
        assert!(id.contains('-'));
    }

    #[test]
    fn test_uuid_generation_unique() {
        let id1 = Uuid::new_v4().to_string();
        let id2 = Uuid::new_v4().to_string();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_config_from_env_defaults() {
        let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
        // Clear env vars to test defaults
        std::env::remove_var("WEBCODEX_ADDR");
        std::env::remove_var("WEBCODEX_DATA");
        std::env::remove_var("WEBCODEX_TOKEN");
        std::env::remove_var("CODEX_BIN");
        std::env::remove_var("CODEX_APPROVAL_MODE");
        std::env::remove_var("CODEX_DEFAULT_TIMEOUT_SECS");
        std::env::remove_var("CODEX_MAX_PROMPT_BYTES");
        std::env::remove_var("CODEX_ALLOWED_EXTRA_ARGS");

        let config = Config::from_env();
        assert_eq!(config.addr, "0.0.0.0:8080");
        assert_eq!(config.data_dir, PathBuf::from("./data"));
        assert_eq!(config.token, None);
        assert!(!config.is_auth_enabled());
        assert_eq!(config.max_text_size, 2 * 1024 * 1024);
        assert_eq!(config.max_file_size, 100 * 1024 * 1024);
        assert_eq!(config.codex.bin, "codex");
        assert_eq!(config.codex.approval_mode, "");
        assert_eq!(config.codex.default_timeout_secs, 3600);
        assert_eq!(config.codex.max_prompt_bytes, 100_000);
        assert!(config.codex.allowed_extra_args.is_empty());
    }

    #[test]
    fn test_config_validate_token() {
        let config = Config {
            addr: "0.0.0.0:8080".to_string(),
            data_dir: PathBuf::from("./data"),
            token: Some("secret123".to_string()),
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: CodexConfig::default(),
            oauth2: crate::OAuth2Config::default(),
        };
        assert!(config.is_auth_enabled());
        assert!(config.validate_token("secret123"));
        assert!(!config.validate_token("wrong"));
        assert!(!config.validate_token(""));
    }

    #[test]
    fn test_config_validate_token_none() {
        let config = Config {
            addr: "0.0.0.0:8080".to_string(),
            data_dir: PathBuf::from("./data"),
            token: None,
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: CodexConfig::default(),
            oauth2: crate::OAuth2Config::default(),
        };
        assert!(!config.is_auth_enabled());
        // When no token is set, validation always returns false
        assert!(!config.validate_token("anything"));
    }

    #[test]
    fn test_filename_sanitization() {
        // Test that path separators are stripped from display names
        let filename = "test/file\\name.txt";
        let safe: String = filename
            .chars()
            .filter(|c| !matches!(c, '/' | '\\' | '\0' | '\r' | '\n'))
            .collect();
        assert_eq!(safe, "testfilename.txt");
    }

    #[test]
    fn test_filename_sanitization_quotes() {
        let filename = "file\"name.txt";
        let safe = filename.replace('"', "_");
        assert_eq!(safe, "file_name.txt");
    }
}
