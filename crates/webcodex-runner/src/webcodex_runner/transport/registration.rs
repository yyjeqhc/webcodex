//! Registration facts, sanitized capability projection and registration recovery policy.
use super::http_client::{
    bounded_single_line, is_active_instance_lease_conflict, looks_like_auth_failure_message,
    post_json, RunnerHttpError, RunnerHttpErrorKind,
};
use crate::webcodex_runner;
use crate::webcodex_runner::config::{
    hostname, max_concurrent_jobs, HotRunnerConfig, ReloadableRunnerConfig, RunnerConfig,
    ShellConfig,
};
use crate::webcodex_runner::job_manager::JobManager;
use crate::webcodex_runner::projects::RunnerProjectCache;
use crate::webcodex_runner::ssh::SshConnectionPool;
use reqwest::blocking::Client;
#[cfg(test)]
use std::path::PathBuf;
use std::sync::{atomic::AtomicBool, Arc};
use webcodex_build_info as build_info;
use webcodex_core::runner_protocol::{
    self, RunnerCapabilities, RunnerCapabilityId, RunnerPolicySummary, RunnerProjectSummary,
    RunnerRegisterRequest, RunnerRegisterResponse, ShellJobInventory, ShellProfileSummaryEntry,
    ShellProfilesSummary, ShellProjectInventoryStatus, RUNNER_PROTOCOL_GENERATION_V2,
};

pub(crate) const RUNNER_REGISTER_PATH: &str = "/api/shell/agent/register";

