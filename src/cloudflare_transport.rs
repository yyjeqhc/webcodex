//! Cloudflared process transport, independent of temporary-share credentials.
//!
//! Connector output is untrusted and may contain tokens. Retain only bounded
//! parsed readiness facts; never retain or return its raw stdout or stderr.

use super::setup_service::{create_private_dir, read_private_value, write_new_private};
use super::ProductError;
use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Notify;
use webcodex_process::ManagedChild;

const MAX_LINE_BYTES: usize = 4096;
const MAX_TOKEN_BYTES: u64 = 64 * 1024;
const PROCESS_POLL: Duration = Duration::from_millis(50);
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(2);
const VERSION_TIMEOUT: Duration = Duration::from_secs(10);
// --token-file was introduced in 2025.4.0. Use the same supported baseline
// for both modes so selecting Quick never silently changes binary policy.
const MIN_VERSION: (u32, u32, u32) = (2025, 4, 0);

#[derive(Default)]
struct Signals {
    quick_url: Option<String>,
    connected: bool,
    readers_finished: usize,
    version: Option<(u32, u32, u32)>,
}

struct OutputState {
    facts: Mutex<Signals>,
    changed: Notify,
}

impl Default for OutputState {
    fn default() -> Self {
        Self {
            facts: Mutex::new(Signals::default()),
            changed: Notify::new(),
        }
    }
}

/// The handle always owns the complete process tree, including during startup
/// and cancellation. Its private directory outlives child shutdown.
pub(crate) struct CloudflareTransport {
    child: ManagedChild,
    output: Arc<OutputState>,
    _directory: tempfile::TempDir,
}

pub(crate) async fn prepare_cloudflared() -> Result<PathBuf, ProductError> {
    super::cloudflared_service::resolve_cloudflared().await
}

pub(crate) fn valid_quick_origin(origin: &str) -> bool {
    super::share_service::parse_quick_tunnel_url(origin).as_deref() == Some(origin)
}

impl CloudflareTransport {
    /// A reported URL identifies the candidate endpoint; the caller must prove
    /// forwarding to its current Server instance before publishing readiness.
    pub(crate) async fn start_quick(
        binary: &Path,
        ingress: &str,
        deadline: Instant,
    ) -> Result<(String, Self), ProductError> {
        validate_ingress(ingress)?;
        verify_supported_binary(binary, deadline).await?;
        check_start_deadline(deadline)?;
        let mut transport = Self::spawn(binary, Some(ingress), None)?;
        transport.wait_ready(false, deadline).await?;
        let url = transport
            .output
            .facts
            .lock()
            .ok()
            .and_then(|facts| facts.quick_url.clone())
            .ok_or_else(|| transport_error("Cloudflare did not provide a Quick Tunnel URL"))?;
        Ok((url, transport))
    }

    /// Named routes remain remotely configured. `origin` identifies the public
    /// endpoint for the caller's forwarding probe; it never overrides ingress.
    /// Registration only proves connector startup, not Server forwarding.
    pub(crate) async fn start_named(
        binary: &Path,
        origin: &str,
        token_file: &Path,
        deadline: Instant,
    ) -> Result<Self, ProductError> {
        validate_origin(origin)?;
        verify_supported_binary(binary, deadline).await?;
        check_start_deadline(deadline)?;
        let mut transport = Self::spawn(binary, None, Some(token_file))?;
        transport.wait_ready(true, deadline).await?;
        Ok(transport)
    }

    fn spawn(
        binary: &Path,
        ingress: Option<&str>,
        token_file: Option<&Path>,
    ) -> Result<Self, ProductError> {
        let binary = std::fs::canonicalize(binary)
            .map_err(|_| transport_error("Cloudflare executable is unavailable"))?;
        let directory = private_directory()?;
        let config = directory.path().join("empty-config.yml");
        write_new_private(&config, b"{}\n")
            .map_err(|_| transport_error("Could not create isolated Cloudflare configuration"))?;
        let mut command = isolated_command(&binary, directory.path());
        command
            .arg("tunnel")
            .arg("--config")
            .arg(&config)
            .arg("--no-autoupdate");
        if let Some(ingress) = ingress {
            command.arg("--url").arg(ingress);
        } else {
            let token_file = token_file
                .ok_or_else(|| transport_error("Cloudflare tunnel token file is missing"))?;
            let metadata = std::fs::symlink_metadata(token_file).map_err(|_| token_error())?;
            if !metadata.file_type().is_file() || metadata.len() > MAX_TOKEN_BYTES {
                return Err(token_error());
            }
            let token = read_private_value(token_file).map_err(|_| token_error())?;
            if token.len() as u64 > MAX_TOKEN_BYTES || token.contains(['\r', '\n']) {
                return Err(token_error());
            }
            let protected_copy = directory.path().join("tunnel-token");
            write_new_private(&protected_copy, token.as_bytes()).map_err(|_| token_error())?;
            command.arg("run").arg("--token-file").arg(protected_copy);
        }
        let (child, output) = spawn_drained(&mut command)?;
        Ok(Self {
            child,
            output,
            _directory: directory,
        })
    }

