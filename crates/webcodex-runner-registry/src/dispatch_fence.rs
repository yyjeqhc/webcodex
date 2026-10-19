//! Complete admission-time identities for requests that must not retarget at dequeue.
use super::state::RunnerRecord;
use super::RunnerFeature;
use webcodex_core::runner_operation::{RunnerFileOperation, RunnerOperation};

#[derive(Debug, Clone)]
pub(super) struct ProjectDispatchFence {
    pub(super) owner: Option<String>,
    pub(super) project_id: String,
    pub(super) cwd: String,
    // Snapshot artifact writes retain their existing placement-only contract.
    // Runtime file reads/mutations additionally bind the exact Runner process.
    pub(super) runner_instance_id: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct McpGatewayDispatchFence {
    pub(super) runner_instance_id: String,
    pub(super) provider_id: String,
    pub(super) provider_instance_id: String,
}

/// One request has one applicable identity constraint. A provider or project
/// fence cannot be partially filled, or combined with an unrelated fence.
/// Admission validates it against the existing canonical RunnerOperation before
/// mutating the queue; dequeue independently checks the current registration.
#[derive(Debug, Clone)]
pub(super) enum PendingDispatchFence {
    Unfenced,
    Project(ProjectDispatchFence),
    McpGateway(McpGatewayDispatchFence),
    RunnerInstruction { runner_instance_id: String },
    RunnerConfig { runner_instance_id: String },
    SshResource { runner_instance_id: String },
    Skill { runner_instance_id: String },
}

impl PendingDispatchFence {
    pub(super) fn validate_operation(
        &self,
        operation: &RunnerOperation,
    ) -> Result<(), &'static str> {
        let valid = match (self, operation) {
            (Self::Unfenced, operation) => !matches!(
                operation,
                RunnerOperation::McpGateway(_)
                    | RunnerOperation::RunnerInstruction(_)
                    | RunnerOperation::RunnerConfig(_)
                    | RunnerOperation::SshResource(_)
                    | RunnerOperation::Skill(_)
            ),
            (Self::Project(fence), RunnerOperation::File(_)) => {
                !fence.project_id.is_empty()
                    && !fence.cwd.is_empty()
                    && fence
                        .runner_instance_id
                        .as_ref()
                        .is_none_or(|id| !id.is_empty())
            }
            (Self::McpGateway(fence), RunnerOperation::McpGateway(operation)) => {
                !fence.runner_instance_id.is_empty()
                    && operation.provider_id() == fence.provider_id
                    && operation.provider_instance_id() == fence.provider_instance_id
            }
            (
                Self::RunnerInstruction { runner_instance_id },
                RunnerOperation::RunnerInstruction(_),
            )
            | (Self::RunnerConfig { runner_instance_id }, RunnerOperation::RunnerConfig(_))
            | (Self::SshResource { runner_instance_id }, RunnerOperation::SshResource(_))
            | (Self::Skill { runner_instance_id }, RunnerOperation::Skill(_)) => {
                !runner_instance_id.is_empty()
            }
            _ => false,
        };
        if valid {
            Ok(())
        } else {
            Err("pending dispatch fence is missing or inconsistent with the Runner operation")
        }
    }

    /// Synchronous domains share rejection handling. MCP keeps its typed waiter
    /// and response contract in polling, including defensive malformed-state checks.
    pub(super) fn synchronous_rejection(
        &self,
        operation: &RunnerOperation,
        runner: Option<&RunnerRecord>,
    ) -> Option<String> {
        if matches!(operation, RunnerOperation::McpGateway(_))
            || matches!(self, Self::McpGateway(_))
        {
            return None;
        }
        if let Err(message) = self.validate_operation(operation) {
            let code = match operation {
                RunnerOperation::Skill(_) => "stale_runner",
                RunnerOperation::File(_) => "stale_project",
                _ => "runner_replaced",
            };
            return Some(format!("{code}: {message}"));
        }
        match self {
            Self::Unfenced | Self::McpGateway(_) => None,
            Self::Project(fence) => fence.rejection(operation, runner),
            Self::Skill { runner_instance_id } => {
                let Some(runner) = runner else {
                    return Some(
                        "stale_runner: Skill target Runner disappeared before dispatch".into(),
                    );
                };
                if runner.runner_instance_id != *runner_instance_id {
                    return Some(
                        "stale_runner: Skill target Runner changed before dispatch".into(),
                    );
                }
                let required = match operation {
                    RunnerOperation::Skill(operation)
                        if operation.requires_management_capability() =>
                    {
                        RunnerFeature::SkillManagement
                    }
                    _ => RunnerFeature::SkillRuntime,
                };
                (!runner.runner_features.supports(required)).then(|| format!(
                    "skill_capability_unavailable: exact Runner no longer advertises {} before dispatch",
                    required.as_wire_name()
                ))
            }
            Self::RunnerInstruction { runner_instance_id } => control_rejection(
                runner,
                runner_instance_id,
                RunnerFeature::InstructionRuntime,
                "instruction observation",
                "capability_unavailable",
                "Runner instruction-runtime capability changed before dispatch",
            ),
            Self::RunnerConfig { runner_instance_id } => control_rejection(
                runner,
                runner_instance_id,
                RunnerFeature::RunnerConfigControl,
                "config operation",
                "capability_unavailable",
                "Runner config-control capability changed before dispatch",
            ),
            Self::SshResource { runner_instance_id } => control_rejection(
                runner,
                runner_instance_id,
                RunnerFeature::ManagedSshResources,
                "SSH resource",
                "ssh_resource_registry_unavailable",
                "Managed SSH resource capability changed before dispatch",
            ),
        }
    }
}

fn control_rejection(
    runner: Option<&RunnerRecord>,
    runner_instance_id: &str,
    required: RunnerFeature,
    subject: &str,
    capability_code: &str,
    capability_message: &str,
) -> Option<String> {
    let Some(runner) = runner else {
        return Some(format!(
            "runner_replaced: Exact Runner disappeared before {subject} dispatch"
        ));
    };
    if runner.runner_instance_id != runner_instance_id {
        return Some(format!(
            "runner_replaced: Exact Runner changed before {subject} dispatch"
        ));
    }
    (!runner.runner_features.supports(required))
        .then(|| format!("{capability_code}: {capability_message}"))
}

impl ProjectDispatchFence {
    fn rejection(
        &self,
        operation: &RunnerOperation,
        runner: Option<&RunnerRecord>,
    ) -> Option<String> {
        let stale_project = || {
            format!(
                "stale_project: target project {} is no longer registered at the resolved path",
                self.project_id
            )
        };
        let Some(runner) = runner else {
            return Some(stale_project());
        };
        if self
            .runner_instance_id
            .as_ref()
            .is_some_and(|expected| runner.runner_instance_id != *expected)
        {
            return Some("stale_runner: target Runner changed before project file dispatch".into());
        }
        if runner.owner != self.owner {
            return Some("stale_authority: target Runner owner changed before dispatch".into());
        }
        let required = match operation {
            RunnerOperation::File(RunnerFileOperation::Read(_)) => RunnerFeature::FileRead,
            _ => RunnerFeature::FileWrite,
        };
        if !runner.runner_features.supports(required) {
            return Some(format!(
                "stale_authority: target Runner no longer advertises {} before dispatch",
                required.as_wire_name()
            ));
        }
        (!runner.projects.iter().any(|project| {
            !project.disabled && project.id == self.project_id && project.path == self.cwd
        }))
        .then(stale_project)
    }
}
