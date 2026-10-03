use super::*;

#[test]
fn tool_definition_source_has_no_module_wide_dead_code_allowance() {
    let source = include_str!("../tool_definition.rs");
    assert!(
        !source.contains("#![allow(dead_code)]"),
        "tool_definition.rs must not use a module-wide dead_code allowance"
    );
}

#[test]
fn tool_definitions_cover_known_names_and_public_specs() {
    let definition_names = tool_definitions()
        .map(|definition| definition.name)
        .collect::<BTreeSet<_>>();
    let definition_order = tool_definitions()
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    let known_names = known_tool_names().collect::<BTreeSet<_>>();
    let hidden_names = model_hidden_tool_names().collect::<BTreeSet<_>>();
    let definition_hidden_names = tool_definitions()
        .filter(|definition| definition.visibility.is_model_hidden())
        .map(|definition| definition.name)
        .collect::<BTreeSet<_>>();
    for name in known_tool_names() {
        assert!(
            lookup_tool_definition(name).is_some(),
            "{name} missing ToolDefinition lookup"
        );
    }
    assert_eq!(definition_names, known_names);
    assert_eq!(definition_order, known_tool_names().collect::<Vec<_>>());
    assert_eq!(hidden_names, definition_hidden_names);

    let specs = registered_tool_specs();
    let spec_names = specs
        .iter()
        .map(|spec| spec.name.as_str())
        .collect::<BTreeSet<_>>();
    let visible_definition_names = model_visible_tool_definitions()
        .map(|definition| definition.name)
        .collect::<BTreeSet<_>>();
    let spec_order = specs
        .iter()
        .map(|spec| spec.name.clone())
        .collect::<Vec<_>>();
    let visible_definition_order = model_visible_tool_definitions()
        .map(|definition| definition.name.to_string())
        .collect::<Vec<_>>();
    assert_eq!(spec_names, visible_definition_names);
    assert_eq!(visible_definition_order, spec_order);
    assert_eq!(registered_tool_names(), visible_definition_order);
}

#[cfg(feature = "experimental-code-mode")]
#[test]
fn experimental_code_mode_is_visible_read_only_and_feature_scoped() {
    let definition =
        lookup_tool_definition("execute_code_mode").expect("execute_code_mode definition");
    let metadata = definition.metadata();
    assert!(definition.visibility.is_model_visible());
    assert_eq!(metadata.effect, ToolEffect::Observe);
    assert_eq!(metadata.risk, ToolRisk::Read);
    assert_eq!(metadata.approval, ToolApprovalPolicy::None);
    assert_eq!(metadata.idempotency, ToolIdempotency::PureRead);
    assert_eq!(definition.adaptive_runtime_direct_rank(), Some(45));
    assert!(registered_tool_specs()
        .iter()
        .any(|spec| spec.name == "execute_code_mode"));
    assert_eq!(definition.category, TOOL_CATEGORY_RUNTIME);
    for intent in ["coding", "audit", "exploration"] {
        assert!(
            TOOL_MANIFEST_INTENTS
                .iter()
                .find(|profile| profile.name == intent)
                .unwrap()
                .tools
                .contains(&"execute_code_mode"),
            "{intent}"
        );
    }
    assert!(is_adaptive_runtime_direct_tool("execute_code_mode"));
}

#[cfg(feature = "experimental-code-mode")]
#[test]
fn experimental_code_mode_effectful_has_conservative_e2a_envelope() {
    let definition = lookup_tool_definition("execute_effectful_code_mode")
        .expect("execute_effectful_code_mode definition");
    let metadata = definition.metadata();
    assert!(definition.visibility.is_model_visible());
    assert_eq!(metadata.effect, ToolEffect::Execute);
    assert_eq!(metadata.risk, ToolRisk::JobRun);
    assert_eq!(metadata.approval, ToolApprovalPolicy::Standard);
    assert_eq!(metadata.idempotency, ToolIdempotency::NonIdempotent);
    assert_eq!(definition.adaptive_runtime_direct_rank(), Some(105));
    assert!(definition.requires_explicit_business_session());
    assert_eq!(
        runtime_tool_composition_policy("execute_effectful_code_mode"),
        ToolCompositionPolicy::Denied,
        "Code Mode must never recursively compose itself"
    );
    assert!(registered_tool_specs()
        .iter()
        .any(|spec| spec.name == "execute_effectful_code_mode"));
    assert_eq!(definition.category, TOOL_CATEGORY_RUNTIME);
}

#[cfg(feature = "experimental-code-mode")]
#[test]
fn experimental_code_mode_mutating_has_conservative_e2c_combined_authority_envelope() {
    let definition = lookup_tool_definition("execute_mutating_code_mode")
        .expect("execute_mutating_code_mode definition");
    let metadata = definition.metadata();
    assert!(definition.visibility.is_model_visible());
    assert_eq!(metadata.effect, ToolEffect::Mutate);
    assert_eq!(metadata.risk, ToolRisk::ProjectWrite);
    assert_eq!(metadata.approval, ToolApprovalPolicy::Standard);
    assert_eq!(metadata.idempotency, ToolIdempotency::NonIdempotent);
    assert!(
        metadata.destructive,
        "E2c can create/edit/delete/rename through canonical edit_project_files"
    );
    assert_eq!(
        metadata.authority,
        ToolAuthorityPolicy::RequireAll(&[PROJECT_WRITE, JOB_RUN])
    );
    assert!(
        metadata.shell_like,
        "structured validation executes project build/test code"
    );
    assert_eq!(definition.permission_risk(), PERMISSION_RISK_WRITE);
    assert_eq!(definition.adaptive_runtime_direct_rank(), Some(65));
    assert!(definition.requires_explicit_business_session());
    assert_eq!(
        runtime_tool_composition_policy("execute_mutating_code_mode"),
        ToolCompositionPolicy::Denied,
        "Code Mode must never recursively compose itself"
    );
    assert!(registered_tool_specs()
        .iter()
        .any(|spec| spec.name == "execute_mutating_code_mode"));
    assert_eq!(definition.category, TOOL_CATEGORY_RUNTIME);
    assert!(CODING_INTENT_TOOL_NAMES.contains(&"execute_mutating_code_mode"));
    assert!(is_adaptive_runtime_direct_tool(
        "execute_mutating_code_mode"
    ));
}