    async fn wait_ready(&mut self, named: bool, deadline: Instant) -> Result<(), ProductError> {
        loop {
            if Instant::now() >= deadline {
                self.stop().await?;
                return Err(transport_error("Cloudflare connector startup timed out"));
            }
            if !self.is_alive()? {
                self.stop().await?;
                return Err(transport_error(
                    "Cloudflare exited before connector readiness",
                ));
            }
            let ready = self.output.facts.lock().is_ok_and(|facts| {
                if named {
                    facts.connected
                } else {
                    facts.quick_url.is_some()
                }
            });
            if ready {
                return Ok(());
            }
            tokio::select! {
                _ = self.output.changed.notified() => {},
                _ = tokio::time::sleep(PROCESS_POLL.min(deadline.saturating_duration_since(Instant::now()))) => {},
            }
        }
    }

    pub(crate) fn is_alive(&mut self) -> Result<bool, ProductError> {
        self.child
            .try_wait()
            .map(|status| status.is_none())
            .map_err(|_| transport_error("Could not inspect the Cloudflare connector"))
    }

    pub(crate) async fn wait_for_exit(&mut self) -> Result<(), ProductError> {
        while self.is_alive()? {
            tokio::time::sleep(PROCESS_POLL).await;
        }
        self.stop().await?;
        Err(transport_error("Cloudflare connector exited"))
    }

    pub(crate) async fn stop(&mut self) -> Result<(), ProductError> {
        cleanup_child(&mut self.child, &self.output).await
    }
}

fn check_start_deadline(deadline: Instant) -> Result<(), ProductError> {
    if Instant::now() >= deadline {
        return Err(transport_error("Cloudflare connector startup timed out"));
    }
    Ok(())
}

fn private_directory() -> Result<tempfile::TempDir, ProductError> {
    let directory = tempfile::Builder::new()
        .prefix("webcodex-cloudflared-")
        .tempdir()
        .map_err(|_| transport_error("Could not create private Cloudflare runtime state"))?;
    create_private_dir(directory.path())
        .map_err(|_| transport_error("Could not protect private Cloudflare runtime state"))?;
    Ok(directory)
}

fn allowed_environment(name: &OsStr) -> bool {
    // Positive selection prevents new credential variables from leaking as
    // providers add them. HOME and all Cloudflare config inputs are replaced.
    matches!(
        name.to_str().map(str::to_ascii_uppercase).as_deref(),
        Some("PATH" | "SYSTEMROOT" | "WINDIR" | "TEMP" | "TMP" | "TMPDIR")
    )
}

fn isolated_command(binary: &Path, directory: &Path) -> Command {
    isolated_command_with(binary, directory, std::env::vars_os())
}

fn isolated_command_with(
    binary: &Path,
    directory: &Path,
    environment: impl IntoIterator<Item = (OsString, OsString)>,
) -> Command {
    let mut command = Command::new(binary);
    command.env_clear();
    command.envs(
        environment
            .into_iter()
            .filter(|(name, _)| allowed_environment(name)),
    );
    command
        .env("HOME", directory)
        .env("USERPROFILE", directory)
        .env("APPDATA", directory)
        .env("XDG_CONFIG_HOME", directory)
        .current_dir(directory)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn spawn_drained(command: &mut Command) -> Result<(ManagedChild, Arc<OutputState>), ProductError> {
    let mut child = ManagedChild::spawn(command)
        .map_err(|_| transport_error("Could not start the Cloudflare executable"))?;
    let stdout = child
        .child_mut()
        .stdout
        .take()
        .ok_or_else(|| transport_error("Could not capture Cloudflare output"))?;
    let stderr = child
        .child_mut()
        .stderr
        .take()
        .ok_or_else(|| transport_error("Could not capture Cloudflare output"))?;
    let output = Arc::new(OutputState::default());
    spawn_reader(stdout, output.clone())?;
    spawn_reader(stderr, output.clone())?;
    Ok((child, output))
}

fn spawn_reader<R: Read + Send + 'static>(
    reader: R,
    output: Arc<OutputState>,
) -> Result<(), ProductError> {
    std::thread::Builder::new()
        .name("webcodex-cloudflared-output".to_string())
        .spawn(move || {
            read_bounded_lines(reader, |line| {
                if let Ok(mut facts) = output.facts.lock() {
                    if facts.quick_url.is_none() {
                        facts.quick_url = super::share_service::parse_quick_tunnel_url(line);
                    }
                    facts.connected |= line.contains("Registered tunnel connection");
                    if facts.version.is_none() {
                        facts.version = parse_version(line);
                    }
                }
                output.changed.notify_one();
            });
            if let Ok(mut facts) = output.facts.lock() {
                facts.readers_finished += 1;
            }
            output.changed.notify_one();
        })
        .map(|_| ())
        .map_err(|_| transport_error("Could not drain Cloudflare output"))
}

