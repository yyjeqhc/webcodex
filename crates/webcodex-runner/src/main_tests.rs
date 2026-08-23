use super::*;
use crate::webcodex_runner::config::validate_shell_config;
#[cfg(target_os = "linux")]
use crate::webcodex_runner::run_shell_with_profiles_in_sandbox;
use crate::webcodex_runner::{
    handle_project_lifecycle_op, handle_project_op_with_temporary_projects_root,
    handle_resolve_or_register_project,
};
pub(crate) static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(crate) fn test_env_lock() -> std::sync::MutexGuard<'static, ()> {
    TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// RAII restore for environment variables mutated by tests: restores the
/// previous value (or absence) on drop, even when the test panics, so a
/// failure cannot leak env state into later tests.
pub(crate) struct EnvGuard {
    restored: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl EnvGuard {
    pub(crate) fn new() -> Self {
        EnvGuard {
            restored: Vec::new(),
        }
    }

    pub(crate) fn set(mut self, name: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        self.capture(name);
        std::env::set_var(name, value.as_ref());
        self
    }

    pub(crate) fn remove(mut self, name: &'static str) -> Self {
        self.capture(name);
        std::env::remove_var(name);
        self
    }

    fn capture(&mut self, name: &'static str) {
        self.restored.push((name, std::env::var_os(name)));
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (name, value) in self.restored.drain(..).rev() {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}

/// Policy for tests that exercise shell/profile behavior inside a temp dir
/// rather than the filesystem boundary itself. `AgentPolicy::default()` is
/// now fail-closed (empty `allowed_roots` reaches nothing), so these tests
/// opt out of the boundary explicitly instead of leaning on a permissive
/// production default.
fn unrestricted_test_policy() -> AgentPolicy {
    AgentPolicy {
        allow_cwd_anywhere: true,
        ..AgentPolicy::default()
    }
}

fn test_config(projects_dir: PathBuf) -> AgentConfig {
    AgentConfig {
        server_url: "http://127.0.0.1:8000".to_string(),
        token: "test-token".to_string(),
        client_id: "oe".to_string(),
        display_name: None,
        owner: Some("alice".to_string()),
        hostname: None,
        host_context: None,
        projects_dir: Some(projects_dir),
        temporary_projects_root: None,
        poll_interval_ms: 1000,
        capabilities: None,
        max_concurrent_jobs: None,
        policy: unrestricted_test_policy(),
        shell: ShellConfig::default(),
        ssh: SshConfig::default(),
        transport: None,
        websocket_connect_timeout_secs: default_websocket_connect_timeout_secs(),
        quic: None,
        tool_providers: Default::default(),
        mcp_gateway: Default::default(),
        acp: Default::default(),
    }
}

#[test]
fn detached_process_capability_matches_supported_native_backends() {
    let capabilities = agent_register_capabilities(&test_config(PathBuf::new()));
    assert_eq!(
        capabilities.detached_process_jobs,
        cfg!(any(target_os = "linux", target_os = "macos", windows))
    );
}

#[test]
fn pointer_capability_matches_supported_native_backends() {
    let capabilities = agent_register_capabilities(&test_config(PathBuf::new()));
    assert_eq!(
        capabilities.computer_pointer_control,
        cfg!(any(target_os = "macos", windows))
    );
}

fn runtime_config(cfg: &AgentConfig) -> Arc<ReloadableAgentConfig> {
    Arc::new(ReloadableAgentConfig::new(cfg.clone(), PathBuf::new()))
}

fn quic_client_config() -> QuicClientConfig {
    QuicClientConfig {
        server_addr: "v4.example.test:8443".to_string(),
        server_name: "v4.example.test".to_string(),
        alpn: default_quic_alpn(),
        connect_timeout_secs: default_quic_connect_timeout_secs(),
        keepalive_interval_secs: default_quic_keepalive_interval_secs(),
    }
}

#[path = "main_tests/agent_config.rs"]
mod agent_config;
#[path = "main_tests/agent_sink.rs"]
mod agent_sink;
#[path = "main_tests/apply_text_edits.rs"]
mod apply_text_edits;
#[path = "main_tests/artifact_read.rs"]
mod artifact_read;
#[path = "main_tests/artifact_upload.rs"]
mod artifact_upload;
#[path = "main_tests/config_reload.rs"]
mod config_reload;
#[path = "main_tests/dispatch_file.rs"]
mod dispatch_file;
#[path = "main_tests/dispatch_shell.rs"]
mod dispatch_shell;
#[path = "main_tests/file_read.rs"]
mod file_read;
#[path = "main_tests/http_recovery.rs"]
mod http_recovery;
#[path = "main_tests/managed_temporary_projects.rs"]
mod managed_temporary_projects;
#[path = "main_tests/profile_process_lifecycle.rs"]
mod profile_process_lifecycle;
#[path = "main_tests/project_creation.rs"]
mod project_creation;
#[path = "main_tests/project_durability.rs"]
mod project_durability;
#[path = "main_tests/project_policy.rs"]
mod project_policy;
#[path = "main_tests/project_registration.rs"]
mod project_registration;
#[path = "main_tests/registration.rs"]
mod registration;
#[path = "main_tests/shell_config.rs"]
mod shell_config;
#[path = "main_tests/shell_job_execution.rs"]
mod shell_job_execution;
#[path = "main_tests/shell_job_tree.rs"]
mod shell_job_tree;
#[path = "main_tests/shell_profiles.rs"]
mod shell_profiles;
#[path = "main_tests/structured_delete.rs"]
mod structured_delete;
#[path = "main_tests/write_project_file.rs"]
mod write_project_file;

// ---------------------------------------------------------------------------
// Stage 2G: platform-aware shell test helpers.
//
// The default shell is `sh -c` on Unix and native PowerShell on Windows, so
// tests that exercise the default shell use dialect-appropriate command text.
// PowerShell output goes through `[Console]::Out` / `[Console]::Error` (no
// host line terminators), and profile init snippets use the platform's
// variable syntax.
// ---------------------------------------------------------------------------

/// Command text that writes `text` to stdout with no trailing newline, using
/// this platform's default shell dialect.
#[cfg(windows)]
fn shell_echo(text: &str) -> String {
    format!("[Console]::Out.Write('{}')", text.replace('\'', "''"))
}

#[cfg(not(windows))]
fn shell_echo(text: &str) -> String {
    format!("printf %s '{}'", text.replace('\'', "'\\''"))
}

/// Command text that writes `text` to stderr with no trailing newline.
#[cfg(windows)]
fn shell_echo_err(text: &str) -> String {
    format!("[Console]::Error.Write('{}')", text.replace('\'', "''"))
}

#[cfg(not(windows))]
fn shell_echo_err(text: &str) -> String {
    format!("printf %s '{}' >&2", text.replace('\'', "'\\''"))
}

/// Command text that writes the value of environment variable `name`.
#[cfg(windows)]
fn shell_env_var(name: &str) -> String {
    format!("[Console]::Out.Write($env:{name})")
}

#[cfg(not(windows))]
fn shell_env_var(name: &str) -> String {
    format!("printf %s \"${}\"", name)
}

/// Command text that echoes stdin back to stdout.
#[cfg(windows)]
fn shell_stdin_cat() -> String {
    "[Console]::Out.Write([Console]::In.ReadToEnd())".to_string()
}

#[cfg(not(windows))]
fn shell_stdin_cat() -> String {
    "cat".to_string()
}

/// Command text that writes `ran` into `path` (a side-effect marker proving a
/// command executed).
#[cfg(windows)]
fn shell_write_file(path: &Path) -> String {
    format!(
        "[IO.File]::WriteAllText({}, 'ran')",
        shell_tree_quote(&path.to_string_lossy())
    )
}

#[cfg(not(windows))]
fn shell_write_file(path: &Path) -> String {
    format!("printf ran > {}", shell_tree_quote(&path.to_string_lossy()))
}

/// Command text that prints `absent` when `name` is not set and `present`
/// when it is set (even to an empty value).
#[cfg(windows)]
fn shell_if_else_env_present(name: &str) -> String {
    format!(
        "if ($null -eq $env:{name}) {{ [Console]::Out.Write('absent') }} else {{ [Console]::Out.Write('present') }}"
    )
}

#[cfg(not(windows))]
fn shell_if_else_env_present(name: &str) -> String {
    format!("if [ -z \"${{{name}+x}}\" ]; then printf absent; else printf present; fi")
}

/// Profile init-script snippet that exports `name=value`, matching this
/// platform's default shell dialect.
#[cfg(windows)]
fn profile_init_export(name: &str, value: &str) -> String {
    format!("$env:{name} = '{}'", value.replace('\'', "''"))
}

#[cfg(not(windows))]
fn profile_init_export(name: &str, value: &str) -> String {
    format!("export {name}={value}")
}

#[cfg(target_os = "linux")]
#[test]
fn inspect_shell_real_smoke_reads_checks_and_blocks_project_writes() {
    if crate::command_sandbox::inspect_sandbox_available().is_err() {
        // The fail-closed unavailable path has a dedicated sandbox test.
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("project");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::create_dir_all(&projects_dir).unwrap();
    let manifest =
        "[package]\nname = \"inspect-runner-smoke\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
    let lockfile = "# This file is automatically @generated by Cargo.\n\
                        # It is not intended for manual editing.\n\
                        version = 3\n\n\
                        [[package]]\n\
                        name = \"inspect-runner-smoke\"\n\
                        version = \"0.1.0\"\n";
    std::fs::write(project.join("Cargo.toml"), manifest).unwrap();
    std::fs::write(project.join("Cargo.lock"), lockfile).unwrap();
    std::fs::write(project.join("src/lib.rs"), "pub fn answer() -> u8 { 42 }\n").unwrap();
    let git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(&project)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "inspect@example.invalid"]);
    git(&["config", "user.name", "Inspect Smoke"]);
    git(&["add", "."]);
    git(&["commit", "-qm", "seed"]);

    let run_inspect = |command: &str| {
        run_shell_with_profiles_in_sandbox(
            1,
            &unrestricted_test_policy(),
            &ShellConfig::default(),
            &projects_dir,
            &PreparedShellProfileCache::default(),
            Some(project.to_string_lossy().as_ref()),
            command,
            None,
            60,
            None,
            Some(crate::command_sandbox::INSPECT_SANDBOX_MODE),
        )
    };

    let inspection = run_inspect(
        "if command -v rg >/dev/null 2>&1; then rg 'inspect-runner-smoke' Cargo.toml; fi \
             && git status --short \
             && cargo check --offline \
             && printf scratch-ok > \"$TMPDIR/proof\" \
             && test \"$(cat \"$TMPDIR/proof\")\" = scratch-ok",
    );
    assert_eq!(inspection.exit_code, Some(0), "{inspection:?}");
    assert!(!project.join("target").exists());

    for command in [
        "printf created > created.txt",
        "printf changed > Cargo.toml",
        "truncate -s 0 Cargo.toml",
        "rm Cargo.toml",
        "mv Cargo.toml renamed.toml",
        "sh -c 'printf child > child.txt'",
    ] {
        let denied = run_inspect(command);
        assert_ne!(denied.exit_code, Some(0), "{command}: {denied:?}");
    }
    assert_eq!(
        std::fs::read_to_string(project.join("Cargo.toml")).unwrap(),
        manifest
    );
    assert!(!project.join("created.txt").exists());
    assert!(!project.join("child.txt").exists());
    assert!(!project.join("renamed.toml").exists());

    let normal = run_shell(
        &unrestricted_test_policy(),
        &ShellConfig::default(),
        Some(project.to_string_lossy().as_ref()),
        "printf normal-ok > normal.txt",
        None,
        10,
        None,
    );
    assert_eq!(normal.exit_code, Some(0), "{normal:?}");
    assert_eq!(
        std::fs::read_to_string(project.join("normal.txt")).unwrap(),
        "normal-ok"
    );
}

fn shell_with_profiles(
    default_profile: Option<&str>,
    profiles: Vec<(&str, ShellProfileConfig)>,
) -> ShellConfig {
    ShellConfig {
        default_profile: default_profile.map(str::to_string),
        profiles: profiles
            .into_iter()
            .map(|(name, profile)| (name.to_string(), profile))
            .collect(),
        ..ShellConfig::default()
    }
}

fn profile_env(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn write_agent_project(projects_dir: &Path, id: &str, path: &Path, shell_profile: Option<&str>) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let shell_profile = shell_profile
        .map(|profile| format!("shell_profile = {:?}\n", profile))
        .unwrap_or_default();
    std::fs::write(
        projects_dir.join(format!("{}.toml", id)),
        format!(
            "id = {:?}\npath = {:?}\nname = {:?}\n{}",
            id,
            path.to_string_lossy(),
            id,
            shell_profile
        ),
    )
    .unwrap();
}

fn run_profile_shell(
    policy: &AgentPolicy,
    shell: &ShellConfig,
    projects_dir: &Path,
    cache: &PreparedShellProfileCache,
    cwd: &Path,
    command: &str,
) -> CommandResult {
    let cwd = cwd.to_string_lossy().to_string();
    run_shell_with_profiles(
        1,
        policy,
        shell,
        projects_dir,
        cache,
        Some(&cwd),
        command,
        None,
        10,
        None,
    )
}

fn shell_job_request(cwd: &Path, command: &str) -> ShellAgentShellRequest {
    ShellAgentShellRequest {
        request_id: "req-job".to_string(),
        client_id: "ws-client".to_string(),
        kind: "start_job".to_string(),
        job_id: Some("job-profile".to_string()),
        cwd: Some(cwd.to_string_lossy().to_string()),
        path: None,
        content: None,
        max_bytes: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        create_dirs: false,
        command: command.to_string(),
        process: None,
        script: None,
        stdin: None,
        timeout_secs: 10,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        sandbox: None,
        job_context: Some(test_job_context(cwd, Vec::new())),
        mcp_gateway: None,
        coding_agent: None,
        persistent_shell: None,
    }
}

#[test]
fn runner_recovery_context_rejects_cross_product_go_test_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let mut request = shell_job_request(temp.path(), "");
    request.kind = "start_validation_job".to_string();
    let cargo_step = ShellJobValidationStep {
        name: "test".to_string(),
        program: "cargo".to_string(),
        args: vec!["test".to_string(), "tool_runtime".to_string()],
        env: Vec::new(),
    };
    request.command = serde_json::to_string(&vec![cargo_step.clone()]).unwrap();
    let context = request.job_context.as_mut().unwrap();
    context.purpose = Some("validation".to_string());
    context.validation_steps = vec!["test".to_string()];
    context.validation = Some(shell_protocol::ShellJobValidationMetadata {
        tool: "go_test".to_string(),
        kind: "test".to_string(),
        steps: vec![cargo_step],
        effective_timeout_secs: 1800,
        sync_wait_secs: 10,
        adapter: "go_test".to_string(),
        validation_target_id: None,
        minimum_tests: None,
    });
    let context = context.clone();

    let error = validate_runner_job_context(&context, &request, "ws-client").unwrap_err();
    assert!(error.contains("validation metadata is invalid"), "{error}");
}

fn wait_for_job_envelope(
    rx: &mut tokio::sync::mpsc::Receiver<AgentEnvelope>,
    message: &str,
) -> AgentEnvelope {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match rx.try_recv() {
            Ok(envelope) => return envelope,
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => panic!("{message}"),
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                panic!("{message}: channel disconnected")
            }
        }
    }
}

