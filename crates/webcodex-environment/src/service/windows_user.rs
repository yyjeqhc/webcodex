//! Per-user interactive Task Scheduler adapter. No password, UAC or SCM account.
//! A fixed native script consumes bounded non-secret JSON as child-local data;
//! callers cannot supply script text. New tasks are registered create-only.
use super::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const SCRIPT: &str = include_str!("windows_user.ps1");

fn task_request(spec: &ServiceSpec, operation: &str) -> Result<Value, ServiceError> {
    let (sid, _) = scope::user_account(spec)?;
    let quote = |value: &str| -> String {
        let mut out = String::from("\"");
        let mut slashes = 0;
        for ch in value.chars() {
            match ch {
                '\\' => slashes += 1,
                '"' => {
                    out.push_str(&"\\".repeat(slashes * 2 + 1));
                    out.push('"');
                    slashes = 0;
                }
                _ => {
                    out.push_str(&"\\".repeat(slashes));
                    slashes = 0;
                    out.push(ch);
                }
            }
        }
        out.push_str(&"\\".repeat(slashes * 2));
        out.push('"');
        out
    };
    let name = task_name(spec, sid);
    let arguments = spec
        .args
        .iter()
        .map(|value| quote(value))
        .collect::<Vec<_>>()
        .join(" ");
    let host = desktop_task_host(&spec.program);
    let program = spec.program.to_str().ok_or_else(|| {
        ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "Task executable must be UTF-8",
        )
    })?;
    let directory = spec.working_directory.to_str().ok_or_else(|| {
        ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "Task directory must be UTF-8",
        )
    })?;
    let host_arguments = format!(
        "--user-task {} {} {} {} {}",
        quote(spec.component.as_str()),
        quote(&spec.config_identity),
        quote(directory),
        quote(program),
        arguments
    );
    Ok(json!({"operation":operation, "name":name,
        "description":format!("{} scope=user", ownership_marker(spec)), "sid":sid,
        "program":program,
        "arguments":arguments, "host":host, "host_arguments":host_arguments,
        "directory":directory}))
}

// The installed GUI and its runtime are verified and updated as one package.
fn desktop_task_host(program: &Path) -> Option<PathBuf> {
    let runtime = program.parent()?;
    if runtime.file_name()? != "webcodex-runtime" {
        return None;
    }
    let host = runtime.parent()?.join("WebCodex.exe");
    #[cfg(windows)]
    {
        // A custom Runtime folder cannot designate its sibling as a launcher.
        use crate::unified_update::{installed_desktop_path, InstallerTarget, RuntimePlatform};
        let current = std::env::current_exe().ok()?.canonicalize().ok()?;
        let host_path = host.canonicalize().ok()?;
        if current != host_path {
            let target = InstallerTarget::default_for_non_linux(RuntimePlatform::current()?)?;
            if installed_desktop_path(target)?.canonicalize().ok()? != host_path {
                return None;
            }
        }
    }
    Some(host)
}

#[cfg(windows)]
pub fn run_desktop_user_task(args: &[String]) -> Result<i32, Box<dyn std::error::Error>> {
    use crate::{service_spec, tunnel_service_spec, EnvironmentStore};
    use std::process::{Command, Stdio};
    let [component, identity, directory, program, worker_args @ ..] = args else {
        return Err("Invalid task arguments".into());
    };
    let directory = PathBuf::from(directory);
    if !directory.is_absolute()
        || directory.canonicalize()? != std::env::current_dir()?.canonicalize()?
    {
        return Err("Task working directory changed".into());
    }
    let (component, root) = match component.as_str() {
        "server" => (Component::Server, directory.parent()),
        "runner" => (Component::Runner, Some(directory.as_path())),
        "tunnel" => (Component::Tunnel, directory.ancestors().nth(3)),
        _ => return Err("Invalid service component".into()),
    };
    let store = EnvironmentStore::open_existing(root.ok_or("Missing task root")?.into())?
        .ok_or("Missing task environment")?;
    let record = match store.load_environment()? {
        Some(record) => record,
        None => {
            store
                .load_journal()?
                .ok_or("Missing setup journal")?
                .environment
        }
    };
    if record.request.service_scope != ServiceScope::User {
        return Err("Task service scope changed".into());
    }
    let spec = if component == Component::Tunnel {
        let profile = directory
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("Missing tunnel profile")?;
        tunnel_service_spec(&store, &record, profile)?
    } else {
        service_spec(&store, &record, component)?
    };
    let expected_host = desktop_task_host(&spec.program).ok_or("Missing packaged task host")?;
    if spec.config_identity != *identity
        || spec.program != Path::new(program)
        || spec.args != worker_args
        || spec.working_directory != directory
        || expected_host.canonicalize()? != std::env::current_exe()?.canonicalize()?
    {
        return Err("Task target changed".into());
    }
    scope::verify_current_user(&spec)?;
    check_install_files(&spec)?;
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .current_dir(&directory)
        .stdin(Stdio::null())
        .env("WEBCODEX_SERVICE_LOG_DIR", &directory);
    // Preserve inherited diagnostic handles. The child writes bounded private
    // service events when Scheduler supplies no console or redirected streams.
    Ok(run_console_free_worker(&mut command)?)
}

