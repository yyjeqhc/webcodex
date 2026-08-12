use super::setup_service::{
    create_private_dir, generate_project_credential, read_project_credential, write_new_private,
};
use super::{
    configured_project, ensure_local_runtime_port_available, executable_name, parse_options, setup,
    start_local_runtime, LocalRuntimeOptions, ProductError, ProjectCommandOptions,
};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

const TUNNEL_START_TIMEOUT: Duration = Duration::from_secs(20);
const TUNNEL_LOG_LINES: usize = 8;
const TUNNEL_LOG_LINE_BYTES: usize = 512;
const TUNNEL_LOG_DRAIN_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TunnelProvider {
    CloudflareQuick,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShareCommandOptions {
    pub(crate) project: ProjectCommandOptions,
    pub(crate) tunnel: TunnelProvider,
}

pub(crate) fn parse_share_options(args: &[String]) -> Result<ShareCommandOptions, String> {
    let mut tunnel = TunnelProvider::CloudflareQuick;
    let mut project_args = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--tunnel" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--tunnel requires a value".to_string())?;
                tunnel = match value.as_str() {
                    "cloudflare" => TunnelProvider::CloudflareQuick,
                    "none" => TunnelProvider::None,
                    _ => {
                        return Err(format!(
                            "unknown tunnel provider '{value}'; expected cloudflare or none"
                        ))
                    }
                };
            }
            flag => project_args.push(flag.to_string()),
        }
        index += 1;
    }
    let project = parse_options(&project_args, "share")?;
    Ok(ShareCommandOptions { project, tunnel })
}

struct ShareSession {
    directory: PathBuf,
    credential_file: PathBuf,
    credential: String,
}

impl ShareSession {
    fn create(state: &Path) -> Result<Self, ProductError> {
        let share_root = state.join("share");
        create_private_dir(&share_root)?;
        let directory = share_root.join(uuid::Uuid::new_v4().simple().to_string());
        let credential_file = directory.join("connector-key");
        let result = (|| {
            create_private_dir(&directory)?;
            let credential = generate_project_credential();
            write_new_private(&credential_file, format!("{credential}\n").as_bytes())?;
            let credential = read_project_credential(&credential_file)?;
            Ok(Self {
                directory: directory.clone(),
                credential_file,
                credential,
            })
        })();
        if result.is_err() {
            let _ = std::fs::remove_dir_all(&directory);
        }
        result
    }
}

impl Drop for ShareSession {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

#[derive(Debug)]
struct CloudflareTunnel {
    child: Child,
    stdout_task: JoinHandle<()>,
    stderr_task: JoinHandle<()>,
}

impl CloudflareTunnel {
    async fn wait_for_exit(&mut self) -> Result<(), ProductError> {
        let status = self
            .child
            .wait()
            .await
            .map_err(|_| tunnel_runtime_error())?;
        Err(ProductError::new(
            "tunnel_unavailable",
            format!("Cloudflare Quick Tunnel stopped unexpectedly ({status})"),
            Some("Check network connectivity and cloudflared, then retry webcodex share."),
        ))
    }

