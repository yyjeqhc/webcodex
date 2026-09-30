use super::*;
use std::sync::{Barrier, Mutex};

const PROJECT: &str = "agent:msi:test";
// These fixtures launch real Windows PowerShell/PowerShell 7 processes. On
// small hosted runners, starting all eleven cold shells at once can exceed
// the production initialization deadline even though each shell is healthy.
// Serialize only this real-process test module; production concurrency and
// the rest of the Rust test suite remain unchanged.
static WINDOWS_PROCESS_TEST_LOCK: Mutex<()> = Mutex::new(());

fn windows_process_test_guard() -> std::sync::MutexGuard<'static, ()> {
    WINDOWS_PROCESS_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn launch(root: &Path, shell_id: &str, session_id: &str) -> ShellLaunch {
    launch_with_program(root, shell_id, session_id, "powershell.exe")
}

fn launch_with_program(
    root: &Path,
    shell_id: &str,
    session_id: &str,
    program: &str,
) -> ShellLaunch {
    let env = std::env::vars().collect::<HashMap<_, _>>();
    ShellLaunch {
        identity: ShellIdentity {
            shell_id: shell_id.to_string(),
            workflow_session_id: session_id.to_string(),
            runtime_project_id: PROJECT.to_string(),
            executor: "agent".to_string(),
            client_id: Some("msi".to_string()),
        },
        dialect: "powershell".to_string(),
        profile: None,
        program: program.to_string(),
        args: vec![
            "-NoProfile".to_string(),
            "-NonInteractive".to_string(),
            "-ExecutionPolicy".to_string(),
            "Bypass".to_string(),
        ],
        initial_cwd: root.to_path_buf(),
        env,
        initialization: None,
        max_output_bytes: 64 * 1024,
    }
}

fn exec(
    manager: &PersistentShellManager,
    shell_id: &str,
    session_id: &str,
    command: &str,
) -> ShellExecResult {
    manager
        .exec(
            shell_id,
            session_id,
            PROJECT,
            command,
            Duration::from_secs(5),
        )
        .unwrap()
}

fn process_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return false;
    }
    let mut exit_code = 0u32;
    let ok = unsafe { GetExitCodeProcess(handle, &mut exit_code) };
    unsafe { CloseHandle(handle) };
    ok == 1 && exit_code == 259
}

#[test]
fn state_stdout_stderr_and_unicode_persist() {
    let _guard = windows_process_test_guard();
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("sub")).unwrap();
    let manager = PersistentShellManager::new(ShellLimits::default());
    manager
        .open(launch(temp.path(), "wc_shell_state", "wc_sess_state"))
        .unwrap();

    let set = exec(
            &manager,
            "wc_shell_state",
            "wc_sess_state",
            "$env:WEBCODEX_PERSIST_TEST='alpha'; Set-Location -LiteralPath 'sub'; $WC_LOCAL='beta'; function WC_FN { [Console]::Out.Write('fn') }",
        );
    assert_eq!(set.exit_code, Some(0));
    let observed = exec(
            &manager,
            "wc_shell_state",
            "wc_sess_state",
            "[Console]::Out.Write($env:WEBCODEX_PERSIST_TEST + '|' + (Get-Location).Path + '|' + $WC_LOCAL + '|'); WC_FN; [Console]::Error.Write('stderr-only')",
        );
    assert!(observed.stdout.starts_with("alpha|"), "{}", observed.stdout);
    assert!(
        observed.stdout.contains("\\sub|beta|fn"),
        "{}",
        observed.stdout
    );
    assert_eq!(observed.stderr, "stderr-only");

    let unicode = exec(
        &manager,
        "wc_shell_state",
        "wc_sess_state",
        "[Console]::Out.Write(\"hello`r`n中文`r`n🙂`r`nmixed ASCII + Unicode\")",
    );
    assert_eq!(
        unicode.stdout,
        "hello\r\n中文\r\n🙂\r\nmixed ASCII + Unicode"
    );
    assert!(!unicode.stdout.contains("WCPSO1"));
    assert!(!unicode.stderr.contains("WCPSE1"));
    let host_unicode = exec(
        &manager,
        "wc_shell_state",
        "wc_sess_state",
        "Write-Output '中文🙂 host-output'",
    );
    assert!(
        host_unicode.stdout.contains("中文🙂 host-output"),
        "{:?}",
        host_unicode.stdout
    );
}

