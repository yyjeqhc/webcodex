use super::*;
use crate::webcodex_runner::config::validate_shell_config;
#[cfg(target_os = "linux")]
use crate::webcodex_runner::run_shell_with_profiles_in_sandbox;
use crate::webcodex_runner::{
    handle_project_lifecycle_op, handle_project_op_with_temporary_projects_root,
    handle_resolve_or_register_project,
};
static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// RAII restore for environment variables mutated by tests: restores the
/// previous value (or absence) on drop, even when the test panics, so a
/// failure cannot leak env state into later tests.
struct EnvGuard {
    restored: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl EnvGuard {
    fn new() -> Self {
        EnvGuard {
            restored: Vec::new(),
        }
    }

    fn set(mut self, name: &'static str, value: &str) -> Self {
        self.capture(name);
        std::env::set_var(name, value);
        self
    }

    fn remove(mut self, name: &'static str) -> Self {
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
    }
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
#[path = "main_tests/apply_text_edits.rs"]
mod apply_text_edits;
#[path = "main_tests/artifact_read.rs"]
mod artifact_read;
#[path = "main_tests/artifact_upload.rs"]
mod artifact_upload;
#[path = "main_tests/config_reload.rs"]
mod config_reload;
#[path = "main_tests/file_read.rs"]
mod file_read;
#[path = "main_tests/http_recovery.rs"]
mod http_recovery;
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

/// Write a shell init script for this platform's default shell into `dir`
/// that exports `name=value`, and return its path.
#[cfg(windows)]
fn write_init_script(dir: &Path, name: &str, value: &str) -> PathBuf {
    let init = dir.join("init.ps1");
    // PowerShell 5.1 decodes .ps1 files with the system ANSI code page unless
    // a UTF-8 BOM is present; the BOM keeps non-ASCII script bodies intact.
    let mut content = "\u{FEFF}".to_string();
    content.push_str(&format!("$env:{name} = '{}'\n", value.replace('\'', "''")));
    std::fs::write(&init, content).unwrap();
    init
}

#[cfg(not(windows))]
fn write_init_script(dir: &Path, name: &str, value: &str) -> PathBuf {
    let init = dir.join("init.sh");
    std::fs::write(&init, format!("export {name}={value}\n")).unwrap();
    init
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

#[test]
fn shell_config_default_preserves_sh_c_behavior() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let cwd = tmp.path().to_string_lossy().to_string();
    let result = run_shell(
        &cfg.policy,
        &ShellConfig::default(),
        Some(&cwd),
        &shell_echo("default-ok"),
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.stdout.as_deref(), Some("default-ok"));
}

#[test]
fn shell_config_default_shell_is_platform_native() {
    let shell = ShellConfig::default();
    #[cfg(windows)]
    {
        // The default Windows shell is native PowerShell; the default command
        // must execute without any sh/Git Bash/WSL on PATH.
        assert_eq!(shell.program, "powershell.exe");
        assert!(shell.args.iter().any(|arg| arg == "-Command"));
        assert!(
            shell.args.iter().any(|arg| arg == "-NoProfile"),
            "default must not load the user's interactive profile"
        );
        let result = run_shell(
            &unrestricted_test_policy(),
            &shell,
            None,
            &shell_echo("power-default-ok"),
            None,
            10,
            None,
        );
        assert_eq!(result.exit_code, Some(0), "{result:?}");
        assert_eq!(result.stdout.as_deref(), Some("power-default-ok"));
    }
    #[cfg(not(windows))]
    {
        assert_eq!(shell.program, "sh");
        assert_eq!(shell.args, vec!["-c".to_string()]);
    }
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

#[cfg(unix)]
#[test]
fn shell_config_path_prepend_discovers_fake_executable() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let bin_dir = tmp.path().join("bin");
    std::fs::create_dir(&bin_dir).unwrap();
    let exe = bin_dir.join("webcodex-fake-tool");
    std::fs::write(&exe, "#!/bin/sh\nprintf fake-tool-ok\n").unwrap();
    let mut perms = std::fs::metadata(&exe).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&exe, perms).unwrap();
    let shell = ShellConfig {
        path_prepend: vec![bin_dir],
        ..ShellConfig::default()
    };
    let cwd = tmp.path().to_string_lossy().to_string();
    let result = run_shell(
        &cfg.policy,
        &shell,
        Some(&cwd),
        "webcodex-fake-tool",
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.stdout.as_deref(), Some("fake-tool-ok"));
}

#[cfg(windows)]
#[test]
fn shell_config_path_prepend_discovers_fake_executable_and_keeps_windows_path() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let bin_dir = tmp.path().join("bin dir");
    std::fs::create_dir(&bin_dir).unwrap();
    // PowerShell executes .cmd files from PATH through cmd.exe; `<nul
    // set /p` writes exactly the payload with no trailing newline, and the
    // explicit `exit /b 0` keeps cmd's errorlevel (set /p with an empty
    // variable name leaves it 1) from leaking into $LASTEXITCODE.
    let exe = bin_dir.join("webcodex-fake-tool.cmd");
    std::fs::write(
        &exe,
        "@echo off\r\n<nul set /p \"=fake-tool-ok\"\r\nexit /b 0\r\n",
    )
    .unwrap();
    let shell = ShellConfig {
        path_prepend: vec![bin_dir],
        ..ShellConfig::default()
    };
    let cwd = tmp.path().to_string_lossy().to_string();
    let result = run_shell(
        &cfg.policy,
        &shell,
        Some(&cwd),
        "webcodex-fake-tool.cmd",
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert_eq!(result.stdout.as_deref(), Some("fake-tool-ok"), "{result:?}");

    // path_prepend must extend the inherited Windows PATH (spelled `Path` in
    // the process block), not replace it: the prepended directory comes
    // first and the System32 entries survive.
    let result = run_shell(
        &cfg.policy,
        &shell,
        Some(&cwd),
        &shell_env_var("PATH"),
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    let path = result.stdout.unwrap();
    let dir_text = tmp.path().join("bin dir").to_string_lossy().to_string();
    assert!(
        path.starts_with(&dir_text),
        "prepended directory is not first in PATH: {path}"
    );
    assert!(
        path.contains("System32"),
        "inherited Windows PATH lost: {path}"
    );
}

#[test]
fn shell_config_dialect_field_parses_and_validates() {
    use crate::webcodex_runner::config::ShellDialect;

    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
        &path,
        r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"

[policy]
allow_cwd_anywhere = true

[shell]
dialect = "powershell"

[shell.profiles.custom]
dialect = "posix"
program = "sh"
args = ["-c"]
"#,
    )
    .unwrap();
    let cfg = load_config(&path).unwrap();
    assert_eq!(cfg.shell.dialect, Some(ShellDialect::PowerShell));
    assert_eq!(
        cfg.shell.profiles.get("custom").unwrap().dialect,
        Some(ShellDialect::Posix)
    );

    // Unknown dialect values are rejected explicitly at parse time.
    std::fs::write(
        &path,
        r#"
server_url = "http://127.0.0.1:8000"
token = "test-token"
client_id = "agent-1"

[policy]
allow_cwd_anywhere = true

[shell]
dialect = "cmd"
"#,
    )
    .unwrap();
    let err = load_config(&path).unwrap_err();
    assert!(err.contains("powershell"), "{err}");
    assert!(err.contains("cmd"), "{err}");
}

#[test]
fn shell_config_default_environment_is_inherited() {
    let _guard = TEST_ENV_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let cwd = tmp.path().to_string_lossy().to_string();
    let _env = EnvGuard::new().set("WEBCODEX_INHERITED_TEST", "inherited-ok");
    let result = run_shell(
        &cfg.policy,
        &ShellConfig::default(),
        Some(&cwd),
        &shell_env_var("WEBCODEX_INHERITED_TEST"),
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert_eq!(result.stdout.as_deref(), Some("inherited-ok"));
}

#[test]
fn shell_config_env_values_are_available() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let shell = ShellConfig {
        env: HashMap::from([("WEBCODEX_TEST_VALUE".to_string(), "env-ok".to_string())]),
        ..ShellConfig::default()
    };
    let cwd = tmp.path().to_string_lossy().to_string();
    let result = run_shell(
        &cfg.policy,
        &shell,
        Some(&cwd),
        &shell_env_var("WEBCODEX_TEST_VALUE"),
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.stdout.as_deref(), Some("env-ok"));
}

#[test]
fn shell_config_init_script_is_sourced() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let init = write_init_script(tmp.path(), "WEBCODEX_INIT_TEST", "init-ok");
    let shell = ShellConfig {
        init_script: Some(init),
        ..ShellConfig::default()
    };
    let cwd = tmp.path().to_string_lossy().to_string();
    let result = run_shell(
        &cfg.policy,
        &shell,
        Some(&cwd),
        &shell_env_var("WEBCODEX_INIT_TEST"),
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.stdout.as_deref(), Some("init-ok"));
}

#[test]
fn shell_config_init_script_awkward_path_is_sourced() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    // Space, single quote, and non-ASCII characters in the init script path.
    let init_dir = tmp.path().join("init dir '脚本");
    std::fs::create_dir_all(&init_dir).unwrap();
    let init = write_init_script(&init_dir, "WEBCODEX_INIT_TEST", "init-ok");
    let shell = ShellConfig {
        init_script: Some(init),
        ..ShellConfig::default()
    };
    let cwd = tmp.path().to_string_lossy().to_string();
    let result = run_shell(
        &cfg.policy,
        &shell,
        Some(&cwd),
        &shell_env_var("WEBCODEX_INIT_TEST"),
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert_eq!(result.stdout.as_deref(), Some("init-ok"));
}

#[test]
fn shell_job_init_script_failure_blocks_command_and_reports_exit() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    // Windows: `exit 3` inside the dot-sourced script terminates the process
    // with 3. Unix: `false` fails the sourced script so `&&` short-circuits
    // with exit 1. Either way the requested command must never run.
    #[cfg(windows)]
    let (init_content, expected_exit) = ("exit 3\n", Some(3));
    #[cfg(not(windows))]
    let (init_content, expected_exit) = ("false\n", Some(1));
    let init = if cfg!(windows) {
        tmp.path().join("fail.ps1")
    } else {
        tmp.path().join("fail.sh")
    };
    std::fs::write(&init, init_content).unwrap();
    let shell = ShellConfig {
        init_script: Some(init),
        ..ShellConfig::default()
    };
    let cwd = tmp.path().to_string_lossy().to_string();
    let marker = tmp.path().join("command-ran.txt");
    let result = run_shell(
        &cfg.policy,
        &shell,
        Some(&cwd),
        &shell_write_file(&marker),
        None,
        10,
        None,
    );
    assert!(
        !marker.exists(),
        "command ran despite init script failure: {result:?}"
    );
    assert_eq!(result.exit_code, expected_exit, "{result:?}");
    assert!(result.error.is_none(), "{result:?}");
}

#[test]
fn shell_config_bash_like_args_are_respected_when_available() {
    if !Path::new("/bin/bash").exists() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let shell = ShellConfig {
        program: "/bin/bash".to_string(),
        args: vec!["-lc".to_string()],
        ..ShellConfig::default()
    };
    let cwd = tmp.path().to_string_lossy().to_string();
    let result = run_shell(
        &cfg.policy,
        &shell,
        Some(&cwd),
        "[[ 1 -eq 1 ]] && printf bash-ok",
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.stdout.as_deref(), Some("bash-ok"));
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

#[cfg(unix)]

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

#[cfg(windows)]
#[test]
fn shell_job_filters_sensitive_env_case_insensitive() {
    let _guard = TEST_ENV_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let cwd = tmp.path().to_string_lossy().to_string();
    // The plain (non-profile) path removes sensitive keys from the child
    // environment; Windows removal must be case-insensitive like the OS.
    for spelling in [
        "WEBCODEX_TOKEN",
        "WebCodex_User_Token",
        "Authorization",
        "webcodex_agent_token",
    ] {
        let _env = EnvGuard::new().set(spelling, "secret-token");
        let result = run_shell(
            &cfg.policy,
            &ShellConfig::default(),
            Some(&cwd),
            &shell_if_else_env_present(spelling),
            None,
            10,
            None,
        );
        assert_eq!(result.exit_code, Some(0), "{result:?}");
        assert_eq!(result.stdout.as_deref(), Some("absent"), "{result:?}");
    }

    // A configured shell env must not be able to re-insert a secret after the
    // inherited environment was scrubbed. Exercise canonical and mixed-case
    // spellings because Windows environment names are case-insensitive.
    for spelling in ["WEBCODEX_TOKEN", "WebCodex_User_Token", "authorization"] {
        let shell = ShellConfig {
            env: HashMap::from([(spelling.to_string(), "configured-secret".to_string())]),
            ..ShellConfig::default()
        };
        let result = run_shell(
            &cfg.policy,
            &shell,
            Some(&cwd),
            &shell_if_else_env_present(spelling),
            None,
            10,
            None,
        );
        assert_eq!(result.exit_code, Some(0), "{result:?}");
        assert_eq!(
            result.stdout.as_deref(),
            Some("absent"),
            "configured sensitive env leaked: {result:?}"
        );
    }
}

#[test]
fn shell_job_success_and_failure_results_are_structured() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let cwd = tmp.path().to_string_lossy().to_string();

    let success = run_shell(
        &cfg.policy,
        &cfg.shell,
        Some(&cwd),
        &format!("{}; {}", shell_echo("hello"), shell_echo_err("warn")),
        None,
        10,
        None,
    );
    assert_eq!(success.exit_code, Some(0));
    assert_eq!(success.stdout.as_deref(), Some("hello"));
    assert_eq!(success.stderr.as_deref(), Some("warn"));
    assert!(success.error.is_none());

    let failure = run_shell(
        &cfg.policy,
        &cfg.shell,
        Some(&cwd),
        "exit 7",
        None,
        10,
        None,
    );
    assert_eq!(failure.exit_code, Some(7));
    assert!(failure.error.is_none());
}

#[test]
fn shell_job_writes_stdin_to_child() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let cwd = tmp.path().to_string_lossy().to_string();

    let result = run_shell(
        &cfg.policy,
        &cfg.shell,
        Some(&cwd),
        &shell_stdin_cat(),
        Some("stdin payload\n"),
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.stdout.as_deref(), Some("stdin payload\n"));
    assert!(result.error.is_none());
}

