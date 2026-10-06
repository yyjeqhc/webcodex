//! Linux TTY authorization for the existing installed owner-receipt helper.
//! The owner process remains unprivileged; no shell or password collector.
use webcodex_environment::unified_update::{
    self as unified, LaunchAdapter, LaunchOutcome, LaunchRequest, UpdateError,
};

pub(super) struct TerminalLauncher;
impl LaunchAdapter for TerminalLauncher {
    fn supported(&self, target: unified::InstallerTarget) -> bool {
        supported(target)
    }
    fn launch<'a>(
        &'a self,
        request: LaunchRequest<'a>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = LaunchOutcome> + Send + 'a>> {
        Box::pin(launch(request))
    }
}

fn supported(target: unified::InstallerTarget) -> bool {
    #[cfg(target_os = "linux")]
    {
        if !trusted_system_file(std::path::Path::new("/usr/bin/sudo")) {
            return false;
        }
        let tools: &[&str] = match target.format {
            unified::PackageFormat::Deb => &["/usr/bin/dpkg", "/usr/bin/dpkg-deb"],
            unified::PackageFormat::Rpm => &["/usr/bin/rpm", "/usr/bin/rpm2cpio", "/usr/bin/cpio"],
            _ => return false,
        };
        target.valid()
            && tools
                .iter()
                .all(|tool| trusted_system_file(std::path::Path::new(tool)))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = target;
        false
    }
}

#[cfg(target_os = "linux")]
fn trusted_system_file(path: &std::path::Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    if !path.is_absolute()
        || !path.ancestors().all(|ancestor| {
            // The literal argv path must be protected too. Checking only its
            // resolved target would trust an alias in a writable directory.
            std::fs::symlink_metadata(ancestor).is_ok_and(|metadata| {
                metadata.uid() == 0 && (metadata.is_symlink() || metadata.mode() & 0o022 == 0)
            })
        })
    {
        return false;
    }
    let Ok(physical) = path.canonicalize() else {
        return false;
    };
    physical.ancestors().all(|ancestor| {
        std::fs::symlink_metadata(ancestor).is_ok_and(|metadata| {
            !metadata.is_symlink()
                && metadata.uid() == 0
                && metadata.mode() & 0o022 == 0
                && (ancestor != physical || (metadata.is_file() && metadata.mode() & 0o111 != 0))
        })
    })
}

#[cfg(target_os = "linux")]
fn arguments(request: &LaunchRequest<'_>) -> Vec<std::ffi::OsString> {
    vec![
        request.cli.as_os_str().into(),
        "environment".into(),
        "installer-apply".into(),
        "--upgrade-receipt".into(),
        request.receipt.as_os_str().into(),
        "--candidate-dir".into(),
        request.candidate.as_os_str().into(),
        "--installer-file".into(),
        request.installer.as_os_str().into(),
        "--installer-target".into(),
        request.target.as_str().into(),
        "--json".into(),
    ]
}

#[cfg(target_os = "linux")]
fn decode_notice(bytes: &[u8], version: &str, operation_id: &str) -> LaunchOutcome {
    if bytes.len() > 4096 {
        return LaunchOutcome::Unknown;
    }
    let Ok(notice) = serde_json::from_slice::<unified::InstallerLaunchNotice>(bytes) else {
        return LaunchOutcome::Unknown;
    };
    if notice.schema_version != 1 {
        return LaunchOutcome::Unknown;
    }
    if notice.started
        && !notice.safe_to_restore
        && notice.error_kind.is_none()
        && notice.version.as_deref() == Some(version)
        && notice.operation_id.as_deref() == Some(operation_id)
    {
        LaunchOutcome::Started
    } else if !notice.started
        && notice.safe_to_restore
        && notice
            .error_kind
            .is_some_and(|error| error != UpdateError::RecoveryRequired)
    {
        LaunchOutcome::NotStarted(notice.error_kind.unwrap())
    } else {
        LaunchOutcome::Unknown
    }
}

