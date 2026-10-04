use super::*;
use webcodex_core::project_instructions::{InstructionSourceScope, LoadedInstructionCandidate};

fn snapshot(
    scope: InstructionSourceScope,
    prefix: &str,
    count: usize,
) -> ProjectInstructionsSnapshot {
    ProjectInstructionsSnapshot::from_candidates(
        (0..count)
            .map(|index| LoadedInstructionCandidate {
                source_scope: scope,
                path: format!("runner/{index}/{prefix}.md"),
                content: "rule".into(),
                total_lines: 1,
                full_sha256: None,
            })
            .collect(),
        true,
    )
}

#[test]
fn both_startup_hard_budget_passes_keep_runner_continuations_absent() {
    let workflow = builtin_coding_workflow_projection(CodingGuidanceProfile::Direct);
    #[cfg(feature = "experimental-code-mode")]
    let workflow = {
        let code = builtin_coding_workflow_projection(CodingGuidanceProfile::CodeMode);
        if serialized_len(&code) > serialized_len(&workflow) {
            code
        } else {
            workflow
        }
    };
    for force_below_floor in [false, true] {
        let mut brief = json!({
            "workflow": workflow,
            "padding": "",
            "instructions": {"sources": [{
                "source_scope": "runner", "path": "runner/0/rules.md",
                "fingerprint": "a".repeat(64), "content": "", "headings": [],
                "read_more": null, "truncated": false,
            }], "truncated": false}
        });
        if force_below_floor {
            let spare = STANDARD_STARTUP_HARD_MAX_BYTES - serialized_len(&brief) - 256;
            brief["padding"] = json!("p".repeat(spare));
        }
        brief["instructions"]["sources"][0]["content"] = json!("界\\\"\n".repeat(20_000));
        enforce_hard_size_limit(&mut brief);
        assert!(serialized_len(&brief) <= STANDARD_STARTUP_HARD_MAX_BYTES);
        let source = &brief["instructions"]["sources"][0];
        assert_eq!(source["truncated"], true);
        assert!(source["read_more"].is_null());
        if force_below_floor {
            assert!(
                json_string_payload_len(source["content"].as_str().unwrap())
                    < MIN_INSTRUCTION_CONTENT_JSON_BYTES
            );
        }
    }
}

#[test]
fn incomplete_global_scan_reports_confirmed_project_changes_without_false_global_removal() {
    let mut project = snapshot(InstructionSourceScope::Project, "unused", 1);
    project.files[0].path = "AGENTS.md".into();
    let previous = ProjectInstructionsSnapshot::with_runner_files(
        snapshot(InstructionSourceScope::Runner, "global", 1).files,
        project,
        true,
    );
    let current = ProjectInstructionsSnapshot::with_runner_files(
        Vec::new(),
        ProjectInstructionsSnapshot::empty(),
        false,
    )
    .retain_unavailable_scopes(Some(&previous));
    let projection =
        instructions_projection(&current, Some(&previous.to_summary()), false, false, false);
    assert_eq!(projection["status"], "unavailable");
    assert_eq!(projection["changed_sources"], json!(["AGENTS.md"]));
    assert_eq!(projection["content_included"], false);
    assert!(projection["sources"][0]["content"].is_null());
}