#[cfg(unix)]
#[test]
fn shell_job_preserves_result_when_child_closes_stdin_early() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let cwd = tmp.path().to_string_lossy().to_string();
    // Larger than a pipe buffer, so write_all observes the closed reader
    // instead of winning the race by buffering the whole payload.
    let input = "unused payload\n".repeat(128 * 1024);

    let result = run_shell(
        &cfg.policy,
        &cfg.shell,
        Some(&cwd),
        "exec 0<&-; printf capability-unavailable; exit 23",
        Some(&input),
        10,
        None,
    );

    assert_eq!(result.exit_code, Some(23), "{result:?}");
    assert_eq!(result.stdout.as_deref(), Some("capability-unavailable"));
    assert!(result.error.is_none(), "{result:?}");
}

#[cfg(unix)]
#[test]
fn shell_job_rejects_cwd_symlink_escape() {
    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), project.path().join("outside")).unwrap();
    let policy = AgentPolicy {
        allow_cwd_anywhere: false,
        allowed_roots: vec![project.path().to_path_buf()],
        ..AgentPolicy::default()
    };

    let result = run_shell(
        &policy,
        &ShellConfig::default(),
        Some(project.path().join("outside").to_string_lossy().as_ref()),
        "pwd",
        None,
        10,
        None,
    );

    assert_eq!(result.exit_code, None);
    assert!(result
        .error
        .as_deref()
        .is_some_and(|error| error.contains("outside allowed_roots")));
}

#[test]
fn shell_job_timeout_returns_timeout_error() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let cwd = tmp.path().to_string_lossy().to_string();

    let result = run_shell(
        &cfg.policy,
        &cfg.shell,
        Some(&cwd),
        "sleep 2",
        None,
        1,
        None,
    );
    assert_eq!(result.exit_code, Some(-1));
    assert_eq!(result.error.as_deref(), Some("command timed out"));
    assert!(result
        .stderr
        .as_deref()
        .unwrap_or_default()
        .contains("command timed out after 1 seconds"));
}

#[cfg(unix)]
fn shell_quote_path(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

#[cfg(unix)]
fn long_lived_descendant_command(pid_file: &Path) -> String {
    format!(
        "sleep 60 & descendant=$!; printf '%s' \"$descendant\" > {}; wait",
        shell_quote_path(pid_file)
    )
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

#[cfg(unix)]
#[test]
fn shell_job_timeout_reaps_descendant_process_group() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let cwd = tmp.path().to_string_lossy().to_string();
    let pid_file = tmp.path().join("timeout-descendant.pid");

    let result = run_shell(
        &cfg.policy,
        &cfg.shell,
        Some(&cwd),
        &long_lived_descendant_command(&pid_file),
        None,
        1,
        None,
    );

    assert_eq!(result.exit_code, Some(-1), "{result:?}");
    assert_eq!(
        result.error.as_deref(),
        Some("command timed out"),
        "{result:?}"
    );
    assert!(
        result
            .stderr
            .as_deref()
            .unwrap_or_default()
            .contains("command timed out after 1 seconds"),
        "{result:?}"
    );
    assert_descendant_reaped(&pid_file);
}

#[cfg(unix)]
#[test]
fn shell_job_timeout_profile_reaps_descendant_process_group() {
    let tmp = tempfile::tempdir().unwrap();
    let shell = shell_with_profiles(Some("test"), vec![("test", ShellProfileConfig::default())]);
    let policy = unrestricted_test_policy();
    let cache = PreparedShellProfileCache::default();
    let cwd = tmp.path().to_string_lossy().to_string();
    let pid_file = tmp.path().join("profile-timeout-descendant.pid");

    // Exercise the production request path directly rather than the
    // test-only `run_shell` wrapper.
    let result = run_shell_with_profiles(
        1,
        &policy,
        &shell,
        tmp.path(),
        &cache,
        Some(&cwd),
        &long_lived_descendant_command(&pid_file),
        None,
        1,
        None,
    );

    assert_eq!(cache.len(), 1, "prepared profile path was not used");
    assert_eq!(result.exit_code, Some(-1), "{result:?}");
    assert_eq!(
        result.error.as_deref(),
        Some("command timed out"),
        "{result:?}"
    );
    assert_descendant_reaped(&pid_file);
}

#[cfg(windows)]
#[test]
fn shell_job_powershell_statement_error_is_nonzero() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let cwd = tmp.path().to_string_lossy().to_string();
    let result = run_shell(
        &cfg.policy,
        &cfg.shell,
        Some(&cwd),
        "Write-Error 'expected failure'",
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(1), "{result:?}");
    assert!(result.error.is_none(), "{result:?}");
}

#[cfg(windows)]
#[test]
fn shell_job_powershell_last_success_overrides_stale_native_exit() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let cwd = tmp.path().to_string_lossy().to_string();
    let result = run_shell(
        &cfg.policy,
        &cfg.shell,
        Some(&cwd),
        "cmd.exe /d /c exit 7; Write-Output 'final-ok'",
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert_eq!(
        result.stdout.as_deref().map(str::trim_end),
        Some("final-ok"),
        "{result:?}"
    );
}

#[test]
fn shell_job_stop_flag_is_best_effort() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let cwd = tmp.path().to_string_lossy().to_string();
    let stop_requested = AtomicBool::new(true);

    let result = run_shell(
        &cfg.policy,
        &cfg.shell,
        Some(&cwd),
        "sleep 2",
        None,
        10,
        Some(&stop_requested),
    );
    assert_eq!(result.exit_code, Some(-1));
    assert_eq!(result.error.as_deref(), Some("job stopped"));
    assert!(result
        .stderr
        .as_deref()
        .unwrap_or_default()
        .contains("job stopped by request"));
}

#[cfg(unix)]
#[test]
fn shell_job_stop_reaps_descendant_process_group() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let cwd = tmp.path().to_string_lossy().to_string();
    let pid_file = tmp.path().join("stop-descendant.pid");
    let stop_requested = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::clone(&stop_requested);
    let stop_pid_file = pid_file.clone();
    let stopper = std::thread::spawn(move || {
        let created = wait_until(Duration::from_secs(2), || stop_pid_file.exists());
        stop_flag.store(true, Ordering::SeqCst);
        created
    });

    let result = run_shell(
        &cfg.policy,
        &cfg.shell,
        Some(&cwd),
        &long_lived_descendant_command(&pid_file),
        None,
        10,
        Some(stop_requested.as_ref()),
    );

    assert!(stopper.join().expect("stopper thread panicked"));
    assert_eq!(result.exit_code, Some(-1), "{result:?}");
    assert_eq!(result.error.as_deref(), Some("job stopped"), "{result:?}");
    assert!(
        result
            .stderr
            .as_deref()
            .unwrap_or_default()
            .contains("job stopped by request"),
        "{result:?}"
    );
    assert_descendant_reaped(&pid_file);
}

#[test]
fn shell_job_stdout_stderr_are_bounded() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = test_config(tmp.path().join("config/projects.d"));
    cfg.policy.max_output_bytes = 8;
    let cwd = tmp.path().to_string_lossy().to_string();

    let result = run_shell(
        &cfg.policy,
        &cfg.shell,
        Some(&cwd),
        &format!(
            "{}; {}",
            shell_echo("0123456789"),
            shell_echo_err("abcdefghij")
        ),
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(0));
    let stdout = result.stdout.unwrap();
    let stderr = result.stderr.unwrap();
    assert_eq!(stdout.len(), 8);
    assert!(stdout.starts_with("[...]\n"), "{stdout:?}");
    assert_eq!(stderr.len(), 8);
    assert!(stderr.starts_with("[...]\n"), "{stderr:?}");
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

#[test]
fn shell_job_normal_success_preserves_output_and_exit_code() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().to_string_lossy().to_string();
    let helper = shell_tree_helper();
    let result = run_shell(
        &unrestricted_test_policy(),
        &shell_tree_test_shell(),
        Some(&cwd),
        &shell_tree_command(
            &helper,
            &["sleep".to_string(), "0".to_string(), "7".to_string()],
        ),
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, Some(7), "{result:?}");
    assert!(result.error.is_none(), "{result:?}");
    assert!(
        result
            .stdout
            .as_deref()
            .unwrap_or_default()
            .contains("VALIDATION_HELPER_STDOUT"),
        "{result:?}"
    );
    assert!(
        result
            .stderr
            .as_deref()
            .unwrap_or_default()
            .contains("VALIDATION_HELPER_STDERR"),
        "{result:?}"
    );
    assert!(
        result.duration_ms.unwrap_or(u64::MAX) < 30_000,
        "unbounded shell run: {result:?}"
    );
}

#[test]
fn shell_job_timeout_kills_whole_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().to_string_lossy().to_string();
    let helper = shell_tree_helper();
    let markers = ShellTreeMarkers::in_dir(tmp.path(), "timeout");
    let timeout_secs = shell_tree_test_timeout_secs();

    let result = run_shell(
        &unrestricted_test_policy(),
        &shell_tree_test_shell(),
        Some(&cwd),
        &markers.keepalive_command(&helper),
        None,
        timeout_secs,
        None,
    );
    assert!(
        wait_until_file(&markers.parent, Duration::from_secs(15)),
        "tree markers were never written: {result:?}"
    );
    assert_eq!(result.exit_code, Some(-1), "{result:?}");
    assert_eq!(
        result.error.as_deref(),
        Some("command timed out"),
        "{result:?}"
    );
    assert!(
        result
            .stderr
            .as_deref()
            .unwrap_or_default()
            .contains(&format!("command timed out after {timeout_secs} seconds")),
        "{result:?}"
    );
    markers.assert_tree_dead("timeout");
}

#[test]
fn shell_job_stop_kills_whole_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().to_string_lossy().to_string();
    let helper = shell_tree_helper();
    let markers = ShellTreeMarkers::in_dir(tmp.path(), "stop");
    let stop_requested = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::clone(&stop_requested);
    let stop_marker = markers.parent.clone();
    let stopper = std::thread::spawn(move || {
        let written = wait_until_file(&stop_marker, Duration::from_secs(15));
        stop_flag.store(true, Ordering::SeqCst);
        written
    });

    let result = run_shell(
        &unrestricted_test_policy(),
        &shell_tree_test_shell(),
        Some(&cwd),
        &markers.keepalive_command(&helper),
        None,
        60,
        Some(stop_requested.as_ref()),
    );

    assert!(stopper.join().expect("stopper thread panicked"));
    assert_eq!(result.exit_code, Some(-1), "{result:?}");
    assert_eq!(result.error.as_deref(), Some("job stopped"), "{result:?}");
    assert!(
        result
            .stderr
            .as_deref()
            .unwrap_or_default()
            .contains("job stopped by request"),
        "{result:?}"
    );
    markers.assert_tree_dead("stop");
}

#[test]
fn shell_job_parent_exit_first_descendant_holds_pipe() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().to_string_lossy().to_string();
    let helper = shell_tree_helper();
    let markers = ShellTreeMarkers::in_dir(tmp.path(), "orphan");

    // `spawn-descendant`: the helper spawns a sleeping descendant that
    // inherits the capture pipes, waits until it is provably alive, then
    // exits 0. The direct shell child therefore exits while the descendant
    // is still running and still holding the stdout/stderr write ends.
    let result = run_shell(
        &unrestricted_test_policy(),
        &shell_tree_test_shell(),
        Some(&cwd),
        &shell_tree_command(
            &helper,
            &[
                "spawn-descendant".to_string(),
                markers.parent.to_string_lossy().into_owned(),
                markers.alive.to_string_lossy().into_owned(),
                "120".to_string(),
            ],
        ),
        None,
        30,
        None,
    );

    // Direct-child exit code is preserved even though the descendant was
    // still alive at that point.
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert!(result.error.is_none(), "{result:?}");
    assert!(
        result
            .stdout
            .as_deref()
            .unwrap_or_default()
            .contains("DESCENDANT_PID="),
        "stdout did not reach EOF with the helper output: {result:?}"
    );
    // The descendant was sleeping (total 120s) when the direct child exited;
    // the whole-tree cleanup must terminate it instead of waiting for the
    // sleep to finish.
    let descendant = read_marker_pid(&markers.parent, "DESCENDANT_PID");
    assert!(
        wait_until_process_dead(descendant, Duration::from_secs(10), "orphan-descendant"),
        "descendant {descendant} survived whole-tree cleanup after direct child exit"
    );
    assert!(
        result.duration_ms.unwrap_or(u64::MAX) < 30_000,
        "runner waited for the descendant's natural sleep: {result:?}"
    );
}

#[cfg(unix)]
#[test]
fn shell_job_unix_graceful_sigterm_responsive_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().to_string_lossy().to_string();
    let helper = shell_tree_helper();

    // `sigterm-marker`: the helper installs a SIGTERM handler that writes
    // SIGTERM_HANDLED to the captured stdout. SIGKILL cannot be caught, so
    // the marker only appears when the graceful phase delivered SIGTERM and
    // the tree exited on its own — no force escalation required.
    let result = run_shell(
        &unrestricted_test_policy(),
        &ShellConfig::default(),
        Some(&cwd),
        &shell_tree_command(&helper, &["sigterm-marker".to_string(), "60".to_string()]),
        None,
        1,
        None,
    );
    assert_eq!(result.exit_code, Some(-1), "{result:?}");
    assert_eq!(
        result.error.as_deref(),
        Some("command timed out"),
        "{result:?}"
    );
    assert!(
        result
            .stdout
            .as_deref()
            .unwrap_or_default()
            .contains("SIGTERM_HANDLED"),
        "graceful SIGTERM handler did not run: {result:?}"
    );
}

#[cfg(unix)]
#[test]
fn shell_job_unix_sigterm_resistant_tree_escalates() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().to_string_lossy().to_string();
    let helper = shell_tree_helper();
    let markers = ShellTreeMarkers::in_dir(tmp.path(), "resist");

    // `ignore-term-keepalive`: the helper (and its descendant) ignore
    // SIGTERM, so the 50ms graceful phase cannot end the tree; only the
    // force escalation (SIGKILL) finishes it, within the cleanup deadline.
    let result = run_shell(
        &unrestricted_test_policy(),
        &ShellConfig::default(),
        Some(&cwd),
        &shell_tree_command(
            &helper,
            &[
                "ignore-term-keepalive".to_string(),
                markers.parent.to_string_lossy().into_owned(),
                markers.alive.to_string_lossy().into_owned(),
                "120".to_string(),
            ],
        ),
        None,
        1,
        None,
    );
    assert!(
        wait_until_file(&markers.parent, Duration::from_secs(15)),
        "tree markers were never written: {result:?}"
    );
    assert_eq!(result.exit_code, Some(-1), "{result:?}");
    assert_eq!(
        result.error.as_deref(),
        Some("command timed out"),
        "{result:?}"
    );
    markers.assert_tree_dead("resist");
}

