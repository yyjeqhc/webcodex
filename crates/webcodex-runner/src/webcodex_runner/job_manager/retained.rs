//! Authoritative retained Job snapshots, terminal immutability and bounded output tails.

use super::lifecycle::{job_prestart_lifecycle, runner_job_is_active, runner_job_is_terminal};
use super::*;
pub(super) fn job_update_from_snapshot(
    client_id: &str,
    runner_instance_id: &str,
    snapshot: &ShellJobSnapshot,
) -> RunnerJobUpdateRequest {
    RunnerJobUpdateRequest {
        client_id: client_id.to_string(),
        runner_instance_id: runner_instance_id.to_string(),
        job_id: snapshot.job_id.clone(),
        request_id: Some(snapshot.request_id.clone()),
        update_seq: Some(snapshot.update_seq),
        status: snapshot.status.clone(),
        stdout_chunk: None,
        stderr_chunk: None,
        log_snapshot: Some(ShellJobLogSnapshot {
            stdout: snapshot.stdout.clone(),
            stderr: snapshot.stderr.clone(),
        }),
        exit_code: snapshot.exit_code,
        duration_ms: snapshot.duration_ms,
        error: snapshot.error.clone(),
        command_execution_state: snapshot.command_execution_state,
        validation_progress: snapshot.validation_progress.clone(),
        test_count_evidence: snapshot.test_count_evidence.clone(),
        activity: snapshot.activity,
        finished: runner_job_is_terminal(&snapshot.status),
    }
}

pub(super) fn runner_retained_line_count(value: &str) -> usize {
    value.lines().count()
}

pub(super) fn append_runner_stream(stream: &mut ShellJobStreamSnapshot, chunk: Option<&str>) {
    let Some(chunk) = chunk else {
        return;
    };
    stream.tail.push_str(chunk);
    if stream.tail.len() > JOB_SNAPSHOT_STREAM_MAX_BYTES {
        let observed_next = stream
            .first_retained_line
            .saturating_add(runner_retained_line_count(&stream.tail));
        let mut minimum_start = stream.tail.len() - JOB_SNAPSHOT_STREAM_MAX_BYTES;
        while minimum_start < stream.tail.len() && !stream.tail.is_char_boundary(minimum_start) {
            minimum_start += 1;
        }
        if let Some(relative_newline) = stream.tail[minimum_start..].find('\n') {
            let drop_end = minimum_start + relative_newline + 1;
            let dropped_lines = stream.tail[..drop_end]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count();
            stream.tail.drain(..drop_end);
            stream.first_retained_line = stream.first_retained_line.saturating_add(dropped_lines);
        } else {
            let dropped_lines = stream.tail[..minimum_start]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count();
            stream.tail.drain(..minimum_start);
            stream.first_retained_line = stream.first_retained_line.saturating_add(dropped_lines);
        }
        if stream.tail.is_empty() {
            // The last retained partial line was dropped too. Preserve the
            // absolute next cursor by advancing the empty range to the
            // observed end rather than resetting it backwards.
            stream.first_retained_line = observed_next;
        }
        stream.truncated = true;
    }
    stream.next_line = stream
        .first_retained_line
        .saturating_add(runner_retained_line_count(&stream.tail));
}

