pub(crate) mod config;
pub(crate) mod projects;

pub(crate) use config::{
    client_profile_agent_config, default_config_path, hostname, load_config, max_concurrent_jobs,
    projects_dir, validate_client_profile, validate_shell_config, AgentConfig, AgentPolicy,
    QuicClientConfig, ShellConfig, ShellProfileConfig,
};
#[cfg(test)]
pub(crate) use config::{
    default_quic_alpn, default_quic_connect_timeout_secs, default_quic_keepalive_interval_secs,
    CLIENT_PROFILE_ERROR, DEFAULT_MAX_CONCURRENT_JOBS,
};
#[cfg(test)]
pub(crate) use projects::{
    agent_project_summary, load_agent_project_summaries_from_dir, parse_agent_project_toml,
    validate_project_path_policy,
};
pub(crate) use projects::{find_project_shell_context, handle_project_op, AgentProjectCache};