    async fn stop(&mut self) {
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
        self.stdout_task.abort();
        self.stderr_task.abort();
    }
}

impl Drop for CloudflareTunnel {
    fn drop(&mut self) {
        self.stdout_task.abort();
        self.stderr_task.abort();
    }
}

fn tunnel_runtime_error() -> ProductError {
    ProductError::new(
        "tunnel_unavailable",
        "Cloudflare Quick Tunnel process could not be supervised",
        Some("Check cloudflared installation and retry webcodex share."),
    )
}

pub(crate) async fn share(options: &ShareCommandOptions) -> Result<(), ProductError> {
    setup(&options.project)?;
    let (config, paths) = configured_project(&options.project)?;
    ensure_local_runtime_port_available(
        config.port,
        "Stop the conflicting process, then retry webcodex share.",
    )?;
    let persistent_credential = read_project_credential(&paths.connector_key)?;
    let session = ShareSession::create(&paths.state)?;
    if session.credential == persistent_credential {
        return Err(ProductError::new(
            "project_credential_invalid",
            "temporary share credential unexpectedly matched the persistent project credential",
            Some("Retry webcodex share."),
        ));
    }

    let local_url = config.server_url();
    let (public_url, mut tunnel) = match options.tunnel {
        TunnelProvider::CloudflareQuick => {
            let binary = locate_cloudflared()?;
            let (url, tunnel) =
                start_cloudflare_quick_with_binary(&binary, &local_url, TUNNEL_START_TIMEOUT)
                    .await?;
            (url, Some(tunnel))
        }
        TunnelProvider::None => (local_url.clone(), None),
    };

    let mut runtime = start_local_runtime(
        &options.project,
        LocalRuntimeOptions {
            public_url: Some(public_url.clone()),
            connector_credential_file: Some(session.credential_file.clone()),
            port_conflict_action: "Stop the conflicting process, then retry webcodex share.",
        },
    )
    .await?;

    println!(
        "{}",
        render_share_ready(
            &runtime.project_name,
            options.tunnel,
            &runtime.public_url,
            &session.credential,
        )
    );

    let outcome = if let Some(tunnel) = tunnel.as_mut() {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => Ok(()),
            result = runtime.wait_for_exit() => result,
            result = tunnel.wait_for_exit() => result,
        }
    } else {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => Ok(()),
            result = runtime.wait_for_exit() => result,
        }
    };

    runtime.stop().await;
    if let Some(tunnel) = tunnel.as_mut() {
        tunnel.stop().await;
    }
    outcome
}

fn render_share_ready(
    project_name: &str,
    tunnel: TunnelProvider,
    public_url: &str,
    credential: &str,
) -> String {
    let tunnel_name = match tunnel {
        TunnelProvider::CloudflareQuick => "Cloudflare Quick Tunnel",
        TunnelProvider::None => "none (local only)",
    };
    let public_access = match tunnel {
        TunnelProvider::CloudflareQuick => "temporary",
        TunnelProvider::None => "local only",
    };
    let ready_message = match tunnel {
        TunnelProvider::CloudflareQuick => "Ready for ChatGPT or another remote MCP client.",
        TunnelProvider::None => "Ready for a local MCP client. No public tunnel is running.",
    };
    let lifetime_message = match tunnel {
        TunnelProvider::CloudflareQuick => "This credential and tunneled URL are temporary.",
        TunnelProvider::None => "This credential is temporary.",
    };
    format!(
        "Project: {project_name}\nRuntime: local\nTunnel: {tunnel_name}\nPublic access: {public_access}\n\nMCP URL:\n  {}/mcp\n\nAuthentication:\n  Bearer token\n\nToken:\n  {credential}\n\n{ready_message}\n\n{lifetime_message}\nPress Ctrl-C to stop sharing.",
        public_url.trim_end_matches('/')
    )
}

fn locate_cloudflared() -> Result<PathBuf, ProductError> {
    let override_bin = std::env::var_os("WEBCODEX_CLOUDFLARED_BIN").map(PathBuf::from);
    require_cloudflared_from(override_bin.as_deref(), std::env::var_os("PATH").as_deref())
}

fn require_cloudflared_from(
    override_bin: Option<&Path>,
    path: Option<&std::ffi::OsStr>,
) -> Result<PathBuf, ProductError> {
    locate_cloudflared_from(override_bin, path).ok_or_else(|| {
        ProductError::new(
            "tunnel_unavailable",
            "cloudflared was not found",
            Some(
                "Install cloudflared from Cloudflare's official downloads (https://developers.cloudflare.com/tunnel/downloads/), ensure it is on PATH, then retry; or use webcodex share --tunnel none for local-only debugging.",
            ),
        )
    })
}

