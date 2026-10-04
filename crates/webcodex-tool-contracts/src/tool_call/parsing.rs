fn validate_coding_project_source_shape(tool_name: &str, arguments: &Value) -> Result<(), String> {
    let Some(arguments) = arguments.as_object() else {
        return Ok(());
    };
    let project = arguments.contains_key("project");
    let client_id = arguments.contains_key("client_id");
    let path = arguments.contains_key("path");
    for field in ["project", "client_id", "path"] {
        if arguments
            .get(field)
            .and_then(Value::as_str)
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(format!(
                "invalid arguments for tool '{tool_name}': {field} must not be empty"
            ));
        }
    }
    if let Some(path) = arguments.get("path").and_then(Value::as_str) {
        if let Err(error) = validate_project_op_path(path) {
            return Err(format!("invalid arguments for tool '{tool_name}': {error}"));
        }
    }
    if project {
        let mut conflicts = Vec::new();
        if client_id {
            conflicts.push("client_id");
        }
        if path {
            conflicts.push("path");
        }
        return if conflicts.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "invalid arguments for tool '{tool_name}': conflicting fields project and {}",
                conflicts.join(", ")
            ))
        };
    }
    if path {
        return if client_id {
            Ok(())
        } else {
            Err(format!(
                "invalid arguments for tool '{tool_name}': missing client_id required with path"
            ))
        };
    }
    if client_id {
        return Err(format!(
            "invalid arguments for tool '{tool_name}': missing path required with client_id"
        ));
    }
    // A Session is an exact continuation selector, not a worktree source or
    // permission to infer a current Project. The runtime authorizes its binding.
    if arguments
        .get("session_id")
        .and_then(Value::as_str)
        .is_some_and(|session_id| !session_id.trim().is_empty())
        && arguments
            .get("mode")
            .and_then(Value::as_str)
            .unwrap_or("checkout")
            == "checkout"
        && !arguments
            .get("base_ref")
            .is_some_and(|value| !value.is_null())
    {
        return Ok(());
    }
    Err(format!(
        "invalid arguments for tool '{tool_name}': missing project source; expected project or client_id + path, or session_id for exact checkout resume"
    ))
}

fn validate_project_artifact_arguments(name: &str, arguments: &Value) -> Result<(), String> {
    if name != "inspect_project_artifact" {
        return Ok(());
    }
    let Some(object) = arguments.as_object() else {
        return Ok(()); // serde reports the canonical object-shape error below.
    };
    let action = object
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "invalid arguments for tool 'inspect_project_artifact': action is required".to_string()
        })?;
    let action_fields: &[&str] = match action {
        "metadata" => &["allow_missing"],
        "inspect" => &["offset", "length", "expected_sha256"],
        "image" | "export" => &[],
        _ => {
            return Err(format!(
                "invalid arguments for tool 'inspect_project_artifact': unsupported action '{action}'; expected metadata, inspect, image, or export"
            ))
        }
    };
    let common = ["project", "path", "action", "session_id"];
    let invalid = object
        .keys()
        .map(String::as_str)
        .filter(|key| !common.contains(key) && !action_fields.contains(key))
        .collect::<Vec<_>>();
    if invalid.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "invalid arguments for tool 'inspect_project_artifact': action={action} does not accept field(s) {}",
            invalid.join(", ")
        ))
    }
}

fn validate_read_project_artifact_expected_sha256(
    name: &str,
    arguments: &Value,
) -> Result<(), String> {
    if !matches!(
        name,
        "read_project_artifact_chunk" | "inspect_project_artifact"
    ) {
        return Ok(());
    }
    let Some(object) = arguments.as_object() else {
        return Ok(());
    };
    let Some(value) = object.get("expected_sha256") else {
        return Ok(());
    };
    let valid = value.as_str().is_some_and(|value| {
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    });
    if valid {
        Ok(())
    } else {
        Err(format!(
            "invalid arguments for tool '{name}': expected_sha256 must be exactly 64 lowercase hexadecimal characters"
        ))
    }
}

fn validate_structured_validation_sync_wait(name: &str, arguments: &Value) -> Result<(), String> {
    if !matches!(name, "cargo_fmt" | "cargo_check" | "cargo_test" | "go_test") {
        return Ok(());
    }
    let Some(object) = arguments.as_object() else {
        return Ok(());
    };
    let Some(sync_wait_value) = object.get("sync_wait_secs") else {
        return Ok(());
    };
    if sync_wait_value.is_null() {
        return Ok(());
    }
    let Some(sync_wait_secs) = sync_wait_value.as_u64() else {
        return Ok(()); // serde reports the canonical type error below.
    };
    if sync_wait_secs == 0 {
        return Err(format!(
            "invalid arguments for tool '{name}': sync_wait_secs must be at least 1"
        ));
    }
    Ok(())
}

