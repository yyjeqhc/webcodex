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
    let definition = lookup_tool_definition("code_mode_exec").expect("code_mode_exec definition");
    let metadata = definition.metadata();
    assert!(definition.visibility.is_model_visible());
    assert_eq!(metadata.effect, ToolEffect::Observe);
    assert_eq!(metadata.risk, ToolRisk::Read);
    assert_eq!(metadata.approval, ToolApprovalPolicy::None);
    assert_eq!(metadata.idempotency, ToolIdempotency::PureRead);
    assert_eq!(definition.adaptive_runtime_direct_rank(), Some(45));
    assert!(registered_tool_specs()
        .iter()
        .any(|spec| spec.name == "code_mode_exec"));
    assert!(TOOL_DISCOVERY_GROUPS
        .iter()
        .filter(|group| matches!(
            group.name,
            TOOL_DISCOVERY_GROUP_INSPECT | TOOL_DISCOVERY_GROUP_RUNTIME
        ))
        .all(|group| group.tools.contains(&"code_mode_exec")));
    for intent in ["coding", "audit", "exploration"] {
        assert!(
            TOOL_MANIFEST_INTENTS
                .iter()
                .find(|profile| profile.name == intent)
                .unwrap()
                .tools
                .contains(&"code_mode_exec"),
            "{intent}"
        );
    }
    assert!(is_adaptive_runtime_direct_tool("code_mode_exec"));
}

#[cfg(feature = "experimental-code-mode")]
#[test]
fn experimental_code_mode_effectful_has_conservative_e2a_envelope() {
    let definition = lookup_tool_definition("code_mode_exec_effectful")
        .expect("code_mode_exec_effectful definition");
    let metadata = definition.metadata();
    assert!(definition.visibility.is_model_visible());
    assert_eq!(metadata.effect, ToolEffect::Execute);
    assert_eq!(metadata.risk, ToolRisk::JobRun);
    assert_eq!(metadata.approval, ToolApprovalPolicy::Standard);
    assert_eq!(metadata.idempotency, ToolIdempotency::NonIdempotent);
    assert_eq!(definition.adaptive_runtime_direct_rank(), Some(105));
    assert!(definition.requires_explicit_business_session());
    assert_eq!(
        runtime_tool_composition_policy("code_mode_exec_effectful"),
        ToolCompositionPolicy::Denied,
        "Code Mode must never recursively compose itself"
    );
    assert!(registered_tool_specs()
        .iter()
        .any(|spec| spec.name == "code_mode_exec_effectful"));
    assert!(TOOL_DISCOVERY_GROUPS
        .iter()
        .find(|group| group.name == TOOL_DISCOVERY_GROUP_RUNTIME)
        .expect("runtime discovery group")
        .tools
        .contains(&"code_mode_exec_effectful"));
}

#[cfg(feature = "experimental-code-mode")]
#[test]
fn experimental_code_mode_mutating_has_conservative_e2c_combined_authority_envelope() {
    let definition = lookup_tool_definition("code_mode_exec_mutating")
        .expect("code_mode_exec_mutating definition");
    let metadata = definition.metadata();
    assert!(definition.visibility.is_model_visible());
    assert_eq!(metadata.effect, ToolEffect::Mutate);
    assert_eq!(metadata.risk, ToolRisk::ProjectWrite);
    assert_eq!(metadata.approval, ToolApprovalPolicy::Standard);
    assert_eq!(metadata.idempotency, ToolIdempotency::NonIdempotent);
    assert!(
        metadata.destructive,
        "E2c can create/edit/delete/rename through canonical apply_text_edits"
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
        runtime_tool_composition_policy("code_mode_exec_mutating"),
        ToolCompositionPolicy::Denied,
        "Code Mode must never recursively compose itself"
    );
    assert!(registered_tool_specs()
        .iter()
        .any(|spec| spec.name == "code_mode_exec_mutating"));
    assert!(TOOL_DISCOVERY_GROUPS
        .iter()
        .find(|group| group.name == TOOL_DISCOVERY_GROUP_RUNTIME)
        .expect("runtime discovery group")
        .tools
        .contains(&"code_mode_exec_mutating"));
    assert!(CODING_INTENT_TOOL_NAMES.contains(&"code_mode_exec_mutating"));
    assert!(is_adaptive_runtime_direct_tool("code_mode_exec_mutating"));
}

