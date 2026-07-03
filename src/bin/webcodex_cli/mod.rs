pub(crate) mod agent_service;
pub(crate) mod connect;
pub(crate) mod doctor;
pub(crate) mod env;
pub(crate) mod http;
pub(crate) mod output;
pub(crate) mod pairing;
pub(crate) mod server;
pub(crate) mod setup;
pub(crate) mod token_commands;
pub(crate) mod tokens;

#[cfg(test)]
pub(crate) use agent_service::render_agent_systemd_unit;
pub(crate) use agent_service::{run_agent_install_service, run_agent_status};
pub(crate) use connect::run_connect;
pub(crate) use doctor::run_doctor;
#[cfg(test)]
pub(crate) use env::parse_env_content_value;
pub(crate) use env::{
    default_server_paths, is_effective_root, read_env_file_value,
    read_pairing_server_env_file_value, render_server_env,
};
#[cfg(test)]
pub(crate) use http::format_error_body;
pub(crate) use http::{
    fetch_runtime_status, http_get_json_status, http_post_json_status, post_json_authed,
    post_json_unauthed, ApiCall,
};
pub(crate) use output::{
    compare_build_commits, doctor_revision_check, local_cli_build_metadata,
    render_build_metadata_block, runtime_build_metadata, server_status_revision_check, DoctorCheck,
};
#[cfg(test)]
pub(crate) use output::{RevisionComparison, RuntimeBuildMetadata};
#[cfg(test)]
pub(crate) use pairing::{ensure_enroll_outputs_available, resolve_pairing_create_token};
pub(crate) use pairing::{run_client_enroll, run_pairing_create};
pub(crate) use server::{run_server_init, run_server_install_service, run_server_up};
pub(crate) use setup::run_setup_single_user;
#[cfg(test)]
pub(crate) use token_commands::resolve_account_credential;
pub(crate) use token_commands::{run_agent_token_create_local, run_token_create_local};
pub(crate) use tokens::{
    generate_bootstrap_token, generate_local_agent_token, generate_local_api_token,
    hash_local_token, local_token_prefix, render_token_generate, resolve_token, token_prefix,
};
