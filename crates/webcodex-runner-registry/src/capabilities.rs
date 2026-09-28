use webcodex_core::runner_protocol::{self as wire, RunnerCapabilities};

/// Whether a feature could ever become a frozen protocol-generation baseline,
/// or must permanently depend on accepted Runner registration semantics.
///
/// C4 freezes `GenerationEligible` as the protocol-generation-2 baseline.
/// `RegistrationRequired` features remain governed only by accepted Runner
/// registration semantics for every generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunnerFeatureInference {
    GenerationEligible,
    RegistrationRequired,
}

// Each row owns the Server identity, wire projection, inference policy and classification.
// RunnerCapabilities remains an explicit, independently reviewed wire compatibility struct.
macro_rules! runner_features {
    ($( $variant:ident => ($wire:ident, $field:ident, $inference:ident, $computer:literal), )+) => {
        /// Canonical Server-side identity for one explicitly advertised Runner capability.
        /// A feature enters [`RunnerFeatureSet`] only through the accepted registration's
        /// wire snapshot, never through inference from generation, transport or host OS.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum RunnerFeature { $( $variant, )+ }

        const ALL_RUNNER_FEATURES: &[RunnerFeature] = &[$(RunnerFeature::$variant,)+];

        impl RunnerFeature {
            pub(crate) const fn all() -> &'static [Self] { ALL_RUNNER_FEATURES }
            pub const fn as_wire_name(self) -> &'static str {
                match self { $( Self::$variant => wire::$wire, )+ }
            }
            pub(crate) fn from_wire_name(name: &str) -> Option<Self> {
                match name { $( wire::$wire => Some(Self::$variant), )+ _ => None }
            }
            pub(crate) const fn inference(self) -> RunnerFeatureInference {
                match self { $( Self::$variant => RunnerFeatureInference::$inference, )+ }
            }
            pub(crate) const fn is_computer(self) -> bool {
                match self { $( Self::$variant => $computer, )+ }
            }
            fn advertised_by(self, capabilities: &RunnerCapabilities) -> bool {
                match self { $( Self::$variant => capabilities.$field, )+ }
            }
        }

        #[cfg(test)]
        pub(crate) fn all_enabled_capabilities() -> RunnerCapabilities {
            // No default tail: a new wire field requires an explicit registry decision.
            RunnerCapabilities { $( $field: true, )+ }
        }
    };
}

