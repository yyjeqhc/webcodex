use super::client_handoff_service::{copy_text_to_clipboard, mcp_url, ClipboardCopyOutcome};
use super::openai_tunnel_service::{prepare_openai_tunnel, start_openai_tunnel};
use super::setup_service::{create_private_dir, write_new_private};
use super::ProductError;
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const REGULAR_TUNNEL_STARTUP_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RegularServerTunnelOptions {
    pub(crate) local_server_url: String,
    pub(crate) user_token_file: PathBuf,
}

struct RegularTunnelSession {
    directory: PathBuf,
}

impl RegularTunnelSession {
    fn create(user_token_file: &Path) -> Result<Self, ProductError> {
        let parent = user_token_file.parent().ok_or_else(|| {
            tunnel_auth_error("the WebCodex user token file has no parent directory")
        })?;
        let root = parent.join("regular-tunnel-runtime");
        create_private_dir(&root)?;
        let directory = root.join(format!("openai-{}", uuid::Uuid::new_v4().simple()));
        create_private_dir(&directory)?;
        Ok(Self { directory })
    }

    fn write_authorization_file(&self, user_token_file: &Path) -> Result<PathBuf, ProductError> {
        let token = read_user_token(user_token_file)?;
        let path = self.directory.join("openai-mcp-authorization");
        write_new_private(&path, format!("Bearer {token}").as_bytes())?;
        Ok(path)
    }
}

impl Drop for RegularTunnelSession {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

pub(crate) async fn run_regular_server_tunnel(
    options: &RegularServerTunnelOptions,
) -> Result<(), ProductError> {
    let local_server_url = validate_local_server_url(&options.local_server_url)?;
    let session = RegularTunnelSession::create(&options.user_token_file)?;
    let authorization_file = session.write_authorization_file(&options.user_token_file)?;
    let prerequisites = prepare_openai_tunnel().await?;
    let deadline = Instant::now() + REGULAR_TUNNEL_STARTUP_TIMEOUT;
    let mut tunnel = start_openai_tunnel(
        &prerequisites,
        &mcp_url(&local_server_url),
        &authorization_file,
        &session.directory,
        deadline,
    )
    .await?;

    let clipboard = copy_text_to_clipboard(&prerequisites.tunnel_id, true).await;
    let ready = machine_regular_tunnel_ready_event(clipboard);
    let encoded = serde_json::to_string(&ready).map_err(|_| {
        ProductError::new(
            "machine_output_failed",
            "WebCodex could not encode regular Tunnel readiness",
            Some("Retry the OpenAI Secure Tunnel."),
        )
    })?;
    println!("{encoded}");

    let outcome = tokio::select! {
        _ = wait_for_regular_tunnel_stop_signal() => Ok(()),
        result = tunnel.wait_for_exit() => result,
    };
    tunnel.stop().await;
    outcome
}

fn machine_regular_tunnel_ready_event(clipboard: ClipboardCopyOutcome) -> Value {
    let clipboard_state = match clipboard {
        ClipboardCopyOutcome::Copied => "copied",
        ClipboardCopyOutcome::Unavailable => "unavailable",
        ClipboardCopyOutcome::Disabled => "disabled",
    };
    json!({
        "event": "ready",
        "schema_version": 1,
        "provider": "openai",
        "ready_for_chatgpt": clipboard == ClipboardCopyOutcome::Copied,
        "connection": {
            "kind": "openai_tunnel",
            "clipboard_state": clipboard_state,
            "clipboard_contains": "tunnel_id",
        }
    })
}

fn validate_local_server_url(value: &str) -> Result<String, ProductError> {
    let value = value.trim().trim_end_matches('/');
    let parsed = url::Url::parse(value).map_err(|_| {
        ProductError::new(
            "unsupported_topology",
            "Regular OpenAI Tunnel requires a valid local WebCodex Server URL",
            Some("Start the local WebCodex runtime before starting the Tunnel."),
        )
    })?;
    let host = parsed.host_str().unwrap_or("");
    if parsed.scheme() != "http"
        || !matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]")
        || parsed.username() != ""
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !matches!(parsed.path(), "" | "/")
    {
        return Err(ProductError::new(
            "unsupported_topology",
            "Regular OpenAI Tunnel only exposes a loopback local WebCodex Server",
            Some("Use this command with Local Full Runtime; remote Server exposure is managed remotely."),
        ));
    }
    Ok(value.to_string())
}

