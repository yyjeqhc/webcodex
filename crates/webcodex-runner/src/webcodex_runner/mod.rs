pub(crate) mod artifacts;
pub(crate) mod browser;
#[cfg(feature = "workspace-checkpoints")]
pub(crate) mod checkpoints;
pub(crate) mod coding_agent;
pub(crate) mod computer;
pub(crate) mod computer_session;
pub(crate) mod config;
pub(crate) mod configured_skills;
pub(crate) mod detached_job;
pub(crate) mod dispatch;
pub(crate) mod execution_io;
#[cfg(windows)]
pub(crate) mod exit_diagnostics;
pub(crate) mod external_tools;
pub(crate) mod file_dispatch;
pub(crate) mod files;
pub(crate) mod job_manager;
pub(crate) mod lsp;
pub(crate) mod managed_ssh;
pub(crate) mod mcp_gateway;
pub(crate) mod output;
pub(crate) mod output_text;
pub(crate) mod patches;
pub(crate) mod persistent_shell;
pub(crate) mod plugin;
pub(crate) mod project_build;
#[cfg(test)]
mod project_build_tests;
pub(crate) mod projects;
pub(crate) mod runner_instructions;
pub(crate) mod runner_skills;
// Remote persistent shells always run POSIX sh/bash on the SSH target. Their
// local child ownership is platform-specific: Unix uses a private process group,
// while Windows owns ssh.exe through ManagedChild's Job Object.
#[cfg(any(unix, windows))]
pub(crate) mod remote_shell;
pub(crate) mod shell;
pub(crate) mod shutdown;
pub(crate) mod skill_store;
pub(crate) mod ssh;
mod string_match;
pub(crate) mod transport;
pub(crate) mod util {
    pub(crate) use webcodex_process::{
        find_executable_in_path, is_executable_file, resolve_program_in_path, ResolvedProgram,
    };
}
pub(crate) mod validation;

#[cfg(test)]
pub(crate) use artifacts::is_artifact_request_kind;
pub(crate) use browser::handle_browser_operation;
pub(crate) use computer::handle_computer_operation;
#[cfg(test)]
pub(crate) use config::SshConfig;
#[cfg(test)]
pub(crate) use config::{
    default_quic_alpn, default_quic_connect_timeout_secs, default_quic_keepalive_interval_secs,
    default_websocket_connect_timeout_secs, QuicClientConfig, ShellProfileConfig,
    CLIENT_PROFILE_ERROR, DEFAULT_MAX_CONCURRENT_JOBS,
};
pub(crate) use config::{HotRunnerConfig, ReloadableRunnerConfig, RunnerPolicy, ShellConfig};
#[cfg(test)]
pub(super) use dispatch::dispatch_request;
#[cfg(test)]
pub(crate) use files::is_basic_file_request_kind;
#[cfg(test)]
pub(crate) use files::resolve_requested_path;
#[cfg(test)]
pub(crate) use files::sha256_hex_bytes;
pub(crate) use lsp::LspSupervisor;
pub(crate) use output::{err_cmd, ok_cmd, CommandResult, ShellCommandResult};
#[cfg(test)]
pub(crate) use patches::is_structured_edit_request_kind;
pub(crate) use persistent_shell::PersistentShellManager;
#[cfg(test)]
pub(crate) use projects::{
    handle_prepare_managed_worktree, handle_project_lifecycle_op, handle_project_op,
    handle_resolve_or_register_project, load_runner_project_summaries_from_dir,
};
pub(crate) use projects::{
    handle_prepare_managed_worktree_operation, handle_project_lifecycle_operation,
    handle_project_operation, handle_resolve_or_register_project_operation,
};
#[cfg(test)]
pub(crate) use projects::{
    parse_runner_project_toml, runner_project_summary, validate_project_path_policy,
};
pub(crate) use runner_instructions::handle_runner_instruction_request;
pub(crate) use runner_skills::{
    handle_runner_skill_request, run_skill_resource_with_profiles_and_execution_state,
};
#[cfg(windows)]
pub(crate) use shell::run_windows_native_single_file_search_with_profiles;
#[cfg(test)]
pub(crate) use shell::{
    cwd_allowed, run_shell, run_shell_with_profiles, PreparedShellProfileCache,
};
pub(crate) use shell::{
    explicit_shell_available, run_internal_posix_script_with_profiles_and_execution_state,
    run_internal_search_script_with_profiles_and_execution_state,
    run_process_with_profiles_and_execution_state, run_script_with_profiles_and_execution_state,
    run_shell_with_profiles_and_execution_state,
};
pub(crate) use ssh::run_ssh_shell_with_execution_state;
pub(crate) use string_match::contains_any;
#[cfg(all(test, unix))]
pub(crate) use transport::install_reload_listener;
#[cfg(test)]
pub(crate) use transport::{
    auto_transport_plan, build_ws_request, effective_transport, non_empty_token,
    quic_client_bind_addr_for, resolve_quic_config, resolve_quic_server_addrs, server_url_to_ws,
    websocket_session, ResultSubmission, RunnerRuntimeState, WS_OUTGOING_CAPACITY,
};
pub(crate) use transport::{RunnerSink, SubmitResultError};

#[cfg(test)]
pub(crate) use config::{
    client_profile_runner_config, load_config, max_concurrent_jobs, project_registry_dir,
    RunnerConfig,
};
#[cfg(test)]
pub(crate) use projects::RunnerProjectCache;
#[cfg(test)]
pub(crate) use ssh::SshConnectionPool;
#[cfg(test)]
pub(crate) use transport::HttpSendConfig;
