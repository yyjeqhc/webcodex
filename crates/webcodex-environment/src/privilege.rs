//! A narrow, explicit OS elevation boundary. Requests contain service metadata,
//! never credentials. Desktop and CLI use the same privileged operation path.
use crate::native::service_error;
use crate::service::{
    Component, ServiceAccount, ServiceCredential, ServiceManager, ServiceSpec, ServiceStatus,
};
use crate::storage::atomic_private_write;
use crate::*;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::Path;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServiceRequest {
    spec: ServiceSpec,
    operation: ServiceOperation,
    requester: LocalAccount,
    project: Option<std::path::PathBuf>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RestoreRequest {
    kind: String,
    operation_id: String,
    requester: LocalAccount,
}

#[cfg(target_os = "linux")]
pub(crate) async fn legacy_server_operation(
    store: &EnvironmentStore,
    request: &SetupRequest,
    action: crate::legacy_system_server::LegacyServerAction,
) -> SetupResultValue<crate::legacy_system_server::LegacyServerObservation> {
    if crate::installer_unix::child_channel_active() {
        return Err(SetupDiagnostic::new(
            "installer_broker_denied",
            "Installer child cannot perform old system Server handoff",
            "Resume migration from the actual project user after installer recovery",
        ));
    }
    let value = crate::legacy_system_server::LegacyServerRequest {
        kind: "legacy_cli_system_server".into(),
        action,
        requester: request.account.clone(),
        listen: match &request.mode {
            EnvironmentMode::Create { listen } => listen.clone(),
            _ => return Err(SetupDiagnostic::io()),
        },
    };
    if elevated()? {
        return crate::legacy_system_server::privileged_apply(store.root(), &value);
    }
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let request_path = store.root().join(format!(".service-request-{nonce}.json"));
    let response_path = store.root().join(format!(".service-result-{nonce}.json"));
    let trusted_cli = trusted_legacy_cli(&request.binaries.cli)?;
    atomic_private_write(
        &request_path,
        &serde_json::to_vec(&value).map_err(|_| SetupDiagnostic::io())?,
    )?;
    atomic_private_write(&response_path, b"null")?;
    let launch = elevate(&trusted_cli, &request_path, &response_path).await;
    let bytes = crate::storage::read_private(&response_path)?;
    let result: Option<
        Result<crate::legacy_system_server::LegacyServerObservation, SetupDiagnostic>,
    > = serde_json::from_slice(&bytes).map_err(|_| SetupDiagnostic::io())?;
    match result {
        Some(result) => {
            let _ = std::fs::remove_file(&request_path);
            let _ = std::fs::remove_file(&response_path);
            result
        }
        None => Err(launch.err().unwrap_or_else(|| SetupDiagnostic::new(
            "legacy_handoff_outcome_unknown", "The privileged old Server handoff did not report a result",
            "Inspect the root-protected handoff receipt and both Server owners before retrying migration"))),
    }
}

#[cfg(target_os = "linux")]
fn trusted_legacy_cli(path: &Path) -> SetupResultValue<std::path::PathBuf> {
    use std::os::unix::fs::MetadataExt;
    let denied = || {
        SetupDiagnostic::new(
            "legacy_cli_program",
            "The privileged handoff CLI is not installed under a root-controlled path",
            "Use the installed WebCodex CLI package for system Server migration",
        )
    };
    if !path.is_absolute() || path.file_name().and_then(|name| name.to_str()) != Some("webcodex") {
        return Err(denied());
    }
    for ancestor in path.parent().into_iter().flat_map(Path::ancestors) {
        let meta = std::fs::symlink_metadata(ancestor).map_err(|_| denied())?;
        if !meta.is_dir()
            || meta.file_type().is_symlink()
            || meta.uid() != 0
            || meta.mode() & 0o022 != 0
        {
            return Err(denied());
        }
    }
    let link = std::fs::symlink_metadata(path).map_err(|_| denied())?;
    if link.file_type().is_symlink() && (link.uid() != 0 || link.mode() & 0o022 != 0) {
        return Err(denied());
    }
    let target = path.canonicalize().map_err(|_| denied())?;
    for ancestor in target.parent().into_iter().flat_map(Path::ancestors) {
        let meta = std::fs::symlink_metadata(ancestor).map_err(|_| denied())?;
        if !meta.is_dir()
            || meta.file_type().is_symlink()
            || meta.uid() != 0
            || meta.mode() & 0o022 != 0
        {
            return Err(denied());
        }
    }
    let meta = std::fs::symlink_metadata(&target).map_err(|_| denied())?;
    if !meta.is_file()
        || meta.file_type().is_symlink()
        || meta.uid() != 0
        || meta.nlink() != 1
        || meta.mode() & 0o022 != 0
        || meta.mode() & 0o111 == 0
    {
        return Err(denied());
    }
    Ok(target)
}

pub(crate) async fn restore_upgrade_programs(
    store: &EnvironmentStore,
    record: &EnvironmentRecord,
    operation_id: &str,
) -> SetupResultValue<()> {
    #[cfg(unix)]
    if crate::installer_unix::child_channel_active() {
        return crate::installer_unix::child_restore(operation_id);
    }
    if elevated()? {
        return crate::upgrade::restore_programs_from_journal(
            store.root(),
            operation_id,
            &record.request.account.identity,
        );
    }
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let request_path = store.root().join(format!(".service-request-{nonce}.json"));
    let response_path = store.root().join(format!(".service-result-{nonce}.json"));
    let request = RestoreRequest {
        kind: "restore_upgrade_programs".into(),
        operation_id: operation_id.into(),
        requester: record.request.account.clone(),
    };
    atomic_private_write(
        &request_path,
        &serde_json::to_vec(&request).map_err(|_| SetupDiagnostic::io())?,
    )?;
    atomic_private_write(&response_path, b"null")?;
    let launch = elevate(&record.request.binaries.cli, &request_path, &response_path).await;
    let bytes = crate::storage::read_private(&response_path)?;
    let result: Option<Result<(), SetupDiagnostic>> =
        serde_json::from_slice(&bytes).map_err(|_| SetupDiagnostic::io())?;
    match result {
        Some(result) => {
            let _ = std::fs::remove_file(&request_path);
            let _ = std::fs::remove_file(&response_path);
            result
        }
        None => Err(launch.err().unwrap_or_else(|| {
            SetupDiagnostic::new(
                "upgrade_restore_outcome_unknown",
                "The privileged restore did not report a result",
                "Inspect the retained backup and installed program hashes before retrying rollback",
            )
        })),
    }
}

pub(crate) async fn service_operation(
    store: &EnvironmentStore,
    record: &EnvironmentRecord,
    component: Component,
    operation: ServiceOperation,
    credential: Option<&ServiceCredential>,
) -> SetupResultValue<ServiceStatus> {
    let spec = service_spec(store, record, component)?;
    service_operation_spec(store, record, spec, operation, credential).await
}

pub(crate) async fn service_operation_spec(
    store: &EnvironmentStore,
    record: &EnvironmentRecord,
    spec: ServiceSpec,
    operation: ServiceOperation,
    credential: Option<&ServiceCredential>,
) -> SetupResultValue<ServiceStatus> {
    #[cfg(unix)]
    if crate::installer_unix::child_channel_active() {
        if credential.is_some() {
            return Err(SetupDiagnostic::new(
                "installer_broker_denied",
                "An installer child cannot send credentials",
                "Retain the prepared upgrade and inspect its original services",
            ));
        }
        return crate::installer_unix::child_service(spec, operation);
    }
    if elevated()? {
        return apply(
            &spec,
            operation,
            credential,
            record.request.project.as_deref(),
        );
    }
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let request_path = store.root().join(format!(".service-request-{nonce}.json"));
    let response_path = store.root().join(format!(".service-result-{nonce}.json"));
    let request = ServiceRequest {
        spec,
        operation,
        requester: record.request.account.clone(),
        project: record.request.project.clone(),
    };
    atomic_private_write(
        &request_path,
        &serde_json::to_vec(&request).map_err(|_| SetupDiagnostic::io())?,
    )?;
    // The elevated child writes through this existing private file without
    // changing its owner, following a link, or selecting another destination.
    atomic_private_write(&response_path, b"null")?;
    let launch = elevate(&record.request.binaries.cli, &request_path, &response_path).await;
    let bytes = crate::storage::read_private(&response_path)?;
    let result: Option<Result<ServiceStatus, SetupDiagnostic>> =
        serde_json::from_slice(&bytes).map_err(|_| SetupDiagnostic::io())?;
    match result {
        Some(result) => {
            let _ = std::fs::remove_file(&request_path);
            let _ = std::fs::remove_file(&response_path);
            result
        }
        None => Err(launch.err().unwrap_or_else(|| {
            SetupDiagnostic::new(
                "service_outcome_unknown",
                "The privileged operation did not report a result",
                "Inspect the service before resuming setup; do not start a second instance",
            )
        })),
    }
}

fn apply(
    spec: &ServiceSpec,
    operation: ServiceOperation,
    credential: Option<&ServiceCredential>,
    project: Option<&Path>,
) -> SetupResultValue<ServiceStatus> {
    #[cfg(windows)]
    let prompted = if matches!(
        operation,
        ServiceOperation::PrepareRunner
            | ServiceOperation::Install
            | ServiceOperation::UpdateCredential
    ) && matches!(spec.account, ServiceAccount::SystemUser { .. })
        && credential.is_none()
        && (operation == ServiceOperation::UpdateCredential
            || !ServiceManager::inspect(spec)
                .map_err(service_error)?
                .installed)
    {
        Some(prompt_windows_credential(spec)?)
    } else {
        None
    };
    #[cfg(windows)]
    let credential = credential.or(prompted.as_ref());
    #[cfg(windows)]
    if let Some(credential) = credential {
        ServiceManager::preflight(spec).map_err(service_error)?;
        ServiceManager::validate_credential(spec, credential, project).map_err(service_error)?;
    }
    #[cfg(not(windows))]
    let _ = project;
    let result = match operation {
        ServiceOperation::PrepareRunner => {
            #[cfg(windows)]
            {
                ServiceManager::prepare_runner(spec, credential).map_err(service_error)
            }
            #[cfg(not(windows))]
            {
                Err(SetupDiagnostic::new(
                    "unsupported_operation",
                    "Account provisioning is a Windows SCM operation",
                    "Install the platform service through setup",
                ))
            }
        }
        ServiceOperation::Install => {
            let status = ServiceManager::install(spec, credential).map_err(service_error)?;
            #[cfg(windows)]
            if spec.component != Component::Runner {
                crate::service::grant_service_directory(spec, &spec.working_directory)
                    .map_err(service_error)?;
            }
            #[cfg(any(windows, target_os = "macos"))]
            if spec.component == Component::Runner {
                let dir = spec
                    .args
                    .last()
                    .map(Path::new)
                    .ok_or_else(SetupDiagnostic::io)?;
                crate::session_service::install_session_helper(spec, dir).map_err(service_error)?;
            }
            Ok(status)
        }
        ServiceOperation::Start => ServiceManager::start(spec).map_err(service_error),
        ServiceOperation::Stop => ServiceManager::stop(spec).map_err(service_error),
        ServiceOperation::Restart => ServiceManager::restart(spec).map_err(service_error),
        ServiceOperation::Uninstall => {
            let status = ServiceManager::inspect(spec).map_err(service_error)?;
            if status.running != Some(false) || status.ownership != service::Ownership::Owned {
                return Err(SetupDiagnostic::new(
                    "service_stop_required",
                    "Confirm the owned service is stopped before uninstalling",
                    "Stop this component explicitly, then retry",
                ));
            }
            #[cfg(any(windows, target_os = "macos"))]
            if spec.component == Component::Runner {
                let dir = spec
                    .args
                    .last()
                    .map(Path::new)
                    .ok_or_else(SetupDiagnostic::io)?;
                crate::session_service::remove_session_helper(spec, dir).map_err(service_error)?;
            }
            ServiceManager::uninstall(spec).map_err(service_error)
        }
        ServiceOperation::UpdateCredential => {
            let credential = credential.ok_or_else(|| {
                SetupDiagnostic::new(
                    "service_credential_required",
                    "A Windows service account credential is required",
                    "Update the service credential through the native credential prompt",
                )
            })?;
            ServiceManager::update_credential(spec, credential).map_err(service_error)
        }
    };
    result
}

/// Internal CLI entrypoint. This never opens a network connection or starts an
/// arbitrary command from a frontend; it accepts only a validated service spec.
pub fn run_privileged_service_request(
    request_path: &Path,
    response_path: &Path,
) -> SetupResultValue<()> {
    if !elevated()? {
        return Err(elevation_required());
    }
    if request_path.parent() != response_path.parent() {
        return Err(SetupDiagnostic::io());
    }
    let mut request_file = open_exchange(request_path, false)?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut request_file)
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| SetupDiagnostic::io())?;
    if bytes.len() > 65536 {
        return Err(SetupDiagnostic::io());
    }
    let payload: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| SetupDiagnostic::io())?;
    let kind = payload.get("kind").and_then(serde_json::Value::as_str);
    let restore: Option<RestoreRequest> = if kind == Some("restore_upgrade_programs") {
        Some(serde_json::from_slice(&bytes).map_err(|_| SetupDiagnostic::io())?)
    } else {
        None
    };
    #[cfg(target_os = "linux")]
    let legacy: Option<crate::legacy_system_server::LegacyServerRequest> =
        if kind == Some("legacy_cli_system_server") {
            Some(serde_json::from_slice(&bytes).map_err(|_| SetupDiagnostic::io())?)
        } else {
            None
        };
    #[cfg(not(target_os = "linux"))]
    let legacy: Option<()> = None;
    let service: Option<ServiceRequest> = if restore.is_none() && legacy.is_none() && kind.is_none()
    {
        Some(serde_json::from_slice(&bytes).map_err(|_| SetupDiagnostic::io())?)
    } else {
        None
    };
    let requester = if let Some(request) = &restore {
        &request.requester
    } else if let Some(request) = &legacy {
        #[cfg(target_os = "linux")]
        {
            &request.requester
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = request;
            return Err(SetupDiagnostic::io());
        }
    } else {
        &service.as_ref().ok_or_else(SetupDiagnostic::io)?.requester
    };
    verify_request_owner(&request_file, requester)?;
    if let Some(request) = &service {
        verify_requester(&request_file, request)?;
    }
    #[cfg(windows)]
    {
        if crate::storage::windows_path_owner(request_path)? != requester.identity
            || crate::storage::windows_path_owner(response_path)? != requester.identity
        {
            return Err(SetupDiagnostic::io());
        }
        crate::runtime_entry::validate_windows_env_acl(request_path)
            .map_err(|_| SetupDiagnostic::io())?;
        crate::runtime_entry::validate_windows_env_acl(response_path)
            .map_err(|_| SetupDiagnostic::io())?;
    }
    let mut response = open_exchange(response_path, true)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if response
            .metadata()
            .map_err(|_| SetupDiagnostic::io())?
            .uid()
            != request_file
                .metadata()
                .map_err(|_| SetupDiagnostic::io())?
                .uid()
        {
            return Err(SetupDiagnostic::io());
        }
    }
    let bytes = if let Some(request) = restore {
        let result = if request.kind == "restore_upgrade_programs" {
            crate::upgrade::restore_programs_from_journal(
                request_path.parent().ok_or_else(SetupDiagnostic::io)?,
                &request.operation_id,
                &request.requester.identity,
            )
        } else {
            Err(SetupDiagnostic::io())
        };
        serde_json::to_vec(&result).map_err(|_| SetupDiagnostic::io())?
    } else if let Some(request) = legacy {
        #[cfg(target_os = "linux")]
        {
            let result = crate::legacy_system_server::privileged_apply(
                request_path.parent().ok_or_else(SetupDiagnostic::io)?,
                &request,
            );
            serde_json::to_vec(&result).map_err(|_| SetupDiagnostic::io())?
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = request;
            return Err(SetupDiagnostic::io());
        }
    } else {
        let request = service.ok_or_else(SetupDiagnostic::io)?;
        let result = apply(
            &request.spec,
            request.operation,
            None,
            request.project.as_deref(),
        );
        serde_json::to_vec(&result).map_err(|_| SetupDiagnostic::io())?
    };
    response
        .set_len(0)
        .and_then(|_| response.write_all(&bytes))
        .and_then(|_| response.sync_all())
        .map_err(|_| SetupDiagnostic::io())?;
    Ok(())
}

