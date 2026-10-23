//! Typed Job requests and their validation, independent of live Registry state.

use super::super::jobs::{command_preview, process_preview, script_preview};
use super::super::requests::capability_upgrade_error;
use super::super::validation::{validate_job_scope, validate_run_request};
use super::*;
use crate::RunnerFeatureSet;
use std::collections::HashSet;
use webcodex_core::runner_operation::{
    RunnerJobBuildOperation, RunnerJobProcessOperation, RunnerJobScriptOperation,
    RunnerJobShellOperation, RunnerJobSkillResourceOperation, RunnerJobValidationOperation,
};
use webcodex_core::runner_protocol::{
    validate_process_argv, validate_script_request, ShellJobCodexMetadata, ShellJobContext,
    ShellRunRequest, ShellScriptLanguage, PROCESS_CWD_MAX_BYTES, PROCESS_STDIN_MAX_BYTES,
    PROCESS_TIMEOUT_MAX_SECS, STRUCTURED_EXECUTION_TIMEOUT_MAX_SECS,
    STRUCTURED_EXECUTION_TIMEOUT_MIN_SECS,
};

/// An internal start request. HTTP operation DTOs are adapted at their boundary;
/// callers cannot attach parameters belonging to a different execution shape.
#[derive(Debug, Clone)]
pub struct JobStartRequest {
    pub client_id: String,
    pub cwd: Option<String>,
    pub timeout_secs: u64,
    pub execution: JobStartExecution,
    pub context: JobStartContext,
    pub codex: Option<ShellJobCodexMetadata>,
}

impl JobStartRequest {
    pub fn new(client_id: impl Into<String>, execution: JobStartExecution) -> Self {
        Self {
            client_id: client_id.into(),
            cwd: None,
            timeout_secs: 120,
            execution,
            context: JobStartContext::default(),
            codex: None,
        }
    }

    pub fn shell(client_id: impl Into<String>, command: impl Into<String>) -> Self {
        Self::new(
            client_id,
            JobStartExecution::Shell {
                command: command.into(),
                target: JobShellTarget::default(),
            },
        )
    }
}

/// Shared authority and observation context. Execution parameters live only in
/// `JobStartExecution`; descriptive shell metadata never selects a shell.
#[derive(Debug, Clone, Default)]
pub struct JobStartContext {
    pub project_id: Option<String>,
    pub session_id: Option<String>,
    pub project_cwd: Option<String>,
    pub purpose: Option<String>,
    pub shell: Option<String>,
    pub visibility: ShellJobVisibility,
}

#[derive(Debug, Clone)]
pub enum JobShellTarget {
    Local {
        shell: Option<ExecutionShell>,
        login: bool,
    },
    Ssh {
        resource: String,
    },
}

impl Default for JobShellTarget {
    fn default() -> Self {
        Self::Local {
            shell: None,
            login: false,
        }
    }
}

/// A validation start owns its steps exactly once. Adapter metadata carries its
/// own plan; direct validation starts carry only the canonical argv steps.
#[derive(Debug, Clone)]
pub enum JobValidationPlan {
    Steps(Vec<ShellJobValidationStep>),
    Metadata(ShellJobValidationMetadata),
}

impl JobValidationPlan {
    fn steps(&self) -> &[ShellJobValidationStep] {
        match self {
            Self::Steps(steps) => steps,
            Self::Metadata(metadata) => &metadata.steps,
        }
    }

    fn metadata(&self) -> Option<&ShellJobValidationMetadata> {
        match self {
            Self::Steps(_) => None,
            Self::Metadata(metadata) => Some(metadata),
        }
    }

