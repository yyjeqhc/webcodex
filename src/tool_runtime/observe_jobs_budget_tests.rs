//! Frozen-reference and boundedness tests for the Job observation batch packer.
//! Keep large fixtures out of the production module; no real Job logs persisted.

use super::*;
use serde_json::json;

// Frozen prior budget packer. It deliberately retains its deep clones and
// canonical serialized ToolResult sizing as the independent oracle.
fn legacy_serialized_batch_fits(output: &Value) -> bool {
    serialized_json_len(&ToolResult::ok(output.clone()))
        .map(|bytes| {
            bytes
                <= MAX_OBSERVE_JOBS_AGGREGATE_RESULT_BYTES
                    .saturating_sub(MODEL_RESULT_ENVELOPE_RESERVE_BYTES)
        })
        .unwrap_or(false)
}

fn legacy_apply_output_budget(
    requested_count: usize,
    completed: Vec<Value>,
    wake_reason: WakeReason,
    waited_ms: u64,
) -> Result<Value, String> {
    let mut returned = Vec::with_capacity(completed.len());
    let mut next_index = None;

    for item in completed {
        let index = item["index"].as_u64().unwrap_or(returned.len() as u64) as usize;
        let mut candidate_items = returned.clone();
        candidate_items.push(item.clone());
        let candidate = batch_output(
            requested_count,
            candidate_items,
            wake_reason,
            waited_ms,
            false,
            None,
        );
        if legacy_serialized_batch_fits(&candidate) {
            returned.push(item);
            continue;
        }

        let single = batch_output(
            requested_count,
            vec![item.clone()],
            wake_reason,
            waited_ms,
            false,
            None,
        );
        if legacy_serialized_batch_fits(&single) {
            next_index = Some(index);
            break;
        }

        let budget_failure =
            output_budget_failure_item(index, item["job_id"].as_str().unwrap_or_default().into());
        let mut candidate_items = returned.clone();
        candidate_items.push(budget_failure.clone());
        let candidate = batch_output(
            requested_count,
            candidate_items,
            wake_reason,
            waited_ms,
            false,
            None,
        );
        if !legacy_serialized_batch_fits(&candidate) {
            if returned.is_empty() {
                return Err(
                    "observe_jobs could not encode a bounded output-budget failure item".into(),
                );
            }
            next_index = Some(index);
            break;
        }
        returned.push(budget_failure);
    }

    Ok(batch_output(
        requested_count,
        returned,
        wake_reason,
        waited_ms,
        next_index.is_some(),
        next_index,
    ))
}

fn budget_parity_fixture(index: usize, bytes: usize, changed: bool, terminal: bool) -> Value {
    json!({
        "index": index,
        "job_id": format!("wc_job_benchmark_{index}"),
        "success": true,
        "output": {
            "changed": changed,
            "terminal": terminal,
            "meaningful_changed": changed,
            "status": if terminal { "completed" } else { "running" },
            "stdout_tail": "ASCII\n界🦀".repeat(bytes / 12),
            "stderr_tail": "\" \\ \u{0001}".repeat(5),
            "unknown_future_metadata": {
                "escaped": "\r\t\\",
                "number": u64::MAX
            }
        },
        "error_kind": null,
        "error": null
    })
}

