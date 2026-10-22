//! Validated Job starts, independent of the live Registry state.
//!
//! The public metadata bag is accepted only here. Admission and dispatch consume
//! a closed execution shape and ordered requirements, never reclassify that bag.

use super::super::jobs::{command_preview, process_preview, script_preview};
use super::super::requests::capability_upgrade_error;
use super::super::validation::validate_run_request;
use super::*;
use crate::RunnerFeatureSet;
use std::collections::HashSet;
use webcodex_core::runner_operation::{
    RunnerJobBuildOperation, RunnerJobProcessOperation, RunnerJobScriptOperation,
    RunnerJobShellOperation, RunnerJobSkillResourceOperation, RunnerJobValidationOperation,
};
use webcodex_core::runner_protocol::{
    validate_process_argv, validate_script_request, ShellJobContext, ShellRunRequest,
    ShellScriptLanguage, PROCESS_CWD_MAX_BYTES, PROCESS_STDIN_MAX_BYTES, PROCESS_TIMEOUT_MAX_SECS,
    STRUCTURED_EXECUTION_TIMEOUT_MAX_SECS, STRUCTURED_EXECUTION_TIMEOUT_MIN_SECS,
};

pub(super) struct JobStartPlan {
    pub client_id: String,
    pub request_id: String,
    pub created_at: i64,
    pub visibility: ShellJobVisibility,
    context: ShellJobContext,
    timeout_secs: u64,
    execution: JobStartExecution,
    correlation: (Option<String>, Option<String>, Option<String>),
}

enum JobStartExecution {
    Shell {
        command: String,
        shell: Option<ExecutionShell>,
        login: bool,
    },
    Validation {
        steps: Vec<ShellJobValidationStep>,
    },
    Build(webcodex_core::project_build::ProjectBuildPlan),
    Format {
        plan: webcodex_core::project_format::ProjectFormatPlan,
        runner_instance_id: String,
    },
    Process {
        process: ShellProcessArgv,
        stdin: Option<String>,
    },
    InteractiveProcess(ShellProcessArgv),
    DetachedProcess {
        process: ShellProcessArgv,
        stdin: Option<String>,
        idempotency_key: String,
    },
    Script {
        script: ShellScriptPayload,
        stdin: Option<String>,
    },
    SkillResource(RunnerSkillExecutionRequest),
}

