use super::*;
use crate::tool_runtime::search_and_read::compact_compound_search;

fn canonical_search_batch(count: usize) -> Value {
    json!({
        "project": "agent:fixture:project", "requested_count": count,
        "returned_count": count, "succeeded_count": count, "failed_count": 0,
        "output_truncated": false,
        "items": (0..count).map(|index| json!({
            "index": index, "success": true, "error": null,
            "output": {
                "project": "agent:fixture:project", "path": "src", "backend": "rg",
                "result_mode": "matches", "pattern_mode": "literal", "effective_timeout_secs": 29,
                "exit_code": 0, "context_before": 0, "context_after": 0, "count": 1,
                "truncated": false, "truncation_reason": null,
                "matches": [{"path": format!("src/file_{index}.rs"), "line": 42,
                    "preview": "exact source evidence", "context_before": [], "context_after": []}]
            }
        })).collect::<Vec<_>>()
    })
}

#[test]
fn compound_projection_preserves_all_matches_and_reports_paired_bytes() {
    let before = canonical_search_batch(4);
    let after = compact_compound_search(before.clone(), &[true; 4]);
    for index in 0..4 {
        assert_eq!(after["items"][index]["index"], index);
        for key in ["path", "line", "preview"] {
            assert_eq!(
                after["items"][index]["output"]["matches"][0][key],
                before["items"][index]["output"]["matches"][0][key]
            );
        }
        assert!(after["items"][index]["output"]["matches"][0]
            .get("context_before")
            .is_none());
    }
    let a = serde_json::to_vec(&before).unwrap().len();
    let b = serde_json::to_vec(&after).unwrap().len();
    println!("OUTPUT_PROJECTION_BENCH {{\"case\":\"four_search_queries_metadata_only\",\"before_bytes\":{a},\"after_bytes\":{b}}}");
    assert!(b < a);
    assert_eq!(compact_compound_search(after.clone(), &[true; 4]), after);
}

#[test]
fn compound_projection_full_result_preserves_source_and_measures_total_payload() {
    let search = canonical_search_batch(4);
    let reads = json!({
        "project": "agent:fixture:project", "requested_count": 4, "returned_count": 4,
        "succeeded_count": 4, "failed_count": 0, "output_truncated": false,
        "items": (0..4).map(|index| json!({
            "index": index, "path": format!("src/file_{index}.rs"), "success": true, "error": null,
            "output": {
                "text": (1..=80).map(|line| format!("{line} | source evidence for file {index}, unchanged line {line}")).collect::<Vec<_>>().join("\n"),
                "read_revision": 1000 + index, "start_line": 1, "end_line": 80,
                "returned_lines": 80, "total_lines": 160, "has_more": true,
            }
        })).collect::<Vec<_>>()
    });
    let before = json!({"success": true, "output": {
        "project": "agent:fixture:project", "search": search,
        "reads": reads, "read_request_count": 4, "coalesced_read_count": 4,
        "read_success": true, "read_error": null
    }});
    let mut after = before.clone();
    after["output"]["search"] =
        compact_compound_search(before["output"]["search"].clone(), &[true; 4]);
    after["output"]["reads"]
        .as_object_mut()
        .unwrap()
        .remove("project");
    assert_eq!(
        after["output"]["reads"]["items"],
        before["output"]["reads"]["items"]
    );
    let a = serde_json::to_vec(&before).unwrap().len();
    let b = serde_json::to_vec(&after).unwrap().len();
    println!("OUTPUT_PROJECTION_BENCH {{\"case\":\"four_files_320_source_lines_full_envelope\",\"before_bytes\":{a},\"after_bytes\":{b}}}");
    assert!(b < a);
}

#[test]
fn compound_projection_keeps_partial_failures_and_truncation_truth() {
    let mut before = canonical_search_batch(3);
    before["succeeded_count"] = json!(2);
    before["failed_count"] = json!(1);
    before["items"][1] = json!({"index": 1, "success": false, "output": {"reason_code": "not_found"}, "error": "missing file"});
    before["items"][2]["output"]["truncated"] = json!(true);
    before["items"][2]["output"]["truncation_reason"] = json!("limit");
    before["items"][2]["output"]["matches"][0]["context_after"] =
        json!([{"line": 43, "text": "nonempty context"}]);
    let after = compact_compound_search(before.clone(), &[true; 3]);
    assert_eq!(after["items"][1], before["items"][1]);
    assert_eq!(after["failed_count"], 1);
    for key in [
        "truncated",
        "truncation_reason",
        "backend",
        "exit_code",
        "effective_timeout_secs",
    ] {
        assert_eq!(
            after["items"][2]["output"][key],
            before["items"][2]["output"][key]
        );
    }
    assert_eq!(
        after["items"][2]["output"]["matches"][0]["context_after"],
        before["items"][2]["output"]["matches"][0]["context_after"]
    );
}

#[test]
fn compound_projection_keeps_explicit_timeout_and_batch_continuation() {
    let mut before = canonical_search_batch(2);
    before["items"][0]["output"]["effective_timeout_secs"] = json!(10);
    before["output_truncated"] = json!(true);
    before["suggested_call"] = json!({"follow_up_kind": "mechanically_followable", "tool": "search_project_texts", "arguments": {"project": "agent:fixture:project", "queries": [{"pattern": "remaining"}]}});
    let after = compact_compound_search(before.clone(), &[false, true]);
    assert_eq!(after["items"][0]["output"]["effective_timeout_secs"], 10);
    assert_eq!(after["output_truncated"], true);
    assert_eq!(after["suggested_call"], before["suggested_call"]);
}
