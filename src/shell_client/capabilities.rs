use crate::shell_protocol::{self as wire, ShellClientCapabilities};
use std::collections::BTreeSet;

/// Canonical Server-side identity for one Runner-advertised wire capability.
///
/// These identities never infer support from protocol generation, transport,
/// host OS, or any other Server-side observation. A feature enters
/// [`RunnerFeatureSet`] only when the accepted registration explicitly
/// advertises its corresponding legacy wire boolean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum RunnerFeature {
    Shell,
    FileRead,
    FileWrite,
    ArtifactExportChunkRead,
    ArtifactExportStreamingMetadata,
    StructuredFileDelete,
    ApplyTextEditOccurrence,
    Git,
    Jobs,
    AsyncJobs,
    AsyncShellJobs,
    SshShell,
    PersistentShell,
    SshPersistentShell,
    StructuredValidationArgv,
    StructuredCargoTestCountAssertion,
    StructuredGoTestJson,
    StructuredGoTestTool,
    StructuredGoTestPackages,
    StructuredProcessArgv,
    StructuredScriptPayload,
    InternalPosixScript,
    StructuredExecutionJobs,
    DetachedProcessJobs,
    LspReadOnlyNavigation,
    LspCallHierarchy,
    SandboxInspectCommands,
    ProjectLifecycle,
    ProjectPathRegistration,
    ComputerObserve,
    ComputerApplicationDiscovery,
    ComputerApplicationLaunch,
    ComputerDisplayObserve,
    ComputerPointerControl,
    ComputerClipboardRead,
    ComputerClipboardWrite,
    ComputerSnapshotRegion,
    ComputerAccessibilityObserve,
    ComputerElementState,
    JobStateReconciliation,
    CodingAgentRuns,
    ComputerControl,
    ComputerScrollToElement,
    ComputerKeyInput,
    ComputerWindowActivate,
    ComputerTextInput,
}

const ALL_RUNNER_FEATURES: [RunnerFeature; 46] = [
    RunnerFeature::Shell,
    RunnerFeature::FileRead,
    RunnerFeature::FileWrite,
    RunnerFeature::ArtifactExportChunkRead,
    RunnerFeature::ArtifactExportStreamingMetadata,
    RunnerFeature::StructuredFileDelete,
    RunnerFeature::ApplyTextEditOccurrence,
    RunnerFeature::Git,
    RunnerFeature::Jobs,
    RunnerFeature::AsyncJobs,
    RunnerFeature::AsyncShellJobs,
    RunnerFeature::SshShell,
    RunnerFeature::PersistentShell,
    RunnerFeature::SshPersistentShell,
    RunnerFeature::StructuredValidationArgv,
    RunnerFeature::StructuredCargoTestCountAssertion,
    RunnerFeature::StructuredGoTestJson,
    RunnerFeature::StructuredGoTestTool,
    RunnerFeature::StructuredGoTestPackages,
    RunnerFeature::StructuredProcessArgv,
    RunnerFeature::StructuredScriptPayload,
    RunnerFeature::InternalPosixScript,
    RunnerFeature::StructuredExecutionJobs,
    RunnerFeature::DetachedProcessJobs,
    RunnerFeature::LspReadOnlyNavigation,
    RunnerFeature::LspCallHierarchy,
    RunnerFeature::SandboxInspectCommands,
    RunnerFeature::ProjectLifecycle,
    RunnerFeature::ProjectPathRegistration,
    RunnerFeature::ComputerObserve,
    RunnerFeature::ComputerApplicationDiscovery,
    RunnerFeature::ComputerApplicationLaunch,
    RunnerFeature::ComputerDisplayObserve,
    RunnerFeature::ComputerPointerControl,
    RunnerFeature::ComputerClipboardRead,
    RunnerFeature::ComputerClipboardWrite,
    RunnerFeature::ComputerSnapshotRegion,
    RunnerFeature::ComputerAccessibilityObserve,
    RunnerFeature::ComputerElementState,
    RunnerFeature::JobStateReconciliation,
    RunnerFeature::CodingAgentRuns,
    RunnerFeature::ComputerControl,
    RunnerFeature::ComputerScrollToElement,
    RunnerFeature::ComputerKeyInput,
    RunnerFeature::ComputerWindowActivate,
    RunnerFeature::ComputerTextInput,
];

