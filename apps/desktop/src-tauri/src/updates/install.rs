mod native;
use crate::runtime_selection::RuntimeSource;
use crate::webcodex::cli::{ResolvedBinaries, ResolvedBinarySource};
pub(crate) use shared::detected_installer_target;
pub use shared::InstallContext;
use webcodex_core::desktop_runtime_contract::MachineBuildInfo;
use webcodex_environment::unified_update::{self as shared, InstallationKind, InstalledBinaries};

pub fn assess_installation(
    build: MachineBuildInfo,
    binaries: Option<ResolvedBinaries>,
    source: RuntimeSource,
    environment_id: Option<String>,
) -> (InstallationKind, Option<InstallContext>) {
    let Some(root) = webcodex_environment::default_environment_dir().ok() else {
        return (InstallationKind::EnvironmentNotConfigured, None);
    };
    let Some(desktop) = std::env::current_exe().ok() else {
        return (InstallationKind::UnmanagedInstallation, None);
    };
    let binaries = binaries.map(|b| InstalledBinaries {
        directory: b.directory,
        binaries: webcodex_environment::RuntimeBinaries {
            cli: b.webcodex,
            server: b.server,
            runner: b.runner,
        },
        builds: b.builds,
        bundled: matches!(b.source, ResolvedBinarySource::Bundled),
    });
    shared::assess_installation(
        build,
        binaries,
        matches!(source, RuntimeSource::Bundled),
        environment_id,
        root,
        desktop,
    )
}

pub(crate) struct NativeLaunchAdapter;
impl shared::LaunchAdapter for NativeLaunchAdapter {
    fn supported(&self, target: shared::InstallerTarget) -> bool {
        native::supported(target)
    }
    fn launch<'a>(
        &'a self,
        request: shared::LaunchRequest<'a>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = shared::LaunchOutcome> + Send + 'a>>
    {
        Box::pin(async move {
            #[cfg(unix)]
            {
                native::launch_unix(
                    request.cli,
                    request.receipt,
                    request.candidate,
                    request.installer,
                    request.version,
                    request.operation_id,
                    request.target,
                )
                .await
            }
            #[cfg(windows)]
            {
                let Some(handoff) = request.windows_handoff else {
                    return shared::LaunchOutcome::NotStarted(
                        shared::UpdateError::GuardedHandoffUnavailable,
                    );
                };
                native::launch_windows(
                    request.installer,
                    request.installer_sha256,
                    request.environment_root,
                    handoff,
                )
                .await
            }
            #[cfg(not(any(unix, windows)))]
            {
                let _ = request;
                shared::LaunchOutcome::NotStarted(shared::UpdateError::UnsupportedPlatform)
            }
        })
    }
}
