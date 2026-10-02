//! Completion policy may change only the published-state model. Process cleanup
//! stays with the AppState supervisor, while migration/update coordinators own
//! their durable restoration. No callback receives DesktopCore or its providers.

use super::ProcessCleanup;
use crate::error::DesktopError;
use crate::models::{
    aggregate_readiness, DesktopOperationKind, DesktopStateSnapshot, ExposureReadiness,
    ProjectReadiness, RunnerReadiness, ServerReadiness,
};
use crate::runtime_selection::RuntimeSwitchResult;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FailureOwner {
    NewProcessDelta,
    DurableCoordinator,
}

#[derive(Clone, Copy)]
enum RuntimeErrorPolicy {
    Unchanged,
    ReflectResult,
    PreserveSwitchRecovery,
}

struct CompletionPolicy {
    failure_owner: FailureOwner,
    runtime_error: RuntimeErrorPolicy,
}

impl CompletionPolicy {
    fn for_kind(kind: DesktopOperationKind) -> Self {
        use DesktopOperationKind::*;
        use FailureOwner::*;
        use RuntimeErrorPolicy::*;
        // Exhaustive: a new operation must explicitly select its completion
        // owner instead of silently inheriting generic cleanup.
        let (failure_owner, runtime_error) = match kind {
            EnvironmentMigration => (DurableCoordinator, ReflectResult),
            DesktopUpdate => (DurableCoordinator, Unchanged),
            RuntimeSwitch => (NewProcessDelta, PreserveSwitchRecovery),
            LocalSetup | RemoteSetup | RuntimeResume | RunnerRestart | EnvironmentService => {
                (NewProcessDelta, ReflectResult)
            }
            LocalProjectActivate | ProjectUnregister | QuickShareStart | QuickShareStop
            | RegularTunnelStart | RegularTunnelStop | LocalRuntimeStop | RuntimeRefresh
            | TunnelProxyUpdate | TunnelConfigUpdate | RunnerSettingsUpdate | RuntimeProbe
            | TraceUpdate | ConfigurationRestore => (NewProcessDelta, Unchanged),
        };
        Self {
            failure_owner,
            runtime_error,
        }
    }
}

pub(super) struct OperationCompletion<'a> {
    kind: DesktopOperationKind,
    error: Option<&'a DesktopError>,
    policy: CompletionPolicy,
}

impl<'a> OperationCompletion<'a> {
    pub(super) fn new(kind: DesktopOperationKind, error: Option<&'a DesktopError>) -> Self {
        Self {
            kind,
            error,
            policy: CompletionPolicy::for_kind(kind),
        }
    }

    pub(super) fn requires_process_cleanup(&self) -> bool {
        self.error.is_some() && self.policy.failure_owner == FailureOwner::NewProcessDelta
    }

    pub(super) fn apply_failure(
        &self,
        snapshot: &mut DesktopStateSnapshot,
        baseline: &DesktopStateSnapshot,
        cleanup: Option<ProcessCleanup>,
    ) -> bool {
        debug_assert_eq!(cleanup.is_some(), self.requires_process_cleanup());
        let Some(error) = self.error else {
            return false;
        };
        let cancelled = error.code == "desktop_operation_cancelled";
        if let Some(cleanup) = cleanup {
            reconcile_after_operation_failure(snapshot, self.kind, baseline, cleanup, cancelled);
        }
        // A durable coordinator has already restored the original process or
        // published an unknown-result state. Never reclaim that generation here.
        terminalize_failed_start(snapshot, cancelled);
        true
    }

    pub(super) fn apply_runtime_error(
        &self,
        snapshot: &mut DesktopStateSnapshot,
        last_switch: Option<&RuntimeSwitchResult>,
    ) -> bool {
        match self.policy.runtime_error {
            RuntimeErrorPolicy::Unchanged => return false,
            RuntimeErrorPolicy::ReflectResult | RuntimeErrorPolicy::PreserveSwitchRecovery => {}
        }
        if let Some(error) = self.error {
            snapshot.runtime_error =
                (error.code != "desktop_operation_cancelled").then(|| error.clone());
        } else {
            let preserve = matches!(
                self.policy.runtime_error,
                RuntimeErrorPolicy::PreserveSwitchRecovery
            ) && last_switch.is_some_and(|switch| {
                matches!(switch.outcome.as_str(), "rolled_back" | "recovery_required")
            });
            if !preserve {
                snapshot.runtime_error = None;
            }
        }
        true
    }
}

