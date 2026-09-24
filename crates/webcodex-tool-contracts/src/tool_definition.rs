//! Runtime tool definitions.
//!
//! This module is the central declaration point for runtime tool names,
//! model-facing visibility/spec association, manifest category, runtime metadata,
//! and Runner capability requirements. Non-runtime route metadata fallbacks remain in `metadata.rs`
//! while the registry migration proceeds in small steps.

mod agent_tasks;
mod agent_waits;
mod artifacts;
mod browser;
#[cfg(feature = "workspace-checkpoints")]
mod checkpoints;
#[cfg(feature = "experimental-code-mode")]
mod code_mode;
mod coding_agents;
mod communication;
mod computer;
mod diagnostics;
mod discovery;
mod edits;
mod files;
mod git;
mod goals;
mod hygiene;
mod jobs;
mod lsp;
mod memory;
mod patches;
mod plugins;
mod runner_config;
mod sessions;
mod skills;
mod ssh_resources;
mod testing;

use super::metadata::{
    metadata as make_tool_metadata, ToolApprovalPolicy, ToolAuthorityPolicy, ToolEffect,
    ToolIdempotency, ToolMetadata, ToolPathHint, ToolRisk, ToolSemanticContract, RUNTIME_READ,
    TOOL_PROVIDER_CONTROL,
};
#[cfg(any(test, feature = "root-test-support"))]
pub use super::tool_catalog::TOOL_MANIFEST_INTENTS;
pub use super::tool_catalog::{
    available_tool_manifest_intent_names, resolve_tool_manifest_intent, CODING_INTENT_TOOL_NAMES,
    TOOL_DISCOVERY_GROUPS, TOOL_RECOMMENDED_FLOWS,
};
#[cfg(any(test, feature = "root-test-support"))]
pub use super::tool_catalog::{
    TOOL_DISCOVERY_GROUP_CHECKPOINT, TOOL_DISCOVERY_GROUP_CLEANUP, TOOL_DISCOVERY_GROUP_EDIT,
    TOOL_DISCOVERY_GROUP_GIT, TOOL_DISCOVERY_GROUP_INSPECT, TOOL_DISCOVERY_GROUP_JOBS,
    TOOL_DISCOVERY_GROUP_PATCH, TOOL_DISCOVERY_GROUP_PROJECTS, TOOL_DISCOVERY_GROUP_REVIEW,
    TOOL_DISCOVERY_GROUP_RUNTIME, TOOL_DISCOVERY_GROUP_SHELL, TOOL_DISCOVERY_GROUP_VALIDATION,
};
#[cfg(any(test, feature = "root-test-support"))]
pub use super::tool_policy::is_known_tool_name;
pub use super::tool_policy::{
    adaptive_runtime_direct_tool_definitions, exploration_tool_names,
    is_adaptive_runtime_direct_tool, is_model_visible_tool_name, lookup_tool_definition,
    model_visible_tool_definitions, model_visible_tool_names_csv,
    runtime_tool_activity_interaction, runtime_tool_activity_semantics,
    runtime_tool_approval_policy, runtime_tool_captures_validation_output, runtime_tool_category,
    runtime_tool_effect_annotations, runtime_tool_execution_contract,
    runtime_tool_host_orchestration_hint, runtime_tool_is_change_summary_like,
    runtime_tool_is_git_like, runtime_tool_is_read_like, runtime_tool_is_shell_like,
    runtime_tool_is_write_like, runtime_tool_metadata, runtime_tool_operator_extension_family,
    runtime_tool_permission_risk, runtime_tool_requires_permission, runtime_tool_runner_capability,
    runtime_tool_session_evidence_policy, runtime_tool_session_risk_class,
};
#[cfg(any(test, feature = "root-test-support"))]
pub use super::tool_policy::{
    is_model_hidden_tool_name, known_tool_names, model_hidden_tool_names,
    runtime_tool_requires_explicit_business_session,
};
use webcodex_core::runner_protocol::{
    RUNNER_CAPABILITY_APPLY_PATCH_MATCH_METADATA, RUNNER_CAPABILITY_ASYNC_JOBS,
    RUNNER_CAPABILITY_ASYNC_SHELL_JOBS, RUNNER_CAPABILITY_CODING_AGENT_RUNS,
    RUNNER_CAPABILITY_COMPUTER_ACCESSIBILITY_OBSERVE,
    RUNNER_CAPABILITY_COMPUTER_APPLICATION_DISCOVERY,
    RUNNER_CAPABILITY_COMPUTER_APPLICATION_LAUNCH, RUNNER_CAPABILITY_COMPUTER_CLIPBOARD_READ,
    RUNNER_CAPABILITY_COMPUTER_CLIPBOARD_WRITE, RUNNER_CAPABILITY_COMPUTER_CONTROL,
    RUNNER_CAPABILITY_COMPUTER_DISPLAY_OBSERVE, RUNNER_CAPABILITY_COMPUTER_ELEMENT_STATE,
    RUNNER_CAPABILITY_COMPUTER_KEY_INPUT, RUNNER_CAPABILITY_COMPUTER_OBSERVE,
    RUNNER_CAPABILITY_COMPUTER_POINTER_CONTROL, RUNNER_CAPABILITY_COMPUTER_SCROLL_TO_ELEMENT,
    RUNNER_CAPABILITY_COMPUTER_TEXT_INPUT, RUNNER_CAPABILITY_COMPUTER_WINDOW_ACTIVATE,
    RUNNER_CAPABILITY_DETACHED_PROCESS_JOBS, RUNNER_CAPABILITY_FILE_READ,
    RUNNER_CAPABILITY_FILE_WRITE, RUNNER_CAPABILITY_GIT, RUNNER_CAPABILITY_INTERNAL_POSIX_SCRIPT,
    RUNNER_CAPABILITY_LSP_CALL_HIERARCHY, RUNNER_CAPABILITY_LSP_READ_ONLY_NAVIGATION,
    RUNNER_CAPABILITY_PERSISTENT_SHELL, RUNNER_CAPABILITY_RUNNER_CONFIG_CONTROL,
    RUNNER_CAPABILITY_SHELL, RUNNER_CAPABILITY_SKILL_MANAGEMENT,
    RUNNER_CAPABILITY_SKILL_RESOURCE_EXECUTION, RUNNER_CAPABILITY_STRUCTURED_PROCESS_ARGV,
    RUNNER_CAPABILITY_STRUCTURED_SCRIPT_PAYLOAD,
};