#[test]
fn shell_job_repeated_stop_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().to_string_lossy().to_string();
    let helper = shell_tree_helper();
    let stop_requested = Arc::new(AtomicBool::new(false));

    let first = ShellTreeMarkers::in_dir(tmp.path(), "repeat-1");
    let stop_flag = Arc::clone(&stop_requested);
    let stop_marker = first.parent.clone();
    let stopper = std::thread::spawn(move || {
        let written = wait_until_file(&stop_marker, Duration::from_secs(15));
        stop_flag.store(true, Ordering::SeqCst);
        written
    });
    let result = run_shell(
        &unrestricted_test_policy(),
        &shell_tree_test_shell(),
        Some(&cwd),
        &first.keepalive_command(&helper),
        None,
        60,
        Some(stop_requested.as_ref()),
    );
    assert!(stopper.join().expect("stopper thread panicked"));
    assert_eq!(result.exit_code, Some(-1), "{result:?}");
    assert_eq!(result.error.as_deref(), Some("job stopped"), "{result:?}");
    first.assert_tree_dead("repeat-1");

    // A second run against the same already-set flag must stop promptly and
    // clean up its own freshly spawned tree without a panic or deadlock.
    let second = ShellTreeMarkers::in_dir(tmp.path(), "repeat-2");
    let result = run_shell(
        &unrestricted_test_policy(),
        &shell_tree_test_shell(),
        Some(&cwd),
        &second.keepalive_command(&helper),
        None,
        60,
        Some(stop_requested.as_ref()),
    );
    assert_eq!(result.exit_code, Some(-1), "{result:?}");
    assert_eq!(result.error.as_deref(), Some("job stopped"), "{result:?}");
    if wait_until_file(&second.parent, Duration::from_secs(5)) {
        // If the tree got far enough to write markers, it must be dead.
        second.assert_tree_dead("repeat-2");
    }
}

#[test]
fn shell_job_timeout_racing_stop_is_bounded() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().to_string_lossy().to_string();
    let helper = shell_tree_helper();
    let markers = ShellTreeMarkers::in_dir(tmp.path(), "race");
    let timeout_secs = shell_tree_test_timeout_secs();
    let stop_requested = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::clone(&stop_requested);
    let stop_marker = markers.parent.clone();
    let stopper = std::thread::spawn(move || {
        let written = wait_until_file(&stop_marker, Duration::from_secs(15));
        // Set the stop flag shortly before the timeout would fire; either
        // outcome (stop or timeout) is legitimate for the shell API.
        let delay = if cfg!(windows) { 3600 } else { 600 };
        std::thread::sleep(Duration::from_millis(delay));
        stop_flag.store(true, Ordering::SeqCst);
        written
    });

    let result = run_shell(
        &unrestricted_test_policy(),
        &shell_tree_test_shell(),
        Some(&cwd),
        &markers.keepalive_command(&helper),
        None,
        timeout_secs,
        Some(stop_requested.as_ref()),
    );

    assert!(stopper.join().expect("stopper thread panicked"));
    assert_eq!(result.exit_code, Some(-1), "{result:?}");
    assert!(
        matches!(
            result.error.as_deref(),
            Some("command timed out" | "job stopped")
        ),
        "unexpected race outcome: {result:?}"
    );
    markers.assert_tree_dead("race");
}

#[test]
fn shell_job_spawn_failure_preserves_error() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("no-such-shell-program");
    let shell = ShellConfig {
        program: missing.to_string_lossy().into_owned(),
        ..ShellConfig::default()
    };
    let result = run_shell(
        &unrestricted_test_policy(),
        &shell,
        None,
        "true",
        None,
        10,
        None,
    );
    assert_eq!(result.exit_code, None, "{result:?}");
    let error = result.error.as_deref().unwrap_or_default();
    assert!(
        error.starts_with("failed to spawn command: "),
        "spawn error semantics changed: {result:?}"
    );
}

#[cfg(unix)]
#[test]
fn shell_job_profile_prepare_stop_reaps_whole_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let pid_file = tmp.path().join("prepare.pid");
    let started_marker = tmp.path().join("prepare-started.txt");
    let init_script = format!(
        "echo $$ > {}; : > {}; sleep 60",
        shell_tree_quote(&pid_file.to_string_lossy()),
        shell_tree_quote(&started_marker.to_string_lossy())
    );
    let shell = shell_with_profiles(
        Some("test"),
        vec![(
            "test",
            ShellProfileConfig {
                init_script: Some(init_script),
                ..ShellProfileConfig::default()
            },
        )],
    );
    let policy = unrestricted_test_policy();
    let cache = PreparedShellProfileCache::default();
    let cwd = tmp.path().to_string_lossy().to_string();
    let stop_requested = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::clone(&stop_requested);
    let stop_marker = started_marker.clone();
    let stopper = std::thread::spawn(move || {
        let written = wait_until_file(&stop_marker, Duration::from_secs(15));
        stop_flag.store(true, Ordering::SeqCst);
        written
    });

    let result = run_shell_with_profiles(
        1,
        &policy,
        &shell,
        tmp.path(),
        &cache,
        Some(&cwd),
        "true",
        None,
        10,
        Some(stop_requested.as_ref()),
    );

    assert!(stopper.join().expect("stopper thread panicked"));
    assert!(
        result
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("profile prepare stopped during runner shutdown"),
        "{result:?}"
    );
    let prepare_pid = std::fs::read_to_string(&pid_file)
        .expect("read prepare pid")
        .trim()
        .parse::<u32>()
        .expect("parse prepare pid");
    assert!(
        wait_until_process_dead(prepare_pid, Duration::from_secs(10), "prepare"),
        "profile prepare tree survived stop"
    );
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
    let _guard = TEST_ENV_LOCK.lock().unwrap();
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
    let saved = std::env::var_os("WEBCODEX_UNICODE_ENV");
    std::env::set_var("WEBCODEX_UNICODE_ENV", "值 测试");
    let result = run_shell(
        &cfg.policy,
        &ShellConfig::default(),
        Some(&cwd),
        &shell_env_var("WEBCODEX_UNICODE_ENV"),
        None,
        10,
        None,
    );
    match saved {
        Some(value) => std::env::set_var("WEBCODEX_UNICODE_ENV", value),
        None => std::env::remove_var("WEBCODEX_UNICODE_ENV"),
    }
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

#[cfg(windows)]
#[test]
fn shell_profile_prepare_timeout_cleans_up_whole_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let helper = shell_tree_helper();
    let markers = ShellTreeMarkers::in_dir(tmp.path(), "prepare-timeout");
    // The init snippet hangs (helper keepalive plus an infinite loop), with a
    // descendant holding the capture pipes; the 30s prepare timeout must
    // terminate the whole tree.
    let init_script = format!(
        "{};\nwhile ($true) {{ Start-Sleep -Seconds 1 }}",
        markers.keepalive_command(&helper)
    );
    let shell = shell_with_profiles(
        Some("test"),
        vec![(
            "test",
            ShellProfileConfig {
                init_script: Some(init_script),
                ..ShellProfileConfig::default()
            },
        )],
    );
    let start = Instant::now();
    let result = run_shell_with_profiles(
        1,
        &unrestricted_test_policy(),
        &shell,
        tmp.path(),
        &PreparedShellProfileCache::default(),
        Some(tmp.path().to_string_lossy().as_ref()),
        "true",
        None,
        10,
        None,
    );
    assert!(
        wait_until_file(&markers.parent, Duration::from_secs(15)),
        "prepare tree markers were never written: {result:?}"
    );
    let err = result.error.expect("prepare should time out");
    assert!(
        err.contains("profile prepare timed out after 30 seconds"),
        "{err}"
    );
    assert!(
        start.elapsed() >= Duration::from_secs(29),
        "prepare timed out too early: {:?}",
        start.elapsed()
    );
    markers.assert_tree_dead("prepare-timeout");
}

#[cfg(windows)]
#[test]
fn shell_profile_prepare_stop_cleans_up_whole_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let helper = shell_tree_helper();
    let markers = ShellTreeMarkers::in_dir(tmp.path(), "prepare-stop");
    let pid_file = tmp.path().join("prepare-stop.pid");
    // Write the prepare process pid, hang the direct prepare process, and
    // keep a descendant holding the capture pipes. The helper writes its own
    // parent marker after spawning the descendant.
    let init_script = format!(
        "[IO.File]::WriteAllText({}, [string]$PID); \
         {}; \
         while ($true) {{ Start-Sleep -Seconds 1 }}",
        shell_tree_quote(&pid_file.to_string_lossy()),
        markers.keepalive_command(&helper)
    );
    let shell = shell_with_profiles(
        Some("test"),
        vec![(
            "test",
            ShellProfileConfig {
                init_script: Some(init_script),
                ..ShellProfileConfig::default()
            },
        )],
    );
    let stop_requested = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::clone(&stop_requested);
    // Wait for the helper's parent marker (written after the descendant is
    // spawned and the pid file exists) so the stop lands after every marker
    // this test asserts on is on disk.
    let stop_marker = markers.parent.clone();
    let stopper = std::thread::spawn(move || {
        let written = wait_until_file(&stop_marker, Duration::from_secs(15));
        stop_flag.store(true, Ordering::SeqCst);
        written
    });

    let result = run_shell_with_profiles(
        1,
        &unrestricted_test_policy(),
        &shell,
        tmp.path(),
        &PreparedShellProfileCache::default(),
        Some(tmp.path().to_string_lossy().as_ref()),
        "true",
        None,
        10,
        Some(stop_requested.as_ref()),
    );

    assert!(stopper.join().expect("stopper thread panicked"));
    assert!(
        result
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("profile prepare stopped during runner shutdown"),
        "{result:?}"
    );
    let prepare_pid = std::fs::read_to_string(&pid_file)
        .expect("read prepare pid")
        .trim()
        .parse::<u32>()
        .expect("parse prepare pid");
    assert!(
        wait_until_process_dead(prepare_pid, Duration::from_secs(10), "prepare"),
        "profile prepare process survived stop"
    );
    markers.assert_tree_dead("prepare-stop");
}

#[test]
fn computer_register_request_announces_platform_capability_and_protocol_version() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = test_config(tmp.path().join("config/projects.d"));
    // A stale or hand-edited config cannot force capability advertisement:
    // registration replaces it with the result of the real host probe.
    cfg.capabilities = Some(ShellClientCapabilities {
        sandbox_inspect_commands: true,
        computer_observe: true,
        computer_snapshot_region: true,
        computer_accessibility_observe: true,
        computer_element_state: true,
        computer_control: true,
        computer_scroll_to_element: true,
        computer_key_input: true,
        computer_window_activate: true,
        computer_text_input: true,
        project_lifecycle: false,
        project_path_registration: false,
        ..Default::default()
    });
    for (version, expected_str) in [
        (AGENT_PROTOCOL_VERSION_POLLING_V1, "polling-v1"),
        (AGENT_PROTOCOL_VERSION_WEBSOCKET_V1, "websocket-v1"),
        (AGENT_PROTOCOL_VERSION_QUIC_V1, "quic-v1"),
    ] {
        let body = build_register_request(&cfg, Vec::new(), version, "inst-1", 0);
        let caps = body.capabilities.as_ref().expect("transport capabilities");
        assert!(caps.structured_go_test_tool, "{expected_str}");
        assert!(caps.structured_go_test_packages, "{expected_str}");
        assert!(caps.structured_file_delete, "{expected_str}");
        assert_eq!(body.agent_instance_id, "inst-1");
        assert_eq!(
            body.agent_protocol_version.as_deref(),
            Some(version),
            "version mismatch for {expected_str}"
        );
        assert_eq!(body.agent_protocol_version.as_deref(), Some(expected_str));
    }
    // Also verify capabilities are advertised (check once for polling).
    let body = build_register_request(
        &cfg,
        Vec::new(),
        AGENT_PROTOCOL_VERSION_POLLING_V1,
        "inst-1",
        0,
    );
    let caps = body.capabilities.expect("agent registers capabilities");
    assert!(caps.shell);
    assert!(caps.file_read);
    assert!(caps.file_write);
    assert!(caps.artifact_export_chunk_read);
    assert!(caps.artifact_export_streaming_metadata);
    assert!(caps.structured_file_delete);
    assert!(caps.async_jobs);
    assert!(caps.async_shell_jobs);
    assert!(caps.structured_validation_argv);
    assert!(caps.structured_go_test_json);
    assert!(caps.structured_go_test_tool);
    assert!(caps.structured_go_test_packages);
    assert!(caps.structured_process_argv);
    assert!(caps.structured_script_payload);
    assert!(caps.internal_posix_script);
    assert!(caps.structured_execution_jobs);
    assert!(caps.lsp_read_only_navigation);
    assert!(caps.lsp_call_hierarchy);
    assert!(caps.project_lifecycle);
    assert!(caps.project_path_registration);
    assert_eq!(
        caps.computer_observe,
        cfg!(any(target_os = "macos", windows)),
        "computer observation is advertised only when this Runner binary has a supported native implementation"
    );
    assert_eq!(
        caps.computer_snapshot_region,
        cfg!(any(target_os = "macos", windows)),
        "computer region snapshot is independently advertised only when native window capture is supported"
    );
    assert_eq!(
        caps.computer_accessibility_observe,
        cfg!(any(target_os = "macos", windows)),
        "computer accessibility observation is advertised only by native AX/UIA implementations"
    );
    assert_eq!(
        caps.computer_element_state,
        cfg!(any(target_os = "macos", windows)),
        "computer element state is independently advertised only by native AX/UIA implementations"
    );
    assert_eq!(
        caps.computer_control,
        cfg!(any(target_os = "macos", windows)),
        "computer control is independently advertised only by native macOS/Windows implementations"
    );
    assert_eq!(
        caps.computer_scroll_to_element,
        cfg!(any(target_os = "macos", windows)),
        "computer scroll-to-element is independently advertised only by native macOS/Windows implementations"
    );
    assert_eq!(
        caps.computer_key_input,
        cfg!(any(target_os = "macos", windows)),
        "computer key input is independently advertised only by native macOS/Windows implementations"
    );
    assert_eq!(
        caps.computer_window_activate,
        cfg!(any(target_os = "macos", windows)),
        "computer window activation is independently advertised only by native macOS/Windows implementations"
    );
    assert_eq!(
        caps.computer_text_input,
        cfg!(any(target_os = "macos", windows)),
        "computer text input is independently advertised only by native macOS/Windows implementations"
    );
    assert_eq!(
        caps.sandbox_inspect_commands,
        crate::command_sandbox::inspect_sandbox_available().is_ok()
    );
}

