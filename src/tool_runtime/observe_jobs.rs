//! Bounded multi-Job observation composed from the canonical single-Job path.

use super::{ObserveJobsItem, ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use futures_util::{stream, StreamExt};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::time::Duration;
use tokio::time::Instant;
use webcodex_workspace::file_read_normalize::MODEL_RESULT_ENVELOPE_RESERVE_BYTES;
use webcodex_workspace::file_read_range::MAX_SERIALIZED_OUTPUT_BYTES;

pub(crate) const MAX_OBSERVE_JOBS_ITEMS: usize = 8;
pub(crate) const DEFAULT_OBSERVE_JOBS_TAIL_LINES: usize = 40;
pub(crate) const MAX_OBSERVE_JOBS_TAIL_LINES: usize = 200;
const MAX_OBSERVE_JOBS_ERROR_CHARS: usize = 512;

#[derive(Debug)]
struct ObservedJob {
    index: usize,
    job_id: String,
    result: ToolResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WakeReason {
    Immediate,
    Updated,
    Terminal,
    ItemError,
    Timeout,
}

impl WakeReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::Immediate => "immediate",
            Self::Updated => "updated",
            Self::Terminal => "terminal",
            Self::ItemError => "item_error",
            Self::Timeout => "timeout",
        }
    }
}

fn observed_has_error(observed: &[ObservedJob]) -> bool {
    observed.iter().any(|item| !item.result.success)
}

fn observed_has_terminal(observed: &[ObservedJob]) -> bool {
    observed
        .iter()
        .any(|item| item.result.output["terminal"].as_bool() == Some(true))
}

fn observed_has_change(observed: &[ObservedJob]) -> bool {
    observed
        .iter()
        .any(|item| item.result.output["changed"].as_bool() == Some(true))
}

fn bounded_error(error: Option<&str>) -> String {
    let error = error.unwrap_or("Job observation failed");
    let mut chars = error.chars();
    let bounded = chars
        .by_ref()
        .take(MAX_OBSERVE_JOBS_ERROR_CHARS)
        .collect::<String>();
    if chars.next().is_some() {
        format!("{bounded}…")
    } else {
        bounded
    }
}

fn observation_error_kind(result: &ToolResult) -> &'static str {
    let error = result.error.as_deref().unwrap_or_default();
    if error.contains("after_observation_token") {
        "invalid_observation_token"
    } else if error.starts_with("unknown job:") {
        "unknown_job"
    } else if error.contains("local_job_observation") {
        "observation_failed"
    } else {
        "job_observation_failed"
    }
}

fn batch_item(observed: ObservedJob) -> Value {
    if observed.result.success {
        json!({
            "index": observed.index,
            "job_id": observed.job_id,
            "success": true,
            "output": observed.result.output,
            "error_kind": null,
            "error": null,
        })
    } else {
        json!({
            "index": observed.index,
            "job_id": observed.job_id,
            "success": false,
            "output": null,
            "error_kind": observation_error_kind(&observed.result),
            "error": bounded_error(observed.result.error.as_deref()),
        })
    }
}

fn output_budget_failure_item(index: usize, job_id: String) -> Value {
    json!({
        "index": index,
        "job_id": job_id,
        "success": false,
        "output": null,
        "error_kind": "output_budget_exceeded",
        "error": "The bounded Job observation cannot fit in one model result; resubmit this Job with a smaller tail_lines value.",
    })
}

fn batch_output(
    requested_count: usize,
    items: Vec<Value>,
    wake_reason: WakeReason,
    waited_ms: u64,
    output_truncated: bool,
    next_index: Option<usize>,
) -> Value {
    let succeeded_count = items
        .iter()
        .filter(|item| item["success"].as_bool() == Some(true))
        .count();
    let returned_count = items.len();
    let changed_count = items
        .iter()
        .filter(|item| {
            item["success"].as_bool() == Some(true)
                && item["output"]["changed"].as_bool() == Some(true)
        })
        .count();
    let terminal_count = items
        .iter()
        .filter(|item| {
            item["success"].as_bool() == Some(true)
                && item["output"]["terminal"].as_bool() == Some(true)
        })
        .count();
    json!({
        "requested_count": requested_count,
        "returned_count": returned_count,
        "succeeded_count": succeeded_count,
        "failed_count": returned_count - succeeded_count,
        "items": items,
        "wake_reason": wake_reason.as_str(),
        "waited_ms": waited_ms,
        "changed_count": changed_count,
        "terminal_count": terminal_count,
        "output_truncated": output_truncated,
        "next_index": next_index,
    })
}

fn serialized_batch_fits(output: &Value) -> bool {
    serde_json::to_vec(&ToolResult::ok(output.clone()))
        .map(|bytes| {
            bytes.len()
                <= MAX_SERIALIZED_OUTPUT_BYTES.saturating_sub(MODEL_RESULT_ENVELOPE_RESERVE_BYTES)
        })
        .unwrap_or(false)
}

