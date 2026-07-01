//! Session project-instructions tests: AGENTS.md loading, candidate paths, truncation.

use super::super::project_instructions;
use super::super::*;
use super::support::*;
use crate::shell_protocol::ShellClientCapabilities;
use std::fs;

#[tokio::test]
async fn start_session_without_project_instructions_when_no_candidate_exists() {
    // The agent is registered (so the project resolves) but no instruction
    // candidate file exists on the agent host. Every candidate file_read is
    // answered with a not-found error, the loader skips them all, and
    // start_session still succeeds with project_instructions.loaded=false.
    let runtime = runtime_with_agent_project("instr-empty");
    register_agent(
        &runtime,
        "instr-empty",
        None,
        ShellClientCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("instr-empty");
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::StartSession {
                        project: Some(project),
                        title: Some("empty".to_string()),
                        mode: SessionMode::Normal,
                        deny_write_tools: false,
                        deny_shell_tools: false,
                    },
                    None,
                )
                .await
        }
    });
    // Drive every candidate file_read in order; each fails with not-found.
    for expected_path in project_instructions::INSTRUCTION_CANDIDATE_PATHS {
        let req = next_agent_request_for_instance(&runtime, "instr-empty", "inst")
            .await
            .expect("each candidate should enqueue an agent file_read");
        assert_eq!(req.kind, "file_read");
        assert_eq!(
            req.path.as_deref(),
            Some(*expected_path),
            "candidates must be tried in fixed order"
        );
        complete_patch_agent_request(
            &runtime,
            "instr-empty",
            &req.request_id,
            1,
            "",
            "no such file or directory",
        )
        .await;
    }
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    let pi = &result.output["project_instructions"];
    assert_eq!(pi["loaded"], false);
    assert!(pi["files"].as_array().unwrap().is_empty());
    assert_eq!(pi["truncated"], false);
    assert_eq!(pi["max_total_chars"], 32 * 1024);
    assert_eq!(
        pi["candidate_paths"].as_array().unwrap().len(),
        project_instructions::INSTRUCTION_CANDIDATE_PATHS.len()
    );
    assert!(pi["note"]
        .as_str()
        .unwrap()
        .contains("project-local guidance only"));
}

