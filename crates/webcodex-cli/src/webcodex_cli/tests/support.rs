pub(super) use crate::admin_cli::{build_admin_request, AdminCliCommand};
pub(super) use crate::webcodex_cli::test_support::{
    args, build_metadata, cli_exit, env_test_guard, EnvGuard,
};
pub(super) use crate::webcodex_cli::{
    client_output_dir_for_profile, compare_build_commits, format_error_body, is_effective_root,
    parse_env_content_value, render_build_metadata_block, resolve_account_credential,
    runtime_build_metadata, server_status_revision_check, token_prefix, RevisionComparison,
    CLIENT_PROFILE_ERROR,
};
// Only the Unix systemd service tests consume these re-exports.
#[cfg(unix)]
pub(super) use crate::webcodex_cli::render_agent_systemd_unit;
pub(super) use crate::*;
pub(super) use serde_json::{json, Value};
#[cfg(unix)]
pub(super) use std::ffi::OsString;
pub(super) use std::io::{Read, Write};
pub(super) use std::net::TcpListener;
pub(super) use std::path::{Path, PathBuf};
pub(super) use std::thread;

pub(super) fn direct_server_http() -> ServerHttpOptions {
    ServerHttpOptions {
        proxy: None,
        no_system_proxy: true,
    }
}

/// Keep the ephemeral port owned until the client actually connects, then close
/// the accepted socket without an HTTP response. Dropping a probe listener
/// before the request lets another concurrent test reuse the port on Windows.
pub(super) fn spawn_connection_drop_server() -> (std::net::SocketAddr, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    drop(stream);
                    return;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "connection-failure fixture was never contacted"
                    );
                    thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(error) => panic!("connection-failure fixture accept failed: {error}"),
            }
        }
    });
    (addr, handle)
}
