
#[tokio::test]
async fn list_project_files_pages_complete_source_without_gap_or_duplicate() {
    let stdout = "zeta.txt\nsrc/\nREADME.md\nCargo.toml\n.alpha\n";

    let first = run_list_project_files_page("list-files-pages", stdout, 2, 0).await;
    assert!(first.success, "{:?}", first.error);
    assert_eq!(first.output["returned"], 2);
    assert_eq!(first.output["total_entries"], 5);
    assert_eq!(first.output["offset"], 0);
    assert_eq!(first.output["next_offset"], 2);
    assert_eq!(first.output["truncated"], true);

    let second = run_list_project_files_page(
        "list-files-pages",
        stdout,
        2,
        first.output["next_offset"].as_u64().unwrap() as usize,
    )
    .await;
    assert_eq!(second.output["returned"], 2);
    assert_eq!(second.output["next_offset"], 4);

    let final_page = run_list_project_files_page(
        "list-files-pages",
        stdout,
        2,
        second.output["next_offset"].as_u64().unwrap() as usize,
    )
    .await;
    assert_eq!(final_page.output["returned"], 1);
    assert_eq!(final_page.output["next_offset"], Value::Null);
    assert_eq!(final_page.output["truncated"], false);

    let reconstructed = first.output["entries"]
        .as_array()
        .unwrap()
        .iter()
        .chain(second.output["entries"].as_array().unwrap())
        .chain(final_page.output["entries"].as_array().unwrap())
        .map(|entry| entry["path"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        reconstructed,
        vec![".alpha", "Cargo.toml", "README.md", "src", "zeta.txt"]
    );

    let past_end = run_list_project_files_page("list-files-pages", stdout, 2, 99).await;
    assert!(past_end.success);
    assert_eq!(past_end.output["returned"], 0);
    assert_eq!(past_end.output["total_entries"], 5);
    assert_eq!(past_end.output["offset"], 99);
    assert_eq!(past_end.output["next_offset"], Value::Null);
    assert_eq!(past_end.output["truncated"], false);
}

#[tokio::test]
async fn list_project_files_fails_closed_when_runner_retained_source_is_incomplete() {
    let stdout = "[output truncated to last 262144 bytes]\nzeta.txt\nsrc/\n";
    let result = run_list_project_files_page("list-files-retained-tail", stdout, 200, 0).await;
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "source_incomplete");
    assert_eq!(
        result.output["reason_code"],
        "runner_result_retention_truncated"
    );
    assert!(result.output.get("next_offset").is_none());
    assert!(result.output.get("total_entries").is_none());
}

