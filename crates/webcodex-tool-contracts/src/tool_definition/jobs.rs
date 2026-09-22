use super::RunnerCapabilityRequirement::{
    AsyncJobs, DetachedProcess, PersistentShell, Shell, StructuredProcess, StructuredScript,
};
use super::ToolVisibility::{ModelHidden, ModelVisible};
use super::{
    adaptive_runtime_direct, def, model_spec, permission_risk, require_all_scopes,
    requires_explicit_business_session, ToolDefinition, PERMISSION_RISK_JOB, TOOL_CATEGORY_JOB,
};
use crate::metadata::{
    ToolPathHint::None as NoPath,
    ToolRisk::{JobRun, Read},
    JOB_RUN, RUNTIME_READ, TOOL_PROVIDER_NATIVE, TOOL_PROVIDER_RUNNER,
};
use webcodex_core::authority::SCOPE_JOB_DETACH;

pub(super) const EXECUTION_DEFINITIONS: &[ToolDefinition] = &[
    adaptive_runtime_direct(
        model_spec(
            def(
                "run_process",
                super::ToolAuditPolicy::TYPED_CANONICAL
                    .session_input(super::ToolAuditSessionInputPolicy::OmitTopLevel(&[
                        "executable",
                        "args",
                        "stdin",
                        "process_summary",
                    ]))
                    .execution(super::ToolAuditExecutionPolicy::DIRECT_ARGV_TEST_COUNTS),
                ModelVisible,
                TOOL_CATEGORY_JOB,
                Some(StructuredProcess),
                TOOL_PROVIDER_RUNNER,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Execute,
                    risk: JobRun,
                    approval: super::ToolApprovalPolicy::Standard,
                    idempotency: super::ToolIdempotency::NonIdempotent,
                },
                Some(JOB_RUN),
                true,
                NoPath,
                true,
                true,
                super::ToolSessionEvidencePolicy::NONE,
            ),
            "Run one one-shot executable with structured argv. This is the preferred route for one native executable with literal argv; Windows batch shims use the bounded Runner-owned quoting contract on executable. Use run_shell only when shell semantics or a short tightly related command chain is required. Do not open a persistent shell merely to run several commands: local persistence is only for same-process cwd/env/exports/functions/umask state; repeated commands on one named SSH resource preserve remote state. New persistent SSH targets use ssh_resource onboarding; one-shot/no-persistence SSH remains valid. Long work continues as the same execution and stays Runner-owned; timeout_secs defaults to 60 seconds and the total execution lifetime clamps at 7 days. If the native child must outlive the Runner because this Runner will restart, upgrade, stop, or be replaced, use run_detached_process from the start; duration alone is not a reason to detach.",
        ).with_gpt_action_description("Run one native executable with literal argv; prefer this over shell when shell syntax is unnecessary. Long work continues as the same Job. Use run_detached_process only when the child must survive Runner restart/upgrade.")
        .with_execution(super::ToolExecutionContract::new(
            super::ToolExecutionForm::NativeArgv,
            super::ToolExecutionLifetime::Runner,
            super::ToolExecutionStart::SyncFirst,
            super::ToolExecutionContinuation::ObserveJobs,
        )),
        70,
    ),
    adaptive_runtime_direct(
        require_all_scopes(
            model_spec(
                def(
                    "run_detached_process",
                    super::ToolAuditPolicy::TYPED_CANONICAL.session_input(
                        super::ToolAuditSessionInputPolicy::OmitTopLevel(&[
                            "executable",
                            "args",
                            "stdin",
                            "idempotency_key",
                            "process_summary",
                        ]),
                    ),
                    ModelVisible,
                    TOOL_CATEGORY_JOB,
                    Some(DetachedProcess),
                    TOOL_PROVIDER_RUNNER,
                    super::ToolSemanticContract {
                        effect: super::ToolEffect::Execute,
                        risk: JobRun,
                        approval: super::ToolApprovalPolicy::Standard,
                        idempotency: super::ToolIdempotency::Keyed,
                    },
                    Some(JOB_RUN),
                    true,
                    NoPath,
                    true,
                    true,
                    super::ToolSessionEvidencePolicy::NONE,
                ),
                "Start a supervisor-owned detached native process as a durable Job when accepted work must outlive the initiating Runner process. Use it from the start when the workflow will restart, upgrade, stop, or replace this Runner and a native child must remain alive across Runner exit or replacement. Duration alone is not a reason to detach: ordinary long work stays Runner-owned. timeout_secs defaults to 60 seconds and the total detached execution lifetime clamps at 7 days. Ownership is handed off before payload start; after restart or upgrade, a replacement Runner can recover the same logical Job only when the supervisor/native identity and lifetime fence reconcile. A bounded replay key prevents duplicate dispatch while retained; expired keys are not retry tokens. Observe or stop with Job tools. No shell, script, SSH-resource, or retry fallback.",
            ).with_gpt_action_description("Start a supervisor-owned native process that must survive Runner restart/upgrade as a durable Job. Requires an idempotency_key; observe/stop with Job tools. Duration alone is not a reason to detach.")
            .with_execution(super::ToolExecutionContract::new(
                super::ToolExecutionForm::NativeArgv,
                super::ToolExecutionLifetime::Supervisor,
                super::ToolExecutionStart::AsyncImmediate,
                super::ToolExecutionContinuation::ObserveJobs,
            )),
            &[JOB_RUN, SCOPE_JOB_DETACH],
        ),
        72,
    ),
    adaptive_runtime_direct(
        model_spec(
            def(
                "run_script",
            super::ToolAuditPolicy::TYPED_CANONICAL
                .session_input(super::ToolAuditSessionInputPolicy::OmitTopLevel(&[
                    "script",
                    "args",
                    "stdin",
                    "script_summary",
                ]))
                .execution(super::ToolAuditExecutionPolicy::SCRIPT_TEST_COUNTS),
            ModelVisible,
            TOOL_CATEGORY_JOB,
            Some(StructuredScript),
            TOOL_PROVIDER_RUNNER,
            super::ToolSemanticContract {
                effect: super::ToolEffect::Execute,
                risk: JobRun,
                approval: super::ToolApprovalPolicy::Standard,
                idempotency: super::ToolIdempotency::NonIdempotent,
            },
            Some(JOB_RUN),
            true,
            NoPath,
            true,
            true,
            super::ToolSessionEvidencePolicy::NONE,
        ),
            "Run bounded sh, bash, PowerShell, JavaScript, or TypeScript as typed Runner-owned script data. JavaScript is Node.js-backed fixed .mjs ESM. TypeScript uses Node native erasable type stripping in .mts ESM, requires Node.js 22.6+, does not type-check, and rejects enum and other transform-required syntax. The Runner owns runtime selection/flags; WebCodex does not install npm dependencies, run tsc, or fall back to Bun/Deno/tsx. Relative ESM imports resolve from the Runner-owned temporary module, not project cwd. Prefer run_process for native argv, run_script for program-like scripts or substantially larger typed programs, and run_shell when shell grammar is required. Long work continues as the same execution / same Job and is never restarted; timeout_secs defaults to 60 seconds and the total script execution lifetime clamps at 7 days; script bodies never become shell command text. If a native child must outlive the Runner across restart/upgrade/stop/replacement, use run_detached_process from the start.",
        )
        .with_gpt_action_description("Run typed sh/bash/PowerShell/JavaScript/TypeScript scripts. Prefer run_process for native argv and run_shell for shell grammar/short chains. Long work continues as the same Job; use run_detached_process only when the child must survive Runner restart/upgrade.")
        .with_execution(super::ToolExecutionContract::new(
            super::ToolExecutionForm::TypedScript,
            super::ToolExecutionLifetime::Runner,
            super::ToolExecutionStart::SyncFirst,
            super::ToolExecutionContinuation::ObserveJobs,
        )),
        74,
    ),
    adaptive_runtime_direct(
        model_spec(
            def(
                "run_shell",
                super::ToolAuditPolicy::TYPED_CANONICAL
                    .session_input(super::ToolAuditSessionInputPolicy::OmitTopLevel(&[
                        "command",
                        "command_summary",
                    ]))
                    .execution(super::ToolAuditExecutionPolicy::TEST_COUNTS),
                ModelVisible,
                TOOL_CATEGORY_JOB,
                Some(Shell),
                TOOL_PROVIDER_RUNNER,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Execute,
                    risk: JobRun,
                    approval: super::ToolApprovalPolicy::Standard,
                    idempotency: super::ToolIdempotency::NonIdempotent,
                },
                Some(JOB_RUN),
                true,
                NoPath,
                true,
                true,
                super::ToolSessionEvidencePolicy::NONE,
            ),
            "Bounded shell command or short related chain; shell semantics; predetermined related observations may share one call; adaptive/result-dependent follow-ups stay sequential; prefer run_process for literal argv; deterministic Python heredoc; one coherent transformation; Project/path/permission policy; avoid unauthorized network; inspect diff; validate final source; run_script handles program-like languages; failure/permission/validation boundaries; commit, push, deploy, restart; same-process state; one named SSH resource; Runner-owned; timeout_secs is total lifetime; sync_wait_secs is Job-handoff grace; later observe wait is one observation; duration does not select the primitive; run_detached_process.",
        ).with_gpt_action_description("Run bounded shell syntax or a short related chain; prefer run_process for literal argv. Long runtime keeps the same Runner-owned execution: timeout_secs is total lifetime and sync_wait_secs is Job-handoff grace. Use observe_jobs later; duration alone does not change the primitive.")
        .with_execution(super::ToolExecutionContract::new(
            super::ToolExecutionForm::ShellCommand,
            super::ToolExecutionLifetime::Runner,
            super::ToolExecutionStart::SyncFirst,
            super::ToolExecutionContinuation::ObserveJobs,
        )),
        75,
    ),
    requires_explicit_business_session(model_spec(
            def(
                "open_session_shell",
                super::ToolAuditPolicy::TYPED_CANONICAL,
                ModelVisible,
                TOOL_CATEGORY_JOB,
                Some(PersistentShell),
                TOOL_PROVIDER_RUNNER,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Execute,
                    risk: JobRun,
                    approval: super::ToolApprovalPolicy::Standard,
                    idempotency: super::ToolIdempotency::NonIdempotent,
                },
                Some(JOB_RUN),
                true,
                NoPath,
                true,
                true,
                super::ToolSessionEvidencePolicy::NONE.persistent_shell(super::PersistentShellEvidenceAction::Open),
            ),
            "Open one bounded long-lived shell for an explicit Workflow Session. Primary use: one shell for repeated commands on the active named SSH resource in execution_context.resource, preserving remote cwd/env/exports/functions/umask. Local sh/bash or Windows PowerShell remains supported only when same local shell-process state is actually required, not merely for several commands. New SSH targets use ssh_resource list/register, Runner restart, list again, then update_session_context; no per-shell host/resource parameter. The SSH target does not need WebCodex Runner.",
    )),
    requires_explicit_business_session(model_spec(
            def(
                "session_shell_exec",
                super::ToolAuditPolicy::TYPED_CANONICAL.session_input(
                    super::ToolAuditSessionInputPolicy::OmitTopLevel(&[
                        "command",
                        "command_summary",
                    ]),
                ),
                ModelVisible,
                TOOL_CATEGORY_JOB,
                Some(PersistentShell),
                TOOL_PROVIDER_RUNNER,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Execute,
                    risk: JobRun,
                    approval: super::ToolApprovalPolicy::Standard,
                    idempotency: super::ToolIdempotency::NonIdempotent,
                },
                Some(JOB_RUN),
                true,
                NoPath,
                true,
                true,
                super::ToolSessionEvidencePolicy::NONE.persistent_shell(super::PersistentShellEvidenceAction::Exec),
            ),
            "Execute one framed command in an existing Session persistent shell. Primary route is repeated commands on the same named SSH resource while retaining remote cwd/env/exports/functions/umask. Local persistent execution remains supported only when the same local shell process must retain state; ordinary one-shot work should use run_process, run_shell for shell semantics or short tightly related chains, and run_script for program-like shell content. Several commands alone are not a reason to open persistent shell. Commands are serialized in the same shell process.",
    )
    .with_execution(super::ToolExecutionContract::new(
        super::ToolExecutionForm::PersistentShellCommand,
        super::ToolExecutionLifetime::SessionShell,
        super::ToolExecutionStart::ExistingSession,
        super::ToolExecutionContinuation::SessionShell,
    ))),
    requires_explicit_business_session(model_spec(
        def(
            "session_shell_status",
            super::ToolAuditPolicy::TYPED_CANONICAL,
            ModelVisible,
            TOOL_CATEGORY_JOB,
            Some(PersistentShell),
            TOOL_PROVIDER_RUNNER,
            super::ToolSemanticContract {
                effect: super::ToolEffect::Observe,
                risk: Read,
                approval: super::ToolApprovalPolicy::None,
                idempotency: super::ToolIdempotency::PureRead,
            },
            Some(RUNTIME_READ),
            true,
            NoPath,
            false,
            false,
            super::ToolSessionEvidencePolicy::NONE.persistent_shell(super::PersistentShellEvidenceAction::Status),
        ),
        "Read Runner-authoritative state for an explicit Session persistent shell. This never sends input to the process.",
    )),
    requires_explicit_business_session(permission_risk(
        model_spec(
            def(
                "close_session_shell",
                super::ToolAuditPolicy::TYPED_CANONICAL,
                ModelVisible,
                TOOL_CATEGORY_JOB,
                Some(PersistentShell),
                TOOL_PROVIDER_RUNNER,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Mutate,
                    risk: JobRun,
                    approval: super::ToolApprovalPolicy::Standard,
                    idempotency: super::ToolIdempotency::DesiredState,
                },
                Some(JOB_RUN),
                true,
                NoPath,
                true,
                false,
                super::ToolSessionEvidencePolicy::NONE.persistent_shell(super::PersistentShellEvidenceAction::Close),
            ),
            "Idempotently close an explicit Session persistent shell and terminate its complete process group.",
        ),
        PERMISSION_RISK_JOB,
    )),
    permission_risk(
        model_spec(
            def(
                "run_job",
                super::ToolAuditPolicy::TYPED_CANONICAL
                    .session_input(super::ToolAuditSessionInputPolicy::OmitTopLevel(&[
                        "command",
                        "command_summary",
                    ]))
                    .execution(super::ToolAuditExecutionPolicy::TEST_COUNTS),
                ModelVisible,
                TOOL_CATEGORY_JOB,
                Some(AsyncJobs),
                TOOL_PROVIDER_RUNNER,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Execute,
                    risk: JobRun,
                    approval: super::ToolApprovalPolicy::Standard,
                    idempotency: super::ToolIdempotency::NonIdempotent,
                },
                Some(JOB_RUN),
                true,
                NoPath,
                true,
                true,
                super::ToolSessionEvidencePolicy::NONE,
            ),
            "Start one Runner-owned asynchronous shell Job immediately and return its stable job_id. Use this only when asynchronous shell execution is intentional from the first call; ordinary work should start on its synchronous execution or structured validation tool and let long execution hand off as the same Job. Queued execution keeps its identity; observe before considering retry. Server disconnect/restart can reconcile the same Job while the owning Runner process remains, but a replacement Runner does not inherit ordinary Jobs. Do not use run_job when a native child must outlive the current Runner process across restart, upgrade, stop, or replacement; use run_detached_process from the start.",
        )
        .with_execution(super::ToolExecutionContract::new(
            super::ToolExecutionForm::ShellCommand,
            super::ToolExecutionLifetime::Runner,
            super::ToolExecutionStart::AsyncImmediate,
            super::ToolExecutionContinuation::ObserveJobs,
        )),
        TOOL_CATEGORY_JOB,
    ),
    adaptive_runtime_direct(permission_risk(
        model_spec(
            def(
                "stop_job",
                super::ToolAuditPolicy::TYPED_CANONICAL,
                ModelVisible,
                TOOL_CATEGORY_JOB,
                None,
                TOOL_PROVIDER_NATIVE,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Mutate,
                    risk: JobRun,
                    approval: super::ToolApprovalPolicy::Standard,
                    idempotency: super::ToolIdempotency::DesiredState,
                },
                Some(JOB_RUN),
                true,
                NoPath,
                true,
                false,
                super::ToolSessionEvidencePolicy::NONE,
            ),
            "Stop one existing WebCodex Job by job_id. Requires confirm=true and preserves project/session ownership; log bodies are not returned.",
        ).with_gpt_action_gateway_only(),
        PERMISSION_RISK_JOB,
    ), 81),
    adaptive_runtime_direct(
        model_spec(
            def(
                "observe_jobs",
                super::ToolAuditPolicy::TYPED_CANONICAL
                    .session_input(super::ToolAuditSessionInputPolicy::ObserveJobs),
                ModelVisible,
                TOOL_CATEGORY_JOB,
                None,
                TOOL_PROVIDER_NATIVE,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Observe,
                    risk: Read,
                    approval: super::ToolApprovalPolicy::None,
                    idempotency: super::ToolIdempotency::PureRead,
                },
                Some(RUNTIME_READ),
                false,
                NoPath,
                false,
                false,
                super::ToolSessionEvidencePolicy::NONE,
            )
            .with_activity(
                super::ToolActivityPresentation::Transport,
                super::ToolActivityInteraction::Meaningful,
            ),
            "Continue known Jobs by job_id; do not call list_jobs first. Prefer observation_ref from a successful item for the next ordinary follow-up, or pass observation_token unchanged as after_observation_token; raw job_id/token remain the recovery fallback. No token gives an immediate baseline; no wait_secs gives an immediate observation. With tokens, bounded wait_secs (runtime max 100) uses wake_on=change by default. Use wait_secs=55,wake_on=terminal when useful progress is blocked on terminal outcome; if independent work remains, observe later—do not poll for visibility. terminal wakes on any terminal Job; all_terminal waits for all; updates never extend the deadline. Item errors return immediately; timeout may include changed=true; unknown/expired refs fail only that item. summary_only=true may omit routine successful validation lines while preserving warnings, counts, and log-loss evidence; suggested_call expands retained logs. Never launches, retries, stops, or subscribes.",
        ).with_gpt_action_description("Continue known Jobs. Prefer the returned continuation; otherwise use wait_secs=55,wake_on=terminal only when blocked on terminal outcome. Runtime allows up to 100s, but longer waits may exceed the Host deadline. Tokens are cursors, never retry authority."),
        80,
    ),
    adaptive_runtime_direct(
        model_spec(
            def(
                "wait_for_job_terminal",
                super::ToolAuditPolicy::TYPED_CANONICAL,
                ModelVisible,
                TOOL_CATEGORY_JOB,
                None,
                TOOL_PROVIDER_NATIVE,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Mutate,
                    risk: Read,
                    approval: super::ToolApprovalPolicy::None,
                    idempotency: super::ToolIdempotency::Keyed,
                },
                Some(RUNTIME_READ),
                false,
                NoPath,
                false,
                false,
                super::ToolSessionEvidencePolicy::NONE,
            )
            .with_activity(
                super::ToolActivityPresentation::Transport,
                super::ToolActivityInteraction::Meaningful,
            ),
            "Arm one caller-owned bounded one-shot terminal attention for one exact existing public job_id. Exact keyed replay returns the same wait. This operation never starts, retries, stops, or replaces the Job; job_id remains execution identity and observation tokens are unrelated cursors. Terminal delivery contains only sparse identity/status/outcome facts, never logs. automatic_resume_available is true only when a real current Host carrier exists. If an MCP result supplies suggested_call for the Host continuation carrier, use it only while the wait is still waiting, only when no independent work remains, and yield/end the current model turn immediately after presentation; an already-triggered wait already belongs to the current turn and needs no follow-up carrier. Do not poll this wait; use observe_jobs only when explicit logs/details or recovery are needed.",
        ).with_gpt_action_description("Arm durable one-shot attention for an existing Job terminal transition. It never changes Job execution. Do not poll the wait; observe_jobs remains the explicit logs/details recovery tool."),
        79,
    ),
];