fn open_exchange(path: &Path, write: bool) -> SetupResultValue<std::fs::File> {
    if !path.is_absolute() {
        return Err(SetupDiagnostic::io());
    }
    for ancestor in path.ancestors() {
        let metadata = std::fs::symlink_metadata(ancestor).map_err(|_| SetupDiagnostic::io())?;
        if metadata.is_symlink() {
            return Err(SetupDiagnostic::io());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(SetupDiagnostic::io());
            }
        }
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(write);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let file = options.open(path).map_err(|_| SetupDiagnostic::io())?;
    let metadata = file.metadata().map_err(|_| SetupDiagnostic::io())?;
    if !metadata.is_file() {
        return Err(SetupDiagnostic::io());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 || metadata.mode() & 0o077 != 0 {
            return Err(SetupDiagnostic::io());
        }
    }
    Ok(file)
}

fn verify_requester(file: &std::fs::File, request: &ServiceRequest) -> SetupResultValue<()> {
    verify_request_owner(file, &request.requester)?;
    if let ServiceAccount::SystemUser {
        expected_identity, ..
    } = &request.spec.account
    {
        if expected_identity != &request.requester.identity {
            return Err(SetupDiagnostic::new(
                "service_owner_conflict",
                "A service request cannot change the project user's identity",
                "Configure services as the actual project user",
            ));
        }
    }
    let name = request
        .spec
        .program
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if !matches!(
        name,
        "webcodex"
            | "webcodex.exe"
            | "webcodex-server"
            | "webcodex-server.exe"
            | "webcodex-runner"
            | "webcodex-runner.exe"
    ) {
        return Err(SetupDiagnostic::new(
            "service_program",
            "The service must use an installed WebCodex runtime",
            "Select a verified WebCodex installation",
        ));
    }
    Ok(())
}

