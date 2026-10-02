//! Workflow startup orchestration and the canonical work_on_project entry.

use super::observations::repository_overview_not_requested;
use super::project::{
    attach_permission, attach_project_resolution, invalid_project_source, resolve_project_source,
    CodingProjectSource, ManagedWorktreeRequest,
};
use super::projection::{
    append_workspace_warnings, owning_runner_available, project_coding_agent_providers,
    project_work_on_project_output_inner, recommended_flow_payload,
    recommended_flow_payload_for_manifest_tools, resolved_project_payload, rules_summary,
    startup_verdict, workspace_payload_from_git_summary,
};
use super::*;

#[derive(Debug, Clone, Copy)]
struct CodingStartupOptions {
    guidance_profile: CodingGuidanceProfile,
    tool_name: &'static str,
    detail: StartupDetail,
    include_repository_overview: bool,
    include_instruction_content: bool,
    include_extension_catalog: bool,
}

impl CodingStartupOptions {
    #[cfg(test)]
    fn diagnostic(detail: StartupDetail) -> Self {
        Self {
            guidance_profile: CodingGuidanceProfile::Direct,
            detail,
            tool_name: "work_on_project",
            include_repository_overview: true,
            include_instruction_content: true,
            include_extension_catalog: false,
        }
    }

    fn work_on_project(
        include_extension_catalog: bool,
        guidance_profile: CodingGuidanceProfile,
    ) -> Self {
        Self {
            guidance_profile,
            detail: StartupDetail::Standard,
            tool_name: "work_on_project",
            include_repository_overview: false,
            include_instruction_content: false,
            include_extension_catalog,
        }
    }
}