fn canonicalize_cargo_check_packages(name: &str, arguments: &mut Value) -> Result<(), String> {
    if name != "cargo_check" {
        return Ok(());
    }
    let Some(object) = arguments.as_object_mut() else {
        return Ok(());
    };
    let package_present = object.get("package").is_some_and(|value| !value.is_null());
    let packages_present = object.get("packages").is_some_and(|value| !value.is_null());
    if package_present && packages_present {
        return Err(
            "invalid arguments for tool 'cargo_check': package and packages are mutually exclusive"
                .to_string(),
        );
    }

    let package = object.get("package").and_then(Value::as_str);
    let packages = match object.get("packages") {
        Some(Value::Array(values)) => values
            .iter()
            .map(Value::as_str)
            .collect::<Option<Vec<_>>>()
            .map(|values| values.into_iter().map(str::to_string).collect::<Vec<_>>()),
        Some(Value::Null) | None => None,
        Some(_) => return Ok(()), // serde reports the canonical type error.
    };
    if package_present && package.is_none() || packages_present && packages.is_none() {
        return Ok(()); // serde reports the canonical item/type error.
    }
    let normalized = normalize_cargo_packages(package, packages.as_deref())
        .map_err(|reason| format!("invalid arguments for tool 'cargo_check': {reason}"))?;
    object.remove("package");
    object.remove("packages");
    if let Some(packages) = normalized {
        object.insert("packages".to_string(), serde_json::json!(packages));
    }
    Ok(())
}

/// Only explicitly documented, lossless model-input spellings belong here.
/// Business ToolCall variants and Runner payloads retain `args` alone.
fn canonicalize_process_argv_alias(name: &str, arguments: &mut Value) -> Result<bool, String> {
    if !matches!(name, "run_process" | "run_detached_process") {
        return Ok(false);
    }
    let Some(object) = arguments.as_object_mut() else {
        return Ok(false);
    };
    let Some(alias) = object.remove("argv") else {
        return Ok(false);
    };
    if let Some(canonical) = object.get("args") {
        if canonical != &alias {
            return Err("ambiguous input alias: args and argv differ".to_string());
        }
    } else {
        object.insert("args".to_string(), alias);
    }
    Ok(true)
}

fn validate_run_shell_login(name: &str, arguments: &Value) -> Result<(), String> {
    if name == "run_shell"
        && arguments.get("login").and_then(Value::as_bool) == Some(true)
        && arguments.get("shell").and_then(Value::as_str) != Some("bash")
    {
        return Err("run_shell login=true requires shell=bash".to_string());
    }
    Ok(())
}

impl ToolCall {
    pub fn from_tool_name(name: &str, arguments: Value) -> Result<Self, String> {
        Self::from_tool_name_with_normalization(name, arguments).map(|(call, _)| call)
    }

