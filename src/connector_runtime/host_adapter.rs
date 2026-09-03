//! Root ToolRuntime/validation-profile adapter for the Connector domain port.

use super::integration;
use crate::auth::AuthContext;
use crate::tool_runtime::kernel::{
    HostFileImportTrust, ToolCallContext, ToolCallErrorStatus,
    ToolCallRequest as KernelToolCallRequest, ToolTransport,
};
use crate::tool_runtime::validation_profile::{resolve_validation_recipe, RecipeId, SemanticCheck};
use crate::tool_runtime::ToolRuntime;
use std::path::Path;
use std::sync::Arc;
use webcodex_connector_runtime::{
    ConnectorExecutionHost, ConnectorHostFuture, ConnectorJobHostError, ConnectorJobRequest,
    ConnectorJobSubmission, ConnectorProjectRegistration, ConnectorRecipeId,
    ConnectorSemanticCheck, ConnectorToolFailure, ConnectorToolRequest, ConnectorTransport,
    ConnectorValidationEvidenceRequest, ConnectorValidationPlan, ConnectorValidationPlanError,
    ConnectorValidationPlanRequest,
};

#[derive(Clone)]
pub(super) struct RootConnectorHost {
    tools: Arc<ToolRuntime>,
    auth: AuthContext,
}

impl RootConnectorHost {
    pub(super) fn new(tools: Arc<ToolRuntime>, auth: AuthContext) -> Self {
        Self { tools, auth }
    }
}

fn tool_transport(transport: ConnectorTransport) -> ToolTransport {
    match transport {
        ConnectorTransport::Api => ToolTransport::Api,
        ConnectorTransport::Mcp => ToolTransport::Mcp,
    }
}

fn recipe_id(recipe: ConnectorRecipeId) -> RecipeId {
    match recipe {
        ConnectorRecipeId::Rust => RecipeId::Rust,
        ConnectorRecipeId::Node => RecipeId::Node,
        ConnectorRecipeId::Python => RecipeId::Python,
        ConnectorRecipeId::Go => RecipeId::Go,
    }
}

fn semantic_check(check: ConnectorSemanticCheck) -> SemanticCheck {
    match check {
        ConnectorSemanticCheck::Format => SemanticCheck::Format,
        ConnectorSemanticCheck::Check => SemanticCheck::Check,
        ConnectorSemanticCheck::Test => SemanticCheck::Test,
    }
}

impl ConnectorExecutionHost for RootConnectorHost {
    fn invoke_tool(
        &self,
        request: ConnectorToolRequest,
    ) -> ConnectorHostFuture<'_, Result<serde_json::Value, ConnectorToolFailure>> {
        Box::pin(async move {
            let outcome = self
                .tools
                .call_tool_with_context(
                    KernelToolCallRequest {
                        tool_name: request.tool_name,
                        arguments: request.arguments,
                    },
                    ToolCallContext {
                        transport: tool_transport(request.transport),
                        session_id: None,
                        auth: Some(&self.auth),
                        window: None,
                        record_oauth_scope_denials: false,
                        host_file_import_trust: HostFileImportTrust::Untrusted,
                    },
                )
                .await;
            match outcome.error_status {
                Some(ToolCallErrorStatus::InsufficientScope {
                    required_scope,
                    description,
                }) => Err(ConnectorToolFailure::Permission {
                    required: integration::permission_from_scope(required_scope),
                    message: description,
                }),
                Some(ToolCallErrorStatus::InvalidArguments { message }) => {
                    Err(ConnectorToolFailure::InvalidArguments(message))
                }
                None => {
                    let result = outcome.result.ok_or_else(|| {
                        ConnectorToolFailure::Adapter(
                            "tool kernel outcome without result".to_string(),
                        )
                    })?;
                    if result.success {
                        Ok(result.output)
                    } else {
                        Err(ConnectorToolFailure::Tool {
                            error: result.error,
                            output: result.output,
                        })
                    }
                }
            }
        })
    }

    fn register_isolated_project(
        &self,
        request: ConnectorProjectRegistration,
    ) -> ConnectorHostFuture<'_, Result<(), ConnectorJobHostError>> {
        Box::pin(async move {
            let result = self
                .tools
                .register_project(
                    request.client_id,
                    request.project_id,
                    request.name,
                    request.path,
                    request.description,
                    true,
                    false,
                    Some(&self.auth),
                )
                .await;
            if result.success {
                Ok(())
            } else {
                Err(ConnectorJobHostError::Rejected(result.error))
            }
        })
    }

    fn start_execution_job(
        &self,
        request: ConnectorJobRequest,
    ) -> ConnectorHostFuture<'_, Result<ConnectorJobSubmission, ConnectorJobHostError>> {
        Box::pin(async move {
            let result = self
                .tools
                .run_job_for_auth(
                    request.project,
                    request.command,
                    None,
                    Some(request.timeout_secs as i64),
                    request.cwd,
                    request.validation_steps,
                    Some(&self.auth),
                )
                .await;
            if !result.success {
                return Err(ConnectorJobHostError::Rejected(result.error));
            }
            let job_id = result.output["job_id"]
                .as_str()
                .ok_or_else(|| {
                    ConnectorJobHostError::Adapter(
                        "execution host result omitted job_id".to_string(),
                    )
                })?
                .to_string();
            let status = result.output["status"]
                .as_str()
                .unwrap_or("queued")
                .to_string();
            Ok(ConnectorJobSubmission { job_id, status })
        })
    }

    fn stop_execution_job(
        &self,
        project: String,
        job_id: String,
    ) -> ConnectorHostFuture<'_, Result<(), ConnectorJobHostError>> {
        Box::pin(async move {
            let result = self
                .tools
                .stop_job_model_facing(project, job_id, None, true, Some(&self.auth))
                .await;
            if result.success {
                Ok(())
            } else {
                Err(ConnectorJobHostError::OutcomeUnknown(result.error))
            }
        })
    }

    fn plan_validation(
        &self,
        request: ConnectorValidationPlanRequest,
    ) -> Result<ConnectorValidationPlan, ConnectorValidationPlanError> {
        let recipe = request.recipe.map(recipe_id);
        let checks = request
            .checks
            .into_iter()
            .map(semantic_check)
            .collect::<Vec<_>>();
        let resolved = resolve_validation_recipe(
            Path::new(&request.execution_root),
            request.cwd.as_deref(),
            recipe,
            &checks,
            request.test_filter.as_deref(),
        )
        .map_err(|error| ConnectorValidationPlanError {
            code: error.code.to_string(),
            details: error.details,
        })?;
        Ok(ConnectorValidationPlan {
            durable_identity: resolved.durable_identity(),
            recipe_root_relative: resolved.recipe_root_relative,
            steps: resolved.steps,
            test_filter: resolved.test_filter,
        })
    }

    fn validation_failure_evidence(
        &self,
        request: ConnectorValidationEvidenceRequest,
    ) -> serde_json::Value {
        super::execution::durable_assertion_evidence(
            &request.check,
            request.recipe_identity.as_ref(),
            request.exit_code,
            &request.stdout,
            &request.stderr,
        )
    }
}