fn verify_request_owner(file: &std::fs::File, requester: &LocalAccount) -> SetupResultValue<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let uid = file
            .metadata()
            .map_err(|_| SetupDiagnostic::io())?
            .uid()
            .to_string();
        if uid != requester.identity {
            return Err(SetupDiagnostic::io());
        }
    }
    Ok(())
}

fn elevation_required() -> SetupDiagnostic {
    SetupDiagnostic::new(
        "elevation_required",
        "Installing or controlling system services requires OS authorization",
        "Run setup interactively and approve the operating system's administrator prompt",
    )
}

fn elevated() -> SetupResultValue<bool> {
    #[cfg(unix)]
    {
        Ok(unsafe { libc::geteuid() } == 0)
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY},
            System::Threading::{GetCurrentProcess, OpenProcessToken},
        };
        let mut handle = std::ptr::null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut handle) } == 0 {
            return Err(elevation_required());
        }
        let mut value = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut length = 0;
        let ok = unsafe {
            GetTokenInformation(
                handle,
                TokenElevation,
                (&mut value as *mut TOKEN_ELEVATION).cast(),
                std::mem::size_of_val(&value) as u32,
                &mut length,
            )
        };
        unsafe {
            CloseHandle(handle);
        }
        if ok == 0 {
            Err(elevation_required())
        } else {
            Ok(value.TokenIsElevated != 0)
        }
    }
}