#[test]
fn phase_e2_register_request_reports_effective_job_concurrency_limit() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = test_config(tmp.path().join("config/projects.d"));
    for (configured, expected) in [
        (None, 4),
        (Some(0), 1),
        (Some(1), 1),
        (Some(8), 8),
        (Some(64), 64),
        (Some(65), 64),
        (Some(128), 64),
    ] {
        cfg.max_concurrent_jobs = configured;
        for protocol in [
            AGENT_PROTOCOL_VERSION_POLLING_V1,
            AGENT_PROTOCOL_VERSION_WEBSOCKET_V1,
            AGENT_PROTOCOL_VERSION_QUIC_V1,
        ] {
            let body = build_register_request(&cfg, Vec::new(), protocol, "inst-limit", 0);
            assert_eq!(
                body.job_concurrency_limit,
                Some(expected),
                "configured={configured:?} protocol={protocol}"
            );
        }
    }
}

#[test]
fn register_request_carries_sanitized_shell_profiles_summary() {
    // A config with one profile carrying a secret env value and a secret
    // init_script body. The sanitized summary must report the profile name,
    // has_init_script=true, and env_keys_count, but MUST NOT include the env
    // value or the init_script body.
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = test_config(tmp.path().join("config/projects.d"));
    let secret_env = "DO_NOT_LEAK_THIS_ENV_VALUE";
    let secret_script = "DO_NOT_LEAK_THIS_INIT_SCRIPT_BODY";
    cfg.shell = shell_with_profiles(
        Some("rust"),
        vec![(
            "rust",
            ShellProfileConfig {
                program: Some("sh".to_string()),
                args: Some(vec!["-c".to_string()]),
                env: profile_env(&[("SECRET_KEY", secret_env)]),
                init_script: Some(secret_script.to_string()),
                ..ShellProfileConfig::default()
            },
        )],
    );
    let body = build_register_request(
        &cfg,
        Vec::new(),
        AGENT_PROTOCOL_VERSION_POLLING_V1,
        "inst-1",
        0,
    );
    let policy = body.policy.expect("agent registers a policy");
    let summary = policy
        .shell_profiles
        .as_ref()
        .expect("sanitized shell profiles summary is present");
    assert_eq!(summary.default_profile.as_deref(), Some("rust"));
    assert_eq!(summary.configured_count, 1);
    assert_eq!(summary.profiles.len(), 1);
    let entry = &summary.profiles[0];
    assert_eq!(entry.name, "rust");
    assert!(entry.has_init_script);
    assert_eq!(entry.env_keys_count, 1);
    assert_eq!(entry.program, "sh");
    assert_eq!(entry.args_count, 1);
    // Sanitization: the rendered summary never carries env values or the
    // init_script body.
    let rendered = serde_json::to_string(summary).unwrap();
    assert!(!rendered.contains(secret_env), "{rendered}");
    assert!(!rendered.contains(secret_script), "{rendered}");
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

#[test]
fn sink_submit_result_sends_result_envelope() {
    type SinkFactory = fn(&str) -> (AgentSink, tokio::sync::mpsc::Receiver<AgentEnvelope>);
    for (label, make_sink, expected_client, expected_instance) in [
        ("ws", ws_sink as SinkFactory, "ws-client", "ws-inst"),
        ("quic", quic_sink as SinkFactory, "quic-client", "quic-inst"),
    ] {
        let (sink, mut rx) = make_sink(expected_client);
        let result = CommandResult {
            exit_code: Some(0),
            stdout: Some("hi".to_string()),
            stderr: Some(String::new()),
            duration_ms: Some(3),
            error: None,
        };
        assert_eq!(
            sink.submit_result("req-9".to_string(), result).unwrap(),
            webcodex_runner::ResultSubmission::Accepted,
            "{label}"
        );
        let env = rx.try_recv().expect("envelope was sent");
        match env {
            AgentEnvelope::Result { payload } => {
                assert_eq!(payload.result.client_id, expected_client, "{label}");
                assert_eq!(
                    payload.result.agent_instance_id, expected_instance,
                    "{label}"
                );
                assert_eq!(payload.result.request_id, "req-9");
                assert_eq!(payload.result.exit_code, Some(0));
                assert_eq!(payload.result.stdout.as_deref(), Some("hi"));
                assert_eq!(payload.command_execution_state, None);
            }
            other => panic!("{label}: expected result, got {:?}", other.kind()),
        }
    }
}

#[test]
fn sink_send_job_update_sends_job_update_envelope() {
    type SinkFactory = fn(&str) -> (AgentSink, tokio::sync::mpsc::Receiver<AgentEnvelope>);
    for (label, make_sink, expected_client) in [
        ("ws", ws_sink as SinkFactory, "ws-client"),
        ("quic", quic_sink as SinkFactory, "quic-client"),
    ] {
        let (sink, mut rx) = make_sink(expected_client);
        let body = ShellAgentJobUpdateRequest {
            client_id: expected_client.to_string(),
            agent_instance_id: sink.agent_instance_id().to_string(),
            job_id: "job-1".to_string(),
            request_id: Some("req-1".to_string()),
            update_seq: None,
            status: "running".to_string(),
            stdout_chunk: Some(format!("{label}-chunk")),
            stderr_chunk: None,
            stdout_tail: None,
            stderr_tail: None,
            log_snapshot: None,
            exit_code: None,
            duration_ms: None,
            error: None,
            command_execution_state: None,
            validation_progress: None,
            finished: false,
        };
        sink.send_job_update(&body).unwrap();
        let env = rx.try_recv().expect("envelope was sent");
        match env {
            AgentEnvelope::JobUpdate { payload } => {
                assert_eq!(payload.client_id, expected_client, "{label}");
                assert_eq!(
                    payload.agent_instance_id,
                    sink.agent_instance_id(),
                    "{label}"
                );
                assert_eq!(payload.job_id, "job-1", "{label}");
                assert_eq!(payload.status, "running", "{label}");
                assert_eq!(
                    payload.stdout_chunk.as_deref(),
                    Some(format!("{label}-chunk").as_str()),
                    "{label}"
                );
            }
            other => panic!("{label}: expected job_update, got {:?}", other.kind()),
        }
    }
}

#[test]
fn sink_try_send_job_update_preserves_full_ws_and_quic_queue_for_retry() {
    for label in ["ws", "quic"] {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        tx.try_send(AgentEnvelope::Ping { ts: 11 }).unwrap();
        let sink = match label {
            "ws" => AgentSink::WebSocket {
                tx,
                client_id: "stream-client".to_string(),
                agent_instance_id: "stream-instance".to_string(),
            },
            "quic" => AgentSink::Quic {
                tx,
                client_id: "stream-client".to_string(),
                agent_instance_id: "stream-instance".to_string(),
            },
            _ => unreachable!(),
        };
        let body = ShellAgentJobUpdateRequest {
            client_id: "stream-client".to_string(),
            agent_instance_id: "stream-instance".to_string(),
            job_id: "job-full".to_string(),
            request_id: Some("request-full".to_string()),
            update_seq: Some(2),
            status: "running".to_string(),
            stdout_chunk: None,
            stderr_chunk: None,
            stdout_tail: None,
            stderr_tail: None,
            log_snapshot: None,
            exit_code: None,
            duration_ms: None,
            error: None,
            command_execution_state: None,
            validation_progress: None,
            finished: false,
        };

        assert_eq!(sink.try_send_job_update(&body), Ok(false), "{label}");
        assert!(matches!(rx.try_recv(), Ok(AgentEnvelope::Ping { ts: 11 })));
        assert_eq!(sink.try_send_job_update(&body), Ok(true), "{label}");
        assert!(matches!(
            rx.try_recv(),
            Ok(AgentEnvelope::JobUpdate { payload }) if payload.job_id == "job-full"
        ));
    }
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

#[test]
fn file_request_kind_includes_edit_and_basic_ops() {
    for kind in [
        "file_read",
        "file_write",
        "file_list",
        "file_project_overview",
        "file_delete_project_files",
        "file_write_project_file",
        "file_apply_text_edits",
    ] {
        assert!(
            is_file_request_kind(kind),
            "{kind} should route to file handler"
        );
    }
    // Removed legacy edit request kinds no longer route to the file handler.
    for kind in [
        "file_replace_line_range",
        "file_insert_at_line",
        "file_delete_line_range",
        "file_replace_exact_block",
        "file_insert_before_pattern",
        "file_insert_after_pattern",
        "file_replace_in_file",
    ] {
        assert!(
            !is_file_request_kind(kind),
            "{kind} must no longer be a file request kind"
        );
    }
    assert!(!is_file_request_kind("run_shell"));
}

#[test]
fn project_overview_agent_request_returns_metadata_without_contents() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    std::fs::write(tmp.path().join("Cargo.toml"), "private manifest content").unwrap();
    std::fs::write(tmp.path().join("README.md"), "private readme content").unwrap();
    std::fs::write(tmp.path().join(".env"), "TOKEN=not-returned").unwrap();
    let request = json_file_op_request(
        tmp.path(),
        "file_project_overview",
        ".",
        serde_json::json!({"max_depth": 2, "limit": 200}),
    );

    let output = line_edit_json(handle_file_request(&policy, &request));
    assert_eq!(output["schema_version"], 1);
    assert_eq!(output["deterministic"], true);
    assert!(output.to_string().contains("Cargo.toml"));
    assert!(!output.to_string().contains("private manifest content"));
    assert!(!output.to_string().contains("TOKEN=not-returned"));
    assert!(!output.to_string().contains(".env"));
    assert!(!output
        .to_string()
        .contains(&tmp.path().display().to_string()));
}

#[test]
fn dispatch_request_edit_routes_to_file_handler() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let cwd = tmp.path().to_string_lossy().to_string();
    let (sink, mut rx) = ws_sink("ws-client");
    let jobs = JobManager::new(max_concurrent_jobs(&cfg));
    let request = ShellAgentShellRequest {
        request_id: "req-edit".to_string(),
        client_id: "ws-client".to_string(),
        kind: "file_write_project_file".to_string(),
        job_id: None,
        cwd: Some(cwd),
        path: Some("new.txt".to_string()),
        content: Some(
            serde_json::json!({
                "path": "new.txt",
                "content": "new content\n",
                "overwrite": false,
            })
            .to_string(),
        ),
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
        timeout_secs: 10,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        sandbox: None,
        job_context: None,
        persistent_shell: None,
    };
    let pdir = projects_dir(&cfg).unwrap();
    let lsp = webcodex_runner::LspSupervisor::default();
    let hot = runtime_config(&cfg);
    let persistent_shells = webcodex_runner::PersistentShellManager::new(
        &cfg.shell,
        webcodex_runner::SshConnectionPool::default(),
    );
    let ran = dispatch_request(
        &sink,
        &hot.snapshot(),
        &hot,
        &jobs,
        &persistent_shells,
        &pdir,
        &lsp,
        request,
    )
    .unwrap();
    assert!(ran);
    let env = rx.try_recv().expect("result envelope was sent");
    match env {
        AgentEnvelope::Result { payload } => {
            assert_eq!(payload.result.request_id, "req-edit");
            assert_eq!(payload.result.exit_code, Some(0));
            let stdout = payload
                .result
                .stdout
                .expect("file handler returns JSON stdout");
            assert!(stdout.contains("\"created\":true"), "stdout was {stdout}");
            assert_eq!(
                std::fs::read_to_string(tmp.path().join("new.txt")).unwrap(),
                "new content\n"
            );
        }
        other => panic!("expected result, got {:?}", other.kind()),
    }
}

#[test]
fn dispatch_request_rejects_unsupported_file_kinds_without_starting_command() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let jobs = JobManager::new(max_concurrent_jobs(&cfg));
    let pdir = projects_dir(&cfg).unwrap();
    let hot = runtime_config(&cfg);
    let persistent_shells = webcodex_runner::PersistentShellManager::new(
        &cfg.shell,
        webcodex_runner::SshConnectionPool::default(),
    );
    let kinds = [
        "file_replace_line_range",
        "file_insert_at_line",
        "file_delete_line_range",
        "file_replace_exact_block",
        "file_insert_before_pattern",
        "file_insert_after_pattern",
        "file_replace_in_file",
        "file_future_unknown_operation",
    ];

    for (index, kind) in kinds.into_iter().enumerate() {
        let marker_name = format!("unsupported-file-marker-{index}");
        let target_name = format!("unsupported-file-target-{index}.txt");
        let marker = tmp.path().join(&marker_name);
        let target = tmp.path().join(&target_name);
        std::fs::write(&target, "original\n").unwrap();
        let command = format!(
            "printf shell-ran > {marker_name}; printf modified > {target_name}; printf shell-stdout"
        );
        let request: ShellAgentShellRequest = serde_json::from_value(serde_json::json!({
            "request_id": format!("req-unsupported-file-{index}"),
            "client_id": "ws-client",
            "kind": kind,
            "cwd": tmp.path().to_string_lossy(),
            "path": target_name,
            "content": "replacement",
            "command": command,
            "old_text": "old",
            "pattern": "needle",
            "line": 10,
            "timeout_secs": 10,
            "requested_by": "tester",
            "created_at": 0,
        }))
        .unwrap();
        let (sink, mut rx) = ws_sink("ws-client");

        let ran = dispatch_request(
            &sink,
            &hot.snapshot(),
            &hot,
            &jobs,
            &persistent_shells,
            &pdir,
            &webcodex_runner::LspSupervisor::default(),
            request,
        )
        .unwrap();

        assert!(ran, "{kind}");
        let env = rx.try_recv().expect("result envelope was sent");
        match env {
            AgentEnvelope::Result { payload } => {
                assert_eq!(payload.result.exit_code, None, "{kind}");
                assert_eq!(payload.result.stdout, None, "{kind}");
                assert_eq!(
                    payload.result.error.as_deref(),
                    Some(
                        "unsupported_file_request_kind: unsupported file request kind; command was not started"
                    ),
                    "{kind}"
                );
            }
            other => panic!("{kind}: expected result, got {:?}", other.kind()),
        }
        assert!(!marker.exists(), "{kind}: shell marker was created");
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "original\n",
            "{kind}: target file was modified"
        );
    }
}