#[cfg(feature = "experimental-code-mode")]
#[test]
fn code_mode_composition_policy_is_canonical_closed_and_independent_from_frontend_admission() {
    const E1_TOOLS: &[&str] = &[
        "read_files",
        "search_project_texts",
        "read_project_overview",
        "list_project_tracked_files",
        "get_git_status",
        "read_git_log",
        "read_git_diff_hunks",
        "read_git_review_summary",
        "read_workspace_changes",
    ];
    for name in E1_TOOLS {
        let definition = lookup_tool_definition(name).unwrap_or_else(|| panic!("missing {name}"));
        assert_eq!(
            runtime_tool_composition_policy(name),
            ToolCompositionPolicy::Parallel,
            "{name}"
        );
        let metadata = definition.metadata();
        assert_eq!(metadata.effect, ToolEffect::Observe, "{name}");
        assert_eq!(metadata.risk, ToolRisk::Read, "{name}");
    }

    for name in ["cargo_check", "cargo_test", "edit_project_files"] {
        assert_eq!(
            runtime_tool_composition_policy(name),
            ToolCompositionPolicy::Sequential,
            "{name}"
        );
    }

    for name in [
        "cargo_fmt",
        "run_process",
        "run_script",
        "run_shell",
        "run_job",
        "run_detached_process",
        "observe_jobs",
        "apply_patch",
        "write_project_file",
        "execute_code_mode",
        "execute_effectful_code_mode",
        "execute_mutating_code_mode",
    ] {
        assert_eq!(
            runtime_tool_composition_policy(name),
            ToolCompositionPolicy::Denied,
            "{name}"
        );
    }
    assert_eq!(
        runtime_tool_composition_policy("future_unknown_tool"),
        ToolCompositionPolicy::Denied
    );
}

#[cfg(not(feature = "experimental-code-mode"))]
#[test]
fn experimental_code_mode_is_absent_without_feature() {
    for name in [
        "execute_code_mode",
        "execute_effectful_code_mode",
        "execute_mutating_code_mode",
    ] {
        assert!(lookup_tool_definition(name).is_none(), "{name}");
        assert!(!known_tool_names().any(|known| known == name), "{name}");
        assert!(
            !registered_tool_specs().iter().any(|spec| spec.name == name),
            "{name}"
        );
        assert!(group_tool_names_by_category([name]).is_empty());
        assert!(
            TOOL_MANIFEST_INTENTS
                .iter()
                .all(|intent| !intent.tools.contains(&name)),
            "{name}"
        );
        assert!(
            TOOL_RECOMMENDED_FLOWS
                .iter()
                .all(|flow| !flow.tools.contains(&name)),
            "{name}"
        );
        assert!(!is_adaptive_runtime_direct_tool(name), "{name}");
    }
}

#[test]
fn final_changes_requires_the_typed_internal_posix_runner_capability() {
    for name in ["present_work_result", "read_changed_file_diff"] {
        let requirement = runtime_tool_runner_capability(name)
            .unwrap_or_else(|| panic!("{name} must require its real Runner execution capability"));
        assert_eq!(
            requirement,
            RunnerCapabilityRequirement::InternalPosixScript
        );
        assert_eq!(requirement.label(), "internal_posix_script");
        assert_eq!(
            requirement.registry_capabilities(),
            &["internal_posix_script"]
        );
    }
}

#[test]
fn tool_definitions_are_activity_semantics_ssot() {
    use ToolActivityInteraction::{Meaningful, NonMeaningful};
    use ToolActivityKind::{Edit, Navigate, None as NoKind, Read, Review, Run, Search, Test};
    use ToolActivityPresentation::{Support, Transport, Work};

    for (name, presentation, interaction, kind) in [
        ("read_files", Work, Meaningful, Read),
        ("search_project_texts", Work, Meaningful, Search),
        ("edit_project_files", Work, Meaningful, Edit),
        ("run_process", Work, Meaningful, Run),
        ("run_shell", Work, Meaningful, Run),
        ("cargo_test", Work, Meaningful, Test),
        ("cargo_check", Work, Meaningful, Test),
        ("read_git_review_summary", Work, Meaningful, Review),
        ("read_git_diff_hunks", Work, Meaningful, Review),
        ("read_workspace_changes", Work, Meaningful, Review),
        ("get_lsp_status", Work, Meaningful, Navigate),
        ("finish_coding_task", Work, Meaningful, Review),
        ("observe_jobs", Transport, Meaningful, NoKind),
        ("wait_for_job_readiness", Transport, Meaningful, NoKind),
        ("list_jobs", Support, Meaningful, NoKind),
        ("read_session_handoff", Support, Meaningful, NoKind),
        ("get_session_handoff_state", Support, NonMeaningful, NoKind),
        ("work_on_project", Support, Meaningful, NoKind),
        ("read_validation_summary", Support, Meaningful, NoKind),
        ("get_runtime_status", Support, NonMeaningful, NoKind),
        ("read_tool_manifest", Support, NonMeaningful, NoKind),
        ("sync_goal_plan", Transport, NonMeaningful, NoKind),
        (
            "bootstrap_agent_conversation",
            Transport,
            NonMeaningful,
            NoKind,
        ),
        ("consume_agent_wake", Transport, NonMeaningful, NoKind),
        (
            "get_agent_continuation_state",
            Transport,
            NonMeaningful,
            NoKind,
        ),
    ] {
        assert_eq!(
            runtime_tool_activity_semantics(name),
            ToolActivitySemantics {
                presentation,
                interaction,
                kind,
            },
            "{name}"
        );
    }

    for name in [
        "get_runtime_status",
        "list_tools",
        "list_runners",
        "list_projects",
        "read_tool_manifest",
        "read_tool_trace",
        "sync_goal_plan",
        "bootstrap_agent_conversation",
        "consume_agent_wake",
        "get_work_result_state",
        "send_work_result_message",
        "get_session_handoff_state",
        "get_agent_wait_state",
        "bind_agent_continuation",
        "recover_agent_continuation_endpoint",
        "get_agent_continuation_state",
        "acquire_agent_continuation_wake",
        "prepare_agent_continuation_wake",
        "finish_agent_continuation_wake",
        "unbind_agent_continuation",
        "bind_job_terminal_continuation",
        "get_job_terminal_continuation_state",
        "prepare_job_terminal_continuation",
        "finish_job_terminal_continuation",
        "unbind_job_terminal_continuation",
    ] {
        assert_eq!(
            runtime_tool_activity_interaction(name),
            NonMeaningful,
            "legacy non-meaningful behavior changed for {name}"
        );
    }

    assert_eq!(
        runtime_tool_activity_semantics("unknown_open_world_tool"),
        ToolActivitySemantics {
            presentation: Work,
            interaction: Meaningful,
            kind: NoKind,
        },
        "unknown names must preserve the legacy conservative meaningful default"
    );

    let hidden = lookup_tool_definition("read_job_tail").expect("read_job_tail definition");
    assert!(hidden.visibility.is_model_hidden());
    let hidden_activity = hidden.activity_semantics();
    assert_eq!(hidden_activity.presentation, Work);
    assert_eq!(hidden_activity.interaction, Meaningful);
}