/// Runner capability or owner-boundary requirement that must hold before a
/// Runner-backed tool can dispatch to its Project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerCapabilityRequirement {
    /// Project-scoped native tools that do not require a Runner capability but
    /// still need the Runner owner boundary when the Project is Runner-backed.
    OwnerOnly,
    /// `run_shell`, `apply_unified_diff` (Runner path runs `git apply` via shell).
    Shell,
    /// General native process + argv execution. This must never be inferred
    /// from shell or structured-validation support.
    StructuredProcess,
    /// Runner-owned trusted Skill resource execution with package identity.
    /// Never infer this from generic structured process or Skill read support.
    SkillResourceExecution,
    /// Durable detached native process Jobs. This explicit authority is never
    /// inferred from ordinary structured process execution.
    DetachedProcess,
    /// Bounded typed script payload execution. Never inferred from raw shell
    /// or either structured argv capability.
    StructuredScript,
    /// Dedicated Server-generated POSIX script runtime. Never infer this from
    /// shell/git support: older mixed-version Runners may advertise those while
    /// lacking the typed internal request kind.
    InternalPosixScript,
    /// `read_file` (Runner path uses the file_read request kind).
    FileRead,
    /// Native file mutation requests handled by the Runner.
    FileWrite,
    /// Runner-authoritative Codex Patch parsing plus transactional file mutation.
    /// This additive request kind is never inferred from generic file-write support.
    ApplyPatch,
    /// `git_status` / `git_diff` (Runner path runs git via shell; accept either
    /// an explicit `git` capability or `shell`).
    GitOrShell,
    /// `run_job` (Runner path starts an async job).
    AsyncJobs,
    /// Explicit process-local Workflow Session persistent shells.
    PersistentShell,
    /// Native read-only desktop/window observation on the exact target Runner.
    ComputerObserve,
    /// Native bounded installed-application discovery on the exact target Runner.
    ComputerApplicationDiscovery,
    /// Native launch of one exact fresh opaque application handle.
    ComputerApplicationLaunch,
    /// Native exact full-display discovery and snapshot observation.
    ComputerDisplayObserve,
    /// Native bounded global clipboard Unicode-text observation.
    ComputerClipboardRead,
    /// Native bounded global clipboard Unicode-text replacement.
    ComputerClipboardWrite,
    /// Snapshot-fenced exact coordinate pointer control on the exact Runner.
    ComputerPointerControl,
    /// Native read-only semantic accessibility inspection on the exact Runner.
    ComputerAccessibilityObserve,
    /// Native read-only normalized state for one exact observed element.
    ComputerElementState,
    /// Native bounded accessibility control on the exact target Runner.
    ComputerControl,
    /// Native semantic scroll-to-visible on one exact observed Accessibility element.
    ComputerScrollToElement,
    /// Native closed-vocabulary key input on one exact already-focused window.
    ComputerKeyInput,
    /// Native activation/raise of one exact previously observed window.
    ComputerWindowActivate,
    /// Native bounded Accessibility text input on the exact target Runner.
    ComputerTextInput,
    /// Read-only Runner-side semantic navigation through constrained LSP profiles.
    LspReadOnlyNavigation,
    /// Bounded typed call-hierarchy traversal; never inferred from navigation.
    LspCallHierarchy,
    /// Runner-owned delegated ACP coding-agent execution. Never inferred from
    /// shell, Job, MCP, or file-write capability.
    CodingAgentRuns,
    /// Exact Runner process-local first-class config check/reload. Never inferred
    /// from SIGHUP, generic shell execution, Plugin support, or protocol generation.
    RunnerConfigControl,
    /// Runner-global managed Skill lifecycle and revision management.
    SkillManagement,
}