#[test]
fn command_failure_does_not_lose_shell() {
    let _guard = windows_process_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let manager = PersistentShellManager::new(ShellLimits::default());
    manager
        .open(launch(temp.path(), "wc_shell_failure", "wc_sess_failure"))
        .unwrap();
    let failed = exec(
        &manager,
        "wc_shell_failure",
        "wc_sess_failure",
        "Write-Error 'expected-failure'",
    );
    assert_eq!(failed.exit_code, Some(1));
    assert_eq!(failed.shell_state, ShellState::Running);
    assert!(
        failed.stderr.contains("expected-failure"),
        "stdout={:?} stderr={:?}",
        failed.stdout,
        failed.stderr
    );
    let recovered = exec(
        &manager,
        "wc_shell_failure",
        "wc_sess_failure",
        "Write-Error 'transient-error'; [Console]::Out.Write('recovered')",
    );
    assert_eq!(recovered.exit_code, Some(0));
    assert_eq!(recovered.stdout, "recovered");
    assert!(recovered.stderr.contains("transient-error"));

    let native_failed = exec(
        &manager,
        "wc_shell_failure",
        "wc_sess_failure",
        "cmd.exe /d /c exit 5",
    );
    assert_eq!(native_failed.exit_code, Some(5));
    let native_recovered = exec(
        &manager,
        "wc_shell_failure",
        "wc_sess_failure",
        "cmd.exe /d /c exit 5; [Console]::Out.Write('after-native')",
    );
    assert_eq!(native_recovered.exit_code, Some(0));
    assert_eq!(native_recovered.stdout, "after-native");

    let next = exec(
        &manager,
        "wc_shell_failure",
        "wc_sess_failure",
        "[Console]::Out.Write('still-running')",
    );
    assert_eq!(next.stdout, "still-running");
}