fn line_edit_json(result: CommandResult) -> serde_json::Value {
    assert_eq!(result.exit_code, Some(0), "unexpected result: {:?}", result);
    assert!(
        result.error.is_none(),
        "unexpected error: {:?}",
        result.error
    );
    serde_json::from_str(result.stdout.as_deref().expect("stdout json")).unwrap()
}

fn json_file_op_request(
    cwd: &Path,
    kind: &str,
    path: &str,
    payload: serde_json::Value,
) -> ShellAgentShellRequest {
    ShellAgentShellRequest {
        request_id: format!("req-{kind}"),
        client_id: "agent-1".to_string(),
        kind: kind.to_string(),
        job_id: None,
        cwd: Some(cwd.to_string_lossy().to_string()),
        path: Some(path.to_string()),
        content: Some(payload.to_string()),
        max_bytes: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        create_dirs: false,
        command: String::new(),
        process: None,
        script: None,
        stdin: None,
        timeout_secs: 30,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        sandbox: None,
        job_context: None,
        mcp_gateway: None,
        coding_agent: None,
        persistent_shell: None,
    }
}

fn append_fake_zip_entry(
    bytes: &mut Vec<u8>,
    central_directory: &mut Vec<u8>,
    name: &str,
    content: &[u8],
    deflate: bool,
) {
    use flate2::{write::DeflateEncoder, Compression};
    use std::io::Write as _;

    let compression_method = if deflate { 8_u16 } else { 0_u16 };
    let compressed = if deflate {
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(content).unwrap();
        encoder.finish().unwrap()
    } else {
        content.to_vec()
    };
    let local_offset = u32::try_from(bytes.len()).unwrap();
    let compressed_size = u32::try_from(compressed.len()).unwrap();
    let uncompressed_size = u32::try_from(content.len()).unwrap();
    let name_len = u16::try_from(name.len()).unwrap();

    bytes.extend_from_slice(b"PK\x03\x04");
    bytes.extend_from_slice(&20_u16.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&compression_method.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&compressed_size.to_le_bytes());
    bytes.extend_from_slice(&uncompressed_size.to_le_bytes());
    bytes.extend_from_slice(&name_len.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(name.as_bytes());
    bytes.extend_from_slice(&compressed);

    central_directory.extend_from_slice(b"PK\x01\x02");
    central_directory.extend_from_slice(&20_u16.to_le_bytes());
    central_directory.extend_from_slice(&20_u16.to_le_bytes());
    central_directory.extend_from_slice(&0_u16.to_le_bytes());
    central_directory.extend_from_slice(&compression_method.to_le_bytes());
    central_directory.extend_from_slice(&0_u16.to_le_bytes());
    central_directory.extend_from_slice(&0_u16.to_le_bytes());
    central_directory.extend_from_slice(&0_u32.to_le_bytes());
    central_directory.extend_from_slice(&compressed_size.to_le_bytes());
    central_directory.extend_from_slice(&uncompressed_size.to_le_bytes());
    central_directory.extend_from_slice(&name_len.to_le_bytes());
    central_directory.extend_from_slice(&0_u16.to_le_bytes());
    central_directory.extend_from_slice(&0_u16.to_le_bytes());
    central_directory.extend_from_slice(&0_u16.to_le_bytes());
    central_directory.extend_from_slice(&0_u16.to_le_bytes());
    central_directory.extend_from_slice(&0_u32.to_le_bytes());
    central_directory.extend_from_slice(&local_offset.to_le_bytes());
    central_directory.extend_from_slice(name.as_bytes());
}

fn fake_ooxml_zip(
    main_part: &str,
    main_content_type: &str,
    malformed_content_types: bool,
) -> Vec<u8> {
    let content_types = if malformed_content_types {
        b"<Types".to_vec()
    } else {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/{main_part}" ContentType="{main_content_type}"/></Types>"#
        )
        .into_bytes()
    };
    let mut bytes = Vec::new();
    let mut central_directory = Vec::new();
    append_fake_zip_entry(
        &mut bytes,
        &mut central_directory,
        "[Content_Types].xml",
        &content_types,
        true,
    );
    append_fake_zip_entry(
        &mut bytes,
        &mut central_directory,
        main_part,
        b"<root/>",
        false,
    );
    let central_offset = u32::try_from(bytes.len()).unwrap();
    let central_size = u32::try_from(central_directory.len()).unwrap();
    bytes.extend_from_slice(&central_directory);
    bytes.extend_from_slice(b"PK\x05\x06");
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&central_size.to_le_bytes());
    bytes.extend_from_slice(&central_offset.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes
}