#[test]
fn dispatch_request_run_shell_sends_result_over_sink() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let jobs = JobManager::new(max_concurrent_jobs(&cfg));
    let pdir = projects_dir(&cfg).unwrap();
    let hot = runtime_config(&cfg);
    let persistent_shells = webcodex_runner::PersistentShellManager::new(
        &cfg.shell,
        webcodex_runner::SshConnectionPool::default(),
    );

    type SinkFactory = fn(&str) -> (AgentSink, tokio::sync::mpsc::Receiver<AgentEnvelope>);
    for (label, make_sink, client_id, expected_stdout) in [
        ("ws", ws_sink as SinkFactory, "ws-client", "wsok"),
        ("quic", quic_sink as SinkFactory, "quic-client", "quic-ok"),
    ] {
        let (sink, mut rx) = make_sink(client_id);
        let request = ShellAgentShellRequest {
            request_id: format!("req-{label}"),
            client_id: client_id.to_string(),
            kind: "run_shell".to_string(),
            job_id: None,
            cwd: Some(tmp.path().to_string_lossy().to_string()),
            path: None,
            content: None,
            max_bytes: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            create_dirs: false,
            command: shell_echo(expected_stdout),
            process: None,
            script: None,
            stdin: None,
            timeout_secs: 10,
            requested_by: "tester".to_string(),
            created_at: 0,
            validation: None,
            lsp: None,
            sandbox: None,
            job_context: None,
            persistent_shell: None,
        };
        let ran = dispatch_request(
            &sink,
            &hot.snapshot(),
            &hot,
            &jobs,
            &persistent_shells,
            &pdir,
            &webcodex_runner::LspSupervisor::default(),
            request,
        )
        .unwrap();
        assert!(ran, "{label}");
        let env = rx.try_recv().expect("result envelope was sent");
        match env {
            AgentEnvelope::Result { payload } => {
                assert_eq!(payload.result.request_id, format!("req-{label}"));
                assert_eq!(payload.result.exit_code, Some(0));
                assert_eq!(payload.result.stdout.as_deref(), Some(expected_stdout));
                assert_eq!(
                    payload.command_execution_state,
                    Some(ShellCommandExecutionState::Completed)
                );
            }
            other => panic!("{label}: expected result, got {:?}", other.kind()),
        }
    }
}

#[test]
fn dispatch_request_internal_search_uses_posix_runtime_not_configured_shell_parser() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = test_config(tmp.path().join("config/projects.d"));
    // The generated search program is POSIX shell. It must not inherit an
    // arbitrary configured shell parser (PowerShell is the Windows production
    // case); use a guaranteed-failing program here to prove the bypass.
    cfg.shell.program = if cfg!(windows) {
        "powershell".to_string()
    } else {
        "/bin/false".to_string()
    };
    let jobs = JobManager::new(max_concurrent_jobs(&cfg));
    let pdir = projects_dir(&cfg).unwrap();
    let hot = runtime_config(&cfg);
    let persistent_shells = webcodex_runner::PersistentShellManager::new(
        &cfg.shell,
        webcodex_runner::SshConnectionPool::default(),
    );
    let (sink, mut rx) = ws_sink("ws-client");
    let marker = r#"{"webcodex_search":{"backend":"grep","feature_unavailable":false}}"#;
    let request = ShellAgentShellRequest {
        request_id: "req-internal-search".to_string(),
        client_id: "ws-client".to_string(),
        kind: "run_shell".to_string(),
        job_id: None,
        cwd: Some(tmp.path().to_string_lossy().to_string()),
        path: None,
        content: None,
        max_bytes: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        create_dirs: false,
        command: format!(
            "{}\nprintf '%s\\n' '{}'\n",
            shell_protocol::EXTERNAL_SEARCH_REQUEST_PREFIX,
            marker
        ),
        process: None,
        script: None,
        stdin: Some("{}".to_string()),
        timeout_secs: if cfg!(windows) { 30 } else { 10 },
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        sandbox: None,
        job_context: None,
        persistent_shell: None,
    };

    assert!(dispatch_request(
        &sink,
        &hot.snapshot(),
        &hot,
        &jobs,
        &persistent_shells,
        &pdir,
        &webcodex_runner::LspSupervisor::default(),
        request,
    )
    .unwrap());
    match rx.try_recv().expect("internal search result") {
        AgentEnvelope::Result { payload } => {
            assert_eq!(payload.result.exit_code, Some(0));
            assert!(payload
                .result
                .stdout
                .as_deref()
                .unwrap_or_default()
                .contains("webcodex_search"));
            assert_eq!(
                payload.command_execution_state,
                Some(ShellCommandExecutionState::Completed)
            );
        }
        other => panic!("expected result, got {:?}", other.kind()),
    }
}

#[cfg(unix)]
#[test]
fn dispatch_request_internal_posix_script_ignores_configured_shell_parser() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = test_config(tmp.path().join("config/projects.d"));
    cfg.shell.program = "/bin/false".to_string();
    cfg.shell.dialect = Some(crate::webcodex_runner::config::ShellDialect::PowerShell);
    let jobs = JobManager::new(max_concurrent_jobs(&cfg));
    let pdir = projects_dir(&cfg).unwrap();
    let hot = runtime_config(&cfg);
    let persistent_shells = webcodex_runner::PersistentShellManager::new(
        &cfg.shell,
        webcodex_runner::SshConnectionPool::default(),
    );
    let (sink, mut rx) = ws_sink("ws-client");
    let request = ShellAgentShellRequest {
        request_id: "req-internal-posix".to_string(),
        client_id: "ws-client".to_string(),
        kind: "run_internal_posix_script".to_string(),
        job_id: None,
        cwd: Some(tmp.path().to_string_lossy().to_string()),
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
        script: Some(shell_protocol::ShellScriptPayload {
            language: shell_protocol::ShellScriptLanguage::Sh,
            script: "printf 'internal-posix-dispatch-ok\\n'\n".to_string(),
            args: Vec::new(),
        }),
        stdin: None,
        timeout_secs: 10,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        sandbox: None,
        job_context: None,
        persistent_shell: None,
    };

    assert!(dispatch_request(
        &sink,
        &hot.snapshot(),
        &hot,
        &jobs,
        &persistent_shells,
        &pdir,
        &webcodex_runner::LspSupervisor::default(),
        request,
    )
    .unwrap());
    match rx.try_recv().expect("internal POSIX result") {
        AgentEnvelope::Result { payload } => {
            assert_eq!(payload.result.exit_code, Some(0));
            assert_eq!(
                payload.result.stdout.as_deref(),
                Some("internal-posix-dispatch-ok\n")
            );
            assert_eq!(
                payload.command_execution_state,
                Some(ShellCommandExecutionState::Completed)
            );
        }
        other => panic!("expected result, got {:?}", other.kind()),
    }
}

#[test]
fn dispatch_request_run_shell_rejects_oversized_wire_command_before_start() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let jobs = JobManager::new(max_concurrent_jobs(&cfg));
    let pdir = projects_dir(&cfg).unwrap();
    let hot = runtime_config(&cfg);
    let persistent_shells = webcodex_runner::PersistentShellManager::new(
        &cfg.shell,
        webcodex_runner::SshConnectionPool::default(),
    );
    let (sink, mut rx) = ws_sink("ws-client");
    let request = ShellAgentShellRequest {
        request_id: "req-oversized-shell".to_string(),
        client_id: "ws-client".to_string(),
        kind: "run_shell".to_string(),
        job_id: None,
        cwd: Some(tmp.path().to_string_lossy().to_string()),
        path: None,
        content: None,
        max_bytes: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        create_dirs: false,
        command: "x".repeat(shell_protocol::RAW_SHELL_WIRE_MAX_BYTES + 1),
        process: None,
        script: None,
        stdin: None,
        timeout_secs: 10,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        sandbox: None,
        job_context: None,
        persistent_shell: None,
    };

    assert!(dispatch_request(
        &sink,
        &hot.snapshot(),
        &hot,
        &jobs,
        &persistent_shells,
        &pdir,
        &webcodex_runner::LspSupervisor::default(),
        request,
    )
    .unwrap());
    let env = rx.try_recv().expect("rejection envelope was sent");
    match env {
        AgentEnvelope::Result { payload } => {
            assert_eq!(payload.result.request_id, "req-oversized-shell");
            assert_eq!(payload.result.exit_code, None);
            assert_eq!(
                payload.command_execution_state,
                Some(ShellCommandExecutionState::NotStarted)
            );
            assert!(payload
                .result
                .error
                .as_deref()
                .unwrap_or_default()
                .contains("invalid_raw_shell_request"));
        }
        other => panic!("expected result, got {:?}", other.kind()),
    }
}

#[test]
fn dispatch_request_structured_process_uses_typed_argv_and_never_shell_fallback() {
    let tmp = tempfile::tempdir().unwrap();
    let helper = tmp.path().join(format!(
        "process-argv-helper{}",
        std::env::consts::EXE_SUFFIX
    ));
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/process_argv_helper.rs");
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let compile = std::process::Command::new(rustc)
        .arg("--edition=2021")
        .arg("--crate-name=webcodex_process_argv_helper")
        .arg(fixture)
        .arg("-o")
        .arg(&helper)
        .output()
        .expect("run rustc for process argv helper");
    assert!(
        compile.status.success(),
        "process argv helper compilation failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let cfg = test_config(tmp.path().join("config/projects.d"));
    let jobs = JobManager::new(max_concurrent_jobs(&cfg));
    let pdir = projects_dir(&cfg).unwrap();
    let hot = runtime_config(&cfg);
    let persistent_shells = webcodex_runner::PersistentShellManager::new(
        &cfg.shell,
        webcodex_runner::SshConnectionPool::default(),
    );
    let marker = tmp.path().join("marker");

    let (sink, mut rx) = ws_sink("ws-client");
    let request = ShellAgentShellRequest {
        request_id: "req-structured-process".to_string(),
        client_id: "ws-client".to_string(),
        kind: "run_process".to_string(),
        job_id: None,
        cwd: Some(tmp.path().to_string_lossy().to_string()),
        path: None,
        content: None,
        max_bytes: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        create_dirs: false,
        command: String::new(),
        process: Some(shell_protocol::ShellProcessArgv {
            executable: helper.to_string_lossy().into_owned(),
            args: vec![
                "argv".to_string(),
                "$(touch marker)".to_string(),
                "; touch marker".to_string(),
            ],
        }),
        script: None,
        stdin: None,
        timeout_secs: 10,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        sandbox: None,
        job_context: None,
        persistent_shell: None,
    };
    assert!(dispatch_request(
        &sink,
        &hot.snapshot(),
        &hot,
        &jobs,
        &persistent_shells,
        &pdir,
        &webcodex_runner::LspSupervisor::default(),
        request,
    )
    .unwrap());
    match rx.try_recv().expect("structured process result") {
        AgentEnvelope::Result { payload } => {
            assert_eq!(payload.result.exit_code, Some(0));
            assert_eq!(
                payload.command_execution_state,
                Some(ShellCommandExecutionState::Completed)
            );
            let stdout = payload.result.stdout.unwrap();
            assert!(stdout.contains("$(touch marker)"));
            assert!(stdout.contains("; touch marker"));
        }
        other => panic!("expected result, got {:?}", other.kind()),
    }
    assert!(!marker.exists());

    let shell_fallback_marker = tmp.path().join("shell-fallback-marker");
    let (sink, mut rx) = ws_sink("ws-client");
    let malformed = ShellAgentShellRequest {
        request_id: "req-structured-process-malformed".to_string(),
        client_id: "ws-client".to_string(),
        kind: "run_process".to_string(),
        job_id: None,
        cwd: Some(tmp.path().to_string_lossy().to_string()),
        path: None,
        content: None,
        max_bytes: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        create_dirs: false,
        command: shell_write_file(&shell_fallback_marker),
        process: None,
        script: None,
        stdin: None,
        timeout_secs: 10,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        sandbox: None,
        job_context: None,
        persistent_shell: None,
    };
    assert!(dispatch_request(
        &sink,
        &hot.snapshot(),
        &hot,
        &jobs,
        &persistent_shells,
        &pdir,
        &webcodex_runner::LspSupervisor::default(),
        malformed,
    )
    .unwrap());
    match rx.try_recv().expect("structured process rejection") {
        AgentEnvelope::Result { payload } => {
            assert_eq!(payload.result.exit_code, None);
            assert_eq!(
                payload.command_execution_state,
                Some(ShellCommandExecutionState::NotStarted)
            );
        }
        other => panic!("expected result, got {:?}", other.kind()),
    }
    assert!(!shell_fallback_marker.exists());
}

#[cfg(unix)]
#[test]
fn dispatch_request_structured_script_uses_typed_file_and_never_shell_fallback() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let jobs = JobManager::new(max_concurrent_jobs(&cfg));
    let pdir = projects_dir(&cfg).unwrap();
    let hot = runtime_config(&cfg);
    let persistent_shells = webcodex_runner::PersistentShellManager::new(
        &cfg.shell,
        webcodex_runner::SshConnectionPool::default(),
    );
    let observed_path = tmp.path().join("observed-script-path");
    let marker = tmp.path().join("marker");
    let shell_fallback_marker = tmp.path().join("shell-fallback-marker");

    let request = ShellAgentShellRequest {
        request_id: "req-structured-script".to_string(),
        client_id: "ws-client".to_string(),
        kind: "run_script".to_string(),
        job_id: None,
        cwd: Some(tmp.path().to_string_lossy().to_string()),
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
        script: Some(shell_protocol::ShellScriptPayload {
            language: shell_protocol::ShellScriptLanguage::Sh,
            script: "printf '%s' \"$0\" > \"$1\"\nprintf '%s\\n' \"$2\"\n".to_string(),
            args: vec![
                observed_path.to_string_lossy().into_owned(),
                "; touch marker".to_string(),
            ],
        }),
        stdin: None,
        timeout_secs: 10,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        sandbox: None,
        job_context: None,
        persistent_shell: None,
    };
    let mut malformed = request.clone();
    malformed.request_id = "req-structured-script-malformed".to_string();
    malformed.command = shell_write_file(&shell_fallback_marker);
    malformed.script = None;

    let (sink, mut rx) = ws_sink("ws-client");
    assert!(dispatch_request(
        &sink,
        &hot.snapshot(),
        &hot,
        &jobs,
        &persistent_shells,
        &pdir,
        &webcodex_runner::LspSupervisor::default(),
        request,
    )
    .unwrap());
    match rx.try_recv().expect("structured script result") {
        AgentEnvelope::Result { payload } => {
            assert_eq!(payload.result.exit_code, Some(0));
            assert_eq!(payload.result.stdout.as_deref(), Some("; touch marker\n"));
            assert_eq!(
                payload.command_execution_state,
                Some(ShellCommandExecutionState::Completed)
            );
        }
        other => panic!("expected result, got {:?}", other.kind()),
    }
    assert!(!marker.exists());
    let temporary_path =
        PathBuf::from(std::fs::read_to_string(&observed_path).expect("script path evidence"));
    assert!(!temporary_path.starts_with(tmp.path()));
    assert!(!temporary_path.exists());

    let (sink, mut rx) = ws_sink("ws-client");
    assert!(dispatch_request(
        &sink,
        &hot.snapshot(),
        &hot,
        &jobs,
        &persistent_shells,
        &pdir,
        &webcodex_runner::LspSupervisor::default(),
        malformed,
    )
    .unwrap());
    match rx.try_recv().expect("structured script rejection") {
        AgentEnvelope::Result { payload } => {
            assert_eq!(payload.result.exit_code, None);
            assert_eq!(
                payload.command_execution_state,
                Some(ShellCommandExecutionState::NotStarted)
            );
        }
        other => panic!("expected result, got {:?}", other.kind()),
    }
    assert!(!shell_fallback_marker.exists());
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
fn register_project_writes_valid_toml_into_projects_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("repo");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&project_dir).unwrap();
    let policy = project_policy(tmp.path());
    let req = project_request(
        "register_project",
        serde_json::json!({
            "id": "demo",
            "name": "Demo",
            "path": project_dir.to_string_lossy(),
            "description": "A demo project",
            "allow_patch": false
        }),
    );

    let value = project_ok(handle_project_op(&policy, &projects_dir, &req));
    assert_eq!(value["created_config"], true);
    assert_eq!(value["overwritten"], false);

    let content = std::fs::read_to_string(projects_dir.join("demo.toml")).unwrap();
    let parsed = parse_agent_project_toml(&content).unwrap();
    assert_eq!(parsed.id, "demo");
    assert_eq!(parsed.name.as_deref(), Some("Demo"));
    assert_eq!(parsed.path, project_dir.to_string_lossy());
    assert!(!parsed.allow_patch);
}

