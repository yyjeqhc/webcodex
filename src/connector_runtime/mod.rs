//! Root composition for the transport-neutral Connector runtime.
//!
//! Durable task/execution/workspace behavior is owned by
//! `webcodex-connector-runtime`. Root keeps authentication, credential
//! verification, ToolRuntime policy, HTTP/readiness, and operator UI adapters.

#[cfg(test)]
#[path = "console_test_support.rs"]
pub(crate) mod execution_tests;
mod host_adapter;
pub(crate) mod http;
mod integration;
pub(crate) mod surface;
#[cfg(test)]
#[path = "root_test_support.rs"]
pub(crate) mod tests;

use crate::auth::{AuthContext, ProjectAgentTokenVerifier, ProjectCredentialVerifier};
use crate::client_window::ClientWindow;
use crate::project_entry::RemoteProbe;
use crate::shell_client::RunnerFeature;
use crate::tool_runtime::ToolRuntime;
use crate::Database;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;
use webcodex_connector_runtime::{
    ConnectorCallContext as DomainCallContext, ConnectorCallOutcome as DomainCallOutcome,
    ConnectorWindowId,
};
use webcodex_store::{ConnectorExecution, ConnectorTaskSnapshot, ConnectorTaskStoreError};

pub(crate) use webcodex_connector_runtime::{
    approval_projection, durable_task_review_projection, result_projection, validate_opaque_id,
    ConnectorContext, LocalResultDecision, TaskCancelInput, TaskReviewInput,
};

pub(crate) mod workspace {
    pub(crate) use webcodex_connector_runtime::workspace::*;
}

#[derive(Clone, Default)]
pub(crate) struct ConnectorRuntimeSlot(pub(crate) Option<Arc<ConnectorRuntime>>);

#[derive(Debug, Clone, Copy)]
pub(crate) enum ConnectorTransport {
    Api,
    Mcp,
}

