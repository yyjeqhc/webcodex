//! Login-session Computer helper autostart, separate from the SCM/launchd
//! Runner service. The helper receives only a protected rendezvous directory.
#[cfg(any(windows, target_os = "macos"))]
use crate::process::CommandOutputExt;
use crate::service::{
    Component, Ownership, ServiceAccount, ServiceError, ServiceErrorCode, ServiceSpec,
    ServiceStatus,
};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
struct HelperPlan {
    id: String,
    marker: String,
    program: PathBuf,
    session_dir: PathBuf,
    account_name: String,
    account_identity: String,
    home: Option<PathBuf>,
}

fn plan(spec: &ServiceSpec, session_dir: &Path) -> Result<HelperPlan, ServiceError> {
    if spec.component != Component::Runner
        || !spec.program.is_absolute()
        || !session_dir.is_absolute()
    {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "Computer helper requires an absolute Runner program and session directory",
        ));
    }
    let ServiceAccount::SystemUser {
        name,
        expected_identity,
        home,
        ..
    } = &spec.account
    else {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "Computer helper requires a real user account",
        ));
    };
    if name.is_empty() || expected_identity.is_empty() || spec.config_identity.is_empty() {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "Computer helper account or owner identity is missing",
        ));
    }
    let expected_args = if cfg!(windows) {
        vec![
            "--windows-service".to_string(),
            spec.id.clone(),
            "--config".to_string(),
        ]
    } else {
        vec!["--config".to_string()]
    };
    if spec.args.len() != expected_args.len() + 3
        || spec.args[..expected_args.len()] != expected_args
        || spec.args[expected_args.len()].is_empty()
        || spec.args[expected_args.len() + 1] != "--computer-session-dir"
        || Path::new(&spec.args[expected_args.len() + 2]) != session_dir
    {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "Runner service and Computer helper must share one exact session directory",
        ));
    }
    let metadata = std::fs::symlink_metadata(session_dir).map_err(|_| {
        ServiceError::new(
            ServiceErrorCode::MissingPrerequisite,
            "Computer session directory must be created before helper install",
        )
    })?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "Computer session directory is not a real directory",
        ));
    }
    let digest = Sha256::digest(format!("{}:{}", spec.id, spec.config_identity).as_bytes());
    let suffix = format!("{:x}", digest);
    Ok(HelperPlan {
        id: format!("WebCodexComputer-{}", &suffix[..16]),
        marker: format!("webcodex-computer-helper:v1:{}", &suffix[..32]),
        program: spec.program.clone(),
        session_dir: session_dir.to_path_buf(),
        account_name: name.clone(),
        account_identity: expected_identity.clone(),
        home: home.clone(),
    })
}

#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
fn status(plan: &HelperPlan, ownership: Ownership) -> ServiceStatus {
    ServiceStatus {
        id: plan.id.clone(),
        ownership,
        installed: ownership != Ownership::Absent,
        enabled: match ownership {
            Ownership::Absent => Some(false),
            Ownership::Owned => Some(true),
            _ => None,
        },
        running: None,
        detail: None,
    }
}

#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn preflight_session_helper(
    spec: &ServiceSpec,
    session_dir: &Path,
) -> Result<ServiceStatus, ServiceError> {
    let plan = plan(spec, session_dir)?;
    platform::preflight(&plan)
}
pub fn inspect_session_helper(
    spec: &ServiceSpec,
    session_dir: &Path,
) -> Result<ServiceStatus, ServiceError> {
    let plan = plan(spec, session_dir)?;
    platform::inspect(&plan)
}
pub fn install_session_helper(
    spec: &ServiceSpec,
    session_dir: &Path,
) -> Result<ServiceStatus, ServiceError> {
    let plan = plan(spec, session_dir)?;
    platform::install(&plan)
}
pub fn remove_session_helper(
    spec: &ServiceSpec,
    session_dir: &Path,
) -> Result<ServiceStatus, ServiceError> {
    let plan = plan(spec, session_dir)?;
    platform::remove(&plan)
}

#[cfg(any(windows, test))]
use quick_xml::{events::Event, Reader};