#[test]
fn resolve_or_register_project_persists_and_reuses_canonical_directory_without_touching_it() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("Example Repo");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&project_dir).unwrap();
    std::fs::write(project_dir.join("keep.txt"), "unchanged").unwrap();
    let target_entries_before = std::fs::read_dir(&project_dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    let policy = project_policy(tmp.path());

    let first = project_ok(handle_resolve_or_register_project(
        &policy,
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": project_dir.join(".").to_string_lossy()}),
        ),
    ));
    assert_eq!(first["outcome"], "auto_registered");
    assert_eq!(first["registered"], true);
    assert_eq!(first["changed"], true);
    let project_id = first["agent_project_id"].as_str().unwrap();
    assert!(project_id.starts_with("example-repo-"), "{project_id}");
    assert!(project_id.len() <= 64);

    let config_path = projects_dir.join(format!("{project_id}.toml"));
    let persisted =
        parse_agent_project_toml(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    assert_eq!(persisted.id, project_id);
    assert_eq!(
        Path::new(&persisted.path),
        project_dir.canonicalize().unwrap()
    );
    assert_eq!(persisted.kind.as_deref(), Some("auto_registered"));
    assert!(persisted.allow_patch);
    assert_eq!(
        std::fs::read_to_string(project_dir.join("keep.txt")).unwrap(),
        "unchanged"
    );
    let target_entries_after = std::fs::read_dir(&project_dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    assert_eq!(target_entries_after, target_entries_before);
    assert!(!project_dir.join(".git").exists());

    let reloaded = load_agent_project_summaries_from_dir(&projects_dir);
    assert_eq!(reloaded.len(), 1);
    assert_eq!(reloaded[0].id, project_id);
    let second = project_ok(handle_resolve_or_register_project(
        &policy,
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": project_dir.to_string_lossy()}),
        ),
    ));
    assert_eq!(second["outcome"], "reused_existing_registration");
    assert_eq!(second["registered"], false);
    assert_eq!(second["agent_project_id"], project_id);
    assert_eq!(
        std::fs::read_dir(&projects_dir).unwrap().count(),
        1,
        "retry created a duplicate registration"
    );
}

#[cfg(unix)]
#[test]
fn resolve_or_register_project_reuses_symlink_target() {
    use std::os::unix::fs::symlink;

    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("repo");
    let link = tmp.path().join("repo-link");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&project_dir).unwrap();
    symlink(&project_dir, &link).unwrap();
    let policy = project_policy(tmp.path());

    let first = project_ok(handle_resolve_or_register_project(
        &policy,
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": project_dir.to_string_lossy()}),
        ),
    ));
    let second = project_ok(handle_resolve_or_register_project(
        &policy,
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": link.to_string_lossy()}),
        ),
    ));
    assert_eq!(second["outcome"], "reused_existing_registration");
    assert_eq!(second["agent_project_id"], first["agent_project_id"]);
    assert_eq!(
        Path::new(second["path"].as_str().unwrap()),
        project_dir.canonicalize().unwrap()
    );
}

#[test]
fn resolve_or_register_project_prefers_manual_id_and_distinguishes_same_basenames() {
    let tmp = tempfile::tempdir().unwrap();
    let first_parent = tmp.path().join("first");
    let second_parent = tmp.path().join("second");
    let manual_dir = tmp.path().join("manual");
    let projects_dir = tmp.path().join("projects.d");
    for directory in [
        first_parent.join("repo"),
        second_parent.join("repo"),
        manual_dir.clone(),
    ] {
        std::fs::create_dir_all(directory).unwrap();
    }
    std::fs::create_dir(&projects_dir).unwrap();
    std::fs::write(
        projects_dir.join("friendly.toml"),
        format!(
            "id = \"friendly\"\nname = \"Friendly\"\npath = {:?}\nallow_patch = true\n",
            manual_dir.to_string_lossy()
        ),
    )
    .unwrap();
    let policy = project_policy(tmp.path());

    let manual = project_ok(handle_resolve_or_register_project(
        &policy,
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": manual_dir.join(".").to_string_lossy()}),
        ),
    ));
    assert_eq!(manual["outcome"], "reused_existing_registration");
    assert_eq!(manual["agent_project_id"], "friendly");

    let first = project_ok(handle_resolve_or_register_project(
        &policy,
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": first_parent.join("repo").to_string_lossy()}),
        ),
    ));
    let second = project_ok(handle_resolve_or_register_project(
        &policy,
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": second_parent.join("repo").to_string_lossy()}),
        ),
    ));
    assert_ne!(first["agent_project_id"], second["agent_project_id"]);
    assert!(first["agent_project_id"]
        .as_str()
        .unwrap()
        .starts_with("repo-"));
    assert!(second["agent_project_id"]
        .as_str()
        .unwrap()
        .starts_with("repo-"));
}

#[test]
fn resolve_or_register_project_fails_closed_for_disabled_and_ambiguous_matches() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("repo");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&project_dir).unwrap();
    std::fs::create_dir(&projects_dir).unwrap();
    std::fs::write(
        projects_dir.join("disabled.toml"),
        format!(
            "id = \"disabled\"\npath = {:?}\ndisabled = true\n",
            project_dir.to_string_lossy()
        ),
    )
    .unwrap();
    let policy = project_policy(tmp.path());

    let disabled = project_error_value(handle_resolve_or_register_project(
        &policy,
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": project_dir.to_string_lossy()}),
        ),
    ));
    assert_eq!(disabled["error_kind"], "project_disabled");
    assert_eq!(disabled["matching_project_id"], "disabled");
    assert_eq!(disabled["state_changed"], false);
    assert!(
        !disabled.to_string().contains(project_dir.to_str().unwrap()),
        "disabled error leaked the absolute path"
    );

    std::fs::write(
        projects_dir.join("alpha.toml"),
        format!(
            "id = \"alpha\"\npath = {:?}\n",
            project_dir.to_string_lossy()
        ),
    )
    .unwrap();
    std::fs::write(
        projects_dir.join("zeta.toml"),
        format!(
            "id = \"zeta\"\npath = {:?}\n",
            project_dir.join(".").to_string_lossy()
        ),
    )
    .unwrap();
    let ambiguous = project_error_value(handle_resolve_or_register_project(
        &policy,
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": project_dir.to_string_lossy()}),
        ),
    ));
    assert_eq!(ambiguous["error_kind"], "ambiguous_project_path");
    assert_eq!(
        ambiguous["matching_project_ids"],
        serde_json::json!(["alpha", "disabled", "zeta"])
    );
    assert_eq!(ambiguous["state_changed"], false);
}

#[test]
fn resolve_or_register_project_rejects_invalid_non_directory_and_disallowed_paths() {
    let allowed = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let projects_dir = allowed.path().join("projects.d");
    let file = allowed.path().join("file.txt");
    std::fs::write(&file, "not a directory").unwrap();
    let policy = project_policy(allowed.path());

    for (path, expected) in [
        ("relative/path".to_string(), "invalid_project_path"),
        (
            allowed.path().join("missing").to_string_lossy().to_string(),
            "project_path_not_found",
        ),
        (
            file.to_string_lossy().to_string(),
            "project_path_not_directory",
        ),
        (
            outside.path().to_string_lossy().to_string(),
            "path_outside_allowed_roots",
        ),
    ] {
        let error = project_error_value(handle_resolve_or_register_project(
            &policy,
            &projects_dir,
            &project_request(
                "resolve_or_register_project",
                serde_json::json!({"path": path}),
            ),
        ));
        assert_eq!(error["error_kind"], expected);
        assert_eq!(error["state_changed"], false);
    }
    assert!(!projects_dir.exists());

    let unrestricted = AgentPolicy {
        allow_cwd_anywhere: true,
        allowed_roots: Vec::new(),
        ..AgentPolicy::default()
    };
    // Dangerous system roots are platform-specific: `/etc` on Unix,
    // `C:\Windows` on Windows (drive roots are also rejected).
    #[cfg(windows)]
    let dangerous_path = "C:\\Windows";
    #[cfg(not(windows))]
    let dangerous_path = "/etc";
    let dangerous = project_error_value(handle_resolve_or_register_project(
        &unrestricted,
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": dangerous_path}),
        ),
    ));
    assert_eq!(dangerous["error_kind"], "path_outside_allowed_roots");
}

#[cfg(windows)]
#[test]
fn resolve_or_register_project_rejects_unc_and_non_local_disk_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let projects_dir = tmp.path().join("projects.d");
    let policy = project_policy(tmp.path());

    // The raw path check must fire before canonicalization: these shares do
    // not exist, but the error is the platform rule, not "path not found".
    for unc_path in [
        r"\\server\share\repo",
        r"\\?\UNC\server\share\repo",
        r"\\.\device\repo",
        r"\\?\Volume{12345678-1234-1234-1234-123456789abc}\repo",
    ] {
        let error = project_error_value(handle_resolve_or_register_project(
            &policy,
            &projects_dir,
            &project_request(
                "resolve_or_register_project",
                serde_json::json!({"path": unc_path}),
            ),
        ));
        assert_eq!(
            error["error_kind"], "unc_project_path_unsupported",
            "{unc_path} must fail closed as an unsupported non-local-disk path"
        );
        assert_eq!(error["state_changed"], false);
    }
    assert!(!projects_dir.exists(), "no registration may be attempted");

    // An allowed_roots entry naming a UNC share must not bypass the rule.
    let unc_allowed = AgentPolicy {
        allow_cwd_anywhere: false,
        allowed_roots: vec![PathBuf::from(r"\\server\share\repo")],
        ..AgentPolicy::default()
    };
    let error = project_error_value(handle_resolve_or_register_project(
        &unc_allowed,
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": r"\\server\share\repo"}),
        ),
    ));
    assert_eq!(
        error["error_kind"], "unc_project_path_unsupported",
        "a UNC allowed_root must not make a UNC project root acceptable"
    );
}

#[cfg(windows)]
#[test]
fn resolve_or_register_project_accepts_local_drive_and_verbatim_disk_identity() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("repo");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&project_dir).unwrap();
    let policy = project_policy(tmp.path());

    // Plain local-drive path registers normally.
    let plain = project_dir.to_string_lossy().to_string();
    assert!(Path::new(&plain).is_absolute());
    let first = project_ok(handle_resolve_or_register_project(
        &policy,
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": plain}),
        ),
    ));
    assert_eq!(first["outcome"], "auto_registered");
    let project_id = first["agent_project_id"].as_str().unwrap().to_string();

    // The canonicalized `\\?\C:\...` spelling of the same directory must
    // reuse the registration instead of minting a duplicate identity. The
    // verbatim form is built from the plain path: `canonicalize()` already
    // returns `\\?\`-prefixed paths on modern Rust, so re-prefixing those
    // would double the prefix.
    let raw = project_dir.to_string_lossy().to_string();
    let verbatim = if raw.starts_with(r"\\?\") {
        raw
    } else {
        // `\\?\` + the raw path: the prefix itself ends with a backslash.
        format!(r"\\?\{raw}")
    };
    let second = project_ok(handle_resolve_or_register_project(
        &policy,
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": verbatim}),
        ),
    ));
    assert_eq!(second["outcome"], "reused_existing_registration");
    assert_eq!(second["agent_project_id"], project_id);
    assert_eq!(
        std::fs::read_dir(&projects_dir).unwrap().count(),
        1,
        "the \\\\?\\ spelling created a duplicate project identity"
    );
}