async fn wait_for_regular_tunnel_stop_signal() {
    tokio::select! {
        _ = wait_for_platform_stop_signal() => {},
        _ = wait_for_stdin_eof() => {},
    }
}

async fn wait_for_stdin_eof() {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = std::thread::Builder::new()
        .name("webcodex-server-tunnel-stdin".to_string())
        .spawn(move || {
            let mut stdin = std::io::stdin().lock();
            let mut buffer = [0_u8; 256];
            loop {
                match stdin.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
            let _ = tx.send(());
        });
    let _ = rx.await;
}

#[cfg(not(windows))]
async fn wait_for_platform_stop_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(windows)]
async fn wait_for_platform_stop_signal() {
    let mut ctrl_break = match tokio::signal::windows::ctrl_break() {
        Ok(signal) => signal,
        Err(_) => {
            let _ = tokio::signal::ctrl_c().await;
            return;
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {},
        _ = ctrl_break.recv() => {},
    }
}

fn tunnel_auth_error(message: &str) -> ProductError {
    ProductError::new(
        "tunnel_auth_invalid",
        message,
        Some("Restore the Desktop-managed WebCodex user token file, then retry."),
    )
}

fn read_user_token(path: &Path) -> Result<String, ProductError> {
    #[cfg(windows)]
    let value = super::windows_private_state::read_private_bytes(path)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok());
    #[cfg(not(windows))]
    let value = crate::auth::read_protected_secret(path).ok();
    let value = value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            tunnel_auth_error("the WebCodex user token file is unreadable or not protected")
        })?;
    if value.starts_with("wc_agent_") {
        return Err(tunnel_auth_error(
            "the selected credential is a Runner transport token, not a WebCodex user API credential",
        ));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_ready_event_contains_only_safe_handoff_metadata() {
        let event = machine_regular_tunnel_ready_event(ClipboardCopyOutcome::Copied);
        let encoded = serde_json::to_string(&event).unwrap();
        assert!(encoded.contains("\"provider\":\"openai\""));
        assert!(encoded.contains("\"clipboard_contains\":\"tunnel_id\""));
        assert!(!encoded.contains("CONTROL_PLANE_API_KEY"));
        assert!(!encoded.contains("Authorization"));
        assert!(!encoded.contains("Bearer"));
        assert!(!encoded.contains("wc_pat_"));
    }

    #[test]
    fn regular_tunnel_rejects_non_loopback_server_origins() {
        assert!(validate_local_server_url("http://127.0.0.1:8080").is_ok());
        assert!(validate_local_server_url("http://localhost:8080").is_ok());
        assert!(validate_local_server_url("https://example.test").is_err());
        assert!(validate_local_server_url("http://0.0.0.0:8080").is_err());
    }

    #[test]
    fn authorization_file_is_private_distinct_and_cleaned_up() {
        let temp = tempfile::tempdir().unwrap();
        let token_file = temp.path().join("webcodex-user-token");
        super::super::setup_service::write_new_private(&token_file, b"wc_pat_test_secret\n")
            .unwrap();
        let session = RegularTunnelSession::create(&token_file).unwrap();
        let session_dir = session.directory.clone();
        let authorization_file = session.write_authorization_file(&token_file).unwrap();
        assert_eq!(
            std::fs::read_to_string(&authorization_file).unwrap(),
            "Bearer wc_pat_test_secret"
        );
        assert_ne!(authorization_file, token_file);
        drop(session);
        assert!(!session_dir.exists());
        assert!(token_file.is_file());
    }

    #[test]
    fn authorization_file_rejects_runner_transport_token_without_echoing_it() {
        let temp = tempfile::tempdir().unwrap();
        let token_file = temp.path().join("webcodex-user-token");
        let secret = "wc_agent_do_not_echo_regular_tunnel_0123456789";
        super::super::setup_service::write_new_private(
            &token_file,
            format!("{secret}\n").as_bytes(),
        )
        .unwrap();
        let session = RegularTunnelSession::create(&token_file).unwrap();
        let error = session.write_authorization_file(&token_file).unwrap_err();
        assert_eq!(error.code, "tunnel_auth_invalid");
        assert!(error.message.contains("Runner transport token"));
        assert!(!error.message.contains(secret));
    }
}