/// These exact metadata failures must not turn a reconnect into process/Job
/// termination. Retry the intact inventory; never omit active records or relax
/// Server validation. Identity, ownership, bounds and protocol failures are not
/// included. A corrected peer or terminal-history expiry may resolve rejection.
pub(super) fn is_retryable_inventory_rejection(message: &str) -> bool {
    matches!(
        message,
        "job inventory shell is invalid" | "job inventory timestamps are inconsistent"
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RegisterRecoveryAction {
    Retry,
    WaitForLease,
    Fatal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RegisterErrorKind {
    Transient,
    InventoryRejected,
    LeaseConflict,
    Auth,
    EndpointMissing,
    Rejected,
    Config,
    Protocol,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RegisterError {
    pub(crate) kind: RegisterErrorKind,
    pub(crate) message: String,
}

impl RegisterError {
    pub(crate) fn from_http(error: RunnerHttpError, client_id: &str) -> Self {
        let kind = match error.kind {
            RunnerHttpErrorKind::ServerUnavailable
            | RunnerHttpErrorKind::Status
            | RunnerHttpErrorKind::RequestTimeout
            | RunnerHttpErrorKind::Request
            | RunnerHttpErrorKind::DecodeTransient => RegisterErrorKind::Transient,
            RunnerHttpErrorKind::Auth => RegisterErrorKind::Auth,
            RunnerHttpErrorKind::NotFound => RegisterErrorKind::EndpointMissing,
            RunnerHttpErrorKind::Config => RegisterErrorKind::Config,
            RunnerHttpErrorKind::ProtocolDecode => RegisterErrorKind::Protocol,
            RunnerHttpErrorKind::ClientRejected
                if is_active_instance_lease_conflict(client_id, error.server_error.as_deref()) =>
            {
                RegisterErrorKind::LeaseConflict
            }
            RunnerHttpErrorKind::ClientRejected
                if error
                    .server_error
                    .as_deref()
                    .is_some_and(is_retryable_inventory_rejection) =>
            {
                RegisterErrorKind::InventoryRejected
            }
            RunnerHttpErrorKind::ClientRejected => RegisterErrorKind::Rejected,
        };
        let message = if error.kind == RunnerHttpErrorKind::ProtocolDecode {
            format!(
                "register response incompatible with server protocol: endpoint={} {}",
                error.path, error.summary
            )
        } else {
            error.to_string()
        };
        Self { kind, message }
    }

    pub(crate) fn from_response_error(client_id: &str, error: Option<String>) -> Self {
        let summary =
            bounded_single_line(error.as_deref().unwrap_or("register failed without error"));
        let kind = if is_active_instance_lease_conflict(client_id, Some(&summary)) {
            RegisterErrorKind::LeaseConflict
        } else if looks_like_auth_failure_message(&summary) {
            RegisterErrorKind::Auth
        } else if is_retryable_inventory_rejection(&summary) {
            RegisterErrorKind::InventoryRejected
        } else {
            RegisterErrorKind::Rejected
        };
        Self {
            kind,
            message: format!("register rejected by server: {summary}"),
        }
    }

    pub(crate) fn recovery_action(&self) -> RegisterRecoveryAction {
        match self.kind {
            RegisterErrorKind::Transient | RegisterErrorKind::InventoryRejected => {
                RegisterRecoveryAction::Retry
            }
            RegisterErrorKind::LeaseConflict => RegisterRecoveryAction::WaitForLease,
            RegisterErrorKind::Auth
            | RegisterErrorKind::EndpointMissing
            | RegisterErrorKind::Rejected
            | RegisterErrorKind::Config
            | RegisterErrorKind::Protocol => RegisterRecoveryAction::Fatal,
        }
    }

    pub(crate) fn into_message(self) -> String {
        self.message
    }
}

impl std::fmt::Display for RegisterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// Hidden, test/ops-only knob: parse `WEBCODEX_RUNNER_DISABLE_JOB_STATE_RECONCILIATION`
/// as a boolean. Default false (reconciliation stays on). Inline rather than
/// shared because the runner crate does not depend on the server config helpers.
pub(crate) fn disable_job_state_reconciliation_for_test() -> bool {
    matches!(
        std::env::var("WEBCODEX_RUNNER_DISABLE_JOB_STATE_RECONCILIATION")
            .ok()
            .map(|raw| raw.trim().to_ascii_lowercase())
            .as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

pub(crate) fn runner_register_capabilities(cfg: &RunnerConfig) -> RunnerCapabilities {
    let configured = cfg.capabilities.clone().unwrap_or_default();
    let mut capabilities = RunnerCapabilities::default();
    // Only these legacy implementation switches come from configuration. Do not
    // copy the whole catalog: a future wire field is not implemented merely
    // because a configuration advertises it. Provider capabilities are observed
    // separately when building the registration request.
    capabilities.set(RunnerCapabilityId::Shell, configured.shell);
    capabilities.set(RunnerCapabilityId::Git, configured.git);
    // This binary accepts a structured local sh/bash selector on raw shell
    // requests. Older Runners omit the bit so current Servers fail closed.
    capabilities.set(RunnerCapabilityId::ExplicitShellSelection, true);
    capabilities.set(RunnerCapabilityId::BashLoginShell, true);
    capabilities.set(RunnerCapabilityId::Jobs, true);
    capabilities.set(RunnerCapabilityId::FileRead, true);
    capabilities.set(RunnerCapabilityId::FileWrite, true);
    // This binary implements the narrow internal seek/read export-chunk path.
    // Older binaries omit the field so Control uses the existing slow fallback.
    capabilities.set(RunnerCapabilityId::FileListPage, true);
    capabilities.set(RunnerCapabilityId::ArtifactExportChunkRead, true);
    // Large export metadata (size/SHA/MIME) is verified with bounded streaming
    // I/O. Keep this separate from chunk-read support for rolling upgrades.
    capabilities.set(RunnerCapabilityId::ArtifactExportStreamingMetadata, true);
    // This binary implements the complete bounded structured delete contract.
    // Older binaries omit the field and therefore keep using the Server's legacy path.
    capabilities.set(RunnerCapabilityId::StructuredFileDelete, true);
    // This binary enforces ApplyTextEditInput.occurrence exactly. Older binaries
    // omit this additive effect-semantics capability and must not receive selectors.
    capabilities.set(RunnerCapabilityId::ApplyTextEditOccurrence, true);
    // Unique exact local edits may be preflighted against current content
    // without a historical whole-file SHA. The transactional source-SHA fence
    // before mutation remains mandatory.
    capabilities.set(RunnerCapabilityId::ApplyTextEditLocalGuardWithoutSha, true);
    // Line scopes are an additive rolling-upgrade fence: advertise only because
    // this binary resolves full-match containment before any mutation.
    capabilities.set(RunnerCapabilityId::ApplyTextEditLineScope, true);
    // Deterministic whole-line range replacement is an additive rolling-upgrade
    // capability and must never be inferred from generic line_scope support.
    capabilities.set(RunnerCapabilityId::ApplyTextEditRange, true);
    // This binary proves explicit all-match cardinality before any file write.
    capabilities.set(RunnerCapabilityId::ApplyTextEditExpectedMatchCount, true);
    // Codex Patch is an additive request kind with Runner-authoritative parsing and
    // transaction semantics. Older Runners omit it and must fail closed.
    capabilities.set(RunnerCapabilityId::ApplyPatch, true);
    // WebCodex 0.4 requires every successful patch to expose the complete bounded
    // patch-plan/match metadata consumed by Server validation. Older apply_patch
    // implementations omit this capability and are rejected before dispatch.
    capabilities.set(RunnerCapabilityId::ApplyPatchMatchMetadata, true);
    // Enum-based matching is the 0.4 model-facing authority. Older Runners omit
    // it, so current Servers fail closed instead of falling back to old defaults.
    capabilities.set(RunnerCapabilityId::ApplyPatchMatchingMode, true);
    capabilities.set(RunnerCapabilityId::AsyncJobs, true);
    capabilities.set(RunnerCapabilityId::AsyncShellJobs, true);
    // SSH support intentionally depends on the local OpenSSH executable.
    // Authentication and Host aliases remain entirely Runner-local.
    capabilities.set(
        RunnerCapabilityId::SshShell,
        SshConnectionPool::is_available(),
    );
    // This binary installs the bounded, process-local persistent-shell
    // manager. Older binaries omit this field and therefore fail closed.
    capabilities.set(
        RunnerCapabilityId::PersistentShell,
        webcodex_persistent_shell::local_shell_supported(),
    );
    // SSH persistent shells reuse the same OpenSSH executable as `ssh_shell`.
    // Older binaries omit this field and therefore fail closed; it is never
    // inferred from `ssh_shell` + `persistent_shell`.
    capabilities.set(
        RunnerCapabilityId::SshPersistentShell,
        SshConnectionPool::persistent_shell_available(),
    );
    capabilities.set(RunnerCapabilityId::StructuredValidationArgv, true);
    // This binary durably round-trips Cargo test-count assertions with
    // validation Job context and reconciliation snapshots.
    capabilities.set(RunnerCapabilityId::StructuredCargoTestCountAssertion, true);
    // Explicit require_tests/no_run policy changes validation proof semantics,
    // so advertise durable preservation independently from the older count
    // assertion capability for rolling upgrades.
    capabilities.set(RunnerCapabilityId::StructuredCargoTestExecutionPolicy, true);
    // `--lib` expands the older structured Cargo test argv vocabulary, so
    // advertise it separately for mixed Server/Runner rolling upgrades.
    capabilities.set(RunnerCapabilityId::StructuredCargoTestLib, true);
    // Repeated `-p` selectors expand the older single-package Cargo check argv
    // vocabulary, so advertise this independently for rolling upgrades.
    capabilities.set(RunnerCapabilityId::StructuredCargoCheckPackages, true);
    // This binary accepts both legacy Go validation argv from old Servers and
    // the current machine-readable JSON argv. Do not trust static config or
    // infer this from generic structured validation support.
    capabilities.set(RunnerCapabilityId::StructuredGoTestJson, true);
    capabilities.set(RunnerCapabilityId::ProjectValidation, true);
    capabilities.set(RunnerCapabilityId::ProjectBuild, true);
    capabilities.set(RunnerCapabilityId::ProjectDependencyPolicy, true);
    // Go project gateways pin GO111MODULE=on and GOWORK=off after prepared shell/profile env is
    // applied. Keep this independent for mixed Server/Runner rolling upgrades.
    capabilities.set(RunnerCapabilityId::ProjectGoSingleModule, true);
    // Portable package scope is additive to project_validation_v1 so mixed
    // Server/Runner deployments fail closed before sending the expanded request.
    capabilities.set(RunnerCapabilityId::ProjectValidationPackageScope, true);
    capabilities.set(RunnerCapabilityId::ProjectAllPackages, true);
    capabilities.set(RunnerCapabilityId::ProjectValidationTestOptions, true);
    capabilities.set(RunnerCapabilityId::ProjectValidationPythonPytest, true);
    // This binary also understands the first-class go_test durable metadata
    // identity. Keep this independent from JSON parsing so an old Runner that
    // supported Connector Go evidence cannot be mistaken for a first-class
    // go_test executor by a newer Server.
    capabilities.set(RunnerCapabilityId::StructuredGoTestTool, true);
    // Focused first-class go_test packages extend the older fixed `./...`
    // wire shape, so advertise them independently for rolling upgrades.
    capabilities.set(RunnerCapabilityId::StructuredGoTestPackages, true);
    capabilities.set(RunnerCapabilityId::StructuredProcessArgv, true);
    capabilities.set(RunnerCapabilityId::StructuredScriptPayload, true);
    // JavaScript extends the older typed-script wire enum. Advertise it
    // separately so a newer Server never sends that variant to an older Runner
    // which already advertised structured_script_payload.
    capabilities.set(RunnerCapabilityId::StructuredScriptJavascript, true);
    // TypeScript extends the same typed-script wire enum independently from
    // JavaScript. This bit means the binary understands the semantic protocol;
    // local Node availability/version is resolved only when execution starts.
    capabilities.set(RunnerCapabilityId::StructuredScriptTypescript, true);
    capabilities.set(RunnerCapabilityId::StructuredScriptPython, true);
    capabilities.set(RunnerCapabilityId::InternalPosixScript, true);
    capabilities.set(RunnerCapabilityId::StructuredExecutionJobs, true);
    capabilities.set(RunnerCapabilityId::JobProcessInput, true);
    // Detached process ownership is an independent additive authority. Until
    // each native backend is implemented and dogfooded it must fail closed
    // rather than being inferred from structured process + durable Jobs.
    capabilities.set(
        RunnerCapabilityId::DetachedProcessJobs,
        cfg!(any(target_os = "linux", target_os = "macos", windows)),
    );
    capabilities.set(RunnerCapabilityId::ProjectLifecycle, true);
    // This binary implements resolve_or_register_project; do not trust config to
    // advertise a capability that the binary does not implement.
    capabilities.set(RunnerCapabilityId::ProjectPathRegistration, true);
    capabilities.set(RunnerCapabilityId::ManagedWorktree, true);
    // Configured live roots and managed active Skills share one Runner-local runtime
    // boundary; managed lifecycle authority remains independently advertised.
    capabilities.set(RunnerCapabilityId::SkillRuntime, true);
    capabilities.set(RunnerCapabilityId::SkillResourceExecution, true);
    capabilities.set(RunnerCapabilityId::SkillManagement, true);
    // Native Tool Plugins are a separate Runner-local gateway capability. Keep
    // this explicit even when zero Plugins are configured so cross-platform
    // `plugin_tool reload` can target the exact Runner.
    capabilities.set(RunnerCapabilityId::NativeToolPlugins, true);
    capabilities.set(RunnerCapabilityId::ManagedSshResources, true);
    // Formal config check/reload is implemented directly against this process's
    // startup-bound runner.toml path on every supported platform. Unix SIGHUP is
    // only an additional trigger and is not part of this capability contract.
    capabilities.set(RunnerCapabilityId::RunnerConfigControl, true);
    // Configured instruction files are observed only through the narrow Runner-owned snapshot boundary.
    capabilities.set(RunnerCapabilityId::InstructionRuntime, true);
    // MCP gateway support is fenced by the validated provider inventory in
    // registration rather than a separate capability bit. Older binaries omit
    // that inventory, so a newer Server will never target them.
    // `job_state_reconciliation` is on by default. A hidden, test/ops-only env
    // knob lets an E2E exercise the valid generation-2 no-reconciliation mode
    // (it then has no job inventory and a disconnect falls straight to `lost`).
    // Default production behavior is unchanged: only the explicit opt-out
    // disables it, and the server already rejects inventory without the
    // capability and vice-versa.
    // Browser capabilities are registration-required and depend on the actual
    // Runner-local Chromium-family discovery result. The Server must never infer
    // them from OS, protocol generation, shell, or Computer capabilities.
    let browser_available = webcodex_browser::discover_chromium_executable().is_some();
    capabilities.set(RunnerCapabilityId::BrowserObserve, browser_available);
    capabilities.set(RunnerCapabilityId::BrowserControl, browser_available);
    // This binary publishes exact snapshot node `actions` and enforces the same
    // admission set before element effects. Keep it separate from generic Browser
    // control so a new Server cannot dispatch the stricter contract to an older Runner.
    capabilities.set(
        RunnerCapabilityId::BrowserElementActionAdmission,
        browser_available,
    );
    capabilities.set(RunnerCapabilityId::BrowserBatch, browser_available);
    capabilities.set(RunnerCapabilityId::BrowserLaunch, browser_available);
    // Native read-only desktop observation is implemented only on macOS and
    // Windows. Unsupported platforms advertise false and fail closed.
    capabilities.set(
        RunnerCapabilityId::ComputerObserve,
        cfg!(any(target_os = "macos", windows)),
    );
    // Installed-application discovery and exact launch are native macOS/Windows
    // additive capabilities. Neither is inferred from observation/control or
    // from each other.
    capabilities.set(
        RunnerCapabilityId::ComputerApplicationDiscovery,
        cfg!(any(target_os = "macos", windows)),
    );
    capabilities.set(
        RunnerCapabilityId::ComputerApplicationLaunch,
        cfg!(any(target_os = "macos", windows)),
    );
    // Exact full-display discovery/snapshot is independently implemented by
    // the native macOS and Windows backends; unsupported platforms fail closed.
    capabilities.set(
        RunnerCapabilityId::ComputerDisplayObserve,
        cfg!(any(target_os = "macos", windows)),
    );
    // Snapshot-fenced exact coordinate pointer input is independently implemented by
    // the native macOS and Windows backends; unsupported platforms fail closed.
    capabilities.set(
        RunnerCapabilityId::ComputerPointerControl,
        cfg!(any(target_os = "macos", windows)),
    );
    // Bounded Unicode-text clipboard observation/replacement are separate
    // native capabilities on macOS and Windows.
    capabilities.set(
        RunnerCapabilityId::ComputerClipboardRead,
        cfg!(any(target_os = "macos", windows)),
    );
    capabilities.set(
        RunnerCapabilityId::ComputerClipboardWrite,
        cfg!(any(target_os = "macos", windows)),
    );
    // Region/downscale snapshot requests use a distinct additive wire fence so
    // old Runners that support only whole-window snapshots fail closed.
    capabilities.set(
        RunnerCapabilityId::ComputerSnapshotRegion,
        cfg!(any(target_os = "macos", windows)),
    );
    // Accessibility inspection is a separate read-only semantic capability.
    // macOS AX and Windows UI Automation share the same model-facing tree;
    // observation authority never implies computer-control authority.
    capabilities.set(
        RunnerCapabilityId::ComputerAccessibilityObserve,
        cfg!(any(target_os = "macos", windows)),
    );
    // Normalized element-state observation is a separate rolling-upgrade wire
    // capability implemented by the same native read-only backends.
    capabilities.set(
        RunnerCapabilityId::ComputerElementState,
        cfg!(any(target_os = "macos", windows)),
    );
    // Accessibility control is independently fenced and implemented by the
    // native macOS AX and Windows UI Automation backends.
    capabilities.set(
        RunnerCapabilityId::ComputerControl,
        cfg!(any(target_os = "macos", windows)),
    );
    // Semantic native scroll-to-visible is independently fenced for rolling upgrades;
    // existing computer_control support never implies it.
    capabilities.set(
        RunnerCapabilityId::ComputerScrollToElement,
        cfg!(any(target_os = "macos", windows)),
    );
    // Closed key input is a separate effect/wire capability implemented by the
    // native macOS and Windows paths and is never implied by control.
    capabilities.set(
        RunnerCapabilityId::ComputerKeyInput,
        cfg!(any(target_os = "macos", windows)),
    );
    // Exact window activation is a separate effect/wire capability. It is
    // independently advertised by native macOS and Windows implementations.
    capabilities.set(
        RunnerCapabilityId::ComputerWindowActivate,
        cfg!(any(target_os = "macos", windows)),
    );
    // Bounded Accessibility text input is a separate rolling-upgrade fence;
    // older native Runners with computer_control must not be treated as capable.
    capabilities.set(
        RunnerCapabilityId::ComputerTextInput,
        cfg!(any(target_os = "macos", windows)),
    );
    capabilities.set(
        RunnerCapabilityId::JobStateReconciliation,
        !disable_job_state_reconciliation_for_test(),
    );

    // New agents always advertise read-only LSP navigation. Older agents omit
    // the field and deserialize as false on the server.
    capabilities.set(RunnerCapabilityId::LspReadOnlyNavigation, true);
    // Advertise the distinct capability only because this binary installs the
    // bounded typed prepare/incoming/outgoing traversal implementation.
    capabilities.set(RunnerCapabilityId::LspCallHierarchy, true);
    capabilities
}

#[cfg(test)]
pub(crate) fn build_register_request(
    cfg: &RunnerConfig,
    runner_instance_id: &str,
    prepared_cache_count: usize,
) -> RunnerRegisterRequest {
    let runtime = ReloadableRunnerConfig::new(cfg.clone(), PathBuf::new());
    build_register_request_with_provider_status(
        cfg,
        &runtime,
        runner_instance_id,
        prepared_cache_count,
        ShellJobInventory {
            active_complete: true,
            jobs: Vec::new(),
        },
    )
    .0
}

pub(crate) fn build_register_request_with_provider_status(
    cfg: &RunnerConfig,
    runtime: &ReloadableRunnerConfig,
    runner_instance_id: &str,
    prepared_cache_count: usize,
    job_inventory: ShellJobInventory,
) -> (
    RunnerRegisterRequest,
    Arc<webcodex_runner::external_tools::ExternalToolRouter>,
    u64,
) {
    webcodex_runner::computer_session::set_server_availability_contract(false);
    let hot = runtime.snapshot();
    let mut capabilities = runner_register_capabilities(cfg);
    let coding_agent_providers = runtime
        .coding_agents()
        .map(|manager| manager.providers())
        .unwrap_or_default();
    let coding_agent_inventory = runtime.coding_agents().map(|manager| manager.inventory());
    capabilities.set(
        RunnerCapabilityId::CodingAgentRuns,
        !coding_agent_providers.is_empty(),
    );
    let (mut tool_providers, revision) = hot.external_tools.registration_status();
    tool_providers.config_reload = hot.reload_status();
    (
        RunnerRegisterRequest {
            client_id: cfg.client_id.clone(),
            runner_instance_id: runner_instance_id.to_string(),
            runner_protocol_generation: RUNNER_PROTOCOL_GENERATION_V2,
            display_name: cfg.display_name.clone(),
            owner: cfg.owner.clone(),
            hostname: cfg.hostname.clone().or_else(hostname),
            host_context: cfg.host_context.clone(),
            capabilities,
            computer_session_availability: webcodex_runner::computer_session::availability(),
            policy: Some(register_policy_summary(
                &hot,
                prepared_cache_count,
                tool_providers,
                runtime.mcp_gateway().provider_inventory(),
            )),
            process_started_at: Some(process_started_at()),
            build: Some(runner_build_info()),
            job_concurrency_limit: Some(max_concurrent_jobs(cfg)),
            // A Runner with reconciliation disabled for E2E must not send a job
            // inventory: the server rejects inventory without the capability and
            // vice-versa.
            job_inventory: if disable_job_state_reconciliation_for_test() {
                None
            } else {
                Some(job_inventory)
            },
            coding_agent_providers: (!coding_agent_providers.is_empty())
                .then_some(coding_agent_providers),
            coding_agent_inventory,
        },
        Arc::clone(&hot.external_tools),
        revision,
    )
}

/// Unix timestamp when this runner process started. Captured on first call;
/// `run_runner` initializes it at startup so registration payloads report the
/// real process start, not the first register time after a reconnect.
pub(crate) fn process_started_at() -> i64 {
    static STARTED_AT: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *STARTED_AT.get_or_init(|| chrono::Utc::now().timestamp())
}

/// Non-secret runner build identity for mixed-version diagnostics.
pub(crate) fn runner_build_info() -> runner_protocol::RunnerBuildInfo {
    let info = build_info::current();
    runner_protocol::RunnerBuildInfo {
        version: Some(info.version.to_string()),
        git_commit: info.git_commit.map(str::to_string),
        git_dirty: info.git_dirty,
        built_at: info.built_at.map(str::to_string),
        target: info.target.map(str::to_string),
        architecture: info.architecture.map(str::to_string),
    }
}

/// Shell dialect derived from a program path basename. Only `sh` and `bash`
/// map to portable POSIX dialects; `powershell`/`powershell.exe`/`pwsh` map to
/// the Windows PowerShell dialect; anything else is `custom` and callers that
/// need deterministic syntax must select an explicit `shell=sh|bash` (or a
/// configured dialect on the Runner side).
pub(crate) fn shell_dialect_for_program(program: &str) -> &'static str {
    match std::path::Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(program)
    {
        "sh" => "sh",
        "bash" => "bash",
        "powershell" | "powershell.exe" | "pwsh" => "powershell",
        _ => "custom",
    }
}

/// Build the sanitized shell-profiles summary from the active shell config.
/// Exposes only safe metadata: profile names, whether each has an init_script
/// (boolean, never the body), env key counts (never values), the resolved
/// program, and arg counts. `prepared_cache_count` is the number of snapshots
/// prepared at call time (typically 0 right after start). Never includes env
/// values, init_script bodies, tokens, or the full env snapshot.
pub(crate) fn build_shell_profiles_summary(
    shell: &ShellConfig,
    prepared_cache_count: usize,
) -> ShellProfilesSummary {
    let profiles: Vec<ShellProfileSummaryEntry> = shell
        .profiles
        .iter()
        .map(|(name, profile)| {
            let program = profile
                .program
                .clone()
                .unwrap_or_else(|| shell.program.clone());
            let args = profile.args.clone().unwrap_or_else(|| shell.args.clone());
            let dialect = shell_dialect_for_program(&program);
            ShellProfileSummaryEntry {
                name: name.clone(),
                has_init_script: profile.init_script.is_some(),
                env_keys_count: profile.env.len(),
                program,
                args_count: args.len(),
                dialect: Some(dialect.to_string()),
            }
        })
        .collect();
    // Default execution path when the caller selects no explicit shell:
    // shell.default_profile if set, otherwise the plain shell program.
    // (A project-level shell_profile override is reported per project.)
    let default_program = shell
        .default_profile
        .as_deref()
        .and_then(|name| shell.profiles.get(name))
        .and_then(|profile| profile.program.clone())
        .unwrap_or_else(|| shell.program.clone());
    let default_dialect = shell_dialect_for_program(&default_program).to_string();
    // Report only semantic shells this exact Runner can resolve through its
    // effective execution PATH. This keeps model recovery guidance from
    // suggesting bash/sh merely because the protocol supports those selectors.
    let mut available: Vec<String> = Vec::new();
    for (name, language) in [
        ("sh", runner_protocol::ShellScriptLanguage::Sh),
        ("bash", runner_protocol::ShellScriptLanguage::Bash),
    ] {
        if webcodex_runner::explicit_shell_available(shell, language) {
            available.push(name.to_string());
        }
    }
    for entry in &profiles {
        if let Some(dialect) = entry.dialect.as_deref() {
            if !available.iter().any(|existing| existing == dialect) {
                available.push(dialect.to_string());
            }
        }
    }
    ShellProfilesSummary {
        default_profile: shell.default_profile.clone(),
        configured_count: shell.profiles.len(),
        prepared_cache_count,
        profiles,
        default_dialect: Some(default_dialect),
        available_dialects: Some(available),
    }
}

/// Build the sanitized Runner policy summary sent at registration. The wire
/// projection remains unchanged; it mirrors local `RunnerPolicy` but carries
/// only non-secret fields. The shell env
/// values and init_script path are intentionally NOT included. The sanitized
/// shell-profiles summary is attached so observability can show which profile
/// a project resolves to without exposing env values or init_script bodies.
pub(crate) fn register_policy_summary(
    cfg: &HotRunnerConfig,
    prepared_cache_count: usize,
    tool_providers: runner_protocol::ToolProvidersStatus,
    mcp_gateway_providers: Vec<webcodex_core::mcp_gateway::McpGatewayProvider>,
) -> RunnerPolicySummary {
    RunnerPolicySummary {
        allow_raw_shell: cfg.policy.allow_raw_shell,
        allow_cwd_anywhere: cfg.policy.allow_cwd_anywhere,
        allowed_roots: cfg.policy.allowed_roots.clone(),
        max_timeout_secs: cfg.policy.max_timeout_secs,
        max_output_bytes: cfg.policy.max_output_bytes,
        shell_profiles: Some(build_shell_profiles_summary(
            &cfg.shell,
            prepared_cache_count,
        )),
        tool_providers: Some(tool_providers),
        mcp_gateway_providers: Some(mcp_gateway_providers),
    }
}

pub(crate) fn register(
    client: &Client,
    cfg: &RunnerConfig,
    runtime: &ReloadableRunnerConfig,
    project_cache: &mut RunnerProjectCache,
    shutdown: Option<&AtomicBool>,
    runner_instance_id: &str,
    prepared_cache_count: usize,
    jobs: &JobManager,
) -> Result<
    (
        usize,
        ShellJobInventory,
        Vec<RunnerProjectSummary>,
        ShellProjectInventoryStatus,
    ),
    RegisterError,
> {
    let projects = project_cache.get_with_shutdown(cfg, shutdown);
    let projects_count = projects.iter().filter(|project| !project.disabled).count();
    let job_inventory = jobs.inventory();
    let (body, provider, provider_revision) = build_register_request_with_provider_status(
        cfg,
        runtime,
        runner_instance_id,
        prepared_cache_count,
        job_inventory.clone(),
    );
    let response: RunnerRegisterResponse = post_json(client, cfg, RUNNER_REGISTER_PATH, &body)
        .map_err(|error| RegisterError::from_http(error, &cfg.client_id))?;
    if response.success {
        webcodex_runner::computer_session::set_server_availability_contract(
            webcodex_runner::computer_session::registration_echo_confirms_contract(
                response
                    .client
                    .as_ref()
                    .and_then(|client| client.computer_session_availability),
            ),
        );
        provider.mark_status_reported(provider_revision);
        let inventory_status = response
            .client
            .as_ref()
            .and_then(|client| client.project_inventory.clone())
            .ok_or_else(|| RegisterError {
                kind: RegisterErrorKind::Protocol,
                message: "register response missing canonical project_inventory acknowledgement; Server is incompatible with this 0.4 Runner".to_string(),
            })?;
        Ok((projects_count, job_inventory, projects, inventory_status))
    } else {
        Err(RegisterError::from_response_error(
            &cfg.client_id,
            response.error,
        ))
    }
}
