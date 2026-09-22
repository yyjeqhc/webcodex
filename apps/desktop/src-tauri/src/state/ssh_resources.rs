use super::*;
use crate::ssh_resources::{
    HttpGateway, Mutation, SshMutationResult, SshRegisterRequest, SshRemoveRequest,
    SshResourcesSnapshot,
};

impl AppState {
    pub async fn ssh_resource_list(&self) -> DesktopResult<SshResourcesSnapshot> {
        if self.shutdown_signal.is_cancelled() || self.operations.current().is_some() {
            return Err(crate::workspace::unavailable());
        }
        let (runtime, identity) = {
            let slot = self.core.lock().await;
            let core = slot.as_ref().ok_or_else(crate::workspace::unavailable)?;
            (
                core.config
                    .runtime
                    .clone()
                    .ok_or_else(crate::workspace::unavailable)?,
                identity_from_config(&core.config).ok_or_else(crate::workspace::unavailable)?,
            )
        };
        // Serialize observations/mutations, but never hold the Desktop state
        // mutex across network I/O. A restart/re-pair invalidates late reads.
        let mut resources = self.ssh_resources.lock().await;
        let snapshot = resources.list(&runtime, &HttpGateway).await;
        let slot = self.core.lock().await;
        if self.shutdown_signal.is_cancelled()
            || self.operations.current().is_some()
            || slot
                .as_ref()
                .and_then(|core| identity_from_config(&core.config))
                .as_ref()
                != Some(&identity)
        {
            resources.invalidate();
            return Err(crate::workspace::unavailable());
        }
        Ok(snapshot)
    }

    pub async fn ssh_resource_register(
        &self,
        request: SshRegisterRequest,
    ) -> DesktopResult<SshMutationResult> {
        self.mutate_ssh_resource(
            request.expected,
            request.observation_id,
            Mutation::Register {
                name: request.name,
                target: request.target,
                default_cwd: request.default_cwd,
            },
        )
        .await
    }

    pub async fn ssh_resource_remove(
        &self,
        request: SshRemoveRequest,
    ) -> DesktopResult<SshMutationResult> {
        self.mutate_ssh_resource(
            request.expected,
            request.observation_id,
            Mutation::Remove { name: request.name },
        )
        .await
    }

    async fn mutate_ssh_resource(
        &self,
        expected: crate::webcodex::settings::SettingsTarget,
        observation_id: String,
        mutation: Mutation,
    ) -> DesktopResult<SshMutationResult> {
        let (operation, cancellation, core, baseline) = self
            .begin_operation(DesktopOperationKind::RunnerSettingsUpdate, false)
            .await?;
        let result = async {
            let mut resources = self.ssh_resources.lock().await;
            let runtime = core
                .config
                .runtime
                .as_ref()
                .ok_or_else(crate::workspace::unavailable)?;
            if let Err(error) = crate::webcodex::settings::verify_target(runtime, &expected) {
                resources.invalidate();
                return Err(error);
            }
            Ok(resources
                .mutate(runtime, &observation_id, mutation, &HttpGateway)
                .await)
        }
        .await;
        let finished = result
            .as_ref()
            .map(|_| core.snapshot.clone())
            .map_err(Clone::clone);
        self.finish_operation(operation, cancellation, core, baseline, finished)
            .await?;
        result
    }
}
