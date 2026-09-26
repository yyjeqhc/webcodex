use crate::tool_runtime::registry;
use crate::tool_runtime::startup_brief::{
    builtin_coding_workflow_projection, validate_schema_instance_for_test,
    BUILTIN_CODING_WORKFLOW_MAX_GUIDANCE_ITEMS,
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

#[test]
fn builtin_coding_workflow_defaults_are_required_and_bounded() {
    let workflow = builtin_coding_workflow_projection(Default::default());
    assert!(workflow["model_protocol"]["context_sidecar"]
        .as_str()
        .unwrap()
        .contains("jobs.attention"));
    let schema = workflow_schema();
    validate_schema_instance_for_test(&workflow, &schema).unwrap();

    let mut missing = workflow.clone();
    missing.as_object_mut().unwrap().remove("guidance");
    assert!(validate_schema_instance_for_test(&missing, &schema).is_err());

    for guidance in [
        json!([]),
        json!(vec!["rule"; BUILTIN_CODING_WORKFLOW_MAX_GUIDANCE_ITEMS + 1]),
        json!(["x".repeat(321)]),
    ] {
        let mut invalid = workflow.clone();
        invalid["guidance"] = guidance;
        assert!(validate_schema_instance_for_test(&invalid, &schema).is_err());
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
        "Ordinary implementation is default",
        "map cross-layer changes end to end",
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
        "wait_for_job_terminal only when terminal outcome is a true dependency",
        "no independent work remains",
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
        "canonical structured editors",
        "apply_text_edits for exact transactional edits",
        "expected_match_count=N",
        "apply_patch for patch-shaped changes",
        "run_script/Python for computation",
        "never use it to bypass revision/SHA fences",
        "read_files(items)",
        "search_project_texts(queries)",
        "search_and_read",
        "cargo_check(packages)",
        "one apply_text_edits batch",
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
fn host_code_mode_strategy_is_bounded_guidance_only() {
    use crate::tool_runtime::tool_inputs::CodingGuidanceProfile;
    let mut direct = builtin_coding_workflow_projection(CodingGuidanceProfile::Direct);
    let mut host = builtin_coding_workflow_projection(CodingGuidanceProfile::HostCodeMode);
    validate_schema_instance_for_test(&host, &workflow_schema()).unwrap();
    assert_eq!(host["tool_strategy"]["profile"], "host_code_mode");
    let strategy = strategy_text(&host);
    for phrase in [
        "model guidance only",
        "read_files(items)",
        "search_project_texts(queries)",
        "cargo_check(packages)",
        "one apply_text_edits batch",
        "do not Promise.all same-kind micro-calls",
        "Known independent cross-tool read-only observations",
        "Host Promise.allSettled",
        "partial evidence is useful",
        "Promise.all only for true all-or-nothing",
        "search_and_read",
        "one Host cell",
        "Do not return to the model merely because one child ToolResult arrived",
        "mechanically determined",
        "Natural model-turn boundaries",
        "semantic choice",
        "ambiguous result",
        "new user decision",
        "authority/permission",
        "outcome_unknown",
        "competing recovery",
        "unresolved mutation intent",
        "full ToolResults in the Host cell",
        "exact pending continuations",
        "observation_ref",
        "read_revision",
        "failure/recovery fields",
        "hide top-level Job bookkeeping",
        "text(JSON.stringify(fullResult))",
        "short dependency DAGs",
        "execution_state=pending",
        "finish independent calls",
        "terminal job_attention",
        "Observe only for details/recovery",
        "wait only when terminal outcome blocks all useful progress",
        "freeze covered source",
        "invalidate that evidence",
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
        "prefer direct search_and_read",
        "multi-step related search/read observations",
        "read-only code_mode_exec",
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
fn tool_strategy_schema_requires_one_known_bounded_profile() {
    let workflow = builtin_coding_workflow_projection(Default::default());
    let schema = workflow_schema();
    for strategy in [
        json!({}),
        json!({"profile":"unknown","guidance":["rule"]}),
        json!({"profile":"direct","guidance":[]}),
        json!({"profile":"direct","guidance":["x".repeat(321)]}),
        json!({"profile":"direct","guidance":vec!["rule"; 9]}),
        json!({"profile":"direct","guidance":["rule"],"code_mode":{}}),
    ] {
        let mut invalid = workflow.clone();
        invalid["tool_strategy"] = strategy;
        assert!(validate_schema_instance_for_test(&invalid, &schema).is_err());
    }
}
