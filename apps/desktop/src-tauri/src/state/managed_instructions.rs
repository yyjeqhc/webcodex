use super::*;
use crate::managed_instructions::{EnableRequest, SaveRequest, Snapshot};
#[cfg(test)]
mod tests;

impl AppState {
    /// Independent of Project discovery and an online Runtime; observation does
    /// not create a file or append it to any configuration.
    pub async fn managed_instructions_read(&self) -> DesktopResult<Snapshot> {
        let managed = Arc::clone(&self.managed_instructions);
        tokio::task::spawn_blocking(move || managed.read())
            .await
            .map_err(|_| desktop_state_unavailable("Instructions worker stopped"))?
    }

    pub async fn managed_instructions_save(&self, request: SaveRequest) -> DesktopResult<Snapshot> {
        let managed = Arc::clone(&self.managed_instructions);
        tokio::task::spawn_blocking(move || managed.save(request))
            .await
            .map_err(|_| desktop_state_unavailable("Instructions worker stopped"))?
    }

    pub async fn managed_instructions_enable(
        &self,
        request: EnableRequest,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::RunnerSettingsUpdate, false)
            .await?;
        let result = async {
            let runtime = core
                .config
                .runtime
                .clone()
                .ok_or_else(|| desktop_state_unavailable("Configure a Runner first"))?;
            crate::webcodex::settings::verify_target(&runtime, &request.target)?;
            let managed = Arc::clone(&self.managed_instructions);
            let edit = tokio::task::spawn_blocking({
                let runtime = runtime.clone();
                move || {
                    // A stale settings selection cannot create a managed file.
                    let current = crate::webcodex::settings::inspect(&runtime, false)?;
                    if current.paths != request.expected {
                        return Err(desktop_state_unavailable(
                            "Runner settings changed; reload before enabling instructions",
                        ));
                    }
                    managed.ensure(&request.expected_revision)?;
                    crate::webcodex::settings::stage_managed_instructions(
                        &runtime,
                        request.target,
                        request.expected,
                        &managed.path(),
                    )
                }
            })
            .await
            .map_err(|_| desktop_state_unavailable("Instructions worker stopped"))??;
            // Failure never deletes user content. A definitely rejected config
            // is restored; indeterminate reloads remain inspectable, not replayed.
            workspace_settings::apply_staged_settings(&runtime, &edit).await?;
            core.get_state().await
        }
        .await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }
}