#[test]
fn parse_and_page_file_list_entries_is_sorted_gap_free_and_unicode_safe() {
    let long_name = format!("long-{}-终.rs", "x".repeat(180));
    let stdout = format!(
        "zeta.txt\nsrc/\nREADME [draft].md\n{}\n.alpha\n目录/\n",
        long_name
    );
    let all = parse_file_list_entries(&stdout, ".");
    assert_eq!(all.len(), 6);
    assert_eq!(
        all.iter()
            .map(|entry| entry["path"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![
            ".alpha",
            "README [draft].md",
            long_name.as_str(),
            "src",
            "zeta.txt",
            "目录",
        ]
    );
    assert_eq!(all[3]["kind"], "dir");
    assert_eq!(all[5]["kind"], "dir");

    let (first, next) = page_file_list_entries(&all, 0, 2);
    assert_eq!(next, Some(2));
    let (middle, next) = page_file_list_entries(&all, next.unwrap(), 2);
    assert_eq!(next, Some(4));
    let (final_page, next) = page_file_list_entries(&all, next.unwrap(), 2);
    assert_eq!(next, None);
    let reconstructed = first
        .into_iter()
        .chain(middle)
        .chain(final_page)
        .map(|entry| entry["path"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        reconstructed,
        all.iter()
            .map(|entry| entry["path"].as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    );
    let (past_end, next) = page_file_list_entries(&all, 99, 2);
    assert!(past_end.is_empty());
    assert_eq!(next, None);
}

#[test]
fn parse_file_list_entries_prepends_subpath_for_relative_paths() {
    let stdout = "main.rs\nlib.rs\n";
    let entries = parse_file_list_entries(stdout, "src");
    let paths: Vec<&str> = entries
        .iter()
        .map(|e| e["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["src/lib.rs", "src/main.rs"]);
}

#[test]
fn validate_project_relative_path_rejects_absolute_and_parent_traversal() {
    assert!(validate_project_relative_path(".").is_ok());
    assert!(validate_project_relative_path("src").is_ok());
    assert!(validate_project_relative_path("src/main.rs").is_ok());
    assert!(validate_project_relative_path("/etc").is_err());
    assert!(validate_project_relative_path("../outside").is_err());
    assert!(validate_project_relative_path("src/../../outside").is_err());
    assert!(validate_project_relative_path("src\0main.rs").is_err());
}

#[test]
fn parse_search_matches_is_bounded_and_strips_dot_slash() {
    let stdout = "{\"webcodex_search\":{\"backend\":\"rg\"}}\n./src/main.rs:10:fn main() {}\n./src/lib.rs:3:pub fn x()\n./src/a:1:1\n";
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(2),
        ..raw_search_request()
    })
    .unwrap();
    let result = search_project_text_output("demo", &options, stdout, Some(0), "");
    let matches = result.output["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 2);
    assert_eq!(result.output["truncated"], true);
    assert_eq!(matches[0]["path"], "src/main.rs");
    assert_eq!(matches[0]["line"], 10);
    assert_eq!(matches[0]["preview"], "fn main() {}");
    assert_eq!(matches[0]["context_before"], json!([]));
    assert_eq!(matches[0]["context_after"], json!([]));
    assert_eq!(
        matches[0]["read_hint"],
        json!({"path": "src/main.rs", "start_line": 1, "limit": 80})
    );
    assert_eq!(matches[1]["path"], "src/lib.rs");
    assert_eq!(
        matches[1]["read_hint"],
        json!({"path": "src/lib.rs", "start_line": 1, "limit": 80})
    );
}

#[test]
fn search_match_read_hint_is_deterministic_and_centers_later_matches() {
    let stdout =
        "{\"webcodex_search\":{\"backend\":\"rg\"}}\nsrc/lib.rs:42:needle\nsrc/lib.rs:120:later\n";
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(10),
        ..raw_search_request()
    })
    .unwrap();
    let result = search_project_text_output("demo", &options, stdout, Some(0), "");
    let matches = result.output["matches"].as_array().unwrap();
    assert_eq!(
        matches[0]["read_hint"],
        json!({"path": "src/lib.rs", "start_line": 22, "limit": 80})
    );
    assert_eq!(
        matches[1]["read_hint"],
        json!({"path": "src/lib.rs", "start_line": 100, "limit": 80})
    );
}

#[test]
fn parse_search_matches_skips_lines_without_line_number() {
    // Binary file matches or malformed lines are skipped, not counted.
    let stdout = "{\"webcodex_search\":{\"backend\":\"rg\"}}\nbinary:file\nsrc/main.rs:5:hit\n";
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(10),
        ..raw_search_request()
    })
    .unwrap();
    let result = search_project_text_output("demo", &options, stdout, Some(0), "");
    let matches = result.output["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0]["path"], "src/main.rs");
}

#[test]
fn parse_search_matches_drops_claude_worktree_records() {
    let stdout = concat!(
        "{\"webcodex_search\":{\"backend\":\"native\"}}\n",
        ".claude/worktrees/stale/src/lib.rs:1:needle stale\n",
        "src/lib.rs:1:needle active\n",
    );
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(10),
        ..raw_search_request()
    })
    .unwrap();
    let result = search_project_text_output("demo", &options, stdout, Some(0), "");
    let matches = result.output["matches"].as_array().unwrap();

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0]["path"], "src/lib.rs");
    assert_eq!(matches[0]["preview"], "needle active");
}