fn locate_cloudflared_from(
    override_bin: Option<&Path>,
    path: Option<&std::ffi::OsStr>,
) -> Option<PathBuf> {
    if let Some(binary) = override_bin {
        return binary.is_file().then(|| binary.to_path_buf());
    }
    let path = path?;
    std::env::split_paths(path)
        .map(|directory| directory.join(executable_name("cloudflared")))
        .find(|candidate| candidate.is_file())
}

async fn start_cloudflare_quick_with_binary(
    binary: &Path,
    local_url: &str,
    timeout: Duration,
) -> Result<(String, CloudflareTunnel), ProductError> {
    let mut child = Command::new(binary)
        .arg("tunnel")
        .arg("--url")
        .arg(local_url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| {
            ProductError::new(
                "tunnel_unavailable",
                "cloudflared could not start",
                Some("Check the cloudflared executable and retry webcodex share."),
            )
        })?;
    let stdout = child.stdout.take().ok_or_else(tunnel_runtime_error)?;
    let stderr = child.stderr.take().ok_or_else(tunnel_runtime_error)?;
    let recent = Arc::new(Mutex::new(VecDeque::with_capacity(TUNNEL_LOG_LINES)));
    let (url_tx, mut url_rx) = mpsc::channel(2);
    let stdout_task = spawn_tunnel_reader(stdout, recent.clone(), url_tx.clone());
    let stderr_task = spawn_tunnel_reader(stderr, recent.clone(), url_tx);
    let deadline = Instant::now() + timeout;

    loop {
        if let Some(status) = child.try_wait().map_err(|_| tunnel_runtime_error())? {
            drain_tunnel_readers(stdout_task, stderr_task).await;
            let detail = bounded_tunnel_log_summary(&recent);
            let message = if detail.is_empty() {
                format!("cloudflared exited before creating a Quick Tunnel ({status})")
            } else {
                format!("cloudflared exited before creating a Quick Tunnel ({status}): {detail}")
            };
            return Err(ProductError::new(
                "tunnel_unavailable",
                message,
                Some("Check cloudflared output and network connectivity, then retry."),
            ));
        }
        let now = Instant::now();
        if now >= deadline {
            let _ = child.start_kill();
            let _ = child.wait().await;
            stdout_task.abort();
            stderr_task.abort();
            return Err(ProductError::new(
                "tunnel_unavailable",
                "Cloudflare Quick Tunnel did not provide a public URL before the startup timeout",
                Some("Check network connectivity and cloudflared, then retry."),
            ));
        }
        let wait = (deadline - now).min(Duration::from_millis(100));
        if let Ok(Some(url)) = tokio::time::timeout(wait, url_rx.recv()).await {
            return Ok((
                url,
                CloudflareTunnel {
                    child,
                    stdout_task,
                    stderr_task,
                },
            ));
        }
    }
}

fn spawn_tunnel_reader<R>(
    reader: R,
    recent: Arc<Mutex<VecDeque<String>>>,
    url_tx: mpsc::Sender<String>,
) -> JoinHandle<()>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            record_tunnel_line(&recent, &line);
            if let Some(url) = parse_quick_tunnel_url(&line) {
                let _ = url_tx.try_send(url);
            }
        }
    })
}

// A child can exit before the async readers are scheduled to consume bytes that
// are already buffered in its pipes. Give them a bounded chance to reach EOF so
// startup diagnostics are retained, but do not wait forever if a descendant
// inherited one of the pipe handles.
async fn drain_tunnel_readers(mut stdout_task: JoinHandle<()>, mut stderr_task: JoinHandle<()>) {
    let drained = tokio::time::timeout(TUNNEL_LOG_DRAIN_TIMEOUT, async {
        let _ = tokio::join!(&mut stdout_task, &mut stderr_task);
    })
    .await
    .is_ok();
    if !drained {
        stdout_task.abort();
        stderr_task.abort();
    }
}