/// Whether a feature could ever become a frozen protocol-generation baseline,
/// or must permanently depend on explicit Runner advertisement.
///
/// `GenerationEligible` is classification only. C3a defines no generation
/// baseline and grants no feature merely because a Runner uses a supported
/// protocol generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum RunnerFeatureInference {
    GenerationEligible,
    ExplicitOnly,
}

impl RunnerFeature {
    pub(crate) const fn all() -> &'static [Self] {
        &ALL_RUNNER_FEATURES
    }

    pub(crate) const fn as_wire_name(self) -> &'static str {
        match self {
            Self::Shell => wire::SHELL_CLIENT_CAPABILITY_SHELL,
            Self::FileRead => wire::SHELL_CLIENT_CAPABILITY_FILE_READ,
            Self::FileWrite => wire::SHELL_CLIENT_CAPABILITY_FILE_WRITE,
            Self::ArtifactExportChunkRead => {
                wire::SHELL_CLIENT_CAPABILITY_ARTIFACT_EXPORT_CHUNK_READ
            }
            Self::ArtifactExportStreamingMetadata => {
                wire::SHELL_CLIENT_CAPABILITY_ARTIFACT_EXPORT_STREAMING_METADATA
            }
            Self::StructuredFileDelete => wire::SHELL_CLIENT_CAPABILITY_STRUCTURED_FILE_DELETE,
            Self::ApplyTextEditOccurrence => {
                wire::SHELL_CLIENT_CAPABILITY_APPLY_TEXT_EDIT_OCCURRENCE
            }
            Self::Git => wire::SHELL_CLIENT_CAPABILITY_GIT,
            Self::Jobs => wire::SHELL_CLIENT_CAPABILITY_JOBS,
            Self::AsyncJobs => wire::SHELL_CLIENT_CAPABILITY_ASYNC_JOBS,
            Self::AsyncShellJobs => wire::SHELL_CLIENT_CAPABILITY_ASYNC_SHELL_JOBS,
            Self::SshShell => wire::SHELL_CLIENT_CAPABILITY_SSH_SHELL,
            Self::PersistentShell => wire::SHELL_CLIENT_CAPABILITY_PERSISTENT_SHELL,
            Self::SshPersistentShell => wire::SHELL_CLIENT_CAPABILITY_SSH_PERSISTENT_SHELL,
            Self::StructuredValidationArgv => {
                wire::SHELL_CLIENT_CAPABILITY_STRUCTURED_VALIDATION_ARGV
            }
            Self::StructuredCargoTestCountAssertion => {
                wire::SHELL_CLIENT_CAPABILITY_STRUCTURED_CARGO_TEST_COUNT_ASSERTION
            }
            Self::StructuredGoTestJson => wire::SHELL_CLIENT_CAPABILITY_STRUCTURED_GO_TEST_JSON,
            Self::StructuredGoTestTool => wire::SHELL_CLIENT_CAPABILITY_STRUCTURED_GO_TEST_TOOL,
            Self::StructuredGoTestPackages => {
                wire::SHELL_CLIENT_CAPABILITY_STRUCTURED_GO_TEST_PACKAGES
            }
            Self::StructuredProcessArgv => wire::SHELL_CLIENT_CAPABILITY_STRUCTURED_PROCESS_ARGV,
            Self::StructuredScriptPayload => {
                wire::SHELL_CLIENT_CAPABILITY_STRUCTURED_SCRIPT_PAYLOAD
            }
            Self::InternalPosixScript => wire::SHELL_CLIENT_CAPABILITY_INTERNAL_POSIX_SCRIPT,
            Self::StructuredExecutionJobs => {
                wire::SHELL_CLIENT_CAPABILITY_STRUCTURED_EXECUTION_JOBS
            }
            Self::DetachedProcessJobs => wire::SHELL_CLIENT_CAPABILITY_DETACHED_PROCESS_JOBS,
            Self::LspReadOnlyNavigation => wire::SHELL_CLIENT_CAPABILITY_LSP_READ_ONLY_NAVIGATION,
            Self::LspCallHierarchy => wire::SHELL_CLIENT_CAPABILITY_LSP_CALL_HIERARCHY,
            Self::SandboxInspectCommands => wire::SHELL_CLIENT_CAPABILITY_SANDBOX_INSPECT_COMMANDS,
            Self::ProjectLifecycle => wire::SHELL_CLIENT_CAPABILITY_PROJECT_LIFECYCLE,
            Self::ProjectPathRegistration => {
                wire::SHELL_CLIENT_CAPABILITY_PROJECT_PATH_REGISTRATION
            }
            Self::ComputerObserve => wire::SHELL_CLIENT_CAPABILITY_COMPUTER_OBSERVE,
            Self::ComputerApplicationDiscovery => {
                wire::SHELL_CLIENT_CAPABILITY_COMPUTER_APPLICATION_DISCOVERY
            }
            Self::ComputerApplicationLaunch => {
                wire::SHELL_CLIENT_CAPABILITY_COMPUTER_APPLICATION_LAUNCH
            }
            Self::ComputerDisplayObserve => wire::SHELL_CLIENT_CAPABILITY_COMPUTER_DISPLAY_OBSERVE,
            Self::ComputerPointerControl => wire::SHELL_CLIENT_CAPABILITY_COMPUTER_POINTER_CONTROL,
            Self::ComputerClipboardRead => wire::SHELL_CLIENT_CAPABILITY_COMPUTER_CLIPBOARD_READ,
            Self::ComputerClipboardWrite => wire::SHELL_CLIENT_CAPABILITY_COMPUTER_CLIPBOARD_WRITE,
            Self::ComputerSnapshotRegion => wire::SHELL_CLIENT_CAPABILITY_COMPUTER_SNAPSHOT_REGION,
            Self::ComputerAccessibilityObserve => {
                wire::SHELL_CLIENT_CAPABILITY_COMPUTER_ACCESSIBILITY_OBSERVE
            }
            Self::ComputerElementState => wire::SHELL_CLIENT_CAPABILITY_COMPUTER_ELEMENT_STATE,
            Self::JobStateReconciliation => wire::SHELL_CLIENT_CAPABILITY_JOB_STATE_RECONCILIATION,
            Self::CodingAgentRuns => wire::SHELL_CLIENT_CAPABILITY_CODING_AGENT_RUNS,
            Self::ComputerControl => wire::SHELL_CLIENT_CAPABILITY_COMPUTER_CONTROL,
            Self::ComputerScrollToElement => {
                wire::SHELL_CLIENT_CAPABILITY_COMPUTER_SCROLL_TO_ELEMENT
            }
            Self::ComputerKeyInput => wire::SHELL_CLIENT_CAPABILITY_COMPUTER_KEY_INPUT,
            Self::ComputerWindowActivate => wire::SHELL_CLIENT_CAPABILITY_COMPUTER_WINDOW_ACTIVATE,
            Self::ComputerTextInput => wire::SHELL_CLIENT_CAPABILITY_COMPUTER_TEXT_INPUT,
        }
    }

    pub(crate) fn from_wire_name(name: &str) -> Option<Self> {
        Some(match name {
            wire::SHELL_CLIENT_CAPABILITY_SHELL => Self::Shell,
            wire::SHELL_CLIENT_CAPABILITY_FILE_READ => Self::FileRead,
            wire::SHELL_CLIENT_CAPABILITY_FILE_WRITE => Self::FileWrite,
            wire::SHELL_CLIENT_CAPABILITY_ARTIFACT_EXPORT_CHUNK_READ => {
                Self::ArtifactExportChunkRead
            }
            wire::SHELL_CLIENT_CAPABILITY_ARTIFACT_EXPORT_STREAMING_METADATA => {
                Self::ArtifactExportStreamingMetadata
            }
            wire::SHELL_CLIENT_CAPABILITY_STRUCTURED_FILE_DELETE => Self::StructuredFileDelete,
            wire::SHELL_CLIENT_CAPABILITY_APPLY_TEXT_EDIT_OCCURRENCE => {
                Self::ApplyTextEditOccurrence
            }
            wire::SHELL_CLIENT_CAPABILITY_GIT => Self::Git,
            wire::SHELL_CLIENT_CAPABILITY_JOBS => Self::Jobs,
            wire::SHELL_CLIENT_CAPABILITY_ASYNC_JOBS => Self::AsyncJobs,
            wire::SHELL_CLIENT_CAPABILITY_ASYNC_SHELL_JOBS => Self::AsyncShellJobs,
            wire::SHELL_CLIENT_CAPABILITY_SSH_SHELL => Self::SshShell,
            wire::SHELL_CLIENT_CAPABILITY_PERSISTENT_SHELL => Self::PersistentShell,
            wire::SHELL_CLIENT_CAPABILITY_SSH_PERSISTENT_SHELL => Self::SshPersistentShell,
            wire::SHELL_CLIENT_CAPABILITY_STRUCTURED_VALIDATION_ARGV => {
                Self::StructuredValidationArgv
            }
            wire::SHELL_CLIENT_CAPABILITY_STRUCTURED_CARGO_TEST_COUNT_ASSERTION => {
                Self::StructuredCargoTestCountAssertion
            }
            wire::SHELL_CLIENT_CAPABILITY_STRUCTURED_GO_TEST_JSON => Self::StructuredGoTestJson,
            wire::SHELL_CLIENT_CAPABILITY_STRUCTURED_GO_TEST_TOOL => Self::StructuredGoTestTool,
            wire::SHELL_CLIENT_CAPABILITY_STRUCTURED_GO_TEST_PACKAGES => {
                Self::StructuredGoTestPackages
            }
            wire::SHELL_CLIENT_CAPABILITY_STRUCTURED_PROCESS_ARGV => Self::StructuredProcessArgv,
            wire::SHELL_CLIENT_CAPABILITY_STRUCTURED_SCRIPT_PAYLOAD => {
                Self::StructuredScriptPayload
            }
            wire::SHELL_CLIENT_CAPABILITY_INTERNAL_POSIX_SCRIPT => Self::InternalPosixScript,
            wire::SHELL_CLIENT_CAPABILITY_STRUCTURED_EXECUTION_JOBS => {
                Self::StructuredExecutionJobs
            }
            wire::SHELL_CLIENT_CAPABILITY_DETACHED_PROCESS_JOBS => Self::DetachedProcessJobs,
            wire::SHELL_CLIENT_CAPABILITY_LSP_READ_ONLY_NAVIGATION => Self::LspReadOnlyNavigation,
            wire::SHELL_CLIENT_CAPABILITY_LSP_CALL_HIERARCHY => Self::LspCallHierarchy,
            wire::SHELL_CLIENT_CAPABILITY_SANDBOX_INSPECT_COMMANDS => Self::SandboxInspectCommands,
            wire::SHELL_CLIENT_CAPABILITY_PROJECT_LIFECYCLE => Self::ProjectLifecycle,
            wire::SHELL_CLIENT_CAPABILITY_PROJECT_PATH_REGISTRATION => {
                Self::ProjectPathRegistration
            }
            wire::SHELL_CLIENT_CAPABILITY_COMPUTER_OBSERVE => Self::ComputerObserve,
            wire::SHELL_CLIENT_CAPABILITY_COMPUTER_APPLICATION_DISCOVERY => {
                Self::ComputerApplicationDiscovery
            }
            wire::SHELL_CLIENT_CAPABILITY_COMPUTER_APPLICATION_LAUNCH => {
                Self::ComputerApplicationLaunch
            }
            wire::SHELL_CLIENT_CAPABILITY_COMPUTER_DISPLAY_OBSERVE => Self::ComputerDisplayObserve,
            wire::SHELL_CLIENT_CAPABILITY_COMPUTER_POINTER_CONTROL => Self::ComputerPointerControl,
            wire::SHELL_CLIENT_CAPABILITY_COMPUTER_CLIPBOARD_READ => Self::ComputerClipboardRead,
            wire::SHELL_CLIENT_CAPABILITY_COMPUTER_CLIPBOARD_WRITE => Self::ComputerClipboardWrite,
            wire::SHELL_CLIENT_CAPABILITY_COMPUTER_SNAPSHOT_REGION => Self::ComputerSnapshotRegion,
            wire::SHELL_CLIENT_CAPABILITY_COMPUTER_ACCESSIBILITY_OBSERVE => {
                Self::ComputerAccessibilityObserve
            }
            wire::SHELL_CLIENT_CAPABILITY_COMPUTER_ELEMENT_STATE => Self::ComputerElementState,
            wire::SHELL_CLIENT_CAPABILITY_JOB_STATE_RECONCILIATION => Self::JobStateReconciliation,
            wire::SHELL_CLIENT_CAPABILITY_CODING_AGENT_RUNS => Self::CodingAgentRuns,
            wire::SHELL_CLIENT_CAPABILITY_COMPUTER_CONTROL => Self::ComputerControl,
            wire::SHELL_CLIENT_CAPABILITY_COMPUTER_SCROLL_TO_ELEMENT => {
                Self::ComputerScrollToElement
            }
            wire::SHELL_CLIENT_CAPABILITY_COMPUTER_KEY_INPUT => Self::ComputerKeyInput,
            wire::SHELL_CLIENT_CAPABILITY_COMPUTER_WINDOW_ACTIVATE => Self::ComputerWindowActivate,
            wire::SHELL_CLIENT_CAPABILITY_COMPUTER_TEXT_INPUT => Self::ComputerTextInput,
            _ => return None,
        })
    }

    /// Classification only; this method is deliberately not consulted by
    /// [`RunnerFeatureSet::from_wire`].
    #[allow(dead_code)]
    pub(crate) const fn inference(self) -> RunnerFeatureInference {
        match self {
            Self::FileRead
            | Self::FileWrite
            | Self::ArtifactExportChunkRead
            | Self::ArtifactExportStreamingMetadata
            | Self::StructuredFileDelete
            | Self::ApplyTextEditOccurrence
            | Self::Jobs
            | Self::AsyncJobs
            | Self::AsyncShellJobs
            | Self::StructuredValidationArgv
            | Self::StructuredCargoTestCountAssertion
            | Self::StructuredGoTestJson
            | Self::StructuredGoTestTool
            | Self::StructuredGoTestPackages
            | Self::StructuredProcessArgv
            | Self::StructuredScriptPayload
            | Self::InternalPosixScript
            | Self::StructuredExecutionJobs
            | Self::LspReadOnlyNavigation
            | Self::LspCallHierarchy
            | Self::ProjectLifecycle
            | Self::ProjectPathRegistration => RunnerFeatureInference::GenerationEligible,
            Self::Shell
            | Self::Git
            | Self::SshShell
            | Self::PersistentShell
            | Self::SshPersistentShell
            | Self::DetachedProcessJobs
            | Self::SandboxInspectCommands
            | Self::ComputerObserve
            | Self::ComputerApplicationDiscovery
            | Self::ComputerApplicationLaunch
            | Self::ComputerDisplayObserve
            | Self::ComputerPointerControl
            | Self::ComputerClipboardRead
            | Self::ComputerClipboardWrite
            | Self::ComputerSnapshotRegion
            | Self::ComputerAccessibilityObserve
            | Self::ComputerElementState
            | Self::JobStateReconciliation
            | Self::CodingAgentRuns
            | Self::ComputerControl
            | Self::ComputerScrollToElement
            | Self::ComputerKeyInput
            | Self::ComputerWindowActivate
            | Self::ComputerTextInput => RunnerFeatureInference::ExplicitOnly,
        }
    }

    fn advertised_by(self, capabilities: &ShellClientCapabilities) -> bool {
        match self {
            Self::Shell => capabilities.shell,
            Self::FileRead => capabilities.file_read,
            Self::FileWrite => capabilities.file_write,
            Self::ArtifactExportChunkRead => capabilities.artifact_export_chunk_read,
            Self::ArtifactExportStreamingMetadata => {
                capabilities.artifact_export_streaming_metadata
            }
            Self::StructuredFileDelete => capabilities.structured_file_delete,
            Self::ApplyTextEditOccurrence => capabilities.apply_text_edit_occurrence,
            Self::Git => capabilities.git,
            Self::Jobs => capabilities.jobs,
            Self::AsyncJobs => capabilities.async_jobs,
            Self::AsyncShellJobs => capabilities.async_shell_jobs,
            Self::SshShell => capabilities.ssh_shell,
            Self::PersistentShell => capabilities.persistent_shell,
            Self::SshPersistentShell => capabilities.ssh_persistent_shell,
            Self::StructuredValidationArgv => capabilities.structured_validation_argv,
            Self::StructuredCargoTestCountAssertion => {
                capabilities.structured_cargo_test_count_assertion
            }
            Self::StructuredGoTestJson => capabilities.structured_go_test_json,
            Self::StructuredGoTestTool => capabilities.structured_go_test_tool,
            Self::StructuredGoTestPackages => capabilities.structured_go_test_packages,
            Self::StructuredProcessArgv => capabilities.structured_process_argv,
            Self::StructuredScriptPayload => capabilities.structured_script_payload,
            Self::InternalPosixScript => capabilities.internal_posix_script,
            Self::StructuredExecutionJobs => capabilities.structured_execution_jobs,
            Self::DetachedProcessJobs => capabilities.detached_process_jobs,
            Self::LspReadOnlyNavigation => capabilities.lsp_read_only_navigation,
            Self::LspCallHierarchy => capabilities.lsp_call_hierarchy,
            Self::SandboxInspectCommands => capabilities.sandbox_inspect_commands,
            Self::ProjectLifecycle => capabilities.project_lifecycle,
            Self::ProjectPathRegistration => capabilities.project_path_registration,
            Self::ComputerObserve => capabilities.computer_observe,
            Self::ComputerApplicationDiscovery => capabilities.computer_application_discovery,
            Self::ComputerApplicationLaunch => capabilities.computer_application_launch,
            Self::ComputerDisplayObserve => capabilities.computer_display_observe,
            Self::ComputerPointerControl => capabilities.computer_pointer_control,
            Self::ComputerClipboardRead => capabilities.computer_clipboard_read,
            Self::ComputerClipboardWrite => capabilities.computer_clipboard_write,
            Self::ComputerSnapshotRegion => capabilities.computer_snapshot_region,
            Self::ComputerAccessibilityObserve => capabilities.computer_accessibility_observe,
            Self::ComputerElementState => capabilities.computer_element_state,
            Self::JobStateReconciliation => capabilities.job_state_reconciliation,
            Self::CodingAgentRuns => capabilities.coding_agent_runs,
            Self::ComputerControl => capabilities.computer_control,
            Self::ComputerScrollToElement => capabilities.computer_scroll_to_element,
            Self::ComputerKeyInput => capabilities.computer_key_input,
            Self::ComputerWindowActivate => capabilities.computer_window_activate,
            Self::ComputerTextInput => capabilities.computer_text_input,
        }
    }
}

/// Canonical Server-side capability truth for one accepted Runner registration.
///
/// There is intentionally no public mutation API. Every set is rebuilt from the
/// corresponding immutable wire snapshot at registration ingress, so canonical
/// semantics cannot acquire features independently from Runner advertisement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunnerFeatureSet {
    explicit_features: BTreeSet<RunnerFeature>,
}

impl RunnerFeatureSet {
    pub(crate) fn from_wire(capabilities: &ShellClientCapabilities) -> Self {
        let explicit_features = RunnerFeature::all()
            .iter()
            .copied()
            .filter(|feature| feature.advertised_by(capabilities))
            .collect();
        Self { explicit_features }
    }

    pub(crate) fn supports(&self, feature: RunnerFeature) -> bool {
        self.explicit_features.contains(&feature)
    }

    pub(crate) fn supports_wire_name(&self, capability: &str) -> bool {
        RunnerFeature::from_wire_name(capability).is_some_and(|feature| self.supports(feature))
    }
}