#[test]
fn borrowed_job_result_sizing_matches_legacy_at_budget_boundaries() {
    let limit = MAX_OBSERVE_JOBS_AGGREGATE_RESULT_BYTES;
    let mut entries = vec![
        json!({}),
        Value::Null,
        json!(["Unicode 🌍", "line\nbreak"]),
        json!({"items": [], "wait": {"outcome": "immediate", "waited_ms": 0}}),
        batch_output(
            1,
            vec![budget_parity_fixture(0, 1, false, false)],
            WakeReason::Immediate,
            0,
            false,
            None,
        ),
    ];
    for bytes in [
        limit - 4_096,
        limit - MODEL_RESULT_ENVELOPE_RESERVE_BYTES - 2_048,
        limit - MODEL_RESULT_ENVELOPE_RESERVE_BYTES,
        limit,
        limit + 4_096,
    ] {
        entries.push(batch_output(
            1,
            vec![json!({
                "index": 0,
                "job_id": "wc_job_large",
                "success": true,
                "output": {"stdout_tail": "界".repeat(bytes / 3), "terminal": true},
                "error_kind": null,
                "error": null
            })],
            WakeReason::Terminal,
            15,
            false,
            None,
        ));
    }
    for output in &entries {
        let canonical = legacy_serialized_batch_fits(output);
        assert_eq!(
            serialized_batch_fits(output),
            canonical,
            "borrowed ToolResult serialization must preserve the exact hard cap"
        );
        let item_bytes = output
            .get("items")
            .and_then(Value::as_array)
            .and_then(|items| {
                items.iter().try_fold(0usize, |total, item| {
                    total.checked_add(serialized_json_len(item).ok()?)
                })
            });
        assert_eq!(
            serialized_batch_fits_with_item_sizes(output, item_bytes),
            canonical,
            "cached per-item JSON sizes must equal canonical whole-result sizing"
        );
    }
}

#[test]
fn in_place_job_budget_matches_frozen_packer_and_recovery_continuation() {
    let budget_limit = MAX_OBSERVE_JOBS_AGGREGATE_RESULT_BYTES;
    let cases = [
        Vec::new(),
        vec![budget_parity_fixture(0, 100, true, false)],
        (0..8)
            .map(|i| budget_parity_fixture(i, 12_000, i % 2 == 0, i == 7))
            .collect::<Vec<_>>(),
        (0..8)
            .map(|i| budget_parity_fixture(i, 95_000, i % 2 == 0, i == 7))
            .collect::<Vec<_>>(),
        vec![budget_parity_fixture(0, budget_limit + 4_096, false, false)],
        vec![
            budget_parity_fixture(0, 90_000, false, false),
            budget_parity_fixture(1, budget_limit + 4_096, true, false),
            budget_parity_fixture(2, 500, false, true),
        ],
        vec![
            budget_parity_fixture(0, 400_000, false, false),
            budget_parity_fixture(1, 200_000, true, false),
        ],
        vec![
            json!({"index": 0, "job_id": "wc_job_error", "success": false,
            "output": null, "error_kind": "unknown_job",
            "error": "unknown_job", "suggested_call": {
                "tool": "list_jobs", "follow_up_kind": "fallback_recovery",
                "arguments": {}
            }}),
            budget_parity_fixture(1, 3_000, true, true),
        ],
        vec![
            json!({"index": 7, "job_id": "wc_job_indexed", "success": true,
                "output": {"changed": "unknown", "terminal": false, "stdout_tail": "🙂"},
                "error": null}),
            json!({"job_id": "wc_job_no_index", "success": false,
                "output": null, "error_kind": "stale_token", "error": "stale token"}),
        ],
    ];
    let mut saw_partial = false;
    let mut saw_failure_item = false;
    let mut saw_complete = false;
    for (case_id, items) in cases.into_iter().enumerate() {
        let count = items.len();
        let originals = (0..count)
            .map(|index| ObserveJobsItem {
                page: false,
                job_id: format!("wc_job_benchmark_{index}"),
                after_observation_token: None,
                observation_ref: None,
            })
            .collect::<Vec<_>>();
        for wake in [
            WakeReason::Immediate,
            WakeReason::Updated,
            WakeReason::Terminal,
            WakeReason::ItemError,
            WakeReason::Timeout,
        ] {
            for waited in [0, 999] {
                let before = legacy_apply_output_budget(count, items.clone(), wake, waited);
                let after = apply_output_budget(count, items.clone(), wake, waited);
                assert_eq!(
                    after, before,
                    "case={case_id} wake={wake:?} waited={waited}: budget state changed"
                );
                let mut prior_output = before.expect("frozen observation packer");
                let mut candidate_output = after.expect("in-place observation packer");
                saw_partial |= candidate_output["output_truncated"] == true;
                saw_complete |= candidate_output["output_truncated"] == false;
                saw_failure_item |= candidate_output["items"].as_array().is_some_and(|entries| {
                    entries
                        .iter()
                        .any(|entry| entry["error_kind"] == "output_budget_exceeded")
                });
                add_actionable_batch_continuation(&mut prior_output, &originals, 40);
                add_actionable_batch_continuation(&mut candidate_output, &originals, 40);
                assert_eq!(
                    candidate_output, prior_output,
                    "case={case_id}: Job recovery continuation / token shape changed"
                );
                if let Some(suggested) = candidate_output.get("suggested_call") {
                    assert_eq!(suggested["tool"], "observe_jobs");
                    crate::tool_runtime::ToolCall::from_tool_name(
                        "observe_jobs",
                        suggested["arguments"].clone(),
                    )
                    .expect("bounded omitted Job follow-up must parse");
                }
            }
        }
    }
    assert!(
        saw_partial,
        "fixtures must exercise omitted whole Job observations"
    );
    assert!(saw_complete, "fixtures must exercise complete results");
    assert!(
        saw_failure_item,
        "fixtures must exercise oversized-job recovery item"
    );
}