async fn elevate(cli: &Path, request: &Path, response: &Path) -> SetupResultValue<()> {
    #[cfg(target_os = "linux")]
    {
        use std::io::IsTerminal;
        let interactive = std::io::stdin().is_terminal();
        let program = if interactive {
            "/usr/bin/sudo"
        } else {
            "/usr/bin/pkexec"
        };
        if !Path::new(program).is_file() {
            return Err(elevation_required());
        }
        let mut command = tokio::process::Command::new(program);
        command
            .arg(cli)
            .args(["environment", "__service-operation"])
            .arg(request)
            .arg(response);
        command.stdin(if interactive {
            std::process::Stdio::inherit()
        } else {
            std::process::Stdio::null()
        });
        command
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::inherit());
        let status = command.status().await.map_err(|_| elevation_required())?;
        if status.success() {
            Ok(())
        } else {
            Err(elevation_required())
        }
    }
    #[cfg(target_os = "macos")]
    {
        let script = "on run argv\nset cmd to quoted form of item 1 of argv & \" environment __service-operation \" & quoted form of item 2 of argv & \" \" & quoted form of item 3 of argv\ndo shell script cmd with administrator privileges\nend run";
        let status = tokio::process::Command::new("/usr/bin/osascript")
            .args(["-e", script, "--"])
            .arg(cli)
            .arg(request)
            .arg(response)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .status()
            .await
            .map_err(|_| elevation_required())?;
        if status.success() {
            Ok(())
        } else {
            Err(elevation_required())
        }
    }
    #[cfg(windows)]
    {
        let cli = cli.to_path_buf();
        let request = request.to_path_buf();
        let response = response.to_path_buf();
        tokio::task::spawn_blocking(move || elevate_windows(&cli, &request, &response))
            .await
            .map_err(|_| elevation_required())?
    }
}