fn directory_contains_name_prefix(dir: &Path, prefix: &str) -> bool {
    if !dir.exists() {
        return false;
    }
    std::fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .any(|name| name.starts_with(prefix))
}

#[cfg(unix)]
fn shell_quote_path(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

#[cfg(unix)]
fn wait_until(timeout: Duration, predicate: impl Fn() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if predicate() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    predicate()
}

#[cfg(unix)]
fn descendant_is_gone(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(target_os = "linux")]
    if let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        let zombie = stat
            .rsplit_once(") ")
            .and_then(|(_, rest)| rest.chars().next())
            .is_some_and(|state| state == 'Z');
        if zombie {
            // A zombie cannot execute and proves the process-group signal
            // terminated the descendant. Minimal container PID 1
            // implementations may leave adopted zombies visible long
            // after the runner has exhausted everything it can reap.
            return true;
        }
    }
    // SAFETY: signal 0 only probes the PID written by this test command;
    // it does not deliver a signal to the process.
    let missing = unsafe { libc::kill(pid as i32, 0) == -1 };
    missing && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
}

#[cfg(unix)]
struct DescendantCleanup {
    pid: u32,
}

#[cfg(unix)]
impl DescendantCleanup {
    fn disarm(&mut self) {
        self.pid = 0;
    }
}

