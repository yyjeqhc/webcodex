//! Native, literal-argv handoff only. No shell, password field, self-overwrite,
//! or platform opener guessed from user-controlled environment variables.
use std::ffi::OsString;
use std::path::Path;
use webcodex_environment::unified_update::{InstallerTarget, PackageFormat, UpdateError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LaunchOutcome {
    Started,
    NotStarted(UpdateError),
    Unknown,
}

pub(super) fn supported(target: InstallerTarget) -> bool {
    if !target.valid() {
        return false;
    }
    #[cfg(target_os = "macos")]
    {
        target.format == PackageFormat::Pkg
            && Path::new("/usr/sbin/installer").is_file()
            && Path::new("/usr/sbin/pkgutil").is_file()
    }
    #[cfg(target_os = "linux")]
    {
        Path::new("/usr/bin/pkexec").is_file()
            && match target.format {
                PackageFormat::Deb => {
                    Path::new("/usr/bin/dpkg").is_file() && Path::new("/usr/bin/dpkg-deb").is_file()
                }
                PackageFormat::Rpm => {
                    Path::new("/usr/bin/rpm").is_file()
                        && Path::new("/usr/bin/rpm2cpio").is_file()
                        && Path::new("/usr/bin/cpio").is_file()
                }
                _ => false,
            }
    }
    #[cfg(windows)]
    {
        target.format == PackageFormat::Exe
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = target;
        false
    }
}

#[cfg(unix)]
fn helper_arguments(
    receipt: &Path,
    candidate: &Path,
    installer: &Path,
    target: InstallerTarget,
) -> Vec<OsString> {
    vec![
        "environment".into(),
        "installer-apply".into(),
        "--upgrade-receipt".into(),
        receipt.as_os_str().into(),
        "--candidate-dir".into(),
        candidate.as_os_str().into(),
        "--installer-file".into(),
        installer.as_os_str().into(),
        "--installer-target".into(),
        target.as_str().into(),
        "--json".into(),
    ]
}

#[cfg(unix)]
fn decode_notice(bytes: &[u8], version: &str, operation_id: &str) -> LaunchOutcome {
    use webcodex_environment::unified_update::InstallerLaunchNotice;
    if bytes.len() > 4096 {
        return LaunchOutcome::Unknown;
    }
    let Ok(notice) = serde_json::from_slice::<InstallerLaunchNotice>(bytes) else {
        return LaunchOutcome::Unknown;
    };
    if notice.schema_version != 1 {
        return LaunchOutcome::Unknown;
    }
    if notice.started {
        if notice.safe_to_restore
            || notice.error_kind.is_some()
            || notice.version.as_deref() != Some(version)
            || notice.operation_id.as_deref() != Some(operation_id)
        {
            return LaunchOutcome::Unknown;
        }
        LaunchOutcome::Started
    } else if notice.safe_to_restore
        && notice
            .error_kind
            .is_some_and(|e| e != UpdateError::RecoveryRequired)
    {
        LaunchOutcome::NotStarted(notice.error_kind.unwrap())
    } else {
        LaunchOutcome::Unknown
    }
}

#[cfg(target_os = "linux")]
pub(super) async fn launch_unix(
    cli: &Path,
    receipt: &Path,
    candidate: &Path,
    installer: &Path,
    version: &str,
    operation_id: &str,
    target: InstallerTarget,
) -> LaunchOutcome {
    use std::process::Stdio;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
    if let Err(error) = webcodex_environment::unified_update::verify_installed_update_cli(cli) {
        return LaunchOutcome::NotStarted(error);
    }
    let mut child = match tokio::process::Command::new("/usr/bin/pkexec")
        .arg("--disable-internal-agent")
        .arg(cli)
        .args(helper_arguments(receipt, candidate, installer, target))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(false)
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return LaunchOutcome::NotStarted(UpdateError::InstallerLaunchFailed),
    };
    let Some(stdout) = child.stdout.take() else {
        return LaunchOutcome::Unknown;
    };
    let mut reader = BufReader::new(stdout.take(4097));
    let mut bytes = Vec::new();
    let read = tokio::time::timeout(
        std::time::Duration::from_secs(600),
        reader.read_until(b'\n', &mut bytes),
    )
    .await;
    if matches!(read, Ok(Ok(_))) && bytes.last() == Some(&b'\n') {
        return decode_notice(&bytes, version, operation_id);
    }
    if bytes.is_empty() && matches!(read, Ok(Ok(0))) {
        // pkexec documents 126 for dismissal and 127 for denied/no usable
        // authentication. Our trusted helper never uses these exit codes.
        if let Ok(Ok(status)) =
            tokio::time::timeout(std::time::Duration::from_secs(2), child.wait()).await
        {
            if matches!(status.code(), Some(126 | 127)) {
                return LaunchOutcome::NotStarted(UpdateError::AuthorizationRequired);
            }
        }
    }
    // Do not kill a privileged helper: it may already own a package transaction.
    LaunchOutcome::Unknown
}