#[cfg(windows)]
#[test]
fn register_project_rejects_unc_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let projects_dir = tmp.path().join("projects.d");
    let policy = project_policy(tmp.path());

    let error = project_error_value(handle_project_op(
        &policy,
        &projects_dir,
        &project_request(
            "register_project",
            serde_json::json!({
                "id": "demo",
                "name": "Demo",
                "path": r"\\server\share\repo",
                "description": "UNC project",
                "allow_patch": false
            }),
        ),
    ));
    assert_eq!(error["error_code"], "unc_project_path_unsupported");
    assert!(!projects_dir.exists());
}

#[test]
fn concurrent_path_resolution_converges_on_one_registration() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("repo");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&project_dir).unwrap();
    let policy = project_policy(tmp.path());
    let mut workers = Vec::new();
    for _ in 0..2 {
        let project_dir = project_dir.clone();
        let projects_dir = projects_dir.clone();
        let policy = policy.clone();
        workers.push(std::thread::spawn(move || {
            project_ok(handle_resolve_or_register_project(
                &policy,
                &projects_dir,
                &project_request(
                    "resolve_or_register_project",
                    serde_json::json!({"path": project_dir.to_string_lossy()}),
                ),
            ))
        }));
    }
    let results = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        results[0]["agent_project_id"],
        results[1]["agent_project_id"]
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| result["registered"] == true)
            .count(),
        1
    );
    assert_eq!(std::fs::read_dir(&projects_dir).unwrap().count(), 1);
}

#[test]
fn auto_project_id_collision_extends_hash_without_overwriting() {
    use sha2::{Digest, Sha256};

    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("repo");
    let other_dir = tmp.path().join("other");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&project_dir).unwrap();
    std::fs::create_dir(&other_dir).unwrap();
    std::fs::create_dir(&projects_dir).unwrap();
    let canonical = project_dir.canonicalize().unwrap();
    // Match the Runner's project identity: raw bytes on Unix, normalized
    // (lowercased, `\\?\` stripped) on Windows.
    #[cfg(windows)]
    let identity = webcodex_agent_config::paths::normalize_path_identity(&canonical);
    #[cfg(not(windows))]
    let identity = canonical.to_string_lossy().to_string();
    let digest = format!("{:x}", Sha256::digest(identity.as_bytes()));
    let colliding_id = format!("repo-{}", &digest[..8]);
    let colliding_config = format!(
        "id = {:?}\npath = {:?}\n",
        colliding_id,
        other_dir.to_string_lossy()
    );
    std::fs::write(
        projects_dir.join(format!("{colliding_id}.toml")),
        &colliding_config,
    )
    .unwrap();

    let result = project_ok(handle_resolve_or_register_project(
        &project_policy(tmp.path()),
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": project_dir.to_string_lossy()}),
        ),
    ));
    let generated = result["agent_project_id"].as_str().unwrap();
    assert_ne!(generated, colliding_id);
    assert_eq!(generated, format!("repo-{}", &digest[..12]));
    assert_eq!(
        std::fs::read_to_string(projects_dir.join(format!("{colliding_id}.toml"))).unwrap(),
        colliding_config
    );
}

#[test]
fn path_registration_publish_failure_leaves_no_config_or_temp_file() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("repo");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&project_dir).unwrap();
    webcodex_runner::projects::fail_next_project_publish_before_rename();

    let error = project_error_value(handle_resolve_or_register_project(
        &project_policy(tmp.path()),
        &projects_dir,
        &project_request(
            "resolve_or_register_project",
            serde_json::json!({"path": project_dir.to_string_lossy()}),
        ),
    ));
    assert_eq!(error["error_kind"], "operation_failed");
    assert_eq!(error["state_changed"], false);
    assert_eq!(std::fs::read_dir(&projects_dir).unwrap().count(), 0);
}

#[test]
fn managed_temporary_project_is_registered_persistent_and_ordinary_project_compatible() {
    let tmp = tempfile::tempdir().unwrap();
    let temporary_root = tmp.path().join("temporary-projects");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&temporary_root).unwrap();
    let policy = project_policy(tmp.path());
    let request = project_request(
        "create_project",
        serde_json::json!({
            "managed_temporary_project": true,
            "name": "Scratch task"
        }),
    );

    let created = project_ok(handle_project_op_with_temporary_projects_root(
        &policy,
        &projects_dir,
        Some(&temporary_root),
        &request,
    ));
    let id = created["agent_project_id"].as_str().unwrap();
    let path = PathBuf::from(created["path"].as_str().unwrap());
    let canonical_root = temporary_root.canonicalize().unwrap();

    assert_eq!(created["source"], "managed_temporary");
    assert_eq!(created["kind"], "managed_temporary");
    assert_eq!(created["git_initialized"], true);
    assert_eq!(path.parent(), Some(canonical_root.as_path()));
    assert!(path.is_dir());
    assert!(path.join(".git").is_dir());
    assert_eq!(
        path.canonicalize().unwrap(),
        path,
        "the returned project path must be canonical"
    );

    let persisted = parse_agent_project_toml(
        &std::fs::read_to_string(projects_dir.join(format!("{id}.toml"))).unwrap(),
    )
    .unwrap();
    assert_eq!(persisted.kind.as_deref(), Some("managed_temporary"));
    assert_eq!(persisted.path, path.to_string_lossy());

    // A fresh project-registry scan models a Runner restart: it finds the same
    // ordinary project record, including its source marker and canonical path.
    let reloaded = load_agent_project_summaries_from_dir(&projects_dir);
    assert_eq!(reloaded.len(), 1);
    assert_eq!(reloaded[0].id, id);
    assert_eq!(reloaded[0].kind.as_deref(), Some("managed_temporary"));
    assert_eq!(reloaded[0].path, path.to_string_lossy());

    // The normal shell and structured project-overview paths receive the
    // registered path with no temporary-project special case.
    let shell = run_shell(
        &policy,
        &ShellConfig::default(),
        Some(path.to_string_lossy().as_ref()),
        &shell_echo("managed-shell"),
        None,
        10,
        None,
    );
    assert_eq!(shell.exit_code, Some(0), "{shell:?}");
    assert_eq!(shell.stdout.as_deref(), Some("managed-shell"));

    std::fs::write(path.join("README.md"), "managed\n").unwrap();
    let overview_request = json_file_op_request(
        &path,
        "file_project_overview",
        ".",
        serde_json::json!({"max_depth": 1, "limit": 20}),
    );
    let overview = project_ok(handle_file_request(&policy, &overview_request));
    assert_eq!(overview["schema_version"], 1);
    assert!(overview.to_string().contains("README.md"));
}

#[test]
fn managed_temporary_project_rejects_path_traversal_and_never_overwrites_existing_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let temporary_root = tmp.path().join("temporary-projects");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&temporary_root).unwrap();
    let pre_existing = temporary_root.join("scratch");
    std::fs::create_dir(&pre_existing).unwrap();
    std::fs::write(pre_existing.join("keep.txt"), "keep").unwrap();
    let policy = project_policy(tmp.path());

    let traversal = project_err(handle_project_op_with_temporary_projects_root(
        &policy,
        &projects_dir,
        Some(&temporary_root),
        &project_request(
            "create_project",
            serde_json::json!({
                "managed_temporary_project": true,
                "name": "Scratch",
                "path": "../escape"
            }),
        ),
    ));
    assert_eq!(traversal, "invalid_request");
    assert!(!tmp.path().join("escape").exists());

    let path_like_name = project_err(handle_project_op_with_temporary_projects_root(
        &policy,
        &projects_dir,
        Some(&temporary_root),
        &project_request(
            "create_project",
            serde_json::json!({
                "managed_temporary_project": true,
                "name": "../escape"
            }),
        ),
    ));
    assert_eq!(path_like_name, "invalid_request");

    let created = project_ok(handle_project_op_with_temporary_projects_root(
        &policy,
        &projects_dir,
        Some(&temporary_root),
        &project_request(
            "create_project",
            serde_json::json!({
                "managed_temporary_project": true,
                "name": "scratch"
            }),
        ),
    ));
    assert_ne!(
        created["path"].as_str(),
        Some(pre_existing.to_string_lossy().as_ref())
    );
    assert_eq!(
        std::fs::read_to_string(pre_existing.join("keep.txt")).unwrap(),
        "keep"
    );
}

#[test]
fn managed_temporary_project_requires_root_inside_runner_policy() {
    let allowed = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let projects_dir = allowed.path().join("projects.d");
    let policy = project_policy(allowed.path());

    let error = project_err(handle_project_op_with_temporary_projects_root(
        &policy,
        &projects_dir,
        Some(outside.path()),
        &project_request(
            "create_project",
            serde_json::json!({"managed_temporary_project": true}),
        ),
    ));
    assert_eq!(error, "temporary_projects_root_outside_allowed_roots");
    assert!(!projects_dir.exists());
}

#[test]
fn create_post_rename_sync_failure_preserves_source_and_registry() {
    let tmp = tempfile::tempdir().unwrap();
    let projects_dir = tmp.path().join("projects.d");
    let create_dir = tmp.path().join("created-after-rename");
    let policy = project_policy(tmp.path());
    webcodex_runner::projects::fail_next_project_parent_sync_after_rename();
    let error = project_err(handle_project_op(
        &policy,
        &projects_dir,
        &project_request(
            "create_project",
            serde_json::json!({
                "id":"indeterminate", "name":"Indeterminate",
                "description":"Preserve me", "path":create_dir.to_string_lossy(),
                "allow_patch":true, "template":"basic", "git_init":true
            }),
        ),
    ));
    assert_eq!(error, "operation_indeterminate");
    assert!(projects_dir.join("indeterminate.toml").is_file());
    assert!(create_dir.join("README.md").is_file());
    assert!(create_dir.join(".gitignore").is_file());
    assert!(create_dir.join(".git").is_dir());
}

#[test]
fn register_and_create_retries_converge_without_duplicate_side_effects() {
    let tmp = tempfile::tempdir().unwrap();
    let projects_dir = tmp.path().join("projects.d");
    let register_dir = tmp.path().join("existing");
    std::fs::create_dir(&register_dir).unwrap();
    let policy = project_policy(tmp.path());
    let register = project_request(
        "register_project",
        serde_json::json!({
            "id":"registered", "name":"Registered",
            "path":register_dir.to_string_lossy(), "allow_patch":true
        }),
    );
    let first = project_ok(handle_project_op(&policy, &projects_dir, &register));
    let retry = project_ok(handle_project_op(&policy, &projects_dir, &register));
    assert_eq!(retry["recovered"], true);
    assert_eq!(retry["changed"], false);
    assert_eq!(retry["revision"], first["revision"]);

    let create_dir = tmp.path().join("created");
    let create = project_request(
        "create_project",
        serde_json::json!({
            "id":"created", "name":"Created", "description":"Fixture",
            "path":create_dir.to_string_lossy(), "allow_patch":true,
            "template":"basic", "git_init":true,
            "allow_existing_empty":false
        }),
    );
    let created = project_ok(handle_project_op(&policy, &projects_dir, &create));
    let readme_before = std::fs::read(create_dir.join("README.md")).unwrap();
    let recovered = project_ok(handle_project_op(&policy, &projects_dir, &create));
    assert_eq!(recovered["recovered"], true);
    assert_eq!(recovered["changed"], false);
    assert_eq!(recovered["revision"], created["revision"]);
    assert_eq!(
        std::fs::read(create_dir.join("README.md")).unwrap(),
        readme_before
    );
    assert!(create_dir.join(".git").is_dir());

    let mismatch = project_err(handle_project_op(
        &policy,
        &projects_dir,
        &project_request(
            "register_project",
            serde_json::json!({
                "id":"registered", "name":"Different",
                "path":register_dir.to_string_lossy(), "allow_patch":true
            }),
        ),
    ));
    assert_eq!(mismatch, "project_already_exists");
}

#[test]
fn project_lifecycle_persists_state_and_unregister_preserves_source() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("repo");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&project_dir).unwrap();
    std::fs::create_dir(project_dir.join(".git")).unwrap();
    std::fs::write(project_dir.join("keep.txt"), "keep").unwrap();
    let policy = project_policy(tmp.path());
    let registered = project_ok(handle_project_op(
        &policy,
        &projects_dir,
        &project_request(
            "register_project",
            serde_json::json!({
                "id": "demo",
                "name": "Demo",
                "path": project_dir.to_string_lossy()
            }),
        ),
    ));
    let revision = registered["revision"].as_str().unwrap().to_string();

    let disabled = project_ok(handle_project_lifecycle_op(
        &policy,
        &projects_dir,
        &project_request(
            "project_lifecycle_disable",
            serde_json::json!({"project_id":"demo","expected_revision":revision}),
        ),
    ));
    assert_eq!(disabled["outcome"], "disabled");
    let retry_disabled = project_ok(handle_project_lifecycle_op(
        &policy,
        &projects_dir,
        &project_request(
            "project_lifecycle_disable",
            serde_json::json!({"project_id":"demo","expected_revision":registered["revision"]}),
        ),
    ));
    assert_eq!(retry_disabled["outcome"], "already_disabled");
    let disabled_revision = disabled["revision"].as_str().unwrap().to_string();
    let summaries = load_agent_project_summaries_from_dir(&projects_dir);
    assert_eq!(summaries.len(), 1);
    assert!(summaries[0].disabled);

    let stale = project_err(handle_project_lifecycle_op(
        &policy,
        &projects_dir,
        &project_request(
            "project_lifecycle_enable",
            serde_json::json!({"project_id":"demo","expected_revision":registered["revision"]}),
        ),
    ));
    assert_eq!(stale, "revision_conflict");

    let enabled = project_ok(handle_project_lifecycle_op(
        &policy,
        &projects_dir,
        &project_request(
            "project_lifecycle_enable",
            serde_json::json!({"project_id":"demo","expected_revision":disabled_revision}),
        ),
    ));
    assert_eq!(enabled["outcome"], "enabled");
    let retry_enabled = project_ok(handle_project_lifecycle_op(
        &policy,
        &projects_dir,
        &project_request(
            "project_lifecycle_enable",
            serde_json::json!({"project_id":"demo","expected_revision":disabled["revision"]}),
        ),
    ));
    assert_eq!(retry_enabled["outcome"], "already_enabled");

    let unregistered = project_ok(handle_project_lifecycle_op(
        &policy,
        &projects_dir,
        &project_request(
            "project_lifecycle_unregister",
            serde_json::json!({
                "project_id":"demo",
                "expected_revision":enabled["revision"]
            }),
        ),
    ));
    assert_eq!(unregistered["outcome"], "unregistered");
    assert!(!projects_dir.join("demo.toml").exists());
    assert!(project_dir.join("keep.txt").exists());
    assert!(project_dir.join(".git").is_dir());

    let repeated = project_ok(handle_project_lifecycle_op(
        &policy,
        &projects_dir,
        &project_request(
            "project_lifecycle_unregister",
            serde_json::json!({"project_id":"demo","expected_revision":enabled["revision"]}),
        ),
    ));
    assert_eq!(repeated["outcome"], "already_unregistered");

    let stale_tombstone = projects_dir.join(".demo.crash.toml.unregistering");
    std::fs::write(&stale_tombstone, "stale").unwrap();
    assert!(load_agent_project_summaries_from_dir(&projects_dir).is_empty());
    let recovered = project_ok(handle_project_lifecycle_op(
        &policy,
        &projects_dir,
        &project_request(
            "project_lifecycle_unregister",
            serde_json::json!({"project_id":"demo","expected_revision":enabled["revision"]}),
        ),
    ));
    assert_eq!(recovered["outcome"], "already_unregistered");
    assert!(!stale_tombstone.exists());
}

