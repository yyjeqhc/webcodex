//! Bounded ordered Job update delivery, reconciliation replay and independent heartbeats.

use super::lifecycle::{runner_job_is_active, runner_job_is_terminal};
use super::retained::job_update_from_snapshot;
use super::*;
#[derive(Debug, Default)]
pub(super) struct JobUpdateDeliverySignalState {
    pub(super) generation: u64,
    pub(super) closed: bool,
}

#[derive(Debug, Default)]
pub(super) struct JobUpdateDeliverySignal {
    pub(super) state: Mutex<JobUpdateDeliverySignalState>,
    pub(super) wake: Condvar,
}

impl JobUpdateDeliverySignal {
    pub(super) fn notify(&self) {
        let mut state = lock_unpoison(&self.state);
        state.generation = state.generation.saturating_add(1);
        // Delivery and heartbeat workers both observe this generation. Wake
        // both so a heartbeat waiter can never consume the delivery worker's
        // only notification and delay a queued update.
        self.wake.notify_all();
    }

    pub(super) fn close(&self) {
        let mut state = lock_unpoison(&self.state);
        state.closed = true;
        state.generation = state.generation.saturating_add(1);
        self.wake.notify_all();
    }

    pub(super) fn generation(&self) -> u64 {
        lock_unpoison(&self.state).generation
    }

    pub(super) fn wait_for_change(&self, observed: u64, timeout: Duration) -> Option<u64> {
        let mut state = lock_unpoison(&self.state);
        if state.closed {
            return None;
        }
        if state.generation == observed {
            let (next, _) = self
                .wake
                .wait_timeout(state, timeout)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state = next;
        }
        (!state.closed).then_some(state.generation)
    }
}

#[derive(Debug, Clone)]
pub(super) struct PendingJobUpdateDelivery {
    pub(super) update_seq: u64,
    pub(super) status: String,
    pub(super) exit_code: Option<i32>,
    pub(super) duration_ms: Option<u64>,
    pub(super) error: Option<String>,
    pub(super) command_execution_state: Option<ShellCommandExecutionState>,
    pub(super) validation_progress: Option<ShellJobValidationProgress>,
    pub(super) test_count_evidence: Option<ShellJobTestCountEvidence>,
    pub(super) activity: Option<ShellJobActivity>,
    pub(super) finished: bool,
    /// Sequence-only liveness marker. Delivery must not project the current
    /// snapshot's logs/activity/result fields into this older sequence. Active
    /// validation Jobs repeat their unchanged progress cursor because the Server
    /// validation protocol requires that proof on every running validation update.
    pub(super) liveness_only: bool,
}

impl PendingJobUpdateDelivery {
    pub(super) fn from_update(update: &RunnerJobUpdateRequest) -> Self {
        Self {
            update_seq: update.update_seq.unwrap_or_default(),
            status: update.status.clone(),
            exit_code: update.exit_code,
            duration_ms: update.duration_ms,
            error: update.error.clone(),
            command_execution_state: update.command_execution_state,
            validation_progress: update.validation_progress.clone(),
            test_count_evidence: update.test_count_evidence.clone(),
            activity: update.activity,
            finished: update.finished,
            liveness_only: false,
        }
    }