impl ConnectorTransport {
    fn domain(self) -> webcodex_connector_runtime::ConnectorTransport {
        match self {
            Self::Api => webcodex_connector_runtime::ConnectorTransport::Api,
            Self::Mcp => webcodex_connector_runtime::ConnectorTransport::Mcp,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ConnectorCallOutcome {
    pub ok: bool,
    pub body: Value,
    pub http_status: u16,
    pub required_scope: Option<&'static str>,
    /// Invalid capability names and malformed inputs are JSON-RPC parameter
    /// errors. Task state and executor failures are normal tool errors.
    pub protocol_error: bool,
}

impl ConnectorCallOutcome {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn error(
        http_status: u16,
        code: impl Into<String>,
        message: impl Into<String>,
        retryable: bool,
        user_action_required: bool,
        suggested_action: Option<&str>,
        required_scope: Option<&'static str>,
        protocol_error: bool,
    ) -> Self {
        Self {
            ok: false,
            body: json!({
                "ok": false,
                "task_id": null,
                "run_id": null,
                "event_cursor": null,
                "data": null,
                "warnings": [],
                "blocking": true,
                "error": {
                    "code": code.into(),
                    "message": message.into(),
                    "retryable": retryable,
                    "user_action_required": user_action_required,
                    "suggested_action": suggested_action
                }
            }),
            http_status,
            required_scope,
            protocol_error,
        }
    }

    fn from_domain(outcome: DomainCallOutcome) -> Self {
        Self {
            ok: outcome.ok,
            body: outcome.body,
            http_status: outcome.http_status,
            required_scope: outcome
                .required_permission
                .map(integration::permission_scope),
            protocol_error: outcome.protocol_error,
        }
    }
}

pub(crate) fn store_error_outcome(
    error: ConnectorTaskStoreError,
    task: Option<&ConnectorTaskSnapshot>,
) -> ConnectorCallOutcome {
    ConnectorCallOutcome::from_domain(webcodex_connector_runtime::store_error_outcome(error, task))
}

pub(crate) struct ConnectorRuntime {
    tools: Arc<ToolRuntime>,
    pub(crate) db: Arc<Database>,
    context: ConnectorContext,
    inner: webcodex_connector_runtime::ConnectorRuntime,
    credential: ProjectCredentialVerifier,
    project_agent_token: Option<ProjectAgentTokenVerifier>,
}

impl ConnectorRuntime {
    pub(crate) fn new(
        tools: Arc<ToolRuntime>,
        db: Arc<Database>,
        context: ConnectorContext,
        credential: ProjectCredentialVerifier,
    ) -> Result<Self, String> {
        if credential.grant_id() != context.project_grant_id {
            return Err("project credential does not match connector grant identity".to_string());
        }
        let inner = webcodex_connector_runtime::ConnectorRuntime::new(
            tools.shell_clients.clone(),
            db.clone(),
            context.clone(),
        )?;
        tools.observations.set_connector_configured();
        Ok(Self {
            tools,
            db,
            context,
            inner,
            credential,
            project_agent_token: None,
        })
    }

    pub(crate) fn with_project_agent_token(mut self, verifier: ProjectAgentTokenVerifier) -> Self {
        self.project_agent_token = Some(verifier);
        self
    }

    pub(crate) fn from_context(
        tools: Arc<ToolRuntime>,
        db: Arc<Database>,
        context: Option<ConnectorContext>,
    ) -> Result<ConnectorRuntimeSlot, String> {
        crate::model_surface::validate_connector_runtime_presence(
            tools.runtime_exposure(),
            context.is_some(),
        )?;
        let Some(context) = context else {
            return Ok(ConnectorRuntimeSlot::default());
        };
        let credential_path = webcodex_connector_runtime::required_env(
            webcodex_connector_runtime::PROJECT_CREDENTIAL_FILE_ENV,
        )?;
        let credential = ProjectCredentialVerifier::from_file(
            context.project_grant_id.clone(),
            Path::new(&credential_path),
        )?;
        let agent_token_path = webcodex_connector_runtime::required_env(
            webcodex_connector_runtime::PROJECT_AGENT_TOKEN_FILE_ENV,
        )?;
        let agent_token = ProjectAgentTokenVerifier::from_file(
            context.project_grant_id.clone(),
            context.executor_client_id()?.to_string(),
            "local-owner".to_string(),
            Path::new(&agent_token_path),
        )?;
        Ok(ConnectorRuntimeSlot(Some(Arc::new(
            Self::new(tools, db, context, credential)?.with_project_agent_token(agent_token),
        ))))
    }

    pub(crate) fn context(&self) -> &ConnectorContext {
        &self.context
    }

    pub(crate) fn workflow_sessions_console_list(
        &self,
        limit: Option<usize>,
    ) -> crate::tool_runtime::sessions::WorkflowSessionConsoleList {
        self.tools
            .workflow_sessions_console_list(&self.context.project_id, limit)
    }

    pub(crate) fn workflow_session_console_detail(
        &self,
        session_id: &str,
        limit: Option<usize>,
    ) -> Option<crate::tool_runtime::sessions::WorkflowSessionConsoleDetail> {
        self.tools
            .workflow_session_console_detail(&self.context.project_id, session_id, limit)
    }

    #[cfg(test)]
    pub(crate) fn tool_runtime_for_test(&self) -> &ToolRuntime {
        self.tools.as_ref()
    }

    pub(crate) fn authenticate_project_credential(&self, token: &str) -> Option<AuthContext> {
        self.credential.authenticate(token)
    }

    pub(crate) fn authenticate_project_agent_token(&self, token: &str) -> Option<AuthContext> {
        self.project_agent_token
            .as_ref()
            .and_then(|verifier| verifier.authenticate(token))
    }

    fn project_access_allowed(&self, auth: &AuthContext) -> bool {
        auth.is_bootstrap()
            || auth.project_grant_id.as_deref() == Some(self.context.project_grant_id.as_str())
    }

    fn domain_context(
        &self,
        auth: &AuthContext,
        window: Option<&ClientWindow>,
    ) -> Result<(DomainCallContext, Option<ConnectorWindowId>), String> {
        let access = integration::connector_access(auth)?;
        let window = window.map(integration::connector_window);
        Ok((
            DomainCallContext {
                access,
                execution_authority: integration::connector_execution_authority(&self.tools),
                host: Arc::new(host_adapter::RootConnectorHost::new(
                    self.tools.clone(),
                    auth.clone(),
                )),
            },
            window,
        ))
    }

    fn execution_context(
        &self,
        auth: &AuthContext,
    ) -> Result<DomainCallContext, ConnectorCallOutcome> {
        self.domain_context(auth, None)
            .map(|(context, _)| context)
            .map_err(|_| store_error_outcome(ConnectorTaskStoreError::NotFound, None))
    }

    pub(crate) async fn readiness(
        &self,
        auth: &AuthContext,
    ) -> Option<crate::project_entry::ProjectReadiness> {
        if !self.project_access_allowed(auth) {
            return None;
        }
        let Some((client_id, project_id)) = self
            .context
            .executor_project
            .strip_prefix("agent:")
            .and_then(|value| value.split_once(':'))
        else {
            return Some(self.observed_readiness(RemoteProbe::ProjectMissing));
        };
        let access = crate::shell_client::runner_access_from_auth(Some(auth));
        let Some(runner) = self
            .tools
            .shell_clients
            .get_client_semantic_view_for_auth(client_id, access.as_ref())
            .await
        else {
            return Some(self.observed_readiness(RemoteProbe::RunnerOffline));
        };
        if runner.view.status != "online" || !runner.view.connected {
            return Some(self.observed_readiness(RemoteProbe::RunnerOffline));
        }
        if !runner
            .view
            .projects
            .iter()
            .any(|project| project.id == project_id && !project.disabled)
        {
            return Some(self.observed_readiness(RemoteProbe::ProjectMissing));
        }
        if !(runner.supports(RunnerFeature::Shell)
            && runner.supports(RunnerFeature::FileRead)
            && runner.supports(RunnerFeature::FileWrite)
            && runner.supports(RunnerFeature::Jobs)
            && runner.supports(RunnerFeature::AsyncJobs)
            && runner.supports(RunnerFeature::AsyncShellJobs))
        {
            return Some(self.observed_readiness(RemoteProbe::RequiredCapabilityMissing));
        }
        if !runner.supports(RunnerFeature::StructuredValidationArgv) {
            return Some(self.observed_readiness(RemoteProbe::StructuredValidationMissing));
        }
        Some(self.observed_readiness(RemoteProbe::Ready))
    }

    fn observed_readiness(&self, probe: RemoteProbe) -> crate::project_entry::ProjectReadiness {
        let status = if matches!(probe, RemoteProbe::Ready) {
            "ready"
        } else {
            "not_ready"
        };
        self.tools.observations.record_connector_observation(
            status,
            "readiness_probe",
            chrono::Utc::now().timestamp(),
        );
        crate::project_entry::runtime_readiness(Some(self.context.project_name.clone()), probe)
    }

    pub(crate) async fn call_for_window(
        &self,
        capability: &str,
        arguments: Value,
        auth: Option<&AuthContext>,
        transport: ConnectorTransport,
        window: Option<&ClientWindow>,
    ) -> ConnectorCallOutcome {
        self.call_for_window_inner(capability, arguments, auth, transport, window, false)
            .await
    }

    pub(crate) async fn call_for_window_with_task_polling(
        &self,
        capability: &str,
        arguments: Value,
        auth: Option<&AuthContext>,
        transport: ConnectorTransport,
        window: Option<&ClientWindow>,
    ) -> ConnectorCallOutcome {
        self.call_for_window_inner(capability, arguments, auth, transport, window, true)
            .await
    }

    async fn call_for_window_inner(
        &self,
        capability: &str,
        arguments: Value,
        auth: Option<&AuthContext>,
        transport: ConnectorTransport,
        window: Option<&ClientWindow>,
        defer_execution_guidance: bool,
    ) -> ConnectorCallOutcome {
        let Some(auth) = auth else {
            return ConnectorCallOutcome::error(
                401,
                "authentication_required",
                "connector capabilities require an authenticated identity",
                false,
                true,
                Some("Configure Bearer authentication in the connector client."),
                None,
                false,
            );
        };
        let (context, window) = match self.domain_context(auth, window) {
            Ok(context) => context,
            Err(message) => {
                return ConnectorCallOutcome::error(
                    403,
                    "identity_not_supported",
                    message,
                    false,
                    true,
                    Some("Use a user, OAuth, shared-key, or bootstrap connector credential."),
                    None,
                    false,
                )
            }
        };
        let outcome = if defer_execution_guidance {
            self.inner
                .call_for_window_with_task_polling(
                    capability,
                    arguments,
                    Some(&context),
                    transport.domain(),
                    window.as_ref(),
                )
                .await
        } else {
            self.inner
                .call_for_window(
                    capability,
                    arguments,
                    Some(&context),
                    transport.domain(),
                    window.as_ref(),
                )
                .await
        };
        if outcome.ok {
            self.tools.observations.record_connector_observation(
                "request_succeeded",
                "connector_request",
                chrono::Utc::now().timestamp(),
            );
        }
        ConnectorCallOutcome::from_domain(outcome)
    }

    pub(crate) async fn host_review(
        &self,
        auth: &AuthContext,
        input: TaskReviewInput,
    ) -> ConnectorCallOutcome {
        let context = match self.execution_context(auth) {
            Ok(context) => context,
            Err(outcome) => return outcome,
        };
        ConnectorCallOutcome::from_domain(self.inner.host_review(&context, input).await)
    }

    pub(crate) async fn host_cancel(
        &self,
        auth: &AuthContext,
        input: TaskCancelInput,
    ) -> ConnectorCallOutcome {
        let context = match self.execution_context(auth) {
            Ok(context) => context,
            Err(outcome) => return outcome,
        };
        ConnectorCallOutcome::from_domain(self.inner.host_cancel(&context, input).await)
    }

    pub(crate) fn host_decide(
        &self,
        task_id: &str,
        result_id: Option<&str>,
        decision: LocalResultDecision,
        reason: Option<&str>,
        now: i64,
    ) -> Result<webcodex_store::ConnectorTaskResult, ConnectorTaskStoreError> {
        self.inner
            .host_decide(task_id, result_id, decision, reason, now)
    }

    pub(crate) fn host_guide(&self, task_id: &str, message: &str) -> ConnectorCallOutcome {
        ConnectorCallOutcome::from_domain(self.inner.host_guide(task_id, message))
    }

    pub(crate) async fn ordinary_execution_result_for_auth(
        &self,
        execution_id: &str,
        auth: &AuthContext,
    ) -> Result<ConnectorCallOutcome, ConnectorCallOutcome> {
        let context = self.execution_context(auth)?;
        self.inner
            .ordinary_execution_result_for_auth(execution_id, &context)
            .await
            .map(ConnectorCallOutcome::from_domain)
            .map_err(ConnectorCallOutcome::from_domain)
    }

    pub(crate) fn materialize_execution_task_for_auth(
        &self,
        execution_id: &str,
        auth: &AuthContext,
    ) -> Result<ConnectorExecution, ConnectorCallOutcome> {
        let context = self.execution_context(auth)?;
        self.inner
            .materialize_execution_task_for_auth(execution_id, &context)
            .map_err(ConnectorCallOutcome::from_domain)
    }

    pub(crate) async fn execution_task_result_for_auth(
        &self,
        execution_id: &str,
        auth: &AuthContext,
    ) -> Result<
        (
            ConnectorTaskSnapshot,
            ConnectorExecution,
            ConnectorCallOutcome,
        ),
        ConnectorCallOutcome,
    > {
        let context = self.execution_context(auth)?;
        self.inner
            .execution_task_result_for_auth(execution_id, &context)
            .await
            .map(|(task, execution, outcome)| {
                (task, execution, ConnectorCallOutcome::from_domain(outcome))
            })
            .map_err(ConnectorCallOutcome::from_domain)
    }

    pub(crate) async fn cancel_execution_task_for_auth(
        &self,
        execution_id: &str,
        auth: &AuthContext,
    ) -> Result<(), ConnectorCallOutcome> {
        let context = self.execution_context(auth)?;
        self.inner
            .cancel_execution_task_for_auth(execution_id, &context)
            .await
            .map_err(ConnectorCallOutcome::from_domain)
    }

    pub(crate) async fn activity_visibility<'a>(
        &self,
        auth: &'a AuthContext,
    ) -> (
        crate::tool_runtime::activity::ActivityVisibility<'a>,
        Vec<String>,
    ) {
        use crate::tool_runtime::activity::ActivityVisibility;
        let access = crate::shell_client::runner_access_from_auth(Some(auth));
        let clients = self
            .tools
            .shell_clients
            .list_clients_for_auth(access.as_ref())
            .await
            .into_iter()
            .map(|client| client.client_id)
            .collect();
        if auth.is_bootstrap() || auth.is_admin() {
            return (ActivityVisibility::Global, clients);
        }
        match auth.project_grant_id.as_deref() {
            Some(grant) if !grant.is_empty() => (ActivityVisibility::ProjectGrant(grant), clients),
            _ => (ActivityVisibility::ProjectGrant(""), Vec::new()),
        }
    }

    pub(crate) async fn host_devices(&self, auth: &AuthContext) -> crate::tool_runtime::ToolResult {
        self.tools.list_runners(Some(auth)).await
    }
}