#[test]
fn instruction_sidecar_budget_preserves_sources_and_local_guidance() {
    let heading = format!("# {}\n", "h".repeat(158));
    let globals = ProjectInstructionsSnapshot::from_candidates(
        (0..16)
            .map(|index| LoadedInstructionCandidate {
                source_scope: InstructionSourceScope::Runner,
                path: format!("runner/{index}/global.md"),
                content: heading.repeat(6),
                total_lines: 6,
                full_sha256: None,
            })
            .collect(),
        true,
    );
    let locals = ProjectInstructionsSnapshot::from_candidates(
        INSTRUCTION_CANDIDATE_PATHS
            .iter()
            .map(|path| LoadedInstructionCandidate {
                source_scope: InstructionSourceScope::Project,
                path: (*path).into(),
                content: "specific project guidance".into(),
                total_lines: 1,
                full_sha256: None,
            })
            .collect(),
        true,
    );
    let combined = ProjectInstructionsSnapshot::with_runner_files(globals.files, locals, true);
    for budget in [19 * 1024, 12 * 1024] {
        let projection = project_instructions_context_projection(&combined, budget);
        assert!(serialized_len(&projection) <= budget);
        let sources = projection["sources"].as_array().unwrap();
        assert_eq!(sources.len(), 21);
        assert!(sources[..16]
            .iter()
            .all(|source| source["read_more"].is_null()));
        assert!(sources[16..]
            .iter()
            .all(|source| source["content"] == "specific project guidance"));
    }
}

#[test]
fn repository_root_agents_and_builtin_workflow_fit_standard_startup_without_truncation() {
    let body = include_str!("../../../AGENTS.md");
    let projected_body = body.trim_end_matches('\n');
    let snapshot = ProjectInstructionsSnapshot::from_candidates(
        vec![LoadedInstructionCandidate {
            source_scope: InstructionSourceScope::Project,
            path: "AGENTS.md".into(),
            content: body.into(),
            total_lines: body.lines().count(),
            full_sha256: None,
        }],
        true,
    );
    let instructions = instructions_projection(&snapshot, None, true, true, false);
    assert_eq!(instructions["truncated"], false);
    assert_eq!(instructions["sources"][0]["truncated"], false);
    assert_eq!(instructions["sources"][0]["content"], projected_body);
    assert!(instructions["sources"][0]["read_more"].is_null());

    for profile in [
        CodingGuidanceProfile::Direct,
        CodingGuidanceProfile::HostCodeMode,
    ] {
        let mut brief = json!({
            "workflow": builtin_coding_workflow_projection(profile),
            "instructions": instructions.clone(),
        });
        enforce_hard_size_limit(&mut brief);
        let bytes = serialized_len(&brief);
        assert!(
            bytes <= STANDARD_STARTUP_HARD_MAX_BYTES,
            "{profile:?}: {bytes}"
        );
        assert_eq!(brief["instructions"]["truncated"], false, "{profile:?}");
        assert_eq!(
            brief["instructions"]["sources"][0]["truncated"], false,
            "{profile:?}"
        );
        assert_eq!(
            brief["instructions"]["sources"][0]["content"], projected_body,
            "{profile:?}"
        );
        assert!(brief["instructions"]["sources"][0]["read_more"].is_null());
    }
}

#[test]
fn bootstrap_guidance_reuses_observations_with_explicit_freshness_exceptions() {
    let workflow = super::builtin_coding_workflow_projection(super::CodingGuidanceProfile::Direct);
    let guidance = workflow["model_protocol"].to_string();
    for phrase in [
        "AGENTS.md",
        "CLAUDE.md",
        "project.instructions",
        "_wc.context",
        "content_included=true",
        "do not immediately reread",
        "fingerprints/revisions",
        "exact source/range",
        "compaction",
        "context loss/restoration/uncertainty",
        "reuse both across tasks/Workflow Sessions",
        "stale workflow/deployment policy",
        "guidance data, not Host schemas",
        "do not poll it",
        "initial branch/HEAD/status observation",
        "mutation fences",
        "without get_lsp_status",
        "probe_timeout",
        "complete/sufficient startup Skills/Plugins catalog",
        "broader/refreshed discovery",
        "first suitable work_on_project",
        "A new Workflow Session does not imply fresh model context",
        "ClientWindow continuity does not prove retention",
        "include_extension_catalog=true/default",
        "Set false only while",
        "changed catalog revision/runtime",
        "truncated discovery",
        "missing relevant metadata",
        "user request",
        "load_skill before broad project discovery",
        "description materially matches the task",
        "Do not load unrelated Skills",
        "Skill guidance grants no authority",
    ] {
        assert!(guidance.contains(phrase), "missing {phrase}");
    }
}