runner_features! {
    Shell => (RUNNER_CAPABILITY_SHELL, shell, RegistrationRequired, false),
    ExplicitShellSelection => (RUNNER_CAPABILITY_EXPLICIT_SHELL_SELECTION, explicit_shell_selection, RegistrationRequired, false),
    BashLoginShell => (RUNNER_CAPABILITY_BASH_LOGIN_SHELL, bash_login_shell, RegistrationRequired, false),
    FileRead => (RUNNER_CAPABILITY_FILE_READ, file_read, GenerationEligible, false),
    FileWrite => (RUNNER_CAPABILITY_FILE_WRITE, file_write, GenerationEligible, false),
    ArtifactExportChunkRead => (RUNNER_CAPABILITY_ARTIFACT_EXPORT_CHUNK_READ, artifact_export_chunk_read, GenerationEligible, false),
    ArtifactExportStreamingMetadata => (RUNNER_CAPABILITY_ARTIFACT_EXPORT_STREAMING_METADATA, artifact_export_streaming_metadata, GenerationEligible, false),
    StructuredFileDelete => (RUNNER_CAPABILITY_STRUCTURED_FILE_DELETE, structured_file_delete, GenerationEligible, false),
    ApplyTextEditOccurrence => (RUNNER_CAPABILITY_APPLY_TEXT_EDIT_OCCURRENCE, apply_text_edit_occurrence, GenerationEligible, false),
    ApplyTextEditLocalGuardWithoutSha => (RUNNER_CAPABILITY_APPLY_TEXT_EDIT_LOCAL_GUARD_WITHOUT_SHA, apply_text_edit_local_guard_without_sha, RegistrationRequired, false),
    ApplyTextEditLineScope => (RUNNER_CAPABILITY_APPLY_TEXT_EDIT_LINE_SCOPE, apply_text_edit_line_scope, RegistrationRequired, false),
    ApplyTextEditRange => (RUNNER_CAPABILITY_APPLY_TEXT_EDIT_RANGE, apply_text_edit_range, RegistrationRequired, false),
    ApplyTextEditExpectedMatchCount => (RUNNER_CAPABILITY_APPLY_TEXT_EDIT_EXPECTED_MATCH_COUNT, apply_text_edit_expected_match_count, RegistrationRequired, false),
    ApplyPatch => (RUNNER_CAPABILITY_APPLY_PATCH, apply_patch, RegistrationRequired, false),
    ApplyPatchMatchMetadata => (RUNNER_CAPABILITY_APPLY_PATCH_MATCH_METADATA, apply_patch_match_metadata, RegistrationRequired, false),
    ApplyPatchMatchingMode => (RUNNER_CAPABILITY_APPLY_PATCH_MATCHING_MODE, apply_patch_matching_mode, RegistrationRequired, false),
    Git => (RUNNER_CAPABILITY_GIT, git, RegistrationRequired, false),
    Jobs => (RUNNER_CAPABILITY_JOBS, jobs, GenerationEligible, false),
    AsyncJobs => (RUNNER_CAPABILITY_ASYNC_JOBS, async_jobs, GenerationEligible, false),
    AsyncShellJobs => (RUNNER_CAPABILITY_ASYNC_SHELL_JOBS, async_shell_jobs, GenerationEligible, false),
    SshShell => (RUNNER_CAPABILITY_SSH_SHELL, ssh_shell, RegistrationRequired, false),
    PersistentShell => (RUNNER_CAPABILITY_PERSISTENT_SHELL, persistent_shell, RegistrationRequired, false),
    SshPersistentShell => (RUNNER_CAPABILITY_SSH_PERSISTENT_SHELL, ssh_persistent_shell, RegistrationRequired, false),
    StructuredValidationArgv => (RUNNER_CAPABILITY_STRUCTURED_VALIDATION_ARGV, structured_validation_argv, GenerationEligible, false),
    StructuredCargoTestCountAssertion => (RUNNER_CAPABILITY_STRUCTURED_CARGO_TEST_COUNT_ASSERTION, structured_cargo_test_count_assertion, GenerationEligible, false),
    StructuredCargoTestExecutionPolicy => (RUNNER_CAPABILITY_STRUCTURED_CARGO_TEST_EXECUTION_POLICY, structured_cargo_test_execution_policy, RegistrationRequired, false),
    StructuredCargoTestLib => (RUNNER_CAPABILITY_STRUCTURED_CARGO_TEST_LIB, structured_cargo_test_lib, RegistrationRequired, false),
    StructuredCargoCheckPackages => (RUNNER_CAPABILITY_STRUCTURED_CARGO_CHECK_PACKAGES, structured_cargo_check_packages, RegistrationRequired, false),
    ProjectValidation => (RUNNER_CAPABILITY_PROJECT_VALIDATION, project_validation_v1, RegistrationRequired, false),
    StructuredGoTestJson => (RUNNER_CAPABILITY_STRUCTURED_GO_TEST_JSON, structured_go_test_json, GenerationEligible, false),
    StructuredGoTestTool => (RUNNER_CAPABILITY_STRUCTURED_GO_TEST_TOOL, structured_go_test_tool, GenerationEligible, false),
    StructuredGoTestPackages => (RUNNER_CAPABILITY_STRUCTURED_GO_TEST_PACKAGES, structured_go_test_packages, GenerationEligible, false),
    StructuredProcessArgv => (RUNNER_CAPABILITY_STRUCTURED_PROCESS_ARGV, structured_process_argv, GenerationEligible, false),
    StructuredScriptPayload => (RUNNER_CAPABILITY_STRUCTURED_SCRIPT_PAYLOAD, structured_script_payload, GenerationEligible, false),
    StructuredScriptJavascript => (RUNNER_CAPABILITY_STRUCTURED_SCRIPT_JAVASCRIPT, structured_script_javascript, RegistrationRequired, false),
    StructuredScriptTypescript => (RUNNER_CAPABILITY_STRUCTURED_SCRIPT_TYPESCRIPT, structured_script_typescript, RegistrationRequired, false),
    StructuredScriptPython => (RUNNER_CAPABILITY_STRUCTURED_SCRIPT_PYTHON, structured_script_python, RegistrationRequired, false),
    InternalPosixScript => (RUNNER_CAPABILITY_INTERNAL_POSIX_SCRIPT, internal_posix_script, GenerationEligible, false),
    StructuredExecutionJobs => (RUNNER_CAPABILITY_STRUCTURED_EXECUTION_JOBS, structured_execution_jobs, GenerationEligible, false),
    DetachedProcessJobs => (RUNNER_CAPABILITY_DETACHED_PROCESS_JOBS, detached_process_jobs, RegistrationRequired, false),
    LspReadOnlyNavigation => (RUNNER_CAPABILITY_LSP_READ_ONLY_NAVIGATION, lsp_read_only_navigation, GenerationEligible, false),
    LspCallHierarchy => (RUNNER_CAPABILITY_LSP_CALL_HIERARCHY, lsp_call_hierarchy, GenerationEligible, false),
    ProjectLifecycle => (RUNNER_CAPABILITY_PROJECT_LIFECYCLE, project_lifecycle, GenerationEligible, false),
    ProjectPathRegistration => (RUNNER_CAPABILITY_PROJECT_PATH_REGISTRATION, project_path_registration, GenerationEligible, false),
    ManagedWorktree => (RUNNER_CAPABILITY_MANAGED_WORKTREE, managed_worktree, RegistrationRequired, false),
    SkillRuntime => (RUNNER_CAPABILITY_SKILL_RUNTIME, skill_runtime, RegistrationRequired, false),
    SkillResourceExecution => (RUNNER_CAPABILITY_SKILL_RESOURCE_EXECUTION, skill_resource_execution, RegistrationRequired, false),
    SkillManagement => (RUNNER_CAPABILITY_SKILL_MANAGEMENT, skill_management, RegistrationRequired, false),
    BrowserObserve => (RUNNER_CAPABILITY_BROWSER_OBSERVE, browser_observe, RegistrationRequired, false),
    BrowserControl => (RUNNER_CAPABILITY_BROWSER_CONTROL, browser_control, RegistrationRequired, false),
    BrowserElementActionAdmission => (RUNNER_CAPABILITY_BROWSER_ELEMENT_ACTION_ADMISSION, browser_element_action_admission, RegistrationRequired, false),
    BrowserLaunch => (RUNNER_CAPABILITY_BROWSER_LAUNCH, browser_launch, RegistrationRequired, false),
    ComputerObserve => (RUNNER_CAPABILITY_COMPUTER_OBSERVE, computer_observe, RegistrationRequired, true),
    ComputerApplicationDiscovery => (RUNNER_CAPABILITY_COMPUTER_APPLICATION_DISCOVERY, computer_application_discovery, RegistrationRequired, true),
    ComputerApplicationLaunch => (RUNNER_CAPABILITY_COMPUTER_APPLICATION_LAUNCH, computer_application_launch, RegistrationRequired, true),
    ComputerDisplayObserve => (RUNNER_CAPABILITY_COMPUTER_DISPLAY_OBSERVE, computer_display_observe, RegistrationRequired, true),
    ComputerPointerControl => (RUNNER_CAPABILITY_COMPUTER_POINTER_CONTROL, computer_pointer_control, RegistrationRequired, true),
    ComputerClipboardRead => (RUNNER_CAPABILITY_COMPUTER_CLIPBOARD_READ, computer_clipboard_read, RegistrationRequired, true),
    ComputerClipboardWrite => (RUNNER_CAPABILITY_COMPUTER_CLIPBOARD_WRITE, computer_clipboard_write, RegistrationRequired, true),
    ComputerSnapshotRegion => (RUNNER_CAPABILITY_COMPUTER_SNAPSHOT_REGION, computer_snapshot_region, RegistrationRequired, true),
    ComputerAccessibilityObserve => (RUNNER_CAPABILITY_COMPUTER_ACCESSIBILITY_OBSERVE, computer_accessibility_observe, RegistrationRequired, true),
    ComputerElementState => (RUNNER_CAPABILITY_COMPUTER_ELEMENT_STATE, computer_element_state, RegistrationRequired, true),
    JobStateReconciliation => (RUNNER_CAPABILITY_JOB_STATE_RECONCILIATION, job_state_reconciliation, RegistrationRequired, false),
    CodingAgentRuns => (RUNNER_CAPABILITY_CODING_AGENT_RUNS, coding_agent_runs, RegistrationRequired, false),
    NativeToolPlugins => (RUNNER_CAPABILITY_NATIVE_TOOL_PLUGINS, native_tool_plugins, RegistrationRequired, false),
    ManagedSshResources => (RUNNER_CAPABILITY_MANAGED_SSH_RESOURCES, managed_ssh_resources, RegistrationRequired, false),
    RunnerConfigControl => (RUNNER_CAPABILITY_RUNNER_CONFIG_CONTROL, runner_config_control, RegistrationRequired, false),
    InstructionRuntime => (RUNNER_CAPABILITY_INSTRUCTION_RUNTIME, instruction_runtime, RegistrationRequired, false),
    ComputerControl => (RUNNER_CAPABILITY_COMPUTER_CONTROL, computer_control, RegistrationRequired, true),
    ComputerScrollToElement => (RUNNER_CAPABILITY_COMPUTER_SCROLL_TO_ELEMENT, computer_scroll_to_element, RegistrationRequired, true),
    ComputerKeyInput => (RUNNER_CAPABILITY_COMPUTER_KEY_INPUT, computer_key_input, RegistrationRequired, true),
    ComputerWindowActivate => (RUNNER_CAPABILITY_COMPUTER_WINDOW_ACTIVATE, computer_window_activate, RegistrationRequired, true),
    ComputerTextInput => (RUNNER_CAPABILITY_COMPUTER_TEXT_INPUT, computer_text_input, RegistrationRequired, true),
}