    fn into_steps(self) -> Vec<ShellJobValidationStep> {
        match self {
            Self::Steps(steps) => steps,
            Self::Metadata(metadata) => metadata.steps,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct JobValidationCorrelation {
    pub validation_identity: Option<String>,
    pub validation_tool: Option<String>,
    pub assertion_name: Option<String>,
}

#[derive(Debug, Clone)]
pub enum JobStartExecution {
    Shell {
        command: String,
        target: JobShellTarget,
    },
    Validation {
        plan: JobValidationPlan,
        ssh_resource: Option<String>,
    },
    Build(webcodex_core::project_build::ProjectBuildPlan),
    Format {
        plan: webcodex_core::project_format::ProjectFormatPlan,
        runner_instance_id: String,
    },
    Process {
        process: ShellProcessArgv,
        stdin: Option<String>,
        correlation: JobValidationCorrelation,
    },
    InteractiveProcess(ShellProcessArgv),
    DetachedProcess {
        process: ShellProcessArgv,
        stdin: Option<String>,
        /// Caller replay key; never copied into Runner protocol, durable state,
        /// or audit. A stable authenticated initiator is checked at admission.
        idempotency_key: String,
        validation_identity: Option<String>,
        validation_tool: Option<String>,
    },
    Script {
        script: ShellScriptPayload,
        stdin: Option<String>,
        correlation: JobValidationCorrelation,
    },
    SkillResource {
        request: RunnerSkillExecutionRequest,
        correlation: JobValidationCorrelation,
    },
}

pub(super) struct JobStartPlan {
    pub client_id: String,
    pub request_id: String,
    pub created_at: i64,
    pub visibility: ShellJobVisibility,
    context: ShellJobContext,
    timeout_secs: u64,
    execution: JobStartExecution,
}

impl JobStartPlan {
    pub fn new(request: JobStartRequest) -> Result<Self, ExecutionAdmissionError> {
        let JobStartRequest {
            client_id,
            cwd,
            timeout_secs,
            execution,
            context,
            ..
        } = request;
        let normalized_job_cwd = cwd.map(|cwd| cwd.trim().to_string());
        let request_id = next_request_id();
        let created_at = now_ts();
        let ssh_resource = match &execution {
            JobStartExecution::Shell {
                target: JobShellTarget::Ssh { resource },
                ..
            } => Some(resource.clone()),
            JobStartExecution::Validation { ssh_resource, .. } => ssh_resource.clone(),
            _ => None,
        };
        if ssh_resource.is_some() && context.session_id.is_none() {
            return Err(ExecutionAdmissionError::invalid_request(
                "ssh_session_required: an SSH resource requires a Workflow Session id",
            ));
        }
        if ssh_resource.as_deref().is_some_and(|resource| {
            resource.is_empty()
                || resource.len() > 80
                || resource.contains("..")
                || !resource.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
                })
        }) {
            return Err(ExecutionAdmissionError::invalid_request(
                "ssh_resource_invalid: resource name is invalid",
            ));
        }
        let safe_command_preview = match &execution {
            JobStartExecution::Shell { command, target } => {
                if matches!(target, JobShellTarget::Local { shell, login: true }
                    if *shell != Some(ExecutionShell::Bash))
                {
                    return Err(ExecutionAdmissionError::invalid_request(
                        "bash login mode requires local shell=bash",
                    ));
                }
                validate_run_request(&ShellRunRequest {
                    login: false,
                    client_id: client_id.clone(),
                    cwd: normalized_job_cwd.clone(),
                    command: command.clone(),
                    stdin: None,
                    timeout_secs,
                    wait_timeout_secs: 0,
                })
                .map_err(ExecutionAdmissionError::invalid_request)?;
                command_preview(command)
            }
            JobStartExecution::Validation { plan, .. } => {
                let steps = plan.steps();
                let project_all_packages = plan.metadata()
                    .and_then(|metadata| metadata.project_validation.as_ref())
                    .and_then(|provenance| provenance.request.scope.as_ref())
                    .is_some_and(webcodex_core::project_validation::ProjectValidationScope::selects_all_packages);
                if (steps.is_empty() && matches!(plan, JobValidationPlan::Steps(_)))
                    || steps.len() > 3
                    || steps.iter().any(|step| {
                        if project_all_packages {
                            !step.is_canonical_project_step()
                        } else {
                            !step.is_canonical()
                        }
                    })
                    || steps
                        .iter()
                        .map(|step| step.name.as_str())
                        .collect::<HashSet<_>>()
                        .len()
                        != steps.len()
                {
                    return Err(ExecutionAdmissionError::invalid_request(
                        "invalid structured validation plan",
                    ));
                }
                if plan.metadata().is_some_and(|metadata| !metadata.is_valid()) {
                    return Err(ExecutionAdmissionError::invalid_request(
                        "invalid structured validation metadata",
                    ));
                }
                validate_id(&client_id, "client_id")
                    .map_err(ExecutionAdmissionError::invalid_request)?;
                validate_job_scope(normalized_job_cwd.as_deref(), timeout_secs)
                    .map_err(ExecutionAdmissionError::invalid_request)?;
                format!(
                    "validation: {}",
                    steps
                        .iter()
                        .map(|step| step.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            JobStartExecution::Format { plan, .. } => {
                plan.digest()
                    .map_err(ExecutionAdmissionError::invalid_request)?;
                if context.visibility != ShellJobVisibility::Public
                    || context.project_id.as_deref()
                        != Some(format!("agent:{client_id}:{}", plan.request.project_id).as_str())
                {
                    return Err(ExecutionAdmissionError::permission_denied(
                        "invalid project format Job authority",
                    ));
                }
                validate_structured_job_common(
                    normalized_job_cwd.as_deref(),
                    None,
                    timeout_secs,
                    webcodex_core::project_format::PROJECT_FORMAT_TIMEOUT_MAX_SECS,
                )
                .map_err(ExecutionAdmissionError::invalid_request)?;
                "project_format selected files".into()
            }
            JobStartExecution::Build(plan) => {
                if !plan.is_valid() {
                    return Err(ExecutionAdmissionError::invalid_request(
                        "invalid typed project build Job plan",
                    ));
                }
                validate_process_argv(&plan.process)
                    .map_err(ExecutionAdmissionError::invalid_request)?;
                validate_structured_job_common(
                    normalized_job_cwd.as_deref(),
                    None,
                    timeout_secs,
                    PROCESS_TIMEOUT_MAX_SECS,
                )
                .map_err(ExecutionAdmissionError::invalid_request)?;
                process_preview(
                    &plan.process.executable,
                    plan.process.args.iter().map(String::as_str),
                )
            }
            JobStartExecution::Process { process, stdin, .. }
            | JobStartExecution::DetachedProcess { process, stdin, .. } => {
                validate_process_argv(process).map_err(ExecutionAdmissionError::invalid_request)?;
                validate_structured_job_common(
                    normalized_job_cwd.as_deref(),
                    stdin.as_deref(),
                    timeout_secs,
                    PROCESS_TIMEOUT_MAX_SECS,
                )
                .map_err(ExecutionAdmissionError::invalid_request)?;
                if matches!(execution, JobStartExecution::DetachedProcess { .. }) {
                    format!("detached process ({} args)", process.args.len())
                } else {
                    process_preview(&process.executable, process.args.iter().map(String::as_str))
                }
            }
            JobStartExecution::InteractiveProcess(process) => {
                validate_process_argv(process).map_err(ExecutionAdmissionError::invalid_request)?;
                validate_structured_job_common(
                    normalized_job_cwd.as_deref(),
                    None,
                    timeout_secs,
                    PROCESS_TIMEOUT_MAX_SECS,
                )
                .map_err(ExecutionAdmissionError::invalid_request)?;
                format!("interactive process ({} args)", process.args.len())
            }
            JobStartExecution::Script { script, stdin, .. } => {
                validate_script_request(
                    script,
                    stdin.as_deref(),
                    normalized_job_cwd.as_deref(),
                    timeout_secs,
                )
                .map_err(ExecutionAdmissionError::invalid_request)?;
                script_preview(
                    script.language.as_str(),
                    script.script.len(),
                    script.args.len(),
                )
            }
            JobStartExecution::SkillResource { request, .. } => {
                request.validate().map_err(|error| {
                    ExecutionAdmissionError::invalid_request(format!(
                        "invalid Runner Skill execution request: {error}"
                    ))
                })?;
                validate_structured_job_common(
                    normalized_job_cwd.as_deref(),
                    None,
                    timeout_secs,
                    STRUCTURED_EXECUTION_TIMEOUT_MAX_SECS,
                )
                .map_err(ExecutionAdmissionError::invalid_request)?;
                format!("trusted Skill resource {}", request.path)
            }
        };
        let (validation_steps, validation) = match &execution {
            JobStartExecution::Validation { plan, .. } => (
                plan.steps().iter().map(|step| step.name.clone()).collect(),
                plan.metadata().cloned(),
            ),
            _ => (Vec::new(), None),
        };
        Ok(Self {
            client_id,
            request_id,
            created_at,
            timeout_secs,
            execution,
            visibility: context.visibility,
            context: ShellJobContext {
                runtime_project_id: context.project_id,
                workflow_session_id: context.session_id,
                ssh_resource,
                project_cwd: context.project_cwd,
                cwd: normalized_job_cwd,
                purpose: context.purpose,
                shell: context.shell,
                command_preview: safe_command_preview,
                validation_steps,
                validation,
                structured_execution: None,
            },
        })
    }

    pub fn idempotency_key(&self) -> Option<&str> {
        match &self.execution {
            JobStartExecution::DetachedProcess {
                idempotency_key, ..
            } => Some(idempotency_key),
            _ => None,
        }
    }

    pub fn detached_intent(&self) -> Option<DetachedIdempotencyIntent> {
        let JobStartExecution::DetachedProcess { process, stdin, .. } = &self.execution else {
            return None;
        };
        Some(DetachedIdempotencyIntent {
            project_id: self.context.runtime_project_id.clone(),
            session_id: self.context.workflow_session_id.clone(),
            project_cwd: self.context.project_cwd.clone(),
            cwd: self.context.cwd.clone(),
            purpose: self.context.purpose.clone(),
            shell: self.context.shell.clone(),
            process: process.clone(),
            stdin: stdin.clone(),
            timeout_secs: self.timeout_secs,
        })
    }

    pub fn job_kind(&self) -> &'static str {
        match self.execution {
            JobStartExecution::Shell { .. } | JobStartExecution::Validation { .. } => "shell",
            JobStartExecution::Build(_) => "project_build",
            JobStartExecution::Format { .. } => "project_format",
            JobStartExecution::Process { .. } | JobStartExecution::InteractiveProcess(_) => {
                "run_process"
            }
            JobStartExecution::DetachedProcess { .. } => "run_detached_process",
            JobStartExecution::Script { .. } => "run_script",
            JobStartExecution::SkillResource { .. } => "run_skill_resource",
        }
    }

    pub fn into_operation(self, job_id: String) -> RunnerJobOperation {
        let Self {
            execution,
            context,
            timeout_secs,
            ..
        } = self;
        let cwd = context.cwd.clone();
        let correlation = match &execution {
            JobStartExecution::Process { correlation, .. }
            | JobStartExecution::Script { correlation, .. }
            | JobStartExecution::SkillResource { correlation, .. } => correlation.clone(),
            JobStartExecution::DetachedProcess {
                validation_identity,
                validation_tool,
                ..
            } => JobValidationCorrelation {
                validation_identity: validation_identity.clone(),
                validation_tool: validation_tool.clone(),
                assertion_name: None,
            },
            _ => JobValidationCorrelation::default(),
        };
        let mut operation = match execution {
            JobStartExecution::Shell { command, target } => {
                let (shell, login) = match target {
                    JobShellTarget::Local { shell, login } => (shell, login),
                    JobShellTarget::Ssh { .. } => (None, false),
                };
                RunnerJobOperation::StartShell(RunnerJobShellOperation {
                    job_id,
                    cwd,
                    command,
                    shell,
                    login,
                    timeout_secs,
                    context,
                })
            }
            JobStartExecution::Validation { plan, .. } => {
                RunnerJobOperation::StartValidation(RunnerJobValidationOperation {
                    job_id,
                    cwd,
                    steps: plan.into_steps(),
                    timeout_secs,
                    context,
                })
            }
            JobStartExecution::Build(plan) => {
                RunnerJobOperation::StartBuild(RunnerJobBuildOperation {
                    job_id,
                    cwd,
                    process: plan.process,
                    provenance: plan.provenance,
                    timeout_secs,
                    context,
                })
            }
            JobStartExecution::Format { plan, .. } => RunnerJobOperation::StartFormat(
                webcodex_core::runner_operation::RunnerJobFormatOperation {
                    job_id,
                    cwd,
                    plan,
                    timeout_secs,
                    context,
                },
            ),
            JobStartExecution::Process { process, stdin, .. } => {
                RunnerJobOperation::StartProcess(RunnerJobProcessOperation {
                    job_id,
                    cwd,
                    process,
                    stdin,
                    timeout_secs,
                    context,
                })
            }
            JobStartExecution::InteractiveProcess(process) => {
                RunnerJobOperation::StartInteractiveProcess(RunnerJobProcessOperation {
                    job_id,
                    cwd,
                    process,
                    stdin: None,
                    timeout_secs,
                    context,
                })
            }
            JobStartExecution::DetachedProcess { process, stdin, .. } => {
                RunnerJobOperation::StartDetachedProcess(RunnerJobProcessOperation {
                    job_id,
                    cwd,
                    process,
                    stdin,
                    timeout_secs,
                    context,
                })
            }
            JobStartExecution::Script { script, stdin, .. } => {
                RunnerJobOperation::StartScript(RunnerJobScriptOperation {
                    job_id,
                    cwd,
                    script,
                    stdin,
                    timeout_secs,
                    context,
                })
            }
            JobStartExecution::SkillResource { request, .. } => {
                RunnerJobOperation::StartSkillResource(RunnerJobSkillResourceOperation {
                    job_id,
                    cwd,
                    request,
                    timeout_secs,
                    context,
                })
            }
        };
        // The canonical operation owns execution facts; this boundary supplies
        // only the already-normalized correlation fields.
        let metadata = operation
            .expected_structured_execution()
            .map(|mut metadata| {
                metadata.validation_identity = correlation.validation_identity;
                metadata.validation_tool = correlation.validation_tool;
                metadata.assertion_name = correlation.assertion_name;
                metadata
            });
        operation
            .context_mut()
            .expect("Job start always has context")
            .structured_execution = metadata;
        operation
    }

    pub fn admission_requirements(&self) -> JobAdmissionRequirements {
        use JobStartExecution as Execution;
        let client_id = &self.client_id;
        let mut required = JobAdmissionRequirements(vec![JobAdmissionRequirement::AsyncJobs]);
        let structured = !matches!(
            self.execution,
            Execution::Shell { .. } | Execution::Validation { .. }
        );
        if let Execution::Shell {
            target: JobShellTarget::Local { shell, login },
            ..
        } = &self.execution
        {
            required.capability(shell.is_some(), RunnerFeature::ExplicitShellSelection,
                format!("capability_unavailable: runner {client_id} does not support explicit_shell_selection"));
            required.capability(
                *login,
                RunnerFeature::BashLoginShell,
                format!(
                    "capability_unavailable: runner {client_id} does not support bash_login_shell"
                ),
            );
        }
        required.capability(structured, RunnerFeature::StructuredExecutionJobs,
            format!("capability_unavailable: runner {client_id} does not support structured_execution_jobs"));
        if let Execution::Format {
            plan,
            runner_instance_id,
        } = &self.execution
        {
            // These checks are ordered: store, feature, instance fence, addon.
            required.0.push(JobAdmissionRequirement::FormatReceiptStore);
            required.upgrade(true, RunnerFeature::ProjectFormat, client_id);
            required.0.push(JobAdmissionRequirement::FormatInstance(
                runner_instance_id.clone(),
            ));
            required.upgrade(
                matches!(
                    plan.profile,
                    webcodex_core::project_format::ProjectFormatProfile::Go {}
                ),
                RunnerFeature::ProjectFormatGo,
                client_id,
            );
        }

        let build = match &self.execution {
            Execution::Build(plan) => Some(plan),
            _ => None,
        };
        let validation = self.context.validation.as_ref();
        let validation_provenance =
            validation.and_then(|metadata| metadata.project_validation.as_ref());
        let steps = match &self.execution {
            Execution::Validation { plan, .. } => plan.steps(),
            _ => &[],
        };
        // The order is observable when an older Runner lacks multiple features.
        for (needed, feature) in [
            (build.is_some_and(|plan| plan.provenance.backend == "node"), RunnerFeature::ProjectBuildNode),
            (build.is_some(), RunnerFeature::ProjectBuild),
            (build.is_some_and(|plan| plan.provenance.request.has_rust_options()), RunnerFeature::ProjectBuildRustOptions),
            (build.is_some_and(|plan| plan.provenance.backend == "python"), RunnerFeature::ProjectBuildPython),
            (build.is_some_and(|plan| plan.provenance.request.scope.as_ref().is_some_and(webcodex_core::project_build::ProjectBuildScope::selects_all_packages))
                || validation_provenance.and_then(|provenance| provenance.request.scope.as_ref()).is_some_and(webcodex_core::project_validation::ProjectValidationScope::selects_all_packages), RunnerFeature::ProjectAllPackages),
            (build.is_some_and(|plan| plan.provenance.request.scope.as_ref().is_some_and(|scope| !scope.exclude_packages.is_empty()))
                || validation_provenance.and_then(|provenance| provenance.request.scope.as_ref()).is_some_and(|scope| !scope.exclude_packages.is_empty()), RunnerFeature::ProjectWorkspaceExclusions),
            (build.is_some_and(|plan| plan.provenance.request.dependency_policy.is_some())
                || validation_provenance.is_some_and(|provenance| provenance.request.dependency_policy.is_some()), RunnerFeature::ProjectDependencyPolicy),
            (build.is_some_and(|plan| plan.provenance.request.dependency_resolution_policy.is_some())
                || validation_provenance.is_some_and(|provenance| provenance.request.dependency_resolution_policy.is_some()), RunnerFeature::ProjectDependencyResolutionOffline),
            (build.is_some_and(|plan| plan.provenance.backend == "go")
                || validation_provenance.is_some_and(|provenance| provenance.backend == "go"), RunnerFeature::ProjectGoSingleModule),
        ] {
            required.upgrade(needed, feature, client_id);
        }
        if let Execution::Script { script, .. } = &self.execution {
            let feature = match script.language {
                ShellScriptLanguage::Javascript => Some((
                    RunnerFeature::StructuredScriptJavascript,
                    "structured_script_javascript",
                )),
                ShellScriptLanguage::Typescript => Some((
                    RunnerFeature::StructuredScriptTypescript,
                    "structured_script_typescript",
                )),
                ShellScriptLanguage::Python => Some((
                    RunnerFeature::StructuredScriptPython,
                    "structured_script_python",
                )),
                _ => None,
            };
            if let Some((feature, name)) = feature {
                required.capability(
                    true,
                    feature,
                    format!("capability_unavailable: runner {client_id} does not support {name}"),
                );
            }
        }
        match &self.execution {
            Execution::SkillResource { .. } => required.capability(true, RunnerFeature::SkillResourceExecution,
                format!("capability_unavailable: runner {client_id} does not support skill_resource_execution")),
            Execution::InteractiveProcess(_) => required.capability(true, RunnerFeature::JobProcessInput,
                "capability_unavailable: upgrade target Runner for job_process_input_v1".into()),
            Execution::DetachedProcess { .. } => required.capability(true, RunnerFeature::DetachedProcessJobs,
                format!("capability_unavailable: runner {client_id} does not support detached_process_jobs")),
            _ => {},
        }
        required.capability(
            self.context.ssh_resource.is_some(),
            RunnerFeature::SshShell,
            format!("agent_capability_unavailable: runner {client_id} does not support ssh_shell"),
        );

        // Both retained adapter metadata and the actual argv steps can demand
        // capabilities. SSH validation deliberately follows this same path.
        let node_script_check = validation.is_some_and(|v| v.adapter == "node:script:check")
            || steps
                .iter()
                .any(ShellJobValidationStep::is_structured_node_check);
        let python_ruff = validation.is_some_and(|v| {
            matches!(
                v.adapter.as_str(),
                "python:ruff:check" | "python:ruff:format"
            )
        }) || steps.iter().any(ShellJobValidationStep::is_structured_ruff);
        let python_pytest = validation.is_some_and(|v| v.adapter == "python:pytest:test")
            || steps
                .iter()
                .any(ShellJobValidationStep::is_structured_pytest);
        let node_test = validation.is_some_and(|v| v.adapter == "node:tap:test")
            || steps
                .iter()
                .any(ShellJobValidationStep::is_structured_node_tap_test);
        let project_test_options =
            validation_provenance.is_some_and(|provenance| provenance.request.test.is_some());
        let go_test_filter = steps.iter().any(|step| {
            step.is_structured_go_test_json() && step.args.get(2).is_some_and(|arg| arg == "-run")
        });
        for (needed, feature) in [
            (
                validation_provenance.is_some(),
                RunnerFeature::ProjectValidation,
            ),
            (
                node_script_check,
                RunnerFeature::ProjectValidationNodeScriptCheck,
            ),
            (python_ruff, RunnerFeature::ProjectValidationPythonRuff),
            (python_pytest, RunnerFeature::ProjectValidationPythonPytest),
            (
                steps
                    .iter()
                    .any(|step| step.node_json_framework().is_some()),
                RunnerFeature::ProjectValidationNodeJson,
            ),
            (
                steps
                    .iter()
                    .any(|step| step.program == "cargo" && step.name == "lint"),
                RunnerFeature::ProjectValidationRustClippy,
            ),
            (
                validation_provenance.is_some_and(|p| {
                    p.backend == "rust"
                        && p.request.action
                            == webcodex_core::project_validation::ProjectValidationAction::Lint
                        && p.request.dependency_resolution_policy.is_some()
                }) || steps.iter().any(|step| {
                    step.program == "cargo"
                        && step.name == "lint"
                        && step.args.iter().any(|arg| arg == "--offline")
                }),
                RunnerFeature::ProjectValidationRustClippyOffline,
            ),
            (node_test, RunnerFeature::ProjectValidationNodeTap),
            (
                project_test_options || go_test_filter,
                RunnerFeature::ProjectValidationTestOptions,
            ),
        ] {
            required.upgrade(needed, feature, client_id);
        }
        for (needed, feature, diagnostic) in [
            (!steps.is_empty(), RunnerFeature::StructuredValidationArgv,
                format!("structured_validation_unavailable: runner {client_id} does not support structured argv validation jobs")),
            (validation.and_then(|metadata| metadata.minimum_tests).is_some(), RunnerFeature::StructuredCargoTestCountAssertion,
                format!("structured_cargo_test_count_assertion_unavailable: runner {client_id} does not support durable Cargo test-count assertions")),
            (validation.is_some_and(|metadata| metadata.require_tests.is_some() || metadata.no_run.is_some()), RunnerFeature::StructuredCargoTestExecutionPolicy,
                format!("structured_cargo_test_execution_policy_unavailable: runner {client_id} does not support durable Cargo validation execution-policy metadata")),
            (steps.iter().any(|step| step.program == "cargo" && step.args.first().is_some_and(|arg| arg == "test") && step.args.iter().any(|arg| arg == "--lib")), RunnerFeature::StructuredCargoTestLib,
                format!("structured_cargo_test_lib_unavailable: runner {client_id} does not support Cargo test --lib validation argv")),
            (steps.iter().any(ShellJobValidationStep::is_multi_package_cargo_check), RunnerFeature::StructuredCargoCheckPackages,
                format!("capability_unavailable: structured_cargo_check_packages_unavailable: runner {client_id} does not support repeated Cargo check package selectors")),
            (steps.iter().any(ShellJobValidationStep::is_structured_go_test_json), RunnerFeature::StructuredGoTestJson,
                format!("structured_go_test_json_unavailable: runner {client_id} does not support machine-readable Go test validation")),
            (steps.iter().any(|step| step.is_structured_go_test_json() && step.args.as_slice() != ["test", "-json", "./..."]), RunnerFeature::StructuredGoTestPackages,
                format!("structured_go_test_packages_unavailable: runner {client_id} does not support focused Go package validation argv")),
            (validation.is_some_and(|metadata| metadata.adapter == "go_test"), RunnerFeature::StructuredGoTestTool,
                format!("structured_go_test_tool_unavailable: runner {client_id} does not support the first-class go_test validation contract")),
        ] {
            required.capability(needed, feature, diagnostic);
        }
        required
    }
}

/// Ordered prerequisites are derived without touching Registry state. They are
/// checked against the current Runner under the same lock as enqueue/insert.
pub(super) struct JobAdmissionRequirements(Vec<JobAdmissionRequirement>);

enum JobAdmissionRequirement {
    AsyncJobs,
    Capability {
        feature: RunnerFeature,
        diagnostic: String,
    },
    FormatReceiptStore,
    FormatInstance(String),
}

impl JobAdmissionRequirements {
    fn capability(&mut self, needed: bool, feature: RunnerFeature, diagnostic: String) {
        if needed {
            self.0.push(JobAdmissionRequirement::Capability {
                feature,
                diagnostic,
            });
        }
    }

    fn upgrade(&mut self, needed: bool, feature: RunnerFeature, client_id: &str) {
        if needed {
            self.capability(true, feature, capability_upgrade_error(client_id, feature));
        }
    }

    pub fn check(
        &self,
        client_id: &str,
        features: &RunnerFeatureSet,
        runner_instance_id: &str,
        has_receipt_store: bool,
    ) -> Result<(), ExecutionAdmissionError> {
        for requirement in &self.0 {
            match requirement {
                JobAdmissionRequirement::AsyncJobs
                    if !(features.supports(RunnerFeature::AsyncJobs)
                        || features.supports(RunnerFeature::AsyncShellJobs)) =>
                {
                    return Err(ExecutionAdmissionError::async_jobs_unavailable(client_id));
                }
                JobAdmissionRequirement::Capability {
                    feature,
                    diagnostic,
                } if !features.supports(*feature) => {
                    return Err(ExecutionAdmissionError::capability_unavailable(
                        diagnostic.clone(),
                    ));
                }
                JobAdmissionRequirement::FormatReceiptStore if !has_receipt_store => {
                    return Err(ExecutionAdmissionError::format_execution_unavailable());
                }
                JobAdmissionRequirement::FormatInstance(expected)
                    if runner_instance_id != expected.as_str() =>
                {
                    return Err(ExecutionAdmissionError::format_runner_replaced());
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn validate_structured_job_common(
    cwd: Option<&str>,
    stdin: Option<&str>,
    timeout_secs: u64,
    timeout_max_secs: u64,
) -> Result<(), String> {
    if let Some(stdin) = stdin {
        if stdin.len() > PROCESS_STDIN_MAX_BYTES {
            return Err(format!(
                "stdin is too large; maximum is {PROCESS_STDIN_MAX_BYTES} bytes"
            ));
        }
        if stdin.contains('\0') {
            return Err("stdin cannot contain NUL bytes".to_string());
        }
    }
    if let Some(cwd) = cwd {
        if cwd.len() > PROCESS_CWD_MAX_BYTES {
            return Err(format!(
                "cwd is too long; maximum is {PROCESS_CWD_MAX_BYTES} bytes"
            ));
        }
        if cwd.contains('\0') {
            return Err("cwd cannot contain NUL bytes".to_string());
        }
    }
    if !(STRUCTURED_EXECUTION_TIMEOUT_MIN_SECS..=timeout_max_secs).contains(&timeout_secs) {
        return Err(format!(
            "timeout_secs must be between {STRUCTURED_EXECUTION_TIMEOUT_MIN_SECS} and {timeout_max_secs}"
        ));
    }
    Ok(())
}
