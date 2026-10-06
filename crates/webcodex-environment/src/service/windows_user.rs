//! Per-user interactive Task Scheduler adapter. No password, UAC or SCM account.
//! A fixed native script consumes bounded non-secret JSON as child-local data;
//! callers cannot supply script text. Registration is create-only, never /F.
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
    Ok(json!({"operation":operation, "name":name,
        "description":format!("{} scope=user", ownership_marker(spec)), "sid":sid,
        "program":spec.program.to_str().ok_or_else(|| ServiceError::new(ServiceErrorCode::InvalidSpec,"Task executable must be UTF-8"))?,
        "arguments":spec.args.iter().map(|value| quote(value)).collect::<Vec<_>>().join(" "),
        "directory":spec.working_directory.to_str().ok_or_else(|| ServiceError::new(ServiceErrorCode::InvalidSpec,"Task working directory must be UTF-8"))?}))
}

/// Pure saved task identity, also used by read-only diagnostic references.
pub(super) fn task_name(spec: &ServiceSpec, sid: &str) -> String {
    let identity = format!("{sid}:{}", spec.config_identity);
    let suffix = format!("{:x}", Sha256::digest(identity.as_bytes()));
    format!("WebCodex-user-{}-{}", &suffix[..16], spec.id)
}

fn decode_status(spec: &ServiceSpec, value: Value) -> Result<ServiceStatus, ServiceError> {
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
        detail: Some(
            "user Task Scheduler; runs only while this owner is signed in; no password stored"
                .into(),
        ),
    })
}

#[cfg(windows)]
fn call(spec: &ServiceSpec, operation: &str) -> Result<ServiceStatus, ServiceError> {
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
    decode_status(spec, value)
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
    fn user_task_does_not_turn_unobserved_state_into_stopped() {
        let spec = super::super::tests::sample_spec();
        let observed = decode_status(
            &spec,
            json!({"exists":true,"owned":true,"enabled":true,"state":0}),
        )
        .unwrap();
        assert_eq!(observed.running, None);
        assert_eq!(
            decode_status(
                &spec,
                json!({"exists":true,"owned":false,"enabled":true,"state":4})
            )
            .unwrap()
            .ownership,
            Ownership::Foreign
        );
        assert!(decode_status(&spec, json!({})).is_err());
        assert!(decode_status(&spec, json!({"error":"native_task_failed"})).is_err());
    }
}
