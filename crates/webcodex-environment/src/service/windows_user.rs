//! Per-user interactive Task Scheduler adapter. No password, UAC or SCM account.
//! Read-only observations use Task Scheduler COM and query-only process handles.
//! Mutations use a fixed script with bounded non-secret JSON as child-local data;
//! callers cannot supply script text. New tasks are registered create-only.
use super::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[cfg(windows)]
mod native;

const SCRIPT: &str = include_str!("windows_user.ps1");

#[derive(Clone, serde::Serialize)]
struct TaskPlan {
    #[cfg(windows)]
    #[serde(skip)]
    id: String,
    #[cfg(windows)]
    #[serde(skip)]
    home: PathBuf,
    name: String,
    description: String,
    sid: String,
    program: String,
    arguments: String,
    host: Option<String>,
    host_arguments: String,
    directory: String,
}

fn task_plan(spec: &ServiceSpec) -> Result<TaskPlan, ServiceError> {
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
    Ok(TaskPlan {
        #[cfg(windows)]
        id: spec.id.clone(),
        #[cfg(windows)]
        home: scope::user_account(spec)?.1.to_owned(),
        name,
        description: format!("{} scope=user", ownership_marker(spec)),
        sid: sid.to_owned(),
        program: program.to_owned(),
        arguments,
        host: host
            .map(|path| path.into_os_string().into_string())
            .transpose()
            .map_err(|_| {
                ServiceError::new(ServiceErrorCode::InvalidSpec, "Task host must be UTF-8")
            })?,
        host_arguments,
        directory: directory.to_owned(),
    })
}

fn task_request(spec: &ServiceSpec, operation: &str) -> Result<Value, ServiceError> {
    let mut request = serde_json::to_value(task_plan(spec)?)
        .map_err(|_| ServiceError::new(ServiceErrorCode::InvalidSpec, "Cannot encode task plan"))?;
    request["operation"] = json!(operation);
    Ok(request)
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
    Ok(observed_status(
        &spec.id,
        owned,
        value["state"].as_i64(),
        value["enabled"].as_bool(),
        value["last_task_result"].as_i64(),
    ))
}

fn observed_status(
    id: &str,
    owned: bool,
    state: Option<i64>,
    enabled: Option<bool>,
    last_result: Option<i64>,
) -> ServiceStatus {
    let running = match state {
        Some(2 | 4) => Some(true),  // queued/running
        Some(1 | 3) => Some(false), // disabled/ready
        _ => None,
    };
    let mut detail = String::from(
        "user Task Scheduler; runs only while this owner is signed in; no password stored",
    );
    if owned && running == Some(false) {
        if let Some(result) = last_result.filter(|result| *result != 0) {
            detail.push_str(&format!("; last Task Scheduler result {result}"));
        }
    }
    ServiceStatus {
        id: id.to_owned(),
        ownership: if owned {
            Ownership::Owned
        } else {
            Ownership::Foreign
        },
        installed: true,
        enabled,
        running,
        detail: Some(detail),
    }
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
    inspect_with_pid(spec).map(|(_, pid)| pid)
}

#[cfg(windows)]
pub(super) fn inspect_with_pid(
    spec: &ServiceSpec,
) -> Result<(ServiceStatus, Option<u32>), ServiceError> {
    native::inspect_many(&[(spec, true)]).remove(0)
}

#[cfg(windows)]
pub(super) fn inspect(spec: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
    native::inspect_many(&[(spec, false)])
        .remove(0)
        .map(|(status, _)| status)
}

/// A fresh batch shares one COM connection, never a cached ownership/PID result.
#[cfg(windows)]
pub(super) fn inspect_many(
    probes: &[(&ServiceSpec, bool)],
) -> Vec<Result<(ServiceStatus, Option<u32>), ServiceError>> {
    native::inspect_many(probes)
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