    /// Returns a stable code only when an explicit ergonomic input alias was
    /// normalized to avoid a mechanical retry. This is not API compatibility.
    pub fn from_tool_name_with_normalization(
        name: &str,
        arguments: Value,
    ) -> Result<(Self, Option<crate::ToolInputNormalizationCode>), String> {
        validate_model_facing_assertion_name(name, &arguments)?;
        validate_model_facing_result_expectation(name, &arguments)?;
        if name == "create_project"
            && arguments
                .as_object()
                .is_some_and(|object| object.contains_key("managed_temporary_project"))
        {
            return Err(
                "invalid arguments for tool 'create_project': field 'managed_temporary_project' is no longer supported; use ordinary explicit project creation"
                    .to_string(),
            );
        }
        if name == "create_project"
            && arguments
                .as_object()
                .is_some_and(|object| object.contains_key("allow_existing_empty"))
        {
            return Err(
                "invalid arguments for tool 'create_project': field 'allow_existing_empty' is no longer supported; use 'adopt_existing_empty' to explicitly adopt an existing empty directory"
                    .to_string(),
            );
        }
        // Reject unknown tool names up front with a helpful message that lists
        // every accepted tool and points the caller at canonical discovery. This
        // avoids leaking a raw serde "unknown variant" error and gives model/API
        // callers an actionable discovery hint.
        let definition = lookup_tool_definition(name).ok_or_else(|| {
            format!(
                "unknown tool '{}'. Available tools: {}. Call read_tool_manifest with \
                 an exact tool_name (or use its category/intent views) to discover \
                 accepted model-visible tool names.",
                name,
                model_visible_tool_names_csv()
            )
        })?;
        validate_structured_validation_sync_wait(name, &arguments)?;
        validate_project_artifact_arguments(name, &arguments)?;
        validate_read_project_artifact_expected_sha256(name, &arguments)?;
        if name == "apply_patch"
            && arguments
                .as_object()
                .is_some_and(|object| object.contains_key("strict_matching"))
        {
            return Err(
                "invalid arguments for tool 'apply_patch': field 'strict_matching' is no longer supported; use matching_mode='exact_unique' for former strict_matching=true, matching_mode='first_match' for former strict_matching=false, or omit matching_mode for the current unique default"
                    .to_string(),
            );
        }
        let mut arguments = strip_tool_call_expectation_metadata(arguments);
        validate_run_shell_login(name, &arguments)?;
        let normalization = canonicalize_process_argv_alias(name, &mut arguments)?
            .then_some(crate::ToolInputNormalizationCode::ArgvToArgs);
        canonicalize_cargo_check_packages(name, &mut arguments)?;
        if name == "read_tool_manifest" {
            if let Some(object) = arguments.as_object_mut() {
                if !object.contains_key("include_recommended_flows") {
                    let exact_lookup = object.contains_key("tool_name") || object.contains_key("query");
                    object.insert(
                        "include_recommended_flows".to_string(),
                        Value::Bool(!exact_lookup),
                    );
                }
            }
        }
        if name == "cargo_test" {
            if let Some(object) = arguments.as_object_mut() {
                if object.get("lib").and_then(Value::as_bool) == Some(false) {
                    object.remove("lib");
                }
            }
        }
        if name == "cargo_fmt" {
            if let Some(object) = arguments.as_object_mut() {
                // Positive sync_wait_secs is a recognized caller-shape hint in
                // ensure-format mode, but that mode is intentionally synchronous.
                // Canonicalize the inert hint away before concrete ToolCall serde
                // so execution/audit truth has one representation: omission.
                if object.get("check").and_then(Value::as_bool) != Some(true) {
                    object.remove("sync_wait_secs");
                }
            }
        }
        if name == "read_project_artifact_chunk"
            && arguments
                .as_object()
                .is_some_and(|object| object.contains_key("max_bytes"))
        {
            return Err(
                "invalid arguments for tool 'read_project_artifact_chunk': field 'max_bytes' is no longer supported; use 'length'"
                    .to_string(),
            );
        }
        if name == "write_project_file"
            && arguments
                .as_object()
                .is_some_and(|object| object.contains_key("expected_content_prefix"))
        {
            return Err(
                "invalid arguments for tool 'write_project_file': field 'expected_content_prefix' is no longer supported; use expected_read_revision"
                    .to_string(),
            );
        }
        if name == "work_on_project" {
            validate_coding_project_source_shape(name, &arguments)?;
        }
        let mut wrapped = serde_json::Map::new();
        wrapped.insert(
            TOOL_CALL_TOOL_FIELD.to_string(),
            Value::String(name.to_string()),
        );
        if definition.requires_artifact_upload_path_binding() {
            let missing_path = arguments
                .as_object()
                .and_then(|obj| obj.get("path"))
                .and_then(Value::as_str)
                .map(str::is_empty)
                .unwrap_or(true);
            if missing_path {
                return Err(format!(
                    "invalid arguments for tool '{}': path is required and must match the path \
                     used by begin_artifact_upload to bind upload_id to the requested target path",
                    name
                ));
            }
        }
        if !definition.uses_unit_arguments() {
            // Non-unit tools always carry a `params` object so variants whose
            // fields are all optional (e.g. `list_jobs`) still deserialize when
            // a caller passes `null` arguments. A null argument is normalized
            // to an empty object; required-field validation still fires for
            // tools that need fields.
            let params = if arguments.is_null() {
                Value::Object(serde_json::Map::new())
            } else {
                arguments
            };
            wrapped.insert(TOOL_CALL_PARAMS_FIELD.to_string(), params);
        }
        let call: Self = serde_json::from_value(Value::Object(wrapped))
            .map_err(|e| format!("invalid arguments for tool '{}': {}", name, e))?;
        if let Self::PluginTool(plugin) = &call {
            plugin
                .validate()
                .map_err(|error| format!("invalid arguments for tool '{}': {}", name, error))?;
        }
        if let Self::SshResource(ssh_resource) = &call {
            ssh_resource
                .validate()
                .map_err(|error| format!("invalid arguments for tool '{}': {}", name, error))?;
        }
        Ok((call, normalization))
    }

    /// Raw command text for shell-like calls. Consumed only by the workspace
    /// activity recorder, which truncates it to a bounded preview and honors
    /// the operator's preview config switch — it is never logged verbatim.
    pub fn command_text(&self) -> Option<&str> {
        match self {
            Self::RunShell { command, .. }
            | Self::SessionShellExec { command, .. }
            | Self::RunJob { command, .. } => Some(command),
            _ => None,
        }
    }

