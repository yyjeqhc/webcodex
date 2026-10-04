use super::*;

#[test]
fn instruction_trim_preserves_source_identity_and_exact_safe_continuations() {
    for body in [
        "",
        "line one\nline two",
        "line one\n",
        "规则😀\"\\\nsecond\n",
    ] {
        for budget in 0..=64 {
            let mut expected = String::new();
            for character in body.chars() {
                let candidate = format!("{expected}{character}");
                if serde_json::to_string(&candidate).unwrap().len() - 2 > budget {
                    break;
                }
                expected = candidate;
            }
            for scope in ["project", "runner"] {
                let mut source = json!({"source_scope":scope, "path":"AGENTS.md", "fingerprint":"exact",
                    "content":body,"headings":["# Keep"],"read_more":null,"truncated":false});
                trim_instruction_source(&mut source, budget);
                assert_eq!(source["content"], expected);
                assert_eq!(source["fingerprint"], "exact");
                assert_eq!(source["headings"], json!(["# Keep"]));
                assert_eq!(source["truncated"], true);
                if scope == "runner" {
                    assert!(source["read_more"].is_null());
                } else {
                    let observed = expected.lines().count().max(1);
                    let start = if expected.ends_with('\n') {
                        observed + 1
                    } else {
                        observed
                    };
                    assert_eq!(
                        source["read_more"],
                        json!({"path":"AGENTS.md","start_line":start,"limit":MAX_LINES_PER_FILE})
                    );
                }
            }
        }
    }
}

#[test]
fn instruction_projection_does_not_mutate_or_reobserve_the_input_snapshot() {
    use super::super::project_instructions::{InstructionSourceScope, LoadedInstructionCandidate};
    let current = ProjectInstructionsSnapshot::from_candidates(
        vec![LoadedInstructionCandidate {
            source_scope: InstructionSourceScope::Project,
            path: "AGENTS.md".into(),
            content: "# Rules\nretain exact input".into(),
            total_lines: 2,
            full_sha256: None,
        }],
        true,
    );
    let previous = current.to_summary();
    let before = serde_json::to_value(&current).unwrap();
    let projection = instructions_projection(&current, Some(&previous), false, true, false);
    assert_eq!(projection["status"], "reused");
    assert_eq!(projection["content_included"], false);
    let full = project_instructions_context_projection(&current, 20 * 1024);
    assert_eq!(full["status"], "loaded");
    assert_eq!(full["sources"][0]["content"], "# Rules\nretain exact input");
    assert_eq!(serde_json::to_value(&current).unwrap(), before);
}