#[test]
fn execution_selection_contract_is_canonical_closed_and_sparse() {
    use ToolExecutionContinuation::{ObserveJobs, SessionShell};
    use ToolExecutionForm::{
        NativeArgv, PersistentShellCommand, ShellCommand, StructuredValidation, TypedScript,
    };
    use ToolExecutionLifetime::{Runner, SessionShell as SessionShellLifetime, Supervisor};
    use ToolExecutionStart::{AsyncImmediate, ExistingSession, SyncFirst};

    let cases = [
        (
            "run_process",
            ToolExecutionContract::new(NativeArgv, Runner, SyncFirst, ObserveJobs),
        ),
        (
            "run_script",
            ToolExecutionContract::new(TypedScript, Runner, SyncFirst, ObserveJobs),
        ),
        (
            "run_shell",
            ToolExecutionContract::new(ShellCommand, Runner, SyncFirst, ObserveJobs),
        ),
        (
            "run_job",
            ToolExecutionContract::new(ShellCommand, Runner, AsyncImmediate, ObserveJobs),
        ),
        (
            "run_detached_process",
            ToolExecutionContract::new(NativeArgv, Supervisor, AsyncImmediate, ObserveJobs),
        ),
        (
            "execute_session_shell",
            ToolExecutionContract::new(
                PersistentShellCommand,
                SessionShellLifetime,
                ExistingSession,
                SessionShell,
            ),
        ),
        (
            "cargo_fmt",
            ToolExecutionContract::new(StructuredValidation, Runner, SyncFirst, ObserveJobs),
        ),
        (
            "cargo_check",
            ToolExecutionContract::new(StructuredValidation, Runner, SyncFirst, ObserveJobs),
        ),
        (
            "cargo_test",
            ToolExecutionContract::new(StructuredValidation, Runner, SyncFirst, ObserveJobs),
        ),
        (
            "go_test",
            ToolExecutionContract::new(StructuredValidation, Runner, SyncFirst, ObserveJobs),
        ),
    ];
    for (name, expected) in cases {
        let definition = lookup_tool_definition(name).unwrap_or_else(|| panic!("missing {name}"));
        assert_eq!(definition.execution, Some(expected), "{name}");
        assert_eq!(
            runtime_tool_execution_contract(name),
            Some(expected),
            "{name}"
        );
    }

    for name in [
        "read_files",
        "edit_project_files",
        "read_workspace_changes",
        "observe_jobs",
        "stop_job",
        "open_session_shell",
    ] {
        assert_eq!(runtime_tool_execution_contract(name), None, "{name}");
    }
}

#[test]
fn host_orchestration_hints_are_static_guidance_independent_from_nested_composition() {
    use ToolCompositionPolicy::{Denied, Parallel, Sequential};
    use ToolHostConcurrencyHint::{IndependentParallelRead, Sequential as HostSequential};

    let cases = [
        (
            "read_files",
            Parallel,
            IndependentParallelRead,
            Some("items"),
            false,
        ),
        (
            "search_project_texts",
            Parallel,
            IndependentParallelRead,
            Some("queries"),
            false,
        ),
        (
            "search_file_context",
            Denied,
            IndependentParallelRead,
            Some("queries"),
            true,
        ),
        (
            "get_git_status",
            Parallel,
            IndependentParallelRead,
            None,
            false,
        ),
        (
            "read_git_diff_hunks",
            Parallel,
            IndependentParallelRead,
            None,
            false,
        ),
        (
            "read_git_review_summary",
            Parallel,
            IndependentParallelRead,
            None,
            false,
        ),
        (
            "get_runtime_status",
            Denied,
            IndependentParallelRead,
            None,
            false,
        ),
        (
            "cargo_check",
            Sequential,
            HostSequential,
            Some("packages"),
            false,
        ),
        ("cargo_test", Sequential, HostSequential, None, false),
        (
            "edit_project_files",
            Sequential,
            HostSequential,
            Some("changes"),
            false,
        ),
        ("observe_jobs", Denied, HostSequential, Some("items"), false),
    ];

    for (name, composition, concurrency, native_batch_field, compound_preferred) in cases {
        let definition = lookup_tool_definition(name).unwrap_or_else(|| panic!("missing {name}"));
        assert_eq!(definition.composition, composition, "{name}");
        assert_eq!(
            definition.host_orchestration.concurrency, concurrency,
            "{name}"
        );
        assert_eq!(
            definition.host_orchestration.native_batch_field, native_batch_field,
            "{name}"
        );
        assert_eq!(
            definition.host_orchestration.compound_preferred, compound_preferred,
            "{name}"
        );
        assert_eq!(
            runtime_tool_host_orchestration_hint(name),
            definition.host_orchestration,
            "{name}"
        );
    }

    assert_eq!(
        runtime_tool_host_orchestration_hint("unknown_tool"),
        ToolHostOrchestrationHint::UNSPECIFIED
    );
}