#[cfg(unix)]
impl Drop for DescendantCleanup {
    fn drop(&mut self) {
        if self.pid != 0 {
            // SAFETY: the PID was created by this test. This is a
            // best-effort failure-path cleanup and never targets a group.
            unsafe {
                libc::kill(self.pid as i32, libc::SIGKILL);
            }
        }
    }
}

#[cfg(unix)]
fn assert_descendant_reaped(pid_file: &Path) {
    assert!(
        wait_until(Duration::from_secs(2), || pid_file.exists()),
        "descendant pid file was not created: {}",
        pid_file.display()
    );
    let pid = std::fs::read_to_string(pid_file)
        .expect("read descendant pid file")
        .trim()
        .parse::<u32>()
        .expect("parse descendant pid");
    let mut cleanup = DescendantCleanup { pid };
    assert!(
        wait_until(Duration::from_secs(5), || descendant_is_gone(pid)),
        "descendant {pid} survived synchronous shell cancellation"
    );
    cleanup.disarm();
}

// ---------------------------------------------------------------------------
// Stage 2F: shell.rs ManagedChild lifecycle coverage.
//
// The shell execution path owns its process tree through ManagedChild (a
// private process group on Unix, a kill-on-close Job Object on Windows). These
// tests drive the cross-platform `validation_tree_helper` fixture through the
// real configured shell (`sh -c` on Unix, PowerShell on Windows) and probe
// descendant pids with a platform-native liveness probe — never with
// taskkill / Stop-Process / wmic / `ps` and never through shell quoting of
// process listings.
// ---------------------------------------------------------------------------

/// Compiled copy of the `validation_tree_helper` fixture, kept alive for the
/// whole test process so its binary path never disappears under a running
/// descendant (same pattern as the validation lifecycle tests).
struct ShellTreeHelper {
    _temp: tempfile::TempDir,
    path: PathBuf,
}

static SHELL_TREE_HELPER: std::sync::OnceLock<std::sync::Arc<ShellTreeHelper>> =
    std::sync::OnceLock::new();

fn shell_tree_helper() -> PathBuf {
    SHELL_TREE_HELPER
        .get_or_init(|| {
            let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("src/webcodex_runner/validation/validation_tree_helper.rs");
            let temp = tempfile::tempdir().unwrap();
            let output = temp
                .path()
                .join(format!("shell-tree-helper{}", std::env::consts::EXE_SUFFIX));
            let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
            let result = std::process::Command::new(rustc)
                .arg("--edition=2021")
                .arg("--crate-name=webcodex_shell_tree_helper")
                .arg(&source)
                .arg("-o")
                .arg(&output)
                .output()
                .expect("run rustc for shell tree helper");
            assert!(
                result.status.success(),
                "shell tree helper compilation failed: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            std::sync::Arc::new(ShellTreeHelper {
                _temp: temp,
                path: output,
            })
        })
        .path
        .clone()
}