#[cfg(windows)]
fn run_console_free_worker(command: &mut std::process::Command) -> std::io::Result<i32> {
    let mut child = webcodex_process::ManagedChild::spawn_background(command)?;
    Ok(child.wait()?.code().unwrap_or(1))
}

/// Pure saved task identity, also used by read-only diagnostic references.
pub(super) fn task_name(spec: &ServiceSpec, sid: &str) -> String {
    let identity = format!("{sid}:{}", spec.config_identity);
    let suffix = format!("{:x}", Sha256::digest(identity.as_bytes()));
    format!("WebCodex-user-{}-{}", &suffix[..16], spec.id)
}

fn decode_status(spec: &ServiceSpec, value: &Value) -> Result<ServiceStatus, ServiceError> {
    if value.get("error").is_some() {
        return Err(ServiceError::new(
            ServiceErrorCode::OutcomeUnknown,
            "Task Scheduler did not confirm the operation; inspect before retrying",
        ));
    }
    if value["exists"].as_bool() == Some(false) {
        return Ok(ServiceStatus::absent(&spec.id));
    }
    if value["exists"].as_bool() != Some(true) {
        return Err(ServiceError::new(
            ServiceErrorCode::OwnershipUnknown,
            "Task status omitted its existence",
        ));
    }
    let owned = value["owned"].as_bool().ok_or_else(|| {
        ServiceError::new(
            ServiceErrorCode::OwnershipUnknown,
            "Task ownership was not observed",
        )
    })?;
    let running = match value["state"].as_u64() {
        Some(2 | 4) => Some(true),  // queued/running
        Some(1 | 3) => Some(false), // disabled/ready
        _ => None,
    };
    let mut detail = String::from(
        "user Task Scheduler; runs only while this owner is signed in; no password stored",
    );
    if owned && running == Some(false) {
        if let Some(result) = value["last_task_result"]
            .as_i64()
            .filter(|result| *result != 0)
        {
            detail.push_str(&format!("; last Task Scheduler result {result}"));
        }
    }
    Ok(ServiceStatus {
        id: spec.id.clone(),
        ownership: if owned {
            Ownership::Owned
        } else {
            Ownership::Foreign
        },
        installed: true,
        enabled: value["enabled"].as_bool(),
        running,
        detail: Some(detail),
    })
}

/// A running task can be queued, and its engine PID need not be its action PID.
/// Accept only the exact live action observation made by the native adapter.
fn decode_running_pid(spec: &ServiceSpec, value: &Value) -> Result<Option<u32>, ServiceError> {
    let status = decode_status(spec, value)?;
    if status.ownership != Ownership::Owned || value["state"].as_u64() != Some(4) {
        return Ok(None);
    }
    if value.get("process_id").is_none_or(Value::is_null) {
        return Ok(None);
    }
    let pid = value["process_id"]
        .as_u64()
        .and_then(|pid| u32::try_from(pid).ok())
        .filter(|pid| *pid != 0);
    let instance = value["instance_guid"]
        .as_str()
        .and_then(|id| uuid::Uuid::parse_str(id).ok())
        .filter(|id| !id.is_nil());
    match (pid, instance) {
        (Some(pid), Some(_)) => Ok(Some(pid)),
        _ => Err(ServiceError::new(
            ServiceErrorCode::OwnershipUnknown,
            "Task Scheduler did not confirm a live action process",
        )),
    }
}