#[test]
fn every_runtime_tool_has_an_explicit_fail_closed_audit_contract() {
    for definition in tool_definitions() {
        assert_eq!(
            runtime_tool_audit_policy(definition.name),
            Some(definition.audit_policy()),
            "{} audit policy must resolve only through ToolDefinition",
            definition.name
        );
        assert!(
            matches!(
                definition.audit_policy().request,
                ToolAuditRequestPolicy::Typed | ToolAuditRequestPolicy::TypedDropNullValues
            ),
            "{} request audit must use the typed canonical boundary",
            definition.name
        );
        if let ToolAuditResultPolicy::Fields(fields) = definition.audit_policy().result {
            assert!(
                !fields.is_empty(),
                "{} narrowed result audit must declare at least one bounded field",
                definition.name
            );
        }
        match definition.audit_policy().session_input {
            ToolAuditSessionInputPolicy::OmitTopLevel(fields) => assert!(
                !fields.is_empty(),
                "{} Session input omission policy must name at least one field",
                definition.name
            ),
            ToolAuditSessionInputPolicy::Bounded
            | ToolAuditSessionInputPolicy::SearchProjectTexts
            | ToolAuditSessionInputPolicy::SearchAndRead
            | ToolAuditSessionInputPolicy::ObserveJobs => {}
        }
        match definition.audit_policy().context {
            ToolAuditContextPolicy::ResultProjection => assert!(
                matches!(
                    definition.audit_policy().result,
                    ToolAuditResultPolicy::Fields(_)
                ),
                "{} may reuse Session context only from a bounded field result projection",
                definition.name
            ),
            ToolAuditContextPolicy::Fields(fields) => assert!(
                !fields.is_empty(),
                "{} Session context policy must declare at least one bounded field",
                definition.name
            ),
            ToolAuditContextPolicy::Omit
            | ToolAuditContextPolicy::WorkingTreeStatus
            | ToolAuditContextPolicy::TaskOutputs => {}
        }
        if definition.audit_policy().execution.detail == ToolAuditExecutionDetail::Omit {
            assert_eq!(
                definition.audit_policy().execution.shell,
                ToolAuditExecutionShell::Output,
                "{} omitted execution evidence must not declare synthetic shell provenance",
                definition.name
            );
        }
    }

    assert_eq!(
        runtime_tool_audit_policy("observe_coding_agent").map(|policy| policy.result),
        Some(ToolAuditResultPolicy::Semantic(
            ToolAuditSemanticResultPolicy::CodingAgentObservation
        ))
    );
    for name in ["commit_git_paths", "read_git_review_summary"] {
        assert_eq!(
            runtime_tool_audit_policy(name).map(|policy| policy.request),
            Some(ToolAuditRequestPolicy::TypedDropNullValues),
            "{name} must preserve legacy omission of invalid normalized commit values"
        );
    }
    assert_eq!(runtime_tool_audit_policy("unknown_open_world_tool"), None);
    assert_eq!(runtime_tool_audit_policy("start_coding_task"), None);
}

#[test]
fn stop_job_preserves_one_canonical_effect() {
    let definition = lookup_tool_definition("stop_job").unwrap();
    assert_eq!(
        tool_definitions()
            .filter(|item| item.name == "stop_job")
            .count(),
        1
    );
    assert_eq!(definition.adaptive_runtime_direct_rank(), None);

    assert_eq!(definition.metadata.effect, ToolEffect::Mutate);
    assert_eq!(definition.metadata.risk, ToolRisk::JobRun);
    assert_eq!(definition.metadata.approval, ToolApprovalPolicy::Standard);
    assert_eq!(
        definition.metadata.idempotency,
        ToolIdempotency::DesiredState
    );
    assert_eq!(
        definition.metadata.authority,
        ToolAuthorityPolicy::Require(JOB_RUN)
    );

    assert!(lookup_tool_definition("cancel_job").is_none());
    assert!(lookup_tool_definition("manage_jobs").is_none());
    let schema = input_schema_for_tool("stop_job");
    let properties = schema["properties"].as_object().unwrap();
    assert_eq!(properties.len(), 4);
    for key in ["project", "job_id", "session_id", "confirm"] {
        assert!(properties.contains_key(key));
    }
    for name in [
        "read_files",
        "search_project_texts",
        "get_git_status",
        "edit_project_files",
    ] {
        let schema = input_schema_for_tool(name);
        let properties = schema["properties"].as_object().unwrap();
        for forbidden in [
            "observe_job",
            "job_id",
            "cancel_job",
            "wait_job",
            "job_control",
        ] {
            assert!(
                !properties.contains_key(forbidden),
                "{name} gained {forbidden}"
            );
        }
    }
}

#[test]
fn agent_continuation_setup_descriptions_are_self_guiding_without_direct_expansion() {
    let description = |name: &str| {
        lookup_tool_definition(name)
            .unwrap_or_else(|| panic!("missing ToolDefinition {name}"))
            .model_spec
            .expect("retained domain description")
            .description
    };
    let create = description("create_agent_identity");
    assert!(create.contains("first setup step"));
    assert!(create.contains("rotate_agent_continuation_endpoint"));
    assert!(create.contains("present_agent_continuation"));
    let rotate = description("rotate_agent_continuation_endpoint");
    assert!(rotate.contains("first-time durable continuation setup"));
    assert!(rotate.contains("present_agent_continuation"));
    assert!(rotate.contains("does not establish a Host binding"));
    let present = description("present_agent_continuation");
    assert!(present.contains(
        "create_agent_identity -> rotate_agent_continuation_endpoint -> present_agent_continuation"
    ));
    assert!(present.contains("yield/end the current model turn promptly"));
    assert!(present.contains("production_auto_resume_available"));
    assert!(present.contains("not production auto-resume readiness"));

    assert_eq!(
        lookup_tool_definition("create_agent_identity")
            .unwrap()
            .adaptive_runtime_direct_rank(),
        None,
        "identity creation stays discoverable through the gateway rather than expanding Direct"
    );
    assert_eq!(
        lookup_tool_definition("present_agent_continuation")
            .unwrap()
            .adaptive_runtime_direct_rank(),
        None
    );
    assert_eq!(
        lookup_tool_definition("rotate_agent_continuation_endpoint")
            .unwrap()
            .adaptive_runtime_direct_rank(),
        None
    );
}

#[test]
fn retired_endpoint_name_is_absent_from_every_tool_surface() {
    assert!(lookup_tool_definition("attach_agent_endpoint").is_none());
    assert!(!known_tool_names().any(|name| name == "attach_agent_endpoint"));
    assert!(!registered_tool_specs()
        .iter()
        .any(|spec| spec.name == "attach_agent_endpoint"));
    let canonical = lookup_tool_definition("rotate_agent_continuation_endpoint").unwrap();
    assert!(canonical.visibility.is_model_visible());
    assert_eq!(canonical.adaptive_runtime_direct, None);
}