fn apply_output_budget(
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
        if serialized_batch_fits(&candidate) {
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
        if serialized_batch_fits(&single) {
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
        if !serialized_batch_fits(&candidate) {
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

impl ToolRuntime {
    fn validate_observe_jobs_input(
        items: &[ObserveJobsItem],
        tail_lines: usize,
        wait_secs: Option<u64>,
    ) -> Result<(), String> {
        if !(1..=MAX_OBSERVE_JOBS_ITEMS).contains(&items.len()) {
            return Err("observe_jobs requires between 1 and 8 items".into());
        }
        if items.iter().any(|item| item.job_id.trim().is_empty()) {
            return Err("observe_jobs requires every item to have a non-empty job_id".into());
        }
        if let Some(item) = items.iter().find(|item| {
            item.after_observation_token.as_ref().is_some_and(|token| {
                token.len() > crate::job_observation::MAX_JOB_OBSERVATION_TOKEN_LEN
            })
        }) {
            return Err(format!(
                "observe_jobs token for job_id {} exceeds 192 bytes",
                item.job_id
            ));
        }
        if !(1..=MAX_OBSERVE_JOBS_TAIL_LINES).contains(&tail_lines) {
            return Err("observe_jobs tail_lines must be between 1 and 200".into());
        }
        if wait_secs.is_some_and(|wait_secs| !(1..=60).contains(&wait_secs)) {
            return Err("observe_jobs wait_secs must be between 1 and 60".into());
        }
        let mut seen = HashSet::with_capacity(items.len());
        if let Some(duplicate) = items
            .iter()
            .map(|item| item.job_id.as_str())
            .find(|job_id| !seen.insert(*job_id))
        {
            return Err(format!(
                "observe_jobs rejects duplicate job_id values: {duplicate}"
            ));
        }
        Ok(())
    }

    async fn observe_jobs_pass(
        &self,
        items: &[ObserveJobsItem],
        tail_lines: usize,
        auth: Option<&AuthContext>,
    ) -> Vec<ObservedJob> {
        let mut observed: Vec<ObservedJob> = stream::iter(items.iter().cloned().enumerate().map(
            |(index, item)| async move {
                let result = self
                    .job_log_for_auth(
                        item.job_id.clone(),
                        None,
                        Some(tail_lines),
                        auth,
                        item.after_observation_token,
                        None,
                    )
                    .await;
                ObservedJob {
                    index,
                    job_id: item.job_id,
                    result,
                }
            },
        ))
        .buffer_unordered(MAX_OBSERVE_JOBS_ITEMS)
        .collect()
        .await;
        observed.sort_by_key(|item| item.index);
        observed
    }

    async fn wait_for_any_observed_job(
        &self,
        items: &[ObserveJobsItem],
        auth: Option<&AuthContext>,
        wait_secs: u64,
    ) -> Result<WakeReason, String> {
        let deadline = Instant::now() + Duration::from_secs(wait_secs);
        loop {
            let mut waits = stream::iter(items.iter().cloned().enumerate().map(
                |(index, item)| async move {
                    let result = self
                        .job_log_for_auth(
                            item.job_id.clone(),
                            None,
                            Some(1),
                            auth,
                            item.after_observation_token,
                            Some(wait_secs),
                        )
                        .await;
                    ObservedJob {
                        index,
                        job_id: item.job_id,
                        result,
                    }
                },
            ))
            .buffer_unordered(MAX_OBSERVE_JOBS_ITEMS);
            let heartbeat = (Instant::now() + Duration::from_millis(200)).min(deadline);
            tokio::select! {
                first = waits.next() => {
                    let first = first.ok_or_else(|| {
                        "observe_jobs shared wait had no item futures".to_string()
                    })?;
                    if !first.result.success {
                        return Ok(WakeReason::ItemError);
                    }
                    if first.result.output["terminal"].as_bool() == Some(true) {
                        return Ok(WakeReason::Terminal);
                    }
                    if first.result.output["changed"].as_bool() == Some(true) {
                        return Ok(WakeReason::Updated);
                    }
                    match first.result.output["wait_outcome"].as_str() {
                        Some("terminal") => return Ok(WakeReason::Terminal),
                        Some("updated" | "immediate") => return Ok(WakeReason::Updated),
                        Some("timeout") if Instant::now() >= deadline => {
                            return Ok(WakeReason::Timeout);
                        }
                        Some("timeout") => {}
                        _ => {
                            return Err(
                                "observe_jobs canonical wait returned an invalid wait outcome"
                                    .into(),
                            );
                        }
                    }
                }
                _ = tokio::time::sleep_until(heartbeat) => {}
            }
            drop(waits);

            // Agent notifications are an optimization, not a second source of
            // truth. Re-enter the canonical immediate path on one shared
            // heartbeat so a notification race cannot defer a visible token
            // change until the full deadline. These one-line snapshots are
            // discarded; the caller performs the final requested-tail refresh.
            let heartbeat_observation = self.observe_jobs_pass(items, 1, auth).await;
            if observed_has_error(&heartbeat_observation) {
                return Ok(WakeReason::ItemError);
            }
            if observed_has_terminal(&heartbeat_observation) {
                return Ok(WakeReason::Terminal);
            }
            if observed_has_change(&heartbeat_observation) {
                return Ok(WakeReason::Updated);
            }
            if Instant::now() >= deadline {
                return Ok(WakeReason::Timeout);
            }
        }
    }

    pub(crate) async fn observe_jobs_for_auth(
        &self,
        items: Vec<ObserveJobsItem>,
        tail_lines: usize,
        wait_secs: Option<u64>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if let Err(error) = Self::validate_observe_jobs_input(&items, tail_lines, wait_secs) {
            return ToolResult::err(error);
        }

        let requested_count = items.len();
        let initial = self.observe_jobs_pass(&items, tail_lines, auth).await;
        let missing_baseline = items
            .iter()
            .any(|item| item.after_observation_token.is_none());
        let immediate_reason = if wait_secs.is_none() || missing_baseline {
            Some(WakeReason::Immediate)
        } else if observed_has_error(&initial) {
            Some(WakeReason::ItemError)
        } else if observed_has_terminal(&initial) {
            Some(WakeReason::Terminal)
        } else if observed_has_change(&initial) {
            Some(WakeReason::Updated)
        } else {
            None
        };

        let (observed, wake_reason, waited_ms) = if let Some(reason) = immediate_reason {
            (initial, reason, 0)
        } else {
            let wait_secs = wait_secs.expect("shared wait requires validated wait_secs");
            let wait_started = Instant::now();
            let wait_reason = match self
                .wait_for_any_observed_job(&items, auth, wait_secs)
                .await
            {
                Ok(reason) => reason,
                Err(error) => return ToolResult::err(error),
            };
            let waited_ms = wait_started.elapsed().as_millis() as u64;
            let refreshed = self.observe_jobs_pass(&items, tail_lines, auth).await;
            let final_reason =
                if observed_has_error(&refreshed) || wait_reason == WakeReason::ItemError {
                    WakeReason::ItemError
                } else if observed_has_terminal(&refreshed) || wait_reason == WakeReason::Terminal {
                    WakeReason::Terminal
                } else if observed_has_change(&refreshed) || wait_reason == WakeReason::Updated {
                    WakeReason::Updated
                } else {
                    WakeReason::Timeout
                };
            (refreshed, final_reason, waited_ms)
        };

        let completed = observed.into_iter().map(batch_item).collect();
        match apply_output_budget(requested_count, completed, wake_reason, waited_ms) {
            Ok(output) => ToolResult::ok(output),
            Err(error) => ToolResult::err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_error_limits_character_count() {
        let error = "雪".repeat(MAX_OBSERVE_JOBS_ERROR_CHARS + 10);
        let bounded = bounded_error(Some(&error));
        assert_eq!(bounded.chars().count(), MAX_OBSERVE_JOBS_ERROR_CHARS + 1);
        assert!(bounded.ends_with('…'));
    }

    #[test]
    fn output_budget_replaces_one_oversized_item_without_partial_json() {
        let item = json!({
            "index": 0,
            "job_id": "job-0",
            "success": true,
            "output": {
                "changed": false,
                "terminal": false,
                "stdout_tail": "x".repeat(MAX_SERIALIZED_OUTPUT_BYTES),
            },
            "error_kind": null,
            "error": null,
        });
        let output = apply_output_budget(1, vec![item], WakeReason::Immediate, 0).unwrap();
        assert_eq!(output["returned_count"], 1);
        assert_eq!(output["items"][0]["success"], false);
        assert_eq!(output["items"][0]["error_kind"], "output_budget_exceeded");
        assert_eq!(output["output_truncated"], false);
        assert!(
            serde_json::to_vec(&ToolResult::ok(output)).unwrap().len()
                <= MAX_SERIALIZED_OUTPUT_BYTES
        );
    }

    #[test]
    fn output_budget_keeps_whole_prefix_and_points_at_first_omitted_index() {
        let item = |index| {
            json!({
                "index": index,
                "job_id": format!("job-{index}"),
                "success": true,
                "output": {
                    "changed": false,
                    "terminal": false,
                    "stdout_tail": "x".repeat(90_000),
                },
                "error_kind": null,
                "error": null,
            })
        };
        let output = apply_output_budget(
            4,
            vec![item(0), item(1), item(2), item(3)],
            WakeReason::Immediate,
            0,
        )
        .unwrap();
        assert_eq!(output["returned_count"], 2);
        assert_eq!(output["output_truncated"], true);
        assert_eq!(output["next_index"], 2);
        assert!(
            serde_json::to_vec(&ToolResult::ok(output)).unwrap().len()
                <= MAX_SERIALIZED_OUTPUT_BYTES
        );
    }
}
