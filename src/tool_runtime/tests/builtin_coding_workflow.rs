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
    assert_guidance_within_soft_target(&workflow["roles"]["independent_review"]["guidance"]);

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
    for pointer in [
        "/guidance",
        "/tool_strategy/guidance",
        "/roles/independent_review/guidance",
    ] {
        let mut relaxed = workflow.clone();
        *relaxed.pointer_mut(pointer).expect("guidance path") = overflow.clone();
        validate_schema_instance_for_test(&relaxed, &schema)
            .unwrap_or_else(|error| panic!("{pointer} retained ergonomic hard bound: {error}"));
    }

    let mut legacy_role = workflow.clone();
    let review_role = legacy_role["roles"]["independent_review"].clone();
    legacy_role["roles"]["implementation_owner"] = review_role;
    assert!(validate_schema_instance_for_test(&legacy_role, &schema).is_err());
}

#[test]
fn builtin_coding_workflow_defaults_cover_unnamed_tasks_without_granting_authority() {
    let workflow = builtin_coding_workflow_projection(Default::default());
    assert_eq!(
        workflow["version"],
        crate::tool_runtime::startup_brief::BUILTIN_CODING_WORKFLOW_VERSION
    );
    assert_eq!(workflow["authority"], "model_guidance_only");
    let role_selection = workflow["role_selection"].as_str().unwrap();
    assert!(role_selection.contains("Ordinary implementation uses default guidance"));
    assert!(role_selection
        .contains("Use independent_review only for an explicit independent review pass"));
    assert!(role_selection.contains("Roles never grant authority"));
    let defaults = workflow["guidance"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_str().unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    for boundary in [
        "concrete, reviewable completion",
        "guidance grants no authority",
        "Recovery/compaction/exact Session resume is continuation",
        "reuse still-current Git/read/validation/Job facts",
        "explicit action/target",
        "user answer/Job/validation/result",
        "continue independent work",
        "wait only on real dependencies",
        "For inspection, files/data/artifacts, diagnostics, or coding",
        "Coding maps cross-layer changes end to end",
        "compiler/schema/exhaustiveness failures",
        "avoid speculative redesign",
        "Validation failure is evidence, not queue cleanliness",
        "Reuse assertion_name",
        "outcome_unknown fails closed",
        "Development validation may overlap independent work",
        "covered-source edits make it stale for final evidence",
        "For closeout evidence, freeze source covered by final validation",
        "invalidate that evidence",
        "rerun the appropriate final validation",
        "one execution/Job",
        "exact continuation",
        "passive Job attention",
        "observe_jobs is for logs/details/recovery",
        "list_jobs is identity recovery",
        "no automatic next turn",
        "finish ready work",
        "After Rust stabilizes, format once",
    ] {
        assert!(defaults.contains(boundary), "missing guidance: {boundary}");
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
fn builtin_coding_workflow_review_does_not_implicitly_authorize_edits() {
    let workflow = builtin_coding_workflow_projection(Default::default());
    assert!(workflow["roles"]
        .as_object()
        .is_some_and(|roles| !roles.contains_key("implementation_owner")));
    let review = workflow["roles"]["independent_review"]["guidance"]
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
fn direct_strategy_prefers_structured_edits_and_coalesces_known_work() {
    let workflow = builtin_coding_workflow_projection(Default::default());
    assert_eq!(workflow["tool_strategy"]["profile"], "direct");
    let strategy = strategy_text(&workflow);
    for phrase in [
        "read_files → edit_project_files",
        "read_revision fence",
        "replace_range",
        "list_project_files/read_files",
        "binary files",
        "independently verify generated/report outputs",
        "Never use scripts to bypass edit_project_files revision fences",
        "read_files(items)",
        "search_project_texts(queries)",
        "search_file_context",
        "project_validate with scope.packages",
        "one edit_project_files batch",
        "result-dependent operations sequential",
        "direct primitive",
        "predetermined independent observations",
        "bounded targeted reads",
        "small files/count search",
        "Avoid ritual",
    ] {
        assert!(strategy.contains(phrase), "{phrase}");
        assert!(
            !workflow["guidance"].to_string().contains(phrase),
            "strategy leaked into shared guidance: {phrase}"
        );
    }
    let text = workflow.to_string().to_lowercase();
    for absent in ["code_mode", "code mode", "promise.all", "text(results)"] {
        assert!(!text.contains(absent), "{absent}");
    }
}

#[test]
fn shared_workflow_guides_general_project_tasks_and_optional_artifact_delivery() {
    let workflow = builtin_coding_workflow_projection(Default::default());
    let guidance = workflow["guidance"].to_string();
    for phrase in [
        "files/data/artifacts, diagnostics, or coding",
        "task-fit validation",
        "Git/Cargo/commit only when needed",
    ] {
        assert!(
            guidance.contains(phrase),
            "missing general workflow guidance: {phrase}"
        );
    }
    let closeout = workflow["model_protocol"]["normal_closeout"]
        .as_str()
        .expect("closeout guidance");
    for phrase in [
        "Artifact tasks",
        "pass outputs",
        "count/content/format checks",
        "read-only tasks without deliverables omit outputs",
    ] {
        assert!(
            closeout.contains(phrase),
            "missing artifact guidance: {phrase}"
        );
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
    for phrase in [
        "model guidance only",
        "read_files(items)",
        "search_project_texts(queries)",
        "project_validate with scope.packages",
        "one edit_project_files batch",
        "do not Promise.all same-kind micro-calls",
        "Independent cross-tool read-only observations",
        "Promise.allSettled",
        "partial evidence",
        "Promise.all for all-or-nothing",
        "search_file_context",
        "one Host cell",
        "Child-call completion alone is not a boundary",
        "mechanically determined",
        "Return to the model for",
        "semantic choice",
        "ambiguous result",
        "new user decision",
        "authority/permission",
        "effect uncertainty",
        "reread_required=true",
        "direct_retry_safe=false",
        "stops effectful replay",
        "not mutation retry authority",
        "outcome_unknown",
        "competing recovery",
        "unresolved mutation intent",
        "full ToolResults in the Host cell",
        "exact pending continuations",
        "observation_ref",
        "read_revision",
        "failure/recovery fields",
        "emit compact evidence",
        "execution_state=pending",
        "retain exact Job identity/continuation as fallback",
        "Job terminal does not imply mechanically_followable",
        "currently-ready independent calls",
        "observe_jobs heartbeat polling",
        "wait_for_job_readiness join barrier",
        "entire exact blocked set",
        "one terminal Job can unlock a useful branch",
        "all only when every blocked dependency is required",
        "Never per-Job waits, Promise.race",
        "largest safe value from the remaining Host activation budget",
        "45s maximum",
        "do not prefer fixed 10/15/20s slices",
        "recompute ready work and the blocked set",
        "do not mechanically repeat the same-set wait",
        "Run to quiescence",
        "freeze covered source",
        "invalidate evidence",
        "does not require WebCodex nested Code Mode",
        "not verified by WebCodex",
    ] {
        assert!(strategy.contains(phrase), "{phrase}");
    }
    direct.as_object_mut().unwrap().remove("tool_strategy");
    host.as_object_mut().unwrap().remove("tool_strategy");
    assert_eq!(direct, host);
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
fn code_mode_strategy_changes_only_guidance_and_teaches_compact_composition() {
    use crate::tool_runtime::tool_inputs::CodingGuidanceProfile;
    let mut direct = builtin_coding_workflow_projection(CodingGuidanceProfile::Direct);
    let mut composed = builtin_coding_workflow_projection(CodingGuidanceProfile::CodeMode);
    validate_schema_instance_for_test(&composed, &workflow_schema()).unwrap();
    assert_eq!(composed["tool_strategy"]["profile"], "code_mode");
    let strategy = strategy_text(&composed);
    for phrase in [
        "simple observation use a direct primitive",
        "prefer direct search_file_context",
        "multi-step related search/read observations",
        "read-only execute_code_mode",
        "soft heuristic",
        "bounded callable contract",
        "adaptive follow-up inside one cell",
        "sequential inside the cell",
        "small dependency DAG",
        "Promise.all for independent observations",
        "native batch shape",
        "avoidable micro-calls",
        "before text(...)",
        "Keep raw child ToolResults inside the cell",
        "do not batch calls then text(results)",
        "proactively before hitting",
        "Canonical mutation is the default",
        "structured validation is the default",
        "one guarded edit",
        "grants no capability or nested admission",
        "effect-certainty",
    ] {
        assert!(strategy.contains(phrase), "{phrase}");
    }
    direct.as_object_mut().unwrap().remove("tool_strategy");
    composed.as_object_mut().unwrap().remove("tool_strategy");
    assert_eq!(
        direct, composed,
        "one shared workflow, protocol and review role"
    );
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
fn sparse_pending_receipts_rely_on_complete_static_scheduling_guidance() {
    use crate::tool_runtime::tool_inputs::CodingGuidanceProfile;
    for profile in [
        CodingGuidanceProfile::Direct,
        CodingGuidanceProfile::HostCodeMode,
    ] {
        let workflow = builtin_coding_workflow_projection(profile);
        let text = workflow.to_string();
        for rule in [
            "continue independent work",
            "finish ready work",
            "wait_for_job_readiness join",
            "any may unblock a branch",
            "all requires every blocker",
            "never mechanically refill",
            "observe_jobs is for logs/details/recovery",
            "Never retry/redispatch",
            "no automatic next turn",
            "fallback_recovery",
        ] {
            assert!(
                text.contains(rule),
                "missing static pending guidance: {rule}"
            );
        }
        assert!(!workflow["guidance"]
            .to_string()
            .contains("wait_for_job_terminal"));
        assert!(!workflow["tool_strategy"]
            .to_string()
            .contains("wait_for_job_terminal"));
        if profile == CodingGuidanceProfile::HostCodeMode {
            for rule in [
                "join barrier",
                "Cell return is not turn completion",
                "do not mechanically repeat the same-set wait",
                "fallback_recovery never auto-runs",
            ] {
                assert!(text.contains(rule), "missing Host guidance: {rule}");
            }
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