impl RunnerCapabilityRequirement {
    pub fn label(self) -> &'static str {
        match self {
            Self::OwnerOnly => "owner boundary",
            Self::Shell => RUNNER_CAPABILITY_SHELL,
            Self::StructuredProcess => RUNNER_CAPABILITY_STRUCTURED_PROCESS_ARGV,
            Self::SkillResourceExecution => RUNNER_CAPABILITY_SKILL_RESOURCE_EXECUTION,
            Self::DetachedProcess => RUNNER_CAPABILITY_DETACHED_PROCESS_JOBS,
            Self::StructuredScript => RUNNER_CAPABILITY_STRUCTURED_SCRIPT_PAYLOAD,
            Self::InternalPosixScript => RUNNER_CAPABILITY_INTERNAL_POSIX_SCRIPT,
            Self::FileRead => RUNNER_CAPABILITY_FILE_READ,
            Self::FileWrite => RUNNER_CAPABILITY_FILE_WRITE,
            Self::ApplyPatch => RUNNER_CAPABILITY_APPLY_PATCH_MATCH_METADATA,
            Self::GitOrShell => "shell or git",
            Self::AsyncJobs => "async shell jobs",
            Self::PersistentShell => RUNNER_CAPABILITY_PERSISTENT_SHELL,
            Self::ComputerObserve => RUNNER_CAPABILITY_COMPUTER_OBSERVE,
            Self::ComputerApplicationDiscovery => RUNNER_CAPABILITY_COMPUTER_APPLICATION_DISCOVERY,
            Self::ComputerApplicationLaunch => RUNNER_CAPABILITY_COMPUTER_APPLICATION_LAUNCH,
            Self::ComputerDisplayObserve => RUNNER_CAPABILITY_COMPUTER_DISPLAY_OBSERVE,
            Self::ComputerClipboardRead => RUNNER_CAPABILITY_COMPUTER_CLIPBOARD_READ,
            Self::ComputerClipboardWrite => RUNNER_CAPABILITY_COMPUTER_CLIPBOARD_WRITE,
            Self::ComputerPointerControl => RUNNER_CAPABILITY_COMPUTER_POINTER_CONTROL,
            Self::ComputerAccessibilityObserve => RUNNER_CAPABILITY_COMPUTER_ACCESSIBILITY_OBSERVE,
            Self::ComputerElementState => RUNNER_CAPABILITY_COMPUTER_ELEMENT_STATE,
            Self::ComputerControl => RUNNER_CAPABILITY_COMPUTER_CONTROL,
            Self::ComputerScrollToElement => RUNNER_CAPABILITY_COMPUTER_SCROLL_TO_ELEMENT,
            Self::ComputerKeyInput => RUNNER_CAPABILITY_COMPUTER_KEY_INPUT,
            Self::ComputerWindowActivate => RUNNER_CAPABILITY_COMPUTER_WINDOW_ACTIVATE,
            Self::ComputerTextInput => RUNNER_CAPABILITY_COMPUTER_TEXT_INPUT,
            Self::LspReadOnlyNavigation => RUNNER_CAPABILITY_LSP_READ_ONLY_NAVIGATION,
            Self::LspCallHierarchy => RUNNER_CAPABILITY_LSP_CALL_HIERARCHY,
            Self::CodingAgentRuns => RUNNER_CAPABILITY_CODING_AGENT_RUNS,
            Self::RunnerConfigControl => RUNNER_CAPABILITY_RUNNER_CONFIG_CONTROL,
            Self::SkillManagement => RUNNER_CAPABILITY_SKILL_MANAGEMENT,
        }
    }

    pub fn registry_capabilities(self) -> &'static [&'static str] {
        match self {
            Self::OwnerOnly => &[],
            Self::Shell => &[RUNNER_CAPABILITY_SHELL],
            Self::StructuredProcess => &[RUNNER_CAPABILITY_STRUCTURED_PROCESS_ARGV],
            Self::SkillResourceExecution => &[RUNNER_CAPABILITY_SKILL_RESOURCE_EXECUTION],
            Self::DetachedProcess => &[RUNNER_CAPABILITY_DETACHED_PROCESS_JOBS],
            Self::StructuredScript => &[RUNNER_CAPABILITY_STRUCTURED_SCRIPT_PAYLOAD],
            Self::InternalPosixScript => &[RUNNER_CAPABILITY_INTERNAL_POSIX_SCRIPT],
            Self::FileRead => &[RUNNER_CAPABILITY_FILE_READ],
            Self::FileWrite => &[RUNNER_CAPABILITY_FILE_WRITE],
            Self::ApplyPatch => &[RUNNER_CAPABILITY_APPLY_PATCH_MATCH_METADATA],
            Self::GitOrShell => &[RUNNER_CAPABILITY_SHELL, RUNNER_CAPABILITY_GIT],
            Self::AsyncJobs => &[
                RUNNER_CAPABILITY_ASYNC_JOBS,
                RUNNER_CAPABILITY_ASYNC_SHELL_JOBS,
            ],
            Self::PersistentShell => &[RUNNER_CAPABILITY_PERSISTENT_SHELL],
            Self::ComputerObserve => &[RUNNER_CAPABILITY_COMPUTER_OBSERVE],
            Self::ComputerApplicationDiscovery => {
                &[RUNNER_CAPABILITY_COMPUTER_APPLICATION_DISCOVERY]
            }
            Self::ComputerApplicationLaunch => &[RUNNER_CAPABILITY_COMPUTER_APPLICATION_LAUNCH],
            Self::ComputerDisplayObserve => &[RUNNER_CAPABILITY_COMPUTER_DISPLAY_OBSERVE],
            Self::ComputerClipboardRead => &[RUNNER_CAPABILITY_COMPUTER_CLIPBOARD_READ],
            Self::ComputerClipboardWrite => &[RUNNER_CAPABILITY_COMPUTER_CLIPBOARD_WRITE],
            Self::ComputerPointerControl => &[RUNNER_CAPABILITY_COMPUTER_POINTER_CONTROL],
            Self::ComputerAccessibilityObserve => {
                &[RUNNER_CAPABILITY_COMPUTER_ACCESSIBILITY_OBSERVE]
            }
            Self::ComputerElementState => &[RUNNER_CAPABILITY_COMPUTER_ELEMENT_STATE],
            Self::ComputerControl => &[RUNNER_CAPABILITY_COMPUTER_CONTROL],
            Self::ComputerScrollToElement => &[RUNNER_CAPABILITY_COMPUTER_SCROLL_TO_ELEMENT],
            Self::ComputerKeyInput => &[RUNNER_CAPABILITY_COMPUTER_KEY_INPUT],
            Self::ComputerWindowActivate => &[RUNNER_CAPABILITY_COMPUTER_WINDOW_ACTIVATE],
            Self::ComputerTextInput => &[RUNNER_CAPABILITY_COMPUTER_TEXT_INPUT],
            Self::LspReadOnlyNavigation => &[RUNNER_CAPABILITY_LSP_READ_ONLY_NAVIGATION],
            Self::LspCallHierarchy => &[RUNNER_CAPABILITY_LSP_CALL_HIERARCHY],
            Self::CodingAgentRuns => &[RUNNER_CAPABILITY_CODING_AGENT_RUNS],
            Self::RunnerConfigControl => &[RUNNER_CAPABILITY_RUNNER_CONFIG_CONTROL],
            Self::SkillManagement => &[RUNNER_CAPABILITY_SKILL_MANAGEMENT],
        }
    }

    pub fn is_owner_only(self) -> bool {
        matches!(self, Self::OwnerOnly)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolVisibility {
    /// The tool is offered to the model via `tools/list` and the manifest.
    ModelVisible,
    /// The tool is dispatched normally but withheld from the model-facing
    /// surface. Used for compatibility, duplicate-granularity, and management
    /// tools that the canonical coding surface already covers. Internal
    /// callers (CLI, tests, back-compat dispatch) keep working.
    ModelHidden,
}

impl ToolVisibility {
    #[cfg(any(test, feature = "root-test-support"))]
    pub fn is_model_hidden(self) -> bool {
        matches!(self, Self::ModelHidden)
    }

    pub fn is_model_visible(self) -> bool {
        matches!(self, Self::ModelVisible)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ToolModelSpecDeclaration {
    pub description: &'static str,
    /// Optional GPT Actions presentation copy. Canonical/MCP descriptions stay
    /// unchanged; this exists only when the Action importer's 300-character
    /// operation-description ceiling needs a deliberately shorter rendering.
    pub gpt_action_description: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolGptActionExposure {
    /// Follow the canonical Adaptive Runtime surface automatically.
    Inherit,
    /// Remain available through the GPT Actions gateway but do not consume a
    /// dedicated OpenAPI operation. Used only for concrete surface-budget needs.
    GatewayOnly,
    /// This tool depends on MCP-only protocol semantics and must not be exposed
    /// directly or through the GPT Actions gateway.
    Unsupported,
}

/// Declarative privacy contract for the bounded Tool Audit / Session-ledger
/// projection. Tool identity lives in `ToolDefinition`; audit code consumes
/// this policy and must never infer a missing policy from the raw tool name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolAuditPolicy {
    pub request: ToolAuditRequestPolicy,
    pub result: ToolAuditResultPolicy,
    /// Final Workflow Session input projection. Runtime request auditing remains
    /// the authoritative typed boundary; this is defense-in-depth for direct
    /// SessionStore callers and persisted restore sanitization.
    pub session_input: ToolAuditSessionInputPolicy,
    /// Bounded result facts allowed to contribute to durable Session context.
    pub context: ToolAuditContextPolicy,
    /// Bounded execution stdout/stderr evidence eligibility and shape.
    pub execution: ToolAuditExecutionPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolAuditSessionInputPolicy {
    /// Apply only the generic bounded/redacted Session value projection. This is
    /// safe for an already-audited typed request and preserves historical shapes.
    Bounded,
    /// Remove explicit top-level fields if an internal caller bypasses the typed
    /// ToolCall audit boundary.
    OmitTopLevel(&'static [&'static str]),
    /// Remove nested search patterns while retaining bounded query metadata.
    SearchProjectTexts,
    /// Preserve the historical single-query omission while redacting nested
    /// patterns from batched compound search queries.
    SearchAndRead,
    /// Remove opaque Job observation tokens from nested items.
    ObserveJobs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolAuditContextPolicy {
    Omit,
    /// Reuse this ToolDefinition's already-declared bounded result projection.
    /// This is valid only for a non-canonical result policy.
    ResultProjection,
    Fields(&'static [ToolAuditResultField]),
    /// Preserve the historical bounded porcelain-derived working-tree summary.
    WorkingTreeStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolAuditExecutionDetail {
    Omit,
    Text,
    TestCounts,
    TestAssertions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolAuditExecutionShell {
    /// Use the bounded shell/executor metadata already present in the result.
    Output,
    /// Structured argv execution has no shell; record the established marker.
    DirectArgv,
    /// Structured script execution uses the bounded language as shell identity.
    ScriptLanguage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolAuditExecutionPolicy {
    pub detail: ToolAuditExecutionDetail,
    pub shell: ToolAuditExecutionShell,
}

impl ToolAuditExecutionPolicy {
    pub const OMIT: Self = Self {
        detail: ToolAuditExecutionDetail::Omit,
        shell: ToolAuditExecutionShell::Output,
    };
    pub const TEXT: Self = Self {
        detail: ToolAuditExecutionDetail::Text,
        shell: ToolAuditExecutionShell::Output,
    };
    pub const TEST_COUNTS: Self = Self {
        detail: ToolAuditExecutionDetail::TestCounts,
        shell: ToolAuditExecutionShell::Output,
    };
    pub const TEST_ASSERTIONS: Self = Self {
        detail: ToolAuditExecutionDetail::TestAssertions,
        shell: ToolAuditExecutionShell::Output,
    };
    pub const DIRECT_ARGV_TEST_COUNTS: Self = Self {
        detail: ToolAuditExecutionDetail::TestCounts,
        shell: ToolAuditExecutionShell::DirectArgv,
    };
    pub const SCRIPT_TEST_COUNTS: Self = Self {
        detail: ToolAuditExecutionDetail::TestCounts,
        shell: ToolAuditExecutionShell::ScriptLanguage,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolAuditRequestPolicy {
    /// Parse the concrete request into the canonical typed `ToolCall` and use
    /// its bounded audit projection. Parse/projection failure is fail-closed.
    Typed,
    /// Use the same typed projection, then omit null-valued audit fields. This
    /// preserves legacy validity-bit + optional-normalized-value contracts
    /// without teaching the projector which tool emitted those fields.
    TypedDropNullValues,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolAuditResultPolicy {
    /// Preserve the established canonical result as audit evidence for tools
    /// whose result contract is intentionally audit-safe. This is an explicit
    /// declaration, never the fallback for an unknown or missing policy.
    CanonicalLedgerEvidence,
    /// Project only the declared bounded fields. Missing inputs become null so
    /// the persisted shape stays stable without admitting undeclared content.
    Fields(&'static [ToolAuditResultField]),
    /// A genuinely semantic projection that cannot be expressed as independent
    /// field selectors.
    Semantic(ToolAuditSemanticResultPolicy),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolAuditResultField {
    Value {
        output: &'static str,
        source: &'static str,
    },
    Pointer {
        output: &'static str,
        pointer: &'static str,
    },
    ArrayLen {
        output: &'static str,
        source: &'static str,
    },
    PointerArrayLen {
        output: &'static str,
        pointer: &'static str,
    },
    StringBytes {
        output: &'static str,
        source: &'static str,
    },
    Presence {
        output: &'static str,
        source: &'static str,
    },
    StringPresent {
        output: &'static str,
        source: &'static str,
    },
    PointerNonNull {
        output: &'static str,
        pointer: &'static str,
    },
}

impl ToolAuditResultField {
    pub const fn value(key: &'static str) -> Self {
        Self::Value {
            output: key,
            source: key,
        }
    }

    pub const fn renamed_value(output: &'static str, source: &'static str) -> Self {
        Self::Value { output, source }
    }

    pub const fn pointer(output: &'static str, pointer: &'static str) -> Self {
        Self::Pointer { output, pointer }
    }

    pub const fn array_len(output: &'static str, source: &'static str) -> Self {
        Self::ArrayLen { output, source }
    }

    pub const fn pointer_array_len(output: &'static str, pointer: &'static str) -> Self {
        Self::PointerArrayLen { output, pointer }
    }

    pub const fn string_bytes(output: &'static str, source: &'static str) -> Self {
        Self::StringBytes { output, source }
    }

    pub const fn presence(output: &'static str, source: &'static str) -> Self {
        Self::Presence { output, source }
    }

    pub const fn string_present(output: &'static str, source: &'static str) -> Self {
        Self::StringPresent { output, source }
    }

    pub const fn pointer_non_null(output: &'static str, pointer: &'static str) -> Self {
        Self::PointerNonNull { output, pointer }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolAuditSemanticResultPolicy {
    /// Project heterogeneous Computer observation results into the same sparse,
    /// privacy-preserving metadata retained by the pre-gateway read tools.
    BrowserObservation,
    BrowserControl,
    ComputerObservation,
    /// Project heterogeneous Computer control results into the sparse lifecycle
    /// metadata retained by the pre-gateway effect tools.
    ComputerControl,
    /// Summarize coding-agent event kinds and body byte counts without retaining
    /// any event body or provider message content.
    CodingAgentObservation,
}

impl ToolAuditPolicy {
    /// Existing audit-safe behavior for tools without a narrower result
    /// projection. Every ToolDefinition must opt in explicitly through `def(...)`;
    /// there is intentionally no implicit/missing-policy default.
    pub const TYPED_CANONICAL: Self = Self {
        request: ToolAuditRequestPolicy::Typed,
        result: ToolAuditResultPolicy::CanonicalLedgerEvidence,
        session_input: ToolAuditSessionInputPolicy::Bounded,
        context: ToolAuditContextPolicy::Omit,
        execution: ToolAuditExecutionPolicy::OMIT,
    };

    pub const fn typed_fields(fields: &'static [ToolAuditResultField]) -> Self {
        Self {
            request: ToolAuditRequestPolicy::Typed,
            result: ToolAuditResultPolicy::Fields(fields),
            session_input: ToolAuditSessionInputPolicy::Bounded,
            context: ToolAuditContextPolicy::Omit,
            execution: ToolAuditExecutionPolicy::OMIT,
        }
    }

    pub const fn drop_null_request_values(mut self) -> Self {
        self.request = ToolAuditRequestPolicy::TypedDropNullValues;
        self
    }

    pub const fn typed_semantic(result: ToolAuditSemanticResultPolicy) -> Self {
        Self {
            request: ToolAuditRequestPolicy::Typed,
            result: ToolAuditResultPolicy::Semantic(result),
            session_input: ToolAuditSessionInputPolicy::Bounded,
            context: ToolAuditContextPolicy::Omit,
            execution: ToolAuditExecutionPolicy::OMIT,
        }
    }

    pub const fn session_input(mut self, policy: ToolAuditSessionInputPolicy) -> Self {
        self.session_input = policy;
        self
    }

    pub const fn context(mut self, policy: ToolAuditContextPolicy) -> Self {
        self.context = policy;
        self
    }

    pub const fn context_from_result(self) -> Self {
        self.context(ToolAuditContextPolicy::ResultProjection)
    }

    pub const fn execution(mut self, policy: ToolAuditExecutionPolicy) -> Self {
        self.execution = policy;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolNavigationEvidenceKind {
    DocumentSymbols,
    DocumentDiagnostics,
    Hover,
    WorkspaceSymbols,
    Locations,
    CallHierarchy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolExplorationEvidence {
    None,
    Read,
    ReadBatch,
    Search,
    SearchBatch,
    /// Successful search records nested under `search` in compound inspection.
    SearchCompound,
    Navigation(ToolNavigationEvidenceKind),
}

impl ToolExplorationEvidence {
    pub const fn is_exploration(self) -> bool {
        !matches!(self, Self::None)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolChangedPathEvidence {
    None,
    ResultField(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistentShellEvidenceAction {
    Open,
    Exec,
    Status,
    Close,
}

impl PersistentShellEvidenceAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Exec => "exec",
            Self::Status => "status",
            Self::Close => "close",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolDiffReviewEvidence {
    None,
    Always,
    ArgumentBool(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolReviewEvidence {
    None,
    ReadOnlyInspection,
    Search,
    DiffReview,
    WorkspaceReview,
    HygieneReview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolFailureEvidence {
    Default,
    ProvenNoStateChangeNonActionable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolSessionLifecycleEffect {
    None,
    Mutation,
    IdempotentClose,
}

pub use webcodex_core::validation_identity::ToolValidationIdentityKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolSessionEvidencePolicy {
    pub exploration: ToolExplorationEvidence,
    pub changed_paths: ToolChangedPathEvidence,
    pub persistent_shell: Option<PersistentShellEvidenceAction>,
    pub diff_review: ToolDiffReviewEvidence,
    pub review: ToolReviewEvidence,
    pub failure: ToolFailureEvidence,
    pub lifecycle: ToolSessionLifecycleEffect,
    pub validation_identity: ToolValidationIdentityKind,
}

impl ToolSessionEvidencePolicy {
    pub const NONE: Self = Self {
        exploration: ToolExplorationEvidence::None,
        changed_paths: ToolChangedPathEvidence::None,
        persistent_shell: None,
        diff_review: ToolDiffReviewEvidence::None,
        review: ToolReviewEvidence::None,
        failure: ToolFailureEvidence::Default,
        lifecycle: ToolSessionLifecycleEffect::None,
        validation_identity: ToolValidationIdentityKind::None,
    };

    pub const fn exploration(mut self, evidence: ToolExplorationEvidence) -> Self {
        self.exploration = evidence;
        self
    }

    pub const fn changed_paths(mut self, evidence: ToolChangedPathEvidence) -> Self {
        self.changed_paths = evidence;
        self
    }

    pub const fn persistent_shell(mut self, action: PersistentShellEvidenceAction) -> Self {
        self.persistent_shell = Some(action);
        self
    }

    pub const fn diff_review(mut self, evidence: ToolDiffReviewEvidence) -> Self {
        self.diff_review = evidence;
        self
    }

    pub const fn review(mut self, evidence: ToolReviewEvidence) -> Self {
        self.review = evidence;
        self
    }

    pub const fn failure(mut self, evidence: ToolFailureEvidence) -> Self {
        self.failure = evidence;
        self
    }

    pub const fn lifecycle(mut self, effect: ToolSessionLifecycleEffect) -> Self {
        self.lifecycle = effect;
        self
    }

    pub const fn validation_identity(mut self, kind: ToolValidationIdentityKind) -> Self {
        self.validation_identity = kind;
        self
    }
}

/// Closed model-selection vocabulary for ordinary execution primitives. These
/// fields describe how a model should select and continue an execution tool;
/// they do not grant authority, change permissions, or redefine runtime state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolExecutionForm {
    NativeArgv,
    TypedScript,
    ShellCommand,
    StructuredValidation,
    PersistentShellCommand,
}

impl ToolExecutionForm {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NativeArgv => "native_argv",
            Self::TypedScript => "typed_script",
            Self::ShellCommand => "shell_command",
            Self::StructuredValidation => "structured_validation",
            Self::PersistentShellCommand => "persistent_shell_command",
        }
    }
}

/// Runtime carrier responsible for the accepted execution lifetime. This is
/// deliberately unrelated to resource ownership, principal identity, or auth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolExecutionLifetime {
    Runner,
    Supervisor,
    SessionShell,
}

impl ToolExecutionLifetime {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Runner => "runner",
            Self::Supervisor => "supervisor",
            Self::SessionShell => "session_shell",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolExecutionStart {
    SyncFirst,
    AsyncImmediate,
    ExistingSession,
}

impl ToolExecutionStart {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SyncFirst => "sync_first",
            Self::AsyncImmediate => "async_immediate",
            Self::ExistingSession => "existing_session",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolExecutionContinuation {
    ObserveJobs,
    SessionShell,
    None,
}

impl ToolExecutionContinuation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ObserveJobs => "observe_jobs",
            Self::SessionShell => "session_shell",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolExecutionContract {
    pub form: ToolExecutionForm,
    pub lifetime: ToolExecutionLifetime,
    pub start: ToolExecutionStart,
    pub continuation: ToolExecutionContinuation,
}

impl ToolExecutionContract {
    pub const fn new(
        form: ToolExecutionForm,
        lifetime: ToolExecutionLifetime,
        start: ToolExecutionStart,
        continuation: ToolExecutionContinuation,
    ) -> Self {
        Self {
            form,
            lifetime,
            start,
            continuation,
        }
    }
}

/// Static Stateless Operator protocol-extension classification. This declares only
/// which protocol capability family admits a hidden runtime tool; authorization,
/// permission, Project authority, and Runner capability remain independent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolOperatorExtensionFamily {
    SkillRuntime,
    SkillManagement,
    MemoryRuntime,
    MemoryManagement,
    TraceDiagnostics,
}

/// User-facing Activity presentation. This is observability metadata only: it
/// grants no authority and must not be used as a Project-visibility shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolActivityPresentation {
    Work,
    Support,
    Transport,
}

impl ToolActivityPresentation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Work => "work",
            Self::Support => "support",
            Self::Transport => "transport",
        }
    }
}

/// Whether one tool call is a meaningful model/environment interaction for
/// Window cadence, runtime freshness, recorder-gap, and Goal liveness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolActivityInteraction {
    Meaningful,
    NonMeaningful,
}

impl ToolActivityInteraction {
    pub const fn is_meaningful(self) -> bool {
        matches!(self, Self::Meaningful)
    }
}

/// Canonical semantic kind projected into human-facing Activity surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolActivityKind {
    Read,
    Search,
    Navigate,
    Edit,
    Run,
    Test,
    Review,
    None,
}

impl ToolActivityKind {
    pub const fn as_str(self) -> Option<&'static str> {
        match self {
            Self::Read => Some("read"),
            Self::Search => Some("search"),
            Self::Navigate => Some("navigate"),
            Self::Edit => Some("edit"),
            Self::Run => Some("run"),
            Self::Test => Some("test"),
            Self::Review => Some("review"),
            Self::None => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolActivityPolicy {
    pub presentation: ToolActivityPresentation,
    pub interaction: ToolActivityInteraction,
    /// Narrow escape hatch for a pre-existing display fact that cannot be
    /// recovered from the other canonical ToolDefinition evidence.
    pub kind_override: Option<ToolActivityKind>,
}

impl ToolActivityPolicy {
    const DEFAULT: Self = Self {
        presentation: ToolActivityPresentation::Work,
        interaction: ToolActivityInteraction::Meaningful,
        kind_override: None,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolActivitySemantics {
    pub presentation: ToolActivityPresentation,
    pub interaction: ToolActivityInteraction,
    pub kind: ToolActivityKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCompositionPolicy {
    Denied,
    Sequential,
    Parallel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolHostConcurrencyHint {
    Unspecified,
    IndependentParallelRead,
    Sequential,
}

impl ToolHostConcurrencyHint {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unspecified => "unspecified",
            Self::IndependentParallelRead => "independent_parallel_read",
            Self::Sequential => "sequential",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolHostOrchestrationHint {
    pub concurrency: ToolHostConcurrencyHint,
    pub native_batch_field: Option<&'static str>,
    pub compound_preferred: bool,
}

impl ToolHostOrchestrationHint {
    pub const UNSPECIFIED: Self = Self {
        concurrency: ToolHostConcurrencyHint::Unspecified,
        native_batch_field: None,
        compound_preferred: false,
    };

    pub const fn independent_parallel_read() -> Self {
        Self {
            concurrency: ToolHostConcurrencyHint::IndependentParallelRead,
            ..Self::UNSPECIFIED
        }
    }

    pub const fn sequential() -> Self {
        Self {
            concurrency: ToolHostConcurrencyHint::Sequential,
            ..Self::UNSPECIFIED
        }
    }

    pub const fn with_native_batch_field(mut self, field: &'static str) -> Self {
        self.native_batch_field = Some(field);
        self
    }

    pub const fn with_compound_preferred(mut self) -> Self {
        self.compound_preferred = true;
        self
    }

    pub const fn is_unspecified(self) -> bool {
        matches!(self.concurrency, ToolHostConcurrencyHint::Unspecified)
            && self.native_batch_field.is_none()
            && !self.compound_preferred
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ToolDefinition {
    pub name: &'static str,
    pub audit: ToolAuditPolicy,
    pub model_spec: Option<ToolModelSpecDeclaration>,
    /// Stable direct-call ordering for the one canonical Adaptive Runtime.
    /// `None` means a model-visible tool belongs to the long tail behind
    /// `call_runtime_tool`.
    pub adaptive_runtime_direct_rank: Option<u16>,
    /// GPT Actions follows canonical Adaptive routing unless this definition
    /// declares a concrete protocol incompatibility or a gateway-only surface
    /// budget exception.
    pub gpt_action_exposure: ToolGptActionExposure,
    pub operator_extension_family: Option<ToolOperatorExtensionFamily>,
    /// Optional canonical selection semantics for ordinary execution tools.
    pub execution: Option<ToolExecutionContract>,
    /// Canonical scheduling policy for nested orchestration. This grants no
    /// authority; orchestration frontends retain their own explicit admission.
    pub composition: ToolCompositionPolicy,
    /// Static guidance for Host-native orchestration. This never changes
    /// admission, authority, effects, permission, retry, idempotency, or the
    /// nested WebCodex composition scheduler.
    pub host_orchestration: ToolHostOrchestrationHint,
    pub visibility: ToolVisibility,
    pub category: &'static str,
    pub metadata: ToolMetadata,
    pub policy: ToolDefinitionPolicy,
    pub activity: ToolActivityPolicy,
    pub session_evidence: ToolSessionEvidencePolicy,
    /// Runner capability/owner requirement before dispatch reaches a Runner-backed
    /// Project. `None` means the tool is not Runner-dispatched or enforces its
    /// ownership boundary inside a specialized handler.
    pub runner_capability: Option<RunnerCapabilityRequirement>,
}

impl ToolDefinition {
    pub const fn with_operator_extension_family(
        mut self,
        family: ToolOperatorExtensionFamily,
    ) -> Self {
        self.operator_extension_family = Some(family);
        self
    }

    pub const fn with_execution(mut self, execution: ToolExecutionContract) -> Self {
        self.execution = Some(execution);
        self
    }

    pub const fn with_composition_policy(mut self, policy: ToolCompositionPolicy) -> Self {
        self.composition = policy;
        self
    }

    pub const fn with_host_orchestration_hint(mut self, hint: ToolHostOrchestrationHint) -> Self {
        self.host_orchestration = hint;
        self
    }

    /// Override only GPT Actions presentation text. This never changes the
    /// canonical ToolSpec schema, semantic contract, authority, or MCP copy.
    pub const fn with_gpt_action_description(mut self, description: &'static str) -> Self {
        if let Some(mut model_spec) = self.model_spec {
            model_spec.gpt_action_description = Some(description);
            self.model_spec = Some(model_spec);
        }
        self
    }

    /// Keep a canonical model-visible tool GPT-Action-compatible while routing
    /// it through call_runtime_tool instead of a dedicated direct operation.
    pub const fn with_gpt_action_gateway_only(mut self) -> Self {
        self.gpt_action_exposure = ToolGptActionExposure::GatewayOnly;
        self
    }

    /// Mark a canonical model-visible tool as incompatible with GPT Actions
    /// transport while leaving canonical runtime admission unchanged.
    pub const fn with_gpt_action_unsupported(mut self) -> Self {
        self.gpt_action_exposure = ToolGptActionExposure::Unsupported;
        self
    }

    pub const fn with_activity(
        mut self,
        presentation: ToolActivityPresentation,
        interaction: ToolActivityInteraction,
    ) -> Self {
        self.activity.presentation = presentation;
        self.activity.interaction = interaction;
        self
    }

    pub const fn with_activity_kind(mut self, kind: ToolActivityKind) -> Self {
        self.activity.kind_override = Some(kind);
        self
    }
}

pub const TOOL_CATEGORY_AGENT_TASK: &str = "agent_task";
pub const TOOL_CATEGORY_AGENT_WAIT: &str = "agent_wait";
pub const TOOL_CATEGORY_ARTIFACT: &str = "artifact";
pub const TOOL_CATEGORY_CHECKPOINT: &str = "checkpoint";
pub const TOOL_CATEGORY_CODING_AGENT: &str = "coding_agent";
pub const TOOL_CATEGORY_BROWSER: &str = "browser";
pub const TOOL_CATEGORY_COMPUTER: &str = "computer";
pub const TOOL_CATEGORY_COMMUNICATION: &str = "communication";
pub const TOOL_CATEGORY_CLEANUP: &str = "cleanup";
pub const TOOL_CATEGORY_EDIT: &str = "edit";
pub const TOOL_CATEGORY_FILE: &str = "file";
pub const TOOL_CATEGORY_GIT: &str = "git";
pub const TOOL_CATEGORY_GOAL: &str = "goal";
pub const TOOL_CATEGORY_JOB: &str = "job";
pub const TOOL_CATEGORY_LSP: &str = "lsp";
pub const TOOL_CATEGORY_PATCH: &str = "patch";
pub const TOOL_CATEGORY_PROJECT: &str = "project";
pub const TOOL_CATEGORY_RUNTIME: &str = "runtime";
pub const TOOL_CATEGORY_SESSION: &str = "session";
pub const TOOL_CATEGORY_VALIDATION: &str = "validation";

pub const PERMISSION_RISK_ARTIFACT_WRITE: &str = "artifact_write";
pub const PERMISSION_RISK_BROWSER_CONTROL: &str = "browser_control";
pub const PERMISSION_RISK_DESTRUCTIVE: &str = "destructive";
pub const PERMISSION_RISK_JOB: &str = "job";
pub const PERMISSION_RISK_PATCH: &str = "patch";
pub const PERMISSION_RISK_SHELL: &str = "shell";
pub const PERMISSION_RISK_VALIDATION: &str = "validation";
pub const PERMISSION_RISK_WRITE: &str = "write";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolEffectAnnotations {
    pub read_only_hint: bool,
    pub destructive_hint: bool,
    pub idempotent_hint: bool,
    pub open_world_hint: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolDefinitionPolicy {
    pub change_summary_like: bool,
    pub captures_validation_output: bool,
    pub git_like: bool,
    pub permission_risk: Option<&'static str>,
    pub requires_artifact_upload_path_binding: bool,
    pub requires_explicit_business_session: bool,
    pub unit_arguments: bool,
}

impl ToolDefinitionPolicy {
    const DEFAULT: Self = Self {
        change_summary_like: false,
        captures_validation_output: false,
        git_like: false,
        permission_risk: None,
        requires_artifact_upload_path_binding: false,
        requires_explicit_business_session: false,
        unit_arguments: false,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolDiscoveryGroup {
    pub name: &'static str,
    pub tools: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolRecommendedFlow {
    pub name: &'static str,
    pub summary: &'static str,
    pub manifest_purpose: &'static str,
    pub tools: &'static [&'static str],
}

/// Model-facing task intent for compact `tool_manifest` discovery views.
/// Distinct from `category` (taxonomy) and recommended flows (short loop hints).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolManifestIntent {
    pub name: &'static str,
    pub purpose: &'static str,
    pub tools: &'static [&'static str],
}

const fn def(
    name: &'static str,
    audit: ToolAuditPolicy,
    visibility: ToolVisibility,
    category: &'static str,
    runner_capability: Option<RunnerCapabilityRequirement>,
    provider_id: &'static str,
    semantic: ToolSemanticContract,
    required_scope: Option<&'static str>,
    requires_project: bool,
    path_hint: ToolPathHint,
    destructive: bool,
    shell_like: bool,
    session_evidence: ToolSessionEvidencePolicy,
) -> ToolDefinition {
    ToolDefinition {
        name,
        audit,
        model_spec: None,
        adaptive_runtime_direct_rank: None,
        gpt_action_exposure: ToolGptActionExposure::Inherit,
        operator_extension_family: None,
        execution: None,
        composition: ToolCompositionPolicy::Denied,
        host_orchestration: ToolHostOrchestrationHint::UNSPECIFIED,
        visibility,
        category,
        metadata: make_tool_metadata(
            name,
            provider_id,
            semantic,
            required_scope,
            requires_project,
            path_hint,
            destructive,
            shell_like,
        ),
        policy: ToolDefinitionPolicy::DEFAULT,
        activity: ToolActivityPolicy::DEFAULT,
        session_evidence,
        runner_capability,
    }
}

const fn model_spec(definition: ToolDefinition, description: &'static str) -> ToolDefinition {
    ToolDefinition {
        model_spec: Some(ToolModelSpecDeclaration {
            description,
            gpt_action_description: None,
        }),
        ..definition
    }
}

const fn adaptive_runtime_direct(definition: ToolDefinition, rank: u16) -> ToolDefinition {
    ToolDefinition {
        adaptive_runtime_direct_rank: Some(rank),
        ..definition
    }
}

const fn require_all_scopes(
    definition: ToolDefinition,
    scopes: &'static [&'static str],
) -> ToolDefinition {
    ToolDefinition {
        metadata: ToolMetadata {
            authority: ToolAuthorityPolicy::RequireAll(scopes),
            ..definition.metadata
        },
        ..definition
    }
}

const fn require_any_scopes(
    definition: ToolDefinition,
    scopes: &'static [&'static str],
) -> ToolDefinition {
    ToolDefinition {
        metadata: ToolMetadata {
            authority: ToolAuthorityPolicy::RequireAny(scopes),
            ..definition.metadata
        },
        ..definition
    }
}

macro_rules! bool_policy_modifier {
    ($function:ident, $field:ident) => {
        const fn $function(definition: ToolDefinition) -> ToolDefinition {
            ToolDefinition {
                policy: ToolDefinitionPolicy {
                    $field: true,
                    ..definition.policy
                },
                ..definition
            }
        }
    };
}

bool_policy_modifier!(captures_validation_output, captures_validation_output);

bool_policy_modifier!(change_summary_like, change_summary_like);

bool_policy_modifier!(git_like, git_like);

const fn permission_risk(
    definition: ToolDefinition,
    permission_risk: &'static str,
) -> ToolDefinition {
    ToolDefinition {
        policy: ToolDefinitionPolicy {
            permission_risk: Some(permission_risk),
            ..definition.policy
        },
        ..definition
    }
}

bool_policy_modifier!(
    requires_artifact_upload_path_binding,
    requires_artifact_upload_path_binding
);

bool_policy_modifier!(
    requires_explicit_business_session,
    requires_explicit_business_session
);

use ToolPathHint::None as NoPath;
use ToolRisk::Read;
use ToolVisibility::ModelVisible;

pub fn tool_definitions() -> impl Iterator<Item = &'static ToolDefinition> {
    TOOL_DEFINITION_GROUPS
        .iter()
        .flat_map(|definitions| definitions.iter())
}

const TOOL_DEFINITION_GROUPS: &[&[ToolDefinition]] = &[
    TOOL_DEFINITION_HEAD,
    sessions::DEFINITIONS,
    communication::DEFINITIONS,
    goals::DEFINITIONS,
    agent_tasks::DEFINITIONS,
    agent_waits::DEFINITIONS,
    memory::DEFINITIONS,
    skills::DEFINITIONS,
    hygiene::DEFINITIONS,
    #[cfg(feature = "workspace-checkpoints")]
    checkpoints::DEFINITIONS,
    coding_agents::DEFINITIONS,
    #[cfg(feature = "experimental-code-mode")]
    code_mode::DEFINITIONS,
    browser::DEFINITIONS,
    computer::DEFINITIONS,
    diagnostics::DEFINITIONS,
    discovery::DEFINITIONS,
    runner_config::DEFINITIONS,
    ssh_resources::DEFINITIONS,
    plugins::DEFINITIONS,
    jobs::EXECUTION_DEFINITIONS,
    files::SEARCH_DEFINITIONS,
    git::SUMMARY_DEFINITIONS,
    jobs::LISTING_DEFINITIONS,
    files::READ_DEFINITIONS,
    lsp::DEFINITIONS,
    git::DETAIL_DEFINITIONS,
    testing::DEFINITIONS,
    patches::DEFINITIONS,
    hygiene::CLEANUP_DEFINITIONS,
    artifacts::DEFINITIONS,
    edits::DEFINITIONS,
];

const TOOL_DEFINITION_HEAD: &[ToolDefinition] = &[model_spec(
    def(
        "list_tools",
        ToolAuditPolicy::TYPED_CANONICAL,
        ModelVisible,
        TOOL_CATEGORY_RUNTIME,
        None,
        TOOL_PROVIDER_CONTROL,
        ToolSemanticContract {
            effect: ToolEffect::Observe,
            risk: Read,
            approval: ToolApprovalPolicy::None,
            idempotency: ToolIdempotency::PureRead,
        },
        Some(RUNTIME_READ),
        false,
        NoPath,
        false,
        false,
        ToolSessionEvidencePolicy::NONE,
    )
    .with_activity(
        ToolActivityPresentation::Support,
        ToolActivityInteraction::NonMeaningful,
    ),
    "List runtime tools. Full output includes schemas and may be large; use summary_only with category, features, or limit for bounded GPT Action discovery.",
)];