impl JobStartPlan {
    pub fn new(
        body: ShellJobOpRequest,
        metadata: ShellJobStartMetadata,
    ) -> Result<Self, ExecutionAdmissionError> {
        let client_id = body
            .client_id
            .clone()
            .ok_or_else(|| "client_id is required for op=start".to_string())
            .map_err(ExecutionAdmissionError::invalid_request)?;
        let command = body
            .command
            .clone()
            .ok_or_else(|| "command is required for op=start".to_string())
            .map_err(ExecutionAdmissionError::invalid_request)?;
        let timeout_secs = body.timeout_secs.unwrap_or(120);
        let normalized_job_cwd = body.cwd.clone().map(|cwd| cwd.trim().to_string());
        let request_id = next_request_id();
        let created_at = now_ts();
        // Ordinary Session-scoped jobs retain their Session id even without an
        // SSH resource. Only the inverse is invalid: remote execution without
        // the Workflow Session that owns the SSH context.
        if metadata.ssh_resource.is_some() && metadata.session_id.is_none() {
            return Err(ExecutionAdmissionError::invalid_request(
                "ssh_session_required: an SSH resource requires a Workflow Session id".to_string(),
            ));
        }
        if metadata.ssh_resource.is_some() && metadata.explicit_shell.is_some() {
            return Err(ExecutionAdmissionError::invalid_request("explicit_shell_selection is local-only; named SSH shell selection must stay in the remote compatibility path"
                    .to_string()));
        }
        if metadata.ssh_resource.as_deref().is_some_and(|resource| {
            resource.is_empty()
                || resource.len() > 80
                || resource.contains("..")
                || !resource.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
                })
        }) {
            return Err(ExecutionAdmissionError::invalid_request(
                "ssh_resource_invalid: resource name is invalid".to_string(),
            ));
        }
        let validation_steps = metadata.validation_steps;
        let validation = metadata.validation;
        let validation_identity = metadata.validation_identity.clone();
        let validation_tool = metadata.validation_tool.clone();
        let assertion_name = metadata.assertion_name.clone();
        let explicit_shell = metadata.explicit_shell;
        let login = body.login;
        if login
            && (metadata.ssh_resource.is_some() || explicit_shell != Some(ExecutionShell::Bash))
        {
            return Err(ExecutionAdmissionError::invalid_request(
                "bash login mode requires local shell=bash".to_string(),
            ));
        }
        let structured_execution = metadata.structured_execution;
        let project_all_packages_request = matches!(
            structured_execution.as_ref(),
            Some(StructuredJobExecution::ProjectBuild(plan))
                if plan
                    .provenance
                    .request
                    .scope
                    .as_ref()
                    .is_some_and(webcodex_core::project_build::ProjectBuildScope::selects_all_packages)
        ) || validation
            .as_ref()
            .and_then(|metadata| metadata.project_validation.as_ref())
            .and_then(|provenance| provenance.request.scope.as_ref())
            .is_some_and(
                webcodex_core::project_validation::ProjectValidationScope::selects_all_packages,
            );
        if explicit_shell.is_some()
            && (structured_execution.is_some()
                || !validation_steps.is_empty()
                || validation.is_some())
        {
            return Err(ExecutionAdmissionError::invalid_request(
                "explicit_shell_selection is valid only for raw shell Job starts".to_string(),
            ));
        }
        let structured_stdin = metadata.stdin;
        if validation_steps.len() > 3
            || validation_steps.iter().any(|step| {
                if project_all_packages_request {
                    !step.is_canonical_project_step()
                } else {
                    !step.is_canonical()
                }
            })
            || validation_steps
                .iter()
                .map(|step| step.name.as_str())
                .collect::<HashSet<_>>()
                .len()
                != validation_steps.len()
        {
            return Err(ExecutionAdmissionError::invalid_request(
                "invalid structured validation plan".to_string(),
            ));
        }
        if validation
            .as_ref()
            .is_some_and(|metadata| !metadata.is_valid() || metadata.steps != validation_steps)
        {
            return Err(ExecutionAdmissionError::invalid_request(
                "invalid structured validation metadata".to_string(),
            ));
        }
        if structured_execution.is_some()
            && (!validation_steps.is_empty() || validation.is_some() || !command.is_empty())
        {
            return Err(ExecutionAdmissionError::invalid_request(
                "typed structured Job starts require command=\"\" and no validation plan"
                    .to_string(),
            ));
        }
        let mut detached_key = metadata.detached_idempotency_key;
        let correlation = (validation_identity, validation_tool, assertion_name);
        let (execution, safe_command_preview, correlation) = match structured_execution {
            Some(StructuredJobExecution::ProjectFormat {
                plan,
                runner_instance_id,
            }) => {
                plan.digest()
                    .map_err(str::to_string)
                    .map_err(ExecutionAdmissionError::invalid_request)?;
                if structured_stdin.is_some()
                    || metadata.ssh_resource.is_some()
                    || metadata.visibility != ShellJobVisibility::Public
                    || metadata.project_id.as_deref()
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
                (
                    JobStartExecution::Format {
                        plan,
                        runner_instance_id,
                    },
                    "project_format selected files".into(),
                    (None, None, None),
                )
            }
            Some(StructuredJobExecution::ProjectBuild(plan)) => {
                if !plan.is_valid() || structured_stdin.is_some() {
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
                let preview = process_preview(
                    &plan.process.executable,
                    plan.process.args.iter().map(String::as_str),
                );
                (JobStartExecution::Build(plan), preview, (None, None, None))
            }
            Some(StructuredJobExecution::Process(process)) => {
                validate_process_argv(&process)
                    .map_err(ExecutionAdmissionError::invalid_request)?;
                validate_structured_job_common(
                    normalized_job_cwd.as_deref(),
                    structured_stdin.as_deref(),
                    timeout_secs,
                    PROCESS_TIMEOUT_MAX_SECS,
                )
                .map_err(ExecutionAdmissionError::invalid_request)?;
                let preview =
                    process_preview(&process.executable, process.args.iter().map(String::as_str));
                (
                    JobStartExecution::Process {
                        process,
                        stdin: structured_stdin,
                    },
                    preview,
                    correlation,
                )
            }
            Some(StructuredJobExecution::InteractiveProcess(process)) => {
                validate_process_argv(&process)
                    .map_err(ExecutionAdmissionError::invalid_request)?;
                validate_structured_job_common(
                    normalized_job_cwd.as_deref(),
                    None,
                    timeout_secs,
                    PROCESS_TIMEOUT_MAX_SECS,
                )
                .map_err(ExecutionAdmissionError::invalid_request)?;
                if structured_stdin.is_some()
                    || correlation.0.is_some()
                    || correlation.1.is_some()
                    || correlation.2.is_some()
                {
                    return Err(ExecutionAdmissionError::invalid_request("interactive process requires subsequent keyed input and is not validation evidence"));
                }
                let preview = format!("interactive process ({} args)", process.args.len());
                (
                    JobStartExecution::InteractiveProcess(process),
                    preview,
                    (None, None, None),
                )
            }
            Some(StructuredJobExecution::DetachedProcess(process)) => {
                validate_process_argv(&process)
                    .map_err(ExecutionAdmissionError::invalid_request)?;
                validate_structured_job_common(
                    normalized_job_cwd.as_deref(),
                    structured_stdin.as_deref(),
                    timeout_secs,
                    PROCESS_TIMEOUT_MAX_SECS,
                )
                .map_err(ExecutionAdmissionError::invalid_request)?;
                let preview = format!("detached process ({} args)", process.args.len());
                let idempotency_key = detached_key.take().ok_or_else(|| ExecutionAdmissionError::invalid_request(
                    "detached idempotency_key must be present exactly for detached process Job starts"))?;
                (
                    JobStartExecution::DetachedProcess {
                        process,
                        stdin: structured_stdin,
                        idempotency_key,
                    },
                    preview,
                    (correlation.0, correlation.1, None),
                )
            }
            Some(StructuredJobExecution::Script(script)) => {
                validate_script_request(
                    &script,
                    structured_stdin.as_deref(),
                    normalized_job_cwd.as_deref(),
                    timeout_secs,
                )
                .map_err(ExecutionAdmissionError::invalid_request)?;
                let preview = script_preview(
                    script.language.as_str(),
                    script.script.len(),
                    script.args.len(),
                );
                (
                    JobStartExecution::Script {
                        script,
                        stdin: structured_stdin,
                    },
                    preview,
                    correlation,
                )
            }
            Some(StructuredJobExecution::SkillResource(request)) => {
                request
                    .validate()
                    .map_err(|error| format!("invalid Runner Skill execution request: {error}"))
                    .map_err(ExecutionAdmissionError::invalid_request)?;
                if structured_stdin.is_some() {
                    return Err(ExecutionAdmissionError::invalid_request(
                        "Skill resource Job does not accept generic stdin",
                    ));
                }
                validate_structured_job_common(
                    normalized_job_cwd.as_deref(),
                    None,
                    timeout_secs,
                    STRUCTURED_EXECUTION_TIMEOUT_MAX_SECS,
                )
                .map_err(ExecutionAdmissionError::invalid_request)?;
                let preview = format!("trusted Skill resource {}", request.path);
                (
                    JobStartExecution::SkillResource(request),
                    preview,
                    correlation,
                )
            }
            None => {
                let run = ShellRunRequest {
                    login: false,
                    client_id: client_id.clone(),
                    cwd: normalized_job_cwd.clone(),
                    command,
                    stdin: None,
                    timeout_secs,
                    wait_timeout_secs: 0,
                };
                validate_run_request(&run).map_err(ExecutionAdmissionError::invalid_request)?;
                if validation_steps.is_empty() {
                    let preview = command_preview(&run.command);
                    (
                        JobStartExecution::Shell {
                            command: run.command,
                            shell: explicit_shell,
                            login,
                        },
                        preview,
                        (None, None, None),
                    )
                } else {
                    let preview = format!(
                        "validation: {}",
                        validation_steps
                            .iter()
                            .map(|step| step.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    (
                        JobStartExecution::Validation {
                            steps: validation_steps,
                        },
                        preview,
                        (None, None, None),
                    )
                }
            }
        };
        if detached_key.is_some() {
            return Err(ExecutionAdmissionError::invalid_request(
                "detached idempotency_key must be present exactly for detached process Job starts",
            ));
        }
        let validation_steps = match &execution {
            JobStartExecution::Validation { steps } => {
                steps.iter().map(|step| step.name.clone()).collect()
            }
            _ => Vec::new(),
        };
        Ok(Self {
            client_id,
            request_id,
            created_at,
            timeout_secs,
            execution,
            correlation,
            visibility: metadata.visibility,
            context: ShellJobContext {
                runtime_project_id: metadata.project_id,
                workflow_session_id: metadata.session_id,
                ssh_resource: metadata.ssh_resource,
                project_cwd: metadata.project_cwd,
                cwd: normalized_job_cwd,
                purpose: metadata.purpose,
                shell: metadata.shell,
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
            JobStartExecution::SkillResource(_) => "run_skill_resource",
        }
    }

    pub fn into_operation(self, job_id: String) -> RunnerJobOperation {
        let Self {
            execution,
            context,
            timeout_secs,
            correlation,
            ..
        } = self;
        let cwd = context.cwd.clone();
        let mut operation = match execution {
            JobStartExecution::Shell {
                command,
                shell,
                login,
            } => RunnerJobOperation::StartShell(RunnerJobShellOperation {
                job_id,
                cwd,
                command,
                shell,
                login,
                timeout_secs,
                context,
            }),
            JobStartExecution::Validation { steps } => {
                RunnerJobOperation::StartValidation(RunnerJobValidationOperation {
                    job_id,
                    cwd,
                    steps,
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
            JobStartExecution::Process { process, stdin } => {
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
            JobStartExecution::Script { script, stdin } => {
                RunnerJobOperation::StartScript(RunnerJobScriptOperation {
                    job_id,
                    cwd,
                    script,
                    stdin,
                    timeout_secs,
                    context,
                })
            }
            JobStartExecution::SkillResource(request) => {
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
                metadata.validation_identity = correlation.0;
                metadata.validation_tool = correlation.1;
                metadata.assertion_name = correlation.2;
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
        if let Execution::Shell { shell, login, .. } = &self.execution {
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
            Execution::Validation { steps } => steps.as_slice(),
            _ => &[],
        };
        // The order is observable when an older Runner lacks multiple features.
        for (needed, feature) in [
            (build.is_some_and(|plan| plan.provenance.backend == "node"), RunnerFeature::ProjectBuildNode),
            (build.is_some(), RunnerFeature::ProjectBuild),
            (build.is_some_and(|plan| plan.provenance.backend == "python"), RunnerFeature::ProjectBuildPython),
            (build.is_some_and(|plan| plan.provenance.request.scope.as_ref().is_some_and(webcodex_core::project_build::ProjectBuildScope::selects_all_packages))
                || validation_provenance.and_then(|provenance| provenance.request.scope.as_ref()).is_some_and(webcodex_core::project_validation::ProjectValidationScope::selects_all_packages), RunnerFeature::ProjectAllPackages),
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
            Execution::SkillResource(_) => required.capability(true, RunnerFeature::SkillResourceExecution,
                format!("capability_unavailable: runner {client_id} does not support skill_resource_execution")),
            Execution::InteractiveProcess(_) => required.capability(true, RunnerFeature::JobProcessInput,
                "capability_unavailable: upgrade target Runner for job_process_input_v1".into()),
            Execution::DetachedProcess { .. } => required.capability(true, RunnerFeature::DetachedProcessJobs,
                format!("capability_unavailable: runner {client_id} does not support detached_process_jobs")),
            _ => {},
        }
        if structured && self.context.ssh_resource.is_some() {
            required
                .0
                .push(JobAdmissionRequirement::StructuredSshUnsupported);
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
    StructuredSshUnsupported,
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
                    return Err(ExecutionAdmissionError::capability_unavailable(format!(
                        "runner {client_id} does not support async shell jobs"
                    )));
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
                    return Err(ExecutionAdmissionError::unavailable(
                        "format_execution_unavailable",
                    ));
                }
                JobAdmissionRequirement::FormatInstance(expected)
                    if runner_instance_id != expected.as_str() =>
                {
                    return Err(ExecutionAdmissionError::target_changed(
                        "format_runner_replaced",
                    ));
                }
                JobAdmissionRequirement::StructuredSshUnsupported => {
                    return Err(ExecutionAdmissionError::invalid_request("ssh_resource_unsupported_for_request: typed structured Jobs do not support SSH resources"));
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