#[cfg(target_os = "macos")]
pub(super) async fn launch_unix(
    cli: &Path,
    receipt: &Path,
    candidate: &Path,
    installer: &Path,
    version: &str,
    operation_id: &str,
    target: InstallerTarget,
) -> LaunchOutcome {
    let cli = cli.to_path_buf();
    let args = helper_arguments(receipt, candidate, installer, target);
    let version = version.to_owned();
    let operation_id = operation_id.to_owned();
    tokio::task::spawn_blocking(move || macos::launch(&cli, &args, &version, &operation_id))
        .await
        .unwrap_or(LaunchOutcome::Unknown)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
#[cfg(unix)]
pub(super) async fn launch_unix(
    _: &Path,
    _: &Path,
    _: &Path,
    _: &Path,
    _: &str,
    _: &str,
) -> LaunchOutcome {
    LaunchOutcome::NotStarted(UpdateError::UnsupportedPlatform)
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::ffi::{c_char, c_void, CString};
    use std::os::unix::ffi::OsStrExt;
    use std::time::{Duration, Instant};

    #[repr(C)]
    struct Item {
        name: *const c_char,
        value_length: usize,
        value: *mut c_void,
        flags: u32,
    }
    #[repr(C)]
    struct Items {
        count: u32,
        items: *mut Item,
    }
    #[link(name = "Security", kind = "framework")]
    unsafe extern "C" {
        fn AuthorizationCreate(
            rights: *const Items,
            environment: *const Items,
            flags: u32,
            authorization: *mut *mut c_void,
        ) -> i32;
        fn AuthorizationCopyRights(
            authorization: *mut c_void,
            rights: *const Items,
            environment: *const Items,
            flags: u32,
            authorized: *mut *mut Items,
        ) -> i32;
        fn AuthorizationExecuteWithPrivileges(
            authorization: *mut c_void,
            path: *const c_char,
            flags: u32,
            arguments: *const *mut c_char,
            pipe: *mut *mut libc::FILE,
        ) -> i32;
        fn AuthorizationFree(authorization: *mut c_void, flags: u32) -> i32;
    }
    struct Authorization(*mut c_void);
    impl Drop for Authorization {
        fn drop(&mut self) {
            unsafe {
                AuthorizationFree(self.0, 1 << 3);
            }
        }
    }
    struct Pipe(*mut libc::FILE);
    impl Drop for Pipe {
        fn drop(&mut self) {
            unsafe {
                libc::fclose(self.0);
            }
        }
    }

    pub(super) fn launch(
        cli: &Path,
        args: &[OsString],
        version: &str,
        operation_id: &str,
    ) -> LaunchOutcome {
        if let Err(error) = webcodex_environment::unified_update::verify_installed_update_cli(cli) {
            return LaunchOutcome::NotStarted(error);
        }
        let Ok(executable) = CString::new(cli.as_os_str().as_bytes()) else {
            return LaunchOutcome::NotStarted(UpdateError::InstallerLaunchFailed);
        };
        let Ok(arguments) = args
            .iter()
            .map(|arg| CString::new(arg.as_os_str().as_bytes()))
            .collect::<Result<Vec<_>, _>>()
        else {
            return LaunchOutcome::NotStarted(UpdateError::InstallerLaunchFailed);
        };
        let mut argv: Vec<_> = arguments
            .iter()
            .map(|arg| arg.as_ptr().cast_mut())
            .collect();
        argv.push(std::ptr::null_mut());
        let mut reference = std::ptr::null_mut();
        if unsafe { AuthorizationCreate(std::ptr::null(), std::ptr::null(), 0, &mut reference) }
            != 0
            || reference.is_null()
        {
            return LaunchOutcome::NotStarted(UpdateError::AuthorizationRequired);
        }
        let authorization = Authorization(reference);
        let mut item = Item {
            name: c"system.privilege.admin".as_ptr(),
            value_length: 0,
            value: std::ptr::null_mut(),
            flags: 0,
        };
        let rights = Items {
            count: 1,
            items: &mut item,
        };
        if unsafe {
            AuthorizationCopyRights(
                authorization.0,
                &rights,
                std::ptr::null(),
                3,
                std::ptr::null_mut(),
            )
        } != 0
        {
            return LaunchOutcome::NotStarted(UpdateError::AuthorizationRequired);
        }
        if let Err(error) = webcodex_environment::unified_update::verify_installed_update_cli(cli) {
            return LaunchOutcome::NotStarted(error);
        }
        // This SDK API is deprecated in favor of a separately signed,
        // launchd-managed helper. v1 deliberately adds no background daemon:
        // constrain it to the existing protected CLI and literal argv, and
        // retain all original-user Core checks in installer-apply. The OS owns
        // the password UI. No authorization reference or password is persisted.
        let mut raw_pipe = std::ptr::null_mut();
        let status = unsafe {
            AuthorizationExecuteWithPrivileges(
                authorization.0,
                executable.as_ptr(),
                0,
                argv.as_ptr(),
                &mut raw_pipe,
            )
        };
        if status != 0 || raw_pipe.is_null() {
            return if matches!(status, -60005 | -60006) {
                LaunchOutcome::NotStarted(UpdateError::AuthorizationRequired)
            } else {
                LaunchOutcome::Unknown
            };
        }
        let pipe = Pipe(raw_pipe);
        let descriptor = unsafe { libc::fileno(pipe.0) };
        let deadline = Instant::now() + Duration::from_secs(600);
        let mut bytes = Vec::new();
        loop {
            if Instant::now() >= deadline {
                return LaunchOutcome::Unknown;
            }
            let mut descriptor_poll = libc::pollfd {
                fd: descriptor,
                events: libc::POLLIN,
                revents: 0,
            };
            let polled = unsafe { libc::poll(&mut descriptor_poll, 1, 1000) };
            if polled < 0 {
                if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return LaunchOutcome::Unknown;
            }
            if polled == 0 {
                continue;
            }
            let mut buffer = [0u8; 1024];
            let length =
                unsafe { libc::read(descriptor, buffer.as_mut_ptr().cast(), buffer.len()) };
            if length <= 0 {
                return LaunchOutcome::Unknown;
            }
            bytes.extend_from_slice(&buffer[..length as usize]);
            if bytes.len() > 4096 {
                return LaunchOutcome::Unknown;
            }
            if let Some(end) = bytes.iter().position(|b| *b == b'\n') {
                return decode_notice(&bytes[..end], version, operation_id);
            }
        }
    }
}

#[cfg(windows)]
pub(super) fn launch_windows(
    installer: &Path,
    expected_sha256: &str,
    environment_dir: &Path,
) -> LaunchOutcome {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    use std::os::windows::{fs::OpenOptionsExt, process::CommandExt};
    // Hold the verified file without write/delete sharing until CreateProcess
    // has mapped its image; no path-based re-open gap after hashing.
    let mut locked = match std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(installer)
    {
        Ok(file) => file,
        Err(_) => return LaunchOutcome::NotStarted(UpdateError::InstallerLaunchFailed),
    };
    let mut hash = Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0u8; 128 * 1024];
    loop {
        let n = match locked.read(&mut buffer) {
            Ok(n) => n,
            Err(_) => return LaunchOutcome::NotStarted(UpdateError::ChecksumMismatch),
        };
        if n == 0 {
            break;
        }
        count = count.saturating_add(n as u64);
        if count > webcodex_environment::unified_update::MAX_INSTALLER_BYTES {
            return LaunchOutcome::NotStarted(UpdateError::DownloadTooLarge);
        }
        hash.update(&buffer[..n]);
    }
    if count == 0 || format!("{:x}", hash.finalize()) != expected_sha256 {
        return LaunchOutcome::NotStarted(UpdateError::ChecksumMismatch);
    }
    let mut environment = OsString::from("/ENVIRONMENTDIR=");
    environment.push(environment_dir.as_os_str());
    match std::process::Command::new(installer)
        .arg(environment)
        .creation_flags(0x0000_0200 | 0x0000_0008)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(_) => LaunchOutcome::Started,
        Err(_) => LaunchOutcome::NotStarted(UpdateError::InstallerLaunchFailed),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn target() -> InstallerTarget {
        let platform = webcodex_environment::unified_update::RuntimePlatform::current().unwrap();
        InstallerTarget::default_for_non_linux(platform)
            .or_else(|| InstallerTarget::for_platform(platform, PackageFormat::Deb))
            .unwrap()
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[tokio::test]
    async fn an_untrusted_cli_is_rejected_before_any_os_authorization_or_process() {
        let path = Path::new("/private/not-the-installed-webcodex");
        assert_eq!(
            launch_unix(path, path, path, path, "1.2.3", "operation", target()).await,
            LaunchOutcome::NotStarted(UpdateError::UpgradePreflightFailed)
        );
    }

    #[test]
    fn helper_argv_never_interprets_metacharacters() {
        let path = Path::new("/Users/example/a b;$(touch nope)/package.pkg");
        let args = helper_arguments(path, path, path, target());
        assert_eq!(args.len(), 11);
        assert_eq!(args[3], path.as_os_str());
        assert_eq!(args[7], path.as_os_str());
        assert_eq!(args[8], "--installer-target");
        let target_name = target().as_str();
        assert_eq!(args[9].as_os_str(), std::ffi::OsStr::new(&target_name));
        assert!(!args.iter().any(|arg| arg == "sh" || arg == "sudo"));
    }
    #[test]
    fn acknowledgement_requires_exact_operation_and_never_claims_installed() {
        let mut notice = serde_json::json!({"schema_version":1,"started":true,"safe_to_restore":false,"operation_id":"operation","version":"1.2.3","error_kind":null});
        let decode = |value: &serde_json::Value| {
            decode_notice(&serde_json::to_vec(value).unwrap(), "1.2.3", "operation")
        };
        assert_eq!(decode(&notice), LaunchOutcome::Started);
        notice["operation_id"] = "other".into();
        assert_eq!(decode(&notice), LaunchOutcome::Unknown);
        notice["operation_id"] = "operation".into();
        notice["safe_to_restore"] = true.into();
        assert_eq!(decode(&notice), LaunchOutcome::Unknown);
        assert_eq!(
            decode_notice(b"Update installed successfully", "1.2.3", "operation"),
            LaunchOutcome::Unknown
        );
        let denied = webcodex_environment::unified_update::InstallerLaunchNotice::not_started(
            UpdateError::AuthorizationRequired,
        );
        assert_eq!(
            decode_notice(&serde_json::to_vec(&denied).unwrap(), "1.2.3", "operation"),
            LaunchOutcome::NotStarted(UpdateError::AuthorizationRequired)
        );
    }
}
