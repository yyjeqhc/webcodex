#![recursion_limit = "256"]

use salvo::cors::Cors;
use salvo::prelude::*;
#[cfg(test)]
use std::path::PathBuf;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;
#[cfg(test)]
use uuid::Uuid;

mod action_audit;
mod action_sessions;
mod agent_ws;
mod audit_http;
mod auth;
mod codex;
mod config;
mod console_web;
mod db;
mod mcp;
mod models;
mod openapi;
mod projects;
mod runtime_http;
mod shell_client;
mod shell_protocol;
mod tool_runtime;

pub(crate) use auth::{get_db, json_error, AuthMiddleware};
pub(crate) use config::load_startup_env_files;
#[cfg(test)]
pub(crate) use config::parse_env_file_line;
pub use config::CodexConfig;
pub use config::Config;
pub use db::Database;
pub use models::{
    ActionEventRecord, ActionSessionRecord, AgentModelProfileRecord, AgentSpecRecord, Channel,
    CodexGoalRecord, CommandAuditRecord, Message, MessageKind,
};
pub(crate) use openapi::openapi_json;
pub(crate) use shell_client::{
    shell_agent_job_update, shell_agent_poll, shell_agent_register, shell_agent_result,
    shell_file_op, shell_job, shell_job_log, shell_job_status, shell_job_stop, shell_jobs_list,
    shell_run, ShellClientRegistry,
};

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let env_loads = load_startup_env_files().map_err(std::io::Error::other)?;
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    for load in &env_loads {
        tracing::info!(
            "Loaded env file {} ({} variables set)",
            load.path.display(),
            load.loaded_count
        );
    }
    let config = Config::from_env();
    if !config.is_auth_enabled() {
        tracing::warn!(
            "DROP_TOKEN is not set! Running in development mode without authentication."
        );
        tracing::warn!("Set DROP_TOKEN environment variable to enable authentication.");
    }
    tracing::info!("Starting Private Drop v{}", env!("CARGO_PKG_VERSION"));
    tracing::info!("Data directory: {:?}", config.data_dir);
    let addr = config.addr.clone();
    tracing::info!("Listening on: {}", addr);
    std::fs::create_dir_all(config.uploads_dir())?;
    let db = Database::open(&config.db_path())?;
    tracing::info!("Database initialized at {:?}", config.db_path());

    // Set max payload size to 2MB for text messages
    salvo::http::request::set_global_secure_max_size(config.max_text_size);

    // Load projects config for Codex API. Keep Codex routes mounted even when
    // the config is invalid so callers get a structured JSON error instead of
    // a confusing router-level 404.
    let projects_config_path = projects::ProjectsConfig::config_path_from_env();
    let projects_state = match projects::ProjectsConfig::load() {
        Ok(cfg) => {
            tracing::info!(
                "Loaded projects config {} with {} projects",
                projects_config_path,
                cfg.projects.len()
            );
            projects::ProjectsState::loaded(cfg, projects_config_path)
        }
        Err(e) => {
            tracing::warn!(
                "Projects config not loaded from {}: {}. Codex API will return config errors.",
                projects_config_path,
                e
            );
            projects::ProjectsState::failed(e, projects_config_path)
        }
    };

    let cors = Cors::permissive();
    let config = Arc::new(config);
    let db = Arc::new(db);
    let projects_state = Arc::new(projects_state);
    let shell_registry = Arc::new(ShellClientRegistry::default());
    let runtime_info = Arc::new(tool_runtime::RuntimeInfo::from_env());
    let tool_runtime = Arc::new(tool_runtime::ToolRuntime::new(
        projects_state.clone(),
        shell_registry.clone(),
        Arc::new(config.codex.clone()),
        runtime_info.clone(),
    ));

    let api_router = Router::with_path("api").push(
        Router::new()
            .hoop(AuthMiddleware)
            .push(Router::with_path("tools/list").post(runtime_http::tools_list))
            .push(Router::with_path("tools/call").post(runtime_http::tools_call))
            .push(Router::with_path("codex/run").post(runtime_http::codex_run))
            .push(Router::with_path("jobs/status").post(runtime_http::job_status))
            .push(Router::with_path("jobs/log").post(runtime_http::job_log))
            .push(Router::with_path("jobs/stop").post(runtime_http::job_stop))
            .push(Router::with_path("jobs/list").post(runtime_http::jobs_list))
            .push(Router::with_path("jobs/tail").post(runtime_http::job_tail))
            .push(Router::with_path("projects/list").post(runtime_http::projects_list))
            .push(Router::with_path("projects/read_file").post(runtime_http::projects_read_file))
            .push(Router::with_path("projects/git_status").post(runtime_http::projects_git_status))
            .push(Router::with_path("projects/git_diff").post(runtime_http::projects_git_diff))
            .push(
                Router::with_path("projects/git_diff_summary")
                    .post(runtime_http::projects_git_diff_summary),
            )
            .push(Router::with_path("projects/list_files").post(runtime_http::projects_list_files))
            .push(
                Router::with_path("projects/search_text").post(runtime_http::projects_search_text),
            )
            .push(
                Router::with_path("projects/apply_patch").post(runtime_http::projects_apply_patch),
            )
            .push(
                Router::with_path("projects/validate_patch")
                    .post(runtime_http::projects_validate_patch),
            )
            .push(Router::with_path("projects/run_shell").post(runtime_http::projects_run_shell))
            .push(Router::with_path("runtime/status").post(runtime_http::runtime_status))
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
            .push(Router::with_path("agents/ws").get(agent_ws::agent_ws)),
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
        .hoop(affix_state::inject(config.clone()))
        .hoop(affix_state::inject(db.clone()))
        .hoop(affix_state::inject(projects_state.clone()))
        .hoop(affix_state::inject(shell_registry.clone()))
        .hoop(affix_state::inject(tool_runtime.clone()))
        .hoop(cors.into_handler())
        .push(api_router)
        .push(openapi_router)
        .push(console_router)
        .push(
            Router::with_path("mcp")
                .hoop(AuthMiddleware)
                .get(mcp::mcp_info)
                .post(mcp::mcp_post),
        );

    // Codex API routes are always mounted. If projects.toml failed to load,
    // handlers return structured errors instead of disappearing with 404.
    router = router.push(
        Router::with_path("api/codex")
            .hoop(AuthMiddleware)
            .push(Router::with_path("context").post(codex::codex_context))
            .push(Router::with_path("projects").post(codex::codex_projects))
            .push(Router::with_path("context_batch").post(codex::codex_context_batch))
            .push(Router::with_path("apply_patch").post(codex::codex_apply_patch))
            .push(Router::with_path("edit").post(codex::codex_edit))
            .push(Router::with_path("artifact").post(codex::codex_artifact))
            .push(Router::with_path("git").post(codex::codex_git))
            .push(Router::with_path("job").post(codex::codex_job))
            .push(Router::with_path("report").post(codex::codex_report)),
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
        let parsed = parse_env_file_line("DROP_ADDR=127.0.0.1:8080")
            .unwrap()
            .unwrap();
        assert_eq!(parsed.0, "DROP_ADDR");
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
        assert!(parse_env_file_line("drop_token=x").unwrap().is_err());
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
        let _guard = crate::config::TEST_ENV_LOCK.lock().unwrap();
        // Clear env vars to test defaults
        std::env::remove_var("DROP_ADDR");
        std::env::remove_var("DROP_DATA");
        std::env::remove_var("DROP_TOKEN");
        std::env::remove_var("DROP_ENABLE_SSH");
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
        assert!(!config.is_ssh_enabled());
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
            enable_ssh: false,
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: CodexConfig::default(),
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
            enable_ssh: false,
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: CodexConfig::default(),
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

    #[test]
    fn test_command_request_claim_is_atomic() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("drop.db")).unwrap();
        let record = CommandAuditRecord {
            id: "req-1".to_string(),
            project: "p".to_string(),
            command: "smoke".to_string(),
            command_text: Some("echo ok".to_string()),
            reason: Some("test".to_string()),
            status: "pending".to_string(),
            created_at: 1,
            approved_at: None,
            executed_at: None,
            exit_code: None,
            stdout_tail: None,
            stderr_tail: None,
            error: None,
        };
        db.insert_command_request(&record).unwrap();
        let claimed = db
            .claim_command_request_for_execution("req-1", 2, 0)
            .unwrap()
            .unwrap();
        assert_eq!(claimed.status, "running");
        assert_eq!(claimed.approved_at, Some(2));
        assert_eq!(claimed.command_text.as_deref(), Some("echo ok"));
        let second = db
            .claim_command_request_for_execution("req-1", 3, 0)
            .unwrap();
        assert!(second.is_none());
        let current = db.get_command_request("req-1").unwrap().unwrap();
        assert_eq!(current.status, "running");
        assert_eq!(current.approved_at, Some(2));
    }

    #[test]
    fn test_command_request_claim_respects_ttl() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("drop.db")).unwrap();
        let record = CommandAuditRecord {
            id: "old-req".to_string(),
            project: "p".to_string(),
            command: "smoke".to_string(),
            command_text: Some("echo ok".to_string()),
            reason: None,
            status: "pending".to_string(),
            created_at: 10,
            approved_at: None,
            executed_at: None,
            exit_code: None,
            stdout_tail: None,
            stderr_tail: None,
            error: None,
        };
        db.insert_command_request(&record).unwrap();
        let claimed = db
            .claim_command_request_for_execution("old-req", 100, 50)
            .unwrap();
        assert!(claimed.is_none());
        let current = db.get_command_request("old-req").unwrap().unwrap();
        assert_eq!(current.status, "pending");
    }

    #[test]
    fn test_command_request_reject_only_pending() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("drop.db")).unwrap();
        let record = CommandAuditRecord {
            id: "reject-req".to_string(),
            project: "p".to_string(),
            command: "smoke".to_string(),
            command_text: Some("echo ok".to_string()),
            reason: None,
            status: "pending".to_string(),
            created_at: 1,
            approved_at: None,
            executed_at: None,
            exit_code: None,
            stdout_tail: None,
            stderr_tail: None,
            error: None,
        };
        db.insert_command_request(&record).unwrap();
        let rejected = db
            .reject_command_request("reject-req", 2, "no")
            .unwrap()
            .unwrap();
        assert_eq!(rejected.status, "rejected");
        assert_eq!(rejected.error.as_deref(), Some("no"));
        let second = db.reject_command_request("reject-req", 3, "again").unwrap();
        assert!(second.is_none());
    }

    #[test]
    fn test_message_serialization() {
        let msg = Message {
            id: "test-id".to_string(),
            channel: "inbox".to_string(),
            kind: MessageKind::Text,
            title: Some("Test".to_string()),
            text: Some("Hello".to_string()),
            file_name: None,
            file_path: None,
            file_size: None,
            mime_type: None,
            created_at: 1234567890,
            expires_at: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("test-id"));
        assert!(json.contains("inbox"));
        assert!(json.contains("text"));
    }
}