    pub(super) fn heartbeat(job: &RunningJob) -> Self {
        Self {
            update_seq: job.snapshot.update_seq,
            status: job.snapshot.status.clone(),
            exit_code: None,
            duration_ms: None,
            error: None,
            command_execution_state: None,
            validation_progress: job.snapshot.validation_progress.clone(),
            test_count_evidence: None,
            activity: None,
            finished: false,
            liveness_only: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct JobUpdateDeliveryQueue {
    pub(super) required: VecDeque<PendingJobUpdateDelivery>,
    pub(super) output_only: Option<PendingJobUpdateDelivery>,
    pub(super) suspended_until_reconciliation: bool,
}

impl JobUpdateDeliveryQueue {
    pub(super) fn enqueue(&mut self, update: PendingJobUpdateDelivery, semantic: bool) -> bool {
        if self.suspended_until_reconciliation {
            return true;
        }
        if semantic {
            if self
                .output_only
                .as_ref()
                .is_some_and(|pending| pending.update_seq <= update.update_seq)
            {
                self.output_only = None;
            }
            if let Some(existing) = self
                .required
                .iter_mut()
                .find(|pending| pending.update_seq == update.update_seq)
            {
                *existing = update;
                return true;
            }
            if self.required.len() >= JOB_UPDATE_REQUIRED_PENDING_MAX {
                self.required.clear();
                self.suspended_until_reconciliation = true;
                return false;
            }
            let insert_at = self
                .required
                .iter()
                .position(|pending| pending.update_seq > update.update_seq)
                .unwrap_or(self.required.len());
            self.required.insert(insert_at, update);
        } else {
            // Update generation happens under the Job map lock, while queue
            // insertion happens later under the delivery lock. A newer semantic
            // update can therefore overtake an older heartbeat/output-only update
            // between those locks. Never retain that stale update behind newer
            // required truth: a legacy/non-sequenced Server would otherwise see
            // the semantic state and then regress when the queue drains.
            if self
                .required
                .iter()
                .any(|required| required.update_seq >= update.update_seq)
            {
                return true;
            }
            if self
                .output_only
                .as_ref()
                .is_some_and(|pending| pending.update_seq >= update.update_seq)
            {
                return true;
            }
            self.output_only = Some(update);
        }
        true
    }

    pub(super) fn next(&self) -> Option<&PendingJobUpdateDelivery> {
        if self.suspended_until_reconciliation {
            None
        } else {
            self.required.front().or(self.output_only.as_ref())
        }
    }

    pub(super) fn acknowledge(&mut self, update_seq: u64) {
        if self
            .required
            .front()
            .is_some_and(|update| update.update_seq == update_seq)
        {
            self.required.pop_front();
        } else if self
            .output_only
            .as_ref()
            .is_some_and(|update| update.update_seq == update_seq)
        {
            self.output_only = None;
        }
    }

    pub(super) fn discard_through(&mut self, update_seq: u64) {
        while self
            .required
            .front()
            .is_some_and(|update| update.update_seq <= update_seq)
        {
            self.required.pop_front();
        }
        if self
            .output_only
            .as_ref()
            .is_some_and(|update| update.update_seq <= update_seq)
        {
            self.output_only = None;
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.required.is_empty() && self.output_only.is_none()
    }
}

pub(super) fn job_update_from_delivery(
    job: &RunningJob,
    pending: &PendingJobUpdateDelivery,
) -> RunnerJobUpdateRequest {
    if pending.liveness_only {
        return RunnerJobUpdateRequest {
            client_id: job.client_id.clone(),
            runner_instance_id: job.runner_instance_id.clone(),
            job_id: job.snapshot.job_id.clone(),
            request_id: Some(job.snapshot.request_id.clone()),
            update_seq: Some(pending.update_seq),
            status: pending.status.clone(),
            stdout_chunk: None,
            stderr_chunk: None,
            log_snapshot: None,
            exit_code: None,
            duration_ms: None,
            error: None,
            command_execution_state: None,
            validation_progress: pending.validation_progress.clone(),
            test_count_evidence: None,
            activity: None,
            finished: false,
        };
    }

    let mut update =
        job_update_from_snapshot(&job.client_id, &job.runner_instance_id, &job.snapshot);
    update.update_seq = Some(pending.update_seq);
    update.status = pending.status.clone();
    update.exit_code = pending.exit_code;
    update.duration_ms = pending.duration_ms;
    update.error = pending.error.clone();
    update.command_execution_state = pending.command_execution_state;
    update.validation_progress = pending.validation_progress.clone();
    update.test_count_evidence = pending.test_count_evidence.clone();
    update.activity = pending.activity;
    update.finished = pending.finished;
    update
}

pub(super) fn spawn_job_update_delivery_worker(
    jobs: Weak<Mutex<HashMap<String, RunningJob>>>,
    current_sink: Weak<Mutex<Option<RunnerSink>>>,
    pending_job_updates: Weak<Mutex<HashMap<String, JobUpdateDeliveryQueue>>>,
    job_update_delivery_order: Weak<Mutex<()>>,
    signal: Arc<JobUpdateDeliverySignal>,
) {
    std::thread::spawn(move || {
        let mut observed_generation = signal.generation();
        loop {
            let Some(pending_map) = pending_job_updates.upgrade() else {
                break;
            };
            let Some(delivery_order) = job_update_delivery_order.upgrade() else {
                break;
            };
            let candidate = {
                // A producer holds this from sequence assignment/snapshot
                // observation through queue insertion. Selecting under the same
                // gate prevents a higher sequence from escaping while an older
                // generated update is still between the Job and delivery locks.
                let _delivery_order = lock_unpoison(&delivery_order);
                let pending = lock_unpoison(&pending_map);
                pending.iter().find_map(|(job_id, queue)| {
                    queue.next().cloned().map(|update| (job_id.clone(), update))
                })
            };
            let Some((job_id, pending_update)) = candidate else {
                let Some(next) =
                    signal.wait_for_change(observed_generation, Duration::from_secs(60))
                else {
                    break;
                };
                observed_generation = next;
                continue;
            };

            let Some(jobs_map) = jobs.upgrade() else {
                break;
            };
            let update = {
                let jobs = lock_unpoison(&jobs_map);
                jobs.get(&job_id)
                    .map(|job| job_update_from_delivery(job, &pending_update))
            };
            let Some(update) = update else {
                lock_unpoison(&pending_map).remove(&job_id);
                continue;
            };

            let Some(sink_slot) = current_sink.upgrade() else {
                break;
            };
            let sink = lock_unpoison(&sink_slot).clone();
            let Some(sink) = sink else {
                let Some(next) =
                    signal.wait_for_change(observed_generation, Duration::from_secs(60))
                else {
                    break;
                };
                observed_generation = next;
                continue;
            };

            let send_result = if matches!(
                &sink,
                RunnerSink::WebSocket { .. } | RunnerSink::Quic { .. }
            ) {
                // Stream delivery is a non-blocking try_send. Keep candidate
                // validation and enqueue atomic with respect to coalescing: a
                // semantic update may supersede an output/activity-only item
                // after the worker clones it but before channel capacity returns.
                // HTTP remains outside this lock because it performs a bounded
                // synchronous request and must never block update producers.
                let pending = lock_unpoison(&pending_map);
                let still_pending = pending
                    .get(&job_id)
                    .and_then(JobUpdateDeliveryQueue::next)
                    .is_some_and(|current| current.update_seq == pending_update.update_seq);
                if !still_pending {
                    continue;
                }
                sink.try_send_job_update(&update)
            } else {
                sink.try_send_job_update(&update)
            };

            match send_result {
                Ok(true) => {
                    let still_current = lock_unpoison(&sink_slot)
                        .as_ref()
                        .is_some_and(|current| current.same_job_update_target(&sink));
                    if still_current {
                        let mut pending = lock_unpoison(&pending_map);
                        let remove = if let Some(queue) = pending.get_mut(&job_id) {
                            queue.acknowledge(pending_update.update_seq);
                            queue.is_empty() && !queue.suspended_until_reconciliation
                        } else {
                            false
                        };
                        if remove {
                            pending.remove(&job_id);
                        }
                    }
                }
                Ok(false) | Err(_) => {
                    let Some(next) =
                        signal.wait_for_change(observed_generation, JOB_UPDATE_DELIVERY_RETRY)
                    else {
                        break;
                    };
                    observed_generation = next;
                }
            }
        }
    });
}

pub(super) fn queue_job_heartbeat_batch(
    jobs_map: &Mutex<HashMap<String, RunningJob>>,
    pending_map: &Mutex<HashMap<String, JobUpdateDeliveryQueue>>,
    delivery_order: &Mutex<()>,
    signal: &JobUpdateDeliverySignal,
) -> usize {
    let _delivery_order = lock_unpoison(delivery_order);
    let pending_heartbeats = {
        let mut jobs = lock_unpoison(jobs_map);
        jobs.values_mut()
            .filter(|job| {
                runner_job_is_active(&job.snapshot.status)
                    // Detached Jobs have a separate durable supervisor/store that
                    // owns update_seq across Runner replacement. Advancing the same
                    // sequence only in JobManager memory lets heartbeat traffic run
                    // ahead of durable state, after which the detached observer
                    // rejects the real terminal record as a sequence regression.
                    // Runner transport liveness remains independently heartbeated;
                    // detached semantic/output changes come from the durable observer.
                    && !job.snapshot.context.structured_execution.as_ref().is_some_and(
                        |metadata| metadata.execution_source == "run_detached_process",
                    )
            })
            .map(|job| {
                job.snapshot.update_seq = job.snapshot.update_seq.saturating_add(1);
                (
                    job.snapshot.job_id.clone(),
                    PendingJobUpdateDelivery::heartbeat(job),
                )
            })
            .collect::<Vec<_>>()
    };
    if pending_heartbeats.is_empty() {
        return 0;
    }

    let active_jobs = pending_heartbeats.len();
    let mut pending = lock_unpoison(pending_map);
    for (job_id, heartbeat) in pending_heartbeats {
        let _ = pending.entry(job_id).or_default().enqueue(heartbeat, false);
    }
    drop(pending);
    signal.notify();
    active_jobs
}

pub(super) fn spawn_job_heartbeat_worker(
    jobs: Weak<Mutex<HashMap<String, RunningJob>>>,
    pending_job_updates: Weak<Mutex<HashMap<String, JobUpdateDeliveryQueue>>>,
    job_update_delivery_order: Weak<Mutex<()>>,
    shutting_down: Weak<AtomicBool>,
    signal: Weak<JobUpdateDeliverySignal>,
) {
    let _ = spawn_job_heartbeat_worker_with_interval(
        jobs,
        pending_job_updates,
        job_update_delivery_order,
        shutting_down,
        signal,
        JOB_HEARTBEAT_INTERVAL,
    );
}

pub(super) fn spawn_job_heartbeat_worker_with_interval(
    jobs: Weak<Mutex<HashMap<String, RunningJob>>>,
    pending_job_updates: Weak<Mutex<HashMap<String, JobUpdateDeliveryQueue>>>,
    job_update_delivery_order: Weak<Mutex<()>>,
    shutting_down: Weak<AtomicBool>,
    signal: Weak<JobUpdateDeliverySignal>,
    interval: Duration,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let Some(signal) = signal.upgrade() else {
            return;
        };
        let mut observed_generation = signal.generation();
        let mut next_heartbeat = Instant::now() + interval;
        loop {
            let remaining = next_heartbeat.saturating_duration_since(Instant::now());
            let Some(next) = signal.wait_for_change(observed_generation, remaining) else {
                break;
            };
            observed_generation = next;
            if shutting_down
                .upgrade()
                .is_none_or(|flag| flag.load(Ordering::SeqCst))
            {
                break;
            }
            if Instant::now() < next_heartbeat {
                continue;
            }

            let Some(jobs_map) = jobs.upgrade() else {
                break;
            };
            let Some(pending_map) = pending_job_updates.upgrade() else {
                break;
            };
            let Some(delivery_order) = job_update_delivery_order.upgrade() else {
                break;
            };
            let active_jobs =
                queue_job_heartbeat_batch(&jobs_map, &pending_map, &delivery_order, &signal);
            next_heartbeat = Instant::now() + interval;
            if active_jobs > 0 {
                tracing::debug!(
                    active_jobs,
                    interval_secs = interval.as_secs(),
                    "runner job heartbeat batch queued"
                );
            }
        }
    })
}
impl JobManager {
    pub(crate) fn install_sink(&self, sink: RunnerSink) {
        *lock_unpoison(&self.current_sink) = Some(sink);
        self.delivery_signal.notify();
    }

    pub(super) fn current_sink(&self) -> Option<RunnerSink> {
        lock_unpoison(&self.current_sink).clone()
    }

    pub(super) fn queue_recorded_update(&self, update: RunnerJobUpdateRequest, semantic: bool) {
        let job_id = update.job_id.clone();
        let accepted = lock_unpoison(&self.pending_job_updates)
            .entry(job_id.clone())
            .or_default()
            .enqueue(PendingJobUpdateDelivery::from_update(&update), semantic);
        if !accepted {
            tracing::warn!(
                job_id = %job_id,
                limit = JOB_UPDATE_REQUIRED_PENDING_MAX,
                "runner job live delivery backlog exceeded its semantic bound; waiting for reconciliation"
            );
        }
        self.delivery_signal.notify();
    }

    pub(in crate::webcodex_runner) fn replay_snapshots_since(
        &self,
        registered: &ShellJobInventory,
    ) {
        if self.current_sink().is_none() {
            return;
        }
        let _delivery_order = lock_unpoison(&self.job_update_delivery_order);
        let registered_by_job = registered
            .jobs
            .iter()
            .map(|snapshot| (snapshot.job_id.as_str(), snapshot))
            .collect::<HashMap<_, _>>();
        let snapshots = self.inventory().jobs;
        let mut pending = lock_unpoison(&self.pending_job_updates);
        let mut remove = Vec::new();
        for snapshot in snapshots {
            let registered_snapshot = registered_by_job.get(snapshot.job_id.as_str()).copied();
            let registered_seq = registered_snapshot.map(|item| item.update_seq).unwrap_or(0);
            let queue = pending.entry(snapshot.job_id.clone()).or_default();
            queue.discard_through(registered_seq);

            if queue.suspended_until_reconciliation {
                if registered_seq >= snapshot.update_seq {
                    queue.suspended_until_reconciliation = false;
                } else {
                    continue;
                }
            }

            if snapshot.update_seq > registered_seq && queue.is_empty() {
                let replay_safe = if snapshot.context.validation_steps.is_empty() {
                    true
                } else {
                    let previous_completed = registered_snapshot
                        .and_then(|item| item.validation_progress.as_ref())
                        .map(|progress| progress.completed)
                        .unwrap_or(0);
                    let current_completed = snapshot
                        .validation_progress
                        .as_ref()
                        .map(|progress| progress.completed)
                        .unwrap_or(0);
                    current_completed <= previous_completed.saturating_add(1)
                };
                if replay_safe {
                    let marker = PendingJobUpdateDelivery {
                        update_seq: snapshot.update_seq,
                        status: snapshot.status.clone(),
                        exit_code: snapshot.exit_code,
                        duration_ms: snapshot.duration_ms,
                        error: snapshot.error.clone(),
                        command_execution_state: snapshot.command_execution_state,
                        validation_progress: snapshot.validation_progress.clone(),
                        test_count_evidence: snapshot.test_count_evidence.clone(),
                        activity: snapshot.activity,
                        finished: runner_job_is_terminal(&snapshot.status),
                        liveness_only: false,
                    };
                    let _ = queue.enqueue(marker, true);
                } else {
                    queue.suspended_until_reconciliation = true;
                }
            }
            if queue.is_empty() && !queue.suspended_until_reconciliation {
                remove.push(snapshot.job_id);
            }
        }
        for job_id in remove {
            pending.remove(&job_id);
        }
        drop(pending);
        self.delivery_signal.notify();
    }

    pub(super) fn resend_snapshot(&self, job_id: &str) {
        let _delivery_order = lock_unpoison(&self.job_update_delivery_order);
        let update = lock_unpoison(&self.jobs).get(job_id).map(|job| {
            job_update_from_snapshot(&job.client_id, &job.runner_instance_id, &job.snapshot)
        });
        if let Some(update) = update {
            self.queue_recorded_update(update, true);
        }
    }
}
