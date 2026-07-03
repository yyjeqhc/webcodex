pub(crate) mod config;
pub(crate) mod output;
pub(crate) mod projects;
pub(crate) mod shell;
pub(crate) mod transport;

pub(crate) use config::{
    client_profile_agent_config, default_config_path, hostname, load_config, projects_dir,
    validate_client_profile, AgentConfig, AgentPolicy, ShellConfig,
};
#[cfg(test)]
pub(crate) use config::{
    default_quic_alpn, default_quic_connect_timeout_secs, default_quic_keepalive_interval_secs,
    max_concurrent_jobs, QuicClientConfig, ShellProfileConfig, CLIENT_PROFILE_ERROR,
    DEFAULT_MAX_CONCURRENT_JOBS,
};
pub(crate) use output::{err_cmd, line_edit_stdout, ok_cmd, CommandResult};
#[cfg(test)]
pub(crate) use projects::{
    agent_project_summary, load_agent_project_summaries_from_dir, parse_agent_project_toml,
    validate_project_path_policy,
};
pub(crate) use projects::{handle_project_op, AgentProjectCache};
#[cfg(test)]
pub(crate) use shell::run_shell;
pub(crate) use shell::{
    configured_prepared_shell_job_command, configured_shell_job_command, cwd_allowed,
    resolve_prepared_shell_profile, run_shell_with_profiles, PreparedShellProfileCache,
};
#[cfg(test)]
pub(crate) use transport::{
    auto_transport_plan, build_ws_request, effective_transport, non_empty_token,
    quic_client_bind_addr_for, resolve_quic_config, resolve_quic_server_addrs, server_url_to_ws,
    websocket_session, WS_OUTGOING_CAPACITY,
};
pub(crate) use transport::{run_agent, AgentSink, HttpSendConfig};
