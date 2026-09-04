use crate::error::{DesktopError, DesktopResult};
use crate::models::BinaryInfo;
use crate::platform;
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

const CLI_OUTPUT_BYTES: usize = 256 * 1024;
const CLI_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedBinarySource {
    Environment,
    SourceDogfoodTarget,
}

impl ResolvedBinarySource {
    fn label(self) -> &'static str {
        match self {
            Self::Environment => "WEBCODEX_DESKTOP_BIN_DIR",
            Self::SourceDogfoodTarget => "source target/dogfood",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedBinaries {
    pub directory: PathBuf,
    pub webcodex: PathBuf,
    pub server: PathBuf,
    pub runner: PathBuf,
    pub version: String,
    pub git_commit: String,
    pub source: ResolvedBinarySource,
}

impl ResolvedBinaries {
    pub async fn resolve() -> DesktopResult<Self> {
        let (directory, source) = if let Some(value) = std::env::var_os("WEBCODEX_DESKTOP_BIN_DIR")
        {
            let directory = PathBuf::from(value);
            if directory.as_os_str().is_empty() {
                return Err(DesktopError::new(
                    "binary_directory_invalid",
                    "WEBCODEX_DESKTOP_BIN_DIR is empty",
                    "Set it to the directory containing the source-matched WebCodex binaries.",
                ));
            }
            (directory, ResolvedBinarySource::Environment)
        } else {
            let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let repo = manifest
                .parent()
                .and_then(Path::parent)
                .and_then(Path::parent)
                .ok_or_else(|| {
                    DesktopError::new(
                        "binary_directory_invalid",
                        "Could not derive the WebCodex source root",
                        "Set WEBCODEX_DESKTOP_BIN_DIR explicitly.",
                    )
                })?;
            (
                repo.join("target").join("dogfood"),
                ResolvedBinarySource::SourceDogfoodTarget,
            )
        };
        let directory = directory.canonicalize().map_err(|_| {
            DesktopError::new(
                "binary_directory_missing",
                format!("WebCodex binary directory does not exist: {}", directory.display()),
                "Build `cargo build --profile dogfood --bins` from this source baseline or set WEBCODEX_DESKTOP_BIN_DIR.",
            )
        })?;
        let webcodex = directory.join(executable_name("webcodex"));
        let server = directory.join(executable_name("webcodex-server"));
        let runner = directory.join(executable_name("webcodex-runner"));
        for path in [&webcodex, &server, &runner] {
            if !path.is_file() {
                return Err(DesktopError::new(
                    "binary_missing",
                    format!("Required WebCodex binary is missing: {}", path.display()),
                    "Build all WebCodex dogfood binaries from the current source baseline.",
                ));
            }
        }

        let cli_version = binary_version(&webcodex).await?;
        let server_version = binary_version(&server).await?;
        let runner_version = binary_version(&runner).await?;
        if cli_version.version != server_version.version
            || cli_version.version != runner_version.version
            || cli_version.git_commit != server_version.git_commit
            || cli_version.git_commit != runner_version.git_commit
        {
            return Err(DesktopError::new(
                "binary_version_mismatch",
                "CLI, Server, and Runner were built from different baselines",
                "Rebuild all dogfood binaries from one WebCodex checkout.",
            ));
        }
        if cli_version.git_commit == "unknown" {
            return Err(DesktopError::new(
                "binary_version_unverifiable",
                "WebCodex binaries do not carry a source revision",
                "Rebuild dogfood binaries with normal WebCodex build metadata enabled.",
            ));
        }
        Ok(Self {
            directory,
            webcodex,
            server,
            runner,
            version: cli_version.version,
            git_commit: cli_version.git_commit,
            source,
        })
    }

    pub fn info(&self) -> BinaryInfo {
        BinaryInfo {
            directory: self.directory.to_string_lossy().to_string(),
            version: self.version.clone(),
            git_commit: self.git_commit.clone(),
            source: self.source.label().to_string(),
        }
    }
}

#[derive(Debug)]
struct VersionLine {
    version: String,
    git_commit: String,
}

async fn binary_version(path: &Path) -> DesktopResult<VersionLine> {
    let output = run_bounded(path, &["--version".to_string()], None, false).await?;
    if output.exit_code != Some(0) {
        return Err(DesktopError::new(
            "binary_probe_failed",
            format!("Could not read build identity from {}", path.display()),
            "Rebuild the WebCodex dogfood binaries.",
        ));
    }
    parse_version_line(&output.stdout).ok_or_else(|| {
        DesktopError::new(
            "binary_probe_failed",
            format!("{} returned an invalid version identity", path.display()),
            "Rebuild the WebCodex dogfood binaries from a compatible source baseline.",
        )
    })
}

fn parse_version_line(output: &[u8]) -> Option<VersionLine> {
    let text = std::str::from_utf8(output).ok()?.trim();
    let mut fields = text.split_whitespace();
    fields.next()?;
    let version = fields.next()?.to_string();
    let commit_marker = text.find("(commit ")? + "(commit ".len();
    let tail = &text[commit_marker..];
    let end = tail.find([',', ')']).unwrap_or(tail.len());
    let git_commit = tail[..end].trim().to_string();
    if version.is_empty() || git_commit.is_empty() {
        return None;
    }
    Some(VersionLine {
        version,
        git_commit,
    })
}

pub async fn run_json<T: DeserializeOwned>(
    executable: &Path,
    args: &[String],
    stdin: Option<&[u8]>,
    secret_output: bool,
) -> DesktopResult<T> {
    let output = run_bounded(executable, args, stdin, secret_output).await?;
    if output.exit_code != Some(0) {
        return Err(DesktopError::new(
            "webcodex_command_failed",
            "WebCodex did not complete the requested operation",
            "Open Activity for safe diagnostics, correct the configuration, and retry.",
        )
        .with_details(serde_json::json!({ "exit_code": output.exit_code })));
    }
    serde_json::from_slice(&output.stdout).map_err(|_| {
        DesktopError::new(
            "webcodex_contract_invalid",
            "WebCodex returned invalid machine-readable output",
            "Verify that Desktop and WebCodex binaries come from the same source baseline.",
        )
    })
}

struct BoundedOutput {
    exit_code: Option<i32>,
    stdout: Vec<u8>,
    #[allow(dead_code)]
    stderr: Vec<u8>,
}

async fn run_bounded(
    executable: &Path,
    args: &[String],
    stdin_payload: Option<&[u8]>,
    _secret_output: bool,
) -> DesktopResult<BoundedOutput> {
    let mut command = Command::new(executable);
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // One-shot Desktop adapter calls must not survive cancellation or an
        // early stdin/write error. Long-lived Server/Runner/Share processes
        // are owned separately by ProcessSupervisor instead.
        .kill_on_drop(true);
    if stdin_payload.is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }
    platform::configure_child(&mut command);
    let mut child = command.spawn().map_err(|error| {
        DesktopError::new(
            "webcodex_command_start_failed",
            "Could not start a WebCodex command",
            "Check the Desktop binary directory and Windows execution permissions.",
        )
        .with_details(serde_json::json!({ "io_kind": format!("{:?}", error.kind()) }))
    })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        DesktopError::new(
            "webcodex_command_start_failed",
            "Could not capture WebCodex output",
            "Retry the operation.",
        )
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        DesktopError::new(
            "webcodex_command_start_failed",
            "Could not capture WebCodex diagnostics",
            "Retry the operation.",
        )
    })?;
    let stdout_task = tokio::spawn(read_bounded(stdout));
    let stderr_task = tokio::spawn(read_bounded(stderr));
    if let Some(payload) = stdin_payload {
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(payload).await.map_err(|_| {
                DesktopError::new(
                    "webcodex_command_input_failed",
                    "Could not pass protected input to WebCodex",
                    "Retry the operation.",
                )
            })?;
            stdin.shutdown().await.ok();
        }
    }
    let status = match tokio::time::timeout(CLI_TIMEOUT, child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(_)) => {
            return Err(DesktopError::new(
                "webcodex_command_wait_failed",
                "Could not observe the WebCodex command result",
                "Retry the operation.",
            ))
        }
        Err(_) => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            return Err(DesktopError::new(
                "webcodex_command_timeout",
                "WebCodex did not finish within the Desktop command timeout",
                "Check Server reachability and retry.",
            ));
        }
    };
    let stdout = stdout_task.await.unwrap_or_default();
    let stderr = stderr_task.await.unwrap_or_default();
    Ok(BoundedOutput {
        exit_code: status.code(),
        stdout,
        stderr,
    })
}

async fn read_bounded<R: AsyncRead + Unpin>(mut reader: R) -> Vec<u8> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let read = match reader.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };
        if output.len() < CLI_OUTPUT_BYTES {
            let remaining = CLI_OUTPUT_BYTES - output.len();
            output.extend_from_slice(&buffer[..read.min(remaining)]);
        }
    }
    output
}

fn executable_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parser_requires_commit_identity() {
        let parsed =
            parse_version_line(b"webcodex 0.3.9 (commit 0123456789abcdef, dirty=false)\n").unwrap();
        assert_eq!(parsed.version, "0.3.9");
        assert_eq!(parsed.git_commit, "0123456789abcdef");
        assert!(parse_version_line(b"webcodex 0.3.9\n").is_none());
    }
}