/// Drain every byte, but discard oversized lines completely. In particular,
/// never accept a valid-looking URL hidden in an oversized log suffix.
fn read_bounded_lines(mut reader: impl Read, mut consume: impl FnMut(&str)) {
    let mut buffer = [0_u8; 4096];
    let mut line = Vec::with_capacity(MAX_LINE_BYTES);
    let mut oversized = false;
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                for byte in &buffer[..count] {
                    if *byte == b'\n' {
                        if !oversized {
                            if let Ok(text) = std::str::from_utf8(&line) {
                                consume(text);
                            }
                        }
                        line.clear();
                        oversized = false;
                    } else if !oversized {
                        if line.len() == MAX_LINE_BYTES {
                            line.clear();
                            oversized = true;
                        } else {
                            line.push(*byte);
                        }
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    if !oversized {
        if let Ok(text) = std::str::from_utf8(&line) {
            consume(text);
        }
    }
}

async fn cleanup_child(child: &mut ManagedChild, output: &OutputState) -> Result<(), ProductError> {
    child
        .terminate_tree()
        .map_err(|_| transport_error("Could not terminate the Cloudflare process tree"))?;
    let deadline = Instant::now() + CLEANUP_TIMEOUT;
    loop {
        let reaped = child
            .try_wait()
            .map_err(|_| transport_error("Could not reap the Cloudflare connector"))?
            .is_some();
        let tree_exited = child
            .try_tree_exit()
            .map_err(|_| transport_error("Could not inspect the Cloudflare process tree"))?;
        let drained = output
            .facts
            .lock()
            .is_ok_and(|facts| facts.readers_finished == 2);
        if reaped && tree_exited && drained {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(transport_error("Cloudflare process tree cleanup timed out"));
        }
        tokio::time::sleep(PROCESS_POLL).await;
    }
}

pub(super) async fn cloudflared_binary_version(
    binary: &Path,
) -> Result<(u32, u32, u32), ProductError> {
    cloudflared_binary_version_until(binary, Instant::now() + VERSION_TIMEOUT).await
}

async fn cloudflared_binary_version_until(
    binary: &Path,
    deadline: Instant,
) -> Result<(u32, u32, u32), ProductError> {
    if Instant::now() >= deadline {
        return Err(transport_error("Cloudflare version verification timed out"));
    }
    let binary = std::fs::canonicalize(binary)
        .map_err(|_| transport_error("Cloudflare executable is unavailable"))?;
    let directory = private_directory()?;
    let mut command = isolated_command(&binary, directory.path());
    command.arg("--version");
    let (mut child, output) = spawn_drained(&mut command)?;
    let success = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|_| transport_error("Cloudflare version verification failed"))?
        {
            break status.success();
        }
        if Instant::now() >= deadline {
            cleanup_child(&mut child, &output).await?;
            return Err(transport_error("Cloudflare version verification timed out"));
        }
        tokio::time::sleep(PROCESS_POLL.min(deadline.saturating_duration_since(Instant::now())))
            .await;
    };
    cleanup_child(&mut child, &output).await?;
    if !success {
        return Err(transport_error("Cloudflare version verification failed"));
    }
    output
        .facts
        .lock()
        .ok()
        .and_then(|facts| facts.version)
        .ok_or_else(|| transport_error("Cloudflare executable did not report a supported version"))
}

async fn verify_supported_binary(binary: &Path, deadline: Instant) -> Result<(), ProductError> {
    let deadline = deadline.min(Instant::now() + VERSION_TIMEOUT);
    if cloudflared_binary_version_until(binary, deadline).await? < MIN_VERSION {
        return Err(transport_error(
            "Cloudflare requires cloudflared 2025.4.0 or later",
        ));
    }
    Ok(())
}

fn parse_version(line: &str) -> Option<(u32, u32, u32)> {
    let version = line
        .trim()
        .strip_prefix("cloudflared version ")?
        .split_whitespace()
        .next()?;
    let mut parts = version.split('.');
    let result = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(result)
}

fn validate_origin(origin: &str) -> Result<(), ProductError> {
    let parsed = url::Url::parse(origin)
        .map_err(|_| transport_error("Cloudflare public origin is invalid"))?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || !matches!(parsed.path(), "" | "/")
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(transport_error(
            "Cloudflare public origin must be an HTTPS origin",
        ));
    }
    Ok(())
}

fn validate_ingress(ingress: &str) -> Result<(), ProductError> {
    let parsed = url::Url::parse(ingress)
        .map_err(|_| transport_error("Cloudflare local ingress is invalid"))?;
    if parsed.scheme() != "http"
        || !matches!(
            parsed.host_str(),
            Some("127.0.0.1" | "localhost" | "[::1]" | "::1")
        )
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || !matches!(parsed.path(), "" | "/")
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(transport_error(
            "Cloudflare ingress must be a loopback HTTP origin",
        ));
    }
    Ok(())
}

fn token_error() -> ProductError {
    transport_error(
        "Cloudflare tunnel token file is missing or is not protected authentication material",
    )
}

fn transport_error(message: &str) -> ProductError {
    ProductError::new("tunnel_unavailable", message, Some("Check the protected Cloudflare token, trusted cloudflared binary, and tunnel configuration, then retry."))
}

#[cfg(test)]
#[path = "cloudflare_transport/tests.rs"]
mod tests;