#[test]
fn inactive_continuation_surface_preserves_domain_definitions() {
    for (name, visible, category, effect) in [
        (
            "wait_for_agent_events",
            true,
            TOOL_CATEGORY_AGENT_WAIT,
            ToolEffect::Mutate,
        ),
        (
            "wait_for_job_terminal",
            true,
            TOOL_CATEGORY_JOB,
            ToolEffect::Mutate,
        ),
        (
            "present_agent_continuation",
            false,
            TOOL_CATEGORY_COMMUNICATION,
            ToolEffect::Observe,
        ),
        (
            "present_job_terminal_continuation",
            false,
            TOOL_CATEGORY_JOB,
            ToolEffect::Observe,
        ),
    ] {
        let definition = lookup_tool_definition(name).expect("retained continuation definition");
        assert_eq!(definition.visibility.is_model_visible(), visible, "{name}");
        assert_eq!(definition.adaptive_runtime_direct, None, "{name}");
        assert_eq!(definition.category, category, "{name}");
        assert_eq!(definition.metadata.effect, effect, "{name}");
        assert!(
            definition.model_spec.is_some(),
            "{name} keeps its domain specification"
        );
        assert_eq!(
            registered_tool_specs().iter().any(|spec| spec.name == name),
            visible,
            "{name}"
        );
        assert_eq!(
            definition.metadata.idempotency,
            if visible {
                ToolIdempotency::Keyed
            } else {
                ToolIdempotency::PureRead
            },
            "{name}"
        );
    }
}

#[test]
fn adaptive_direct_reason_is_independent_of_domain_and_authority() {
    use crate::tool_definition::{ToolAdaptiveDirectPolicy, ToolDirectReason};

    for (name, reason) in [
        ("read_files", ToolDirectReason::CoreWorkflow),
        ("inspect_project_artifact", ToolDirectReason::CoreWorkflow),
        ("import_host_files", ToolDirectReason::HostIntegration),
        ("present_work_result", ToolDirectReason::Presentation),
        ("present_goal_plan", ToolDirectReason::Presentation),
    ] {
        let definition = lookup_tool_definition(name).unwrap();
        assert_eq!(
            definition.adaptive_runtime_direct_reason(),
            Some(reason),
            "{name}"
        );
    }
    assert_eq!(
        lookup_tool_definition("inspect_project_artifact")
            .unwrap()
            .category,
        lookup_tool_definition("import_host_files")
            .unwrap()
            .category
    );
    for original in tool_definitions() {
        for reason in [
            ToolDirectReason::CoreWorkflow,
            ToolDirectReason::HostIntegration,
            ToolDirectReason::Presentation,
            ToolDirectReason::Continuation,
        ] {
            let mut changed = *original;
            // Only change an existing policy; never admit a hidden/gateway tool.
            changed.adaptive_runtime_direct =
                original
                    .adaptive_runtime_direct
                    .map(|policy| ToolAdaptiveDirectPolicy {
                        rank: policy.rank,
                        reason,
                    });
            assert_eq!(changed.category, original.category);
            assert_eq!(changed.visibility, original.visibility);
            assert_eq!(changed.runner_capability, original.runner_capability);
            assert_eq!(changed.policy, original.policy);
            assert_eq!(changed.audit, original.audit);
            assert_eq!(changed.effect_annotations(), original.effect_annotations());
            assert_eq!(changed.metadata.effect, original.metadata.effect);
            assert_eq!(changed.metadata.risk, original.metadata.risk);
            assert_eq!(changed.metadata.authority, original.metadata.authority);
            assert_eq!(changed.metadata.approval, original.metadata.approval);
            assert_eq!(changed.metadata.idempotency, original.metadata.idempotency);
            assert_eq!(
                changed.adaptive_runtime_direct_rank(),
                original.adaptive_runtime_direct_rank()
            );
        }
    }
    let model_specs = serde_json::to_string(&registered_tool_specs()).unwrap();
    for internal_key in [
        "\"adaptive_runtime_direct\"",
        "\"adaptive_runtime_direct_reason\"",
        "\"ToolDirectReason\"",
    ] {
        assert!(!model_specs.contains(internal_key), "{internal_key}");
    }
}

