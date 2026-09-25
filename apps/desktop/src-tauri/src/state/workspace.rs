use super::*;

impl AppState {
    pub async fn workspace_query(
        &self,
        request: crate::workspace::WorkspaceRequest,
    ) -> DesktopResult<serde_json::Value> {
        if self.shutdown_signal.is_cancelled() || self.operations.current().is_some() {
            return Err(crate::workspace::unavailable());
        }
        let generation = self.operations.generation();
        let overview = matches!(&request, crate::workspace::WorkspaceRequest::Overview {});
        let runtime = {
            let slot = self.core.lock().await;
            let core = slot.as_ref().ok_or_else(crate::workspace::unavailable)?;
            let runtime = core
                .config
                .runtime
                .clone()
                .ok_or_else(crate::workspace::unavailable)?;
            runtime
        };
        // Never keep the state mutex across an HTTP request.
        let value = crate::workspace::query(&runtime, request).await?;
        let mut slot = self.core.lock().await;
        let core = slot.as_mut().ok_or_else(crate::workspace::unavailable)?;
        if self.shutdown_signal.is_cancelled()
            || self.operations.current().is_some()
            || self.operations.generation() != generation
            || core.config.runtime.as_ref() != Some(&runtime)
        {
            return Err(crate::workspace::unavailable());
        }
        if overview {
            core.reconcile_inventory(&value).await;
        }
        Ok(value)
    }
}

impl AppState {
    pub async fn prepare_project_unregister(
        &self,
        project: &str,
    ) -> DesktopResult<crate::project_inventory::UnregisterObservation> {
        if self.operations.current().is_some() || self.shutdown_signal.is_cancelled() {
            return Err(crate::workspace::unavailable());
        }
        let runtime = {
            let slot = self.core.lock().await;
            slot.as_ref()
                .and_then(|core| core.config.runtime.clone())
                .ok_or_else(crate::workspace::unavailable)?
        };
        let observation = crate::project_inventory::observe(&runtime, project).await?;
        let slot = self.core.lock().await;
        if self.operations.current().is_some()
            || self.shutdown_signal.is_cancelled()
            || slot.as_ref().and_then(|core| core.config.runtime.as_ref()) != Some(&runtime)
        {
            return Err(crate::workspace::unavailable());
        }
        Ok(observation)
    }

    pub async fn unregister_project(
        &self,
        request: crate::project_inventory::UnregisterRequest,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::ProjectUnregister, false)
            .await?;
        let result = async {
            let runtime = core
                .config
                .runtime
                .clone()
                .ok_or_else(crate::workspace::unavailable)?;
            let path = crate::project_inventory::unregister(&runtime, &request).await?;
            crate::project_inventory::forget(&mut core.config, &request.project, &path);
            // Remote removal is already confirmed. Never roll it back or redispatch on a local persistence failure.
            core.clear_absent_project_observation();
            core.inventory_persistence_pending = true;
            if core.save_config().await.is_ok() {
                core.inventory_persistence_pending = false;
            } else {
                core.activity.push(
                    ActivityEventKind::StateRecovered,
                    "desktop",
                    ActivityLevel::Warning,
                    "Project registration removed; Desktop will retry saving local history",
                );
            }
            core.get_state().await
        }
        .await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }
}

impl DesktopCore {
    pub(super) async fn reconcile_inventory(&mut self, value: &Value) {
        let changed = crate::project_inventory::reconcile(&mut self.config, value);
        if changed || self.inventory_persistence_pending {
            // Keep confirmed inventory in memory even when disk is unavailable.
            // A later observation retries this local write, never unregister.
            self.inventory_persistence_pending = true;
            if self.save_config().await.is_ok() {
                self.inventory_persistence_pending = false;
            }
            self.clear_absent_project_observation();
            self.publish_snapshot();
        }
    }

    fn clear_absent_project_observation(&mut self) {
        if self.config.project.is_none() {
            self.snapshot.project = None;
            self.snapshot.chatgpt_activity = None;
            self.snapshot.readiness = aggregate_readiness(
                self.snapshot.readiness.server.clone(),
                self.snapshot.readiness.runner.clone(),
                self.snapshot.readiness.exposure.clone(),
                ProjectReadiness::None,
            );
        }
    }
}
