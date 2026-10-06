use std::{path::PathBuf, sync::Arc};
use webcodex_environment::unified_update::{self as shared, InstallerTarget, ReleaseNotice};
pub use webcodex_environment::unified_update::{DownloadStatus, InstallationKind};

pub struct UpdateManager {
    engine: Arc<shared::UpdateManager>,
}
impl std::ops::Deref for UpdateManager {
    type Target = shared::UpdateManager;
    fn deref(&self) -> &Self::Target {
        &self.engine
    }
}
impl UpdateManager {
    pub fn new(data_dir: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            engine: shared::UpdateManager::new(data_dir),
        })
    }
    pub fn request(
        self: &Arc<Self>,
        notice: Option<ReleaseNotice>,
        automatic: bool,
        manual: bool,
        installation: InstallationKind,
        target: Option<InstallerTarget>,
    ) {
        self.engine
            .request(notice, automatic, manual, installation, target);
    }
    pub async fn install_checked(
        &self,
        context: &shared::InstallContext,
        identity: &shared::CandidateIdentity,
        target: &webcodex_environment::UpgradeTarget,
        confirmed: bool,
    ) -> shared::UpdateResult<bool> {
        self.engine
            .install_checked(
                context,
                identity,
                target,
                confirmed,
                &super::install::NativeLaunchAdapter,
            )
            .await
            .map(|outcome| {
                matches!(
                    outcome,
                    shared::LaunchOutcome::Started | shared::LaunchOutcome::StartedWithOperation(_)
                )
            })
    }
}
