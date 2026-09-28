use super::project_instructions::{ProjectInstructionFile, ProjectInstructionsSnapshot};
use super::project_resolution::ResolvedProject;
use super::ToolRuntime;
use crate::auth::AuthContext;
use crate::runner_http::{requested_by_from_auth, runner_access_from_auth, RunnerFeature};
use std::time::{Duration, Instant};
use webcodex_core::project_instructions::RunnerInstructionObservation;
use webcodex_core::runner_instruction::{
    RunnerInstructionRequest, RunnerInstructionSnapshotResponse,
    RUNNER_INSTRUCTION_RESPONSE_MAX_BYTES,
};

const RUNNER_INSTRUCTION_WAIT_SECS: u64 = 20;

impl ToolRuntime {
    pub(crate) async fn load_effective_coding_instructions(
        &self,
        project: &ResolvedProject,
        auth: Option<&AuthContext>,
    ) -> ProjectInstructionsSnapshot {
        let project_started_at = Instant::now();
        let local = self.load_coding_project_instructions(&project.config);
        let ((runner_files, runner_complete, observation), local) =
            if project.resolved_id.starts_with("agent:") {
                let runner = self.load_runner_instruction_files(&project.config.client_id, auth);
                futures_util::future::join(runner, local).await
            } else {
                ((Vec::new(), true, None), local.await)
            };
        let mut snapshot =
            ProjectInstructionsSnapshot::with_runner_files(runner_files, local, runner_complete);
        let scan = snapshot.scan.as_mut().expect("combined scan");
        scan.runner = observation;
        scan.project_started_at = Some(project_started_at);
        snapshot
    }

    pub(crate) async fn load_effective_session_instructions(
        &self,
        project: &ResolvedProject,
        auth: Option<&AuthContext>,
    ) -> ProjectInstructionsSnapshot {
        let project_started_at = Instant::now();
        let local = self.load_project_instructions(&project.config);
        let ((runner_files, runner_complete, observation), local) =
            if project.resolved_id.starts_with("agent:") {
                let runner = self.load_runner_instruction_files(&project.config.client_id, auth);
                futures_util::future::join(runner, local).await
            } else {
                ((Vec::new(), true, None), local.await)
            };
        let mut snapshot =
            ProjectInstructionsSnapshot::with_runner_files(runner_files, local, runner_complete);
        let scan = snapshot.scan.as_mut().expect("combined scan");
        scan.runner = observation;
        scan.project_started_at = Some(project_started_at);
        snapshot
    }

    async fn load_runner_instruction_files(
        &self,
        client_id: &str,
        auth: Option<&AuthContext>,
    ) -> (
        Vec<ProjectInstructionFile>,
        bool,
        Option<RunnerInstructionObservation>,
    ) {
        let started_at = Instant::now();
        let access = runner_access_from_auth(auth);
        let semantic = match self
            .runner_registry
            .get_runner_semantic_view_checked_for_auth(client_id, access.as_ref())
            .await
        {
            Ok(semantic) => semantic,
            Err(_) => return (Vec::new(), false, None),
        };
        let mut observation = RunnerInstructionObservation {
            instance_id: semantic.view.runner_instance_id.clone(),
            generation: advertised_generation(&semantic.view),
            started_at,
            instance_verified_at: semantic.observed_at,
        };
        if !semantic.supports(RunnerFeature::InstructionRuntime) {
            // An old Runner is a confirmed empty global scope.
            return (Vec::new(), true, Some(observation));
        }
        let runner_instance_id = observation.instance_id.clone();
        if runner_instance_id.is_empty() {
            return (Vec::new(), false, None);
        }
        let mut result = async {
            let (request_id, receiver) = match self
                .runner_registry
                .enqueue_runner_instruction(
                    client_id,
                    &runner_instance_id,
                    RunnerInstructionRequest::snapshot(),
                    access.as_ref(),
                    requested_by_from_auth(auth),
                )
                .await
            {
                Ok(request) => request,
                Err(_) => return (Vec::new(), false),
            };
            let response = match tokio::time::timeout(
                Duration::from_secs(RUNNER_INSTRUCTION_WAIT_SECS),
                receiver,
            )
            .await
            {
                Ok(Ok(response)) => response,
                Ok(Err(_)) | Err(_) => {
                    self.runner_registry
                        .cancel_request_dispatch_state(&request_id)
                        .await;
                    return (Vec::new(), false);
                }
            };
            if response.error.is_some() || response.exit_code != Some(0) {
                return (Vec::new(), false);
            }
            let Some(stdout) = response.stdout.as_deref() else {
                return (Vec::new(), false);
            };
            if stdout.len() > RUNNER_INSTRUCTION_RESPONSE_MAX_BYTES {
                return (Vec::new(), false);
            }
            let mut parsed = match serde_json::from_str::<RunnerInstructionSnapshotResponse>(stdout)
            {
                Ok(parsed) => parsed,
                Err(_) => return (Vec::new(), false),
            };
            if parsed.bind_visible_fingerprints().is_err() {
                return (Vec::new(), false);
            }
            if observation
                .generation
                .is_some_and(|generation| generation > parsed.generation)
            {
                return (Vec::new(), false);
            }
            observation.generation = Some(parsed.generation);
            (parsed.files, parsed.scan_complete)
        }
        .await;
        // Recheck even after a failed read: failure from a new config/process
        // must not retain the obsolete global scope.
        if let Ok(current) = self
            .runner_registry
            .get_runner_semantic_view_checked_for_auth(client_id, access.as_ref())
            .await
        {
            observation.instance_verified_at = current.observed_at;
            let generation = advertised_generation(&current.view);
            if current.view.runner_instance_id != runner_instance_id
                || generation.is_some_and(|current| {
                    observation
                        .generation
                        .is_none_or(|observed| current > observed)
                })
            {
                result = (Vec::new(), false);
                observation.instance_id = current.view.runner_instance_id;
                observation.generation = generation;
            }
        } else {
            result = (Vec::new(), false);
        }
        (result.0, result.1, Some(observation))
    }
}

fn advertised_generation(view: &webcodex_core::runner_protocol::RunnerView) -> Option<u64> {
    view.policy
        .as_ref()?
        .tool_providers
        .as_ref()
        .map(|providers| providers.config_reload.generation)
}