#[cfg(windows)]
fn elevate_windows(cli: &Path, request: &Path, response: &Path) -> SetupResultValue<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0},
        System::Threading::{GetExitCodeProcess, WaitForSingleObject},
        UI::{
            Shell::{
                ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
            },
            WindowsAndMessaging::SW_SHOWNORMAL,
        },
    };
    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<u16>>()
    };
    let program = wide(cli);
    let verb: Vec<u16> = "runas\0".encode_utf16().collect();
    let quote = |value: &Path| -> SetupResultValue<String> {
        let value = value.to_str().ok_or_else(SetupDiagnostic::io)?;
        if value.contains(['"', '\0', '\r', '\n']) {
            return Err(SetupDiagnostic::io());
        }
        Ok(format!("\"{}\"", value.trim_end_matches('\\')))
    };
    let parameters: Vec<u16> = format!(
        "environment __service-operation {} {}",
        quote(request)?,
        quote(response)?
    )
    .encode_utf16()
    .chain(Some(0))
    .collect();
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of_val(&info) as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
    info.lpVerb = verb.as_ptr();
    info.lpFile = program.as_ptr();
    info.lpParameters = parameters.as_ptr();
    info.nShow = SW_SHOWNORMAL;
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        return Err(elevation_required());
    }
    let waited = unsafe { WaitForSingleObject(info.hProcess, 600_000) };
    let mut code = 1;
    unsafe {
        GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
    }
    if waited == WAIT_OBJECT_0 && code == 0 {
        Ok(())
    } else {
        Err(elevation_required())
    }
}