#[test]
fn model_envelope_budget_counts_unicode_json_escapes_and_long_job_ids() {
    // Character length is not the serialized byte count: multibyte UTF-8
    // and escaped control characters make this a stricter envelope check.
    let long_id = format!("wc_job_{}", "x".repeat(180));
    let tail = "中文🦀\\\"quoted\\\"\n\t".repeat(6_500);
    let items = (0..8)
        .map(|index| {
            json!({
                "index": index,
                "job_id": if index == 0 {
                    long_id.clone()
                } else {
                    format!("wc_job_{index}")
                },
                "success": true,
                "output": {
                    "changed": index == 1,
                    "terminal": false,
                    "stdout_tail": tail,
                },
                "error_kind": null,
                "error": null,
            })
        })
        .collect::<Vec<_>>();

    let output = apply_output_budget(8, items, WakeReason::Updated, 0).unwrap();
    let count = output["returned_count"].as_u64().unwrap();
    assert!(count > 0 && count < 8, "test must exercise truncation");
    assert_eq!(output["output_truncated"], true);
    assert_eq!(output["next_index"], count);
    assert_eq!(output["items"][0]["job_id"], long_id);
    assert!(
        serde_json::to_vec(&ToolResult::ok(output)).unwrap().len()
            <= MAX_OBSERVE_JOBS_AGGREGATE_RESULT_BYTES
    );
}

#[test]
fn model_envelope_stays_bounded_after_actionable_continuation() {
    // Pack an almost-full prefix and verify the handler's later continuation
    // rewrite also fits. No consumer should need to parse partial JSON.
    let ids = (0..3)
        .map(|index| format!("wc_job_{index}_{}", "i".repeat(180)))
        .collect::<Vec<_>>();
    let items = ids
        .iter()
        .enumerate()
        .map(|(index, job_id)| {
            json!({
                "index": index,
                "job_id": job_id,
                "success": true,
                "output": {
                    "changed": false,
                    "terminal": false,
                    "stdout_tail": "x".repeat(230_000),
                },
                "error_kind": null,
                "error": null,
            })
        })
        .collect::<Vec<_>>();
    let originals = ids
        .iter()
        .map(|job_id| ObserveJobsItem {
            page: false,
            job_id: job_id.clone(),
            after_observation_token: None,
            observation_ref: None,
        })
        .collect::<Vec<_>>();

    let mut output = apply_output_budget(3, items, WakeReason::Immediate, 0).unwrap();
    assert_eq!(output["returned_count"], 2);
    assert_eq!(output["next_index"], 2);
    add_actionable_batch_continuation(&mut output, &originals, 40);
    assert!(output.get("next_index").is_none());
    assert_eq!(output["suggested_call"]["tool"], "observe_jobs");
    assert_eq!(
        output["suggested_call"]["arguments"]["items"][0]["job_id"],
        ids[2]
    );
    assert!(
        serde_json::to_vec(&ToolResult::ok(output)).unwrap().len()
            <= MAX_OBSERVE_JOBS_AGGREGATE_RESULT_BYTES
    );
}