/// Single-quote `value` for the platform test shell. Windows uses PowerShell
/// ('' escapes an embedded quote); Unix uses POSIX sh ('\'' escapes one).
#[cfg(windows)]
fn shell_tree_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(not(windows))]
fn shell_tree_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Build the shell command line that runs the fixture helper with `args`.
///
/// Windows drives the helper through PowerShell with single-quoted paths (no
/// cmd.exe quote-parsing pitfalls) and appends `exit $LASTEXITCODE` so the
/// helper's exit status becomes the shell's exit status. Unix uses the POSIX
/// shell directly.
fn shell_tree_command(helper: &Path, args: &[String]) -> String {
    let mut parts: Vec<String> = vec![shell_tree_quote(&helper.to_string_lossy())];
    parts.extend(args.iter().map(|arg| shell_tree_quote(arg)));
    let joined = parts.join(" ");
    #[cfg(windows)]
    {
        format!("& {joined}; exit $LASTEXITCODE")
    }
    #[cfg(not(windows))]
    {
        joined
    }
}

/// Test shell that can actually run on this platform: PowerShell on Windows
/// (cmd.exe quote parsing and missing `sleep` make POSIX-style commands
/// unusable), the default `sh -c` on Unix.
#[cfg(windows)]
fn shell_tree_test_shell() -> ShellConfig {
    ShellConfig {
        program: "powershell.exe".to_string(),
        args: vec!["-NoProfile".to_string(), "-Command".to_string()],
        ..ShellConfig::default()
    }
}

#[cfg(not(windows))]
fn shell_tree_test_shell() -> ShellConfig {
    ShellConfig::default()
}

/// Shell timeout used by the tree tests: Windows needs headroom for
/// PowerShell startup, Unix shells start instantly.
fn shell_tree_test_timeout_secs() -> u64 {
    if cfg!(windows) {
        5
    } else {
        1
    }
}

/// Platform-native liveness probe, so the tree tests never shell out to
/// `tasklist` / `ps` / PowerShell / wmic and never depend on shell quoting.
#[cfg(windows)]
fn shell_tree_process_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    // SAFETY: OpenProcess returns a handle or NULL; NULL means the pid no
    // longer exists (or is inaccessible, which also means not ours).
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return false;
    }
    let mut exit_code = 0u32;
    // SAFETY: `handle` is valid; `exit_code` is a valid out-param.
    let ok = unsafe { GetExitCodeProcess(handle, &mut exit_code) };
    // SAFETY: close the handle we opened.
    unsafe { CloseHandle(handle) };
    ok == 1 && exit_code == 259 // 259 == STILL_ACTIVE
}

#[cfg(target_os = "linux")]
fn shell_tree_process_alive(pid: u32) -> bool {
    let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };
    stat.rsplit_once(") ")
        .and_then(|(_, rest)| rest.chars().next())
        .is_some_and(|state| state != 'Z')
}

