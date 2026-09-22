#[cfg(test)]
mod tests;

use super::*;

impl AppState {
    pub async fn authorize_runner_capabilities(
        &self,
        request: crate::runner_capability_grant::GrantRequest,
    ) -> DesktopResult<crate::runner_capability_grant::AuthorizationSnapshot> {
        if !request.confirmed {
            return Err(crate::runner_capability_grant::unavailable());
        }
        let (operation, cancellation, core, baseline) = self
            .begin_operation(DesktopOperationKind::RunnerSettingsUpdate, false)
            .await?;
        let result = async {
            let runtime = core
                .config
                .runtime
                .as_ref()
                .ok_or_else(crate::runner_capability_grant::unavailable)?;
            crate::webcodex::settings::verify_target(runtime, &request.expected)?;
            if !crate::runner_capability_grant::can_authorize(runtime) {
                return Err(crate::runner_capability_grant::unavailable());
            }
            let env_file = runtime
                .server_env_file
                .clone()
                .ok_or_else(crate::runner_capability_grant::unavailable)?;
            let expected_url = runtime.server_url.clone();
            let token = tokio::task::spawn_blocking(move || {
                // Bind address and operator credential to one bounded snapshot;
                // two independent reads could mix concurrently replaced files.
                let content = read_desktop_server_env(&env_file)?;
                if format!("http://{}", desktop_server_listen_from_content(&content)?)
                    != expected_url
                {
                    return Err(crate::runner_capability_grant::unavailable());
                }
                crate::runner_capability_grant::operator_token(&content)
            })
            .await
            .map_err(|_| crate::runner_capability_grant::unavailable())??;
            self.ssh_resources.lock().await.invalidate();
            let granted = crate::runner_capability_grant::grant(runtime, token).await;
            let observed = crate::runner_capability_grant::observe(runtime).await;
            granted?;
            observed
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

    pub async fn runner_capability_authorization(
        &self,
        expected: crate::webcodex::settings::SettingsTarget,
    ) -> DesktopResult<crate::runner_capability_grant::AuthorizationSnapshot> {
        let runtime = {
            let slot = self.core.lock().await;
            if self.shutdown_signal.is_cancelled() || self.operations.current().is_some() {
                return Err(crate::runner_capability_grant::unavailable());
            }
            let runtime = slot
                .as_ref()
                .and_then(|core| core.config.runtime.clone())
                .ok_or_else(crate::runner_capability_grant::unavailable)?;
            crate::webcodex::settings::verify_target(&runtime, &expected)?;
            runtime
        };
        let observed = crate::runner_capability_grant::observe(&runtime).await?;
        let slot = self.core.lock().await;
        let current = slot
            .as_ref()
            .and_then(|core| core.config.runtime.as_ref())
            .ok_or_else(crate::runner_capability_grant::unavailable)?;
        if self.shutdown_signal.is_cancelled()
            || self.operations.current().is_some()
            || current.user_token_file != runtime.user_token_file
            || current.server_env_file != runtime.server_env_file
        {
            return Err(crate::runner_capability_grant::unavailable());
        }
        crate::webcodex::settings::verify_target(current, &expected)?;
        Ok(observed)
    }
}
