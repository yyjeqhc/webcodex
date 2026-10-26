use crate::tool_runtime::registry;
use crate::tool_runtime::startup_brief::{
    builtin_coding_workflow_projection, validate_schema_instance_for_test,
    BUILTIN_CODING_WORKFLOW_GUIDANCE_TARGET_ITEMS,
    BUILTIN_CODING_WORKFLOW_GUIDANCE_TARGET_ITEM_CHARS,
};
use serde_json::{json, Value};

fn workflow_schema() -> Value {
    let schema = registry::coding_workflow_diagnostic_output_schema_for_test();
    schema["properties"]["output"]["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .find(|variant| variant["properties"]["detail"]["const"] == "standard")
        .unwrap()["properties"]["workflow"]
        .clone()
}

fn assert_guidance_within_soft_target(guidance: &Value) {
    let guidance = guidance.as_array().expect("guidance array");
    assert!(
        guidance.len() <= BUILTIN_CODING_WORKFLOW_GUIDANCE_TARGET_ITEMS,
        "built-in guidance exceeded soft item-count target"
    );
    for item in guidance {
        assert!(
            item.as_str().expect("guidance string").chars().count()
                <= BUILTIN_CODING_WORKFLOW_GUIDANCE_TARGET_ITEM_CHARS,
            "built-in guidance exceeded soft per-item character target"
        );
    }
}

#[test]
fn builtin_coding_workflow_defaults_are_required_but_soft_budgets_are_not_wire_limits() {
    let workflow = builtin_coding_workflow_projection(Default::default());
    assert!(workflow["model_protocol"]["context_sidecar"]
        .as_str()
        .unwrap()
        .contains("jobs.attention"));
    let schema = workflow_schema();
    validate_schema_instance_for_test(&workflow, &schema).unwrap();

    assert_guidance_within_soft_target(&workflow["guidance"]);
    assert_guidance_within_soft_target(&workflow["tool_strategy"]["guidance"]);

    let mut missing = workflow.clone();
    missing.as_object_mut().unwrap().remove("guidance");
    assert!(validate_schema_instance_for_test(&missing, &schema).is_err());

    let mut empty = workflow.clone();
    empty["guidance"] = json!([]);
    assert!(validate_schema_instance_for_test(&empty, &schema).is_err());

    let overflow = json!(vec![
        "x".repeat(
            BUILTIN_CODING_WORKFLOW_GUIDANCE_TARGET_ITEM_CHARS + 1
        );
        BUILTIN_CODING_WORKFLOW_GUIDANCE_TARGET_ITEMS + 1
    ]);
    for pointer in ["/guidance", "/tool_strategy/guidance"] {
        let mut relaxed = workflow.clone();
        *relaxed.pointer_mut(pointer).expect("guidance path") = overflow.clone();
        validate_schema_instance_for_test(&relaxed, &schema)
            .unwrap_or_else(|error| panic!("{pointer} retained ergonomic hard bound: {error}"));
    }

    let mut unexpected = workflow.clone();
    unexpected["unexpected"] = json!({});
    assert!(validate_schema_instance_for_test(&unexpected, &schema).is_err());
}

#[test]
fn builtin_workflow_defaults_are_shared_project_guidance_without_task_roles() {
    let workflow = builtin_coding_workflow_projection(Default::default());
    assert_eq!(
        workflow["version"],
        crate::tool_runtime::startup_brief::BUILTIN_CODING_WORKFLOW_VERSION
    );
    assert_eq!(workflow["authority"], "model_guidance_only");
    assert_eq!(
        workflow["guidance"],
        serde_json::to_value(crate::tool_runtime::guidance::core_guidance()).unwrap()
    );
    assert!(workflow.get("roles").is_none());
    assert!(workflow.get("role_selection").is_none());
    let route = workflow["model_protocol"]["coding_guidance"]
        .as_str()
        .unwrap();
    assert!(route.contains(crate::tool_runtime::guidance::CODING_GUIDANCE_CONTEXT_KEY));
    // Keep only safety-critical prose checks; contributor membership and task
    // applicability are tested as structured composition, not editorial phrases.
    let defaults = workflow["guidance"].to_string();
    for boundary in [
        "guidance grants no authority",
        "outcome_unknown fails closed",
    ] {
        assert!(defaults.contains(boundary), "{boundary}");
    }
}

#[test]
fn builtin_workflow_teaches_per_request_recording_independently_of_business_target() {
    use crate::tool_runtime::tool_inputs::CodingGuidanceProfile;

    let direct = builtin_coding_workflow_projection(CodingGuidanceProfile::Direct);
    let recording = direct["model_protocol"]["session_recording"]
        .as_str()
        .expect("recorder guidance");
    for boundary in [
        "session_ref as _wc.record on each ordinary call",
        "for that Session's recorder provenance",
        "canonical wc_sess_* remains valid",
        "business session_id may target another Session but does not select a recorder",
        "may target another Session",
        "_wc.record grants no authority",
        "Recorder is request-scoped, never sticky",
        "omitted calls are not backfilled",
    ] {
        assert!(
            recording.contains(boundary),
            "missing recorder boundary: {boundary}"
        );
    }
    assert!(!recording.contains("on the next ordinary"));

    let host = builtin_coding_workflow_projection(CodingGuidanceProfile::HostCodeMode);
    assert_eq!(host["model_protocol"]["session_recording"], recording);
    #[cfg(feature = "experimental-code-mode")]
    {
        let composed = builtin_coding_workflow_projection(CodingGuidanceProfile::CodeMode);
        assert_eq!(composed["model_protocol"]["session_recording"], recording);
    }
}

#[test]
fn builtin_coding_workflow_routes_persistent_shell_to_ssh_state_not_local_command_count() {
    let workflow = builtin_coding_workflow_projection(Default::default());
    let guidance = workflow["model_protocol"]["persistent_shell"]
        .as_str()
        .expect("persistent shell guidance");

    for boundary in [
        "run_process=literal argv",
        "run_shell=shell grammar/short chains",
        "run_script=program-like scripts",
        "specialize for added semantics",
        "repeated named-SSH state",
        "local same-process state",
    ] {
        assert!(
            guidance.contains(boundary),
            "missing routing boundary: {boundary}"
        );
    }
    assert!(!guidance.contains("For repeated commands in one Workflow Session"));
    assert!(!guidance.contains("structured tools -> run_process/run_script -> run_shell"));
}

#[test]
fn coding_chapter_review_does_not_implicitly_authorize_edits() {
    let chapter = crate::tool_runtime::guidance::coding_guidance_projection();
    assert_eq!(chapter["authority"], "model_guidance_only");
    let review = chapter["roles"]["independent_review"]["guidance"]
        .as_array()
        .unwrap();
    assert!(review.iter().any(|item| {
        let text = item.as_str().unwrap();
        text.contains("review-only")
            && text.contains("do not edit")
            && text.contains("only when the task authorizes corrections")
    }));
}

fn strategy_text(workflow: &Value) -> String {
    workflow["tool_strategy"]["guidance"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn direct_strategy_is_isolated_and_preserves_the_source_revision_boundary() {
    use crate::tool_runtime::tool_inputs::CodingGuidanceProfile;
    let workflow = builtin_coding_workflow_projection(CodingGuidanceProfile::Direct);
    assert_eq!(workflow["tool_strategy"]["profile"], "direct");
    assert!(workflow["tool_strategy"]
        .get("host_orchestration")
        .is_none());
    assert_eq!(
        workflow["tool_strategy"]["guidance"],
        serde_json::to_value(crate::tool_runtime::guidance::tool_strategy_guidance(
            CodingGuidanceProfile::Direct
        ))
        .unwrap()
    );
    let strategy = strategy_text(&workflow);
    assert!(strategy.contains("read_revision fence"));
    assert!(strategy.contains("Never use scripts to bypass edit_project_files revision fences"));
    assert_guidance_within_soft_target(&workflow["tool_strategy"]["guidance"]);
}

#[test]
fn optional_coding_chapter_is_bounded_and_does_not_select_a_tool_strategy() {
    let chapter = crate::tool_runtime::guidance::coding_guidance_projection();
    assert!(chapter.get("tool_strategy").is_none());
    assert!(chapter.get("guidance").is_none());
    let roles = chapter["roles"].as_object().unwrap();
    assert_eq!(roles.len(), 2);
    for name in ["coding", "independent_review"] {
        assert!(roles[name]["purpose"]
            .as_str()
            .is_some_and(|text| !text.is_empty()));
        assert_guidance_within_soft_target(&roles[name]["guidance"]);
    }
}

#[test]
fn host_code_mode_strategy_is_bounded_guidance_only() {
    use crate::tool_runtime::tool_inputs::CodingGuidanceProfile;
    let mut direct = builtin_coding_workflow_projection(CodingGuidanceProfile::Direct);
    let mut host = builtin_coding_workflow_projection(CodingGuidanceProfile::HostCodeMode);
    validate_schema_instance_for_test(&host, &workflow_schema()).unwrap();
    assert_guidance_within_soft_target(&host["tool_strategy"]["guidance"]);
    assert_eq!(host["tool_strategy"]["profile"], "host_code_mode");
    let strategy = strategy_text(&host);
    for boundary in [
        "reread_required=true",
        "direct_retry_safe=false",
        "stops effectful replay",
        "outcome_unknown",
        "fallback_recovery never auto-runs",
    ] {
        assert!(strategy.contains(boundary), "{boundary}");
    }
    direct.as_object_mut().unwrap().remove("tool_strategy");
    host.as_object_mut().unwrap().remove("tool_strategy");
    assert_eq!(direct, host);
}

#[test]
fn host_code_mode_guidance_preserves_attention_when_compacting_model_output() {
    use crate::tool_runtime::tool_inputs::CodingGuidanceProfile;
    let host = builtin_coding_workflow_projection(CodingGuidanceProfile::HostCodeMode);
    let direct = builtin_coding_workflow_projection(CodingGuidanceProfile::Direct);
    validate_schema_instance_for_test(&host, &workflow_schema()).unwrap();

    let strategy = strategy_text(&host);
    for field in [
        "session_attention",
        "operator_messages",
        "peer_messages",
        "job_attention",
    ] {
        assert!(
            strategy.contains(field),
            "missing Host attention guidance for {field}"
        );
        assert!(
            !strategy_text(&direct).contains(field),
            "Host-only output instructions leaked into Direct: {field}"
        );
    }
    assert!(strategy.contains("never drop"));

    // A Host projection is not a Server receipt. The protocol and Direct
    // workflow remain unchanged; only the Host tool strategy gains guidance.
    let mut host_without_strategy = host;
    let mut direct_without_strategy = direct;
    host_without_strategy
        .as_object_mut()
        .unwrap()
        .remove("tool_strategy");
    direct_without_strategy
        .as_object_mut()
        .unwrap()
        .remove("tool_strategy");
    assert_eq!(host_without_strategy, direct_without_strategy);
}

#[test]
fn host_code_mode_catalog_is_derived_from_tool_definition_hints_only() {
    use crate::tool_runtime::tool_definition::{
        model_visible_tool_definitions, ToolHostConcurrencyHint,
    };
    use crate::tool_runtime::tool_inputs::CodingGuidanceProfile;

    let direct = builtin_coding_workflow_projection(CodingGuidanceProfile::Direct);
    assert!(direct["tool_strategy"].get("host_orchestration").is_none());

    let host = builtin_coding_workflow_projection(CodingGuidanceProfile::HostCodeMode);
    let catalog = &host["tool_strategy"]["host_orchestration"];
    assert_eq!(catalog["guidance_only"], true);

    let mut native_batch_first = Vec::new();
    let mut independent_parallel_reads = Vec::new();
    let mut compound_preferred = Vec::new();
    let mut sequential = Vec::new();
    for definition in model_visible_tool_definitions() {
        let hint = definition.host_orchestration;
        if hint.native_batch_field.is_some() {
            native_batch_first.push(definition.name);
        }
        match hint.concurrency {
            ToolHostConcurrencyHint::IndependentParallelRead => {
                independent_parallel_reads.push(definition.name)
            }
            ToolHostConcurrencyHint::Sequential => sequential.push(definition.name),
            ToolHostConcurrencyHint::Unspecified => {}
        }
        if hint.compound_preferred {
            compound_preferred.push(definition.name);
        }
    }
    for values in [
        &mut native_batch_first,
        &mut independent_parallel_reads,
        &mut compound_preferred,
        &mut sequential,
    ] {
        values.sort_unstable();
    }

    assert_eq!(catalog["native_batch_first"], json!(native_batch_first));
    assert_eq!(
        catalog["independent_parallel_reads"],
        json!(independent_parallel_reads)
    );
    assert_eq!(catalog["compound_preferred"], json!(compound_preferred));
    assert_eq!(catalog["sequential"], json!(sequential));
}

#[cfg(feature = "experimental-code-mode")]
#[test]
fn code_mode_strategy_changes_only_guidance_and_keeps_canonical_effect_semantics() {
    use crate::tool_runtime::tool_inputs::CodingGuidanceProfile;
    let mut direct = builtin_coding_workflow_projection(CodingGuidanceProfile::Direct);
    let mut composed = builtin_coding_workflow_projection(CodingGuidanceProfile::CodeMode);
    validate_schema_instance_for_test(&composed, &workflow_schema()).unwrap();
    assert_eq!(composed["tool_strategy"]["profile"], "code_mode");
    assert_guidance_within_soft_target(&composed["tool_strategy"]["guidance"]);
    let strategy = strategy_text(&composed);
    for boundary in [
        "grants no capability or nested admission",
        "canonical Project/Session authority",
        "effect-certainty",
    ] {
        assert!(strategy.contains(boundary), "{boundary}");
    }
    direct.as_object_mut().unwrap().remove("tool_strategy");
    composed.as_object_mut().unwrap().remove("tool_strategy");
    assert_eq!(direct, composed);
}

#[test]
fn tool_strategy_schema_requires_one_known_profile_and_closed_shape() {
    let workflow = builtin_coding_workflow_projection(Default::default());
    let schema = workflow_schema();
    for strategy in [
        json!({}),
        json!({"profile":"unknown","guidance":["rule"]}),
        json!({"profile":"direct","guidance":[]}),
        json!({"profile":"direct","guidance":["rule"],"code_mode":{}}),
    ] {
        let mut invalid = workflow.clone();
        invalid["tool_strategy"] = strategy;
        assert!(validate_schema_instance_for_test(&invalid, &schema).is_err());
    }
}

#[test]
fn every_strategy_preserves_job_identity_recovery_and_no_redispatch() {
    use crate::tool_runtime::tool_inputs::CodingGuidanceProfile;
    for profile in [
        CodingGuidanceProfile::Direct,
        CodingGuidanceProfile::HostCodeMode,
        #[cfg(feature = "experimental-code-mode")]
        CodingGuidanceProfile::CodeMode,
    ] {
        let workflow = builtin_coding_workflow_projection(profile);
        let shared = workflow["guidance"].to_string();
        // These are execution-safety instructions; ordinary scheduling prose
        // and profile selection are covered by composition/catalog tests.
        for boundary in [
            "one execution/Job",
            "observe_jobs is for logs/details/recovery",
            "Never retry/redispatch",
            "fallback_recovery",
        ] {
            assert!(shared.contains(boundary), "{boundary}");
        }
    }
}

#[test]
fn bootstrap_guidance_schema_allows_semantic_headroom_but_keeps_a_ceiling() {
    use webcodex_core::runtime_contract::BUILTIN_BOOTSTRAP_GUIDANCE_MAX_CHARS;
    let schema = workflow_schema();
    let workflow = builtin_coding_workflow_projection(Default::default());
    for field in ["bootstrap_reuse", "bootstrap_observations"] {
        assert_eq!(
            schema["properties"]["model_protocol"]["properties"][field]["maxLength"],
            BUILTIN_BOOTSTRAP_GUIDANCE_MAX_CHARS
        );
        let mut at_bound = workflow.clone();
        at_bound["model_protocol"][field] = json!("x".repeat(BUILTIN_BOOTSTRAP_GUIDANCE_MAX_CHARS));
        validate_schema_instance_for_test(&at_bound, &schema).unwrap();
        at_bound["model_protocol"][field] =
            json!("x".repeat(BUILTIN_BOOTSTRAP_GUIDANCE_MAX_CHARS + 1));
        assert!(validate_schema_instance_for_test(&at_bound, &schema).is_err());
    }
    validate_schema_instance_for_test(&workflow, &schema).unwrap();
}
