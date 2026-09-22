use super::*;
use crate::coding_agents::{CodingAgentRemove, CodingAgentStore, CodingAgentUpdate};

impl AppState {
    pub async fn save_coding_agent(
        &self,
        request: CodingAgentUpdate,
    ) -> DesktopResult<DesktopStateSnapshot> {
        self.mutate_coding_agents(request.target.clone(), move |store| {
            store.stage_update(request)
        })
        .await
    }

    pub async fn remove_coding_agent(
        &self,
        request: CodingAgentRemove,
    ) -> DesktopResult<DesktopStateSnapshot> {
        self.mutate_coding_agents(request.target.clone(), move |store| {
            store.stage_remove(&request.provider_id, request.expected_revision)
        })
        .await
    }

    async fn mutate_coding_agents<F>(
        &self,
        target: crate::webcodex::settings::SettingsTarget,
        change: F,
    ) -> DesktopResult<DesktopStateSnapshot>
    where
        F: FnOnce(&mut CodingAgentStore) -> DesktopResult<()> + Send + 'static,
    {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::RunnerSettingsUpdate, false)
            .await?;
        let result = async {
            let runtime = core
                .config
                .runtime
                .clone()
                .ok_or_else(|| desktop_state_unavailable("Configure a Runner first"))?;
            crate::webcodex::settings::verify_target(&runtime, &target)?;
            let mut candidate = core.coding_agents.clone();
            let candidate = tokio::task::spawn_blocking(move || {
                change(&mut candidate)?;
                // Save is validation + desired-state commit, never a process restart.
                crate::webcodex::settings::reconcile_acp(&runtime, &candidate, true)?;
                candidate.commit()?;
                Ok::<_, DesktopError>(candidate)
            })
            .await
            .map_err(|_| desktop_state_unavailable("Coding Agent worker stopped"))??;
            core.coding_agents = candidate;
            core.get_state().await
        }
        .await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }
}
