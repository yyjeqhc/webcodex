pub(crate) mod agent_service;
pub(crate) mod env;
pub(crate) mod http;
pub(crate) mod ops;
pub(crate) mod output;
pub(crate) mod pairing;
pub(crate) mod profiles;
pub(crate) mod server;
pub(crate) mod setup;
pub(crate) mod system;
#[cfg(test)]
pub(crate) mod test_support;
pub(crate) mod token_commands;
pub(crate) mod tokens;
pub(crate) mod usage;

#[cfg(test)]
pub(crate) use agent_service::render_agent_systemd_unit;
pub(crate) use agent_service::{run_agent_install_service, run_agent_status};
#[cfg(test)]
pub(crate) use env::is_effective_root;
#[cfg(test)]
pub(crate) use env::parse_env_content_value;
pub(crate) use env::{
    default_server_paths, read_env_file_value, read_pairing_server_env_file_value,
    render_server_env,
};
#[cfg(test)]
pub(crate) use http::format_error_body;
pub(crate) use http::{
    fetch_runtime_status, http_post_json_status, post_json_authed, post_json_unauthed, ApiCall,
};
pub(crate) use ops::{run_ops_command, OpsCommand, OpsCommonOptions, OpsSmokePreflightOptions};
#[cfg(test)]
pub(crate) use output::RevisionComparison;
pub(crate) use output::{
    compare_build_commits, local_cli_build_metadata, render_build_metadata_block,
    runtime_build_metadata, server_status_revision_check,
};
#[cfg(test)]
pub(crate) use pairing::{ensure_enroll_outputs_available, resolve_pairing_create_token};
pub(crate) use pairing::{run_client_enroll, run_pairing_create};
#[cfg(test)]
pub(crate) use profiles::{client_output_dir_for_profile, CLIENT_PROFILE_ERROR};
pub(crate) use profiles::{
    client_profile_agent_config, client_profile_agent_token_file, client_profile_projects_dir,
    client_profile_service_file, client_profile_user_token_file,
    default_client_output_dir_for_profile, validate_client_profile,
};
pub(crate) use server::{
    run_server_init, run_server_install_service, run_server_status, run_server_up,
    ServerStatusOptions,
};
pub(crate) use setup::run_setup_single_user;
pub(crate) use system::{
    discover_named_binary_absolute, discover_webcodex_binary, is_systemd_platform,
    query_systemd_service_status, query_systemd_status, read_optional_token, write_secret_file,
    write_text_file,
};
#[cfg(test)]
pub(crate) use token_commands::resolve_account_credential;
pub(crate) use token_commands::{run_agent_token_create_local, run_token_create_local};
pub(crate) use tokens::{
    generate_bootstrap_token, generate_local_agent_token, generate_local_api_token,
    hash_local_token, local_token_prefix, render_token_generate, resolve_token, token_prefix,
};
pub(crate) use usage::{
    agent_init_usage, agent_install_service_usage, agent_status_usage, agent_usage,
    client_enroll_usage, client_usage, ops_agents_usage, ops_projects_usage,
    ops_smoke_preflight_usage, ops_status_usage, ops_usage, pairing_create_usage, pairing_usage,
    server_init_usage, server_install_service_usage, server_status_usage, server_up_usage,
    server_usage, usage,
};