#[cfg(any(windows, test))]
fn quote_arg(value: &str) -> String {
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
}
#[cfg(any(windows, test))]
fn arguments(plan: &HelperPlan) -> String {
    format!(
        "--computer-session-helper --session-state-dir {}",
        quote_arg(plan.session_dir.to_str().unwrap_or(""))
    )
}
#[cfg(any(windows, test))]
fn render_task(plan: &HelperPlan) -> Result<String, ServiceError> {
    let command = plan.program.to_str().ok_or_else(|| {
        ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "Computer helper program path is not UTF-8",
        )
    })?;
    let dir = plan.session_dir.to_str().ok_or_else(|| {
        ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "Computer session path is not UTF-8",
        )
    })?;
    if dir.is_empty() {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "Computer session path is empty",
        ));
    }
    Ok(format!("<?xml version=\"1.0\" encoding=\"UTF-16\"?>\n<Task version=\"1.4\" xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\"><RegistrationInfo><Description>{}</Description></RegistrationInfo><Triggers><LogonTrigger><Enabled>true</Enabled><UserId>{}</UserId></LogonTrigger></Triggers><Principals><Principal id=\"Owner\"><UserId>{}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals><Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><ExecutionTimeLimit>PT0S</ExecutionTimeLimit><Enabled>true</Enabled></Settings><Actions Context=\"Owner\"><Exec><Command>{}</Command><Arguments>{}</Arguments></Exec></Actions></Task>", xml(&plan.marker), xml(&plan.account_identity), xml(&plan.account_identity), xml(command), xml(&arguments(plan))))
}
#[cfg(any(windows, test))]
fn task_fields(
    source: &str,
) -> Result<std::collections::BTreeMap<String, Vec<String>>, ServiceError> {
    let mut reader = Reader::from_str(source);
    let mut fields = std::collections::BTreeMap::<String, Vec<String>>::new();
    let mut current = None;
    loop {
        match reader.read_event() {
            Ok(Event::Start(tag)) => {
                let name = String::from_utf8_lossy(tag.local_name().as_ref()).into_owned();
                fields.entry(name.clone()).or_default().push(String::new());
                current = Some(name);
            }
            Ok(Event::Text(value)) => {
                if let Some(name) = &current {
                    let text = value.decode().map_err(|_| {
                        ServiceError::new(
                            ServiceErrorCode::OwnershipUnknown,
                            "Computer task XML is invalid",
                        )
                    })?;
                    fields
                        .get_mut(name)
                        .unwrap()
                        .last_mut()
                        .unwrap()
                        .push_str(&text);
                }
            }
            Ok(Event::GeneralRef(reference)) => {
                if let Some(name) = &current {
                    let value = reference.decode().map_err(|_| {
                        ServiceError::new(
                            ServiceErrorCode::OwnershipUnknown,
                            "Computer task XML entity is invalid",
                        )
                    })?;
                    let ch = match value.as_ref() {
                        "amp" => '&',
                        "lt" => '<',
                        "gt" => '>',
                        "quot" => '"',
                        "apos" => '\'',
                        _ => reference
                            .resolve_char_ref()
                            .map_err(|_| {
                                ServiceError::new(
                                    ServiceErrorCode::OwnershipUnknown,
                                    "Computer task XML entity is invalid",
                                )
                            })?
                            .ok_or_else(|| {
                                ServiceError::new(
                                    ServiceErrorCode::OwnershipUnknown,
                                    "Computer task XML entity is unknown",
                                )
                            })?,
                    };
                    fields.get_mut(name).unwrap().last_mut().unwrap().push(ch);
                }
            }
            Ok(Event::End(_)) => current = None,
            Ok(Event::Eof) => break,
            Err(_) => {
                return Err(ServiceError::new(
                    ServiceErrorCode::OwnershipUnknown,
                    "Computer task XML is invalid",
                ))
            }
            _ => {}
        }
    }
    Ok(fields)
}
#[cfg(any(windows, test))]
fn owned_task(plan: &HelperPlan, source: &str) -> bool {
    let Ok(fields) = task_fields(source) else {
        return false;
    };
    let only = |name: &str, expected: &str| {
        fields
            .get(name)
            .is_some_and(|values| values.len() == 1 && values[0] == expected)
    };
    only("Description", &plan.marker)
        && fields.get("UserId").is_some_and(|values| {
            values.len() == 2
                && values
                    .iter()
                    .all(|value| value.eq_ignore_ascii_case(&plan.account_identity))
        })
        && only("LogonType", "InteractiveToken")
        && only("RunLevel", "LeastPrivilege")
        && only("Command", plan.program.to_str().unwrap_or(""))
        && only("Arguments", &arguments(plan))
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::ffi::c_void;
    use std::os::windows::fs::MetadataExt;
    use std::process::Command;
    use windows_sys::Win32::Foundation::{GetLastError, LocalFree, ERROR_INSUFFICIENT_BUFFER};
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows_sys::Win32::Security::{LookupAccountNameW, SID_NAME_USE};
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }
    fn resolved_sid(name: &str) -> Result<String, ServiceError> {
        let name = wide(name);
        let mut sid_size = 0;
        let mut domain_size = 0;
        let mut kind: SID_NAME_USE = 0;
        unsafe {
            LookupAccountNameW(
                std::ptr::null(),
                name.as_ptr(),
                std::ptr::null_mut(),
                &mut sid_size,
                std::ptr::null_mut(),
                &mut domain_size,
                &mut kind,
            );
        }
        if sid_size == 0 || unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
            return Err(ServiceError::new(
                ServiceErrorCode::MissingPrerequisite,
                "Computer helper account SID is unavailable",
            ));
        }
        let mut sid = vec![0_u64; sid_size.div_ceil(8) as usize];
        let mut domain = vec![0_u16; domain_size as usize];
        if unsafe {
            LookupAccountNameW(
                std::ptr::null(),
                name.as_ptr(),
                sid.as_mut_ptr().cast::<c_void>(),
                &mut sid_size,
                domain.as_mut_ptr(),
                &mut domain_size,
                &mut kind,
            )
        } == 0
        {
            return Err(ServiceError::new(
                ServiceErrorCode::MissingPrerequisite,
                "Computer helper account SID is unavailable",
            ));
        }
        let mut string = std::ptr::null_mut();
        if unsafe { ConvertSidToStringSidW(sid.as_mut_ptr().cast(), &mut string) } == 0 {
            return Err(ServiceError::new(
                ServiceErrorCode::OperationFailed,
                "Computer helper SID conversion failed",
            ));
        }
        let length = unsafe { (0..).find(|&i| *string.add(i) == 0).unwrap_or(0) };
        let value = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(string, length) });
        unsafe {
            LocalFree(string.cast());
        }
        Ok(value)
    }

    fn validate(plan: &HelperPlan) -> Result<(), ServiceError> {
        if !plan.account_identity.starts_with("S-1-")
            || !resolved_sid(&plan.account_name)?.eq_ignore_ascii_case(&plan.account_identity)
        {
            return Err(ServiceError::new(
                ServiceErrorCode::PermissionDenied,
                "Computer helper account no longer matches selected SID",
            ));
        }
        let meta = std::fs::symlink_metadata(&plan.session_dir).map_err(|_| {
            ServiceError::new(
                ServiceErrorCode::MissingPrerequisite,
                "Computer session directory disappeared",
            )
        })?;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(ServiceError::new(
                ServiceErrorCode::InvalidSpec,
                "Computer session directory is a reparse point",
            ));
        }
        Ok(())
    }

    fn schtasks(args: &[&str]) -> Result<std::process::Output, ServiceError> {
        Command::new("schtasks.exe")
            .args(args)
            .bounded_output()
            .map_err(|_| {
                ServiceError::new(
                    ServiceErrorCode::OperationFailed,
                    "Task Scheduler command is unavailable",
                )
            })
    }
    fn decode_xml(bytes: &[u8]) -> Result<String, ServiceError> {
        if bytes.starts_with(&[0xff, 0xfe]) {
            if bytes.len() % 2 != 0 {
                return Err(ServiceError::new(
                    ServiceErrorCode::OwnershipUnknown,
                    "Task Scheduler XML is invalid",
                ));
            }
            let units = bytes[2..]
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect::<Vec<_>>();
            return String::from_utf16(&units).map_err(|_| {
                ServiceError::new(
                    ServiceErrorCode::OwnershipUnknown,
                    "Task Scheduler XML is invalid",
                )
            });
        }
        if bytes.starts_with(&[0xfe, 0xff]) {
            if bytes.len() % 2 != 0 {
                return Err(ServiceError::new(
                    ServiceErrorCode::OwnershipUnknown,
                    "Task Scheduler XML is invalid",
                ));
            }
            let units = bytes[2..]
                .chunks_exact(2)
                .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
                .collect::<Vec<_>>();
            return String::from_utf16(&units).map_err(|_| {
                ServiceError::new(
                    ServiceErrorCode::OwnershipUnknown,
                    "Task Scheduler XML is invalid",
                )
            });
        }
        String::from_utf8(
            bytes
                .strip_prefix(&[0xef, 0xbb, 0xbf])
                .unwrap_or(bytes)
                .to_vec(),
        )
        .map_err(|_| {
            ServiceError::new(
                ServiceErrorCode::OwnershipUnknown,
                "Task Scheduler XML is invalid",
            )
        })
    }
    fn query(plan: &HelperPlan) -> Result<Option<String>, ServiceError> {
        let result = schtasks(&["/Query", "/TN", &plan.id, "/XML"])?;
        if result.status.success() {
            return decode_xml(&result.stdout).map(Some);
        }
        // schtasks uses the same generic exit code for an absent task and an
        // inaccessible task. A successful enumeration proves absence; otherwise
        // installation must not overwrite or adopt an unknown task.
        let all = schtasks(&["/Query", "/FO", "CSV", "/NH"])?;
        if !all.status.success() {
            return Err(ServiceError::new(
                ServiceErrorCode::OwnershipUnknown,
                "Cannot determine Computer helper task ownership",
            ));
        }
        let all = String::from_utf8_lossy(&all.stdout);
        let expected = format!("\\{}", plan.id);
        if all
            .lines()
            .any(|line| line.trim_start().starts_with(&format!("\"{}\",", expected)))
        {
            return Err(ServiceError::new(
                ServiceErrorCode::OwnershipUnknown,
                "Computer helper task exists but cannot be inspected",
            ));
        }
        Ok(None)
    }
    pub(super) fn inspect(plan: &HelperPlan) -> Result<ServiceStatus, ServiceError> {
        validate(plan)?;
        match query(plan)? {
            None => Ok(status(plan, Ownership::Absent)),
            Some(source) if owned_task(plan, &source) => Ok(status(plan, Ownership::Owned)),
            Some(_) => Ok(status(plan, Ownership::Foreign)),
        }
    }
    pub(super) fn preflight(plan: &HelperPlan) -> Result<ServiceStatus, ServiceError> {
        let current = inspect(plan)?;
        if current.ownership == Ownership::Foreign {
            return Err(ServiceError::new(
                ServiceErrorCode::ForeignService,
                "Computer helper task belongs to another owner",
            ));
        }
        Ok(current)
    }
    pub(super) fn install(plan: &HelperPlan) -> Result<ServiceStatus, ServiceError> {
        validate(plan)?;
        if let Some(source) = query(plan)? {
            if owned_task(plan, &source) {
                return Ok(start_current_login(plan, status(plan, Ownership::Owned)));
            }
            return Err(ServiceError::new(
                ServiceErrorCode::ForeignService,
                "Computer helper task belongs to another owner",
            ));
        }
        let xml = render_task(plan)?;
        let path = plan.session_dir.join(format!(".{}-task.xml", plan.id));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|_| {
                ServiceError::new(
                    ServiceErrorCode::Busy,
                    "Computer helper task staging file already exists",
                )
            })?;
        use std::io::Write;
        // schtasks accepts UTF-16 XML reliably even when paths contain Unicode.
        let utf16: Vec<u8> = std::iter::once(0xFEFF_u16)
            .chain(xml.encode_utf16())
            .flat_map(u16::to_le_bytes)
            .collect();
        let write = file.write_all(&utf16).and_then(|()| file.sync_all());
        drop(file);
        if write.is_err() {
            let _ = std::fs::remove_file(&path);
            return Err(ServiceError::new(
                ServiceErrorCode::OperationFailed,
                "Computer helper task XML could not be staged",
            ));
        }
        let path_text = path.to_str().ok_or_else(|| {
            ServiceError::new(
                ServiceErrorCode::InvalidSpec,
                "Computer helper task path is not UTF-8",
            )
        })?;
        let result = schtasks(&["/Create", "/TN", &plan.id, "/XML", path_text]);
        let _ = std::fs::remove_file(&path);
        let result = result?;
        if !result.status.success() {
            return Err(ServiceError::new(
                ServiceErrorCode::OutcomeUnknown,
                "Computer helper task creation failed; inspect before retrying",
            ));
        }
        let confirmed = inspect(plan)?;
        if confirmed.ownership != Ownership::Owned {
            return Err(ServiceError::new(
                ServiceErrorCode::OutcomeUnknown,
                "Computer helper task creation was not confirmed",
            ));
        }
        Ok(start_current_login(plan, confirmed))
    }
    fn start_current_login(plan: &HelperPlan, mut installed: ServiceStatus) -> ServiceStatus {
        // The task principal is the verified project-user SID with
        // InteractiveToken. /Run works for an already signed-in owner without
        // Desktop, while the helper/Runner handshake decides which unlocked
        // OS session (if any) may offer Computer Use.
        if schtasks(&["/Run", "/TN", &plan.id]).is_ok_and(|result| result.status.success()) {
            installed.detail = Some("Start requested for the selected user's interactive task; Runner must verify the exact unlocked session".into());
        } else {
            installed.detail = Some("Current-session helper start was not confirmed; the selected user's login task remains installed".into());
        }
        installed
    }
    pub(super) fn remove(plan: &HelperPlan) -> Result<ServiceStatus, ServiceError> {
        match preflight(plan)? {
            current if current.ownership == Ownership::Absent => return Ok(current),
            _ => {}
        }
        let result = schtasks(&["/Delete", "/TN", &plan.id, "/F"])?;
        if !result.status.success() {
            return Err(ServiceError::new(
                ServiceErrorCode::OutcomeUnknown,
                "Computer helper task removal failed",
            ));
        }
        let remaining = inspect(plan)?;
        if remaining.ownership != Ownership::Absent {
            return Err(ServiceError::new(
                ServiceErrorCode::OutcomeUnknown,
                "Computer helper task remains after removal",
            ));
        }
        Ok(remaining)
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
    use std::process::Command;

    fn uid(plan: &HelperPlan) -> Result<u32, ServiceError> {
        plan.account_identity.parse::<u32>().map_err(|_| {
            ServiceError::new(
                ServiceErrorCode::InvalidSpec,
                "Computer helper expected UID is invalid",
            )
        })
    }
    fn ensure_owner(path: &Path, expected: u32) -> Result<(), ServiceError> {
        let current = fs::symlink_metadata(path).map_err(|_| {
            ServiceError::new(
                ServiceErrorCode::OutcomeUnknown,
                "Computer LaunchAgent path disappeared",
            )
        })?;
        if current.uid() == expected {
            return Ok(());
        }
        let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|_| {
            ServiceError::new(
                ServiceErrorCode::InvalidSpec,
                "Computer LaunchAgent path contains NUL",
            )
        })?;
        if unsafe { libc::chown(c_path.as_ptr(), expected, u32::MAX) } != 0 {
            return Err(ServiceError::new(
                ServiceErrorCode::PermissionDenied,
                "Cannot assign Computer LaunchAgent to selected user",
            ));
        }
        Ok(())
    }
    fn plist_path(plan: &HelperPlan) -> Result<PathBuf, ServiceError> {
        let home = plan.home.as_ref().ok_or_else(|| {
            ServiceError::new(
                ServiceErrorCode::InvalidSpec,
                "Computer helper account home is missing",
            )
        })?;
        if !home.is_absolute() {
            return Err(ServiceError::new(
                ServiceErrorCode::InvalidSpec,
                "Computer helper account home must be absolute",
            ));
        }
        let meta = fs::symlink_metadata(home).map_err(|_| {
            ServiceError::new(
                ServiceErrorCode::MissingPrerequisite,
                "Computer helper account home is absent",
            )
        })?;
        if !meta.is_dir() || meta.file_type().is_symlink() || meta.uid() != uid(plan)? {
            return Err(ServiceError::new(
                ServiceErrorCode::PermissionDenied,
                "Computer helper account home owner changed",
            ));
        }
        Ok(home
            .join("Library/LaunchAgents")
            .join(format!("org.webcodex.{}.plist", plan.id)))
    }
    fn validate_dir(plan: &HelperPlan) -> Result<(), ServiceError> {
        let meta = fs::symlink_metadata(&plan.session_dir).map_err(|_| {
            ServiceError::new(
                ServiceErrorCode::MissingPrerequisite,
                "Computer session directory disappeared",
            )
        })?;
        if meta.uid() != uid(plan)? || meta.mode() & 0o077 != 0 {
            return Err(ServiceError::new(
                ServiceErrorCode::PermissionDenied,
                "Computer session directory must be owner-only",
            ));
        }
        Ok(())
    }
    fn render_plist(plan: &HelperPlan) -> Result<String, ServiceError> {
        let program = plan.program.to_str().ok_or_else(|| {
            ServiceError::new(
                ServiceErrorCode::InvalidSpec,
                "Computer helper program path is not UTF-8",
            )
        })?;
        let dir = plan.session_dir.to_str().ok_or_else(|| {
            ServiceError::new(
                ServiceErrorCode::InvalidSpec,
                "Computer session path is not UTF-8",
            )
        })?;
        Ok(format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\"><plist version=\"1.0\"><dict><key>Label</key><string>org.webcodex.{}</string><key>ProgramArguments</key><array><string>{}</string><string>--computer-session-helper</string><string>--session-state-dir</string><string>{}</string></array><key>LimitLoadToSessionType</key><string>Aqua</string><key>RunAtLoad</key><true/><key>KeepAlive</key><true/><key>StandardErrorPath</key><string>/dev/null</string><key>StandardOutPath</key><string>/dev/null</string><!-- {} --></dict></plist>\n", xml(&plan.id), xml(program), xml(dir), xml(&plan.marker)))
    }
    pub(super) fn inspect(plan: &HelperPlan) -> Result<ServiceStatus, ServiceError> {
        validate_dir(plan)?;
        let path = plist_path(plan)?;
        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(status(plan, Ownership::Absent))
            }
            Err(_) => {
                return Err(ServiceError::new(
                    ServiceErrorCode::OwnershipUnknown,
                    "Computer LaunchAgent cannot be inspected",
                ))
            }
        };
        if !meta.is_file()
            || meta.file_type().is_symlink()
            || meta.uid() != uid(plan)?
            || meta.mode() & 0o077 != 0
        {
            return Ok(status(plan, Ownership::Foreign));
        }
        let current = fs::read_to_string(&path).map_err(|_| {
            ServiceError::new(
                ServiceErrorCode::OwnershipUnknown,
                "Computer LaunchAgent cannot be read",
            )
        })?;
        Ok(status(
            plan,
            if current == render_plist(plan)? {
                Ownership::Owned
            } else {
                Ownership::Foreign
            },
        ))
    }
    pub(super) fn preflight(plan: &HelperPlan) -> Result<ServiceStatus, ServiceError> {
        let current = inspect(plan)?;
        if current.ownership == Ownership::Foreign {
            return Err(ServiceError::new(
                ServiceErrorCode::ForeignService,
                "Computer LaunchAgent belongs to another owner",
            ));
        }
        Ok(current)
    }
    pub(super) fn install(plan: &HelperPlan) -> Result<ServiceStatus, ServiceError> {
        let current = preflight(plan)?;
        if current.ownership == Ownership::Owned {
            return Ok(start_current_login(plan, current));
        }
        let path = plist_path(plan)?;
        let parent = path.parent().unwrap();
        let library = parent.parent().unwrap();
        let meta = fs::symlink_metadata(library).map_err(|_| {
            ServiceError::new(
                ServiceErrorCode::MissingPrerequisite,
                "Account Library directory is absent",
            )
        })?;
        if !meta.is_dir() || meta.file_type().is_symlink() || meta.uid() != uid(plan)? {
            return Err(ServiceError::new(
                ServiceErrorCode::PermissionDenied,
                "Account Library directory owner changed",
            ));
        }
        if !parent.exists() {
            fs::create_dir(parent).map_err(|_| {
                ServiceError::new(
                    ServiceErrorCode::OperationFailed,
                    "Cannot create Computer LaunchAgents directory",
                )
            })?;
            ensure_owner(parent, uid(plan)?)?;
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).map_err(|_| {
                ServiceError::new(
                    ServiceErrorCode::OperationFailed,
                    "Cannot secure Computer LaunchAgents directory",
                )
            })?;
        }
        let parent_meta = fs::symlink_metadata(parent).map_err(|_| {
            ServiceError::new(
                ServiceErrorCode::OwnershipUnknown,
                "Computer LaunchAgents directory disappeared",
            )
        })?;
        if !parent_meta.is_dir()
            || parent_meta.file_type().is_symlink()
            || parent_meta.uid() != uid(plan)?
            || parent_meta.mode() & 0o077 != 0
        {
            return Err(ServiceError::new(
                ServiceErrorCode::PermissionDenied,
                "Computer LaunchAgents directory is not owner-only",
            ));
        }
        let temp = parent.join(format!(
            ".{}.{}.tmp",
            plan.id,
            uuid::Uuid::new_v4().simple()
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)
            .map_err(|_| {
                ServiceError::new(
                    ServiceErrorCode::OperationFailed,
                    "Cannot stage Computer LaunchAgent",
                )
            })?;
        if file
            .write_all(render_plist(plan)?.as_bytes())
            .and_then(|()| file.sync_all())
            .is_err()
        {
            drop(file);
            let _ = fs::remove_file(&temp);
            return Err(ServiceError::new(
                ServiceErrorCode::OperationFailed,
                "Cannot write Computer LaunchAgent",
            ));
        }
        drop(file);
        if let Err(error) = ensure_owner(&temp, uid(plan)?) {
            let _ = fs::remove_file(&temp);
            return Err(error);
        }
        let result = fs::hard_link(&temp, &path);
        let _ = fs::remove_file(&temp);
        match result {
            Ok(()) => inspect(plan).map(|installed| start_current_login(plan, installed)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(ServiceError::new(
                    ServiceErrorCode::ForeignService,
                    "Computer LaunchAgent appeared during install",
                ))
            }
            Err(_) => Err(ServiceError::new(
                ServiceErrorCode::OutcomeUnknown,
                "Computer LaunchAgent install failed; inspect before retrying",
            )),
        }
    }
    fn start_current_login(plan: &HelperPlan, mut installed: ServiceStatus) -> ServiceStatus {
        let Ok(path) = plist_path(plan) else {
            return installed;
        };
        let Ok(uid) = uid(plan) else {
            return installed;
        };
        let target = format!("gui/{uid}/org.webcodex.{}", plan.id);
        let loaded = Command::new("/bin/launchctl")
            .args(["print", &target])
            .bounded_output()
            .is_ok_and(|output| output.status.success());
        if !loaded {
            let started = Command::new("/bin/launchctl")
                .args(["bootstrap", &format!("gui/{uid}"), &path.to_string_lossy()])
                .bounded_output()
                .is_ok_and(|output| output.status.success());
            if !started {
                installed.detail = Some("Current-session LaunchAgent start was not confirmed; the selected user's login agent remains installed".into());
            } else {
                installed.detail = Some("LaunchAgent start requested in the selected user's GUI domain; Runner must verify the exact unlocked session".into());
            }
        } else {
            installed.detail = Some("LaunchAgent is loaded for the selected user; Runner must verify the exact unlocked session".into());
        }
        installed
    }
    pub(super) fn remove(plan: &HelperPlan) -> Result<ServiceStatus, ServiceError> {
        let current = preflight(plan)?;
        if current.ownership == Ownership::Absent {
            return Ok(current);
        }
        let uid = uid(plan)?;
        let label = format!("gui/{}/org.webcodex.{}", uid, plan.id);
        let loaded = Command::new("/bin/launchctl")
            .args(["print", &label])
            .bounded_output()
            .map_err(|_| {
                ServiceError::new(
                    ServiceErrorCode::OutcomeUnknown,
                    "Cannot inspect loaded Computer LaunchAgent",
                )
            })?;
        if loaded.status.success() {
            let removed = Command::new("/bin/launchctl")
                .args(["bootout", &label])
                .bounded_output()
                .map_err(|_| {
                    ServiceError::new(
                        ServiceErrorCode::OutcomeUnknown,
                        "Cannot unload Computer LaunchAgent",
                    )
                })?;
            if !removed.status.success() {
                return Err(ServiceError::new(
                    ServiceErrorCode::OutcomeUnknown,
                    "Cannot unload Computer LaunchAgent",
                ));
            }
        }
        fs::remove_file(plist_path(plan)?).map_err(|_| {
            ServiceError::new(
                ServiceErrorCode::OutcomeUnknown,
                "Cannot remove Computer LaunchAgent",
            )
        })?;
        inspect(plan)
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod platform {
    use super::*;
    fn unsupported() -> ServiceError {
        ServiceError::new(
            ServiceErrorCode::Unsupported,
            "Computer session helper autostart requires Windows or macOS",
        )
    }
    pub(super) fn preflight(_: &HelperPlan) -> Result<ServiceStatus, ServiceError> {
        Err(unsupported())
    }
    pub(super) fn inspect(_: &HelperPlan) -> Result<ServiceStatus, ServiceError> {
        Err(unsupported())
    }
    pub(super) fn install(_: &HelperPlan) -> Result<ServiceStatus, ServiceError> {
        Err(unsupported())
    }
    pub(super) fn remove(_: &HelperPlan) -> Result<ServiceStatus, ServiceError> {
        Err(unsupported())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn spec(dir: &Path) -> ServiceSpec {
        ServiceSpec {
            id: "WebCodexRunner-test".into(),
            component: Component::Runner,
            program: PathBuf::from("/opt/webcodex/webcodex-runner"),
            args: if cfg!(windows) {
                vec![
                    "--windows-service".into(),
                    "WebCodexRunner-test".into(),
                    "--config".into(),
                    "/opt/webcodex/runner.toml".into(),
                    "--computer-session-dir".into(),
                    dir.to_string_lossy().into_owned(),
                ]
            } else {
                vec![
                    "--config".into(),
                    "/opt/webcodex/runner.toml".into(),
                    "--computer-session-dir".into(),
                    dir.to_string_lossy().into_owned(),
                ]
            },
            working_directory: dir.to_path_buf(),
            account: ServiceAccount::SystemUser {
                name: "owner".into(),
                group: None,
                expected_identity: "501".into(),
                home: Some(PathBuf::from("/Users/owner")),
            },
            config_identity: "environment-id".into(),
            env_file: None,
            environment: Default::default(),
            linux_socket: None,
        }
    }
    #[test]
    fn plan_uses_only_runner_binary_and_shared_directory() {
        let dir = tempfile::tempdir().unwrap();
        let mut spec = spec(dir.path());
        let helper = plan(&spec, dir.path()).unwrap();
        assert_eq!(helper.program, spec.program);
        assert_eq!(helper.session_dir, dir.path());
        assert!(!helper.marker.contains("token"));
        spec.args.push("--token".into());
        assert!(plan(&spec, dir.path()).is_err());
    }
    #[test]
    fn different_owner_cannot_share_helper_name() {
        let dir = tempfile::tempdir().unwrap();
        let a = plan(&spec(dir.path()), dir.path()).unwrap();
        let mut other = spec(dir.path());
        other.config_identity = "foreign".into();
        let b = plan(&other, dir.path()).unwrap();
        assert_ne!(a.id, b.id);
        assert_ne!(a.marker, b.marker);
    }
    #[test]
    fn windows_task_contract_is_interactive_limited_and_owned_exactly() {
        let dir = tempfile::tempdir().unwrap();
        let mut helper = plan(&spec(dir.path()), dir.path()).unwrap();
        helper.account_identity = "S-1-5-21-100".into();
        helper.program = PathBuf::from("C:/Program Files/WebCodex/runner.exe");
        let xml = render_task(&helper).unwrap();
        assert!(xml.contains("<LogonType>InteractiveToken</LogonType>"));
        assert!(xml.contains("<RunLevel>LeastPrivilege</RunLevel>"));
        assert!(xml.contains("<LogonTrigger>"));
        assert!(xml.contains("--computer-session-helper"));
        assert!(!xml.contains("--config"));
        assert!(!xml.contains("--token"));
        assert!(owned_task(&helper, &xml), "{:?}", task_fields(&xml));
        for (from, to) in [
            ("InteractiveToken", "Password"),
            ("LeastPrivilege", "HighestAvailable"),
            ("S-1-5-21-100", "S-1-5-21-101"),
            ("--computer-session-helper", "--other-mode"),
        ] {
            assert!(
                !owned_task(&helper, &xml.replace(from, to)),
                "accepted altered task: {from}"
            );
        }
    }
}