pub(super) fn terminalize_failed_start(snapshot: &mut DesktopStateSnapshot, cancelled: bool) {
    let server = if snapshot.readiness.server == ServerReadiness::Starting {
        if cancelled {
            ServerReadiness::Stopped
        } else {
            ServerReadiness::Error
        }
    } else {
        snapshot.readiness.server.clone()
    };
    let runner = if snapshot.readiness.runner == RunnerReadiness::Connecting {
        if cancelled {
            RunnerReadiness::Stopped
        } else {
            RunnerReadiness::Error
        }
    } else {
        snapshot.readiness.runner.clone()
    };
    let exposure = if snapshot.readiness.exposure == ExposureReadiness::Starting {
        if cancelled {
            ExposureReadiness::Disabled
        } else {
            ExposureReadiness::Error
        }
    } else {
        snapshot.readiness.exposure.clone()
    };
    snapshot.readiness =
        aggregate_readiness(server, runner, exposure, snapshot.readiness.project.clone());
}

pub(super) fn reconcile_after_operation_failure(
    snapshot: &mut DesktopStateSnapshot,
    kind: DesktopOperationKind,
    baseline: &DesktopStateSnapshot,
    cleanup: ProcessCleanup,
    cancelled: bool,
) {
    let observed_binaries = snapshot.binaries.clone();
    match kind {
        DesktopOperationKind::RuntimeResume => {
            // Resume is a desired-state replay of an already committed setup.
            // If any step fails or is cancelled, newly owned processes have
            // already been reclaimed above; restore the last published view
            // rather than leaving a synthetic "starting" state behind.
            *snapshot = baseline.clone();
        }
        DesktopOperationKind::QuickShareStart => {
            // Quick Share is intentionally ephemeral. Any failed start has
            // already stopped (or will have cleanup stop) the newly owned
            // foreground process, so return the public topology to the
            // last committed runtime instead of leaving "starting" behind.
            *snapshot = baseline.clone();
        }
        DesktopOperationKind::RegularTunnelStart if cancelled => {
            // User cancellation is not a tunnel failure. Restore the last
            // observed full-runtime state after the exact owned tunnel is
            // reclaimed rather than publishing a synthetic tunnel error.
            *snapshot = baseline.clone();
        }
        DesktopOperationKind::RuntimeRefresh if cancelled => {
            // A cancelled observation must not partially overwrite the
            // last published control-plane state.
            *snapshot = baseline.clone();
        }
        DesktopOperationKind::LocalSetup => {
            let server = if cleanup.local_server {
                ServerReadiness::Stopped
            } else if snapshot.readiness.server == ServerReadiness::Starting {
                baseline.readiness.server.clone()
            } else {
                snapshot.readiness.server.clone()
            };
            let runner = if cleanup.local_runner {
                RunnerReadiness::Stopped
            } else if snapshot.readiness.runner == RunnerReadiness::Connecting {
                baseline.readiness.runner.clone()
            } else {
                snapshot.readiness.runner.clone()
            };
            let project = if cleanup.local_server || cleanup.local_runner {
                snapshot
                    .project
                    .as_ref()
                    .map(|_| ProjectReadiness::Configured)
                    .unwrap_or(ProjectReadiness::None)
            } else {
                snapshot.readiness.project.clone()
            };
            snapshot.readiness =
                aggregate_readiness(server, runner, snapshot.readiness.exposure.clone(), project);
        }
        DesktopOperationKind::RemoteSetup => {
            let runner = if cleanup.local_runner {
                RunnerReadiness::Stopped
            } else if snapshot.readiness.runner == RunnerReadiness::Connecting {
                baseline.readiness.runner.clone()
            } else {
                snapshot.readiness.runner.clone()
            };
            let project = if cleanup.local_runner {
                snapshot
                    .project
                    .as_ref()
                    .map(|_| ProjectReadiness::Configured)
                    .unwrap_or(ProjectReadiness::None)
            } else {
                snapshot.readiness.project.clone()
            };
            snapshot.readiness = aggregate_readiness(
                snapshot.readiness.server.clone(),
                runner,
                snapshot.readiness.exposure.clone(),
                project,
            );
        }
        _ => {}
    }
    if observed_binaries.is_some() {
        snapshot.binaries = observed_binaries;
    }
}

#[cfg(test)]
mod tests;