    pub fn tool_name(&self) -> &'static str {
        match self {
            Self::ListTools { .. } => "list_tools",
            Self::StartSession { .. } => "start_session",
            Self::WorkOnProject { .. } => "work_on_project",
            Self::FinishCodingTask { .. } => "finish_coding_task",
            Self::PresentPdf { .. } => "present_pdf",
            Self::ReadPdfChunk { .. } => "read_pdf_chunk",
            Self::PresentWorkResult { .. } => "present_work_result",
            Self::WorkResultState { .. } => "get_work_result_state",
            Self::WorkResultActivityDetail { .. } => "read_work_result_activity_detail",
            Self::WorkResultSendMessage { .. } => "send_work_result_message",
            Self::ChangesFileDiff { .. } => "read_changed_file_diff",
            Self::ListSessions { .. } => "list_sessions",
            Self::SessionSummary { .. } => "read_session_summary",
            Self::UpdateSessionContext { .. } => "update_session_context",
            Self::CloseSession { .. } => "close_session",
            Self::ValidationSummary { .. } => "read_validation_summary",
            Self::RecordExternalObservation { .. } => "record_external_observation",
            Self::ListExternalObservations { .. } => "list_external_observations",
            Self::PostSessionMessage { .. } => "post_session_message",
            Self::PostPeerMessage { .. } => "post_peer_message",
            Self::ListSessionMessages { .. } => "list_session_messages",
            Self::GetSessionAssignment { .. } => "get_session_assignment",
            Self::ObserveSessionMessages { .. } => "observe_session_messages",
            Self::ResolveSessionMessage { .. } => "resolve_session_message",
            Self::CompleteSessionMessage { .. } => "complete_session_message",
            Self::SessionDiscussionSummary { .. } => "read_session_discussion_summary",
            Self::SessionHandoffSummary { .. } => "read_session_handoff",
            Self::SessionHandoffState { .. } => "get_session_handoff_state",
            #[cfg(feature = "workspace-checkpoints")]
            Self::WorkspaceCheckpointCreate { .. } => "create_workspace_checkpoint",
            #[cfg(feature = "workspace-checkpoints")]
            Self::WorkspaceCheckpointList { .. } => "list_workspace_checkpoints",
            #[cfg(feature = "workspace-checkpoints")]
            Self::WorkspaceCheckpointShow { .. } => "read_workspace_checkpoint",
            #[cfg(feature = "workspace-checkpoints")]
            Self::WorkspaceCheckpointRestore { .. } => "restore_workspace_checkpoint",
            #[cfg(feature = "workspace-checkpoints")]
            Self::WorkspaceCheckpointDelete { .. } => "delete_workspace_checkpoint",
            #[cfg(feature = "experimental-code-mode")]
            Self::CodeModeExec { .. } => "execute_code_mode",
            #[cfg(feature = "experimental-code-mode")]
            Self::CodeModeExecEffectful { .. } => "execute_effectful_code_mode",
            #[cfg(feature = "experimental-code-mode")]
            Self::CodeModeExecMutating { .. } => "execute_mutating_code_mode",
            Self::RunProcess { .. } => "run_process",
            Self::RunDetachedProcess { .. } => "run_detached_process",
            Self::CodingAgentStart { .. } => "start_coding_agent",
            Self::CodingAgentObserve { .. } => "observe_coding_agent",
            Self::CodingAgentCancel { .. } => "cancel_coding_agent",
            Self::RunScript { .. } => "run_script",
            Self::RunShell { .. } => "run_shell",
            Self::OpenSessionShell { .. } => "open_session_shell",
            Self::SessionShellExec { .. } => "execute_session_shell",
            Self::SessionShellStatus { .. } => "get_session_shell_status",
            Self::CloseSessionShell { .. } => "close_session_shell",
            Self::ApplyPatch { .. } => "apply_patch",
            Self::ApplyUnifiedDiff { .. } => "apply_unified_diff",
            Self::DeleteProjectFiles { .. } => "delete_project_files",
            Self::GitRestorePaths { .. } => "restore_git_paths",
            Self::DiscardUntracked { .. } => "discard_untracked",
            Self::GitCommitPaths { .. } => "commit_git_paths",
            Self::GitStatus { .. } => "get_git_status",
            Self::GitDiffHunks { .. } => "read_git_diff_hunks",
            Self::GitReviewSummary { .. } => "read_git_review_summary",
            Self::ReviewChanges { .. } => "review_changes",
            Self::GitLog { .. } => "read_git_log",
            Self::CargoFmt { .. } => "cargo_fmt",
            Self::CargoCheck { .. } => "cargo_check",
            Self::CargoTest { .. } => "cargo_test",
            Self::ProjectBuild { .. } => "project_build",
            Self::ProjectValidate { .. } => "project_validate",
            Self::GoTest { .. } => "go_test",
            Self::ReadFiles { .. } => "read_files",
            Self::SkillLoad { .. } => "load_skill",
            Self::RunSkillResource { .. } => "run_skill_resource",
            Self::SkillList { .. } => "list_skills",
            Self::SkillReadFile { .. } => "read_skill_file",
            Self::SkillVersions { .. } => "list_skill_versions",
            Self::SkillInstall { .. } => "install_skill",
            Self::SkillActivate { .. } => "activate_skill",
            Self::SkillRemoveRevision { .. } => "remove_skill_revision",
            Self::PrepareGoalWorkflow { .. } => "prepare_goal_workflow",
            Self::CreateGoal { .. } => "create_goal",
            Self::GetGoal { .. } => "get_goal",
            Self::PresentGoalPlan { .. } => "present_goal_plan",
            Self::GoalPlanSync { .. } => "sync_goal_plan",
            Self::CheckpointGoal { .. } => "checkpoint_goal",
            Self::OpenWebcodexWorkbench { .. } => "open_webcodex_workbench",
            Self::SearchWebcodexResources { .. } => "search_webcodex_resources",
            Self::ReadWebcodexResource { .. } => "read_webcodex_resource",
            Self::ListGoals { .. } => "list_goals",
            Self::UpdateGoal { .. } => "update_goal",
            Self::AssociateGoalAgentTask { .. } => "associate_goal_agent_task",
            Self::AssociateGoalWorkflowSession { .. } => "associate_goal_workflow_session",
            Self::WaitForAgentEvents { .. } => "wait_for_agent_events",
            Self::ReadAgentWait { .. } => "read_agent_wait",
            Self::CancelAgentWait { .. } => "cancel_agent_wait",
            Self::AgentWaitState { .. } => "get_agent_wait_state",
            Self::CreateAgentTask { .. } => "create_agent_task",
            Self::ListAgentTasks { .. } => "list_agent_tasks",
            Self::ReadAgentTask { .. } => "read_agent_task",
            Self::AssignAgentTask { .. } => "assign_agent_task",
            Self::StartAgentTaskAttempt { .. } => "start_agent_task_attempt",
            Self::StartAgentTaskEndpointContinuation { .. } => {
                "start_agent_task_endpoint_continuation"
            }
            Self::StartAgentTaskCodingRun { .. } => "start_agent_task_coding_run",
            Self::ReconcileAgentTaskCodingRun { .. } => "reconcile_agent_task_coding_run",
            Self::HeartbeatAgentTaskAttempt { .. } => "heartbeat_agent_task_attempt",
            Self::CompleteAgentTaskAttempt { .. } => "complete_agent_task_attempt",
            Self::CreateAgentIdentity { .. } => "create_agent_identity",
            Self::ListAgentIdentities { .. } => "list_agent_identities",
            Self::UpdateAgentIdentity { .. } => "update_agent_identity",
            Self::RotateAgentContinuationEndpoint { .. } => "rotate_agent_continuation_endpoint",

            Self::PresentAgentContinuation { .. } => "present_agent_continuation",
            Self::AgentContinuationBind { .. } => "bind_agent_continuation",
            Self::AgentContinuationRecoverEndpoint { .. } => "recover_agent_continuation_endpoint",
            Self::AgentContinuationState { .. } => "get_agent_continuation_state",
            Self::AgentContinuationWakeAcquire { .. } => "acquire_agent_continuation_wake",
            Self::AgentContinuationWakePrepare { .. } => "prepare_agent_continuation_wake",
            Self::AgentContinuationWakeFinish { .. } => "finish_agent_continuation_wake",
            Self::AgentContinuationUnbind { .. } => "unbind_agent_continuation",
            Self::DetachAgentEndpoint { .. } => "detach_agent_endpoint",
            Self::CreateConversation { .. } => "create_conversation",
            Self::ListConversations { .. } => "list_conversations",
            Self::ReadConversation { .. } => "read_conversation",
            Self::PostConversationMessage { .. } => "post_conversation_message",
            Self::ListAgentInbox { .. } => "list_agent_inbox",
            Self::ConsumeAgentDeliveries { .. } => "consume_agent_deliveries",
            Self::BootstrapAgentConversation { .. } => "bootstrap_agent_conversation",
            Self::ConsumeAgentWake { .. } => "consume_agent_wake",
            Self::MemorySearch { .. } => "search_memory",
            Self::MemoryRead { .. } => "read_memory",
            Self::MemorySet { .. } => "set_memory",
            Self::MemoryDelete { .. } => "delete_memory",
            Self::MemoryScopeList { .. } => "list_memory_scopes",
            Self::MemoryScopePurge { .. } => "purge_memory_scope",
            Self::RunJob { .. } => "run_job",
            Self::StopJob { .. } => "stop_job",
            Self::JobWriteInput { .. } => "write_job_input",
            Self::ObserveJobs { .. } => "observe_jobs",
            Self::WaitForJobReadiness { .. } => "wait_for_job_readiness",
            Self::WaitForJobTerminal { .. } => "wait_for_job_terminal",
            Self::PresentJobTerminalContinuation { .. } => "present_job_terminal_continuation",
            Self::JobTerminalContinuationBind { .. } => "bind_job_terminal_continuation",
            Self::JobTerminalContinuationState { .. } => "get_job_terminal_continuation_state",
            Self::JobTerminalContinuationPrepare { .. } => "prepare_job_terminal_continuation",
            Self::JobTerminalContinuationFinish { .. } => "finish_job_terminal_continuation",
            Self::JobTerminalContinuationUnbind { .. } => "unbind_job_terminal_continuation",
            Self::ListProjectFiles { .. } => "list_project_files",
            Self::ListProjectTrackedFiles { .. } => "list_project_tracked_files",
            Self::ProjectOverview { .. } => "read_project_overview",
            Self::SearchProjectTexts { .. } => "search_project_texts",
            Self::SearchAndRead { .. } => "search_file_context",
            Self::ShowChanges { .. } => "read_workspace_changes",
            Self::WorkspaceHygieneCheck { .. } => "check_workspace_hygiene",
            Self::ListJobs { .. } => "list_jobs",
            Self::CurrentWindowActivity { .. } => "read_current_window_activity",
            Self::JobTail { .. } => "read_job_tail",
            Self::WriteProjectFile { .. } => "write_project_file",
            Self::SaveProjectArtifact { .. } => "save_project_artifact",
            Self::ImportConversationFilesToProject { .. } => "import_host_files",
            Self::TransferProjectArtifact { .. } => "transfer_project_artifact",
            Self::AcceptArtifactHandoff { .. } => "import_artifact_handoff",
            Self::ProjectArtifact { .. } => "inspect_project_artifact",
            Self::ReadProjectArtifactMetadata { .. } => "read_project_artifact_metadata",
            Self::ReadProjectArtifact { .. } => "read_project_artifact_chunk",
            Self::ArtifactUploadBegin { .. } => "begin_artifact_upload",
            Self::ArtifactUploadChunk { .. } => "upload_artifact_chunk",
            Self::ArtifactUploadFinish { .. } => "finish_artifact_upload",
            Self::ArtifactUploadAbort { .. } => "abort_artifact_upload",
            Self::ApplyTextEdits { .. } => "edit_project_files",
            Self::LspStatus { .. } => "get_lsp_status",
            Self::DocumentSymbols { .. } => "list_document_symbols",
            Self::DocumentDiagnostics { .. } => "read_document_diagnostics",
            Self::Hover { .. } => "read_symbol_hover",
            Self::WorkspaceSymbols { .. } => "list_workspace_symbols",
            Self::GotoDefinition { .. } => "find_definition",
            Self::FindReferences { .. } => "find_references",
            Self::CallHierarchy { .. } => "read_call_hierarchy",
            Self::BrowserObserve(..) => "observe_browser",
            Self::BrowserAct(..) => "control_browser",
            Self::ComputerObserve(..) => "observe_computer",
            Self::ComputerControl(..) => "control_computer",
            Self::ComputerSaveSnapshot { .. } => "save_computer_snapshot",
            Self::ResolveWorkspace { .. } => "resolve_workspace",
            Self::UnregisterProjects { .. } => "unregister_projects",
            Self::ListProjects { .. } => "list_projects",
            Self::RegisterProject { .. } => "register_project",
            Self::UnregisterProject { .. } => "unregister_project",
            Self::CreateProject { .. } => "create_project",
            Self::ListRunners { .. } => "list_runners",
            Self::RunnerConfigCheck { .. } => "check_runner_config",
            Self::RunnerConfigReload { .. } => "reload_runner_config",
            Self::PluginTool(_) => "plugin_tool",
            Self::SshResource(_) => "manage_ssh_resource",
            Self::RuntimeStatus { .. } => "get_runtime_status",
            Self::ReadToolTrace { .. } => "read_tool_trace",
            Self::ToolManifest { .. } => "read_tool_manifest",
        }
    }

    pub fn session_id(&self) -> Option<&str> {
        match self {
            // External-observation ingress/read carries an exact business Session
            // that is independently re-authorized by the runtime method. Keep it
            // out of this generic recorder projection so adapter traffic cannot
            // become native Session evidence or consume the bounded event tail.
            Self::RecordExternalObservation { .. } | Self::ListExternalObservations { .. } => None,
            #[cfg(feature = "experimental-code-mode")]
            Self::CodeModeExec { session_id, .. }
            | Self::CodeModeExecEffectful { session_id, .. }
            | Self::CodeModeExecMutating { session_id, .. } => Some(session_id.as_str()),
            Self::RunProcess { session_id, .. }
            | Self::RunDetachedProcess { session_id, .. }
            | Self::RunScript { session_id, .. }
            | Self::RunShell { session_id, .. }
            | Self::ApplyPatch { session_id, .. }
            | Self::ApplyUnifiedDiff { session_id, .. }
            | Self::DeleteProjectFiles { session_id, .. }
            | Self::GitRestorePaths { session_id, .. }
            | Self::DiscardUntracked { session_id, .. }
            | Self::GitCommitPaths { session_id, .. }
            | Self::GitStatus { session_id, .. }
            | Self::GitDiffHunks { session_id, .. }
            | Self::GitReviewSummary { session_id, .. }
            | Self::ReviewChanges { session_id, .. }
            | Self::GitLog { session_id, .. }
            | Self::CargoFmt { session_id, .. }
            | Self::CargoCheck { session_id, .. }
            | Self::CargoTest { session_id, .. }
            | Self::ProjectBuild { session_id, .. }
            | Self::ProjectValidate { session_id, .. }
            | Self::GoTest { session_id, .. }
            | Self::ReadFiles { session_id, .. }
            | Self::SkillLoad { session_id, .. }
            | Self::RunSkillResource { session_id, .. }
            | Self::SkillList { session_id, .. }
            | Self::SkillReadFile { session_id, .. }
            | Self::SkillVersions { session_id, .. }
            | Self::SkillInstall { session_id, .. }
            | Self::SkillActivate { session_id, .. }
            | Self::SkillRemoveRevision { session_id, .. }
            | Self::MemorySearch { session_id, .. }
            | Self::MemoryRead { session_id, .. }
            | Self::MemorySet { session_id, .. }
            | Self::MemoryDelete { session_id, .. }
            | Self::RunJob { session_id, .. }
            | Self::StopJob { session_id, .. }
            | Self::ListProjectFiles { session_id, .. }
            | Self::ListProjectTrackedFiles { session_id, .. }
            | Self::ProjectOverview { session_id, .. }
            | Self::SearchProjectTexts { session_id, .. }
            | Self::SearchAndRead { session_id, .. }
            | Self::ShowChanges { session_id, .. }
            | Self::WriteProjectFile { session_id, .. }
            | Self::SaveProjectArtifact { session_id, .. }
            | Self::ComputerSaveSnapshot { session_id, .. }
            | Self::ProjectArtifact { session_id, .. }
            | Self::ReadProjectArtifactMetadata { session_id, .. }
            | Self::ReadProjectArtifact { session_id, .. }
            | Self::ArtifactUploadBegin { session_id, .. }
            | Self::ArtifactUploadChunk { session_id, .. }
            | Self::ArtifactUploadFinish { session_id, .. }
            | Self::ArtifactUploadAbort { session_id, .. }
            | Self::ApplyTextEdits { session_id, .. }
            | Self::WorkspaceHygieneCheck { session_id, .. }
            | Self::LspStatus { session_id, .. }
            | Self::DocumentSymbols { session_id, .. }
            | Self::DocumentDiagnostics { session_id, .. }
            | Self::Hover { session_id, .. }
            | Self::WorkspaceSymbols { session_id, .. }
            | Self::GotoDefinition { session_id, .. }
            | Self::FindReferences { session_id, .. } => session_id.as_deref(),
            #[cfg(feature = "workspace-checkpoints")]
            Self::WorkspaceCheckpointCreate { session_id, .. }
            | Self::WorkspaceCheckpointList { session_id, .. }
            | Self::WorkspaceCheckpointShow { session_id, .. }
            | Self::WorkspaceCheckpointRestore { session_id, .. }
            | Self::WorkspaceCheckpointDelete { session_id, .. } => session_id.as_deref(),
            Self::SessionHandoffSummary { session_id, .. }
            | Self::FinishCodingTask { session_id, .. } => Some(session_id.as_str()),            // Window-card presentation/refresh never becomes generic Session recorder
            // evidence. An optional Session selector is association evidence only.
            Self::PresentWorkResult { .. }
            | Self::WorkResultState { .. }
            | Self::WorkResultActivityDetail { .. }
            | Self::WorkResultSendMessage { .. }
            | Self::ChangesFileDiff { .. }
            | Self::SessionHandoffState { .. } => None,
            Self::ImportConversationFilesToProject { session_id, .. } => session_id.as_deref(),
            Self::CallHierarchy { session_id, .. } => session_id.as_deref(),
            Self::WorkOnProject { session_id, .. } => session_id.as_deref(),
            Self::OpenSessionShell { session_id, .. }
            | Self::SessionShellExec { session_id, .. }
            | Self::SessionShellStatus { session_id, .. }
            | Self::CloseSessionShell { session_id, .. } => Some(session_id.as_str()),
            _ => None,
        }
    }

    /// Attach explicit generic recorder provenance to a CodingAgentStart only.
    /// This never makes the recorder a business Session or Run authority.
    pub fn with_coding_agent_recording_session_id(
        mut self,
        recorder_session_id: Option<String>,
    ) -> Self {
        if let (
            Self::CodingAgentStart {
                recording_session_id,
                ..
            },
            Some(recorder_session_id),
        ) = (&mut self, recorder_session_id)
        {
            if recording_session_id.is_none() {
                *recording_session_id = Some(recorder_session_id);
            }
        }
        self
    }

    /// Apply project-matched Session defaults without overwriting explicit
    /// per-call arguments.
    pub fn with_session_execution_context(
        mut self,
        execution_context: &SessionExecutionContext,
    ) -> Self {
        match &mut self {
            Self::RunProcess { cwd, .. }
            | Self::RunDetachedProcess { cwd, .. }
            | Self::RunScript { cwd, .. }
            | Self::RunSkillResource { cwd, .. }
            | Self::ProjectBuild { cwd, .. }
                if cwd.is_none() =>
            {
                *cwd = execution_context.default_cwd.clone();
            }
            Self::RunShell { cwd, shell, .. }
            | Self::RunJob { cwd, shell, .. }
            | Self::OpenSessionShell { cwd, shell, .. } => {
                if cwd.is_none() {
                    *cwd = execution_context.default_cwd.clone();
                }
                if shell.is_none() {
                    *shell = execution_context.default_shell;
                }
            }
            _ => {}
        }
        self
    }

    pub fn project(&self) -> Option<&str> {
        match self {
            Self::ListSessions { project, .. }
            | Self::RecordExternalObservation { project, .. }
            | Self::ListExternalObservations { project, .. } => Some(project),
            #[cfg(feature = "experimental-code-mode")]
            Self::CodeModeExec { project, .. }
            | Self::CodeModeExecEffectful { project, .. }
            | Self::CodeModeExecMutating { project, .. } => Some(project.as_str()),
            Self::AcceptArtifactHandoff {
                destination_project,
                ..
            } => Some(destination_project.as_str()),            Self::RunProcess { project, .. }
            | Self::RunDetachedProcess { project, .. }
            | Self::CodingAgentStart { project, .. }
            | Self::StartAgentTaskCodingRun { project, .. }
            | Self::RunScript { project, .. }
            | Self::RunShell { project, .. }
            | Self::OpenSessionShell { project, .. }
            | Self::SessionShellExec { project, .. }
            | Self::SessionShellStatus { project, .. }
            | Self::CloseSessionShell { project, .. }
            | Self::ApplyPatch { project, .. }
            | Self::ApplyUnifiedDiff { project, .. }
            | Self::DeleteProjectFiles { project, .. }
            | Self::GitRestorePaths { project, .. }
            | Self::DiscardUntracked { project, .. }
            | Self::GitCommitPaths { project, .. }
            | Self::GitStatus { project, .. }
            | Self::GitDiffHunks { project, .. }
            | Self::GitReviewSummary { project, .. }
            | Self::ReviewChanges { project, .. }
            | Self::GitLog { project, .. }
            | Self::CargoFmt { project, .. }
            | Self::CargoCheck { project, .. }
            | Self::CargoTest { project, .. }
            | Self::ProjectBuild { project, .. }
            | Self::ProjectValidate { project, .. }
            | Self::GoTest { project, .. }
            | Self::ReadFiles { project, .. }
            | Self::SkillLoad { project, .. }
            | Self::RunSkillResource { project, .. }
            | Self::SkillList { project, .. }
            | Self::SkillReadFile { project, .. }
            | Self::SkillVersions { project, .. }
            | Self::SkillInstall { project, .. }
            | Self::SkillActivate { project, .. }
            | Self::SkillRemoveRevision { project, .. }
            | Self::MemorySearch { project, .. }
            | Self::MemoryRead { project, .. }
            | Self::MemorySet { project, .. }
            | Self::MemoryDelete { project, .. }
            | Self::RunJob { project, .. }
            | Self::StopJob { project, .. }
            | Self::JobWriteInput { project, .. }
            | Self::ListProjectFiles { project, .. }
            | Self::ListProjectTrackedFiles { project, .. }
            | Self::ProjectOverview { project, .. }
            | Self::SearchProjectTexts { project, .. }
            | Self::SearchAndRead { project, .. }
            | Self::ShowChanges { project, .. }
            | Self::WriteProjectFile { project, .. }
            | Self::SaveProjectArtifact { project, .. }
            | Self::ComputerSaveSnapshot { project, .. }
            | Self::ImportConversationFilesToProject { project, .. }
            | Self::ProjectArtifact { project, .. }
            | Self::ReadProjectArtifactMetadata { project, .. }
            | Self::ReadProjectArtifact { project, .. }
            | Self::ArtifactUploadBegin { project, .. }
            | Self::ArtifactUploadChunk { project, .. }
            | Self::ArtifactUploadFinish { project, .. }
            | Self::ArtifactUploadAbort { project, .. }
            | Self::ApplyTextEdits { project, .. }
            | Self::WorkspaceHygieneCheck { project, .. }
            | Self::LspStatus { project, .. }
            | Self::DocumentSymbols { project, .. }
            | Self::DocumentDiagnostics { project, .. }
            | Self::Hover { project, .. }
            | Self::WorkspaceSymbols { project, .. }
            | Self::GotoDefinition { project, .. }
            | Self::FindReferences { project, .. } => Some(project.as_str()),
            #[cfg(feature = "workspace-checkpoints")]
            Self::WorkspaceCheckpointCreate { project, .. }
            | Self::WorkspaceCheckpointList { project, .. }
            | Self::WorkspaceCheckpointShow { project, .. }
            | Self::WorkspaceCheckpointRestore { project, .. }
            | Self::WorkspaceCheckpointDelete { project, .. } => Some(project.as_str()),
            Self::CallHierarchy { project, .. } => Some(project.as_str()),
            Self::WorkOnProject { project, .. } if !project.trim().is_empty() => {
                Some(project.as_str())
            }
            Self::FinishCodingTask { project, .. }
            | Self::PresentPdf { project, .. }
            | Self::ReadPdfChunk { project, .. }
            | Self::PresentWorkResult { project, .. }
            | Self::WorkResultState { project, .. }
            | Self::WorkResultActivityDetail { project, .. }
            | Self::WorkResultSendMessage { project, .. }
            | Self::ChangesFileDiff { project, .. } => Some(project.as_str()),
            Self::UpdateSessionContext { project, .. }
            | Self::ValidationSummary { project, .. } => Some(project.as_str()),
            Self::SessionHandoffSummary { project, .. } => project.as_deref(),
            Self::SessionHandoffState { project, .. } => Some(project.as_str()),
            _ => None,
        }
    }
}