#[test]
fn project_unregister_post_rename_sync_failure_is_indeterminate_and_retry_converges() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("repo");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&project_dir).unwrap();
    std::fs::write(project_dir.join("keep.txt"), "keep").unwrap();
    let policy = project_policy(tmp.path());
    let registered = project_ok(handle_project_op(
        &policy,
        &projects_dir,
        &project_request(
            "register_project",
            serde_json::json!({
                "id": "demo",
                "name": "Demo",
                "path": project_dir.to_string_lossy()
            }),
        ),
    ));
    let revision = registered["revision"].as_str().unwrap().to_string();

    webcodex_runner::projects::fail_next_project_parent_sync_after_rename();
    let error = project_error_value(handle_project_lifecycle_op(
        &policy,
        &projects_dir,
        &project_request(
            "project_lifecycle_unregister",
            serde_json::json!({"project_id":"demo","expected_revision":revision}),
        ),
    ));
    assert_eq!(error["error_code"], "operation_indeterminate");
    assert_eq!(error["state_changed"], true);
    assert!(!projects_dir.join("demo.toml").exists());
    assert!(project_dir.join("keep.txt").exists());

    let retry = project_ok(handle_project_lifecycle_op(
        &policy,
        &projects_dir,
        &project_request(
            "project_lifecycle_unregister",
            serde_json::json!({"project_id":"demo","expected_revision":registered["revision"]}),
        ),
    ));
    assert_eq!(retry["outcome"], "already_unregistered");
    assert_eq!(retry["changed"], false);
    assert!(!projects_dir.join("demo.toml").exists());
    assert!(project_dir.join("keep.txt").exists());
}

#[test]
fn register_project_rejects_path_outside_allowed_roots() {
    let allowed = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let projects_dir = allowed.path().join("projects.d");
    let policy = project_policy(allowed.path());
    let req = project_request(
        "register_project",
        serde_json::json!({
            "id": "outside",
            "name": "Outside",
            "path": outside.path().to_string_lossy()
        }),
    );

    let err = project_err(handle_project_op(&policy, &projects_dir, &req));
    assert_eq!(err, "path_outside_allowed_roots");
    assert!(!projects_dir.join("outside.toml").exists());
}

#[test]
fn register_project_rejects_dangerous_subpaths_without_explicit_root() {
    let policy = AgentPolicy {
        allow_cwd_anywhere: true,
        allowed_roots: Vec::new(),
        ..AgentPolicy::default()
    };

    // Dangerous system roots are platform-specific: the well-known Unix trees,
    // or the Windows OS trees (which must still be local-disk paths to reach
    // the dangerous-root check at all).
    #[cfg(windows)]
    let dangerous_paths: &[&str] = &[
        r"C:\Windows\System32\drivers\etc",
        r"C:\Program Files\WebCodex",
        r"C:\Program Files (x86)\something",
    ];
    #[cfg(not(windows))]
    let dangerous_paths: &[&str] = &[
        "/etc/nginx",
        "/usr/local",
        "/var/lib",
        "/proc/self",
        "/dev/shm",
    ];
    for path in dangerous_paths {
        let err = validate_project_path_policy(&policy, Path::new(path)).unwrap_err();
        assert!(err.contains("dangerous system root"), "{path}: {err}");
    }

    #[cfg(windows)]
    let safe_path = r"C:\Users\alice\projects";
    #[cfg(not(windows))]
    let safe_path = "/usr2/local";
    validate_project_path_policy(&policy, Path::new(safe_path)).unwrap();
}

#[test]
fn load_config_defaults_empty_allowed_roots_to_home() {
    let _guard = agent_init::TEST_ENV_LOCK.lock().unwrap();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if let Some(home) = home {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("agent.toml");
        std::fs::write(
            &path,
            "server_url = \"http://x\"\ntoken = \"t\"\nclient_id = \"c\"\n",
        )
        .unwrap();
        let cfg = load_config(&path).unwrap();
        assert_eq!(
            cfg.policy.allowed_roots,
            vec![home],
            "empty allowed_roots must default to HOME"
        );
    }
}

#[test]
fn load_config_defaults_allow_cwd_anywhere_to_false() {
    let _guard = agent_init::TEST_ENV_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let base = "server_url = \"http://x\"\ntoken = \"t\"\nclient_id = \"c\"\n";

    // A config that omits `[policy]` entirely falls back to
    // `AgentPolicy::default()`; one that has `[policy]` without the field
    // falls back to the per-field serde default. Both must fail closed —
    // otherwise the agent runs with no filesystem boundary at all.
    for (label, body) in [
        ("no [policy] section", base.to_string()),
        (
            "[policy] without allow_cwd_anywhere",
            format!("{base}\n[policy]\nallow_raw_shell = true\n"),
        ),
    ] {
        let path = tmp.path().join("agent.toml");
        std::fs::write(&path, body).unwrap();
        let cfg = load_config(&path).unwrap();
        assert!(
            !cfg.policy.allow_cwd_anywhere,
            "{label}: allow_cwd_anywhere must default to false"
        );
    }
}

#[test]
fn default_policy_denies_paths_outside_allowed_roots() {
    // The shipped default must not resolve an absolute path outside the
    // configured roots. `AgentPolicy::default()` has no roots at all, so
    // every path is out of bounds.
    let policy = AgentPolicy::default();
    assert!(!policy.allow_cwd_anywhere);
    let err = resolve_requested_path(&policy, Some("/tmp"), "/etc/passwd")
        .expect_err("default policy must not reach /etc/passwd");
    assert!(err.contains("outside allowed_roots"), "{err}");

    // With HOME as the root — what `effective_allowed_roots` fills in — a
    // path inside the root still resolves, so the default is restrictive
    // rather than broken.
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::write(root.join("in-bounds.txt"), "ok").unwrap();
    let scoped = AgentPolicy {
        allowed_roots: vec![root.clone()],
        ..AgentPolicy::default()
    };
    resolve_requested_path(&scoped, Some(root.to_str().unwrap()), "in-bounds.txt")
        .expect("in-bounds path must still resolve under the fail-closed default");
}

#[test]
fn load_config_explicit_allowed_roots_override_home_default() {
    let _guard = agent_init::TEST_ENV_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
            &path,
            "server_url = \"http://x\"\ntoken = \"t\"\nclient_id = \"c\"\n[policy]\nallowed_roots = [\"/root/git\"]\n",
        )
        .unwrap();
    let cfg = load_config(&path).unwrap();
    assert_eq!(
        cfg.policy.allowed_roots,
        vec![PathBuf::from("/root/git")],
        "explicit allowed_roots must override the HOME default"
    );
}

#[test]
fn load_config_empty_roots_without_home_and_no_cwd_anywhere_errors() {
    let _guard = agent_init::TEST_ENV_LOCK.lock().unwrap();
    // Windows derives the allowed-root default from USERPROFILE, so both
    // home sources must be absent to exercise the fail-closed branch.
    let _env = EnvGuard::new()
        .remove("HOME")
        .remove("USERPROFILE")
        .remove("APPDATA");
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("agent.toml");
    std::fs::write(
        &path,
        "server_url = \"http://x\"\ntoken = \"t\"\nclient_id = \"c\"\n\
             [policy]\nallow_cwd_anywhere = false\n",
    )
    .unwrap();
    let err = load_config(&path).unwrap_err();
    assert!(err.contains("allowed_roots is empty"));
}

#[test]
fn register_project_overwrite_semantics_are_accurate() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("repo");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&project_dir).unwrap();
    let policy = project_policy(tmp.path());
    let payload = |overwrite| {
        serde_json::json!({
            "id": "demo",
            "name": "Demo",
            "path": project_dir.to_string_lossy(),
            "overwrite": overwrite
        })
    };

    let first = project_ok(handle_project_op(
        &policy,
        &projects_dir,
        &project_request("register_project", payload(false)),
    ));
    assert_eq!(first["created_config"], true);
    assert_eq!(first["overwritten"], false);

    let retry = project_ok(handle_project_op(
        &policy,
        &projects_dir,
        &project_request("register_project", payload(false)),
    ));
    assert_eq!(retry["recovered"], true);
    assert_eq!(retry["changed"], false);

    let overwritten = project_ok(handle_project_op(
        &policy,
        &projects_dir,
        &project_request("register_project", payload(true)),
    ));
    assert_eq!(overwritten["created_config"], false);
    assert_eq!(overwritten["overwritten"], true);
}

#[test]
fn create_project_basic_creates_readme_and_gitignore() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("new-project");
    let projects_dir = tmp.path().join("projects.d");
    let policy = project_policy(tmp.path());
    let req = project_request(
        "create_project",
        serde_json::json!({
            "id": "basic",
            "name": "Basic",
            "path": project_dir.to_string_lossy(),
            "description": "Basic template",
            "template": "basic"
        }),
    );

    let value = project_ok(handle_project_op(&policy, &projects_dir, &req));
    assert_eq!(value["created_directory"], true);
    assert!(project_dir.join("README.md").exists());
    assert!(project_dir.join(".gitignore").exists());
    assert!(std::fs::read_to_string(project_dir.join("README.md"))
        .unwrap()
        .contains("Basic template"));
}

#[test]
fn create_project_rejects_existing_non_empty_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("existing");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&project_dir).unwrap();
    let keep = project_dir.join("keep.txt");
    std::fs::write(&keep, "keep").unwrap();
    let policy = project_policy(tmp.path());
    let req = project_request(
        "create_project",
        serde_json::json!({
            "id": "existing",
            "name": "Existing",
            "path": project_dir.to_string_lossy(),
            "template": "basic",
            "allow_existing_empty": true
        }),
    );

    let err = project_err(handle_project_op(&policy, &projects_dir, &req));
    assert_eq!(err, "path_not_empty");
    assert_eq!(std::fs::read_to_string(keep).unwrap(), "keep");
}

#[test]
fn create_project_rejects_unknown_template() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("new-project");
    let projects_dir = tmp.path().join("projects.d");
    let policy = project_policy(tmp.path());
    let req = project_request(
        "create_project",
        serde_json::json!({
            "id": "badtemplate",
            "name": "Bad Template",
            "path": project_dir.to_string_lossy(),
            "template": "cargo"
        }),
    );

    let err = project_err(handle_project_op(&policy, &projects_dir, &req));
    assert_eq!(err, "invalid_request");
    assert!(!project_dir.exists());
}

#[test]
fn create_project_created_config_and_overwritten_semantics_are_accurate() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("empty-project");
    let projects_dir = tmp.path().join("projects.d");
    let policy = project_policy(tmp.path());
    let payload = |overwrite| {
        serde_json::json!({
            "id": "empty",
            "name": "Empty",
            "path": project_dir.to_string_lossy(),
            "template": "empty",
            "allow_existing_empty": true,
            "overwrite": overwrite
        })
    };

    let first = project_ok(handle_project_op(
        &policy,
        &projects_dir,
        &project_request("create_project", payload(false)),
    ));
    assert_eq!(first["created_directory"], true);
    assert_eq!(first["created_config"], true);
    assert_eq!(first["overwritten"], false);

    let second = project_ok(handle_project_op(
        &policy,
        &projects_dir,
        &project_request("create_project", payload(true)),
    ));
    assert_eq!(second["created_directory"], false);
    assert_eq!(second["created_config"], false);
    assert_eq!(second["overwritten"], true);
}

#[test]
fn create_project_cleanup_removes_only_files_created_on_failure() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("existing-empty");
    std::fs::create_dir(&project_dir).unwrap();
    let projects_dir_file = tmp.path().join("projects.d-is-file");
    std::fs::write(&projects_dir_file, "not a dir").unwrap();
    let policy = project_policy(tmp.path());
    let req = project_request(
        "create_project",
        serde_json::json!({
            "id": "cleanup",
            "name": "Cleanup",
            "path": project_dir.to_string_lossy(),
            "template": "basic",
            "allow_existing_empty": true
        }),
    );

    let err = project_err(handle_project_op(&policy, &projects_dir_file, &req));
    assert_eq!(err, "operation_failed");
    assert!(project_dir.exists());
    assert!(!project_dir.join("README.md").exists());
    assert!(!project_dir.join(".gitignore").exists());
}

#[test]
fn create_project_does_not_delete_pre_existing_files() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("existing");
    std::fs::create_dir(&project_dir).unwrap();
    let pre_existing = project_dir.join("pre-existing.txt");
    std::fs::write(&pre_existing, "original").unwrap();
    let projects_dir_file = tmp.path().join("projects.d-is-file");
    std::fs::write(&projects_dir_file, "not a dir").unwrap();
    let policy = project_policy(tmp.path());
    let req = project_request(
        "create_project",
        serde_json::json!({
            "id": "keep",
            "name": "Keep",
            "path": project_dir.to_string_lossy(),
            "template": "basic",
            "allow_existing_empty": true
        }),
    );

    let err = project_err(handle_project_op(&policy, &projects_dir_file, &req));
    assert_eq!(err, "path_not_empty");
    assert_eq!(std::fs::read_to_string(pre_existing).unwrap(), "original");
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