#[cfg(windows)]
fn prompt_windows_credential(spec: &ServiceSpec) -> SetupResultValue<ServiceCredential> {
    use std::io::IsTerminal;
    use windows_sys::Win32::Security::Credentials::*;
    if std::io::stdin().is_terminal() {
        return prompt_windows_console_credential(spec);
    }
    let ServiceAccount::SystemUser { name, .. } = &spec.account else {
        return Err(elevation_required());
    };
    let mut username = vec![0u16; 514];
    let mut password = vec![0u16; 257];
    let name_wide: Vec<u16> = name.encode_utf16().collect();
    if name_wide.len() >= username.len() {
        return Err(SetupDiagnostic::io());
    }
    username[..name_wide.len()].copy_from_slice(&name_wide);
    let target: Vec<u16> = "WebCodex Runner service\0".encode_utf16().collect();
    let mut save = 0;
    let result = unsafe {
        CredUIPromptForCredentialsW(
            std::ptr::null(),
            target.as_ptr(),
            std::ptr::null(),
            0,
            username.as_mut_ptr(),
            username.len() as u32,
            password.as_mut_ptr(),
            password.len() as u32,
            &mut save,
            CREDUI_FLAGS_GENERIC_CREDENTIALS
                | CREDUI_FLAGS_DO_NOT_PERSIST
                | CREDUI_FLAGS_ALWAYS_SHOW_UI
                | CREDUI_FLAGS_KEEP_USERNAME,
        )
    };
    if result != 0 {
        password.fill(0);
        return Err(SetupDiagnostic::new("service_credential_required", "Windows service login credentials were not supplied", "Use the project user's account password; a Windows Hello PIN is not a service password"));
    }
    let supplied = String::from_utf16_lossy(
        &username[..username
            .iter()
            .position(|n| *n == 0)
            .unwrap_or(username.len())],
    );
    if !supplied.eq_ignore_ascii_case(name) {
        password.fill(0);
        return Err(SetupDiagnostic::new(
            "service_owner_conflict",
            "The credential prompt selected another account",
            "Use the project owner's Windows account",
        ));
    }
    let secret = Secret::new(String::from_utf16_lossy(
        &password[..password
            .iter()
            .position(|n| *n == 0)
            .unwrap_or(password.len())],
    ));
    password.fill(0);
    Ok(ServiceCredential::from_password(secret.expose()))
}