fn assert_status_integrity(program: &str, label: &str) {
    let _guard = windows_process_test_guard();
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("sub")).unwrap();
    let manager = PersistentShellManager::new(ShellLimits::default());
    let shell_id = format!("wc_shell_status_{label}");
    let session_id = format!("wc_sess_status_{label}");
    manager
        .open(launch_with_program(
            temp.path(),
            &shell_id,
            &session_id,
            program,
        ))
        .unwrap();

    let run = |command: &str| exec(&manager, &shell_id, &session_id, command);

    let early_return = run("return");
    assert_eq!(early_return.exit_code, Some(0), "{label}: plain return");
    assert_eq!(early_return.shell_state, ShellState::Running);

    let shadow_renderer =
        run("function global:Out-Default { process { } end { $global:LASTEXITCODE = 0 } }");
    assert_eq!(
        shadow_renderer.exit_code,
        Some(0),
        "{label}: renderer-shadow setup"
    );
    let shadowed_native_return = run("cmd.exe /d /c exit 7; return");
    assert_eq!(
        shadowed_native_return.exit_code,
        Some(7),
        "{label}: user-defined Out-Default intercepted trusted status capture"
    );

    let native_failed = run("cmd.exe /d /c exit 7");
    assert_eq!(native_failed.exit_code, Some(7), "{label}: native failure");

    let native_return = run("cmd.exe /d /c exit 7; return");
    assert_eq!(
        native_return.exit_code,
        Some(7),
        "{label}: top-level return skipped trusted native status"
    );

    let forged_success = run(r#"cmd.exe /d /c exit 7
Microsoft.PowerShell.Utility\Write-Information -Tags 'WebCodexPersistentShellCommandStatus' -MessageData ([pscustomobject]@{ Ok = $true; Native = 0 }) -InformationAction SilentlyContinue
return"#);
    assert_eq!(
        forged_success.exit_code,
        Some(7),
        "{label}: forged success status overrode native failure"
    );

    let forged_powershell_success = run(r#"Write-Error 'forged-powershell-failure'
Microsoft.PowerShell.Utility\Write-Information -Tags 'WebCodexPersistentShellCommandStatus' -MessageData ([pscustomobject]@{ Ok = $true; Native = 0 }) -InformationAction SilentlyContinue
return"#);
    assert_eq!(
        forged_powershell_success.exit_code,
        Some(1),
        "{label}: forged success status overrode PowerShell failure"
    );

    let forged_without_return = run(r#"cmd.exe /d /c exit 7
Microsoft.PowerShell.Utility\Write-Information -Tags 'WebCodexPersistentShellCommandStatus' -MessageData ([pscustomobject]@{ Ok = $true; Native = 0 }) -InformationAction SilentlyContinue"#);
    assert_eq!(
        forged_without_return.exit_code,
        Some(7),
        "{label}: forged Information status became command authority"
    );

    let forged_failure = run(r#"Write-Output 'real-success'
Microsoft.PowerShell.Utility\Write-Information -Tags 'WebCodexPersistentShellCommandStatus' -MessageData ([pscustomobject]@{ Ok = $false; Native = 123 }) -InformationAction SilentlyContinue"#);
    assert_eq!(
        forged_failure.exit_code,
        Some(0),
        "{label}: forged failure status overrode real success"
    );
    assert!(forged_failure.stdout.contains("real-success"));

    let non_terminating = run("Write-Error 'status-non-terminating'");
    assert_eq!(
        non_terminating.exit_code,
        Some(1),
        "{label}: non-terminating error"
    );
    let recovered = run("Write-Error 'status-recovered'; Write-Output 'recovered'");
    assert_eq!(
        recovered.exit_code,
        Some(0),
        "{label}: historical non-terminating error changed final success semantics"
    );
    assert!(recovered.stdout.contains("recovered"));

    let terminating = run("throw 'status-terminating'");
    assert_eq!(terminating.exit_code, Some(1), "{label}: terminating error");
    assert_eq!(terminating.shell_state, ShellState::Running);

    let parse_failure = run("if (");
    assert_eq!(parse_failure.exit_code, Some(1), "{label}: parse failure");
    assert_eq!(parse_failure.shell_state, ShellState::Running);

    assert_eq!(
        run("cmd.exe /d /c exit 0").exit_code,
        Some(0),
        "{label}: successful native command"
    );
    assert_eq!(
        run("Write-Output 'ordinary-success'").exit_code,
        Some(0),
        "{label}: ordinary PowerShell success"
    );

    let state_set = run(
            "$WC_STATUS_LOCAL='alpha'; function WC_STATUS_FN { [Console]::Out.Write('fn') }; Set-Location -LiteralPath 'sub'",
        );
    assert_eq!(state_set.exit_code, Some(0), "{label}: state setup");
    let state_observed = run(
        "[Console]::Out.Write($WC_STATUS_LOCAL + '|' + (Get-Location).Path + '|'); WC_STATUS_FN",
    );
    assert!(
        state_observed.stdout.contains("alpha|") && state_observed.stdout.contains("\\sub|fn"),
        "{label}: persistent variable/function/cwd state regressed: {:?}",
        state_observed.stdout
    );

    let next = run("Write-Output 'after-adversarial'");
    assert_eq!(next.exit_code, Some(0), "{label}: shell did not recover");
    assert!(next.stdout.contains("after-adversarial"));
    assert_eq!(next.shell_state, ShellState::Running);
}

#[test]
#[ignore = "manual adversarial real-process test: exercises many host-status forgery cases"]
fn user_command_status_is_host_authoritative() {
    assert_status_integrity("powershell.exe", "windows_powershell");
}

#[test]
#[ignore = "manual PowerShell 7 compatibility: duplicates real-process status coverage"]
fn configured_pwsh_user_command_status_is_host_authoritative() {
    let Ok(program) = std::env::var("WEBCODEX_TEST_PWSH") else {
        return;
    };
    assert!(
        Path::new(&program).is_file(),
        "configured pwsh does not exist: {program}"
    );
    assert_status_integrity(&program, "powershell_7");
}

#[test]
fn shell_exit_is_terminal_and_next_exec_fails_closed() {
    let _guard = windows_process_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let manager = PersistentShellManager::new(ShellLimits::default());
    manager
        .open(launch(temp.path(), "wc_shell_exit", "wc_sess_exit"))
        .unwrap();
    let exited = exec(&manager, "wc_shell_exit", "wc_sess_exit", "exit 7");
    assert_eq!(exited.shell_state, ShellState::Exited);
    assert_eq!(exited.exit_code, Some(7));
    let error = manager
        .exec(
            "wc_shell_exit",
            "wc_sess_exit",
            PROJECT,
            "[Console]::Out.Write('forbidden')",
            Duration::from_secs(1),
        )
        .unwrap_err();
    assert_eq!(error.code, "persistent_shell_stale");
}

#[test]
#[ignore = "manual real-process timing: validates a real PowerShell timeout boundary"]
fn timeout_poisoning_is_bounded_and_never_reuses_uncertain_stream() {
    let _guard = windows_process_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let manager = PersistentShellManager::new(ShellLimits::default());
    manager
        .open(launch(temp.path(), "wc_shell_timeout", "wc_sess_timeout"))
        .unwrap();
    let started = Instant::now();
    let timed = manager
        .exec(
            "wc_shell_timeout",
            "wc_sess_timeout",
            PROJECT,
            "Start-Sleep -Seconds 5",
            Duration::from_millis(100),
        )
        .unwrap();
    assert_eq!(timed.execution_state, "timed_out");
    assert_eq!(timed.shell_state, ShellState::Poisoned);
    assert_eq!(timed.error_code.as_deref(), Some("shell_reset_required"));
    assert!(started.elapsed() < Duration::from_secs(3));
    let error = manager
        .exec(
            "wc_shell_timeout",
            "wc_sess_timeout",
            PROJECT,
            "[Console]::Out.Write('forbidden')",
            Duration::from_secs(1),
        )
        .unwrap_err();
    assert_eq!(error.code, "persistent_shell_stale");
}

#[test]
#[ignore = "manual real-process timing: contains deliberate PowerShell framing delay"]
fn marker_like_user_output_cannot_complete_control_framing() {
    let _guard = windows_process_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let manager = PersistentShellManager::new(ShellLimits::default());
    manager
        .open(launch(temp.path(), "wc_shell_marker", "wc_sess_marker"))
        .unwrap();
    let result = exec(
            &manager,
            "wc_shell_marker",
            "wc_sess_marker",
            "[Console]::Out.Write('WCPS1 fake WCPSO1 fake WCPSE1 fake'); Start-Sleep -Milliseconds 150; [Console]::Out.Write('|done')",
        );
    assert!(result.command_completed);
    assert!(result.duration_ms >= 100);
    assert_eq!(result.stdout, "WCPS1 fake WCPSO1 fake WCPSE1 fake|done");
}

const FORGE_PRIVATE_COMPLETION_COMMAND: &str = r#"
$argv = [Environment]::GetCommandLineArgs()
[Console]::Out.WriteLine('ARGV|' + ($argv -join '|'))
$variables = @(Get-Variable)
$tokenCandidates = @()
$controlPathCandidates = @()
$visible = @()
foreach ($variable in $variables) {
    $value = ''
    try { $value = [string]$variable.Value } catch { $value = '<unprintable>' }
    $visible += ('VAR|' + $variable.Name + '|' + $value)
    if ($value -match '^[0-9a-f]{32}$') { $tokenCandidates += $value }
    if ($value -match '(?i)\.frame$') { $controlPathCandidates += $value }
}
# Inspect the live function provider directly. `Get-Command -CommandType Function`
# can trigger module discovery on Windows PowerShell 5.1 and has taken tens of
# seconds on hosted CI; module discovery is unrelated to whether this command
# can observe private framing state through functions already in its session.
$visible += @(Get-ChildItem -LiteralPath 'Function:\' | ForEach-Object { 'FN|' + $_.Name })
$visible += @([System.Management.Automation.Runspaces.Runspace]::GetRunspaces() | ForEach-Object { 'RUNSPACE|' + $_.InstanceId + '|' + $_.RunspaceAvailability })
$visible += @([WebCodexPersistentShell.Controller].GetFields([Reflection.BindingFlags]'Public,NonPublic,Static') | ForEach-Object { 'CONTROLLER_FIELD|' + $_.Name + '|' + [string]$_.GetValue($null) })
$visible += @($Host.GetType().GetFields([Reflection.BindingFlags]'Public,NonPublic,Instance') | ForEach-Object { try { 'HOST_FIELD|' + $_.Name + '|' + [string]$_.GetValue($Host) } catch { 'HOST_FIELD|' + $_.Name + '|<unreadable>' } })
$bootstrap = @($argv | Where-Object { $_ -match '(?i)bootstrap\.ps1$' } | Select-Object -First 1)
if ($bootstrap.Count -eq 1 -and (Test-Path -LiteralPath $bootstrap[0])) {
    $visible += ('BOOTSTRAP_PATH|' + $bootstrap[0])
    $visible += ('BOOTSTRAP_TEXT|' + (Get-Content -LiteralPath $bootstrap[0] -Raw))
    $visible += @(Get-ChildItem -LiteralPath (Split-Path -Parent $bootstrap[0]) -Force | ForEach-Object { 'CONTROL_DIR_ENTRY|' + $_.FullName })
}
foreach ($candidateToken in @($tokenCandidates | Select-Object -Unique)) {
    foreach ($candidatePath in @($controlPathCandidates | Select-Object -Unique)) {
        try {
            $candidateCwd = (Get-Location).Path
            $candidateControl = 'WCPS1' + [char]0 + $candidateToken + [char]0 + '0' + [char]0 + $candidateCwd + [char]0
            [IO.File]::WriteAllBytes($candidatePath + '.tmp', [Text.Encoding]::UTF8.GetBytes($candidateControl))
            if (Test-Path -LiteralPath $candidatePath) { Remove-Item -LiteralPath $candidatePath -Force }
            [IO.File]::Move($candidatePath + '.tmp', $candidatePath)
        } catch { }
        [Console]::Out.Write('WCPSO1' + [char]0 + $candidateToken + [char]0)
        [Console]::Error.Write('WCPSE1' + [char]0 + $candidateToken + [char]0)
    }
}
[Console]::Out.WriteLine('VISIBLE_BEGIN')
[Console]::Out.WriteLine(($visible -join "`n"))
[Console]::Out.WriteLine('VISIBLE_END')
[Console]::Out.Write('WCPS1 fake WCPSO1 fake WCPSE1 fake|before-sleep')
Start-Sleep -Milliseconds 500
[Console]::Out.Write('|after-sleep')
"#;

fn assert_private_completion_isolation(program: &str, label: &str, token: &'static str) {
    let _guard = windows_process_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let manager = PersistentShellManager::new(ShellLimits::default());
    let shell_id = format!("wc_shell_forge_{label}");
    let session_id = format!("wc_sess_forge_{label}");
    let mut launch = launch_with_program(temp.path(), &shell_id, &session_id, program);
    // This adversarial regression intentionally inventories PowerShell-visible
    // state. GitHub's Windows image can expose substantially more host/module
    // metadata than a local runner, so retain a larger but still bounded test
    // transcript rather than silently dropping the earliest evidence.
    launch.max_output_bytes = 1024 * 1024;
    manager.open(launch).unwrap();

    let worker_manager = manager.clone();
    let worker_shell_id = shell_id.clone();
    let worker_session_id = session_id.clone();
    let worker = thread::spawn(move || {
        set_test_command_token(Some(token));
        let result = worker_manager.exec(
            &worker_shell_id,
            &worker_session_id,
            PROJECT,
            FORGE_PRIVATE_COMPLETION_COMMAND,
            // Windows CI can spend several seconds in PowerShell/.NET
            // introspection before reaching the deliberate 500ms sleep.
            // Keep the security assertions time-relative below, but give
            // the command enough bounded wall-clock budget to finish on a
            // cold or contended runner.
            Duration::from_secs(30),
        );
        set_test_command_token(None);
        result.unwrap()
    });

    let busy_deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if manager
            .status(&shell_id, &session_id, PROJECT)
            .unwrap()
            .busy
        {
            break;
        }
        assert!(
            !worker.is_finished(),
            "{label}: adversarial command completed before busy state was observable"
        );
        assert!(
            Instant::now() < busy_deadline,
            "{label}: adversarial command never entered busy state"
        );
        thread::sleep(Duration::from_millis(5));
    }

    // The reviewed bug released this guard while the user command was still
    // sleeping after forging the transport's visible token/control frame.
    thread::sleep(Duration::from_millis(150));
    let busy = manager
        .exec(
            &shell_id,
            &session_id,
            PROJECT,
            "[Console]::Out.Write('must-not-run')",
            Duration::from_secs(1),
        )
        .unwrap_err();
    assert_eq!(
        busy.code, "shell_busy",
        "{label}: busy guard released early"
    );

    let result = worker.join().unwrap();
    assert!(
        result.command_completed,
        "{label}: command did not complete: execution_state={} shell_state={:?} error_code={:?}",
        result.execution_state, result.shell_state, result.error_code
    );
    assert_eq!(result.shell_state, ShellState::Running);
    assert!(
        !result.stdout_truncated && !result.stderr_truncated,
        "{label}: adversarial visibility evidence was truncated"
    );
    assert!(
        result.duration_ms >= 400,
        "{label}: completion arrived before the 500ms user sleep returned: {}ms",
        result.duration_ms
    );
    assert!(
        result.stdout.contains("|after-sleep"),
        "{label}: trailing user output was not attributed to the command: {:?}",
        result.stdout
    );
    assert!(result.stdout.contains("WCPS1 fake WCPSO1 fake WCPSE1 fake"));
    assert!(
        !result.stdout.contains(token) && !result.stderr.contains(token),
        "{label}: active correlation token leaked into user-visible state"
    );

    let argv_line = result
        .stdout
        .lines()
        .find(|line| line.starts_with("ARGV|"))
        .unwrap_or_else(|| panic!("{label}: process argv was not observed"));
    let bootstrap = argv_line
        .split('|')
        .skip(1)
        .find(|value| value.to_ascii_lowercase().ends_with("bootstrap.ps1"))
        .unwrap_or_else(|| panic!("{label}: bootstrap path was not discoverable from argv"));
    let expected_control_path = Path::new(bootstrap)
        .parent()
        .unwrap()
        .join(format!("{token}.frame"))
        .to_string_lossy()
        .to_string();
    let visible_output = format!("{}\n{}", result.stdout, result.stderr).to_ascii_lowercase();
    assert!(
        !visible_output.contains(&expected_control_path.to_ascii_lowercase()),
        "{label}: active control publication target leaked into user-visible state"
    );
    assert!(
        result.stdout.contains("BOOTSTRAP_TEXT|"),
        "{label}: test did not inspect the ordinary bootstrap file"
    );

    let clean = exec(
        &manager,
        &shell_id,
        &session_id,
        "[Console]::Out.Write('clean')",
    );
    assert_eq!(
        clean.stdout, "clean",
        "{label}: prior stdout leaked forward"
    );
    assert!(
        clean.stderr.is_empty(),
        "{label}: prior stderr leaked forward"
    );
}

#[test]
#[ignore = "manual adversarial real-process test: heavy PowerShell introspection and timing"]
fn user_command_cannot_forge_private_completion() {
    assert_private_completion_isolation(
        "powershell.exe",
        "windows_powershell",
        "13579bdf2468ace013579bdf2468ace0",
    );
}

#[test]
#[ignore = "manual PowerShell 7 adversarial test: heavy introspection and timing"]
fn configured_pwsh_user_command_cannot_forge_private_completion() {
    let Ok(program) = std::env::var("WEBCODEX_TEST_PWSH") else {
        return;
    };
    assert!(
        Path::new(&program).is_file(),
        "configured pwsh does not exist: {program}"
    );
    assert_private_completion_isolation(
        &program,
        "powershell_7",
        "02468ace13579bdf02468ace13579bdf",
    );
}

#[test]
#[ignore = "manual real-process timing: depends on concurrent PowerShell scheduling"]
fn concurrent_exec_is_serialized_by_busy_guard() {
    let _guard = windows_process_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let manager = PersistentShellManager::new(ShellLimits::default());
    manager
        .open(launch(temp.path(), "wc_shell_busy_win", "wc_sess_busy_win"))
        .unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let worker_manager = manager.clone();
    let worker_barrier = Arc::clone(&barrier);
    let worker = thread::spawn(move || {
        worker_barrier.wait();
        worker_manager
            .exec(
                "wc_shell_busy_win",
                "wc_sess_busy_win",
                PROJECT,
                "Start-Sleep -Milliseconds 300; [Console]::Out.Write('first')",
                Duration::from_secs(2),
            )
            .unwrap()
    });
    barrier.wait();
    while !manager
        .status("wc_shell_busy_win", "wc_sess_busy_win", PROJECT)
        .unwrap()
        .busy
    {
        thread::yield_now();
    }
    let busy = manager
        .exec(
            "wc_shell_busy_win",
            "wc_sess_busy_win",
            PROJECT,
            "[Console]::Out.Write('second')",
            Duration::from_secs(1),
        )
        .unwrap_err();
    assert_eq!(busy.code, "shell_busy");
    assert_eq!(worker.join().unwrap().stdout, "first");
}

#[test]
#[ignore = "manual real-process lifecycle: validates descendant liveness and teardown"]
fn close_is_idempotent_and_kills_owned_descendants() {
    let _guard = windows_process_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let manager = PersistentShellManager::new(ShellLimits::default());
    manager
        .open(launch(
            temp.path(),
            "wc_shell_close_win",
            "wc_sess_close_win",
        ))
        .unwrap();
    let child = exec(
            &manager,
            "wc_shell_close_win",
            "wc_sess_close_win",
            "$p = Start-Process -FilePath 'powershell.exe' -ArgumentList '-NoProfile','-NonInteractive','-Command','Start-Sleep -Seconds 30' -PassThru; [Console]::Out.Write($p.Id)",
        );
    let child_pid: u32 = child.stdout.parse().unwrap();
    assert!(process_alive(child_pid));
    let closed = manager
        .close(
            "wc_shell_close_win",
            "wc_sess_close_win",
            PROJECT,
            "explicit_close",
        )
        .unwrap();
    assert_eq!(closed.summary.state, ShellState::Closed);
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline && process_alive(child_pid) {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !process_alive(child_pid),
        "owned child {child_pid} leaked after close"
    );
    let again = manager
        .close(
            "wc_shell_close_win",
            "wc_sess_close_win",
            PROJECT,
            "explicit_close",
        )
        .unwrap();
    assert!(again.already_closed);
}