#[cfg(all(unix, not(target_os = "linux")))]
fn shell_tree_process_alive(pid: u32) -> bool {
    // SAFETY: signal 0 is an existence probe; the pid comes from our own
    // helper subprocess.
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

fn wait_until_file(path: &Path, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if path.exists() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn wait_until_process_dead(pid: u32, timeout: Duration, tag: &str) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if !shell_tree_process_alive(pid) {
            return true;
        }
        if Instant::now() >= deadline {
            eprintln!("wait_until_process_dead({tag}): pid {pid} still alive");
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Parse `KEY=<pid>` from a marker file written by the fixture helper.
fn read_marker_pid(marker: &Path, key: &str) -> u32 {
    let text = std::fs::read_to_string(marker).expect("read pid marker");
    text.lines()
        .find_map(|line| {
            line.strip_prefix(key)
                .and_then(|rest| rest.strip_prefix('='))
                .and_then(|value| value.trim().parse().ok())
        })
        .unwrap_or_else(|| panic!("marker {marker:?} missing {key}: {text}"))
}

/// Marker paths and the keepalive command for the two-argument
/// `spawn-descendant-keepalive` / `spawn-descendant` fixtures.
struct ShellTreeMarkers {
    parent: PathBuf,
    alive: PathBuf,
}

impl ShellTreeMarkers {
    fn in_dir(tmp: &std::path::Path, tag: &str) -> Self {
        Self {
            parent: tmp.join(format!("{tag}-parent.txt")),
            alive: tmp.join(format!("{tag}-alive.txt")),
        }
    }

    fn keepalive_command(&self, helper: &Path) -> String {
        shell_tree_command(
            helper,
            &[
                "spawn-descendant-keepalive".to_string(),
                self.parent.to_string_lossy().into_owned(),
                self.alive.to_string_lossy().into_owned(),
                "120".to_string(),
            ],
        )
    }

    /// Both pids must be dead after cancellation; `PARENT_PID` and
    /// `DESCENDANT_PID` are both written to the parent marker.
    fn assert_tree_dead(&self, tag: &str) {
        let parent = read_marker_pid(&self.parent, "PARENT_PID");
        let descendant = read_marker_pid(&self.parent, "DESCENDANT_PID");
        assert!(
            wait_until_process_dead(parent, Duration::from_secs(10), &format!("{tag}-parent")),
            "tree parent {parent} survived {tag}"
        );
        assert!(
            wait_until_process_dead(
                descendant,
                Duration::from_secs(10),
                &format!("{tag}-descendant")
            ),
            "tree descendant {descendant} survived {tag}"
        );
    }
}

// ---------------------------------------------------------------------------
// Stage 2G: Windows-native PowerShell shell semantics.
// ---------------------------------------------------------------------------

#[cfg(windows)]
#[test]
fn shell_job_native_exe_nonzero_exit_code_is_preserved() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().to_string_lossy().to_string();
    let helper = shell_tree_helper();
    // No fixture-level `exit $LASTEXITCODE`: the PowerShell command wrapper
    // must propagate the native executable's exit code on its own.
    let command = format!(
        "& {} sleep 0 3",
        shell_tree_quote(&helper.to_string_lossy())
    );
    let result = run_shell(
        &unrestricted_test_policy(),
        &shell_tree_test_shell(),
        Some(&cwd),
        &command,
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(3), "{result:?}");
    assert!(result.error.is_none(), "{result:?}");
}

#[cfg(windows)]
#[test]
fn shell_job_unicode_stdout_stderr_env_and_cwd() {
    let _guard = test_env_lock();
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let unicode_cwd = tmp.path().join("unicode cwd 测试");
    std::fs::create_dir_all(&unicode_cwd).unwrap();
    let cwd = unicode_cwd.to_string_lossy().to_string();

    // Unicode stdout and stderr. The PowerShell wrapper installs the UTF-8
    // console encodings before the command, so this never depends on the
    // machine's legacy console code page.
    let result = run_shell(
        &cfg.policy,
        &ShellConfig::default(),
        Some(&cwd),
        "[Console]::Out.Write('café ☃ 测试'); [Console]::Error.Write('err 測試')",
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert_eq!(result.stdout.as_deref(), Some("café ☃ 测试"));
    assert_eq!(result.stderr.as_deref(), Some("err 測試"));

    // Unicode environment value inherited from the parent process.
    let _env = EnvGuard::new().set("WEBCODEX_UNICODE_ENV", "值 测试");
    let result = run_shell(
        &cfg.policy,
        &ShellConfig::default(),
        Some(&cwd),
        &shell_env_var("WEBCODEX_UNICODE_ENV"),
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert_eq!(result.stdout.as_deref(), Some("值 测试"));

    // Unicode cwd: the shell reports its working directory verbatim.
    let result = run_shell(
        &cfg.policy,
        &ShellConfig::default(),
        Some(&cwd),
        "[Console]::Out.Write((Get-Location).Path)",
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert!(
        result
            .stdout
            .as_deref()
            .unwrap_or_default()
            .contains("测试"),
        "{result:?}"
    );
}

#[cfg(windows)]
#[test]
fn prepared_profile_unicode_env_round_trip_and_unicode_init_path() {
    let tmp = tempfile::tempdir().unwrap();
    // The init script lives in a directory with spaces and non-ASCII
    // characters, and exports a Unicode value that must survive the whole
    // snapshot pipeline: PowerShell `Get-ChildItem Env:` -> UTF-8 NUL
    // payload -> Runner parse -> environment of later commands.
    let init_dir = tmp.path().join("init 目录");
    std::fs::create_dir_all(&init_dir).unwrap();
    let init = init_dir.join("profile 脚本.ps1");
    // UTF-8 BOM: PowerShell 5.1 otherwise decodes .ps1 files with the system
    // ANSI code page and corrupts the non-ASCII value.
    let mut content = "\u{FEFF}".to_string();
    content.push_str("$env:WEBCODEX_TEST_PROFILE = 'café 值'\n");
    std::fs::write(&init, content).unwrap();
    let shell = shell_with_profiles(
        Some("test"),
        vec![(
            "test",
            ShellProfileConfig {
                init_script: Some(format!(". {}", shell_tree_quote(&init.to_string_lossy()))),
                ..ShellProfileConfig::default()
            },
        )],
    );
    let result = run_profile_shell(
        &unrestricted_test_policy(),
        &shell,
        tmp.path(),
        &PreparedShellProfileCache::default(),
        tmp.path(),
        &shell_env_var("WEBCODEX_TEST_PROFILE"),
    );
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert_eq!(result.stdout.as_deref(), Some("café 值"), "{result:?}");
}

// ------------------------------------------------------------------------
// WebSocket transport helpers + shared dispatch over a WebSocket sink
// ------------------------------------------------------------------------

#[test]
fn server_url_to_ws_converts_http_https_and_rejects_bare() {
    assert_eq!(
        server_url_to_ws("http://127.0.0.1:8080", "/api/agents/ws").unwrap(),
        "ws://127.0.0.1:8080/api/agents/ws"
    );
    assert_eq!(
        server_url_to_ws("https://example.com/", "/api/agents/ws").unwrap(),
        "wss://example.com/api/agents/ws"
    );
    // Already a ws(s) URL passes through.
    assert_eq!(
        server_url_to_ws("wss://example.com", "/api/agents/ws").unwrap(),
        "wss://example.com/api/agents/ws"
    );
    assert!(server_url_to_ws("ftp://x", "/api/agents/ws").is_err());
}

#[test]
fn generated_agent_instance_id_is_non_empty_uuid_like() {
    // `run_agent` generates the instance id the same way; verify the
    // format here without driving the full agent loop.
    let id = uuid::Uuid::new_v4().to_string();
    assert!(!id.is_empty());
    // Canonical UUID v4 is 36 chars: 8-4-4-4-12 hex groups.
    assert_eq!(id.len(), 36);
    assert_eq!(id.chars().filter(|c| *c == '-').count(), 4);
    assert!(id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
    // The register builder carries it through unchanged.
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let body = build_register_request(&cfg, Vec::new(), AGENT_PROTOCOL_VERSION_POLLING_V1, &id, 0);
    assert_eq!(body.agent_instance_id, id);
    assert!(!body.agent_instance_id.is_empty());
    assert_eq!(
        body.agent_protocol_version.as_deref(),
        Some(AGENT_PROTOCOL_VERSION_POLLING_V1),
        "first-party registration builders must always declare protocol identity"
    );
}

fn ws_sink(client_id: &str) -> (AgentSink, tokio::sync::mpsc::Receiver<AgentEnvelope>) {
    let (tx, rx) = tokio::sync::mpsc::channel::<AgentEnvelope>(WS_OUTGOING_CAPACITY);
    (
        AgentSink::WebSocket {
            tx,
            client_id: client_id.to_string(),
            agent_instance_id: "ws-inst".to_string(),
        },
        rx,
    )
}

fn quic_sink(client_id: &str) -> (AgentSink, tokio::sync::mpsc::Receiver<AgentEnvelope>) {
    let (tx, rx) = tokio::sync::mpsc::channel::<AgentEnvelope>(WS_OUTGOING_CAPACITY);
    (
        AgentSink::Quic {
            tx,
            client_id: client_id.to_string(),
            agent_instance_id: "quic-inst".to_string(),
        },
        rx,
    )
}

#[cfg(unix)]
#[test]
fn job_manager_stop_all_clears_queue_and_requests_running_stop() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let jobs = JobManager::new(1);
    let stop_requested = Arc::new(AtomicBool::new(false));
    let mut running_command =
        configured_shell_job_command(&ShellConfig::default(), "sleep 60").unwrap();
    running_command
        .current_dir(tmp.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let running_child = Arc::new(Mutex::new(
        ManagedChild::spawn(&mut running_command).unwrap(),
    ));
    let running_pid = lock_unpoison(&running_child).id();
    jobs.jobs.lock().unwrap().insert(
        "running-job".to_string(),
        RunningJob {
            client_id: "ws-client".to_string(),
            agent_instance_id: "ws-instance".to_string(),
            snapshot: test_job_snapshot("running-job"),
            child: Some(Arc::clone(&running_child)),
            stop_requested: stop_requested.clone(),
            slot_reserved: true,
        },
    );
    let (sink, mut rx) = ws_sink("ws-client");
    let request = ShellAgentShellRequest {
        request_id: "req-queued".to_string(),
        client_id: "ws-client".to_string(),
        kind: "start_job".to_string(),
        job_id: Some("queued-job".to_string()),
        cwd: Some(tmp.path().to_string_lossy().to_string()),
        path: None,
        content: None,
        max_bytes: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        create_dirs: false,
        command: ": > queued-started".to_string(),
        process: None,
        script: None,
        stdin: None,
        timeout_secs: 60,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        sandbox: None,
        job_context: Some(test_job_context(tmp.path(), Vec::new())),
        mcp_gateway: None,
        coding_agent: None,
        persistent_shell: None,
    };
    let mut rejected_request = request.clone();
    rejected_request.request_id = "req-after-shutdown".to_string();
    rejected_request.job_id = Some("job-after-shutdown".to_string());

    jobs.enqueue(
        sink,
        PendingJobStart {
            generation: 1,
            policy: cfg.policy.clone(),
            shell: cfg.shell.clone(),
            ssh: cfg.ssh.clone(),
            projects_dir: projects_dir(&cfg).unwrap(),
            request,
        },
    );
    match wait_for_job_envelope(&mut rx, "queued status was sent") {
        AgentEnvelope::JobUpdate { payload } => {
            assert_eq!(payload.job_id, "queued-job");
            assert_eq!(payload.status, "agent_queued");
        }
        other => panic!("expected job_update, got {:?}", other.kind()),
    }
    assert_eq!(jobs.queued.lock().unwrap().len(), 1);

    jobs.stop_all();

    assert!(stop_requested.load(Ordering::SeqCst));
    assert!(jobs.queued.lock().unwrap().is_empty());
    assert!(lock_unpoison(&running_child).try_wait().unwrap().is_some());
    assert!(!job_manager_tests::process_running(running_pid));
    assert!(
        !tmp.path().join("queued-started").exists(),
        "queued job started during shutdown"
    );

    let (rejected_sink, mut rejected_rx) = ws_sink("ws-client");
    jobs.enqueue(
        rejected_sink,
        PendingJobStart {
            generation: 1,
            policy: cfg.policy.clone(),
            shell: cfg.shell.clone(),
            ssh: cfg.ssh.clone(),
            projects_dir: projects_dir(&cfg).unwrap(),
            request: rejected_request,
        },
    );
    assert!(jobs.queued.lock().unwrap().is_empty());
    let rejected = (0..2)
        .find_map(
            |_| match wait_for_job_envelope(&mut rejected_rx, "shutdown update was sent") {
                AgentEnvelope::JobUpdate { payload } if payload.finished => Some(payload),
                AgentEnvelope::JobUpdate { .. } => None,
                other => panic!("expected job_update, got {:?}", other.kind()),
            },
        )
        .expect("shutdown rejection terminal update was sent");
    assert_eq!(rejected.job_id, "job-after-shutdown");
    assert_eq!(rejected.status, "failed");
    assert!(rejected.finished);
    assert_eq!(rejected.error.as_deref(), Some("runner is shutting down"));
}

fn project_policy(root: &Path) -> AgentPolicy {
    AgentPolicy {
        allow_cwd_anywhere: false,
        allowed_roots: vec![root.to_path_buf()],
        ..AgentPolicy::default()
    }
}

fn project_request(kind: &str, payload: serde_json::Value) -> ShellAgentShellRequest {
    ShellAgentShellRequest {
        request_id: format!("req-{}", kind),
        client_id: "oe".to_string(),
        kind: kind.to_string(),
        job_id: None,
        cwd: None,
        path: None,
        content: None,
        max_bytes: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        create_dirs: false,
        command: String::new(),
        process: None,
        script: None,
        stdin: Some(payload.to_string()),
        timeout_secs: 10,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        sandbox: None,
        job_context: None,
        mcp_gateway: None,
        coding_agent: None,
        persistent_shell: None,
    }
}

fn project_ok(result: CommandResult) -> serde_json::Value {
    assert_eq!(result.exit_code, Some(0), "unexpected result: {:?}", result);
    assert!(
        result.error.is_none(),
        "unexpected error: {:?}",
        result.error
    );
    serde_json::from_str(result.stdout.as_deref().expect("stdout json")).unwrap()
}

fn project_err(result: CommandResult) -> String {
    if let Some(error) = result.error {
        return error;
    }
    assert_ne!(
        result.exit_code,
        Some(0),
        "unexpected success: {:?}",
        result
    );
    serde_json::from_str::<serde_json::Value>(result.stdout.as_deref().expect("error json"))
        .unwrap()["error_code"]
        .as_str()
        .expect("error_code")
        .to_string()
}

fn project_error_value(result: CommandResult) -> serde_json::Value {
    assert_ne!(
        result.exit_code,
        Some(0),
        "unexpected success: {:?}",
        result
    );
    assert!(result.error.is_none(), "unexpected raw error: {:?}", result);
    serde_json::from_str(result.stdout.as_deref().expect("error json")).unwrap()
}

#[test]
fn agent_project_cache_invalidate_refreshes_after_project_op() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("repo");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&project_dir).unwrap();
    let mut cfg = test_config(projects_dir.clone());
    cfg.policy = project_policy(tmp.path());
    let mut cache = AgentProjectCache::default();
    assert!(cache.get(&cfg).is_empty());

    let req = project_request(
        "register_project",
        serde_json::json!({
            "id": "cached",
            "name": "Cached",
            "path": project_dir.to_string_lossy()
        }),
    );
    project_ok(handle_project_op(&cfg.policy, &projects_dir, &req));

    assert!(
        cache.get(&cfg).is_empty(),
        "cache should still be stale before invalidation"
    );
    cache.invalidate();
    let projects = cache.get(&cfg);
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].id, "cached");
}

#[test]
fn http_sink_client_id_matches_config() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let client = Client::new();
    let sink = AgentSink::Http(HttpSendConfig {
        client,
        server_url: cfg.server_url.clone(),
        token: cfg.token.clone(),
        client_id: cfg.client_id.clone(),
        agent_instance_id: "inst-1".to_string(),
        shutdown: Arc::new(AtomicBool::new(false)),
    });
    assert_eq!(sink.client_id(), "oe");
    assert_eq!(sink.agent_instance_id(), "inst-1");
}

#[test]
fn empty_tokens_are_not_sent_as_credentials() {
    use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;

    let request = build_ws_request("ws://127.0.0.1:8080/api/agents/ws", "").unwrap();
    assert!(request.headers().get(AUTHORIZATION).is_none());

    let request = build_ws_request("ws://127.0.0.1:8080/api/agents/ws", "   \t").unwrap();
    assert!(request.headers().get(AUTHORIZATION).is_none());

    let request = build_ws_request("ws://127.0.0.1:8080/api/agents/ws", "  abc123  ").unwrap();
    assert_eq!(
        request.headers().get(AUTHORIZATION).unwrap(),
        "Bearer abc123"
    );
    assert_eq!(request.uri().path(), "/api/agents/ws");
    assert!(
        request.uri().query().is_none(),
        "first-party WebSocket auth must not put credentials in the URL"
    );

    assert_eq!(non_empty_token(""), None);
    assert_eq!(non_empty_token("   \t"), None);
    assert_eq!(non_empty_token("  abc123  "), Some("abc123".to_string()));
}

#[test]
fn empty_tokens_http_register_omits_authorization_header() {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut buf = [0u8; 16 * 1024];
        let n = stream.read(&mut buf).unwrap();
        let request = String::from_utf8_lossy(&buf[..n]).to_string();
        assert!(
            request.starts_with("POST /api/shell/agent/register "),
            "unexpected request: {request}"
        );
        assert!(
            !request.to_ascii_lowercase().contains("authorization:"),
            "empty token must not send Authorization header: {request}"
        );
        let body = r#"{"success":true,"client":null,"error":null}"#;
        write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
    });

    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = test_config(tmp.path().join("projects.d"));
    cfg.server_url = format!("http://{}", addr);
    cfg.token = "   \t".to_string();

    let client = Client::builder().no_proxy().build().unwrap();
    let mut project_cache = AgentProjectCache::default();
    let runtime = ReloadableAgentConfig::new(cfg.clone(), PathBuf::new());
    register(
        &client,
        &cfg,
        &runtime,
        &mut project_cache,
        None,
        "inst-empty-token",
        0,
        &JobManager::new(1),
    )
    .unwrap();
    server.join().unwrap();
}