#[test]
fn adaptive_runtime_direct_declarations_are_visible_ranked_and_unique() {
    let mut seen_ranks = std::collections::BTreeMap::new();
    for definition in tool_definitions() {
        if definition.visibility.is_model_hidden() {
            assert_eq!(
                definition.adaptive_runtime_direct, None,
                "{}",
                definition.name
            );
        }
        let Some(policy) = definition.adaptive_runtime_direct else {
            assert_eq!(definition.adaptive_runtime_direct_rank(), None);
            assert_eq!(definition.adaptive_runtime_direct_reason(), None);
            continue;
        };
        assert_eq!(definition.adaptive_runtime_direct_rank(), Some(policy.rank));
        assert_eq!(
            definition.adaptive_runtime_direct_reason(),
            Some(policy.reason)
        );
        assert!(definition.visibility.is_model_visible());
        assert!(seen_ranks.insert(policy.rank, definition.name).is_none());
    }

    let derived = adaptive_runtime_direct_tool_definitions();
    assert!(!derived.is_empty());
    for pair in derived.windows(2) {
        assert!(pair[0].adaptive_runtime_direct_rank() < pair[1].adaptive_runtime_direct_rank());
    }
    assert_eq!(derived.len(), seen_ranks.len());
    let apply_patch = lookup_tool_definition("apply_patch").expect("apply_patch definition");
    assert!(apply_patch.visibility.is_model_hidden());
    assert_eq!(
        apply_patch.adaptive_runtime_direct_rank(),
        None,
        "specialized patching is exact-manifest gateway-only"
    );
    assert!(
        derived
            .iter()
            .any(|definition| definition.name == "edit_project_files"),
        "canonical ordinary edits must remain adaptive-direct"
    );

    for (name, expected_rank) in [
        ("import_host_files", 55),
        ("inspect_project_artifact", 56),
        ("write_job_input", 71),
        ("run_script", 74),
        ("run_shell", 75),
        ("observe_jobs", 80),
    ] {
        let definition = derived
            .iter()
            .copied()
            .find(|definition| definition.name == name)
            .unwrap_or_else(|| panic!("{name} must be adaptive-direct"));
        assert_eq!(
            definition.adaptive_runtime_direct_rank(),
            Some(expected_rank)
        );
    }

    for name in [
        "read_workspace_changes",
        "read_session_handoff",
        "rotate_agent_continuation_endpoint",
        "run_skill_resource",
        "list_jobs",
        "stop_job",
        "run_detached_process",
        "transfer_project_artifact",
        "check_workspace_hygiene",
        "finish_coding_task",
        "check_runner_config",
        "reload_runner_config",
        "manage_ssh_resource",
        "open_session_shell",
        "execute_session_shell",
        "get_session_shell_status",
        "close_session_shell",
        "save_project_artifact",
        "read_project_artifact_chunk",
        "begin_artifact_upload",
        "upload_artifact_chunk",
        "finish_artifact_upload",
        "abort_artifact_upload",
        "go_test",
        "read_git_diff_hunks",
        "read_git_review_summary",
    ] {
        let definition = lookup_tool_definition(name).expect("model-visible long-tail definition");
        assert!(definition.visibility.is_model_visible(), "{name}");
        assert_eq!(
            definition.adaptive_runtime_direct_rank(),
            None,
            "{name} should stay behind adaptive discovery/gateway"
        );
    }

    for (name, expected_rank, expected_authority) in [
        ("read_session_discussion_summary", 15, RUNTIME_READ),
        ("review_changes", 120, PROJECT_READ),
    ] {
        let definition = derived
            .iter()
            .copied()
            .find(|definition| definition.name == name)
            .unwrap_or_else(|| panic!("{name} must be adaptive-direct"));
        assert_eq!(
            definition.adaptive_runtime_direct_rank(),
            Some(expected_rank)
        );
        assert_eq!(definition.metadata.effect, ToolEffect::Observe);
        assert_eq!(definition.metadata.risk, ToolRisk::Read);
        assert_eq!(definition.metadata.approval, ToolApprovalPolicy::None);
        assert_eq!(definition.metadata.idempotency, ToolIdempotency::PureRead);
        assert_eq!(
            definition.metadata.authority,
            ToolAuthorityPolicy::Require(expected_authority)
        );
    }

    let observe_jobs = registered_tool_specs()
        .into_iter()
        .find(|spec| spec.name == "observe_jobs")
        .expect("observe_jobs ToolSpec");
    let list_jobs = registered_tool_specs()
        .into_iter()
        .find(|spec| spec.name == "list_jobs")
        .expect("list_jobs ToolSpec");
    assert!(observe_jobs
        .description
        .contains("do not call list_jobs first"));
    assert!(observe_jobs.description.contains("after_observation_token"));
    assert!(observe_jobs.description.contains("bounded wait_secs"));
    assert!(list_jobs
        .description
        .contains("Recovery and inventory primitive"));
    assert!(list_jobs
        .description
        .contains("retain that continuation and continue independent work"));
    assert!(list_jobs
        .description
        .contains("using observe_jobs only when logs/details/recovery are needed"));

    let git_review = registered_tool_specs()
        .into_iter()
        .find(|spec| spec.name == "read_git_review_summary")
        .expect("read_git_review_summary ToolSpec");
    assert!(git_review
        .description
        .contains("Summarize an exact committed Git range"));
    assert!(git_review
        .description
        .contains("Ordinary review uses review_changes"));
}

#[test]
fn turn_economy_descriptors_stay_converged_and_bounded() {
    let specs = registered_tool_specs();

    for name in ["run_process", "run_script", "run_shell"] {
        let spec = spec_named(&specs, name);
        for phrase in [
            "continue independent work",
            "observe_jobs only for",
            "bounded wait_for_job_readiness",
        ] {
            assert!(spec.description.contains(phrase), "{name}: {phrase}");
        }
        assert!(
            !spec.description.contains("Use observe_jobs later"),
            "{name}"
        );

        assert!(
            spec.description.contains("sparse terminal Job attention"),
            "{name}"
        );
    }

    for name in ["cargo_check", "cargo_test"] {
        let spec = spec_named(&specs, name);
        for phrase in [
            "continue independent work",
            "do not poll",
            "covered source",
            "final evidence freeze covered source",
        ] {
            assert!(spec.description.contains(phrase), "{name}: {phrase}");
        }
    }
    let cargo_fmt = spec_named(&specs, "cargo_fmt");
    for phrase in [
        "continue independent work",
        "do not poll",
        "evidence is stale",
        "freeze covered source",
    ] {
        assert!(
            cargo_fmt.description.contains(phrase),
            "cargo_fmt: {phrase}"
        );
    }

    let observe = spec_named(&specs, "observe_jobs");
    for phrase in [
        "logs/details/recovery",
        "not the default follow-up to execution_state=pending",
        "Never launches, retries",
    ] {
        assert!(
            observe.description.contains(phrase),
            "observe_jobs: {phrase}"
        );
    }
    let wait = spec_named(&specs, "wait_for_job_terminal");
    assert!(wait
        .description
        .contains("explicitly selected continuation workflow"));
    assert!(wait.description.contains("not a blocking wait"));
    assert!(wait.description.contains("never starts a model turn"));
    assert!(wait
        .description
        .contains("wait_for_job_readiness in the current turn"));
    let list = spec_named(&specs, "list_jobs");
    assert!(list
        .description
        .contains("Recovery and inventory primitive"));
    assert!(list
        .description
        .contains("not the normal continuation step"));
    assert!(list
        .description
        .contains("retain that continuation and continue independent work"));
    assert!(list
        .description
        .contains("observe_jobs only when logs/details/recovery are needed"));

    let edits = spec_named(&specs, "edit_project_files");
    for phrase in [
        "read_files",
        "expected_read_revision",
        "Exact edits fail closed on ambiguity",
        "replace_range",
        "from that snapshot",
        "preflight transactionally",
        "dry_run",
        "review_changes when Git review is useful",
        "task-appropriate validation",
        "outcome_unknown",
    ] {
        assert!(
            edits.description.contains(phrase),
            "edit_project_files: {phrase}"
        );
    }
    assert!(spec_named(&specs, "cargo_check")
        .description
        .contains("packages for a known set"));

    for name in [
        "run_process",
        "run_script",
        "run_shell",
        "observe_jobs",
        "wait_for_job_terminal",
        "list_jobs",
        "cargo_check",
        "cargo_test",
        "edit_project_files",
    ] {
        let spec = spec_named(&specs, name);
        assert!(
            spec.description.chars().count() <= MODEL_TOOL_DESCRIPTION_MAX_CHARS,
            "{name} canonical description budget"
        );
    }
}