pub(super) fn trim_runner_stream_to(stream: &mut ShellJobStreamSnapshot, max_bytes: usize) {
    if stream.tail.len() <= max_bytes {
        return;
    }
    let observed_next = stream
        .first_retained_line
        .saturating_add(runner_retained_line_count(&stream.tail));
    let mut minimum_start = stream.tail.len().saturating_sub(max_bytes);
    while minimum_start < stream.tail.len() && !stream.tail.is_char_boundary(minimum_start) {
        minimum_start += 1;
    }
    if let Some(relative_newline) = stream.tail[minimum_start..].find('\n') {
        let drop_end = minimum_start + relative_newline + 1;
        let dropped_lines = stream.tail[..drop_end]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count();
        stream.tail.drain(..drop_end);
        stream.first_retained_line = stream.first_retained_line.saturating_add(dropped_lines);
    } else {
        let dropped_lines = stream.tail[..minimum_start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count();
        stream.tail.drain(..minimum_start);
        stream.first_retained_line = stream.first_retained_line.saturating_add(dropped_lines);
    }
    if stream.tail.is_empty() {
        stream.first_retained_line = observed_next;
    }
    stream.truncated = true;
    stream.next_line = stream
        .first_retained_line
        .saturating_add(runner_retained_line_count(&stream.tail));
}

pub(super) fn bounded_runner_error(error: Option<String>) -> Option<String> {
    error.map(|error| error.chars().take(4_096).collect())
}

/// Retain enough SSH diagnostic output to classify an uncertain transport
/// failure without allowing a chatty remote command to grow worker memory.
pub(super) fn append_bounded_tail(target: &mut String, next: &str, max_bytes: usize) {
    target.push_str(next);
    if target.len() <= max_bytes {
        return;
    }
    let mut start = target.len().saturating_sub(max_bytes);
    while start < target.len() && !target.is_char_boundary(start) {
        start += 1;
    }
    target.drain(..start);
}
impl JobManager {
    pub(super) fn record_update(
        &self,
        job_id: &str,
        mut delta: RunnerJobDelta,
    ) -> Option<(RunnerJobUpdateRequest, bool)> {
        let (update, semantic) = {
            let mut jobs = lock_unpoison(&self.jobs);
            let job = jobs.get_mut(job_id)?;
            if runner_job_is_terminal(&job.snapshot.status) {
                // The first locally observed terminal outcome is immutable.
                // In particular, a racing stop request or late output poll
                // must not revive a handle-free retained record.
                return None;
            }
            let previous_status = job.snapshot.status.clone();
            let previous_progress = job.snapshot.validation_progress.clone();
            let explicit_semantic =
                delta.finished || delta.command_execution_state.is_some() || delta.error.is_some();
            let now = chrono::Utc::now().timestamp();
            append_runner_stream(&mut job.snapshot.stdout, delta.stdout_chunk.as_deref());
            append_runner_stream(&mut job.snapshot.stderr, delta.stderr_chunk.as_deref());
            if let Some(max_bytes) = delta.stream_limit_bytes {
                let max_bytes = max_bytes.min(JOB_SNAPSHOT_STREAM_MAX_BYTES);
                trim_runner_stream_to(&mut job.snapshot.stdout, max_bytes);
                trim_runner_stream_to(&mut job.snapshot.stderr, max_bytes);
            }
            job.snapshot.update_seq = job.snapshot.update_seq.saturating_add(1);
            if !delta.status.trim().is_empty() {
                let incoming_status = delta.status.trim();
                let current_lifecycle = RunnerJobLifecycle::from_wire(&job.snapshot.status).ok();
                let incoming_lifecycle = RunnerJobLifecycle::from_wire(incoming_status).ok();
                let would_regress_stop = current_lifecycle
                    == Some(RunnerJobLifecycle::StopRequested)
                    && matches!(
                        incoming_lifecycle,
                        Some(RunnerJobLifecycle::RunnerQueued | RunnerJobLifecycle::Running)
                    );
                let would_regress_running = current_lifecycle == Some(RunnerJobLifecycle::Running)
                    && incoming_lifecycle == Some(RunnerJobLifecycle::RunnerQueued);
                if !would_regress_stop && !would_regress_running {
                    job.snapshot.status = incoming_status.to_string();
                }
            }
            if delta.command_execution_state.is_some() {
                job.snapshot.command_execution_state = delta.command_execution_state;
            }
            if job.snapshot.started_at.is_none()
                && job.snapshot.command_execution_state
                    != Some(ShellCommandExecutionState::NotStarted)
                && RunnerJobLifecycle::from_wire(&job.snapshot.status).is_ok_and(|lifecycle| {
                    matches!(
                        lifecycle,
                        RunnerJobLifecycle::Running
                            | RunnerJobLifecycle::Completed
                            | RunnerJobLifecycle::Failed
                            | RunnerJobLifecycle::Stopped
                            | RunnerJobLifecycle::Timeout
                            | RunnerJobLifecycle::TimedOut
                            | RunnerJobLifecycle::Cancelled
                    )
                })
            {
                job.snapshot.started_at = Some(now);
            }
            if delta.validation_progress.is_some() {
                job.snapshot.validation_progress = delta.validation_progress.clone();
            }
            if delta.test_count_evidence.is_some() {
                job.snapshot.test_count_evidence = delta.test_count_evidence.clone();
            }
            if let Some(activity) = delta.activity {
                debug_assert!(activity.is_canonical());
                job.snapshot.activity = Some(activity);
            }
            if runner_job_is_terminal(&job.snapshot.status) || delta.finished {
                if let Some(input) = &job.input {
                    input.finish();
                }
                job.snapshot.activity = None;
                job.snapshot.ended_at.get_or_insert(now);
                job.snapshot.exit_code = delta.exit_code;
                job.snapshot.duration_ms = delta.duration_ms;
                job.snapshot.error = bounded_runner_error(delta.error.take());
                job.child = None;
                job.slot_reserved = false;
            } else if delta.error.is_some() {
                job.snapshot.error = bounded_runner_error(delta.error.take());
            }
            let semantic = explicit_semantic
                || job.snapshot.status != previous_status
                || job.snapshot.validation_progress != previous_progress;
            // Each sequenced update carries the current authoritative bounded
            // tails and activity. Delivery may coalesce output/activity-only
            // attempts, but required semantic markers preserve their sequence
            // while using the latest retained authoritative snapshot at send
            // time.
            (
                job_update_from_snapshot(&job.client_id, &job.runner_instance_id, &job.snapshot),
                semantic,
            )
        };
        self.prune_terminal_records();
        Some((update, semantic))
    }

    pub(super) fn update_and_send(&self, job_id: &str, delta: RunnerJobDelta) {
        let _delivery_order = lock_unpoison(&self.job_update_delivery_order);
        if let Some((update, semantic)) = self.record_update(job_id, delta) {
            self.queue_recorded_update(update, semantic);
        }
    }

    pub(super) fn fail_job(
        &self,
        operation: &RunnerJobOperation,
        error: String,
        validation_progress: Option<ShellJobValidationProgress>,
    ) {
        let job_id = operation.job_id();
        self.update_and_send(
            job_id,
            RunnerJobDelta {
                status: "failed".to_string(),
                duration_ms: Some(0),
                error: Some(error),
                command_execution_state: job_prestart_lifecycle(operation),
                validation_progress,
                finished: true,
                ..Default::default()
            },
        );
        self.start_available_queued();
    }

    pub(super) fn prune_terminal_records(&self) {
        let now = chrono::Utc::now().timestamp();
        let removed = {
            let mut jobs = lock_unpoison(&self.jobs);
            let mut removed = jobs
                .iter()
                .filter(|(_, job)| {
                    runner_job_is_terminal(&job.snapshot.status)
                        && job.snapshot.ended_at.is_some_and(|ended| {
                            now.saturating_sub(ended) >= JOB_TERMINAL_RETENTION_SECS
                        })
                })
                .map(|(job_id, _)| job_id.clone())
                .collect::<Vec<_>>();
            for job_id in &removed {
                jobs.remove(job_id);
            }
            let mut terminal = jobs
                .iter()
                .filter(|(_, job)| runner_job_is_terminal(&job.snapshot.status))
                .map(|(job_id, job)| {
                    (
                        job_id.clone(),
                        job.snapshot.ended_at.unwrap_or(job.snapshot.created_at),
                    )
                })
                .collect::<Vec<_>>();
            terminal.sort_by_key(|(_, ended_at)| *ended_at);
            let excess = terminal
                .len()
                .saturating_sub(JOB_INVENTORY_MAX_TERMINAL_JOBS);
            for (job_id, _) in terminal.into_iter().take(excess) {
                jobs.remove(&job_id);
                removed.push(job_id);
            }
            removed
        };
        if !removed.is_empty() {
            let mut pending = lock_unpoison(&self.pending_job_updates);
            let mut detached = lock_unpoison(&self.detached_jobs);
            for job_id in removed {
                pending.remove(&job_id);
                detached.remove(&job_id);
            }
        }
    }

    pub(crate) fn inventory(&self) -> ShellJobInventory {
        self.prune_terminal_records();
        let jobs = lock_unpoison(&self.jobs);
        let mut active = jobs
            .values()
            .filter(|job| runner_job_is_active(&job.snapshot.status))
            .map(|job| job.snapshot.clone())
            .collect::<Vec<_>>();
        let mut terminal = jobs
            .values()
            .filter(|job| runner_job_is_terminal(&job.snapshot.status))
            .map(|job| job.snapshot.clone())
            .collect::<Vec<_>>();
        drop(jobs);
        active.sort_by_key(|snapshot| snapshot.created_at);
        terminal.sort_by(|left, right| {
            right
                .ended_at
                .unwrap_or(right.created_at)
                .cmp(&left.ended_at.unwrap_or(left.created_at))
        });
        terminal.truncate(JOB_INVENTORY_MAX_TERMINAL_JOBS);
        let mut inventory = ShellJobInventory {
            active_complete: true,
            jobs: active,
        };

        // Active records are never omitted. Only when active records alone
        // exceed the frame budget do their authoritative tails shrink.
        let mut tail_limit = JOB_SNAPSHOT_STREAM_MAX_BYTES;
        while serde_json::to_vec(&inventory)
            .map(|bytes| bytes.len() > JOB_INVENTORY_MAX_SERIALIZED_BYTES)
            .unwrap_or(true)
            && tail_limit > 0
        {
            tail_limit /= 2;
            for snapshot in &mut inventory.jobs {
                trim_runner_stream_to(&mut snapshot.stdout, tail_limit);
                trim_runner_stream_to(&mut snapshot.stderr, tail_limit);
            }
        }

        // Add newest terminal history only while it fits. Serializing each
        // record once avoids repeatedly encoding a multi-megabyte inventory
        // while preserving the newest-first eviction rule.
        let mut serialized_len = serde_json::to_vec(&inventory)
            .map(|bytes| bytes.len())
            .unwrap_or(JOB_INVENTORY_MAX_SERIALIZED_BYTES.saturating_add(1));
        for snapshot in terminal {
            let Ok(encoded) = serde_json::to_vec(&snapshot) else {
                continue;
            };
            let separator = usize::from(!inventory.jobs.is_empty());
            let added = encoded.len().saturating_add(separator);
            if serialized_len.saturating_add(added) > JOB_INVENTORY_MAX_SERIALIZED_BYTES {
                break;
            }
            serialized_len = serialized_len.saturating_add(added);
            inventory.jobs.push(snapshot);
        }
        inventory
    }
}
