//! Exhaustive tool-family routing after the single governance chain.

use super::*;

impl AuthorizedDispatch<'_, '_> {
    pub(super) async fn execute(
        self,
        runtime: &ToolRuntime,
        correlation: &mut crate::tool_runtime::window_activity::ToolCallCorrelation,
        bootstrap_context: &mut Option<crate::tool_runtime::coding_task::BootstrapContext>,
    ) -> ToolResult {
        let Self {
            call,
            context,
            ssh_resource,
            project_resolution,
            structured_handoff_max_secs,
        } = self;
        let ToolCallContext {
            auth,
            window,
            transport,
            ..
        } = context.caller;
        let transport = transport.into();
        let protocol_capabilities = context.capabilities;
        let recorder = &context.recorder;
        let validation_assertion_name = recorder.expectation.assertion_name.as_deref();
        let _logical_invocation_id = recorder.logical_invocation_id.as_deref();
        let trusted_recording_session_id = recorder
            .recording_session_authorized
            .then_some(recorder.recording_session_id.as_deref())
            .flatten();
        let trusted_recording_session_project = recorder
            .recording_session_authorized
            .then_some(recorder.recording_session_project.as_deref())
            .flatten();
        match call {
            ToolCall::OpenWebcodexWorkbench {
                client_id,
                project,
                session_id,
            } => {
                runtime
                    .open_webcodex_workbench(project, session_id, client_id, auth)
                    .await
            }
            ToolCall::SearchWebcodexResources {
                client_id,
                kind,
                query,
                project,
                session_id,
                offset,
                limit,
            } => {
                runtime
                    .search_webcodex_resources(
                        kind, query, project, session_id, offset, limit, client_id, auth,
                    )
                    .await
            }
            ToolCall::ReadWebcodexResource { uri } => {
                runtime.read_webcodex_resource(&uri, auth).await
            }

            call @ (ToolCall::ListTools { .. }
            | ToolCall::ListRunners { .. }
            | ToolCall::RuntimeStatus { .. }
            | ToolCall::ReadToolTrace { .. }
            | ToolCall::ToolManifest { .. }) => {
                runtime
                    .dispatch_discovery_tool(
                        call,
                        auth,
                        protocol_capabilities,
                        context.discovery_tools.as_ref(),
                    )
                    .await
            }

            ToolCall::CurrentWindowActivity {
                limit,
                include_nonmeaningful,
            } => {
                runtime
                    .current_window_activity(window, auth, limit, include_nonmeaningful)
                    .await
            }

            call @ (ToolCall::RunnerConfigCheck { .. } | ToolCall::RunnerConfigReload { .. }) => {
                runtime.dispatch_runner_config_tool(call, auth).await
            }

            ToolCall::PluginTool(_) => {
                unreachable!(
                    "plugin_tool is dispatched before generic static ToolDefinition policy"
                )
            }

            ToolCall::SshResource(_) => {
                unreachable!(
                    "manage_ssh_resource is dispatched before generic static ToolDefinition policy"
                )
            }

            ToolCall::PostPeerMessage {
                peer_id,
                kind,
                message,
                tags,
                priority,
                requires_ack,
                delivery_key,
            } => runtime.post_peer_message_tool(
                peer_id,
                kind,
                message,
                tags,
                priority,
                requires_ack,
                delivery_key,
                auth,
                window,
                trusted_recording_session_id,
                trusted_recording_session_project,
            ),
            call @ (ToolCall::StartSession { .. }
            | ToolCall::ListSessions { .. }
            | ToolCall::SessionSummary { .. }
            | ToolCall::UpdateSessionContext { .. }
            | ToolCall::CloseSession { .. }
            | ToolCall::ValidationSummary { .. }
            | ToolCall::RecordExternalObservation { .. }
            | ToolCall::ListExternalObservations { .. }
            | ToolCall::PostSessionMessage { .. }
            | ToolCall::ListSessionMessages { .. }
            | ToolCall::GetSessionAssignment { .. }
            | ToolCall::ObserveSessionMessages { .. }
            | ToolCall::ResolveSessionMessage { .. }
            | ToolCall::CompleteSessionMessage { .. }
            | ToolCall::SessionDiscussionSummary { .. }
            | ToolCall::WorkOnProject { .. }
            | ToolCall::FinishCodingTask { .. }
            | ToolCall::PresentDocx { .. }
            | ToolCall::PresentWorkResult { .. }
            | ToolCall::PresentPdf { .. }
            | ToolCall::ReadAppArtifactChunk { .. }
            | ToolCall::WorkResultState { .. }
            | ToolCall::WorkResultActivityDetail { .. }
            | ToolCall::WorkResultSendMessage { .. }
            | ToolCall::ChangesFileDiff { .. }
            | ToolCall::SessionHandoffSummary { .. }
            | ToolCall::SessionHandoffState { .. }
            | ToolCall::ResolveWorkspace { .. }
            | ToolCall::UnregisterProjects { .. }
            | ToolCall::ListProjects { .. }
            | ToolCall::RegisterProject { .. }
            | ToolCall::UnregisterProject { .. }
            | ToolCall::CreateProject { .. }) => {
                runtime
                    .dispatch_workflows_authorized(
                        call,
                        auth,
                        transport,
                        window,
                        trusted_recording_session_id,
                        trusted_recording_session_project,
                        correlation,
                        bootstrap_context,
                    )
                    .await
            }

            #[cfg(feature = "workspace-checkpoints")]
            call @ (ToolCall::WorkspaceCheckpointCreate { .. }
            | ToolCall::WorkspaceCheckpointList { .. }
            | ToolCall::WorkspaceCheckpointShow { .. }
            | ToolCall::WorkspaceCheckpointRestore { .. }
            | ToolCall::WorkspaceCheckpointDelete { .. }) => {
                runtime.dispatch_workspace_checkpoint_tool(call).await
            }
            #[cfg(feature = "experimental-code-mode")]
            call @ (ToolCall::CodeModeExec { .. }
            | ToolCall::CodeModeExecEffectful { .. }
            | ToolCall::CodeModeExecMutating { .. }) => {
                runtime
                    .dispatch_code_mode_authorized(
                        call,
                        auth,
                        transport,
                        project_resolution,
                        _logical_invocation_id,
                        correlation,
                    )
                    .await
            }

            call @ (ToolCall::CodingAgentStart { .. }
            | ToolCall::CodingAgentObserve { .. }
            | ToolCall::CodingAgentCancel { .. }
            | ToolCall::RunProcess { .. }
            | ToolCall::JobWriteInput { .. }
            | ToolCall::RunDetachedProcess { .. }
            | ToolCall::RunScript { .. }
            | ToolCall::RunShell { .. }
            | ToolCall::ProjectBuild { .. }
            | ToolCall::ProjectFormat { .. }
            | ToolCall::OpenSessionShell { .. }
            | ToolCall::SessionShellExec { .. }
            | ToolCall::SessionShellStatus { .. }
            | ToolCall::CloseSessionShell { .. }
            | ToolCall::ApplyPatch { .. }
            | ToolCall::ApplyUnifiedDiff { .. }
            | ToolCall::RunSkillResource { .. }) => {
                runtime
                    .dispatch_execution_authorized(
                        call,
                        auth,
                        ssh_resource,
                        validation_assertion_name,
                        project_resolution,
                        structured_handoff_max_secs,
                    )
                    .await
            }

            ToolCall::BrowserObserve(_) | ToolCall::BrowserAct(_) => ToolResult::err(
                "Browser gateways must pass action-sensitive specialized governance".to_string(),
            ),
            ToolCall::ComputerObserve(_) | ToolCall::ComputerControl(_) => ToolResult::err(
                "Computer gateways must pass action-sensitive specialized governance".to_string(),
            ),
            call @ ToolCall::ComputerSaveSnapshot { .. } => {
                runtime.dispatch_computer_tool(call, auth).await
            }
            call @ (ToolCall::SkillLoad { .. }
            | ToolCall::SkillList { .. }
            | ToolCall::SkillReadFile { .. }
            | ToolCall::SkillVersions { .. }
            | ToolCall::SkillInstall { .. }
            | ToolCall::SkillActivate { .. }
            | ToolCall::SkillRemoveRevision { .. }) => {
                runtime
                    .dispatch_skills_authorized(call, auth, project_resolution)
                    .await
            }

            call @ (ToolCall::PrepareGoalWorkflow { .. }
            | ToolCall::CreateGoal { .. }
            | ToolCall::CheckpointGoal { .. }
            | ToolCall::ReviseGoalPlan { .. }
            | ToolCall::GetGoal { .. }
            | ToolCall::PresentGoalPlan { .. }
            | ToolCall::GoalPlanSync { .. }
            | ToolCall::ListGoals { .. }
            | ToolCall::UpdateGoal { .. }
            | ToolCall::AssociateGoalAgentTask { .. }
            | ToolCall::AssociateGoalWorkflowSession { .. }) => {
                runtime.dispatch_goals_authorized(call, auth, window).await
            }

            call @ (ToolCall::WaitForAgentEvents { .. }
            | ToolCall::ReadAgentWait { .. }
            | ToolCall::CancelAgentWait { .. }
            | ToolCall::AgentWaitState { .. }
            | ToolCall::CreateAgentTask { .. }
            | ToolCall::ListAgentTasks { .. }
            | ToolCall::ReadAgentTask { .. }
            | ToolCall::AssignAgentTask { .. }
            | ToolCall::StartAgentTaskAttempt { .. }
            | ToolCall::StartAgentTaskEndpointContinuation { .. }
            | ToolCall::StartAgentTaskCodingRun { .. }
            | ToolCall::ReconcileAgentTaskCodingRun { .. }
            | ToolCall::HeartbeatAgentTaskAttempt { .. }
            | ToolCall::CompleteAgentTaskAttempt { .. }) => {
                runtime.dispatch_agent_work_authorized(call, auth).await
            }

            call @ (ToolCall::CreateAgentIdentity(_)
            | ToolCall::ListAgentIdentities(_)
            | ToolCall::UpdateAgentIdentity(_)
            | ToolCall::RotateAgentContinuationEndpoint(_)
            | ToolCall::PresentAgentContinuation { .. }
            | ToolCall::AgentContinuationBind { .. }
            | ToolCall::AgentContinuationRecoverEndpoint { .. }
            | ToolCall::AgentContinuationState { .. }
            | ToolCall::AgentContinuationWakeAcquire { .. }
            | ToolCall::AgentContinuationWakePrepare { .. }
            | ToolCall::AgentContinuationWakeFinish { .. }
            | ToolCall::AgentContinuationUnbind { .. }
            | ToolCall::DetachAgentEndpoint(_)
            | ToolCall::CreateConversation(_)
            | ToolCall::ListConversations(_)
            | ToolCall::ReadConversation(_)
            | ToolCall::PostConversationMessage(_)
            | ToolCall::ListAgentInbox(_)
            | ToolCall::ConsumeAgentDeliveries(_)
            | ToolCall::BootstrapAgentConversation { .. }
            | ToolCall::ConsumeAgentWake { .. }) => {
                runtime.dispatch_agents_authorized(call, auth, window).await
            }

            call @ (ToolCall::PresentJobTerminalContinuation { .. }
            | ToolCall::JobTerminalContinuationBind { .. }
            | ToolCall::JobTerminalContinuationState { .. }
            | ToolCall::JobTerminalContinuationPrepare { .. }
            | ToolCall::JobTerminalContinuationFinish { .. }
            | ToolCall::JobTerminalContinuationUnbind { .. }) => {
                runtime
                    .dispatch_job_continuation_authorized(call, auth, window)
                    .await
            }

            call @ (ToolCall::MemorySearch { .. }
            | ToolCall::MemoryRead { .. }
            | ToolCall::MemorySet { .. }
            | ToolCall::MemoryDelete { .. }
            | ToolCall::MemoryScopeList { .. }
            | ToolCall::MemoryScopePurge { .. }) => {
                runtime
                    .dispatch_memory_authorized(call, auth, project_resolution)
                    .await
            }

            call @ ToolCall::ImportConversationFilesToProject { .. } => {
                runtime
                    .dispatch_conversation_import_tool(call, auth, transport)
                    .await
            }

            call @ (ToolCall::DeleteProjectFiles { .. }
            | ToolCall::ReadFiles { .. }
            | ToolCall::ListProjectFiles { .. }
            | ToolCall::ListProjectTrackedFiles { .. }
            | ToolCall::ProjectOverview { .. }
            | ToolCall::SearchProjectTexts { .. }
            | ToolCall::SearchAndRead { .. }
            | ToolCall::WriteProjectFile { .. }
            | ToolCall::SaveProjectArtifact { .. }
            | ToolCall::TransferProjectArtifact { .. }
            | ToolCall::AcceptArtifactHandoff { .. }
            | ToolCall::PresentSpreadsheet { .. }
            | ToolCall::ProjectArtifact { .. }
            | ToolCall::ReadProjectArtifactMetadata { .. }
            | ToolCall::ReadProjectArtifact { .. }
            | ToolCall::ArtifactUploadBegin { .. }
            | ToolCall::ArtifactUploadChunk { .. }
            | ToolCall::ArtifactUploadFinish { .. }
            | ToolCall::ArtifactUploadAbort { .. }
            | ToolCall::ApplyTextEdits { .. }) => {
                runtime
                    .dispatch_file_tool(call, transport, project_resolution, auth)
                    .await
            }

            call @ (ToolCall::GitRestorePaths { .. }
            | ToolCall::DiscardUntracked { .. }
            | ToolCall::GitCommitPaths { .. }
            | ToolCall::GitStatus { .. }
            | ToolCall::GitDiffHunks { .. }
            | ToolCall::GitReviewSummary { .. }
            | ToolCall::ReviewChanges { .. }
            | ToolCall::GitLog { .. }
            | ToolCall::ShowChanges { .. }) => runtime.dispatch_git_tool(call, auth).await,

            call @ (ToolCall::CargoFmt { .. }
            | ToolCall::CargoCheck { .. }
            | ToolCall::CargoTest { .. }
            | ToolCall::ProjectValidate { .. }
            | ToolCall::GoTest { .. }) => {
                runtime
                    .dispatch_cargo_tool(call, ssh_resource, auth, structured_handoff_max_secs)
                    .await
            }

            call @ (ToolCall::RunJob { .. }
            | ToolCall::StopJob { .. }
            | ToolCall::ObserveJobs { .. }
            | ToolCall::WaitForJobReadiness { .. }
            | ToolCall::WaitForJobTerminal { .. }
            | ToolCall::ListJobs { .. }
            | ToolCall::JobTail { .. }) => {
                runtime
                    .dispatch_job_tool(call, auth, ssh_resource, correlation)
                    .await
            }

            call @ ToolCall::WorkspaceHygieneCheck { .. } => {
                runtime.dispatch_hygiene_tool(call).await
            }

            call @ (ToolCall::LspStatus { .. }
            | ToolCall::DocumentSymbols { .. }
            | ToolCall::DocumentDiagnostics { .. }
            | ToolCall::Hover { .. }
            | ToolCall::WorkspaceSymbols { .. }
            | ToolCall::GotoDefinition { .. }
            | ToolCall::FindReferences { .. }
            | ToolCall::CallHierarchy { .. }) => runtime.dispatch_lsp_tool(call).await,
        }
    }
}