// ------------------------------------------------------------------------
// WebSocket session: Pong must be handled as keepalive, not unexpected
// ------------------------------------------------------------------------

#[tokio::test]
async fn websocket_session_accepts_pong_without_error_or_disconnect() {
    use futures_util::{SinkExt, StreamExt};
    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::Message as WsMessage;

    // Minimal WS server. It:
    //   1. reads the agent's Register,
    //   2. sends a Registered ack,
    //   3. sends a Pong (the frame that previously triggered the noisy
    //      "ignoring unexpected envelope: pong" path),
    //   4. sends a Ping and waits for the agent's Pong reply — if the
    //      agent had exited on the Pong in step 3 it would never reply,
    //      and this receive would time out (failing the test),
    //   5. drops the socket so the agent's session returns cleanly.
    //
    // This both guards the "Pong is not unexpected" regression and proves
    // the session stays alive after a Pong.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();

        // Read Register.
        let reg_msg = ws.next().await.unwrap().unwrap();
        let reg_env = AgentEnvelope::from_slice(reg_msg.into_text().unwrap().as_bytes()).unwrap();
        assert!(matches!(reg_env, AgentEnvelope::Register { .. }));

        // Ack register.
        let ack = AgentEnvelope::Registered {
            success: true,
            client: None,
            error: None,
        };
        ws.send(WsMessage::Text(ack.to_json().unwrap().into()))
            .await
            .unwrap();

        // Send a Pong — the agent must accept it as keepalive and stay
        // connected (this is the regression we are guarding against).
        let pong = AgentEnvelope::Pong { ts: 42 };
        ws.send(WsMessage::Text(pong.to_json().unwrap().into()))
            .await
            .unwrap();

        // Probe liveness: send a Ping and expect a Pong reply. If the
        // agent had broken out of its read loop on the Pong above, this
        // would time out.
        ws.send(WsMessage::Text(
            AgentEnvelope::Ping { ts: 7 }.to_json().unwrap().into(),
        ))
        .await
        .unwrap();
        let reply = tokio::time::timeout(Duration::from_secs(2), ws.next())
            .await
            .expect("agent did not reply to ping after pong (session exited on pong)")
            .expect("stream open")
            .expect("ok message");
        match AgentEnvelope::from_slice(reply.into_text().unwrap().as_bytes()).unwrap() {
            AgentEnvelope::Pong { ts } => assert_eq!(ts, 7),
            other => panic!("expected pong reply, got {:?}", other.kind()),
        }

        // Drop the socket; the agent's reader will error/EOF and the
        // session returns cleanly. Avoids a close-handshake that can hang
        // on a current-thread test runtime.
        drop(ws);
    });

    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = test_config(tmp.path().join("config/projects.d"));
    cfg.server_url = format!("http://{}", addr);
    cfg.transport = Some(TRANSPORT_WEBSOCKET.to_string());
    let runtime = AgentRuntimeState::new(&cfg, PathBuf::new());

    let outcome = tokio::time::timeout(
        Duration::from_secs(10),
        websocket_session(&cfg, Vec::new(), "inst-1", &runtime),
    )
    .await
    .expect("websocket_session did not complete in time");

    // The session must end (server dropped the socket) and must NOT have
    // returned an error — a Pong is normal keepalive traffic.
    assert!(
        outcome.is_ok(),
        "websocket_session errored on Pong (regression): {:?}",
        outcome
    );

    server_task.await.unwrap();
}