#[test]
fn tool_definitions_drive_metadata_visibility_and_categories() {
    for definition in tool_definitions() {
        let metadata = definition.metadata();
        let facade_metadata = lookup_tool_metadata(definition.name)
            .copied()
            .unwrap_or_else(|| panic!("{} missing metadata facade entry", definition.name));
        assert_eq!(metadata, facade_metadata);
        assert_eq!(metadata.name, definition.name);
        assert_eq!(
            definition.visibility.is_model_hidden(),
            is_model_hidden_tool_name(definition.name)
        );
        assert_eq!(definition.category, runtime_tool_category(definition.name));
        assert_eq!(definition.metadata().authority, metadata.authority);
    }
}

#[test]
fn operator_extension_families_are_definition_owned_and_registry_derived() {
    use ToolOperatorExtensionFamily::{
        MemoryManagement, MemoryRuntime, SkillManagement, SkillRuntime, TraceDiagnostics,
    };

    let cases: &[(ToolOperatorExtensionFamily, &[&str])] = &[
        (SkillRuntime, &["list_skills", "read_skill_file"]),
        (
            SkillManagement,
            &[
                "list_skill_versions",
                "install_skill",
                "activate_skill",
                "remove_skill_revision",
            ],
        ),
        (MemoryRuntime, &["search_memory", "read_memory"]),
        (
            MemoryManagement,
            &[
                "set_memory",
                "delete_memory",
                "list_memory_scopes",
                "purge_memory_scope",
            ],
        ),
        (TraceDiagnostics, &["read_tool_trace"]),
    ];

    for (family, expected_names) in cases {
        for name in *expected_names {
            assert_eq!(runtime_tool_operator_extension_family(name), Some(*family));
            assert_eq!(
                lookup_tool_definition(name)
                    .and_then(|definition| definition.operator_extension_family),
                Some(*family)
            );
        }

        let actual_names = match family {
            SkillRuntime => skill_runtime_tool_specs(),
            SkillManagement => skill_management_tool_specs(),
            MemoryRuntime => memory_runtime_tool_specs(),
            MemoryManagement => memory_management_tool_specs(),
            TraceDiagnostics => operator_diagnostic_tool_specs(),
        }
        .into_iter()
        .map(|spec| spec.name)
        .collect::<Vec<_>>();
        assert_eq!(
            actual_names,
            expected_names
                .iter()
                .map(|name| (*name).to_string())
                .collect::<Vec<_>>()
        );
    }

    for definition in
        tool_definitions().filter(|definition| definition.operator_extension_family.is_some())
    {
        assert!(
            definition.visibility.is_model_hidden(),
            "{} operator extension must remain ModelHidden and surface-gated",
            definition.name
        );
    }

    let declared_names = tool_definitions()
        .filter(|definition| definition.operator_extension_family.is_some())
        .map(|definition| definition.name)
        .collect::<BTreeSet<_>>();
    let registry_names = stateless_operator_extension_tool_specs()
        .into_iter()
        .map(|spec| spec.name)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        declared_names,
        registry_names.iter().map(String::as_str).collect(),
        "registry operator-extension membership must be derived from ToolDefinition families"
    );

    assert_eq!(runtime_tool_operator_extension_family("run_shell"), None);
    assert_eq!(runtime_tool_operator_extension_family("unknown_tool"), None);
}

#[test]
fn run_skill_resource_contract_distinguishes_live_configured_and_managed_fences() {
    let specs = registered_tool_specs();
    let run_spec = spec_named(&specs, "run_skill_resource")
        .description
        .to_ascii_lowercase();
    for phrase in [
        "configured skills are live resources",
        "expected_definition_revision",
        "resource bytes are read at execution",
        "expected_package_revision",
        "immutable package",
        "package-relative helpers",
        "__file__",
        "requested project cwd",
    ] {
        assert!(
            run_spec.contains(phrase),
            "run_skill_resource ToolSpec must describe {phrase:?}: {run_spec}"
        );
    }
    assert!(
        !run_spec.contains("revision-fenced script"),
        "configured resources must not be described as pre-pinned immutable scripts: {run_spec}"
    );

    let extension_specs = stateless_operator_extension_tool_specs();
    let list_spec = spec_named(&extension_specs, "list_skills")
        .description
        .to_ascii_lowercase();
    assert!(list_spec.contains("webcodex does not modify configured skill roots"));
    assert!(list_spec.contains("run_skill_resource"));
    assert!(!list_spec.contains("configured live read-only skills"));

    let definition =
        lookup_tool_definition("run_skill_resource").expect("run_skill_resource definition");
    let model_description = definition
        .model_spec
        .expect("run_skill_resource model spec")
        .description
        .to_ascii_lowercase();
    for phrase in [
        "configured skills are live resources",
        "expected_definition_revision",
        "resource bytes are read at execution",
        "expected_package_revision",
        "immutable package",
        "package-relative helpers",
        "__file__",
        "requested project cwd",
    ] {
        assert!(
            model_description.contains(phrase),
            "run_skill_resource ToolDefinition must describe {phrase:?}: {model_description}"
        );
    }
}

#[cfg(feature = "experimental-code-mode")]
#[test]
fn code_mode_discovery_ranks_inspection_before_specialized_effects_without_changing_admission() {
    let position = |name| {
        CODING_INTENT_TOOL_NAMES
            .iter()
            .position(|candidate| *candidate == name)
            .unwrap()
    };
    assert!(position("execute_code_mode") < position("execute_mutating_code_mode"));
    assert!(position("edit_project_files") < position("execute_mutating_code_mode"));
    assert!(position("execute_code_mode") < position("execute_effectful_code_mode"));
    assert!(position("cargo_test") < position("execute_effectful_code_mode"));
    for name in [
        "execute_code_mode",
        "execute_effectful_code_mode",
        "execute_mutating_code_mode",
    ] {
        assert_eq!(
            lookup_tool_definition(name).unwrap().category,
            TOOL_CATEGORY_RUNTIME
        );
        assert!(is_adaptive_runtime_direct_tool(name));
        assert_eq!(
            runtime_tool_composition_policy(name),
            ToolCompositionPolicy::Denied
        );
    }
    let specs = registered_tool_specs();
    let read = specs
        .iter()
        .find(|spec| spec.name == "execute_code_mode")
        .unwrap();
    for phrase in [
        "direct tool for one simple observation",
        "Promise.all only for independent",
        "sequential inside one cell",
        "before text(value)",
        "never a raw-result dump",
        "outer-output limit",
    ] {
        assert!(read.description.contains(phrase), "{phrase}");
    }
}