#[tokio::test]
async fn start_session_loads_agents_md_from_agent_project() {
    let runtime = runtime_with_agent_project("instr-loader");
    register_agent(
        &runtime,
        "instr-loader",
        None,
        ShellClientCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("instr-loader");
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::StartSession {
                        project: Some(project),
                        title: Some("load instructions".to_string()),
                        mode: SessionMode::Normal,
                        deny_write_tools: false,
                        deny_shell_tools: false,
                    },
                    None,
                )
                .await
        }
    });
    // The loader tries AGENTS.md first; drive that single file_read.
    let req = next_agent_request_for_instance(&runtime, "instr-loader", "inst")
        .await
        .expect("instruction load should enqueue an agent file_read");
    assert_eq!(req.kind, "file_read");
    assert_eq!(req.path.as_deref(), Some("AGENTS.md"));
    complete_patch_agent_request(
        &runtime,
        "instr-loader",
        &req.request_id,
        0,
        "# Agent Guide\n\nRespect the AGENTS.md rules.\n",
        "",
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    let pi = &result.output["project_instructions"];
    assert_eq!(pi["loaded"], true);
    let files = pi["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["path"], "AGENTS.md");
    assert_eq!(files[0]["truncated"], false);
    assert!(files[0]["read_more"].is_null());
    assert!(
        files[0]["content"]
            .as_str()
            .unwrap()
            .contains("Respect the AGENTS.md rules."),
        "content should carry AGENTS.md body"
    );
    assert_eq!(files[0]["limit"], 400);
    assert_eq!(files[0]["start_line"], 1);
    assert!(pi["note"]
        .as_str()
        .unwrap()
        .contains("project-local guidance only"));
}

#[tokio::test]
async fn start_session_truncates_large_instruction_file() {
    let runtime = runtime_with_agent_project("instr-trunc");
    register_agent(
        &runtime,
        "instr-trunc",
        None,
        ShellClientCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("instr-trunc");
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::StartSession {
                        project: Some(project),
                        title: Some("trunc".to_string()),
                        mode: SessionMode::Normal,
                        deny_write_tools: false,
                        deny_shell_tools: false,
                    },
                    None,
                )
                .await
        }
    });
    let req = next_agent_request_for_instance(&runtime, "instr-trunc", "inst")
        .await
        .expect("instruction load should enqueue an agent file_read");
    assert_eq!(req.kind, "file_read");
    assert_eq!(req.path.as_deref(), Some("AGENTS.md"));
    // Simulate the agent returning MAX_LINES_PER_FILE + 1 lines for a file
    // that is larger than the per-file line cap.
    let body = (0..(project_instructions::MAX_LINES_PER_FILE + 1))
        .map(|i| format!("rule line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    complete_patch_agent_request(&runtime, "instr-trunc", &req.request_id, 0, &body, "").await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    let pi = &result.output["project_instructions"];
    assert_eq!(pi["loaded"], true);
    assert_eq!(pi["truncated"], true);
    let file = &pi["files"][0];
    assert_eq!(file["truncated"], true);
    let read_more = &file["read_more"];
    assert_eq!(read_more["path"], "AGENTS.md");
    assert_eq!(
        read_more["start_line"],
        project_instructions::MAX_LINES_PER_FILE + 1
    );
    assert_eq!(read_more["limit"], project_instructions::MAX_LINES_PER_FILE);
    // Kept content is capped at MAX_LINES_PER_FILE lines.
    assert_eq!(
        file["content"].as_str().unwrap().lines().count(),
        project_instructions::MAX_LINES_PER_FILE
    );
    assert_eq!(file["limit"], project_instructions::MAX_LINES_PER_FILE);
}

#[tokio::test]
async fn session_summary_returns_project_instructions_without_content() {
    let runtime = runtime_with_agent_project("instr-summary");
    register_agent(
        &runtime,
        "instr-summary",
        None,
        ShellClientCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("instr-summary");
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::StartSession {
                        project: Some(project),
                        title: Some("summary".to_string()),
                        mode: SessionMode::Normal,
                        deny_write_tools: false,
                        deny_shell_tools: false,
                    },
                    None,
                )
                .await
        }
    });
    let req = next_agent_request_for_instance(&runtime, "instr-summary", "inst")
        .await
        .expect("instruction load should enqueue an agent file_read");
    complete_patch_agent_request(
        &runtime,
        "instr-summary",
        &req.request_id,
        0,
        "secret project rule that must not leak into session_summary\n",
        "",
    )
    .await;
    let start_result = task.await.unwrap();
    assert!(start_result.success, "{:?}", start_result.error);
    let session_id = start_result.output["session_id"]
        .as_str()
        .unwrap()
        .to_string();

    let summary_result = runtime
        .dispatch_with_auth(
            ToolCall::SessionSummary {
                session_id: session_id.clone(),
                limit: Some(20),
            },
            None,
        )
        .await;
    assert!(summary_result.success, "{:?}", summary_result.error);
    let pi = &summary_result.output["project_instructions"];
    assert_eq!(pi["loaded"], true);
    let files = pi["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["path"], "AGENTS.md");
    // Summary-only: content must NOT be present on the file entry.
    assert!(
        files[0].get("content").is_none(),
        "session_summary project_instructions file must be summary-only"
    );
    assert!(files[0]["chars"].as_u64().is_some());
    assert_eq!(files[0]["truncated"], false);
    assert_eq!(
        pi["total_chars"],
        start_result.output["project_instructions"]["total_chars"]
    );
    // The instruction body must not leak anywhere in the summary output.
    let serialized = serde_json::to_string(&summary_result.output).unwrap();
    assert!(
        !serialized.contains("secret project rule"),
        "session_summary leaked instruction content: {serialized}"
    );
}

#[tokio::test]
async fn load_project_instructions_first_match_wins_locally() {
    // Direct unit-style test of the loader against a local project root so
    // the local read path and first-match-wins ordering are exercised
    // without driving an agent.
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("CLAUDE.md"),
        "# Claude\n\nclaude-local rules\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("agents.md"),
        "# lower agents\n\nignored because CLAUDE.md wins earlier? no, AGENTS.md is first\n",
    )
    .unwrap();
    // Note: AGENTS.md is absent, agents.md is present (2nd candidate),
    // CLAUDE.md is present (3rd candidate). First match wins => agents.md.
    let config = local_project_config(&dir.path().to_string_lossy());
    let runtime = test_runtime();
    let snapshot = runtime.load_project_instructions(&config).await;
    assert!(snapshot.loaded);
    assert_eq!(snapshot.files.len(), 1);
    assert_eq!(snapshot.files[0].path, "agents.md");
    assert!(snapshot.files[0].content.contains("lower agents"));
}

#[tokio::test]
async fn load_project_instructions_empty_when_no_candidates_exist() {
    let dir = tempfile::tempdir().unwrap();
    let config = local_project_config(&dir.path().to_string_lossy());
    let runtime = test_runtime();
    let snapshot = runtime.load_project_instructions(&config).await;
    assert!(!snapshot.loaded);
    assert!(snapshot.files.is_empty());
}