#[cfg(windows)]
fn prompt_windows_console_credential(spec: &ServiceSpec) -> SetupResultValue<ServiceCredential> {
    use std::io::{BufRead, Write};
    use windows_sys::Win32::System::Console::{
        GetConsoleMode, GetStdHandle, SetConsoleMode, ENABLE_ECHO_INPUT, STD_INPUT_HANDLE,
    };
    let ServiceAccount::SystemUser { name, .. } = &spec.account else {
        return Err(elevation_required());
    };
    let input = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    let mut mode = 0;
    if unsafe { GetConsoleMode(input, &mut mode) } == 0
        || unsafe { SetConsoleMode(input, mode & !ENABLE_ECHO_INPUT) } == 0
    {
        return Err(SetupDiagnostic::new(
            "service_credential_input",
            "Secure console credential input is unavailable",
            "Configure this Runner from Desktop's native credential prompt",
        ));
    }
    struct Restore(windows_sys::Win32::Foundation::HANDLE, u32);
    impl Drop for Restore {
        fn drop(&mut self) {
            unsafe {
                SetConsoleMode(self.0, self.1);
            }
            eprintln!();
        }
    }
    let _restore = Restore(input, mode);
    eprint!("Windows service password for {name} (not a Hello PIN): ");
    std::io::stderr()
        .flush()
        .map_err(|_| SetupDiagnostic::io())?;
    let mut bytes = Vec::new();
    std::io::stdin()
        .lock()
        .take(8193)
        .read_until(b'\n', &mut bytes)
        .map_err(|_| SetupDiagnostic::io())?;
    if bytes.len() > 8192 {
        return Err(SetupDiagnostic::io());
    }
    let value = String::from_utf8(bytes).map_err(|_| SetupDiagnostic::io())?;
    let secret = Secret::new(value.trim_end_matches(['\r', '\n']).to_owned());
    if secret.expose().is_empty() {
        return Err(SetupDiagnostic::new(
            "service_credential_required",
            "A service account password is required",
            "Windows Hello PIN cannot be used for a persistent service",
        ));
    }
    Ok(ServiceCredential::from_password(secret.expose()))
}