/// Canonical Server-side capability truth for one accepted Runner registration.
///
/// There is intentionally no public mutation API. Every set is rebuilt from the
/// corresponding immutable wire snapshot at registration ingress, so canonical
/// semantics cannot acquire features independently from accepted registration
/// semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerFeatureSet {
    capabilities: RunnerCapabilities,
}

impl RunnerFeatureSet {
    /// Normalize one accepted generation-2 registration into canonical feature truth.
    ///
    /// Every frozen generation-2 baseline capability must remain true in the
    /// explicit bool projection; contradictions reject registration instead of
    /// being silently inferred. RegistrationRequired features are never inferred
    /// from generation.
    pub(crate) fn try_from_registration(capabilities: &RunnerCapabilities) -> Result<Self, String> {
        for feature in RunnerFeature::all().iter().copied() {
            if feature.inference() == RunnerFeatureInference::GenerationEligible
                && !feature.advertised_by(capabilities)
            {
                return Err(format!(
                    "runner generation baseline capability mismatch: {}",
                    feature.as_wire_name()
                ));
            }
        }
        Ok(Self {
            capabilities: capabilities.clone(),
        })
    }

    #[cfg(any(test, feature = "root-test-support"))]
    pub(crate) fn from_wire_for_test(capabilities: &RunnerCapabilities) -> Self {
        Self {
            capabilities: capabilities.clone(),
        }
    }

    pub fn supports(&self, feature: RunnerFeature) -> bool {
        feature.advertised_by(&self.capabilities)
    }

    pub fn supports_wire_name(&self, capability: &str) -> bool {
        RunnerFeature::from_wire_name(capability).is_some_and(|feature| self.supports(feature))
    }

    pub fn wire_capabilities(&self) -> &RunnerCapabilities {
        &self.capabilities
    }
}