#[cfg(feature = "experimental-code-mode")]
#[test]
fn code_mode_composition_policy_is_canonical_closed_and_independent_from_frontend_admission() {
    const E1_TOOLS: &[&str] = &[
        "read_files",
        "search_project_texts",
        "project_overview",
        "list_project_tracked_files",
        "git_status",
        "git_log",
        "git_diff_hunks",
        "git_review_summary",
        "show_changes",
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

    for name in ["cargo_check", "cargo_test", "apply_text_edits"] {
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
        "code_mode_exec",
        "code_mode_exec_effectful",
        "code_mode_exec_mutating",
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
        "code_mode_exec",
        "code_mode_exec_effectful",
        "code_mode_exec_mutating",
    ] {
        assert!(lookup_tool_definition(name).is_none(), "{name}");
        assert!(!known_tool_names().any(|known| known == name), "{name}");
        assert!(
            !registered_tool_specs().iter().any(|spec| spec.name == name),
            "{name}"
        );
        assert!(
            TOOL_DISCOVERY_GROUPS
                .iter()
                .all(|group| !group.tools.contains(&name)),
            "{name}"
        );
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
    for name in ["present_work_result", "changes_file_diff"] {
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
        ("apply_text_edits", Work, Meaningful, Edit),
        ("run_process", Work, Meaningful, Run),
        ("run_shell", Work, Meaningful, Run),
        ("cargo_test", Work, Meaningful, Test),
        ("cargo_check", Work, Meaningful, Test),
        ("git_review_summary", Work, Meaningful, Review),
        ("git_diff_hunks", Work, Meaningful, Review),
        ("show_changes", Work, Meaningful, Review),
        ("lsp_status", Work, Meaningful, Navigate),
        ("finish_coding_task", Work, Meaningful, Review),
        ("observe_jobs", Transport, Meaningful, NoKind),
        ("list_jobs", Support, Meaningful, NoKind),
        ("session_handoff_summary", Support, Meaningful, NoKind),
        ("session_handoff_state", Support, NonMeaningful, NoKind),
        ("work_on_project", Support, Meaningful, NoKind),
        ("validation_summary", Support, Meaningful, NoKind),
        ("runtime_status", Support, NonMeaningful, NoKind),
        ("tool_manifest", Support, NonMeaningful, NoKind),
        ("goal_plan_sync", Transport, NonMeaningful, NoKind),
        (
            "bootstrap_agent_conversation",
            Transport,
            NonMeaningful,
            NoKind,
        ),
        ("consume_agent_wake", Transport, NonMeaningful, NoKind),
        ("agent_continuation_state", Transport, NonMeaningful, NoKind),
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
        "runtime_status",
        "list_tools",
        "list_runners",
        "list_projects",
        "tool_manifest",
        "read_tool_trace",
        "goal_plan_sync",
        "bootstrap_agent_conversation",
        "consume_agent_wake",
        "work_result_state",
        "work_result_send_message",
        "session_handoff_state",
        "agent_wait_state",
        "agent_continuation_bind",
        "agent_continuation_recover_endpoint",
        "agent_continuation_state",
        "agent_continuation_wake_acquire",
        "agent_continuation_wake_prepare",
        "agent_continuation_wake_finish",
        "agent_continuation_unbind",
        "job_terminal_continuation_bind",
        "job_terminal_continuation_state",
        "job_terminal_continuation_prepare",
        "job_terminal_continuation_finish",
        "job_terminal_continuation_unbind",
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

    let hidden = lookup_tool_definition("job_tail").expect("job_tail definition");
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
            "session_shell_exec",
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
        "apply_text_edits",
        "show_changes",
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
            "search_and_read",
            Denied,
            IndependentParallelRead,
            Some("queries"),
            true,
        ),
        ("git_status", Parallel, IndependentParallelRead, None, false),
        (
            "git_diff_hunks",
            Parallel,
            IndependentParallelRead,
            None,
            false,
        ),
        (
            "git_review_summary",
            Parallel,
            IndependentParallelRead,
            None,
            false,
        ),
        (
            "runtime_status",
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
            "apply_text_edits",
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
            ToolAuditContextPolicy::Omit | ToolAuditContextPolicy::WorkingTreeStatus => {}
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
        runtime_tool_audit_policy("coding_agent_observe").map(|policy| policy.result),
        Some(ToolAuditResultPolicy::Semantic(
            ToolAuditSemanticResultPolicy::CodingAgentObservation
        ))
    );
    for name in ["git_commit_paths", "git_review_summary"] {
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
fn stop_job_direct_exposure_preserves_one_canonical_effect_and_gateway_budget() {
    let definition = lookup_tool_definition("stop_job").unwrap();
    assert_eq!(
        tool_definitions()
            .filter(|item| item.name == "stop_job")
            .count(),
        1
    );
    assert_eq!(definition.adaptive_runtime_direct_rank(), Some(81));
    assert_eq!(
        definition.gpt_action_exposure(),
        ToolGptActionExposure::GatewayOnly
    );
    assert!(definition.supports_gpt_actions());
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
    assert!(!gpt_action_direct_tool_definitions()
        .iter()
        .any(|item| item.name == "stop_job"));
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
        "git_status",
        "apply_text_edits",
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
    let specs = registered_tool_specs();
    let description = |name: &str| {
        specs
            .iter()
            .find(|spec| spec.name == name)
            .unwrap_or_else(|| panic!("missing ToolSpec {name}"))
            .description
            .as_str()
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
        Some(18)
    );
    assert_eq!(
        lookup_tool_definition("rotate_agent_continuation_endpoint")
            .unwrap()
            .adaptive_runtime_direct_rank(),
        Some(19)
    );
    assert_eq!(
        lookup_tool_definition("attach_agent_endpoint")
            .unwrap()
            .adaptive_runtime_direct_rank(),
        None,
        "compatibility alias must not become a second canonical Direct entry"
    );
}

#[test]
fn adaptive_runtime_direct_declarations_are_visible_ranked_and_unique() {
    let mut seen_ranks = std::collections::BTreeMap::new();
    for definition in tool_definitions() {
        let Some(rank) = definition.adaptive_runtime_direct_rank() else {
            continue;
        };
        assert!(definition.visibility.is_model_visible());
        assert!(seen_ranks.insert(rank, definition.name).is_none());
    }

    let derived = adaptive_runtime_direct_tool_definitions();
    assert!(!derived.is_empty());
    for pair in derived.windows(2) {
        assert!(pair[0].adaptive_runtime_direct_rank() < pair[1].adaptive_runtime_direct_rank());
    }
    assert_eq!(derived.len(), seen_ranks.len());
    let apply_patch = lookup_tool_definition("apply_patch").expect("apply_patch definition");
    assert!(apply_patch.visibility.is_model_visible());
    assert_eq!(
        apply_patch.adaptive_runtime_direct_rank(),
        None,
        "specialized patching should stay ModelVisible but use Adaptive discovery/gateway"
    );
    assert!(
        derived
            .iter()
            .any(|definition| definition.name == "apply_text_edits"),
        "canonical ordinary edits must remain adaptive-direct"
    );

    for (name, expected_rank) in [
        ("rotate_agent_continuation_endpoint", 19),
        ("import_conversation_files_to_project", 55),
        ("project_artifact", 56),
        ("transfer_project_artifact", 57),
        ("run_detached_process", 72),
        ("run_script", 74),
        ("run_shell", 75),
        ("observe_jobs", 80),
        ("stop_job", 81),
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
        "workspace_hygiene_check",
        "finish_coding_task",
        "runner_config_check",
        "runner_config_reload",
        "ssh_resource",
        "open_session_shell",
        "session_shell_exec",
        "session_shell_status",
        "close_session_shell",
        "attach_agent_endpoint",
        "apply_patch",
        "save_project_artifact",
        "read_project_artifact",
        "artifact_upload_begin",
        "artifact_upload_chunk",
        "artifact_upload_finish",
        "artifact_upload_abort",
        "go_test",
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
        ("session_discussion_summary", 15, RUNTIME_READ),
        ("list_jobs", 85, RUNTIME_READ),
        ("git_diff_hunks", 125, PROJECT_READ),
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

    let gpt_action_direct = gpt_action_direct_tool_definitions();
    let expected_gpt_action_direct = derived
        .iter()
        .copied()
        .filter(|definition| {
            definition.supports_gpt_actions()
                && definition.gpt_action_exposure() != ToolGptActionExposure::GatewayOnly
        })
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    assert_eq!(
        gpt_action_direct
            .iter()
            .map(|definition| definition.name)
            .collect::<Vec<_>>(),
        expected_gpt_action_direct,
        "GPT Actions direct exposure must inherit Adaptive Direct ordering minus definition-owned unsupported/gateway-only exceptions"
    );
    assert!(gpt_action_tool_supported("apply_patch"));
    #[cfg(feature = "experimental-code-mode")]
    {
        assert!(gpt_action_tool_supported("code_mode_exec_effectful"));
        assert!(gpt_action_tool_supported("code_mode_exec_mutating"));
        for name in ["code_mode_exec_effectful", "code_mode_exec_mutating"] {
            assert!(
                !gpt_action_direct
                    .iter()
                    .any(|definition| definition.name == name),
                "{name} must stay GPT-Action-supported behind call_runtime_tool to preserve the OpenAPI operation budget"
            );
        }
    }
    assert!(
        !gpt_action_direct
            .iter()
            .any(|definition| definition.name == "apply_patch"),
        "apply_patch stays GPT-Action-supported long-tail behind call_runtime_tool"
    );
    for name in [
        "present_goal_plan",
        "present_agent_continuation",
        "present_work_result",
        "rotate_agent_continuation_endpoint",
    ] {
        assert!(
            !gpt_action_tool_supported(name),
            "{name} depends on MCP-only presentation/resource semantics"
        );
    }

    for definition in &gpt_action_direct {
        let model_spec = definition
            .model_spec
            .expect("model-visible direct tool spec");
        let action_description = definition
            .gpt_action_description()
            .expect("GPT Action description projection");
        assert!(
            action_description.chars().count() <= GPT_ACTION_DESCRIPTION_MAX_CHARS,
            "{} GPT Action description exceeds {} characters",
            definition.name,
            GPT_ACTION_DESCRIPTION_MAX_CHARS
        );
        if model_spec.description.chars().count() > GPT_ACTION_DESCRIPTION_MAX_CHARS {
            assert!(
                model_spec.gpt_action_description.is_some(),
                "{} needs an explicit short GPT Action presentation description",
                definition.name
            );
        }
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
        .find(|spec| spec.name == "git_review_summary")
        .expect("git_review_summary ToolSpec");
    assert!(git_review.description.contains("git_diff_hunks/read_files"));
    assert!(!git_review.description.contains("git_diff_hunks/read_file "));
}

#[test]
fn turn_economy_descriptors_stay_converged_and_bounded() {
    let specs = registered_tool_specs();

    for name in ["run_process", "run_script", "run_shell"] {
        let spec = spec_named(&specs, name);
        for phrase in [
            "continue independent work",
            "observe_jobs only for",
            "wait_for_job_terminal only when",
        ] {
            assert!(spec.description.contains(phrase), "{name}: {phrase}");
        }
        assert!(
            !spec.description.contains("Use observe_jobs later"),
            "{name}"
        );
        let action = lookup_tool_definition(name)
            .unwrap()
            .gpt_action_description()
            .expect("execution action description");
        assert!(!action.contains("Use observe_jobs later"), "{name}");
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
        .contains("only when no independent work remains"));
    assert!(wait
        .description
        .contains("explicit logs/details or recovery"));
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

    let edits = spec_named(&specs, "apply_text_edits");
    for phrase in [
        "expected_match_count=N",
        "exact cardinality is known",
        "optional dry_run",
        "dry_run is not ritual",
        "change_summary",
        "mechanical scope",
        "not semantic review",
        "show_changes",
        "git_diff_hunks",
        "git_review_summary",
    ] {
        assert!(
            edits.description.contains(phrase),
            "apply_text_edits: {phrase}"
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
        "apply_text_edits",
    ] {
        let spec = spec_named(&specs, name);
        assert!(
            spec.description.chars().count() <= MODEL_TOOL_DESCRIPTION_MAX_CHARS,
            "{name} canonical description budget"
        );
        let action = lookup_tool_definition(name)
            .unwrap()
            .gpt_action_description()
            .expect("model-facing action description");
        assert!(
            action.chars().count() <= GPT_ACTION_DESCRIPTION_MAX_CHARS,
            "{name} GPT Action description budget"
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
        (SkillRuntime, &["skill_list", "skill_read_file"]),
        (
            SkillManagement,
            &[
                "skill_versions",
                "skill_install",
                "skill_activate",
                "skill_remove_revision",
            ],
        ),
        (MemoryRuntime, &["memory_search", "memory_read"]),
        (
            MemoryManagement,
            &[
                "memory_set",
                "memory_delete",
                "memory_scope_list",
                "memory_scope_purge",
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
    let list_spec = spec_named(&extension_specs, "skill_list")
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
    let action_description = definition
        .gpt_action_description()
        .expect("run_skill_resource GPT Action description")
        .to_ascii_lowercase();
    for phrase in [
        "configured skills are live",
        "expected_definition_revision",
        "managed skills additionally require expected_package_revision",
    ] {
        assert!(
            action_description.contains(phrase),
            "run_skill_resource GPT Action description must describe {phrase:?}: {action_description}"
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
    assert!(position("code_mode_exec") < position("code_mode_exec_mutating"));
    assert!(position("apply_text_edits") < position("code_mode_exec_mutating"));
    assert!(position("code_mode_exec") < position("code_mode_exec_effectful"));
    assert!(position("cargo_test") < position("code_mode_exec_effectful"));
    for (name, group) in [
        ("code_mode_exec", TOOL_DISCOVERY_GROUP_INSPECT),
        ("code_mode_exec_effectful", TOOL_DISCOVERY_GROUP_VALIDATION),
        ("code_mode_exec_mutating", TOOL_DISCOVERY_GROUP_EDIT),
    ] {
        assert!(TOOL_DISCOVERY_GROUPS
            .iter()
            .find(|candidate| candidate.name == group)
            .unwrap()
            .tools
            .contains(&name));
        assert!(is_adaptive_runtime_direct_tool(name));
        assert_eq!(
            runtime_tool_composition_policy(name),
            ToolCompositionPolicy::Denied
        );
    }
    let specs = registered_tool_specs();
    let read = specs
        .iter()
        .find(|spec| spec.name == "code_mode_exec")
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