#[cfg(any(windows, test))]
fn decode_status_with_pid(
    spec: &ServiceSpec,
    value: &Value,
) -> Result<(ServiceStatus, Option<u32>), ServiceError> {
    Ok((
        decode_status(spec, value)?,
        decode_running_pid(spec, value)?,
    ))
}

#[cfg(windows)]
fn call_value(spec: &ServiceSpec, operation: &str) -> Result<Value, ServiceError> {
    use crate::process::CommandOutputExt;
    use std::os::windows::process::CommandExt;
    scope::verify_current_user(spec)?;
    if !spec.environment.is_empty() {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "User Task Scheduler does not inject environment values",
        ));
    }
    let request = serde_json::to_string(&task_request(spec, operation)?)
        .map_err(|_| ServiceError::new(ServiceErrorCode::InvalidSpec, "Cannot encode task plan"))?;
    if request.encode_utf16().count() > 24_000 {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "Task plan exceeds its native input budget",
        ));
    }
    // Fixed system executable, not a PATH-resolved PowerShell from a repository.
    let root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .filter(|root| root.is_absolute())
        .ok_or_else(|| {
            ServiceError::new(
                ServiceErrorCode::MissingPrerequisite,
                "Windows system root unavailable",
            )
        })?;
    let mut command =
        std::process::Command::new(root.join("System32/WindowsPowerShell/v1.0/powershell.exe"));
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            SCRIPT,
        ])
        .env("WEBCODEX_SERVICE_REQUEST", request)
        .creation_flags(0x08000000);
    let output = command.bounded_output().map_err(|_| {
        ServiceError::new(
            ServiceErrorCode::OutcomeUnknown,
            "Task Scheduler command incomplete; inspect without replay",
        )
    })?;
    if !output.status.success() {
        return Err(ServiceError::new(
            ServiceErrorCode::OutcomeUnknown,
            "Task Scheduler request failed; inspect before retrying",
        ));
    }
    let value = serde_json::from_slice(&output.stdout).map_err(|_| {
        ServiceError::new(
            ServiceErrorCode::OwnershipUnknown,
            "Task Scheduler returned no valid bounded observation",
        )
    })?;
    Ok(value)
}

#[cfg(windows)]
fn call(spec: &ServiceSpec, operation: &str) -> Result<ServiceStatus, ServiceError> {
    decode_status(spec, &call_value(spec, operation)?)
}

#[cfg(windows)]
pub(super) fn running_pid(spec: &ServiceSpec) -> Result<Option<u32>, ServiceError> {
    decode_running_pid(spec, &call_value(spec, "running_pid")?)
}

#[cfg(windows)]
pub(super) fn inspect_with_pid(
    spec: &ServiceSpec,
) -> Result<(ServiceStatus, Option<u32>), ServiceError> {
    let value = call_value(spec, "running_pid")?;
    decode_status_with_pid(spec, &value)
}