#[test]
fn readiness_tool_is_sequential_outer_only() {
    let name = "wait_for_job_readiness";
    let definition = lookup_tool_definition(name).unwrap();
    assert_eq!(definition.composition, ToolCompositionPolicy::Denied);
    assert_eq!(
        definition.host_orchestration.concurrency,
        ToolHostConcurrencyHint::Sequential
    );
    assert_eq!(
        definition.host_orchestration.native_batch_field,
        Some("job_ids")
    );
    assert!(is_adaptive_runtime_direct_tool(name));
    assert!(!runtime_tool_supports_passive_job_attention(name));
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, name);
    assert_eq!(spec.input_schema["additionalProperties"], false);
    assert_eq!(
        spec.input_schema["properties"].as_object().unwrap().len(),
        3
    );
    assert_eq!(
        spec.input_schema["properties"]["wait_secs"]["maximum"],
        crate::tool_call::MAX_JOB_READINESS_WAIT_SECS
    );
    assert_eq!(spec.input_schema["properties"]["job_ids"]["maxItems"], 8);
    let description = spec.description.as_str();
    for phrase in [
        "join barrier",
        "currently-ready independent work",
        "Use any when one terminal Job can unlock a useful dependent branch",
        "use all only at a true join",
        "do not mechanically repeat the same-set wait",
        "largest safe remaining Host activation budget",
        "no fixed 10/15/20s slice",
        "Logs/details/recovery use observe_jobs",
        "No automatic next turn",
    ] {
        assert!(description.contains(phrase), "{phrase}: {description}");
    }
    let mode_description = spec.input_schema["properties"]["mode"]["description"]
        .as_str()
        .expect("readiness mode description");
    assert!(mode_description.contains("one terminal Job can unlock a useful dependent branch"));
    assert!(mode_description.contains("every blocked dependency is required"));
    let wait_description = spec.input_schema["properties"]["wait_secs"]["description"]
        .as_str()
        .expect("readiness wait description");
    assert!(wait_description.contains("largest safe value"));
    assert!(wait_description.contains("no fixed 10/15/20-second slice"));
    assert!(wait_description.contains("mechanically repeating the same wait"));
    for mode in ["any", "all"] {
        let args = json!({"job_ids":["wc_job_A","wc_job_A","wc_job_B"],"mode":mode,"wait_secs":12});
        crate::test_support::validate_schema_instance(&args, &spec.input_schema).unwrap();
        ToolCall::from_tool_name(name, args).unwrap();
    }
    for args in [
        json!({"job_ids":[],"mode":"any","wait_secs":1}),
        json!({"job_ids":["a"],"mode":"invalid","wait_secs":1}),
        json!({"job_ids":["a"],"mode":"any","wait_secs":0}),
        json!({"job_ids":["a"],"mode":"any","wait_secs":46}),
        json!({"job_ids":["a"],"mode":"any","wait_secs":1,"project":"foreign"}),
    ] {
        assert!(crate::test_support::validate_schema_instance(&args, &spec.input_schema).is_err());
    }
    let valid = json!({"success":true,"output":{"wait_state":"deadline","mode":"all","waited_ms":12000,"ready":[{"job_id":"a","status":"failed","outcome":"failed"}],"pending_job_ids":["b"]}});
    crate::test_support::validate_schema_instance(&valid, &spec.output_schema).unwrap();
    let mut with_sidecar = valid.clone();
    with_sidecar["output"]["operator_messages"] = json!({"messages":[],"ack":{"accepted_ids":[]}});
    crate::test_support::validate_schema_instance(&with_sidecar, &spec.output_schema).unwrap();
    crate::test_support::validate_schema_instance(
        &json!({"success":false,"output":null,"error":"unavailable"}),
        &spec.output_schema,
    )
    .unwrap();
    for forbidden in [
        "stdout",
        "stderr",
        "logs",
        "command",
        "command_summary",
        "path",
        "project",
        "observation_token",
        "recovery_kind",
        "suggested_call",
        "diagnostics",
    ] {
        let mut invalid = valid.clone();
        invalid["output"][forbidden] = json!("must not leak");
        assert!(
            crate::test_support::validate_schema_instance(&invalid, &spec.output_schema).is_err(),
            "{forbidden}"
        );
        let mut invalid = valid.clone();
        invalid["output"]["ready"][0][forbidden] = json!("must not leak");
        assert!(
            crate::test_support::validate_schema_instance(&invalid, &spec.output_schema).is_err(),
            "nested {forbidden}"
        );
    }
}

#[test]
fn project_build_is_gateway_visible_but_not_adaptive_direct() {
    let definition = lookup_tool_definition("project_build").expect("project_build definition");
    assert!(definition.visibility.is_model_visible());
    assert_eq!(definition.category, TOOL_CATEGORY_EXECUTION);
    assert_eq!(definition.adaptive_runtime_direct_rank(), None);
    assert!(!is_adaptive_runtime_direct_tool("project_build"));
    let requirement = runtime_tool_runner_capability("project_build")
        .expect("project_build must require its typed Runner capability");
    assert_eq!(requirement, RunnerCapabilityRequirement::ProjectBuild);
    assert_eq!(requirement.label(), "project_build_v1");
    assert_eq!(requirement.registry_capabilities(), &["project_build_v1"]);
    assert_eq!(
        runtime_tool_execution_contract("project_build").map(|contract| contract.form),
        Some(ToolExecutionForm::ProjectBuild)
    );
    assert_eq!(
        definition.audit_policy().execution,
        ToolAuditExecutionPolicy::DIRECT_ARGV_TEXT
    );
}