impl ToolRuntime {
    #[allow(clippy::too_many_arguments)]
    async fn start_coding_workflow(
        &self,
        project: String,
        client_id: Option<String>,
        path: Option<String>,
        mut managed_worktree: Option<ManagedWorktreeRequest>,
        title: Option<String>,
        mode: SessionMode,
        deny_write_tools: bool,
        deny_shell_tools: bool,
        startup: CodingStartupOptions,
        resume_session_id: Option<String>,
        execution_context: Option<sessions::SessionExecutionContext>,
        auth: Option<&AuthContext>,
        trusted_recording_session_id: Option<&str>,
        trusted_recording_session_project: Option<&str>,
        transport: SessionTransport,
        bootstrap_context: &mut Option<BootstrapContext>,
    ) -> ToolResult {
        let detail = startup.detail;
        let project_source = match resolve_project_source(project, client_id, path) {
            Ok(source) => source,
            Err(result) => return result,
        };
        let resume_requested = resume_session_id.is_some();
        let execution_context = match execution_context
            .map(sessions::SessionExecutionContext::validated)
            .transpose()
        {
            Ok(context) => context,
            Err(error) => {
                return ToolResult::err_with_output(
                    error,
                    json!({
                        "error_kind": "invalid_execution_context",
                        "failure_kind": "invalid_arguments",
                        "field": "execution_context",
                        "state_changed": false,
                    }),
                );
            }
        };
        let resume_session_id = match resume_session_id {
            Some(session_id)
                if session_id != session_id.trim()
                    || !sessions::is_valid_session_id(&session_id) =>
            {
                return ToolResult::err_with_output(
                    "resume_session_id must be a valid wc_sess_* Workflow Session id",
                    json!({
                        "error_kind": "invalid_resume_session_id",
                        "failure_kind": "invalid_arguments",
                        "field": "resume_session_id",
                        "expected_format": "wc_sess_*",
                        "state_changed": false,
                    }),
                );
            }
            Some(session_id) => Some(session_id),
            None => None,
        };
        let resume_session_project = match resume_session_id.as_deref() {
            Some(session_id) => match self
                .explicit_coding_session_project(session_id, startup.tool_name, auth)
                .await
            {
                Ok(project) => project,
                Err(result) => return result,
            },
            None => None,
        };
        if let (Some(worktree), Some(session_project)) =
            (managed_worktree.as_mut(), resume_session_project.as_ref())
        {
            let Some(project_id) = crate::tool_runtime::lsp_tools::runner_local_project_id(
                &session_project.resolved_id,
            ) else {
                return session_project_mismatch_result(
                    resume_session_id
                        .as_deref()
                        .expect("resume project requires session id"),
                    startup.tool_name,
                    &SessionProjectMismatch {
                        session_project: session_project.resolved_id.clone(),
                        request_project: "managed_worktree".to_string(),
                    },
                );
            };
            worktree.resume_project_id = Some(project_id.to_string());
        }
        let trusted_recording_session_resolved_project = match (
            trusted_recording_session_id,
            trusted_recording_session_project,
        ) {
            (Some(_), Some(project)) => {
                match self.resolve_project_input_for_auth(project, auth).await {
                    Ok(resolved) => Some(resolved),
                    Err(error) => return error.into_tool_result(),
                }
            }
            _ => None,
        };
        let title = match title {
            Some(title) => {
                let title = title.trim().to_string();
                if title.is_empty()
                    || title.chars().count() > sessions::MAX_CODING_INSTRUCTION_CHARS
                {
                    return ToolResult::err_with_output(
                        format!(
                            "title must contain 1..={} characters",
                            sessions::MAX_CODING_INSTRUCTION_CHARS
                        ),
                        json!({
                            "error_kind": "invalid_coding_instruction",
                            "field": "title",
                            "max_chars": sessions::MAX_CODING_INSTRUCTION_CHARS,
                        }),
                    );
                }
                Some(title)
            }
            None => None,
        };
        let (project, mut project_resolution) = match self
            .resolve_coding_startup_project(
                project_source,
                managed_worktree.as_ref(),
                resume_session_id.as_deref(),
                resume_session_project.as_ref(),
                trusted_recording_session_id,
                trusted_recording_session_resolved_project.as_ref(),
                startup.tool_name,
                auth,
            )
            .await
        {
            Ok(resolved) => resolved,
            Err(result) => return result,
        };
        // `detail` is the single startup projection control: full keeps the
        // complete runtime status, recent commits, rules, and tool manifest;
        // standard/minimal use the compact projections.
        let compact_startup = detail != StartupDetail::Full;
        let include_recent_commits = detail == StartupDetail::Full;
        let include_tool_manifest = detail == StartupDetail::Full;
        let tool_manifest = if include_tool_manifest {
            match self.compact_tool_manifest_payload_bounded(None, None, None) {
                Ok(payload) => Some(payload),
                Err(result) => return attach_project_resolution(result, &project_resolution),
            }
        } else {
            None
        };

        let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
            Ok(resolved) => resolved,
            Err(err) => {
                return attach_project_resolution(err.into_tool_result(), &project_resolution)
            }
        };
        project_resolution.resolved_project = resolved.resolved_id.clone();
        if let Some(session_id) = resume_session_id.as_deref() {
            if resume_session_project
                .as_ref()
                .map(|project| project.resolved_id.as_str())
                != Some(resolved.resolved_id.as_str())
            {
                return attach_project_resolution(
                    session_project_mismatch_result(
                        session_id,
                        startup.tool_name,
                        &SessionProjectMismatch {
                            session_project: resume_session_project
                                .as_ref()
                                .map(|project| project.resolved_id.clone())
                                .unwrap_or_else(|| "<unscoped>".to_string()),
                            request_project: resolved.resolved_id.clone(),
                        },
                    ),
                    &project_resolution,
                );
            }
        }
        if let Some(recording_session_id) = trusted_recording_session_id {
            if trusted_recording_session_project != Some(resolved.resolved_id.as_str()) {
                return attach_project_resolution(
                    session_project_mismatch_result(
                        recording_session_id,
                        startup.tool_name,
                        &SessionProjectMismatch {
                            session_project: trusted_recording_session_project
                                .unwrap_or("<unscoped>")
                                .to_string(),
                            request_project: resolved.resolved_id.clone(),
                        },
                    ),
                    &project_resolution,
                );
            }
        }
        // LSP availability is advisory: observe it concurrently, but stop waiting
        // when the mandatory startup observations finish.
        let (startup_complete, startup_completed) = tokio::sync::oneshot::channel();
        let extension_discovery = async {
            if startup.include_extension_catalog {
                Some(self.extension_discovery_for_startup(&resolved, auth).await)
            } else {
                None
            }
        };
        let startup_context = Box::pin(async {
            let repository_overview = async {
                if startup.include_repository_overview {
                    self.repository_overview_for_startup(&resolved, auth).await
                } else {
                    repository_overview_not_requested()
                }
            };
            futures_util::future::join3(
                self.load_effective_coding_instructions(&resolved, auth),
                repository_overview,
                extension_discovery,
            )
            .await
        });
        let mandatory = async {
            let observations = futures_util::future::join3(
                startup_context,
                self.runtime_status(auth),
                Box::pin(
                    self.coding_startup_git_summary(&resolved.resolved_id, include_recent_commits),
                ),
            )
            .await;
            let _ = startup_complete.send(());
            observations
        };
        let (
            semantic_navigation,
            (
                (project_instructions, repository_overview, extensions),
                runtime_status_result,
                (git, git_warnings),
            ),
        ) = futures_util::future::join(
            self.probe_semantic_navigation_for_startup(&resolved, startup_completed),
            mandatory,
        )
        .await;
        let semantic_navigation = serde_json::to_value(semantic_navigation).unwrap_or_else(|_| {
            json!({
                "supported": false,
                "available": Value::Null,
                "status": "probe_failed",
                "reason_code": "status_probe_failed",
            })
        });
        *bootstrap_context = Some(BootstrapContext {
            project: resolved.clone(),
            instructions: project_instructions.clone(),
        });
        // Coding startup always observes every fixed repository-rule
        // candidate. The complete bounded body remains only in the in-memory
        // Workflow Session; the ledger persistence path omits it.
        let mut warnings = git_warnings;
        if repository_overview.get("status").and_then(Value::as_str) == Some("unavailable")
            && repository_overview
                .get("reason_code")
                .and_then(Value::as_str)
                != Some(REPOSITORY_OVERVIEW_NOT_REQUESTED_REASON)
        {
            warnings.push(json!({
                "kind": "repository_overview_unavailable",
                "message": "repository structure overview was unavailable during startup",
            }));
        }
        let mut runtime_status_call_failed = false;
        let (runtime_status, runtime_status_for_brief) = {
            if !runtime_status_result.success {
                runtime_status_call_failed = true;
                warnings.push(json!({
                    "kind": "runtime_status_unavailable",
                    "message": "runtime status was unavailable during startup",
                }));
            }
            let raw = runtime_status_result.output;
            let projected = if compact_startup {
                compact_runtime_status(&raw)
            } else {
                raw.clone()
            };
            (projected, raw)
        };
        let owning_runner_available = owning_runner_available(
            &resolved,
            &runtime_status_for_brief,
            runtime_status_call_failed,
        );
        let coding_agent_providers =
            project_coding_agent_providers(&resolved.config.client_id, &runtime_status_for_brief);
        // Surface dirty/conflict worktree state at top-level so compact Action
        // responses that omit full git payloads still keep the warning reason.
        if !git.is_null() {
            append_workspace_warnings(&workspace_payload_from_git_summary(&git), &mut warnings);
        }
        let git_baseline_tree = if resume_session_id.is_none() {
            self.capture_coding_git_baseline_tree(&resolved.resolved_id, &git, &mut warnings)
                .await
        } else {
            None
        };
        let write_scope_verified =
            auth.is_none_or(|auth| auth.has_scope(crate::auth::SCOPE_PROJECT_WRITE));
        let authority_fingerprint = match workflow_session_authority_fingerprint(auth) {
            Ok(fingerprint) => fingerprint,
            Err(_) => {
                return attach_project_resolution(
                    ToolResult::err_with_output(
                        "caller has no canonical Workflow Session authority identity",
                        json!({
                            "error_kind": "session_authority_identity_unavailable",
                            "failure_kind": "session_authority_denied",
                            "state_changed": false,
                        }),
                    ),
                    &project_resolution,
                );
            }
        };
        let session_outcome = match self.sessions.ensure_coding_session_with_git_baseline(
            sessions::CodingSessionRequest {
                project: resolved.resolved_id.clone(),
                authority_fingerprint,
                resume_session_id: resume_session_id.clone(),
                instruction: title.clone(),
                mode,
                guards: sessions::SessionGuards {
                    deny_write_tools,
                    deny_shell_tools,
                },
                execution_context,
                project_instructions: Some(project_instructions.clone()),
                transport,
                // Startup always re-reads bounded current Git state and the
                // fixed project-instruction candidates.
                context_refreshed: true,
                write_scope_verified,
            },
            git_baseline_tree,
        ) {
            Ok(outcome) => outcome,
            Err(error) => {
                return attach_project_resolution(
                    coding_session_start_error(error, mode),
                    &project_resolution,
                );
            }
        };
        let project_instructions = session_outcome
            .project_instructions
            .as_ref()
            .unwrap_or(&project_instructions);
        let session_summary = &session_outcome.summary;
        let mut connection_state = runtime_status
            .get("connection_layers")
            .cloned()
            .unwrap_or_else(|| {
                json!({
                    "runner_process": {"status": "not_observed"},
                    "server_transport": {"status": "not_observed"},
                    "server_registration": {"status": "not_observed"},
                    "project_registry": {"status": "resolved", "resolved_project": resolved.resolved_id},
                    "last_successful_tool_call": {"status": "not_observed"},
                })
            });
        connection_state["project_registry"]["resolved_project"] = json!(resolved.resolved_id);
        let recommended_flow = match &tool_manifest {
            Some(manifest) => recommended_flow_payload_for_manifest_tools(manifest),
            None => recommended_flow_payload(),
        };
        // Continuation feedback for reused/resumed/restored sessions. Pure
        // read-only projection over existing session ledger, validation evidence,
        // bounded job metadata, and the message board. Never executes shell,
        // reads project files, enqueues Runner requests, mutates the ledger,
        // refreshes activity, or consumes guidance. `created` (fresh empty
        // session) surfaces a compact `not_applicable` verdict.
        let continuation_kind = if resume_requested {
            "resumed_explicitly"
        } else {
            "created"
        };
        // Read the lifecycle-aware, project-scoped summary once, after the
        // potentially slow startup probes, then share it across continuation,
        // the legacy full verdict, and the model-facing brief.
        let active_jobs = self
            .active_jobs_summary(
                Some(&resolved.resolved_id),
                Some(&session_outcome.summary.session_id),
                auth,
                10,
            )
            .await;
        let continuation_feedback = self
            .startup_continuation_feedback(
                &session_outcome.summary,
                session_outcome.pre_instruction_summary.as_ref(),
                continuation_kind,
                &active_jobs,
                git.pointer("/counts/conflicted")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    > 0,
            )
            .await;
        let session_ref = self.session_reference_for_id(&session_summary.session_id, auth);
        let mut output = json!({
            "detail": detail.as_str(),
            "project": project.clone(),
            "project_resolution": project_resolution.clone(),
            "resolved_project": resolved_project_payload(&resolved),
            "session": {
                "session_id": session_summary.session_id,
                "mode": session_summary.mode,
                "guards": session_summary.guards,
                "execution_context": session_summary.execution_context,
                "lifecycle": session_summary.lifecycle,
                "continuation": if resume_requested { "resumed_explicitly" } else { "created" },
                "reused": session_outcome.reused,
                "resume_requested": resume_requested,
                "instruction_appended": title.is_some(),
                "root_title": session_summary.title,
                "capability": {
                    "changed": session_outcome.capability_changed,
                    "previous_mode": session_outcome.previous_mode,
                    "previous_guards": session_outcome.previous_guards,
                    "requested_mode": mode,
                    "mode": session_summary.mode,
                    "guards": session_summary.guards,
                    "write_scope_verified": write_scope_verified,
                },
                "context": {
                    "refreshed": true,
                    "git_state_recaptured": true,
                    "rules_recaptured": true,
                    "execution_context_changed": session_outcome.execution_context_changed,
                },
                "explicit_resume_required_for_continuation": true,
                "explicit_session_id_fields": {
                    "tool_business_input": "session_id",
                    "generic_wrapper_recorder": TOOL_CALL_RECORDING_SESSION_ID_FIELD
                },
            },
            "runtime_status": runtime_status.clone(),
            "connection_state": connection_state,
            "authority": authority_profile_payload(),
            "rules": rules_summary(Some(project_instructions)),
            "git": git.clone(),
            "semantic_navigation": semantic_navigation.clone(),
            "recommended_flow": recommended_flow,
            "continuation_feedback": continuation_feedback.clone(),
            "deterministic": true,
            "llm_summary": false,
            "warnings": warnings,
        });
        if let Some(session_ref) = session_ref.as_deref() {
            output["session"]["session_ref"] = json!(session_ref);
        }
        if let Some(tool_manifest) = tool_manifest {
            output["tool_manifest"] = tool_manifest;
        }
        output["startup_verdict"] = startup_verdict(
            &output,
            &active_jobs,
            owning_runner_available,
            runtime_status_call_failed,
            include_tool_manifest,
        );
        let previous_instructions = session_outcome
            .pre_instruction_summary
            .as_ref()
            .and_then(|summary| summary.project_instructions.as_ref());
        // Reload rule bodies only when there is no prior snapshot to compare
        // against (fresh session, or a session whose rules were never
        // persisted, e.g. restored after a restart). Otherwise the shared
        // brief compares fingerprints and reports reused/changed. Diagnostic
        // projections may include current/changed bodies; work_on_project keeps
        // bodies out of its primary result and uses context_request when needed.
        let force_instruction_load = previous_instructions.is_none();
        let canonical_repository_root_matches = if resume_requested {
            None
        } else {
            // A fresh Session starts at the currently resolved canonical root.
            Some(true)
        };
        let knowledge_association = self
            .project_knowledge_association_diagnostic(&resolved, auth)
            .await;
        let project_resolution_value =
            serde_json::to_value(&project_resolution).unwrap_or_else(|_| json!({}));
        let project_ref = self.project_reference_for_resolved(&resolved, auth);
        let mut startup_brief = build_startup_brief(StartupBriefInput {
            guidance_profile: startup.guidance_profile,
            detail,
            requested_project: &project,
            project_resolution: &project_resolution_value,
            resolved: &resolved,
            project_ref: project_ref.as_deref(),
            knowledge_association: knowledge_association.as_ref(),
            session: session_summary,
            continuation_kind,
            reused: session_outcome.reused,
            resume_requested,
            instructions: project_instructions,
            previous_instructions,
            force_instruction_load,
            include_instruction_content: startup.include_instruction_content,
            extensions: extensions.as_ref(),
            coding_agent_providers: &coding_agent_providers,
            git: &git,
            semantic_navigation: &semantic_navigation,
            repository: &repository_overview,
            continuation_feedback: &continuation_feedback,
            active_jobs: &active_jobs,
            owning_runner_available,
            canonical_repository_root_matches,
            runtime_status_call_failed,
        });
        if let Some(session_ref) = session_ref.as_deref() {
            startup_brief["session"]["session_ref"] = json!(session_ref);
        }
        let result = if detail == StartupDetail::Full {
            output["startup_brief"] = startup_brief;
            ToolResult::ok(output)
        } else {
            ToolResult::ok(startup_brief)
        };
        attach_permission(result, project_resolution.permission.as_ref())
    }

    /// Test-only diagnostic projection over the same canonical coding workflow
    /// engine used by `work_on_project`. This is deliberately not a ToolCall or
    /// a model/API identity.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn start_coding_workflow_for_test(
        &self,
        project: String,
        client_id: Option<String>,
        path: Option<String>,
        title: Option<String>,
        mode: SessionMode,
        deny_write_tools: bool,
        deny_shell_tools: bool,
        detail: StartupDetail,
        resume_session_id: Option<String>,
        execution_context: Option<sessions::SessionExecutionContext>,
        auth: Option<&AuthContext>,
        trusted_recording_session_id: Option<&str>,
        trusted_recording_session_project: Option<&str>,
        transport: SessionTransport,
    ) -> ToolResult {
        self.start_coding_workflow(
            project,
            client_id,
            path,
            None,
            title,
            mode,
            deny_write_tools,
            deny_shell_tools,
            CodingStartupOptions::diagnostic(detail),
            resume_session_id,
            execution_context,
            auth,
            trusted_recording_session_id,
            trusted_recording_session_project,
            transport,
            &mut None,
        )
        .await
    }

    async fn extension_discovery_for_startup(
        &self,
        project: &ResolvedProject,
        auth: Option<&AuthContext>,
    ) -> StartupExtensions {
        let skills = self.startup_skills_catalog(project, auth);
        let plugins = async {
            if auth.is_some_and(|auth| !auth.has_scope(crate::auth::SCOPE_PLUGIN_INSPECT)) {
                return StartupPluginsCatalog::unavailable("plugin_inspect_scope_unavailable");
            }
            match self.project_plugin_catalog(project, auth).await {
                Ok(catalog) => {
                    let entries = catalog
                        .entries
                        .into_iter()
                        .map(|entry| StartupPluginEntry {
                            plugin: entry.plugin,
                            name: entry.name,
                            tool: entry.tool,
                            title: entry.title,
                            description: entry
                                .description
                                .as_deref()
                                .map(bounded_extension_description),
                            annotations: entry.annotations,
                        })
                        .collect();
                    StartupPluginsCatalog::available(
                        catalog.catalog_revision,
                        catalog.total_count,
                        entries,
                    )
                }
                Err(reason_code) => StartupPluginsCatalog::unavailable(reason_code),
            }
        };
        let (skills, plugins) = futures_util::future::join(skills, plugins).await;
        StartupExtensions { skills, plugins }
    }

    /// Canonical entry for the daily model coding loop.
    ///
    /// This validates the public inputs, maps them onto the shared coding
    /// workflow engine, and projects a compact startup result. With `session_id` present, it
    /// exactly resumes that one Workflow Session after project/lifecycle/access/
    /// capability checks; without it, it always creates a fresh Session.
    pub(crate) async fn work_on_project(
        &self,
        project: String,
        client_id: Option<String>,
        path: Option<String>,
        mode: Option<String>,
        base_ref: Option<String>,
        instruction: String,
        session_id: Option<String>,
        guidance_profile: CodingGuidanceProfile,
        include_extension_catalog: bool,
        auth: Option<&AuthContext>,
        trusted_recording_session_id: Option<&str>,
        trusted_recording_session_project: Option<&str>,
        transport: SessionTransport,
        correlation: &mut ToolCallCorrelation,
        bootstrap_context: &mut Option<BootstrapContext>,
    ) -> ToolResult {
        let project_source = match resolve_project_source(project, client_id, path) {
            Ok(source) => source,
            Err(result) => return result,
        };
        let mode = mode.as_deref().unwrap_or("checkout");
        if !matches!(mode, "checkout" | "worktree") {
            return invalid_project_source(
                "mode must be 'checkout' or 'worktree'",
                json!({"field": "mode", "allowed": ["checkout", "worktree"]}),
            );
        }
        if mode == "checkout" && base_ref.is_some() {
            return invalid_project_source(
                "base_ref is only valid with mode='worktree'",
                json!({"field": "base_ref", "requires": {"mode": "worktree"}}),
            );
        }
        if let Some(base_ref) = base_ref.as_deref() {
            if base_ref.is_empty() || base_ref.len() > 1024 || base_ref.contains('\0') {
                return invalid_project_source(
                    "base_ref must contain 1..=1024 non-NUL bytes",
                    json!({"field": "base_ref"}),
                );
            }
        }
        let managed_worktree_requested = mode == "worktree";
        let managed_worktree = (mode == "worktree").then(|| ManagedWorktreeRequest {
            base_ref,
            operation_id: uuid::Uuid::new_v4().to_string(),
            resume_project_id: None,
        });
        let (project, client_id, path) = match project_source {
            CodingProjectSource::Existing { project } => (project, None, None),
            CodingProjectSource::RunnerPath { client_id, path } => {
                (String::new(), Some(client_id), Some(path))
            }
        };
        let instruction = instruction.trim().to_string();
        if instruction.is_empty()
            || instruction.chars().count() > sessions::MAX_CODING_INSTRUCTION_CHARS
        {
            return ToolResult::err_with_output(
                format!(
                    "instruction must contain 1..={} characters",
                    sessions::MAX_CODING_INSTRUCTION_CHARS
                ),
                json!({
                    "error_kind": "invalid_coding_instruction",
                    "field": "instruction",
                    "max_chars": sessions::MAX_CODING_INSTRUCTION_CHARS,
                    "state_changed": false,
                }),
            );
        }
        let session_id = match session_id {
            Some(session_id)
                if session_id != session_id.trim()
                    || !sessions::is_valid_session_id(&session_id) =>
            {
                return ToolResult::err_with_output(
                    "session_id must be a valid wc_sess_* Workflow Session id",
                    json!({
                        "error_kind": "invalid_session_id",
                        "failure_kind": "invalid_arguments",
                        "field": "session_id",
                        "expected_format": "wc_sess_*",
                        "state_changed": false,
                    }),
                );
            }
            Some(session_id) => Some(session_id),
            None => None,
        };
        // Map onto the canonical coding workflow engine. The work-on-project
        // profile keeps the standard shared brief,
        // including rules, semantic navigation, workspace, and job metadata,
        // while deliberately skipping the optional repository overview. Without
        // an explicit session_id this always creates a fresh Workflow Session.
        let result = self
            .start_coding_workflow(
                project.clone(),
                client_id,
                path,
                managed_worktree,
                Some(instruction.clone()),
                SessionMode::Normal,
                false,
                false,
                CodingStartupOptions::work_on_project(include_extension_catalog, guidance_profile),
                session_id.clone(),
                None,
                auth,
                trusted_recording_session_id,
                trusted_recording_session_project,
                transport,
                bootstrap_context,
            )
            .await;
        if !result.success {
            return result;
        }
        let projected_project = if project.is_empty() || managed_worktree_requested {
            startup_brief_from_output(&result.output)
                .and_then(|brief| {
                    brief
                        .pointer("/project/resolved_id")
                        .and_then(Value::as_str)
                })
                .unwrap_or_default()
                .to_string()
        } else {
            project
        };
        // Fresh work always starts with Goal admission still undecided. Only an
        // explicit exact Session re-entry projects previously correlated active
        // Goals, avoiding an unnecessary Goal-store read (and any surprising
        // concurrent correlation observation) for a newly created Session.
        let goal_context = session_id.as_ref().and_then(|_| {
            startup_brief_from_output(&result.output)
                .and_then(|brief| brief.pointer("/session/session_id"))
                .and_then(Value::as_str)
                .and_then(|session_id| self.active_goal_context_for_session(auth, session_id))
        });
        let mut projected = project_work_on_project_output_inner(
            projected_project,
            result.output,
            guidance_profile,
            Some(correlation),
        );
        if projected.success {
            if let Some(goal_context) = goal_context {
                projected.output["goal_context"] = goal_context;
            }
        }
        projected
    }
}