#[cfg(windows)]
pub(super) fn inspect(spec: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
    call(spec, "inspect")
}
#[cfg(windows)]
pub(super) fn preflight(spec: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
    check_files(spec)?;
    let status = inspect(spec)?;
    match status.ownership {
        Ownership::Owned | Ownership::Absent => Ok(status),
        _ => Err(ServiceError::new(
            ServiceErrorCode::ForeignService,
            "Task identity or definition differs from this environment",
        )),
    }
}
#[cfg(windows)]
pub(super) fn install(
    spec: &ServiceSpec,
    credential: Option<&ServiceCredential>,
) -> Result<ServiceStatus, ServiceError> {
    if credential.is_some() {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "User-session tasks never accept an account password",
        ));
    }
    preflight(spec)?;
    check_install_files(spec)?;
    call(spec, "install")
}
#[cfg(windows)]
pub(super) fn control(spec: &ServiceSpec, operation: &str) -> Result<ServiceStatus, ServiceError> {
    let state = inspect(spec)?;
    if operation == "uninstall" && state.ownership == Ownership::Absent {
        return Ok(state);
    }
    check_owned(&state)?;
    if operation == "uninstall" && state.running != Some(false) {
        return Err(ServiceError::new(
            ServiceErrorCode::Busy,
            "Stop the user task before uninstalling",
        ));
    }
    call(spec, operation)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn native_powershell_parses_the_fixed_task_script_without_registering_or_running_it() {
        use crate::process::CommandOutputExt;
        use std::os::windows::process::CommandExt;
        let root = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap());
        let result=std::process::Command::new(root.join("System32/WindowsPowerShell/v1.0/powershell.exe"))
            .args(["-NoLogo","-NoProfile","-NonInteractive","-Command", "$ErrorActionPreference='Stop'; $null=[ScriptBlock]::Create($env:WEBCODEX_PARSE_SCRIPT); exit 0"])
            .env("WEBCODEX_PARSE_SCRIPT",SCRIPT).creation_flags(0x08000000).bounded_output().unwrap();
        assert!(
            result.status.success(),
            "fixed Task Scheduler script must parse in native Windows PowerShell"
        );
    }
    #[test]
    fn user_task_identity_and_arguments_remain_data() {
        let mut spec = super::super::tests::sample_spec();
        spec.scope = ServiceScope::User;
        if let ServiceAccount::SystemUser { group, home, .. } = &mut spec.account {
            *group = None;
            *home = Some(
                std::env::current_exe()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .to_owned(),
            );
        }
        spec.args = vec![
            "--config".into(),
            "x \\\" y; $env:SECRET".into(),
            "trailing\\".into(),
        ];
        let request = task_request(&spec, "install").unwrap();
        assert!(request["arguments"]
            .as_str()
            .unwrap()
            .contains("$env:SECRET"));
        assert!(!SCRIPT.contains("Invoke-Expression"));
        assert!(!SCRIPT.contains("HighestAvailable"));
        assert!(SCRIPT.contains("$folder.RegisterTaskDefinition($inputData.name, $definition, 2,"));
        let mut other = spec.clone();
        other.config_identity.push_str("-other");
        assert_ne!(
            request["name"],
            task_request(&other, "install").unwrap()["name"]
        );
        assert!(!request.to_string().contains("--windows-service"));
    }
    #[test]
    fn user_task_status_and_pid_share_one_owner_checked_observation() {
        let spec = super::super::tests::sample_spec();
        let current = json!({
            "exists": true, "owned": true, "enabled": true, "state": 4,
            "process_id": 4312,
            "instance_guid": "{723A491B-825B-4C30-BB2B-F86AA98F94AA}"
        });
        let (status, pid) = decode_status_with_pid(&spec, &current).unwrap();
        assert_eq!(status.ownership, Ownership::Owned);
        assert_eq!(status.running, Some(true));
        assert_eq!(pid, Some(4312));
        let mut foreign = current.clone();
        foreign["owned"] = json!(false);
        assert_eq!(decode_status_with_pid(&spec, &foreign).unwrap().1, None);
        let mut invalid = current;
        invalid["instance_guid"] = json!("unknown");
        assert!(decode_status_with_pid(&spec, &invalid).is_err());
    }

    #[test]
    fn user_task_running_pid_requires_a_live_owned_action_observation() {
        let spec = super::super::tests::sample_spec();
        let current = json!({
            "exists": true, "owned": true, "enabled": true, "state": 4,
            "process_id": 4312,
            "instance_guid": "{723A491B-825B-4C30-BB2B-F86AA98F94AA}"
        });
        assert_eq!(decode_running_pid(&spec, &current).unwrap(), Some(4312));
        for state in [0, 1, 2, 3] {
            let mut observation = current.clone();
            observation["state"] = json!(state);
            assert_eq!(decode_running_pid(&spec, &observation).unwrap(), None);
        }
        let mut foreign = current.clone();
        foreign["owned"] = json!(false);
        assert_eq!(decode_running_pid(&spec, &foreign).unwrap(), None);
        let mut missing = current.clone();
        missing.as_object_mut().unwrap().remove("process_id");
        assert_eq!(decode_running_pid(&spec, &missing).unwrap(), None);
        for pid in [
            json!(0),
            json!(-1),
            json!(u64::from(u32::MAX) + 1),
            json!("4312"),
        ] {
            let mut invalid = current.clone();
            invalid["process_id"] = pid;
            assert!(decode_running_pid(&spec, &invalid).is_err());
        }
        for instance in [
            Value::Null,
            json!("unknown"),
            json!(uuid::Uuid::nil().to_string()),
        ] {
            let mut invalid = current.clone();
            invalid["instance_guid"] = instance;
            assert!(decode_running_pid(&spec, &invalid).is_err());
        }
    }

    #[cfg(windows)]
    #[test]
    fn console_free_worker_has_no_console_and_preserves_exit_and_redirected_streams() {
        use std::process::{Command, Stdio};
        let temp = crate::test_tempdir().unwrap();
        let stdout = temp.path().join("stdout");
        let stderr = temp.path().join("stderr");
        let root = PathBuf::from(std::env::var_os("SystemRoot").unwrap());
        let mut command = Command::new(root.join("System32/WindowsPowerShell/v1.0/powershell.exe"));
        command.args(["-NoProfile", "-NonInteractive", "-Command", r#"
            Add-Type -Name Console -Namespace TaskTest -MemberDefinition '[DllImport("kernel32.dll")] public static extern IntPtr GetConsoleWindow();'
            if ([TaskTest.Console]::GetConsoleWindow() -ne [IntPtr]::Zero) { exit 11 }
            [Console]::Out.WriteLine('fixture-out')
            [Console]::Error.WriteLine('fixture-err')
            exit 37
        "#]).stdin(Stdio::null())
            .stdout(std::fs::File::create(&stdout).unwrap())
            .stderr(std::fs::File::create(&stderr).unwrap());
        assert_eq!(run_console_free_worker(&mut command).unwrap(), 37);
        assert!(std::fs::read_to_string(stdout)
            .unwrap()
            .contains("fixture-out"));
        assert!(std::fs::read_to_string(stderr)
            .unwrap()
            .contains("fixture-err"));
    }

    #[cfg(windows)]
    #[test]
    fn user_task_action_pid_selection_rejects_ambiguous_or_replaced_processes() {
        use crate::process::CommandOutputExt;
        use std::os::windows::process::CommandExt;
        let root = PathBuf::from(std::env::var_os("SystemRoot").unwrap());
        let result =
            std::process::Command::new(root.join("System32/WindowsPowerShell/v1.0/powershell.exe"))
                .args([
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    include_str!("windows_user_pid_tests.ps1"),
                ])
                .env("WEBCODEX_PARSE_SCRIPT", SCRIPT)
                .creation_flags(0x08000000)
                .bounded_output()
                .unwrap();
        assert!(
            result.status.success(),
            "mocked action PID selection failed"
        );
    }

    #[cfg(windows)]
    #[test]
    fn user_task_restart_waits_for_owned_process_exit_and_confirms_one_new_instance() {
        use crate::process::CommandOutputExt;
        use std::os::windows::process::CommandExt;
        let root = PathBuf::from(std::env::var_os("SystemRoot").unwrap());
        let result =
            std::process::Command::new(root.join("System32/WindowsPowerShell/v1.0/powershell.exe"))
                .args([
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    include_str!("windows_user_lifecycle_tests.ps1"),
                ])
                .env("WEBCODEX_PARSE_SCRIPT", SCRIPT)
                .creation_flags(0x08000000)
                .bounded_output()
                .unwrap();
        assert!(
            result.status.success(),
            "native user-task lifecycle races failed"
        );
    }

    #[test]
    fn user_task_does_not_turn_unobserved_state_into_stopped() {
        let spec = super::super::tests::sample_spec();
        let observed = decode_status(
            &spec,
            &json!({"exists":true,"owned":true,"enabled":true,"state":0}),
        )
        .unwrap();
        assert_eq!(observed.running, None);
        assert_eq!(
            decode_status(
                &spec,
                &json!({"exists":true,"owned":false,"enabled":true,"state":4})
            )
            .unwrap()
            .ownership,
            Ownership::Foreign
        );
        assert!(decode_status(&spec, &json!({})).is_err());
        assert!(decode_status(&spec, &json!({"error":"native_task_failed"})).is_err());
    }
}
