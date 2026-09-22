use super::*;
use crate::mcp_providers::{McpProviderRequest, McpProviderStore};

impl AppState {
    pub async fn save_mcp_provider(
        &self,
        request: McpProviderRequest,
    ) -> DesktopResult<DesktopStateSnapshot> {
        self.mutate_mcp_providers(move |store| store.update(request).map(|_| ()))
            .await
    }
    pub async fn remove_mcp_provider(
        &self,
        id: String,
        expected_revision: u64,
    ) -> DesktopResult<DesktopStateSnapshot> {
        self.mutate_mcp_providers(move |store| store.remove(&id, expected_revision))
            .await
    }
    async fn mutate_mcp_providers(
        &self,
        change: impl FnOnce(&mut McpProviderStore) -> DesktopResult<()> + Send + 'static,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::RunnerSettingsUpdate, false)
            .await?;
        let result = async {
            cancellation.check()?;
            let mut candidate = core.mcp_providers.clone();
            candidate = tokio::task::spawn_blocking(move || {
                change(&mut candidate)?;
                Ok::<_, DesktopError>(candidate)
            })
            .await
            .map_err(|_| crate::mcp_providers::invalid())??;
            core.mcp_providers = candidate;
            // Saving stages desired state. Apply at the explicit Runner restart;
            // never partially hot-reload an env reference missing from the old process.
            core.get_state().await
        }
        .await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }
}

impl DesktopCore {
    pub(super) async fn prepare_runner_command(
        &mut self,
        identity: &ProjectRuntimeIdentity,
    ) -> DesktopResult<std::process::Command> {
        let store = self.mcp_providers.clone();
        let coding_agents = self.coding_agents.clone();
        if !store.managed_ids().is_empty()
            || store.snapshot(None).config_error
            || coding_agents.needs_reconciliation()
        {
            let suffix = format!(":{}", identity.project_id);
            let client_id = identity
                .runtime_project_id
                .strip_prefix("agent:")
                .and_then(|id| id.strip_suffix(&suffix))
                .filter(|id| !id.is_empty())
                .ok_or_else(crate::mcp_providers::invalid)?
                .to_string();
            let runtime = StoredRuntime {
                server_url: identity.server_url.clone(),
                server_env_file: None,
                runner_config: Some(identity.runner_config.clone()),
                user_token_file: Some(identity.user_token_file.clone()),
                runner_client_id: Some(client_id),
                project_id: Some(identity.project_id.clone()),
                runtime_project_id: Some(identity.runtime_project_id.clone()),
            };
            tokio::task::spawn_blocking(move || {
                // Detect ACP ownership conflicts before changing any capability.
                crate::webcodex::settings::reconcile_acp(&runtime, &coding_agents, true)?;
                crate::webcodex::settings::reconcile_mcp(&runtime, &store)?;
                crate::webcodex::settings::reconcile_acp(&runtime, &coding_agents, false)
            })
            .await
            .map_err(|_| crate::mcp_providers::invalid())??;
        }
        let mut command = self.adapter.local_runner_command(&identity.runner_config)?;
        self.mcp_providers.apply_to_command(&mut command)?;
        Ok(command)
    }

    pub(super) async fn spawn_configured_runner(
        &mut self,
        identity: &ProjectRuntimeIdentity,
        cancellation: &CancellationContext,
    ) -> DesktopResult<()> {
        let command = self.prepare_runner_command(identity).await?;
        self.spawn_owned(ProcessKey::LocalRunner, command, false, cancellation)
            .await?;
        self.mcp_applied_revision = Some(self.mcp_providers.revision());
        self.coding_agents_applied_revision = Some(self.coding_agents.revision());
        Ok(())
    }
}

#[cfg(test)]
mod tests;
