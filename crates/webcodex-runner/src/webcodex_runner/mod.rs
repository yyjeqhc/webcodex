pub(crate) mod artifacts;
pub(crate) mod checkpoints;
pub(crate) mod config;
pub(crate) mod dispatch;
pub(crate) mod external_tools;
pub(crate) mod files;
pub(crate) mod lsp;
pub(crate) mod output;
pub(crate) mod patches;
pub(crate) mod persistent_shell;
pub(crate) mod projects;
// The remote SSH persistent-shell transport is built on Unix process groups and
// inherited descriptors; Windows has no persistent shell yet (the local shell
// fails closed there and `ssh_persistent_shell` is advertised false), so the
// whole module is compiled out on non-Unix targets.
#[cfg(unix)]
pub(crate) mod remote_shell;
pub(crate) mod shell;
pub(crate) mod shutdown;
pub(crate) mod ssh;
pub(crate) mod transport;
pub(crate) mod util;
pub(crate) mod validation;

pub(crate) use artifacts::{handle_artifact_file_request, is_artifact_request_kind};
pub(crate) use checkpoints::{handle_checkpoint_file_request, is_checkpoint_request_kind};
pub(crate) use config::SshConfig;
pub(crate) use config::{
    client_profile_agent_config, default_config_path, hostname, load_config, projects_dir,
    validate_client_profile, AgentConfig, AgentPolicy, HotAgentConfig, ReloadableAgentConfig,
    ShellConfig,
};
#[cfg(test)]
pub(crate) use config::{
    default_quic_alpn, default_quic_connect_timeout_secs, default_quic_keepalive_interval_secs,
    default_websocket_connect_timeout_secs, max_concurrent_jobs, QuicClientConfig,
    ShellProfileConfig, CLIENT_PROFILE_ERROR, DEFAULT_MAX_CONCURRENT_JOBS,
};
pub(super) use dispatch::{dispatch_request, is_project_op};
#[cfg(test)]
pub(crate) use files::sha256_hex_bytes;
pub(crate) use files::{
    handle_basic_file_request, is_basic_file_request_kind, resolve_requested_path,
};
pub(crate) use lsp::LspSupervisor;
pub(crate) use output::{err_cmd, ok_cmd, CommandResult, ShellCommandResult};
pub(crate) use patches::{
    handle_apply_text_edits_file_request, handle_write_project_file_request,
    is_structured_edit_request_kind, validate_structured_edit_agent_path,
};
pub(crate) use persistent_shell::PersistentShellManager;
#[cfg(test)]
pub(crate) use projects::handle_project_op;
#[cfg(test)]
pub(crate) use projects::load_agent_project_summaries_from_dir;
#[cfg(test)]
pub(crate) use projects::{
    agent_project_summary, parse_agent_project_toml, validate_project_path_policy,
};
pub(crate) use projects::{
    handle_project_lifecycle_op, handle_project_op_with_temporary_projects_root,
    handle_resolve_or_register_project, AgentProjectCache,
};
#[cfg(test)]
pub(crate) use shell::run_shell;
#[cfg(test)]
pub(crate) use shell::run_shell_with_profiles;
pub(crate) use shell::{
    configured_prepared_shell_job_command, configured_shell_job_command,
    configured_validation_job_command, cwd_allowed, resolve_prepared_shell_profile,
    run_shell_with_profiles_in_sandbox, run_shell_with_profiles_in_sandbox_and_execution_state,
    PreparedShellProfile, PreparedShellProfileCache,
};
pub(crate) use ssh::{is_transport_failure, run_ssh_shell_with_execution_state, SshConnectionPool};
#[cfg(test)]
pub(crate) use transport::{
    auto_transport_plan, build_ws_request, effective_transport, non_empty_token,
    quic_client_bind_addr_for, resolve_quic_config, resolve_quic_server_addrs, server_url_to_ws,
    websocket_session, AgentRuntimeState, ResultSubmission, WS_OUTGOING_CAPACITY,
};
pub(crate) use transport::{run_agent, AgentSink, HttpSendConfig, SubmitResultError};
pub(crate) use util::contains_any;