#[cfg(target_os = "linux")]
async fn launch(request: LaunchRequest<'_>) -> LaunchOutcome {
    use std::io::IsTerminal;
    use std::process::Stdio;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
    if !std::io::stdin().is_terminal() {
        return LaunchOutcome::NotStarted(UpdateError::AuthorizationRequired);
    }
    if !supported(request.target) {
        return LaunchOutcome::NotStarted(UpdateError::UnsupportedPlatform);
    }
    if let Err(error) = unified::verify_installed_update_cli(request.cli) {
        return LaunchOutcome::NotStarted(error);
    }
    let mut child = match tokio::process::Command::new("/usr/bin/sudo")
        .args(arguments(&request))
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(false)
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return LaunchOutcome::NotStarted(UpdateError::InstallerLaunchFailed),
    };
    let Some(stdout) = child.stdout.take() else {
        return LaunchOutcome::Unknown;
    };
    let mut reader = BufReader::new(stdout);
    let mut bytes = Vec::new();
    let acknowledgement = tokio::time::timeout(std::time::Duration::from_secs(600), async {
        (&mut reader).take(4097).read_until(b'\n', &mut bytes).await
    })
    .await;
    let outcome = match acknowledgement {
        Ok(Ok(_)) if bytes.last() == Some(&b'\n') => {
            decode_notice(&bytes, request.version, request.operation_id)
        }
        // No helper acknowledgement: sudo could have failed after dispatch.
        // An exit code alone never proves that replacement did not occur.
        _ => LaunchOutcome::Unknown,
    };
    if matches!(outcome, LaunchOutcome::Unknown) {
        return outcome;
    }
    // Drain any final safe helper reply without forwarding arbitrary stdout.
    // Keep the command alive through the package transaction; interruption
    // leaves authoritative recovery records and does not claim cancellation.
    let completion = tokio::time::timeout(std::time::Duration::from_secs(3600), async {
        let mut count = 0usize;
        let mut buffer = [0; 4096];
        loop {
            let length = reader.read(&mut buffer).await?;
            if length == 0 {
                break;
            }
            count = count.saturating_add(length);
            if count > 64 * 1024 {
                return Err(std::io::Error::other("bounded helper reply exceeded"));
            }
        }
        child.wait().await
    })
    .await;
    match (outcome, completion) {
        (LaunchOutcome::NotStarted(error), Ok(Ok(_))) => LaunchOutcome::NotStarted(error),
        (LaunchOutcome::Started, Ok(Ok(_))) => LaunchOutcome::Started,
        _ => LaunchOutcome::Unknown,
    }
}

#[cfg(not(target_os = "linux"))]
async fn launch(_: LaunchRequest<'_>) -> LaunchOutcome {
    LaunchOutcome::NotStarted(UpdateError::UnsupportedPlatform)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    #[test]
    fn notices_bind_exact_operation_and_never_infer_success_from_output() {
        let notice = serde_json::json!({"schema_version":1,"started":true,"safe_to_restore":false,"version":"1.2.3","operation_id":"selected","error_kind":null});
        let bytes = serde_json::to_vec(&notice).unwrap();
        assert_eq!(
            decode_notice(&bytes, "1.2.3", "selected"),
            LaunchOutcome::Started
        );
        assert_eq!(
            decode_notice(&bytes, "1.2.3", "other"),
            LaunchOutcome::Unknown
        );
        assert_eq!(
            decode_notice(&bytes, "1.2.4", "selected"),
            LaunchOutcome::Unknown
        );
        assert_eq!(
            decode_notice(b"private-canary-arbitrary-output", "1.2.3", "selected"),
            LaunchOutcome::Unknown
        );
        assert_eq!(
            decode_notice(&vec![b'x'; 4097], "1.2.3", "selected"),
            LaunchOutcome::Unknown
        );
    }
    #[test]
    fn writable_alias_to_a_trusted_tool_is_not_a_trusted_literal_path() {
        let directory = tempfile::tempdir().unwrap();
        let alias = directory.path().join("sudo");
        std::os::unix::fs::symlink("/usr/bin/true", &alias).unwrap();
        assert_eq!(
            alias.canonicalize().unwrap(),
            std::path::Path::new("/usr/bin/true")
                .canonicalize()
                .unwrap()
        );
        assert!(!trusted_system_file(&alias));
    }
    #[test]
    fn helper_paths_are_literal_arguments() {
        let cli = std::path::Path::new("/usr/lib/webcodex/webcodex-runtime/webcodex");
        let receipt = std::path::Path::new("/home/test/env with spaces/upgrade-prepared.json");
        let candidate = std::path::Path::new("/home/test/cache/$(touch injected)");
        let request = LaunchRequest {
            windows_handoff: None,
            cli,
            receipt,
            candidate,
            installer: std::path::Path::new("/home/test/cache/package.deb"),
            version: "1.2.3",
            operation_id: "selected",
            target: unified::InstallerTarget::new(
                unified::RuntimePlatform::LinuxX64,
                unified::PackageFormat::Deb,
            ),
            installer_sha256: "unused",
            environment_root: std::path::Path::new("/home/test/env with spaces"),
        };
        let argv = arguments(&request);
        assert_eq!(argv[0], cli.as_os_str());
        assert_eq!(argv[4], receipt.as_os_str());
        assert_eq!(argv[6], candidate.as_os_str());
        assert!(!argv
            .iter()
            .any(|arg| arg == "sh" || arg == "-c" || arg == "pkexec"));
    }
}
