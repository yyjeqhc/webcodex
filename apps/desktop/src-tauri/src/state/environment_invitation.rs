//! Desktop-only projections and invitation handoff; Environment remains authority.
use super::*;
use crate::models::EnvironmentSetupSnapshot;
use serde::{Deserialize, Serialize, Serializer};
use webcodex_environment::{EnvironmentStore, NativeEnvironment, Secret};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InvitationRequest {
    pub environment_id: String,
}

/// Only the explicit IPC response can serialize this secret. No Debug/Clone,
/// Desktop snapshot, preference or diagnostic representation includes it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvitationResponse {
    pub environment_id: String,
    #[serde(serialize_with = "serialize_invitation")]
    pairing_code: Secret,
}

fn serialize_invitation<S: Serializer>(code: &Secret, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(code.expose())
}

pub(super) fn setup_snapshot(snapshot: &DesktopStateSnapshot) -> Option<EnvironmentSetupSnapshot> {
    // A hand-configured/legacy Desktop stack has its own explicit migration
    // entry. Merely opening First Run must not reinterpret it as a new role.
    if snapshot.persistent_environment.is_none() && snapshot.topology.is_some() {
        return None;
    }
    let root = webcodex_environment::default_environment_dir().ok()?;
    if !root.is_dir() {
        return None;
    }
    let store = EnvironmentStore::open(root).ok()?;
    setup_snapshot_in(&store, snapshot.persistent_environment.as_deref()).ok()?
}

fn setup_snapshot_in(
    store: &EnvironmentStore,
    expected: Option<&str>,
) -> DesktopResult<Option<EnvironmentSetupSnapshot>> {
    let pending = store
        .load_journal()
        .map_err(environment::desktop_error)?
        .filter(|journal| !journal.environment.configured)
        .map(|journal| journal.environment);
    let saved = store
        .load_environment()
        .map_err(environment::desktop_error)?;
    let Some(record) = pending.or(saved) else {
        return Ok(None);
    };
    if expected.is_some_and(|id| id != record.environment_id) {
        return Ok(None);
    }
    Ok(Some(EnvironmentSetupSnapshot {
        runner_display_name: record.request.runner_display_name.clone(),
        environment_id: record.environment_id,
        mode: if record.request.local_server() {
            "create"
        } else {
            "join"
        }
        .into(),
        runner: record.request.local_runner(),
        server_url: record.request.server_url,
        project_path: record
            .request
            .project
            .map(|path| path.to_string_lossy().into_owned()),
        service_scope: record.request.service_scope,
        configured: record.configured,
    }))
}

fn changed() -> DesktopError {
    DesktopError::new(
        "environment_changed",
        "The saved environment changed",
        "Refresh the environment before creating a device invitation.",
    )
}

fn verify_target(
    state: &DesktopStateSnapshot,
    store: &EnvironmentStore,
    expected: &str,
) -> DesktopResult<()> {
    if state.persistent_environment.as_deref() != Some(expected) {
        return Err(changed());
    }
    let saved = store
        .load_environment()
        .map_err(environment::desktop_error)?;
    if saved.as_ref().map(|record| record.environment_id.as_str()) != Some(expected) {
        return Err(changed());
    }
    // No topology or role here grants invitation authority. Core verifies its
    // local Server requirement and the Server authorizes the pairing request.
    Ok(())
}

impl AppState {
    pub async fn create_environment_invitation(
        &self,
        request: InvitationRequest,
    ) -> DesktopResult<InvitationResponse> {
        if self.get_state().persistent_environment.as_deref()
            != Some(request.environment_id.as_str())
        {
            return Err(changed());
        }
        let store = environment::store()?;
        self.create_environment_invitation_in(&store, request).await
    }

    async fn create_environment_invitation_in(
        &self,
        store: &EnvironmentStore,
        request: InvitationRequest,
    ) -> DesktopResult<InvitationResponse> {
        if self.shutdown_signal.is_cancelled() {
            return Err(cancelled_error());
        }
        let operation = self
            .operations
            .admit(DesktopOperationKind::EnvironmentInvite, false)?;
        // This operation cannot launch or replace runtime processes. Do not
        // route a failed invitation through runtime cleanup/readiness policies.
        let result = async {
            if self.get_state().configuration_issue.is_some() {
                return Err(DesktopError::new(
                    "configuration_migration_failed",
                    "Configuration needs recovery",
                    "Recover the saved configuration before creating an invitation.",
                ));
            }
            verify_target(&self.get_state(), store, &request.environment_id)?;
            let code = NativeEnvironment::new()
                .map_err(environment::desktop_error)?
                .invite_for_environment(store, &request.environment_id)
                .await
                .map_err(environment::desktop_error)?;
            verify_target(&self.get_state(), store, &request.environment_id)?;
            if self.shutdown_signal.is_cancelled() {
                return Err(cancelled_error());
            }
            Ok(InvitationResponse {
                environment_id: request.environment_id,
                pairing_code: code,
            })
        }
        .await;
        self.operations.finish(&operation.id, &result);
        result
    }
}

#[cfg(test)]
mod tests;