fn record_tunnel_line(recent: &Arc<Mutex<VecDeque<String>>>, line: &str) {
    let mut line = line.to_string();
    if line.len() > TUNNEL_LOG_LINE_BYTES {
        let mut end = TUNNEL_LOG_LINE_BYTES;
        while !line.is_char_boundary(end) {
            end -= 1;
        }
        line.truncate(end);
    }
    if let Ok(mut lines) = recent.lock() {
        if lines.len() == TUNNEL_LOG_LINES {
            lines.pop_front();
        }
        lines.push_back(line);
    }
}

fn bounded_tunnel_log_summary(recent: &Arc<Mutex<VecDeque<String>>>) -> String {
    recent
        .lock()
        .map(|lines| lines.iter().cloned().collect::<Vec<_>>().join(" | "))
        .unwrap_or_default()
}

fn parse_quick_tunnel_url(line: &str) -> Option<String> {
    let start = line.find("https://")?;
    let candidate = line[start..]
        .split_whitespace()
        .next()?
        .trim_end_matches(|character: char| {
            matches!(character, ',' | ';' | ')' | ']' | '}' | '"' | '\'')
        });
    let parsed = url::Url::parse(candidate).ok()?;
    let host = parsed.host_str()?;
    (parsed.scheme() == "https"
        && host.ends_with(".trycloudflare.com")
        && parsed.path() == "/"
        && parsed.query().is_none()
        && parsed.fragment().is_none())
    .then(|| format!("https://{host}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parses_cloudflare_quick_tunnel_url_from_bounded_log_line() {
        assert_eq!(
            parse_quick_tunnel_url(
                "INF +--------------------------------------------------------------------------------------------+ https://bright-example.trycloudflare.com"
            ),
            Some("https://bright-example.trycloudflare.com".to_string())
        );
        assert_eq!(parse_quick_tunnel_url("https://example.com"), None);
        assert_eq!(
            parse_quick_tunnel_url("https://bad.trycloudflare.com/mcp"),
            None
        );
    }

    #[test]
    fn missing_cloudflared_reports_actionable_error() {
        let temp = tempfile::tempdir().unwrap();
        let error = require_cloudflared_from(Some(&temp.path().join("missing")), None).unwrap_err();
        assert_eq!(error.code, "tunnel_unavailable");
        assert!(error.message.contains("cloudflared was not found"));
        let next_action = error.next_action.unwrap();
        assert!(next_action.contains("https://developers.cloudflare.com/tunnel/downloads/"));
        assert!(next_action.contains("--tunnel none"));
    }

    #[test]
    fn share_cli_defaults_to_cloudflare_and_accepts_none() {
        let default = parse_share_options(&[]).unwrap();
        assert_eq!(default.tunnel, TunnelProvider::CloudflareQuick);
        let explicit =
            parse_share_options(&["--tunnel".to_string(), "cloudflare".to_string()]).unwrap();
        assert_eq!(explicit.tunnel, TunnelProvider::CloudflareQuick);
        let local = parse_share_options(&["--tunnel".to_string(), "none".to_string()]).unwrap();
        assert_eq!(local.tunnel, TunnelProvider::None);
        assert!(parse_share_options(&["--tunnel".to_string(), "unknown".to_string()]).is_err());
    }

    #[test]
    fn share_output_contains_only_the_temporary_connector_credential() {
        let persistent = "webcodex_persistent-never-print";
        let temporary = "webcodex_temporary-print-once";
        let output = render_share_ready(
            "demo",
            TunnelProvider::CloudflareQuick,
            "https://demo.trycloudflare.com",
            temporary,
        );
        assert!(output.contains(temporary));
        assert!(!output.contains(persistent));
        assert!(output.contains("https://demo.trycloudflare.com/mcp"));
    }

    #[test]
    fn local_only_share_output_does_not_claim_remote_chatgpt_readiness() {
        let output = render_share_ready(
            "demo",
            TunnelProvider::None,
            "http://127.0.0.1:23456",
            "webcodex_temporary-print-once",
        );
        assert!(output.contains("Ready for a local MCP client"));
        assert!(output.contains("No public tunnel is running"));
        assert!(!output.contains("Ready for ChatGPT"));
        assert!(!output.contains("tunneled URL"));
    }

    #[test]
    fn tunnel_log_truncation_preserves_utf8_boundaries() {
        let recent = Arc::new(Mutex::new(VecDeque::new()));
        let line = format!("{}界tail", "a".repeat(TUNNEL_LOG_LINE_BYTES - 1));
        record_tunnel_line(&recent, &line);
        let recorded = recent.lock().unwrap();
        let line = recorded.back().unwrap();
        assert!(line.len() <= TUNNEL_LOG_LINE_BYTES);
        assert_eq!(line, &"a".repeat(TUNNEL_LOG_LINE_BYTES - 1));
    }

    #[test]
    fn local_port_preflight_rejects_an_occupied_port() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let error = ensure_local_runtime_port_available(port, "stop conflict").unwrap_err();
        assert_eq!(error.code, "server_unreachable");
        assert!(error.message.contains("already in use"));
    }

    #[test]
    fn temporary_share_credential_is_private_distinct_and_cleaned_up() {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join("state");
        create_private_dir(&state).unwrap();
        let persistent = state.join("credentials/connector-key");
        let persistent_value = generate_project_credential();
        write_new_private(&persistent, format!("{persistent_value}\n").as_bytes()).unwrap();
        let persistent_before = fs::read_to_string(&persistent).unwrap();
        let session = ShareSession::create(&state).unwrap();
        assert_ne!(session.credential, persistent_value);
        assert_eq!(fs::read_to_string(&persistent).unwrap(), persistent_before);
        assert_eq!(
            crate::auth::read_protected_secret(&session.credential_file).unwrap(),
            session.credential
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(state.join("share"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(&session.directory)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(&session.credential_file)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        let directory = session.directory.clone();
        drop(session);
        assert!(!directory.exists());
        assert!(persistent.is_file());
    }

    #[cfg(unix)]
    fn fake_cloudflared(script: &str) -> (tempfile::TempDir, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("cloudflared");
        fs::write(&path, script).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        (temp, path)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn tunnel_startup_failure_is_reported() {
        let (_temp, binary) = fake_cloudflared("#!/bin/sh\necho startup-failed >&2\nexit 7\n");
        let error = start_cloudflare_quick_with_binary(
            &binary,
            "http://127.0.0.1:23456",
            Duration::from_secs(1),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "tunnel_unavailable");
        assert!(error.message.contains("startup-failed"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn tunnel_startup_timeout_kills_fake_process() {
        let (_temp, binary) = fake_cloudflared("#!/bin/sh\nsleep 5\n");
        let error = start_cloudflare_quick_with_binary(
            &binary,
            "http://127.0.0.1:23456",
            Duration::from_millis(100),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "tunnel_unavailable");
        assert!(error.message.contains("startup timeout"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn tunnel_stop_reaps_fake_process() {
        let (_temp, binary) = fake_cloudflared(
            "#!/bin/sh\necho https://cleanup-test.trycloudflare.com >&2\nsleep 5\n",
        );
        let (_url, mut tunnel) = start_cloudflare_quick_with_binary(
            &binary,
            "http://127.0.0.1:23456",
            Duration::from_secs(1),
        )
        .await
        .unwrap();
        tunnel.stop().await;
        assert!(tunnel.child.try_wait().unwrap().is_some());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn tunnel_early_exit_after_url_is_supervised() {
        let (_temp, binary) = fake_cloudflared(
            "#!/bin/sh\necho https://short-lived.trycloudflare.com >&2\nsleep 0.1\nexit 9\n",
        );
        let (url, mut tunnel) = start_cloudflare_quick_with_binary(
            &binary,
            "http://127.0.0.1:23456",
            Duration::from_secs(1),
        )
        .await
        .unwrap();
        assert_eq!(url, "https://short-lived.trycloudflare.com");
        let error = tunnel.wait_for_exit().await.unwrap_err();
        assert_eq!(error.code, "tunnel_unavailable");
        assert!(error.message.contains("stopped unexpectedly"));
    }
}