pub(super) const LISTING_DEFINITIONS: &[ToolDefinition] = &[
    adaptive_runtime_direct(
        model_spec(
            def(
                "list_jobs",
                super::ToolAuditPolicy::TYPED_CANONICAL.session_input(
                    super::ToolAuditSessionInputPolicy::OmitTopLevel(&["project", "session_id"]),
                ),
                ModelVisible,
                TOOL_CATEGORY_JOB,
                None,
                TOOL_PROVIDER_NATIVE,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Observe,
                    risk: Read,
                    approval: super::ToolApprovalPolicy::None,
                    idempotency: super::ToolIdempotency::PureRead,
                },
                Some(RUNTIME_READ),
                false,
                NoPath,
                false,
                false,
                super::ToolSessionEvidencePolicy::NONE,
            )
            .with_activity(
                super::ToolActivityPresentation::Support,
                super::ToolActivityInteraction::Meaningful,
            ),
            "Recovery and inventory primitive for caller-visible Jobs, not the normal continuation step. Do not call list_jobs when the initiating tool or current context already provides an exact job_id; continue that Job with observe_jobs instead. Use list_jobs when exact Job identity was lost, unknown_job explicitly requests inventory recovery, the user asks to enumerate background work, or multiple historical/parallel Jobs must be inspected. Exact project/session_id filters are preferred when known and combine with status using AND semantics. stdout/stderr bodies are never included; exact Job logs and continuation belong to observe_jobs.",
        ).with_gpt_action_description("Inventory caller-visible Jobs when exact identity is lost or enumeration is requested. Prefer exact project/session/status filters. If job_id is already known, continue with observe_jobs instead."),
        85,
    ),
    adaptive_runtime_direct(
        model_spec(
            def(
                "present_job_terminal_continuation",
                super::ToolAuditPolicy::typed_fields(&[
                    super::ToolAuditResultField::pointer("wait_id", "/job_terminal_continuation/wait_id"),
                    super::ToolAuditResultField::pointer("job_id", "/job_terminal_continuation/job_id"),
                    super::ToolAuditResultField::pointer("state", "/job_terminal_continuation/state"),
                    super::ToolAuditResultField::pointer("delivery_state", "/job_terminal_continuation/delivery_state"),
                    super::ToolAuditResultField::pointer("automatic_resume_available", "/job_terminal_continuation/automatic_resume_available"),
                    super::ToolAuditResultField::value("error_kind"),
                ]),
                ModelVisible,
                TOOL_CATEGORY_JOB,
                None,
                TOOL_PROVIDER_NATIVE,
                super::ToolSemanticContract {
                    effect: super::ToolEffect::Observe,
                    risk: Read,
                    approval: super::ToolApprovalPolicy::None,
                    idempotency: super::ToolIdempotency::PureRead,
                },
                Some(RUNTIME_READ),
                false,
                NoPath,
                false,
                false,
                super::ToolSessionEvidencePolicy::NONE,
            ),
            "Present one exact caller-owned still-waiting Job terminal wait as a bounded MCP App continuation card. Use this only as the final meaningful action when progress is blocked on that terminal transition; after successful presentation, yield/end the current model turn promptly so a later Host follow-up can create a fresh turn. An already-triggered wait should be handled in the current turn instead. Requires explicit wait_id, independently re-authorizes the wait and underlying Job visibility, never infers identity from Project, Session, ClientWindow, peer identity, credential, or recent activity, and never changes Job execution or terminal truth.",
        )
        .with_gpt_action_unsupported(),
        78,
    ),
    def(
        "job_terminal_continuation_bind",
        super::ToolAuditPolicy::typed_fields(&[
            super::ToolAuditResultField::pointer("wait_id", "/job_terminal_continuation/wait_id"),
            super::ToolAuditResultField::pointer("job_id", "/job_terminal_continuation/job_id"),
            super::ToolAuditResultField::pointer("delivery_state", "/job_terminal_continuation/delivery_state"),
            super::ToolAuditResultField::value("state_changed"),
            super::ToolAuditResultField::value("error_kind"),
        ]),
        ModelHidden,
        TOOL_CATEGORY_JOB,
        None,
        TOOL_PROVIDER_NATIVE,
        super::ToolSemanticContract {
            effect: super::ToolEffect::Mutate,
            risk: Read,
            approval: super::ToolApprovalPolicy::None,
            idempotency: super::ToolIdempotency::DesiredState,
        },
        Some(RUNTIME_READ),
        false,
        NoPath,
        false,
        false,
        super::ToolSessionEvidencePolicy::NONE,
    )
    .with_activity(
        super::ToolActivityPresentation::Transport,
        super::ToolActivityInteraction::NonMeaningful,
    ),
    def(
        "job_terminal_continuation_state",
        super::ToolAuditPolicy::typed_fields(&[
            super::ToolAuditResultField::pointer("wait_id", "/job_terminal_continuation/wait_id"),
            super::ToolAuditResultField::pointer("job_id", "/job_terminal_continuation/job_id"),
            super::ToolAuditResultField::pointer("state", "/job_terminal_continuation/state"),
            super::ToolAuditResultField::pointer("delivery_state", "/job_terminal_continuation/delivery_state"),
            super::ToolAuditResultField::value("error_kind"),
        ]),
        ModelHidden,
        TOOL_CATEGORY_JOB,
        None,
        TOOL_PROVIDER_NATIVE,
        super::ToolSemanticContract {
            effect: super::ToolEffect::Observe,
            risk: Read,
            approval: super::ToolApprovalPolicy::None,
            idempotency: super::ToolIdempotency::PureRead,
        },
        Some(RUNTIME_READ),
        false,
        NoPath,
        false,
        false,
        super::ToolSessionEvidencePolicy::NONE,
    )
    .with_activity(
        super::ToolActivityPresentation::Transport,
        super::ToolActivityInteraction::NonMeaningful,
    ),
    def(
        "job_terminal_continuation_prepare",
        super::ToolAuditPolicy::typed_fields(&[
            super::ToolAuditResultField::value("wait_id"),
            super::ToolAuditResultField::value("job_id"),
            super::ToolAuditResultField::value("attempt_id"),
            super::ToolAuditResultField::value("delivery_state"),
            super::ToolAuditResultField::value("dispatch_observation"),
            super::ToolAuditResultField::value("state_changed"),
            super::ToolAuditResultField::value("error_kind"),
        ]),
        ModelHidden,
        TOOL_CATEGORY_JOB,
        None,
        TOOL_PROVIDER_NATIVE,
        super::ToolSemanticContract {
            effect: super::ToolEffect::Mutate,
            risk: Read,
            approval: super::ToolApprovalPolicy::None,
            idempotency: super::ToolIdempotency::FencedReplay,
        },
        Some(RUNTIME_READ),
        false,
        NoPath,
        false,
        false,
        super::ToolSessionEvidencePolicy::NONE,
    )
    .with_activity(
        super::ToolActivityPresentation::Transport,
        super::ToolActivityInteraction::NonMeaningful,
    ),
    def(
        "job_terminal_continuation_finish",
        super::ToolAuditPolicy::typed_fields(&[
            super::ToolAuditResultField::value("wait_id"),
            super::ToolAuditResultField::value("job_id"),
            super::ToolAuditResultField::value("attempt_id"),
            super::ToolAuditResultField::value("delivery_state"),
            super::ToolAuditResultField::value("dispatch_observation"),
            super::ToolAuditResultField::value("state_changed"),
            super::ToolAuditResultField::value("error_kind"),
        ]),
        ModelHidden,
        TOOL_CATEGORY_JOB,
        None,
        TOOL_PROVIDER_NATIVE,
        super::ToolSemanticContract {
            effect: super::ToolEffect::Mutate,
            risk: Read,
            approval: super::ToolApprovalPolicy::None,
            idempotency: super::ToolIdempotency::FencedReplay,
        },
        Some(RUNTIME_READ),
        false,
        NoPath,
        false,
        false,
        super::ToolSessionEvidencePolicy::NONE,
    )
    .with_activity(
        super::ToolActivityPresentation::Transport,
        super::ToolActivityInteraction::NonMeaningful,
    ),
    def(
        "job_terminal_continuation_unbind",
        super::ToolAuditPolicy::typed_fields(&[
            super::ToolAuditResultField::pointer("wait_id", "/job_terminal_continuation/wait_id"),
            super::ToolAuditResultField::pointer("job_id", "/job_terminal_continuation/job_id"),
            super::ToolAuditResultField::pointer("delivery_state", "/job_terminal_continuation/delivery_state"),
            super::ToolAuditResultField::value("state_changed"),
            super::ToolAuditResultField::value("error_kind"),
        ]),
        ModelHidden,
        TOOL_CATEGORY_JOB,
        None,
        TOOL_PROVIDER_NATIVE,
        super::ToolSemanticContract {
            effect: super::ToolEffect::Mutate,
            risk: Read,
            approval: super::ToolApprovalPolicy::None,
            idempotency: super::ToolIdempotency::FencedReplay,
        },
        Some(RUNTIME_READ),
        false,
        NoPath,
        false,
        false,
        super::ToolSessionEvidencePolicy::NONE,
    )
    .with_activity(
        super::ToolActivityPresentation::Transport,
        super::ToolActivityInteraction::NonMeaningful,
    ),
    def(
        "job_tail",
        super::ToolAuditPolicy::TYPED_CANONICAL,
        ModelHidden,
        TOOL_CATEGORY_JOB,
        None,
        TOOL_PROVIDER_NATIVE,
        super::ToolSemanticContract {
            effect: super::ToolEffect::Observe,
            risk: Read,
            approval: super::ToolApprovalPolicy::None,
            idempotency: super::ToolIdempotency::PureRead,
        },
        Some(RUNTIME_READ),
        false,
        NoPath,
        false,
        false,
        super::ToolSessionEvidencePolicy::NONE,
    ),
];