fn coding_session_start_error(
    error: sessions::CodingSessionError,
    mode: SessionMode,
) -> ToolResult {
    match error {
        sessions::CodingSessionError::InvalidResumeSessionId => {
            ToolResult::err_with_output(
                "resume_session_id must be a valid wc_sess_* Workflow Session id",
                json!({
                    "error_kind": "invalid_resume_session_id",
                    "failure_kind": "invalid_arguments",
                    "field": "resume_session_id",
                    "expected_format": "wc_sess_*",
                    "state_changed": false,
                }),
            )
        }
        sessions::CodingSessionError::UnknownResumeSession { session_id } => {
            unknown_session_result(&session_id)
        }
        sessions::CodingSessionError::ResumeRetentionExpired { session_id } => {
            session_retention_expired_result(&session_id)
        }
        sessions::CodingSessionError::ResumeSessionNotActive {
            session_id,
            lifecycle,
        } => {
            let error_kind = match lifecycle {
                sessions::SessionLifecycle::Closed => "session_closed",
                sessions::SessionLifecycle::Active => "session_lifecycle_denied",
            };
            ToolResult::err_with_output(
                format!(
                    "{error_kind}: work_on_project cannot resume a {} session",
                    lifecycle.as_str()
                ),
                json!({
                    "error_kind": error_kind,
                    "failure_kind": error_kind,
                    "session_id": session_id,
                    "lifecycle": lifecycle,
                    "resume_requested": true,
                    "state_changed": false,
                }),
            )
        }
        sessions::CodingSessionError::ResumeProjectMismatch {
            session_id,
            session_project,
            request_project,
        } => {
            ToolResult::err_with_output(
                "session_project_mismatch: explicit Workflow Session resume requires an exact project match",
                json!({
                    "error_kind": "session_project_mismatch",
                    "failure_kind": "session_project_mismatch",
                    "session_id": session_id,
                    "session_project": session_project,
                    "request_project": request_project,
                    "resume_requested": true,
                    "state_changed": false,
                }),
            )
        }
        sessions::CodingSessionError::ResumeAuthorityMismatch { session_id } => {
            ToolResult::err_with_output(
                "session_authority_denied",
                json!({
                    "error_kind": "session_authority_denied",
                    "failure_kind": "session_authority_denied",
                    "session_id": session_id,
                    "resume_requested": true,
                    "state_changed": false,
                }),
            )
        }
        sessions::CodingSessionError::WriteScopeRequired => {
            ToolResult::err_with_output(
                "session capability upgrade requires project:write",
                json!({
                    "error_kind": "session_capability_upgrade_denied",
                    "required_scope": crate::auth::SCOPE_PROJECT_WRITE,
                    "mode": mode.as_str(),
                    "state_changed": false,
                }),
            )
        }
        sessions::CodingSessionError::InvalidExecutionContext(error) => {
            ToolResult::err_with_output(
                error,
                json!({
                    "error_kind": "invalid_execution_context",
                    "failure_kind": "invalid_arguments",
                    "field": "execution_context",
                    "state_changed": false,
                }),
            )
        }
        sessions::CodingSessionError::CommitFailed => {
            ToolResult::err_with_output(
                "coding continuity state could not be committed",
                json!({
                    "error_kind": "coding_continuity_commit_failed",
                    "state_changed": false,
                }),
            )
        }
    }
}
